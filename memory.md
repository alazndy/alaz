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
- Fourth-generator worlds (small, seed tag 0xA1A7) get rivers and lakes from a priority flood over the 256 x 256 domain. Integer math only, so every peer gets the same water.
- A river's current covers its whole width and is weaker toward the banks. A swimmer is carried along the drainage.
- Fords are bars of gravel laid across a river, so walkers and routes cross dry-shod. The drainage runs on through them. Gravel is sand terrain in the sim and is drawn as stone. Rivers wider than 9 tiles have no ford yet.

- The start screen has a version screen (`crates/game/src/update_ui.rs`, logic in `crates/updater`). It reads GitHub's release list once per start (`ALAZ_NO_UPDATE_CHECK=1` turns it off) and installs only after the player presses the button and the download matches `SHA256SUMS`. Tags with a dash (`v0.6.0-rc.1`) are pre-releases and belong to the Preview channel. No update from a running game.
- Run dependency audits with `~/.cargo/bin/cargo-audit audit` (not on PATH). Open warnings: `heapless` 0.7.17 through `postcard` (needs a `postcard` upgrade that may touch the net protocol), `atomic-polyfill`, `ttf-parser`.
- Wild animals (not wolves) go to drink: in thirsty spells of 6 epochs (1 spell in 3, picked from seed and animal id) `Wild::wander` aims at `nearest_bank`, a fixed-order ring scan of 20 tiles. No new saved field, so old saves still load. Fish already existed (`fish_school`, any water with depth 1 or more, rod or spear) and also work in fourth-generator water (test added).
- Natural causeways (`Crossing::Bridge` in `rivers.rs`): a rock bar like a ford, across rivers with 4x RIVER_AT drainage or more and 10 to 24 tiles of water, one every 60 tiles. Same `gravel` mask, so the renderer draws them as stone. Seed 7 at 256 has 4 (spans 11 to 20). Lakes and the sea are never crossed. A player-built bridge was left out on purpose: it needs a saved terrain overlay, a save version bump and about 12 water code paths.

## Important Links & Paths

- Source and project description are in `gitrepo.md` and `README.md`.
- Updater: `crates/updater/` (version order, release pick, checksum, guarded download, install). Release workflow: `.github/workflows/release.yml` (builds `SHA256SUMS`).
- Rivers, lakes, current and fords are in `crates/sim/src/rivers.rs`. The water shader streaks are in `crates/render/src/shader.wgsl`.
- Density and navigation are in `crates/sim/src/cave_terrain.rs` and `crates/sim/src/cave_terrain/`.

## Current Focus

Seed 42 provides two connected entrances near tiles (222, 150) and (218, 166).

Small worlds use a separate seed tag so existing cave entrances and saved node indices stay stable.
Check backup recovery through the menu and the next save. The loader alone misses failures in either step.
The working tree includes the relay and the current cave changes. Public DNS, TLS, hosting, and two-network verification remain.
The Book links discovery paths to recipes and the item guide. Check deterministic start selection and measure startup on larger worlds.

## Change Log & Agent Trail

- [2026-11-23] [Claude Kaira]: Added the updater crate and the version screen (check, channel choice, checked install). The workflow now publishes `SHA256SUMS` and marks dash tags as pre-releases. Updated `url` to 2.5.9 after `cargo audit`. Not yet tested: a real release with `SHA256SUMS` (needs a tag such as `v0.6.0-rc.1`), the Windows installer path, and the screen by eye.
- [2026-11-23] [Claude Kaira]: Added rivers and lakes, a current across the full river width, fords of gravel (small and bigger rivers), and stone drawing for ford gravel. Pushed `feat/seamless-caves`. Open: animals and fish at the water, bridges for rivers over 9 tiles wide, rivers in older worlds, and a check of the moving streaks.
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
- [2026-11-24] [Claude Kaira]: Animals drink at river banks. Added a fishing test for fourth-generator water, `--ui-shot` shots of the version screen, version 0.6.0-rc.1 (pre-release published, installed end to end against GitHub), PR #4 open. Titan checkout is `~/Projects/alaz`.
- [2026-11-24] [Claude Kaira]: Added natural rock causeways over wide fourth-generator rivers (test: a walker crosses dry).
