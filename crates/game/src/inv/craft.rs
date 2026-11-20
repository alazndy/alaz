//! Yap on top: four hand slots and "Dene". Items come from the bag grid below by click or drag. What the player
//! knows is in the book.

use alaz_render::egui::{self, Color32, Stroke, Ui};
use alaz_sim::rules::rules;
use alaz_sim::{CommandKind, ItemKind, World};

use super::info::{describe, Subject};
use super::{slot, Carry, Cx, SlotLook};
use crate::hud::UiAction;
use crate::style::icons::{icon_button, Icon};
use crate::style::lines::{draw_lines, Line, Picto};
use crate::style::Family;

const EMPTY: u8 = 255;

fn held(hands: &[u8; 4], k: ItemKind) -> u16 {
    hands.iter().filter(|&&i| i == k as u8).count() as u16
}

fn free_in_bag(cx: &Cx, k: ItemKind) -> u16 {
    cx.me.inventory.get(k).saturating_sub(held(cx.hands, k))
}

/// Lets go of hands whose item has left the bag: the bag can change under the hands (eaten, built with, loaded into
/// a machine), and "Dene" would try a mix the player cannot see they no longer have.
fn release_missing(cx: &mut Cx) {
    let mut taken = [0u16; ItemKind::COUNT];
    for h in cx.hands.iter_mut() {
        if let Some(&k) = ItemKind::ALL.get(*h as usize) {
            taken[k as usize] += 1;
            if taken[k as usize] > cx.me.inventory.get(k) {
                *h = EMPTY;
            }
        }
    }
}

/// Index of the known no-station, instant recipe that the hands would make, if any.
pub(crate) fn known_match(w: &World, hands: &[u8; 4]) -> Option<usize> {
    let mut want = [0u16; ItemKind::COUNT];
    for &i in hands.iter() {
        if let Some(k) = ItemKind::ALL.get(i as usize) {
            want[*k as usize] += 1;
        }
    }
    if want.iter().all(|&n| n == 0) {
        return None;
    }
    rules().recipes.iter().position(|r| {
        r.station.is_none() && r.ticks == 0 && w.knows(r.discovery) && {
            let mut need = [0u16; ItemKind::COUNT];
            for i in &r.inputs {
                need[i.item as usize] += i.n;
            }
            need == want
        }
    })
}

/// What the info panel says: the hovered item, else the recipe the hands would make if it is known, else a "?".
pub(crate) fn info_lines(w: &World, hands: &[u8; 4], hovered: Option<ItemKind>, level: u8) -> Vec<Line> {
    if let Some(k) = hovered {
        return describe(w, Subject::Item(k), level);
    }
    match known_match(w, hands) {
        Some(i) => describe(w, Subject::Recipe(i), level),
        None => vec![Line::Pictos(vec![Picto::Icon(Icon::Question)])],
    }
}

/// The info panel for Yap, drawn by the window into the info region.
pub(crate) fn draw_info(ui: &mut Ui, cx: &Cx) {
    let era = crate::style::current_era();
    let level = crate::style::era_level(cx.w.perception_level());
    draw_lines(ui, cx.w, era, &info_lines(cx.w, cx.hands, super::hovered_item(ui.ctx()), level));
}

pub(crate) fn draw(ui: &mut Ui, cx: &mut Cx) {
    release_missing(cx);
    let era = crate::style::current_era();
    let pal = crate::style::palette(era);
    let any = cx.hands.iter().any(|&i| i != EMPTY);
    // the work surface fills the main region's width; hands, arrow and "Dene" sit in its middle
    let hands_sz = egui::vec2(super::SLOT * 2.0 + 6.0, super::SLOT * 2.0 + 6.0);
    let arrow_sz = egui::vec2(72.0, 60.0);
    let btn = crate::style::icons::BUTTON;
    let gap = 28.0;
    let row_w = hands_sz.x + gap + arrow_sz.x + gap + btn;
    let size = egui::vec2(ui.available_width().min(row_w + 160.0), hands_sz.y + 80.0);
    ui.add_space(((ui.available_height() - size.y) * 0.3).max(8.0));
    let (surface, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let p = ui.painter().clone();
    crate::style::frame(&p, surface, era, Some(crate::style::accent(Family::Bench)));
    let left = surface.center().x - row_w / 2.0;
    let top = surface.center().y - hands_sz.y / 2.0;
    let hands_rect = egui::Rect::from_min_size(egui::pos2(left, top), hands_sz);
    ui.scope_builder(egui::UiBuilder::new().max_rect(hands_rect), |ui| {
        egui::Grid::new("inv_hands").spacing([6.0, 6.0]).show(ui, |ui| {
            for i in 0..4 {
                hand_slot(ui, cx, i);
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
    });
    arrow(&p, egui::Rect::from_center_size(egui::pos2(hands_rect.right() + gap + arrow_sz.x / 2.0, surface.center().y), arrow_sz), pal.light);
    let btn_rect = egui::Rect::from_center_size(egui::pos2(left + row_w - btn / 2.0, surface.center().y), egui::Vec2::splat(btn));
    ui.scope_builder(egui::UiBuilder::new().max_rect(btn_rect), |ui| {
        let go = ui.add_enabled_ui(any, |ui| icon_button(ui, Icon::Play, "Dene", false, era)).inner;
        if go.clicked() {
            cx.acts.push(UiAction::Cmd(CommandKind::Combine { items: *cx.hands }));
            *cx.hands = [EMPTY; 4];
        }
    });
}

/// Painted, because the pixel font has no arrow glyph.
fn arrow(p: &egui::Painter, rect: egui::Rect, col: Color32) {
    let t = crate::pxui::unit() * 4.0;
    let (a, b) = (rect.left_center(), rect.right_center());
    p.line_segment([a, b], Stroke::new(t, col));
    p.line_segment([b, b + egui::vec2(-18.0, -16.0)], Stroke::new(t, col));
    p.line_segment([b, b + egui::vec2(-18.0, 16.0)], Stroke::new(t, col));
}

fn hand_slot(ui: &mut Ui, cx: &mut Cx, i: usize) {
    let item = ItemKind::ALL.get(cx.hands[i] as usize).copied();
    let resp = crate::pilot::scoped("hands", || slot(ui, cx.w, item, 1, if item.is_some() { SlotLook::Normal } else { SlotLook::Empty }));
    let name = item.map_or_else(|| "boş el".to_string(), |k| crate::lang::item_word(cx.w, crate::lang::looks_like(cx.w, k)));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("El {}: {name}", i + 1)));
    crate::pilot::mark(ui.ctx(), || format!("hand:{}", i + 1), resp.rect);
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

    fn hands_for(r: &alaz_sim::rules::RecipeDef) -> Option<[u8; 4]> {
        let mut hands = [255u8; 4];
        let mut n = 0;
        for c in &r.inputs {
            for _ in 0..c.n {
                *hands.get_mut(n)? = c.item as u8;
                n += 1;
            }
        }
        Some(hands)
    }

    #[test]
    fn craft_info_shows_known_match() {
        use crate::inv::info::{describe, Subject};
        use crate::style::lines::{Line, Picto};
        let mut w = world_with(&[]);
        w.unlock_everything();
        let (hands, r) = rules()
            .recipes
            .iter()
            .find_map(|r| (r.station.is_none() && r.ticks == 0 && !r.inputs.is_empty() && w.knows(r.discovery)).then(|| hands_for(r).map(|h| (h, r))).flatten())
            .expect("an instant hand recipe fits in four hands");
        let i = known_match(&w, &hands).expect("the known mix is matched");
        assert_eq!(rules().recipes[i].inputs.len(), r.inputs.len());
        assert_eq!(info_lines(&w, &hands, None, 3), describe(&w, Subject::Recipe(i), 3));
        assert!(!info_lines(&w, &hands, None, 3).is_empty());
        // the hovered item wins over the hands
        assert_eq!(info_lines(&w, &hands, Some(ItemKind::Stone), 3), describe(&w, Subject::Item(ItemKind::Stone), 3));
        // an unknown mix says only "?"
        let fresh = world_with(&[]);
        let mix = [ItemKind::Stone as u8, ItemKind::Stone as u8, ItemKind::Stone as u8, ItemKind::Stone as u8];
        assert_eq!(info_lines(&fresh, &mix, None, 3), vec![Line::Pictos(vec![Picto::Icon(crate::style::icons::Icon::Question)])]);
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
