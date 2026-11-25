//! Disk side of saving. The byte format lives in `alaz_sim::save`.
//! Location: `$ALAZ_SAVE_DIR`, else on Windows `%APPDATA%\ALAZ`, else `$XDG_DATA_HOME/alaz`, else `~/.local/share/alaz`.

mod discovery;

use crate::cave_map::KnownTiles;
use alaz_sim::{AgentId, World};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub const SLOTS: u8 = 3;
static SLOT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(1);
const MAIN: &str = "save.alz";
const BACKUP: &str = "save.alz.bak";
static KNOWN_PRIMARY: std::sync::Mutex<Option<(PathBuf, Vec<u8>)>> = std::sync::Mutex::new(None);

pub struct LoadedSave {
    pub world: World,
    pub seen: KnownTiles,
    /// The main file was missing or damaged and the backup (the save before it) was opened instead: the player lost
    /// what happened since that older save and has to be told.
    pub recovered: bool,
}

pub enum SaveOutcome {
    Complete,
    WorldOnly(io::Error),
}

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
    // a test run must never read or write the player's own settings and saves (ui.cfg, the keys, the autosave): the
    // interface size in ui.cfg alone changed what the layout tests saw
    if cfg!(test) {
        return std::env::temp_dir().join(format!("alaz-test-{}", std::process::id()));
    }
    // Windows: %APPDATA%\ALAZ
    if cfg!(windows)
        && let Some(d) = std::env::var_os("APPDATA") {
            return PathBuf::from(d).join("ALAZ");
        }
    if let Some(d) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(d).join("alaz");
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".local/share/alaz")
}

/// Rotate only a loadable primary into the backup. A recovered backup survives the next save.
pub fn save_in(dir: &Path, world: &World, me: AgentId, seen: &KnownTiles) -> io::Result<SaveOutcome> {
    let mut known = KNOWN_PRIMARY.lock().unwrap_or_else(|e| e.into_inner());
    fs::create_dir_all(dir)?;
    let main = dir.join(MAIN);
    let backup = dir.join(BACKUP);
    let mut retained = None;
    match fs::read(&main) {
        Ok(bytes) => {
            // Rebuilding a loaded world can stall a frame. Compare with the exact bytes we loaded or wrote.
            let already_known = known
                .as_ref()
                .is_some_and(|(path, valid)| *path == main && *valid == bytes);
            if already_known || World::from_bytes(&bytes).is_ok() {
                write_atomic(&backup, &bytes)?;
                retained = discovery::save_fingerprint(&bytes);
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let bytes = world.to_bytes();
    let discovery = discovery::save_fingerprint(&bytes).map_or(Ok(()), |fingerprint| {
        let retained = retained.or_else(|| discovery::read_fingerprint(&backup));
        // Commit the matching pair of exploration records before replacing the world.
        // A save interrupted here can still open the old primary or the retained backup.
        discovery::update(dir, fingerprint, retained, me, seen)
    });
    write_atomic(&main, &bytes)?;
    *known = Some((main, bytes));
    Ok(match discovery {
        Ok(()) => SaveOutcome::Complete,
        Err(error) => SaveOutcome::WorldOnly(error),
    })
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let tmp = path.with_extension(format!(
        "tmp.{}.{}",
        std::process::id(),
        SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)?;
    let staged = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    let result = staged.and_then(|()| fs::rename(&tmp, path));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn read_save<T>(
    dir: &Path,
    mut decode: impl FnMut(Vec<u8>) -> Result<T, alaz_sim::SaveError>,
) -> (Result<Option<T>, String>, bool) {
    let mut try_file = |name| {
        let p = dir.join(name);
        let bytes = match fs::read(&p) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
            Err(e) => return Some(Err(format!("{}: {e}", p.display()))),
        };
        Some(decode(bytes).map_err(|e| format!("{}: {e}", p.display())))
    };
    let main = try_file(MAIN);
    if let Some(Ok(save)) = main {
        return (Ok(Some(save)), false);
    }
    match try_file(BACKUP) {
        Some(Ok(save)) => (Ok(Some(save)), true),
        backup => (main.or(backup).transpose(), false),
    }
}

/// `Ok(None)` when neither file exists. Fall back if the primary is missing or unusable.
pub fn load_in(dir: &Path, me: AgentId) -> Result<Option<LoadedSave>, String> {
    let (result, recovered) = read_save(dir, |bytes| {
        let world = World::from_bytes(&bytes)?;
        Ok((world, bytes))
    });
    if recovered {
        eprintln!("{}: yedekten yüklendi", dir.display());
    }
    result.map(|save| {
        save.map(|(world, bytes)| {
            let seen = discovery::load(dir, &bytes, &world, me);
            if !recovered {
                let mut known = KNOWN_PRIMARY.lock().unwrap_or_else(|e| e.into_inner());
                *known = Some((dir.join(MAIN), bytes));
            }
            LoadedSave { world, seen, recovered }
        })
    })
}

pub fn delete_in(dir: &Path) {
    let _ = fs::remove_file(dir.join(MAIN));
    let _ = fs::remove_file(dir.join(BACKUP));
    let _ = fs::remove_file(dir.join(discovery::FILE_NAME));
}

/// Slot 1 lives in the base directory (so older saves keep working), the others in `slotN/`.
pub fn slot_dir(n: u8) -> PathBuf {
    if n <= 1 {
        base_dir()
    } else {
        base_dir().join(format!("slot{n}"))
    }
}

/// Append one line to the play log next to the save: what the player did and how it went, so difficulty can be
/// tuned from real play instead of guesses. Best effort; the log stops growing at 1 MB.
pub fn log_play(line: &str) {
    use std::io::Write;
    let path = dir().join("playlog.txt");
    if fs::metadata(&path)
        .map(|m| m.len() > 1_000_000)
        .unwrap_or(false)
    {
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
    slot_info_in(&slot_dir(n))
}

fn slot_info_in(dir: &Path) -> Option<Result<alaz_sim::SaveInfo, String>> {
    read_save(dir, |bytes| alaz_sim::peek_save(&bytes))
        .0
        .transpose()
}

pub fn save(world: &World, me: AgentId, seen: &KnownTiles) -> io::Result<SaveOutcome> {
    save_in(&dir(), world, me, seen)
}

pub fn load(me: AgentId) -> Result<Option<LoadedSave>, String> {
    load_in(&dir(), me)
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

    fn save_world(dir: &Path, world: &World) {
        save_in(dir, world, AgentId(0), &KnownTiles::new()).unwrap();
    }

    fn load_recovered(dir: &Path) -> Option<bool> {
        load_in(dir, AgentId(0)).unwrap().map(|s| s.recovered)
    }

    fn load_world(dir: &Path) -> Result<Option<World>, String> {
        Ok(load_in(dir, AgentId(0))?.map(|save| save.world))
    }

    #[test]
    fn roundtrip_missing_and_delete() {
        let d = temp_dir("rt");
        assert!(load_world(&d).unwrap().is_none());
        let w = world(3);
        save_world(&d, &w);
        assert_eq!(load_world(&d).unwrap().unwrap().hash(), w.hash());
        assert_eq!(fs::read_dir(&d).unwrap().count(), 1);
        delete_in(&d);
        assert!(load_world(&d).unwrap().is_none());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_main_falls_back_to_backup() {
        let d = temp_dir("backup-only");
        let w = world(3);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join(BACKUP), w.to_bytes()).unwrap();
        assert_eq!(slot_info_in(&d).unwrap().unwrap().seed, w.seed);
        assert_eq!(load_world(&d).unwrap().unwrap().hash(), w.hash());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn corrupt_main_recovers_and_preserves_backup() {
        let d = temp_dir("bak");
        let (older, newer) = (world(1), world(2));
        save_world(&d, &older);
        save_world(&d, &newer);
        assert_eq!(slot_info_in(&d).unwrap().unwrap().seed, newer.seed);
        assert_eq!(load_world(&d).unwrap().unwrap().hash(), newer.hash());
        assert_eq!(load_recovered(&d), Some(false), "a good main file is no recovery");

        let mut bytes = fs::read(d.join(MAIN)).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        fs::write(d.join(MAIN), &bytes).unwrap();
        assert_eq!(slot_info_in(&d).unwrap().unwrap().seed, older.seed);
        assert_eq!(load_recovered(&d), Some(true), "the player is told the backup was used");
        let recovered = load_world(&d).unwrap().unwrap();
        assert_eq!(recovered.hash(), older.hash());

        let backup = fs::read(d.join(BACKUP)).unwrap();
        save_world(&d, &recovered);
        assert_eq!(load_world(&d).unwrap().unwrap().hash(), older.hash());
        assert_eq!(fs::read(d.join(BACKUP)).unwrap(), backup);

        fs::write(d.join(MAIN), &bytes).unwrap();
        fs::write(d.join(BACKUP), b"junk").unwrap();
        assert!(slot_info_in(&d).unwrap().is_err());
        assert!(load_world(&d).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn readable_summary_does_not_make_primary_safe_to_back_up() {
        let d = temp_dir("invalid-world");
        let w = world(3);
        fs::create_dir_all(&d).unwrap();
        let backup = w.to_bytes();
        fs::write(d.join(BACKUP), &backup).unwrap();

        let mut invalid = world(4);
        invalid.map.w = 0;
        let primary = invalid.to_bytes();
        assert_eq!(alaz_sim::peek_save(&primary).unwrap().seed, invalid.seed);
        assert!(matches!(
            World::from_bytes(&primary),
            Err(alaz_sim::SaveError::WorldMismatch)
        ));
        fs::write(d.join(MAIN), primary).unwrap();

        let recovered = load_world(&d).unwrap().unwrap();
        assert_eq!(recovered.hash(), w.hash());
        save_world(&d, &recovered);
        assert_eq!(load_world(&d).unwrap().unwrap().hash(), w.hash());
        assert_eq!(fs::read(d.join(BACKUP)).unwrap(), backup);
        let _ = fs::remove_dir_all(&d);
    }

    fn explored(x: i32) -> KnownTiles {
        use alaz_sim::{cave_terrain::CaveTile, caves::Cell};
        KnownTiles::from([
            (CaveTile { x, y: 10, z: -3 }, Cell::Floor),
            (CaveTile { x, y: 11, z: -4 }, Cell::Exit),
        ])
    }

    #[test]
    fn cave_discovery_is_local_to_the_saved_player() {
        let d = temp_dir("cave-players");
        let mut w = world(4);
        let (sx, sy) = w.spawn_mm;
        let guest = w.spawn_agent(sx, sy);
        let host = AgentId(0);
        save_in(&d, &w, host, &explored(10)).unwrap();
        let loaded = load_in(&d, host).unwrap().unwrap();
        assert_eq!(loaded.world.hash(), w.hash());
        assert_eq!(loaded.seen, explored(10));
        assert!(load_in(&d, guest).unwrap().unwrap().seen.is_empty());
        assert_eq!(fs::read(d.join(MAIN)).unwrap(), w.to_bytes());

        save_in(&d, &w, guest, &explored(20)).unwrap();
        assert_eq!(load_in(&d, host).unwrap().unwrap().seen, explored(10));
        assert_eq!(load_in(&d, guest).unwrap().unwrap().seen, explored(20));

        delete_in(&d);
        assert!(!d.join(discovery::FILE_NAME).exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn cave_discovery_rolls_back_to_the_retained_world() {
        let d = temp_dir("cave-backup");
        let mut w = world(4);
        let me = AgentId(0);
        let old = w.hash();
        save_in(&d, &w, me, &explored(10)).unwrap();
        w.step(&[]);
        save_in(&d, &w, me, &explored(20)).unwrap();
        assert_eq!(load_in(&d, me).unwrap().unwrap().seen, explored(20));
        fs::write(d.join(MAIN), b"invalid primary").unwrap();
        let mut recovered = load_in(&d, me).unwrap().unwrap();
        assert_eq!(recovered.world.hash(), old);
        assert_eq!(recovered.seen, explored(10));
        recovered.world.step(&[]);
        save_in(&d, &recovered.world, me, &explored(15)).unwrap();
        assert_eq!(load_in(&d, me).unwrap().unwrap().seen, explored(15));
        fs::remove_file(d.join(MAIN)).unwrap();
        let recovered = load_in(&d, me).unwrap().unwrap();
        assert_eq!(recovered.world.hash(), old);
        assert_eq!(recovered.seen, explored(10));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn cave_discovery_for_an_interrupted_save_keeps_the_old_primary_map() {
        let d = temp_dir("cave-interrupted");
        let mut w = world(4);
        let me = AgentId(0);
        save_in(&d, &w, me, &explored(10)).unwrap();
        let previous = w.to_bytes();
        write_atomic(&d.join(BACKUP), &previous).unwrap();
        w.step(&[]);
        discovery::update(
            &d,
            discovery::save_fingerprint(&w.to_bytes()).unwrap(),
            discovery::save_fingerprint(&previous),
            me,
            &explored(20),
        )
        .unwrap();
        assert_eq!(load_in(&d, me).unwrap().unwrap().seen, explored(10));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn damaged_cave_discovery_does_not_prevent_loading_the_world() {
        let d = temp_dir("cave-map-damaged");
        let w = world(4);
        let me = AgentId(0);
        save_in(&d, &w, me, &explored(10)).unwrap();
        let path = d.join(discovery::FILE_NAME);
        let mut bytes = fs::read(&path).unwrap();
        bytes[12] ^= 1;
        fs::write(&path, bytes).unwrap();
        let loaded = load_in(&d, me).unwrap().unwrap();
        assert_eq!(loaded.world.hash(), w.hash());
        assert!(loaded.seen.is_empty());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn unwritable_cave_discovery_still_saves_the_world() {
        let d = temp_dir("cave-map-unwritable");
        let w = world(4);
        let me = AgentId(0);
        fs::create_dir_all(d.join(discovery::FILE_NAME)).unwrap();
        let outcome = save_in(&d, &w, me, &explored(10)).unwrap();
        assert!(matches!(outcome, SaveOutcome::WorldOnly(_)));
        let restored = load_in(&d, me).unwrap().unwrap();
        assert_eq!(restored.world.hash(), w.hash());
        assert!(restored.seen.is_empty());
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
        assert!(
            text.contains("Struck") && text.lines().count() >= 2,
            "{text:?}"
        );
    }
}
