//! glTF 2.0 binary files (.glb). Triangles only. Colour comes from the vertex colours (`COLOR_0`), the material's base
//! colour factor and, when the file carries a PNG, its base colour texture sampled at each vertex. A material with an
//! emissive colour shines by itself. The node tree is flattened into world space; nodes named `spin_`, `piston_`,
//! `lit_`, `idle_` or `foliage_` start parts of their own (their children belong to them).
//!
//! The game's own extra per-vertex numbers are read when present and written on export, so a model the game exported
//! comes back exactly: `_LIGHT` (face light level), `_GLOW` (shines by itself) and `_MAT` (painted material).

use super::model::{face_light, Model, Opts, Part, Role, MV};
use glam::{Mat4, Vec3};
use serde_json::json;
use std::collections::HashMap;

/// Part role from a node name (case does not matter), plus the extras the node carries.
pub fn role_of(name: &str, extras: Option<&serde_json::Value>) -> (Role, bool) {
    let n = name.to_ascii_lowercase();
    let num = |k: &str| extras.and_then(|e| e.get(k)).and_then(|v| v.as_f64()).map(|v| v as f32);
    let axis_spin = |axis: usize| Role::Spin { axis, speed: num("speed").unwrap_or(0.03) };
    if n.starts_with("spinx_") {
        (axis_spin(0), false)
    } else if n.starts_with("spiny_") || n.starts_with("spin_") {
        (axis_spin(1), false)
    } else if n.starts_with("spinz_") {
        (axis_spin(2), false)
    } else if n.starts_with("piston_") {
        (Role::Piston { amp: num("amp").unwrap_or(0.15), speed: num("speed").unwrap_or(0.2) }, false)
    } else if n.starts_with("lit_") {
        (Role::Lit, false)
    } else if n.starts_with("idle_") {
        (Role::Idle, false)
    } else if n.starts_with("foliage_") {
        (Role::Static, true)
    } else {
        (Role::Static, false)
    }
}

fn is_special(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    ["spinx_", "spiny_", "spinz_", "spin_", "piston_", "lit_", "idle_", "foliage_"].iter().any(|p| n.starts_with(p))
}

struct Ctx<'a> {
    blob: Option<&'a [u8]>,
    scale: f32,
    parts: Vec<Part>,
    by_node: HashMap<usize, usize>,
    textures: HashMap<usize, Option<super::Image>>,
}

pub fn load(bytes: &[u8], opts: &Opts) -> Result<Model, String> {
    let g = gltf::Gltf::from_slice(bytes).map_err(|e| e.to_string())?;
    let scene = g.default_scene().or_else(|| g.scenes().next()).ok_or("sahne yok")?;
    let mut cx = Ctx { blob: g.blob.as_deref(), scale: opts.scale.unwrap_or(1.0), parts: Vec::new(), by_node: HashMap::new(), textures: HashMap::new() };
    for n in scene.nodes() {
        cx.node(&n, Mat4::IDENTITY, None, 0);
    }
    cx.parts.retain(|p| !p.verts.is_empty());
    if cx.parts.is_empty() {
        return Err("üçgen yok".into());
    }
    let top = cx.parts.iter().flat_map(|p| p.verts.iter()).map(|v| v.pos[1]).fold(0.0f32, f32::max);
    Ok(Model { parts: cx.parts, opts: opts.clone(), height: top })
}

impl Ctx<'_> {
    fn part_for(&mut self, key: Option<usize>, make: impl FnOnce() -> Part) -> usize {
        // (the static part is keyed by None, stored under usize::MAX)
        let k = key.unwrap_or(usize::MAX);
        if let Some(&i) = self.by_node.get(&k) {
            return i;
        }
        self.parts.push(make());
        self.by_node.insert(k, self.parts.len() - 1);
        self.parts.len() - 1
    }

    fn node(&mut self, n: &gltf::Node, parent: Mat4, owner: Option<usize>, depth: u32) {
        if depth > 64 {
            return;
        }
        let world = parent * Mat4::from_cols_array_2d(&n.transform().matrix());
        let mut owner = owner;
        if let Some(name) = n.name().filter(|nm| is_special(nm)) {
            let extras = n.extras().as_ref().and_then(|r| serde_json::from_str::<serde_json::Value>(r.get()).ok());
            let (role, foliage) = role_of(name, extras.as_ref());
            let pivot = world.transform_point3(Vec3::ZERO) * self.scale;
            let key = n.index();
            self.part_for(Some(key), || Part { name: name.to_string(), role, pivot: pivot.to_array(), foliage, verts: Vec::new() });
            owner = Some(key);
        }
        if let Some(mesh) = n.mesh() {
            let pi = self.part_for(owner, || Part { name: "model".into(), role: Role::Static, pivot: [0.0; 3], foliage: false, verts: Vec::new() });
            for prim in mesh.primitives() {
                let verts = self.primitive(&prim, world);
                self.parts[pi].verts.extend(verts);
            }
        }
        for c in n.children() {
            self.node(&c, world, owner, depth + 1);
        }
    }

    fn texture(&mut self, tex: &gltf::Texture) -> Option<&super::Image> {
        let idx = tex.index();
        if !self.textures.contains_key(&idx) {
            let img = match tex.source().source() {
                gltf::image::Source::View { view, mime_type } if mime_type == "image/png" => {
                    let blob = self.blob;
                    blob.and_then(|b| b.get(view.offset()..view.offset() + view.length())).and_then(|raw| super::decode_png(raw).ok())
                }
                gltf::image::Source::View { mime_type, .. } => {
                    eprintln!("[assets] doku biçimi desteklenmiyor ({mime_type}); yalnız PNG");
                    None
                }
                _ => None,
            };
            self.textures.insert(idx, img);
        }
        self.textures.get(&idx).and_then(|i| i.as_ref())
    }

    fn primitive(&mut self, prim: &gltf::Primitive, world: Mat4) -> Vec<MV> {
        if prim.mode() != gltf::mesh::Mode::Triangles {
            return Vec::new();
        }
        let blob = self.blob;
        let get = |b: gltf::Buffer| match b.source() {
            gltf::buffer::Source::Bin => blob,
            gltf::buffer::Source::Uri(_) => None,
        };
        let reader = prim.reader(get);
        let Some(pos) = reader.read_positions().map(|p| p.collect::<Vec<_>>()) else { return Vec::new() };
        let idx: Vec<u32> = reader.read_indices().map(|i| i.into_u32().collect()).unwrap_or_else(|| (0..pos.len() as u32).collect());
        let colors: Option<Vec<[f32; 3]>> = reader.read_colors(0).map(|c| c.into_rgb_f32().collect());
        let normals: Option<Vec<[f32; 3]>> = reader.read_normals().map(|n| n.collect());
        let uvs: Option<Vec<[f32; 2]>> = reader.read_tex_coords(0).map(|t| t.into_f32().collect());
        let custom = |name: &str| -> Option<Vec<f32>> {
            let acc = prim.get(&gltf::Semantic::Extras(name.to_string()))?;
            gltf::accessor::Iter::<f32>::new(acc, get).map(|i| i.collect())
        };
        let (light, glow, mat) = (custom("LIGHT"), custom("GLOW"), custom("MAT"));
        let material = prim.material();
        let pbr = material.pbr_metallic_roughness();
        let factor = pbr.base_color_factor();
        let em = material.emissive_factor();
        let emissive = em[0] + em[1] + em[2] > 0.1;
        let tex = pbr.base_color_texture().map(|t| t.texture());
        // (the texture is looked at through a copy of what the sampler needs, to keep the borrow short)
        let image = tex.and_then(|t| self.texture(&t)).map(|i| (i.w, i.h, i.rgba.clone()));
        let nm = world.inverse().transpose();
        let mut out = Vec::with_capacity(idx.len());
        for tri in idx.as_chunks::<3>().0 {
            let t = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
            if t.iter().any(|&i| i >= pos.len()) {
                continue;
            }
            let wp = t.map(|i| world.transform_point3(Vec3::from(pos[i])));
            // the face normal: the file's, else from the winding
            let n = match &normals {
                Some(ns) if t.iter().all(|&i| i < ns.len()) => {
                    let s = (Vec3::from(ns[t[0]]) + Vec3::from(ns[t[1]]) + Vec3::from(ns[t[2]])) / 3.0;
                    nm.transform_vector3(s)
                }
                _ => (wp[1] - wp[0]).cross(wp[2] - wp[0]),
            };
            let flat_light = face_light(n.to_array());
            for (k, &i) in t.iter().enumerate() {
                let mut c = colors.as_ref().and_then(|c| c.get(i)).copied().unwrap_or([1.0; 3]);
                for j in 0..3 {
                    c[j] *= factor[j];
                }
                if let (Some((w, h, rgba)), Some(uv)) = (&image, uvs.as_ref().and_then(|u| u.get(i))) {
                    let (fu, fv) = (uv[0].rem_euclid(1.0), uv[1].rem_euclid(1.0));
                    let (px, py) = (((fu * *w as f32) as u32).min(*w - 1), ((fv * *h as f32) as u32).min(*h - 1));
                    let o = ((py * *w + px) * 4) as usize;
                    for j in 0..3 {
                        c[j] *= rgba[o + j] as f32 / 255.0;
                    }
                }
                let p = wp[k] * self.scale;
                out.push(MV {
                    pos: p.to_array(),
                    color: c,
                    glow: glow.as_ref().and_then(|g| g.get(i)).copied().unwrap_or(if emissive { 1.0 } else { 0.0 }),
                    light: light.as_ref().and_then(|l| l.get(i)).copied().unwrap_or(flat_light),
                    mat: mat.as_ref().and_then(|m| m.get(i)).copied().unwrap_or(-1.0),
                });
            }
        }
        out
    }
}

/// A part to write: its node name, the point it turns about (its node's translation) and its triangles (3 vertices
/// each, positions relative to the model origin).
pub struct GlbPart<'a> {
    pub name: &'a str,
    pub pivot: [f32; 3],
    pub verts: &'a [MV],
    /// The `speed` and `amp` extras of a moving part (read back by `role_of`), if it has them.
    pub extras: Option<[f32; 2]>,
}

/// A .glb with one mesh per part: positions, flat normals, `COLOR_0`, and the game's `_LIGHT`, `_GLOW`, `_MAT`.
pub fn write_glb(parts: &[GlbPart]) -> Vec<u8> {
    let mut bin: Vec<u8> = Vec::new();
    let mut views = Vec::new();
    let mut accessors = Vec::new();
    let mut meshes = Vec::new();
    let mut nodes = Vec::new();
    let mut push = |bin: &mut Vec<u8>, data: &[f32], ty: &str, count: usize, minmax: Option<([f32; 3], [f32; 3])>| -> usize {
        let off = bin.len();
        for f in data {
            bin.extend_from_slice(&f.to_le_bytes());
        }
        views.push(json!({"buffer": 0, "byteOffset": off, "byteLength": data.len() * 4, "target": 34962}));
        let mut a = json!({"bufferView": views.len() - 1, "componentType": 5126, "count": count, "type": ty});
        if let Some((lo, hi)) = minmax {
            a["min"] = json!(lo);
            a["max"] = json!(hi);
        }
        accessors.push(a);
        accessors.len() - 1
    };
    for (pi, part) in parts.iter().enumerate() {
        let v = part.verts;
        if v.len() < 3 {
            continue;
        }
        let rel = |p: [f32; 3]| [p[0] - part.pivot[0], p[1] - part.pivot[1], p[2] - part.pivot[2]];
        let mut pos = Vec::with_capacity(v.len() * 3);
        let mut nor = Vec::with_capacity(v.len() * 3);
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for tri in v.as_chunks::<3>().0 {
            let p = [rel(tri[0].pos), rel(tri[1].pos), rel(tri[2].pos)];
            let a = Vec3::from(p[1]) - Vec3::from(p[0]);
            let b = Vec3::from(p[2]) - Vec3::from(p[0]);
            let n = a.cross(b).try_normalize().unwrap_or(Vec3::Y);
            for q in p {
                for k in 0..3 {
                    lo[k] = lo[k].min(q[k]);
                    hi[k] = hi[k].max(q[k]);
                }
                pos.extend_from_slice(&q);
                nor.extend_from_slice(&n.to_array());
            }
        }
        let col: Vec<f32> = v.iter().flat_map(|x| x.color).collect();
        let light: Vec<f32> = v.iter().map(|x| x.light).collect();
        let glow: Vec<f32> = v.iter().map(|x| x.glow).collect();
        let mat: Vec<f32> = v.iter().map(|x| x.mat).collect();
        let n = v.len();
        let a_pos = push(&mut bin, &pos, "VEC3", n, Some((lo, hi)));
        let a_nor = push(&mut bin, &nor, "VEC3", n, None);
        let a_col = push(&mut bin, &col, "VEC3", n, None);
        let a_light = push(&mut bin, &light, "SCALAR", n, None);
        let a_glow = push(&mut bin, &glow, "SCALAR", n, None);
        let a_mat = push(&mut bin, &mat, "SCALAR", n, None);
        meshes.push(json!({"name": part.name, "primitives": [{"attributes": {"POSITION": a_pos, "NORMAL": a_nor, "COLOR_0": a_col, "_LIGHT": a_light, "_GLOW": a_glow, "_MAT": a_mat}, "material": 0, "mode": 4}]}));
        let mut node = json!({"name": part.name, "mesh": meshes.len() - 1, "translation": part.pivot});
        if let Some([speed, amp]) = part.extras {
            node["extras"] = json!({"speed": speed, "amp": amp});
        }
        nodes.push(node);
        let _ = pi;
    }
    let root = json!({
        "asset": {"version": "2.0", "generator": "alaz --export-assets"},
        "scene": 0,
        "scenes": [{"nodes": (0..nodes.len()).collect::<Vec<_>>()}],
        "nodes": nodes,
        "meshes": meshes,
        "materials": [{"name": "vertex colour", "doubleSided": true, "pbrMetallicRoughness": {"baseColorFactor": [1.0, 1.0, 1.0, 1.0], "metallicFactor": 0.0, "roughnessFactor": 1.0}}],
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{"byteLength": bin.len()}],
    });
    let mut js = serde_json::to_vec(&root).unwrap_or_default();
    while !js.len().is_multiple_of(4) {
        js.push(b' ');
    }
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let total = 12 + 8 + js.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(js.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&js);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin);
    out
}
