// On Windows the game is a window, not a console program (no empty terminal behind it).
#![cfg_attr(windows, windows_subsystem = "windows")]

use alaz_render::glam::{Vec2, Vec3};
use alaz_render::{Camera, Ghost, Renderer};
use alaz_sim::time::TICKS_PER_SECOND;
use alaz_sim::world::{dist2, BUILD_RADIUS_MM};
use alaz_net::{apply_turn, Client, Host};
use alaz_sim::{rules, AgentId, BuildingKind, Command, CommandKind, RouteStep, World};
use hud::{Selection, UiAction, UiCtx};
mod art;
mod audio;
mod book;
mod cave_map;
mod export;
mod hud;
mod icons;
mod inv;
mod item_book;
mod journal_ui;
mod keys;
mod lang;
mod menu;
mod portrait;
mod pxpanel;
mod pilot;
mod progression_view;
mod pxui;
mod fx;
mod gfxcfg;
mod saveio;
mod story;
mod style;
mod ui_art;
mod uishot;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

const STEP: f32 = 1.0 / TICKS_PER_SECOND as f32;
/// Autosave every 7200 ticks (2 real minutes). `ALAZ_AUTOSAVE_TICKS` overrides it for testing.
const AUTOSAVE_TICKS: u64 = 7200;

struct Args {
    new_game: bool,
    seed: Option<u64>,
    hardcore: bool,
    demo: bool,
    creative: bool,
    host: bool,
    size: i32,
    port: u16,
    join: Option<String>,
}

fn parse_args() -> Args {
    let mut a = Args { new_game: false, seed: None, hardcore: false, demo: false, creative: false, host: false, size: alaz_sim::map::MAP_W, port: alaz_net::DEFAULT_PORT, join: None };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--new" => a.new_game = true,
            "--hardcore" => a.hardcore = true,
            "--demo" => a.demo = true,
            "--creative" => a.creative = true,
            "--host" => a.host = true,
            "--size" => a.size = it.next().and_then(|v| v.parse().ok()).unwrap_or(a.size),
            "--port" => a.port = it.next().and_then(|v| v.parse().ok()).unwrap_or(a.port),
            "--join" => a.join = it.next(),
            "--seed" => a.seed = it.next().and_then(|v| v.parse().ok()),
            _ => eprintln!("bilinmeyen argüman: {arg} (--new, --seed N, --hardcore, --creative, --demo, --size N, --host [--port N], --join ADRES)"),
        }
    }
    a
}

fn new_world(seed: u64, hardcore: bool, size: i32) -> World {
    new_world_with(seed, hardcore, size, Default::default())
}

fn new_world_with(seed: u64, hardcore: bool, size: i32, settings: alaz_sim::world::WorldSettings) -> World {
    // new worlds come from the third generator: continents, mountain ranges, climate belts, rivers and lakes
    // (its continents are kilometres across: a small world would sit inside one, so small worlds keep the second)
    // (the third's continents are kilometres across: below 32 km a world would sit inside one, mostly sea or
    // mostly plain without rock, so smaller worlds keep the second)
    let tagged = alaz_sim::map::tag_seed_new_world(seed, size.clamp(alaz_sim::map::MIN_MAP_SIDE, alaz_sim::map::MAX_MAP_SIDE));
    let mut w = World::with_size(tagged, size, size);
    w.settings = settings;
    let (sx, sy) = w.spawn_mm;
    w.spawn_agent(sx, sy);
    w.hardcore = hardcore;
    if settings.difficulty == alaz_sim::world::Difficulty::Creative {
        w.unlock_everything();
    }
    w
}

/// Developer aid (`--demo`): a stocked world with buildings placed, workers hired and a bot on a route.
/// Returns the selection to show (the bot).
fn demo_setup(w: &mut World) -> Selection {
    use alaz_sim::{AgentKind, ItemKind, NodeKind, RecipeId};
    let id = w.agents[0].id;
    let (sx, sy) = w.spawn_mm;
    w.learn_everything();
    let give = |w: &mut World, k: ItemKind, n: u16| w.agents[0].inventory.counts[k as usize] = n;
    let restock = |w: &mut World| {
        for (k, n) in [(ItemKind::Wood, 40), (ItemKind::Stone, 40), (ItemKind::Clay, 10), (ItemKind::IronBloom, 4)] {
            w.agents[0].inventory.counts[k as usize] = n;
        }
    };
    let mut taken: Vec<(i64, i64)> = Vec::new();
    for kind in [
        BuildingKind::Campfire,
        BuildingKind::CharcoalKiln,
        BuildingKind::Furnace,
        BuildingKind::Shelter,
        BuildingKind::Depot,
        BuildingKind::Workshop,
        BuildingKind::Workbench,
    ] {
        restock(w);
        'search: for dy in -9..=9i64 {
            for dx in -9..=9i64 {
                let (x, y) = (sx + dx * 1000, sy + dy * 1000);
                let far = taken.iter().all(|&(tx, ty)| dist2(tx, ty, x, y) >= 3500 * 3500);
                if far && dist2(sx, sy, x, y) <= 7500 * 7500 && w.can_place(kind, x, y).is_ok() {
                    w.step(&[Command { agent: id, kind: CommandKind::Build { kind, x, y } }]);
                    taken.push((x, y));
                    break 'search;
                }
            }
        }
    }
    let find = |w: &World, k: BuildingKind| w.buildings.iter().position(|b| b.kind == k);
    // the kiln is working, a few berries are in the depot
    if let Some(kiln) = find(w, BuildingKind::CharcoalKiln) {
        let (kx, ky) = (w.buildings[kiln].x, w.buildings[kiln].y);
        w.agents[0].x = kx + 1500;
        w.agents[0].y = ky;
        give(w, ItemKind::Wood, 10);
        let rid = alaz_sim::rules().recipes.iter().position(|r| r.id == "charcoal").unwrap() as u16;
        w.step(&[Command { agent: id, kind: CommandKind::StartJob { building: kiln as u32, recipe: RecipeId(rid) } }]);
    }
    let depot = find(w, BuildingKind::Depot);
    if let Some(d) = depot {
        w.buildings[d].output[ItemKind::Berries as usize] = 6;
    }
    give(w, ItemKind::Spear, 1);
    give(w, ItemKind::Hammer, 1);
    give(w, ItemKind::Knife, 1);
    // a water wheel on the nearest river bank, and a wolf lurking beyond the campfire's glow
    let spot = (-45..=45i64)
        .flat_map(|dy| (-45..=45i64).map(move |dx| (dx, dy)))
        .map(|(dx, dy)| (sx + dx * 1000, sy + dy * 1000))
        .filter(|&(x, y)| w.can_place(BuildingKind::WaterWheel, x, y).is_ok())
        .min_by_key(|&(x, y)| dist2(sx, sy, x, y));
    if let Some((x, y)) = spot {
        let home = (w.agents[0].x, w.agents[0].y);
        w.agents[0].x = x + 1500;
        w.agents[0].y = y;
        give(w, ItemKind::Wood, 20);
        w.agents[0].inventory.counts[ItemKind::Stone as usize] = 10;
        w.agents[0].inventory.counts[ItemKind::Gear as usize] = 2;
        w.step(&[Command { agent: id, kind: CommandKind::Build { kind: BuildingKind::WaterWheel, x, y } }]);
        w.agents[0].x = home.0;
        w.agents[0].y = home.1;
    }
    w.wild.push(alaz_sim::Wild::new(9_001, alaz_sim::WildSpecies::Wolf, sx - 12_000, sy + 6_000));
    w.wild.push(alaz_sim::Wild::new(9_000, alaz_sim::WildSpecies::Deer, sx + 9_000, sy + 5_000));
    // workers around the depot
    let (dx, dy) = depot.map_or((sx, sy), |d| (w.buildings[d].x, w.buildings[d].y));
    w.spawn_worker(AgentKind::Ox, dx + 2500, dy + 1200);
    w.spawn_worker(AgentKind::Dog, dx + 2000, dy - 1800);
    w.spawn_worker(AgentKind::Horse, dx - 2600, dy + 1500);
    let bot = w.spawn_worker(AgentKind::Bot, dx + 1500, dy);
    // the bot fetches wood from the nearest tree and drops it in the depot
    if let Some(d) = depot {
        let tree = w
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Tree)
            .min_by_key(|n| {
                let (x, y) = n.center_mm();
                dist2(x, y, dx, dy)
            })
            .map(|n| n.center_mm());
        if let Some((tx, ty)) = tree {
            let route = vec![
                RouteStep::Harvest { kind: NodeKind::Tree, x: tx, y: ty, radius_mm: 4_000 },
                RouteStep::Deposit { building: d as u32 },
            ];
            w.step(&[Command { agent: id, kind: CommandKind::SetRoute { worker: bot, route } }]);
        }
    }
    // the player is in the middle of knapping, so the craft gauge is on screen
    w.agents[0].inventory.counts[ItemKind::Stone as usize] = 6;
    w.agents[0].inventory.counts[ItemKind::StoneAxe as usize] = 1;
    if std::env::var_os("ALAZ_DEMO_DRESSED").is_some() {
        for k in [ItemKind::FiberWrap, ItemKind::HideCloak, ItemKind::HideBoots] {
            w.agents[0].inventory.counts[k as usize] = 1;
        }
    }
    if std::env::var_os("ALAZ_DEMO_CAVE").is_some() {
        // stand beside the first cave mouth on the map
        if let Some(n) = w.nodes.iter().find(|n| n.kind == alaz_sim::NodeKind::CaveMouth && n.tx > 12 && n.ty > 12) {
            w.agents[0].x = n.tx as i64 * 1000 - 1_500;
            w.agents[0].y = n.ty as i64 * 1000 + 2_500;
        }
    }
    if let Some((tx, ty)) = std::env::var("ALAZ_DEMO_TILE").ok().and_then(|v| v.split_once(',').and_then(|(a, b)| Some((a.parse::<i64>().ok()?, b.parse::<i64>().ok()?)))) {
        // stand on an exact tile, to look at a place found by other means
        w.agents[0].x = tx * 1000 + 500;
        w.agents[0].y = ty * 1000 + 500;
    }
    if let Ok(v) = std::env::var("ALAZ_DEMO_AT") {
        // "grass", "forest", "sand" or "peak": stand on the first such spot at least 6 tiles from other ground types
        let want = |tx: i32, ty: i32| -> bool {
            let t = w.map.terrain(tx, ty);
            let ok = match v.as_str() {
                "forest" => t == Some(alaz_sim::Terrain::Forest),
                "sand" => t == Some(alaz_sim::Terrain::Sand),
                // standing on a beach with open water a few metres off
                "coast" => {
                    t == Some(alaz_sim::Terrain::Sand)
                        && (5..9).all(|d| w.map.terrain(tx + d, ty) == Some(alaz_sim::Terrain::Water))
                        && (10..20).all(|d| w.map.terrain(tx + d, ty) == Some(alaz_sim::Terrain::Water))
                }
                "peak" => alaz_sim::elevation_mm(w.seed, tx, ty) > 9_000,
                // out of one's depth, with the shore a few metres off
                "swim" => alaz_sim::map::water_depth(&w.map, tx, ty) == 2 && (-4..=4).any(|d| w.map.terrain(tx + d, ty).is_some_and(|t| t != alaz_sim::Terrain::Water)),
                "steppe" | "jungle" | "autumn" | "tundra" | "desert" | "swamp" => {
                    let (warm, wet) = alaz_sim::climate(w.seed, tx, ty);
                    t == Some(alaz_sim::Terrain::Grass)
                        && match v.as_str() {
                            "steppe" => warm > 580 && wet < 480,
                            "jungle" => warm > 600 && wet > 550,
                            "autumn" => (330..520).contains(&warm) && wet > 450,
                            "desert" => warm > 700 && wet < 300,
                            "swamp" => (440..560).contains(&warm) && wet > 760 && alaz_sim::elevation_mm(w.seed, tx, ty) < 1_500,
                            _ => warm < 330,
                        }
                }
                _ => t == Some(alaz_sim::Terrain::Grass),
            };
            ok && (v == "coast" || v == "swim" || (-4..=4).all(|d| w.map.terrain(tx + d, ty + d) == t && w.map.terrain(tx - d, ty + d) == t))
        };
        'find: for ty in 8..w.map.h - 8 {
            for tx in 8..w.map.w - 8 {
                if want(tx, ty) {
                    w.agents[0].x = tx as i64 * 1000 + 500;
                    w.agents[0].y = ty as i64 * 1000 + 500;
                    break 'find;
                }
            }
        }
    }
    if let Ok(v) = std::env::var("ALAZ_DEMO_WORK") {
        // stand at the nearest tree / rock / clay / bush / water and get to work on it, to look at the work motions
        use alaz_sim::NodeKind as N;
        // an empty pack (bar the axe) so the harvest goes on
        let axe = w.agents[0].inventory.counts[ItemKind::StoneAxe as usize];
        w.agents[0].inventory.counts = [0; ItemKind::COUNT];
        w.agents[0].inventory.counts[ItemKind::StoneAxe as usize] = axe;
        // rock seams want a hammer, trees an axe
        w.agents[0].inventory.counts[ItemKind::Hammer as usize] = 1;
        w.agents[0].inventory.counts[ItemKind::StoneAxe as usize] = 1;
        let (px, py) = ((w.agents[0].x / 1000) as i32, (w.agents[0].y / 1000) as i32);
        let d2 = |tx: i32, ty: i32| (tx - px).pow(2) + (ty - py).pow(2);
        let kind = |k: N| match v.as_str() {
            "chop" => k == N::Tree,
            "mine" => matches!(k, N::StoneOutcrop),
            "dig" => k == N::ClayDeposit,
            "pick" => matches!(k, N::BerryBush),
            _ => false,
        };
        if v == "drink" {
            let mut best = None;
            for ty in (py - 60).max(1)..(py + 60).min(w.map.h - 1) {
                for tx in (px - 60).max(1)..(px + 60).min(w.map.w - 1) {
                    if w.map.terrain(tx, ty) == Some(alaz_sim::map::Terrain::Water) && w.map.terrain(tx, ty + 1).is_some_and(|t| t != alaz_sim::map::Terrain::Water) && w.node_at(tx, ty + 1).is_none()
                        && best.is_none_or(|(_, _, d)| d2(tx, ty) < d) {
                            best = Some((tx, ty, d2(tx, ty)));
                        }
                }
            }
            if let Some((tx, ty, _)) = best {
                w.agents[0].x = tx as i64 * 1000 + 500;
                w.agents[0].y = ty as i64 * 1000 + 1_700;
                w.agents[0].thirst = -30_000;
                w.step(&[Command { agent: id, kind: CommandKind::Drink { tx, ty } }]);
            }
        } else if let Some(i) = (0..w.nodes.len()).filter(|&i| w.nodes[i].amount > 0 && kind(w.nodes[i].kind) && w.node_at(w.nodes[i].tx, w.nodes[i].ty).map(|n| n.0 as usize) == Some(i) && w.node_at(w.nodes[i].tx, w.nodes[i].ty + 1).is_none() && w.node_at(w.nodes[i].tx, w.nodes[i].ty + 2).is_none()).min_by_key(|&i| d2(w.nodes[i].tx, w.nodes[i].ty)) {
            w.nodes[i].amount = 999;
            let n = &w.nodes[i];
            w.agents[0].x = n.tx as i64 * 1000 + 500;
            w.agents[0].y = n.ty as i64 * 1000 + 1_900;
            w.step(&[Command { agent: id, kind: CommandKind::Gather { node: alaz_sim::command::NodeId(i as u32) } }]);
        }
    }
    if std::env::var_os("ALAZ_DEMO_FIRE").is_some() {
        // a lit campfire beside the player, to look at firelight
        let (ax, ay) = (w.agents[0].x, w.agents[0].y);
        let (x, y) = [(1_800, 600), (-1_800, 600), (600, 1_800), (600, -1_800), (2_400, 2_000), (-2_400, -2_000)]
            .into_iter()
            .map(|(dx, dy)| (ax + dx, ay + dy))
            .find(|&(x, y)| w.can_place(alaz_sim::BuildingKind::Campfire, x, y).is_ok())
            .unwrap_or((ax + 1_800, ay + 600));
        w.buildings.push(alaz_sim::Building { fuel: 100_000, ..alaz_sim::Building::new(alaz_sim::BuildingKind::Campfire, x, y) });
    }
    if let Ok(list) = std::env::var("ALAZ_DEMO_BUILD") {
        // machines in rows beside the player, each with its first recipe running, and what they need set up beside
        // them: a windmill for the powered ones, a bellows and a windmill for the converter, a boiler under steam for
        // the hammer (ALAZ_DEMO_BUILD=lathe,press,... by file name)
        let kinds: Vec<BuildingKind> = list.split(',').filter_map(|n| BuildingKind::ALL.iter().copied().find(|k| alaz_render::assets::snake(&format!("{k:?}")) == n.trim())).collect();
        let (ax, ay) = (w.agents[0].x, w.agents[0].y);
        for (i, kind) in kinds.iter().enumerate() {
            // ALAZ_DEMO_GAP=<metres> packs them closer (default 6)
            let gap = std::env::var("ALAZ_DEMO_GAP").ok().and_then(|v| v.parse::<f64>().ok()).map_or(6_000, |m| (m * 1000.0) as i64);
            let (x, y) = (ax - gap * 4 / 3 + (i % 4) as i64 * gap, ay - gap * 4 / 3 + (i / 4) as i64 * gap * 7 / 6);
            let mut b = alaz_sim::Building::new(*kind, x, y);
            let def = rules().building(*kind);
            if let Some(ri) = rules().recipes.iter().position(|r| r.station == Some(*kind)) {
                b.job = Some(alaz_sim::Job { recipe: ri as u16, remaining: rules().recipes[ri].ticks.max(60) * 100 });
                b.temp = def.heat_max;
            }
            let mi = w.buildings.len();
            w.buildings.push(b);
            let aid = |w: &mut World, k: BuildingKind, dx: i64, dy: i64| w.buildings.push(alaz_sim::Building::new(k, x + dx, y + dy));
            let powered = std::env::var_os("ALAZ_DEMO_NOPOWER").is_none();
            if ((kind.needs_power() || (kind.on_grid() && *kind != BuildingKind::Dynamo)) && powered) || matches!(kind, BuildingKind::Bessemer | BuildingKind::Cupola) {
                aid(w, BuildingKind::Windmill, 0, -3_800);
            }
            // grid machines hang on a dynamo that the windmill turns
            if kind.on_grid() && powered && *kind != BuildingKind::Dynamo {
                aid(w, BuildingKind::Dynamo, 2_200, -2_200);
            }
            // a railway carries a load; an accumulator holds some charge
            if *kind == BuildingKind::Railway {
                w.buildings[mi].output[ItemKind::Shaft as usize] = 4;
            }
            if *kind == BuildingKind::Accumulator {
                w.buildings[mi].fuel = 600;
            }
            // a depot with goods in it, for a telegraph or a radio to report
            if *kind == BuildingKind::Depot {
                w.buildings[mi].output[ItemKind::Steel as usize] = 12;
                w.buildings[mi].output[ItemKind::Plank as usize] = 6;
            }
            if *kind == BuildingKind::SteamPump {
                let mut boiler = alaz_sim::Building::new(BuildingKind::Boiler, x + 3_500, y);
                boiler.job = Some(alaz_sim::Job { recipe: rules().recipes.iter().position(|r| r.id == "boiler_coal").unwrap() as u16, remaining: 1_000_000 });
                boiler.temp = 300;
                w.buildings.push(boiler);
            }
            if matches!(kind, BuildingKind::Bessemer | BuildingKind::Cupola) {
                aid(w, BuildingKind::Bellows, 1_800, 0);
            }
            if *kind == BuildingKind::SteamHammer {
                let mut boiler = alaz_sim::Building::new(BuildingKind::Boiler, x + 3_500, y);
                boiler.job = Some(alaz_sim::Job { recipe: rules().recipes.iter().position(|r| r.id == "boiler_coal").unwrap() as u16, remaining: 1_000_000 });
                boiler.temp = 300;
                w.buildings.push(boiler);
            }
        }
    }
    if std::env::var_os("ALAZ_DEMO_MOVE").is_some() {
        // walking instead: shows the route line and the walk animation
        let (x, y) = (w.agents[0].x + 14_000, w.agents[0].y + 9_000);
        w.step(&[Command { agent: id, kind: CommandKind::MoveTo { x, y } }]);
    } else if let Ok(name) = std::env::var("ALAZ_DEMO_CRAFT") {
        // start a recipe by name and play it perfectly up to a step (ALAZ_DEMO_STEP) to look at that screen
        let want: u8 = std::env::var("ALAZ_DEMO_STEP").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        let r = rules().recipes.iter().position(|r| r.id == name).expect("recipe");
        for (item, n) in [(ItemKind::SharpStone, 2), (ItemKind::Branch, 2), (ItemKind::Fiber, 4), (ItemKind::Herb, 2), (ItemKind::Wood, 10)] {
            w.agents[0].inventory.counts[item as usize] = n;
        }
        w.step(&[Command { agent: id, kind: CommandKind::Craft { recipe: RecipeId(r as u16) } }]);
        while let Some(v) = w.craft_view(id) {
            if v.step >= want {
                break;
            }
            let kind = match v.kind {
                alaz_sim::MinigameKind::Knap => Some(CommandKind::StrikeAt { angle: v.center as u16, power: v.power as u8 }),
                alaz_sim::MinigameKind::Fit => Some(CommandKind::PlaceAt { x: v.tx as i16, y: v.ty as i16 }),
                alaz_sim::MinigameKind::Tension => Some(CommandKind::Tighten { power: v.power as u8 }),
                alaz_sim::MinigameKind::Trim => Some(CommandKind::PlaceAt { x: v.tx as i16, y: v.ty as i16 }),
                alaz_sim::MinigameKind::Rub => Some(CommandKind::Rub { pace: v.power as u8, grip: true }),
                alaz_sim::MinigameKind::Timing => (v.pos.abs_diff(v.center) <= v.half / 3).then_some(CommandKind::Strike),
                alaz_sim::MinigameKind::Stack => Some(CommandKind::PlaceAt { x: 0, y: 0 }),
            };
            w.step(&kind.map(|k| vec![Command { agent: id, kind: k }]).unwrap_or_default());
        }
    } else if std::env::var_os("ALAZ_DEMO_WORK").is_none() && std::env::var_os("ALAZ_DEMO_IDLE").is_none() {
        w.step(&[Command { agent: id, kind: CommandKind::Craft { recipe: RecipeId(0) } }]);
    }
    if std::env::var("ALAZ_DEMO_CAVE").as_deref() == Ok("in")
        && let Some(node) = w.nodes.iter().enumerate().find_map(|(i, _)| w.natural_mouth(i as u32).map(|_| alaz_sim::NodeId(i as u32)))
        && let Some(position) = w.cave_entrance_destination(node)
    {
            let n = &w.nodes[node.0 as usize];
            (w.agents[0].x, w.agents[0].y) = n.center_mm();
            w.agents[0].task = alaz_sim::world::Task::Idle;
            w.agents[0].inventory.counts[ItemKind::Torch as usize] = 1;
            w.step(&[Command { agent: id, kind: CommandKind::MoveToFloor { position } }]);
            for _ in 0..200 {
                if w.agents[0].cave_view().is_some() {
                    break;
                }
                w.step(&[]);
            }
            match std::env::var("ALAZ_DEMO_CAVE_DO").as_deref() {
                Ok("dark") => {
                    w.agents[0].inventory.counts[ItemKind::Torch as usize] = 0;
                    w.step(&[]);
                }
                Ok("dig") => {
                    w.agents[0].inventory.counts[ItemKind::IronPickaxe as usize] = 1;
                    if let Some((tx, ty)) = w.cave_dig_target(id) {
                        w.step(&[Command { agent: id, kind: CommandKind::CaveDig { tx, ty } }]);
                        for _ in 0..400 {
                            w.step(&[]);
                        }
                    }
                }
                Ok("walk") => {
                    if let Some((x, y)) = w.cave_down_spot(id) {
                        w.step(&[Command { agent: id, kind: CommandKind::MoveTo { x, y } }]);
                        for _ in 0..240 {
                            w.step(&[]);
                        }
                    }
                }
                _ => {}
            }
        }
    if let Some(h) = std::env::var("ALAZ_DEMO_HOUR").ok().and_then(|v| v.parse::<u64>().ok()) {
        // dev aid: jump to that hour of the first day to look at the world in daylight
        let day = alaz_sim::time::TICKS_PER_DAY;
        let now = alaz_sim::time::minute_of_day(w.tick) as u64 * day / 1440;
        let want = (h % 24) * 60 * day / 1440;
        w.tick += (want + day - now) % day;
    }
    if std::env::var("ALAZ_DEMO_WEATHER").as_deref() == Ok("clear") {
        // dev aid: skip whole days (same hour) until the sky is clear, to look at sun shadows
        let day = alaz_sim::time::TICKS_PER_DAY;
        while w.weather() != alaz_sim::time::Weather::Clear {
            w.tick += day;
        }
    } else if let Ok(v) = std::env::var("ALAZ_DEMO_WEATHER") {
        // jump to the first stretch of rain (or snow, in winter) to look at the sky
        use alaz_sim::time::{weather, Weather, TICKS_PER_DAY};
        let want = if v == "snow" { Weather::Snow } else { Weather::Rain };
        let days = w.settings.season_days as u64;
        // winter is the fourth season
        let start = if v == "snow" { TICKS_PER_DAY * days * 3 } else { TICKS_PER_DAY };
        // with ALAZ_DEMO_HOUR too, the first such weather at that hour
        let hour = std::env::var("ALAZ_DEMO_HOUR").ok().and_then(|v| v.parse::<u32>().ok());
        let at_hour = |t: u64| hour.is_none_or(|h| alaz_sim::time::minute_of_day(t) / 60 == h % 24);
        let mut t = start;
        let step = if hour.is_some() { TICKS_PER_DAY / 96 } else { TICKS_PER_DAY / 6 };
        while t < start + TICKS_PER_DAY * 30 && (weather(w.seed, t, days) != want || !at_hour(t)) {
            t += step;
        }
        w.tick = t + 120;
    }
    // Dev aid: ALAZ_DEMO_SURFACE=mud|puddle|ice|fire|ash looks at the ground states of the surface layer. `mud`: the clear
    // sky just after the first rain, `puddle`: the end of that rain, `ice`: the first frost after wet weather in winter,
    // `fire`: grass alight (older flames beside younger ones) on a dry day, `ash`: the same a quarter minute on.
    if let Ok(v) = std::env::var("ALAZ_DEMO_SURFACE") {
        use alaz_sim::time::{Weather, TICKS_PER_DAY};
        let days = w.settings.season_days as u64;
        // (a moment of the demo clock's hour, on some later day, when the ground and the sky are as wanted)
        let hour = alaz_sim::time::minute_of_day(w.tick) / 60;
        let find = |from: u64, to: u64, ok: &dyn Fn(&alaz_sim::Ground) -> bool| {
            (from..to).step_by(300).find(|&t| alaz_sim::time::minute_of_day(t) / 60 == hour && ok(&alaz_sim::Ground::at(w.seed, t, days)))
        };
        match v.as_str() {
            "mud" => w.tick = find(TICKS_PER_DAY, TICKS_PER_DAY * 60, &|g| g.sky == Weather::Clear && (550..800).contains(&g.wet) && !g.freezing()).unwrap_or(w.tick),
            "puddle" => w.tick = find(TICKS_PER_DAY, TICKS_PER_DAY * 60, &|g| g.sky == Weather::Rain && g.wet >= 980 && !g.freezing()).unwrap_or(w.tick),
            "ice" => w.tick = find(TICKS_PER_DAY * days * 3, TICKS_PER_DAY * days * 4, &|g| g.freezing() && g.wet >= 800).unwrap_or(w.tick),
            "fire" | "ash" => {
                let (px, py) = ((w.agents[0].x / 1000) as i32, (w.agents[0].y / 1000) as i32);
                // an old patch that has burnt down to ash by the time the young one catches
                for dy in -3..=3 {
                    for dx in 2..=7 {
                        w.ignite(px + dx, py + dy - 3);
                    }
                }
                for _ in 0..(if v == "ash" { 1100 } else { 420 }) {
                    w.step(&[]);
                }
                for dy in -3..=3 {
                    for dx in 5..=10 {
                        w.ignite(px + dx, py + dy + 2);
                    }
                }
            }
            _ => {}
        }
    }
    if std::env::var_os("ALAZ_DEMO_NEWBIE").is_some() {
        // a tribe that has learned nothing yet: empty hands, nothing seen, see how little it can make out
        for a in w.agents.iter_mut() {
            a.inventory.counts = [0; ItemKind::COUNT];
        }
        for b in w.buildings.iter_mut() {
            b.input = [0; ItemKind::COUNT];
            b.output = [0; ItemKind::COUNT];
        }
        w.forget_everything();
    }
    // Dev aids for looking at the character: ALAZ_DEMO_STAGE=0..5 sets the perception stage (and so the body, the
    // interface theme and the font); ALAZ_DEMO_EVO=0..9 (fractional) draws the body and portrait at that stage
    // coordinate whatever the stage is; ALAZ_DEMO_OUTFIT=<mask> puts clothes in the player's hands: 1 fibre wrap, 2 cloak,
    // 4 boots, 8 tunic, 16 hood, 32 bronze plate, 64 steel helm, 128 apron, 256 felt cloak, 512 leather armour
    // (ALAZ_DEMO_DRESSED is mask 7).
    if let Some(st) = std::env::var("ALAZ_DEMO_STAGE").ok().and_then(|v| v.parse::<usize>().ok()) {
        w.learn_first([0, 4, 11, 21, 32, 43][st.min(5)]);
    }
    if let Some(mask) = std::env::var("ALAZ_DEMO_OUTFIT").ok().and_then(|v| v.parse::<u32>().ok()) {
        const WEAR: [ItemKind; 10] = [ItemKind::FiberWrap, ItemKind::HideCloak, ItemKind::HideBoots, ItemKind::Tunic, ItemKind::LinenHood, ItemKind::BronzePlate, ItemKind::SteelHelm, ItemKind::WorkApron, ItemKind::FeltCloak, ItemKind::LeatherArmor];
        for (i, k) in WEAR.iter().enumerate() {
            if mask & (1 << i) != 0 {
                w.agents[0].inventory.counts[*k as usize] = 1;
            }
        }
    }
    // Dev aids for the journal and the camp. ALAZ_DEMO_JOURNAL=1: some days of history and a tribe that knows its
    // first 21 discoveries (with paper in the bag). ALAZ_DEMO_CAMP=1: dusk, a lit fire, the player sitting at it;
    // ALAZ_DEMO_CAMP=goal: the morning after, with the goal chosen at the fire.
    let camp = std::env::var("ALAZ_DEMO_CAMP").ok();
    if std::env::var_os("ALAZ_DEMO_JOURNAL").is_some() || camp.is_some() {
        w.learn_first(21);
        w.agents[0].inventory.counts[ItemKind::Paper as usize] = 2;
        w.tick += 6 * alaz_sim::time::TICKS_PER_DAY;
        w.demo_journal(6);
    }
    if let Some(mode) = camp {
        use alaz_sim::time::{minute_of_day, TICKS_PER_DAY};
        let now = minute_of_day(w.tick) as u64 * TICKS_PER_DAY / 1440;
        w.tick += (19 * 60 * TICKS_PER_DAY / 1440 + TICKS_PER_DAY - now) % TICKS_PER_DAY;
        let (ax, ay) = (w.agents[0].x, w.agents[0].y);
        w.buildings.push(alaz_sim::Building { fuel: 100_000, ..alaz_sim::Building::new(BuildingKind::Campfire, ax + 1_800, ay + 600) });
        w.step(&[Command { agent: id, kind: CommandKind::Camp }]);
        if mode == "goal" {
            if let Some(g) = w.camp_goals().first().copied() {
                w.step(&[Command { agent: id, kind: CommandKind::SetGoal { goal: Some(g) } }]);
            }
            w.tick += 15 * 60 * TICKS_PER_DAY / 1440;
            w.agents[0].seated = None;
        }
    }
    if std::env::var_os("ALAZ_PLAYLOG").is_none() {
        w.take_events();
    }
    if std::env::var_os("ALAZ_DEMO_WORK").is_some() {
        return Selection::Agent(0);
    }
    Selection::Agent(bot.0 as usize)
}

fn random_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
}

const JOIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

/// A join running on its own thread: one message, the joined client and the host's world, or why it failed.
type Pending = std::sync::mpsc::Receiver<Result<(Client, World), alaz_net::NetError>>;

/// Connect without blocking the window: `Client::connect` can wait up to `JOIN_TIMEOUT`.
fn connect_in_background(addr: String) -> Pending {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // the receiver is gone when the player started another world meanwhile; the result is not needed then
        let _ = tx.send(Client::connect(&addr, JOIN_TIMEOUT));
    });
    rx
}

/// A join through the relay also waits for the host to pair, which the relay allows 10 s.
const CODE_JOIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(16);

fn connect_code_in_background(code: String) -> Pending {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(Client::connect_code(&code, CODE_JOIN_TIMEOUT));
    });
    rx
}

/// "192.168.1.5" → "192.168.1.5:7777"; an address with a port stays as it is.
fn normalize_addr(s: &str) -> String {
    let s = s.trim();
    if s.contains(':') { s.to_string() } else { format!("{s}:{}", alaz_net::DEFAULT_PORT) }
}

fn lan_ip_text() -> String {
    alaz_net::discovery::lan_ip().map_or_else(|| "?".to_string(), |ip| ip.to_string())
}

/// Let LAN players find this host. Hosting works without it: they can still type the address.
fn host_discovery(host: &mut Host, slot: u8, world: &World) -> Result<(), String> {
    host.enable_discovery(alaz_net::discovery::DISCOVERY_PORT, format!("Yuva {slot} · Gün {}", world.day() + 1))
        .map_err(|e| format!("Ağda bulunma kapalı: {e}. Arkadaşların adresi elle yazabilir."))
}

struct Gfx {
    window: Arc<Window>,
    renderer: Renderer,
}

/// `ALAZ_PERF=1` prints frame statistics every 5 s. `ALAZ_BENCH=<secs>` additionally flies the camera
/// around the map (fast pan, spin, zoom swings), prints a summary and exits. Pair with `ALAZ_NOVSYNC=1`.
struct Perf {
    bench_secs: Option<f32>,
    t0: Instant,
    last_print: Instant,
    frame_dt: Vec<f32>,
    render_ms: Vec<f32>,
    sim_ms: Vec<f32>,
    max_chunks: u32,
    max_verts: u64,
    built0: u64,
    build0: f64,
}

impl Perf {
    fn from_env() -> Option<Perf> {
        let bench_secs = std::env::var("ALAZ_BENCH").ok().and_then(|v| v.parse::<f32>().ok());
        if bench_secs.is_none() && std::env::var_os("ALAZ_PERF").is_none() {
            return None;
        }
        let now = Instant::now();
        Some(Perf { bench_secs, t0: now, last_print: now, frame_dt: vec![], render_ms: vec![], sim_ms: vec![], max_chunks: 0, max_verts: 0, built0: 0, build0: 0.0 })
    }

    fn report(&mut self, label: &str, built: u64, build_ms: f64) {
        fn pct(v: &mut Vec<f32>, p: f32) -> f32 {
            if v.is_empty() {
                return 0.0;
            }
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            v[((v.len() as f32 * p) as usize).min(v.len() - 1)]
        }
        let n = self.frame_dt.len().max(1) as f32;
        let avg = self.frame_dt.iter().sum::<f32>() / n;
        let r = self.render_ms.iter().sum::<f32>() / n;
        let s = self.sim_ms.iter().sum::<f32>() / n;
        let (p99, mx) = (pct(&mut self.frame_dt, 0.99) * 1000.0, pct(&mut self.frame_dt, 1.0) * 1000.0);
        let rp99 = pct(&mut self.render_ms, 0.99);
        eprintln!(
            "[perf] {label}: {:.0} fps | frame avg {:.2} ms p99 {:.2} max {:.2} | render() avg {:.2} p99 {:.2} ms | sim avg {:.3} ms | chunks<={} verts<={} | chunk builds {} ({:.1} ms)",
            1.0 / avg.max(1e-6), avg * 1000.0, p99, mx, r, rp99, s, self.max_chunks, self.max_verts, built - self.built0, build_ms - self.build0
        );
        self.frame_dt.clear();
        self.render_ms.clear();
        self.sim_ms.clear();
        self.built0 = built;
        self.build0 = build_ms;
    }
}

enum Net {
    Solo,
    Host(Host),
    Client(Client),
}

struct App {
    gfx: Option<Gfx>,
    world: World,
    cam: Camera,
    pitch_goal: f32,
    keys: HashSet<KeyCode>,
    pending: Vec<Command>,
    cursor: (f32, f32),
    last: Instant,
    accum: f32,
    /// Sandbox: how many sim ticks run per tick of real time (solo only).
    time_scale: u32,
    fps_t: f32,
    fps_n: u32,
    hints: Vec<(String, f32)>,
    warned: [bool; 3],
    last_save_tick: u64,
    autosave_ticks: u64,
    build_mode: Option<BuildingKind>,
    /// Set while a placed building is being moved: the ghost places it instead of building a new one.
    moving: Option<usize>,
    /// Quarter turns for the building being placed or moved (R turns it).
    facing: u8,
    /// A building just ordered, to be turned to `facing` once it stands: (x, y, quarter turns).
    turn_after: Option<(i64, i64, u8)>,
    grid_snap: bool,
    selected: Option<Selection>,
    /// The selected machine the player stood within reach of last frame; walking out of reach closes its window.
    machine_near: Option<usize>,
    recording: bool,
    recorded: Vec<RouteStep>,
    wild_hinted: bool,
    wolf_hinted: bool,
    /// Which ground states have been explained once (bit per `alaz_sim::Surface`).
    surf_hinted: u8,
    last_strike: Option<(bool, f32)>,
    /// The evolution stage (0..=9) last seen, to announce the next one.
    last_stage: u8,
    /// The perception (permille) the people are drawn at: it glides toward the world's over a few seconds, so a
    /// sudden gain in understanding changes the body gradually (None until the first frame of a world).
    evo_view: Option<f32>,
    /// The stage coordinate drawn this frame (`alaz_render::Evo`), for the bodies and the portrait.
    evo_pos: f32,
    cave_map: cave_map::CaveMap,
    /// Dev aid: `ALAZ_DEMO_EVO=0..9` draws everyone at this stage coordinate, whatever the world says.
    evo_force: Option<f32>,
    knap: hud::KnapState,
    story: story::Story,
    /// The title screen and the way into a world.
    fx: fx::Fx,
    /// How many times the dog has been petted (just picks the line).
    pets: u32,
    /// A debug script driving the player (`ALAZ_SCRIPT=file`), and where its log goes.
    script: Option<alaz_sim::debug::Script>,
    script_done: bool,
    /// Dev aid: plays through real input events (`ALAZ_PILOT=file`).
    pilot: Option<pilot::Pilot>,
    /// Dev aid: the pilot froze the simulation.
    paused: bool,
    audio: Option<audio::Audio>,
    /// Health last frame, to notice a blow.
    prev_hp: Option<i32>,
    /// 1 right after a blow, fading to 0: red edge on the screen and a shake.
    hurt_flash: f32,
    /// Seconds until the ambience is told about the surroundings again.
    scene_t: f32,
    demo_hurt_t: f32,
    demo_cliff: Option<(i32, i32, i32, i32)>,
    quit: bool,
    binds: keys::Keybinds,
    /// The camera trails the player until the player pans away by hand.
    follow: bool,
    /// Zoom before a craft closed in on the worker, restored afterwards.
    zoom_before: Option<f32>,
    /// Keyboard walking: whether the walk keys were held last frame, and the time until the next steer command.
    walking: bool,
    sprinting: bool,
    walk_cool: f32,
    tab: Option<hud::Tab>,
    mid_drag: Option<(f32, f32)>,
    menu: Option<menu::Menu>,
    net: Net,
    perf: Option<Perf>,
    me: AgentId,
    /// Joined players never write the shared save slot.
    no_save: bool,
    /// A join started from the menu: the normalized address and its background connect.
    connecting: Option<(String, Pending)>,
    /// Looks for LAN hosts while the multiplayer screen is open.
    finder: Option<alaz_net::discovery::Finder>,
    /// The finder could not open a socket: do not retry every frame until the screen is opened again.
    finder_off: bool,
}

impl App {
    fn new(args: Args) -> Self {
        let mut hints = Vec::new();
        let mut loaded = None;
        let mut net = Net::Solo;
        let mut me = AgentId(0);
        let mut joined = None;
        if let Some(addr) = &args.join {
            let addr = normalize_addr(addr);
            match Client::connect(&addr, JOIN_TIMEOUT) {
                Ok((client, world)) => {
                    me = client.agent;
                    hints.push((format!("{addr} sunucusuna katıldın."), hud::HINT_SECONDS));
                    net = Net::Client(client);
                    joined = Some(world);
                }
                Err(e) => {
                    eprintln!("katılınamadı ({addr}): {e}");
                    std::process::exit(1);
                }
            }
        }
        // a plain launch opens the start menu; any world-selecting flag skips it
        let show_menu = !(args.new_game || args.demo || args.host || args.join.is_some() || args.seed.is_some() || args.hardcore);
        if !args.new_game && joined.is_none() && !show_menu {
            match saveio::load(me) {
                Ok(w) => loaded = w,
                Err(e) => {
                    eprintln!("kayıt yüklenemedi: {e}");
                    hints.push((format!("Kayıt yüklenemedi ({e}). Yeni oyun başlıyor."), hud::HINT_SECONDS));
                }
            }
        }
        let (world, seen) = match loaded {
            _ if joined.is_some() => (joined.take().unwrap(), Default::default()),
            Some(save) => {
                hints.push((format!("Kayıt yüklendi (gün {}).", save.world.day() + 1), hud::HINT_SECONDS));
                (save.world, save.seen)
            }
            None if show_menu => (new_world(1, false, 64), Default::default()),
            None => {
                hints.push(("Sol tık: yürü. Sağ tık: topla. Taş, odun, yemiş, dal ve lif topla.".into(), hud::HINT_SECONDS));
                hints.push(("Yap sekmesinde eline iki şey alıp Dene dersen ne olacağını öğrenirsin.".into(), hud::HINT_SECONDS));
                (new_world_with(
                    args.seed.unwrap_or_else(random_seed),
                    args.hardcore,
                    args.size,
                    alaz_sim::world::WorldSettings { difficulty: if args.creative { alaz_sim::world::Difficulty::Creative } else { Default::default() }, ..Default::default() },
                ), Default::default())
            }
        };
        let mut app = Self {
            gfx: None,
            world,
            pitch_goal: Camera::PITCH_ISO,
            cam: Camera::new(),
            keys: HashSet::new(),
            pending: Vec::new(),
            cursor: (0.0, 0.0),
            last: Instant::now(),
            accum: 0.0,
            // dev aid: ALAZ_TIME_SCALE=N runs a sandbox world N times faster from the start
            time_scale: std::env::var("ALAZ_TIME_SCALE").ok().and_then(|v| v.parse().ok()).unwrap_or(1u32).clamp(1, 10),
            fps_t: 0.0,
            fps_n: 0,
            hints,
            warned: [false; 3],
            last_save_tick: 0,
            autosave_ticks: std::env::var("ALAZ_AUTOSAVE_TICKS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(AUTOSAVE_TICKS),
            build_mode: None,
            grid_snap: false,
            selected: None,
            machine_near: None,
            recording: false,
            recorded: Vec::new(),
            wild_hinted: false,
            wolf_hinted: false,
            surf_hinted: 0,
            last_strike: None,
            last_stage: 255,
            evo_view: None,
            evo_pos: 9.0,
            cave_map: cave_map::CaveMap::from_known(seen),
            evo_force: std::env::var("ALAZ_DEMO_EVO").ok().and_then(|v| v.parse::<f32>().ok()).map(|v| v.clamp(0.0, 9.0)),
            knap: hud::KnapState::default(),
            story: story::Story::default(),
            fx: fx::Fx::default(),
            pets: 0,
            moving: None,
            facing: 0,
            turn_after: None,
            script_done: false,
            pilot: pilot::Pilot::from_env(),
            paused: false,
            script: std::env::var("ALAZ_SCRIPT").ok().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| alaz_sim::debug::Script::new(&t)),
            audio: audio::Audio::new(),
            prev_hp: None,
            hurt_flash: 0.0,
            scene_t: 0.0,
            demo_hurt_t: 0.0,
            demo_cliff: None,
            menu: None,
            quit: false,
            binds: keys::Keybinds::load(),
            follow: true,
            zoom_before: None,
            walking: false,
            sprinting: false,
            walk_cool: 0.0,
            tab: None,
            mid_drag: None,
            no_save: show_menu || matches!(net, Net::Client(_)),
            perf: Perf::from_env(),
            net,
            me,
            connecting: None,
            finder: None,
            finder_off: false,
        };
        // dev aid: ALAZ_DEMO_EVO_CARD=<stage 0..9> shows the card for reaching that stage at the start
        if let Some(i) = std::env::var("ALAZ_DEMO_EVO_CARD").ok().and_then(|v| v.parse::<usize>().ok()) {
            app.fx.evolved(hud::EVO_NAMES[i.min(hud::EVO_NAMES.len() - 1)]);
        }
        if let Some(z) = std::env::var("ALAZ_DEMO_ZOOM").ok().and_then(|v| v.parse::<f32>().ok()) {
            app.cam.zoom = z;
        }
        // the model preview wants to be close
        if std::env::var_os("ALAZ_DEMO_VIEW").is_some() && std::env::var_os("ALAZ_DEMO_ZOOM").is_none() {
            app.cam.zoom = 2.0;
        }
        if let Some(p) = std::env::var("ALAZ_DEMO_PITCH").ok().and_then(|v| v.parse::<f32>().ok()) {
            app.pitch_goal = p.clamp(Camera::PITCH_MIN, Camera::PITCH_TOP);
            app.cam.pitch_deg = app.pitch_goal;
        }
        app.tab = match std::env::var("ALAZ_DEMO_TAB").as_deref() {
            Ok("bag") => Some(hud::Tab::Bag),
            Ok("craft") => Some(hud::Tab::Craft),
            Ok("build") => Some(hud::Tab::Build),
            Ok("help") => Some(hud::Tab::Help),
            Ok("creative") => Some(hud::Tab::Creative),
            _ => None,
        };
        // ALAZ_DEMO_JOURNAL=days|finds|recipes opens that page of the journal
        match std::env::var("ALAZ_DEMO_JOURNAL").as_deref() {
            Ok("finds") => app.knap.journal.section = 1,
            Ok("recipes") => app.knap.journal.section = 2,
            _ => {}
        }
        if std::env::var_os("ALAZ_DEMO_JOURNAL").is_some() {
            app.knap.journal.book_open = true;
        }
        if show_menu && app.perf.as_ref().is_none_or(|p| p.bench_secs.is_none()) {
            app.menu = Some(menu::Menu::new(false));
            app.no_save = true;
        }
        if app.perf.as_ref().is_some_and(|p| p.bench_secs.is_some()) {
            app.no_save = true;
        }
        if args.host && !args.join.is_some() {
            match Host::bind(("0.0.0.0", args.port)) {
                Ok(mut h) => {
                    if let Err(hint) = host_discovery(&mut h, saveio::slot(), &app.world) {
                        app.say(hint);
                    }
                    app.net = Net::Host(h);
                    app.say(format!("Coop sunucusu açık: port {}. Arkadaşların --join <senin-ip> ile katılabilir.", args.port));
                }
                Err(e) => {
                    eprintln!("sunucu açılamadı (port {}): {e}", args.port);
                    std::process::exit(1);
                }
            }
        }
        if args.demo {
            app.selected = Some(demo_setup(&mut app.world));
            // ALAZ_DEMO_SELECT=<building file name>: show that building's panel instead of the bot's
            if let Ok(n) = std::env::var("ALAZ_DEMO_SELECT")
                && let Some(i) = app.world.buildings.iter().position(|b| alaz_render::assets::snake(&format!("{:?}", b.kind)) == n) {
                    app.selected = Some(hud::Selection::Building(i));
                }
        }
        app.last_save_tick = app.world.tick;
        app.center_camera();
        // the title screen, or straight into the world; not in the dev demo, benchmarks or when turned off
        let quiet_fx = args.demo || app.perf.is_some() || std::env::var_os("ALAZ_NOFX").is_some();
        if !quiet_fx {
            if app.menu.is_some() {
                app.fx.boot();
            } else {
                app.fx.enter_world(&app.world);
            }
        }
        // a fresh world opens with its story; a loaded one only tells the ages still to come
        if app.world.tick == 0 && app.menu.is_none() && !args.demo {
            app.story.begin(&app.world);
        } else {
            app.story.resume(&app.world);
        }
        app
    }

    fn center_camera(&mut self) {
        let position = self.me_agent().map(|a| self.world.agent_walk_position(a));
        let (x, y) = position.map_or(self.world.spawn_mm, |p| (p.x_mm, p.y_mm));
        let x = x as f32 / 1000.0;
        let z = y as f32 / 1000.0;
        let height = position.map_or_else(|| alaz_render::ground_height(self.world.seed, x, z), |p| p.z_mm as f32 / 1000.0);
        self.cam.target = Vec3::new(x, height, z);
    }

    /// Closes the machine window once the player walks from within reach of the selected machine to beyond it. A
    /// machine picked from afar stays open (its slots are disabled) until the player has been near it once.
    fn follow_machine(&mut self) {
        let near = match self.selected {
            Some(Selection::Building(i)) => self
                .world
                .buildings
                .get(i)
                .filter(|b| inv::machine::opens_window(&self.world, b.kind))
                .zip(self.me_agent())
                .is_some_and(|(b, me)| inv::machine::within_reach(me, b))
                .then_some(i),
            _ => None,
        };
        let walked_off = self.machine_near.is_some_and(|was| near != Some(was) && self.selected == Some(Selection::Building(was)));
        if walked_off {
            self.selected = None;
        }
        self.machine_near = near;
    }

    fn me_agent(&self) -> Option<&alaz_sim::world::Agent> {
        self.world.agents.get(self.me.0 as usize)
    }

    /// Show a hint. The same text said again (a rejected click repeated, say) refreshes the one on screen
    /// instead of stacking copies that push everything else away.
    fn say(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.hints.retain(|(t, _)| *t != text);
        self.hints.push((text, hud::HINT_SECONDS));
    }

    fn replace_world(&mut self, mut w: World) {
        if w.agents.is_empty() {
            let (sx, sy) = w.spawn_mm;
            w.spawn_agent(sx, sy);
        }
        if let Net::Host(host) = &mut self.net {
            host.replace_world(&mut w);
        }
        self.world = w;
        self.cave_map = cave_map::CaveMap::default();
        self.pending.clear();
        self.build_mode = None;
        self.selected = None;
        self.recording = false;
        // route steps point at buildings by index: they mean nothing in another world
        self.recorded.clear();
        self.accum = 0.0;
        self.warned = [false; 3];
        self.last_save_tick = self.world.tick;
        // per-world UI state: without this a loaded save flashes red (health "dropped"), announces an
        // evolution it already had, or keeps a half-played craft and stale hands
        self.prev_hp = None;
        self.hurt_flash = 0.0;
        self.last_stage = 255;
        self.evo_view = None;
        self.last_strike = None;
        self.knap = hud::KnapState::default();
        self.zoom_before = None;
        self.walking = false;
        self.sprinting = false;
        self.follow = true;
        self.center_camera();
        if let Some(g) = self.gfx.as_mut() {
            g.renderer.reset_scene();
        }
        self.story.resume(&self.world);
    }

    fn restore_saved_world(&mut self, save: saveio::LoadedSave) {
        self.replace_world(save.world);
        self.cave_map = cave_map::CaveMap::from_known(save.seen);
    }

    fn toggle_tab(&mut self, t: hud::Tab) {
        self.tab = if self.tab == Some(t) { None } else { Some(t) };
    }

    // The window's input, one method per kind, so the pilot's made-up events take the same path as real ones.
    fn input_key(&mut self, code: KeyCode, pressed: bool, repeat: bool) {
        if pressed {
            let used = self.menu.as_mut().is_some_and(|m| if m.capturing() { m.on_key(code, &mut self.binds) } else { false });
            if !repeat && !used {
                for action in self.binds.actions_for(code) {
                    self.on_action(action);
                }
            }
            if !used {
                self.keys.insert(code);
            }
        } else {
            self.keys.remove(&code);
        }
    }

    fn input_cursor(&mut self, now: (f32, f32)) {
        let shift = self.keys.contains(&KeyCode::ShiftLeft) || self.keys.contains(&KeyCode::ShiftRight);
        if let (Some(from), false) = (self.mid_drag, shift) {
            // middle drag orbits: across turns the camera, up and down tilts it from low and wide to straight down
            self.cam.yaw_deg -= (now.0 - from.0) * 0.35;
            self.pitch_goal = (self.pitch_goal + (now.1 - from.1) * 0.25).clamp(Camera::PITCH_MIN, Camera::PITCH_TOP);
            self.cam.pitch_deg = self.pitch_goal;
            self.mid_drag = Some(now);
        } else if let (Some(from), Some(g)) = (self.mid_drag, self.gfx.as_ref()) {
            // shift + middle drag: drag the ground under the cursor along with it
            let (w, h) = g.renderer.size();
            let at = |c: (f32, f32)| self.cam.ground_at(2.0 * c.0 / w as f32 - 1.0, 1.0 - 2.0 * c.1 / h as f32, g.renderer.aspect());
            if let (Some(a), Some(b)) = (at(from), at(now)) {
                self.cam.target.x += a.x - b.x;
                self.cam.target.z += a.z - b.z;
                self.follow = false;
            }
            self.mid_drag = Some(now);
        }
        self.cursor = now;
    }

    fn input_button(&mut self, button: MouseButton, pressed: bool, ui_consumed: bool) {
        match button {
            MouseButton::Middle => self.mid_drag = pressed.then_some(self.cursor),
            MouseButton::Left | MouseButton::Right if pressed => {
                let over_ui = self.gfx.as_ref().is_some_and(|g| g.renderer.pointer_over_ui());
                if !ui_consumed && !over_ui {
                    self.click(button == MouseButton::Left);
                }
            }
            _ => {}
        }
    }

    fn input_wheel(&mut self, lines: f32) {
        if self.menu.is_none() {
            self.cam.zoom = (self.cam.zoom * 1.1f32.powf(lines)).clamp(0.2, 8.0);
        }
    }

    /// How many sim ticks run per real tick. Creative worlds choose it in the UI; for tests `ALAZ_TIME_SCALE` works in
    /// any solo world.
    fn sim_scale(&self) -> u32 {
        let solo = matches!(self.net, Net::Solo);
        let free = self.world.settings.difficulty == alaz_sim::world::Difficulty::Creative || std::env::var_os("ALAZ_TIME_SCALE").is_some();
        if self.paused {
            0
        } else if solo && free {
            self.time_scale.max(1)
        } else {
            1
        }
    }

    /// The pilot's turn: it gets game time, and what it does reaches the game as ordinary input.
    fn pilot_tick(&mut self, raw_dt: f32) {
        let Some(mut pilot) = self.pilot.take() else { return };
        if let Some(g) = self.gfx.as_ref() {
            // a paused game still lets the pilot work, in real time
            let scale = self.sim_scale().max(1) as f64;
            let sim_s = self.world.tick as f64 / TICKS_PER_SECOND as f64;
            let ctx = g.renderer.egui_context().clone();
            let ppp_now = ctx.pixels_per_point();
            // a point under a HUD panel or window cannot be clicked in the world
            let pick = |x: f32, y: f32| {
                let covered = ctx.layer_id_at(alaz_render::egui::pos2(x / ppp_now, y / ppp_now)).is_some_and(|l| l.order != alaz_render::egui::Order::Background);
                if covered { None } else { self.node_under((x, y)) }
            };
            let view = pilot::View { signal: pilot::read_signal(g.renderer.egui_context()), craft: self.world.craft_view(self.me), pick: &pick, world: &self.world, me: self.me, cam: self.cam, win: g.renderer.size(), ppp: g.renderer.egui_context().pixels_per_point(), marks: pilot::marks(g.renderer.egui_context()), sim_s };
            let inputs = pilot.tick(raw_dt.min(0.25) as f64 * 1000.0 * scale, raw_dt, &view);
            let wants: Vec<pilot::Want> = std::mem::take(&mut pilot.wants);
            let mut shots: Vec<String> = Vec::new();
            let ppp = view.ppp;
            for want in wants {
                match want {
                    pilot::Want::State => {
                        let line = self.pilot_state();
                        pilot.say(sim_s, line);
                    }
                    pilot::Want::Look => {
                        for l in pilot.look(&view) {
                            pilot.say(sim_s, l);
                        }
                    }
                    pilot::Want::Shot(name) => {
                        let dir = std::path::PathBuf::from(std::env::var_os("ALAZ_PILOT").unwrap_or_default());
                        let file = dir.with_file_name(format!("{name}.ppm"));
                        shots.push(file.display().to_string());
                        pilot.say(sim_s, format!("shot {}", file.display()));
                    }
                    pilot::Want::Marks => {
                        let names: Vec<String> = view.marks.iter().map(|(l, r)| format!("{l} @{:.0},{:.0}", r.center().x * view.ppp, r.center().y * view.ppp)).collect();
                        pilot.say(sim_s, format!("marks: {}", names.join(" | ")));
                    }
                }
            }
            if let Some(g) = self.gfx.as_mut() {
                for s in shots {
                    g.renderer.request_shot(s);
                }
            }
            if let Some(n) = pilot.speed_req.take() {
                self.paused = n == 0;
                if n > 0 {
                    self.time_scale = n.clamp(1, 10);
                }
                pilot.say(sim_s, format!("speed {}", if n == 0 { "paused".to_string() } else { format!("x{}", n.clamp(1, 10)) }));
            }
            self.pilot = Some(pilot);
            self.pilot_apply(inputs, ppp);
        } else {
            self.pilot = Some(pilot);
        }
    }

    fn pilot_apply(&mut self, inputs: Vec<pilot::Input>, ppp: f32) {
        use alaz_render::egui;
        for input in inputs {
            let mut events = Vec::new();
            let at = |c: (f32, f32)| egui::pos2(c.0 / ppp, c.1 / ppp);
            match input {
                pilot::Input::Cursor(x, y) => {
                    events.push(egui::Event::PointerMoved(at((x, y))));
                    self.input_cursor((x, y));
                }
                pilot::Input::Button(b, pressed) => {
                    let button = match b {
                        pilot::Btn::Left => MouseButton::Left,
                        pilot::Btn::Right => MouseButton::Right,
                        pilot::Btn::Middle => MouseButton::Middle,
                    };
                    let eb = match b {
                        pilot::Btn::Left => egui::PointerButton::Primary,
                        pilot::Btn::Right => egui::PointerButton::Secondary,
                        pilot::Btn::Middle => egui::PointerButton::Middle,
                    };
                    events.push(egui::Event::PointerButton { pos: at(self.cursor), button: eb, pressed, modifiers: self.pilot_mods() });
                    // the egui pass of the next frame decides whether the UI took it; the game asks that pass's answer
                    self.input_button(button, pressed, false);
                }
                pilot::Input::Key(code, pressed) => {
                    if let Some(key) = pilot::egui_key(code) {
                        events.push(egui::Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers: self.pilot_mods() });
                    }
                    self.input_key(code, pressed, false);
                }
                pilot::Input::Wheel(n) => {
                    events.push(egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Line,
                        delta: egui::vec2(0.0, n),
                        phase: egui::TouchPhase::Move,
                        source: egui::MouseWheelSource::Wheel,
                        modifiers: self.pilot_mods(),
                    });
                    if !self.gfx.as_ref().is_some_and(|g| g.renderer.pointer_over_ui()) {
                        self.input_wheel(n);
                    }
                }
            }
            if let Some(g) = self.gfx.as_mut() {
                g.renderer.inject_ui_events(events);
            }
        }
    }

    fn pilot_mods(&self) -> alaz_render::egui::Modifiers {
        let shift = self.keys.contains(&KeyCode::ShiftLeft) || self.keys.contains(&KeyCode::ShiftRight);
        let ctrl = self.keys.contains(&KeyCode::ControlLeft);
        alaz_render::egui::Modifiers { shift, ctrl, command: ctrl, ..Default::default() }
    }

    /// One line about the player and the world, for the pilot's log.
    fn pilot_state(&self) -> String {
        let Some(a) = self.me_agent() else { return "no player".into() };
        let w = &self.world;
        let pack: Vec<String> = alaz_sim::ItemKind::ALL.iter().filter(|k| a.inventory.get(**k) > 0).map(|k| format!("{} {}", alaz_sim::debug::snake(&format!("{k:?}")), a.inventory.get(*k))).collect();
        let day = w.tick / alaz_sim::time::TICKS_PER_DAY;
        let craft = self.world.craft_view(self.me).map(|c| format!(" | CRAFT {:?} step {}/{} done {} pos {} center {} half {} span {} power {}+-{} t({},{}) stack {}", c.kind, c.step, c.steps, c.done, c.pos, c.center, c.half, c.span, c.power, c.power_half, c.tx, c.ty, c.stack_n)).unwrap_or_default();
        format!(
            "STATE day {day}{craft} | {} | perception {} ({}) | at ({:.0},{:.0}) m | hp {} food {} water {} warm {} stam {} | task {:?} | tab {:?} | selected {} | pack: {}",
            hud::era_name(w),
            w.perception_permille(),
            w.perception_level(),
            a.x as f64 / 1000.0,
            a.y as f64 / 1000.0,
            a.health,
            a.hunger,
            a.thirst,
            a.warmth,
            a.stamina,
            a.task,
            self.tab,
            self.selected.is_some(),
            pack.join(", ")
        )
    }

    fn on_action(&mut self, action: keys::Action) {
        use keys::Action as A;
        // while a menu is open only the menu key and saving make sense
        if self.menu.is_some() {
            if action == A::Menu && self.menu.as_ref().is_some_and(|m| m.paused && !m.capturing()) {
                self.menu = None;
            }
            return;
        }
        match action {
            A::Menu => {
                if self.build_mode.is_some() || self.selected.is_some() {
                    self.build_mode = None;
                    self.selected = None;
                } else if self.tab.is_some() {
                    self.tab = None;
                } else if self.knap.journal.book_open {
                    self.knap.journal.book_open = false;
                } else {
                    self.menu = Some(menu::Menu::new(true));
                }
            }
            A::Grid => self.grid_snap = !self.grid_snap,
            // held, not pressed: the minigames read it every frame
            A::Grip => {}
            A::Drop => {
                // one of whatever there is most of (tools stay in hand)
                let pick = self.me_agent().and_then(|m| {
                    alaz_sim::ItemKind::ALL.iter().copied().filter(|k| !k.is_tool() && m.inventory.get(*k) > 0).max_by_key(|k| m.inventory.get(*k))
                });
                match pick {
                    Some(item) => self.issue(CommandKind::Drop { item, n: 1 }),
                    None => self.say("Bırakacak bir şey yok.".to_string()),
                }
            }
            A::Mute => {
                if let Some(a) = self.audio.as_mut() {
                    a.toggle_mute();
                    let m = a.muted;
                    self.say(if m { "Ses kapalı." } else { "Ses açık." });
                }
            }
            A::Save => self.save_game(true),
            A::Load => {
                if !matches!(self.net, Net::Solo) {
                    self.say("Coop sırasında kayıt yüklenemez.");
                } else if self.load_game() {
                    self.say("Kayıt yüklendi.");
                } else {
                    self.say("Yüklenecek kayıt yok.");
                }
            }
            A::Primary => {
                if let Some(v) = self.world.craft_view(self.me) {
                    // during the mouse-played steps Space only brings the camera back; a jump would throw the work away
                    if v.kind == alaz_sim::MinigameKind::Timing {
                        self.issue(CommandKind::Strike);
                    } else {
                        self.follow = true;
                    }
                } else if !self.in_cave() {
                    self.follow = true;
                    self.issue(CommandKind::Jump);
                }
            }
            A::Follow => {
                self.follow = !self.follow;
                self.say(if self.follow { "Kamera karakteri takip ediyor." } else { "Kamera serbest." });
            }
            A::Stop => {
                if let Some((x, y)) = self.me_pos() {
                    self.issue(CommandKind::MoveTo { x, y });
                }
            }
            A::Bag => self.toggle_tab(hud::Tab::Bag),
            A::Craft => self.toggle_tab(hud::Tab::Craft),
            A::Build => self.toggle_tab(hud::Tab::Build),
            A::Goals => self.toggle_tab(hud::Tab::Goals),
            A::Journal => self.knap.journal.toggle_book(),
            A::ItemGuide => {
                let item = self.gfx.as_ref().and_then(|g| inv::hovered_item(g.renderer.egui_context()));
                self.knap.journal.open_reference(item.map_or(item_book::Page::Index, item_book::Page::Item));
            }
            // while placing, R turns the building; otherwise it opens the routes
            A::Routes if self.build_mode.is_some() => self.facing = (self.facing + 1) & 3,
            A::Routes => self.toggle_tab(hud::Tab::Routes),
            A::Creative => {
                if self.world.settings.difficulty == alaz_sim::world::Difficulty::Creative {
                    self.toggle_tab(hud::Tab::Creative);
                }
            }
            A::Help => self.toggle_tab(hud::Tab::Help),
            A::Sprint | A::MoveUp | A::MoveDown | A::MoveLeft | A::MoveRight | A::CamUp | A::CamDown | A::CamLeft | A::CamRight | A::RotLeft | A::RotRight => {}
        }
    }

    fn menu_action(&mut self, a: menu::MenuAction) {
        use menu::MenuAction as M;
        match a {
            M::Resume => self.menu = None,
            M::Quit => {
                self.quit = true;
            }
            M::SaveQuit => {
                self.save_game(false);
                self.quit = true;
            }
            M::Delete(n) => {
                saveio::delete_in(&saveio::slot_dir(n));
                if let Some(m) = self.menu.as_mut() {
                    m.refresh();
                }
            }
            M::Continue(n) => {
                if let Err(e) = self.start_slot(n)
                    && let Some(m) = self.menu.as_mut()
                {
                    m.error = Some(e);
                }
            }
            M::New { slot, seed, size, hardcore, settings } => self.start_new(menu::NewWorld { slot, seed, size, hardcore, settings }),
            M::Host { slot, port, new } => {
                // bind first: a taken port must leave the current world and the menu as they are
                let mut host = match Host::bind(("0.0.0.0", port)) {
                    Ok(h) => h,
                    Err(e) => {
                        self.mp_error(format!("Sunucu açılamadı (port {port}): {e}"));
                        return;
                    }
                };
                match new {
                    Some(nw) => self.start_new(nw),
                    None => {
                        if let Err(e) = self.start_slot(slot) {
                            self.mp_error(e);
                            return;
                        }
                    }
                }
                if let Err(hint) = host_discovery(&mut host, slot, &self.world) {
                    self.say(hint);
                }
                self.net = Net::Host(host);
                self.say(format!("Sunucu açık: {}:{port}", lan_ip_text()));
            }
            M::HostOnline { slot, port: _, new } => {
                let started = !matches!(self.net, Net::Host(_));
                if started {
                    // online-only: the code is the only way in, so no LAN port and no discovery beacon
                    let host = match Host::bind_online_only() {
                        Ok(h) => h,
                        Err(e) => {
                            self.mp_error(format!("Sunucu açılamadı: {e}"));
                            return;
                        }
                    };
                    match new {
                        Some(nw) => self.start_new(nw),
                        None => {
                            if let Err(e) = self.start_slot(slot) {
                                self.mp_error(e);
                                return;
                            }
                        }
                    }
                    self.net = Net::Host(host);
                }
                if let Net::Host(h) = &mut self.net {
                    if let Err(e) = h.open_relay(&alaz_net::relay_address()) {
                        self.say(format!("Ağ açılamadı: {e}"));
                    } else if started {
                        // the code shows in the pause card; a fresh host would otherwise play on without seeing it
                        self.menu = Some(menu::Menu::new(true));
                    }
                }
            }
            M::CloseOnline => {
                if let Net::Host(h) = &mut self.net {
                    h.relay_close();
                }
            }
            M::NewCode => {
                if let Net::Host(h) = &mut self.net {
                    h.relay_new_code();
                }
            }
            M::JoinCode(text) => {
                if self.connecting.is_none() {
                    self.connecting = Some(("çevrimiçi oyun".into(), connect_code_in_background(text)));
                    if let Some(m) = self.menu.as_mut() {
                        m.connecting = true;
                        m.mp_error = None;
                    }
                }
            }
            M::Join(text) => {
                if self.connecting.is_none() {
                    let addr = normalize_addr(&text);
                    self.connecting = Some((addr.clone(), connect_in_background(addr)));
                    if let Some(m) = self.menu.as_mut() {
                        m.connecting = true;
                        m.mp_error = None;
                    }
                }
            }
        }
    }

    fn mp_error(&mut self, e: String) {
        if let Some(m) = self.menu.as_mut() {
            m.mp_error = Some(e);
        }
    }

    /// Open save slot `n` and play it. An empty slot or a broken save is an error and changes nothing.
    fn start_slot(&mut self, n: u8) -> Result<(), String> {
        if self.menu.as_ref().is_some_and(|m| m.paused) {
            self.save_game(false); // keep the current game before swapping
        }
        let player = AgentId(0);
        match saveio::load_in(&saveio::slot_dir(n), player) {
            Ok(Some(save)) => {
                saveio::set_slot(n);
                self.me = player;
                self.restore_saved_world(save);
                self.no_save = false;
                self.menu = None;
                self.fx.enter_world(&self.world);
                self.say(format!("Yuva {n} yüklendi."));
                Ok(())
            }
            Ok(None) => Err(format!("Yuva {n} boş.")),
            Err(e) => Err(format!("Yuva {n} yüklenemedi: {e}")),
        }
    }

    fn start_new(&mut self, nw: menu::NewWorld) {
        if self.menu.as_ref().is_some_and(|m| m.paused) {
            self.save_game(false);
        }
        saveio::set_slot(nw.slot);
        let w = new_world_with(nw.seed.unwrap_or_else(random_seed), nw.hardcore, nw.size, nw.settings);
        self.me = AgentId(0);
        self.replace_world(w);
        self.story.begin(&self.world);
        self.no_save = false;
        self.menu = None;
        self.fx.enter_world(&self.world);
        self.save_game(false);
        self.say("Sol tık: yürü. Sağ tık: topla. Taş, odun, yemiş, dal ve lif topla.");
        self.say("Yap sekmesinde eline iki şey alıp Dene dersen ne olacağını öğrenirsin.");
    }

    /// Menu-side networking, each frame before the menu draws: finish a background join, run the LAN finder
    /// while the multiplayer screen is open, and show the host's address in the pause menu.
    fn menu_net(&mut self) {
        let Some(m) = self.menu.as_mut() else {
            // a world was started from the menu meanwhile: that choice wins over a join still under way
            self.connecting = None;
            self.finder = None;
            self.finder_off = false;
            return;
        };
        if let Some((addr, rx)) = &self.connecting {
            let fail = |why: String| format!("Katılınamadı ({addr}): {why}");
            let done = match rx.try_recv() {
                Ok(Ok(joined)) => Some(Ok(joined)),
                Ok(Err(e)) => Some(Err(fail(e.to_string()))),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(fail("bağlantı kesildi".into()))),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            };
            if let Some(done) = done {
                let (addr, _) = self.connecting.take().unwrap();
                m.connecting = false;
                match done {
                    Ok((client, world)) => {
                        self.me = client.agent;
                        self.net = Net::Client(client);
                        self.replace_world(world);
                        self.no_save = true;
                        self.menu = None;
                        self.finder = None;
                        self.finder_off = false;
                        self.fx.enter_world(&self.world);
                        self.say(format!("{addr} sunucusuna katıldın."));
                        return;
                    }
                    Err(e) => m.mp_error = Some(e),
                }
            }
        }
        if m.mp_screen {
            if self.finder.is_none() && !self.finder_off {
                match alaz_net::discovery::Finder::new() {
                    Ok(f) => self.finder = Some(f),
                    Err(_) => {
                        self.finder_off = true;
                        m.mp_error = Some("Ağda arama kapalı".into());
                    }
                }
            }
            if let Some(f) = self.finder.as_mut() {
                f.poll();
                m.found = f.found();
            }
        } else {
            self.finder = None;
            self.finder_off = false;
        }
        m.net_line = match &self.net {
            Net::Host(h) if !h.lan_open() => Some(format!("Çevrimiçi sunucu · {} oyuncu", h.players())),
            Net::Host(h) => {
                let port = h.local_addr().map_or(alaz_net::DEFAULT_PORT, |a| a.port());
                Some(format!("Sunucu açık: {}:{port} · {} oyuncu", lan_ip_text(), h.players()))
            }
            _ => None,
        };
        m.hosting = matches!(self.net, Net::Host(_));
        m.lan_open = matches!(&self.net, Net::Host(h) if h.lan_open());
        (m.relay, m.relay_players) = match &self.net {
            Net::Host(h) => (Some(h.relay_status()), h.players()),
            _ => (None, 0),
        };
    }

    fn save_game(&mut self, announce: bool) {
        if self.no_save {
            return;
        }
        self.cave_map.observe(&self.world, self.me);
        match saveio::save(&self.world, self.me, self.cave_map.known()) {
            Ok(outcome) => {
                self.last_save_tick = self.world.tick;
                match outcome {
                    saveio::SaveOutcome::WorldOnly(e) => {
                        eprintln!("mağara haritası kaydedilemedi ({e})");
                        self.say("Dünya kaydedildi. Mağara haritası kaydedilemedi.");
                    }
                    saveio::SaveOutcome::Complete if announce => self.say("Kaydedildi."),
                    saveio::SaveOutcome::Complete => {}
                }
            }
            Err(e) => self.say(format!("Kaydedilemedi: {e}")),
        }
    }

    fn load_game(&mut self) -> bool {
        match saveio::load(self.me) {
            Ok(Some(save)) => {
                self.restore_saved_world(save);
                true
            }
            Ok(None) => false,
            Err(e) => {
                self.say(format!("Kayıt yüklenemedi: {e}"));
                false
            }
        }
    }

    /// Death rules: hardcore wipes the save and starts a new world, otherwise resume the last save.
    fn handle_death(&mut self) {
        if self.world.hardcore {
            saveio::delete();
            let seed = self.world.seed.wrapping_add(1);
            let size = self.world.map.w;
            // a hardcore death starts over with the same settings
            let settings = self.world.settings;
            self.replace_world(new_world_with(seed, true, size, settings));
            self.say("Öldün. Hardcore: dünya silindi, yepyeni bir dünyada başlıyorsun.");
        } else if self.load_game() {
            self.say("Öldün. Son kayıttan devam ediyorsun.");
        } else {
            self.say("Öldün. Kayıt yoktu, her şeyi bırakıp yeniden başladın.");
        }
    }

    fn frame(&mut self) {
        let now = Instant::now();
        let raw_dt = (now - self.last).as_secs_f32();
        let dt = (now - self.last).as_secs_f32().min(0.25);
        self.fx.update(dt);
        // a building ordered turned: once it stands, turn it
        if let Some((x, y, q)) = self.turn_after
            && let Some(i) = self.world.buildings.iter().position(|b| b.x == x && b.y == y) {
                self.turn_after = None;
                for _ in 0..q {
                    self.issue(CommandKind::Turn { building: i as u32 });
                }
            }
        self.last = now;
        if let Some(g) = self.gfx.as_ref() {
            self.cam.win = g.renderer.size();
        }
        self.pilot_tick(raw_dt);

        let bench_pose = self.perf.as_ref().and_then(|p| p.bench_secs.map(|_| p.t0.elapsed().as_secs_f32()));
        if let Some(t) = bench_pose {
            let half = self.world.map.w as f32 / 2.0 - 40.0;
            let amp = half.min(3000.0);
            let c = (self.world.map.w as f32 / 2.0, self.world.map.h as f32 / 2.0);
            self.cam.target.x = c.0 + amp * (t * 0.045).sin();
            self.cam.target.z = c.1 + amp * (t * 0.031).cos();
            self.cam.yaw_deg = 45.0 + t * 40.0;
            self.cam.zoom = 1.0 + 0.9 * (t * 0.7).sin();
            self.pitch_goal = if (t as i32 / 8) % 2 == 0 { Camera::PITCH_ISO } else { Camera::PITCH_TOP };
        }
        // crafting happens in the world: close in on the worker, and step back afterwards
        let crafting = self.world.craft_view(self.me).is_some();
        if crafting {
            let before = *self.zoom_before.get_or_insert(self.cam.zoom);
            let goal = before.max(1.7);
            self.cam.zoom += (goal - self.cam.zoom) * (1.0 - (-6.0 * dt).exp());
            self.follow = true;
        } else if let Some(z) = self.zoom_before {
            self.cam.zoom += (z - self.cam.zoom) * (1.0 - (-6.0 * dt).exp());
            if (self.cam.zoom - z).abs() < 0.01 {
                self.cam.zoom = z;
                self.zoom_before = None;
            }
        }
        // camera input (render-side only)
        let speed = 14.0 * dt / self.cam.zoom;
        let held = |a| self.binds.held(&self.keys, a) as i32 as f32;
        let (pr, pf) = (held(keys::Action::CamRight) - held(keys::Action::CamLeft), held(keys::Action::CamUp) - held(keys::Action::CamDown));
        let rot = held(keys::Action::RotRight) - held(keys::Action::RotLeft);
        if self.menu.is_none() {
            if pr != 0.0 || pf != 0.0 {
                self.follow = false;
                self.cam.pan(pr * speed, pf * speed);
            }
            self.cam.yaw_deg += rot * 90.0 * dt;
        }
        // walking with the keyboard: head the way the keys point, relative to the camera, a few metres at a time
        let (wr, wf) = (held(keys::Action::MoveRight) - held(keys::Action::MoveLeft), held(keys::Action::MoveUp) - held(keys::Action::MoveDown));
        let sprint_held = held(keys::Action::Sprint) > 0.5;
        // running while the sprint key is held (the sim stops it when the breath runs out)
        let want_sprint = self.menu.is_none() && !crafting && sprint_held;
        let sim_sprint = self.me_agent().is_some_and(|a| a.sprint);
        if want_sprint != self.sprinting || (!want_sprint && sim_sprint) || (want_sprint && !sim_sprint && self.sprinting && self.me_agent().is_some_and(|a| a.stamina >= alaz_sim::world::SPRINT_MIN_START)) {
            self.sprinting = want_sprint;
            self.issue(CommandKind::SetSprint { on: want_sprint });
        }
        let walk_ok = self.menu.is_none() && !crafting;
        self.walk_cool -= dt;
        if walk_ok && (wr != 0.0 || wf != 0.0) {
            if self.walk_cool <= 0.0
                && let Some((ax, ay)) = self.me_pos() {
                    let y = self.cam.yaw_deg.to_radians();
                    let (dx, dz) = (wr * y.cos() - wf * y.sin(), -wr * y.sin() - wf * y.cos());
                    let len = (dx * dx + dz * dz).sqrt().max(1e-3);
                    let reach = 4000.0;
                    let (tx, ty) = (ax + (dx / len * reach) as i64, ay + (dz / len * reach) as i64);
                    // straight where the keys push, over an edge if one is in the way
                    self.issue(CommandKind::Walk { x: tx, y: ty });
                    self.walk_cool = 0.2;
                }
            self.walking = true;
            self.follow = true;
        } else if self.walking {
            self.walking = false;
            // letting go of the keys stops the walk, but must not cancel a craft or a cave that just began
            let busy = crafting;
            if let Some((x, y)) = self.me_pos().filter(|_| !busy) {
                self.issue(CommandKind::MoveTo { x, y });
            }
        }
        let player = self.me_agent().map(|a| self.world.agent_walk_position(a));
        if self.follow && bench_pose.is_none() {
            // follow where the body was drawn (between ticks), not the tick's position: at 144 Hz the tick's 60 Hz
            // steps would shake the whole picture against the body
            let drawn = self.gfx.as_ref().and_then(|g| g.renderer.drawn_pos(self.me)).filter(|_| self.menu.is_none());
            if let Some((x, y)) = drawn.or_else(|| self.me_pos().map(|(x, y)| (x as f32 / 1000.0, y as f32 / 1000.0))) {
                let k = 1.0 - (-7.0 * dt).exp();
                self.cam.target.x += (x - self.cam.target.x) * k;
                self.cam.target.z += (y - self.cam.target.z) * k;
            }
        }
        // Follow the same ramp as the walker without changing camera modes at the entrance.
        let walk_height = player.filter(|_| self.follow || self.in_cave()).map(|p| p.z_mm as f32 / 1000.0);
        let gh = walk_height.unwrap_or_else(|| alaz_render::ground_height(self.world.seed, self.cam.target.x, self.cam.target.z));
        self.cam.target.y += (gh - self.cam.target.y) * (1.0 - (-8.0 * dt).exp());
        let d = self.pitch_goal - self.cam.pitch_deg;
        self.cam.pitch_deg += d * (1.0 - (-10.0 * dt).exp());

        self.menu_net();
        let sim_t0 = Instant::now();
        // fixed-step simulation
        if let Net::Client(client) = &mut self.net {
            // the host owns the clock: apply whatever turns arrived, catching up when behind
            let previous_generation = client.world_generation();
            client.poll();
            if let Some(w) = client.take_fresh_world() {
                self.me = client.agent;
                if client.world_generation() != previous_generation {
                    self.pending.clear();
                    self.cave_map = cave_map::CaveMap::default();
                }
                self.world = w;
                self.recording = false;
                self.recorded.clear();
                // indexes into the old copy may point at something else now
                self.selected = None;
                self.build_mode = None;
                self.prev_hp = None;
                if let Some(g) = self.gfx.as_mut() {
                    g.renderer.reset_scene();
                }
                self.hints.push(("Dünya sunucuyla yeniden eşitlendi.".into(), hud::HINT_SECONDS));
            }
            let budget = if client.backlog() > 30 { 60 } else { 6 };
            for _ in 0..budget {
                let Some(turn) = client.next_turn() else { break };
                if !apply_turn(&mut self.world, &turn) {
                    client.request_resync();
                    break;
                }
            }
            client.send(std::mem::take(&mut self.pending));
            if !client.is_connected() {
                self.net = Net::Solo;
                self.say("Sunucu bağlantısı koptu. Bu kopya yerel kalır, kaydedilmez.");
            }
        } else if self.menu.is_some() && matches!(self.net, Net::Solo) {
            self.accum = 0.0;
            self.pending.clear();
        } else {
            let scale = self.sim_scale();
            self.accum += dt * scale as f32;
            let mut steps = 0;
            while self.accum >= STEP && steps < 5 * scale {
                let mut local = std::mem::take(&mut self.pending);
                // a debug script plays as this player, one tick at a time
                if let Some(sc) = self.script.as_mut() {
                    local.extend(sc.tick(&mut self.world, self.me));
                    for s in sc.shots.drain(..) {
                        eprintln!("SHOT {s}");
                    }
                    for l in sc.log.drain(..) {
                        eprintln!("SCRIPT {l}");
                    }
                }
                let cmds = match &mut self.net {
                    Net::Host(host) => {
                        host.poll(&self.world);
                        host.turn(&self.world, local)
                    }
                    _ => local,
                };
                let seen_before = self.world.events.len();
                self.world.step(&cmds);
                if let Some(sc) = self.script.as_mut() {
                    let evs = self.world.events[seen_before.min(self.world.events.len())..].to_vec();
                    sc.watch(&self.world, self.me, &evs);
                    if sc.done() && !self.script_done {
                        self.script_done = true;
                        eprintln!("SCRIPT done");
                    }
                }
                self.accum -= STEP;
                steps += 1;
            }
            if steps == 5 * scale {
                self.accum = 0.0;
            }
        }
        // drawing sits exactly this far between the last tick and the next: steady motion whatever the frame rate
        let exact = matches!(self.net, Net::Solo | Net::Host(_)) && self.menu.is_none();
        if let Some(g) = self.gfx.as_mut() {
            g.renderer.sim_alpha = exact.then(|| (self.accum / STEP).clamp(0.0, 1.0));
        }

        let sim_ms = sim_t0.elapsed().as_secs_f32() * 1000.0;
        // hints: sim events and need warnings
        let pm = self.world.perception_permille();
        let stage = alaz_render::evo_stage(pm) as u8;
        if self.last_stage != 255 && stage > self.last_stage {
            // a new named stage: say what changed, and when it is also a step in what the hands can do, that too
            let mut line = format!("Evrildin: {}. {}", hud::EVO_NAMES[stage as usize], hud::evo_look(stage));
            if alaz_sim::PERCEPTION_STEPS.contains(&alaz_render::EVO_STARTS[stage as usize]) {
                line += &format!(" {}", hud::stage_gift(self.world.perception_level()));
            }
            self.say(line);
            self.fx.evolved(hud::EVO_NAMES[stage as usize]);
        }
        self.last_stage = stage;
        // the body follows the understanding with a lag of a few seconds
        let goal = pm as f32;
        let view = self.evo_view.map_or(goal, |v| glide_evo(v, goal, dt));
        self.evo_view = Some(view);
        self.evo_pos = self.evo_force.unwrap_or_else(|| alaz_render::evo_pos(view));
        if let Some(g) = self.gfx.as_mut() {
            g.renderer.evo = Some(self.evo_pos);
        }
        let events = self.world.take_events();
        let me = self.me;
        if (!self.no_save || std::env::var_os("ALAZ_PLAYLOG").is_some()) && matches!(self.net, Net::Solo) {
            for e in &events {
                let mine = match e {
                    alaz_sim::Event::Struck { agent, .. } | alaz_sim::Event::Crafted { agent, .. } | alaz_sim::Event::Rejected(agent, _) | alaz_sim::Event::Bitten(agent) => *agent == me,
                    alaz_sim::Event::Died(agent) => *agent == me,
                    alaz_sim::Event::Discovered(_) | alaz_sim::Event::Milestone(_) => true,
                    _ => false,
                };
                if mine {
                    saveio::log_play(&format!("{}\t{:?}", self.world.tick, e));
                }
            }
        }
        if let Some(g) = self.gfx.as_mut() {
            for e in &events {
                if let alaz_sim::Event::Killed { wild, species, x, y } = e {
                    g.renderer.wild_killed(*wild, *species, *x, *y);
                }
            }
        }
        let died_me = events.iter().any(|e| matches!(e, alaz_sim::Event::Died(id) if *id == me));
        if died_me && matches!(self.net, Net::Solo) && !self.no_save {
            self.handle_death();
        } else {
            let coop = !matches!(self.net, Net::Solo);
            let texts: Vec<String> = events
                .iter()
                .filter(|e| match e {
                    alaz_sim::Event::Rejected(a, _) | alaz_sim::Event::Hunted { agent: a, .. } => *a == me,
                    _ => true,
                })
                .filter_map(|e| match e {
                    alaz_sim::Event::Died(id) if coop => Some(if *id == me {
                        "Dayanamadın. Başlangıç noktasında, eşyasız yeniden doğdun.".to_string()
                    } else {
                        format!("{}. oyuncu düştü ve başlangıçta yeniden doğdu.", id.0 + 1)
                    }),
                    _ => hud::event_hint(e, &self.world, me),
                })
                .collect();
            for t in texts {
                self.say(t);
            }
        }
        let in_danger = self.me_agent().is_some_and(|a| a.hunger < 2000 || a.thirst < 2000 || a.health < 3000 || a.warmth < 2000);
        if self.world.tick.saturating_sub(self.last_save_tick) >= self.autosave_ticks && !in_danger {
            self.save_game(false);
        }
        if let Some(a) = self.me_agent() {
            let checks = [
                (a.thirst, "Çok susadın. Bir su kenarına git ve içmeyi dene."),
                (a.hunger, "Karnın acıkıyor. Yemiş ara ve ye."),
                (a.warmth, "Üşüyorsun. Odun toplayıp ateş yak ya da güneşi bekle."),
            ];
            for (i, (v, msg)) in checks.iter().enumerate() {
                let limit = if i == 2 { 4500 } else { 3500 };
                if *v < limit && !self.warned[i] {
                    self.warned[i] = true;
                    self.hints.push((msg.to_string(), hud::HINT_SECONDS));
                } else if *v > limit + 2500 {
                    self.warned[i] = false;
                }
            }
        }
        // the first time the ground turns to mud, ice or flames underfoot (or beside it), say what it does
        if self.world.tick.is_multiple_of(15) && self.surf_hinted != 0b11_1111
            && let Some(a) = self.me_agent() {
                use alaz_sim::Surface;
                let (tx, ty) = (a.x.div_euclid(1000) as i32, a.y.div_euclid(1000) as i32);
                let here = self.world.surface_at(tx, ty);
                let fire_near = (-2..=2).any(|dy| (-2..=2).any(|dx| self.world.surface_at(tx + dx, ty + dy) == Surface::Fire));
                let (which, text) = if fire_near && self.surf_hinted & 1 << Surface::Fire as u8 == 0 {
                    (Surface::Fire, "Ot yanıyor! Alev kuru otları yalayıp rüzgârla yayılır, canını yakar. Uzak dur; ıslak toprak ve yağmur durdurur.")
                } else if here == Surface::Mud {
                    (Surface::Mud, "Yağmur zemini çamura çevirdi: adımların ağırlaştı. Tabanı tutan çizme çamurda daha az yavaşlatır; güneş kuruturur.")
                } else if here == Surface::Ice {
                    (Surface::Ice, "Su birikintileri buz tuttu. Buzda koşarsan kayıp düşersin: yürü, ya da tabanı tutan çizme giy.")
                } else if here == Surface::Puddle {
                    (Surface::Puddle, "Su birikintisi: üstünde ateş tutuşmaz, içinden yürümek biraz yavaşlatır.")
                } else {
                    (Surface::Dry, "")
                };
                if which != Surface::Dry && self.surf_hinted & 1 << which as u8 == 0 {
                    self.surf_hinted |= 1 << which as u8;
                    self.say(text);
                }
            }
        if !self.wolf_hinted
            && let Some(a) = self.me_agent() {
                let wolf_near = self
                    .world
                    .wild
                    .iter()
                    .any(|d| d.species == alaz_sim::WildSpecies::Wolf && dist2(d.x, d.y, a.x, a.y) < 25_000 * 25_000);
                if wolf_near {
                    self.wolf_hinted = true;
                    self.say("Yakınlarda bir kurt var! Ateşin yanında güvendesin. Mızrağın varsa ona sağ tıkla.");
                }
            }
        if !self.wild_hinted
            && let Some(a) = self.me_agent()
                && self.world.wild_at(a.x, a.y, 12_000).is_some() {
                    self.wild_hinted = true;
                    self.say("Yabani bir hayvan var. Yanında yemişin varsa ona sağ tıkla: güvenini kazanabilirsin.");
                }
        for e in &events {
            if let alaz_sim::Event::Struck { agent, hit } = e
                && *agent == self.me {
                    self.last_strike = Some((*hit, 0.35));
                }
            if let Some(a) = &self.audio {
                use alaz_sim::Event as E;
                match e {
                    E::Struck { agent, hit } if *agent == self.me => a.play(if *hit { audio::Sfx::Hit } else { audio::Sfx::Miss }),
                    E::Crafted { agent, quality, .. } if *agent == self.me => {
                        a.play(match quality {
                            2 => audio::Sfx::Chime,
                            1 => audio::Sfx::Ding,
                            _ => audio::Sfx::Fail,
                        })
                    }
                    E::Discovered(_) | E::GoalDone(_) => a.play(audio::Sfx::Chime),
                    E::Milestone(alaz_sim::Milestone::Visitor) => a.play(audio::Sfx::Bark),
                    E::Milestone(_) => a.play(audio::Sfx::Fanfare),
                    E::Died(id) if *id == self.me => a.play(audio::Sfx::Hurt),
                    E::Tamed(_) | E::WorkerReady(_) => a.play(audio::Sfx::Tame),
                    E::JobDone { .. } => a.play(audio::Sfx::Ding),
                    E::Hunted { agent, .. } if *agent == self.me => a.play(audio::Sfx::Hit),
                    E::Fell { agent, .. } if *agent == self.me => a.play(audio::Sfx::Thud),
                    E::TreeDown { x, y, .. } if self.near_me(*x, *y, 30_000) => a.play(audio::Sfx::Timber),
                    E::Chopped { agent } if *agent == self.me => a.play(audio::Sfx::Chop),
                    E::RockBounce { x, y } | E::Rockfall { x, y } if self.near_me(*x, *y, 20_000) => a.play(audio::Sfx::Rumble),
                    _ => {}
                }
            }
        }
        if let Some((_, t)) = self.last_strike.as_mut() {
            *t -= dt;
        }
        if self.last_strike.is_some_and(|(_, t)| t <= 0.0) {
            self.last_strike = None;
        }
        for h in &mut self.hints {
            h.1 -= dt;
        }
        self.hints.retain(|h| h.1 > 0.0);

        // dev aid: keep the demo drinker thirsty, so the drinking goes on
        if std::env::var("ALAZ_DEMO_WORK").as_deref() == Ok("drink")
            && let Some(a) = self.world.agents.get_mut(self.me.0 as usize) {
                a.thirst = a.thirst.min(2_000);
            }
        // dev aid: hold the walk keys, as a player would ("dx,dy" in metres: the way to keep walking)
        if let Some(d) = std::env::var("ALAZ_DEMO_WALK").ok().map(|v| v.split(',').filter_map(|n| n.trim().parse::<i64>().ok()).collect::<Vec<_>>())
            && d.len() == 2 && self.world.tick.is_multiple_of(12)
                && let Some((ax, ay)) = self.me_agent().map(|a| (a.x, a.y)) {
                    let len = ((d[0] * d[0] + d[1] * d[1]) as f64).sqrt().max(1.0);
                    self.issue(CommandKind::Walk { x: ax + (d[0] as f64 / len * 4_000.0) as i64, y: ay + (d[1] as f64 / len * 4_000.0) as i64 });
                }
        // dev aid: gravity at work around the player, over and over, to look at it
        if let Ok(v) = std::env::var("ALAZ_DEMO_PHYS") {
            self.demo_phys(&v);
        }
        // dev aid: a blow a few seconds in, to look at the hurt effects
        if std::env::var_os("ALAZ_DEMO_HURT").is_some() && self.demo_hurt_t < 4.0 {
            self.demo_hurt_t += dt;
            if self.demo_hurt_t >= 4.0
                && let Some(a) = self.world.agents.get_mut(self.me.0 as usize) {
                    a.health -= 1500;
                }
        }
        // getting hurt: any real drop in health (a bite, a fall, a blow), not the slow drain of hunger or cold
        let hp = self.me_agent().map(|a| a.health);
        if let (Some(now_hp), Some(before)) = (hp, self.prev_hp)
            && before - now_hp >= 150 {
                self.hurt_flash = 1.0;
                if let Some(a) = &self.audio {
                    a.play(audio::Sfx::Hurt);
                }
            }
        self.prev_hp = hp;
        self.hurt_flash = (self.hurt_flash - dt * 1.8).max(0.0);
        self.follow_machine();
        // the ambience follows the player around a few times a second
        self.scene_t -= dt;
        if self.scene_t <= 0.0 {
            self.scene_t = 0.15;
            if let (Some(a), Some(g), Some((ax, ay))) = (&self.audio, self.gfx.as_ref(), self.me_agent().map(|a| (a.x, a.y))) {
                a.set_scene(self.ambience_at(ax, ay, g.renderer.weather_now()));
            }
        }

        let ghost_mm = self.ghost_pos();
        let ghost = self.build_mode.zip(ghost_mm).map(|(kind, (x, y))| Ghost {
            x_m: x as f32 / 1000.0,
            z_m: y as f32 / 1000.0,
            kind,
            valid: self.placement_ok(kind, x, y),
            facing: self.facing,
        });
        // what the pointer is on gets a bright rim, so it is plain what a right click would take
        let pointer_active = self.menu.is_none() && self.build_mode.is_none() && self.world.craft_view(self.me).is_none()
            && self.gfx.as_ref().is_some_and(|g| !g.renderer.pointer_over_ui());
        let target = pointer_active.then(|| self.pointer_target()).flatten();
        let surface = target.and_then(|(ray, target)| match target {
            PointerGround::Surface(ground) => Some((ray, ground)),
            PointerGround::Cave(_) => None,
        });
        let picked = surface.and_then(|(ray, ground)| self.pick_node(ray, ground));
        let hover = picked.and_then(|id| self.world.nodes.get(id.0 as usize)).map(|n| (n.tx, n.ty));
        // dev aid for screenshots: rim the nearest tree when the pointer is not on anything
        let hover = hover.or_else(|| {
            std::env::var_os("ALAZ_DEMO_HOVER")?;
            let me = self.me_agent()?;
            self.world.nodes.iter().filter(|n| n.kind == alaz_sim::NodeKind::Tree).min_by_key(|n| dist2(me.x, me.y, n.center_mm().0, n.center_mm().1)).map(|n| (n.tx, n.ty))
        });
        let pointer_text = if pointer_active {
            self.pointer_text(target, picked)
        } else {
            // placing a building on a spot that will not do: say why, instead of only turning the ghost red
            self.build_mode.zip(ghost_mm).filter(|&(kind, (x, y))| !self.placement_ok(kind, x, y)).and_then(|(kind, (x, y))| self.placement_why(kind, x, y))
        };
        let Some(g) = self.gfx.as_mut() else { return };
        g.renderer.hover_node = hover;
        let (world, hints) = (&self.world, &self.hints);
        self.cave_map.observe(world, self.me);
        let net_status = match &self.net {
            Net::Solo => String::new(),
            Net::Host(h) => format!("Coop: sunucu, {} oyuncu", h.players()),
            Net::Client(c) if c.resyncing() => "Coop: eşitleniyor...".to_string(),
            Net::Client(_) => "Coop: bağlı".to_string(),
        };
        let world_ref_anchor = world.agents.get(self.me.0 as usize).map(|a| {
            let ground = world.agent_walk_position(a);
            let p = Vec3::new(ground.x_mm as f32 / 1000.0, ground.z_mm as f32 / 1000.0 + 1.9, ground.y_mm as f32 / 1000.0);
            self.cam.project(g.renderer.aspect(), p)
        });
        let ui_ctx = UiCtx {
            me: self.me,
            last_strike: self.last_strike,
            tab: self.tab,
            binds: &self.binds,
            net_status: &net_status,
            hints,
            build_mode: self.build_mode,
            grid_snap: self.grid_snap,
            selection: self.selected,
            recording: self.recording,
            recorded: &self.recorded,
            anchor: world_ref_anchor,
            grip: self.binds.held(&self.keys, keys::Action::Grip),
            time_scale: self.time_scale,
            evo: self.evo_pos,
            yaw: self.cam.yaw_deg,
            cave_tiles: Some(self.cave_map.known()),
        };
        let mut acts = Vec::new();
        let mut knap = std::mem::take(&mut self.knap);
        let mut story = std::mem::take(&mut self.story);
        let mut fx = std::mem::take(&mut self.fx);
        let me_id = self.me;
        let mut menu = self.menu.take();
        let mut menu_act = None;
        let mut binds = self.binds.clone();
        let mut route_of = vec![self.me.0 as usize];
        if let Some(Selection::Agent(i)) = self.selected {
            route_of.push(i);
        }
        let render_t0 = Instant::now();
        // a blow shakes the view for a moment and reddens the edges of the screen
        let flash = self.hurt_flash;
        let mut cam = self.cam;
        if flash > 0.0 {
            let t = render_t0.elapsed().as_secs_f32() + self.world.tick as f32 * 0.37;
            let k = flash * flash * 0.18;
            cam.target.x += (t * 61.0).sin() * k;
            cam.target.z += (t * 47.0).cos() * k;
        }
        // the swoop into a new world is only how it is drawn: the game's own camera is left alone
        fx.camera(&mut cam);
        // graphics settings chosen in the panels reach the renderer here (dev aid: ALAZ_DEMO_GFX_AT=n ALAZ_DEMO_GFX=pixels=coarse,shadows=low
        // changes them on frame n, to check a change while playing)
        {
            static FRAMES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let n = FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if std::env::var("ALAZ_DEMO_GFX_AT").ok().and_then(|v| v.parse::<u64>().ok()) == Some(n) {
                let mut q = gfxcfg::current();
                for kv in std::env::var("ALAZ_DEMO_GFX").unwrap_or_default().split(',') {
                    match kv.split_once('=') {
                        Some(("pixels", v)) => q.pixels = alaz_render::PixelDetail::parse(v).unwrap_or(q.pixels),
                        Some(("shadows", v)) => q.shadows = alaz_render::ShadowQuality::parse(v).unwrap_or(q.shadows),
                        Some(("decor", v)) => q.decor = v != "0",
                        _ => {}
                    }
                }
                gfxcfg::set(q);
            }
            if let Some(q) = gfxcfg::take_change() {
                g.renderer.set_quality(q);
            }
        }
        g.renderer.render(world, &cam, ghost, self.me, &route_of, |ctx| {
            if flash > 0.0 {
                hud::hurt_vignette(ctx, flash);
            }
            if let Some(k) = view_item() {
                // the preview of an item icon: the file's picture if there is one, else the painted icon, big
                alaz_render::egui::Area::new(alaz_render::egui::Id::NULL.with("view_icon")).anchor(alaz_render::egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                    let (rect, _) = ui.allocate_exact_size(alaz_render::egui::vec2(256.0, 256.0), alaz_render::egui::Sense::hover());
                    icons::draw_slot(ui.painter(), rect, k, true);
                });
            }
            if menu.is_none() {
                let (sun, k) = alaz_render::sun_of(world.tick, 0.0, world.settings.season_days as u64);
                hud::compass(ctx, cam.yaw_deg, cam.pitch_deg, sun, k > 0.3);
            }
            match menu.as_mut() {
                Some(m) => menu_act = m.draw(ctx, &mut binds),
                None => {
                    hud::draw(ctx, world, &ui_ctx, &mut knap, &mut acts);
                    // the story's first page waits until the way in has played
                    if let Some(me_a) = world.agents.get(me_id.0 as usize).filter(|_| !fx.entering()) {
                        story.draw(ctx, world, me_a, &binds, ui_ctx.selection.is_some());
                    }
                    if let Some(t) = &pointer_text {
                        hud::pointer_label(ctx, t);
                    }
                }
            }
            fx.draw(ctx);
        });
        // a click on the title screen only skips it
        if fx.splash_up() {
            menu_act = None;
        }
        self.fx = fx;
        self.menu = menu;
        self.binds = binds;
        let render_ms = render_t0.elapsed().as_secs_f32() * 1000.0;
        self.knap = knap;
        self.story = story;
        let stats = g.renderer.stats();
        if let Some(a) = menu_act {
            self.menu_action(a);
        }
        for act in acts {
            match act {
                UiAction::Cmd(kind) => self.issue(kind),
                UiAction::BeginBuild(k) => {
                    self.build_mode = Some(k);
                    self.facing = 0;
                    self.moving = None;
                    self.selected = None;
                }
                UiAction::ToggleTab(t) => self.toggle_tab(t),
                UiAction::SetTimeScale(s) => self.time_scale = s.clamp(1, 10),
                UiAction::ToggleGrid => self.grid_snap = !self.grid_snap,
                UiAction::MoveBuilding(i) => {
                    if let Some(b) = self.world.buildings.get(i) {
                        self.build_mode = Some(b.kind);
                        self.facing = b.facing;
                        self.moving = Some(i);
                        self.selected = None;
                        self.say("Yeni yeri sol tıkla seç; sağ tık vazgeçer.".to_string());
                    }
                }
                UiAction::Deselect => self.selected = None,
                UiAction::ToggleBook => self.knap.journal.toggle_book(),
                UiAction::CloseWindow => {
                    self.tab = None;
                    self.selected = None;
                }
                UiAction::ToggleRecording => {
                    if !self.recording {
                        self.recorded.clear();
                        self.say("Rota kaydı başladı: topla, yükle, al eylemlerin rotaya eklenir.");
                    } else if self.recorded.is_empty() {
                        self.say("Kayıt boş kaldı.");
                    } else {
                        self.say(format!("Rota kaydedildi ({} adım). Bir işçi seçip ata.", self.recorded.len()));
                    }
                    self.recording = !self.recording;
                }
                UiAction::ClearRecorded => self.recorded.clear(),
                UiAction::RecordWait => self.recorded.push(RouteStep::Wait { ticks: 5 * TICKS_PER_SECOND as u32 }),
                UiAction::NewCycle => {
                    // the loop: only a solo player or the host may start it; joined players follow the host's world
                    if matches!(self.net, Net::Client(_)) {
                        self.say("Döngüyü sunucu başlatır.");
                    } else {
                        let next = self.world.next_cycle();
                        self.save_game(false);
                        self.replace_world(next);
                        self.save_game(false);
                        self.fx.enter_world(&self.world);
                        self.say("Yeni bir dünya. Yine hiçbir şey bilmiyorsun; ama bu dünya senin kurallarınla dönüyor, eski yaptıkların harabe olarak duruyor.");
                    }
                }
                UiAction::AssignRecorded(worker) => {
                    let route = self.recorded.clone();
                    self.issue(CommandKind::SetRoute { worker, route });
                    self.say("Rota atandı.");
                }
            }
        }

        if let Some(p) = self.perf.as_mut() {
            p.frame_dt.push(raw_dt);
            p.render_ms.push(render_ms);
            p.sim_ms.push(sim_ms);
            p.max_chunks = p.max_chunks.max(stats.chunks_drawn);
            p.max_verts = p.max_verts.max(stats.vertices);
            let done = p.bench_secs.is_some_and(|s| p.t0.elapsed().as_secs_f32() >= s);
            if done || p.last_print.elapsed().as_secs_f32() >= 5.0 {
                p.last_print = Instant::now();
                p.report(if done { "summary" } else { "5s" }, stats.chunks_built, stats.build_ms);
            }
            if done {
                std::process::exit(0);
            }
        }
        self.fps_t += dt;
        self.fps_n += 1;
        if self.fps_t >= 0.5 {
            let mode = if self.pitch_goal >= 89.0 { "2D" } else { "2.5D" };
            if let Some(g) = self.gfx.as_ref() {
                g.window.set_title(&format!("A.L.A.Z | {mode} | {:.0} fps", self.fps_n as f32 / self.fps_t));
            }
            self.fps_t = 0.0;
            self.fps_n = 0;
        }
    }

    fn pointer_ray(&self) -> Option<PointerRay> {
        let g = self.gfx.as_ref()?;
        let mut cam = self.cam;
        let (w, h) = g.renderer.size();
        cam.win = (w, h);
        let ndc = (2.0 * self.cursor.0 / w as f32 - 1.0, 1.0 - 2.0 * self.cursor.1 / h as f32);
        PointerRay::from_camera(&cam, ndc)
    }

    /// The node the pointer would pick if it stood at `c` (pixels).
    fn node_under(&self, c: (f32, f32)) -> Option<u32> {
        let g = self.gfx.as_ref()?;
        let mut cam = self.cam;
        let (w, h) = g.renderer.size();
        cam.win = (w, h);
        let ray = PointerRay::from_camera(&cam, (2.0 * c.0 / w as f32 - 1.0, 1.0 - 2.0 * c.1 / h as f32))?;
        match self.pointer_ground_along(ray)? {
            PointerGround::Surface(ground) => self.pick_node(ray, ground).map(|id| id.0),
            PointerGround::Cave(_) => None,
        }
    }

    fn pointer_target(&self) -> Option<(PointerRay, PointerGround)> {
        let ray = self.pointer_ray()?;
        Some((ray, self.pointer_ground_along(ray)?))
    }

    fn pointer_ground_along(&self, ray: PointerRay) -> Option<PointerGround> {
        let renderer = &self.gfx.as_ref()?.renderer;
        let map = &self.world.map;
        let surface = ray.terrain_hit_where(
            |x, y| alaz_sim::elevation_mm(self.world.seed, x, y) as f32 / 1000.0,
            |x, y, p| map.in_bounds(x, y) && !map.surface_cave_gap(x, y) && !renderer.surface_cutaway_contains(p),
        );
        if !alaz_sim::map::has_natural_caves(self.world.seed) {
            return surface.map(PointerGround::Surface);
        }
        let floor = ray.cave_floor_hit(|x, y, z| renderer.has_cave_floor(x, y, z));
        let rock = self.me_agent().and_then(|a| a.cave_view()).and_then(|c| {
            let p = ray.at(f32::from(c.elevation_m()));
            matches!(self.cave_cell_at(p)?.1, alaz_sim::caves::Cell::Rock | alaz_sim::caves::Cell::Rubble).then_some(p)
        });
        let cave = [floor, rock].into_iter().flatten().max_by(|a, b| a.y.total_cmp(&b.y));
        choose_pointer_ground(surface, cave)
    }

    /// Dev aid (ALAZ_DEMO_PHYS): `tree` fells a tree beside the player every few seconds, `rock` drops stones down the
    /// nearest slope, `cliff` walks the player over the nearest cliff edge again and again.
    fn demo_phys(&mut self, what: &str) {
        let me = self.me;
        let Some(a) = self.world.agents.get(me.0 as usize) else { return };
        let (ax, ay) = (a.x, a.y);
        let t = self.world.tick;
        let w = &mut self.world;
        match what {
            "tree" if t.is_multiple_of(420) && !w.bodies.iter().any(|b| b.kind != alaz_sim::physics::BodyKind::Rock) => {
                // a tall tree with open ground to its east: stand west of it and send it over toward the camera's right
                let (px, py) = ((ax / 1000) as i32, (ay / 1000) as i32);
                let open = |w: &alaz_sim::World, tx: i32, ty: i32| (1..=7).all(|k| w.node_at(tx + k, ty).is_none() && w.map.terrain(tx + k, ty).is_some_and(|t| t != alaz_sim::map::Terrain::Water));
                let pick = (0..w.nodes.len())
                    .filter(|&i| w.nodes[i].kind == alaz_sim::NodeKind::Tree && w.nodes[i].amount > 0 && alaz_sim::map::tree_height_mm(w.seed, w.nodes[i].tx, w.nodes[i].ty) > 3_000 && open(w, w.nodes[i].tx, w.nodes[i].ty))
                    .min_by_key(|&i| (w.nodes[i].tx - px).abs() + (w.nodes[i].ty - py).abs());
                if let Some(i) = pick {
                    let (tx, ty) = (w.nodes[i].tx, w.nodes[i].ty);
                    let (bx, by) = alaz_sim::map::node_anchor_mm(w.seed, tx, ty);
                    if let Some(a) = w.agents.get_mut(me.0 as usize) {
                        a.x = bx - 900;
                        a.y = by + 300;
                        a.task = alaz_sim::world::Task::Idle;
                    }
                    w.set_node_amount(i, 0);
                    let log = w.fell_tree(tx, ty, me, bx - 900, by);
                    // and then break it up and gather the pieces, as a cutter does
                    if let Some(a) = w.agents.get_mut(me.0 as usize) {
                        a.task = alaz_sim::world::Task::Split { body: log, progress: 0 };
                        a.inventory.counts[alaz_sim::ItemKind::Wood as usize] = 0;
                    }
                    eprintln!("demo: felled tree at tick {t}");
                }
            }
            // held in mid-air, falling (or rising with `jumpup`), to look at the pose
            "fall" | "jumpup" => {
                let q = alaz_sim::physics::Q;
                let g = alaz_sim::map::elevation_at_mm(w.seed, ax, ay) as i64 * q;
                let vz = if what == "fall" { -60 * q } else { 50 * q };
                w.air.retain(|f| f.agent != me);
                w.air.push(alaz_sim::physics::Air { agent: me, x: ax * q, y: ay * q, z: g + 1_200 * q, vx: 0, vy: 0, vz: vz + alaz_sim::physics::G, top: g + 1_200 * q });
            }
            "rock" if t.is_multiple_of(150) => {
                let (ox, oy) = (ax + 3_000, ay - 2_000);
                w.loose_rock(ox, oy, -6, 5, t);
                if let Some(b) = w.bodies.last_mut() {
                    b.z += 4_000 * alaz_sim::physics::Q;
                }
            }
            "cliff" => {
                let start = *self.demo_cliff.get_or_insert_with(|| {
                    let (px, py) = ((ax / 1000) as i32, (ay / 1000) as i32);
                    let mut best = None;
                    for ty in (py - 80).max(2)..(py + 80).min(w.map.h - 2) {
                        for tx in (px - 80).max(2)..(px + 80).min(w.map.w - 2) {
                            if w.map.terrain(tx, ty) != Some(alaz_sim::map::Terrain::Rock) {
                                continue;
                            }
                            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                                let d = alaz_sim::elevation_mm(w.seed, tx, ty) - alaz_sim::elevation_mm(w.seed, tx + dx, ty + dy);
                                let clear = (-1..=3).all(|k| w.node_at(tx + dx * k, ty + dy * k).is_none());
                                let dd = (tx - px).pow(2) + (ty - py).pow(2);
                                if d > 2_600 && clear && best.is_none_or(|(_, _, _, _, b)| dd < b) {
                                    best = Some((tx, ty, dx, dy, dd));
                                }
                            }
                        }
                    }
                    best.map(|(tx, ty, dx, dy, _)| (tx, ty, dx, dy)).unwrap_or((px, py, 1, 0))
                });
                let (tx, ty, dx, dy) = start;
                if t.is_multiple_of(300) {
                    let (x, y) = (tx as i64 * 1000 - dx as i64 * 1_200, ty as i64 * 1000 - dy as i64 * 1_200);
                    if let Some(a) = w.agents.get_mut(me.0 as usize) {
                        a.x = x;
                        a.y = y;
                        a.health = alaz_sim::world::MAX_STAT;
                        a.broken = 0;
                    }
                    w.air.retain(|f| f.agent != me);
                    self.issue(CommandKind::Walk { x: x + dx as i64 * 6_000, y: y + dy as i64 * 6_000 });
                }
            }
            _ => {}
        }
    }

    /// What a right click at (x, y) mm would do to a body lying there: pick up a piece, or break up a trunk.
    fn body_at(&self, x: i64, y: i64) -> Option<CommandKind> {
        use alaz_sim::physics::{BodyKind, Q};
        let piece = self
            .world
            .bodies
            .iter()
            .filter(|b| b.kind == BodyKind::Piece)
            .map(|b| (dist2(b.x / Q, b.y / Q, x, y), b.id))
            .filter(|(d, _)| *d <= 600 * 600)
            .min();
        if let Some((_, body)) = piece {
            return Some(CommandKind::Pick { body });
        }
        // anywhere along what is left of a lying trunk
        self.world.bodies.iter().filter(|b| b.lying() && b.wood > 0).find_map(|b| {
            let (bx, by, _) = b.pos_mm();
            let (rx, ry) = (x - bx, y - by);
            let t = (rx * b.dx as i64 + ry * b.dy as i64) / 1000;
            let side = ((rx * b.dy as i64 - ry * b.dx as i64) / 1000).abs();
            (t >= -300 && t <= b.trunk_left_mm() + 300 && side <= 500).then_some(CommandKind::Split { body: b.id })
        })
    }

    /// The building whose model the pointer is on, if any (its roof as well as its foot).
    fn pick_building(&self, ray: PointerRay, ground: Vec3) -> Option<usize> {
        self.world.buildings.iter().enumerate().filter_map(|(i, b)| {
            let at = Vec2::new(b.x as f32, b.y as f32) / 1000.0;
            let radius = (self.world.rules().building(b.kind).radius_mm + 300) as f32 / 1000.0;
            let base = ray.model_base(self.world.seed, at, radius, ground.y)?;
            ray.column_hit(at, radius, ground.y.max(base), base + 2.5).map(|y| (i, y))
        }).max_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i)
    }

    /// The worker (dog, ox, horse, bot) the pointer is on, if any.
    fn pick_worker(&self, ray: PointerRay, ground: Vec3) -> Option<usize> {
        self.world.agents.iter().enumerate().filter(|(_, a)| a.kind.is_worker() && a.cave_view().is_none()).filter_map(|(i, a)| {
            let at = Vec2::new(a.x as f32, a.y as f32) / 1000.0;
            let base = ray.model_base(self.world.seed, at, 0.7, ground.y)?;
            ray.column_hit(at, 0.7, ground.y.max(base), base + 2.5).map(|y| (i, y))
        }).max_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i)
    }

    /// What the pointer is on, in words, with what a right click would do: a resource, something lying on the
    /// ground, a building, a person or an animal.
    fn pointer_text(&self, target: Option<(PointerRay, PointerGround)>, picked: Option<alaz_sim::NodeId>) -> Option<String> {
        use alaz_sim::physics::{BodyKind, Q};
        // dev aid for close-up screenshots of the character: no tip beside the pointer
        if std::env::var_os("ALAZ_DEMO_NOTIP").is_some() {
            return None;
        }
        let w = &self.world;
        let (ray, p) = match target? {
            (_, PointerGround::Cave(ground)) => return self.cave_pointer_text(ground),
            (ray, PointerGround::Surface(ground)) => (ray, ground),
        };
        if w.surface_at(p.x.floor() as i32, p.z.floor() as i32) == alaz_sim::Surface::Fire {
            return Some("Yanıyor! Uzak dur".to_string());
        }
        if let Some(id) = picked {
            let n = w.nodes.get(id.0 as usize)?;
            let action = if n.kind == alaz_sim::NodeKind::CaveMouth { "gir" } else { "topla" };
            return Some(format!("{}  ·  sağ tık: {action}", hud::node_label(w, n)));
        }
        let (x, y) = ((p.x * 1000.0) as i64, (p.z * 1000.0) as i64);
        match self.body_at(x, y) {
            Some(CommandKind::Pick { body }) => {
                let b = w.bodies.iter().find(|b| b.id == body)?;
                let item = alaz_sim::ItemKind::ALL.get(b.item as usize).copied().unwrap_or(alaz_sim::ItemKind::Wood);
                let near = w.bodies.iter().filter(|o| o.kind == BodyKind::Piece && dist2(o.x / Q, o.y / Q, b.x / Q, b.y / Q) <= 2_000 * 2_000).count();
                let more = if near > 1 { format!(" (yakında {near} parça)") } else { String::new() };
                return Some(format!("Yerde: {}{more}  ·  sağ tık: topla", crate::lang::item_word(w, item)));
            }
            Some(CommandKind::Split { .. }) => return Some("Devrik gövde  ·  sağ tık: parçala".to_string()),
            _ => {}
        }
        let rules = w.rules();
        if let Some(b) = self.pick_building(ray, p).and_then(|i| w.buildings.get(i)) {
            return Some(format!("{}  ·  sol tık: aç", rules.building(b.kind).name));
        }
        if let Some(a) = self.pick_worker(ray, p).and_then(|i| w.agents.get(i)) {
            return Some(format!("{}  ·  sol tık: aç", hud::kind_name(a.kind)));
        }
        if let Some(a) = w.agents.iter().filter(|a| a.id != self.me && a.health > 0).find(|a| dist2(a.x, a.y, x, y) <= 700 * 700) {
            return Some(hud::kind_name(a.kind).to_string());
        }
        let (tx, ty) = (x.div_euclid(1000) as i32, y.div_euclid(1000) as i32);
        if w.map.terrain(tx, ty) == Some(alaz_sim::Terrain::Water) && alaz_sim::map::school_near(&w.map, tx, ty, 3).is_some() {
            return Some("Balık sürüsü  ·  sağ tık: balık tut (olta ya da mızrak)".to_string());
        }
        if let Some(d) = w.wild.iter().find(|d| dist2(d.x, d.y, x, y) <= 900 * 900) {
            let name = match d.species {
                alaz_sim::WildSpecies::Dog => "Yabani köpek",
                alaz_sim::WildSpecies::Ox => "Yaban öküzü",
                alaz_sim::WildSpecies::Horse => "Yaban atı",
                alaz_sim::WildSpecies::Deer => "Geyik",
                alaz_sim::WildSpecies::Wolf => "Kurt (tehlikeli)",
            };
            return Some(name.to_string());
        }
        if w.map.terrain(tx, ty).is_some_and(|t| t != alaz_sim::Terrain::Water) {
            return match w.surface_at(tx, ty) {
                alaz_sim::Surface::Mud => Some("Çamur: yavaşlatır (çizme yardım eder)".to_string()),
                alaz_sim::Surface::Puddle => Some("Su birikintisi: ateş tutuşmaz".to_string()),
                alaz_sim::Surface::Ice => Some("Buz: kaygan, koşarsan düşersin".to_string()),
                alaz_sim::Surface::Ash => Some("Kül: otlar yeniden yeşerecek".to_string()),
                alaz_sim::Surface::Fire => Some("Yanıyor! Uzak dur".to_string()),
                alaz_sim::Surface::Dry => None,
            };
        }
        None
    }

    /// The cave tile under the pointer, with what it is.
    fn cave_cell_at(&self, p: Vec3) -> Option<((i32, i32), alaz_sim::caves::Cell)> {
        use alaz_sim::caves::Cell;
        let c = self.me_agent()?.cave_view()?;
        let t = (p.x.floor() as i32, p.z.floor() as i32);
        let cell = self.world.natural_cave_cell(c, t.0, t.1)?;
        (cell != Cell::Rock || p.y == f32::from(c.elevation_m())).then_some((t, cell))
    }

    /// What the pointer is on underground, in words, with what a click would do.
    fn cave_pointer_text(&self, ground: Vec3) -> Option<String> {
        use alaz_sim::caves::Cell;
        let c = self.me_agent()?.cave_view()?;
        let ((tx, ty), cell) = self.cave_cell_at(ground)?;
        if cell == Cell::Rubble {
            return Some("Çöken taşlar  ·  sağ tık: temizle".into());
        }
        if cell != Cell::Rock {
            return None;
        }
        Some(self.world.natural_cave_ore(c, tx, ty).and_then(alaz_sim::caves::ore_item)
            .map_or_else(|| "Kaya  ·  sağ tık: kaz".to_string(), |ore| format!("{} damarı  ·  sağ tık: kaz", crate::lang::item_word(&self.world, ore))))
    }

    /// A click underground. Left: walk there (the stairs and the exit are walked onto). Right: dig the wall under the
    /// pointer, or clear fallen rock.
    fn cave_click(&mut self, p: Vec3, primary: bool) {
        use alaz_sim::caves::Cell;
        let position = alaz_sim::cave_terrain::WorldPoint {
            x_mm: (p.x * 1000.0) as i64,
            y_mm: (p.z * 1000.0) as i64,
            z_mm: (p.y * 1000.0).round() as i64,
        };
        if primary {
            self.selected = None;
        } else if let Some(((tx, ty), Cell::Rock | Cell::Rubble)) = self.cave_cell_at(p) {
            self.issue(CommandKind::CaveDig { tx, ty });
            return;
        }
        self.issue(CommandKind::MoveToFloor { position });
    }

    /// Is (x, y) mm within `r` mm of me?
    fn near_me(&self, x: i64, y: i64, r: i64) -> bool {
        self.me_agent().is_some_and(|a| dist2(a.x, a.y, x, y) <= r * r)
    }

    /// Find the closest resource model before the ray reaches the ground.
    fn pick_node(&self, ray: PointerRay, ground: Vec3) -> Option<alaz_sim::NodeId> {
        let (id, via_crown) = self.pick_node_with(ray, ground, true)?;
        if !via_crown {
            return Some(id);
        }
        // only a crown is in the way: a small resource showing through it (the crown opens up around it) is what the
        // pointer is on; the trunk and the crown itself still take the click when nothing else is there
        match self.pick_node_with(ray, ground, false) {
            Some((small, _)) if self.world.nodes[small.0 as usize].kind != alaz_sim::NodeKind::Tree => Some(small),
            _ => Some(id),
        }
    }

    /// The node under a ray and whether it was hit through a tree's crown; with `crowns` false a crown is no hit.
    fn pick_node_with(&self, ray: PointerRay, ground: Vec3, crowns: bool) -> Option<(alaz_sim::NodeId, bool)> {
        let mut top = PointerRay::MAX_MODEL_Y;
        while top > ground.y {
            // Query the node index in short sections along the ray. Even the widest tree has a radius under 4 m.
            let bottom = (top - 8.0).max(ground.y);
            let a = ray.horizontal(top);
            let b = ray.horizontal(bottom);
            let lo = (a.min(b) - Vec2::splat(4.0)).floor();
            let hi = (a.max(b) + Vec2::splat(4.0)).ceil();
            let hit = self.world.nodes_in(lo.x as i32, lo.y as i32, hi.x as i32, hi.y as i32)
                .filter_map(|n| {
                    let (anchor, radius, height) = alaz_render::node_bounds(&self.world, n);
                    let at = Vec2::from_array(anchor);
                    let base = ray.model_base(self.world.seed, at, radius, bottom)?;
                    let (y, crown_hit) = if n.kind == alaz_sim::NodeKind::Tree {
                        let crown = base + height * 0.45;
                        let in_crown = if crowns { ray.column_hit(at, radius, bottom.max(crown), top.min(base + height)) } else { None };
                        match in_crown {
                            Some(y) => (y, true),
                            None => (ray.column_hit(at, 0.32, bottom.max(base), top.min(crown))?, false),
                        }
                    } else {
                        (ray.column_hit(at, radius, bottom.max(base), top.min(base + height))?, false)
                    };
                    Some((n, y, crown_hit))
                })
                .max_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((n, _, via_crown)) = hit {
                return self.world.node_at(n.tx, n.ty).map(|id| (id, via_crown));
            }
            top = bottom;
        }
        // A natural opening is cut into the tile next to its interaction anchor.
        let (x, y) = (ground.x.floor() as i32, ground.z.floor() as i32);
        if self.world.map.surface_cave_gap(x, y)
            && let Some(mouth) = [(0, 1), (1, 0), (0, -1), (-1, 0)].into_iter().find_map(|(dx, dy)| {
                let anchor = (x + dx, y + dy);
                let mouth = self.world.map.natural_cave_mouth(anchor.0, anchor.1)?;
                (mouth.threshold.xy() == (x, y))
                    .then(|| self.world.node_at(anchor.0, anchor.1))
                    .flatten()
            })
        {
            return Some((mouth, false));
        }
        // Small resources are also selectable anywhere on the tile where they grow.
        self.world.node_at(x, y)
            .filter(|id| self.world.nodes[id.0 as usize].kind != alaz_sim::NodeKind::Tree)
            .map(|id| (id, false))
    }

    /// Where a building would go (mm), optionally snapped to tile centers.
    fn ghost_pos(&self) -> Option<(i64, i64)> {
        self.build_mode?;
        if self.gfx.as_ref().is_some_and(|g| g.renderer.pointer_over_ui()) {
            return None;
        }
        let (_, PointerGround::Surface(p)) = self.pointer_target()? else { return None };
        let (x, z) = if self.grid_snap { (p.x.floor() + 0.5, p.z.floor() + 0.5) } else { (p.x, p.z) };
        Some(((x * 1000.0).round() as i64, (z * 1000.0).round() as i64))
    }

    fn placement_ok(&self, kind: BuildingKind, x: i64, y: i64) -> bool {
        let Some(a) = self.me_agent() else { return false };
        if let Some(i) = self.moving {
            return self.world.can_place_except(kind, x, y, Some(i)).is_ok() && dist2(a.x, a.y, x, y) <= BUILD_RADIUS_MM * BUILD_RADIUS_MM;
        }
        self.world.can_place(kind, x, y).is_ok()
            && dist2(a.x, a.y, x, y) <= BUILD_RADIUS_MM * BUILD_RADIUS_MM
            && rules().building(kind).cost_for(|c| a.inventory.has(c)).is_some()
            && rules().building(kind).needs.iter().all(|&k| a.inventory.get(k) > 0)
            && self.world.knows(rules().building(kind).discovery)
    }

    /// Why a building cannot go at (x, y) mm, in the player's words (None when it can).
    fn placement_why(&self, kind: BuildingKind, x: i64, y: i64) -> Option<String> {
        let a = self.me_agent()?;
        let def = rules().building(kind);
        if !self.world.knows(def.discovery) {
            return None;
        }
        if dist2(a.x, a.y, x, y) > BUILD_RADIUS_MM * BUILD_RADIUS_MM {
            return Some("Çok uzak: önce yaklaş.".to_string());
        }
        match self.world.can_place(kind, x, y) {
            Err(alaz_sim::world::Reject::NeedsWater) => return Some("Suya yakın olmalı.".to_string()),
            Err(alaz_sim::world::Reject::WetGround) => return Some("Islak zemin: ateş tutmaz.".to_string()),
            Err(_) => {
                let r = def.radius_mm + 400;
                let (tx, ty) = (x.div_euclid(alaz_sim::map::TILE_MM) as i32, y.div_euclid(alaz_sim::map::TILE_MM) as i32);
                let near = self.world.nodes_in(tx - 4, ty - 4, tx + 5, ty + 5).filter(|n| {
                    let (nx, ny) = n.center_mm();
                    dist2(nx, ny, x, y) < r * r
                }).min_by_key(|n| {
                    let (nx, ny) = n.center_mm();
                    dist2(nx, ny, x, y)
                });
                if let Some(n) = near {
                    return Some(format!("Yer dar: yakında {} var. Önce onu kes ya da topla, ya da açıklığa geç.", hud::node_label(&self.world, n).to_lowercase()));
                }
                if self.world.buildings.iter().any(|b| dist2(b.x, b.y, x, y) < (def.radius_mm + rules().building(b.kind).radius_mm + 200).pow(2)) {
                    return Some("Başka bir yapıya çok yakın.".to_string());
                }
                return Some("Zemin uygun değil (su, kaya ya da yarık).".to_string());
            }
            Ok(()) => {}
        }
        if def.cost_for(|c| a.inventory.has(c)).is_none() {
            return Some("Malzeme eksik.".to_string());
        }
        None
    }

    /// Send a command as the player; while recording, harvest/deposit/collect also become route steps.
    fn issue(&mut self, kind: CommandKind) {
        let id = self.me;
        if self.me_agent().is_none() {
            return;
        }
        if self.recording {
            let step = match &kind {
                CommandKind::Gather { node } => self.world.nodes.get(node.0 as usize).map(|n| {
                    let (x, y) = n.center_mm();
                    RouteStep::Harvest { kind: n.kind, x, y, radius_mm: 6_000 }
                }),
                CommandKind::Deposit { building } => Some(RouteStep::Deposit { building: *building }),
                CommandKind::Collect { building, item } => Some(RouteStep::Collect { building: *building, item: *item }),
                _ => None,
            };
            if let Some(step) = step {
                // two harvests of the same resource in a row are one step
                let dup = matches!((self.recorded.last(), &step),
                    (Some(RouteStep::Harvest { kind: a, .. }), RouteStep::Harvest { kind: b, .. }) if a == b);
                if !dup {
                    self.recorded.push(step);
                }
            }
        }
        self.pending.push(Command { agent: id, kind });
    }

    /// What the player would hear standing at (x, y) mm: woods, water and fires close by, the hour and the weather.
    fn ambience_at(&self, x: i64, y: i64, (rain, snow, wind): (f32, f32, f32)) -> audio::Scene {
        let w = &self.world;
        let (tx, ty) = ((x / 1000) as i32, (y / 1000) as i32);
        let trees = w.nodes_in(tx - 9, ty - 9, tx + 10, ty + 10).filter(|n| n.kind == alaz_sim::NodeKind::Tree).count() as f32;
        let mut near_water = f32::MAX;
        for dy in -9..=9 {
            for dx in -9..=9 {
                if w.map.terrain(tx + dx, ty + dy) == Some(alaz_sim::Terrain::Water) {
                    near_water = near_water.min(((dx * dx + dy * dy) as f32).sqrt());
                }
            }
        }
        let near_fire = w
            .buildings
            .iter()
            .filter(|b| b.kind == alaz_sim::BuildingKind::Campfire && b.active())
            .map(|b| (((b.x - x) as f32).hypot((b.y - y) as f32)) / 1000.0)
            .fold(f32::MAX, f32::min);
        audio::Scene {
            wind: wind + snow * 0.5,
            rain,
            day: ((alaz_sim::time::daylight_permille(w.tick) - 300) as f32 / 700.0).clamp(0.0, 1.0),
            trees: (trees / 40.0).min(1.0),
            water: (1.0 - near_water / 9.0).clamp(0.0, 1.0),
            fire: (1.0 - near_fire / 9.0).clamp(0.0, 1.0),
            cave: self.in_cave(),
        }
    }

    /// Where I stand, in mm: on the surface map, or on the cave level when underground.
    fn me_pos(&self) -> Option<(i64, i64)> {
        self.me_agent().map(|a| match a.cave_view() {
            Some(c) => (c.x, c.y),
            None => (a.x, a.y),
        })
    }

    fn in_cave(&self) -> bool {
        self.me_agent().is_some_and(|a| a.cave_view().is_some())
    }

    /// Left button (`primary`): walk there, or place the building being built. Right button: use what is under the
    /// pointer: open a machine's panel, gather, hunt, tame, enter a cave; nothing under it closes the panel.
    fn click(&mut self, primary: bool) {
        // clicks beside a menu window must not walk the player around (in coop the world keeps running)
        if self.me_agent().is_none() || self.menu.is_some() {
            return;
        }
        // a stray click must not throw away a craft in progress: walking away is a deliberate key press
        if self.world.craft_view(self.me).is_some() {
            return;
        }
        if let Some(kind) = self.build_mode {
            if primary {
                if let Some((x, y)) = self.ghost_pos() {
                    // the sim says why a bad spot is refused; build mode stays on so the next try is one click
                    let ok = self.placement_ok(kind, x, y);
                    match self.moving {
                        Some(i) => {
                            self.issue(CommandKind::Relocate { building: i as u32, x, y });
                            let have = self.world.buildings.get(i).map_or(0, |b| b.facing);
                            for _ in 0..(self.facing + 4 - have) % 4 {
                                self.issue(CommandKind::Turn { building: i as u32 });
                            }
                        }
                        None => {
                            self.issue(CommandKind::Build { kind, x, y });
                            if self.facing != 0 {
                                self.turn_after = Some((x, y, self.facing));
                            }
                        }
                    }
                    if ok {
                        self.build_mode = None;
                        self.moving = None;
                    }
                }
            } else {
                self.build_mode = None;
                self.moving = None;
            }
            return;
        }
        let Some((ray, target)) = self.pointer_target() else { return };
        let p = match target {
            PointerGround::Surface(p) => p,
            PointerGround::Cave(p) => {
                self.cave_click(p, primary);
                return;
            }
        };
        let (px, py) = ((p.x * 1000.0) as i64, (p.z * 1000.0) as i64);
        if primary {
            if self.in_cave() {
                let z_mm = i64::from(alaz_sim::map::elevation_at_mm(self.world.seed, px, py));
                let position = alaz_sim::cave_terrain::WorldPoint { x_mm: px, y_mm: py, z_mm };
                self.selected = None;
                self.issue(CommandKind::MoveToFloor { position });
                return;
            }
            // a building or a worker under the pointer (anywhere on its model): open it; elsewhere, walk there
            if let Some(i) = self.pick_building(ray, p) {
                self.selected = Some(Selection::Building(i));
                return;
            }
            if let Some(i) = self.pick_worker(ray, p) {
                self.selected = Some(Selection::Agent(i));
                return;
            }
            self.issue(CommandKind::MoveTo { x: px, y: py });
            return;
        }
        // a piece of wood lying about: pick it up (and the rest near it); a felled trunk: break it up
        if let Some(kind) = self.body_at(px, py) {
            self.issue(kind);
            return;
        }
        if let Some(i) = self.pick_building(ray, p) {
            self.selected = Some(Selection::Building(i));
            return;
        }
        if let Some(i) = self.pick_worker(ray, p) {
            // your own dog, close by: a right click pets it
            let me = self.me_agent().map(|m| (m.x, m.y));
            if let (Some(d), Some((mx, my))) = (self.world.agents.get(i), me)
                && d.kind == alaz_sim::AgentKind::Dog && (d.x - mx).pow(2) + (d.y - my).pow(2) <= 2_500i64.pow(2) {
                    if let Some(a) = self.audio.as_mut() {
                        a.play(audio::Sfx::Bark);
                        a.play(audio::Sfx::Tame);
                    }
                    let line = ["Köpeğin başını okşadın; kuyruğunu sallıyor.", "Köpek sırtüstü yattı, karnını kaşıdın.", "Köpek elini yaladı."];
                    self.pets = self.pets.wrapping_add(1);
                    self.say(line[self.pets as usize % line.len()].to_string());
                    return;
                }
            self.selected = Some(Selection::Agent(i));
            return;
        }
        self.selected = None;
        if let Some(wild) = self.world.wild_at(px, py, 1_200) {
            let prey = self.world.wild.iter().any(|d| d.id == wild && d.species.huntable());
            if prey && self.me_agent().map_or(0, |a| a.inventory.get(alaz_sim::ItemKind::Spear)) == 0 {
                self.say("Bu bir av hayvanı. Mızrak (keskin taş + 2 odun) gerekir.");
            } else if prey {
                self.issue(CommandKind::Hunt { wild });
            } else {
                self.issue(CommandKind::Tame { wild });
            }
            return;
        }
        let picked = self.pick_node(ray, p).or_else(|| self.world.node_at(p.x.floor() as i32, p.z.floor() as i32));
        if let Some(node) = picked {
            let command = if self.world.nodes[node.0 as usize].kind == alaz_sim::NodeKind::CaveMouth {
                let Some(position) = self.world.cave_entrance_destination(node) else { return };
                CommandKind::MoveToFloor { position }
            } else {
                CommandKind::Gather { node }
            };
            self.issue(command);
            return;
        }
        // water with fish in it: cast there
        let (tx, ty) = (p.x.floor() as i32, p.z.floor() as i32);
        if self.world.map.terrain(tx, ty) == Some(alaz_sim::Terrain::Water) {
            if alaz_sim::map::school_near(&self.world.map, tx, ty, 3).is_some() {
                self.issue(CommandKind::Fish { tx, ty });
            } else {
                self.say("Burada balık yok. Sudaki koyu gölgeleri ara.");
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let attrs = Window::default_attributes().with_title("A.L.A.Z");
        // dev aid for screenshots: ALAZ_WINDOW=x,y,w,h (on Wayland the position is only a hint)
        let attrs = match std::env::var("ALAZ_WINDOW").ok().map(|v| v.split(',').filter_map(|n| n.trim().parse::<i32>().ok()).collect::<Vec<_>>()) {
            Some(v) if v.len() == 4 => attrs
                .with_position(winit::dpi::PhysicalPosition::new(v[0], v[1]))
                .with_inner_size(winit::dpi::PhysicalSize::new(v[2] as u32, v[3] as u32)),
            _ => attrs,
        };
        let window = Arc::new(el.create_window(attrs).expect("window"));
        let renderer = Renderer::with_quality(window.clone(), gfxcfg::current());
        self.gfx = Some(Gfx { window, renderer });
        self.last = Instant::now();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, ev: WindowEvent) {
        let ui_consumed = self.gfx.as_mut().is_some_and(|g| g.renderer.on_window_event(&ev));
        match ev {
            WindowEvent::CloseRequested => {
                self.save_game(false);
                el.exit();
            }
            WindowEvent::Resized(s) => {
                if let Some(g) = self.gfx.as_mut() {
                    g.renderer.resize(s.width, s.height);
                }
            }
            WindowEvent::Focused(false) => {
                self.keys.clear();
                self.mid_drag = None;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    self.input_key(code, event.state == ElementState::Pressed, event.repeat);
                }
            }
            WindowEvent::CursorMoved { position, .. } => self.input_cursor((position.x as f32, position.y as f32)),
            WindowEvent::MouseInput { state, button, .. } => self.input_button(button, state == ElementState::Pressed, ui_consumed),
            WindowEvent::MouseWheel { delta, .. } if !ui_consumed => {
                let s = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.input_wheel(s);
            }
            WindowEvent::RedrawRequested => {
                self.frame();
                if self.quit {
                    el.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }
}

/// `ALAZ_DEMO_VIEW=<item id>` (an icon preview instead of a model).
fn view_item() -> Option<alaz_sim::ItemKind> {
    static ITEM: std::sync::OnceLock<Option<alaz_sim::ItemKind>> = std::sync::OnceLock::new();
    *ITEM.get_or_init(|| {
        let v = std::env::var("ALAZ_DEMO_VIEW").ok()?;
        alaz_sim::ItemKind::ALL.iter().copied().find(|k| alaz_render::assets::snake(&format!("{k:?}")) == v)
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PointerGround {
    Surface(Vec3),
    Cave(Vec3),
}

fn choose_pointer_ground(surface: Option<Vec3>, cave: Option<Vec3>) -> Option<PointerGround> {
    match (surface, cave) {
        (Some(surface), Some(cave)) if cave.y > surface.y => Some(PointerGround::Cave(cave)),
        (Some(surface), _) => Some(PointerGround::Surface(surface)),
        (None, cave) => cave.map(PointerGround::Cave),
    }
}

/// The orthographic pointer line, parameterized by world height in meters.
#[derive(Clone, Copy)]
struct PointerRay {
    zero: Vec2,
    rise: Vec2,
}

impl PointerRay {
    // The generator adds at most 21.21 m of base height and 42.22 m of ledges.
    const MAX_GROUND_Y: f32 = 64.0;
    const MAX_MODEL_Y: f32 = Self::MAX_GROUND_Y + 16.0;

    fn from_camera(cam: &Camera, ndc: (f32, f32)) -> Option<Self> {
        let p = cam.ground_at(ndc.0, ndc.1, 1.0)?;
        let (sin, cos) = cam.yaw_deg.to_radians().sin_cos();
        let (up, across) = cam.pitch_deg.to_radians().sin_cos();
        Some(Self { zero: Vec2::new(p.x, p.z), rise: Vec2::new(sin, cos) * (across / up) })
    }

    fn horizontal(self, y: f32) -> Vec2 {
        self.zero + self.rise * y
    }

    fn at(self, y: f32) -> Vec3 {
        let p = self.horizontal(y);
        Vec3::new(p.x, y, p.y)
    }

    /// The first terrain triangle crossed by the pointer, ignoring open mouths and camera cuts.
    fn terrain_hit_where(
        self,
        height: impl Fn(i32, i32) -> f32,
        visible: impl Fn(i32, i32, Vec3) -> bool,
    ) -> Option<Vec3> {
        let mut top = Self::MAX_GROUND_Y;
        let mut tile = self.horizontal(top).floor().as_ivec2();
        let next_edge = |axis: usize, tile: i32| {
            let rise = self.rise[axis];
            if rise == 0.0 {
                return f32::NEG_INFINITY;
            }
            let edge = tile + i32::from(rise < 0.0);
            (edge as f32 - self.zero[axis]) / rise
        };
        while top > 0.0 {
            let x_edge = next_edge(0, tile.x);
            let z_edge = next_edge(1, tile.y);
            let bottom = x_edge.max(z_edge).clamp(0.0, top);
            let xz = tile.as_vec2();
            let a = height(tile.x, tile.y);
            let b = height(tile.x + 1, tile.y);
            let c = height(tile.x + 1, tile.y + 1);
            let d = height(tile.x, tile.y + 1);
            let clearance = |y| {
                let p = self.horizontal(y) - xz;
                // The terrain mesh splits each tile along the diagonal from (0, 0) to (1, 1).
                let surface = if p.x >= p.y {
                    a + (b - a) * p.x + (c - b) * p.y
                } else {
                    a + (c - d) * p.x + (d - a) * p.y
                };
                y - surface
            };
            let diagonal = (xz.x - xz.y - self.zero.x + self.zero.y) / (self.rise.x - self.rise.y);
            let mut from = top;
            let mut above = clearance(from);
            for to in [diagonal, bottom].into_iter().filter(|&y| y >= bottom && y < top) {
                let below = clearance(to);
                let crossing = if above == 0.0 {
                    Some(from)
                } else if (above > 0.0 && below <= 0.0) || (above < 0.0 && below >= 0.0) {
                    Some(from + (to - from) * (above / (above - below)))
                } else {
                    None
                };
                if let Some(y) = crossing {
                    let point = self.at(y);
                    if visible(tile.x, tile.y, point) {
                        return Some(point);
                    }
                }
                from = to;
                above = below;
            }
            if x_edge >= z_edge {
                tile.x += if self.rise.x > 0.0 { -1 } else { 1 };
            }
            if z_edge >= x_edge {
                tile.y += if self.rise.y > 0.0 { -1 } else { 1 };
            }
            top = bottom;
        }
        None
    }

    fn cave_floor_hit(self, floor_at: impl Fn(i32, i32, i16) -> bool) -> Option<Vec3> {
        // Walkable floors lie above bedrock and below the highest generated ground.
        (-95i16..=64).rev().find_map(|z| {
            let point = self.at(f32::from(z));
            floor_at(point.x.floor() as i32, point.z.floor() as i32, z).then_some(point)
        })
    }

    fn column_hit(self, center: Vec2, radius: f32, bottom: f32, top: f32) -> Option<f32> {
        if bottom > top {
            return None;
        }
        let offset = self.horizontal(top) - center;
        let radius_sq = radius * radius;
        if offset.length_squared() <= radius_sq {
            return Some(top);
        }
        let speed_sq = self.rise.length_squared();
        if speed_sq == 0.0 {
            return None;
        }
        let nearest = offset.dot(self.rise) / speed_sq;
        if nearest <= 0.0 {
            return None;
        }
        let miss_sq = (offset - self.rise * nearest).length_squared();
        if miss_sq > radius_sq {
            return None;
        }
        let y = top - nearest + ((radius_sq - miss_sq) / speed_sq).sqrt();
        (y >= bottom).then_some(y.min(top))
    }

    /// Static model vertices all use the terrain height at their anchor, plus a 2 cm lift.
    fn model_base(self, seed: u64, at: Vec2, radius: f32, min_y: f32) -> Option<f32> {
        self.column_hit(at, radius, min_y, Self::MAX_MODEL_Y)?;
        Some(alaz_render::ground_height(seed, at.x, at.y) + 0.02)
    }
}

#[cfg(test)]
mod pointer_tests {
    use super::*;

    #[test]
    fn projected_point_lies_on_pointer_ray() {
        for pitch_deg in [28.0, 45.0, 90.0] {
            let cam = Camera { target: Vec3::new(658.0, 32.0, 777.0), pitch_deg, yaw_deg: 213.0, ..Camera::new() };
            let point = Vec3::new(660.0, 34.0, 778.0);
            let [x, y] = cam.project(1.0, point);
            let ray = PointerRay::from_camera(&cam, (x, y)).unwrap();
            assert!(ray.at(point.y).distance(point) < 0.002);
        }
    }

    #[test]
    fn pointer_hits_front_of_ridge_before_the_ground_behind_it() {
        let ray = PointerRay { zero: Vec2::new(0.0, 0.5), rise: Vec2::X };
        let hit = ray.terrain_hit_where(|x, _| if x == 3 { 5.0 } else { 0.0 }, |_, _, _| true).unwrap();
        assert!(hit.distance(Vec3::new(10.0 / 3.0, 10.0 / 3.0, 0.5)) < 0.001);
    }

    #[test]
    fn terrain_hit_uses_the_mesh_diagonal() {
        let ray = PointerRay { zero: Vec2::new(0.25, 0.75), rise: Vec2::ZERO };
        let hit = ray.terrain_hit_where(|x, z| if x == 1 && z == 1 { 8.0 } else { 0.0 }, |_, _, _| true).unwrap();
        assert_eq!(hit, Vec3::new(0.25, 2.0, 0.75));
    }

    #[test]
    fn a_cave_floor_is_picked_through_a_hole_in_the_surface() {
        let ray = PointerRay { zero: Vec2::splat(0.5), rise: Vec2::ZERO };
        let floor = ray.cave_floor_hit(|x, y, z| x == 0 && y == 0 && [-2, -5].contains(&z));
        let surface = ray.terrain_hit_where(|_, _| 1.0, |_, _, _| true);

        assert_eq!(choose_pointer_ground(surface, floor), Some(PointerGround::Surface(ray.at(1.0))));
        let opening = ray.terrain_hit_where(|_, _| 1.0, |x, y, _| (x, y) != (0, 0));
        assert_eq!(choose_pointer_ground(opening, floor), Some(PointerGround::Cave(ray.at(-2.0))));
    }

    #[test]
    fn ray_picks_side_of_model_above_the_ground_hit() {
        let ray = PointerRay { zero: Vec2::ZERO, rise: Vec2::X };
        let target = Vec2::new(5.0, 0.0);
        assert_eq!(ray.column_hit(target, 0.5, 3.0, 7.0), Some(5.5));
        assert_eq!(ray.column_hit(target, 0.5, 6.0, 7.0), None);
    }
}

/// The Windows build has no console, so a panic would vanish with the window. The message goes to a file the player can send.
#[cfg(windows)]
fn install_crash_log() {
    let Some(dir) = std::env::var_os("LOCALAPPDATA").map(|d| std::path::PathBuf::from(d).join("alaz")) else { return };
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("crash.txt"), format!("{info}\n\n{}", std::backtrace::Backtrace::force_capture()));
        previous(info);
    }));
}

fn main() {
    #[cfg(windows)]
    install_crash_log();
    // `--export-assets <folder>`: write the code-made models, tiles and icons out as files, then leave (no window)
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--export-assets") {
        let dir = args.get(i + 1).cloned().unwrap_or_else(|| "alaz-assets".into());
        export::run(std::path::Path::new(&dir));
        return;
    }
    // `--ui-shot <folder>`: draw the inventory window per era and tab into PNGs, then leave (no window)
    if let Some(i) = args.iter().position(|a| a == "--ui-shot") {
        let dir = args.get(i + 1).cloned().unwrap_or_else(|| "alaz-ui-shots".into());
        uishot::run(std::path::Path::new(&dir));
        return;
    }
    let el = EventLoop::new().expect("event loop");
    el.set_control_flow(ControlFlow::Poll);
    el.run_app(&mut App::new(parse_args())).expect("run");
}

/// How long the drawn body takes to cover most of the way (seconds, as an exponential time constant): 90% in about
/// four seconds and 99% in about eight, so a sudden gain in understanding changes the body over a few seconds.
const EVO_TAU: f32 = 1.8;

/// One frame of the body's glide toward the perception it has reached (both in permille): an exponential approach,
/// the same whatever the frame rate, that lands on the goal once it is within half a permille.
fn glide_evo(view: f32, goal: f32, dt: f32) -> f32 {
    let n = view + (goal - view) * (1.0 - (-dt.max(0.0) / EVO_TAU).exp());
    if (goal - n).abs() < 0.4 {
        goal
    } else {
        n
    }
}

#[cfg(test)]
mod glide_tests {
    use super::*;

    fn run(from: f32, to: f32, secs: f32, fps: f32) -> f32 {
        let mut v = from;
        for _ in 0..(secs * fps) as usize {
            v = glide_evo(v, to, 1.0 / fps);
        }
        v
    }

    #[test]
    fn body_catches_up_in_seconds() {
        // 0 to a gameplay step 860: nine tenths of the way in 4.5 s, all but done in 9 s, never past the goal
        assert!(run(0.0, 860.0, 4.5, 60.0) > 0.9 * 860.0);
        assert!(run(0.0, 860.0, 9.0, 60.0) > 0.99 * 860.0);
        assert!(run(0.0, 860.0, 1.0, 60.0) < 0.5 * 860.0, "not instant");
        assert_eq!(run(0.0, 860.0, 30.0, 60.0), 860.0);
        // the other way round too, and a long frame (a hitch) does not overshoot
        assert!(run(860.0, 100.0, 9.0, 60.0) < 100.0 + 0.01 * 760.0);
        assert!(glide_evo(0.0, 500.0, 5.0) <= 500.0);
        // the frame rate does not matter
        let (a, b) = (run(0.0, 640.0, 3.0, 144.0), run(0.0, 640.0, 3.0, 30.0));
        assert!((a - b).abs() < 2.0, "{a} vs {b}");
        // each frame moves a little: no jump on the way
        let mut v = 0.0;
        for _ in 0..600 {
            let n = glide_evo(v, 860.0, 1.0 / 60.0);
            assert!(n - v < 8.0 && n >= v);
            v = n;
        }
    }
}

#[cfg(test)]
mod net_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn normalize_addr_adds_default_port() {
        assert_eq!(normalize_addr("10.0.0.7"), "10.0.0.7:7777");
        assert_eq!(normalize_addr("10.0.0.7:9000"), "10.0.0.7:9000");
    }

    #[test]
    fn background_connect_reports_refused_quickly() {
        let addr = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().to_string();
        let t0 = Instant::now();
        let rx = connect_in_background(addr);
        assert!(t0.elapsed() < Duration::from_millis(100), "the call itself must not block");
        assert!(matches!(rx.recv_timeout(Duration::from_secs(10)), Ok(Err(_))));
    }

    #[test]
    fn background_connect_joins_a_host() {
        let mut host = Host::bind("127.0.0.1:0").unwrap();
        let mut w = World::new(5);
        let (sx, sy) = w.spawn_mm;
        w.spawn_agent(sx, sy);
        let rx = connect_in_background(host.local_addr().unwrap().to_string());
        let t0 = Instant::now();
        loop {
            host.poll(&w);
            let c = host.turn(&w, vec![]);
            w.step(&c);
            match rx.try_recv() {
                Ok(Ok((client, _))) => {
                    assert_eq!(client.agent, AgentId(1));
                    break;
                }
                Ok(Err(e)) => panic!("join failed: {e}"),
                Err(_) => {}
            }
            assert!(t0.elapsed() < Duration::from_secs(5), "no join within 5 s");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
