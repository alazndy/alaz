//! What the info panel says about an item, a recipe or a machine, as data. How much it says grows with perception:
//! pictures only, then names and words, then numbers, then a terminal-style data block. Drawing is done elsewhere.

use alaz_render::egui::{Context, Id};
use alaz_sim::buildings::Job;
use alaz_sim::rules::RecipeDef;
use alaz_sim::time::TICKS_PER_SECOND;
use alaz_sim::{BuildingKind, Discovery, ItemKind, World};

use crate::hud;
use crate::lang;
use crate::style::icons::Icon;
use crate::style::lines::{Chip, Line, Picto};

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Subject {
    Item(ItemKind),
    /// Index into `w.rules().recipes`.
    Recipe(usize),
    /// Index into `w.buildings`.
    Machine(usize),
}

pub(crate) fn describe(w: &World, s: Subject, level: u8) -> Vec<Line> {
    match s {
        Subject::Item(k) => item_lines(w, k, level),
        Subject::Recipe(i) => match w.rules().recipes.get(i) {
            Some(r) if w.knows(r.discovery) => recipe_lines(w, r, level),
            _ => Vec::new(),
        },
        Subject::Machine(i) => machine_lines(w, i, level),
    }
}

fn pick_id(building: usize) -> Id {
    Id::NULL.with(("recipe_pick", building))
}

/// The recipe the player chose for this machine; it lives in egui memory so it survives frames.
pub(crate) fn recipe_pick(ctx: &Context, building: usize) -> Option<usize> {
    ctx.memory(|m| m.data.get_temp::<usize>(pick_id(building)))
}

pub(crate) fn set_recipe_pick(ctx: &Context, building: usize, recipe: usize) {
    ctx.memory_mut(|m| m.data.insert_temp(pick_id(building), recipe));
}

/// The recipe set to run by itself, else the first one the tribe knows.
pub(crate) fn default_pick(w: &World, building: usize) -> Option<usize> {
    let b = w.buildings.get(building)?;
    if let Some(a) = b.auto {
        return Some(a as usize);
    }
    station_recipes(w, b.kind).into_iter().find(|&(_, known)| known).map(|(i, _)| i)
}

/// Recipes of a station with a flag for whether the tribe knows them. Retired recipes are left out.
pub(crate) fn station_recipes(w: &World, kind: BuildingKind) -> Vec<(usize, bool)> {
    w.rules()
        .recipes
        .iter()
        .enumerate()
        .filter(|(_, r)| r.station == Some(kind) && r.discovery != Discovery::Retired)
        .map(|(i, r)| (i, w.knows(r.discovery)))
        .collect()
}

const FLAMES: u8 = 4;
const GLASSES: u8 = 4;

/// 1..=4 flames, so a stone-age player reads "hotter" without a number. 0 means no heat is needed.
fn flames_for(deg: i16) -> u8 {
    match deg {
        i16::MIN..=0 => 0,
        1..=209 => 1,
        210..=579 => 2,
        580..=1099 => 3,
        _ => 4,
    }
}

/// 1..=4 hourglasses by the length of the job. 0 means instant.
fn glasses_for(ticks: u32) -> u8 {
    match u64::from(ticks) / TICKS_PER_SECOND {
        0 => 0,
        1..=10 => 1,
        11..=30 => 2,
        31..=60 => 3,
        _ => 4,
    }
}

fn names(w: &World, items: &[alaz_sim::rules::ItemCount]) -> String {
    items.iter().map(|c| lang::item_word(w, c.item)).collect::<Vec<_>>().join(", ")
}

fn item_lines(w: &World, k: ItemKind, level: u8) -> Vec<Line> {
    if level <= 1 {
        return vec![Line::Pictos(vec![Picto::Item(k)])];
    }
    vec![Line::Title(lang::item_word(w, k)), Line::Chips(vec![Chip::Item(k, 1)])]
}

fn job_secs(r: &RecipeDef) -> f32 {
    r.ticks as f32 / TICKS_PER_SECOND as f32
}

/// Outputs per minute of the first product; none for instant recipes or ones that make no item.
fn per_min(r: &RecipeDef) -> Option<f32> {
    let first = r.outputs.first()?;
    (r.ticks > 0).then(|| f32::from(first.n) * 60.0 / job_secs(r))
}

/// An alloy recipe lists only its fuel as inputs; the two ores and the ingot live in its alloy part.
fn alloy_parts(r: &RecipeDef) -> Option<([ItemKind; 2], ItemKind)> {
    r.alloy.as_ref().map(|a| ([a.base, a.add], a.output))
}

fn pictos_row(r: &RecipeDef) -> Vec<Picto> {
    let mut row: Vec<Picto> = r.inputs.iter().map(|c| Picto::Item(c.item)).collect();
    if let Some((ores, _)) = alloy_parts(r) {
        row.extend(ores.map(Picto::Item));
    }
    row.push(Picto::Icon(Icon::Arrow));
    row.extend(r.outputs.iter().map(|c| Picto::Item(c.item)));
    if let Some((_, out)) = alloy_parts(r) {
        row.push(Picto::Item(out));
    }
    row
}

/// "Bakır cevheri + Kalay cevheri → Bronz" for an alloy recipe: what the ratio of the ores makes is a matter of
/// skill, so no counts.
fn alloy_text(w: &World, r: &RecipeDef) -> Option<String> {
    alloy_parts(r).map(|([a, b], out)| format!("{} + {} → {}", lang::item_word(w, a), lang::item_word(w, b), lang::item_word(w, out)))
}

fn recipe_lines(w: &World, r: &RecipeDef, level: u8) -> Vec<Line> {
    match level {
        0 | 1 => {
            let mut lines = vec![Line::Pictos(pictos_row(r))];
            if flames_for(r.heat) > 0 {
                lines.push(Line::Pictos(vec![Picto::Flames { lit: flames_for(r.heat), of: FLAMES }]));
            }
            if glasses_for(r.ticks) > 0 {
                lines.push(Line::Pictos(vec![Picto::Glasses { lit: glasses_for(r.ticks), of: GLASSES }]));
            }
            lines
        }
        2 | 3 => {
            let mut lines = vec![Line::Title(r.name.clone())];
            if !r.inputs.is_empty() {
                lines.push(Line::Text(format!("gerek: {}", names(w, &r.inputs))));
            }
            if let Some(t) = alloy_text(w, r) {
                lines.push(Line::Text(t));
            }
            if r.heat > 0 {
                lines.push(Line::Chips(vec![Chip::HeatWord(hud::heat_word(r.heat))]));
            }
            if let Some(note) = r.station.and_then(hud::machine_note) {
                lines.push(Line::Text(note.to_string()));
            }
            lines
        }
        4 => {
            let mut lines = vec![Line::Title(r.name.clone())];
            if !r.inputs.is_empty() {
                lines.push(Line::Chips(r.inputs.iter().map(|c| Chip::Item(c.item, c.n)).collect()));
            }
            if let Some(t) = alloy_text(w, r) {
                lines.push(Line::Text(t));
            }
            let mut figures = Vec::new();
            if r.heat > 0 {
                figures.push(Chip::HeatC { need: r.heat, now: None });
            }
            if r.ticks > 0 {
                figures.push(Chip::Secs((u64::from(r.ticks) / TICKS_PER_SECOND) as u32));
            }
            if let Some(p) = per_min(r) {
                figures.push(Chip::PerMin(p));
            }
            if !figures.is_empty() {
                lines.push(Line::Chips(figures));
            }
            if let Some(note) = r.station.and_then(hud::machine_note) {
                lines.push(Line::Text(note.to_string()));
            }
            lines
        }
        _ => vec![Line::Mono(data_block(r, None, None))],
    }
}

/// The level-4 data as terminal rows. Efficiency is not in the sim, so it has no row.
fn data_block(r: &RecipeDef, machine: Option<&alaz_sim::Building>, progress: Option<u32>) -> Vec<String> {
    let id = r.id.to_uppercase();
    let list = |items: &[alaz_sim::rules::ItemCount]| items.iter().map(|c| format!("{} x{}", hud::item_name(c.item), c.n)).collect::<Vec<_>>().join(", ");
    let mut rows = vec![id];
    if !r.inputs.is_empty() {
        rows.push(format!("IN {}", list(&r.inputs)));
    }
    if !r.outputs.is_empty() {
        rows.push(format!("OUT {}", list(&r.outputs)));
    }
    if let Some(([a, b], out)) = alloy_parts(r) {
        rows.push(format!("ALLOY {} + {} > {}", hud::item_name(a), hud::item_name(b), hud::item_name(out)));
    }
    if r.heat > 0 {
        rows.push(match machine {
            Some(b) => format!("T {}/{} C", b.temp, r.heat),
            None => format!("T {} C", r.heat),
        });
    }
    if r.ticks > 0 {
        rows.push(format!("t {} s", u64::from(r.ticks) / TICKS_PER_SECOND));
    }
    if let Some(p) = per_min(r) {
        rows.push(format!("RATE {p:.1}/min"));
    }
    if let Some(p) = progress {
        rows.push(format!("JOB {p}%"));
    }
    if let Some(b) = machine {
        rows.push(format!("auto {}", if b.auto.is_some() { "ON" } else { "OFF" }));
    }
    rows
}

/// Job progress in percent, from the ticks left against the recipe length.
fn job_percent(j: &Job, r: &RecipeDef) -> u32 {
    if r.ticks == 0 {
        return 100;
    }
    (100 - (u64::from(j.remaining.min(r.ticks)) * 100 / u64::from(r.ticks)) as u32).min(100)
}

fn machine_lines(w: &World, i: usize, level: u8) -> Vec<Line> {
    let Some(b) = w.buildings.get(i) else { return Vec::new() };
    let defs = w.rules();
    let job = b.job.as_ref().and_then(|j| defs.recipes.get(j.recipe as usize).map(|r| (j, r)));
    let shown = job.map(|(_, r)| r).or_else(|| b.auto.and_then(|a| defs.recipes.get(a as usize)));
    let hot = shown.is_some_and(|r| r.heat > 0) || b.temp > 0;
    let progress = job.map(|(j, r)| job_percent(j, r));
    match level {
        0 | 1 => {
            let mut lines = Vec::new();
            if hot {
                lines.push(Line::Pictos(vec![Picto::Flames { lit: flames_for(b.temp), of: FLAMES }]));
            }
            if let Some(p) = progress {
                lines.push(Line::Pictos(vec![Picto::Glasses { lit: (p * u32::from(GLASSES) / 100) as u8, of: GLASSES }]));
            }
            lines
        }
        _ => {
            let mut lines = vec![Line::Title(defs.building(b.kind).name.clone())];
            if let Some((_, r)) = job {
                lines.push(Line::Text(format!("çalışıyor: {}", r.name)));
            }
            if level >= 5 {
                if let Some(r) = shown {
                    lines.push(Line::Mono(data_block(r, Some(b), progress)));
                } else {
                    lines.push(Line::Mono(vec![format!("T {} C", b.temp), format!("auto {}", if b.auto.is_some() { "ON" } else { "OFF" })]));
                }
            } else {
                if hot {
                    if level >= 4 {
                        let need = shown.map_or(0, |r| r.heat);
                        lines.push(Line::Chips(vec![Chip::HeatC { need, now: Some(b.temp) }]));
                    } else {
                        lines.push(Line::Chips(vec![Chip::HeatWord(hud::heat_word(b.temp))]));
                    }
                }
                if let Some(p) = progress.filter(|_| level >= 4) {
                    lines.push(Line::Text(format!("ilerleme: %{p}")));
                }
            }
            if let Some(req) = b.kind.requirement().filter(|_| !w.requirement_met(i)) {
                lines.push(Line::Warn(hud::requirement_missing(req).to_string()));
            } else if let Some(note) = hud::machine_note(b.kind) {
                lines.push(Line::Text(note.to_string()));
            }
            lines
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inv::tests::world_with;
    use alaz_sim::Building;

    fn recipe_index(w: &World, id: &str) -> usize {
        w.rules().recipes.iter().position(|r| r.id == id).expect("recipe in rules")
    }

    fn learned() -> World {
        let mut w = world_with(&[]);
        w.unlock_everything();
        w
    }

    fn only_pictos(lines: &[Line]) -> bool {
        lines.iter().all(|l| matches!(l, Line::Pictos(_)))
    }

    #[test]
    fn stone_age_recipe_has_no_text_or_numbers() {
        let w = learned();
        for level in [0, 1] {
            for id in ["coke", "iron_bloom", "alloy_bronze"] {
                let lines = describe(&w, Subject::Recipe(recipe_index(&w, id)), level);
                assert!(!lines.is_empty(), "{id}: level {level} says something");
                assert!(only_pictos(&lines), "{id}: level {level} has words or numbers");
            }
            let m = describe(&w, Subject::Item(ItemKind::Stone), level);
            assert!(only_pictos(&m), "item at level {level}");
        }
    }

    #[test]
    fn metal_age_names_inputs_and_heat_word() {
        let w = learned();
        let ri = recipe_index(&w, "coke");
        let r = &w.rules().recipes[ri];
        for level in [2, 3] {
            let lines = describe(&w, Subject::Recipe(ri), level);
            assert!(lines.iter().any(|l| matches!(l, Line::Title(t) if *t == r.name)));
            let said = lines.iter().any(|l| matches!(l, Line::Chips(c) if c.iter().any(|c| matches!(c, Chip::HeatWord(h) if *h == hud::heat_word(r.heat)))));
            assert!(said, "heat word missing at level {level}");
            let first = r.inputs[0].item;
            let named = lines.iter().any(|l| matches!(l, Line::Text(t) if t.contains(hud::item_name(first))));
            assert!(named, "input name missing at level {level}");
            let numbers = lines.iter().any(|l| matches!(l, Line::Chips(c) if c.iter().any(|c| matches!(c, Chip::HeatC { .. } | Chip::Secs(_) | Chip::PerMin(_)))));
            assert!(!numbers, "no degrees, seconds or rates before level 4");
        }
    }

    #[test]
    fn machine_age_shows_degrees_and_seconds() {
        let w = learned();
        let ri = recipe_index(&w, "coke");
        let r = &w.rules().recipes[ri];
        let lines = describe(&w, Subject::Recipe(ri), 4);
        let chips: Vec<&Chip> = lines.iter().filter_map(|l| if let Line::Chips(c) = l { Some(c) } else { None }).flatten().collect();
        assert!(chips.iter().any(|c| matches!(c, Chip::HeatC { need, .. } if *need == r.heat)));
        assert!(chips.iter().any(|c| matches!(c, Chip::Secs(s) if u64::from(*s) == u64::from(r.ticks) / TICKS_PER_SECOND)));
    }

    #[test]
    fn computer_age_is_mono_and_has_auto_state() {
        let mut w = learned();
        let ri = recipe_index(&w, "coke");
        w.buildings.push(Building::new(BuildingKind::Furnace, 0, 0));
        let off = describe(&w, Subject::Machine(0), 5);
        assert!(off.iter().any(|l| matches!(l, Line::Mono(m) if m.iter().any(|s| s.contains("auto OFF")))));
        w.buildings[0].auto = Some(ri as u16);
        let on = describe(&w, Subject::Machine(0), 5);
        assert!(on.iter().any(|l| matches!(l, Line::Mono(m) if m.iter().any(|s| s.contains("auto ON")))));
        let rec = describe(&w, Subject::Recipe(ri), 5);
        assert!(rec.iter().any(|l| matches!(l, Line::Mono(_))));
        assert!(!rec.iter().any(|l| matches!(l, Line::Chips(_) | Line::Pictos(_))));
    }

    #[test]
    fn per_min_matches_ticks() {
        let w = learned();
        let ri = recipe_index(&w, "iron_bloom");
        let r = &w.rules().recipes[ri];
        assert_eq!((r.ticks, r.outputs.len(), r.outputs[0].n), (2400, 1, 1));
        let lines = describe(&w, Subject::Recipe(ri), 4);
        let rate = lines.iter().filter_map(|l| if let Line::Chips(c) = l { Some(c) } else { None }).flatten().find_map(|c| if let Chip::PerMin(p) = c { Some(*p) } else { None });
        assert_eq!(rate, Some(1.5));
    }

    #[test]
    fn default_pick_prefers_auto() {
        let mut w = learned();
        w.buildings.push(Building::new(BuildingKind::Furnace, 0, 0));
        let list = station_recipes(&w, BuildingKind::Furnace);
        assert!(list.len() >= 2);
        assert_eq!(default_pick(&w, 0), Some(list[0].0));
        let other = list[1].0;
        w.buildings[0].auto = Some(other as u16);
        assert_eq!(default_pick(&w, 0), Some(other));
        assert_eq!(default_pick(&w, 9), None);
    }

    #[test]
    fn unknown_recipes_are_flagged() {
        let mut w = world_with(&[]);
        w.learn_first(0);
        let list = station_recipes(&w, BuildingKind::Furnace);
        assert!(!list.is_empty());
        assert!(list.iter().all(|(_, known)| !known));
        assert!(describe(&w, Subject::Recipe(list[0].0), 4).is_empty(), "an unknown recipe says nothing");
        w.unlock_everything();
        assert!(station_recipes(&w, BuildingKind::Furnace).iter().all(|(_, known)| *known));
    }
}
