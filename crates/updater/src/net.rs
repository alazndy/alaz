//! The only code that talks to the network: read-only GET requests to GitHub.

use std::time::Duration;

/// Hosts the updater may contact. Any other host, and any redirect to one, is refused (SSRF guard). GitHub serves a
/// release file from its own storage hosts, so those are listed too.
pub const ALLOWED_HOSTS: &[&str] = &["api.github.com", "github.com", "objects.githubusercontent.com", "release-assets.githubusercontent.com"];

/// Where the list of releases is read from.
pub const RELEASES_URL: &str = "https://api.github.com/repos/alazndy/alaz/releases";

/// Largest body the updater accepts: the release list is small, a package is tens of megabytes.
pub const MAX_LIST_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_PACKAGE_BYTES: u64 = 512 * 1024 * 1024;

const MAX_REDIRECTS: usize = 5;

/// Is `url` an https address on an allowed host (no user name, no port)?
pub fn allowed(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    ALLOWED_HOSTS.contains(&host)
}

/// GET `url` and return the body, or an error for a refused address, a network failure, a bad status or a body
/// larger than `max_bytes`. Sends a generic client name and nothing about the player.
pub fn get(url: &str, max_bytes: u64) -> Result<Vec<u8>, String> {
    get_checked(url, max_bytes, allowed)
}

/// `get` with the address check passed in, so a test can talk to a server on this machine.
pub(crate) fn get_checked(url: &str, max_bytes: u64, allow: fn(&str) -> bool) -> Result<Vec<u8>, String> {
    // redirects are followed by hand: each hop must pass the same check as the first address
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .max_redirects(0)
        .http_status_as_error(false)
        .user_agent("alaz-updater")
        .timeout_global(Some(if max_bytes > MAX_LIST_BYTES { Duration::from_secs(900) } else { Duration::from_secs(20) }))
        .build()
        .into();
    let mut url = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        if !allow(&url) {
            return Err(format!("address not allowed: {}", url.split(['?', '#']).next().unwrap_or("")));
        }
        let response = agent.get(&url).header("Accept", "application/vnd.github+json, application/octet-stream").call().map_err(|e| format!("network error: {e}"))?;
        let status = response.status().as_u16();
        if (300..400).contains(&status) {
            let next = response.headers().get("location").and_then(|v| v.to_str().ok()).ok_or("redirect without a location")?;
            url = next.to_string();
            continue;
        }
        if status != 200 {
            return Err(format!("the server answered {status}"));
        }
        let declared = response.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
        if declared.is_some_and(|n| n > max_bytes) {
            return Err("the answer is larger than allowed".to_string());
        }
        let body = response.into_body().with_config().limit(max_bytes + 1).read_to_vec().map_err(|e| format!("could not read the answer: {e}"))?;
        return if body.len() as u64 > max_bytes { Err("the answer is larger than allowed".to_string()) } else { Ok(body) };
    }
    Err("too many redirects".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn local(url: &str) -> bool {
        url.starts_with("http://127.0.0.1:")
    }

    /// Serves each canned answer to one connection, in order, and returns the address.
    fn serve(answers: Vec<String>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for answer in answers {
                let Ok((mut stream, _)) = listener.accept() else { return };
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(answer.as_bytes());
            }
        });
        format!("http://{addr}")
    }

    fn ok(body: &str) -> String {
        format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
    }

    #[test]
    fn only_https_on_listed_hosts_is_allowed() {
        assert!(allowed("https://api.github.com/repos/alazndy/alaz/releases"));
        assert!(allowed("https://github.com/alazndy/alaz/releases/download/v0.6.0/SHA256SUMS"));
        assert!(allowed("https://objects.githubusercontent.com/x?y=z"));
        for bad in [
            "http://api.github.com/x",
            "https://evil.example/x",
            "https://api.github.com.evil.example/x",
            "https://evil.example/api.github.com",
            "https://api.github.com@evil.example/x",
            "https://api.github.com:8443/x",
            "https://evil.example#@api.github.com/",
            "ftp://github.com/x",
            "https://",
            "",
            "file:///etc/passwd",
        ] {
            assert!(!allowed(bad), "{bad:?} must be refused");
        }
    }

    #[test]
    fn get_refuses_an_address_before_any_connection() {
        let err = get("http://127.0.0.1:1/x", 10).unwrap_err();
        assert!(err.contains("not allowed"), "{err}");
    }

    #[test]
    fn reads_a_body_and_enforces_the_size_limit() {
        let base = serve(vec![ok("hello"), ok("hello")]);
        assert_eq!(get_checked(&format!("{base}/a"), 100, local).unwrap(), b"hello");
        let err = get_checked(&format!("{base}/b"), 3, local).unwrap_err();
        assert!(err.contains("larger"), "{err}");
    }

    #[test]
    fn a_body_longer_than_it_declared_is_cut_off_at_the_limit() {
        let lying = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nabcdefghij".to_string();
        let base = serve(vec![lying]);
        let result = get_checked(&format!("{base}/c"), 5, local);
        assert!(result.as_ref().map_or(true, |b| b.len() <= 5), "{result:?}");
    }

    #[test]
    fn a_bad_status_is_an_error() {
        let base = serve(vec!["HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()]);
        assert!(get_checked(&format!("{base}/d"), 100, local).unwrap_err().contains("404"));
    }

    #[test]
    fn a_redirect_to_a_refused_host_is_not_followed() {
        let redirect = "HTTP/1.1 302 Found\r\nLocation: https://evil.example/steal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string();
        let base = serve(vec![redirect]);
        let err = get_checked(&format!("{base}/e"), 100, local).unwrap_err();
        assert!(err.contains("not allowed") && err.contains("evil.example"), "{err}");
    }

    #[test]
    fn a_redirect_to_an_allowed_address_is_followed() {
        let target = serve(vec![ok("moved")]);
        let redirect = format!("HTTP/1.1 302 Found\r\nLocation: {target}/there\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        let base = serve(vec![redirect]);
        assert_eq!(get_checked(&format!("{base}/f"), 100, local).unwrap(), b"moved");
    }

    #[test]
    fn a_redirect_loop_ends() {
        let base_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = base_listener.local_addr().unwrap();
        std::thread::spawn(move || {
            while let Ok((mut stream, _)) = base_listener.accept() {
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(format!("HTTP/1.1 302 Found\r\nLocation: http://{addr}/again\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes());
            }
        });
        assert!(get_checked(&format!("http://{addr}/start"), 100, local).unwrap_err().contains("too many redirects"));
    }
}
