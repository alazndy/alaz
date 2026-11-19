# Handoff: inventory window (2026-11-18, from Claude Kaira on yoga-c930)

Spec: `docs/superpowers/specs/2026-11-18-inventory-window-design.md`
Plan: `docs/superpowers/plans/2026-11-18-inventory-window.md` (tasks 1-6)

## Branches
| Branch | State |
|---|---|
| `feat/inventory-window` | Tasks 1, 2, 4 done and merged. Tests green on titan: alaz 42+2, alaz-sim 242, alaz-net 8. |
| `inv-t3` | Task 3 (Yap tab) **unfinished**, one `wip:` commit. Agent stopped while fixing a missing `egui_kittest::kittest::Queryable` import in tests. Not tested. |
| `inv-t5` | Task 5 (machine tab) **unfinished**, one `wip:` commit. Agent had just started writing the 6 tests. Not tested. |

`inv-t3` and `inv-t5` branch from `e8f21ee` (before Task 4). Both edit `crates/game/src/hud.rs` in different places
(t3 deletes `tab_craft`, t5 changes `building_body`/`draw_building`); merges are expected to be clean.

## Next steps
1. Finish Task 3 on `inv-t3`: make the tests in `inv/craft.rs` compile, then run `cargo test --release -p alaz`.
   Compare with plan Task 3; migrated hud craft tests must pass.
2. Finish Task 5 on `inv-t5`: review the WIP against plan Task 5, write the 6 named tests (fail first), implement,
   add auto-close in `main.rs`.
3. Merge `inv-t3` and `inv-t5` into `feat/inventory-window`; run `cargo test --release -p alaz -p alaz-sim -p alaz-net`.
4. Remove `#[allow(dead_code)]` on `Carry`/`SlotLook` in `inv/mod.rs` if all variants are now used.
5. Task 6: README note (text in the plan) and a full `cargo test --workspace`.
6. Manual check in the game, then ask the user before merging to `main` or making a release.

## Notes
- Task 4 (İnşa): its "Yerleştir hidden before a tile is picked" assertion never ran red first; behaviour verified green.
- `PROTOCOL` is now 2: this branch cannot play with 0.4.1.
- A parallel session works on `feat/animation-polish`; do not touch it.
- Agents must not commit; the orchestrator reviews and commits each task.
