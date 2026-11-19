//! The look of the inventory window and the book: era palettes, pixel frames and slots, machine families.
//! Everything sits on the interface pixel (`pxui::unit()`): flat colour bands, hard or stepped corners, no gradients.

use alaz_render::egui::{self, Color32, Painter, Pos2, Rect, Shape, Vec2};
use alaz_sim::BuildingKind;

use crate::pxui::{snap_rect, unit};

pub(crate) mod icons;
pub(crate) mod lines;
pub(crate) mod machines;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Era {
    Stone,
    Metal,
    Machine,
    Computer,
}

/// The look for a perception level: 0-1 stone, 2-3 metal, 4 machine, 5 and up computer.
pub(crate) fn era_of(perception: u8) -> Era {
    match perception {
        0 | 1 => Era::Stone,
        2 | 3 => Era::Metal,
        4 => Era::Machine,
        _ => Era::Computer,
    }
}

thread_local! {
    /// The era `--ui-shot` forces while it draws; per thread, like `hud::ERA`, so tests do not see each other's.
    static FORCED: std::cell::Cell<Option<u8>> = const { std::cell::Cell::new(None) };
}

/// Forces the perception level the window and the HUD dress for (None: the world's own).
pub(crate) fn force_level(level: Option<u8>) {
    FORCED.with(|c| c.set(level));
}

fn env_level() -> Option<u8> {
    static ENV: std::sync::OnceLock<Option<u8>> = std::sync::OnceLock::new();
    *ENV.get_or_init(|| std::env::var("ALAZ_DEMO_ERA").ok().and_then(|v| v.trim().parse::<u8>().ok()).map(|v| v.min(5)))
}

/// The level the interface dresses for: a forced one, else `ALAZ_DEMO_ERA=0..5`, else `perception`.
pub(crate) fn era_level(perception: u8) -> u8 {
    FORCED.with(|c| c.get()).or_else(env_level).unwrap_or(perception)
}

/// The era look right now: `hud::ERA` (set each frame from the world), overridden by `ALAZ_DEMO_ERA`.
pub(crate) fn current_era() -> Era {
    era_of(era_level(crate::hud::ERA.load(std::sync::atomic::Ordering::Relaxed)))
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Palette {
    pub base: Color32,
    pub base_dark: Color32,
    pub light: Color32,
    pub edge: Color32,
    pub slot: Color32,
    pub slot_shadow: Color32,
    pub ink: Color32,
    pub ink_dim: Color32,
    pub good: Color32,
    pub warn: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub(crate) fn palette(era: Era) -> Palette {
    match era {
        // tanned hide on a bark rim; light ink, because egui's own widgets inside the window write light text
        Era::Stone => Palette {
            base: rgb(112, 82, 52),
            base_dark: rgb(88, 62, 38),
            light: rgb(170, 132, 86),
            edge: rgb(46, 32, 20),
            slot: rgb(62, 44, 28),
            slot_shadow: rgb(34, 23, 14),
            ink: rgb(248, 236, 210),
            ink_dim: rgb(206, 182, 140),
            good: rgb(150, 214, 110),
            warn: rgb(246, 150, 70),
        },
        Era::Metal => Palette {
            base: rgb(84, 86, 92),
            base_dark: rgb(70, 72, 78),
            light: rgb(146, 150, 158),
            edge: rgb(30, 31, 34),
            slot: rgb(42, 43, 47),
            slot_shadow: rgb(16, 16, 18),
            ink: rgb(242, 238, 228),
            ink_dim: rgb(186, 186, 184),
            good: rgb(130, 220, 130),
            warn: rgb(255, 150, 60),
        },
        Era::Machine => Palette {
            base: rgb(34, 54, 76),
            base_dark: rgb(48, 72, 98),
            light: rgb(166, 192, 214),
            edge: rgb(14, 24, 34),
            slot: rgb(22, 36, 52),
            slot_shadow: rgb(10, 18, 27),
            ink: rgb(234, 242, 250),
            ink_dim: rgb(160, 186, 208),
            good: rgb(120, 230, 160),
            warn: rgb(255, 180, 90),
        },
        Era::Computer => Palette {
            base: rgb(8, 24, 14),
            base_dark: rgb(5, 17, 10),
            light: rgb(70, 200, 110),
            edge: rgb(30, 62, 38),
            slot: rgb(12, 40, 22),
            slot_shadow: rgb(3, 10, 6),
            ink: rgb(130, 255, 158),
            ink_dim: rgb(70, 170, 98),
            good: rgb(170, 255, 120),
            warn: rgb(255, 200, 80),
        },
    }
}

/// egui's own widgets inside the window (buttons, labels, scroll bars) in the era's colours.
pub(crate) fn apply_visuals(v: &mut egui::Visuals, era: Era) {
    let pal = palette(era);
    v.override_text_color = Some(pal.ink);
    v.extreme_bg_color = pal.slot_shadow;
    v.faint_bg_color = pal.base_dark;
    v.panel_fill = pal.base;
    v.window_fill = pal.base;
    v.selection.bg_fill = mix(pal.slot, pal.light, 0.45);
    v.selection.stroke.color = pal.ink;
    v.hyperlink_color = pal.light;
    let w = &mut v.widgets;
    w.noninteractive.bg_stroke.color = pal.edge;
    w.noninteractive.fg_stroke.color = pal.ink;
    for (st, fill, edge) in [(&mut w.inactive, pal.slot, pal.edge), (&mut w.hovered, mix(pal.slot, pal.light, 0.25), pal.light), (&mut w.active, mix(pal.slot, pal.light, 0.4), pal.ink), (&mut w.open, pal.slot, pal.light)] {
        st.bg_fill = fill;
        st.weak_bg_fill = fill;
        st.bg_stroke.color = edge;
        st.fg_stroke.color = pal.ink;
    }
}

pub(crate) fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

fn rect(out: &mut Vec<Shape>, r: Rect, c: Color32) {
    let r = snap_rect(r);
    if r.width() > 0.0 && r.height() > 0.0 {
        out.push(Shape::rect_filled(r, 0.0, c));
    }
}

fn xywh(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h))
}

/// `r` filled with its corners cut in `steps` pixel stairs: the pixel-art "rounded" corner.
fn stepped(out: &mut Vec<Shape>, r: Rect, steps: usize, c: Color32) {
    let u = unit();
    let r = snap_rect(r);
    if steps == 0 {
        rect(out, r, c);
        return;
    }
    // the middle band, then one narrower band per step above and below it
    let s = steps as f32 * u;
    rect(out, Rect::from_min_max(Pos2::new(r.left(), r.top() + s), Pos2::new(r.right(), r.bottom() - s)), c);
    for i in 0..steps {
        let inset = (steps - i) as f32 * u;
        let y = i as f32 * u;
        rect(out, xywh(r.left() + inset, r.top() + y, r.width() - 2.0 * inset, u), c);
        rect(out, xywh(r.left() + inset, r.bottom() - y - u, r.width() - 2.0 * inset, u), c);
    }
}

/// A hollow pixel border `t` pixels thick inside `r`, with the corner pixel left out when `cut`.
fn border(out: &mut Vec<Shape>, r: Rect, t: f32, c: Color32, cut: bool) {
    let u = unit();
    let r = snap_rect(r);
    let t = t * u;
    let k = if cut { t } else { 0.0 };
    rect(out, Rect::from_min_max(Pos2::new(r.left() + k, r.top()), Pos2::new(r.right() - k, r.top() + t)), c);
    rect(out, Rect::from_min_max(Pos2::new(r.left() + k, r.bottom() - t), Pos2::new(r.right() - k, r.bottom())), c);
    rect(out, Rect::from_min_max(Pos2::new(r.left(), r.top() + t), Pos2::new(r.left() + t, r.bottom() - t)), c);
    rect(out, Rect::from_min_max(Pos2::new(r.right() - t, r.top() + t), Pos2::new(r.right(), r.bottom() - t)), c);
}

/// Builds `make` once per (rect, era, pixel size, key) and replays it: a full window of texture is a few
/// thousand rectangles, too many to rebuild every frame.
fn cached(p: &Painter, salt: &str, r: Rect, era: Era, key: u64, make: impl FnOnce(&mut Vec<Shape>)) {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for v in [r.min.x, r.min.y, r.max.x, r.max.y, unit()] {
        v.to_bits().hash(&mut h);
    }
    (era as u8, key).hash(&mut h);
    let stamp = h.finish();
    let id = egui::Id::NULL.with(("style_cache", salt));
    let kept: Option<(u64, std::sync::Arc<Vec<Shape>>)> = p.ctx().data(|d| d.get_temp(id));
    let shapes = match kept {
        Some((s, shapes)) if s == stamp => shapes,
        _ => {
            let mut out = Vec::new();
            make(&mut out);
            let shapes = std::sync::Arc::new(out);
            p.ctx().data_mut(|d| d.insert_temp(id, (stamp, shapes.clone())));
            shapes
        }
    };
    p.extend(shapes.iter().cloned());
}

/// The window body in the era's material: stitched hide on a bark rim, riveted iron plates, a blueprint sheet on a
/// steel rule, or a screen with scanlines. `accent` (the machine family's colour) runs along the top edge.
pub(crate) fn frame(p: &Painter, r: Rect, era: Era, accent: Option<Color32>) {
    let key = accent.map_or(0, |c| u32::from_le_bytes(c.to_array()) as u64 + 1);
    let salt = format!("frame{:.0}x{:.0}", r.min.x, r.min.y);
    cached(p, &salt, r, era, key, |out| frame_shapes(out, r, era, accent));
}

fn frame_shapes(out: &mut Vec<Shape>, r: Rect, era: Era, accent: Option<Color32>) {
    let u = unit();
    let pal = palette(era);
    let r = snap_rect(r);
    // a hard pixel shadow, down and right
    let shadow = Color32::from_black_alpha(110);
    rect(out, xywh(r.left() + 3.0 * u, r.bottom(), r.width(), 3.0 * u), shadow);
    rect(out, xywh(r.right(), r.top() + 3.0 * u, 3.0 * u, r.height() - 3.0 * u), shadow);
    match era {
        Era::Stone => {
            stepped(out, r, 3, pal.edge);
            // bark rim: vertical fibres in two browns, broken by short cracks
            let rim = 7.0 * u;
            let bark_l = rgb(112, 80, 48);
            let bark_d = rgb(28, 18, 10);
            let body = r.shrink(u);
            let fibres = |out: &mut Vec<Shape>, band: Rect, seed: u32| {
                let mut x = band.left() + 2.0 * u;
                let mut i = 0;
                while x < band.right() - 2.0 * u {
                    let h = crate::pxui::hash(i, 3, seed);
                    let len = ((2 + h % 5) as f32 * u).min(band.height());
                    let y = band.top() + ((h >> 8) % 3) as f32 * u;
                    rect(out, xywh(x, y.min(band.bottom() - len), u, len), if h & 16 == 0 { bark_l } else { bark_d });
                    x += (2 + (h >> 12) % 3) as f32 * u;
                    i += 1;
                }
            };
            fibres(out, xywh(body.left() + 3.0 * u, body.top() + u, body.width() - 6.0 * u, rim - u), 11);
            fibres(out, xywh(body.left() + 3.0 * u, body.bottom() - rim, body.width() - 6.0 * u, rim - u), 12);
            let mut y = body.top() + rim;
            let mut i = 0;
            while y < body.bottom() - rim {
                let h = crate::pxui::hash(i, 9, 13);
                let len = ((2 + h % 5) as f32 * u).min(rim);
                rect(out, xywh(body.left() + u + ((h >> 8) % 3) as f32 * u, y, len, u), if h & 16 == 0 { bark_l } else { bark_d });
                rect(out, xywh(body.right() - u - len - ((h >> 10) % 3) as f32 * u, y + u, len, u), if h & 32 == 0 { bark_l } else { bark_d });
                y += (2 + (h >> 12) % 3) as f32 * u;
                i += 1;
            }
            let hide = r.shrink(rim + u);
            stepped(out, hide, 3, pal.base);
            // hide grain: uneven darker streaks and a few pale scuffs, staggered row to row
            let scuff = mix(pal.base, pal.light, 0.35);
            let mut y = hide.top() + 3.0 * u;
            let mut row = 0;
            while y < hide.bottom() - 3.0 * u {
                let mut x = hide.left() + 3.0 * u + (row % 4) as f32 * 3.0 * u;
                let mut j = 0;
                while x < hide.right() - 3.0 * u {
                    let h = crate::pxui::hash(j, row, 3);
                    let len = (6 + h % 22) as f32 * u;
                    let end = (x + len).min(hide.right() - 3.0 * u);
                    if h % 5 < 3 {
                        rect(out, Rect::from_min_max(Pos2::new(x, y), Pos2::new(end, y + u)), pal.base_dark);
                    } else if h % 5 == 3 {
                        rect(out, Rect::from_min_max(Pos2::new(x, y), Pos2::new((x + len * 0.4).min(end), y + u)), scuff);
                    }
                    x += len + (3 + (h >> 7) % 8) as f32 * u;
                    j += 1;
                }
                y += 2.0 * u;
                row += 1;
            }
            // lacing: pale thongs crossing from the rim onto the hide, a dark punch hole at each end
            let thong = rgb(222, 196, 146);
            let hole = rgb(24, 15, 8);
            let step = 12.0 * u;
            let mut x = hide.left() + 8.0 * u;
            while x < hide.right() - 8.0 * u {
                for (y0, dir) in [(hide.top() - 2.0 * u, 1.0), (hide.bottom() + u, -1.0)] {
                    for k in 0..4 {
                        rect(out, xywh(x + k as f32 * u, y0 + dir * k as f32 * u, u, 2.0 * u), thong);
                    }
                    rect(out, xywh(x + 4.0 * u, y0 + dir * 3.0 * u, u, u), hole);
                }
                x += step;
            }
            let mut y = hide.top() + 8.0 * u;
            while y < hide.bottom() - 8.0 * u {
                for (x0, dir) in [(hide.left() - 2.0 * u, 1.0), (hide.right() + u, -1.0)] {
                    for k in 0..4 {
                        rect(out, xywh(x0 + dir * k as f32 * u, y + k as f32 * u, 2.0 * u, u), thong);
                    }
                    rect(out, xywh(x0 + dir * 3.0 * u, y + 4.0 * u, u, u), hole);
                }
                y += step;
            }
        }
        Era::Metal => {
            rect(out, r, pal.edge);
            let plate = r.shrink(2.0 * u);
            rect(out, plate, pal.base);
            // plates: a seam every ~150 points, light lip below a dark line
            let seam_gap = (150.0 / u).round() * u;
            let mut y = plate.top() + seam_gap;
            while y < plate.bottom() - 20.0 {
                rect(out, xywh(plate.left(), y, plate.width(), u), pal.edge);
                rect(out, xywh(plate.left(), y + u, plate.width(), u), mix(pal.base, pal.light, 0.5));
                y += seam_gap;
            }
            // bevel and a darker band under the top edge
            rect(out, xywh(plate.left(), plate.top(), plate.width(), u), pal.light);
            rect(out, xywh(plate.left(), plate.top(), u, plate.height()), pal.light);
            rect(out, xywh(plate.left(), plate.bottom() - u, plate.width(), u), pal.slot_shadow);
            rect(out, xywh(plate.right() - u, plate.top(), u, plate.height()), pal.slot_shadow);
            // rivets along the rim: a light head with a dark foot
            let step = (22.0 / u).round() * u;
            let rivet = |out: &mut Vec<Shape>, x: f32, y: f32| {
                rect(out, xywh(x, y, 2.0 * u, 2.0 * u), pal.light);
                rect(out, xywh(x + u, y + u, u, u), mix(pal.light, Color32::WHITE, 0.4));
                rect(out, xywh(x + 2.0 * u, y + u, u, 2.0 * u), pal.slot_shadow);
                rect(out, xywh(x + u, y + 2.0 * u, u, u), pal.slot_shadow);
            };
            let mut x = plate.left() + 4.0 * u;
            while x < plate.right() - 6.0 * u {
                rivet(out, x, plate.top() + 3.0 * u);
                rivet(out, x, plate.bottom() - 6.0 * u);
                x += step;
            }
            let mut y = plate.top() + 4.0 * u + step;
            while y < plate.bottom() - 6.0 * u - step / 2.0 {
                rivet(out, plate.left() + 3.0 * u, y);
                rivet(out, plate.right() - 6.0 * u, y);
                y += step;
            }
        }
        Era::Machine => {
            rect(out, r, pal.edge);
            let sheet = r.shrink(2.0 * u);
            rect(out, sheet, pal.base);
            // blueprint grid, every fifth line brighter
            let cell = (14.0 / u).round() * u;
            let major = mix(pal.base_dark, pal.light, 0.18);
            let (mut x, mut i) = (sheet.left() + cell, 1);
            while x < sheet.right() {
                rect(out, xywh(x, sheet.top(), u, sheet.height()), if i % 5 == 0 { major } else { pal.base_dark });
                x += cell;
                i += 1;
            }
            let (mut y, mut i) = (sheet.top() + cell, 1);
            while y < sheet.bottom() {
                rect(out, xywh(sheet.left(), y, sheet.width(), u), if i % 5 == 0 { major } else { pal.base_dark });
                y += cell;
                i += 1;
            }
            // a thin light rule, set in from the edge, with corner ticks
            border(out, sheet.shrink(3.0 * u), 1.0, pal.light, false);
            let t = 10.0 * u;
            for (cx, cy, sx, sy) in [(sheet.left(), sheet.top(), 1.0, 1.0), (sheet.right(), sheet.top(), -1.0, 1.0), (sheet.left(), sheet.bottom(), 1.0, -1.0), (sheet.right(), sheet.bottom(), -1.0, -1.0)] {
                let hx = if sx > 0.0 { cx } else { cx - t };
                let vy = if sy > 0.0 { cy } else { cy - t };
                rect(out, xywh(hx, if sy > 0.0 { cy } else { cy - u }, t, u), pal.ink);
                rect(out, xywh(if sx > 0.0 { cx } else { cx - u }, vy, u, t), pal.ink);
            }
        }
        Era::Computer => {
            stepped(out, r, 2, pal.edge);
            let bezel = r.shrink(3.0 * u);
            rect(out, bezel, mix(pal.edge, Color32::BLACK, 0.5));
            let screen = bezel.shrink(2.0 * u);
            stepped(out, screen, 2, pal.base);
            // scanlines: every other pixel row a little darker
            let mut y = screen.top() + 2.0 * u;
            while y < screen.bottom() - 2.0 * u {
                rect(out, xywh(screen.left() + u, y, screen.width() - 2.0 * u, u), pal.base_dark);
                y += 2.0 * u;
            }
            // a faint glow line just inside the glass
            border(out, screen.shrink(u), 1.0, mix(pal.base, pal.light, 0.3), true);
        }
    }
    if let Some(a) = accent {
        let top = match era {
            Era::Stone => r.top() + 6.0 * u,
            Era::Computer => r.top() + 5.0 * u,
            _ => r.top() + 2.0 * u,
        };
        let inset = 12.0 * u;
        rect(out, xywh(r.left() + inset, top, r.width() - 2.0 * inset, 2.0 * u), a);
        rect(out, xywh(r.left() + inset, top + 2.0 * u, r.width() - 2.0 * inset, u), mix(a, Color32::BLACK, 0.45));
    }
}

/// A sunken sub-panel inside a frame (the info panel, the bag): darker than the body, same material.
pub(crate) fn well(p: &Painter, r: Rect, era: Era) {
    let salt = format!("well{:.0}x{:.0}", r.min.x, r.min.y);
    cached(p, &salt, r, era, 0, |out| {
        let u = unit();
        let pal = palette(era);
        let r = snap_rect(r);
        match era {
            Era::Stone => {
                stepped(out, r, 2, pal.slot_shadow);
                stepped(out, Rect::from_min_max(r.min + Vec2::splat(2.0 * u), r.max - Vec2::new(u, u)), 2, mix(pal.slot, pal.base, 0.35));
                rect(out, xywh(r.left() + 3.0 * u, r.bottom() - u, r.width() - 6.0 * u, u), pal.light);
            }
            Era::Metal => {
                rect(out, r, pal.slot_shadow);
                rect(out, Rect::from_min_max(r.min + Vec2::splat(2.0 * u), r.max - Vec2::splat(u)), mix(pal.slot, pal.base, 0.3));
                rect(out, xywh(r.left(), r.bottom() - u, r.width(), u), pal.light);
                rect(out, xywh(r.right() - u, r.top(), u, r.height()), pal.light);
            }
            Era::Machine => {
                rect(out, r, pal.slot);
                border(out, r, 1.0, mix(pal.light, pal.base, 0.35), false);
            }
            Era::Computer => {
                rect(out, r, pal.slot_shadow);
                border(out, r, 1.0, pal.ink_dim, true);
            }
        }
    });
}

/// The ground of one slot, in the era's material: a stepped pit in the hide, an inset iron socket, a thin blueprint
/// box, a screen cell. `Target` adds the drop highlight; the item itself is drawn by the caller.
pub(crate) fn slot_frame(p: &Painter, r: Rect, era: Era, look: crate::inv::SlotLook) {
    let u = unit();
    let pal = palette(era);
    let r = snap_rect(r);
    let mut out = Vec::with_capacity(12);
    match era {
        Era::Stone => {
            stepped(&mut out, r, 2, pal.edge);
            stepped(&mut out, r.shrink(u), 2, pal.slot);
            // shadow inside the upper lip, a lit lower rim: the pit is dug in
            rect(&mut out, xywh(r.left() + 3.0 * u, r.top() + u, r.width() - 6.0 * u, 2.0 * u), pal.slot_shadow);
            rect(&mut out, xywh(r.left() + u, r.top() + 3.0 * u, 2.0 * u, r.height() - 6.0 * u), pal.slot_shadow);
            rect(&mut out, xywh(r.left() + 3.0 * u, r.bottom() - 2.0 * u, r.width() - 6.0 * u, u), mix(pal.slot, pal.light, 0.45));
        }
        Era::Metal => {
            rect(&mut out, r, pal.edge);
            rect(&mut out, r.shrink(u), pal.slot);
            rect(&mut out, xywh(r.left() + u, r.top() + u, r.width() - 2.0 * u, 2.0 * u), pal.slot_shadow);
            rect(&mut out, xywh(r.left() + u, r.top() + u, 2.0 * u, r.height() - 2.0 * u), pal.slot_shadow);
            rect(&mut out, xywh(r.left() + u, r.bottom() - 2.0 * u, r.width() - 2.0 * u, u), mix(pal.slot, pal.light, 0.4));
            rect(&mut out, xywh(r.right() - 2.0 * u, r.top() + u, u, r.height() - 2.0 * u), mix(pal.slot, pal.light, 0.4));
        }
        Era::Machine => {
            rect(&mut out, r, pal.slot);
            border(&mut out, r, 1.0, mix(pal.light, pal.slot, 0.25), false);
        }
        Era::Computer => {
            rect(&mut out, r, pal.slot);
            border(&mut out, r, 1.0, pal.ink_dim, true);
            rect(&mut out, xywh(r.left() + 2.0 * u, r.top() + 2.0 * u, 2.0 * u, u), pal.ink_dim);
        }
    }
    match look {
        crate::inv::SlotLook::Target => border(&mut out, r, 2.0, pal.good, era == Era::Stone),
        crate::inv::SlotLook::Empty => {}
        crate::inv::SlotLook::Normal | crate::inv::SlotLook::Disabled => {}
    }
    p.extend(out);
}

/// The hover ring a live slot or button gets.
pub(crate) fn highlight(p: &Painter, r: Rect, col: Color32) {
    let mut out = Vec::with_capacity(4);
    border(&mut out, r, 2.0, col, false);
    p.extend(out);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Family {
    Fire,
    Kiln,
    Furnace,
    Smithing,
    Grinding,
    Bench,
    Textile,
    MachineTool,
    Storage,
    Growing,
    Electric,
}

/// The one table of machine families (spec "Machine families"); a building not listed is a Bench.
pub(crate) fn family(kind: BuildingKind) -> Family {
    use BuildingKind as B;
    match kind {
        B::Campfire | B::Smokehouse => Family::Fire,
        B::CharcoalKiln | B::PitKiln | B::CokeOven | B::LimeKiln | B::CementKiln | B::BrickKiln => Family::Kiln,
        B::Furnace | B::Bloomery | B::BlastFurnace | B::Cupola | B::PuddlingFurnace | B::Bessemer | B::GlassFurnace => Family::Furnace,
        B::Anvil | B::IronAnvil | B::SteelAnvil | B::TripHammer | B::SteamHammer | B::ForgeHammer => Family::Smithing,
        B::HandMill | B::StoneMortar | B::WaterMill | B::AnimalGin | B::OilPress | B::Press => Family::Grinding,
        B::Workbench | B::Workshop | B::MoldingBench | B::PotterWheel | B::Tannery => Family::Bench,
        B::Loom | B::PowerLoom | B::SpinningWheel | B::SpinningJenny => Family::Textile,
        B::Lathe | B::DrillMill | B::Sawmill | B::RollingMill | B::RollingLine | B::CncMachine => Family::MachineTool,
        B::Depot | B::Cellar | B::Cooler => Family::Storage,
        B::Field | B::DryingRack => Family::Growing,
        // the late stations that run on the grid
        B::Computer | B::AssemblyRobot | B::ArcFurnace | B::Electrolysis | B::Welder | B::CrystalPuller | B::Photolith | B::Radio | B::Telegraph | B::VacuumPump => Family::Electric,
        _ => Family::Bench,
    }
}

pub(crate) fn family_name(f: Family) -> &'static str {
    match f {
        Family::Fire => "fire",
        Family::Kiln => "kiln",
        Family::Furnace => "furnace",
        Family::Smithing => "smithing",
        Family::Grinding => "grinding",
        Family::Bench => "bench",
        Family::Textile => "textile",
        Family::MachineTool => "tool",
        Family::Storage => "storage",
        Family::Growing => "growing",
        Family::Electric => "electric",
    }
}

pub(crate) fn accent(f: Family) -> Color32 {
    match f {
        Family::Fire => rgb(246, 146, 52),
        Family::Kiln => rgb(206, 72, 48),
        Family::Furnace => rgb(255, 120, 30),
        Family::Smithing => rgb(120, 160, 210),
        Family::Grinding => rgb(84, 160, 220),
        Family::Bench => rgb(176, 126, 74),
        Family::Textile => rgb(236, 222, 184),
        Family::MachineTool => rgb(120, 178, 96),
        Family::Storage => rgb(176, 170, 156),
        Family::Growing => rgb(110, 196, 84),
        Family::Electric => rgb(80, 220, 236),
    }
}

/// Liberation Mono (SIL OFL 1.1): only for the computer era's data block.
pub(crate) const MONO_FONT: &[u8] = include_bytes!("../../assets/liberation-mono.ttf");
pub(crate) const MONO: &str = "alaz-mono";

/// Adds the mono face as its own family, so nothing outside the data block changes.
pub(crate) fn add_fonts(fonts: &mut egui::FontDefinitions) {
    fonts.font_data.insert(MONO.into(), std::sync::Arc::new(egui::FontData::from_static(MONO_FONT)));
    fonts.families.insert(egui::FontFamily::Name(MONO.into()), vec![MONO.into(), "alaz-era".into(), "alaz-pixel".into()]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn era_mapping() {
        let want = [Era::Stone, Era::Stone, Era::Metal, Era::Metal, Era::Machine, Era::Computer];
        for (level, era) in want.into_iter().enumerate() {
            assert_eq!(era_of(level as u8), era, "perception {level}");
        }
    }

    #[test]
    fn every_window_building_has_a_family() {
        let mut w = alaz_sim::World::new(5);
        w.learn_everything();
        for kind in BuildingKind::ALL {
            if crate::inv::machine::opens_window(&w, kind) {
                let _ = family(kind);
            }
        }
        for (kind, fam) in [
            (BuildingKind::Furnace, Family::Furnace),
            (BuildingKind::HandMill, Family::Grinding),
            (BuildingKind::Loom, Family::Textile),
            (BuildingKind::Depot, Family::Storage),
            (BuildingKind::Field, Family::Growing),
            (BuildingKind::Campfire, Family::Fire),
        ] {
            assert_eq!(family(kind), fam, "{kind:?}");
        }
    }
}
