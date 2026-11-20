//! The inventory window: one centered window in the era's material, a header with icon tabs (Çanta, Yap, İnşa, and
//! the open machine), the chosen tab in the main region, an info panel on the right and the bag grid below.
//! Items move between the parts by egui drag and drop.

use alaz_render::egui::{self, Align2, Color32, Context, Response, Ui, Vec2};
use alaz_sim::world::Agent;
use alaz_sim::{CommandKind, ItemKind, World};

use crate::hud::{self, Selection, Tab, UiAction};

pub(crate) mod bag;
pub(crate) mod build;
pub(crate) mod craft;
// Consumed by the tabs in later tasks.
#[allow(dead_code)]
pub(crate) mod info;
pub(crate) mod machine;

/// Where the window puts its parts this frame.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Regions {
    pub window: egui::Rect,
    pub main: egui::Rect,
    pub info: egui::Rect,
    pub bag: egui::Rect,
}

impl Regions {
    fn shifted(self, dx: f32) -> Regions {
        let by = |r: egui::Rect| r.translate(egui::vec2(dx, 0.0));
        Regions { window: by(self.window), main: by(self.main), info: by(self.info), bag: by(self.bag) }
    }
}

fn regions_id() -> egui::Id {
    egui::Id::NULL.with("inv_regions")
}

/// The regions the window used last.
pub(crate) fn regions(ctx: &Context) -> Option<Regions> {
    ctx.data(|d| d.get_temp(regions_id()))
}

/// The info panel's rect, for the tab drawing into it (empty when the window is not open).
// the tabs that fill the info panel are built on this module after it
#[allow(dead_code)]
pub(crate) fn info_rect(ui: &Ui) -> egui::Rect {
    regions(ui.ctx()).map_or(egui::Rect::NOTHING, |r| r.info)
}

/// The window's size on a screen of `screen` points: up to 1100 x 720, always 40 x 80 smaller than the screen
/// (so at least 900 x 620 whenever the screen has 940 x 700).
pub(crate) fn window_size(screen: Vec2) -> Vec2 {
    egui::vec2(WIN_MAX.x.min(screen.x - 40.0), WIN_MAX.y.min(screen.y - 80.0)).max(egui::vec2(SLOT * 6.0, 320.0))
}

/// Where the parts go on a screen: header on top; the info panel on the right, or under the main region when the
/// window is narrow; the bag at the bottom, three rows high when there is room.
pub(crate) fn layout(screen: egui::Rect) -> Regions {
    let window = crate::pxui::snap_rect(egui::Rect::from_center_size(screen.center(), window_size(screen.size())));
    let inner = window.shrink(PAD);
    let body = egui::Rect::from_min_max(egui::pos2(inner.left(), inner.top() + HEADER_H), inner.max);
    let rows = if body.height() >= 480.0 { 3.0 } else { 2.0 };
    let bag_h = BAG_HEAD + rows * (SLOT + GAP) + 2.0 * WELL_PAD;
    let split = 2.0 * GAP;
    if body.width() >= INFO_W + 500.0 {
        let info = egui::Rect::from_min_max(egui::pos2(body.right() - INFO_W, body.top()), body.max);
        let left = egui::Rect::from_min_max(body.min, egui::pos2(info.left() - split, body.bottom()));
        let bag = egui::Rect::from_min_max(egui::pos2(left.left(), left.bottom() - bag_h), left.max);
        let main = egui::Rect::from_min_max(left.min, egui::pos2(left.right(), bag.top() - split));
        Regions { window, main, info, bag }
    } else {
        let bag = egui::Rect::from_min_max(egui::pos2(body.left(), body.bottom() - bag_h), body.max);
        let info = egui::Rect::from_min_max(egui::pos2(body.left(), bag.top() - split - NARROW_INFO_H), egui::pos2(body.right(), bag.top() - split));
        let main = egui::Rect::from_min_max(body.min, egui::pos2(body.right(), info.top() - split));
        Regions { window, main, info, bag }
    }
}

/// What is being dragged, and where it was picked up.
// Output and Hand are picked up by the machine and Yap tabs, which are built on this module.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Carry {
    Bag(ItemKind),
    Output { building: usize, item: ItemKind },
    Hand(usize),
}

/// How a slot is drawn and whether it reacts to the pointer.
// Target is shown by the machine and Yap drop zones, which are built on this module.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SlotLook {
    Normal,
    Empty,
    /// Drawn dimmed; it never reports a click or starts a drag.
    Disabled,
    /// A dragged item may be dropped here.
    Target,
}

/// Everything a tab needs for one frame.
pub(crate) struct Cx<'a> {
    pub w: &'a World,
    pub me: &'a Agent,
    pub ui_ctx: &'a hud::UiCtx<'a>,
    pub hands: &'a mut [u8; 4],
    pub acts: &'a mut Vec<UiAction>,
}

/// The tab shown in the upper half of the window.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Top {
    Bag,
    Craft,
    Build,
    Machine(usize),
}

/// Called by the window for every bag slot; each top tab decides what a click means.
pub(crate) type BagClick<'f> = &'f mut dyn FnMut(&mut Cx, ItemKind, bool);

pub(crate) const SLOT: f32 = 56.0;
const GAP: f32 = 6.0;
const WIN_MAX: Vec2 = egui::vec2(1100.0, 720.0);
const PAD: f32 = 14.0;
const HEADER_H: f32 = 58.0;
const INFO_W: f32 = 260.0;
const NARROW_INFO_H: f32 = 130.0;
const BAG_HEAD: f32 = 32.0;
/// Room between a well's edge and what is drawn in it.
const WELL_PAD: f32 = 8.0;
/// The most a single drop puts on the ground, as the old right-click menu did.
const DROP_MAX: u16 = 10;

/// One slot in the era's look; item None = empty. Hover shows item_tip.
pub(crate) fn slot(ui: &mut Ui, w: &World, item: Option<ItemKind>, n: u16, look: SlotLook) -> Response {
    let live = item.is_some() && look != SlotLook::Disabled && look != SlotLook::Empty;
    let sense = if live { egui::Sense::click_and_drag() } else { egui::Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(SLOT, SLOT), sense);
    let era = crate::style::current_era();
    let pal = crate::style::palette(era);
    let p = ui.painter();
    crate::style::slot_frame(p, rect, era, look);
    if let Some(k) = item {
        item_in(p, rect, w, k, era);
        if n > 1 {
            // slot counts are the one place text may be smaller than the body; 22 keeps them readable
            let at = rect.right_bottom() - egui::vec2(5.0, 1.0);
            let font = egui::FontId::proportional(22.0);
            p.text(at + Vec2::splat(crate::pxui::unit()), Align2::RIGHT_BOTTOM, n.to_string(), font.clone(), pal.slot_shadow);
            p.text(at, Align2::RIGHT_BOTTOM, n.to_string(), font, Color32::WHITE);
        }
    }
    match look {
        SlotLook::Disabled => {
            p.rect_filled(crate::pxui::snap_rect(rect), 0.0, Color32::from_black_alpha(150));
        }
        SlotLook::Normal if resp.hovered() => crate::style::highlight(p, rect, pal.light),
        _ => {}
    }
    if let (Some(k), true) = (item, resp.hovered()) {
        let pass = ui.ctx().cumulative_pass_nr();
        ui.ctx().data_mut(|d| d.insert_temp(hover_id(), (pass, k)));
    }
    let name = item.map_or_else(|| "boş".to_string(), |k| crate::lang::item_word(w, crate::lang::looks_like(w, k)));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, look != SlotLook::Disabled, &name));
    let Some(k) = item else { return resp };
    crate::pilot::mark(ui.ctx(), || format!("slot:{}", alaz_sim::debug::snake(&format!("{k:?}"))), rect);
    // dev aid for screenshots: ALAZ_DEMO_TIP keeps the cloak's tooltip open
    if k == ItemKind::HideCloak && std::env::var_os("ALAZ_DEMO_TIP").is_some() {
        let tip = hud::item_tip(w, k);
        egui::Area::new(egui::Id::NULL.with("demo_tip")).order(egui::Order::Tooltip).fixed_pos(resp.rect.right_top()).show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| ui.label(tip));
        });
    }
    resp.on_hover_text(hud::item_tip(w, crate::lang::looks_like(w, k)))
}

fn hover_id() -> egui::Id {
    egui::Id::NULL.with("inv_hover_item")
}

/// The item in the slot under the pointer, this pass or the one before: the bag is drawn after the tab, so a tab's
/// info panel learns of a bag hover one pass late.
pub(crate) fn hovered_item(ctx: &Context) -> Option<ItemKind> {
    let now = ctx.cumulative_pass_nr();
    ctx.data(|d| d.get_temp::<(u64, ItemKind)>(hover_id())).filter(|&(at, _)| at + 1 >= now).map(|(_, k)| k)
}

/// Makes `resp` a drag source carrying `c`.
pub(crate) fn drag_source(resp: &Response, c: Carry) {
    resp.dnd_set_drag_payload(c);
}

/// The Carry released on `resp` this frame, if any.
pub(crate) fn dropped_on(resp: &Response) -> Option<Carry> {
    resp.dnd_release_payload::<Carry>().map(|c| *c)
}

/// The Carry being dragged over `resp` right now, if any (for `SlotLook::Target`).
pub(crate) fn hovering(resp: &Response) -> Option<Carry> {
    resp.dnd_hover_payload::<Carry>().map(|c| *c)
}

/// What the window shows on top this frame, or None when it is closed.
pub(crate) fn top(cx: &Cx) -> Option<Top> {
    match cx.ui_ctx.tab {
        Some(Tab::Bag) => return Some(Top::Bag),
        Some(Tab::Craft) => return Some(Top::Craft),
        Some(Tab::Build) => return Some(Top::Build),
        _ => {}
    }
    open_machine(cx).map(Top::Machine)
}

/// The selected building, if it is one the window opens for. In a cave nothing outside is selected.
fn open_machine(cx: &Cx) -> Option<usize> {
    open_machine_of(cx.w, cx.me, cx.ui_ctx.selection)
}

/// The same for callers without a `Cx` (the book).
pub(crate) fn open_machine_of(w: &World, me: &Agent, selection: Option<Selection>) -> Option<usize> {
    if me.cave.is_some() {
        return None;
    }
    match selection {
        Some(Selection::Building(i)) => w.buildings.get(i).filter(|b| machine::opens_window(w, b.kind)).map(|_| i),
        _ => None,
    }
}

/// Puts a carried item on the ground (let go outside the window or on the drop box); a held item leaves the hand. A bag heap goes ten at a time, or whole with `all` (shift held).
pub(crate) fn drop_carry(cx: &mut Cx, c: Carry, all: bool) {
    match c {
        Carry::Bag(k) => {
            let have = cx.me.inventory.get(k);
            if have > 0 {
                let n = if all { have } else { have.min(DROP_MAX) };
                cx.acts.push(UiAction::Cmd(CommandKind::Drop { item: k, n }));
            }
        }
        Carry::Hand(i) => {
            if let Some(h) = cx.hands.get_mut(i) {
                *h = 255;
            }
        }
        // machine output only moves into the bag
        Carry::Output { .. } => {}
    }
}

/// A carried item let go over the bag grid.
pub(crate) fn into_bag(cx: &mut Cx, c: Carry) {
    match c {
        Carry::Output { building, item } => cx.acts.push(UiAction::Cmd(CommandKind::Collect { building: building as u32, item: Some(item) })),
        Carry::Hand(i) => {
            if let Some(h) = cx.hands.get_mut(i) {
                *h = 255;
            }
        }
        Carry::Bag(_) => {}
    }
}

/// Decides Top, draws the tab row, the top tab and the bag grid. Returns the window's rect, or None when it is not shown.
pub(crate) fn draw_window(ctx: &Context, cx: &mut Cx) -> Option<egui::Rect> {
    // while the hands are busy with a workpiece the window steps aside instead of lying over it
    if cx.w.craft_view(cx.ui_ctx.me).is_some() {
        return None;
    }
    let top = top(cx)?;
    let era = crate::style::current_era();
    let screen = ctx.content_rect();
    // an open book on a wide screen takes the left side and moves the window right by half of its own width
    let regs = layout(screen).shifted(crate::book::dock_shift(ctx, screen));
    ctx.data_mut(|d| d.insert_temp(regions_id(), regs));
    let fam = match top {
        Top::Machine(i) => Some(crate::style::family(cx.w.buildings[i].kind)),
        _ => None,
    };
    let area = egui::Area::new(egui::Id::NULL.with("inv_window")).fixed_pos(regs.window.min).constrain(false).order(egui::Order::Foreground).fade_in(false).show(ctx, |ui| {
        ui.expand_to_include_rect(regs.window);
        crate::style::apply_visuals(ui.visuals_mut(), era);
        let p = ui.painter().clone();
        crate::style::frame(&p, regs.window, era, fam.map(crate::style::accent));
        header(ui, cx, top, era, regs.window);
        crate::style::frame(&p, regs.info, era, None);
        ui.scope_builder(egui::UiBuilder::new().max_rect(regs.main), |ui| {
            egui::ScrollArea::vertical().id_salt("inv_top").max_height(regs.main.height()).auto_shrink([false, false]).show(ui, |ui| match top {
                Top::Bag => bag::draw(ui, cx),
                Top::Craft => craft::draw(ui, cx),
                Top::Build => build::draw(ui, cx),
                Top::Machine(i) => machine::draw(ui, cx, i),
            });
        });
        crate::style::well(&p, regs.bag, era);
        let mut click = |cx: &mut Cx, k: ItemKind, shift: bool| match top {
            Top::Bag => bag::click(cx, k, shift),
            Top::Craft => craft::click(cx, k, shift),
            Top::Build => {}
            Top::Machine(i) => machine::click(cx, i, k, shift),
        };
        ui.scope_builder(egui::UiBuilder::new().max_rect(regs.bag.shrink(WELL_PAD)), |ui| bag_grid(ui, cx, top, era, &mut click));
        // after the bag grid, so the hover of this frame is already known
        ui.scope_builder(egui::UiBuilder::new().max_rect(regs.info.shrink(WELL_PAD + 6.0)), |ui| match top {
            Top::Craft => craft::draw_info(ui, cx),
            Top::Build => build::draw_info(ui, cx),
            Top::Bag | Top::Machine(_) => {}
        });
        // the whole bag area takes back what was carried out of it (machine output, a held item)
        crate::pilot::mark(ui.ctx(), || "bag".to_string(), regs.bag);
        let zone = ui.interact(regs.bag, egui::Id::NULL.with("inv_bag_zone"), egui::Sense::hover());
        if hovering(&zone).is_some_and(|c| !matches!(c, Carry::Bag(_))) {
            crate::style::highlight(&p, regs.bag, crate::style::palette(era).good);
        }
        if let Some(c) = dropped_on(&zone) {
            into_bag(cx, c);
        }
    });
    let released = ctx.input(|i| i.pointer.any_released());
    let pos = ctx.input(|i| i.pointer.latest_pos());
    if let Some(c) = egui::DragAndDrop::payload::<Carry>(ctx).map(|c| *c) {
        match pos {
            Some(p) if released && !area.response.rect.contains(p) => {
                egui::DragAndDrop::clear_payload(ctx);
                drop_carry(cx, c, ctx.input(|i| i.modifiers.shift));
            }
            Some(p) if !released => ghost(ctx, cx, c, p),
            _ => {}
        }
    }
    Some(regs.window)
}

/// The item picture inside a slot rect; an item never seen is a "?".
fn item_in(p: &egui::Painter, rect: egui::Rect, w: &World, k: ItemKind, era: crate::style::Era) {
    let inner = rect.shrink(rect.width() * 0.14);
    if w.has_seen(k) {
        crate::icons::draw_item(p, inner, crate::lang::looks_like(w, k));
    } else {
        crate::style::icons::icon(p, inner.shrink(4.0), crate::style::icons::Icon::Question, crate::style::palette(era).ink_dim);
    }
}

/// The dragged item follows the pointer.
fn ghost(ctx: &Context, cx: &Cx, c: Carry, p: egui::Pos2) {
    let item = match c {
        Carry::Bag(k) | Carry::Output { item: k, .. } => Some(k),
        Carry::Hand(i) => cx.hands.get(i).and_then(|&h| ItemKind::ALL.get(h as usize)).copied(),
    };
    let Some(k) = item else { return };
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::NULL.with("inv_ghost")));
    let r = egui::Rect::from_center_size(p, egui::vec2(SLOT * 0.75, SLOT * 0.75));
    let era = crate::style::current_era();
    crate::style::slot_frame(&painter, r, era, SlotLook::Normal);
    item_in(&painter, r, cx.w, k, era);
}

/// The header: what is open (picture and name) on the left; the tabs as icon buttons and the close button on the
/// right. The tab word is the button's tooltip.
fn header(ui: &mut Ui, cx: &mut Cx, top: Top, era: crate::style::Era, window: egui::Rect) {
    use crate::style::icons::{family_button, icon, icon_button, icon_colour, Icon};
    let w = cx.w;
    let pal = crate::style::palette(era);
    let r = egui::Rect::from_min_size(window.min + Vec2::splat(PAD), egui::vec2(window.width() - 2.0 * PAD, HEADER_H - 10.0));
    let p = ui.painter().clone();
    let pic = egui::Rect::from_center_size(egui::pos2(r.left() + 20.0, r.center().y), Vec2::splat(40.0));
    let title = match top {
        Top::Machine(i) => {
            let kind = w.buildings[i].kind;
            crate::style::icons::family_icon(&p, pic, crate::style::family(kind));
            alaz_sim::rules().building(kind).name.clone()
        }
        _ => {
            let (ic, word) = match top {
                Top::Craft => (Icon::Hammer, "Yap"),
                Top::Build => (Icon::House, "İnşa"),
                _ => (Icon::Bag, "Çanta"),
            };
            icon(&p, pic, ic, icon_colour(era));
            crate::lang::tab_word(w, word).to_string()
        }
    };
    // painted, not a label: the tab buttons already carry these words for screen readers and tests
    let at = egui::pos2(pic.right() + 12.0, r.center().y);
    let font = egui::FontId::proportional(30.0);
    p.text(at + Vec2::splat(crate::pxui::unit()), Align2::LEFT_CENTER, &title, font.clone(), pal.slot_shadow);
    p.text(at, Align2::LEFT_CENTER, &title, font, pal.ink);
    let rule = egui::Rect::from_min_size(egui::pos2(r.left(), r.bottom() + 3.0), egui::vec2(r.width(), crate::pxui::unit()));
    p.rect_filled(crate::pxui::snap_rect(rule), 0.0, crate::style::mix(pal.base, pal.edge, 0.6));
    let has_build = alaz_sim::rules().buildings.iter().any(|b| w.knows(b.discovery));
    let machine = open_machine(cx);
    ui.scope_builder(egui::UiBuilder::new().max_rect(r).layout(egui::Layout::right_to_left(egui::Align::Center)), |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if icon_button(ui, Icon::Close, "Kapat", false, era).clicked() {
            cx.acts.push(UiAction::CloseWindow);
        }
        ui.add_space(16.0);
        if let Some(i) = machine {
            let on = top == Top::Machine(i);
            let kind = w.buildings[i].kind;
            let name = alaz_sim::rules().building(kind).name.as_str();
            if family_button(ui, crate::style::family(kind), name, on, era).clicked() && !on {
                // clearing the window tab lets the selected machine come to the top
                if let Some(tab) = cx.ui_ctx.tab {
                    cx.acts.push(UiAction::ToggleTab(tab));
                }
            }
        }
        let tabs = [(Tab::Build, Top::Build, "İnşa", Icon::House, has_build), (Tab::Craft, Top::Craft, "Yap", Icon::Hammer, true), (Tab::Bag, Top::Bag, "Çanta", Icon::Bag, true)];
        for (tab, t, word, ic, enabled) in tabs {
            let on = top == t;
            let resp = ui.add_enabled_ui(enabled, |ui| icon_button(ui, ic, crate::lang::tab_word(w, word), on, era)).inner;
            if resp.clicked() && !on {
                cx.acts.push(UiAction::ToggleTab(tab));
            }
        }
    });
}

/// The bag, grouped by what the eye can tell apart, one slot per heap. An open machine dims what it would not take.
fn bag_grid(ui: &mut Ui, cx: &mut Cx, top: Top, era: crate::style::Era, click: BagClick) {
    let (w, inv) = (cx.w, &cx.me.inventory);
    let mut items: Vec<(ItemKind, u16)> = Vec::new();
    for &k in ItemKind::ALL.iter() {
        let n = inv.get(k);
        if n == 0 {
            continue;
        }
        let seen = crate::lang::looks_like(w, k);
        match items.iter_mut().find(|(s, _)| *s == seen) {
            Some(e) => e.1 += n,
            None => items.push((seen, n)),
        }
    }
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(24.0), egui::Sense::hover());
        crate::style::icons::icon(ui.painter(), r, crate::style::icons::Icon::Bag, crate::style::icons::icon_colour(era));
        ui.label(format!("{}/{}", inv.load(), cx.me.capacity())).on_hover_text(format!(
            "Çanta: {}/{} yer dolu. Bırakmak için eşyaya sağ tıkla ya da «Yere bırak» kutusuna sürükle. Shift basılıysa yığının hepsi gider.",
            inv.load(),
            cx.me.capacity()
        ));
        // a target that is always in the window: the wide window leaves almost no empty margin to drop into
        let (zone, resp) = ui.allocate_exact_size(egui::vec2(200.0, 26.0), egui::Sense::hover());
        let pal = crate::style::palette(era);
        let live = hovering(&resp).is_some_and(|c| matches!(c, Carry::Bag(_) | Carry::Hand(_)));
        crate::style::slot_frame(ui.painter(), zone, era, if live { SlotLook::Target } else { SlotLook::Empty });
        ui.painter().text(zone.center(), Align2::CENTER_CENTER, "Yere bırak (Shift: hepsi)", egui::FontId::proportional(18.0), pal.ink);
        crate::pilot::mark(ui.ctx(), || "dropzone".to_string(), zone);
        if let Some(c) = dropped_on(&resp) {
            let all = ui.input(|i| i.modifiers.shift);
            drop_carry(cx, c, all);
        }
    });
    if items.is_empty() {
        ui.label("Çanta boş. Etraftan taş, odun ve yemiş topla: bir şeye sağ tıkla.");
        return;
    }
    let cols = (((ui.available_width() + GAP) / (SLOT + GAP)).floor() as usize).max(1);
    let shift = ui.input(|i| i.modifiers.shift);
    egui::ScrollArea::vertical().id_salt("inv_bag").auto_shrink([false, false]).show(ui, |ui| bag_slots(ui, cx, top, items, cols, shift, click));
}

fn bag_slots(ui: &mut Ui, cx: &mut Cx, top: Top, items: Vec<(ItemKind, u16)>, cols: usize, shift: bool, click: BagClick) {
    let (w, inv) = (cx.w, &cx.me.inventory);
    egui::Grid::new("inv_bag_grid").spacing([GAP, GAP]).show(ui, |ui| {
        for (n, (k, count)) in items.into_iter().enumerate() {
            // from a mixed heap, whichever comes to hand (the biggest share)
            let real = crate::lang::hidden_in(w, inv, k).into_iter().max_by_key(|(_, c)| *c).map_or(k, |(r, _)| r);
            let look = match top {
                Top::Machine(i) => machine::bag_look(cx, i, real),
                _ => SlotLook::Normal,
            };
            let resp = slot(ui, w, Some(k), count, look);
            drag_source(&resp, Carry::Bag(real));
            if resp.clicked() {
                click(cx, real, shift);
            }
            resp.context_menu(|ui| {
                if ui.button("1 tane bırak").clicked() {
                    cx.acts.push(UiAction::Cmd(CommandKind::Drop { item: real, n: 1 }));
                    ui.close();
                }
                let m = cx.me.inventory.get(real);
                if m > 1 && ui.button(format!("{} tane bırak", m.min(DROP_MAX))).clicked() {
                    cx.acts.push(UiAction::Cmd(CommandKind::Drop { item: real, n: m.min(DROP_MAX) }));
                    ui.close();
                }
            });
            if (n + 1) % cols == 0 {
                ui.end_row();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::{KnapState, UiCtx};
    use crate::keys::Keybinds;
    use alaz_sim::command::Command;
    use alaz_sim::AgentId;
    use egui_kittest::kittest::Queryable;

    pub(crate) struct State {
        pub w: World,
        pub knap: KnapState,
        pub acts: Vec<UiAction>,
        pub tab: Option<Tab>,
        pub selection: Option<Selection>,
        pub binds: Keybinds,
    }

    pub(crate) fn ui_ctx<'a>(s_tab: Option<Tab>, selection: Option<Selection>, binds: &'a Keybinds) -> UiCtx<'a> {
        UiCtx {
            hints: &[],
            build_mode: None,
            grid_snap: false,
            selection,
            recording: false,
            recorded: &[],
            me: AgentId(0),
            net_status: "",
            last_strike: None,
            tab: s_tab,
            binds,
            anchor: Some([0.0, 0.0]),
            grip: true,
            time_scale: 1,
            evo: 9.0,
            yaw: 0.0,
            fade: 0.0,
            cave_tiles: None,
        }
    }

    pub(crate) fn harness(w: World, tab: Option<Tab>, selection: Option<Selection>) -> egui_kittest::Harness<'static, State> {
        let state = State { w, knap: KnapState::default(), acts: Vec::new(), tab, selection, binds: Keybinds::default() };
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1280.0, 800.0)).build_ui_state(
            |ui, s: &mut State| {
                let ui_ctx = ui_ctx(s.tab, s.selection, &s.binds);
                hud::draw(ui.ctx(), &s.w, &ui_ctx, &mut s.knap, &mut s.acts);
            },
            state,
        );
        h.run();
        h
    }

    pub(crate) fn world_with(items: &[(ItemKind, u16)]) -> World {
        let mut w = World::new(5);
        let (sx, sy) = w.spawn_mm;
        w.spawn_agent(sx, sy);
        for &(k, n) in items {
            w.agents[0].inventory.counts[k as usize] = n;
            w.mark_seen(k);
        }
        w
    }

    #[test]
    fn window_opens_on_each_tab() {
        for tab in [Tab::Bag, Tab::Craft, Tab::Build] {
            let mut w = world_with(&[(ItemKind::Stone, 3)]);
            w.learn_everything();
            let h = harness(w, Some(tab), None);
            for word in ["Çanta", "Yap", "İnşa"] {
                assert!(h.query_by_label(word).is_some(), "{tab:?}: no {word} tab button");
            }
            assert!(h.query_all_by_label("Taş").count() > 0, "{tab:?}: no stone slot in the bag grid");
        }
    }

    #[test]
    fn window_steps_aside_for_the_workpiece() {
        let mut w = world_with(&[(ItemKind::Stone, 2)]);
        w.step(&[Command { agent: AgentId(0), kind: CommandKind::Combine { items: [ItemKind::Stone as u8, ItemKind::Stone as u8, 255, 255] } }]);
        assert!(w.craft_view(AgentId(0)).is_some(), "knapping started");
        let h = harness(w, Some(Tab::Bag), None);
        assert!(h.query_by_label("Çanta").is_none(), "the window lies over the workpiece");
        assert!(h.query_by_label("Torba").is_none(), "the window lies over the workpiece");
    }

    #[test]
    fn dragging_a_bag_item_outside_drops_it() {
        for (have, want) in [(25u16, 10u16), (3, 3)] {
            let w = world_with(&[(ItemKind::Stone, have)]);
            let binds = Keybinds::default();
            let ui_ctx = ui_ctx(Some(Tab::Bag), None, &binds);
            let mut hands = [255u8; 4];
            let mut acts = Vec::new();
            let mut cx = Cx { w: &w, me: &w.agents[0], ui_ctx: &ui_ctx, hands: &mut hands, acts: &mut acts };
            drop_carry(&mut cx, Carry::Bag(ItemKind::Stone), false);
            assert!(
                matches!(acts.as_slice(), [UiAction::Cmd(CommandKind::Drop { item: ItemKind::Stone, n })] if *n == want),
                "{have} stones: expected one Drop of {want}"
            );
        }
    }

    #[test]
    fn window_fits_720p() {
        let kinds = [
            ItemKind::Stone,
            ItemKind::Wood,
            ItemKind::Berries,
            ItemKind::Clay,
            ItemKind::Fiber,
            ItemKind::Branch,
            ItemKind::Hide,
            ItemKind::RawMeat,
            ItemKind::Seed,
            ItemKind::Herb,
        ];
        let items: Vec<(ItemKind, u16)> = kinds.iter().map(|&k| (k, 2)).collect();
        let w = world_with(&items);
        let names: Vec<String> = kinds.iter().map(|&k| crate::lang::item_word(&w, crate::lang::looks_like(&w, k))).collect();
        let state = State { w, knap: KnapState::default(), acts: Vec::new(), tab: Some(Tab::Bag), selection: None, binds: Keybinds::default() };
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1280.0, 720.0)).build_ui_state(
            |ui, s: &mut State| {
                let ui_ctx = ui_ctx(s.tab, s.selection, &s.binds);
                hud::draw(ui.ctx(), &s.w, &ui_ctx, &mut s.knap, &mut s.acts);
            },
            state,
        );
        h.run();
        let screen = h.ctx.content_rect();
        let regions = regions(&h.ctx).expect("the window was laid out");
        assert!(screen.contains_rect(regions.window), "window {:?} leaves the screen {screen:?}", regions.window);
        for r in [regions.main, regions.info, regions.bag] {
            assert!(regions.window.contains_rect(r), "region {r:?} leaves the window {:?}", regions.window);
        }
        // the bag slots are the nodes with these names inside the bag region
        let tops: Vec<f32> = names.iter().filter_map(|n| h.query_all_by_label(n).map(|node| node.rect()).find(|r| regions.bag.contains_rect(*r))).map(|r| r.top()).collect();
        assert_eq!(tops.len(), names.len(), "a bag slot is missing");
        let row = tops.iter().copied().fold(f32::MAX, f32::min);
        let first_row = tops.iter().filter(|&&t| (t - row).abs() < 1.0).count();
        assert!(first_row >= 8, "only {first_row} bag columns");
    }

    #[test]
    fn close_button_closes() {
        let w = world_with(&[(ItemKind::Stone, 3)]);
        let mut h = harness(w, Some(Tab::Bag), None);
        h.get_by_label("Kapat").click();
        h.run();
        assert!(h.state().acts.iter().any(|a| matches!(a, UiAction::CloseWindow)), "Kapat asks to close the window");
    }
}
