//! The open machine on top: stations, storage, fields and fires. Drawn by the old building panel until it is
//! rebuilt from slots.

use alaz_render::egui::Ui;
use alaz_sim::{BuildingKind, CommandKind, ItemKind, World};

use super::{Carry, Cx, SlotLook};
use crate::hud::UiAction;

pub(crate) fn draw(ui: &mut Ui, cx: &mut Cx, i: usize) {
    let Some(b) = cx.w.buildings.get(i) else { return };
    ui.horizontal(|ui| {
        if ui.small_button("Döndür").on_hover_text("Çeyrek tur çevir").clicked() {
            cx.acts.push(UiAction::Cmd(CommandKind::Turn { building: i as u32 }));
        }
        if ui.small_button("Taşı").on_hover_text("Binayı içindekilerle başka bir yere koy").clicked() {
            cx.acts.push(UiAction::MoveBuilding(i));
        }
    });
    ui.add_space(2.0);
    crate::hud::building_body(ui, cx.w, i, b, cx.me, cx.acts);
}

/// The old panel loads with its own buttons; the bag grid below adds nothing to it yet.
pub(crate) fn click(_cx: &mut Cx, _i: usize, _k: ItemKind, _shift: bool) {}

pub(crate) fn bag_look(_cx: &Cx, _i: usize, _k: ItemKind) -> SlotLook { SlotLook::Normal }
pub(crate) fn drop_on_input(_cx: &mut Cx, _i: usize, _c: Carry) {}

/// Whether selecting this building opens the window on its tab (otherwise the old side panel shows it):
/// stations with recipes, storage, fields and fires.
pub(crate) fn opens_window(w: &World, kind: BuildingKind) -> bool {
    matches!(kind, BuildingKind::Depot | BuildingKind::Cellar | BuildingKind::Cooler | BuildingKind::Field)
        || kind.burns()
        || w.rules().recipes.iter().any(|r| r.station == Some(kind))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::{Selection, UiAction};
    use crate::inv::tests::{harness, ui_ctx, world_with};
    use crate::keys::Keybinds;
    use alaz_sim::Building;
    use egui_kittest::kittest::Queryable;

    /// A world whose player stands `dx` mm from a building of `kind`; returns the world and the building's index.
    fn at(kind: BuildingKind, dx: i64, items: &[(ItemKind, u16)]) -> (World, usize) {
        let mut w = world_with(items);
        w.learn_everything();
        let (x, y) = (w.agents[0].x, w.agents[0].y);
        w.buildings.push(Building::new(kind, x + dx, y));
        let i = w.buildings.len() - 1;
        (w, i)
    }

    fn clicked(w: &World, i: usize, k: ItemKind, shift: bool) -> Vec<UiAction> {
        let binds = Keybinds::default();
        let ui_ctx = ui_ctx(None, Some(Selection::Building(i)), &binds);
        let mut hands = [255u8; 4];
        let mut acts = Vec::new();
        let mut cx = Cx { w, me: &w.agents[0], ui_ctx: &ui_ctx, hands: &mut hands, acts: &mut acts };
        click(&mut cx, i, k, shift);
        acts
    }

    #[test]
    fn shift_click_loads_all() {
        let (w, i) = at(BuildingKind::Furnace, 1_500, &[(ItemKind::CopperOre, 4)]);
        let acts = clicked(&w, i, ItemKind::CopperOre, true);
        assert!(
            matches!(acts.as_slice(), [UiAction::Cmd(CommandKind::Load { building, item: ItemKind::CopperOre, n: u16::MAX })] if *building as usize == i),
            "{} actions", acts.len()
        );
    }

    #[test]
    fn plain_click_loads_one() {
        let (w, i) = at(BuildingKind::Furnace, 1_500, &[(ItemKind::CopperOre, 4)]);
        let acts = clicked(&w, i, ItemKind::CopperOre, false);
        assert!(matches!(acts.as_slice(), [UiAction::Cmd(CommandKind::Load { item: ItemKind::CopperOre, n: 1, .. })]), "{} actions", acts.len());
    }

    #[test]
    fn machine_ignores_items_it_does_not_take() {
        let (w, i) = at(BuildingKind::Furnace, 1_500, &[(ItemKind::Berries, 4)]);
        assert!(clicked(&w, i, ItemKind::Berries, false).is_empty());
        assert!(clicked(&w, i, ItemKind::Berries, true).is_empty());
        let binds = Keybinds::default();
        let ui_ctx = ui_ctx(None, Some(Selection::Building(i)), &binds);
        let (mut hands, mut acts) = ([255u8; 4], Vec::new());
        let mut cx = Cx { w: &w, me: &w.agents[0], ui_ctx: &ui_ctx, hands: &mut hands, acts: &mut acts };
        assert_eq!(bag_look(&cx, i, ItemKind::Berries), SlotLook::Disabled, "berries look unusable at a furnace");
        drop_on_input(&mut cx, i, Carry::Bag(ItemKind::Berries));
        assert!(acts.is_empty(), "a dropped berry loads nothing");
    }

    #[test]
    fn clicking_output_collects_it() {
        let (mut w, i) = at(BuildingKind::Furnace, 1_500, &[(ItemKind::Stone, 1)]);
        w.mark_seen(ItemKind::Bronze);
        w.buildings[i].output[ItemKind::Bronze as usize] = 2;
        let name = crate::lang::item_word(&w, ItemKind::Bronze);
        let mut h = harness(w, None, Some(Selection::Building(i)));
        h.get_by_label(&name).click();
        h.run();
        assert!(
            h.state().acts.iter().any(|a| matches!(a, UiAction::Cmd(CommandKind::Collect { building, item: Some(ItemKind::Bronze) }) if *building as usize == i)),
            "no Collect of the bronze"
        );
    }

    #[test]
    fn far_machine_slots_are_disabled() {
        let (w, i) = at(BuildingKind::Furnace, 10_000, &[(ItemKind::CopperOre, 4)]);
        assert!(clicked(&w, i, ItemKind::CopperOre, true).is_empty(), "a far machine takes nothing");
        let h = harness(w, None, Some(Selection::Building(i)));
        assert!(h.query_by_label("Kullanmak için yanına git (3 m).").is_some());
    }

    #[test]
    fn depot_shows_storage_grid() {
        let (mut w, i) = at(BuildingKind::Depot, 1_500, &[(ItemKind::Stone, 1)]);
        w.mark_seen(ItemKind::Berries);
        w.buildings[i].output[ItemKind::Berries as usize] = 3;
        let h = harness(w, None, Some(Selection::Building(i)));
        assert!(h.query_by_label("Yemiş").is_some(), "no berry slot in the depot");
        assert!(h.query_all_by_label_contains("Başlat").next().is_none(), "a depot has no recipes to start");
    }
}
