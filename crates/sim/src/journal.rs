//! The journal: what the tribe did and found, day by day, and the goal chosen at the evening fire.
//!
//! It is the tribe's (every player writes in the same one), the way discoveries and milestones are the tribe's.
//! Everything in it is made from the event stream and the world's own counters in `World::run_journal`, so it is
//! deterministic. It is bounded: the recent days keep their detail, old days shrink to their highlights, and the
//! oldest drop off, so a save never grows without limit. It is saved in the extension block of the save.

use crate::agents::AgentKind;
use crate::buildings::BuildingKind;
use crate::map::ItemKind;
use crate::world::{Discovery, Milestone};
use serde::{Deserialize, Serialize};

/// Days that keep every entry; older days keep only their highlights.
pub const RECENT_DAYS: usize = 12;
/// Days kept at all (the oldest are dropped).
pub const MAX_DAYS: usize = 90;
/// Entries one day can hold; past it, only highlights still get in (they push out the small ones).
pub const MAX_ENTRIES: usize = 36;
/// Entries a compacted day keeps.
pub const COMPACT_ENTRIES: usize = 8;
/// Rows of a day's tally of goods (the smallest drop out).
pub const MAX_TALLY: usize = 12;
/// Tally rows a compacted day keeps.
pub const COMPACT_TALLY: usize = 4;
/// People who can have sat at the fire today (the oldest record is replaced).
const MAX_CAMPERS: usize = 16;

/// A place, in tiles. Only set when someone carried paper to sketch it on.
pub type At = Option<(i32, i32)>;

/// The things that can go wrong to a person, merged by kind within a day.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Hazard {
    Bite,
    Fall,
    /// A falling tree or a rolling stone.
    Blow,
    Burn,
    Fracture,
    Infection,
    Cave,
}

/// What the tribe has set itself to do the next day.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Goal {
    /// Reach a milestone.
    Reach(Milestone),
    /// Find out something new.
    Discover(Discovery),
    /// Put up a building of this kind (one more than there is now).
    Build(BuildingKind),
    /// Make something that has never been made.
    Make(ItemKind),
}

/// One thing written down.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Entry {
    Discovered { d: Discovery, at: At },
    Milestone(Milestone),
    /// An item made for the first time (by hand or at a station).
    FirstMade { item: ItemKind, at: At },
    Built { kind: BuildingKind, at: At },
    Tamed(AgentKind),
    /// A new animal or worker, young or newly made.
    Born(AgentKind),
    Died,
    Hazard { kind: Hazard, n: u16 },
    FireStarted,
    /// The sky turned to rain (1) or snow (2).
    Sky(u8),
    /// The crop froze in a field.
    Frozen,
    /// Someone sat down at the fire.
    Camp,
    GoalSet(Goal),
    GoalDone(Goal),
}

impl Entry {
    /// Highlights survive a crowded day and an old one; the small things do not.
    pub fn highlight(&self) -> bool {
        matches!(
            self,
            Entry::Discovered { .. } | Entry::Milestone(_) | Entry::FirstMade { .. } | Entry::Built { .. } | Entry::Tamed(_) | Entry::Born(_) | Entry::Died | Entry::GoalDone(_)
        )
    }
}

/// One day's page.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DayLog {
    pub day: u32,
    /// (minute of the day, what happened), in order.
    pub entries: Vec<(u16, Entry)>,
    /// What was gathered, and what was made by recipes and stations, by item.
    pub gathered: Vec<(ItemKind, u16)>,
    pub made: Vec<(ItemKind, u16)>,
}

/// A discovery and when it came. `day` is `UNKNOWN_DAY` for one made before the journal was kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Find {
    pub d: Discovery,
    pub day: u32,
    pub minute: u16,
    pub at: At,
}

pub const UNKNOWN_DAY: u32 = u32::MAX;

/// An item made for the first time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct First {
    pub item: ItemKind,
    pub day: u32,
    pub minute: u16,
    pub at: At,
}

/// The goal in force.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalState {
    pub goal: Goal,
    /// The day it was chosen (the evening before the day it is for).
    pub set_day: u32,
    /// What the goal counts from: buildings of the kind standing then, or the item's total made.
    pub base: u32,
    pub done_day: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Journal {
    /// Oldest first; the last is the current day's page.
    pub days: Vec<DayLog>,
    pub finds: Vec<Find>,
    pub firsts: Vec<First>,
    pub goal: Option<GoalState>,
    /// (person, day) of the last evening each sat at the fire.
    pub campers: Vec<(u32, u32)>,
    /// The counters the journal measures change against were set (a new world, or a save from before the journal).
    pub started: bool,
    /// Milestone bits already written down.
    pub milestones_logged: u64,
    /// What `World::produced` held when last looked at.
    pub produced_mark: Vec<u32>,
    /// The sky when last looked at (0 clear, 1 rain, 2 snow).
    pub sky: u8,
}

impl Journal {
    /// The page of this day, if anything was written on it.
    pub fn day(&self, day: u32) -> Option<&DayLog> {
        self.days.iter().rev().find(|d| d.day == day)
    }

    pub fn find(&self, d: Discovery) -> Option<&Find> {
        self.finds.iter().find(|f| f.d == d)
    }

    pub fn first(&self, item: ItemKind) -> Option<&First> {
        self.firsts.iter().find(|f| f.item == item)
    }

    /// Has this person sat at the fire on this day?
    pub fn camped(&self, person: u32, day: u32) -> bool {
        self.campers.iter().any(|&(p, d)| p == person && d == day)
    }

    pub(crate) fn note_camp(&mut self, person: u32, day: u32) {
        self.campers.retain(|&(p, _)| p != person);
        if self.campers.len() >= MAX_CAMPERS {
            self.campers.remove(0);
        }
        self.campers.push((person, day));
    }

    /// The page for `day`, started if it is new (and the older pages tidied up then).
    fn page(&mut self, day: u32) -> &mut DayLog {
        if self.days.last().is_none_or(|d| d.day != day) {
            self.days.push(DayLog { day, ..Default::default() });
            self.compact(day);
        }
        self.days.last_mut().expect("a page")
    }

    /// Write something on `day`'s page. Hazards of one kind are added up; a full page keeps only highlights.
    pub(crate) fn record(&mut self, day: u32, minute: u16, e: Entry) {
        let page = self.page(day);
        if let Entry::Hazard { kind, n } = e {
            if let Some((_, Entry::Hazard { n: m, .. })) = page.entries.iter_mut().find(|(_, x)| matches!(x, Entry::Hazard { kind: k, .. } if *k == kind)) {
                *m = m.saturating_add(n);
                return;
            }
        }
        if page.entries.len() >= MAX_ENTRIES {
            if !e.highlight() {
                return;
            }
            // a highlight takes the place of the oldest small thing; a page of nothing but highlights is full
            match page.entries.iter().position(|(_, x)| !x.highlight()) {
                Some(i) => {
                    page.entries.remove(i);
                }
                None => return,
            }
        }
        page.entries.push((minute, e));
    }

    pub(crate) fn tally_gathered(&mut self, day: u32, item: ItemKind, n: u16) {
        let page = self.page(day);
        add_tally(&mut page.gathered, item, n);
    }

    pub(crate) fn tally_made(&mut self, day: u32, item: ItemKind, n: u16) {
        let page = self.page(day);
        add_tally(&mut page.made, item, n);
    }

    /// Old pages shrink to their highlights and biggest tallies; the oldest go.
    fn compact(&mut self, today: u32) {
        let keep_from = today.saturating_sub(RECENT_DAYS as u32 - 1);
        for page in self.days.iter_mut().filter(|d| d.day < keep_from) {
            if page.entries.len() > COMPACT_ENTRIES || page.entries.iter().any(|(_, e)| !e.highlight()) {
                page.entries.retain(|(_, e)| e.highlight());
                page.entries.truncate(COMPACT_ENTRIES);
            }
            for t in [&mut page.gathered, &mut page.made] {
                if t.len() > COMPACT_TALLY {
                    t.sort_by(|a, b| b.1.cmp(&a.1).then((a.0 as usize).cmp(&(b.0 as usize))));
                    t.truncate(COMPACT_TALLY);
                }
            }
        }
        // a page with nothing on it is not worth keeping once it is old
        self.days.retain(|d| d.day >= keep_from || !(d.entries.is_empty() && d.gathered.is_empty() && d.made.is_empty()));
        if self.days.len() > MAX_DAYS {
            let drop = self.days.len() - MAX_DAYS;
            self.days.drain(..drop);
        }
    }
}

fn add_tally(t: &mut Vec<(ItemKind, u16)>, item: ItemKind, n: u16) {
    match t.iter_mut().find(|(k, _)| *k == item) {
        Some((_, m)) => *m = m.saturating_add(n),
        None => {
            t.push((item, n));
            if t.len() > MAX_TALLY {
                // the smallest row goes (the newest, on a tie)
                let i = (0..t.len()).rev().min_by_key(|&i| t[i].1).unwrap_or(0);
                t.remove(i);
            }
        }
    }
}

/// Every discovery that is taught, in the order the tribe is meant to meet them.
pub const DISCOVERY_ORDER: [Discovery; 59] = [
    Discovery::SharpStone,
    Discovery::StoneAxe,
    Discovery::Hammer,
    Discovery::Clamp,
    Discovery::Fire,
    Discovery::Shelter,
    Discovery::Clothing,
    Discovery::Spear,
    Discovery::Cooking,
    Discovery::Medicine,
    Discovery::Farming,
    Discovery::Milling,
    Discovery::Pottery,
    Discovery::Weaving,
    Discovery::Husbandry,
    Discovery::Casting,
    Discovery::Depot,
    Discovery::Kiln,
    Discovery::Furnace,
    Discovery::Smelting,
    Discovery::Workbench,
    Discovery::Ironwork,
    Discovery::Mechanics,
    Discovery::Workshop,
    Discovery::Alloying,
    Discovery::Bronzework,
    Discovery::Draught,
    Discovery::Steelmaking,
    Discovery::Steam,
    Discovery::Electricity,
    Discovery::Electronics,
    Discovery::Computing,
    Discovery::Geology,
    Discovery::Fishing,
    Discovery::Masonry,
    Discovery::Cement,
    Discovery::Foundry,
    Discovery::Converter,
    Discovery::Machining,
    Discovery::Throwing,
    Discovery::OilPressing,
    Discovery::Tanning,
    Discovery::Curing,
    Discovery::Smoking,
    Discovery::Ropemaking,
    Discovery::Wells,
    Discovery::Felting,
    Discovery::Hoisting,
    Discovery::Glassblowing,
    Discovery::Papermaking,
    Discovery::Printing,
    Discovery::Ginning,
    Discovery::Spinning,
    Discovery::Drawing,
    Discovery::HeavyCurrent,
    Discovery::ArcSmelting,
    Discovery::Chemistry,
    Discovery::Semiconductors,
    Discovery::Automation,
];

/// Evening and night: from 17:00 until first light at 05:00. Camp is made then.
pub fn is_evening(tick: u64) -> bool {
    let m = crate::time::minute_of_day(tick);
    !(300..1020).contains(&m)
}
