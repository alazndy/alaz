//! Debug gates: a small script language that drives the game the way a player would, one line at a time, and
//! logs what happens. The same runner works in the game (`ALAZ_SCRIPT=file`) and headless in tests, so any part
//! of the game can be reached, played and looked at without hands on the keys.
//!
//! A script is one command per line (`#` starts a comment). Player commands wait until they are done (or give up
//! after a while), so a script reads like a list of things to do:
//!
//! ```text
//! gather stone 2        # walk to the nearest stone and take two
//! gather deadwood 2
//! gather fiber_plant 4
//! combine stone stone   # try what two stones make (knapping starts if it is known)
//! craft sharp_stone     # play a recipe's minigame perfectly
//! build campfire        # on free ground near by
//! eat berries / drink / walk 1 0 5 / goto 1200 800 / sprint on / jump / wait 3
//! startjob furnace smelt_copper / deposit depot / collect furnace
//! tp grass|forest|sand|coast|swim|peak / give stone 10 / hour 21 / learn / shot NAME / state
//! ```
//!
//! Debug-only lines (`tp`, `give`, `hour`, `learn`) change the world directly and are marked so in the log.

use crate::buildings::BuildingKind;
use crate::command::{AgentId, Command, CommandKind, NodeId, RecipeId};
use crate::map::{ItemKind, NodeKind, Terrain, TILE_MM};
use crate::world::{Event, World};

/// An item, node or building name as scripts write it (snake case of the Rust name).
pub fn snake(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    out
}

pub fn item_named(s: &str) -> Option<ItemKind> {
    ItemKind::ALL.iter().copied().find(|k| snake(&format!("{k:?}")) == s)
}

const NODES: [NodeKind; 11] = [
    NodeKind::StoneOutcrop,
    NodeKind::Tree,
    NodeKind::BerryBush,
    NodeKind::ClayDeposit,
    NodeKind::IronOutcrop,
    NodeKind::HerbPatch,
    NodeKind::CopperOutcrop,
    NodeKind::TinOutcrop,
    NodeKind::Deadwood,
    NodeKind::FiberPlant,
    NodeKind::CaveMouth,
];

pub fn node_named(s: &str) -> Option<NodeKind> {
    // short names too: "stone" for a stone outcrop, "fiber" for the fibre plant, "berry"
    let short = |k: NodeKind| match k {
        NodeKind::StoneOutcrop => "stone",
        NodeKind::Tree => "tree",
        NodeKind::BerryBush => "berry",
        NodeKind::ClayDeposit => "clay",
        NodeKind::IronOutcrop => "iron",
        NodeKind::HerbPatch => "herb",
        NodeKind::CopperOutcrop => "copper",
        NodeKind::TinOutcrop => "tin",
        NodeKind::Deadwood => "deadwood",
        NodeKind::FiberPlant => "fiber",
        NodeKind::CaveMouth => "cave",
    };
    NODES.iter().copied().find(|&k| snake(&format!("{k:?}")) == s || short(k) == s)
}

pub fn building_named(s: &str) -> Option<BuildingKind> {
    BuildingKind::ALL.iter().copied().find(|k| snake(&format!("{k:?}")) == s)
}

/// What the script is waiting for before the next line.
#[derive(Clone, Debug)]
enum Wait {
    None,
    /// Until this tick.
    Until(u64),
    /// Gathering: until the pack holds `target` of the item (re-aiming at the next node as each runs out).
    Gather { node: NodeKind, item: ItemKind, target: u16, give_up: u64 },
    /// A minigame being played perfectly, then done.
    Craft { give_up: u64 },
    /// Walking (held keys): every fifth of a second another step that way, until the tick.
    Walk { dx: i64, dy: i64, until: u64 },
    /// Until the agent stands idle again (moving somewhere, drinking, eating).
    Idle { give_up: u64 },
}

/// A script being run against a world for one agent.
pub struct Script {
    lines: Vec<String>,
    next: usize,
    wait: Wait,
    /// Everything that happened, as lines: commands, waits, events of note, results.
    pub log: Vec<String>,
    /// Screenshot requests the front end should act on (names), taken out by the caller.
    pub shots: Vec<String>,
    /// Lines that could not be carried out.
    pub failures: Vec<String>,
    /// The line that has already walked up to its building once (so it does not walk for ever).
    walked: Option<usize>,
}

fn secs(w: &World) -> f64 {
    w.tick as f64 / crate::time::TICKS_PER_SECOND as f64
}

impl Script {
    pub fn new(text: &str) -> Self {
        let lines = text.lines().map(|l| l.split('#').next().unwrap_or("").trim().to_string()).filter(|l| !l.is_empty()).collect();
        Self { lines, next: 0, wait: Wait::None, log: Vec::new(), shots: Vec::new(), failures: Vec::new(), walked: None }
    }

    pub fn done(&self) -> bool {
        self.next >= self.lines.len() && matches!(self.wait, Wait::None)
    }

    fn note(&mut self, w: &World, s: impl AsRef<str>) {
        self.log.push(format!("[{:7.1}s] {}", secs(w), s.as_ref()));
    }

    fn fail(&mut self, w: &World, s: impl AsRef<str>) {
        let line = format!("[{:7.1}s] FAIL {}", secs(w), s.as_ref());
        self.failures.push(line.clone());
        self.log.push(line);
    }

    /// Log the events of a step that a player would notice (call with the world's events before they are taken).
    pub fn watch(&mut self, w: &World, me: AgentId, events: &[Event]) {
        for e in events {
            let mine = match e {
                Event::Rejected(a, why) if *a == me => {
                    // a refusal ends what the line was waiting for: a player would stop and think
                    // a heap whose loose stones ran out mid-gather: on to the next one, as a player would
                    let next_heap = matches!(self.wait, Wait::Gather { .. }) && matches!(why, crate::world::Reject::NeedsTool { .. });
                    if !next_heap && !matches!(self.wait, Wait::None | Wait::Until(_)) {
                        self.wait = Wait::None;
                        self.failures.push(format!("[{:7.1}s] FAIL refused: {why:?}", secs(w)));
                    }
                    Some(format!("rejected: {why:?}"))
                }
                Event::Discovered(d) => Some(format!("discovered: {d:?}")),
                Event::Milestone(m) => Some(format!("milestone: {m:?}")),
                Event::Died(a) if *a == me => Some("DIED".to_string()),
                Event::Bitten(a) if *a == me => Some("bitten".to_string()),
                _ => None,
            };
            if let Some(s) = mine {
                self.note(w, s);
            }
        }
    }

    /// Run the script for one tick: the commands to send this tick (debug lines change the world directly).
    pub fn tick(&mut self, w: &mut World, me: AgentId) -> Vec<Command> {
        let cmd = |kind: CommandKind| Command { agent: me, kind };
        let Some(a) = w.agents.iter().find(|a| a.id == me).cloned() else { return Vec::new() };
        // carry on with what is under way
        match self.wait.clone() {
            Wait::None => {}
            Wait::Until(t) => {
                if w.tick < t {
                    return Vec::new();
                }
                self.wait = Wait::None;
            }
            Wait::Gather { node, item, target, give_up } => {
                if a.inventory.get(item) >= target {
                    self.note(w, format!("  have {} {item:?}", a.inventory.get(item)));
                    self.wait = Wait::None;
                } else if w.tick >= give_up {
                    self.fail(w, format!("gather {node:?}: only {} of {target} {item:?} in time", a.inventory.get(item)));
                    self.wait = Wait::None;
                } else if matches!(a.task, crate::world::Task::Idle) && w.tick % 30 == 0 {
                    // the node ran out or the pick-up is done: the next nearest
                    return match nearest(w, &a, node) {
                        Some(id) => vec![cmd(CommandKind::Gather { node: id })],
                        None => {
                            self.fail(w, format!("gather {node:?}: none left in reach"));
                            self.wait = Wait::None;
                            Vec::new()
                        }
                    };
                } else {
                    return Vec::new();
                }
            }
            Wait::Craft { give_up } => {
                if let Some(v) = w.craft_view(me) {
                    if w.tick >= give_up {
                        self.fail(w, "craft: still at it, gave up");
                        self.wait = Wait::None;
                        return vec![cmd(CommandKind::Walk { x: a.x + 1, y: a.y })];
                    }
                    // a perfect hand, every few ticks
                    if w.tick % 4 != 0 {
                        return Vec::new();
                    }
                    let k = match v.kind {
                        crate::rules::MinigameKind::Knap => Some(CommandKind::StrikeAt { angle: v.center as u16, power: v.power as u8 }),
                        crate::rules::MinigameKind::Fit => Some(CommandKind::PlaceAt { x: v.tx as i16, y: v.ty as i16 }),
                        crate::rules::MinigameKind::Tension => Some(CommandKind::Tighten { power: v.power as u8 }),
                        crate::rules::MinigameKind::Timing => (v.pos.abs_diff(v.center) <= v.half / 3).then_some(CommandKind::Strike),
                        crate::rules::MinigameKind::Stack => Some(CommandKind::PlaceAt { x: 0, y: 0 }),
                    };
                    return k.map(|k| vec![cmd(k)]).unwrap_or_default();
                }
                self.wait = Wait::None;
            }
            Wait::Walk { dx, dy, until } => {
                if w.tick >= until {
                    self.wait = Wait::None;
                } else {
                    if w.tick % 12 == 0 {
                        let len = ((dx * dx + dy * dy) as f64).sqrt().max(1.0);
                        return vec![cmd(CommandKind::Walk { x: a.x + (dx as f64 / len * 4_000.0) as i64, y: a.y + (dy as f64 / len * 4_000.0) as i64 })];
                    }
                    return Vec::new();
                }
            }
            Wait::Idle { give_up } => {
                if matches!(a.task, crate::world::Task::Idle) || w.tick >= give_up {
                    self.wait = Wait::None;
                } else {
                    return Vec::new();
                }
            }
        }
        // the next line
        let Some(line) = self.lines.get(self.next).cloned() else { return Vec::new() };
        self.next += 1;
        self.note(w, format!("> {line}"));
        let p: Vec<&str> = line.split_whitespace().collect();
        let arg = |i: usize| p.get(i).copied().unwrap_or("");
        let num = |i: usize, d: i64| p.get(i).and_then(|v| v.parse::<i64>().ok()).unwrap_or(d);
        let soon = w.tick + 60 * 240;
        match arg(0) {
            "wait" => self.wait = Wait::Until(w.tick + (num(1, 1) * 60) as u64),
            "shot" => self.shots.push(arg(1).to_string()),
            "state" => {
                let held: Vec<String> = ItemKind::ALL.iter().filter(|k| a.inventory.get(**k) > 0).map(|k| format!("{} {}", snake(&format!("{k:?}")), a.inventory.get(*k))).collect();
                let s = format!(
                    "  at ({:.1}, {:.1}) m on {:?}; health {} food {} water {} warm {} breath {}; pack: {}; task {:?}",
                    a.x as f64 / 1000.0,
                    a.y as f64 / 1000.0,
                    w.map.terrain_mm(a.x, a.y),
                    a.health,
                    a.hunger,
                    a.thirst,
                    a.warmth,
                    a.stamina,
                    held.join(", "),
                    a.task
                );
                self.note(w, s);
            }
            "gather" => match node_named(arg(1)) {
                Some(node) => {
                    let item = node.item();
                    let target = a.inventory.get(item) + num(2, 1) as u16;
                    match nearest(w, &a, node) {
                        Some(id) => {
                            self.wait = Wait::Gather { node, item, target, give_up: soon };
                            return vec![cmd(CommandKind::Gather { node: id })];
                        }
                        None => self.fail(w, format!("gather: no {node:?} within reach")),
                    }
                }
                None => self.fail(w, format!("gather: unknown node {}", arg(1))),
            },
            "craft" => {
                let rules = w.rules();
                match rules.recipes.iter().position(|r| r.id == arg(1)) {
                    Some(r) => {
                        self.wait = Wait::Craft { give_up: soon };
                        return vec![cmd(CommandKind::Craft { recipe: RecipeId(r as u16) })];
                    }
                    None => self.fail(w, format!("craft: unknown recipe {}", arg(1))),
                }
            }
            "combine" => {
                let mut slots = [255u8; 4];
                for (i, name) in p.iter().skip(1).take(4).enumerate() {
                    match item_named(name) {
                        Some(k) => slots[i] = k as u8,
                        None => self.fail(w, format!("combine: unknown item {name}")),
                    }
                }
                self.wait = Wait::Craft { give_up: soon };
                return vec![cmd(CommandKind::Combine { items: slots })];
            }
            "build" => match building_named(arg(1)) {
                Some(kind) => match free_spot(w, &a, kind) {
                    Some((x, y)) => {
                        self.wait = Wait::Until(w.tick + 2);
                        return vec![cmd(CommandKind::Build { kind, x, y })];
                    }
                    None => self.fail(w, "build: no free ground near by"),
                },
                None => self.fail(w, format!("build: unknown building {}", arg(1))),
            },
            "startjob" | "deposit" | "collect" | "refuel" => match building_named(arg(1)).and_then(|k| nearest_building(w, &a, k)) {
                // first walk up to it, as a player would, then do the line again
                Some(b) if (w.buildings[b as usize].x - a.x).pow(2) + (w.buildings[b as usize].y - a.y).pow(2) > 2_400 * 2_400 && self.walked != Some(self.next) => {
                    self.walked = Some(self.next);
                    self.next -= 1;
                    let (bx, by) = (w.buildings[b as usize].x, w.buildings[b as usize].y);
                    let d = (((bx - a.x).pow(2) + (by - a.y).pow(2)) as f64).sqrt().max(1.0);
                    let (x, y) = (bx - ((bx - a.x) as f64 / d * 1_600.0) as i64, by - ((by - a.y) as f64 / d * 1_600.0) as i64);
                    self.wait = Wait::Idle { give_up: soon };
                    return vec![cmd(CommandKind::MoveTo { x, y })];
                }
                Some(b) => {
                    let kind = match arg(0) {
                        "deposit" => Some(CommandKind::Deposit { building: b }),
                        "collect" => Some(CommandKind::Collect { building: b, item: item_named(arg(2)) }),
                        "refuel" => Some(CommandKind::Refuel { building: b }),
                        _ => w.rules().recipes.iter().position(|r| r.id == arg(2)).map(|r| CommandKind::StartJob { building: b, recipe: RecipeId(r as u16) }),
                    };
                    match kind {
                        Some(k) => {
                            self.wait = Wait::Idle { give_up: soon };
                            return vec![cmd(k)];
                        }
                        None => self.fail(w, format!("{}: unknown recipe {}", arg(0), arg(2))),
                    }
                }
                None => self.fail(w, format!("{}: no {} near by", arg(0), arg(1))),
            },
            "eat" => match item_named(arg(1)) {
                Some(item) => {
                    self.wait = Wait::Until(w.tick + 2);
                    return vec![cmd(CommandKind::Eat { item })];
                }
                None => self.fail(w, format!("eat: unknown item {}", arg(1))),
            },
            "drink" => match crate::world::nearest_water_tile(w, a.x, a.y, 60) {
                Some((tx, ty)) => {
                    self.wait = Wait::Idle { give_up: soon };
                    return vec![cmd(CommandKind::Drink { tx, ty })];
                }
                None => self.fail(w, "drink: no water within 60 m"),
            },
            "walk" => self.wait = Wait::Walk { dx: num(1, 1), dy: num(2, 0), until: w.tick + (num(3, 3) * 60) as u64 },
            "goto" => {
                self.wait = Wait::Idle { give_up: soon };
                return vec![cmd(CommandKind::MoveTo { x: num(1, 0) * 1000, y: num(2, 0) * 1000 })];
            }
            "sprint" => return vec![cmd(CommandKind::SetSprint { on: arg(1) != "off" })],
            "jump" => return vec![cmd(CommandKind::Jump)],
            "prospect" => return vec![cmd(CommandKind::Prospect)],
            "fish" => {
                // the nearest school within 60 m, fished from where it lies
                let (tx, ty) = (a.x.div_euclid(TILE_MM) as i32, a.y.div_euclid(TILE_MM) as i32);
                match crate::map::school_near(&w.map, tx, ty, 60) {
                    Some((sx, sy)) => {
                        self.wait = Wait::Until(w.tick + (num(1, 30) * 60) as u64);
                        return vec![cmd(CommandKind::Fish { tx: sx, ty: sy })];
                    }
                    None => self.fail(w, "fish: no school within 60 m"),
                }
            }
            "drop" => match item_named(arg(1)) {
                Some(item) => return vec![cmd(CommandKind::Drop { item, n: num(2, 1) as u16 })],
                None => self.fail(w, format!("drop: unknown item {}", arg(1))),
            },
            "pick" | "split" => {
                // the nearest piece lying about, or the nearest felled trunk
                let want = if arg(0) == "pick" { crate::physics::BodyKind::Piece } else { crate::physics::BodyKind::Tree };
                match w.bodies.iter().filter(|b| b.kind == want).min_by_key(|b| { let (x, y, _) = b.pos_mm(); (x - a.x).pow(2) + (y - a.y).pow(2) }).map(|b| b.id) {
                    Some(body) => {
                        self.wait = Wait::Idle { give_up: soon };
                        return vec![cmd(if arg(0) == "pick" { CommandKind::Pick { body } } else { CommandKind::Split { body } })];
                    }
                    None => self.fail(w, format!("{}: nothing lying about", arg(0))),
                }
            }
            "hunt" | "tame" => {
                let species = match arg(1) { "dog" => crate::wildlife::WildSpecies::Dog, "ox" => crate::wildlife::WildSpecies::Ox, "horse" => crate::wildlife::WildSpecies::Horse, "wolf" => crate::wildlife::WildSpecies::Wolf, _ => crate::wildlife::WildSpecies::Deer };
                match w.wild.iter().filter(|x| x.species == species).min_by_key(|x| (x.x - a.x).pow(2) + (x.y - a.y).pow(2)).map(|x| x.id) {
                    Some(id) => {
                        self.wait = Wait::Idle { give_up: soon };
                        return vec![cmd(if arg(0) == "hunt" { CommandKind::Hunt { wild: id } } else { CommandKind::Tame { wild: id } })];
                    }
                    None => self.fail(w, format!("{}: no {species:?} about", arg(0))),
                }
            }
            "feed" | "hitch" | "route" => match w
                .agents
                .iter()
                // a cart or plow goes on a draught animal; the rest to any worker
                .filter(|x| x.kind.is_worker() && (arg(0) != "hitch" || matches!(x.kind, crate::agents::AgentKind::Ox | crate::agents::AgentKind::Horse)))
                .min_by_key(|x| (x.x - a.x).pow(2) + (x.y - a.y).pow(2)).map(|x| x.id) {
                Some(worker) => {
                    let k = match arg(0) {
                        "feed" => CommandKind::Feed { target: worker },
                        "hitch" => CommandKind::Hitch { worker, item: item_named(arg(1)).unwrap_or(ItemKind::Cart) },
                        // route: harvest NODE round here, put it in the nearest depot
                        _ => {
                            let depot = nearest_building(w, &a, BuildingKind::Depot);
                            let mut route = vec![crate::agents::RouteStep::Harvest { kind: node_named(arg(1)).unwrap_or(NodeKind::Tree), x: a.x, y: a.y, radius_mm: 30_000 }];
                            if let Some(d) = depot {
                                route.push(crate::agents::RouteStep::Deposit { building: d });
                            }
                            CommandKind::SetRoute { worker, route }
                        }
                    };
                    self.wait = Wait::Until(w.tick + 2);
                    return vec![cmd(k)];
                }
                None => self.fail(w, format!("{}: no worker about", arg(0))),
            },
            "auto" => match building_named(arg(1)).and_then(|k| nearest_building(w, &a, k)) {
                Some(b) => {
                    let recipe = w.rules().recipes.iter().position(|r| r.id == arg(2)).map(|r| RecipeId(r as u16));
                    return vec![cmd(CommandKind::SetAuto { building: b, recipe })];
                }
                None => self.fail(w, format!("auto: no {} near by", arg(1))),
            },
            "cave" => {
                let k = match arg(1) {
                    "mine" => Some(CommandKind::CaveMine),
                    "deeper" => Some(CommandKind::CaveDeeper),
                    "leave" => Some(CommandKind::CaveLeave),
                    _ => nearest(w, &a, NodeKind::CaveMouth).map(|node| CommandKind::EnterCave { node }),
                };
                match k {
                    Some(k) => {
                        self.wait = Wait::Until(w.tick + 120);
                        return vec![cmd(k)];
                    }
                    None => self.fail(w, "cave: no cave mouth within reach"),
                }
            }
            "repair" => match item_named(arg(1)) {
                Some(item) => return vec![cmd(CommandKind::Repair { item })],
                None => self.fail(w, format!("repair: unknown item {}", arg(1))),
            },
            "design" => {
                // design OUT N IN N [IN N ..]
                let out = item_named(arg(1)).map(|k| (k, num(2, 1) as u16));
                let ins: Vec<(ItemKind, u16)> = (3..p.len()).step_by(2).filter_map(|i| item_named(p[i]).map(|k| (k, p.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(1)))).collect();
                match out {
                    Some(output) => return vec![cmd(CommandKind::Design { inputs: ins, output })],
                    None => self.fail(w, format!("design: unknown item {}", arg(1))),
                }
            }
            "cycle" => {
                // debug: the loop, as the computer starts it
                let mut next = w.next_cycle();
                if next.agents.is_empty() {
                    let (sx, sy) = next.spawn_mm;
                    next.spawn_agent(sx, sy);
                }
                *w = next;
                self.note(w, "  (debug) a new world from this one");
            }
            "worker" => {
                // debug: a worker of a kind beside you (bot, dog, ox, horse)
                let kind = match arg(1) { "dog" => crate::agents::AgentKind::Dog, "ox" => crate::agents::AgentKind::Ox, "horse" => crate::agents::AgentKind::Horse, _ => crate::agents::AgentKind::Bot };
                w.spawn_worker(kind, a.x + 1_500, a.y);
                self.note(w, format!("  (debug) a {kind:?} beside you"));
            }
            // debug-only: the world is changed directly
            "give" => match item_named(arg(1)) {
                Some(k) => {
                    if let Some(ag) = w.agents.iter_mut().find(|x| x.id == me) {
                        ag.inventory.counts[k as usize] += num(2, 1) as u16;
                    }
                    w.mark_seen(k);
                    self.note(w, "  (debug) given");
                }
                None => self.fail(w, format!("give: unknown item {}", arg(1))),
            },
            "learn" => {
                w.learn_everything();
                self.note(w, "  (debug) everything learned");
            }
            "hour" => {
                // on to the next time the clock shows that hour
                let day = crate::time::TICKS_PER_DAY;
                let want = (num(1, 12) as u64 % 24) * 60;
                let now = crate::time::minute_of_day(w.tick) as u64;
                let minutes = if want >= now { want - now } else { 1440 - now + want };
                let skip = minutes * day / 1440;
                w.tick += skip;
                self.note(w, format!("  (debug) clock moved on {skip} ticks"));
            }
            "tp" => match teleport_spot(w, arg(1)) {
                Some((x, y)) => {
                    if let Some(ag) = w.agents.iter_mut().find(|x| x.id == me) {
                        ag.x = x;
                        ag.y = y;
                        ag.task = crate::world::Task::Idle;
                    }
                    self.note(w, format!("  (debug) moved to ({}, {}) m", x / 1000, y / 1000));
                }
                None => self.fail(w, format!("tp: no {} ground found", arg(1))),
            },
            other => self.fail(w, format!("unknown command {other}")),
        }
        Vec::new()
    }
}

/// The nearest live node of a kind, within 120 m.
fn nearest(w: &World, a: &crate::world::Agent, kind: NodeKind) -> Option<NodeId> {
    let (tx, ty) = (a.x.div_euclid(TILE_MM) as i32, a.y.div_euclid(TILE_MM) as i32);
    let r = 120;
    w.nodes_in(tx - r, ty - r, tx + r, ty + r)
        // only what this hand can take now: a stone heap with its loose stones gone wants a pick
        .filter(|n| n.kind == kind && crate::world::gate_check(n, a.kind, &a.inventory).is_ok())
        .min_by_key(|n| (n.tx - tx).pow(2) + (n.ty - ty).pow(2))
        .and_then(|n| w.node_at(n.tx, n.ty))
}

fn nearest_building(w: &World, a: &crate::world::Agent, kind: BuildingKind) -> Option<u32> {
    w.buildings
        .iter()
        .enumerate()
        .filter(|(_, b)| b.kind == kind)
        .min_by_key(|(_, b)| (b.x - a.x).pow(2) + (b.y - a.y).pow(2))
        .map(|(i, _)| i as u32)
}

/// Ground near the agent where this building may go (the game's own placing rules), nearest first, out to 40 m.
pub fn free_spot(w: &World, a: &crate::world::Agent, kind: BuildingKind) -> Option<(i64, i64)> {
    for r in 3..40i64 {
        for k in 0..(8 * r) {
            let ang = k as f64 / (8 * r) as f64 * std::f64::consts::TAU;
            let (x, y) = (a.x + (ang.cos() * r as f64 * 1000.0) as i64, a.y + (ang.sin() * r as f64 * 1000.0) as i64);
            if w.can_place(kind, x, y).is_ok() {
                return Some((x, y));
            }
        }
    }
    None
}

/// A spot of a kind of ground, searched outward from the spawn.
fn teleport_spot(w: &World, what: &str) -> Option<(i64, i64)> {
    let (sx, sy) = ((w.spawn_mm.0 / TILE_MM) as i32, (w.spawn_mm.1 / TILE_MM) as i32);
    let ok = |tx: i32, ty: i32| -> bool {
        let t = w.map.terrain(tx, ty);
        match what {
            "grass" => t == Some(Terrain::Grass),
            "forest" => t == Some(Terrain::Forest),
            "sand" => t == Some(Terrain::Sand),
            "rock" => t == Some(Terrain::Rock),
            "coast" => t == Some(Terrain::Sand) && (2..6).all(|d| w.map.terrain(tx + d, ty) == Some(Terrain::Water)),
            "swim" => crate::map::water_depth(&w.map, tx, ty) == 2 && (-4..=4).any(|d| w.map.terrain(tx + d, ty).is_some_and(|t| t != Terrain::Water)),
            "peak" => crate::map::elevation_mm(w.seed, tx, ty) > 9_000,
            "spawn" => true,
            // beside a cave mouth
            "cave" => (-2..=2).any(|d| w.map.node_for(w.seed, tx + d, ty + 2) == Some(NodeKind::CaveMouth)) && matches!(t, Some(Terrain::Grass | Terrain::Forest | Terrain::Sand | Terrain::Rock)),
            _ => false,
        }
    };
    for r in 0..1200i32 {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx.abs().max(dy.abs()) == r && ok(sx + dx, sy + dy) {
                    return Some(((sx + dx) as i64 * TILE_MM + TILE_MM / 2, (sy + dy) as i64 * TILE_MM + TILE_MM / 2));
                }
            }
        }
    }
    None
}
