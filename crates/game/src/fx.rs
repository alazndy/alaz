//! The opening and the way into a world: a title screen when the game starts (the letters of A.L.A.Z dropping in
//! one by one over rising embers), and when a world begins, darkness opening in a widening circle while the camera
//! swoops down from high above, with a card naming the day, the season and the age. Any key or click skips the
//! title screen; the way in only lasts a few seconds and does not stop play.

use alaz_render::egui::{self, pos2, vec2, Align2, Color32, FontId, Rect, Stroke};
use alaz_render::Camera;

/// How long each part lasts (seconds).
const SPLASH: f32 = 3.4;
const ENTER: f32 = 3.8;
/// The camera's swoop down into the world.
const SWOOP: f32 = 2.6;

#[derive(Default)]
pub struct Fx {
    splash: Option<f32>,
    enter: Option<f32>,
    title: String,
    subtitle: String,
}

fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

impl Fx {
    /// The game has just started: show the title screen.
    pub fn boot(&mut self) {
        self.splash = Some(0.0);
    }

    /// A world has just been entered (new, loaded or grown from the last).
    pub fn enter_world(&mut self, w: &alaz_sim::World) {
        self.splash = None;
        self.enter = Some(0.0);
        let season = ["Bahar", "Yaz", "Güz", "Kış"][w.season() as usize];
        self.title = format!("{} {}  ·  {}", crate::lang::day_word(w), w.day() + 1, season);
        self.subtitle = crate::hud::era_name(w).to_string();
    }

    pub fn update(&mut self, dt: f32) {
        if let Some(t) = self.splash.as_mut() {
            *t += dt;
            if *t >= SPLASH {
                self.splash = None;
            }
        }
        if let Some(t) = self.enter.as_mut() {
            *t += dt;
            if *t >= ENTER {
                self.enter = None;
            }
        }
    }

    /// The way into a world is still playing (the story waits for it).
    pub fn entering(&self) -> bool {
        self.enter.is_some()
    }

    /// While the title screen is up, the menu waits under it (and input goes to skipping it).
    pub fn splash_up(&self) -> bool {
        self.splash.is_some()
    }

    /// The camera as drawn while the way in plays: from high above and far out, down to where it really is.
    pub fn camera(&self, cam: &mut Camera) {
        let Some(t) = self.enter else { return };
        let k = ease_out((t - 0.2) / SWOOP);
        cam.zoom *= 0.42 + 0.58 * k;
        cam.pitch_deg += (82.0 - cam.pitch_deg) * (1.0 - k);
        cam.yaw_deg -= 35.0 * (1.0 - k);
    }

    pub fn draw(&mut self, ctx: &egui::Context) {
        if let Some(t) = self.splash {
            // any key or click skips it
            let skip = ctx.input(|i| i.pointer.any_pressed() || i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. })));
            if skip && t > 0.25 {
                self.splash = None;
            } else {
                splash(ctx, t);
            }
        }
        if let Some(t) = self.enter {
            enter(ctx, t, &self.title, &self.subtitle);
        }
    }
}

fn layer(ctx: &egui::Context, name: &str) -> egui::Painter {
    ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new(name)))
}

/// The title screen.
fn splash(ctx: &egui::Context, t: f32) {
    let p = layer(ctx, "fx_splash");
    let r = ctx.content_rect();
    // everything fades out at the end, showing the menu beneath
    let out = 1.0 - ((t - (SPLASH - 0.6)) / 0.6).clamp(0.0, 1.0);
    let a = |v: f32| (v * out * 255.0) as u8;
    p.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(14, 9, 6, a(1.0)));
    // embers drifting up from the bottom, each on its own path (a fixed scatter, so it looks the same every time)
    for k in 0..70u32 {
        let h = k.wrapping_mul(2_654_435_761);
        let speed = 40.0 + (h >> 8 & 63) as f32;
        let life = r.height() + 60.0;
        let y = r.bottom() + 20.0 - ((t * speed + (h >> 16 & 1023) as f32) % life);
        let x = r.left() + (h & 1023) as f32 / 1023.0 * r.width() + (t * 1.3 + k as f32).sin() * 14.0;
        let s = 2.0 + (h >> 20 & 3) as f32;
        let glow = 0.4 + 0.6 * ((t * 5.0 + k as f32 * 0.7).sin() * 0.5 + 0.5);
        p.rect_filled(Rect::from_min_size(pos2(x.round(), y.round()), vec2(s, s)), 0.0, Color32::from_rgba_unmultiplied(255, (120.0 + 80.0 * glow) as u8, 40, a(0.85 * glow)));
    }
    // the letters drop in one by one and settle with a little bounce
    let word = ['A', '.', 'L', '.', 'A', '.', 'Z'];
    let size = 132.0;
    let font = FontId::proportional(size);
    let widths: Vec<f32> = word.iter().map(|c| p.layout_no_wrap(c.to_string(), font.clone(), Color32::WHITE).size().x).collect();
    let total: f32 = widths.iter().sum();
    let mut x = r.center().x - total / 2.0;
    let base = r.center().y - 30.0;
    for (i, c) in word.iter().enumerate() {
        let start = 0.15 + i as f32 * 0.14;
        let lt = ((t - start) / 0.5).clamp(0.0, 1.0);
        if lt > 0.0 {
            // fall, then a bounce
            let drop = if lt < 0.7 { (1.0 - lt / 0.7).powi(2) * -260.0 } else { ((lt - 0.7) / 0.3 * std::f32::consts::PI).sin() * -14.0 };
            let y = (base + drop).round();
            let gold = Color32::from_rgba_unmultiplied(244, 196, 92, a(lt.min(1.0)));
            p.text(pos2(x.round() + 6.0, y + 6.0), Align2::LEFT_CENTER, c.to_string(), font.clone(), Color32::from_rgba_unmultiplied(60, 26, 10, a(lt)));
            p.text(pos2(x.round(), y), Align2::LEFT_CENTER, c.to_string(), font.clone(), gold);
        }
        x += widths[i];
    }
    // a line, then the subtitle typed out
    let lt = ((t - 1.25) / 0.4).clamp(0.0, 1.0);
    if lt > 0.0 {
        let w = total * lt;
        p.rect_filled(Rect::from_min_size(pos2((r.center().x - w / 2.0).round(), base + 70.0), vec2(w.round(), 4.0)), 0.0, Color32::from_rgba_unmultiplied(214, 110, 40, a(1.0)));
    }
    let sub = "Ape Learns Another Zing";
    let shown = (((t - 1.5) / 0.9).clamp(0.0, 1.0) * sub.chars().count() as f32) as usize;
    if shown > 0 {
        let s: String = sub.chars().take(shown).collect();
        p.text(pos2(r.center().x - 0.5 * p.layout_no_wrap(sub.to_string(), FontId::proportional(33.0), Color32::WHITE).size().x, base + 110.0), Align2::LEFT_CENTER, s, FontId::proportional(33.0), Color32::from_rgba_unmultiplied(236, 226, 206, a(1.0)));
    }
    if t > 2.0 {
        let blink = ((t * 3.0).sin() * 0.5 + 0.5) * ((t - 2.0) / 0.3).clamp(0.0, 1.0);
        p.text(pos2(r.center().x, r.bottom() - 60.0), Align2::CENTER_CENTER, "bir tuşa bas", FontId::proportional(22.0), Color32::from_rgba_unmultiplied(170, 154, 126, a(blink)));
    }
}

/// The way into a world.
fn enter(ctx: &egui::Context, t: f32, title: &str, subtitle: &str) {
    let p = layer(ctx, "fx_enter");
    let r = ctx.content_rect();
    let c = r.center();
    let far = r.width().hypot(r.height());
    // darkness, then a circle opening from the middle (where the player stands)
    let open = ease_out((t - 0.35) / 1.5);
    if open < 1.0 {
        let hole = (open * far * 0.55).max(0.0);
        if hole < 1.0 {
            p.rect_filled(r, 0.0, Color32::BLACK);
        } else {
            // one thick ring covers everything outside the hole
            let width = far;
            p.circle_stroke(c, hole + width / 2.0, Stroke::new(width, Color32::BLACK));
            // a warm rim on the circle's edge
            p.circle_stroke(c, hole + 3.0, Stroke::new(6.0, Color32::from_rgba_unmultiplied(214, 110, 40, ((1.0 - open) * 200.0) as u8)));
        }
    }
    // the card: the day, the season and the age
    let fade = ((t - 0.8) / 0.5).clamp(0.0, 1.0) * (1.0 - ((t - (ENTER - 0.8)) / 0.8).clamp(0.0, 1.0));
    if fade > 0.0 {
        let a = |v: f32| (v * fade * 255.0) as u8;
        let y = r.top() + r.height() * 0.24;
        let band = Rect::from_center_size(pos2(c.x, y + 22.0), vec2(r.width(), 110.0));
        p.rect_filled(band, 0.0, Color32::from_rgba_unmultiplied(10, 6, 4, a(0.55)));
        p.text(pos2(c.x + 4.0, y + 4.0), Align2::CENTER_CENTER, title, FontId::proportional(44.0), Color32::from_rgba_unmultiplied(40, 20, 8, a(1.0)));
        p.text(pos2(c.x, y), Align2::CENTER_CENTER, title, FontId::proportional(44.0), Color32::from_rgba_unmultiplied(246, 206, 112, a(1.0)));
        let w = 260.0 * ((t - 1.0) / 0.5).clamp(0.0, 1.0);
        p.rect_filled(Rect::from_min_size(pos2((c.x - w / 2.0).round(), y + 30.0), vec2(w.round(), 4.0)), 0.0, Color32::from_rgba_unmultiplied(214, 110, 40, a(1.0)));
        p.text(pos2(c.x, y + 56.0), Align2::CENTER_CENTER, subtitle, FontId::proportional(22.0), Color32::from_rgba_unmultiplied(236, 226, 206, a(1.0)));
    }
}
