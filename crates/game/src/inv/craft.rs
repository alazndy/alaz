//! Yap on top: four hand slots, "Dene", and the notebook of what the player knows. Items come from the bag grid
//! below by click or drag.

use alaz_render::egui::{self, Align2, Color32, RichText, Stroke, Ui};
use alaz_sim::rules::rules;
use alaz_sim::world::Inventory;
use alaz_sim::{CommandKind, Discovery, ItemKind, RecipeId};

use super::{slot, Carry, Cx, SlotLook};
use crate::hud::UiAction;

const EMPTY: u8 = 255;

fn held(hands: &[u8; 4], k: ItemKind) -> u16 {
    hands.iter().filter(|&&i| i == k as u8).count() as u16
}

fn free_in_bag(cx: &Cx, k: ItemKind) -> u16 {
    cx.me.inventory.get(k).saturating_sub(held(cx.hands, k))
}

fn needs_met(needs: &[ItemKind], inv: &Inventory) -> bool {
    needs.iter().all(|&k| inv.get(k) > 0)
}

pub(crate) fn draw(ui: &mut Ui, cx: &mut Cx) {
    // the bag can change under the hands (eaten, built with, loaded into a machine): let go of what is no longer there,
    // or "Dene" would try a mix the player cannot see they no longer have
    let mut taken = [0u16; ItemKind::COUNT];
    for h in cx.hands.iter_mut() {
        if let Some(&k) = ItemKind::ALL.get(*h as usize) {
            taken[k as usize] += 1;
            if taken[k as usize] > cx.me.inventory.get(k) {
                *h = EMPTY;
            }
        }
    }
    ui.label(RichText::new("Ellerin").strong());
    ui.horizontal(|ui| {
        egui::Grid::new("inv_hands").spacing([6.0, 6.0]).show(ui, |ui| {
            for i in 0..4 {
                hand_slot(ui, cx, i);
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
        arrow(ui);
        let any = cx.hands.iter().any(|&i| i != EMPTY);
        if ui.add_enabled(any, egui::Button::new("Dene").min_size(egui::vec2(80.0, 60.0))).clicked() {
            cx.acts.push(UiAction::Cmd(CommandKind::Combine { items: *cx.hands }));
            *cx.hands = [EMPTY; 4];
        }
    });
    let mut want = [0u16; ItemKind::COUNT];
    for &i in cx.hands.iter() {
        if let Some(k) = ItemKind::ALL.get(i as usize) {
            want[*k as usize] += 1;
        }
    }
    let known_match = rules().recipes.iter().find(|r| {
        r.station.is_none() && r.ticks == 0 && cx.w.knows(r.discovery) && {
            let mut need = [0u16; ItemKind::COUNT];
            for i in &r.inputs {
                need[i.item as usize] += i.n;
            }
            need == want
        }
    });
    ui.label(
        RichText::new(match known_match {
            Some(r) => format!("Bunu biliyorsun: {}", r.name),
            None if cx.hands.iter().all(|&i| i == EMPTY) => "Çantandan bir şey seç, elinde birleştir.".to_string(),
            None => "Ne olacağını bilmiyorsun. Dene.".to_string(),
        })
        .small()
        .weak(),
    );
    ui.separator();
    notebook(ui, cx);
}

/// Painted, because the pixel font has no arrow glyph.
fn arrow(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(48.0, 60.0), egui::Sense::hover());
    let col = Color32::from_rgb(214, 160, 70);
    let t = crate::pxui::unit() * 3.0;
    let (a, b) = (rect.left_center() + egui::vec2(6.0, 0.0), rect.right_center() - egui::vec2(6.0, 0.0));
    let p = ui.painter();
    p.line_segment([a, b], Stroke::new(t, col));
    p.line_segment([b, b + egui::vec2(-12.0, -10.0)], Stroke::new(t, col));
    p.line_segment([b, b + egui::vec2(-12.0, 10.0)], Stroke::new(t, col));
}

fn hand_slot(ui: &mut Ui, cx: &mut Cx, i: usize) {
    let item = ItemKind::ALL.get(cx.hands[i] as usize).copied();
    let resp = slot(ui, cx.w, item, 1, if item.is_some() { SlotLook::Normal } else { SlotLook::Empty });
    let name = item.map_or_else(|| "boş el".to_string(), |k| crate::lang::item_word(cx.w, crate::lang::looks_like(cx.w, k)));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("El {}: {name}", i + 1)));
    if item.is_some() {
        super::drag_source(&resp, Carry::Hand(i));
        if resp.clicked() {
            cx.hands[i] = EMPTY;
        }
    }
    if let Some(c) = super::dropped_on(&resp) {
        drop_on_hand(cx, i, c);
    }
}

/// A bag click puts one into the first empty hand; with shift, into every empty hand while the bag has enough.
pub(crate) fn click(cx: &mut Cx, k: ItemKind, shift: bool) {
    for i in 0..4 {
        if cx.hands[i] != EMPTY {
            continue;
        }
        if free_in_bag(cx, k) == 0 {
            return;
        }
        cx.hands[i] = k as u8;
        if !shift {
            return;
        }
    }
}

/// A carried item let go over hand `i`. Hands only hold what the bag really has.
pub(crate) fn drop_on_hand(cx: &mut Cx, i: usize, c: Carry) {
    if i >= cx.hands.len() {
        return;
    }
    match c {
        Carry::Bag(k) => {
            if free_in_bag(cx, k) > 0 {
                cx.hands[i] = k as u8;
            }
        }
        Carry::Hand(j) if j < cx.hands.len() => cx.hands.swap(i, j),
        Carry::Hand(_) | Carry::Output { .. } => {}
    }
}

fn notebook(ui: &mut Ui, cx: &mut Cx) {
    let (w, me, defs) = (cx.w, cx.me, rules());
    ui.label(RichText::new("Defter (bildiklerin)").strong());
    let hand: Vec<(usize, &_)> = defs.recipes.iter().enumerate().filter(|(_, r)| r.station.is_none() && w.knows(r.discovery)).collect();
    if hand.is_empty() {
        ui.label("Defterin boş. Elinde bir şeyler dene, yaparak öğrenirsin.");
    }
    // Clues: everything a hand recipe needs has been seen but the recipe is not known yet. No name, no steps:
    // just a nudge to try those things together in the hands.
    // a farming tool only makes sense once there is something to farm: those clues wait until seed or grain has been seen
    let era_ready = |d: Discovery| match d {
        Discovery::Farming => w.has_seen(ItemKind::Seed),
        Discovery::Milling => w.has_seen(ItemKind::Grain),
        _ => true,
    };
    let clues: Vec<&_> = defs.recipes.iter().filter(|r| r.station.is_none() && r.ticks == 0 && !w.knows(r.discovery) && era_ready(r.discovery) && !r.inputs.is_empty() && r.inputs.iter().all(|c| w.has_seen(c.item))).collect();
    for r in clues {
        egui::Frame::new().fill(Color32::from_black_alpha(60)).inner_margin(8).corner_radius(0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                for c in &r.inputs {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
                    crate::icons::draw_slot(ui.painter(), rect, c.item, true);
                }
                let line = "Bunlar bir arada işe yarar gibi. Elinde dene.";
                ui.label(RichText::new(line).small().weak());
            });
        });
    }
    // the book as a wall of cards: what it makes, big; what it takes underneath; click a lit card to make it
    let card_w = 112.0;
    let cols = ((ui.available_width() + 6.0) / (card_w + 6.0)).floor().max(1.0) as usize;
    let mut sorted = hand;
    // what can be made right now first
    sorted.sort_by_key(|(_, r)| !(me.inventory.has(&r.inputs) && needs_met(&r.needs, &me.inventory)));
    egui::Grid::new("craft_cards").spacing([6.0, 6.0]).show(ui, |ui| {
        for (n, (i, r)) in sorted.into_iter().enumerate() {
            let can = me.inventory.has(&r.inputs) && needs_met(&r.needs, &me.inventory);
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(card_w, 124.0), egui::Sense::click());
            let p = ui.painter();
            let hover = resp.hovered() && can;
            p.rect_filled(rect, 0.0, if hover { Color32::from_rgb(58, 46, 30) } else { Color32::from_rgb(34, 27, 20) });
            let edge = if can { Color32::from_rgb(214, 160, 70) } else { Color32::from_rgb(70, 56, 40) };
            p.rect_stroke(crate::pxui::snap_rect(rect), 0.0, Stroke::new(crate::pxui::unit() * if hover { 2.0 } else { 1.0 }, edge), egui::StrokeKind::Inside);
            // the product
            if let Some(o) = r.outputs.first() {
                let icon = egui::Rect::from_center_size(rect.center_top() + egui::vec2(0.0, 28.0), egui::vec2(44.0, 44.0));
                crate::icons::draw_slot(p, icon, o.item, true);
                if o.n > 1 {
                    p.text(icon.right_bottom() - egui::vec2(2.0, 1.0), Align2::RIGHT_BOTTOM, format!("x{}", o.n), egui::FontId::proportional(22.0), Color32::WHITE);
                }
            }
            let name_col = if can { Color32::from_rgb(244, 220, 160) } else { Color32::from_rgb(150, 136, 112) };
            let galley = p.layout(r.name.clone(), egui::FontId::proportional(22.0), name_col, card_w - 10.0);
            p.galley(egui::pos2(rect.center().x - galley.size().x / 2.0, rect.top() + 54.0), galley, name_col);
            // what it takes: small icons, red when short
            let k = r.inputs.len().max(1) as f32;
            let iw = 22.0;
            let mut x = rect.center().x - (k * (iw + 4.0) - 4.0) / 2.0;
            for c in &r.inputs {
                let ir = egui::Rect::from_min_size(egui::pos2(x, rect.bottom() - 28.0), egui::vec2(iw, iw));
                crate::icons::draw_slot(p, ir, c.item, w.has_seen(c.item));
                let short = me.inventory.get(c.item) < c.n;
                p.text(ir.right_bottom() + egui::vec2(1.0, 3.0), Align2::RIGHT_BOTTOM, c.n.to_string(), egui::FontId::proportional(11.0), if short { Color32::from_rgb(240, 90, 70) } else { Color32::WHITE });
                x += iw + 4.0;
            }
            let mut tip = format!("{}\n", r.name);
            for c in &r.inputs {
                tip += &format!("{} {} (sende {})\n", c.n, crate::lang::item_word(w, c.item), me.inventory.get(c.item));
            }
            for k in &r.needs {
                tip += &format!("Gerekir: {}\n", crate::lang::item_word(w, *k));
            }
            if !r.steps.is_empty() {
                let names: Vec<&str> = r.steps.iter().map(|s| s.label.as_str()).collect();
                tip += &format!("Adımlar: {}", names.join(" → "));
            }
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, can, format!("Yap: {}", r.name)));
            if resp.on_hover_text(tip).clicked() && can {
                cx.acts.push(UiAction::Cmd(CommandKind::Craft { recipe: RecipeId(i as u16) }));
            }
            if (n + 1) % cols == 0 {
                ui.end_row();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::super::tests::{harness, ui_ctx, world_with};
    use super::*;
    use crate::hud::{Tab, UiAction};
    use crate::keys::Keybinds;
    use alaz_sim::World;
    use alaz_render::egui;
    use egui_kittest::kittest::Queryable;

    fn with_cx<R>(w: &World, hands: &mut [u8; 4], f: impl FnOnce(&mut Cx) -> R) -> R {
        let binds = Keybinds::default();
        let ui_ctx = ui_ctx(Some(Tab::Craft), None, &binds);
        let mut acts: Vec<UiAction> = Vec::new();
        let mut cx = Cx { w, me: &w.agents[0], ui_ctx: &ui_ctx, hands, acts: &mut acts };
        f(&mut cx)
    }

    #[test]
    fn bag_click_fills_the_first_empty_hand() {
        let w = world_with(&[(ItemKind::Stone, 2), (ItemKind::Branch, 1)]);
        let mut hands = [255, ItemKind::Branch as u8, 255, 255];
        with_cx(&w, &mut hands, |cx| click(cx, ItemKind::Stone, false));
        assert_eq!(hands, [ItemKind::Stone as u8, ItemKind::Branch as u8, 255, 255]);
        // shift fills every empty hand while the bag has enough: one stone is already held, so one is left
        let mut hands = [ItemKind::Stone as u8, 255, 255, 255];
        with_cx(&w, &mut hands, |cx| click(cx, ItemKind::Stone, true));
        assert_eq!(hands, [ItemKind::Stone as u8, ItemKind::Stone as u8, 255, 255]);
        // plain click when the bag is spent does nothing
        let mut hands = [ItemKind::Stone as u8, ItemKind::Stone as u8, 255, 255];
        with_cx(&w, &mut hands, |cx| click(cx, ItemKind::Stone, false));
        assert_eq!(hands, [ItemKind::Stone as u8, ItemKind::Stone as u8, 255, 255]);
    }

    #[test]
    fn clicking_a_bag_slot_puts_it_in_a_hand() {
        let w = world_with(&[(ItemKind::Stone, 2)]);
        let mut h = harness(w, Some(Tab::Craft), None);
        for n in h.query_all_by_role(egui::accesskit::Role::Button) { eprintln!("BTN {:?}", n); }
        h.get_by_label("Taş").click();
        h.run();
        assert_eq!(h.state().knap.hands[0], ItemKind::Stone as u8);
    }

    #[test]
    fn dropping_a_bag_item_on_a_hand_holds_it() {
        let w = world_with(&[(ItemKind::Stone, 1)]);
        let mut hands = [255u8; 4];
        with_cx(&w, &mut hands, |cx| drop_on_hand(cx, 2, Carry::Bag(ItemKind::Stone)));
        assert_eq!(hands, [255, 255, ItemKind::Stone as u8, 255]);
        // the bag has only one stone and it is already held: a second hand cannot hold it
        with_cx(&w, &mut hands, |cx| drop_on_hand(cx, 0, Carry::Bag(ItemKind::Stone)));
        assert_eq!(hands, [255, 255, ItemKind::Stone as u8, 255]);
        // a held item moved to another hand
        with_cx(&w, &mut hands, |cx| drop_on_hand(cx, 3, Carry::Hand(2)));
        assert_eq!(hands, [255, 255, 255, ItemKind::Stone as u8]);
    }
}
