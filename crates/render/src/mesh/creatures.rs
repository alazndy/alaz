use super::DetMath;
use super::*;

pub(crate) struct Quad {
    pub(crate) body: [f32; 3],
    pub(crate) head: [f32; 3],
    pub(crate) len: f32,
    pub(crate) leg_h: f32,
    pub(crate) thick: f32,
    pub(crate) head_r: f32,
    pub(crate) neck: f32,
    pub(crate) tail: bool,
    pub(crate) horns: bool,
}

/// A four-legged animal built from rounded parts; legs swing with the walk phase.
pub(crate) fn push_quad(out: &mut Mesh, x: f32, z: f32, pose: Pose, q: Quad) {
    out.set_anchor(x, z);
    use shapes::{limb, sphere};
    let (s, c) = pose.heading.dsin_cos();
    let (fx, fz, rx, rz) = (s, c, c, -s);
    let at = |f: f32, r: f32, h: f32| [x + fx * f + rx * r, h, z + fz * f + rz * r];
    let bob = pose.phase.dsin().abs() * 0.04 * pose.amp + (pose.t * 1.6).dsin() * 0.006 * (1.0 - pose.amp);
    let top = q.leg_h + q.thick + bob;
    let dark = [q.body[0] * 0.7, q.body[1] * 0.7, q.body[2] * 0.7];
    for (fi, f) in [q.len * 0.7, -q.len * 0.7].into_iter().enumerate() {
        for (si, side) in [-1.0f32, 1.0].into_iter().enumerate() {
            let ph = if (fi + si) % 2 == 0 { 1.0 } else { -1.0 };
            let (cycle, raise) = foot_cycle(pose.phase + if ph > 0.0 { 0.0 } else { std::f32::consts::PI });
            let off = cycle * pose.stride * pose.amp.min(1.0);
            let hip = at(f, side * q.thick * 0.6, q.leg_h + q.thick * 0.6 + bob);
            // only the swinging foot leaves the ground
            let foot = at(f + off, side * q.thick * 0.6, (raise * pose.amp.min(1.0) * pose.stride * 0.35).min(0.2));
            limb(out, hip, foot, q.thick * 0.28, q.thick * 0.18, dark, 6);
        }
    }
    limb(out, at(-q.len, 0.0, top - q.thick * 0.2), at(q.len, 0.0, top - q.thick * 0.1), q.thick * 0.95, q.thick * 0.85, q.body, 10);
    // standing animals put their heads down to graze now and then, and look around in between
    let still = (1.0 - pose.amp * 4.0).clamp(0.0, 1.0);
    let k = pose.id as f32 * 1.7;
    let graze = ((pose.t * 0.33 + k).dsin() * 1.8 - 0.5).clamp(0.0, 1.0) * still;
    let look = (pose.t * 0.7 + k * 0.5).dsin() * 0.25 * still * (1.0 - graze);
    let hy = top + q.neck * 0.5 + (q.head_r * 1.1 - top - q.neck * 0.5) * graze;
    let hf = q.len + q.neck * (0.5 + 0.25 * graze);
    let head_at = at(hf, look * q.neck, hy);
    limb(out, at(q.len * 0.8, 0.0, top), head_at, q.thick * 0.55, q.head_r * 0.9, q.body, 8);
    sphere(out, head_at, [q.head_r; 3], q.head, 10, 8, 0.0, 7);
    let snout = at(hf + q.head_r * 1.3, look * q.neck, hy - q.head_r * (0.4 + 0.5 * graze));
    sphere(out, snout, [q.head_r * 0.6; 3], q.head, 8, 6, 0.0, 8);
    if q.horns {
        for side in [-1.0f32, 1.0] {
            limb(out, at(hf, look * q.neck + side * q.head_r * 0.8, hy + q.head_r * 0.6), at(hf - q.head_r * 0.4 * graze, look * q.neck + side * q.head_r * 2.0, hy + q.head_r * 1.8), 0.05, 0.015, [0.9, 0.86, 0.75], 6);
        }
    }
    if q.tail {
        // the tail swishes, quicker while walking
        let wag = (pose.t * (3.0 + 4.0 * pose.amp) + k).dsin() * (0.06 + 0.06 * pose.amp);
        limb(out, at(-q.len, 0.0, top), at(-q.len - 0.18 - q.head_r, wag, top - 0.1 - 0.08 * still), 0.05, 0.02, dark, 6);
    }
}

pub(crate) fn push_creature(out: &mut Mesh, kind: AgentKind, x: f32, z: f32, marked: bool, pose: Pose) {
    out.set_anchor(x, z);
    use shapes::{limb, sphere};
    let height = match kind {
        AgentKind::Human => {
            push_human(out, x, z, pose);
            1.9
        }
        AgentKind::Dog => {
            push_quad(out, x, z, pose, Quad { body: [0.58, 0.42, 0.28], head: [0.62, 0.46, 0.32], len: 0.28, leg_h: 0.22, thick: 0.15, head_r: 0.11, neck: 0.18, tail: true, horns: false });
            0.6
        }
        AgentKind::Ox => {
            push_quad(out, x, z, pose, Quad { body: [0.30, 0.20, 0.14], head: [0.34, 0.23, 0.16], len: 0.55, leg_h: 0.45, thick: 0.40, head_r: 0.2, neck: 0.5, tail: true, horns: true });
            1.4
        }
        AgentKind::Horse => {
            push_quad(out, x, z, pose, Quad { body: [0.62, 0.40, 0.22], head: [0.58, 0.37, 0.20], len: 0.5, leg_h: 0.7, thick: 0.28, head_r: 0.15, neck: 0.62, tail: true, horns: false });
            1.7
        }
        AgentKind::Bot => {
            sphere(out, [x, 0.5, z], [0.3, 0.42, 0.3], [0.62, 0.66, 0.72], 12, 8, 0.0, 3);
            sphere(out, [x, 1.05, z], [0.22, 0.22, 0.22], [0.72, 0.76, 0.82], 12, 8, 0.0, 4);
            let (fx, fz) = (pose.heading.dsin(), pose.heading.dcos());
            sphere(out, [x + fx * 0.17, 1.08, z + fz * 0.17], [0.1, 0.045, 0.05], [0.3, 0.9, 1.0], 8, 4, 0.0, 5);
            limb(out, [x, 0.0, z], [x, 0.18, z], 0.2, 0.28, [0.4, 0.42, 0.46], 8);
            1.4
        }
    };
    if marked {
        push_box(out, [x, height + 0.12, z], [0.07, 0.07, 0.07], [0.3, 0.9, 0.9], 1.0);
    }
}

/// Leg length of a wild species (metres): sets its stride, and how far it has to fall and sink when it dies.
pub(crate) fn wild_leg(s: alaz_sim::WildSpecies) -> f32 {
    use alaz_sim::WildSpecies as S;
    match s {
        S::Dog => 0.22,
        S::Ox => 0.45,
        S::Horse => 0.7,
        S::Wolf => 0.35,
        S::Deer => 0.65,
    }
}

/// Half the body's width (metres): a falling animal tips over the line of its feet on one side.
fn wild_half_width(s: alaz_sim::WildSpecies) -> f32 {
    use alaz_sim::WildSpecies as S;
    match s {
        S::Dog => 0.15,
        S::Ox => 0.40,
        S::Horse => 0.28,
        S::Wolf | S::Deer => 0.2,
    }
}

/// A wild animal in the given pose.
pub(crate) fn push_wild(out: &mut Mesh, s: alaz_sim::WildSpecies, x: f32, z: f32, pose: Pose) {
    use alaz_sim::WildSpecies as S;
    match s {
        S::Dog => push_creature(out, AgentKind::Dog, x, z, false, pose),
        S::Ox => push_creature(out, AgentKind::Ox, x, z, false, pose),
        S::Horse => push_creature(out, AgentKind::Horse, x, z, false, pose),
        S::Wolf => {
            push_quad(out, x, z, pose, Quad { body: [0.32, 0.32, 0.35], head: [0.36, 0.36, 0.40], len: 0.4, leg_h: 0.35, thick: 0.2, head_r: 0.13, neck: 0.25, tail: true, horns: false });
        }
        S::Deer => {
            push_quad(out, x, z, pose, Quad { body: [0.66, 0.46, 0.26], head: [0.70, 0.50, 0.30], len: 0.42, leg_h: 0.65, thick: 0.2, head_r: 0.11, neck: 0.5, tail: true, horns: true });
        }
    }
}

/// Seconds a killed animal takes to fall, lie still and sink out of sight.
pub(crate) const FALL_SECS: f32 = 0.5;
pub(crate) const LIE_UNTIL: f32 = 3.0;
pub(crate) const DEATH_SECS: f32 = 4.2;

/// How far over a killed animal has tipped (0 standing .. 1 on its side) and how deep it has sunk (metres), `age`
/// seconds after the blow. It drops like a weight (slow start, fast end), bounces once, lies still, then sinks.
pub(crate) fn death_pose(s: alaz_sim::WildSpecies, age: f32) -> (f32, f32) {
    let tip = if age < FALL_SECS {
        let t = age / FALL_SECS;
        t * t
    } else if age < FALL_SECS + 0.2 {
        1.0 - 0.09 * (std::f32::consts::PI * (age - FALL_SECS) / 0.2).dsin()
    } else {
        1.0
    };
    let depth = wild_leg(s) + 2.2 * wild_half_width(s);
    let sink = ((age - LIE_UNTIL) / (DEATH_SECS - LIE_UNTIL)).clamp(0.0, 1.0) * depth;
    (tip, sink)
}

/// Tip the vertices from `start` on over onto one side (`side` = 1 or -1 of the body's right) and sink them. The body
/// turns about the line under its feet on that side, so it comes to rest lying on the ground, not through it.
pub(crate) fn fall_over(out: &mut [Vertex], start: usize, s: alaz_sim::WildSpecies, x: f32, z: f32, heading: f32, side: f32, tip: f32, sink: f32) {
    let (sh, ch) = heading.dsin_cos();
    let (fx, fz, rx, rz) = (sh, ch, ch, -sh);
    let (st, ct) = (tip * std::f32::consts::FRAC_PI_2).dsin_cos();
    let pivot = side * wild_half_width(s);
    for v in &mut out[start..] {
        let (dx, dz) = (v.pos[0] - x, v.pos[2] - z);
        let (a, b, y) = (dx * rx + dz * rz - pivot, dx * fx + dz * fz, v.pos[1]);
        let a2 = a * ct + side * y * st;
        let y2 = -side * a * st + y * ct;
        let a2 = a2 + pivot;
        v.pos = [x + rx * a2 + fx * b, y2 - sink, z + rz * a2 + fz * b];
    }
}

/// A short trail of paw prints behind an animal.
/// Used as a fixed mesh-hash fixture for the mark geometry and both colors.
#[cfg(test)]
pub(crate) fn push_tracks(out: &mut Mesh, id: u32, x: f32, z: f32, heading: (i64, i64), wolf: bool) {
    out.set_anchor(x, z);
    let (hx, hz) = (heading.0 as f32, heading.1 as f32);
    let len = (hx * hx + hz * hz).sqrt();
    // behind a walking animal; a resting one shows a fixed trail
    let (dx, dz) = if len > 100.0 {
        (-hx / len, -hz / len)
    } else {
        let a = (id.wrapping_mul(2654435761) >> 8) as f32 * 0.0001;
        (a.dcos(), a.dsin())
    };
    for i in 1..=6 {
        let d = 0.9 + i as f32 * 0.8;
        let side = if i % 2 == 0 { 0.18 } else { -0.18 };
        let (px, pz) = (x + dx * d - dz * side, z + dz * d + dx * side);
        push_paw_mark(out, px, pz, wolf);
    }
}

/// Paint one paw mark. The caller sets the terrain anchor before drawing.
pub(crate) fn push_paw_mark(out: &mut Mesh, x: f32, z: f32, wolf: bool) {
    let col = if wolf { [0.20, 0.17, 0.15] } else { [0.30, 0.25, 0.19] };
    push_box(out, [x, 0.015, z], [0.07, 0.008, 0.09], col, 0.0);
}
