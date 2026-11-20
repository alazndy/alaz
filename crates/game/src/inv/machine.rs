//! The open machine on top: its drawing in the middle with the slots on it (input at the mouth, fuel below, output
//! beside), the station's recipes as a strip of pictures above it, and the info panel on the right with what the
//! pointer is on (or the picked recipe) and the actions as icon buttons. Storage and fields show their drawing
//! beside one grid. Items go in from the bag by click or drag and come out into the bag the same way.

use alaz_render::egui::{self, Pos2, Rect, Sense, Stroke, Ui, UiBuilder, Vec2};
use alaz_sim::world::{FIRE_FUEL_MAX, USE_RADIUS_MM};
use alaz_sim::{Building, BuildingKind, CommandKind, ItemKind, RecipeId, World};

use super::info::{self, Subject};
use super::{Carry, Cx, SlotLook, SLOT};
use crate::hud::UiAction;
use crate::pxui::snap_rect;
use crate::style::icons::{icon, icon_button, Icon, BUTTON};
use crate::style::lines::{draw_lines, Chip, Line};
use crate::style::machines::{draw_machine, MachineState};
use crate::style::{Era, Family};

const FAR_NOTE: &str = "Kullanmak için yanına git (3 m).";
const STORAGE_COLS: usize = 8;
const GAP: f32 = 6.0;
/// Clear of the info frame's rim and lacing.
const INFO_PAD: f32 = 22.0;
/// The main region is never laid out shorter than this, even before the window has stored its regions.
const MIN_MAIN_H: f32 = 300.0;
/// What a kiln or furnace burns: shown in the slot under the fire instead of at the mouth.
const FUELS: [ItemKind; 4] = [ItemKind::Wood, ItemKind::Charcoal, ItemKind::Coal, ItemKind::Coke];
/// Fuel items loaded that make the drawn flame full.
const FULL_FIRE: f32 = 8.0;

pub(crate) fn draw(ui: &mut Ui, cx: &mut Cx, i: usize) {
    let w = cx.w;
    let Some(b) = w.buildings.get(i) else { return };
    let near = near(cx, i);
    let era = crate::style::current_era();
    let fam = crate::style::family(b.kind);
    let main_h = super::regions(ui.ctx()).map_or(MIN_MAIN_H, |r| r.main.height()).max(MIN_MAIN_H);
    let area = Rect::from_min_size(ui.max_rect().min, egui::vec2(ui.available_width(), main_h));
    ui.allocate_rect(area, Sense::hover());
    let kind_grid = is_storage(b.kind) || b.kind == BuildingKind::Field;
    let picked = pick(ui.ctx(), w, i);
    let top = Rect::from_min_size(area.min, egui::vec2(area.width(), BUTTON));
    let mut strip_hover = None;
    ui.scope_builder(UiBuilder::new().max_rect(top).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        if !kind_grid && has_recipes(w, b.kind) {
            strip_hover = strip(ui, cx, i, picked, era, fam);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icon_button(ui, Icon::Move, "Taşı", false, era).on_hover_text("Binayı içindekilerle başka bir yere koy").clicked() {
                cx.acts.push(UiAction::MoveBuilding(i));
            }
            if icon_button(ui, Icon::Turn, "Döndür", false, era).on_hover_text("Çeyrek tur çevir").clicked() {
                cx.acts.push(UiAction::Cmd(CommandKind::Turn { building: i as u32 }));
            }
        });
    });
    let art = Rect::from_min_max(egui::pos2(area.left(), top.bottom() + 2.0 * GAP), area.max);
    let art_hover = ui.rect_contains_pointer(art);
    let st = machine_state(ui.ctx(), w, i, b);
    if is_storage(b.kind) {
        let cols = grid_cols(art.width());
        let grid = cells(&b.output, cols, cols, |_| true);
        beside_grid(ui, cx, i, "storage", &grid, cols, true, near, art, (fam, era, &st));
    } else if b.kind == BuildingKind::Field {
        let grid = cells(&b.input, 4, 4, |_| true);
        beside_grid(ui, cx, i, "field", &grid, 4, false, near, art, (fam, era, &st));
    } else {
        station(ui, cx, i, b, near, art, (fam, era, &st));
    }
    info_panel(ui, cx, i, near, era, picked, strip_hover, art_hover);
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

/// The recipe the strip and the actions work on: the player's pick while it is a known recipe of this station,
/// else the job running now, else the automatic one or the first known.
fn pick(ctx: &egui::Context, w: &World, i: usize) -> Option<usize> {
    let b = w.buildings.get(i)?;
    let ok = |r: usize| info::station_recipes(w, b.kind).contains(&(r, true));
    let running = b.job.as_ref().map(|j| j.recipe as usize);
    info::recipe_pick(ctx, i).filter(|&r| ok(r)).or(running.filter(|&r| ok(r))).or_else(|| info::default_pick(w, i).filter(|&r| ok(r)))
}

/// What the info panel tells about: the carried item, else the item under the pointer, a recipe on the strip, the
/// machine drawing, and with the pointer elsewhere the picked recipe.
fn info_subject(ctx: &egui::Context, w: &World, i: usize, picked: Option<usize>, strip_hover: Option<usize>, art_hover: bool) -> Subject {
    let carried = egui::DragAndDrop::payload::<Carry>(ctx).and_then(|c| match *c {
        Carry::Bag(k) | Carry::Output { item: k, .. } => Some(k),
        Carry::Hand(_) => None,
    });
    if let Some(k) = carried.or_else(|| super::hovered_item(ctx)) {
        return Subject::Item(crate::lang::looks_like(w, k));
    }
    if let Some(r) = strip_hover {
        return Subject::Recipe(r);
    }
    if art_hover {
        return Subject::Machine(i);
    }
    picked.map_or(Subject::Machine(i), Subject::Recipe)
}

/// The drawing's inputs: fuel from the fire, heat from the building's temperature, motion while a job runs, and the
/// red outline while what the machine needs around it is missing.
fn machine_state(ctx: &egui::Context, w: &World, i: usize, b: &Building) -> MachineState {
    let heat_max = w.rules().building(b.kind).heat_max;
    let glow = if heat_max > 0 { (f32::from(b.temp) / f32::from(heat_max)).clamp(0.0, 1.0) } else { 0.0 };
    let fuel = if b.kind.burns() {
        b.fuel as f32 / FIRE_FUEL_MAX as f32
    } else {
        let loaded: u32 = FUELS.iter().map(|&k| u32::from(b.input[k as usize])).sum();
        (loaded as f32 / FULL_FIRE).max(glow)
    };
    MachineState {
        fuel: fuel.clamp(0.0, 1.0),
        glow,
        working: b.job.is_some(),
        blocked: b.kind.requirement().is_some() && !w.requirement_met(i),
        t: ctx.input(|inp| inp.time) as f32,
    }
}

/// One cell per item held that `keep` lets through, then empty cells up to at least `min` and a full last row of
/// `cols`.
fn cells(counts: &[u16; ItemKind::COUNT], min: usize, cols: usize, keep: impl Fn(ItemKind) -> bool) -> Vec<(Option<ItemKind>, u16)> {
    let mut v: Vec<(Option<ItemKind>, u16)> = ItemKind::ALL.iter().filter(|&&k| counts[k as usize] > 0 && keep(k)).map(|&k| (Some(k), counts[k as usize])).collect();
    let want = v.len().max(min).div_ceil(cols.max(1)) * cols.max(1);
    v.resize(want.max(min), (None, 0));
    v
}

fn span(n: usize) -> f32 {
    n as f32 * SLOT + n.saturating_sub(1) as f32 * GAP
}

/// Where a grid of `n` slots in `cols` columns goes: centred on `at`, then moved inside `bounds`.
fn grid_rect(at: Pos2, n: usize, cols: usize, bounds: Rect) -> Rect {
    let cols = cols.max(1);
    let size = egui::vec2(span(cols), span(n.div_ceil(cols).max(1)));
    let r = Rect::from_center_size(at, size);
    let dx = (bounds.left() - r.left()).max(0.0) + (bounds.right() - r.right()).min(0.0);
    let dy = (bounds.top() - r.top()).max(0.0) + (bounds.bottom() - r.bottom()).min(0.0);
    snap_rect(r.translate(egui::vec2(dx, dy)))
}

/// Storage columns that leave the drawing a third of the width.
fn grid_cols(width: f32) -> usize {
    (((width * 0.66 + GAP) / (SLOT + GAP)).floor() as usize).clamp(4, STORAGE_COLS)
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
                let resp = crate::pilot::scoped("machine", || super::slot(ui, cx.w, item, count, look));
                if let (Some(k), true) = (item, gives_back && near) {
                    output_slot(cx, i, k, &resp);
                }
                if (n + 1) % cols == 0 {
                    ui.end_row();
                }
            }
        });
    });
    let zone = ui.interact(zone.response.rect, egui::Id::NULL.with(("machine_zone", id, i)), Sense::hover());
    if wanted && super::hovering(&zone).is_some() {
        let good = crate::style::palette(crate::style::current_era()).good;
        ui.painter().rect_stroke(snap_rect(zone.rect.expand(2.0)), 0.0, Stroke::new(crate::pxui::unit() * 2.0, good), egui::StrokeKind::Outside);
    }
    if let Some(c) = super::dropped_on(&zone) {
        drop_on_input(cx, i, c);
    }
}

/// The input zone placed at `r`.
#[allow(clippy::too_many_arguments)]
fn input_zone_at(ui: &mut Ui, r: Rect, cx: &mut Cx, i: usize, id: &str, cells: &[(Option<ItemKind>, u16)], cols: usize, gives_back: bool, near: bool) {
    ui.scope_builder(UiBuilder::new().max_rect(r), |ui| input_zone(ui, cx, i, id, cells, cols, gives_back, near));
}

/// A filled output or storage slot: click or drag onto the bag takes the stack.
fn output_slot(cx: &mut Cx, i: usize, k: ItemKind, resp: &egui::Response) {
    super::drag_source(resp, Carry::Output { building: i, item: k });
    if resp.clicked() {
        cx.acts.push(UiAction::Cmd(CommandKind::Collect { building: i as u32, item: Some(k) }));
    }
}

/// Storage and fields: the drawing on the left, the one grid on the right.
#[allow(clippy::too_many_arguments)]
fn beside_grid(ui: &mut Ui, cx: &mut Cx, i: usize, id: &str, grid: &[(Option<ItemKind>, u16)], cols: usize, gives_back: bool, near: bool, art: Rect, look: (Family, Era, &MachineState)) {
    let (fam, era, st) = look;
    let gw = span(cols);
    let pic = Rect::from_min_max(art.min, egui::pos2((art.right() - gw - 3.0 * GAP).max(art.left()), art.bottom()));
    draw_machine(ui.painter(), pic, fam, era, st);
    let gh = span(grid.len().div_ceil(cols));
    // level with the drawing while the grid is short; a long one starts at the top and scrolls
    let top = (art.center().y - gh / 2.0).max(art.top());
    let r = Rect::from_min_size(egui::pos2(art.right() - gw, top), egui::vec2(gw, gh));
    input_zone_at(ui, snap_rect(r), cx, i, id, grid, cols, gives_back, near);
}

/// A station: the family drawing with the input slots at its mouth, fuel under the fire, the output beside it
/// and the job's progress as a bar with an arrow.
fn station(ui: &mut Ui, cx: &mut Cx, i: usize, b: &Building, near: bool, art: Rect, look: (Family, Era, &MachineState)) {
    let (fam, era, st) = look;
    let a = draw_machine(ui.painter(), art, fam, era, st);
    // a fire is fed wood by Refuel; a kiln or furnace takes its fuel as an input, shown under the fire
    let fuel_apart = a.fuel.is_some() && !b.kind.burns();
    let is_fuel = |k: ItemKind| fuel_apart && FUELS.contains(&k);
    let ins = cells(&b.input, 2, 2, |k| !is_fuel(k));
    input_zone_at(ui, grid_rect(a.input.center(), ins.len(), 2, art), cx, i, "station", &ins, 2, false, near);
    if let Some(fr) = a.fuel {
        if b.kind.burns() {
            fire_slot(ui, cx, i, b, near, grid_rect(fr.center(), 1, 1, art), era);
        } else {
            let fuel = cells(&b.input, 1, 2, is_fuel);
            let cols = fuel.len().min(2);
            input_zone_at(ui, grid_rect(fr.center(), fuel.len(), cols, art), cx, i, "fuel", &fuel, cols, false, near);
        }
    } else if b.kind.burns() {
        let at = egui::pos2(art.center().x, art.bottom() - SLOT / 2.0);
        fire_slot(ui, cx, i, b, near, grid_rect(at, 1, 1, art), era);
    }
    let out = cells(&b.output, 2, 2, |_| true);
    let out_r = grid_rect(a.output.center(), out.len(), 2, art);
    ui.scope_builder(UiBuilder::new().max_rect(out_r), |ui| {
        egui::Grid::new(("machine_out", i)).spacing([GAP, GAP]).show(ui, |ui| {
            for (n, &(item, count)) in out.iter().enumerate() {
                let look = match item {
                    Some(_) if near => SlotLook::Normal,
                    Some(_) => SlotLook::Disabled,
                    None => SlotLook::Empty,
                };
                let resp = crate::pilot::scoped("machine", || super::slot(ui, cx.w, item, count, look));
                if let (Some(k), true) = (item, near) {
                    output_slot(cx, i, k, &resp);
                }
                if (n + 1) % 2 == 0 {
                    ui.end_row();
                }
            }
        });
    });
    let defs = cx.w.rules();
    let done = b.job.as_ref().and_then(|j| defs.recipes.get(j.recipe as usize).map(|r| 1.0 - j.remaining as f32 / r.ticks.max(1) as f32));
    progress_bar(ui.painter(), a.progress, done, era, crate::style::accent(fam));
}

/// The job's progress: a trough filled in the family's accent, ending in an arrow that lights while a job runs.
fn progress_bar(p: &egui::Painter, r: Rect, done: Option<f32>, era: Era, acc: egui::Color32) {
    let pal = crate::style::palette(era);
    let u = crate::pxui::unit();
    let arrow = Rect::from_min_size(egui::pos2(r.right() - 28.0, r.center().y - 14.0), Vec2::splat(28.0));
    let trough = snap_rect(Rect::from_min_max(egui::pos2(r.left(), r.center().y - 7.0), egui::pos2(arrow.left() - 4.0, r.center().y + 7.0)));
    p.rect_filled(trough, 0.0, crate::style::mix(pal.base_dark, pal.edge, 0.5));
    let inner = trough.shrink(u);
    p.rect_filled(snap_rect(inner), 0.0, pal.slot_shadow);
    if let Some(f) = done {
        let fill = Rect::from_min_size(inner.min, egui::vec2(inner.width() * f.clamp(0.0, 1.0), inner.height()));
        p.rect_filled(snap_rect(fill), 0.0, acc);
    }
    icon(p, arrow, Icon::Arrow, if done.is_some() { acc } else { pal.ink_dim });
}

/// The fire's wood slot, with how long it burns as a flame bar beside it. Its tooltip says why it takes no wood.
fn fire_slot(ui: &mut Ui, cx: &mut Cx, i: usize, b: &Building, near: bool, r: Rect, era: Era) {
    let ok = near && can_refuel(cx, i);
    let carried = egui::DragAndDrop::payload::<Carry>(ui.ctx()).map(|c| *c);
    let look = match carried {
        _ if !ok => SlotLook::Disabled,
        Some(Carry::Bag(ItemKind::Wood)) => SlotLook::Target,
        _ => SlotLook::Normal,
    };
    let why = if !near {
        FAR_NOTE
    } else if cx.me.inventory.get(ItemKind::Wood) == 0 {
        "Çantada odun yok."
    } else if !ok {
        "Ateş sönmüş: yeniden yakmak için çakmaktaşı gerek."
    } else {
        "Odunu buraya bırak ya da tıkla: ateş alabildiği kadar odun alır."
    };
    let resp = ui.scope_builder(UiBuilder::new().max_rect(r), |ui| super::slot(ui, cx.w, Some(ItemKind::Wood), 0, look)).inner.on_hover_text(why);
    if resp.clicked() {
        cx.acts.push(UiAction::Cmd(CommandKind::Refuel { building: i as u32 }));
    }
    if let Some(c) = super::dropped_on(&resp) {
        drop_on_fuel(cx, i, c);
    }
    let pal = crate::style::palette(era);
    let p = ui.painter();
    let flame = Rect::from_min_size(egui::pos2(r.right() + GAP, r.center().y - 12.0), Vec2::splat(24.0));
    icon(p, flame, Icon::Flame, egui::Color32::from_rgb(255, 140, 40));
    let bar = snap_rect(Rect::from_min_size(egui::pos2(flame.right() + 4.0, r.center().y - 6.0), egui::vec2(72.0, 12.0)));
    p.rect_filled(bar, 0.0, pal.slot_shadow);
    let frac = (b.fuel as f32 / FIRE_FUEL_MAX as f32).clamp(0.0, 1.0);
    p.rect_filled(snap_rect(Rect::from_min_size(bar.min, egui::vec2(bar.width() * frac, bar.height()))), 0.0, egui::Color32::from_rgb(232, 118, 44));
}

/// The station's recipes as pictures: a known one shows its first product, an unknown one a "?" that does
/// nothing. A click picks the recipe; the picked one is framed in the family's accent. Returns the known recipe
/// under the pointer.
fn strip(ui: &mut Ui, cx: &mut Cx, i: usize, picked: Option<usize>, era: Era, fam: Family) -> Option<usize> {
    let w = cx.w;
    let kind = w.buildings[i].kind;
    let mut hover = None;
    for (ri, known) in info::station_recipes(w, kind) {
        let resp = recipe_button(ui, w, ri, known, picked == Some(ri), era, crate::style::accent(fam));
        if !known {
            continue;
        }
        if resp.hovered() {
            hover = Some(ri);
        }
        if resp.clicked() {
            info::set_recipe_pick(ui.ctx(), i, ri);
        }
    }
    hover
}

fn recipe_button(ui: &mut Ui, w: &World, ri: usize, known: bool, picked: bool, era: Era, acc: egui::Color32) -> egui::Response {
    let r = &w.rules().recipes[ri];
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(BUTTON), if known { Sense::click() } else { Sense::hover() });
    let label = if known { r.name.as_str() } else { "?" };
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, known, label));
    let pal = crate::style::palette(era);
    let p = ui.painter();
    crate::style::slot_frame(p, rect, era, SlotLook::Normal);
    if picked {
        let u = crate::pxui::unit();
        let lit = Rect::from_min_max(rect.min + Vec2::splat(2.0 * u), rect.max - Vec2::splat(2.0 * u));
        p.rect_filled(snap_rect(lit), 0.0, crate::style::mix(pal.slot, acc, 0.28));
        crate::style::highlight(p, rect, acc);
    } else if known && resp.hovered() {
        crate::style::highlight(p, rect, crate::style::mix(pal.light, pal.ink, 0.3));
    }
    let inner = rect.shrink(6.0);
    // knowing a recipe means knowing what it makes, so its product shows even before one was ever seen
    let product = r.outputs.first().map(|c| c.item).or(r.alloy.as_ref().map(|al| al.output)).or(r.inputs.first().map(|c| c.item));
    match product.filter(|_| known) {
        Some(k) => crate::icons::draw_item(p, inner, k),
        None => icon(p, rect.shrink(10.0), if known { Icon::Play } else { Icon::Question }, pal.ink_dim),
    }
    resp.on_hover_text(if known { r.name.clone() } else { "Henüz bilinmiyor".to_string() })
}

/// The info panel: lines for the subject in the era's words (pictures only in the stone age), the warnings that
/// always apply, and the actions as icon buttons along the bottom.
#[allow(clippy::too_many_arguments)]
fn info_panel(ui: &mut Ui, cx: &mut Cx, i: usize, near: bool, era: Era, picked: Option<usize>, strip_hover: Option<usize>, art_hover: bool) {
    let Some(regs) = super::regions(ui.ctx()) else { return };
    let w = cx.w;
    let b = &w.buildings[i];
    let inner = regs.info.shrink(INFO_PAD);
    // the panel lies outside the main region's scroll area, so it gets its own clip
    let mut panel = ui.new_child(UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)));
    panel.set_clip_rect(inner);
    let level = crate::style::era_level(w.perception_level());
    let subject = info_subject(ui.ctx(), w, i, picked, strip_hover, art_hover);
    let mut lines = info::describe(w, subject, level);
    if matches!(subject, Subject::Recipe(_)) && w.rules().building(b.kind).heat_max > 0 {
        for line in &mut lines {
            if let Line::Chips(chips) = line {
                for c in chips {
                    if let Chip::HeatC { now, .. } = c {
                        *now = Some(b.temp);
                    }
                }
            }
        }
    }
    // the machine's own lines already carry its warning
    if !matches!(subject, Subject::Machine(_))
        && let Some(req) = b.kind.requirement().filter(|_| !w.requirement_met(i))
    {
        lines.push(Line::Warn(crate::hud::requirement_missing(req).to_string()));
    }
    if !near {
        lines.push(Line::Warn(FAR_NOTE.to_string()));
    }
    let text = Rect::from_min_max(inner.min, egui::pos2(inner.right(), inner.bottom() - BUTTON - 2.0 * GAP));
    panel.scope_builder(UiBuilder::new().max_rect(text), |ui| {
        egui::ScrollArea::vertical().id_salt(("machine_info", i)).auto_shrink([false, false]).show(ui, |ui| {
            ui.set_width(text.width());
            draw_lines(ui, w, era, &lines);
        });
    });
    let row = Rect::from_min_max(egui::pos2(inner.left(), inner.bottom() - BUTTON), inner.max);
    panel.scope_builder(UiBuilder::new().max_rect(row).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        actions(ui, cx, i, near, era, picked);
    });
}

/// Başlat and Otomatik for the picked recipe, then load-all and take-all.
fn actions(ui: &mut Ui, cx: &mut Cx, i: usize, near: bool, era: Era, picked: Option<usize>) {
    let w = cx.w;
    let b = &w.buildings[i];
    let storage = is_storage(b.kind);
    let station = !storage && b.kind != BuildingKind::Field && has_recipes(w, b.kind);
    if station {
        let inv = &cx.me.inventory;
        // what is loaded counts first and the bag makes up the rest, as the sim reckons it
        let have = |r: usize| w.rules().recipes[r].inputs.iter().all(|c| b.input[c.item as usize].saturating_add(inv.get(c.item)) >= c.n);
        let can_start = near && b.job.is_none() && picked.is_some_and(have);
        if ui.add_enabled_ui(can_start, |ui| icon_button(ui, Icon::Play, "Başlat", false, era)).inner.clicked()
            && let Some(r) = picked
        {
            cx.acts.push(UiAction::Cmd(CommandKind::StartJob { building: i as u32, recipe: RecipeId(r as u16) }));
        }
        let auto_on = picked.is_some() && b.auto.map(usize::from) == picked;
        if ui.add_enabled_ui(picked.is_some(), |ui| icon_button(ui, Icon::Auto, "Otomatik", auto_on, era)).inner.clicked()
            && let Some(r) = picked
        {
            cx.acts.push(UiAction::Cmd(CommandKind::SetAuto { building: i as u32, recipe: if auto_on { None } else { Some(RecipeId(r as u16)) } }));
        }
    }
    if storage || station {
        let tip = if storage { "Hepsini yükle (araçlar hariç)" } else { "Girdileri yükle" };
        if ui.add_enabled_ui(near, |ui| icon_button(ui, Icon::Load, tip, false, era)).inner.clicked() {
            cx.acts.push(UiAction::Cmd(CommandKind::Deposit { building: i as u32 }));
        }
    }
    if station && ui.add_enabled_ui(near && b.output_total() > 0, |ui| icon_button(ui, Icon::Take, "Hepsini al", false, era)).inner.clicked() {
        cx.acts.push(UiAction::Cmd(CommandKind::Collect { building: i as u32, item: None }));
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

    /// Node rects come back off by a fraction of a point, so the region is grown by one.
    fn main_region(h: &egui_kittest::Harness<'static, crate::inv::tests::State>) -> egui::Rect {
        crate::inv::regions(&h.ctx).expect("the window was laid out").main.expand(1.0)
    }

    fn info_region(h: &egui_kittest::Harness<'static, crate::inv::tests::State>) -> egui::Rect {
        crate::inv::regions(&h.ctx).expect("the window was laid out").info.expand(1.0)
    }

    /// The furnace recipe after the default one, whose inputs are listed (so a bag can hold them).
    fn second_recipe(w: &World) -> usize {
        let list = crate::inv::info::station_recipes(w, BuildingKind::Furnace);
        assert!(list.len() >= 2, "the furnace has several recipes");
        list.iter().skip(1).map(|&(r, _)| r).find(|&r| !w.rules().recipes[r].inputs.is_empty()).expect("a furnace recipe with inputs")
    }

    #[test]
    fn unknown_recipe_is_a_question_mark() {
        let mut w = world_with(&[(ItemKind::Stone, 1)]);
        let (x, y) = (w.agents[0].x, w.agents[0].y);
        w.buildings.push(Building::new(BuildingKind::Furnace, x + 1_500, y));
        let i = w.buildings.len() - 1;
        let list = crate::inv::info::station_recipes(&w, BuildingKind::Furnace);
        assert!(!list.is_empty() && list.iter().all(|(_, known)| !known), "nothing about the furnace is known yet");
        let names: Vec<String> = list.iter().map(|&(r, _)| w.rules().recipes[r].name.clone()).collect();
        crate::style::force_level(Some(4));
        let mut h = harness(w, None, Some(Selection::Building(i)));
        let main = main_region(&h);
        assert_eq!(h.query_all_by_label("?").filter(|n| main.contains_rect(n.rect())).count(), list.len(), "one '?' per unknown recipe on the strip");
        h.query_all_by_label("?").find(|n| main.contains_rect(n.rect())).expect("a '?' on the strip").click();
        h.run();
        h.run();
        crate::style::force_level(None);
        assert_eq!(crate::inv::info::recipe_pick(&h.ctx, i), None, "an unknown recipe cannot be picked");
        assert!(h.state().acts.is_empty(), "clicking '?' sends nothing");
        for name in &names {
            assert!(h.query_all_by_label_contains(name).next().is_none(), "the window names the unknown recipe {name}");
        }
    }

    #[test]
    fn strip_click_picks_the_recipe() {
        let (w, i) = at(BuildingKind::Furnace, 1_500, &[(ItemKind::Stone, 1)]);
        let target = second_recipe(&w);
        let name = w.rules().recipes[target].name.clone();
        assert_ne!(crate::inv::info::default_pick(&w, i), Some(target));
        let mut h = harness(w, None, Some(Selection::Building(i)));
        let main = main_region(&h);
        h.query_all_by_label(&name).find(|n| main.contains_rect(n.rect())).expect("the recipe is on the strip").click();
        h.run();
        assert_eq!(crate::inv::info::recipe_pick(&h.ctx, i), Some(target));
    }

    #[test]
    fn play_starts_the_picked_recipe() {
        let probe = at(BuildingKind::Furnace, 1_500, &[]).0;
        let target = second_recipe(&probe);
        let items: Vec<(ItemKind, u16)> = probe.rules().recipes[target].inputs.iter().map(|c| (c.item, c.n.saturating_mul(2))).collect();
        let (w, i) = at(BuildingKind::Furnace, 1_500, &items);
        let mut h = harness(w, None, Some(Selection::Building(i)));
        crate::inv::info::set_recipe_pick(&h.ctx, i, target);
        h.run();
        h.get_by_label("Başlat").click();
        h.run();
        assert!(
            h.state().acts.iter().any(|a| matches!(a, UiAction::Cmd(CommandKind::StartJob { building, recipe }) if *building as usize == i && recipe.0 as usize == target)),
            "Başlat starts the picked recipe"
        );
    }

    #[test]
    fn auto_toggles_the_picked_recipe() {
        let (mut w, i) = at(BuildingKind::Furnace, 1_500, &[(ItemKind::Stone, 1)]);
        let target = second_recipe(&w);
        let mut h = harness(w.clone(), None, Some(Selection::Building(i)));
        crate::inv::info::set_recipe_pick(&h.ctx, i, target);
        h.run();
        h.get_by_label("Otomatik").click();
        h.run();
        assert!(
            h.state().acts.iter().any(|a| matches!(a, UiAction::Cmd(CommandKind::SetAuto { building, recipe: Some(r) }) if *building as usize == i && r.0 as usize == target)),
            "Otomatik turns the picked recipe on"
        );
        w.buildings[i].auto = Some(target as u16);
        let mut h = harness(w, None, Some(Selection::Building(i)));
        h.get_by_label("Otomatik").click();
        h.run();
        assert!(
            h.state().acts.iter().any(|a| matches!(a, UiAction::Cmd(CommandKind::SetAuto { building, recipe: None }) if *building as usize == i)),
            "Otomatik turns the running recipe off"
        );
    }

    #[test]
    fn info_shows_carried_item_while_dragging() {
        let (w, i) = at(BuildingKind::Furnace, 1_500, &[(ItemKind::CopperOre, 4)]);
        let name = crate::lang::item_word(&w, ItemKind::CopperOre);
        crate::style::force_level(Some(4));
        let mut h = harness(w, None, Some(Selection::Building(i)));
        let info = info_region(&h);
        let in_info = |h: &egui_kittest::Harness<'static, crate::inv::tests::State>| h.query_all_by_label(&name).any(|n| info.contains_rect(n.rect()));
        assert!(!in_info(&h), "the info panel starts on the recipe, not the ore");
        egui::DragAndDrop::set_payload(&h.ctx, Carry::Bag(ItemKind::CopperOre));
        h.run();
        h.run();
        let shown = in_info(&h);
        crate::style::force_level(None);
        assert!(shown, "the info panel tells about the carried ore");
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
