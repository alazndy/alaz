use super::*;

/// kind: 0 solid overlay, 1 ground tile, 2 sprite, 3 decal. `off` is a whole-texel offset from the anchor (y up).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct Vertex {
    pub(crate) pos: [f32; 3],
    pub(crate) color: [f32; 3],
    pub(crate) glow: f32,
    pub(crate) uv: [f32; 2],
    pub(crate) off: [f32; 2],
    pub(crate) kind: f32,
}

/// Everything a model builder needs besides the geometry: the vertices made so far and the settings that used to be
/// hidden thread-locals, now carried by the mesh itself.
pub(crate) struct Mesh {
    pub(crate) verts: Vec<Vertex>,
    /// Ground point of the object being built: its surface texture is laid out relative to this, so the texture
    /// stays on the object when it moves.
    pub(crate) anchor: [f32; 2],
    /// While set, everything pushed is leaf material whatever its colour (autumn crowns are orange, not green).
    pub(crate) foliage: bool,
    /// Fixed scenery of a chunk: nothing in it ever leaves the ground, so an end standing on the ground can be left
    /// open. (Bodies jump, swim and lean; theirs stay closed.)
    pub(crate) rooted: bool,
    /// This frame's wind, for models that move with it (smoke): (x, z) scaled by strength, and the strength 0..1.
    pub(crate) wind: [f32; 3],
}

impl Mesh {
    pub(crate) fn new() -> Mesh {
        Mesh { verts: Vec::new(), anchor: [0.0, 0.0], foliage: false, rooted: false, wind: [0.0; 3] }
    }

    /// An empty mesh for a chunk's fixed scenery.
    pub(crate) fn fixed() -> Mesh {
        Mesh { rooted: true, ..Mesh::new() }
    }

    pub(crate) fn set_anchor(&mut self, x: f32, z: f32) {
        self.anchor = [x, z];
    }

    pub(crate) fn push(&mut self, v: Vertex) {
        self.verts.push(v);
    }

    pub(crate) fn extend_from_slice(&mut self, v: &[Vertex]) {
        self.verts.extend_from_slice(v);
    }

    /// A vertex of a model at the current anchor, with the material its colour (or the foliage switch) says.
    pub(crate) fn vl(&self, pos: [f32; 3], color: [f32; 3], glow: f32, light: f32) -> Vertex {
        let mat = if glow > 0.5 { 0.0 } else if self.foliage { 2.0 } else { material_of(color) };
        Vertex { pos, color, glow, uv: [light, mat], off: self.anchor, kind: 4.0 }
    }

    pub(crate) fn v(&self, pos: [f32; 3], color: [f32; 3], glow: f32) -> Vertex {
        self.vl(pos, color, glow, 1.0)
    }
}

impl std::ops::Deref for Mesh {
    type Target = Vec<Vertex>;
    fn deref(&self) -> &Vec<Vertex> {
        &self.verts
    }
}

impl std::ops::DerefMut for Mesh {
    fn deref_mut(&mut self) -> &mut Vec<Vertex> {
        &mut self.verts
    }
}

pub(crate) const WHITE: [f32; 3] = [1.0, 1.0, 1.0];

/// Stand the models of a cached mesh on the ground here, once, instead of in the vertex shader every frame (twice:
/// the sun's view and the eye's). The shader reads the height back from `kind` (see `unpack` in shader.wgsl); it is
/// the same bilinear surface the shader would have sampled.
pub(crate) fn pre_lift(world: &World, verts: &mut [Vertex]) {
    let mut last = ([f32::NAN; 2], 0.0);
    for v in verts.iter_mut().filter(|v| v.kind == 4.0) {
        let anchored = v.off != [0.0, 0.0];
        let at = if anchored { v.off } else { [v.pos[0], v.pos[2]] };
        // a model's vertices share one anchor, so its height is worked out once
        if at != last.0 {
            last = (at, ground_height(world.seed, at[0], at[1]));
        }
        v.kind = 1000.0 + last.1 + if anchored { 0.02 } else { 0.0 };
    }
}

pub fn ground_height(seed: u64, x: f32, z: f32) -> f32 {
    let (fx, fz) = (x.floor(), z.floor());
    let (tx, tz) = (x - fx, z - fz);
    let h = |dx: i32, dz: i32| alaz_sim::elevation_mm(seed, fx as i32 + dx, fz as i32 + dz) as f32 / 1000.0;
    let a = h(0, 0) + (h(1, 0) - h(0, 0)) * tx;
    let b = h(0, 1) + (h(1, 1) - h(0, 1)) * tx;
    a + (b - a) * tz
}

/// Which painted material a surface of this colour is made of: 0 none, 1 wood, 2 leaves, 3 stone, 4 skin and cloth.
pub(crate) fn material_of(c: [f32; 3]) -> f32 {
    let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
    let sat = (mx - mn) / mx.max(1e-3);
    if sat < 0.18 || c[2] > c[0] {
        3.0
    } else if c[1] >= c[0] * 1.05 && c[1] >= c[2] {
        2.0
    } else if c[0] >= c[1] && mx < 0.5 {
        1.0
    } else {
        4.0
    }
}

/// A flat painted rectangle lying on the ground (shadow, pool of firelight), `w` x `d` metres, centred on (x, z).
pub(crate) fn decal(out: &mut Mesh, s: Spr, x: f32, z: f32, tint: [f32; 3], glow: f32) {
    decal_sized(out, s, x, z, s.w as f32 / sprites::PPM, s.h as f32 / sprites::PPM, tint, glow);
}

/// A decal stretched to `w` x `d` metres whatever its texel size (ground fringes are one tile).
#[allow(clippy::too_many_arguments)]
pub(crate) fn decal_sized(out: &mut Mesh, s: Spr, x: f32, z: f32, w: f32, d: f32, tint: [f32; 3], glow: f32) {
    let (u0, u1, v0, v1) = (s.x as f32, (s.x + s.w) as f32, s.y as f32, (s.y + s.h) as f32);
    let q = |dx: f32, dz: f32, u: f32, vv: f32| Vertex { pos: [x + dx, 0.015, z + dz], color: tint, glow, uv: [u, vv], off: [0.0, 0.0], kind: 5.0 };
    let (a, b, c, e) = (q(-w / 2.0, -d / 2.0, u0, v0), q(w / 2.0, -d / 2.0, u1, v0), q(w / 2.0, d / 2.0, u1, v1), q(-w / 2.0, d / 2.0, u0, v1));
    out.extend_from_slice(&[a, b, c, a, c, e]);
}

/// A flat ribbon on the ground from `a` to `b`.
pub(crate) fn push_segment(out: &mut Mesh, a: [f32; 2], b: [f32; 2], width: f32, y: f32, col: [f32; 3], glow: f32) {
    out.set_anchor(a[0], a[1]);
    let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
    let len = (dx * dx + dz * dz).sqrt();
    if len < 1e-4 {
        return;
    }
    let (nx, nz) = (-dz / len * width / 2.0, dx / len * width / 2.0);
    let q = [[a[0] + nx, y, a[1] + nz], [a[0] - nx, y, a[1] - nz], [b[0] - nx, y, b[1] - nz], [b[0] + nx, y, b[1] + nz]];
    for i in [0, 1, 2, 0, 2, 3, 2, 1, 0, 3, 2, 0] {
        out.push(out.v(q[i], col, glow));
    }
}

/// Turn the vertices pushed since `start` by quarter turns about (cx, cz).
pub(crate) fn turn_quarters(out: &mut [Vertex], start: usize, cx: f32, cz: f32, q: u8) {
    for v in &mut out[start..] {
        for _ in 0..(q & 3) {
            let (dx, dz) = (v.pos[0] - cx, v.pos[2] - cz);
            v.pos[0] = cx - dz;
            v.pos[2] = cz + dx;
        }
    }
}

/// Soft value noise on the tile grid, -1..1.
pub(crate) fn noise(world: &World, salt: u64, x: f32, y: f32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let h = |dx: i32, dy: i32| alaz_sim::rng::hash2(world.seed ^ salt, x0 as i32 + dx, y0 as i32 + dy) as f32 % 1000.0 / 500.0 - 1.0;
    let (a, b) = (h(0, 0) + (h(1, 0) - h(0, 0)) * sx, h(0, 1) + (h(1, 1) - h(0, 1)) * sx);
    a + (b - a) * sy
}
