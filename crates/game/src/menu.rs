//! Start screen and pause menu: pick a save slot, continue, start a new world, quit.

use crate::keys::{bindable, Action, Keybinds};
use crate::ui_art::{self, Glyph};
use alaz_render::egui::{self, epaint::Mesh, pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui};
use winit::keyboard::KeyCode;

pub enum MenuAction {
    Continue(u8),
    New { slot: u8, seed: Option<u64>, size: i32, hardcore: bool, settings: alaz_sim::world::WorldSettings },
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
    /// Waiting for a key press to bind to (action, slot).
    capturing: Option<(Action, usize)>,
    pub error: Option<String>,
    /// When the screen first drew, for the intro animation.
    born: Option<f64>,
}

const DIFFICULTIES: [&str; 2] = ["Affedici", "Gerçekçi"];
const SEASONS: [(&str, u8); 3] = [("Mevsim 3 gün", 3), ("Mevsim 6 gün", 6), ("Mevsim 9 gün", 9)];

/// A row of choices with a sliding highlight, like the map size picker.
fn segmented(ui: &mut Ui, id: &str, names: &[&str], idx: &mut usize) {
    let (sr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
    let seg_w = sr.width() / names.len() as f32;
    let slide = ui.ctx().animate_value_with_time(egui::Id::NULL.with((id, "slide")), *idx as f32, 0.18);
    ui.painter().rect_filled(sr, 2.0, Color32::from_black_alpha(110));
    ui.painter().rect_filled(Rect::from_min_size(pos2(sr.left() + slide * seg_w, sr.top()), vec2(seg_w, sr.height())).shrink(2.0), 2.0, Color32::from_rgb(150, 84, 34));
    for (k, name) in names.iter().enumerate() {
        let cell = Rect::from_min_size(pos2(sr.left() + k as f32 * seg_w, sr.top()), vec2(seg_w, sr.height()));
        let resp = ui.interact(cell, egui::Id::NULL.with((id, k)), Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, *name));
        if resp.clicked() {
            *idx = k;
        }
        let on = *idx == k;
        ui.painter().text(cell.center(), Align2::CENTER_CENTER, *name, FontId::proportional(13.0), if on { Color32::WHITE } else if resp.hovered() { INK } else { Color32::from_rgb(170, 154, 126) });
    }
    ui.painter().rect_stroke(sr, 2.0, Stroke::new(1.0, LINE), StrokeKind::Inside);
}

const SIZES: [(&str, i32); 6] = [("Küçük 128", 128), ("Orta 256", 256), ("Büyük 1024", 1024), ("Dev 4096", 4096), ("Devasa 32768", 32768), ("Sınırsız", alaz_sim::map::MAX_MAP_SIDE)];

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
            keys_screen: false,
            capturing: None,
            error: None,
            born: None,
        };
        m.refresh();
        m
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

/// The big extruded title, letters dropping in one after another, sparks between them.
fn title(p: &Painter, at: Pos2, size: f32, intro: f32, t: f32) {
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
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, text));
    let hov = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.14);
    let t = ui.input(|i| i.time) as f32;
    let down = resp.is_pointer_button_down_on();
    let p = ui.painter();
    let r = rect.translate(vec2(0.0, if down { 1.0 } else { 0.0 }));
    match look {
        Look::Primary => {
            let mut m = Mesh::default();
            let l = lerp_c(Color32::from_rgb(176, 92, 34), Color32::from_rgb(222, 120, 44), hov);
            let rr = lerp_c(Color32::from_rgb(120, 60, 28), Color32::from_rgb(176, 90, 36), hov);
            quad(&mut m, r, l, rr, rr, l);
            p.add(Shape::mesh(m));
            // a shine band sweeping across now and then
            let sweep = ((t * 0.45).fract() * 1.6 - 0.3) * r.width();
            let band = Rect::from_min_max(pos2(r.left() + sweep, r.top()), pos2(r.left() + sweep + 26.0, r.bottom())).intersect(r);
            if band.width() > 0.0 {
                p.rect_filled(band, 0.0, Color32::from_white_alpha(30));
            }
            p.rect_stroke(r, 2.0, Stroke::new(1.5, GOLD), StrokeKind::Inside);
            let shift = hov * 6.0;
            p.text(pos2(r.left() + 16.0 + shift, r.center().y), Align2::LEFT_CENTER, text, FontId::proportional(17.0), Color32::from_rgb(255, 244, 220));
            if r.width() > 140.0 {
                ui_art::glyph(p, pos2(r.right() - 20.0 - (1.0 - hov) * 4.0, r.center().y), 16.0, Glyph::Flame, Color32::from_rgb(255, 226, 150));
            }
        }
        Look::Plain | Look::Danger => {
            let acc = if look == Look::Danger { Color32::from_rgb(220, 90, 80) } else { EMBER };
            p.rect_filled(r, 2.0, Color32::from_rgba_unmultiplied(0, 0, 0, 90));
            // the fill grows out of the left edge
            let fw = r.width() * hov;
            p.rect_filled(Rect::from_min_size(r.min, vec2(fw, r.height())), 2.0, Color32::from_rgba_unmultiplied(acc.r(), acc.g(), acc.b(), 70));
            p.rect_filled(Rect::from_min_size(r.min, vec2(3.0, r.height())), 0.0, acc.gamma_multiply(0.5 + 0.5 * hov));
            p.rect_stroke(r, 2.0, Stroke::new(1.0, lerp_c(LINE, acc, hov)), StrokeKind::Inside);
            let shift = hov * 5.0;
            p.text(pos2(r.left() + 12.0 + shift, r.center().y), Align2::LEFT_CENTER, text, FontId::proportional(15.0), lerp_c(INK, Color32::WHITE, hov));
        }
        Look::Cap { waiting } => {
            let pulse = if waiting { 0.5 + 0.5 * (t * 6.0).sin() } else { 0.0 };
            let top = lerp_c(Color32::from_rgb(70, 58, 44), Color32::from_rgb(104, 84, 56), hov.max(pulse));
            p.rect_filled(r.translate(vec2(0.0, 2.0)), 4.0, Color32::from_rgb(20, 16, 12));
            p.rect_filled(r, 4.0, top);
            p.rect_filled(Rect::from_min_size(r.min + vec2(3.0, 1.0), vec2(r.width() - 6.0, 2.0)), 1.0, Color32::from_white_alpha(40));
            p.rect_stroke(r, 4.0, Stroke::new(1.2, if waiting { GOLD } else { LINE }), StrokeKind::Inside);
            p.text(r.center(), Align2::CENTER_CENTER, text, FontId::proportional(14.0), if waiting { GOLD } else { INK });
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
    let g = p.layout_no_wrap(text.to_uppercase(), FontId::proportional(13.0), GOLD);
    let w = g.size().x;
    p.galley(pos2(rect.left() + 18.0, rect.center().y - g.size().y / 2.0), g, GOLD);
    p.line_segment([pos2(rect.left() + 28.0 + w, rect.center().y), pos2(rect.right(), rect.center().y)], Stroke::new(1.0, LINE.gamma_multiply(0.7)));
}

/// The panel: dark glass, a thin frame, and gold corner brackets instead of a rounded window.
fn card(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui)) {
    let frame = egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(18, 14, 12, 228))
        .stroke(Stroke::new(1.0, LINE.gamma_multiply(0.8)))
        .inner_margin(egui::Margin::symmetric(20, 18))
        .shadow(egui::epaint::Shadow { offset: [0, 6], blur: 24, spread: 0, color: Color32::from_black_alpha(160) });
    let r = frame
        .show(ui, |ui| {
            ui.set_width(width);
            add(ui);
        })
        .response
        .rect;
    let p = ui.painter();
    let arm = 16.0;
    for (corner, dx, dy) in [(r.left_top(), 1.0, 1.0), (r.right_top(), -1.0, 1.0), (r.right_bottom(), -1.0, -1.0), (r.left_bottom(), 1.0, -1.0)] {
        let c = corner + vec2(-dx * 4.0, -dy * 4.0);
        p.line_segment([c, c + vec2(dx * arm, 0.0)], Stroke::new(2.0, GOLD));
        p.line_segment([c, c + vec2(0.0, dy * arm)], Stroke::new(2.0, GOLD));
    }
}

impl Menu {
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
                ui.label(egui::RichText::new("Tuş atamaları").size(24.0).strong().color(GOLD));
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

    pub fn draw(&mut self, ctx: &egui::Context, binds: &mut Keybinds) -> Option<MenuAction> {
        crate::hud::apply_style(ctx);
        if self.keys_screen {
            self.draw_keys(ctx, binds);
            return None;
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
                        ui.label(egui::RichText::new("A.L.A.Z").size(28.0).strong().color(GOLD));
                        ui.label(egui::RichText::new("Duraklatıldı").color(Color32::from_rgb(190, 176, 150)));
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
                        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Yuva {n}")));
                        if resp.clicked() {
                            self.slot = n;
                        }
                        let picked = self.slot == n;
                        let hov = ui.ctx().animate_bool_with_time(resp.id, resp.hovered() || picked, 0.15);
                        let p = ui.painter();
                        p.rect_filled(rect, 2.0, lerp_c(Color32::from_rgb(26, 21, 17), Color32::from_rgb(48, 36, 24), hov));
                        p.rect_stroke(rect, 2.0, Stroke::new(if picked { 1.6 } else { 1.0 }, if picked { GOLD } else { lerp_c(LINE, EMBER, hov * 0.6) }), StrokeKind::Inside);
                        if picked {
                            p.rect_filled(Rect::from_min_size(rect.min, vec2(3.0, rect.height())), 0.0, GOLD);
                        }
                        // a hexagon with the slot number; lit when a world lives there
                        let hc = pos2(rect.left() + 32.0, rect.center().y);
                        let full = matches!(self.slots[i], Some(Ok(_)));
                        let hex: Vec<Pos2> = (0..6).map(|k| hc + vec2((k as f32 * 1.0472 + 0.5236).cos(), (k as f32 * 1.0472 + 0.5236).sin()) * 17.0).collect();
                        p.add(Shape::convex_polygon(hex, if full { Color32::from_rgb(120, 62, 28) } else { Color32::from_rgb(22, 18, 14) }, Stroke::new(1.5, if full { EMBER } else { LINE })));
                        p.text(hc, Align2::CENTER_CENTER, n.to_string(), FontId::proportional(18.0), if full { Color32::from_rgb(255, 226, 170) } else { Color32::from_rgb(140, 124, 100) });
                        let (head, sub, sub_col) = match &self.slots[i] {
                            None => (format!("Yuva {n}"), "Boş, yeni bir hikâye bekliyor".to_string(), Color32::from_rgb(140, 126, 104)),
                            Some(Err(e)) => (format!("Yuva {n}"), format!("Okunamadı: {e}"), Color32::from_rgb(240, 130, 90)),
                            Some(Ok(s)) => {
                                let day = s.tick / alaz_sim::time::TICKS_PER_DAY + 1;
                                let tag = if s.hardcore { " · hardcore" } else { "" };
                                (format!("Yuva {n}"), format!("Gün {day} · {}² · {} oyuncu{tag}", s.map_w, s.players), Color32::from_rgb(200, 186, 156))
                            }
                        };
                        p.text(pos2(rect.left() + 60.0, rect.center().y - 9.0), Align2::LEFT_CENTER, head, FontId::proportional(16.0), INK);
                        p.text(pos2(rect.left() + 60.0, rect.center().y + 10.0), Align2::LEFT_CENTER, sub, FontId::proportional(12.5), sub_col);
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
                    ui.painter().rect_filled(rect, 2.0, Color32::from_black_alpha(110));
                    ui.painter().rect_stroke(rect, 2.0, Stroke::new(1.0, LINE), StrokeKind::Inside);
                    ui.painter().text(pos2(rect.left() + 10.0, rect.center().y), Align2::LEFT_CENTER, "TOHUM", FontId::proportional(11.5), Color32::from_rgb(160, 140, 104));
                    let er = Rect::from_min_max(pos2(rect.left() + 64.0, rect.top() + 4.0), pos2(rect.right() - 8.0, rect.bottom() - 4.0));
                    ui.scope_builder(egui::UiBuilder::new().max_rect(er), |ui| {
                        ui.add_sized(er.size(), egui::TextEdit::singleline(&mut self.seed_text).frame(egui::Frame::NONE).hint_text("boş = rastgele").vertical_align(egui::Align::Center));
                    });
                    ui.add_space(4.0);
                    // size: a segmented control whose highlight glides to the chosen part
                    let (sr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
                    let seg_w = sr.width() / SIZES.len() as f32;
                    let slide = ui.ctx().animate_value_with_time(egui::Id::NULL.with("size_slide"), self.size_idx as f32, 0.18);
                    ui.painter().rect_filled(sr, 2.0, Color32::from_black_alpha(110));
                    ui.painter().rect_filled(Rect::from_min_size(pos2(sr.left() + slide * seg_w, sr.top()), vec2(seg_w, sr.height())).shrink(2.0), 2.0, Color32::from_rgb(150, 84, 34));
                    for (k, (name, _)) in SIZES.iter().enumerate() {
                        let cell = Rect::from_min_size(pos2(sr.left() + k as f32 * seg_w, sr.top()), vec2(seg_w, sr.height()));
                        let resp = ui.interact(cell, egui::Id::NULL.with(("size", k)), Sense::click());
                        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, *name));
                        if resp.clicked() {
                            self.size_idx = k;
                        }
                        let on = self.size_idx == k;
                        ui.painter().text(cell.center(), Align2::CENTER_CENTER, *name, FontId::proportional(13.0), if on { Color32::WHITE } else if resp.hovered() { INK } else { Color32::from_rgb(170, 154, 126) });
                    }
                    ui.painter().rect_stroke(sr, 2.0, Stroke::new(1.0, LINE), StrokeKind::Inside);
                    ui.add_space(6.0);
                    // difficulty and season length are fixed for the life of the world
                    segmented(ui, "difficulty", &DIFFICULTIES, &mut self.difficulty_idx);
                    ui.label(egui::RichText::new(if self.difficulty_idx == 0 {
                        "Affedici: işler kısa, başarısız el işinde malzemenin yarısı geri gelir, kış serin."
                    } else {
                        "Gerçekçi: uzun işler, kalıcı hatalar, sert kış. Sonradan değiştirilemez."
                    }).small().color(Color32::from_rgb(190, 176, 150)));
                    ui.add_space(4.0);
                    let season_names: Vec<&str> = SEASONS.iter().map(|s| s.0).collect();
                    segmented(ui, "season", &season_names, &mut self.season_idx);
                    ui.add_space(6.0);
                    // hardcore: a switch whose knob slides, and a skull that warms up when it is on
                    let (hr, hresp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
                    hresp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Hardcore"));
                    if hresp.clicked() {
                        self.hardcore = !self.hardcore;
                    }
                    let on = ui.ctx().animate_bool_with_time(hresp.id.with("sw"), self.hardcore, 0.16);
                    let tr = Rect::from_min_size(pos2(hr.left() + 2.0, hr.center().y - 10.0), vec2(42.0, 20.0));
                    let p = ui.painter();
                    p.rect_filled(tr, 10.0, lerp_c(Color32::from_rgb(34, 28, 22), Color32::from_rgb(150, 44, 36), on));
                    p.rect_stroke(tr, 10.0, Stroke::new(1.2, lerp_c(LINE, Color32::from_rgb(240, 90, 70), on)), StrokeKind::Inside);
                    p.circle_filled(pos2(tr.left() + 10.0 + on * 22.0, tr.center().y), 7.5, lerp_c(Color32::from_rgb(190, 176, 150), Color32::WHITE, on));
                    ui_art::glyph(p, pos2(tr.right() + 20.0, hr.center().y), 18.0, Glyph::Skull, lerp_c(Color32::from_rgb(120, 108, 90), Color32::from_rgb(244, 120, 100), on));
                    p.text(pos2(tr.right() + 38.0, hr.center().y), Align2::LEFT_CENTER, "Hardcore: ölünce dünya silinir", FontId::proportional(14.0), lerp_c(Color32::from_rgb(190, 176, 150), INK, on));
                    ui.add_space(8.0);
                    let occupied = self.slots[self.slot as usize - 1].is_some();
                    let label = if occupied { "Yeni dünya başlat (yuvanın üstüne yazar)" } else { "Yeni dünya başlat" };
                    if button(ui, label, vec2(ui.available_width(), 48.0), Look::Primary).clicked() {
                        let seed = self.seed_text.trim().parse::<u64>().ok();
                        let difficulty = if self.difficulty_idx == 0 { alaz_sim::world::Difficulty::Forgiving } else { alaz_sim::world::Difficulty::Realistic };
                        let settings = alaz_sim::world::WorldSettings { difficulty, season_days: SEASONS[self.season_idx].1 };
                        act = Some(MenuAction::New { slot: self.slot, seed, size: SIZES[self.size_idx].1, hardcore: self.hardcore, settings });
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        let w = (ui.available_width() - 12.0) / if self.paused { 3.0 } else { 2.0 };
                        if self.paused && button(ui, "Kaydet ve çık", vec2(w, 34.0), Look::Plain).clicked() {
                            act = Some(MenuAction::SaveQuit);
                        }
                        if button(ui, "Tuşlar", vec2(w, 34.0), Look::Plain).clicked() {
                            self.keys_screen = true;
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
}
