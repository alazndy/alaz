# Inventory window (Minecraft-style GUI): design

Date: 2026-11-18. Size: project.

## Intent
Machines (workbench, furnace, kiln, …) today open a text-heavy panel in the top-right corner. The bag, craft and
build tabs are separate side panels made of text and buttons. The user wants a Minecraft-like GUI: one window with
item slots, an arrow that shows progress, and the bag always visible below, so items can be moved by hand between
the bag and a machine (or the hands, for crafting). The point is the feel of handling items yourself, not only a
new look.

## Goals
- One centered window with tabs on top: **Çanta**, **Yap**, **İnşa**, and **the machine** when one is open.
  The bag grid is always shown in the lower half, whichever tab is on top.
- Machine tab: input slots, a progress arrow with percent, output slots, fuel (burning buildings) and heat
  (heat_max > 0) where they apply, the recipe chooser with "Başlat" and "Otomatik", and the building's notes.
- Moving items:
  - Drag a bag slot onto a machine input area → load that item. Click = 1, Shift+click = all (up to what fits).
  - Click or drag an output slot into the bag → collect that item (existing `Collect { item: Some(k) }`).
  - Drag a bag slot onto a hand slot in **Yap** → it goes into the hands (UI state `hands: [u8; 4]`, as today).
  - Drag a bag slot outside the window → drop it on the ground (existing `Drop`), at most 10 per drop, as today.
- Keys: I opens the window on Çanta, C on Yap, B on İnşa (existing bindings `Action::Bag/Craft/Build`).
  The same key again closes it. Clicking a machine opens the window on its tab.
- The machine tab closes by itself when the player is farther than the use radius (3 m, `USE_RADIUS_MM`).
- Look: the existing era pixel panels (`pxpanel`, `pxui`), item icons (`icons::draw_slot`), fonts and colours.

## Non-goals
- No change to recipes, jobs, station capacity (`STATION_INPUT_CAP`), or discovery.
- No Minecraft "shape" crafting grid: Yap keeps the 4 hands + "Dene" discovery mechanic.
- Goals, Journal, Help, Routes, Creative tabs, worker panel, knapping/rubbing mini-games, cave HUD: unchanged.
- No 3D thumbnails of buildings (see "Left to the builder").

## Anti-goals
- The window must not hide the knapping mini-games: while `w.craft_view(me)` is Some the window steps aside, as
  the tab panel does today.
- A click must never send a command the sim rejects silently in a way the player cannot see; disabled slots look
  disabled.
- Multiplayer must stay in lockstep: every item move is a sim command, never a client-side change.

## Constraints
- Rust, egui 0.37 (`dnd_drag_source` / `dnd_drop_zone` available), egui_kittest for UI tests.
- The sim is deterministic and integer-based. Commands are serialized with postcard; adding a `CommandKind`
  variant changes the wire format, so `alaz_net::PROTOCOL` goes from 1 to 2.
- Append the new variant at the end of `CommandKind`, so old saves and recorded scripts keep their variant indices.
- `crates/game/src/hud.rs` is 4 100 lines; new UI code goes in a new file, not into hud.rs.
- A parallel session edits physics and rendering; do not touch `crates/render` or physics code.

## Approaches
- **Picked: one window built from shared widgets** in a new `crates/game/src/inv_window.rs`
  (slot, slot grid, drag payload, drop zones), using egui's own drag and drop. Bag, Yap, İnşa and machine tabs are
  drawn with these widgets.
- Rejected: re-skin the existing separate panels with slots. The user chose one window.
- Rejected: hand-written drag and drop. egui already has it; writing it again only adds risk.

## Decided
### Sim (crates/sim)
- New command, appended last: `CommandKind::Load { building: u32, item: ItemKind, n: u16 }`.
  - Same distance check and `Reject::TooFar` as `Deposit`.
  - Same acceptance rules as `deposit_into`, limited to `item` and at most `n`: depots/cellars/coolers take any
    non-tool item; fields take manure/fertilizer; stations take an item only if one of their recipes (or alloy)
    uses it, up to `STATION_INPUT_CAP`.
  - Refactor `deposit_into` into one function with an optional `(item, n)` limit, so `Deposit` and `Load` share it.
  - Partial loads are fine: `Load` moves `min(n, have, room)`. Nothing to move is a silent no-op, as `Deposit` is.
  - Export the acceptance rule from the sim as `pub fn load_room(rules, b: &Building, item) -> u16` (0 = not
    taken). The UI uses it to disable and highlight drop targets, so it never mirrors the rule and never sends a
    `Load` the sim would ignore. No new `Reject` variant.
  - Not allowed in caves: add it to the same list in `world/cave_play.rs` as `Deposit`.
- Fuel (my call): the fuel slot of a burning building is "fill the fire". A wood drop or click sends one existing
  `Refuel`, which already takes as much wood as fits. No count, no new command. The slot is disabled when the
  fire is out (`fuel == 0`) and the bag has no flint (the sim would reject with `NoFlint`).
- `alaz_net::PROTOCOL = 2`. Discovery shares it, so 0.4.x and the new version neither find nor join each other.
  That is accepted: players install the same release. Saves store no protocol number and no commands; debug
  scripts are text and unaffected.

### Game UI (crates/game)
- `inv_window.rs`: the window, its tab row, and widgets:
  - `slot(ui, w, kind, n, state) -> Response` (reuses `icons::draw_slot`; empty, normal, disabled, highlighted).
  - a drag payload type `Carry { from: From, item: ItemKind, n: u16 }` with `From::{Bag, Output(building), Hand(i)}`.
  - drop zones: bag, machine input, machine fuel, hand slots, "outside the window".
- Window state replaces the bag/craft/build entries of `Tab`: `WindowTab::{Bag, Craft, Build, Machine(usize)}`.
  The other `Tab` values keep their current panels.
- Selecting a building (`Selection::Building(i)`) that is a station, depot, cellar, cooler, field, or burns opens
  the window on `Machine(i)`. Other buildings (shelter, railway, telegraph, …) keep today's `draw_building` panel.
- The machine tab shows everything the old panel showed for that building (notes, warnings, requirement lines),
  under the slots, in smaller text.
- Çanta tab (top half): load meter, tools with wear bars and "Onar", and the help line. Bag grid stays below.
- Yap tab: 4 hand slots in a 2×2 grid, an arrow, the "Dene" button, and the known-recipe hints the craft tab shows
  today.
- İnşa tab: the same groups as today (`BUILD_GROUPS`), each as a grid of tiles (group glyph + name, green frame
  when it can be built now); clicking a tile shows its cost, needs and notes on the right with "Yerleştir".
- Window size: up to 760×560, shrinks with the screen (same limits idea as today's tab panel).

### Machine tab layouts (my call)
- Station (has recipes): input grid → arrow with job percent → output grid; fuel and heat when they apply.
- Storage (depot, cellar, cooler): one storage grid over `b.output`, no arrow. Drop in = `Load`, click = `Collect`.
- Field: input grid (manure, fertilizer) only, plus today's soil and season lines.
- Clicking a filled output/storage slot sends `Collect { item: Some(k) }` (whole stack, limited by capacity, as
  today). Dragging it onto the bag does the same.
- Bag slot in a machine tab: click = `Load n=1`, Shift+click = `Load n=all`, drag = `Load n=all`.
- Bag to hand (Yap) is UI-only: it edits `hands`, no command.

### Old code (my call)
- Delete `tab_bag`, `tab_craft`, `tab_build` and the station/storage/field parts of `draw_building` once the window
  replaces them. Git history is the rollback.
- `Tab::{Bag, Craft, Build}` become the window's tabs. Update `main.rs` (`ALAZ_DEMO_TAB`, `toggle_tab`) and migrate
  the existing hud tests that click the old Yap/Dene buttons to the new window.

### Tests
- Sim: `Load` loads only the named item, respects `n`, `STATION_INPUT_CAP`, distance, and refuses unknown items;
  `Deposit` behaves exactly as before (existing tests stay green).
- Net: existing lockstep tests pass with PROTOCOL 2.
- UI (egui_kittest): the window opens on each tab; Shift+click on a bag slot in a machine tab produces
  `Cmd(Load { n: all })`; click on an output slot produces `Collect { item: Some(k) }`; dragging is covered by
  calling the drop handler directly (kittest drag support is limited).

## Left to the builder
- Exact pixel layout and sizes, within the existing panel style.
- Building tiles use a group glyph plus the name. Real building pictures are a later change.
- Hover tooltips reuse `item_tip`.
- README: a short "Envanter penceresi" note in the controls part; `packaging/RELEASE_NOTES.md` gets the change
  when it ships.
