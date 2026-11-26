//! The only code of the report that talks to the network: one POST to the feedback site.

use std::time::Duration;

use crate::outbox::Outcome;
use crate::report::Report;

pub const URL: &str = "https://feedback.alazlab.com";

/// The address to send to. An override that is not `https`, or `http` to this machine, gives none: a report must
/// not go somewhere else than where the developer meant.
pub fn base(over: Option<&str>) -> Option<String> {
    let Some(over) = over else { return Some(URL.to_string()) };
    let address = over.trim_end_matches('/');
    let (secure, rest) = match (address.strip_prefix("https://"), address.strip_prefix("http://")) {
        (Some(rest), _) => (true, rest),
        (None, Some(rest)) => (false, rest),
        _ => return None,
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') || address.contains(char::is_whitespace) {
        return None;
    }
    let here = ["localhost", "127.0.0.1", "[::1]"].iter().any(|host| {
        authority.strip_prefix(host).is_some_and(|port| port.is_empty() || port.strip_prefix(':').is_some_and(|p| p.parse::<u16>().is_ok()))
    });
    (secure || here).then(|| address.to_string())
}

/// The report as `multipart/form-data`.
pub fn body(report: &Report, boundary: &str) -> Vec<u8> {
    let mut out = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"report\"\r\n\r\n{}\r\n", report.fields.json()).into_bytes();
    for (part, bytes) in &report.files {
        let name = part.name();
        out.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes());
        out.extend_from_slice(bytes);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    out
}

/// Send one report. Only the answers the site gives for a bad report end the attempts: a missing page or a refused
/// visit comes from a wrong address or from the platform, and the report must survive that.
pub fn send(base: &str, report: &Report) -> Outcome {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .max_redirects(0)
        .http_status_as_error(false)
        .user_agent("alaz-feedback")
        .timeout_global(Some(Duration::from_secs(60)))
        .build()
        .into();
    let boundary = format!("alaz-{}", report.fields.id);
    let answer = agent
        .post(format!("{base}/api/reports"))
        .header("Content-Type", format!("multipart/form-data; boundary={boundary}"))
        .send(&body(report, &boundary)[..]);
    match answer.map(|a| a.status().as_u16()) {
        Ok(200 | 201) => Outcome::Sent,
        Ok(400 | 413) => Outcome::Refused,
        _ => Outcome::Later,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{tests::draft, Draft};
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;

    fn report() -> Report {
        Draft { screenshot: Some(vec![0x89, 0, 1]), log: Some(b"satir\n".to_vec()), ..draft() }.build("a".repeat(32)).unwrap()
    }

    /// A server on this machine that answers one request with `status` and hands back what it got.
    fn server(status: &'static str) -> (String, mpsc::Receiver<(String, Vec<u8>)>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let (mut head, mut length) = (String::new(), 0usize);
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap();
                }
                if line == "\r\n" {
                    break;
                }
                head.push_str(&line);
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            write!(reader.get_mut(), "HTTP/1.1 {status}\r\nContent-Length: 0\r\nLocation: http://127.0.0.1:1/x\r\nConnection: close\r\n\r\n").unwrap();
            tx.send((head, body)).unwrap();
        });
        (base, rx)
    }

    #[test]
    fn only_https_or_this_machine() {
        assert_eq!(base(None).as_deref(), Some(URL));
        assert_eq!(base(Some("https://x.example/")).as_deref(), Some("https://x.example"));
        for ok in ["http://localhost:3000", "http://127.0.0.1:1", "http://[::1]:2", "http://localhost"] {
            assert_eq!(base(Some(ok)).as_deref(), Some(ok));
        }
        for bad in ["http://example.com", "ftp://x", "https://a@b", "https://", "http://localhost.evil.com", "http://127.0.0.1.evil.com:80", "http://localhost:80@evil.com", "http://localhost:x", "localhost:3000", "https://a b", ""] {
            assert_eq!(base(Some(bad)), None, "{bad}");
        }
    }

    #[test]
    fn the_body_is_a_form_with_the_fields_as_text() {
        let r = report();
        let text = String::from_utf8_lossy(&body(&r, "BB")).into_owned();
        assert!(text.starts_with(&format!("--BB\r\nContent-Disposition: form-data; name=\"report\"\r\n\r\n{}\r\n", r.fields.json())), "{text}");
        assert!(text.contains("name=\"screenshot\"; filename=\"screenshot\""));
        assert!(text.contains("name=\"log\"; filename=\"log\"\r\nContent-Type: application/octet-stream\r\n\r\nsatir\n\r\n"));
        assert!(!text.contains("name=\"save\""));
        assert!(text.ends_with("\r\n--BB--\r\n"));
    }

    #[test]
    fn the_request_reaches_the_upload_address() {
        let (base, got) = server("201 Created");
        assert_eq!(send(&base, &report()), Outcome::Sent);
        let (head, sent) = got.recv().unwrap();
        assert!(head.starts_with("POST /api/reports HTTP/1.1"), "{head}");
        let head = head.to_ascii_lowercase();
        assert!(head.contains("user-agent: alaz-feedback") && head.contains("content-type: multipart/form-data; boundary="), "{head}");
        let boundary = format!("alaz-{}", "a".repeat(32));
        assert_eq!(sent, body(&report(), &boundary));
    }

    #[test]
    fn only_a_bad_report_is_refused_for_good() {
        for (status, want) in [
            ("200 OK", Outcome::Sent),
            ("400 Bad Request", Outcome::Refused),
            ("413 Payload Too Large", Outcome::Refused),
            ("401 Unauthorized", Outcome::Later),
            ("403 Forbidden", Outcome::Later),
            ("404 Not Found", Outcome::Later),
            ("429 Too Many Requests", Outcome::Later),
            ("503 Service Unavailable", Outcome::Later),
            ("302 Found", Outcome::Later),
        ] {
            let (base, _got) = server(status);
            assert_eq!(send(&base, &report()), want, "{status}");
        }
    }

    #[test]
    fn no_server_means_later() {
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        assert_eq!(send(&format!("http://127.0.0.1:{port}"), &report()), Outcome::Later);
    }
}
