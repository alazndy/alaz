//! The feedback form, the question after a crash, and the short message that follows. Drawing only: what is sent,
//! and when, is decided in `feedback`.

use alaz_feedback::report::{Kind, NAME_MAX, TEXT_MAX};
use alaz_render::egui::{self, Align2, Color32, RichText};

use crate::hud::panel;

const WIDTH: f32 = 520.0;
const THUMB: f32 = 240.0;
/// Height taken by everything outside the scrolling middle, a notice included.
const FRAME_HEIGHT: f32 = 170.0;
const WARN: Color32 = Color32::from_rgb(240, 110, 110);

#[derive(Default)]
pub(crate) struct Form {
    pub kind: Kind,
    pub text: String,
    pub name: String,
    pub send_shot: bool,
    pub send_log: bool,
    pub send_save: bool,
    /// A small copy of the captured picture (rows of RGB, width, height).
    preview: Option<(Vec<u8>, u32, u32)>,
    thumb: Option<egui::TextureHandle>,
    has_save: bool,
    /// Why the last try to send did not go through.
    pub notice: Option<String>,
    focused: bool,
}

impl Form {
    /// Everything there is to send is switched on.
    pub fn new(name: String, preview: Option<(Vec<u8>, u32, u32)>, has_save: bool) -> Self {
        Self { name, send_shot: preview.is_some(), send_log: true, send_save: has_save, preview, has_save, ..Self::default() }
    }

    fn ready(&self) -> bool {
        !self.text.trim().is_empty() && self.text.chars().count() <= TEXT_MAX
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FormAct {
    Send,
    Cancel,
}

/// A window above everything else; nothing under it can be clicked.
fn window<R>(ctx: &egui::Context, id: &str, add: impl FnOnce(&mut egui::Ui, egui::Rect) -> R) -> R {
    let screen = ctx.content_rect();
    let id = egui::Id::NULL.with(id);
    egui::Modal::new(id)
        // above the game's own windows, also one that appears after this
        .area(egui::Modal::default_area(id).order(egui::Order::Tooltip))
        .frame(egui::Frame::NONE)
        // the story and the hints behind would show through the panel
        .backdrop_color(Color32::from_black_alpha(150))
        .show(ctx, |ui| panel(ui, 10.0, |ui| add(ui, screen)).inner)
        .inner
}

pub(crate) fn draw_form(ctx: &egui::Context, form: &mut Form) -> Option<FormAct> {
    window(ctx, "feedback_form", |ui, screen| {
        let mut act = None;
        let width = WIDTH.min(screen.width() - 48.0);
        ui.set_width(width);
        ui.heading("Geri bildirim");
        egui::ScrollArea::vertical().id_salt("feedback_scroll").max_height((screen.height() - FRAME_HEIGHT).max(80.0)).show(ui, |ui| {
            ui.set_width(width);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut form.kind, Kind::Bug, "Hata");
                ui.selectable_value(&mut form.kind, Kind::Idea, "İstek");
            });
            let label = ui.label("Ne oldu?");
            let text = ui.add(egui::TextEdit::multiline(&mut form.text).desired_rows(5).desired_width(f32::INFINITY)).labelled_by(label.id);
            if !form.focused {
                text.request_focus();
                form.focused = true;
            }
            let count = form.text.chars().count();
            let counter = RichText::new(format!("{count} / {TEXT_MAX}")).small();
            ui.label(if count > TEXT_MAX { counter.color(WARN) } else { counter });
            ui.horizontal(|ui| {
                let label = ui.label("İsmin (istersen)");
                ui.add(egui::TextEdit::singleline(&mut form.name).char_limit(NAME_MAX).desired_width(f32::INFINITY)).labelled_by(label.id);
            });
            ui.horizontal_wrapped(|ui| {
                ui.add_enabled(form.preview.is_some(), egui::Checkbox::new(&mut form.send_shot, "Ekran görüntüsü"));
                ui.checkbox(&mut form.send_log, "Loglar");
                ui.add_enabled(form.has_save, egui::Checkbox::new(&mut form.send_save, "Kayıt dosyası"));
            });
            if let Some((rgb, w, h)) = form.preview.as_ref().filter(|_| form.send_shot) {
                let thumb = form.thumb.get_or_insert_with(|| {
                    let image = egui::ColorImage::from_rgb([*w as usize, *h as usize], rgb);
                    ui.ctx().load_texture("feedback_shot", image, egui::TextureOptions::LINEAR)
                });
                let wide = THUMB.min(width);
                ui.image((thumb.id(), egui::vec2(wide, wide * *h as f32 / (*w).max(1) as f32)));
            }
            ui.label(RichText::new("Raporu geliştirici ve davetli testçiler görür.").small());
        });
        if let Some(notice) = &form.notice {
            ui.label(RichText::new(notice).color(WARN));
        }
        ui.horizontal(|ui| {
            if ui.add_enabled(form.ready(), egui::Button::new("Gönder")).clicked() {
                act = Some(FormAct::Send);
            }
            if ui.button("Vazgeç").clicked() {
                act = Some(FormAct::Cancel);
            }
        });
        act
    })
}

/// `Some(true)` to send the report of the last crash, `Some(false)` to let it be.
pub(crate) fn draw_crash_prompt(ctx: &egui::Context) -> Option<bool> {
    window(ctx, "feedback_crash", |ui, screen| {
        let mut answer = None;
        ui.set_width(380.0f32.min(screen.width() - 48.0));
        ui.label("Oyun geçen sefer çöktü. Rapor gönderilsin mi?");
        ui.label(RichText::new("Çökme bilgisi, loglar ve son kayıt gider. Raporu geliştirici ve davetli testçiler görür.").small());
        ui.horizontal(|ui| {
            if ui.button("Gönder").clicked() {
                answer = Some(true);
            }
            if ui.button("Hayır").clicked() {
                answer = Some(false);
            }
        });
        answer
    })
}

/// A short message at the bottom of the screen; it takes no clicks.
pub(crate) fn draw_toast(ctx: &egui::Context, text: &str) {
    egui::Area::new(egui::Id::NULL.with("feedback_toast"))
        .anchor(Align2::CENTER_BOTTOM, [0.0, -64.0])
        .order(egui::Order::Tooltip)
        .interactable(false)
        .show(ctx, |ui| {
            panel(ui, 8.0, |ui| ui.add(egui::Label::new(text).extend()));
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;

    type State = (Form, Vec<FormAct>);

    fn harness(form: Form, size: egui::Vec2) -> egui_kittest::Harness<'static, State> {
        let mut h = egui_kittest::Harness::builder().with_size(size).build_ui_state(
            |ui, (form, acts): &mut State| acts.extend(draw_form(ui.ctx(), form)),
            (form, Vec::new()),
        );
        h.run_steps(3);
        h
    }

    fn picture() -> Option<(Vec<u8>, u32, u32)> {
        Some((vec![90; 320 * 180 * 3], 320, 180))
    }

    fn click(h: &mut egui_kittest::Harness<'static, State>, label: &str) {
        h.get_by_label(label).click();
        h.run_steps(3);
    }

    #[test]
    fn nothing_is_sent_until_something_is_written() {
        let mut empty = harness(Form::new(String::new(), picture(), true), egui::vec2(1000.0, 800.0));
        click(&mut empty, "Gönder");
        assert!(empty.state().1.is_empty());
        let mut h = harness(Form::new(String::new(), picture(), true), egui::vec2(1000.0, 800.0));
        // the text box has the cursor as soon as the form opens
        h.event(egui::Event::Text("Ağaç yok oldu".into()));
        h.run_steps(3);
        assert_eq!(h.state().0.text, "Ağaç yok oldu");
        assert!(h.query_by_label("13 / 4000").is_some());
        click(&mut h, "Gönder");
        assert_eq!(h.state().1, vec![FormAct::Send]);
    }

    #[test]
    fn too_long_a_text_cannot_be_sent() {
        let mut form = Form::new(String::new(), None, false);
        form.text = "x".repeat(4001);
        let mut h = harness(form, egui::vec2(1000.0, 800.0));
        click(&mut h, "Gönder");
        assert!(h.state().1.is_empty());
        assert!(h.query_by_label("4001 / 4000").is_some());
    }

    #[test]
    fn cancel_and_the_kind() {
        let mut h = harness(Form::new("Ece".into(), None, false), egui::vec2(1000.0, 800.0));
        assert!(h.query_by_label("Raporu geliştirici ve davetli testçiler görür.").is_some());
        assert_eq!(h.state().0.kind, Kind::Bug);
        click(&mut h, "İstek");
        assert_eq!(h.state().0.kind, Kind::Idea);
        click(&mut h, "Hata");
        assert_eq!(h.state().0.kind, Kind::Bug);
        click(&mut h, "Vazgeç");
        assert_eq!(h.state().1, vec![FormAct::Cancel]);
        assert_eq!(h.state().0.name, "Ece");
    }

    #[test]
    fn the_boxes_are_on_for_what_there_is() {
        let mut full = harness(Form::new(String::new(), picture(), true), egui::vec2(1000.0, 800.0));
        let f = &full.state().0;
        assert!(f.send_shot && f.send_log && f.send_save);
        for label in ["Ekran görüntüsü", "Loglar", "Kayıt dosyası"] {
            click(&mut full, label);
        }
        let f = &full.state().0;
        assert!(!f.send_shot && !f.send_log && !f.send_save);
        let mut bare = harness(Form::new(String::new(), None, false), egui::vec2(1000.0, 800.0));
        for label in ["Ekran görüntüsü", "Kayıt dosyası"] {
            click(&mut bare, label);
        }
        let f = &bare.state().0;
        assert!(!f.send_shot && f.send_log && !f.send_save);
    }

    #[test]
    fn the_buttons_stay_in_a_small_window_at_a_large_interface_size() {
        let mut form = Form::new(String::new(), picture(), true);
        form.text = "satır\n".repeat(200);
        form.notice = Some("Bekleyen 10 rapor var; yenisi eklenemiyor.".into());
        let mut h = harness(form, egui::vec2(800.0, 600.0));
        h.ctx.set_zoom_factor(1.5);
        h.run_steps(5);
        let window = h.ctx.content_rect();
        for label in ["Gönder", "Vazgeç", "Bekleyen 10 rapor var; yenisi eklenemiyor."] {
            let r = h.get_by_label(label).rect();
            assert!(window.contains_rect(r), "{label}: {r:?} outside {window:?}");
        }
    }

    #[test]
    fn a_panel_that_appears_later_does_not_lie_over_the_form() {
        type Late = (Form, Vec<FormAct>, bool);
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1000.0, 800.0)).build_ui_state(
            |ui, (form, acts, late): &mut Late| {
                acts.extend(draw_form(ui.ctx(), form));
                if *late {
                    egui::Area::new(egui::Id::NULL.with("late")).order(egui::Order::Foreground).fixed_pos([0.0, 0.0]).show(ui.ctx(), |ui| {
                        let _ = ui.allocate_exact_size(egui::vec2(1000.0, 800.0), egui::Sense::click());
                    });
                }
            },
            (Form::new(String::new(), None, false), Vec::new(), false),
        );
        h.run_steps(3);
        h.state_mut().2 = true;
        h.run_steps(3);
        let at = h.get_by_label("Vazgeç").rect().center();
        for pressed in [true, false] {
            h.event(egui::Event::PointerMoved(at));
            h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() });
            h.step();
        }
        h.step();
        assert_eq!(h.state().1, vec![FormAct::Cancel]);
    }

    #[test]
    fn the_question_after_a_crash_takes_either_answer() {
        for (label, answer) in [("Gönder", true), ("Hayır", false)] {
            let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(800.0, 600.0)).build_ui_state(
                |ui, got: &mut Vec<bool>| got.extend(draw_crash_prompt(ui.ctx())),
                Vec::new(),
            );
            h.run_steps(3);
            assert!(h.query_by_label("Oyun geçen sefer çöktü. Rapor gönderilsin mi?").is_some());
            h.get_by_label(label).click();
            h.run_steps(3);
            assert_eq!(h.state(), &vec![answer]);
        }
    }
}
