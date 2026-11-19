# Fonts

The whole interface uses one face: Liberation Serif (`liberation-serif.ttf`), metric-compatible with Times New Roman.
Times New Roman itself is proprietary and cannot be shipped. Source: the Liberation fonts 2.x, SIL Open Font License 1.1.

`liberation-mono.ttf` (Liberation Mono 2.1.5, SIL Open Font License 1.1, Courier New metrics) is used only for the
computer era's data block in the inventory window (`src/style/lines.rs`), as its own font family `alaz-mono`.

`alaz-pixel.ttf` is our own 5x7 face (see `tools/make_pixel_font.py`); it stays behind Liberation Serif for any glyph it lacks.
The old era fonts (`pixelify-sans.ttf`, `jacquard-12.ttf`, `jersey-10.ttf`) are no longer used by `set_era_font` in `src/hud.rs`.
