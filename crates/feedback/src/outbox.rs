//! Reports waiting on disk, so none is lost to a missing network. One folder per report:
//! `report.json` and a file for each part.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use crate::report::{Part, Report};

pub const WAITING_MAX: usize = 10;
const REFUSED_MAX: usize = 10;
const REFUSED: &str = "refused";
const JSON: &str = "report.json";
const PARTIAL: &str = ".tmp-";
const STALE: Duration = Duration::from_secs(3600);

static FLUSH: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Sent,
    Later,
    Refused,
}

#[derive(Debug)]
pub enum PutError {
    Full,
    Io(io::Error),
}

impl From<io::Error> for PutError {
    fn from(e: io::Error) -> Self {
        PutError::Io(e)
    }
}

/// The report folders, oldest first.
fn reports(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<(SystemTime, PathBuf)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            e.path().is_dir() && !name.starts_with(PARTIAL) && name != REFUSED
        })
        .map(|e| {
            let path = e.path();
            let at = fs::metadata(path.join(JSON)).or_else(|_| fs::metadata(&path)).and_then(|m| m.modified());
            (at.unwrap_or(SystemTime::UNIX_EPOCH), path)
        })
        .collect();
    found.sort();
    found.into_iter().map(|(_, path)| path).collect()
}

pub fn waiting(dir: &Path) -> usize {
    reports(dir).len()
}

fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    // on disk before the folder takes its real name
    file.sync_all()
}

/// Store a report. The folder gets its real name only when every file in it is complete.
pub fn put(dir: &Path, report: &Report) -> Result<(), PutError> {
    if waiting(dir) >= WAITING_MAX {
        return Err(PutError::Full);
    }
    let partial = dir.join(format!("{PARTIAL}{}", report.fields.id));
    fs::create_dir_all(&partial)?;
    for (part, bytes) in &report.files {
        write(&partial.join(part.name()), bytes)?;
    }
    write(&partial.join(JSON), report.fields.json().as_bytes())?;
    fs::rename(&partial, dir.join(&report.fields.id))?;
    Ok(())
}

fn load(folder: &Path) -> Option<Report> {
    let fields = serde_json::from_slice(&fs::read(folder.join(JSON)).ok()?).ok()?;
    let mut files = Vec::new();
    for part in Part::ALL {
        match fs::read(folder.join(part.name())) {
            Ok(bytes) => files.push((part, bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return None,
        }
    }
    Some(Report { fields, files })
}

/// Keep a report the server will not take where the player can still find it.
fn set_aside(dir: &Path, folder: &Path) {
    let aside = dir.join(REFUSED);
    let Some(name) = folder.file_name() else { return };
    let moved = fs::create_dir_all(&aside).and_then(|()| fs::rename(folder, aside.join(name)));
    if moved.is_err() {
        // it must not be tried again and again
        let _ = fs::remove_dir_all(folder);
    }
    let old = reports(&aside);
    for extra in &old[..old.len().saturating_sub(REFUSED_MAX)] {
        let _ = fs::remove_dir_all(extra);
    }
}

fn drop_stale_partials(dir: &Path) {
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let stale = entry.metadata().and_then(|m| m.modified()).is_ok_and(|at| at.elapsed().is_ok_and(|age| age > STALE));
        if stale && entry.file_name().to_string_lossy().starts_with(PARTIAL) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// Send what waits, oldest first, until one has to wait longer. One flush at a time, so nothing is sent twice.
pub fn flush(dir: &Path, send: &mut dyn FnMut(&Report) -> Outcome) -> Vec<(String, Outcome)> {
    let _one = FLUSH.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    drop_stale_partials(dir);
    let mut done = Vec::new();
    for folder in reports(dir) {
        let id = folder.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let Some(report) = load(&folder) else {
            set_aside(dir, &folder);
            continue;
        };
        let outcome = send(&report);
        done.push((id, outcome));
        match outcome {
            Outcome::Sent => {
                let _ = fs::remove_dir_all(&folder);
            }
            Outcome::Refused => set_aside(dir, &folder),
            Outcome::Later => break,
        }
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{tests::draft, Draft, Part};
    use std::fs;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    fn report(n: u32) -> Report {
        Draft { text: format!("rapor {n}"), screenshot: Some(vec![1, 2, 3]), save: Some(vec![4; 100]), ..draft() }.build(format!("{n:032x}")).unwrap()
    }

    /// Reports are sent oldest first: keep their times apart.
    fn add(dir: &Path, n: u32) {
        put(dir, &report(n)).unwrap();
        std::thread::sleep(Duration::from_millis(20));
    }

    #[test]
    fn what_is_put_is_what_is_sent_then_it_is_gone() {
        let dir = crate::temp("roundtrip").join("reports");
        add(&dir, 1);
        assert_eq!(waiting(&dir), 1);
        let mut seen = Vec::new();
        let done = flush(&dir, &mut |r| {
            seen.push(r.clone());
            Outcome::Sent
        });
        assert_eq!(done, vec![(format!("{:032x}", 1), Outcome::Sent)]);
        assert_eq!(seen, vec![report(1)]);
        assert_eq!(seen[0].files.iter().map(|f| f.0).collect::<Vec<_>>(), vec![Part::Screenshot, Part::Save]);
        assert_eq!(waiting(&dir), 0);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
    }

    #[test]
    fn the_eleventh_report_does_not_push_one_out() {
        let dir = crate::temp("full").join("reports");
        for n in 0..10 {
            put(&dir, &report(n)).unwrap();
        }
        assert!(matches!(put(&dir, &report(10)), Err(PutError::Full)));
        assert_eq!(waiting(&dir), WAITING_MAX);
        assert!(dir.join(format!("{:032x}", 0)).exists() && !dir.join(format!("{:032x}", 10)).exists());
    }

    #[test]
    fn a_half_written_report_is_neither_counted_nor_sent() {
        let dir = crate::temp("half").join("reports");
        let half = dir.join(".tmp-abc");
        fs::create_dir_all(&half).unwrap();
        fs::write(half.join("report.json"), report(1).fields.json()).unwrap();
        assert_eq!(waiting(&dir), 0);
        assert!(flush(&dir, &mut |_| panic!("sent a half report")).is_empty());
        assert!(half.exists(), "it may still be being written");
    }

    #[test]
    fn later_keeps_this_report_and_the_newer_ones() {
        let dir = crate::temp("later").join("reports");
        for n in 1..=3 {
            add(&dir, n);
        }
        let mut tried = Vec::new();
        flush(&dir, &mut |r| {
            tried.push(r.fields.text.clone());
            if tried.len() == 1 { Outcome::Sent } else { Outcome::Later }
        });
        assert_eq!(tried, ["rapor 1", "rapor 2"]);
        assert_eq!(waiting(&dir), 2);
    }

    #[test]
    fn a_refused_report_moves_aside_and_the_next_one_goes() {
        let dir = crate::temp("refused").join("reports");
        add(&dir, 1);
        add(&dir, 2);
        let done = flush(&dir, &mut |r| if r.fields.text == "rapor 1" { Outcome::Refused } else { Outcome::Sent });
        assert_eq!(done.iter().map(|d| d.1).collect::<Vec<_>>(), [Outcome::Refused, Outcome::Sent]);
        assert_eq!(waiting(&dir), 0);
        assert!(dir.join("refused").join(format!("{:032x}", 1)).join("report.json").exists());
    }

    #[test]
    fn only_the_newest_refused_reports_are_kept() {
        let dir = crate::temp("refused-cap").join("reports");
        for n in 0..13 {
            add(&dir, n);
            flush(&dir, &mut |_| Outcome::Refused);
        }
        assert_eq!(fs::read_dir(dir.join("refused")).unwrap().count(), 10);
        assert!(dir.join("refused").join(format!("{:032x}", 12)).exists());
    }

    #[test]
    fn a_broken_report_moves_aside() {
        let dir = crate::temp("broken").join("reports");
        fs::create_dir_all(dir.join("deadbeef")).unwrap();
        fs::write(dir.join("deadbeef/report.json"), "{").unwrap();
        add(&dir, 2);
        let mut sent = 0;
        flush(&dir, &mut |_| {
            sent += 1;
            Outcome::Sent
        });
        assert_eq!((sent, waiting(&dir)), (1, 0));
        assert!(dir.join("refused/deadbeef").exists());
    }

    #[test]
    fn two_flushes_at_once_send_each_report_once() {
        let dir = crate::temp("twice").join("reports");
        for n in 0..5 {
            add(&dir, n);
        }
        let sent = Arc::new(Mutex::new(Vec::new()));
        let threads: Vec<_> = (0..2)
            .map(|_| {
                let (dir, sent) = (dir.clone(), sent.clone());
                std::thread::spawn(move || {
                    flush(&dir, &mut |r| {
                        std::thread::sleep(Duration::from_millis(5));
                        sent.lock().unwrap().push(r.fields.id.clone());
                        Outcome::Sent
                    });
                })
            })
            .collect();
        threads.into_iter().for_each(|t| t.join().unwrap());
        let mut ids = sent.lock().unwrap().clone();
        ids.sort();
        ids.dedup();
        assert_eq!((ids.len(), sent.lock().unwrap().len()), (5, 5));
    }
}
