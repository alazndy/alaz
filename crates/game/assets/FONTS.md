# Fonts

Interface fonts change with the age the tribe has reached (see `set_era_font` in `src/hud.rs`):

| Ages | Font | Source |
|---|---|---|
| first perception steps (stone) | Pixelify Sans, `pixelify-sans.ttf` | github.com/google/fonts, ofl/pixelifysans |
| middle steps (metal) | Jacquard 12, `jacquard-12.ttf` | github.com/google/fonts, ofl/jacquard12 |
| late steps (machines) | Jersey 10, `jersey-10.ttf` | github.com/google/fonts, ofl/jersey10 |

All three are licensed under the SIL Open Font License 1.1 (https://openfontlicense.org). `alaz-pixel.ttf` is our own
5x7 face (see `tools/make_pixel_font.py`); it stays behind the others for any glyph they lack.
