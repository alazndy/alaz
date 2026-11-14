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
    use alaz_render::{OUT_APRON, OUT_CLOAK, OUT_HELM, OUT_HOOD, OUT_PLATE, OUT_TUNIC, OUT_WRAP};
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
    let (hrx, hry) = if ape { (5.6, 6.0) } else { (5.0, 6.3) };
    let in_head = ell(fx, fy, hx, hy, hrx, hry) || (ape && ell(fx, fy, hx, hy + 3.2, 4.2, 3.4));
    let face_box = ell(fx, fy, hx, hy + 0.6, 4.1, 5.3);

    // face details first: they sit on top of the head
    let helm = o & OUT_HELM != 0;
    let hood = o & OUT_HOOD != 0;
    if in_head {
        let (ex, ey) = (if ape { 2.4 } else { 2.3 }, 9.0);
        // eyes in a dark band under the brows
        for s in [-1.0f32, 1.0] {
            let cx = hx + s * ex;
            if fx > cx - 0.9 && fx < cx + 0.9 && fy > ey + 0.5 && fy < ey + 1.6 && !helm_covers(helm, fy) {
                return Some(Color32::from_rgb(14, 11, 11));
            }
            if fx > cx - 1.6 && fx < cx + 1.6 && fy > ey - 1.0 && fy < ey + (if ape { 0.6 } else { -0.2 }) && !helm_covers(helm, fy) {
                return shade(if ape { [skin[0] * 0.55, skin[1] * 0.5, skin[2] * 0.5] } else { hair.map(|c| c * 0.55) });
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
    if in_head {
        let style = if ape { 3 } else { lk.style };
        let hair_cap = fy < 5.6 + if style == 5 { 0.4 } else { 0.0 } || (fy < 7.4 && (fx - hx).abs() > 3.6);
        let shag = ape && !face_box && (fy < 10.5 || (fx - hx).abs() > 4.0);
        if !helm && ((hair_cap && !hood) || shag) {
            return shade(hair);
        }
        if !helm && !hood && lk.beard && !ape && fy > 12.6 {
            return shade(hair);
        }
        if ape && fy > 11.5 && fy < 12.4 && (fx - hx).abs() < 3.6 {
            return shade([skin[0] * 0.8, skin[1] * 0.76, skin[2] * 0.74]);
        }
        // an ape's face is bare and darker, in the shadow of the brow
        return shade(if ape { [skin[0] * 0.82, skin[1] * 0.78, skin[2] * 0.76] } else { skin });
    }
    // hair hanging beside the head and neck
    if !helm && !hood {
        let style = if ape { 3 } else { lk.style };
        let long = style == 1 && fy > 6.0 && fy < 19.0 && ((fx - hx).abs() > 4.4 && (fx - hx).abs() < 6.6);
        let braid = style == 4 && fx > hx + lk.braid_side * 5.0 - 0.9 && fx < hx + lk.braid_side * 5.0 + 0.9 && fy > 10.0 && fy < 21.0;
        let knot = style == 2 && ell(fx, fy, hx, 2.0, 2.0, 1.8);
        let bushy = style == 3 && !ape && ell(fx, fy, hx, 8.0, 6.3, 6.0) && fy < 10.0;
        if long || braid || knot || bushy {
            return shade(hair);
        }
    }
    // neck
    if fx > hx - 1.8 && fx < hx + 1.8 && fy >= 14.5 && fy < 19.0 {
        return shade([skin[0] * 0.86, skin[1] * 0.8, skin[2] * 0.78]);
    }
    // shoulders and chest
    let sh = lk.frame.0;
    if ell(fx, fy, hx, 25.0, 9.6 * sh, 7.6) && fy > 16.5 {
        if o & OUT_PLATE != 0 {
            let pauldron = (fx - hx).abs() > 6.0 * sh && fy < 21.5;
            return shade(if pauldron { [bronze[0] * 0.8, bronze[1] * 0.8, bronze[2] * 0.8] } else { bronze });
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
        if o & OUT_TUNIC != 0 {
            let collar = (fx - hx).abs() < 2.6 && fy < 19.5;
            return shade(if collar { linen } else { lk.dye });
        }
        if o & OUT_WRAP != 0 && fy > 20.0 {
            return shade([0.66, 0.70, 0.36]);
        }
        return shade(if ape { fur } else { skin });
    }
    None
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
    pxui::cells_cached(ui.ctx(), egui::Id::new("portrait"), key, ui.painter(), r, |p: Pos2| {
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
