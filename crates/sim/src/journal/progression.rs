//! Read-only discovery paths. Production and construction come from the loaded rules.

use super::DISCOVERY_ORDER;
use crate::caves::CAVE_LEVELS;
use crate::item_guide::{cave_depths, found_in};
use crate::rules::{Rules, ToolKind};
use crate::{BuildingKind, Discovery, ItemKind, NodeKind, World};
use std::sync::{Arc, Mutex};

use BuildingKind as B;
use Discovery as D;
use ItemKind as I;

const DISCOVERY_SLOTS: usize = 64;
const BUILDING_START: usize = DISCOVERY_SLOTS + I::COUNT;
const FACT_COUNT: usize = BUILDING_START + B::ALL.len();

/// The main path follows the technology that changes what a tribe can make at each age.
const MAIN: &[Discovery] = &[
    D::SharpStone,
    D::StoneAxe,
    D::Fire,
    D::Pottery,
    D::Casting,
    D::Alloying,
    D::Bronzework,
    D::Smelting,
    D::Ironwork,
    D::Steelmaking,
    D::Steam,
    D::Electricity,
    D::Electronics,
    D::Computing,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressionState {
    Known,
    Next,
    Hidden,
}

#[derive(Clone, Debug)]
pub struct ProgressionNode {
    pub discovery: Discovery,
    /// Discoveries required by every route, with indirect links removed.
    pub prerequisites: Vec<Discovery>,
    /// Zero-based age band: stone, metal, machine, and computing.
    pub band: u8,
    /// Lane zero is the main path.
    pub lane: u8,
    /// Position within the band.
    pub column: u8,
    pub main: bool,
    reachable: bool,
    routes: Arc<Routes>,
}

impl ProgressionNode {
    pub fn state(&self, world: &World) -> ProgressionState {
        if world.knows(self.discovery) {
            ProgressionState::Known
        } else if self.reachable && self.routes.ready(world.known) & bit(self.discovery) != 0 {
            ProgressionState::Next
        } else {
            ProgressionState::Hidden
        }
    }

    /// The next direct step toward this discovery, or the discovery itself when it is ready.
    pub fn next(&self, world: &World) -> Option<Discovery> {
        match self.state(world) {
            ProgressionState::Known => None,
            ProgressionState::Next => Some(self.discovery),
            ProgressionState::Hidden if self.reachable => self
                .prerequisites
                .iter()
                .copied()
                .find(|&d| !world.knows(d))
                .or_else(|| self.routes.alternative(world.known, self.discovery)),
            ProgressionState::Hidden => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Progression {
    nodes: Vec<ProgressionNode>,
}

impl Progression {
    pub fn new(rules: &Rules) -> Self {
        let facts = Facts::new(rules);
        let reachable = facts.reach(0, 0);
        let mut requirements = [0_u64; DISCOVERY_SLOTS];

        // If a discovery still opens with another disabled, that other discovery is optional.
        // Evaluating complete routes this way accounts for alternative products and prices.
        for required in DISCOVERY_ORDER.into_iter().filter(|&d| reachable[known(d)]) {
            let missing = facts.reach(0, bit(required));
            for target in DISCOVERY_ORDER.into_iter().filter(|&d| d != required && reachable[known(d)]) {
                if !missing[known(target)] {
                    requirements[known(target)] |= bit(required);
                }
            }
        }

        let routes = Arc::new(Routes {
            facts,
            cache: Mutex::new(None),
        });
        let mut nodes = Vec::with_capacity(DISCOVERY_ORDER.len());
        let mut placed = 0_u64;
        let mut positions = [(0_u8, 0_u8); DISCOVERY_SLOTS];
        let mut occupied = [[0_u8; 6]; 4];
        while nodes.len() < DISCOVERY_ORDER.len() {
            let discovery = DISCOVERY_ORDER
                .into_iter()
                .find(|&d| placed & bit(d) == 0 && requirements[known(d)] & !placed == 0)
                .expect("discovery relations are acyclic");
            let mask = requirements[known(discovery)];
            let implied = DISCOVERY_ORDER
                .into_iter()
                .filter(|&d| mask & bit(d) != 0)
                .fold(0, |combined, d| combined | requirements[known(d)]);
            let prerequisites: Vec<_> = DISCOVERY_ORDER.into_iter().filter(|&d| mask & !implied & bit(d) != 0).collect();

            let (mut band, lane) = placement(discovery);
            for &d in &prerequisites {
                band = band.max(positions[known(d)].0);
            }
            let column = prerequisites.iter().fold(occupied[band as usize][lane as usize], |column, &d| {
                let (previous_band, previous_column) = positions[known(d)];
                if previous_band == band {
                    column.max(previous_column + 1)
                } else {
                    column
                }
            });
            occupied[band as usize][lane as usize] = column + 1;
            positions[known(discovery)] = (band, column);
            placed |= bit(discovery);
            nodes.push(ProgressionNode {
                discovery,
                prerequisites,
                band,
                lane,
                column,
                main: lane == 0,
                reachable: reachable[known(discovery)],
                routes: Arc::clone(&routes),
            });
        }
        nodes.sort_by_key(|node| (node.band, node.column, node.lane));
        Self { nodes }
    }

    pub fn nodes(&self) -> &[ProgressionNode] {
        &self.nodes
    }

    pub fn node(&self, discovery: Discovery) -> Option<&ProgressionNode> {
        self.nodes.iter().find(|node| node.discovery == discovery)
    }

    /// Suggest the next step on the main route. Branches remain available on the tree.
    pub fn next(&self, world: &World) -> Option<Discovery> {
        MAIN.iter()
            .copied()
            .find_map(|discovery| self.next_for(discovery, world))
            .or_else(|| {
                self.nodes
                    .iter()
                    .find(|node| node.state(world) == ProgressionState::Next)
                    .map(|node| node.discovery)
            })
    }

    /// Resolve the first playable step toward a selected discovery.
    pub fn next_for(&self, discovery: Discovery, world: &World) -> Option<Discovery> {
        let mut node = self.node(discovery)?;
        while node.reachable {
            let next = node.next(world)?;
            if next == node.discovery {
                return Some(next);
            }
            node = self.node(next)?;
        }
        None
    }
}

fn placement(discovery: Discovery) -> (u8, u8) {
    use Discovery::*;
    let band = match discovery {
        Casting | Alloying | Bronzework | Draught | Geology | Smelting | Ironwork | Workshop | Mechanics | Drawing | OilPressing
        | Hoisting => 1,
        Steelmaking | Steam | Masonry | Cement | Foundry | Converter | Machining | Glassblowing | Chemistry | Spinning => 2,
        Electricity | Electronics | Computing | HeavyCurrent | ArcSmelting | Semiconductors | Automation => 3,
        _ => 0,
    };
    let lane = if MAIN.contains(&discovery) {
        0
    } else {
        match discovery {
            Hammer | Workbench | Furnace | Workshop | Mechanics | Machining | Converter | Drawing | HeavyCurrent | ArcSmelting => 1,
            Shelter | Depot | Husbandry | Draught | Geology | Masonry | Cement | Foundry | Wells | Hoisting => 2,
            Spear | Cooking | Fishing | Curing | Smoking | Farming | Milling | Medicine | OilPressing => 3,
            Clothing | Clamp | Weaving | Ropemaking | Felting | Ginning | Spinning | Tanning => 4,
            _ => 5,
        }
    };
    (band, lane)
}

fn bit(d: Discovery) -> u64 {
    1 << known(d)
}

const fn known(d: Discovery) -> usize {
    d as usize
}

const fn item(i: ItemKind) -> usize {
    DISCOVERY_SLOTS + i as usize
}

const fn building(b: BuildingKind) -> usize {
    BUILDING_START + b as usize
}

#[derive(Debug)]
struct Routes {
    facts: Facts,
    cache: Mutex<Option<RouteCache>>,
}

#[derive(Debug)]
struct RouteCache {
    known: u64,
    ready: u64,
    alternatives: [Option<Option<Discovery>>; DISCOVERY_SLOTS],
}

impl Routes {
    fn cache_for<'a>(&self, cache: &'a mut Option<RouteCache>, known: u64) -> &'a mut RouteCache {
        if cache.as_ref().is_none_or(|entry| entry.known != known) {
            *cache = Some(RouteCache {
                known,
                ready: self.facts.ready(known),
                alternatives: [None; DISCOVERY_SLOTS],
            });
        }
        cache.as_mut().expect("the cache is initialized")
    }

    fn ready(&self, known: u64) -> u64 {
        self.cache_for(&mut self.cache.lock().expect("route cache"), known).ready
    }

    fn alternative(&self, learned: u64, target: Discovery) -> Option<Discovery> {
        let mut cache = self.cache.lock().expect("route cache");
        let cache = self.cache_for(&mut cache, learned);
        let ready = cache.ready;
        *cache.alternatives[known(target)].get_or_insert_with(|| {
            // Drop each optional step while preserving the target to find one complete route.
            let mut excluded = 0;
            for discovery in DISCOVERY_ORDER.into_iter().rev() {
                if discovery == target || learned & bit(discovery) != 0 {
                    continue;
                }
                let without = excluded | bit(discovery);
                if self.facts.reach(learned, without)[known(target)] {
                    excluded = without;
                }
            }
            DISCOVERY_ORDER.into_iter().find(|&d| (ready & !learned & !excluded) & bit(d) != 0)
        })
    }
}

#[derive(Debug)]
struct Transition {
    needs: Vec<usize>,
    gives: Vec<usize>,
}

#[derive(Debug, Default)]
struct Facts {
    transitions: Vec<Transition>,
    learning: Vec<Transition>,
}

impl Facts {
    fn add(&mut self, needs: impl IntoIterator<Item = usize>, gives: impl IntoIterator<Item = usize>) {
        self.transitions.push(Transition {
            needs: needs.into_iter().collect(),
            gives: gives.into_iter().collect(),
        });
    }

    fn tools(&mut self, rules: &Rules, kind: ToolKind, tier: u8, gives: usize) {
        for tool in rules.tools.iter().filter(|tool| tool.kind == kind && tool.tier >= tier) {
            self.add([item(tool.item)], [gives]);
        }
    }

    fn new(rules: &Rules) -> Self {
        let mut facts = Self::default();
        for kind in I::ALL {
            for node in found_in(kind).filter(|&node| node != NodeKind::CaveMouth) {
                match rules.gate(node) {
                    Some(gate) if gate.loose == 0 || kind == I::Limestone => {
                        facts.tools(rules, gate.tool, gate.tier, item(kind));
                    }
                    _ => facts.add([], [item(kind)]),
                }
            }
            for depth in cave_depths(kind).filter(|&depth| depth < CAVE_LEVELS) {
                facts.tools(rules, ToolKind::Pick, rules.cave_tier(depth), item(kind));
            }
        }

        facts.add([], [item(I::Manure)]);
        facts.add([item(I::Spear)], [item(I::RawMeat)]);
        for knife in [I::Knife, I::CopperKnife] {
            facts.add([item(I::Spear), item(knife)], [item(I::Hide)]);
        }
        facts.add([item(I::Pot)], [item(I::WaterPot)]);
        facts.add([item(I::Spear)], [item(I::RawFish)]);
        facts.add([item(I::FishingRod)], [item(I::RawFish)]);
        facts.add([building(B::Field), item(I::Seed), item(I::Hoe)], [item(I::Grain)]);

        for def in rules.buildings.iter().filter(|def| def.discovery != D::Retired) {
            for cost in def.price_lists() {
                let needs = [known(def.discovery)]
                    .into_iter()
                    .chain(def.needs.iter().copied().map(item))
                    .chain(cost.iter().map(|count| item(count.item)));
                facts.add(needs, [building(def.kind)]);
            }
        }
        for recipe in rules.recipes.iter().filter(|recipe| recipe.discovery != D::Retired) {
            let mut needs: Vec<_> = [known(recipe.discovery)]
                .into_iter()
                .chain(recipe.inputs.iter().map(|count| item(count.item)))
                .chain(recipe.needs.iter().copied().map(item))
                .chain(recipe.station.into_iter().chain(recipe.near).map(building))
                .collect();
            let mut gives: Vec<_> = recipe.outputs.iter().map(|count| item(count.item)).collect();
            if let Some(alloy) = recipe.alloy {
                needs.extend([item(alloy.base), item(alloy.add)]);
                gives.extend([item(alloy.output), item(alloy.bad)]);
            }
            if recipe.mine {
                gives.extend([I::IronOre, I::CopperOre, I::TinOre, I::Coal, I::Oil, I::Bauxite].map(item));
            }
            if !gives.is_empty() {
                facts.add(needs, gives);
            }
        }

        for discovery in DISCOVERY_ORDER {
            for route in learning_routes(discovery) {
                facts.learning.push(Transition {
                    needs: route.to_vec(),
                    gives: vec![known(discovery)],
                });
            }
        }
        facts
    }

    fn ready(&self, learned: u64) -> u64 {
        let reached = self.closure(learned, 0, false);
        self.learning
            .iter()
            .filter(|transition| transition.needs.iter().all(|&fact| reached[fact]))
            .fold(0, |mask, transition| mask | (1 << transition.gives[0]))
    }

    fn reach(&self, learned: u64, excluded: u64) -> [bool; FACT_COUNT] {
        self.closure(learned, excluded, true)
    }

    fn closure(&self, learned: u64, excluded: u64, discover: bool) -> [bool; FACT_COUNT] {
        let mut reached = [false; FACT_COUNT];
        for (discovery, reached) in reached[..DISCOVERY_SLOTS].iter_mut().enumerate() {
            *reached = learned & (1 << discovery) != 0;
        }
        loop {
            let mut changed = false;
            let learning = self.learning.iter().filter(|_| discover);
            for transition in self.transitions.iter().chain(learning) {
                if transition.needs.iter().all(|&fact| reached[fact]) {
                    for &fact in &transition.gives {
                        if !reached[fact] && (fact >= DISCOVERY_SLOTS || excluded & (1 << fact) == 0) {
                            reached[fact] = true;
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                return reached;
            }
        }
    }
}

/// Discovery conditions in World::run_discoveries, projected to technology.
/// Counts, taming, and inventory fullness are player actions, so they don't add discovery edges.
fn learning_routes(discovery: Discovery) -> &'static [&'static [usize]] {
    match discovery {
        D::SharpStone => const { &[&[item(I::Stone)]] },
        D::StoneAxe => const { &[&[item(I::SharpStone), item(I::Branch)]] },
        D::Fire => const { &[&[item(I::Wood)]] },
        D::Shelter => const { &[&[known(D::Fire), item(I::Wood)]] },
        D::Kiln => const { &[&[building(B::Campfire), item(I::Wood)]] },
        D::Furnace => const { &[&[item(I::Clay), item(I::Stone)]] },
        D::Smelting => const { &[&[known(D::Furnace), known(D::Bronzework), item(I::IronOre)]] },
        D::Depot => const { &[&[]] },
        D::Workshop => const { &[&[item(I::IronBloom)]] },
        D::Workbench => const { &[&[known(D::StoneAxe), item(I::Wood)]] },
        D::Spear => const { &[&[item(I::SharpStone), item(I::Branch), item(I::Fiber)]] },
        D::Cooking => const { &[&[known(D::Fire), item(I::RawMeat)]] },
        D::Ironwork => const { &[&[known(D::Workbench), item(I::IronBloom)]] },
        D::Mechanics => const { &[&[known(D::Ironwork), item(I::Wood)]] },
        D::Medicine => const { &[&[item(I::Herb)]] },
        D::Alloying => const { &[&[known(D::Furnace), item(I::CopperOre), item(I::TinOre)]] },
        D::Bronzework => const { &[&[known(D::Workbench), item(I::Bronze)]] },
        D::Clamp => const { &[&[item(I::Branch), item(I::Fiber)]] },
        D::Clothing => const { &[&[item(I::Fiber)], &[item(I::Hide)]] },
        D::Farming => const { &[&[known(D::StoneAxe), item(I::Seed)]] },
        D::Milling => const { &[&[known(D::Fire), item(I::Grain)]] },
        D::Pottery => const { &[&[known(D::Fire), known(D::Farming), item(I::Clay)]] },
        D::Weaving => const { &[&[known(D::Clothing), known(D::Farming), item(I::Fiber)]] },
        D::Husbandry => const { &[&[known(D::Farming), item(I::Wood)]] },
        D::Draught => const { &[&[known(D::Bronzework)]] },
        D::Hammer => const { &[&[known(D::SharpStone), item(I::Stone), item(I::Branch)]] },
        D::Casting => const { &[&[known(D::Pottery), known(D::Fire), item(I::CopperOre)]] },
        D::Steelmaking => const { &[&[known(D::Ironwork), item(I::Limestone)], &[known(D::Ironwork), item(I::FireClay)]] },
        D::Steam => const { &[&[known(D::Steelmaking), item(I::Coal)], &[known(D::Steelmaking), item(I::Sand)]] },
        D::Electricity => const { &[&[known(D::Steam), item(I::CopperIngot)]] },
        D::Electronics => const { &[&[known(D::Electricity), item(I::Sand), item(I::Coke)]] },
        D::Computing => const { &[&[item(I::CircuitBoard)], &[item(I::Chip)]] },
        D::Geology => const { &[&[known(D::Bronzework)]] },
        D::Fishing => const { &[&[known(D::Spear)], &[item(I::RawFish)]] },
        D::Masonry => {
            const {
                &[
                    &[known(D::Steelmaking), item(I::Limestone)],
                    &[known(D::Steelmaking), item(I::FireClay)],
                ]
            }
        }
        D::Cement => const { &[&[known(D::Masonry), item(I::Lime)]] },
        D::Foundry => const { &[&[known(D::Steelmaking), item(I::PigIron)]] },
        D::Converter => const { &[&[known(D::Steam), known(D::Foundry), item(I::Steel)]] },
        D::Machining => const { &[&[known(D::Steam), item(I::CastIronPart)]] },
        D::Throwing => const { &[&[known(D::Pottery), item(I::Plank)]] },
        D::OilPressing => const { &[&[known(D::Mechanics), item(I::Linseed)], &[known(D::Mechanics), item(I::Seed)]] },
        D::Tanning => const { &[&[known(D::Pottery), item(I::Hide)]] },
        D::Curing => {
            const {
                &[
                    &[known(D::Fire), item(I::Berries)],
                    &[known(D::Fire), item(I::RawMeat)],
                    &[known(D::Fire), item(I::RawFish)],
                ]
            }
        }
        D::Smoking => const { &[&[known(D::Curing), building(B::DryingRack)]] },
        D::Ropemaking => const { &[&[known(D::Weaving), item(I::Fiber)]] },
        D::Wells => const { &[&[known(D::Pottery), item(I::Rope)]] },
        D::Felting => const { &[&[known(D::Weaving), known(D::Pottery), item(I::Fiber)]] },
        D::Hoisting => const { &[&[known(D::Mechanics), item(I::Rope)]] },
        D::Glassblowing => const { &[&[item(I::Glass)]] },
        D::Papermaking => const { &[&[known(D::Weaving), known(D::Pottery), item(I::Cloth)]] },
        D::Printing => const { &[&[known(D::Mechanics), known(D::Papermaking), item(I::Paper)]] },
        D::Ginning => const { &[&[item(I::Cotton)], &[known(D::Mechanics), known(D::Weaving), item(I::Thread)]] },
        D::Spinning => const { &[&[known(D::Machining), item(I::Thread)]] },
        D::Drawing => {
            const {
                &[
                    &[known(D::Mechanics), item(I::CopperIngot)],
                    &[known(D::Mechanics), item(I::Sheet)],
                    &[known(D::Mechanics), item(I::Steel)],
                ]
            }
        }
        D::HeavyCurrent => const { &[&[known(D::Electricity), building(B::Dynamo)]] },
        D::ArcSmelting => {
            const {
                &[
                    &[known(D::HeavyCurrent), item(I::Steel)],
                    &[known(D::HeavyCurrent), item(I::PigIron)],
                    &[known(D::HeavyCurrent), item(I::CopperIngot)],
                    &[known(D::HeavyCurrent), item(I::Bauxite)],
                ]
            }
        }
        D::Chemistry => {
            const {
                &[
                    &[known(D::Steam), item(I::Tar)],
                    &[known(D::Steam), item(I::Oil)],
                    &[known(D::Steam), item(I::Lime)],
                ]
            }
        }
        D::Semiconductors => const { &[&[known(D::Electronics), item(I::Silicon)]] },
        D::Automation => {
            const {
                &[
                    &[known(D::Electronics), item(I::Transistor)],
                    &[known(D::Electronics), item(I::CircuitBoard)],
                    &[known(D::Electronics), item(I::Chip)],
                ]
            }
        }
        D::Retired => const { &[] },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_taught_discovery_has_a_playable_acyclic_path() {
        let tree = Progression::new(crate::rules());
        let mut positions = HashSet::new();
        let mut placed = 0;
        for node in tree.nodes() {
            assert!(node.reachable, "{:?}: no route from world resources", node.discovery);
            assert_eq!(placed & bit(node.discovery), 0);
            assert!(positions.insert((node.band, node.lane, node.column)));
            assert!(
                node.prerequisites.iter().all(|&d| placed & bit(d) != 0),
                "{:?}: {:?}",
                node.discovery,
                node.prerequisites
            );
            placed |= bit(node.discovery);
        }
        assert_eq!(placed, DISCOVERY_ORDER.into_iter().map(bit).fold(0, |sum, flag| sum | flag));
        assert!(MAIN.windows(2).all(|pair| {
            let first = tree.node(pair[0]).unwrap();
            let second = tree.node(pair[1]).unwrap();
            first.main && second.main && (first.band, first.column) < (second.band, second.column)
        }));
    }

    #[test]
    fn production_paths_use_tools_stations_and_real_unlocks() {
        let tree = Progression::new(crate::rules());
        let needs = |discovery| tree.node(discovery).unwrap().prerequisites.as_slice();
        fn requires(tree: &Progression, target: Discovery, required: Discovery) -> bool {
            tree.node(target)
                .unwrap()
                .prerequisites
                .iter()
                .any(|&discovery| discovery == required || requires(tree, discovery, required))
        }
        assert_eq!(needs(D::StoneAxe), &[D::SharpStone]);
        assert!(requires(&tree, D::Cooking, D::Spear), "hunting for meat uses a spear");
        assert!(needs(D::Alloying).contains(&D::Casting), "tin needs a copper pick");
        assert_eq!(needs(D::Kiln), &[D::Fire], "kiln needs a built campfire");
        assert!(needs(D::Smelting).contains(&D::Bronzework));
        assert!(requires(&tree, D::Ironwork, D::Hammer), "refining iron needs a hammer");
        assert_eq!(needs(D::Computing), &[D::Electronics], "chips are an alternative to circuit boards");
        assert!(needs(D::HeavyCurrent).contains(&D::Electricity));
        assert!(
            !needs(D::Machining).contains(&D::Foundry),
            "the blast furnace also casts iron parts"
        );
        assert!(needs(D::Machining).contains(&D::Steam));
    }

    #[test]
    fn custom_rules_change_dependencies_without_requiring_all_producers() {
        let mut rules = Rules::parse(include_str!("../../data/rules.toml")).unwrap();
        let recipe = rules.recipes.iter_mut().find(|r| r.id == "sharp_stone").unwrap();
        recipe.discovery = D::Medicine;
        let tree = Progression::new(&rules);
        assert_eq!(tree.node(D::StoneAxe).unwrap().prerequisites, [D::Medicine]);

        let mut alternate = rules.recipes.iter().find(|r| r.id == "sharp_stone").unwrap().clone();
        alternate.discovery = D::Clothing;
        rules.recipes.push(alternate);
        let tree = Progression::new(&rules);
        let axe = tree.node(D::StoneAxe).unwrap();
        assert!(axe.prerequisites.is_empty());
        let mut world = World::with_size(42, 64, 64);
        assert_eq!(axe.state(&world), ProgressionState::Hidden);
        assert!(matches!(tree.next_for(D::StoneAxe, &world), Some(D::Medicine | D::Clothing)));
        world.learn(D::Clothing);
        assert_eq!(axe.state(&world), ProgressionState::Next);
        assert_eq!(tree.next_for(D::StoneAxe, &world), Some(D::StoneAxe));
    }

    #[test]
    fn building_prices_change_only_the_routes_that_use_them() {
        let mut rules = Rules::parse(include_str!("../../data/rules.toml")).unwrap();
        let dynamo = rules.buildings.iter_mut().find(|b| b.kind == B::Dynamo).unwrap();
        dynamo.cost.push(crate::rules::ItemCount { item: I::Paper, n: 1 });
        dynamo.alt_cost.clear();
        let tree = Progression::new(&rules);
        let current = tree.node(D::HeavyCurrent).unwrap();
        assert!(current.prerequisites.contains(&D::Papermaking));
        assert!(!tree.node(D::Electricity).unwrap().prerequisites.contains(&D::Papermaking));
    }

    #[test]
    fn known_steps_unlock_the_next_playable_hint() {
        let tree = Progression::new(crate::rules());
        let mut world = World::with_size(42, 64, 64);
        world.forget_everything();
        let axe = tree.node(D::StoneAxe).unwrap();
        assert_eq!(axe.state(&world), ProgressionState::Hidden);
        assert_eq!(tree.next_for(D::StoneAxe, &world), Some(D::SharpStone));
        assert_eq!(tree.next(&world), Some(D::SharpStone));

        world.learn(D::SharpStone);
        assert_eq!(axe.state(&world), ProgressionState::Next);
        assert_eq!(tree.next(&world), Some(D::StoneAxe));

        world.learn(D::StoneAxe);
        assert_eq!(axe.state(&world), ProgressionState::Known);
        assert_eq!(tree.next(&world), Some(D::Fire));
    }
}
