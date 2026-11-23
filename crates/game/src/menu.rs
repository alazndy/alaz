//! Start screen and pause menu: pick a save slot, continue, start a new world, quit.

use crate::keys::{bindable, Action, Keybinds};
use crate::ui_art::{self, Glyph};
use alaz_render::egui::{self, epaint::Mesh, pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui};
use winit::keyboard::KeyCode;

/// Everything needed to create a world, as picked on the menu.
pub struct NewWorld {
    pub slot: u8,
    pub seed: Option<u64>,
    pub size: i32,
    pub hardcore: bool,
    pub settings: alaz_sim::world::WorldSettings,
}

pub use alaz_net::discovery::Found;

pub enum MenuAction {
    Continue(u8),
    New { slot: u8, seed: Option<u64>, size: i32, hardcore: bool, settings: alaz_sim::world::WorldSettings },
    /// `new` is None when the slot already holds a save to load.
    Host { slot: u8, port: u16, new: Option<NewWorld> },
    /// Raw text: "192.168.1.5" or "192.168.1.5:7777".
    Join(String),
    /// Open the hosted world to the internet. Carries the host settings because the world may not be hosted yet.
    HostOnline { slot: u8, port: u16, new: Option<NewWorld> },
    /// Raw text of the code field; `Client::connect_code` normalizes it.
    JoinCode(String),
    CloseOnline,
    NewCode,
    Delete(u8),
    Resume,
    SaveQuit,
    Quit,
}

type Info = Option<Result<alaz_sim::SaveInfo, String>>;

pub struct Menu {
    /// Opened with Esc during play (has Resume) rather than at startup.
    pub paused: bool,
    pub slots: [Info; crate::saveio::SLOTS as usize],
    slot: u8,
    seed_text: String,
    size_idx: usize,
    difficulty_idx: usize,
    season_idx: usize,
    hardcore: bool,
    confirm_delete: Option<u8>,
    /// The key bindings screen is open.
    keys_screen: bool,
    /// The graphics settings screen is open.
    gfx_screen: bool,
    /// The version screen is open.
    update_screen: bool,
    pub updater: crate::update_ui::UpdateState,
    /// Waiting for a key press to bind to (action, slot).
    capturing: Option<(Action, usize)>,
    pub error: Option<String>,
    /// A join attempt is running in the background.
    pub connecting: bool,
    pub mp_error: Option<String>,
    /// Shown in the pause card while hosting.
    pub net_line: Option<String>,
    pub found: Vec<Found>,
    /// The multiplayer screen is open; the app runs LAN discovery only while it is.
    pub mp_screen: bool,
    /// This game hosts a world right now.
    pub hosting: bool,
    /// The hosted world also accepts LAN connections (not only the relay code).
    pub lan_open: bool,
    /// The relay link of the hosted world, if it ever was opened.
    pub relay: Option<alaz_net::RelayStatus>,
    pub relay_players: usize,
    port_text: String,
    addr_text: String,
    code_text: String,
    /// When the screen first drew, for the intro animation.
    born: Option<f64>,
}

const DIFFICULTIES: [&str; 3] = ["Affedici", "Gerçekçi", "Yaratıcı"];
const SEASONS: [(&str, u8); 3] = [("Mevsim 3 gün", 3), ("Mevsim 6 gün", 6), ("Mevsim 9 gün", 9)];

/// A row of choices with a sliding highlight, like the map size picker.
fn segmented(ui: &mut Ui, id: &str, names: &[&str], idx: &mut usize) {
    let (sr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
    let seg_w = sr.width() / names.len() as f32;
    let slide = ui.ctx().animate_value_with_time(egui::Id::NULL.with((id, "slide")), *idx as f32, 0.18);
    ui.painter().rect_filled(sr, 0.0, Color32::from_black_alpha(110));
    ui.painter().rect_filled(Rect::from_min_size(pos2(sr.left() + slide * seg_w, sr.top()), vec2(seg_w, sr.height())).shrink(2.0), 0.0, Color32::from_rgb(150, 84, 34));
    for (k, name) in names.iter().enumerate() {
        let cell = Rect::from_min_size(pos2(sr.left() + k as f32 * seg_w, sr.top()), vec2(seg_w, sr.height()));
        let resp = ui.interact(cell, egui::Id::NULL.with((id, k)), Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, *name));
        if resp.clicked() {
            *idx = k;
        }
        let on = *idx == k;
        ui.painter().text(cell.center(), Align2::CENTER_CENTER, *name, FontId::proportional(22.0), if on { Color32::WHITE } else if resp.hovered() { INK } else { Color32::from_rgb(170, 154, 126) });
    }
    ui.painter().rect_stroke(crate::pxui::snap_rect(sr), 0.0, Stroke::new(crate::pxui::unit(), LINE), StrokeKind::Inside);
}

/// World sizes: the short name on the selector, the side in metres, and a line on what that world is like.
const SIZES: [(&str, i32); 6] = [("128 m", 128), ("256 m", 256), ("1 km", 1024), ("4 km", 4096), ("32 km", 32768), ("262 km", alaz_sim::map::MAX_MAP_SIDE)];
const SIZE_NOTES: [&str; 6] = [
    "Küçük: hızlı bir deneme için.",
    "Orta: bir vadi, bir kıyı.",
    "Büyük: nehirler, göller, dağlar.",
    "Dev: bir ülke kadar.",
    "Devasa: kıtalar ve okyanuslar, yürüdükçe açılır.",
    "Sınırsız: baştan başa yürümek iki gün sürer.",
];

impl Menu {
    pub fn new(paused: bool) -> Menu {
        let mut m = Menu {
            paused,
            slots: Default::default(),
            slot: crate::saveio::slot(),
            seed_text: String::new(),
            size_idx: 1,
            difficulty_idx: 0,
            season_idx: 1,
            hardcore: false,
            confirm_delete: None,
            keys_screen: std::env::var_os("ALAZ_DEMO_KEYS").is_some(),
            gfx_screen: std::env::var_os("ALAZ_DEMO_GFXSCREEN").is_some(),
            update_screen: false,
            updater: crate::update_ui::UpdateState::live(),
            capturing: None,
            error: None,
            connecting: false,
            mp_error: None,
            net_line: None,
            found: Vec::new(),
            mp_screen: std::env::var_os("ALAZ_DEMO_MPSCREEN").is_some(),
            hosting: false,
            lan_open: false,
            relay: None,
            relay_players: 0,
            port_text: "7777".to_string(),
            addr_text: String::new(),
            code_text: String::new(),
            born: None,
        };
        m.refresh();
        m
    }

    /// Opens the version screen directly (the screenshot tool has no click to do it).
    pub(crate) fn open_update_screen(&mut self) {
        self.update_screen = true;
    }

    pub fn refresh(&mut self) {
        for i in 0..self.slots.len() {
            self.slots[i] = crate::saveio::slot_info(i as u8 + 1);
        }
    }

    /// A key press while the menu is open. Returns true when the menu used it (binding capture).
    pub fn on_key(&mut self, code: KeyCode, binds: &mut Keybinds) -> bool {
        let Some((action, slot)) = self.capturing else { return false };
        if code == KeyCode::Escape {
            self.capturing = None;
        } else if code == KeyCode::Delete || code == KeyCode::Backspace {
            binds.set(action, slot, None);
            binds.save();
            self.capturing = None;
        } else if bindable(code) {
            binds.set(action, slot, Some(code));
            binds.save();
            self.capturing = None;
        }
        true
    }

    pub fn capturing(&self) -> bool {
        self.capturing.is_some()
    }

    fn new_world(&self) -> NewWorld {
        let difficulty = match self.difficulty_idx {
            0 => alaz_sim::world::Difficulty::Forgiving,
            1 => alaz_sim::world::Difficulty::Realistic,
            _ => alaz_sim::world::Difficulty::Creative,
        };
        NewWorld {
            slot: self.slot,
            seed: self.seed_text.trim().parse::<u64>().ok(),
            size: SIZES[self.size_idx].1,
            hardcore: self.hardcore,
            settings: alaz_sim::world::WorldSettings { difficulty, season_days: SEASONS[self.season_idx].1 },
        }
    }

}

// ───────────────────────── palette and small helpers ─────────────────────────

const GOLD: Color32 = Color32::from_rgb(244, 208, 120);
const EMBER: Color32 = Color32::from_rgb(232, 118, 44);
const INK: Color32 = Color32::from_rgb(238, 228, 208);
const LINE: Color32 = Color32::from_rgb(96, 78, 54);

fn ease(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    1.0 - (1.0 - x).powi(3)
}

/// Cheap deterministic noise in 0..1 for stars, embers and trees.
fn h01(n: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9E37_79B1);
    x ^= x >> 15;
    x = x.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 13;
    (x & 0xffff) as f32 / 65535.0
}

fn lerp_c(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t.clamp(0.0, 1.0)) as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

fn quad(m: &mut Mesh, r: Rect, tl: Color32, tr: Color32, br: Color32, bl: Color32) {
    let i = m.vertices.len() as u32;
    for (p, c) in [(r.left_top(), tl), (r.right_top(), tr), (r.right_bottom(), br), (r.left_bottom(), bl)] {
        m.colored_vertex(p, c);
    }
    m.add_triangle(i, i + 1, i + 2);
    m.add_triangle(i, i + 2, i + 3);
}

fn ridge_y(x: f32, h: f32, layer: usize, t: f32) -> f32 {
    let l = layer as f32;
    let xx = x + (3.0 + 4.0 * l) * t;
    let base = h * (0.60 + 0.09 * l);
    let amp = h * (0.12 - 0.02 * l);
    let a = (xx * 0.0028 * (1.0 + 0.4 * l) + l * 2.1).sin();
    let b = (xx * 0.0071 + l * 0.9).sin();
    let c = (xx * 0.019 + l * 4.0).sin();
    base - amp * (0.5 * a + 0.35 * (1.0 - b.abs()) + 0.15 * c)
}

/// The whole start-screen backdrop: dusk sky, stars, moon, three ridges of mountains, pines,
/// a campfire with rising embers and a hunched figure beside it. Everything moves a little.
fn scene(p: &Painter, screen: Rect, t: f32, fire_x: f32) {
    let (w, h) = (screen.width(), screen.height());
    let sc = (h / 900.0).clamp(0.6, 1.5);
    let o = screen.min.to_vec2();
    let mut sky = Mesh::default();
    let stops = [
        (0.0, Color32::from_rgb(6, 8, 20)),
        (0.45, Color32::from_rgb(26, 20, 42)),
        (0.72, Color32::from_rgb(92, 46, 44)),
        (1.0, Color32::from_rgb(150, 76, 40)),
    ];
    for k in 0..stops.len() - 1 {
        let (a, ca) = stops[k];
        let (b, cb) = stops[k + 1];
        quad(&mut sky, Rect::from_min_max(pos2(screen.left(), screen.top() + a * h), pos2(screen.right(), screen.top() + b * h)), ca, ca, cb, cb);
    }
    p.add(Shape::mesh(sky));
    // stars
    for i in 0..120u32 {
        let x = h01(i * 3) * w;
        let y = h01(i * 3 + 1) * h * 0.5;
        let tw = 0.5 + 0.5 * (t * (0.8 + h01(i * 3 + 2) * 2.2) + i as f32).sin();
        let a = ((60.0 + tw * 160.0) * (1.0 - y / (h * 0.55))) as u8;
        p.circle_filled(pos2(x, y) + o, 0.6 + h01(i * 7) * 1.1, Color32::from_rgba_unmultiplied(255, 244, 220, a));
    }
    // moon with a soft halo
    let mc = pos2(w * 0.52, h * 0.17) + o;
    let mr = h * 0.045;
    for k in (1..6).rev() {
        p.circle_filled(mc, mr * (1.0 + k as f32 * 0.55), Color32::from_rgba_unmultiplied(220, 220, 255, 7));
    }
    p.circle_filled(mc, mr, Color32::from_rgb(234, 230, 210));
    for (dx, dy, r) in [(-0.3, -0.2, 0.22), (0.25, 0.1, 0.16), (-0.1, 0.35, 0.12)] {
        p.circle_filled(mc + vec2(dx * mr, dy * mr), r * mr, Color32::from_rgb(206, 202, 186));
    }
    // ridges, far to near
    let cols = [Color32::from_rgb(52, 38, 60), Color32::from_rgb(30, 22, 38), Color32::from_rgb(11, 10, 16)];
    for layer in 0..3 {
        let mut m = Mesh::default();
        let mut x = 0.0;
        let top_c = cols[layer];
        let bot_c = lerp_c(cols[layer], Color32::BLACK, 0.5);
        while x < w {
            let nx = (x + 8.0).min(w);
            let y1 = ridge_y(x, h, layer, t);
            let y2 = ridge_y(nx, h, layer, t);
            let y = y1.min(y2);
            quad(&mut m, Rect::from_min_max(pos2(x, y) + o, pos2(nx, h) + o), top_c, top_c, bot_c, bot_c);
            // the strip's top edge follows the slope: patch the wedge between the two heights
            if (y1 - y2).abs() > 0.5 {
                let i = m.vertices.len() as u32;
                m.colored_vertex(pos2(x, y1) + o, top_c);
                m.colored_vertex(pos2(nx, y2) + o, top_c);
                m.colored_vertex(pos2(if y1 > y2 { x } else { nx }, y) + o, top_c);
                m.add_triangle(i, i + 1, i + 2);
            }
            x = nx;
        }
        p.add(Shape::mesh(m));
    }
    // pines along the near ridge
    let mut x = -10.0;
    let mut i = 0u32;
    while x < w + 20.0 {
        if h01(i + 900) > 0.3 {
            let base = ridge_y(x, h, 2, t) + 6.0;
            let th = (22.0 + h01(i + 500) * 46.0) * sc;
            let sway = (t * 0.9 + i as f32).sin() * 1.6;
            for (k, wd) in [(0.0, 0.5), (0.3, 0.38), (0.58, 0.26)] {
                let by = base - th * k;
                let ty = base - th * (k + 0.5);
                let half = th * wd * 0.5;
                p.add(Shape::convex_polygon(
                    vec![pos2(x - half, by) + o, pos2(x + half, by) + o, pos2(x + sway * (k + 0.6), ty) + o],
                    cols[2],
                    Stroke::NONE,
                ));
            }
        }
        x += 22.0 * sc;
        i += 1;
    }
    // the campfire
    let gy = ridge_y(fire_x, h, 2, t) + 10.0 * sc + o.y;
    let fx = fire_x + o.x;
    let flick = 0.85 + 0.15 * (t * 11.0).sin() * (t * 7.3).cos();
    for k in 0..14 {
        let r = (40.0 + k as f32 * 20.0) * sc * flick;
        p.circle_filled(pos2(fx, gy - 22.0 * sc), r, Color32::from_rgba_unmultiplied(255, 130, 40, 5));
    }
    // ground glow
    p.add(Shape::ellipse_filled(pos2(fx, gy + 2.0), vec2(150.0 * sc * flick, 16.0 * sc), Color32::from_rgba_unmultiplied(255, 150, 60, 36)));
    p.line_segment([pos2(fx - 26.0 * sc, gy + 2.0), pos2(fx + 20.0 * sc, gy - 8.0 * sc)], Stroke::new(7.0 * sc, Color32::from_rgb(70, 44, 26)));
    p.line_segment([pos2(fx + 26.0 * sc, gy + 2.0), pos2(fx - 20.0 * sc, gy - 8.0 * sc)], Stroke::new(7.0 * sc, Color32::from_rgb(56, 34, 20)));
    for (scale, col, ph) in [(1.0, Color32::from_rgb(226, 84, 28), 0.0), (0.74, Color32::from_rgb(250, 156, 50), 1.7), (0.46, Color32::from_rgb(255, 232, 150), 3.1)] {
        for tongue in 0..3 {
            let off = (tongue as f32 - 1.0) * 13.0 * sc * scale;
            let hgt = (50.0 + 22.0 * (1.0 - (tongue as f32 - 1.0).abs())) * sc * scale;
            let wob = (t * (9.0 + tongue as f32 * 2.3) + ph + tongue as f32).sin();
            let wob2 = (t * 13.0 + ph * 2.0 + tongue as f32 * 1.7).sin();
            let tip = pos2(fx + off * 0.6 + wob * 6.0 * sc, gy - hgt * (0.9 + 0.1 * wob2) - 4.0 * sc);
            let hw = 13.0 * sc * scale;
            p.add(Shape::convex_polygon(
                vec![
                    pos2(fx + off - hw, gy - 4.0 * sc),
                    pos2(fx + off - hw * 0.9 + wob * 2.0, gy - hgt * 0.45),
                    tip,
                    pos2(fx + off + hw * 0.9 + wob * 2.0, gy - hgt * 0.45),
                    pos2(fx + off + hw, gy - 4.0 * sc),
                ],
                col,
                Stroke::NONE,
            ));
        }
    }
    // embers drifting up and out
    for n in 0..44u32 {
        let life = (t * (0.18 + h01(n) * 0.3) + h01(n + 77)).fract();
        let x = fx + (h01(n + 5) - 0.5) * 50.0 * sc + (t * 1.3 + n as f32).sin() * 26.0 * life * sc;
        let y = gy - 30.0 * sc - life * h * (0.25 + 0.3 * h01(n + 9));
        let a = ((1.0 - life) * 230.0) as u8;
        p.circle_filled(pos2(x, y), (1.0 + h01(n + 31) * 1.6) * sc * (1.0 - life * 0.6), Color32::from_rgba_unmultiplied(255, 176 - (life * 70.0) as u8, 60, a));
    }
    // a hunched figure warming its hands
    let ax = fx + 92.0 * sc;
    let dark = Color32::from_rgb(8, 6, 8);
    let k = sc;
    p.add(Shape::convex_polygon(
        vec![pos2(ax - 22.0 * k, gy + 2.0), pos2(ax - 18.0 * k, gy - 30.0 * k), pos2(ax - 6.0 * k, gy - 44.0 * k), pos2(ax + 10.0 * k, gy - 38.0 * k), pos2(ax + 16.0 * k, gy + 2.0)],
        dark,
        Stroke::NONE,
    ));
    p.circle_filled(pos2(ax - 16.0 * k, gy - 46.0 * k), 9.5 * k, dark);
    p.line_segment([pos2(ax - 12.0 * k, gy - 30.0 * k), pos2(ax - 38.0 * k, gy - 20.0 * k)], Stroke::new(5.5 * k, dark));
    // firelight on the figure's near edge
    p.line_segment([pos2(ax - 25.0 * k, gy - 8.0 * k), pos2(ax - 19.0 * k, gy - 30.0 * k)], Stroke::new(1.6, Color32::from_rgba_unmultiplied(255, 150, 60, (140.0 * flick) as u8)));
    p.circle_stroke(pos2(ax - 16.0 * k, gy - 46.0 * k), 9.5 * k, Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 150, 60, 70)));
    // vignette: darken the corners so the eye goes to the middle
    let mut v = Mesh::default();
    let edge = Color32::from_black_alpha(150);
    let none = Color32::from_black_alpha(0);
    let band = h * 0.22;
    quad(&mut v, Rect::from_min_max(screen.min, pos2(screen.right(), screen.top() + band)), edge, edge, none, none);
    quad(&mut v, Rect::from_min_max(pos2(screen.left(), screen.bottom() - band), screen.max), none, none, edge, edge);
    p.add(Shape::mesh(v));
}

/// The big extruded title, letters dropping in one after another, sparks between them. A logo picture
/// (`assets/ui/logo.png`) takes its place, fading in.
fn title(p: &Painter, at: Pos2, size: f32, intro: f32, t: f32) {
    if crate::art::any()
        && let Some((tex, dim)) = crate::art::texture(p.ctx(), "ui/logo.png") {
            let a = (ease(intro / 0.8) * 255.0) as u8;
            let h = size * 1.5;
            let r = Rect::from_min_size(at, vec2(h * dim[0] as f32 / dim[1] as f32, h));
            p.image(tex.id(), r, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::from_white_alpha(a));
            return;
        }
    let letters = ["A", "L", "A", "Z"];
    let mut x = at.x;
    for (i, l) in letters.iter().enumerate() {
        let e = ease((intro - 0.12 * i as f32) / 0.6);
        let galley = p.layout_no_wrap(l.to_string(), FontId::proportional(size), Color32::WHITE);
        let y = at.y + (1.0 - e) * -40.0;
        let a = (e * 255.0) as u8;
        for d in (1..=7).rev() {
            let c = lerp_c(Color32::from_rgb(40, 20, 12), Color32::from_rgb(120, 60, 24), 1.0 - d as f32 / 7.0);
            p.galley_with_override_text_color(pos2(x + d as f32 * 0.9, y + d as f32 * 1.1), galley.clone(), Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a));
        }
        let lit = lerp_c(GOLD, Color32::WHITE, 0.25 * (0.5 + 0.5 * (t * 2.0 + i as f32).sin()));
        p.galley_with_override_text_color(pos2(x, y), galley.clone(), Color32::from_rgba_unmultiplied(lit.r(), lit.g(), lit.b(), a));
        x += galley.size().x * 0.94;
        if i + 1 < letters.len() {
            let pulse = 0.6 + 0.4 * (t * 3.0 + i as f32 * 1.4).sin();
            let c = pos2(x + size * 0.05, at.y + size * 0.86);
            p.circle_filled(c, size * 0.2 * pulse, Color32::from_rgba_unmultiplied(255, 140, 40, (30.0 * e) as u8));
            p.circle_filled(c, size * 0.055, Color32::from_rgba_unmultiplied(255, 190, 80, a));
            x += size * 0.2;
        }
    }
    // tagline types itself out under a line that draws across
    let ln = ease((intro - 0.7) / 0.6);
    let y = at.y + size * 1.2;
    p.line_segment([pos2(at.x, y), pos2(at.x + (x - at.x) * ln, y)], Stroke::new(2.0, EMBER));
    p.circle_filled(pos2(at.x + (x - at.x) * ln, y), 3.0, GOLD);
    let tag = "Ape Learns Another Zing";
    let n = (((intro - 1.0) * 28.0).max(0.0) as usize).min(tag.chars().count());
    let shown: String = tag.chars().take(n).collect();
    let cursor = if n < tag.chars().count() || (t * 2.0) as i32 % 2 == 0 { "|" } else { "" };
    p.text(pos2(at.x + 2.0, y + 12.0), Align2::LEFT_TOP, format!("{shown}{cursor}"), FontId::proportional((size * 0.22).max(15.0)), Color32::from_rgb(214, 196, 160));
}

#[derive(Clone, Copy, PartialEq)]
enum Look {
    Primary,
    Plain,
    Danger,
    Cap { waiting: bool },
}

/// One painted button for the whole menu: hover slides a warm fill across, press sinks it.
fn button(ui: &mut Ui, text: &str, size: egui::Vec2, look: Look) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, text));
    let hov = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.14);
    let t = ui.input(|i| i.time) as f32;
    let down = resp.is_pointer_button_down_on();
    let p = ui.painter();
    let r = rect.translate(vec2(0.0, if down { crate::pxui::unit() } else { 0.0 }));
    match look {
        Look::Primary => {
            // a flat face with a lit top and a shaded bottom, no gradient
            let u = crate::pxui::unit();
            let r = crate::pxui::snap_rect(r);
            let l = lerp_c(Color32::from_rgb(176, 92, 34), Color32::from_rgb(222, 120, 44), hov);
            let rr = lerp_c(Color32::from_rgb(120, 60, 28), Color32::from_rgb(176, 90, 36), hov);
            p.rect_filled(r, 0.0, lerp_c(l, rr, 0.35));
            p.rect_filled(Rect::from_min_max(r.min, pos2(r.right(), r.top() + 2.0 * u)), 0.0, l);
            p.rect_filled(Rect::from_min_max(pos2(r.left(), r.bottom() - 2.0 * u), r.max), 0.0, rr);
            // a shine band sweeping across now and then
            let sweep = ((t * 0.45).fract() * 1.6 - 0.3) * r.width();
            let band = Rect::from_min_max(pos2(r.left() + sweep, r.top()), pos2(r.left() + sweep + 26.0, r.bottom())).intersect(r);
            if band.width() > 0.0 {
                p.rect_filled(band, 0.0, Color32::from_white_alpha(30));
            }
            p.rect_stroke(crate::pxui::snap_rect(r), 0.0, Stroke::new(crate::pxui::unit(), GOLD), StrokeKind::Inside);
            let shift = hov * 6.0;
            p.text(pos2(r.left() + 16.0 + shift, r.center().y), Align2::LEFT_CENTER, text, FontId::proportional(22.0), Color32::from_rgb(255, 244, 220));
            if r.width() > 140.0 {
                ui_art::glyph(p, pos2(r.right() - 20.0 - (1.0 - hov) * 4.0, r.center().y), 16.0, Glyph::Flame, Color32::from_rgb(255, 226, 150));
            }
        }
        Look::Plain | Look::Danger => {
            let acc = if look == Look::Danger { Color32::from_rgb(220, 90, 80) } else { EMBER };
            p.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(0, 0, 0, 90));
            // the fill grows out of the left edge
            let fw = r.width() * hov;
            p.rect_filled(Rect::from_min_size(r.min, vec2(fw, r.height())), 0.0, Color32::from_rgba_unmultiplied(acc.r(), acc.g(), acc.b(), 70));
            p.rect_filled(Rect::from_min_size(r.min, vec2(3.0, r.height())), 0.0, acc.gamma_multiply(0.5 + 0.5 * hov));
            p.rect_stroke(crate::pxui::snap_rect(r), 0.0, Stroke::new(crate::pxui::unit(), lerp_c(LINE, acc, hov)), StrokeKind::Inside);
            let shift = hov * 5.0;
            p.text(pos2(r.left() + 12.0 + shift, r.center().y), Align2::LEFT_CENTER, text, FontId::proportional(22.0), lerp_c(INK, Color32::WHITE, hov));
        }
        Look::Cap { waiting } => {
            let pulse = if waiting { 0.5 + 0.5 * (t * 6.0).sin() } else { 0.0 };
            let top = lerp_c(Color32::from_rgb(70, 58, 44), Color32::from_rgb(104, 84, 56), hov.max(pulse));
            p.rect_filled(r.translate(vec2(0.0, 2.0)), 0.0, Color32::from_rgb(20, 16, 12));
            p.rect_filled(r, 0.0, top);
            p.rect_filled(Rect::from_min_size(r.min + vec2(3.0, 1.0), vec2(r.width() - 6.0, 2.0)), 0.0, Color32::from_white_alpha(40));
            p.rect_stroke(crate::pxui::snap_rect(r), 0.0, Stroke::new(crate::pxui::unit(), if waiting { GOLD } else { LINE }), StrokeKind::Inside);
            p.text(r.center(), Align2::CENTER_CENTER, text, FontId::proportional(22.0), if waiting { GOLD } else { INK });
        }
    }
    resp
}

/// A small heading: a diamond, the words, and a hairline running off to the right.
fn section(ui: &mut Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
    let p = ui.painter();
    let c = pos2(rect.left() + 6.0, rect.center().y);
    p.add(Shape::convex_polygon(vec![c + vec2(-5.0, 0.0), c + vec2(0.0, -5.0), c + vec2(5.0, 0.0), c + vec2(0.0, 5.0)], EMBER, Stroke::NONE));
    let g = p.layout_no_wrap(text.to_uppercase(), FontId::proportional(22.0), GOLD);
    let w = g.size().x;
    p.galley(pos2(rect.left() + 18.0, rect.center().y - g.size().y / 2.0), g, GOLD);
    p.line_segment([pos2(rect.left() + 28.0 + w, rect.center().y), pos2(rect.right(), rect.center().y)], Stroke::new(1.0, LINE.gamma_multiply(0.7)));
}

/// The panel: dark glass, a thin frame, and gold corner brackets instead of a rounded window.
fn card(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui)) {
    let mut th = crate::pxpanel::Theme::era(2);
    th.fill = Color32::from_rgba_unmultiplied(18, 14, 12, 232);
    th.light = LINE;
    th.dark = Color32::from_rgb(44, 36, 26);
    let r = crate::pxpanel::frame_with(ui, &th, None, 10.0, |ui| {
        ui.set_width(width);
        add(ui);
    })
    .response
    .rect;
    let p = ui.painter();
    let arm = 16.0;
    for (corner, dx, dy) in [(r.left_top(), 1.0, 1.0), (r.right_top(), -1.0, 1.0), (r.right_bottom(), -1.0, -1.0), (r.left_bottom(), 1.0, -1.0)] {
        let c = corner + vec2(-dx * 4.0, -dy * 4.0);
        let u = crate::pxui::unit();
        let c = crate::pxui::snap_pos(c);
        let (ex, ey) = (c.x + dx * arm, c.y + dy * arm);
        let span = |a: f32, b: f32| (a.min(b), a.max(b));
        let (x0, x1) = span(c.x, ex);
        let (y0, y1) = span(c.y, ey);
        // two pixels thick, like the strokes they replace
        let hx = if dy > 0.0 { (c.y, c.y + 2.0 * u) } else { (c.y - 2.0 * u, c.y) };
        let vx = if dx > 0.0 { (c.x, c.x + 2.0 * u) } else { (c.x - 2.0 * u, c.x) };
        crate::pxui::fill(p, Rect::from_min_max(pos2(x0, hx.0), pos2(x1, hx.1)), GOLD);
        crate::pxui::fill(p, Rect::from_min_max(pos2(vx.0, y0), pos2(vx.1, y1)), GOLD);
    }
}

/// A painted text field with a small caption on the left, like the seed field.
fn painted_field(ui: &mut Ui, caption: &str, hint: &str, text: &mut String) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha(110));
    ui.painter().rect_stroke(crate::pxui::snap_rect(rect), 0.0, Stroke::new(crate::pxui::unit(), LINE), StrokeKind::Inside);
    ui.painter().text(pos2(rect.left() + 10.0, rect.center().y), Align2::LEFT_CENTER, caption, FontId::proportional(11.0), Color32::from_rgb(160, 140, 104));
    let er = Rect::from_min_max(pos2(rect.left() + 64.0, rect.top() + 4.0), pos2(rect.right() - 8.0, rect.bottom() - 4.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(er), |ui| {
        ui.add_sized(er.size(), egui::TextEdit::singleline(text).frame(egui::Frame::NONE).hint_text(hint).align(egui::Align2::LEFT_CENTER));
    });
}

/// Same red as the other multiplayer errors.
const ERR_RED: Color32 = Color32::from_rgb(240, 110, 110);

impl Menu {
    /// The internet part of hosting: open button, then the code and its controls once the relay answers.
    fn online_panel(&mut self, ui: &mut Ui, note: Color32) -> Option<MenuAction> {
        use alaz_net::RelayStatus as R;
        let mut act = None;
        let open = |menu: &Menu| {
            let port = menu.port_text.parse::<u16>().ok().filter(|p| *p != 0).unwrap_or(7777);
            let full = menu.slots[menu.slot as usize - 1].is_some();
            let new = if full || menu.hosting { None } else { Some(menu.new_world()) };
            MenuAction::HostOnline { slot: menu.slot, port, new }
        };
        match self.relay.clone() {
            None | Some(R::Closed) => {
                if button(ui, "Ağa aç", vec2(ui.available_width(), 38.0), Look::Primary).clicked() {
                    act = Some(open(self));
                }
                ui.label(egui::RichText::new("Bir kod alırsın; arkadaşın kodla katılır. Port açmak gerekmez.").small().color(note));
            }
            Some(R::Connecting) => {
                ui.label(egui::RichText::new("Bağlanıyor…").color(GOLD));
                if button(ui, "Ağı kapat", vec2(ui.available_width(), 34.0), Look::Plain).clicked() {
                    act = Some(MenuAction::CloseOnline);
                }
            }
            Some(R::Failed(why)) => {
                ui.colored_label(ERR_RED, format!("Ağ açılamadı: {why}"));
                if button(ui, "Ağa aç", vec2(ui.available_width(), 38.0), Look::Primary).clicked() {
                    act = Some(open(self));
                }
            }
            Some(R::Open(code)) => {
                let text = code.to_string();
                // selectable, so a failed clipboard still lets the player copy it by hand
                ui.add(egui::Label::new(egui::RichText::new(&text).size(40.0).strong().color(GOLD).monospace()).selectable(true));
                ui.label(egui::RichText::new(format!("{} oyuncu bağlı", self.relay_players)).color(note));
                ui.horizontal(|ui| {
                    let w = (ui.available_width() - 24.0) / 3.0;
                    if button(ui, "Kopyala", vec2(w, 32.0), Look::Plain).clicked() {
                        ui.ctx().copy_text(text.clone());
                    }
                    if button(ui, "Yeni kod", vec2(w, 32.0), Look::Plain).clicked() {
                        act = Some(MenuAction::NewCode);
                    }
                    if button(ui, "Ağı kapat", vec2(w, 32.0), Look::Danger).clicked() {
                        act = Some(MenuAction::CloseOnline);
                    }
                });
                ui.label(egui::RichText::new("Kod, ağ açık kaldıkça geçerlidir.").small().color(note));
            }
        }
        let line = if self.lan_open { "Aynı ağdaki oyunlar için LAN sunucusu açık." } else { "Yalnızca kodla katılım." };
        ui.label(egui::RichText::new(line).small().color(note));
        act
    }

    fn intro(&mut self, ctx: &egui::Context) -> f32 {
        let now = ctx.input(|i| i.time);
        let born = *self.born.get_or_insert(now);
        if !cfg!(test) {
            ctx.request_repaint();
        }
        if cfg!(test) {
            3.0
        } else {
            (now - born) as f32
        }
    }

    fn draw_keys(&mut self, ctx: &egui::Context, binds: &mut Keybinds) {
        let t = self.intro(ctx);
        let screen = ctx.content_rect();
        let bg = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::NULL.with("menu_bg")));
        bg.rect_filled(screen, 0.0, Color32::from_black_alpha(205));
        let e = ease(t / 0.35);
        egui::Area::new(egui::Id::NULL.with("menu")).anchor(Align2::CENTER_CENTER, [0.0, (1.0 - e) * 24.0]).show(ctx, |ui| {
            ui.set_opacity(e);
            card(ui, 470.0, |ui| {
                ui.label(egui::RichText::new("Tuş atamaları").size(33.0).strong().color(GOLD));
                ui.label(egui::RichText::new("Bir kutuya tıkla, sonra yeni tuşa bas. Delete: boşalt, Esc: vazgeç.").small().color(Color32::from_rgb(190, 176, 150)));
                ui.add_space(8.0);
                egui::ScrollArea::vertical().max_height((screen.height() - 220.0).clamp(160.0, 380.0)).show(ui, |ui| {
                    egui::Grid::new("keys").num_columns(3).spacing([14.0, 8.0]).show(ui, |ui| {
                        for a in Action::ALL {
                            ui.label(a.label());
                            for slot in 0..2 {
                                let waiting = self.capturing == Some((a, slot));
                                let text = if waiting {
                                    "tuşa bas...".to_string()
                                } else {
                                    binds.slots(a)[slot].map_or("-".to_string(), |k| crate::keys::key_name(k).to_string())
                                };
                                if button(ui, &text, vec2(112.0, 28.0), Look::Cap { waiting }).clicked() {
                                    self.capturing = Some((a, slot));
                                }
                            }
                            ui.end_row();
                        }
                    });
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if button(ui, "Varsayılana dön", vec2(170.0, 34.0), Look::Plain).clicked() {
                        binds.reset();
                        binds.save();
                    }
                    if button(ui, "Geri", vec2(110.0, 34.0), Look::Primary).clicked() {
                        self.keys_screen = false;
                        self.capturing = None;
                    }
                });
            });
        });
    }

    fn draw_multiplayer(&mut self, ctx: &egui::Context) -> Option<MenuAction> {
        let t = self.intro(ctx);
        let screen = ctx.content_rect();
        let bg = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::NULL.with("menu_bg")));
        bg.rect_filled(screen, 0.0, Color32::from_black_alpha(205));
        let e = ease(t / 0.35);
        let note = Color32::from_rgb(190, 176, 150);
        let mut act = None;
        egui::Area::new(egui::Id::NULL.with("menu")).anchor(Align2::CENTER_CENTER, [0.0, (1.0 - e) * 24.0]).show(ctx, |ui| {
            ui.set_opacity(e);
            let max_h = (screen.height() - 40.0).max(200.0);
            let mut body = |ui: &mut Ui| {
                card(ui, 470.0, |ui| {
                    ui.label(egui::RichText::new("Çok oyunculu").size(33.0).strong().color(GOLD));
                    ui.add_space(6.0);
                    section(ui, "Sunucu kur");
                    let full = self.slots[self.slot as usize - 1].is_some();
                    let what = if full { "kayıtlı dünya açılır" } else { "yeni dünya kurulur (menüdeki ayarlarla)" };
                    ui.label(egui::RichText::new(format!("Yuva {}: {what}", self.slot)).color(note));
                    ui.add_space(4.0);
                    painted_field(ui, "PORT", "7777", &mut self.port_text);
                    self.port_text.retain(|c| c.is_ascii_digit());
                    self.port_text.truncate(5);
                    ui.add_space(4.0);
                    if button(ui, "Sunucu kur", vec2(ui.available_width(), 42.0), Look::Primary).clicked() {
                        let port = self.port_text.parse::<u16>().ok().filter(|p| *p != 0).unwrap_or(7777);
                        let new = if full { None } else { Some(self.new_world()) };
                        act = Some(MenuAction::Host { slot: self.slot, port, new });
                    }
                    ui.add_space(8.0);
                    section(ui, "İnternetten");
                    if let Some(a) = self.online_panel(ui, note) {
                        act = Some(a);
                    }
                    ui.add_space(4.0);
                    painted_field(ui, "KOD", "K7Q4-XM2P", &mut self.code_text);
                    // char-wise: byte truncation panics inside a multibyte character (ç, ş, ...)
                    self.code_text = self.code_text.chars().filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '-').take(24).collect();
                    ui.add_space(4.0);
                    if button(ui, "Kodla katıl", vec2(ui.available_width(), 38.0), Look::Primary).clicked() && !self.connecting && !self.code_text.trim().is_empty() {
                        act = Some(MenuAction::JoinCode(self.code_text.clone()));
                    }
                    ui.add_space(8.0);
                    section(ui, "Ağdaki oyunlar");
                    if self.found.is_empty() {
                        ui.label(egui::RichText::new("Aranıyor… Aynı ağda sunucu yoksa adresi elle yaz.").color(note));
                    }
                    for f in &self.found {
                        ui.horizontal(|ui| {
                            let bw = 90.0;
                            let text = format!("{} · {} oyuncu · {}", f.label, f.players, f.addr.ip());
                            ui.add_sized(vec2((ui.available_width() - bw - 8.0).max(60.0), 28.0), egui::Label::new(egui::RichText::new(text).color(INK)).truncate());
                            if button(ui, "Katıl", vec2(bw, 28.0), Look::Plain).clicked() && !self.connecting {
                                act = Some(MenuAction::Join(f.addr.to_string()));
                            }
                        });
                    }
                    ui.add_space(8.0);
                    section(ui, "Adresle katıl");
                    painted_field(ui, "ADRES", "192.168.1.5", &mut self.addr_text);
                    ui.add_space(4.0);
                    if button(ui, "Katıl", vec2(ui.available_width(), 38.0), Look::Primary).clicked() && !self.connecting && !self.addr_text.trim().is_empty() {
                        act = Some(MenuAction::Join(self.addr_text.trim().to_string()));
                    }
                    if self.connecting {
                        ui.label(egui::RichText::new("Bağlanıyor…").color(GOLD));
                    }
                    if let Some(err) = &self.mp_error {
                        ui.colored_label(Color32::from_rgb(240, 110, 110), err);
                    }
                    ui.add_space(10.0);
                    if button(ui, "Geri", vec2(110.0, 34.0), Look::Plain).clicked() {
                        self.mp_screen = false;
                        self.mp_error = None;
                    }
                });
            };
            // the card grew with the internet section: scroll it on short windows, as the main menu does
            if screen.height() < 800.0 {
                egui::ScrollArea::vertical().max_height(max_h).auto_shrink([true, true]).show(ui, body);
            } else {
                body(ui);
            }
        });
        act
    }

    /// Pixel size, shadows and far decor, applied while the menu is up (the world behind is not drawn at the start screen,
    /// but the pause menu shows the change at once).
    fn draw_graphics(&mut self, ctx: &egui::Context) {
        use crate::gfxcfg::{DECOR_NAMES, PIXEL_NAMES, SHADOW_NAMES};
        let t = self.intro(ctx);
        let screen = ctx.content_rect();
        let bg = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::NULL.with("menu_bg")));
        bg.rect_filled(screen, 0.0, Color32::from_black_alpha(205));
        let e = ease(t / 0.35);
        let mut q = crate::gfxcfg::current();
        let note = Color32::from_rgb(190, 176, 150);
        egui::Area::new(egui::Id::NULL.with("menu")).anchor(Align2::CENTER_CENTER, [0.0, (1.0 - e) * 24.0]).show(ctx, |ui| {
            ui.set_opacity(e);
            card(ui, 470.0, |ui| {
                ui.label(egui::RichText::new("Grafik").size(33.0).strong().color(GOLD));
                ui.label(egui::RichText::new("Zayıf bir ekran kartında oyun takılırsa daha hafif bir ayar seç. Hemen uygulanır.").small().color(note));
                ui.add_space(8.0);
                section(ui, "Piksel inceliği");
                let mut i = PIXEL_NAMES.iter().position(|p| p.0 == q.pixels).unwrap_or(0);
                segmented(ui, "gfx_pixels", &PIXEL_NAMES.map(|p| p.1), &mut i);
                q.pixels = PIXEL_NAMES[i].0;
                ui.label(egui::RichText::new("Kaba: pikseller büyür, ekran kartı daha az çalışır.").small().color(note));
                ui.add_space(6.0);
                section(ui, "Gölge kalitesi");
                let mut i = SHADOW_NAMES.iter().position(|p| p.0 == q.shadows).unwrap_or(0);
                segmented(ui, "gfx_shadows", &SHADOW_NAMES.map(|p| p.1), &mut i);
                q.shadows = SHADOW_NAMES[i].0;
                ui.label(egui::RichText::new("Düşük: gölgeler köşeli olur. Kapalı: gölge çizilmez, en hafifi.").small().color(note));
                ui.add_space(6.0);
                section(ui, "Uzak çim ve süs");
                let mut i = DECOR_NAMES.iter().position(|p| p.0 == q.decor).unwrap_or(0);
                segmented(ui, "gfx_decor", &DECOR_NAMES.map(|p| p.1), &mut i);
                q.decor = DECOR_NAMES[i].0;
                ui.add_space(10.0);
                if button(ui, "Geri", vec2(110.0, 34.0), Look::Primary).clicked() {
                    self.gfx_screen = false;
                }
            });
        });
        crate::gfxcfg::set(q);
    }

    /// The version screen: what is installed, which channel to follow, and the update when one is found.
    fn draw_update(&mut self, ctx: &egui::Context) -> Option<MenuAction> {
        use crate::update_ui::Phase;
        use alaz_updater::install::Next;
        use alaz_updater::release::Channel;
        self.updater.poll();
        if self.updater.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let t = self.intro(ctx);
        let screen = ctx.content_rect();
        let bg = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::NULL.with("menu_bg")));
        bg.rect_filled(screen, 0.0, Color32::from_black_alpha(205));
        let e = ease(t / 0.35);
        let note = Color32::from_rgb(190, 176, 150);
        let mut act = None;
        let mut leave = false;
        // a setup program has been started: the game has to close so it can replace the files
        if matches!(self.updater.phase, Phase::Done(Next::Exit)) {
            return Some(MenuAction::Quit);
        }
        egui::Area::new(egui::Id::NULL.with("menu")).anchor(Align2::CENTER_CENTER, [0.0, (1.0 - e) * 24.0]).show(ctx, |ui| {
            ui.set_opacity(e);
            card(ui, 470.0, |ui| {
                ui.label(egui::RichText::new("Sürüm").size(33.0).strong().color(GOLD));
                ui.label(egui::RichText::new(format!("Kurulu sürüm: {}", alaz_updater::version::Version::current())).color(INK));
                ui.add_space(8.0);
                section(ui, "Hangi sürümleri izleyeyim?");
                let mut i = usize::from(self.updater.channel == Channel::Preview);
                segmented(ui, "update_channel", &["Kararlı", "Önizleme"], &mut i);
                self.updater.set_channel(if i == 1 { Channel::Preview } else { Channel::Stable });
                ui.label(egui::RichText::new("Önizleme: yayına girmeden önceki deneme sürümlerini de gösterir.").small().color(note));
                ui.add_space(8.0);
                match &self.updater.phase {
                    Phase::Idle => {
                        ui.label(egui::RichText::new("Henüz denetlenmedi.").color(note));
                    }
                    Phase::Checking => {
                        ui.label(egui::RichText::new("Denetleniyor…").color(note));
                    }
                    Phase::UpToDate => {
                        ui.label(egui::RichText::new("Güncelsin. Daha yeni sürüm yok.").color(INK));
                    }
                    Phase::Available(update) => {
                        ui.label(egui::RichText::new(format!("Yeni sürüm: {}", update.version)).color(GOLD));
                        ui.label(egui::RichText::new("İndirilen dosya, yayındaki SHA-256 listesiyle denetlenir. Tutmazsa kurulmaz.").small().color(note));
                    }
                    Phase::Installing(update) => {
                        ui.label(egui::RichText::new(format!("{} indiriliyor ve kuruluyor…", update.version)).color(note));
                    }
                    Phase::Failed(err) => {
                        ui.colored_label(Color32::from_rgb(240, 110, 110), format!("Olmadı: {err}"));
                    }
                    Phase::Done(_) => {
                        ui.label(egui::RichText::new("Kuruldu. Yeni sürümle başlamak için oyunu yeniden başlat.").color(INK));
                    }
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    match &self.updater.phase {
                        Phase::Available(_) => {
                            if button(ui, "Güncelle", vec2(130.0, 34.0), Look::Primary).clicked() {
                                self.updater.start_install();
                            }
                        }
                        Phase::Done(_) => {
                            if button(ui, "Yeniden başlat", vec2(160.0, 34.0), Look::Primary).clicked() {
                                crate::update_ui::relaunch();
                                act = Some(MenuAction::Quit);
                            }
                        }
                        Phase::Checking | Phase::Installing(_) => {}
                        _ => {
                            if button(ui, "Denetle", vec2(130.0, 34.0), Look::Primary).clicked() {
                                self.updater.start_check();
                            }
                        }
                    }
                    if !self.updater.busy() && button(ui, "Geri", vec2(110.0, 34.0), Look::Plain).clicked() {
                        leave = true;
                    }
                });
            });
        });
        if leave {
            self.update_screen = false;
        }
        act
    }

    pub fn draw(&mut self, ctx: &egui::Context, binds: &mut Keybinds) -> Option<MenuAction> {
        crate::hud::apply_style(ctx);
        if self.keys_screen {
            self.draw_keys(ctx, binds);
            return None;
        }
        if self.gfx_screen {
            self.draw_graphics(ctx);
            return None;
        }
        if self.update_screen {
            return self.draw_update(ctx);
        }
        if self.mp_screen {
            return self.draw_multiplayer(ctx);
        }
        let t = self.intro(ctx);
        let screen = ctx.content_rect();
        let wide = screen.width() >= 1000.0;
        let bg = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::NULL.with("menu_bg")));
        let clock = ctx.input(|i| i.time) as f32;
        if self.paused {
            bg.rect_filled(screen, 0.0, Color32::from_black_alpha(190));
        } else {
            scene(&bg, screen, clock, if wide { screen.width() * 0.2 } else { screen.width() * 0.5 });
            let size = (screen.height() * 0.12).clamp(56.0, 118.0);
            let at = if wide { pos2(screen.width() * 0.07, screen.height() * 0.12) } else { pos2(screen.center().x - size * 1.7, screen.height() * 0.03) };
            title(&bg, at, size, t, clock);
        }
        let mut act = None;
        let e = ease((t - 0.5) / 0.5);
        let anchor = if wide { (Align2::RIGHT_CENTER, [-screen.width() * 0.06 + (1.0 - e) * 60.0, 0.0]) } else { (Align2::CENTER_CENTER, [0.0, screen.height() * 0.08 + (1.0 - e) * 40.0]) };
        egui::Area::new(egui::Id::NULL.with("menu")).anchor(anchor.0, anchor.1).show(ctx, |ui| {
            ui.set_opacity(e);
            let max_h = (screen.height() - if wide { 40.0 } else { screen.height() * 0.15 }).max(200.0);
            let mut body = |ui: &mut Ui| {
                card(ui, 430.0, |ui| {
                    if self.paused {
                        ui.label(egui::RichText::new("A.L.A.Z").size(33.0).strong().color(GOLD));
                        ui.label(egui::RichText::new("Duraklatıldı").color(Color32::from_rgb(190, 176, 150)));
                        if let Some(line) = &self.net_line {
                            ui.label(egui::RichText::new(line).color(GOLD));
                        }
                        if self.hosting
                            && let Some(a) = self.online_panel(ui, Color32::from_rgb(190, 176, 150))
                        {
                            act = Some(a);
                        }
                        ui.add_space(8.0);
                    }
                    if let Some(e) = &self.error {
                        ui.colored_label(Color32::from_rgb(240, 110, 110), e);
                    }
                    if self.paused && button(ui, "Oyuna dön", vec2(ui.available_width(), 46.0), Look::Primary).clicked() {
                        act = Some(MenuAction::Resume);
                    }
                    section(ui, "Kayıt yuvaları");
                    for i in 0..self.slots.len() {
                        let n = i as u8 + 1;
                        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::click());
                        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Yuva {n}")));
                        if resp.clicked() {
                            self.slot = n;
                        }
                        let picked = self.slot == n;
                        let hov = ui.ctx().animate_bool_with_time(resp.id, resp.hovered() || picked, 0.15);
                        let p = ui.painter();
                        p.rect_filled(rect, 0.0, lerp_c(Color32::from_rgb(26, 21, 17), Color32::from_rgb(48, 36, 24), hov));
                        p.rect_stroke(crate::pxui::snap_rect(rect), 0.0, Stroke::new(crate::pxui::unit(), if picked { GOLD } else { lerp_c(LINE, EMBER, hov * 0.6) }), StrokeKind::Inside);
                        if picked {
                            p.rect_filled(Rect::from_min_size(rect.min, vec2(3.0, rect.height())), 0.0, GOLD);
                        }
                        // a hexagon with the slot number; lit when a world lives there
                        let hc = pos2(rect.left() + 32.0, rect.center().y);
                        let full = matches!(self.slots[i], Some(Ok(_)));
                        let hex: Vec<Pos2> = (0..6)
                            .map(|k| {
                                let angle = k as f32 * std::f32::consts::FRAC_PI_3 + std::f32::consts::FRAC_PI_6;
                                let (sin, cos) = angle.sin_cos();
                                hc + vec2(cos, sin) * 17.0
                            })
                            .collect();
                        p.add(Shape::convex_polygon(hex, if full { Color32::from_rgb(120, 62, 28) } else { Color32::from_rgb(22, 18, 14) }, Stroke::new(1.5, if full { EMBER } else { LINE })));
                        p.text(hc, Align2::CENTER_CENTER, n.to_string(), FontId::proportional(22.0), if full { Color32::from_rgb(255, 226, 170) } else { Color32::from_rgb(140, 124, 100) });
                        let (head, sub, sub_col) = match &self.slots[i] {
                            None => (format!("Yuva {n}"), "Boş, yeni bir hikâye bekliyor".to_string(), Color32::from_rgb(140, 126, 104)),
                            Some(Err(e)) => (format!("Yuva {n}"), format!("Okunamadı: {e}"), Color32::from_rgb(240, 130, 90)),
                            Some(Ok(s)) => {
                                let day = s.tick / alaz_sim::time::TICKS_PER_DAY + 1;
                                let tag = if s.hardcore { " · hardcore" } else { "" };
                                (format!("Yuva {n}"), format!("Gün {day} · {}² · {} oyuncu{tag}", s.map_w, s.players), Color32::from_rgb(200, 186, 156))
                            }
                        };
                        p.text(pos2(rect.left() + 60.0, rect.center().y - 9.0), Align2::LEFT_CENTER, head, FontId::proportional(22.0), INK);
                        p.text(pos2(rect.left() + 60.0, rect.center().y + 10.0), Align2::LEFT_CENTER, sub, FontId::proportional(22.0), sub_col);
                        let mut bx = rect.right() - 8.0;
                        if self.slots[i].is_some() {
                            let (label, look, w) = if self.confirm_delete == Some(n) { ("Emin misin?", Look::Danger, 104.0) } else { ("Sil", Look::Danger, 46.0) };
                            let br = Rect::from_min_size(pos2(bx - w, rect.center().y - 14.0), vec2(w, 28.0));
                            bx -= w + 6.0;
                            if ui.scope_builder(egui::UiBuilder::new().max_rect(br), |ui| button(ui, label, br.size(), look)).inner.clicked() {
                                if self.confirm_delete == Some(n) {
                                    self.confirm_delete = None;
                                    act = Some(MenuAction::Delete(n));
                                } else {
                                    self.confirm_delete = Some(n);
                                }
                            }
                        }
                        if matches!(self.slots[i], Some(Ok(_))) {
                            let br = Rect::from_min_size(pos2(bx - 74.0, rect.center().y - 14.0), vec2(74.0, 28.0));
                            if ui.scope_builder(egui::UiBuilder::new().max_rect(br), |ui| button(ui, "Devam", br.size(), Look::Primary)).inner.clicked() {
                                act = Some(MenuAction::Continue(n));
                            }
                        }
                        ui.add_space(2.0);
                    }
                    ui.add_space(8.0);
                    section(ui, &format!("Yeni dünya · yuva {}", self.slot));
                    // seed: a painted field with the text edit sitting in it
                    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
                    ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha(110));
                    ui.painter().rect_stroke(crate::pxui::snap_rect(rect), 0.0, Stroke::new(crate::pxui::unit(), LINE), StrokeKind::Inside);
                    ui.painter().text(pos2(rect.left() + 10.0, rect.center().y), Align2::LEFT_CENTER, "TOHUM", FontId::proportional(11.0), Color32::from_rgb(160, 140, 104));
                    let er = Rect::from_min_max(pos2(rect.left() + 64.0, rect.top() + 4.0), pos2(rect.right() - 8.0, rect.bottom() - 4.0));
                    ui.scope_builder(egui::UiBuilder::new().max_rect(er), |ui| {
                        ui.add_sized(er.size(), egui::TextEdit::singleline(&mut self.seed_text).frame(egui::Frame::NONE).hint_text("boş = rastgele").align(egui::Align2::LEFT_CENTER));
                    });
                    ui.add_space(4.0);
                    // size: a segmented control whose highlight glides to the chosen part
                    let (sr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
                    let seg_w = sr.width() / SIZES.len() as f32;
                    let slide = ui.ctx().animate_value_with_time(egui::Id::NULL.with("size_slide"), self.size_idx as f32, 0.18);
                    ui.painter().rect_filled(sr, 0.0, Color32::from_black_alpha(110));
                    ui.painter().rect_filled(Rect::from_min_size(pos2(sr.left() + slide * seg_w, sr.top()), vec2(seg_w, sr.height())).shrink(2.0), 0.0, Color32::from_rgb(150, 84, 34));
                    for (k, (name, _)) in SIZES.iter().enumerate() {
                        let cell = Rect::from_min_size(pos2(sr.left() + k as f32 * seg_w, sr.top()), vec2(seg_w, sr.height()));
                        let resp = ui.interact(cell, egui::Id::NULL.with(("size", k)), Sense::click());
                        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, *name));
                        if resp.clicked() {
                            self.size_idx = k;
                        }
                        let on = self.size_idx == k;
                        ui.painter().text(cell.center(), Align2::CENTER_CENTER, *name, FontId::proportional(22.0), if on { Color32::WHITE } else if resp.hovered() { INK } else { Color32::from_rgb(170, 154, 126) });
                    }
                    ui.painter().rect_stroke(crate::pxui::snap_rect(sr), 0.0, Stroke::new(crate::pxui::unit(), LINE), StrokeKind::Inside);
                    ui.label(egui::RichText::new(SIZE_NOTES[self.size_idx]).color(Color32::from_rgb(170, 154, 126)));
                    ui.add_space(6.0);
                    // difficulty and season length are fixed for the life of the world
                    segmented(ui, "difficulty", &DIFFICULTIES, &mut self.difficulty_idx);
                    ui.label(egui::RichText::new(match self.difficulty_idx {
                        0 => "Affedici: işler kısa, başarısız el işinde malzemenin yarısı geri gelir, kış serin.",
                        1 => "Gerçekçi: uzun işler, kalıcı hatalar, sert kış. Sonradan değiştirilemez.",
                        _ => "Yaratıcı: ihtiyaç ve ölüm yok, her şey açık ve bedava, eşya ver, zamanı ve havayı değiştir, koşarken uç.",
                    }).small().color(Color32::from_rgb(190, 176, 150)));
                    ui.add_space(4.0);
                    let season_names: Vec<&str> = SEASONS.iter().map(|s| s.0).collect();
                    segmented(ui, "season", &season_names, &mut self.season_idx);
                    ui.add_space(6.0);
                    // hardcore: a switch whose knob slides, and a skull that warms up when it is on
                    let (hr, hresp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
                    hresp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Hardcore"));
                    if hresp.clicked() {
                        self.hardcore = !self.hardcore;
                    }
                    let on = ui.ctx().animate_bool_with_time(hresp.id.with("sw"), self.hardcore, 0.16);
                    let tr = Rect::from_min_size(pos2(hr.left() + 2.0, hr.center().y - 10.0), vec2(42.0, 20.0));
                    let p = ui.painter();
                    p.rect_filled(tr, 0.0, lerp_c(Color32::from_rgb(34, 28, 22), Color32::from_rgb(150, 44, 36), on));
                    p.rect_stroke(crate::pxui::snap_rect(tr), 0.0, Stroke::new(crate::pxui::unit(), lerp_c(LINE, Color32::from_rgb(240, 90, 70), on)), StrokeKind::Inside);
                    p.circle_filled(pos2(tr.left() + 10.0 + on * 22.0, tr.center().y), 7.5, lerp_c(Color32::from_rgb(190, 176, 150), Color32::WHITE, on));
                    ui_art::glyph(p, pos2(tr.right() + 20.0, hr.center().y), 18.0, Glyph::Skull, lerp_c(Color32::from_rgb(120, 108, 90), Color32::from_rgb(244, 120, 100), on));
                    p.text(pos2(tr.right() + 38.0, hr.center().y), Align2::LEFT_CENTER, "Hardcore: ölünce dünya silinir", FontId::proportional(22.0), lerp_c(Color32::from_rgb(190, 176, 150), INK, on));
                    ui.add_space(8.0);
                    let occupied = self.slots[self.slot as usize - 1].is_some();
                    let label = if occupied { "Yeni dünya başlat (yuvanın üstüne yazar)" } else { "Yeni dünya başlat" };
                    if button(ui, label, vec2(ui.available_width(), 48.0), Look::Primary).clicked() {
                        let NewWorld { slot, seed, size, hardcore, settings } = self.new_world();
                        act = Some(MenuAction::New { slot, seed, size, hardcore, settings });
                    }
                    ui.add_space(10.0);
                    if let Some(v) = self.updater.available().filter(|_| !self.paused) {
                        ui.label(egui::RichText::new(format!("Yeni sürüm var: {v}")).small().color(GOLD));
                    }
                    ui.horizontal(|ui| {
                        let parts = if self.paused { 2.0 } else { 4.0 };
                        let w = (ui.available_width() - 12.0 * (parts - 1.0)) / parts;
                        if !self.paused && button(ui, "Çok oyunculu", vec2(w, 34.0), Look::Plain).clicked() {
                            self.mp_screen = true;
                        }
                        if button(ui, "Tuşlar", vec2(w, 34.0), Look::Plain).clicked() {
                            self.keys_screen = true;
                        }
                        if button(ui, "Grafik", vec2(w, 34.0), Look::Plain).clicked() {
                            self.gfx_screen = true;
                        }
                        // updating replaces the program, so it is only offered where no world is running
                        if !self.paused && button(ui, "Sürüm", vec2(w, 34.0), Look::Plain).clicked() {
                            self.update_screen = true;
                        }
                    });
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        let w = if self.paused { (ui.available_width() - 12.0) / 2.0 } else { ui.available_width() };
                        if self.paused && button(ui, "Kaydet ve çık", vec2(w, 34.0), Look::Plain).clicked() {
                            act = Some(MenuAction::SaveQuit);
                        }
                        if button(ui, "Çık", vec2(w, 34.0), Look::Danger).clicked() {
                            act = Some(MenuAction::Quit);
                        }
                    });
                });
            };
            // the panel only scrolls when the window is too short for it
            if screen.height() < 800.0 {
                egui::ScrollArea::vertical().max_height(max_h).auto_shrink([true, true]).show(ui, body);
            } else {
                body(ui);
            }
        });
        act
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;

    struct S {
        menu: Menu,
        binds: Keybinds,
        got: Option<MenuAction>,
    }

    #[test]
    fn starting_a_new_world_from_the_menu_works_with_a_click() {
        let state = S { menu: Menu::new(false), binds: Keybinds::default(), got: None };
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 900.0)).build_ui_state(
            |ui, s: &mut S| {
                if let Some(a) = s.menu.draw(ui.ctx(), &mut s.binds) {
                    s.got = Some(a);
                }
            },
            state,
        );
        h.run();
        h.get_by_label_contains("Yeni dünya başlat").click();
        h.run();
        assert!(matches!(h.state().got, Some(MenuAction::New { .. })), "clicking starts a new world");
    }

    fn harness(menu: Menu) -> egui_kittest::Harness<'static, S> {
        let state = S { menu, binds: Keybinds::default(), got: None };
        egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 900.0)).build_ui_state(
            |ui, s: &mut S| {
                if let Some(a) = s.menu.draw(ui.ctx(), &mut s.binds) {
                    s.got = Some(a);
                }
            },
            state,
        )
    }

    fn fake_update() -> alaz_updater::release::Update {
        let asset = |name: &str| alaz_updater::release::Asset { name: name.to_string(), browser_download_url: format!("https://github.com/x/{name}"), size: 1 };
        alaz_updater::release::Update { version: alaz_updater::version::Version::parse("99.0.0").unwrap(), package: asset("ALAZ-pkg"), sums: asset("SHA256SUMS") }
    }

    fn fake_backend() -> crate::update_ui::Backend {
        crate::update_ui::Backend {
            check: |_| Ok(Some(fake_update())),
            install: |_| Ok(alaz_updater::install::Next::Restart),
        }
    }

    fn settle(h: &mut egui_kittest::Harness<'static, S>, until: &str) {
        for _ in 0..200 {
            h.step();
            if h.query_by_label_contains(until).is_some() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let phase = match &h.state().menu.updater.phase {
            crate::update_ui::Phase::Idle => "idle",
            crate::update_ui::Phase::Checking => "checking",
            crate::update_ui::Phase::UpToDate => "up to date",
            crate::update_ui::Phase::Available(_) => "available",
            crate::update_ui::Phase::Installing(_) => "installing",
            crate::update_ui::Phase::Failed(_) => "failed",
            crate::update_ui::Phase::Done(_) => "done",
        };
        panic!("the screen never showed {until:?} (the update state is {phase})");
    }

    #[test]
    fn version_screen_checks_then_installs_only_on_the_click() {
        let mut menu = Menu::new(false);
        menu.updater = crate::update_ui::UpdateState::new(fake_backend(), alaz_updater::release::Channel::Stable);
        let mut h = harness(menu);
        h.run();
        h.get_by_label("Sürüm").click();
        h.run();
        h.get_by_label_contains("Kurulu sürüm").click();
        assert!(h.query_by_label_contains("Henüz denetlenmedi").is_some(), "nothing is looked up before the player asks");
        h.get_by_label("Denetle").click();
        settle(&mut h, "Yeni sürüm: 99.0.0");
        assert!(h.query_by_label_contains("Kuruldu").is_none(), "finding an update does not install it");
        h.run();
        h.get_by_label("Güncelle").click();
        h.step();
        h.step();
        settle(&mut h, "Kuruldu");
        assert!(h.state().got.is_none(), "the game is not closed behind the player's back");
    }

    #[test]
    fn the_start_screen_says_when_a_newer_version_was_found() {
        let mut menu = Menu::new(false);
        menu.updater = crate::update_ui::UpdateState::new(fake_backend(), alaz_updater::release::Channel::Stable);
        menu.updater.phase = crate::update_ui::Phase::Available(fake_update());
        let mut h = harness(menu);
        h.run();
        assert!(h.query_by_label_contains("Yeni sürüm var: 99.0.0").is_some());
    }

    #[test]
    fn a_running_game_cannot_be_updated_from_the_pause_menu() {
        let mut h = harness(Menu::new(true));
        h.run();
        assert!(h.query_by_label("Sürüm").is_none());
    }

    #[test]
    fn multiplayer_screen_hosts_the_selected_slot() {
        let mut h = harness(Menu::new(false));
        h.run();
        h.get_by_label_contains("Çok oyunculu").click();
        h.run();
        h.get_by_label("Sunucu kur").click();
        h.run();
        assert!(matches!(h.state().got, Some(MenuAction::Host { port: 7777, .. })), "hosting uses the default port");
    }

    #[test]
    fn multiplayer_screen_joins_a_typed_address() {
        let mut h = harness(Menu::new(false));
        h.run();
        h.get_by_label_contains("Çok oyunculu").click();
        h.run();
        let fields: Vec<_> = h.query_all_by_role(egui::accesskit::Role::TextInput).collect();
        fields[fields.len() - 1].click();
        h.run();
        let fields: Vec<_> = h.query_all_by_role(egui::accesskit::Role::TextInput).collect();
        fields[fields.len() - 1].type_text("10.0.0.7");
        h.run();
        h.get_by_label("Katıl").click();
        h.run();
        assert!(matches!(&h.state().got, Some(MenuAction::Join(a)) if a == "10.0.0.7"));
    }

    fn open_mp(menu: Menu) -> egui_kittest::Harness<'static, S> {
        let mut h = harness(menu);
        h.run();
        h.get_by_label_contains("Çok oyunculu").click();
        h.run();
        h
    }

    #[test]
    fn online_button_emits_host_online() {
        let mut h = open_mp(Menu::new(false));
        h.get_by_label("Ağa aç").click();
        h.run();
        assert!(matches!(h.state().got, Some(MenuAction::HostOnline { port: 7777, .. })));
    }

    #[test]
    fn online_panel_says_whether_the_lan_port_is_open() {
        let mut m = Menu::new(false);
        m.lan_open = false;
        let h = open_mp(m);
        assert!(h.query_by_label("Yalnızca kodla katılım.").is_some());
        assert!(h.query_by_label("Aynı ağdaki oyunlar için LAN sunucusu açık.").is_none());
        let mut m = Menu::new(false);
        m.lan_open = true;
        let h = open_mp(m);
        assert!(h.query_by_label("Aynı ağdaki oyunlar için LAN sunucusu açık.").is_some());
        assert!(h.query_by_label("Yalnızca kodla katılım.").is_none());
    }

    #[test]
    fn code_field_emits_join_code_with_typed_text() {
        let mut h = open_mp(Menu::new(false));
        let fields: Vec<_> = h.query_all_by_role(egui::accesskit::Role::TextInput).collect();
        fields[fields.len() - 2].click();
        h.run();
        let fields: Vec<_> = h.query_all_by_role(egui::accesskit::Role::TextInput).collect();
        fields[fields.len() - 2].type_text("k7q4 xm2p");
        h.run();
        h.get_by_label("Kodla katıl").click();
        h.run();
        assert!(matches!(&h.state().got, Some(MenuAction::JoinCode(c)) if c == "k7q4 xm2p"));
    }

    #[test]
    fn code_field_survives_multibyte_text() {
        for typed in ["abcdefghijklmnopqrstuvwç".to_string() + "şğöü", "çşğöüçşğöüçşğöüçşğöüçşğöüçşğöü".to_string(), "abçcd-ef gh".to_string()] {
            let mut h = open_mp(Menu::new(false));
            let fields: Vec<_> = h.query_all_by_role(egui::accesskit::Role::TextInput).collect();
            fields[fields.len() - 2].click();
            h.run();
            let fields: Vec<_> = h.query_all_by_role(egui::accesskit::Role::TextInput).collect();
            fields[fields.len() - 2].type_text(&typed);
            h.run();
            h.run();
            let t = &h.state().menu.code_text;
            assert!(t.chars().count() <= 24);
            assert!(t.chars().all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '-'), "got {t:?}");
        }
    }

    #[test]
    fn opened_state_shows_code_and_player_count() {
        let mut menu = Menu::new(false);
        menu.hosting = true;
        menu.relay = Some(alaz_net::RelayStatus::Open(alaz_net::Code::parse("k7q4xm2p").unwrap()));
        menu.relay_players = 3;
        let mut h = open_mp(menu);
        h.get_by_label("K7Q4-XM2P");
        h.get_by_label_contains("3 oyuncu");
        h.get_by_label("Kopyala");
        h.get_by_label("Yeni kod").click();
        h.run();
        assert!(matches!(h.state().got, Some(MenuAction::NewCode)));
        h.get_by_label("Ağı kapat").click();
        h.run();
        assert!(matches!(h.state().got, Some(MenuAction::CloseOnline)));
    }

    #[test]
    fn copy_button_does_not_panic_without_a_clipboard() {
        let mut menu = Menu::new(false);
        menu.hosting = true;
        menu.relay = Some(alaz_net::RelayStatus::Open(alaz_net::Code::parse("k7q4xm2p").unwrap()));
        let mut h = open_mp(menu);
        h.get_by_label("Kopyala").click();
        h.run();
        h.get_by_label("K7Q4-XM2P");
    }

    #[test]
    fn connecting_state_shows_progress() {
        let mut menu = Menu::new(false);
        menu.hosting = true;
        menu.relay = Some(alaz_net::RelayStatus::Connecting);
        let h = open_mp(menu);
        h.get_by_label_contains("Bağlanıyor");
    }

    #[test]
    fn failed_state_shows_red_message_and_allows_retry() {
        let mut menu = Menu::new(false);
        menu.hosting = true;
        menu.relay = Some(alaz_net::RelayStatus::Failed("röleye ulaşılamadı".into()));
        let mut h = open_mp(menu);
        h.get_by_label_contains("röleye ulaşılamadı");
        h.get_by_label("Ağa aç").click();
        h.run();
        assert!(matches!(h.state().got, Some(MenuAction::HostOnline { .. })), "Failed is terminal: the button opens again");
    }

    #[test]
    fn paused_menu_without_multiplayer_still_hidden() {
        let mut h = harness(Menu::new(true));
        h.run();
        assert!(h.query_by_label_contains("Çok oyunculu").is_none());
        assert!(h.query_by_label("Ağa aç").is_none(), "not hosting: no online controls in the pause card");
    }

    #[test]
    fn paused_menu_while_hosting_shows_the_code() {
        let mut menu = Menu::new(true);
        menu.hosting = true;
        menu.relay = Some(alaz_net::RelayStatus::Open(alaz_net::Code::parse("k7q4xm2p").unwrap()));
        let mut h = harness(menu);
        h.run();
        h.get_by_label("K7Q4-XM2P");
    }

    #[test]
    fn paused_menu_has_no_multiplayer_button() {
        let mut h = harness(Menu::new(true));
        h.run();
        assert!(h.query_by_label_contains("Çok oyunculu").is_none());
    }
}
