//! İnşa on top: what can be built. Drawn by the old build panel until it is rebuilt from tiles.

use alaz_render::egui::Ui;

use super::Cx;

pub(crate) fn draw(ui: &mut Ui, cx: &mut Cx) {
    crate::hud::tab_build(ui, cx.w, cx.me, cx.ui_ctx, cx.acts);
}
