//! The inside of a cave: floor, rock walls cut into blocks, ore in the walls, stairs, rubble and a few stalagmites.
//! Built once per level (and again when a wall is dug or the water goes down) from the sim's grid, in metres with a
//! tile one metre square. Models are lit by the torch alone (see the cave branch of the shader): a face tagged
//! `WALL` in `uv.x` is rock that thins out in front of the player, as trees do.

use super::*;
use alaz_sim::caves::{Cell, Level, CAVE_H, CAVE_W, ORE_COAL, ORE_COPPER, ORE_IRON, ORE_TIN};
use super::shapes::{limb, rock, sphere};

/// Tiles on a side of one cached block of the cave mesh.
pub(crate) const BLOCK: i32 = 8;

/// A ball that shines by itself.
fn lamp(out: &mut Mesh, c: [f32; 3], r: [f32; 3], col: [f32; 3], seed: u32, glow: f32) {
    let start = out.len();
    sphere(out, c, r, col, 5, 3, 0.0, seed);
    for v in &mut out.verts[start..] {
        v.glow = glow;
        v.uv[1] = 0.0;
    }
}

/// `uv.x` of a face of a wall block: the shader lets the walls between the eye and the player open up.
pub(crate) const WALL: f32 = 2.0;
/// Height of the rock above the floor (metres).
const WALL_H: f32 = 2.3;

fn unit(seed: u64, x: i32, y: i32, salt: i32) -> f32 {
    (alaz_sim::rng::hash2(seed ^ (salt as u64).wrapping_mul(0x9E37_79B9), x, y) % 10_000) as f32 / 10_000.0
}

fn quad(out: &mut Mesh, p: [[f32; 3]; 4], col: [f32; 3], glow: f32, light: f32) {
    for i in [0, 1, 2, 0, 2, 3] {
        out.push(out.vl(p[i], col, glow, light));
    }
}

fn mul(c: [f32; 3], k: f32) -> [f32; 3] {
    [c[0] * k, c[1] * k, c[2] * k]
}

/// Rock colours of a level: warm brown near the surface, cooler and darker further down.
fn palette(level: u8) -> ([f32; 3], [f32; 3]) {
    match level {
        1 => ([0.45, 0.40, 0.35], [0.36, 0.32, 0.29]),
        2 => ([0.42, 0.40, 0.37], [0.34, 0.32, 0.31]),
        3 => ([0.38, 0.39, 0.41], [0.30, 0.31, 0.34]),
        _ => ([0.36, 0.35, 0.41], [0.28, 0.27, 0.33]),
    }
}

fn ore_color(code: u8) -> ([f32; 3], f32) {
    match code {
        ORE_IRON => ([0.60, 0.30, 0.20], 0.0),
        ORE_COPPER => ([0.25, 0.78, 0.60], 0.6),
        ORE_TIN => ([0.72, 0.80, 0.92], 0.6),
        ORE_COAL => ([0.07, 0.07, 0.09], 0.0),
        _ => ([0.5, 0.5, 0.5], 0.0),
    }
}

/// One exposed side of a wall block: a 2x2 faceted face from the floor up, its middle pushed in or out, in two
/// bands of rock. `(x, z)` is the tile corner, `dir` which side of the tile it is on (0 north (-z), 1 east, 2 south,
/// 3 west).
fn wall_face(out: &mut Mesh, seed: u64, tx: i32, ty: i32, dir: usize, top: f32, base: [f32; 3], low: [f32; 3]) {
    let (x, z) = (tx as f32, ty as f32);
    // the face's corners along its length (a, b) and the outward normal
    let (a, b, n): ([f32; 2], [f32; 2], [f32; 2]) = match dir {
        0 => ([x, z], [x + 1.0, z], [0.0, -1.0]),
        1 => ([x + 1.0, z], [x + 1.0, z + 1.0], [1.0, 0.0]),
        2 => ([x + 1.0, z + 1.0], [x, z + 1.0], [0.0, 1.0]),
        _ => ([x, z + 1.0], [x, z], [-1.0, 0.0]),
    };
    // the wall is on the far side of the face from the open ground: its surface is the face, bumps come out of it
    // toward the open ground, which is the opposite way of the wall block's own outward normal. The open ground is
    // the way `n` points.
    let bump = (unit(seed, tx, ty, 11 + dir as i32) - 0.35) * 0.30;
    let mid = |t: f32, h: f32| [a[0] + (b[0] - a[0]) * t, h, a[1] + (b[1] - a[1]) * t];
    let off = |p: [f32; 3], k: f32| [p[0] + n[0] * k, p[1], p[2] + n[1] * k];
    // vertices on a 3x3 grid (columns 0, 1/2, 1; rows 0, mid, top); only the middle one is pushed
    let rows = [0.0, top * 0.42, top];
    let cols = [0.0f32, 0.5, 1.0];
    let jitter = (unit(seed, tx, ty, 21 + dir as i32) - 0.5) * 0.4;
    let pt = |i: usize, j: usize| {
        let mut p = mid(cols[i], rows[j]);
        if i == 1 && j == 1 {
            p = off(p, bump);
        }
        // the band line wanders along the face a little
        if j == 1 && i != 1 {
            p[1] += jitter * 0.4;
        }
        p
    };
    for j in 0..2 {
        for i in 0..2 {
            let q = [pt(i, j), pt(i + 1, j), pt(i + 1, j + 1), pt(i, j + 1)];
            let shade = 0.88 + 0.22 * unit(seed, tx * 4 + i as i32, ty * 4 + j as i32 + dir as i32 * 97, 31);
            let col = if j == 0 { mul(low, shade) } else { mul(base, shade) };
            quad(out, q, col, 0.0, WALL);
        }
    }
}

/// A speck of ore: a rough lump that sits on a wall face.
fn nugget(out: &mut Mesh, c: [f32; 3], r: f32, code: u8, seed: u32) {
    let (col, glow) = ore_color(code);
    // (a glowing lump is drawn as a bright core with a lit crust round it)
    out.set_anchor(0.0, 0.0);
    rock(out, c, [r, r * 0.85, r], mul(col, 1.0), seed);
    if glow > 0.0 {
        lamp(out, c, [r * 0.55; 3], col, seed ^ 7, glow);
    }
}

/// One block (BLOCK x BLOCK tiles) of a level.
pub(crate) fn build_cave_block(level: &Level, seed: u64, node: u32, depth: u8, bx: i32, by: i32) -> Vec<Vertex> {
    let mut out = Mesh::new();
    let (base, low) = palette(depth);
    let seed = seed ^ (node as u64) << 20 ^ (depth as u64) << 52;
    let open = |x: i32, y: i32| level.cell(x, y).walkable();
    // how far a tile is from open ground (chebyshev, up to 4): rock deeper than that is not drawn
    let near_open = |x: i32, y: i32| (-3..=3).any(|dy| (-3..=3).any(|dx| open(x + dx, y + dy)));
    let top_h = |x: i32, y: i32| WALL_H + 0.22 * (unit(seed, x, y, 5) - 0.5) * 2.0;
    for ty in by * BLOCK..(by * BLOCK + BLOCK).min(CAVE_H) {
        for tx in bx * BLOCK..(bx * BLOCK + BLOCK).min(CAVE_W) {
            let cell = level.cell(tx, ty);
            let (x, z) = (tx as f32, ty as f32);
            match cell {
                Cell::Rock | Cell::Rubble => {
                    if cell == Cell::Rubble {
                        // fallen stone on the floor: a heap you can clear
                        quad(&mut out, [[x, 0.0, z], [x + 1.0, 0.0, z], [x + 1.0, 0.0, z + 1.0], [x, 0.0, z + 1.0]], mul(low, 0.9), 0.0, 1.0);
                        out.set_anchor(0.0, 0.0);
                        for k in 0..6 {
                            let f = |s: i32| unit(seed, tx, ty, 40 + k * 3 + s);
                            let r = 0.22 + 0.2 * f(0);
                            let c = [x + 0.2 + 0.6 * f(1), 0.14 + 0.45 * f(2) * (1.0 - (f(1) - 0.5).abs()), z + 0.2 + 0.6 * f(0) * 0.5 + 0.3 * f(2)];
                            rock(&mut out, c, [r, r * 0.8, r], mul(base, 0.75 + 0.5 * f(2)), (tx * 31 + ty * 17 + k) as u32);
                        }
                        continue;
                    }
                    if !near_open(tx, ty) {
                        continue;
                    }
                    let h = top_h(tx, ty);
                    let tcol = mul(base, 0.88 + 0.2 * unit(seed, tx, ty, 6));
                    quad(&mut out, [[x, h, z], [x + 1.0, h, z], [x + 1.0, h, z + 1.0], [x, h, z + 1.0]], tcol, 0.0, WALL);
                    // sides on the open ground
                    for (dir, (dx, dy)) in [(0, -1), (1, 0), (0, 1), (-1, 0)].into_iter().enumerate() {
                        let (nx, ny) = (tx + dx, ty + dy);
                        if open(nx, ny) {
                            wall_face(&mut out, seed, tx, ty, dir, h, base, low);
                            if let Some((code, left)) = level.ore_at(tx, ty) {
                                // ore in the wall: lumps, fewer as the vein is worked
                                let (a, b) = match dir {
                                    0 => ([x, z], [x + 1.0, z]),
                                    1 => ([x + 1.0, z], [x + 1.0, z + 1.0]),
                                    2 => ([x + 1.0, z + 1.0], [x, z + 1.0]),
                                    _ => ([x, z + 1.0], [x, z]),
                                };
                                let n = [dx as f32, dy as f32];
                                for k in 0..(2 + left as i32).min(6) {
                                    let t = 0.15 + 0.7 * unit(seed, tx, ty, 70 + k * 5 + dir as i32);
                                    let yy = 0.35 + 1.15 * unit(seed, tx, ty, 71 + k * 5 + dir as i32);
                                    let r = 0.10 + 0.09 * unit(seed, tx, ty, 72 + k * 5 + dir as i32);
                                    let c = [a[0] + (b[0] - a[0]) * t + n[0] * 0.04, yy, a[1] + (b[1] - a[1]) * t + n[1] * 0.04];
                                    nugget(&mut out, c, r, code, (tx * 13 + ty * 7 + k) as u32);
                                }
                            } else if unit(seed, tx, ty, 90 + dir as i32) < 0.045 {
                                // a few pale crystals growing out of the bare rock
                                let (a, b) = match dir {
                                    0 => ([x, z], [x + 1.0, z]),
                                    1 => ([x + 1.0, z], [x + 1.0, z + 1.0]),
                                    2 => ([x + 1.0, z + 1.0], [x, z + 1.0]),
                                    _ => ([x, z + 1.0], [x, z]),
                                };
                                let t = 0.25 + 0.5 * unit(seed, tx, ty, 95 + dir as i32);
                                let base_p = [a[0] + (b[0] - a[0]) * t, 0.5 + unit(seed, tx, ty, 96) * 0.8, a[1] + (b[1] - a[1]) * t];
                                for k in 0..3 {
                                    let lean = 0.10 + 0.09 * k as f32;
                                    let tip = [base_p[0] + dx as f32 * (0.2 + lean), base_p[1] + 0.12 * (k as f32 - 1.0), base_p[2] + dy as f32 * (0.2 + lean)];
                                    limb(&mut out, [base_p[0] + (k as f32 - 1.0) * 0.05 * dy as f32, base_p[1], base_p[2] + (k as f32 - 1.0) * 0.05 * dx as f32], tip, 0.05, 0.008, [0.55, 0.78, 1.0], 4);
                                }
                                // (limbs are lit; the tip glows)
                                lamp(&mut out, base_p, [0.045; 3], [0.7, 0.9, 1.0], 3, 0.6);
                            }
                        } else if !near_open(nx, ny) {
                            // the side towards rock that is not drawn: nothing to see
                        } else if matches!(level.cell(nx, ny), Cell::Rock) {
                            // a step in the rock tops: close the gap on the lower side
                            let hn = top_h(nx, ny);
                            if hn < h - 0.01 {
                                let (a, b) = match dir {
                                    0 => ([x, z], [x + 1.0, z]),
                                    1 => ([x + 1.0, z], [x + 1.0, z + 1.0]),
                                    2 => ([x + 1.0, z + 1.0], [x, z + 1.0]),
                                    _ => ([x, z + 1.0], [x, z]),
                                };
                                quad(&mut out, [[a[0], hn, a[1]], [b[0], hn, b[1]], [b[0], h, b[1]], [a[0], h, a[1]]], mul(low, 1.05), 0.0, WALL);
                            }
                        }
                    }
                }
                Cell::Floor | Cell::Shallow | Cell::Deep | Cell::Down | Cell::Up | Cell::Exit => {
                    let tint = 0.86 + 0.24 * unit(seed, tx, ty, 2);
                    let fl = mul(base, 0.95 * tint);
                    let (y, col) = match cell {
                        Cell::Shallow => (-0.04, [0.13, 0.26, 0.32]),
                        Cell::Deep => (-0.12, [0.06, 0.15, 0.27]),
                        _ => (0.0, fl),
                    };
                    quad(&mut out, [[x, y, z], [x + 1.0, y, z], [x + 1.0, y, z + 1.0], [x, y, z + 1.0]], col, 0.0, 1.0);
                    if cell == Cell::Shallow || cell == Cell::Deep {
                        // pale ripples on the water
                        for k in 0..2 {
                            if unit(seed, tx, ty, 120 + k) < 0.5 {
                                let (rx, rz) = (x + 0.15 + 0.7 * unit(seed, tx, ty, 122 + k), z + 0.15 + 0.7 * unit(seed, tx, ty, 124 + k));
                                quad(&mut out, [[rx, y + 0.006, rz], [rx + 0.34, y + 0.006, rz], [rx + 0.34, y + 0.006, rz + 0.05], [rx, y + 0.006, rz + 0.05]], [0.35, 0.48, 0.56], 0.0, 1.0);
                            }
                        }
                    }
                    match cell {
                        Cell::Down => {
                            // a stair spiralling down into the dark: a rim and three steps getting darker
                            let (cx, cz) = (x + 0.5, z + 0.5);
                            push_box(&mut out, [cx, 0.0, cz], [0.5, 0.05, 0.5], [0.55, 0.52, 0.47], 0.0);
                            for (k, (half, shade)) in [(0.38f32, 0.5f32), (0.27, 0.3), (0.16, 0.12)].into_iter().enumerate() {
                                let top = -0.10 - 0.12 * k as f32;
                                let ang = k as f32 * 0.5;
                                let (dx, dz) = (ang.dsin() * 0.05, ang.dcos() * 0.05);
                                push_box(&mut out, [cx + dx, top - 0.3, cz + dz], [half, 0.3, half], [0.5 * shade * 2.0, 0.48 * shade * 2.0, 0.46 * shade * 2.0], 0.0);
                            }
                        }
                        Cell::Up | Cell::Exit => {
                            let (cx, cz) = (x + 0.5, z + 0.5);
                            let exit = cell == Cell::Exit;
                            for (k, half) in [0.46f32, 0.34, 0.22].into_iter().enumerate() {
                                let top = 0.10 + 0.12 * k as f32;
                                let c = if exit && k == 2 { [0.92, 0.96, 1.0] } else { [0.62, 0.60, 0.56] };
                                push_box(&mut out, [cx, 0.0, cz], [half, top, half], c, if exit && k == 2 { 0.85 } else { 0.0 });
                            }
                        }
                        _ => {}
                    }
                    // fixtures that do not get in the way: a stalagmite or a few pebbles, by the wall only
                    let by_wall = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().find(|&&(dx, dy)| matches!(level.cell(tx + dx, ty + dy), Cell::Rock));
                    if cell == Cell::Floor {
                        if let Some(&(dx, dy)) = by_wall {
                            let r = unit(seed, tx, ty, 3);
                            let (px, pz) = (x + 0.5 + dx as f32 * 0.36, z + 0.5 + dy as f32 * 0.36);
                            if r < 0.08 {
                                let h = 0.5 + 0.9 * unit(seed, tx, ty, 4);
                                limb(&mut out, [px, 0.0, pz], [px, h, pz], 0.13, 0.015, mul(base, 1.05), 5);
                                limb(&mut out, [px + 0.18, 0.0, pz + 0.08], [px + 0.18, h * 0.5, pz + 0.08], 0.08, 0.012, mul(base, 0.95), 5);
                            } else if r < 0.22 {
                                for k in 0..3 {
                                    let f = |s: i32| unit(seed, tx, ty, 50 + k * 4 + s);
                                    rock(&mut out, [px + (f(0) - 0.5) * 0.4, 0.05, pz + (f(1) - 0.5) * 0.4], [0.07 + 0.06 * f(2), 0.05, 0.07 + 0.06 * f(2)], mul(base, 0.8 + 0.4 * f(3)), (tx * 7 + ty + k) as u32);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    for v in &mut out.verts {
        // models stand at height 0 here (the height window is flat in a cave)
        v.off = [0.0, 0.0];
    }
    out.verts
}

/// A burning torch held at (x, z): a stick and a flame that flickers and shrinks as it burns down. `frac` is the
/// share of the light left (0..1; below 0.2 the flame is low).
pub(crate) fn push_torch(out: &mut Mesh, x: f32, z: f32, secs: f32, frac: f32) {
    out.set_anchor(x, z);
    limb(out, [x, 1.0, z], [x, 1.42, z], 0.03, 0.026, [0.30, 0.20, 0.12], 5);
    if frac <= 0.0 {
        // burnt out: a dark head
        sphere(out, [x, 1.44, z], [0.04; 3], [0.1, 0.08, 0.07], 5, 3, 0.0, 1);
        return;
    }
    let low = (frac / 0.2).min(1.0);
    let flick = 1.0 + 0.12 * (secs * 17.0).dsin() + 0.08 * (secs * 29.0).dsin();
    let r = (0.05 + 0.04 * low) * flick;
    lamp(out, [x, 1.50 + 0.03 * (secs * 11.0).dsin(), z], [r, r * 1.7, r], [1.0, 0.62, 0.20], 2, 1.0);
    lamp(out, [x, 1.48, z], [r * 0.55, r, r * 0.55], [1.0, 0.92, 0.55], 3, 1.0);
}
