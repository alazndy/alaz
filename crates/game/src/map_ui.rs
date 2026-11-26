//! The surface map on the screen: the small one in the corner and the large one in its own window.

use crate::hud::{node_name, panel};
use crate::surface_map::{CHUNK, Records, SurfaceMap, Tile, in_sight, mapped, tile_of};
use crate::ui_art::{Glyph, glyph};
use alaz_render::egui::{self, Align2, Color32, ColorImage, Pos2, Rect, RichText, Shape, Stroke, TextureHandle, TextureOptions, Vec2};
use alaz_sim::agents::AgentKind;
use alaz_sim::map::{NodeKind, TILE_MM, Terrain, elevation_mm, water_depth};
use alaz_sim::world::Agent;
use alaz_sim::{BuildingKind, World};
use std::collections::{BTreeSet, HashMap};

/// Side of the small map, in points, and how many tiles it shows across.
pub(crate) const SMALL: f32 = 180.0;
const SMALL_TILES: f32 = 90.0;
/// How far inside the small map's edge a far camp's mark sits, leaving room for its arrow.
const CAMP_INSET: f32 = 18.0;
/// Points per tile on the large map.
const ZOOM_MIN: f32 = 0.25;
const ZOOM_MAX: f32 = 12.0;
const ZOOM_START: f32 = 3.0;
/// Below this zoom a resource is only a dot.
const ZOOM_DOTS: f32 = 1.5;
/// A chunk's picture asks the land generator about every tile: only a few are made in one frame.
const PICTURES_PER_FRAME: usize = 4;
const FOG: Color32 = Color32::from_rgb(14, 14, 18);

const KINDS: [NodeKind; 9] = [
    NodeKind::StoneOutcrop,
    NodeKind::ClayDeposit,
    NodeKind::CopperOutcrop,
    NodeKind::TinOutcrop,
    NodeKind::IronOutcrop,
    NodeKind::BerryBush,
    NodeKind::HerbPatch,
    NodeKind::FiberPlant,
    NodeKind::CaveMouth,
];

/// What the large map asks the game to do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum MapAct {
    Close,
    SetCamp(Option<Tile>),
}

/// How the player is looking at the map; the map itself is `SurfaceMap`.
#[derive(Default)]
pub(crate) struct MapView {
    pub open: bool,
    /// The tile in the middle of the large map; `None` follows the player.
    center: Option<(f32, f32)>,
    /// Points per tile on the large map; 0 until first used.
    zoom: f32,
    /// Resource names switched off.
    hidden: BTreeSet<&'static str>,
    /// Chunk pictures, with the version of the ground each was made from.
    cache: HashMap<Tile, (u32, TextureHandle)>,
    seed: u64,
}

impl MapView {
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.center = None;
    }

    /// Make the pictures that `chunks` need, a few at a time; returns how many are still missing or stale.
    fn pictures(&mut self, ctx: &egui::Context, world: &World, map: &SurfaceMap, chunks: &[Tile]) -> usize {
        if self.seed != world.seed {
            self.cache.clear();
            self.seed = world.seed;
        }
        let (mut made, mut waiting) = (0, 0);
        for &chunk in chunks {
            let version = map.chunk_version(chunk);
            if self.cache.get(&chunk).is_some_and(|(v, _)| *v == version) {
                continue;
            }
            if made == PICTURES_PER_FRAME {
                waiting += 1;
                continue;
            }
            made += 1;
            let image = chunk_image(world, map.records(), chunk);
            // sharp tiles up close, smooth when far enough out that tiles are smaller than pixels
            let options = TextureOptions { minification: egui::TextureFilter::Linear, ..TextureOptions::NEAREST };
            match self.cache.get_mut(&chunk) {
                Some((v, texture)) => {
                    texture.set(image, options);
                    *v = version;
                }
                None => {
                    let texture = ctx.load_texture(format!("map:{}:{}", chunk.0, chunk.1), image, options);
                    self.cache.insert(chunk, (version, texture));
                }
            }
        }
        waiting
    }
}

/// Screen offset (in tiles, y down) of a ground offset, with the camera turned by `yaw_deg`; as the cave arrow has it.
pub(crate) fn project(delta: Vec2, yaw_deg: f32) -> Vec2 {
    let (sin, cos) = yaw_deg.to_radians().sin_cos();
    egui::vec2(delta.x * cos - delta.y * sin, delta.x * sin + delta.y * cos)
}

/// Where on the edge of a square (half side `half`) an outside offset is shown; `None` when it is inside.
pub(crate) fn edge_point(offset: Vec2, half: f32) -> Option<Vec2> {
    let most = offset.x.abs().max(offset.y.abs());
    (most > half).then(|| offset * (half / most))
}

/// The arrow beside the mark of a camp that is off the small map, as offsets from the map's middle.
fn camp_arrow(edge: Vec2) -> [Vec2; 3] {
    let out = edge.normalized();
    let side = egui::vec2(-out.y, out.x);
    [edge + out * 14.0, edge + out * 7.0 + side * 5.0, edge + out * 7.0 - side * 5.0]
}

/// The new middle and zoom when the map is zoomed by `factor` about `pointer`: the ground under it stays put.
fn zoom_about(rect: Rect, pointer: Pos2, centre: (f32, f32), zoom: f32, factor: f32) -> ((f32, f32), f32) {
    let new = (zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
    let off = pointer - rect.center();
    let shift = off / zoom - off / new;
    ((centre.0 + shift.x, centre.1 + shift.y), new)
}

fn ground_color(world: &World, tx: i32, ty: i32) -> Color32 {
    let (r, g, b) = match world.map.terrain(tx, ty) {
        None => return FOG,
        Some(Terrain::Water) if water_depth(&world.map, tx, ty) > 1 => return Color32::from_rgb(34, 70, 128),
        Some(Terrain::Water) => return Color32::from_rgb(58, 112, 172),
        Some(Terrain::Sand) => (198, 180, 126),
        Some(Terrain::Grass) => (98, 142, 72),
        Some(Terrain::Forest) => (52, 96, 54),
        Some(Terrain::Rock) => (128, 120, 112),
    };
    // light from the north-west: ground that rises toward it is darker, ground that falls is lighter
    let seed = world.map.seed;
    let rise = elevation_mm(seed, tx, ty) - elevation_mm(seed, (tx - 1).max(0), (ty - 1).max(0));
    let light = (1.0 + rise as f32 / 3000.0).clamp(0.72, 1.28);
    let shade = |c: u8| (f32::from(c) * light).min(255.0) as u8;
    Color32::from_rgb(shade(r), shade(g), shade(b))
}

/// One pixel per tile of a chunk: the ground where it has been seen, fog elsewhere.
pub(crate) fn chunk_image(world: &World, records: &Records, chunk: Tile) -> ColorImage {
    let side = CHUNK as usize;
    let mut image = ColorImage::filled([side, side], FOG);
    let Some(rows) = records.explored.get(&chunk) else { return image };
    for (y, row) in rows.iter().enumerate() {
        for x in 0..side {
            if row >> x & 1 != 0 {
                image.pixels[y * side + x] = ground_color(world, chunk.0 * CHUNK + x as i32, chunk.1 * CHUNK + y as i32);
            }
        }
    }
    image
}

/// The names the marked resources go by now: ores are all plain rock until the eye can tell them apart.
fn resource_names(world: &World) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = Vec::new();
    for kind in KINDS {
        let name = node_name(world, kind);
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

fn resource_color(name: &str) -> Color32 {
    match name {
        "Taş yığını" => Color32::from_rgb(222, 222, 226),
        "Kaya" => Color32::from_rgb(172, 164, 160),
        "Kil yatağı" => Color32::from_rgb(226, 132, 66),
        "Bakır damarı" => Color32::from_rgb(244, 156, 100),
        "Kalay damarı" => Color32::from_rgb(150, 214, 232),
        "Demir damarı" => Color32::from_rgb(190, 72, 62),
        "Yemiş çalısı" => Color32::from_rgb(224, 62, 128),
        "Şifalı ot" => Color32::from_rgb(160, 248, 120),
        "Lif otu" | "Ot" => Color32::from_rgb(238, 224, 130),
        _ => Color32::from_rgb(24, 16, 30),
    }
}

/// How big a resource's dot is: small enough that the ground shows between them.
fn mark_radius(ppt: f32) -> f32 {
    (ppt * 0.6).clamp(1.0, 3.5)
}

fn building_mark(painter: &egui::Painter, at: Pos2, size: f32, kind: BuildingKind) {
    let (g, colour) = if kind.burns() {
        (Glyph::Flame, Color32::from_rgb(250, 140, 50))
    } else if kind == BuildingKind::Shelter {
        (Glyph::House, Color32::from_rgb(222, 176, 128))
    } else {
        (Glyph::Hammer, Color32::from_rgb(214, 214, 224))
    };
    glyph(painter, at, size, g, colour);
}

fn camp_mark(painter: &egui::Painter, at: Pos2) {
    painter.circle(at, 8.0, Color32::from_rgb(20, 16, 12), Stroke::new(1.5, Color32::from_rgb(250, 204, 80)));
    glyph(painter, at, 11.0, Glyph::House, Color32::from_rgb(250, 204, 80));
}

/// The player's position on the ground, in tiles.
fn tiles_of(a: &Agent) -> Vec2 {
    egui::vec2(a.x as f32 / TILE_MM as f32, a.y as f32 / TILE_MM as f32)
}

/// What one map shows: `centre` (tiles) in the middle of `rect`, `ppt` points to a tile, turned by `yaw`.
struct Window {
    rect: Rect,
    centre: Vec2,
    ppt: f32,
    yaw: f32,
}

impl Window {
    fn at(&self, ground: Vec2) -> Pos2 {
        self.rect.center() + project(ground - self.centre, self.yaw) * self.ppt
    }

    fn tile(&self, t: Tile) -> Pos2 {
        self.at(egui::vec2(t.0 as f32 + 0.5, t.1 as f32 + 0.5))
    }

    /// The tile under a point of the screen.
    fn tile_at(&self, p: Pos2) -> Tile {
        let ground = self.centre + project((p - self.rect.center()) / self.ppt, -self.yaw);
        (ground.x.floor() as i32, ground.y.floor() as i32)
    }
}

fn paint(painter: &egui::Painter, win: &Window, world: &World, me: &Agent, map: &SurfaceMap, view: &mut MapView) {
    let painter = painter.with_clip_rect(win.rect.intersect(painter.clip_rect()));
    painter.rect_filled(win.rect, 0.0, FOG);
    let records = map.records();
    // a circle around the middle that covers the rectangle however it is turned
    let reach = (win.rect.size().length() * 0.5 / win.ppt) as i32 + 1;
    let (cx, cy) = (win.centre.x as i32, win.centre.y as i32);
    let (x0, y0, x1, y1) = (cx - reach, cy - reach, cx + reach, cy + reach);
    let columns = (x0.div_euclid(CHUNK), i32::MIN)..=(x1.div_euclid(CHUNK), i32::MAX);
    let chunks: Vec<Tile> =
        records.explored.range(columns).map(|(&c, _)| c).filter(|c| c.1 >= y0.div_euclid(CHUNK) && c.1 <= y1.div_euclid(CHUNK)).collect();
    if view.pictures(painter.ctx(), world, map, &chunks) > 0 {
        painter.ctx().request_repaint();
    }
    for chunk in &chunks {
        let Some((_, texture)) = view.cache.get(chunk) else { continue };
        let mut mesh = egui::Mesh::with_texture(texture.id());
        for (u, v) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
            let corner = egui::vec2((chunk.0 as f32 + u) * CHUNK as f32, (chunk.1 as f32 + v) * CHUNK as f32);
            mesh.vertices.push(egui::epaint::Vertex { pos: win.at(corner), uv: egui::pos2(u, v), color: Color32::WHITE });
        }
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(Shape::mesh(mesh));
    }

    let seen = win.rect.expand(8.0);
    let (left, right) = ((x0, i32::MIN), (x1, i32::MAX));
    for (&tile, &kind) in records.resources.range(left..=right) {
        let p = win.tile(tile);
        if !seen.contains(p) || !mapped(kind) {
            continue;
        }
        let name = node_name(world, kind);
        if view.hidden.contains(name) {
            continue;
        }
        if win.ppt < ZOOM_DOTS {
            painter.rect_filled(Rect::from_center_size(p, Vec2::splat(2.0)), 0.0, resource_color(name));
        } else {
            let edge = if kind == NodeKind::CaveMouth { Color32::WHITE } else { Color32::from_black_alpha(190) };
            painter.circle(p, mark_radius(win.ppt), resource_color(name), Stroke::new(0.5, edge));
        }
    }
    for (&tile, &kind) in records.buildings.range(left..=right) {
        let p = win.tile(tile);
        if seen.contains(p) {
            building_mark(&painter, p, if win.ppt < ZOOM_DOTS { 8.0 } else { 13.0 }, kind);
        }
    }
    if let Some(camp) = records.camp {
        camp_mark(&painter, win.tile(camp));
    }
    // others are drawn only while they are in sight: the map keeps no news of them
    let here = tile_of(me.x, me.y);
    for a in &world.agents {
        if a.id != me.id && a.kind == AgentKind::Human && a.health > 0 && a.cave_view().is_none() && in_sight(here, tile_of(a.x, a.y)) {
            painter.circle(win.at(tiles_of(a)), 4.0, Color32::from_rgb(90, 170, 250), Stroke::new(1.0, Color32::BLACK));
        }
    }
    let (hx, hy) = map.heading();
    let ahead = project(egui::vec2(hx, hy), win.yaw);
    let side = egui::vec2(-ahead.y, ahead.x);
    let p = win.at(tiles_of(me));
    painter.add(Shape::convex_polygon(
        vec![p + ahead * 8.0, p - ahead * 5.0 + side * 5.0, p - ahead * 2.0, p - ahead * 5.0 - side * 5.0],
        Color32::from_rgb(255, 250, 240),
        Stroke::new(1.0, Color32::BLACK),
    ));
}

/// The small map in the top right corner, turned with the camera; returns the rectangle it takes.
pub(crate) fn draw_small(ctx: &egui::Context, world: &World, me: &Agent, map: &SurfaceMap, view: &mut MapView, yaw_deg: f32) -> Rect {
    egui::Area::new(egui::Id::NULL.with("minimap")).anchor(Align2::RIGHT_TOP, [-12.0, 12.0]).show(ctx, |ui| {
        panel(ui, 4.0, |ui| {
            ui.set_max_width(SMALL);
            let (resp, painter) = ui.allocate_painter(Vec2::splat(SMALL), egui::Sense::hover());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, "Küçük harita"));
            let win = Window { rect: resp.rect, centre: tiles_of(me), ppt: SMALL / SMALL_TILES, yaw: yaw_deg };
            paint(&painter, &win, world, me, map, view);
            if let Some(camp) = map.records().camp {
                let to = egui::vec2(camp.0 as f32 + 0.5, camp.1 as f32 + 0.5) - win.centre;
                let on_map = project(to, yaw_deg) * win.ppt;
                if let Some(edge) = edge_point(on_map, SMALL * 0.5 - CAMP_INSET) {
                    let middle = resp.rect.center();
                    camp_mark(&painter, middle + edge);
                    painter.add(Shape::convex_polygon(
                        camp_arrow(edge).map(|p| middle + p).to_vec(),
                        Color32::from_rgb(250, 204, 80),
                        Stroke::new(1.0, Color32::BLACK),
                    ));
                }
                ui.label(RichText::new(format!("Kamp: {:.0} m", to.length())).small());
            }
        });
    })
    .response
    .rect
}

/// The large map: north up, dragged and zoomed by the mouse.
pub(crate) fn draw_large(ctx: &egui::Context, world: &World, me: &Agent, map: &SurfaceMap, view: &mut MapView, key_label: &str) -> Vec<MapAct> {
    let mut acts = Vec::new();
    if view.zoom == 0.0 {
        view.zoom = ZOOM_START;
    }
    let screen = ctx.content_rect();
    let size = egui::vec2((screen.width() - 48.0).min(1100.0), (screen.height() - 48.0).min(820.0));
    let underground = me.cave_view().is_some();
    egui::Area::new(egui::Id::NULL.with("bigmap")).anchor(Align2::CENTER_CENTER, [0.0, 0.0]).order(egui::Order::Foreground).show(ctx, |ui| {
        panel(ui, 8.0, |ui| {
            ui.set_width(size.x - 16.0);
            ui.horizontal_wrapped(|ui| {
                crate::ui_art::heading(ui, Glyph::Map, "Harita", Color32::from_rgb(130, 200, 150));
                if ui.button("Beni ortala").clicked() {
                    view.center = None;
                }
                if ui.add_enabled(!underground, egui::Button::new("Kampımı buraya koy")).on_hover_text("Haritada sağ tık da olur").clicked() {
                    acts.push(MapAct::SetCamp(Some(tile_of(me.x, me.y))));
                }
                if ui.add_enabled(map.records().camp.is_some(), egui::Button::new("Kamp işaretini kaldır")).clicked() {
                    acts.push(MapAct::SetCamp(None));
                }
                if ui.button(format!("Kapat [{key_label}]")).clicked() {
                    acts.push(MapAct::Close);
                }
            });
            if underground {
                ui.label(RichText::new("Yer altındasın: harita yüzeyi gösterir.").small());
            }
            ui.separator();
            let height = (size.y - 130.0).max(160.0);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(150.0);
                    egui::ScrollArea::vertical().id_salt("map_side").max_height(height).show(ui, |ui| {
                        for name in resource_names(world) {
                            let mut shown = !view.hidden.contains(name);
                            ui.horizontal(|ui| {
                                let (dot, p) = ui.allocate_painter(Vec2::splat(12.0), egui::Sense::hover());
                                p.circle(dot.rect.center(), 5.0, resource_color(name), Stroke::new(1.0, Color32::GRAY));
                                if ui.checkbox(&mut shown, name).changed() {
                                    if shown {
                                        view.hidden.remove(name);
                                    } else {
                                        view.hidden.insert(name);
                                    }
                                }
                            });
                        }
                        ui.add_space(6.0);
                        for line in ["Ok: sen", "Mavi nokta: arkadaş", "Alev: ateş", "Ev: barınak", "Çekiç: yapı", "Sarı halka: kampın"] {
                            ui.label(RichText::new(line).small());
                        }
                    });
                });
                let width = ui.available_width().max(160.0);
                let (resp, painter) = ui.allocate_painter(egui::vec2(width, height), egui::Sense::click_and_drag());
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, "Harita alanı"));
                let mut centre = view.center.unwrap_or_else(|| (tiles_of(me).x, tiles_of(me).y));
                if resp.dragged() {
                    let by = resp.drag_delta() / view.zoom;
                    centre = (centre.0 - by.x, centre.1 - by.y);
                    view.center = Some(centre);
                }
                if let Some(pointer) = resp.hover_pos() {
                    let notches: f32 = ui.input(|i| {
                        i.events
                            .iter()
                            .filter_map(|e| match e {
                                egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta, .. } => Some(delta.y / 40.0),
                                egui::Event::MouseWheel { delta, .. } => Some(delta.y),
                                _ => None,
                            })
                            .sum()
                    });
                    if notches != 0.0 {
                        let (c, z) = zoom_about(resp.rect, pointer, centre, view.zoom, 1.15f32.powf(notches));
                        (centre, view.zoom) = (c, z);
                        view.center = Some(c);
                    }
                }
                let win = Window { rect: resp.rect, centre: egui::vec2(centre.0, centre.1), ppt: view.zoom, yaw: 0.0 };
                paint(&painter, &win, world, me, map, view);
                if resp.secondary_clicked()
                    && let Some(p) = resp.interact_pointer_pos()
                {
                    let tile = win.tile_at(p);
                    if map.records().is_explored(tile) {
                        acts.push(MapAct::SetCamp(Some(tile)));
                    }
                }
            });
        });
    });
    acts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface_map::Records;
    use alaz_sim::AgentId;
    use egui_kittest::kittest::Queryable;

    fn world() -> (World, AgentId) {
        let mut w = World::with_size(42, 256, 256);
        let (x, y) = w.spawn_mm;
        let me = w.spawn_agent(x, y);
        (w, me)
    }

    fn seen_with_camp(w: &World, me: AgentId, camp_east: i32) -> SurfaceMap {
        let mut map = SurfaceMap::default();
        map.observe(w, me);
        let at = tile_of(w.agents[0].x, w.agents[0].y);
        map.set_camp(Some((at.0 + camp_east, at.1)));
        map
    }

    #[test]
    fn north_is_up_when_the_camera_is_straight() {
        let up = project(egui::vec2(0.0, -1.0), 0.0);
        assert!((up - egui::vec2(0.0, -1.0)).length() < 1e-5, "{up:?}");
        let turned = project(egui::vec2(1.0, 0.0), 90.0);
        assert!((turned - egui::vec2(0.0, 1.0)).length() < 1e-5, "{turned:?}");
        let back = project(project(egui::vec2(3.0, 4.0), 37.0), -37.0);
        assert!((back - egui::vec2(3.0, 4.0)).length() < 1e-4);
    }

    #[test]
    fn a_far_camp_is_pinned_to_the_edge() {
        assert_eq!(edge_point(egui::vec2(200.0, 0.0), 80.0), Some(egui::vec2(80.0, 0.0)));
        assert_eq!(edge_point(egui::vec2(-100.0, -200.0), 80.0), Some(egui::vec2(-40.0, -80.0)));
        assert_eq!(edge_point(egui::vec2(10.0, 10.0), 80.0), None);
    }

    #[test]
    fn the_whole_camp_arrow_is_on_the_small_map() {
        for to in [egui::vec2(0.0, -500.0), egui::vec2(500.0, 500.0), egui::vec2(-500.0, 90.0)] {
            let edge = edge_point(to, SMALL * 0.5 - CAMP_INSET).expect("far away");
            for p in camp_arrow(edge) {
                assert!(p.x.abs() <= SMALL * 0.5 && p.y.abs() <= SMALL * 0.5, "{to:?}: {p:?}");
            }
        }
    }

    #[test]
    fn unexplored_pixels_stay_fog() {
        let (w, _) = world();
        let mut records = Records::default();
        let mut rows = [0u64; 64];
        rows[5] = 1 << 7;
        records.explored.insert((1, 1), rows);
        let picture = chunk_image(&w, &records, (1, 1));
        assert_eq!(picture.size, [64, 64]);
        for (i, &c) in picture.pixels.iter().enumerate() {
            if i == 5 * 64 + 7 {
                assert_ne!(c, FOG);
            } else {
                assert_eq!(c, FOG, "pixel {i}");
            }
        }
    }

    #[test]
    fn a_large_explored_area_builds_few_pictures_per_frame() {
        let (w, _) = world();
        let mut records = Records::default();
        let chunks: Vec<Tile> = (0..4).flat_map(|x| (0..4).map(move |y| (x, y))).collect();
        for &c in &chunks {
            records.explored.insert(c, [u64::MAX; 64]);
        }
        let map = SurfaceMap::from_records(records);
        let ctx = egui::Context::default();
        let mut view = MapView::default();
        assert_eq!(view.pictures(&ctx, &w, &map, &chunks), 12);
        assert_eq!(view.cache.len(), 4);
        assert_eq!(view.pictures(&ctx, &w, &map, &chunks), 8);
        assert_eq!(view.cache.len(), 8);
        let other = World::with_size(43, 256, 256);
        view.pictures(&ctx, &other, &SurfaceMap::default(), &[]);
        assert!(view.cache.is_empty(), "another world, other pictures");
    }

    #[test]
    fn the_small_map_shows_the_distance_to_camp() {
        let (w, me) = world();
        let map = seen_with_camp(&w, me, 120);
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(800.0, 600.0)).build_ui_state(
            move |ui, (w, map, view): &mut (World, SurfaceMap, MapView)| {
                draw_small(ui.ctx(), w, &w.agents[0], map, view, 45.0);
            },
            (w, map, MapView::default()),
        );
        h.step();
        h.step();
        assert!(h.ctx.content_rect().contains_rect(h.get_by_label("Kamp: 120 m").rect()));
    }

    type Large = (World, SurfaceMap, MapView, Vec<MapAct>);

    fn large(size: egui::Vec2) -> egui_kittest::Harness<'static, Large> {
        let (w, me) = world();
        let map = seen_with_camp(&w, me, 20);
        let mut h = egui_kittest::Harness::builder().with_size(size).build_ui_state(
            |ui, (w, map, view, acts): &mut Large| {
                acts.extend(draw_large(ui.ctx(), w, &w.agents[0], map, view, "M"));
            },
            (w, map, MapView { open: true, ..MapView::default() }, Vec::new()),
        );
        h.step();
        h.step();
        h
    }

    #[test]
    fn the_large_map_fits_a_small_window() {
        let h = large(egui::vec2(800.0, 600.0));
        let window = h.ctx.content_rect();
        for label in ["Kapat [M]", "Beni ortala", "Kampımı buraya koy", "Kamp işaretini kaldır", "Taş yığını", "Mağara ağzı", "Harita alanı"] {
            let r = h.get_by_label(label).rect();
            assert!(window.contains_rect(r), "{label}: {r:?}");
        }
    }

    #[test]
    fn the_large_map_buttons_ask_for_what_they_say() {
        let mut h = large(egui::vec2(1200.0, 900.0));
        let at = tile_of(h.state().0.agents[0].x, h.state().0.agents[0].y);
        h.get_by_label("Kampımı buraya koy").click();
        h.step();
        h.step();
        assert!(h.state().3.contains(&MapAct::SetCamp(Some(at))), "{:?}", h.state().3);
        h.get_by_label("Kamp işaretini kaldır").click();
        h.step();
        h.step();
        assert!(h.state().3.contains(&MapAct::SetCamp(None)));
        h.get_by_label("Taş yığını").click();
        h.step();
        h.step();
        assert!(h.state().2.hidden.contains("Taş yığını"));
        h.get_by_label("Kapat [M]").click();
        h.step();
        h.step();
        assert!(h.state().3.contains(&MapAct::Close));
    }

    #[test]
    fn zoom_keeps_the_spot_under_the_pointer() {
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 200.0));
        let pointer = egui::pos2(300.0, 50.0);
        let spot = |centre: (f32, f32), zoom: f32| (centre.0 + (pointer.x - rect.center().x) / zoom, centre.1 + (pointer.y - rect.center().y) / zoom);
        let (centre, zoom) = zoom_about(rect, pointer, (100.0, 100.0), 3.0, 2.0);
        assert_eq!(zoom, 6.0);
        let (a, b) = (spot((100.0, 100.0), 3.0), spot(centre, zoom));
        assert!((a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3);
        assert_eq!(zoom_about(rect, pointer, (0.0, 0.0), 3.0, 100.0).1, ZOOM_MAX);
        assert_eq!(zoom_about(rect, pointer, (0.0, 0.0), 3.0, 0.001).1, ZOOM_MIN);
    }

    #[test]
    fn resource_marks_grow_with_the_zoom_and_stay_small_on_the_small_map() {
        assert!(mark_radius(SMALL / SMALL_TILES) <= 1.5, "the ground must show between them");
        assert!(mark_radius(ZOOM_START) < 2.0);
        assert_eq!(mark_radius(ZOOM_MIN), 1.0);
        assert_eq!(mark_radius(ZOOM_MAX), 3.5);
    }

    #[test]
    fn ores_stay_plain_rock_on_the_map_until_the_eye_can_tell() {
        let (w, _) = world();
        assert!(w.perception_level() < 2);
        let names = resource_names(&w);
        assert!(names.contains(&"Kaya") && !names.contains(&"Demir damarı"), "{names:?}");
    }
}
