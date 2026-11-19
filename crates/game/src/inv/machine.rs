//! The open machine on top: stations (input → output, fuel, recipes), storage (one grid), fields (manure in) and
//! fires. Items go in from the bag by click or drag and come out into the bag the same way.

use alaz_render::egui::{self, Color32, RichText, Stroke, Ui};
use alaz_sim::time::TICKS_PER_SECOND;
use alaz_sim::world::{FIRE_FUEL_MAX, USE_RADIUS_MM};
use alaz_sim::{Building, BuildingKind, CommandKind, Discovery, ItemKind, RecipeId, World};

use super::{Carry, Cx, SlotLook};
use crate::hud::UiAction;

const FAR_NOTE: &str = "Kullanmak için yanına git (3 m).";
const STORAGE_COLS: usize = 8;
const STATION_COLS: usize = 4;
const GAP: f32 = 6.0;

pub(crate) fn draw(ui: &mut Ui, cx: &mut Cx, i: usize) {
    let w = cx.w;
    let Some(b) = w.buildings.get(i) else { return };
    let near = near(cx, i);
    ui.horizontal(|ui| {
        if ui.small_button("Döndür").on_hover_text("Çeyrek tur çevir").clicked() {
            cx.acts.push(UiAction::Cmd(CommandKind::Turn { building: i as u32 }));
        }
        if ui.small_button("Taşı").on_hover_text("Binayı içindekilerle başka bir yere koy").clicked() {
            cx.acts.push(UiAction::MoveBuilding(i));
        }
    });
    ui.add_space(2.0);
    if b.kind.burns() {
        fuel(ui, cx, i, b, near);
    }
    if is_storage(b.kind) {
        let grid = cells(&b.output, STORAGE_COLS, STORAGE_COLS);
        input_zone(ui, cx, i, "storage", &grid, STORAGE_COLS, true, near);
        ui.horizontal(|ui| {
            if ui.add_enabled(near, egui::Button::new("Hepsini yükle (araçlar hariç)")).clicked() {
                cx.acts.push(UiAction::Cmd(CommandKind::Deposit { building: i as u32 }));
            }
        });
    } else if b.kind == BuildingKind::Field {
        let grid = cells(&b.input, 2, STATION_COLS);
        input_zone(ui, cx, i, "field", &grid, STATION_COLS, false, near);
    } else if has_recipes(w, b.kind) {
        station(ui, cx, i, b, near);
    }
    if !near {
        ui.label(RichText::new(FAR_NOTE).small().color(Color32::from_rgb(235, 140, 90)));
    }
    ui.add_space(4.0);
    crate::hud::building_notes(ui, w, i, b, cx.acts);
}

/// A bag click: one of the item into the machine, or with Shift all of it. Items the machine does not take, and
/// any item while the player is too far, send nothing.
pub(crate) fn click(cx: &mut Cx, i: usize, k: ItemKind, shift: bool) {
    if takes(cx, i, k) {
        cx.acts.push(UiAction::Cmd(CommandKind::Load { building: i as u32, item: k, n: if shift { u16::MAX } else { 1 } }));
    }
}

/// How a bag slot looks while this machine is open: dimmed (and dead) when the machine would not take it.
pub(crate) fn bag_look(cx: &Cx, i: usize, k: ItemKind) -> SlotLook {
    if takes(cx, i, k) { SlotLook::Normal } else { SlotLook::Disabled }
}

/// A carried item let go over the input (or storage) grid: a bag item goes in whole.
pub(crate) fn drop_on_input(cx: &mut Cx, i: usize, c: Carry) {
    if let Carry::Bag(k) = c
        && takes(cx, i, k)
    {
        cx.acts.push(UiAction::Cmd(CommandKind::Load { building: i as u32, item: k, n: u16::MAX }));
    }
}

/// A carried item let go over the fuel slot: wood feeds the fire (as much as it takes).
pub(crate) fn drop_on_fuel(cx: &mut Cx, i: usize, c: Carry) {
    if c == Carry::Bag(ItemKind::Wood) && can_refuel(cx, i) {
        cx.acts.push(UiAction::Cmd(CommandKind::Refuel { building: i as u32 }));
    }
}

/// Whether selecting this building opens the window on its tab (otherwise the old side panel shows it):
/// stations with recipes, storage, fields and fires.
pub(crate) fn opens_window(w: &World, kind: BuildingKind) -> bool {
    is_storage(kind) || kind == BuildingKind::Field || kind.burns() || has_recipes(w, kind)
}

/// The player is close enough for the sim to accept a Load, Collect, Refuel or StartJob.
pub(crate) fn within_reach(me: &alaz_sim::world::Agent, b: &Building) -> bool {
    (me.x - b.x).pow(2) + (me.y - b.y).pow(2) <= USE_RADIUS_MM * USE_RADIUS_MM
}

fn near(cx: &Cx, i: usize) -> bool {
    cx.w.buildings.get(i).is_some_and(|b| within_reach(cx.me, b))
}

fn is_storage(kind: BuildingKind) -> bool {
    matches!(kind, BuildingKind::Depot | BuildingKind::Cellar | BuildingKind::Cooler)
}

fn has_recipes(w: &World, kind: BuildingKind) -> bool {
    w.rules().recipes.iter().any(|r| r.station == Some(kind))
}

/// The bag has `k` and the machine, in reach, would take some of it now.
fn takes(cx: &Cx, i: usize, k: ItemKind) -> bool {
    let Some(b) = cx.w.buildings.get(i) else { return false };
    within_reach(cx.me, b) && cx.me.inventory.get(k) > 0 && alaz_sim::load_room(cx.w.rules(), b, k) > 0
}

/// The sim's Refuel rule: in reach, wood in the bag, and a dead fire needs flint to be struck again.
fn can_refuel(cx: &Cx, i: usize) -> bool {
    let Some(b) = cx.w.buildings.get(i) else { return false };
    let inv = &cx.me.inventory;
    b.kind.burns() && within_reach(cx.me, b) && inv.get(ItemKind::Wood) > 0 && (b.fuel > 0 || inv.get(ItemKind::Flint) > 0)
}

/// One cell per item held, then empty cells up to at least `min` and a full last row of `cols`.
fn cells(counts: &[u16; ItemKind::COUNT], min: usize, cols: usize) -> Vec<(Option<ItemKind>, u16)> {
    let mut v: Vec<(Option<ItemKind>, u16)> = ItemKind::ALL.iter().filter(|&&k| counts[k as usize] > 0).map(|&k| (Some(k), counts[k as usize])).collect();
    let want = v.len().max(min).div_ceil(cols.max(1)) * cols.max(1);
    v.resize(want.max(min), (None, 0));
    v
}

/// A grid of slots that takes bag items dropped on it. Filled cells of a storage grid hand their stack back to the
/// bag on click or drag.
#[allow(clippy::too_many_arguments)]
fn input_zone(ui: &mut Ui, cx: &mut Cx, i: usize, id: &str, cells: &[(Option<ItemKind>, u16)], cols: usize, gives_back: bool, near: bool) {
    // while a bag item the machine takes is carried, the whole grid shows where it can go
    let carried = egui::DragAndDrop::payload::<Carry>(ui.ctx()).map(|c| *c);
    let wanted = matches!(carried, Some(Carry::Bag(k)) if takes(cx, i, k));
    let zone = ui.scope(|ui| {
        egui::Grid::new(("machine_in", id)).spacing([GAP, GAP]).show(ui, |ui| {
            for (n, &(item, count)) in cells.iter().enumerate() {
                let look = match item {
                    _ if !near => SlotLook::Disabled,
                    _ if wanted => SlotLook::Target,
                    Some(_) if gives_back => SlotLook::Normal,
                    // a station's input stays in the station; the slot only shows what is loaded
                    Some(_) => SlotLook::Empty,
                    None => SlotLook::Empty,
                };
                let resp = super::slot(ui, cx.w, item, count, look);
                if let (Some(k), true) = (item, gives_back && near) {
                    output_slot(cx, i, k, &resp);
                }
                if (n + 1) % cols == 0 {
                    ui.end_row();
                }
            }
        });
    });
    let zone = ui.interact(zone.response.rect, egui::Id::NULL.with(("machine_zone", id, i)), egui::Sense::hover());
    if wanted && super::hovering(&zone).is_some() {
        ui.painter().rect_stroke(crate::pxui::snap_rect(zone.rect.expand(2.0)), 0.0, Stroke::new(crate::pxui::unit() * 2.0, Color32::from_rgb(130, 215, 130)), egui::StrokeKind::Outside);
    }
    if let Some(c) = super::dropped_on(&zone) {
        drop_on_input(cx, i, c);
    }
}

/// A filled output or storage slot: click or drag onto the bag takes the stack.
fn output_slot(cx: &mut Cx, i: usize, k: ItemKind, resp: &egui::Response) {
    super::drag_source(resp, Carry::Output { building: i, item: k });
    if resp.clicked() {
        cx.acts.push(UiAction::Cmd(CommandKind::Collect { building: i as u32, item: Some(k) }));
    }
}

/// The fire's fuel: how long it burns, and a wood slot that feeds it.
fn fuel(ui: &mut Ui, cx: &mut Cx, i: usize, b: &Building, near: bool) {
    let frac = b.fuel as f32 / FIRE_FUEL_MAX as f32;
    let secs = b.fuel as u64 / TICKS_PER_SECOND;
    let ok = near && can_refuel(cx, i);
    ui.horizontal(|ui| {
        let carried = egui::DragAndDrop::payload::<Carry>(ui.ctx()).map(|c| *c);
        let look = match carried {
            _ if !ok => SlotLook::Disabled,
            Some(Carry::Bag(ItemKind::Wood)) => SlotLook::Target,
            _ => SlotLook::Normal,
        };
        let resp = super::slot(ui, cx.w, Some(ItemKind::Wood), 0, look);
        if resp.clicked() {
            cx.acts.push(UiAction::Cmd(CommandKind::Refuel { building: i as u32 }));
        }
        if let Some(c) = super::dropped_on(&resp) {
            drop_on_fuel(cx, i, c);
        }
        ui.vertical(|ui| {
            crate::ui_art::stat_bar(ui, crate::ui_art::Glyph::Flame, &format!("Yakıt {}:{:02}", secs / 60, secs % 60), frac, Color32::from_rgb(232, 118, 44), 270.0);
            let why = if !near {
                FAR_NOTE
            } else if cx.me.inventory.get(ItemKind::Wood) == 0 {
                "Çantada odun yok."
            } else if !ok {
                "Ateş sönmüş: yeniden yakmak için çakmaktaşı gerek."
            } else {
                "Odunu buraya bırak ya da tıkla: ateş alabildiği kadar odun alır."
            };
            ui.label(RichText::new(why).small().weak());
        });
    });
    ui.add_space(4.0);
}

/// Input grid → arrow with the job's progress → output grid, then the recipes to start.
fn station(ui: &mut Ui, cx: &mut Cx, i: usize, b: &Building, near: bool) {
    let w = cx.w;
    let defs = w.rules();
    let job = b.job.as_ref().map(|j| {
        let r = &defs.recipes[j.recipe as usize];
        (r, 1.0 - j.remaining as f32 / r.ticks.max(1) as f32, j.remaining as u64 / TICKS_PER_SECOND)
    });
    ui.horizontal(|ui| {
        let cells_in = cells(&b.input, STATION_COLS, 2);
        input_zone(ui, cx, i, "station", &cells_in, 2, false, near);
        ui.vertical(|ui| {
            ui.add_space(super::SLOT * 0.5);
            let arrow = match job {
                Some((_, done, _)) => format!("→ %{}", (done * 100.0).round() as u32),
                None => "→".to_string(),
            };
            ui.label(RichText::new(arrow).strong().size(22.0).color(Color32::from_rgb(240, 200, 100)));
        });
        let out = cells(&b.output, STATION_COLS, 2);
        egui::Grid::new(("machine_out", i)).spacing([GAP, GAP]).show(ui, |ui| {
            for (n, &(item, count)) in out.iter().enumerate() {
                let look = match item {
                    Some(_) if near => SlotLook::Normal,
                    Some(_) => SlotLook::Disabled,
                    None => SlotLook::Empty,
                };
                let resp = super::slot(ui, w, item, count, look);
                if let (Some(k), true) = (item, near) {
                    output_slot(cx, i, k, &resp);
                }
                if (n + 1) % 2 == 0 {
                    ui.end_row();
                }
            }
        });
    });
    if let Some((r, done, left)) = job {
        crate::ui_art::stat_bar(ui, crate::ui_art::Glyph::Hammer, &format!("{} · {} sn kaldı", r.name, left), done, Color32::from_rgb(120, 190, 120), 270.0);
    }
    ui.horizontal(|ui| {
        if ui.add_enabled(near, egui::Button::new("Girdileri yükle")).on_hover_text("Bu makinenin aldığı her şeyi çantadan yükle").clicked() {
            cx.acts.push(UiAction::Cmd(CommandKind::Deposit { building: i as u32 }));
        }
        if b.output_total() > 0 && ui.add_enabled(near, egui::Button::new("Hepsini al")).clicked() {
            cx.acts.push(UiAction::Cmd(CommandKind::Collect { building: i as u32, item: None }));
        }
    });
    ui.add_space(4.0);
    recipes(ui, cx, i, b, near);
}

/// The station's recipes, each with what it needs, what it makes, "Başlat" and "Otomatik".
fn recipes(ui: &mut Ui, cx: &mut Cx, i: usize, b: &Building, near: bool) {
    let w = cx.w;
    let inv = &cx.me.inventory;
    let defs = w.rules();
    for (ri, r) in defs.recipes.iter().enumerate().filter(|(_, r)| r.station == Some(b.kind) && r.discovery != Discovery::Retired) {
        if !w.knows(r.discovery) {
            ui.add_enabled(false, egui::Button::new(format!("{} (henüz bilinmiyor)", r.name)));
            continue;
        }
        egui::Frame::new()
            .fill(Color32::from_rgba_unmultiplied(0, 0, 0, 60))
            .stroke(Stroke::new(1.0, Color32::from_rgb(70, 56, 40)))
            .corner_radius(0)
            .inner_margin(6)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&r.name).strong());
                    ui.label(RichText::new(format!("{} sn", r.ticks as u64 / TICKS_PER_SECOND)).small().weak());
                });
                crate::hud::need_row(ui, w, &r.inputs, inv);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("->").strong().color(Color32::from_rgb(240, 200, 100)));
                    match r.spawns {
                        Some(k) => {
                            ui.label(crate::hud::kind_name(k));
                        }
                        None => {
                            for it in &r.outputs {
                                let (rect, resp) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
                                crate::icons::draw_slot(ui.painter(), rect, it.item, w.has_seen(it.item));
                                resp.on_hover_text(crate::lang::item_word(w, it.item));
                                ui.label(format!("x{}", it.n));
                            }
                        }
                    }
                });
                ui.horizontal(|ui| {
                    // what is loaded counts first and the bag makes up the rest, as the sim reckons it
                    let have = r.inputs.iter().all(|c| b.input[c.item as usize].saturating_add(inv.get(c.item)) >= c.n);
                    if ui.add_enabled(near && b.job.is_none() && have, egui::Button::new("Başlat")).clicked() {
                        cx.acts.push(UiAction::Cmd(CommandKind::StartJob { building: i as u32, recipe: RecipeId(ri as u16) }));
                    }
                    let auto_on = b.auto == Some(ri as u16);
                    if ui.selectable_label(auto_on, "Otomatik").clicked() {
                        cx.acts.push(UiAction::Cmd(CommandKind::SetAuto {
                            building: i as u32,
                            recipe: if auto_on { None } else { Some(RecipeId(ri as u16)) },
                        }));
                    }
                });
            });
    }
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
        let name = crate::lang::item_word(&w, crate::lang::looks_like(&w, ItemKind::CopperOre));
        let mut h = harness(w, None, Some(Selection::Building(i)));
        h.get_by_label(&name).click();
        h.run();
        assert!(
            h.state().acts.iter().any(|a| matches!(a, UiAction::Cmd(CommandKind::Load { item: ItemKind::CopperOre, n: 1, .. }))),
            "a click on the bag slot loads one"
        );
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
        let name = crate::lang::item_word(&w, ItemKind::Berries);
        let mut h = harness(w.clone(), None, Some(Selection::Building(i)));
        h.get_by_label(&name).click();
        h.run();
        assert!(h.state().acts.is_empty(), "clicking the dimmed berry slot sends nothing");
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
    fn wood_feeds_a_fire_only_when_the_sim_would_take_it() {
        let refuels = |fuel: u32, items: &[(ItemKind, u16)], dx: i64| {
            let (mut w, i) = at(BuildingKind::Campfire, dx, items);
            w.buildings[i].fuel = fuel;
            let binds = Keybinds::default();
            let ui_ctx = ui_ctx(None, Some(Selection::Building(i)), &binds);
            let (mut hands, mut acts) = ([255u8; 4], Vec::new());
            let mut cx = Cx { w: &w, me: &w.agents[0], ui_ctx: &ui_ctx, hands: &mut hands, acts: &mut acts };
            drop_on_fuel(&mut cx, i, Carry::Bag(ItemKind::Wood));
            matches!(acts.as_slice(), [UiAction::Cmd(CommandKind::Refuel { building })] if *building as usize == i)
        };
        assert!(refuels(500, &[(ItemKind::Wood, 2)], 1_500), "a burning fire takes wood");
        assert!(!refuels(0, &[(ItemKind::Wood, 2)], 1_500), "a dead fire needs flint");
        assert!(refuels(0, &[(ItemKind::Wood, 2), (ItemKind::Flint, 1)], 1_500), "flint strikes a dead fire");
        assert!(!refuels(500, &[(ItemKind::Stone, 2)], 1_500), "no wood, nothing to feed");
        assert!(!refuels(500, &[(ItemKind::Wood, 2)], 10_000), "too far to feed it");
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
