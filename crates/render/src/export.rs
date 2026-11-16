//! `alaz --export-assets <folder>`: writes everything the game draws in code out as files an artist can open and
//! change: every building and scenery model as .glb (vertex colours), the ground tiles as .png. (Item icons are
//! painted by the interface and exported from there.) Dropped into `assets/` the files draw the same picture.

use crate::assets::{self, GlbPart, MV};
use crate::mesh::*;
use crate::sprites;
use alaz_sim::{BuildingKind, NodeKind, World};
use std::collections::HashMap;
use std::path::Path;

fn to_mv(v: &Vertex, origin: [f32; 3]) -> MV {
    MV { pos: [v.pos[0] - origin[0], v.pos[1] - origin[1], v.pos[2] - origin[2]], color: v.color, glow: v.glow, light: v.uv[0], mat: v.uv[1] }
}

fn tri_key(t: &[Vertex]) -> Vec<u8> {
    bytemuck::cast_slice::<Vertex, u8>(t).to_vec()
}

/// Splits two poses of one model into what both share and what only one of them has (whole triangles).
fn split(idle: &[Vertex], busy: &[Vertex]) -> (Vec<Vertex>, Vec<Vertex>, Vec<Vertex>) {
    let mut pool: HashMap<Vec<u8>, usize> = HashMap::new();
    for t in busy.chunks_exact(3) {
        *pool.entry(tri_key(t)).or_default() += 1;
    }
    let (mut both, mut only_idle, mut only_busy) = (Vec::new(), Vec::new(), Vec::new());
    for t in idle.chunks_exact(3) {
        match pool.get_mut(&tri_key(t)) {
            Some(n) if *n > 0 => {
                *n -= 1;
                both.extend_from_slice(t);
            }
            _ => only_idle.extend_from_slice(t),
        }
    }
    // the busy-only triangles: those the shared ones do not account for
    let mut used: HashMap<Vec<u8>, usize> = HashMap::new();
    for t in both.chunks_exact(3) {
        *used.entry(tri_key(t)).or_default() += 1;
    }
    for t in busy.chunks_exact(3) {
        match used.get_mut(&tri_key(t)) {
            Some(n) if *n > 0 => *n -= 1,
            _ => only_busy.extend_from_slice(t),
        }
    }
    (both, only_idle, only_busy)
}

/// A model's .glb from its idle and working poses: the triangles of both are the main part, the ones only a working
/// machine has are `lit_extra`, the ones only an idle one has are `idle_extra`.
pub(crate) fn glb_of_poses(idle: &[Vertex], busy: &[Vertex], origin: [f32; 3]) -> Vec<u8> {
    glb_with_moving(idle, busy, origin, &[])
}

/// The same, with moving parts (`spin_`, `piston_` nodes) written as parts of their own.
fn glb_with_moving(idle: &[Vertex], busy: &[Vertex], origin: [f32; 3], moving: &[(String, [f32; 3], f32, f32, Vec<MV>)]) -> Vec<u8> {
    let (both, only_idle, only_busy) = split(idle, busy);
    let conv = |v: &[Vertex]| v.iter().map(|x| to_mv(x, origin)).collect::<Vec<MV>>();
    let (a, b, c) = (conv(&both), conv(&only_idle), conv(&only_busy));
    let mut parts = vec![GlbPart { name: "model", pivot: [0.0; 3], verts: &a, extras: None }];
    if !b.is_empty() {
        parts.push(GlbPart { name: "idle_extra", pivot: [0.0; 3], verts: &b, extras: None });
    }
    if !c.is_empty() {
        parts.push(GlbPart { name: "lit_extra", pivot: [0.0; 3], verts: &c, extras: None });
    }
    for (name, pivot, speed, amp, verts) in moving {
        parts.push(GlbPart { name, pivot: [pivot[0] - origin[0], pivot[1] - origin[1], pivot[2] - origin[2]], verts, extras: Some([*speed, *amp]) });
    }
    assets::write_glb(&parts)
}

/// A building's .glb from its two poses: what the building drew at rest and at work, with the moving parts the
/// machine listed (see `Mesh::marks`) taken out and written as `spin_` / `piston_` nodes.
pub(crate) fn glb_of_building(idle: &Mesh, busy: &Mesh) -> Vec<u8> {
    let strip = |m: &Mesh| -> Vec<Vertex> {
        let marks = m.marks.as_deref().unwrap_or(&[]);
        m.verts.iter().enumerate().filter(|(i, _)| !marks.iter().any(|p| (p.from..p.to).contains(i))).map(|(_, v)| *v).collect()
    };
    let moving: Vec<(String, [f32; 3], f32, f32, Vec<MV>)> = busy
        .marks
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|p| (p.name.clone(), p.pivot, p.speed, p.amp, busy.verts[p.from..p.to].iter().map(|v| to_mv(v, [0.0; 3])).collect()))
        .collect();
    glb_with_moving(&strip(idle), &strip(busy), [0.0; 3], &moving)
}

pub(crate) fn building_poses(kind: BuildingKind) -> (Mesh, Mesh) {
    let mut idle = Mesh { marks: Some(Vec::new()), ..Mesh::new() };
    let mut busy = Mesh { marks: Some(Vec::new()), ..Mesh::new() };
    if kind == BuildingKind::Campfire {
        push_campfire(&mut idle, 0.0, 0.0, 0.0, 0.0, None);
        push_campfire(&mut busy, 0.0, 0.0, 1.0, 0.0, None);
    } else {
        push_building(&mut idle, kind, 0.0, 0.0, false, 0, None);
        push_building(&mut busy, kind, 0.0, 0.0, true, 0, None);
    }
    (idle, busy)
}

/// The nodes to export: one of each kind (and of each tree species), as `(file name, mesh, anchor)`.
pub(crate) fn node_samples(known: bool) -> Vec<(String, Mesh, [f32; 3])> {
    let mut world = World::with_size(alaz_sim::map::tag_seed_v2(7), 4096, 4096);
    if known {
        for k in alaz_sim::ItemKind::ALL {
            world.mark_seen(k);
        }
    }
    let mut out: Vec<(String, Mesh, [f32; 3])> = Vec::new();
    let mut seen_names: Vec<String> = Vec::new();
    let mut trees: HashMap<&'static str, usize> = HashMap::new();
    for n in world.nodes.iter() {
        let name = if n.kind == NodeKind::Tree {
            // a few of each species, so a forest from the files is not one tree repeated
            let seed = alaz_sim::rng::hash2(world.seed ^ 0x51, n.tx, n.ty) as u32;
            let sp = tree_species_name(&world, n, seed);
            let k = trees.entry(sp).or_default();
            if *k >= 4 {
                continue;
            }
            *k += 1;
            format!("tree_{sp}_{k}")
        } else {
            assets::node_name(n.kind).to_string()
        };
        if seen_names.contains(&name) {
            continue;
        }
        seen_names.push(name.clone());
        let mut m = Mesh::new();
        push_node(&mut m, &world, n, 0);
        if m.is_empty() {
            continue;
        }
        let a = [m.anchor[0], 0.0, m.anchor[1]];
        out.push((name, m, a));
        if seen_names.len() > 90 {
            break;
        }
    }
    out
}

/// Writes the models and tiles into `dir` (as `models/*.glb`, `tiles/*.png`); returns what was written.
pub fn export_world_assets(dir: &Path) -> std::io::Result<Vec<String>> {
    assets::pause(true);
    let r = (|| {
        let mut done = Vec::new();
        std::fs::create_dir_all(dir.join("models"))?;
        std::fs::create_dir_all(dir.join("tiles"))?;
        for kind in BuildingKind::ALL {
            let (idle, busy) = building_poses(kind);
            let name = format!("models/{}.glb", assets::building_name(kind));
            std::fs::write(dir.join(&name), glb_of_building(&idle, &busy))?;
            done.push(name);
        }
        let known = node_samples(true);
        let unknown = node_samples(false);
        for (name, m, a) in &known {
            let f = format!("models/{name}.glb");
            std::fs::write(dir.join(&f), glb_of_poses(&m.verts, &m.verts, *a))?;
            done.push(f);
            // the version before its ore is recognised, when it looks different
            if let Some((_, u, ua)) = unknown.iter().find(|(n, _, _)| n == name) {
                if tri_key(&u.verts) != tri_key(&m.verts) {
                    let f = format!("models/{name}_unknown.glb");
                    std::fs::write(dir.join(&f), glb_of_poses(&u.verts, &u.verts, *ua))?;
                    done.push(f);
                }
            }
        }
        for (name, bmp) in sprites::export_tiles() {
            let f = format!("tiles/{name}.png");
            let mut rgba = Vec::with_capacity(bmp.d.len() * 4);
            for p in &bmp.d {
                rgba.extend_from_slice(p);
            }
            std::fs::write(dir.join(&f), assets::encode_png(bmp.w as u32, bmp.h as u32, &rgba))?;
            done.push(f);
        }
        Ok(done)
    })();
    assets::pause(false);
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::Place;

    /// Whether two meshes hold the same triangles (any order), positions measured from each mesh's own anchor within
    /// `tol` metres. Everything else (colour, glow, light, material) has to match exactly.
    fn same_tris(a: &[Vertex], b: &[Vertex], anchor_a: [f32; 2], anchor_b: [f32; 2], tol: f32) -> Result<(), String> {
        if a.len() != b.len() {
            return Err(format!("{} vs {} vertices", a.len(), b.len()));
        }
        type Key = [u32; 6];
        let key = |t: &[Vertex]| -> Key { [t[0].color[0].to_bits(), t[0].color[1].to_bits(), t[0].color[2].to_bits(), t[0].glow.to_bits(), t[0].uv[0].to_bits(), t[0].uv[1].to_bits()] };
        let rel = |t: &[Vertex], an: [f32; 2]| -> [[f32; 3]; 3] { std::array::from_fn(|i| [t[i].pos[0] - an[0], t[i].pos[1], t[i].pos[2] - an[1]]) };
        let mut buckets: HashMap<Key, Vec<[[f32; 3]; 3]>> = HashMap::new();
        for t in b.chunks_exact(3) {
            buckets.entry(key(t)).or_default().push(rel(t, anchor_b));
        }
        for t in a.chunks_exact(3) {
            let p = rel(t, anchor_a);
            let list = buckets.get_mut(&key(t)).ok_or("a triangle's attributes are missing")?;
            let pos = list.iter().position(|q| (0..3).all(|i| (0..3).all(|k| (p[i][k] - q[i][k]).abs() <= tol)));
            match pos {
                Some(i) => {
                    list.swap_remove(i);
                }
                None => return Err(format!("no match for triangle {p:?}")),
            }
        }
        Ok(())
    }

    #[test]
    fn buildings_round_trip_through_glb() {
        for kind in BuildingKind::ALL {
            let (idle, busy) = building_poses(kind);
            let glb = glb_of_building(&idle, &busy);
            let model = assets::load_glb(&glb, &Default::default()).unwrap_or_else(|e| panic!("{kind:?}: {e}"));
            for (active, want) in [(false, &idle), (true, &busy)] {
                let mut got = Mesh::new();
                model.place(&mut got, &Place::at(0.0, 0.0).working(active, 0));
                same_tris(&got.verts, &want.verts, [0.0; 2], [0.0; 2], 1e-5).unwrap_or_else(|e| panic!("{kind:?} active={active}: {e}"));
            }
        }
    }

    #[test]
    fn scenery_round_trips_through_glb() {
        let samples = node_samples(true);
        assert!(samples.len() >= 5, "only {} kinds of scenery found", samples.len());
        for (name, m, a) in samples {
            let glb = glb_of_poses(&m.verts, &m.verts, a);
            let model = assets::load_glb(&glb, &Default::default()).unwrap_or_else(|e| panic!("{name}: {e}"));
            let mut got = Mesh::new();
            model.place(&mut got, &Place::at(a[0], a[2]));
            // (a mesh far from the origin has coarser floats: a few tenths of a millimetre)
            same_tris(&got.verts, &m.verts, [a[0], a[2]], [a[0], a[2]], 2e-3).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }
}
