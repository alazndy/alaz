use crate::agents::{AgentKind, RouteStep};
use crate::buildings::{Building, BuildingKind, Job};
use crate::command::{AgentId, Command, CommandKind, NodeId};
use crate::nav::{advance, walk_free, Free, Nav, Obstacles, Step, PATHS_PER_TICK};
use crate::map::{ItemKind, Map, NodeKind, Terrain, MAP_H, MAP_W, MAX_MAP_SIDE, MIN_MAP_SIDE, TILE_MM};
use crate::rules::ItemCount;
use crate::time::{ambient_x10, TICKS_PER_DAY};
use crate::wildlife::{self, Wild, WildSpecies};
use crate::physics::{steepest_drop, LOOSE_ROCK_DROP_MM};

/// Ticks to harvest one unit (1.5 s); halved for trees with an axe.
pub(crate) const GATHER_TICKS: u32 = 90;
/// Score at which perception is complete (see `perception_permille`).
const PERCEPTION_FULL_SCORE: u32 = 50;
/// Permille thresholds of the perception levels 0..=5.
pub const PERCEPTION_STEPS: [u32; 6] = [0, 80, 220, 420, 640, 860];
/// Reach to a node center in mm.
const REACH_MM: i64 = 1500;
/// Carrying capacity of a person (other kinds: `AgentKind::capacity`).
pub const INVENTORY_CAPACITY: u16 = 40;
/// Extra room a woven bag gives a person.
pub const BAG_EXTRA: u16 = 15;
/// Animals within this distance of a pen's centre are in it.
pub const PEN_RADIUS_MM: i64 = 3_500;
/// Extra load an ox or a horse pulls in a cart.
pub const CART_EXTRA: u16 = 60;

/// An ox or a horse with a plough hitched, standing at the field at (x, y): the field is ploughed.
pub fn plow_team_at(agents: &[Agent], x: i64, y: i64) -> bool {
    agents.iter().any(|a| matches!(a.kind, AgentKind::Ox | AgentKind::Horse) && a.inventory.get(ItemKind::Plow) > 0 && dist2(a.x, a.y, x, y) <= PEN_RADIUS_MM * PEN_RADIUS_MM)
}

/// Whether two animals of `kind` stand in the pen at (x, y): breeding needs a pair.
pub fn pair_in_pen(agents: &[Agent], kind: AgentKind, x: i64, y: i64) -> bool {
    agents.iter().filter(|a| a.kind == kind && dist2(a.x, a.y, x, y) <= PEN_RADIUS_MM * PEN_RADIUS_MM).count() >= 2
}
/// Wildlife is simulated only within this many chunks (of 16 tiles) of a person or worker.
const ACTIVE_CHUNKS: i32 = 5;
const CHUNK_MM: i64 = 16 * TILE_MM;

fn chunk_of(x: i64, y: i64) -> (i32, i32) {
    (x.div_euclid(CHUNK_MM) as i32, y.div_euclid(CHUNK_MM) as i32)
}

/// Wild animals bucketed by chunk. Derived data (never saved or hashed): it is rebuilt whenever the
/// animal list changes length, and kept current as animals cross chunk borders.
#[derive(Default, Clone, Debug)]
pub(crate) struct WildIndex {
    len: usize,
    buckets: std::collections::HashMap<(i32, i32), Vec<u32>>,
}

impl WildIndex {
    fn rebuild(&mut self, wild: &[Wild]) {
        self.buckets.clear();
        for (i, w) in wild.iter().enumerate() {
            self.buckets.entry(chunk_of(w.x, w.y)).or_default().push(i as u32);
        }
        self.len = wild.len();
    }

    fn moved(&mut self, wi: u32, from: (i32, i32), to: (i32, i32)) {
        if let Some(b) = self.buckets.get_mut(&from) {
            b.retain(|&i| i != wi);
        }
        self.buckets.entry(to).or_default().push(wi);
    }
}

/// A node's share of `World::node_acc`: nothing while it is untouched, so a fresh world digests to 0.
fn node_contrib(index: u32, n: &Node) -> u64 {
    if n.amount == n.kind.initial_amount() {
        return 0;
    }
    let mut z = (index as u64) << 16 | n.amount as u64;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

/// Start index of every row in the row-major node list (length h + 1).
/// Nodes by chunk: for each chunk, the indexes of its nodes sorted by (row, column).
pub(crate) type NodeChunks = std::collections::BTreeMap<(i32, i32), Vec<u32>>;

fn node_chunk(tx: i32, ty: i32) -> (i32, i32) {
    (tx.div_euclid(crate::map::CHUNK), ty.div_euclid(crate::map::CHUNK))
}

pub(crate) fn index_nodes(nodes: &[Node]) -> NodeChunks {
    let mut chunks = NodeChunks::new();
    for (i, n) in nodes.iter().enumerate() {
        chunks.entry(node_chunk(n.tx, n.ty)).or_default().push(i as u32);
    }
    for v in chunks.values_mut() {
        v.sort_by_key(|&i| (nodes[i as usize].ty, nodes[i as usize].tx));
    }
    chunks
}

/// Index of the node on a tile (live or depleted), by binary search inside its chunk.
pub(crate) fn find_node(nodes: &[Node], chunks: &NodeChunks, tx: i32, ty: i32) -> Option<u32> {
    let v = chunks.get(&node_chunk(tx, ty))?;
    v.binary_search_by_key(&(ty, tx), |&i| (nodes[i as usize].ty, nodes[i as usize].tx)).ok().map(|k| v[k])
}

pub const MAX_STAT: i32 = 10_000;
const BERRY_FOOD: i32 = 1800;
const DRINK_PER_TICK: i32 = 40;
const TICKS_PER_SECOND_U64: u64 = crate::time::TICKS_PER_SECOND;
/// What a pot of water quenches: about a third of a full thirst bar.
const WATER_POT_DRINK: i32 = 3_500;
const FIRE_FUEL_START: u32 = 20_000;
const FIRE_FUEL_PER_WOOD: u32 = 6_000;
pub const FIRE_FUEL_MAX: u32 = 40_000;
const FIRE_WARM_RADIUS_MM: i64 = 3_500;
/// Roof warmth radius beyond the shelter footprint, and the warmth floor it guarantees.
const SHELTER_EXTRA_MM: i64 = 1_200;
pub(crate) const SHELTER_WARMTH_FLOOR: i32 = 6_500;
/// How close the agent must be to use a building / how far it can place one.
pub const USE_RADIUS_MM: i64 = 3_000;
pub const BUILD_RADIUS_MM: i64 = 8_000;
/// Per-item cap of a station's input buffer.
pub const STATION_INPUT_CAP: u16 = 60;

/// Spear reach, time to land the throw, and how long a chase may last before the prey escapes.
const SPEAR_RANGE_MM: i64 = 3_500;
const HUNT_TICKS: u32 = 20;
const HUNT_GIVE_UP_TICKS: u32 = 1_800;
/// Food values (out of 10000) and the health cost of eating raw meat.
const RAW_MEAT_FOOD: i32 = 1_500;
const COOKED_MEAT_FOOD: i32 = 4_500;
/// A loaf fills more than meat and keeps without making anyone sick.
const BREAD_FOOD: i32 = 5_000;
/// A bowl of hot lentil soup.
const SOUP_FOOD: i32 = 4_000;
/// Raw meat makes you sick for this long; wolves bite for this much and make you bleed.
const RAW_MEAT_SICK_TICKS: i32 = 5_400;
const WOLF_AGGRO_MM: i64 = 9_000;
const WOLF_DAY_AGGRO_MM: i64 = 3_500;
const WOLF_STEP_MM: i64 = 20;
const WOLF_BITE_RANGE_MM: i64 = 1_400;
const WOLF_BITE_EVERY: u64 = 150;
const WOLF_BITE_DAMAGE: i32 = 1_200;
const WOLF_BITE_BLEED: i32 = 1_800;
/// One cast of a line or a spear's wait at the water: three seconds.
const FISH_CAST_TICKS: u32 = 180;
/// How long a wolf draws off after a bite (6 s): time to run, to the fire or away.
const WOLF_DRAW_OFF_TICKS: i32 = 360;
const FIRE_SCARE_MM: i64 = 7_000;
/// A free dog starts after its person once they are this far.
const DOG_FOLLOW_MM: i64 = 8_000;
/// A craft minigame left alone this long is abandoned (nothing is lost, inputs are only taken at the end).
const CRAFT_IDLE_LIMIT: u16 = 3600;
/// Luck: a good craft may turn out fine, a failed one may be saved.
const LUCK_UPGRADE_PERCENT: u64 = 25;
const LUCK_RESCUE_PERCENT: u64 = 20;
/// A water wheel powers stations this close; powered jobs run twice as fast.

const TAME_TICKS: u32 = 120;
const TAME_GAIN: i32 = 34;
const TAME_RANGE_MM: i64 = 1_500;
/// Workers top up from a depot below this upkeep level, within this distance.
const WORKER_FEED_BELOW: i32 = 4_000;
const DEPOT_FEED_RADIUS_MM: i64 = 6_000;
const MAX_ROUTE_STEPS: usize = 32;
const ROUTE_HARVEST_TRIES: u8 = 12;
const ROUTE_WALK_TRIES: u8 = 3;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Inventory {
    #[serde(with = "crate::items_serde")]
    pub counts: [u16; ItemKind::COUNT],
}

impl Default for Inventory {
    fn default() -> Self {
        Inventory { counts: [0; ItemKind::COUNT] }
    }
}

impl Inventory {
    pub fn total(&self) -> u16 {
        self.counts.iter().sum()
    }
    pub fn get(&self, k: ItemKind) -> u16 {
        self.counts[k as usize]
    }
    /// What weighs on the pack: everything but tools, which hang on the belt or are in the hand.
    pub fn load(&self) -> u16 {
        ItemKind::ALL.iter().filter(|k| !k.is_tool()).map(|k| self.counts[*k as usize]).sum()
    }
    pub fn is_full(&self, capacity: u16) -> bool {
        self.load() >= capacity
    }
    pub fn has(&self, items: &[ItemCount]) -> bool {
        items.iter().all(|c| self.get(c.item) >= c.n)
    }
    fn take(&mut self, items: &[ItemCount]) {
        for c in items {
            self.counts[c.item as usize] -= c.n;
        }
    }
    fn give(&mut self, items: &[ItemCount]) {
        for c in items {
            self.counts[c.item as usize] += c.n;
        }
    }
}

pub const SPRINT_MIN_START: i32 = 1_500;
const SPRINT_DRAIN: i32 = 8;
const SPRINT_REGEN: i32 = 5;
pub const BREAK_TICKS: i32 = 7_200;
const INFECTION_PCT: u64 = 35;
pub const CAVE_TORCH_TICKS: u16 = 3_600;
pub const CAVE_MAX_DEPTH: u8 = 3;
pub const CAVE_MINE_TICKS: u16 = 150;
pub const CAVE_DEEPER_TICKS: u16 = 180;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Task {
    Idle,
    Move { x: i64, y: i64 },
    Gather { node: NodeId, progress: u32 },
    Drink { tx: i32, ty: i32 },
    Tame { wild: u32, progress: u32 },
    Hunt { wild: u32, progress: u32, chase: u32 },
    /// Playing a craft minigame: strikes landed so far, how many hit, ticks since the last strike.
    /// `salt` varies the target positions between crafts.
    /// `stack`/`n`: the pieces placed so far in a stacking step (centres in cm).
    Craft { recipe: u16, step: u8, done: u8, hits: u8, idle: u16, salt: u16, score: u16, stack: [(i8, u8); 8], n: u8 },
    /// Inside a cave: how deep, torch ticks left, and what is being done (`act` 0 looking around, 1 mining, 2 going deeper)
    /// with `work` ticks spent on it so far.
    Cave { node: NodeId, depth: u8, light: u16, act: u8, work: u16 },
    /// Walking straight the way the keys push (no route planning, and over an edge if there is one).
    Walk { x: i64, y: i64 },
    /// Breaking up a felled trunk on the ground (a body id); `progress` ticks into the current blow.
    Split { body: u32, progress: u32 },
    /// Picking up a piece from the ground (a body id).
    Pick { body: u32, progress: u32 },
    /// Fishing the water at (tx, ty): ticks spent on the current cast.
    Fish { tx: i32, ty: i32, progress: u32 },
}

/// Felling a standing tree takes this many rounds of chopping; it gives no wood itself. The wood is in the trunk.
const FELL_CUTS: u16 = 3;
/// A blow that breaks a piece off a lying trunk takes this share (percent) of a chopping round.
const SPLIT_SHARE: u32 = 45;
/// Ticks to stoop and pick up a piece, how close you must be, and how far around you look for the next one (mm).
pub const PICK_TICKS: u32 = 20;
const PICK_REACH_MM: i64 = 750;
const PICK_AROUND_MM: i64 = 6_000;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Agent {
    pub id: AgentId,
    pub kind: AgentKind,
    pub x: i64,
    pub y: i64,
    pub task: Task,
    pub inventory: Inventory,
    pub health: i32,
    /// People: fullness. Workers: upkeep (animals eat berries, bots burn charcoal).
    pub hunger: i32,
    pub thirst: i32,
    pub warmth: i32,
    /// Standing orders of a worker, repeated forever.
    pub route: Vec<RouteStep>,
    pub route_i: u32,
    route_wait: u32,
    route_try: u8,
    /// Bleeding and sickness: ticks left. They drain health and fade faster near warmth.
    pub injury: i32,
    /// A broken leg: ticks until it has knit. Walking is slow meanwhile.
    pub broken: i32,
    /// Running, and the breath left for it (0..=MAX_STAT).
    pub sprint: bool,
    pub stamina: i32,
    pub sick: i32,
    /// Planned route around obstacles: remaining waypoint tiles (next one last) and the goal tile it is for.
    pub path: Vec<(i32, i32)>,
    pub path_goal: (i32, i32),
    /// Animals: how many generations from the wild (a calf is its mother's plus one). Saved in the extension block.
    #[serde(skip)]
    pub generation: u8,
    /// Wear on the tools carried (uses since the last repair), by tool. Saved in the extension block.
    #[serde(skip)]
    pub wear: Vec<(ItemKind, u16)>,
    /// People: how well fed on each food group lately (grain, protein, fruit and greens), permille. A group kept
    /// up counts towards the most health the body can have. Saved in the extension block.
    #[serde(skip, default = "half_fed")]
    pub nutrition: [u16; 3],
}

fn half_fed() -> [u16; 3] {
    [600; 3]
}

/// Food groups: grain, protein, fruit and greens.
pub const NUTRITION_GROUPS: [&str; 3] = ["Tahıl", "Protein", "Meyve ve sebze"];

/// What a food gives to each group, permille.
fn food_groups(k: ItemKind) -> [u16; 3] {
    match k {
        ItemKind::Berries => [0, 0, 250],
        ItemKind::Herb => [0, 0, 60],
        ItemKind::RawMeat => [0, 200, 0],
        ItemKind::CookedMeat => [0, 350, 0],
        ItemKind::RawFish => [0, 150, 0],
        ItemKind::CookedFish => [0, 300, 0],
        ItemKind::Bread => [350, 0, 0],
        ItemKind::Soup => [150, 200, 100],
        _ => [0, 0, 0],
    }
}

impl Agent {
    /// The most health this body can have now: every food group kept up (300‰ or more) raises it.
    pub fn health_cap(&self, settings: &WorldSettings) -> i32 {
        if self.kind != AgentKind::Human {
            return MAX_STAT;
        }
        let ok = self.nutrition.iter().filter(|&&n| n >= 300).count() as i32;
        let pct = if settings.difficulty == Difficulty::Forgiving { 85 + 5 * ok } else { 70 + 10 * ok };
        MAX_STAT * pct / 100
    }
}

impl Agent {
    /// Uses on this tool since it was last mended.
    pub fn wear_of(&self, item: ItemKind) -> u16 {
        self.wear.iter().find(|w| w.0 == item).map_or(0, |w| w.1)
    }

    fn add_wear(&mut self, item: ItemKind) -> u16 {
        match self.wear.iter_mut().find(|w| w.0 == item) {
            Some(w) => {
                w.1 = w.1.saturating_add(1);
                w.1
            }
            None => {
                self.wear.push((item, 1));
                1
            }
        }
    }
}

/// Uses a tool takes before it goes blunt in this world (forgiving doubles it); 0 = never.
pub fn tool_life(t: &crate::rules::ToolDef, settings: &WorldSettings) -> u16 {
    if settings.difficulty == Difficulty::Forgiving { t.durability.saturating_mul(2) } else { t.durability }
}

#[derive(Clone, Debug)]
pub struct Node {
    pub kind: NodeKind,
    pub tx: i32,
    pub ty: i32,
    pub amount: u16,
}

impl Node {
    pub fn center_mm(&self) -> (i64, i64) {
        (self.tx as i64 * TILE_MM + TILE_MM / 2, self.ty as i64 * TILE_MM + TILE_MM / 2)
    }
}

/// Things the team has learned by doing (shared across agents).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Discovery {
    SharpStone,
    StoneAxe,
    Fire,
    Shelter,
    Kiln,
    Furnace,
    Smelting,
    Depot,
    Workshop,
    Workbench,
    Spear,
    Cooking,
    Ironwork,
    Mechanics,
    Medicine,
    Alloying,
    Bronzework,
    Clamp,
    Clothing,
    // appended: bits are stored in saves by position
    /// Wild seed in hand: it can be sown in broken ground.
    Farming,
    /// Grain in hand, with fire known: ground to flour on a stone, baked to bread.
    Milling,
    /// Clay in hand among people who farm: coiled into pots and fired.
    Pottery,
    /// A good armful of fibre, for people who dress and farm: the loom.
    Weaving,
    /// Farmers with a tamed animal and wood for a fence: a pen, and young.
    Husbandry,
    /// Bronze worked among people with an ox or a horse: the wheel, the cart and the plough.
    Draught,
    /// Two stones and a stick in hand, sharp stone known: a stone hammer.
    Hammer,
    /// Copper in hand among potters who know fire: melt it in a crucible and pour it into clay moulds.
    Casting,
    /// Never learned in play: recipes the rules keep for old saves but no longer teach.
    Retired,
    /// Iron worked, and limestone or fire clay in hand: fire brick, the blast furnace, pig iron and steel.
    Steelmaking,
    /// Steel worked, and coal in hand: the boiler, the steam engine, the pump; sand to glass.
    Steam,
    /// Steam known, and a copper ingot in hand: wire, magnets, the dynamo and the motor.
    Electricity,
    /// Electricity known, and sand and coke in hand: silicon, the transistor, the circuit board.
    Electronics,
    /// A circuit board in hand: the computer.
    Computing,
    /// Bronze worked: the prospector's pick, hidden deposits, the mine shaft (and later rock oil).
    Geology,
    /// A spear in hand by the water: fish can be had, with a spear or better with a rod.
    Fishing,
}

impl Discovery {
    fn bit(self) -> u64 {
        1 << self as u64
    }
}

/// Why a command was refused (shown to the player as a hint).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reject {
    NotLearned,
    NoMaterials,
    TooFar,
    Blocked,
    Busy,
    /// The prey got away.
    Escaped,
    /// Needs water within a few tiles.
    NeedsWater,
    /// Nothing comes of working these things together.
    NoIdea,
    /// The pack is full: nothing more can be picked up.
    PackFull,
    /// No way through to where you wanted to go.
    Unreachable,
    /// A cave needs a torch.
    NoLight,
    /// Lighting a fire needs flint in the pack.
    NoFlint,
    /// Breeding in a pen needs two animals of that kind inside it.
    NeedsPair,
    /// Only an ox or a horse can pull a cart or a plough, and each pulls one of a kind.
    CannotHitch,
    /// This resource needs a tool of this kind and at least this tier (stone 0, copper 1, bronze 2, iron 3).
    NeedsTool { tool: crate::rules::ToolKind, tier: u8 },
    /// This work is done standing at a building of this kind (forging at an anvil).
    NeedsStation(BuildingKind),
    /// The deepest chamber is under water: a working pump near the mouth keeps it dry.
    Flooded,
    /// The design table: a recipe may not give more than it takes (by value), and uses only things known.
    Unbalanced,
    /// A mine shaft goes only on a deposit: there is none within 4 m here.
    NoDeposit,
    /// No fish here: look for a school (dark shapes in the water) and fish next to it.
    NoFish,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Discovered(Discovery),
    Died(AgentId),
    Rejected(AgentId, Reject),
    JobDone { building: u32 },
    Tamed(AgentKind),
    WorkerReady(AgentKind),
    /// A hard winter killed the crop standing in a field.
    CropFrozen { building: u32 },
    /// A fish landed.
    Caught { agent: AgentId },
    /// A prospector read the rock: deposits within about 50 m, and the nearest one's offset in tiles.
    Prospected { agent: AgentId, kinds: Vec<ItemKind>, nearest: Option<(ItemKind, i32, i32)> },
    /// Food in someone's pack went bad (it turned to manure).
    Spoiled { agent: AgentId, item: ItemKind, n: u16 },
    /// A tool went blunt: it still works, slowly, until it is mended.
    ToolBlunt { agent: AgentId, item: ItemKind },
    Hunted { agent: AgentId, meat: u16 },
    Bitten(AgentId),
    /// One strike of a craft minigame landed (or missed).
    Struck { agent: AgentId, hit: bool },
    /// A minigame craft finished. quality: 0 failed, 1 good, 2 fine.
    Crafted { agent: AgentId, recipe: u16, quality: u8 },
    /// A craft was dropped: nobody worked on it for a minute, or the worker walked away. The materials stay in the bag.
    CraftAbandoned(AgentId),
    /// A mining swing in a cave brought something down on the miner's head.
    CaveRockfall(AgentId),
    /// A leg broke.
    Fractured(AgentId),
    /// A wound left open went bad.
    WoundInfected(AgentId),
    /// The torch died or the seam ran dry: the miner is back at the mouth. `dark` is true when the light ran out.
    CaveLeft { agent: AgentId, dark: bool },
    Milestone(Milestone),
    /// Came down from a height (mm from the top of the flight to the ground).
    Fell { agent: AgentId, height_mm: i32 },
    /// A felled tree hit the ground (mm: the middle of the trunk, and its length); `struck` if it came down on someone.
    TreeDown { x: i64, y: i64, size_mm: i32, struck: bool },
    /// A falling tree came down on this one.
    Crushed(AgentId),
    /// A rolling rock struck this one.
    RockHit(AgentId),
    /// A stone broke off a ledge up here (mm).
    Rockfall { x: i64, y: i64 },
    /// A falling rock struck the ground hard (mm).
    RockBounce { x: i64, y: i64 },
}

/// Goals that together make up the iron age. Completed ones are remembered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Milestone {
    FirstFire,
    Shelter,
    Companion,
    Charcoal,
    Iron,
    IronTools,
    Mechanism,
    Automaton,
    /// All of the above.
    IronAge,
    Bronze,
    BronzeTools,
    /// Bronze and bronze tools.
    BronzeAge,
    /// A stray dog came to the fire (not a goal; it only marks that the visit happened).
    Visitor,
    // Neolithic, appended so the saved bits of the others stay put (this fills all 16 bits)
    /// Grain harvested from a sown field.
    Harvest,
    /// Bread baked.
    Bread,
    /// Harvest, bread and a fired pot: the first farmers.
    NeolithicAge,
    /// A cart built.
    Wheel,
    /// A plough built.
    Plow,
    /// Came through the first winter with food in the belly.
    Winter,
    /// Something cast in copper.
    FirstMetal,
    /// A copper axe and a copper pick.
    CopperTools,
    /// First metal and copper tools: the copper age.
    CopperAge,
    /// Flour ground by a mill an animal turns.
    AnimalPower,
    /// Steel made.
    Steel,
    /// A machine run by the wind.
    WindPower,
    /// Goods carried between buildings by a belt, no hands.
    BeltLine,
    /// An automaton working a punched card (a route).
    CardBot,
    /// The steel and mechanical age.
    SteelAge,
    /// Coal from the deep.
    Coal,
    /// A machine run by steam.
    SteamPower,
    /// Down into the flooded chamber, with a pump keeping it dry.
    DrainMine,
    /// Glass made.
    Glass,
    /// The steam age.
    SteamAge,
    /// A dynamo feeding a live grid.
    Electricity,
    /// A machine run by an electric motor.
    ElectricMotor,
    /// An electric lamp lit.
    ElectricLight,
    /// The electric age.
    ElectricAge,
    /// Silicon made.
    Silicon,
    /// A transistor made.
    Transistor,
    /// A circuit board made.
    CircuitBoard,
    /// The electronic age.
    ElectronicsAge,
    /// A computer running on a live grid.
    ComputerOn,
    /// A rule of your own written at the design table.
    DesignWritten,
    /// The computer age: the way to a new world is open.
    ComputerAge,
}

impl Milestone {
    /// In the order they are meant to be reached (the guide and the goal list follow it): bronze before iron,
    /// as in history. Saves store goals by enum bit, not by this order, so it may change freely.
    pub const ALL: [Milestone; 43] = [
        Milestone::FirstFire,
        Milestone::Shelter,
        Milestone::Companion,
        Milestone::Harvest,
        Milestone::Bread,
        Milestone::Winter,
        Milestone::NeolithicAge,
        Milestone::FirstMetal,
        Milestone::CopperTools,
        Milestone::CopperAge,
        Milestone::Charcoal,
        Milestone::Bronze,
        Milestone::BronzeTools,
        Milestone::Wheel,
        Milestone::Plow,
        Milestone::AnimalPower,
        Milestone::BronzeAge,
        Milestone::Iron,
        Milestone::IronTools,
        Milestone::Mechanism,
        Milestone::Automaton,
        Milestone::IronAge,
        Milestone::Steel,
        Milestone::WindPower,
        Milestone::BeltLine,
        Milestone::CardBot,
        Milestone::SteelAge,
        Milestone::Coal,
        Milestone::SteamPower,
        Milestone::DrainMine,
        Milestone::Glass,
        Milestone::SteamAge,
        Milestone::Electricity,
        Milestone::ElectricMotor,
        Milestone::ElectricLight,
        Milestone::ElectricAge,
        Milestone::Silicon,
        Milestone::Transistor,
        Milestone::CircuitBoard,
        Milestone::ElectronicsAge,
        Milestone::ComputerOn,
        Milestone::DesignWritten,
        Milestone::ComputerAge,
    ];

    pub fn bit(self) -> u64 {
        1 << self as u64
    }
}

/// Where the craft minigame's marker is at `tick`: a triangle wave over `0..=period/2`.
fn marker_pos(tick: u64, period: u32) -> u32 {
    if period == 0 {
        return 0;
    }
    let span = period / 2;
    let p = (tick % period as u64) as u32;
    if p < span { p } else { period - p }
}

/// Target of one strike. Timing: marker span, zone center and half-width in ticks. Knap: target angle and
/// half-width in degrees, target force and half-width. Tools in the inventory widen the zone, which is
/// how better tools make delicate work easier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Geometry {
    pub span: u32,
    pub center: u32,
    pub half: u32,
    pub power: u32,
    pub power_half: u32,
    /// Fit: target point in the -100..=100 square.
    pub tx: i32,
    pub ty: i32,
}

/// Knapping keeps the stone's shape: eight sectors, each with an amount (cm) that has to come off, and the
/// amount already taken off lives in the craft's `stack` (second field). Cuts cannot be undone.
pub(crate) fn knap_needed(seed: u64, agent: AgentId, recipe: u16, step: u8, salt: u16, sector: usize) -> i32 {
    18 + (crate::rng::hash2(seed ^ 0x5A9E, agent.0 as i32 * 256 + step as i32 * 16 + sector as i32, recipe as i32 * 65_536 + salt as i32) % 23) as i32
}

fn craft_geometry(stage: u8, mg: &crate::rules::Minigame, inv: &Inventory, seed: u64, agent: AgentId, recipe: u16, step: u8, done: u8, salt: u16, stack: &[(i8, u8); 8]) -> Geometry {
    use crate::rules::MinigameKind;
    // clumsy hands at first, surer with every step the tribe has evolved: -2 at stage 0, +8 at stage 5
    let bonus: u32 = (mg.tools.iter().filter(|t| inv.get(t.item) > 0).map(|t| t.bonus).sum::<u32>() + 2 * stage as u32).saturating_sub(2);
    let n = step as i32 * 16 + done as i32;
    let h = |tag: u64| crate::rng::hash2(seed ^ tag, agent.0 as i32 * 256 + n, recipe as i32 * 65_536 + salt as i32);
    let mut g = Geometry { span: 0, center: 0, half: 0, power: 0, power_half: 0, tx: 0, ty: 0 };
    match mg.kind {
        MinigameKind::Timing => {
            let span = (mg.period / 2).max(4);
            g.span = span;
            g.half = (mg.window + bonus).min(span / 2 - 1);
            g.center = span / 4 + (h(0xC8AF) % (span as u64 / 2).max(1)) as u32;
        }
        MinigameKind::Knap => {
            g.span = 360;
            g.half = (mg.window + bonus).min(80);
            // aim at the sector with the most stone still to come off; a flake of that depth needs power = 2 x cm
            let excess = |s: usize| knap_needed(seed, agent, recipe, step, salt, s) - stack[s].1 as i32;
            let s = (0..8).fold(0, |best, s| if excess(s) > excess(best) { s } else { best });
            g.center = (s as u64 * 45 + 12 + h(0xC8AF) % 21) as u32;
            g.power = (excess(s) * 2).clamp(25, 90) as u32;
            g.power_half = 10 + g.half / 3;
        }
        MinigameKind::Fit => {
            g.half = (mg.window + bonus).min(60);
            g.tx = (h(0xF17A) % 121) as i32 - 60;
            g.ty = (h(0xF17B) % 121) as i32 - 60;
        }
        MinigameKind::Tension => {
            g.half = (mg.window + bonus).min(40);
            g.power = 30 + (h(0x7E45) % 46) as u32;
            g.power_half = g.half;
        }
        MinigameKind::Stack => {
            // half-width of the ground the pile stands on; `span` is the width of the next piece
            g.half = (mg.window + bonus).min(100);
            g.span = STACK_MIN_W + (h(0x57AC) % STACK_W_RANGE) as u32;
        }
    }
    g
}

const STACK_MIN_W: u32 = 34;
const STACK_W_RANGE: u64 = 42;

/// How well a pile holds together after dropping a piece (`w` wide, centred `c`) on it. Pieces are uniform bars,
/// so their mass is their width. Returns the smallest safety margin in percent over every joint (100 = dead
/// centre, 0 = about to tip), or None if the pile falls: the new piece must touch the one below, and for every
/// piece the centre of mass of everything above it has to rest within it (the first one within the ground).
pub(crate) fn stack_margin(stack: &[(i8, u8)], ground: u32, c: i32, w: u32) -> Option<i32> {
    let n = stack.len();
    let mut cs: Vec<i32> = stack.iter().map(|p| p.0 as i32).collect();
    let mut ws: Vec<u32> = stack.iter().map(|p| p.1 as u32).collect();
    cs.push(c);
    ws.push(w);
    if n > 0 && (cs[n] - cs[n - 1]).unsigned_abs() * 2 >= (ws[n] + ws[n - 1]) {
        return None;
    }
    let mut worst = i32::MAX;
    // the whole pile on the ground
    let (mut m, mut mc) = (0i64, 0i64);
    for i in 0..=n {
        m += ws[i] as i64;
        mc += ws[i] as i64 * cs[i] as i64;
    }
    let com = (mc / m.max(1)) as i32;
    let g = ground.max(1) as i32;
    if com.abs() > g {
        return None;
    }
    worst = worst.min((g - com.abs()) * 100 / g);
    // each piece carries everything above it
    for j in 0..n {
        let (mut m, mut mc) = (0i64, 0i64);
        for i in j + 1..=n {
            m += ws[i] as i64;
            mc += ws[i] as i64 * cs[i] as i64;
        }
        let com = (mc / m.max(1)) as i32;
        let half = (ws[j] / 2).max(1) as i32;
        let off = (com - cs[j]).abs();
        if off > half {
            return None;
        }
        worst = worst.min((half - off) * 100 / half);
    }
    Some(worst)
}


/// Angular distance in degrees (0..=180).
fn angle_diff(a: u32, b: u32) -> u32 {
    let d = (a % 360).abs_diff(b % 360);
    d.min(360 - d)
}

/// What the craft minigame looks like right now (for drawing it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CraftView {
    pub recipe: usize,
    /// Which step of the recipe is being played, and how many there are.
    pub step: u8,
    pub steps: u8,
    pub kind: crate::rules::MinigameKind,
    pub strikes: u8,
    pub done: u8,
    pub hits: u8,
    /// Timing: marker position, zone center and half-width, all within `0..=span`.
    /// Knap: `pos` is unused, `center` is the target angle in degrees and `half` its half-width.
    pub pos: u32,
    pub center: u32,
    pub half: u32,
    pub span: u32,
    /// Knap only: target force (0..=100) and its half-width.
    pub power: u32,
    pub power_half: u32,
    /// Fit only: target point in the -100..=100 square (`half` is the tolerance radius).
    pub tx: i32,
    pub ty: i32,
    /// Stack only: the pieces already placed (centre in cm, width in cm), how many, and the ground half-width
    /// (`half`); `span` is the width of the piece to drop next.
    pub stack: [(i8, u8); 8],
    pub stack_n: u8,
}

/// Ticks one unit of a gather takes for this inventory at this evolutionary stage (what `Task::Gather::progress`
/// counts up to).
pub fn gather_need(kind: NodeKind, inv: &Inventory, stage: u8) -> u32 {
    (gather_ticks(kind, inv) * (110 - 4 * stage as u32) / 100).max(1)
}

/// Ticks one blow that breaks a piece off a lying trunk takes.
pub fn split_need(inv: &Inventory, stage: u8) -> u32 {
    (gather_ticks(NodeKind::Tree, inv) * (110 - 4 * stage as u32) / 100 * SPLIT_SHARE / 100).max(1)
}

/// Ticks a spear throw takes to land once the prey is in range.
pub const HUNT_THROW_TICKS: u32 = HUNT_TICKS;

/// Seconds to harvest one unit with the tools in hand.
pub(crate) fn gather_ticks(kind: NodeKind, inv: &Inventory) -> u32 {
    let has = |k| inv.get(k) > 0;
    match kind {
        NodeKind::Tree if has(ItemKind::IronAxe) => GATHER_TICKS / 3,
        NodeKind::Tree if has(ItemKind::BronzeAxe) => GATHER_TICKS * 2 / 5,
        NodeKind::Tree if has(ItemKind::CopperAxe) => GATHER_TICKS * 9 / 20,
        NodeKind::Tree if has(ItemKind::StoneAxe) => GATHER_TICKS / 2,
        NodeKind::StoneOutcrop | NodeKind::CopperOutcrop | NodeKind::TinOutcrop if has(ItemKind::CopperPickaxe) && !has(ItemKind::BronzePickaxe) && !has(ItemKind::IronPickaxe) => GATHER_TICKS * 4 / 5,
        NodeKind::FiberPlant | NodeKind::HerbPatch if has(ItemKind::Knife) || has(ItemKind::CopperKnife) => GATHER_TICKS / 2,
        NodeKind::ClayDeposit if has(ItemKind::Shovel) => GATHER_TICKS / 2,
        NodeKind::StoneOutcrop | NodeKind::IronOutcrop | NodeKind::CopperOutcrop | NodeKind::TinOutcrop
            if has(ItemKind::IronPickaxe) =>
        {
            GATHER_TICKS / 2
        }
        NodeKind::StoneOutcrop | NodeKind::IronOutcrop | NodeKind::CopperOutcrop | NodeKind::TinOutcrop
            if has(ItemKind::BronzePickaxe) =>
        {
            GATHER_TICKS * 3 / 5
        }
        _ => GATHER_TICKS,
    }
}

/// Can this agent work this node with what it carries? Rock seams want a hammer or a pick, trees an axe, of at
/// least the gate's tier; the loose units on top of a seam come away by hand. Bots carry their own iron-tier tools.
pub fn gate_check(n: &Node, kind: AgentKind, inv: &Inventory) -> Result<(), Reject> {
    gate_check_in(crate::rules::rules(), n, kind, inv)
}

/// `gate_check` under a world's own rules.
pub fn gate_check_in(rules: &crate::rules::Rules, n: &Node, kind: AgentKind, inv: &Inventory) -> Result<(), Reject> {
    let Some(g) = rules.gate(n.kind) else { return Ok(()) };
    if g.loose > 0 && n.amount > n.kind.initial_amount().saturating_sub(g.loose) {
        return Ok(());
    }
    let tier = if kind == AgentKind::Bot { Some(3) } else { rules.tool_tier(g.tool, |k| inv.get(k) > 0) };
    if tier.is_some_and(|t| t >= g.tier) { Ok(()) } else { Err(Reject::NeedsTool { tool: g.tool, tier: g.tier }) }
}

/// Best pick tier an agent can swing (bots count as iron).
pub fn pick_tier(rules: &crate::rules::Rules, kind: AgentKind, inv: &Inventory) -> Option<u8> {
    if kind == AgentKind::Bot { Some(3) } else { rules.tool_tier(crate::rules::ToolKind::Pick, |k| inv.get(k) > 0) }
}

pub fn dist2(ax: i64, ay: i64, bx: i64, by: i64) -> i64 {
    (ax - bx) * (ax - bx) + (ay - by) * (ay - by)
}

impl Agent {
    fn with_kind(id: AgentId, kind: AgentKind, x: i64, y: i64) -> Self {
        Self {
            id,
            kind,
            x,
            y,
            task: Task::Idle,
            inventory: Inventory::default(),
            health: MAX_STAT,
            hunger: MAX_STAT,
            thirst: MAX_STAT,
            warmth: MAX_STAT,
            route: Vec::new(),
            route_i: 0,
            route_wait: 0,
            route_try: 0,
            injury: 0,
            broken: 0,
            sprint: false,
            stamina: MAX_STAT,
            sick: 0,
            path: Vec::new(),
            path_goal: (-1, -1),
            generation: 0,
            wear: Vec::new(),
            nutrition: half_fed(),
        }
    }

    fn new(id: AgentId, x: i64, y: i64) -> Self {
        Self::with_kind(id, AgentKind::Human, x, y)
    }

    /// What this agent can carry: its kind's load, plus a woven bag for a person who has one.
    pub fn capacity(&self) -> u16 {
        let bag = if self.kind == AgentKind::Human && self.inventory.get(ItemKind::Bag) > 0 { BAG_EXTRA } else { 0 };
        let cart = if matches!(self.kind, AgentKind::Ox | AgentKind::Horse) && self.inventory.get(ItemKind::Cart) > 0 { CART_EXTRA } else { 0 };
        // bred animals grow stronger: 5% more load a generation, up to ten
        let base = if matches!(self.kind, AgentKind::Ox | AgentKind::Horse | AgentKind::Dog) {
            self.kind.capacity() * (100 + 5 * self.generation.min(10) as u16) / 100
        } else {
            self.kind.capacity()
        };
        base + bag + cart
    }

    #[cfg(test)]
    pub(crate) fn route_i_for_test(&self) -> u32 {
        self.route_i
    }
}

/// Put everything a building accepts into it. Depots take all but tools; stations take recipe inputs.
fn deposit_into(rules: &crate::rules::Rules, inv: &mut Inventory, b: &mut Building) {
    if matches!(b.kind, BuildingKind::Depot | BuildingKind::Cellar) {
        for k in ItemKind::ALL {
            if !k.is_tool() {
                b.output[k as usize] += std::mem::take(&mut inv.counts[k as usize]);
            }
        }
        return;
    }
    if b.kind == BuildingKind::Field {
        b.input[ItemKind::Manure as usize] += std::mem::take(&mut inv.counts[ItemKind::Manure as usize]);
    }
    for r in rules.recipes.iter().filter(|r| r.station == Some(b.kind)) {
        if let Some(al) = &r.alloy {
            for k in [al.base, al.add] {
                let room = STATION_INPUT_CAP.saturating_sub(b.input[k as usize]);
                let n = inv.counts[k as usize].min(room);
                inv.counts[k as usize] -= n;
                b.input[k as usize] += n;
            }
        }
        for c in &r.inputs {
            let room = STATION_INPUT_CAP.saturating_sub(b.input[c.item as usize]);
            let n = inv.counts[c.item as usize].min(room);
            inv.counts[c.item as usize] -= n;
            b.input[c.item as usize] += n;
        }
    }
}

/// Take goods out of a station/depot as far as the carrier has room.
fn collect_from(inv: &mut Inventory, capacity: u16, b: &mut Building, only: Option<ItemKind>) {
    for k in ItemKind::ALL {
        if only.is_some_and(|f| f != k) {
            continue;
        }
        let room = capacity.saturating_sub(inv.load());
        let take = b.output[k as usize].min(room);
        b.output[k as usize] -= take;
        inv.counts[k as usize] += take;
    }
}

fn nearest_node(
    nodes: &[Node],
    rows: &NodeChunks,
    (mw, mh): (i32, i32),
    kind: NodeKind,
    (cx, cy, radius): (i64, i64, i64),
    (fx, fy): (i64, i64),
) -> Option<NodeId> {
    let (x0, x1) = ((cx - radius).div_euclid(TILE_MM) as i32, (cx + radius).div_euclid(TILE_MM) as i32);
    let (y0, y1) = ((cy - radius).div_euclid(TILE_MM) as i32, (cy + radius).div_euclid(TILE_MM) as i32);
    let mut best: Option<(i64, u32)> = None;
    for ty in y0.max(0)..=y1.min(mh - 1) {
        for tx in x0.max(0)..=x1.min(mw - 1) {
            let Some(i) = find_node(nodes, rows, tx, ty) else { continue };
            let n = &nodes[i as usize];
            if n.kind != kind || n.amount == 0 {
                continue;
            }
            let (nx, ny) = n.center_mm();
            if dist2(nx, ny, cx, cy) > radius * radius {
                continue;
            }
            let d = dist2(nx, ny, fx, fy);
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, i));
            }
        }
    }
    best.map(|(_, i)| NodeId(i))
}

/// How hard a world is (chosen when it is made, never changed: it is part of the state every peer shares).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Difficulty {
    /// Long jobs short, half the materials back from a failed craft, slow wear and spoilage, a cool winter.
    #[default]
    Forgiving,
    /// TerraFirmaCraft-like: long jobs, failed crafts lose everything, fast wear and spoilage, hard winters.
    Realistic,
}

/// Settings a world is made with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorldSettings {
    pub difficulty: Difficulty,
    /// Days in a season (new worlds 6; worlds from before the setting keep 3).
    pub season_days: u8,
}

impl Default for WorldSettings {
    fn default() -> Self {
        Self { difficulty: Difficulty::Forgiving, season_days: crate::time::NEW_SEASON_DAYS as u8 }
    }
}

#[derive(Clone, Debug)]
pub struct World {
    pub seed: u64,
    /// Chosen when the world was made.
    pub settings: WorldSettings,
    /// Rules this world plays by when they are not the base rules (the final's design table writes them):
    /// the source text, kept for the save, and the parsed rules.
    pub(crate) custom_rules: Option<(String, std::sync::Arc<crate::rules::Rules>)>,
    pub tick: u64,
    pub map: Map,
    pub nodes: Vec<Node>,
    /// Nodes by chunk (see `NodeChunks`).
    pub(crate) node_rows: NodeChunks,
    /// A world made in chunks: the chunks made so far, in the order they were made (their nodes were appended in
    /// that order, so a save replays it), and the same as a set.
    pub(crate) chunks_done: Vec<(i32, i32)>,
    pub(crate) chunk_set: std::collections::BTreeSet<(i32, i32)>,
    pub(crate) wild_index: WildIndex,
    pub(crate) obstacles: Obstacles,
    /// XOR of `node_contrib` over every node: an incrementally kept digest of all node amounts, so
    /// `hash()` does not have to walk millions of nodes. Derived data (rebuilt on load).
    pub(crate) node_acc: u64,
    /// Bumped when the whole node set is replaced (a load): renderers drop everything they built.
    pub node_version: u64,
    /// Per 64-tile region, when its nodes last changed (a node used up, a chunk made): renderers rebuild only
    /// what lies there. Derived, not saved.
    pub(crate) node_stamps: std::collections::HashMap<(i32, i32), u64>,
    pub(crate) node_seq: u64,
    pub buildings: Vec<Building>,
    pub agents: Vec<Agent>,
    pub wild: Vec<Wild>,
    pub spawn_mm: (i64, i64),
    pub(crate) known: u64,
    /// Items any human has ever held (bit per `ItemKind`): you can only name what you have handled.
    pub(crate) seen: u128,
    /// Total of each item ever made by recipes or stations (not gathered).
    pub produced: [u32; ItemKind::COUNT],
    pub(crate) milestones: u64,
    /// Game rule: death deletes the save and ends the run (decided by the front end).
    pub hardcore: bool,
    /// People off the ground and things falling, rolling or lying where they fell.
    pub air: Vec<crate::physics::Air>,
    pub bodies: Vec<crate::physics::Body>,
    /// Not part of the deterministic state: drained by the front end for hints and HUD.
    pub events: Vec<Event>,
}

impl World {
    pub fn new(seed: u64) -> Self {
        Self::with_size(seed, MAP_W, MAP_H)
    }

    /// The rules this world plays by: its own if it has them (written at the design table), else the base rules.
    pub fn rules(&self) -> &crate::rules::Rules {
        match &self.custom_rules {
            Some((_, r)) => r,
            None => crate::rules::rules(),
        }
    }

    /// The same rules as a shared handle, for code that changes the world while it reads them.
    pub fn rules_arc(&self) -> std::sync::Arc<crate::rules::Rules> {
        match &self.custom_rules {
            Some((_, r)) => std::sync::Arc::clone(r),
            None => crate::rules::rules_arc(),
        }
    }

    /// Give this world its own rules (TOML text in the same shape as `data/rules.toml`).
    pub fn set_custom_rules(&mut self, text: String) -> Result<(), String> {
        let parsed = crate::rules::Rules::parse(&text)?;
        self.custom_rules = Some((text, std::sync::Arc::new(parsed)));
        Ok(())
    }

    /// Season now (0 spring .. 3 winter), with this world's season length.
    pub fn season(&self) -> u8 {
        crate::time::season(self.tick, self.settings.season_days as u64)
    }

    /// Air temperature now, tenths of a degree C.
    pub fn ambient_x10(&self) -> i32 {
        crate::time::ambient_x10(self.tick, self.settings.season_days as u64)
    }

    /// The sky now.
    pub fn weather(&self) -> crate::time::Weather {
        crate::time::weather(self.seed, self.tick, self.settings.season_days as u64)
    }

    /// A world of `w` x `h` tiles (clamped to the supported range).
    pub fn with_size(seed: u64, w: i32, h: i32) -> Self {
        Self::make(seed, w, h, true)
    }

    /// A world as a save is loaded into: a world made in chunks gets none yet (the save replays them in order).
    pub(crate) fn for_loading(seed: u64, w: i32, h: i32) -> Self {
        Self::make(seed, w, h, false)
    }

    fn make(seed: u64, w: i32, h: i32, first_chunks: bool) -> Self {
        let (w, h) = (w.clamp(MIN_MAP_SIDE, MAX_MAP_SIDE), h.clamp(MIN_MAP_SIDE, MAX_MAP_SIDE));
        let map = Map::generate(seed, w, h);
        let lazy = map.lazy();
        let nodes = crate::map::par_rows(if lazy { 0 } else { w }, if lazy { 0 } else { h }, |y0, y1| {
            let mut out = Vec::new();
            for ty in y0..y1 {
                for tx in 0..w {
                    if let Some(kind) = map.node_for(seed, tx, ty) {
                        out.push(Node { kind, tx, ty, amount: kind.initial_amount() });
                    }
                }
            }
            out
        });
        let node_rows = index_nodes(&nodes);
        let mut w = Self {
            seed,
            settings: WorldSettings::default(),
            custom_rules: None,
            tick: 0,
            map,
            nodes,
            node_rows,
            chunks_done: Vec::new(),
            chunk_set: Default::default(),
            wild_index: WildIndex::default(),
            obstacles: Obstacles::default(),
            node_acc: 0,
            node_version: 0,
            node_stamps: Default::default(),
            node_seq: 0,
            buildings: Vec::new(),
            agents: Vec::new(),
            wild: Vec::new(),
            spawn_mm: (0, 0),
            known: 0,
            produced: [0; ItemKind::COUNT],
            milestones: 0,
            seen: 0,
            hardcore: false,
            air: Vec::new(),
            bodies: Vec::new(),
            events: Vec::new(),
        };
        w.spawn_mm = w.find_spawn();
        let spawn_tile = ((w.spawn_mm.0 / TILE_MM) as i32, (w.spawn_mm.1 / TILE_MM) as i32);
        w.wild = wildlife::generate(seed, &w.map, spawn_tile, 15);
        if lazy && first_chunks {
            w.chunks_around(w.spawn_mm.0, w.spawn_mm.1, true);
        }
        w
    }

    /// A start a newcomer can live from: on grass or in a forest, with trees, three stone heaps (each has only two
    /// loose stones, and the first tools take four), berries, dry branches and
    /// fibre within 20 m and water within 30 m. Searched outward from the middle on a coarse grid (newer worlds).
    fn find_good_spawn(&self) -> Option<(i64, i64)> {
        let (cx, cy) = (self.map.w / 2, self.map.h / 2);
        let seed = self.seed;
        // the cheap test first, from the raw height alone (no rivers, nothing remembered): a country to grow into,
        // not an islet — mostly land within 250 m, and rock (where the ores are) within 300 m
        let country = |tx: i32, ty: i32| -> bool {
            let h = |x: i32, y: i32| crate::map::raw_height(seed, x, y);
            let (mut land, mut all) = (0, 0);
            for dy in (-240..=240).step_by(60) {
                for dx in (-240..=240).step_by(60) {
                    all += 1;
                    land += (h(tx + dx, ty + dy) >= 95) as i32;
                }
            }
            if land * 100 < all * 50 {
                return false;
            }
            // copper crops out of rock, clay out of the sand by water: is there enough of each about to be worth
            // the close look (a sample every 10 m)
            let (mut rock, mut shore, mut sea) = (0, 0, 0);
            for dy in (-200..=200).step_by(10) {
                for dx in (-200..=200).step_by(10) {
                    let v = h(tx + dx, ty + dy);
                    rock += (v > 168) as i32;
                    if dx.abs() <= 150 && dy.abs() <= 150 {
                        shore += (95..108).contains(&v) as i32;
                        sea += (v < 95) as i32;
                    }
                }
            }
            rock >= 40 && shore >= 6 && sea >= 2
        };
        let good = |tx: i32, ty: i32| -> bool {
            if !matches!(self.map.terrain(tx, ty), Some(Terrain::Grass | Terrain::Forest)) || self.map.node_for(seed, tx, ty).is_some() {
                return false;
            }
            let mut n = [0u32; 11];
            for dy in -20..=20 {
                for dx in -20..=20 {
                    if let Some(k) = self.map.node_for(seed, tx + dx, ty + dy) {
                        n[k as usize] += 1;
                    }
                }
            }
            let has = |k: NodeKind, m: u32| n[k as usize] >= m;
            has(NodeKind::Tree, 4) && has(NodeKind::StoneOutcrop, 3) && has(NodeKind::BerryBush, 1) && has(NodeKind::Deadwood, 1) && has(NodeKind::FiberPlant, 2)
                && (-30..=30).step_by(3).any(|dy| (-30..=30).step_by(3).any(|dx| self.map.terrain(tx + dx, ty + dy) == Some(Terrain::Water)))
        };
        // clay for the first pots within 150 m, and (newer worlds, whose rock can lie farther off) copper, the first
        // metal, within 200 m: looked up in lists gathered once per searched cell, not by scanning round each spot
        let finds = |x0: i32, y0: i32, x1: i32, y1: i32| -> (Vec<(i32, i32)>, Vec<(i32, i32)>) {
            let (mut clay, mut copper) = (Vec::new(), Vec::new());
            for ty in y0..=y1 {
                for tx in x0..=x1 {
                    match self.map.node_for(seed, tx, ty) {
                        Some(NodeKind::ClayDeposit) => clay.push((tx, ty)),
                        Some(NodeKind::CopperOutcrop) => copper.push((tx, ty)),
                        _ => {}
                    }
                }
            }
            (clay, copper)
        };
        let within = |list: &[(i32, i32)], tx: i32, ty: i32, r: i32| list.iter().any(|&(x, y)| (x - tx).pow(2) + (y - ty).pow(2) <= r * r);
        // the third generator's land comes in continents: first find a 160 m cell that sits in good country, then a
        // spot inside it; the second's is everywhere, so it is searched spot by spot from the middle
        let (cell, step) = if crate::map::gen_v3(seed) { (160, 16) } else { (12, 12) };
        // one cell: the first spot in it (in a fixed order) that passes everything
        let eval_cell = |bx: i32, by: i32| -> Option<(i64, i64)> {
            if !self.map.in_bounds(bx, by) || (crate::map::gen_v3(seed) && !country(bx, by)) {
                return None;
            }
            let mut lists: Option<(Vec<(i32, i32)>, Vec<(i32, i32)>)> = None;
            for oy in (0..cell).step_by(step as usize) {
                for ox in (0..cell).step_by(step as usize) {
                    let (tx, ty) = (bx - cell / 2 + ox, by - cell / 2 + oy);
                    if !self.map.in_bounds(tx, ty) || !good(tx, ty) {
                        continue;
                    }
                    // the lists for this cell, gathered the first time a spot in it passes the rest
                    let (clay, copper) = lists.get_or_insert_with(|| finds(bx - cell / 2 - 200, by - cell / 2 - 200, bx + cell / 2 + 200, by + cell / 2 + 200));
                    if within(clay, tx, ty, 150) && (!crate::map::gen_v3(seed) || within(copper, tx, ty, 200)) {
                        return Some((tx as i64 * TILE_MM + TILE_MM / 2, ty as i64 * TILE_MM + TILE_MM / 2));
                    }
                }
            }
            None
        };
        // ring by ring out from the middle; the cells of a ring are looked at on several threads, and the first of
        // them in ring order that has a spot wins, so every machine picks the same start
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(8);
        for r in 0..(self.map.w.max(self.map.h) / 2 / cell).min(700) {
            let mut ring = Vec::new();
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dy.abs()) == r {
                        ring.push((cx + dx * cell, cy + dy * cell));
                    }
                }
            }
            let found: Vec<Option<(i64, i64)>> = if threads <= 1 || ring.len() < 4 {
                ring.iter().map(|&(bx, by)| eval_cell(bx, by)).collect()
            } else {
                let per = ring.len().div_ceil(threads);
                std::thread::scope(|sc| {
                    let parts: Vec<_> = ring.chunks(per).map(|part| sc.spawn(|| part.iter().map(|&(bx, by)| eval_cell(bx, by)).collect::<Vec<_>>())).collect();
                    parts.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
                })
            };
            if let Some(p) = found.into_iter().flatten().next() {
                return Some(p);
            }
        }
        None
    }

    /// How many chunks out from each person the world is made, in a world made in chunks (about 200 m).
    const CHUNK_REACH: i32 = 3;

    /// Make the chunks around (x, y) mm that are not made yet, nearest rows first, in a fixed order.
    pub(crate) fn chunks_around(&mut self, x: i64, y: i64, wild: bool) {
        let (cx, cy) = node_chunk(x.div_euclid(TILE_MM) as i32, y.div_euclid(TILE_MM) as i32);
        let r = Self::CHUNK_REACH;
        for dy in -r..=r {
            for dx in -r..=r {
                self.make_chunk(cx + dx, cy + dy, wild);
            }
        }
    }

    /// Make one chunk: its resource nodes (appended, so node ids stay put) and, the first time, its animals.
    pub(crate) fn make_chunk(&mut self, cx: i32, cy: i32, wild: bool) {
        let c = crate::map::CHUNK;
        if !self.map.lazy() || cx < 0 || cy < 0 || cx * c >= self.map.w || cy * c >= self.map.h || !self.chunk_set.insert((cx, cy)) {
            return;
        }
        self.chunks_done.push((cx, cy));
        let seed = self.seed;
        let spawn_tile = ((self.spawn_mm.0 / TILE_MM) as i32, (self.spawn_mm.1 / TILE_MM) as i32);
        let mut idx = Vec::new();
        for ty in cy * c..((cy + 1) * c).min(self.map.h) {
            for tx in cx * c..((cx + 1) * c).min(self.map.w) {
                if let Some(kind) = self.map.node_for(seed, tx, ty) {
                    idx.push(self.nodes.len() as u32);
                    self.nodes.push(Node { kind, tx, ty, amount: kind.initial_amount() });
                }
                if wild && (tx - spawn_tile.0).abs().max((ty - spawn_tile.1).abs()) >= 15 {
                    if let Some(species) = wildlife::wild_at(seed, &self.map, tx, ty, spawn_tile) {
                        let id = self.wild.iter().map(|w| w.id + 1).max().unwrap_or(0);
                        let (x, y) = (tx as i64 * TILE_MM + TILE_MM / 2, ty as i64 * TILE_MM + TILE_MM / 2);
                        self.wild.push(Wild::new(id, species, x, y));
                    }
                }
            }
        }
        if !idx.is_empty() {
            self.node_rows.insert((cx, cy), idx);
        }
        self.node_seq += 1;
        self.node_stamps.insert((cx, cy), self.node_seq);
    }

    /// In a world made in chunks: make what lies around everyone who walks it.
    fn grow_chunks(&mut self) {
        if !self.map.lazy() {
            return;
        }
        let at: Vec<(i64, i64)> = self.agents.iter().filter(|a| a.kind == AgentKind::Human && a.health > 0).map(|a| (a.x, a.y)).collect();
        for (x, y) in at {
            self.chunks_around(x, y, true);
        }
    }

    /// Nearest free land tile to the map center (square rings outward).
    fn find_spawn(&self) -> (i64, i64) {
        if crate::map::gen_v2(self.seed) {
            if let Some(p) = self.find_good_spawn() {
                return p;
            }
        }
        let (cx, cy) = (self.map.w / 2, self.map.h / 2);
        for r in 0..self.map.w.max(self.map.h) / 2 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dy.abs()) != r {
                        continue;
                    }
                    let (tx, ty) = (cx + dx, cy + dy);
                    let ok = matches!(self.map.terrain(tx, ty), Some(Terrain::Grass | Terrain::Sand | Terrain::Forest));
                    if ok && self.map.node_for(self.seed, tx, ty).is_none() {
                        return (tx as i64 * TILE_MM + TILE_MM / 2, ty as i64 * TILE_MM + TILE_MM / 2);
                    }
                }
            }
        }
        (cx as i64 * TILE_MM, cy as i64 * TILE_MM)
    }

    /// Wild animals inside a rectangle (mm), for culling when drawing. Uses the chunk index when it is
    /// current, so the cost follows the size of the view, not of the world.
    pub fn wild_in(&self, x0: i64, y0: i64, x1: i64, y1: i64) -> Vec<&Wild> {
        let inside = |w: &&Wild| w.x >= x0 && w.x <= x1 && w.y >= y0 && w.y <= y1;
        if self.wild_index.len != self.wild.len() {
            return self.wild.iter().filter(inside).collect();
        }
        let (c0, c1) = (chunk_of(x0, y0), chunk_of(x1, y1));
        let mut out = Vec::new();
        for cy in c0.1..=c1.1 {
            for cx in c0.0..=c1.0 {
                if let Some(b) = self.wild_index.buckets.get(&(cx, cy)) {
                    out.extend(b.iter().map(|&i| &self.wild[i as usize]).filter(inside));
                }
            }
        }
        out
    }

    /// Live nodes inside the tile rectangle `[x0, x1) x [y0, y1)` (for chunked rendering).
    pub fn nodes_in(&self, x0: i32, y0: i32, x1: i32, y1: i32) -> impl Iterator<Item = &Node> {
        let (a, b) = (node_chunk(x0, y0), node_chunk(x1 - 1, y1 - 1));
        (a.1..=b.1)
            .flat_map(move |cy| (a.0..=b.0).map(move |cx| (cx, cy)))
            .filter_map(move |c| self.node_rows.get(&c))
            .flat_map(move |v| v.iter().map(move |&i| &self.nodes[i as usize]))
            .filter(move |n| n.amount > 0 && n.tx >= x0 && n.tx < x1 && n.ty >= y0 && n.ty < y1)
    }

    /// Live (non-depleted) node on a tile.
    pub fn node_at(&self, tx: i32, ty: i32) -> Option<NodeId> {
        if !self.map.in_bounds(tx, ty) {
            return None;
        }
        find_node(&self.nodes, &self.node_rows, tx, ty)
            .filter(|&i| self.nodes[i as usize].amount > 0)
            .map(NodeId)
    }

    pub fn spawn_agent(&mut self, x: i64, y: i64) -> AgentId {
        self.spawn_worker(AgentKind::Human, x, y)
    }

    /// A new human arrives near the spawn point; ids are deterministic (next free index).
    fn join_player(&mut self) {
        let n = self.agents.iter().filter(|a| a.kind == AgentKind::Human).count() as i64;
        let (sx, sy) = self.spawn_mm;
        self.spawn_agent(sx + (n % 5 - 2) * 1_500, sy + (n / 5 % 3) * 1_500);
    }

    pub fn spawn_worker(&mut self, kind: AgentKind, x: i64, y: i64) -> AgentId {
        let id = AgentId(self.agents.len() as u32);
        self.agents.push(Agent::with_kind(id, kind, x, y));
        id
    }

    pub fn day(&self) -> u64 {
        self.tick / TICKS_PER_DAY
    }

    pub fn knows(&self, d: Discovery) -> bool {
        self.known & d.bit() != 0
    }

    /// Developer aid (`--demo`): a tribe that has learned and seen nothing yet.
    pub fn forget_everything(&mut self) {
        self.known = 0;
        self.seen = 0;
        self.milestones = 0;
    }

    /// Developer aid and tests: the tribe has learned this one thing.
    pub fn learn(&mut self, d: Discovery) {
        self.discover(d);
    }

    /// Developer aid (`--demo`): mark every discovery as known.
    pub fn learn_everything(&mut self) {
        self.known = u64::MAX;
    }

    /// The running craft minigame of an agent, if any.
    pub fn craft_view(&self, agent: AgentId) -> Option<CraftView> {
        let rules_now = self.rules_arc();
        let a = self.agents.get(agent.0 as usize)?;
        let Task::Craft { recipe, step, done, hits, salt, stack, n, .. } = a.task else { return None };
        let def = (&*rules_now).recipes.get(recipe as usize)?;
        let mg = def.steps.get(step as usize)?;
        let g = craft_geometry(self.perception_level(), mg, &a.inventory, self.seed, a.id, recipe, step, done, salt, &stack);
        Some(CraftView {
            recipe: recipe as usize,
            step,
            steps: def.steps.len() as u8,
            kind: mg.kind,
            strikes: mg.strikes,
            done,
            hits,
            pos: marker_pos(self.tick, mg.period),
            center: g.center,
            half: g.half,
            span: g.span,
            power: g.power,
            power_half: g.power_half,
            tx: g.tx,
            ty: g.ty,
            stack: if mg.kind == crate::rules::MinigameKind::Knap {
                std::array::from_fn(|i| (knap_needed(self.seed, a.id, recipe, step, salt, i) as i8, stack[i].1))
            } else {
                stack
            },
            stack_n: n,
        })
    }

    /// Bit set of items ever held (for caches that depend on what is recognised).
    pub fn seen_bits(&self) -> u128 {
        self.seen
    }

    /// Developer aid and tests: pretend this item has been held.
    pub fn mark_seen(&mut self, k: ItemKind) {
        self.seen |= 1u128 << k as u32;
    }

    /// Has anyone held this item yet?
    pub fn has_seen(&self, k: ItemKind) -> bool {
        self.seen & (1u128 << k as u32) != 0
    }

    /// How much the tribe understands of the world, 0..=1000, from what it has discovered, achieved and handled.
    /// It sharpens how things look and how they are named. Derived, never stored.
    pub fn perception_permille(&self) -> u32 {
        let score = self.known.count_ones() + 2 * self.milestones.count_ones() + self.seen.count_ones() / 2;
        (score * 1000 / PERCEPTION_FULL_SCORE).min(1000)
    }

    /// Stages of perception 0..=5, from the animal view to the modern one.
    pub fn perception_level(&self) -> u8 {
        let p = self.perception_permille();
        PERCEPTION_STEPS.iter().rposition(|&t| p >= t).unwrap_or(0) as u8
    }

    /// Note that the nodes around (tx, ty) changed.
    fn touch_nodes(&mut self, tx: i32, ty: i32) {
        self.node_seq += 1;
        self.node_stamps.insert(node_chunk(tx, ty), self.node_seq);
    }

    /// When the nodes of the 64-tile region holding (tx, ty) last changed, with the whole set's version: a
    /// renderer's mesh of that ground is current while this is unchanged.
    pub fn node_stamp(&self, tx: i32, ty: i32) -> (u64, u64) {
        (self.node_version, self.node_stamps.get(&node_chunk(tx, ty)).copied().unwrap_or(0))
    }

    /// Mark a milestone reached (tests and tools in other crates).
    #[doc(hidden)]
    pub fn debug_reach(&mut self, m: Milestone) {
        self.milestones |= m.bit();
    }

    /// Every milestone reached, as bits (`Milestone::bit`).
    pub fn milestone_bits(&self) -> u64 {
        self.milestones
    }

    pub fn milestone_done(&self, m: Milestone) -> bool {
        self.milestones & m.bit() != 0
    }

    /// Set a node's remaining amount (used when loading a save and by tests), keeping the digest right.
    pub fn set_node_amount(&mut self, index: usize, amount: u16) {
        self.node_acc ^= node_contrib(index as u32, &self.nodes[index]);
        self.nodes[index].amount = amount;
        self.node_acc ^= node_contrib(index as u32, &self.nodes[index]);
        let (tx, ty) = (self.nodes[index].tx, self.nodes[index].ty);
        self.touch_nodes(tx, ty);
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// Is a lit campfire within `radius_mm` of the point?
    pub fn near_fire(&self, x: i64, y: i64, radius_mm: i64) -> bool {
        self.buildings
            .iter()
            .any(|b| b.kind.burns() && b.fuel > 0 && dist2(b.x, b.y, x, y) <= radius_mm * radius_mm)
    }

    /// Building whose footprint (plus a margin) contains the point.
    pub fn building_at(&self, x: i64, y: i64, margin_mm: i64) -> Option<usize> {
        let rules_now = self.rules_arc();
        let r = &*rules_now;
        self.buildings.iter().position(|b| {
            let rad = r.building(b.kind).radius_mm + margin_mm;
            dist2(b.x, b.y, x, y) <= rad * rad
        })
    }

    /// A worker (non-human agent) near the point, if any.
    pub fn worker_at(&self, x: i64, y: i64, radius_mm: i64) -> Option<usize> {
        self.agents
            .iter()
            .position(|a| a.kind.is_worker() && dist2(a.x, a.y, x, y) <= radius_mm * radius_mm)
    }

    /// A wild animal near the point, if any.
    pub fn wild_at(&self, x: i64, y: i64, radius_mm: i64) -> Option<u32> {
        self.wild.iter().find(|w| dist2(w.x, w.y, x, y) <= radius_mm * radius_mm).map(|w| w.id)
    }

    fn discover(&mut self, d: Discovery) {
        if !self.knows(d) {
            self.known |= d.bit();
            self.events.push(Event::Discovered(d));
        }
    }

    /// Terrain, overlap and resource checks for placing a building. Ignores cost and reach.
    pub fn can_place(&self, kind: BuildingKind, x: i64, y: i64) -> Result<(), Reject> {
        let rules_now = self.rules_arc();
        let r = (&*rules_now).building(kind).radius_mm;
        if kind == BuildingKind::WaterWheel
            && !self.map.water_within(x.div_euclid(TILE_MM) as i32, y.div_euclid(TILE_MM) as i32, 3)
        {
            return Err(Reject::NeedsWater);
        }
        for (ox, oy) in [(0, 0), (r, 0), (-r, 0), (0, r), (0, -r)] {
            match self.map.terrain_mm(x + ox, y + oy) {
                None | Some(Terrain::Water) => return Err(Reject::Blocked),
                _ => {}
            }
        }
        let defs = &*rules_now;
        for b in &self.buildings {
            let min = r + defs.building(b.kind).radius_mm + 200;
            if dist2(b.x, b.y, x, y) < min * min {
                return Err(Reject::Blocked);
            }
        }
        let (t0x, t1x) = ((x - r).div_euclid(TILE_MM) as i32, (x + r).div_euclid(TILE_MM) as i32);
        let (t0y, t1y) = ((y - r).div_euclid(TILE_MM) as i32, (y + r).div_euclid(TILE_MM) as i32);
        for ty in t0y - 1..=t1y + 1 {
            for tx in t0x - 1..=t1x + 1 {
                if let Some(id) = self.node_at(tx, ty) {
                    let (nx, ny) = self.nodes[id.0 as usize].center_mm();
                    let min = r + 400;
                    if dist2(nx, ny, x, y) < min * min {
                        return Err(Reject::Blocked);
                    }
                }
            }
        }
        Ok(())
    }

    fn reject(&mut self, agent: AgentId, why: Reject) {
        self.events.push(Event::Rejected(agent, why));
    }

    fn apply_command(&mut self, c: &Command) {
        let rules_now = self.rules_arc();
        let idx = c.agent.0 as usize;
        if idx >= self.agents.len() {
            return;
        }
        let id = c.agent;
        let (ax, ay) = (self.agents[idx].x, self.agents[idx].y);
        let kind_of_agent = self.agents[idx].kind;
        let human = kind_of_agent == AgentKind::Human;
        match &c.kind {
            CommandKind::Join => {}
            CommandKind::MoveTo { x, y } => {
                if matches!(self.agents[idx].task, Task::Craft { .. }) && human {
                    self.events.push(Event::CraftAbandoned(id));
                }
                self.agents[idx].task = Task::Move { x: *x, y: *y }
            }
            CommandKind::Walk { x, y } => {
                if matches!(self.agents[idx].task, Task::Craft { .. }) && human {
                    self.events.push(Event::CraftAbandoned(id));
                }
                if !matches!(self.agents[idx].task, Task::Cave { .. }) {
                    self.agents[idx].task = Task::Walk { x: *x, y: *y }
                }
            }
            CommandKind::Jump => self.jump(idx),
            CommandKind::Split { body } => {
                if kind_of_agent.can_harvest() && self.bodies.iter().any(|b| b.id == *body && b.kind == crate::physics::BodyKind::Tree && b.wood > 0) {
                    self.agents[idx].task = Task::Split { body: *body, progress: 0 };
                }
            }
            CommandKind::Pick { body } => {
                if kind_of_agent.can_harvest() && self.bodies.iter().any(|b| b.id == *body && b.kind == crate::physics::BodyKind::Piece) {
                    self.agents[idx].task = Task::Pick { body: *body, progress: 0 };
                }
            }
            CommandKind::Gather { node } => {
                if kind_of_agent.can_harvest() && self.nodes.get(node.0 as usize).is_some_and(|n| n.amount > 0) {
                    if let Err(why) = gate_check_in(&*rules_now, &self.nodes[node.0 as usize], kind_of_agent, &self.agents[idx].inventory) {
                        return self.reject(id, why);
                    }
                    self.agents[idx].task = Task::Gather { node: *node, progress: 0 };
                }
            }
            CommandKind::EnterCave { node } => {
                let near = self.nodes.get(node.0 as usize).filter(|n| n.kind == NodeKind::CaveMouth && n.amount > 0).map(|n| n.center_mm());
                let a = &mut self.agents[idx];
                match near {
                    _ if !human => {}
                    None => {}
                    Some((nx, ny)) if dist2(a.x, a.y, nx, ny) > 3_000 * 3_000 => self.events.push(Event::Rejected(id, Reject::TooFar)),
                    Some(_) if a.inventory.get(ItemKind::Torch) == 0 => self.events.push(Event::Rejected(id, Reject::NoLight)),
                    Some(_) => {
                        a.inventory.counts[ItemKind::Torch as usize] -= 1;
                        a.task = Task::Cave { node: *node, depth: 0, light: CAVE_TORCH_TICKS, act: 0, work: 0 };
                    }
                }
            }
            CommandKind::CaveMine | CommandKind::CaveDeeper => {
                if let Task::Cave { node, depth, light, act: 0, .. } = self.agents[idx].task {
                    let deeper = matches!(c.kind, CommandKind::CaveDeeper);
                    // mining wants a hammer or a pick; each chamber further down wants a harder one
                    let need = if deeper { (&*rules_now).cave_tier(depth + 1) } else { 0 };
                    let have = pick_tier(&*rules_now, self.agents[idx].kind, &self.agents[idx].inventory);
                    // the deepest chamber is under water: it opens only with a working pump within 15 m of the mouth
                    let mouth = self.nodes.get(node.0 as usize).map(|n| n.center_mm()).unwrap_or((ax, ay));
                    let pumped = self.buildings.iter().filter(|p| p.kind == BuildingKind::Pump && dist2(p.x, p.y, mouth.0, mouth.1) <= 15_000 * 15_000).any(|p| self.powered_at(p.x, p.y));
                    if deeper && depth + 1 == CAVE_MAX_DEPTH && !pumped {
                        self.events.push(Event::Rejected(id, Reject::Flooded));
                    } else if have.is_none_or(|t| t < need) && (!deeper || depth < CAVE_MAX_DEPTH) {
                        self.events.push(Event::Rejected(id, Reject::NeedsTool { tool: crate::rules::ToolKind::Pick, tier: need }));
                    } else if !deeper && self.agents[idx].inventory.is_full(self.agents[idx].capacity()) {
                        self.events.push(Event::Rejected(id, Reject::PackFull));
                    } else if !deeper || depth < CAVE_MAX_DEPTH {
                        if deeper && depth + 1 == CAVE_MAX_DEPTH && self.milestones & Milestone::DrainMine.bit() == 0 {
                            self.milestones |= Milestone::DrainMine.bit();
                            self.events.push(Event::Milestone(Milestone::DrainMine));
                        }
                        self.agents[idx].task = Task::Cave { node, depth, light, act: if deeper { 2 } else { 1 }, work: 0 };
                    }
                }
            }
            CommandKind::SetSprint { on } => {
                let a = &mut self.agents[idx];
                if human {
                    // winded people cannot start running until they have caught some breath
                    a.sprint = *on && a.stamina >= SPRINT_MIN_START && a.broken == 0;
                }
            }
            CommandKind::CaveLeave => {
                if matches!(self.agents[idx].task, Task::Cave { .. }) {
                    self.agents[idx].task = Task::Idle;
                    self.events.push(Event::CaveLeft { agent: id, dark: false });
                }
            }
            CommandKind::Fish { tx, ty } => {
                if !human || self.map.terrain(*tx, *ty) != Some(Terrain::Water) {
                    return;
                }
                let inv = &self.agents[idx].inventory;
                if inv.get(ItemKind::FishingRod) == 0 && inv.get(ItemKind::Spear) == 0 {
                    return self.reject(id, Reject::NoMaterials);
                }
                match crate::map::school_near(&self.map, *tx, *ty, 3) {
                    Some(_) => self.agents[idx].task = Task::Fish { tx: *tx, ty: *ty, progress: 0 },
                    None => self.reject(id, Reject::NoFish),
                }
            }
            CommandKind::Drink { tx, ty } => {
                if human && self.map.terrain(*tx, *ty) == Some(Terrain::Water) {
                    self.agents[idx].task = Task::Drink { tx: *tx, ty: *ty };
                }
            }
            CommandKind::Eat { item: ItemKind::Splint } => {
                let a = &mut self.agents[idx];
                if human && a.broken > 0 && a.inventory.get(ItemKind::Splint) > 0 {
                    a.inventory.counts[ItemKind::Splint as usize] -= 1;
                    // bound up, the bone knits in a third of the time
                    a.broken /= 3;
                    a.health = (a.health + 300).min(MAX_STAT);
                }
            }
            CommandKind::Eat { item } if matches!(item, ItemKind::Herb | ItemKind::Bandage) => {
                let a = &mut self.agents[idx];
                let n = &mut a.inventory.counts[*item as usize];
                if human && *n > 0 {
                    *n -= 1;
                    if *item == ItemKind::Herb {
                        // bitter herbs settle the stomach
                        a.sick = (a.sick - 3_000).max(0);
                        a.health = (a.health + 300).min(MAX_STAT);
                    } else {
                        a.injury = 0;
                        a.health = (a.health + 600).min(MAX_STAT);
                    }
                }
            }
            CommandKind::Eat { item: ItemKind::WaterPot } => {
                let a = &mut self.agents[idx];
                if human && a.inventory.counts[ItemKind::WaterPot as usize] > 0 && a.thirst < MAX_STAT {
                    a.inventory.counts[ItemKind::WaterPot as usize] -= 1;
                    a.inventory.counts[ItemKind::Pot as usize] += 1;
                    a.thirst = (a.thirst + WATER_POT_DRINK).min(MAX_STAT);
                }
            }
            CommandKind::Eat { item } => {
                let food = match item {
                    ItemKind::Berries => BERRY_FOOD,
                    ItemKind::RawMeat => RAW_MEAT_FOOD,
                    ItemKind::CookedMeat => COOKED_MEAT_FOOD,
                    ItemKind::RawFish => RAW_MEAT_FOOD * 2 / 3,
                    ItemKind::CookedFish => COOKED_MEAT_FOOD * 4 / 5,
                    ItemKind::Bread => BREAD_FOOD,
                    ItemKind::Soup => SOUP_FOOD,
                    _ => return,
                };
                let a = &mut self.agents[idx];
                let n = &mut a.inventory.counts[*item as usize];
                if human && *n > 0 && a.hunger < MAX_STAT {
                    *n -= 1;
                    a.hunger = (a.hunger + food).min(MAX_STAT);
                    for (g, add) in food_groups(*item).iter().enumerate() {
                        a.nutrition[g] = (a.nutrition[g] + add).min(1000);
                    }
                    match item {
                        // raw meat makes you sick; a hot meal helps you get over it
                        ItemKind::RawMeat => a.sick = a.sick.max(RAW_MEAT_SICK_TICKS),
                        ItemKind::CookedMeat | ItemKind::CookedFish => a.sick /= 2,
                        // raw fish less so than raw meat
                        ItemKind::RawFish => a.sick = a.sick.max(RAW_MEAT_SICK_TICKS / 2),
                        _ => {}
                    }
                }
            }
            CommandKind::Hunt { wild } => {
                let prey = self.wild.iter().any(|w| w.id == *wild && w.species.huntable());
                if !human || !prey {
                    return;
                }
                if self.agents[idx].inventory.get(ItemKind::Spear) == 0 {
                    return self.reject(id, Reject::NoMaterials);
                }
                self.agents[idx].task = Task::Hunt { wild: *wild, progress: 0, chase: 0 };
            }
            CommandKind::Combine { items } => {
                if !human {
                    return;
                }
                let mut want = [0u16; ItemKind::COUNT];
                let mut any = false;
                for &i in items.iter() {
                    if let Some(k) = ItemKind::ALL.get(i as usize) {
                        want[*k as usize] += 1;
                        any = true;
                    }
                }
                if !any {
                    return;
                }
                let short = (0..ItemKind::COUNT).any(|k| want[k] > self.agents[idx].inventory.counts[k]);
                if short {
                    return self.reject(id, Reject::NoMaterials);
                }
                let found = (&*rules_now).recipes.iter().position(|r| {
                    r.station.is_none() && r.ticks == 0 && {
                        let mut need = [0u16; ItemKind::COUNT];
                        for i in &r.inputs {
                            need[i.item as usize] += i.n;
                        }
                        need == want
                    }
                });
                match found {
                    Some(r) => {
                        let d = (&*rules_now).recipes[r].discovery;
                        self.discover(d);
                        self.apply_command(&Command { agent: id, kind: CommandKind::Craft { recipe: crate::command::RecipeId(r as u16) } });
                    }
                    None => self.reject(id, Reject::NoIdea),
                }
            }
            CommandKind::Craft { recipe } => {
                let Some(def) = (&*rules_now).recipes.get(recipe.0 as usize) else { return };
                if !human || def.station.is_some() || def.ticks != 0 {
                    return;
                }
                if !self.knows(def.discovery) {
                    return self.reject(id, Reject::NotLearned);
                }
                let inv = &mut self.agents[idx].inventory;
                if !inv.has(&def.inputs) {
                    return self.reject(id, Reject::NoMaterials);
                }
                if def.needs.iter().any(|&k| inv.get(k) == 0) {
                    return self.reject(id, if def.needs.contains(&ItemKind::Flint) { Reject::NoFlint } else { Reject::NoMaterials });
                }
                if let Some(at) = def.near {
                    let close = self.buildings.iter().any(|b| b.kind == at && dist2(b.x, b.y, ax, ay) <= USE_RADIUS_MM * USE_RADIUS_MM);
                    if !close {
                        return self.reject(id, Reject::NeedsStation(at));
                    }
                }
                let inv = &mut self.agents[idx].inventory;
                if def.steps.iter().any(|m| m.strikes > 0) {
                    // played as minigames; materials are only used up when the last step ends
                    self.agents[idx].task = Task::Craft { recipe: recipe.0, step: 0, done: 0, hits: 0, idle: 0, salt: (self.tick & 0xffff) as u16, score: 0, stack: [(0, 0); 8], n: 0 };
                    return;
                }
                inv.take(&def.inputs);
                inv.give(&def.outputs);
                for o in &def.outputs {
                    self.produced[o.item as usize] += o.n as u32;
                }
            }
            CommandKind::Strike | CommandKind::StrikeAt { .. } | CommandKind::PlaceAt { .. } | CommandKind::Tighten { .. } => {
                if !human {
                    return;
                }
                let Task::Craft { recipe, step, done, hits, salt, score, stack, n, .. } = self.agents[idx].task else { return };
                let (mut stack, mut n) = (stack, n);
                let def = &(&*rules_now).recipes[recipe as usize];
                let Some(mg) = def.steps.get(step as usize) else { return };
                let g = craft_geometry(self.perception_level(), mg, &self.agents[idx].inventory, self.seed, id, recipe, step, done, salt, &stack);
                use crate::rules::MinigameKind;
                // how well the strike went: 100 dead on, 60 inside the zone, 25 just off, 0 miss
                let band = |off: u32, half: u32| -> u16 {
                    if off * 2 <= half {
                        100
                    } else if off <= half {
                        60
                    } else if off * 10 <= half * 16 {
                        25
                    } else {
                        0
                    }
                };
                let points: u16 = match (&c.kind, mg.kind) {
                    (CommandKind::Strike, MinigameKind::Timing) => {
                        if marker_pos(self.tick, mg.period).abs_diff(g.center) <= g.half { 100 } else { 0 }
                    }
                    (CommandKind::StrikeAt { angle, power }, MinigameKind::Knap) => {
                        // the flake comes off the sector that was aimed at; too deep a cut ruins it for good
                        // a strike close enough to the mark lands on the intended facet, a wild one on whatever it hit
                        let near = angle_diff(*angle as u32, g.center) * 10 <= g.half * 16;
                        let sector = (if near { g.center as usize } else { *angle as usize % 360 }) / 45;
                        let depth = (*power).min(100) as i32 / 2;
                        let excess = knap_needed(self.seed, id, recipe, step, salt, sector) - stack[sector].1 as i32;
                        let ruined = depth > excess + 8;
                        let (ea, ep) = (angle_diff(*angle as u32, g.center), (*power as u32).min(100).abs_diff(g.power));
                        let p = if ruined {
                            0
                        } else if ea <= g.half / 2 && ep <= g.power_half / 2 {
                            100
                        } else if ea <= g.half && ep <= g.power_half {
                            60
                        } else if ea * 10 <= g.half * 16 && ep * 10 <= g.power_half * 16 {
                            25
                        } else {
                            0
                        };
                        let taken = if p >= 60 { depth } else if p > 0 { depth / 2 } else { depth / 3 };
                        stack[sector].1 = (stack[sector].1 as i32 + taken).min(255) as u8;
                        p
                    }
                    (CommandKind::PlaceAt { x, y }, MinigameKind::Fit) => {
                        let (dx, dy) = ((*x).clamp(-100, 100) as i64 - g.tx as i64, (*y).clamp(-100, 100) as i64 - g.ty as i64);
                        let d2 = dx * dx + dy * dy;
                        let h = g.half as i64;
                        if d2 * 4 <= h * h {
                            100
                        } else if d2 <= h * h {
                            60
                        } else if d2 * 100 <= h * h * 256 {
                            25
                        } else {
                            0
                        }
                    }
                    (CommandKind::Tighten { power }, MinigameKind::Tension) => band((*power as u32).min(100).abs_diff(g.power), g.power_half),
                    (CommandKind::PlaceAt { x, .. }, MinigameKind::Stack) => {
                        // drop the next piece at this sideways position; it either holds or the pile gives way
                        let c = (*x).clamp(-100, 100) as i32;
                        match stack_margin(&stack[..n as usize], g.half, c, g.span) {
                            None => 0,
                            Some(m) => {
                                if (n as usize) < stack.len() {
                                    stack[n as usize] = (c as i8, g.span as u8);
                                    n += 1;
                                }
                                if m >= 45 {
                                    100
                                } else if m >= 20 {
                                    60
                                } else {
                                    25
                                }
                            }
                        }
                    }
                    _ => return, // the wrong kind of input for this step
                };
                let hit = points >= 60;
                self.events.push(Event::Struck { agent: id, hit });
                let (done, hits, score) = (done + 1, hits + hit as u8, score + points);
                if done < mg.strikes {
                    self.agents[idx].task = Task::Craft { recipe, step, done, hits, idle: 0, salt, score, stack, n };
                    return;
                }
                if (step as usize) + 1 < def.steps.len() {
                    self.agents[idx].task = Task::Craft { recipe, step: step + 1, done: 0, hits, idle: 0, salt, score, stack: [(0, 0); 8], n: 0 };
                    return;
                }
                self.agents[idx].task = Task::Idle;
                if !self.agents[idx].inventory.has(&def.inputs) {
                    return self.reject(id, Reject::NoMaterials);
                }
                // average points per strike over every step: 92+ is fine work, 60+ is good
                let avg = score as u32 / def.total_strikes().max(1);
                let mut quality = if avg >= 92 { 2 } else if avg >= 60 { 1 } else { 0 };
                let roll = crate::rng::hash2(self.seed ^ 0xC4AF7, id.0 as i32, (self.tick % 1_000_000) as i32) % 100;
                if quality == 1 && roll < LUCK_UPGRADE_PERCENT {
                    quality = 2;
                } else if quality == 0 && roll < LUCK_RESCUE_PERCENT {
                    quality = 1;
                }
                let forgiving = self.settings.difficulty == Difficulty::Forgiving;
                // a clay mould cracks now and then (one cast in three when forgiving), always when it is realistic
                let cracks = crate::rng::hash2(self.seed ^ 0x3017D, id.0 as i32, (self.tick % 1_000_000) as i32) % 1000 < if forgiving { 333 } else { 1000 };
                let inv = &mut self.agents[idx].inventory;
                inv.take(&def.inputs);
                if quality > 0 {
                    inv.give(&def.outputs);
                    for o in &def.outputs {
                        self.produced[o.item as usize] += o.n as u32;
                    }
                    if quality == 2 && def.fine_bonus > 0 {
                        if let Some(first) = def.outputs.first() {
                            inv.give(&[ItemCount { item: first.item, n: def.fine_bonus }]);
                            self.produced[first.item as usize] += def.fine_bonus as u32;
                        }
                    }
                    if let Some(m) = def.mold.filter(|_| !cracks) {
                        inv.give(&[ItemCount { item: m, n: 1 }]);
                    }
                } else if !def.fail_returns.is_empty() {
                    inv.give(&def.fail_returns);
                } else if forgiving {
                    // a botched job is not all waste: about half the materials can be used again
                    let back: Vec<ItemCount> = def.inputs.iter().map(|c| ItemCount { item: c.item, n: c.n / 2 }).filter(|c| c.n > 0).collect();
                    inv.give(&back);
                }
                self.events.push(Event::Crafted { agent: id, recipe, quality });
            }
            CommandKind::Build { kind, x, y } => {
                let (kind, x, y) = (*kind, *x, *y);
                let def = (&*rules_now).building(kind);
                if !human {
                    return;
                }
                if kind == BuildingKind::MineShaft && deposits_near(self.seed, (x / 1000) as i32, (y / 1000) as i32, 4).is_empty() {
                    return self.reject(id, Reject::NoDeposit);
                }
                if !self.knows(def.discovery) {
                    return self.reject(id, Reject::NotLearned);
                }
                if dist2(ax, ay, x, y) > BUILD_RADIUS_MM * BUILD_RADIUS_MM {
                    return self.reject(id, Reject::TooFar);
                }
                if !self.agents[idx].inventory.has(&def.cost) {
                    return self.reject(id, Reject::NoMaterials);
                }
                if def.needs.iter().any(|&k| self.agents[idx].inventory.get(k) == 0) {
                    return self.reject(id, if def.needs.contains(&ItemKind::Flint) { Reject::NoFlint } else { Reject::NoMaterials });
                }
                if let Err(why) = self.can_place(kind, x, y) {
                    return self.reject(id, why);
                }
                self.agents[idx].inventory.take(&def.cost);
                let fuel = if kind.burns() { FIRE_FUEL_START } else { 0 };
                self.buildings.push(Building { fuel, ..Building::new(kind, x, y) });
                self.obstacles.block_footprint(&self.map, x, y, def.radius_mm);
                self.obstacles.built_for = self.buildings.len();
            }
            CommandKind::StartJob { building, recipe } => {
                let Some(def) = (&*rules_now).recipes.get(recipe.0 as usize) else { return };
                let Some(b) = self.buildings.get_mut(*building as usize) else { return };
                if !human || def.station != Some(b.kind) {
                    return;
                }
                if dist2(ax, ay, b.x, b.y) > USE_RADIUS_MM * USE_RADIUS_MM {
                    return self.reject(id, Reject::TooFar);
                }
                if b.job.is_some() {
                    return self.reject(id, Reject::Busy);
                }
                if self.known & def.discovery.bit() == 0 {
                    return self.reject(id, Reject::NotLearned);
                }
                if let Some(kind) = def.spawns.filter(|_| b.kind == BuildingKind::Pen) {
                    if !pair_in_pen(&self.agents, kind, b.x, b.y) {
                        return self.reject(id, Reject::NeedsPair);
                    }
                }
                // what was loaded into the station counts first, the pack makes up the rest
                let inv = &mut self.agents[idx].inventory;
                if !def.inputs.iter().all(|c| b.input[c.item as usize] + inv.get(c.item) >= c.n) {
                    return self.reject(id, Reject::NoMaterials);
                }
                for c in &def.inputs {
                    let from_station = b.input[c.item as usize].min(c.n);
                    b.input[c.item as usize] -= from_station;
                    inv.counts[c.item as usize] -= c.n - from_station;
                }
                b.job = Some(Job { recipe: recipe.0, remaining: def.ticks });
            }
            CommandKind::Deposit { building } => {
                let Some(b) = self.buildings.get_mut(*building as usize) else { return };
                if dist2(ax, ay, b.x, b.y) > USE_RADIUS_MM * USE_RADIUS_MM {
                    return self.reject(id, Reject::TooFar);
                }
                deposit_into(&*rules_now, &mut self.agents[idx].inventory, b);
            }
            CommandKind::Collect { building, item } => {
                let cap = self.agents[idx].capacity();
                let Some(b) = self.buildings.get_mut(*building as usize) else { return };
                if dist2(ax, ay, b.x, b.y) > USE_RADIUS_MM * USE_RADIUS_MM {
                    return self.reject(id, Reject::TooFar);
                }
                collect_from(&mut self.agents[idx].inventory, cap, b, *item);
            }
            CommandKind::SetAuto { building, recipe } => {
                let Some(b) = self.buildings.get_mut(*building as usize) else { return };
                match recipe {
                    None => b.auto = None,
                    Some(r) => {
                        let Some(def) = (&*rules_now).recipes.get(r.0 as usize) else { return };
                        if def.station != Some(b.kind) {
                            return;
                        }
                        if self.known & def.discovery.bit() == 0 {
                            return self.reject(id, Reject::NotLearned);
                        }
                        b.auto = Some(r.0);
                    }
                }
            }
            CommandKind::Refuel { building } => {
                let Some(b) = self.buildings.get_mut(*building as usize) else { return };
                if !b.kind.burns() {
                    return;
                }
                if dist2(ax, ay, b.x, b.y) > USE_RADIUS_MM * USE_RADIUS_MM {
                    return self.reject(id, Reject::TooFar);
                }
                // embers keep a fire going; a dead one has to be struck alight again
                if b.fuel == 0 && self.agents[idx].inventory.get(ItemKind::Flint) == 0 {
                    return self.reject(id, Reject::NoFlint);
                }
                let wood = &mut self.agents[idx].inventory.counts[ItemKind::Wood as usize];
                if *wood == 0 {
                    return self.reject(id, Reject::NoMaterials);
                }
                // as much wood as the fire will take (one log at least), not one log a click
                let room = (FIRE_FUEL_MAX.saturating_sub(b.fuel) / FIRE_FUEL_PER_WOOD).max(1) as u16;
                let n = room.min(*wood);
                *wood -= n;
                b.fuel = (b.fuel + FIRE_FUEL_PER_WOOD * n as u32).min(FIRE_FUEL_MAX);
            }
            CommandKind::Tame { wild } => {
                if !human || !self.wild.iter().any(|w| w.id == *wild && w.species.tamed().is_some()) {
                    return;
                }
                if self.agents[idx].inventory.get(ItemKind::Berries) == 0 {
                    return self.reject(id, Reject::NoMaterials);
                }
                self.agents[idx].task = Task::Tame { wild: *wild, progress: 0 };
            }
            CommandKind::Feed { target } => {
                let t = target.0 as usize;
                if !human || t >= self.agents.len() {
                    return;
                }
                let tk = self.agents[t].kind;
                let Some(item) = tk.upkeep_item() else { return };
                if dist2(ax, ay, self.agents[t].x, self.agents[t].y) > USE_RADIUS_MM * USE_RADIUS_MM {
                    return self.reject(id, Reject::TooFar);
                }
                let have = &mut self.agents[idx].inventory.counts[item as usize];
                if *have == 0 {
                    return self.reject(id, Reject::NoMaterials);
                }
                *have -= 1;
                self.agents[t].hunger = (self.agents[t].hunger + tk.upkeep_gain()).min(MAX_STAT);
            }
            CommandKind::Hitch { worker, item } => {
                let t = worker.0 as usize;
                if !human || t >= self.agents.len() || !matches!(item, ItemKind::Cart | ItemKind::Plow) {
                    return;
                }
                if !matches!(self.agents[t].kind, AgentKind::Ox | AgentKind::Horse) || self.agents[t].inventory.get(*item) > 0 {
                    return self.reject(id, Reject::CannotHitch);
                }
                if dist2(ax, ay, self.agents[t].x, self.agents[t].y) > USE_RADIUS_MM * USE_RADIUS_MM {
                    return self.reject(id, Reject::TooFar);
                }
                if self.agents[idx].inventory.counts[*item as usize] == 0 {
                    return self.reject(id, Reject::NoMaterials);
                }
                self.agents[idx].inventory.counts[*item as usize] -= 1;
                self.agents[t].inventory.counts[*item as usize] += 1;
            }
            CommandKind::Design { inputs, output } => {
                // at a running computer only
                let live = self.electric_live();
                let at = self.buildings.iter().zip(&live).any(|(b, &on)| on && b.kind == BuildingKind::Computer && dist2(b.x, b.y, ax, ay) <= USE_RADIUS_MM * USE_RADIUS_MM);
                if !human || !at {
                    return self.reject(id, Reject::NeedsStation(BuildingKind::Computer));
                }
                match self.design_recipe(inputs, *output) {
                    Ok(text) => {
                        let base = self.custom_rules.as_ref().map_or_else(|| crate::rules::Rules::base_text().to_string(), |(t, _)| t.clone());
                        if self.set_custom_rules(format!("{base}{text}")).is_err() {
                            return self.reject(id, Reject::Unbalanced);
                        }
                    }
                    Err(why) => return self.reject(id, why),
                }
            }
            CommandKind::Drop { item, n } => {
                let have = self.agents[idx].inventory.get(*item);
                let n = (*n).min(have).min(10);
                if n == 0 || !kind_of_agent.can_harvest() {
                    return;
                }
                self.agents[idx].inventory.counts[*item as usize] -= n;
                let seed = self.seed;
                let tick = self.tick;
                for k in 0..n as i64 {
                    // spread round the feet, so they do not lie all on one spot
                    let h = crate::rng::hash2(seed ^ tick ^ 0xD0, id.0 as i32, k as i32);
                    let a = (h % 6283) as i64;
                    let r = 500 + (h >> 16) as i64 % 400;
                    let (dx, dy) = (r * isin_mrad(a + 1571) / 1000, r * isin_mrad(a) / 1000);
                    let z = crate::map::elevation_at_mm(seed, ax + dx, ay + dy) as i64 + 300;
                    self.throw_piece(*item, (ax + dx, ay + dy, z), (dx / 40, dy / 40, 10));
                }
            }
            CommandKind::Prospect => {
                if !human || self.agents[idx].inventory.get(ItemKind::ProspectorPick) == 0 {
                    return self.reject(id, Reject::NoMaterials);
                }
                let (tx, ty) = ((ax / 1000) as i32, (ay / 1000) as i32);
                let found = deposits_near(self.seed, tx, ty, 50);
                let mut kinds: Vec<ItemKind> = found.iter().map(|d| d.2).collect();
                kinds.sort_by_key(|k| *k as u32);
                kinds.dedup();
                let nearest = found.iter().min_by_key(|d| (d.0 - tx).pow(2) + (d.1 - ty).pow(2)).map(|d| (d.2, d.0 - tx, d.1 - ty));
                self.events.push(Event::Prospected { agent: id, kinds, nearest });
            }
            CommandKind::Repair { item } => {
                let Some(t) = (&*rules_now).tool(*item).copied() else { return };
                let Some(mat) = t.repair else { return };
                let a = &mut self.agents[idx];
                if !human || a.inventory.get(*item) == 0 || a.wear_of(*item) == 0 {
                    return;
                }
                if a.inventory.get(mat) == 0 {
                    return self.reject(id, Reject::NoMaterials);
                }
                a.inventory.counts[mat as usize] -= 1;
                a.wear.retain(|w| w.0 != *item);
            }
            CommandKind::SetRoute { worker, route } => {
                let Some(w) = self.agents.get_mut(worker.0 as usize) else { return };
                if !w.kind.is_worker() {
                    return;
                }
                w.route = route.iter().copied().take(MAX_ROUTE_STEPS).collect();
                w.route_i = 0;
                w.route_wait = 0;
                w.route_try = 0;
                w.task = Task::Idle;
            }
        }
    }

    /// Rebuild the building obstacle set if buildings were added behind the sim's back (tests, loading).
    pub(crate) fn sync_obstacles(&mut self) {
        let rules_now = self.rules_arc();
        if self.obstacles.built_for == self.buildings.len() {
            return;
        }
        self.obstacles.clear();
        for b in &self.buildings {
            let r = (&*rules_now).building(b.kind).radius_mm;
            self.obstacles.block_footprint(&self.map, b.x, b.y, r);
        }
        self.obstacles.built_for = self.buildings.len();
    }

    fn run_tasks(&mut self) {
        let rules_now = self.rules_arc();
        self.sync_obstacles();
        let stage = self.perception_level() as u32;
        let settings = self.settings;
        let humans: Vec<(i64, i64)> = self.agents.iter().filter(|a| a.kind == AgentKind::Human).map(|a| (a.x, a.y)).collect();
        let seed = self.seed;
        let tick = self.tick;
        // what the work this tick knocks over: (tree tile, cutter), and loose rocks (where, which way)
        let mut felled: Vec<(i32, i32, AgentId, i64, i64)> = Vec::new();
        let mut loosened: Vec<(i64, i64, i64, i64, u64)> = Vec::new();
        let mut launched: Vec<(AgentId, i64, i64)> = Vec::new();
        // pieces broken off trunks this tick (what, from where, which way), and who is looking for the next piece
        let mut thrown: Vec<(ItemKind, (i64, i64, i64), (i64, i64, i64))> = Vec::new();
        let mut seek: Vec<usize> = Vec::new();
        let Self { map, nodes, node_seq, node_stamps, node_acc, agents, obstacles, events, air, bodies, .. } = self;
        let mut nav = Nav { map, obstacles, budget: PATHS_PER_TICK };
        for (ai, a) in agents.iter_mut().enumerate() {
            // off the ground, nobody walks or works: the fall has them until they land
            if air.iter().any(|f| f.agent == a.id) {
                continue;
            }
            // a dog without orders trots after the nearest person once they get a good way off
            if a.kind == AgentKind::Dog && a.task == Task::Idle && a.route.is_empty() && a.hunger > 0 {
                let near = humans.iter().map(|&(hx, hy)| (dist2(a.x, a.y, hx, hy), hx, hy)).min();
                if let Some((d2, hx, hy)) = near.filter(|t| t.0 > DOG_FOLLOW_MM * DOG_FOLLOW_MM) {
                    let _ = d2;
                    a.task = Task::Move { x: hx, y: hy };
                }
            }
            // A worker without upkeep is too weak to do anything.
            if a.kind.is_worker() && a.hunger == 0 {
                continue;
            }
            match a.task {
                Task::Idle | Task::Tame { .. } | Task::Hunt { .. } | Task::Craft { .. } | Task::Cave { .. } => {}
                Task::Move { x, y } => {
                    if !matches!(advance(a, &mut nav, x, y), Step::Moved) {
                        a.task = Task::Idle;
                    }
                }
                Task::Split { body, progress } => {
                    let cap = a.kind.capacity();
                    let Some(b) = bodies.iter_mut().find(|b| b.id == body && b.kind == crate::physics::BodyKind::Tree && b.wood > 0) else {
                        a.task = Task::Idle;
                        seek.push(ai);
                        continue;
                    };
                    // still coming down: stand clear until it lies still
                    if !b.lying() {
                        continue;
                    }
                    if a.inventory.is_full(cap) {
                        if a.kind == AgentKind::Human {
                            events.push(Event::Rejected(a.id, Reject::PackFull));
                        }
                        a.task = Task::Idle;
                        continue;
                    }
                    // a trunk that fell over the edge of the world is worked from the side still in it
                    let (cx, cy) = b.cut_point();
                    let (mut cx, mut cy) = (cx.clamp(400, map.w as i64 * TILE_MM - 400), cy.clamp(400, map.h as i64 * TILE_MM - 400));
                    // a tall trunk whose crown end lies in the water is worked from the last bit of it still on land
                    let mut back = 0;
                    while matches!(map.terrain_mm(cx, cy), None | Some(Terrain::Water)) && back < b.size as i64 {
                        cx -= b.dx as i64 * 500 / 1000;
                        cy -= b.dy as i64 * 500 / 1000;
                        back += 500;
                    }
                    let need = split_need(&a.inventory, stage as u8);
                    if dist2(a.x, a.y, cx, cy) > REACH_MM * REACH_MM {
                        if matches!(advance(a, &mut nav, cx, cy), Step::Blocked) {
                            if a.kind == AgentKind::Human {
                                events.push(Event::Rejected(a.id, Reject::Unreachable));
                            }
                            a.task = Task::Idle;
                        }
                    } else if progress + 1 >= need {
                        // a piece breaks off and flies clear, out to one side of the trunk
                        b.wood -= 1;
                        let h = crate::rng::hash2(seed ^ 0x5B11, body as i32, b.wood as i32);
                        let side = if h & 1 == 0 { 1 } else { -1 };
                        let (sx, sy) = (-(b.dy as i64) * side, b.dx as i64 * side);
                        let out = 9 + (h >> 8) as i64 % 10;
                        let back = (h >> 16) as i64 % 9 - 4;
                        let z = crate::map::elevation_at_mm(seed, cx, cy) as i64 + 250;
                        thrown.push((ItemKind::Wood, (cx, cy, z), (sx * out / 1000 + b.dx as i64 * back / 1000, sy * out / 1000 + b.dy as i64 * back / 1000, 30 + (h >> 24) as i64 % 14)));
                        if b.wood == 0 {
                            a.task = Task::Idle;
                            seek.push(ai);
                        } else {
                            a.task = Task::Split { body, progress: 0 };
                        }
                    } else {
                        a.task = Task::Split { body, progress: progress + 1 };
                    }
                }
                Task::Pick { body, progress } => {
                    let cap = a.kind.capacity();
                    let Some(bi) = bodies.iter().position(|b| b.id == body && b.kind == crate::physics::BodyKind::Piece) else {
                        a.task = Task::Idle;
                        seek.push(ai);
                        continue;
                    };
                    let (px, py, _) = bodies[bi].pos_mm();
                    if a.inventory.is_full(cap) {
                        if a.kind == AgentKind::Human {
                            events.push(Event::Rejected(a.id, Reject::PackFull));
                        }
                        a.task = Task::Idle;
                    } else if dist2(a.x, a.y, px, py) > PICK_REACH_MM * PICK_REACH_MM {
                        if matches!(advance(a, &mut nav, px, py), Step::Blocked) {
                            a.task = Task::Idle;
                        }
                    } else if progress + 1 >= PICK_TICKS {
                        let item = ItemKind::ALL.get(bodies[bi].item as usize).copied().unwrap_or(ItemKind::Wood);
                        a.inventory.counts[item as usize] += 1;
                        bodies.remove(bi);
                        a.task = Task::Idle;
                        seek.push(ai);
                    } else {
                        a.task = Task::Pick { body, progress: progress + 1 };
                    }
                }
                Task::Walk { x, y } => match walk_free(a, &nav, x, y) {
                    Free::Moved => {}
                    Free::Arrived => a.task = Task::Idle,
                    Free::Launch { vx, vy } => launched.push((a.id, vx, vy)),
                    // a wall, water or a building straight ahead: find the way round it instead
                    Free::Blocked => {
                        if !matches!(advance(a, &mut nav, x, y), Step::Moved) {
                            a.task = Task::Idle;
                        }
                    }
                },
                Task::Fish { tx, ty, progress } => {
                    let (wx, wy) = (tx as i64 * TILE_MM + TILE_MM / 2, ty as i64 * TILE_MM + TILE_MM / 2);
                    if a.inventory.is_full(a.capacity()) {
                        events.push(Event::Rejected(a.id, Reject::PackFull));
                        a.task = Task::Idle;
                    } else if dist2(a.x, a.y, wx, wy) > 3_500 * 3_500 {
                        if matches!(advance(a, &mut nav, wx, wy), Step::Blocked) {
                            a.task = Task::Idle;
                        }
                    } else if progress + 1 >= FISH_CAST_TICKS {
                        // a cast: a rod lands a fish nearly half the time, a spear a quarter
                        let chance = if a.inventory.get(ItemKind::FishingRod) > 0 { 45 } else { 25 };
                        let roll = crate::rng::hash2(seed ^ 0xF15C, a.id.0 as i32, (tick / FISH_CAST_TICKS as u64) as i32) % 100;
                        if roll < chance {
                            a.inventory.counts[ItemKind::RawFish as usize] += 1;
                            events.push(Event::Caught { agent: a.id });
                        }
                        a.task = Task::Fish { tx, ty, progress: 0 };
                    } else {
                        a.task = Task::Fish { tx, ty, progress: progress + 1 };
                    }
                }
                Task::Drink { tx, ty } => {
                    let (wx, wy) = (tx as i64 * TILE_MM + TILE_MM / 2, ty as i64 * TILE_MM + TILE_MM / 2);
                    let empty_pots = a.inventory.counts[ItemKind::Pot as usize];
                    if a.thirst >= MAX_STAT && empty_pots == 0 {
                        a.task = Task::Idle;
                    } else if dist2(a.x, a.y, wx, wy) > REACH_MM * REACH_MM {
                        if matches!(advance(a, &mut nav, wx, wy), Step::Blocked) {
                            a.task = Task::Idle;
                        }
                    } else if a.thirst < MAX_STAT {
                        a.thirst = (a.thirst + DRINK_PER_TICK).min(MAX_STAT);
                    } else {
                        // drunk your fill: the empty pots go under the surface too
                        a.inventory.counts[ItemKind::Pot as usize] = 0;
                        a.inventory.counts[ItemKind::WaterPot as usize] += empty_pots;
                        a.task = Task::Idle;
                    }
                }
                Task::Gather { node, progress } => {
                    let cap = a.capacity();
                    let n = &mut nodes[node.0 as usize];
                    if n.amount == 0 || a.inventory.is_full(cap) {
                        if n.amount > 0 && a.kind == AgentKind::Human {
                            events.push(Event::Rejected(a.id, Reject::PackFull));
                        }
                        a.task = Task::Idle;
                        continue;
                    }
                    if let Err(why) = gate_check_in(&*rules_now, n, a.kind, &a.inventory) {
                        if a.kind == AgentKind::Human {
                            events.push(Event::Rejected(a.id, why));
                        }
                        a.task = Task::Idle;
                        continue;
                    }
                    let (nx, ny) = n.center_mm();
                    // the tool doing the work (not for the loose stones on top) wears; a blunt one is three times slower
                    let tool = (&*rules_now).gate(n.kind).filter(|g| !(g.loose > 0 && n.amount > n.kind.initial_amount().saturating_sub(g.loose))).and_then(|g| (&*rules_now).best_tool(g.tool, |k| a.inventory.get(k) > 0)).copied();
                    let life = tool.map_or(0, |t| tool_life(&t, &settings));
                    let blunt = a.kind == AgentKind::Human && tool.is_some_and(|t| life > 0 && a.wear_of(t.item) >= life);
                    // handier with each evolutionary step: 10% slower at stage 0, 10% faster at stage 5
                    let need = (gather_ticks(n.kind, &a.inventory) * (110 - 4 * stage) / 100).max(1) * if blunt { 3 } else { 1 };
                    if dist2(a.x, a.y, nx, ny) > REACH_MM * REACH_MM {
                        if matches!(advance(a, &mut nav, nx, ny), Step::Blocked) {
                            if a.kind == AgentKind::Human {
                                events.push(Event::Rejected(a.id, Reject::Unreachable));
                            }
                            a.task = Task::Idle;
                        }
                    } else if progress + 1 >= need {
                        // deep in a cave the seam is mixed: every few swings turn up ore instead of plain stone
                        if n.kind == NodeKind::Tree {
                            // chopping a standing tree only brings it down; the wood is got from the trunk on the ground
                            *node_acc ^= node_contrib(node.0, n);
                            n.amount = n.amount.saturating_sub(NodeKind::Tree.initial_amount().div_ceil(FELL_CUTS));
                            *node_acc ^= node_contrib(node.0, n);
                            if n.amount == 0 {
                                *node_seq += 1;
                                node_stamps.insert(node_chunk(n.tx, n.ty), *node_seq);
                                felled.push((n.tx, n.ty, a.id, a.x, a.y));
                                a.task = Task::Idle;
                            } else {
                                a.task = Task::Gather { node, progress: 0 };
                            }
                            continue;
                        }
                        let item = match (n.kind, n.amount % 6) {
                            (NodeKind::CaveMouth, 0) => ItemKind::IronOre,
                            (NodeKind::CaveMouth, 3) => ItemKind::CopperOre,
                            (NodeKind::CaveMouth, 5) => ItemKind::TinOre,
                            // now and then a stone seam gives up a nodule of flint
                            (NodeKind::StoneOutcrop, 2) => ItemKind::Flint,
                            // deeper in some seams lies limestone, the flux of the blast furnace
                            (NodeKind::StoneOutcrop, 3) if crate::rng::hash2(seed ^ 0x11E5, node.0 as i32, 3) % 4 == 0 => ItemKind::Limestone,
                            // and at the bottom of some clay beds a pale clay that stands great heat
                            (NodeKind::ClayDeposit, 1) if crate::rng::hash2(seed ^ 0xF1C1, node.0 as i32, 1) % 3 == 0 => ItemKind::FireClay,
                            (NodeKind::ClayDeposit, 2) if crate::rng::hash2(seed ^ 0x5A4D, node.0 as i32, 2) % 2 == 0 => ItemKind::Sand,
                            // and once in a while one of the loose stones on top is a flint nodule
                            (NodeKind::StoneOutcrop, a) if a > 3 && crate::rng::hash2(seed ^ 0xF117, node.0 as i32, a as i32) % 8 == 0 => ItemKind::Flint,
                            // the tall grass is wild grain: the last armful of a tuft shakes out seed instead of fibre
                            (NodeKind::FiberPlant, 1) => {
                                // the wild crop of the place: lentils on dry warm land, flax where it is wet, else grain
                                let (warm, wet) = crate::map::climate(seed, n.tx, n.ty);
                                if wet > 550 {
                                    ItemKind::Flax
                                } else if warm > 580 && wet < 480 {
                                    ItemKind::Lentil
                                } else {
                                    ItemKind::Seed
                                }
                            }
                            _ => n.kind.item(),
                        };
                        a.inventory.counts[item as usize] += 1;
                        if let Some(t) = tool.filter(|_| a.kind == AgentKind::Human && life > 0) {
                            if a.add_wear(t.item) == life {
                                events.push(Event::ToolBlunt { agent: a.id, item: t.item });
                            }
                        }
                        *node_acc ^= node_contrib(node.0, n);
                        n.amount -= 1;
                        *node_acc ^= node_contrib(node.0, n);
                        if n.amount == 0 {
                            *node_seq += 1;
                            node_stamps.insert(node_chunk(n.tx, n.ty), *node_seq);
                        }
                        // breaking rock on a steep face sends a stone off down it now and then
                        if matches!(n.kind, NodeKind::StoneOutcrop | NodeKind::IronOutcrop | NodeKind::CopperOutcrop | NodeKind::TinOutcrop) {
                            let h = crate::rng::hash2(seed ^ 0x1005E, node.0 as i32, tick as i32);
                            let (dx, dy, drop) = steepest_drop(seed, n.tx, n.ty);
                            if drop >= LOOSE_ROCK_DROP_MM && h % 100 < 45 {
                                let (cx, cy) = n.center_mm();
                                loosened.push((cx + dx as i64 * 300, cy + dy as i64 * 300, dx as i64 * 6, dy as i64 * 6, h));
                            }
                        }
                        a.task = Task::Gather { node, progress: 0 };
                    } else {
                        a.task = Task::Gather { node, progress: progress + 1 };
                    }
                }
            }
        }
        for (tx, ty, by, cx, cy) in felled {
            let log = self.fell_tree(tx, ty, by, cx, cy);
            // whoever brought it down goes on to break it up
            if let Some(a) = self.agents.iter_mut().find(|a| a.id == by) {
                a.task = Task::Split { body: log, progress: 0 };
            }
        }
        for (item, at, v) in thrown {
            self.throw_piece(item, at, v);
        }
        for ai in seek {
            self.seek_piece(ai);
        }
        for (x, y, vx, vy, salt) in loosened {
            self.loose_rock(x, y, vx, vy, salt);
        }
        for (id, vx, vy) in launched {
            self.launch(id, vx, vy);
        }
    }

    /// Go on to the nearest piece lying about that nobody else is already after, if any is near.
    fn seek_piece(&mut self, ai: usize) {
        let a = &self.agents[ai];
        if !matches!(a.task, Task::Idle) || a.inventory.is_full(a.kind.capacity()) {
            return;
        }
        let taken: Vec<u32> = self.agents.iter().filter_map(|o| if let Task::Pick { body, .. } = o.task { Some(body) } else { None }).collect();
        let near = self
            .bodies
            .iter()
            .filter(|b| b.kind == crate::physics::BodyKind::Piece && !taken.contains(&b.id))
            .map(|b| (dist2(a.x, a.y, b.x / crate::physics::Q, b.y / crate::physics::Q), b.id))
            .filter(|(d, _)| *d <= PICK_AROUND_MM * PICK_AROUND_MM)
            .min();
        if let Some((_, body)) = near {
            self.agents[ai].task = Task::Pick { body, progress: 0 };
        }
    }

    fn run_taming(&mut self) {
        for i in 0..self.agents.len() {
            let Task::Tame { wild, progress } = self.agents[i].task else { continue };
            let Some(wi) = self.wild.iter().position(|w| w.id == wild) else {
                self.agents[i].task = Task::Idle;
                continue;
            };
            let (wx, wy) = (self.wild[wi].x, self.wild[wi].y);
            let a = &mut self.agents[i];
            if dist2(a.x, a.y, wx, wy) > TAME_RANGE_MM * TAME_RANGE_MM {
                if matches!(advance(a, &mut Nav { map: &self.map, obstacles: &self.obstacles, budget: PATHS_PER_TICK }, wx, wy), Step::Blocked) {
                    a.task = Task::Idle;
                }
                continue;
            }
            // The animal stands still while someone is feeding it.
            if self.wild[wi].trust == 0 {
                self.wild[wi].trust = 1;
            }
            if progress + 1 < TAME_TICKS {
                a.task = Task::Tame { wild, progress: progress + 1 };
                continue;
            }
            let berries = &mut a.inventory.counts[ItemKind::Berries as usize];
            if *berries == 0 {
                a.task = Task::Idle;
                let aid = a.id;
                self.events.push(Event::Rejected(aid, Reject::NoMaterials));
                continue;
            }
            *berries -= 1;
            self.wild[wi].trust = (self.wild[wi].trust + TAME_GAIN).min(100);
            if self.wild[wi].trust >= 100 {
                a.task = Task::Idle;
                let w = self.wild.remove(wi);
                if let Some(kind) = w.species.tamed() {
                    self.spawn_worker(kind, w.x, w.y);
                    self.events.push(Event::Tamed(kind));
                }
            } else {
                a.task = Task::Tame { wild, progress: 0 };
            }
        }
    }

    fn run_hunting(&mut self) {
        for i in 0..self.agents.len() {
            let Task::Hunt { wild, progress, chase } = self.agents[i].task else { continue };
            let Some(wi) = self.wild.iter().position(|w| w.id == wild) else {
                self.agents[i].task = Task::Idle;
                continue;
            };
            let (wx, wy) = (self.wild[wi].x, self.wild[wi].y);
            let a = &mut self.agents[i];
            if a.inventory.get(ItemKind::Spear) == 0 {
                a.task = Task::Idle;
                continue;
            }
            if chase >= HUNT_GIVE_UP_TICKS {
                a.task = Task::Idle;
                let aid = a.id;
                self.events.push(Event::Rejected(aid, Reject::Escaped));
                continue;
            }
            if dist2(a.x, a.y, wx, wy) > SPEAR_RANGE_MM * SPEAR_RANGE_MM {
                if matches!(advance(a, &mut Nav { map: &self.map, obstacles: &self.obstacles, budget: PATHS_PER_TICK }, wx, wy), Step::Blocked) {
                    a.task = Task::Idle;
                } else {
                    a.task = Task::Hunt { wild, progress: 0, chase: chase + 1 };
                }
                continue;
            }
            if progress + 1 < HUNT_TICKS {
                a.task = Task::Hunt { wild, progress: progress + 1, chase: chase + 1 };
                continue;
            }
            // the throw lands: 2 to 4 pieces of meat, as many as the hunter can carry
            let roll = crate::rng::hash2(self.seed ^ 0x5555, wild as i32, (self.tick % 1_000_000) as i32) % 3;
            let room = a.capacity().saturating_sub(a.inventory.load());
            // a knife skins the kill: the full share of meat and the hide; bare hands tear off a little meat
            let knife = a.inventory.get(ItemKind::Knife) > 0 || a.inventory.get(ItemKind::CopperKnife) > 0;
            let meat = if knife { 2 + roll as u16 } else { 1 }.min(room);
            a.inventory.counts[ItemKind::RawMeat as usize] += meat;
            if knife && meat > 0 && room > meat {
                a.inventory.counts[ItemKind::Hide as usize] += 1;
            }
            a.task = Task::Idle;
            self.wild.remove(wi);
            let aid = a.id;
            self.events.push(Event::Hunted { agent: aid, meat });
        }
    }

    /// Time in the cave: the torch burns, a swing of the pick takes a while, a deeper step longer.
    fn run_caves(&mut self) {
        for ai in 0..self.agents.len() {
            let a = &self.agents[ai];
            let Task::Cave { node, depth, light, act, work } = a.task else { continue };
            let id = a.id;
            if light == 0 {
                // the dark: groping back to the mouth costs blood
                let a = &mut self.agents[ai];
                a.task = Task::Idle;
                a.health -= 600;
                a.injury += 300;
                self.events.push(Event::CaveLeft { agent: id, dark: true });
                continue;
            }
            let light = light - 1;
            let mut next = Task::Cave { node, depth, light, act, work: work + 1 };
            match act {
                1 if work + 1 >= CAVE_MINE_TICKS => {
                    let roll = crate::rng::hash2(self.seed ^ 0xCA7E, node.0 as i32 * 8 + depth as i32, (self.tick % 1_000_000) as i32) % 100;
                    let fall = 5 + 7 * depth as u64;
                    if roll < fall {
                        let a = &mut self.agents[ai];
                        a.health -= 1_500;
                        a.injury += 600;
                        self.events.push(Event::CaveRockfall(id));
                        if depth >= 2 && crate::rng::hash2(self.seed ^ 0xB0E5, id.0 as i32, (self.tick % 1_000_000) as i32) % 100 < 50 && a.broken == 0 {
                            a.broken = BREAK_TICKS;
                            self.events.push(Event::Fractured(id));
                        }
                    } else {
                        // the deeper, the more of the seam is metal
                        let pick = crate::rng::hash2(self.seed ^ 0xCA7F, node.0 as i32 * 8 + depth as i32, (self.tick % 1_000_000) as i32) % 100;
                        let ore = match depth {
                            0 => (pick < 35).then_some(ItemKind::IronOre),
                            1 => if pick < 30 { Some(ItemKind::IronOre) } else if pick < 60 { Some(ItemKind::CopperOre) } else { None },
                            2 => if pick < 25 { Some(ItemKind::CopperOre) } else if pick < 50 { Some(ItemKind::TinOre) } else if pick < 65 { Some(ItemKind::IronOre) } else if pick < 85 { Some(ItemKind::Coal) } else { None },
                            _ => if pick < 30 { Some(ItemKind::TinOre) } else if pick < 50 { Some(ItemKind::CopperOre) } else if pick < 65 { Some(ItemKind::IronOre) } else { Some(ItemKind::Coal) },
                        };
                        let a = &mut self.agents[ai];
                        a.inventory.counts[ItemKind::Stone as usize] += 2;
                        if let Some(o) = ore {
                            a.inventory.counts[o as usize] += 1 + (depth >= 3) as u16;
                        }
                        // the seam shrinks with every swing, and the cave goes quiet when it is spent
                        if let Some(n) = self.nodes.get_mut(node.0 as usize) {
                            self.node_acc ^= node_contrib(node.0, n);
                            n.amount = n.amount.saturating_sub(1);
                            self.node_acc ^= node_contrib(node.0, n);
                            if n.amount == 0 {
                                let (tx, ty) = (n.tx, n.ty);
                                self.touch_nodes(tx, ty);
                                let a = &mut self.agents[ai];
                                a.task = Task::Idle;
                                self.events.push(Event::CaveLeft { agent: id, dark: false });
                                continue;
                            }
                        }
                    }
                    next = Task::Cave { node, depth, light, act: 0, work: 0 };
                }
                2 if work + 1 >= CAVE_DEEPER_TICKS => {
                    next = Task::Cave { node, depth: (depth + 1).min(CAVE_MAX_DEPTH), light, act: 0, work: 0 };
                }
                0 => next = Task::Cave { node, depth, light, act, work: 0 },
                _ => {}
            }
            self.agents[ai].task = next;
        }
    }

    fn run_crafting(&mut self) {
        for a in &mut self.agents {
            if let Task::Craft { recipe, step, done, hits, idle, salt, score, stack, n } = a.task {
                a.task = if idle >= CRAFT_IDLE_LIMIT {
                    self.events.push(Event::CraftAbandoned(a.id));
                    Task::Idle
                } else {
                    Task::Craft { recipe, step, done, hits, idle: idle + 1, salt, score, stack, n }
                };
            }
        }
    }

    fn run_routes(&mut self) {
        let rules_now = self.rules_arc();
        let Self { nodes, node_rows, buildings, agents, map, .. } = self;
        for a in agents.iter_mut() {
            if !a.kind.is_worker() || a.route.is_empty() || a.task != Task::Idle || a.hunger == 0 {
                continue;
            }
            if a.route_i as usize >= a.route.len() {
                a.route_i = 0;
            }
            let mut done = false;
            match a.route[a.route_i as usize] {
                RouteStep::Wait { ticks } => {
                    if a.route_wait >= ticks {
                        done = true;
                    } else {
                        a.route_wait += 1;
                    }
                }
                RouteStep::Harvest { kind, x, y, radius_mm } => {
                    if !a.kind.can_harvest() || a.inventory.is_full(a.capacity()) || a.route_try >= ROUTE_HARVEST_TRIES {
                        done = true;
                    } else if let Some(node) = nearest_node(nodes, node_rows, (map.w, map.h), kind, (x, y, radius_mm), (a.x, a.y)) {
                        a.route_try += 1;
                        a.task = Task::Gather { node, progress: 0 };
                    } else {
                        done = true;
                    }
                }
                RouteStep::Deposit { building } | RouteStep::Collect { building, .. } => {
                    let step = a.route[a.route_i as usize];
                    let Some(b) = buildings.get_mut(building as usize) else {
                        a.route_i = (a.route_i + 1) % a.route.len() as u32;
                        continue;
                    };
                    let reach = USE_RADIUS_MM - 500;
                    if dist2(a.x, a.y, b.x, b.y) <= reach * reach {
                        match step {
                            RouteStep::Deposit { .. } => deposit_into(&*rules_now, &mut a.inventory, b),
                            RouteStep::Collect { item, .. } => {
                                let cap = a.capacity();
                                collect_from(&mut a.inventory, cap, b, item)
                            }
                            _ => {}
                        }
                        done = true;
                    } else if a.route_try >= ROUTE_WALK_TRIES {
                        done = true;
                    } else {
                        a.route_try += 1;
                        a.task = Task::Move { x: b.x, y: b.y };
                    }
                }
            }
            if done {
                a.route_i = (a.route_i + 1) % a.route.len() as u32;
                a.route_wait = 0;
                a.route_try = 0;
            }
        }
    }

    fn run_buildings(&mut self) {
        let rules_now = self.rules_arc();
        let recipes = &(&*rules_now).recipes;
        let mut spawns: Vec<(AgentKind, i64, i64, u8)> = Vec::new();
        let gins: Vec<(i64, i64)> = self.buildings.iter().filter(|b| b.kind == BuildingKind::AnimalGin).map(|b| (b.x, b.y)).collect();
        let mut mills: Vec<(i64, i64)> = self.buildings.iter().filter(|b| b.kind == BuildingKind::Windmill).map(|b| (b.x, b.y)).collect();
        let steam: Vec<(i64, i64)> = self.buildings.iter().filter(|b| b.steaming()).map(|b| (b.x, b.y)).collect();
        // electric motors on a live grid drive machines like a wheel does
        let live = self.electric_live();
        let motors: Vec<(i64, i64)> = self.buildings.iter().zip(&live).filter(|(b, on)| **on && b.kind == BuildingKind::ElectricMotor).map(|(b, _)| (b.x, b.y)).collect();
        let mut motor_worked = false;
        if live.iter().zip(&self.buildings).any(|(on, b)| *on && b.kind == BuildingKind::Dynamo) && self.milestones & Milestone::Electricity.bit() == 0 {
            self.milestones |= Milestone::Electricity.bit();
            self.events.push(Event::Milestone(Milestone::Electricity));
        }
        if live.iter().zip(&self.buildings).any(|(on, b)| *on && b.kind == BuildingKind::Computer) && self.milestones & Milestone::ComputerOn.bit() == 0 {
            self.milestones |= Milestone::ComputerOn.bit();
            self.events.push(Event::Milestone(Milestone::ComputerOn));
        }
        if live.iter().zip(&self.buildings).any(|(on, b)| *on && b.kind == BuildingKind::ElectricLamp) && self.milestones & Milestone::ElectricLight.bit() == 0 {
            self.milestones |= Milestone::ElectricLight.bit();
            self.events.push(Event::Milestone(Milestone::ElectricLight));
        }
        let mut steam_worked = false;
        let wind_only = mills.clone();
        mills.extend(steam.iter().copied());
        mills.extend(motors.iter().copied());
        let mut wind_worked = false;
        let mut animal_milled = false;
        let mut water_forged = false;
        // bloomeries with a working bellows beside them (worked by hand, by an animal at a gin or by water)
        let blown: Vec<(i64, i64)> = self
            .buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::Bellows)
            .filter(|bl| {
                let near = |px: i64, py: i64, r: i64| dist2(px, py, bl.x, bl.y) <= r * r;
                self.agents.iter().any(|a| a.kind == AgentKind::Human && near(a.x, a.y, 2_500))
                    || self.buildings.iter().any(|w| (matches!(w.kind, BuildingKind::WaterWheel | BuildingKind::Windmill) || w.steaming()) && near(w.x, w.y, 8_000))
                    || self.buildings.iter().filter(|g| g.kind == BuildingKind::AnimalGin && near(g.x, g.y, 6_000)).any(|g| self.agents.iter().any(|a| matches!(a.kind, AgentKind::Ox | AgentKind::Horse) && dist2(a.x, a.y, g.x, g.y) <= PEN_RADIUS_MM * PEN_RADIUS_MM))
            })
            .map(|b| (b.x, b.y))
            .collect();
        let wheels: Vec<(i64, i64)> =
            self.buildings.iter().filter(|b| b.kind == BuildingKind::WaterWheel).map(|b| (b.x, b.y)).collect();
        let air_c = (crate::time::ambient_x10(self.tick, self.settings.season_days as u64) / 10) as i16;
        let winter = self.season() == 3;
        let realistic = self.settings.difficulty == Difficulty::Realistic;
        let tick = self.tick;
        let second = self.tick % crate::time::TICKS_PER_SECOND == 0;
        for (i, b) in self.buildings.iter_mut().enumerate() {
            let powered = false; // water wheels drive machines through the power checks below, not a speed-up aura
            let ploughed = b.kind == BuildingKind::Field && b.job.is_some() && plow_team_at(&self.agents, b.x, b.y);
            if b.kind.burns() {
                b.fuel = b.fuel.saturating_sub(1);
            }
            // a fire-based station only works while it burns
            let lit = !b.kind.burns() || b.fuel > 0;
            // heat: a source climbs toward its ceiling while it burns or works, and cools toward the air otherwise
            let bdef = (&*rules_now).building(b.kind);
            // a bloomery without air from a working bellows within 3 m never gets past 1000 °C
            let ceiling = if matches!(b.kind, BuildingKind::Bloomery | BuildingKind::BlastFurnace) && !blown.iter().any(|&(x, y)| dist2(x, y, b.x, b.y) <= 3_000 * 3_000) { bdef.heat_max.min(1000) } else { bdef.heat_max };
            if second && bdef.heat_max > 0 {
                let working = if b.kind.burns() { b.fuel > 0 } else { b.job.is_some() };
                if working && b.temp < ceiling {
                    b.temp = (b.temp + bdef.heat_rate.max(1)).min(ceiling);
                } else if working && b.temp > ceiling {
                    b.temp = (b.temp - 10).max(ceiling);
                } else if !working && b.temp > air_c {
                    b.temp = (b.temp - ((b.temp - air_c) / 40).max(1)).max(air_c);
                } else if !working {
                    b.temp = air_c;
                }
            }
            let hot_enough = b.job.as_ref().is_none_or(|j| b.temp >= recipes[j.recipe as usize].heat);
            // fields: nothing grows in winter (a hard winter kills the crop); the soil rests while the field lies
            // empty, and manure worked in restores it
            let mut growing = true;
            if b.kind == BuildingKind::Field {
                if winter && b.job.is_some() {
                    growing = false;
                    if realistic {
                        b.job = None;
                        self.events.push(Event::CropFrozen { building: i as u32 });
                    }
                }
                if b.job.is_none() && tick % 600 == 0 && b.fertility < 1000 {
                    b.fertility += 1;
                }
                if b.input[ItemKind::Manure as usize] > 0 && b.fertility <= 750 {
                    b.input[ItemKind::Manure as usize] -= 1;
                    b.fertility += 250;
                }
            }
            // penned animals leave dung behind
            if b.kind == BuildingKind::Pen && tick % 6000 == 0 && tick > 0 {
                let r2 = PEN_RADIUS_MM * PEN_RADIUS_MM;
                if self.agents.iter().any(|a| matches!(a.kind, AgentKind::Ox | AgentKind::Horse | AgentKind::Dog) && dist2(a.x, a.y, b.x, b.y) <= r2) {
                    b.output[ItemKind::Manure as usize] += 1;
                }
            }
            // a mill turns only with a hand at it or an animal walking a gin beside it: power against its resistance
            // power at a spot: a person working it (1), an animal walking a gin within 6 m (3 each), a water wheel
            // within 8 m (3 each)
            let power_at = |x: i64, y: i64, hands_ok: bool| -> (u32, u32, u32) {
                let near = |px: i64, py: i64, r: i64| dist2(px, py, x, y) <= r * r;
                let hands = if hands_ok { self.agents.iter().filter(|a| a.kind == AgentKind::Human && near(a.x, a.y, 2_500)).count() as u32 } else { 0 };
                let animals = gins
                    .iter()
                    .filter(|&&(gx, gy)| near(gx, gy, 6_000))
                    .map(|&(gx, gy)| self.agents.iter().filter(|a| matches!(a.kind, AgentKind::Ox | AgentKind::Horse) && dist2(a.x, a.y, gx, gy) <= PEN_RADIUS_MM * PEN_RADIUS_MM).count() as u32)
                    .sum::<u32>();
                let water = wheels.iter().chain(mills.iter()).filter(|&&(wx, wy)| near(wx, wy, 8_000)).count() as u32;
                (hands, animals, water)
            };
            let mut mill_power = 1u32;
            let mut by_animal = false;
            let mut by_water = false;
            if b.kind == BuildingKind::HandMill {
                let (hands, animals, water) = power_at(b.x, b.y, true);
                by_animal = animals > 0;
                by_water = water > 0;
                mill_power = (hands + 3 * animals + 3 * water).min(3);
            }
            if matches!(b.kind, BuildingKind::TripHammer | BuildingKind::RollingMill) {
                let (_, animals, water) = power_at(b.x, b.y, false);
                by_water = water > 0;
                mill_power = (3 * animals + 3 * water).min(3);
            }
            if let Some(job) = b.job.as_mut().filter(|_| lit && hot_enough && growing && mill_power > 0) {
                job.remaining = job.remaining.saturating_sub(if powered { 2 } else { 1 } * if ploughed { 2 } else { 1 } * mill_power);
                if job.remaining == 0 && by_animal && self.milestones & Milestone::AnimalPower.bit() == 0 {
                    animal_milled = true;
                }
                if job.remaining == 0 && by_water && b.kind == BuildingKind::TripHammer {
                    water_forged = true;
                }
                // a machine finished its work with a windmill driving it (directly, or through its bellows)
                let machine = matches!(b.kind, BuildingKind::HandMill | BuildingKind::TripHammer | BuildingKind::RollingMill | BuildingKind::Bloomery | BuildingKind::BlastFurnace);
                if job.remaining == 0 && machine && wind_only.iter().any(|&(mx, my)| dist2(mx, my, b.x, b.y) <= 12_000 * 12_000) {
                    wind_worked = true;
                }
                if job.remaining == 0 && machine && steam.iter().any(|&(mx, my)| dist2(mx, my, b.x, b.y) <= 12_000 * 12_000) {
                    steam_worked = true;
                }
                if job.remaining == 0 && machine && motors.iter().any(|&(mx, my)| dist2(mx, my, b.x, b.y) <= 8_000 * 8_000) {
                    motor_worked = true;
                }
                if job.remaining == 0 {
                    let def = &recipes[job.recipe as usize];
                    if def.mine {
                        let seed = self.seed;
                        if let Some(d) = deposits_near(seed, (b.x / 1000) as i32, (b.y / 1000) as i32, 4).first() {
                            let n = 1 + d.3 as u16;
                            b.output[d.2 as usize] += n;
                        }
                    }
                    if let Some(al) = &def.alloy {
                        // whatever ore was loaded melts together; the share of the base metal decides what comes out
                        let (base, add) = (b.input[al.base as usize] as u32, b.input[al.add as usize] as u32);
                        b.input[al.base as usize] = 0;
                        b.input[al.add as usize] = 0;
                        let ingots = ((base + add) / al.ores_per_ingot.max(1)) as u16;
                        if ingots > 0 {
                            let share = base * 100 / (base + add);
                            let (lo, hi) = if realistic { (al.lo_realistic, al.hi_realistic) } else { (al.lo, al.hi) };
                            let out = if (lo as u32..=hi as u32).contains(&share) { al.output } else { al.bad };
                            b.output[out as usize] += ingots;
                            self.produced[out as usize] += ingots as u32;
                        }
                    }
                    for (k, o) in def.outputs.iter().enumerate() {
                        // a field's crop follows its soil (the seed kept back does not); each harvest tires it
                        let n = if b.kind == BuildingKind::Field && k == 0 { (o.n as u32 * b.fertility as u32 / 1000).max(1) as u16 } else { o.n };
                        b.output[o.item as usize] += n;
                        self.produced[o.item as usize] += n as u32;
                    }
                    if b.kind == BuildingKind::Field {
                        b.fertility = b.fertility.saturating_sub(150).max(300);
                    }
                    if let Some(kind) = def.spawns {
                        // the young is a generation further from the wild than the best of its parents
                        let r2 = PEN_RADIUS_MM * PEN_RADIUS_MM;
                        let parents = self.agents.iter().filter(|a| a.kind == kind && dist2(a.x, a.y, b.x, b.y) <= r2).map(|a| a.generation).max().unwrap_or(0);
                        spawns.push((kind, b.x, b.y, parents.saturating_add(1).min(10)));
                    }
                    b.job = None;
                    self.events.push(Event::JobDone { building: i as u32 });
                }
            }
            if b.job.is_none() && lit {
                if let Some(ri) = b.auto {
                    let def = &recipes[ri as usize];
                    // a pen breeds only while a pair of that kind is in it
                    let pair = b.kind != BuildingKind::Pen || def.spawns.is_none_or(|k| pair_in_pen(&self.agents, k, b.x, b.y));
                    if pair && def.inputs.iter().all(|c| b.input[c.item as usize] >= c.n) {
                        for c in &def.inputs {
                            b.input[c.item as usize] -= c.n;
                        }
                        b.job = Some(Job { recipe: ri, remaining: def.ticks });
                    }
                }
            }
        }
        if motor_worked && self.milestones & Milestone::ElectricMotor.bit() == 0 {
            self.milestones |= Milestone::ElectricMotor.bit();
            self.events.push(Event::Milestone(Milestone::ElectricMotor));
        }
        if steam_worked && self.milestones & Milestone::SteamPower.bit() == 0 {
            self.milestones |= Milestone::SteamPower.bit();
            self.events.push(Event::Milestone(Milestone::SteamPower));
        }
        if wind_worked && self.milestones & Milestone::WindPower.bit() == 0 {
            self.milestones |= Milestone::WindPower.bit();
            self.events.push(Event::Milestone(Milestone::WindPower));
        }
        self.run_conveyors();
        if water_forged && self.milestones & Milestone::Mechanism.bit() == 0 {
            self.milestones |= Milestone::Mechanism.bit();
            self.events.push(Event::Milestone(Milestone::Mechanism));
        }
        if animal_milled {
            self.milestones |= Milestone::AnimalPower.bit();
            self.events.push(Event::Milestone(Milestone::AnimalPower));
        }
        for (kind, x, y, generation) in spawns {
            // Step out of the door if there is room, otherwise appear on the spot.
            let (sx, sy) = if self.map.terrain_mm(x + 1600, y).is_some_and(|t| t != Terrain::Water) {
                (x + 1600, y)
            } else {
                (x, y)
            };
            let id = self.spawn_worker(kind, sx, sy);
            self.agents[id.0 as usize].generation = generation;
            self.events.push(Event::WorkerReady(kind));
        }
    }

    fn run_wildlife(&mut self) {
        let (seed, tick) = (self.seed, self.tick);
        let people: Vec<(i64, i64)> =
            self.agents.iter().filter(|a| a.kind == AgentKind::Human).map(|a| (a.x, a.y)).collect();
        let active = self.active_chunks();
        if self.wild_index.len != self.wild.len() {
            self.wild_index.rebuild(&self.wild);
        }
        // only animals in active chunks are touched, in ascending index order
        let mut idx: Vec<u32> = active.iter().filter_map(|c| self.wild_index.buckets.get(c)).flatten().copied().collect();
        idx.sort_unstable();
        let busy = self.run_wolves(&idx);
        for (k, &wi) in idx.iter().enumerate() {
            if busy[k] {
                continue;
            }
            let w = &mut self.wild[wi as usize];
            let before = chunk_of(w.x, w.y);
            w.wander(seed, tick, &self.map, &people);
            let after = chunk_of(w.x, w.y);
            if before != after {
                self.wild_index.moved(wi, before, after);
            }
        }
    }

    /// Chunks near any person or worker, sorted. Wildlife elsewhere stays frozen, so the cost of a tick
    /// depends on how much the team is doing, not on how big the world is.
    fn active_chunks(&self) -> Vec<(i32, i32)> {
        let mut near: Vec<(i32, i32)> = self.agents.iter().map(|a| chunk_of(a.x, a.y)).collect();
        near.sort_unstable();
        near.dedup();
        let mut out = Vec::with_capacity(near.len() * 121);
        for (cx, cy) in near {
            for dy in -ACTIVE_CHUNKS..=ACTIVE_CHUNKS {
                for dx in -ACTIVE_CHUNKS..=ACTIVE_CHUNKS {
                    out.push((cx + dx, cy + dy));
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Wolves stalk the nearest person and bite, unless a lit fire is close. Returns which wolves
    /// moved on their own this tick (so they do not also wander).
    fn run_wolves(&mut self, idx: &[u32]) -> Vec<bool> {
        let tick = self.tick;
        let mut fires: Vec<(i64, i64)> =
            self.buildings.iter().filter(|b| b.kind.burns() && b.fuel > 0).map(|b| (b.x, b.y)).collect();
        // a tamed dog keeps watch: wolves keep away from it much as from a fire (it scares them where it stands)
        fires.extend(self.agents.iter().filter(|a| a.kind == AgentKind::Dog).map(|a| (a.x, a.y)));
        // by day wolves keep to themselves unless you walk right into them; dusk and night is their hunt
        // the very first day is a grace period: a newcomer learns before the pack hunts
        let aggro = if tick < crate::time::TICKS_PER_DAY || crate::time::daylight_permille(tick) >= 500 { WOLF_DAY_AGGRO_MM } else { WOLF_AGGRO_MM };
        let mut busy = vec![false; idx.len()];
        let mut bites: Vec<usize> = Vec::new();
        for (k, &wi) in idx.iter().enumerate() {
            let w = &mut self.wild[wi as usize];
            if w.species != WildSpecies::Wolf {
                continue;
            }
            let wi = k;
            let step_dir = |w: &mut Wild, map: &Map, tx: i64, ty: i64, away: bool| {
                let (dx, dy) = if away { (w.x - tx, w.y - ty) } else { (tx - w.x, ty - w.y) };
                let len = ((dx * dx + dy * dy) as u64).isqrt().max(1) as i64;
                let (nx, ny) = (w.x + dx * WOLF_STEP_MM / len, w.y + dy * WOLF_STEP_MM / len);
                if !matches!(map.terrain_mm(nx, ny), None | Some(Terrain::Water)) {
                    w.x = nx;
                    w.y = ny;
                }
            };
            // after a bite a wolf draws off for a few seconds before it comes again (a wolf's count of its own
            // backing off is kept in its trust, which a wolf never otherwise has)
            if w.trust < 0 {
                w.trust += 1;
                if let Some(&(hx, hy)) = self.agents.iter().filter(|a| a.kind == AgentKind::Human).map(|a| (a.x, a.y)).collect::<Vec<_>>().iter().min_by_key(|&&(x, y)| dist2(x, y, w.x, w.y)) {
                    step_dir(w, &self.map, hx, hy, true);
                }
                w.settle();
                busy[wi] = true;
                continue;
            }
            let scare = fires
                .iter()
                .map(|&(fx, fy)| (dist2(fx, fy, w.x, w.y), fx, fy))
                .filter(|&(d, _, _)| d < FIRE_SCARE_MM * FIRE_SCARE_MM)
                .min();
            if let Some((_, fx, fy)) = scare {
                step_dir(w, &self.map, fx, fy, true);
                w.settle();
                busy[wi] = true;
                continue;
            }
            let target = self
                .agents
                .iter()
                .enumerate()
                .filter(|(_, a)| a.kind == AgentKind::Human)
                .map(|(i, a)| (dist2(a.x, a.y, w.x, w.y), i, a.x, a.y))
                .min();
            if let Some((d2, ai, ax, ay)) = target.filter(|t| t.0 < aggro * aggro) {
                busy[wi] = true;
                if d2 > WOLF_BITE_RANGE_MM * WOLF_BITE_RANGE_MM {
                    step_dir(w, &self.map, ax, ay, false);
                    w.settle();
                } else if tick % WOLF_BITE_EVERY == w.id as u64 % WOLF_BITE_EVERY {
                    bites.push(ai);
                    w.trust = -WOLF_DRAW_OFF_TICKS;
                }
            }
        }
        // the first two days a newcomer is still learning: the bites are half as bad
        let mercy = if self.tick < 2 * crate::time::TICKS_PER_DAY { 2 } else { 1 };
        for ai in bites {
            let a = &mut self.agents[ai];
            a.health -= WOLF_BITE_DAMAGE / mercy;
            a.injury += WOLF_BITE_BLEED / mercy;
            let id = a.id;
            self.events.push(Event::Bitten(id));
        }
        busy
    }

    /// Rivers carry whoever is in them along: a swimmer fully, a wader half as much.
    fn run_currents(&mut self) {
        for a in self.agents.iter_mut().filter(|a| a.kind == AgentKind::Human && a.health > 0) {
            let (tx, ty) = (a.x.div_euclid(TILE_MM) as i32, a.y.div_euclid(TILE_MM) as i32);
            let depth = crate::map::water_depth(&self.map, tx, ty);
            if depth == 0 {
                continue;
            }
            let (cx, cy) = crate::map::current_mm(&self.map, tx, ty);
            let (cx, cy) = if depth == 2 { (cx, cy) } else { (cx / 2, cy / 2) };
            if self.map.terrain_mm(a.x + cx, a.y + cy).is_some() {
                a.x += cx;
                a.y += cy;
            }
        }
    }

    fn run_needs(&mut self) {
        let rules_now = self.rules_arc();
        let tick = self.tick;
        let settings_now = self.settings;
        let ambient = ambient_x10(tick, self.settings.season_days as u64);
        let sky = crate::time::weather(self.seed, tick, self.settings.season_days as u64);
        let spawn = self.spawn_mm;
        let defs = &*rules_now;
        let heat: Vec<(i64, i64, i64, bool)> = self
            .buildings
            .iter()
            .filter_map(|b| match b.kind {
                k if k.burns() && b.fuel > 0 => Some((b.x, b.y, FIRE_WARM_RADIUS_MM, true)),
                BuildingKind::Shelter => Some((b.x, b.y, defs.building(b.kind).radius_mm + SHELTER_EXTRA_MM, false)),
                _ => None,
            })
            .collect();
        let mut died = Vec::new();
        let mut infected = Vec::new();
        let seed = self.seed;
        {
            let Self { agents, buildings, .. } = self;
            for a in agents.iter_mut() {
                if a.kind.is_worker() {
                    if tick % a.kind.upkeep_decay_ticks() == 0 {
                        a.hunger = (a.hunger - 1).max(0);
                    }
                    // Hungry workers help themselves from a nearby depot.
                    if a.hunger < WORKER_FEED_BELOW {
                        if let Some(item) = a.kind.upkeep_item() {
                            let r2 = DEPOT_FEED_RADIUS_MM * DEPOT_FEED_RADIUS_MM;
                            if let Some(b) = buildings.iter_mut().find(|b| {
                                b.kind == BuildingKind::Depot
                                    && b.output[item as usize] > 0
                                    && dist2(b.x, b.y, a.x, a.y) <= r2
                            }) {
                                b.output[item as usize] -= 1;
                                a.hunger = (a.hunger + a.kind.upkeep_gain()).min(MAX_STAT);
                            }
                        }
                    }
                    continue;
                }
                if tick % 13 == 0 {
                    a.hunger = (a.hunger - 1).max(0);
                }
                if tick % 6 == 0 {
                    a.thirst = (a.thirst - 1).max(0);
                }
                // out in the rain or snow without cover the cold bites harder
                let covered = heat.iter().any(|&(hx, hy, r, _)| dist2(hx, hy, a.x, a.y) <= r * r);
                let wet = if covered { 0 } else { match sky { crate::time::Weather::Clear => 0, crate::time::Weather::Rain => 30, crate::time::Weather::Snow => 50 } };
                let worn: i32 = ItemKind::ALL.iter().filter(|k| a.inventory.get(**k) > 0).map(|k| k.warmth_bonus()).sum();
                let mut target = ((ambient + worn - wet) * MAX_STAT / 150).clamp(0, MAX_STAT);
                for &(hx, hy, r, fire) in &heat {
                    if dist2(hx, hy, a.x, a.y) <= r * r {
                        target = target.max(if fire { MAX_STAT } else { SHELTER_WARMTH_FLOOR });
                    }
                }
                a.warmth = if a.warmth > target { (a.warmth - 1).max(target) } else { (a.warmth + 3).min(target) };

                if tick % 10 == 0 {
                    let harm = (a.hunger == 0) as i32 + (a.thirst == 0) as i32 + (a.warmth < 500) as i32;
                    a.health -= harm;
                }
                let warm_spot = heat.iter().any(|&(hx, hy, r, _)| dist2(hx, hy, a.x, a.y) <= r * r);
                let heal = if warm_spot { 3 } else { 1 };
                // swimming: it costs breath and the water chills; out of breath, you go under
                let depth = if a.kind == AgentKind::Human { crate::map::water_depth(&self.map, a.x.div_euclid(TILE_MM) as i32, a.y.div_euclid(TILE_MM) as i32) } else { 0 };
                if depth == 2 {
                    a.stamina = (a.stamina - SPRINT_DRAIN / 2).max(0);
                    a.sprint = false;
                    a.warmth = (a.warmth - 2).max(0);
                    if a.stamina == 0 && tick % 2 == 0 {
                        a.health -= 5;
                    }
                } else if depth == 1 && tick % 3 == 0 {
                    a.warmth = (a.warmth - 1).max(0);
                }
                // breath: running burns it, standing or walking brings it back
                if depth == 2 {
                } else if a.sprint && matches!(a.task, Task::Move { .. } | Task::Gather { .. } | Task::Hunt { .. }) {
                    a.stamina = (a.stamina - SPRINT_DRAIN).max(0);
                    if tick % 3 == 0 {
                        a.thirst = (a.thirst - 1).max(0);
                    }
                    if a.stamina == 0 {
                        a.sprint = false;
                    }
                } else {
                    a.stamina = (a.stamina + SPRINT_REGEN).min(MAX_STAT);
                }
                if a.broken > 0 {
                    a.broken = (a.broken - heal).max(0);
                }
                if a.injury > 0 {
                    a.injury = (a.injury - heal).max(0);
                    if tick % 6 == 0 {
                        a.health -= 1;
                    }
                    // a wound that closes by itself, never cleaned or bound, may fester
                    if a.injury == 0 && crate::rng::hash2(seed ^ 0x1FEC, a.id.0 as i32, (tick % 1_000_000) as i32) % 100 < INFECTION_PCT {
                        a.sick += 5_000;
                        infected.push(a.id);
                    }
                }
                if a.sick > 0 {
                    a.sick = (a.sick - heal).max(0);
                    if tick % 30 == 0 {
                        a.health -= 1;
                    }
                    if tick % 12 == 0 {
                        a.hunger = (a.hunger - 1).max(0);
                    }
                }
                // what was eaten lately fades; a lopsided diet caps how healthy the body gets
                if a.kind == AgentKind::Human && tick % 120 == 0 {
                    for g in a.nutrition.iter_mut() {
                        *g = g.saturating_sub(1);
                    }
                }
                let cap = a.health_cap(&settings_now);
                if tick % 20 == 0 && a.injury == 0 && a.sick == 0 && a.hunger > 3000 && a.thirst > 3000 && a.warmth > 3000 && a.health < cap {
                    a.health = (a.health + 1).min(cap);
                }
                if a.health <= 0 {
                    died.push(a.id);
                }
            }
        }
        for id in infected {
            self.events.push(Event::WoundInfected(id));
        }
        for id in died {
            // Hardcore/save-reload is decided by the front end; the sim itself respawns with nothing.
            self.agents[id.0 as usize] = Agent::new(id, spawn.0, spawn.1);
            self.events.push(Event::Died(id));
            self.clear_wolves_around(spawn);
        }
    }

    /// A fresh start must not be a wolf's dinner: predators that were camping where someone respawns
    /// are driven out to well beyond their sight.
    fn clear_wolves_around(&mut self, (sx, sy): (i64, i64)) {
        let keep_out = WOLF_AGGRO_MM * 2;
        let dirs: [(i64, i64); 8] = [(1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -1), (1, -1)];
        let mut moved = false;
        for wi in 0..self.wild.len() {
            let w = &self.wild[wi];
            if w.species != WildSpecies::Wolf || dist2(w.x, w.y, sx, sy) >= keep_out * keep_out {
                continue;
            }
            let (dx, dy) = (w.x - sx, w.y - sy);
            let first = dirs.iter().enumerate().min_by_key(|(_, d)| (d.0 * 1000 - dx.signum() * 1000).pow(2) + (d.1 * 1000 - dy.signum() * 1000).pow(2)).map(|(i, _)| i).unwrap_or(0);
            for k in 0..8 {
                let d = dirs[(first + k) % 8];
                let (nx, ny) = (sx + d.0 * keep_out * 12 / 10, sy + d.1 * keep_out * 12 / 10);
                if !matches!(self.map.terrain_mm(nx, ny), None | Some(Terrain::Water)) {
                    self.wild[wi].x = nx;
                    self.wild[wi].y = ny;
                    moved = true;
                    break;
                }
            }
        }
        if moved {
            self.wild_index.rebuild(&self.wild);
        }
    }

    fn run_discoveries(&mut self) {
        let mut best = [0u16; ItemKind::COUNT];
        let mut person_full = false;
        for a in &self.agents {
            for (i, c) in a.inventory.counts.iter().enumerate() {
                best[i] = best[i].max(*c);
            }
            person_full |= a.kind == AgentKind::Human && a.inventory.is_full(a.capacity());
        }
        for (i, c) in best.iter().enumerate() {
            if *c > 0 {
                self.seen |= 1u128 << i;
            }
        }
        let n = |k: ItemKind| best[k as usize];
        let has_campfire = self.buildings.iter().any(|b| b.kind == BuildingKind::Campfire);
        if n(ItemKind::Stone) >= 2 {
            self.discover(Discovery::SharpStone);
        }
        if n(ItemKind::SharpStone) >= 1 && n(ItemKind::Branch) >= 1 {
            self.discover(Discovery::StoneAxe);
        }
        if n(ItemKind::Branch) >= 2 && n(ItemKind::Fiber) >= 1 {
            self.discover(Discovery::Clamp);
        }
        if n(ItemKind::Wood) >= 3 {
            self.discover(Discovery::Fire);
        }
        // known once there is wood enough to build one (it costs ten): no finding it out only to be told it cannot be
        if self.knows(Discovery::Fire) && n(ItemKind::Wood) >= 10 {
            self.discover(Discovery::Shelter);
        }
        if has_campfire && n(ItemKind::Wood) >= 5 {
            self.discover(Discovery::Kiln);
        }
        if n(ItemKind::Clay) >= 1 && n(ItemKind::Stone) >= 2 {
            self.discover(Discovery::Furnace);
        }
        // iron wants a hotter, longer fire than copper: as in history it only comes after bronze is worked
        if n(ItemKind::IronOre) >= 1 && self.knows(Discovery::Furnace) && self.knows(Discovery::Bronzework) {
            self.discover(Discovery::Smelting);
        }
        if person_full {
            self.discover(Discovery::Depot);
        }
        if self.knows(Discovery::StoneAxe) && n(ItemKind::Wood) >= 6 {
            self.discover(Discovery::Workbench);
        }
        if n(ItemKind::SharpStone) >= 1 && n(ItemKind::Branch) >= 1 && n(ItemKind::Fiber) >= 1 {
            self.discover(Discovery::Spear);
        }
        if n(ItemKind::RawMeat) >= 1 && self.knows(Discovery::Fire) {
            self.discover(Discovery::Cooking);
        }
        if n(ItemKind::Fiber) >= 4 || n(ItemKind::Hide) >= 1 {
            self.discover(Discovery::Clothing);
        }
        if n(ItemKind::Herb) >= 1 {
            self.discover(Discovery::Medicine);
        }
        if self.knows(Discovery::Ironwork) && n(ItemKind::Wood) >= 4 {
            self.discover(Discovery::Mechanics);
        }
        if n(ItemKind::IronBloom) >= 1 && self.knows(Discovery::Workbench) {
            self.discover(Discovery::Ironwork);
        }
        if n(ItemKind::IronBloom) >= 1 {
            self.discover(Discovery::Workshop);
        }
        // the first metal: copper and tin melt in the clay furnace, no iron needed
        if n(ItemKind::CopperOre) >= 1 && n(ItemKind::TinOre) >= 1 && self.knows(Discovery::Furnace) {
            self.discover(Discovery::Alloying);
        }
        if n(ItemKind::Bronze) >= 1 && self.knows(Discovery::Workbench) {
            self.discover(Discovery::Bronzework);
        }
        if n(ItemKind::Stone) >= 2 && n(ItemKind::Branch) >= 1 && self.knows(Discovery::SharpStone) {
            self.discover(Discovery::Hammer);
        }
        if self.knows(Discovery::Electricity) && n(ItemKind::Sand) >= 1 && n(ItemKind::Coke) >= 1 {
            self.discover(Discovery::Electronics);
        }
        if self.knows(Discovery::Bronzework) {
            self.discover(Discovery::Geology);
        }
        if self.knows(Discovery::Spear) || n(ItemKind::RawFish) >= 1 {
            self.discover(Discovery::Fishing);
        }
        if n(ItemKind::CircuitBoard) >= 1 {
            self.discover(Discovery::Computing);
        }
        if self.knows(Discovery::Steam) && n(ItemKind::CopperIngot) >= 1 {
            self.discover(Discovery::Electricity);
        }
        if self.knows(Discovery::Steelmaking) && (n(ItemKind::Coal) >= 1 || n(ItemKind::Sand) >= 1) {
            self.discover(Discovery::Steam);
        }
        if self.knows(Discovery::Ironwork) && (n(ItemKind::Limestone) >= 1 || n(ItemKind::FireClay) >= 1) {
            self.discover(Discovery::Steelmaking);
        }
        if n(ItemKind::CopperOre) >= 1 && self.knows(Discovery::Pottery) && self.knows(Discovery::Fire) {
            self.discover(Discovery::Casting);
        }
        if n(ItemKind::Seed) >= 1 && self.knows(Discovery::StoneAxe) {
            self.discover(Discovery::Farming);
        }
        if n(ItemKind::Grain) >= 1 && self.knows(Discovery::Fire) {
            self.discover(Discovery::Milling);
        }
        if n(ItemKind::Clay) >= 2 && self.knows(Discovery::Fire) && self.knows(Discovery::Farming) {
            self.discover(Discovery::Pottery);
        }
        if n(ItemKind::Fiber) >= 6 && self.knows(Discovery::Clothing) && self.knows(Discovery::Farming) {
            self.discover(Discovery::Weaving);
        }
        let has_animal = self.agents.iter().any(|a| matches!(a.kind, AgentKind::Dog | AgentKind::Ox | AgentKind::Horse));
        let has_draught = self.agents.iter().any(|a| matches!(a.kind, AgentKind::Ox | AgentKind::Horse));
        if has_draught && self.knows(Discovery::Bronzework) {
            self.discover(Discovery::Draught);
        }
        if has_animal && n(ItemKind::Wood) >= 4 && self.knows(Discovery::Farming) {
            self.discover(Discovery::Husbandry);
        }
    }

    /// The camp draws company: a person who lingers at a burning fire eventually draws a stray dog
    /// out of the dark, once. Chance is a pure function of seed and time, so every peer agrees.
    fn run_camp(&mut self) {
        const VISIT_RADIUS_MM: i64 = 6_000;
        if self.tick % 60 != 0 || self.milestone_done(Milestone::Visitor) || self.milestone_done(Milestone::Companion) {
            return;
        }
        let Some(fire) = self.buildings.iter().find(|b| b.kind == BuildingKind::Campfire && b.active()) else { return };
        let (fx, fy) = (fire.x, fire.y);
        let near = self.agents.iter().any(|a| a.kind == AgentKind::Human && dist2(a.x, a.y, fx, fy) <= VISIT_RADIUS_MM * VISIT_RADIUS_MM);
        if !near || crate::rng::hash2(self.seed ^ 0xD06, (self.tick / 60) as i32, 0) % 40 != 0 {
            return;
        }
        let ang = (crate::rng::hash2(self.seed ^ 0xD07, (self.tick / 60) as i32, 1) % 6283) as f32 / 1000.0;
        let (x, y) = (fx + (ang.cos() * 11_000.0) as i64, fy + (ang.sin() * 11_000.0) as i64);
        if matches!(self.map.terrain_mm(x, y), None | Some(Terrain::Water)) {
            return; // try again next second; the dark was in the water
        }
        let id = self.wild.iter().map(|w| w.id).max().map_or(0, |m| m + 1);
        self.wild.push(crate::wildlife::Wild::new(id, crate::wildlife::WildSpecies::Dog, x, y));
        self.milestones |= Milestone::Visitor.bit();
        self.events.push(Event::Milestone(Milestone::Visitor));
    }

    /// What each item is worth: 1 for what is gathered, and for what is made the least its inputs cost (plus a little
    /// for the work) shared over what comes out. Used by the design table to keep new recipes honest.
    pub fn item_values(&self) -> [u32; ItemKind::COUNT] {
        let rules = self.rules();
        let mut v = [u32::MAX; ItemKind::COUNT];
        let made: std::collections::HashSet<usize> = rules.recipes.iter().flat_map(|r| r.outputs.iter().map(|o| o.item as usize)).collect();
        for k in 0..ItemKind::COUNT {
            if !made.contains(&k) {
                v[k] = 1;
            }
        }
        for _ in 0..12 {
            let mut changed = false;
            for r in rules.recipes.iter().filter(|r| r.discovery != Discovery::Retired && r.alloy.is_none()) {
                let mut cost: u64 = (r.ticks as u64 / 600).min(50) + r.steps.len() as u64;
                if r.inputs.iter().any(|c| v[c.item as usize] == u32::MAX) {
                    continue;
                }
                cost += r.inputs.iter().map(|c| v[c.item as usize] as u64 * c.n as u64).sum::<u64>();
                let outs: u64 = r.outputs.iter().map(|o| o.n as u64).sum::<u64>().max(1);
                let each = (cost / outs).max(1) as u32;
                for o in &r.outputs {
                    if each < v[o.item as usize] {
                        v[o.item as usize] = each;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        for x in v.iter_mut() {
            if *x == u32::MAX {
                *x = 1;
            }
        }
        v
    }

    /// The design table: a new hand recipe from known things, as TOML to add to this world's rules. It may not give
    /// more than it takes by value, and every item in it must have been seen.
    pub fn design_recipe(&self, inputs: &[(ItemKind, u16)], output: (ItemKind, u16)) -> Result<String, Reject> {
        if inputs.is_empty() || inputs.len() > 4 || output.1 == 0 || inputs.iter().any(|&(_, n)| n == 0 || n > 20) || output.1 > 20 {
            return Err(Reject::Unbalanced);
        }
        if inputs.iter().any(|&(k, _)| !self.has_seen(k)) || !self.has_seen(output.0) {
            return Err(Reject::Unbalanced);
        }
        let v = self.item_values();
        let take: u64 = inputs.iter().map(|&(k, n)| v[k as usize] as u64 * n as u64).sum();
        let give = v[output.0 as usize] as u64 * output.1 as u64;
        if give > take {
            return Err(Reject::Unbalanced);
        }
        let n = self.rules().recipes.iter().filter(|r| r.id.starts_with("own_")).count() + 1;
        let snake = |k: ItemKind| serde_json_like(k);
        let ins: Vec<String> = inputs.iter().map(|&(k, c)| format!("{{ item = \"{}\", n = {c} }}", snake(k))).collect();
        Ok(format!(
            "\n[[recipe]]\nid = \"own_{n}\"\nname = \"Kendi tarifim {n}\"\ninputs = [{}]\noutputs = [{{ item = \"{}\", n = {} }}]\nticks = 0\ndiscovery = \"computing\"\n",
            ins.join(", "),
            snake(output.0),
            output.1
        ))
    }

    /// The loop: a new world grown from this one. Its seed is this world's state, it keeps the settings and the rules
    /// written here, and what was built here stands in it as ruins around the new spawn. The newcomer starts with
    /// nothing, as an ape, and the tribe has forgotten everything.
    pub fn next_cycle(&self) -> World {
        let mut w = World::with_size(if self.map.w > 4096 { crate::map::tag_seed_v3(self.hash()) } else { crate::map::tag_seed_v2(self.hash()) }, self.map.w, self.map.h);
        w.settings = self.settings;
        if let Some((text, rules)) = &self.custom_rules {
            w.custom_rules = Some((text.clone(), std::sync::Arc::clone(rules)));
        }
        let (ox, oy) = self.spawn_mm;
        let (nx, ny) = w.spawn_mm;
        for b in self.buildings.iter().filter(|b| b.kind != BuildingKind::Ruin) {
            let (x, y) = (nx + (b.x - ox), ny + (b.y - oy));
            let on_land = w.map.terrain_mm(x, y).is_some_and(|t| t != Terrain::Water);
            if on_land && dist2(x, y, nx, ny) >= 4_000 * 4_000 && w.buildings.iter().all(|r| dist2(r.x, r.y, x, y) >= 1_800 * 1_800) {
                // the old kind is kept in the fuel field of a ruin (ruins never burn)
                w.buildings.push(Building { fuel: b.kind as u32, ..Building::new(BuildingKind::Ruin, x, y) });
            }
        }
        w.spawn_agent(nx, ny);
        w
    }

    /// Is there power at (x, y): a water wheel, windmill or working steam engine within 8 m, or an animal walking a
    /// gin within 6 m?
    pub fn powered_at(&self, x: i64, y: i64) -> bool {
        if self.mech_powered_at(x, y) {
            return true;
        }
        let live = self.electric_live();
        self.buildings.iter().zip(&live).any(|(b, &on)| on && b.kind == BuildingKind::ElectricMotor && dist2(b.x, b.y, x, y) <= 8_000 * 8_000)
    }

    /// Which buildings are on a live grid: a dynamo turned by mechanical power feeds it, and grid buildings (poles,
    /// motors, lamps, computers, other dynamos) within 12 m of a live one are live too.
    pub fn electric_live(&self) -> Vec<bool> {
        let mut live = vec![false; self.buildings.len()];
        let mut todo: Vec<usize> = Vec::new();
        for (i, b) in self.buildings.iter().enumerate() {
            if b.kind == BuildingKind::Dynamo && self.mech_powered_at(b.x, b.y) {
                live[i] = true;
                todo.push(i);
            }
        }
        while let Some(i) = todo.pop() {
            let (x, y) = (self.buildings[i].x, self.buildings[i].y);
            for (j, c) in self.buildings.iter().enumerate() {
                if !live[j] && c.kind.on_grid() && dist2(c.x, c.y, x, y) <= 12_000 * 12_000 {
                    live[j] = true;
                    todo.push(j);
                }
            }
        }
        live
    }

    /// Mechanical power at (x, y): water wheel, windmill or steam within 8 m, or an animal at a gin within 6 m.
    fn mech_powered_at(&self, x: i64, y: i64) -> bool {
        let near = |px: i64, py: i64, r: i64| dist2(px, py, x, y) <= r * r;
        self.buildings.iter().any(|w| (matches!(w.kind, BuildingKind::WaterWheel | BuildingKind::Windmill) || w.steaming()) && near(w.x, w.y, 8_000))
            || self.buildings.iter().filter(|g| g.kind == BuildingKind::AnimalGin && near(g.x, g.y, 6_000)).any(|g| self.agents.iter().any(|a| matches!(a.kind, AgentKind::Ox | AgentKind::Horse) && dist2(a.x, a.y, g.x, g.y) <= PEN_RADIUS_MM * PEN_RADIUS_MM))
    }

    /// Powered belts, once a second: everything the stations within 3 m have made goes to the nearest depot within
    /// 12 m. Power is a water wheel or windmill within 8 m, or an animal at a gin within 6 m.
    fn run_conveyors(&mut self) {
        if self.tick % TICKS_PER_SECOND_U64 != 0 {
            return;
        }
        let belts: Vec<(i64, i64)> = self.buildings.iter().filter(|b| b.kind == BuildingKind::Conveyor).map(|b| (b.x, b.y)).collect();
        let mut moved_any = false;
        for (cx, cy) in belts {
            let near = |x: i64, y: i64, r: i64| dist2(x, y, cx, cy) <= r * r;
            let water = self.buildings.iter().any(|w| (matches!(w.kind, BuildingKind::WaterWheel | BuildingKind::Windmill) || w.steaming()) && near(w.x, w.y, 8_000));
            let animal = self.buildings.iter().filter(|g| g.kind == BuildingKind::AnimalGin && near(g.x, g.y, 6_000)).any(|g| self.agents.iter().any(|a| matches!(a.kind, AgentKind::Ox | AgentKind::Horse) && dist2(a.x, a.y, g.x, g.y) <= PEN_RADIUS_MM * PEN_RADIUS_MM));
            if !water && !animal {
                continue;
            }
            let Some(depot) = self.buildings.iter().enumerate().filter(|(_, b)| b.kind == BuildingKind::Depot && near(b.x, b.y, 12_000)).min_by_key(|(_, b)| dist2(b.x, b.y, cx, cy)).map(|(i, _)| i) else { continue };
            let mut load = [0u16; ItemKind::COUNT];
            for b in self.buildings.iter_mut().filter(|b| !matches!(b.kind, BuildingKind::Depot | BuildingKind::Conveyor) && dist2(b.x, b.y, cx, cy) <= 3_000 * 3_000) {
                for (i, n) in b.output.iter_mut().enumerate() {
                    load[i] += std::mem::take(n);
                }
            }
            if load.iter().any(|&n| n > 0) {
                moved_any = true;
                for (i, n) in load.iter().enumerate() {
                    self.buildings[depot].output[i] += n;
                }
            }
        }
        if moved_any && self.milestones & Milestone::BeltLine.bit() == 0 {
            self.milestones |= Milestone::BeltLine.bit();
            self.events.push(Event::Milestone(Milestone::BeltLine));
        }
    }

    /// Every in-game hour a share of each food stack goes bad and turns to manure: how much is a pure function of
    /// seed and time (the fraction left over is rounded by a hash), so peers agree and nothing new is saved. Winter
    /// halves it, each storage jar in a depot slows it there, and the realistic setting doubles it.
    fn run_spoilage(&mut self) {
        const HOUR: u64 = TICKS_PER_DAY / 24;
        if self.tick % HOUR != 0 || self.tick == 0 {
            return;
        }
        let rules_now = self.rules_arc();
        if rules_now.perish.is_empty() {
            return;
        }
        let mut scale: u32 = if self.season() == 3 { 500 } else { 1000 };
        if self.settings.difficulty == Difficulty::Realistic {
            scale *= 2;
        }
        let (seed, tick) = (self.seed, self.tick);
        let spoil = |counts: &mut [u16; ItemKind::COUNT], salt: i32, jar_slow: u32, mut gone: Option<&mut Vec<(ItemKind, u16)>>| {
            for p in &rules_now.perish {
                let c = counts[p.item as usize] as u64;
                if c == 0 {
                    continue;
                }
                // expected loss this hour, in thousandths of a unit
                let x = c * p.per_day as u64 * scale as u64 / 1000 / jar_slow as u64 / 24;
                let roll = crate::rng::hash2(seed ^ 0x5B011, salt.wrapping_mul(97).wrapping_add(p.item as i32), (tick / HOUR) as i32) % 1000;
                let lost = ((x / 1000) + (roll < x % 1000) as u64).min(c) as u16;
                if lost > 0 {
                    counts[p.item as usize] -= lost;
                    counts[ItemKind::Manure as usize] += lost;
                    if let Some(g) = gone.as_mut() {
                        g.push((p.item, lost));
                    }
                }
            }
        };
        let mut spoiled = Vec::new();
        for a in self.agents.iter_mut() {
            let mut gone = Vec::new();
            spoil(&mut a.inventory.counts, a.id.0 as i32, 1, Some(&mut gone));
            if a.kind == AgentKind::Human {
                spoiled.extend(gone.into_iter().map(|(k, n)| (a.id, k, n)));
            }
        }
        for (i, b) in self.buildings.iter_mut().enumerate() {
            // storage jars in a depot: each one slows the spoiling there, up to four
            let jars = match b.kind {
                BuildingKind::Depot => b.output[ItemKind::LargePot as usize].min(4) as u32,
                // a cellar: cool and dark, five times slower
                BuildingKind::Cellar => 4,
                _ => 0,
            };
            spoil(&mut b.input, 100_000 + i as i32 * 2, 1, None);
            spoil(&mut b.output, 100_001 + i as i32 * 2, 1 + jars, None);
        }
        for (agent, item, n) in spoiled {
            self.events.push(Event::Spoiled { agent, item, n });
        }
    }

    fn run_milestones(&mut self) {
        let has_building = |k: BuildingKind| self.buildings.iter().any(|b| b.kind == k);
        let has_agent = |f: fn(AgentKind) -> bool| self.agents.iter().any(|a| f(a.kind));
        let made = |k: ItemKind| self.produced[k as usize] > 0;
        let conditions = [
            (Milestone::FirstFire, has_building(BuildingKind::Campfire)),
            (Milestone::Shelter, has_building(BuildingKind::Shelter)),
            (Milestone::Companion, has_agent(|k| matches!(k, AgentKind::Dog | AgentKind::Ox | AgentKind::Horse))),
            (Milestone::Charcoal, made(ItemKind::Charcoal)),
            (Milestone::Iron, made(ItemKind::IronBloom)),
            (Milestone::IronTools, made(ItemKind::IronAxe) && made(ItemKind::IronPickaxe)),
            // set when a water-driven trip hammer finishes its work (run_buildings)
            (Milestone::Mechanism, false),
            (Milestone::Automaton, has_agent(|k| k == AgentKind::Bot)),
        ];
        for (m, ok) in conditions {
            if ok && !self.milestone_done(m) {
                self.milestones |= m.bit();
                self.events.push(Event::Milestone(m));
            }
        }
        let bronze = [
            (Milestone::Bronze, made(ItemKind::Bronze)),
            (Milestone::BronzeTools, made(ItemKind::BronzeAxe) && made(ItemKind::BronzePickaxe)),
            (Milestone::Wheel, made(ItemKind::Cart)),
            (Milestone::Plow, made(ItemKind::Plow)),
        ];
        for (m, ok) in bronze {
            if ok && !self.milestone_done(m) {
                self.milestones |= m.bit();
                self.events.push(Event::Milestone(m));
            }
        }
        let copper = [
            (Milestone::FirstMetal, made(ItemKind::CopperIngot) || made(ItemKind::CopperAxeHead) || made(ItemKind::CopperPickHead) || made(ItemKind::CopperKnifeHead)),
            (Milestone::CopperTools, made(ItemKind::CopperAxe) && made(ItemKind::CopperPickaxe)),
        ];
        for (m, ok) in copper {
            if ok && !self.milestone_done(m) {
                self.milestones |= m.bit();
                self.events.push(Event::Milestone(m));
            }
        }
        if self.milestone_done(Milestone::FirstMetal) && self.milestone_done(Milestone::CopperTools) && !self.milestone_done(Milestone::CopperAge) {
            self.milestones |= Milestone::CopperAge.bit();
            self.events.push(Event::Milestone(Milestone::CopperAge));
        }
        let steel = [
            (Milestone::Steel, made(ItemKind::Steel)),
            (Milestone::CardBot, self.agents.iter().any(|a| a.kind == AgentKind::Bot && !a.route.is_empty())),
        ];
        for (m, ok) in steel {
            if ok && !self.milestone_done(m) {
                self.milestones |= m.bit();
                self.events.push(Event::Milestone(m));
            }
        }
        let steam_goals = [(Milestone::Coal, self.has_seen(ItemKind::Coal)), (Milestone::Glass, made(ItemKind::Glass))];
        for (m, ok) in steam_goals {
            if ok && !self.milestone_done(m) {
                self.milestones |= m.bit();
                self.events.push(Event::Milestone(m));
            }
        }
        let electronics = [
            (Milestone::Silicon, made(ItemKind::Silicon)),
            (Milestone::Transistor, made(ItemKind::Transistor)),
            (Milestone::CircuitBoard, made(ItemKind::CircuitBoard)),
            (Milestone::DesignWritten, self.custom_rules.is_some()),
        ];
        for (m, ok) in electronics {
            if ok && !self.milestone_done(m) {
                self.milestones |= m.bit();
                self.events.push(Event::Milestone(m));
            }
        }
        if [Milestone::Silicon, Milestone::Transistor, Milestone::CircuitBoard].iter().all(|m| self.milestone_done(*m)) && !self.milestone_done(Milestone::ElectronicsAge) {
            self.milestones |= Milestone::ElectronicsAge.bit();
            self.events.push(Event::Milestone(Milestone::ElectronicsAge));
        }
        if [Milestone::ComputerOn, Milestone::DesignWritten].iter().all(|m| self.milestone_done(*m)) && !self.milestone_done(Milestone::ComputerAge) {
            self.milestones |= Milestone::ComputerAge.bit();
            self.events.push(Event::Milestone(Milestone::ComputerAge));
        }
        let electric_age = [Milestone::Electricity, Milestone::ElectricMotor, Milestone::ElectricLight].iter().all(|m| self.milestone_done(*m));
        if electric_age && !self.milestone_done(Milestone::ElectricAge) {
            self.milestones |= Milestone::ElectricAge.bit();
            self.events.push(Event::Milestone(Milestone::ElectricAge));
        }
        let steam_age = [Milestone::Coal, Milestone::SteamPower, Milestone::DrainMine, Milestone::Glass].iter().all(|m| self.milestone_done(*m));
        if steam_age && !self.milestone_done(Milestone::SteamAge) {
            self.milestones |= Milestone::SteamAge.bit();
            self.events.push(Event::Milestone(Milestone::SteamAge));
        }
        let steel_age = [Milestone::Steel, Milestone::WindPower, Milestone::BeltLine, Milestone::Automaton, Milestone::CardBot].iter().all(|m| self.milestone_done(*m));
        if steel_age && !self.milestone_done(Milestone::SteelAge) {
            self.milestones |= Milestone::SteelAge.bit();
            self.events.push(Event::Milestone(Milestone::SteelAge));
        }
        let neolithic = [(Milestone::Harvest, made(ItemKind::Grain)), (Milestone::Bread, made(ItemKind::Bread))];
        for (m, ok) in neolithic {
            if ok && !self.milestone_done(m) {
                self.milestones |= m.bit();
                self.events.push(Event::Milestone(m));
            }
        }
        // the first winter is over (spring of the second year) and someone came through it fed
        let year = TICKS_PER_DAY * self.settings.season_days as u64 * 4;
        let wintered = self.tick >= year && self.agents.iter().any(|a| a.kind == AgentKind::Human && a.hunger > 0);
        if wintered && !self.milestone_done(Milestone::Winter) {
            self.milestones |= Milestone::Winter.bit();
            self.events.push(Event::Milestone(Milestone::Winter));
        }
        let farmers = self.milestone_done(Milestone::Harvest) && self.milestone_done(Milestone::Bread) && made(ItemKind::Pot) && self.milestone_done(Milestone::Winter);
        if farmers && !self.milestone_done(Milestone::NeolithicAge) {
            self.milestones |= Milestone::NeolithicAge.bit();
            self.events.push(Event::Milestone(Milestone::NeolithicAge));
        }
        let bronze_age = [Milestone::Bronze, Milestone::BronzeTools, Milestone::Wheel, Milestone::Plow, Milestone::AnimalPower].iter().all(|m| self.milestone_done(*m));
        if bronze_age && !self.milestone_done(Milestone::BronzeAge) {
            self.milestones |= Milestone::BronzeAge.bit();
            self.events.push(Event::Milestone(Milestone::BronzeAge));
        }
        // the automaton belongs to the steel and mechanical age now
        let all = conditions.iter().filter(|(m, _)| *m != Milestone::Automaton).all(|(m, _)| self.milestone_done(*m));
        if all && !self.milestone_done(Milestone::IronAge) {
            self.milestones |= Milestone::IronAge.bit();
            self.events.push(Event::Milestone(Milestone::IronAge));
        }
    }

    /// Advance one fixed step. Commands must be in a deterministic order (sorted by the caller/lockstep layer).
    pub fn step(&mut self, commands: &[Command]) {
        self.grow_chunks();
        for c in commands {
            if c.kind == CommandKind::Join {
                self.join_player();
            } else {
                self.apply_command(c);
            }
        }
        self.run_tasks();
        self.run_physics();
        self.run_taming();
        self.run_hunting();
        self.run_crafting();
        self.run_caves();
        self.run_routes();
        self.run_buildings();
        self.run_wildlife();
        self.run_needs();
        self.run_currents();
        self.run_spoilage();
        self.run_discoveries();
        self.run_camp();
        self.run_milestones();
        self.tick += 1;
    }

    /// Cheap state hash for desync detection / determinism tests (word-wise multiply-xorshift).
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        // one multiply per value (FNV over bytes was 8x slower and the hash covers every animal)
        let mut feed = |v: u64| {
            h = (h ^ v).wrapping_mul(0x9E37_79B9_7F4A_7C15);
            h ^= h >> 32;
        };
        feed(self.seed);
        feed(self.settings.season_days as u64 | (self.settings.difficulty as u64) << 8);
        if let Some((_, r)) = &self.custom_rules {
            feed(r.checksum);
        }
        feed(self.map.w as u64);
        feed(self.map.h as u64);
        feed(self.tick);
        feed(self.known);
        feed(self.milestones as u64);
        // the upper half only once items beyond 64 have been seen, so older states keep their hash
        feed(self.seen as u64);
        if self.seen >> 64 != 0 {
            feed((self.seen >> 64) as u64);
        }
        for p in self.produced {
            feed(p as u64);
        }
        feed(self.node_acc);
        for b in &self.buildings {
            feed(b.kind as u64);
            feed(b.x as u64);
            feed(b.y as u64);
            feed(b.fuel as u64);
            if let Some(j) = &b.job {
                feed(1 + j.recipe as u64);
                feed(j.remaining as u64);
            }
            feed(b.auto.map_or(0, |r| 1 + r as u64));
            feed(b.temp as u64 | (b.fertility as u64) << 16);
            for c in b.input {
                feed(c as u64);
            }
            for c in b.output {
                feed(c as u64);
            }
        }
        for w in &self.wild {
            feed(w.id as u64);
            feed(w.x as u64);
            feed(w.y as u64);
            feed(w.trust as u64);
        }
        for a in &self.agents {
            feed(a.id.0 as u64);
            feed(a.kind as u64);
            feed(a.x as u64);
            feed(a.y as u64);
            feed(a.health as u64);
            feed(a.hunger as u64);
            feed(a.injury as u64);
            feed(a.broken as u64);
            feed(a.stamina as u64 | (a.sprint as u64) << 32);
            feed(a.path.len() as u64 | (a.generation as u64) << 40);
            for &(k, n) in &a.wear {
                feed((k as u64) << 16 | n as u64);
            }
            feed(a.nutrition[0] as u64 | (a.nutrition[1] as u64) << 16 | (a.nutrition[2] as u64) << 32);
            if let Task::Craft { step, done, hits, score, stack, n, .. } = a.task {
                feed(0xC0DE_0000 | (done as u64) << 8 | hits as u64 | (score as u64) << 20 | (step as u64) << 40);
                for (c, w) in stack.iter().take(n as usize) {
                    feed((*c as u8 as u64) << 8 | *w as u64);
                }
            }
            if let Task::Cave { depth, light, act, work, .. } = a.task {
                feed(0xCA7E_0000 | (depth as u64) << 8 | act as u64 | (light as u64) << 24 | (work as u64) << 40);
            }
            if let Some(&(px, py)) = a.path.last() {
                feed((px as u64) << 32 | py as u64 & 0xffff_ffff);
            }
            feed(a.sick as u64);
            feed(a.thirst as u64);
            feed(a.warmth as u64);
            feed(a.route.len() as u64);
            feed(a.route_i as u64);
            for c in a.inventory.counts {
                feed(c as u64);
            }
        }
        for f in &self.air {
            feed(f.agent.0 as u64);
            for v in [f.x, f.y, f.z, f.vx, f.vy, f.vz, f.top] {
                feed(v as u64);
            }
        }
        for b in &self.bodies {
            feed(b.kind as u64 | (b.age as u64) << 8 | (b.rest as u64) << 40);
            feed(b.id as u64 | (b.wood as u64) << 32 | (b.item as u64) << 48);
            for v in [b.x, b.y, b.z, b.vx, b.vy, b.vz, b.angle, b.spin] {
                feed(v as u64);
            }
        }
        h
    }
}

/// An item's name as the rules file spells it (snake case).
fn serde_json_like(k: ItemKind) -> String {
    let name = format!("{k:?}");
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    out
}

/// Hidden deposits: one at most in each 24-tile cell, placed and filled from the seed alone (no map change, nothing
/// saved). Kind by chance: iron, copper, tin, coal, or rock oil; richness 1..=3.
pub fn deposit_in_cell(seed: u64, cx: i32, cy: i32) -> Option<(i32, i32, ItemKind, u8)> {
    let h = crate::rng::hash2(seed ^ 0xDE905, cx, cy);
    if h % 100 >= 35 {
        return None;
    }
    let tx = cx * 24 + ((h >> 8) % 20) as i32 + 2;
    let ty = cy * 24 + ((h >> 16) % 20) as i32 + 2;
    let kind = [ItemKind::IronOre, ItemKind::CopperOre, ItemKind::TinOre, ItemKind::Coal, ItemKind::Coal, ItemKind::Oil][((h >> 24) % 6) as usize];
    Some((tx, ty, kind, 1 + ((h >> 28) % 3) as u8))
}

/// Deposits whose centre is within `r` tiles of (tx, ty).
pub fn deposits_near(seed: u64, tx: i32, ty: i32, r: i32) -> Vec<(i32, i32, ItemKind, u8)> {
    let mut out = Vec::new();
    for cy in (ty - r).div_euclid(24)..=(ty + r).div_euclid(24) {
        for cx in (tx - r).div_euclid(24)..=(tx + r).div_euclid(24) {
            if let Some(d) = deposit_in_cell(seed, cx, cy) {
                if (d.0 - tx).pow(2) + (d.1 - ty).pow(2) <= r * r {
                    out.push(d);
                }
            }
        }
    }
    out
}

/// sin of an angle in milliradians, as permille (integer, deterministic; a parabola fit, good to about 6 permille).
fn isin_mrad(a: i64) -> i64 {
    let a = a.rem_euclid(6283);
    let (a, sign) = if a > 3141 { (a - 3141, -1) } else { (a, 1) };
    // 4x(pi-x)/pi^2 scaled
    sign * 4 * a * (3141 - a) / (3141 * 3141 / 1000)
}

/// The nearest water tile within `r` tiles of (x, y) mm, if any.
pub fn nearest_water_tile(w: &World, x: i64, y: i64, r: i32) -> Option<(i32, i32)> {
    let (tx, ty) = (x.div_euclid(TILE_MM) as i32, y.div_euclid(TILE_MM) as i32);
    for k in 0..=r {
        for dy in -k..=k {
            for dx in -k..=k {
                if dx.abs().max(dy.abs()) == k && w.map.terrain(tx + dx, ty + dy) == Some(Terrain::Water) {
                    return Some((tx + dx, ty + dy));
                }
            }
        }
    }
    None
}
