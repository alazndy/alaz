//! Painted widgets for the HUD: little glyphs, icon bars, icon buttons and chips. No image files, only egui shapes.

use alaz_render::egui::{self, Align2, Color32, FontId, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Heart,
    Food,
    Drop,
    Flame,
    Breath,
    Bag,
    Hammer,
    House,
    Flag,
    Route,
    Help,
    Sun,
    Moon,
    Cloud,
    Rain,
    Snow,
    Eye,
    Bolt,
    Plus,
    Leg,
    Skull,
}

fn p2(c: Pos2, s: f32, x: f32, y: f32) -> Pos2 {
    Pos2::new(c.x + x * s, c.y + y * s)
}

/// Draw `g` centred on `c`, about `s` points across. `col` is the main colour.
pub fn glyph(p: &Painter, c: Pos2, s: f32, g: Glyph, col: Color32) {
    let q = |x: f32, y: f32| p2(c, s, x, y);
    let dark = Color32::from_black_alpha(130);
    let shine = Color32::from_white_alpha(120);
    let line = |a: (f32, f32), b: (f32, f32), w: f32, k: Color32| {
        p.line_segment([q(a.0, a.1), q(b.0, b.1)], Stroke::new((w * s).max(1.0), k));
    };
    let poly = |pts: &[(f32, f32)], fill: Color32| {
        p.add(Shape::convex_polygon(pts.iter().map(|&(x, y)| q(x, y)).collect(), fill, Stroke::new(1.0, dark)));
    };
    match g {
        Glyph::Heart => {
            p.circle_filled(q(-0.17, -0.12), 0.23 * s, col);
            p.circle_filled(q(0.17, -0.12), 0.23 * s, col);
            poly(&[(-0.38, -0.02), (0.38, -0.02), (0.0, 0.42)], col);
            p.circle_filled(q(-0.22, -0.18), 0.07 * s, shine);
        }
        Glyph::Food => {
            // a drumstick: meat on one end, bone on the other
            line((0.0, 0.0), (0.34, 0.34), 0.11, Color32::from_rgb(235, 225, 200));
            p.circle_filled(q(0.38, 0.28), 0.08 * s, Color32::from_rgb(235, 225, 200));
            p.circle_filled(q(0.3, 0.4), 0.08 * s, Color32::from_rgb(235, 225, 200));
            p.circle_filled(q(-0.12, -0.12), 0.3 * s, col);
            p.circle_filled(q(-0.2, -0.2), 0.09 * s, shine);
        }
        Glyph::Drop => {
            p.circle_filled(q(0.0, 0.16), 0.27 * s, col);
            poly(&[(-0.25, 0.08), (0.0, -0.42), (0.25, 0.08)], col);
            p.circle_filled(q(-0.1, 0.12), 0.07 * s, shine);
        }
        Glyph::Flame => {
            poly(&[(0.0, -0.46), (0.3, 0.04), (0.28, 0.28), (0.0, 0.44), (-0.28, 0.28), (-0.3, 0.02), (-0.1, -0.12)], col);
            poly(&[(0.0, -0.06), (0.14, 0.22), (0.0, 0.38), (-0.14, 0.22)], Color32::from_rgb(255, 224, 120));
        }
        Glyph::Breath => {
            for (y, len) in [(-0.22, 0.4), (0.0, 0.5), (0.22, 0.34)] {
                line((-0.42, y), (-0.42 + len, y), 0.09, col);
                p.circle_filled(q(-0.42 + len + 0.07, y - 0.05), 0.07 * s, col);
            }
        }
        Glyph::Bag => {
            p.rect_filled(Rect::from_center_size(q(0.0, 0.1), egui::vec2(0.7 * s, 0.6 * s)), 0.18 * s, col);
            p.circle_filled(q(0.0, -0.3), 0.14 * s, col);
            line((-0.3, 0.0), (0.3, 0.0), 0.06, dark);
            p.circle_filled(q(0.0, 0.0), 0.06 * s, shine);
        }
        Glyph::Hammer => {
            line((-0.3, 0.4), (0.18, -0.12), 0.12, Color32::from_rgb(150, 108, 66));
            poly(&[(0.0, -0.4), (0.42, -0.1), (0.3, 0.06), (-0.12, -0.24)], col);
        }
        Glyph::House => {
            poly(&[(-0.44, -0.04), (0.0, -0.42), (0.44, -0.04)], col);
            p.rect_filled(Rect::from_center_size(q(0.0, 0.2), egui::vec2(0.6 * s, 0.4 * s)), 0.0, col);
            p.rect_filled(Rect::from_center_size(q(0.0, 0.28), egui::vec2(0.16 * s, 0.24 * s)), 0.0, dark);
        }
        Glyph::Flag => {
            line((-0.28, -0.42), (-0.28, 0.44), 0.07, Color32::from_rgb(190, 160, 120));
            poly(&[(-0.26, -0.4), (0.4, -0.2), (-0.26, 0.06)], col);
        }
        Glyph::Route => {
            p.circle_filled(q(-0.32, 0.26), 0.12 * s, col);
            p.circle_filled(q(0.32, -0.26), 0.12 * s, col);
            line((-0.28, 0.2), (0.0, 0.0), 0.06, col);
            line((0.0, 0.0), (0.26, -0.2), 0.06, col);
        }
        Glyph::Help => {
            p.text(c, Align2::CENTER_CENTER, "?", FontId::proportional(s * 0.95), col);
        }
        Glyph::Sun => {
            p.circle_filled(c, 0.24 * s, col);
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::FRAC_PI_4;
                let (sn, cs) = a.sin_cos();
                line((cs * 0.34, sn * 0.34), (cs * 0.48, sn * 0.48), 0.07, col);
            }
        }
        Glyph::Moon => {
            p.circle_filled(c, 0.34 * s, col);
            p.circle_filled(q(0.16, -0.1), 0.3 * s, Color32::from_rgb(24, 20, 17));
        }
        Glyph::Cloud | Glyph::Rain | Glyph::Snow => {
            p.circle_filled(q(-0.18, 0.0), 0.2 * s, col);
            p.circle_filled(q(0.06, -0.12), 0.26 * s, col);
            p.circle_filled(q(0.26, 0.04), 0.18 * s, col);
            p.rect_filled(Rect::from_min_max(q(-0.18, 0.0), q(0.26, 0.2)), 0.0, col);
            for k in 0..3 {
                let x = -0.2 + k as f32 * 0.2;
                match g {
                    Glyph::Rain => line((x, 0.28), (x - 0.06, 0.46), 0.06, Color32::from_rgb(120, 170, 235)),
                    Glyph::Snow => {
                        p.circle_filled(q(x, 0.36), 0.05 * s, Color32::WHITE);
                    }
                    _ => {}
                }
            }
        }
        Glyph::Eye => {
            poly(&[(-0.46, 0.0), (-0.2, -0.22), (0.2, -0.22), (0.46, 0.0), (0.2, 0.22), (-0.2, 0.22)], col);
            p.circle_filled(c, 0.14 * s, Color32::from_rgb(30, 36, 50));
            p.circle_filled(q(-0.04, -0.04), 0.04 * s, Color32::WHITE);
        }
        Glyph::Bolt => {
            poly(&[(0.1, -0.46), (-0.28, 0.06), (-0.02, 0.06), (-0.12, 0.46), (0.3, -0.1), (0.04, -0.1)], col);
        }
        Glyph::Plus => {
            line((0.0, -0.34), (0.0, 0.34), 0.2, col);
            line((-0.34, 0.0), (0.34, 0.0), 0.2, col);
        }
        Glyph::Leg => {
            line((-0.1, -0.4), (0.0, 0.0), 0.14, col);
            line((0.0, 0.0), (0.18, 0.4), 0.14, col);
            line((-0.14, 0.0), (0.16, 0.0), 0.06, Color32::from_rgb(230, 80, 80));
        }
        Glyph::Skull => {
            p.circle_filled(q(0.0, -0.06), 0.3 * s, col);
            p.rect_filled(Rect::from_center_size(q(0.0, 0.26), egui::vec2(0.3 * s, 0.2 * s)), 0.0, col);
            p.circle_filled(q(-0.12, -0.06), 0.07 * s, dark);
            p.circle_filled(q(0.12, -0.06), 0.07 * s, dark);
        }
    }
}

/// A round badge with a glyph in it, the way the stat bars wear their icon.
pub fn badge(p: &Painter, c: Pos2, r: f32, g: Glyph, col: Color32) {
    p.circle_filled(c, r, Color32::from_rgb(22, 18, 14));
    p.circle_stroke(c, r, Stroke::new(1.5, col.gamma_multiply(0.75)));
    glyph(p, c, r * 1.25, g, col);
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// A stat bar with its icon on the left. Empty bars pulse red so trouble is seen, not read.
pub fn stat_bar(ui: &mut Ui, g: Glyph, label: &str, frac: f32, col: Color32, width: f32) -> Response {
    let frac = frac.clamp(0.0, 1.0);
    let h = 22.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, h), Sense::hover());
    let t = ui.input(|i| i.time) as f32;
    let low = frac < 0.25 && matches!(g, Glyph::Heart | Glyph::Food | Glyph::Drop | Glyph::Flame | Glyph::Breath);
    let pulse = if low { 0.5 + 0.5 * (t * 6.0).sin() } else { 0.0 };
    let p = ui.painter();
    let track = Rect::from_min_max(egui::pos2(rect.left() + 12.0, rect.top() + 3.0), egui::pos2(rect.right(), rect.bottom() - 3.0));
    p.rect_filled(track, 6.0, Color32::from_rgb(18, 14, 11));
    let fill_col = if low { mix(col, Color32::from_rgb(235, 60, 50), pulse) } else { col };
    if frac > 0.0 {
        let inner = track.shrink(1.5);
        let fw = (inner.width() * frac).max(inner.height());
        let fr = Rect::from_min_size(inner.min, egui::vec2(fw, inner.height()));
        p.rect_filled(fr, 5.0, fill_col.gamma_multiply(0.82));
        // lit upper half and a darker lower edge give the bar some body
        let top = Rect::from_min_size(fr.min, egui::vec2(fr.width(), fr.height() * 0.5));
        p.rect_filled(top, 5.0, Color32::from_white_alpha(46));
        p.rect_filled(Rect::from_min_max(egui::pos2(fr.left(), fr.bottom() - 2.0), fr.max), 2.0, Color32::from_black_alpha(55));
    }
    for k in 1..4 {
        let x = track.left() + track.width() * k as f32 / 4.0;
        p.line_segment([egui::pos2(x, track.top() + 3.0), egui::pos2(x, track.bottom() - 3.0)], Stroke::new(1.0, Color32::from_black_alpha(70)));
    }
    let edge = if low { mix(Color32::from_rgb(90, 70, 50), Color32::from_rgb(240, 70, 60), pulse) } else { Color32::from_rgb(96, 78, 54) };
    p.rect_stroke(track, 6.0, Stroke::new(1.2, edge), StrokeKind::Inside);
    if !label.is_empty() {
        let at = egui::pos2(track.left() + 20.0, track.center().y);
        p.text(at + egui::vec2(1.0, 1.0), Align2::LEFT_CENTER, label, FontId::proportional(12.5), Color32::from_black_alpha(190));
        p.text(at, Align2::LEFT_CENTER, label, FontId::proportional(12.5), Color32::from_rgb(246, 238, 222));
    }
    badge(p, egui::pos2(rect.left() + 11.0, rect.center().y), 10.5, g, if low { mix(col, Color32::from_rgb(255, 90, 80), pulse) } else { col });
    // the game redraws every frame, so the pulse needs no repaint request of its own
    resp
}

/// A button with a painted icon, an optional key badge, and a selected state.
/// An icon button with no visible text: `name` is what hover and screen readers say.
pub fn icon_only(ui: &mut Ui, g: Glyph, name: &str, key: Option<&str>, selected: bool, enabled: bool, tint: Color32) -> Response {
    let resp = icon_button(ui, g, "", key, selected, enabled, tint);
    let full = match key {
        Some(k) => format!("{name} [{k}]"),
        None => name.to_string(),
    };
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &full));
    resp.on_hover_text(name)
}

pub fn icon_button(ui: &mut Ui, g: Glyph, text: &str, key: Option<&str>, selected: bool, enabled: bool, tint: Color32) -> Response {
    let font = FontId::proportional(15.0);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, Color32::WHITE);
    let key_galley = key.map(|k| ui.painter().layout_no_wrap(k.to_string(), FontId::proportional(11.5), Color32::WHITE));
    let key_w = key_galley.as_ref().map_or(0.0, |k| k.size().x + 12.0);
    let w = 12.0 + 20.0 + 6.0 + galley.size().x + if key.is_some() { 8.0 + key_w } else { 0.0 } + 12.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 36.0), if enabled { Sense::click() } else { Sense::hover() });
    // painted, so tell accessibility (and the UI tests) what this button says
    let name = match key {
        Some(k) => format!("{text} [{k}]"),
        None => text.to_string(),
    };
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &name));
    let hovered = resp.hovered() && enabled;
    let held = resp.is_pointer_button_down_on() && enabled;
    let p = ui.painter();
    let fill = if selected {
        Color32::from_rgb(138, 102, 54)
    } else if held {
        Color32::from_rgb(120, 92, 56)
    } else if hovered {
        Color32::from_rgb(92, 72, 50)
    } else {
        Color32::from_rgb(56, 45, 35)
    };
    let alpha = if enabled { 1.0 } else { 0.4 };
    let r = rect.shrink2(egui::vec2(0.0, 1.0));
    p.rect_filled(r, 9.0, fill.gamma_multiply(alpha));
    // a bright top lip and a dark base: the button reads as a raised key
    p.rect_filled(Rect::from_min_size(r.min + egui::vec2(3.0, 1.0), egui::vec2(r.width() - 6.0, 2.0)), 1.0, Color32::from_white_alpha(if selected { 70 } else { 34 }));
    let stroke = if selected { Color32::from_rgb(244, 208, 120) } else if hovered { Color32::from_rgb(170, 138, 90) } else { Color32::from_rgb(98, 80, 56) };
    p.rect_stroke(r, 9.0, Stroke::new(if selected { 1.8 } else { 1.2 }, stroke.gamma_multiply(alpha)), StrokeKind::Inside);
    let icon_c = egui::pos2(r.left() + 12.0 + 10.0, r.center().y + if held { 1.0 } else { 0.0 });
    glyph(p, icon_c, 19.0, g, tint.gamma_multiply(alpha));
    let tx = icon_c.x + 16.0;
    p.galley(egui::pos2(tx, r.center().y - galley.size().y / 2.0), galley.clone(), Color32::from_rgb(244, 234, 214).gamma_multiply(alpha));
    if let Some(kg) = key_galley {
        let kr = Rect::from_min_size(egui::pos2(tx + galley.size().x + 8.0, r.center().y - 9.0), egui::vec2(kg.size().x + 10.0, 18.0));
        p.rect_filled(kr, 5.0, Color32::from_black_alpha(120));
        p.rect_stroke(kr, 5.0, Stroke::new(1.0, Color32::from_rgb(120, 100, 72).gamma_multiply(alpha)), StrokeKind::Inside);
        p.galley(kr.center() - kg.size() / 2.0, kg, Color32::from_rgb(214, 198, 168).gamma_multiply(alpha));
    }
    resp
}

/// A small rounded tag, for discoveries and states.
pub fn chip(ui: &mut Ui, text: &str, fill: Color32, ink: Color32) -> Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(12.0), ink);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + egui::vec2(14.0, 6.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 8.0, fill);
    p.rect_stroke(rect, 8.0, Stroke::new(1.0, ink.gamma_multiply(0.45)), StrokeKind::Inside);
    p.galley(rect.center() - galley.size() / 2.0, galley, ink);
    resp
}

/// A row of dots, `on` of `total` lit, for steps like the stages of evolution.
pub fn pips(ui: &mut Ui, on: usize, total: usize, col: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(total as f32 * 16.0, 14.0), Sense::hover());
    let p = ui.painter();
    for i in 0..total {
        let c = egui::pos2(rect.left() + 8.0 + i as f32 * 16.0, rect.center().y);
        if i < on {
            p.circle_filled(c, 5.0, col);
            p.circle_filled(c - egui::vec2(1.5, 1.5), 1.6, Color32::from_white_alpha(110));
        } else {
            p.circle_filled(c, 5.0, Color32::from_rgb(24, 20, 16));
        }
        p.circle_stroke(c, 5.0, Stroke::new(1.0, if i < on { col.gamma_multiply(0.6) } else { Color32::from_rgb(86, 72, 54) }));
        if i + 1 < total {
            p.line_segment([c + egui::vec2(6.0, 0.0), c + egui::vec2(10.0, 0.0)], Stroke::new(1.0, Color32::from_rgb(86, 72, 54)));
        }
    }
    resp
}

/// A section heading with a glyph and a hairline, to break long panels into parts.
pub fn heading(ui: &mut Ui, g: Glyph, text: &str, col: Color32) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), Sense::hover());
        badge(ui.painter(), rect.center(), 10.0, g, col);
        ui.label(egui::RichText::new(text).size(17.0).strong());
    });
}
