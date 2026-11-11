//! Sound, all synthesized in code (no asset files) and streamed as raw stereo PCM to the system audio server
//! through a long-lived `pw-cat` (PipeWire) or `aplay` process: short effects for game events, and a live
//! ambience (wind, leaves, rain, birds, crickets, owls, water, fire) that follows where the player stands, the
//! hour and the weather. Everything is best effort: with no player available or `ALAZ_MUTE` set, nothing plays.

#[cfg(unix)]
use std::io::Write;
#[cfg(unix)]
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

const RATE: u32 = 44_100;
const SR: f32 = RATE as f32;
/// Frames written per mixer step (10 ms).
const CHUNK: usize = 441;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sfx {
    Hit,
    Miss,
    Chime,
    Fanfare,
    Fail,
    Hurt,
    Ding,
    Tame,
    Bark,
    /// A body hitting the ground from a height.
    Thud,
    /// A tree creaking over and crashing down.
    Timber,
    /// Rock knocking on rock.
    Rumble,
    /// An axe biting into wood.
    Chop,
}

const ALL: [Sfx; 13] = [Sfx::Hit, Sfx::Miss, Sfx::Chime, Sfx::Fanfare, Sfx::Fail, Sfx::Hurt, Sfx::Ding, Sfx::Tame, Sfx::Bark, Sfx::Thud, Sfx::Timber, Sfx::Rumble, Sfx::Chop];

pub struct Audio {
    tx: Sender<Msg>,
    pub muted: bool,
}

enum Msg {
    Play(Sfx),
    Mute(bool),
    Scene(Scene),
}

/// What is around the player right now; every value is 0..1.
#[derive(Clone, Copy, Debug, Default)]
pub struct Scene {
    pub wind: f32,
    pub rain: f32,
    /// 0 at night, 1 in full daylight.
    pub day: f32,
    /// How much woodland is around.
    pub trees: f32,
    /// How close open water is.
    pub water: f32,
    /// How close a burning fire is.
    pub fire: f32,
    /// Inside a cave: the open-air sounds fall away.
    pub cave: bool,
}



/// Tiny deterministic noise source so the effects sound the same every run.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

fn tone(freq: f32, secs: f32, decay: f32, vol: f32, start: &mut Vec<f32>, at: f32) {
    let (a, n) = ((at * RATE as f32) as usize, (secs * RATE as f32) as usize);
    if start.len() < a + n {
        start.resize(a + n, 0.0);
    }
    for i in 0..n {
        let t = i as f32 / RATE as f32;
        let env = (-t * decay).exp() * (1.0 - (-t * 400.0).exp());
        start[a + i] += (2.0 * std::f32::consts::PI * freq * t).sin() * env * vol;
    }
}

fn noise_burst(secs: f32, decay: f32, vol: f32, lowpass: f32, out: &mut Vec<f32>) {
    let mut n = Noise(0x1234_5678);
    let len = (secs * RATE as f32) as usize;
    if out.len() < len {
        out.resize(len, 0.0);
    }
    let mut y = 0.0;
    for i in 0..len {
        let t = i as f32 / RATE as f32;
        y += (n.next() - y) * lowpass;
        out[i] += y * (-t * decay).exp() * vol;
    }
}

fn synth(s: Sfx) -> Vec<f32> {
    let mut v = Vec::new();
    match s {
        Sfx::Hit => {
            noise_burst(0.09, 55.0, 0.8, 0.55, &mut v);
            tone(190.0, 0.12, 35.0, 0.55, &mut v, 0.0);
            tone(1250.0, 0.04, 90.0, 0.2, &mut v, 0.0);
        }
        Sfx::Miss => {
            noise_burst(0.07, 70.0, 0.45, 0.12, &mut v);
            tone(110.0, 0.08, 50.0, 0.25, &mut v, 0.0);
        }
        Sfx::Bark => {
            // two short yips, the second a little lower
            noise_burst(0.05, 40.0, 0.5, 0.2, &mut v);
            tone(520.0, 0.12, 22.0, 0.35, &mut v, 0.0);
            tone(430.0, 0.14, 20.0, 0.35, &mut v, 0.22);
        }
        Sfx::Chime => {
            tone(660.0, 0.35, 9.0, 0.3, &mut v, 0.0);
            tone(990.0, 0.35, 9.0, 0.22, &mut v, 0.07);
        }
        Sfx::Fanfare => {
            for (i, f) in [523.0, 659.0, 784.0, 1047.0].into_iter().enumerate() {
                tone(f, 0.45, 6.0, 0.28, &mut v, i as f32 * 0.11);
            }
        }
        Sfx::Fail => {
            tone(300.0, 0.18, 12.0, 0.3, &mut v, 0.0);
            tone(210.0, 0.28, 10.0, 0.3, &mut v, 0.14);
        }
        Sfx::Hurt => {
            noise_burst(0.12, 30.0, 0.5, 0.3, &mut v);
            for k in 0..3 {
                tone(260.0 - k as f32 * 50.0, 0.15, 14.0, 0.3, &mut v, k as f32 * 0.05);
            }
        }
        Sfx::Ding => tone(880.0, 0.25, 14.0, 0.22, &mut v, 0.0),
        Sfx::Chop => {
            // a dry knock with a short woody ring: the bite, then the wood answering
            noise_burst(0.06, 70.0, 0.9, 0.0, &mut v);
            tone(180.0, 0.12, 38.0, 0.55, &mut v, 0.0);
            tone(310.0, 0.07, 55.0, 0.25, &mut v, 0.004);
        }
        Sfx::Thud => {
            noise_burst(0.25, 18.0, 0.9, 0.08, &mut v);
            tone(62.0, 0.3, 11.0, 0.7, &mut v, 0.0);
            tone(95.0, 0.15, 25.0, 0.3, &mut v, 0.0);
        }
        Sfx::Timber => {
            // the crash of the trunk and the swish and crack of the crown all at once, then a low boom
            noise_burst(0.9, 4.5, 0.85, 0.22, &mut v);
            tone(48.0, 0.9, 4.0, 0.75, &mut v, 0.0);
            tone(140.0, 0.25, 16.0, 0.3, &mut v, 0.02);
            for k in 0..4 {
                tone(900.0 + 260.0 * k as f32, 0.04, 80.0, 0.12, &mut v, 0.05 + 0.07 * k as f32);
            }
        }
        Sfx::Rumble => {
            noise_burst(0.18, 22.0, 0.55, 0.15, &mut v);
            tone(120.0, 0.14, 26.0, 0.4, &mut v, 0.0);
            tone(310.0, 0.05, 60.0, 0.18, &mut v, 0.0);
        }
        Sfx::Tame => {
            for (i, f) in [440.0, 554.0, 659.0].into_iter().enumerate() {
                tone(f, 0.3, 8.0, 0.25, &mut v, i as f32 * 0.09);
            }
        }
    }
    v
}

/// Where the mixed sound goes: called with each 10 ms of interleaved stereo; false when the output is gone.
type Sink = Box<dyn FnMut(&[i16]) -> bool>;

/// Linux and the like: a long-lived `pw-cat` or `aplay` reading raw PCM on its stdin.
#[cfg(unix)]
fn open_sink() -> Option<Sink> {
    let mut child = spawn_player()?;
    let mut stdin = child.stdin.take()?;
    // a small pipe keeps the delay between a game event and its sound short
    {
        use std::os::fd::AsRawFd;
        unsafe {
            libc::fcntl(stdin.as_raw_fd(), libc::F_SETPIPE_SZ, 4096);
        }
    }
    Some(Box::new(move |buf: &[i16]| {
        let bytes: Vec<u8> = buf.iter().flat_map(|s| s.to_le_bytes()).collect();
        if stdin.write_all(&bytes).is_err() {
            let _ = child.kill();
            return false;
        }
        true
    }))
}

/// Windows: the default output device through cpal; the mixer's 44.1 kHz stereo is fed through a short queue and
/// stepped to the device's own rate and channel count.
#[cfg(windows)]
fn open_sink() -> Option<Sink> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    let dev = cpal::default_host().default_output_device()?;
    let supported = dev.default_output_config().ok()?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let channels = config.channels as usize;
    let step = RATE as f32 / config.sample_rate.0 as f32;
    let queue: Arc<Mutex<VecDeque<i16>>> = Arc::new(Mutex::new(VecDeque::new()));
    // pull one output frame: the mixer frame under the playback cursor (nearest, which is plenty for game sound)
    let make_pull = |queue: Arc<Mutex<VecDeque<i16>>>| {
        let mut pos = 0.0f32;
        let mut cur = (0.0f32, 0.0f32);
        move |frames: usize, out: &mut dyn FnMut(usize, f32, f32)| {
            let mut q = queue.lock().unwrap_or_else(|e| e.into_inner());
            for f in 0..frames {
                pos += step;
                while pos >= 1.0 {
                    pos -= 1.0;
                    cur = match (q.pop_front(), q.pop_front()) {
                        (Some(l), Some(r)) => (l as f32 / 32768.0, r as f32 / 32768.0),
                        _ => (0.0, 0.0),
                    };
                }
                out(f, cur.0, cur.1);
            }
        }
    };
    let err = |_| {};
    let stream = match format {
        cpal::SampleFormat::I16 => {
            let mut pull = make_pull(queue.clone());
            dev.build_output_stream(
                &config,
                move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    let frames = data.len() / channels;
                    pull(frames, &mut |f, l, r| {
                        for c in 0..channels {
                            data[f * channels + c] = ((if c % 2 == 0 { l } else { r }) * 32767.0) as i16;
                        }
                    });
                },
                err,
                None,
            )
            .ok()?
        }
        _ => {
            let mut pull = make_pull(queue.clone());
            dev.build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let frames = data.len() / channels;
                    pull(frames, &mut |f, l, r| {
                        for c in 0..channels {
                            data[f * channels + c] = if c % 2 == 0 { l } else { r };
                        }
                    });
                },
                err,
                None,
            )
            .ok()?
        }
    };
    stream.play().ok()?;
    Some(Box::new(move |buf: &[i16]| {
        let _keep_playing = &stream;
        let mut q = queue.lock().unwrap_or_else(|e| e.into_inner());
        // never let it fall far behind: drop the oldest if more than a tenth of a second is waiting
        let max = (RATE as usize / 10) * 2;
        if q.len() > max {
            let extra = q.len() - max;
            q.drain(..extra);
        }
        q.extend(buf.iter().copied());
        true
    }))
}

#[cfg(unix)]
fn spawn_player() -> Option<std::process::Child> {
    let attempts: [(&str, &[&str]); 2] = [
        ("pw-cat", &["--playback", "--raw", "--format", "s16", "--rate", "44100", "--channels", "2", "--latency", "30ms", "-"]),
        ("aplay", &["-q", "-t", "raw", "-f", "S16_LE", "-r", "44100", "-c", "2", "-"]),
    ];
    for (cmd, args) in attempts {
        if let Ok(c) = Command::new(cmd).args(args).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
            return Some(c);
        }
    }
    None
}

fn mixer(rx: Receiver<Msg>, mut sink: Sink) {
    let bank: Vec<(Sfx, Vec<f32>)> = ALL.iter().map(|&s| (s, synth(s))).collect();
    let mut voices: Vec<(usize, usize)> = Vec::new(); // (bank index, position)
    let mut muted = false;
    let mut amb = Ambience::new();
    let mut buf = vec![0i16; CHUNK * 2];
    let mut next = Instant::now();
    loop {
        loop {
            match rx.try_recv() {
                Ok(Msg::Play(Sfx::Hurt)) => {
                    if !muted {
                        amb.hurt();
                    }
                }
                Ok(Msg::Play(s)) => {
                    if !muted {
                        if let Some(i) = bank.iter().position(|(b, _)| *b == s) {
                            if voices.len() < 16 {
                                voices.push((i, 0));
                            }
                        }
                    }
                }
                Ok(Msg::Scene(sc)) => amb.set(sc),
                Ok(Msg::Mute(m)) => {
                    muted = m;
                    if m {
                        voices.clear();
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
            }
        }
        // effects (mono, both ears) over the ambience, then a soft limiter
        for f in 0..CHUNK {
            let mut sfx = 0.0;
            for (bi, pos) in voices.iter_mut() {
                let samples = &bank[*bi].1;
                if *pos < samples.len() {
                    sfx += samples[*pos];
                    *pos += 1;
                }
            }
            let (l, r) = amb.frame();
            let k = if muted { 0.0 } else { 1.0 };
            let (l, r) = ((l * 0.75 + sfx) * k, (r * 0.75 + sfx) * k);
            buf[f * 2] = ((l * 0.8).tanh() * 15000.0) as i16;
            buf[f * 2 + 1] = ((r * 0.8).tanh() * 15000.0) as i16;
        }
        voices.retain(|(bi, pos)| *pos < bank[*bi].1.len());
        if !sink(&buf) {
            return; // the output went away: stay silent
        }
        next += Duration::from_millis(10);
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else if now - next > Duration::from_millis(200) {
            next = now;
        }
    }
}

impl Audio {
    pub fn new() -> Option<Audio> {
        if std::env::var_os("ALAZ_MUTE").is_some() {
            return None;
        }
        let (tx, rx) = channel();
        // the output is opened on the audio thread itself (a Windows stream stays on the thread that made it)
        let (ok_tx, ok_rx) = channel();
        std::thread::Builder::new()
            .name("audio".into())
            .spawn(move || match open_sink() {
                Some(sink) => {
                    let _ = ok_tx.send(true);
                    mixer(rx, sink)
                }
                None => {
                    let _ = ok_tx.send(false);
                }
            })
            .ok()?;
        ok_rx.recv().ok().filter(|&ok| ok)?;
        Some(Audio { tx, muted: false })
    }

    pub fn play(&self, s: Sfx) {
        let _ = self.tx.send(Msg::Play(s));
    }

    /// What the ambience should sound like now.
    pub fn set_scene(&self, scene: Scene) {
        let _ = self.tx.send(Msg::Scene(scene));
    }

    pub fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        let _ = self.tx.send(Msg::Mute(self.muted));
    }
}

// ------------------------------------------------------------------ building blocks

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    /// 0..1
    fn f(&mut self) -> f32 {
        (self.next() >> 8) as f32 / (1u32 << 24) as f32
    }
    /// -1..1
    fn s(&mut self) -> f32 {
        self.f() * 2.0 - 1.0
    }
    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
}

/// One-pole low-pass.
#[derive(Default, Clone, Copy)]
struct Lp(f32);
impl Lp {
    fn run(&mut self, x: f32, hz: f32) -> f32 {
        let k = 1.0 - (-std::f32::consts::TAU * hz / SR).exp();
        self.0 += (x - self.0) * k;
        self.0
    }
}

/// State-variable filter (band-pass out).
#[derive(Default, Clone, Copy)]
struct Bp {
    low: f32,
    band: f32,
}
impl Bp {
    fn run(&mut self, x: f32, hz: f32, q: f32) -> f32 {
        let f = 2.0 * (std::f32::consts::PI * hz.min(SR * 0.2) / SR).sin();
        self.low += f * self.band;
        let high = x - self.low - self.band / q;
        self.band += f * high;
        self.band
    }
}

/// Equal-power pan: -1 left .. 1 right.
fn pan(x: f32, p: f32) -> (f32, f32) {
    let a = (p.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    (x * a.cos(), x * a.sin())
}

#[derive(Clone, Copy)]
enum Kind {
    /// A bird syllable: a fast sweep from f0 to f1 with a trill.
    Chirp { f0: f32, f1: f32 },
    /// An owl's hoot.
    Hoot { f: f32 },
    /// A crackle or pop in a fire.
    Crackle,
    /// A bubble rising in water.
    Bubble { f: f32 },
    /// A raindrop tick.
    Drop,
    /// A heavy blow to the body.
    Thud,
    /// A short pained voice.
    Grunt { f: f32 },
}

#[derive(Clone, Copy)]
struct Voice {
    kind: Kind,
    t: f32,
    dur: f32,
    amp: f32,
    pan: f32,
    phase: f32,
    lp: Lp,
    bp: Bp,
}

impl Voice {
    fn new(kind: Kind, dur: f32, amp: f32, pan: f32) -> Self {
        Voice { kind, t: 0.0, dur, amp, pan, phase: 0.0, lp: Lp::default(), bp: Bp::default() }
    }

    fn sample(&mut self, rng: &mut Rng) -> f32 {
        let u = (self.t / self.dur).min(1.0);
        let dt = 1.0 / SR;
        let x = match self.kind {
            Kind::Chirp { f0, f1 } => {
                let f = f0 + (f1 - f0) * u + (self.t * 90.0).sin() * 180.0;
                self.phase += f * dt;
                let env = (u * std::f32::consts::PI).sin().powf(1.5);
                (self.phase * std::f32::consts::TAU).sin() * env
            }
            Kind::Hoot { f } => {
                let f = f * (1.0 - 0.08 * u);
                self.phase += f * dt;
                let env = (u * 6.0).min(1.0) * (1.0 - u).powf(1.4);
                let s = (self.phase * std::f32::consts::TAU).sin();
                (s + 0.2 * (self.phase * 2.0 * std::f32::consts::TAU).sin()) * env
            }
            Kind::Crackle => {
                let env = (-self.t * 900.0).exp();
                self.bp.run(rng.s(), 2600.0, 1.2) * env * 3.0
            }
            Kind::Bubble { f } => {
                let f = f * (1.0 + 2.2 * u);
                self.phase += f * dt;
                let env = (-self.t * 40.0).exp();
                (self.phase * std::f32::consts::TAU).sin() * env
            }
            Kind::Drop => {
                let env = (-self.t * 1400.0).exp();
                self.bp.run(rng.s(), 3800.0, 2.0) * env * 2.5
            }
            Kind::Thud => {
                let f = 120.0 * (1.0 - 0.6 * u);
                self.phase += f * dt;
                let body = (self.phase * std::f32::consts::TAU).sin() * (-self.t * 14.0).exp();
                let hit = self.lp.run(rng.s(), 900.0) * (-self.t * 120.0).exp() * 2.0;
                body + hit
            }
            Kind::Grunt { f } => {
                // a buzzing voice through two mouth resonances, falling in pitch
                let f = f * (1.0 - 0.25 * u);
                self.phase = (self.phase + f * dt).fract();
                let saw = self.phase * 2.0 - 1.0;
                let env = (u * 12.0).min(1.0) * (1.0 - u).powf(2.0);
                let a = self.bp.run(saw, 700.0, 3.0);
                let b = self.lp.run(saw, 1500.0);
                (a * 0.8 + b * 0.3) * env
            }
        };
        self.t += dt;
        x * self.amp
    }

    fn done(&self) -> bool {
        self.t >= self.dur
    }
}

// ------------------------------------------------------------------ the ambience

struct Ambience {
    rng: Rng,
    /// Smoothed scene values.
    now: Scene,
    target: Scene,
    n: u64,
    voices: Vec<Voice>,
    wind_l: [Lp; 2],
    wind_r: [Lp; 2],
    gust: f32,
    gust_goal: f32,
    leaves: [Lp; 2],
    rain_lp: [Lp; 2],
    rain_hp: [Lp; 2],
    water: [Lp; 2],
    roar: Lp,
    bird_wait: f32,
    phrase: u32,
    phrase_wait: f32,
    bird_voice: (f32, f32, f32),
    owl_wait: f32,
    hoots: Vec<f32>,
}

impl Ambience {
    fn new() -> Self {
        Ambience {
            rng: Rng(0x2545_F491),
            now: Scene::default(),
            target: Scene::default(),
            n: 0,
            voices: Vec::new(),
            wind_l: Default::default(),
            wind_r: Default::default(),
            gust: 0.5,
            gust_goal: 0.5,
            leaves: Default::default(),
            rain_lp: Default::default(),
            rain_hp: Default::default(),
            water: Default::default(),
            roar: Lp::default(),
            bird_wait: 2.0,
            phrase: 0,
            phrase_wait: 0.0,
            bird_voice: (3000.0, 0.0, 0.0),
            owl_wait: 15.0,
            hoots: Vec::new(),
        }
    }

    fn set(&mut self, scene: Scene) {
        self.target = scene;
    }

    /// A heavy blow to the body and a pained grunt.
    fn hurt(&mut self) {
        let p = self.rng.range(-0.15, 0.15);
        self.voices.push(Voice::new(Kind::Thud, 0.35, 0.55, p));
        let f = self.rng.range(150.0, 190.0);
        self.voices.push(Voice::new(Kind::Grunt { f }, 0.26, 0.35, p));
    }

    /// Runs every 256 samples: smoothing and the random events (birds, owls, crackles, bubbles, drops).
    fn tick(&mut self) {
        let dt = 256.0 / SR;
        let k = 1.0 - (-dt / 0.6).exp();
        let (a, b) = (&mut self.now, self.target);
        let sm = |x: &mut f32, y: f32| *x += (y - *x) * k;
        sm(&mut a.wind, b.wind);
        sm(&mut a.rain, b.rain);
        sm(&mut a.day, b.day);
        sm(&mut a.trees, b.trees);
        sm(&mut a.water, b.water);
        sm(&mut a.fire, b.fire);
        a.cave = b.cave;
        let s = self.now;
        let open = if s.cave { 0.0 } else { 1.0 };
        let r = &mut self.rng;

        // gusts wander toward a new strength every few seconds
        if r.f() < dt / 3.0 {
            self.gust_goal = r.range(0.2, 1.0);
        }
        self.gust += (self.gust_goal - self.gust) * (dt / 1.5).min(1.0);

        if self.voices.len() > 40 {
            return;
        }
        // birds sing by day, less in rain, more among trees; a phrase is a few syllables from one bird
        let birds = s.day.powf(2.0) * (1.0 - s.rain) * (0.35 + 0.65 * s.trees) * open;
        if self.phrase > 0 {
            self.phrase_wait -= dt;
            if self.phrase_wait <= 0.0 {
                let (base, pan, amp) = self.bird_voice;
                let up = r.f() < 0.5;
                let (f0, f1) = if up { (base * 0.8, base * 1.25) } else { (base * 1.3, base * 0.85) };
                let dur = r.range(0.04, 0.12);
                self.voices.push(Voice::new(Kind::Chirp { f0, f1 }, dur, amp, pan));
                self.phrase -= 1;
                self.phrase_wait = dur + r.range(0.03, 0.14);
            }
        } else {
            self.bird_wait -= dt * birds;
            if self.bird_wait <= 0.0 {
                self.bird_voice = (r.range(2200.0, 4800.0), r.s() * 0.9, r.range(0.025, 0.06));
                self.phrase = 2 + r.next() % 6;
                self.phrase_wait = 0.0;
                self.bird_wait = r.range(0.8, 4.0);
            }
        }
        // now and then an owl, far off in the night woods
        let owls = (1.0 - s.day).powf(2.0) * (1.0 - s.rain) * (0.3 + 0.7 * s.trees) * open;
        self.owl_wait -= dt * owls;
        if self.owl_wait <= 0.0 {
            let f = r.range(330.0, 420.0);
            let pan = r.s() * 0.8;
            self.hoots = vec![0.0, 0.55, 0.85];
            for t in &self.hoots {
                let mut v = Voice::new(Kind::Hoot { f }, if *t == 0.0 { 0.5 } else { 0.32 }, 0.07, pan);
                v.t = -t;
                self.voices.push(v);
            }
            self.owl_wait = r.range(18.0, 45.0);
        }
        // a fire crackles and pops
        let crackles = 16.0 * s.fire;
        if r.f() < crackles * dt {
            let loud = if r.f() < 0.12 { 0.5 } else { r.range(0.08, 0.22) };
            self.voices.push(Voice::new(Kind::Crackle, 0.02, loud * s.fire, r.s() * 0.3));
        }
        // bubbles and plops by the water
        if r.f() < 1.6 * s.water * dt {
            self.voices.push(Voice::new(Kind::Bubble { f: r.range(260.0, 520.0) }, 0.09, 0.05 * s.water, r.s() * 0.7));
        }
        // single raindrops hitting leaves close by
        let drops = 40.0 * s.rain * open;
        let n = (drops * dt + r.f()) as u32;
        for _ in 0..n.min(3) {
            self.voices.push(Voice::new(Kind::Drop, 0.01, r.range(0.03, 0.09) * s.rain, r.s()));
        }
    }

    fn frame(&mut self) -> (f32, f32) {
        if self.n % 256 == 0 {
            self.tick();
        }
        self.n += 1;
        let s = self.now;
        let t = self.n as f32 / SR;
        let open = if s.cave { 0.15 } else { 1.0 };
        let (mut l, mut r) = (0.0f32, 0.0f32);
        let rng = &mut self.rng;

        // wind: two low-passed noises (one per ear) whose brightness and level follow the gusts
        let w = (s.wind / 2.0).clamp(0.0, 1.0);
        let g = self.gust;
        let cut = 180.0 + 520.0 * g * (0.4 + w);
        let wl0 = self.wind_l[0].run(rng.s(), cut);
        let wl = self.wind_l[1].run(wl0, cut);
        let wr0 = self.wind_r[0].run(rng.s(), cut);
        let wr = self.wind_r[1].run(wr0, cut);
        let wa = (0.12 + 0.55 * w) * (0.5 + 0.7 * g) * open * 1.4;
        l += wl * wa;
        r += wr * wa;

        // leaves rustle in the gusts where there are trees
        let n = rng.s();
        let hp = n - self.leaves[0].run(n, 2500.0);
        let rustle = self.leaves[1].run(hp, 7000.0) * s.trees * (0.15 + 0.9 * w) * g * g * open * 0.35;
        let (a, b) = pan(rustle, (t * 0.13).sin() * 0.6);
        l += a;
        r += b;

        // rain: a wide, soft hiss
        if s.rain > 0.01 {
            for (i, out) in [&mut l, &mut r].into_iter().enumerate() {
                let x = rng.s();
                let hi = x - self.rain_hp[i].run(x, 900.0);
                *out += self.rain_lp[i].run(hi, 5000.0) * s.rain * 0.16 * open;
            }
        }

        // crickets: a few insects chirping in short pulse groups at their own tempo, only at night
        let night = (1.0 - s.day).powf(1.5) * (1.0 - s.rain) * open;
        if night > 0.01 {
            for (i, (f, rate, p)) in [(4300.0f32, 0.72f32, -0.6f32), (4750.0, 0.55, 0.5), (4050.0, 0.9, 0.1)].into_iter().enumerate() {
                let cyc = (t * rate + i as f32 * 0.37).fract();
                // three pulses at the start of each cycle
                let pulse = if cyc < 0.15 { ((cyc / 0.05) * std::f32::consts::PI).sin().abs() } else { 0.0 };
                let x = (t * f * std::f32::consts::TAU).sin() * pulse * 0.022 * night;
                let (a, b) = pan(x, p);
                l += a;
                r += b;
            }
        }

        // water: slow lapping waves, a low murmur that swells and falls
        if s.water > 0.01 {
            let swell = 0.55 + 0.45 * (t * 0.45).sin() * (t * 0.17 + 1.0).sin();
            for (i, out) in [&mut l, &mut r].into_iter().enumerate() {
                *out += self.water[i].run(rng.s(), 380.0 + 200.0 * swell) * swell * s.water * 0.9;
            }
        }

        // fire: a low roar under the crackles
        if s.fire > 0.01 {
            let x = self.roar.run(rng.s(), 160.0) * s.fire * 1.2 * (0.8 + 0.2 * (t * 7.0).sin());
            l += x;
            r += x;
        }

        for v in &mut self.voices {
            if v.t < 0.0 {
                v.t += 1.0 / SR;
                continue;
            }
            let x = v.sample(rng);
            let (a, b) = pan(x, v.pan);
            l += a;
            r += b;
        }
        self.voices.retain(|v| !v.done());

        (l, r)
    }
}
