use super::*;

/// Render-side walking state: where a creature was, which way it faces, and how far into its stride it is.
#[derive(Clone, Copy)]
pub(crate) struct Walk {
    pub(crate) x: f32,
    pub(crate) z: f32,
    pub(crate) heading: f32,
    pub(crate) phase: f32,
    pub(crate) amp: f32,
    /// Health last frame, and how much of a flinch is left after a blow (1 to 0).
    pub(crate) hp: i32,
    pub(crate) hurt: f32,
    /// The two latest simulated positions and the tick of the newer one: drawing slides between them, so movement
    /// is smooth whatever the frame rate and however many ticks a frame runs.
    pub(crate) prev: [f32; 2],
    pub(crate) sim: [f32; 2],
    pub(crate) tick: u64,
    /// How far a foot swings forward and back (metres); the walk cycle advances so that a planted foot stays put.
    pub(crate) stride: f32,
    pub(crate) speed: f32,
    /// Crafting: the last strike count seen and seconds since the hand last came down.
    pub(crate) craft: u64,
    pub(crate) strike: f32,
    /// How fast the body is turning (radians a second): turns ease in and out, and the body leans into them.
    pub(crate) spin: f32,
}

impl Walk {
    pub(crate) fn at(x: f32, z: f32, heading: f32, hp: i32) -> Walk {
        Walk { x, z, heading, phase: 0.0, amp: 0.0, hp, hurt: 0.0, prev: [x, z], sim: [x, z], tick: 0, stride: 0.3, speed: 0.0, craft: 0, strike: 9.0, spin: 0.0 }
    }

    /// Where to draw it this frame: between the last two simulated positions.
    pub(crate) fn smooth(&mut self, x: f32, z: f32, tick: u64, alpha: f32) -> (f32, f32) {
        if tick != self.tick {
            let jump = (x - self.sim[0]).hypot(z - self.sim[1]);
            self.prev = if jump > 3.0 { [x, z] } else { self.sim };
            self.sim = [x, z];
            self.tick = tick;
        }
        (self.prev[0] + (self.sim[0] - self.prev[0]) * alpha, self.prev[1] + (self.sim[1] - self.prev[1]) * alpha)
    }
}

/// Foot swing (metres) for a four-legged animal with legs `leg_h` long.
pub(crate) fn quad_stride(leg_h: f32) -> f32 {
    0.55 * leg_h + 0.04
}

/// Follow a creature from where it was to where it is now: turn toward the way it goes, advance the stride by the
/// ground covered, and ease the stride size toward its pace (a run swings harder). True when it moved.
/// Follows a creature's drawn position: turns it the way it goes and runs its walk cycle. `base` is how far a foot
/// swings at a walk (metres); at a run the stride lengthens. The cycle advances by exactly the ground covered, so
/// a planted foot moves back as fast as the body moves forward and nothing slides.
pub(crate) fn step_walk(w: &mut Walk, x: f32, z: f32, dt: f32, base: f32) -> bool {
    let (dx, dz) = (x - w.x, z - w.z);
    let step = (dx * dx + dz * dz).sqrt();
    let moved = step > 0.0002 && step < 3.0;
    let v = if moved { step / dt.max(1e-3) } else { 0.0 };
    w.speed += (v - w.speed) * (1.0 - (-10.0 * dt).exp());
    // turning: a spring toward where it is going (no snapping round), its speed kept for the lean
    let want = if moved { dx.atan2(dz) } else { w.heading };
    let omega = 11.0;
    let off = wrap_pi(want - w.heading);
    w.spin += (omega * omega * off - 2.0 * omega * w.spin) * dt.min(0.05);
    w.heading += w.spin * dt;
    // walking is about 1.5 m/s; faster is a run, with longer strides
    let pace = if w.speed > 0.05 { (w.speed / 1.5).clamp(1.0, 1.8) } else { 0.0 };
    w.amp += (pace - w.amp) * (1.0 - (-12.0 * dt).exp());
    let stride = base * (1.0 + 0.45 * (pace.max(1.0) - 1.0));
    w.stride += (stride - w.stride) * (1.0 - (-8.0 * dt).exp());
    if moved {
        // a foot travels 2 * stride while planted, over half a cycle (pi)
        w.phase += step * std::f32::consts::PI / (2.0 * w.stride.max(0.05));
    }
    (w.x, w.z) = (x, z);
    moved
}

pub(crate) fn wrap_pi(mut d: f32) -> f32 {
    while d > std::f32::consts::PI {
        d -= 2.0 * std::f32::consts::PI;
    }
    while d < -std::f32::consts::PI {
        d += 2.0 * std::f32::consts::PI;
    }
    d
}

/// The route an agent is walking: a line along its path to the target, with a pulsing flag at the end.
pub(crate) fn push_route(out: &mut Mesh, world: &World, a: &alaz_sim::world::Agent, tick: u64) {
    use alaz_sim::world::Task;
    let target = match a.task {
        Task::Move { x, y } => Some((x, y)),
        Task::Gather { node, .. } => world.nodes.get(node.0 as usize).map(|n| n.center_mm()),
        Task::Drink { tx, ty } => Some((tx as i64 * 1000 + 500, ty as i64 * 1000 + 500)),
        Task::Tame { wild, .. } | Task::Hunt { wild, .. } => world.wild.iter().find(|w| w.id == wild).map(|w| (w.x, w.y)),
        _ => None,
    };
    let Some((tx, ty)) = target else { return };
    let m = |v: i64| v as f32 / 1000.0;
    let mut pts: Vec<[f32; 2]> = vec![[m(a.x), m(a.y)]];
    for &(px, py) in a.path.iter().rev() {
        pts.push([px as f32 + 0.5, py as f32 + 0.5]);
    }
    pts.push([m(tx), m(ty)]);
    let col = [1.0, 0.86, 0.25];
    for w in pts.windows(2) {
        push_segment(out, w[0], w[1], 0.14, 0.07, col, 0.7);
        // dashes between the nodes make the direction readable
        let (dx, dz) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
        let len = (dx * dx + dz * dz).sqrt();
        let n = (len / 1.2) as i32;
        let flow = (tick as f32 / 20.0).fract();
        for i in 0..n {
            let t = ((i as f32 + flow) * 1.2 / len).min(1.0);
            push_box(out, [w[0][0] + dx * t, 0.08, w[0][1] + dz * t], [0.1, 0.05, 0.1], [1.0, 0.98, 0.7], 1.0);
        }
    }
    let end = *pts.last().unwrap();
    // going to a resource: no flag, the resource itself glows (drawn by the caller)
    if matches!(a.task, Task::Gather { .. }) {
        return;
    }
    let pulse = 0.5 + 0.5 * (tick as f32 / 12.0).sin();
    push_box(out, [end[0], 0.0, end[1]], [0.04, 1.3, 0.04], [0.9, 0.85, 0.6], 0.5);
    push_box(out, [end[0] + 0.2, 1.0, end[1]], [0.22, 0.28, 0.03], [1.0, 0.3 + 0.4 * pulse, 0.2], 1.0);
    push_box(out, [end[0], 0.0, end[1]], [0.32 + 0.12 * pulse, 0.03, 0.32 + 0.12 * pulse], col, 1.0);
}

/// Where in a work cycle the blow lands (the tool meets the wood, the hands reach the mouth, ...).
pub(crate) const IMPACT: f32 = 0.62;

/// How far through its work an agent is, as whole cycles plus a fraction (blows land at fraction `IMPACT`).
/// Gathering is tied to the simulation's own progress so that each unit arrives exactly on a blow; a throw winds
/// up over the ticks it takes to land; a craft strike plays when the player strikes.
pub(crate) fn work_cycle(world: &World, a: &alaz_sim::world::Agent, work: Work, w: &mut Walk, alpha: f32, secs: f32, dt: f32) -> f32 {
    use alaz_sim::world::Task;
    // ALAZ_DEMO_U freezes every work motion at one moment of its cycle, for screenshots
    static FREEZE: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    if let Some(u) = *FREEZE.get_or_init(|| std::env::var("ALAZ_DEMO_U").ok().and_then(|v| v.parse().ok())) {
        return u;
    }
    let off = (a.id.0 % 7) as f32 * 0.13;
    match (work, a.task) {
        (Work::Chop | Work::Mine | Work::Dig | Work::Pick, Task::Gather { node, progress }) => {
            let Some(n) = world.nodes.get(node.0 as usize) else { return 0.0 };
            let need = alaz_sim::world::gather_need(n.kind, &a.inventory, world.perception_level()) as f32;
            // seconds one blow should take, so a long harvest is several quick blows rather than one slow one
            let beat = match work {
                Work::Chop => 0.7,
                Work::Mine => 0.8,
                Work::Dig => 0.85,
                _ => 0.55,
            };
            let blows = (need / alaz_sim::time::TICKS_PER_SECOND as f32 / beat).round().max(1.0);
            ((progress as f32 + alpha) / need).min(1.0) * blows + IMPACT
        }
        (Work::Mine, Task::Split { progress, .. }) => {
            // one blow for each piece that breaks off
            let need = alaz_sim::world::split_need(&a.inventory, world.perception_level()) as f32;
            ((progress as f32 + alpha) / need).min(1.0) + IMPACT
        }
        (Work::Pick, Task::Pick { progress, .. }) => ((progress as f32 + alpha) / alaz_sim::world::PICK_TICKS as f32).min(1.0) + IMPACT - 0.5,
        (Work::Throw, Task::Hunt { progress, .. }) => IMPACT * ((progress as f32 + alpha) / alaz_sim::world::HUNT_THROW_TICKS as f32).min(1.0),
        (Work::Hammer, Task::Craft { step, done, hits, .. }) => {
            let key = (step as u64) << 16 | (done as u64) << 8 | hits as u64 | 1 << 40;
            if key != w.craft {
                if w.craft != 0 {
                    w.strike = 0.0;
                }
                w.craft = key;
            }
            w.strike += dt;
            // a quick downstroke from the raised hand, the knock, and back up; at rest the hand waits raised
            let s = w.strike;
            if s < 0.07 {
                0.4 + (IMPACT - 0.4) * s / 0.07
            } else if s < 0.45 {
                IMPACT + (1.0 - IMPACT) * (s - 0.07) / 0.38
            } else {
                0.1 + 0.05 * (secs * 2.0).sin()
            }
        }
        (Work::Drink, _) => secs / 1.3 + off,
        (Work::Offer, _) => secs / 1.6 + off,
        _ => 0.0,
    }
}

/// Keyframed motion: `k` holds (time 0..1, values); values ease between neighbouring keys.
pub(crate) fn keys<const N: usize>(u: f32, k: &[(f32, [f32; N])]) -> [f32; N] {
    let i = k.iter().rposition(|(t, _)| *t <= u).unwrap_or(0).min(k.len() - 2);
    let (t0, a) = k[i];
    let (t1, b) = k[i + 1];
    let s = ((u - t0) / (t1 - t0).max(1e-4)).clamp(0.0, 1.0);
    let s = s * s * (3.0 - 2.0 * s);
    std::array::from_fn(|j| a[j] + (b[j] - a[j]) * s)
}

/// Whole-body pose for a moment of work. Hands and the tool are in the frame of the turned upper body
/// (forward, up, side); feet in the frame of the hips.
pub(crate) struct Act {
    pub(crate) crouch: f32,
    pub(crate) lean: f32,
    /// Upper body turned right of the hips (radians).
    pub(crate) twist: f32,
    /// Right hand, left hand (forward, height, side).
    pub(crate) hands: [[f32; 3]; 2],
    /// Which way the held tool points from the right hand.
    pub(crate) dir: [f32; 3],
    /// Head nudged forward and up, and turned (radians) relative to the shoulders.
    pub(crate) head: [f32; 3],
    /// Feet (forward, side), left then right, and whether that leg kneels.
    pub(crate) feet: [[f32; 2]; 2],
    pub(crate) kneel: [bool; 2],
}

/// The pose of each kind of work at fraction `u` of its cycle. Fast strikes, a held moment at the blow, and a
/// slower wind-up, so every action reads at a glance.
pub(crate) fn act(w: Work, u: f32) -> Act {
    // key layout: crouch, lean, twist, right hand xyz, left hand xyz, tool dir xyz, head f/up/yaw
    type K = [f32; 15];
    let pick = |k: K| -> Act {
        Act {
            crouch: k[0],
            lean: k[1],
            twist: k[2],
            hands: [[k[3], k[4], k[5]], [k[6], k[7], k[8]]],
            dir: [k[9], k[10], k[11]],
            head: [k[12], k[13], k[14]],
            feet: [[0.0, -0.11], [0.0, 0.11]],
            kneel: [false, false],
        }
    };
    match w {
        Work::Chop => {
            // a diagonal two-handed swing from over the right shoulder, the whole body turning into it
            let k: [(f32, K); 6] = [
                (0.0, [0.06, 0.1, 0.25, 0.25, 1.18, 0.16, 0.0, 0.0, 0.0, 0.25, 0.92, 0.2, 0.0, 0.0, -0.2]),
                (0.45, [0.02, -0.04, 0.85, -0.04, 1.62, 0.2, 0.0, 0.0, 0.0, -0.6, 0.72, 0.25, -0.02, 0.01, -0.65]),
                (0.54, [0.02, -0.06, 0.92, -0.06, 1.64, 0.2, 0.0, 0.0, 0.0, -0.72, 0.6, 0.3, -0.02, 0.01, -0.7]),
                (IMPACT, [0.14, 0.3, -0.12, 0.56, 1.04, -0.02, 0.0, 0.0, 0.0, 0.95, 0.02, -0.3, 0.04, -0.02, 0.08]),
                (0.74, [0.12, 0.26, -0.08, 0.52, 1.07, 0.0, 0.0, 0.0, 0.0, 0.95, 0.07, -0.26, 0.03, -0.01, 0.05]),
                (1.0, [0.06, 0.1, 0.25, 0.25, 1.18, 0.16, 0.0, 0.0, 0.0, 0.25, 0.92, 0.2, 0.0, 0.0, -0.2]),
            ];
            Act { feet: [[0.22, -0.2], [-0.14, 0.2]], ..pick(keys(u, &k)) }
        }
        Work::Mine => {
            // high over the head, back arched, then a slam down into the rock bending the knees
            let k: [(f32, K); 6] = [
                (0.0, [0.05, 0.12, 0.0, 0.32, 1.2, 0.04, 0.0, 0.0, 0.0, 0.45, 0.9, 0.0, 0.0, 0.0, 0.0]),
                (0.45, [0.0, -0.14, 0.12, -0.06, 1.95, 0.04, 0.0, 0.0, 0.0, -0.75, 0.66, 0.0, -0.04, 0.03, -0.1]),
                (0.53, [0.0, -0.16, 0.12, -0.09, 1.97, 0.04, 0.0, 0.0, 0.0, -0.86, 0.5, 0.0, -0.04, 0.03, -0.1]),
                (IMPACT, [0.24, 0.44, 0.0, 0.56, 0.84, 0.03, 0.0, 0.0, 0.0, 0.55, -0.83, 0.0, 0.06, -0.06, 0.0]),
                (0.76, [0.22, 0.42, 0.0, 0.53, 0.87, 0.03, 0.0, 0.0, 0.0, 0.6, -0.8, 0.0, 0.05, -0.05, 0.0]),
                (1.0, [0.05, 0.12, 0.0, 0.32, 1.2, 0.04, 0.0, 0.0, 0.0, 0.45, 0.9, 0.0, 0.0, 0.0, 0.0]),
            ];
            Act { feet: [[0.14, -0.19], [-0.06, 0.19]], ..pick(keys(u, &k)) }
        }
        Work::Dig => {
            // squatting: both hands scoop the ground, haul the clay up and toss it to the side
            let k: [(f32, K); 7] = [
                (0.0, [0.38, 0.3, 0.0, 0.3, 0.74, 0.12, 0.3, 0.74, -0.12, 0.0, 1.0, 0.0, 0.03, -0.03, 0.0]),
                (0.45, [0.42, 0.52, 0.0, 0.6, 0.16, 0.1, 0.6, 0.16, -0.1, 0.0, 1.0, 0.0, 0.07, -0.07, 0.0]),
                (IMPACT, [0.45, 0.56, -0.05, 0.56, 0.1, 0.08, 0.56, 0.1, -0.08, 0.0, 1.0, 0.0, 0.07, -0.07, 0.0]),
                (0.8, [0.36, 0.3, 0.5, 0.25, 0.86, 0.34, 0.25, 0.84, 0.18, 0.0, 1.0, 0.0, 0.02, 0.0, -0.3]),
                (0.87, [0.34, 0.24, 0.75, 0.12, 1.02, 0.62, 0.15, 0.96, 0.44, 0.0, 1.0, 0.0, 0.0, 0.0, -0.4]),
                (0.92, [0.35, 0.26, 0.6, 0.18, 0.95, 0.5, 0.2, 0.9, 0.32, 0.0, 1.0, 0.0, 0.0, 0.0, -0.3]),
                (1.0, [0.38, 0.3, 0.0, 0.3, 0.74, 0.12, 0.3, 0.74, -0.12, 0.0, 1.0, 0.0, 0.03, -0.03, 0.0]),
            ];
            Act { feet: [[0.1, -0.24], [0.0, 0.24]], ..pick(keys(u, &k)) }
        }
        Work::Pick => {
            // bent over the bush, hands taking turns: one plucks and pulls to the pouch while the other reaches in
            let k: [(f32, K); 5] = [
                (0.0, [0.12, 0.5, -0.15, 0.64, 0.62, 0.12, 0.14, 0.88, -0.26, 0.0, 1.0, 0.0, 0.05, -0.05, 0.1]),
                (0.3, [0.13, 0.52, -0.12, 0.66, 0.58, 0.1, 0.4, 0.75, -0.2, 0.0, 1.0, 0.0, 0.05, -0.05, 0.05]),
                (IMPACT, [0.12, 0.46, 0.18, 0.12, 0.9, 0.27, 0.64, 0.62, -0.12, 0.0, 1.0, 0.0, 0.05, -0.05, -0.1]),
                (0.8, [0.13, 0.52, 0.12, 0.4, 0.75, 0.2, 0.66, 0.58, -0.1, 0.0, 1.0, 0.0, 0.05, -0.05, -0.05]),
                (1.0, [0.12, 0.5, -0.15, 0.64, 0.62, 0.12, 0.14, 0.88, -0.26, 0.0, 1.0, 0.0, 0.05, -0.05, 0.1]),
            ];
            Act { feet: [[0.1, -0.15], [-0.06, 0.15]], ..pick(keys(u, &k)) }
        }
        Work::Drink => {
            // down on one knee: cup the water, lift it to the mouth and tip the head back
            let k: [(f32, K); 5] = [
                (0.0, [0.45, 0.4, 0.0, 0.6, 0.1, 0.06, 0.6, 0.1, -0.06, 0.0, 1.0, 0.0, 0.06, -0.06, 0.0]),
                (0.3, [0.46, 0.42, 0.0, 0.63, 0.08, 0.05, 0.63, 0.08, -0.05, 0.0, 1.0, 0.0, 0.06, -0.06, 0.0]),
                (IMPACT, [0.42, 0.26, 0.0, 0.56, 1.08, 0.05, 0.56, 1.08, -0.05, 0.0, 1.0, 0.0, -0.02, 0.02, 0.0]),
                (0.85, [0.4, 0.14, 0.0, 0.46, 1.16, 0.05, 0.46, 1.16, -0.05, 0.0, 1.0, 0.0, -0.07, 0.03, 0.0]),
                (1.0, [0.45, 0.4, 0.0, 0.6, 0.1, 0.06, 0.6, 0.1, -0.06, 0.0, 1.0, 0.0, 0.06, -0.06, 0.0]),
            ];
            Act { feet: [[0.3, -0.13], [-0.36, 0.12]], kneel: [false, true], ..pick(keys(u, &k)) }
        }
        Work::Hammer => {
            // kneeling over the work: the left hand holds the core low, the right knocks it with a hammerstone
            let k: [(f32, K); 5] = [
                (0.0, [0.42, 0.36, 0.05, 0.36, 0.86, 0.15, 0.5, 0.42, -0.06, 0.0, 1.0, 0.0, 0.07, -0.07, 0.0]),
                (0.4, [0.41, 0.33, 0.08, 0.3, 0.98, 0.17, 0.5, 0.42, -0.06, 0.0, 1.0, 0.0, 0.07, -0.07, 0.0]),
                (IMPACT, [0.45, 0.44, -0.04, 0.5, 0.5, 0.0, 0.5, 0.4, -0.06, 0.0, 1.0, 0.0, 0.08, -0.08, 0.0]),
                (0.72, [0.44, 0.42, -0.02, 0.48, 0.57, 0.03, 0.5, 0.41, -0.06, 0.0, 1.0, 0.0, 0.08, -0.08, 0.0]),
                (1.0, [0.42, 0.36, 0.05, 0.36, 0.86, 0.15, 0.5, 0.42, -0.06, 0.0, 1.0, 0.0, 0.07, -0.07, 0.0]),
            ];
            Act { feet: [[0.3, -0.13], [-0.36, 0.12]], kneel: [false, true], ..pick(keys(u, &k)) }
        }
        Work::Throw => {
            // spear cocked back over the shoulder, free arm aimed at the prey, then a lunge and the release
            let k: [(f32, K); 4] = [
                (0.0, [0.04, 0.05, 0.35, 0.0, 1.55, 0.3, 0.45, 1.25, -0.2, 1.0, 0.08, 0.0, 0.0, 0.0, -0.35]),
                (0.5, [0.07, -0.12, 0.8, -0.32, 1.6, 0.32, 0.56, 1.42, -0.12, 1.0, 0.15, 0.0, -0.02, 0.0, -0.8]),
                (IMPACT, [0.1, 0.32, -0.35, 0.56, 1.4, 0.15, 0.0, 1.0, -0.3, 1.0, -0.06, 0.0, 0.04, -0.02, 0.35]),
                (1.0, [0.1, 0.3, -0.3, 0.54, 1.38, 0.15, 0.02, 1.0, -0.3, 1.0, -0.06, 0.0, 0.04, -0.02, 0.3]),
            ];
            Act { feet: [[0.32, -0.16], [-0.24, 0.16]], ..pick(keys(u, &k)) }
        }
        Work::Offer => {
            // crouched low and still, the hand held out with the berry, edging it closer and back
            let k: [(f32, K); 3] = [
                (0.0, [0.3, 0.3, 0.0, 0.5, 0.84, 0.12, 0.12, 0.74, -0.27, 0.0, 1.0, 0.0, 0.05, -0.03, 0.0]),
                (0.5, [0.32, 0.36, 0.0, 0.62, 0.9, 0.1, 0.12, 0.72, -0.27, 0.0, 1.0, 0.0, 0.07, -0.04, 0.0]),
                (1.0, [0.3, 0.3, 0.0, 0.5, 0.84, 0.12, 0.12, 0.74, -0.27, 0.0, 1.0, 0.0, 0.05, -0.03, 0.0]),
            ];
            Act { feet: [[0.18, -0.15], [-0.1, 0.15]], ..pick(keys(u, &k)) }
        }
        Work::None => pick([0.0; 15]),
    }
}

/// How a creature is moving this frame.
#[derive(Clone, Copy)]
pub(crate) struct Pose {
    pub(crate) heading: f32,
    pub(crate) phase: f32,
    pub(crate) amp: f32,
    /// Seconds, for work cycles.
    pub(crate) t: f32,
    pub(crate) work: Work,
    pub(crate) tool: Tool,
    /// Clothes worn (bit 1 wrap, 2 cloak, 4 boots) and the person's id, which varies skin and hair.
    pub(crate) outfit: u8,
    pub(crate) id: u32,
    /// How far the tribe has evolved, 0 (ape) to 5 (sage): posture, height and body hair follow.
    pub(crate) stage: u8,
    /// Rise of the ground along the heading (metres per metre): steep climbs and slides change the gait.
    pub(crate) slope: f32,
    /// Flinch after a blow, 1 to 0.
    pub(crate) hurt: f32,
    /// Foot swing in metres, matched to the walk cycle (see `step_walk`).
    pub(crate) stride: f32,
    /// Work motion: whole cycles done plus the fraction of the current one; each blow lands at `IMPACT`.
    pub(crate) cyc: f32,
    /// In the air: rising (up to 1) or falling (down to -1); 0 on the ground.
    pub(crate) air: f32,
}

impl Pose {
    pub(crate) fn idle(t: f32) -> Pose {
        Pose { heading: 0.0, phase: 0.0, amp: 0.0, t, work: Work::None, tool: Tool::Hand, outfit: 0, id: 0, stage: 5, slope: 0.0, hurt: 0.0, stride: 0.3, cyc: 0.0, air: 0.0 }
    }
}

/// What a creature is busy doing, drawn as a looping motion.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Work {
    None,
    Chop,
    Mine,
    Pick,
    Dig,
    Drink,
    Hammer,
    Throw,
    Offer,
}

/// What is held while working (the order matches the atlas: hand, stone axe, metal axe, metal pick, spear).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tool {
    Hand,
    StoneAxe,
    MetalAxe,
    MetalPick,
    Spear,
}

/// What this agent is doing right now (if it is close enough to the thing to actually be doing it).
pub(crate) fn work_of(world: &World, a: &alaz_sim::world::Agent) -> (Work, Tool, (f32, f32)) {
    use alaz_sim::world::Task;
    use alaz_sim::ItemKind as I;
    let has = |k| a.inventory.get(k) > 0;
    let m = |x: i64, y: i64| (x as f32 / 1000.0, y as f32 / 1000.0);
    let me = m(a.x, a.y);
    let near = |p: (f32, f32), r: f32| (p.0 - me.0).powi(2) + (p.1 - me.1).powi(2) <= r * r;
    match a.task {
        Task::Gather { node, .. } => {
            let Some(n) = world.nodes.get(node.0 as usize) else { return (Work::None, Tool::Hand, me) };
            let p = { let (x, y) = n.center_mm(); m(x, y) };
            if !near(p, 2.4) {
                return (Work::None, Tool::Hand, p);
            }
            match n.kind {
                NodeKind::Tree => {
                    let t = if has(I::IronAxe) || has(I::BronzeAxe) { Tool::MetalAxe } else if has(I::StoneAxe) { Tool::StoneAxe } else { Tool::Hand };
                    (Work::Chop, t, p)
                }
                NodeKind::StoneOutcrop | NodeKind::IronOutcrop | NodeKind::CopperOutcrop | NodeKind::TinOutcrop => {
                    let t = if has(I::IronPickaxe) || has(I::BronzePickaxe) { Tool::MetalPick } else if has(I::StoneAxe) { Tool::StoneAxe } else { Tool::Hand };
                    (Work::Mine, t, p)
                }
                NodeKind::ClayDeposit => (Work::Dig, Tool::Hand, p),
                _ => (Work::Pick, Tool::Hand, p),
            }
        }
        Task::Split { body, .. } => match world.bodies.iter().find(|b| b.id == body) {
            Some(b) => {
                let p = { let (x, y) = b.cut_point(); m(x, y) };
                // bent over the trunk on the ground, the axe coming down on it
                if b.lying() && near(p, 2.4) {
                    let t = if has(I::IronAxe) || has(I::BronzeAxe) { Tool::MetalAxe } else if has(I::StoneAxe) { Tool::StoneAxe } else { Tool::Hand };
                    (Work::Mine, t, p)
                } else {
                    (Work::None, Tool::Hand, p)
                }
            }
            None => (Work::None, Tool::Hand, me),
        },
        Task::Pick { body, .. } => match world.bodies.iter().find(|b| b.id == body) {
            Some(b) => {
                let p = m(b.x / alaz_sim::physics::Q, b.y / alaz_sim::physics::Q);
                if near(p, 1.2) { (Work::Pick, Tool::Hand, p) } else { (Work::None, Tool::Hand, p) }
            }
            None => (Work::None, Tool::Hand, me),
        },
        Task::Drink { tx, ty } => {
            let p = (tx as f32 + 0.5, ty as f32 + 0.5);
            if near(p, 2.4) { (Work::Drink, Tool::Hand, p) } else { (Work::None, Tool::Hand, p) }
        }
        Task::Craft { .. } => (Work::Hammer, Tool::Hand, (me.0 + 0.0, me.1 + 0.0)),
        Task::Hunt { wild, .. } => match world.wild.iter().find(|w| w.id == wild) {
            Some(w) => {
                let p = m(w.x, w.y);
                if near(p, 7.0) { (Work::Throw, Tool::Spear, p) } else { (Work::None, Tool::Hand, p) }
            }
            None => (Work::None, Tool::Hand, me),
        },
        Task::Tame { wild, .. } => match world.wild.iter().find(|w| w.id == wild) {
            Some(w) => {
                let p = m(w.x, w.y);
                if near(p, 3.0) { (Work::Offer, Tool::Hand, p) } else { (Work::None, Tool::Hand, p) }
            }
            None => (Work::None, Tool::Hand, me),
        },
        _ => (Work::None, Tool::Hand, me),
    }
}

/// Felled trees and loose rocks. A tree is drawn exactly as it stood, turned over on its stump; once down it settles
/// along the slope, kicks up dust, and after a while sinks away into the ground. Rocks tumble as they roll.
pub(crate) fn push_bodies(out: &mut Mesh, world: &World, alpha: f32, inside: &dyn Fn(f32, f32) -> bool) {
    use alaz_sim::physics::{BodyKind, HALF_PI_URAD, LOG_ROT_TICKS, Q};
    let q = Q as f32;
    for b in &world.bodies {
        let (bx, bz) = ((b.x as f32 + b.vx as f32 * alpha) / q / 1000.0, (b.y as f32 + b.vy as f32 * alpha) / q / 1000.0);
        if !inside(bx, bz) {
            continue;
        }
        match b.kind {
            BodyKind::Tree => {
                let node = alaz_sim::world::Node { kind: NodeKind::Tree, tx: b.tx, ty: b.ty, amount: 1 };
                let start = out.len();
                push_node(out, world, &node, world.perception_level());
                out.set_anchor(0.0, 0.0);
                out.foliage = false;
                let Some(v0) = out.get(start) else { continue };
                let (ax, az) = (v0.off[0], v0.off[1]);
                let (dx, dz) = (b.dx as f32 / 1000.0, b.dy as f32 / 1000.0);
                let len = b.size as f32 / 1000.0;
                // it comes to lie along the ground, not through it
                let g0 = ground_height(world.seed, ax, az);
                let tip_h = ground_height(world.seed, ax + dx * len * 0.8, az + dz * len * 0.8) - g0;
                let lie = std::f32::consts::FRAC_PI_2 - (tip_h / (len * 0.8)).atan();
                let raw = ((b.angle as f32 + b.spin as f32 * alpha) / HALF_PI_URAD as f32).min(1.0);
                let th = raw * lie;
                let (sn, cs) = th.sin_cos();
                // left to rot, it sinks into the earth over its last ten seconds
                let left = LOG_ROT_TICKS.saturating_sub(b.rest) as f32 / 600.0;
                let sink = (1.0 - left).clamp(0.0, 1.0) * 0.9;
                // broken up from the crown end: what has been cut away is gone (whole triangles past the cut)
                let cut = b.trunk_left_mm() as f32 / 1000.0;
                let broken = b.wood < b.full;
                let mut along_of = Vec::with_capacity(out.len() - start);
                for v in &mut out[start..] {
                    let (px, pz) = (v.pos[0] - ax, v.pos[2] - az);
                    let (along, up) = (px * dx + pz * dz, v.pos[1]);
                    along_of.push(up);
                    let (sx, sz) = (px - along * dx, pz - along * dz);
                    let (na, nu) = (along * cs + up * sn, up * cs - along * sn);
                    v.pos = [ax + sx + na * dx, nu - sink, az + sz + na * dz];
                }
                if broken {
                    // standing, "up" was along the trunk; keep the triangles below the cut
                    let mut keep = start;
                    for t in 0..(out.len() - start) / 3 {
                        let i = start + t * 3;
                        let far = along_of[t * 3].max(along_of[t * 3 + 1]).max(along_of[t * 3 + 2]);
                        if far <= cut + 0.05 {
                            out.copy_within(i..i + 3, keep);
                            keep += 3;
                        }
                    }
                    out.truncate(keep);
                    // the fresh cut face, pale wood
                    let r = 0.06 + len * 0.022;
                    out.set_anchor(ax, az);
                    shapes::sphere(out, [ax + dx * cut, r * 0.9 - sink, az + dz * cut], [r * 0.45, r * 0.95, r * 0.95], [0.86, 0.72, 0.48], 6, 4, 0.0, 5);
                }
                // dust where it hit, for a moment
                if b.angle >= HALF_PI_URAD && b.rest < 36 {
                    // small puffs thrown out along the trunk, rising, spreading and thinning out
                    let k = b.rest as f32 / 36.0;
                    out.set_anchor(ax, az);
                    for i in 0..8u32 {
                        let t = 0.2 + 0.8 * i as f32 / 7.0;
                        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
                        let r = (0.14 + 0.1 * k) * (1.0 - k * k);
                        let spread = 0.25 + 0.6 * k;
                        let p = [ax + dx * len * t - dz * side * spread, 0.08 + 0.45 * k, az + dz * len * t + dx * side * spread];
                        shapes::sphere(out, p, [r, r * 0.7, r], [0.72, 0.68, 0.6], 5, 3, 0.3, i);
                    }
                }
            }
            BodyKind::Piece => {
                // a short split length of wood, pale where it broke, lying at its own angle and turning as it rolls
                let z_m = (b.z as f32 + b.vz as f32 * alpha) / q / 1000.0;
                let lift = z_m - ground_height(world.seed, bx, bz);
                let h = b.id.wrapping_mul(2_654_435_761);
                let a = (h % 628) as f32 / 100.0 + b.angle as f32 / 150.0;
                let (ox, oz) = (a.cos() * 0.2, a.sin() * 0.2);
                out.set_anchor(bx, bz);
                let y = 0.07 + lift.max(0.0);
                let item = alaz_sim::map::ItemKind::ALL.get(b.item as usize).copied().unwrap_or(alaz_sim::map::ItemKind::Wood);
                // afloat: it bobs on the swell
                let afloat = world.map.terrain(bx.floor() as i32, bz.floor() as i32) == Some(alaz_sim::map::Terrain::Water);
                let lift = if afloat { lift.max(0.0) + 0.04 + 0.05 * (world.tick as f32 / 25.0 + b.id as f32).sin() } else { lift };
                if item != alaz_sim::map::ItemKind::Wood {
                    // something put down: a small bundle in a colour of its own, with a pale halo on the ground
                    let col = piece_colour(item);
                    let y0 = lift.max(0.0);
                    use alaz_sim::map::ItemKind as I;
                    match item {
                        I::Branch | I::Plank | I::Spear => {
                            shapes::limb(out, [bx - ox * 2.4, y0 + 0.05, bz - oz * 2.4], [bx + ox * 2.4, y0 + 0.07, bz + oz * 2.4], 0.045, 0.03, col, 5);
                        }
                        I::Fiber | I::Herb => {
                            for k in 0..6u32 {
                                let t = (k as f32 - 2.5) * 0.04;
                                shapes::limb(out, [bx - ox * 2.0 + t, y0 + 0.04, bz - oz * 2.0 - t], [bx + ox * 2.0 + t, y0 + 0.05, bz + oz * 2.0 - t], 0.018, 0.01, col, 3);
                            }
                        }
                        I::Stone | I::Flint | I::SharpStone | I::CopperOre | I::TinOre | I::IronOre | I::Coal | I::Limestone => {
                            shapes::sphere(out, [bx, y0 + 0.1, bz], [0.17, 0.12, 0.15], col, 6, 4, 0.25, h);
                        }
                        I::Berries => {
                            for k in 0..5u32 {
                                let a2 = k as f32 * 1.26;
                                shapes::sphere(out, [bx + a2.cos() * 0.08, y0 + 0.06, bz + a2.sin() * 0.08], [0.06; 3], col, 5, 3, 0.0, k);
                            }
                        }
                        _ => {
                            // a tied bundle
                            shapes::sphere(out, [bx, y0 + 0.14, bz], [0.2, 0.15, 0.2], col, 7, 4, 0.12, h);
                            shapes::sphere(out, [bx, y0 + 0.3, bz], [0.06, 0.05, 0.06], [col[0] * 0.7, col[1] * 0.7, col[2] * 0.7], 5, 3, 0.0, 2);
                        }
                    }
                    continue;
                }
                let bark = [0.38 + 0.04 * ((h >> 9) % 3) as f32, 0.26, 0.15];
                shapes::limb(out, [bx - ox, y, bz - oz], [bx + ox, y, bz + oz], 0.07, 0.065, bark, 6);
                shapes::sphere(out, [bx + ox, y, bz + oz], [0.055; 3], [0.86, 0.72, 0.48], 5, 3, 0.0, 1);
                shapes::sphere(out, [bx - ox, y, bz - oz], [0.05; 3], [0.8, 0.66, 0.44], 5, 3, 0.0, 2);
                out.set_anchor(0.0, 0.0);
            }
            BodyKind::Rock => {
                let r = b.size as f32 / 1000.0;
                let z_m = (b.z as f32 + b.vz as f32 * alpha) / q / 1000.0;
                let lift = z_m - ground_height(world.seed, bx, bz);
                out.set_anchor(bx, bz);
                let start = out.len();
                let seed = (b.size as u32).wrapping_mul(2_654_435_761);
                let g = 0.5 + 0.06 * ((seed >> 7) % 4) as f32;
                shapes::rock(out, [bx, r, bz], [r * 1.1, r * 0.85, r], [g, g * 0.98, g * 0.95], seed);
                // tumbling: turned about two axes by how far it has rolled
                let roll = b.angle as f32 / 1000.0 / r.max(0.05);
                let (s1, c1) = (roll).sin_cos();
                let (s2, c2) = (roll * 0.63).sin_cos();
                for v in &mut out[start..] {
                    let (px, py, pz) = (v.pos[0] - bx, v.pos[1] - r, v.pos[2] - bz);
                    let (py, pz) = (py * c1 - pz * s1, py * s1 + pz * c1);
                    let (px, py) = (px * c2 - py * s2, px * s2 + py * c2);
                    v.pos = [bx + px, r + py + lift, bz + pz];
                }
                out.set_anchor(0.0, 0.0);
            }
        }
    }
}

/// The colour a dropped item lies on the ground in.
pub(crate) fn piece_colour(k: alaz_sim::map::ItemKind) -> [f32; 3] {
    use alaz_sim::map::ItemKind as I;
    match k {
        I::Stone | I::Flint | I::SharpStone => [0.55, 0.55, 0.55],
        I::Fiber | I::Cloth => [0.86, 0.78, 0.42],
        I::Branch | I::Plank => [0.55, 0.40, 0.24],
        I::Berries => [0.78, 0.12, 0.22],
        I::RawMeat | I::CookedMeat => [0.70, 0.30, 0.25],
        I::Clay | I::Pot | I::LargePot => [0.66, 0.44, 0.30],
        I::CopperOre | I::CopperIngot | I::CopperWire => [0.78, 0.46, 0.26],
        I::IronOre | I::Steel => [0.42, 0.42, 0.46],
        I::Coal | I::Charcoal | I::Coke => [0.12, 0.12, 0.12],
        _ => [0.80, 0.70, 0.52],
    }
}
