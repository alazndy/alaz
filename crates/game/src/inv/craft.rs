//! Yap on top: four hand slots and "Dene". Items come from the bag grid below by click or drag. What the player
//! knows is in the book.

use alaz_render::egui::{self, Color32, RichText, Stroke, Ui};
use alaz_sim::rules::rules;
use alaz_sim::{CommandKind, ItemKind};

use super::{slot, Carry, Cx, SlotLook};
use crate::hud::UiAction;

const EMPTY: u8 = 255;

fn held(hands: &[u8; 4], k: ItemKind) -> u16 {
    hands.iter().filter(|&&i| i == k as u8).count() as u16
}

fn free_in_bag(cx: &Cx, k: ItemKind) -> u16 {
    cx.me.inventory.get(k).saturating_sub(held(cx.hands, k))
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

#[cfg(test)]
mod tests {
    use super::super::tests::{harness, ui_ctx, world_with};
    use super::*;
    use crate::hud::{Tab, UiAction};
    use crate::keys::Keybinds;
    use alaz_sim::World;
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
        let stone = crate::lang::item_word(&w, ItemKind::Stone);
        let mut h = harness(w, Some(Tab::Craft), None);
        h.get_by_label(&stone).click();
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
