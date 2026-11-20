use alaz_render::egui;
use alaz_sim::{ItemKind, World, item_guide};

use crate::journal_ui::PageCx;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Page {
    #[default]
    Index,
    Item(ItemKind),
    Recipe(usize),
}

#[derive(Default)]
pub(crate) struct ItemBook {
    page: Page,
    back: Vec<Page>,
    forward: Vec<Page>,
    query: String,
}

impl ItemBook {
    pub(crate) fn open(&mut self, page: Page) {
        if self.page != page {
            self.back.push(self.page);
            self.page = page;
            self.forward.clear();
        }
    }

    fn go_back(&mut self) {
        if let Some(page) = self.back.pop() {
            self.forward.push(std::mem::replace(&mut self.page, page));
        }
    }

    fn go_forward(&mut self) {
        if let Some(page) = self.forward.pop() {
            self.back.push(std::mem::replace(&mut self.page, page));
        }
    }

    pub(crate) fn draw(&mut self, ui: &mut egui::Ui, world: &World, cx: &mut PageCx<'_>) {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!self.back.is_empty(), egui::Button::new("Geri"))
                .clicked()
            {
                self.go_back();
            }
            if ui
                .add_enabled(!self.forward.is_empty(), egui::Button::new("İleri"))
                .clicked()
            {
                self.go_forward();
            }
            if ui.button("Tüm eşyalar").clicked() {
                self.open(Page::Index);
            }
        });
        ui.separator();
        match self.page {
            Page::Index => {
                ui.add(egui::TextEdit::singleline(&mut self.query).hint_text("Eşya ara"));
                let query = self.query.trim().to_lowercase();
                ui.horizontal_wrapped(|ui| {
                    for item in ItemKind::ALL
                        .iter()
                        .copied()
                        .filter(|&item| world.has_seen(item))
                    {
                        if query.is_empty()
                            || crate::lang::item_word(world, item)
                                .to_lowercase()
                                .contains(&query)
                        {
                            self.item_link(ui, world, item, 1);
                        }
                    }
                });
                if !ItemKind::ALL.iter().any(|&item| world.has_seen(item)) {
                    ui.label("Eşya gördükçe Defter’e eklenir.");
                }
            }
            Page::Item(item) if world.has_seen(item) => self.draw_item(ui, world, cx, item),
            Page::Recipe(index) => self.draw_recipe(ui, world, cx, index),
            Page::Item(_) => {
                ui.label("?");
            }
        }
    }

    fn item_link(&mut self, ui: &mut egui::Ui, world: &World, item: ItemKind, count: u16) {
        if !world.has_seen(item) {
            ui.label("?");
            return;
        }
        let word = crate::lang::item_word(world, item);
        if world.perception_level() == 0 {
            let (rect, hit) = ui.allocate_exact_size(egui::Vec2::splat(34.0), egui::Sense::click());
            crate::icons::draw_slot(ui.painter(), rect, item, true);
            if hit.on_hover_text(word).clicked() {
                self.open(Page::Item(item));
            }
        } else {
            let label = if count > 1 && world.perception_level() >= 2 {
                format!("{count} × {word}")
            } else {
                word
            };
            if ui.link(label).clicked() {
                self.open(Page::Item(item));
            }
        }
    }

    fn open_recipe(&mut self, ctx: &egui::Context, world: &World, cx: &PageCx<'_>, index: usize) {
        let recipe = &world.rules().recipes[index];
        if let Some((building, _)) = cx.machine.filter(|(_, kind)| recipe.station == Some(*kind)) {
            crate::inv::info::set_recipe_pick(ctx, building, index);
        }
        self.open(Page::Recipe(index));
    }

    fn draw_item(&mut self, ui: &mut egui::Ui, world: &World, cx: &PageCx<'_>, item: ItemKind) {
        if world.perception_level() > 0 {
            ui.heading(crate::lang::item_word(world, item));
        }
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(42.0), egui::Sense::hover());
        crate::icons::draw_slot(ui.painter(), rect, item, true);
        ui.separator();
        ui.label("Nerede bulunur?");
        let mut sources = item_guide::found_in(item).peekable();
        if sources.peek().is_none() {
            ui.weak("?");
        }
        for kind in sources {
            let label = if kind == alaz_sim::NodeKind::CaveMouth {
                "Mağara damarları"
            } else {
                crate::hud::node_name(world, kind)
            };
            ui.label(label);
            if kind == alaz_sim::NodeKind::CaveMouth && world.perception_level() >= 2 {
                let bands = ["Üst galeri", "Orta galeri", "Derin galeri", "En dip"];
                let names = item_guide::cave_depths(item)
                    .map(|depth| bands[depth as usize])
                    .collect::<Vec<_>>();
                if !names.is_empty() {
                    ui.weak(names.join(" · "));
                }
            }
        }
        let rules = world.rules();
        for (title, recipes) in [
            (
                "Nasıl yapılır?",
                item_guide::made_by(rules, item).collect::<Vec<_>>(),
            ),
            (
                "Nerede kullanılır?",
                item_guide::used_in(rules, item).collect::<Vec<_>>(),
            ),
        ] {
            ui.separator();
            ui.label(title);
            let mut shown = false;
            for (index, recipe) in recipes
                .into_iter()
                .filter(|(_, recipe)| world.knows(recipe.discovery))
            {
                shown = true;
                if world.perception_level() == 0 {
                    ui.horizontal_wrapped(|ui| {
                        for output in &recipe.outputs {
                            self.item_link(ui, world, output.item, output.n);
                        }
                        if ui.small_button("Tarif").clicked() {
                            self.open_recipe(ui.ctx(), world, cx, index);
                        }
                    });
                } else if ui.link(&recipe.name).clicked() {
                    self.open_recipe(ui.ctx(), world, cx, index);
                }
            }
            if title == "Nerede kullanılır?" {
                for def in item_guide::used_for_building(rules, item)
                    .filter(|def| world.knows(def.discovery))
                {
                    shown = true;
                    egui::CollapsingHeader::new(&def.name)
                        .id_salt(def.kind as usize)
                        .show(ui, |ui| {
                            for (i, cost) in def.price_lists().enumerate() {
                                if i > 0 {
                                    ui.weak("Alternatif");
                                }
                                ui.horizontal_wrapped(|ui| {
                                    for entry in cost {
                                        self.item_link(ui, world, entry.item, entry.n);
                                    }
                                });
                            }
                            if !def.needs.is_empty() {
                                ui.weak("Gerekli aletler");
                                ui.horizontal_wrapped(|ui| {
                                    for &item in &def.needs {
                                        self.item_link(ui, world, item, 1);
                                    }
                                });
                            }
                        });
                }
            }
            if !shown {
                ui.weak("?");
            }
        }
    }

    fn draw_recipe(&mut self, ui: &mut egui::Ui, world: &World, cx: &PageCx<'_>, index: usize) {
        let Some(recipe) = world
            .rules()
            .recipes
            .get(index)
            .filter(|r| world.knows(r.discovery))
        else {
            ui.label("?");
            return;
        };
        if world.perception_level() > 0 {
            ui.heading(&recipe.name);
        }
        ui.label(recipe.station.or(recipe.near).map_or_else(
            || "El ile".into(),
            |kind| world.rules().building(kind).name.clone(),
        ));
        if world.perception_level() >= 2 {
            if recipe.heat > 0 {
                ui.label(format!("En az {} °C", recipe.heat));
            }
            if recipe.ticks > 0 {
                ui.label(format!(
                    "{} sn",
                    u64::from(recipe.ticks).div_ceil(alaz_sim::time::TICKS_PER_SECOND)
                ));
            }
        }
        for (title, items) in [("Girdiler", &recipe.inputs), ("Çıktılar", &recipe.outputs)] {
            ui.separator();
            ui.label(title);
            ui.horizontal_wrapped(|ui| {
                for entry in items {
                    self.item_link(ui, world, entry.item, entry.n);
                }
            });
        }
        if !recipe.needs.is_empty() {
            ui.separator();
            ui.label("Gerekli aletler");
            ui.horizontal_wrapped(|ui| {
                for &item in &recipe.needs {
                    self.item_link(ui, world, item, 1);
                }
            });
        }
        if let Some(alloy) = &recipe.alloy {
            ui.separator();
            ui.label("Alaşım");
            ui.horizontal_wrapped(|ui| {
                self.item_link(ui, world, alloy.base, 1);
                self.item_link(ui, world, alloy.add, 1);
                ui.label("→");
                self.item_link(ui, world, alloy.output, 1);
            });
        }
        if recipe.mine {
            ui.label("Kuyunun altındaki yatağa göre ürün çıkar.");
        }
        if let Some((building, kind)) = cx.machine.filter(|(_, kind)| recipe.station == Some(*kind))
        {
            let label = format!("{} için seç", world.rules().building(kind).name);
            if ui.button(label).clicked() {
                crate::inv::info::set_recipe_pick(ui.ctx(), building, index);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;

    #[test]
    fn navigation_returns_to_items_and_replaces_the_forward_branch() {
        let mut book = ItemBook::default();
        let stone = Page::Item(ItemKind::Stone);
        book.open(stone);
        book.open(Page::Recipe(0));
        book.go_back();
        assert_eq!(book.page, stone);
        book.go_forward();
        assert_eq!(book.page, Page::Recipe(0));
        book.go_back();
        book.open(Page::Item(ItemKind::Branch));
        assert!(book.forward.is_empty());
        book.go_back();
        assert_eq!(book.page, stone);
    }

    #[test]
    fn an_unseen_item_has_no_named_source_or_recipe() {
        let mut world = World::new(42);
        let (x, y) = world.spawn_mm;
        world.spawn_agent(x, y);
        let mut book = ItemBook::default();
        book.open(Page::Item(ItemKind::IronOre));
        let h = egui_kittest::Harness::builder().build_ui_state(
            |ui, (world, book)| {
                let mut cx = PageCx {
                    me: &world.agents[0],
                    machine: None,
                    acts: &mut Vec::new(),
                };
                book.draw(ui, world, &mut cx);
            },
            (world, book),
        );
        assert!(h.query_by_label("?").is_some());
        assert!(h.query_by_label("Nerede bulunur?").is_none());
    }

    #[test]
    fn an_item_page_opens_a_recipe_and_returns_to_the_item() {
        let mut world = World::new(42);
        let (x, y) = world.spawn_mm;
        world.spawn_agent(x, y);
        world.unlock_everything();
        let (index, recipe) = item_guide::made_by(world.rules(), ItemKind::StoneAxe)
            .next()
            .unwrap();
        let title = recipe.name.clone();
        let mut book = ItemBook::default();
        book.open(Page::Item(ItemKind::StoneAxe));
        let mut h = egui_kittest::Harness::builder().build_ui_state(
            |ui, (world, book)| {
                let mut cx = PageCx {
                    me: &world.agents[0],
                    machine: None,
                    acts: &mut Vec::new(),
                };
                book.draw(ui, world, &mut cx);
            },
            (world, book),
        );
        h.query_all_by_label(&title).last().unwrap().click();
        h.run();
        assert_eq!(h.state().1.page, Page::Recipe(index));
        assert!(h.query_by_label("Girdiler").is_some());
        h.get_by_label("Geri").click();
        h.run();
        assert_eq!(h.state().1.page, Page::Item(ItemKind::StoneAxe));
    }
}
