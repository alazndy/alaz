//! The state behind the version screen: whether a newer release exists, and the check and install that run in the
//! background. The screen itself is drawn by the menu. Nothing is downloaded until the player asks for it.

use alaz_updater::install::{self, Next};
use alaz_updater::net;
use alaz_updater::release::{self, Channel, Platform, Update};
use alaz_updater::verify;
use alaz_updater::version::Version;
use std::sync::mpsc::{channel, Receiver, TryRecvError};

/// What the state machine calls out to, so a test can stand in for the network.
#[derive(Clone, Copy)]
pub struct Backend {
    pub check: fn(Channel) -> Result<Option<Update>, String>,
    pub install: fn(&Update) -> Result<Next, String>,
}

impl Backend {
    pub fn live() -> Backend {
        Backend { check: live_check, install: live_install }
    }
}

pub enum Phase {
    Idle,
    Checking,
    UpToDate,
    Available(Update),
    Installing(Update),
    Failed(String),
    Done(Next),
}

enum Msg {
    Checked(Result<Option<Update>, String>),
    Installed(Result<Next, String>),
}

pub struct UpdateState {
    pub phase: Phase,
    pub channel: Channel,
    backend: Backend,
    rx: Option<Receiver<Msg>>,
}

impl UpdateState {
    pub fn new(backend: Backend, channel: Channel) -> UpdateState {
        UpdateState { phase: Phase::Idle, channel, backend, rx: None }
    }

    /// The live backend and the channel the player chose last time.
    pub fn live() -> UpdateState {
        UpdateState::new(Backend::live(), load_channel())
    }

    pub fn busy(&self) -> bool {
        matches!(self.phase, Phase::Checking | Phase::Installing(_))
    }

    /// Looks for a newer release in the background. Does nothing while a check or install is running.
    pub fn start_check(&mut self) {
        if self.busy() {
            return;
        }
        let (tx, rx) = channel();
        let (check, channel_now) = (self.backend.check, self.channel);
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Checked(check(channel_now)));
        });
        self.rx = Some(rx);
        self.phase = Phase::Checking;
    }

    /// Downloads, verifies and installs the release found by the check. Only from the "available" state.
    pub fn start_install(&mut self) {
        let Phase::Available(update) = std::mem::replace(&mut self.phase, Phase::Idle) else { return };
        let (tx, rx) = channel();
        let (install, job) = (self.backend.install, update.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Installed(install(&job)));
        });
        self.rx = Some(rx);
        self.phase = Phase::Installing(update);
    }

    /// Switches channel, remembers the choice and forgets what the old channel found.
    pub fn set_channel(&mut self, channel: Channel) {
        if channel != self.channel && !self.busy() {
            self.channel = channel;
            self.phase = Phase::Idle;
            save_channel(channel);
        }
    }

    /// Takes in the answer of the background job, if it has come.
    pub fn poll(&mut self) {
        let Some(rx) = &self.rx else { return };
        let msg = match rx.try_recv() {
            Ok(msg) => msg,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.rx = None;
                self.phase = Phase::Failed("the update job stopped without an answer".to_string());
                return;
            }
        };
        self.rx = None;
        self.phase = match msg {
            Msg::Checked(Ok(Some(update))) => Phase::Available(update),
            Msg::Checked(Ok(None)) => Phase::UpToDate,
            Msg::Installed(Ok(next)) => Phase::Done(next),
            Msg::Checked(Err(e)) | Msg::Installed(Err(e)) => Phase::Failed(e),
        };
    }

    /// The version a check found, for a line on the start screen.
    pub fn available(&self) -> Option<&Version> {
        match &self.phase {
            Phase::Available(update) => Some(&update.version),
            _ => None,
        }
    }
}

fn channel_file() -> Option<std::path::PathBuf> {
    install::data_dir().ok().map(|d| d.join("update-channel"))
}

/// The channel saved by `save_channel`; Stable when nothing is saved or the file says something else.
pub fn load_channel() -> Channel {
    match channel_file().and_then(|f| std::fs::read_to_string(f).ok()) {
        Some(text) if text.trim() == "preview" => Channel::Preview,
        _ => Channel::Stable,
    }
}

fn save_channel(channel: Channel) {
    if let Some(file) = channel_file() {
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(file, if channel == Channel::Preview { "preview" } else { "stable" });
    }
}

/// Starts the game again from the program that is running now.
pub fn relaunch() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).spawn();
    }
}

/// A failure in plain Turkish for the version screen. The English text from the updater stays under it as the detail,
/// because it names the exact step that failed.
pub fn explain(err: &str) -> &'static str {
    const CASES: [(&str, &str); 12] = [
        ("network error", "Sunucuya bağlanılamadı. İnternet bağlantını kontrol et."),
        ("could not read the answer", "Sunucunun cevabı okunamadı. Biraz sonra yeniden dene."),
        ("the server answered", "Sunucu hata döndürdü. Biraz sonra yeniden dene."),
        ("does not match", "İndirilen dosya sağlama toplamıyla uyuşmadı. Güvenlik için kurulmadı."),
        ("no checksum listed", "Bu sürümün sağlama toplamı listede yok. Güvenlik için kurulmadı."),
        ("address not allowed", "Beklenmeyen bir adres görüldü. Güvenlik için durduruldu."),
        ("too many redirects", "Sunucu çok kez yönlendirdi. Güvenlik için durduruldu."),
        ("larger than allowed", "İndirilen dosya beklenenden büyük. Durduruldu."),
        ("release list could not be read", "Sürüm listesi okunamadı."),
        ("no package for this system", "Bu sistem için hazır bir paket yok."),
        ("unsafe package name", "Paket adı güvenli değil. Kurulmadı."),
        ("setup program", "Kurulum programı başlatılamadı."),
    ];
    CASES.iter().find(|(key, _)| err.contains(key)).map_or("Kurulum tamamlanamadı. Eski sürüm olduğu gibi duruyor.", |(_, text)| text)
}

fn live_check(channel: Channel) -> Result<Option<Update>, String> {
    check_against(channel, &Version::current())
}

fn check_against(channel: Channel, current: &Version) -> Result<Option<Update>, String> {
    let platform = Platform::current().ok_or("there is no package for this system")?;
    let body = net::get(net::RELEASES_URL, net::MAX_LIST_BYTES)?;
    let text = String::from_utf8(body).map_err(|_| "the release list is not text".to_string())?;
    let releases = release::parse_releases(&text)?;
    Ok(release::pick_update(&releases, current, channel, platform))
}

fn live_install(update: &Update) -> Result<Next, String> {
    install_into(update, &std::env::current_exe().map_err(|e| e.to_string())?)
}

/// The whole install, with the program to replace as a parameter so a test can aim it at a copy.
fn install_into(update: &Update, target: &std::path::Path) -> Result<Next, String> {
    let platform = Platform::current().ok_or("there is no package for this system")?;
    // the name comes from the network: it must be a plain file name before it is joined to a path
    let name = &update.package.name;
    if name.is_empty() || name.contains(['/', '\\']) || name.contains("..") {
        return Err(format!("unsafe package name: {name}"));
    }
    let sums = String::from_utf8(net::get(&update.sums.browser_download_url, net::MAX_LIST_BYTES)?).map_err(|_| "the checksum list is not text".to_string())?;
    let bytes = net::get(&update.package.browser_download_url, net::MAX_PACKAGE_BYTES)?;
    verify::check(&sums, name, &bytes)?;
    let stage = install::stage_dir()?;
    let file = stage.join(name);
    std::fs::write(&file, &bytes).map_err(|e| format!("could not save the package: {e}"))?;
    match platform {
        Platform::LinuxX86_64 => install::install_linux(&file, &stage, target),
        Platform::WindowsInstaller => install::install_windows(&file),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alaz_updater::release::Asset;

    fn update(version: &str) -> Update {
        let asset = |name: &str| Asset { name: name.to_string(), browser_download_url: format!("https://github.com/x/{name}"), size: 1 };
        Update { version: Version::parse(version).unwrap(), package: asset("ALAZ-pkg"), sums: asset("SHA256SUMS") }
    }

    fn wait(state: &mut UpdateState) {
        for _ in 0..500 {
            state.poll();
            if !state.busy() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        panic!("the background job never answered");
    }

    fn found(_: Channel) -> Result<Option<Update>, String> {
        Ok(Some(update("0.6.0")))
    }
    fn nothing(_: Channel) -> Result<Option<Update>, String> {
        Ok(None)
    }
    fn offline(_: Channel) -> Result<Option<Update>, String> {
        Err("network error".to_string())
    }
    fn installed(_: &Update) -> Result<Next, String> {
        Ok(Next::Restart)
    }
    fn refused(_: &Update) -> Result<Next, String> {
        Err("checksum of ALAZ-pkg does not match".to_string())
    }

    #[test]
    fn a_check_finds_an_update_or_none_or_fails() {
        let mut s = UpdateState::new(Backend { check: found, install: installed }, Channel::Stable);
        s.start_check();
        assert!(matches!(s.phase, Phase::Checking));
        wait(&mut s);
        assert_eq!(s.available().map(|v| v.to_string()).as_deref(), Some("0.6.0"));

        let mut s = UpdateState::new(Backend { check: nothing, install: installed }, Channel::Stable);
        s.start_check();
        wait(&mut s);
        assert!(matches!(s.phase, Phase::UpToDate));

        let mut s = UpdateState::new(Backend { check: offline, install: installed }, Channel::Stable);
        s.start_check();
        wait(&mut s);
        assert!(matches!(&s.phase, Phase::Failed(e) if e == "network error"));
    }

    #[test]
    fn nothing_is_installed_before_a_check_found_something() {
        let mut s = UpdateState::new(Backend { check: found, install: installed }, Channel::Stable);
        s.start_install();
        assert!(matches!(s.phase, Phase::Idle), "no install from the idle state");
        s.start_check();
        wait(&mut s);
        s.start_install();
        assert!(matches!(s.phase, Phase::Installing(_)));
        wait(&mut s);
        assert!(matches!(s.phase, Phase::Done(Next::Restart)));
    }

    #[test]
    fn a_refused_install_is_reported_not_hidden() {
        let mut s = UpdateState::new(Backend { check: found, install: refused }, Channel::Stable);
        s.start_check();
        wait(&mut s);
        s.start_install();
        wait(&mut s);
        assert!(matches!(&s.phase, Phase::Failed(e) if e.contains("does not match")));
    }

    #[test]
    fn a_second_check_while_one_runs_does_nothing() {
        let mut s = UpdateState::new(Backend { check: found, install: installed }, Channel::Stable);
        s.start_check();
        s.start_check();
        wait(&mut s);
        assert!(s.available().is_some());
    }

    #[test]
    fn a_job_that_dies_leaves_a_failure_not_a_hang() {
        fn dies(_: Channel) -> Result<Option<Update>, String> {
            panic!("the check crashed");
        }
        let mut s = UpdateState::new(Backend { check: dies, install: installed }, Channel::Stable);
        s.start_check();
        wait(&mut s);
        assert!(matches!(s.phase, Phase::Failed(_)));
    }

    /// Talks to the real GitHub: `XDG_DATA_HOME=$(mktemp -d) cargo test -p alaz -- --ignored real_release`. Needs the
    /// published v0.6.0 release (and no newer one: a later release makes the version asserts below move).
    #[test]
    #[ignore]
    #[cfg(target_os = "linux")]
    fn real_release_is_found_verified_and_installed() {
        let old = Version::parse("0.5.0").unwrap();
        let preview = check_against(Channel::Preview, &old).unwrap().expect("the preview channel finds a newer release");
        assert_eq!(preview.version.to_string(), "0.6.0");
        let stable = check_against(Channel::Stable, &old).unwrap().expect("the stable channel finds 0.6.0");
        assert_eq!(stable.version.to_string(), "0.6.0");
        // a player on the pre-release is offered the final release, and a player on 0.6.0 is up to date
        let rc = Version::parse("0.6.0-rc.1").unwrap();
        assert_eq!(check_against(Channel::Stable, &rc).unwrap().expect("rc to final").version.to_string(), "0.6.0");
        assert!(check_against(Channel::Stable, &Version::parse("0.6.0").unwrap()).unwrap().is_none());
        let root = std::env::temp_dir().join(format!("alaz-e2e-{}", std::process::id()));
        let target = root.join("bin/alaz");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, "old").unwrap();
        assert_eq!(install_into(&preview, &target).unwrap(), Next::Restart);
        let bytes = std::fs::read(&target).unwrap();
        assert!(bytes.starts_with(b"\x7fELF"), "the installed file is a real program");
        eprintln!("installed {} bytes at {}", bytes.len(), target.display());
    }

    #[test]
    fn failures_are_explained_in_turkish() {
        assert!(explain("network error: timed out").starts_with("Sunucuya"));
        assert!(explain("checksum of ALAZ-0.6.0.tar.gz does not match").contains("sağlama toplamı"));
        assert!(explain("the server answered 404").contains("Sunucu hata"));
        assert!(explain("could not start the setup program: denied").contains("başlatılamadı"));
        assert!(explain("tar failed: x").contains("Eski sürüm"), "anything else falls back to a safe line");
        assert!(explain("unsafe package name: ../x").contains("güvenli değil"));
    }

    #[test]
    fn live_install_refuses_a_package_name_that_is_a_path() {
        for name in ["../evil", "a/b", "a\\b", "..", ""] {
            let mut u = update("0.6.0");
            u.package.name = name.to_string();
            assert!(live_install(&u).is_err(), "{name:?}");
        }
    }
}
