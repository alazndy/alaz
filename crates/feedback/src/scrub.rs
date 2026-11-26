//! Takes what identifies the player or their network out of a log before it leaves the machine.

use std::net::{IpAddr, SocketAddr};

/// The letters of a room code (`alaz-relay-proto`).
const CODE: &str = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

pub fn scrub(text: &str, home: Option<&str>, user: Option<&str>) -> String {
    let mut out = text.to_string();
    // a home folder of "/" would turn every path into "~"
    if let Some(home) = home.filter(|h| h.len() > 1) {
        for form in [home.to_string(), home.replace('\\', "/"), home.replace('/', "\\")] {
            out = out.replace(&form, "~");
        }
    }
    out = addresses_and_codes(&out);
    // a short name would be found inside ordinary words
    if let Some(user) = user.filter(|u| u.chars().count() >= 3) {
        out = whole_words(&out, user, "<user>");
    }
    out
}

fn in_address(c: char) -> bool {
    c.is_ascii_alphanumeric() || ".:[]%-".contains(c)
}

fn addresses_and_codes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        let start = rest.find(in_address).unwrap_or(rest.len());
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let end = rest.find(|c| !in_address(c)).unwrap_or(rest.len());
        let (run, after) = rest.split_at(end);
        // punctuation of the sentence is not part of the address
        let core = run.trim_end_matches(['.', ':', '-']);
        let tail = &run[core.len()..];
        if is_address(core) {
            out.push_str("<addr>");
            out.push_str(tail);
        } else if is_code(core) {
            out.push_str("<code>");
            out.push_str(tail);
        } else {
            out.push_str(run);
        }
        rest = after;
    }
    out
}

fn is_address(run: &str) -> bool {
    let bare = run.strip_prefix('[').and_then(|r| r.strip_suffix(']')).unwrap_or(run);
    let unzoned = bare.split('%').next().unwrap_or(bare);
    run.parse::<SocketAddr>().is_ok() || unzoned.parse::<IpAddr>().is_ok()
}

fn is_code(run: &str) -> bool {
    let letters: Vec<char> = match run.split_once('-') {
        Some((a, b)) if a.len() == 4 && b.len() == 4 => a.chars().chain(b.chars()).collect(),
        Some(_) => return false,
        None => run.chars().collect(),
    };
    letters.len() == 8 && letters.iter().all(|&c| CODE.contains(c))
}

fn whole_words(text: &str, word: &str, with: &str) -> String {
    // ascii lower case keeps every byte position, so the found places fit the original text
    let (lower, needle) = (text.to_ascii_lowercase(), word.to_ascii_lowercase());
    let mut out = String::with_capacity(text.len());
    let mut done = 0;
    for (at, _) in lower.match_indices(&needle) {
        let end = at + needle.len();
        let inside = text[..at].chars().next_back().is_some_and(char::is_alphanumeric) || text[end..].chars().next().is_some_and(char::is_alphanumeric);
        if !inside && at >= done {
            out.push_str(&text[done..at]);
            out.push_str(with);
            done = end;
        }
    }
    out.push_str(&text[done..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(text: &str) -> String {
        scrub(text, Some("/home/ece"), Some("ece"))
    }

    #[test]
    fn the_home_folder_and_user_name_go() {
        assert_eq!(clean("açılamadı /home/ece/.local/share/alaz/save.alz"), "açılamadı ~/.local/share/alaz/save.alz");
        assert_eq!(clean("kullanıcı ece girdi"), "kullanıcı <user> girdi");
        let windows = scrub(r"C:\Users\Ece\AppData ve C:/Users/Ece/AppData", Some(r"C:\Users\Ece"), Some("Ece"));
        assert_eq!(windows, r"~\AppData ve ~/AppData");
    }

    #[test]
    fn a_very_short_user_name_is_not_replaced_everywhere() {
        assert_eq!(scrub("an animal", None, Some("an")), "an animal");
        assert_eq!(scrub("x", Some(""), Some("")), "x");
        assert_eq!(scrub("a/b", Some("/"), None), "a/b");
    }

    #[test]
    fn network_addresses_go() {
        assert_eq!(clean("katılınamadı (192.168.1.5:7777): zaman aşımı"), "katılınamadı (<addr>): zaman aşımı");
        assert_eq!(clean("bağlandı 10.0.0.2, sonra [fe80::1]:7777 ve 2001:db8::5."), "bağlandı <addr>, sonra <addr> ve <addr>.");
        assert_eq!(clean("adres=100.64.1.9"), "adres=<addr>");
    }

    #[test]
    fn room_codes_go() {
        assert_eq!(clean("oda ABCD2345 açıldı"), "oda <code> açıldı");
        assert_eq!(clean("kod: ABCD-EFGH."), "kod: <code>.");
    }

    #[test]
    fn ordinary_text_stays() {
        let keep = "[+12.3s] saat 12:03:04, sürüm 0.6.0, rapor 0123456789abcdef0123456789abcdef, Mağara KAYIT Taş Devri 1280x720 :: son";
        assert_eq!(scrub(keep, None, None), keep);
    }
}
