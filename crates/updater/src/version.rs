//! Release versions and their order.

use std::cmp::Ordering;

/// `major.minor.patch` with an optional pre-release tag, as in `0.6.0-rc1`. A version with a tag is older than the same
/// version without one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub pre: Option<String>,
}

impl Version {
    /// Reads `0.5.0`, `v0.5.0` and `0.6.0-rc1`. Anything else (extra parts, letters in the numbers) is None.
    pub fn parse(text: &str) -> Option<Version> {
        let text = text.strip_prefix('v').unwrap_or(text);
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) if !pre.is_empty() && pre.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') => (core, Some(pre.to_string())),
            Some(_) => return None,
            None => (text, None),
        };
        let mut parts = core.split('.');
        let mut number = || {
            let part = parts.next()?;
            (!part.is_empty() && part.bytes().all(|b| b.is_ascii_digit())).then(|| part.parse::<u32>().ok()).flatten()
        };
        let (major, minor, patch) = (number()?, number()?, number()?);
        if parts.next().is_some() {
            return None;
        }
        Some(Version { major, minor, patch, pre })
    }

    /// The version this program was built as (the workspace version).
    pub fn current() -> Version {
        Version::parse(env!("CARGO_PKG_VERSION")).expect("the workspace version is major.minor.patch")
    }
}

/// Pre-release tags compare part by part: numbers by value, other text by letters, a number before text, and a tag with
/// more parts after an equal start is later.
fn compare_pre(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.split('.'), b.split('.'));
    loop {
        match (a.next(), b.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let order = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => x.cmp(y),
                };
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch)).then_with(|| match (&self.pre, &other.pre) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => compare_pre(a, b),
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        match &self.pre {
            Some(pre) => write!(f, "-{pre}"),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap_or_else(|| panic!("{text} should parse"))
    }

    #[test]
    fn reads_plain_prefixed_and_tagged_versions() {
        assert_eq!(v("0.5.0"), Version { major: 0, minor: 5, patch: 0, pre: None });
        assert_eq!(v("v1.2.3"), Version { major: 1, minor: 2, patch: 3, pre: None });
        assert_eq!(v("0.6.0-rc1").pre.as_deref(), Some("rc1"));
        assert_eq!(v("0.6.0-rc1").to_string(), "0.6.0-rc1");
    }

    #[test]
    fn refuses_what_is_not_a_version() {
        for bad in ["", "1", "1.2", "1.2.3.4", "a.b.c", "1.2.x", "1.2.3-", "1..3", "-1.2.3", "1.2.3-rc 1", "v", "1.2.99999999999"] {
            assert!(Version::parse(bad).is_none(), "{bad:?} must not parse");
        }
    }

    #[test]
    fn orders_numbers_by_value_not_by_text() {
        assert!(v("0.10.0") > v("0.9.9"));
        assert!(v("1.0.0") > v("0.99.99"));
        assert!(v("0.5.1") > v("0.5.0"));
    }

    #[test]
    fn a_pre_release_is_older_than_its_release_and_newer_than_the_one_before() {
        assert!(v("0.6.0-rc1") < v("0.6.0"));
        assert!(v("0.6.0-rc1") > v("0.5.9"));
        assert!(v("0.6.0-rc2") > v("0.6.0-rc1"));
        // text is compared by letters, as in semver: name tags `rc.1`, `rc.10` to have the numbers compared by value
        assert!(v("0.6.0-rc10") < v("0.6.0-rc9"));
        assert!(v("0.6.0-rc.10") > v("0.6.0-rc.9"));
        assert!(v("0.6.0-1") < v("0.6.0-rc1"));
        assert!(v("0.6.0-rc.1") < v("0.6.0-rc.1.1"));
    }

    #[test]
    fn the_current_version_is_the_workspace_one() {
        assert_eq!(Version::current().to_string(), env!("CARGO_PKG_VERSION"));
    }
}
