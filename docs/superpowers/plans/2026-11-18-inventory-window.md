# Inventory Window Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One Minecraft-style window (Çanta / Yap / İnşa / machine tabs over an always-visible bag grid) with drag-and-drop item moves.

**Architecture:** The sim gets one command, `Load`, and one query, `load_room`. A new module `crates/game/src/inv/` holds the
window, the shared slot widgets and one file per tab. `hud.rs` stops drawing the old bag/craft/build panels and the
station/storage/field part of `draw_building`, and calls `inv::draw_window` instead.

**Tech Stack:** Rust workspace, egui 0.37 (`Response::dnd_set_drag_payload`, `dnd_release_payload`, `egui::DragAndDrop`), egui_kittest, postcard.

**Spec:** `docs/superpowers/specs/2026-11-18-inventory-window-design.md`

## Global Constraints
- Work only in the worktree `/home/alaz/alaz-invwin` (branch `feat/inventory-window`) or the task worktree you are given. Never touch `/home/alaz/alaz`.
- Do not touch `crates/render` or physics code.
- New `CommandKind` variant goes **last** in the enum. `alaz_net::PROTOCOL = 2`.
- User text Turkish; code, comments, commits English. Comments only for the "why". No TODOs.
- New UI code lives in `crates/game/src/inv/`, not in `hud.rs`.
- Use the era panel style (`hud::panel`, `pxui`, `pxpanel`) and `icons::draw_slot`; tooltips reuse `hud::item_tip`.
- Agents do not commit. The orchestrator reviews and commits.
- Before reporting: `cargo test -p <touched crates>` green and `cargo clippy -p <crate> --all-targets` with no new warnings in touched files (pre-existing `approx_constant` errors in icons.rs/menu.rs are known).

## Review Focus
1. Bag item the open machine does not take: slot drawn disabled, click/drag sends nothing. (Task 5 test `machine_ignores_items_it_does_not_take`.)
2. Machine selected from far away: slots disabled, note "Kullanmak için yanına git (3 m)."; window closes when the player walks from ≤3 m to >3 m. (Task 5 test `far_machine_slots_are_disabled` + Task 5 main.rs check.)
3. Station input full (`STATION_INPUT_CAP`): `Load` moves only what fits, rest stays in bag. (Task 1 test `load_respects_station_cap`.)
4. Hands holding an item that leaves the bag (eaten, loaded into a machine): hand empties itself, as today. (Task 3 migrated test `hands_let_go_of_what_left_the_bag`.)
5. Knapping mini-game active: window is hidden (anti-goal). (Task 2 test `window_steps_aside_for_the_workpiece`.)

---

### Task 1: Sim `Load` command + `load_room` — model **Sonnet**

**Files:** Modify `crates/sim/src/command.rs`, `crates/sim/src/world.rs` (~1116 `deposit_into`, ~2531 `Deposit` arm), `crates/sim/src/world/cave_play.rs` (~47 list), `crates/net/src/lib.rs` (`PROTOCOL`). Tests in `crates/sim/src/tests.rs`.

**Produces:**
```rust
// command.rs, appended LAST in CommandKind:
/// Put up to `n` of one item into a building (same rules as Deposit, one item only).
Load { building: u32, item: ItemKind, n: u16 },
// world.rs (pub, re-exported from lib.rs like other world fns):
pub fn load_room(rules: &Rules, b: &Building, item: ItemKind) -> u16; // 0 = not taken
```
- `deposit_into(rules, inv, b, only: Option<(ItemKind, u16)>)`: one function; `Deposit` passes `None`.
- `load_room`: storage (Depot/Cellar/Cooler) → `u16::MAX` for non-tools, 0 for tools; Field → `u16::MAX` for Manure/Fertilizer; station → `STATION_INPUT_CAP - b.input[item]` if any recipe with `station == Some(b.kind)` uses it (inputs or alloy base/add), else 0.
- `Load`: distance check → `Reject::TooFar`; moves `min(n, have, load_room)`.

- [ ] **Step 1: Failing tests** in `crates/sim/src/tests.rs`:
  - `load_moves_only_the_named_item`: agent next to a Furnace (pick a station whose recipe uses CopperOre) with 5 CopperOre + 5 Wood (wood not an input); `Load { item: CopperOre, n: 3 }` → bag CopperOre 2, `b.input[CopperOre] == 3`, wood untouched.
  - `load_respects_station_cap`: bag has `STATION_INPUT_CAP + 5`; `Load n = u16::MAX` → input == cap, bag keeps 5.
  - `load_refuses_from_far_away`: agent 5 m away → nothing moves, last reject is `TooFar`.
  - `load_into_depot_takes_any_non_tool`: 4 Berries → `b.output[Berries] == 4`; a tool item → `load_room == 0`, nothing moves.
  - `load_room_matches_deposit`: for every `BuildingKind` that has a recipe and every item, `load_room > 0` ⇔ `Deposit` would move that item (build the expectation by running `Deposit` on a clone).
- [ ] **Step 2:** `cargo test -p alaz-sim load_` → FAIL (no variant).
- [ ] **Step 3:** Implement; add `Load { .. }` to the cave_play list next to `Deposit`; set `PROTOCOL = 2`.
- [ ] **Step 4:** `cargo test -p alaz-sim -p alaz-net` all green (existing Deposit tests unchanged).
- [ ] **Step 5:** Report.

### Task 2: Window shell, widgets, Çanta tab — model **Opus** (parallel with Task 1)

**Files:** Create `crates/game/src/inv/mod.rs`, `crates/game/src/inv/bag.rs`. Modify `crates/game/src/main.rs` (`mod inv;`, `UiAction::CloseWindow` handling), `crates/game/src/hud.rs` (call site ~2037 tab panel; delete `tab_bag`; add `UiAction::CloseWindow`; make `item_tip`, `panel`, `slot_cell` helpers `pub(crate)` as needed).

**Produces (later tasks rely on these exact names):**
```rust
// inv/mod.rs
pub(crate) enum Carry { Bag(ItemKind), Output { building: usize, item: ItemKind }, Hand(usize) }
pub(crate) enum SlotLook { Normal, Empty, Disabled, Target }
pub(crate) struct Cx<'a> { pub w: &'a World, pub me: &'a Agent, pub ui_ctx: &'a hud::UiCtx<'a>,
                           pub hands: &'a mut [u8; 4], pub acts: &'a mut Vec<hud::UiAction> }
pub(crate) enum Top { Bag, Craft, Build, Machine(usize) }
/// 64×64 slot; item None = empty. Hover shows item_tip.
pub(crate) fn slot(ui: &mut Ui, w: &World, item: Option<ItemKind>, n: u16, look: SlotLook) -> Response;
/// Makes `resp` a drag source carrying `c`.
pub(crate) fn drag_source(resp: &Response, c: Carry);
/// The Carry released on `resp` this frame, if any.
pub(crate) fn dropped_on(resp: &Response) -> Option<Carry>;
/// Called by the window for every bag slot; each top tab decides what a click means.
pub(crate) type BagClick<'f> = &'f mut dyn FnMut(&mut Cx, ItemKind, bool /*shift*/);
pub(crate) fn draw_window(ctx: &Context, cx: &mut Cx);   // decides Top, draws tab row, top, bag grid
// tab entry points, one per file (Task 2 creates craft/build/machine as stubs that render the OLD content):
// inv/bag.rs:     pub(crate) fn draw(ui: &mut Ui, cx: &mut Cx); pub(crate) fn click(cx: &mut Cx, k: ItemKind, shift: bool)
// inv/craft.rs:   same two fns      inv/build.rs: draw only (click = no-op)
// inv/machine.rs: pub(crate) fn draw(ui, cx, i: usize); pub(crate) fn click(cx, i, k, shift); pub(crate) fn opens_window(w: &World, kind: BuildingKind) -> bool
```
- `Top` choice: `ui_ctx.tab` ∈ {Bag, Craft, Build} → that; else selected building `i` with `machine::opens_window` → `Machine(i)`; else no window. Tab row shows Çanta / Yap / İnşa and the machine's name when one is selected; clicking a tab pushes `ToggleTab`, clicking the machine tab pushes `ToggleTab(current)` (clears tab → machine shows). A close button pushes `CloseWindow` (main.rs: `tab = None; selected = None`).
- Bag grid: same grouping as old `tab_bag` (`lang::looks_like`, `hidden_in`); 10 columns; each slot is a drag source `Carry::Bag(real)`; click → current tab's `click`; right-click keeps the old "1 tane / N tane bırak" menu.
- Drop outside: if a `Carry::Bag(k)` is released while the pointer is outside the window rect → `Drop { item: k, n: min(10, have) }`.
- Window: anchored center, width ≤ 760, height ≤ 560, shrinks like today's tab panel; hidden while `w.craft_view(me)` is Some.
- Çanta top: load meter + tools with wear and "Onar" (moved from old `tab_bag`).
- Stubs: `craft::draw` calls the old `hud::tab_craft`, `build::draw` calls `hud::tab_build`, `machine::draw` calls the old `draw_building` body (make them `pub(crate)`); `machine::opens_window` returns true for stations (any recipe with that station), Depot/Cellar/Cooler, Field, and `kind.burns()`. When `Top::Machine`, hud must not also draw the old right-top building panel.

- [ ] **Step 1: Failing kittest tests** in `inv/mod.rs` (`build_ui_state` like hud tests ~3509):
  - `window_opens_on_each_tab`: for `Tab::Bag`, `Tab::Craft`, `Tab::Build` the label "Çanta", "Yap", "İnşa" tab buttons exist and the bag grid shows a slot labeled with an item in the bag ("Taş").
  - `window_steps_aside_for_the_workpiece`: with a knapping job running (reuse the setup of `a_player_can_combine_two_stones_with_the_mouse_and_play_the_knapping`), no "Çanta" tab label.
  - `dragging_a_bag_item_outside_drops_it`: call the drop-outside handler (`fn drop_outside(cx, Carry)`, pub(crate) for tests) → `Cmd(Drop { item: Stone, n: min(10, have) })`.
  - `close_button_closes`: click "Kapat" → `UiAction::CloseWindow` pushed.
- [ ] **Step 2:** `cargo test -p alaz inv::` → FAIL.
- [ ] **Step 3:** Implement; delete `tab_bag`; keep all other hud tests green.
- [ ] **Step 4:** `cargo test -p alaz` all green (record count); clippy clean for new files.
- [ ] **Step 5:** Report.

### Task 3: Yap tab — model **Sonnet** (after Task 2, own worktree)

**Files:** `crates/game/src/inv/craft.rs`; delete `tab_craft` from `hud.rs`; move the craft tests of `hud.rs` (~3500–3780 that use `Tab::Craft`) to work against the window.
- Layout: 4 hand slots as 2×2, arrow, "Dene" button (`Cmd(Combine { items: *hands })`, enabled when any hand is filled), then the known-recipe hints the old tab showed.
- Hand slot: drop `Carry::Bag(k)` → `hands[i] = k as u8` (UI only); click a filled hand → empties it (`hands[i] = 255`), as today; filled hand is a drag source `Carry::Hand(i)`, dropping it on the bag empties it.
- `click(cx, k, shift)`: put `k` into the first empty hand (shift: fill all empty hands with `k` while the bag has enough).
- Keep the old rule: hands let go of items no longer in the bag.
- [ ] **Step 1:** Make the migrated tests fail-first where behaviour is new: `bag_click_fills_the_first_empty_hand`, `dropping_a_bag_item_on_a_hand_holds_it` (call `dropped_on` path via a pub(crate) `fn drop_on_hand(cx, i, Carry)`).
- [ ] **Step 2–4:** red → implement → `cargo test -p alaz` green, including the migrated `a_stone_axe_can_be_made_start_to_finish_with_the_mouse`, `clicking_a_filled_hand_slot_empties_it`, `hands_let_go_of_what_left_the_bag`.
- [ ] **Step 5:** Report.

### Task 4: İnşa tab — model **Sonnet** (after Task 2, own worktree, parallel with 3 and 5)

**Files:** `crates/game/src/inv/build.rs`; delete `tab_build` from `hud.rs` (keep `BUILD_GROUPS`, `build_group`, `need_row`, `needs_row`, `machine_note`, `requirement_note` — make them `pub(crate)`).
- Left: per group (`BUILD_GROUPS`) a header and a tile grid (72×72: group glyph + name; green frame when `cost_for(...)` and `has_needs` pass; dim when not). Right: the selected tile's cost rows, needs, notes, "Yerleştir" (`UiAction::BeginBuild(kind)`), plus the grid-snap toggle and build-mode line from the old tab. Selected tile is UI state in `egui::Memory` (id `"build_pick"`).
- [ ] **Step 1:** Failing tests: `build_tab_shows_a_tile_per_known_building` (world with Fire + Shelter known → tiles "Kamp ateşi"/"Barınak" exist — use the names from rules), `placing_from_the_build_tab_begins_build_mode` (click tile, click "Yerleştir" → `BeginBuild(kind)`).
- [ ] **Step 2–4:** red → implement → `cargo test -p alaz` green.
- [ ] **Step 5:** Report.

### Task 5: Machine tab — model **Opus** (after Tasks 1 and 2, own worktree, parallel with 3 and 4)

**Files:** `crates/game/src/inv/machine.rs`; `crates/game/src/hud.rs` (remove from `draw_building` everything now drawn by the machine tab; `draw_building` stays for buildings where `opens_window` is false); `crates/game/src/main.rs` (auto-close).
**Consumes:** `alaz_sim::load_room`, `CommandKind::Load` (Task 1); `Cx`, `Carry`, `slot`, `drag_source`, `dropped_on` (Task 2).
- Layouts (spec "Machine tab layouts"): station = input grid → arrow with job percent (from `b.job`) → output grid, fuel bar + slot when `kind.burns()`, heat line when `heat_max > 0`, recipe list with "Başlat"/"Otomatik" (moved from old panel); storage = one grid over `b.output`; field = input grid + soil/season lines. All the building's notes/warnings from the old panel go under the slots in small text. Header keeps "Döndür" and "Taşı".
- Input zone accepts `Carry::Bag(k)` when `load_room(..) > 0` → `Load { n: u16::MAX }`; looks `Target` while such a drag hovers, `Disabled` for items with room 0.
- Output/storage slot: click or drag onto bag → `Collect { item: Some(k) }`.
- Fuel slot: drop/click Wood → one `Refuel`; disabled when `fuel == 0` and bag has no Flint, or bag has no Wood.
- `click(cx, i, k, shift)`: `Load { n: if shift { u16::MAX } else { 1 } }` when room > 0, else nothing.
- Far away (> `USE_RADIUS_MM` from the player): all slots Disabled, note "Kullanmak için yanına git (3 m)."
- main.rs: keep `machine_near: bool` for the selected building; when it goes true → false, set `selected = None` (window closes).
- [ ] **Step 1: Failing tests** in `inv/machine.rs`:
  - `shift_click_loads_all`: furnace near, 4 CopperOre, `click(.., shift=true)` → `Cmd(Load { item: CopperOre, n: u16::MAX })`.
  - `plain_click_loads_one`: same, shift=false → `n: 1`.
  - `machine_ignores_items_it_does_not_take`: bag Berries at a furnace → `click` pushes nothing.
  - `clicking_output_collects_it`: `b.output[CopperIngot or the recipe's output] = 2`, click that slot in the harness → `Cmd(Collect { item: Some(k) })`.
  - `far_machine_slots_are_disabled`: player 10 m away → label "Kullanmak için yanına git (3 m)." present and `click` pushes nothing.
  - `depot_shows_storage_grid`: depot with 3 Berries in output → a slot with "Yemiş" and no "Başlat".
- [ ] **Step 2–4:** red → implement → `cargo test -p alaz -p alaz-sim` green.
- [ ] **Step 5:** Report.

### Task 6: Docs and sweep — model **Haiku** (after merge of 3–5)
- README: under the controls part add "**Envanter penceresi:** I/C/B ya da bir makineye tıkla. Eşyayı çantadan sürükleyip makineye ya da ellere bırak; tık = 1, Shift+tık = hepsi; pencerenin dışına sürükle = yere bırak." 
- Run `cargo test --workspace 2>&1 | grep -E "test result|FAILED|panicked"` and report the summed counts and failing names. Read-only besides README.
