//! Yap on top: the hands, "Dene" and the notebook. Drawn by the old craft panel until it is rebuilt from slots.

use alaz_render::egui::Ui;
use alaz_sim::ItemKind;

use super::Cx;

pub(crate) fn draw(ui: &mut Ui, cx: &mut Cx) {
    crate::hud::tab_craft(ui, cx.w, cx.me, cx.hands, cx.acts);
}

/// The old panel has its own "Çantadan al" row; the bag grid below adds nothing to it yet.
pub(crate) fn click(_cx: &mut Cx, _k: ItemKind, _shift: bool) {}
