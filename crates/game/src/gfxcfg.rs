//! Graphics settings the player picks (pixel size, shadows, far decor) and ui.cfg, the little `key=value` file beside
//! the saves that keeps them together with the interface size. The renderer applies a change on the next frame.
use alaz_render::{PixelDetail, Quality, ShadowQuality};
use alaz_render::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static CURRENT: Mutex<Option<Quality>> = Mutex::new(None);
static DIRTY: AtomicBool = AtomicBool::new(false);
static PREFS: Mutex<Option<Prefs>> = Mutex::new(None);
static PREFS_DIRTY: AtomicBool = AtomicBool::new(false);

/// What the settings screen sets besides graphics: how loud each kind of sound is, and whether the window fills the
/// screen (borderless).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Prefs {
    pub volumes: crate::audio::Volumes,
    pub fullscreen: bool,
}

/// The settings in `pairs` (ui.cfg lines); anything missing or unreadable keeps its default.
fn parse_prefs(pairs: &[(String, String)]) -> Prefs {
    let mut p = Prefs::default();
    for (k, v) in pairs {
        let num = || v.parse::<f32>().ok().filter(|x| x.is_finite());
        match k.as_str() {
            "volume" => p.volumes.master = num().unwrap_or(p.volumes.master),
            "sfx" => p.volumes.effects = num().unwrap_or(p.volumes.effects),
            "ambience" => p.volumes.ambience = num().unwrap_or(p.volumes.ambience),
            "music" => p.volumes.music = num().unwrap_or(p.volumes.music),
            "fullscreen" => p.fullscreen = v == "1",
            _ => {}
        }
    }
    p.volumes = p.volumes.clamped();
    p
}

/// The whole text of ui.cfg.
fn render_cfg(zoom: Option<f32>, q: Quality, p: Prefs) -> String {
    let mut out = String::new();
    if let Some(z) = zoom {
        out += &format!("zoom={z:.2}\n");
    }
    out += &format!("pixels={}\nshadows={}\ndecor={}\n", q.pixels.key(), q.shadows.key(), q.decor as u8);
    let v = p.volumes;
    out += &format!("volume={:.2}\nsfx={:.2}\nambience={:.2}\nmusic={:.2}\nfullscreen={}\n", v.master, v.effects, v.ambience, v.music, p.fullscreen as u8);
    out
}

/// The sound and window settings saved last time (the defaults when none).
pub fn prefs() -> Prefs {
    let mut cur = PREFS.lock().unwrap();
    *cur.get_or_insert_with(|| parse_prefs(&read_pairs()))
}

/// Chooses new sound and window settings: kept in ui.cfg, and the game loop applies them.
pub fn set_prefs(p: Prefs) {
    let p = Prefs { volumes: p.volumes.clamped(), ..p };
    if p == prefs() {
        return;
    }
    *PREFS.lock().unwrap() = Some(p);
    PREFS_DIRTY.store(true, Ordering::Relaxed);
    save_cfg(None);
}

/// A change waiting to be applied, once.
pub fn take_prefs_change() -> Option<Prefs> {
    PREFS_DIRTY.swap(false, Ordering::Relaxed).then(prefs)
}

fn path() -> std::path::PathBuf {
    crate::saveio::base_dir().join("ui.cfg")
}

fn read_pairs() -> Vec<(String, String)> {
    let text = std::fs::read_to_string(path()).unwrap_or_default();
    text.lines().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect()
}

/// Writes ui.cfg whole: the interface size (`None` keeps the saved one) and the graphics settings.
pub fn save_cfg(zoom: Option<f32>) {
    let pairs = read_pairs();
    let zoom = zoom.or_else(|| pairs.iter().find(|(k, _)| k == "zoom").and_then(|(_, v)| v.parse().ok()));
    let out = render_cfg(zoom, current(), prefs());
    let _ = std::fs::create_dir_all(crate::saveio::base_dir());
    let _ = std::fs::write(path(), out);
}

/// The settings saved last time (the defaults when none).
pub fn current() -> Quality {
    let mut cur = CURRENT.lock().unwrap();
    *cur.get_or_insert_with(|| {
        let mut q = Quality::default();
        for (k, v) in read_pairs() {
            match k.as_str() {
                "pixels" => q.pixels = PixelDetail::parse(&v).unwrap_or(q.pixels),
                "shadows" => q.shadows = ShadowQuality::parse(&v).unwrap_or(q.shadows),
                "decor" => q.decor = v != "0",
                _ => {}
            }
        }
        q
    })
}

/// Chooses new settings: kept in ui.cfg, and the game loop hands them to the renderer.
pub fn set(q: Quality) {
    if q == current() {
        return;
    }
    *CURRENT.lock().unwrap() = Some(q);
    DIRTY.store(true, Ordering::Relaxed);
    save_cfg(None);
}

/// A change waiting for the renderer, once.
pub fn take_change() -> Option<Quality> {
    DIRTY.swap(false, Ordering::Relaxed).then(current)
}

pub const PIXEL_NAMES: [(PixelDetail, &str); 2] = [(PixelDetail::Fine, "İnce"), (PixelDetail::Coarse, "Kaba")];
pub const SHADOW_NAMES: [(ShadowQuality, &str); 3] = [(ShadowQuality::High, "Yüksek"), (ShadowQuality::Low, "Düşük"), (ShadowQuality::Off, "Kapalı")];
pub const DECOR_NAMES: [(bool, &str); 2] = [(true, "Açık"), (false, "Kapalı")];

/// The settings panel for the help tab, beside the interface size.
pub fn controls(ui: &mut egui::Ui) {
    let mut q = current();
    ui.label(egui::RichText::new("Grafik").strong());
    let row = |ui: &mut egui::Ui, label: &str, hint: &str| {
        ui.label(label).on_hover_text(hint);
    };
    ui.horizontal(|ui| {
        row(ui, "Piksel inceliği", "Kaba: daha az piksel, zayıf ekran kartları için");
        for (v, name) in PIXEL_NAMES {
            if ui.add(egui::Button::new(name).selected(q.pixels == v)).clicked() {
                q.pixels = v;
            }
        }
    });
    ui.horizontal(|ui| {
        row(ui, "Gölge kalitesi", "Düşük: yarı çözünürlük. Kapalı: gölge hiç çizilmez");
        for (v, name) in SHADOW_NAMES {
            if ui.add(egui::Button::new(name).selected(q.shadows == v)).clicked() {
                q.shadows = v;
            }
        }
    });
    ui.horizontal(|ui| {
        row(ui, "Uzak çim ve süs", "Çim, çiçek ve çakıl demetleri");
        for (v, name) in DECOR_NAMES {
            if ui.add(egui::Button::new(name).selected(q.decor == v)).clicked() {
                q.decor = v;
            }
        }
    });
    set(q);
}

#[cfg(test)]
mod prefs_tests {
    use super::*;
    use crate::audio::Volumes;

    fn pairs(text: &str) -> Vec<(String, String)> {
        text.lines().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect()
    }

    #[test]
    fn what_is_written_is_read_back() {
        let p = Prefs { volumes: Volumes { master: 0.4, effects: 0.8, ambience: 0.0, music: 0.65 }, fullscreen: true };
        let text = render_cfg(Some(1.1), Quality::default(), p);
        assert_eq!(parse_prefs(&pairs(&text)), p);
        assert!(text.contains("zoom=1.10") && text.contains("pixels="), "the graphics lines are still written");
    }

    #[test]
    fn a_file_without_the_new_keys_gets_the_defaults() {
        assert_eq!(parse_prefs(&pairs("zoom=0.90\npixels=fine\n")), Prefs::default());
        assert!(!Prefs::default().fullscreen);
    }

    #[test]
    fn bad_numbers_cannot_break_the_volume() {
        let p = parse_prefs(&pairs("volume=banana\nsfx=9\nmusic=-3\nambience=NaN\n"));
        assert_eq!(p.volumes.master, Volumes::default().master);
        assert_eq!(p.volumes.effects, 1.0);
        assert_eq!(p.volumes.music, 0.0);
        assert_eq!(p.volumes.ambience, Volumes::default().ambience);
    }

    #[test]
    fn a_graphics_change_does_not_wipe_the_sound_settings() {
        let p = Prefs { volumes: Volumes { master: 0.3, ..Volumes::default() }, fullscreen: true };
        let mut q = Quality::default();
        q.decor = !q.decor;
        let text = render_cfg(None, q, p);
        assert_eq!(parse_prefs(&pairs(&text)), p);
    }
}
