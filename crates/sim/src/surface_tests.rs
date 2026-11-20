//! Surfaces and elements: wet ground, mud, ice and fire.

use crate::command::{AgentId, Command, CommandKind};
use crate::map::{ItemKind, Terrain};
use crate::surface::{Burn, Ground, Surface, MAX_FIRES};
use crate::time::{self, Weather, TICKS_PER_DAY};
use crate::world::{Event, World};

const SEED: u64 = 42;
const DAYS: u64 = time::NEW_SEASON_DAYS;

fn world(seed: u64) -> World {
    World::new(seed)
}

/// First tick (stepping by `by`) from `from` for which `ok` holds.
fn find_tick(seed: u64, from: u64, by: u64, ok: impl Fn(&Ground) -> bool) -> u64 {
    (0..20_000u64).map(|i| from + i * by).find(|&t| ok(&Ground::at(seed, t, DAYS))).expect("a fitting moment")
}

fn grass_tiles(w: &World) -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    for ty in 4..w.map.h - 4 {
        for tx in 4..w.map.w - 4 {
            if w.map.terrain(tx, ty) == Some(Terrain::Grass) {
                v.push((tx, ty));
            }
        }
    }
    v
}

fn with_agent(w: &mut World) -> AgentId {
    let (sx, sy) = w.spawn_mm;
    w.spawn_agent(sx, sy)
}

#[test]
fn rain_wets_the_ground_and_the_sun_dries_it() {
    let rain = (TICKS_PER_DAY..TICKS_PER_DAY * 60).step_by((TICKS_PER_DAY / 6) as usize).find(|&t| time::weather(SEED, t, DAYS) == Weather::Rain).expect("rain");
    let block = TICKS_PER_DAY / 6;
    let (start, end) = (time::ground_wet(SEED, rain, DAYS), time::ground_wet(SEED, rain + block - 1, DAYS));
    assert!(end > start && end >= 390, "rain soaks the ground: {start} -> {end}");
    // after the rain: drying, block by block, never wetter, down to dry once the sky has stayed clear
    let mut last = end;
    let mut dry_at = None;
    for k in 1..40u64 {
        let t = rain + k * block + block - 1;
        let w = time::ground_wet(SEED, t, DAYS);
        if time::weather(SEED, t, DAYS) == Weather::Clear && (1..=k).all(|j| time::weather(SEED, rain + j * block, DAYS) == Weather::Clear) {
            assert!(w <= last, "drying only dries: {last} -> {w}");
            last = w;
            if w < 40 {
                dry_at = Some(k);
                break;
            }
        } else {
            break;
        }
    }
    // (a clear stretch long enough may not follow every rain; the bound is checked where there is one)
    if let Some(k) = dry_at {
        assert!(k <= 9, "the ground dries within a day and a half: {k} blocks");
    }
    assert_eq!(time::ground_wet(SEED, 100, DAYS), 0, "the first day is dry");
}

/// A straight run of tiles that are all `want` at this moment.
fn strip(w: &World, g: &Ground, want: Surface, len: i32) -> (i32, i32) {
    for (tx, ty) in grass_tiles(w) {
        if (0..len).all(|i| w.map.terrain(tx + i, ty) == Some(Terrain::Grass) && g.weather_surface(&w.map, tx + i, ty) == want) {
            return (tx, ty);
        }
    }
    panic!("no strip of {want:?}");
}

fn walked(w: &mut World, a: AgentId, from: (i32, i32), boots: bool) -> i64 {
    let (x0, y0) = (from.0 as i64 * 1000 + 500, from.1 as i64 * 1000 + 500);
    let ag = &mut w.agents[a.0 as usize];
    ag.x = x0;
    ag.y = y0;
    ag.inventory.counts[ItemKind::HideBoots as usize] = boots as u16;
    w.step(&[Command { agent: a, kind: CommandKind::Walk { x: x0 + 9000, y: y0 } }]);
    for _ in 0..100 {
        w.step(&[]);
    }
    w.agents[a.0 as usize].x - x0
}

#[test]
fn mud_slows_the_going_and_boots_help() {
    let mut w = world(SEED);
    let a = with_agent(&mut w);
    let wet_t = find_tick(SEED, TICKS_PER_DAY, 600, |g| g.wet >= 950 && g.sky == Weather::Rain && !g.freezing());
    let dry_t = 600;
    let g = Ground::at(SEED, wet_t, DAYS);
    let at = strip(&w, &g, Surface::Mud, 11);
    assert_eq!(Ground::at(SEED, dry_t, DAYS).weather_surface(&w.map, at.0, at.1), Surface::Dry);
    w.tick = dry_t;
    let dry = walked(&mut w, a, at, false);
    // (the walk takes the same time from the same place, in the wet and the dry)
    let mut w = world(SEED);
    let a = with_agent(&mut w);
    w.tick = wet_t;
    let mud = walked(&mut w, a, at, false);
    let mut w = world(SEED);
    let a = with_agent(&mut w);
    w.tick = wet_t;
    let booted = walked(&mut w, a, at, true);
    assert!(mud < dry * 85 / 100, "mud slows: {mud} vs {dry}");
    assert!(booted > mud, "soles with grip keep more of the pace: {booted} vs {mud}");
}

#[test]
fn puddles_stand_in_flat_hollows_and_freeze_to_ice() {
    let w = world(SEED);
    let wet_t = find_tick(SEED, TICKS_PER_DAY, 300, |g| g.wet >= 990 && !g.freezing() && g.sky == Weather::Rain);
    let g = Ground::at(SEED, wet_t, DAYS);
    let tiles = grass_tiles(&w);
    let count = |g: &Ground, s: Surface| tiles.iter().filter(|&&(x, y)| g.weather_surface(&w.map, x, y) == s).count();
    let (mud, puddle) = (count(&g, Surface::Mud), count(&g, Surface::Puddle));
    assert!(mud > 0 && puddle > 0 && puddle < mud, "mud {mud}, puddles {puddle}");
    let dry = Ground::at(SEED, 10, DAYS);
    assert_eq!(count(&dry, Surface::Mud) + count(&dry, Surface::Puddle) + count(&dry, Surface::Ice), 0);
    // the same wet in the cold: ice, no mud; thawing brings the puddles back
    let cold_t = find_tick(SEED, TICKS_PER_DAY, 300, |g| g.wet >= 700 && g.freezing());
    let cold = Ground::at(SEED, cold_t, DAYS);
    let (ice, mud_c) = (count(&cold, Surface::Ice), count(&cold, Surface::Mud));
    assert!(ice > 0 && mud_c == 0, "ice {ice}, mud {mud_c}");
    let thaw = Ground { ambient_x10: 80, ..cold };
    assert_eq!(count(&thaw, Surface::Ice), 0, "ice melts when it warms");
    assert!(count(&thaw, Surface::Mud) + count(&thaw, Surface::Puddle) > 0);
}

#[test]
fn runners_slip_on_ice_and_grip_helps() {
    let w0 = world(SEED);
    let t0 = long_frost();
    let g = Ground::at(SEED, t0, DAYS);
    let ice = grass_tiles(&w0).into_iter().find(|&(x, y)| g.weather_surface(&w0.map, x, y) == Surface::Ice).expect("ice");
    let slips = |boots: bool| {
        let mut w = world(SEED);
        let a = with_agent(&mut w);
        w.tick = t0;
        let mut n = 0;
        for _ in 0..18_000 {
            let ag = &mut w.agents[a.0 as usize];
            ag.x = ice.0 as i64 * 1000 + 500;
            ag.y = ice.1 as i64 * 1000 + 500;
            ag.sprint = true;
            ag.stamina = 10_000;
            ag.health = 10_000;
            ag.inventory.counts[ItemKind::HideBoots as usize] = boots as u16;
            ag.task = crate::world::Task::Walk { x: ag.x + 50_000, y: ag.y };
            w.run_surface();
            w.tick += 1;
            n += w.events.drain(..).filter(|e| matches!(e, Event::Slipped(_))).count();
        }
        n
    };
    let (bare, booted) = (slips(false), slips(true));
    eprintln!("slips bare {bare} booted {booted}");
    assert!(bare > 20, "runners slip: {bare}");
    assert!(booted < bare, "soles with grip slip less: {booted} vs {bare}");
}

fn long_frost() -> u64 {
    (TICKS_PER_DAY..TICKS_PER_DAY * 400)
        .step_by(300)
        .find(|&t| [0, 9000, 18000].iter().all(|d| {
            let g = Ground::at(SEED, t + d, DAYS);
            g.wet >= 900 && g.freezing()
        }))
        .expect("a long frost")
}

/// Burn a patch for a while, with only the fire step running; returns (peak burning, tiles that ever burned).
fn burn_for(w: &mut World, at: (i32, i32), ticks: u64) -> (usize, usize) {
    assert!(w.ignite(at.0, at.1));
    let mut ever = std::collections::BTreeSet::new();
    let mut peak = 0;
    for _ in 0..ticks {
        w.run_surface();
        w.tick += 1;
        let n = w.burning().count();
        peak = peak.max(n);
        ever.extend(w.burning());
    }
    (peak, ever.len())
}

#[test]
fn fire_spreads_in_dry_grass_but_only_so_far() {
    let mut biggest = 0;
    let mut spread_runs = 0;
    for seed in [1u64, 2, 3, 5, 8, 13] {
        let w0 = world(seed);
        let tiles = grass_tiles(&w0);
        for k in 0..6 {
            let mut w = world(seed);
            w.tick = 600 + k * 7000;
            let at = tiles[(k as usize * 7919) % tiles.len()];
            // dry, warm, clear at this moment
            if w.ground().fuel(&w.map, at.0, at.1) == 0 {
                continue;
            }
            let (peak, ever) = burn_for(&mut w, at, 4000);
            assert!(peak <= MAX_FIRES);
            assert_eq!(w.burning().count(), 0, "it burns out");
            biggest = biggest.max(ever);
            spread_runs += (ever > 3) as i32;
        }
    }
    eprintln!("fire patches: biggest {biggest}, spreading runs {spread_runs}");
    assert!(spread_runs > 0, "fire does spread in dry grass");
    assert!(biggest < 1500, "but a single fire stays a patch: {biggest} tiles");
}

/// A campfire on a windy dry day throws embers now and then, and not on a calm or a wet day.
#[test]
fn a_campfire_throws_embers_in_a_gale_only() {
    let mut w = world(5);
    let tiles = grass_tiles(&w);
    // five lit fires on open grass (the spark only needs them lit)
    for k in 0..5usize {
        let (tx, ty) = tiles[k * 1301 % tiles.len()];
        let mut b = crate::buildings::Building::new(crate::buildings::BuildingKind::Campfire, tx as i64 * 1000 + 500, ty as i64 * 1000 + 500);
        b.fuel = 30_000;
        w.buildings.push(b);
    }
    let (mut gale, mut calm) = (0, 0);
    for t in (0..TICKS_PER_DAY * 3).step_by(60) {
        let g = Ground::at(5, t, DAYS);
        if g.sky != Weather::Clear || g.wet >= 300 {
            continue;
        }
        let before = w.burn.len();
        w.tick = t;
        w.run_sparks();
        if w.burn.len() > before {
            if time::wind(5, t, DAYS).1 > 500 { gale += 1 } else { calm += 1 }
        }
        // keep the fires from piling up (and from spreading): clear them each time
        w.burn.clear();
    }
    assert_eq!(calm, 0, "no embers in calm air");
    assert!(gale > 0, "a gale throws embers over three days");
}

/// In the strongest wind the dry weather brings, a fire still stays a patch.
#[test]
fn a_gale_pushes_a_fire_but_cannot_run_it_away() {
    let w0 = world(5);
    let tiles = grass_tiles(&w0);
    // the windiest dry, warm moments of the first days
    let mut moments: Vec<(i32, u64)> = (0..TICKS_PER_DAY * 3).step_by(300).filter(|&t| {
        let g = Ground::at(5, t, DAYS);
        g.sky == Weather::Clear && g.wet < 300 && g.ambient_x10 >= 100
    }).map(|t| (time::wind(5, t, DAYS).1, t)).collect();
    moments.sort();
    let (strength, t0) = *moments.last().expect("a windy dry moment");
    assert!(strength > 400, "{strength}");
    let mut biggest = 0;
    for k in 0..12usize {
        let mut w = world(5);
        w.tick = t0 - t0 % 30;
        let at = tiles[k * 1999 % tiles.len()];
        if w.ground().fuel(&w.map, at.0, at.1) == 0 {
            continue;
        }
        let (peak, ever) = burn_for(&mut w, at, 3600);
        assert!(peak <= MAX_FIRES);
        biggest = biggest.max(ever);
    }
    eprintln!("gale {strength}: biggest {biggest}");
    assert!(biggest < 600, "{biggest}");
}

#[test]
fn fire_does_not_start_or_spread_on_wet_ground_and_rain_puts_it_out() {
    let mut w = world(SEED);
    let wet_t = find_tick(SEED, TICKS_PER_DAY, 300, |g| g.wet >= 700 && g.sky == Weather::Rain && !g.freezing());
    w.tick = wet_t;
    let at = grass_tiles(&w)[100];
    assert!(!w.ignite(at.0, at.1), "wet grass does not light");
    // a fire already burning when the rain comes goes out, and leaves ash
    for i in 0..6 {
        w.burn.insert((at.0 + i, at.1), Burn { fire: true, born: wet_t, until: wet_t + 100_000 });
    }
    for _ in 0..600 {
        w.run_surface();
        w.tick += 1;
    }
    assert_eq!(w.burning().count(), 0, "the rain put it out");
    assert!(w.burn.values().all(|b| !b.fire));
    assert_eq!(w.surface_at(at.0, at.1), Surface::Ash);
    // ash does not burn again
    let dry = world(SEED);
    let mut dry = dry;
    dry.tick = 100;
    dry.burn.insert(at, Burn { fire: false, born: 0, until: 90_000 });
    assert!(!dry.ignite(at.0, at.1));
    // and it clears after a day
    dry.tick = 91_000;
    dry.burn.insert((at.0 + 1, at.1), Burn { fire: false, born: 0, until: 90_000 });
    dry.tick -= dry.tick % 30;
    dry.run_surface();
    assert!(dry.burn.is_empty() || dry.burn.values().all(|b| b.until > dry.tick), "old ash clears");
}

#[test]
fn bare_ground_and_buildings_do_not_burn() {
    let mut w = world(SEED);
    w.tick = 600;
    let sand = (0..w.map.w).flat_map(|x| (0..w.map.h).map(move |y| (x, y))).find(|&(x, y)| w.map.terrain(x, y) == Some(Terrain::Sand)).expect("sand");
    assert!(!w.ignite(sand.0, sand.1));
    let at = grass_tiles(&w)[300];
    w.buildings.push(crate::buildings::Building::new(crate::buildings::BuildingKind::Campfire, at.0 as i64 * 1000 + 500, at.1 as i64 * 1000 + 500));
    w.obstacles.block_footprint(&w.map, at.0 as i64 * 1000 + 500, at.1 as i64 * 1000 + 500, 600);
    w.obstacles.built_for = w.buildings.len();
    assert!(!w.ignite(at.0, at.1), "a building's footprint is bare");
}

#[test]
fn flames_hurt_and_fire_gear_helps() {
    let hurt = |gear: bool| {
        let mut w = world(SEED);
        let a = with_agent(&mut w);
        w.tick = 600;
        let (tx, ty) = ((w.agents[0].x / 1000) as i32, (w.agents[0].y / 1000) as i32);
        w.burn.insert((tx, ty), Burn { fire: true, born: w.tick, until: w.tick + 1000 });
        w.agents[a.0 as usize].inventory.counts[ItemKind::HideCloak as usize] = gear as u16;
        let h = w.agents[0].health;
        w.run_surface();
        h - w.agents[0].health
    };
    let (bare, cloaked) = (hurt(false), hurt(true));
    assert!(bare > 0 && cloaked < bare, "{cloaked} vs {bare}");
}

#[test]
fn surface_fire_and_ice_pass_over_underground_agents() {
    let mut w = world(SEED);
    let t = long_frost();
    let ice = strip(&w, &Ground::at(SEED, t, DAYS), Surface::Ice, 1);
    let (x, y) = (i64::from(ice.0) * 1_000 + 500, i64::from(ice.1) * 1_000 + 500);
    let surface = w.spawn_agent(x, y);
    let underground = w.spawn_agent(x, y);
    w.agents[1].cave = Some(crate::CaveSelf {
        node: 0, level: 90, x, y, light: 60, full: 60, dark: 0, cool: 0, leaving: false,
    });
    w.tick = 600;
    w.burn.insert(ice, Burn { fire: true, born: w.tick, until: w.tick + 1_000 });
    let health = w.agents[1].health;
    w.run_surface();
    assert!(w.agents[0].health < health);
    assert_eq!(w.agents[1].health, health);

    w.burn.clear();
    let walk = crate::world::Task::Walk { x: x + 2_000, y };
    let mut surface_slipped = false;
    for tick in (t..t + 6_000).step_by(30) {
        w.tick = tick;
        for a in &mut w.agents {
            a.sprint = true;
            a.stamina = 10_000;
            a.task = walk;
        }
        w.run_surface();
        surface_slipped |= w.events.iter().any(|e| matches!(e, Event::Slipped(id) if *id == surface));
        assert!(!w.events.iter().any(|e| matches!(e, Event::Slipped(id) if *id == underground)));
        assert_eq!(w.agents[1].task, walk);
        w.events.clear();
    }
    assert!(surface_slipped);
}

#[test]
fn wet_ground_makes_a_fire_hard_to_light() {
    use crate::buildings::BuildingKind::Campfire;
    let wet_t = find_tick(SEED, TICKS_PER_DAY, 300, |g| g.wet >= 990 && g.sky == Weather::Rain && !g.freezing());
    let g = Ground::at(SEED, wet_t, DAYS);
    let w0 = world(SEED);
    // open ground: no tree or stone close by, so the fire can go up there
    let free = |w: &World, x: i64, y: i64| w.can_place(Campfire, x, y).is_ok();
    let find = |want: Surface| {
        grass_tiles(&w0).into_iter().find(|&(tx, ty)| {
            g.weather_surface(&w0.map, tx, ty) == want && Ground::at(SEED, 600, DAYS).weather_surface(&w0.map, tx, ty) == Surface::Dry && {
                let mut w = world(SEED);
                w.tick = 600;
                free(&w, tx as i64 * 1000 + 500, ty as i64 * 1000 + 500)
            }
        })
    };
    let lit = |t: u64, tile: (i32, i32)| {
        let mut w = world(SEED);
        w.known = u64::MAX;
        w.settings.difficulty = crate::world::Difficulty::Creative;
        let a = with_agent(&mut w);
        w.tick = t;
        let (x, y) = (tile.0 as i64 * 1000 + 500, tile.1 as i64 * 1000 + 500);
        w.agents[a.0 as usize].x = x;
        w.agents[a.0 as usize].y = y;
        w.step(&[Command { agent: a, kind: CommandKind::Build { kind: Campfire, x, y } }]);
        w.buildings.last().filter(|b| b.kind == Campfire).map(|b| b.fuel)
    };
    let mud = find(Surface::Mud).expect("mud place");
    let (dry_fuel, mud_fuel) = (lit(600, mud).expect("lit on dry ground"), lit(wet_t, mud).expect("lit in mud"));
    assert!(mud_fuel < dry_fuel, "wet earth starts the fire with less: {mud_fuel} vs {dry_fuel}");
    // not at all in a puddle
    let puddle = find(Surface::Puddle).expect("puddle place");
    assert!(lit(wet_t, puddle).is_none(), "no fire on standing water");
    let mut w = world(SEED);
    w.tick = wet_t;
    assert_eq!(w.can_place(Campfire, puddle.0 as i64 * 1000 + 500, puddle.1 as i64 * 1000 + 500), Err(crate::world::Reject::WetGround));
}

#[test]
fn the_same_world_burns_the_same_way_and_saves_with_its_ash() {
    let run = || {
        let mut w = world(7);
        let tiles = grass_tiles(&w);
        for k in 0..3u64 {
            w.tick = 900 + k * 5000;
            let at = tiles[(k as usize * 4001 + 11) % tiles.len()];
            w.ignite(at.0, at.1);
            for _ in 0..1500 {
                w.step(&[]);
            }
        }
        w
    };
    let (a, b) = (run(), run());
    assert_eq!(a.hash(), b.hash());
    assert_eq!(a.burn, b.burn);
    assert!(!a.burn.is_empty());
    let c = World::from_bytes(&a.to_bytes()).expect("load");
    assert_eq!(c.burn, a.burn);
    assert_eq!(c.hash(), a.hash());
}

/// Lit every day across the seasons, a world never burns away: the share of land under flame or ash stays small.
#[test]
fn a_long_dry_year_does_not_burn_the_map() {
    let mut w = world(11);
    let tiles = grass_tiles(&w);
    let land = tiles.len();
    let mut peak_burn = 0;
    let mut peak_ash = 0;
    // a spark somewhere every in-game hour for four seasons; only the fire step runs, to keep it quick
    let end = TICKS_PER_DAY * DAYS * 4;
    let hour = TICKS_PER_DAY / 24;
    let mut t = 0;
    while t < end {
        if t % hour == 0 {
            let h = crate::rng::hash2(99, (t / hour) as i32, 1) as usize;
            let at = tiles[h % tiles.len()];
            w.ignite(at.0, at.1);
        }
        w.tick = t;
        w.run_surface();
        if t % 30 == 0 {
            peak_burn = peak_burn.max(w.burning().count());
            peak_ash = peak_ash.max(w.burn.len());
        }
        t += 1;
    }
    eprintln!("year: peak burning {peak_burn}, peak ash {peak_ash}, land {land}");
    assert!(peak_burn <= MAX_FIRES);
    assert!(peak_ash * 10 < land, "ash {peak_ash} of {land} land tiles");
}
