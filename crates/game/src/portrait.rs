//! The player's portrait for the status box: head and shoulders in pixel cells, drawn from the same looks
//! (`alaz_render::look`) and clothes the body in the world has, and from the same stage coordinate (`Evo`: furry,
//! heavy-browed and muzzled at first, then smooth, greying at the last), in a frame in the era's colours. Every
//! feature slides with the stage coordinate, so the face changes a cell at a time, never in a jump. Kept between
//! frames (`cells_cached`), so it is only rebuilt when the person, their stage, their clothes or the interface
//! scale change.

use crate::pxpanel::{Px, Theme};
use crate::pxui::{self, INK};
use alaz_render::egui::{self, Color32, Pos2, Rect, Sense, Ui, Vec2};
use alaz_render::{look, Evo, Look};

/// Cells across and down, frame included.
pub const W: i32 = 24;
pub const H: i32 = 28;
/// Frame thickness in cells.
const B: i32 = 2;

fn col(c: [f32; 3], k: f32) -> Color32 {
    let f = |v: f32| (v * k * 255.0).clamp(0.0, 255.0) as u8;
    Color32::from_rgb(f(c[0]), f(c[1]), f(c[2]))
}

fn ell(x: f32, y: f32, cx: f32, cy: f32, rx: f32, ry: f32) -> bool {
    let (dx, dy) = ((x - cx) / rx, (y - cy) / ry);
    dx * dx + dy * dy <= 1.0
}

fn mix(a: [f32; 3], b: [f32; 3], k: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k]
}

/// The colour of the cell at (x, y) of the picture (0..W-2B, 0..H-2B), or none outside the figure.
fn figure(lk: &Look, ev: &Evo, o: u16, x: i32, y: i32) -> Option<Color32> {
    use alaz_render::{OUT_APRON, OUT_CLOAK, OUT_FELT, OUT_HELM, OUT_HOOD, OUT_LEATHER, OUT_PLATE, OUT_TUNIC, OUT_WRAP};
    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
    let (m, br, shag) = (ev.muzzle, ev.brow, ev.shag);
    let fur_c = [0.33, 0.23, 0.15];
    let skin: [f32; 3] = mix(lk.skin, fur_c, ev.stoop * 0.5);
    let hair = mix(lk.hair, [0.76, 0.74, 0.7], ev.gray * 0.9);
    let (hide, fluff, linen) = ([0.56, 0.37, 0.21], [0.86, 0.80, 0.66], [0.84, 0.78, 0.62]);
    let (bronze, steel, canvas) = ([0.78, 0.56, 0.24], [0.7, 0.74, 0.8], [0.43, 0.36, 0.27]);
    let ochre = [0.72, 0.3, 0.14];
    // light from the upper left
    let lit = |x: f32, y: f32| 1.0 - 0.16 * ((x - 6.0) / 10.0).clamp(0.0, 1.0) - 0.08 * ((y - 6.0) / 18.0).clamp(0.0, 1.0);
    let shade = |c: [f32; 3]| Some(col(c, lit(fx, fy)));
    let (hx, hy) = (10.0, 9.5);
    // the mane's outline is ragged while it is thick; the skull sits a little lower and wider the nearer the ape
    let j = (noise(x, y) - 0.5) * (0.4 + 0.7 * m) * shag;
    let in_head = ell(fx, fy, hx, hy + 0.7 * m, 5.0 + 0.6 * m + j, 6.3 + 0.1 * m + j);
    let face_box = ell(fx, fy, hx, hy + 0.6, 4.1, 5.3);
    let style = lk.style;
    // the person's own cut grows in as the mane thins (style 3 is the shaggy one: it stays)
    let cut = 1.0 - shag;
    let mane = if style == 3 { 1.0 } else { shag };
    // the mane's tufts, strong while it is thick
    let furry = |c: [f32; 3]| {
        let n = noise(x, y);
        let k = if n < 0.2 { 0.82 } else if n > 0.86 { 1.1 } else { 1.0 };
        c.map(|v| v * (1.0 + (k - 1.0) * shag))
    };

    // face details first: they sit on top of the head
    let helm = o & OUT_HELM != 0;
    let hood = o & OUT_HOOD != 0;
    if in_head {
        // a heavy brow pulls the eyes together and sinks them into shadow
        let (ex, ey0, ey1, ew) = (2.3 - 0.8 * br, 9.5 + 0.5 * br, 10.6 + 0.4 * br, 0.9 - 0.35 * br);
        if !helm_covers(helm, fy) {
            for s in [-1.0f32, 1.0] {
                let cx = hx + s * ex;
                if fx > cx - ew && fx < cx + ew && fy > ey0 && fy < ey1 {
                    return Some(Color32::from_rgb(14, 11, 11));
                }
                if fx > cx - 1.6 - 1.2 * br && fx < cx + 1.6 + 1.2 * br && fy > 8.0 && fy < 8.8 + 0.2 * br {
                    return shade(mix(hair.map(|c| c * 0.55), skin.map(|c| c * 0.44), br));
                }
                if br > 0.25 && fx > cx - 1.7 && fx < cx + 1.7 && fy > ey0 - 0.5 && fy < ey1 + 0.4 {
                    return shade(skin.map(|c| c * (1.0 - 0.45 * br)));
                }
            }
            // nose shadow (broad and flat on a muzzle), a lighter muzzle patch and a mouth line
            if fx > hx - 0.6 - 1.5 * m && fx < hx + 0.6 + 1.5 * m && fy > 10.4 + 0.6 * m && fy < 12.4 - 0.4 * m {
                let k = 0.82 - 0.45 * m;
                return shade([skin[0] * k, skin[1] * (k - 0.12), skin[2] * (k - 0.18)]);
            }
            if m > 0.12 && ell(fx, fy, hx, 13.2 + 0.5 * m, 1.0 + 2.7 * m, 0.5 + 2.0 * m) {
                let patch = lk.skin.map(|c| (c * 0.8 + 0.07).min(1.0));
                if fy > 13.4 + 0.6 * m && fy < 14.3 + 0.6 * m && (fx - hx).abs() < 1.4 + 1.4 * m {
                    return shade([skin[0] * (0.6 - 0.24 * m), skin[1] * (0.4 - 0.1 * m), skin[2] * (0.38 - 0.1 * m)]);
                }
                return shade(mix(skin, patch, m.min(1.0)));
            }
            if fx > hx - 1.4 - 1.4 * m && fx < hx + 1.4 + 1.4 * m && fy > 13.4 + 0.6 * m && fy < 14.3 + 0.6 * m {
                return shade([skin[0] * (0.6 - 0.24 * m), skin[1] * (0.4 - 0.1 * m), skin[2] * (0.38 - 0.1 * m)]);
            }
            // war paint: a stripe over each cheek
            if ev.ochre > 0.2 && (fx - hx).abs() > 2.4 && (fx - hx).abs() < 2.4 + 1.4 * ev.ochre && fy > 11.0 && fy < 11.9 {
                return shade(ochre);
            }
        }
    }
    // helm: a steel cap with a rim over the brows, a nose guard and a red plume
    if helm {
        if fy > 2.0 && fy < 7.4 && ell(fx, fy, hx, 7.6, 5.6, 5.8) {
            return shade(steel);
        }
        if fy >= 7.4 && fy < 8.4 && ell(fx, fy, hx, 8.0, 5.8, 3.0) {
            return shade([steel[0] * 0.65, steel[1] * 0.65, steel[2] * 0.7]);
        }
        if fx > hx - 0.6 && fx < hx + 0.6 && fy >= 8.4 && fy < 12.0 {
            return shade(steel);
        }
        if fx > hx - 0.8 && fx < hx + 0.8 && fy > 0.2 && fy <= 2.0 {
            return shade([0.66, 0.16, 0.12]);
        }
    }
    // hood: linen round the face, with a cowl on the shoulders
    if hood && ell(fx, fy, hx, 9.2, 6.6, 7.6) && !face_box {
        return shade(linen);
    }
    // a straw hat: a dome and a brim that spreads as it comes on
    if ev.hat > 0.1 && !helm && !hood {
        let k = ev.hat;
        let straw = [0.86, 0.74, 0.42];
        if fy < 4.6 && ell(fx, fy, hx, 3.8, 4.4 * k, 2.4 * k) {
            return shade(if fy > 3.8 { lk.dye } else { straw });
        }
        if ell(fx, fy, hx, 4.6, 7.8 * k, 1.0 * k) {
            return shade(straw.map(|c| c * 0.92));
        }
    }
    if in_head {
        let hairline = 5.6 + if style == 5 { 0.4 } else { 0.0 } + 2.4 * br;
        let side = 7.4 + 9.0 * shag;
        let hair_cap = fy < hairline || (fy < side && (fx - hx).abs() > 3.6 - 0.2 * shag);
        if !helm && hair_cap && !hood {
            return shade(furry(hair));
        }
        // a beard: some grow one from the early stages, every sage wears a long one
        let bk = if lk.beard { ev.beard_grow.max(ev.beard_all) } else { ev.beard_all };
        if !helm && !hood && bk > 0.05 && fy > 15.8 - 3.2 * bk {
            return shade(hair);
        }
        return shade(skin);
    }
    // ears stick out beyond the head of the first stages
    if m > 0.2 && !helm && !hood {
        for s in [-1.0f32, 1.0] {
            let ecx = hx + s * (5.5 + 0.6 * m);
            if ell(fx, fy, ecx, hy + 0.7, 1.6 * m, 1.8 * m) {
                return shade(if ell(fx, fy, ecx - s * 0.2, hy + 0.9, 0.7 * m, 1.0 * m) { skin.map(|c| c * 0.6) } else { skin.map(|c| c * 0.82) });
            }
        }
    }
    // hair hanging beside the head and neck
    if !helm && !hood {
        let long = style == 1 && fy > 6.0 && fy < 6.0 + 13.0 * cut && ((fx - hx).abs() > 4.4 && (fx - hx).abs() < 6.6);
        let braid = style == 4 && fx > hx + lk.braid_side * 5.0 - 0.9 && fx < hx + lk.braid_side * 5.0 + 0.9 && fy > 10.0 && fy < 10.0 + 11.0 * cut;
        let knot = style == 2 && ell(fx, fy, hx, 2.0, 2.0 * cut, 1.8 * cut);
        let bushy = mane > 0.02 && ell(fx, fy, hx, 8.0 + 0.7 * m, 6.3 * mane, 6.0 * mane) && fy < 10.0 + 0.7 * m;
        if long || braid || knot || bushy {
            return shade(furry(hair));
        }
        // the sage's long beard falls over the chest
        let bl = ev.beard_all;
        if bl > 0.05 && fy > 15.5 && fy < 15.5 + 5.5 * bl && (fx - hx).abs() < 2.4 * (1.0 - (fy - 15.5) / 8.0) {
            return shade(hair);
        }
    }
    // neck: there is none to speak of while the jaw sits on humped shoulders
    let nw = 1.8 * (1.0 - m);
    if nw > 0.25 && fx > hx - nw && fx < hx + nw && fy >= 14.5 + 0.7 * m && fy < 19.0 {
        return shade([skin[0] * 0.86, skin[1] * 0.8, skin[2] * 0.78]);
    }
    // shoulders and chest
    let sh = lk.frame.0;
    let st = ev.stoop;
    let hump = smooth(0.4, 1.0, st);
    let body = (ell(fx, fy, hx, 25.0 - 0.6 * st, (9.6 + 0.8 * st) * sh, 7.6 + 0.4 * st) && fy > 16.5 - 0.5 * st)
        || (hump > 0.05 && [-1.0f32, 1.0].iter().any(|s| ell(fx, fy, hx + s * 6.0 * sh, 19.2, 4.6 * sh * hump, 3.5 * hump)));
    if body {
        // trinkets over everything: a gold amulet, a necklace of teeth, a dyed sash
        if ev.amulet > 0.1 && ell(fx, fy, hx, 20.6, 1.1 * ev.amulet, 1.2 * ev.amulet) {
            return shade([0.92, 0.74, 0.28]);
        }
        if ev.tooth > 0.1 && (fx - hx).abs() < 4.0 * ev.tooth.sqrt() && x % 2 == 0 && (fy - (17.2 + ((fx - hx) / 4.0).powi(2) * 2.0)).abs() < 0.6 {
            return shade([0.93, 0.9, 0.8]);
        }
        if ev.sash > 0.1 {
            let t = (fx - (hx - 5.0)) / 10.0;
            if (0.0..1.0).contains(&t) && (fy - (17.5 + t * 8.0)).abs() < 0.5 + 0.5 * ev.sash {
                return shade(lk.dye);
            }
        }
        if o & OUT_PLATE != 0 {
            let pauldron = (fx - hx).abs() > 6.0 * sh && fy < 21.5;
            return shade(if pauldron { [bronze[0] * 0.8, bronze[1] * 0.8, bronze[2] * 0.8] } else { bronze });
        }
        if o & OUT_LEATHER != 0 {
            let pauldron = (fx - hx).abs() > 6.0 * sh && fy < 21.5;
            let leather = [0.52, 0.34, 0.19];
            return shade(if pauldron { leather.map(|c| c * 0.8) } else { leather });
        }
        if o & OUT_APRON != 0 {
            let strap = (fx - hx).abs() > 2.2 && (fx - hx).abs() < 3.4 && fy < 21.0;
            if strap || ((fx - hx).abs() < 3.4 && fy > 20.5) {
                if (fx - hx).abs() < 0.8 && fy > 21.0 && fy < 22.2 {
                    return Some(Color32::from_rgb(220, 180, 76));
                }
                return shade(canvas);
            }
            return shade(linen);
        }
        if o & OUT_CLOAK != 0 {
            return shade(if fy < 19.2 { fluff } else { hide });
        }
        if o & OUT_FELT != 0 {
            return shade(if fy < 19.2 { [0.62, 0.64, 0.66] } else { [0.50, 0.54, 0.60] });
        }
        if o & OUT_TUNIC != 0 {
            let collar = (fx - hx).abs() < 2.6 && fy < 19.5;
            return shade(if collar { linen } else { lk.dye });
        }
        if o & OUT_WRAP != 0 && fy > 20.0 {
            return shade([0.66, 0.70, 0.36]);
        }
        // ochre stripes across the bare chest
        if ev.ochre > 0.2 && (fx - hx).abs() < 3.6 * ev.ochre && ((fy > 18.6 && fy < 19.4) || (fy > 20.3 && fy < 21.1 && o & OUT_WRAP == 0)) {
            return shade(ochre);
        }
        return shade(chest(lk, ev, skin, x, y));
    }
    None
}

/// A steady pseudo-random 0..1 per cell, so fur looks tufted but never shimmers between frames.
fn noise(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77) ^ 0x2C1B_3C6D;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 65535.0
}

/// Smooth 0..1 ramp (flat outside `a..b`).
fn smooth(a: f32, b: f32, v: f32) -> f32 {
    alaz_render::smooth(a, b, v)
}

/// A bare chest: skin, with the fur still on it where each cell's own dice say so (more cells the nearer the ape),
/// and a bare patch on the breast that grows as the fur thins.
fn chest(lk: &Look, ev: &Evo, skin: [f32; 3], x: i32, y: i32) -> [f32; 3] {
    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
    let f = ev.fur;
    let patch = 3.6 * (1.0 - f);
    if patch > 0.3 && ell(fx - 10.0, fy, 0.0, 20.6, patch, 2.8) {
        return lk.skin.map(|c| c * (0.74 + 0.26 * (1.0 - f)));
    }
    if noise(x, y) < f * 1.1 {
        let fur = [0.33, 0.23, 0.15];
        let body = mix(fur, lk.hair.map(|c| c * 0.9), 0.4);
        let n = noise(x + 7, y + 3);
        let k = if n < 0.2 { 0.82 } else if n > 0.86 { 1.1 } else { 1.0 };
        return body.map(|v| v * k);
    }
    skin
}

fn helm_covers(helm: bool, fy: f32) -> bool {
    helm && fy < 8.4
}

/// The colour of the cell (cx, cy) of the whole plate: frame, background, or the figure with an ink outline.
fn cell(th: &Theme, lk: &Look, ev: &Evo, o: u16, cx: i32, cy: i32) -> Option<Color32> {
    if cx < 0 || cy < 0 || cx >= W || cy >= H {
        return None;
    }
    let edge = cx == 0 || cy == 0 || cx == W - 1 || cy == H - 1;
    if edge {
        return Some(th.edge);
    }
    let bevel = cx == 1 || cy == 1 || cx == W - 2 || cy == H - 2;
    if bevel {
        return Some(if cx == 1 || cy == 1 { th.light } else { th.dark });
    }
    let (x, y) = (cx - B, cy - B);
    let (iw, ih) = (W - 2 * B, H - 2 * B);
    let inside = |x: i32, y: i32| x >= 0 && y >= 0 && x < iw && y < ih;
    if let Some(c) = inside(x, y).then(|| figure(lk, ev, o, x, y)).flatten() {
        return Some(c);
    }
    // an ink outline round the figure
    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let (nx, ny) = (x + dx, y + dy);
        if inside(nx, ny) && figure(lk, ev, o, nx, ny).is_some() {
            return Some(INK);
        }
    }
    Some(th.track)
}

/// Draw the portrait (as wide as `W` cells, as high as `H`); `level` picks the frame, `evo` the stage coordinate the
/// face and shoulders are drawn at (0..=9, see `Evo`), `id` and `outfit` the person.
pub fn portrait(ui: &mut Ui, th: &Theme, level: u8, evo: f32, id: u32, outfit: u16) -> egui::Response {
    let px = Px::of(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(Vec2::new(px.n(W as f32), px.n(H as f32)), Sense::hover());
    let r = Rect::from_min_size(px.pos(r.min), r.size());
    // the stage is told in 1/32ths: a glide of a few seconds redraws it a few dozen times, no more
    let step = (evo.clamp(0.0, 9.0) * 32.0).round() as u64;
    let key = (id as u64) << 24 ^ (outfit as u64) << 8 ^ (level as u64) << 4 ^ th.light.r() as u64 ^ 0x5EED ^ step << 40;
    let lk = look(id);
    let ev = Evo::at(step as f32 / 32.0);
    let (u, left, top) = (px.u, r.left(), r.top());
    pxui::cells_cached(ui.ctx(), egui::Id::unique("portrait"), key, ui.painter(), r, |p: Pos2| {
        let (cx, cy) = (((p.x - left) / u).floor() as i32, ((p.y - top) / u).floor() as i32);
        cell(th, &lk, &ev, outfit, cx, cy)
    });
    resp
}

/// What the tribe has mastered, as a row of tiny pictures: each one lights when its discovery is known. The name
/// shows on hover. Nothing is shown for what is not known yet, so the row grows with the ages.
pub fn tech_strip(ui: &mut Ui, w: &alaz_sim::World) {
    use alaz_sim::world::Discovery as D;
    use crate::pxpanel as pp;
    const TECH: [(D, &[&str], Color32); 9] = [
        (D::Fire, &["..a..", "..aa.", ".aaa.", "aaaaa", "aaada", ".aaa."], Color32::from_rgb(240, 130, 50)),
        (D::Farming, &["a.a.a", ".aaa.", "a.a.a", ".aaa.", "..a..", "..a.."], Color32::from_rgb(226, 190, 80)),
        (D::Pottery, &[".aaa.", "a...a", "aaaaa", "aaaaa", ".aaa."], Color32::from_rgb(190, 110, 70)),
        (D::Weaving, &["aaaaa", "a.a.a", "aaaaa", "a.a.a", "aaaaa"], Color32::from_rgb(214, 200, 160)),
        (D::Bronzework, &["aaaaa", "aaaaa", ".aaa.", "..a..", ".aaa.", "aaaaa"], Color32::from_rgb(210, 150, 70)),
        (D::Steelmaking, &["aaaaa", "adaad", "aaaaa", ".aaa.", "aaaaa"], Color32::from_rgb(150, 170, 204)),
        (D::Steam, &[".a.a.", "aaaaa", "aa.aa", "aaaaa", ".a.a."], Color32::from_rgb(176, 184, 196)),
        (D::Electricity, &["..aa.", ".aa..", "aaaa.", "..aa.", ".aa..", "a...."], Color32::from_rgb(250, 224, 90)),
        (D::Computing, &["a.a.a", "aaaaa", "aa.aa", "aaaaa", "a.a.a"], Color32::from_rgb(110, 214, 224)),
    ];
    let px = Px::of(ui.ctx());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = px.n(1.0);
        for (d, rows, c) in TECH {
            if !w.knows(d) {
                continue;
            }
            let (r, resp) = ui.allocate_exact_size(Vec2::new(px.n(6.0), px.n(6.0)), Sense::hover());
            pp::sprite(ui, r.min, rows, c);
            resp.on_hover_text(crate::hud::discovery_name(d));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole plate as cells (frame included), for a person at a stage coordinate.
    fn plate(id: u32, outfit: u16, v: f32) -> Vec<Option<Color32>> {
        let th = Theme::era(3);
        let (lk, ev) = (look(id), Evo::at(v));
        (0..H).flat_map(|cy| (0..W).map(|cx| (cx, cy)).collect::<Vec<_>>()).map(|(cx, cy)| cell(&th, &lk, &ev, outfit, cx, cy)).collect()
    }

    /// How many cells changed visibly: drawn or not drawn, or a colour that moved by more than a shade.
    fn differing(a: &[Option<Color32>], b: &[Option<Color32>]) -> usize {
        let far = |p: Color32, q: Color32| p.r().abs_diff(q.r()) as u32 + p.g().abs_diff(q.g()) as u32 + p.b().abs_diff(q.b()) as u32 > 24;
        a.iter().zip(b).filter(|(p, q)| match (p, q) {
            (Some(p), Some(q)) => far(*p, *q),
            (None, None) => false,
            _ => true,
        }).count()
    }

    /// Stage coordinates 1/32 apart (what the portrait redraws at) differ by a handful of cells, never a jump, from
    /// the ape to the sage, for people with every cut of hair, with and without clothes.
    #[test]
    fn portrait_evolves_without_jumps() {
        let mut worst = 0;
        for (id, outfit) in [(12u32, 0u16), (8, 0), (1, 0), (10, 0), (5, 7), (4, 28), (23, 100), (35, 640), (50, 128)] {
            let mut last = plate(id, outfit, 0.0);
            for k in 1..=9 * 32 {
                let now = plate(id, outfit, k as f32 / 32.0);
                let d = differing(&last, &now);
                worst = worst.max(d);
                assert!(d <= 20, "id {id} outfit {outfit:#x}: {d} cells change at stage {}", k as f32 / 32.0);
                last = now;
            }
        }
        println!("largest step: {worst} cells of {}", W * H);
    }

    /// Each named stage has a face of its own, and the ends are far apart.
    #[test]
    fn named_stages_look_different() {
        for id in [12u32, 8, 1] {
            for i in 0..9 {
                let d = differing(&plate(id, 0, i as f32), &plate(id, 0, i as f32 + 1.0));
                assert!(d >= 6, "id {id}: stages {i} and {} differ in only {d} cells", i + 1);
            }
            assert!(differing(&plate(id, 0, 0.0), &plate(id, 0, 9.0)) > 120, "ape and sage alike");
        }
    }

    /// The portrait is whole at every stage: something is drawn in the middle of the plate, nothing leaks out of the frame.
    #[test]
    fn portrait_is_drawn_at_every_stage() {
        for i in 0..=36 {
            let p = plate(12, 0xFFFF, i as f32 * 0.25);
            assert!(p[(H / 2 * W + W / 2) as usize].is_some());
            assert!(p.iter().filter(|c| c.is_some()).count() > (W * H) as usize * 9 / 10);
        }
    }
}
