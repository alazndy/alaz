//! The evolution of the body, as one continuous number. The tribe's perception (0..=1000 permille) is mapped onto
//! a stage coordinate 0..=9: each of the ten named stages starts at a whole number, and everything between two
//! stages is a smooth blend of their looks. The body and the portrait both read the same `Evo`, so a posture, a
//! coat of fur or a grey hair never jumps: it only ever slides.

/// Named stages of the body, from the first ape to the sage.
pub const EVO_STAGES: usize = 10;

/// Perception (permille) at which each stage starts. The five gameplay steps (`PERCEPTION_STEPS`: 80, 220, 420, 640,
/// 860) are stage starts too, so a change in what the hands can do always arrives with a new look.
pub const EVO_STARTS: [u32; EVO_STAGES] = [0, 45, 80, 150, 220, 320, 420, 540, 640, 860];

/// The stage coordinate (0..=9, fractional) for a perception in permille: stage `i` starts at `i`, and the way to
/// the next one is a straight line in permille.
pub fn evo_pos(permille: f32) -> f32 {
    let p = permille.clamp(0.0, EVO_STARTS[EVO_STAGES - 1] as f32);
    let i = (0..EVO_STAGES - 1).rev().find(|&i| p >= EVO_STARTS[i] as f32).unwrap_or(0);
    let (a, b) = (EVO_STARTS[i] as f32, EVO_STARTS[i + 1] as f32);
    i as f32 + ((p - a) / (b - a)).clamp(0.0, 1.0)
}

/// The named stage (0..=9) a perception in permille is in.
pub fn evo_stage(permille: u32) -> usize {
    EVO_STARTS.iter().rposition(|&t| permille >= t).unwrap_or(0)
}

/// Seconds the moment of evolving lasts.
pub const EVOLVE_SECS: f32 = 3.2;

/// How far past its own stage the body looks `age` seconds into the moment of evolving (in stage steps): it rises
/// toward the next look and settles back, like drawing a breath. 0 before and after.
pub fn evolve_bump(age: f32) -> f32 {
    if !(0.0..EVOLVE_SECS).contains(&age) {
        return 0.0;
    }
    let s = (std::f32::consts::PI * age / EVOLVE_SECS).sin();
    0.8 * s * s
}

/// Smooth 0..1 ramp between `a` and `b` (flat outside, zero slope at both ends).
pub fn smooth(a: f32, b: f32, v: f32) -> f32 {
    let t = ((v - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A value keyed to each stage, blended between neighbours (half straight, half eased, so it neither stalls at
/// the stages nor turns with a kink there).
fn key(v: f32, k: &[f32; EVO_STAGES]) -> f32 {
    let v = v.clamp(0.0, (EVO_STAGES - 1) as f32);
    let i = (v.floor() as usize).min(EVO_STAGES - 2);
    let t = v - i as f32;
    let e = t * t * (3.0 - 2.0 * t);
    k[i] + (k[i + 1] - k[i]) * (0.5 * t + 0.5 * e)
}

/// Everything about the body that changes with the stages. 1 is the ape's end of a channel, 0 the sage's; the
/// accessories are 0 until their stage and fade again where the next fashion takes over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Evo {
    /// The stage coordinate itself.
    pub v: f32,
    /// Forward bend of the back and the lean of the walk.
    pub stoop: f32,
    /// Body hair (chest, back, limbs).
    pub fur: f32,
    /// The shaggy mane on the head, which the person's own cut replaces as it thins.
    pub shag: f32,
    /// Heavy brow ridge and deep-set eyes; a low forehead.
    pub brow: f32,
    /// How far the muzzle (jaw, nose, mouth) juts out.
    pub muzzle: f32,
    /// How low the arms hang.
    pub arm: f32,
    /// Leg length against the trunk (1 at the sage).
    pub leg: f32,
    /// Hair greying.
    pub gray: f32,
    /// A beard that grows on some from the early stages...
    pub beard_grow: f32,
    /// ... and a long white one on every sage.
    pub beard_all: f32,
    /// Belt and pouch.
    pub belt: f32,
    /// Ochre paint on the cheeks and chest.
    pub ochre: f32,
    /// A carrying sack on the back.
    pub sack: f32,
    /// A necklace of teeth.
    pub tooth: f32,
    /// A straw hat.
    pub hat: f32,
    /// Leather cuffs and a hammer at the belt.
    pub tools: f32,
    /// A dyed sash.
    pub sash: f32,
    /// A gold amulet.
    pub amulet: f32,
}

impl Evo {
    /// The body at stage coordinate `v` (clamped to 0..=9).
    pub fn at(v: f32) -> Evo {
        let v = if v.is_finite() { v.clamp(0.0, (EVO_STAGES - 1) as f32) } else { 0.0 };
        // accessory window: rises over `a..b`, falls over `c..d`
        let win = |a: f32, b: f32, c: f32, d: f32| smooth(a, b, v) * (1.0 - smooth(c, d, v));
        Evo {
            v,
            stoop: key(v, &[1.0, 0.97, 0.93, 0.87, 0.74, 0.55, 0.38, 0.25, 0.12, 0.0]),
            fur: key(v, &[1.0, 0.98, 0.94, 0.86, 0.70, 0.50, 0.30, 0.14, 0.05, 0.0]),
            shag: key(v, &[1.0, 0.97, 0.90, 0.75, 0.50, 0.25, 0.05, 0.0, 0.0, 0.0]),
            brow: key(v, &[1.0, 0.97, 0.92, 0.84, 0.70, 0.52, 0.34, 0.20, 0.09, 0.0]),
            muzzle: key(v, &[1.0, 0.95, 0.88, 0.78, 0.62, 0.42, 0.24, 0.11, 0.04, 0.0]),
            arm: key(v, &[1.0, 0.96, 0.90, 0.82, 0.68, 0.50, 0.34, 0.20, 0.08, 0.0]),
            leg: key(v, &[0.84, 0.85, 0.865, 0.885, 0.915, 0.95, 0.98, 1.0, 1.0, 1.0]),
            gray: smooth(7.6, 9.0, v),
            beard_grow: smooth(2.0, 5.0, v),
            beard_all: smooth(8.0, 9.0, v),
            belt: smooth(1.6, 3.6, v),
            ochre: win(2.0, 3.0, 4.6, 6.2),
            sack: win(3.0, 4.0, 6.4, 7.4),
            tooth: win(4.0, 5.0, 7.4, 8.6),
            hat: win(5.0, 6.0, 6.9, 7.9),
            tools: win(6.0, 7.0, 8.0, 9.0),
            sash: win(7.0, 8.0, 8.5, 9.0),
            amulet: smooth(8.0, 9.0, v),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pos_runs_through_the_stages() {
        assert_eq!(evo_pos(0.0), 0.0);
        for (i, &s) in EVO_STARTS.iter().enumerate() {
            assert!((evo_pos(s as f32) - i as f32).abs() < 1e-5, "{i}");
            assert_eq!(evo_stage(s), i);
        }
        assert_eq!(evo_pos(1000.0), 9.0);
        // always rising, never jumping
        let mut last = 0.0;
        for p in 0..=1000 {
            let v = evo_pos(p as f32);
            assert!(v >= last && v - last < 0.05, "{p}");
            last = v;
        }
    }

    /// The ape lasts longer: through the fourth stage (perception 150) the body is still mostly an ape's, and the
    /// change that makes a person is spread over the stages after it.
    #[test]
    fn the_body_stays_an_ape_longer_and_never_turns_back() {
        let at3 = Evo::at(3.0);
        for (name, v) in [("stoop", at3.stoop), ("fur", at3.fur), ("shag", at3.shag), ("brow", at3.brow), ("muzzle", at3.muzzle), ("arm", at3.arm)] {
            assert!(v >= 0.75, "{name} is still an ape's at stage 3: {v}");
        }
        let mut last = Evo::at(0.0);
        let mut v = 0.0;
        while v < 9.0 {
            v += 0.01;
            let e = Evo::at(v);
            for (a, b) in [(last.stoop, e.stoop), (last.fur, e.fur), (last.shag, e.shag), (last.brow, e.brow), (last.muzzle, e.muzzle), (last.arm, e.arm)] {
                assert!(b <= a + 1e-6, "an ape channel only ever falls: {v}: {a} -> {b}");
            }
            assert!(e.leg >= last.leg - 1e-6);
            last = e;
        }
    }

    #[test]
    fn the_moment_of_evolving_rises_and_settles() {
        assert_eq!(evolve_bump(-1.0), 0.0);
        assert_eq!(evolve_bump(0.0), 0.0);
        assert_eq!(evolve_bump(EVOLVE_SECS), 0.0);
        assert_eq!(evolve_bump(EVOLVE_SECS + 5.0), 0.0);
        let peak = (0..320).map(|i| evolve_bump(i as f32 / 100.0)).fold(0.0f32, f32::max);
        assert!((peak - 0.8).abs() < 0.01, "it peaks at 0.8 of a stage: {peak}");
        let mut last = 0.0;
        for i in 0..160 {
            let b = evolve_bump(i as f32 / 100.0);
            assert!(b >= last - 1e-6, "rising to the middle");
            last = b;
        }
    }

    #[test]
    fn channels_slide_smoothly() {
        let mut last = Evo::at(0.0);
        let mut v = 0.0;
        while v < 9.0 {
            v += 0.01;
            let e = Evo::at(v);
            for (a, b) in [(last.stoop, e.stoop), (last.fur, e.fur), (last.shag, e.shag), (last.brow, e.brow), (last.muzzle, e.muzzle), (last.arm, e.arm), (last.leg, e.leg), (last.gray, e.gray), (last.belt, e.belt), (last.hat, e.hat), (last.sash, e.sash), (last.tooth, e.tooth), (last.ochre, e.ochre)] {
                assert!((a - b).abs() < 0.03, "jump at {v}: {a} -> {b}");
            }
            last = e;
        }
        // the ends: an ape and a sage
        let (a, s) = (Evo::at(0.0), Evo::at(9.0));
        assert!(a.stoop == 1.0 && a.fur == 1.0 && s.stoop == 0.0 && s.fur == 0.0 && s.leg == 1.0 && s.gray == 1.0);
    }
}
