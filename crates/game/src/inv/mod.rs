//! The inventory window: one centered window with a tab row (Çanta, Yap, İnşa, and the open machine), the chosen
//! tab on top and the bag grid always below it. Items move between the parts by egui drag and drop.

use alaz_render::egui::{self, Align2, Color32, Context, Response, Stroke, Ui};
use alaz_sim::world::Agent;
use alaz_sim::{CommandKind, ItemKind, World};

use crate::hud::{self, Selection, Tab, UiAction};

pub(crate) mod bag;
pub(crate) mod build;
pub(crate) mod craft;
pub(crate) mod machine;

/// What is being dragged, and where it was picked up.
// Output and Hand are picked up by the machine and Yap tabs, which are built on this module.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Carry {
    Bag(ItemKind),
    Output { building: usize, item: ItemKind },
    Hand(usize),
}

/// How a slot is drawn and whether it reacts to the pointer.
// Target is shown by the machine and Yap drop zones, which are built on this module.
#[allow(dead_code)]
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

pub(crate) const SLOT: f32 = 64.0;
const BAG_COLS: usize = 10;
const GAP: f32 = 6.0;
const MAX_W: f32 = 760.0;
const MAX_H: f32 = 560.0;
/// The most a single drop puts on the ground, as the old right-click menu did.
const DROP_MAX: u16 = 10;

/// 64×64 slot; item None = empty. Hover shows item_tip.
pub(crate) fn slot(ui: &mut Ui, w: &World, item: Option<ItemKind>, n: u16, look: SlotLook) -> Response {
    let live = item.is_some() && look != SlotLook::Disabled && look != SlotLook::Empty;
    let sense = if live { egui::Sense::click_and_drag() } else { egui::Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(SLOT, SLOT), sense);
    let p = ui.painter();
    match item {
        Some(k) => {
            crate::icons::draw_slot(p, rect, crate::lang::looks_like(w, k), w.has_seen(k));
            if n > 1 {
                p.text(rect.right_bottom() - egui::vec2(4.0, 2.0), Align2::RIGHT_BOTTOM, n.to_string(), egui::FontId::proportional(20.0), Color32::WHITE);
            }
        }
        None => {
            p.rect_filled(rect, 0.0, Color32::from_rgb(26, 21, 17));
            p.rect_stroke(crate::pxui::snap_rect(rect), 0.0, Stroke::new(crate::pxui::unit(), Color32::from_rgb(70, 56, 40)), egui::StrokeKind::Inside);
        }
    }
    match look {
        SlotLook::Disabled => {
            p.rect_filled(rect, 0.0, Color32::from_black_alpha(150));
        }
        SlotLook::Target => {
            p.rect_stroke(crate::pxui::snap_rect(rect), 0.0, Stroke::new(crate::pxui::unit() * 2.0, Color32::from_rgb(130, 215, 130)), egui::StrokeKind::Inside);
        }
        SlotLook::Normal if resp.hovered() => {
            p.rect_stroke(crate::pxui::snap_rect(rect), 0.0, Stroke::new(crate::pxui::unit() * 2.0, Color32::from_rgb(214, 160, 70)), egui::StrokeKind::Inside);
        }
        _ => {}
    }
    let name = item.map_or_else(|| "boş".to_string(), |k| crate::lang::item_word(w, crate::lang::looks_like(w, k)));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, look != SlotLook::Disabled, &name));
    let Some(k) = item else { return resp };
    // dev aid for screenshots: ALAZ_DEMO_TIP keeps the cloak's tooltip open
    if k == ItemKind::HideCloak && std::env::var_os("ALAZ_DEMO_TIP").is_some() {
        let tip = hud::item_tip(w, k);
        egui::Area::new(egui::Id::NULL.with("demo_tip")).order(egui::Order::Tooltip).fixed_pos(resp.rect.right_top()).show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| ui.label(tip));
        });
    }
    resp.on_hover_text(hud::item_tip(w, crate::lang::looks_like(w, k)))
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
    if cx.me.cave.is_some() {
        return None;
    }
    match cx.ui_ctx.selection {
        Some(Selection::Building(i)) => cx.w.buildings.get(i).filter(|b| machine::opens_window(cx.w, b.kind)).map(|_| i),
        _ => None,
    }
}

/// A dragged item let go outside the window. A bag item goes on the ground; a held item leaves the hand.
pub(crate) fn drop_outside(cx: &mut Cx, c: Carry) {
    match c {
        Carry::Bag(k) => {
            let have = cx.me.inventory.get(k);
            if have > 0 {
                cx.acts.push(UiAction::Cmd(CommandKind::Drop { item: k, n: have.min(DROP_MAX) }));
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

/// Decides Top, draws the tab row, the top tab and the bag grid.
pub(crate) fn draw_window(ctx: &Context, cx: &mut Cx) {
    // while the hands are busy with a workpiece the window steps aside instead of lying over it
    if cx.w.craft_view(cx.ui_ctx.me).is_some() {
        return;
    }
    let Some(top) = top(cx) else { return };
    let screen = ctx.content_rect();
    let width = MAX_W.min(screen.width() - 40.0).max(SLOT * 3.0);
    // leave room for the action bar below and the status line above, whatever the window size
    let height = MAX_H.min(screen.height() - 150.0).max(240.0);
    let area = egui::Area::new(egui::Id::NULL.with("inv_window")).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
        hud::panel(ui, 6.0, |ui| {
            ui.set_width(width);
            tab_row(ui, cx, top);
            ui.separator();
            let rest = (height - 60.0).max(160.0);
            egui::ScrollArea::vertical().id_salt("inv_top").max_height(rest * 0.5).auto_shrink([false, true]).show(ui, |ui| match top {
                Top::Bag => bag::draw(ui, cx),
                Top::Craft => craft::draw(ui, cx),
                Top::Build => build::draw(ui, cx),
                Top::Machine(i) => machine::draw(ui, cx, i),
            });
            ui.separator();
            let mut click = |cx: &mut Cx, k: ItemKind, shift: bool| match top {
                Top::Bag => bag::click(cx, k, shift),
                Top::Craft => craft::click(cx, k, shift),
                Top::Build => {}
                Top::Machine(i) => machine::click(cx, i, k, shift),
            };
            let grid = egui::ScrollArea::vertical().id_salt("inv_bag").max_height(rest * 0.5).auto_shrink([false, true]).show(ui, |ui| bag_grid(ui, cx, &mut click));
            // the whole bag area takes back what was carried out of it (machine output, a held item)
            let zone = ui.interact(grid.inner_rect, egui::Id::NULL.with("inv_bag_zone"), egui::Sense::hover());
            if hovering(&zone).is_some_and(|c| !matches!(c, Carry::Bag(_))) {
                ui.painter().rect_stroke(crate::pxui::snap_rect(grid.inner_rect), 0.0, Stroke::new(crate::pxui::unit() * 2.0, Color32::from_rgb(130, 215, 130)), egui::StrokeKind::Inside);
            }
            if let Some(c) = dropped_on(&zone) {
                into_bag(cx, c);
            }
        });
    });
    let released = ctx.input(|i| i.pointer.any_released());
    let pos = ctx.input(|i| i.pointer.latest_pos());
    if let Some(c) = egui::DragAndDrop::payload::<Carry>(ctx).map(|c| *c) {
        match pos {
            Some(p) if released && !area.response.rect.contains(p) => {
                egui::DragAndDrop::clear_payload(ctx);
                drop_outside(cx, c);
            }
            Some(p) if !released => ghost(ctx, cx, c, p),
            _ => {}
        }
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
    crate::icons::draw_slot(&painter, r, crate::lang::looks_like(cx.w, k), cx.w.has_seen(k));
}

fn tab_row(ui: &mut Ui, cx: &mut Cx, top: Top) {
    let w = cx.w;
    let has_build = alaz_sim::rules().buildings.iter().any(|b| w.knows(b.discovery));
    let machine = open_machine(cx);
    ui.horizontal(|ui| {
        let tabs = [(Tab::Bag, Top::Bag, "Çanta", true), (Tab::Craft, Top::Craft, "Yap", true), (Tab::Build, Top::Build, "İnşa", has_build)];
        for (tab, t, word, enabled) in tabs {
            let on = top == t;
            let resp = ui.add_enabled(enabled, egui::Button::new(crate::lang::tab_word(w, word)).selected(on));
            if resp.clicked() && !on {
                cx.acts.push(UiAction::ToggleTab(tab));
            }
        }
        if let Some(i) = machine {
            let on = top == Top::Machine(i);
            let name = alaz_sim::rules().building(w.buildings[i].kind).name.as_str();
            if ui.add(egui::Button::new(name).selected(on)).clicked() && !on {
                // clearing the window tab lets the selected machine come to the top
                if let Some(tab) = cx.ui_ctx.tab {
                    cx.acts.push(UiAction::ToggleTab(tab));
                }
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Kapat").clicked() {
                cx.acts.push(UiAction::CloseWindow);
            }
        });
    });
}

/// The bag, grouped by what the eye can tell apart, one slot per heap.
fn bag_grid(ui: &mut Ui, cx: &mut Cx, click: BagClick) {
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
    ui.label(egui::RichText::new(format!("Çanta: {}/{} yer dolu", inv.load(), cx.me.capacity())).small().weak());
    if items.is_empty() {
        ui.label("Çanta boş. Etraftan taş, odun ve yemiş topla: bir şeye sağ tıkla.");
        return;
    }
    let cols = (((ui.available_width() + GAP) / (SLOT + GAP)).floor() as usize).clamp(1, BAG_COLS);
    let shift = ui.input(|i| i.modifiers.shift);
    egui::Grid::new("inv_bag_grid").spacing([GAP, GAP]).show(ui, |ui| {
        for (n, (k, count)) in items.into_iter().enumerate() {
            // from a mixed heap, whichever comes to hand (the biggest share)
            let real = crate::lang::hidden_in(w, inv, k).into_iter().max_by_key(|(_, c)| *c).map_or(k, |(r, _)| r);
            let resp = slot(ui, w, Some(k), count, SlotLook::Normal);
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
    ui.label(egui::RichText::new("Sürükleyip pencerenin dışına bırak ya da sağ tıkla: yere bırakır.").small().weak());
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
            drop_outside(&mut cx, Carry::Bag(ItemKind::Stone));
            assert!(
                matches!(acts.as_slice(), [UiAction::Cmd(CommandKind::Drop { item: ItemKind::Stone, n })] if *n == want),
                "{have} stones: expected one Drop of {want}"
            );
        }
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
