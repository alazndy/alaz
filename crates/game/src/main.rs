// On Windows the game is a window, not a console program (no empty terminal behind it).
#![cfg_attr(windows, windows_subsystem = "windows")]

use alaz_render::{Camera, Ghost, Renderer};
use alaz_sim::time::TICKS_PER_SECOND;
use alaz_sim::world::{dist2, BUILD_RADIUS_MM};
use alaz_net::{apply_turn, Client, Host};
use alaz_sim::{rules, AgentId, BuildingKind, Command, CommandKind, RouteStep, World};
use hud::{Selection, UiAction, UiCtx};
mod audio;
mod hud;
mod icons;
mod keys;
mod lang;
mod menu;
mod saveio;
mod story;
mod ui_art;

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
    host: bool,
    size: i32,
    port: u16,
    join: Option<String>,
}

fn parse_args() -> Args {
    let mut a = Args { new_game: false, seed: None, hardcore: false, demo: false, host: false, size: alaz_sim::map::MAP_W, port: alaz_net::DEFAULT_PORT, join: None };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--new" => a.new_game = true,
            "--hardcore" => a.hardcore = true,
            "--demo" => a.demo = true,
            "--host" => a.host = true,
            "--size" => a.size = it.next().and_then(|v| v.parse().ok()).unwrap_or(a.size),
            "--port" => a.port = it.next().and_then(|v| v.parse().ok()).unwrap_or(a.port),
            "--join" => a.join = it.next(),
            "--seed" => a.seed = it.next().and_then(|v| v.parse().ok()),
            _ => eprintln!("bilinmeyen argüman: {arg} (--new, --seed N, --hardcore, --demo, --size N, --host [--port N], --join ADRES)"),
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
    let tagged = if size > 4096 { alaz_sim::map::tag_seed_v3(seed) } else { alaz_sim::map::tag_seed_v2(seed) };
    let mut w = World::with_size(tagged, size, size);
    w.settings = settings;
    let (sx, sy) = w.spawn_mm;
    w.spawn_agent(sx, sy);
    w.hardcore = hardcore;
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
                    if w.map.terrain(tx, ty) == Some(alaz_sim::map::Terrain::Water) && w.map.terrain(tx, ty + 1).is_some_and(|t| t != alaz_sim::map::Terrain::Water) && w.node_at(tx, ty + 1).is_none() {
                        if best.is_none_or(|(_, _, d)| d2(tx, ty) < d) {
                            best = Some((tx, ty, d2(tx, ty)));
                        }
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
                alaz_sim::MinigameKind::Timing => (v.pos.abs_diff(v.center) <= v.half / 3).then_some(CommandKind::Strike),
                alaz_sim::MinigameKind::Stack => Some(CommandKind::PlaceAt { x: 0, y: 0 }),
            };
            w.step(&kind.map(|k| vec![Command { agent: id, kind: k }]).unwrap_or_default());
        }
    } else if std::env::var_os("ALAZ_DEMO_WORK").is_none() && std::env::var_os("ALAZ_DEMO_IDLE").is_none() {
        w.step(&[Command { agent: id, kind: CommandKind::Craft { recipe: RecipeId(0) } }]);
    }
    if std::env::var("ALAZ_DEMO_CAVE").as_deref() == Ok("in") {
        if let Some(n) = w.nodes.iter().find(|n| n.kind == alaz_sim::NodeKind::CaveMouth && n.tx > 12 && n.ty > 12).cloned() {
            w.agents[0].x = n.tx as i64 * 1000 + 500;
            w.agents[0].y = n.ty as i64 * 1000 + 1_500;
            w.agents[0].task = alaz_sim::world::Task::Idle;
            w.agents[0].inventory.counts[ItemKind::Torch as usize] = 1;
            let node = alaz_sim::NodeId(w.nodes.iter().position(|m| m.tx == n.tx && m.ty == n.ty).unwrap() as u32);
            w.step(&[Command { agent: id, kind: CommandKind::EnterCave { node } }]);
            w.step(&[Command { agent: id, kind: CommandKind::CaveDeeper }]);
            for _ in 0..200 {
                w.step(&[]);
            }
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
        let mut t = start;
        while t < start + TICKS_PER_DAY * 30 && weather(w.seed, t, days) != want {
            t += TICKS_PER_DAY / 6;
        }
        w.tick = t + 120;
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
    fps_t: f32,
    fps_n: u32,
    hints: Vec<(String, f32)>,
    warned: [bool; 3],
    last_save_tick: u64,
    autosave_ticks: u64,
    build_mode: Option<BuildingKind>,
    grid_snap: bool,
    selected: Option<Selection>,
    recording: bool,
    recorded: Vec<RouteStep>,
    wild_hinted: bool,
    wolf_hinted: bool,
    last_strike: Option<(bool, f32)>,
    /// The evolution stage last seen, to announce the next one.
    last_stage: u8,
    knap: hud::KnapState,
    story: story::Story,
    /// A debug script driving the player (`ALAZ_SCRIPT=file`), and where its log goes.
    script: Option<alaz_sim::debug::Script>,
    script_done: bool,
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
}

impl App {
    fn new(args: Args) -> Self {
        let mut hints = Vec::new();
        let mut loaded = None;
        let mut net = Net::Solo;
        let mut me = AgentId(0);
        let mut joined = None;
        if let Some(addr) = &args.join {
            let addr = if addr.contains(':') { addr.clone() } else { format!("{addr}:{}", alaz_net::DEFAULT_PORT) };
            match Client::connect(&addr, std::time::Duration::from_secs(8)) {
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
            match saveio::load() {
                Ok(w) => loaded = w,
                Err(e) => {
                    eprintln!("kayıt yüklenemedi: {e}");
                    hints.push((format!("Kayıt yüklenemedi ({e}). Yeni oyun başlıyor."), hud::HINT_SECONDS));
                }
            }
        }
        let world = match loaded {
            _ if joined.is_some() => joined.take().unwrap(),
            Some(w) => {
                hints.push((format!("Kayıt yüklendi (gün {}).", w.day() + 1), hud::HINT_SECONDS));
                w
            }
            None if show_menu => new_world(1, false, 64),
            None => {
                hints.push(("Sol tık: yürü. Sağ tık: topla. Taş, odun, yemiş, dal ve lif topla.".into(), hud::HINT_SECONDS));
                hints.push(("Yap sekmesinde eline iki şey alıp Dene dersen ne olacağını öğrenirsin.".into(), hud::HINT_SECONDS));
                new_world(args.seed.unwrap_or_else(random_seed), args.hardcore, args.size)
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
            recording: false,
            recorded: Vec::new(),
            wild_hinted: false,
            wolf_hinted: false,
            last_strike: None,
            last_stage: 255,
            knap: hud::KnapState::default(),
            story: story::Story::default(),
            script_done: false,
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
            no_save: matches!(net, Net::Client(_)),
            perf: Perf::from_env(),
            net,
            me,
        };
        if let Some(z) = std::env::var("ALAZ_DEMO_ZOOM").ok().and_then(|v| v.parse::<f32>().ok()) {
            app.cam.zoom = z;
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
            _ => None,
        };
        if show_menu && app.perf.as_ref().is_none_or(|p| p.bench_secs.is_none()) {
            app.menu = Some(menu::Menu::new(false));
            app.no_save = true;
        }
        if app.perf.as_ref().is_some_and(|p| p.bench_secs.is_some()) {
            app.no_save = true;
        }
        if args.host && !args.join.is_some() {
            match Host::bind(("0.0.0.0", args.port)) {
                Ok(h) => {
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
        }
        app.last_save_tick = app.world.tick;
        app.center_camera();
        // a fresh world opens with its story; a loaded one only tells the ages still to come
        if app.world.tick == 0 && app.menu.is_none() && !args.demo {
            app.story.begin(&app.world);
        } else {
            app.story.resume(&app.world);
        }
        app
    }

    fn center_camera(&mut self) {
        let (x, y) = self.me_agent().map_or(self.world.spawn_mm, |a| (a.x, a.y));
        self.cam.target.x = x as f32 / 1000.0;
        self.cam.target.z = y as f32 / 1000.0;
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
        self.world = w;
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

    fn toggle_tab(&mut self, t: hud::Tab) {
        self.tab = if self.tab == Some(t) { None } else { Some(t) };
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
                } else {
                    self.menu = Some(menu::Menu::new(true));
                }
            }
            A::Grid => self.grid_snap = !self.grid_snap,
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
                if let Some((x, y)) = self.me_agent().map(|a| (a.x, a.y)) {
                    self.issue(CommandKind::MoveTo { x, y });
                }
            }
            A::View => {
                self.pitch_goal = if self.pitch_goal >= 89.0 { Camera::PITCH_ISO } else { Camera::PITCH_TOP };
            }
            A::Bag => self.toggle_tab(hud::Tab::Bag),
            A::Craft => self.toggle_tab(hud::Tab::Craft),
            A::Build => self.toggle_tab(hud::Tab::Build),
            A::Goals => self.toggle_tab(hud::Tab::Goals),
            A::Routes => self.toggle_tab(hud::Tab::Routes),
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
                if self.menu.as_ref().is_some_and(|m| m.paused) {
                    self.save_game(false); // keep the current game before swapping
                }
                saveio::set_slot(n);
                match saveio::load() {
                    Ok(Some(w)) => {
                        self.replace_world(w);
                        self.no_save = false;
                        self.menu = None;
                        self.say(format!("Yuva {n} yüklendi."));
                    }
                    Ok(None) => {}
                    Err(e) => {
                        if let Some(m) = self.menu.as_mut() {
                            m.error = Some(format!("Yuva {n} yüklenemedi: {e}"));
                        }
                    }
                }
            }
            M::New { slot, seed, size, hardcore, settings } => {
                if self.menu.as_ref().is_some_and(|m| m.paused) {
                    self.save_game(false);
                }
                saveio::set_slot(slot);
                let w = new_world_with(seed.unwrap_or_else(random_seed), hardcore, size, settings);
                self.replace_world(w);
                self.story.begin(&self.world);
                self.no_save = false;
                self.menu = None;
                self.save_game(false);
                self.say("Sol tık: yürü. Sağ tık: topla. Taş, odun, yemiş, dal ve lif topla.");
                self.say("Yap sekmesinde eline iki şey alıp Dene dersen ne olacağını öğrenirsin.");
            }
        }
    }

    fn save_game(&mut self, announce: bool) {
        if self.no_save {
            return;
        }
        match saveio::save(&self.world) {
            Ok(()) => {
                self.last_save_tick = self.world.tick;
                if announce {
                    self.say("Kaydedildi.");
                }
            }
            Err(e) => self.say(format!("Kaydedilemedi: {e}")),
        }
    }

    fn load_game(&mut self) -> bool {
        match saveio::load() {
            Ok(Some(w)) => {
                self.replace_world(w);
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
        self.last = now;
        if let Some(g) = self.gfx.as_ref() {
            self.cam.win = g.renderer.size();
        }

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
        let want_sprint = self.menu.is_none() && !crafting && !self.in_cave() && sprint_held;
        let sim_sprint = self.me_agent().is_some_and(|a| a.sprint);
        if want_sprint != self.sprinting || (!want_sprint && sim_sprint) || (want_sprint && !sim_sprint && self.sprinting && self.me_agent().is_some_and(|a| a.stamina >= alaz_sim::world::SPRINT_MIN_START)) {
            self.sprinting = want_sprint;
            self.issue(CommandKind::SetSprint { on: want_sprint });
        }
        let walk_ok = self.menu.is_none() && !crafting && !self.in_cave();
        self.walk_cool -= dt;
        if walk_ok && (wr != 0.0 || wf != 0.0) {
            if self.walk_cool <= 0.0 {
                if let Some((ax, ay)) = self.me_agent().map(|a| (a.x, a.y)) {
                    let y = self.cam.yaw_deg.to_radians();
                    let (dx, dz) = (wr * y.cos() - wf * y.sin(), -wr * y.sin() - wf * y.cos());
                    let len = (dx * dx + dz * dz).sqrt().max(1e-3);
                    let reach = 4000.0;
                    let (tx, ty) = (ax + (dx / len * reach) as i64, ay + (dz / len * reach) as i64);
                    // straight where the keys push, over an edge if one is in the way
                    self.issue(CommandKind::Walk { x: tx, y: ty });
                    self.walk_cool = 0.2;
                }
            }
            self.walking = true;
            self.follow = true;
        } else if self.walking {
            self.walking = false;
            // letting go of the keys stops the walk, but must not cancel a craft or a cave that just began
            let busy = crafting || self.in_cave();
            if let Some((x, y)) = self.me_agent().filter(|_| !busy).map(|a| (a.x, a.y)) {
                self.issue(CommandKind::MoveTo { x, y });
            }
        }
        if self.follow && bench_pose.is_none() {
            if let Some((x, y)) = self.me_agent().map(|a| (a.x as f32 / 1000.0, a.y as f32 / 1000.0)) {
                let k = 1.0 - (-7.0 * dt).exp();
                self.cam.target.x += (x - self.cam.target.x) * k;
                self.cam.target.z += (y - self.cam.target.z) * k;
            }
        }
        // the camera rides the hills with the ground under its target
        let gh = alaz_render::ground_height(self.world.seed, self.cam.target.x, self.cam.target.z);
        self.cam.target.y += (gh - self.cam.target.y) * (1.0 - (-8.0 * dt).exp());
        let d = self.pitch_goal - self.cam.pitch_deg;
        self.cam.pitch_deg += d * (1.0 - (-10.0 * dt).exp());

        let sim_t0 = Instant::now();
        // fixed-step simulation
        if let Net::Client(client) = &mut self.net {
            // the host owns the clock: apply whatever turns arrived, catching up when behind
            client.poll();
            if let Some(w) = client.take_fresh_world() {
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
            self.accum += dt;
            let mut steps = 0;
            while self.accum >= STEP && steps < 5 {
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
            if steps == 5 {
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
        let stage = self.world.perception_level().min(5);
        if self.last_stage != 255 && stage > self.last_stage {
            self.say(format!("Evrildin: {}. {}", hud::STAGES[stage as usize], hud::stage_gift(stage)));
        }
        self.last_stage = stage;
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
        if !self.wolf_hinted {
            if let Some(a) = self.me_agent() {
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
        }
        if !self.wild_hinted {
            if let Some(a) = self.me_agent() {
                if self.world.wild_at(a.x, a.y, 12_000).is_some() {
                    self.wild_hinted = true;
                    self.say("Yabani bir hayvan var. Yanında yemişin varsa ona sağ tıkla: güvenini kazanabilirsin.");
                }
            }
        }
        for e in &events {
            if let alaz_sim::Event::Struck { agent, hit } = e {
                if *agent == self.me {
                    self.last_strike = Some((*hit, 0.35));
                }
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
                    E::Discovered(_) => a.play(audio::Sfx::Chime),
                    E::Milestone(alaz_sim::Milestone::Visitor) => a.play(audio::Sfx::Bark),
                    E::Milestone(_) => a.play(audio::Sfx::Fanfare),
                    E::Died(id) if *id == self.me => a.play(audio::Sfx::Hurt),
                    E::Tamed(_) | E::WorkerReady(_) => a.play(audio::Sfx::Tame),
                    E::JobDone { .. } => a.play(audio::Sfx::Ding),
                    E::Hunted { agent, .. } if *agent == self.me => a.play(audio::Sfx::Hit),
                    E::Fell { agent, .. } if *agent == self.me => a.play(audio::Sfx::Thud),
                    E::TreeDown { x, y, .. } if self.near_me(*x, *y, 30_000) => a.play(audio::Sfx::Timber),
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
        if std::env::var("ALAZ_DEMO_WORK").as_deref() == Ok("drink") {
            if let Some(a) = self.world.agents.get_mut(self.me.0 as usize) {
                a.thirst = a.thirst.min(2_000);
            }
        }
        // dev aid: hold the walk keys, as a player would ("dx,dy" in metres: the way to keep walking)
        if let Some(d) = std::env::var("ALAZ_DEMO_WALK").ok().map(|v| v.split(',').filter_map(|n| n.trim().parse::<i64>().ok()).collect::<Vec<_>>()) {
            if d.len() == 2 && self.world.tick % 12 == 0 {
                if let Some((ax, ay)) = self.me_agent().map(|a| (a.x, a.y)) {
                    let len = ((d[0] * d[0] + d[1] * d[1]) as f64).sqrt().max(1.0);
                    self.issue(CommandKind::Walk { x: ax + (d[0] as f64 / len * 4_000.0) as i64, y: ay + (d[1] as f64 / len * 4_000.0) as i64 });
                }
            }
        }
        // dev aid: gravity at work around the player, over and over, to look at it
        if let Ok(v) = std::env::var("ALAZ_DEMO_PHYS") {
            self.demo_phys(&v);
        }
        // dev aid: a blow a few seconds in, to look at the hurt effects
        if std::env::var_os("ALAZ_DEMO_HURT").is_some() && self.demo_hurt_t < 4.0 {
            self.demo_hurt_t += dt;
            if self.demo_hurt_t >= 4.0 {
                if let Some(a) = self.world.agents.get_mut(self.me.0 as usize) {
                    a.health -= 1500;
                }
            }
        }
        // getting hurt: any real drop in health (a bite, a fall, a blow), not the slow drain of hunger or cold
        let hp = self.me_agent().map(|a| a.health);
        if let (Some(now_hp), Some(before)) = (hp, self.prev_hp) {
            if before - now_hp >= 150 {
                self.hurt_flash = 1.0;
                if let Some(a) = &self.audio {
                    a.play(audio::Sfx::Hurt);
                }
            }
        }
        self.prev_hp = hp;
        self.hurt_flash = (self.hurt_flash - dt * 1.8).max(0.0);
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
        });
        // what the pointer is on gets a bright rim, so it is plain what a right click would take
        let hover = if self.menu.is_none() && self.build_mode.is_none() && self.world.craft_view(self.me).is_none() {
            self.pick_node().and_then(|id| self.world.nodes.get(id.0 as usize)).map(|n| (n.tx, n.ty))
        } else {
            None
        };
        // dev aid for screenshots: rim the nearest tree when the pointer is not on anything
        let hover = hover.or_else(|| {
            std::env::var_os("ALAZ_DEMO_HOVER")?;
            let me = self.me_agent()?;
            self.world.nodes.iter().filter(|n| n.kind == alaz_sim::NodeKind::Tree).min_by_key(|n| dist2(me.x, me.y, n.center_mm().0, n.center_mm().1)).map(|n| (n.tx, n.ty))
        });
        let pointer_text = if self.menu.is_none() && self.world.craft_view(self.me).is_none() { self.pointer_text() } else { None };
        let Some(g) = self.gfx.as_mut() else { return };
        g.renderer.hover_node = hover;
        let (world, hints) = (&self.world, &self.hints);
        let net_status = match &self.net {
            Net::Solo => String::new(),
            Net::Host(h) => format!("Coop: sunucu, {} oyuncu", h.players()),
            Net::Client(c) if c.resyncing() => "Coop: eşitleniyor...".to_string(),
            Net::Client(_) => "Coop: bağlı".to_string(),
        };
        let world_ref_anchor = world.agents.get(self.me.0 as usize).map(|a| {
            let p = alaz_render::glam::Vec3::new(a.x as f32 / 1000.0, 1.9, a.y as f32 / 1000.0);
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
        };
        let mut acts = Vec::new();
        let mut knap = std::mem::take(&mut self.knap);
        let mut story = std::mem::take(&mut self.story);
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
        g.renderer.render(world, &cam, ghost, self.me, &route_of, |ctx| {
            if flash > 0.0 {
                hud::hurt_vignette(ctx, flash);
            }
            if menu.is_none() {
                let (sun, k) = alaz_render::sun_of(world.tick, 0.0, world.settings.season_days as u64);
                hud::compass(ctx, cam.yaw_deg, cam.pitch_deg, sun, k > 0.3);
            }
            match menu.as_mut() {
                Some(m) => menu_act = m.draw(ctx, &mut binds),
                None => {
                    hud::draw(ctx, world, &ui_ctx, &mut knap, &mut acts);
                    if let Some(me_a) = world.agents.get(me_id.0 as usize) {
                        story.draw(ctx, world, me_a, &binds, ui_ctx.selection.is_some());
                    }
                    if let Some(t) = &pointer_text {
                        hud::pointer_label(ctx, t);
                    }
                }
            }
        });
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
                    self.selected = None;
                }
                UiAction::ToggleTab(t) => self.toggle_tab(t),
                UiAction::ToggleGrid => self.grid_snap = !self.grid_snap,
                UiAction::Deselect => self.selected = None,
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

    /// Ground point (meters) under the mouse cursor.
    fn cursor_ground(&self) -> Option<alaz_render::glam::Vec3> {
        let g = self.gfx.as_ref()?;
        let (w, h) = g.renderer.size();
        let ndc = (2.0 * self.cursor.0 / w as f32 - 1.0, 1.0 - 2.0 * self.cursor.1 / h as f32);
        let seed = self.world.seed;
        self.cam.ground_at_height(ndc.0, ndc.1, |x, z| alaz_render::ground_height(seed, x, z))
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
            "tree" if t % 420 == 0 && !w.bodies.iter().any(|b| b.kind != alaz_sim::physics::BodyKind::Rock) => {
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
            "rock" if t % 150 == 0 => {
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
                if t % 300 == 0 {
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

    /// What the pointer is on, in words, with what a right click would do: a resource, something lying on the
    /// ground, a building, a person or an animal.
    fn pointer_text(&self) -> Option<String> {
        use alaz_sim::physics::{BodyKind, Q};
        let g = self.gfx.as_ref()?;
        if g.renderer.pointer_over_ui() || self.build_mode.is_some() {
            return None;
        }
        let w = &self.world;
        if let Some(id) = self.pick_node() {
            let n = w.nodes.get(id.0 as usize)?;
            return Some(format!("{}  ·  sağ tık: topla", hud::node_label(w, n)));
        }
        let p = self.cursor_ground()?;
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
        if let Some(b) = w.buildings.iter().find(|b| {
            let r = rules.building(b.kind).radius_mm.max(500) as i64;
            dist2(b.x, b.y, x, y) <= r * r
        }) {
            return Some(format!("{}  ·  sol tık: aç", rules.building(b.kind).name));
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
        None
    }

    /// Is (x, y) mm within `r` mm of me?
    fn near_me(&self, x: i64, y: i64, r: i64) -> bool {
        self.me_agent().is_some_and(|a| dist2(a.x, a.y, x, y) <= r * r)
    }

    /// The resource node the pointer is on. The pointer's ray is tried from high to low, so a tree is found by its
    /// crown as well as its trunk, and whatever stands nearest the eye wins.
    fn pick_node(&self) -> Option<alaz_sim::NodeId> {
        let g = self.gfx.as_ref()?;
        if g.renderer.pointer_over_ui() {
            return None;
        }
        let (w, h) = g.renderer.size();
        let ndc = (2.0 * self.cursor.0 / w as f32 - 1.0, 1.0 - 2.0 * self.cursor.1 / h as f32);
        let seed = self.world.seed;
        // walk the ray from near the eye to the ground; the first model it passes through wins. Each node is a
        // column as tall as its model really is (a sapling does not hide the reeds behind it), and a tree is only
        // its trunk below the crown, so the fibre and herbs growing round its foot can be pointed at.
        for k in (0..=32).rev() {
            let lift = k as f32 * 0.25;
            let Some(p) = self.cam.ground_at_height(ndc.0, ndc.1, |x, z| alaz_render::ground_height(seed, x, z) + lift) else { continue };
            let (tx, ty) = (p.x.floor() as i32, p.z.floor() as i32);
            let mut best: Option<(f32, alaz_sim::NodeId)> = None;
            for dy in -2..=2 {
                for dx in -2..=2 {
                    let Some(id) = self.world.node_at(tx + dx, ty + dy) else { continue };
                    let n = &self.world.nodes[id.0 as usize];
                    let (at, r, h) = alaz_render::node_bounds(&self.world, n);
                    if lift > h {
                        continue;
                    }
                    let r = if n.kind == alaz_sim::NodeKind::Tree && lift < h * 0.45 { 0.32 } else { r };
                    let d = ((p.x - at[0]).powi(2) + (p.z - at[1]).powi(2)).sqrt();
                    // a small thing is also taken anywhere on its own tile
                    let on_tile = dx == 0 && dy == 0 && n.kind != alaz_sim::NodeKind::Tree;
                    if (d <= r || on_tile) && best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, id));
                    }
                }
            }
            if let Some((_, id)) = best {
                return Some(id);
            }
        }
        None
    }

    /// Where a building would go (mm), optionally snapped to tile centers.
    fn ghost_pos(&self) -> Option<(i64, i64)> {
        self.build_mode?;
        if self.gfx.as_ref().is_some_and(|g| g.renderer.pointer_over_ui()) {
            return None;
        }
        let p = self.cursor_ground()?;
        let (x, z) = if self.grid_snap { (p.x.floor() + 0.5, p.z.floor() + 0.5) } else { (p.x, p.z) };
        Some(((x * 1000.0).round() as i64, (z * 1000.0).round() as i64))
    }

    fn placement_ok(&self, kind: BuildingKind, x: i64, y: i64) -> bool {
        let Some(a) = self.me_agent() else { return false };
        self.world.can_place(kind, x, y).is_ok()
            && dist2(a.x, a.y, x, y) <= BUILD_RADIUS_MM * BUILD_RADIUS_MM
            && a.inventory.has(&rules().building(kind).cost)
            && rules().building(kind).needs.iter().all(|&k| a.inventory.get(k) > 0)
            && self.world.knows(rules().building(kind).discovery)
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

    fn in_cave(&self) -> bool {
        self.me_agent().is_some_and(|a| matches!(a.task, alaz_sim::world::Task::Cave { .. }))
    }

    /// Left button (`primary`): walk there, or place the building being built. Right button: use what is under the
    /// pointer: open a machine's panel, gather, hunt, tame, enter a cave; nothing under it closes the panel.
    fn click(&mut self, primary: bool) {
        // clicks beside a menu window must not walk the player around (in coop the world keeps running)
        if self.me_agent().is_none() || self.menu.is_some() {
            return;
        }
        // a stray click must not throw away a craft in progress: walking away is a deliberate key press
        if self.world.craft_view(self.me).is_some() || self.in_cave() {
            return;
        }
        if let Some(kind) = self.build_mode {
            if primary {
                if let Some((x, y)) = self.ghost_pos() {
                    // the sim says why a bad spot is refused; build mode stays on so the next try is one click
                    let ok = self.placement_ok(kind, x, y);
                    self.issue(CommandKind::Build { kind, x, y });
                    if ok {
                        self.build_mode = None;
                    }
                }
            } else {
                self.build_mode = None;
            }
            return;
        }
        let Some(p) = self.cursor_ground() else { return };
        let (px, py) = ((p.x * 1000.0) as i64, (p.z * 1000.0) as i64);
        if primary {
            self.issue(CommandKind::MoveTo { x: px, y: py });
            return;
        }
        // a piece of wood lying about: pick it up (and the rest near it); a felled trunk: break it up
        if let Some(kind) = self.body_at(px, py) {
            self.issue(kind);
            return;
        }
        if let Some(i) = self.world.building_at(px, py, 300) {
            self.selected = Some(Selection::Building(i));
            return;
        }
        if let Some(i) = self.world.worker_at(px, py, 1_000) {
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
        let picked = self.pick_node().or_else(|| self.world.node_at(p.x.floor() as i32, p.z.floor() as i32));
        if let Some(node) = picked {
            if self.world.nodes[node.0 as usize].kind == alaz_sim::NodeKind::CaveMouth {
                // a cave is entered, not harvested from outside: walk up to it, then step in with a torch
                let (nx, ny) = self.world.nodes[node.0 as usize].center_mm();
                let near = self.me_agent().is_some_and(|a| dist2(a.x, a.y, nx, ny) <= 2_800 * 2_800);
                if near {
                    self.issue(CommandKind::EnterCave { node });
                } else {
                    self.say("Mağaranın ağzına yaklaş, sonra tekrar sağ tıkla. Girmek için meşale gerek.");
                    self.issue(CommandKind::MoveTo { x: nx + 1_200, y: ny + 1_800 });
                }
                return;
            }
        }
        if let Some(node) = picked {
            self.issue(CommandKind::Gather { node });
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
        let renderer = Renderer::new(window.clone());
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
                    if event.state == ElementState::Pressed {
                        let used = self.menu.as_mut().is_some_and(|m| {
                            if m.capturing() { m.on_key(code, &mut self.binds) } else { false }
                        });
                        if !event.repeat && !used {
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
            }
            WindowEvent::CursorMoved { position, .. } => {
                let now = (position.x as f32, position.y as f32);
                let shift = self.keys.contains(&winit::keyboard::KeyCode::ShiftLeft) || self.keys.contains(&winit::keyboard::KeyCode::ShiftRight);
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
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                let over_ui = self.gfx.as_ref().is_some_and(|g| g.renderer.pointer_over_ui());
                if !ui_consumed && !over_ui {
                    self.click(true);
                }
            }
            WindowEvent::MouseInput { button: MouseButton::Middle, state, .. } => {
                self.mid_drag = (state == ElementState::Pressed).then_some(self.cursor);
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Right, .. } => {
                let over_ui = self.gfx.as_ref().is_some_and(|g| g.renderer.pointer_over_ui());
                if !ui_consumed && !over_ui {
                    self.click(false);
                }
            }
            WindowEvent::MouseWheel { delta, .. } if !ui_consumed && self.menu.is_none() => {
                let s = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.cam.zoom = (self.cam.zoom * 1.1f32.powf(s)).clamp(0.2, 8.0);
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

fn main() {
    let el = EventLoop::new().expect("event loop");
    el.set_control_flow(ControlFlow::Poll);
    el.run_app(&mut App::new(parse_args())).expect("run");
}
