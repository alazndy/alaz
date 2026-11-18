//! İnşa on top: a tile per known building, grouped by purpose; the picked tile's cost and notes on the right.

use alaz_render::egui::{self, Color32, RichText, Stroke, Ui};
use alaz_sim::buildings::BuildingKind;
use alaz_sim::rules::BuildingDef;
use alaz_sim::world::Inventory;
use alaz_sim::{rules, Discovery};

use super::Cx;
use crate::hud::{self, UiAction};
use crate::keys::Action;
use crate::ui_art::Glyph;

const TILE: f32 = 72.0;
const GAP: f32 = 6.0;

/// Whether the bag has the materials and the carried tools to place this building now.
fn ready(b: &BuildingDef, inv: &Inventory) -> bool {
    b.cost_for(|c| inv.has(c)).is_some() && hud::has_needs(&b.needs, inv)
}

fn group_glyph(group: usize) -> Glyph {
    match group {
        0 => Glyph::Flame,
        1 => Glyph::Hammer,
        2 => Glyph::Food,
        3 => Glyph::Drop,
        4 => Glyph::Route,
        5 => Glyph::Plus,
        _ => Glyph::House,
    }
}

fn tile(ui: &mut Ui, b: &BuildingDef, group: usize, ok: bool, picked: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(TILE, TILE), egui::Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, Color32::from_rgb(38, 30, 22));
    crate::ui_art::glyph(p, rect.center_top() + egui::vec2(0.0, 24.0), 30.0, group_glyph(group), Color32::from_rgb(214, 190, 150));
    let text = p.layout(b.name.clone(), egui::FontId::proportional(12.0), Color32::from_rgb(235, 225, 205), TILE - 6.0);
    p.galley(egui::pos2(rect.center().x - text.size().x / 2.0, rect.bottom() - text.size().y - 3.0), text, Color32::WHITE);
    if !ok {
        p.rect_filled(rect, 0.0, Color32::from_black_alpha(120));
    }
    let (col, w) = if picked {
        (Color32::from_rgb(214, 160, 70), 2.0)
    } else if ok {
        (Color32::from_rgb(110, 190, 110), 1.0)
    } else {
        (Color32::from_rgb(70, 56, 40), 1.0)
    };
    p.rect_stroke(crate::pxui::snap_rect(rect), 0.0, Stroke::new(crate::pxui::unit() * w, col), egui::StrokeKind::Inside);
    let name = b.name.clone();
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
    resp
}

fn details(ui: &mut Ui, cx: &mut Cx, b: &BuildingDef) {
    let (w, inv) = (cx.w, &cx.me.inventory);
    ui.label(RichText::new(b.name.as_str()).strong());
    hud::need_row(ui, w, &b.cost, inv);
    if !b.alt_cost.is_empty() {
        ui.label(RichText::new("ya da").small().weak());
        hud::need_row(ui, w, &b.alt_cost, inv);
    }
    if !b.alt_cost2.is_empty() {
        ui.label(RichText::new("ya da").small().weak());
        hud::need_row(ui, w, &b.alt_cost2, inv);
    }
    hud::needs_row(ui, w, &b.needs, inv);
    if let Some(note) = hud::machine_note(b.kind) {
        ui.label(RichText::new(note).small().weak());
    }
    if b.heavy() && w.knows(Discovery::Hoisting) {
        ui.label(RichText::new("Büyük yapı: vinç yakınında dörtte bir ucuz.").small().weak());
    }
    if let Some(req) = b.kind.requirement() {
        if !matches!(req, alaz_sim::buildings::Requirement::Blast) {
            ui.label(RichText::new(hud::requirement_note(req)).small().color(Color32::from_rgb(220, 180, 110)));
        }
    }
    let on = cx.ui_ctx.build_mode == Some(b.kind);
    if ui.add_enabled(ready(b, inv), egui::Button::new("Yerleştir").selected(on).min_size(egui::vec2(80.0, 28.0))).clicked() {
        cx.acts.push(UiAction::BeginBuild(b.kind));
    }
}

pub(crate) fn draw(ui: &mut Ui, cx: &mut Cx) {
    let defs = rules();
    let known: Vec<&BuildingDef> = defs.buildings.iter().filter(|b| cx.w.knows(b.discovery)).collect();
    if known.is_empty() {
        ui.label("Henüz bir şey inşa etmeyi bilmiyorsun.");
        return;
    }
    // the pick lives in egui memory so it survives frames without a field on the window
    let pick_id = egui::Id::NULL.with("build_pick");
    let picked: Option<BuildingKind> = ui.memory(|m| m.data.get_temp::<BuildingKind>(pick_id)).filter(|k| known.iter().any(|b| b.kind == *k));
    let mut new_pick = picked;
    let left_w = ((ui.available_width() - 12.0) * 0.6).max(TILE);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(left_w);
            let cols = (((left_w + GAP) / (TILE + GAP)).floor() as usize).max(1);
            for (gi, name) in hud::BUILD_GROUPS.iter().enumerate() {
                let group: Vec<&&BuildingDef> = known.iter().filter(|b| hud::build_group(b.kind) == gi).collect();
                if group.is_empty() {
                    continue;
                }
                let n_ready = group.iter().filter(|b| ready(b, &cx.me.inventory)).count();
                ui.label(RichText::new(format!("{name} ({n_ready} / {} hazır)", group.len())).strong());
                egui::Grid::new(("build_tiles", gi)).spacing([GAP, GAP]).show(ui, |ui| {
                    for (n, b) in group.into_iter().enumerate() {
                        if tile(ui, b, gi, ready(b, &cx.me.inventory), picked == Some(b.kind)).clicked() {
                            new_pick = Some(b.kind);
                        }
                        if (n + 1) % cols == 0 {
                            ui.end_row();
                        }
                    }
                });
                ui.add_space(4.0);
            }
        });
        ui.separator();
        ui.vertical(|ui| match picked.map(|k| defs.building(k)) {
            Some(b) => details(ui, cx, b),
            None => {
                ui.label(RichText::new("Bir yapı seç: malzemesi burada görünür.").small().weak());
            }
        });
    });
    if new_pick != picked {
        ui.memory_mut(|m| match new_pick {
            Some(k) => {
                m.data.insert_temp(pick_id, k);
            }
            None => {
                m.data.remove::<BuildingKind>(pick_id);
            }
        });
    }
    ui.add_space(4.0);
    let snap = if cx.ui_ctx.grid_snap { "açık" } else { "kapalı" };
    if ui.small_button(format!("Izgaraya oturt: {snap} [{}]", cx.ui_ctx.binds.short(Action::Grid))).clicked() {
        cx.acts.push(UiAction::ToggleGrid);
    }
    if let Some(kind) = cx.ui_ctx.build_mode {
        ui.colored_label(Color32::from_rgb(140, 220, 140), format!("{}: sol tık yerleştirir, {} döndürür, sağ tık iptal eder", defs.building(kind).name, cx.ui_ctx.binds.short(Action::Routes)));
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{harness, world_with};
    use crate::hud::{Tab, UiAction};
    use alaz_sim::buildings::BuildingKind;
    use alaz_sim::{Discovery, ItemKind};
    use egui_kittest::kittest::Queryable;

    #[test]
    fn build_tab_shows_a_tile_per_known_building() {
        let mut w = world_with(&[(ItemKind::Wood, 10)]);
        w.learn(Discovery::Fire);
        w.learn(Discovery::Shelter);
        let h = harness(w, Some(Tab::Build), None);
        assert_eq!(h.query_all_by_label("Kamp ateşi").count(), 1, "fire tile");
        assert_eq!(h.query_all_by_label("Barınak").count(), 1, "shelter tile");
    }

    #[test]
    fn placing_from_the_build_tab_begins_build_mode() {
        let mut w = world_with(&[(ItemKind::Wood, 10)]);
        w.learn(Discovery::Shelter);
        let mut h = harness(w, Some(Tab::Build), None);
        assert!(h.query_by_label("Yerleştir").is_none(), "no details before a tile is picked");
        h.get_by_label("Barınak").click();
        h.run();
        h.get_by_label("Yerleştir").click();
        h.run();
        assert!(h.state().acts.iter().any(|a| matches!(a, UiAction::BeginBuild(BuildingKind::Shelter))), "Yerleştir begins build mode");
    }
}
