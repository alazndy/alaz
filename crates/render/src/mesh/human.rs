use super::*;

/// Chips, sparks and splashes thrown off by a blow, flying out from where it landed and falling back.
pub(crate) fn push_chips(out: &mut Mesh, work: Work, target: (f32, f32), me: (f32, f32), cyc: f32, tool: Tool) {
    let u = cyc.fract();
    let age = (u - IMPACT) / 0.3;
    if !(0.0..1.0).contains(&age) {
        return;
    }
    // the face of the thing that was hit, toward the worker
    let (dx, dz) = (me.0 - target.0, me.1 - target.1);
    let d = dx.hypot(dz).max(1e-3);
    let (nx, nz) = (dx / d, dz / d);
    let (spot, y0, col, glow, n, speed) = match work {
        Work::Chop => (0.28, 1.0, [0.78, 0.6, 0.36], 0.0, 6, 1.6),
        Work::Mine => (0.45, 0.35, [0.62, 0.62, 0.64], 0.0, 6, 1.8),
        _ => return,
    };
    out.set_anchor(target.0, target.1);
    let seed = cyc.floor() as i32;
    let t = age * 0.45;
    for i in 0..n {
        let h = (alaz_sim::rng::hash2(0xC41F, seed, i) % 10_000) as f32 / 10_000.0;
        let a = (h - 0.5) * 2.4;
        let (s, c) = a.sin_cos();
        // fan out toward the worker and to the sides
        let (vx, vz) = ((nx * c - nz * s) * speed, (nz * c + nx * s) * speed);
        let vy = 1.2 + 1.6 * h;
        let p = [target.0 + nx * spot + vx * t, (y0 + vy * t - 4.9 * t * t).max(0.03), target.1 + nz * spot + vz * t];
        let r = 0.035 + 0.02 * (i % 3) as f32;
        push_box(out, p, [r, r * 0.8, r], col, glow);
    }
    // a pick of metal on stone strikes bright sparks
    if work == Work::Mine && tool == Tool::MetalPick && age < 0.4 {
        for i in 0..5 {
            let a = i as f32 * 1.26 + seed as f32;
            let k = age / 0.4 * 0.7;
            push_box(out, [target.0 + nx * spot + a.cos() * k, y0 + 0.1 + k * 0.6, target.1 + nz * spot + a.sin() * k], [0.025; 3], [1.0, 0.85, 0.4], 1.0);
        }
    }
}

/// Clothes worn, as bits of `Pose::outfit`.
pub const OUT_WRAP: u16 = 1;
pub const OUT_CLOAK: u16 = 2;
pub const OUT_BOOTS: u16 = 4;
pub const OUT_TUNIC: u16 = 8;
pub const OUT_HOOD: u16 = 16;
pub const OUT_PLATE: u16 = 32;
pub const OUT_HELM: u16 = 64;
pub const OUT_APRON: u16 = 128;

/// A person's looks, fixed by the id: skin, hair, dye, build.
pub struct Look {
    pub skin: [f32; 3],
    pub hair: [f32; 3],
    /// Hair cut 0..6 (3 shaggy, 4 braid, 5 cropped).
    pub style: usize,
    pub dye: [f32; 3],
    /// Shoulder and hip width factors.
    pub frame: (f32, f32),
    pub beard: bool,
    pub braid_side: f32,
}

/// The looks of the person with this id; the body and the portrait both read them.
pub fn look(id: u32) -> Look {
    // every trait draws on its own hash of the id, so neighbours in the tribe do not share a look
    let pick = |k: i32, n: usize| (alaz_sim::rng::hash2(0xA11CE, id as i32, k) % n as u64) as usize;
    let tones = [[0.90, 0.69, 0.53], [0.78, 0.55, 0.39], [0.60, 0.42, 0.30], [0.94, 0.77, 0.64], [0.47, 0.32, 0.23], [0.84, 0.62, 0.44]];
    let hairs = [[0.17, 0.11, 0.06], [0.33, 0.19, 0.08], [0.08, 0.07, 0.07], [0.54, 0.35, 0.15], [0.46, 0.17, 0.08], [0.66, 0.62, 0.56], [0.24, 0.17, 0.12]];
    let dyes = [[0.72, 0.28, 0.2], [0.26, 0.36, 0.6], [0.86, 0.64, 0.2], [0.38, 0.52, 0.3], [0.52, 0.3, 0.46]];
    let tint = 0.95 + 0.03 * pick(1, 4) as f32;
    let frame = [(0.93, 1.07), (1.0, 1.0), (1.08, 0.97)][pick(6, 3)];
    Look {
        skin: tones[pick(0, tones.len())].map(|c| c * tint),
        hair: hairs[pick(2, hairs.len())],
        style: pick(3, 6),
        dye: dyes[pick(8, dyes.len())],
        frame,
        beard: frame.0 > 0.95 && pick(5, 3) == 0,
        braid_side: if pick(4, 2) == 0 { 1.0 } else { -1.0 },
    }
}

/// What a person wears, from what they carry. A tunic replaces the fibre wrap; the rest layer over each other.
pub fn outfit_bits(inv: &alaz_sim::world::Inventory) -> u16 {
    use alaz_sim::ItemKind as I;
    [(I::FiberWrap, OUT_WRAP), (I::HideCloak, OUT_CLOAK), (I::HideBoots, OUT_BOOTS), (I::Tunic, OUT_TUNIC), (I::LinenHood, OUT_HOOD), (I::BronzePlate, OUT_PLATE), (I::SteelHelm, OUT_HELM), (I::WorkApron, OUT_APRON)]
        .iter()
        .filter(|(k, _)| inv.counts[*k as usize] > 0)
        .fold(0, |m, (_, b)| m | b)
}

/// A person this tall on the screen (pixels) or less is drawn with `Pose::lod`; the figure is 1.9 m.
const LOD_PX: f32 = 70.0;

/// Whether the people in view are small enough (zoomed far out) for the simpler build.
pub(crate) fn human_lod(cam: &Camera) -> bool {
    1.9 * Camera::ppm() * cam.scale() < LOD_PX
}

/// A person built from tapered limbs and round parts: legs and arms bend, walking swings them,
/// and every kind of work gets its own motion.
pub(crate) fn push_human(out: &mut Mesh, x: f32, z: f32, pose: Pose) {
    out.set_anchor(x, z);
    let lod = pose.lod;
    // the simple build uses fewer sides; the full one passes every count through unchanged
    let limb = |out: &mut Mesh, a: [f32; 3], b: [f32; 3], r0: f32, r1: f32, col: [f32; 3], segs: usize| shapes::limb(out, a, b, r0, r1, col, if lod { segs.div_ceil(2).max(4) } else { segs });
    let sphere = |out: &mut Mesh, c: [f32; 3], r: [f32; 3], col: [f32; 3], nu: usize, nv: usize, wob: f32, seed: u32| {
        let (nu, nv) = if lod { (nu.div_ceil(2).max(4), nv.min(3)) } else { (nu, nv) };
        shapes::sphere(out, c, r, col, nu, nv, wob, seed)
    };
    let ellipsoid = |out: &mut Mesh, c: [f32; 3], axes: [[f32; 3]; 3], col: [f32; 3], nu: usize, nv: usize| {
        let (nu, nv) = if lod { (nu.div_ceil(2).max(5), nv.min(3)) } else { (nu, nv) };
        shapes::ellipsoid(out, c, axes, col, nu, nv)
    };
    let (s, c) = pose.heading.sin_cos();
    let (fx, fz, rx, rz) = (s, c, c, -s);
    let id = pose.id as usize;
    // every trait draws on its own hash of the id, so neighbours in the tribe do not share a look
    let lk = look(pose.id);
    // the further from the ape, the straighter, taller and smoother: hunched and furry at stage 0
    let stoop = (5 - pose.stage.min(5)) as f32 / 5.0;
    let ape = stoop > 0.55;
    let fur = [0.33, 0.23, 0.15];
    let base_skin = lk.skin;
    let skin = [0, 1, 2].map(|i| base_skin[i] + (fur[i] - base_skin[i]) * stoop * 0.5);
    let skin_dk = [skin[0] * 0.86, skin[1] * 0.8, skin[2] * 0.78];
    let hair = lk.hair;
    let brow = hair.map(|c| c * 0.55);
    let hide = [0.56, 0.37, 0.21];
    let hide_dk = [0.40, 0.26, 0.15];
    let fluff = [0.86, 0.80, 0.66];
    let fibre = [0.66, 0.70, 0.36];
    let fibre_dk = [0.47, 0.52, 0.24];
    let mut o = pose.outfit;
    if !lod && assets::has_models() {
        // garments that come from a model file are stamped on the body's spot; the code-made ones are left out
        o &= !file_outfit(out, x, z, &pose);
    }
    let (wrap, cloak, boots) = (o & OUT_WRAP != 0, o & OUT_CLOAK != 0, o & OUT_BOOTS != 0);
    let (tunic, hood, plate, helm, apron) = (o & OUT_TUNIC != 0, o & OUT_HOOD != 0, o & OUT_PLATE != 0, o & OUT_HELM != 0, o & OUT_APRON != 0);
    // cloth is natural linen or dyed one of five ways; metal is bronze or steel
    let linen = [0.84, 0.78, 0.62];
    let dye = lk.dye;
    let bronze = [0.78, 0.56, 0.24];
    let bronze_dk = [0.58, 0.4, 0.17];
    let steel = [0.7, 0.74, 0.8];
    let steel_dk = [0.46, 0.5, 0.58];
    let denim = [0.3, 0.35, 0.45];
    let canvas = [0.43, 0.36, 0.27];
    // a frame per person: slight, even or broad in the shoulder, with hips the other way
    let (sh_k, hip_k) = lk.frame;
    // build and height differ a little from person to person, so a tribe is not a row of copies
    let build = 0.94 + 0.04 * ((id * 7) % 4) as f32;
    let body_k = (0.86 + 0.14 * (1.0 - stoop)) * (0.97 + 0.02 * ((id * 5) % 3) as f32);
    let w = pose.work;
    let working = w != Work::None;
    let still = (1.0 - pose.amp * 4.0).clamp(0.0, 1.0) * if working { 0.0 } else { 1.0 };
    let swing = pose.phase.sin() * pose.amp;
    let breathe = (pose.t * 1.9 + id as f32).sin() * 0.008 * still;
    let bob = pose.phase.sin().abs() * 0.05 * pose.amp + breathe;
    let u = pose.cyc.fract();
    let act = act(w, u);
    // the moment right after a blow lands: the body gives a little
    let hit = if working && u >= IMPACT { (1.0 - (u - IMPACT) / 0.12).max(0.0) } else { 0.0 };
    let (crouch, lean) = if working { (act.crouch + 0.03 * hit, act.lean + stoop * 0.12) } else { (0.0, (pose.amp - 1.0).max(0.0) * 0.3 + stoop * 0.22) };
    let twist = act.twist;
    // on steep ground: climbing bends forward and grabs ahead, going down leans back and skids with arms out
    let moving = (pose.amp * 3.0).min(1.0) * if working { 0.0 } else { 1.0 };
    let ss = |a: f32, b: f32, v: f32| ((v - a) / (b - a)).clamp(0.0, 1.0);
    let climb = ss(0.22, 0.5, pose.slope) * moving;
    let slide = ss(0.22, 0.5, -pose.slope) * moving;
    let crouch = crouch + 0.13 * climb + 0.16 * slide;
    let lean = lean + 0.34 * climb - 0.22 * slide;
    let swing = swing * (1.0 - 0.85 * slide);
    // a blow: the body jerks back and doubles over for a moment
    let jolt = pose.hurt * pose.hurt;
    let crouch = crouch + 0.12 * jolt;
    let lean = lean - 0.22 * jolt;
    let start = out.len();
    // points are (forward, up, side) around the body, turned to the heading
    let at = |f: f32, up: f32, side: f32| [x + fx * f + rx * side * build, up * body_k, z + fz * f + rz * side * build];
    // a body part shaped by half-widths across, up and along the facing, leaning forward by `tilt`
    let part = |out: &mut Mesh, c: [f32; 3], side: f32, up: f32, fwd: f32, tilt: f32, col: [f32; 3], nu: usize| {
        let (ts, tc) = tilt.sin_cos();
        let u = [fx * ts * up, tc * up * body_k, fz * ts * up];
        let f = [fx * tc * fwd, -ts * fwd * body_k, fz * tc * fwd];
        ellipsoid(out, c, [[rx * side * build, 0.0, rz * side * build], u, f], col, nu, 5);
    };
    // the upper body, turned by `twist` over the hips
    let (ts_, tc_) = (pose.heading + twist).sin_cos();
    let (ufx, ufz, urx, urz) = (ts_, tc_, tc_, -ts_);
    let ua = |f: f32, up: f32, side: f32| [x + ufx * f + urx * side * build, up * body_k, z + ufz * f + urz * side * build];
    let upart = |out: &mut Mesh, c: [f32; 3], side: f32, up: f32, fwd: f32, tilt: f32, col: [f32; 3], nu: usize| {
        let (ts, tc) = tilt.sin_cos();
        let u = [ufx * ts * up, tc * up * body_k, ufz * ts * up];
        let f = [ufx * tc * fwd, -ts * fwd * body_k, ufz * tc * fwd];
        ellipsoid(out, c, [[urx * side * build, 0.0, urz * side * build], u, f], col, nu, 5);
    };
    let y = |h: f32| h + bob - crouch;
    // bending forward turns the whole upper body over the hips, so the spine keeps its length; `lean` is how far
    // ahead the chest goes
    let bend = lean.atan2(0.4).clamp(-0.6, 1.2);
    let (bs, bc) = bend.sin_cos();
    let tilt = bend;
    // a point of the upper body, given as it would be standing (`extra` ahead of the spine, height `h`), bent over
    let ub = |extra: f32, h: f32, side: f32| {
        let d = h - 0.9;
        ua(d * bs + extra * bc, y(0.9 + d * bc - extra * bs), side)
    };
    let pelvis = at(0.0, y(0.9), 0.0);
    let chest = ub(0.0, 1.3, 0.0);
    let neck = ub(0.03, 1.5, 0.0);

    // legs: thigh, shin and a foot that points where the body faces; stride when walking, planted when working
    for side in [-1.0f32, 1.0] {
        let ph = -side;
        // the foot's swing is the stride, fading in from a standstill; planted feet keep pace with the ground
        let off = pose.phase.sin() * pose.stride * pose.amp.min(1.0) * (1.0 - 0.85 * slide) * ph;
        let lift = ((pose.phase + if ph > 0.0 { 0.0 } else { std::f32::consts::PI }).cos()).max(0.0) * pose.amp * 0.14 * (1.0 + 1.4 * climb) * (1.0 - slide);
        let planted = if working { 0.1 * ph } else { 0.0 } + slide * 0.16 * ph;
        let hip = at(0.0, y(0.88), side * 0.1);
        let li = if side < 0.0 { 0 } else { 1 };
        let (ankle, knee, foot) = if working {
            // feet set for the job; the knee found from the two leg bones, bending forward (or down onto the ground)
            let [ff, fs] = act.feet[li];
            let kneel = act.kneel[li];
            let ankle = at(ff, if kneel { 0.07 } else { 0.09 }, fs);
            let seg = 0.42 * body_k;
            let d = [ankle[0] - hip[0], ankle[1] - hip[1], ankle[2] - hip[2]];
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-3);
            let a = (seg * seg - len * len * 0.25).max(0.0).sqrt();
            let dn = [d[0] / len, d[1] / len, d[2] / len];
            let fdot = fx * dn[0] + fz * dn[2];
            let mut p = [fx - dn[0] * fdot, -dn[1] * fdot, fz - dn[2] * fdot];
            let pl = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt().max(1e-3);
            p = [p[0] / pl, p[1] / pl, p[2] / pl];
            let mut knee = [hip[0] + d[0] * 0.5 + p[0] * a, hip[1] + d[1] * 0.5 + p[1] * a, hip[2] + d[2] * 0.5 + p[2] * a];
            knee[1] = knee[1].max(0.07);
            let foot = if kneel { at(ff - 0.07, 0.045, fs) } else { at(ff + 0.06, 0.045, fs) };
            (ankle, knee, foot)
        } else {
            // in the air the knees come up on the rise; falling, the legs hang and kick
            let lift = if pose.air != 0.0 {
                let up = pose.air.max(0.0);
                0.08 + 0.2 * up + if pose.air < 0.0 { 0.06 * (pose.t * 13.0 + side * 1.7).sin() * (-pose.air) } else { 0.0 }
            } else {
                lift
            };
            let ankle = at(off + planted, lift + 0.09, side * 0.11);
            let bend = 0.07 + 0.18 * (crouch / 0.36) + lift * 0.9;
            let knee = at((off + planted) * 0.5 + bend, 0.47 + lift * 0.5 - crouch * 0.35, side * 0.11);
            (ankle, knee, at(off + planted + 0.06, lift + 0.045, side * 0.11))
        };
        let trousers = apron;
        limb(out, hip, knee, 0.09, 0.07, if trousers { denim } else if ape { fur } else { skin_dk }, 6);
        limb(out, knee, ankle, 0.07, 0.05, if boots { hide } else if trousers { denim } else { skin_dk }, 6);
        part(out, foot, 0.06, 0.045, 0.11, 0.0, if boots { hide_dk } else { skin_dk }, 6);
        if boots && !lod {
            // a fur cuff where the boot ends
            let cuff = [knee[0] * 0.45 + ankle[0] * 0.55, knee[1] * 0.45 + ankle[1] * 0.55, knee[2] * 0.45 + ankle[2] * 0.55];
            sphere(out, cuff, [0.078, 0.045, 0.078], fluff, 6, 3, 0.12, 1);
        }
    }

    // torso: hips, a waist and a chest wider than it is deep, rounded shoulders
    let torso_col = if ape { fur } else { skin };
    part(out, pelvis, 0.17 * hip_k, 0.12, 0.12, 0.0, if ape { fur } else { skin_dk }, 8);
    limb(out, pelvis, chest, 0.15, 0.17, torso_col, 8);
    upart(out, chest, 0.22 * sh_k, 0.17, 0.14, tilt, torso_col, 9);
    if ape {
        // shaggy shoulders and back
        sphere(out, ub(-0.06, 1.4, 0.0), [0.2, 0.12, 0.2], fur, 8, 4, 0.18, id as u32);
    }
    // a loincloth until the tribe learns to make clothes; then a fibre skirt and a band across the chest
    if tunic {
        // a woven tunic in dyed cloth: a skirt to the knee with an undyed hem, a corded waist and a collar
        limb(out, at(0.0, y(1.14), 0.0), at(0.01, y(0.6), 0.0), 0.168, 0.25 * hip_k, dye, 9);
        limb(out, at(0.0, y(0.63), 0.0), at(0.0, y(0.58), 0.0), 0.252 * hip_k, 0.258 * hip_k, linen, 9);
        upart(out, ub(0.0, 1.3, 0.0), 0.228 * sh_k, 0.185, 0.152, tilt, dye, 9);
        if !lod {
            sphere(out, ub(0.02, 1.47, 0.0), [0.1, 0.03, 0.1], linen, 7, 3, 0.0, 1);
        }
    } else if wrap {
        limb(out, at(0.0, y(0.94), 0.0), at(0.0, y(0.55), 0.0), 0.18, 0.26, fibre, 9);
        limb(out, at(0.0, y(0.7), 0.0), at(0.0, y(0.54), 0.0), 0.24, 0.27, fibre_dk, 9);
        upart(out, ub(0.0, 1.32, 0.0), 0.225, 0.07, 0.145, tilt, fibre, 9);
    } else {
        limb(out, at(0.0, y(0.92), 0.0), at(0.03, y(0.66), 0.0), 0.165, 0.14, hide, 7);
    }
    if pose.stage >= 2 {
        // a belt with a pouch on the hip for what the hands do not hold
        let belt_r = if tunic || apron || plate { 0.2 } else { 0.175 };
        limb(out, at(0.0, y(0.97), 0.0), at(0.0, y(0.91), 0.0), belt_r * hip_k.max(1.0), belt_r * hip_k.max(1.0), hide_dk, 9);
        if !lod {
            part(out, at(-0.02, y(0.86), 0.19), 0.05, 0.065, 0.06, 0.0, hide, 6);
        }
    }
    if apron {
        // a canvas bib and skirt with a brass buckle, held by straps over the shoulders
        part(out, at(0.13, y(0.8), 0.0), 0.15, 0.3, 0.035, 0.0, canvas, 8);
        upart(out, ub(0.14, 1.27, 0.0), 0.13, 0.16, 0.03, tilt, canvas, 8);
        if !lod {
            for side in [-1.0f32, 1.0] {
                limb(out, ub(0.13, 1.4, side * 0.09), ub(-0.02, 1.47, side * 0.15), 0.02, 0.02, canvas, 5);
            }
            sphere(out, ub(0.16, 1.3, 0.0), [0.03; 3], [0.85, 0.7, 0.3], 5, 3, 0.0, 1);
            part(out, at(0.16, y(0.78), 0.07), 0.05, 0.045, 0.02, 0.0, [0.34, 0.28, 0.2], 6);
        }
    }
    if plate {
        // a bronze breastplate with a studded kilt band
        upart(out, ub(0.0, 1.31, 0.0), 0.236 * sh_k, 0.2, 0.16, tilt, bronze, 9);
        limb(out, at(0.0, y(0.99), 0.0), at(0.0, y(0.84), 0.0), 0.205 * hip_k, 0.235 * hip_k, bronze_dk, 9);
        if !lod {
            sphere(out, ub(0.15, 1.34, 0.0), [0.045, 0.06, 0.03], bronze_dk, 6, 4, 0.0, 2);
        }
    }
    if cloak {
        // a hide mantle over the shoulders with a fur collar, hanging down the back
        upart(out, ub(-0.02, 1.43, 0.0), 0.28, 0.1, 0.19, tilt, hide, 9);
        limb(out, ub(-0.1, 1.42, 0.0), at(-0.16, y(0.7) + crouch * 0.4, 0.0), 0.21, 0.27, hide_dk, 8);
        sphere(out, ub(-0.02, 1.46, 0.0), [0.17, 0.05, 0.16], fluff, 8, 4, 0.12, 2);
    }

    // head: turns to look around while standing idle
    let look = ((pose.t * 0.45 + id as f32 * 1.3).sin() * 1.6).clamp(-1.0, 1.0) * 0.55 * still;
    let (ls, lc) = (pose.heading + look + twist + act.head[2]).sin_cos();
    let (hfx, hfz, hrx, hrz) = (ls, lc, lc, -ls);
    let head = ub(0.05 * bs.max(0.0) + act.head[0], 1.67 + act.head[1], 0.0);
    let hd = |f: f32, up: f32, side: f32| [head[0] + hfx * f + hrx * side, head[1] + up * body_k, head[2] + hfz * f + hrz * side];
    let hpart = |out: &mut Mesh, c: [f32; 3], side: f32, up: f32, fwd: f32, col: [f32; 3]| {
        ellipsoid(out, c, [[hrx * side, 0.0, hrz * side], [0.0, up * body_k, 0.0], [hfx * fwd, 0.0, hfz * fwd]], col, 8, 5);
    };
    limb(out, chest, neck, 0.075, 0.065, skin, 6);
    hpart(out, head, 0.135, 0.15, 0.14, skin);
    // jaw and chin, ears
    hpart(out, hd(0.04 + 0.04 * stoop, -0.08, 0.0), 0.095, 0.06, 0.09 + 0.03 * stoop, skin);
    for side in [-1.0f32, 1.0] {
        if !lod {
            hpart(out, hd(-0.01, 0.0, side * 0.135), 0.022, 0.04, 0.03, skin_dk);
        }
    }
    if stoop > 0.35 {
        // an early skull: a heavy brow ridge
        hpart(out, hd(0.1, 0.055, 0.0), 0.13, 0.035, 0.05, skin_dk);
    }
    // eyes, brows and a nose; at 27 texels to the metre a face is a few texels, so the eyes sit in a dark band
    // (brows dark, a touch of shadow beside the nose) and the mouth is a single line
    for side in [-1.0f32, 1.0] {
        sphere(out, hd(0.127, 0.012, side * 0.052), [0.024, 0.02, 0.02], [0.05, 0.04, 0.04], 4, 3, 0.0, 1);
        if stoop <= 0.35 {
            hpart(out, hd(0.12, 0.052, side * 0.052), 0.034, 0.014, 0.02, brow);
        } else {
            // under the heavy ridge the eyes are deep in shadow
            hpart(out, hd(0.118, 0.026, side * 0.052), 0.036, 0.022, 0.02, skin_dk);
        }
    }
    let nose_dk = [skin[0] * 0.8, skin[1] * 0.68, skin[2] * 0.62];
    hpart(out, hd(0.15, -0.025, 0.0), 0.026, 0.035, 0.03, [skin[0] * 0.95, skin[1] * 0.84, skin[2] * 0.78]);
    if !lod {
        hpart(out, hd(0.14, -0.06, 0.0), 0.03, 0.01, 0.026, nose_dk);
        hpart(out, hd(0.122, -0.088, 0.0), 0.034, 0.007, 0.016, [skin[0] * 0.6, skin[1] * 0.4, skin[2] * 0.38]);
    }
    // hair: one of six cuts per person (the ape is always shaggy), and a beard on some
    let style = if ape { 3 } else { lk.style };
    let crop = if style == 5 { 0.94 } else { 1.0 };
    hpart(out, hd(-0.02, 0.055, 0.0), 0.148 * crop, 0.12 * crop, 0.15 * crop, hair);
    if style != 5 {
        hpart(out, hd(0.075, 0.1, 0.0), 0.12, 0.045, 0.07, hair);
    }
    match style {
        // a braid over one shoulder, tied off with a bit of hide
        4 => {
            let side = lk.braid_side;
            let top = hd(-0.07, -0.02, side * 0.09);
            let end = [neck[0] + urx * side * 0.17 - ufx * 0.08, neck[1] - 0.3 * body_k, neck[2] + urz * side * 0.17 - ufz * 0.08];
            limb(out, top, end, 0.045, 0.028, hair, 6);
            if !lod {
                sphere(out, end, [0.03; 3], hide, 5, 3, 0.0, 1);
            }
        }
        5 => {}
        // long hair down the back
        1 => limb(out, hd(-0.08, 0.02, 0.0), [neck[0] - fx * 0.14, neck[1] - 0.16 * body_k, neck[2] - fz * 0.14], 0.12, 0.08, hair, 7),
        // a knot on top
        2 => sphere(out, hd(-0.07, 0.17, 0.0), [0.065, 0.06, 0.065], hair, 6, 4, 0.0, 3),
        // shaggy all round
        3 => sphere(out, hd(-0.04, 0.03, 0.0), [0.17, 0.14, 0.17], hair, 8, 5, 0.22, id as u32),
        // short, with a tuft down the neck
        _ => hpart(out, hd(-0.1, -0.05, 0.0), 0.09, 0.09, 0.06, hair),
    }
    if !ape && lk.beard {
        hpart(out, hd(0.09, -0.1, 0.0), 0.1, 0.07, 0.07, hair);
    }
    if hood {
        // a linen hood: over the crown and ears, a cowl at the neck and a soft point behind
        hpart(out, hd(-0.035, 0.08, 0.0), 0.158, 0.115, 0.155, linen);
        limb(out, hd(-0.1, 0.02, 0.0), ub(-0.04, 1.44, 0.0), 0.11, 0.17, linen, 7);
        if !lod {
            sphere(out, hd(-0.15, 0.09, 0.0), [0.04, 0.05, 0.04], linen, 5, 3, 0.0, 1);
            hpart(out, hd(0.1, 0.1, 0.0), 0.12, 0.012, 0.03, dye);
        }
    }
    if helm {
        // a steel cap with a rim above the brows, a nose guard and a plume along the crest
        hpart(out, hd(-0.01, 0.085, 0.0), 0.158, 0.115, 0.162, steel);
        if !lod {
            hpart(out, hd(0.0, 0.098, 0.0), 0.164, 0.026, 0.168, steel_dk);
            hpart(out, hd(0.152, -0.01, 0.0), 0.012, 0.07, 0.012, steel);
        }
        hpart(out, hd(-0.01, 0.2, 0.0), 0.012, 0.03, 0.12, [0.66, 0.16, 0.12]);
    }

    // arms: shoulder, elbow, hand; an ape's hang lower
    let reach = 0.12 * stoop;
    let arm = |out: &mut Mesh, side: f32, hand: [f32; 3]| {
        let sh = ub(0.0, 1.42, side * 0.235 * sh_k);
        let mid = [(sh[0] + hand[0]) * 0.5, (sh[1] + hand[1]) * 0.5, (sh[2] + hand[2]) * 0.5];
        let elbow = [mid[0] + urx * side * 0.07 - ufx * 0.03, mid[1] - 0.06, mid[2] + urz * side * 0.07 - ufz * 0.03];
        let upper = if cloak { hide } else if tunic { dye } else if apron { linen } else if ape { fur } else { skin };
        if plate {
            sphere(out, sh, [0.098, 0.07, 0.098], bronze, 6, 4, 0.0, 1);
        } else {
            sphere(out, sh, [0.075; 3], upper, 6, 4, 0.0, 1);
        }
        limb(out, sh, elbow, 0.065, 0.052, upper, 6);
        limb(out, elbow, hand, 0.052, 0.042, skin, 6);
        sphere(out, hand, [0.055; 3], skin, 6, 4, 0.0, 1);
    };
    let wood = [0.46, 0.32, 0.18];
    match w {
        Work::None => {
            for side in [-1.0f32, 1.0] {
                let off = swing * side * 0.3;
                let walk = at(off + lean * 0.6, 0.86 + bob + 0.05 * pose.amp - reach, side * 0.28);
                // climbing: hands reach up the slope in turn; sliding: arms out for balance
                let grab = (pose.phase + if side > 0.0 { 0.0 } else { std::f32::consts::PI }).sin();
                let up = at(0.42 + 0.12 * grab, 1.0 + 0.22 * grab - crouch, side * 0.22);
                let wide = at(0.05, 1.22 + 0.04 * (pose.t * 6.0 + side).sin() - crouch, side * 0.6);
                let mixp = |a: [f32; 3], b: [f32; 3], k: f32| [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k];
                let hand = mixp(mixp(walk, up, climb), wide, slide);
                // a jump swings the arms up and forward; a fall throws them up and out, grabbing at the air
                let rise = at(0.3, 1.45, side * 0.24);
                let flail = at(0.08 + 0.1 * (pose.t * 11.0 + side).sin(), 1.75 + 0.1 * (pose.t * 14.0 + side * 2.0).sin(), side * 0.48);
                let hand = mixp(mixp(hand, rise, pose.air.max(0.0) * 0.8), flail, (-pose.air).max(0.0));
                arm(out, side, hand);
            }
            if slide > 0.3 && !lod {
                // dust kicked up behind the skidding feet
                for k in 0..4 {
                    let ph = (pose.t * 2.2 + k as f32 * 0.25).fract();
                    let side = if k % 2 == 0 { 0.12 } else { -0.12 };
                    let p = at(-0.15 - 0.55 * ph, 0.08 + 0.3 * ph, side * (1.0 + ph));
                    let r = 0.05 + 0.1 * ph;
                    sphere(out, p, [r, r * 0.8, r], [0.62, 0.56, 0.46], 5, 3, 0.2, k);
                }
            }
        }
        _ => {
            // a direction in the turned upper body's frame, in the world
            let lw = |v: [f32; 3]| {
                let w = [ufx * v[0] + urx * v[2], v[1], ufz * v[0] + urz * v[2]];
                let l = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt().max(1e-4);
                [w[0] / l, w[1] / l, w[2] / l]
            };
            let along = |p: [f32; 3], d: [f32; 3], k: f32| [p[0] + d[0] * k, p[1] + d[1] * k, p[2] + d[2] * k];
            let hpos = |h: [f32; 3]| ua(h[0], h[1], h[2]);
            let stone = [0.56, 0.56, 0.6];
            let [hr, hl] = act.hands;
            let right = hpos(hr);
            match w {
                Work::Chop | Work::Mine => {
                    // both hands on one handle, the left lower down it
                    let d = lw(act.dir);
                    let left = along(right, d, -0.17);
                    arm(out, 1.0, right);
                    arm(out, -1.0, left);
                    let file = if lod { None } else { tool_file(pose.tool) };
                    if let Some(m) = file {
                        // a model file for the tool: grip at the hand, handle up its y toward the head, blade facing +z
                        let n = lw(if w == Work::Mine { [0.0, 0.0, 1.0] } else { [-0.3, 0.11, -0.95] });
                        let b = [n[1] * d[2] - n[2] * d[1], n[2] * d[0] - n[0] * d[2], n[0] * d[1] - n[1] * d[0]];
                        m.place(out, &assets::Place { origin: right, basis: [[-b[0], -b[1], -b[2]], d, n], active: false, tick: 0, tint: None });
                    } else if pose.tool == Tool::Hand {
                        sphere(out, along(right, d, 0.06), [0.08, 0.07, 0.08], stone, 7, 5, 0.15, 3);
                    } else {
                        let len = if w == Work::Mine { 0.62 } else { 0.68 };
                        let end = along(right, d, len);
                        limb(out, along(right, d, -0.26), end, 0.038, 0.032, wood, 5);
                        // the swing's plane: the head's edge leads the blow
                        let n = lw(if w == Work::Mine { [0.0, 0.0, 1.0] } else { [-0.3, 0.11, -0.95] });
                        let b = [n[1] * d[2] - n[2] * d[1], n[2] * d[0] - n[0] * d[2], n[0] * d[1] - n[1] * d[0]];
                        let head_col = if pose.tool == Tool::StoneAxe { [0.56, 0.56, 0.58] } else { [0.66, 0.72, 0.82] };
                        if w == Work::Mine && pose.tool == Tool::MetalPick {
                            limb(out, end, along(end, b, 0.2), 0.035, 0.008, head_col, 5);
                            limb(out, end, along(end, b, -0.16), 0.035, 0.01, head_col, 5);
                        } else {
                            // a wedge of stone or metal lashed across the handle's end
                            let c = along(along(end, d, -0.06), b, 0.06);
                            limb(out, along(c, b, -0.07), along(c, b, 0.13), 0.07, 0.025, head_col, 5);
                            limb(out, along(end, d, -0.1), along(end, d, -0.02), 0.04, 0.04, [0.5, 0.42, 0.28], 5);
                        }
                    }
                }
                Work::Throw => {
                    let d = lw(act.dir);
                    arm(out, 1.0, right);
                    arm(out, -1.0, hpos(hl));
                    let tip = along(right, d, 1.0);
                    let file = if lod { None } else { tool_file(Tool::Spear) };
                    if let Some(m) = file {
                        // (the spear model: its middle at the hand, point up its y)
                        let up = [0.0, 1.0, 0.0];
                        let x = cross(up, d);
                        let x = if x[0] * x[0] + x[1] * x[1] + x[2] * x[2] < 1e-4 { [1.0, 0.0, 0.0] } else { norm(x) };
                        let zc = cross(x, d);
                        m.place(out, &assets::Place { origin: right, basis: [x, d, zc], active: false, tick: 0, tint: None });
                    } else {
                        limb(out, along(right, d, -0.6), tip, 0.025, 0.016, wood, 5);
                        limb(out, along(tip, d, -0.04), along(tip, d, 0.14), 0.04, 0.005, [0.62, 0.62, 0.66], 5);
                    }
                }
                Work::Hammer => {
                    let left = hpos(hl);
                    arm(out, 1.0, right);
                    arm(out, -1.0, left);
                    sphere(out, [right[0], right[1] + 0.03, right[2]], [0.07, 0.065, 0.08], [0.62, 0.62, 0.66], 7, 5, 0.12, 3);
                    let core = [left[0], left[1] + 0.05, left[2]];
                    sphere(out, core, [0.1, 0.07, 0.09], [0.48, 0.48, 0.52], 7, 5, 0.14, 5);
                    if hit > 0.0 && !lod {
                        // sparks and flakes off the core
                        let k = 1.0 - hit;
                        for i in 0..5 {
                            let a = i as f32 * 1.26 + pose.cyc.floor() * 2.1;
                            let r = 0.08 + 0.4 * k;
                            let p = [core[0] + a.cos() * r, core[1] + 0.05 + 0.5 * k - 1.4 * k * k, core[2] + a.sin() * r];
                            let (c, g) = if i % 2 == 0 { ([1.0, 0.85, 0.4], 1.0) } else { ([0.7, 0.7, 0.74], 0.0) };
                            shapes::sphere(out, p, [0.022; 3], c, 4, 3, 0.0, i);
                            if g > 0.0 {
                                shapes::sphere(out, p, [0.028; 3], c, 4, 3, 0.0, i);
                            }
                        }
                    }
                }
                Work::Dig => {
                    let left = hpos(hl);
                    arm(out, 1.0, right);
                    arm(out, -1.0, left);
                    let clay = [0.6, 0.42, 0.3];
                    let mid = [(right[0] + left[0]) * 0.5, (right[1] + left[1]) * 0.5 + 0.04, (right[2] + left[2]) * 0.5];
                    if (IMPACT..0.87).contains(&u) {
                        sphere(out, mid, [0.1, 0.07, 0.1], clay, 6, 4, 0.25, 7);
                    } else if u >= 0.87 {
                        // the toss: the clod flies off to the side and lands
                        let k = (u - 0.87) / 0.13;
                        let p = [mid[0] + urx * 0.9 * k, (mid[1] + 0.5 * k - 1.2 * k * k).max(0.05), mid[2] + urz * 0.9 * k];
                        sphere(out, p, [0.1, 0.07, 0.1], clay, 6, 4, 0.25, 7);
                    }
                    if hit > 0.0 && !lod {
                        let k = 1.0 - hit;
                        for i in 0..4 {
                            let a = i as f32 * 1.6 + pose.cyc.floor();
                            let p = [mid[0] + a.cos() * 0.3 * k, 0.05 + 0.3 * k - 0.4 * k * k, mid[2] + a.sin() * 0.3 * k];
                            sphere(out, p, [0.04; 3], clay, 4, 3, 0.2, i);
                        }
                    }
                }
                Work::Pick => {
                    let left = hpos(hl);
                    arm(out, 1.0, right);
                    arm(out, -1.0, left);
                    // whatever was plucked rides to the pouch in the hand that pulled it
                    let leafy = [0.42, 0.62, 0.26];
                    if (0.3..IMPACT + 0.05).contains(&u) {
                        sphere(out, [right[0], right[1] + 0.05, right[2]], [0.06; 3], leafy, 5, 4, 0.2, 2);
                    }
                    if u >= 0.8 || u < 0.05 {
                        sphere(out, [left[0], left[1] + 0.05, left[2]], [0.06; 3], leafy, 5, 4, 0.2, 2);
                    }
                }
                Work::Drink => {
                    let left = hpos(hl);
                    arm(out, 1.0, right);
                    arm(out, -1.0, left);
                    let water = [0.45, 0.66, 0.95];
                    let mid = [(right[0] + left[0]) * 0.5, (right[1] + left[1]) * 0.5, (right[2] + left[2]) * 0.5];
                    if (0.3..IMPACT).contains(&u) {
                        // water in the cupped hands, dripping on the way up
                        sphere(out, [mid[0], mid[1] + 0.04, mid[2]], [0.07, 0.03, 0.07], water, 6, 3, 0.0, 1);
                        for i in 0..2 {
                            let k = ((u - 0.3) * 6.0 + i as f32 * 0.5).fract();
                            shapes::sphere(out, [mid[0], mid[1] - 0.5 * k * k * 1.2, mid[2]], [0.022; 3], water, 4, 3, 0.0, i);
                        }
                    } else if u < 0.2 {
                        // the splash where the hands go in
                        for i in 0..4 {
                            let a = i as f32 * 1.57 + pose.cyc.floor();
                            let k = u / 0.2;
                            shapes::sphere(out, [mid[0] + a.cos() * 0.15 * k, 0.04 + 0.2 * k - 0.3 * k * k, mid[2] + a.sin() * 0.15 * k], [0.025; 3], water, 4, 3, 0.0, i);
                        }
                    }
                }
                Work::Offer => {
                    arm(out, 1.0, right);
                    arm(out, -1.0, hpos(hl));
                    sphere(out, along(right, lw([1.0, 0.6, 0.0]), 0.06), [0.05; 3], [0.8, 0.12, 0.2], 6, 4, 0.0, 1);
                }
                _ => {}
            }
        }
    }
    // ... and flashes red, then white at the very start
    if pose.hurt > 0.0 {
        let k = pose.hurt.powf(1.5);
        let flash = if pose.hurt > 0.85 { [1.0, 0.9, 0.85] } else { [1.0, 0.18, 0.12] };
        for v in &mut out[start..] {
            for i in 0..3 {
                v.color[i] += (flash[i] - v.color[i]) * k * 0.75;
            }
        }
        // a few drops flying off
        if pose.hurt > 0.4 && !lod {
            let u = 1.0 - pose.hurt;
            for i in 0..5u32 {
                let a = i as f32 * 1.26 + pose.id as f32;
                let (dx, dz) = (a.cos() * (0.25 + 1.4 * u), a.sin() * (0.25 + 1.4 * u));
                let y = 1.25 + 1.6 * u - 5.0 * u * u;
                shapes::sphere(out, [x + dx, y.max(0.05), z + dz], [0.035; 3], [0.62, 0.06, 0.06], 4, 3, 0.0, i);
            }
        }
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn norm(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / l, v[1] / l, v[2] / l]
}

/// The model file of a held tool (`models/tool_stone_axe.vox` ...), if any.
fn tool_file(tool: Tool) -> Option<std::sync::Arc<assets::Model>> {
    if !assets::has_models() {
        return None;
    }
    let name = match tool {
        Tool::Hand => return None,
        Tool::StoneAxe => "tool_stone_axe",
        Tool::MetalAxe => "tool_metal_axe",
        Tool::MetalPick => "tool_metal_pick",
        Tool::Spear => "tool_spear",
    };
    assets::model(name, 0)
}

/// Garments that have a model file (`models/outfit_cloak.vox` ...) are stamped at the person's feet, turned with them;
/// returns the bits of the ones that were, so the body does not draw its own.
fn file_outfit(out: &mut Mesh, x: f32, z: f32, pose: &Pose) -> u16 {
    const FILES: [(u16, &str); 8] = [(OUT_WRAP, "outfit_wrap"), (OUT_CLOAK, "outfit_cloak"), (OUT_BOOTS, "outfit_boots"), (OUT_TUNIC, "outfit_tunic"), (OUT_HOOD, "outfit_hood"), (OUT_PLATE, "outfit_plate"), (OUT_HELM, "outfit_helm"), (OUT_APRON, "outfit_apron")];
    let mut done = 0;
    for (bit, name) in FILES {
        if pose.outfit & bit != 0 {
            if let Some(m) = assets::model(name, pose.id) {
                m.place(out, &assets::Place::at(x, z).turned(pose.heading, 1.0));
                done |= bit;
            }
        }
    }
    out.set_anchor(x, z);
    done
}
