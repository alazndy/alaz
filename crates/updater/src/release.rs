//! What GitHub says about the releases, and which one and which file to take.

use crate::version::Version;
use serde::Deserialize;

/// Stable follows full releases only; Preview also follows pre-releases (tags such as `v0.6.0-rc1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    Stable,
    Preview,
}

/// The parts of a GitHub release the updater uses (`GET /repos/{owner}/{repo}/releases`).
#[derive(Clone, Debug, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

/// The systems a package is built for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    WindowsInstaller,
    LinuxX86_64,
}

impl Platform {
    /// The platform this program runs on, or None where no package is built.
    pub fn current() -> Option<Platform> {
        if cfg!(all(windows, target_arch = "x86_64")) {
            Some(Platform::WindowsInstaller)
        } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            Some(Platform::LinuxX86_64)
        } else {
            None
        }
    }

    /// How the package for this platform is named by the release workflow (`ALAZ-<version>-<this ending>`).
    pub fn package_ending(self) -> &'static str {
        match self {
            Platform::WindowsInstaller => "-windows-setup.exe",
            Platform::LinuxX86_64 => "-linux-x86_64.tar.gz",
        }
    }
}

/// The name of the checksum list a release carries.
pub const SUMS_NAME: &str = "SHA256SUMS";

/// A newer release, with the package for this platform and the checksum list that goes with it.
#[derive(Clone, Debug)]
pub struct Update {
    pub version: Version,
    pub package: Asset,
    pub sums: Asset,
}

/// Reads the JSON list of releases. A body that does not parse is an error, never an empty list.
pub fn parse_releases(json: &str) -> Result<Vec<Release>, String> {
    serde_json::from_str(json).map_err(|e| format!("the release list could not be read: {e}"))
}

/// The newest release on `channel` that is newer than `current` and has both the package for `platform` and a
/// `SHA256SUMS` file. Drafts are ignored. None when there is nothing to install.
pub fn pick_update(releases: &[Release], current: &Version, channel: Channel, platform: Platform) -> Option<Update> {
    releases
        .iter()
        .filter(|r| !r.draft)
        .filter_map(|r| {
            let version = Version::parse(&r.tag_name)?;
            // a tag with "-" is a pre-release even if GitHub's flag was not ticked, and the other way round
            let pre = r.prerelease || version.pre.is_some();
            if (pre && channel == Channel::Stable) || version <= *current {
                return None;
            }
            let package = r.assets.iter().find(|a| a.name.starts_with("ALAZ-") && a.name.ends_with(platform.package_ending()))?;
            let sums = r.assets.iter().find(|a| a.name == SUMS_NAME)?;
            Some(Update { version, package: package.clone(), sums: sums.clone() })
        })
        .max_by(|a, b| a.version.cmp(&b.version))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> String {
        format!(r#"{{"name":"{name}","browser_download_url":"https://github.com/alazndy/alaz/releases/download/x/{name}","size":10}}"#)
    }

    fn release(tag: &str, prerelease: bool, draft: bool, names: &[&str]) -> String {
        let assets: Vec<String> = names.iter().map(|n| asset(n)).collect();
        format!(r#"{{"tag_name":"{tag}","prerelease":{prerelease},"draft":{draft},"assets":[{}],"body":"ignored"}}"#, assets.join(","))
    }

    fn files(version: &str) -> Vec<String> {
        vec![format!("ALAZ-{version}-windows-setup.exe"), format!("ALAZ-{version}-linux-x86_64.tar.gz"), "SHA256SUMS".to_string()]
    }

    fn list(entries: &[(&str, bool, bool)]) -> Vec<Release> {
        let json: Vec<String> = entries
            .iter()
            .map(|(tag, pre, draft)| {
                let names = files(tag.trim_start_matches('v'));
                release(tag, *pre, *draft, &names.iter().map(String::as_str).collect::<Vec<_>>())
            })
            .collect();
        parse_releases(&format!("[{}]", json.join(","))).unwrap()
    }

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn a_body_that_is_not_a_release_list_is_an_error() {
        assert!(parse_releases("").is_err());
        assert!(parse_releases("{\"message\":\"rate limit\"}").is_err());
        assert!(parse_releases("[]").unwrap().is_empty());
    }

    #[test]
    fn picks_the_newest_package_for_the_platform() {
        let releases = list(&[("v0.5.0", false, false), ("v0.6.0", false, false), ("v0.5.1", false, false)]);
        let update = pick_update(&releases, &v("0.5.0"), Channel::Stable, Platform::LinuxX86_64).unwrap();
        assert_eq!(update.version, v("0.6.0"));
        assert_eq!(update.package.name, "ALAZ-0.6.0-linux-x86_64.tar.gz");
        assert_eq!(update.sums.name, "SHA256SUMS");
        let win = pick_update(&releases, &v("0.5.0"), Channel::Stable, Platform::WindowsInstaller).unwrap();
        assert_eq!(win.package.name, "ALAZ-0.6.0-windows-setup.exe");
    }

    #[test]
    fn nothing_when_already_current_or_ahead() {
        let releases = list(&[("v0.5.0", false, false)]);
        assert!(pick_update(&releases, &v("0.5.0"), Channel::Stable, Platform::LinuxX86_64).is_none());
        assert!(pick_update(&releases, &v("0.5.1"), Channel::Preview, Platform::LinuxX86_64).is_none());
        assert!(pick_update(&[], &v("0.5.0"), Channel::Stable, Platform::LinuxX86_64).is_none());
    }

    #[test]
    fn stable_skips_pre_releases_and_preview_takes_them() {
        let releases = list(&[("v0.5.0", false, false), ("v0.6.0-rc1", true, false)]);
        assert!(pick_update(&releases, &v("0.5.0"), Channel::Stable, Platform::LinuxX86_64).is_none());
        let update = pick_update(&releases, &v("0.5.0"), Channel::Preview, Platform::LinuxX86_64).unwrap();
        assert_eq!(update.version, v("0.6.0-rc1"));
        // a dash in the tag is enough even when the flag was not set
        let unflagged = list(&[("v0.6.0-rc1", false, false)]);
        assert!(pick_update(&unflagged, &v("0.5.0"), Channel::Stable, Platform::LinuxX86_64).is_none());
    }

    #[test]
    fn a_release_beats_its_own_pre_release_on_preview() {
        let releases = list(&[("v0.6.0-rc1", true, false), ("v0.6.0", false, false)]);
        assert_eq!(pick_update(&releases, &v("0.5.0"), Channel::Preview, Platform::LinuxX86_64).unwrap().version, v("0.6.0"));
        // and a pre-release is an update for someone on the older release, not for someone on the release itself
        assert!(pick_update(&releases, &v("0.6.0"), Channel::Preview, Platform::LinuxX86_64).is_none());
    }

    #[test]
    fn drafts_and_releases_without_the_files_are_ignored() {
        let drafts = list(&[("v0.6.0", false, true)]);
        assert!(pick_update(&drafts, &v("0.5.0"), Channel::Preview, Platform::LinuxX86_64).is_none());
        let no_sums = parse_releases(&format!("[{}]", release("v0.6.0", false, false, &["ALAZ-0.6.0-linux-x86_64.tar.gz"]))).unwrap();
        assert!(pick_update(&no_sums, &v("0.5.0"), Channel::Stable, Platform::LinuxX86_64).is_none());
        let no_package = parse_releases(&format!("[{}]", release("v0.6.0", false, false, &["SHA256SUMS", "ALAZ-0.6.0-windows-setup.exe"]))).unwrap();
        assert!(pick_update(&no_package, &v("0.5.0"), Channel::Stable, Platform::LinuxX86_64).is_none());
        let odd_tag = list(&[("nightly", false, false)]);
        assert!(pick_update(&odd_tag, &v("0.5.0"), Channel::Preview, Platform::LinuxX86_64).is_none());
    }

    #[test]
    fn falls_back_to_an_older_release_that_has_the_files() {
        let newest = release("v0.7.0", false, false, &["SHA256SUMS"]);
        let older = release("v0.6.0", false, false, &files("0.6.0").iter().map(String::as_str).collect::<Vec<_>>());
        let releases = parse_releases(&format!("[{newest},{older}]")).unwrap();
        assert_eq!(pick_update(&releases, &v("0.5.0"), Channel::Stable, Platform::LinuxX86_64).unwrap().version, v("0.6.0"));
    }
}
