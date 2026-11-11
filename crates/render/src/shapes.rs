//! Low-poly building blocks. Every face is flat: the albedo goes in the vertex colour and one light level per face
//! in `uv.x`; the shader cuts that level into a few bands (cel shading).

use super::{vl, Vertex};

const LIGHT: [f32; 3] = [-0.45, 0.78, -0.42];

/// Light level 0..1 of a surface facing `n` (sun from the upper left, a little ambient).
pub fn shade(n: [f32; 3]) -> f32 {
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
    let d = (LIGHT[0] * n[0] + LIGHT[1] * n[1] + LIGHT[2] * n[2]) / len / 0.993;
    0.36 + 0.64 * d.max(0.0)
}

fn hash(seed: u32, a: u32, b: u32) -> f32 {
    let mut h = seed.wrapping_mul(0x9E37_79B1) ^ a.wrapping_mul(0x85EB_CA6B) ^ b.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h % 2000) as f32 / 1000.0 - 1.0
}

fn tri(out: &mut Vec<Vertex>, p: [[f32; 3]; 3], light: f32, col: [f32; 3], glow: f32) {
    for q in p {
        out.push(vl(q, col, glow, light));
    }
}

/// A tapered, many-sided rod from `a` (radius `r0`) to `b` (radius `r1`) with flat ends.
pub fn limb(out: &mut Vec<Vertex>, a: [f32; 3], b: [f32; 3], r0: f32, r1: f32, col: [f32; 3], segs: usize) {
    let ax = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let len = (ax[0] * ax[0] + ax[1] * ax[1] + ax[2] * ax[2]).sqrt();
    if len < 1e-5 {
        return;
    }
    let ax = [ax[0] / len, ax[1] / len, ax[2] / len];
    let helper = if ax[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
    let cross = |u: [f32; 3], v: [f32; 3]| [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let mut u = cross(ax, helper);
    let ul = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
    u = [u[0] / ul, u[1] / ul, u[2] / ul];
    let w = cross(ax, u);
    let ring = |k: usize, base: [f32; 3], r: f32| {
        let t = k as f32 * std::f32::consts::TAU / segs as f32;
        let (s, c) = t.sin_cos();
        [base[0] + (u[0] * c + w[0] * s) * r, base[1] + (u[1] * c + w[1] * s) * r, base[2] + (u[2] * c + w[2] * s) * r]
    };
    for k in 0..segs {
        let (p0, p1, p2, p3) = (ring(k, a, r0), ring(k + 1, a, r0), ring(k + 1, b, r1), ring(k, b, r1));
        let tm = (k as f32 + 0.5) * std::f32::consts::TAU / segs as f32;
        let (s, c) = tm.sin_cos();
        let n = [u[0] * c + w[0] * s, u[1] * c + w[1] * s, u[2] * c + w[2] * s];
        let l = shade(n);
        tri(out, [p0, p1, p2], l, col, 0.0);
        tri(out, [p0, p2, p3], l, col, 0.0);
        tri(out, [a, p1, p0], shade([-ax[0], -ax[1], -ax[2]]), col, 0.0);
        tri(out, [b, p3, p2], shade(ax), col, 0.0);
    }
}

/// A rough ellipsoid centred on `c` with radii `r`. `wob` pushes the surface in and out per vertex.
#[allow(clippy::too_many_arguments)]
pub fn sphere(out: &mut Vec<Vertex>, c: [f32; 3], r: [f32; 3], col: [f32; 3], nu: usize, nv: usize, wob: f32, seed: u32) {
    let pt = |i: usize, j: usize| -> ([f32; 3], [f32; 3]) {
        let (th, ph) = (i as f32 * std::f32::consts::TAU / nu as f32, j as f32 * std::f32::consts::PI / nv as f32);
        let d = [ph.sin() * th.cos(), ph.cos(), ph.sin() * th.sin()];
        let k = if j == 0 || j == nv { 1.0 } else { 1.0 + wob * hash(seed, (i % nu) as u32, j as u32) };
        ([c[0] + d[0] * r[0] * k, c[1] + d[1] * r[1] * k, c[2] + d[2] * r[2] * k], d)
    };
    for j in 0..nv {
        for i in 0..nu {
            let ((a, da), (b, db), (cc, dc), (d, dd)) = (pt(i, j), pt(i + 1, j), pt(i + 1, j + 1), pt(i, j + 1));
            let nrm = |ds: &[[f32; 3]]| {
                let mut s = [0.0f32; 3];
                for x in ds {
                    for k in 0..3 {
                        s[k] += x[k] / r[k];
                    }
                }
                s
            };
            if j != 0 {
                tri(out, [a, cc, b], shade(nrm(&[da, db, dc])), col, 0.0);
            }
            if j != nv - 1 {
                tri(out, [a, d, cc], shade(nrm(&[da, dc, dd])), col, 0.0);
            }
        }
    }
}

/// An ellipsoid with its own axes (half-lengths included): `axes[1]` runs pole to pole. The axes must be
/// perpendicular; this lets a body part turn with the creature instead of staying lined up with the world.
pub fn ellipsoid(out: &mut Vec<Vertex>, c: [f32; 3], axes: [[f32; 3]; 3], col: [f32; 3], nu: usize, nv: usize) {
    let l2 = axes.map(|a| (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).max(1e-9));
    let pt = |i: usize, j: usize| -> ([f32; 3], [f32; 3]) {
        let (th, ph) = (i as f32 * std::f32::consts::TAU / nu as f32, j as f32 * std::f32::consts::PI / nv as f32);
        let d = [ph.sin() * th.cos(), ph.cos(), ph.sin() * th.sin()];
        let mut p = c;
        let mut n = [0.0f32; 3];
        for k in 0..3 {
            for a in 0..3 {
                p[k] += axes[a][k] * d[a];
                n[k] += axes[a][k] * d[a] / l2[a];
            }
        }
        (p, n)
    };
    for j in 0..nv {
        for i in 0..nu {
            let ((a, na), (b, nb), (cc, nc), (d, nd)) = (pt(i, j), pt(i + 1, j), pt(i + 1, j + 1), pt(i, j + 1));
            let sum = |x: [[f32; 3]; 3]| [x[0][0] + x[1][0] + x[2][0], x[0][1] + x[1][1] + x[2][1], x[0][2] + x[1][2] + x[2][2]];
            if j != 0 {
                tri(out, [a, cc, b], shade(sum([na, nb, nc])), col, 0.0);
            }
            if j != nv - 1 {
                tri(out, [a, d, cc], shade(sum([na, nc, nd])), col, 0.0);
            }
        }
    }
}

pub fn rock(out: &mut Vec<Vertex>, c: [f32; 3], r: [f32; 3], col: [f32; 3], seed: u32) {
    sphere(out, c, r, col, 7, 4, 0.3, seed);
}
