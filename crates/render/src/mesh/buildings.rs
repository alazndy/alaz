use super::*;

/// A box with a flat light level per face (top lit, sides in between, bottom dark).
pub(crate) fn push_box(out: &mut Mesh, base: [f32; 3], half: [f32; 3], col: [f32; 3], glow: f32) {
    let [cx, cy, cz] = base;
    let [hx, h, hz] = half;
    let p = |sx: f32, sy: f32, sz: f32| [cx + sx * hx, cy + sy * h, cz + sz * hz];
    let faces: [([[f32; 3]; 4], f32); 6] = [
        ([p(-1., 1., -1.), p(1., 1., -1.), p(1., 1., 1.), p(-1., 1., 1.)], 1.0),
        ([p(-1., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(-1., 1., 1.)], 0.7),
        ([p(1., 0., -1.), p(-1., 0., -1.), p(-1., 1., -1.), p(1., 1., -1.)], 0.5),
        ([p(1., 0., 1.), p(1., 0., -1.), p(1., 1., -1.), p(1., 1., 1.)], 0.64),
        ([p(-1., 0., -1.), p(-1., 0., 1.), p(-1., 1., 1.), p(-1., 1., -1.)], 0.48),
        ([p(-1., 0., -1.), p(1., 0., -1.), p(1., 0., 1.), p(-1., 0., 1.)], 0.3),
    ];
    for (q, k) in faces {
        for i in [0, 1, 2, 0, 2, 3] {
            out.push(out.vl(q[i], col, glow, k));
        }
    }
}

/// Which numbered variant of a model stands at (x, z): fixed by the place, so it does not change from frame to frame.
/// (The placement preview always shows the first.)
pub(crate) fn site_seed(x: f32, z: f32, tint: Option<[f32; 3]>) -> u32 {
    if tint.is_some() {
        return 0;
    }
    ((x * 100.0) as i32 as u32).wrapping_mul(0x9E37_79B1) ^ ((z * 100.0) as i32 as u32).wrapping_mul(0x85EB_CA6B)
}

/// Make what was drawn since `start` shine by itself (flames, embers, sparks).
pub(crate) fn glowify(out: &mut [Vertex], start: usize) {
    for v in &mut out[start..] {
        v.glow = 1.0;
        v.uv[1] = 0.0;
    }
}

/// A campfire: a ring of stones round an ash bed, a teepee of logs that sinks into a heap as the wood burns down,
/// and layered tongues of flame (red outside, yellow, a white heart) that lick and sway, with sparks rising and a
/// little smoke. The flames grow with the fuel left; a dead fire is charred wood smouldering in grey ash.
/// `tint` draws the whole thing in one flat colour (placement preview).
pub(crate) fn push_campfire(out: &mut Mesh, x: f32, z: f32, fuel: f32, t: f32, tint: Option<[f32; 3]>) {
    use shapes::{limb, sphere};
    // a model file stands in for the whole fire (its `lit_` parts show while there is fuel)
    if let Some(m) = assets::model("campfire", site_seed(x, z, tint)) {
        m.place(out, &assets::Place::at(x, z).working(fuel > 0.0, (t * 60.0) as u64).tinted(tint));
        return;
    }
    out.set_anchor(x, z);
    let fuel = fuel.clamp(0.0, 1.0);
    let lit = fuel > 0.0;
    let col = |c: [f32; 3]| tint.unwrap_or(c);
    let seed = ((x * 7.0 + z * 13.0).abs() as u32).wrapping_mul(2654435761);
    let jit = |k: u32| ((seed.rotate_left(k) % 1000) as f32 / 1000.0 - 0.5) * 2.0;
    // the stone ring, uneven
    for k in 0..9u32 {
        let a = k as f32 * std::f32::consts::TAU / 9.0 + 0.2 * jit(k);
        let r = 0.42 + 0.03 * jit(k + 9);
        let sz = 0.1 + 0.035 * jit(k + 3).abs();
        let g = 0.5 + 0.08 * jit(k + 5);
        sphere(out, [x + a.cos() * r, sz * 0.6, z + a.sin() * r], [sz * 1.2, sz * 0.8, sz], col([g, g * 0.97, g * 0.93]), 7, 4, 0.18, seed ^ k);
    }
    // ash bed, scorched black at the middle
    sphere(out, [x, 0.0, z], [0.36, 0.035, 0.36], col([0.36, 0.34, 0.32]), 12, 3, 0.05, seed);
    sphere(out, [x, 0.012, z], [0.22, 0.03, 0.22], col([0.12, 0.1, 0.09]), 10, 3, 0.05, seed ^ 3);
    // logs: a teepee while there is wood to burn, slumping into a low heap as it goes
    let stand = if lit { 0.25 + 0.75 * fuel.sqrt() } else { 0.0 };
    for k in 0..5u32 {
        let a = k as f32 * std::f32::consts::TAU / 5.0 + 0.5 + 0.15 * jit(k + 20);
        let (ca, sa) = (a.cos(), a.sin());
        let foot = [x + ca * 0.32, 0.05, z + sa * 0.32];
        let tip = [x + ca * (0.08 + 0.12 * (1.0 - stand)), 0.06 + 0.26 * stand, z + sa * (0.08 + 0.12 * (1.0 - stand))];
        let mid = [(foot[0] + tip[0]) * 0.5, (foot[1] + tip[1]) * 0.5, (foot[2] + tip[2]) * 0.5];
        limb(out, foot, mid, 0.05, 0.046, col([0.42, 0.28, 0.16]), 6);
        // burnt black toward the flames
        limb(out, mid, tip, 0.04, 0.026, col([0.22, 0.12, 0.08]), 6);
        if lit && tint.is_none() {
            // glowing coals along the burnt end
            let s = out.len();
            let e = [mid[0] * 0.4 + tip[0] * 0.6, mid[1] * 0.4 + tip[1] * 0.6 + 0.02, mid[2] * 0.4 + tip[2] * 0.6];
            sphere(out, e, [0.03, 0.02, 0.03], [1.0, 0.32 + 0.1 * (t * 3.0 + k as f32).sin(), 0.08], 5, 3, 0.0, k);
            glowify(out, s);
        }
    }
    if tint.is_some() {
        // the preview shows where the flames will be
        let s = out.len();
        limb(out, [x, 0.08, z], [x, 0.6, z], 0.14, 0.02, col([1.0, 0.6, 0.2]), 6);
        glowify(out, s);
        return;
    }
    if !lit {
        // smouldering: a thin grey wisp curling up from the ash
        for k in 0..3u32 {
            let age = (t * 0.35 + k as f32 / 3.0).fract();
            let p = [x + (t * 0.9 + k as f32 * 2.0).sin() * 0.06 * (1.0 + age * 2.0), 0.15 + age * 1.3, z + (t * 0.7 + k as f32).cos() * 0.05];
            let r = 0.04 + 0.08 * age;
            let g = 0.55 + 0.1 * age;
            sphere(out, p, [r, r * 0.8, r], [g, g, g], 5, 3, 0.2, k);
        }
        return;
    }
    let size = 0.45 + 0.55 * fuel.sqrt();
    let s = out.len();
    // a bed of embers under the flames
    for k in 0..6u32 {
        let a = k as f32 * 1.05 + jit(k + 30);
        let r = 0.08 + 0.08 * jit(k + 40).abs();
        let pulse = 0.5 + 0.5 * (t * 4.0 + k as f32 * 1.9).sin();
        sphere(out, [x + a.cos() * r, 0.05, z + a.sin() * r], [0.05, 0.03, 0.05], [1.0, 0.25 + 0.25 * pulse, 0.05], 5, 3, 0.1, k);
    }
    // tongues of flame: each licks up and down at its own pace, the tips swaying
    let tongue = |out: &mut Mesh, k: u32, ring: f32, base_r: f32, h: f32, c: [f32; 3]| {
        let a = k as f32 * 2.399 + jit(k + 50);
        let ph = t * (7.0 + 1.3 * (k % 3) as f32) + k as f32 * 1.7;
        let lick = 0.75 + 0.25 * ph.sin() + 0.12 * (ph * 2.3).sin();
        let hh = h * lick * size;
        let foot = [x + a.cos() * ring, 0.06, z + a.sin() * ring];
        let sway = |f: f32| [(ph * 0.7 + f).sin() * 0.07 * size, (ph * 0.6 + f + 1.0).cos() * 0.07 * size];
        let [s1x, s1z] = sway(0.0);
        let [s2x, s2z] = sway(1.4);
        let mid = [foot[0] * 0.6 + x * 0.4 + s1x * 0.5, 0.06 + hh * 0.45, foot[2] * 0.6 + z * 0.4 + s1z * 0.5];
        let tip = [x + s2x, 0.06 + hh, z + s2z];
        limb(out, foot, mid, base_r * size, base_r * 0.75 * size, c, 6);
        limb(out, mid, tip, base_r * 0.75 * size, 0.008, c, 6);
    };
    for k in 0..7 {
        tongue(out, k, 0.15, 0.1, 0.75, [1.0, 0.34, 0.07]);
    }
    for k in 7..11 {
        tongue(out, k, 0.08, 0.1, 1.0, [1.0, 0.64, 0.14]);
    }
    for k in 11..13 {
        tongue(out, k, 0.02, 0.075, 0.65, [1.0, 0.93, 0.62]);
    }
    // sparks drifting up out of the fire and winking out
    for k in 0..6u32 {
        let age = (t * 0.55 + k as f32 / 6.0 + 0.3 * jit(k + 60)).fract();
        let cyc = (t * 0.55 + k as f32 / 6.0).floor();
        let a = k as f32 * 2.1 + cyc * 1.3;
        let p = [x + a.cos() * 0.25 * age + (t * 3.0 + k as f32).sin() * 0.04, 0.4 + age * 1.8 * size, z + a.sin() * 0.25 * age];
        let r = 0.022 * (1.0 - age);
        if r > 0.004 {
            push_box(out, p, [r, r, r], [1.0, 0.75 - 0.4 * age, 0.25], 1.0);
        }
    }
    glowify(out, s);
    // a faint plume of smoke above the flames
    for k in 0..4u32 {
        let age = (t * 0.22 + k as f32 / 4.0).fract();
        // smoke leans off downwind, the more the stronger it blows
        let [wx, wz, _] = out.wind;
        let p = [x + (t * 0.5 + k as f32 * 1.7).sin() * 0.12 * age + wx * 1.6 * age * age, 0.9 * size + age * 2.2 * (1.0 - 0.4 * (wx * wx + wz * wz).sqrt()), z + (t * 0.4 + k as f32).cos() * 0.1 * age + wz * 1.6 * age * age];
        let r = (0.06 + 0.2 * age) * (1.0 - age * 0.5);
        let g = 0.42 + 0.2 * age;
        sphere(out, p, [r, r * 0.85, r], [g, g, g * 1.02], 6, 4, 0.25, k);
    }
}

/// Fire in the grass on one tile: three low tongues of flame that lick and sway (red outside, a yellow heart), a spark
/// or two rising, and a thin curl of smoke leaning downwind. `age` (0..3) shrinks the flames toward the end.
pub(crate) fn push_grass_fire(out: &mut Mesh, x: f32, z: f32, age: f32, t: f32) {
    use shapes::{limb, sphere};
    out.set_anchor(x, z);
    let seed = ((x * 7.0 + z * 13.0).abs() as u32).wrapping_mul(2654435761);
    let jit = |k: u32| ((seed.rotate_left(k) % 1000) as f32 / 1000.0 - 0.5) * 2.0;
    let size = 1.0 - 0.22 * age;
    let s = out.len();
    for k in 0..3u32 {
        let (ox, oz) = (0.3 * jit(k + 3), 0.3 * jit(k + 11));
        let ph = t * (6.0 + 1.1 * (k % 3) as f32) + k as f32 * 1.9 + jit(k + 17) * 3.0;
        let lick = 0.7 + 0.3 * ph.sin() + 0.12 * (ph * 2.3).sin();
        let h = (0.42 + 0.22 * jit(k + 23).abs()) * lick * size;
        let [wx, wz, _] = out.wind;
        let sway = [(ph * 0.7).sin() * 0.05 + wx * 0.1, (ph * 0.6 + 1.0).cos() * 0.05 + wz * 0.1];
        let foot = [x + ox, 0.03, z + oz];
        let mid = [foot[0] + sway[0] * 0.5, 0.03 + h * 0.5, foot[2] + sway[1] * 0.5];
        let tip = [foot[0] + sway[0], 0.03 + h, foot[2] + sway[1]];
        limb(out, foot, mid, 0.075 * size, 0.055 * size, [1.0, 0.30, 0.06], 5);
        limb(out, mid, tip, 0.055 * size, 0.008, [1.0, 0.42, 0.08], 5);
        if k < 2 {
            let ih = h * 0.6;
            limb(out, foot, [foot[0] + sway[0] * 0.4, 0.03 + ih, foot[2] + sway[1] * 0.4], 0.04 * size, 0.008, [1.0, 0.82, 0.30], 5);
        }
    }
    // a spark lifting off
    let a = (t * 0.7 + jit(29) * 0.5).fract();
    let r = 0.02 * (1.0 - a);
    if r > 0.004 {
        push_box(out, [x + 0.15 * jit(31) + (t * 3.0).sin() * 0.04, 0.3 + a * 1.3, z + 0.15 * jit(37)], [r, r, r], [1.0, 0.75 - 0.4 * a, 0.25], 1.0);
    }
    glowify(out, s);
    // smoke: one grey puff leaning off downwind
    let age_s = (t * 0.25 + (seed % 100) as f32 / 100.0).fract();
    let [wx, wz, _] = out.wind;
    let p = [x + wx * 1.4 * age_s * age_s + (t * 0.5 + jit(5)).sin() * 0.06, 0.5 + age_s * 1.6, z + wz * 1.4 * age_s * age_s];
    let r = (0.05 + 0.16 * age_s) * (1.0 - age_s * 0.5);
    let g = 0.34 + 0.2 * age_s;
    sphere(out, p, [r, r * 0.85, r], [g, g, g * 1.02], 5, 3, 0.25, seed);
}

/// Building models from simple boxes. `tint` turns it into a flat bright preview colour.
pub(crate) fn push_building(out: &mut Mesh, kind: BuildingKind, x: f32, z: f32, active: bool, tick: u64, tint: Option<[f32; 3]>) {
    // a model file for this kind (`models/<kind>.vox|.glb`) replaces the code-made one
    if let Some(m) = assets::model(assets::building_name(kind), site_seed(x, z, tint)) {
        m.place(out, &assets::Place::at(x, z).working(active, tick).tinted(tint));
        return;
    }
    out.set_anchor(x, z);
    fn round(out: &mut Mesh, c: [f32; 3], r: [f32; 3], col: [f32; 3], tint: Option<[f32; 3]>) {
        match tint {
            Some(t) => shapes::sphere(out, c, r, t, 12, 8, 0.0, 11),
            None => shapes::sphere(out, c, r, col, 12, 8, 0.02, 11),
        }
    }
    let bx = |out: &mut Mesh, base: [f32; 3], half: [f32; 3], col: [f32; 3], glow: f32| match tint {
        Some(t) => push_box(out, base, half, t, 1.0),
        None => push_box(out, base, half, col, glow),
    };
    match kind {
        BuildingKind::Campfire => {
            push_campfire(out, x, z, 1.0, tick as f32 / 60.0, tint);
        }
        BuildingKind::Shelter => {
            for (sx, sz) in [(-1.1, -0.9), (1.1, -0.9), (-1.1, 0.9), (1.1, 0.9)] {
                bx(out, [x + sx, 0.0, z + sz], [0.06, 1.3, 0.06], [0.40, 0.27, 0.15], 0.0);
            }
            round(out, [x, 1.4, z], [1.6, 0.28, 1.4], [0.25, 0.40, 0.18], tint);
            bx(out, [x, 0.0, z - 0.95], [1.1, 0.9, 0.05], [0.32, 0.30, 0.16], 0.0);
        }
        BuildingKind::CharcoalKiln => {
            round(out, [x, 0.4, z], [0.9, 0.5, 0.9], [0.25, 0.17, 0.12], tint);
            round(out, [x, 0.8, z], [0.55, 0.3, 0.55], [0.20, 0.14, 0.10], tint);
            if active {
                bx(out, [x, 0.7, z], [0.16, 0.10, 0.16], [1.0, 0.45, 0.10], 1.0);
            }
        }
        BuildingKind::Depot => {
            bx(out, [x, 0.0, z], [0.85, 0.55, 0.85], [0.45, 0.32, 0.18], 0.0);
            bx(out, [x, 0.55, z], [0.95, 0.08, 0.95], [0.34, 0.24, 0.14], 0.0);
            round(out, [x - 0.35, 0.85, z + 0.2], [0.26, 0.28, 0.26], [0.55, 0.42, 0.24], tint);
            round(out, [x + 0.3, 0.82, z - 0.25], [0.24, 0.24, 0.24], [0.60, 0.46, 0.28], tint);
            round(out, [x + 0.35, 0.8, z + 0.35], [0.2, 0.18, 0.2], [0.5, 0.4, 0.3], tint);
        }
        BuildingKind::Workshop => {
            bx(out, [x, 0.0, z], [1.1, 0.9, 0.9], [0.50, 0.46, 0.42], 0.0);
            round(out, [x, 1.0, z], [1.35, 0.32, 1.1], [0.28, 0.22, 0.20], tint);
            bx(out, [x + 0.5, 0.9, z + 0.3], [0.12, 0.7, 0.12], [0.35, 0.33, 0.34], 0.0);
            if active {
                bx(out, [x + 0.5, 1.6, z + 0.3], [0.14, 0.08, 0.14], [1.0, 0.45, 0.10], 1.0);
                bx(out, [x, 0.1, z + 0.92], [0.5, 0.35, 0.03], [1.0, 0.6, 0.2], 1.0);
            }
        }
        BuildingKind::WaterWheel => {
            bx(out, [x, 0.0, z], [0.9, 0.35, 0.5], [0.45, 0.32, 0.18], 0.0);
            // spokes of the wheel, turning with the day clock is overkill: draw it upright
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::TAU / 8.0 + tick as f32 * if active { 0.02 } else { 0.0 };
                shapes::limb(out, [x, 1.05, z], [x + a.cos() * 1.0, 1.05 + a.sin() * 1.0, z], 0.06, 0.09, [0.55, 0.40, 0.22], 6);
            }
            round(out, [x, 1.05, z], [0.18, 0.18, 0.6], [0.35, 0.35, 0.40], tint);
            bx(out, [x, 0.2, z], [0.25, 0.25, 0.7], [0.35, 0.35, 0.40], 0.0);
        }
        BuildingKind::Workbench => {
            bx(out, [x, 0.5, z], [0.9, 0.12, 0.5], [0.52, 0.36, 0.20], 0.0);
            for (dx, dz) in [(-0.38, -0.2), (0.38, -0.2), (-0.38, 0.2), (0.38, 0.2)] {
                bx(out, [x + dx, 0.0, z + dz], [0.08, 0.5, 0.08], [0.40, 0.28, 0.16], 0.0);
            }
            bx(out, [x - 0.2, 0.62, z], [0.2, 0.08, 0.14], [0.62, 0.62, 0.66], 0.0);
            if active {
                bx(out, [x + 0.25, 0.62, z], [0.14, 0.05, 0.14], [1.0, 0.6, 0.2], 1.0);
            }
        }
        BuildingKind::Field => {
            // broken ground in furrows, a stake at each corner; green shoots while a crop grows
            for k in 0..5 {
                let fz = z - 1.0 + k as f32 * 0.5;
                bx(out, [x, 0.0, fz], [1.2, 0.06, 0.16], [0.30, 0.20, 0.12], 0.0);
                if active {
                    for j in 0..7 {
                        let fx = x - 1.05 + j as f32 * 0.35;
                        let h = 0.18 + 0.05 * (((j * 3 + k * 5) % 4) as f32);
                        bx(out, [fx, 0.06, fz], [0.025, h, 0.025], [0.42, 0.66, 0.24], 0.0);
                    }
                }
            }
            for (sx, sz) in [(-1.35, -1.25), (1.35, -1.25), (-1.35, 1.25), (1.35, 1.25)] {
                bx(out, [x + sx, 0.0, z + sz], [0.05, 0.45, 0.05], [0.42, 0.29, 0.16], 0.0);
            }
        }
        BuildingKind::Bloomery => {
            // a tapering clay shaft with a glowing mouth
            round(out, [x, 0.6, z], [0.55, 0.7, 0.55], [0.58, 0.40, 0.30], tint);
            round(out, [x, 1.35, z], [0.32, 0.3, 0.32], [0.55, 0.38, 0.28], tint);
            if active {
                bx(out, [x, 0.1, z + 0.5], [0.16, 0.18, 0.04], [1.0, 0.55, 0.15], 1.0);
            }
        }
        BuildingKind::Bellows => {
            bx(out, [x, 0.0, z], [0.4, 0.12, 0.25], [0.45, 0.31, 0.17], 0.0);
            round(out, [x, 0.25, z], [0.35, 0.12, 0.22], [0.55, 0.40, 0.26], tint);
            shapes::limb(out, [x + 0.35, 0.2, z], [x + 0.9, 0.15, z], 0.05, 0.03, [0.35, 0.33, 0.34], 4);
        }
        BuildingKind::TripHammer => {
            bx(out, [x, 0.0, z], [0.8, 0.5, 0.3], [0.48, 0.33, 0.18], 0.0);
            let lift = if active { ((tick as f32) * 0.2).sin().abs() * 0.3 } else { 0.0 };
            shapes::limb(out, [x - 0.7, 0.9, z], [x + 0.5, 0.75 + lift, z], 0.08, 0.06, [0.50, 0.35, 0.19], 5);
            bx(out, [x + 0.5, 0.5 + lift, z], [0.15, 0.15, 0.15], [0.35, 0.34, 0.36], 0.0);
            bx(out, [x + 0.5, 0.0, z], [0.2, 0.4, 0.2], [0.40, 0.40, 0.42], 0.0);
        }
        BuildingKind::IronAnvil => {
            round(out, [x, 0.25, z], [0.3, 0.28, 0.3], [0.42, 0.29, 0.16], tint);
            bx(out, [x, 0.5, z], [0.42, 0.12, 0.2], [0.30, 0.30, 0.34], 0.0);
            bx(out, [x + 0.45, 0.55, z], [0.12, 0.06, 0.08], [0.30, 0.30, 0.34], 0.0);
        }
        BuildingKind::BlastFurnace => {
            bx(out, [x, 0.0, z], [0.7, 1.2, 0.7], [0.66, 0.56, 0.46], 0.0);
            bx(out, [x, 2.4, z], [0.45, 0.8, 0.45], [0.62, 0.52, 0.42], 0.0);
            if active {
                bx(out, [x, 0.15, z + 0.71], [0.2, 0.2, 0.02], [1.0, 0.6, 0.2], 1.0);
            }
        }
        BuildingKind::SteelAnvil => {
            round(out, [x, 0.25, z], [0.3, 0.28, 0.3], [0.42, 0.29, 0.16], tint);
            bx(out, [x, 0.5, z], [0.44, 0.12, 0.2], [0.62, 0.66, 0.72], 0.0);
        }
        BuildingKind::Windmill => {
            bx(out, [x, 0.0, z], [0.5, 1.6, 0.5], [0.52, 0.40, 0.26], 0.0);
            for k in 0..4 {
                let a = k as f32 * std::f32::consts::TAU / 4.0 + tick as f32 * 0.02;
                shapes::limb(out, [x, 2.8, z + 0.55], [x + a.cos() * 1.8, 2.8 + a.sin() * 1.8, z + 0.55], 0.05, 0.05, [0.85, 0.82, 0.70], 4);
            }
        }
        BuildingKind::Conveyor => {
            bx(out, [x, 0.0, z], [1.2, 0.35, 0.3], [0.45, 0.31, 0.17], 0.0);
            bx(out, [x, 0.35, z], [1.2, 0.03, 0.26], [0.30, 0.26, 0.22], 0.0);
        }
        BuildingKind::SteamEngine => {
            // a brick firebox, a round boiler on it, a stack, a flywheel
            bx(out, [x, 0.0, z], [0.8, 0.4, 0.5], [0.66, 0.56, 0.46], 0.0);
            round(out, [x, 0.75, z], [0.8, 0.35, 0.35], [0.30, 0.30, 0.34], tint);
            bx(out, [x - 0.6, 1.0, z], [0.1, 0.9, 0.1], [0.25, 0.25, 0.28], 0.0);
            let a = tick as f32 * if active { 0.08 } else { 0.0 };
            for k in 0..6 {
                let t = a + k as f32 * std::f32::consts::TAU / 6.0;
                shapes::limb(out, [x + 0.9, 0.8, z + 0.5], [x + 0.9 + t.cos() * 0.6, 0.8 + t.sin() * 0.6, z + 0.5], 0.04, 0.04, [0.30, 0.30, 0.34], 4);
            }
            if active {
                bx(out, [x, 0.1, z + 0.51], [0.2, 0.15, 0.02], [1.0, 0.5, 0.15], 1.0);
            }
        }
        BuildingKind::Pump => {
            bx(out, [x, 0.0, z], [0.3, 0.8, 0.3], [0.30, 0.30, 0.34], 0.0);
            let lift = if active { ((tick as f32) * 0.1).sin() * 0.2 } else { 0.0 };
            shapes::limb(out, [x - 0.8, 1.2 + lift, z], [x + 0.8, 1.2 - lift, z], 0.06, 0.06, [0.45, 0.31, 0.17], 4);
        }
        BuildingKind::RollingMill => {
            bx(out, [x, 0.0, z], [0.8, 0.45, 0.4], [0.30, 0.30, 0.34], 0.0);
            round(out, [x, 0.6, z], [0.7, 0.14, 0.14], [0.55, 0.56, 0.60], tint);
            round(out, [x, 0.85, z], [0.7, 0.14, 0.14], [0.55, 0.56, 0.60], tint);
        }
        BuildingKind::Dynamo => {
            bx(out, [x, 0.0, z], [0.6, 0.15, 0.4], [0.30, 0.30, 0.34], 0.0);
            round(out, [x, 0.5, z], [0.4, 0.35, 0.35], [0.70, 0.42, 0.24], tint);
        }
        BuildingKind::Pole => {
            bx(out, [x, 0.0, z], [0.06, 1.8, 0.06], [0.42, 0.29, 0.16], 0.0);
            bx(out, [x, 3.5, z], [0.5, 0.04, 0.04], [0.42, 0.29, 0.16], 0.0);
        }
        BuildingKind::ElectricMotor => {
            round(out, [x, 0.35, z], [0.35, 0.3, 0.3], [0.30, 0.34, 0.40], tint);
            shapes::limb(out, [x + 0.35, 0.35, z], [x + 0.7, 0.35, z], 0.05, 0.05, [0.55, 0.56, 0.60], 4);
        }
        BuildingKind::ElectricLamp => {
            bx(out, [x, 0.0, z], [0.05, 1.2, 0.05], [0.30, 0.30, 0.34], 0.0);
            round(out, [x, 2.5, z], [0.18, 0.2, 0.18], [1.0, 0.95, 0.7], tint);
        }
        BuildingKind::Computer => {
            bx(out, [x, 0.0, z], [0.6, 0.45, 0.4], [0.80, 0.78, 0.72], 0.0);
            bx(out, [x, 0.9, z], [0.45, 0.35, 0.05], [0.20, 0.30, 0.25], 0.0);
            bx(out, [x, 0.95, z + 0.06], [0.38, 0.28, 0.01], [0.35, 0.9, 0.5], 0.8);
        }
        BuildingKind::Ruin => {
            // tumbled blocks, half sunk, moss on them
            for (dx, dz, h) in [(-0.5f32, -0.3f32, 0.4f32), (0.4, -0.4, 0.25), (0.2, 0.5, 0.55), (-0.4, 0.4, 0.2)] {
                round(out, [x + dx, h * 0.5, z + dz], [0.35, h, 0.3], [0.48, 0.48, 0.44], tint);
            }
            round(out, [x, 0.05, z], [0.9, 0.06, 0.9], [0.30, 0.42, 0.22], tint);
        }
        BuildingKind::Cellar => {
            round(out, [x, 0.2, z], [0.9, 0.35, 0.9], [0.32, 0.44, 0.24], tint);
            bx(out, [x, 0.0, z + 0.85], [0.35, 0.45, 0.06], [0.40, 0.28, 0.16], 0.0);
        }
        BuildingKind::MineShaft => {
            for (dx, dz) in [(-0.6f32, -0.6f32), (0.6, -0.6), (-0.6, 0.6), (0.6, 0.6)] {
                bx(out, [x + dx, 0.0, z + dz], [0.06, 1.2, 0.06], [0.42, 0.29, 0.16], 0.0);
            }
            bx(out, [x, 2.4, z], [0.7, 0.06, 0.7], [0.45, 0.31, 0.17], 0.0);
            round(out, [x, 0.02, z], [0.5, 0.02, 0.5], [0.08, 0.07, 0.07], tint);
        }
        BuildingKind::Anvil => {
            // a dressed block of stone on a stump
            round(out, [x, 0.25, z], [0.3, 0.28, 0.3], [0.42, 0.29, 0.16], tint);
            bx(out, [x, 0.5, z], [0.38, 0.12, 0.22], [0.48, 0.47, 0.50], 0.0);
        }
        BuildingKind::HandMill => {
            // two round stones, the upper one turning, on a plank stand
            bx(out, [x, 0.0, z], [0.6, 0.35, 0.6], [0.52, 0.36, 0.20], 0.0);
            round(out, [x, 0.42, z], [0.5, 0.1, 0.5], [0.55, 0.54, 0.52], tint);
            let a = tick as f32 * if active { 0.05 } else { 0.0 };
            round(out, [x, 0.58, z], [0.48, 0.09, 0.48], [0.62, 0.61, 0.58], tint);
            shapes::limb(out, [x + a.cos() * 0.35, 0.66, z + a.sin() * 0.35], [x + a.cos() * 0.35, 0.95, z + a.sin() * 0.35], 0.04, 0.04, [0.42, 0.29, 0.16], 4);
        }
        BuildingKind::AnimalGin => {
            // an upright post and a long sweep
            bx(out, [x, 0.0, z], [0.12, 1.1, 0.12], [0.45, 0.31, 0.17], 0.0);
            let a = tick as f32 * 0.01;
            shapes::limb(out, [x, 0.95, z], [x + a.cos() * 2.4, 0.9, z + a.sin() * 2.4], 0.06, 0.05, [0.50, 0.35, 0.19], 5);
        }
        BuildingKind::PitKiln => {
            // a shallow pit ringed with earth, a mound of straw and logs over the pots; smoke and a glow while firing
            round(out, [x, 0.05, z], [0.95, 0.12, 0.95], [0.30, 0.22, 0.14], tint);
            round(out, [x, 0.25, z], [0.7, 0.28, 0.7], [0.70, 0.62, 0.34], tint);
            for k in 0..5 {
                let a = k as f32 * std::f32::consts::TAU / 5.0;
                shapes::limb(out, [x + a.cos() * 0.75, 0.15, z + a.sin() * 0.75], [x + a.cos() * 0.1, 0.55, z + a.sin() * 0.1], 0.07, 0.06, [0.42, 0.29, 0.16], 5);
            }
            if active {
                bx(out, [x, 0.42, z], [0.22, 0.06, 0.22], [1.0, 0.45, 0.10], 1.0);
            }
        }
        BuildingKind::Pen => {
            // a ring of posts with two rails, open on the near side for the gate, a trough in the middle
            let r = 3.2f32;
            let n = 16;
            for k in 0..n {
                if k == 4 {
                    continue; // the gate
                }
                let (a0, a1) = (k as f32 / n as f32 * std::f32::consts::TAU, (k + 1) as f32 / n as f32 * std::f32::consts::TAU);
                let (p0, p1) = ([x + r * a0.cos(), z + r * a0.sin()], [x + r * a1.cos(), z + r * a1.sin()]);
                bx(out, [p0[0], 0.0, p0[1]], [0.05, 0.55, 0.05], [0.42, 0.29, 0.16], 0.0);
                if k != 3 {
                    for h in [0.3f32, 0.75] {
                        shapes::limb(out, [p0[0], h, p0[1]], [p1[0], h, p1[1]], 0.035, 0.035, [0.50, 0.35, 0.19], 4);
                    }
                }
            }
            bx(out, [x, 0.0, z], [0.5, 0.18, 0.22], [0.40, 0.28, 0.16], 0.0);
            if active {
                bx(out, [x, 0.18, z], [0.42, 0.04, 0.16], [0.86, 0.72, 0.36], 0.0);
            }
        }
        BuildingKind::Loom => {
            // a warp-weighted loom: two posts, a beam across the top, threads hanging to clay weights
            for sx in [-0.7f32, 0.7] {
                bx(out, [x + sx, 0.0, z], [0.06, 1.0, 0.06], [0.42, 0.29, 0.16], 0.0);
            }
            bx(out, [x, 1.9, z], [0.82, 0.06, 0.07], [0.48, 0.33, 0.18], 0.0);
            for k in 0..9 {
                let tx = x - 0.56 + k as f32 * 0.14;
                bx(out, [tx, 0.45, z], [0.012, 0.72, 0.012], [0.82, 0.80, 0.66], 0.0);
                round(out, [tx, 0.4, z], [0.06, 0.07, 0.06], [0.62, 0.38, 0.26], tint);
            }
            // the woven part grows down from the beam while it works
            let woven = if active { 0.5 } else { 0.25 };
            bx(out, [x, 1.84 - 2.0 * woven, z + 0.01], [0.6, woven, 0.02], [0.78, 0.72, 0.52], 0.0);
        }
        // the industrial machines have their own file (moving parts, smoke)
        BuildingKind::CokeOven
        | BuildingKind::LimeKiln
        | BuildingKind::CementKiln
        | BuildingKind::BrickKiln
        | BuildingKind::PuddlingFurnace
        | BuildingKind::Bessemer
        | BuildingKind::Cupola
        | BuildingKind::MoldingBench
        | BuildingKind::Lathe
        | BuildingKind::DrillMill
        | BuildingKind::Press
        | BuildingKind::Sawmill
        | BuildingKind::SpinningWheel
        | BuildingKind::PowerLoom
        | BuildingKind::Boiler
        | BuildingKind::SteamHammer
        | BuildingKind::RollingLine
        | BuildingKind::PotterWheel
        | BuildingKind::OilPress
        | BuildingKind::Tannery
        | BuildingKind::DryingRack
        | BuildingKind::Smokehouse
        | BuildingKind::Well
        | BuildingKind::StoneMortar
        | BuildingKind::WaterMill
        | BuildingKind::ForgeHammer
        | BuildingKind::FeltMallet
        | BuildingKind::Ropewalk
        | BuildingKind::Crane
        | BuildingKind::GlassFurnace
        | BuildingKind::PaperVat
        | BuildingKind::PrintingPress
        | BuildingKind::CottonGin
        | BuildingKind::SpinningJenny
        => push_machine(out, kind, x, z, active, tick, tint),
        BuildingKind::Furnace => {
            round(out, [x, 0.55, z], [0.7, 0.62, 0.7], [0.55, 0.53, 0.50], tint);
            round(out, [x, 1.0, z], [0.45, 0.35, 0.45], [0.5, 0.48, 0.46], tint);
            bx(out, [x, 0.0, z + 0.6], [0.25, 0.35, 0.04], [0.08, 0.07, 0.07], 0.0);
            if active {
                bx(out, [x, 1.0, z], [0.25, 0.10, 0.25], [1.0, 0.45, 0.10], 1.0);
                bx(out, [x, 0.0, z + 0.62], [0.2, 0.3, 0.03], [1.0, 0.5, 0.15], 1.0);
            }
        }
    }
}
