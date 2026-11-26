//! The book: the days, the finds and the recipes of the tribe, in its own window. On a wide screen it docks to the left
//! of the inventory window; on a narrow one it lies over it, centred, and the window stays where it is.

use alaz_render::egui::{self, Align2, Context, Rect, Vec2};
use alaz_sim::world::Agent;
use alaz_sim::World;

use crate::hud::UiAction;
use crate::journal_ui::{JournalState, PageCx, SECTIONS};
use crate::style::icons::{icon, icon_button, icon_colour, Icon};

pub(crate) const BOOK_W: f32 = 520.0;
const GAP: f32 = 12.0;
const PAD: f32 = 14.0;
const HEADER_H: f32 = 58.0;
const WELL_PAD: f32 = 10.0;
const MAX_H: f32 = 720.0;
const PAGES: [Icon; 4] = [Icon::Clock, Icon::Flame, Icon::Hammer, Icon::Bag];

fn open_id() -> egui::Id {
    egui::Id::NULL.with("book_open")
}

fn rect_id() -> egui::Id {
    egui::Id::NULL.with("book_rect")
}

/// The HUD says each frame whether the book is open, so the inventory window can make room before it is drawn.
pub(crate) fn set_open(ctx: &Context, open: bool) {
    ctx.data_mut(|d| d.insert_temp(open_id(), open));
}

fn is_open(ctx: &Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(open_id())).unwrap_or(false)
}

/// The book's rect as it was drawn last frame, None while it is closed.
#[cfg(test)]
pub(crate) fn rect(ctx: &Context) -> Option<Rect> {
    ctx.data(|d| d.get_temp::<Option<Rect>>(rect_id())).flatten()
}

fn book_width(screen: Vec2) -> f32 {
    BOOK_W.min(screen.x - 40.0)
}

/// Both fit side by side with a margin of at least 12 points on the outer edges.
fn fits_beside(screen_w: f32, window_w: f32) -> bool {
    screen_w - window_w >= BOOK_W + 2.0 * GAP
}

/// How far the inventory window moves right so that book and window sit side by side, centred as a pair.
pub(crate) fn dock_shift(ctx: &Context, screen: Rect) -> f32 {
    let window_w = crate::inv::window_size(screen.size()).x;
    if !is_open(ctx) || !fits_beside(screen.width(), window_w) {
        return 0.0;
    }
    let u = crate::pxui::unit();
    ((BOOK_W + GAP) / 2.0 / u).round() * u
}

/// Draws the book when `st.book_open`. `inv_open` is the inventory window's rect while it is shown; `machine` the
/// building it has open, whose recipes the book lets the player pick.
pub(crate) fn draw(ctx: &Context, w: &World, me: &Agent, st: &mut JournalState, inv_open: Option<Rect>, machine: Option<usize>, acts: &mut Vec<UiAction>) {
    if !st.book_open {
        ctx.data_mut(|d| d.insert_temp::<Option<Rect>>(rect_id(), None));
        return;
    }
    let screen = ctx.content_rect();
    let era = crate::style::current_era();
    let pal = crate::style::palette(era);
    let size = egui::vec2(book_width(screen.size()), inv_open.map_or_else(|| MAX_H.min(screen.height() - 80.0), |r| r.height()));
    let top = inv_open.map_or_else(|| screen.center().y - size.y / 2.0, |r| r.top());
    let overlay = inv_open.is_some_and(|r| !fits_beside(screen.width(), r.width()));
    let left = match inv_open {
        Some(r) if !overlay => r.left() - GAP - size.x,
        _ => screen.center().x - size.x / 2.0,
    };
    let book = crate::pxui::snap_rect(Rect::from_min_size(egui::pos2(left, top), size));
    ctx.data_mut(|d| d.insert_temp(rect_id(), Some(book)));

    let area = egui::Area::new(egui::Id::NULL.with("book")).fixed_pos(book.min).constrain(false).order(egui::Order::Foreground).fade_in(false).show(ctx, |ui| {
        ui.expand_to_include_rect(book);
        crate::style::apply_visuals(ui.visuals_mut(), era);
        let p = ui.painter().clone();
        crate::style::frame(&p, book, era, None);
        let head = Rect::from_min_size(book.min + Vec2::splat(PAD), egui::vec2(book.width() - 2.0 * PAD, HEADER_H - 10.0));
        let pic = Rect::from_center_size(egui::pos2(head.left() + 20.0, head.center().y), Vec2::splat(40.0));
        icon(&p, pic, Icon::Book, icon_colour(era));
        // painted, not a label: the action bar's button already carries this word for screen readers and tests
        let at = egui::pos2(pic.right() + 12.0, head.center().y);
        let font = egui::FontId::proportional(30.0);
        p.text(at + Vec2::splat(crate::pxui::unit()), Align2::LEFT_CENTER, "Günlük", font.clone(), pal.slot_shadow);
        p.text(at, Align2::LEFT_CENTER, "Günlük", font, pal.ink);
        let rule = Rect::from_min_size(egui::pos2(head.left(), head.bottom() + 3.0), egui::vec2(head.width(), crate::pxui::unit()));
        p.rect_filled(crate::pxui::snap_rect(rule), 0.0, crate::style::mix(pal.base, pal.edge, 0.6));
        ui.scope_builder(egui::UiBuilder::new().max_rect(head).layout(egui::Layout::right_to_left(egui::Align::Center)), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if icon_button(ui, Icon::Close, "Defteri kapat", false, era).clicked() {
                st.book_open = false;
            }
            ui.add_space(16.0);
            for i in (0..PAGES.len()).rev() {
                if icon_button(ui, PAGES[i], SECTIONS[i], st.section as usize == i, era).clicked() {
                    st.section = i as u8;
                }
            }
        });
        let body = Rect::from_min_max(egui::pos2(book.left() + PAD, book.top() + PAD + HEADER_H), book.max - Vec2::splat(PAD));
        crate::style::well(&p, body, era);
        let inner = body.shrink(WELL_PAD);
        let mut pc = PageCx { me, machine: machine.map(|i| (i, w.buildings[i].kind)), acts };
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            if st.section == 1 {
                ui.set_max_size(inner.size());
                crate::journal_ui::page(ui, w, st, &mut pc);
                return;
            }
            egui::ScrollArea::vertical().id_salt(("book_page", st.section)).max_height(inner.height()).auto_shrink([false, false]).show(ui, |ui| {
                ui.set_width(inner.width() - 12.0);
                crate::journal_ui::page(ui, w, st, &mut pc);
            });
        });
    });
    // lying over the window, the book is the one the player is reading: a click on the window must not bury it
    if overlay {
        ctx.move_to_top(area.response.layer_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::{KnapState, Selection, Tab, UiCtx};
    use crate::keys::Keybinds;
    use alaz_sim::command::Command;
    use alaz_sim::{AgentId, BuildingKind, CommandKind, ItemKind};
    use egui_kittest::kittest::Queryable;

    struct State {
        w: World,
        knap: KnapState,
        acts: Vec<UiAction>,
        tab: Option<Tab>,
        selection: Option<Selection>,
        binds: Keybinds,
    }

    fn harness(w: World, tab: Option<Tab>, selection: Option<Selection>, section: u8, book: bool, size: (f32, f32)) -> egui_kittest::Harness<'static, State> {
        let mut knap = KnapState::default();
        knap.journal.section = section;
        knap.journal.book_open = book;
        let state = State { w, knap, acts: Vec::new(), tab, selection, binds: Keybinds::default() };
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(size.0, size.1)).with_step_dt(1.0 / 60.0).build_ui_state(
            |ui, s: &mut State| {
                let ui_ctx = UiCtx {
                    hints: &[], build_mode: None, grid_snap: false, selection: s.selection, recording: false, recorded: &[], me: AgentId(0),
                    net_status: "", last_strike: None, tab: s.tab, binds: &s.binds, anchor: None, grip: false, time_scale: 1, evo: 9.0, yaw: 0.0, cave_tiles: None, surface: None,
                };
                crate::hud::draw(ui.ctx(), &s.w, &ui_ctx, &mut s.knap, &mut s.acts);
            },
            state,
        );
        h.step();
        h.step();
        h.step();
        h
    }

    fn world() -> World {
        let mut w = World::new(5);
        let (sx, sy) = w.spawn_mm;
        w.spawn_agent(sx, sy);
        w.learn_everything();
        w.agents[0].inventory.counts[ItemKind::Paper as usize] = 1;
        w.tick = alaz_sim::time::TICKS_PER_DAY * 6;
        w.demo_journal(6);
        w
    }

    #[test]
    fn j_opens_the_book_without_closing_the_window() {
        let mut h = harness(world(), Some(Tab::Bag), None, 0, false, (1920.0, 1080.0));
        assert!(h.query_by_label("Günler").is_none(), "the book starts closed");
        h.get_by_label("Günlük [J]").click();
        h.step();
        let acts = std::mem::take(&mut h.state_mut().acts);
        assert!(acts.iter().any(|a| matches!(a, UiAction::ToggleBook)), "the book button asks for the book");
        assert!(!acts.iter().any(|a| matches!(a, UiAction::ToggleTab(_) | UiAction::CloseWindow)), "and for nothing else");
        for a in acts {
            if matches!(a, UiAction::ToggleBook) {
                h.state_mut().knap.journal.toggle_book();
            }
        }
        h.run();
        assert_eq!(h.state().tab, Some(Tab::Bag), "the window keeps its tab");
        assert!(h.query_by_label("Günler").is_some(), "the book is open");
        assert!(crate::inv::regions(&h.ctx).is_some() && h.query_by_label("Çanta").is_some(), "the window is still there");
    }

    #[test]
    fn book_docks_left_on_wide_screens() {
        let h = harness(world(), Some(Tab::Bag), None, 0, true, (1920.0, 1080.0));
        let screen = h.ctx.content_rect();
        let book = rect(&h.ctx).expect("the book was drawn");
        let win = crate::inv::regions(&h.ctx).expect("the window was drawn").window;
        assert!(screen.contains_rect(book) && screen.contains_rect(win), "book {book:?} or window {win:?} off screen");
        assert!(book.right() <= win.left(), "book {book:?} does not sit left of the window {win:?}");
    }

    #[test]
    fn book_overlays_on_narrow_screens() {
        let h = harness(world(), Some(Tab::Bag), None, 0, true, (1280.0, 720.0));
        let screen = h.ctx.content_rect();
        let book = rect(&h.ctx).expect("the book was drawn");
        let win = crate::inv::regions(&h.ctx).expect("the window was drawn").window;
        assert!(screen.contains_rect(book), "book {book:?} off screen");
        assert!(screen.contains_rect(win), "the window {win:?} was pushed off screen");
        assert!(book.intersects(win), "narrow screens overlay the book on the window");
    }

    #[test]
    fn book_recipe_click_picks_it_on_the_open_machine() {
        let (w, furnace) = crate::uishot::demo_world();
        let (idx, name) = alaz_sim::rules().recipes.iter().enumerate().find(|(_, r)| r.station == Some(BuildingKind::Furnace) && w.knows(r.discovery)).map(|(i, r)| (i, r.name.clone())).expect("a furnace recipe");
        let mut h = harness(w, None, Some(Selection::Building(furnace)), 2, true, (1920.0, 1080.0));
        let book = rect(&h.ctx).expect("the book was drawn");
        let node = h.query_all_by_label(&name).find(|n| book.contains_rect(n.rect())).expect("the recipe is listed in the book");
        node.click();
        h.run();
        assert_eq!(crate::inv::info::recipe_pick(&h.ctx, furnace), Some(idx));
    }

    #[test]
    fn goals_open_the_next_discovery_without_revealing_locked_names() {
        let mut w = World::new(5);
        let (x, y) = w.spawn_mm;
        w.spawn_agent(x, y);
        let next = alaz_sim::journal::Progression::new(w.rules()).next(&w).expect("an early discovery");
        let next_name = crate::hud::discovery_name(next);
        let mut h = harness(w, Some(Tab::Goals), None, 1, false, (1280.0, 720.0));
        let locked = crate::hud::discovery_name(alaz_sim::Discovery::Computing);
        assert!(h.query_by_label(locked).is_none());
        h.get_by_label(next_name).click();
        h.run();
        assert!(h.state().knap.journal.book_open);
        assert!(h.query_by_label("Ağaca dön").is_some());
        h.get_by_label("Ağaca dön").click();
        h.run();
        assert!(h.query_all_by_label("?").next().is_some());
        assert!(h.query_by_label(locked).is_none());
    }

    #[test]
    fn known_discovery_links_to_its_recipe_page() {
        let w = world();
        let discovery = alaz_sim::Discovery::SharpStone;
        let recipe = w.rules().recipes.iter().find(|r| r.discovery == discovery).expect("sharp stone recipe").name.clone();
        let mut h = harness(w, None, None, 1, true, (1280.0, 720.0));
        h.state_mut().knap.journal.show_discovery(discovery);
        h.run();
        h.get(egui_kittest::kittest::By::new().label(&recipe).role(egui::Role::Link)).click();
        h.run();
        assert_eq!(h.state().knap.journal.section, 3);
    }

    /// The book (every page) and the camp window must stay fully on screen in small windows.
    #[test]
    fn journal_and_camp_fit_small_windows() {
        for size in [(800.0f32, 600.0f32), (1000.0, 720.0), (1280.0, 800.0)] {
            for section in 0..3u8 {
                let h = harness(world(), None, None, section, true, size);
                let screen = h.ctx.content_rect();
                let book = rect(&h.ctx).expect("the book was drawn");
                assert!(screen.contains_rect(book), "book {book:?} outside {screen:?}");
                let r = h.get_by_label("Defteri kapat").rect();
                assert!(screen.contains_rect(r), "book close button {r:?} outside {screen:?} (section {section})");
                let r = h.get_by_label(crate::journal_ui::SECTIONS[section as usize]).rect();
                assert!(screen.contains_rect(r), "book page button {r:?} outside {screen:?}");
            }
        }
        for size in [(800.0f32, 600.0f32), (1280.0, 800.0)] {
            let mut w = World::new(5);
            let (sx, sy) = w.spawn_mm;
            w.spawn_agent(sx, sy);
            w.learn_everything();
            w.tick = alaz_sim::time::tick_of_minute(19 * 60);
            w.buildings.push(alaz_sim::Building { fuel: 100_000, ..alaz_sim::Building::new(BuildingKind::Campfire, sx + 1_800, sy + 600) });
            w.step(&[Command { agent: AgentId(0), kind: CommandKind::Camp }]);
            assert!(w.agents[0].is_seated());
            let h = harness(w, None, None, 0, false, size);
            let screen = h.ctx.content_rect();
            let r = h.get_by_label("Kapat").rect();
            assert!(screen.contains_rect(r), "camp close button {r:?} outside {screen:?}");
            let r = h.get_by_label("Yarın için hedef").rect();
            assert!(screen.contains_rect(r), "camp goal list {r:?} outside {screen:?}");
        }
    }

    #[test]
    fn notebook_clues_live_in_the_recipes_page() {
        let mk = |seen: bool| {
            let mut w = World::new(5);
            let (sx, sy) = w.spawn_mm;
            w.spawn_agent(sx, sy);
            w.forget_everything();
            if seen {
                w.mark_seen(ItemKind::Stone);
            }
            harness(w, Some(Tab::Craft), None, 2, true, (1280.0, 800.0))
        };
        assert!(mk(false).query_by_label_contains("işe yarar gibi").is_none(), "nothing seen, no clue");
        assert!(mk(true).query_by_label_contains("işe yarar gibi").is_some(), "stones seen: the book nudges");
    }
}
