//! Remembers that the game panicked, so the next start can offer to report it.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const NAME: &str = "crash.txt";
const SEEN: &str = "crash.last.txt";

/// Folders this run wrote a crash file into.
static WRITTEN: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

#[derive(Debug, PartialEq)]
pub struct Crash {
    /// The save slot in use at the time.
    pub slot: u8,
    pub text: String,
}

pub fn write(dir: &Path, slot: u8, text: &str) {
    let _ = fs::create_dir_all(dir);
    let _ = fs::write(dir.join(NAME), format!("slot={slot}\n{text}"));
}

/// The crash waiting to be told, once.
pub fn take(dir: &Path) -> Option<Crash> {
    let raw = fs::read(dir.join(NAME)).ok()?;
    let _ = fs::rename(dir.join(NAME), dir.join(SEEN));
    // a file that cannot be moved must not ask at every start
    let _ = fs::remove_file(dir.join(NAME));
    let raw = String::from_utf8_lossy(&raw);
    let (slot, text) = raw
        .split_once('\n')
        .and_then(|(first, rest)| Some((first.strip_prefix("slot=")?.parse().ok()?, rest)))
        .unwrap_or((1, &raw));
    Some(Crash { slot, text: text.to_string() })
}

/// Write a crash file on every panic, then do what was done before.
pub fn install(dir: PathBuf, slot: fn() -> u8) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        write(&dir, slot(), &format!("{info}\n\n{}", std::backtrace::Backtrace::force_capture()));
        if let Ok(mut written) = WRITTEN.try_lock() {
            written.push(dir.clone());
        }
        crate::log::line(&format!("çökme: {info}"));
        previous(info);
    }));
}

/// The game is closing as it should: a panic it survived is not a crash to report.
pub fn forget_own(dir: &Path) {
    if WRITTEN.lock().is_ok_and(|written| written.iter().any(|d| d == dir)) {
        let _ = fs::remove_file(dir.join(NAME));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crash_is_reported_once() {
        let dir = crate::temp("once");
        assert_eq!(take(&dir), None);
        write(&dir, 3, "patladı\nsatır");
        assert_eq!(take(&dir), Some(Crash { slot: 3, text: "patladı\nsatır".into() }));
        assert_eq!(take(&dir), None);
        assert!(dir.join("crash.last.txt").exists(), "still there for the player");
    }

    #[test]
    fn a_crash_file_of_an_older_build_is_read_too() {
        let dir = crate::temp("old");
        std::fs::write(dir.join("crash.txt"), "panicked at x").unwrap();
        assert_eq!(take(&dir), Some(Crash { slot: 1, text: "panicked at x".into() }));
    }

    #[test]
    fn a_clean_exit_keeps_a_crash_of_an_earlier_run() {
        let dir = crate::temp("earlier");
        std::fs::write(dir.join("crash.txt"), "slot=1\nbefore").unwrap();
        forget_own(&dir);
        assert!(take(&dir).is_some());
    }

    #[test]
    fn the_hook_writes_the_message_and_slot_and_a_clean_exit_drops_it() {
        let dir = crate::temp("hook");
        let before = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        install(dir.clone(), || 2);
        let _ = std::panic::catch_unwind(|| panic!("deneme çökmesi"));
        let _ = std::panic::take_hook();
        std::panic::set_hook(before);
        let text = std::fs::read_to_string(dir.join("crash.txt")).unwrap();
        assert!(text.starts_with("slot=2\n") && text.contains("deneme çökmesi"), "{text}");
        forget_own(&dir);
        assert_eq!(take(&dir), None);
    }
}
