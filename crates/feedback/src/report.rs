//! What a report holds. The sizes and rules mirror the server (`web/feedback/src/lib/report.ts`).

use serde::{Deserialize, Serialize};

pub const SCREENSHOT_MAX: usize = 2_500_000;
pub const LOG_MAX: usize = 300_000;
pub const SAVE_MAX: usize = 1_000_000;
pub const FIELDS_MAX: usize = 20_000;
pub const BODY_MAX: usize = 4_000_000;
pub const TEXT_MAX: usize = 4000;
pub const NAME_MAX: usize = 40;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Bug,
    Idea,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Solo,
    Host,
    Guest,
    Menu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Part {
    Screenshot,
    Log,
    Save,
}

impl Part {
    pub const ALL: [Part; 3] = [Part::Screenshot, Part::Log, Part::Save];

    /// The name of the upload field.
    pub fn name(self) -> &'static str {
        match self {
            Part::Screenshot => "screenshot",
            Part::Log => "log",
            Part::Save => "save",
        }
    }

    pub fn max(self) -> usize {
        match self {
            Part::Screenshot => SCREENSHOT_MAX,
            Part::Log => LOG_MAX,
            Part::Save => SAVE_MAX,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fields {
    pub id: String,
    pub kind: Kind,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub game_version: String,
    pub os: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zoom: Option<f32>,
    /// A string: the server's numbers cannot hold every u64.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub day: Option<u32>,
    pub mode: Mode,
    pub crash: bool,
    pub omitted: Vec<Part>,
}

impl Fields {
    pub fn json(&self) -> String {
        serde_json::to_string(self).expect("plain data always serializes")
    }
}

/// A report as the game fills it in, before the limits are applied.
pub struct Draft {
    pub kind: Kind,
    pub text: String,
    pub name: String,
    pub crash: bool,
    pub game_version: String,
    pub os: String,
    pub gpu: Option<String>,
    pub window: Option<String>,
    pub zoom: Option<f32>,
    pub seed: Option<u64>,
    pub world_size: Option<u32>,
    pub day: Option<u32>,
    pub mode: Mode,
    pub screenshot: Option<Vec<u8>>,
    pub log: Option<Vec<u8>>,
    pub save: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub fields: Fields,
    pub files: Vec<(Part, Vec<u8>)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    Empty,
    TooLong,
}

fn clean(text: &str) -> String {
    text.chars().filter(|&c| c == '\n' || c == '\t' || !c.is_control()).collect()
}

fn cut(text: &str, max: usize) -> String {
    clean(text).chars().take(max).collect()
}

impl Draft {
    pub fn build(self, id: String) -> Result<Report, Problem> {
        let text = clean(&self.text).trim().to_string();
        if text.is_empty() {
            return Err(Problem::Empty);
        }
        if text.chars().count() > TEXT_MAX {
            return Err(Problem::TooLong);
        }
        let name = cut(self.name.trim(), NAME_MAX);
        let mut files = Vec::new();
        let mut omitted = Vec::new();
        for (part, bytes) in Part::ALL.into_iter().zip([self.screenshot, self.log, self.save]) {
            match bytes {
                Some(b) if b.len() > part.max() => omitted.push(part),
                Some(b) => files.push((part, b)),
                None => {}
            }
        }
        let fields = Fields {
            id,
            kind: self.kind,
            text,
            name: (!name.is_empty()).then_some(name),
            game_version: self.game_version,
            os: cut(&self.os, 64),
            gpu: self.gpu.map(|g| cut(&g, 128)),
            window: self.window.map(|w| cut(&w, 32)),
            zoom: self.zoom.filter(|z| (0.2..=4.0).contains(z)),
            seed: self.seed.map(|s| s.to_string()),
            world_size: self.world_size,
            day: self.day,
            mode: self.mode,
            crash: self.crash,
            omitted,
        };
        Ok(Report { fields, files })
    }
}

/// 32 hex digits from the operating system's random source; the server uses it to store a report only once.
pub fn new_id() -> String {
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        // never seen in practice, and no reason to end the game: the clock still tells two reports apart
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        bytes = now.to_le_bytes();
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn draft() -> Draft {
        Draft {
            kind: Kind::Bug,
            text: "Ağaç yok oldu".into(),
            name: String::new(),
            crash: false,
            game_version: "0.6.0".into(),
            os: "linux x86_64".into(),
            gpu: None,
            window: None,
            zoom: None,
            seed: None,
            world_size: None,
            day: None,
            mode: Mode::Solo,
            screenshot: None,
            log: None,
            save: None,
        }
    }

    fn id() -> String {
        "a".repeat(32)
    }

    #[test]
    fn empty_and_too_long_text_are_refused() {
        assert_eq!(Draft { text: " \n\u{1b} ".into(), ..draft() }.build(id()).unwrap_err(), Problem::Empty);
        assert_eq!(Draft { text: "x".repeat(4001), ..draft() }.build(id()).unwrap_err(), Problem::TooLong);
    }

    #[test]
    fn the_longest_text_still_fits_the_fields() {
        let r = Draft { text: "🪨".repeat(4000), name: "🪨".repeat(40), gpu: Some("g".repeat(500)), window: Some("w".repeat(99)), os: "o".repeat(99), seed: Some(u64::MAX), ..draft() }.build(id()).unwrap();
        assert!(r.fields.json().len() <= FIELDS_MAX);
        assert_eq!(r.fields.gpu.as_ref().unwrap().chars().count(), 128);
        assert_eq!(r.fields.window.as_ref().unwrap().len(), 32);
        assert_eq!(r.fields.os.len(), 64);
        assert_eq!(r.fields.seed.as_deref(), Some("18446744073709551615"));
    }

    #[test]
    fn text_is_cleaned_and_trimmed() {
        let r = Draft { text: "  a\u{0}b\u{1b}\nc\td\r  ".into(), name: " Ece\u{7} ".into(), ..draft() }.build(id()).unwrap();
        assert_eq!(r.fields.text, "ab\nc\td");
        assert_eq!(r.fields.name.as_deref(), Some("Ece"));
        let long = Draft { name: "x".repeat(60), ..draft() }.build(id()).unwrap();
        assert_eq!(long.fields.name.unwrap().len(), NAME_MAX);
    }

    #[test]
    fn the_json_names_match_the_server_and_absent_values_are_left_out() {
        let json = draft().build(id()).unwrap().fields.json();
        for key in ["\"id\"", "\"kind\":\"bug\"", "\"game_version\"", "\"os\"", "\"mode\":\"solo\"", "\"crash\":false", "\"omitted\":[]"] {
            assert!(json.contains(key), "{key} in {json}");
        }
        for key in ["name", "gpu", "window", "zoom", "seed", "world_size", "day"] {
            assert!(!json.contains(&format!("\"{key}\"")), "{key} in {json}");
        }
        let full = Draft { world_size: Some(256), day: Some(3), zoom: Some(1.5), kind: Kind::Idea, mode: Mode::Guest, ..draft() }.build(id()).unwrap().fields.json();
        for key in ["\"world_size\":256", "\"day\":3", "\"zoom\":1.5", "\"kind\":\"idea\"", "\"mode\":\"guest\""] {
            assert!(full.contains(key), "{key} in {full}");
        }
    }

    #[test]
    fn a_zoom_the_server_would_refuse_is_left_out() {
        for z in [0.1, 4.5, f32::NAN] {
            assert_eq!(Draft { zoom: Some(z), ..draft() }.build(id()).unwrap().fields.zoom, None);
        }
    }

    #[test]
    fn a_file_that_is_too_large_is_named_instead_of_sent() {
        let r = Draft { screenshot: Some(vec![0; SCREENSHOT_MAX + 1]), log: Some(b"ok".to_vec()), save: Some(vec![0; SAVE_MAX + 1]), ..draft() }.build(id()).unwrap();
        assert_eq!(r.fields.omitted, vec![Part::Screenshot, Part::Save]);
        assert_eq!(r.files, vec![(Part::Log, b"ok".to_vec())]);
        assert!(SCREENSHOT_MAX + LOG_MAX + SAVE_MAX + FIELDS_MAX < BODY_MAX);
        assert_eq!([Part::Screenshot, Part::Log, Part::Save].map(Part::name), ["screenshot", "log", "save"]);
    }

    #[test]
    fn ids_are_random_hex() {
        let (a, b) = (new_id(), new_id());
        assert_ne!(a, b);
        assert!(a.len() == 32 && a.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)), "{a}");
    }
}
