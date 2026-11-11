//! Disk side of saving. The byte format lives in `alaz_sim::save`.
//! Location: `$ALAZ_SAVE_DIR`, else on Windows `%APPDATA%\ALAZ`, else `$XDG_DATA_HOME/alaz`, else `~/.local/share/alaz`.

use alaz_sim::World;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const SLOTS: u8 = 3;
static SLOT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(1);

/// Choose the save slot (1..=SLOTS) that `save`, `load` and `delete` use from now on.
pub fn set_slot(n: u8) {
    SLOT.store(n.clamp(1, SLOTS), std::sync::atomic::Ordering::Relaxed);
}

pub fn slot() -> u8 {
    SLOT.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn base_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("ALAZ_SAVE_DIR") {
        return PathBuf::from(d);
    }
    // Windows: %APPDATA%\ALAZ
    if cfg!(windows) {
        if let Some(d) = std::env::var_os("APPDATA") {
            return PathBuf::from(d).join("ALAZ");
        }
    }
    if let Some(d) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(d).join("alaz");
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join(".local/share/alaz")
}

/// Atomic write: temp file, fsync, rename. The previous save is kept as `.bak`.
pub fn save_in(dir: &Path, world: &World) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let (main, bak, tmp) = (dir.join("save.alz"), dir.join("save.alz.bak"), dir.join("save.alz.tmp"));
    {
        use std::io::Write;
        let mut f = fs::File::create(&tmp)?;
        f.write_all(&world.to_bytes())?;
        f.sync_all()?;
    }
    if main.exists() {
        let _ = fs::copy(&main, &bak);
    }
    fs::rename(&tmp, &main)
}

/// `Ok(None)` when there is no save. A corrupt main save falls back to the backup.
pub fn load_in(dir: &Path) -> Result<Option<World>, String> {
    let try_file = |p: PathBuf| -> Option<Result<World, String>> {
        let bytes = match fs::read(&p) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
            Err(e) => return Some(Err(format!("{}: {e}", p.display()))),
        };
        Some(World::from_bytes(&bytes).map_err(|e| format!("{}: {e}", p.display())))
    };
    match try_file(dir.join("save.alz")) {
        None => Ok(None),
        Some(Ok(w)) => Ok(Some(w)),
        Some(Err(main_err)) => match try_file(dir.join("save.alz.bak")) {
            Some(Ok(w)) => {
                eprintln!("uyarı: ana kayıt okunamadı ({main_err}), yedekten yüklendi");
                Ok(Some(w))
            }
            _ => Err(main_err),
        },
    }
}

pub fn delete_in(dir: &Path) {
    let _ = fs::remove_file(dir.join("save.alz"));
    let _ = fs::remove_file(dir.join("save.alz.bak"));
}

/// Slot 1 lives in the base directory (so older saves keep working), the others in `slotN/`.
pub fn slot_dir(n: u8) -> PathBuf {
    if n <= 1 { base_dir() } else { base_dir().join(format!("slot{n}")) }
}

/// Append one line to the play log next to the save: what the player did and how it went, so difficulty can be
/// tuned from real play instead of guesses. Best effort; the log stops growing at 1 MB.
pub fn log_play(line: &str) {
    use std::io::Write;
    let path = dir().join("playlog.txt");
    if fs::metadata(&path).map(|m| m.len() > 1_000_000).unwrap_or(false) {
        return;
    }
    let _ = fs::create_dir_all(dir());
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
}

pub fn dir() -> PathBuf {
    slot_dir(slot())
}

/// Summary of a slot: `None` when empty, `Some(Err)` when unreadable.
pub fn slot_info(n: u8) -> Option<Result<alaz_sim::SaveInfo, String>> {
    let bytes = fs::read(slot_dir(n).join("save.alz")).ok()?;
    Some(alaz_sim::peek_save(&bytes).map_err(|e| e.to_string()))
}

pub fn save(world: &World) -> io::Result<()> {
    save_in(&dir(), world)
}

pub fn load() -> Result<Option<World>, String> {
    load_in(&dir())
}

pub fn delete() {
    delete_in(&dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("alaz-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn world(seed: u64) -> World {
        let mut w = World::new(seed);
        let (x, y) = w.spawn_mm;
        w.spawn_agent(x, y);
        for _ in 0..100 {
            w.step(&[]);
        }
        w
    }

    #[test]
    fn roundtrip_missing_and_delete() {
        let d = temp_dir("rt");
        assert!(load_in(&d).unwrap().is_none());
        let w = world(3);
        save_in(&d, &w).unwrap();
        assert_eq!(load_in(&d).unwrap().unwrap().hash(), w.hash());
        assert!(!d.join("save.alz.tmp").exists());
        delete_in(&d);
        assert!(load_in(&d).unwrap().is_none());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn corrupt_main_falls_back_to_backup() {
        let d = temp_dir("bak");
        let (older, newer) = (world(1), world(2));
        save_in(&d, &older).unwrap();
        save_in(&d, &newer).unwrap(); // older becomes .bak
        assert_eq!(load_in(&d).unwrap().unwrap().seed, 2);
        let mut bytes = fs::read(d.join("save.alz")).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        fs::write(d.join("save.alz"), bytes).unwrap();
        assert_eq!(load_in(&d).unwrap().unwrap().seed, 1);
        fs::write(d.join("save.alz.bak"), b"junk").unwrap();
        assert!(load_in(&d).is_err());
        let _ = fs::remove_dir_all(&d);
    }
}

#[cfg(test)]
mod log_tests {
    #[test]
    fn the_play_log_appends_lines_next_to_the_save() {
        let dir = std::env::temp_dir().join(format!("alaz-log-test-{}", std::process::id()));
        // SAFETY: only this test and the menu test touch the variable, and neither depends on the other's directory
        unsafe { std::env::set_var("ALAZ_SAVE_DIR", &dir) };
        super::set_slot(1);
        super::log_play("12\tStruck { hit: true }");
        super::log_play("13\tCrafted");
        let text = std::fs::read_to_string(super::dir().join("playlog.txt")).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(text.contains("Struck") && text.lines().count() >= 2, "{text:?}");
    }
}
