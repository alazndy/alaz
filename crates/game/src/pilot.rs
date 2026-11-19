//! Pilot: plays the game through its real input paths. Dev aid, off unless `ALAZ_PILOT=<command file>` is set.
//!
//! The command file is read as it grows (append lines while the game runs). Each line is one thing a player does:
//!
//! ```text
//! move TARGET [ms]        glide the mouse there (an eased, slightly curved path, like a hand)
//! click [left|right|middle]   press and release in place
//! rclick / lclick TARGET  move there, then click
//! drag FROM TO [ms]       press on FROM, glide to TO, release (item dragging)
//! key NAME [hold_ms]      tap or hold a key (w, i, c, space, shift, esc, f5 ...)
//! keydown NAME / keyup NAME
//! wheel N                 scroll (zoom)
//! wait MS                 pause
//! waitidle [max_s]        wait until the player has nothing left to do
//! state / look / marks    write the player's state / what is in sight / the UI's named areas to the log
//! say TEXT                put a line in the log
//! ```
//! TARGET is x,y (pixels) | node:KIND (nearest in sight) | ui:LABEL (a named UI area, see marks) | me | ground:DX,DZ (metres).
//! All times are game time: at `ALAZ_TIME_SCALE=10` the whole run is ten times quicker but every pause, move and
//! click lasts what it would at normal speed. A press and its release are always at least one frame apart.

use alaz_render::egui;
use alaz_render::Camera;
use alaz_sim::world::{Task, World};
use alaz_sim::{AgentId, NodeKind};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::OnceLock;
use winit::keyboard::KeyCode;

/// True when a pilot file is set: the UI then records its named areas.
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("ALAZ_PILOT").is_some())
}

type Marks = (u64, Vec<(String, egui::Rect)>);

fn marks_id() -> egui::Id {
    egui::Id::NULL.with("pilot_marks")
}

thread_local! {
    static SCOPE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Runs `f` with a prefix on every mark it makes (`hands/slot:stone`), so the same item in two places has two names.
pub fn scoped<R>(name: &str, f: impl FnOnce() -> R) -> R {
    SCOPE.with(|s| *s.borrow_mut() = format!("{name}/"));
    let r = f();
    SCOPE.with(|s| s.borrow_mut().clear());
    r
}

/// Names an area of the UI for the pilot (cheap no-op when no pilot runs). Marks live for one pass.
pub fn mark(ctx: &egui::Context, label: impl FnOnce() -> String, rect: egui::Rect) {
    if !enabled() {
        return;
    }
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| {
        let m = d.get_temp_mut_or_insert_with::<Marks>(marks_id(), || (pass, Vec::new()));
        if m.0 != pass {
            *m = (pass, Vec::new());
        }
        let prefix = SCOPE.with(|s| s.borrow().clone());
        m.1.push((format!("{prefix}{}", label()), rect));
    });
}

fn signal_id() -> egui::Id {
    egui::Id::NULL.with("pilot_signal")
}

/// A number the UI keeps to itself that a player can see on screen (a meter's level, and whether it is rising).
pub fn signal(ctx: &egui::Context, level: f32, rising: bool) {
    if enabled() {
        ctx.data_mut(|d| d.insert_temp(signal_id(), (level, rising)));
    }
}

pub fn read_signal(ctx: &egui::Context) -> (f32, bool) {
    ctx.data(|d| d.get_temp::<(f32, bool)>(signal_id())).unwrap_or((0.0, true))
}

/// The areas named in the last finished pass.
pub fn marks(ctx: &egui::Context) -> Vec<(String, egui::Rect)> {
    ctx.data(|d| d.get_temp::<Marks>(marks_id())).map(|m| m.1).unwrap_or_default()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Btn {
    Left,
    Right,
    Middle,
}

/// One input event for the game to treat as if the window had received it.
#[derive(Clone, Copy, Debug)]
pub enum Input {
    Cursor(f32, f32),
    Button(Btn, bool),
    Key(KeyCode, bool),
    Wheel(f32),
}

/// What the pilot wants written down; main answers (it knows the game).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Want {
    State,
    Look,
    Marks,
    Shot(String),
}

/// What the pilot may look at.
pub struct View<'a> {
    pub world: &'a World,
    pub me: AgentId,
    pub cam: Camera,
    pub win: (u32, u32),
    pub ppp: f32,
    pub marks: Vec<(String, egui::Rect)>,
    pub sim_s: f64,
    /// The level of the force meter on screen, and whether it is rising.
    pub signal: (f32, bool),
    /// The craft being played, if any.
    pub craft: Option<alaz_sim::world::CraftView>,
    /// The node the game's own picking finds under a screen point (pixels).
    pub pick: &'a dyn Fn(f32, f32) -> Option<u32>,
}

#[derive(Clone, Debug)]
pub enum Target {
    Px(f32, f32),
    Node(NodeKind),
    NodeId(u32),
    Ui(String),
    Me,
    Ground(f32, f32),
    /// The rim of the knapping stone, where the green arc is (or at this angle).
    KnapRim,
    /// Where a fit, trim or stack click belongs.
    CraftSpot,
    /// The two ends of the rubbing stroke, and the middle of the work area.
    RubStart,
    RubEnd,
    CraftMid,
}

enum Step {
    Move { target: Target, dur: Option<f64>, run: Option<MoveRun> },
    Wait { ms: f64, until: Option<(f64, u64)> },
    Speed(u32),
    Press(Btn),
    Release(Btn),
    KeyDown(KeyCode),
    KeyUp(KeyCode),
    Wheel(f32),
    Want(Want),
    Say(String),
    Idle { max_ms: f64, until: Option<f64>, calm_since: Option<f64> },
    /// Holding the button down while the force meter fills (it swings up at 1.1 per second of real time).
    Charge { c: f32 },
    /// Hold until the on-screen meter reaches the craft's target force, then let go.
    Hold,
    /// Plays the craft in front of the player, one try at a time, until it is over.
    CraftPlay { left: u32 },
    /// Slow to normal speed for a minigame, and back.
    SpeedLow,
    SpeedBack,
}

struct MoveRun {
    from: (f32, f32),
    f0: u64,
    t0: f64,
    dur: f64,
    bend: f32,
    target: Target,
}

pub struct Pilot {
    path: PathBuf,
    log_path: PathBuf,
    read: u64,
    steps: VecDeque<Step>,
    t_ms: f64,
    pub mouse: (f32, f32),
    rng: u64,
    pub wants: Vec<Want>,
    log: Vec<String>,
    started: bool,
    frame: u64,
    /// Speed asked for by a `speed` line (0 = paused); main takes it.
    pub speed_req: Option<u32>,
    speed: u32,
    saved_speed: u32,
    last_pos: u32,
}

/// However fast the game runs, one frame is the least a pointer glides over, and two the least a pause lasts:
/// the UI sees every state a hand would have shown it.
const MOVE_FRAMES: f64 = 10.0;
const WAIT_FRAMES: u64 = 2;

fn key_named(n: &str) -> Option<KeyCode> {
    use KeyCode::*;
    let n = n.to_ascii_lowercase();
    if n.len() == 1 {
        let c = n.as_bytes()[0];
        const L: [KeyCode; 26] = [KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ];
        const D: [KeyCode; 10] = [Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9];
        return match c {
            b'a'..=b'z' => Some(L[(c - b'a') as usize]),
            b'0'..=b'9' => Some(D[(c - b'0') as usize]),
            _ => None,
        };
    }
    Some(match n.as_str() {
        "space" => Space,
        "shift" => ShiftLeft,
        "ctrl" => ControlLeft,
        "esc" | "escape" => Escape,
        "tab" => Tab,
        "enter" => Enter,
        "up" => ArrowUp,
        "down" => ArrowDown,
        "left" => ArrowLeft,
        "right" => ArrowRight,
        "f1" => F1,
        "f5" => F5,
        "f9" => F9,
        _ => return None,
    })
}

/// The egui key for a key code, for the few keys egui widgets care about.
pub fn egui_key(code: KeyCode) -> Option<egui::Key> {
    use egui::Key as K;
    use KeyCode::*;
    Some(match code {
        Space => K::Space,
        Escape => K::Escape,
        Enter => K::Enter,
        Tab => K::Tab,
        ArrowUp => K::ArrowUp,
        ArrowDown => K::ArrowDown,
        ArrowLeft => K::ArrowLeft,
        ArrowRight => K::ArrowRight,
        KeyA => K::A,
        KeyC => K::C,
        KeyI => K::I,
        KeyB => K::B,
        KeyH => K::H,
        KeyJ => K::J,
        KeyX => K::X,
        _ => return None,
    })
}

fn parse_target(s: &str) -> Option<Target> {
    if s == "me" {
        return Some(Target::Me);
    }
    if let Some(k) = s.strip_prefix("node:") {
        return alaz_sim::debug::node_named(k).map(Target::Node);
    }
    if let Some(l) = s.strip_prefix("ui:") {
        return Some(Target::Ui(l.to_string()));
    }
    if let Some(g) = s.strip_prefix("ground:") {
        let (a, b) = g.split_once(',')?;
        return Some(Target::Ground(a.parse().ok()?, b.parse().ok()?));
    }
    let (a, b) = s.split_once(',')?;
    Some(Target::Px(a.parse().ok()?, b.parse().ok()?))
}

impl Pilot {
    pub fn from_env() -> Option<Pilot> {
        let path = PathBuf::from(std::env::var_os("ALAZ_PILOT")?);
        let log_path = std::env::var_os("ALAZ_PILOT_LOG").map(PathBuf::from).unwrap_or_else(|| path.with_extension("log"));
        let _ = std::fs::write(&log_path, "");
        Some(Pilot { path, log_path, read: 0, steps: VecDeque::new(), t_ms: 0.0, mouse: (400.0, 300.0), rng: 0x9E37_79B9_7F4A_7C15, wants: Vec::new(), log: Vec::new(), started: false, frame: 0, speed_req: None, speed: 10, saved_speed: 10, last_pos: 0 })
    }

    fn rand(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }

    /// A person's pause between two actions.
    fn react(&mut self) -> Step {
        Step::Wait { ms: 90.0 + self.rand() * 130.0, until: None }
    }

    pub fn say(&mut self, sim_s: f64, line: impl AsRef<str>) {
        use std::io::Write;
        let text = format!("[{sim_s:8.1}s] {}\n", line.as_ref());
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).create(true).open(&self.log_path) {
            let _ = f.write_all(text.as_bytes());
        }
        eprint!("PILOT {text}");
    }

    /// Reads lines that were added to the command file since last time.
    fn poll_file(&mut self, sim_s: f64) {
        let Ok(meta) = std::fs::metadata(&self.path) else { return };
        if meta.len() <= self.read {
            return;
        }
        let Ok(bytes) = std::fs::read(&self.path) else { return };
        let from = self.read as usize;
        // only whole lines: a half-written one waits for its end
        let Some(end) = bytes[from..].iter().rposition(|&b| b == b'\n') else { return };
        let text = String::from_utf8_lossy(&bytes[from..from + end + 1]).into_owned();
        self.read += (end + 1) as u64;
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if !line.is_empty() {
                if let Err(e) = self.queue(line) {
                    self.say(sim_s, format!("? {line}: {e}"));
                }
            }
        }
    }

    fn queue(&mut self, line: &str) -> Result<(), String> {
        let w: Vec<&str> = line.split_whitespace().collect();
        let tgt = |i: usize| w.get(i).and_then(|s| parse_target(s)).ok_or_else(|| format!("target {i} missing or unknown"));
        let ms = |i: usize| w.get(i).and_then(|s| s.parse::<f64>().ok());
        let btn = |i: usize| match w.get(i).copied() {
            Some("right") => Btn::Right,
            Some("middle") => Btn::Middle,
            _ => Btn::Left,
        };
        let mut out: Vec<Step> = Vec::new();
        match w[0] {
            "move" => out.push(Step::Move { target: tgt(1)?, dur: ms(2), run: None }),
            "click" | "rclick" | "lclick" => {
                let b = if w[0] == "rclick" { Btn::Right } else if w[0] == "lclick" { Btn::Left } else { btn(1) };
                if w[0] != "click" {
                    out.push(Step::Move { target: tgt(1)?, dur: ms(2), run: None });
                    out.push(Step::Wait { ms: 60.0 + self.rand() * 90.0, until: None });
                }
                out.extend(self.click_steps(b));
            }
            "drag" => {
                out.push(Step::Move { target: tgt(1)?, dur: None, run: None });
                out.push(Step::Wait { ms: 70.0 + self.rand() * 60.0, until: None });
                out.push(Step::Press(Btn::Left));
                out.push(Step::Wait { ms: 110.0 + self.rand() * 80.0, until: None });
                out.push(Step::Move { target: tgt(2)?, dur: ms(3).or(Some(450.0 + self.rand() * 250.0)), run: None });
                out.push(Step::Wait { ms: 90.0 + self.rand() * 60.0, until: None });
                out.push(Step::Release(Btn::Left));
                out.push(self.react());
            }
            "key" => {
                let k = key_named(w.get(1).ok_or("key name missing")?).ok_or("unknown key")?;
                out.push(Step::KeyDown(k));
                out.push(Step::Wait { ms: ms(2).unwrap_or(70.0 + self.rand() * 50.0), until: None });
                out.push(Step::KeyUp(k));
                out.push(self.react());
            }
            "strike" => {
                // one knapping strike: grip the stone, aim at the arc, hold until the force is right, let go
                let t = key_named("t").ok_or("t")?;
                out.push(Step::KeyDown(t));
                out.push(Step::Wait { ms: 90.0, until: None });
                out.push(Step::Move { target: Target::KnapRim, dur: None, run: None });
                out.push(Step::Wait { ms: 160.0 + self.rand() * 100.0, until: None });
                out.push(Step::Press(Btn::Left));
                out.push(Step::Charge { c: 0.0 });
                out.push(Step::Release(Btn::Left));
                out.push(Step::Wait { ms: 250.0, until: None });
                out.push(Step::KeyUp(t));
                out.push(self.react());
            }
            "craft" => {
                // plays the open craft to its end the way a hand would, at normal speed whatever the game speed is
                out.push(Step::SpeedLow);
                out.push(Step::CraftPlay { left: ms(1).unwrap_or(60.0) as u32 });
                out.push(Step::SpeedBack);
            }
            "keydown" => out.push(Step::KeyDown(key_named(w.get(1).ok_or("key name missing")?).ok_or("unknown key")?)),
            "keyup" => out.push(Step::KeyUp(key_named(w.get(1).ok_or("key name missing")?).ok_or("unknown key")?)),
            "wheel" => out.push(Step::Wheel(ms(1).ok_or("amount missing")? as f32)),
            "wait" => out.push(Step::Wait { ms: ms(1).ok_or("ms missing")?, until: None }),
            "speed" => out.push(Step::Speed(ms(1).ok_or("speed missing")? as u32)),
            "pause" => out.push(Step::Speed(0)),
            "waitidle" => out.push(Step::Idle { max_ms: ms(1).unwrap_or(60.0) * 1000.0, until: None, calm_since: None }),
            "state" => out.push(Step::Want(Want::State)),
            "look" => out.push(Step::Want(Want::Look)),
            "marks" => out.push(Step::Want(Want::Marks)),
            "shot" => out.push(Step::Want(Want::Shot(w.get(1).copied().unwrap_or("shot").to_string()))),
            "say" => out.push(Step::Say(line[3..].trim().to_string())),
            other => return Err(format!("unknown command {other}")),
        }
        self.steps.extend(out);
        Ok(())
    }

    fn click_steps(&mut self, b: Btn) -> Vec<Step> {
        vec![Step::Press(b), Step::Wait { ms: 55.0 + self.rand() * 60.0, until: None }, Step::Release(b), self.react()]
    }

    /// Screen position (pixels) of a target, or None when it is not in sight.
    pub fn resolve(&self, t: &Target, v: &View) -> Option<(f32, f32)> {
        let (w, h) = v.win;
        let proj = |xm: f32, ym: f32, zm: f32| {
            let mut cam = v.cam;
            cam.win = v.win;
            let n = cam.project(1.0, alaz_render::glam::Vec3::new(xm, ym, zm));
            ((n[0] + 1.0) * 0.5 * w as f32, (1.0 - n[1]) * 0.5 * h as f32)
        };
        let ground = |x_mm: i64, y_mm: i64, up: f32| {
            let (tx, ty) = ((x_mm / alaz_sim::map::TILE_MM) as i32, (y_mm / alaz_sim::map::TILE_MM) as i32);
            let e = alaz_sim::elevation_mm(v.world.seed, tx, ty) as f32 / 1000.0;
            proj(x_mm as f32 / 1000.0, e + up, y_mm as f32 / 1000.0)
        };
        let me = v.world.agents.iter().find(|a| a.id == v.me)?;
        match t {
            Target::Px(x, y) => Some((*x, *y)),
            Target::Me => Some(ground(me.x, me.y, 0.9)),
            Target::Ground(dx, dz) => Some(ground(me.x + (*dx * 1000.0) as i64, me.y + (*dz * 1000.0) as i64, 0.0)),
            Target::NodeId(i) => {
                let n = v.world.nodes.get(*i as usize)?;
                let (x, y) = n.center_mm();
                Some(ground(x, y, 0.5))
            }
            Target::Node(k) => self.nearest_node(*k, v).and_then(|i| self.resolve(&Target::NodeId(i), v)),
            Target::KnapRim => {
                let cv = v.craft.as_ref()?;
                let hit = v.marks.iter().find(|(l, _)| l == "knap")?;
                let (c, r) = (hit.1.center(), hit.1.width() * 0.369 * 0.85);
                let deg = (cv.center as f32).to_radians();
                Some(((c.x + r * deg.sin()) * v.ppp, (c.y - r * deg.cos()) * v.ppp))
            }
            Target::CraftSpot | Target::CraftMid | Target::RubStart | Target::RubEnd => {
                use alaz_sim::MinigameKind as K;
                let cv = v.craft.as_ref()?;
                let area = v.marks.iter().find(|(l, _)| l == "craft")?.1;
                let c = area.center();
                let pt = match (t, cv.kind) {
                    (Target::CraftMid, _) => c,
                    (Target::RubStart, _) => egui::pos2(area.left() + 30.0, c.y + 6.0),
                    (Target::RubEnd, _) => egui::pos2(area.right() - 20.0, c.y + 6.0),
                    (_, K::Fit) => {
                        let k = area.width() / 290.0 * 1.35;
                        egui::pos2(c.x + cv.tx as f32 * k, c.y + cv.ty as f32 * k)
                    }
                    (_, K::Trim) => egui::pos2(c.x + cv.tx as f32 * 1.5, c.y + cv.ty as f32 * 1.5),
                    // stacking: straight over the middle of the pile always balances
                    (_, _) => egui::pos2(c.x, c.y),
                };
                Some((pt.x * v.ppp, pt.y * v.ppp))
            }
            Target::Ui(label) => {
                // underscores stand for spaces ("ui:Su_iç"), and the other way round
                let norm = |s: &str| s.replace(' ', "_");
                let want = norm(label);
                let hit = v.marks.iter().find(|(l, _)| norm(l) == want).or_else(|| v.marks.iter().find(|(l, _)| norm(l).starts_with(&want)))?;
                let c = hit.1.center();
                Some((c.x * v.ppp, c.y * v.ppp))
            }
        }
    }

    /// The nearest node of a kind that is on screen and still has something in it.
    fn nearest_node(&self, k: NodeKind, v: &View) -> Option<u32> {
        let me = v.world.agents.iter().find(|a| a.id == v.me)?;
        let (r, w, h) = (60, v.win.0 as f32, v.win.1 as f32);
        let (tx, ty) = ((me.x / alaz_sim::map::TILE_MM) as i32, (me.y / alaz_sim::map::TILE_MM) as i32);
        let mut cands: Vec<(i64, u32, (f32, f32))> = Vec::new();
        for n in v.world.nodes_in(tx - r, ty - r, tx + r, ty + r) {
            if n.kind != k || n.amount == 0 {
                continue;
            }
            let Some(id) = v.world.node_at(n.tx, n.ty) else { continue };
            let Some((px, py)) = self.resolve(&Target::NodeId(id.0), v) else { continue };
            if px < 8.0 || py < 8.0 || px > w - 8.0 || py > h - 8.0 {
                continue;
            }
            let (cx, cy) = n.center_mm();
            cands.push((alaz_sim::world::dist2(me.x, me.y, cx, cy), id.0, (px, py)));
        }
        cands.sort_by_key(|c| c.0);
        // the nearest one a click would really reach: a big tree may stand in front of a plant
        cands.into_iter().find(|c| (v.pick)(c.2 .0, c.2 .1) == Some(c.1)).map(|c| c.1)
    }

    /// Nearest nodes, for the log: kind, tile, metres away, screen pixels.
    pub fn look(&self, v: &View) -> Vec<String> {
        let Some(me) = v.world.agents.iter().find(|a| a.id == v.me) else { return vec![] };
        let (tx, ty) = ((me.x / alaz_sim::map::TILE_MM) as i32, (me.y / alaz_sim::map::TILE_MM) as i32);
        let mut all: Vec<(i64, String)> = Vec::new();
        for n in v.world.nodes_in(tx - 40, ty - 40, tx + 40, ty + 40) {
            if n.amount == 0 {
                continue;
            }
            let Some(id) = v.world.node_at(n.tx, n.ty) else { continue };
            let (cx, cy) = n.center_mm();
            let d = alaz_sim::world::dist2(me.x, me.y, cx, cy);
            let at = self.resolve(&Target::NodeId(id.0), v).map_or("off".to_string(), |(x, y)| format!("{x:.0},{y:.0}"));
            all.push((d, format!("  {:?} x{} at {:.0} m, screen {at}", n.kind, n.amount, (d as f64).sqrt() / 1000.0)));
        }
        all.sort();
        all.into_iter().take(14).map(|a| a.1).collect()
    }

    /// Advances the pilot's clock by game time and returns the inputs for this frame.
    pub fn tick(&mut self, dt_ms: f64, real_dt: f32, v: &View) -> Vec<Input> {
        if !self.started {
            self.started = true;
            self.say(v.sim_s, format!("pilot ready; reading {}", self.path.display()));
        }
        self.t_ms += dt_ms;
        self.frame += 1;
        self.poll_file(v.sim_s);
        let mut out = Vec::new();
        let now = self.t_ms;
        while let Some(step) = self.steps.pop_front() {
            match step {
                Step::Wait { ms, until } => {
                    let until = until.unwrap_or((now + ms, self.frame + WAIT_FRAMES));
                    if now < until.0 || self.frame < until.1 {
                        self.steps.push_front(Step::Wait { ms, until: Some(until) });
                        break;
                    }
                }
                Step::Speed(n) => {
                    if n > 0 {
                        self.speed = n;
                    }
                    self.speed_req = Some(n);
                }
                Step::SpeedLow => {
                    self.saved_speed = self.speed;
                    self.speed_req = Some(1);
                }
                Step::SpeedBack => self.speed_req = Some(self.saved_speed),
                Step::Hold => {
                    let Some(cv) = v.craft.as_ref() else { continue };
                    let (level, rising) = v.signal;
                    // let go a frame early: the game sees the release one frame after it is sent
                    if rising && level + 0.025 >= cv.power as f32 / 100.0 {
                        continue;
                    }
                    self.steps.push_front(Step::Hold);
                    break;
                }
                Step::CraftPlay { left } => {
                    use alaz_sim::MinigameKind as K;
                    let Some(cv) = v.craft.as_ref() else { continue };
                    if left == 0 {
                        self.say(v.sim_s, "? craft: out of tries");
                        continue;
                    }
                    let again = Step::CraftPlay { left: left - 1 };
                    let key = |s: &str| key_named(s).unwrap();
                    let click = |me: &mut Self, target: Target, mut v2: Vec<Step>| -> Vec<Step> {
                        let mut s = vec![Step::Move { target, dur: None, run: None }, Step::Wait { ms: 140.0 + me.rand() * 100.0, until: None }];
                        s.append(&mut v2);
                        s
                    };
                    let mut plan: Vec<Step>;
                    match cv.kind {
                        K::Timing => {
                            // the marker moves; press when it is about to be inside the middle of the zone
                            let vel = cv.pos as i64 - self.last_pos as i64;
                            self.last_pos = cv.pos;
                            let ahead = cv.pos as i64 + vel * 3;
                            if (ahead - cv.center as i64).unsigned_abs() as f32 <= (cv.half as f32 * 0.3).max(1.0) {
                                plan = vec![Step::KeyDown(key("space")), Step::Wait { ms: 60.0, until: None }, Step::KeyUp(key("space")), Step::Wait { ms: 400.0, until: None }];
                            } else {
                                self.steps.push_front(again);
                                break;
                            }
                        }
                        K::Knap => {
                            plan = vec![
                                Step::KeyDown(key("t")),
                                Step::Wait { ms: 90.0, until: None },
                                Step::Move { target: Target::KnapRim, dur: None, run: None },
                                Step::Wait { ms: 160.0 + self.rand() * 100.0, until: None },
                                Step::Press(Btn::Left),
                                Step::Hold,
                                Step::Release(Btn::Left),
                                Step::Wait { ms: 300.0, until: None },
                                Step::KeyUp(key("t")),
                                Step::Wait { ms: 150.0, until: None },
                            ];
                        }
                        K::Fit | K::Trim | K::Stack => {
                            let tail = vec![Step::Press(Btn::Left), Step::Wait { ms: 70.0, until: None }, Step::Release(Btn::Left), Step::Wait { ms: 500.0, until: None }];
                            plan = click(self, Target::CraftSpot, tail);
                        }
                        K::Tension => {
                            let tail = vec![Step::Press(Btn::Left), Step::Hold, Step::Release(Btn::Left), Step::Wait { ms: 500.0, until: None }];
                            plan = click(self, Target::CraftMid, tail);
                        }
                        K::Rub => {
                            // an even stroke: the pace the game wants is progress per second, 2.5 times the target force
                            let secs = (40.0 / cv.power.max(5) as f64).clamp(0.25, 3.0);
                            plan = vec![
                                Step::KeyDown(key("t")),
                                Step::Move { target: Target::RubStart, dur: None, run: None },
                                Step::Wait { ms: 200.0, until: None },
                                Step::Press(Btn::Left),
                                Step::Move { target: Target::RubEnd, dur: Some(secs * 1000.0), run: None },
                                Step::Release(Btn::Left),
                                Step::Wait { ms: 300.0, until: None },
                                Step::KeyUp(key("t")),
                                Step::Wait { ms: 300.0, until: None },
                            ];
                        }
                    }
                    plan.push(again);
                    for s in plan.into_iter().rev() {
                        self.steps.push_front(s);
                    }
                }
                Step::Move { target, dur, run } => {
                    let run = match run {
                        Some(r) => r,
                        None => {
                            let Some(to) = self.resolve(&target, v) else {
                                self.say(v.sim_s, format!("? move target {target:?} not in sight: skipping to the end of the batch"));
                                // drop the plain steps; reports and speed changes still run, so a batch always ends in its pause
                                self.steps.retain(|s| matches!(s, Step::Want(_) | Step::Speed(_) | Step::Say(_)));
                                break;
                            };
                            let dist = ((to.0 - self.mouse.0).powi(2) + (to.1 - self.mouse.1).powi(2)).sqrt() as f64;
                            let dur = dur.unwrap_or((170.0 + dist * 0.45).min(950.0)) * (0.9 + 0.2 * self.rand());
                            let bend = ((self.rand() - 0.5) * 14.0) as f32;
                            MoveRun { from: self.mouse, f0: self.frame, t0: now, dur, bend, target: target.clone() }
                        }
                    };
                    let to = self.resolve(&run.target, v).unwrap_or(run.from);
                    let p = (((now - run.t0) / run.dur).min((self.frame - run.f0) as f64 / MOVE_FRAMES)).clamp(0.0, 1.0) as f32;
                    // ease in and out, with a small sideways bow in the middle of the path
                    let e = p * p * (3.0 - 2.0 * p);
                    let (dx, dy) = (to.0 - run.from.0, to.1 - run.from.1);
                    let len = (dx * dx + dy * dy).sqrt().max(1.0);
                    let bow = run.bend * (std::f32::consts::PI * p).sin() * (len / 400.0).min(1.0);
                    let at = (run.from.0 + dx * e - dy / len * bow, run.from.1 + dy * e + dx / len * bow);
                    self.mouse = at;
                    out.push(Input::Cursor(at.0, at.1));
                    if p < 1.0 {
                        self.steps.push_front(Step::Move { target, dur, run: Some(run) });
                    }
                    break;
                }
                Step::Press(b) => {
                    out.push(Input::Button(b, true));
                    break;
                }
                Step::Release(b) => {
                    out.push(Input::Button(b, false));
                    break;
                }
                Step::KeyDown(k) => {
                    out.push(Input::Key(k, true));
                    break;
                }
                Step::KeyUp(k) => {
                    out.push(Input::Key(k, false));
                    break;
                }
                Step::Wheel(n) => {
                    out.push(Input::Wheel(n));
                    break;
                }
                Step::Charge { c } => {
                    let Some(cv) = v.craft.as_ref() else {
                        // the craft ended (or never began): nothing to hold for
                        self.steps.retain(|s| matches!(s, Step::Want(_) | Step::Speed(_) | Step::Say(_) | Step::Release(_) | Step::KeyUp(_)));
                        continue;
                    };
                    let dt = real_dt.min(0.05);
                    let c = c + 1.1 * dt;
                    // the release is seen one frame after it is sent, so let go one frame early
                    if c + 1.1 * dt >= cv.power as f32 / 100.0 {
                        continue;
                    }
                    self.steps.push_front(Step::Charge { c });
                    break;
                }
                Step::Want(w) => self.wants.push(w),
                Step::Say(s) => self.log.push(s),
                Step::Idle { max_ms, until, calm_since } => {
                    let until = until.unwrap_or(now + max_ms);
                    let busy = v.world.agents.iter().find(|a| a.id == v.me).is_none_or(|a| !matches!(a.task, Task::Idle));
                    let calm = if busy { None } else { Some(calm_since.unwrap_or(now)) };
                    let done = calm.is_some_and(|c| now - c >= 400.0) || now >= until;
                    if !done {
                        self.steps.push_front(Step::Idle { max_ms, until: Some(until), calm_since: calm });
                        break;
                    }
                }
            }
        }
        for l in std::mem::take(&mut self.log) {
            self.say(v.sim_s, l);
        }
        out
    }
}
