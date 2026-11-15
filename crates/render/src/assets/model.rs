//! A model loaded from a file, ready to be stamped into a mesh: vertices in metres, y up, origin where the artist put
//! it (a .vox is moved so the origin is the middle of its underside), split into named parts so some of them can move.

use crate::mesh::Mesh;
use std::sync::Arc;

/// One vertex of a model. `mat` < 0 means "work the material out from the colour".
#[derive(Clone, Copy, Debug)]
pub struct MV {
    pub pos: [f32; 3],
    pub color: [f32; 3],
    pub glow: f32,
    pub light: f32,
    pub mat: f32,
}

/// What a part does. Set by the name of the part (`spin_`, `spinx_`, `spiny_`, `spinz_`, `piston_`, `lit_`, `idle_`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Role {
    Static,
    /// Drawn only while the machine works.
    Lit,
    /// Drawn only while it stands idle.
    Idle,
    /// Turns about its pivot (radians per tick) while the machine works.
    Spin { axis: usize, speed: f32 },
    /// Slides along its own y (metres, cycles per tick) while the machine works.
    Piston { amp: f32, speed: f32 },
}

#[derive(Clone, Debug)]
pub struct Part {
    pub name: String,
    pub role: Role,
    /// The point a spinning part turns about.
    pub pivot: [f32; 3],
    pub foliage: bool,
    pub verts: Vec<MV>,
}

/// Settings from the `<name>.toml` beside a model.
#[derive(Clone, Debug)]
pub struct Opts {
    /// .vox: metres per voxel (default 1/16). .glb: a multiplier (default 1).
    pub scale: Option<f32>,
    /// The whole model is leaf material (swayed by the wind).
    pub foliage: bool,
    /// Scenery gets a random turn about the vertical axis (default true).
    pub rotate: bool,
    /// Trees only: stretch the model so it is as tall as the tree in the world (default true for trees).
    pub fit_height: Option<bool>,
    /// Colours (as bytes) that shine by themselves, and colours that are leaves.
    pub glow: Vec<[u8; 3]>,
    pub leaf: Vec<[u8; 3]>,
    pub offset: [f32; 3],
}

impl Default for Opts {
    fn default() -> Opts {
        Opts { scale: None, foliage: false, rotate: true, fit_height: None, glow: Vec::new(), leaf: Vec::new(), offset: [0.0; 3] }
    }
}

fn hex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().trim_start_matches('#');
    (s.len() == 6).then(|| [0, 2, 4].map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap_or(0)))
}

impl Opts {
    pub fn parse(text: Option<&str>) -> Opts {
        let mut o = Opts::default();
        let Some(text) = text else { return o };
        let table = match text.parse::<toml::Table>() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[assets] .toml okunamadı: {e}");
                return o;
            }
        };
        let num = |v: &toml::Value| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)).map(|f| f as f32);
        o.scale = table.get("scale").and_then(num);
        o.foliage = table.get("foliage").and_then(|v| v.as_bool()).unwrap_or(false);
        o.rotate = table.get("rotate").and_then(|v| v.as_bool()).unwrap_or(true);
        o.fit_height = table.get("fit_height").and_then(|v| v.as_bool());
        let colors = |k: &str| table.get(k).and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|c| c.as_str().and_then(hex)).collect()).unwrap_or_default();
        o.glow = colors("glow");
        o.leaf = colors("leaf");
        if let Some(a) = table.get("offset").and_then(|v| v.as_array()) {
            for (i, v) in a.iter().take(3).enumerate() {
                o.offset[i] = num(v).unwrap_or(0.0);
            }
        }
        o
    }
}

#[derive(Clone, Debug)]
pub struct Model {
    pub parts: Vec<Part>,
    pub opts: Opts,
    /// Height of the highest vertex above the origin, metres.
    pub height: f32,
}

/// All the files of one model name: `<name>` and `<name>_1`, `<name>_2` ...
pub struct ModelSet {
    pub variants: Vec<Arc<Model>>,
}

impl ModelSet {
    pub fn pick(&self, seed: u32) -> Arc<Model> {
        // (a hash first, so neighbouring seeds do not pick in a row)
        let h = seed.wrapping_mul(0x9E37_79B1) >> 7;
        self.variants[h as usize % self.variants.len()].clone()
    }
}

/// Where and how a model is stamped: its origin in the world, the world directions of its x, y and z axes (columns,
/// scaled by `scale`), whether the machine it belongs to works, and the tick for the moving parts.
#[derive(Clone, Copy)]
pub struct Place {
    pub origin: [f32; 3],
    pub basis: [[f32; 3]; 3],
    pub active: bool,
    pub tick: u64,
    pub tint: Option<[f32; 3]>,
}

impl Place {
    pub fn at(x: f32, z: f32) -> Place {
        Place { origin: [x, 0.0, z], basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], active: false, tick: 0, tint: None }
    }

    /// Turned about the vertical axis by `a` radians (like a person's heading: 0 faces +z) and scaled.
    pub fn turned(mut self, a: f32, scale: f32) -> Place {
        let (s, c) = a.sin_cos();
        self.basis = [[c * scale, 0.0, -s * scale], [0.0, scale, 0.0], [s * scale, 0.0, c * scale]];
        self
    }

    pub fn working(mut self, active: bool, tick: u64) -> Place {
        self.active = active;
        self.tick = tick;
        self
    }

    pub fn tinted(mut self, tint: Option<[f32; 3]>) -> Place {
        self.tint = tint;
        self
    }
}

impl Model {
    /// The vertices of one part as they stand now (moving parts moved), in model space.
    fn part_pos(part: &Part, v: &MV, p: &Place) -> [f32; 3] {
        let mut q = v.pos;
        if p.active {
            match part.role {
                Role::Spin { axis, speed } => {
                    let (s, c) = (p.tick as f32 * speed).sin_cos();
                    let d = [q[0] - part.pivot[0], q[1] - part.pivot[1], q[2] - part.pivot[2]];
                    let (a, b) = match axis {
                        0 => (1, 2),
                        1 => (2, 0),
                        _ => (0, 1),
                    };
                    let mut r = d;
                    r[a] = d[a] * c - d[b] * s;
                    r[b] = d[a] * s + d[b] * c;
                    q = [part.pivot[0] + r[0], part.pivot[1] + r[1], part.pivot[2] + r[2]];
                }
                Role::Piston { amp, speed } => q[1] += amp * (p.tick as f32 * speed).sin().abs(),
                _ => {}
            }
        }
        q
    }

    /// Stamp the model into `out`. The mesh's anchor is set to the origin on the ground, as the code-made models do.
    pub(crate) fn place(&self, out: &mut Mesh, p: &Place) {
        out.set_anchor(p.origin[0], p.origin[2]);
        let was = out.foliage;
        let b = p.basis;
        let o = self.opts.offset;
        for part in &self.parts {
            match part.role {
                Role::Lit if !p.active => continue,
                Role::Idle if p.active => continue,
                _ => {}
            }
            out.foliage = was || part.foliage;
            for v in &part.verts {
                let mut q = Self::part_pos(part, v, p);
                for k in 0..3 {
                    q[k] += o[k];
                }
                let pos = [0, 1, 2].map(|i| p.origin[i] + b[0][i] * q[0] + b[1][i] * q[1] + b[2][i] * q[2]);
                let (color, glow) = match p.tint {
                    Some(t) => (t, 1.0),
                    None => (v.color, v.glow),
                };
                let mut vert = out.vl(pos, color, glow, v.light);
                // (a material fixed in the file wins over the one the colour would give; shining ones have none)
                if v.mat >= 0.0 && glow <= 0.5 && !out.foliage {
                    vert.uv[1] = v.mat;
                }
                out.push(vert);
            }
        }
        out.foliage = was;
    }
}

/// How lit a face looking along `n` is: the six levels of the code-made boxes (top 1, the south and east sides
/// brighter than the north and west, the underside dark), blended smoothly for slanted faces.
pub fn face_light(n: [f32; 3]) -> f32 {
    let l2 = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).max(1e-12);
    let (x, y, z) = (n[0] * n[0] / l2, n[1] * n[1] / l2, n[2] * n[2] / l2);
    x * if n[0] > 0.0 { 0.64 } else { 0.48 } + y * if n[1] > 0.0 { 1.0 } else { 0.3 } + z * if n[2] > 0.0 { 0.7 } else { 0.5 }
}
