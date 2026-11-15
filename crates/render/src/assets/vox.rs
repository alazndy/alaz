//! MagicaVoxel files. The scene graph is flattened: every shape becomes voxels in one lattice, named transforms
//! (`spin_wheel`) become parts of their own. Each part is meshed greedily: hidden faces are dropped, and
//! neighbouring faces of one colour merge into a single rectangle.
//!
//! Axes: MagicaVoxel is z-up with its front on the -y side; the game is y-up with the front toward +z. So
//! game x = voxel x, game y = voxel z, game z = -voxel y.

use super::model::{face_light, Model, Opts, Part, Role, MV};
use std::collections::HashMap;

/// Metres in one voxel unless the .toml says otherwise (the sprites of the old atlas were 16 texels to the metre).
pub const VOXEL: f32 = 1.0 / 16.0;

type Cell = [i32; 3];

/// A signed permutation matrix from MagicaVoxel's rotation byte: row r takes `sign * v[col]`.
#[derive(Clone, Copy)]
struct Rot([(usize, i32); 3]);

impl Rot {
    const ID: Rot = Rot([(0, 1), (1, 1), (2, 1)]);

    fn from_byte(b: u8) -> Rot {
        let (i0, i1) = ((b & 3) as usize, ((b >> 2) & 3) as usize);
        if i0 > 2 || i1 > 2 || i0 == i1 {
            return Rot::ID;
        }
        let i2 = 3 - i0 - i1;
        let sg = |bit: u8| if b & (1 << bit) != 0 { -1 } else { 1 };
        Rot([(i0, sg(4)), (i1, sg(5)), (i2, sg(6))])
    }

    fn apply(&self, v: [i32; 3]) -> [i32; 3] {
        [0, 1, 2].map(|r| self.0[r].1 * v[self.0[r].0])
    }

    /// self after `inner`.
    fn then(&self, inner: &Rot) -> Rot {
        // column c of the product: self applied to column c of inner
        let mut rows = [(0usize, 1i32); 3];
        for c in 0..3 {
            let mut e = [0i32; 3];
            e[c] = 1;
            let r = self.apply(inner.apply(e));
            for k in 0..3 {
                if r[k] != 0 {
                    rows[k] = (c, r[k]);
                }
            }
        }
        Rot(rows)
    }
}

struct Instance {
    name: Option<String>,
    cells: Vec<(Cell, u8)>,
}

pub fn load(bytes: &[u8], opts: &Opts) -> Result<Model, String> {
    let data = dot_vox::load_bytes(bytes).map_err(|e| e.to_string())?;
    let mut inst: Vec<Instance> = Vec::new();
    // each shape: the model's cells moved by the transforms above it
    let mut visit = |model_id: u32, rot: Rot, t2: [i32; 3], name: Option<String>| {
        let Some(m) = data.models.get(model_id as usize) else { return };
        let half = [m.size.x as i32 / 2, m.size.y as i32 / 2, m.size.z as i32 / 2];
        let mut cells = Vec::with_capacity(m.voxels.len());
        for v in &m.voxels {
            // doubled coordinates of the voxel's centre, so a flip lands on whole cells
            let d = [2 * (v.x as i32 - half[0]) + 1, 2 * (v.y as i32 - half[1]) + 1, 2 * (v.z as i32 - half[2]) + 1];
            let r = rot.apply(d);
            let w = [(r[0] + t2[0] - 1) / 2, (r[1] + t2[1] - 1) / 2, (r[2] + t2[2] - 1) / 2];
            // (the division is exact: r is odd, t2 even)
            cells.push((w, v.i));
        }
        inst.push(Instance { name, cells });
    };
    if data.scenes.is_empty() {
        for id in 0..data.models.len() {
            visit(id as u32, Rot::ID, [0; 3], None);
        }
    } else {
        fn walk(data: &dot_vox::DotVoxData, node: u32, rot: Rot, t2: [i32; 3], name: Option<String>, depth: u32, visit: &mut dyn FnMut(u32, Rot, [i32; 3], Option<String>)) {
            if depth > 64 {
                return;
            }
            match data.scenes.get(node as usize) {
                Some(dot_vox::SceneNode::Transform { attributes, frames, child, .. }) => {
                    let f = frames.first();
                    let r = f.and_then(|f| f.attributes.get("_r")).and_then(|s| s.trim().parse::<u8>().ok()).map(Rot::from_byte).unwrap_or(Rot::ID);
                    let t = f.and_then(|f| f.position()).map(|p| [p.x, p.y, p.z]).unwrap_or([0; 3]);
                    let name = attributes.get("_name").filter(|n| !n.is_empty()).cloned().or(name);
                    // world = parent_rot * (rot * p + t) + parent_t
                    let tw = rot.apply(t);
                    let t2n = [t2[0] + 2 * tw[0], t2[1] + 2 * tw[1], t2[2] + 2 * tw[2]];
                    walk(data, *child, rot.then(&r), t2n, name, depth + 1, visit);
                }
                Some(dot_vox::SceneNode::Group { children, .. }) => {
                    for c in children {
                        walk(data, *c, rot, t2, name.clone(), depth + 1, visit);
                    }
                }
                Some(dot_vox::SceneNode::Shape { models, .. }) => {
                    if let Some(m) = models.first() {
                        visit(m.model_id, rot, t2, name);
                    }
                }
                None => {}
            }
        }
        walk(&data, 0, Rot::ID, [0; 3], None, 0, &mut visit);
    }
    if inst.iter().all(|i| i.cells.is_empty()) {
        return Err("boş model".into());
    }

    // the whole model's bounds: the origin is the middle of the underside
    let (mut lo, mut hi) = ([i32::MAX; 3], [i32::MIN; 3]);
    for (c, _) in inst.iter().flat_map(|i| i.cells.iter()) {
        for k in 0..3 {
            lo[k] = lo[k].min(c[k]);
            hi[k] = hi[k].max(c[k]);
        }
    }
    let scale = opts.scale.unwrap_or(VOXEL);
    // game axes: x = vx, y = vz, z = -vy ; the origin in voxel units
    let ox = (lo[0] + hi[0] + 1) as f32 / 2.0;
    let oy = lo[2] as f32;
    let oz = (lo[1] + hi[1] + 1) as f32 / 2.0;
    let to_game = |v: [f32; 3]| [(v[0] - ox) * scale, (v[2] - oy) * scale, -(v[1] - oz) * scale];

    // palette and emissive materials
    let colour = |i: u8| data.palette.get(i as usize).map(|c| [c.r, c.g, c.b]).unwrap_or([200, 200, 200]);
    let emissive = |i: u8| {
        let by_material = data.materials.iter().any(|m| m.id == i as u32 + 1 && (m.material_type() == Some("_emit") || m.properties.get("_emit").and_then(|v| v.parse::<f32>().ok()).is_some_and(|e| e > 0.0)));
        by_material || opts.glow.contains(&colour(i))
    };

    // group instances into parts by name; the unnamed share the static one
    let mut order: Vec<Option<String>> = Vec::new();
    let mut groups: HashMap<Option<String>, Vec<(Cell, u8)>> = HashMap::new();
    for i in inst {
        if !groups.contains_key(&i.name) {
            order.push(i.name.clone());
        }
        groups.entry(i.name).or_default().extend(i.cells);
    }
    let mut parts = Vec::new();
    let mut top = 0.0f32;
    for key in order {
        let cells = groups.remove(&key).unwrap_or_default();
        let name = key.clone().unwrap_or_else(|| "model".into());
        let (role, foliage) = super::glb::role_of(&name, None);
        let verts = mesh_part(&cells, &colour, &emissive, opts, &to_game);
        if verts.is_empty() {
            continue;
        }
        let (mut a, mut b) = ([f32::MAX; 3], [f32::MIN; 3]);
        for v in &verts {
            for k in 0..3 {
                a[k] = a[k].min(v.pos[k]);
                b[k] = b[k].max(v.pos[k]);
            }
        }
        top = top.max(b[1]);
        parts.push(Part { name, role, pivot: [0, 1, 2].map(|k| (a[k] + b[k]) / 2.0), foliage, verts });
    }
    // (a model with no static part first would still draw; order only matters for export)
    let _ = Role::Static;
    Ok(Model { parts, opts: opts.clone(), height: top })
}

/// Greedy meshing of one part's cells.
fn mesh_part(cells: &[(Cell, u8)], colour: &dyn Fn(u8) -> [u8; 3], emissive: &dyn Fn(u8) -> bool, opts: &Opts, to_game: &dyn Fn([f32; 3]) -> [f32; 3]) -> Vec<MV> {
    let (mut lo, mut hi) = ([i32::MAX; 3], [i32::MIN; 3]);
    for (c, _) in cells {
        for k in 0..3 {
            lo[k] = lo[k].min(c[k]);
            hi[k] = hi[k].max(c[k]);
        }
    }
    if cells.is_empty() {
        return Vec::new();
    }
    let dim = [(hi[0] - lo[0] + 1) as usize, (hi[1] - lo[1] + 1) as usize, (hi[2] - lo[2] + 1) as usize];
    // 0 = empty, else palette index + 1
    let mut grid = vec![0u16; dim[0] * dim[1] * dim[2]];
    let at = |x: usize, y: usize, z: usize| (z * dim[1] + y) * dim[0] + x;
    for (c, i) in cells {
        grid[at((c[0] - lo[0]) as usize, (c[1] - lo[1]) as usize, (c[2] - lo[2]) as usize)] = *i as u16 + 1;
    }
    let get = |p: [i32; 3]| -> u16 {
        if (0..3).any(|k| p[k] < 0 || p[k] >= dim[k] as i32) {
            0
        } else {
            grid[at(p[0] as usize, p[1] as usize, p[2] as usize)]
        }
    };
    let mut out = Vec::new();
    // for each of the six face directions: sweep slices along the axis, merge equal faces in the slice's plane
    for axis in 0..3usize {
        for dir in [-1i32, 1] {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let mut n = [0f32; 3];
            n[axis] = dir as f32;
            // the voxel normal in game axes: x = vx, y = vz, z = -vy
            let ng = [n[0], n[2], -n[1]];
            let light = face_light(ng);
            for s in 0..dim[axis] as i32 {
                let mut mask = vec![0u16; dim[u] * dim[v]];
                for j in 0..dim[v] {
                    for i in 0..dim[u] {
                        let mut p = [0i32; 3];
                        p[axis] = s;
                        p[u] = i as i32;
                        p[v] = j as i32;
                        let c = get(p);
                        if c == 0 {
                            continue;
                        }
                        let mut q = p;
                        q[axis] += dir;
                        if get(q) == 0 {
                            mask[j * dim[u] + i] = c;
                        }
                    }
                }
                for j in 0..dim[v] {
                    let mut i = 0;
                    while i < dim[u] {
                        let c = mask[j * dim[u] + i];
                        if c == 0 {
                            i += 1;
                            continue;
                        }
                        let mut w = 1;
                        while i + w < dim[u] && mask[j * dim[u] + i + w] == c {
                            w += 1;
                        }
                        let mut h = 1;
                        'grow: while j + h < dim[v] {
                            for k in 0..w {
                                if mask[(j + h) * dim[u] + i + k] != c {
                                    break 'grow;
                                }
                            }
                            h += 1;
                        }
                        for jj in 0..h {
                            for ii in 0..w {
                                mask[(j + jj) * dim[u] + i + ii] = 0;
                            }
                        }
                        // the rectangle's corners in the lattice (voxel units, absolute)
                        let plane = (s + if dir > 0 { 1 } else { 0 } + lo[axis]) as f32;
                        let corner = |du: usize, dv: usize| {
                            let mut p = [0f32; 3];
                            p[axis] = plane;
                            p[u] = (lo[u] + (i + du) as i32) as f32;
                            p[v] = (lo[v] + (j + dv) as i32) as f32;
                            to_game(p)
                        };
                        let (a, b, cc, d) = (corner(0, 0), corner(w, 0), corner(w, h), corner(0, h));
                        let idx = (c - 1) as u8;
                        let rgb = colour(idx);
                        let color = [rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0];
                        let glow = if emissive(idx) { 1.0 } else { 0.0 };
                        let mat = if opts.leaf.contains(&rgb) { 2.0 } else { -1.0 };
                        for p in [a, b, cc, a, cc, d] {
                            out.push(MV { pos: p, color, glow, light, mat });
                        }
                        i += w;
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use dot_vox::{Color, DotVoxData, Model as VModel, Size, Voxel};

    fn vox(size: (u32, u32, u32), cells: &[(u8, u8, u8, u8)]) -> Vec<u8> {
        let mut palette = vec![Color { r: 255, g: 255, b: 255, a: 255 }; 256];
        palette[0] = Color { r: 200, g: 40, b: 40, a: 255 };
        palette[1] = Color { r: 40, g: 40, b: 200, a: 255 };
        let data = DotVoxData {
            version: 150,
            index_map: (0..=255u8).collect(),
            models: vec![VModel { size: Size { x: size.0, y: size.1, z: size.2 }, voxels: cells.iter().map(|&(x, y, z, i)| Voxel { x, y, z, i }).collect() }],
            palette,
            materials: vec![],
            scenes: vec![],
            layers: vec![],
        };
        let mut out = Vec::new();
        data.write_vox(&mut out).unwrap();
        out
    }

    #[test]
    fn a_solid_block_meshes_to_six_rectangles() {
        let mut cells = Vec::new();
        for x in 0..4 {
            for y in 0..2 {
                for z in 0..3 {
                    cells.push((x, y, z, 0));
                }
            }
        }
        let m = load(&vox((4, 2, 3), &cells), &Opts::default()).unwrap();
        let v = &m.parts[0].verts;
        assert_eq!(v.len(), 36, "greedy meshing: one rectangle per side");
        // 4 x 2 x 3 voxels of 1/16 m, standing on the origin
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in v {
            for k in 0..3 {
                lo[k] = lo[k].min(p.pos[k]);
                hi[k] = hi[k].max(p.pos[k]);
            }
        }
        let e = 1e-5;
        assert!((lo[0] + 4.0 / 32.0).abs() < e && (hi[0] - 4.0 / 32.0).abs() < e, "x centred: {lo:?} {hi:?}");
        assert!(lo[1].abs() < e && (hi[1] - 3.0 / 16.0).abs() < e, "y from the ground up");
        assert!((lo[2] + 2.0 / 32.0).abs() < e && (hi[2] - 2.0 / 32.0).abs() < e, "z centred");
        assert!((m.height - 3.0 / 16.0).abs() < e);
    }

    #[test]
    fn hidden_faces_are_dropped_and_colours_keep_apart() {
        // two voxels side by side in different colours: 5 + 5 faces, the two touching ones hidden (the long sides do not merge)
        let m = load(&vox((2, 1, 1), &[(0, 0, 0, 0), (1, 0, 0, 1)]), &Opts::default()).unwrap();
        assert_eq!(m.parts[0].verts.len(), 10 * 6, "ten quads, five of each colour");
    }
}
