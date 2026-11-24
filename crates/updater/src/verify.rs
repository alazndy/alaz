//! Checks a downloaded file against the release's `SHA256SUMS`.

use sha2::{Digest, Sha256};

/// Finds the hex digest listed for `file_name` in a `sha256sum`-style list (`<hex>  <name>` per line, the name
/// optionally marked with `*`). The digest comes back in lowercase; a line that is not 64 hex digits is skipped.
pub fn expected_digest(sums: &str, file_name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (digest, name) = line.trim().split_once(char::is_whitespace)?;
        let name = name.trim_start().trim_start_matches('*');
        (name == file_name && digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit())).then(|| digest.to_ascii_lowercase())
    })
}

/// The SHA-256 of `bytes` as lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Ok only when the digest listed for `file_name` equals the digest of `bytes`. A missing entry is an error.
pub fn check(sums: &str, file_name: &str, bytes: &[u8]) -> Result<(), String> {
    let expected = expected_digest(sums, file_name).ok_or_else(|| format!("no checksum listed for {file_name}"))?;
    let actual = sha256_hex(bytes);
    if expected == actual { Ok(()) } else { Err(format!("checksum of {file_name} does not match")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn hashes_known_inputs() {
        assert_eq!(sha256_hex(b""), EMPTY);
        assert_eq!(sha256_hex(b"abc"), ABC);
    }

    #[test]
    fn finds_the_digest_of_a_file_in_a_list() {
        let sums = format!("{EMPTY}  one.zip\n{}  *two.tar.gz\n", ABC.to_uppercase());
        assert_eq!(expected_digest(&sums, "one.zip").as_deref(), Some(EMPTY));
        assert_eq!(expected_digest(&sums, "two.tar.gz").as_deref(), Some(ABC));
        assert_eq!(expected_digest(&sums, "three.zip"), None);
        // part of a name is not the name
        assert_eq!(expected_digest(&sums, "one"), None);
    }

    #[test]
    fn skips_lines_that_are_not_digests() {
        let sums = format!("not-a-digest  one.zip\n{}  one.zip\n{EMPTY}  one.zip\n", "g".repeat(64));
        assert_eq!(expected_digest(&sums, "one.zip").as_deref(), Some(EMPTY));
    }

    #[test]
    fn accepts_only_a_matching_file() {
        let sums = format!("{ABC}  pkg.tar.gz\n");
        assert!(check(&sums, "pkg.tar.gz", b"abc").is_ok());
        assert!(check(&sums, "pkg.tar.gz", b"abd").unwrap_err().contains("does not match"));
        assert!(check(&sums, "other.tar.gz", b"abc").unwrap_err().contains("no checksum"));
        assert!(check("", "pkg.tar.gz", b"abc").is_err());
    }
}
