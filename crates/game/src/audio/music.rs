//! Generated music: a slow pad that changes chord every sixteen seconds, under a sparse pentatonic melody, in a
//! soft echo. Nothing is recorded. The scene shapes it: by day the melody is brighter and more frequent, at night it
//! drops an octave and thins out, in a cave it falls almost silent and only the pad is left.

use super::{pan, Lp, Rng, Scene, SR};

/// Seconds one chord lasts.
const CHORD_SECS: f32 = 16.0;
/// Hz of the lowest root (A2).
const ROOT_HZ: f32 = 110.0;
/// Chords as (root offset in semitones from A, third and fifth above it): Am, F, C, G.
const PROGRESSION: [(i32, [i32; 3]); 4] = [(0, [0, 3, 7]), (-4, [0, 4, 7]), (3, [0, 4, 7]), (-2, [0, 4, 7])];
/// The minor pentatonic, in semitones above the chord's key note (A).
const SCALE: [i32; 5] = [0, 3, 5, 7, 10];
/// Echo lengths in samples (left, right): different, so the repeats drift across the picture.
const ECHO: (usize, usize) = ((0.37 * SR) as usize, (0.43 * SR) as usize);
const MAX_PLUCKS: usize = 5;

fn hz(semitones: f32) -> f32 {
    ROOT_HZ * (semitones / 12.0).exp2()
}

/// One struck note of the melody.
struct Pluck {
    freq: f32,
    phase: f32,
    age: f32,
    pan: f32,
    amp: f32,
}

pub(super) struct Music {
    rng: Rng,
    now: Scene,
    target: Scene,
    /// Samples played.
    n: u64,
    /// Pad oscillators: two slightly apart for each of three chord tones, with their phase and the pitch they are gliding to.
    pad_phase: [f32; 6],
    pad_freq: [f32; 6],
    pad_goal: [f32; 6],
    /// Seconds until the next melody note.
    wait: f32,
    /// Where the melody stands in the scale (steps wander by one or two).
    step: i32,
    plucks: Vec<Pluck>,
    dark: [Lp; 2],
    echo_l: Vec<f32>,
    echo_r: Vec<f32>,
    echo_at: usize,
}

impl Music {
    pub(super) fn new() -> Self {
        let mut m = Music {
            rng: Rng(0x9E37_79B9),
            now: Scene::default(),
            target: Scene::default(),
            n: 0,
            pad_phase: [0.0; 6],
            pad_freq: [0.0; 6],
            pad_goal: [0.0; 6],
            wait: 4.0,
            step: 2,
            plucks: Vec::new(),
            dark: Default::default(),
            echo_l: vec![0.0; ECHO.0],
            echo_r: vec![0.0; ECHO.1],
            echo_at: 0,
        };
        m.set_chord(0);
        m.pad_freq = m.pad_goal;
        m
    }

    pub(super) fn set(&mut self, scene: Scene) {
        self.target = scene;
    }

    /// Aim the pad at the chord with this index.
    fn set_chord(&mut self, i: usize) {
        let (root, tones) = PROGRESSION[i % PROGRESSION.len()];
        for (t, tone) in tones.iter().enumerate() {
            let f = hz((root + tone) as f32);
            self.pad_goal[t * 2] = f * 0.9975;
            self.pad_goal[t * 2 + 1] = f * 1.0025;
        }
    }

    fn chord_index(&self) -> usize {
        (self.n as f32 / SR / CHORD_SECS) as usize % PROGRESSION.len()
    }

    /// Every 256 samples: ease the scene in, change chords, decide when the next note falls.
    fn tick(&mut self) {
        let dt = 256.0 / SR;
        let k = 1.0 - (-dt / 1.5).exp();
        let (a, b) = (&mut self.now, self.target);
        a.day += (b.day - a.day) * k;
        a.rain += (b.rain - a.rain) * k;
        a.fire += (b.fire - a.fire) * k;
        a.cave = b.cave;
        let chord = self.chord_index();
        self.set_chord(chord);
        self.wait -= dt;
        if self.wait <= 0.0 && self.plucks.len() < MAX_PLUCKS {
            self.strike();
        }
    }

    /// Mean seconds between notes: brisk by day, slow by night, very rare in a cave.
    fn gap(&self) -> f32 {
        if self.now.cave {
            return 40.0;
        }
        let day = self.now.day.clamp(0.0, 1.0);
        14.0 - 8.0 * day
    }

    fn strike(&mut self) {
        let r = &mut self.rng;
        // a short walk along the scale, one or two steps either way, pulled back if it strays
        let mut step = self.step + if r.f() < 0.5 { -1 } else { 1 } * if r.f() < 0.35 { 2 } else { 1 };
        if !(-2..=7).contains(&step) {
            step = 2;
        }
        self.step = step;
        let octave = step.div_euclid(5);
        let semis = SCALE[step.rem_euclid(5) as usize] + 12 * octave;
        // two octaves above the pad's root by day, one at night
        let lift = if self.now.day > 0.5 { 24 } else { 12 };
        let amp = if self.now.cave { 0.05 } else { 0.09 + 0.05 * self.now.day };
        self.plucks.push(Pluck { freq: hz((semis + lift) as f32), phase: 0.0, age: 0.0, pan: r.range(-0.6, 0.6), amp });
        self.wait = r.range(0.6, 1.4) * self.gap();
    }

    /// One stereo sample of music, about 0 to 0.3 at its loudest.
    pub(super) fn frame(&mut self) -> (f32, f32) {
        if self.n.is_multiple_of(256) {
            self.tick();
        }
        self.n += 1;
        let tau = std::f32::consts::TAU;
        // the pad: each tone glides to its chord (a slow portamento), and breathes slowly
        let t = self.n as f32 / SR;
        let breathe = 0.75 + 0.25 * (t * tau / 11.0).sin();
        let mut pad = (0.0, 0.0);
        for i in 0..6 {
            self.pad_freq[i] += (self.pad_goal[i] - self.pad_freq[i]) * 0.00004;
            self.pad_phase[i] = (self.pad_phase[i] + self.pad_freq[i] / SR).fract();
            let s = (self.pad_phase[i] * tau).sin() * 0.026 * breathe;
            // spread the tones: low ones in the middle, upper ones out to the sides
            let (l, r) = pan(s, [-0.2, 0.2, -0.45, 0.45, -0.7, 0.7][i]);
            pad.0 += l;
            pad.1 += r;
        }
        // the melody
        let mut mel = (0.0, 0.0);
        for p in &mut self.plucks {
            p.phase = (p.phase + p.freq / SR).fract();
            p.age += 1.0 / SR;
            let env = (1.0 - (-p.age * 60.0).exp()) * (-p.age * 1.1).exp();
            let s = ((p.phase * tau).sin() + 0.25 * (p.phase * tau * 2.0).sin()) * p.amp * env;
            let (l, r) = pan(s, p.pan);
            mel.0 += l;
            mel.1 += r;
        }
        self.plucks.retain(|p| p.age < 6.0);
        // in a cave everything is duller; rain keeps it a little lower
        let hz_cut = if self.now.cave { 500.0 } else { 3500.0 - 1500.0 * self.now.rain };
        let (l, r) = (self.dark[0].run(pad.0 + mel.0, hz_cut), self.dark[1].run(pad.1 + mel.1, hz_cut));
        // a soft echo, each side feeding the other a little
        let (el, er) = (self.echo_l[self.echo_at % ECHO.0], self.echo_r[self.echo_at % ECHO.1]);
        self.echo_l[self.echo_at % ECHO.0] = l + er * 0.4;
        self.echo_r[self.echo_at % ECHO.1] = r + el * 0.4;
        self.echo_at += 1;
        (l + el * 0.38, r + er * 0.38)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(scene: Scene, secs: f32) -> Vec<(f32, f32)> {
        let mut m = Music::new();
        m.set(scene);
        (0..(secs * SR) as usize).map(|_| m.frame()).collect()
    }

    fn rms(v: &[(f32, f32)]) -> f32 {
        (v.iter().map(|(l, r)| l * l + r * r).sum::<f32>() / (2.0 * v.len() as f32)).sqrt()
    }

    #[test]
    fn it_plays_something_gentle_and_never_clips() {
        let day = Scene { day: 1.0, ..Scene::default() };
        let v = play(day, 40.0);
        let peak = v.iter().map(|(l, r)| l.abs().max(r.abs())).fold(0.0f32, f32::max);
        assert!(rms(&v) > 0.003, "audible: {}", rms(&v));
        assert!(peak < 0.4, "leaves room for the rest of the mix: {peak}");
        assert!(v.iter().all(|(l, r)| l.is_finite() && r.is_finite()));
    }

    #[test]
    fn it_is_the_same_every_run() {
        let day = Scene { day: 1.0, ..Scene::default() };
        assert_eq!(play(day, 8.0), play(day, 8.0));
    }

    #[test]
    fn a_cave_is_duller_and_quieter_than_the_open_day() {
        let day = rms(&play(Scene { day: 1.0, ..Scene::default() }, 60.0));
        let cave = rms(&play(Scene { day: 1.0, cave: true, ..Scene::default() }, 60.0));
        assert!(cave < day, "cave {cave} against day {day}");
    }

    #[test]
    fn the_chord_changes_and_the_pad_follows_it() {
        let mut m = Music::new();
        let first = m.pad_goal;
        m.n = (CHORD_SECS * SR) as u64 + 1;
        m.tick();
        assert_ne!(first, m.pad_goal, "the second chord has other notes");
        m.n = (CHORD_SECS * 4.0 * SR) as u64 + 1;
        m.tick();
        assert_eq!(first, m.pad_goal, "after four chords it comes round again");
    }
}
