//! Pixel-drawn glyphs for the inventory window and the book: 12 x 12 character bitmaps scaled by whole pixels.
//! No font glyphs and no emoji, so every era and every font draws them the same.

use alaz_render::egui::{self, Color32, Painter, Rect, Response, Sense, Ui, Vec2};

use super::{mix, palette, Era, Family};
use crate::pxui::{snap_pos, unit};

// Play, Auto and Book are for the machine tab's actions and the book, which are built on this module
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Icon {
    Flame,
    Hourglass,
    Arrow,
    Play,
    Auto,
    Question,
    Bag,
    Hammer,
    House,
    Book,
    Close,
    Thermo,
    Clock,
    Warn,
    /// Everything the machine takes goes in: an arrow down into a tray.
    Load,
    /// Everything made comes out: an arrow up out of a tray.
    Take,
    /// A quarter turn.
    Turn,
    /// Four arrows: put it somewhere else.
    Move,
}

// 'X' main colour, 'L' lighter, 'W' near white, 'D' darker, 'K' ink, 'S'/'s' stone grey light/dark,
// 'B'/'b' wood or soil brown light/dark, '.' empty.
fn rows(icon: Icon) -> [&'static str; 12] {
    match icon {
        Icon::Flame => [
            ".....X......",
            ".....XX.....",
            "....XXX.....",
            "....XLX..X..",
            "...XXLXX.X..",
            "..XXLLLXXX..",
            "..XLLLLLXX..",
            ".XXLLWLLLXX.",
            ".XLLWWWLLLX.",
            ".XLLWWWWLLX.",
            "..XLLWWLLX..",
            "...XXXXXX...",
        ],
        Icon::Hourglass => [
            ".DDDDDDDDDD.",
            "..XXXXXXXX..",
            "..X.LLLL.X..",
            "...X.LL.X...",
            "....X..X....",
            ".....XX.....",
            ".....XX.....",
            "....X..X....",
            "...X.LL.X...",
            "..XLLLLLLX..",
            "..XXXXXXXX..",
            ".DDDDDDDDDD.",
        ],
        Icon::Arrow => [
            "............",
            ".......X....",
            ".......XX...",
            ".......XXX..",
            "XXXXXXXXXXX.",
            "XXXXXXXXXXXX",
            "XXXXXXXXXXXX",
            "XXXXXXXXXXX.",
            ".......XXX..",
            ".......XX...",
            ".......X....",
            "............",
        ],
        Icon::Play => [
            "...X........",
            "...XX.......",
            "...XXX......",
            "...XLXX.....",
            "...XLLXX....",
            "...XLLLXX...",
            "...XLLLXX...",
            "...XLLXX....",
            "...XLXX.....",
            "...XXX......",
            "...XX.......",
            "...X........",
        ],
        Icon::Auto => [
            "...XXXXX....",
            "..XX...XX.X.",
            ".XX.....XXX.",
            ".X.....XXXX.",
            ".X..........",
            ".X..........",
            "..........X.",
            "..........X.",
            ".XXXX.....X.",
            ".XXX.....XX.",
            ".X.XX...XX..",
            "....XXXXX...",
        ],
        Icon::Question => [
            "...XXXXXX...",
            "..XXXXXXXX..",
            "..XX....XX..",
            "........XX..",
            ".......XXX..",
            "......XXX...",
            ".....XXX....",
            ".....XX.....",
            ".....XX.....",
            "............",
            ".....XX.....",
            ".....XX.....",
        ],
        Icon::Bag => [
            "....DDDD....",
            "...D....D...",
            "...D....D...",
            "..XXXXXXXX..",
            ".XXLLLLLLXX.",
            ".XLLXXXXLLX.",
            ".XLXXDDXXLX.",
            ".XLXXXXXXLX.",
            ".XXXXXXXXXX.",
            ".XXXXXXXXXX.",
            ".DXXXXXXXXD.",
            "..DDDDDDDD..",
        ],
        Icon::Hammer => [
            "............",
            ".XXXXXXXX...",
            ".XLLLLLLXX..",
            ".XXXXXXXXX..",
            "..XXXXXXX...",
            ".....BB.....",
            ".....BB.....",
            ".....BB.....",
            ".....BB.....",
            ".....bB.....",
            ".....bB.....",
            ".....bb.....",
        ],
        Icon::House => [
            ".....XX.....",
            "....XXXX....",
            "...XXLLXX...",
            "..XXLLLLXX..",
            ".XXLLLLLLXX.",
            "XXXXXXXXXXXX",
            ".XLLLLLLLLX.",
            ".XLDDLLLLLX.",
            ".XLDDLLDDLX.",
            ".XLLLLLDDLX.",
            ".XLLLLLDDLX.",
            ".XXXXXXXXXX.",
        ],
        Icon::Book => [
            "............",
            ".XXXX..XXXX.",
            "XLLLLXXLLLLX",
            "XLDDLXXLDDLX",
            "XLLLLXXLLLLX",
            "XLDDLXXLDDLX",
            "XLLLLXXLLLLX",
            "XLDDLXXLDDLX",
            "XLLLLXXLLLLX",
            "XXXXXXXXXXXX",
            ".....XX.....",
            "............",
        ],
        Icon::Close => [
            "............",
            ".XX......XX.",
            ".XXX....XXX.",
            "..XXX..XXX..",
            "...XXXXXX...",
            "....XXXX....",
            "....XXXX....",
            "...XXXXXX...",
            "..XXX..XXX..",
            ".XXX....XXX.",
            ".XX......XX.",
            "............",
        ],
        Icon::Thermo => [
            ".....XX.....",
            "....X..X....",
            "....X..X.D..",
            "....XLLX....",
            "....XLLX.D..",
            "....XLLX....",
            "....XLLX.D..",
            "...XXLLXX...",
            "..XLLLLLLX..",
            "..XLLWWLLX..",
            "..XLLLLLLX..",
            "...XXXXXX...",
        ],
        Icon::Clock => [
            "...XXXXXX...",
            "..X......X..",
            ".X...L....X.",
            "X....L.....X",
            "X....L.....X",
            "X....LLLL..X",
            "X..........X",
            "X..........X",
            ".X........X.",
            "..X......X..",
            "...XXXXXX...",
            "............",
        ],
        Icon::Warn => [
            ".....XX.....",
            ".....XX.....",
            "....XXXX....",
            "....XKKX....",
            "...XXKKXX...",
            "...XXKKXX...",
            "..XXXKKXXX..",
            "..XXXKKXXX..",
            ".XXXXXXXXXX.",
            ".XXXXKKXXXX.",
            "XXXXXXXXXXXX",
            "XXXXXXXXXXXX",
        ],
        Icon::Load => [
            ".....XX.....",
            ".....XX.....",
            ".....XX.....",
            "..XX.XX.XX..",
            "...XXXXXX...",
            "....XXXX....",
            ".....XX.....",
            "X..........X",
            "X..........X",
            "XLLLLLLLLLLX",
            "XXXXXXXXXXXX",
            "DDDDDDDDDDDD",
        ],
        Icon::Take => [
            ".....XX.....",
            "....XXXX....",
            "...XXXXXX...",
            "..XX.XX.XX..",
            ".....XX.....",
            ".....XX.....",
            ".....XX.....",
            "X..........X",
            "X..........X",
            "XLLLLLLLLLLX",
            "XXXXXXXXXXXX",
            "DDDDDDDDDDDD",
        ],
        Icon::Turn => [
            "....XXXX....",
            "..XXXXXXXX..",
            ".XXX....XXX.",
            ".XX......XX.",
            "XX........XX",
            "XX........XX",
            "XX.....XXXXX",
            "XX......XXX.",
            ".XX......X..",
            ".XXX........",
            "..XXXXX.....",
            "....XXX.....",
        ],
        Icon::Move => [
            ".....XX.....",
            "....XXXX....",
            "...XXXXXX...",
            ".X...XX...X.",
            "XX...XX...XX",
            "XXXXXXXXXXXX",
            "XXXXXXXXXXXX",
            "XX...XX...XX",
            ".X...XX...X.",
            "...XXXXXX...",
            "....XXXX....",
            ".....XX.....",
        ],
    }
}

fn family_rows(f: Family) -> [&'static str; 12] {
    match f {
        Family::Fire => [
            ".....X......",
            "....XX......",
            "....XLX.....",
            "...XLLX.X...",
            "...XLWLXX...",
            "..XLLWWLX...",
            "..XLWWWLX...",
            "...XLWLX....",
            ".SSsSSSSsSS.",
            "SSSSsSSsSSSS",
            ".ssssssssss.",
            "............",
        ],
        Family::Kiln => [
            "............",
            "....SSSS....",
            "..SSSSSSSS..",
            ".SSSsSSsSSS.",
            ".SSSSSSSSSS.",
            "SSSSSSSSSSSS",
            "SSSSKKKKSSSS",
            "SSSKXXXXKSSS",
            "SSSKXLLXKSSS",
            "SSSKXLWXKSSS",
            "SSSKXXXXKSSS",
            "ssssssssssss",
        ],
        Family::Furnace => [
            "....SSSS....",
            "....sSSs....",
            "....SSSS....",
            "...SSSSSS...",
            "...SsSSsS...",
            "..SSSSSSSS..",
            "..SSSSSSSS..",
            ".SSSKKKKSSS.",
            ".SSKXLLXKSS.",
            ".SSKLWWLKSS.",
            ".SSKXXXXKSS.",
            "ssssssssssss",
        ],
        Family::Smithing => [
            "......SSS...",
            "......SSS...",
            ".......B....",
            "........B...",
            ".........B..",
            ".LLLLLLLLLL.",
            "XXXXXXXXXXXX",
            ".XXXXXXXXXX.",
            "....XXXX....",
            "....XXXX....",
            "...XXXXXX...",
            "..DDDDDDDD..",
        ],
        Family::Grinding => [
            "...SSSSSS...",
            "..SSSSSSSS..",
            ".SSSsSSsSSS.",
            ".SSSSKKSSSS.",
            ".SSSSKKSSSS.",
            ".SSSsSSsSSS.",
            "..SSSSSSSS..",
            "...SSSSSS...",
            "............",
            ".XX..XX..XX.",
            "X..XX..XX..X",
            "............",
        ],
        Family::Bench => [
            "............",
            ".S........S.",
            ".SS......SS.",
            "..S......S..",
            "..S..SS..S..",
            "XXXXXXXXXXXX",
            "LLLLLLLLLLLL",
            "XXXXXXXXXXXX",
            ".D........D.",
            ".D........D.",
            ".D........D.",
            ".D........D.",
        ],
        Family::Textile => [
            "BBBBBBBBBBBB",
            "B.L.L.L.L.LB",
            "B.L.L.L.L.LB",
            "BXXXXXXXXXXB",
            "B.L.L.L.L.LB",
            "BXXXXXXXXXXB",
            "B.L.L.L.L.LB",
            "BXXXXXXXXXXB",
            "B.L.L.L.L.LB",
            "B.L.L.L.L.LB",
            "BBBBBBBBBBBB",
            "B..........B",
        ],
        Family::MachineTool => [
            "............",
            ".SSSS.......",
            ".SKKS.......",
            ".SSSSXXXXXXX",
            ".SSSS.......",
            ".SSSS...SS..",
            ".SSSS...SS..",
            "XXXXXXXXXXXX",
            "XLLLLLLLLLLX",
            "XXXXXXXXXXXX",
            ".D........D.",
            ".D........D.",
        ],
        Family::Storage => [
            "BBBBBBBBBBBB",
            "BLLLLBLLLLLB",
            "BLXXLBLXXXLB",
            "BLLLLBLLLLLB",
            "BBBBBBBBBBBB",
            "BLLLLLBLLLLB",
            "BLXXXLBLXXLB",
            "BLLLLLBLLLLB",
            "BBBBBBBBBBBB",
            "B..........B",
            "B..........B",
            "............",
        ],
        Family::Growing => [
            "............",
            "..X....X....",
            ".XLX..XLX...",
            "..X.X..X.X..",
            "..X..X.X..X.",
            "..X....X....",
            "..X....X....",
            "BBBBBBBBBBBB",
            "bbbbbbbbbbbb",
            "BBBBBBBBBBBB",
            "bbbbbbbbbbbb",
            "............",
        ],
        Family::Electric => [
            "SSSSSSSSSSSS",
            "SKKKKKKKKKKS",
            "SKLLKLLLKKKS",
            "SKKKKKKKKKKS",
            "SKLLLKLLKKKS",
            "SKKKKKKKKKKS",
            "SKXKKKKKKKKS",
            "SSSSSSSSSSSS",
            "....SSSS....",
            "....SSSS....",
            "..ssssssss..",
            "............",
        ],
    }
}

fn colour(ch: char, col: Color32) -> Option<Color32> {
    Some(match ch {
        'X' => col,
        'L' => mix(col, Color32::WHITE, 0.4),
        'W' => mix(col, Color32::WHITE, 0.78),
        'D' => mix(col, Color32::BLACK, 0.45),
        'K' => Color32::from_rgb(22, 18, 14),
        'S' => Color32::from_rgb(156, 152, 144),
        's' => Color32::from_rgb(92, 90, 86),
        'B' => Color32::from_rgb(140, 100, 60),
        'b' => Color32::from_rgb(92, 64, 38),
        _ => return None,
    })
}

/// Paints a 12 x 12 bitmap centred in `r`, each cell a whole number of interface pixels when it fits.
fn bitmap(p: &Painter, r: Rect, rows: &[&str; 12], col: Color32) {
    let side = r.width().min(r.height());
    let fit = side / 12.0;
    let cell = if fit >= unit() { (fit / unit()).floor() * unit() } else { fit };
    let size = cell * 12.0;
    let o = snap_pos(r.center() - Vec2::splat(size / 2.0));
    let mut shapes = Vec::new();
    for (j, row) in rows.iter().enumerate() {
        let chars: Vec<char> = row.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let ch = chars[i];
            let mut end = i + 1;
            while end < chars.len() && chars[end] == ch {
                end += 1;
            }
            if let Some(c) = colour(ch, col) {
                let rr = Rect::from_min_max(egui::pos2(o.x + i as f32 * cell, o.y + j as f32 * cell), egui::pos2(o.x + end as f32 * cell, o.y + (j + 1) as f32 * cell));
                shapes.push(egui::Shape::rect_filled(rr, 0.0, c));
            }
            i = end;
        }
    }
    p.extend(shapes);
}

/// Draws `icon` in `col`, fitted into `r`.
pub(crate) fn icon(p: &Painter, r: Rect, icon: Icon, col: Color32) {
    bitmap(p, r, &rows(icon), col);
}

/// Draws the little picture of a machine family, in its accent colour.
pub(crate) fn family_icon(p: &Painter, r: Rect, f: Family) {
    bitmap(p, r, &family_rows(f), super::accent(f));
}

/// The colour an icon takes on an era's buttons.
pub(crate) fn icon_colour(era: Era) -> Color32 {
    match era {
        Era::Stone => Color32::from_rgb(240, 214, 160),
        Era::Metal => Color32::from_rgb(226, 222, 212),
        Era::Machine => Color32::from_rgb(220, 236, 250),
        Era::Computer => palette(era).ink,
    }
}

pub(crate) const BUTTON: f32 = 44.0;

/// A square button with a pixel picture and no text: `tip` is its hover text and its accessible name.
pub(crate) fn icon_button(ui: &mut Ui, ic: Icon, tip: &str, selected: bool, era: Era) -> Response {
    picture_button(ui, tip, selected, era, None, |p, r, col| icon(p, r, ic, col))
}

/// As `icon_button`, with a machine family's picture and accent.
pub(crate) fn family_button(ui: &mut Ui, f: Family, tip: &str, selected: bool, era: Era) -> Response {
    picture_button(ui, tip, selected, era, Some(super::accent(f)), |p, r, _| family_icon(p, r, f))
}

fn picture_button(ui: &mut Ui, tip: &str, selected: bool, era: Era, accent: Option<Color32>, draw: impl FnOnce(&Painter, Rect, Color32)) -> Response {
    let enabled = ui.is_enabled();
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(BUTTON), if enabled { Sense::click() } else { Sense::hover() });
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, tip));
    crate::pilot::mark(ui.ctx(), || tip.to_string(), rect);
    let pal = palette(era);
    let p = ui.painter();
    // a selected button is the slot ground lit up, framed in the accent (or the era's light)
    super::slot_frame(p, rect, era, crate::inv::SlotLook::Normal);
    if selected {
        let lit = Rect::from_min_max(rect.min + Vec2::splat(2.0 * unit()), rect.max - Vec2::splat(2.0 * unit()));
        p.rect_filled(crate::pxui::snap_rect(lit), 0.0, mix(pal.slot, accent.unwrap_or(pal.light), 0.28));
        super::highlight(p, rect, accent.unwrap_or(pal.light));
    } else if resp.hovered() && enabled {
        super::highlight(p, rect, mix(pal.light, pal.ink, 0.3));
    }
    let held = resp.is_pointer_button_down_on() && enabled;
    let inner = rect.shrink(8.0).translate(Vec2::new(0.0, if held { unit() } else { 0.0 }));
    let col = if enabled { icon_colour(era) } else { mix(icon_colour(era), pal.slot, 0.6) };
    draw(p, inner, col);
    if !enabled {
        p.rect_filled(rect, 0.0, Color32::from_black_alpha(90));
    }
    resp.on_hover_text(tip)
}
