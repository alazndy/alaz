# UI Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Re-draw the inventory window and a separate book window in A.L.A.Z's own pixel style: era material, machine drawings with slots on them, recipe strip, info panel whose detail grows with perception.

**Architecture:** A new `crates/game/src/style/` module owns everything visual (era palettes, pixel frames, slots, icons, machine-family drawings, the info-line renderer). `crates/game/src/inv/info.rs` turns game data into info lines by perception level. The `inv/` tabs and a new `book.rs` window use both. A `--ui-shot` dev command renders windows to PNG offscreen so agents can see their work.

**Tech Stack:** Rust, egui 0.37 painter (no new crates), egui_kittest, the software rasterizer pattern in `crates/game/src/export.rs`.

**Spec:** `docs/superpowers/specs/2026-11-19-ui-redesign-design.md`

## Global Constraints
- Branch `feat/ui-redesign`; each task works in its own worktree `/home/alaz/alaz-ui-tN` (branch `ui-tN`) made by the orchestrator. Never touch `/home/alaz/alaz` or other worktrees.
- Pixel art: draw on the UI pixel grid (`pxui::unit()`, `pxui::snap_rect`), hard corners, stepped curves, flat colour bands. No smooth gradients, no rounded rects, no emoji or font glyphs as icons.
- Fonts: Liberation Serif everywhere; Liberation Mono only for the computer-era data block (add the font file under `crates/game/assets/`, OFL licence noted in `FONTS.md`).
- Body text ≥ 22 px; 11 px only for slot counts. Panels fully opaque.
- Do not change sim rules, commands, network protocol, or the HUD outside the inventory and book windows.
- Never invent numbers: only `RecipeDef.heat`, `RecipeDef.ticks`, `Building.temp`, `b.job`, `b.auto`, recipe inputs/outputs. No efficiency.
- Build and test on titan: `/home/alaz/bin/titan-cargo <worktree> test --release -p alaz` and `... clippy -p alaz --all-targets`. Do not run cargo locally.
- User text Turkish; code/comments/commits English; comments only for "why"; no TODOs. Agents do not commit.

## Review Focus
1. Era 0 player must see no numbers and no long words in the info panel. (Task 2 test `stone_age_recipe_has_no_text_or_numbers`.)
2. Window on a 1280×720 screen: nothing clipped, text not below 22 px. (Task 1 test `window_fits_720p`; Task 6 re-runs it with all tabs.)
3. A machine with an unknown recipe: strip shows "?", info panel says nothing about it. (Task 5 test `unknown_recipe_is_a_question_mark`.)
4. Book and inventory both open on 1280 width: book overlays, does not push the window off screen. (Task 3 test `book_overlays_on_narrow_screens`.)
5. Existing inventory behaviour (load/collect/fuel/drag/hands/build) unchanged. (All existing `inv::` tests must stay green in every task.)

---

### Task 1: Style foundation, window shell, `--ui-shot` — **Opus** (wave 1, parallel with Task 2)

**Files:** Create `crates/game/src/style/{mod.rs,icons.rs,lines.rs}`, `crates/game/src/uishot.rs`; modify `inv/mod.rs` (frame, sizes, tab row, slot drawing), `main.rs` (`mod style; mod uishot;`, `--ui-shot <dir>` flag), `hud.rs` only to honour `ALAZ_DEMO_ERA` where `ERA` is set.

**Produces:**
```rust
// style/mod.rs
#[derive(Clone, Copy, PartialEq, Eq, Debug)] pub(crate) enum Era { Stone, Metal, Machine, Computer }
pub(crate) fn era_of(perception: u8) -> Era;          // 0-1 Stone, 2-3 Metal, 4 Machine, 5+ Computer
pub(crate) fn current_era() -> Era;                    // hud::ERA, overridden by ALAZ_DEMO_ERA=0..5
#[derive(Clone, Copy)] pub(crate) struct Palette { pub base: Color32, pub base_dark: Color32, pub light: Color32,
    pub edge: Color32, pub slot: Color32, pub slot_shadow: Color32, pub ink: Color32, pub ink_dim: Color32,
    pub good: Color32, pub warn: Color32 }
pub(crate) fn palette(era: Era) -> Palette;
pub(crate) fn frame(p: &Painter, r: Rect, era: Era, accent: Option<Color32>);   // hide+bark / rivets / blueprint / screen
pub(crate) fn slot_frame(p: &Painter, r: Rect, era: Era, look: crate::inv::SlotLook);
#[derive(Clone, Copy, PartialEq, Eq, Debug)] pub(crate) enum Family { Fire, Kiln, Furnace, Smithing, Grinding,
    Bench, Textile, MachineTool, Storage, Growing, Electric }
pub(crate) fn family(kind: BuildingKind) -> Family;   // table from the spec; unlisted → Bench
pub(crate) fn accent(f: Family) -> Color32;
// style/icons.rs
#[derive(Clone, Copy, PartialEq, Eq, Debug)] pub(crate) enum Icon { Flame, Hourglass, Arrow, Play, Auto, Question,
    Bag, Hammer, House, Book, Close, Thermo, Clock, Warn }
pub(crate) fn icon(p: &Painter, r: Rect, icon: Icon, col: Color32);              // pixel-drawn
pub(crate) fn icon_button(ui: &mut Ui, icon: Icon, tip: &str, selected: bool, era: Era) -> Response;
// style/lines.rs  (Task 2 produces these, Task 1 renders them)
pub(crate) enum Picto { Item(ItemKind), Icon(Icon), Flames { lit: u8, of: u8 }, Glasses { lit: u8, of: u8 } }
pub(crate) enum Chip { Item(ItemKind, u16), HeatC { need: i16, now: Option<i16> }, HeatWord(&'static str), Secs(u32), PerMin(f32) }
pub(crate) enum Line { Title(String), Label(&'static str), Pictos(Vec<Picto>), Chips(Vec<Chip>), Text(String),
    Mono(Vec<String>), Warn(String) }
pub(crate) fn draw_lines(ui: &mut Ui, w: &World, era: Era, lines: &[Line]);
```
- Window: opaque `frame`, header (machine family icon + name, or tab icon + word) and `Icon::Close`; tab row = `icon_button`s (Bag/Hammer/House + machine family accent), tab word as tooltip. Size: `min(1100, screen−40) × min(720, screen−80)`, never under the 900×620 target when the screen allows. Bag slots 56 px, columns fit the width. Layout regions reserved for later tasks: `top_left` (machine/craft/build area), `right` (info panel, 260 px), `bottom` (bag). Expose `pub(crate) struct Regions { pub main: Rect, pub info: Rect, pub bag: Rect }` from `draw_window` to tabs via `Cx`-adjacent argument (tabs keep their `draw(ui, cx)` signature; add `pub(crate) fn info_rect(ui) -> Rect` helper).
- `uishot.rs`: `alaz --ui-shot <dir>` builds a demo world (furnace, campfire, depot, bag items, known recipes) and renders, for each era 0..5 mapped to the 4 looks, the window on tabs Bag/Craft/Build/Machine(furnace) at 1280×800, into `<dir>/<era>-<tab>.png`. Reuse the software rasterizer from `export.rs` and add font-atlas sampling (apply `textures_delta` for `TextureId::default()` into an RGBA buffer and sample it), so text shows.
- [ ] **Step 1: Failing tests** (`style/mod.rs`, `inv/mod.rs`):
  - `era_mapping`: `era_of(0)==Stone, era_of(1)==Stone, era_of(2)==Metal, era_of(3)==Metal, era_of(4)==Machine, era_of(5)==Computer`.
  - `every_window_building_has_a_family`: for every `BuildingKind` where `machine::opens_window` is true, `family(kind)` returns without panic and the spec's examples map right (`Furnace→Furnace, HandMill→Grinding, Loom→Textile, Depot→Storage, Field→Growing, Campfire→Fire`).
  - `window_fits_720p`: harness at 1280×720 with Tab::Bag → window rect inside the screen; the bag grid shows ≥ 8 columns.
  - `uishot_writes_pngs`: run the shot function into a temp dir for era 0 and the Bag tab → file exists, decodes as 1280×800 PNG, and is not a single colour.
- [ ] **Step 2:** titan test → FAIL. **Step 3:** implement. **Step 4:** titan test `-p alaz` all green (existing inv tests too). Run `--ui-shot` on titan, copy 4 PNGs back (`scp titan:...`), **look at them with the Read tool** and fix anything unreadable.
- [ ] **Step 5:** Report with the PNG paths.

### Task 2: Info lines by perception — **Sonnet** (wave 1, parallel with Task 1; logic only, no drawing)

**Files:** Create `crates/game/src/inv/info.rs` (+ `mod info;`). If Task 1's `style/lines.rs` is not in your tree, create it with exactly the `Picto/Chip/Line` types above (no `draw_lines`), the orchestrator merges.

**Produces:**
```rust
#[derive(Clone, Copy, PartialEq, Debug)] pub(crate) enum Subject { Item(ItemKind), Recipe(usize), Machine(usize) }
pub(crate) fn describe(w: &World, s: Subject, level: u8) -> Vec<Line>;   // Recipe(i) = index into w.rules().recipes
pub(crate) fn recipe_pick(ctx: &Context, building: usize) -> Option<usize>;
pub(crate) fn set_recipe_pick(ctx: &Context, building: usize, recipe: usize);
pub(crate) fn default_pick(w: &World, building: usize) -> Option<usize>;  // b.auto, else first known recipe of the station
pub(crate) fn station_recipes(w: &World, kind: BuildingKind) -> Vec<(usize, bool /*known*/)>;
```
Content per spec "Info panel content by perception": 0–1 Pictos only; 2–3 Title + item-name Chips/Text + `hud::heat_word` + one note (`hud::machine_note` / `requirement_missing`); 4 adds `HeatC`, `Secs(ticks / TICKS_PER_SECOND)`, `PerMin(outputs_per_job * 60 / secs)`; 5 = `Mono` block of the level-4 data plus `auto: ON/OFF`. Machine subject at 4–5 includes current temp and job progress %.
- [ ] **Step 1: Failing tests:** `stone_age_recipe_has_no_text_or_numbers` (level 0: no `Text`, `Chips` or `Mono`; only `Pictos`), `metal_age_names_inputs_and_heat_word`, `machine_age_shows_degrees_and_seconds` (contains `HeatC{need: r.heat}` and `Secs`), `computer_age_is_mono_and_has_auto_state`, `per_min_matches_ticks` (recipe with ticks = 40 s and 1 output → 1.5), `default_pick_prefers_auto`, `unknown_recipes_are_flagged` (station_recipes marks unknown false).
- [ ] **Step 2–4:** red → implement → titan `test --release -p alaz` green.
- [ ] **Step 5:** Report.

### Task 3: Book window — **Sonnet** (wave 2, after 1 and 2)

**Files:** Create `crates/game/src/book.rs`; modify `main.rs` (`book_open: bool`, `Action::Journal` toggles it), `hud.rs` (remove the Journal tab button and `Tab::Journal` drawing; keep `JournalState`), `journal_ui.rs` (make section fns callable from the book), `inv/craft.rs` (remove `notebook`, its content moves to the book's "Tarifler" page).
- `book::draw(ctx, w, me, st: &mut JournalState, inv_open: Option<Rect>, acts)`: era material via `style::frame` (stone = parchment colours, metal = bound book, machine = ledger, computer = screen); pages as `icon_button`s; docked left of `inv_open` when `screen_w − inv.width ≥ book_w + 24`, else centred overlay above the window. Clicking a known recipe whose station is the open machine calls `info::set_recipe_pick`.
- [ ] **Step 1: Failing tests:** `j_opens_the_book_without_closing_the_window`, `book_docks_left_on_wide_screens` (1920×1080), `book_overlays_on_narrow_screens` (1280×720), `book_recipe_click_picks_it_on_the_open_machine`, and move `journal_and_camp_fit_small_windows` to the book.
- [ ] **Step 2–5:** red → implement → titan green → `--ui-shot` adds `<era>-book.png`, look at them → report.

### Task 4: Machine family drawings — **Sonnet** (wave 2, after 1)

**Files:** Create `crates/game/src/style/machines.rs`.
```rust
pub(crate) struct MachineState { pub fuel: f32 /*0..1*/, pub glow: f32 /*0..1 from temp*/, pub working: bool, pub blocked: bool, pub t: f32 /*seconds, for motion*/ }
pub(crate) struct Anchors { pub input: Rect, pub fuel: Option<Rect>, pub output: Rect, pub progress: Rect }
pub(crate) fn draw_machine(p: &Painter, r: Rect, fam: Family, era: Era, st: &MachineState) -> Anchors;
```
- One pixel drawing per family (spec table), material tinted by era, accent by family; flame size from `fuel`, glow from `glow`, a moving part when `working`, red pixel outline when `blocked`. `Anchors` say where the machine tab puts its slots (input at the mouth/hopper, fuel under the fire, output beside).
- [ ] **Step 1: Failing tests:** `anchors_lie_inside_the_drawing` (every family × era: all anchor rects inside `r`, input/output not overlapping), `fuel_anchor_only_for_burning_families` (Fire/Kiln/Furnace have `Some`, Storage/Textile `None`).
- [ ] **Step 2–5:** red → implement → titan green → add a `--ui-shot` page `machines-<era>.png` (all 11 families on one sheet), look at all four, fix ugly or unreadable ones → report.

### Task 5: Machine tab in the new layout — **Opus** (wave 3, after 1, 2, 4)

**Files:** `crates/game/src/inv/machine.rs`.
- Main region: `draw_machine` for the building's family; input slots laid out in `Anchors.input`, fuel slot in `Anchors.fuel`, output in `Anchors.output`, progress arrow in `Anchors.progress`. Storage and Growing families keep their grid layouts inside the drawing area.
- Top of main region: recipe strip from `info::station_recipes` (known = item icon of first output, unknown = `Icon::Question`, picked = accent frame). Click → `set_recipe_pick`.
- Info region: `describe(Subject::Item(hovered or carried) | Recipe(pick) | Machine(i), level)` rendered by `draw_lines`; below it `icon_button(Play, "Başlat")` → `StartJob{recipe: pick}`, `icon_button(Auto, "Otomatik")` → toggle `SetAuto`. Old text buttons ("Hepsini yükle", "Girdileri yükle", "Hepsini al") become icon buttons with tooltips.
- Keep every existing behaviour and test (load/collect/fuel/far/auto-close).
- [ ] **Step 1: Failing tests:** `unknown_recipe_is_a_question_mark`, `strip_click_picks_the_recipe`, `play_starts_the_picked_recipe`, `auto_toggles_the_picked_recipe`, `info_shows_carried_item_while_dragging` (set a drag payload, info lines are for that item).
- [ ] **Step 2–5:** red → implement → titan green → `--ui-shot` machine pages for 4 eras, look → report.

### Task 6: Yap and İnşa in the new style — **Sonnet** (wave 3, after 1, 2, 4; parallel with 5)

**Files:** `inv/craft.rs`, `inv/build.rs`.
- Yap: 2×2 hands on an era work surface (`style::frame` with `Family::Bench` accent), arrow + `icon_button(Play, "Dene")`; info region shows `describe` for the hovered item, or for the recipe the current hands would match if it is known, else a `Picto` "?" row.
- İnşa: tiles show a small `draw_machine` of the building's family (state idle) instead of the group glyph; info region shows cost `Chips`, needs, `machine_note`, and `icon_button(House, "Yerleştir")`.
- [ ] **Step 1: Failing tests:** `craft_info_shows_known_match`, `build_tile_uses_family_drawing` (tile label = building name, picked tile → info has a `Chips` line with the cost), plus all existing craft/build tests green.
- [ ] **Step 2–5:** red → implement → titan green → `--ui-shot` craft/build pages, look → report.

### Task 7: Sweep and docs — **Haiku** (wave 4)
- README controls note: "**Defter:** J ile ayrı açılır, envanterle birlikte açık kalabilir." Update `crates/game/assets/FONTS.md` if Task 1 added Liberation Mono.
- Run on titan `cargo test --release --workspace` and `--ui-shot`; report summed counts, failing names, and the list of PNGs. Read-only otherwise.
