//! Çanta on top: how full the bag is, and the tools with their wear.

use alaz_render::egui::{self, Color32, RichText, Ui};
use alaz_sim::{CommandKind, ItemKind};

use super::Cx;
use crate::hud::{item_name, UiAction};

pub(crate) fn draw(ui: &mut Ui, cx: &mut Cx) {
    let (w, me) = (cx.w, cx.me);
    let inv = &me.inventory;
    crate::ui_art::stat_bar(
        ui,
        crate::ui_art::Glyph::Bag,
        &format!("{}/{} yer dolu (aletler sayılmaz)", inv.load(), me.capacity()),
        inv.load() as f32 / me.capacity().max(1) as f32,
        Color32::from_rgb(150, 120, 70),
        200.0,
    );
    // tools wear with use; a blunt one still works, three times slower, until it is mended
    let worn: Vec<_> = w.rules().tools.iter().filter(|t| t.durability > 0 && inv.get(t.item) > 0).collect();
    if !worn.is_empty() {
        ui.add_space(6.0);
        ui.label(RichText::new("Aletler").strong());
        for t in worn {
            let life = alaz_sim::world::tool_life(t, &w.settings).max(1);
            let used = me.wear_of(t.item);
            let left = life.saturating_sub(used);
            ui.horizontal(|ui| {
                let label = if left == 0 { format!("{}: kör", item_name(t.item)) } else { format!("{}: {left}/{life}", item_name(t.item)) };
                let col = if left == 0 { Color32::from_rgb(220, 90, 70) } else { Color32::from_rgb(140, 190, 140) };
                crate::ui_art::stat_bar(ui, crate::ui_art::Glyph::Hammer, &label, left as f32 / life as f32, col, 220.0);
                if let Some(mat) = t.repair.filter(|_| used > 0) {
                    let ok = inv.get(mat) > 0;
                    if ui.add_enabled(ok, egui::Button::new(format!("Onar (1 {})", item_name(mat).to_lowercase()))).clicked() {
                        cx.acts.push(UiAction::Cmd(CommandKind::Repair { item: t.item }));
                    }
                }
            });
        }
    }
    ui.add_space(4.0);
    ui.label(RichText::new("Bırakmak için eşyaya sağ tıkla ya da pencerenin dışına sürükle. Yerdekini toplamak için üstüne sağ tıkla.").small().weak());
}

/// With only the bag open a click just points at the item (its tooltip says what it is); nothing is sent, so a
/// stray click can never drop or use anything.
pub(crate) fn click(_cx: &mut Cx, _k: ItemKind, _shift: bool) {}
