//! Messages and the join code shared by the relay server and the game client.
//! Kept free of `alaz-sim` so the relay binary builds fast.
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fmt;
use std::io::{self, Read, Write};

pub const RELAY_PROTOCOL: u32 = 1;
pub const MAX_RELAY_FRAME: usize = 4096;

/// A-Z without I and O, plus 2-9: no symbol can be mistaken for 0 or 1.
const ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// Deserialization validates every byte: a peer must not smuggle a non-alphabet `Code` past `read_msg`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "[u8; 8]")]
pub struct Code([u8; 8]);

impl TryFrom<[u8; 8]> for Code {
    type Error = &'static str;
    fn try_from(raw: [u8; 8]) -> Result<Self, Self::Error> {
        if raw.iter().all(|b| ALPHABET.contains(b)) {
            Ok(Code(raw))
        } else {
            Err("code symbol outside the alphabet")
        }
    }
}

impl Code {
    pub fn generate() -> Code {
        let mut raw = [0u8; 8];
        getrandom::fill(&mut raw).expect("OS randomness unavailable");
        // 32 symbols: the low 5 bits map onto the alphabet without modulo bias
        Code(raw.map(|b| ALPHABET[(b & 31) as usize]))
    }

    /// Accepts any case, spaces, a missing dash and surrounding whitespace.
    pub fn parse(s: &str) -> Option<Code> {
        let mut out = [0u8; 8];
        let mut n = 0;
        for ch in s.trim().chars() {
            if ch == '-' || ch == ' ' {
                continue;
            }
            let up = ch.to_ascii_uppercase();
            if n == 8 || !up.is_ascii() || !ALPHABET.contains(&(up as u8)) {
                return None;
            }
            out[n] = up as u8;
            n += 1;
        }
        (n == 8).then_some(Code(out))
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = std::str::from_utf8(&self.0).map_err(|_| fmt::Error)?;
        write!(f, "{}-{}", &s[..4], &s[4..])
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToRelay {
    Open { protocol: u32 },
    NewCode,
    Join { protocol: u32, code: Code },
    Accept { id: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FromRelay {
    Opened { code: Code },
    Incoming { id: u64 },
    Paired,
    Refused(Refusal),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Refusal {
    BadProtocol,
    NoSuchCode,
    RoomFull,
    TooManyAttempts,
    Busy,
}

pub fn write_msg<T: Serialize, W: Write>(w: &mut W, m: &T) -> io::Result<()> {
    let body = postcard::to_allocvec(m).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    if body.len() > MAX_RELAY_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "relay frame too large"));
    }
    let mut frame = (body.len() as u32).to_le_bytes().to_vec();
    frame.extend_from_slice(&body);
    w.write_all(&frame)?;
    w.flush()
}

pub fn read_msg<T: DeserializeOwned, R: Read>(r: &mut R) -> io::Result<T> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let len = u32::from_le_bytes(len) as usize;
    // checked before allocating: a hostile peer must not choose our buffer size
    if len > MAX_RELAY_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "relay frame too large"));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    postcard::from_bytes(&body).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn parse_accepts_lowercase_spaces_and_missing_dash() {
        let a = Code::parse("k7q4 xm2p").unwrap();
        assert_eq!(a, Code::parse("K7Q4-XM2P").unwrap());
        assert_eq!(a, Code::parse("k7q4xm2p").unwrap());
    }

    #[test]
    fn parse_trims_pasted_whitespace_and_newline() {
        assert!(Code::parse("  K7Q4-XM2P \r\n").is_some());
    }

    #[test]
    fn parse_rejects_wrong_length_and_foreign_symbols() {
        assert!(Code::parse("").is_none());
        assert!(Code::parse("K7Q4-XM2").is_none());
        assert!(Code::parse("K7Q4-XM2PA").is_none());
        for bad in ["K7Q4-XM20", "K7Q4-XMO2", "K7Q4-XM12", "K7Q4-XMI2", "K7Q4-XM2!"] {
            assert!(Code::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn display_roundtrips_through_parse() {
        let c = Code::generate();
        let s = c.to_string();
        assert_eq!(s.len(), 9);
        assert_eq!(s.as_bytes()[4], b'-');
        assert_eq!(Code::parse(&s), Some(c));
    }

    #[test]
    fn generate_uses_only_alphabet_and_varies() {
        let mut seen = HashSet::new();
        for _ in 0..200 {
            let c = Code::generate();
            assert!(c.0.iter().all(|b| ALPHABET.contains(b)));
            seen.insert(c);
        }
        assert!(seen.len() >= 190);
    }

    #[test]
    fn read_msg_rejects_oversize_frame() {
        let mut buf = ((MAX_RELAY_FRAME as u32) + 1).to_le_bytes().to_vec();
        buf.extend(vec![0u8; MAX_RELAY_FRAME + 1]);
        let err = read_msg::<ToRelay, _>(&mut buf.as_slice()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn read_msg_rejects_join_with_foreign_code_bytes() {
        let good = ToRelay::Join { protocol: RELAY_PROTOCOL, code: Code::parse("K7Q4-XM2P").unwrap() };
        for bad in [0xFFu8, b'0', b'a'] {
            let mut body = postcard::to_allocvec(&good).unwrap();
            *body.last_mut().unwrap() = bad;
            let mut frame = (body.len() as u32).to_le_bytes().to_vec();
            frame.extend(body);
            let err = read_msg::<ToRelay, _>(&mut frame.as_slice()).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        }
    }

    #[test]
    fn read_msg_reports_truncated_body() {
        let mut frame = 10u32.to_le_bytes().to_vec();
        frame.extend([1, 2, 3]);
        let err = read_msg::<ToRelay, _>(&mut frame.as_slice()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn read_msg_accepts_frame_of_exactly_max_size() {
        let mut body = postcard::to_allocvec(&ToRelay::NewCode).unwrap();
        body.resize(MAX_RELAY_FRAME, 0);
        let mut frame = (body.len() as u32).to_le_bytes().to_vec();
        frame.extend(body);
        assert_eq!(read_msg::<ToRelay, _>(&mut frame.as_slice()).unwrap(), ToRelay::NewCode);
    }

    fn rt<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(m: T) {
        let mut buf = Vec::new();
        write_msg(&mut buf, &m).unwrap();
        assert_eq!(read_msg::<T, _>(&mut buf.as_slice()).unwrap(), m);
    }

    #[test]
    fn msg_roundtrip_for_every_variant() {
        let code = Code::parse("K7Q4-XM2P").unwrap();
        rt(ToRelay::Open { protocol: RELAY_PROTOCOL });
        rt(ToRelay::NewCode);
        rt(ToRelay::Join { protocol: RELAY_PROTOCOL, code });
        rt(ToRelay::Accept { id: u64::MAX });
        rt(FromRelay::Opened { code });
        rt(FromRelay::Incoming { id: 7 });
        rt(FromRelay::Paired);
        for r in [Refusal::BadProtocol, Refusal::NoSuchCode, Refusal::RoomFull, Refusal::TooManyAttempts, Refusal::Busy] {
            rt(FromRelay::Refused(r));
        }
    }
}
