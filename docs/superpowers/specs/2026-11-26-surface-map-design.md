# Surface map: design

Date: 2026-11-26. Status: approved by the user on 2026-11-26. Size: project.

## Intent
The user wants a map to return to camp and to remember where resources are.
The user decided: a small corner map and a large map; unexplored ground stays hidden; approach A.

## Goals
- A small map always shows on the surface. It points to the camp mark with a distance.
- A large map opens with a key. The user can zoom and drag it.
- The map shows only explored ground.
- Resources and buildings show as the player last saw them.

## Non-goals
- No click to walk. No hand-placed resource pins. No shared map between players.
- No icon for single trees or deadwood.
- No change to the cave map, the sim rules, the world save format, or `PROTOCOL`.

## Approaches
| Approach | Trade-off |
|---|---|
| A. Remembering map (chosen by the user) | Most work. Information can be out of date. |
| B. Live map: store explored area only, read contents from the world | Simplest. Shows changes the player never saw. |
| C. Hand-placed marks | Clean map. The player must remember by hand. |

## Behaviour
- **Sight:** a circle of 40 tiles (1 tile = 1 m) around the player marks ground as explored.
- **Update:** observe when the player enters a new tile, and once per second while still.
- **Memory:** inside the circle, replace remembered resources and buildings with the current ones.
  A depleted resource disappears only when the player sees the spot again. Outside the circle nothing changes.
- **Resources:** stone, clay, copper, tin, iron, berries, herbs, fiber, cave mouth.
- **Buildings:** every building kind. Campfires and shelters get their own marks.
- **Camp mark:** one per player per world. Set it with a button ("here") or a right click on the large map. It can be removed.
- **Underground:** observing pauses. The cave map replaces the small map. The large map still opens.
- **Other players:** shown only inside the sight circle.
- **Old saves:** the map starts empty.

## Interface
- Small map: top right, about 180 px, 80 tiles across at least. Building, worker, and cave panels must not overlap it.
  It turns with the camera, so "up" on the map is "up" on the screen.
- A camp mark outside the small map shows as an arrow on its edge with the distance in meters.
- Large map: centered panel over most of the window. North up. Player drawn with a heading arrow.
  Wheel zooms around the pointer. Drag pans. A button centers on the player.
  Checkboxes filter resource kinds (not saved). A legend names the marks.
- Open and close: the map key, `Escape`, or a new "Harita" button in the left menu bar.
- While the pointer is over the large map, the wheel and clicks do not reach the camera or the world.
- Unexplored ground is flat dark. Explored ground uses one color per terrain, darker for deep water, lightly shaded by height.
- All text is Turkish and follows the era font and panel style.

## Keys
- New action `Map`, default `M`. `Mute` default moves to `N`.
- Existing `keys.cfg` without a `map` line: if `mute` is `M`, move `mute` to `N` when `N` is free.
  If another action holds `M` or `N` by the player's choice, do not take it. `Map` then has no key; the menu button still works.

## Parts
| Part | Responsibility |
|---|---|
| `crates/game/src/surface_map.rs` (new) | Data and observing. No drawing. |
| `crates/game/src/map_ui.rs` (new) | Small map, large map, tile pictures. |
| `crates/game/src/saveio.rs`, `saveio/` | Store and load the map file. |
| `keys.rs`, `main.rs`, `hud.rs` | Key, state, panel placement, input. |

Data (per player):
- `explored`: map from chunk (64 x 64 tiles) to a 4096-bit mask.
- `resources`: map from tile to `NodeKind`.
- `buildings`: map from tile to `BuildingKind`.
- `camp`: optional tile.

## Storage
- New file `surface-map.alz` next to the save. It uses the same pairing as `cave-map.alz`:
  records match the save fingerprint and the player, so a backup restore gets the matching map.
- The file has its own magic and a checksum. A damaged or foreign file gives an empty map, never a failed load.
- Reject out-of-bounds tiles and unknown kinds.
- `delete_in` removes the file.
- A save by an older build changes the fingerprint. The map is then empty. This is accepted.
- **Multiplayer guest:** guests do not save worlds. The guest map goes to `guest-maps/<world seed>.alz`
  in the data folder, written on the autosave interval and on leaving. Write it atomically.

## Performance
- Build a chunk picture once, then cache it. Rebuild only chunks whose mask changed.
- Build at most a few chunk pictures per frame, so large maps do not stall (terrain generation is lazy).
- Draw only chunks inside the view. When zoomed far out, draw resources as small dots.

## Tests
- Observe: marks the circle only; remembers a node; keeps it while away; forgets it on a second look after depletion.
- File: round trip; two players; backup restore; damaged file; delete.
- Keys: old config migration, including the cases that must not move a key.
- Interface: both maps fit small windows and large interface zoom; panels do not overlap; every control has a label.
- No sim crate change. The existing full suite stays green.

## Left to the builder
Colors, mark shapes, zoom limits, chunk picture budget, and how to share code with the cave map file.
