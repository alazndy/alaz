//! Connects the report form to the game: when it opens, what goes into a report, and what happens to it.
//! A report is written to disk first and sent from a thread, so the game never waits for the network.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;

use alaz_feedback::crash::Crash;
use alaz_feedback::outbox::{self, Outcome, PutError};
use alaz_feedback::report::{Draft, Kind, Mode, Part, Problem, Report, SCREENSHOT_MAX};
use alaz_feedback::{crash, log, report, scrub, shot, upload};
use alaz_render::egui;
use alaz_render::Capture;

use crate::report_ui::{self, Form, FormAct};

const OUTBOX: &str = "reports";
const NAME_FILE: &str = "reports-name.txt";
/// How long the form waits for a picture of the game before it opens without one.
const CAPTURE_FRAMES: u32 = 5;
const SHOT_WIDTH: u32 = 1920;
const PREVIEW_WIDTH: u32 = 320;
const TOAST_SECONDS: f32 = 4.0;
/// How much of the play log is read before the report keeps its last lines.
const PLAY_TAIL: usize = 200_000;
const CRASH_LINES: usize = 20;
const FULL: &str = "Bekleyen 10 rapor var; yenisi eklenemiyor.";

/// How a report leaves the machine. Tests put their own here.
pub(crate) type Transport = Arc<dyn Fn(&Report) -> Outcome + Send + Sync>;

/// Keeps every report waiting: what tests use, and the game when it has no usable address.
pub(crate) fn offline() -> Transport {
    Arc::new(|_| Outcome::Later)
}

/// The feedback site, or the address in `ALAZ_FEEDBACK_URL`.
pub(crate) fn network() -> Transport {
    match upload::base(std::env::var("ALAZ_FEEDBACK_URL").ok().as_deref()) {
        Some(base) => Arc::new(move |r| upload::send(&base, r)),
        None => {
            log::line("ALAZ_FEEDBACK_URL kullanılamıyor (https ya da bu bilgisayara http olmalı); raporlar bekletiliyor.");
            offline()
        }
    }
}

/// What the game knows about itself at the moment of a report.
pub(crate) struct Facts {
    pub gpu: Option<String>,
    pub window: Option<String>,
    pub zoom: Option<f32>,
    pub seed: Option<u64>,
    pub world_size: Option<u32>,
    pub day: Option<u32>,
    pub mode: Mode,
    pub save: Option<Vec<u8>>,
    pub playlog: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Act {
    Send,
    Cancel,
    Crash(bool),
}

/// What is on the screen. Apart from `Feedback` so the drawing code can borrow it alone.
#[derive(Default)]
pub(crate) struct Screen {
    pub form: Option<Form>,
    pub crash: Option<Crash>,
    /// A message and the seconds it stays.
    pub toast: Option<(String, f32)>,
}

impl Screen {
    pub fn draw(&mut self, ctx: &egui::Context) -> Option<Act> {
        if let Some((text, _)) = &self.toast {
            report_ui::draw_toast(ctx, text);
        }
        if self.crash.is_some() {
            return report_ui::draw_crash_prompt(ctx).map(Act::Crash);
        }
        match report_ui::draw_form(ctx, self.form.as_mut()?)? {
            FormAct::Send => Some(Act::Send),
            FormAct::Cancel => Some(Act::Cancel),
        }
    }
}

pub(crate) struct Feedback {
    dir: PathBuf,
    transport: Transport,
    /// Frames spent waiting for the picture; `None` when no form is on its way.
    waiting: Option<u32>,
    shot: Option<Capture>,
    pub screen: Screen,
    news: (mpsc::Sender<Outcome>, mpsc::Receiver<Outcome>),
}

/// A text with everything that identifies the player or their network taken out.
fn clean(text: &str) -> String {
    let var = |a: &str, b: &str| std::env::var(a).or_else(|_| std::env::var(b)).ok();
    scrub::scrub(text, var("HOME", "USERPROFILE").as_deref(), var("USER", "USERNAME").as_deref())
}

fn log_of(game: &str, playlog: &Path) -> Vec<u8> {
    log::compose(&clean(game), &clean(&log::tail(playlog, PLAY_TAIL)))
}

impl Feedback {
    /// `dir` is the game's data folder.
    pub fn new(dir: PathBuf, transport: Transport) -> Self {
        Self { dir, transport, waiting: None, shot: None, screen: Screen::default(), news: mpsc::channel() }
    }

    /// Whether keys, clicks and a world played alone have to wait.
    pub fn blocking(&self) -> bool {
        self.waiting.is_some() || self.screen.form.is_some() || self.screen.crash.is_some()
    }

    /// A picture of the next frame is wanted.
    pub fn capturing(&self) -> bool {
        self.waiting.is_some()
    }

    /// Ask for the form; it opens once there is a picture of the game as it is now.
    pub fn request(&mut self) {
        if !self.blocking() {
            self.waiting = Some(0);
            self.shot = None;
        }
    }

    /// Called after each drawn frame while `capturing`. `in_play` says whether there is a world to send.
    pub fn after_frame(&mut self, shot: Option<Capture>, can_capture: bool, in_play: bool) {
        let Some(frames) = self.waiting else { return };
        if shot.is_none() && can_capture && frames + 1 < CAPTURE_FRAMES {
            self.waiting = Some(frames + 1);
            return;
        }
        self.waiting = None;
        let preview = shot.as_ref().map(|c| shot::shrink(&c.rgb, c.width, c.height, PREVIEW_WIDTH));
        self.shot = shot;
        let name = fs::read_to_string(self.dir.join(NAME_FILE)).unwrap_or_default();
        self.screen.form = Some(Form::new(name.trim().to_string(), preview, in_play));
    }

    /// Close whatever is open; a question about a crash counts as answered with no.
    pub fn close(&mut self) {
        self.waiting = None;
        self.shot = None;
        self.screen.form = None;
        self.screen.crash = None;
    }

    /// Once, when the game starts: ask about a crash and send what waited.
    pub fn at_start(&mut self) {
        self.screen.crash = crash::take(&self.dir);
        self.flush(None);
    }

    /// Send the form that is open. It stays open, saying why, when the report cannot be kept.
    pub fn send(&mut self, facts: Facts) {
        let Some(form) = self.screen.form.as_ref() else { return };
        let picture = self.shot.as_ref().filter(|_| form.send_shot);
        let screenshot = picture.and_then(|c| shot::png(&c.rgb, c.width, c.height, SHOT_WIDTH, SCREENSHOT_MAX));
        let no_room = picture.is_some() && screenshot.is_none();
        let game = log::tail(&self.dir.join(log::NAME), report::LOG_MAX);
        let draft = Draft {
            kind: form.kind,
            text: form.text.clone(),
            name: form.name.clone(),
            crash: false,
            screenshot,
            log: form.send_log.then(|| log_of(&game, &facts.playlog)),
            save: facts.save.clone().filter(|_| form.send_save),
            ..draft_of(facts)
        };
        let name = form.name.trim().to_string();
        let notice = match draft.build(report::new_id()) {
            Err(Problem::Empty) => Some("Önce ne olduğunu yaz.".to_string()),
            Err(Problem::TooLong) => Some("Yazı çok uzun.".to_string()),
            Ok(mut made) => {
                if no_room {
                    made.fields.omitted.push(Part::Screenshot);
                }
                self.keep(&made).err()
            }
        };
        match notice {
            Some(notice) => {
                if let Some(form) = self.screen.form.as_mut() {
                    form.notice = Some(notice);
                }
            }
            None => {
                let _ = fs::write(self.dir.join(NAME_FILE), name);
                self.close();
            }
        }
    }

    /// The report of the last run's crash; `facts` carry that run's save and play log.
    pub fn send_crash(&mut self, crash: &Crash, facts: Facts) {
        let first: Vec<&str> = crash.text.lines().take(CRASH_LINES).collect();
        let game = format!("{}\n== crash ==\n{}", log::tail(&self.dir.join(log::OLD_NAME), report::LOG_MAX), crash.text);
        let draft = Draft {
            text: clean(&format!("Oyun geçen sefer çöktü.\n{}", first.join("\n"))).chars().take(report::TEXT_MAX).collect(),
            name: fs::read_to_string(self.dir.join(NAME_FILE)).unwrap_or_default(),
            crash: true,
            log: Some(log_of(&game, &facts.playlog)),
            save: facts.save.clone(),
            ..draft_of(facts)
        };
        if let Ok(made) = draft.build(report::new_id())
            && let Err(why) = self.keep(&made)
        {
            self.say(&why);
        }
    }

    /// Write a report to the outbox and start sending.
    fn keep(&mut self, made: &Report) -> Result<(), String> {
        match outbox::put(&self.dir.join(OUTBOX), made) {
            Ok(()) => {
                self.flush(Some(made.fields.id.clone()));
                Ok(())
            }
            Err(PutError::Full) => {
                // sending what waits is what makes room
                self.flush(None);
                Err(FULL.to_string())
            }
            Err(PutError::Io(e)) => Err(format!("Rapor yazılamadı: {e}")),
        }
    }

    /// Send what waits, off the game's thread. `watch` is the report the player just wrote: they hear what became of
    /// it; of the others only a refusal is told.
    fn flush(&self, watch: Option<String>) {
        let (dir, transport, news) = (self.dir.join(OUTBOX), self.transport.clone(), self.news.0.clone());
        std::thread::spawn(move || {
            let done = outbox::flush(&dir, &mut |r| transport(r));
            let told = match watch {
                // another flush may have taken it: then the disk tells
                Some(id) => Some(done.iter().find(|d| d.0 == id).map(|d| d.1).unwrap_or_else(|| {
                    if dir.join(&id).exists() {
                        Outcome::Later
                    } else if dir.join("refused").join(&id).exists() {
                        Outcome::Refused
                    } else {
                        Outcome::Sent
                    }
                })),
                None => done.iter().any(|d| d.1 == Outcome::Refused).then_some(Outcome::Refused),
            };
            if let Some(told) = told {
                let _ = news.send(told);
            }
        });
    }

    fn say(&mut self, text: &str) {
        self.screen.toast = Some((text.to_string(), TOAST_SECONDS));
    }

    /// Once a frame: let the message fade and pick up what the sending thread found.
    pub fn update(&mut self, dt: f32) {
        if let Some((_, left)) = self.screen.toast.as_mut() {
            *left -= dt;
            if *left <= 0.0 {
                self.screen.toast = None;
            }
        }
        while let Ok(outcome) = self.news.1.try_recv() {
            log::line(&format!("geri bildirim: {outcome:?}"));
            self.say(match outcome {
                Outcome::Sent => "Gönderildi",
                Outcome::Later => "Sonra gönderilecek",
                Outcome::Refused => "Sunucu raporu kabul etmedi.",
            });
        }
    }
}

/// The part of a draft that comes from the game, not from the player.
fn draft_of(facts: Facts) -> Draft {
    Draft {
        kind: Kind::Bug,
        text: String::new(),
        name: String::new(),
        crash: false,
        game_version: env!("CARGO_PKG_VERSION").to_string(),
        os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        gpu: facts.gpu,
        window: facts.window,
        zoom: facts.zoom,
        seed: facts.seed,
        world_size: facts.world_size,
        day: facts.day,
        mode: facts.mode,
        screenshot: None,
        log: None,
        save: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{keys, menu, saveio, App, Args, Net};
    use std::time::Duration;
    use winit::keyboard::KeyCode;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("alaz-test-feedback-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A game in a new world whose reports go to a folder of its own and reach no server.
    fn app(name: &str, seed: u64) -> (App, PathBuf) {
        let mut app = App::new(Args { new_game: true, seed: Some(seed), hardcore: false, demo: false, creative: false, host: false, size: 256, port: 0, join: None });
        let dir = temp(name);
        app.feedback = Feedback::new(dir.clone(), offline());
        (app, dir)
    }

    /// Everything waiting in the outbox, taken out of it.
    fn queued(dir: &Path) -> Vec<Report> {
        let mut all = Vec::new();
        outbox::flush(&dir.join(OUTBOX), &mut |r| {
            all.push(r.clone());
            Outcome::Sent
        });
        all
    }

    fn open(app: &mut App) {
        app.input_key(KeyCode::F8, true, false);
        app.input_key(KeyCode::F8, false, false);
        let in_play = app.in_play();
        app.feedback.after_frame(None, false, in_play);
        assert!(app.feedback.screen.form.is_some());
    }

    fn write(app: &mut App, text: &str) {
        app.feedback.screen.form.as_mut().unwrap().text = text.into();
    }

    fn file(report: &Report, part: Part) -> Option<&[u8]> {
        report.files.iter().find(|f| f.0 == part).map(|f| f.1.as_slice())
    }

    fn frame(app: &mut App) {
        app.last = std::time::Instant::now() - Duration::from_millis(100);
        app.frame();
    }

    #[test]
    fn keys_belong_to_the_form_while_it_is_open() {
        let (mut app, _) = app("keys", 987_001);
        app.input_key(KeyCode::KeyD, true, false);
        open(&mut app);
        assert!(app.keys.iter().all(|&k| k == KeyCode::F8), "a held key lets go: {:?}", app.keys);
        for code in [KeyCode::KeyW, KeyCode::KeyM, KeyCode::KeyC, KeyCode::F5] {
            app.input_key(code, true, false);
            assert!(!app.keys.contains(&code));
        }
        assert!(!app.knap.map.open && app.tab.is_none());
        let zoom = app.cam.zoom;
        app.input_wheel(3.0);
        assert_eq!(app.cam.zoom, zoom);
        app.input_key(KeyCode::Escape, true, false);
        assert!(!app.feedback.blocking());
        assert!(app.menu.is_none(), "escape only closed the form");
        app.input_key(KeyCode::KeyM, true, false);
        assert!(app.knap.map.open);
    }

    #[test]
    fn a_solo_world_waits_for_the_form() {
        let (mut app, _) = app("pause", 987_002);
        open(&mut app);
        let tick = app.world.tick;
        for _ in 0..3 {
            frame(&mut app);
        }
        assert_eq!(app.world.tick, tick);
        app.input_key(KeyCode::Escape, true, false);
        frame(&mut app);
        assert!(app.world.tick > tick);
    }

    #[test]
    fn a_shared_world_runs_on_under_the_form() {
        let (mut app, _) = app("shared", 987_003);
        app.net = Net::Host(alaz_net::Host::bind("127.0.0.1:0").unwrap());
        open(&mut app);
        let tick = app.world.tick;
        frame(&mut app);
        assert!(app.world.tick > tick);
    }

    #[test]
    fn a_report_from_play_carries_log_and_save_and_writes_no_save() {
        let (mut app, dir) = app("play", 987_004);
        app.no_save = false;
        fs::write(dir.join(alaz_feedback::log::NAME), "[+1.0s] bağlandı 192.168.1.9:7777\n").unwrap();
        open(&mut app);
        write(&mut app, "ağaç yok oldu");
        app.feedback.screen.form.as_mut().unwrap().name = "Ece".into();
        app.send_report();
        assert!(!app.feedback.blocking(), "the form closes at once");
        let all = queued(&dir);
        assert_eq!(all.len(), 1);
        let f = &all[0].fields;
        assert_eq!((f.text.as_str(), f.mode, f.crash, f.seed.clone()), ("ağaç yok oldu", Mode::Solo, false, Some(app.world.seed.to_string())));
        assert_eq!((f.world_size, f.day, f.name.as_deref()), (Some(256), Some(1), Some("Ece")));
        assert_eq!(f.game_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(file(&all[0], Part::Save), Some(app.world.to_bytes().as_slice()));
        let log = String::from_utf8(file(&all[0], Part::Log).unwrap().to_vec()).unwrap();
        assert!(log.contains("bağlandı <addr>"), "{log}");
        assert_eq!(file(&all[0], Part::Screenshot), None);
        for slot in 1..=saveio::SLOTS {
            assert_ne!(saveio::main_save_in(&saveio::slot_dir(slot)), Some(app.world.to_bytes()));
        }
        open(&mut app);
        assert_eq!(app.feedback.screen.form.as_ref().unwrap().name, "Ece");
    }

    #[test]
    fn the_picture_goes_along_and_boxes_leave_parts_out() {
        let (mut app, dir) = app("shot", 987_005);
        app.open_report();
        app.feedback.after_frame(Some(Capture { width: 640, height: 360, rgb: vec![40; 640 * 360 * 3] }), true, true);
        write(&mut app, "resimli");
        app.send_report();
        app.open_report();
        app.feedback.after_frame(Some(Capture { width: 640, height: 360, rgb: vec![40; 640 * 360 * 3] }), true, true);
        write(&mut app, "boş");
        let form = app.feedback.screen.form.as_mut().unwrap();
        (form.send_shot, form.send_log, form.send_save) = (false, false, false);
        app.send_report();
        let mut all = queued(&dir);
        all.sort_by_key(|r| r.fields.text.clone());
        assert!(all[0].files.is_empty() && all[0].fields.omitted.is_empty(), "{:?}", all[0].fields);
        assert!(file(&all[1], Part::Screenshot).unwrap().starts_with(b"\x89PNG"));
    }

    #[test]
    fn the_form_waits_a_few_frames_for_the_picture() {
        let (mut app, _) = app("wait", 987_006);
        app.open_report();
        for _ in 0..CAPTURE_FRAMES - 1 {
            assert!(app.feedback.capturing() && app.feedback.blocking());
            app.feedback.after_frame(None, true, true);
            assert!(app.feedback.screen.form.is_none());
        }
        app.feedback.after_frame(None, true, true);
        assert!(app.feedback.screen.form.is_some() && !app.feedback.capturing());
    }

    #[test]
    fn the_menu_opens_it_and_sends_no_world() {
        let (mut app, dir) = app("menu", 987_007);
        app.menu = Some(menu::Menu::new(false));
        open(&mut app);
        app.input_key(KeyCode::Escape, true, false);
        assert!(app.menu.is_some());
        app.menu_action(menu::MenuAction::Report);
        let in_play = app.in_play();
        app.feedback.after_frame(None, false, in_play);
        write(&mut app, "menüden");
        app.send_report();
        let all = queued(&dir);
        let f = &all[0].fields;
        assert_eq!((f.mode, f.seed.clone(), f.day, f.world_size), (Mode::Menu, None, None, None));
        assert_eq!(file(&all[0], Part::Save), None);
        // the pause menu is over a world, and that world can be sent
        app.menu = Some(menu::Menu::new(true));
        assert!(app.in_play());
    }

    #[test]
    fn a_full_outbox_keeps_the_form_and_the_text() {
        let (mut app, dir) = app("full", 987_008);
        for n in 0..outbox::WAITING_MAX {
            open(&mut app);
            write(&mut app, &format!("rapor {n}"));
            app.send_report();
        }
        open(&mut app);
        write(&mut app, "on birinci");
        app.send_report();
        let form = app.feedback.screen.form.as_ref().expect("still open");
        assert_eq!(form.text, "on birinci");
        assert_eq!(form.notice.as_deref(), Some("Bekleyen 10 rapor var; yenisi eklenemiyor."));
        assert_eq!(queued(&dir).len(), 10);
    }

    #[test]
    fn after_a_crash_the_player_is_asked_once() {
        for (yes, count) in [(false, 0), (true, 1)] {
            let (mut app, dir) = app(if yes { "crash-yes" } else { "crash-no" }, 987_009);
            fs::write(dir.join(alaz_feedback::log::OLD_NAME), "[+9.0s] son satır\n").unwrap();
            let long: String = (0..60).map(|n| format!("iz{n}\n")).collect();
            alaz_feedback::crash::write(&dir, 2, &format!("panicked at 10.0.0.1:99\n{long}"));
            app.menu = Some(menu::Menu::new(false));
            app.feedback.at_start();
            assert!(app.feedback.screen.crash.is_some() && app.feedback.blocking());
            app.answer_crash(yes);
            assert!(!app.feedback.blocking());
            let all = queued(&dir);
            assert_eq!(all.len(), count);
            if let Some(r) = all.first() {
                assert!(r.fields.crash && r.fields.kind == Kind::Bug && r.fields.mode == Mode::Menu);
                assert!(r.fields.text.starts_with("Oyun geçen sefer çöktü.\npanicked at <addr>\niz0"), "{}", r.fields.text);
                assert!(r.fields.text.contains("iz18\n") || r.fields.text.ends_with("iz18"));
                assert!(!r.fields.text.contains("iz19"), "twenty lines of the crash");
                let log = String::from_utf8(file(r, Part::Log).unwrap().to_vec()).unwrap();
                assert!(log.contains("son satır") && log.contains("iz59"), "{log}");
                assert_eq!(file(r, Part::Screenshot), None);
            }
            app.feedback.at_start();
            assert!(app.feedback.screen.crash.is_none());
        }
    }

    #[test]
    fn the_message_says_what_became_of_the_report() {
        for (answer, said) in [(Outcome::Sent, "Gönderildi"), (Outcome::Later, "Sonra gönderilecek"), (Outcome::Refused, "Sunucu raporu kabul etmedi.")] {
            let (mut app, dir) = app(&format!("toast-{said}"), 987_010);
            app.feedback = Feedback::new(dir.clone(), Arc::new(move |_| answer));
            open(&mut app);
            write(&mut app, "x");
            app.send_report();
            let t0 = std::time::Instant::now();
            while app.feedback.screen.toast.is_none() {
                app.feedback.update(0.0);
                assert!(t0.elapsed() < Duration::from_secs(5), "no message");
                std::thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(app.feedback.screen.toast.as_ref().unwrap().0, said);
            app.feedback.update(TOAST_SECONDS + 1.0);
            assert!(app.feedback.screen.toast.is_none());
        }
    }

    #[test]
    fn the_save_of_a_slot_is_read_as_it_is() {
        let dir = temp("mainsave");
        assert_eq!(saveio::main_save_in(&dir), None);
        fs::write(dir.join("save.alz"), b"ALAZxx").unwrap();
        assert_eq!(saveio::main_save_in(&dir), Some(b"ALAZxx".to_vec()));
    }

    #[test]
    fn the_feedback_key_can_be_rebound() {
        let (mut app, _) = app("rebound", 987_011);
        app.binds.set(keys::Action::Report, 0, Some(KeyCode::F7));
        app.input_key(KeyCode::F7, true, false);
        assert!(app.feedback.blocking());
    }
}
