//! The open machine on top: stations, storage, fields and fires. Drawn by the old building panel until it is
//! rebuilt from slots.

use alaz_render::egui::Ui;
use alaz_sim::{BuildingKind, CommandKind, ItemKind, World};

use super::Cx;
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

/// Whether selecting this building opens the window on its tab (otherwise the old side panel shows it):
/// stations with recipes, storage, fields and fires.
pub(crate) fn opens_window(w: &World, kind: BuildingKind) -> bool {
    matches!(kind, BuildingKind::Depot | BuildingKind::Cellar | BuildingKind::Cooler | BuildingKind::Field)
        || kind.burns()
        || w.rules().recipes.iter().any(|r| r.station == Some(kind))
}
