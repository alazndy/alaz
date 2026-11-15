use super::*;

/// Small living detail on the ground: grass tufts, wild flowers, ferns, mushrooms, reeds, pebbles. Purely visual and
/// derived from the seed, so nothing here is saved or simulated.
/// Stamp a model file standing in for one bit of ground cover (`models/grass.vox` ...). False when there is none.
fn file(out: &mut Mesh, name: &str, x: f32, z: f32, seed: u32) -> bool {
    if !assets::has_models() {
        return false;
    }
    let Some(m) = assets::model(name, seed) else { return false };
    let yaw = if m.opts.rotate { (seed >> 4) as f32 % 628.0 / 100.0 } else { 0.0 };
    m.place(out, &assets::Place::at(x, z).turned(yaw, 1.0));
    true
}

pub(crate) fn build_decor(out: &mut Mesh, world: &World, (x0, y0, x1, y1): (i32, i32, i32, i32)) {
    use shapes::{limb, rock, sphere};
    for ty in y0..y1 {
        for tx in x0..x1 {
            let Some(t) = world.map.terrain(tx, ty) else { continue };
            if t == Terrain::Water || alaz_sim::elevation_mm(world.seed, tx, ty) > 9_000 {
                continue;
            }
            let hs = alaz_sim::rng::hash2(world.seed ^ 0xDEC0, tx, ty);
            let roll = hs % 1000;
            let seed = (hs >> 10) as u32;
            let jit = |k: u32| ((seed.rotate_left(k) % 1000) as f32 / 1000.0 - 0.5) * 2.0;
            let (x, z) = (tx as f32 + 0.5 + 0.38 * jit(3), ty as f32 + 0.5 + 0.38 * jit(9));
            // dry or lush ground tints the grass
            let bio = noise(world, 0x51DE, tx as f32 * 0.035, ty as f32 * 0.035);
            let green = [0.30 + 0.12 * jit(5) - 0.10 * (bio < -0.2) as u8 as f32, 0.55 + 0.10 * jit(6) + 0.08 * (bio > 0.3) as u8 as f32, 0.20 + 0.06 * jit(7)];
            let (warm, wet) = alaz_sim::climate(world.seed, tx, ty);
            let (warm, wet) = (warm as f32 / 1000.0, wet as f32 / 1000.0);
            let cold = warm < 0.38;
            let steppe = warm > 0.56 && wet < 0.5;
            let jungle = warm > 0.56 && wet >= 0.5;
            let autumn = !cold && warm < 0.55 && warm > 0.32 && wet > 0.4;
            // the colour of the grass is the colour of the country
            let green = if cold {
                [0.40 + 0.05 * jit(5), 0.56, 0.46]
            } else if steppe {
                [0.80 + 0.06 * jit(5), 0.66 + 0.05 * jit(6), 0.28]
            } else if jungle {
                [0.10 + 0.04 * jit(5), 0.54 + 0.06 * jit(6), 0.16]
            } else if autumn {
                [0.76 + 0.06 * jit(5), 0.46 + 0.08 * jit(6), 0.16]
            } else {
                green
            };
            let dry = [0.62 + 0.08 * jit(5), 0.56 + 0.06 * jit(6), 0.28];
            let blade = |out: &mut Mesh, bx: f32, bz: f32, h: f32, lean: (f32, f32), col: [f32; 3]| {
                limb(out, [bx, 0.0, bz], [bx + lean.0, h * 1.25, bz + lean.1], 0.034, 0.006, col, 3);
            };
            match t {
                Terrain::Grass => {
                    let flowery = if wet > 0.55 && !cold { 330 } else if steppe { 40 } else if cold { 25 } else { 90 };
                    if roll < 380 {
                        if file(out, "grass", x, z, seed) {
                            continue;
                        }
                        out.set_anchor(x, z);
                        let col = if bio < -0.25 && !steppe && !jungle { dry } else { green };
                        for k in 0..4u32 {
                            let a = k as f32 * 1.7 + jit(2);
                            let tall = if steppe { 1.7 } else if jungle { 1.35 } else { 1.0 };
                            blade(out, x + a.cos() * 0.05, z + a.sin() * 0.05, (0.22 + 0.16 * ((k + seed) % 3) as f32 / 2.0) * tall, (a.cos() * 0.1, a.sin() * 0.1), col);
                        }
                    } else if roll < 380 + flowery {
                        // wild flowers in one of several colours, a few to a clump
                        const PETALS: [[f32; 3]; 6] = [[0.96, 0.84, 0.25], [0.97, 0.97, 0.93], [0.93, 0.50, 0.68], [0.66, 0.46, 0.88], [0.92, 0.28, 0.24], [0.40, 0.62, 0.95]];
                        let pc = if steppe {
                            [[0.96, 0.78, 0.18], [0.94, 0.52, 0.16], [0.88, 0.26, 0.2]][(seed % 3) as usize]
                        } else if cold {
                            [[0.96, 0.97, 0.99], [0.55, 0.7, 0.96], [0.72, 0.6, 0.92]][(seed % 3) as usize]
                        } else if jungle {
                            [[0.95, 0.2, 0.45], [0.98, 0.55, 0.12], [0.62, 0.3, 0.9], [0.98, 0.9, 0.2]][(seed % 4) as usize]
                        } else {
                            PETALS[(seed % 6) as usize]
                        };
                        if file(out, "flower", x, z, seed) {
                            continue;
                        }
                        out.set_anchor(x, z);
                        for k in 0..(2 + seed % 3) {
                            let (dx, dz) = (jit(11 + k) * 0.09, jit(17 + k) * 0.09);
                            let h = 0.22 + 0.1 * ((seed >> k) % 3) as f32;
                            blade(out, x + dx, z + dz, h, (0.01, 0.0), green);
                            sphere(out, [x + dx + 0.01, h + 0.02, z + dz], [0.075, 0.045, 0.075], pc, 6, 3, 0.0, 1);
                            sphere(out, [x + dx + 0.01, h + 0.04, z + dz], [0.03, 0.022, 0.03], [0.98, 0.82, 0.2], 4, 3, 0.0, 1);
                        }
                    }
                }
                Terrain::Forest => {
                    if roll < 90 {
                        // a fern: arching fronds from one point
                        if file(out, "fern", x, z, seed) {
                            continue;
                        }
                        out.set_anchor(x, z);
                        for k in 0..7u32 {
                            let a = k as f32 * 0.9 + jit(2);
                            limb(out, [x, 0.02, z], [x + a.cos() * 0.26, 0.2, z + a.sin() * 0.26], 0.014, 0.004, [0.16, 0.42 + 0.06 * jit(4), 0.2], 3);
                        }
                    } else if roll < 115 {
                        if file(out, "mushroom", x, z, seed) {
                            continue;
                        }
                        let cap = if seed % 3 == 0 { [0.86, 0.22, 0.18] } else { [0.62, 0.44, 0.28] };
                        out.set_anchor(x, z);
                        limb(out, [x, 0.0, z], [x, 0.09, z], 0.02, 0.016, [0.92, 0.9, 0.82], 4);
                        sphere(out, [x, 0.1, z], [0.06, 0.035, 0.06], cap, 7, 3, 0.0, 2);
                    }
                }
                Terrain::Sand => {
                    let near_water = world.map.water_within(tx, ty, 1);
                    if near_water && roll < 160 {
                        // reeds with brown heads, tall and thin
                        if file(out, "reeds", x, z, seed) {
                            continue;
                        }
                        out.set_anchor(x, z);
                        for k in 0..4u32 {
                            let (dx, dz) = (jit(21 + k) * 0.1, jit(27 + k) * 0.1);
                            let h = 0.7 + 0.35 * ((seed >> k) % 3) as f32 / 2.0;
                            blade(out, x + dx, z + dz, h, (0.05 * jit(31 + k), 0.04), [0.42, 0.55, 0.26]);
                            limb(out, [x + dx, h - 0.1, z + dz], [x + dx + 0.01, h + 0.06, z + dz], 0.022, 0.018, [0.42, 0.26, 0.14], 4);
                        }
                    } else if roll < 50 {
                        if file(out, "pebbles", x, z, seed) {
                            continue;
                        }
                        out.set_anchor(x, z);
                        rock(out, [x, 0.03, z], [0.07, 0.04, 0.06], [0.72, 0.68, 0.6], seed);
                        rock(out, [x + 0.09, 0.02, z + 0.04], [0.04, 0.025, 0.04], [0.66, 0.62, 0.56], seed ^ 3);
                    } else if roll < 80 {
                        if file(out, "dry_grass", x, z, seed) {
                            continue;
                        }
                        out.set_anchor(x, z);
                        for k in 0..3u32 {
                            let a = k as f32 * 2.1 + jit(2);
                            blade(out, x + a.cos() * 0.03, z + a.sin() * 0.03, 0.2, (a.cos() * 0.08, a.sin() * 0.08), [0.74, 0.66, 0.34]);
                        }
                    }
                }
                Terrain::Rock => {
                    if roll < 80 {
                        if file(out, "rubble", x, z, seed) {
                            continue;
                        }
                        out.set_anchor(x, z);
                        let g = 0.5 + 0.08 * jit(4);
                        rock(out, [x, 0.07, z], [0.14 + 0.06 * jit(5).abs(), 0.09, 0.12], [g, g, g + 0.02], seed);
                    } else if roll < 140 {
                        if file(out, "rubble", x, z, seed ^ 1) {
                            continue;
                        }
                        out.set_anchor(x, z);
                        for k in 0..3u32 {
                            let g = 0.46 + 0.1 * jit(k + 4);
                            rock(out, [x + jit(k + 8) * 0.12, 0.03, z + jit(k + 14) * 0.12], [0.05, 0.03, 0.05], [g, g, g + 0.02], seed ^ k);
                        }
                    }
                }
                Terrain::Water => {}
            }
        }
    }
}
