//! The evening camp and the journal's side of the world: writing events down, sitting at the fire, and the goal
//! chosen there for the next day.

use crate::agents::AgentKind;
use crate::command::AgentId;
use crate::journal::{is_evening, Entry, Find, First, Goal, GoalState, Hazard, DISCOVERY_ORDER, UNKNOWN_DAY};
use crate::map::{ItemKind, TILE_MM};
use crate::time::minute_of_day;
use crate::world::{Event, Milestone, Reject, Task, World};

/// How close to a lit campfire a person must be to sit at it (mm).
pub const CAMP_RADIUS_MM: i64 = 4_500;
/// A fall from this high goes in the journal (mm).
const NOTABLE_FALL_MM: i32 = 3_000;

impl World {
    /// Could this person sit down at camp now?
    pub fn can_camp(&self, agent: AgentId) -> Result<(), Reject> {
        let a = self.agents.get(agent.0 as usize).filter(|a| a.kind == AgentKind::Human).ok_or(Reject::NotLearned)?;
        if !is_evening(self.tick) {
            return Err(Reject::TooBright);
        }
        if !self.near_fire(a.x, a.y, CAMP_RADIUS_MM) {
            return Err(Reject::NoCampfire);
        }
        Ok(())
    }

    /// Sit down at the fire: rest, warmth and a little mending, once an evening; the camp is written down.
    pub(crate) fn do_camp(&mut self, idx: usize) {
        let id = self.agents[idx].id;
        if self.agents[idx].passage.is_some() || matches!(self.agents[idx].task, Task::Craft { .. }) {
            return;
        }
        if let Err(why) = self.can_camp(id) {
            self.events.push(Event::Rejected(id, why));
            return;
        }
        let day = self.day() as u32;
        let first = !self.journal.camped(id.0, day);
        let settings = self.settings;
        let a = &mut self.agents[idx];
        a.task = Task::Idle;
        a.sprint = false;
        a.seated = Some((a.x, a.y));
        if first {
            a.stamina = crate::world::MAX_STAT;
            a.warmth = crate::world::MAX_STAT;
            // a rest by the fire mends a little, but only for someone fed and watered
            if a.hunger > 2_000 && a.thirst > 2_000 {
                let cap = a.health_cap(&settings);
                a.health = (a.health + cap / 12).min(cap).max(a.health);
            }
            for v in [&mut a.injury, &mut a.sick, &mut a.broken] {
                *v = *v * 4 / 5;
            }
            self.journal.note_camp(id.0, day);
            self.events.push(Event::Camped(id));
        }
    }

    /// Sensible goals for tomorrow, from what is still ahead: the next milestones, the next thing to find out, a
    /// building that is known but not yet built, and something known that has never been made.
    pub fn camp_goals(&self) -> Vec<Goal> {
        let mut v = Vec::new();
        v.extend(Milestone::ALL.iter().filter(|m| !m.is_age() && !self.milestone_done(**m)).take(2).map(|m| Goal::Reach(*m)));
        if let Some(d) = DISCOVERY_ORDER.iter().find(|d| !self.knows(**d)) {
            v.push(Goal::Discover(*d));
        }
        let r = self.rules();
        if let Some(b) = r.buildings.iter().find(|b| self.knows(b.discovery) && !self.buildings.iter().any(|x| x.kind == b.kind)) {
            v.push(Goal::Build(b.kind));
        }
        let make = r.recipes.iter().filter(|rc| rc.spawns.is_none() && self.knows(rc.discovery)).filter_map(|rc| rc.outputs.first()).find(|o| self.produced[o.item as usize] == 0);
        if let Some(o) = make {
            v.push(Goal::Make(o.item));
        }
        v
    }

    fn goal_count(&self, g: Goal) -> u32 {
        match g {
            Goal::Build(k) => self.buildings.iter().filter(|b| b.kind == k).count() as u32,
            Goal::Make(i) => self.produced[i as usize],
            _ => 0,
        }
    }

    /// Has the goal been reached since it was chosen?
    pub fn goal_met(&self, gs: &GoalState) -> bool {
        match gs.goal {
            Goal::Reach(m) => self.milestone_done(m),
            Goal::Discover(d) => self.knows(d),
            g @ (Goal::Build(_) | Goal::Make(_)) => self.goal_count(g) > gs.base,
        }
    }

    /// Choose (or drop) the goal for the next day. Only someone who sat at the fire today may, and only from the list.
    pub(crate) fn set_goal(&mut self, idx: usize, goal: Option<Goal>) {
        let id = self.agents[idx].id;
        let day = self.day() as u32;
        if self.agents[idx].kind != AgentKind::Human || !self.journal.camped(id.0, day) {
            return;
        }
        match goal {
            None => self.journal.goal = None,
            Some(g) => {
                if !self.camp_goals().contains(&g) {
                    return;
                }
                self.journal.goal = Some(GoalState { goal: g, set_day: day, base: self.goal_count(g), done_day: None });
                self.journal.record(day, minute_of_day(self.tick) as u16, Entry::GoalSet(g));
            }
        }
    }

    /// Where someone carrying paper is, in tiles: the journal only sketches places when there is paper to do it on.
    fn ink(&self) -> Option<(i32, i32)> {
        self.agents
            .iter()
            .find(|a| a.kind == AgentKind::Human && a.inventory.get(ItemKind::Paper) > 0)
            .map(|a| ((a.x.div_euclid(TILE_MM)) as i32, (a.y.div_euclid(TILE_MM)) as i32))
    }

    /// Read what happened this step into the journal. Called at the end of every step and when events are taken.
    pub(crate) fn run_journal(&mut self) {
        let tick = self.tick;
        let day = self.day() as u32;
        let minute = minute_of_day(tick) as u16;
        if !self.journal.started {
            self.journal.started = true;
            self.journal.milestones_logged = self.milestones;
            self.journal.produced_mark = self.produced.to_vec();
            for d in DISCOVERY_ORDER {
                if self.knows(d) && self.journal.find(d).is_none() {
                    self.journal.finds.push(Find { d, day: UNKNOWN_DAY, minute: 0, at: None });
                }
            }
        }
        let from = self.journal_cursor.min(self.events.len());
        if from < self.events.len() {
            let new: Vec<Event> = self.events[from..].to_vec();
            self.journal_cursor = self.events.len();
            let at = self.ink();
            for e in new {
                let entry = match e {
                    Event::Discovered(d) => {
                        if self.journal.find(d).is_none_or(|f| f.day == UNKNOWN_DAY) {
                            self.journal.finds.retain(|f| f.d != d);
                            self.journal.finds.push(Find { d, day, minute, at });
                        }
                        Some(Entry::Discovered { d, at })
                    }
                    Event::Built { kind, x, y } => {
                        let at = at.map(|_| (x.div_euclid(TILE_MM) as i32, y.div_euclid(TILE_MM) as i32));
                        Some(Entry::Built { kind, at })
                    }
                    Event::Gathered { item, .. } => {
                        self.journal.tally_gathered(day, item, 1);
                        None
                    }
                    Event::Caught { .. } => {
                        self.journal.tally_gathered(day, ItemKind::RawFish, 1);
                        None
                    }
                    Event::Hunted { meat, .. } => {
                        self.journal.tally_gathered(day, ItemKind::RawMeat, meat);
                        None
                    }
                    Event::Tamed(k) => Some(Entry::Tamed(k)),
                    Event::WorkerReady(k) => Some(Entry::Born(k)),
                    Event::Died(_) => Some(Entry::Died),
                    Event::Bitten(_) => Some(Entry::Hazard { kind: Hazard::Bite, n: 1 }),
                    Event::Fractured(_) => Some(Entry::Hazard { kind: Hazard::Fracture, n: 1 }),
                    Event::WoundInfected(_) => Some(Entry::Hazard { kind: Hazard::Infection, n: 1 }),
                    Event::Burned(_) => Some(Entry::Hazard { kind: Hazard::Burn, n: 1 }),
                    Event::Crushed(_) | Event::RockHit(_) => Some(Entry::Hazard { kind: Hazard::Blow, n: 1 }),
                    Event::CaveRockfall(_) => Some(Entry::Hazard { kind: Hazard::Cave, n: 1 }),
                    Event::Fell { height_mm, .. } if height_mm >= NOTABLE_FALL_MM => Some(Entry::Hazard { kind: Hazard::Fall, n: 1 }),
                    Event::FireStarted { .. } => Some(Entry::FireStarted),
                    Event::CropFrozen { .. } => Some(Entry::Frozen),
                    Event::Camped(_) => Some(Entry::Camp),
                    _ => None,
                };
                if let Some(e) = entry {
                    self.journal.record(day, minute, e);
                }
            }
        }
        // milestones are set in many places; the bits tell which are new
        let fresh = self.milestones & !self.journal.milestones_logged;
        if fresh != 0 {
            self.journal.milestones_logged |= fresh;
            for m in Milestone::ALL {
                if fresh & m.bit() != 0 && m != Milestone::Visitor {
                    self.journal.record(day, minute, Entry::Milestone(m));
                }
            }
        }
        // once a second: what was made, the sky, the goal
        if tick % 60 == 59 {
            let at = self.ink();
            self.journal.produced_mark.resize(ItemKind::COUNT, 0);
            for i in 0..ItemKind::COUNT {
                let (now, mark) = (self.produced[i], self.journal.produced_mark[i]);
                if now > mark {
                    let item = ItemKind::ALL[i];
                    self.journal.tally_made(day, item, (now - mark).min(u16::MAX as u32) as u16);
                    if mark == 0 && self.journal.first(item).is_none() {
                        self.journal.firsts.push(First { item, day, minute, at });
                        self.journal.record(day, minute, Entry::FirstMade { item, at });
                    }
                    self.journal.produced_mark[i] = now;
                }
            }
            let sky = match self.weather() {
                crate::time::Weather::Clear => 0,
                crate::time::Weather::Rain => 1,
                crate::time::Weather::Snow => 2,
            };
            if sky != self.journal.sky {
                self.journal.sky = sky;
                if sky != 0 {
                    self.journal.record(day, minute, Entry::Sky(sky));
                }
            }
            if let Some(gs) = self.journal.goal
                && gs.done_day.is_none() && self.goal_met(&gs) {
                    if let Some(g) = self.journal.goal.as_mut() {
                        g.done_day = Some(day);
                    }
                    self.journal.record(day, minute, Entry::GoalDone(gs.goal));
                    self.events.push(Event::GoalDone(gs.goal));
                }
        }
    }
}

impl World {
    /// Developer aid (`--demo`): a few days of made-up history in the journal, for looking at its pages.
    pub fn demo_journal(&mut self, days: u32) {
        let today = self.day() as u32;
        self.journal = Default::default();
        self.journal.started = true;
        self.journal.milestones_logged = self.milestones;
        self.journal.produced_mark = self.produced.to_vec();
        let known: Vec<_> = DISCOVERY_ORDER.iter().copied().filter(|d| self.knows(*d)).collect();
        let items = [ItemKind::Wood, ItemKind::Stone, ItemKind::Fiber, ItemKind::Berries, ItemKind::Clay, ItemKind::Herb];
        for (n, d) in known.iter().enumerate() {
            let day = today.saturating_sub(days) + (n as u32 * (days + 1)) / known.len().max(1) as u32;
            let minute = 480 + (n as u16 * 37) % 600;
            let at = if n % 3 == 0 { Some((40 + n as i32, 52 - n as i32)) } else { None };
            self.journal.finds.push(Find { d: *d, day, minute, at });
            self.journal.record(day, minute, Entry::Discovered { d: *d, at });
            for (i, k) in items.iter().enumerate() {
                self.journal.tally_gathered(day, *k, 3 + (n + i) as u16 % 7);
            }
        }
        self.journal.record(today, 640, Entry::Hazard { kind: Hazard::Bite, n: 1 });
        self.journal.record(today, 700, Entry::Sky(1));
        if let Some(b) = self.rules().buildings.iter().find(|b| self.knows(b.discovery)) {
            self.journal.record(today, 560, Entry::Built { kind: b.kind, at: None });
        }
        self.journal.tally_made(today, ItemKind::Stone, 2);
        self.journal.firsts.push(First { item: ItemKind::Fiber, day: today, minute: 600, at: None });
        self.journal.record(today, 600, Entry::FirstMade { item: ItemKind::Fiber, at: None });
    }
}
