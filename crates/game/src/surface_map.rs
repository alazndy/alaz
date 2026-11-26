//! What this player has seen of the surface. It belongs to the viewer, like the cave map: the sim never reads it.

use alaz_sim::map::{NodeKind, TILE_MM};
use alaz_sim::time::TICKS_PER_SECOND;
use alaz_sim::{AgentId, BuildingKind, World};
use std::collections::BTreeMap;

/// How far a person sees on the map, in tiles.
pub(crate) const SIGHT: i32 = 40;
/// Explored ground is kept in squares of this many tiles.
pub(crate) const CHUNK: i32 = 64;

pub(crate) type Tile = (i32, i32);
/// One word per row of a chunk; bit n is the n-th tile of the row.
pub(crate) type Mask = [u64; CHUNK as usize];

#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub(crate) struct Records {
    /// Keyed by chunk.
    pub explored: BTreeMap<Tile, Mask>,
    /// Keyed by tile, as last seen.
    pub resources: BTreeMap<Tile, NodeKind>,
    /// Keyed by tile, as last seen.
    pub buildings: BTreeMap<Tile, BuildingKind>,
    pub camp: Option<Tile>,
}

impl Records {
    pub fn is_explored(&self, tile: Tile) -> bool {
        let chunk = (tile.0.div_euclid(CHUNK), tile.1.div_euclid(CHUNK));
        self.explored.get(&chunk).is_some_and(|m| m[tile.1.rem_euclid(CHUNK) as usize] >> tile.0.rem_euclid(CHUNK) & 1 != 0)
    }

    pub fn is_empty(&self) -> bool {
        self.explored.is_empty() && self.resources.is_empty() && self.buildings.is_empty() && self.camp.is_none()
    }
}

/// Is this resource marked on the map? Trees and fallen branches are everywhere: the forest colour says enough.
pub(crate) fn mapped(kind: NodeKind) -> bool {
    !matches!(kind, NodeKind::Tree | NodeKind::Deadwood)
}

pub(crate) fn tile_of(x_mm: i64, y_mm: i64) -> Tile {
    (x_mm.div_euclid(TILE_MM) as i32, y_mm.div_euclid(TILE_MM) as i32)
}

/// Is `tile` inside the sight circle around `centre`?
pub(crate) fn in_sight(centre: Tile, tile: Tile) -> bool {
    let (dx, dy) = (i64::from(tile.0 - centre.0), i64::from(tile.1 - centre.1));
    dx * dx + dy * dy <= i64::from(SIGHT * SIGHT)
}

#[derive(Default)]
pub(crate) struct SurfaceMap {
    records: Records,
    /// Bumped for a chunk when new ground appears in it (its picture is then stale).
    versions: BTreeMap<Tile, u32>,
    /// Where and when the last look was.
    sight: Option<(Tile, u64)>,
    position: Option<(i64, i64)>,
    heading: Option<(f32, f32)>,
}

impl SurfaceMap {
    pub fn from_records(records: Records) -> Self {
        Self { records, ..Self::default() }
    }

    pub fn records(&self) -> &Records {
        &self.records
    }

    pub fn chunk_version(&self, chunk: Tile) -> u32 {
        self.versions.get(&chunk).copied().unwrap_or(0)
    }

    /// Unit vector of the last move; north before the first one.
    pub fn heading(&self) -> (f32, f32) {
        self.heading.unwrap_or((0.0, -1.0))
    }

    pub fn set_camp(&mut self, camp: Option<Tile>) {
        self.records.camp = camp;
    }

    /// Look around from where the player stands. Nothing is learned underground.
    pub fn observe(&mut self, world: &World, player: AgentId) {
        let Some(a) = world.agents.get(player.0 as usize).filter(|a| a.cave_view().is_none()) else {
            self.sight = None;
            self.position = None;
            return;
        };
        if let Some((x, y)) = self.position.replace((a.x, a.y)) {
            let (dx, dy) = ((a.x - x) as f32, (a.y - y) as f32);
            let len = dx.hypot(dy);
            if len > 0.0 {
                self.heading = Some((dx / len, dy / len));
            }
        }
        let centre = tile_of(a.x, a.y);
        // a new tile, or a second gone by (something in view may have changed); a clock that went back is a new world
        let due = self.sight.is_none_or(|(at, tick)| at != centre || world.tick < tick || world.tick - tick >= TICKS_PER_SECOND);
        if !due {
            return;
        }
        self.sight = Some((centre, world.tick));
        self.look(world, centre);
    }

    fn look(&mut self, world: &World, centre: Tile) {
        let mut grown = std::collections::BTreeSet::new();
        for ty in (centre.1 - SIGHT).max(0)..=(centre.1 + SIGHT).min(world.map.h - 1) {
            for tx in (centre.0 - SIGHT).max(0)..=(centre.0 + SIGHT).min(world.map.w - 1) {
                if !in_sight(centre, (tx, ty)) {
                    continue;
                }
                let chunk = (tx.div_euclid(CHUNK), ty.div_euclid(CHUNK));
                let row = &mut self.records.explored.entry(chunk).or_insert([0; CHUNK as usize])[ty.rem_euclid(CHUNK) as usize];
                let bit = 1u64 << tx.rem_euclid(CHUNK);
                if *row & bit == 0 {
                    *row |= bit;
                    grown.insert(chunk);
                }
            }
        }
        for chunk in grown {
            *self.versions.entry(chunk).or_insert(0) += 1;
        }

        forget_in_sight(&mut self.records.resources, centre);
        forget_in_sight(&mut self.records.buildings, centre);
        let (x0, y0, x1, y1) = (centre.0 - SIGHT, centre.1 - SIGHT, centre.0 + SIGHT + 1, centre.1 + SIGHT + 1);
        for n in world.nodes_in(x0, y0, x1, y1) {
            if mapped(n.kind) && in_sight(centre, (n.tx, n.ty)) {
                self.records.resources.insert((n.tx, n.ty), n.kind);
            }
        }
        for b in &world.buildings {
            let at = tile_of(b.x, b.y);
            if in_sight(centre, at) && world.map.in_bounds(at.0, at.1) {
                self.records.buildings.insert(at, b.kind);
            }
        }
    }
}

/// Drop what was remembered inside the sight circle: what is really there is written down again.
fn forget_in_sight<T>(memory: &mut BTreeMap<Tile, T>, centre: Tile) {
    // the keys order by x first, so one range covers the circle's columns without walking the whole memory
    let gone: Vec<Tile> = memory
        .range((centre.0 - SIGHT, i32::MIN)..=(centre.0 + SIGHT, i32::MAX))
        .map(|(&t, _)| t)
        .filter(|&t| in_sight(centre, t))
        .collect();
    for t in gone {
        memory.remove(&t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alaz_sim::{Building, Command, CommandKind, ItemKind, map::tag_seed_natural};

    fn world() -> (World, AgentId) {
        let mut w = World::with_size(42, 256, 256);
        let (x, y) = w.spawn_mm;
        let me = w.spawn_agent(x, y);
        (w, me)
    }

    fn stand(w: &mut World, me: AgentId, tile: Tile) {
        let a = &mut w.agents[me.0 as usize];
        a.x = i64::from(tile.0) * TILE_MM + TILE_MM / 2;
        a.y = i64::from(tile.1) * TILE_MM + TILE_MM / 2;
    }

    fn here(w: &World, me: AgentId) -> Tile {
        let a = &w.agents[me.0 as usize];
        tile_of(a.x, a.y)
    }

    /// A tile of the map more than the sight circle away from `from`.
    fn far_from(from: Tile) -> Tile {
        [(8, 8), (247, 247), (8, 247), (247, 8)]
            .into_iter()
            .find(|c| (c.0 - from.0).pow(2) + (c.1 - from.1).pow(2) > (SIGHT + 4).pow(2))
            .expect("a far corner")
    }

    #[test]
    fn sight_marks_a_circle_around_the_player() {
        let (mut w, me) = world();
        stand(&mut w, me, (100, 100));
        let mut map = SurfaceMap::default();
        map.observe(&w, me);
        let seen = map.records();
        for inside in [(100, 100), (140, 100), (100, 60), (128, 128)] {
            assert!(seen.is_explored(inside), "{inside:?}");
        }
        for outside in [(141, 100), (100, 59), (129, 129)] {
            assert!(!seen.is_explored(outside), "{outside:?}");
        }
    }

    #[test]
    fn sight_stops_at_the_edge_of_the_world() {
        let (mut w, me) = world();
        stand(&mut w, me, (0, 0));
        let mut map = SurfaceMap::default();
        map.observe(&w, me);
        assert!(map.records().is_explored((0, 0)));
        assert!(!map.records().is_explored((-1, 0)));
        assert!(map.records().explored.keys().all(|c| c.0 >= 0 && c.1 >= 0));
    }

    #[test]
    fn a_resource_is_remembered_until_the_spot_is_seen_again() {
        let (mut w, me) = world();
        let start = here(&w, me);
        let node = w
            .nodes
            .iter()
            .position(|n| n.amount > 0 && mapped(n.kind) && (n.tx - start.0).pow(2) + (n.ty - start.1).pow(2) <= 30 * 30)
            .expect("a resource near the start");
        let (at, kind) = ((w.nodes[node].tx, w.nodes[node].ty), w.nodes[node].kind);
        let mut map = SurfaceMap::default();
        map.observe(&w, me);
        assert_eq!(map.records().resources.get(&at), Some(&kind));

        stand(&mut w, me, far_from(at));
        w.nodes[node].amount = 0;
        w.tick += 60;
        map.observe(&w, me);
        assert_eq!(map.records().resources.get(&at), Some(&kind), "unseen changes stay unknown");

        stand(&mut w, me, start);
        w.tick += 60;
        map.observe(&w, me);
        assert_eq!(map.records().resources.get(&at), None);
    }

    #[test]
    fn trees_and_deadwood_are_not_remembered() {
        let (w, me) = world();
        let mut map = SurfaceMap::default();
        map.observe(&w, me);
        assert!(w.nodes.iter().any(|n| !mapped(n.kind)), "the world has trees");
        assert!(!map.records().resources.is_empty());
        assert!(map.records().resources.values().all(|&k| mapped(k)));
    }

    #[test]
    fn a_building_is_remembered_until_the_spot_is_seen_again() {
        let (mut w, me) = world();
        let start = here(&w, me);
        let at = (start.0 + 5, start.1);
        w.buildings.push(Building::new(BuildingKind::Campfire, i64::from(at.0) * TILE_MM + 500, i64::from(at.1) * TILE_MM + 500));
        let mut map = SurfaceMap::default();
        map.observe(&w, me);
        assert_eq!(map.records().buildings.get(&at), Some(&BuildingKind::Campfire));

        stand(&mut w, me, far_from(at));
        w.buildings.clear();
        w.tick += 60;
        map.observe(&w, me);
        assert_eq!(map.records().buildings.get(&at), Some(&BuildingKind::Campfire));

        stand(&mut w, me, start);
        w.tick += 60;
        map.observe(&w, me);
        assert!(map.records().buildings.is_empty());
    }

    #[test]
    fn a_change_in_view_shows_while_standing_still() {
        let (mut w, me) = world();
        let at = here(&w, me);
        let mut map = SurfaceMap::default();
        map.observe(&w, me);
        w.buildings.push(Building::new(BuildingKind::Campfire, i64::from(at.0 + 3) * TILE_MM + 500, i64::from(at.1) * TILE_MM + 500));
        w.tick += alaz_sim::time::TICKS_PER_SECOND;
        map.observe(&w, me);
        assert_eq!(map.records().buildings.len(), 1);
    }

    #[test]
    fn nothing_is_observed_underground() {
        let mut w = World::with_size(tag_seed_natural(42, 256), 256, 256);
        let node = w.node_at(222, 150).unwrap();
        let (x, y) = w.nodes[node.0 as usize].center_mm();
        let me = w.spawn_agent(x, y);
        w.agents[me.0 as usize].inventory.counts[ItemKind::Torch as usize] = 1;
        let position = w.cave_entrance_destination(node).unwrap();
        w.step(&[Command { agent: me, kind: CommandKind::MoveToFloor { position } }]);
        for _ in 0..100 {
            if w.agents[0].cave_view().is_some() {
                break;
            }
            w.step(&[]);
        }
        assert!(w.agents[0].cave_view().is_some());
        let mut map = SurfaceMap::default();
        map.observe(&w, me);
        assert!(map.records().is_empty());
    }

    #[test]
    fn a_chunk_version_changes_only_for_new_ground() {
        let (mut w, me) = world();
        let chunk = {
            let t = here(&w, me);
            (t.0.div_euclid(CHUNK), t.1.div_euclid(CHUNK))
        };
        let mut map = SurfaceMap::default();
        assert_eq!(map.chunk_version(chunk), 0);
        map.observe(&w, me);
        let first = map.chunk_version(chunk);
        assert!(first > 0);
        w.tick += 60;
        map.observe(&w, me);
        assert_eq!(map.chunk_version(chunk), first);
    }

    #[test]
    fn the_heading_follows_the_last_move() {
        let (mut w, me) = world();
        let mut map = SurfaceMap::default();
        assert_eq!(map.heading(), (0.0, -1.0));
        map.observe(&w, me);
        w.agents[me.0 as usize].x += 700;
        map.observe(&w, me);
        assert_eq!(map.heading(), (1.0, 0.0));
        map.observe(&w, me);
        assert_eq!(map.heading(), (1.0, 0.0), "standing still keeps it");
    }

    #[test]
    fn the_camp_mark_is_set_and_removed() {
        let mut map = SurfaceMap::default();
        map.set_camp(Some((3, 4)));
        assert_eq!(map.records().camp, Some((3, 4)));
        assert!(!map.records().is_empty());
        map.set_camp(None);
        assert!(map.records().is_empty());
    }
}
