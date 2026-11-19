//! İnşa on top: a tile per known building, grouped by purpose; the picked tile's cost and notes on the right.

use alaz_render::egui::{self, Ui};
use alaz_sim::buildings::BuildingKind;
use alaz_sim::rules::{BuildingDef, ItemCount};
use alaz_sim::world::Inventory;
use alaz_sim::{rules, Discovery};

use super::Cx;
use crate::hud::{self, UiAction};
use crate::keys::Action;
use crate::style::icons::{icon_button, Icon};
use crate::style::lines::{draw_lines, Chip, Line};
use crate::style::machines::{draw_machine, MachineState};
use crate::style::icons::family_icon;
use crate::style::{accent, family, Era, Family};

const TILE: egui::Vec2 = egui::vec2(88.0, 72.0);
const GAP: f32 = 6.0;

/// Whether the bag has the materials and the carried tools to place this building now.
fn ready(b: &BuildingDef, inv: &Inventory) -> bool {
    b.cost_for(|c| inv.has(c)).is_some() && hud::has_needs(&b.needs, inv)
}

fn chips(list: &[ItemCount]) -> Line {
    Line::Chips(list.iter().map(|c| Chip::Item(c.item, c.n)).collect())
}

/// What the info panel says about a building: the cost (and the other ways to pay), what must be carried, notes.
pub(crate) fn info_lines(w: &alaz_sim::World, b: &BuildingDef) -> Vec<Line> {
    let mut lines = vec![Line::Title(b.name.clone()), chips(&b.cost)];
    for alt in [&b.alt_cost, &b.alt_cost2] {
        if !alt.is_empty() {
            lines.push(Line::Label("ya da"));
            lines.push(chips(alt));
        }
    }
    if !b.needs.is_empty() {
        lines.push(Line::Label("Elde olmalı"));
        lines.push(Line::Chips(b.needs.iter().map(|&k| Chip::Item(k, 1)).collect()));
    }
    if let Some(note) = hud::machine_note(b.kind) {
        lines.push(Line::Text(note.to_string()));
    }
    if b.heavy() && w.knows(Discovery::Hoisting) {
        lines.push(Line::Text("Büyük yapı: vinç yakınında dörtte bir ucuz.".to_string()));
    }
    if let Some(req) = b.kind.requirement().filter(|r| !matches!(r, alaz_sim::buildings::Requirement::Blast)) {
        lines.push(Line::Warn(hud::requirement_note(req).to_string()));
    }
    lines
}

/// A tile is the building's own small drawing; its name is the tooltip and the accessible name, not painted text.
fn tile(ui: &mut Ui, b: &BuildingDef, ok: bool, picked: bool, era: Era) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(TILE, egui::Sense::click());
    let p = ui.painter();
    let pal = crate::style::palette(era);
    let fam = family(b.kind);
    crate::style::slot_frame(p, rect, era, super::SlotLook::Normal);
    let art = egui::Rect::from_center_size(rect.center(), egui::vec2(70.0, 50.0));
    draw_machine(p, art, fam, era, &MachineState { fuel: 0.0, glow: 0.0, working: false, blocked: false, t: 0.0 });
    if !ok {
        p.rect_filled(crate::pxui::snap_rect(rect), 0.0, egui::Color32::from_black_alpha(80));
    }
    if picked {
        crate::style::highlight(p, rect, accent(fam));
    } else if ok {
        crate::style::highlight(p, rect, pal.good);
    }
    let name = b.name.clone();
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
    resp.on_hover_text(b.name.as_str())
}

fn pick_id() -> egui::Id {
    egui::Id::NULL.with("build_pick")
}

/// `--ui-shot` picks a tile this way to show the info panel.
pub(crate) fn pick_for_shot(ctx: &egui::Context, kind: BuildingKind) {
    ctx.memory_mut(|m| m.data.insert_temp(pick_id(), kind));
}

/// The pick lives in egui memory so it survives frames without a field on the window.
fn picked(ctx: &egui::Context, known: &[&BuildingDef]) -> Option<BuildingKind> {
    ctx.memory(|m| m.data.get_temp::<BuildingKind>(pick_id())).filter(|k| known.iter().any(|b| b.kind == *k))
}

/// The info panel for İnşa: the picked building's lines and the place button.
pub(crate) fn draw_info(ui: &mut Ui, cx: &mut Cx) {
    let defs = rules();
    let era = crate::style::current_era();
    let known: Vec<&BuildingDef> = defs.buildings.iter().filter(|b| cx.w.knows(b.discovery)).collect();
    let Some(kind) = picked(ui.ctx(), &known) else {
        draw_lines(ui, cx.w, era, &[Line::Pictos(vec![crate::style::lines::Picto::Icon(Icon::Question)])]);
        return;
    };
    let b = defs.building(kind);
    draw_lines(ui, cx.w, era, &info_lines(cx.w, b));
    let on = cx.ui_ctx.build_mode == Some(kind);
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let place = ui.add_enabled_ui(ready(b, &cx.me.inventory), |ui| icon_button(ui, Icon::House, "Yerleştir", on, era)).inner;
        if place.clicked() {
            cx.acts.push(UiAction::BeginBuild(kind));
        }
    });
    if on {
        let hint = format!("Sol tık yerleştirir, {} döndürür, sağ tık iptal eder.", cx.ui_ctx.binds.short(Action::Routes));
        draw_lines(ui, cx.w, era, &[Line::Text(hint)]);
    }
}

pub(crate) fn draw(ui: &mut Ui, cx: &mut Cx) {
    let defs = rules();
    let era = crate::style::current_era();
    let known: Vec<&BuildingDef> = defs.buildings.iter().filter(|b| cx.w.knows(b.discovery)).collect();
    if known.is_empty() {
        ui.label("Henüz bir şey inşa etmeyi bilmiyorsun.");
        return;
    }
    let picked = picked(ui.ctx(), &known);
    let mut new_pick = picked;
    let cols = (((ui.available_width() + GAP) / (TILE.x + GAP)).floor() as usize).max(1);
    for gi in 0..hud::BUILD_GROUPS.len() {
        let group: Vec<&&BuildingDef> = known.iter().filter(|b| hud::build_group(b.kind) == gi).collect();
        if group.is_empty() {
            continue;
        }
        let n_ready = group.iter().filter(|b| ready(b, &cx.me.inventory)).count();
        group_head(ui, gi, n_ready, group.len(), era);
        egui::Grid::new(("build_tiles", gi)).spacing([GAP, GAP]).show(ui, |ui| {
            for (n, b) in group.into_iter().enumerate() {
                if tile(ui, b, ready(b, &cx.me.inventory), picked == Some(b.kind), era).clicked() {
                    new_pick = Some(b.kind);
                }
                if (n + 1) % cols == 0 {
                    ui.end_row();
                }
            }
        });
        ui.add_space(6.0);
    }
    if new_pick != picked {
        ui.memory_mut(|m| match new_pick {
            Some(k) => {
                m.data.insert_temp(pick_id(), k);
            }
            None => {
                m.data.remove::<BuildingKind>(pick_id());
            }
        });
    }
    let snap = if cx.ui_ctx.grid_snap { "açık" } else { "kapalı" };
    if ui.button(format!("Izgaraya oturt: {snap} [{}]", cx.ui_ctx.binds.short(Action::Grid))).clicked() {
        cx.acts.push(UiAction::ToggleGrid);
    }
}

fn group_family(group: usize) -> Family {
    match group {
        0 => Family::Furnace,
        1 => Family::Bench,
        2 => Family::Growing,
        3 => Family::Grinding,
        4 => Family::MachineTool,
        5 => Family::Electric,
        _ => Family::Storage,
    }
}

/// A group is a picture and "ready / all", not a sentence; the group's name is the picture's tooltip.
fn group_head(ui: &mut Ui, group: usize, ready: usize, all: usize, era: Era) {
    let pal = crate::style::palette(era);
    ui.horizontal(|ui| {
        let (r, resp) = ui.allocate_exact_size(egui::Vec2::splat(30.0), egui::Sense::hover());
        family_icon(ui.painter(), r, group_family(group));
        resp.on_hover_text(hud::BUILD_GROUPS[group]);
        ui.label(egui::RichText::new(format!("{ready}/{all}")).size(22.0).color(pal.ink_dim));
    });
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
    fn build_tile_uses_family_drawing() {
        use crate::style::lines::{Chip, Line};
        let mut w = world_with(&[(ItemKind::Wood, 10)]);
        w.learn(Discovery::Shelter);
        let mut h = harness(w, Some(Tab::Build), None);
        // the tile is the building's name as accessible label, with a drawing instead of painted text
        h.get_by_label("Barınak").click();
        h.run();
        let def = alaz_sim::rules().building(BuildingKind::Shelter);
        let lines = super::info_lines(&h.state().w, def);
        let cost = lines.iter().find_map(|l| if let Line::Chips(c) = l { Some(c.clone()) } else { None }).expect("a Chips line");
        let want: Vec<Chip> = def.cost.iter().map(|c| Chip::Item(c.item, c.n)).collect();
        assert_eq!(cost, want);
        assert!(matches!(lines.first(), Some(Line::Title(n)) if *n == def.name));
        assert_eq!(crate::style::family(BuildingKind::Shelter), crate::style::Family::Bench);
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
