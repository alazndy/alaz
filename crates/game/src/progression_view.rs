use std::collections::BTreeMap;
use std::sync::Arc;

use alaz_render::egui::{self, Align2, Color32, FontId, Pos2, Rect, Stroke, Vec2};
use alaz_sim::journal::{Progression, ProgressionState, UNKNOWN_DAY};
use alaz_sim::{AgentId, Discovery, Event, ItemKind, World};

use crate::item_book::Page;

const BAND_NAMES: [&str; 4] = ["Taş", "Metal", "Makine", "Bilgisayar"];
const CELL: Vec2 = Vec2::new(138.0, 61.0);
const HEADER: f32 = 36.0;
const GOLD: Color32 = Color32::from_rgb(228, 184, 97);

pub(crate) struct ProgressionView {
    rules: Option<Arc<alaz_sim::rules::Rules>>,
    graph: Option<Progression>,
    selected: Option<Discovery>,
    zoom: f32,
    show_firsts: bool,
}

impl Default for ProgressionView {
    fn default() -> Self {
        Self {
            rules: None,
            graph: None,
            selected: None,
            zoom: 0.9,
            show_firsts: false,
        }
    }
}

impl ProgressionView {
    fn refresh(&mut self, world: &World) {
        let rules = world.rules_arc();
        if self
            .rules
            .as_ref()
            .is_none_or(|cached| !Arc::ptr_eq(cached, &rules))
        {
            self.graph = Some(Progression::new(&rules));
            self.rules = Some(rules);
        }
    }

    pub(crate) fn next(&mut self, world: &World) -> Option<Discovery> {
        self.refresh(world);
        self.graph.as_ref()?.next(world)
    }

    pub(crate) fn select(&mut self, discovery: Discovery) {
        self.selected = Some(discovery);
        self.show_firsts = false;
    }

    pub(crate) fn draw(
        &mut self,
        ui: &mut egui::Ui,
        world: &World,
        player: AgentId,
    ) -> Option<Page> {
        let next = self.next(world);
        let graph = self.graph.as_ref().expect("graph was refreshed");
        if self.show_firsts {
            if ui.button("Ağaca dön").clicked() {
                self.show_firsts = false;
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("progression_firsts")
                    .show(ui, |ui| crate::journal_ui::section_firsts(ui, world));
                return None;
            }
        }
        if let Some(node) = self.selected.and_then(|discovery| graph.node(discovery))
            && node.state(world) != ProgressionState::Hidden
        {
            let discovery = node.discovery;
            if !ui.button("Ağaca dön").clicked() {
                return egui::ScrollArea::vertical()
                    .id_salt(("discovery_detail", discovery as u8))
                    .show(ui, |ui| {
                        detail(ui, world, player, discovery, &node.prerequisites)
                    })
                    .inner;
            }
        }
        self.selected = None;

        ui.horizontal_wrapped(|ui| {
            if ui.button("−").on_hover_text("Uzaklaştır").clicked() {
                self.zoom /= 1.15;
            }
            if ui.button("+").on_hover_text("Yakınlaştır").clicked() {
                self.zoom *= 1.15;
            }
            if let Some(discovery) = next
                && ui.button("Sıradaki keşif").clicked()
            {
                self.selected = Some(discovery);
            }
            if ui.button("İlk yapılanlar").clicked() {
                self.show_firsts = true;
            }
        });
        ui.weak("Sürükleyerek kaydır · Tekerlekle yakınlaştır");
        if ui.rect_contains_pointer(ui.available_rect_before_wrap()) {
            self.zoom *= ui.input_mut(|input| {
                let pinch = input.zoom_delta();
                if pinch != 1.0 {
                    pinch
                } else if !input.modifiers.shift && input.smooth_scroll_delta.y != 0.0 {
                    let scroll = input.smooth_scroll_delta.y;
                    input.smooth_scroll_delta.y = 0.0;
                    (scroll / 220.0).exp()
                } else {
                    1.0
                }
            });
        }
        self.zoom = self.zoom.clamp(0.55, 1.65);
        let scale = self.zoom;
        let nodes = graph.nodes();
        let mut columns = [1; 4];
        let mut lanes = 1;
        for node in nodes {
            columns[usize::from(node.band).min(3)] =
                columns[usize::from(node.band).min(3)].max(usize::from(node.column) + 1);
            lanes = lanes.max(usize::from(node.lane) + 1);
        }
        let mut offset = [0; 4];
        for i in 1..4 {
            offset[i] = offset[i - 1] + columns[i - 1];
        }
        let size = Vec2::new(
            columns.iter().sum::<usize>() as f32 * CELL.x,
            HEADER + lanes as f32 * CELL.y,
        ) * scale;
        let viewport = ui.available_size();
        egui::ScrollArea::both()
            .id_salt("progression_graph")
            .auto_shrink([false, false])
            .max_width(viewport.x)
            .max_height(viewport.y.max(85.0))
            .show(ui, |ui| {
                let (canvas, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                let p = ui.painter().clone();
                let theme = crate::style::palette(crate::style::current_era());
                for i in 0..4 {
                    let left = canvas.left() + offset[i] as f32 * CELL.x * scale;
                    let band = Rect::from_min_size(
                        Pos2::new(left, canvas.top()),
                        Vec2::new(columns[i] as f32 * CELL.x * scale, canvas.height()),
                    );
                    p.rect_filled(
                        band,
                        0.0,
                        if i % 2 == 0 {
                            theme.slot
                        } else {
                            theme.slot_shadow
                        },
                    );
                    p.text(
                        band.left_top() + Vec2::splat(9.0 * scale),
                        Align2::LEFT_TOP,
                        BAND_NAMES[i],
                        FontId::proportional(16.0 * scale),
                        theme.ink,
                    );
                }
                let trunk = Rect::from_min_size(
                    canvas.min + Vec2::new(0.0, HEADER * scale),
                    Vec2::new(canvas.width(), CELL.y * scale),
                );
                p.rect_filled(trunk, 0.0, GOLD.gamma_multiply(0.10));
                let rects: BTreeMap<_, _> = nodes
                    .iter()
                    .map(|node| {
                        let col = offset[usize::from(node.band).min(3)] + usize::from(node.column);
                        let top = canvas.min
                            + Vec2::new(
                                col as f32 * CELL.x + 9.0,
                                HEADER + f32::from(node.lane) * CELL.y + 5.0,
                            ) * scale;
                        (
                            node.discovery as u8,
                            Rect::from_min_size(top, Vec2::new(118.0, 44.0) * scale),
                        )
                    })
                    .collect();
                for node in nodes {
                    let rect = rects[&(node.discovery as u8)];
                    for dependency in &node.prerequisites {
                        if let Some(from) = rects.get(&(*dependency as u8)) {
                            let color = if node.main && world.knows(*dependency) {
                                GOLD
                            } else {
                                theme.edge
                            };
                            arrow(&p, *from, rect, Stroke::new(scale.max(1.0), color));
                        }
                    }
                }
                for node in nodes {
                    let state = node.state(world);
                    let (label, fill, border) = match state {
                        ProgressionState::Known => (
                            crate::hud::discovery_name(node.discovery),
                            theme.base,
                            if node.main { GOLD } else { theme.good },
                        ),
                        ProgressionState::Next => (
                            crate::hud::discovery_name(node.discovery),
                            theme.base_dark,
                            GOLD,
                        ),
                        ProgressionState::Hidden => {
                            ("?", theme.slot_shadow, theme.edge.gamma_multiply(0.55))
                        }
                    };
                    let rect = rects[&(node.discovery as u8)];
                    if !ui.is_rect_visible(rect) {
                        continue;
                    }
                    let label = egui::RichText::new(label).size(15.0 * scale).color(
                        if state == ProgressionState::Hidden {
                            theme.ink_dim
                        } else {
                            theme.ink
                        },
                    );
                    let button = egui::Button::new(label)
                        .wrap()
                        .fill(fill)
                        .stroke(Stroke::new(scale.max(1.0), border));
                    if state == ProgressionState::Hidden {
                        ui.add_enabled_ui(false, |ui| ui.put(rect, button));
                    } else if ui.put(rect, button).clicked() {
                        self.selected = Some(node.discovery);
                    }
                }
            });
        None
    }
}

fn arrow(p: &egui::Painter, from: Rect, to: Rect, stroke: Stroke) {
    let rightward = from.center().x < to.center().x;
    let (a, b) = if rightward {
        (from.right_center(), to.left_center())
    } else {
        (from.left_center(), to.right_center())
    };
    let mid = (a.x + b.x) / 2.0;
    let end = Pos2::new(mid, b.y);
    p.line_segment([a, Pos2::new(mid, a.y)], stroke);
    p.line_segment([Pos2::new(mid, a.y), end], stroke);
    p.arrow(end, b - end, stroke);
}

fn detail(
    ui: &mut egui::Ui,
    world: &World,
    player: AgentId,
    discovery: Discovery,
    prerequisites: &[Discovery],
) -> Option<Page> {
    ui.heading(crate::hud::discovery_name(discovery));
    if let Some(find) = world
        .journal
        .find(discovery)
        .filter(|find| find.day != UNKNOWN_DAY)
    {
        ui.weak(format!(
            "{}. gün · {}{}",
            find.day + 1,
            crate::journal_ui::clock(find.minute),
            crate::journal_ui::place(find.at)
        ));
    }
    if !prerequisites.is_empty() {
        ui.label("Önkoşullar");
        ui.horizontal_wrapped(|ui| {
            for &required in prerequisites {
                if world.knows(required) {
                    ui.weak(crate::hud::discovery_name(required));
                }
            }
        });
    }
    let known = world.knows(discovery);
    if let Some(hint) = crate::hud::event_hint(&Event::Discovered(discovery), world, player) {
        ui.label(crate::lang::hint_text(world, &hint));
    } else if !known {
        ui.label("Keşfetmek için gerekli malzemeleri topla ve uygun araçlarla dene.");
    }

    let rules = world.rules();
    let recipes: Vec<_> = rules
        .recipes
        .iter()
        .enumerate()
        .filter(|(_, recipe)| recipe.discovery == discovery)
        .collect();
    let buildings: Vec<_> = rules
        .buildings
        .iter()
        .filter(|def| def.discovery == discovery)
        .collect();
    if known && !(recipes.is_empty() && buildings.is_empty()) {
        ui.separator();
        ui.label("Açılan tarifler ve yapılar");
        for (index, recipe) in &recipes {
            if ui.link(&recipe.name).clicked() {
                return Some(Page::Recipe(*index));
            }
        }
        for building in &buildings {
            ui.label(&building.name);
        }
    }
    let recipe_items = recipes.into_iter().flat_map(|(_, recipe)| {
        recipe
            .inputs
            .iter()
            .map(|entry| entry.item)
            .chain(recipe.needs.iter().copied())
            .chain(
                recipe
                    .outputs
                    .iter()
                    .map(|entry| entry.item)
                    .filter(|_| known),
            )
    });
    let building_items = buildings.into_iter().flat_map(|building| {
        building
            .price_lists()
            .flatten()
            .map(|entry| entry.item)
            .chain(building.needs.iter().copied())
    });
    let items: BTreeMap<u8, ItemKind> = recipe_items
        .chain(building_items)
        .filter(|&item| world.has_seen(item))
        .map(|item| (item as u8, item))
        .collect();
    if !items.is_empty() {
        ui.separator();
        ui.label("İlgili eşyalar");
    }
    let mut opened = None;
    ui.horizontal_wrapped(|ui| {
        for item in items.into_values() {
            if ui.link(crate::lang::item_word(world, item)).clicked() {
                opened = Some(Page::Item(item));
            }
        }
    });
    opened
}
