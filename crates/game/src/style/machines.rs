//! Machine drawings: one pixel picture per family on a 56 x 40 cell grid. A cell is a whole number of interface
//! pixels (`pxui::unit()`), so every edge lies on the pixel grid at any size; curves are rows of different width.

use alaz_render::egui::{Color32, Painter, Pos2, Rect, Shape, Vec2};

use super::{accent, mix, palette, Era, Family};
use crate::pxui::{snap_pos, snap_rect, unit};

/// Grid size in cells.
const W: i32 = 56;
const H: i32 = 40;
/// Where the machine stands: the ground line, and the centre column of the drawing.
const GROUND: i32 = 30;
const CX: i32 = 28;

pub(crate) struct MachineState {
    pub fuel: f32,
    pub glow: f32,
    pub working: bool,
    pub blocked: bool,
    pub t: f32,
}

/// The machine tab reads these; until it does, only the tests do.
#[allow(dead_code)]
pub(crate) struct Anchors {
    pub input: Rect,
    pub fuel: Option<Rect>,
    pub output: Rect,
    pub progress: Rect,
}

/// One material in four flat bands: outline, shade, body, lit edge.
#[derive(Clone, Copy)]
struct Mat {
    edge: Color32,
    dark: Color32,
    mid: Color32,
    light: Color32,
}

fn ramp(mid: Color32, era: Era) -> Mat {
    // the screen era keeps its outlines bright, so parts stay apart on a near-black ground
    let edge = if era == Era::Computer { mix(mid, Color32::BLACK, 0.55) } else { mix(mid, Color32::BLACK, 0.78) };
    Mat { edge, dark: mix(mid, Color32::BLACK, 0.34), mid, light: mix(mid, Color32::WHITE, 0.3) }
}

fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// The era's masonry, wood and iron.
struct Kit {
    stone: Mat,
    wood: Mat,
    iron: Mat,
}

fn kit(era: Era) -> Kit {
    let (s, w, i) = match era {
        Era::Stone => (rgb(138, 124, 104), rgb(156, 108, 62), rgb(104, 98, 94)),
        Era::Metal => (rgb(124, 116, 108), rgb(134, 96, 62), rgb(134, 138, 148)),
        Era::Machine => (rgb(86, 118, 150), rgb(120, 104, 88), rgb(160, 180, 198)),
        Era::Computer => (rgb(28, 130, 70), rgb(40, 160, 86), rgb(64, 196, 112)),
    };
    Kit { stone: ramp(s, era), wood: ramp(w, era), iron: ramp(i, era) }
}

/// The colour of something hot: dull ember, orange, then yellow-white as `g` rises.
fn hot(g: f32) -> Color32 {
    let g = g.clamp(0.0, 1.0);
    if g < 0.5 {
        mix(rgb(44, 24, 20), rgb(226, 86, 24), g * 2.0)
    } else {
        mix(rgb(226, 86, 24), rgb(255, 232, 150), (g - 0.5) * 2.0)
    }
}

struct Pen {
    out: Vec<Shape>,
    o: Pos2,
    c: f32,
}

impl Pen {
    fn r(&mut self, x: i32, y: i32, w: i32, h: i32, col: Color32) {
        if w <= 0 || h <= 0 {
            return;
        }
        let min = self.o + Vec2::new(x as f32 * self.c, y as f32 * self.c);
        let rect = snap_rect(Rect::from_min_size(min, Vec2::new(w as f32 * self.c, h as f32 * self.c)));
        self.out.push(Shape::rect_filled(rect, 0.0, col));
    }

    fn cell(&self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect::from_min_size(self.o + Vec2::new(x as f32 * self.c, y as f32 * self.c), Vec2::new(w as f32 * self.c, h as f32 * self.c))
    }

    /// A box with an outline, a lit top and left edge and a shaded bottom and right edge.
    fn bx(&mut self, x: i32, y: i32, w: i32, h: i32, m: &Mat) {
        self.r(x, y, w, h, m.edge);
        self.r(x + 1, y + 1, w - 2, h - 2, m.mid);
        self.r(x + 1, y + 1, w - 2, 1, m.light);
        self.r(x + 1, y + 1, 1, h - 2, m.light);
        self.r(x + 1, y + h - 2, w - 2, 1, m.dark);
        self.r(x + w - 2, y + 1, 1, h - 2, m.dark);
    }

    /// A mass built from rows of half-width `hws` (bottom row first) around column `cx`, outlined, lit on the
    /// left and shaded on the right, with brick courses and staggered joints when `bricks`.
    fn mass(&mut self, cx: i32, base: i32, hws: &[i32], m: &Mat, bricks: bool) {
        let n = hws.len() as i32;
        for (k, &hw) in hws.iter().enumerate() {
            let y = base - 1 - k as i32;
            // a ledge where the next row up is narrower gets an outline row as wide as the row below it
            let below = if k > 0 { hws[k - 1] } else { hw };
            let e = hw.max(below) + 1;
            self.r(cx - e, y, 2 * e, 1, m.edge);
        }
        let top = *hws.last().unwrap_or(&1);
        self.r(cx - top - 1, base - 1 - n, 2 * top + 2, 1, m.edge);
        for (k, &hw) in hws.iter().enumerate() {
            let y = base - 1 - k as i32;
            self.r(cx - hw, y, 2 * hw, 1, m.mid);
            if hw >= 3 {
                self.r(cx - hw, y, 2, 1, m.light);
                self.r(cx + hw - 3, y, 3, 1, m.dark);
            }
            if bricks {
                let course = (k as i32) % 3;
                if course == 2 {
                    self.r(cx - hw + 2, y, 2 * hw - 4, 1, m.dark);
                } else {
                    let off = if (k as i32 / 3) % 2 == 0 { 0 } else { 2 };
                    let mut x = cx - hw + 3 + off;
                    while x < cx + hw - 3 {
                        self.r(x, y, 1, 1, m.dark);
                        x += 4;
                    }
                }
            }
        }
    }

    /// A flame of `h` rows on `base` (bottom edge), `w` wide at its foot, in three bands. `t` sways the tip.
    fn flame(&mut self, cx: i32, base: i32, h: i32, w: i32, t: f32, sway: bool) {
        for k in 0..h {
            let f = k as f32 / h as f32;
            let half = (((w as f32 / 2.0) * (1.0 - f).powf(0.8)).round() as i32).max(1);
            let s = if sway && k > h / 3 { ((t * 9.0 + k as f32 * 1.4).sin() * 1.2).round() as i32 } else { 0 };
            let y = base - 1 - k;
            self.r(cx - half + s, y, 2 * half, 1, rgb(214, 62, 24));
            if f < 0.78 && half > 1 {
                self.r(cx - half + 1 + s, y, 2 * half - 2, 1, rgb(250, 140, 36));
            }
            if f < 0.52 && half > 2 {
                self.r(cx - half + 2 + s, y, 2 * half - 4, 1, rgb(255, 226, 120));
            }
        }
    }

    /// Smoke puffs drifting up from (`x`, `y`): two grey bands, the old ones smaller.
    fn smoke(&mut self, x: i32, y: i32, t: f32, rise: i32) {
        for i in 0..4 {
            let ph = (t * 0.8 + i as f32 * 0.25).fract();
            let yy = y - (ph * rise as f32) as i32;
            let xx = x + (((ph + i as f32) * 6.0).sin() * 2.0).round() as i32;
            if ph < 0.5 {
                self.r(xx - 1, yy - 1, 2, 2, rgb(176, 172, 166));
            } else {
                self.r(xx, yy, 1, 1, rgb(124, 122, 118));
            }
        }
    }

    /// A pixel disc of radius `r` around (`cx`, `cy`).
    fn disc(&mut self, cx: i32, cy: i32, r: i32, col: Color32) {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r * r + r / 2 {
                    self.r(cx + dx, cy + dy, 1, 1, col);
                }
            }
        }
    }

    /// A one-cell line from (`x0`, `y0`) to (`x1`, `y1`).
    fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, col: Color32) {
        let n = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
        for i in 0..=n {
            let x = x0 + ((x1 - x0) as f32 * i as f32 / n as f32).round() as i32;
            let y = y0 + ((y1 - y0) as f32 * i as f32 / n as f32).round() as i32;
            self.r(x, y, 1, 1, col);
        }
    }
}

/// The cell size and origin that centre the grid in `r`, a whole number of interface pixels per cell when it fits.
fn layout(r: Rect) -> (f32, Pos2) {
    let fit = (r.width() / W as f32).min(r.height() / H as f32);
    let u = unit();
    let c = if fit >= u { (fit / u).floor() * u } else { fit };
    let o = snap_pos(r.center() - Vec2::new(W as f32 * c, H as f32 * c) / 2.0);
    (c, o)
}

fn anchors(pen: &Pen, fam: Family) -> Anchors {
    let burns = matches!(fam, Family::Fire | Family::Kiln | Family::Furnace);
    Anchors { input: pen.cell(1, 5, 12, 13), fuel: burns.then(|| pen.cell(22, 31, 12, 8)), output: pen.cell(43, 5, 12, 13), progress: pen.cell(1, 32, 19, 6) }
}

/// Draws `fam` in `era`'s materials into `r` (centred, as large as a whole cell size allows) and returns where the
/// machine tab puts its slots. `fuel` sizes the flame, `glow` heats the mouth, `working` sets the moving part
/// going, `blocked` outlines the machine in red.
pub(crate) fn draw_machine(p: &Painter, r: Rect, fam: Family, era: Era, st: &MachineState) -> Anchors {
    let (c, o) = layout(r);
    let mut pen = Pen { out: Vec::with_capacity(400), o, c };
    let k = kit(era);
    let acc = accent(fam);
    // idle machines keep their accent dim, working ones show it lit
    let lit = if st.working { acc } else { mix(acc, Color32::BLACK, 0.5) };
    let pal = palette(era);
    // a ground line the machine stands on
    pen.r(13, GROUND, 30, 1, mix(pal.slot_shadow, pal.light, 0.15));
    match fam {
        Family::Fire => fire(&mut pen, &k, st),
        Family::Kiln => kiln(&mut pen, &k, st),
        Family::Furnace => furnace(&mut pen, &k, st),
        Family::Smithing => smithing(&mut pen, &k, st, lit),
        Family::Grinding => grinding(&mut pen, &k, st, acc),
        Family::Bench => bench(&mut pen, &k, st, acc),
        Family::Textile => textile(&mut pen, &k, st, acc),
        Family::MachineTool => machine_tool(&mut pen, &k, st, lit),
        Family::Storage => storage(&mut pen, &k, st, acc),
        Family::Growing => growing(&mut pen, &k, st, acc),
        Family::Electric => electric(&mut pen, &k, st, acc),
    }
    if st.blocked {
        let red = rgb(232, 52, 40);
        pen.r(12, 1, 32, 1, red);
        pen.r(12, 31, 32, 1, red);
        pen.r(12, 1, 1, 31, red);
        pen.r(43, 1, 1, 31, red);
        // heavier corners, so the outline reads as a warning and not as a frame
        for (x, y) in [(12, 1), (41, 1), (12, 29), (41, 29)] {
            pen.r(x, y, 3, 3, red);
        }
    }
    let a = anchors(&pen, fam);
    p.extend(pen.out);
    a
}

fn fire(pen: &mut Pen, k: &Kit, st: &MachineState) {
    let burning = st.fuel > 0.0 && (st.working || st.glow > 0.05);
    // back stones, logs, flame, then the front stones over the flame's foot
    for (i, x) in [17, 22, 27, 32, 37].into_iter().enumerate() {
        pen.bx(x, 24 + (i as i32 % 2), 4, 3, &k.stone);
    }
    let charred = Mat { edge: rgb(20, 12, 8), dark: rgb(40, 24, 14), mid: rgb(64, 40, 22), light: rgb(92, 60, 32) };
    let log = if burning { k.wood } else { charred };
    pen.bx(20, 26, 17, 3, &log);
    pen.bx(23, 24, 11, 3, &log);
    if burning {
        let h = 4 + (st.fuel.clamp(0.0, 1.0) * 18.0) as i32;
        let w = 6 + (st.fuel.clamp(0.0, 1.0) * 6.0) as i32;
        pen.flame(CX, 27, h, w, st.t, st.working);
        if st.working {
            for i in 0..3 {
                let ph = (st.t * 1.3 + i as f32 / 3.0).fract();
                let x = CX - 4 + i * 4 + ((ph * 9.0).sin() * 2.0).round() as i32;
                pen.r(x, 27 - h - (ph * 6.0) as i32, 1, 1, rgb(255, 214, 96));
            }
        }
    } else if st.glow > 0.05 {
        for (x, y) in [(24, 26), (28, 25), (31, 26), (27, 27)] {
            pen.r(x, y, 1, 1, hot(st.glow));
        }
    }
    for (i, x) in [15, 20, 25, 30, 35].into_iter().enumerate() {
        pen.bx(x, 26 + (i as i32 % 2), 6, 4 - (i as i32 % 2), &k.stone);
    }
    pen.bx(40, 27, 3, 3, &k.stone);
}

fn kiln(pen: &mut Pen, k: &Kit, st: &MachineState) {
    let (rx, ry) = (13.0_f32, 20);
    let hws: Vec<i32> = (0..ry).map(|i| (rx * (1.0 - ((i as f32 + 0.5) / ry as f32).powi(2)).sqrt()).round() as i32).collect();
    pen.mass(CX, GROUND, &hws, &k.stone, true);
    // glow cracks in the dome
    if st.glow > 0.4 {
        let g = hot(st.glow * 0.9);
        for (x, y, w, h) in [(21, 17, 1, 2), (35, 21, 1, 2), (25, 13, 2, 1), (32, 15, 1, 2)] {
            pen.r(x, y, w, h, g);
        }
    }
    // the vent stack on top
    pen.bx(26, 6, 4, 4, &k.stone);
    pen.bx(25, 5, 6, 2, &k.stone);
    if st.working {
        pen.smoke(CX, 4, st.t, 10);
    }
    // the door: a lintel over a dark arch, hot inside
    let arch = [4, 4, 4, 4, 4, 4, 4, 3, 2];
    pen.r(CX - 6, GROUND - 11, 12, 11, k.stone.edge);
    pen.mass(CX, GROUND, &arch, &Mat { edge: k.stone.edge, dark: rgb(28, 18, 14), mid: hot(st.glow), light: mix(hot(st.glow), Color32::WHITE, 0.2) }, false);
    pen.r(CX - 2, GROUND - 6, 4, 6, mix(hot(st.glow), rgb(255, 240, 190), st.glow * 0.5));
    if st.fuel > 0.0 && (st.working || st.glow > 0.05) {
        pen.flame(CX, GROUND, 3 + (st.fuel * 4.0) as i32, 6, st.t, st.working);
    }
    pen.bx(CX - 7, GROUND - 12, 14, 2, &k.stone);
    for x in [CX - 7, CX + 5] {
        pen.bx(x, GROUND - 3, 2, 3, &k.stone);
    }
}

fn furnace(pen: &mut Pen, k: &Kit, st: &MachineState) {
    let hws: Vec<i32> = (0..24).map(|i| 10 - (i * 4) / 24).collect();
    pen.bx(14, 28, 28, 3, &k.stone);
    pen.mass(CX, 28, &hws, &k.stone, true);
    pen.bx(20, 2, 16, 3, &k.stone);
    // iron hoops
    for y in [11, 18] {
        let hw = 10 - ((28 - y) * 4) / 24 + 2;
        pen.bx(CX - hw, y, 2 * hw, 2, &k.iron);
    }
    // the mouth: a dark arch filled with heat
    pen.bx(CX - 7, 19, 14, 9, &k.stone);
    pen.r(CX - 5, 21, 10, 7, rgb(24, 16, 14));
    pen.r(CX - 4, 21, 8, 7, hot(st.glow));
    pen.r(CX - 2, 23, 4, 5, mix(hot(st.glow), rgb(255, 240, 190), st.glow * 0.6));
    if st.fuel > 0.0 && (st.working || st.glow > 0.05) {
        pen.flame(CX, 28, 3 + (st.fuel * 5.0) as i32, 7, st.t, st.working);
    }
    if st.glow > 0.4 {
        pen.r(CX - 5, 28, 10, 1, hot(st.glow * 0.8));
    }
    // fire and smoke at the throat
    if st.working && st.glow > 0.5 {
        pen.flame(CX, 2, 3 + (st.glow * 4.0) as i32, 6, st.t, true);
        pen.smoke(CX, -1, st.t, 8);
    }
}

fn smithing(pen: &mut Pen, k: &Kit, st: &MachineState, lit: Color32) {
    pen.bx(20, 25, 17, 5, &k.wood);
    pen.bx(19, 22, 19, 4, &k.iron);
    pen.r(21, 24, 15, 1, lit);
    pen.bx(22, 18, 13, 5, &k.iron);
    pen.bx(17, 14, 24, 5, &k.iron);
    pen.bx(13, 15, 5, 3, &k.iron);
    if st.glow > 0.05 || st.working {
        pen.r(24, 12, 10, 2, hot(st.glow.max(0.35)));
    }
    // the arm swings on a pivot top right; the head falls onto the bar
    let lift = if st.working { (0.5 + 0.5 * (st.t * 7.0).sin()).max(0.0) } else { 0.0 };
    let hy = 8 - (lift * 5.0).round() as i32;
    let hx = 25;
    let (px, py) = (40, 3);
    let n = px - (hx + 6);
    for i in 0..=n {
        let y = hy + 1 + ((py - hy - 1) as f32 * i as f32 / n as f32).round() as i32;
        pen.r(hx + 6 + i, y, 1, 2, k.wood.mid);
        pen.r(hx + 6 + i, y + 2, 1, 1, k.wood.dark);
    }
    pen.bx(hx, hy, 6, 4, &k.iron);
    pen.bx(px - 1, py - 1, 3, 3, &k.iron);
    if st.working && lift < 0.3 {
        for (x, y) in [(21, 10), (35, 11), (19, 12), (38, 9), (23, 8)] {
            pen.r(x, y, 1, 1, rgb(255, 214, 96));
        }
    }
}

fn grinding(pen: &mut Pen, k: &Kit, st: &MachineState, acc: Color32) {
    pen.bx(13, 24, 18, 5, &k.stone);
    pen.bx(14, 19, 16, 5, &k.stone);
    // grooves in the runner stone slide sideways while it turns
    let turn = if st.working { (st.t * 6.0) as i32 } else { 0 };
    for i in 0..4 {
        let x = 16 + ((i * 4 + turn) % 14);
        pen.r(x, 20, 1, 3, k.stone.dark);
    }
    // hopper: a funnel of wood over the eye of the stone
    for row in 0..7 {
        let hw = 7 - row;
        pen.r(22 - hw - 1, 6 + row, 2 * hw + 2, 1, k.wood.edge);
        pen.r(22 - hw, 6 + row, 2 * hw, 1, if row % 2 == 0 { k.wood.mid } else { k.wood.light });
    }
    pen.r(15, 6, 14, 1, k.wood.edge);
    pen.r(16, 7, 12, 1, rgb(222, 196, 120));
    pen.bx(20, 13, 4, 6, &k.wood);
    if st.working {
        pen.r(22, 14 + (st.t * 6.0) as i32 % 4, 1, 2, rgb(222, 196, 120));
    }
    // flour pile at the stone's foot
    for (x, w, y) in [(30, 6, 28), (31, 4, 27), (32, 2, 26)] {
        pen.r(x, y, w, 1, rgb(232, 224, 200));
    }
    // the wheel: rim, turning spokes, paddles, and water below
    let (wx, wy, wr) = (37, 21, 7);
    pen.r(30, 21, 3, 1, k.iron.mid);
    let spin = if st.working { st.t * 1.4 } else { 0.0 };
    for i in 0..36 {
        let a = i as f32 * std::f32::consts::TAU / 36.0;
        let (cx, cy) = (a.cos(), a.sin());
        pen.r(wx + (cx * wr as f32).round() as i32, wy + (cy * wr as f32).round() as i32, 1, 1, k.wood.mid);
        pen.r(wx + (cx * (wr - 1) as f32).round() as i32, wy + (cy * (wr - 1) as f32).round() as i32, 1, 1, k.wood.dark);
    }
    for i in 0..4 {
        let a = spin + i as f32 * std::f32::consts::FRAC_PI_4;
        for s in [-1.0_f32, 1.0] {
            for d in 1..wr {
                pen.r(wx + (a.cos() * d as f32 * s).round() as i32, wy + (a.sin() * d as f32 * s).round() as i32, 1, 1, k.wood.light);
            }
        }
    }
    for i in 0..8 {
        let a = spin + i as f32 * std::f32::consts::FRAC_PI_4;
        pen.r(wx + (a.cos() * (wr + 1) as f32).round() as i32, wy + (a.sin() * (wr + 1) as f32).round() as i32, 1, 1, k.wood.edge);
    }
    pen.bx(wx - 1, wy - 1, 3, 3, &k.iron);
    pen.r(30, 29, 14, 1, mix(acc, Color32::BLACK, 0.3));
    pen.r(30, 28, 14, 1, acc);
    let shift = if st.working { (st.t * 4.0) as i32 % 4 } else { 0 };
    for i in 0..4 {
        pen.r(31 + i * 4 + shift, 28, 2, 1, mix(acc, Color32::WHITE, 0.5));
    }
}

fn bench(pen: &mut Pen, k: &Kit, st: &MachineState, acc: Color32) {
    pen.bx(16, 19, 3, 11, &k.wood);
    pen.bx(37, 19, 3, 11, &k.wood);
    pen.r(19, 25, 18, 2, k.wood.dark);
    pen.bx(14, 15, 28, 5, &k.wood);
    pen.r(17, 20, 20, 1, mix(acc, Color32::BLACK, if st.working { 0.0 } else { 0.4 }));
    // a plank in the vice, a plane on it
    pen.bx(15, 10, 4, 5, &k.iron);
    pen.bx(20, 13, 20, 2, &k.wood);
    let px = if st.working { 26 + ((st.t * 3.0).sin() * 5.0).round() as i32 } else { 30 };
    pen.bx(px, 10, 6, 3, &k.iron);
    pen.r(px + 1, 9, 3, 1, k.wood.mid);
    if st.working {
        pen.r(px - 2, 12, 1, 1, rgb(236, 220, 176));
        pen.r(px - 4, 13, 1, 1, rgb(236, 220, 176));
    }
    // chisels standing in the corner
    for x in [37, 39] {
        pen.r(x, 11, 1, 3, k.iron.light);
        pen.r(x, 14, 1, 1, k.wood.mid);
    }
}

fn textile(pen: &mut Pen, k: &Kit, st: &MachineState, acc: Color32) {
    let thread = acc;
    let thread_d = mix(acc, Color32::BLACK, 0.3);
    pen.bx(15, 5, 3, 25, &k.wood);
    pen.bx(38, 5, 3, 25, &k.wood);
    pen.bx(15, 4, 26, 3, &k.wood);
    pen.bx(15, 27, 26, 3, &k.wood);
    let shed = st.working && ((st.t * 2.5).sin() > 0.0);
    for i in 0..9 {
        let x = 19 + i * 2;
        let s = if shed && i % 2 == 1 { 1 } else { 0 };
        pen.r(x, 7, 1, 7, thread);
        pen.r(x + s, 14, 1, 6, thread);
        pen.r(x, 20, 1, 7, thread);
    }
    // cloth: weft rows with a stripe pattern
    for y in 21..27 {
        pen.r(18, y, 21, 1, if y % 2 == 0 { thread } else { thread_d });
        for x in (19..38).step_by(4) {
            pen.r(x + (y % 2), y, 2, 1, mix(thread, Color32::BLACK, 0.55));
        }
    }
    // the shuttle runs across with its thread behind it
    let sx = if st.working { 19 + ((0.5 + 0.5 * (st.t * 2.5).sin()) * 14.0).round() as i32 } else { 33 };
    if st.working {
        pen.r(19, 16, sx - 19, 1, thread);
    }
    pen.bx(sx, 15, 5, 3, &k.wood);
    pen.r(sx + 1, 16, 3, 1, k.wood.dark);
}

fn machine_tool(pen: &mut Pen, k: &Kit, st: &MachineState, lit: Color32) {
    pen.bx(16, 28, 4, 2, &k.iron);
    pen.bx(36, 28, 4, 2, &k.iron);
    pen.bx(14, 25, 28, 4, &k.iron);
    pen.r(16, 26, 24, 1, lit);
    pen.bx(14, 13, 9, 12, &k.stone);
    pen.r(15, 15, 2, 2, lit);
    // the motor pulley on top, a marker turning round it
    pen.disc(18, 9, 3, k.iron.edge);
    pen.disc(18, 9, 2, k.iron.mid);
    let a = if st.working { st.t * 6.0 } else { 0.7 };
    pen.r(18 + (a.cos() * 2.0).round() as i32, 9 + (a.sin() * 2.0).round() as i32, 1, 1, k.iron.light);
    pen.r(18, 12, 1, 1, k.iron.dark);
    pen.bx(22, 15, 3, 8, &k.iron);
    pen.bx(25, 16, 11, 4, &k.iron);
    let turn = if st.working { (st.t * 10.0) as i32 } else { 0 };
    for i in 0..4 {
        pen.r(26 + (i * 3 + turn) % 9, 17, 1, 2, k.iron.dark);
    }
    pen.bx(36, 15, 5, 9, &k.stone);
    pen.r(35, 17, 1, 2, k.iron.light);
    let cx = if st.working { 27 + ((st.t * 0.8).sin() * 3.0).round() as i32 } else { 28 };
    pen.bx(cx, 21, 8, 4, &k.iron);
    pen.bx(cx + 2, 19, 4, 3, &k.iron);
    pen.r(cx + 3, 20, 2, 1, k.iron.light);
    if st.working {
        for (dx, dy) in [(5, 17), (7, 16), (3, 15)] {
            pen.r(cx + dx, dy, 1, 1, rgb(255, 214, 96));
        }
    }
}

fn storage(pen: &mut Pen, k: &Kit, st: &MachineState, acc: Color32) {
    pen.bx(14, 4, 3, 26, &k.wood);
    pen.bx(39, 4, 3, 26, &k.wood);
    for y in [12, 20, 28] {
        pen.bx(14, y, 28, 2, &k.wood);
        pen.r(17, y, 22, 1, mix(acc, Color32::BLACK, 0.4));
    }
    // (x, width, height, sack?) per shelf, bottom edge on the board
    type Item = (i32, i32, i32, bool);
    let rows: [(i32, [Item; 4]); 3] = [
        (12, [(17, 6, 6, false), (24, 5, 5, true), (30, 7, 4, false), (38, 1, 0, false)]),
        (20, [(17, 5, 5, true), (23, 7, 7, false), (31, 4, 4, false), (36, 3, 6, true)]),
        (28, [(17, 7, 5, false), (25, 5, 6, true), (31, 6, 7, false), (38, 1, 0, false)]),
    ];
    for (ri, (base, items)) in rows.iter().enumerate() {
        for (ii, &(x, w, h, sack)) in items.iter().enumerate() {
            if h == 0 {
                continue;
            }
            let bob = if st.working && ri == 0 && ii == 2 && (st.t * 3.0).sin() > 0.0 { 1 } else { 0 };
            let y = base - h - bob;
            if sack {
                let hws = [w / 2, w / 2, w / 2 - 1].map(|v| v.max(1));
                let rows: Vec<i32> = (0..h).map(|i| if i >= h - 1 { (w / 2 - 1).max(1) } else { hws[(i as usize).min(2)] }).collect();
                pen.mass(x + w / 2, base - bob, &rows, &Mat { edge: k.wood.edge, dark: rgb(150, 132, 100), mid: rgb(196, 176, 134), light: rgb(226, 210, 168) }, false);
            } else {
                pen.bx(x, y, w, h, &k.wood);
                pen.line(x + 1, y + 1, x + w - 2, y + h - 2, k.wood.dark);
                pen.line(x + w - 2, y + 1, x + 1, y + h - 2, k.wood.dark);
            }
        }
    }
}

fn growing(pen: &mut Pen, k: &Kit, st: &MachineState, acc: Color32) {
    let leaf_d = mix(acc, Color32::BLACK, 0.35);
    // the drying rack behind: posts, a rail and bundles hanging from it
    pen.bx(15, 6, 2, 16, &k.wood);
    pen.bx(39, 6, 2, 16, &k.wood);
    pen.bx(14, 6, 28, 2, &k.wood);
    for (i, x) in [18, 22, 26, 30, 34, 38].into_iter().enumerate() {
        let sway = if st.working && (st.t * 3.0 + i as f32).sin() > 0.3 { 1 } else { 0 };
        pen.r(x, 8, 1, 2, k.wood.dark);
        pen.r(x - 1 + sway, 10, 3, 4, acc);
        pen.r(x + sway, 14, 1, 2, leaf_d);
        pen.r(x - 1 + sway, 10, 1, 4, leaf_d);
    }
    // three ridges of soil, rows of sprouts along them
    let soil = Mat { edge: rgb(30, 20, 12), dark: rgb(60, 40, 24), mid: rgb(92, 62, 38), light: rgb(122, 86, 54) };
    pen.r(13, 22, 30, 8, soil.edge);
    for (i, y) in [23, 26, 29 - 1].into_iter().enumerate() {
        pen.r(14, y, 28, 2, soil.mid);
        pen.r(14, y, 28, 1, soil.light);
        pen.r(14, y + 2, 28, 1, soil.dark);
        for j in 0..6 {
            let x = 16 + j * 4 + (i as i32 % 2) * 2;
            let h = 2 + (crate::pxui::hash(j, i as i32, 5) % 3) as i32;
            let s = if st.working && (st.t * 2.0 + j as f32).sin() > 0.4 { 1 } else { 0 };
            pen.r(x, y - h, 1, h, acc);
            pen.r(x - 1 + s, y - h, 1, 1, leaf_d);
            pen.r(x + 1 + s, y - h + 1, 1, 1, acc);
        }
    }
}

fn electric(pen: &mut Pen, k: &Kit, st: &MachineState, acc: Color32) {
    pen.bx(14, 4, 28, 26, &k.iron);
    pen.r(15, 5, 26, 1, if st.working { acc } else { mix(acc, Color32::BLACK, 0.5) });
    // the screen: dark glass, a faint grid, a trace that runs only while the machine does
    pen.r(17, 7, 22, 13, k.iron.edge);
    pen.r(18, 8, 20, 11, rgb(6, 18, 24));
    for x in (20..38).step_by(5) {
        for y in (10..18).step_by(4) {
            pen.r(x, y, 1, 1, mix(acc, Color32::BLACK, 0.75));
        }
    }
    for x in 0..20 {
        let y = if st.working { 13 + (3.0 * ((x as f32 + st.t * 8.0) * 0.5).sin()).round() as i32 } else { 13 };
        pen.r(18 + x, y, 1, 1, if st.working { acc } else { mix(acc, Color32::BLACK, 0.55) });
    }
    pen.r(19, 9, 6, 1, mix(acc, Color32::BLACK, 0.45));
    // lamps, knobs and vents
    for i in 0..4 {
        let on = st.working && ((st.t * 2.0) as i32 + i) % 4 != 0;
        let col = if st.blocked && i == 3 { rgb(232, 52, 40) } else if on { rgb(130, 255, 140) } else { rgb(24, 52, 30) };
        pen.bx(18 + i * 3, 22, 2, 2, &Mat { edge: k.iron.edge, dark: col, mid: col, light: col });
    }
    for x in [31, 35] {
        pen.disc(x + 1, 23, 1, k.iron.edge);
        pen.r(x, 22, 2, 2, k.iron.light);
        pen.r(x + 1, 22, 1, 1, k.iron.dark);
    }
    for y in [26, 28] {
        pen.r(17, y, 22, 1, k.iron.edge);
    }
    // the mains cable leaves at the foot
    pen.r(27, 29, 2, 1, k.iron.dark);
}

#[cfg(test)]
mod tests {
    use super::*;
    use alaz_render::egui::{self, LayerId, Pos2, Vec2};

    const ALL: [Family; 11] = [Family::Fire, Family::Kiln, Family::Furnace, Family::Smithing, Family::Grinding, Family::Bench, Family::Textile, Family::MachineTool, Family::Storage, Family::Growing, Family::Electric];
    const ERAS: [Era; 4] = [Era::Stone, Era::Metal, Era::Machine, Era::Computer];

    fn painter() -> Painter {
        egui::Context::default().layer_painter(LayerId::background())
    }

    #[test]
    fn anchors_lie_inside_the_drawing() {
        let p = painter();
        let st = MachineState { fuel: 0.8, glow: 0.9, working: true, blocked: false, t: 1.0 };
        for size in [Vec2::new(420.0, 300.0), Vec2::new(300.0, 300.0), Vec2::new(560.0, 400.0)] {
            let r = Rect::from_min_size(Pos2::new(37.0, 21.0), size);
            for fam in ALL {
                for era in ERAS {
                    let a = draw_machine(&p, r, fam, era, &st);
                    let slack = r.expand(0.01);
                    let mut all = vec![a.input, a.output, a.progress];
                    all.extend(a.fuel);
                    for q in all {
                        assert!(q.width() > 0.0 && q.height() > 0.0 && slack.contains_rect(q), "{fam:?} {era:?} {q:?} outside {r:?}");
                    }
                    assert!(!a.input.intersects(a.output), "{fam:?} {era:?}: input and output overlap");
                }
            }
        }
    }

    #[test]
    fn fuel_anchor_only_for_burning_families() {
        let p = painter();
        let st = MachineState { fuel: 0.0, glow: 0.0, working: false, blocked: false, t: 0.0 };
        let r = Rect::from_min_size(Pos2::ZERO, Vec2::new(420.0, 300.0));
        for fam in ALL {
            let a = draw_machine(&p, r, fam, Era::Metal, &st);
            let burns = matches!(fam, Family::Fire | Family::Kiln | Family::Furnace);
            assert_eq!(a.fuel.is_some(), burns, "{fam:?}");
        }
    }
}
