use super::DetMath;
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TreeDetail {
    Full,
    Far,
}

pub(crate) fn build_nodes(out: &mut Mesh, world: &World, (x0, y0, x1, y1): (i32, i32, i32, i32), level: u8, tree_detail: TreeDetail) {
    for n in world.nodes_in(x0, y0, x1, y1) {
        push_node(out, world, n, level, tree_detail);
    }
}

/// The node under the pointer again, puffed out a little and in one bright colour, sitting just behind the real
/// one: what shows around the real model is a glowing rim.
pub(crate) fn push_hover_rim(out: &mut Mesh, world: &World, tile: (i32, i32), level: u8, cam: &Camera, t: f32, tree_detail: TreeDetail) {
    let Some(id) = world.node_at(tile.0, tile.1) else { return };
    let Some(n) = world.nodes.get(id.0 as usize) else { return };
    let mut verts = Mesh::new();
    push_node(&mut verts, world, n, level, tree_detail);
    out.set_anchor(0.0, 0.0);
    out.foliage = false;
    if verts.is_empty() {
        return;
    }
    let inv = 1.0 / verts.len() as f32;
    let c = verts.iter().fold([0.0f32; 3], |a, v| [a[0] + v.pos[0] * inv, a[1] + v.pos[1] * inv, a[2] + v.pos[2] * inv]);
    let (p, y) = (cam.pitch_deg.to_radians(), cam.yaw_deg.to_radians());
    let fwd = [-(p.dcos() * y.dsin()), -p.dsin(), -(p.dcos() * y.dcos())];
    let pulse = 0.5 + 0.5 * (t * 4.0).dsin();
    let col = [1.0, 0.86 + 0.1 * pulse, 0.42 + 0.3 * pulse];
    for mut v in verts.verts {
        let d = [v.pos[0] - c[0], (v.pos[1] - c[1]) * 0.8, v.pos[2] - c[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-3);
        let grow = 0.09;
        for k in 0..3 {
            v.pos[k] += d[k] / len * grow + fwd[k] * 0.25;
        }
        v.color = col;
        v.glow = 2.0;
        out.push(v);
    }
}

/// Where a node stands (its model's anchor), and how wide and tall it is: used to tell what hides the player.
pub(crate) fn node_shape(world: &World, n: &alaz_sim::world::Node) -> ([f32; 2], f32, f32) {
    if n.kind == NodeKind::CaveMouth
        && let Some(mouth) = world.map.natural_cave_mouth(n.tx, n.ty)
    {
        return ([mouth.threshold.x as f32 + 0.5, mouth.threshold.y as f32 + 0.5], 1.2, 2.2);
    }
    let seed = alaz_sim::rng::hash2(world.seed ^ 0x51, n.tx, n.ty) as u32;
    let jit = |k: u32| ((seed.rotate_left(k) % 1000) as f32 / 1000.0 - 0.5) * 2.0;
    let at = [n.tx as f32 + 0.5 + 0.18 * jit(3), n.ty as f32 + 0.5 + 0.18 * jit(7)];
    let (r, h) = match n.kind {
        NodeKind::Tree => {
            let h = alaz_sim::map::tree_height_mm(world.seed, n.tx, n.ty) as f32 / 1000.0;
            (h * 0.2 + 0.3, h)
        }
        NodeKind::CaveMouth => (1.4, 2.2),
        _ => (0.6, 0.9),
    };
    (at, r, h)
}

/// Where a node's model stands and how wide and tall it is, for pointer picking.
pub fn node_bounds(world: &World, n: &alaz_sim::world::Node) -> ([f32; 2], f32, f32) {
    node_shape(world, n)
}

pub(crate) fn push_node(out: &mut Mesh, world: &World, n: &alaz_sim::world::Node, level: u8, tree_detail: TreeDetail) {
    use shapes::{limb, rock, sphere};
    {
        let seed = alaz_sim::rng::hash2(world.seed ^ 0x51, n.tx, n.ty) as u32;
        let jit = |k: u32| ((seed.rotate_left(k) % 1000) as f32 / 1000.0 - 0.5) * 2.0;
        // things you have never handled look like the ordinary thing they resemble
        let known = world.has_seen(n.kind.item());
        let (x, z) = (n.tx as f32 + 0.5 + 0.18 * jit(3), n.ty as f32 + 0.5 + 0.18 * jit(7));
        if n.kind == NodeKind::CaveMouth {
            return;
        }
        let far_tree = n.kind == NodeKind::Tree && tree_detail == TreeDetail::Far;
        if !far_tree && assets::has_models() && push_node_file(out, world, n, (x, z), seed, known) {
            return;
        }
        out.set_anchor(x, z);
        let stone = |out: &mut Mesh, x: f32, z: f32| {
            let g = 0.58 + 0.06 * jit(11);
            rock(out, [x, 0.2, z], [0.42 + 0.06 * jit(5), 0.26, 0.36], [g, g, g + 0.02], seed);
            rock(out, [x + 0.3, 0.1, z + 0.15], [0.2, 0.14, 0.18], [g * 0.9, g * 0.9, g * 0.92], seed ^ 77);
        };
        let tuft = |out: &mut Mesh, x: f32, z: f32, col: [f32; 3]| {
            for k in 0..5 {
                let a = k as f32 * 1.3 + jit(2);
                let (dx, dz) = (a.dcos() * 0.14, a.dsin() * 0.14);
                limb(out, [x + dx * 0.4, 0.0, z + dz * 0.4], [x + dx * 1.4, 0.34 + 0.08 * (k % 3) as f32, z + dz * 1.4], 0.025, 0.004, col, 4);
            }
        };
        match n.kind {
            NodeKind::StoneOutcrop => stone(out, x, z),
            NodeKind::IronOutcrop | NodeKind::CopperOutcrop | NodeKind::TinOutcrop => {
                stone(out, x, z);
                if known {
                    let vein = match n.kind {
                        NodeKind::IronOutcrop => [0.66, 0.34, 0.16],
                        NodeKind::CopperOutcrop => [0.22, 0.66, 0.52],
                        _ => [0.82, 0.84, 0.90],
                    };
                    sphere(out, [x + 0.22, 0.34, z - 0.08], [0.1, 0.07, 0.09], vein, 6, 4, 0.1, seed);
                    sphere(out, [x - 0.18, 0.28, z + 0.12], [0.08, 0.06, 0.08], vein, 6, 4, 0.1, seed ^ 5);
                    sphere(out, [x + 0.02, 0.44, z + 0.16], [0.07, 0.05, 0.07], vein, 6, 4, 0.1, seed ^ 9);
                } else if level >= 3 {
                    // not handled yet, but a sharper eye notices something odd in the rock
                    sphere(out, [x + 0.2, 0.34, z - 0.06], [0.07, 0.05, 0.06], [0.72, 0.66, 0.58], 6, 4, 0.1, seed ^ 13);
                }
            }
            NodeKind::Tree => {
                // saplings, grown trees and a few giants; forests lean to pines. A person stands about 1.8 m tall.
                let segments = |full, far| if far_tree { far } else { full };
                let height = alaz_sim::map::tree_height_mm(world.seed, n.tx, n.ty) as f32 / 1000.0;
                let (warm, wet) = alaz_sim::climate(world.seed, n.tx, n.ty);
                let (warm, wet) = (warm as f32 / 1000.0, wet as f32 / 1000.0);
                let cold = warm < 0.38;
                let steppe = warm > 0.56 && wet < 0.5;
                let jungle = warm > 0.56 && wet >= 0.5;
                let autumn = !cold && warm < 0.55 && warm > 0.32 && wet > 0.4;
                let desert = warm > 0.66 && wet < 0.34;
                let swamp = wet > 0.7 && warm > 0.4 && warm < 0.6;
                let terrain = world.map.terrain(n.tx, n.ty);
                let pine = cold && (seed >> 4) % 100 < 85
                    || terrain == Some(Terrain::Forest) && !steppe && !jungle && (seed >> 4) % 100 < 45
                    || terrain == Some(Terrain::Grass) && (seed >> 4) % 100 < 8;
                let trunk_r = 0.05 + height * 0.022;
                let bark = [0.34 + 0.04 * jit(4), 0.24, 0.15];
                if desert {
                    // a cactus: a fat ribbed column with an arm or two turned up
                    let h = (height * 0.55).clamp(1.2, 3.4);
                    let col = [0.34 + 0.04 * jit(6), 0.52 + 0.05 * jit(8), 0.28];
                    let r = 0.16 + 0.03 * jit(3);
                    limb(out, [x, 0.0, z], [x, h, z], r, r * 0.9, col, segments(8, 5));
                    sphere(out, [x, h, z], [r * 0.9, r * 0.6, r * 0.9], col, segments(8, 5), segments(4, 3), 0.0, 1);
                    let arms = 1 + (seed >> 9) % 2;
                    for k in 0..arms {
                        let a = k as f32 * std::f32::consts::PI + jit(5);
                        let (ax, az) = (a.dcos(), a.dsin());
                        let y0 = h * (0.4 + 0.15 * k as f32);
                        let out_p = [x + ax * 0.42, y0 + 0.05, z + az * 0.42];
                        limb(out, [x, y0, z], out_p, r * 0.6, r * 0.6, col, segments(6, 4));
                        let top = [out_p[0], y0 + h * 0.35, out_p[2]];
                        limb(out, out_p, top, r * 0.62, r * 0.55, col, segments(6, 4));
                        sphere(out, top, [r * 0.55, r * 0.4, r * 0.55], col, segments(6, 4), segments(3, 2), 0.0, 2);
                    }
                    // a red flower on top of some
                    if seed.is_multiple_of(3) {
                        sphere(out, [x, h + r * 0.55, z], [0.07, 0.05, 0.07], [0.86, 0.24, 0.3], 5, 3, 0.0, 3);
                    }
                } else if swamp && !pine {
                    // a willow: a leaning trunk and a crown that hangs down in long strands
                    let lean = 0.2 * jit(4) * height;
                    let top = [x + lean, height * 0.62, z];
                    limb(out, [x, 0.0, z], top, trunk_r * 1.2, trunk_r * 0.7, [0.30, 0.25, 0.17], segments(7, 5));
                    let cr = height * 0.2 + 0.4;
                    let g = [0.42 + 0.04 * jit(6), 0.52 + 0.05 * jit(8), 0.22];
                    out.foliage = true;
                    sphere(out, [top[0], top[1] + cr * 0.3, top[2]], [cr, cr * 0.55, cr], g, segments(10, 6), segments(5, 4), 0.12, seed);
                    for k in (0..14u32).step_by(segments(1, 3)) {
                        let a = k as f32 * 0.449 + jit(k + 7) * 0.3;
                        let (ax, az) = (a.dcos() * cr * 0.9, a.dsin() * cr * 0.9);
                        let len = height * (0.32 + 0.1 * jit(k + 11).abs());
                        limb(out, [top[0] + ax, top[1] + cr * 0.2, top[2] + az], [top[0] + ax * 1.08, (top[1] - len).max(height * 0.3), top[2] + az * 1.08], 0.09, 0.04, [g[0] * 0.92, g[1] * 0.95, g[2]], segments(5, 4));
                    }
                    out.foliage = false;
                } else if pine {
                    let g = if cold { [0.12 + 0.03 * jit(6), 0.36 + 0.05 * jit(8), 0.30] } else { [0.15 + 0.04 * jit(6), 0.42 + 0.06 * jit(8), 0.22 + 0.04 * jit(10)] };
                    // the trunk runs up into the crown and shows bare below the lowest tier
                    limb(out, [x, 0.0, z], [x, height * 0.85, z], trunk_r, trunk_r * 0.35, bark, segments(7, 5));
                    // tiers of needles, widest at the bottom
                    out.foliage = true;
                    for tier in 0..4u32 {
                        let f = tier as f32 / 4.0;
                        let (y0, y1) = (height * (0.32 + 0.15 * f), height * (0.5 + 0.15 * f));
                        let rad = height * (0.19 - 0.035 * tier as f32);
                        let shade = 1.0 + 0.05 * tier as f32;
                        limb(out, [x, y0, z], [x, y1, z], rad, 0.02, [g[0] * shade, g[1] * shade, g[2] * shade], segments(8, 5));
                    }
                    limb(out, [x, height * 0.82, z], [x, height, z], height * 0.1, 0.01, [g[0] * 1.2, g[1] * 1.2, g[2] * 1.1], segments(7, 5));
                    out.foliage = false;
                } else if steppe {
                    // an acacia: a thin leaning trunk and one flat umbrella of leaves
                    let lean = 0.25 * jit(4) * height;
                    limb(out, [x, 0.0, z], [x + lean, height * 0.62, z], trunk_r * 0.8, trunk_r * 0.5, [0.38, 0.3, 0.2], segments(6, 5));
                    let cr = height * 0.17 + 0.25;
                    let top = [0.58 + 0.06 * jit(6), 0.62 + 0.05 * jit(8), 0.24];
                    out.foliage = true;
                    sphere(out, [x + lean, height * 0.68, z], [cr * 1.3, cr * 0.3, cr * 1.3], top, segments(10, 6), segments(5, 3), 0.06, seed);
                    sphere(out, [x + lean + cr * 0.5, height * 0.62, z + cr * 0.3], [cr * 0.9, cr * 0.2, cr * 0.9], [top[0] * 0.9, top[1] * 0.92, top[2]], segments(8, 5), segments(4, 3), 0.06, seed ^ 3);
                    out.foliage = false;
                } else {
                    // leaves: green, golden-red in autumn country, deep and vivid in the jungle
                    let g = if autumn {
                        [[0.84, 0.38, 0.10], [0.88, 0.62, 0.12], [0.68, 0.20, 0.12], [0.62, 0.58, 0.14]][((seed >> 6) % 4) as usize]
                    } else if jungle {
                        [0.06 + 0.04 * jit(6), 0.46 + 0.08 * jit(8), 0.13]
                    } else if wet > 0.62 {
                        [0.12 + 0.04 * jit(6), 0.52 + 0.06 * jit(8), 0.18]
                    } else {
                        [0.18 + 0.05 * jit(6), 0.44 + 0.07 * jit(8), 0.17]
                    };
                    let birch = !jungle && !autumn && (seed >> 5) % 100 < 22;
                    let height = if jungle { height * 1.25 } else { height };
                    let trunk_top = height * 0.5;
                    let bark = if birch { [0.88, 0.86, 0.8] } else { [0.34 + 0.04 * jit(4), 0.24, 0.15] };
                    limb(out, [x, 0.0, z], [x + 0.03 * jit(4) * height, trunk_top, z], trunk_r, trunk_r * 0.6, bark, segments(7, 5));
                    let cr = (height * 0.16 + 0.26) * if jungle { 1.3 } else { 1.0 };
                    // a few branches reaching out under the crown
                    for k in (0..3u32).step_by(segments(1, 2)) {
                        let a = k as f32 * 2.1 + jit(2);
                        limb(out, [x, trunk_top * 0.8, z], [x + a.dcos() * cr * 0.8, trunk_top + cr * 0.3, z + a.dsin() * cr * 0.8], trunk_r * 0.45, trunk_r * 0.2, bark, segments(5, 4));
                    }
                    let g = if birch { [g[0] * 1.2, g[1] * 1.15, g[2] * 1.1] } else { g };
                    // the crown is a loose heap of leaf clusters, not one ball
                    out.foliage = true;
                    let lobes = 7 + (seed % 3);
                    for k in (0..lobes).step_by(segments(1, 2)) {
                        let a = k as f32 * 2.4 + jit(20 + k % 5) * 0.7;
                        let ring = if k == 0 { 0.0 } else { 0.55 + 0.25 * jit(30 + k % 7).abs() };
                        let up = if k == 0 { 1.15 } else { 0.55 + 0.7 * ((k * 5) % 7) as f32 / 7.0 };
                        let lr = cr * (0.5 + 0.2 * jit(40 + k % 6).abs());
                        let shade = 0.9 + 0.15 * ((k * 3) % 5) as f32 / 4.0;
                        sphere(out, [x + a.dcos() * cr * ring, trunk_top + cr * up, z + a.dsin() * cr * ring], [lr, lr * 0.8, lr], [g[0] * shade, g[1] * shade, g[2]], segments(7, 6), segments(4, 3), 0.18, seed ^ (k * 7 + 3));
                    }
                    out.foliage = false;
                }
            }
            NodeKind::BerryBush => {
                let g = [0.2, 0.44, 0.18];
                sphere(out, [x, 0.3, z], [0.42, 0.3, 0.4], g, 8, 5, 0.08, seed);
                sphere(out, [x + 0.22, 0.24, z + 0.12], [0.3, 0.22, 0.3], [g[0] * 1.1, g[1] * 1.05, g[2]], 7, 4, 0.08, seed ^ 2);
                for k in 0..5 {
                    let a = k as f32 * 1.25 + jit(9);
                    sphere(out, [x + a.dcos() * 0.3, 0.32 + 0.1 * (k % 2) as f32, z + a.dsin() * 0.3], [0.05; 3], [0.78, 0.1, 0.2], 5, 4, 0.0, 1);
                }
            }
            NodeKind::ClayDeposit => {
                let c = if known { [0.62, 0.40, 0.28] } else { [0.78, 0.70, 0.52] };
                sphere(out, [x, 0.0, z], [0.46, 0.14, 0.4], c, 9, 4, 0.08, seed);
                sphere(out, [x + 0.14, 0.05, z - 0.08], [0.26, 0.1, 0.24], [c[0] * 1.08, c[1] * 1.08, c[2] * 1.06], 7, 4, 0.08, seed ^ 6);
            }
            NodeKind::HerbPatch => {
                tuft(out, x, z, [0.30, 0.52, 0.24]);
                if known {
                    sphere(out, [x - 0.1, 0.4, z + 0.06], [0.05; 3], [0.88, 0.5, 0.7], 5, 4, 0.0, 1);
                    sphere(out, [x + 0.12, 0.36, z - 0.1], [0.05; 3], [0.96, 0.86, 0.4], 5, 4, 0.0, 1);
                    sphere(out, [x + 0.02, 0.44, z + 0.14], [0.045; 3], [0.88, 0.5, 0.7], 5, 4, 0.0, 1);
                } else if level >= 2 {
                    // a pale speck among the green: this one is not just grass
                    sphere(out, [x - 0.06, 0.38, z + 0.04], [0.04; 3], [0.82, 0.78, 0.7], 5, 4, 0.0, 1);
                }
            }
            NodeKind::CaveMouth => {}
            NodeKind::FiberPlant => {
                // a tall clump of pale straw-gold stalks with seed heads: it stands out of the green grass
                let c = if known { [0.86, 0.78, 0.42] } else { [0.74, 0.70, 0.40] };
                let head = [0.62, 0.48, 0.26];
                for k in 0..14u32 {
                    let a = k as f32 * 0.449 + jit(5 + k % 4) * 0.4;
                    let lean = 0.18 + 0.14 * jit(k + 2).abs();
                    let hgt = 1.0 + 0.3 * jit(k + 1);
                    let top = [x + a.dcos() * lean, hgt, z + a.dsin() * lean];
                    limb(out, [x + a.dcos() * 0.05, 0.0, z + a.dsin() * 0.05], top, 0.03, 0.008, c, 4);
                    if k % 2 == 0 {
                        sphere(out, top, [0.035, 0.09, 0.035], head, 5, 3, 0.0, k);
                    }
                }
            }
            NodeKind::Deadwood => {
                // a heap of fallen branches, grey-silver with age, a few sticking up so it shows over the grass
                let c = if known { [0.62, 0.56, 0.48] } else { [0.56, 0.52, 0.46] };
                let dark = [c[0] * 0.75, c[1] * 0.72, c[2] * 0.7];
                limb(out, [x - 0.6, 0.08, z - 0.1], [x + 0.6, 0.1, z + 0.1], 0.07, 0.05, c, 6);
                limb(out, [x - 0.3, 0.1, z + 0.4], [x + 0.35, 0.12, z - 0.4], 0.06, 0.04, dark, 6);
                limb(out, [x + 0.2, 0.1, z + 0.3], [x - 0.45, 0.14, z - 0.25], 0.05, 0.035, c, 5);
                for k in 0..3u32 {
                    let a = k as f32 * 2.1 + jit(k + 3);
                    limb(out, [x + a.dcos() * 0.2, 0.08, z + a.dsin() * 0.2], [x + a.dcos() * 0.45, 0.55 + 0.15 * jit(k + 6), z + a.dsin() * 0.45], 0.035, 0.012, dark, 4);
                }
            }
        }
    }
}

/// The species name a tree gets in file names (`tree_pine.vox`), by the same rules the code-made trees use.
pub(crate) fn tree_species_name(world: &World, n: &alaz_sim::world::Node, seed: u32) -> &'static str {
    let (warm, wet) = alaz_sim::climate(world.seed, n.tx, n.ty);
    let (warm, wet) = (warm as f32 / 1000.0, wet as f32 / 1000.0);
    let cold = warm < 0.38;
    let steppe = warm > 0.56 && wet < 0.5;
    let jungle = warm > 0.56 && wet >= 0.5;
    let autumn = !cold && warm < 0.55 && warm > 0.32 && wet > 0.4;
    let desert = warm > 0.66 && wet < 0.34;
    let swamp = wet > 0.7 && warm > 0.4 && warm < 0.6;
    let terrain = world.map.terrain(n.tx, n.ty);
    let pine = cold && (seed >> 4) % 100 < 85
        || terrain == Some(Terrain::Forest) && !steppe && !jungle && (seed >> 4) % 100 < 45
        || terrain == Some(Terrain::Grass) && (seed >> 4) % 100 < 8;
    if desert {
        "cactus"
    } else if swamp && !pine {
        "willow"
    } else if pine {
        "pine"
    } else if steppe {
        "acacia"
    } else if jungle {
        "jungle"
    } else if autumn {
        "autumn"
    } else if (seed >> 5) % 100 < 22 {
        "birch"
    } else {
        "oak"
    }
}

/// A model file for this node (`models/tree_pine.vox`, then `models/tree`; `iron_outcrop_unknown` while its ore is
/// still unrecognised), standing where the code-made one would. Returns false when there is none.
fn push_node_file(out: &mut Mesh, world: &World, n: &alaz_sim::world::Node, (x, z): (f32, f32), seed: u32, known: bool) -> bool {
    let base = assets::node_name(n.kind);
    let model = if n.kind == NodeKind::Tree {
        assets::model(&format!("tree_{}", tree_species_name(world, n, seed)), seed).or_else(|| assets::model(base, seed))
    } else if !known {
        assets::model(&format!("{base}_unknown"), seed).or_else(|| assets::model(base, seed))
    } else {
        assets::model(base, seed)
    };
    let Some(m) = model else { return false };
    let mut scale = 1.0;
    if n.kind == NodeKind::Tree && m.opts.fit_height.unwrap_or(true) && m.height > 0.05 {
        let h = alaz_sim::map::tree_height_mm(world.seed, n.tx, n.ty) as f32 / 1000.0;
        scale = (h / m.height).clamp(0.25, 4.0);
    }
    let yaw = if m.opts.rotate { (seed >> 8) as f32 % 628.0 / 100.0 } else { 0.0 };
    m.place(out, &assets::Place::at(x, z).turned(yaw, scale));
    true
}
