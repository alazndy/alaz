//! Pixel panels for the interface. Where `pxui` paints the hand-work minigames, this is the toolkit for the HUD
//! itself: a panel frame with a hard ink outline, a bevel and a stepped corner, and the small parts that go in it
//! (bar, pips, chip, tab, rule). Panel contents are still ordinary egui widgets; only the chrome is painted here.
//!
//! One pixel is 2 points, but never a fractional number of screen pixels: `Px` rounds the pixel to a whole number
//! of physical pixels for the current `pixels_per_point`, so edges stay crisp at any interface scale.
// the kit is ahead of its users: only one panel has moved over so far
#![allow(dead_code)]

use alaz_render::egui::{self, Align2, Color32, FontId, InnerResponse, Pos2, Rect, Response, Sense, Shape, Ui, Vec2};

use crate::pxui::{INK, PAPER};

/// What a pixel is for the current scale: a whole number of physical pixels, expressed in points.
#[derive(Clone, Copy)]
pub struct Px {
    pub u: f32,
}

impl Px {
    pub fn of(ctx: &egui::Context) -> Self {
        let ppp = ctx.pixels_per_point();
        Px { u: (crate::pxui::PX * ppp).round().max(1.0) / ppp }
    }

    /// Nearest grid line.
    pub fn snap(self, v: f32) -> f32 {
        (v / self.u).round() * self.u
    }

    pub fn pos(self, p: Pos2) -> Pos2 {
        Pos2::new(self.snap(p.x), self.snap(p.y))
    }

    /// The smallest grid-aligned rectangle that holds `r`.
    pub fn outer(self, r: Rect) -> Rect {
        let f = |v: f32| (v / self.u).floor() * self.u;
        let c = |v: f32| (v / self.u).ceil() * self.u;
        Rect::from_min_max(Pos2::new(f(r.min.x), f(r.min.y)), Pos2::new(c(r.max.x), c(r.max.y)))
    }

    /// A rectangle `n` pixels wide and high, in points.
    pub fn n(self, n: f32) -> f32 {
        n * self.u
    }
}

/// The colours of a panel. One per era so far; add a field here and every panel follows.
#[derive(Clone, Copy)]
pub struct Theme {
    pub fill: Color32,
    pub edge: Color32,
    pub light: Color32,
    pub dark: Color32,
    pub shadow: Color32,
    pub track: Color32,
    pub title_fill: Color32,
    pub title_ink: Color32,
}

impl Theme {
    /// The panel look for a perception level: rough and earthy at first, plainer and cooler as the tribe learns.
    pub fn era(level: u8) -> Self {
        let shadow = Color32::from_black_alpha(150);
        match level {
            0 | 1 => Theme {
                fill: Color32::from_rgba_unmultiplied(44, 32, 21, 238),
                edge: INK,
                light: Color32::from_rgb(150, 108, 66),
                dark: Color32::from_rgb(80, 54, 32),
                shadow,
                track: Color32::from_rgb(18, 14, 11),
                title_fill: Color32::from_rgb(96, 68, 40),
                title_ink: PAPER,
            },
            2 | 3 => Theme {
                fill: Color32::from_rgba_unmultiplied(30, 24, 19, 238),
                edge: INK,
                light: Color32::from_rgb(124, 100, 68),
                dark: Color32::from_rgb(60, 48, 34),
                shadow,
                track: Color32::from_rgb(18, 14, 11),
                title_fill: Color32::from_rgb(74, 58, 40),
                title_ink: PAPER,
            },
            _ => Theme {
                fill: Color32::from_rgba_unmultiplied(24, 27, 33, 240),
                edge: Color32::from_rgb(10, 12, 16),
                light: Color32::from_rgb(112, 134, 160),
                dark: Color32::from_rgb(48, 58, 72),
                shadow,
                track: Color32::from_rgb(12, 14, 18),
                title_fill: Color32::from_rgb(52, 64, 82),
                title_ink: Color32::from_rgb(214, 226, 240),
            },
        }
    }
}

fn block(out: &mut Vec<Shape>, r: Rect, c: Color32) {
    if r.width() > 0.0 && r.height() > 0.0 {
        out.push(Shape::rect_filled(r, 0.0, c));
    }
}

fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
    Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1))
}

/// The chrome of a panel as flat blocks: hard shadow, ink outline with stepped corners, bevel, fill.
fn chrome(out: &mut Vec<Shape>, r: Rect, th: &Theme, px: Px) {
    let u = px.u;
    let s = 2.0 * u;
    // the shadow only where it shows, so it does not tint the translucent fill
    block(out, rect(r.right(), r.top() + s, r.right() + s, r.bottom() + s), th.shadow);
    block(out, rect(r.left() + s, r.bottom(), r.right(), r.bottom() + s), th.shadow);
    // outline, corner pixels left out
    block(out, rect(r.left() + u, r.top(), r.right() - u, r.top() + u), th.edge);
    block(out, rect(r.left() + u, r.bottom() - u, r.right() - u, r.bottom()), th.edge);
    block(out, rect(r.left(), r.top() + u, r.left() + u, r.bottom() - u), th.edge);
    block(out, rect(r.right() - u, r.top() + u, r.right(), r.bottom() - u), th.edge);
    // bevel: light on the top and left, dark on the bottom and right
    let i = r.shrink(u);
    block(out, rect(i.left(), i.top(), i.right(), i.top() + u), th.light);
    block(out, rect(i.left(), i.top() + u, i.left() + u, i.bottom()), th.light);
    block(out, rect(i.left() + u, i.bottom() - u, i.right(), i.bottom()), th.dark);
    block(out, rect(i.right() - u, i.top() + u, i.right(), i.bottom() - u), th.dark);
    block(out, i.shrink(u), th.fill);
}

/// A panel: chrome behind, ordinary egui widgets inside. With a `title` a strip across the top carries it.
pub fn frame<R>(ui: &mut Ui, th: &Theme, title: Option<&str>, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    let px = Px::of(ui.ctx());
    let back = ui.painter().add(Shape::Noop);
    let margin = (px.n(4.0).round() as i8).max(2);
    let mut strip: Option<Rect> = None;
    let inner = egui::Frame::new().inner_margin(margin).show(ui, |ui| {
        if let Some(t) = title {
            let r = ui.label(egui::RichText::new(t).strong().color(th.title_ink)).rect;
            strip = Some(r);
            ui.add_space(px.n(1.0));
        }
        add(ui)
    });
    let outer = px.outer(inner.response.rect);
    let mut shapes = Vec::with_capacity(12);
    chrome(&mut shapes, outer, th, px);
    if let Some(t) = strip {
        let i = outer.shrink(px.n(2.0));
        let bottom = px.snap(t.bottom() + px.n(1.0));
        block(&mut shapes, rect(i.left(), i.top(), i.right(), bottom), th.title_fill);
        block(&mut shapes, rect(i.left(), bottom, i.right(), bottom + px.u), th.edge);
    }
    ui.painter().set(back, Shape::Vec(shapes));
    inner
}

/// A one-pixel rule across the panel.
pub fn rule(ui: &mut Ui, th: &Theme) {
    let px = Px::of(ui.ctx());
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, px.n(3.0)), Sense::hover());
    let y = px.snap(r.center().y);
    let mut v = Vec::new();
    block(&mut v, rect(r.left(), y, r.right(), y + px.u), th.edge);
    block(&mut v, rect(r.left(), y + px.u, r.right(), y + 2.0 * px.u), shade(th.light, 0.5));
    ui.painter().extend(v);
}

/// Text on the grid with a one-pixel ink shadow.
pub fn text(ui: &Ui, pos: Pos2, align: Align2, s: &str, size: f32, col: Color32) {
    let px = Px::of(ui.ctx());
    let p = ui.painter();
    let pos = px.pos(pos);
    let shadow = Color32::from_rgba_unmultiplied(INK.r(), INK.g(), INK.b(), col.a());
    p.text(pos + Vec2::splat(px.u), align, s, FontId::proportional(size), shadow);
    p.text(pos, align, s, FontId::proportional(size), col);
}

/// A picture of characters: '.' empty, 'a' the main colour, 'd' a shade of it, 'w' paper, 'k' ink, 'b' blue.
pub fn sprite(ui: &Ui, origin: Pos2, rows: &[&str], main: Color32) {
    let px = Px::of(ui.ctx());
    let o = px.pos(origin);
    let pal = |c: char| match c {
        'a' => Some(main),
        'd' => Some(shade(main, 0.62)),
        'w' => Some(PAPER),
        'k' => Some(INK),
        'b' => Some(Color32::from_rgb(120, 170, 235)),
        _ => None,
    };
    let mut out = Vec::new();
    for (j, row) in rows.iter().enumerate() {
        let cs: Vec<char> = row.chars().collect();
        let mut i = 0;
        while i < cs.len() {
            let Some(col) = pal(cs[i]) else {
                i += 1;
                continue;
            };
            let mut e = i + 1;
            while e < cs.len() && cs[e] == cs[i] {
                e += 1;
            }
            let x = o.x + i as f32 * px.u;
            let y = o.y + j as f32 * px.u;
            block(&mut out, rect(x, y, x + (e - i) as f32 * px.u, y + px.u), col);
            i = e;
        }
    }
    ui.painter().extend(out);
}

pub fn sprite_size(px: Px, rows: &[&str]) -> Vec2 {
    Vec2::new(rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as f32 * px.u, rows.len() as f32 * px.u)
}

pub const SUN: &[&str] = &[
    ".....aa.....",
    ".a...aa...a.",
    "..a......a..",
    "...aaaaaa...",
    "...aaaaaa...",
    "aa.aaaaaad.a",
    "aa.aaaaaad.a",
    "...aaaaadd..",
    "...adddddd..",
    "..a......a..",
    ".a...aa...a.",
    ".....aa.....",
];
pub const MOON: &[&str] = &[
    "....aaaaa...",
    "..aaaaaa....",
    ".aaaaa......",
    ".aaaa.......",
    "aaaaa.......",
    "aaaaa.......",
    "aaaaa.......",
    "aaaaad......",
    ".aaaaad.....",
    ".aaaaaadd...",
    "..aaaaaaddd.",
    "....dddddd..",
];
pub const RAIN: &[&str] = &[
    "............",
    "....aaa.....",
    "..aaaaaaa...",
    ".aaaaaaaaaa.",
    "aaaaaaaaaaaa",
    ".dddddddddd.",
    "............",
    "..b..b..b...",
    ".b..b..b....",
    "..b..b..b...",
    ".b..b..b....",
    "............",
];
pub const SNOW: &[&str] = &[
    "............",
    "....aaa.....",
    "..aaaaaaa...",
    ".aaaaaaaaaa.",
    "aaaaaaaaaaaa",
    ".dddddddddd.",
    "............",
    "..w..w..w...",
    "...w..w..w..",
    "..w..w..w...",
    "...w..w..w..",
    "............",
];
pub const EYE: &[&str] = &[
    "...aaaaaa...",
    ".aaawwwwaaa.",
    "aawwwddwwwaa",
    "aawwwddwwwaa",
    ".aaawwwwaaa.",
    "...aaaaaa...",
];

/// Darken an opaque colour (egui's `gamma_multiply` would make it translucent).
fn shade(c: Color32, f: f32) -> Color32 {
    Color32::from_rgb((c.r() as f32 * f) as u8, (c.g() as f32 * f) as u8, (c.b() as f32 * f) as u8)
}

fn lighten(c: Color32, t: f32) -> Color32 {
    let m = |x: u8| (x as f32 + (255.0 - x as f32) * t) as u8;
    Color32::from_rgb(m(c.r()), m(c.g()), m(c.b()))
}

/// A sprite in a square of `cells` pixels, for the head of a panel row.
pub fn icon(ui: &mut Ui, rows: &[&str], main: Color32, cells: f32) -> Response {
    let px = Px::of(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(px.n(cells)), Sense::hover());
    let s = sprite_size(px, rows);
    sprite(ui, r.center() - s * 0.5, rows, main);
    resp
}

/// A meter: ink-framed track, a lit fill in whole pixels, quarter ticks, an optional label, a sprite at the left.
/// Pulses red when `warn` is set.
pub fn bar(ui: &mut Ui, th: &Theme, icon_rows: &[&str], label: &str, frac: f32, col: Color32, width: f32, warn: bool) -> Response {
    let px = Px::of(ui.ctx());
    let u = px.u;
    let frac = frac.clamp(0.0, 1.0);
    let (rect_all, resp) = ui.allocate_exact_size(Vec2::new(px.snap(width), px.n(11.0)), Sense::hover());
    let t = ui.input(|i| i.time) as f32;
    let pulse = if warn { 0.5 + 0.5 * (t * 6.0).sin() } else { 0.0 };
    let col = if warn { lerp(col, Color32::from_rgb(235, 60, 50), pulse) } else { col };
    let s = sprite_size(px, icon_rows);
    sprite(ui, Pos2::new(rect_all.left(), rect_all.center().y - s.y * 0.5), icon_rows, col);
    let track = rect(rect_all.left() + px.snap(s.x + 2.0 * u), rect_all.top() + px.n(1.0), rect_all.right(), rect_all.bottom() - px.n(1.0));
    let track = Rect::from_min_max(px.pos(track.min), px.pos(track.max));
    let mut v = Vec::new();
    let edge = if warn { lerp(th.dark, Color32::from_rgb(240, 70, 60), pulse) } else { th.edge };
    block(&mut v, track, edge);
    let inner = track.shrink(u);
    block(&mut v, inner, th.track);
    if frac > 0.0 {
        let fw = px.snap(inner.width() * frac).max(px.n(2.0)).min(inner.width());
        let f = rect(inner.left(), inner.top(), inner.left() + fw, inner.bottom());
        block(&mut v, f, shade(col, 0.82));
        block(&mut v, rect(f.left(), f.top(), f.right(), f.top() + u), lighten(col, 0.35));
        block(&mut v, rect(f.left(), f.bottom() - u, f.right(), f.bottom()), shade(col, 0.5));
    }
    for k in 1..4 {
        let x = px.snap(inner.left() + inner.width() * k as f32 / 4.0);
        block(&mut v, rect(x, inner.top() + u, x + u, inner.bottom() - u), Color32::from_black_alpha(70));
    }
    ui.painter().extend(v);
    if !label.is_empty() {
        text(ui, Pos2::new(track.left() + px.n(4.0), track.center().y), Align2::LEFT_CENTER, label, 22.0, Color32::from_rgb(246, 238, 222));
    }
    resp
}

fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// A row of square pips, `on` of `total` lit.
pub fn pips(ui: &mut Ui, th: &Theme, on: usize, total: usize, col: Color32) -> Response {
    let px = Px::of(ui.ctx());
    let u = px.u;
    let step = px.n(8.0);
    let (r, resp) = ui.allocate_exact_size(Vec2::new(step * total as f32 - px.n(2.0), px.n(7.0)), Sense::hover());
    let mut v = Vec::new();
    for i in 0..total {
        let x = r.left() + i as f32 * step;
        let b = rect(x, r.top(), x + px.n(6.0), r.top() + px.n(6.0));
        block(&mut v, b, th.edge);
        let n = b.shrink(u);
        if i < on {
            block(&mut v, n, col);
            block(&mut v, rect(n.left(), n.top(), n.right() - u, n.top() + u), lighten(col, 0.5));
            block(&mut v, rect(n.left(), n.bottom() - u, n.right(), n.bottom()), shade(col, 0.6));
        } else {
            block(&mut v, n, th.track);
        }
    }
    ui.painter().extend(v);
    resp
}

/// A small label plate.
pub fn chip(ui: &mut Ui, th: &Theme, label: &str, fill: Color32, ink: Color32) -> Response {
    let px = Px::of(ui.ctx());
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(22.0), ink);
    let want = Vec2::new(px.snap(galley.size().x + px.n(8.0)), px.snap(galley.size().y + px.n(3.0)));
    let (r, resp) = ui.allocate_exact_size(want, Sense::hover());
    let u = px.u;
    let mut v = Vec::new();
    block(&mut v, rect(r.left() + u, r.top(), r.right() - u, r.bottom()), th.edge);
    block(&mut v, rect(r.left(), r.top() + u, r.right(), r.bottom() - u), th.edge);
    let i = r.shrink(u);
    block(&mut v, rect(i.left() + u, i.top(), i.right() - u, i.bottom()), fill);
    block(&mut v, rect(i.left(), i.top() + u, i.right(), i.bottom() - u), fill);
    block(&mut v, rect(i.left() + u, i.top(), i.right() - u, i.top() + u), lighten(fill, 0.25));
    ui.painter().extend(v);
    ui.painter().galley(px.pos(r.center() - galley.size() / 2.0), galley, ink);
    resp
}

/// A beveled button or tab. `selected` shows it pressed in; it sits a pixel lower while held.
pub fn button(ui: &mut Ui, th: &Theme, label: &str, selected: bool, enabled: bool) -> Response {
    let px = Px::of(ui.ctx());
    let u = px.u;
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(22.0), Color32::WHITE);
    let want = Vec2::new(px.snap(galley.size().x + px.n(10.0)), px.snap(galley.size().y + px.n(6.0)));
    let (r, resp) = ui.allocate_exact_size(want, if enabled { Sense::click() } else { Sense::hover() });
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    let down = enabled && (selected || resp.is_pointer_button_down_on());
    let (face, light, dark) = if !enabled {
        (Color32::from_rgb(70, 62, 54), Color32::from_rgb(90, 80, 70), Color32::from_rgb(50, 44, 38))
    } else if down {
        (th.dark, shade(th.dark, 0.7), th.light)
    } else if resp.hovered() {
        (lighten(th.title_fill, 0.12), lighten(th.light, 0.2), th.dark)
    } else {
        (th.title_fill, th.light, th.dark)
    };
    let mut v = Vec::new();
    chrome_flat(&mut v, r, th.edge, face, light, dark, px);
    ui.painter().extend(v);
    let c = r.center() + if down { Vec2::splat(u) } else { Vec2::ZERO };
    text(ui, c, Align2::CENTER_CENTER, label, 22.0, if enabled { th.title_ink } else { Color32::from_gray(140) });
    resp
}

/// Ink outline with stepped corners, bevel edges and a flat face, all opaque.
fn chrome_flat(out: &mut Vec<Shape>, r: Rect, edge: Color32, face: Color32, light: Color32, dark: Color32, px: Px) {
    let u = px.u;
    block(out, rect(r.left() + u, r.top(), r.right() - u, r.bottom()), edge);
    block(out, rect(r.left(), r.top() + u, r.right(), r.bottom() - u), edge);
    let i = r.shrink(u);
    block(out, i, face);
    block(out, rect(i.left(), i.top(), i.right(), i.top() + u), light);
    block(out, rect(i.left(), i.top(), i.left() + u, i.bottom()), light);
    block(out, rect(i.left(), i.bottom() - u, i.right(), i.bottom()), dark);
    block(out, rect(i.right() - u, i.top(), i.right(), i.bottom()), dark);
}
