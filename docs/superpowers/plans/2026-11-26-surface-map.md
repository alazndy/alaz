# Surface Map Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** A small corner map and a large map that show only explored ground, remembered resources and buildings, and a camp mark.

**Architecture:** The game crate holds per-player records (`surface_map.rs`) and updates them from the world it already has. A sidecar file stores them next to the save. `map_ui.rs` draws both maps with cached chunk textures. The sim crate does not change.

**Tech Stack:** Rust, egui (through `alaz_render::egui`), `egui_kittest`.

**Spec:** `docs/superpowers/specs/2026-11-26-surface-map-design.md`

## Global Constraints

- No change in `crates/sim`, the world save format, or `PROTOCOL`.
- All player text is Turkish. Every widget has a label (accessibility check in `egui_kittest`).
- Sight is a circle of 40 tiles. Chunk is 64 x 64 tiles. Small map: 180 points, 90 tiles across.
- Mapped resources: every `NodeKind` except `Tree` and `Deadwood`.
- Keys: `Map` = `M`, `Mute` = `N`.
- Run tests with `cargo test -p alaz`; the last task runs `cargo test --workspace`.

## Review Focus

- A long walk in a 262 km world: draw cost stays bounded by the view, and picture building is capped per frame.
- A player's old `keys.cfg`: no key triggers two of Map, Mute, or another action.
- Loading another slot or starting a new world: no tile, mark, or camp carries over.
- A guest who disconnects or closes the window: the guest map is on disk.
- Large interface zoom in an 800 x 600 window: the small map does not cover a panel's controls.

---

### Task 1: Records and observing

**Files:** Create `crates/game/src/surface_map.rs`. Modify `crates/game/src/main.rs` (`mod surface_map;`).

**Produces:**
```rust
pub(crate) const SIGHT: i32 = 40;
pub(crate) const CHUNK: i32 = 64;
pub(crate) type Tile = (i32, i32);
pub(crate) type Mask = [u64; 64]; // one row per word, bit = x within the chunk

#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub(crate) struct Records {
    pub explored: BTreeMap<Tile, Mask>, // key = chunk
    pub resources: BTreeMap<Tile, NodeKind>,
    pub buildings: BTreeMap<Tile, BuildingKind>,
    pub camp: Option<Tile>,
}
impl Records {
    pub fn is_explored(&self, tile: Tile) -> bool;
    pub fn is_empty(&self) -> bool;
}
pub(crate) fn mapped(kind: NodeKind) -> bool;
pub(crate) fn tile_of(x_mm: i64, y_mm: i64) -> Tile;

#[derive(Default)]
pub(crate) struct SurfaceMap { /* records, chunk versions, last sight, heading */ }
impl SurfaceMap {
    pub fn from_records(records: Records) -> Self;
    pub fn records(&self) -> &Records;
    pub fn observe(&mut self, world: &World, player: AgentId);
    pub fn chunk_version(&self, chunk: Tile) -> u32; // 0 = as loaded; bumps when new tiles appear
    pub fn heading(&self) -> (f32, f32);             // unit vector of the last move, (0, -1) at start
    pub fn set_camp(&mut self, camp: Option<Tile>);
}
```

- [ ] **Step 1: Write the failing tests** in the module. World: `World::with_size(42, 256, 256)`, one agent at `spawn_mm`.
  - `sight_marks_a_circle_around_the_player`: with player tile `(x, y)`: `(x+40, y)` and `(x+28, y+28)` explored; `(x+41, y)` and `(x+29, y+29)` not (skip a probe that leaves the map).
  - `a_resource_is_remembered_until_the_spot_is_seen_again`: observe near a mapped node; it is in `resources`. Move more than 40 tiles away, set its `amount` to 0, advance `tick` by 60, observe: still there. Move back, observe: gone.
  - `trees_and_deadwood_are_not_remembered`: no value in `resources` fails `mapped`.
  - `a_building_is_remembered_until_the_spot_is_seen_again`: same pattern with a campfire that is removed while the player is away.
  - `nothing_is_observed_underground`: reuse the seed-42 cave entry steps from `cave_map.rs` tests; records stay unchanged after entering.
  - `a_chunk_version_changes_only_for_new_ground`: second observe at the same place keeps the version.
  - `the_heading_follows_the_last_move`: move +x, observe, `heading() == (1.0, 0.0)`.
- [ ] **Step 2:** `cargo test -p alaz surface_map` fails to compile (missing items).
- [ ] **Step 3: Implement.** Observe returns early underground (`cave_view().is_some()`). Run it when the tile changes, or `TICKS_PER_SECOND` ticks after the last run (also when `tick` went backwards). Inside the circle: mark bits for in-bounds tiles, drop remembered resources and buildings, insert the current ones (`world.nodes_in` on the bounding box; `world.buildings`).
- [ ] **Step 4:** `cargo test -p alaz surface_map` passes.
- [ ] **Step 5:** Commit `feat(game): remember the explored surface`.

### Task 2: Storage

**Files:** Create `crates/game/src/saveio/surface.rs`. Modify `crates/game/src/saveio.rs`, `saveio/discovery.rs`, `main.rs` (call sites).

**Consumes:** `Records`.

**Produces:**
```rust
// saveio.rs
pub fn save_in(dir, world, me, seen: &KnownTiles, surface: &Records) -> io::Result<SaveOutcome>;
pub fn save(world, me, seen, surface) -> io::Result<SaveOutcome>;
pub struct LoadedSave { world, seen, surface: Records, recovered }
pub fn guest_save_in(base: &Path, world: &World, surface: &Records) -> io::Result<()>;
pub fn guest_load_in(base: &Path, world: &World) -> Records;
// saveio/surface.rs
pub(super) const FILE_NAME: &str = "surface-map.alz";
pub(super) fn encode(records: &Records) -> Vec<u8>;          // empty records -> empty vec
pub(super) fn decode(bytes: &[u8], world: &World) -> Option<Records>;
```

- Share the pairing code: `discovery::update` and the record lookup take `file name`, `magic`, `unit` (bytes per counted item) and a payload slice. Cave map: magic `ACM1`, unit 11 (format unchanged). Surface map: magic `ASM1`, unit 1.
- Payload, little endian: `u32` count + chunks (`i32 x, i32 y, 64 x u64`); `u32` count + resources (`i32, i32, u8`); `u32` count + buildings (`i32, i32, u8`); `u8` flag + camp (`i32, i32`).
- Resource code = index in a fixed `NodeKind` table (enum order, append only). Building code = index in `BuildingKind::ALL`.
- `decode` returns `None` for: short or extra bytes, unknown code, tile or chunk outside the map, duplicate key.
- Guest file: `<base>/guest-maps/<seed as 16 hex digits>.alz` = magic `ASG1` + payload + 8-byte checksum; written with `write_atomic`.
- `delete_in` also removes `surface-map.alz`.

- [ ] **Step 1: Write the failing tests** (in `saveio.rs` tests):
  - `the_surface_map_returns_with_its_save_and_player`: two players save different records; each loads their own.
  - `a_backup_restore_brings_the_matching_surface_map`: save A, save B, corrupt `save.alz`, load: `recovered` and records A.
  - `a_damaged_surface_map_gives_an_empty_map`: flip one byte in `surface-map.alz`: load succeeds, `surface` empty, cave map intact.
  - `decode_rejects_bad_records`: truncated, unknown code, out-of-bounds tile: `None`.
  - `deleting_a_slot_removes_the_surface_map`.
  - `a_guest_map_is_kept_by_world_seed`: round trip; a world with another seed loads empty.
- [ ] **Step 2:** `cargo test -p alaz saveio` fails.
- [ ] **Step 3: Implement.** Existing tests pass `&Records::default()`. A sidecar error stays `SaveOutcome::WorldOnly`; the message becomes "Dünya kaydedildi. Harita kaydedilemedi."
- [ ] **Step 4:** `cargo test -p alaz saveio` passes, cave tests unchanged in meaning.
- [ ] **Step 5:** Commit `feat(game): store the surface map beside the save`.

### Task 3: Keys

**Files:** Modify `crates/game/src/keys.rs`.

**Produces:** `Action::Map` (id `map`, label "Harita", default `M`), placed after `Routes`; `Mute` default `N`; `Action::ALL` has 32 entries.

- [ ] **Step 1: Write the failing tests:**
  - `a_fresh_setup_has_map_on_m_and_mute_on_n`.
  - `an_old_config_moves_mute_to_n`: `from_text("mute=M,yok\n")` → Map `M`, Mute `N`.
  - `an_old_config_keeps_a_chosen_n`: `"mute=M,yok\ndrop=N,yok\n"` → Mute `M`, Drop `N`, Map none.
  - `an_old_config_with_m_elsewhere_leaves_map_unbound`: `"mute=K,yok\ndrop=M,yok\n"` → Drop `M`, Map none, Mute `K`.
  - `an_old_config_with_mute_elsewhere_gives_map_m`: `"mute=K,yok\n"` → Map `M`, Mute `K`.
  - `a_config_with_a_map_line_is_taken_as_written`: `"map=yok,yok\nmute=M,yok\n"` → Map none, Mute `M`.
- [ ] **Step 2:** `cargo test -p alaz keys` fails.
- [ ] **Step 3: Implement** the migration at the end of `from_text`, only when the text has no `map` line. A key counts as free when no action other than Mute and Map holds it.
- [ ] **Step 4:** `cargo test -p alaz keys` passes.
- [ ] **Step 5:** Commit `feat(game): map key, mute moves to N`.

### Task 4: Drawing the two maps

**Files:** Create `crates/game/src/map_ui.rs`. Modify `ui_art.rs` (`Glyph::Map`), `main.rs` (`mod map_ui;`).

**Consumes:** `SurfaceMap`.

**Produces:**
```rust
#[derive(Default)]
pub(crate) struct MapView { pub open: bool, center: Option<(f32, f32)> /* None = follow */, zoom: f32,
    hidden: BTreeSet<NodeKind>, cache: HashMap<Tile, (u32, TextureHandle)>, seed: u64 }
impl MapView { pub fn toggle(&mut self); }
pub(crate) enum MapAct { Close, SetCamp(Option<Tile>) }
pub(crate) fn project(delta_tiles: Vec2, yaw_deg: f32) -> Vec2; // screen offset in tiles
pub(crate) fn edge_point(offset: Vec2, half: f32) -> Option<Vec2>; // None when inside the square
pub(crate) fn chunk_image(world: &World, records: &Records, chunk: Tile) -> ColorImage;
pub(crate) fn draw_small(ctx, world, me: &Agent, map: &SurfaceMap, view: &mut MapView, yaw_deg: f32) -> Rect;
pub(crate) fn draw_large(ctx, world, me: &Agent, map: &SurfaceMap, view: &mut MapView) -> Vec<MapAct>;
```

- `project` matches the cave arrow: `x = dx cos - dy sin`, `y = dx sin + dy cos`.
- Both maps use one painter routine: fog fill, chunk quads inside the view (nearest filtering), buildings, resources, camp, other humans within `SIGHT`, the player arrow.
- Build at most 4 chunk pictures per frame; request a repaint while some are missing. Clear the cache when the world seed changes.
- Large map: zoom 0.25 to 12 points per tile, default 3; below 1.5 resources are 2-point dots. Right click on explored ground sets the camp.
- Controls: "Kapat [key]", "Beni ortala", "Kampımı buraya koy", "Kamp işaretini kaldır", one checkbox per resource kind with its Turkish name, a legend. Small map camp text: "Kamp: 120 m".

- [ ] **Step 1: Write the failing tests:**
  - `north_is_up_when_the_camera_is_straight`: `project((0,-1), 0.0) ≈ (0,-1)`; `project((1,0), 90.0) ≈ (0,1)`.
  - `a_far_camp_is_pinned_to_the_edge`: `edge_point((200,0), 80) ≈ (80,0)`; `edge_point((10,10), 80) == None`.
  - `unexplored_pixels_stay_fog`: one explored tile in a chunk: that pixel differs from fog, all 4095 others equal it.
  - `the_small_map_shows_the_distance_to_camp`: camp 120 tiles east: label "Kamp: 120 m" exists and lies inside an 800 x 600 window.
  - `the_large_map_fits_and_sets_the_camp`: all control labels inside 800 x 600; click "Kampımı buraya koy" returns `SetCamp(Some(player tile))`; unchecking "Taş" hides that kind.
  - `a_large_explored_area_builds_few_pictures_per_frame`: 100 explored chunks, one frame: `cache.len() <= 4`.
- [ ] **Step 2:** `cargo test -p alaz map_ui` fails.
- [ ] **Step 3: Implement.** Terrain colors from `Terrain`; deep water darker (`water_depth`); shade by `elevation_mm`.
- [ ] **Step 4:** `cargo test -p alaz map_ui` passes.
- [ ] **Step 5:** Commit `feat(game): draw the small and large surface maps`.

### Task 5: Place the maps in the interface

**Files:** Modify `crates/game/src/hud.rs`.

**Consumes:** `draw_small`, `draw_large`, `MapAct`.

**Produces:** `UiCtx.surface: Option<&SurfaceMap>`; `KnapState.map: MapView` (kept by `reset`); `UiAction::SetCamp(Option<Tile>)`; a "Harita" button in the left menu bar.

- The small map shows when: on the surface, no craft, no inventory window, no book, large map closed.
- `draw_building` and `draw_worker` take `top: f32` and start under the small map. Their body scrolls when it does not fit.
- Opening the map closes the book and any tab; opening a tab or the book closes the map. A craft closes it.

- [ ] **Step 1: Write the failing tests** (`hud.rs` tests):
  - `the_small_map_leaves_the_building_panel_clear`: 800 x 600, campfire selected: the panel rect does not intersect the map rect and every panel button is inside the window. Repeat at 1.5 interface zoom.
  - `the_map_button_opens_the_large_map_and_it_closes_again`.
  - `the_small_map_steps_aside`: hidden while the inventory window or the book is open, and underground.
- [ ] **Step 2:** `cargo test -p alaz hud` fails.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** `cargo test -p alaz hud` passes.
- [ ] **Step 5:** Commit `feat(game): put the maps on the screen`.

### Task 6: Wire the game loop

**Files:** Modify `crates/game/src/main.rs`.

**Consumes:** all of the above.

- Field `surface_map: SurfaceMap`. Reset in `replace_world`; set from the save in `restore_saved_world`.
- Observe before saving and once per frame next to `cave_map.observe`.
- `A::Map` toggles; `A::Menu` closes the large map first. `UiAction::SetCamp` calls `set_camp`.
- The wheel and clicks over the large map do not reach the camera or the world (egui consumes them; verify).
- Guest: at join and after a world resync with a new generation, load with `guest_load_in`. `save_game` writes the guest map for `Net::Client` before the `no_save` check and updates `last_save_tick`. Call it on quit, window close, menu quit, disconnect (before `net` becomes `Solo`), and before a new generation replaces the map. After a disconnect, never write a host save.
- `ALAZ_DEMO_TAB=map` opens the large map.

- [ ] **Step 1: Write the failing test** where the logic is pure: `guest_maps_are_written_for_the_world_they_belong_to` (a helper `fn flush_guest(&Net-independent args)` writes to a temp base). Wiring with a window is checked in Step 4.
- [ ] **Step 2:** Run it; it fails.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** `cargo test -p alaz` passes. Then take screenshots with `ALAZ_SHOT` and an isolated `XDG_DATA_HOME`: default view, `ALAZ_DEMO_TAB=map`, and `ALAZ_UI_ZOOM=1.5`. Look at each.
- [ ] **Step 5:** Commit `feat(game): the surface map in play`.

### Task 7: Documents and final check

**Files:** `README.md` (keys, map, environment variable table), `memory.md`.

- [ ] **Step 1:** Update both files.
- [ ] **Step 2:** `cargo test --workspace`: all pass. `cargo clippy --workspace --all-targets`: no new warnings. `git diff main --stat -- crates/sim`: empty.
- [ ] **Step 3:** Read the full diff against the spec and the Review Focus list. Fix what fails.
- [ ] **Step 4:** Commit and push `feat/map`. Do not merge.
