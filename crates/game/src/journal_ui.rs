//! The journal tab and the evening camp window: the tribe's days, what it has found, the recipes it knows, and the
//! goal chosen at the fire for the next day.

use crate::hud::{discovery_name, item_name, kind_name, milestone_name, panel, UiAction};
use alaz_render::egui::{self, Align2, Color32, RichText};
use alaz_sim::journal::{Entry, Goal, Hazard};
use alaz_sim::rules::rules;
use alaz_sim::world::{Agent, Inventory};
use alaz_sim::{BuildingKind, CommandKind, Discovery, ItemKind, RecipeId, World};

/// UI state of the journal and the camp window.
#[derive(Default)]
pub struct JournalState {
    /// Days, discoveries, recipes, or the item guide.
    pub section: u8,
    pub search: String,
    reference: crate::item_book::ItemBook,
    pub(crate) progression: crate::progression_view::ProgressionView,
    /// The camp window was closed with its button (it comes back with the next sitting down).
    pub camp_closed: bool,
    /// The book lies open beside the inventory window; it is its own window, not a tab.
    pub book_open: bool,
}

impl JournalState {
    pub(crate) fn open_reference(&mut self, page: crate::item_book::Page) {
        self.reference.open(page);
        self.section = 3;
        self.book_open = true;
    }

    pub(crate) fn show_discovery(&mut self, discovery: Discovery) {
        self.progression.select(discovery);
        self.section = 1;
        self.book_open = true;
    }

    pub fn toggle_book(&mut self) {
        self.book_open = !self.book_open;
    }
}

pub const SECTIONS: [&str; 4] = ["Günler", "Keşifler", "Tarifler", "Eşyalar"];

pub fn building_name(k: BuildingKind) -> String {
    rules().buildings.iter().find(|b| b.kind == k).map(|b| b.name.clone()).unwrap_or_else(|| format!("{k:?}"))
}

pub fn clock(minute: u16) -> String {
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

pub fn goal_text(g: Goal) -> String {
    match g {
        Goal::Reach(m) => format!("Hedefe var: {}", milestone_name(m)),
        Goal::Discover(d) => format!("Keşfet: {}", discovery_name(d)),
        Goal::Build(k) => format!("Kur: {}", building_name(k)),
        Goal::Make(i) => format!("İlk kez yap: {}", item_name(i)),
    }
}

fn hazard_text(h: Hazard, n: u16) -> String {
    let what = match h {
        Hazard::Bite => "ısırık",
        Hazard::Fall => "ağır düşme",
        Hazard::Blow => "darbe",
        Hazard::Burn => "yanık",
        Hazard::Fracture => "kırık",
        Hazard::Infection => "iltihaplanma",
        Hazard::Cave => "mağara çöküntüsü",
    };
    if n > 1 { format!("Tehlike: {what} (x{n})") } else { format!("Tehlike: {what}") }
}

/// A place from a sketch, only written when there was paper to sketch it on.
pub(crate) fn place(at: Option<(i32, i32)>) -> String {
    match at {
        Some((x, y)) => format!("  [haritada: {x}, {y}]"),
        None => String::new(),
    }
}

pub fn entry_text(e: &Entry) -> String {
    match *e {
        Entry::Discovered { d, at } => format!("Keşif: {}{}", discovery_name(d), place(at)),
        Entry::Milestone(m) => format!("{}: {}", if crate::hud::is_age(m) { "Yeni çağ" } else { "Hedef" }, milestone_name(m)),
        Entry::FirstMade { item, at } => format!("İlk kez yapıldı: {}{}", item_name(item), place(at)),
        Entry::Built { kind, at } => format!("Kuruldu: {}{}", building_name(kind), place(at)),
        Entry::Tamed(k) => format!("Evcilleşti: {}", kind_name(k)),
        Entry::Born(k) => format!("Aramıza katıldı: {}", kind_name(k)),
        Entry::Died => "Biri dayanamadı".to_string(),
        Entry::Hazard { kind, n } => hazard_text(kind, n),
        Entry::FireStarted => "Bir yerde yangın çıktı".to_string(),
        Entry::Sky(1) => "Yağmur bastırdı".to_string(),
        Entry::Sky(_) => "Kar yağmaya başladı".to_string(),
        Entry::Frozen => "Don ekini vurdu".to_string(),
        Entry::Camp => "Ateş başında oturuldu".to_string(),
        Entry::GoalSet(g) => format!("Yarının hedefi: {}", goal_text(g)),
        Entry::GoalDone(g) => format!("Hedef tamam: {}", goal_text(g)),
    }
}

fn entry_color(e: &Entry) -> Color32 {
    match e {
        Entry::Discovered { .. } | Entry::FirstMade { .. } => Color32::from_rgb(190, 214, 170),
        Entry::Milestone(_) | Entry::GoalDone(_) | Entry::GoalSet(_) => Color32::from_rgb(244, 208, 120),
        Entry::Built { .. } | Entry::Tamed(_) | Entry::Born(_) | Entry::Camp => Color32::from_rgb(220, 196, 160),
        Entry::Hazard { .. } | Entry::Died | Entry::FireStarted => Color32::from_rgb(236, 140, 120),
        Entry::Sky(_) | Entry::Frozen => Color32::from_rgb(150, 190, 230),
    }
}

fn tally_line(t: &[(ItemKind, u16)]) -> String {
    t.iter().map(|(k, n)| format!("{n} {}", item_name(*k))).collect::<Vec<_>>().join(", ")
}

/// How a day is called: the clock starts at day 0, people count from 1.
fn day_name(day: u32) -> String {
    format!("{}. gün", day + 1)
}

/// What the recipes page needs besides the world: the player (what can be made now), the machine the inventory
/// window has open (a click on its recipes picks them) and where commands go.
pub struct PageCx<'a> {
    pub me: &'a Agent,
    pub machine: Option<(usize, BuildingKind)>,
    pub acts: &'a mut Vec<UiAction>,
}

/// The page the book shows for `st.section`.
pub fn page(ui: &mut egui::Ui, w: &World, st: &mut JournalState, pc: &mut PageCx) {
    match st.section {
        0 => section_days(ui, w),
        1 => {
            if let Some(page) = st.progression.draw(ui, w, pc.me.id) {
                st.open_reference(page);
            }
        }
        2 => section_recipes(ui, w, st, pc),
        _ => st.reference.draw(ui, w, pc),
    }
}

fn section_days(ui: &mut egui::Ui, w: &World) {
    let today = w.day() as u32;
    let j = &w.journal;
    if let Some(gs) = &j.goal {
        let done = gs.done_day.is_some();
        ui.label(RichText::new(format!("{} {}", if done { "Hedef tamam:" } else { "Hedefin:" }, goal_text(gs.goal))).color(Color32::from_rgb(244, 208, 120)));
        ui.separator();
    }
    let mut any = false;
    for page in j.days.iter().rev() {
        if page.entries.is_empty() && page.gathered.is_empty() && page.made.is_empty() {
            continue;
        }
        any = true;
        ui.label(RichText::new(if page.day == today { format!("{} (bugün)", day_name(page.day)) } else { day_name(page.day) }).strong());
        for (min, e) in &page.entries {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(clock(*min)).weak());
                ui.label(RichText::new(entry_text(e)).color(entry_color(e)));
            });
        }
        if !page.gathered.is_empty() {
            ui.add(egui::Label::new(RichText::new(format!("Toplanan: {}", tally_line(&page.gathered))).weak()).wrap());
        }
        if !page.made.is_empty() {
            ui.add(egui::Label::new(RichText::new(format!("Yapılan: {}", tally_line(&page.made))).weak()).wrap());
        }
        ui.add_space(6.0);
    }
    if !any {
        ui.label("Henüz yazacak bir şey yok. Günlük, yaptıkların ve bulduklarınla dolar.");
    }
}

pub(crate) fn section_firsts(ui: &mut egui::Ui, w: &World) {
    ui.heading("İlk yapılanlar");
    if w.journal.firsts.is_empty() {
        ui.label("İlk yaptığın eşyalar burada görünür.");
    }
    for f in w.journal.firsts.iter().rev().take(24) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(item_name(f.item)).color(Color32::from_rgb(220, 196, 160)));
            ui.label(RichText::new(format!("{}, {}{}", day_name(f.day), clock(f.minute), place(f.at))).weak());
        });
    }
}

fn recipe_line(r: &alaz_sim::rules::RecipeDef) -> String {
    let side = |v: &[alaz_sim::rules::ItemCount]| v.iter().map(|c| if c.n > 1 { format!("{} {}", c.n, item_name(c.item)) } else { item_name(c.item).to_string() }).collect::<Vec<_>>().join(" + ");
    let out = if let Some(k) = r.spawns { kind_name(k).to_string() } else { side(&r.outputs) };
    let mut s = format!("{} -> {}", side(&r.inputs), out);
    if !r.needs.is_empty() {
        s += &format!("   (gerek: {})", r.needs.iter().map(|k| item_name(*k)).collect::<Vec<_>>().join(", "));
    }
    s
}

fn needs_met(needs: &[ItemKind], inv: &Inventory) -> bool {
    needs.iter().all(|&k| inv.get(k) > 0)
}

/// Everything a hand recipe needs has been seen but the recipe is not known yet. No name, no steps: just a nudge to
/// try those things together in the hands.
fn clues(ui: &mut egui::Ui, w: &World) {
    // a farming tool only makes sense once there is something to farm: those clues wait until seed or grain has been seen
    let era_ready = |d: Discovery| match d {
        Discovery::Farming => w.has_seen(ItemKind::Seed),
        Discovery::Milling => w.has_seen(ItemKind::Grain),
        _ => true,
    };
    for r in w.rules().recipes.iter().filter(|r| r.station.is_none() && r.ticks == 0 && !w.knows(r.discovery) && era_ready(r.discovery) && !r.inputs.is_empty() && r.inputs.iter().all(|c| w.has_seen(c.item))) {
        egui::Frame::new().fill(Color32::from_black_alpha(60)).inner_margin(8).corner_radius(0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                for c in &r.inputs {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
                    crate::icons::draw_slot(ui.painter(), rect, c.item, true);
                }
                ui.label(RichText::new("Bunlar bir arada işe yarar gibi. Elinde dene.").small().weak());
            });
        });
    }
}

fn recipe_tip(w: &World, me: &Agent, r: &alaz_sim::rules::RecipeDef) -> String {
    let mut tip = format!("{}\n", r.name);
    for c in &r.inputs {
        tip += &format!("{} {} (sende {})\n", c.n, crate::lang::item_word(w, c.item), me.inventory.get(c.item));
    }
    for k in &r.needs {
        tip += &format!("Gerekir: {}\n", crate::lang::item_word(w, *k));
    }
    if !r.steps.is_empty() {
        let names: Vec<&str> = r.steps.iter().map(|s| s.label.as_str()).collect();
        tip += &format!("Adımlar: {}", names.join(" → "));
    }
    tip
}

/// The known recipes of one station (None: by hand), with their index in the rules.
type Group<'a> = (Option<BuildingKind>, Vec<(usize, &'a alaz_sim::rules::RecipeDef)>);

fn section_recipes(ui: &mut egui::Ui, w: &World, st: &mut JournalState, pc: &mut PageCx) {
    let defs = w.rules();
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut st.search).hint_text("Tarif ara...").desired_width(220.0));
        if !st.search.is_empty() && ui.small_button("Temizle").clicked() {
            st.search.clear();
        }
    });
    let q = st.search.trim().to_lowercase();
    let known: Vec<(usize, &alaz_sim::rules::RecipeDef)> = defs.recipes.iter().enumerate().filter(|(_, r)| w.knows(r.discovery)).collect();
    if known.iter().all(|(_, r)| r.station.is_some()) {
        ui.label("Defterin boş. Elinde bir şeyler dene, yaparak öğrenirsin.");
    }
    if q.is_empty() {
        clues(ui, w);
    }
    ui.label(RichText::new(format!("Bilinen tarifler: {}/{}", known.len(), defs.recipes.len())).weak());
    // grouped by where the work is done: by hand first, then each station in the order the rules list them
    let mut groups: Vec<Group> = Vec::new();
    for e in known {
        match groups.iter_mut().find(|(s, _)| *s == e.1.station) {
            Some((_, v)) => v.push(e),
            None => groups.push((e.1.station, vec![e])),
        }
    }
    groups.sort_by_key(|(s, _)| s.map(|k| k as usize + 1).unwrap_or(0));
    let picked = pc.machine.and_then(|(b, _)| crate::inv::info::recipe_pick(ui.ctx(), b).or_else(|| crate::inv::info::default_pick(w, b)));
    for (station, list) in groups {
        let title = station.map_or("El ile".to_string(), building_name);
        let shown: Vec<_> = list.iter().filter(|(_, r)| q.is_empty() || recipe_line(r).to_lowercase().contains(&q) || r.name.to_lowercase().contains(&q) || title.to_lowercase().contains(&q)).collect();
        if shown.is_empty() {
            continue;
        }
        let head = format!("{title} ({})", shown.len());
        let open_machine = pc.machine.is_some_and(|(_, k)| Some(k) == station);
        let body = |ui: &mut egui::Ui| {
            for &&(i, r) in &shown {
                let pickable = open_machine;
                let makeable = station.is_none() && pc.me.inventory.has(&r.inputs) && needs_met(&r.needs, &pc.me.inventory);
                let col = if picked == Some(i) && pickable {
                    Color32::from_rgb(190, 232, 150)
                } else if station.is_none() && !makeable {
                    Color32::from_rgb(170, 154, 126)
                } else {
                    Color32::from_rgb(244, 218, 150)
                };
                let sense = if pickable || makeable { egui::Sense::click() } else { egui::Sense::hover() };
                let resp = ui.add(egui::Label::new(RichText::new(&r.name).color(col)).wrap().sense(sense));
                if resp.on_hover_text(recipe_tip(w, pc.me, r)).clicked() {
                    match pc.machine {
                        Some((b, _)) if pickable => crate::inv::info::set_recipe_pick(ui.ctx(), b, i),
                        _ if makeable => pc.acts.push(UiAction::Cmd(CommandKind::Craft { recipe: RecipeId(i as u16) })),
                        _ => {}
                    }
                }
                if ui.small_button("Tarif ayrıntısı").clicked() {
                    st.open_reference(crate::item_book::Page::Recipe(i));
                }
                ui.add(egui::Label::new(RichText::new(recipe_line(r)).weak().small()).wrap());
            }
        };
        // a search opens every group that has a hit; the open machine's own recipes are the ones to choose from
        let open = if q.is_empty() && !open_machine { None } else { Some(true) };
        egui::CollapsingHeader::new(head).id_salt(("recipes", station.map(|k| k as usize))).open(open).show(ui, body);
    }
}

/// The goal in force, as a line for the HUD: (text, reached).
pub fn goal_line(w: &World) -> Option<(String, bool)> {
    let gs = w.journal.goal.as_ref()?;
    let today = w.day() as u32;
    match gs.done_day {
        // a reached goal stays on view for the rest of the day it was reached
        Some(d) if d < today => None,
        Some(_) => Some((format!("Günün hedefi tamam: {}", goal_text(gs.goal)), true)),
        None if gs.set_day < today => Some((format!("Günün hedefi: {}", goal_text(gs.goal)), false)),
        None => Some((format!("Yarının hedefi: {}", goal_text(gs.goal)), false)),
    }
}

/// The button to sit at the fire, when it can be done: the text and whether it is a go.
pub fn camp_button(w: &World, me: &alaz_sim::world::Agent) -> bool {
    !me.is_seated() && w.can_camp(me.id).is_ok()
}

/// The camp window: the day in short, and the choice of a goal for tomorrow.
pub fn draw_camp(ctx: &egui::Context, w: &World, me: &alaz_sim::world::Agent, st: &mut JournalState, acts: &mut Vec<UiAction>) {
    if !me.is_seated() {
        st.camp_closed = false;
        return;
    }
    if st.camp_closed {
        return;
    }
    let today = w.day() as u32;
    let page = w.journal.day(today);
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::NULL.with("camp")).anchor(Align2::LEFT_CENTER, [100.0, -10.0]).order(egui::Order::Foreground).show(ctx, |ui| {
        panel(ui, 8.0, |ui| {
            ui.set_width(520.0_f32.min(screen.width() - 40.0));
            ui.label(RichText::new(format!("Ateş başı - {}", day_name(today))).size(26.0).strong().color(Color32::from_rgb(244, 190, 100)));
            ui.separator();
            egui::ScrollArea::vertical().max_height((screen.height() - 430.0).clamp(70.0, 250.0)).auto_shrink([false, true]).show(ui, |ui| {
                ui.label(RichText::new("Bugün").strong());
                match page {
                    Some(p) => {
                        if !p.gathered.is_empty() {
                            ui.add(egui::Label::new(format!("Toplanan: {}", tally_line(&p.gathered))).wrap());
                        }
                        if !p.made.is_empty() {
                            ui.add(egui::Label::new(format!("Yapılan: {}", tally_line(&p.made))).wrap());
                        }
                        for (_, e) in p.entries.iter().filter(|(_, e)| !matches!(e, Entry::Camp | Entry::GoalSet(_) | Entry::Hazard { .. })) {
                            ui.add(egui::Label::new(RichText::new(entry_text(e)).color(entry_color(e))).wrap());
                        }
                        for (_, e) in p.entries.iter().filter(|(_, e)| matches!(e, Entry::Hazard { .. })) {
                            ui.add(egui::Label::new(RichText::new(entry_text(e)).color(entry_color(e))).wrap());
                        }
                        if p.gathered.is_empty() && p.made.is_empty() && p.entries.iter().all(|(_, e)| matches!(e, Entry::Camp)) {
                            ui.label("Sakin bir gün geçti.");
                        }
                    }
                    None => {
                        ui.label("Sakin bir gün geçti.");
                    }
                }
                if let Some(gs) = &w.journal.goal {
                    let (t, c) = if gs.done_day.is_some() { ("Tamamlanan hedef", Color32::from_rgb(140, 200, 140)) } else { ("Sürmekte olan hedef", Color32::from_rgb(244, 208, 120)) };
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("{t}: {}", goal_text(gs.goal))).color(c));
                }
            });
            // the choice stays in view whatever the day held
            ui.separator();
            ui.label(RichText::new("Yarın için hedef").strong());
            let cur = w.journal.goal.filter(|g| g.set_day == today && g.done_day.is_none()).map(|g| g.goal);
            for g in w.camp_goals() {
                if ui.selectable_label(cur == Some(g), goal_text(g)).clicked() {
                    acts.push(UiAction::Cmd(CommandKind::SetGoal { goal: Some(g) }));
                }
            }
            if cur.is_some() && ui.small_button("Hedefi bırak").clicked() {
                acts.push(UiAction::Cmd(CommandKind::SetGoal { goal: None }));
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new("Ateşin sıcağı dinlendirir. Kalkmak için yürü.").weak().small());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Kapat").clicked() {
                        st.camp_closed = true;
                    }
                });
            });
        });
    });
}
