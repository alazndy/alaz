//! The journal tab and the evening camp window: the tribe's days, what it has found, the recipes it knows, and the
//! goal chosen at the fire for the next day.

use crate::hud::{discovery_name, item_name, kind_name, milestone_name, panel, UiAction};
use alaz_render::egui::{self, Align2, Color32, RichText};
use alaz_sim::journal::{Entry, Goal, Hazard, UNKNOWN_DAY};
use alaz_sim::rules::rules;
use alaz_sim::{BuildingKind, CommandKind, Discovery, ItemKind, World};

/// UI state of the journal and the camp window.
#[derive(Default)]
pub struct JournalState {
    /// 0 days, 1 discoveries, 2 recipes.
    pub section: u8,
    pub search: String,
    /// The camp window was closed with its button (it comes back with the next sitting down).
    pub camp_closed: bool,
}

pub const SECTIONS: [&str; 3] = ["Günler", "Keşifler", "Tarifler"];

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
fn place(at: Option<(i32, i32)>) -> String {
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

pub fn tab_journal(ui: &mut egui::Ui, w: &World, st: &mut JournalState) {
    ui.horizontal(|ui| {
        for (i, name) in SECTIONS.iter().enumerate() {
            if ui.selectable_label(st.section as usize == i, *name).clicked() {
                st.section = i as u8;
            }
        }
    });
    ui.separator();
    match st.section {
        0 => section_days(ui, w),
        1 => section_finds(ui, w),
        _ => section_recipes(ui, w, st),
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

fn section_finds(ui: &mut egui::Ui, w: &World) {
    let j = &w.journal;
    let mut found: Vec<Discovery> = alaz_sim::journal::DISCOVERY_ORDER.iter().copied().filter(|d| w.knows(*d)).collect();
    ui.label(RichText::new(format!("Keşifler ({}/{})", found.len(), alaz_sim::journal::DISCOVERY_ORDER.len())).strong());
    // newest first: what was found most recently is what people look for
    found.sort_by_key(|d| std::cmp::Reverse(j.find(*d).map_or(0, |f| if f.day == UNKNOWN_DAY { 0 } else { f.day as u64 * 1440 + f.minute as u64 + 1 })));
    for d in found {
        let when = match j.find(d) {
            Some(f) if f.day != UNKNOWN_DAY => format!("{}, {}{}", day_name(f.day), clock(f.minute), place(f.at)),
            _ => "baştan beri".to_string(),
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(discovery_name(d)).color(Color32::from_rgb(190, 214, 170)));
            ui.label(RichText::new(when).weak());
        });
        let made: Vec<String> = rules().recipes.iter().filter(|r| r.discovery == d && r.spawns.is_none()).filter_map(|r| r.outputs.first()).map(|o| item_name(o.item).to_string()).take(4).collect();
        let built: Vec<String> = rules().buildings.iter().filter(|b| b.discovery == d).map(|b| b.name.clone()).take(3).collect();
        let mut opens: Vec<String> = Vec::new();
        for n in built.into_iter().chain(made) {
            if !opens.contains(&n) {
                opens.push(n);
            }
        }
        if !opens.is_empty() {
            ui.add(egui::Label::new(RichText::new(format!("  açtığı: {}", opens.join(", "))).weak().small()).wrap());
        }
    }
    if !j.firsts.is_empty() {
        ui.separator();
        ui.label(RichText::new("İlk yapılanlar").strong());
        for f in j.firsts.iter().rev().take(24) {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(item_name(f.item)).color(Color32::from_rgb(220, 196, 160)));
                ui.label(RichText::new(format!("{}, {}{}", day_name(f.day), clock(f.minute), place(f.at))).weak());
            });
        }
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

fn section_recipes(ui: &mut egui::Ui, w: &World, st: &mut JournalState) {
    let defs = rules();
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut st.search).hint_text("Tarif ara...").desired_width(220.0));
        if !st.search.is_empty() && ui.small_button("Temizle").clicked() {
            st.search.clear();
        }
    });
    let q = st.search.trim().to_lowercase();
    let known: Vec<&alaz_sim::rules::RecipeDef> = defs.recipes.iter().filter(|r| w.knows(r.discovery)).collect();
    ui.label(RichText::new(format!("Bilinen tarifler: {}/{}", known.len(), defs.recipes.len())).weak());
    // grouped by where the work is done: by hand first, then each station in the order the rules list them
    let mut groups: Vec<(Option<BuildingKind>, Vec<&alaz_sim::rules::RecipeDef>)> = Vec::new();
    for r in known {
        match groups.iter_mut().find(|(s, _)| *s == r.station) {
            Some((_, v)) => v.push(r),
            None => groups.push((r.station, vec![r])),
        }
    }
    groups.sort_by_key(|(s, _)| s.map(|k| k as usize + 1).unwrap_or(0));
    for (station, list) in groups {
        let title = station.map_or("El ile".to_string(), building_name);
        let shown: Vec<_> = list.iter().filter(|r| q.is_empty() || recipe_line(r).to_lowercase().contains(&q) || r.name.to_lowercase().contains(&q) || title.to_lowercase().contains(&q)).collect();
        if shown.is_empty() {
            continue;
        }
        let head = format!("{title} ({})", shown.len());
        let body = |ui: &mut egui::Ui| {
            for r in &shown {
                ui.add(egui::Label::new(RichText::new(&r.name).color(Color32::from_rgb(244, 218, 150))).wrap());
                ui.add(egui::Label::new(RichText::new(recipe_line(r)).weak().small()).wrap());
            }
        };
        // a search opens every group that has a hit
        egui::CollapsingHeader::new(head).id_salt(("recipes", station.map(|k| k as usize))).open(if q.is_empty() { None } else { Some(true) }).show(ui, body);
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
