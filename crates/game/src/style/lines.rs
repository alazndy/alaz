//! Info lines: what the info panel says (made by `inv::info` per perception level), and how they are drawn here,
//! in the era's look. Pictures first: an item picture, never the item's name, unless the line asks for words.

use alaz_render::egui::{self, Color32, FontId, RichText, Sense, Ui, Vec2};
use alaz_sim::{ItemKind, World};

use super::icons::{icon, Icon};
use super::{mix, palette, Era};

/// One picture in a picture row.
#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Picto {
    Item(ItemKind),
    Icon(Icon),
    /// Heat as flames: `lit` of `of` are burning.
    Flames { lit: u8, of: u8 },
    /// Time as hourglasses: `lit` of `of` are full.
    Glasses { lit: u8, of: u8 },
}

/// A small framed tag: picture and a word or a number.
#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Chip {
    Item(ItemKind, u16),
    HeatC { need: i16, now: Option<i16> },
    HeatWord(&'static str),
    Secs(u32),
    PerMin(f32),
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Line {
    Title(String),
    Label(&'static str),
    Pictos(Vec<Picto>),
    Chips(Vec<Chip>),
    Text(String),
    Mono(Vec<String>),
    Warn(String),
}

const BODY: f32 = 22.0;
const PICTO: f32 = 40.0;
const CHIP_ICON: f32 = 24.0;

/// Draws `lines` top to bottom into `ui`, in the era's look.
pub(crate) fn draw_lines(ui: &mut Ui, w: &World, era: Era, lines: &[Line]) {
    let pal = palette(era);
    ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
    for line in lines {
        match line {
            Line::Title(s) => {
                ui.add(egui::Label::new(RichText::new(s).size(26.0).strong().color(pal.ink)).wrap());
            }
            Line::Label(s) => {
                ui.add_space(2.0);
                ui.label(RichText::new(*s).size(BODY).color(pal.ink_dim));
            }
            Line::Pictos(ps) => {
                ui.horizontal_wrapped(|ui| {
                    for p in ps {
                        picto(ui, w, era, p);
                    }
                });
            }
            Line::Chips(cs) => {
                ui.horizontal_wrapped(|ui| {
                    for c in cs {
                        chip(ui, w, era, c);
                    }
                });
            }
            Line::Text(s) => {
                ui.add(egui::Label::new(RichText::new(s).size(BODY).color(pal.ink)).wrap());
            }
            Line::Mono(rows) => {
                let font = mono_font(ui);
                egui::Frame::new().fill(pal.slot_shadow).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for r in rows {
                        ui.add(egui::Label::new(RichText::new(r).font(font.clone()).color(pal.ink)).wrap());
                    }
                });
            }
            Line::Warn(s) => {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(CHIP_ICON), Sense::hover());
                    icon(ui.painter(), r, Icon::Warn, pal.warn);
                    ui.add(egui::Label::new(RichText::new(s).size(BODY).color(pal.warn)).wrap());
                });
            }
        }
    }
}

/// Liberation Mono once the fonts carry it; the first frame after the fonts are set they may not yet.
fn mono_font(ui: &Ui) -> FontId {
    let fam = egui::FontFamily::Name(super::MONO.into());
    let has = ui.ctx().fonts(|f| f.definitions().families.contains_key(&fam));
    FontId::new(BODY, if has { fam } else { egui::FontFamily::Monospace })
}

fn item_picture(p: &egui::Painter, r: egui::Rect, w: &World, k: ItemKind, era: Era) {
    let seen = crate::lang::looks_like(w, k);
    if w.has_seen(k) {
        crate::icons::draw_item(p, r, seen);
    } else {
        icon(p, r.shrink(r.width() * 0.15), Icon::Question, palette(era).ink_dim);
    }
}

fn picto(ui: &mut Ui, w: &World, era: Era, p: &Picto) {
    let pal = palette(era);
    let row = |ui: &mut Ui, ic: Icon, lit: u8, of: u8, on: Color32| {
        for i in 0..of {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
            let col = if i < lit { on } else { mix(pal.slot, pal.ink_dim, 0.35) };
            icon(ui.painter(), r, ic, col);
        }
    };
    match *p {
        Picto::Item(k) => {
            let (r, resp) = ui.allocate_exact_size(Vec2::splat(PICTO), Sense::hover());
            item_picture(ui.painter(), r, w, k, era);
            resp.on_hover_text(crate::lang::item_word(w, crate::lang::looks_like(w, k)));
        }
        Picto::Icon(ic) => {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(PICTO), Sense::hover());
            icon(ui.painter(), r.shrink(4.0), ic, pal.ink);
        }
        Picto::Flames { lit, of } => row(ui, Icon::Flame, lit, of, Color32::from_rgb(255, 140, 40)),
        Picto::Glasses { lit, of } => row(ui, Icon::Hourglass, lit, of, mix(pal.ink, Color32::from_rgb(240, 200, 110), 0.6)),
    }
}

/// "1,5" for 1.5 and "2" for 2.0: Turkish decimal comma, no trailing ",0".
fn per_min(v: f32) -> String {
    let s = format!("{v:.1}");
    let s = s.strip_suffix(".0").map(str::to_string).unwrap_or(s);
    s.replace('.', ",")
}

fn chip(ui: &mut Ui, w: &World, era: Era, c: &Chip) {
    let pal = palette(era);
    let (text, col) = match *c {
        Chip::Item(k, n) => {
            let name = crate::lang::item_word(w, crate::lang::looks_like(w, k));
            let words = matches!(era, Era::Stone | Era::Metal);
            let text = match (words, n > 1) {
                (true, true) => format!("{name} ×{n}"),
                (true, false) => name,
                (false, _) => format!("×{n}"),
            };
            (text, pal.ink)
        }
        Chip::HeatC { need, now } => match now {
            Some(t) => (format!("{t}/{need} °C"), if t >= need { pal.good } else { pal.warn }),
            None => (format!("{need} °C"), pal.ink),
        },
        Chip::HeatWord(s) => (s.to_string(), pal.ink),
        Chip::Secs(s) => (format!("{s} sn"), pal.ink),
        Chip::PerMin(v) => (format!("{}/dk", per_min(v)), pal.ink),
    };
    let galley = ui.painter().layout_no_wrap(text, FontId::proportional(BODY), col);
    let pad = 6.0;
    let size = Vec2::new(pad + CHIP_ICON + 4.0 + galley.size().x + pad, CHIP_ICON.max(galley.size().y) + 6.0);
    let (r, resp) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    super::slot_frame(p, r, era, crate::inv::SlotLook::Normal);
    let ir = egui::Rect::from_min_size(egui::pos2(r.left() + pad, r.center().y - CHIP_ICON / 2.0), Vec2::splat(CHIP_ICON));
    match *c {
        Chip::Item(k, _) => item_picture(p, ir, w, k, era),
        Chip::HeatC { .. } => icon(p, ir, Icon::Thermo, Color32::from_rgb(232, 96, 70)),
        Chip::HeatWord(_) => icon(p, ir, Icon::Flame, Color32::from_rgb(255, 140, 40)),
        Chip::Secs(_) => icon(p, ir, Icon::Clock, pal.ink),
        Chip::PerMin(_) => icon(p, ir, Icon::Arrow, pal.ink),
    }
    p.galley(egui::pos2(ir.right() + 4.0, r.center().y - galley.size().y / 2.0), galley, col);
    if let Chip::Item(k, _) = *c {
        resp.on_hover_text(crate::lang::item_word(w, crate::lang::looks_like(w, k)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_min_uses_a_decimal_comma() {
        assert_eq!(per_min(1.5), "1,5");
        assert_eq!(per_min(2.0), "2");
    }
}
