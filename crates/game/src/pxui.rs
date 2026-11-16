//! Pixel-grid painting for the hand-work minigames. Everything here sits on a 2-point pixel (the same pixel the
//! game's font uses at sizes 11 and 22): rectangles are snapped, curves are filled cell by cell and little
//! pictures are drawn from character bitmaps, so nothing is smooth, anti-aliased or off-grid.

use alaz_render::egui::{self, Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Ui, Vec2};

thread_local! {
    /// One pixel of the minigame art, in points: 2 at scale 1, and a whole number of screen pixels at any other
    /// scale (`set_unit` is called once per frame from the interface scale). Kept per thread: the interface runs on
    /// one, and tests that build interfaces of other sizes side by side must not change each other's unit.
    static UNIT: std::cell::Cell<f32> = const { std::cell::Cell::new(2.0) };
}

pub fn unit() -> f32 {
    UNIT.with(|u| u.get())
}

pub fn set_unit(u: f32) {
    UNIT.with(|c| c.set(u));
}

pub const INK: Color32 = Color32::from_rgb(24, 17, 11);
pub const WELL: Color32 = Color32::from_rgb(52, 42, 33);
pub const WELL_LIGHT: Color32 = Color32::from_rgb(98, 76, 52);
pub const WOOD_D: Color32 = Color32::from_rgb(84, 58, 36);
pub const WOOD: Color32 = Color32::from_rgb(120, 86, 52);
pub const WOOD_L: Color32 = Color32::from_rgb(158, 118, 76);
pub const STONE_D: Color32 = Color32::from_rgb(70, 68, 64);
pub const STONE: Color32 = Color32::from_rgb(138, 134, 128);
pub const STONE_L: Color32 = Color32::from_rgb(190, 186, 176);
pub const GOOD: Color32 = Color32::from_rgb(108, 214, 118);
pub const GOOD_L: Color32 = Color32::from_rgb(170, 250, 170);
pub const BAD: Color32 = Color32::from_rgb(232, 94, 84);
pub const GOLD: Color32 = Color32::from_rgb(250, 222, 110);
pub const PAPER: Color32 = Color32::from_rgb(226, 210, 176);

pub fn snap(v: f32) -> f32 {
    (v / unit()).round() * unit()
}

pub fn snap_pos(p: Pos2) -> Pos2 {
    Pos2::new(snap(p.x), snap(p.y))
}

pub fn snap_rect(r: Rect) -> Rect {
    Rect::from_min_max(snap_pos(r.min), snap_pos(r.max))
}

/// How the last blow went, as the colour a target takes: green after a hit, red after a miss.
pub fn verdict(last: Option<(bool, f32)>, idle: Color32) -> Color32 {
    match last {
        Some((true, _)) => GOOD_L,
        Some((false, _)) => BAD,
        None => idle,
    }
}

pub fn fill(p: &Painter, r: Rect, c: Color32) {
    let r = snap_rect(r);
    if r.width() > 0.0 && r.height() > 0.0 {
        p.rect_filled(r, 0.0, c);
    }
}

/// A hollow rectangle, `t` pixels thick, drawn inside `r`.
pub fn outline(p: &Painter, r: Rect, t: f32, c: Color32) {
    let r = snap_rect(r);
    let t = t * unit();
    fill(p, Rect::from_min_max(r.min, egui::pos2(r.right(), r.top() + t)), c);
    fill(p, Rect::from_min_max(egui::pos2(r.left(), r.bottom() - t), r.max), c);
    fill(p, Rect::from_min_max(egui::pos2(r.left(), r.top() + t), egui::pos2(r.left() + t, r.bottom() - t)), c);
    fill(p, Rect::from_min_max(egui::pos2(r.right() - t, r.top() + t), egui::pos2(r.right(), r.bottom() - t)), c);
}

/// A raised block: ink outline, a light top-left edge and a dark bottom-right edge.
pub fn bevel(p: &Painter, r: Rect, face: Color32, light: Color32, dark: Color32) {
    let r = snap_rect(r);
    outline(p, r, 1.0, INK);
    let i = r.shrink(unit());
    fill(p, i, face);
    fill(p, Rect::from_min_max(i.min, egui::pos2(i.right(), i.top() + unit())), light);
    fill(p, Rect::from_min_max(i.min, egui::pos2(i.left() + unit(), i.bottom())), light);
    fill(p, Rect::from_min_max(egui::pos2(i.left(), i.bottom() - unit()), i.max), dark);
    fill(p, Rect::from_min_max(egui::pos2(i.right() - unit(), i.top()), i.max), dark);
}

pub fn hash(x: i32, y: i32, seed: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA6B) ^ seed.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 13)
}

/// The sunken work surface every minigame is played on: dark dust with sparse specks, a pressed-in edge.
pub fn well(p: &Painter, r: Rect, seed: u32) {
    let r = snap_rect(r);
    fill(p, r, WELL);
    // sparse specks: one in every ~29 cells, placed by hash, not found by testing every cell
    let (nx, ny) = (((r.width() / unit()) as u32).max(5), ((r.height() / unit()) as u32).max(5));
    for i in 0..(nx * ny / 29) {
        let h = hash(i as i32, 0, seed);
        let (cx, cy) = (2 + h % (nx - 4), 2 + (h >> 11) % (ny - 4));
        let c = if h & (1 << 22) == 0 { Color32::from_rgb(62, 50, 39) } else { Color32::from_rgb(44, 35, 27) };
        fill(p, Rect::from_min_size(egui::pos2(r.left() + cx as f32 * unit(), r.top() + cy as f32 * unit()), Vec2::splat(unit())), c);
    }
    fill(p, Rect::from_min_max(r.min, egui::pos2(r.right(), r.top() + unit())), INK);
    fill(p, Rect::from_min_max(r.min, egui::pos2(r.left() + unit(), r.bottom())), INK);
    fill(p, Rect::from_min_max(egui::pos2(r.left(), r.bottom() - unit()), r.max), WELL_LIGHT);
    fill(p, Rect::from_min_max(egui::pos2(r.right() - unit(), r.top()), r.max), WELL_LIGHT);
}

/// Fill every cell of `bounds` for which `f(cell centre)` gives a colour; equal neighbours in a row become one
/// rectangle. This is how discs, rings and the stone are drawn without a single smooth edge.
pub fn cells(p: &Painter, bounds: Rect, f: impl Fn(Pos2) -> Option<Color32>) {
    let mut out = Vec::new();
    cells_into(&mut out, bounds, f);
    p.extend(out);
}

/// As `cells`, but the shapes are kept between frames and only rebuilt when `key` changes: a picture that does not
/// change (the stone between two blows) costs a copy, not thousands of colour lookups.
pub fn cells_cached(ctx: &egui::Context, id: egui::Id, key: u64, p: &Painter, bounds: Rect, f: impl Fn(Pos2) -> Option<Color32>) {
    // a change of interface scale changes the pixel, so it is part of the key
    let key = key.wrapping_mul(1_000_003).wrapping_add(unit().to_bits() as u64);
    let kept: Option<(u64, Vec<egui::Shape>)> = ctx.data(|d| d.get_temp(id));
    let shapes = match kept {
        Some((k, s)) if k == key => s,
        _ => {
            let mut out = Vec::new();
            cells_into(&mut out, bounds, f);
            ctx.data_mut(|d| d.insert_temp(id, (key, out.clone())));
            out
        }
    };
    p.extend(shapes);
}

fn cells_into(out: &mut Vec<egui::Shape>, bounds: Rect, f: impl Fn(Pos2) -> Option<Color32>) {
    let b = snap_rect(bounds);
    let (nx, ny) = ((b.width() / unit()) as i32, (b.height() / unit()) as i32);
    for j in 0..ny {
        let y = b.top() + j as f32 * unit();
        let mut run: Option<(i32, Color32)> = None;
        for i in 0..=nx {
            let col = if i < nx { f(egui::pos2(b.left() + (i as f32 + 0.5) * unit(), y + unit() * 0.5)) } else { None };
            match (run, col) {
                (Some((s, c)), Some(n)) if c == n => run = Some((s, c)),
                (Some((s, c)), _) => {
                    out.push(egui::Shape::rect_filled(Rect::from_min_max(egui::pos2(b.left() + s as f32 * unit(), y), egui::pos2(b.left() + i as f32 * unit(), y + unit())), 0.0, c));
                    run = col.map(|n| (i, n));
                }
                (None, Some(n)) => run = Some((i, n)),
                (None, None) => {}
            }
        }
    }
}

/// A circle outline `t` pixels thick on the pixel grid.
pub fn ring(p: &Painter, c: Pos2, r: f32, t: f32, col: Color32) {
    let c = snap_pos(c);
    let t = t * unit();
    cells(p, Rect::from_center_size(c, Vec2::splat(r * 2.0 + unit() * 4.0)), |q| {
        let d = (q - c).length();
        (d <= r && d > r - t).then_some(col)
    });
}

/// A dotted straight line, one pixel dots with a gap between.
pub fn dotted(p: &Painter, a: Pos2, b: Pos2, col: Color32) {
    let n = ((b - a).length() / (unit() * 4.0)).floor() as i32;
    for i in 0..=n {
        let q = a + (b - a) * (i as f32 / n.max(1) as f32);
        fill(p, Rect::from_center_size(snap_pos(q), Vec2::splat(unit())), col);
    }
}

/// Text on the pixel grid with a one-pixel shadow. Only sizes 11 and 22 sit on whole font pixels.
pub fn text(p: &Painter, pos: Pos2, align: Align2, s: &str, size: f32, col: Color32) {
    let pos = snap_pos(pos);
    let shadow = Color32::from_rgba_unmultiplied(INK.r(), INK.g(), INK.b(), col.a());
    p.text(pos + Vec2::splat(unit()), align, s, FontId::proportional(size), shadow);
    p.text(pos, align, s, FontId::proportional(size), col);
}

/// A picture drawn from characters: '.' is empty, every other character is looked up in `pal`.
pub fn sprite(p: &Painter, origin: Pos2, rows: &[&str], pal: &[(char, Color32)], cell: f32) {
    let o = snap_pos(origin);
    for (j, row) in rows.iter().enumerate() {
        let mut start: Option<(usize, char)> = None;
        let chars: Vec<char> = row.chars().collect();
        for i in 0..=chars.len() {
            let ch = chars.get(i).copied().filter(|c| *c != '.');
            match (start, ch) {
                (Some((_, a)), Some(b)) if a == b => {}
                (Some((s, a)), _) => {
                    if let Some(&(_, col)) = pal.iter().find(|(k, _)| *k == a) {
                        let r = Rect::from_min_max(
                            egui::pos2(o.x + s as f32 * cell, o.y + j as f32 * cell),
                            egui::pos2(o.x + i as f32 * cell, o.y + (j + 1) as f32 * cell),
                        );
                        p.rect_filled(r, 0.0, col);
                    }
                    start = ch.map(|b| (i, b));
                }
                (None, Some(b)) => start = Some((i, b)),
                (None, None) => {}
            }
        }
    }
}

pub fn sprite_size(rows: &[&str], cell: f32) -> Vec2 {
    Vec2::new(rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as f32 * cell, rows.len() as f32 * cell)
}

/// Draw a sprite so that its middle is at `c`.
pub fn sprite_at(p: &Painter, c: Pos2, rows: &[&str], pal: &[(char, Color32)], cell: f32) {
    let s = sprite_size(rows, cell);
    sprite(p, c - s * 0.5, rows, pal, cell);
}

const STONE_PAL: &[(char, Color32)] = &[('D', STONE_D), ('M', STONE), ('L', STONE_L), ('i', INK)];

/// The hammerstone held for knapping.
pub const HAMMERSTONE: &[&str] = &[
    "...iiiiiii...",
    "..iLLLLMMMi..",
    ".iLLLMMMMMDi.",
    "iLLMMMMMMMDDi",
    "iLMMMMMMMDDDi",
    "iMMMMMMMDDDDi",
    ".iMMMMDDDDDi.",
    "..iDDDDDDDi..",
    "...iiiiiii...",
];

/// The stone head carried to its handle.
pub const STONE_HEAD: &[&str] = &[
    "..iiiiiii..",
    ".iLLLMMMMi.",
    "iLLMMMMMMDi",
    "iLMMMMMMDDi",
    "iMMMMMMDDDi",
    ".iMMMDDDDi.",
    "..iiDDDii..",
    "....iii....",
];

/// The scraper that sweeps along the plank: a handle above, a stone blade below, the tip is the point that counts.
pub const SCRAPER: &[&str] = &[
    "..iwi..",
    "..iWi..",
    "..iwi..",
    "..iWi..",
    "..iwi..",
    ".iiFii.",
    ".iFFFi.",
    "..iLMi.",
    "..iMDi.",
    "...iDi.",
    "...iii.",
];

pub fn stone_pal() -> Vec<(char, Color32)> {
    let mut v = STONE_PAL.to_vec();
    v.push(('w', WOOD));
    v.push(('W', WOOD_L));
    v.push(('F', Color32::from_rgb(194, 206, 120)));
    v
}

/// A small check mark, for a part that is done.
pub const CHECK: &[&str] = &["......G", ".....GG", "G...GG.", "GG.GG..", ".GGG...", "..G...."];

/// The state of a single blow in the row under the header.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Pip {
    Hit,
    Miss,
    Todo,
}

pub fn pip(p: &Painter, r: Rect, state: Pip) {
    let r = snap_rect(r);
    match state {
        Pip::Hit => {
            bevel(p, r, GOOD, GOOD_L, Color32::from_rgb(52, 130, 64));
        }
        Pip::Miss => {
            bevel(p, r, BAD, Color32::from_rgb(250, 150, 140), Color32::from_rgb(130, 40, 40));
        }
        Pip::Todo => {
            fill(p, r, Color32::from_rgb(132, 108, 78));
            fill(p, r.shrink(unit()), Color32::from_rgb(40, 32, 24));
        }
    }
}

/// A straight meter, vertical (0 at the bottom) or horizontal (0 at the left), with the band to hit in green and
/// a white pointer for the present value. `band` and `at` are 0..=100.
pub fn meter(p: &Painter, r: Rect, vertical: bool, band: (f32, f32), band_col: Color32, at: Option<f32>) {
    let r = snap_rect(r);
    well(p, r, 7);
    let inner = r.shrink(unit() * 2.0);
    let pos = |v: f32| -> f32 {
        let t = (v / 100.0).clamp(0.0, 1.0);
        if vertical { inner.bottom() - inner.height() * t } else { inner.left() + inner.width() * t }
    };
    // scale marks every tenth, longer at the half
    for k in 0..=10 {
        let v = k as f32 * 10.0;
        let len = if k % 5 == 0 { unit() * 4.0 } else { unit() * 2.0 };
        let m = if vertical {
            Rect::from_min_size(egui::pos2(inner.left(), pos(v) - unit() * 0.5), Vec2::new(len, unit()))
        } else {
            Rect::from_min_size(egui::pos2(pos(v) - unit() * 0.5, inner.bottom() - len), Vec2::new(unit(), len))
        };
        fill(p, m, Color32::from_rgb(120, 98, 70));
    }
    let b = if vertical {
        Rect::from_min_max(egui::pos2(inner.left(), pos(band.1)), egui::pos2(inner.right(), pos(band.0)))
    } else {
        Rect::from_min_max(egui::pos2(pos(band.0), inner.top()), egui::pos2(pos(band.1), inner.bottom()))
    };
    fill(p, b, band_col);
    // a checkered shine inside the band so it reads as a target, not a bar
    let b = snap_rect(b);
    let (nx, ny) = ((b.width() / unit()) as i32, (b.height() / unit()) as i32);
    for j in 0..ny {
        for i in 0..nx {
            if (i + j) % 2 == 0 {
                p.rect_filled(Rect::from_min_size(egui::pos2(b.left() + i as f32 * unit(), b.top() + j as f32 * unit()), Vec2::splat(unit())), 0.0, Color32::from_white_alpha(34));
            }
        }
    }
    outline(p, b, 1.0, Color32::from_black_alpha(120));
    if let Some(v) = at {
        let a = pos(v);
        if vertical {
            fill(p, Rect::from_min_max(egui::pos2(r.left() - unit() * 2.0, a - unit()), egui::pos2(r.right() + unit() * 2.0, a + unit())), Color32::WHITE);
            fill(p, Rect::from_min_max(egui::pos2(r.left() - unit() * 2.0, a + unit()), egui::pos2(r.right() + unit() * 2.0, a + unit() * 2.0)), INK);
        } else {
            fill(p, Rect::from_min_max(egui::pos2(a - unit(), r.top() - unit() * 2.0), egui::pos2(a + unit(), r.bottom() + unit() * 2.0)), Color32::WHITE);
            fill(p, Rect::from_min_max(egui::pos2(a + unit(), r.top() - unit() * 2.0), egui::pos2(a + unit() * 2.0, r.bottom() + unit() * 2.0)), INK);
        }
    }
}

/// A beveled button. Gives the click; the label sits a pixel lower while it is held.
pub fn button(ui: &mut Ui, label: &str, size: f32, enabled: bool) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(size), Color32::WHITE);
    let want = egui::vec2(snap(galley.size().x + unit() * 10.0), snap(galley.size().y + unit() * 6.0));
    let (rect, resp) = ui.allocate_exact_size(want, if enabled { Sense::click() } else { Sense::hover() });
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    let down = enabled && resp.is_pointer_button_down_on();
    // the button takes the era's panel colours
    let th = crate::pxpanel::Theme::era(crate::hud::ERA.load(std::sync::atomic::Ordering::Relaxed));
    let (face, light, dark) = if !enabled {
        (Color32::from_rgb(70, 62, 54), Color32::from_rgb(90, 80, 70), Color32::from_rgb(50, 44, 38))
    } else if down {
        (th.dark, th.edge, th.light)
    } else if resp.hovered() {
        (crate::pxpanel::lighten(th.title_fill, 0.12), crate::pxpanel::lighten(th.light, 0.2), th.dark)
    } else {
        (th.title_fill, th.light, th.dark)
    };
    // a button picture (`assets/ui/button.png`, with `button_hover.png` and `button_down.png` if wanted) replaces the bevel
    let era = crate::hud::ERA.load(std::sync::atomic::Ordering::Relaxed);
    let state = if !enabled { "off" } else if down { "down" } else if resp.hovered() { "hover" } else { "" };
    let pic = if crate::art::any() {
        let named = |n: &str| crate::art::era_texture(ui.ctx(), n, era);
        let base = || named("button");
        match state {
            "" => base().map(|t| (t, Color32::WHITE)),
            "hover" => named("button_hover").map(|t| (t, Color32::WHITE)).or_else(|| base().map(|t| (t, Color32::WHITE))),
            "down" => named("button_down").map(|t| (t, Color32::WHITE)).or_else(|| base().map(|t| (t, Color32::from_gray(190)))),
            _ => named("button_off").map(|t| (t, Color32::WHITE)).or_else(|| base().map(|t| (t, Color32::from_gray(120)))),
        }
    } else {
        None
    };
    match pic {
        Some(((tex, size), tint)) => {
            for sh in crate::art::nine_slice(&tex, size, rect, unit(), tint) {
                ui.painter().add(sh);
            }
        }
        None => bevel(ui.painter(), rect, face, light, dark),
    }
    let c = rect.center() + if down { Vec2::splat(unit()) } else { Vec2::ZERO };
    text(ui.painter(), c, Align2::CENTER_CENTER, label, size, if enabled { th.title_ink } else { Color32::from_gray(140) });
    resp
}

/// Chips thrown off a good blow, as little squares that arc outward and fade.
pub fn chips(p: &Painter, at: Pos2, age: f32, alpha: f32, n: usize) {
    for k in 0..n {
        let a = k as f32 * 0.63 + 0.3;
        let (dx, dy) = (a.cos() * 70.0 * age, -a.sin().abs() * 60.0 * age + 80.0 * age * age);
        let tone = if k % 3 == 0 { WOOD_L } else if k % 3 == 1 { Color32::from_rgb(214, 168, 110) } else { PAPER };
        let c = Color32::from_rgba_unmultiplied(tone.r(), tone.g(), tone.b(), (alpha * 255.0) as u8);
        fill(p, Rect::from_center_size(at + egui::vec2(dx, dy), Vec2::splat(if k % 2 == 0 { unit() * 2.0 } else { unit() })), c);
    }
}

/// Shows whether the other hand is holding the work: green while the grip key is down, red when it is not.
pub fn grip_badge(ui: &mut Ui, held: bool, key: &str) {
    let label = if held { format!("Tutuyorsun [{key}]") } else { format!("Tut! [{key}]") };
    let galley = ui.painter().layout_no_wrap(label.clone(), FontId::proportional(22.0), Color32::WHITE);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(snap(galley.size().x + unit() * 10.0), snap(galley.size().y + unit() * 6.0)), Sense::hover());
    let (face, light, dark) = if held {
        (Color32::from_rgb(70, 150, 84), GOOD_L, Color32::from_rgb(40, 96, 52))
    } else {
        (Color32::from_rgb(170, 64, 56), Color32::from_rgb(240, 140, 128), Color32::from_rgb(110, 36, 36))
    };
    bevel(ui.painter(), rect, face, light, dark);
    text(ui.painter(), rect.center(), Align2::CENTER_CENTER, &label, 22.0, PAPER);
}
