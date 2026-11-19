# UI redesign: A.L.A.Z's own style (design)

Date: 2026-11-19. Size: project. Builds on the inventory window (`crates/game/src/inv/`, merged to local `main` 51837be).
Mockups: `.superpowers/brainstorm/242577-1795076211/content/own-style.html` (also `layout.html`).

## Intent
The new inventory window works, but the user finds it text-heavy and hard to read: a small (≈600×320 px)
half-transparent panel, small serif text over the world, the known-recipe "Defter" squeezed into the Yap tab, plain
text tabs. The user wants the GUI redesigned from scratch, nicer and more readable, with Minecraft, Factorio and
Satisfactory as references **for layout and how explanations are shown**, but in A.L.A.Z's own style. The look must
follow the **era** and the **machine**.

## Goals
1. **Layout (hybrid of the references):**
   - Centre: the **machine drawn** (furnace mouth and flame, mill stone and wheel, bench top…), with the slots placed
     on the drawing where they belong (input at the mouth, fuel below, output beside).
   - Top: a **recipe strip** of recipe icons for this machine (unknown = "?"). Click selects the recipe.
   - Right: a fixed **info panel**: what the pointer is on, else the selected recipe. Inputs, outputs and conditions
     as icon "chips", one short note line, and the actions **Başlat** (▶) and **Otomatik** (↻) as icon buttons
     with tooltips.
   - Bottom: the bag grid. Slots carry no text; names appear in tooltips and in the info panel.
2. **Era material.** The window looks made of the era's material:
   - stone (perception 0–1): hide and bark, rounded "pit" slots, warm brown;
   - metal (2–3): riveted iron plates, inset dark slots;
   - machine (4): steel / blueprint grid, thin light frames;
   - computer (5): green-on-black screen, scanlines.
3. **Explanations grow with perception** (the game already switches heat words to °C at level 3):
   - 0–1: pictures only (item icons, `🥩 → 🔥 → 🍖`-style rows, heat as 1–4 flames, time as hourglasses);
   - 2–3: pictures + words (item names, heat words from `hud::heat_word`, one short note);
   - 4: words + numbers (°C need/now, seconds, rate per minute);
   - 5: full data in a terminal-like block (inputs, temperatures, time, efficiency, output rate, auto state).
4. **Per machine:** each window-building belongs to a **machine family** with its own drawing and accent colour
   (see "Machine families"). The era sets the material; the family sets the shape and accent.
5. **Readable:** panels fully opaque; window ≥ 900×620 logical px at 1080p (shrinks to fit smaller screens);
   bag slots ≥ 56 px; text never smaller than the 22 px body size; strong contrast per era palette.
6. **The book (Defter) is separate.** A book window of its own (key J, as the journal today), openable at the same
   time as the inventory window, docked to its left. It holds the journal sections (days, finds, recipes) and the
   known-recipe hints that are in the Yap tab today. Its material follows the era: hide parchment → bound book →
   ledger → screen. Clicking a recipe in the book while a machine is open selects it on the recipe strip.
7. **Yap and İnşa** use the same system: Yap draws the 2×2 hands on an era work surface, with the info panel telling
   what is known about the current combination (perception-scaled). İnşa shows tiles with the family drawing of the
   building, and the info panel shows cost chips, needs, notes and **Yerleştir**.

## Non-goals
- No change to sim rules, recipes, commands or the network protocol.
- No change to the HUD outside the windows (status, needs, action bar, compass), the knapping mini-games, the worker
  panel, the cave HUD. (They can follow later.)
- No hand-drawn PNG art in this project. All drawing is procedural (egui painter), as `icons.rs` does today; the
  existing `assets/icons/<item>.png` override keeps working.

## Anti-goals
- A design that copies one reference's look; it must read as A.L.A.Z.
- Early-era players drowning in numbers, or late-era players losing information they had.
- Text in the window that only repeats what an icon already shows.
- Regressions in what the inventory-window tests cover (load/collect/fuel/drag/hand/build behaviour).

## Constraints
- Fonts stay **Liberation Serif** for all text (the user's earlier rule: all fonts Times New Roman). The computer era
  may use **Liberation Mono** (same family, Courier New metrics) for its data block. The mockup fonts were only
  illustrative.
- egui 0.37 painter only; no new crates.
- Era = `hud::ERA` (perception level 0–5, `PERCEPTION_STEPS` has 6 steps). Today `pxpanel::Theme::era` has three
  looks (0–1, 2–3, 4+); this project makes four (0–1, 2–3, 4, 5).
- `hud.rs` is large; new code lives in `crates/game/src/inv/` and a new `crates/game/src/style/` module.
- Work on branch `feat/ui-redesign` (worktree `/home/alaz/alaz-ui`); heavy builds and tests run on titan-home.

## Approaches
- **Picked: procedural drawing with a style module.** A `style` module gives era palettes, frame painters (hide,
  rivets, blueprint, screen), slot painters, and `machine_art(family, era, rect, state)` drawings. Everything stays in
  code, scales to any size and era, and needs no art files. PNG overrides can come later.
- Rejected: PNG art per era and machine. Best looking, but needs an artist for ~4 eras × ~11 families × states.
- Rejected: only re-skinning the current window (colours, opacity). Does not fix layout or the text load.

## Decided
### Machine families (drawing + accent)
| Family | Buildings | Drawing | Accent |
|---|---|---|---|
| Fire | Campfire, Smokehouse | stones ring, flame | orange |
| Kiln | CharcoalKiln, PitKiln, CokeOven, LimeKiln, CementKiln, BrickKiln | dome with door | ember red |
| Furnace | Furnace, Bloomery, BlastFurnace, Cupola, PuddlingFurnace, Bessemer | tall stack, glowing mouth | molten orange |
| Smithing | Anvil, IronAnvil, SteelAnvil, TripHammer, SteamHammer, ForgeHammer | anvil (+ hammer arm) | steel blue |
| Grinding | HandMill, StoneMortar, WaterMill, AnimalGin, OilPress, Press | millstone (+ wheel) | water blue |
| Bench | Workbench, Workshop, MoldingBench, PotterWheel, Tannery | work surface, tools | wood brown |
| Textile | Loom, PowerLoom, SpinningWheel | frame and threads | wool cream |
| Machine tool | Lathe, DrillMill, Sawmill, RollingMill, RollingLine | bed and spindle | oil green |
| Storage | Depot, Cellar, Cooler | shelves / crate grid | neutral |
| Growing | Field, DryingRack | soil rows / rack | leaf green |
| Electric | Computer, AssemblyRobot, and other stations with power needs | panel and screen | cyan |
Any window-building not listed falls back to Bench. The family list lives in one table in code.

### Machine state in the drawing
- The drawing reacts: flame size from fuel, glow from temperature, moving part while a job runs, dim when idle,
  red outline when a requirement is missing (the old warning lines become this plus one info-panel line).

### Info panel content by perception (furnace example)
- 0–1: `🥩 → 🔥 → 🍖`, heat as filled/empty flames, time as hourglasses.
- 2–3: "Bronz · gerek: bakır, kalay · ısı: kızgın, yetiyor · not: körük yanındaysa çabuk ısınır."
- 4: "Bakır külçe · girdi ×4 cevher ×1 kömür · 1150/1085 °C · 40 sn · 1,5/dk".
- 5: monospace block: `CU_INGOT / IN ore×4 coke×1 / T 1150/1085°C / t 40s η 0.92 / OUT 1.5/min / auto ON`.
- Every number shown at 4–5 comes from data the sim already has; the builder must not invent values (if a rate
  or efficiency is not computable, leave that line out).

### Window and book
- Window: centred, opaque, era frame, header = family icon + machine name + close. Tabs are icon buttons
  (🎒 Çanta, 🔨 Yap, 🏠 İnşa, machine icon) with the tab word as tooltip.
- Book: separate window; J toggles it; it can be open with or without the inventory window; docked left of the
  inventory window when both are open, otherwise centred. It replaces the Journal tab panel.

## Drawing language (user's decision)
- **Pixel art**, like the current HUD: every frame, slot, machine drawing and glyph is drawn on the UI pixel grid
  (`pxui::unit()`, `pxui::snap_rect`), hard corners, curves as stepped pixels, flat colour bands instead of smooth
  gradients. The mockups' smooth curves were only illustrative. "Rounded pit slots" means stepped-corner slots.
- The new `style` painters build on `pxui`/`pxpanel` helpers instead of free vector shapes.

## Builder-check answers (my calls unless marked)
- **Icons:** every glyph (flames, hourglasses, arrows, tab icons, "?", family icons) is painter-drawn: extend
  `ui_art::Glyph`; item chips reuse `icons::draw_item`. No emoji or font glyphs (the mockups' emoji were stand-ins).
- **Theme:** a new `style::Palette` type used only by the inventory window and the book. `pxpanel::Theme::era` and the
  rest of the HUD stay as they are (non-goal).
- **Data for perception 4–5:** need °C = `RecipeDef.heat`, now °C = `Building.temp`, time = `RecipeDef.ticks` /
  `TICKS_PER_SECOND`, progress from `b.job`, rate per minute = outputs per job ÷ job minutes, auto = `b.auto`.
  No efficiency value exists: leave it out. Level 5 = level 4 data in a monospace block plus auto state; that is
  accepted. The note line = `hud::machine_note(kind)` plus the requirement line (`requirement_missing`) when it
  applies; no new per-recipe text table. "gerek: …" at 2–3 = input item names.
- **Recipe strip:** recipes with `station == Some(kind)`; known = `w.knows(r.discovery)`, unknown shows "?".
  The selected recipe is UI state per building (egui memory, default = `b.auto` or the first known). Başlat sends
  `StartJob { recipe: selected }`, Otomatik toggles `SetAuto { recipe: Some(selected) / None }`. The book click sets
  the same UI state.
- **Book:** a separate `book_open: bool` in `App`, toggled by J (`Action::Journal`); `Tab::Journal` and its tab
  button are removed; the book draws `journal_ui::tab_journal` content plus the known-recipe hints moved out of Yap.
  The camp window stays as it is. Existing journal small-window tests move to the book window.
- **Shrinking:** below ~1300 px width the book overlays instead of docking; below ~1000 px the info panel moves under
  the machine; bag columns drop to fit; the 22 px body text is a floor (11 px only for slot counts, as today).
  Existing small-window fit tests stay and get a book-window case.
- **During a drag** the info panel shows the carried item; with no hover, the selected recipe.
- **Family table** is keyed by `BuildingKind` only.
- **Era** is the global `hud::ERA`; the look may change live while a window is open.

## Left to the builder
- Exact drawings, sizes and colours inside the families and eras, matching the mockups' spirit.
- How "moving part" is animated (frame time from egui is fine; UI-only, no sim effect).
- Tooltip wording.
- Splitting the work into tasks (suggested: style module + window frame; info panel + perception text; machine
  families; book window; Yap/İnşa restyle).
- Screenshot-based checks: use the existing `ALAZ_DEMO_*` env aids, and add `ALAZ_DEMO_ERA=<0..5>` to force the era
  look for screenshots.
