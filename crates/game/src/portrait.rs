//! The player's portrait for the status box: head and shoulders in pixel cells, drawn from the same looks
//! (`alaz_render::look`) and clothes the body in the world has, and from the same stage (furry and heavy-browed
//! at first, then smooth), in a frame in the era's colours. Kept between frames (`cells_cached`), so it is
//! only rebuilt when the person, their stage, their clothes or the interface scale change.

use crate::pxpanel::{Px, Theme};
use crate::pxui::{self, INK};
use alaz_render::egui::{self, Color32, Pos2, Rect, Sense, Ui, Vec2};
use alaz_render::{look, Look};

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

/// The colour of the cell at (x, y) of the picture (0..W-2B, 0..H-2B), or none outside the figure.
fn figure(lk: &Look, stage: u8, o: u16, x: i32, y: i32) -> Option<Color32> {
    use alaz_render::{OUT_APRON, OUT_CLOAK, OUT_FELT, OUT_HELM, OUT_HOOD, OUT_LEATHER, OUT_PLATE, OUT_TUNIC, OUT_WRAP};
    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
    let ape = stage <= 2;
    let stoop = (5 - stage.min(5)) as f32 / 5.0;
    let fur = [0.33, 0.23, 0.15];
    let skin: [f32; 3] = [0, 1, 2].map(|i| lk.skin[i] + (fur[i] - lk.skin[i]) * stoop * 0.5);
    let hair = lk.hair;
    let (hide, fluff, linen) = ([0.56, 0.37, 0.21], [0.86, 0.80, 0.66], [0.84, 0.78, 0.62]);
    let (bronze, steel, canvas) = ([0.78, 0.56, 0.24], [0.7, 0.74, 0.8], [0.43, 0.36, 0.27]);
    // light from the upper left
    let lit = |x: f32, y: f32| 1.0 - 0.16 * ((x - 6.0) / 10.0).clamp(0.0, 1.0) - 0.08 * ((y - 6.0) / 18.0).clamp(0.0, 1.0);
    let shade = |c: [f32; 3]| Some(col(c, lit(fx, fy)));
    let (hx, hy) = (10.0, 9.5);
    let in_head = !ape && ell(fx, fy, hx, hy, 5.0, 6.3);
    let face_box = ell(fx, fy, hx, hy + 0.6, 4.1, 5.3);

    // face details first: they sit on top of the head
    let helm = o & OUT_HELM != 0;
    let hood = o & OUT_HOOD != 0;
    if in_head {
        let (ex, ey) = (2.3, 9.0);
        // eyes in a dark band under the brows
        for s in [-1.0f32, 1.0] {
            let cx = hx + s * ex;
            if fx > cx - 0.9 && fx < cx + 0.9 && fy > ey + 0.5 && fy < ey + 1.6 && !helm_covers(helm, fy) {
                return Some(Color32::from_rgb(14, 11, 11));
            }
            if fx > cx - 1.6 && fx < cx + 1.6 && fy > ey - 1.0 && fy < ey - 0.2 && !helm_covers(helm, fy) {
                return shade(hair.map(|c| c * 0.55));
            }
        }
        if !helm_covers(helm, fy) {
            // nose shadow and a mouth line
            if fx > hx - 0.6 && fx < hx + 0.6 && fy > 10.4 && fy < 12.4 {
                return shade([skin[0] * 0.82, skin[1] * 0.7, skin[2] * 0.64]);
            }
            if fx > hx - 1.4 && fx < hx + 1.4 && fy > 13.4 && fy < 14.3 {
                return shade([skin[0] * 0.6, skin[1] * 0.4, skin[2] * 0.38]);
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
    if ape && !helm {
        if let Some(c) = ape_head(lk, skin, stage, x, y) {
            return shade(c);
        }
    }
    if in_head {
        let style = lk.style;
        let hair_cap = fy < 5.6 + if style == 5 { 0.4 } else { 0.0 } || (fy < 7.4 && (fx - hx).abs() > 3.6);
        if !helm && hair_cap && !hood {
            return shade(hair);
        }
        if !helm && !hood && lk.beard && fy > 12.6 {
            return shade(hair);
        }
        return shade(skin);
    }
    // hair hanging beside the head and neck
    if !helm && !hood {
        let style = if ape { 3 } else { lk.style };
        let long = style == 1 && fy > 6.0 && fy < 19.0 && ((fx - hx).abs() > 4.4 && (fx - hx).abs() < 6.6);
        let braid = style == 4 && fx > hx + lk.braid_side * 5.0 - 0.9 && fx < hx + lk.braid_side * 5.0 + 0.9 && fy > 10.0 && fy < 21.0;
        let knot = style == 2 && ell(fx, fy, hx, 2.0, 2.0, 1.8);
        let bushy = style == 3 && ell(fx, fy, hx, 8.0, 6.3, 6.0) && fy < 10.0;
        if long || braid || knot || bushy {
            return shade(hair);
        }
    }
    // neck
    if !ape && fx > hx - 1.8 && fx < hx + 1.8 && fy >= 14.5 && fy < 19.0 {
        return shade([skin[0] * 0.86, skin[1] * 0.8, skin[2] * 0.78]);
    }
    // shoulders and chest
    let sh = lk.frame.0;
    // an ape has no neck to speak of: the jaw sits on humped shoulders
    let body = if ape {
        (ell(fx, fy, hx, 24.4, 10.4 * sh, 8.0) && fy > 16.0)
            || [-1.0f32, 1.0].iter().any(|s| ell(fx, fy, hx + s * 6.0 * sh, 19.2, 4.6 * sh, 3.5))
    } else {
        ell(fx, fy, hx, 25.0, 9.6 * sh, 7.6) && fy > 16.5
    };
    if body {
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
        if ape {
            return shade(ape_chest(lk, stage, x, y));
        }
        return shade(skin);
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

/// How ape-like the face still is: 1 at stage 0, fading to 0.33 at stage 2 (the human look starts at stage 3).
fn apeness(stage: u8) -> f32 {
    (3 - stage.min(3)) as f32 / 3.0
}

/// A tuft of fur: the base colour with the odd darker or lighter strand.
fn tuft(c: [f32; 3], x: i32, y: i32) -> [f32; 3] {
    let n = noise(x, y);
    let k = if n < 0.2 { 0.82 } else if n > 0.86 { 1.1 } else { 1.0 };
    c.map(|v| v * k)
}

/// The head of an ape-man, bare only in the middle of the face and shaggy all round it: a low brow with a heavy
/// ridge over deep sockets, broad flat nostrils, a muzzle that juts out, big ears at the sides. The more the
/// stages go on, the less fur, the smaller the muzzle, the higher the brow.
fn ape_head(lk: &Look, skin: [f32; 3], stage: u8, x: i32, y: i32) -> Option<[f32; 3]> {
    let a = apeness(stage);
    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
    let dx = fx - 10.0;
    let ax = dx.abs();
    let bare = skin.map(|c| c * 0.82);
    // a lighter, pinker patch for the muzzle than the rest of the skin
    let muzzle = lk.skin.map(|c| (c * 0.8 + 0.07).min(1.0));
    let dark = |k: f32| skin.map(|c| c * k);
    let hw = 3.4 + (1.0 - a) * 0.7;
    // ears first: they stick out beyond the fur
    for s in [-1.0f32, 1.0] {
        let ecx = s * (5.5 + 0.4 * a);
        if ell(dx, fy, ecx, 10.2, 1.5 + 0.3 * a, 1.7 + 0.4 * a) {
            return Some(if ell(dx, fy, ecx + s * -0.2, 10.4, 0.7, 1.0) { dark(0.6) } else { bare });
        }
    }
    // the fur outline is ragged, most of all in the first stage
    let j = (noise(x, y) - 0.5) * (0.4 + 0.7 * a);
    let skull = ell(dx, fy, 0.0, 8.5, 4.8 + 0.8 * a + j, 4.7 + j);
    let jaw = ell(dx, fy, 0.0, 12.3, 4.3 + 0.6 * a + j, 4.4 + j);
    if !skull && !jaw {
        return None;
    }
    let fur = tuft(lk.hair.map(|c| c * 0.92), x, y);
    let yi = y;
    // the bare middle of the face, row by row
    if ax < hw + 1.0 && yi == 8 && ax < hw + 0.4 + 0.5 * a {
        // brow ridge: one dark shelf across both eyes
        return Some(dark(0.44 + 0.2 * (1.0 - a)));
    }
    if yi == 7 && a < 0.8 && ax < 2.5 {
        return Some(bare);
    }
    if yi == 6 && a < 0.5 && ax < 2.0 {
        return Some(bare);
    }
    if ax < hw {
        match yi {
            9 => return Some(if ax < 1.0 { dark(0.7) } else { dark(0.5 + 0.12 * (1.0 - a)) }),
            10 => {
                if (ax - 1.5).abs() < 0.1 {
                    return Some([0.055, 0.043, 0.043]);
                }
                return Some(if ax > 2.0 { dark(0.58 + 0.12 * (1.0 - a)) } else { bare });
            }
            11 if ax < 2.1 => return Some(if ax > 1.0 { dark(0.34) } else { skin.map(|c| c * 0.7) }),
            _ => {}
        }
        // the muzzle: broad, jutting, with a long mouth and a chin below it
        let mrx = 2.9 + 0.8 * a;
        if yi >= 12 && ell(dx, fy, 0.0, 13.7, mrx, 2.5 + 0.2 * a) {
            if yi == 14 && ax < mrx - 1.1 {
                return Some(dark(0.36));
            }
            return Some(if yi >= 15 { muzzle.map(|c| c * 0.9) } else { muzzle });
        }
        if (11..14).contains(&yi) {
            return Some(bare);
        }
    }
    Some(fur)
}

/// Shoulders and chest of an ape-man: shaggy, with a bare patch on the breast that grows as the fur thins.
fn ape_chest(lk: &Look, stage: u8, x: i32, y: i32) -> [f32; 3] {
    let a = apeness(stage);
    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
    if a < 1.0 && ell(fx - 10.0, fy, 0.0, 20.6, 1.4 + 2.2 * (1.0 - a), 2.8) {
        return lk.skin.map(|c| c * 0.74);
    }
    let fur = [0.33, 0.23, 0.15];
    let body: [f32; 3] = [0, 1, 2].map(|i| fur[i] + (lk.hair[i] * 0.9 - fur[i]) * 0.4);
    tuft(body, x, y)
}

fn helm_covers(helm: bool, fy: f32) -> bool {
    helm && fy < 8.4
}

/// The colour of the cell (cx, cy) of the whole plate: frame, background, or the figure with an ink outline.
fn cell(th: &Theme, lk: &Look, stage: u8, o: u16, cx: i32, cy: i32) -> Option<Color32> {
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
    if let Some(c) = inside(x, y).then(|| figure(lk, stage, o, x, y)).flatten() {
        return Some(c);
    }
    // an ink outline round the figure
    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let (nx, ny) = (x + dx, y + dy);
        if inside(nx, ny) && figure(lk, stage, o, nx, ny).is_some() {
            return Some(INK);
        }
    }
    Some(th.track)
}

/// Draw the portrait (as wide as `W` cells, as high as `H`); `level` picks the frame, `id` and `outfit` the person.
pub fn portrait(ui: &mut Ui, th: &Theme, level: u8, id: u32, outfit: u16) -> egui::Response {
    let px = Px::of(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(Vec2::new(px.n(W as f32), px.n(H as f32)), Sense::hover());
    let r = Rect::from_min_size(px.pos(r.min), r.size());
    let key = (id as u64) << 24 ^ (outfit as u64) << 8 ^ (level as u64) << 4 ^ th.light.r() as u64 ^ 0x5EED;
    let lk = look(id);
    let (u, left, top) = (px.u, r.left(), r.top());
    pxui::cells_cached(ui.ctx(), egui::Id::unique("portrait"), key, ui.painter(), r, |p: Pos2| {
        let (cx, cy) = (((p.x - left) / u).floor() as i32, ((p.y - top) / u).floor() as i32);
        cell(th, &lk, level, outfit, cx, cy)
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
