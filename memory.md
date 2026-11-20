# Project memory

## Context

A.L.A.Z uses a deterministic Rust simulation, a wgpu renderer, and lockstep multiplayer.

## Active Objectives

Verify that natural caves remain playable across world sizes and support early mining progression.
Finish the full backlog. It includes cave access and exploration saves, multiplayer, the book, world rendering, and the remaining simulation systems.

## Technical Decisions

- Generate natural cave voxels from the world seed and the surface elevation. Keep all entrances in the same coordinate space.
- Save player positions, excavated voxels, and rubble. Restore generated geometry from the seed.
- Keep the seed tags and resource node order stable for older saves. Discover unmarked entrances from terrain density.
- Save all worlds in format 33. Read older layouts, retain valid natural cave positions, and discard artificial chambers.
- Check air connectivity and walkability separately. A route needs headroom, floor support, and a climbable slope.
- Save rubble at the floor's world coordinates. Clear it by hand even with a full pack so it cannot trap a player.
- Find the nearest cave exit through the walkable graph. Recheck tool and pump access when a route changes.
- Use the same access rules for a clicked cave destination and the exit route. Keep entrance supports intact.
- Track natural cave exploration in the local player session. Sight stops at walls and closed corners.
- Match exploration to the exact saved world and player. Keep both the current and backup maps in the slot sidecar.
- Derive surface entrances from the existing threshold, slope, and roof. Preserve saved entrances and collision.
- Keep the exact cave floor selection when caching meshes. Upload only changed height regions when the texture origin stays fixed.
- Build the surface opening mesh from nearby seeded floors and ceilings. Share floor edges across chunks so the gallery remains visible through the mouth.
- Share player ownership and world generation checks between direct and relayed connections.

## Important Links & Paths

- Source and project description are in `gitrepo.md` and `README.md`.
- Density and navigation are in `crates/sim/src/cave_terrain.rs` and `crates/sim/src/cave_terrain/`.

## Current Focus

Seed 42 provides two connected entrances near tiles (222, 150) and (218, 166).

Small worlds use a separate seed tag so existing cave entrances and saved node indices stay stable.
Check backup recovery through the menu and the next save. The loader alone misses failures in either step.
The working tree includes the relay and the current cave changes. Public DNS, TLS, hosting, and two-network verification remain.
The Book links discovery paths to recipes and the item guide. Check deterministic start selection and measure startup on larger worlds.

## Change Log & Agent Trail

- [2026-11-20] [Codex Kaira]: Replaced the entrance back wall with the real gallery mesh. Added a regression test for the open seam and the landing geometry.
- [2026-11-20] [Codex Kaira]: Unified walking coordinates, route search, and step speed. Added density-based entrances, a horizontal cutaway, save format 33, and protocol 4. Surface orders resume after the player walks out. Preserve distinct entrance thresholds to prevent repeated transitions.
- [2026-11-19] [Codex Kaira]: Connected natural cave terrain to entrances, walking, excavation, rendering, and saved state.
- [2026-11-19] [Codex Kaira]: Added shared rockfalls and rubble clearing. Format 32 stores rubble. Format 31 still loads.
- [2026-11-19] [Codex Kaira]: Added nearest-exit routing, replanning on collapse, and a local exploration map.
- [2026-11-19] [Codex Kaira]: Audited cave coverage, movement, save recovery, and multiplayer command ownership.
- [2026-11-19] [Codex Kaira]: Fixed slot recovery and backup rotation. Added surface-connected entrance selection for new small worlds.
- [2026-11-20] [Codex Kaira]: Added access-aware cave walking, protected entrance supports, and wired one-click entry.
- [2026-11-20] [Codex Kaira]: Isolated surface fire and ice. Added per-connection command ownership and reseating after a world swap.
- [2026-11-20] [Codex Kaira]: Added terrain-based cave entrance profiles and local exploration checkpoints.
- [2026-11-20] [Codex Kaira]: Reduced cave upload work and integrated the relay with world replacement and command ownership.
- [2026-11-20] [Codex Kaira]: Added item and recipe pages, source lookup, recipe navigation, and the U shortcut.
- [2026-11-20] [Codex Kaira]: Added the discovery tree with rules-based prerequisites. Goals and the Book share the same next discovery.
