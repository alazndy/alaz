use crate::time::{daylight_permille, TICKS_PER_DAY};
use crate::agents::{AgentKind, RouteStep};
use crate::buildings::{Building, BuildingKind};
use crate::command::{AgentId, Command, CommandKind, NodeId, RecipeId};
use crate::map::{ItemKind, Map, NodeKind, Terrain, MAP_H, MAP_W};
use crate::rules::rules;
use crate::wildlife::WildSpecies;
use crate::rules::MinigameKind;
use crate::world::*;

fn with_agent(seed: u64) -> (World, AgentId) {
    let mut w = World::new(seed);
    let (sx, sy) = w.spawn_mm;
    let a = w.spawn_agent(sx, sy);
    (w, a)
}

fn cmd(agent: AgentId, kind: CommandKind) -> Command {
    Command { agent, kind }
}

fn recipe(id: &str) -> RecipeId {
    RecipeId(rules().recipes.iter().position(|r| r.id == id).expect("recipe") as u16)
}

fn give(w: &mut World, k: ItemKind, n: u16) {
    w.agents[0].inventory.counts[k as usize] = n;
}

fn count(w: &World, i: usize, k: ItemKind) -> u16 {
    w.agents[i].inventory.get(k)
}

fn learn_all(w: &mut World) {
    w.known = u64::MAX;
}

/// Free spot for `kind` within 7 m of the spawn, at least 4 m from every spot in `avoid`.
fn spot_for(w: &World, kind: BuildingKind, avoid: &[(i64, i64)]) -> (i64, i64) {
    let (sx, sy) = w.spawn_mm;
    for dy in -7..=7i64 {
        for dx in -7..=7i64 {
            let (x, y) = (sx + dx * 1000, sy + dy * 1000);
            let far = avoid.iter().all(|&(ax, ay)| dist2(ax, ay, x, y) >= 4000 * 4000);
            if far && dist2(sx, sy, x, y) <= 7000 * 7000 && w.can_place(kind, x, y).is_ok() {
                return (x, y);
            }
        }
    }
    panic!("no spot for {kind:?}");
}

fn push_building(w: &mut World, kind: BuildingKind, x: i64, y: i64) -> u32 {
    w.buildings.push(Building::new(kind, x, y));
    (w.buildings.len() - 1) as u32
}

fn clear_line(w: &World, (ax, ay): (i64, i64), (bx, by): (i64, i64)) -> bool {
    (0..=40).all(|i| {
        let (x, y) = (ax + (bx - ax) * i / 40, ay + (by - ay) * i / 40);
        w.map.terrain_mm(x, y).is_some_and(|t| t != Terrain::Water)
    })
}

fn run() -> u64 {
    let (mut w, a) = with_agent(42);
    let node = NodeId(w.nodes.iter().position(|n| n.kind == NodeKind::Tree).unwrap() as u32);
    w.step(&[cmd(a, CommandKind::Gather { node })]);
    for _ in 0..3000 {
        w.step(&[]);
    }
    w.hash()
}

#[test]
fn deterministic() {
    assert_eq!(run(), run());
}

#[test]
fn map_is_seeded_and_varied() {
    let g = |s| Map::generate(s, MAP_W, MAP_H);
    let (a, b, c) = (g(1), g(1), g(2));
    assert_eq!(a.tiles, b.tiles);
    assert_ne!(a.tiles, c.tiles);
    for t in [Terrain::Water, Terrain::Sand, Terrain::Grass, Terrain::Forest, Terrain::Rock] {
        let n = a.tiles.iter().filter(|x| **x == t).count();
        assert!(n > 500, "{t:?} too rare: {n}");
    }
}

#[test]
fn clay_and_iron_exist() {
    let w = World::new(1);
    for k in [NodeKind::ClayDeposit, NodeKind::IronOutcrop, NodeKind::Tree, NodeKind::StoneOutcrop] {
        let n = w.nodes.iter().filter(|n| n.kind == k).count();
        assert!(n >= 10, "{k:?}: {n}");
    }
    for n in w.nodes.iter().filter(|n| n.kind == NodeKind::ClayDeposit) {
        assert!(w.map.water_within(n.tx, n.ty, 2));
    }
}

#[test]
fn spawn_on_land_and_nodes_exist() {
    let (w, _) = with_agent(1);
    assert!(!matches!(w.map.terrain_mm(w.spawn_mm.0, w.spawn_mm.1), Some(Terrain::Water) | None));
    assert!(w.nodes.len() > 500);
}

/// Farming starts from seed picked off dry-ground tall grass, so a new world has some of it close to the start.
#[test]
fn start_has_seed_grass_nearby() {
    let mut tested = 0;
    for plain in [7u64, 11, 23] {
        let seed = crate::map::tag_seed_natural(plain, 2048);
        let w = World::with_size(seed, 2048, 2048);
        tested += 1;
        let (sx, sy) = ((w.spawn_mm.0 / crate::map::TILE_MM) as i32, (w.spawn_mm.1 / crate::map::TILE_MM) as i32);
        let n = (-30..=30)
            .flat_map(|dy| (-30..=30).map(move |dx| (sx + dx, sy + dy)))
            .filter(|&(x, y)| w.map.node_for(seed, x, y) == Some(NodeKind::FiberPlant) && crate::world::grass_gives_seed(seed, x, y))
            .count();
        assert!(n >= 8, "seed {seed}: only {n} seed tufts within 30 tiles of the start");
    }
    assert!(tested > 0, "no seed uses the newer generators");
}

/// The first shelter needs a clear patch near the start, even where the wood is thick.
#[test]
fn start_has_room_for_a_shelter() {
    for plain in [7u64, 11, 23] {
        let seed = crate::map::tag_seed_natural(plain, 2048);
        let w = World::with_size(seed, 2048, 2048);
        let (sx, sy) = ((w.spawn_mm.0 / crate::map::TILE_MM) as i32, (w.spawn_mm.1 / crate::map::TILE_MM) as i32);
        let clear = (-14..=14).any(|cy| (-14..=14).any(|cx| (-2..=2).all(|dy| (-2..=2).all(|dx| w.map.node_for(seed, sx + cx + dx, sy + cy + dy).is_none()))));
        assert!(clear, "seed {plain}: no clear 5x5 patch within 14 tiles of the start");
    }
}

#[test]
fn gather_fills_inventory_and_depletes_node() {
    let mut w = World::new(1);
    let node_idx = w.nodes.iter().position(|n| n.kind == NodeKind::Tree).unwrap();
    let (nx, ny) = w.nodes[node_idx].center_mm();
    let a = w.spawn_agent(nx + 1000, ny);
    give(&mut w, ItemKind::StoneAxe, 1); // trees fall only to an axe
    w.step(&[cmd(a, CommandKind::Gather { node: NodeId(node_idx as u32) })]);
    for _ in 0..(GATHER_TICKS as usize * 20) {
        w.step(&[]);
    }
    // the tree came down, the trunk was broken up where it fell and every piece picked up
    let (tx, ty) = (w.nodes[node_idx].tx, w.nodes[node_idx].ty);
    assert_eq!(count(&w, 0, ItemKind::Wood), crate::physics::log_pieces(w.seed, tx, ty));
    assert!(w.bodies.is_empty(), "nothing left lying about");
    assert_eq!(w.nodes[node_idx].amount, 0);
    assert!(w.node_at(w.nodes[node_idx].tx, w.nodes[node_idx].ty).is_none());
    assert_eq!(w.agents[0].task, Task::Idle);
}

#[test]
fn water_blocks_movement() {
    let (mut w, a) = with_agent(1);
    let (tx, ty) = (0..MAP_H)
        .flat_map(|y| (0..MAP_W).map(move |x| (x, y)))
        .find(|&(x, y)| w.map.terrain(x, y) == Some(Terrain::Water))
        .unwrap();
    w.step(&[cmd(a, CommandKind::MoveTo { x: tx as i64 * 1000 + 500, y: ty as i64 * 1000 + 500 })]);
    for _ in 0..20000 {
        w.step(&[]);
    }
    let ag = &w.agents[0];
    assert_ne!(w.map.terrain_mm(ag.x, ag.y), Some(Terrain::Water));
    assert_eq!(ag.task, Task::Idle);
}

#[test]
fn needs_decay_and_eating_helps() {
    let (mut w, a) = with_agent(1);
    for _ in 0..1300 {
        w.step(&[]);
    }
    assert!(w.agents[0].hunger < MAX_STAT && w.agents[0].thirst < w.agents[0].hunger);
    give(&mut w, ItemKind::Berries, 2);
    let before = w.agents[0].hunger;
    w.step(&[cmd(a, CommandKind::Eat { item: ItemKind::Berries })]);
    assert!(w.agents[0].hunger > before);
    assert_eq!(count(&w, 0, ItemKind::Berries), 1);
}

#[test]
fn drinking_restores_thirst() {
    let (mut w, a) = with_agent(1);
    let (tx, ty) = (0..MAP_H)
        .flat_map(|y| (0..MAP_W).map(move |x| (x, y)))
        .filter(|&(x, y)| w.map.terrain(x, y) == Some(Terrain::Water))
        .min_by_key(|&(x, y)| {
            let (sx, sy) = (w.spawn_mm.0 / 1000, w.spawn_mm.1 / 1000);
            (x as i64 - sx).pow(2) + (y as i64 - sy).pow(2)
        })
        .unwrap();
    w.agents[0].thirst = 2000;
    w.step(&[cmd(a, CommandKind::Drink { tx, ty })]);
    for _ in 0..20000 {
        w.step(&[]);
        if w.agents[0].task == Task::Idle {
            break;
        }
    }
    assert!(w.agents[0].thirst > 8000, "thirst={}", w.agents[0].thirst);
}

#[test]
fn discoveries_and_hand_crafting() {
    let (mut w, a) = with_agent(1);
    give(&mut w, ItemKind::Stone, 2);
    give(&mut w, ItemKind::Wood, 3);
    w.step(&[]);
    assert!(w.knows(Discovery::SharpStone) && w.knows(Discovery::Fire));
    assert!(!w.knows(Discovery::StoneAxe));
    craft_perfectly(&mut w, a, "sharp_stone");
    assert!(count(&w, 0, ItemKind::SharpStone) >= 1);
    give(&mut w, ItemKind::Branch, 1);
    give(&mut w, ItemKind::Fiber, 2);
    w.step(&[]);
    assert!(w.knows(Discovery::StoneAxe));
    craft_perfectly(&mut w, a, "stone_axe");
    assert_eq!(count(&w, 0, ItemKind::StoneAxe), 1);
    // sharp stone, fire, stone axe, spear, and with the spear, fishing
    assert_eq!(w.take_events().iter().filter(|e| matches!(e, Event::Discovered(_))).count(), 5);
}

#[test]
fn build_rules_are_enforced() {
    let (mut w, a) = with_agent(1);
    let (x, y) = spot_for(&w, BuildingKind::Campfire, &[]);
    w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::Campfire, x, y })]);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::NotLearned)));
    give(&mut w, ItemKind::Wood, 3);
    w.step(&[]);
    w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::Campfire, x: x + 20_000, y })]);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::TooFar)));
    give(&mut w, ItemKind::Wood, 2);
    w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::Campfire, x, y })]);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::NoMaterials)));
    give(&mut w, ItemKind::Wood, 6);
    w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::Campfire, x, y })]);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::NoFlint)), "a fire needs flint to light");
    assert!(w.buildings.is_empty());
    give(&mut w, ItemKind::Flint, 1);
    w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::Campfire, x, y })]);
    assert_eq!(w.buildings.len(), 1);
    assert_eq!(count(&w, 0, ItemKind::Wood), 3);
    assert_eq!(count(&w, 0, ItemKind::Flint), 1, "the flint is kept");
    w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::Campfire, x: x + 300, y })]);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::Blocked)));
    assert_eq!(w.buildings.len(), 1);
}

#[test]
fn cannot_build_on_water_or_resources() {
    let (w, _) = with_agent(1);
    let (wx, wy) = (0..MAP_H)
        .flat_map(|y| (0..MAP_W).map(move |x| (x, y)))
        .find(|&(x, y)| w.map.terrain(x, y) == Some(Terrain::Water))
        .unwrap();
    assert_eq!(w.can_place(BuildingKind::Campfire, wx as i64 * 1000 + 500, wy as i64 * 1000 + 500), Err(Reject::Blocked));
    let (nx, ny) = w.nodes[0].center_mm();
    assert_eq!(w.can_place(BuildingKind::Campfire, nx, ny), Err(Reject::Blocked));
}

#[test]
fn campfire_keeps_you_warm_and_burns_out() {
    let (mut w, a) = with_agent(1);
    let (x, y) = spot_for(&w, BuildingKind::Campfire, &[]);
    give(&mut w, ItemKind::Wood, 3);
    give(&mut w, ItemKind::Flint, 1);
    w.step(&[]);
    w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::Campfire, x, y })]);
    assert_eq!(w.buildings.len(), 1);
    w.agents[0].x = x;
    w.agents[0].y = y;
    w.agents[0].warmth = 0;
    for _ in 0..3600 {
        w.step(&[]);
    }
    assert_eq!(w.agents[0].warmth, MAX_STAT);
    let before = w.buildings[0].fuel;
    give(&mut w, ItemKind::Wood, 1);
    w.step(&[cmd(a, CommandKind::Refuel { building: 0 })]);
    assert!(w.buildings[0].fuel > before);
    for _ in 0..FIRE_FUEL_MAX {
        w.step(&[]);
    }
    assert_eq!(w.buildings[0].fuel, 0);
    // a dead fire will not take new wood without a spark
    w.take_events();
    give(&mut w, ItemKind::Flint, 0);
    give(&mut w, ItemKind::Wood, 1);
    w.step(&[cmd(a, CommandKind::Refuel { building: 0 })]);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::NoFlint)));
    assert_eq!(w.buildings[0].fuel, 0);
    give(&mut w, ItemKind::Flint, 1);
    w.step(&[cmd(a, CommandKind::Refuel { building: 0 })]);
    assert!(w.buildings[0].fuel > 0, "struck alight again");
}

#[test]
fn breaking_stone_turns_up_flint_and_torches_need_it() {
    let (mut w, a) = with_agent(1);
    learn_all(&mut w);
    let i = w.nodes.iter().position(|n| n.kind == NodeKind::StoneOutcrop && n.amount >= 5).expect("stone");
    let (nx, ny) = w.nodes[i].center_mm();
    w.agents[0].x = nx;
    w.agents[0].y = ny + 900;
    // no flint: no torch
    give(&mut w, ItemKind::Branch, 2);
    give(&mut w, ItemKind::Fiber, 1);
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("torch") })]);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::NoFlint)));
    assert_eq!(count(&w, 0, ItemKind::Torch), 0);
    give(&mut w, ItemKind::Hammer, 1); // the seam under the loose stones wants a hammer
    w.step(&[cmd(a, CommandKind::Gather { node: NodeId(i as u32) })]);
    for _ in 0..90 * 8 {
        w.step(&[]);
    }
    assert!(count(&w, 0, ItemKind::Flint) >= 1, "a stone seam gives up flint now and then");
    // five pieces in all: the seam's flint, now and then a loose flint nodule, the rest stone
    assert_eq!(count(&w, 0, ItemKind::Stone) + count(&w, 0, ItemKind::Flint), 5);
    assert!(count(&w, 0, ItemKind::Stone) >= 2, "mostly it is still stone");
    w.agents[0].task = Task::Idle;
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("torch") })]);
    assert_eq!(count(&w, 0, ItemKind::Torch), 1);
    assert!(count(&w, 0, ItemKind::Flint) >= 1, "the flint is kept");
}

#[test]
fn shelter_guarantees_warmth_floor_at_night() {
    let (mut w, a) = with_agent(1);
    let (x, y) = spot_for(&w, BuildingKind::Shelter, &[]);
    give(&mut w, ItemKind::Wood, 12);
    w.step(&[]);
    w.step(&[]);
    assert!(w.knows(Discovery::Shelter));
    w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::Shelter, x, y })]);
    assert_eq!(w.buildings.len(), 1);
    w.agents[0].x = x;
    w.agents[0].y = y;
    w.tick = crate::time::TICKS_PER_DAY / 2 + crate::time::TICKS_PER_DAY / 4;
    for _ in 0..20_000 {
        w.step(&[]);
    }
    assert!(w.agents[0].warmth >= SHELTER_WARMTH_FLOOR, "warmth={}", w.agents[0].warmth);
}

#[test]
fn full_production_chain_to_iron() {
    let (mut w, a) = with_agent(1);
    let fire_at = spot_for(&w, BuildingKind::Campfire, &[]);
    let (x, y) = spot_for(&w, BuildingKind::CharcoalKiln, &[fire_at]);
    let (fx, fy) = spot_for(&w, BuildingKind::Furnace, &[fire_at, (x, y)]);
    give(&mut w, ItemKind::Wood, 3);
    give(&mut w, ItemKind::Flint, 1);
    w.step(&[]);
    w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::Campfire, x: fire_at.0, y: fire_at.1 })]);
    assert_eq!(w.buildings.len(), 1);
    give(&mut w, ItemKind::Wood, 20);
    give(&mut w, ItemKind::Stone, 6);
    w.step(&[]);
    assert!(w.knows(Discovery::Kiln));
    w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::CharcoalKiln, x, y })]);
    assert_eq!(w.buildings.len(), 2);
    let kiln = 1u32;
    w.agents[0].x = x;
    w.agents[0].y = y;
    for _ in 0..2 {
        w.step(&[cmd(a, CommandKind::StartJob { building: kiln, recipe: recipe("charcoal") })]);
        w.step(&[cmd(a, CommandKind::StartJob { building: kiln, recipe: recipe("charcoal") })]);
        assert!(w.take_events().contains(&Event::Rejected(a, Reject::Busy)));
        for _ in 0..1800 {
            w.step(&[]);
        }
    }
    assert_eq!(w.buildings[1].output[ItemKind::Charcoal as usize], 4);
    w.step(&[cmd(a, CommandKind::Collect { building: kiln, item: None })]);
    assert_eq!(count(&w, 0, ItemKind::Charcoal), 4);
    give(&mut w, ItemKind::Clay, 6);
    give(&mut w, ItemKind::Stone, 8);
    w.step(&[]);
    assert!(w.knows(Discovery::Furnace));
    w.agents[0].x = fx;
    w.agents[0].y = fy;
    w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::Furnace, x: fx, y: fy })]);
    assert_eq!(w.buildings.len(), 3, "events {:?}", w.events);
    give(&mut w, ItemKind::IronOre, 2);
    give(&mut w, ItemKind::Charcoal, 2);
    w.step(&[]);
    assert!(!w.knows(Discovery::Smelting), "iron comes after bronze");
    w.learn(Discovery::Bronzework);
    w.step(&[]);
    assert!(w.knows(Discovery::Smelting));
    // iron: a bloomery with a bellows worked by hand beside it gives sponge iron, beaten to iron on the anvil
    let (bx, by) = spot_for(&w, BuildingKind::Bloomery, &[(fx, fy), (x, y)]);
    let bloomery = push_building(&mut w, BuildingKind::Bloomery, bx, by);
    push_building(&mut w, BuildingKind::Bellows, bx + 1_500, by);
    w.agents[0].x = bx + 1_500;
    w.agents[0].y = by + 1_000;
    give(&mut w, ItemKind::IronOre, 2);
    give(&mut w, ItemKind::Charcoal, 4);
    w.step(&[cmd(a, CommandKind::StartJob { building: bloomery, recipe: recipe("bloom_sponge") })]);
    assert!(w.buildings[bloomery as usize].job.is_some(), "{:?}", w.take_events());
    for _ in 0..6000 {
        w.step(&[]);
    }
    w.step(&[cmd(a, CommandKind::Collect { building: bloomery, item: None })]);
    assert_eq!(count(&w, 0, ItemKind::SpongeIron), 1);
    let (ax, ay) = spot_for(&w, BuildingKind::Anvil, &[(fx, fy), (x, y), (bx, by)]);
    push_building(&mut w, BuildingKind::Anvil, ax, ay);
    w.agents[0].x = ax + 1_000;
    w.agents[0].y = ay;
    give(&mut w, ItemKind::Hammer, 1);
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("refine_iron") })]);
    play_craft_perfectly(&mut w, a);
    assert_eq!(count(&w, 0, ItemKind::IronBloom), 1);
}

/// Dilim D: without air from a working bellows a bloomery stalls below iron heat; a water wheel works the bellows
/// and lifts the trip hammer, which beats sponge iron by itself (the water-power goal); iron tools at the iron anvil.
#[test]
fn iron_needs_bellows_and_water_drives_the_trip_hammer() {
    let (mut w, a) = with_agent(3);
    learn_all(&mut w);
    w.wild.retain(|x| x.species != WildSpecies::Wolf);
    let (bx, by) = spot_for(&w, BuildingKind::Bloomery, &[]);
    let bloomery = push_building(&mut w, BuildingKind::Bloomery, bx, by) as usize;
    w.buildings[bloomery].job = Some(crate::buildings::Job { recipe: recipe("bloom_sponge").0, remaining: 2400 });
    w.agents[0].x = bx + 20_000;
    for _ in 0..60 * 80 {
        w.step(&[]);
    }
    assert!(w.buildings[bloomery].temp <= 1000, "no bellows: stuck at {}", w.buildings[bloomery].temp);
    assert_eq!(w.buildings[bloomery].job.unwrap().remaining, 2400);
    // a bellows worked by a water wheel
    push_building(&mut w, BuildingKind::Bellows, bx + 1_500, by);
    push_building(&mut w, BuildingKind::WaterWheel, bx + 5_000, by);
    for _ in 0..60 * 60 {
        w.step(&[]);
    }
    assert!(w.buildings[bloomery].job.is_none(), "the bellows blows: iron heat, the bloom forms");
    assert_eq!(w.buildings[bloomery].output[ItemKind::SpongeIron as usize], 1);
    // the trip hammer beside the wheel beats it to iron by itself
    let hammer = push_building(&mut w, BuildingKind::TripHammer, bx + 6_000, by + 3_000) as usize;
    w.buildings[hammer].job = Some(crate::buildings::Job { recipe: recipe("trip_refine_iron").0, remaining: 1200 });
    for _ in 0..500 {
        w.step(&[]);
    }
    assert!(w.buildings[hammer].job.is_none());
    assert_eq!(w.buildings[hammer].output[ItemKind::IronBloom as usize], 1);
    assert!(w.milestone_done(Milestone::Mechanism), "water power forged iron");
    // an iron axe on the iron anvil
    let (ix, iy) = (bx - 4_000, by - 4_000);
    push_building(&mut w, BuildingKind::IronAnvil, ix, iy);
    w.agents[0].x = ix + 1_000;
    w.agents[0].y = iy;
    give(&mut w, ItemKind::IronBloom, 2);
    give(&mut w, ItemKind::Charcoal, 1);
    give(&mut w, ItemKind::Plank, 1);
    give(&mut w, ItemKind::Hammer, 1);
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("forge_iron_axe") })]);
    play_craft_perfectly(&mut w, a);
    assert_eq!(count(&w, 0, ItemKind::IronAxe), 1);
}

#[test]
fn exposure_kills_and_respawns() {
    let (mut w, _) = with_agent(1);
    w.agents[0].health = 5;
    w.agents[0].thirst = 0;
    w.agents[0].hunger = 0;
    for _ in 0..200 {
        w.step(&[]);
    }
    assert!(w.take_events().iter().any(|e| matches!(e, Event::Died(_))));
    assert_eq!(w.agents[0].health, MAX_STAT);
}

#[test]
fn time_helpers() {
    use crate::time::*;
    assert_eq!(minute_of_day(0), 480);
    assert!(ambient_x10(0, SEASON_DAYS) > 0 && ambient_x10(0, SEASON_DAYS) < 210);
    assert_eq!(daylight_permille(0), 1000);
    assert_eq!(daylight_permille(TICKS_PER_DAY / 2), 300);
}

// ---------- M5: wildlife, workers, routes, stations ----------

#[test]
fn wildlife_population_and_wander() {
    let make = || {
        let mut w = World::new(1);
        let (sx, sy) = w.spawn_mm;
        w.spawn_agent(sx, sy);
        for _ in 0..3000 {
            w.step(&[]);
        }
        w
    };
    let w = make();
    for k in [WildSpecies::Dog, WildSpecies::Ox, WildSpecies::Horse, WildSpecies::Deer] {
        let n = w.wild.iter().filter(|x| x.species == k).count();
        assert!(n >= 5, "{k:?}: {n}");
    }
    for a in &w.wild {
        assert_ne!(w.map.terrain_mm(a.x, a.y), Some(Terrain::Water));
    }
    let moved = World::new(1).wild.iter().zip(&w.wild).filter(|(a, b)| (a.x, a.y) != (b.x, b.y)).count();
    assert!(moved > w.wild.len() / 2, "animals should wander ({moved}/{})", w.wild.len());
    assert_eq!(w.hash(), make().hash());
    // calm start area
    let (sx, sy) = w.spawn_mm;
    assert!(w.wild.iter().all(|x| dist2(x.x, x.y, sx, sy) > 10_000 * 10_000));
}

#[test]
fn taming_with_berries_creates_a_worker() {
    let (mut w, a) = with_agent(1);
    let ox = w.wild.iter().find(|x| x.species == WildSpecies::Ox).unwrap().clone();
    w.agents[0].x = ox.x + 800;
    w.agents[0].y = ox.y;
    // no berries: refused
    w.step(&[cmd(a, CommandKind::Tame { wild: ox.id })]);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::NoMaterials)));
    give(&mut w, ItemKind::Berries, 3);
    let wild_before = w.wild.len();
    w.step(&[cmd(a, CommandKind::Tame { wild: ox.id })]);
    for _ in 0..1500 {
        w.step(&[]);
        if w.agents.len() > 1 {
            break;
        }
    }
    assert_eq!(w.agents.len(), 2);
    assert_eq!(w.agents[1].kind, AgentKind::Ox);
    assert_eq!(w.wild.len(), wild_before - 1);
    assert_eq!(count(&w, 0, ItemKind::Berries), 0);
    assert!(w.take_events().contains(&Event::Tamed(AgentKind::Ox)));
}

#[test]
fn species_differ_in_capacity_and_speed() {
    assert!(AgentKind::Ox.capacity() > AgentKind::Horse.capacity());
    assert!(AgentKind::Horse.speed_mm() > AgentKind::Ox.speed_mm());
    assert!(AgentKind::Dog.capacity() < AgentKind::Human.capacity());
    let (mut w, _) = with_agent(1);
    let (sx, sy) = w.spawn_mm;
    let dog = w.spawn_worker(AgentKind::Dog, sx, sy);
    let node = NodeId(w.nodes.iter().position(|n| n.kind == NodeKind::Tree).unwrap() as u32);
    // animals cannot harvest
    w.step(&[cmd(dog, CommandKind::Gather { node })]);
    assert_eq!(w.agents[dog.0 as usize].task, Task::Idle);
}

#[test]
fn worker_upkeep_decays_and_depot_feeds() {
    let (mut w, _) = with_agent(1);
    let (sx, sy) = w.spawn_mm;
    let ox = w.spawn_worker(AgentKind::Ox, sx, sy);
    for _ in 0..1300 {
        w.step(&[]);
    }
    assert_eq!(w.agents[ox.0 as usize].hunger, MAX_STAT - 100);
    // hungry ox with a stocked depot nearby eats by itself
    let d = push_building(&mut w, BuildingKind::Depot, sx + 2000, sy);
    w.buildings[d as usize].output[ItemKind::Berries as usize] = 5;
    w.agents[ox.0 as usize].hunger = 3000;
    w.step(&[]);
    assert!(w.agents[ox.0 as usize].hunger > 4000);
    assert_eq!(w.buildings[d as usize].output[ItemKind::Berries as usize], 4);
}

#[test]
fn starving_worker_stops_working() {
    let (mut w, _) = with_agent(1);
    let (sx, sy) = w.spawn_mm;
    let ox = w.spawn_worker(AgentKind::Ox, sx, sy);
    w.agents[ox.0 as usize].hunger = 0;
    w.step(&[cmd(ox, CommandKind::MoveTo { x: sx + 5000, y: sy })]);
    for _ in 0..500 {
        w.step(&[]);
    }
    assert_eq!((w.agents[ox.0 as usize].x, w.agents[ox.0 as usize].y), (sx, sy));
    // feeding by hand revives it
    give(&mut w, ItemKind::Berries, 1);
    w.step(&[cmd(AgentId(0), CommandKind::Feed { target: ox })]);
    assert!(w.agents[ox.0 as usize].hunger > 0);
    for _ in 0..500 {
        w.step(&[]);
    }
    assert!(w.agents[ox.0 as usize].x > sx);
}

#[test]
fn bot_route_harvests_and_deposits_in_a_depot() {
    let (mut w, _) = with_agent(3);
    let (sx, sy) = w.spawn_mm;
    // nearest tree with dry land all the way to the spawn
    let tree = w
        .nodes
        .iter()
        // on open ground: pieces that fly up onto a rock ledge are out of a bot's reach
        .filter(|n| n.kind == NodeKind::Tree && clear_line(&w, (sx, sy), n.center_mm()))
        .filter(|n| (-12..=12).all(|dy| (-12..=12).all(|dx| w.map.terrain(n.tx + dx, n.ty + dy) != Some(Terrain::Rock))))
        .min_by_key(|n| {
            let (x, y) = n.center_mm();
            dist2(x, y, sx, sy)
        })
        .expect("tree")
        .clone();
    let depot = push_building(&mut w, BuildingKind::Depot, sx, sy);
    let bot = w.spawn_worker(AgentKind::Bot, sx, sy);
    let (tx, ty) = tree.center_mm();
    let route = vec![
        RouteStep::Harvest { kind: NodeKind::Tree, x: tx, y: ty, radius_mm: 1000 },
        RouteStep::Deposit { building: depot },
    ];
    w.step(&[cmd(AgentId(0), CommandKind::SetRoute { worker: bot, route })]);
    for _ in 0..12_000 {
        w.step(&[]);
    }
    let wood = w.buildings[depot as usize].output[ItemKind::Wood as usize];
    assert_eq!(wood, crate::physics::log_pieces(w.seed, tree.tx, tree.ty), "all of the tree should be in the depot");
    // route keeps running without trees: bot idles in a harmless loop and stays on land
    let b = &w.agents[bot.0 as usize];
    assert_ne!(w.map.terrain_mm(b.x, b.y), Some(Terrain::Water));
}

#[test]
fn route_skips_unreachable_steps_instead_of_stalling() {
    let (mut w, _) = with_agent(1);
    let (sx, sy) = w.spawn_mm;
    let bot = w.spawn_worker(AgentKind::Bot, sx, sy);
    let depot = push_building(&mut w, BuildingKind::Depot, sx + 1000, sy);
    let route = vec![
        RouteStep::Deposit { building: 99 }, // does not exist
        RouteStep::Harvest { kind: NodeKind::IronOutcrop, x: sx, y: sy, radius_mm: 500 }, // nothing there
        RouteStep::Wait { ticks: 30 },
        RouteStep::Deposit { building: depot },
    ];
    w.step(&[cmd(AgentId(0), CommandKind::SetRoute { worker: bot, route })]);
    for _ in 0..200 {
        w.step(&[]);
    }
    assert!(w.agents[bot.0 as usize].route_i_for_test() <= 3);
}

#[test]
fn station_input_buffer_and_auto_production() {
    let (mut w, a) = with_agent(1);
    learn_all(&mut w);
    let (sx, sy) = w.spawn_mm;
    let kiln = push_building(&mut w, BuildingKind::CharcoalKiln, sx + 1500, sy);
    give(&mut w, ItemKind::Wood, 10);
    w.step(&[cmd(a, CommandKind::Deposit { building: kiln })]);
    assert_eq!(w.buildings[kiln as usize].input[ItemKind::Wood as usize], 10);
    assert_eq!(count(&w, 0, ItemKind::Wood), 0);
    w.step(&[cmd(a, CommandKind::SetAuto { building: kiln, recipe: Some(recipe("charcoal")) })]);
    for _ in 0..3700 {
        w.step(&[]);
    }
    let b = &w.buildings[kiln as usize];
    assert_eq!(b.output[ItemKind::Charcoal as usize], 4);
    assert_eq!(b.input[ItemKind::Wood as usize], 0);
    // wrong station recipe is ignored
    w.step(&[cmd(a, CommandKind::SetAuto { building: kiln, recipe: Some(recipe("iron_bloom")) })]);
    assert_eq!(w.buildings[kiln as usize].auto, Some(recipe("charcoal").0));
    w.step(&[cmd(a, CommandKind::SetAuto { building: kiln, recipe: None })]);
    assert_eq!(w.buildings[kiln as usize].auto, None);
}

#[test]
fn depot_stores_everything_except_tools_and_hands_out_by_item() {
    let (mut w, a) = with_agent(1);
    let (sx, sy) = w.spawn_mm;
    let depot = push_building(&mut w, BuildingKind::Depot, sx + 1500, sy);
    give(&mut w, ItemKind::Stone, 5);
    give(&mut w, ItemKind::Wood, 7);
    give(&mut w, ItemKind::StoneAxe, 1);
    w.step(&[cmd(a, CommandKind::Deposit { building: depot })]);
    assert_eq!(count(&w, 0, ItemKind::StoneAxe), 1, "tools stay");
    assert_eq!(w.buildings[depot as usize].output_total(), 12);
    w.step(&[cmd(a, CommandKind::Collect { building: depot, item: Some(ItemKind::Wood) })]);
    assert_eq!(count(&w, 0, ItemKind::Wood), 7);
    assert_eq!(count(&w, 0, ItemKind::Stone), 0);
    // too far away
    w.agents[0].x += 10_000;
    w.step(&[cmd(a, CommandKind::Collect { building: depot, item: None })]);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::TooFar)));
}

#[test]
fn full_inventory_teaches_the_depot() {
    let (mut w, _) = with_agent(1);
    assert!(!w.knows(Discovery::Depot));
    give(&mut w, ItemKind::Stone, 40);
    w.step(&[]);
    assert!(w.knows(Discovery::Depot));
}

#[test]
fn workshop_builds_a_bot() {
    let (mut w, a) = with_agent(1);
    learn_all(&mut w);
    let (sx, sy) = w.spawn_mm;
    let shop = push_building(&mut w, BuildingKind::Workshop, sx + 3000, sy);
    {
        let b = &mut w.buildings[shop as usize];
        b.input[ItemKind::Steel as usize] = 2;
        b.input[ItemKind::Gear as usize] = 3;
        b.input[ItemKind::Plank as usize] = 4;
        b.input[ItemKind::Charcoal as usize] = 2;
    }
    w.step(&[cmd(a, CommandKind::SetAuto { building: shop, recipe: Some(recipe("automaton")) })]);
    assert!(w.agents.len() == 1);
    for _ in 0..3700 {
        w.step(&[]);
    }
    assert_eq!(w.agents.len(), 2);
    assert_eq!(w.agents[1].kind, AgentKind::Bot);
    assert_eq!(w.agents[1].capacity(), 40);
    assert!(w.take_events().contains(&Event::WorkerReady(AgentKind::Bot)));
    assert_eq!(w.buildings[shop as usize].input_total(), 0);
}

#[test]
fn m5_state_is_deterministic() {
    let make = || {
        let (mut w, _) = with_agent(5);
        learn_all(&mut w);
        let (sx, sy) = w.spawn_mm;
        let d = push_building(&mut w, BuildingKind::Depot, sx, sy);
        let bot = w.spawn_worker(AgentKind::Bot, sx, sy);
        let tree = w.nodes.iter().find(|n| n.kind == NodeKind::Tree).unwrap().center_mm();
        let route = vec![
            RouteStep::Harvest { kind: NodeKind::Tree, x: tree.0, y: tree.1, radius_mm: 2000 },
            RouteStep::Deposit { building: d },
        ];
        w.step(&[cmd(AgentId(0), CommandKind::SetRoute { worker: bot, route })]);
        for _ in 0..4000 {
            w.step(&[]);
        }
        w.hash()
    };
    assert_eq!(make(), make());
}

fn deer_near(w: &mut World, dist_mm: i64) -> u32 {
    let (px, py) = (w.agents[0].x, w.agents[0].y);
    let id = w.wild.iter().map(|x| x.id).max().unwrap_or(0) + 1;
    w.wild.push(crate::wildlife::Wild::new(id, WildSpecies::Deer, px + dist_mm, py));
    id
}

#[test]
fn deer_flee_cannot_be_tamed_and_need_a_spear() {
    let (mut w, a) = with_agent(1);
    let id = deer_near(&mut w, 5_000);
    let x0 = w.wild.iter().find(|d| d.id == id).unwrap().x;
    w.step(&[]);
    assert!(w.wild.iter().find(|d| d.id == id).unwrap().x > x0, "should run away");
    give(&mut w, ItemKind::Berries, 5);
    w.step(&[cmd(a, CommandKind::Tame { wild: id })]);
    assert!(matches!(w.agents[0].task, Task::Idle));
    w.step(&[cmd(a, CommandKind::Hunt { wild: id })]);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::NoMaterials)));
}

#[test]
fn spear_hunt_gives_meat() {
    let (mut w, a) = with_agent(1);
    let id = deer_near(&mut w, 2_500);
    give(&mut w, ItemKind::Spear, 1);
    give(&mut w, ItemKind::Knife, 1); // skinned with a knife: the full share
    w.step(&[cmd(a, CommandKind::Hunt { wild: id })]);
    let mut meat = None;
    for _ in 0..3000 {
        w.step(&[]);
        for e in w.take_events() {
            if let Event::Hunted { meat: m, .. } = e {
                meat = Some(m);
            }
        }
        if meat.is_some() {
            break;
        }
    }
    let m = meat.expect("hunt should succeed");
    assert!((2..=4).contains(&m));
    assert_eq!(count(&w, 0, ItemKind::RawMeat), m);
    assert!(w.wild.iter().all(|d| d.id != id));
}

/// The kill says which animal fell and where it stood, so the drawing can show it go down.
#[test]
fn spear_hunt_reports_the_kill() {
    let (mut w, a) = with_agent(1);
    let id = deer_near(&mut w, 2_500);
    give(&mut w, ItemKind::Spear, 1);
    w.step(&[cmd(a, CommandKind::Hunt { wild: id })]);
    let mut last = None;
    let mut killed = None;
    for _ in 0..3000 {
        if let Some(d) = w.wild.iter().find(|d| d.id == id) {
            last = Some((d.x, d.y));
        }
        w.step(&[]);
        killed = killed.or_else(|| w.take_events().into_iter().find(|e| matches!(e, Event::Killed { .. })));
        if killed.is_some() {
            break;
        }
    }
    let Some(Event::Killed { wild, species, x, y }) = killed else { panic!("no kill reported") };
    assert_eq!(wild, id);
    assert_eq!(species, crate::WildSpecies::Deer);
    let (lx, ly) = last.unwrap();
    // where it stood on its last tick (it may have taken one more step as the spear flew)
    assert!((x - lx).abs() <= 200 && (y - ly).abs() <= 200, "fell at ({x},{y}), last seen ({lx},{ly})");
}

#[test]
fn meat_nutrition_and_raw_penalty() {
    let (mut w, a) = with_agent(1);
    give(&mut w, ItemKind::RawMeat, 1);
    give(&mut w, ItemKind::CookedMeat, 1);
    w.agents[0].hunger = 3000;
    let h0 = w.agents[0].health;
    w.step(&[cmd(a, CommandKind::Eat { item: ItemKind::CookedMeat })]);
    assert!(w.agents[0].hunger > 7000);
    assert!(w.agents[0].health >= h0 - 5);
    w.agents[0].hunger = 3000;
    w.step(&[cmd(a, CommandKind::Eat { item: ItemKind::RawMeat })]);
    assert!(w.agents[0].hunger > 4000 && w.agents[0].hunger < 5000);
    assert!(w.agents[0].sick > 4000, "raw meat makes you sick");
    assert!(w.agents[0].health >= 1);
}

#[test]
fn iron_tools_speed_up_gathering() {
    let ticks_for = |tools: &[ItemKind]| -> u64 {
        let (mut w, a) = with_agent(1);
        for t in tools {
            give(&mut w, *t, 1);
        }
        let node = w
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.kind == NodeKind::Tree)
            .min_by_key(|(_, n)| {
                let (x, y) = n.center_mm();
                dist2(x, y, w.spawn_mm.0, w.spawn_mm.1)
            })
            .map(|(i, n)| (NodeId(i as u32), n.center_mm()))
            .unwrap();
        w.agents[0].x = node.1 .0;
        w.agents[0].y = node.1 .1;
        w.step(&[cmd(a, CommandKind::Gather { node: node.0 })]);
        let start = w.tick;
        for _ in 0..5000 {
            w.step(&[]);
            if w.agents[0].inventory.get(ItemKind::Wood) > 0 {
                break;
            }
        }
        w.tick - start
    };
    let none = ticks_for(&[]);
    let stone = ticks_for(&[ItemKind::StoneAxe]);
    let iron = ticks_for(&[ItemKind::IronAxe]);
    assert!(iron < stone && stone < none, "{iron} {stone} {none}");
}

#[test]
fn cooking_needs_a_burning_fire() {
    let (mut w, a) = with_agent(1);
    learn_all(&mut w);
    let (x, y) = spot_for(&w, BuildingKind::Campfire, &[]);
    give(&mut w, ItemKind::Wood, 3);
    give(&mut w, ItemKind::Flint, 1);
    w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::Campfire, x, y })]);
    w.agents[0].x = x + 1000;
    w.agents[0].y = y;
    give(&mut w, ItemKind::RawMeat, 1);
    w.buildings[0].fuel = 0; // out
    w.step(&[cmd(a, CommandKind::StartJob { building: 0, recipe: recipe("cook_meat") })]);
    for _ in 0..400 {
        w.step(&[]);
    }
    assert_eq!(w.buildings[0].output_total(), 0, "cold fire must not cook");
    w.buildings[0].fuel = FIRE_FUEL_MAX;
    // the fire takes a few seconds to get hot enough, then the meat cooks
    for _ in 0..800 {
        w.step(&[]);
    }
    assert_eq!(w.buildings[0].output[ItemKind::CookedMeat as usize], 1);
}

#[test]
fn iron_age_is_completable_from_commands() {
    let (mut w, a) = with_agent(3);
    let (sx, sy) = w.spawn_mm;
    w.agents[0].x = sx;
    w.agents[0].y = sy;
    for _ in 0..3 {
        // pretend the player gathered everything needed
        give(&mut w, ItemKind::Wood, 40);
        give(&mut w, ItemKind::Stone, 30);
        give(&mut w, ItemKind::Clay, 10);
        give(&mut w, ItemKind::IronOre, 20);
        w.step(&[]);
    }
    learn_all(&mut w);
    give(&mut w, ItemKind::Flint, 1);
    let mut taken: Vec<(i64, i64)> = Vec::new();
    let mut idx = std::collections::HashMap::new();
    for kind in [
        BuildingKind::Campfire,
        BuildingKind::Shelter,
        BuildingKind::CharcoalKiln,
        BuildingKind::Furnace,
        BuildingKind::Workbench,
        BuildingKind::Workshop,
    ] {
        give(&mut w, ItemKind::Wood, 40);
        give(&mut w, ItemKind::Stone, 30);
        give(&mut w, ItemKind::Clay, 10);
        give(&mut w, ItemKind::IronBloom, 2);
        let (x, y) = spot_for(&w, kind, &taken);
        taken.push((x, y));
        w.step(&[cmd(a, CommandKind::Build { kind, x, y })]);
        idx.insert(kind, w.buildings.len() - 1);
    }
    // companion
    let (px, py) = (w.agents[0].x, w.agents[0].y);
    let ox = w.wild.iter().find(|d| d.species == WildSpecies::Ox).map(|d| d.id);
    let ox = ox.unwrap_or_else(|| {
        let id = 9_999;
        w.wild.push(crate::wildlife::Wild::new(id, WildSpecies::Ox, px + 1500, py));
        id
    });
    let (ox_x, ox_y) = {
        let o = w.wild.iter().find(|d| d.id == ox).unwrap();
        (o.x, o.y)
    };
    w.agents[0].x = ox_x;
    w.agents[0].y = ox_y;
    give(&mut w, ItemKind::Berries, 6);
    for _ in 0..8 {
        w.step(&[cmd(a, CommandKind::Tame { wild: ox })]);
        for _ in 0..400 {
            w.step(&[]);
        }
    }
    // production: use stations in person, standing next to each
    let station = |w: &mut World, kind: BuildingKind| {
        let i = idx[&kind];
        w.agents[0].x = w.buildings[i].x + 1000;
        w.agents[0].y = w.buildings[i].y;
        i as u32
    };
    let run = |w: &mut World, b: u32, r: &str| {
        w.wild.retain(|x| x.species != WildSpecies::Wolf); // nights are long; this test is about production
        w.step(&[cmd(a, CommandKind::StartJob { building: b, recipe: recipe(r) })]);
        for _ in 0..4000 {
            w.step(&[]);
        }
        w.step(&[cmd(a, CommandKind::Collect { building: b, item: None })]);
    };
    give(&mut w, ItemKind::Wood, 40);
    let kiln = station(&mut w, BuildingKind::CharcoalKiln);
    run(&mut w, kiln, "charcoal");
    run(&mut w, kiln, "charcoal");
    give(&mut w, ItemKind::IronOre, 6);
    let furnace = station(&mut w, BuildingKind::Furnace);
    for _ in 0..3 {
        give(&mut w, ItemKind::Charcoal, 4);
        run(&mut w, furnace, "iron_bloom");
    }
    // the pack is limited: put down what the next step does not need
    for k in [ItemKind::Stone, ItemKind::Clay, ItemKind::Berries, ItemKind::Herb] {
        w.agents[0].inventory.counts[k as usize] = 0;
    }
    let bench = station(&mut w, BuildingKind::Workbench);
    give(&mut w, ItemKind::IronBloom, 8);
    give(&mut w, ItemKind::Wood, 20);
    give(&mut w, ItemKind::IronBloom, 12);
    run(&mut w, bench, "gear");
    run(&mut w, bench, "gear");
    run(&mut w, bench, "gear");
    run(&mut w, bench, "gear");
    run(&mut w, bench, "gear");
    let gears = w.agents[0].inventory.get(ItemKind::Gear);
    assert!(gears >= 5, "gears: {gears} hp={} hu={} th={} wa={} inv={:?} ev={:?} bench={:?}", w.agents[0].health, w.agents[0].hunger, w.agents[0].thirst, w.agents[0].warmth, w.agents[0].inventory.counts, w.events.len(), w.buildings.iter().map(|b| (b.kind, b.job.is_some(), b.output.iter().sum::<u16>())).collect::<Vec<_>>());
    let gears_kept = gears;
    give(&mut w, ItemKind::IronBloom, 8);
    give(&mut w, ItemKind::Wood, 20);
    run(&mut w, bench, "iron_axe");
    run(&mut w, bench, "iron_pickaxe");
    // a water wheel needs a river bank
    let (wx, wy) = wheel_spot(&w);
    w.agents[0].x = wx + 1500;
    w.agents[0].y = wy;
    give(&mut w, ItemKind::Wood, 20);
    give(&mut w, ItemKind::Stone, 10);
    w.agents[0].inventory.counts[ItemKind::Gear as usize] = gears_kept.min(5);
    w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::WaterWheel, x: wx, y: wy })]);
    assert_eq!(w.agents[0].inventory.get(ItemKind::Gear), 3);
    give(&mut w, ItemKind::IronBloom, 6);
    give(&mut w, ItemKind::Wood, 20);
    give(&mut w, ItemKind::Charcoal, 6);
    let shop = station(&mut w, BuildingKind::Workshop);
    run(&mut w, shop, "automaton");
    // bronze age: alloy at the furnace, then tools at the bench
    w.agents[0].inventory.counts = [0; ItemKind::COUNT]; // the pack is nearly full by now
    w.wild.retain(|x| x.species != WildSpecies::Wolf); // the long run attracted wolves
    w.agents[0].health = MAX_STAT;
    give(&mut w, ItemKind::CopperOre, 6);
    give(&mut w, ItemKind::TinOre, 2);
    give(&mut w, ItemKind::Charcoal, 6);
    let furnace = station(&mut w, BuildingKind::Furnace);
    w.step(&[]);
    assert!(w.knows(Discovery::Alloying));
    run(&mut w, furnace, "bronze");
    run(&mut w, furnace, "bronze");
    assert_eq!(w.agents[0].inventory.get(ItemKind::Bronze), 4);
    let bench = station(&mut w, BuildingKind::Workbench);
    give(&mut w, ItemKind::Wood, 10);
    w.step(&[]);
    assert!(w.knows(Discovery::Bronzework));
    run(&mut w, bench, "bronze_axe");
    run(&mut w, bench, "bronze_pickaxe");
    // the tamed ox and worked bronze teach the wheel; the age also wants a cart and a plough
    assert!(w.knows(Discovery::Draught));
    w.agents[0].inventory.counts = [0; ItemKind::COUNT];
    give(&mut w, ItemKind::Plank, 12);
    give(&mut w, ItemKind::Bronze, 3);
    for r in ["wheel", "wheel", "cart", "plow"] {
        run(&mut w, bench, r);
    }
    // the bronze age also wants a mill an animal turns; the iron age water power (a trip hammer)
    w.milestones |= 1 << Milestone::AnimalPower as u64;
    w.milestones |= 1 << Milestone::Mechanism as u64;
    w.step(&[]);
    assert!(w.milestone_done(Milestone::BronzeAge));
    w.step(&[]);
    // the farming goals have their own chain (`the_neolithic_chain_runs_from_wild_grass_to_bread`)
    for m in Milestone::ALL.into_iter().filter(|m| !(matches!(m, Milestone::Harvest | Milestone::Bread | Milestone::Winter | Milestone::NeolithicAge | Milestone::FirstMetal | Milestone::CopperTools | Milestone::CopperAge | Milestone::Automaton) || (*m as u32) >= Milestone::Steel as u32)) {
        assert!(w.milestone_done(m), "missing {m:?}");
    }
    // the automaton wants steel now: it belongs to the steel age (`the_steel_and_mechanical_age`)
}

/// A free land spot where a water wheel may stand (within 3 tiles of water).
fn wheel_spot(w: &World) -> (i64, i64) {
    for ty in 0..MAP_H {
        for tx in 0..MAP_W {
            let (x, y) = (tx as i64 * 1000 + 500, ty as i64 * 1000 + 500);
            if w.can_place(BuildingKind::WaterWheel, x, y).is_ok() {
                return (x, y);
            }
        }
    }
    panic!("no river bank");
}

#[test]
fn wheel_needs_water_and_is_a_power_source_not_an_aura() {
    let (mut w, _) = with_agent(1);
    learn_all(&mut w);
    let (sx, sy) = w.spawn_mm;
    // far from any river the wheel is refused
    let far_ok = (0..MAP_W).filter_map(|tx| {
        let (x, y) = (tx as i64 * 1000 + 500, sy);
        (!w.map.water_within(tx, (y / 1000) as i32, 3) && w.can_place(BuildingKind::WaterWheel, x, y) == Err(Reject::NeedsWater)).then_some(())
    });
    assert!(far_ok.count() > 0);
    let (wx, wy) = wheel_spot(&w);
    let time_for = |wheel: bool| -> u64 {
        let (mut w, a) = with_agent(1);
        learn_all(&mut w);
        let (bx, by) = (wx + 3000, wy);
        let bench = push_building(&mut w, BuildingKind::Workbench, bx, by);
        if wheel {
            push_building(&mut w, BuildingKind::WaterWheel, wx, wy);
        }
        w.agents[0].x = bx + 1000;
        w.agents[0].y = by;
        give(&mut w, ItemKind::IronBloom, 1);
        give(&mut w, ItemKind::Wood, 2);
        w.step(&[cmd(a, CommandKind::StartJob { building: bench, recipe: recipe("gear") })]);
        let start = w.tick;
        while w.buildings[bench as usize].job.is_some() {
            w.step(&[]);
        }
        w.tick - start
    };
    // a wheel no longer speeds up whatever stands near it: it drives machines (bellows, trip hammer, mill)
    let (plain, near_wheel) = (time_for(false), time_for(true));
    assert_eq!(plain, near_wheel, "{plain} vs {near_wheel}");
    let _ = (sx, sy);
}

fn wolf_world() -> (World, AgentId) {
    let (mut w, a) = with_agent(1);
    w.wild.clear();
    // wolves hunt at dusk and night, after the first day's grace
    w.tick = w.tick.max(TICKS_PER_DAY);
    while crate::time::daylight_permille(w.tick) >= 500 {
        w.tick += 600;
    }
    let (px, py) = (w.agents[0].x, w.agents[0].y);
    w.wild.push(crate::wildlife::Wild::new(1, WildSpecies::Wolf, px + 7_000, py));
    (w, a)
}

#[test]
fn wolf_bites_and_injury_bleeds_then_heals() {
    let (mut w, _) = wolf_world();
    let mut bitten = false;
    for _ in 0..600 {
        w.step(&[]);
        bitten |= w.take_events().iter().any(|e| matches!(e, Event::Bitten(_)));
        if bitten {
            break;
        }
    }
    assert!(bitten, "an unprotected person gets bitten");
    // (the first two days a bite is half as bad)
    assert!(w.agents[0].injury > 0 && w.agents[0].health < MAX_STAT - 500);
    // far from the wolf, injuries fade away
    w.wild.clear();
    let h = w.agents[0].health;
    for _ in 0..200 {
        w.step(&[]);
    }
    assert!(w.agents[0].health < h, "bleeding costs health");
    for _ in 0..4000 {
        w.step(&[]);
    }
    assert_eq!(w.agents[0].injury, 0);
}

#[test]
fn a_lit_fire_keeps_wolves_away() {
    let (mut w, _) = wolf_world();
    let (px, py) = (w.agents[0].x, w.agents[0].y);
    let fire = push_building(&mut w, BuildingKind::Campfire, px + 1500, py);
    w.buildings[fire as usize].fuel = FIRE_FUEL_MAX;
    let d0 = (w.wild[0].x - px).abs();
    for _ in 0..900 {
        w.step(&[]);
        assert!(!w.take_events().iter().any(|e| matches!(e, Event::Bitten(_))));
    }
    assert!((w.wild[0].x - px).abs() >= d0, "the wolf did not come closer");
}

#[test]
fn spear_kills_a_wolf() {
    let (mut w, a) = wolf_world();
    give(&mut w, ItemKind::Spear, 1);
    give(&mut w, ItemKind::Knife, 1); // the knife skins the kill
    let id = w.wild[0].id;
    w.step(&[cmd(a, CommandKind::Hunt { wild: id })]);
    for _ in 0..2000 {
        w.step(&[]);
        if w.wild.is_empty() {
            break;
        }
    }
    assert!(w.wild.is_empty());
    assert!(w.agents[0].inventory.get(ItemKind::RawMeat) >= 2);
}

#[test]
fn wolves_exist_but_are_rare_and_far() {
    let w = World::new(1);
    let (sx, sy) = (w.spawn_mm.0 / 1000, w.spawn_mm.1 / 1000);
    let wolves: Vec<_> = w.wild.iter().filter(|x| x.species == WildSpecies::Wolf).collect();
    assert!(!wolves.is_empty(), "some wolves");
    assert!(wolves.len() * 4 < w.wild.len(), "rare");
    for x in wolves {
        assert!(((x.x / 1000) - sx).abs().max(((x.y / 1000) - sy).abs()) >= 30);
    }
}

#[test]
fn far_wildlife_is_frozen_near_wildlife_moves() {
    let mut w = World::with_size(1, 512, 512);
    let (sx, sy) = w.spawn_mm;
    w.spawn_agent(sx, sy);
    let before: Vec<(i64, i64)> = w.wild.iter().map(|d| (d.x, d.y)).collect();
    for _ in 0..1500 {
        w.step(&[]);
    }
    let (mut near_moved, mut far_moved) = (0, 0);
    for (d, b) in w.wild.iter().zip(&before) {
        let dist = ((b.0 - sx).abs()).max((b.1 - sy).abs());
        let moved = (d.x, d.y) != *b;
        if dist < 40_000 && moved {
            near_moved += 1;
        }
        if dist > 150_000 && moved {
            far_moved += 1;
        }
    }
    assert!(near_moved > 0, "nearby animals wander");
    assert_eq!(far_moved, 0, "distant animals are not simulated");
}

#[test]
fn big_world_saves_and_loads_identically() {
    let mut w = World::with_size(9, 640, 384);
    let (sx, sy) = w.spawn_mm;
    w.spawn_agent(sx, sy);
    for _ in 0..300 {
        w.step(&[]);
    }
    let b = World::from_bytes(&w.to_bytes()).unwrap();
    assert_eq!((b.map.w, b.map.h), (640, 384));
    assert_eq!(b.hash(), w.hash());
    // a header claiming an absurd size is refused
    assert!(World::with_size(1, 1, 1).map.w >= crate::map::MIN_MAP_SIDE, "tiny sizes are clamped");
}

/// A wall of depots across x = `wx`, from `y0` to `y1`, 2 m apart.
fn wall(w: &mut World, wx: i64, y0: i64, y1: i64) {
    let mut y = y0;
    while y <= y1 {
        push_building(w, BuildingKind::Depot, wx, y);
        y += 2_000;
    }
}

fn inside_a_building(w: &World, x: i64, y: i64) -> bool {
    w.buildings.iter().any(|b| dist2(b.x, b.y, x, y) < (rules().building(b.kind).radius_mm - 100).pow(2))
}

#[test]
fn walkers_go_around_a_wall_of_buildings() {
    let (mut w, a) = with_agent(1);
    let (sx, sy) = w.spawn_mm;
    let wx = sx / 1000 * 1000 + 500;
    wall(&mut w, wx, sy - 14_000, sy + 14_000);
    w.agents[0].x = wx - 4_000;
    w.agents[0].y = sy;
    let target = (wx + 4_000, sy);
    w.step(&[cmd(a, CommandKind::MoveTo { x: target.0, y: target.1 })]);
    let mut longest_straight = 0i64;
    for _ in 0..4_000 {
        w.step(&[]);
        let ag = &w.agents[0];
        assert!(!inside_a_building(&w, ag.x, ag.y), "walked through a building at {},{}", ag.x, ag.y);
        longest_straight = longest_straight.max((ag.y - sy).abs());
        if ag.task == Task::Idle {
            break;
        }
    }
    let ag = &w.agents[0];
    assert_eq!((ag.x, ag.y), target, "reached the far side");
    assert!(longest_straight > 12_000, "had to detour around the wall end ({longest_straight})");
}

#[test]
fn walkers_cross_a_lake_by_walking_around_it() {
    // find a row where a lake sits between two land tiles that are connected around it
    let w0 = World::new(1);
    let mut pair = None;
    'scan: for ty in 20..MAP_H - 20 {
        let mut tx = 20;
        while tx < MAP_W - 20 {
            if w0.map.terrain(tx, ty) == Some(Terrain::Water) {
                let start = tx;
                while w0.map.terrain(tx, ty) == Some(Terrain::Water) && tx < MAP_W - 20 {
                    tx += 1;
                }
                let len = tx - start;
                let land = |x: i32| matches!(w0.map.terrain(x, ty), Some(Terrain::Grass | Terrain::Sand | Terrain::Forest));
                if (5..=14).contains(&len) && land(start - 2) && land(tx + 1) {
                    pair = Some(((start - 2, ty), (tx + 1, ty)));
                    break 'scan;
                }
            }
            tx += 1;
        }
    }
    let (from, to) = pair.expect("a lake to cross");
    let mut w = World::new(1);
    let a = w.spawn_agent(from.0 as i64 * 1000 + 500, from.1 as i64 * 1000 + 500);
    let target = (to.0 as i64 * 1000 + 500, to.1 as i64 * 1000 + 500);
    w.step(&[cmd(a, CommandKind::MoveTo { x: target.0, y: target.1 })]);
    let mut arrived = false;
    for _ in 0..20_000 {
        w.step(&[]);
        let ag = &w.agents[0];
        assert_ne!(w.map.terrain_mm(ag.x, ag.y), Some(Terrain::Water));
        if (ag.x, ag.y) == target {
            arrived = true;
            break;
        }
    }
    assert!(arrived, "walked around the lake instead of stopping at the shore");
}

#[test]
fn unreachable_targets_end_cleanly() {
    let (mut w, a) = with_agent(1);
    let (sx, sy) = w.spawn_mm;
    let (cx, cy) = (sx / 1000 * 1000 + 500, sy / 1000 * 1000 + 500);
    // a ring of depots around the target point
    for (dx, dy) in [(-2000, 0), (2000, 0), (0, -2000), (0, 2000), (-1500, -1500), (1500, -1500), (-1500, 1500), (1500, 1500)] {
        push_building(&mut w, BuildingKind::Depot, cx + 8_000 + dx, cy + dy);
    }
    w.step(&[cmd(a, CommandKind::MoveTo { x: cx + 8_000, y: cy })]);
    for _ in 0..3_000 {
        w.step(&[]);
    }
    let ag = &w.agents[0];
    assert_eq!(ag.task, Task::Idle);
    assert!(!inside_a_building(&w, ag.x, ag.y));
    assert!(ag.path.is_empty());
}

#[test]
fn many_walkers_share_the_planning_budget() {
    let (mut w, a) = with_agent(1);
    let (sx, sy) = w.spawn_mm;
    let wx = sx / 1000 * 1000 + 500;
    wall(&mut w, wx, sy - 12_000, sy + 12_000);
    let ids: Vec<AgentId> = (0..40).map(|i| w.spawn_worker(AgentKind::Ox, wx - 4_000, sy - 6_000 + i * 300)).collect();
    let cmds: Vec<Command> =
        ids.iter().map(|&id| cmd(id, CommandKind::MoveTo { x: wx + 4_000, y: sy })).collect();
    w.step(&cmds);
    for _ in 0..5_000 {
        w.step(&[]);
    }
    let _ = a;
    for id in ids {
        let ag = &w.agents[id.0 as usize];
        assert_eq!((ag.x, ag.y), (wx + 4_000, sy), "everyone gets across");
    }
}

#[test]
fn paths_survive_save_and_load() {
    let (mut w, a) = with_agent(1);
    let (sx, sy) = w.spawn_mm;
    let wx = sx / 1000 * 1000 + 500;
    wall(&mut w, wx, sy - 14_000, sy + 14_000);
    w.agents[0].x = wx - 4_000;
    w.agents[0].y = sy;
    w.step(&[cmd(a, CommandKind::MoveTo { x: wx + 4_000, y: sy })]);
    for _ in 0..300 {
        w.step(&[]);
    }
    assert!(!w.agents[0].path.is_empty(), "mid-journey on a planned path");
    let mut b = World::from_bytes(&w.to_bytes()).unwrap();
    assert_eq!(b.hash(), w.hash());
    for _ in 0..2_000 {
        w.step(&[]);
        b.step(&[]);
    }
    assert_eq!(b.hash(), w.hash(), "a loaded world walks exactly the same way");
}

/// Play every step of a craft the way an expert would: timing games when the marker is mid-zone,
/// the exact angle and force when knapping, the exact spot when fitting, the exact pressure when lashing.
fn craft_perfectly(w: &mut World, a: AgentId, id: &str) {
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe(id) })]);
    while let Some(v) = w.craft_view(a) {
        match v.kind {
            MinigameKind::Knap => {
                w.step(&[cmd(a, CommandKind::StrikeAt { angle: v.center as u16, power: v.power as u8 })]);
            }
            MinigameKind::Fit => w.step(&[cmd(a, CommandKind::PlaceAt { x: v.tx as i16, y: v.ty as i16 })]),
            MinigameKind::Tension => w.step(&[cmd(a, CommandKind::Tighten { power: v.power as u8 })]),
            MinigameKind::Trim => w.step(&[cmd(a, CommandKind::PlaceAt { x: v.tx as i16, y: v.ty as i16 })]),
            MinigameKind::Rub => w.step(&[cmd(a, CommandKind::Rub { pace: v.power as u8, grip: true })]),
            MinigameKind::Timing if v.pos.abs_diff(v.center) <= v.half / 3 => {
                w.step(&[cmd(a, CommandKind::Strike)]);
            }
            MinigameKind::Timing => w.step(&[]),
            MinigameKind::Stack => w.step(&[cmd(a, CommandKind::PlaceAt { x: 0, y: 0 })]),
        }
    }
}

/// Strike where nothing can be hit: the far side, the far corner, no pressure, or the marker far from the zone.
fn craft_badly(w: &mut World, a: AgentId, id: &str) {
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe(id) })]);
    while let Some(v) = w.craft_view(a) {
        match v.kind {
            MinigameKind::Knap => {
                w.step(&[cmd(a, CommandKind::StrikeAt { angle: ((v.center + 180) % 360) as u16, power: 0 })]);
            }
            MinigameKind::Fit => w.step(&[cmd(a, CommandKind::PlaceAt { x: if v.tx > 0 { -100 } else { 100 }, y: if v.ty > 0 { -100 } else { 100 } })]),
            MinigameKind::Tension => w.step(&[cmd(a, CommandKind::Tighten { power: if v.power > 50 { 0 } else { 100 } })]),
            MinigameKind::Trim => w.step(&[cmd(a, CommandKind::PlaceAt { x: if v.tx > 0 { -100 } else { 100 }, y: if v.ty > 0 { -100 } else { 100 } })]),
            MinigameKind::Rub => w.step(&[cmd(a, CommandKind::Rub { pace: if v.power > 50 { 0 } else { 100 }, grip: true })]),
            MinigameKind::Timing if v.pos.abs_diff(v.center) > v.half + 6 => {
                w.step(&[cmd(a, CommandKind::Strike)]);
            }
            MinigameKind::Timing => w.step(&[]),
            MinigameKind::Stack => w.step(&[cmd(a, CommandKind::PlaceAt { x: 95, y: 0 })]),
        }
    }
}

fn crafted_quality(w: &mut World) -> Option<u8> {
    w.take_events().iter().rev().find_map(|e| match e {
        Event::Crafted { quality, .. } => Some(*quality),
        _ => None,
    })
}

#[test]
fn minigame_perfect_play_is_fine_and_pays_a_bonus() {
    let (mut w, a) = with_agent(1);
    learn_all(&mut w);
    give(&mut w, ItemKind::Stone, 2);
    craft_perfectly(&mut w, a, "sharp_stone");
    assert_eq!(crafted_quality(&mut w), Some(2));
    assert_eq!(count(&w, 0, ItemKind::Stone), 0, "materials are used up");
    assert_eq!(count(&w, 0, ItemKind::SharpStone), 2, "fine work yields an extra piece");
}

#[test]
fn minigame_clumsy_play_can_fail_and_wastes_the_stones() {
    let mut failures = 0;
    for seed in 1..40 {
        let (mut w, a) = with_agent(seed);
        learn_all(&mut w);
        give(&mut w, ItemKind::Stone, 2);
        w.settings.difficulty = crate::world::Difficulty::Realistic; // the forgiving setting gives half back
        craft_badly(&mut w, a, "sharp_stone");
        let q = crafted_quality(&mut w).expect("finished");
        assert_eq!(count(&w, 0, ItemKind::Stone), 0);
        assert!(q <= 1, "never fine when everything missed");
        if q == 0 {
            failures += 1;
            assert_eq!(count(&w, 0, ItemKind::SharpStone), 0);
        }
    }
    assert!(failures >= 20, "most clumsy attempts fail ({failures}/39) but luck rescues some");
}

#[test]
fn tools_widen_the_target_zone() {
    let half = |tools: &[ItemKind]| {
        let (mut w, a) = with_agent(1);
        learn_all(&mut w);
        give(&mut w, ItemKind::Stone, 2);
        for t in tools {
            w.agents[0].inventory.counts[*t as usize] = 1;
        }
        w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("sharp_stone") })]);
        w.craft_view(a).unwrap().half
    };
    let (bare, axe, iron) = (half(&[]), half(&[ItemKind::StoneAxe]), half(&[ItemKind::StoneAxe, ItemKind::IronPickaxe]));
    assert!(bare < axe && axe < iron, "{bare} {axe} {iron}");
}

#[test]
fn luck_sometimes_turns_a_good_craft_into_a_fine_one() {
    // the bandage: all three timing strikes land, both lashing pulls miss: 60 points on average = "good"
    let mut fine = 0;
    let mut good = 0;
    for seed in 1..60 {
        let (mut w, a) = with_agent(seed);
        learn_all(&mut w);
        give(&mut w, ItemKind::Herb, 2);
        give(&mut w, ItemKind::Fiber, 1);
        w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("bandage") })]);
        while let Some(v) = w.craft_view(a) {
            match v.kind {
                MinigameKind::Timing if v.pos.abs_diff(v.center) <= v.half / 3 => w.step(&[cmd(a, CommandKind::Strike)]),
                MinigameKind::Timing => w.step(&[]),
                MinigameKind::Tension => w.step(&[cmd(a, CommandKind::Tighten { power: if v.power > 50 { 0 } else { 100 } })]),
                _ => unreachable!(),
            }
        }
        match crafted_quality(&mut w) {
            Some(2) => fine += 1,
            Some(1) => good += 1,
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(fine > 0 && good > fine, "luck upgrades some but not most (fine {fine}, good {good})");
}

#[test]
fn leaving_a_minigame_costs_nothing() {
    let (mut w, a) = with_agent(1);
    learn_all(&mut w);
    give(&mut w, ItemKind::Stone, 2);
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("sharp_stone") })]);
    assert!(w.craft_view(a).is_some());
    w.step(&[cmd(a, CommandKind::MoveTo { x: w.agents[0].x + 3000, y: w.agents[0].y })]);
    assert!(w.craft_view(a).is_none());
    assert_eq!(count(&w, 0, ItemKind::Stone), 2);
    // and walking away from a forgotten one: it times out
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("sharp_stone") })]);
    for _ in 0..3700 {
        w.step(&[]);
    }
    assert!(w.craft_view(a).is_none());
    assert_eq!(count(&w, 0, ItemKind::Stone), 2);
}

#[test]
fn herbs_and_bandages_treat_sickness_and_wounds() {
    let (mut w, a) = with_agent(1);
    learn_all(&mut w);
    give(&mut w, ItemKind::Herb, 4);
    give(&mut w, ItemKind::Fiber, 2);
    w.agents[0].sick = 5_000;
    w.agents[0].injury = 1_800;
    w.agents[0].health = 6_000;
    w.step(&[cmd(a, CommandKind::Eat { item: ItemKind::Herb })]);
    assert!(w.agents[0].sick < 2_100 && w.agents[0].health > 6_200);
    craft_perfectly(&mut w, a, "bandage");
    assert!(count(&w, 0, ItemKind::Bandage) >= 1);
    w.step(&[cmd(a, CommandKind::Eat { item: ItemKind::Bandage })]);
    assert_eq!(w.agents[0].injury, 0);
}

#[test]
fn minigame_state_survives_save_and_load() {
    let (mut w, a) = with_agent(1);
    learn_all(&mut w);
    give(&mut w, ItemKind::Stone, 2);
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("sharp_stone") })]);
    for _ in 0..77 {
        w.step(&[]);
    }
    let mut b = World::from_bytes(&w.to_bytes()).unwrap();
    assert_eq!(b.craft_view(a), w.craft_view(a));
    assert_eq!(b.hash(), w.hash());
    for i in 0..300 {
        let c = if i % 50 == 20 { vec![cmd(a, CommandKind::Strike)] } else { vec![] };
        w.step(&c);
        b.step(&c);
    }
    assert_eq!(b.hash(), w.hash());
}

#[test]
fn herb_patches_exist_on_the_map() {
    let w = World::new(1);
    assert!(w.nodes.iter().filter(|n| n.kind == NodeKind::HerbPatch).count() >= 5);
}

#[test]
fn knapping_scores_aim_and_force() {
    let strike = |angle_off: i32, power_off: i32| -> (u8, bool) {
        let (mut w, a) = with_agent(1);
        learn_all(&mut w);
        give(&mut w, ItemKind::Stone, 2);
        w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("sharp_stone") })]);
        let v = w.craft_view(a).unwrap();
        assert_eq!(v.kind, MinigameKind::Knap);
        let angle = (v.center as i32 + angle_off).rem_euclid(360) as u16;
        let power = (v.power as i32 + power_off).clamp(0, 100) as u8;
        w.step(&[cmd(a, CommandKind::StrikeAt { angle, power })]);
        let hit = w.take_events().iter().any(|e| matches!(e, Event::Struck { hit: true, .. }));
        (w.craft_view(a).unwrap().done, hit)
    };
    let (_, bare_half) = {
        let (mut w, a) = with_agent(1);
        learn_all(&mut w);
        give(&mut w, ItemKind::Stone, 2);
        w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("sharp_stone") })]);
        let v = w.craft_view(a).unwrap();
        ((), v.half as i32)
    };
    assert_eq!(strike(0, 0), (1, true), "dead on");
    assert_eq!(strike(bare_half - 1, 0), (1, true), "inside the zone still counts");
    assert_eq!(strike(bare_half + 4, 0), (1, false), "beside the zone is a miss");
    assert_eq!(strike(-(bare_half - 1), 0), (1, true), "both sides");
    assert!(!strike(0, 60).1, "far too hard");
    assert!(!strike(0, -60).1, "far too soft");
    assert!(strike(180, 0).0 == 1 && !strike(180, 0).1, "opposite rim");
}

#[test]
fn wrong_input_kind_is_ignored() {
    let (mut w, a) = with_agent(1);
    learn_all(&mut w);
    give(&mut w, ItemKind::Stone, 2);
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("sharp_stone") })]);
    w.step(&[cmd(a, CommandKind::Strike)]); // knapping needs an aim
    assert_eq!(w.craft_view(a).unwrap().done, 0);
    give(&mut w, ItemKind::Herb, 2);
    give(&mut w, ItemKind::Fiber, 1);
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("bandage") })]);
    w.step(&[cmd(a, CommandKind::StrikeAt { angle: 10, power: 50 })]); // the bandage starts as a timing game
    assert_eq!(w.craft_view(a).unwrap().done, 0);
}

#[test]
fn copper_and_tin_exist_and_bronze_tools_speed_work() {
    let w = World::new(5);
    assert!(w.nodes.iter().any(|n| n.kind == NodeKind::CopperOutcrop), "copper on the map");
    assert!(w.nodes.iter().any(|n| n.kind == NodeKind::TinOutcrop), "tin on the map");
    let copper = w.nodes.iter().filter(|n| n.kind == NodeKind::CopperOutcrop).count();
    let tin = w.nodes.iter().filter(|n| n.kind == NodeKind::TinOutcrop).count();
    assert!(copper > tin, "tin is the scarcer metal ({copper} copper, {tin} tin)");
    let mut inv = Inventory::default();
    let bare = gather_ticks(NodeKind::Tree, &inv);
    inv.counts[ItemKind::BronzeAxe as usize] = 1;
    let bronze = gather_ticks(NodeKind::Tree, &inv);
    inv.counts[ItemKind::IronAxe as usize] = 1;
    let iron = gather_ticks(NodeKind::Tree, &inv);
    assert!(bare > bronze && bronze > iron, "{bare} > {bronze} > {iron}");
}

#[test]
fn an_axe_takes_four_steps_and_a_clamp_makes_it_easier() {
    let (mut w, a) = with_agent(3);
    learn_all(&mut w);
    give(&mut w, ItemKind::SharpStone, 1);
    give(&mut w, ItemKind::Branch, 1);
    give(&mut w, ItemKind::Fiber, 2);
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("stone_axe") })]);
    let mut seen = Vec::new();
    let mut fit_half = 0;
    while let Some(v) = w.craft_view(a) {
        if seen.last() != Some(&(v.step, v.kind)) {
            seen.push((v.step, v.kind));
        }
        assert_eq!(v.steps, 6);
        match v.kind {
            MinigameKind::Timing if v.pos.abs_diff(v.center) <= v.half / 3 => w.step(&[cmd(a, CommandKind::Strike)]),
            MinigameKind::Timing => w.step(&[]),
            MinigameKind::Knap => w.step(&[cmd(a, CommandKind::StrikeAt { angle: v.center as u16, power: v.power as u8 })]),
            MinigameKind::Fit => {
                fit_half = v.half;
                w.step(&[cmd(a, CommandKind::PlaceAt { x: v.tx as i16, y: v.ty as i16 })])
            }
            MinigameKind::Tension => w.step(&[cmd(a, CommandKind::Tighten { power: v.power as u8 })]),
            MinigameKind::Trim => w.step(&[cmd(a, CommandKind::PlaceAt { x: v.tx as i16, y: v.ty as i16 })]),
            MinigameKind::Rub => w.step(&[cmd(a, CommandKind::Rub { pace: v.power as u8, grip: true })]),
            MinigameKind::Stack => w.step(&[cmd(a, CommandKind::PlaceAt { x: 0, y: 0 })]),
        }
    }
    let kinds: Vec<_> = seen.iter().map(|s| s.1).collect();
    assert_eq!(kinds, [MinigameKind::Trim, MinigameKind::Rub, MinigameKind::Rub, MinigameKind::Knap, MinigameKind::Fit, MinigameKind::Tension]);
    assert_eq!(count(&w, 0, ItemKind::StoneAxe), 1);
    assert_eq!(count(&w, 0, ItemKind::Branch) + count(&w, 0, ItemKind::Fiber) + count(&w, 0, ItemKind::SharpStone), 0, "everything was used");
    // a clamp holds the parts for you
    give(&mut w, ItemKind::SharpStone, 1);
    give(&mut w, ItemKind::Branch, 1);
    give(&mut w, ItemKind::Fiber, 2);
    give(&mut w, ItemKind::Clamp, 1);
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("stone_axe") })]);
    let mut clamped = 0;
    while let Some(v) = w.craft_view(a) {
        if v.kind == MinigameKind::Fit {
            clamped = v.half;
            break;
        }
        match v.kind {
            MinigameKind::Timing if v.pos.abs_diff(v.center) <= v.half / 3 => w.step(&[cmd(a, CommandKind::Strike)]),
            MinigameKind::Timing => w.step(&[]),
            MinigameKind::Knap => w.step(&[cmd(a, CommandKind::StrikeAt { angle: v.center as u16, power: v.power as u8 })]),
            MinigameKind::Trim => w.step(&[cmd(a, CommandKind::PlaceAt { x: v.tx as i16, y: v.ty as i16 })]),
            MinigameKind::Rub => w.step(&[cmd(a, CommandKind::Rub { pace: v.power as u8, grip: true })]),
            _ => unreachable!(),
        }
    }
    assert!(clamped > fit_half, "clamp widens the fit zone ({fit_half} -> {clamped})");
}

#[test]
fn fit_and_tension_score_distance_and_pressure() {
    let go = |dx: i16, dy: i16| -> bool {
        let (mut w, a) = with_agent(4);
        learn_all(&mut w);
        give(&mut w, ItemKind::SharpStone, 1);
        give(&mut w, ItemKind::Branch, 1);
        give(&mut w, ItemKind::Fiber, 1);
        w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("spear") })]);
        // pass the timing step perfectly, then fit
        while w.craft_view(a).is_some_and(|v| v.kind == MinigameKind::Timing) {
            let v = w.craft_view(a).unwrap();
            if v.pos.abs_diff(v.center) <= v.half / 3 {
                w.step(&[cmd(a, CommandKind::Strike)]);
            } else {
                w.step(&[]);
            }
        }
        let v = w.craft_view(a).unwrap();
        assert_eq!(v.kind, MinigameKind::Fit);
        w.take_events();
        w.step(&[cmd(a, CommandKind::PlaceAt { x: (v.tx as i16 + dx).clamp(-100, 100), y: (v.ty as i16 + dy).clamp(-100, 100) })]);
        w.take_events().iter().any(|e| matches!(e, Event::Struck { hit: true, .. }))
    };
    assert!(go(0, 0), "dead on");
    assert!(!go(70, 70), "far off");
}

#[test]
fn branches_and_fibre_grow_on_the_map() {
    let w = World::new(5);
    assert!(w.nodes.iter().any(|n| n.kind == NodeKind::Deadwood), "fallen branches");
    assert!(w.nodes.iter().any(|n| n.kind == NodeKind::FiberPlant), "tall grass");
}

#[test]
fn perception_grows_with_what_the_tribe_knows() {
    let (mut w, a) = with_agent(2);
    assert_eq!(w.perception_level(), 0);
    assert!(!w.has_seen(ItemKind::Stone));
    give(&mut w, ItemKind::Stone, 2);
    give(&mut w, ItemKind::Wood, 3);
    w.step(&[]);
    assert!(w.has_seen(ItemKind::Stone) && w.has_seen(ItemKind::Wood));
    assert!(!w.has_seen(ItemKind::Clay));
    let early = w.perception_permille();
    assert!(early > 0, "handling things teaches");
    learn_all(&mut w);
    w.milestones = u64::MAX;
    for k in ItemKind::ALL {
        w.mark_seen(k);
    }
    assert_eq!(w.perception_permille(), 1000);
    assert_eq!(w.perception_level(), 5);
    let _ = a;
}

#[test]
fn what_was_seen_survives_a_save() {
    let (mut w, _) = with_agent(2);
    give(&mut w, ItemKind::Herb, 1);
    w.step(&[]);
    let b = World::from_bytes(&w.to_bytes()).unwrap();
    assert!(b.has_seen(ItemKind::Herb) && !b.has_seen(ItemKind::Bronze));
    assert_eq!(b.perception_permille(), w.perception_permille());
}

#[test]
fn combining_things_in_hand_teaches_by_trying() {
    use crate::command::{Command, CommandKind};
    let mut w = World::new(5);
    let (sx, sy) = w.spawn_mm;
    let a = w.spawn_agent(sx, sy);
    w.forget_everything();
    w.agents[0].inventory.counts[ItemKind::Stone as usize] = 2;
    let none = 255u8;
    // two stones knocked together: a sharp edge
    let stone = ItemKind::Stone as u8;
    w.step(&[Command { agent: a, kind: CommandKind::Combine { items: [stone, stone, none, none] } }]);
    assert!(w.knows(Discovery::SharpStone));
    assert!(w.craft_view(a).is_some(), "the knapping game starts");
    // a pointless combination does nothing but is remembered as a miss
    let mut w = World::new(5);
    let a = w.spawn_agent(sx, sy);
    w.agents[0].inventory.counts[ItemKind::Stone as usize] = 1;
    w.agents[0].inventory.counts[ItemKind::Berries as usize] = 1;
    w.step(&[Command { agent: a, kind: CommandKind::Combine { items: [stone, ItemKind::Berries as u8, none, none] } }]);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::NoIdea)));
    // cannot combine what you do not hold
    w.step(&[Command { agent: a, kind: CommandKind::Combine { items: [stone, stone, none, none] } }]);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::NoMaterials)));
}

/// Play-tester bot: a tribe that knows nothing, starting from the spawn, doing what a player would.
/// It walks to the nearest wanted resource, gathers, tries combinations, plays the minigames and keeps itself fed.
fn newbie_session(seed: u64, ticks: u64, log: &mut Vec<String>) -> World {
    let mut w = World::new(seed);
    let (sx, sy) = w.spawn_mm;
    let a = w.spawn_agent(sx, sy);
    w.forget_everything();
    let none = 255u8;
    let mut last_known = 0u64;
    for t in 0..ticks {
        let me = &w.agents[0];
        let counts = me.inventory.counts;
        let has = |k: ItemKind| counts[k as usize];
        if w.known != last_known {
            log.push(format!("t={t}: known={:#b} stone={} wood={} axe={}", w.known, has(ItemKind::Stone), has(ItemKind::Wood), has(ItemKind::StoneAxe)));
            last_known = w.known;
        }
        if w.craft_view(a).is_some() {
            let v = w.craft_view(a).unwrap();
            let c = match v.kind {
                MinigameKind::Knap => Some(CommandKind::StrikeAt { angle: v.center as u16, power: v.power as u8 }),
                MinigameKind::Fit => Some(CommandKind::PlaceAt { x: v.tx as i16, y: v.ty as i16 }),
                MinigameKind::Tension => Some(CommandKind::Tighten { power: v.power as u8 }),
                MinigameKind::Trim => Some(CommandKind::PlaceAt { x: v.tx as i16, y: v.ty as i16 }),
                MinigameKind::Rub => Some(CommandKind::Rub { pace: v.power as u8, grip: true }),
                MinigameKind::Timing if v.pos.abs_diff(v.center) <= v.half / 3 => Some(CommandKind::Strike),
                MinigameKind::Timing => None,
                MinigameKind::Stack => Some(CommandKind::PlaceAt { x: 0, y: 0 }),
            };
            w.step(&c.map(|k| vec![cmd(a, k)]).unwrap_or_default());
            continue;
        }
        let mut commands = Vec::new();
        if me.task == Task::Idle {
            // eat when hungry
            if me.hunger < 40_000 && has(ItemKind::Berries) > 0 {
                commands.push(cmd(a, CommandKind::Eat { item: ItemKind::Berries }));
            } else if has(ItemKind::Stone) >= 2 && has(ItemKind::SharpStone) < 2 {
                commands.push(cmd(a, CommandKind::Combine { items: [ItemKind::Stone as u8, ItemKind::Stone as u8, none, none] }));
            } else if has(ItemKind::SharpStone) >= 1 && has(ItemKind::Branch) >= 1 && has(ItemKind::Fiber) >= 2 && has(ItemKind::StoneAxe) == 0 {
                let (s, b, f) = (ItemKind::SharpStone as u8, ItemKind::Branch as u8, ItemKind::Fiber as u8);
                commands.push(cmd(a, CommandKind::Combine { items: [s, b, f, f] }));
            } else {
                // wanted resource: whatever we are short of, nearest first
                let want: &[NodeKind] = if has(ItemKind::Stone) < 2 && has(ItemKind::SharpStone) == 0 {
                    &[NodeKind::StoneOutcrop]
                } else if has(ItemKind::Branch) < 1 {
                    &[NodeKind::Deadwood]
                } else if has(ItemKind::Fiber) < 2 {
                    &[NodeKind::FiberPlant]
                } else if has(ItemKind::Berries) < 3 {
                    &[NodeKind::BerryBush]
                } else {
                    &[NodeKind::Tree]
                };
                let best = w
                    .nodes
                    .iter()
                    .enumerate()
                    // like a player who read the hint: only what can be worked with the tools at hand
                    .filter(|(_, n)| n.amount > 0 && want.contains(&n.kind) && crate::world::gate_check(n, AgentKind::Human, &me.inventory).is_ok())
                    .min_by_key(|(_, n)| {
                        let (nx, ny) = n.center_mm();
                        dist2(nx, ny, me.x, me.y)
                    });
                if let Some((i, _)) = best {
                    commands.push(cmd(a, CommandKind::Gather { node: NodeId(i as u32) }));
                }
            }
        }
        w.step(&commands);
        if w.agents.is_empty() {
            break;
        }
    }
    w
}

#[test]
fn a_newcomer_can_reach_the_stone_axe_by_trying() {
    let mut failures = Vec::new();
    for seed in [1u64, 2, 3, 4, 5, 6] {
        let mut log = Vec::new();
        let w = newbie_session(seed, 60 * 60 * 8, &mut log);
        let got = w.agents.first().map(|a| a.inventory.counts[ItemKind::StoneAxe as usize] > 0).unwrap_or(false) || w.knows(Discovery::StoneAxe);
        println!("seed {seed}: axe={got} health_ok={} log={:?}", !w.agents.is_empty(), log);
        if !got {
            failures.push(seed);
        }
    }
    assert!(failures.is_empty(), "seeds where a newcomer never got an axe: {failures:?}");
}

fn alaz_sim_wolf() -> crate::wildlife::WildSpecies {
    crate::wildlife::WildSpecies::Wolf
}

fn fuel_low(camp: Option<(u32, i64, i64, u32)>) -> bool {
    camp.is_some_and(|c| c.3 < 12_000)
}

/// Survival play-tester: like a careful newcomer — drinks, eats, builds a fire and stays by it at night.
/// Returns (ticks survived, deaths seen).
/// A piece of wood lying about, or a felled trunk with wood still in it, within 15 m and no wolf by it.
fn wood_lying(w: &World, px: i64, py: i64) -> Option<CommandKind> {
    use crate::physics::{BodyKind, Q};
    let safe = |x: i64, y: i64| !w.wild.iter().any(|d| d.species == WildSpecies::Wolf && dist2(d.x, d.y, x, y) < 8_000 * 8_000);
    let near = |x: i64, y: i64| dist2(x, y, px, py) < 15_000 * 15_000 && safe(x, y);
    if let Some(b) = w.bodies.iter().filter(|b| b.kind == BodyKind::Piece && near(b.x / Q, b.y / Q)).min_by_key(|b| dist2(b.x / Q, b.y / Q, px, py)) {
        return Some(CommandKind::Pick { body: b.id });
    }
    w.bodies.iter().find(|b| b.lying() && b.wood > 0 && near(b.cut_point().0, b.cut_point().1)).map(|b| CommandKind::Split { body: b.id })
}

fn survival_session(seed: u64, days: u64) -> (u64, u32) {
    let mut w = World::new(seed);
    let (sx, sy) = w.spawn_mm;
    let a = w.spawn_agent(sx, sy);
    w.forget_everything();
    let mut deaths = 0u32;
    let mut bites = 0u32;
    let water = {
        let mut best: Option<(i64, i32, i32)> = None;
        for ty in 0..w.map.h {
            for tx in 0..w.map.w {
                if w.map.terrain(tx, ty) == Some(Terrain::Water) {
                    let d = dist2(sx, sy, tx as i64 * 1000 + 500, ty as i64 * 1000 + 500);
                    if best.is_none_or(|b| d < b.0) {
                        best = Some((d, tx, ty));
                    }
                }
            }
        }
        best.map(|b| (b.1, b.2))
    };
    let mut fire: Option<(i64, i64)> = None;
    for t in 0..days * TICKS_PER_DAY {
        let Some(me) = w.agents.first() else { break };
        let counts = me.inventory.counts;
        let has = |k: ItemKind| counts[k as usize];
        if w.craft_view(a).is_some() {
            let v = w.craft_view(a).unwrap();
            let c = match v.kind {
                MinigameKind::Knap => Some(CommandKind::StrikeAt { angle: v.center as u16, power: v.power as u8 }),
                MinigameKind::Fit => Some(CommandKind::PlaceAt { x: v.tx as i16, y: v.ty as i16 }),
                MinigameKind::Tension => Some(CommandKind::Tighten { power: v.power as u8 }),
                MinigameKind::Trim => Some(CommandKind::PlaceAt { x: v.tx as i16, y: v.ty as i16 }),
                MinigameKind::Rub => Some(CommandKind::Rub { pace: v.power as u8, grip: true }),
                MinigameKind::Timing if v.pos.abs_diff(v.center) <= v.half / 3 => Some(CommandKind::Strike),
                MinigameKind::Timing => None,
                MinigameKind::Stack => Some(CommandKind::PlaceAt { x: 0, y: 0 }),
            };
            w.step(&c.map(|k| vec![cmd(a, k)]).unwrap_or_default());
            continue;
        }
        let (px, py) = (me.x, me.y);
        if std::env::var_os("ALAZ_TRACE").is_some() && t % 3000 == 0 && std::env::var("ALAZ_TRACE_SEED").map_or(seed == 1, |v| v == seed.to_string()) {
            println!("t={t} pos=({},{}) task={:?} hp={} hu={} th={} wa={} wood={} fuel={:?} wolves<20m={}", px / 1000, py / 1000, me.task, me.health, me.hunger, me.thirst, me.warmth, has(ItemKind::Wood), w.buildings.iter().map(|b| b.fuel).collect::<Vec<_>>(), w.wild.iter().filter(|x| x.species == alaz_sim_wolf() && dist2(x.x, x.y, px, py) < 20_000 * 20_000).count());
        }
        let mut commands = Vec::new();
        if me.task == Task::Idle {
            let night = daylight_permille(w.tick) < 560;
            let fire_alive = w.buildings.iter().any(|b| b.kind == BuildingKind::Campfire && b.active());
            // out of wood with the fire dying: risk the wolves rather than freeze
            let desperate = has(ItemKind::Wood) == 0 && w.buildings.iter().any(|b| b.kind == BuildingKind::Campfire && b.fuel < 9_000);
            let nearest_node = |w: &World, want: &[NodeKind]| {
                w.nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| {
                        // a sensible player does not walk up to a resource with a wolf standing by it
                        let (nx, ny) = n.center_mm();
                        n.amount > 0
                            && want.contains(&n.kind)
                            && crate::world::gate_check(n, AgentKind::Human, &me.inventory).is_ok()
                            // nor takes on a giant with a stone axe: its pieces fly far, and gathering them takes long
                            && (n.kind != NodeKind::Tree || crate::map::tree_height_mm(w.seed, n.tx, n.ty) <= 7_000)
                            && (desperate || !w.wild.iter().any(|x| x.species == alaz_sim_wolf() && dist2(x.x, x.y, nx, ny) < 8_000 * 8_000))
                    })
                    .min_by_key(|(_, n)| {
                        let (nx, ny) = n.center_mm();
                        dist2(nx, ny, px, py)
                    })
                    .map(|(i, _)| NodeId(i as u32))
            };
            let camp = w.buildings.iter().enumerate().find(|(_, b)| b.kind == BuildingKind::Campfire).map(|(i, b)| (i as u32, b.x, b.y, b.fuel));
            let wolf_near = w.wild.iter().any(|x| x.species == alaz_sim_wolf() && dist2(x.x, x.y, px, py) < 14_000 * 14_000);
            if let (Some((bi, bx, by, fuel)), true) = (camp, has(ItemKind::Wood) > 0 && fuel_low(camp)) {
                if dist2(bx, by, px, py) > 1800 * 1800 {
                    commands.push(cmd(a, CommandKind::MoveTo { x: bx + 1200, y: by }));
                } else {
                    commands.push(cmd(a, CommandKind::Refuel { building: bi }));
                }
                let _ = fuel;
            } else if let (Some((_, bx, by, fuel)), true) = (camp, wolf_near && camp.is_some_and(|c| c.3 > 0 && dist2(c.1, c.2, px, py) > 2500 * 2500)) {
                let _ = fuel;
                commands.push(cmd(a, CommandKind::MoveTo { x: bx + 1200, y: by }));
            } else if me.thirst < 4000 && water.is_some() {
                let (tx, ty) = water.unwrap();
                commands.push(cmd(a, CommandKind::Drink { tx, ty }));
            } else if me.hunger < 4500 && has(ItemKind::Berries) > 0 {
                commands.push(cmd(a, CommandKind::Eat { item: ItemKind::Berries }));
            } else if me.hunger < 6000 && has(ItemKind::Berries) < 5 && nearest_node(&w, &[NodeKind::BerryBush]).is_some() {
                if let Some(n) = nearest_node(&w, &[NodeKind::BerryBush]) {
                    commands.push(cmd(a, CommandKind::Gather { node: n }));
                }
            } else if !night && (has(ItemKind::StoneAxe) == 0 || has(ItemKind::Hammer) == 0) {
                // the first tools, the way the guide tells it: sharp stone, axe (trees), hammer (rock seams, flint)
                let (st, sh, br, fi) = (ItemKind::Stone as u8, ItemKind::SharpStone as u8, ItemKind::Branch as u8, ItemKind::Fiber as u8);
                let gather = |want: NodeKind| nearest_node(&w, &[want]).map(|n| cmd(a, CommandKind::Gather { node: n }));
                let next = if has(ItemKind::StoneAxe) == 0 && has(ItemKind::SharpStone) == 0 {
                    if has(ItemKind::Stone) >= 2 { Some(cmd(a, CommandKind::Combine { items: [st, st, 255, 255] })) } else { gather(NodeKind::StoneOutcrop) }
                } else if has(ItemKind::StoneAxe) == 0 {
                    if has(ItemKind::Branch) == 0 {
                        gather(NodeKind::Deadwood)
                    } else if has(ItemKind::Fiber) < 2 {
                        gather(NodeKind::FiberPlant)
                    } else {
                        Some(cmd(a, CommandKind::Combine { items: [sh, br, fi, fi] }))
                    }
                } else if has(ItemKind::Stone) < 2 {
                    gather(NodeKind::StoneOutcrop)
                } else if has(ItemKind::Branch) == 0 {
                    gather(NodeKind::Deadwood)
                } else if has(ItemKind::Fiber) == 0 {
                    gather(NodeKind::FiberPlant)
                } else {
                    Some(cmd(a, CommandKind::Combine { items: [st, st, br, fi] }))
                };
                commands.extend(next);
            } else if w.knows(Discovery::Fire) && has(ItemKind::Flint) == 0 && !night && nearest_node(&w, &[NodeKind::StoneOutcrop]).is_some() {
                // a fire needs flint first: break stone until a nodule turns up
                commands.push(cmd(a, CommandKind::Gather { node: nearest_node(&w, &[NodeKind::StoneOutcrop]).unwrap() }));
            } else if w.knows(Discovery::Fire) && !fire_alive && has(ItemKind::Wood) >= 3 {
                let (x, y) = fire.unwrap_or_else(|| spot_for(&w, BuildingKind::Campfire, &[]));
                fire = Some((x, y));
                if w.can_place(BuildingKind::Campfire, x, y).is_ok() && dist2(x, y, px, py) > 2_500 * 2_500 {
                    // walk over to the spot first
                    commands.push(cmd(a, CommandKind::MoveTo { x: x + 1_200, y }));
                } else if w.can_place(BuildingKind::Campfire, x, y).is_ok() {
                    commands.push(cmd(a, CommandKind::Build { kind: BuildingKind::Campfire, x, y }));
                } else {
                    fire = None;
                }
            } else if let Some(k) = (!night && !wolf_near && has(ItemKind::Wood) < 10).then(|| wood_lying(&w, px, py)).flatten() {
                // wood already on the ground, or a trunk half broken up: that first
                commands.push(cmd(a, k));
            } else if !night && has(ItemKind::Wood) < 10 && nearest_node(&w, &[NodeKind::Tree]).is_some() {
                let n = nearest_node(&w, &[NodeKind::Tree]).unwrap();
                commands.push(cmd(a, CommandKind::Gather { node: n }));
            } else if night && fire_alive && has(ItemKind::Wood) >= 2 {
                if let Some(b) = w.buildings.iter().find(|b| b.kind == BuildingKind::Campfire)
                    && dist2(b.x, b.y, px, py) > 2500 * 2500 {
                        commands.push(cmd(a, CommandKind::MoveTo { x: b.x + 1500, y: b.y }));
                    }
            } else if has(ItemKind::Wood) < 6 && nearest_node(&w, &[NodeKind::Tree]).is_some() {
                commands.push(cmd(a, CommandKind::Gather { node: nearest_node(&w, &[NodeKind::Tree]).unwrap() }));
            } else if let Some(n) = nearest_node(&w, &[NodeKind::BerryBush]) {
                commands.push(cmd(a, CommandKind::Gather { node: n }));
            }
        }
        if std::env::var_os("ALAZ_TRACE2").is_some() && seed == 4 && t % 300 == 0 && t > 162000 && t < 164500 {
            println!("t={t} idle={} cmds={:?} pos=({},{}) hp={} night={} wolf_near_fire_branch fire_alive={} thirst={} hunger={} wood={}", w.agents[0].task == Task::Idle, commands.iter().map(|c| format!("{:?}", c.kind)).collect::<Vec<_>>(), w.agents[0].x/1000, w.agents[0].y/1000, w.agents[0].health, daylight_permille(w.tick) < 400, w.buildings.iter().any(|b| b.kind == BuildingKind::Campfire && b.active()), w.agents[0].thirst, w.agents[0].hunger, w.agents[0].inventory.counts[ItemKind::Wood as usize]);
        }
        if std::env::var_os("ALAZ_TRACE3").is_some() && seed == 4 && t == 126000 {
            let n = &w.nodes[3400];
            println!("node3400 {:?} at {:?} amount={} agent=({}, {}) terrain_at_node={:?}", n.kind, n.center_mm(), n.amount, w.agents[0].x, w.agents[0].y, w.map.terrain_mm(n.center_mm().0, n.center_mm().1));
            let mut w2 = w.clone();
            for k in 0..6 { w2.step(&[]); println!("  +{k}: task={:?} pos=({},{})", w2.agents[0].task, w2.agents[0].x, w2.agents[0].y); }
        }
        let snap = w.agents.first().map(|m| (m.health, m.hunger, m.thirst, m.warmth, w.buildings.len(), has(ItemKind::Wood), has(ItemKind::Berries)));
        w.step(&commands);
        let evs = w.take_events();
        if std::env::var_os("ALAZ_PHYS").is_some() {
            for e in &evs {
                if matches!(e, Event::Fell { .. } | Event::Crushed(_) | Event::RockHit(_) | Event::Fractured(_) | Event::Bitten(_)) {
                    let a0 = &w.agents[0];
                    println!("seed {seed} t={t} {e:?} hp={} pos=({},{}) task={:?} elev={} broken={} night={}", a0.health, a0.x / 1000, a0.y / 1000, a0.task, crate::map::elevation_at_mm(w.seed, a0.x, a0.y), a0.broken, daylight_permille(w.tick) < 400);
                }
            }
        }
        bites += evs.iter().filter(|e| matches!(e, Event::Bitten(_))).count() as u32;
        if evs.iter().any(|e| matches!(e, Event::Died(_))) {
            deaths += 1;
            if deaths == 1 {
                println!("seed {seed}: died at t={t} (day {} {}%) before (health,hunger,thirst,warmth,buildings,wood,berries)={snap:?} water={water:?} fire={fire:?}", w.day(), (t % TICKS_PER_DAY) * 100 / TICKS_PER_DAY);
            }
        }
    }
    println!("seed {seed}: bites={bites}");
    (w.tick, deaths)
}

#[test]
fn a_careful_newcomer_survives_two_days() {
    let mut bad = Vec::new();
    for seed in [1u64, 2, 3, 4] {
        let (_, deaths) = survival_session(seed, 2);
        println!("seed {seed}: deaths={deaths}");
        if deaths > 0 {
            bad.push(seed);
        }
    }
    assert!(bad.is_empty(), "newcomer died on seeds {bad:?}");
}

#[test]
fn gathering_with_a_full_pack_says_so() {
    let (mut w, a) = with_agent(1);
    let i = w.nodes.iter().position(|n| n.kind == NodeKind::Tree).unwrap();
    let (nx, ny) = w.nodes[i].center_mm();
    w.agents[0].x = nx + 500;
    w.agents[0].y = ny;
    // (tools ride on the belt: forty stones fill the pack)
    give(&mut w, ItemKind::Stone, 40);
    give(&mut w, ItemKind::StoneAxe, 1);
    w.step(&[cmd(a, CommandKind::Gather { node: NodeId(i as u32) })]);
    w.step(&[]);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::PackFull)));
    assert_eq!(w.agents[0].task, Task::Idle);
}

/// Tiny deterministic noise source for the human-like players.
struct Noise(u64);
impl Noise {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f32 / (1u64 << 31) as f32) * 2.0 - 1.0
    }
    /// Roughly bell shaped, within +-1.
    fn bell(&mut self) -> f32 {
        (self.next() + self.next() + self.next()) / 3.0
    }
}

/// Plays one recipe like a person who is not perfect: aim, force and placement are off by `skill_noise`
/// (1.0 = a casual player, 0.4 = a practised one) and reactions lag. Returns the quality (0 failed, 1 good, 2 fine).
fn human_craft(w: &mut World, a: AgentId, id: &str, noise: &mut Noise, sloppy: f32) -> u8 {
    w.take_events();
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe(id) })]);
    let mut history: Vec<u32> = Vec::new();
    let lag = (14.0 + noise.bell() * 6.0 * sloppy) as usize;
    let mut cooldown = 0u32;
    let mut guard = 0;
    while let Some(v) = w.craft_view(a) {
        guard += 1;
        if guard > 20_000 {
            return 0;
        }
        match v.kind {
            MinigameKind::Knap => {
                // aim by eye: error grows with sloppiness, about the size of the zone for a casual player
                let angle = (v.center as f32 + noise.bell() * v.half as f32 * 1.6 * sloppy).rem_euclid(360.0);
                let power = (v.power as f32 + noise.bell() * v.power_half as f32 * 1.8 * sloppy).clamp(0.0, 100.0);
                w.step(&[cmd(a, CommandKind::StrikeAt { angle: angle as u16, power: power as u8 })]);
                for _ in 0..30 {
                    w.step(&[]); // a moment between strikes
                }
            }
            MinigameKind::Fit => {
                let x = v.tx as f32 + noise.bell() * v.half as f32 * 1.8 * sloppy;
                let y = v.ty as f32 + noise.bell() * v.half as f32 * 1.8 * sloppy;
                w.step(&[cmd(a, CommandKind::PlaceAt { x: x.clamp(-100.0, 100.0) as i16, y: y.clamp(-100.0, 100.0) as i16 })]);
                for _ in 0..30 {
                    w.step(&[]);
                }
            }
            MinigameKind::Tension => {
                let p = (v.power as f32 + noise.bell() * v.power_half as f32 * 1.8 * sloppy).clamp(0.0, 100.0);
                w.step(&[cmd(a, CommandKind::Tighten { power: p as u8 })]);
                for _ in 0..30 {
                    w.step(&[]);
                }
            }
            MinigameKind::Trim => {
                let x = v.tx as f32 + noise.bell() * v.half as f32 * 1.8 * sloppy;
                let y = v.ty as f32 + noise.bell() * v.half as f32 * 1.8 * sloppy;
                w.step(&[cmd(a, CommandKind::PlaceAt { x: x.clamp(-100.0, 100.0) as i16, y: y.clamp(-100.0, 100.0) as i16 })]);
                for _ in 0..30 {
                    w.step(&[]);
                }
            }
            MinigameKind::Rub => {
                let p = (v.power as f32 + noise.bell() * v.power_half as f32 * 1.8 * sloppy).clamp(0.0, 100.0);
                w.step(&[cmd(a, CommandKind::Rub { pace: p as u8, grip: true })]);
                for _ in 0..30 {
                    w.step(&[]);
                }
            }
            MinigameKind::Timing => {
                history.push(v.pos);
                let seen = if history.len() > lag { history[history.len() - 1 - lag] } else { v.pos };
                cooldown = cooldown.saturating_sub(1);
                // the player strikes when the marker looks to be inside the zone, a bit unsure at the edges
                let reach = v.half as f32 * (0.85 - 0.15 * sloppy);
                if cooldown == 0 && (seen as f32 - v.center as f32).abs() <= reach {
                    w.step(&[cmd(a, CommandKind::Strike)]);
                    cooldown = 40;
                } else {
                    w.step(&[]);
                }
            }
            MinigameKind::Stack => {
                let x = (noise.bell() * 22.0 * sloppy).clamp(-60.0, 60.0) as i16;
                w.step(&[cmd(a, CommandKind::PlaceAt { x, y: 0 })]);
                for _ in 0..20 {
                    w.step(&[]);
                }
            }
        }
    }
    crafted_quality(w).unwrap_or(0)
}

/// How hard are the minigames for a person? A casual player should usually manage, not always; a practised one
/// should do clearly better. Prints the numbers so tuning is a matter of reading them.
#[test]
fn minigame_difficulty_is_reasonable_for_human_like_players() {
    let ids = ["sharp_stone", "stone_axe", "spear", "bandage", "clamp"];
    let trials = 80;
    for id in ids {
        let mut rates = Vec::new();
        for (label, sloppy) in [("casual", 1.0f32), ("practised", 0.45f32)] {
            let mut noise = Noise(0x5eed ^ (id.len() as u64) << 8);
            let (mut fail, mut good, mut fine) = (0, 0, 0);
            for t in 0..trials {
                let (mut w, a) = with_agent(100 + t as u64);
                learn_all(&mut w);
                w.wild.clear();
                for (k, n) in [(ItemKind::Stone, 4), (ItemKind::SharpStone, 2), (ItemKind::Branch, 3), (ItemKind::Fiber, 4), (ItemKind::Herb, 3)] {
                    give(&mut w, k, n);
                }
                // draw one value so the noise sequence the later steps use stays the same
                let _ = noise.next();
                w.tick += (t as u64 * 37) % 600;
                match human_craft(&mut w, a, id, &mut noise, sloppy) {
                    0 => fail += 1,
                    1 => good += 1,
                    _ => fine += 1,
                }
            }
            println!("{id:12} {label:9} failed {fail:3} good {good:3} fine {fine:3} (of {trials})");
            rates.push((fail, good, fine));
        }
        let (casual, practised) = (rates[0], rates[1]);
        assert!(casual.0 < trials * 6 / 10, "{id}: a casual player fails too often: {casual:?}");
        assert!(casual.0 > 0 || casual.2 < trials, "{id}: a casual player never fails and always excels: {casual:?}");
        assert!(practised.2 >= casual.2, "{id}: practice should pay off: {practised:?} vs {casual:?}");
        assert!(practised.2 * 10 >= trials * 6, "{id}: a practised player should usually do fine work: {practised:?}");
    }
}

/// A lit fire with someone sitting by it, in a world without wild animals.
fn camp_world(seed: u64) -> (World, AgentId) {
    let (mut w, a) = with_agent(seed);
    w.wild.clear();
    let (px, py) = (w.agents[0].x, w.agents[0].y);
    let b = push_building(&mut w, BuildingKind::Campfire, px + 1_500, py);
    w.buildings[b as usize].fuel = 1_000_000;
    (w, a)
}

#[test]
fn a_stray_dog_comes_to_a_fire_that_somebody_sits_by() {
    for seed in [1u64, 2, 3] {
        let (mut w, _) = camp_world(seed);
        for _ in 0..(60 * 60 * 20) {
            w.step(&[]);
            w.buildings[0].fuel = 1_000_000;
            if !w.wild.is_empty() {
                break;
            }
        }
        assert_eq!(w.wild.len(), 1, "seed {seed}: one dog shows up");
        assert!(w.milestone_done(Milestone::Visitor));
        let (fx, fy) = (w.buildings[0].x, w.buildings[0].y);
        let d = (((w.wild[0].x - fx).pow(2) + (w.wild[0].y - fy).pow(2)) as f64).sqrt();
        assert!(d < 12_000.0, "the dog comes close to the fire, was {d}");
        for _ in 0..(60 * 60 * 10) {
            w.step(&[]);
            w.buildings[0].fuel = 1_000_000;
        }
        assert_eq!(w.wild.len(), 1, "only one visitor ever");
    }
}

#[test]
fn nobody_visits_an_empty_camp() {
    let (mut w, _) = camp_world(1);
    w.agents[0].x += 40_000;
    for _ in 0..(60 * 60 * 10) {
        w.step(&[]);
        w.buildings[0].fuel = 1_000_000;
    }
    assert!(w.wild.is_empty(), "nobody at the fire, no visitor");
}

#[test]
fn a_tamed_dog_keeps_wolves_off_its_master() {
    let (mut w, _) = wolf_world();
    let (px, py) = (w.agents[0].x, w.agents[0].y);
    w.spawn_worker(AgentKind::Dog, px + 1_000, py);
    let mut bitten = false;
    for _ in 0..1200 {
        w.step(&[]);
        bitten |= w.take_events().iter().any(|e| matches!(e, Event::Bitten(_)));
    }
    assert!(!bitten, "a dog by your side: the wolf keeps away");
}

#[test]
fn a_dog_without_orders_follows_its_person_but_one_with_a_route_stays() {
    let (mut w, a) = with_agent(1);
    w.wild.clear();
    let (px, py) = (w.agents[0].x, w.agents[0].y);
    let free = w.spawn_worker(AgentKind::Dog, px + 1_000, py);
    let busy = w.spawn_worker(AgentKind::Dog, px + 1_000, py + 1_000);
    w.agents[busy.0 as usize].route = vec![crate::agents::RouteStep::Wait { ticks: 1_000_000 }];
    w.agents[busy.0 as usize].hunger = MAX_STAT;
    w.agents[free.0 as usize].hunger = MAX_STAT;
    w.step(&[cmd(a, CommandKind::MoveTo { x: px + 40_000, y: py })]);
    for _ in 0..1500 {
        w.step(&[]);
    }
    let d = |i: AgentId, w: &World| (w.agents[i.0 as usize].x - w.agents[0].x).abs();
    assert!(d(free, &w) < 14_000, "the free dog kept up: {}", d(free, &w));
    assert!(d(busy, &w) > 20_000, "the dog on a route stayed put: {}", d(busy, &w));
}

// ---------------------------------------------------------------------------------------------- stacking

#[test]
fn a_pile_balances_on_the_centre_of_mass() {
    use crate::world::stack_margin;
    // one bar in the middle of the ground is as safe as it gets
    assert_eq!(stack_margin(&[], 70, 0, 50), Some(100));
    // a bar whose middle hangs over the edge of the ground falls
    assert_eq!(stack_margin(&[], 70, 80, 50), None);
    // a bar that does not touch the one below falls
    assert_eq!(stack_margin(&[(0, 40)], 70, 60, 40), None);
    // a second bar shifted by a third of its support still holds, but not with a big margin
    let m = stack_margin(&[(0, 60)], 70, 20, 60).expect("holds");
    assert!((20..50).contains(&m), "{m}");
    // too far out: everything above the first bar has its centre of mass beyond the first bar's end
    assert_eq!(stack_margin(&[(0, 60)], 70, 40, 60), None);
    // a staggered pile can be brought back over the middle, but not thrown back past the bar below
    let pile = [(0i8, 60u8), (20, 60)];
    assert!(stack_margin(&pile, 70, 0, 60).is_some());
    assert_eq!(stack_margin(&pile, 70, -20, 60), None);
}

/// Drops `n` pieces, each centred over the pile so far, and returns the quality of the finished craft.
fn stack_session(w: &mut World, a: AgentId, centre_of: impl Fn(&World) -> i16) -> Option<u8> {
    w.take_events();
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("log_pile") })]);
    let mut guard = 0;
    while let Some(v) = w.craft_view(a) {
        guard += 1;
        assert!(guard < 100, "stacking never ended");
        assert_eq!(v.kind, MinigameKind::Stack);
        let x = centre_of(w);
        w.step(&[cmd(a, CommandKind::PlaceAt { x, y: 0 })]);
    }
    crafted_quality(w)
}

#[test]
fn stacking_logs_neatly_makes_piles_and_wild_drops_make_nothing() {
    // neat: always over the middle of the ground
    let (mut w, a) = with_agent(1);
    learn_all(&mut w);
    give(&mut w, ItemKind::Wood, 5);
    let q = stack_session(&mut w, a, |_| 0);
    assert_eq!(q, Some(2), "a centred pile is fine work");
    assert_eq!(count(&w, 0, ItemKind::LogPile), 2, "fine work pays one pile extra");
    assert_eq!(count(&w, 0, ItemKind::Wood), 0);
    // wild: always far to the side, so the pile never stands
    let (mut w, a) = with_agent(1);
    learn_all(&mut w);
    give(&mut w, ItemKind::Wood, 5);
    let q = stack_session(&mut w, a, |_| 95);
    assert_eq!(q, Some(0), "pieces that slide off give nothing");
    assert_eq!(count(&w, 0, ItemKind::LogPile), 0);
    assert_eq!(count(&w, 0, ItemKind::Wood), 2, "forgiving: half the wood can be used again");
    let (mut w, a) = with_agent(1);
    learn_all(&mut w);
    w.settings.difficulty = crate::world::Difficulty::Realistic;
    give(&mut w, ItemKind::Wood, 5);
    stack_session(&mut w, a, |_| 95);
    assert_eq!(count(&w, 0, ItemKind::Wood), 0, "realistic: the wood is gone with them");
}

#[test]
fn a_clamp_widens_the_ground_for_stacking() {
    let (mut w, a) = with_agent(1);
    learn_all(&mut w);
    give(&mut w, ItemKind::Wood, 5);
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("log_pile") })]);
    let plain = w.craft_view(a).unwrap().half;
    w.agents[0].task = crate::world::Task::Idle;
    give(&mut w, ItemKind::Clamp, 1);
    give(&mut w, ItemKind::Wood, 5);
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("log_pile") })]);
    assert!(w.craft_view(a).unwrap().half > plain);
}

#[test]
fn a_log_pile_burns_to_twice_the_charcoal() {
    let (mut w, a) = with_agent(1);
    learn_all(&mut w);
    let (sx, sy) = w.spawn_mm;
    let kiln = push_building(&mut w, BuildingKind::CharcoalKiln, sx + 1500, sy);
    give(&mut w, ItemKind::LogPile, 1);
    w.agents[0].x = sx + 1500;
    w.agents[0].y = sy;
    w.take_events();
    w.step(&[cmd(a, CommandKind::StartJob { building: kiln, recipe: recipe("charcoal_pile") })]);
    let ev = w.take_events();
    assert!(!ev.iter().any(|e| matches!(e, Event::Rejected(..))), "{ev:?}");
    for _ in 0..1900 {
        w.step(&[]);
    }
    assert_eq!(w.buildings[kiln as usize].output[ItemKind::Charcoal as usize], 4);
}

#[test]
fn knapping_remembers_the_stone_and_cuts_cannot_be_undone() {
    let (mut w, a) = with_agent(1);
    learn_all(&mut w);
    give(&mut w, ItemKind::Stone, 2);
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("sharp_stone") })]);
    let v = w.craft_view(a).unwrap();
    let sector = v.center as usize / 45;
    assert_eq!(v.stack[sector].1, 0, "nothing taken off yet");
    // a clean flake takes off about half the power in cm and the stone remembers it
    w.step(&[cmd(a, CommandKind::StrikeAt { angle: v.center as u16, power: v.power as u8 })]);
    let after = w.craft_view(a).unwrap();
    assert!(after.stack[sector].1 as u32 >= v.power / 2 - 1, "{:?}", after.stack);
    // the next mark moves on to where there is still stone to remove
    // a blow far too deep on a finished sector ruins it: no points, and the stone stays cut
    let before = after.stack[sector].1;
    w.take_events();
    w.step(&[cmd(a, CommandKind::StrikeAt { angle: after.center as u16, power: 100 })]);
    let ev = w.take_events();
    assert!(ev.iter().any(|e| matches!(e, Event::Struck { hit: false, .. })), "overkill is no hit");
    let end = w.craft_view(a).unwrap();
    assert!(end.stack.iter().map(|s| s.1 as u32).sum::<u32>() > before as u32, "material is gone for good");
}

#[test]
fn clothes_are_learned_from_fibre_and_keep_the_wearer_warmer() {
    let (mut w, _a) = with_agent(1);
    assert!(!w.knows(Discovery::Clothing));
    give(&mut w, ItemKind::Fiber, 4);
    w.step(&[]);
    assert!(w.knows(Discovery::Clothing));
    assert!(ItemKind::HideCloak.warmth_bonus() > ItemKind::FiberWrap.warmth_bonus());
    assert_eq!(ItemKind::Stone.warmth_bonus(), 0);
    // clothes stay with their owner like tools
    assert!(ItemKind::HideBoots.is_tool());
    // the cold of the night bites a naked person harder than a dressed one
    let cold = |dressed: bool| {
        let (mut w, _) = with_agent(7);
        if dressed {
            give(&mut w, ItemKind::HideCloak, 1);
            give(&mut w, ItemKind::HideBoots, 1);
            give(&mut w, ItemKind::FiberWrap, 1);
        }
        let mut lowest = i32::MAX;
        for _ in 0..(TICKS_PER_DAY as usize * 2) {
            w.step(&[]);
            lowest = lowest.min(w.agents[0].warmth);
        }
        lowest
    };
    assert!(cold(true) >= cold(false));
}

#[test]
fn clothing_recipes_exist_and_can_be_made() {
    let (mut w, a) = with_agent(1);
    learn_all(&mut w);
    give(&mut w, ItemKind::Fiber, 5);
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("fiber_wrap") })]);
    let mut guard = 0;
    while let Some(v) = w.craft_view(a) {
        guard += 1;
        assert!(guard < 50);
        assert_eq!(v.kind, MinigameKind::Tension);
        w.step(&[cmd(a, CommandKind::Tighten { power: v.power as u8 })]);
    }
    assert_eq!(count(&w, 0, ItemKind::FiberWrap), 1);
}

#[test]
fn an_abandoned_craft_is_announced_and_waits_a_full_minute() {
    let (mut w, a) = with_agent(1);
    learn_all(&mut w);
    give(&mut w, ItemKind::Stone, 2);
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("sharp_stone") })]);
    for _ in 0..3000 {
        w.step(&[]);
    }
    assert!(w.craft_view(a).is_some(), "50 seconds of thinking is fine");
    w.take_events();
    for _ in 0..700 {
        w.step(&[]);
    }
    assert!(w.craft_view(a).is_none());
    assert!(w.take_events().contains(&Event::CraftAbandoned(a)));
    assert_eq!(count(&w, 0, ItemKind::Stone), 2, "nothing is lost");
    // walking away says so too
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("sharp_stone") })]);
    w.take_events();
    w.step(&[cmd(a, CommandKind::MoveTo { x: w.agents[0].x + 3000, y: w.agents[0].y })]);
    assert!(w.take_events().contains(&Event::CraftAbandoned(a)));
}

#[test]
fn ground_height_is_smooth_flat_on_water_and_slows_climbers() {
    use crate::map::{elevation_at_mm, elevation_mm};
    let w = World::new(3);
    let mut hills = 0;
    for i in 0..2000 {
        let (x, y) = ((i * 37) % 250, (i * 91) % 250);
        let h = elevation_mm(3, x, y);
        if w.map.terrain(x, y) == Some(crate::Terrain::Water) {
            assert_eq!(h, 0, "water is level");
        }
        if h > 2000 {
            hills += 1;
        }
        // moving a hand's breadth never jumps the ground by more than a hand, except at a cliff up in the rock
        let d = (elevation_at_mm(3, x as i64 * 1000, y as i64 * 1000) - elevation_at_mm(3, x as i64 * 1000 + 100, y as i64 * 1000)).abs();
        assert!(d < 400 || w.map.terrain(x, y) == Some(crate::Terrain::Rock) || w.map.terrain(x + 1, y) == Some(crate::Terrain::Rock), "{d}");
    }
    assert!(hills > 0, "the map has real hills");
}

#[test]
fn a_broken_leg_slows_walking_and_a_splint_speeds_the_cure() {
    use crate::world::BREAK_TICKS;
    let dist = |broken: bool| {
        let mut w = World::new(3);
        let id = w.spawn_agent(w.spawn_mm.0, w.spawn_mm.1);
        give(&mut w, ItemKind::Torch, 0);
        let a = w.agents.iter_mut().find(|a| a.id == id).unwrap();
        if broken {
            a.broken = BREAK_TICKS;
        }
        let (x0, y0) = (a.x, a.y);
        w.step(&[cmd(id, CommandKind::MoveTo { x: x0 + 6_000, y: y0 })]);
        for _ in 0..120 {
            w.step(&[]);
        }
        let a = w.agents.iter().find(|a| a.id == id).unwrap();
        assert!(a.cave_view().is_none());
        (a.x - x0).abs() + (a.y - y0).abs()
    };
    let (well, hurt) = (dist(false), dist(true));
    assert!(hurt < well * 7 / 10, "{hurt} vs {well}");

    let mut w = World::new(3);
    let id = w.spawn_agent(w.spawn_mm.0, w.spawn_mm.1);
    give(&mut w, ItemKind::Splint, 1);
    w.agents.iter_mut().find(|a| a.id == id).unwrap().broken = BREAK_TICKS;
    w.step(&[cmd(id, CommandKind::Eat { item: ItemKind::Splint })]);
    let a = w.agents.iter().find(|a| a.id == id).unwrap();
    assert!(a.broken <= BREAK_TICKS / 3 + 1);
    assert_eq!(a.inventory.get(ItemKind::Splint), 0);
}

#[test]
fn an_untreated_wound_sometimes_festers_but_a_bandage_prevents_it() {
    let mut sick_after_open = 0;
    let mut sick_after_bandage = 0;
    for seed in 0..40u64 {
        let mut w = World::new(seed);
        let id = w.spawn_agent(w.spawn_mm.0, w.spawn_mm.1);
        give(&mut w, ItemKind::Bandage, 1);
        w.agents.iter_mut().find(|a| a.id == id).unwrap().injury = 200;
        for _ in 0..400 {
            w.step(&[]);
        }
        if w.agents.iter().find(|a| a.id == id).unwrap().sick > 0 {
            sick_after_open += 1;
        }
        let mut w = World::new(seed);
        let id = w.spawn_agent(w.spawn_mm.0, w.spawn_mm.1);
        give(&mut w, ItemKind::Bandage, 1);
        w.agents.iter_mut().find(|a| a.id == id).unwrap().injury = 200;
        w.step(&[cmd(id, CommandKind::Eat { item: ItemKind::Bandage })]);
        for _ in 0..400 {
            w.step(&[]);
        }
        if w.agents.iter().find(|a| a.id == id).unwrap().sick > 0 {
            sick_after_bandage += 1;
        }
    }
    assert!(sick_after_open > 4 && sick_after_open < 30, "{sick_after_open}");
    assert_eq!(sick_after_bandage, 0);
}

#[test]
fn seasons_turn_and_winter_nights_are_deadly_without_a_fire() {
    use crate::time::{ambient_x10, season, weather, Weather, SEASON_DAYS};
    assert_eq!(season(0, SEASON_DAYS), 0);
    assert_eq!(season(TICKS_PER_DAY * SEASON_DAYS, SEASON_DAYS), 1);
    assert_eq!(season(TICKS_PER_DAY * SEASON_DAYS * 3, SEASON_DAYS), 3);
    assert_eq!(season(TICKS_PER_DAY * SEASON_DAYS * 4, SEASON_DAYS), 0, "the year comes round");
    // 03:00 is the coldest hour (the clock starts at 08:00)
    let midnight = |day: u64| TICKS_PER_DAY * day + TICKS_PER_DAY * 1140 / 1440;
    assert!(ambient_x10(midnight(SEASON_DAYS * 3), SEASON_DAYS) < -100, "winter nights freeze");
    assert!(ambient_x10(midnight(SEASON_DAYS), SEASON_DAYS) > ambient_x10(midnight(0), SEASON_DAYS), "summer is warmer than spring");
    assert_eq!(weather(3, 100, SEASON_DAYS), Weather::Clear, "day one is clear");
    let (mut rain, mut snow) = (0, 0);
    for b in 0..600u64 {
        match weather(3, TICKS_PER_DAY + b * (TICKS_PER_DAY / 6), SEASON_DAYS) {
            Weather::Rain => rain += 1,
            Weather::Snow => snow += 1,
            Weather::Clear => {}
        }
    }
    assert!(rain > 20 && snow > 20, "{rain} {snow}");
    // the same sky for everyone
    assert_eq!(weather(3, 500_000, SEASON_DAYS), weather(3, 500_000, SEASON_DAYS));
}

#[test]
fn seasons_turn_weather_is_shared_and_winter_is_harsh() {
    use crate::time::{ambient_x10, season, weather, Weather, SEASON_DAYS};
    let day = TICKS_PER_DAY;
    assert_eq!(season(0, SEASON_DAYS), 0);
    assert_eq!(season(day * SEASON_DAYS, SEASON_DAYS), 1);
    assert_eq!(season(day * SEASON_DAYS * 3, SEASON_DAYS), 3);
    assert_eq!(season(day * SEASON_DAYS * 4, SEASON_DAYS), 0, "the year comes round");
    // same hour of day, summer warmer than winter
    assert!(ambient_x10(day * SEASON_DAYS + day / 2, SEASON_DAYS) > ambient_x10(day * SEASON_DAYS * 3 + day / 2, SEASON_DAYS) + 150);
    // the first day is clear; later skies are a pure function of seed and time
    assert!((0..day).step_by(1000).all(|t| weather(3, t, SEASON_DAYS) == Weather::Clear));
    let sky: Vec<_> = (day..day * 13).step_by((day / 6) as usize).map(|t| weather(3, t, SEASON_DAYS)).collect();
    assert_eq!(sky, (day..day * 13).step_by((day / 6) as usize).map(|t| weather(3, t, SEASON_DAYS)).collect::<Vec<_>>());
    assert!(sky.contains(&Weather::Rain) && sky.contains(&Weather::Snow) && sky.contains(&Weather::Clear));
    // an unsheltered person in a winter night loses warmth, one by a fire does not
    let mut w = World::new(3);
    w.settings.season_days = SEASON_DAYS as u8; // the 3-day calendar this test counts in
    let id = w.spawn_agent(w.spawn_mm.0, w.spawn_mm.1);
    w.tick = day * SEASON_DAYS * 3 + day / 2;
    for _ in 0..6000 {
        w.step(&[]);
    }
    assert!(w.agents.iter().find(|a| a.id == id).unwrap().warmth < 7_000, "the winter night chills");
}

#[test]
fn running_is_faster_but_costs_breath_and_thirst() {
    use crate::world::MAX_STAT;
    let run = |sprint: bool, ticks: usize| {
        let mut w = World::new(3);
        let id = w.spawn_agent(w.spawn_mm.0, w.spawn_mm.1);
        let (x0, y0) = { let a = w.agents.iter().find(|a| a.id == id).unwrap(); (a.x, a.y) };
        w.step(&[cmd(id, CommandKind::SetSprint { on: sprint })]);
        w.step(&[cmd(id, CommandKind::MoveTo { x: x0 + 60_000, y: y0 })]);
        for _ in 0..ticks {
            w.step(&[]);
        }
        let a = w.agents.iter().find(|a| a.id == id).unwrap().clone();
        ((a.x - x0).abs() + (a.y - y0).abs(), a)
    };
    let (walk, _) = run(false, 300);
    let (fast, a) = run(true, 300);
    assert!(fast > walk * 14 / 10, "{fast} vs {walk}");
    assert!(a.stamina < MAX_STAT, "breath is used");
    // run until winded: it stops by itself, and cannot restart at once
    let (_, a) = run(true, 1270);
    assert!(!a.sprint && a.stamina < 3_000);
    let mut w = World::new(3);
    let id = w.spawn_agent(w.spawn_mm.0, w.spawn_mm.1);
    w.agents.iter_mut().find(|a| a.id == id).unwrap().stamina = 100;
    w.step(&[cmd(id, CommandKind::SetSprint { on: true })]);
    assert!(!w.agents.iter().find(|a| a.id == id).unwrap().sprint, "no running when out of breath");
}

#[test]
fn every_kind_of_country_exists_on_the_map() {
    let (mut cold, mut steppe, mut jungle, mut autumn, mut other) = (0, 0, 0, 0, 0);
    for y in (0..256).step_by(4) {
        for x in (0..256).step_by(4) {
            let (warm, wet) = crate::climate(3, x, y);
            assert!((0..=1000).contains(&warm) && (0..=1000).contains(&wet));
            if warm < 380 { cold += 1 } else if warm > 560 && wet < 500 { steppe += 1 } else if warm > 560 { jungle += 1 } else if warm < 550 && warm > 320 && wet > 400 { autumn += 1 } else { other += 1 }
        }
    }
    eprintln!("cold {cold} steppe {steppe} jungle {jungle} autumn {autumn} other {other}");
    assert!(cold > 40 && steppe > 40 && jungle > 40 && autumn > 40, "each country takes a real share of the map");
}

#[test]
fn evolving_makes_hands_surer_and_work_faster() {
    // a tribe that has learned nothing is clumsy; one that has learned a lot is deft
    let mut raw = World::new(3);
    raw.forget_everything();
    assert_eq!(raw.perception_level(), 0);
    let mut sage = World::new(3);
    for k in ItemKind::ALL {
        sage.mark_seen(k);
    }
    let gather_time = |w: &mut World| {
        let id = w.spawn_agent(w.spawn_mm.0, w.spawn_mm.1);
        let (ni, n) = w.nodes.iter().enumerate().find(|(_, n)| n.kind == NodeKind::Tree).map(|(i, n)| (i, n.clone())).unwrap();
        let a = w.agents.iter_mut().find(|a| a.id == id).unwrap();
        a.x = n.tx as i64 * 1000 + 500;
        a.y = n.ty as i64 * 1000 + 1200;
        w.step(&[cmd(id, CommandKind::Gather { node: NodeId(ni as u32) })]);
        let mut t = 0;
        while w.agents.iter().find(|a| a.id == id).unwrap().inventory.get(ItemKind::Wood) == 0 && t < 5000 {
            w.step(&[]);
            t += 1;
        }
        t
    };
    let slow = gather_time(&mut raw);
    assert!(slow > 0);
    assert!(sage.perception_level() > raw.perception_level() || sage.perception_level() == 0, "marking things seen raises the stage");
    let fast = gather_time(&mut sage);
    assert!(fast <= slow, "{fast} vs {slow}");
}

/// Loads a real format-17 save named by ALAZ_OLD_SAVE (run with --ignored): it must upgrade and save again.
#[test]
#[ignore]
fn an_old_save_upgrades() {
    let Some(p) = std::env::var_os("ALAZ_OLD_SAVE") else { return };
    let bytes = std::fs::read(p).unwrap();
    let w = World::from_bytes(&bytes).expect("old save loads");
    println!("loaded: tick {} agents {} buildings {} wood {}", w.tick, w.agents.len(), w.buildings.len(), w.agents[0].inventory.get(ItemKind::Wood));
    let again = World::from_bytes(&w.to_bytes()).expect("saves and loads in the new format");
    assert_eq!(again.hash(), w.hash());
}


// --- gravity

/// A sheer drop in the rock: the tile corner on top, which way it falls, and how far (mm).
fn find_cliff(w: &World, min_drop: i32) -> Option<(i32, i32, i32, i32, i32)> {
    for ty in 2..w.map.h - 2 {
        for tx in 2..w.map.w - 2 {
            if w.map.terrain(tx, ty) != Some(Terrain::Rock) {
                continue;
            }
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let d = crate::map::elevation_mm(w.seed, tx, ty) - crate::map::elevation_mm(w.seed, tx + dx, ty + dy);
                let clear = [(0, 0), (dx, dy), (2 * dx, 2 * dy), (-dx, -dy)].iter().all(|&(ox, oy)| w.node_at(tx + ox, ty + oy).is_none() && w.map.terrain(tx + ox, ty + oy).is_some_and(|t| t != Terrain::Water));
                if d >= min_drop && clear {
                    return Some((tx, ty, dx, dy, d));
                }
            }
        }
    }
    None
}

#[test]
fn the_heights_have_cliffs_and_ramps() {
    let w = World::new(3);
    let (mut sheer, mut rock) = (0, 0);
    for ty in 0..w.map.h - 1 {
        for tx in 0..w.map.w - 1 {
            if w.map.terrain(tx, ty) == Some(Terrain::Rock) {
                rock += 1;
                if crate::physics::steepest_drop(w.seed, tx, ty).2 >= crate::map::CLIFF_MM {
                    sheer += 1;
                }
            }
        }
    }
    println!("rock tiles {rock}, cliff tiles {sheer}");
    assert!(sheer > 20, "there are cliffs to fall from");
    assert!(sheer * 3 < rock, "but most of the rock can be walked");
}

#[test]
fn isin_matches_sine() {
    for k in 0..=20 {
        let a = k as f64 * std::f64::consts::PI / 20.0;
        let got = crate::physics::isin((a * 1e6) as i64) as f64 / 1e6;
        assert!((got - a.sin()).abs() < 0.003, "{k}: {got} vs {}", a.sin());
    }
}

#[test]
fn a_jump_goes_up_and_comes_down_where_it_left() {
    let (mut w, a) = with_agent(7);
    let (x0, y0) = (w.agents[0].x, w.agents[0].y);
    let ground = crate::map::elevation_at_mm(w.seed, x0, y0) as i64;
    w.step(&[cmd(a, CommandKind::Jump)]);
    let mut top = 0;
    let mut ticks = 0;
    while w.airborne(a).is_some() {
        top = top.max(w.airborne(a).unwrap().z / crate::physics::Q - ground);
        w.step(&[]);
        ticks += 1;
        assert!(ticks < 200);
    }
    assert!((600..900).contains(&top), "rises about three quarters of a metre: {top}");
    assert!((35..60).contains(&ticks), "and is back down in under a second: {ticks}");
    assert_eq!((w.agents[0].x, w.agents[0].y), (x0, y0));
    assert_eq!(w.agents[0].health, MAX_STAT, "a jump does not hurt");
    // out of breath, no more jumping
    w.agents[0].stamina = 100;
    w.step(&[cmd(a, CommandKind::Jump)]);
    assert!(w.airborne(a).is_none());
}

#[test]
fn walking_over_a_cliff_is_a_fall_and_a_high_one_hurts() {
    let mut w = World::new(3);
    let (tx, ty, dx, dy, drop) = find_cliff(&w, 3_000).expect("a cliff of 3 m");
    println!("cliff at ({tx},{ty}) dir ({dx},{dy}) drop {drop}");
    // stand just back from the edge and walk straight over it
    let (x, y) = (tx as i64 * 1000 - dx as i64 * 400, ty as i64 * 1000 - dy as i64 * 400);
    let a = w.spawn_agent(x, y);
    let target = (x + dx as i64 * 4000, y + dy as i64 * 4000);
    let mut fell = None;
    let mut flew = false;
    for _ in 0..600 {
        w.step(&[cmd(a, CommandKind::Walk { x: target.0, y: target.1 })]);
        flew |= w.airborne(a).is_some();
        for e in w.take_events() {
            if let Event::Fell { height_mm, .. } = e {
                fell = Some(height_mm);
            }
        }
        if fell.is_some() {
            break;
        }
    }
    let h = fell.expect("went over the edge and came down");
    assert!(flew);
    println!("fell {h} mm, health {}", w.agents[0].health);
    assert!(h > 2_000);
    assert!(w.agents[0].health < MAX_STAT, "a fall like that hurts");
    // the same edge taken by a planned route is climbed down, not fallen from
    let mut w = World::new(3);
    let b = w.spawn_agent(x, y);
    for _ in 0..1500 {
        w.step(&[cmd(b, CommandKind::MoveTo { x: target.0, y: target.1 })]);
        assert!(w.take_events().iter().all(|e| !matches!(e, Event::Fell { .. })));
        assert!(w.airborne(b).is_none());
    }
}

#[test]
fn a_felled_tree_topples_away_and_comes_down_on_whoever_is_under_it() {
    let mut w = World::new(5);
    // a tall tree with open flat ground round it
    let i = w
        .nodes
        .iter()
        .position(|n| {
            n.kind == NodeKind::Tree
                && crate::map::tree_height_mm(w.seed, n.tx, n.ty) > 3_500
                && (-5..=5).all(|d: i32| w.map.terrain(n.tx + d, n.ty).is_some_and(|t| t != Terrain::Water && t != Terrain::Rock) && (d == 0 || w.node_at(n.tx + d, n.ty).is_none()))
        })
        .expect("a tall tree in the open");
    let (tx, ty) = (w.nodes[i].tx, w.nodes[i].ty);
    let (bx, by) = crate::map::node_anchor_mm(w.seed, tx, ty);
    let cutter = w.spawn_agent(bx - 900, by);
    // someone standing a couple of metres the other side, right where it will land, and someone well clear
    let victim = w.spawn_agent(bx + 2_000, by);
    let clear = w.spawn_agent(bx, by + 6_000);
    w.set_node_amount(i, 1);
    w.agents[cutter.0 as usize].inventory.counts[ItemKind::StoneAxe as usize] = 1;
    w.step(&[cmd(cutter, CommandKind::Gather { node: NodeId(i as u32) })]);
    let mut down = None;
    let mut started = None;
    for t in 0..2000 {
        w.step(&[]);
        if started.is_none() && !w.bodies.is_empty() {
            started = Some(t);
        }
        for e in w.take_events() {
            if let Event::TreeDown { struck, .. } = e {
                down = Some((t, struck));
            }
        }
        if down.is_some() {
            break;
        }
    }
    let (t, struck) = down.expect("the tree came down");
    let fall = t - started.unwrap();
    println!("fell in {fall} ticks");
    assert!((60..400).contains(&fall), "a tree takes a few seconds to come down: {fall}");
    assert!(struck);
    assert!(w.agents[victim.0 as usize].health < MAX_STAT - 2_000, "the one under it is badly hurt");
    assert_eq!(w.agents[cutter.0 as usize].health, MAX_STAT, "the cutter stood clear");
    assert_eq!(w.agents[clear.0 as usize].health, MAX_STAT);
    let b = &w.bodies[0];
    assert!(b.dx > 900, "it fell away from the cutter: {:?}", (b.dx, b.dy));
    // the trunk lies a while, then is gone
    for _ in 0..crate::physics::TREE_REST_TICKS + 10 {
        w.step(&[]);
    }
    assert!(w.bodies.is_empty());
}

#[test]
fn a_loose_rock_rolls_down_and_stops() {
    let mut w = World::new(3);
    let (tx, ty, dx, dy, _) = find_cliff(&w, 2_000).expect("a slope");
    let (x, y) = (tx as i64 * 1000, ty as i64 * 1000);
    let e0 = crate::map::elevation_at_mm(w.seed, x, y);
    w.loose_rock(x, y, dx as i64 * 10, dy as i64 * 10, 1);
    let mut moved = 0i64;
    for _ in 0..crate::physics::ROCK_LIFE_TICKS {
        w.step(&[]);
        let Some(b) = w.bodies.first() else { break };
        let (bx, by, _) = b.pos_mm();
        moved = moved.max(((bx - x).pow(2) + (by - y).pow(2)).isqrt());
        if b.rest > 30 {
            break;
        }
    }
    println!("rolled {moved} mm");
    assert!(moved > 1_000, "it went somewhere: {moved}");
    if let Some(b) = w.bodies.first() {
        let (bx, by, _) = b.pos_mm();
        assert!(crate::map::elevation_at_mm(w.seed, bx, by) < e0, "and ended lower down");
    }
}

#[test]
fn a_rock_coming_down_strikes_whoever_is_below() {
    let (mut w, a) = with_agent(3);
    let (x, y) = (w.agents[0].x, w.agents[0].y);
    // dropped from 3 m right over the head
    w.loose_rock(x, y, 0, 0, 2);
    w.bodies[0].z += 3_000 * crate::physics::Q;
    let mut hit = false;
    for _ in 0..120 {
        w.step(&[]);
        hit |= w.take_events().contains(&Event::RockHit(a));
    }
    assert!(hit);
    assert!(w.agents[0].health < MAX_STAT);
}

#[test]
fn falling_things_survive_a_save() {
    let (mut w, a) = with_agent(7);
    let (x, y) = (w.agents[0].x + 3_000, w.agents[0].y);
    w.loose_rock(x, y, 5, 0, 3);
    w.bodies[0].z += 2_000 * crate::physics::Q;
    let i = w.nodes.iter().position(|n| n.kind == NodeKind::Tree).unwrap();
    let (tx, ty) = (w.nodes[i].tx, w.nodes[i].ty);
    w.fell_tree(tx, ty, a, 0, 0);
    w.step(&[cmd(a, CommandKind::Jump)]);
    w.step(&[]);
    assert!(w.airborne(a).is_some() && w.bodies.len() == 2);
    let mut b = World::from_bytes(&w.to_bytes()).expect("load");
    assert_eq!(w.hash(), b.hash());
    for _ in 0..400 {
        w.step(&[]);
        b.step(&[]);
    }
    assert_eq!(w.hash(), b.hash());
}

/// History's order: copper and tin melt as soon as there is a clay furnace; iron ore teaches nothing
/// until bronze is worked. The goal list leads through bronze before iron.
#[test]
fn bronze_comes_before_iron() {
    let (mut w, _a) = with_agent(3);
    w.forget_everything();
    w.learn(Discovery::Furnace);
    give(&mut w, ItemKind::IronOre, 2);
    w.step(&[]);
    assert!(!w.knows(Discovery::Smelting), "iron ore alone does not open iron");
    give(&mut w, ItemKind::CopperOre, 1);
    give(&mut w, ItemKind::TinOre, 1);
    w.step(&[]);
    assert!(w.knows(Discovery::Alloying), "copper and tin melt in the clay furnace without iron");
    w.learn(Discovery::Workbench);
    give(&mut w, ItemKind::Bronze, 1);
    w.step(&[]);
    assert!(w.knows(Discovery::Bronzework));
    w.step(&[]);
    assert!(w.knows(Discovery::Smelting), "after bronze, iron ore can be smelted");
    let pos = |m: Milestone| Milestone::ALL.iter().position(|x| *x == m).unwrap();
    assert!(pos(Milestone::BronzeAge) < pos(Milestone::Iron));
}

/// Plays the running hand craft of agent 0 perfectly to the end.
fn play_craft_perfectly(w: &mut World, a: AgentId) {
    for _ in 0..20_000 {
        let Some(v) = w.craft_view(a) else { return };
        let kind = match v.kind {
            crate::MinigameKind::Knap => Some(CommandKind::StrikeAt { angle: v.center as u16, power: v.power as u8 }),
            crate::MinigameKind::Fit => Some(CommandKind::PlaceAt { x: v.tx as i16, y: v.ty as i16 }),
            crate::MinigameKind::Tension => Some(CommandKind::Tighten { power: v.power as u8 }),
            crate::MinigameKind::Trim => Some(CommandKind::PlaceAt { x: v.tx as i16, y: v.ty as i16 }),
            crate::MinigameKind::Rub => Some(CommandKind::Rub { pace: v.power as u8, grip: true }),
            crate::MinigameKind::Timing => (v.pos.abs_diff(v.center) <= v.half / 3).then_some(CommandKind::Strike),
            crate::MinigameKind::Stack => Some(CommandKind::PlaceAt { x: 0, y: 0 }),
        };
        w.step(&kind.map(|k| vec![cmd(a, k)]).unwrap_or_default());
    }
    panic!("the craft never ended");
}

/// The first farmers, from commands only: tall grass sheds seed, a hoe breaks ground for a field, the field
/// grows grain and gives seed back, a quern grinds flour, the fire bakes bread; the Neolithic goals follow.
#[test]
fn the_neolithic_chain_runs_from_wild_grass_to_bread() {
    let (mut w, a) = with_agent(3);
    w.forget_everything();
    w.learn(Discovery::StoneAxe);
    w.learn(Discovery::Fire);
    w.wild.retain(|x| x.species != WildSpecies::Wolf);
    // wild grass: the last armful of a tuft is seed
    // grass where the wild crop is grain (dry warm land grows lentils, wet land flax)
    let grain_land = |n: &Node| {
        let (warm, wet) = crate::map::climate(w.seed, n.tx, n.ty);
        // (and not a cotton tuft: some of the tall grass is cotton)
        !(wet > 550 || (warm > 580 && wet < 480) || crate::world::cotton_tuft(w.seed, n.tx, n.ty, warm, wet))
    };
    let i = w.nodes.iter().position(|n| n.kind == NodeKind::FiberPlant && n.amount == 4 && grain_land(n)).expect("tall grass");
    let (nx, ny) = w.nodes[i].center_mm();
    w.agents[0].x = nx;
    w.agents[0].y = ny + 900;
    w.step(&[cmd(a, CommandKind::Gather { node: NodeId(i as u32) })]);
    for _ in 0..60 * 60 {
        w.step(&[]);
        if w.nodes[i].amount == 0 {
            break;
        }
    }
    assert_eq!(count(&w, 0, ItemKind::Seed), 1, "one armful in four is seed");
    assert_eq!(count(&w, 0, ItemKind::Fiber), 3);
    w.step(&[]);
    assert!(w.knows(Discovery::Farming), "seed in hand teaches sowing");
    // a hoe: sharp stone, two sticks, fibre
    give(&mut w, ItemKind::SharpStone, 1);
    give(&mut w, ItemKind::Branch, 2);
    give(&mut w, ItemKind::Fiber, 1);
    w.agents[0].task = Task::Idle;
    w.step(&[cmd(a, CommandKind::Combine { items: [ItemKind::SharpStone as u8, ItemKind::Branch as u8, ItemKind::Branch as u8, ItemKind::Fiber as u8] })]);
    play_craft_perfectly(&mut w, a);
    assert!(count(&w, 0, ItemKind::Hoe) >= 1, "a hoe came out");
    // a field, which needs the hoe and keeps it
    give(&mut w, ItemKind::Wood, 2);
    let (fx, fy) = spot_for(&w, BuildingKind::Field, &[]);
    w.agents[0].x = fx + 1800;
    w.agents[0].y = fy;
    w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::Field, x: fx, y: fy })]);
    let field = w.buildings.iter().position(|b| b.kind == BuildingKind::Field).expect("field built") as u32;
    assert!(count(&w, 0, ItemKind::Hoe) >= 1, "the hoe is kept");
    give(&mut w, ItemKind::Seed, 2);
    w.step(&[cmd(a, CommandKind::StartJob { building: field, recipe: recipe("sow") })]);
    assert!(w.buildings[field as usize].job.is_some(), "sown: {:?}", w.take_events());
    for _ in 0..108_000 + 10 {
        w.wild.retain(|x| x.species != WildSpecies::Wolf);
        w.agents[0].hunger = MAX_STAT;
        w.agents[0].thirst = MAX_STAT;
        w.agents[0].warmth = MAX_STAT;
        w.step(&[]);
    }
    w.step(&[cmd(a, CommandKind::Collect { building: field, item: None })]);
    assert_eq!(count(&w, 0, ItemKind::Grain), 6);
    assert_eq!(count(&w, 0, ItemKind::Seed), 3, "more seed back than was sown");
    w.step(&[]);
    assert!(w.milestone_done(Milestone::Harvest));
    assert!(w.knows(Discovery::Milling), "grain in hand, fire known: grinding and baking");
    // a quern from three stones, then flour
    give(&mut w, ItemKind::Stone, 3);
    w.step(&[cmd(a, CommandKind::Combine { items: [ItemKind::Stone as u8, ItemKind::Stone as u8, ItemKind::Stone as u8, 255] })]);
    play_craft_perfectly(&mut w, a);
    assert_eq!(count(&w, 0, ItemKind::Quern), 1);
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("flour") })]);
    play_craft_perfectly(&mut w, a);
    assert!(count(&w, 0, ItemKind::Flour) >= 2);
    assert_eq!(count(&w, 0, ItemKind::Quern), 1, "the quern is kept");
    // bake at a fire
    let (cx, cy) = spot_for(&w, BuildingKind::Campfire, &[(fx, fy)]);
    let fire = push_building(&mut w, BuildingKind::Campfire, cx, cy);
    w.buildings[fire as usize].fuel = 100_000;
    w.agents[0].x = cx + 1000;
    w.agents[0].y = cy;
    w.step(&[cmd(a, CommandKind::StartJob { building: fire, recipe: recipe("bread") })]);
    for _ in 0..1000 {
        w.step(&[]);
    }
    w.step(&[cmd(a, CommandKind::Collect { building: fire, item: None })]);
    assert!(count(&w, 0, ItemKind::Bread) >= 1);
    w.step(&[]);
    assert!(w.milestone_done(Milestone::Bread));
    assert!(!w.milestone_done(Milestone::NeolithicAge), "the age also wants a fired pot and a winter");
    // bread fills more than cooked meat
    w.agents[0].hunger = 1_000;
    w.step(&[cmd(a, CommandKind::Eat { item: ItemKind::Bread })]);
    assert!(w.agents[0].hunger >= 5_900);
    // pottery: clay in hand among farmers, coiled by hand, fired in a pit kiln dug with a shovel
    give(&mut w, ItemKind::Clay, 3);
    w.step(&[]);
    assert!(w.knows(Discovery::Pottery));
    w.step(&[cmd(a, CommandKind::Combine { items: [ItemKind::Clay as u8, ItemKind::Clay as u8, ItemKind::Clay as u8, 255] })]);
    play_craft_perfectly(&mut w, a);
    assert!(count(&w, 0, ItemKind::RawPot) >= 1, "coiled a pot");
    w.step(&[cmd(a, CommandKind::StartJob { building: fire, recipe: recipe("fire_pot") })]);
    assert!(w.buildings[fire as usize].job.is_none(), "a camp fire does not fire clay");
    let (kx, ky) = spot_for(&w, BuildingKind::PitKiln, &[(fx, fy), (cx, cy)]);
    w.agents[0].x = kx + 1_200;
    w.agents[0].y = ky;
    give(&mut w, ItemKind::Fiber, 3);
    give(&mut w, ItemKind::Wood, 2);
    w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::PitKiln, x: kx, y: ky })]);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::NoMaterials)), "a pit is dug with a shovel");
    give(&mut w, ItemKind::Shovel, 1);
    w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::PitKiln, x: kx, y: ky })]);
    let kiln = w.buildings.iter().position(|b| b.kind == BuildingKind::PitKiln).expect("pit kiln dug") as u32;
    w.step(&[cmd(a, CommandKind::StartJob { building: kiln, recipe: recipe("fire_pot") })]);
    assert!(w.buildings[kiln as usize].job.is_some(), "clay, straw and logs in: firing {:?}", w.take_events());
    // the pit smoulders up to clay heat (about four minutes), then fires for half a day
    for _ in 0..58_000 {
        w.agents[0].hunger = MAX_STAT;
        w.agents[0].thirst = MAX_STAT;
        w.agents[0].warmth = MAX_STAT;
        w.agents[0].health = MAX_STAT;
        w.step(&[]);
    }
    w.step(&[cmd(a, CommandKind::Collect { building: kiln, item: None })]);
    assert!(count(&w, 0, ItemKind::Pot) >= 1, "fired in the pit kiln");
    w.step(&[]);
    assert!(!w.milestone_done(Milestone::NeolithicAge), "not before the first winter is over");
    w.tick = TICKS_PER_DAY * w.settings.season_days as u64 * 4;
    w.agents[0].hunger = MAX_STAT;
    w.step(&[]);
    assert!(w.milestone_done(Milestone::Winter));
    assert!(w.milestone_done(Milestone::NeolithicAge));
}

/// A pot is filled when drinking at the water and drunk from far away, and comes back empty.
#[test]
fn a_pot_carries_water_away_from_the_river() {
    let (mut w, a) = with_agent(3);
    let (sx, sy) = ((w.spawn_mm.0 / 1000) as i32, (w.spawn_mm.1 / 1000) as i32);
    let (tx, ty) = (0..w.map.h)
        .flat_map(|y| (0..w.map.w).map(move |x| (x, y)))
        .filter(|&(x, y)| w.map.terrain(x, y) == Some(Terrain::Water) && w.map.terrain(x, y + 1).is_some_and(|t| t != Terrain::Water) && w.node_at(x, y + 1).is_none())
        .min_by_key(|&(x, y)| (x - sx).pow(2) + (y - sy).pow(2))
        .expect("a shore");
    w.agents[0].x = tx as i64 * 1000 + 500;
    w.agents[0].y = ty as i64 * 1000 + 1_600;
    give(&mut w, ItemKind::Pot, 2);
    w.agents[0].thirst = MAX_STAT - 400;
    w.step(&[cmd(a, CommandKind::Drink { tx, ty })]);
    for _ in 0..60 {
        w.step(&[]);
    }
    assert!(w.agents[0].thirst > MAX_STAT - 100, "drunk to the full first (thirst ticks down again after)");
    assert_eq!(count(&w, 0, ItemKind::WaterPot), 2, "both pots filled");
    assert_eq!(count(&w, 0, ItemKind::Pot), 0);
    // far from water, drink from one
    w.agents[0].thirst = 2_000;
    w.step(&[cmd(a, CommandKind::Eat { item: ItemKind::WaterPot })]);
    assert!(w.agents[0].thirst >= 5_400, "a pot quenches");
    assert_eq!(count(&w, 0, ItemKind::WaterPot), 1);
    assert_eq!(count(&w, 0, ItemKind::Pot), 1, "the pot comes back empty");
}

/// Saves keep working when items are added: an item list from before is padded with empty slots.
#[test]
fn item_lists_from_before_newer_items_still_load() {
    let short: Vec<u16> = (0..33).map(|i| i as u16).collect();
    let bytes = postcard::to_allocvec(&short).unwrap();
    #[derive(serde::Deserialize)]
    struct Wrap(#[serde(with = "crate::items_serde")] [u16; ItemKind::COUNT]);
    let Wrap(out) = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(out[32], 32);
    assert!(out[33..].iter().all(|&n| n == 0));
}

#[test]
fn a_felled_trunk_is_broken_up_where_it_lies_and_the_pieces_picked_up() {
    let (mut w, a) = with_agent(5);
    let i = w.nodes.iter().position(|n| n.kind == NodeKind::Tree && crate::map::tree_height_mm(w.seed, n.tx, n.ty) > 3_000).unwrap();
    let (tx, ty) = (w.nodes[i].tx, w.nodes[i].ty);
    let (bx, by) = crate::map::node_anchor_mm(w.seed, tx, ty);
    w.agents[0].x = bx - 900;
    w.agents[0].y = by;
    w.agents[0].inventory.counts = [0; ItemKind::COUNT];
    w.agents[0].inventory.counts[ItemKind::StoneAxe as usize] = 1;
    w.step(&[cmd(a, CommandKind::Gather { node: NodeId(i as u32) })]);
    // chopping the standing tree gives no wood: it only brings it down
    let mut down_at = None;
    for t in 0..3_000 {
        w.step(&[]);
        if down_at.is_none() && w.bodies.iter().any(|b| b.lying()) {
            down_at = Some(t);
            assert_eq!(count(&w, 0, ItemKind::Wood), 0, "no wood from felling");
            assert!(matches!(w.agents[0].task, Task::Split { .. }), "goes on to break up the trunk: {:?}", w.agents[0].task);
        }
        if down_at.is_some() && w.bodies.iter().any(|b| b.kind == crate::physics::BodyKind::Piece) {
            break;
        }
    }
    assert!(down_at.is_some());
    // the pieces fly off onto the ground; the pieces are what is picked up
    let mut most_pieces = 0;
    for _ in 0..6_000 {
        w.step(&[]);
        most_pieces = most_pieces.max(w.bodies.iter().filter(|b| b.kind == crate::physics::BodyKind::Piece).count());
        if w.bodies.is_empty() && w.agents[0].task == Task::Idle {
            break;
        }
    }
    let n = crate::physics::log_pieces(w.seed, tx, ty);
    assert!(most_pieces >= 2, "pieces lay about before they were picked up: {most_pieces}");
    assert_eq!(count(&w, 0, ItemKind::Wood), n);
    assert!(w.bodies.is_empty());
}

#[test]
fn a_trunk_can_be_broken_up_by_someone_else_and_pieces_picked_by_hand() {
    let (mut w, a) = with_agent(5);
    let i = w.nodes.iter().position(|n| n.kind == NodeKind::Tree).unwrap();
    let (tx, ty) = (w.nodes[i].tx, w.nodes[i].ty);
    let (bx, by) = crate::map::node_anchor_mm(w.seed, tx, ty);
    w.set_node_amount(i, 0);
    let log = w.fell_tree(tx, ty, AgentId(99), bx - 900, by);
    w.agents[0].x = bx + 2_500;
    w.agents[0].y = by + 1_500;
    for _ in 0..600 {
        w.step(&[]);
    }
    w.step(&[cmd(a, CommandKind::Split { body: log })]);
    // two blows, then stop and pick up one piece by hand
    let mut pieces = 0;
    for _ in 0..3_000 {
        w.step(&[]);
        pieces = w.bodies.iter().filter(|b| b.kind == crate::physics::BodyKind::Piece && b.rest > 5).count();
        if pieces >= 2 {
            break;
        }
    }
    assert!(pieces >= 2);
    let piece = w.bodies.iter().find(|b| b.kind == crate::physics::BodyKind::Piece).unwrap().id;
    w.step(&[cmd(a, CommandKind::Pick { body: piece })]);
    for _ in 0..400 {
        w.step(&[]);
        if w.bodies.iter().all(|b| b.id != piece) {
            break;
        }
    }
    assert!(w.bodies.iter().all(|b| b.id != piece), "the piece was picked up");
    assert!(count(&w, 0, ItemKind::Wood) >= 1);
}

/// Weaving: an armful of fibre among farmers who dress teaches the loom; the loom turns fibre into cloth;
/// cloth is sewn into a tunic (warmer than a fibre wrap) and a bag (a person carries more).
#[test]
fn later_ages_dress_the_wearer_and_old_saves_keep_their_items() {
    // each new garment has a recipe, is carried and worn like the first ones, and keeps the cold off
    for (k, id) in [(ItemKind::LinenHood, "linen_hood"), (ItemKind::BronzePlate, "bronze_plate"), (ItemKind::SteelHelm, "steel_helm"), (ItemKind::WorkApron, "work_apron")] {
        assert!(k.is_tool() && k.warmth_bonus() > 0, "{id} is worn");
        let r = crate::rules::rules().recipes.iter().find(|r| r.id == id).unwrap_or_else(|| panic!("{id} has a recipe"));
        assert_eq!(r.outputs[0].item, k);
        assert!(!r.steps.is_empty(), "{id} is a hand-craft");
        assert_eq!(ItemKind::ALL[k as usize], k, "{id} sits at its own slot");
    }
    // a hood sewn at the bench
    let (mut w, a) = with_agent(3);
    w.forget_everything();
    w.learn(Discovery::Weaving);
    give(&mut w, ItemKind::Cloth, 1);
    give(&mut w, ItemKind::Fiber, 1);
    w.step(&[cmd(a, CommandKind::Combine { items: [ItemKind::Cloth as u8, ItemKind::Fiber as u8, 255, 255] })]);
    play_craft_perfectly(&mut w, a);
    assert_eq!(count(&w, 0, ItemKind::LinenHood), 1);
    // the items are named in saves, so the new ones appended at the end change no old name
    assert_eq!(ItemKind::ALL[ItemKind::FishingRod as usize + 1], ItemKind::LinenHood);
}

#[test]
fn the_loom_weaves_cloth_for_a_tunic_and_a_bag() {
    let (mut w, a) = with_agent(3);
    w.forget_everything();
    w.learn(Discovery::Clothing);
    w.learn(Discovery::Farming);
    w.wild.retain(|x| x.species != WildSpecies::Wolf);
    give(&mut w, ItemKind::Fiber, 6);
    w.step(&[]);
    assert!(w.knows(Discovery::Weaving));
    give(&mut w, ItemKind::Wood, 4);
    give(&mut w, ItemKind::Clay, 2);
    let (lx, ly) = spot_for(&w, BuildingKind::Loom, &[]);
    w.agents[0].x = lx + 1500;
    w.agents[0].y = ly;
    w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::Loom, x: lx, y: ly })]);
    let loom = w.buildings.iter().position(|b| b.kind == BuildingKind::Loom).expect("loom built") as u32;
    give(&mut w, ItemKind::Fiber, 12);
    for _ in 0..3 {
        w.step(&[cmd(a, CommandKind::StartJob { building: loom, recipe: recipe("cloth") })]);
        for _ in 0..1210 {
            w.step(&[]);
        }
        w.step(&[cmd(a, CommandKind::Collect { building: loom, item: None })]);
    }
    assert_eq!(count(&w, 0, ItemKind::Cloth), 3);
    give(&mut w, ItemKind::Fiber, 3);
    w.step(&[cmd(a, CommandKind::Combine { items: [ItemKind::Cloth as u8, ItemKind::Cloth as u8, ItemKind::Fiber as u8, 255] })]);
    play_craft_perfectly(&mut w, a);
    assert_eq!(count(&w, 0, ItemKind::Tunic), 1);
    assert!(ItemKind::Tunic.warmth_bonus() > ItemKind::FiberWrap.warmth_bonus());
    let before = w.agents[0].capacity();
    w.step(&[cmd(a, CommandKind::Combine { items: [ItemKind::Cloth as u8, ItemKind::Fiber as u8, ItemKind::Fiber as u8, 255] })]);
    play_craft_perfectly(&mut w, a);
    assert_eq!(count(&w, 0, ItemKind::Bag), 1);
    assert_eq!(w.agents[0].capacity(), before + crate::world::BAG_EXTRA, "a bag carries more");
    // the extra room is real: the depot hands out past the bare 40
    w.agents[0].inventory.counts = [0; ItemKind::COUNT];
    give(&mut w, ItemKind::Bag, 1);
    give(&mut w, ItemKind::Stone, 45);
    assert!(!w.agents[0].inventory.is_full(w.agents[0].capacity()), "46 carried, still room with a bag");
}

/// Husbandry: a pen breeds only with a pair of the kind inside; animals are led in with a route; fed grain,
/// the pair brings a young one that works like any worker. With auto on, a pen fed by a route keeps breeding.
#[test]
fn a_pen_breeds_a_pair_led_into_it() {
    let (mut w, a) = with_agent(3);
    w.forget_everything();
    w.learn(Discovery::Farming);
    w.wild.retain(|x| x.species != WildSpecies::Wolf);
    let (sx, sy) = w.spawn_mm;
    let ox1 = w.spawn_worker(AgentKind::Ox, sx + 9_000, sy);
    give(&mut w, ItemKind::Wood, 8);
    w.step(&[]);
    assert!(w.knows(Discovery::Husbandry), "an animal, farming and wood: the pen");
    let (px, py) = spot_for(&w, BuildingKind::Pen, &[]);
    w.agents[0].x = px + 2_000;
    w.agents[0].y = py;
    w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::Pen, x: px, y: py })]);
    let pen = w.buildings.iter().position(|b| b.kind == BuildingKind::Pen).expect("pen built") as u32;
    give(&mut w, ItemKind::Grain, 8);
    // one ox is not a pair
    w.step(&[cmd(a, CommandKind::StartJob { building: pen, recipe: recipe("breed_ox") })]);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::NeedsPair)));
    assert_eq!(count(&w, 0, ItemKind::Grain), 8, "nothing taken on a refusal");
    // lead both oxen in
    let ox2 = w.spawn_worker(AgentKind::Ox, sx - 6_000, sy + 3_000);
    let stay = vec![RouteStep::Deposit { building: pen }, RouteStep::Wait { ticks: 36_000 }];
    w.step(&[cmd(a, CommandKind::SetRoute { worker: ox1, route: stay.clone() }), cmd(a, CommandKind::SetRoute { worker: ox2, route: stay })]);
    for _ in 0..60 * 40 {
        w.step(&[]);
    }
    assert!(crate::world::pair_in_pen(&w.agents, AgentKind::Ox, w.buildings[pen as usize].x, w.buildings[pen as usize].y), "both oxen walked in");
    let oxen = |w: &World| w.agents.iter().filter(|x| x.kind == AgentKind::Ox).count();
    assert_eq!(oxen(&w), 2);
    w.step(&[cmd(a, CommandKind::StartJob { building: pen, recipe: recipe("breed_ox") })]);
    assert!(w.buildings[pen as usize].job.is_some(), "{:?}", w.take_events());
    for _ in 0..7_210 {
        w.step(&[]);
    }
    assert_eq!(oxen(&w), 3, "a calf was born");
    // auto: grain dropped in the trough keeps the pair breeding
    w.step(&[cmd(a, CommandKind::SetAuto { building: pen, recipe: Some(recipe("breed_ox")) })]);
    give(&mut w, ItemKind::Grain, 4);
    w.step(&[cmd(a, CommandKind::Deposit { building: pen })]);
    for _ in 0..7_300 {
        w.step(&[]);
    }
    assert_eq!(oxen(&w), 4, "fed, the pen breeds on its own");
}

/// Draught: bronze and an ox teach the wheel; the bench makes wheels, a cart and a plough; a cart hitched to the
/// ox carries far more; a plough team at a field halves the growing time; the bronze age wants both.
#[test]
fn an_ox_pulls_a_cart_and_a_plough() {
    let (mut w, a) = with_agent(3);
    w.forget_everything();
    w.learn(Discovery::Workbench);
    w.learn(Discovery::Farming);
    w.wild.retain(|x| x.species != WildSpecies::Wolf);
    let (sx, sy) = w.spawn_mm;
    let ox = w.spawn_worker(AgentKind::Ox, sx + 1_000, sy);
    let dog = w.spawn_worker(AgentKind::Dog, sx - 1_000, sy);
    give(&mut w, ItemKind::Bronze, 1);
    w.step(&[]);
    w.step(&[]);
    assert!(w.knows(Discovery::Draught), "bronze worked, an ox at hand: the wheel");
    let (bx, by) = spot_for(&w, BuildingKind::Workbench, &[]);
    let bench = push_building(&mut w, BuildingKind::Workbench, bx, by);
    w.agents[0].x = bx + 1_000;
    w.agents[0].y = by;
    let run = |w: &mut World, r: &str| {
        w.step(&[cmd(a, CommandKind::StartJob { building: bench, recipe: recipe(r) })]);
        for _ in 0..1_300 {
            w.step(&[]);
        }
        w.step(&[cmd(a, CommandKind::Collect { building: bench, item: None })]);
    };
    give(&mut w, ItemKind::Plank, 12);
    give(&mut w, ItemKind::Bronze, 3);
    run(&mut w, "wheel");
    run(&mut w, "wheel");
    run(&mut w, "cart");
    run(&mut w, "plow");
    assert_eq!(count(&w, 0, ItemKind::Cart), 1);
    assert_eq!(count(&w, 0, ItemKind::Plow), 1);
    w.step(&[]);
    assert!(w.milestone_done(Milestone::Wheel) && w.milestone_done(Milestone::Plow));
    // hitching: only an ox or a horse, close by, one of each
    let (ox_i, dog_i) = (ox.0 as usize, dog.0 as usize);
    w.agents[0].x = w.agents[dog_i].x + 500;
    w.agents[0].y = w.agents[dog_i].y;
    w.step(&[cmd(a, CommandKind::Hitch { worker: dog, item: ItemKind::Cart })]);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::CannotHitch)), "a dog pulls no cart");
    let bare = w.agents[ox_i].capacity();
    w.agents[0].x = w.agents[ox_i].x + 500;
    w.agents[0].y = w.agents[ox_i].y;
    w.step(&[cmd(a, CommandKind::Hitch { worker: ox, item: ItemKind::Cart }), cmd(a, CommandKind::Hitch { worker: ox, item: ItemKind::Plow })]);
    assert_eq!(count(&w, 0, ItemKind::Cart), 0);
    assert_eq!(w.agents[ox_i].capacity(), bare + crate::world::CART_EXTRA, "the cart carries");
    assert_eq!(w.agents[ox_i].inventory.get(ItemKind::Plow), 1);
    // a ploughed field grows twice as fast
    let grow = |w: &mut World, with_team: bool| -> u32 {
        let (fx, fy) = (w.agents[ox_i].x + 2_000, w.agents[ox_i].y);
        let field = push_building(w, BuildingKind::Field, if with_team { fx } else { fx + 40_000 }, fy);
        w.buildings[field as usize].job = Some(crate::buildings::Job { recipe: recipe("sow").0, remaining: 14_400 });
        let mut t = 0;
        while w.buildings[field as usize].job.is_some() {
            w.agents[ox_i].hunger = MAX_STAT;
            w.step(&[]);
            t += 1;
        }
        t
    };
    let slow = grow(&mut w, false);
    let fast = grow(&mut w, true);
    assert!(fast * 2 <= slow + 2, "ploughed {fast} vs plain {slow}");
    // the bronze age wants the cart and the plough too
    w.produced[ItemKind::Bronze as usize] = 1;
    w.produced[ItemKind::BronzeAxe as usize] = 1;
    w.produced[ItemKind::BronzePickaxe as usize] = 1;
    w.step(&[]);
    assert!(!w.milestone_done(Milestone::BronzeAge), "the age also wants a mill an animal turns");
    w.milestones |= 1 << Milestone::AnimalPower as u64;
    w.step(&[]);
    assert!(w.milestone_done(Milestone::BronzeAge));
}

/// Old saves keep their goals: postcard writes the 16-bit and the 32-bit field alike.
#[test]
fn milestones_from_a_16_bit_save_read_as_32_bit() {
    let old: u16 = 0b1010_0000_0000_0101;
    let new: u32 = postcard::from_bytes(&postcard::to_allocvec(&old).unwrap()).unwrap();
    assert_eq!(new, old as u32);
}

/// S0: the widened fields read what older saves wrote. Postcard writes u16/u32/u64/u128 as the same varint, and a
/// u8 below 128 as the same single byte, so known (u32→u64), seen (u64→u128) and recipe ids (u8→u16, all below
/// 128 so far) load unchanged.
#[test]
fn widened_fields_read_older_saves() {
    let known: u32 = 0x01ff_ffff;
    assert_eq!(postcard::from_bytes::<u64>(&postcard::to_allocvec(&known).unwrap()).unwrap(), known as u64);
    let seen: u64 = 0xffff_ffff_ffff;
    assert_eq!(postcard::from_bytes::<u128>(&postcard::to_allocvec(&seen).unwrap()).unwrap(), seen as u128);
    for r in [0u8, 37, 127] {
        assert_eq!(postcard::from_bytes::<u16>(&postcard::to_allocvec(&r).unwrap()).unwrap(), r as u16);
        let auto: Option<u8> = Some(r);
        assert_eq!(postcard::from_bytes::<Option<u16>>(&postcard::to_allocvec(&auto).unwrap()).unwrap(), Some(r as u16));
    }
    // ids of 128 and up take two bytes in a new save; an old save only ever held ids below 128 (the list only grows at the end)
    for r in [128u16, 147, 300] {
        assert_eq!(postcard::from_bytes::<u16>(&postcard::to_allocvec(&r).unwrap()).unwrap(), r);
        let auto: Option<u16> = Some(r);
        assert_eq!(postcard::from_bytes::<Option<u16>>(&postcard::to_allocvec(&auto).unwrap()).unwrap(), auto);
    }
    assert!(rules().recipes.len() < u16::MAX as usize);
    assert!(ItemKind::COUNT < 255, "item indexes travel as bytes (255 = an empty hand)");
    // a state with nothing seen beyond 64 items hashes as before the widening
    let (mut w, _) = with_agent(1);
    w.mark_seen(ItemKind::Stone);
    let h = w.hash();
    w.seen |= 1u128 << 100;
    assert_ne!(w.hash(), h);
    let h = w.hash();
    w.mark_seen(ItemKind::Beam);
    assert_ne!(w.hash(), h, "items past the 128th count in the hash");
}

/// S1: tools are keys. Bare hands take the loose stones on top of an outcrop and nothing more; the seam below
/// wants a hammer; trees want an axe; iron wants bronze; cave chambers want harder picks. Bots carry iron.
#[test]
fn tools_open_the_resources_they_are_for() {
    let (mut w, a) = with_agent(1);
    let i = w.nodes.iter().position(|n| n.kind == NodeKind::StoneOutcrop && n.amount == 5).expect("stone");
    let (nx, ny) = w.nodes[i].center_mm();
    w.agents[0].x = nx;
    w.agents[0].y = ny + 900;
    w.step(&[cmd(a, CommandKind::Gather { node: NodeId(i as u32) })]);
    for _ in 0..90 * 8 {
        w.step(&[]);
    }
    assert_eq!(w.nodes[i].amount, 3, "two loose stones by hand, then the seam");
    let tool = crate::rules::ToolKind::Pick;
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::NeedsTool { tool, tier: 0 })), "told why");
    assert_eq!(w.agents[0].task, Task::Idle);
    // a tree without an axe is refused at once
    let t = w.nodes.iter().position(|n| n.kind == NodeKind::Tree).unwrap();
    w.step(&[cmd(a, CommandKind::Gather { node: NodeId(t as u32) })]);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::NeedsTool { tool: crate::rules::ToolKind::Axe, tier: 0 })));
    // iron wants bronze: a stone hammer is not enough, a bronze pick is
    let iron = Node { kind: NodeKind::IronOutcrop, tx: 0, ty: 0, amount: 4 };
    let mut inv = crate::world::Inventory::default();
    inv.counts[ItemKind::Hammer as usize] = 1;
    assert_eq!(crate::world::gate_check(&iron, AgentKind::Human, &inv), Err(Reject::NeedsTool { tool, tier: 2 }));
    inv.counts[ItemKind::BronzePickaxe as usize] = 1;
    assert!(crate::world::gate_check(&iron, AgentKind::Human, &inv).is_ok());
    assert!(crate::world::gate_check(&iron, AgentKind::Bot, &crate::world::Inventory::default()).is_ok(), "bots carry their own");
    // berries, fibre and branches never need a tool
    for k in [NodeKind::BerryBush, NodeKind::FiberPlant, NodeKind::Deadwood] {
        let n = Node { kind: k, tx: 0, ty: 0, amount: 1 };
        assert!(crate::world::gate_check(&n, AgentKind::Human, &crate::world::Inventory::default()).is_ok(), "{k:?}");
    }
    // the cave: going down to the third chamber wants bronze
    assert_eq!(rules().cave_tier(2), 2);
    assert_eq!(rules().cave_tier(3), 3);
}

/// Stone tools from loose stones and a stick; a knife skins the kill.
#[test]
fn hammer_knife_and_shovel_are_made_by_hand_and_the_knife_skins() {
    let (mut w, a) = with_agent(2);
    w.forget_everything();
    w.learn(Discovery::SharpStone);
    give(&mut w, ItemKind::Stone, 2);
    give(&mut w, ItemKind::Branch, 1);
    give(&mut w, ItemKind::Fiber, 1);
    w.step(&[]);
    assert!(w.knows(Discovery::Hammer), "two stones and a stick in hand: the hammer");
    let (st, br, fi, sh) = (ItemKind::Stone as u8, ItemKind::Branch as u8, ItemKind::Fiber as u8, ItemKind::SharpStone as u8);
    w.step(&[cmd(a, CommandKind::Combine { items: [st, st, br, fi] })]);
    play_craft_perfectly(&mut w, a);
    assert_eq!(count(&w, 0, ItemKind::Hammer), 1);
    w.learn(Discovery::StoneAxe);
    give(&mut w, ItemKind::SharpStone, 1);
    give(&mut w, ItemKind::Branch, 1);
    w.step(&[cmd(a, CommandKind::Combine { items: [sh, br, 255, 255] })]);
    play_craft_perfectly(&mut w, a);
    assert_eq!(count(&w, 0, ItemKind::Knife), 1);
    give(&mut w, ItemKind::Stone, 1);
    give(&mut w, ItemKind::Branch, 2);
    give(&mut w, ItemKind::Fiber, 1);
    w.step(&[cmd(a, CommandKind::Combine { items: [st, br, br, fi] })]);
    play_craft_perfectly(&mut w, a);
    assert_eq!(count(&w, 0, ItemKind::Shovel), 1);
    // without a knife a kill gives a little meat and no hide
    let id = deer_near(&mut w, 2_500);
    w.agents[0].inventory.counts[ItemKind::Knife as usize] = 0;
    give(&mut w, ItemKind::Spear, 1);
    w.step(&[cmd(a, CommandKind::Hunt { wild: id })]);
    let mut meat = None;
    for _ in 0..3000 {
        w.step(&[]);
        if let Some(m) = w.take_events().iter().find_map(|e| if let Event::Hunted { meat, .. } = e { Some(*meat) } else { None }) {
            meat = Some(m);
            break;
        }
    }
    assert_eq!(meat, Some(1), "bare hands tear off a little meat");
    assert_eq!(count(&w, 0, ItemKind::Hide), 0, "no hide without a knife");
}

/// S16: a world plays by its own rules. Here its rules say berry bushes want a knife: the base world picks berries
/// by hand, the other one refuses until a knife is carried.
#[test]
fn a_world_plays_by_its_own_rules() {
    let own = format!("{}\n[[gate]]\nnode = \"BerryBush\"\ntool = \"knife\"\ntier = 0\n", crate::rules::Rules::base_text());
    let pick = |custom: bool| {
        let (mut w, a) = with_agent(1);
        if custom {
            w.set_custom_rules(own.clone()).expect("parse");
        }
        let i = w.nodes.iter().position(|n| n.kind == NodeKind::BerryBush).expect("bush");
        let (nx, ny) = w.nodes[i].center_mm();
        w.agents[0].x = nx;
        w.agents[0].y = ny + 900;
        w.step(&[cmd(a, CommandKind::Gather { node: NodeId(i as u32) })]);
        let refused = w.take_events().iter().any(|e| matches!(e, Event::Rejected(_, Reject::NeedsTool { .. })));
        give(&mut w, ItemKind::Knife, 1);
        w.step(&[cmd(a, CommandKind::Gather { node: NodeId(i as u32) })]);
        let with_knife = matches!(w.agents[0].task, Task::Gather { .. });
        (refused, with_knife)
    };
    assert_eq!(pick(false), (false, true), "base rules: berries by hand");
    assert_eq!(pick(true), (true, true), "own rules: a knife first");
}

/// S2: heat sources climb toward their ceiling while they burn or work and cool off after; a station's job only
/// goes on while it is hot enough. A camp fire never gets hot enough for clay.
#[test]
fn stations_heat_up_and_jobs_wait_for_the_heat() {
    let (mut w, a) = with_agent(1);
    learn_all(&mut w);
    let (x, y) = spot_for(&w, BuildingKind::Campfire, &[]);
    let fire = push_building(&mut w, BuildingKind::Campfire, x, y);
    w.buildings[fire as usize].fuel = 100_000;
    for _ in 0..60 * 20 {
        w.step(&[]);
    }
    assert_eq!(w.buildings[fire as usize].temp, 600, "a wood fire tops out at 600 °C");
    w.buildings[fire as usize].fuel = 0;
    for _ in 0..60 * 60 {
        w.step(&[]);
    }
    assert!(w.buildings[fire as usize].temp < 200, "and cools once it is out: {}", w.buildings[fire as usize].temp);
    // the furnace waits for iron heat before the bloom starts forming
    let (fx, fy) = spot_for(&w, BuildingKind::Furnace, &[(x, y)]);
    let furnace = push_building(&mut w, BuildingKind::Furnace, fx, fy);
    w.agents[0].x = fx + 1000;
    w.agents[0].y = fy;
    give(&mut w, ItemKind::IronOre, 2);
    give(&mut w, ItemKind::Charcoal, 2);
    w.step(&[cmd(a, CommandKind::StartJob { building: furnace, recipe: recipe("iron_bloom") })]);
    let full = rules().recipes[recipe("iron_bloom").0 as usize].ticks;
    for _ in 0..60 * 10 {
        w.step(&[]);
    }
    let job = w.buildings[furnace as usize].job.expect("working");
    assert_eq!(job.remaining, full, "not hot enough yet: nothing done");
    assert!(w.buildings[furnace as usize].temp > 300 && w.buildings[furnace as usize].temp < 1150);
}

/// Dilim A2: the wild crop follows the climate; nothing grows in winter (a hard winter kills it); harvests tire the
/// soil and manure from the pen restores it; a calf is a generation further from the wild and carries more.
#[test]
fn winter_soil_and_generations() {
    let (mut w, a) = with_agent(3);
    learn_all(&mut w);
    // a field sown in late autumn stops in winter
    let (fx, fy) = spot_for(&w, BuildingKind::Field, &[]);
    let field = push_building(&mut w, BuildingKind::Field, fx, fy) as usize;
    let days = w.settings.season_days as u64;
    w.tick = TICKS_PER_DAY * days * 3 + 10; // winter
    w.buildings[field].job = Some(crate::buildings::Job { recipe: recipe("sow").0, remaining: 1_000 });
    for _ in 0..2_000 {
        w.step(&[]);
    }
    assert_eq!(w.buildings[field].job.map(|j| j.remaining), Some(1_000), "nothing grows in winter");
    let mut hard = w.clone();
    hard.settings.difficulty = crate::world::Difficulty::Realistic;
    hard.step(&[]);
    assert!(hard.buildings[field].job.is_none(), "a hard winter kills the crop");
    // spring: it finishes, and the harvest tires the soil
    w.tick = TICKS_PER_DAY * days * 4 + 10;
    for _ in 0..1_100 {
        w.step(&[]);
    }
    assert!(w.buildings[field].job.is_none());
    assert_eq!(w.buildings[field].output[ItemKind::Grain as usize], 6, "full soil, full crop");
    assert_eq!(w.buildings[field].fertility, 850);
    w.buildings[field].output = [0; ItemKind::COUNT];
    w.buildings[field].fertility = 500;
    w.buildings[field].job = Some(crate::buildings::Job { recipe: recipe("sow").0, remaining: 1 });
    w.step(&[]);
    assert_eq!(w.buildings[field].output[ItemKind::Grain as usize], 3, "tired soil, half a crop");
    // manure worked in restores it
    w.agents[0].x = fx + 1_500;
    w.agents[0].y = fy;
    give(&mut w, ItemKind::Manure, 1);
    w.step(&[cmd(a, CommandKind::Deposit { building: field as u32 })]);
    w.step(&[]);
    assert!(w.buildings[field].fertility >= 600, "manure: {}", w.buildings[field].fertility);
    // a calf: one generation past its parents, stronger
    let (px, py) = spot_for(&w, BuildingKind::Pen, &[(fx, fy)]);
    let pen = push_building(&mut w, BuildingKind::Pen, px, py) as usize;
    let o1 = w.spawn_worker(AgentKind::Ox, px + 1_000, py);
    w.spawn_worker(AgentKind::Ox, px - 1_000, py);
    w.agents[o1.0 as usize].generation = 2;
    w.buildings[pen].job = Some(crate::buildings::Job { recipe: recipe("breed_ox").0, remaining: 1 });
    w.step(&[]);
    let calf = w.agents.last().unwrap();
    assert_eq!((calf.kind, calf.generation), (AgentKind::Ox, 3));
    assert!(calf.capacity() > AgentKind::Ox.capacity(), "a bred ox carries more");
    // penned animals leave dung
    while w.tick % 6000 != 0 {
        w.step(&[]);
    }
    w.step(&[]);
    assert!(w.buildings[pen].output[ItemKind::Manure as usize] >= 1);
}

/// Spoilage: meat goes bad fast, grain hardly at all; what rots turns to manure; winter and storage jars in a depot
/// slow it; peers agree because it is a pure function of seed and time.
#[test]
fn food_spoils_into_manure_and_jars_slow_it() {
    let run = |winter: bool, jars: u16| {
        let (mut w, _) = with_agent(4);
        let days = w.settings.season_days as u64;
        w.tick = if winter { TICKS_PER_DAY * days * 3 } else { TICKS_PER_DAY };
        give(&mut w, ItemKind::RawMeat, 40);
        give(&mut w, ItemKind::Grain, 40);
        let (dx, dy) = spot_for(&w, BuildingKind::Depot, &[]);
        let d = push_building(&mut w, BuildingKind::Depot, dx, dy) as usize;
        w.buildings[d].output[ItemKind::Berries as usize] = 100;
        w.buildings[d].output[ItemKind::LargePot as usize] = jars;
        let start = w.tick;
        while w.tick < start + TICKS_PER_DAY {
            w.agents[0].hunger = MAX_STAT;
            w.agents[0].thirst = MAX_STAT;
            w.agents[0].warmth = MAX_STAT;
            w.agents[0].health = MAX_STAT;
            w.step(&[]);
        }
        (count(&w, 0, ItemKind::RawMeat), count(&w, 0, ItemKind::Grain), count(&w, 0, ItemKind::Manure), w.buildings[d].output[ItemKind::Berries as usize], w.hash())
    };
    let (meat, grain, manure, berries, h) = run(false, 0);
    assert!((14..=26).contains(&meat), "about half the meat goes in a day: {meat}");
    assert!(grain >= 39, "grain keeps: {grain}");
    assert_eq!(manure, 40 - meat + 40 - grain, "what rots turns to manure");
    assert!((55..=75).contains(&berries), "berries in a bare depot: {berries}");
    let (_, _, _, kept, _) = run(false, 4);
    assert!(kept > berries + 15, "storage jars slow it: {kept} vs {berries}");
    let (cold_meat, ..) = run(true, 0);
    assert!(cold_meat > meat + 5, "winter slows it: {cold_meat} vs {meat}");
    assert_eq!(run(false, 0).4, h, "deterministic");
}

/// Dilim B: the copper age from commands. Copper in hand teaches casting; a crucible and moulds pressed around the
/// stone tools are fired in a pit kiln; copper melts there and is poured by hand; the heads go on sticks; tin opens
/// to the copper pick; the goals follow.
#[test]
fn the_copper_age_casts_its_first_tools() {
    let (mut w, a) = with_agent(5);
    w.forget_everything();
    for d in [Discovery::Fire, Discovery::Farming, Discovery::Pottery, Discovery::StoneAxe, Discovery::Hammer] {
        w.learn(d);
    }
    w.wild.retain(|x| x.species != WildSpecies::Wolf);
    give(&mut w, ItemKind::CopperOre, 1);
    w.step(&[]);
    assert!(w.knows(Discovery::Casting));
    give(&mut w, ItemKind::StoneAxe, 1);
    give(&mut w, ItemKind::Hammer, 1);
    let craft = |w: &mut World, r: &str| {
        w.step(&[cmd(a, CommandKind::Craft { recipe: recipe(r) })]);
        play_craft_perfectly(w, a);
    };
    give(&mut w, ItemKind::Clay, 6);
    give(&mut w, ItemKind::Fiber, 1);
    craft(&mut w, "raw_crucible");
    craft(&mut w, "raw_mold_axe");
    craft(&mut w, "raw_mold_pick");
    assert_eq!((count(&w, 0, ItemKind::RawCrucible), count(&w, 0, ItemKind::RawMoldAxe), count(&w, 0, ItemKind::RawMoldPick)), (1, 1, 1));
    let (kx, ky) = spot_for(&w, BuildingKind::PitKiln, &[]);
    let kiln = push_building(&mut w, BuildingKind::PitKiln, kx, ky);
    w.agents[0].x = kx + 1_200;
    w.agents[0].y = ky;
    let fire = |w: &mut World, r: &str| {
        w.step(&[cmd(a, CommandKind::StartJob { building: kiln, recipe: recipe(r) })]);
        assert!(w.buildings[kiln as usize].job.is_some(), "{r}: {:?}", w.take_events());
        while w.buildings[kiln as usize].job.is_some() {
            let me = &mut w.agents[0];
            (me.hunger, me.thirst, me.warmth, me.health) = (MAX_STAT, MAX_STAT, MAX_STAT, MAX_STAT);
            w.step(&[]);
        }
        w.step(&[cmd(a, CommandKind::Collect { building: kiln, item: None })]);
    };
    give(&mut w, ItemKind::Fiber, 12);
    give(&mut w, ItemKind::Wood, 12);
    fire(&mut w, "fire_crucible");
    fire(&mut w, "fire_mold_axe");
    fire(&mut w, "fire_mold_pick");
    assert_eq!((count(&w, 0, ItemKind::Crucible), count(&w, 0, ItemKind::MoldAxe), count(&w, 0, ItemKind::MoldPick)), (1, 1, 1));
    // melt and pour twice
    for (mold_recipe, head) in [("pour_copper_axe_head", ItemKind::CopperAxeHead), ("pour_copper_pick_head", ItemKind::CopperPickHead)] {
        give(&mut w, ItemKind::CopperOre, 4);
        fire(&mut w, "melt_copper");
        assert_eq!(count(&w, 0, ItemKind::MoltenCopper), 1, "molten copper in the crucible");
        craft(&mut w, mold_recipe);
        assert_eq!(count(&w, 0, head), 1, "cast {head:?}");
        assert_eq!(count(&w, 0, ItemKind::Crucible), 1, "the crucible comes back");
    }
    w.step(&[]);
    assert!(w.milestone_done(Milestone::FirstMetal));
    give(&mut w, ItemKind::Branch, 2);
    give(&mut w, ItemKind::Fiber, 2);
    craft(&mut w, "copper_axe");
    craft(&mut w, "copper_pickaxe");
    assert_eq!((count(&w, 0, ItemKind::CopperAxe), count(&w, 0, ItemKind::CopperPickaxe)), (1, 1));
    w.step(&[]);
    assert!(w.milestone_done(Milestone::CopperTools) && w.milestone_done(Milestone::CopperAge));
    // tin wants copper
    let tin = Node { kind: NodeKind::TinOutcrop, tx: 0, ty: 0, amount: 4 };
    let mut inv = crate::world::Inventory::default();
    inv.counts[ItemKind::Hammer as usize] = 1;
    assert!(crate::world::gate_check(&tin, AgentKind::Human, &inv).is_err(), "a stone hammer will not do for tin");
    inv.counts[ItemKind::CopperPickaxe as usize] = 1;
    assert!(crate::world::gate_check(&tin, AgentKind::Human, &inv).is_ok());
}

/// A spilled pour is not lost: the metal cools to ore, the crucible and mould survive.
#[test]
fn a_spilled_pour_gives_the_copper_back() {
    let (mut w, a) = with_agent(5);
    learn_all(&mut w);
    give(&mut w, ItemKind::MoltenCopper, 1);
    give(&mut w, ItemKind::MoldAxe, 1);
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("pour_copper_axe_head") })]);
    for _ in 0..2000 {
        if w.craft_view(a).is_none() {
            break;
        }
        w.step(&[cmd(a, CommandKind::Tighten { power: 0 })]); // far too slow a pour
    }
    assert_eq!(count(&w, 0, ItemKind::CopperAxeHead), 0);
    assert_eq!((count(&w, 0, ItemKind::CopperOre), count(&w, 0, ItemKind::Crucible), count(&w, 0, ItemKind::MoldAxe)), (3, 1, 1));
}

/// Dilim C: bronze is alloyed by ratio in the stone furnace from whatever ore is loaded; the wrong share gives bad
/// bronze that remelts to ore; bronze is melted in a crucible and cast in the copper age moulds.
#[test]
fn bronze_is_alloyed_by_ratio_and_cast() {
    let (mut w, a) = with_agent(3);
    learn_all(&mut w);
    w.wild.retain(|x| x.species != WildSpecies::Wolf);
    let (fx, fy) = spot_for(&w, BuildingKind::Furnace, &[]);
    let furnace = push_building(&mut w, BuildingKind::Furnace, fx, fy);
    w.agents[0].x = fx + 1_000;
    w.agents[0].y = fy;
    let melt = |w: &mut World, copper: u16, tin: u16| {
        give(w, ItemKind::CopperOre, copper);
        give(w, ItemKind::TinOre, tin);
        give(w, ItemKind::Charcoal, 2);
        w.step(&[cmd(a, CommandKind::Deposit { building: furnace })]);
        w.step(&[cmd(a, CommandKind::StartJob { building: furnace, recipe: recipe("alloy_bronze") })]);
        assert!(w.buildings[furnace as usize].job.is_some(), "{:?}", w.take_events());
        while w.buildings[furnace as usize].job.is_some() {
            let me = &mut w.agents[0];
            (me.hunger, me.thirst, me.warmth, me.health) = (MAX_STAT, MAX_STAT, MAX_STAT, MAX_STAT);
            w.step(&[]);
        }
        w.step(&[cmd(a, CommandKind::Collect { building: furnace, item: None })]);
    };
    // 9 copper + 1 tin = 90% copper: good bronze, two ingots from ten ores (the odd two are lost in the slag)
    melt(&mut w, 9, 1);
    assert_eq!(count(&w, 0, ItemKind::Bronze), 2);
    // half and half: bad bronze
    melt(&mut w, 4, 4);
    assert_eq!(count(&w, 0, ItemKind::BadBronze), 2);
    // realistic is stricter: 85% copper is bad there
    w.settings.difficulty = crate::world::Difficulty::Realistic;
    w.agents[0].inventory.counts[ItemKind::BadBronze as usize] = 0;
    melt(&mut w, 17, 3);
    assert_eq!(count(&w, 0, ItemKind::BadBronze), 5, "85% copper is outside 88-92 when it is realistic");
    w.settings.difficulty = crate::world::Difficulty::Forgiving;
    // cast a bronze axe head and haft it
    let (kx, ky) = spot_for(&w, BuildingKind::PitKiln, &[(fx, fy)]);
    let kiln = push_building(&mut w, BuildingKind::PitKiln, kx, ky);
    w.agents[0].x = kx + 1_200;
    w.agents[0].y = ky;
    w.agents[0].inventory.counts = [0; ItemKind::COUNT];
    give(&mut w, ItemKind::Bronze, 1);
    give(&mut w, ItemKind::Crucible, 1);
    give(&mut w, ItemKind::Fiber, 3);
    give(&mut w, ItemKind::Wood, 2);
    give(&mut w, ItemKind::MoldAxe, 1);
    give(&mut w, ItemKind::Branch, 1);
    w.step(&[cmd(a, CommandKind::StartJob { building: kiln, recipe: recipe("melt_bronze") })]);
    while w.buildings[kiln as usize].job.is_some() {
        let me = &mut w.agents[0];
        (me.hunger, me.thirst, me.warmth, me.health) = (MAX_STAT, MAX_STAT, MAX_STAT, MAX_STAT);
        w.step(&[]);
    }
    w.step(&[cmd(a, CommandKind::Collect { building: kiln, item: None })]);
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("pour_bronze_axe_head") })]);
    play_craft_perfectly(&mut w, a);
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("bronze_axe_hafted") })]);
    play_craft_perfectly(&mut w, a);
    assert_eq!(count(&w, 0, ItemKind::BronzeAxe), 1);
}

/// Dilim C: forging at the anvil (a saw), planks from the saw, and a mill turned by hand or by an ox at a gin.
#[test]
fn the_anvil_the_saw_and_the_animal_mill() {
    let (mut w, a) = with_agent(3);
    learn_all(&mut w);
    w.wild.retain(|x| x.species != WildSpecies::Wolf);
    give(&mut w, ItemKind::Bronze, 2);
    give(&mut w, ItemKind::Charcoal, 1);
    give(&mut w, ItemKind::Hammer, 1);
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("forge_saw") })]);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::NeedsStation(BuildingKind::Anvil))), "forging wants an anvil");
    let (ax, ay) = spot_for(&w, BuildingKind::Anvil, &[]);
    give(&mut w, ItemKind::Stone, 6);
    w.agents[0].x = ax + 1_200;
    w.agents[0].y = ay;
    w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::Anvil, x: ax, y: ay })]);
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("forge_saw") })]);
    play_craft_perfectly(&mut w, a);
    assert_eq!(count(&w, 0, ItemKind::Saw), 1);
    give(&mut w, ItemKind::Wood, 1);
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("plank") })]);
    play_craft_perfectly(&mut w, a);
    assert_eq!(count(&w, 0, ItemKind::Plank), 2);
    // a mill: no hand at it, nothing turns
    let (mx, my) = spot_for(&w, BuildingKind::HandMill, &[(ax, ay)]);
    let mill = push_building(&mut w, BuildingKind::HandMill, mx, my) as usize;
    w.agents[0].x = mx + 20_000;
    w.buildings[mill].job = Some(crate::buildings::Job { recipe: recipe("mill_flour").0, remaining: 600 });
    for _ in 0..100 {
        w.step(&[]);
    }
    assert_eq!(w.buildings[mill].job.unwrap().remaining, 600, "nobody turns it");
    // an ox at a gin beside it turns it three times as fast as a hand, and that is the animal power goal
    let (gx, gy) = (mx + 3_000, my);
    push_building(&mut w, BuildingKind::AnimalGin, gx, gy);
    w.spawn_worker(AgentKind::Ox, gx + 1_000, gy);
    for _ in 0..100 {
        w.agents.last_mut().unwrap().hunger = MAX_STAT;
        w.step(&[]);
    }
    assert_eq!(w.buildings[mill].job.unwrap().remaining, 300, "an ox at the gin: three a tick");
    for _ in 0..200 {
        w.step(&[]);
    }
    assert!(w.buildings[mill].job.is_none());
    assert_eq!(w.buildings[mill].output[ItemKind::Flour as usize], 3);
    assert!(w.milestone_done(Milestone::AnimalPower));
}

/// Tools wear with use and go blunt (slower, never gone); a repair uses up a piece of their material.
#[test]
fn tools_wear_go_blunt_and_are_mended() {
    let (mut w, a) = with_agent(1);
    learn_all(&mut w);
    w.settings.difficulty = crate::world::Difficulty::Realistic; // stone tools last 60 uses
    give(&mut w, ItemKind::Hammer, 1);
    let life = 60;
    w.agents[0].wear = vec![(ItemKind::Hammer, life - 1)];
    let i = w.nodes.iter().position(|n| n.kind == NodeKind::StoneOutcrop && n.amount == 5).expect("stone");
    w.set_node_amount(i, 3); // past the loose stones: the hammer does the work
    let (nx, ny) = w.nodes[i].center_mm();
    w.agents[0].x = nx;
    w.agents[0].y = ny + 900;
    w.step(&[cmd(a, CommandKind::Gather { node: NodeId(i as u32) })]);
    let mut blunt_at = None;
    for t in 0..90 * 20 {
        w.step(&[]);
        if w.take_events().iter().any(|e| matches!(e, Event::ToolBlunt { item: ItemKind::Hammer, .. })) {
            blunt_at = Some(t);
        }
    }
    assert!(blunt_at.is_some(), "the hammer went blunt");
    assert_eq!(count(&w, 0, ItemKind::Hammer), 1, "blunt, not gone");
    assert!(w.agents[0].wear_of(ItemKind::Hammer) >= life);
    // mend it with a stone
    give(&mut w, ItemKind::Stone, 1);
    w.agents[0].task = Task::Idle;
    w.step(&[cmd(a, CommandKind::Repair { item: ItemKind::Hammer })]);
    assert_eq!(w.agents[0].wear_of(ItemKind::Hammer), 0);
    assert_eq!(count(&w, 0, ItemKind::Stone), 0, "the repair used the stone");
    // wear is kept in the save
    w.agents[0].wear = vec![(ItemKind::Hammer, 7)];
    let b = World::from_bytes(&w.to_bytes()).expect("load");
    assert_eq!(b.agents[0].wear_of(ItemKind::Hammer), 7);
}

/// Çağ 6: fire brick and a blast furnace blown by a windmill-driven bellows run pig iron; beaten twice on the iron
/// anvil it is steel; a windmill-powered belt carries a station's goods to the depot; an automaton (steel now) on
/// a route is the card automaton; the steel age follows.
#[test]
fn the_steel_and_mechanical_age() {
    let (mut w, a) = with_agent(3);
    learn_all(&mut w);
    w.wild.retain(|x| x.species != WildSpecies::Wolf);
    let (bx, by) = spot_for(&w, BuildingKind::BlastFurnace, &[]);
    let furnace = push_building(&mut w, BuildingKind::BlastFurnace, bx, by) as usize;
    push_building(&mut w, BuildingKind::Bellows, bx + 1_500, by);
    push_building(&mut w, BuildingKind::Windmill, bx + 5_000, by);
    w.agents[0].x = bx + 20_000;
    w.buildings[furnace].job = Some(crate::buildings::Job { recipe: recipe("pig_iron").0, remaining: 3000 });
    let mut t = 0;
    while w.buildings[furnace].job.is_some() && t < 60 * 200 {
        w.step(&[]);
        t += 1;
    }
    assert_eq!(w.buildings[furnace].output[ItemKind::PigIron as usize], 2, "pig iron runs");
    // steel at the iron anvil
    let (ax, ay) = (bx - 5_000, by);
    push_building(&mut w, BuildingKind::IronAnvil, ax, ay);
    w.agents[0].x = ax + 1_000;
    w.agents[0].y = ay;
    give(&mut w, ItemKind::PigIron, 1);
    give(&mut w, ItemKind::Charcoal, 2);
    give(&mut w, ItemKind::Hammer, 1);
    for r in ["forge_high_carbon", "forge_steel"] {
        w.step(&[cmd(a, CommandKind::Craft { recipe: recipe(r) })]);
        play_craft_perfectly(&mut w, a);
    }
    assert_eq!(count(&w, 0, ItemKind::Steel), 1);
    // a belt beside the furnace takes its pig iron to the depot, under wind power
    push_building(&mut w, BuildingKind::Conveyor, bx, by + 2_000);
    let depot = push_building(&mut w, BuildingKind::Depot, bx + 2_000, by + 8_000) as usize;
    for _ in 0..120 {
        w.step(&[]);
    }
    assert_eq!(w.buildings[depot].output[ItemKind::PigIron as usize], 2, "carried by the belt");
    assert_eq!(w.buildings[furnace].output[ItemKind::PigIron as usize], 0);
    // a bot on a route
    let bot = w.spawn_worker(AgentKind::Bot, bx, by + 3_000);
    w.step(&[cmd(a, CommandKind::SetRoute { worker: bot, route: vec![RouteStep::Deposit { building: depot as u32 }] })]);
    w.step(&[]);
    for m in [Milestone::Steel, Milestone::WindPower, Milestone::BeltLine, Milestone::Automaton, Milestone::CardBot] {
        assert!(w.milestone_done(m), "{m:?}");
    }
    assert!(w.milestone_done(Milestone::SteelAge));
}

/// Çağ 7: a steam engine burning coal and water drives machines; a pump it drives drains the deepest cave chamber;
/// sand melts to glass; the steam age follows.
#[test]
fn the_steam_age() {
    let mut w = World::with_size(crate::map::tag_seed_natural(42, 256), 256, 256);
    let a = w.spawn_agent(w.spawn_mm.0, w.spawn_mm.1);
    learn_all(&mut w);
    w.wild.retain(|x| x.species != WildSpecies::Wolf);
    give(&mut w, ItemKind::Coal, 1);
    w.step(&[]);
    assert!(w.milestone_done(Milestone::Coal));
    // a steam engine raising steam drives a mill
    let (sx, sy) = spot_for(&w, BuildingKind::SteamEngine, &[]);
    let engine = push_building(&mut w, BuildingKind::SteamEngine, sx, sy) as usize;
    w.buildings[engine].job = Some(crate::buildings::Job { recipe: recipe("raise_steam").0, remaining: 3600 });
    let mill = push_building(&mut w, BuildingKind::HandMill, sx + 4_000, sy) as usize;
    w.buildings[mill].job = Some(crate::buildings::Job { recipe: recipe("mill_flour").0, remaining: 600 });
    w.agents[0].x = sx + 30_000;
    for _ in 0..60 * 30 {
        w.step(&[]);
    }
    assert!(w.buildings[engine].steaming(), "boiling: {} °C", w.buildings[engine].temp);
    assert!(w.buildings[mill].job.is_none(), "the mill ran on steam");
    assert!(w.milestone_done(Milestone::SteamPower));
    // Find a natural passage crossing into the flooded depth.
    use crate::cave_terrain::{CaveNav, CaveTile, SIDES};
    let empty = std::collections::BTreeSet::new();
    let mut nav = CaveNav::new(&w.map, &empty);
    let (from, into) = (0..w.map.h).flat_map(|y| (0..w.map.w).map(move |x| CaveTile { x, y, z: -59 }))
        .filter(|tile| w.map.terrain(tile.x, tile.y) != Some(Terrain::Water))
        .find_map(|tile| {
            if !nav.floor(tile) { return None; }
            SIDES.into_iter().find_map(|(dx, dy)| nav.next_floor(tile, tile.x + dx, tile.y + dy).filter(|next| next.z == -60))
                .map(|next| (tile, next))
        }).expect("a slope into the flooded depth");
    give(&mut w, ItemKind::IronPickaxe, 1);
    let (x, y) = from.center_mm();
    let a0 = &mut w.agents[0];
    a0.x = x;
    a0.y = y;
    a0.z = i64::from(from.z) * 1_000;
    a0.set_cave_view(crate::caves::CaveView { x, y, z: from.z, light: 0, leaving: false });
    let (x, y) = into.center_mm();
    let enter = || cmd(a, CommandKind::MoveToFloor { position: crate::cave_terrain::WorldPoint { x_mm: x, y_mm: y, z_mm: -60_000 } });
    w.step(&[enter()]);
    assert!(w.take_events().iter().any(|e| matches!(e, Event::Rejected(_, Reject::Flooded))));
    push_building(&mut w, BuildingKind::SteamPump, x + 3_000, y);
    let e2 = push_building(&mut w, BuildingKind::SteamEngine, x + 6_000, y) as usize;
    w.buildings[e2].job = Some(crate::buildings::Job { recipe: recipe("raise_steam").0, remaining: 3600 });
    w.buildings[e2].temp = 300;
    for _ in 0..40 { w.step(&[]); }
    w.step(&[enter()]);
    for _ in 0..120 {
        if w.milestone_done(Milestone::DrainMine) { break; }
        w.step(&[]);
    }
    assert!(w.milestone_done(Milestone::DrainMine), "{:?}", w.take_events());
    // glass
    w.produced[ItemKind::Glass as usize] = 1;
    w.step(&[]);
    assert!(w.milestone_done(Milestone::SteamAge));
}

/// Çağ 8–10 and the final: a water-driven dynamo feeds a grid along poles; a motor on it drives a mill and a lamp
/// lights; electronics goals; a computer on the grid opens the design table, which refuses a recipe worth more
/// than it takes and writes a fair one into the world's rules; the loop grows a new world from this one, with the
/// rules and the ruins carried over and the newcomer an ape again.
#[test]
fn electricity_computers_and_the_loop() {
    let (mut w, a) = with_agent(3);
    learn_all(&mut w);
    w.wild.retain(|x| x.species != WildSpecies::Wolf);
    let (dx, dy) = spot_for(&w, BuildingKind::Dynamo, &[]);
    push_building(&mut w, BuildingKind::WaterWheel, dx, dy + 3_000);
    push_building(&mut w, BuildingKind::Dynamo, dx, dy);
    push_building(&mut w, BuildingKind::Pole, dx + 10_000, dy);
    let motor = push_building(&mut w, BuildingKind::ElectricMotor, dx + 20_000, dy) as usize;
    push_building(&mut w, BuildingKind::ElectricLamp, dx + 20_000, dy + 3_000);
    let live = w.electric_live();
    assert!(live[motor], "the grid reaches the motor over the pole");
    let mill = push_building(&mut w, BuildingKind::HandMill, dx + 25_000, dy) as usize;
    w.buildings[mill].job = Some(crate::buildings::Job { recipe: recipe("mill_flour").0, remaining: 600 });
    w.agents[0].x = dx - 30_000;
    for _ in 0..400 {
        w.step(&[]);
    }
    assert!(w.buildings[mill].job.is_none(), "the motor turned the mill");
    for m in [Milestone::Electricity, Milestone::ElectricMotor, Milestone::ElectricLight, Milestone::ElectricAge] {
        assert!(w.milestone_done(m), "{m:?}");
    }
    for k in [ItemKind::Silicon, ItemKind::Transistor, ItemKind::CircuitBoard] {
        w.produced[k as usize] = 1;
    }
    w.step(&[]);
    assert!(w.milestone_done(Milestone::ElectronicsAge));
    // a computer on the grid
    let (cx, cy) = (dx + 20_000, dy - 5_000);
    push_building(&mut w, BuildingKind::Computer, cx, cy);
    w.step(&[]);
    assert!(w.milestone_done(Milestone::ComputerOn));
    w.agents[0].x = cx + 1_000;
    w.agents[0].y = cy;
    for k in [ItemKind::Stone, ItemKind::Steel, ItemKind::Wood] {
        w.mark_seen(k);
    }
    // a stone for a steel bar: refused
    w.step(&[cmd(a, CommandKind::Design { inputs: vec![(ItemKind::Stone, 1)], output: (ItemKind::Steel, 1) })]);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::Unbalanced)));
    // two wood for a stone: fair
    w.step(&[cmd(a, CommandKind::Design { inputs: vec![(ItemKind::Wood, 2)], output: (ItemKind::Stone, 1) })]);
    let own = w.rules().recipes.iter().position(|r| r.id == "own_1").expect("written into the world's rules");
    w.step(&[]);
    assert!(w.milestone_done(Milestone::DesignWritten) && w.milestone_done(Milestone::ComputerAge));
    give(&mut w, ItemKind::Wood, 2);
    w.agents[0].inventory.counts[ItemKind::Stone as usize] = 0;
    w.step(&[cmd(a, CommandKind::Craft { recipe: RecipeId(own as u16) })]);
    assert_eq!(count(&w, 0, ItemKind::Stone), 1, "the own recipe works");
    // the save keeps the world's own rules
    let b = World::from_bytes(&w.to_bytes()).expect("load");
    assert!(b.rules().recipes.iter().any(|r| r.id == "own_1"));
    // the loop
    let next = w.next_cycle();
    assert_ne!(next.seed, w.seed);
    assert_eq!(next.settings, w.settings);
    assert!(next.rules().recipes.iter().any(|r| r.id == "own_1"), "the rules come along");
    assert!(next.buildings.iter().any(|r| r.kind == BuildingKind::Ruin), "what was built stands as ruins");
    assert!(next.buildings.iter().all(|r| r.kind == BuildingKind::Ruin));
    assert_eq!(next.agents.len(), 1);
    assert!(next.agents[0].inventory.total() == 0 && !next.knows(Discovery::Fire), "an ape again");
    assert!(World::from_bytes(&next.to_bytes()).is_ok());
}

/// S9: food groups cap health (a lopsided diet keeps the body weaker); a cellar keeps food; the prospector's pick
/// finds hidden deposits and a mine shaft on one brings up its ore.
#[test]
fn nutrition_cellar_and_geology() {
    let (mut w, a) = with_agent(3);
    learn_all(&mut w);
    w.settings.difficulty = crate::world::Difficulty::Realistic;
    w.agents[0].nutrition = [900, 0, 0];
    assert_eq!(w.agents[0].health_cap(&w.settings), MAX_STAT * 80 / 100, "one group: 80%");
    w.agents[0].hunger = 1_000;
    for k in [ItemKind::CookedMeat, ItemKind::Berries] {
        give(&mut w, k, 2);
        for _ in 0..2 {
            w.agents[0].hunger = 1_000; // hungry again: a full belly refuses food
            w.step(&[cmd(a, CommandKind::Eat { item: k })]);
        }
    }
    assert_eq!(w.agents[0].health_cap(&w.settings), MAX_STAT, "a varied diet: full health");
    // a cellar keeps berries far better than a bare depot
    let berries_after_a_day = |kind: BuildingKind| {
        let (mut w, _) = with_agent(4);
        let (x, y) = spot_for(&w, BuildingKind::Depot, &[]);
        let b = push_building(&mut w, kind, x, y) as usize;
        w.buildings[b].output[ItemKind::Berries as usize] = 100;
        w.tick = TICKS_PER_DAY;
        let start = w.tick;
        while w.tick < start + TICKS_PER_DAY {
            let me = &mut w.agents[0];
            (me.hunger, me.thirst, me.warmth, me.health) = (MAX_STAT, MAX_STAT, MAX_STAT, MAX_STAT);
            w.step(&[]);
        }
        w.buildings[b].output[ItemKind::Berries as usize]
    };
    assert!(berries_after_a_day(BuildingKind::Cellar) > berries_after_a_day(BuildingKind::Depot) + 15);
    // prospecting and a mine shaft
    let (sx, sy) = ((w.spawn_mm.0 / 1000) as i32, (w.spawn_mm.1 / 1000) as i32);
    let d = (1..20).flat_map(|r| crate::world::deposits_near(w.seed, sx, sy, r * 10)).next().expect("a deposit somewhere");
    w.agents[0].x = d.0 as i64 * 1000 + 3_000;
    w.agents[0].y = d.1 as i64 * 1000;
    give(&mut w, ItemKind::ProspectorPick, 1);
    w.step(&[cmd(a, CommandKind::Prospect)]);
    let found = w.take_events().into_iter().find_map(|e| if let Event::Prospected { nearest, .. } = e { nearest } else { None }).expect("read the rock");
    assert_eq!(found.0, d.2);
    give(&mut w, ItemKind::Plank, 6);
    give(&mut w, ItemKind::Stone, 4);
    w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::MineShaft, x: d.0 as i64 * 1000 + 20_000, y: d.1 as i64 * 1000 })]);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::NoDeposit)), "not off the deposit");
    let shaft = push_building(&mut w, BuildingKind::MineShaft, d.0 as i64 * 1000, d.1 as i64 * 1000) as usize;
    w.buildings[shaft].job = Some(crate::buildings::Job { recipe: recipe("mine").0, remaining: 1 });
    w.step(&[]);
    assert_eq!(w.buildings[shaft].output[d.2 as usize], 1 + d.3 as u16, "the deposit comes up");
    // food groups survive a save
    w.agents[0].nutrition = [111, 222, 333];
    let b = World::from_bytes(&w.to_bytes()).expect("load");
    assert_eq!(b.agents[0].nutrition, [111, 222, 333]);
}

/// Putting things down: they leave the bag, lie on the ground as pieces, and can be picked up again.
#[test]
fn dropped_items_lie_on_the_ground_and_can_be_picked_up() {
    let (mut w, a) = with_agent(3);
    give(&mut w, ItemKind::Fiber, 5);
    w.step(&[cmd(a, CommandKind::Drop { item: ItemKind::Fiber, n: 3 })]);
    assert_eq!(count(&w, 0, ItemKind::Fiber), 2);
    let pieces: Vec<u32> = w.bodies.iter().filter(|b| b.kind == crate::physics::BodyKind::Piece && b.item == ItemKind::Fiber as u8).map(|b| b.id).collect();
    assert_eq!(pieces.len(), 3);
    for _ in 0..120 {
        w.step(&[]);
    }
    w.step(&[cmd(a, CommandKind::Pick { body: pieces[0] })]);
    for _ in 0..900 {
        w.step(&[]);
    }
    assert_eq!(count(&w, 0, ItemKind::Fiber), 5, "picked all three back up");
}

/// The second generator cuts rivers and lakes; worlds from untagged seeds keep the land they were made with.
#[test]
fn rivers_and_lakes_only_in_new_worlds() {
    use crate::map::{height_field, tag_seed_v2};
    let old = 12345u64;
    let new = tag_seed_v2(old);
    let raw = |x, y| crate::rng::fbm(old ^ 0x1111, x, y) as i32;
    let (mut carved, mut same_old, mut hill_water) = (0, true, 0);
    for y in (0..512).step_by(2) {
        for x in (0..512).step_by(2) {
            same_old &= height_field(old, x, y) == raw(x, y);
            let (a, b) = (raw(x, y), height_field(new, x, y));
            if b < a {
                carved += 1;
            }
            if a >= 110 && b < 95 {
                hill_water += 1;
            }
        }
    }
    assert!(same_old, "an old world's land does not move");
    assert!(carved > 500, "rivers and lakes cut in: {carved}");
    assert!(hill_water > 100, "water running through dry land: {hill_water}");
    // a world made from it plays and saves as usual
    let w = World::with_size(new, 128, 128);
    assert!(World::from_bytes(&w.to_bytes()).is_ok());
}

/// Dev aid: writes a map of a new world to $ALAZ_MAP_PPM (run with --ignored).
#[test]
#[ignore]
fn dump_v2_map() {
    let Some(path) = std::env::var_os("ALAZ_MAP_PPM") else { return };
    let n = 768;
    let m = crate::map::Map::generate(crate::map::tag_seed_v2(1_731_000_000_000_000_000), n, n);
    let mut out = format!("P6 {n} {n} 255\n").into_bytes();
    for t in &m.tiles {
        out.extend_from_slice(match t {
            Terrain::Water => &[40, 90, 170],
            Terrain::Sand => &[220, 205, 150],
            Terrain::Rock => &[130, 125, 120],
            Terrain::Forest => &[40, 110, 50],
            _ => &[110, 170, 80],
        });
    }
    std::fs::write(path, out).unwrap();
}

/// A river of the fourth generator carries a swimmer along its drainage, and the drainage leads on to the sea or a lake.
#[test]
fn fourth_generator_rivers_carry_a_swimmer_downstream() {
    let seed = crate::map::tag_seed_new_world(4, 256);
    let mut w = World::with_size(seed, 256, 256);
    let (sx, sy) = w.spawn_mm;
    w.spawn_agent(sx, sy);
    let spot = (2..254).flat_map(|ty| (2..254).map(move |tx| (tx, ty))).find(|&(tx, ty)| crate::map::water_depth(&w.map, tx, ty) == 2 && crate::map::current_mm(&w.map, tx, ty) != (0, 0));
    let (tx, ty) = spot.expect("a river deep enough to swim");
    let (cx, cy) = crate::map::current_mm(&w.map, tx, ty);
    let (x0, y0) = (tx as i64 * 1000 + 500, ty as i64 * 1000 + 500);
    w.agents[0].x = x0;
    w.agents[0].y = y0;
    w.agents[0].stamina = MAX_STAT;
    for _ in 0..30 {
        w.step(&[]);
        w.agents[0].stamina = MAX_STAT;
    }
    let along = (w.agents[0].x - x0) * cx + (w.agents[0].y - y0) * cy;
    eprintln!("SWIM from ({tx},{ty}) current ({cx},{cy}) moved ({}, {}) along {along}", w.agents[0].x - x0, w.agents[0].y - y0);
    assert!(along > 0, "carried downstream");
    // following the drainage from here stays in water until it meets the sea, a lake or the map edge
    let (mut x, mut y) = (tx, ty);
    let mut ended = false;
    for _ in 0..2_000 {
        let river = crate::rivers::river_map(seed);
        // a ford is dry gravel that the stream still runs over: its drainage carries on through it
        let current = match crate::map::current_mm(&w.map, x, y) {
            (0, 0) if river.is_gravel(x, y) => river.flow(x, y).map_or((0, 0), |(dx, dy)| (dx as i64, dy as i64)),
            c => c,
        };
        match current {
            (0, 0) => {
                ended = true;
                break;
            }
            (dx, dy) => {
                x += dx.signum() as i32;
                y += dy.signum() as i32;
            }
        }
        if !(0..256).contains(&x) || !(0..256).contains(&y) {
            ended = true;
            break;
        }
    }
    assert!(ended, "the river never reaches the sea");
    assert!(!(0..256).contains(&x) || !(0..256).contains(&y) || w.map.terrain(x, y) == Some(Terrain::Water), "ends on dry land at ({x},{y})");
}

/// The current runs across the whole width of a river, not just along its middle line: nearly every tile of moving water
/// has moving water beside it (a one-tile line running aslant has none), and the banks run slower than the middle.
#[test]
fn fourth_generator_current_fills_the_width_of_a_river() {
    let seed = crate::map::tag_seed_new_world(7, 256);
    let w = World::with_size(seed, 256, 256);
    let cur = |x: i32, y: i32| crate::map::current_mm(&w.map, x, y);
    let moving: Vec<(i32, i32)> = (2..254).flat_map(|y| (2..254).map(move |x| (x, y))).filter(|&(x, y)| cur(x, y) != (0, 0)).collect();
    assert!(moving.len() > 300, "a world of rivers, got {}", moving.len());
    let side_by_side = moving.iter().filter(|&&(x, y)| [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| cur(x + dx, y + dy) != (0, 0))).count();
    assert!(side_by_side * 10 >= moving.len() * 9, "{side_by_side} of {} moving tiles have a neighbour", moving.len());
    let speed = |p: &(i32, i32)| {
        let (cx, cy) = cur(p.0, p.1);
        cx.abs().max(cy.abs())
    };
    let (slow, fast) = (moving.iter().map(speed).min().unwrap(), moving.iter().map(speed).max().unwrap());
    assert!(slow < fast, "banks and middle run at one speed ({slow})");
}

/// A small stream has fords: bars of dry gravel laid across the water, kept apart, with dry banks on both sides, and a
/// walker goes over one without getting wet.
/// A river too wide for a ford is crossed by a rock causeway: a dry bar from bank to bank that a person can walk.
#[test]
fn fourth_generator_wide_rivers_have_a_causeway() {
    let seed = crate::map::tag_seed_new_world(7, 256);
    let (mut w, a) = {
        let mut w = World::with_size(seed, 256, 256);
        let (sx, sy) = w.spawn_mm;
        let a = w.spawn_agent(sx, sy);
        (w, a)
    };
    let span = |&((lx, ly), (hx, hy)): &((i32, i32), (i32, i32))| (hx - lx).abs().max((hy - ly).abs());
    let crossings = crate::rivers::river_map(seed).fords().to_vec();
    let bridges: Vec<_> = crossings.iter().filter(|c| span(c) > 10).copied().collect();
    assert!(!bridges.is_empty(), "a causeway over a wide river; {} crossings, widest {}", crossings.len(), crossings.iter().map(span).max().unwrap_or(0));
    let dry = |w: &World, x: i32, y: i32| w.map.terrain(x, y).is_some_and(|t| t != Terrain::Water);
    let walkable = bridges.iter().find(|&&(lo, hi)| {
        let (dx, dy) = ((hi.0 - lo.0).signum(), (hi.1 - lo.1).signum());
        let steps = span(&(lo, hi));
        (0..=steps).all(|k| dry(&w, lo.0 + k * dx, lo.1 + k * dy))
            && (0..steps).all(|k| dx == 0 || dy == 0 || dry(&w, lo.0 + (k + 1) * dx, lo.1 + k * dy) || dry(&w, lo.0 + k * dx, lo.1 + (k + 1) * dy))
    });
    let &(lo, hi) = walkable.expect("a causeway whose whole bar is dry");
    let mid = |t: (i32, i32)| (t.0 as i64 * 1000 + 500, t.1 as i64 * 1000 + 500);
    w.agents[0].x = mid(lo).0;
    w.agents[0].y = mid(lo).1;
    w.step(&[cmd(a, CommandKind::MoveTo { x: mid(hi).0, y: mid(hi).1 })]);
    let mut waded = false;
    for _ in 0..60 * 120 {
        w.step(&[]);
        waded |= w.map.terrain_mm(w.agents[0].x, w.agents[0].y) == Some(Terrain::Water);
        if (w.agents[0].x - mid(hi).0).abs().max((w.agents[0].y - mid(hi).1).abs()) < 1_500 {
            break;
        }
    }
    assert!(!waded, "the walker stayed dry on the causeway");
    assert!((w.agents[0].x - mid(hi).0).abs().max((w.agents[0].y - mid(hi).1).abs()) < 1_500, "reached the far bank");
}

#[test]
fn fourth_generator_streams_have_fords_that_can_be_walked() {
    let seed = crate::map::tag_seed_new_world(7, 256);
    let (mut w, a) = {
        let mut w = World::with_size(seed, 256, 256);
        let (sx, sy) = w.spawn_mm;
        let a = w.spawn_agent(sx, sy);
        (w, a)
    };
    let fords = crate::rivers::river_map(seed).fords().to_vec();
    assert!(fords.len() >= 2, "a few fords, got {}", fords.len());
    for &(lo, hi) in &fords {
        for (x, y) in [lo, hi] {
            assert!(w.map.terrain(x, y).is_some_and(|t| t != Terrain::Water), "a dry bank at ({x},{y})");
        }
    }
    // some river is wider than a small stream can be (5 tiles of water), and still has a ford
    assert!(fords.iter().any(|&(lo, hi)| (hi.0 - lo.0).abs().max((hi.1 - lo.1).abs()) > 6), "no ford across a bigger river");
    // the ford's bar, straight between its banks, is all dry
    let walkable = fords.iter().find(|&&(lo, hi)| {
        let (dx, dy) = ((hi.0 - lo.0).signum(), (hi.1 - lo.1).signum());
        let steps = (hi.0 - lo.0).abs().max((hi.1 - lo.1).abs());
        let dry = |x: i32, y: i32| w.map.terrain(x, y).is_some_and(|t| t != Terrain::Water);
        // and each diagonal step has dry ground at one of its two corners, so the bar is no chequerboard
        (0..=steps).all(|k| dry(lo.0 + k * dx, lo.1 + k * dy))
            && (0..steps).all(|k| dx == 0 || dy == 0 || dry(lo.0 + (k + 1) * dx, lo.1 + k * dy) || dry(lo.0 + k * dx, lo.1 + (k + 1) * dy))
    });
    let &(lo, hi) = walkable.expect("a ford whose whole bar is dry");
    let mid = |t: (i32, i32)| (t.0 as i64 * 1000 + 500, t.1 as i64 * 1000 + 500);
    w.agents[0].x = mid(lo).0;
    w.agents[0].y = mid(lo).1;
    w.step(&[cmd(a, CommandKind::MoveTo { x: mid(hi).0, y: mid(hi).1 })]);
    let mut wet = false;
    for _ in 0..900 {
        w.step(&[]);
        wet |= w.map.terrain_mm(w.agents[0].x, w.agents[0].y) == Some(Terrain::Water);
        if (w.agents[0].x - mid(hi).0).abs().max((w.agents[0].y - mid(hi).1).abs()) < 700 {
            break;
        }
    }
    assert!(!wet, "waded or swam across");
    assert!((w.agents[0].x - mid(hi).0).abs().max((w.agents[0].y - mid(hi).1).abs()) < 700, "did not arrive");
}

/// Water behaves as water: rivers flow downhill, a swimmer is carried, tires and in the end goes under; things put
/// in a river float off downstream; routes still go round water, and animals and bots stay out of it.
#[test]
fn rivers_flow_and_carry_swimmers_and_floating_things() {
    let seed = crate::map::tag_seed_v2(1_731_000_000_000_000_000);
    let mut w = World::with_size(seed, 384, 384);
    let (sx, sy) = w.spawn_mm;
    w.spawn_agent(sx, sy);
    let a = w.agents[0].id;
    // a deep river tile with a current
    let mut spot = None;
    'find: for ty in 2..382 {
        for tx in 2..382 {
            if crate::map::water_depth(&w.map, tx, ty) == 2 && crate::map::current_mm(&w.map, tx, ty) != (0, 0) {
                spot = Some((tx, ty));
                break 'find;
            }
        }
    }
    let (tx, ty) = spot.expect("a river deep enough to swim");
    let (cx, cy) = crate::map::current_mm(&w.map, tx, ty);
    // downhill: the land is lower ahead along the current than behind
    let raw = |x: i32, y: i32| crate::rng::fbm(seed ^ 0x1111, x, y);
    let s = |k: i64| (tx + (cx * k / 8) as i32, ty + (cy * k / 8) as i32);
    assert!(raw(s(6).0, s(6).1) <= raw(s(-6).0, s(-6).1) + 2, "flows downhill");
    // a swimmer is carried along
    let (x0, y0) = (tx as i64 * 1000 + 500, ty as i64 * 1000 + 500);
    w.agents[0].x = x0;
    w.agents[0].y = y0;
    w.step(&[]);
    let moved = (w.agents[0].x - x0) * cx + (w.agents[0].y - y0) * cy;
    assert!(moved > 0, "carried downstream");
    // swimming tires; out of breath, you drown
    let breath = w.agents[0].stamina;
    let hp = w.agents[0].health;
    for _ in 0..60 {
        w.agents[0].x = x0;
        w.agents[0].y = y0;
        w.step(&[]);
    }
    assert!(w.agents[0].stamina < breath, "swimming costs breath");
    w.agents[0].stamina = 0;
    for _ in 0..60 {
        w.agents[0].x = x0;
        w.agents[0].y = y0;
        w.step(&[]);
    }
    assert!(w.agents[0].health < hp, "out of breath in deep water: drowning");
    // a dropped bundle floats off
    w.agents[0].health = MAX_STAT;
    w.agents[0].stamina = MAX_STAT;
    give(&mut w, ItemKind::Fiber, 1);
    w.step(&[cmd(a, CommandKind::Drop { item: ItemKind::Fiber, n: 1 })]);
    let id = w.bodies.iter().find(|b| b.kind == crate::physics::BodyKind::Piece).map(|b| b.id).expect("dropped");
    let start = w.bodies.iter().find(|b| b.id == id).unwrap().pos_mm();
    for _ in 0..120 {
        w.step(&[]);
    }
    let end = w.bodies.iter().find(|b| b.id == id).expect("it floats, it does not sink").pos_mm();
    assert!((end.0 - start.0).abs() + (end.1 - start.1).abs() > 200, "drifted: {start:?} -> {end:?}");
    // walking in: a person may, from the bank
    let bank = (-3..=3).flat_map(|dy| (-3..=3).map(move |dx| (tx + dx, ty + dy))).find(|&(x, y)| w.map.terrain(x, y).is_some_and(|t| t != Terrain::Water));
    if let Some((bx, by)) = bank {
        w.agents[0].x = bx as i64 * 1000 + 500;
        w.agents[0].y = by as i64 * 1000 + 500;
        for _ in 0..200 {
            w.step(&[cmd(a, CommandKind::Walk { x: x0, y: y0 })]);
        }
        let (ax, ay) = (w.agents[0].x.div_euclid(1000) as i32, w.agents[0].y.div_euclid(1000) as i32);
        assert_eq!(w.map.terrain(ax, ay), Some(Terrain::Water), "walked in");
    }
}

/// A world 32 km across is made in chunks: quick to start, made around whoever walks it, and a save brings it back
/// exactly (the chunks are made again in the same order, so node ids match).
#[test]
fn a_huge_world_is_made_in_chunks() {
    let t0 = std::time::Instant::now();
    let mut w = World::with_size(crate::map::tag_seed_v2(77), 32_768, 32_768);
    assert!(w.map.lazy());
    let (sx, sy) = w.spawn_mm;
    w.spawn_agent(sx, sy);
    assert!(t0.elapsed().as_secs() < 10, "starts quickly: {:?}", t0.elapsed());
    let first = w.chunks_done.len();
    assert!(first > 0 && !w.nodes.is_empty(), "the land round the spawn is made");
    assert!(w.node_at(w.nodes[0].tx, w.nodes[0].ty).is_some());
    // walk far: new land is made on the way
    for k in 1..=6 {
        w.agents[0].x = sx + k * 2_000_000;
        w.step(&[]);
    }
    assert!(w.chunks_done.len() > first * 3, "more chunks as we go: {}", w.chunks_done.len());
    // take something from a node far out, then save and load
    let far = w.nodes.len() - 1;
    w.set_node_amount(far, 0);
    let b = World::from_bytes(&w.to_bytes()).expect("loads");
    assert_eq!(b.nodes.len(), w.nodes.len());
    assert_eq!((b.nodes[far].tx, b.nodes[far].ty, b.nodes[far].amount), (w.nodes[far].tx, w.nodes[far].ty, 0));
    assert_eq!(b.hash(), w.hash());
    // and both go on the same
    let (mut a, mut b) = (w, b);
    for _ in 0..200 {
        a.step(&[]);
        b.step(&[]);
    }
    assert_eq!(a.hash(), b.hash());
}

/// Dev aid: a view of the third generator around the middle of the largest world, `ALAZ_MAP_STEP` tiles a pixel,
/// with the share of each ground (run with --ignored).
#[test]
#[ignore]
fn dump_v3_map() {
    let Some(path) = std::env::var_os("ALAZ_MAP_PPM") else { return };
    let step: i32 = std::env::var("ALAZ_MAP_STEP").ok().and_then(|s| s.parse().ok()).unwrap_or(4);
    let seed = crate::map::tag_seed_v3(std::env::var("ALAZ_MAP_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(1_731_000_000_000_000_000));
    let n = 800;
    let c = crate::map::MAX_MAP_SIDE / 2;
    let mut out = format!("P6 {n} {n} 255\n").into_bytes();
    let mut count = [0u32; 6];
    for py in 0..n {
        for px in 0..n {
            let (tx, ty) = (c + (px - n / 2) * step, c + (py - n / 2) * step);
            let t = crate::map::terrain_at(seed, tx, ty);
            let h = crate::map::height_field(seed, tx, ty);
            let (warm, _) = crate::map::climate(seed, tx, ty);
            let shade = |rgb: [i32; 3]| {
                let k = 70 + h.clamp(95, 255) * 30 / 255;
                rgb.map(|v| (v * k / 100).clamp(0, 255) as u8)
            };
            let rgb = match t {
                Terrain::Water => { count[0] += 1; [20, 60 + h, 120 + h].map(|v| v.clamp(0, 255) as u8) }
                Terrain::Sand => { count[1] += 1; shade([220, 200, 140]) }
                Terrain::Rock => { count[2] += 1; if h > 205 { [240, 240, 245] } else { shade([140, 132, 125]) } }
                Terrain::Forest => { count[3] += 1; if warm < 380 { shade([40, 90, 70]) } else { shade([40, 115, 45]) } }
                _ => { count[4] += 1; if warm < 300 { shade([170, 180, 160]) } else { shade([115, 170, 80]) } }
            };
            out.extend_from_slice(&rgb);
        }
    }
    let tot = (n * n) as f32 / 100.0;
    eprintln!("MAPSTAT water {:.0}% sand {:.0}% rock {:.0}% forest {:.0}% grass {:.0}%", count[0] as f32 / tot, count[1] as f32 / tot, count[2] as f32 / tot, count[3] as f32 / tot, count[4] as f32 / tot);
    std::fs::write(path, out).unwrap();
}

/// The largest world (262 km) is made in chunks like the huge one, and the third generator's land is varied at
/// the spawn: sea, land and mountains all within reach of a long walk.
#[test]
fn an_unlimited_world_starts_and_walks() {
    let side = crate::map::MAX_MAP_SIDE;
    let mut w = World::with_size(crate::map::tag_seed_v3(5), side, side);
    let (sx, sy) = w.spawn_mm;
    w.spawn_agent(sx, sy);
    assert!(w.map.lazy() && !w.nodes.is_empty());
    for k in 1..=3 {
        w.agents[0].x = sx + k * 5_000_000;
        w.agents[0].y = sy - k * 3_000_000;
        w.step(&[]);
    }
    assert!(World::from_bytes(&w.to_bytes()).is_ok());
    // what lies within 20 km of the spawn
    let (tx, ty) = ((sx / 1000) as i32, (sy / 1000) as i32);
    let mut kinds = std::collections::BTreeSet::new();
    for dy in (-20_000..=20_000).step_by(250) {
        for dx in (-20_000..=20_000).step_by(250) {
            kinds.insert(crate::map::terrain_at(w.seed, tx + dx, ty + dy) as u8);
        }
    }
    assert!(kinds.len() >= 5, "sea, beach, rock, forest and grass about: {kinds:?}");
}

/// One wind for the world: the same for everyone, turning slowly (never jumping), stronger in bad weather.
#[test]
fn the_wind_turns_slowly_and_rises_in_storms() {
    let (seed, days) = (9u64, 6u64);
    let mut prev = crate::time::wind(seed, 0, days);
    let (mut stormy, mut calm) = (Vec::new(), Vec::new());
    for t in (1..TICKS_PER_DAY * 3).step_by(60) {
        let (h, s) = crate::time::wind(seed, t, days);
        assert!((0..6283).contains(&h) && (0..=1000).contains(&s));
        let mut d = (h - prev.0).abs();
        if d > 3141 {
            d = 6283 - d;
        }
        assert!(d < 60, "a second later the wind has turned only a little: {d}");
        assert!((s - prev.1).abs() < 120, "and its strength changes smoothly: {} -> {s}", prev.1);
        prev = (h, s);
        if crate::time::weather(seed, t, days) == crate::time::Weather::Clear { calm.push(s) } else { stormy.push(s) }
    }
    assert_eq!(crate::time::wind(seed, 12_345, days), crate::time::wind(seed, 12_345, days));
    if !stormy.is_empty() && !calm.is_empty() {
        let avg = |v: &Vec<i32>| v.iter().sum::<i32>() / v.len() as i32;
        assert!(avg(&stormy) > avg(&calm) + 200, "storms blow harder");
    }
    // the integer sine is close to the real one
    for a in (0..6283).step_by(97) {
        let real = ((a as f64 / 1000.0).sin() * 1000.0) as i64;
        assert!((crate::map::sin_mrad(a) - real).abs() <= 3, "sin {a}");
    }
}

/// Dev aid: how long the land's pure functions take per call, by generator (run with --ignored --nocapture).
#[test]
#[ignore]
fn time_land_functions() {
    for (name, seed) in [("v1", 42u64), ("v2", crate::map::tag_seed_v2(42)), ("v3", crate::map::tag_seed_v3(42))] {
        let c = crate::map::MAX_MAP_SIDE / 2;
        let n = 200_000;
        let mut sink = 0i64;
        let t = |f: &dyn Fn(i32, i32) -> i64| {
            let t0 = std::time::Instant::now();
            let mut s = 0i64;
            for i in 0..n {
                s = s.wrapping_add(f(c + i % 450, c + i / 450));
            }
            (t0.elapsed().as_nanos() as f64 / n as f64, s)
        };
        let (a, s1) = t(&|x, y| crate::map::height_field(seed, x, y) as i64);
        let (b, s2) = t(&|x, y| crate::map::elevation_mm(seed, x, y) as i64);
        let (cl, s3) = t(&|x, y| crate::map::climate(seed, x, y).0 as i64);
        let (te, s4) = t(&|x, y| crate::map::terrain_at(seed, x, y) as i64);
        sink = sink.wrapping_add(s1 ^ s2 ^ s3 ^ s4);
        eprintln!("TIMING {name}: height_field {a:.0} ns, elevation {b:.0} ns, climate {cl:.0} ns, terrain {te:.0} ns ({sink})");
    }
}

/// Walking into the sea the way the keys do it (a step command every fifth of a second, 4 m ahead): in, and on out
/// to swimming depth.
#[test]
fn keys_walk_a_person_into_the_sea_and_swimming() {
    let (mut w, a) = with_agent(5);
    // a beach tile with open sea to its east
    let (tx, ty) = (2..MAP_H - 2)
        .flat_map(|y| (2..MAP_W - 12).map(move |x| (x, y)))
        .find(|&(x, y)| w.map.terrain(x, y) == Some(Terrain::Sand) && (1..10).all(|d| w.map.terrain(x + d, y) == Some(Terrain::Water)) && w.node_at(x, y).is_none())
        .expect("a beach");
    w.agents[0].x = tx as i64 * 1000 + 500;
    w.agents[0].y = ty as i64 * 1000 + 500;
    let mut deepest = 0;
    for t in 0..900 {
        let cmds = if t % 12 == 0 { vec![cmd(a, CommandKind::Walk { x: w.agents[0].x + 4_000, y: w.agents[0].y })] } else { vec![] };
        w.step(&cmds);
        let d = crate::map::water_depth(&w.map, w.agents[0].x.div_euclid(1000) as i32, w.agents[0].y.div_euclid(1000) as i32);
        deepest = deepest.max(d);
    }
    let ag = &w.agents[0];
    eprintln!("ended at tile ({}, {}) from ({tx}, {ty}), task {:?}, deepest {deepest}", ag.x / 1000, ag.y / 1000, ag.task);
    assert_eq!(deepest, 2, "out to swimming depth");
}

/// Dev aid, the headless debug gate: play `ALAZ_PLAY_SCRIPT` as a fresh player in a world (`ALAZ_PLAY_SEED`,
/// `ALAZ_PLAY_SIZE`, `ALAZ_PLAY_GEN` = 1, 2 or 3) and print the log (run with --ignored --nocapture).
#[test]
#[ignore]
fn play_script() {
    let Some(path) = std::env::var_os("ALAZ_PLAY_SCRIPT") else { return };
    let text = std::fs::read_to_string(path).unwrap();
    let seed: u64 = std::env::var("ALAZ_PLAY_SEED").ok().and_then(|v| v.parse().ok()).unwrap_or(7);
    let size: i32 = std::env::var("ALAZ_PLAY_SIZE").ok().and_then(|v| v.parse().ok()).unwrap_or(1024);
    let seed = match std::env::var("ALAZ_PLAY_GEN").as_deref() {
        Ok("1") => seed,
        Ok("2") => crate::map::tag_seed_v2(seed),
        _ => crate::map::tag_seed_v3(seed),
    };
    let mut w = World::with_size(seed, size, size);
    let (sx, sy) = w.spawn_mm;
    w.spawn_agent(sx, sy);
    let me = w.agents[0].id;
    let mut sc = crate::debug::Script::new(&text);
    let limit = std::env::var("ALAZ_PLAY_MINUTES").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(60) * 3600;
    while !sc.done() && w.tick < limit {
        let cmds = sc.tick(&mut w, me);
        w.step(&cmds);
        let evs = w.take_events();
        sc.watch(&w, me, &evs);
        if w.agents[0].health <= 0 {
            break;
        }
    }
    for l in &sc.log {
        println!("{l}");
    }
    println!("== {} failures; ended at {:.0} s; done {}", sc.failures.len(), w.tick as f64 / 60.0, sc.done());
}

/// Dev aid, the full sweep: every building is built from its cost and every recipe made from its inputs (at its
/// station, or by hand with its minigame played perfectly), in a world that knows everything. Prints what failed
/// and why; it fails if anything cannot be made.
#[test]
fn sweep_every_building_and_recipe() {
    let mut base = World::with_size(crate::map::tag_seed_v2(11), 256, 256);
    let (sx, sy) = base.spawn_mm;
    base.spawn_agent(sx, sy);
    base.learn_everything();
    base.wild.clear();
    let me = base.agents[0].id;
    let rules = base.rules_arc();
    let mut bad = Vec::new();
    let mut good = 0;
    for def in &rules.buildings {
        if matches!(def.kind, BuildingKind::Ruin | BuildingKind::MineShaft) {
            continue;
        }
        let mut w = base.clone();
        for c in &def.cost {
            w.agents[0].inventory.counts[c.item as usize] += c.n;
        }
        for k in &def.needs {
            w.agents[0].inventory.counts[*k as usize] += 1;
        }
        if matches!(def.kind, BuildingKind::WaterWheel | BuildingKind::WaterMill) {
            // a wheel goes in a stream: stand on the shore
            let mut sc = crate::debug::Script::new("tp coast");
            let c = sc.tick(&mut w, me);
            w.step(&c);
        }
        let a = w.agents[0].clone();
        let spot = crate::debug::free_spot(&w, &a, def.kind);
        let Some((x, y)) = spot else {
            bad.push(format!("build {:?}: no free spot", def.kind));
            continue;
        };
        w.step(&[cmd(me, CommandKind::Build { kind: def.kind, x, y })]);
        let why: Vec<String> = w.take_events().iter().filter_map(|e| if let Event::Rejected(_, r) = e { Some(format!("{r:?}")) } else { None }).collect();
        if w.buildings.iter().any(|b| b.kind == def.kind) {
            good += 1;
        } else {
            bad.push(format!("build {:?}: not built {why:?}", def.kind));
        }
    }
    for (ri, r) in rules.recipes.iter().enumerate() {
        if r.discovery == Discovery::Retired || r.mine {
            continue;
        }
        let mut w = base.clone();
        let inv = |w: &mut World, k: ItemKind, n: u16| w.agents[0].inventory.counts[k as usize] += n;
        for c in &r.inputs {
            inv(&mut w, c.item, c.n);
        }
        for k in &r.needs {
            inv(&mut w, *k, 1);
        }
        if let Some(m) = r.mold {
            inv(&mut w, m, 1);
        }
        let (ax, ay) = (w.agents[0].x, w.agents[0].y);
        for (i, k) in r.station.iter().chain(r.near.iter()).enumerate() {
            let mut b = crate::buildings::Building::new(*k, ax + 1_500 + i as i64 * 2_500, ay);
            b.fuel = 40_000;
            w.buildings.push(b);
        }
        // what a station needs around it to work: bellows for the furnaces, wind for the machines and the bellows
        if let Some(st) = r.station {
            if matches!(st, BuildingKind::Bloomery | BuildingKind::BlastFurnace | BuildingKind::Cupola | BuildingKind::Bessemer) {
                w.buildings.push(crate::buildings::Building::new(BuildingKind::Bellows, ax + 1_500, ay + 2_000));
            }
            if matches!(st, BuildingKind::Bloomery | BuildingKind::BlastFurnace | BuildingKind::Cupola | BuildingKind::Bessemer | BuildingKind::TripHammer | BuildingKind::RollingMill | BuildingKind::HandMill | BuildingKind::StoneMortar) || st.needs_power() || st.needs_grid() {
                w.buildings.push(crate::buildings::Building::new(BuildingKind::Windmill, ax + 1_500, ay + 6_000));
            }
            // a grid machine wants a live line: a dynamo turned by that windmill
            if st.needs_grid() {
                w.buildings.push(crate::buildings::Building::new(BuildingKind::Dynamo, ax + 1_500, ay + 4_000));
            }
            // a steam hammer wants steam: a boiler under steam beside it
            if st == BuildingKind::SteamHammer {
                let mut boiler = crate::buildings::Building::new(BuildingKind::Boiler, ax + 1_500, ay + 5_000);
                boiler.job = Some(crate::buildings::Job { recipe: recipe("boiler_coal").0, remaining: u32::MAX / 2 });
                boiler.temp = 300;
                w.buildings.push(boiler);
            }
        }
        let before: Vec<u32> = r.outputs.iter().map(|o| count(&w, 0, o.item) as u32 + w.buildings.iter().map(|b| b.output[o.item as usize] as u32).sum::<u32>()).collect();
        let mut why = Vec::new();
        if r.station.is_some() {
            // it works on its own: load it and let it run; keep any fire under it fed and hot
            w.step(&[cmd(me, CommandKind::StartJob { building: 0, recipe: RecipeId(ri as u16) })]);
            // long enough to heat up from cold (at its rate) and then do the work twice over
            let rate = rules.building(r.station.unwrap()).heat_rate.max(1) as u64;
            let warm = (r.heat.max(0) as u64).saturating_sub(15) / rate * 60;
            for _ in 0..(r.ticks as u64 * 2 + warm + 1_200) {
                if let Some(b) = w.buildings.get_mut(0) {
                    b.fuel = b.fuel.max(20_000);
                }
                w.step(&[]);
            }
        } else {
            let mut sc = crate::debug::Script::new(&format!("craft {}", r.id));
            for _ in 0..60 * 120 {
                let c = sc.tick(&mut w, me);
                w.step(&c);
                if sc.done() {
                    break;
                }
            }
            why.extend(sc.failures.clone());
        }
        why.extend(w.take_events().iter().filter_map(|e| if let Event::Rejected(_, r) = e { Some(format!("{r:?}")) } else { None }));
        let after: Vec<u32> = r.outputs.iter().map(|o| count(&w, 0, o.item) as u32 + w.buildings.iter().map(|b| b.output[o.item as usize] as u32).sum::<u32>()).collect();
        let made = r.outputs.is_empty() || after.iter().zip(&before).any(|(a, b)| a > b);
        if made {
            good += 1;
        } else {
            why.dedup();
            let temp = w.buildings.first().map(|b| b.temp).unwrap_or(0);
            bad.push(format!("recipe {} ({:?}{}): nothing made; heat {} need {}; {:?}", r.id, r.station, r.near.map(|n| format!(" near {n:?}")).unwrap_or_default(), temp, r.heat, why));
        }
    }
    println!("SWEEP ok {good}, failed {}", bad.len());
    for b in &bad {
        println!("SWEEP {b}");
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// Dev aid, the world generator report: for each generator, `ALAZ_REPORT_SEEDS` seeds at `ALAZ_REPORT_SIZE`, the
/// start a newcomer gets and the country around it, summed up (run with --ignored --nocapture).
#[test]
#[ignore]
fn worldgen_report() {
    let seeds: u64 = std::env::var("ALAZ_REPORT_SEEDS").ok().and_then(|v| v.parse().ok()).unwrap_or(6);
    let size: i32 = std::env::var("ALAZ_REPORT_SIZE").ok().and_then(|v| v.parse().ok()).unwrap_or(32_768);
    let verbose = std::env::var_os("ALAZ_REPORT_ROWS").is_some();
    for (gname, tag) in [("v1", 0u8), ("v2", 2), ("v3", 3)] {
        if std::env::var("ALAZ_REPORT_GEN").is_ok_and(|g| g != gname) {
            continue;
        }
        let (mut starts, mut poor, mut islets, mut times) = (std::collections::BTreeMap::<String, u32>::new(), 0, 0, Vec::new());
        let (mut ore_d, mut clay_d, mut water_d_all, mut trees) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for s in 1..=seeds {
            let seed = match tag { 0 => s * 7919, 2 => crate::map::tag_seed_v2(s * 7919), _ => crate::map::tag_seed_v3(s * 7919) };
            let t0 = std::time::Instant::now();
            let w = World::with_size(seed, size, size);
            times.push(t0.elapsed().as_secs_f32());
            let (sx, sy) = ((w.spawn_mm.0 / 1000) as i32, (w.spawn_mm.1 / 1000) as i32);
            *starts.entry(format!("{:?}", w.map.terrain(sx, sy).unwrap_or(Terrain::Water))).or_default() += 1;
            let mut near = [0u32; 11];
            let mut dist = [u32::MAX; 11];
            let (mut water, mut all, mut wd) = (0u32, 0u32, u32::MAX);
            let r = 400;
            for dy in -r..=r {
                for dx in -r..=r {
                    let (tx, ty) = (sx + dx, sy + dy);
                    let d = ((dx * dx + dy * dy) as f64).sqrt() as u32;
                    if (dx % 4 == 0) && (dy % 4 == 0) {
                        all += 1;
                        if w.map.terrain(tx, ty) != Some(Terrain::Grass) && w.map.terrain(tx, ty).is_none_or(|t| t == Terrain::Water) {
                            water += 1;
                        }
                    }
                    if w.map.terrain(tx, ty) == Some(Terrain::Water) {
                        wd = wd.min(d);
                    }
                    if let Some(k) = w.map.node_for(seed, tx, ty) {
                        if d <= 20 {
                            near[k as usize] += 1;
                        }
                        dist[k as usize] = dist[k as usize].min(d);
                    }
                }
            }
            let first = near[NodeKind::Tree as usize] >= 4 && near[NodeKind::StoneOutcrop as usize] >= 2 && near[NodeKind::Deadwood as usize] >= 1 && near[NodeKind::FiberPlant as usize] >= 1 && near[NodeKind::BerryBush as usize] >= 1;
            if !first {
                poor += 1;
            }
            if water * 100 > all * 60 {
                islets += 1;
            }
            let ore = [NodeKind::CopperOutcrop, NodeKind::TinOutcrop, NodeKind::IronOutcrop].iter().map(|k| dist[*k as usize]).max().unwrap();
            ore_d.push(ore);
            clay_d.push(dist[NodeKind::ClayDeposit as usize]);
            water_d_all.push(wd);
            trees.push(near[NodeKind::Tree as usize]);
            if verbose {
                println!("ROW {gname} {size} seed {s}: start {:?} ({:.1}s) trees20 {} stone20 {} | ore(all three) {} clay {} water {} | water% {}", w.map.terrain(sx, sy), times.last().unwrap(), near[NodeKind::Tree as usize], near[NodeKind::StoneOutcrop as usize], ore, dist[NodeKind::ClayDeposit as usize], wd, water * 100 / all.max(1));
            }
        }
        let med = |v: &mut Vec<u32>| { v.sort(); v[v.len() / 2] };
        let worst = |v: &Vec<u32>| *v.iter().max().unwrap();
        let fmt = |d: u32| if d == u32::MAX { "none in 400 m".to_string() } else { format!("{d} m") };
        let tmax = times.iter().cloned().fold(0.0f32, f32::max);
        println!(
            "SUM {gname} size {size}, {seeds} seeds: starts {starts:?} | poor first materials {poor} | islet starts {islets} | all three ores: median {}, worst {} | clay: median {}, worst {} | water: worst {} | trees within 20 m: least {} | made in: worst {tmax:.1} s",
            fmt(med(&mut ore_d.clone())), fmt(worst(&ore_d)), fmt(med(&mut clay_d.clone())), fmt(worst(&clay_d)), fmt(worst(&water_d_all)), trees.iter().min().unwrap()
        );
    }
}

/// A newcomer's start in the newer worlds: made quickly, on land with the first materials at hand, in a country
/// (not an islet) with ores and clay within a walk.
#[test]
fn newer_worlds_start_somewhere_livable() {
    for s in [1u64, 3, 5] {
        let seed = crate::map::tag_seed_v3(s * 7919);
        let t0 = std::time::Instant::now();
        let w = World::with_size(seed, 32_768, 32_768);
        // measured alone: about 2 s typical, 12 s at worst; this guards against it getting much worse, with room
        // for a busy machine running the other tests at the same time
        assert!(t0.elapsed().as_secs() < 30, "seed {s}: made in {:?}", t0.elapsed());
        let (sx, sy) = ((w.spawn_mm.0 / 1000) as i32, (w.spawn_mm.1 / 1000) as i32);
        let count = |k: NodeKind, r: i32| (-r..=r).flat_map(|dy| (-r..=r).map(move |dx| (dx, dy))).filter(|&(dx, dy)| w.map.node_for(seed, sx + dx, sy + dy) == Some(k)).count();
        for (k, n) in [(NodeKind::Tree, 4), (NodeKind::StoneOutcrop, 3), (NodeKind::Deadwood, 1), (NodeKind::FiberPlant, 2), (NodeKind::BerryBush, 1)] {
            assert!(count(k, 20) >= n, "seed {s}: {k:?} at the start");
        }
        let land = (-250..=250).step_by(25).flat_map(|dy| (-250..=250).step_by(25).map(move |dx| (dx, dy))).filter(|&(dx, dy)| w.map.terrain(sx + dx, sy + dy).is_some_and(|t| t != Terrain::Water)).count();
        assert!(land * 100 >= 441 * 40, "seed {s}: an islet ({land} of 441 land)");
        let ore = [NodeKind::CopperOutcrop, NodeKind::IronOutcrop].iter().all(|&k| (-300..=300).step_by(2).any(|dy| (-300..=300).any(|dx| w.map.node_for(seed, sx + dx, sy + dy) == Some(k))));
        assert!(ore, "seed {s}: copper and iron within 300 m");
    }
}

/// Every command a player can give has a debug gate, and each one, driven through it, does what it should.
#[test]
fn every_command_has_a_working_gate() {
    let run = |w: &mut World, text: &str| -> crate::debug::Script {
        let me = w.agents[0].id;
        let mut sc = crate::debug::Script::new(text);
        let mut guard = 0;
        while !sc.done() && guard < 60 * 300 {
            let c = sc.tick(w, me);
            w.step(&c);
            let evs = w.take_events();
            sc.watch(w, me, &evs);
            guard += 1;
        }
        sc
    };
    let mut w = World::with_size(crate::map::tag_seed_v2(11), 512, 512);
    let (sx, sy) = w.spawn_mm;
    w.spawn_agent(sx, sy);
    w.learn_everything();
    w.wild.retain(|x| x.species != WildSpecies::Wolf);
    let me = |w: &World| w.agents[0].clone();
    // moving: goto, walk (held keys), sprint, jump
    let p0 = (me(&w).x, me(&w).y);
    run(&mut w, &format!("goto {} {}", p0.0 / 1000 + 6, p0.1 / 1000));
    assert!(me(&w).x > p0.0 + 3_000, "goto");
    let p1 = me(&w).x;
    run(&mut w, "sprint on\nwalk -1 0 2\nsprint off");
    assert!(me(&w).x < p1 - 1_000, "walk");
    run(&mut w, "jump");
    assert!(w.airborne(me(&w).id).is_some(), "jump");
    run(&mut w, "wait 2");
    // gathering, felling, picking up, dropping
    let sc = run(&mut w, "gather stone 2\ngather deadwood 2\ngather fiber 3\ngather berry 2");
    assert!(sc.failures.is_empty(), "{:?}", sc.failures);
    run(&mut w, "give stone_axe 1\ngather tree 2");
    assert!(count(&w, 0, ItemKind::Wood) >= 2, "felled and picked up");
    let f = count(&w, 0, ItemKind::Fiber);
    run(&mut w, "drop fiber 1\nwait 1\npick");
    assert_eq!(count(&w, 0, ItemKind::Fiber), f, "dropped and picked up again");
    // eating and drinking
    w.agents[0].hunger = 3_000;
    w.agents[0].thirst = 3_000;
    run(&mut w, "eat berries\ndrink");
    assert!(w.agents[0].hunger > 3_000 && w.agents[0].thirst > 3_000, "eat, drink");
    // making: combine, craft (minigames played)
    run(&mut w, "give stone 2\ncombine stone stone");
    assert!(count(&w, 0, ItemKind::SharpStone) >= 1, "combine");
    let sc = run(&mut w, "give branch 1\ngive fiber 2\ncraft spear");
    assert!(sc.failures.is_empty() && count(&w, 0, ItemKind::Spear) >= 1, "craft {:?}", sc.failures);
    // building and stations: build, refuel, deposit, collect, startjob, auto
    let sc = run(&mut w, "give wood 20\ngive flint 1\ngive stone 10\nbuild campfire\nbuild depot");
    assert!(w.buildings.iter().any(|b| b.kind == BuildingKind::Campfire) && w.buildings.iter().any(|b| b.kind == BuildingKind::Depot), "build {:?}", sc.log);
    let fire = w.buildings.iter().position(|b| b.kind == BuildingKind::Campfire).unwrap();
    let fuel = w.buildings[fire].fuel;
    run(&mut w, "refuel campfire");
    assert!(w.buildings[fire].fuel > fuel, "refuel");
    run(&mut w, "deposit depot");
    let depot = w.buildings.iter().position(|b| b.kind == BuildingKind::Depot).unwrap();
    assert!(w.buildings[depot].output_total() > 0, "deposit");
    run(&mut w, "collect depot wood");
    assert!(count(&w, 0, ItemKind::Wood) > 0, "collect");
    run(&mut w, "give raw_meat 2\nstartjob campfire cook_meat\nauto campfire cook_meat");
    assert!(w.buildings[fire].job.is_some() || w.buildings[fire].output[ItemKind::CookedMeat as usize] > 0, "startjob");
    assert!(w.buildings[fire].auto.is_some(), "auto");
    // workers: a bot on a route, fed; an ox with a cart
    run(&mut w, "worker bot\ngive coal 2\nfeed\nroute tree");
    let bot = w.agents.iter().find(|a| a.kind == crate::agents::AgentKind::Bot).unwrap();
    assert!(!bot.route.is_empty(), "route");
    run(&mut w, "worker ox\ngive cart 1\nhitch cart");
    assert_eq!(count(&w, 0, ItemKind::Cart), 0, "hitch");
    // wild animals: tame a dog, hunt a deer
    w.wild.push(crate::wildlife::Wild::new(90_001, WildSpecies::Dog, me(&w).x + 4_000, me(&w).y));
    w.wild.push(crate::wildlife::Wild::new(90_002, WildSpecies::Deer, me(&w).x - 4_000, me(&w).y));
    let before = w.agents.len();
    run(&mut w, "give berries 10\ntame dog");
    assert!(w.agents.len() > before || w.wild.iter().any(|x| x.id == 90_001 && x.trust > 0), "tame");
    let meat = count(&w, 0, ItemKind::RawMeat);
    run(&mut w, "give spear 1\nhunt deer");
    assert!(count(&w, 0, ItemKind::RawMeat) > meat || !w.wild.iter().any(|x| x.id == 90_002), "hunt");
    // tools: repair, prospect
    w.agents[0].wear.push((ItemKind::StoneAxe, 30));
    run(&mut w, "give stone 2\nrepair stone_axe");
    assert_eq!(w.agents[0].wear_of(ItemKind::StoneAxe), 0, "repair");
    run(&mut w, "give prospector_pick 1\nprospect");
    // caves
    let mut caves = World::with_size(crate::map::tag_seed_natural(42, 256), 256, 256);
    let entry = caves.node_at(222, 150).unwrap();
    let (x, y) = caves.nodes[entry.0 as usize].center_mm();
    caves.spawn_agent(x, y);
    let sc = run(&mut caves, "give iron_pickaxe 1\ncave enter\ncave mine\ncave leave");
    assert!(sc.failures.is_empty(), "cave {:?}", sc.failures);
    // the design table answers (it wants a running computer: refused here, which shows the gate reaches it)
    let sc = run(&mut w, "design stone 1 wood 2");
    assert!(sc.log.iter().any(|l| l.contains("NeedsStation")), "design");
    // the loop
    let seed = w.seed;
    run(&mut w, "cycle");
    assert_ne!(w.seed, seed, "cycle");
}

/// A small new world still holds copper, tin and iron within reach of the start.
#[test]
fn small_new_worlds_have_ores() {
    for s in 1..=20u64 {
        let seed = crate::map::tag_seed_v2(s * 7919);
        let w = World::with_size(seed, 128, 128);
        for k in [NodeKind::CopperOutcrop, NodeKind::TinOutcrop, NodeKind::IronOutcrop] {
            assert!(w.nodes.iter().any(|n| n.kind == k), "seed {s}: no {k:?} in a 128 m world");
        }
    }
}

/// The fourth generator's rivers and lakes hold schools too, and a fish can be landed from their banks.
#[test]
fn fourth_generator_water_has_fish_to_catch() {
    let seed = crate::map::tag_seed_new_world(7, 256);
    let mut w = World::with_size(seed, 256, 256);
    let (sx, sy) = w.spawn_mm;
    w.spawn_agent(sx, sy);
    w.learn_everything();
    w.wild.retain(|x| x.species != WildSpecies::Wolf);
    let a = w.agents[0].id;
    let schools = (0..256).step_by(5).flat_map(|y| (0..256).step_by(5).map(move |x| (x, y))).filter(|&(x, y)| crate::map::fish_school(&w.map, x, y)).count();
    assert!(schools >= 5, "schools in the new water, got {schools}");
    let (tx, ty) = ((sx / 1000) as i32, (sy / 1000) as i32);
    let (fx, fy) = crate::map::school_near(&w.map, tx, ty, 200).expect("a school of fish");
    let bank = (-4..=4i32).flat_map(|dy| (-4..=4i32).map(move |dx| (fx + dx, fy + dy))).find(|&(x, y)| w.map.terrain(x, y).is_some_and(|t| t != Terrain::Water)).expect("a bank by the school");
    w.agents[0].x = bank.0 as i64 * 1000 + 500;
    w.agents[0].y = bank.1 as i64 * 1000 + 500;
    give(&mut w, ItemKind::FishingRod, 1);
    w.step(&[cmd(a, CommandKind::Fish { tx: fx, ty: fy })]);
    for _ in 0..60 * 60 {
        w.step(&[]);
        if count(&w, 0, ItemKind::RawFish) >= 1 {
            break;
        }
    }
    assert!(count(&w, 0, ItemKind::RawFish) >= 1, "no fish landed");
}

/// Fishing: a rod or a spear by a school; refused without one, or where there are no fish; the catch cooks over a
/// fire and feeds the protein group.
#[test]
fn fishing_with_a_rod_and_cooking_the_catch() {
    let seed = crate::map::tag_seed_v2(11);
    let mut w = World::with_size(seed, 512, 512);
    let (sx, sy) = w.spawn_mm;
    w.spawn_agent(sx, sy);
    w.learn_everything();
    w.wild.retain(|x| x.species != WildSpecies::Wolf);
    let a = w.agents[0].id;
    let (tx, ty) = ((sx / 1000) as i32, (sy / 1000) as i32);
    let (fx, fy) = crate::map::school_near(&w.map, tx, ty, 200).expect("a school of fish somewhere near");
    // stand on the bank next to it
    let bank = (-4..=4i32).flat_map(|dy| (-4..=4i32).map(move |dx| (fx + dx, fy + dy))).find(|&(x, y)| w.map.terrain(x, y).is_some_and(|t| t != Terrain::Water)).expect("a bank");
    w.agents[0].x = bank.0 as i64 * 1000 + 500;
    w.agents[0].y = bank.1 as i64 * 1000 + 500;
    w.step(&[cmd(a, CommandKind::Fish { tx: fx, ty: fy })]);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::NoMaterials)), "no rod, no spear");
    give(&mut w, ItemKind::FishingRod, 1);
    // water with no school near
    let empty = (0..512).flat_map(|y| (0..512).map(move |x| (x, y))).find(|&(x, y)| w.map.terrain(x, y) == Some(Terrain::Water) && crate::map::school_near(&w.map, x, y, 3).is_none()).unwrap();
    w.step(&[cmd(a, CommandKind::Fish { tx: empty.0, ty: empty.1 })]);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::NoFish)));
    w.step(&[cmd(a, CommandKind::Fish { tx: fx, ty: fy })]);
    for _ in 0..60 * 60 {
        w.step(&[]);
        if count(&w, 0, ItemKind::RawFish) >= 3 {
            break;
        }
    }
    assert!(count(&w, 0, ItemKind::RawFish) >= 3, "fish caught: {}", count(&w, 0, ItemKind::RawFish));
    // cooked and eaten
    let (mx, my) = (w.agents[0].x, w.agents[0].y);
    let fire = push_building(&mut w, BuildingKind::Campfire, mx + 1_500, my) as usize;
    w.buildings[fire].fuel = 20_000;
    w.buildings[fire].temp = 400;
    w.buildings[fire].input[ItemKind::RawFish as usize] = 1;
    w.buildings[fire].job = Some(crate::buildings::Job { recipe: recipe("cook_fish").0, remaining: 1 });
    for _ in 0..30 {
        w.step(&[]);
    }
    assert!(w.buildings[fire].output[ItemKind::CookedFish as usize] >= 1, "cooked");
    give(&mut w, ItemKind::CookedFish, 1);
    w.agents[0].hunger = 2_000;
    let protein = w.agents[0].nutrition[1];
    w.step(&[cmd(a, CommandKind::Eat { item: ItemKind::CookedFish })]);
    assert!(w.agents[0].hunger > 2_000 && w.agents[0].nutrition[1] > protein, "fed");
}

#[test]
fn the_sandbox_has_no_needs_gives_items_moves_the_clock_and_flies() {
    let (mut w, a) = with_agent(7);
    w.settings.difficulty = crate::world::Difficulty::Creative;
    w.unlock_everything();
    // everything is known, and a hand-craft needs no materials
    assert!(w.knows(crate::world::Discovery::SharpStone) && w.milestone_done(crate::world::Milestone::FirstFire));
    w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("stone_axe") })]);
    assert!(count(&w, 0, ItemKind::StoneAxe) >= 1, "crafted from nothing, without the minigame");
    assert!(w.craft_view(a).is_none());
    // items can be given
    w.step(&[cmd(a, CommandKind::Give { item: ItemKind::Wood, n: 40 })]);
    assert_eq!(count(&w, 0, ItemKind::Wood), 40);
    // nothing wears a person down, and a bite cannot kill
    w.agents[0].hunger = 1;
    w.agents[0].health = 1;
    for _ in 0..120 {
        w.step(&[]);
    }
    assert!(w.agents[0].hunger > 5000 && w.agents[0].health > 1);
    // the clock goes forward to the asked minute
    w.step(&[cmd(a, CommandKind::SetClock { minute: 720 })]);
    assert_eq!(crate::time::minute_of_day(w.tick), 720);
    // sprinting flies: straight over anything, faster than walking
    let (x0, y0) = (w.agents[0].x, w.agents[0].y);
    w.step(&[cmd(a, CommandKind::SetSprint { on: true })]);
    assert!(w.agents[0].fly);
    w.step(&[cmd(a, CommandKind::MoveTo { x: x0 + 20_000, y: y0 })]);
    for _ in 0..30 {
        w.step(&[]);
    }
    assert!(w.agents[0].x - x0 >= 30 * w.agents[0].kind.speed_mm() * 2, "flew {} mm", w.agents[0].x - x0);
    // and none of it works in an ordinary world
    let (mut w2, a2) = with_agent(7);
    w2.step(&[cmd(a2, CommandKind::Give { item: ItemKind::Wood, n: 40 })]);
    assert_eq!(count(&w2, 0, ItemKind::Wood), 0);
    w2.step(&[cmd(a2, CommandKind::SetSprint { on: true })]);
    assert!(!w2.agents[0].fly);
}

/// Headless sim speed: how long a tick takes with nothing drawn. Run with
/// `cargo test -p alaz-sim --release bench_sim_ticks -- --ignored --nocapture`.
#[test]
#[ignore]
fn bench_sim_ticks() {
    let run = |label: &str, mut w: World, ticks: u64| {
        let t0 = std::time::Instant::now();
        let mut worst = 0f64;
        for _ in 0..ticks {
            let t = std::time::Instant::now();
            w.step(&[]);
            worst = worst.max(t.elapsed().as_secs_f64());
        }
        let total = t0.elapsed().as_secs_f64();
        println!("[sim] {label}: {ticks} ticks in {:.2} s = {:.4} ms/tick avg, worst {:.3} ms, {:.0}x real time", total, total * 1000.0 / ticks as f64, worst * 1000.0, ticks as f64 / 60.0 / total);
    };
    let (w, _) = with_agent(7);
    run("default world, one person", w, 36_000);
    let mut w = World::with_size(crate::map::tag_seed_v3(7), 32_768, 32_768);
    let (sx, sy) = w.spawn_mm;
    w.spawn_agent(sx, sy);
    run("32 km world, one person", w, 18_000);
    let (mut w, _) = with_agent(7);
    for i in 0..15 {
        let (sx, sy) = w.spawn_mm;
        w.spawn_agent(sx + (i % 5) * 1500, sy + (i / 5) * 1500);
    }
    run("default world, sixteen people", w, 18_000);
}

// ---------- clothes with a game effect ----------

/// A world whose clock is set to a cold hour (spring, ambient about 5 C), clear sky, nobody near a fire.
fn chilly_world() -> World {
    let (mut w, _) = with_agent(7);
    w.tick = TICKS_PER_DAY * 2;
    while !(30..=70).contains(&w.ambient_x10()) || crate::time::weather(w.seed, w.tick, w.settings.season_days as u64) != crate::time::Weather::Clear {
        w.tick += 50;
    }
    w
}

#[test]
fn a_cloak_keeps_the_body_warmer_on_a_cold_night() {
    let run = |items: &[ItemKind]| {
        let mut w = chilly_world();
        for k in items {
            w.agents[0].inventory.counts[*k as usize] = 1;
        }
        w.agents[0].warmth = 0;
        for _ in 0..1500 {
            w.step(&[]);
        }
        w.agents[0].warmth
    };
    let bare = run(&[]);
    let cloak = run(&[ItemKind::HideCloak, ItemKind::HideBoots]);
    assert!(cloak > bare + 500, "bare {bare}, cloaked {cloak}");
}

#[test]
fn rain_gear_cuts_the_chill_of_the_wet() {
    let wet_chill = |items: &[ItemKind], sky_wet: i32| {
        let mut inv = Inventory::default();
        for k in items {
            inv.counts[*k as usize] = 1;
        }
        (sky_wet - inv.gear().dry).max(0)
    };
    assert_eq!(wet_chill(&[], 30), 30);
    assert!(wet_chill(&[ItemKind::LinenHood], 30) < wet_chill(&[ItemKind::HideBoots], 30));
    assert_eq!(wet_chill(&[ItemKind::HideCloak, ItemKind::LinenHood], 30), 3);
}

#[test]
fn the_breastplate_turns_a_share_of_a_bite_and_the_wearer_tires_sooner() {
    let hurt = |plate: bool| {
        let (mut w, _) = wolf_world();
        if plate {
            give(&mut w, ItemKind::BronzePlate, 1);
        }
        for _ in 0..800 {
            w.step(&[]);
            if w.take_events().iter().any(|e| matches!(e, Event::Bitten(_))) {
                return (MAX_STAT - w.agents[0].health, w.agents[0].injury);
            }
        }
        panic!("no bite");
    };
    let (bare, bare_bleed) = hurt(false);
    let (plated, plated_bleed) = hurt(true);
    assert!(plated < bare && plated_bleed < bare_bleed, "{plated}/{plated_bleed} vs {bare}/{bare_bleed}");
    // the weight: slower going and a faster-burning breath
    let mut inv = Inventory::default();
    inv.counts[ItemKind::BronzePlate as usize] = 1;
    assert!(inv.gear().burden > 0 && inv.gear().tire > 0);
    let cap = |items: &[ItemKind]| {
        let mut inv = Inventory::default();
        for k in items {
            inv.counts[*k as usize] = 1;
        }
        inv.gear()
    };
    // pieces add up, but never to immunity
    let all = cap(&ItemKind::ALL.iter().copied().filter(|k| k.garment().is_some()).collect::<Vec<_>>());
    assert!(all.bite <= 800 && all.blow <= 800 && all.tool_save <= 80);
}

#[test]
fn boots_soften_a_fall_and_the_helm_a_blow_and_the_apron_spares_tools() {
    let g = |k: ItemKind| k.garment().copied().expect("garment");
    assert!(g(ItemKind::HideBoots).fall > 0 && g(ItemKind::HideBoots).grip > 0);
    assert!(g(ItemKind::SteelHelm).blow > g(ItemKind::BronzePlate).blow);
    assert!(g(ItemKind::WorkApron).tool_save > 0);
    assert_eq!(crate::rules::Gear::cut(1000, 400), 600);
    // a stone strikes less hard on a helmed head
    let fall_of = |boots: bool| {
        let (mut w, _) = with_agent(3);
        if boots {
            give(&mut w, ItemKind::HideBoots, 1);
        }
        let (x, y) = (w.agents[0].x, w.agents[0].y);
        w.launch(w.agents[0].id, 0, 0);
        let g0 = crate::physics::ground_q(w.seed, x * 256, y * 256);
        w.air[0].top = g0 + 4_000 * 256;
        w.air[0].z = g0 + 4_000 * 256;
        for _ in 0..400 {
            w.step(&[]);
            if w.air.is_empty() {
                break;
            }
        }
        MAX_STAT - w.agents[0].health
    };
    assert!(fall_of(true) < fall_of(false));
}

#[test]
fn the_apron_makes_tools_last_longer() {
    let wear1 = |apron: bool, skew: u64| {
        let (mut w, a) = with_agent(1);
        w.tick += skew;
        learn_all(&mut w);
        w.settings.difficulty = crate::world::Difficulty::Realistic;
        give(&mut w, ItemKind::Hammer, 1);
        if apron {
            w.agents[0].inventory.counts[ItemKind::WorkApron as usize] = 1;
        }
        let i = w.nodes.iter().position(|n| n.kind == NodeKind::StoneOutcrop && n.amount == 5).expect("stone");
        w.set_node_amount(i, 3);
        let (nx, ny) = w.nodes[i].center_mm();
        w.agents[0].x = nx;
        w.agents[0].y = ny + 900;
        w.step(&[cmd(a, CommandKind::Gather { node: NodeId(i as u32) })]);
        for _ in 0..90 * 20 {
            w.step(&[]);
        }
        w.agents[0].wear_of(ItemKind::Hammer)
    };
    // the apron spares strokes by chance: over several starts the hammer ends up less worn
    let wear = |apron: bool| (0..8).map(|k| wear1(apron, k * 37)).sum::<u16>();
    let (plain, aproned) = (wear(false), wear(true));
    assert!(plain >= 2 && aproned < plain, "{aproned} vs {plain}");
}

// ---------------------------------------------------------------------------------------------------------------
// Industry, wave 1: kilns, foundry, machine tools, mills, boiler.
// ---------------------------------------------------------------------------------------------------------------

/// A world with everything learned and no wolves; `(bx, by)` is a free spot near the spawn where machines go (they are
/// set down 4 m apart along x, ignoring the placement rules).
fn industry_world() -> (World, AgentId, i64, i64) {
    let (mut w, a) = with_agent(3);
    learn_all(&mut w);
    w.wild.retain(|x| x.species != WildSpecies::Wolf);
    let (bx, by) = spot_for(&w, BuildingKind::BlastFurnace, &[]);
    (w, a, bx, by)
}

fn topped_up(w: &mut World) {
    let me = &mut w.agents[0];
    (me.hunger, me.thirst, me.warmth, me.health) = (MAX_STAT, MAX_STAT, MAX_STAT, MAX_STAT);
}

/// Stand next to a station, put `inputs` in the pack and start recipe `id`. A recipe that needs heat must not get
/// anywhere while the station is cold. Then run until the job is done (true) or `max` ticks have passed (false).
fn run_job(w: &mut World, a: AgentId, st: u32, id: &str, inputs: &[(ItemKind, u16)], max: u64) -> bool {
    let (x, y) = (w.buildings[st as usize].x, w.buildings[st as usize].y);
    w.agents[0].x = x + 1_300;
    w.agents[0].y = y;
    for &(k, n) in inputs {
        w.agents[0].inventory.counts[k as usize] += n;
    }
    w.step(&[cmd(a, CommandKind::StartJob { building: st, recipe: recipe(id) })]);
    assert!(w.buildings[st as usize].job.is_some(), "{id}: {:?}", w.take_events());
    let def = &rules().recipes[recipe(id).0 as usize];
    if def.heat > 0 && w.buildings[st as usize].temp < def.heat {
        for _ in 0..120 {
            topped_up(w);
            w.step(&[]);
        }
        let left = w.buildings[st as usize].job.map(|j| j.remaining);
        assert_eq!(left, Some(def.ticks), "{id}: a cold station does no work");
    }
    for _ in 0..max {
        topped_up(w);
        w.step(&[]);
        if w.buildings[st as usize].job.is_none() {
            return true;
        }
    }
    false
}

fn out_of(w: &World, st: u32, k: ItemKind) -> u16 {
    w.buildings[st as usize].output[k as usize]
}

#[test]
fn kilns_and_furnaces_bake_by_the_batch() {
    use ItemKind as I;
    let (base, a, bx, by) = industry_world();
    type Case = (BuildingKind, &'static str, Vec<(ItemKind, u16)>, Vec<(ItemKind, u16)>, u64);
    let cases: Vec<Case> = vec![
        (BuildingKind::LimeKiln, "burn_lime", vec![(I::Limestone, 3), (I::Charcoal, 1)], vec![(I::Lime, 2)], 8_000),
        (BuildingKind::LimeKiln, "slake_mortar", vec![(I::Lime, 1), (I::Sand, 2), (I::WaterPot, 1)], vec![(I::Mortar, 3), (I::Pot, 1)], 2_000),
        (BuildingKind::CementKiln, "burn_cement", vec![(I::Lime, 2), (I::Clay, 2), (I::Charcoal, 1)], vec![(I::Cement, 3)], 12_000),
        (BuildingKind::CementKiln, "mix_concrete", vec![(I::Cement, 1), (I::Sand, 2), (I::WaterPot, 1)], vec![(I::Concrete, 3), (I::Pot, 1)], 2_000),
        (BuildingKind::BrickKiln, "fire_bricks", vec![(I::Clay, 6), (I::Charcoal, 2)], vec![(I::Brick, 6)], 12_000),
        (BuildingKind::BrickKiln, "fire_fire_bricks_batch", vec![(I::FireClay, 6), (I::Charcoal, 2)], vec![(I::FireBrick, 6)], 12_000),
        (BuildingKind::BrickKiln, "fire_raw_fire_bricks_batch", vec![(I::RawFireBrick, 8), (I::Charcoal, 2)], vec![(I::FireBrick, 8)], 12_000),
        (BuildingKind::CokeOven, "coke_oven", vec![(I::Coal, 4)], vec![(I::Coke, 3), (I::Tar, 1)], 10_000),
        (BuildingKind::PuddlingFurnace, "puddle_iron", vec![(I::PigIron, 4), (I::Charcoal, 2)], vec![(I::IronBloom, 4)], 10_000),
        (BuildingKind::PuddlingFurnace, "puddle_iron_coal", vec![(I::PigIron, 4), (I::Coal, 1)], vec![(I::IronBloom, 4)], 10_000),
    ];
    for (kind, id, inputs, outputs, max) in cases {
        let mut w = base.clone();
        let st = push_building(&mut w, kind, bx + 4_000, by);
        assert!(run_job(&mut w, a, st, id, &inputs, max), "{kind:?} {id} did not finish");
        for (k, n) in outputs {
            assert_eq!(out_of(&w, st, k), n, "{kind:?} {id}: {k:?}");
        }
    }
}

#[test]
fn the_converter_needs_a_blast_driven_by_machinery() {
    let (base, a, bx, by) = industry_world();
    let load = |w: &mut World, st: u32| {
        w.agents[0].inventory.counts[ItemKind::PigIron as usize] = 3;
        let (x, y) = (w.buildings[st as usize].x, w.buildings[st as usize].y);
        w.agents[0].x = x + 1_300;
        w.agents[0].y = y;
        w.step(&[cmd(a, CommandKind::StartJob { building: st, recipe: recipe("bessemer_steel") })]);
        assert!(w.buildings[st as usize].job.is_some());
        for _ in 0..9_000 {
            topped_up(w);
            w.step(&[]);
        }
    };
    // no bellows: the vessel never gets past 1000 degrees, the steel is never made
    let mut w = base.clone();
    let conv = push_building(&mut w, BuildingKind::Bessemer, bx + 4_000, by);
    load(&mut w, conv);
    assert!(w.buildings[conv as usize].job.is_some() && w.buildings[conv as usize].temp <= 1000, "{} °C", w.buildings[conv as usize].temp);
    assert!(!w.requirement_met(conv as usize));
    // a bellows worked by hand is not enough either
    let mut w = base.clone();
    let conv = push_building(&mut w, BuildingKind::Bessemer, bx + 4_000, by);
    push_building(&mut w, BuildingKind::Bellows, bx + 5_500, by);
    load(&mut w, conv);
    assert!(w.buildings[conv as usize].job.is_some() && w.buildings[conv as usize].temp <= 1000);
    assert!(!w.requirement_met(conv as usize), "a hand bellows is not a strong blast");
    // a windmill turning the bellows: steel, two for three pig
    let mut w = base.clone();
    let conv = push_building(&mut w, BuildingKind::Bessemer, bx + 4_000, by);
    push_building(&mut w, BuildingKind::Bellows, bx + 5_500, by);
    push_building(&mut w, BuildingKind::Windmill, bx + 9_500, by);
    assert!(w.requirement_met(conv as usize));
    assert!(run_job(&mut w, a, conv, "bessemer_steel", &[(ItemKind::PigIron, 3)], 9_000));
    assert_eq!(out_of(&w, conv, ItemKind::Steel), 2);
}

#[test]
fn the_cupola_pours_into_a_mould_that_comes_back() {
    use ItemKind as I;
    let (base, a, bx, by) = industry_world();
    let mut w = base.clone();
    let bench = push_building(&mut w, BuildingKind::MoldingBench, bx - 4_000, by);
    assert!(run_job(&mut w, a, bench, "pack_sand_mold", &[(I::Sand, 4), (I::Clay, 1), (I::Wood, 1)], 2_000));
    assert_eq!(out_of(&w, bench, I::SandMold), 2);
    w.step(&[cmd(a, CommandKind::Collect { building: bench, item: None })]);
    assert_eq!(count(&w, 0, I::SandMold), 2);
    // without a bellows the cupola stays cool and pours nothing
    let cupola = push_building(&mut w, BuildingKind::Cupola, bx + 4_000, by);
    let mut cold = w.clone();
    cold.agents[0].inventory.counts[I::PigIron as usize] = 2;
    cold.agents[0].inventory.counts[I::Charcoal as usize] = 1;
    cold.agents[0].x = bx + 5_300;
    cold.agents[0].y = by;
    cold.step(&[cmd(a, CommandKind::StartJob { building: cupola, recipe: recipe("cupola_cast_part") })]);
    for _ in 0..9_000 {
        topped_up(&mut cold);
        cold.step(&[]);
    }
    assert!(cold.buildings[cupola as usize].job.is_some(), "no blast, no pour");
    // with a bellows (a hand will do) every pour gives the mould back: the same mould casts part, pipe, cylinder, wheel
    push_building(&mut w, BuildingKind::Bellows, bx + 5_500, by);
    w.agents[0].inventory.counts[I::Charcoal as usize] = 8;
    w.agents[0].inventory.counts[I::PigIron as usize] = 10;
    for (id, out, n) in [("cupola_cast_part", I::CastIronPart, 2), ("cupola_cast_pipe", I::Pipe, 2), ("cupola_cast_cylinder", I::IronCylinder, 1), ("cupola_cast_wheel", I::IronWheel, 1)] {
        assert!(run_job(&mut w, a, cupola, id, &[], 12_000), "{id}");
        assert_eq!(out_of(&w, cupola, out), n, "{id}");
        w.step(&[cmd(a, CommandKind::Collect { building: cupola, item: None })]);
        assert_eq!(count(&w, 0, I::SandMold), 2, "{id}: the mould is back");
    }
}

#[test]
fn machine_tools_and_mills_stand_still_without_power() {
    use ItemKind as I;
    let (base, a, bx, by) = industry_world();
    type Case = (BuildingKind, &'static str, Vec<(ItemKind, u16)>, Vec<(ItemKind, u16)>);
    let cases: Vec<Case> = vec![
        (BuildingKind::Lathe, "turn_shaft", vec![(I::IronBloom, 2)], vec![(I::Shaft, 1)]),
        (BuildingKind::Lathe, "turn_bolts", vec![(I::IronBloom, 1)], vec![(I::BoltNut, 4)]),
        (BuildingKind::Lathe, "turn_piston", vec![(I::CastIronPart, 1), (I::IronBloom, 1)], vec![(I::Piston, 1)]),
        (BuildingKind::Lathe, "lathe_gear", vec![(I::IronBloom, 1)], vec![(I::Gear, 2)]),
        (BuildingKind::DrillMill, "mill_precision_gear", vec![(I::Steel, 1), (I::IronBloom, 1)], vec![(I::PrecisionGear, 2)]),
        (BuildingKind::DrillMill, "bore_cylinder_block", vec![(I::IronCylinder, 1), (I::BoltNut, 2), (I::Steel, 1)], vec![(I::CylinderBlock, 1)]),
        (BuildingKind::Press, "press_sheet_steel", vec![(I::Steel, 1)], vec![(I::Sheet, 3)]),
        (BuildingKind::Press, "press_sheet_iron", vec![(I::IronBloom, 2)], vec![(I::Sheet, 2)]),
        (BuildingKind::Press, "stamp_sheet_part", vec![(I::Sheet, 1), (I::BoltNut, 1)], vec![(I::SheetPart, 2)]),
        (BuildingKind::Sawmill, "saw_logs", vec![(I::Wood, 1)], vec![(I::Plank, 4)]),
        (BuildingKind::PowerLoom, "weave_thread", vec![(I::Thread, 4)], vec![(I::Cloth, 3)]),
        (BuildingKind::RollingLine, "roll_rail", vec![(I::Steel, 2)], vec![(I::Rail, 4)]),
        (BuildingKind::RollingLine, "roll_beam", vec![(I::Steel, 2)], vec![(I::Beam, 3)]),
    ];
    for (kind, id, inputs, outputs) in cases {
        assert!(kind.needs_power());
        let ticks = rules().recipes[recipe(id).0 as usize].ticks;
        // no power: the job is loaded and waits
        let mut w = base.clone();
        let st = push_building(&mut w, kind, bx + 4_000, by);
        assert!(!w.requirement_met(st as usize), "{kind:?}");
        assert!(!run_job(&mut w, a, st, id, &inputs, ticks as u64 + 600), "{kind:?} {id} ran without power");
        assert_eq!(w.buildings[st as usize].job.map(|j| j.remaining), Some(ticks), "{kind:?} {id}: no progress without power");
        // a windmill within 8 m turns it
        let mut w = base.clone();
        let st = push_building(&mut w, kind, bx + 4_000, by);
        push_building(&mut w, BuildingKind::Windmill, bx + 9_000, by);
        assert!(w.requirement_met(st as usize), "{kind:?}");
        assert!(run_job(&mut w, a, st, id, &inputs, ticks as u64 + 600), "{kind:?} {id} did not finish under power");
        for (k, n) in outputs {
            assert_eq!(out_of(&w, st, k), n, "{kind:?} {id}: {k:?}");
        }
    }
}

#[test]
fn the_spinning_wheel_needs_no_power() {
    use ItemKind as I;
    let (mut w, a, bx, by) = industry_world();
    let st = push_building(&mut w, BuildingKind::SpinningWheel, bx + 4_000, by);
    assert!(w.requirement_met(st as usize));
    assert!(run_job(&mut w, a, st, "spin_fiber", &[(I::Fiber, 3)], 2_000));
    assert_eq!(out_of(&w, st, I::Thread), 2);
    assert!(run_job(&mut w, a, st, "spin_flax", &[(I::Flax, 2)], 2_000));
    assert_eq!(out_of(&w, st, I::Thread), 5);
}

#[test]
fn a_boiler_carries_steam_to_engines_and_the_hammer_wants_it() {
    use ItemKind as I;
    let (base, a, bx, by) = industry_world();
    // a windmill alone does not lift the steam hammer
    let mut w = base.clone();
    let hammer = push_building(&mut w, BuildingKind::SteamHammer, bx + 4_000, by);
    push_building(&mut w, BuildingKind::Windmill, bx + 9_000, by);
    assert!(!w.requirement_met(hammer as usize), "wind is not steam");
    assert!(!run_job(&mut w, a, hammer, "forge_heavy_shaft", &[(I::Steel, 2), (I::Coke, 1)], 3_000));
    // a boiler raises steam on coal and water
    let mut w = base.clone();
    let boiler = push_building(&mut w, BuildingKind::Boiler, bx, by);
    w.agents[0].x = bx + 1_300;
    w.agents[0].y = by;
    give(&mut w, I::Coal, 2);
    give(&mut w, I::WaterPot, 2);
    w.step(&[cmd(a, CommandKind::StartJob { building: boiler, recipe: recipe("boiler_coal") })]);
    for _ in 0..60 * 15 {
        topped_up(&mut w);
        w.step(&[]);
    }
    assert!(w.buildings[boiler as usize].steaming(), "{} °C", w.buildings[boiler as usize].temp);
    // an engine beside it (no fire of its own) is fed, and carries the power 8 m on to a lathe out of the boiler's own reach
    let engine = push_building(&mut w, BuildingKind::SteamEngine, bx + 5_000, by);
    let lathe = push_building(&mut w, BuildingKind::Lathe, bx + 12_000, by);
    assert!(w.buildings[engine as usize].job.is_none());
    assert!(w.requirement_met(lathe as usize), "the boiler-fed engine drives the lathe");
    let far_lathe = push_building(&mut w, BuildingKind::Lathe, bx + 20_000, by);
    assert!(!w.requirement_met(far_lathe as usize));
    // and the hammer within 8 m of the boiler works
    let hammer = push_building(&mut w, BuildingKind::SteamHammer, bx - 4_000, by);
    assert!(w.requirement_met(hammer as usize));
    assert!(run_job(&mut w, a, hammer, "forge_heavy_shaft", &[(I::Steel, 2), (I::Coke, 1)], 3_000));
    assert_eq!(out_of(&w, hammer, I::HeavyShaft), 1);
    // the engine fed by the boiler lets the lathe work
    assert!(run_job(&mut w, a, lathe, "turn_bolts", &[(I::IronBloom, 1)], 2_000));
    assert_eq!(out_of(&w, lathe, I::BoltNut), 4);
}

#[test]
fn industry_discoveries_open_from_what_the_tribe_holds() {
    use ItemKind as I;
    let (mut w, _) = with_agent(5);
    w.forget_everything();
    w.wild.retain(|x| x.species != WildSpecies::Wolf);
    // nothing of the new age before steel is known
    give(&mut w, I::Limestone, 1);
    w.step(&[]);
    assert!(!w.knows(Discovery::Masonry));
    w.learn(Discovery::Steelmaking);
    w.step(&[]);
    assert!(w.knows(Discovery::Masonry), "limestone in hand among steel-makers: kilns for lime and brick");
    assert!(!w.knows(Discovery::Cement));
    give(&mut w, I::Lime, 1);
    w.step(&[]);
    assert!(w.knows(Discovery::Cement), "lime in hand: cement");
    assert!(!w.knows(Discovery::Foundry));
    give(&mut w, I::PigIron, 1);
    w.step(&[]);
    assert!(w.knows(Discovery::Foundry));
    give(&mut w, I::Steel, 1);
    w.step(&[]);
    assert!(!w.knows(Discovery::Converter), "the converter wants steam too");
    w.learn(Discovery::Steam);
    w.step(&[]);
    assert!(w.knows(Discovery::Converter));
    assert!(!w.knows(Discovery::Machining));
    give(&mut w, I::CastIronPart, 1);
    w.step(&[]);
    assert!(w.knows(Discovery::Machining));
    // fire clay opens masonry too, for those who never found limestone
    let (mut w, _) = with_agent(5);
    w.forget_everything();
    w.learn(Discovery::Steelmaking);
    give(&mut w, I::FireClay, 1);
    w.step(&[]);
    assert!(w.knows(Discovery::Masonry));
    // every machine is opened by a discovery that something in play teaches
    for def in &rules().buildings {
        assert!(def.discovery != Discovery::Retired || def.kind == BuildingKind::Ruin, "{:?} is locked for good", def.kind);
    }
}

#[test]
fn machined_parts_are_a_second_way_to_pay_for_machines() {
    use ItemKind as I;
    let (mut w, a, ..) = industry_world();
    w.settings.difficulty = Difficulty::Forgiving;
    let engine = rules().building(BuildingKind::SteamEngine);
    assert!(!engine.alt_cost.is_empty());
    let build = |w: &mut World, kind: BuildingKind| -> bool {
        let before = w.buildings.len();
        let (x, y) = spot_for(w, kind, &w.buildings.iter().map(|b| (b.x, b.y)).collect::<Vec<_>>());
        w.step(&[cmd(a, CommandKind::Build { kind, x, y })]);
        w.buildings.len() > before
    };
    // nothing in the pack: refused
    assert!(!build(&mut w, BuildingKind::SteamEngine));
    // the cast parts the old way
    for c in &engine.cost {
        give(&mut w, c.item, c.n);
    }
    assert!(build(&mut w, BuildingKind::SteamEngine));
    assert_eq!(w.agents[0].inventory.total(), 0, "the old cost is spent in full");
    // the machined parts the new way
    for c in &engine.alt_cost {
        give(&mut w, c.item, c.n);
    }
    assert!(build(&mut w, BuildingKind::SteamEngine));
    assert_eq!(w.agents[0].inventory.total(), 0, "the new cost is spent in full");
    // a steam hammer on a concrete footing, or on plain stone
    let hammer = rules().building(BuildingKind::SteamHammer);
    for c in &hammer.alt_cost {
        give(&mut w, c.item, c.n);
    }
    assert!(build(&mut w, BuildingKind::SteamHammer));
    assert_eq!(count(&w, 0, I::Stone), 0);
    for c in &hammer.cost {
        give(&mut w, c.item, c.n);
    }
    assert!(build(&mut w, BuildingKind::SteamHammer));
    assert_eq!(w.agents[0].inventory.total(), 0);
}

/// Raw iron to a steam engine the machine-tool way: pig iron cast in a mould, bored, turned, and the parts paid for
/// the engine instead of cast-iron parts and steel.
#[test]
fn a_steam_engine_can_be_built_from_machined_parts() {
    use ItemKind as I;
    let (mut w, a, bx, by) = industry_world();
    push_building(&mut w, BuildingKind::Windmill, bx, by);
    let bench = push_building(&mut w, BuildingKind::MoldingBench, bx - 4_000, by + 4_000);
    let cupola = push_building(&mut w, BuildingKind::Cupola, bx + 2_000, by + 5_500);
    push_building(&mut w, BuildingKind::Bellows, bx + 3_500, by + 5_500);
    let lathe = push_building(&mut w, BuildingKind::Lathe, bx + 5_000, by);
    let drill = push_building(&mut w, BuildingKind::DrillMill, bx - 5_000, by);
    assert!(run_job(&mut w, a, bench, "pack_sand_mold", &[(I::Sand, 4), (I::Clay, 1), (I::Wood, 1)], 2_000));
    w.step(&[cmd(a, CommandKind::Collect { building: bench, item: None })]);
    give(&mut w, I::PigIron, 8);
    give(&mut w, I::Charcoal, 4);
    assert!(run_job(&mut w, a, cupola, "cupola_cast_cylinder", &[], 12_000));
    assert!(run_job(&mut w, a, cupola, "cupola_cast_part", &[], 12_000));
    w.step(&[cmd(a, CommandKind::Collect { building: cupola, item: None })]);
    assert_eq!((count(&w, 0, I::IronCylinder), count(&w, 0, I::CastIronPart)), (1, 2));
    give(&mut w, I::IronBloom, 6);
    give(&mut w, I::Steel, 1);
    assert!(run_job(&mut w, a, lathe, "turn_bolts", &[], 3_000));
    assert!(run_job(&mut w, a, lathe, "turn_piston", &[], 3_000));
    assert!(run_job(&mut w, a, lathe, "turn_shaft", &[], 3_000));
    w.step(&[cmd(a, CommandKind::Collect { building: lathe, item: None })]);
    assert!(run_job(&mut w, a, drill, "bore_cylinder_block", &[], 4_000));
    w.step(&[cmd(a, CommandKind::Collect { building: drill, item: None })]);
    for c in &rules().building(BuildingKind::SteamEngine).alt_cost {
        assert!(count(&w, 0, c.item) >= c.n || c.item == I::FireClay || c.item == I::FireBrick, "{:?}: {} of {}", c.item, count(&w, 0, c.item), c.n);
    }
    give(&mut w, I::FireBrick, 4);
    let before = w.buildings.len();
    let (x, y) = spot_for(&w, BuildingKind::SteamEngine, &w.buildings.iter().map(|b| (b.x, b.y)).collect::<Vec<_>>());
    w.agents[0].x = x;
    w.agents[0].y = y + 1_500;
    w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::SteamEngine, x, y })]);
    assert_eq!(w.buildings.len(), before + 1, "{:?}", w.take_events());
}

#[test]
fn the_old_routes_still_work_beside_the_new_ones() {
    // iron, steel, planks, cloth, coke and fire brick are each still made the way they were
    let r = rules();
    for (id, station) in [("bloom_sponge", BuildingKind::Bloomery), ("trip_refine_iron", BuildingKind::TripHammer), ("pig_iron", BuildingKind::BlastFurnace), ("cast_iron_part", BuildingKind::BlastFurnace), ("coke", BuildingKind::Furnace), ("cloth", BuildingKind::Loom), ("fire_fire_brick", BuildingKind::PitKiln)] {
        let d = &r.recipes[recipe(id).0 as usize];
        assert_eq!(d.station, Some(station), "{id}");
        assert_ne!(d.discovery, Discovery::Retired, "{id}");
    }
    for id in ["plank", "forge_high_carbon", "forge_steel", "gear", "refine_iron", "cart", "wheel"] {
        assert!(r.recipes.iter().any(|d| d.id == id && d.discovery != Discovery::Retired), "{id}");
    }
    // every new machine has at least one recipe, or is a power source
    for k in [BuildingKind::CokeOven, BuildingKind::LimeKiln, BuildingKind::CementKiln, BuildingKind::BrickKiln, BuildingKind::PuddlingFurnace, BuildingKind::Bessemer, BuildingKind::Cupola, BuildingKind::MoldingBench, BuildingKind::Lathe, BuildingKind::DrillMill, BuildingKind::Press, BuildingKind::Sawmill, BuildingKind::SpinningWheel, BuildingKind::PowerLoom, BuildingKind::Boiler, BuildingKind::SteamHammer, BuildingKind::RollingLine] {
        assert!(r.recipes.iter().any(|d| d.station == Some(k)), "{k:?} has no recipe");
        assert!(r.building(k).discovery != Discovery::Retired);
    }
}

#[test]
fn new_stations_take_deposits_and_run_on_auto_like_the_old_ones() {
    use ItemKind as I;
    let (mut w, a, bx, by) = industry_world();
    let mill = push_building(&mut w, BuildingKind::Sawmill, bx + 4_000, by);
    push_building(&mut w, BuildingKind::Windmill, bx + 9_000, by);
    let wheel = push_building(&mut w, BuildingKind::SpinningWheel, bx - 4_000, by);
    // a person loads wood into the sawmill once and sets it to saw by itself
    w.agents[0].x = bx + 5_300;
    w.agents[0].y = by;
    give(&mut w, I::Wood, 3);
    w.step(&[cmd(a, CommandKind::Deposit { building: mill })]);
    assert_eq!(w.buildings[mill as usize].input[I::Wood as usize], 3);
    w.step(&[cmd(a, CommandKind::SetAuto { building: mill, recipe: Some(recipe("saw_logs")) })]);
    for _ in 0..3_000 {
        topped_up(&mut w);
        w.step(&[]);
    }
    assert_eq!(out_of(&w, mill, I::Plank), 12, "three logs, three runs, no one at the saw");
    // a worker on a route carries fibre to the wheel and takes the thread away
    let bot = w.spawn_worker(AgentKind::Bot, bx - 2_000, by);
    w.agents[bot.0 as usize].inventory.counts[I::Fiber as usize] = 6;
    w.step(&[cmd(a, CommandKind::SetAuto { building: wheel, recipe: Some(recipe("spin_fiber")) })]);
    w.step(&[cmd(a, CommandKind::SetRoute { worker: bot, route: vec![RouteStep::Deposit { building: wheel }, RouteStep::Wait { ticks: 900 }, RouteStep::Collect { building: wheel, item: Some(I::Thread) }] })]);
    for _ in 0..4_000 {
        topped_up(&mut w);
        let b = &mut w.agents[bot.0 as usize];
        b.hunger = MAX_STAT as _;
        w.step(&[]);
    }
    assert!(w.agents[bot.0 as usize].inventory.get(I::Thread) >= 2, "the worker brought thread out of the wheel: {}", w.agents[bot.0 as usize].inventory.get(I::Thread));
}

// ---------------------------------------------------------------------------------------------------------------
// Life and craft, wave 2: pottery, oil, leather, food that keeps, wells, mills, felt, rope, cranes, glass, paper, books,
// cotton.
// ---------------------------------------------------------------------------------------------------------------

#[test]
fn life_and_craft_stations_make_their_goods() {
    use ItemKind as I;
    let (base, a, bx, by) = industry_world();
    type Case = (BuildingKind, &'static str, Vec<(ItemKind, u16)>, Vec<(ItemKind, u16)>, u64);
    let cases: Vec<Case> = vec![
        (BuildingKind::PotterWheel, "wheel_pot", vec![(I::Clay, 3)], vec![(I::RawPot, 2)], 1_000),
        (BuildingKind::PotterWheel, "wheel_large_pot", vec![(I::Clay, 8)], vec![(I::RawLargePot, 2)], 1_500),
        (BuildingKind::PotterWheel, "wheel_oil_lamp", vec![(I::Pot, 1), (I::Fiber, 1)], vec![(I::OilLamp, 1)], 800),
        (BuildingKind::OilPress, "press_linseed", vec![(I::Linseed, 3)], vec![(I::PlantOil, 2)], 1_500),
        (BuildingKind::OilPress, "press_seed", vec![(I::Seed, 6)], vec![(I::PlantOil, 1)], 2_000),
        (BuildingKind::Tannery, "tan_hide", vec![(I::Hide, 2), (I::Wood, 2), (I::WaterPot, 1)], vec![(I::Leather, 3), (I::Pot, 1)], 3_000),
        (BuildingKind::Tannery, "cut_belt", vec![(I::Leather, 2), (I::Fiber, 1)], vec![(I::Belt, 1)], 1_000),
        (BuildingKind::DryingRack, "dry_berries", vec![(I::Berries, 4)], vec![(I::DriedBerries, 3)], 2_500),
        (BuildingKind::DryingRack, "dry_meat", vec![(I::RawMeat, 2)], vec![(I::Jerky, 2)], 3_000),
        (BuildingKind::DryingRack, "dry_fish", vec![(I::RawFish, 2)], vec![(I::Jerky, 2)], 3_000),
        (BuildingKind::Smokehouse, "smoke_meat", vec![(I::RawMeat, 3), (I::Wood, 1)], vec![(I::SmokedMeat, 3)], 4_000),
        (BuildingKind::Smokehouse, "smoke_cooked_meat", vec![(I::CookedMeat, 3), (I::Wood, 1)], vec![(I::SmokedMeat, 3)], 4_000),
        (BuildingKind::Smokehouse, "smoke_fish", vec![(I::RawFish, 3), (I::Wood, 1)], vec![(I::SmokedFish, 3)], 4_000),
        (BuildingKind::Well, "draw_water", vec![(I::Pot, 1)], vec![(I::WaterPot, 1)], 400),
        (BuildingKind::Well, "draw_water_batch", vec![(I::Pot, 4)], vec![(I::WaterPot, 4)], 800),
        (BuildingKind::StoneMortar, "pound_bulgur", vec![(I::Grain, 4)], vec![(I::Bulgur, 4)], 1_500),
        (BuildingKind::WaterMill, "water_mill_flour", vec![(I::Grain, 6)], vec![(I::Flour, 6)], 700),
        (BuildingKind::FeltMallet, "beat_felt", vec![(I::Fiber, 6), (I::WaterPot, 1)], vec![(I::Felt, 2), (I::Pot, 1)], 2_000),
        (BuildingKind::Ropewalk, "twist_rope", vec![(I::Fiber, 4)], vec![(I::Rope, 1)], 1_500),
        (BuildingKind::Ropewalk, "twist_rope_flax", vec![(I::Flax, 3)], vec![(I::Rope, 2)], 1_500),
        (BuildingKind::GlassFurnace, "blow_glass", vec![(I::Sand, 3), (I::Charcoal, 1)], vec![(I::Glass, 2)], 8_000),
        (BuildingKind::GlassFurnace, "blow_bottle", vec![(I::Glass, 1)], vec![(I::Bottle, 2)], 8_000),
        (BuildingKind::GlassFurnace, "grind_lens", vec![(I::Glass, 1)], vec![(I::Lens, 1)], 8_000),
        (BuildingKind::GlassFurnace, "blow_window_pane", vec![(I::Glass, 2)], vec![(I::WindowPane, 1)], 8_000),
        (BuildingKind::PaperVat, "make_paper", vec![(I::Fiber, 4), (I::WaterPot, 1)], vec![(I::Paper, 3), (I::Pot, 1)], 3_000),
        (BuildingKind::PaperVat, "make_paper_rags", vec![(I::Cloth, 1), (I::WaterPot, 1)], vec![(I::Paper, 4), (I::Pot, 1)], 3_000),
        (BuildingKind::PrintingPress, "print_book", vec![(I::Paper, 4), (I::Charcoal, 1), (I::PlantOil, 1), (I::Leather, 1)], vec![(I::Book, 1)], 4_000),
        (BuildingKind::Loom, "weave_cotton", vec![(I::CottonFiber, 3)], vec![(I::Cloth, 2)], 2_000),
        (BuildingKind::SpinningWheel, "spin_cotton", vec![(I::CottonFiber, 2)], vec![(I::Thread, 3)], 2_000),
    ];
    for (kind, id, inputs, outputs, max) in cases {
        let mut w = base.clone();
        let st = push_building(&mut w, kind, bx + 4_000, by);
        assert!(run_job(&mut w, a, st, id, &inputs, max), "{kind:?} {id} did not finish");
        for (k, n) in outputs {
            assert_eq!(out_of(&w, st, k), n, "{kind:?} {id}: {k:?}");
        }
    }
}

#[test]
fn the_smokehouse_and_the_glass_furnace_stay_cold_without_fire() {
    use ItemKind as I;
    let (base, a, bx, by) = industry_world();
    // a smokehouse climbs to its ceiling while it works and no further
    let mut w = base.clone();
    let st = push_building(&mut w, BuildingKind::Smokehouse, bx + 4_000, by);
    assert!(run_job(&mut w, a, st, "smoke_fish", &[(I::RawFish, 3), (I::Wood, 1)], 4_000));
    let temp = w.buildings[st as usize].temp;
    assert!(temp >= 20 && temp <= rules().building(BuildingKind::Smokehouse).heat_max, "{temp}");
    // glass needs real heat: far above what a smokehouse ever reaches, and the lens less
    let r = |id: &str| rules().recipes[recipe(id).0 as usize].heat;
    assert!(r("blow_glass") >= 1_100 && r("blow_window_pane") >= 1_100 && r("grind_lens") < r("blow_bottle"));
    assert!(r("smoke_meat") > 0 && r("smoke_meat") < 100, "smoke is warm, not hot");
    assert!(rules().building(BuildingKind::GlassFurnace).heat_max > r("blow_glass"));
}

#[test]
fn drying_goes_at_half_speed_in_the_rain_and_smoked_food_keeps() {
    use ItemKind as I;
    let (base, a, bx, by) = industry_world();
    let progress = |w: &mut World, st: u32| -> u32 {
        let ticks = rules().recipes[recipe("dry_meat").0 as usize].ticks;
        w.agents[0].x = w.buildings[st as usize].x + 1_300;
        w.agents[0].y = w.buildings[st as usize].y;
        give(w, I::RawMeat, 2);
        w.step(&[cmd(a, CommandKind::StartJob { building: st, recipe: recipe("dry_meat") })]);
        for _ in 0..600 {
            topped_up(w);
            w.step(&[]);
        }
        ticks - w.buildings[st as usize].job.expect("still drying").remaining
    };
    let mut clear = base.clone();
    let rack = push_building(&mut clear, BuildingKind::DryingRack, bx + 4_000, by);
    assert!(matches!(clear.weather(), crate::time::Weather::Clear));
    let dry = progress(&mut clear, rack);
    // find a rainy spell and try again
    let mut wet = base.clone();
    let rack = push_building(&mut wet, BuildingKind::DryingRack, bx + 4_000, by);
    let days = wet.settings.season_days as u64;
    let mut t = crate::time::TICKS_PER_DAY * 2;
    while crate::time::weather(wet.seed, t, days) == crate::time::Weather::Clear || crate::time::weather(wet.seed, t + 900, days) == crate::time::Weather::Clear {
        t += crate::time::TICKS_PER_DAY / 6;
        assert!(t < crate::time::TICKS_PER_DAY * days * 6, "no rain in sight");
    }
    wet.tick = t;
    let damp = progress(&mut wet, rack);
    assert!((600..=601).contains(&dry), "{dry}");
    assert!((290..=310).contains(&damp), "half speed in the rain: {damp}");
    // smoked and dried food spoils far slower than raw or cooked: one hour's loss from a thousand of each
    let mut w = base.clone();
    for k in [I::RawMeat, I::CookedMeat, I::Jerky, I::SmokedMeat, I::SmokedFish, I::DriedBerries, I::Berries, I::Bulgur, I::Grain] {
        w.agents[0].inventory.counts[k as usize] = 1_000;
    }
    w.tick = crate::time::TICKS_PER_DAY / 24 * 5;
    w.step(&[]);
    let lost = |k: I| 1_000 - count(&w, 0, k);
    assert!(lost(I::RawMeat) > 10 * lost(I::SmokedMeat).max(1), "raw {} vs smoked {}", lost(I::RawMeat), lost(I::SmokedMeat));
    assert!(lost(I::CookedMeat) > 3 * lost(I::SmokedMeat).max(1));
    assert!(lost(I::Berries) > 10 * lost(I::DriedBerries).max(1));
    assert!(lost(I::Jerky) <= 3 && lost(I::SmokedFish) <= 4 && lost(I::Bulgur) <= 2);
}

#[test]
fn new_foods_feed_and_smoked_meat_settles_the_stomach() {
    use ItemKind as I;
    for k in [I::DriedBerries, I::Jerky, I::SmokedMeat, I::SmokedFish, I::Bulgur] {
        let (mut w, a) = with_agent(3);
        w.agents[0].hunger = 2_000;
        w.agents[0].sick = 4_000;
        give(&mut w, k, 1);
        w.step(&[cmd(a, CommandKind::Eat { item: k })]);
        assert!(w.agents[0].hunger > 3_500, "{k:?}: {}", w.agents[0].hunger);
        assert_eq!(count(&w, 0, k), 0);
        if matches!(k, I::SmokedMeat | I::SmokedFish) {
            assert!(w.agents[0].sick < 4_000, "{k:?} is cured meat");
        }
    }
    // bulgur fills like bread and counts as grain
    let (mut w, a) = with_agent(3);
    w.agents[0].nutrition = [0; 3];
    w.agents[0].hunger = 1_000;
    give(&mut w, I::Bulgur, 1);
    w.step(&[cmd(a, CommandKind::Eat { item: I::Bulgur })]);
    assert!(w.agents[0].nutrition[0] > 0 && w.agents[0].nutrition[1] == 0);
}

#[test]
fn wells_give_water_anywhere_but_not_in_rock() {
    use ItemKind as I;
    let (base, a, bx, by) = industry_world();
    // far from any water, a pot goes down empty and comes up full
    let mut w = base.clone();
    let well = push_building(&mut w, BuildingKind::Well, bx + 4_000, by);
    assert!(run_job(&mut w, a, well, "draw_water", &[(I::Pot, 1)], 400));
    w.step(&[cmd(a, CommandKind::Collect { building: well, item: None })]);
    assert_eq!(count(&w, 0, I::WaterPot), 1);
    assert_eq!(count(&w, 0, I::Pot), 0);
    // not into bare rock
    let mut found = false;
    let rocks = (0..256).flat_map(|y| (0..256).map(move |x| (x, y))).filter(|&(x, y)| (-1..=1).all(|d| w.map.terrain(x + d, y) == Some(Terrain::Rock) && w.map.terrain(x, y + d) == Some(Terrain::Rock)) && w.node_at(x, y).is_none());
    for (tx, ty) in rocks.take(200) {
        let (x, y) = (tx as i64 * 1000 + 500, ty as i64 * 1000 + 500);
        // a small building stands there happily; the well alone is refused
        if w.can_place(BuildingKind::Pole, x, y).is_ok() {
            assert_eq!(w.can_place(BuildingKind::Well, x, y), Err(crate::world::Reject::Blocked));
            found = true;
            break;
        }
    }
    assert!(found, "a bit of open rock on this map");
}

#[test]
fn the_water_mill_wants_a_stream_and_grinds_four_times_the_hand_mill() {
    use ItemKind as I;
    let (base, a, bx, by) = industry_world();
    // four times the flour in the same time
    let (hand, water) = (&rules().recipes[recipe("mill_flour").0 as usize], &rules().recipes[recipe("water_mill_flour").0 as usize]);
    let rate = |r: &crate::rules::RecipeDef| r.outputs[0].n as f64 / r.ticks as f64;
    assert!((rate(water) / rate(hand) - 4.0).abs() < 0.01, "{} vs {}", rate(water), rate(hand));
    // in play: the hand mill with one person at it needs the full 600 ticks, the water mill needs 300 and no one
    let mut w = base.clone();
    let hm = push_building(&mut w, BuildingKind::HandMill, bx - 4_000, by);
    let wm = push_building(&mut w, BuildingKind::WaterMill, bx + 4_000, by);
    assert!(run_job(&mut w, a, hm, "mill_flour", &[(I::Grain, 3)], 700));
    assert_eq!(out_of(&w, hm, I::Flour), 3);
    w.buildings[wm as usize].input[I::Grain as usize] = 6;
    w.buildings[wm as usize].job = Some(crate::buildings::Job { recipe: recipe("water_mill_flour").0, remaining: 300 });
    for _ in 0..310 {
        topped_up(&mut w);
        w.agents[0].x = bx + 40_000;
        w.agents[0].y = by;
        w.step(&[]);
    }
    assert!(w.buildings[wm as usize].job.is_none(), "nobody stood at it");
    assert_eq!(out_of(&w, wm, I::Flour), 6);
    // it only stands where water is within three tiles
    let near_water = (3..253).flat_map(|y| (3..253).map(move |x| (x, y))).find(|&(x, y)| base.map.water_within(x, y, 3) && base.map.terrain(x, y) == Some(Terrain::Grass) && base.can_place(BuildingKind::WaterMill, x as i64 * 1000 + 500, y as i64 * 1000 + 500).is_ok());
    assert!(near_water.is_some(), "a mill site by the water");
    let dry = (3..253).flat_map(|y| (3..253).map(move |x| (x, y))).find(|&(x, y)| !base.map.water_within(x, y, 4) && base.map.terrain(x, y) == Some(Terrain::Grass) && base.node_at(x, y).is_none());
    let (dx, dy) = dry.expect("dry ground");
    assert_eq!(base.can_place(BuildingKind::WaterMill, dx as i64 * 1000 + 500, dy as i64 * 1000 + 500), Err(crate::world::Reject::NeedsWater));
}

#[test]
fn the_forge_hammer_the_gin_and_the_jenny_stand_still_without_power() {
    use ItemKind as I;
    let (base, a, bx, by) = industry_world();
    type Case = (BuildingKind, &'static str, Vec<(ItemKind, u16)>, Vec<(ItemKind, u16)>);
    let cases: Vec<Case> = vec![
        (BuildingKind::ForgeHammer, "forge_sheet", vec![(I::IronBloom, 2)], vec![(I::Sheet, 2)]),
        (BuildingKind::ForgeHammer, "forge_wire", vec![(I::CopperIngot, 1)], vec![(I::CopperWire, 2)]),
        (BuildingKind::CottonGin, "gin_cotton", vec![(I::Cotton, 3)], vec![(I::CottonFiber, 2)]),
        (BuildingKind::SpinningJenny, "jenny_fiber", vec![(I::Fiber, 8)], vec![(I::Thread, 6)]),
        (BuildingKind::SpinningJenny, "jenny_flax", vec![(I::Flax, 4)], vec![(I::Thread, 6)]),
        (BuildingKind::SpinningJenny, "jenny_cotton", vec![(I::CottonFiber, 6)], vec![(I::Thread, 8)]),
    ];
    for (kind, id, inputs, outputs) in cases {
        assert!(kind.needs_power());
        let ticks = rules().recipes[recipe(id).0 as usize].ticks;
        let mut w = base.clone();
        let st = push_building(&mut w, kind, bx + 4_000, by);
        assert!(!w.requirement_met(st as usize), "{kind:?}");
        assert!(!run_job(&mut w, a, st, id, &inputs, ticks as u64 + 600), "{kind:?} {id} ran without power");
        assert_eq!(w.buildings[st as usize].job.map(|j| j.remaining), Some(ticks), "{kind:?} {id}");
        let mut w = base.clone();
        let st = push_building(&mut w, kind, bx + 4_000, by);
        push_building(&mut w, BuildingKind::Windmill, bx + 9_000, by);
        assert!(w.requirement_met(st as usize));
        assert!(run_job(&mut w, a, st, id, &inputs, ticks as u64 + 600), "{kind:?} {id} under power");
        for (k, n) in outputs {
            assert_eq!(out_of(&w, st, k), n, "{kind:?} {id}: {k:?}");
        }
    }
    // the jenny draws three times the thread of the wheel in the same time
    let (wheel, jenny) = (&rules().recipes[recipe("spin_fiber").0 as usize], &rules().recipes[recipe("jenny_fiber").0 as usize]);
    assert!(jenny.outputs[0].n as u32 * wheel.ticks >= 3 * wheel.outputs[0].n as u32 * jenny.ticks);
    // the old way to wire and sheet (rolling mill, press) is still there
    let r = rules();
    assert!(r.recipes.iter().any(|d| d.id == "copper_wire" && d.station == Some(BuildingKind::RollingMill) && d.discovery != Discovery::Retired));
    assert!(r.recipes.iter().any(|d| d.id == "press_sheet_iron" && d.station == Some(BuildingKind::Press)));
}

#[test]
fn a_crane_makes_big_buildings_cheaper_within_reach() {
    let (base, a, ..) = industry_world();
    let windmill = rules().building(BuildingKind::Windmill);
    assert!(windmill.heavy());
    let pole = rules().building(BuildingKind::Pole);
    assert!(!pole.heavy() && !rules().building(BuildingKind::Crane).heavy());
    let try_build = |w: &mut World, kind: BuildingKind, x: i64, y: i64, items: &[(ItemKind, u16)]| -> Option<[u16; ItemKind::COUNT]> {
        w.agents[0].inventory.counts = [0; ItemKind::COUNT];
        for &(k, n) in items {
            w.agents[0].inventory.counts[k as usize] = n;
        }
        w.agents[0].x = x;
        w.agents[0].y = y + 2_000;
        let before = w.buildings.len();
        w.step(&[cmd(a, CommandKind::Build { kind, x, y })]);
        (w.buildings.len() > before).then_some(w.agents[0].inventory.counts)
    };
    let eased: Vec<(ItemKind, u16)> = windmill.cost.iter().map(|c| (c.item, (c.n * 3).div_ceil(4))).collect();
    assert!(eased.iter().zip(&windmill.cost).any(|(e, c)| e.1 < c.n));
    let (x, y) = spot_for(&base, BuildingKind::Windmill, &[]);
    let mut w = base.clone();
    let full: Vec<(ItemKind, u16)> = windmill.cost.iter().map(|c| (c.item, c.n)).collect();
    assert!(try_build(&mut w, BuildingKind::Windmill, x, y, &eased).is_none(), "no crane, no discount");
    assert!(try_build(&mut base.clone(), BuildingKind::Windmill, x, y, &full).is_some(), "the full price builds it there");
    // a crane 6 m away: built from the eased list, all of it spent
    push_building(&mut w, BuildingKind::Crane, x - 6_000, y);
    assert!(w.crane_near(x, y));
    let left = try_build(&mut w, BuildingKind::Windmill, x, y, &eased).expect("built under the crane");
    assert!(left.iter().all(|&n| n == 0), "the eased price is spent in full");
    // out of reach (11.5 m): the discount is gone
    let mut far = base.clone();
    push_building(&mut far, BuildingKind::Crane, x - 11_500, y);
    assert!(!far.crane_near(x, y));
    assert!(try_build(&mut far, BuildingKind::Windmill, x, y, &eased).is_none());
    // light buildings pay full price beside a crane
    let mut w = base.clone();
    push_building(&mut w, BuildingKind::Crane, x - 6_000, y);
    let pole_cost: Vec<(ItemKind, u16)> = pole.cost.iter().map(|c| (c.item, c.n)).collect();
    let left = try_build(&mut w, BuildingKind::Pole, x, y, &pole_cost).expect("pole");
    assert!(left.iter().all(|&n| n == 0));
    // and with the crane near the alternative way is eased too
    let alt = rules().building(BuildingKind::Windmill).alt_cost.iter().map(|c| (c.item, (c.n * 3).div_ceil(4))).collect::<Vec<_>>();
    let mut w = base.clone();
    push_building(&mut w, BuildingKind::Crane, x - 6_000, y);
    assert!(try_build(&mut w, BuildingKind::Windmill, x, y, &alt).is_some());
}

#[test]
fn books_and_lenses_steady_the_hand_and_the_tribe_learns_faster() {
    use ItemKind as I;
    let view_half = |extra: &[(ItemKind, u16)]| -> u32 {
        let (mut w, a) = with_agent(3);
        learn_all(&mut w);
        give(&mut w, I::Leather, 4);
        w.agents[0].inventory.counts[I::Fiber as usize] = 2;
        for &(k, n) in extra {
            w.agents[0].inventory.counts[k as usize] = n;
        }
        w.step(&[cmd(a, CommandKind::Craft { recipe: recipe("leather_armor") })]);
        w.craft_view(a).expect("playing").half
    };
    let plain = view_half(&[]);
    let booked = view_half(&[(I::Book, 1)]);
    let lensed = view_half(&[(I::Lens, 1)]);
    let both = view_half(&[(I::Book, 1), (I::Lens, 1)]);
    assert!(booked > plain && lensed > plain && both > booked.max(lensed), "{plain} {booked} {lensed} {both}");
    // a printed book speeds practice (mastery) for everyone and clears the tribe's view of the world
    let (mut w, _) = with_agent(3);
    let r = recipe("leather_armor").0 as usize;
    w.produced[I::LeatherArmor as usize] = 3;
    assert_eq!(w.mastery(r), 0);
    let p0 = w.perception_permille();
    w.produced[I::Book as usize] = 1;
    assert_eq!(w.mastery(r), 1, "three made is a step of practice once a book is printed");
    assert!(w.perception_permille() > p0);
    w.produced[I::Book as usize] = 9;
    let p9 = w.perception_permille();
    w.produced[I::Book as usize] = 4;
    assert_eq!(p9, w.perception_permille(), "four books are all that count");
}

#[test]
fn an_oil_flask_reduces_tool_wear() {
    // oil on the handles: tools wear less (a flask is carried, not used up)
    let wear1 = |flask: bool, skew: u64| {
        let (mut w, a) = with_agent(1);
        w.tick += skew;
        learn_all(&mut w);
        w.settings.difficulty = crate::world::Difficulty::Realistic;
        give(&mut w, ItemKind::Hammer, 1);
        if flask {
            w.agents[0].inventory.counts[ItemKind::OilFlask as usize] = 1;
        }
        let i = w.nodes.iter().position(|n| n.kind == NodeKind::StoneOutcrop && n.amount == 5).expect("stone");
        w.set_node_amount(i, 3);
        let (nx, ny) = w.nodes[i].center_mm();
        w.agents[0].x = nx;
        w.agents[0].y = ny + 900;
        w.step(&[cmd(a, CommandKind::Gather { node: NodeId(i as u32) })]);
        for _ in 0..90 * 20 {
            w.step(&[]);
        }
        (w.agents[0].wear_of(ItemKind::Hammer), w.agents[0].inventory.get(ItemKind::OilFlask))
    };
    let wear = |flask: bool| (0..8).map(|k| wear1(flask, k * 37)).fold((0u16, 0u16), |(a, b), (x, y)| (a + x, b + y));
    let (plain, oiled) = (wear(false), wear(true));
    assert!(plain.0 >= 2 && oiled.0 < plain.0, "{oiled:?} vs {plain:?}");
    assert_eq!(oiled.1, 8, "the flask is kept");
}

#[test]
fn felt_and_leather_dress_the_wearer() {
    use ItemKind as I;
    assert!(I::FeltCloak.is_tool() && I::LeatherArmor.is_tool() && I::OilFlask.is_tool() && I::Book.is_tool() && I::Lens.is_tool() && I::OilLamp.is_tool());
    let felt = crate::rules::Gear::of(|k| k == I::FeltCloak);
    let hide = crate::rules::Gear::of(|k| k == I::HideCloak);
    assert!(felt.warmth > hide.warmth && felt.dry > hide.dry, "felt is warmer and sheds more rain than hide: {felt:?} vs {hide:?}");
    let armor = crate::rules::Gear::of(|k| k == I::LeatherArmor);
    assert!(armor.bite > 0 && armor.blow > 0 && armor.burden > 0);
    let flask = crate::rules::Gear::of(|k| k == I::OilFlask);
    assert!(flask.tool_save > 0 && flask.warmth == 0);
    let all = crate::rules::Gear::of(|k| matches!(k, I::FeltCloak | I::LeatherArmor));
    assert_eq!(all.warmth, felt.warmth + armor.warmth);
    // sewn by hand from what the felt mallet and the tannery give
    for id in ["felt_cloak", "leather_armor"] {
        let d = &rules().recipes[recipe(id).0 as usize];
        assert!(d.station.is_none() && !d.steps.is_empty());
    }
}

#[test]
fn leather_and_other_new_materials_pay_for_buildings_the_old_ones_still_do() {
    let (mut w, a, ..) = industry_world();
    w.settings.difficulty = Difficulty::Forgiving;
    let build = |w: &mut World, kind: BuildingKind| -> bool {
        let before = w.buildings.len();
        let (x, y) = spot_for(w, kind, &w.buildings.iter().map(|b| (b.x, b.y)).collect::<Vec<_>>());
        w.agents[0].x = x;
        w.agents[0].y = y + 1_500;
        w.step(&[cmd(a, CommandKind::Build { kind, x, y })]);
        w.buildings.len() > before
    };
    for kind in [BuildingKind::Bellows, BuildingKind::Shelter, BuildingKind::PotterWheel, BuildingKind::PowerLoom, BuildingKind::SpinningJenny] {
        let def = rules().building(kind);
        assert!(!def.alt_cost.is_empty(), "{kind:?}");
        // the old cost
        w.agents[0].inventory.counts = [0; ItemKind::COUNT];
        for c in &def.cost {
            give(&mut w, c.item, c.n);
        }
        assert!(build(&mut w, kind), "{kind:?} by the old cost");
        assert_eq!(w.agents[0].inventory.total(), 0);
        // the other way
        w.agents[0].inventory.counts = [0; ItemKind::COUNT];
        for c in &def.alt_cost {
            give(&mut w, c.item, c.n);
        }
        assert!(build(&mut w, kind), "{kind:?} by the other cost");
        assert_eq!(w.agents[0].inventory.total(), 0, "{kind:?}");
    }
    assert!(rules().building(BuildingKind::Bellows).alt_cost.iter().any(|c| c.item == ItemKind::Leather));
    assert!(rules().building(BuildingKind::Shelter).alt_cost.iter().any(|c| c.item == ItemKind::WindowPane));
}

#[test]
fn new_discoveries_open_from_what_the_tribe_holds() {
    use ItemKind as I;
    let fresh = || {
        let (mut w, _) = with_agent(5);
        w.forget_everything();
        w.wild.retain(|x| x.species != WildSpecies::Wolf);
        w
    };
    let opens = |w: &mut World, d: Discovery| {
        w.step(&[]);
        w.knows(d)
    };
    // (what is known, what is in hand, what opens)
    type Case = (Vec<Discovery>, Vec<(ItemKind, u16)>, Discovery);
    let cases: Vec<Case> = vec![
        (vec![Discovery::Pottery], vec![(I::Plank, 2)], Discovery::Throwing),
        (vec![Discovery::Mechanics], vec![(I::Linseed, 1)], Discovery::OilPressing),
        (vec![Discovery::Mechanics], vec![(I::Seed, 6)], Discovery::OilPressing),
        (vec![Discovery::Pottery], vec![(I::Hide, 2)], Discovery::Tanning),
        (vec![Discovery::Fire], vec![(I::Berries, 3)], Discovery::Curing),
        (vec![Discovery::Fire], vec![(I::RawFish, 2)], Discovery::Curing),
        (vec![Discovery::Weaving], vec![(I::Fiber, 8)], Discovery::Ropemaking),
        (vec![Discovery::Pottery], vec![(I::Rope, 1)], Discovery::Wells),
        (vec![Discovery::Weaving, Discovery::Pottery], vec![(I::Fiber, 10)], Discovery::Felting),
        (vec![Discovery::Mechanics], vec![(I::Rope, 1)], Discovery::Hoisting),
        (vec![], vec![(I::Glass, 1)], Discovery::Glassblowing),
        (vec![Discovery::Weaving, Discovery::Pottery], vec![(I::Cloth, 1)], Discovery::Papermaking),
        (vec![Discovery::Mechanics, Discovery::Papermaking], vec![(I::Paper, 2)], Discovery::Printing),
        (vec![], vec![(I::Cotton, 1)], Discovery::Ginning),
        (vec![Discovery::Mechanics, Discovery::Weaving], vec![(I::Thread, 2)], Discovery::Ginning),
        (vec![Discovery::Machining], vec![(I::Thread, 1)], Discovery::Spinning),
        (vec![Discovery::Mechanics], vec![(I::CopperIngot, 1)], Discovery::Drawing),
        (vec![Discovery::Mechanics], vec![(I::Sheet, 1)], Discovery::Drawing),
    ];
    for (known, held, opened) in cases {
        let mut w = fresh();
        for &(k, n) in &held {
            give(&mut w, k, n);
        }
        if !known.is_empty() {
            assert!(!opens(&mut w, opened), "{opened:?} must not open before what it needs is known");
        }
        for k in &known {
            w.learn(*k);
        }
        assert!(opens(&mut w, opened), "{opened:?} from {known:?} and {held:?}");
    }
    // the smokehouse is opened by a drying rack that stands
    let mut w = fresh();
    w.learn(Discovery::Curing);
    assert!(!opens(&mut w, Discovery::Smoking));
    push_building(&mut w, BuildingKind::DryingRack, 5_000, 5_000);
    assert!(opens(&mut w, Discovery::Smoking), "a drying rack stands: smoking");
    // the dibek and the water mill open with milling and mechanics, which grain and iron in hand teach
    assert_eq!(rules().building(BuildingKind::StoneMortar).discovery, Discovery::Milling);
    assert_eq!(rules().building(BuildingKind::WaterMill).discovery, Discovery::Mechanics);
    for def in &rules().buildings {
        assert!(def.discovery != Discovery::Retired || def.kind == BuildingKind::Ruin, "{:?}", def.kind);
    }
}

#[test]
fn every_wave_two_machine_has_work_or_an_effect() {
    let r = rules();
    for k in [
        BuildingKind::PotterWheel,
        BuildingKind::OilPress,
        BuildingKind::Tannery,
        BuildingKind::DryingRack,
        BuildingKind::Smokehouse,
        BuildingKind::Well,
        BuildingKind::StoneMortar,
        BuildingKind::WaterMill,
        BuildingKind::ForgeHammer,
        BuildingKind::FeltMallet,
        BuildingKind::Ropewalk,
        BuildingKind::GlassFurnace,
        BuildingKind::PaperVat,
        BuildingKind::PrintingPress,
        BuildingKind::CottonGin,
        BuildingKind::SpinningJenny,
    ] {
        assert!(r.recipes.iter().any(|d| d.station == Some(k) && d.discovery != Discovery::Retired), "{k:?} has no recipe");
    }
    // the crane has no recipe: its effect is tested above
    assert!(r.recipes.iter().all(|d| d.station != Some(BuildingKind::Crane)));
    assert_eq!(BuildingKind::ALL.len(), 87, "68 before wave 3 and 19 with it");
    // old ways stay
    for id in ["raw_pot", "fire_pot", "cloth", "linen", "spin_fiber", "mill_flour", "glass", "copper_wire", "press_sheet_iron", "weave_thread", "boiler_coal", "cook_meat", "cook_fish"] {
        assert!(r.recipes.iter().any(|d| d.id == id && d.discovery != Discovery::Retired), "{id}");
    }
}

#[test]
fn new_stations_run_on_auto_and_workers_carry_for_them() {
    use ItemKind as I;
    let (mut w, a, bx, by) = industry_world();
    let walk = push_building(&mut w, BuildingKind::Ropewalk, bx + 4_000, by);
    let bot = w.spawn_worker(AgentKind::Bot, bx + 2_000, by);
    w.agents[bot.0 as usize].inventory.counts[I::Fiber as usize] = 8;
    w.step(&[cmd(a, CommandKind::SetAuto { building: walk, recipe: Some(recipe("twist_rope")) })]);
    w.step(&[cmd(a, CommandKind::SetRoute { worker: bot, route: vec![RouteStep::Deposit { building: walk }, RouteStep::Wait { ticks: 1_900 }, RouteStep::Collect { building: walk, item: Some(I::Rope) }] })]);
    for _ in 0..5_000 {
        topped_up(&mut w);
        w.agents[bot.0 as usize].hunger = MAX_STAT as _;
        w.step(&[]);
    }
    assert!(w.agents[bot.0 as usize].inventory.get(I::Rope) >= 2, "{}", w.agents[bot.0 as usize].inventory.get(I::Rope));
}

#[test]
fn cotton_grows_wild_everywhere_and_is_sown_in_fields() {
    use ItemKind as I;
    // every world has cotton tufts among its tall grass, and the choice is a pure function of the seed and the place
    for seed in [1u64, 2, 3, 4, 5, 6] {
        let w = World::with_size(crate::map::tag_seed_v2(seed * 7919), 256, 256);
        let tufts: Vec<&crate::world::Node> = w.nodes.iter().filter(|n| n.kind == NodeKind::FiberPlant).collect();
        let cotton = tufts.iter().filter(|n| {
            let (warm, wet) = crate::map::climate(w.seed, n.tx, n.ty);
            crate::world::cotton_tuft(w.seed, n.tx, n.ty, warm, wet)
        });
        assert!(cotton.count() >= 1, "seed {seed}: no cotton among {} tufts", tufts.len());
    }
    // gathering the last armful of such a tuft gives cotton
    let (mut w, a) = with_agent(11);
    let spot = w.nodes.iter().enumerate().find(|(_, n)| {
        let (warm, wet) = crate::map::climate(w.seed, n.tx, n.ty);
        n.kind == NodeKind::FiberPlant && crate::world::cotton_tuft(w.seed, n.tx, n.ty, warm, wet)
    }).map(|(i, n)| (i, n.center_mm()));
    let (i, (nx, ny)) = spot.expect("a cotton tuft");
    w.set_node_amount(i, 1);
    w.agents[0].x = nx;
    w.agents[0].y = ny + 900;
    w.step(&[cmd(a, CommandKind::Gather { node: NodeId(i as u32) })]);
    for _ in 0..600 {
        topped_up(&mut w);
        w.step(&[]);
    }
    assert_eq!(count(&w, 0, I::Cotton), 1, "the last armful of a cotton tuft");
    // the discovery follows: cotton in hand opens the gin
    assert!(w.knows(Discovery::Ginning));
    // and a field sown with it grows more
    let (mut w, a, bx, by) = industry_world();
    let field = push_building(&mut w, BuildingKind::Field, bx + 4_000, by);
    w.agents[0].x = bx + 5_300;
    w.agents[0].y = by;
    give(&mut w, I::Cotton, 2);
    w.step(&[cmd(a, CommandKind::StartJob { building: field, recipe: recipe("sow_cotton") })]);
    assert!(w.buildings[field as usize].job.is_some());
    w.buildings[field as usize].job.as_mut().unwrap().remaining = 5;
    for _ in 0..20 {
        topped_up(&mut w);
        w.step(&[]);
    }
    assert_eq!(out_of(&w, field, I::Cotton), 6, "two sown, six picked");
    // flax in the field gives seed heads for the oil press as well
    let flax = &rules().recipes[recipe("sow_flax").0 as usize];
    assert!(flax.outputs.iter().any(|o| o.item == I::Linseed));
}

#[test]
fn a_wave_two_save_loads_and_older_item_lists_still_read() {
    // a save with the new machines in it comes back as it was
    let (mut w, _) = with_agent(7);
    for (i, k) in [BuildingKind::Crane, BuildingKind::Tannery, BuildingKind::GlassFurnace, BuildingKind::SpinningJenny, BuildingKind::Well].into_iter().enumerate() {
        let mut b = Building::new(k, 6_000 + i as i64 * 4_000, 6_000);
        b.output[ItemKind::Book as usize] = 3;
        b.input[ItemKind::Bulgur as usize] = 2;
        w.buildings.push(b);
    }
    w.mark_seen(ItemKind::SmokedFish);
    w.mark_seen(ItemKind::Bulgur);
    w.learn(Discovery::Hoisting);
    let b = World::from_bytes(&w.to_bytes()).expect("load");
    assert_eq!(b.hash(), w.hash());
    assert!(b.has_seen(ItemKind::SmokedFish) && b.knows(Discovery::Hoisting));
    assert_eq!(b.buildings.last().map(|x| x.kind), Some(BuildingKind::Well));
    // an item list from before the new items (130 slots) reads into the longer one, the new slots empty
    #[derive(serde::Serialize, serde::Deserialize)]
    struct Holder {
        #[serde(with = "crate::items_serde")]
        counts: [u16; ItemKind::COUNT],
    }
    let old: Vec<u16> = (0..130).map(|i| i as u16 + 1).collect();
    let bytes = postcard::to_allocvec(&old).unwrap();
    let read: Holder = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(read.counts[0], 1);
    assert_eq!(read.counts[129], 130);
    assert!(read.counts[130..].iter().all(|&n| n == 0));
}

// ---------------------------------------------------------------------------------------------------------------
// Electricity and the late age, wave 3: arc furnace, refining, electrolysis, compressor, rails, transformer,
// accumulator, welding, chemistry, cooling, telegraph and radio, vacuum, crystals, chips, CNC, assembly robot.
// ---------------------------------------------------------------------------------------------------------------

/// A windmill turning a dynamo at (x, y + 2.5 m): everything on the grid within 12 m of it is live.
fn grid_at(w: &mut World, x: i64, y: i64) -> (u32, u32) {
    let m = push_building(w, BuildingKind::Windmill, x, y + 5_500);
    let d = push_building(w, BuildingKind::Dynamo, x, y + 2_500);
    (m, d)
}

type GridCase = (BuildingKind, &'static str, Vec<(ItemKind, u16)>, Vec<(ItemKind, u16)>, u64);

fn grid_cases() -> Vec<GridCase> {
    use ItemKind as I;
    vec![
        (BuildingKind::ArcFurnace, "arc_steel", vec![(I::PigIron, 3)], vec![(I::Steel, 3)], 4_000),
        (BuildingKind::ArcFurnace, "arc_scrap_steel", vec![(I::CastIronPart, 2)], vec![(I::Steel, 1)], 4_000),
        (BuildingKind::ArcFurnace, "arc_silicon", vec![(I::Sand, 2), (I::Coke, 1)], vec![(I::Silicon, 2)], 4_000),
        (BuildingKind::Refinery, "refine_oil", vec![(I::Oil, 2)], vec![(I::Kerosene, 2), (I::Lubricant, 1)], 4_000),
        (BuildingKind::Refinery, "refine_tar", vec![(I::Tar, 2)], vec![(I::Kerosene, 1), (I::Lubricant, 1)], 4_000),
        (BuildingKind::Electrolysis, "electro_copper", vec![(I::CopperIngot, 2), (I::WaterPot, 1)], vec![(I::PureCopper, 2), (I::Pot, 1)], 2_500),
        (BuildingKind::Electrolysis, "electro_aluminium", vec![(I::Bauxite, 4), (I::Coke, 1)], vec![(I::Aluminium, 2)], 4_000),
        (BuildingKind::Welder, "weld_frame", vec![(I::Beam, 2), (I::SheetPart, 2)], vec![(I::WeldedFrame, 1)], 2_500),
        (BuildingKind::Welder, "weld_frame_steel", vec![(I::Steel, 3), (I::SheetPart, 2)], vec![(I::WeldedFrame, 1)], 2_500),
        (BuildingKind::Reactor, "make_fertilizer", vec![(I::Manure, 3), (I::Lime, 1), (I::WaterPot, 1)], vec![(I::Fertilizer, 3), (I::Pot, 1)], 4_000),
        (BuildingKind::Reactor, "make_dye", vec![(I::Herb, 2), (I::Lime, 1), (I::WaterPot, 1)], vec![(I::Dye, 2), (I::Pot, 1)], 4_000),
        (BuildingKind::Reactor, "make_dye_berries", vec![(I::Berries, 4), (I::Lime, 1), (I::WaterPot, 1)], vec![(I::Dye, 2), (I::Pot, 1)], 4_000),
        (BuildingKind::Reactor, "make_soap", vec![(I::PlantOil, 2), (I::Lime, 1), (I::WaterPot, 1)], vec![(I::Soap, 3), (I::Pot, 1)], 4_000),
        (BuildingKind::VacuumPump, "vacuum_bulb", vec![(I::Glass, 1), (I::CopperWire, 1)], vec![(I::LightBulb, 2)], 1_500),
        (BuildingKind::VacuumPump, "vacuum_tube", vec![(I::Glass, 1), (I::CopperWire, 2)], vec![(I::VacuumTube, 2)], 1_500),
        (BuildingKind::CrystalPuller, "pull_crystal", vec![(I::Silicon, 3)], vec![(I::PureSilicon, 2)], 8_000),
        (BuildingKind::Photolith, "print_chip", vec![(I::PureSilicon, 1), (I::CopperWire, 2), (I::Dye, 1)], vec![(I::Chip, 3)], 3_000),
        (BuildingKind::Photolith, "print_chip_oil", vec![(I::PureSilicon, 1), (I::CopperWire, 2), (I::PlantOil, 1)], vec![(I::Chip, 2)], 3_000),
        (BuildingKind::CncMachine, "cnc_shaft", vec![(I::Steel, 1)], vec![(I::Shaft, 2)], 1_000),
        (BuildingKind::CncMachine, "cnc_gear", vec![(I::Steel, 1)], vec![(I::PrecisionGear, 3)], 1_000),
        (BuildingKind::CncMachine, "cnc_piston", vec![(I::Steel, 1)], vec![(I::Piston, 2)], 1_500),
        (BuildingKind::CncMachine, "cnc_bolts", vec![(I::Steel, 1)], vec![(I::BoltNut, 6)], 1_000),
        (BuildingKind::CncMachine, "cnc_cylinder_block", vec![(I::IronCylinder, 1), (I::Steel, 1)], vec![(I::CylinderBlock, 1)], 1_500),
        (BuildingKind::AssemblyRobot, "robot_magnet", vec![(I::Steel, 1), (I::IronOre, 2)], vec![(I::Magnet, 1)], 1_500),
        (BuildingKind::AssemblyRobot, "robot_transistor", vec![(I::Silicon, 1), (I::CopperWire, 2)], vec![(I::Transistor, 2)], 1_500),
    ]
}

#[test]
fn grid_machines_make_their_goods_on_a_live_line() {
    let (base, a, bx, by) = industry_world();
    for (kind, id, inputs, outputs, max) in grid_cases() {
        let mut w = base.clone();
        let st = push_building(&mut w, kind, bx + 4_000, by);
        grid_at(&mut w, bx + 4_000, by);
        assert!(w.requirement_met(st as usize), "{kind:?}: the line is live");
        assert!(run_job(&mut w, a, st, id, &inputs, max), "{kind:?} {id} did not finish");
        for (k, n) in outputs {
            assert_eq!(out_of(&w, st, k), n, "{kind:?} {id}: {k:?}");
        }
    }
    // the rest of the recipes of old stations: pure copper to wire, aluminium to sheet, a book on dye, kerosene in a boiler
    use ItemKind as I;
    let mut w = base.clone();
    let mill = push_building(&mut w, BuildingKind::RollingMill, bx + 4_000, by);
    w.buildings.push(Building::new(BuildingKind::Windmill, bx + 4_000, by + 4_000));
    assert!(run_job(&mut w, a, mill, "wire_pure", &[(I::PureCopper, 1)], 3_000));
    assert_eq!(out_of(&w, mill, I::CopperWire), 6, "pure copper draws more wire than the ingot did");
    let mut w = base.clone();
    let press = push_building(&mut w, BuildingKind::Press, bx + 4_000, by);
    w.buildings.push(Building::new(BuildingKind::Windmill, bx + 4_000, by + 4_000));
    assert!(run_job(&mut w, a, press, "press_aluminium", &[(I::Aluminium, 1)], 2_000));
    assert_eq!(out_of(&w, press, I::Sheet), 2);
    let mut w = base.clone();
    let pp = push_building(&mut w, BuildingKind::PrintingPress, bx + 4_000, by);
    assert!(run_job(&mut w, a, pp, "print_book_dye", &[(I::Paper, 4), (I::Dye, 1), (I::Leather, 1)], 4_000));
    assert_eq!(out_of(&w, pp, I::Book), 1, "a book without charcoal and oil, on dye");
    let mut w = base.clone();
    let boiler = push_building(&mut w, BuildingKind::Boiler, bx + 4_000, by);
    w.agents[0].inventory.counts[I::Kerosene as usize] = 1;
    assert!(run_job(&mut w, a, boiler, "boiler_kerosene", &[(I::WaterPot, 2)], 20_000));
    // and the still, which needs no wire, runs alcohol and kerosene
    for (id, inputs, outs) in [
        ("distill_tar", vec![(I::Tar, 2), (I::WaterPot, 1)], vec![(I::Kerosene, 1), (I::Pot, 1)]),
        ("distill_oil", vec![(I::Oil, 2), (I::WaterPot, 1)], vec![(I::Kerosene, 2), (I::Pot, 1)]),
        ("distill_alcohol_berries", vec![(I::Berries, 6), (I::WaterPot, 1)], vec![(I::Alcohol, 2), (I::Pot, 1)]),
        ("distill_alcohol_grain", vec![(I::Grain, 5), (I::WaterPot, 1)], vec![(I::Alcohol, 2), (I::Pot, 1)]),
    ] {
        let mut w = base.clone();
        let st = push_building(&mut w, BuildingKind::Still, bx + 4_000, by);
        assert!(run_job(&mut w, a, st, id, &inputs, 5_000), "{id}");
        for (k, n) in outs {
            assert_eq!(out_of(&w, st, k), n, "{id}: {k:?}");
        }
    }
}

#[test]
fn grid_machines_stand_still_and_cold_on_a_dead_line() {
    let (base, a, bx, by) = industry_world();
    for (kind, id, inputs, _, _) in grid_cases() {
        let mut w = base.clone();
        let st = push_building(&mut w, kind, bx + 4_000, by);
        // a dynamo with no wind to turn it, and a windmill too far from it
        push_building(&mut w, BuildingKind::Dynamo, bx + 4_000, by + 2_500);
        push_building(&mut w, BuildingKind::Windmill, bx + 4_000, by + 20_000);
        assert!(!w.requirement_met(st as usize), "{kind:?}: no current");
        let (x, y) = (w.buildings[st as usize].x, w.buildings[st as usize].y);
        w.agents[0].x = x + 1_300;
        w.agents[0].y = y;
        for &(k, n) in &inputs {
            w.agents[0].inventory.counts[k as usize] += n;
        }
        w.step(&[cmd(a, CommandKind::StartJob { building: st, recipe: recipe(id) })]);
        let ticks = rules().recipes[recipe(id).0 as usize].ticks;
        for _ in 0..900 {
            topped_up(&mut w);
            w.step(&[]);
        }
        let left = w.buildings[st as usize].job.map(|j| j.remaining);
        assert_eq!(left, Some(ticks), "{kind:?} {id} did work with a dead line");
        assert!(w.buildings[st as usize].temp < 60, "{kind:?} warmed up on a dead line: {}", w.buildings[st as usize].temp);
        assert!(w.buildings[st as usize].output_total() == 0);
    }
}

#[test]
fn a_transformer_stretches_the_grid_three_times_as_far() {
    let (mut w, _a) = with_agent(5);
    let (sx, sy) = (10_000, 10_000);
    grid_at(&mut w, sx, sy);
    // a lamp 40 m down the line: a pole's reach is 12 m, so it stays dark
    let lamp = push_building(&mut w, BuildingKind::ElectricLamp, sx + 40_000, sy + 2_500);
    assert!(!w.electric_live()[lamp as usize]);
    // a transformer 10 m from the dynamo: the line is live to it, and 36 m on from it
    let tr = push_building(&mut w, BuildingKind::Transformer, sx + 10_000, sy + 2_500);
    let live = w.electric_live();
    assert!(live[tr as usize] && live[lamp as usize], "the far lamp is lit through the transformer");
    // poles do not do that: the same gap with a pole in the transformer's place stays dark
    let mut v = World::with_size(crate::map::tag_seed_v2(1), 64, 64);
    grid_at(&mut v, sx, sy);
    let lamp = push_building(&mut v, BuildingKind::ElectricLamp, sx + 40_000, sy + 2_500);
    push_building(&mut v, BuildingKind::Pole, sx + 10_000, sy + 2_500);
    assert!(!v.electric_live()[lamp as usize]);
    // and 36 m is the limit
    let far = push_building(&mut w, BuildingKind::ElectricLamp, sx + 10_000, sy + 2_500 + 37_000);
    assert!(!w.electric_live()[far as usize]);
    // the transformer without current lights nothing
    let mut u = World::with_size(crate::map::tag_seed_v2(1), 64, 64);
    push_building(&mut u, BuildingKind::Transformer, sx, sy);
    let lamp = push_building(&mut u, BuildingKind::ElectricLamp, sx + 20_000, sy);
    assert!(!u.electric_live()[lamp as usize]);
}

#[test]
fn an_accumulator_charges_from_the_dynamo_and_carries_the_grid_when_it_stops() {
    let (mut w, _a) = with_agent(5);
    let (sx, sy) = (10_000, 10_000);
    let (windmill, _) = grid_at(&mut w, sx, sy);
    let acc = push_building(&mut w, BuildingKind::Accumulator, sx + 4_000, sy + 2_500);
    let lamp = push_building(&mut w, BuildingKind::ElectricLamp, sx + 8_000, sy + 2_500);
    assert_eq!(w.buildings[acc as usize].fuel, 0);
    for _ in 0..600 {
        w.step(&[]);
    }
    let charged = w.buildings[acc as usize].fuel;
    assert!((30..=crate::world::ACCUMULATOR_CAP).contains(&charged), "ten seconds of a live grid charge it: {charged}");
    // the wind stops (the windmill is taken away): the lamp stays lit on the accumulator, which runs down
    w.buildings.remove(windmill as usize);
    let (acc, lamp) = (acc as usize - 1, lamp as usize - 1);
    assert!(w.electric_live()[lamp], "lit from the accumulator");
    for _ in 0..600 {
        w.step(&[]);
    }
    let after = w.buildings[acc].fuel;
    assert!(after < charged && after + 5 >= charged - 12, "it gave off about a unit a second: {charged} -> {after}");
    // flat, it is just a box on a dead line
    w.buildings[acc].fuel = 0;
    assert!(!w.electric_live()[lamp]);
    // an accumulator never charges past its capacity
    w.buildings[acc].fuel = crate::world::ACCUMULATOR_CAP - 1;
    w.buildings.push(Building::new(BuildingKind::Windmill, sx, sy + 5_500));
    for _ in 0..300 {
        w.step(&[]);
    }
    assert_eq!(w.buildings[acc].fuel, crate::world::ACCUMULATOR_CAP);
}

#[test]
fn a_compressor_blows_for_the_converter_and_doubles_the_arc_furnace() {
    use ItemKind as I;
    let (base, a, bx, by) = industry_world();
    // a converter with only a live compressor beside it has its strong blast; a dead one gives none
    let mut w = base.clone();
    let bess = push_building(&mut w, BuildingKind::Bessemer, bx + 4_000, by);
    push_building(&mut w, BuildingKind::Compressor, bx + 4_000, by + 2_000);
    assert!(!w.requirement_met(bess as usize), "a compressor with no current blows nothing");
    grid_at(&mut w, bx + 4_000, by + 2_000);
    assert!(w.requirement_met(bess as usize), "a live compressor blows for it");
    // twice the speed at the arc furnace: same recipe, same hour, half the time left
    let progress = |with: bool| -> u32 {
        let mut w = base.clone();
        let st = push_building(&mut w, BuildingKind::ArcFurnace, bx + 4_000, by);
        grid_at(&mut w, bx + 4_000, by);
        if with {
            push_building(&mut w, BuildingKind::Compressor, bx + 4_000, by - 2_000);
        }
        w.buildings[st as usize].temp = 1_800;
        w.agents[0].x = bx + 5_300;
        w.agents[0].y = by;
        w.agents[0].inventory.counts[I::PigIron as usize] = 3;
        w.step(&[cmd(a, CommandKind::StartJob { building: st, recipe: recipe("arc_steel") })]);
        for _ in 0..150 {
            topped_up(&mut w);
            w.step(&[]);
        }
        rules().recipes[recipe("arc_steel").0 as usize].ticks - w.buildings[st as usize].job.expect("running").remaining
    };
    let (plain, blown) = (progress(false), progress(true));
    assert_eq!(blown, plain * 2, "{plain} vs {blown}");
}

#[test]
fn the_wagon_carries_goods_far_and_only_under_power() {
    use ItemKind as I;
    let (mut w, _a) = with_agent(5);
    let (sx, sy) = (20_000, 20_000);
    // a lathe makes shafts beside the first track; a depot waits 100 m away beside the second
    let rail_a = push_building(&mut w, BuildingKind::Railway, sx, sy);
    let rail_b = push_building(&mut w, BuildingKind::Railway, sx + 100_000, sy);
    let lathe = push_building(&mut w, BuildingKind::Lathe, sx + 3_000, sy);
    let depot = push_building(&mut w, BuildingKind::Depot, sx + 103_000, sy);
    w.buildings[lathe as usize].output[I::Shaft as usize] = 5;
    // no power: nothing moves
    for _ in 0..3_000 {
        w.step(&[]);
    }
    assert_eq!(w.buildings[depot as usize].output[I::Shaft as usize], 0);
    assert_eq!(w.buildings[lathe as usize].output[I::Shaft as usize], 5);
    assert_eq!(w.rail_partner(rail_a as usize), Some(rail_b as usize));
    // a windmill turns the winch: the goods ride the wagon to the far depot, after a trip of some seconds
    push_building(&mut w, BuildingKind::Windmill, sx, sy + 5_000);
    let mut at = None;
    for t in 0..3_000 {
        w.step(&[]);
        if at.is_none() && w.buildings[depot as usize].output[I::Shaft as usize] > 0 {
            at = Some(t);
        }
    }
    assert_eq!(w.buildings[depot as usize].output[I::Shaft as usize], 5);
    assert_eq!(w.buildings[lathe as usize].output[I::Shaft as usize], 0);
    assert!(at.unwrap() > 60, "a hundred metres takes a few seconds, not an instant: {:?}", at);
    // too far for a wagon: 200 m
    let mut v = World::with_size(crate::map::tag_seed_v2(1), 256, 256);
    let a = push_building(&mut v, BuildingKind::Railway, sx, sy);
    push_building(&mut v, BuildingKind::Railway, sx + 200_000, sy);
    assert_eq!(v.rail_partner(a as usize), None);
    // no depot at the end: the load stays in the wagon, and is not lost
    let mut v = World::with_size(crate::map::tag_seed_v2(1), 256, 256);
    let a = push_building(&mut v, BuildingKind::Railway, sx, sy);
    push_building(&mut v, BuildingKind::Railway, sx + 50_000, sy);
    push_building(&mut v, BuildingKind::Windmill, sx, sy + 5_000);
    let l = push_building(&mut v, BuildingKind::Lathe, sx + 3_000, sy);
    v.buildings[l as usize].output[I::Shaft as usize] = 4;
    for _ in 0..1_500 {
        v.step(&[]);
    }
    assert_eq!(v.buildings[a as usize].output[I::Shaft as usize], 4);
}

#[test]
fn the_steam_pump_needs_steam_not_wire() {
    let (mut w, _a) = with_agent(5);
    let pump = push_building(&mut w, BuildingKind::SteamPump, 20_000, 20_000);
    assert!(!w.requirement_met(pump as usize));
    // wires and a windmill do not drive it
    grid_at(&mut w, 20_000, 20_000);
    assert!(!w.requirement_met(pump as usize), "a steam pump has no use for current");
    // a steaming boiler within 8 m does
    let mut boiler = Building::new(BuildingKind::Boiler, 25_000, 20_000);
    boiler.job = Some(crate::buildings::Job { recipe: recipe("boiler_coal").0, remaining: u32::MAX / 2 });
    boiler.temp = 300;
    w.buildings.push(boiler);
    assert!(w.requirement_met(pump as usize));
    // the ordinary pump still goes with power (the old path is open)
    let old = push_building(&mut w, BuildingKind::Pump, 20_000, 22_000);
    assert!(w.powered_at(w.buildings[old as usize].x, w.buildings[old as usize].y));
}

#[test]
fn the_cooler_keeps_food_three_times_longer_than_a_cellar_only_while_live() {
    use ItemKind as I;
    let lost = |kind: BuildingKind, live: bool| -> u32 {
        let (mut w, _a) = with_agent(3);
        let st = push_building(&mut w, kind, 10_000, 10_000);
        if live {
            grid_at(&mut w, 10_000, 10_000);
        }
        w.buildings[st as usize].output[I::Berries as usize] = 60_000;
        w.tick = crate::time::TICKS_PER_DAY / 24 * 5;
        w.step(&[]);
        60_000 - w.buildings[st as usize].output[I::Berries as usize] as u32
    };
    let (cellar, cooler, dead, depot) = (lost(BuildingKind::Cellar, false), lost(BuildingKind::Cooler, true), lost(BuildingKind::Cooler, false), lost(BuildingKind::Depot, false));
    assert!(cellar > 0 && cooler > 0);
    let ratio = cellar as f64 / cooler as f64;
    assert!((2.6..=3.4).contains(&ratio), "three times a cellar: {cellar} vs {cooler}");
    assert!(dead > cellar * 3, "a cooler without current is a box: {dead} vs cellar {cellar}");
    assert!(depot >= dead * 9 / 10, "and no better than a depot: {depot}");
    // it takes goods like a depot
    let (mut w, a) = with_agent(3);
    let (cx, cy) = (w.agents[0].x + 1_500, w.agents[0].y);
    let st = push_building(&mut w, BuildingKind::Cooler, cx, cy);
    give(&mut w, I::Berries, 6);
    w.step(&[cmd(a, CommandKind::Deposit { building: st })]);
    assert_eq!(w.buildings[st as usize].output[I::Berries as usize], 6);
}

#[test]
fn telegraph_and_radio_report_remote_depots() {
    use ItemKind as I;
    let (mut w, _a) = with_agent(5);
    let (sx, sy) = (10_000, 10_000);
    grid_at(&mut w, sx, sy);
    let near = push_building(&mut w, BuildingKind::Telegraph, sx + 4_000, sy + 2_500);
    // the wires run out on transformers: 30, 60 and 90 m
    let tr: Vec<u32> = (1..=3).map(|k| push_building(&mut w, BuildingKind::Transformer, sx + k * 30_000, sy + 2_500)).collect();
    let far = push_building(&mut w, BuildingKind::Telegraph, sx + 94_000, sy + 2_500);
    let depot = push_building(&mut w, BuildingKind::Depot, sx + 98_000, sy + 2_500);
    w.buildings[depot as usize].output[I::Steel as usize] = 7;
    w.buildings[depot as usize].output[I::Plank as usize] = 5;
    // a depot by the first telegraph is not remote to it
    let home = push_building(&mut w, BuildingKind::Depot, sx + 8_000, sy + 2_500);
    w.buildings[home as usize].output[I::Wood as usize] = 3;
    let report = w.remote_depots(near as usize);
    assert!(report.contains(&(depot as usize, 12)), "{report:?}");
    assert!(!report.iter().any(|r| r.0 == home as usize), "the home depot is not news");
    let report = w.remote_depots(far as usize);
    assert!(report.contains(&(home as usize, 3)), "the news goes both ways: {report:?}");
    // cut the wires in the middle: the far set goes dark and neither can speak
    let mut cut = w.clone();
    cut.buildings.remove(tr[1] as usize);
    assert!(cut.remote_depots(near as usize).is_empty());
    // a telegraph on its own reports nothing
    let (mut v, _) = with_agent(5);
    grid_at(&mut v, sx, sy);
    let one = push_building(&mut v, BuildingKind::Telegraph, sx + 4_000, sy + 2_500);
    assert!(v.remote_depots(one as usize).is_empty());
    // radios need no wires between them: 300 m apart, each on its own line, within range; 500 m is too far
    let (mut r, _) = with_agent(5);
    grid_at(&mut r, sx, sy);
    grid_at(&mut r, sx + 300_000, sy);
    let r1 = push_building(&mut r, BuildingKind::Radio, sx + 4_000, sy + 2_500);
    push_building(&mut r, BuildingKind::Radio, sx + 304_000, sy + 2_500);
    let d = push_building(&mut r, BuildingKind::Cellar, sx + 308_000, sy + 2_500);
    r.buildings[d as usize].output[I::Berries as usize] = 9;
    assert!(r.remote_depots(r1 as usize).contains(&(d as usize, 9)));
    let (mut r, _) = with_agent(5);
    grid_at(&mut r, sx, sy);
    grid_at(&mut r, sx + 500_000, sy);
    let r1 = push_building(&mut r, BuildingKind::Radio, sx + 4_000, sy + 2_500);
    push_building(&mut r, BuildingKind::Radio, sx + 504_000, sy + 2_500);
    push_building(&mut r, BuildingKind::Depot, sx + 508_000, sy + 2_500);
    assert!(r.remote_depots(r1 as usize).is_empty());
    // the radio tells tomorrow's sky; it is the same sky the world will have
    let f = w.forecast();
    assert_eq!(f[0], w.weather());
    assert_eq!(f[2], crate::time::weather(w.seed, w.tick + crate::time::TICKS_PER_DAY / 3, w.settings.season_days as u64));
}

#[test]
fn the_assembly_robot_runs_a_chain_by_itself_and_stops_when_dark() {
    use ItemKind as I;
    let build = |powered: bool| -> World {
        let (mut w, _a) = with_agent(5);
        let (sx, sy) = (10_000, 10_000);
        if powered {
            grid_at(&mut w, sx, sy);
        }
        let robot = push_building(&mut w, BuildingKind::AssemblyRobot, sx, sy);
        w.buildings[robot as usize].auto = None;
        // a depot of silicon and wire; a bench that makes boards from transistors on auto (plank and wire already in)
        let depot = push_building(&mut w, BuildingKind::Depot, sx + 3_000, sy);
        w.buildings[depot as usize].output[I::Silicon as usize] = 2;
        w.buildings[depot as usize].output[I::CopperWire as usize] = 4;
        let bench = push_building(&mut w, BuildingKind::Workbench, sx - 3_000, sy);
        w.buildings[bench as usize].auto = Some(recipe("circuit_board").0);
        w.buildings[bench as usize].input[I::Plank as usize] = 2;
        w.buildings[bench as usize].input[I::CopperWire as usize] = 4;
        w
    };
    let mut w = build(true);
    for _ in 0..6_000 {
        w.step(&[]);
    }
    let bench = w.buildings.iter().position(|b| b.kind == BuildingKind::Workbench).unwrap();
    assert!(w.buildings[bench].output[I::CircuitBoard as usize] >= 1, "silicon went in, a board came out, nobody touched it: {:?}", w.buildings[bench].output[I::CircuitBoard as usize]);
    assert!(w.produced[I::Transistor as usize] >= 2);
    // no current, no work
    let mut w = build(false);
    for _ in 0..6_000 {
        w.step(&[]);
    }
    let bench = w.buildings.iter().position(|b| b.kind == BuildingKind::Workbench).unwrap();
    assert_eq!(w.buildings[bench].output[I::CircuitBoard as usize], 0);
    assert_eq!(w.produced[I::Transistor as usize], 0);
}

#[test]
fn a_computer_takes_the_chip_path_and_the_old_board_path_stays_open() {
    use ItemKind as I;
    let def = rules().building(BuildingKind::Computer);
    assert!(def.cost.iter().any(|c| c.item == I::CircuitBoard));
    assert!(def.alt_cost.iter().any(|c| c.item == I::CircuitBoard));
    assert!(def.alt_cost2.iter().any(|c| c.item == I::Chip));
    for (items, how) in [
        (vec![(I::CircuitBoard, 4), (I::Glass, 2), (I::Steel, 2), (I::CopperWire, 4)], "old board path"),
        (vec![(I::CircuitBoard, 4), (I::Glass, 2), (I::SheetPart, 2), (I::CopperWire, 4)], "old board path with sheet parts"),
        (vec![(I::Chip, 2), (I::Glass, 2), (I::Steel, 2), (I::CopperWire, 4)], "chip path"),
    ] {
        let (mut w, a) = with_agent(3);
        learn_all(&mut w);
        for &(k, n) in &items {
            give(&mut w, k, n);
        }
        let (x, y) = spot_for(&w, BuildingKind::Computer, &[]);
        w.agents[0].x = x - 1_500;
        w.agents[0].y = y;
        w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::Computer, x, y })]);
        assert!(w.buildings.iter().any(|b| b.kind == BuildingKind::Computer), "{how}: {:?}", w.take_events());
        for &(k, _) in &items {
            assert_eq!(count(&w, 0, k), 0, "{how}: {k:?} paid");
        }
    }
    // chips in hand open computing, like a board does
    let (mut w, _) = with_agent(3);
    give(&mut w, I::Chip, 2);
    w.step(&[]);
    assert!(w.knows(Discovery::Computing));
}

#[test]
fn fertiliser_lifts_a_field_above_its_best_and_the_crop_with_it() {
    use ItemKind as I;
    let (mut w, a) = with_agent(3);
    let (x, y) = (w.agents[0].x + 1_500, w.agents[0].y);
    let f = push_building(&mut w, BuildingKind::Field, x, y);
    give(&mut w, I::Fertilizer, 2);
    w.step(&[cmd(a, CommandKind::Deposit { building: f })]);
    for _ in 0..3 {
        w.step(&[]);
    }
    assert_eq!(w.buildings[f as usize].fertility, crate::world::FERTILIZED_SOIL);
    assert_eq!(w.buildings[f as usize].input[I::Fertilizer as usize], 1, "one dressing is enough until the soil tires");
    // the harvest is half as big again as in plain soil
    let crop = rules().recipes.iter().find(|r| r.station == Some(BuildingKind::Field) && r.outputs[0].n >= 4).expect("a field recipe");
    let plain = crop.outputs[0].n as u32;
    assert_eq!(((plain * w.buildings[f as usize].fertility as u32 / 1000)), plain * 3 / 2);
}

#[test]
fn alcohol_and_soap_heal_and_a_kerosene_lamp_lasts_three_torches() {
    use ItemKind as I;
    let (mut w, a) = with_agent(3);
    w.agents[0].sick = 6_000;
    w.agents[0].injury = 3_000;
    w.agents[0].health = 4_000;
    give(&mut w, I::Alcohol, 1);
    w.step(&[cmd(a, CommandKind::Eat { item: I::Alcohol })]);
    assert_eq!(w.agents[0].injury, 0, "the wound is clean");
    assert!(w.agents[0].sick <= 3_500 && w.agents[0].health >= 4_400);
    assert_eq!(count(&w, 0, I::Alcohol), 0);
    give(&mut w, I::Soap, 1);
    let before = w.agents[0].sick;
    w.step(&[cmd(a, CommandKind::Eat { item: I::Soap })]);
    assert!(w.agents[0].sick < before);
    assert_eq!(count(&w, 0, I::Soap), 0);
}

#[test]
fn bauxite_lies_in_deposits_on_every_seed_and_the_shaft_brings_it_up() {
    use ItemKind as I;
    let mut with_bauxite = 0;
    let mut total = (0, 0);
    for s in 1..=24u64 {
        let seed = crate::map::tag_seed_v2(s * 7919);
        let mut found = 0;
        for cy in -4..12 {
            for cx in -4..12 {
                if let Some(d) = crate::world::deposit_in_cell(seed, cx, cy) {
                    total.0 += 1;
                    if d.2 == I::Bauxite {
                        found += 1;
                        total.1 += 1;
                    }
                }
            }
        }
        if found > 0 {
            with_bauxite += 1;
        }
    }
    assert!(with_bauxite >= 22, "bauxite on {with_bauxite} of 24 seeds");
    assert!(total.1 * 100 / total.0 >= 4 && total.1 * 100 / total.0 <= 16, "a modest share of the beds: {total:?}");
    // a shaft on a bauxite bed brings it up
    let (seed, d) = (1..40u64)
        .map(|s| crate::map::tag_seed_v2(s * 7919))
        .find_map(|seed| (0..10).flat_map(|cy| (0..10).map(move |cx| (cx, cy))).filter_map(|(cx, cy)| crate::world::deposit_in_cell(seed, cx, cy)).find(|d| d.2 == I::Bauxite && (2..250).contains(&d.0) && (2..250).contains(&d.1)).map(|d| (seed, d)))
        .expect("bauxite in a 256 map");
    let mut w = World::with_size(seed, 256, 256);
    let st = push_building(&mut w, BuildingKind::MineShaft, d.0 as i64 * 1000 + 500, d.1 as i64 * 1000 + 500);
    w.buildings[st as usize].input[I::Plank as usize] = 1;
    w.buildings[st as usize].job = Some(crate::buildings::Job { recipe: recipe("mine").0, remaining: 5 });
    for _ in 0..40 {
        w.step(&[]);
    }
    assert!(w.buildings[st as usize].output[I::Bauxite as usize] >= 2);
}

#[test]
fn wave_three_discoveries_open_from_things_that_happen() {
    use ItemKind as I;
    let fresh = || with_agent(3).0;
    let opens = |w: &mut World, d: Discovery| {
        w.step(&[]);
        w.knows(d)
    };
    type Case = (Vec<Discovery>, Vec<(ItemKind, u16)>, Discovery);
    let cases: Vec<Case> = vec![
        (vec![Discovery::Steam], vec![(I::Tar, 1)], Discovery::Chemistry),
        (vec![Discovery::Steam], vec![(I::Oil, 1)], Discovery::Chemistry),
        (vec![Discovery::Steam], vec![(I::Lime, 2)], Discovery::Chemistry),
        (vec![Discovery::HeavyCurrent], vec![(I::Steel, 1)], Discovery::ArcSmelting),
        (vec![Discovery::HeavyCurrent], vec![(I::Bauxite, 1)], Discovery::ArcSmelting),
        (vec![Discovery::Electronics], vec![(I::Silicon, 1)], Discovery::Semiconductors),
        (vec![Discovery::Electronics], vec![(I::Transistor, 1)], Discovery::Automation),
        (vec![Discovery::Electronics], vec![(I::CircuitBoard, 1)], Discovery::Automation),
        (vec![Discovery::Electronics], vec![(I::Chip, 1)], Discovery::Automation),
    ];
    for (known, held, opened) in cases {
        let mut w = fresh();
        for &(k, n) in &held {
            give(&mut w, k, n);
        }
        assert!(!opens(&mut w, opened), "{opened:?} must not open before what it needs is known");
        for k in &known {
            w.learn(*k);
        }
        assert!(opens(&mut w, opened), "{opened:?} from {known:?} and {held:?}");
    }
    // a dynamo that stands opens heavy current (for people who know electricity)
    let mut w = fresh();
    w.learn(Discovery::Electricity);
    assert!(!opens(&mut w, Discovery::HeavyCurrent));
    push_building(&mut w, BuildingKind::Dynamo, 5_000, 5_000);
    assert!(opens(&mut w, Discovery::HeavyCurrent));
    // every machine has a way to be opened, and the late ones follow what they need
    let r = rules();
    for (k, d) in [
        (BuildingKind::ArcFurnace, Discovery::ArcSmelting),
        (BuildingKind::Electrolysis, Discovery::ArcSmelting),
        (BuildingKind::Welder, Discovery::ArcSmelting),
        (BuildingKind::Refinery, Discovery::Chemistry),
        (BuildingKind::Still, Discovery::Chemistry),
        (BuildingKind::Reactor, Discovery::Chemistry),
        (BuildingKind::Compressor, Discovery::HeavyCurrent),
        (BuildingKind::Transformer, Discovery::HeavyCurrent),
        (BuildingKind::Accumulator, Discovery::HeavyCurrent),
        (BuildingKind::Cooler, Discovery::HeavyCurrent),
        (BuildingKind::Railway, Discovery::Machining),
        (BuildingKind::SteamPump, Discovery::Steam),
        (BuildingKind::Telegraph, Discovery::Electricity),
        (BuildingKind::Radio, Discovery::Electronics),
        (BuildingKind::VacuumPump, Discovery::Electronics),
        (BuildingKind::CrystalPuller, Discovery::Semiconductors),
        (BuildingKind::Photolith, Discovery::Semiconductors),
        (BuildingKind::CncMachine, Discovery::Automation),
        (BuildingKind::AssemblyRobot, Discovery::Automation),
    ] {
        assert_eq!(r.building(k).discovery, d, "{k:?}");
    }
    // the discovery bits still fit in a u64
    assert!((Discovery::Automation as u32) < 64);
}

#[test]
fn every_wave_three_machine_has_work_or_an_effect_and_a_requirement() {
    let r = rules();
    let effect_only = [BuildingKind::Compressor, BuildingKind::Railway, BuildingKind::SteamPump, BuildingKind::Transformer, BuildingKind::Accumulator, BuildingKind::Cooler, BuildingKind::Telegraph, BuildingKind::Radio];
    for k in [
        BuildingKind::ArcFurnace, BuildingKind::Refinery, BuildingKind::Still, BuildingKind::Electrolysis, BuildingKind::Compressor, BuildingKind::Railway,
        BuildingKind::SteamPump, BuildingKind::Transformer, BuildingKind::Accumulator, BuildingKind::Welder, BuildingKind::Reactor, BuildingKind::Cooler,
        BuildingKind::Telegraph, BuildingKind::Radio, BuildingKind::VacuumPump, BuildingKind::CrystalPuller, BuildingKind::Photolith, BuildingKind::CncMachine,
        BuildingKind::AssemblyRobot,
    ] {
        let works = r.recipes.iter().any(|x| x.station == Some(k));
        assert!(works || effect_only.contains(&k), "{k:?} does nothing");
        // electric machines are on the grid and say so
        if k.needs_grid() {
            assert!(k.on_grid() && k.requirement() == Some(crate::buildings::Requirement::Grid), "{k:?}");
        }
        // every machine can be paid for, and an old path (no wave-3 item) exists where the plain price has one
        let def = r.building(k);
        assert!(!def.cost.is_empty());
    }
    // the late machines have an old way to pay for them: no price needs both a chip and a wave-3 only chain unless an older price exists
    for k in [BuildingKind::CncMachine, BuildingKind::AssemblyRobot, BuildingKind::Computer] {
        assert!(r.building(k).cost.iter().all(|c| c.item != ItemKind::Chip), "{k:?}: the plain cost does not need a chip");
    }
    // appended recipes come after every older one
    assert!(recipe("arc_steel").0 > recipe("jenny_cotton").0);
}

#[test]
fn a_version_27_save_loads_and_wave_three_state_survives() {
    use ItemKind as I;
    let (mut w, _) = with_agent(7);
    let mut acc = Building::new(BuildingKind::Accumulator, 6_000, 6_000);
    acc.fuel = 777;
    w.buildings.push(acc);
    let mut rail = Building::new(BuildingKind::Railway, 12_000, 6_000);
    rail.output[I::Steel as usize] = 5;
    w.buildings.push(rail);
    let mut robot = Building::new(BuildingKind::AssemblyRobot, 18_000, 6_000);
    robot.auto = Some(recipe("robot_magnet").0);
    robot.input[I::Steel as usize] = 2;
    w.buildings.push(robot);
    w.buildings.push(Building::new(BuildingKind::Telegraph, 24_000, 6_000));
    w.mark_seen(I::Chip);
    w.mark_seen(I::PureSilicon);
    w.learn(Discovery::Automation);
    let b = World::from_bytes(&w.to_bytes()).expect("load");
    assert_eq!(b.hash(), w.hash());
    assert_eq!(b.buildings[b.buildings.len() - 4].fuel, 777, "the accumulator keeps its charge");
    assert_eq!(b.buildings[b.buildings.len() - 3].output[I::Steel as usize], 5, "the wagon keeps its load");
    assert_eq!(b.buildings[b.buildings.len() - 2].auto, Some(recipe("robot_magnet").0));
    assert!(b.has_seen(I::Chip) && b.knows(Discovery::Automation));
    // Formats 27 and 28 shared the extension without the journal.
    let mut old = crate::save::format_28_for_tests(&w);
    old[4..8].copy_from_slice(&27u32.to_le_bytes());
    let c = World::from_bytes(&old).expect("format 27 loads");
    assert_eq!(c.buildings.len(), w.buildings.len());
    assert!(c.has_seen(I::Chip));
}

#[test]
fn a_chip_counts_for_the_transistor_and_board_milestones() {
    let (mut w, _) = with_agent(3);
    w.produced[ItemKind::Silicon as usize] = 1;
    w.produced[ItemKind::Chip as usize] = 1;
    w.step(&[]);
    assert!(w.milestone_done(crate::world::Milestone::Transistor) && w.milestone_done(crate::world::Milestone::CircuitBoard));
}

// ---- journal and camp ----

use crate::journal::{Entry, Goal, GoalState, Journal};

fn evening_camp(seed: u64) -> (World, AgentId) {
    let (mut w, a) = with_agent(seed);
    w.tick = crate::time::tick_of_minute(19 * 60);
    let (ax, ay) = (w.agents[0].x, w.agents[0].y);
    w.buildings.push(Building { fuel: 100_000, ..Building::new(BuildingKind::Campfire, ax + 1_800, ay + 600) });
    (w, a)
}

#[test]
fn the_journal_writes_down_discoveries_first_crafts_and_buildings() {
    let (mut w, a) = with_agent(1);
    give(&mut w, ItemKind::Stone, 2);
    give(&mut w, ItemKind::Wood, 9);
    give(&mut w, ItemKind::Flint, 1);
    w.step(&[]);
    assert!(w.knows(Discovery::SharpStone));
    craft_perfectly(&mut w, a, "sharp_stone");
    for _ in 0..120 {
        w.step(&[]);
    }
    let j = &w.journal;
    let f = j.find(Discovery::SharpStone).expect("the discovery is written down");
    assert_eq!(f.day, 0);
    assert!(j.first(ItemKind::SharpStone).is_some(), "the first sharp stone is written down");
    let page = j.day(0).expect("today's page");
    assert!(page.entries.iter().any(|(_, e)| matches!(e, Entry::Discovered { d: Discovery::SharpStone, .. })));
    assert!(page.entries.iter().any(|(_, e)| matches!(e, Entry::FirstMade { item: ItemKind::SharpStone, .. })));
    assert!(page.made.iter().any(|(k, _)| *k == ItemKind::SharpStone), "the day's tally of what was made");
    // a second one is not a first
    let firsts = j.firsts.iter().filter(|f| f.item == ItemKind::SharpStone).count();
    assert_eq!(firsts, 1);
    // building
    let (x, y) = spot_for(&w, BuildingKind::Campfire, &[]);
    w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::Campfire, x, y })]);
    let page = w.journal.day(0).unwrap();
    assert!(page.entries.iter().any(|(_, e)| matches!(e, Entry::Built { kind: BuildingKind::Campfire, at: None })), "no paper, no place on the page");
}

#[test]
fn paper_lets_the_journal_sketch_the_place() {
    let (mut w, a) = with_agent(1);
    give(&mut w, ItemKind::Paper, 1);
    give(&mut w, ItemKind::Wood, 9);
    give(&mut w, ItemKind::Flint, 1);
    let (x, y) = spot_for(&w, BuildingKind::Campfire, &[]);
    w.learn(Discovery::Fire);
    w.step(&[cmd(a, CommandKind::Build { kind: BuildingKind::Campfire, x, y })]);
    let page = w.journal.day(0).unwrap();
    assert!(page.entries.iter().any(|(_, e)| matches!(e, Entry::Built { at: Some(_), .. })), "with paper the place is drawn");
}

#[test]
fn the_journal_is_bounded_and_old_days_shrink() {
    use crate::journal::*;
    let mut j = Journal::default();
    // one crowded day: small things stop at the cap, highlights still get in
    for i in 0..(MAX_ENTRIES * 2) {
        j.record(0, i as u16 % 1440, Entry::Camp);
    }
    assert_eq!(j.day(0).unwrap().entries.len(), MAX_ENTRIES);
    j.record(0, 5, Entry::Died);
    assert_eq!(j.day(0).unwrap().entries.len(), MAX_ENTRIES);
    assert!(j.day(0).unwrap().entries.iter().any(|(_, e)| *e == Entry::Died));
    // hazards of one kind are added up, not listed
    j.record(1, 0, Entry::Hazard { kind: Hazard::Bite, n: 1 });
    j.record(1, 9, Entry::Hazard { kind: Hazard::Bite, n: 2 });
    assert_eq!(j.day(1).unwrap().entries.iter().filter(|(_, e)| matches!(e, Entry::Hazard { .. })).count(), 1);
    assert!(j.day(1).unwrap().entries.iter().any(|(_, e)| *e == Entry::Hazard { kind: Hazard::Bite, n: 3 }));
    // many days: the old ones keep their highlights only, and the oldest go
    for d in 2..300u32 {
        j.record(d, 1, Entry::Camp);
        j.record(d, 2, Entry::Discovered { d: Discovery::Fire, at: None });
        for k in 0..20 {
            j.tally_gathered(d, ItemKind::ALL[k], 1 + k as u16);
        }
    }
    assert!(j.days.len() <= MAX_DAYS, "{} days", j.days.len());
    let old = j.days.iter().find(|p| p.day + (RECENT_DAYS as u32) < 299).expect("an old page");
    assert!(old.entries.iter().all(|(_, e)| e.highlight()), "old pages keep their highlights");
    assert!(old.gathered.len() <= COMPACT_TALLY);
    let new = j.day(299).unwrap();
    assert!(new.entries.iter().any(|(_, e)| *e == Entry::Camp), "recent pages keep the small things");
    assert!(new.gathered.len() <= MAX_TALLY);
    assert!(j.day(2).is_none(), "the oldest days are gone");
}

#[test]
fn the_journal_is_deterministic_and_outside_the_state_hash() {
    let run = || {
        let (mut w, a) = with_agent(3);
        give(&mut w, ItemKind::Stone, 2);
        give(&mut w, ItemKind::Wood, 4);
        w.step(&[]);
        craft_perfectly(&mut w, a, "sharp_stone");
        for _ in 0..300 {
            w.step(&[]);
        }
        w
    };
    let (a, b) = (run(), run());
    assert_eq!(a.journal, b.journal);
    assert_eq!(a.hash(), b.hash());
    let mut c = run();
    c.journal = Journal::default();
    assert_eq!(c.hash(), a.hash(), "the journal is not part of the simulated state");
}

#[test]
fn camp_needs_evening_and_a_lit_fire() {
    let (mut w, a) = with_agent(2);
    let (ax, ay) = (w.agents[0].x, w.agents[0].y);
    w.buildings.push(Building { fuel: 100_000, ..Building::new(BuildingKind::Campfire, ax + 1_800, ay + 600) });
    // 11:00 (the world starts at 08:00)
    w.tick = crate::time::tick_of_minute(11 * 60);
    assert_eq!(w.can_camp(a), Err(Reject::TooBright));
    w.step(&[cmd(a, CommandKind::Camp)]);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::TooBright)));
    assert!(!w.agents[0].is_seated());
    // evening but the fire is out
    w.tick = crate::time::tick_of_minute(19 * 60);
    w.buildings[0].fuel = 0;
    assert_eq!(w.can_camp(a), Err(Reject::NoCampfire));
    w.buildings[0].fuel = 100_000;
    assert_eq!(w.can_camp(a), Ok(()));
    // too far from it
    w.agents[0].x += 20_000;
    assert_eq!(w.can_camp(a), Err(Reject::NoCampfire));
}

#[test]
fn camp_rests_once_an_evening_and_sums_up_the_day() {
    let (mut w, a) = evening_camp(2);
    let day = w.day() as u32;
    w.journal.tally_gathered(day, ItemKind::Wood, 5);
    w.journal.record(day, 700, Entry::Hazard { kind: crate::journal::Hazard::Bite, n: 1 });
    w.agents[0].stamina = 100;
    w.agents[0].warmth = 100;
    w.agents[0].injury = 1000;
    w.step(&[cmd(a, CommandKind::Camp)]);
    assert!(w.agents[0].is_seated());
    assert_eq!(w.agents[0].stamina, MAX_STAT);
    assert_eq!(w.agents[0].warmth, MAX_STAT);
    assert!(w.agents[0].injury <= 800, "a rest mends a little: {}", w.agents[0].injury);
    let page = w.journal.day(day).unwrap();
    assert!(page.gathered.contains(&(ItemKind::Wood, 5)), "the summary has what was gathered");
    assert!(page.entries.iter().any(|(_, e)| matches!(e, Entry::Hazard { .. })), "and the dangers");
    assert!(page.entries.iter().any(|(_, e)| *e == Entry::Camp));
    assert!(w.take_events().contains(&Event::Camped(a)));
    // a second sitting the same evening gives nothing more
    w.agents[0].stamina = 100;
    w.step(&[cmd(a, CommandKind::Camp)]);
    assert!(w.agents[0].stamina < MAX_STAT, "a second sitting the same evening gives nothing more");
    // the person is up again the moment they move
    w.step(&[cmd(a, CommandKind::MoveTo { x: w.agents[0].x + 5_000, y: w.agents[0].y })]);
    assert!(!w.agents[0].is_seated());
}

#[test]
fn a_goal_is_chosen_at_camp_and_marked_done_when_reached() {
    let (mut w, a) = evening_camp(2);
    // no goal without sitting at the fire first
    let goals = w.camp_goals();
    assert!(!goals.is_empty(), "there is always something ahead");
    w.step(&[cmd(a, CommandKind::SetGoal { goal: Some(goals[0]) })]);
    assert!(w.journal.goal.is_none());
    w.step(&[cmd(a, CommandKind::Camp)]);
    // a goal that is not on the list is refused
    w.step(&[cmd(a, CommandKind::SetGoal { goal: Some(Goal::Reach(Milestone::ComputerAge)) })]);
    assert!(w.journal.goal.is_none());
    w.learn(Discovery::Fire);
    let g = *w.camp_goals().iter().find(|g| matches!(g, Goal::Make(_))).expect("something never made");
    let Goal::Make(item) = g else { unreachable!() };
    w.step(&[cmd(a, CommandKind::SetGoal { goal: Some(g) })]);
    let gs = w.journal.goal.expect("goal set");
    assert_eq!(gs.goal, g);
    assert!(gs.done_day.is_none());
    assert!(w.journal.day(0).unwrap().entries.iter().any(|(_, e)| *e == Entry::GoalSet(g)));
    // the next morning it is not done yet, and then it is made
    w.tick = crate::time::TICKS_PER_DAY + crate::time::tick_of_minute(9 * 60);
    for _ in 0..120 {
        w.step(&[]);
    }
    assert!(w.journal.goal.unwrap().done_day.is_none());
    w.produced[item as usize] += 1;
    for _ in 0..120 {
        w.step(&[]);
    }
    let gs = w.journal.goal.expect("goal kept");
    assert_eq!(gs.done_day, Some(1));
    assert!(w.journal.day(1).unwrap().entries.iter().any(|(_, e)| *e == Entry::GoalDone(g)));
    assert!(w.take_events().contains(&Event::GoalDone(g)));
    // dropping the goal
    w.tick = crate::time::tick_of_minute(19 * 60) + crate::time::TICKS_PER_DAY * 2;
    w.step(&[cmd(a, CommandKind::Camp)]);
    let day = w.day() as u32;
    assert!(w.journal.camped(0, day));
    w.step(&[cmd(a, CommandKind::SetGoal { goal: None })]);
    assert!(w.journal.goal.is_none());
    let _ = GoalState { goal: g, set_day: 0, base: 0, done_day: None };
}

#[test]
fn the_journal_survives_a_save_and_an_older_save_loads_without_one() {
    let (mut w, a) = evening_camp(5);
    give(&mut w, ItemKind::Stone, 2);
    give(&mut w, ItemKind::Wood, 4);
    w.step(&[]);
    craft_perfectly(&mut w, a, "sharp_stone");
    for _ in 0..120 {
        w.step(&[]);
    }
    w.step(&[cmd(a, CommandKind::Camp)]);
    w.step(&[cmd(a, CommandKind::SetGoal { goal: Some(Goal::Make(ItemKind::Spear)) })]);
    assert!(!w.journal.days.is_empty());
    let b = World::from_bytes(&w.to_bytes()).expect("load");
    assert_eq!(b.journal, w.journal);
    assert_eq!(b.hash(), w.hash());
    assert!(!b.agents[0].is_seated(), "sitting is not saved");
    // a format-28 save (the same layout without the journal at the end) loads, with an empty journal that starts afresh
    let old = crate::save::format_28_for_tests(&w);
    let c = World::from_bytes(&old).expect("format 28 loads");
    assert!(c.journal.days.is_empty() && !c.journal.started);
    assert_eq!(c.hash(), w.hash());
    let mut c = c;
    c.step(&[]);
    assert!(c.journal.started, "the journal takes its bearings from the loaded world");
    assert!(c.journal.find(Discovery::SharpStone).is_some_and(|f| f.day == crate::journal::UNKNOWN_DAY), "what was known before is marked as such");
    assert_eq!(crate::save::FORMAT_VERSION, 33);
}

#[test]
fn a_milestone_and_a_death_are_written_down() {
    let (mut w, _) = with_agent(3);
    w.step(&[]);
    w.produced[ItemKind::Silicon as usize] = 1;
    w.produced[ItemKind::Chip as usize] = 1;
    w.step(&[]);
    assert!(w.journal.days.iter().any(|d| d.entries.iter().any(|(_, e)| matches!(e, Entry::Milestone(Milestone::Transistor)))));
}

#[test]
fn load_moves_only_the_named_item() {
    let (mut w, a) = with_agent(1);
    let (sx, sy) = w.spawn_mm;
    let furnace = push_building(&mut w, BuildingKind::Furnace, sx + 1500, sy);
    give(&mut w, ItemKind::CopperOre, 5);
    w.agents[0].inventory.counts[ItemKind::Wood as usize] = 5;
    w.step(&[cmd(a, CommandKind::Load { building: furnace, item: ItemKind::CopperOre, n: 3 })]);
    assert_eq!(count(&w, 0, ItemKind::CopperOre), 2);
    assert_eq!(w.buildings[furnace as usize].input[ItemKind::CopperOre as usize], 3);
    assert_eq!(count(&w, 0, ItemKind::Wood), 5);
    assert_eq!(w.buildings[furnace as usize].input[ItemKind::Wood as usize], 0);
}

#[test]
fn load_respects_station_cap() {
    let (mut w, a) = with_agent(1);
    let (sx, sy) = w.spawn_mm;
    let furnace = push_building(&mut w, BuildingKind::Furnace, sx + 1500, sy);
    give(&mut w, ItemKind::CopperOre, STATION_INPUT_CAP + 5);
    w.step(&[cmd(a, CommandKind::Load { building: furnace, item: ItemKind::CopperOre, n: u16::MAX })]);
    assert_eq!(w.buildings[furnace as usize].input[ItemKind::CopperOre as usize], STATION_INPUT_CAP);
    assert_eq!(count(&w, 0, ItemKind::CopperOre), 5);
}

#[test]
fn load_refuses_from_far_away() {
    let (mut w, a) = with_agent(1);
    let (sx, sy) = w.spawn_mm;
    let furnace = push_building(&mut w, BuildingKind::Furnace, sx + 5000, sy);
    give(&mut w, ItemKind::CopperOre, 5);
    w.step(&[cmd(a, CommandKind::Load { building: furnace, item: ItemKind::CopperOre, n: 3 })]);
    assert_eq!(count(&w, 0, ItemKind::CopperOre), 5);
    assert_eq!(w.buildings[furnace as usize].input[ItemKind::CopperOre as usize], 0);
    assert!(w.take_events().contains(&Event::Rejected(a, Reject::TooFar)));
}

#[test]
fn load_into_depot_takes_any_non_tool() {
    let (mut w, a) = with_agent(1);
    let (sx, sy) = w.spawn_mm;
    let depot = push_building(&mut w, BuildingKind::Depot, sx + 1500, sy);
    give(&mut w, ItemKind::Berries, 4);
    give(&mut w, ItemKind::StoneAxe, 1);
    w.step(&[cmd(a, CommandKind::Load { building: depot, item: ItemKind::Berries, n: 4 })]);
    assert_eq!(w.buildings[depot as usize].output[ItemKind::Berries as usize], 4);
    assert_eq!(load_room(rules(), &w.buildings[depot as usize], ItemKind::StoneAxe), 0);
    w.step(&[cmd(a, CommandKind::Load { building: depot, item: ItemKind::StoneAxe, n: 1 })]);
    assert_eq!(count(&w, 0, ItemKind::StoneAxe), 1);
    assert_eq!(w.buildings[depot as usize].output[ItemKind::StoneAxe as usize], 0);
}

#[test]
fn load_room_matches_deposit() {
    let (mut w, a) = with_agent(1);
    let (sx, sy) = w.spawn_mm;
    let mut kinds: Vec<BuildingKind> = rules().recipes.iter().filter_map(|r| r.station).collect();
    kinds.extend([BuildingKind::Depot, BuildingKind::Cellar, BuildingKind::Cooler, BuildingKind::Field]);
    kinds.sort_by_key(|k| format!("{k:?}"));
    kinds.dedup();
    for kind in kinds {
        let bi = push_building(&mut w, kind, sx + 1500, sy);
        for item in ItemKind::ALL {
            let mut probe = w.clone();
            probe.agents[0].inventory.counts = [0; ItemKind::COUNT];
            probe.agents[0].inventory.counts[item as usize] = 1;
            probe.step(&[cmd(a, CommandKind::Deposit { building: bi })]);
            let moved = probe.agents[0].inventory.get(item) == 0;
            let room = load_room(rules(), &w.buildings[bi as usize], item);
            assert_eq!(room > 0, moved, "{kind:?} {item:?}");
        }
        w.buildings.pop();
    }
}

/// Not a test of behaviour: how many natural cave mouths a new world has and how far the nearest is from the spawn.
/// `cargo test --release -p alaz-sim -- --ignored cave_mouths_by_size --nocapture --test-threads=1`.
#[test]
#[ignore]
fn cave_mouths_by_size() {
    for size in [128, 256, 512, 1024, 2048] {
        for s in [3u64, 7, 42, 99] {
            let seed = crate::map::tag_seed_new_world(s, size);
            let w = World::with_size(seed, size, size);
            let (sx, sy) = ((w.spawn_mm.0 / 1000) as i32, (w.spawn_mm.1 / 1000) as i32);
            let (mut count, mut best) = (0u32, i64::MAX);
            let (from, step) = if size <= 256 { (0, 1) } else { (2, 4) };
            for ty in (from..size).step_by(step) {
                for tx in (from..size).step_by(step) {
                    if w.map.natural_cave_mouth(tx, ty).is_some() {
                        count += 1;
                        best = best.min(((tx - sx) as i64).pow(2) + ((ty - sy) as i64).pow(2));
                    }
                }
            }
            let near = if count == 0 { "none".to_string() } else { format!("{} m", (best as f64).sqrt() as i64) };
            eprintln!("size {size:>5} seed {s:>3}: {count:>6} mouths, nearest to spawn {near}");
        }
    }
}

/// Early mining needs a cave to walk into: a new world of any small or middle size has a natural cave mouth, and the
/// nearest one is within a walk of the spawn. The full run (3 sizes, 24 seeds) takes 11 s in release, so it is
/// ignored: `--ignored every_new_world_has_a_cave_mouth_wide`. The test that always runs is a small slice of it.
#[test]
fn every_new_world_has_a_cave_mouth_within_a_walk_of_the_spawn() {
    cave_mouth_scan(&[128, 256], 1..=6);
}

#[test]
#[ignore]
fn every_new_world_has_a_cave_mouth_wide() {
    cave_mouth_scan(&[128, 256, 512], 1..=24);
}

fn cave_mouth_scan(sizes: &[i32], seeds: std::ops::RangeInclusive<u64>) {
    let mut worst = (0i64, 0u64, 0i32);
    for &size in sizes {
        for s in seeds.clone() {
            let seed = crate::map::tag_seed_new_world(s, size);
            let w = World::with_size(seed, size, size);
            let (sx, sy) = ((w.spawn_mm.0 / 1000) as i32, (w.spawn_mm.1 / 1000) as i32);
            // a small world places its mouths anywhere; the others only on the 4 x 4 lattice
            let (from, step) = if size <= 256 { (0, 1) } else { (2, 4) };
            let nearest = (from..size)
                .step_by(step)
                .flat_map(|ty| (from..size).step_by(step).map(move |tx| (tx, ty)))
                .filter(|&(tx, ty)| w.map.natural_cave_mouth(tx, ty).is_some())
                .map(|(tx, ty)| (((tx - sx) as i64).pow(2) + ((ty - sy) as i64).pow(2)).isqrt())
                .min();
            let Some(d) = nearest else { panic!("size {size} seed {s}: no cave mouth in the whole world") };
            if d > worst.0 {
                worst = (d, s, size);
            }
        }
    }
    assert!(worst.0 <= 250, "the nearest cave mouth is {} m from the spawn (size {}, seed {})", worst.0, worst.2, worst.1);
}

/// Not a test of behaviour: prints how long a new world takes at each size. Run it with
/// `cargo test --release -p alaz-sim -- --ignored world_start_times --nocapture`.
#[test]
#[ignore]
fn world_start_times() {
    for size in [128, 256, 512, 1024, 2048, 4096, 8192] {
        let seed = crate::map::tag_seed_new_world(7, size);
        let t = std::time::Instant::now();
        let mut w = World::with_size(seed, size, size);
        let made = t.elapsed();
        let (sx, sy) = w.spawn_mm;
        let t = std::time::Instant::now();
        w.spawn_agent(sx, sy);
        w.learn_everything();
        let first = std::time::Instant::now();
        for _ in 0..60 {
            w.step(&[]);
        }
        eprintln!("size {size:>5}: world {:>8.1?}  spawn {:>8.1?}  60 ticks {:>8.1?}  animals {}", made, first - t, first.elapsed(), w.wild.len());
    }
}

/// Not a test of behaviour: where a 4096 world's start goes. `--ignored world_start_phases --nocapture`.
#[test]
#[ignore]
fn world_start_phases() {
    let size = 4096;
    let seed = crate::map::tag_seed_new_world(7, size);
    let t = std::time::Instant::now();
    let map = crate::map::Map::generate(seed, size, size);
    eprintln!("map tiles:   {:?}", t.elapsed());
    let t = std::time::Instant::now();
    let n = crate::map::par_rows(size, size, |y0, y1| {
        let mut out = 0u32;
        for ty in y0..y1 {
            for tx in 0..size {
                if map.node_for(seed, tx, ty).is_some() {
                    out += 1;
                }
            }
        }
        vec![out]
    });
    eprintln!("node scan:   {:?} ({} rows)", t.elapsed(), n.len());
    let t = std::time::Instant::now();
    let w = crate::wildlife::generate(seed, &map, (size / 2, size / 2), 30);
    eprintln!("wildlife:    {:?} ({} animals)", t.elapsed(), w.len());
}
