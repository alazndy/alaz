//! Puts a verified package in place.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What the game must do after an install.
#[derive(Debug, PartialEq, Eq)]
pub enum Next {
    /// The new program is in place: start the game again.
    Restart,
    /// A setup program was started: close the game so it can replace the files.
    Exit,
}

/// Linux: unpack the tar.gz under `stage`, then replace the installed binary (`~/.local/bin/alaz`) with the new one in
/// one rename, so a failed install leaves the old game working.
pub fn install_linux(package: &Path, stage: &Path, target: &Path) -> Result<Next, String> {
    let unpacked = stage.join("unpacked");
    fs::create_dir_all(&unpacked).map_err(|e| format!("could not make {}: {e}", unpacked.display()))?;
    let out = Command::new("tar").arg("-xzf").arg(package).arg("-C").arg(&unpacked).output().map_err(|e| format!("could not run tar: {e}"))?;
    if !out.status.success() {
        return Err(format!("tar failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let binary = find_binary(&unpacked)?;
    let dir = target.parent().ok_or("the install path has no folder")?;
    fs::create_dir_all(dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
    // next to the target, so the rename stays on one disk and is one step
    let fresh = dir.join(".alaz.update");
    fs::copy(&binary, &fresh).map_err(|e| format!("could not copy the new program: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = fs::set_permissions(&fresh, fs::Permissions::from_mode(0o755)) {
            let _ = fs::remove_file(&fresh);
            return Err(format!("could not make the new program runnable: {e}"));
        }
    }
    if let Err(e) = fs::rename(&fresh, target) {
        let _ = fs::remove_file(&fresh);
        return Err(format!("could not put the new program in place: {e}"));
    }
    Ok(Next::Restart)
}

/// The `alaz` file in the unpacked package: at the top or one folder down (`ALAZ-<version>-linux-x86_64/alaz`).
fn find_binary(unpacked: &Path) -> Result<PathBuf, String> {
    let direct = unpacked.join("alaz");
    if direct.is_file() {
        return Ok(direct);
    }
    let entries = fs::read_dir(unpacked).map_err(|e| format!("could not read the unpacked package: {e}"))?;
    for entry in entries.flatten() {
        let candidate = entry.path().join("alaz");
        if entry.path().is_dir() && candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err("the package has no alaz program".to_string())
}

/// Windows: start the downloaded setup program. The player sees the system's own prompts.
pub fn install_windows(setup: &Path) -> Result<Next, String> {
    if !setup.is_file() {
        return Err(format!("{} is missing", setup.display()));
    }
    Command::new(setup).spawn().map_err(|e| format!("could not start the setup program: {e}"))?;
    Ok(Next::Exit)
}

/// The game's own data folder (`%APPDATA%\ALAZ`, or `~/.local/share/alaz` on Linux).
pub fn data_dir() -> Result<PathBuf, String> {
    if cfg!(windows) {
        Ok(PathBuf::from(std::env::var_os("APPDATA").ok_or("APPDATA is not set")?).join("ALAZ"))
    } else if let Some(data) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        Ok(PathBuf::from(data).join("alaz"))
    } else {
        Ok(PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?).join(".local/share/alaz"))
    }
}

/// A fresh folder for a download, under the game's own data folder.
pub fn stage_dir() -> Result<PathBuf, String> {
    stage_dir_in(&data_dir()?.join("update"))
}

fn stage_dir_in(parent: &Path) -> Result<PathBuf, String> {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = parent.join(format!("{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
    Ok(dir)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn temp(name: &str) -> PathBuf {
        stage_dir_in(&std::env::temp_dir().join(format!("alaz-updater-test-{name}"))).unwrap()
    }

    /// A tar.gz holding `<folder>/alaz` with `script` as its content.
    fn package(root: &Path, folder: &str, script: &str) -> PathBuf {
        let src = root.join("src");
        fs::create_dir_all(src.join(folder)).unwrap();
        fs::write(src.join(folder).join("alaz"), script).unwrap();
        let tarball = root.join("pkg.tar.gz");
        let status = Command::new("tar").arg("-czf").arg(&tarball).arg("-C").arg(&src).arg(folder).status().unwrap();
        assert!(status.success());
        tarball
    }

    #[test]
    fn installs_a_new_binary_over_the_old_one() {
        let root = temp("ok");
        let pkg = package(&root, "ALAZ-0.6.0-linux-x86_64", "#!/bin/sh\necho new\n");
        let target = root.join("bin/alaz");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "old").unwrap();
        assert_eq!(install_linux(&pkg, &root.join("stage"), &target).unwrap(), Next::Restart);
        assert_eq!(fs::read_to_string(&target).unwrap(), "#!/bin/sh\necho new\n");
        assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o111, 0o111, "runnable");
        assert!(!target.parent().unwrap().join(".alaz.update").exists(), "no leftover file");
    }

    #[test]
    fn a_broken_package_leaves_the_old_game_alone() {
        let root = temp("bad");
        let target = root.join("bin/alaz");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "old").unwrap();
        let garbage = root.join("garbage.tar.gz");
        fs::write(&garbage, "this is not a tarball").unwrap();
        assert!(install_linux(&garbage, &root.join("stage"), &target).is_err());
        assert!(install_linux(&root.join("missing.tar.gz"), &root.join("stage2"), &target).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "old");
    }

    #[test]
    fn a_package_without_the_program_is_refused() {
        let root = temp("empty");
        let src = root.join("src/ALAZ-x");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("README.md"), "no program here").unwrap();
        let tarball = root.join("pkg.tar.gz");
        assert!(Command::new("tar").arg("-czf").arg(&tarball).arg("-C").arg(root.join("src")).arg("ALAZ-x").status().unwrap().success());
        let target = root.join("bin/alaz");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "old").unwrap();
        assert!(install_linux(&tarball, &root.join("stage"), &target).unwrap_err().contains("no alaz program"));
        assert_eq!(fs::read_to_string(&target).unwrap(), "old");
    }

    #[test]
    fn a_missing_setup_program_is_an_error() {
        assert!(install_windows(Path::new("/nonexistent/setup.exe")).is_err());
    }

    #[test]
    fn each_stage_folder_is_new() {
        let parent = std::env::temp_dir().join("alaz-updater-test-stage");
        let (a, b) = (stage_dir_in(&parent).unwrap(), stage_dir_in(&parent).unwrap());
        assert!(a.is_dir() && b.is_dir() && a != b);
    }
}
