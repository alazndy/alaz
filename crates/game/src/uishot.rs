//! `alaz --ui-shot <folder>`: draws the inventory window offscreen (no window, no GPU) for every era and tab and
//! saves `<era>-<tab>.png`, 1280 x 800, so the look can be checked without playing to that era.
//! The interface is tessellated by egui and filled by a small software rasterizer, with the font atlas sampled so
//! text shows.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use alaz_render::assets;
use alaz_render::egui::{self, epaint::Primitive, Color32, TextureId};
use alaz_sim::{AgentId, Building, BuildingKind, ItemKind, World};

use crate::hud::{KnapState, Selection, Tab, UiCtx};

pub(crate) const SHOT_W: u32 = 1280;
pub(crate) const SHOT_H: u32 = 800;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ShotTab {
    Bag,
    Craft,
    Build,
    Machine,
    /// The book open beside the Bag tab.
    Book,
}

impl ShotTab {
    pub(crate) const ALL: [ShotTab; 5] = [ShotTab::Bag, ShotTab::Craft, ShotTab::Build, ShotTab::Machine, ShotTab::Book];

    fn name(self) -> &'static str {
        match self {
            ShotTab::Bag => "bag",
            ShotTab::Craft => "craft",
            ShotTab::Build => "build",
            ShotTab::Machine => "machine",
            ShotTab::Book => "book",
        }
    }
}

/// The demo world: a player with a full bag beside a lit furnace, a campfire and a depot; every recipe known.
/// Returns the world and the furnace's index.
pub(crate) fn demo_world() -> (World, usize) {
    let mut w = World::new(5);
    let (sx, sy) = w.spawn_mm;
    w.spawn_agent(sx, sy);
    w.learn_everything();
    for (k, n) in [
        (ItemKind::Stone, 12),
        (ItemKind::Wood, 8),
        (ItemKind::Flint, 3),
        (ItemKind::Berries, 6),
        (ItemKind::Clay, 4),
        (ItemKind::Charcoal, 9),
        (ItemKind::CopperOre, 7),
        (ItemKind::TinOre, 2),
        (ItemKind::RawMeat, 2),
        (ItemKind::Fiber, 5),
        (ItemKind::Hide, 1),
        (ItemKind::Hammer, 1),
    ] {
        w.agents[0].inventory.counts[k as usize] = n;
        w.mark_seen(k);
    }
    for k in [ItemKind::Bronze, ItemKind::CookedMeat] {
        w.mark_seen(k);
    }
    let (x, y) = (w.agents[0].x, w.agents[0].y);
    let mut furnace = Building::new(BuildingKind::Furnace, x + 1_500, y);
    furnace.input[ItemKind::CopperOre as usize] = 4;
    furnace.input[ItemKind::Charcoal as usize] = 2;
    furnace.output[ItemKind::Bronze as usize] = 2;
    furnace.temp = 900;
    w.buildings.push(furnace);
    let i = w.buildings.len() - 1;
    let mut fire = Building::new(BuildingKind::Campfire, x - 1_500, y);
    fire.fuel = 600;
    w.buildings.push(fire);
    let mut depot = Building::new(BuildingKind::Depot, x, y + 2_000);
    depot.output[ItemKind::Berries as usize] = 5;
    w.buildings.push(depot);
    (w, i)
}

/// Until the machine tab fills the info panel itself, the machine shots show what `draw_lines` makes of the
/// furnace's bronze recipe in each era. Every number comes from the recipe and the building.
fn sample_info(ctx: &egui::Context, w: &World, furnace: usize) {
    use crate::style::lines::{draw_lines, Chip, Line, Picto};
    use crate::style::Era;
    let Some(regs) = crate::inv::regions(ctx) else { return };
    let Some(r) = alaz_sim::rules().recipes.iter().find(|r| r.station == Some(BuildingKind::Furnace) && r.outputs.iter().any(|o| o.item == ItemKind::Bronze)) else { return };
    let b = &w.buildings[furnace];
    let era = crate::style::current_era();
    let secs = (r.ticks as u64 / alaz_sim::time::TICKS_PER_SECOND).max(1) as u32;
    let out_n: u16 = r.outputs.iter().map(|o| o.n).sum();
    let item_chips = || r.inputs.iter().map(|i| Chip::Item(i.item, i.n)).collect::<Vec<_>>();
    let lines = match era {
        Era::Stone => {
            let mut row: Vec<Picto> = r.inputs.iter().map(|i| Picto::Item(i.item)).collect();
            row.push(Picto::Icon(crate::style::icons::Icon::Arrow));
            row.extend(r.outputs.iter().map(|o| Picto::Item(o.item)));
            vec![Line::Pictos(row), Line::Pictos(vec![Picto::Flames { lit: 3, of: 4 }]), Line::Pictos(vec![Picto::Glasses { lit: 2, of: 3 }])]
        }
        Era::Metal => {
            let mut v = vec![Line::Title(r.name.clone()), Line::Label("gerek"), Line::Chips(item_chips()), Line::Label("ısı"), Line::Chips(vec![Chip::HeatWord(crate::hud::heat_word(r.heat))])];
            if let Some(note) = crate::hud::machine_note(BuildingKind::Furnace) {
                v.push(Line::Label("not"));
                v.push(Line::Text(note.to_string()));
            }
            v
        }
        Era::Machine => {
            let mut v = vec![
            Line::Title(r.name.clone()),
            Line::Label("girdi"),
            Line::Chips(item_chips()),
            Line::Label("koşul"),
            Line::Chips(vec![Chip::HeatC { need: r.heat, now: Some(b.temp) }, Chip::Secs(secs)]),
            Line::Label("hız"),
            Line::Chips(vec![Chip::PerMin(out_n as f32 * 60.0 / secs as f32)]),
            ];
            if let Some(req) = b.kind.requirement() {
                v.push(Line::Warn(crate::hud::requirement_note(req).to_string()));
            }
            v
        }
        Era::Computer => {
            let ins: Vec<String> = r.inputs.iter().map(|i| format!("{:?}×{}", i.item, i.n).to_lowercase()).collect();
            vec![
                Line::Title(r.name.clone()),
                Line::Mono(vec![
                    r.id.to_uppercase(),
                    format!("IN  {}", ins.join(" ")),
                    format!("T   {}/{}°C", b.temp, r.heat),
                    format!("t   {secs}s"),
                    format!("OUT {:.1}/min", out_n as f32 * 60.0 / secs as f32),
                    format!("> auto: {}", if b.auto.is_some() { "ON" } else { "OFF" }),
                ]),
            ]
        }
    };
    // clear of the frame's rim and lacing
    let inner = regs.info.shrink(22.0);
    egui::Area::new(egui::Id::NULL.with("uishot_info")).order(egui::Order::Foreground).fade_in(false).fixed_pos(inner.min).show(ctx, |ui| {
        ui.set_width(inner.width());
        ui.set_max_width(inner.width());
        crate::style::apply_visuals(ui.visuals_mut(), era);
        draw_lines(ui, w, era, &lines);
    });
}

/// Textures by id, kept across frames: egui sends the font atlas once and later only patches.
type Textures = HashMap<TextureId, (usize, usize, Vec<Color32>)>;

fn apply_textures(tex: &mut Textures, delta: &egui::TexturesDelta) {
    for (id, d) in delta.set.iter().flat_map(|(id, ds)| ds.iter().map(move |d| (id, d))) {
        let egui::ImageData::Color(img) = &d.image;
        let [w, h] = img.size;
        match d.pos {
            None => {
                tex.insert(*id, (w, h, img.pixels.clone()));
            }
            Some([x0, y0]) => {
                if let Some((tw, _, px)) = tex.get_mut(id) {
                    for y in 0..h {
                        for x in 0..w {
                            if let Some(t) = px.get_mut((y0 + y) * *tw + x0 + x) {
                                *t = img.pixels[y * w + x];
                            }
                        }
                    }
                }
            }
        }
    }
    for id in &delta.free {
        tex.remove(id);
    }
}

fn sample(tex: &Textures, id: TextureId, u: f32, v: f32) -> [f32; 4] {
    let Some((w, h, px)) = tex.get(&id) else { return [255.0; 4] };
    let x = ((u * *w as f32) as isize).clamp(0, *w as isize - 1) as usize;
    let y = ((v * *h as f32) as isize).clamp(0, *h as isize - 1) as usize;
    let c = px[y * w + x].to_array();
    [c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32]
}

/// Fills the triangles into a premultiplied RGBA canvas of `SHOT_W` x `SHOT_H` pixels.
fn rasterize(canvas: &mut [[f32; 4]], prims: &[egui::ClippedPrimitive], tex: &Textures, ppp: f32, size: (u32, u32)) {
    let (dw, dh) = (size.0 as usize, size.1 as usize);
    for prim in prims {
        let Primitive::Mesh(mesh) = &prim.primitive else { continue };
        let clip = prim.clip_rect;
        let (cx0, cy0) = ((clip.min.x * ppp).round().max(0.0) as usize, (clip.min.y * ppp).round().max(0.0) as usize);
        let (cx1, cy1) = (((clip.max.x * ppp).round().max(0.0) as usize).min(dw), ((clip.max.y * ppp).round().max(0.0) as usize).min(dh));
        if cx0 >= cx1 || cy0 >= cy1 {
            continue;
        }
        for tri in mesh.indices.as_chunks::<3>().0 {
            let v: [_; 3] = std::array::from_fn(|i| mesh.vertices[tri[i] as usize]);
            let p: [[f32; 2]; 3] = std::array::from_fn(|i| [v[i].pos.x * ppp, v[i].pos.y * ppp]);
            let area = (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]);
            if area.abs() < 1e-9 {
                continue;
            }
            let lo = |k: usize| p.iter().map(|q| q[k]).fold(f32::MAX, f32::min).floor().max(0.0) as usize;
            let hi = |k: usize| p.iter().map(|q| q[k]).fold(f32::MIN, f32::max).ceil().max(0.0) as usize;
            let (x0, x1) = (lo(0).max(cx0), hi(0).min(cx1));
            let (y0, y1) = (lo(1).max(cy0), hi(1).min(cy1));
            for y in y0..y1 {
                for x in x0..x1 {
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    let w0 = ((p[1][0] - px) * (p[2][1] - py) - (p[2][0] - px) * (p[1][1] - py)) / area;
                    let w1 = ((p[2][0] - px) * (p[0][1] - py) - (p[0][0] - px) * (p[2][1] - py)) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                        continue;
                    }
                    let u = w0 * v[0].uv.x + w1 * v[1].uv.x + w2 * v[2].uv.x;
                    let t = w0 * v[0].uv.y + w1 * v[1].uv.y + w2 * v[2].uv.y;
                    let texel = sample(tex, mesh.texture_id, u, t);
                    // vertex colour and texel are both premultiplied: the product is too
                    let c: [f32; 4] = std::array::from_fn(|k| {
                        let ch = |col: Color32| col.to_array()[k] as f32;
                        (w0 * ch(v[0].color) + w1 * ch(v[1].color) + w2 * ch(v[2].color)) * texel[k] / 255.0
                    });
                    let d = &mut canvas[y * dw + x];
                    let inv = 1.0 - c[3] / 255.0;
                    for k in 0..4 {
                        d[k] = c[k] + d[k] * inv;
                    }
                }
            }
        }
    }
}

/// Keeps one egui context per thread: the HUD sets its fonts once per thread, so a second context would get none.
struct Shooter {
    ctx: egui::Context,
    tex: Textures,
}

thread_local! {
    static SHOOTER: std::cell::RefCell<Option<Shooter>> = const { std::cell::RefCell::new(None) };
}

/// Draws the window for perception `level` on `tab` and writes `<dir>/<level>-<tab>.png`.
pub(crate) fn shoot(dir: &Path, level: u8, tab: ShotTab) -> std::io::Result<PathBuf> {
    let (w, furnace) = demo_world();
    let (ui_tab, selection) = match tab {
        ShotTab::Bag | ShotTab::Book => (Some(Tab::Bag), None),
        ShotTab::Craft => (Some(Tab::Craft), None),
        ShotTab::Build => (Some(Tab::Build), None),
        ShotTab::Machine => (None, Some(Selection::Building(furnace))),
    };
    let binds = crate::keys::Keybinds::default();
    let ui_ctx = UiCtx {
        hints: &[],
        build_mode: None,
        grid_snap: false,
        selection,
        recording: false,
        recorded: &[],
        me: AgentId(0),
        net_status: "",
        last_strike: None,
        tab: ui_tab,
        binds: &binds,
        anchor: Some([0.0, 0.0]),
        grip: true,
        time_scale: 1,
        evo: 9.0,
        yaw: 0.0,
        fade: 0.0,
    };
    let mut knap = KnapState::default();
    if tab == ShotTab::Book {
        knap.journal.book_open = true;
        knap.journal.section = 2;
    }
    // filled hands, so the info panel has something to show
    let instant = alaz_sim::rules().recipes.iter().find(|r| r.station.is_none() && r.ticks == 0 && w.knows(r.discovery) && !r.inputs.is_empty() && r.inputs.iter().map(|c| c.n).sum::<u16>() <= 4);
    if let (ShotTab::Craft, Some(r)) = (tab, instant) {
        let items = r.inputs.iter().flat_map(|c| std::iter::repeat_n(c.item as u8, c.n as usize));
        for (slot, item) in knap.hands.iter_mut().zip(items) {
            *slot = item;
        }
    }
    crate::style::force_level(Some(level));
    let rgba = SHOOTER.with(|cell| {
        let mut cell = cell.borrow_mut();
        let sh = cell.get_or_insert_with(|| Shooter { ctx: egui::Context::default(), tex: HashMap::new() });
        if tab == ShotTab::Build {
            crate::inv::build::pick_for_shot(&sh.ctx, BuildingKind::Furnace);
        }
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(SHOT_W as f32, SHOT_H as f32));
        let mut last = None;
        // areas are sized on their first frame and shown from the next; fonts and zoom also settle in a frame or two
        for _ in 0..4 {
            let mut acts = Vec::new();
            let input = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
            let out = sh.ctx.run_ui(input, |ui| {
                crate::hud::draw(ui.ctx(), &w, &ui_ctx, &mut knap, &mut acts);
                if tab == ShotTab::Machine {
                    sample_info(ui.ctx(), &w, furnace);
                }
            });
            apply_textures(&mut sh.tex, &out.textures_delta);
            last = Some(out);
        }
        let out = last.expect("four frames ran");
        let ppp = out.pixels_per_point;
        let prims = sh.ctx.tessellate(out.shapes, ppp);
        // a muted ground behind the interface, as the world would be
        let mut canvas = vec![[58.0f32, 72.0, 60.0, 255.0]; (SHOT_W * SHOT_H) as usize];
        rasterize(&mut canvas, &prims, &sh.tex, ppp, (SHOT_W, SHOT_H));
        let mut rgba = Vec::with_capacity(canvas.len() * 4);
        for c in canvas {
            for ch in &c[..3] {
                rgba.push(ch.round().clamp(0.0, 255.0) as u8);
            }
            rgba.push(255);
        }
        rgba
    });
    crate::style::force_level(None);
    let path = dir.join(format!("{level}-{}.png", tab.name()));
    std::fs::write(&path, assets::encode_png(SHOT_W, SHOT_H, &rgba))?;
    Ok(path)
}

const SHEET_W: u32 = 2016;
const SHEET_H: u32 = 1180;

/// Draws every machine family in `level`'s era, idle and working, plus one blocked, and writes
/// `<dir>/machines-<era>.png`. A bigger canvas than the window shots: the drawings are 56 x 40 cells.
pub(crate) fn shoot_machines(dir: &Path, level: u8) -> std::io::Result<PathBuf> {
    use crate::style::machines::{draw_machine, MachineState};
    use crate::style::{accent, family_name, palette, Family};
    let fams = [Family::Fire, Family::Kiln, Family::Furnace, Family::Smithing, Family::Grinding, Family::Bench, Family::Textile, Family::MachineTool, Family::Storage, Family::Growing, Family::Electric];
    let era = crate::style::era_of(level);
    let pal = palette(era);
    let idle = MachineState { fuel: 0.0, glow: 0.0, working: false, blocked: false, t: 0.0 };
    let busy = MachineState { fuel: 0.8, glow: 0.9, working: true, blocked: false, t: 0.35 };
    let blocked = MachineState { fuel: 0.0, glow: 0.0, working: false, blocked: true, t: 0.0 };
    // tiles in reading order: idle then working per family, then one blocked furnace
    let mut tiles: Vec<(Family, &str, &MachineState)> = Vec::new();
    for f in fams {
        tiles.push((f, "bos", &idle));
        tiles.push((f, "calisiyor", &busy));
    }
    tiles.push((Family::Furnace, "tikali", &blocked));
    let (cols, tw, th) = (6usize, 336.0f32, 288.0f32);
    let ctx = egui::Context::default();
    let mut tex = HashMap::new();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(SHEET_W as f32, SHEET_H as f32));
    let mut last = None;
    for _ in 0..3 {
        let input = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
        let out = ctx.run_ui(input, |ui| {
            let p = ui.painter().clone();
            p.rect_filled(screen, 0.0, pal.base);
            for (i, (fam, label, st)) in tiles.iter().enumerate() {
                let (col, row) = (i % cols, i / cols);
                let cell = egui::Rect::from_min_size(egui::pos2(col as f32 * tw, row as f32 * th), egui::vec2(tw, th)).shrink(3.0);
                crate::style::well(&p, cell, era);
                let art = egui::Rect::from_min_size(cell.min + egui::vec2(0.0, 24.0), egui::vec2(cell.width(), cell.height() - 24.0));
                draw_machine(&p, art, *fam, era, st);
                p.text(cell.min + egui::vec2(8.0, 3.0), egui::Align2::LEFT_TOP, format!("{} / {label}", family_name(*fam)), egui::FontId::proportional(18.0), accent(*fam));
            }
        });
        apply_textures(&mut tex, &out.textures_delta);
        last = Some(out);
    }
    let out = last.expect("frames ran");
    let ppp = out.pixels_per_point;
    let prims = ctx.tessellate(out.shapes, ppp);
    let mut canvas = vec![[0.0f32, 0.0, 0.0, 255.0]; (SHEET_W * SHEET_H) as usize];
    rasterize(&mut canvas, &prims, &tex, ppp, (SHEET_W, SHEET_H));
    let mut rgba = Vec::with_capacity(canvas.len() * 4);
    for c in canvas {
        for ch in &c[..3] {
            rgba.push(ch.round().clamp(0.0, 255.0) as u8);
        }
        rgba.push(255);
    }
    let name = ["stone", "metal", "machine", "computer"][era as usize];
    let path = dir.join(format!("machines-{name}.png"));
    std::fs::write(&path, assets::encode_png(SHEET_W, SHEET_H, &rgba))?;
    Ok(path)
}

/// Runs every era and tab into `dir`, printing what it wrote.
pub fn run(dir: &Path) {
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("{}: {e}", dir.display());
        std::process::exit(1);
    }
    assets::pause(true);
    let mut n = 0;
    for level in 0..=5u8 {
        for tab in ShotTab::ALL {
            match shoot(dir, level, tab) {
                Ok(p) => {
                    println!("{}", p.display());
                    n += 1;
                }
                Err(e) => {
                    eprintln!("{level}-{}: {e}", tab.name());
                    std::process::exit(1);
                }
            }
        }
    }
    for level in [0u8, 2, 4, 5] {
        match shoot_machines(dir, level) {
            Ok(p) => {
                println!("{}", p.display());
                n += 1;
            }
            Err(e) => {
                eprintln!("machines {level}: {e}");
                std::process::exit(1);
            }
        }
    }
    assets::pause(false);
    println!("{n} görüntü yazıldı: {}", dir.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uishot_writes_pngs() {
        let dir = std::env::temp_dir().join(format!("alaz-uishot-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = shoot(&dir, 0, ShotTab::Bag).expect("shot");
        assert_eq!(path, dir.join("0-bag.png"));
        let img = assets::decode_png(&std::fs::read(&path).expect("png written")).expect("decodes");
        assert_eq!((img.w, img.h), (SHOT_W, SHOT_H));
        let (px, _) = img.rgba.as_chunks::<4>();
        assert!(px.iter().any(|p| p != &px[0]), "the shot is one colour");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
