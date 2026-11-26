//! The game's own log file: Windows has no console, so what the game prints would be lost.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use crate::report::LOG_MAX;

pub const NAME: &str = "game.log";
pub const OLD_NAME: &str = "game.old.log";
pub const CAP: u64 = 1_000_000;
const PLAY_LINES: usize = 200;

static OPEN: Mutex<Option<(PathBuf, Instant)>> = Mutex::new(None);

/// Begin a new log in `dir`; the one of the run before is kept beside it.
pub fn start(dir: &Path) {
    let path = dir.join(NAME);
    let _ = fs::create_dir_all(dir);
    let _ = fs::rename(&path, dir.join(OLD_NAME));
    let _ = fs::write(&path, "");
    if let Ok(mut open) = OPEN.lock() {
        *open = Some((path, Instant::now()));
    }
}

/// Print `msg` and, once `start` has run, add it to the log with the seconds since then. Best effort.
pub fn line(msg: &str) {
    eprintln!("{msg}");
    // a panic while the lock was held must not silence the crash message
    let open = OPEN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((path, since)) = open.as_ref() {
        append(path, &format!("[+{:.1}s] {msg}\n", since.elapsed().as_secs_f32()));
    }
}

fn append(path: &Path, text: &str) {
    if fs::metadata(path).is_ok_and(|m| m.len() + text.len() as u64 > CAP) {
        let kept = tail(path, CAP as usize / 2);
        let from = kept.find('\n').map_or(0, |at| at + 1);
        let _ = fs::write(path, &kept[from..]);
    }
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(text.as_bytes());
    }
}

/// The last `max` bytes of `text`, starting on a whole character.
fn end_of(text: &str, max: usize) -> &str {
    let mut from = text.len().saturating_sub(max);
    while !text.is_char_boundary(from) {
        from += 1;
    }
    &text[from..]
}

/// The end of a text file, at most `max` bytes, without NUL characters; empty when it cannot be read.
pub fn tail(path: &Path, max: usize) -> String {
    let bytes = fs::read(path).unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes).replace('\0', "");
    end_of(&text, max).to_string()
}

/// The log of a report: the game's log, then what the player did last.
pub fn compose(game: &str, play: &str) -> Vec<u8> {
    const GAME: &str = "== game.log ==\n";
    const PLAY: &str = "== playlog.txt ==\n";
    let lines: Vec<&str> = play.lines().collect();
    let recent = lines[lines.len().saturating_sub(PLAY_LINES)..].join("\n");
    let recent = end_of(&recent, LOG_MAX / 2);
    let room = LOG_MAX - GAME.len() - PLAY.len() - recent.len() - 1;
    format!("{GAME}{}\n{PLAY}{recent}", end_of(game, room)).into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::LOG_MAX;
    use std::fs;

    #[test]
    fn starting_keeps_exactly_one_old_log() {
        let dir = crate::temp("rotate");
        for run in ["bir", "iki", "üç"] {
            start(&dir);
            line(run);
        }
        let now = fs::read_to_string(dir.join(NAME)).unwrap();
        let old = fs::read_to_string(dir.join(OLD_NAME)).unwrap();
        assert!(now.contains("üç") && !now.contains("iki"), "{now}");
        assert!(old.contains("iki") && !old.contains("bir"), "{old}");
        assert!(now.starts_with("[+"), "{now}");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
    }

    #[test]
    fn the_file_stays_under_the_cap_and_keeps_the_newest() {
        let dir = crate::temp("cap");
        let path = dir.join(NAME);
        for n in 0..2100 {
            append(&path, &format!("{n} {}\n", "ğ".repeat(500)));
        }
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.len() as u64 <= CAP);
        assert!(text.contains("\n2099 "));
        // the cut falls between lines
        assert!(text.lines().next().unwrap().split(' ').next().unwrap().parse::<u32>().is_ok());
    }

    #[test]
    fn the_tail_is_whole_characters_without_nul() {
        let dir = crate::temp("tail");
        let path = dir.join("x.txt");
        fs::write(&path, "abc\0ğğğ").unwrap();
        assert_eq!(tail(&path, 100), "abcğğğ");
        assert_eq!(tail(&path, 5), "ğğ");
        assert_eq!(tail(&dir.join("none"), 5), "");
    }

    #[test]
    fn the_report_log_holds_both_and_fits() {
        let play: String = (0..300).map(|n| format!("olay{n}\n")).collect();
        let both = String::from_utf8(compose("oyun açıldı\n", &play)).unwrap();
        assert!(both.find("oyun açıldı").unwrap() < both.find("olay299").unwrap());
        assert!(both.contains("olay100\n") && !both.contains("olay99\n"), "last 200 lines");
        let huge = format!("BAŞ\n{}SON\n", "ğ".repeat(LOG_MAX));
        let cut = compose(&huge, &play);
        assert!(cut.len() <= LOG_MAX);
        let cut = String::from_utf8(cut).unwrap();
        assert!(cut.contains("SON") && !cut.contains("BAŞ") && cut.contains("olay299"));
        let long_lines = compose("", &"y".repeat(LOG_MAX * 2));
        assert!(long_lines.len() <= LOG_MAX);
    }
}
