//! Graphics settings the player picks (pixel size, shadows, far decor) and ui.cfg, the little `key=value` file beside
//! the saves that keeps them together with the interface size. The renderer applies a change on the next frame.
use alaz_render::{PixelDetail, Quality, ShadowQuality};
use alaz_render::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static CURRENT: Mutex<Option<Quality>> = Mutex::new(None);
static DIRTY: AtomicBool = AtomicBool::new(false);

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
    let q = current();
    let mut out = String::new();
    if let Some(z) = zoom {
        out += &format!("zoom={z:.2}\n");
    }
    out += &format!("pixels={}\nshadows={}\ndecor={}\n", q.pixels.key(), q.shadows.key(), q.decor as u8);
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
