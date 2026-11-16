//! The industrial machines, drawn in code from boxes and rods. Each one is told apart by its silhouette first (a tall
//! stack, a long bed, a tilted vessel) and by what it does while it works: the moving parts turn or slide, hot
//! mouths glow, chimneys smoke.
//!
//! Moving parts are drawn through `moving`, which gives them the same roles a model file gets from a node name
//! (`spin_`, `spinx_`, `spinz_`, `piston_`). The exporter asks for them at rest and writes them as such nodes, so an
//! exported machine is animated by the same rules when it comes back from a file.

use super::buildings::push_box;
use super::*;
use std::f32::consts::TAU;

const BRICK: [f32; 3] = [0.62, 0.34, 0.26];
const BRICK_D: [f32; 3] = [0.50, 0.27, 0.21];
const IRON: [f32; 3] = [0.46, 0.47, 0.53];
const IRON_D: [f32; 3] = [0.35, 0.36, 0.42];
const STEEL: [f32; 3] = [0.80, 0.84, 0.90];
const WOOD: [f32; 3] = [0.45, 0.31, 0.17];
const WOOD_L: [f32; 3] = [0.54, 0.38, 0.21];
const LIME: [f32; 3] = [0.88, 0.86, 0.78];
const CONC: [f32; 3] = [0.64, 0.64, 0.62];
const SOOT: [f32; 3] = [0.08, 0.07, 0.07];
const FIRE: [f32; 3] = [1.0, 0.5, 0.12];
const BRASS: [f32; 3] = [0.80, 0.60, 0.22];
const CLOTH: [f32; 3] = [0.80, 0.74, 0.55];

/// How a moving part moves: about an axis (0 x, 1 y, 2 z), or up and down.
#[derive(Clone, Copy)]
enum Move {
    Spin(usize),
    Piston,
}

/// Draw a part that moves while the machine works. It is drawn at rest by `draw`, then turned about `pivot` by
/// `tick * speed` radians (or lifted by `amp * |sin(tick * speed)|` metres) when `active`. While the exporter is
/// collecting parts the part stays at rest and is listed instead.
#[allow(clippy::too_many_arguments)]
fn moving(out: &mut Mesh, name: &str, mv: Move, pivot: [f32; 3], speed: f32, amp: f32, active: bool, tick: u64, draw: impl FnOnce(&mut Mesh)) {
    let from = out.len();
    draw(out);
    let to = out.len();
    if let Some(marks) = out.marks.as_mut() {
        let prefix = match mv {
            Move::Spin(0) => "spinx_",
            Move::Spin(2) => "spinz_",
            Move::Spin(_) => "spin_",
            Move::Piston => "piston_",
        };
        marks.push(PartMark { name: format!("{prefix}{name}"), pivot, from, to, speed, amp });
        return;
    }
    if !active {
        return;
    }
    let a = tick as f32 * speed;
    for v in &mut out.verts[from..to] {
        match mv {
            Move::Spin(axis) => {
                let (s, c) = a.sin_cos();
                let d = [v.pos[0] - pivot[0], v.pos[1] - pivot[1], v.pos[2] - pivot[2]];
                let (i, j) = match axis {
                    0 => (1, 2),
                    1 => (2, 0),
                    _ => (0, 1),
                };
                let mut r = d;
                r[i] = d[i] * c - d[j] * s;
                r[j] = d[i] * s + d[j] * c;
                v.pos = [pivot[0] + r[0], pivot[1] + r[1], pivot[2] + r[2]];
            }
            Move::Piston => v.pos[1] += amp * a.sin().abs(),
        }
    }
}

/// A flat disc of spokes and a rim standing in the plane across `axis`: a flywheel, a pulley, a spinning wheel.
fn wheel(out: &mut Mesh, c: [f32; 3], axis: usize, r: f32, col: [f32; 3], spokes: usize) {
    let at = |a: f32, rr: f32| match axis {
        0 => [c[0], c[1] + a.cos() * rr, c[2] + a.sin() * rr],
        2 => [c[0] + a.cos() * rr, c[1] + a.sin() * rr, c[2]],
        _ => [c[0] + a.cos() * rr, c[1], c[2] + a.sin() * rr],
    };
    let n = 12;
    for k in 0..n {
        let (a0, a1) = (k as f32 * TAU / n as f32, (k + 1) as f32 * TAU / n as f32);
        shapes::limb(out, at(a0, r), at(a1, r), r * 0.1, r * 0.1, col, 4);
    }
    for k in 0..spokes {
        let a = k as f32 * TAU / spokes as f32;
        shapes::limb(out, c, at(a, r), r * 0.06, r * 0.06, col, 4);
    }
    shapes::sphere(out, c, [r * 0.18; 3], col, 6, 4, 0.0, 3);
}

/// The industrial machines (see `BuildingKind`); anything else is not drawn here.
pub(crate) fn push_machine(out: &mut Mesh, kind: BuildingKind, x: f32, z: f32, active: bool, tick: u64, tint: Option<[f32; 3]>) {
    let t = tick as f32;
    let bx = |out: &mut Mesh, base: [f32; 3], half: [f32; 3], col: [f32; 3], glow: f32| match tint {
        Some(c) => push_box(out, base, half, c, 1.0),
        None => push_box(out, base, half, col, glow),
    };
    let rd = |out: &mut Mesh, c: [f32; 3], r: [f32; 3], col: [f32; 3]| match tint {
        Some(c2) => shapes::sphere(out, c, r, c2, 12, 8, 0.0, 11),
        None => shapes::sphere(out, c, r, col, 12, 8, 0.02, 11),
    };
    let lb = |out: &mut Mesh, a: [f32; 3], b: [f32; 3], r0: f32, r1: f32, col: [f32; 3], segs: usize| shapes::limb(out, a, b, r0, r1, tint.unwrap_or(col), segs);
    // a few grey puffs rising from a stack while the machine works
    let smoke = |out: &mut Mesh, sx: f32, sy: f32, sz: f32, big: f32| {
        if !active || tint.is_some() {
            return;
        }
        let [wx, wz, _] = out.wind;
        for k in 0..3u32 {
            let age = (t * 0.012 + k as f32 / 3.0).fract();
            let p = [sx + (t * 0.03 + k as f32 * 2.0).sin() * 0.05 * age + wx * 1.4 * age * age, sy + age * 1.8, sz + wz * 1.4 * age * age];
            let r = (0.07 + 0.22 * age) * big * (1.0 - age * 0.45);
            let g = 0.40 + 0.22 * age;
            shapes::sphere(out, p, [r, r * 0.85, r], [g, g, g * 1.02], 6, 4, 0.25, k);
        }
    };
    // a flickering glow: 0.5..1
    let flick = |k: f32| 0.75 + 0.25 * (t * 0.37 + k * 1.9).sin();
    out.set_anchor(x, z);
    match kind {
        BuildingKind::CokeOven => {
            // a long low battery of brick ovens with their iron doors, a tall stack, a barrel of tar
            bx(out, [x, 0.0, z], [1.0, 0.9, 0.55], BRICK, 0.0);
            bx(out, [x, 0.9, z], [1.05, 0.1, 0.6], BRICK_D, 0.0);
            for k in [-1.0f32, 0.0, 1.0] {
                bx(out, [x + k * 0.62, 0.12, z + 0.55], [0.22, 0.55, 0.03], SOOT, 0.0);
                bx(out, [x + k * 0.62, 0.99, z], [0.14, 0.1, 0.14], IRON, 0.0);
                if active {
                    bx(out, [x + k * 0.62, 0.3, z + 0.58], [0.16, 0.16 * flick(k), 0.02], FIRE, 1.0);
                }
            }
            bx(out, [x + 0.78, 1.0, z - 0.1], [0.16, 1.7, 0.16], BRICK_D, 0.0);
            bx(out, [x + 0.78, 2.7, z - 0.1], [0.2, 0.08, 0.2], IRON, 0.0);
            rd(out, [x - 0.95, 0.22, z + 0.6], [0.2, 0.22, 0.2], [0.10, 0.09, 0.10]);
            lb(out, [x - 0.9, 0.8, z + 0.4], [x - 0.95, 0.4, z + 0.6], 0.04, 0.04, IRON, 4);
            smoke(out, x + 0.78, 2.8, z - 0.1, 1.0);
        }
        BuildingKind::LimeKiln => {
            // a squat pale shaft kiln, chalk white, with a charging ring on top and a fire mouth at its foot
            rd(out, [x, 0.7, z], [0.8, 0.85, 0.8], LIME);
            rd(out, [x, 1.4, z], [0.5, 0.4, 0.5], [0.80, 0.78, 0.70]);
            bx(out, [x - 0.18, 0.0, z + 0.62], [0.36, 0.5, 0.18], SOOT, 0.0);
            if active {
                bx(out, [x - 0.14, 0.05, z + 0.78], [0.28, 0.3 * flick(0.0), 0.02], FIRE, 1.0);
            }
            for (dx, dz, r) in [(0.7f32, 0.4f32, 0.16f32), (0.85, 0.15, 0.12), (0.62, 0.7, 0.13)] {
                rd(out, [x + dx, r * 0.6, z + dz], [r, r * 0.7, r], [0.92, 0.91, 0.86]);
            }
            smoke(out, x, 1.7, z, 1.0);
        }
        BuildingKind::CementKiln => {
            // a long steel drum lying a little tilted on concrete piers, ribbed so its turning shows, a firebox at the low end
            for sx in [-0.75f32, 0.45] {
                bx(out, [x + sx, 0.0, z], [0.18, 0.62, 0.45], CONC, 0.0);
            }
            moving(out, "drum", Move::Spin(0), [x, 1.05, z], 0.05, 0.0, active, tick, |o| {
                lb(o, [x - 1.0, 1.05, z], [x + 0.7, 1.05, z], 0.42, 0.42, [0.66, 0.62, 0.58], 10);
                for k in 0..3 {
                    let bxp = x - 0.7 + k as f32 * 0.55;
                    bx(o, [bxp, 1.05 + 0.4, z], [0.07, 0.08, 0.14], IRON_D, 0.0);
                    bx(o, [bxp, 1.05 - 0.48, z], [0.07, 0.08, 0.14], IRON_D, 0.0);
                }
            });
            bx(out, [x + 0.7, 0.0, z], [0.32, 1.5, 0.5], BRICK, 0.0);
            bx(out, [x + 0.7, 0.2, z + 0.5], [0.2, 0.4, 0.03], SOOT, 0.0);
            if active {
                bx(out, [x + 0.7, 0.25, z + 0.53], [0.16, 0.28 * flick(1.0), 0.02], FIRE, 1.0);
            }
            smoke(out, x + 0.7, 1.55, z, 1.2);
        }
        BuildingKind::BrickKiln => {
            // a clamp of brick with an arched top, a firing mouth, a stack of fresh bricks beside it
            bx(out, [x, 0.0, z], [0.8, 0.8, 0.75], BRICK, 0.0);
            rd(out, [x, 0.8, z], [0.8, 0.38, 0.75], BRICK_D);
            for k in 0..3 {
                bx(out, [x, 0.2 + k as f32 * 0.22, z + 0.755], [0.78, 0.015, 0.005], [0.78, 0.74, 0.66], 0.0);
            }
            bx(out, [x - 0.28, 0.0, z + 0.75], [0.56, 0.5, 0.04], SOOT, 0.0);
            if active {
                bx(out, [x - 0.2, 0.05, z + 0.79], [0.4, 0.36 * flick(2.0), 0.02], FIRE, 1.0);
            }
            for layer in 0..4 {
                let off = if layer % 2 == 0 { 0.0 } else { 0.1 };
                bx(out, [x + 0.45 + off, layer as f32 * 0.1, z + 0.45], [0.28, 0.1, 0.14], [0.70, 0.40, 0.30], 0.0);
            }
            bx(out, [x - 0.6, 1.0, z - 0.5], [0.12, 0.9, 0.12], BRICK_D, 0.0);
            smoke(out, x - 0.6, 1.9, z - 0.5, 0.8);
        }
        BuildingKind::PuddlingFurnace => {
            // a low flame furnace with iron doors and a long rabble rod through one, a tall thin stack
            bx(out, [x, 0.0, z], [0.9, 0.7, 0.6], BRICK, 0.0);
            rd(out, [x, 0.7, z], [0.9, 0.24, 0.6], BRICK_D);
            for dx in [-0.42f32, 0.18] {
                bx(out, [x + dx, 0.18, z + 0.6], [0.3, 0.38, 0.03], IRON, 0.0);
                if active {
                    bx(out, [x + dx + 0.05, 0.3, z + 0.64], [0.2, 0.12 * flick(dx), 0.015], FIRE, 1.0);
                }
            }
            moving(out, "rabble", Move::Piston, [x - 0.4, 0.4, z + 0.8], 0.12, 0.08, active, tick, |o| {
                lb(o, [x - 0.4, 0.4, z + 0.62], [x - 0.4, 0.45, z + 1.35], 0.03, 0.03, IRON, 4);
                bx(o, [x - 0.4, 0.4, z + 1.3], [0.12, 0.1, 0.04], IRON_D, 0.0);
            });
            bx(out, [x + 0.72, 0.7, z - 0.25], [0.14, 2.2, 0.14], BRICK_D, 0.0);
            smoke(out, x + 0.72, 2.95, z - 0.25, 0.9);
        }
        BuildingKind::Bessemer => {
            // a pear-shaped vessel on trunnions between two iron stands, mouth tipped up; a plume of flame when blowing
            bx(out, [x, 0.0, z], [0.95, 0.12, 0.7], CONC, 0.0);
            for sx in [-0.62f32, 0.62] {
                bx(out, [x + sx, 0.12, z], [0.13, 1.1, 0.4], IRON, 0.0);
            }
            lb(out, [x - 0.75, 1.25, z], [x + 0.75, 1.25, z], 0.08, 0.08, IRON_D, 5);
            rd(out, [x, 1.35, z], [0.55, 0.62, 0.5], [0.52, 0.54, 0.60]);
            lb(out, [x + 0.1, 1.75, z], [x + 0.42, 2.25, z], 0.34, 0.22, [0.46, 0.48, 0.54], 8);
            rd(out, [x, 1.0, z + 0.0], [0.5, 0.06, 0.46], [0.48, 0.30, 0.20]);
            lb(out, [x - 0.8, 0.35, z + 0.3], [x - 0.5, 0.9, z + 0.05], 0.08, 0.08, IRON, 4);
            if active {
                for k in 0..5u32 {
                    let h = 2.3 + k as f32 * 0.32;
                    let w = 0.2 - 0.03 * k as f32;
                    let sway = (t * 0.2 + k as f32 * 1.3).sin() * 0.06 * k as f32;
                    let c = if k < 2 { [1.0, 0.85, 0.5] } else { [1.0, 0.45 + 0.1 * flick(k as f32), 0.1] };
                    bx(out, [x + 0.42 + sway, h, z], [w, 0.34, w], c, 1.0);
                }
            }
        }
        BuildingKind::Cupola => {
            // a tall steel shaft on legs with a brick-lined top, a charging door, a blast pipe, a tapping spout
            for (dx, dz) in [(-0.4f32, -0.3f32), (0.4, -0.3), (-0.4, 0.3), (0.4, 0.3)] {
                bx(out, [x + dx, 0.0, z + dz], [0.07, 0.4, 0.07], IRON_D, 0.0);
            }
            lb(out, [x, 0.4, z], [x, 2.3, z], 0.55, 0.5, [0.54, 0.56, 0.62], 10);
            lb(out, [x, 2.3, z], [x, 2.9, z], 0.5, 0.42, [0.56, 0.36, 0.28], 10);
            for h in [0.9f32, 1.6] {
                lb(out, [x, h, z], [x, h + 0.06, z], 0.57, 0.57, IRON_D, 10);
            }
            bx(out, [x - 0.2, 1.45, z + 0.46], [0.4, 0.4, 0.05], SOOT, 0.0);
            lb(out, [x - 1.0, 0.7, z], [x - 0.5, 0.7, z], 0.1, 0.1, IRON, 5);
            lb(out, [x + 0.45, 0.55, z], [x + 1.0, 0.3, z], 0.11, 0.09, IRON_D, 5);
            if active {
                bx(out, [x - 0.15, 1.5, z + 0.5], [0.3, 0.3 * flick(3.0), 0.02], FIRE, 1.0);
                rd(out, [x + 1.05, 0.22, z], [0.14, 0.08, 0.12], [1.0, 0.55, 0.15]);
                shapes::sphere(out, [x + 1.05, 0.24, z], [0.1, 0.05, 0.09], [1.0, 0.85, 0.4], 6, 4, 0.0, 5);
            }
            smoke(out, x, 3.0, z, 1.0);
        }
        BuildingKind::MoldingBench => {
            // a bench with sand boxes, a heap of sand and a rammer beside them
            bx(out, [x, 0.5, z], [0.9, 0.1, 0.5], WOOD_L, 0.0);
            for (dx, dz) in [(-0.8f32, -0.4f32), (0.8, -0.4), (-0.8, 0.4), (0.8, 0.4)] {
                bx(out, [x + dx, 0.0, z + dz], [0.06, 0.5, 0.06], WOOD, 0.0);
            }
            for sx in [-0.45f32, 0.25] {
                bx(out, [x + sx, 0.6, z], [0.3, 0.14, 0.22], WOOD, 0.0);
                bx(out, [x + sx, 0.73, z], [0.26, 0.015, 0.18], [0.78, 0.68, 0.46], 0.0);
            }
            rd(out, [x + 0.7, 0.66, z + 0.05], [0.18, 0.12, 0.16], [0.78, 0.68, 0.46]);
            rd(out, [x - 0.75, 0.14, z + 0.65], [0.28, 0.14, 0.24], [0.78, 0.68, 0.46]);
            moving(out, "rammer", Move::Piston, [x - 0.1, 0.75, z], 0.2, 0.18, active, tick, |o| {
                lb(o, [x - 0.1, 0.75, z - 0.05], [x - 0.1, 1.35, z - 0.05], 0.035, 0.035, WOOD, 4);
                bx(o, [x - 0.18, 0.72, z - 0.12], [0.16, 0.06, 0.14], IRON, 0.0);
            });
        }
        BuildingKind::Lathe => {
            // a long iron bed with headstock, chuck, tailstock and tool post; belt drive from an overhead pulley
            bx(out, [x, 0.4, z], [0.9, 0.12, 0.26], IRON, 0.0);
            for sx in [-0.7f32, 0.7] {
                bx(out, [x + sx, 0.0, z], [0.14, 0.4, 0.22], IRON_D, 0.0);
            }
            bx(out, [x - 0.62, 0.52, z], [0.28, 0.42, 0.28], IRON_D, 0.0);
            bx(out, [x + 0.62, 0.52, z], [0.14, 0.26, 0.2], IRON, 0.0);
            moving(out, "chuck", Move::Spin(0), [x - 0.3, 0.8, z], 0.35, 0.0, active, tick, |o| {
                lb(o, [x - 0.4, 0.8, z], [x - 0.28, 0.8, z], 0.2, 0.2, STEEL, 8);
                bx(o, [x - 0.36, 0.8 + 0.14, z - 0.03], [0.06, 0.1, 0.06], [0.90, 0.92, 0.96], 0.0);
                lb(o, [x - 0.28, 0.8, z], [x + 0.55, 0.8, z], 0.06, 0.06, STEEL, 6);
            });
            bx(out, [x + 0.05, 0.52, z + 0.2], [0.12, 0.24, 0.1], IRON, 0.0);
            moving(out, "pulley", Move::Spin(2), [x - 0.62, 1.45, z - 0.15], 0.16, 0.0, active, tick, |o| {
                wheel(o, [x - 0.62, 1.45, z - 0.15], 2, 0.3, WOOD_L, 6);
            });
            lb(out, [x - 0.62, 1.2, z - 0.15], [x - 0.62, 0.95, z - 0.15], 0.025, 0.025, [0.2, 0.15, 0.1], 3);
            if active {
                bx(out, [x + 0.0, 0.76, z + 0.12], [0.05, 0.05 * flick(4.0), 0.05], [1.0, 0.85, 0.4], 1.0);
            }
        }
        BuildingKind::DrillMill => {
            // a column with a head and a drill spindle over a table that rises to the cut
            bx(out, [x, 0.0, z], [0.6, 0.14, 0.5], IRON_D, 0.0);
            bx(out, [x - 0.38, 0.14, z - 0.2], [0.14, 1.75, 0.14], IRON, 0.0);
            bx(out, [x - 0.05, 1.5, z - 0.12], [0.45, 0.38, 0.28], IRON_D, 0.0);
            moving(out, "drill", Move::Spin(1), [x + 0.1, 1.2, z], 0.4, 0.0, active, tick, |o| {
                lb(o, [x + 0.1, 1.5, z], [x + 0.1, 0.85, z], 0.05, 0.03, STEEL, 6);
                bx(o, [x + 0.15, 0.9, z - 0.01], [0.02, 0.45, 0.02], [0.92, 0.94, 0.98], 0.0);
            });
            moving(out, "table", Move::Piston, [x + 0.1, 0.5, z], 0.1, 0.1, active, tick, |o| {
                bx(o, [x + 0.1, 0.42, z], [0.36, 0.08, 0.3], STEEL, 0.0);
                bx(o, [x + 0.1, 0.5, z], [0.22, 0.14, 0.16], [0.46, 0.48, 0.52], 0.0);
            });
            bx(out, [x + 0.1, 0.14, z], [0.1, 0.28, 0.1], IRON, 0.0);
            wheel(out, [x + 0.32, 0.85, z + 0.3], 2, 0.14, IRON, 3);
        }
        BuildingKind::Press => {
            // a heavy frame, a ram coming down on a die, a flywheel on the side
            bx(out, [x, 0.0, z], [0.85, 0.2, 0.6], CONC, 0.0);
            for sx in [-0.55f32, 0.55] {
                bx(out, [x + sx, 0.2, z], [0.14, 1.45, 0.38], IRON, 0.0);
            }
            bx(out, [x, 1.6, z], [0.75, 0.26, 0.4], IRON_D, 0.0);
            bx(out, [x, 0.2, z], [0.36, 0.1, 0.3], STEEL, 0.0);
            bx(out, [x, 0.3, z], [0.32, 0.02, 0.26], [0.74, 0.76, 0.80], 0.0);
            moving(out, "ram", Move::Piston, [x, 1.0, z], 0.12, 0.34, active, tick, |o| {
                lb(o, [x, 1.6, z], [x, 1.05, z], 0.1, 0.1, STEEL, 6);
                bx(o, [x, 0.78, z], [0.3, 0.26, 0.26], [0.62, 0.64, 0.70], 0.0);
            });
            moving(out, "flywheel", Move::Spin(2), [x + 0.78, 1.15, z], 0.09, 0.0, active, tick, |o| {
                wheel(o, [x + 0.78, 1.15, z], 2, 0.4, IRON, 6);
            });
        }
        BuildingKind::Sawmill => {
            // an open timber frame with a log on a carriage and a saw gate going up and down through it
            for (dx, dz) in [(-1.0f32, -0.5f32), (1.0, -0.5), (-1.0, 0.5), (1.0, 0.5)] {
                bx(out, [x + dx, 0.0, z + dz], [0.08, 1.9, 0.08], WOOD, 0.0);
            }
            for dz in [-0.5f32, 0.5] {
                bx(out, [x, 1.9, z + dz], [1.05, 0.1, 0.07], WOOD_L, 0.0);
            }
            for dx in [-1.0f32, 1.0] {
                bx(out, [x + dx, 1.9, z], [0.07, 0.1, 0.5], WOOD_L, 0.0);
            }
            bx(out, [x, 0.3, z], [1.0, 0.08, 0.32], WOOD, 0.0);
            lb(out, [x - 0.95, 0.6, z], [x + 0.95, 0.6, z], 0.24, 0.24, [0.38, 0.26, 0.15], 8);
            moving(out, "gate", Move::Piston, [x, 0.8, z], 0.25, 0.3, active, tick, |o| {
                bx(o, [x - 0.0, 0.4, z - 0.3], [0.04, 1.5, 0.04], WOOD_L, 0.0);
                bx(o, [x - 0.0, 0.4, z + 0.3], [0.04, 1.5, 0.04], WOOD_L, 0.0);
                bx(o, [x - 0.0, 0.55, z], [0.025, 1.0, 0.3], STEEL, 0.0);
            });
            moving(out, "crank", Move::Spin(2), [x + 1.1, 1.0, z + 0.62], 0.12, 0.0, active, tick, |o| {
                wheel(o, [x + 1.1, 1.0, z + 0.62], 2, 0.32, WOOD_L, 6);
            });
            for k in 0..3 {
                bx(out, [x - 0.7, k as f32 * 0.08, z + 0.85], [0.4, 0.08, 0.16], [0.80, 0.66, 0.44], 0.0);
            }
            rd(out, [x + 0.6, 0.06, z + 0.85], [0.35, 0.08, 0.2], [0.78, 0.64, 0.42]);
        }
        BuildingKind::SpinningWheel => {
            // a big wheel on a bench, a distaff of fibre, a spindle with a bobbin of thread
            bx(out, [x, 0.3, z], [0.4, 0.06, 0.16], WOOD_L, 0.0);
            for (dx, dz) in [(-0.34f32, 0.0f32), (0.34, -0.1), (0.34, 0.1)] {
                bx(out, [x + dx, 0.0, z + dz], [0.03, 0.3, 0.03], WOOD, 0.0);
            }
            bx(out, [x - 0.1, 0.36, z - 0.1], [0.03, 0.62, 0.03], WOOD, 0.0);
            bx(out, [x - 0.1, 0.36, z + 0.1], [0.03, 0.62, 0.03], WOOD, 0.0);
            moving(out, "wheel", Move::Spin(2), [x - 0.1, 0.75, z], 0.2, 0.0, active, tick, |o| {
                wheel(o, [x - 0.1, 0.75, z - 0.13], 2, 0.34, WOOD_L, 8);
                bx(o, [x - 0.1 + 0.3, 0.75, z - 0.15], [0.03, 0.03, 0.04], [0.9, 0.9, 0.85], 0.0);
            });
            lb(out, [x + 0.3, 0.36, z], [x + 0.36, 0.95, z], 0.025, 0.025, WOOD, 4);
            rd(out, [x + 0.36, 1.0, z], [0.09, 0.12, 0.09], [0.86, 0.84, 0.66]);
            lb(out, [x + 0.15, 0.55, z], [x + 0.32, 0.55, z], 0.02, 0.02, [0.7, 0.7, 0.66], 3);
            rd(out, [x + 0.12, 0.55, z], [0.05, 0.05, 0.05], [0.82, 0.74, 0.52]);
        }
        BuildingKind::PowerLoom => {
            // a wide frame strung with warp, a batten that beats the weft, a cloth beam and a belt pulley
            for (dx, dz) in [(-0.8f32, -0.45f32), (0.8, -0.45), (-0.8, 0.45), (0.8, 0.45)] {
                bx(out, [x + dx, 0.0, z + dz], [0.07, 1.2, 0.07], IRON_D, 0.0);
            }
            bx(out, [x, 1.2, z - 0.45], [0.85, 0.1, 0.07], IRON, 0.0);
            bx(out, [x, 1.2, z + 0.45], [0.85, 0.1, 0.07], IRON, 0.0);
            for k in 0..12 {
                bx(out, [x - 0.66 + k as f32 * 0.12, 0.25, z - 0.05], [0.008, 0.95, 0.008], [0.86, 0.84, 0.70], 0.0);
            }
            bx(out, [x, 0.25, z + 0.3], [0.66, 0.03, 0.2], CLOTH, 0.0);
            lb(out, [x - 0.78, 0.45, z + 0.45], [x + 0.78, 0.45, z + 0.45], 0.11, 0.11, CLOTH, 6);
            moving(out, "batten", Move::Piston, [x, 0.5, z], 0.3, 0.08, active, tick, |o| {
                bx(o, [x, 0.45, z + 0.08], [0.7, 0.5, 0.05], WOOD, 0.0);
            });
            moving(out, "pulley", Move::Spin(2), [x + 0.95, 0.9, z], 0.2, 0.0, active, tick, |o| {
                wheel(o, [x + 0.95, 0.9, z], 2, 0.3, IRON, 6);
            });
        }
        BuildingKind::Boiler => {
            // a great riveted drum on a brick firebox, a brass steam dome, a stack, a glowing fire door
            bx(out, [x, 0.0, z], [1.0, 0.55, 0.6], BRICK, 0.0);
            bx(out, [x - 0.2, 0.12, z + 0.6], [0.3, 0.36, 0.03], SOOT, 0.0);
            if active {
                bx(out, [x - 0.15, 0.2, z + 0.63], [0.22, 0.18 * flick(5.0), 0.02], FIRE, 1.0);
            }
            lb(out, [x - 1.0, 1.1, z], [x + 1.0, 1.1, z], 0.55, 0.55, [0.46, 0.50, 0.58], 12);
            for k in 0..4 {
                let bxp = x - 0.7 + k as f32 * 0.46;
                lb(out, [bxp, 1.1, z], [bxp + 0.05, 1.1, z], 0.575, 0.575, BRASS, 12);
            }
            rd(out, [x - 0.4, 1.78, z], [0.28, 0.3, 0.28], BRASS);
            bx(out, [x + 0.78, 0.9, z], [0.15, 1.9, 0.15], IRON, 0.0);
            bx(out, [x + 0.78, 2.8, z], [0.2, 0.07, 0.2], IRON_D, 0.0);
            lb(out, [x + 0.25, 1.62, z], [x + 0.25, 1.95, z], 0.03, 0.03, BRASS, 4);
            smoke(out, x + 0.78, 2.9, z, 1.3);
            if active {
                // a jet of white steam from the safety valve
                let age = (t * 0.03).fract();
                let r = 0.05 + 0.12 * age;
                shapes::sphere(out, [x + 0.25, 2.0 + age * 0.9, z], [r, r, r], [0.92, 0.94, 0.96], 6, 4, 0.2, 7);
            }
        }
        BuildingKind::SteamHammer => {
            // an anvil block between two tall columns, a cylinder on top, and the hammer head dropping on a glowing billet
            bx(out, [x, 0.0, z], [0.6, 0.45, 0.45], IRON_D, 0.0);
            bx(out, [x, 0.45, z], [0.36, 0.12, 0.26], STEEL, 0.0);
            for sx in [-0.72f32, 0.72] {
                bx(out, [x + sx, 0.0, z], [0.16, 2.4, 0.32], IRON, 0.0);
            }
            bx(out, [x, 2.2, z], [0.82, 0.2, 0.34], IRON_D, 0.0);
            bx(out, [x, 2.4, z], [0.3, 0.42, 0.26], IRON, 0.0);
            moving(out, "ram", Move::Piston, [x, 1.3, z], 0.14, 0.55, active, tick, |o| {
                lb(o, [x, 2.2, z], [x, 1.25, z], 0.1, 0.1, STEEL, 6);
                bx(o, [x, 0.7, z], [0.3, 0.55, 0.24], [0.60, 0.62, 0.68], 0.0);
            });
            if active {
                bx(out, [x, 0.57, z], [0.22, 0.06, 0.16], [1.0, 0.55, 0.2], 1.0);
            }
            lb(out, [x + 0.72, 1.4, z + 0.2], [x + 0.3, 2.5, z + 0.1], 0.04, 0.04, BRASS, 4);
            if active {
                let age = (t * 0.07).fract();
                shapes::sphere(out, [x + 0.4, 2.3 + age * 0.6, z + 0.12], [0.08 + 0.1 * age; 3], [0.92, 0.94, 0.96], 6, 4, 0.2, 9);
            }
        }
        BuildingKind::RollingLine => {
            // three roll stands in a row over a roller table, a hot bar running through, a flywheel at the end
            bx(out, [x, 0.0, z], [1.0, 0.3, 0.14], IRON, 0.0);
            for k in 0..3 {
                let sx = x + (k as f32 - 1.0) * 0.62;
                bx(out, [sx - 0.0, 0.0, z - 0.55], [0.18, 1.2, 0.09], IRON_D, 0.0);
                bx(out, [sx - 0.0, 0.0, z + 0.55], [0.18, 1.2, 0.09], IRON_D, 0.0);
                bx(out, [sx, 1.15, z], [0.2, 0.1, 0.6], IRON, 0.0);
                for (h, shift) in [(0.55f32, 0.0f32), (0.95, 0.5)] {
                    moving(out, &format!("roll_{k}_{}", (h * 10.0) as i32), Move::Spin(2), [sx, h, z], 0.14, 0.0, active, tick, |o| {
                        lb(o, [sx, h, z - 0.5], [sx, h, z + 0.5], 0.13, 0.13, STEEL, 8);
                        bx(o, [sx - 0.02 + shift * 0.0, h + 0.12, z - 0.2], [0.04, 0.04, 0.4], [0.92, 0.94, 0.98], 0.0);
                    });
                }
            }
            lb(out, [x - 1.1, 0.75, z], [x + 1.1, 0.75, z], 0.055, 0.055, if active { FIRE } else { [0.5, 0.52, 0.55] }, 6);
            if active {
                bx(out, [x - 1.0, 0.7, z], [0.5, 0.06, 0.06], [1.0, 0.5 + 0.2 * flick(6.0), 0.15], 1.0);
            }
            moving(out, "flywheel", Move::Spin(2), [x + 1.15, 0.9, z + 0.1], 0.1, 0.0, active, tick, |o| {
                wheel(o, [x + 1.15, 0.9, z + 0.1], 2, 0.34, IRON, 6);
            });
        }
        _ => {}
    }
}
