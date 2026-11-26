use alaz_sim::{AgentId, World, cave_terrain::CaveTile, caves::Cell};
use std::collections::BTreeMap;

pub(crate) type KnownTiles = BTreeMap<CaveTile, Cell>;

#[derive(PartialEq, Eq)]
struct Sight {
    tile: CaveTile,
    lit: bool,
    revision: u64,
    water_tick: u64,
}

/// Local exploration belongs to the player viewing the world.
#[derive(Default)]
pub(crate) struct CaveMap {
    known: KnownTiles,
    sight: Option<Sight>,
}

impl CaveMap {
    pub(crate) fn from_known(known: KnownTiles) -> Self {
        Self { known, sight: None }
    }

    pub(crate) fn known(&self) -> &KnownTiles {
        &self.known
    }

    pub(crate) fn observe(&mut self, world: &World, player: AgentId) {
        let Some(cave) = world.agents.get(player.0 as usize).and_then(|a| a.cave_view()) else {
            self.sight = None;
            return;
        };
        let z = cave.elevation_m();
        let (x, y) = cave.tile();
        let sight = Sight {
            tile: CaveTile { x, y, z },
            lit: cave.light > 0,
            revision: world.natural_cave_revision(),
            // Drainage can change while the player stands still.
            water_tick: if z <= -53 { world.tick / 30 } else { 0 },
        };
        if self.sight.as_ref() != Some(&sight) {
            self.known.extend(world.natural_cave_sight(cave));
            self.sight = Some(sight);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alaz_sim::{Command, CommandKind, ItemKind, map::tag_seed_natural};

    #[test]
    fn a_saved_cave_map_survives_leaving_the_cave_and_reopening_the_game() {
        let mut world = World::with_size(tag_seed_natural(42, 256), 256, 256);
        let node = world.node_at(222, 150).unwrap();
        let (x, y) = world.nodes[node.0 as usize].center_mm();
        let me = world.spawn_agent(x, y);
        world.agents[me.0 as usize].inventory.counts[ItemKind::Torch as usize] = 1;
        let position = world.cave_entrance_destination(node).unwrap();
        world.step(&[Command { agent: me, kind: CommandKind::MoveToFloor { position } }]);
        for _ in 0..100 {
            if world.agents[0].cave_view().is_some() { break; }
            world.step(&[]);
        }
        let mut map = CaveMap::default();
        map.observe(&world, me);
        assert!(map.known().values().any(|&cell| cell == Cell::Exit));
        assert!(map.known().len() > 1);

        let position = world.cave_exit_destination(me).unwrap();
        world.step(&[Command { agent: me, kind: CommandKind::MoveToFloor { position } }]);
        for _ in 0..240 {
            if world.agents[me.0 as usize].cave_view().is_none() {
                break;
            }
            world.step(&[]);
        }
        assert!(world.agents[me.0 as usize].cave_view().is_none());
        map.observe(&world, me);

        let dir =
            std::env::temp_dir().join(format!("alaz-exploration-reopen-{}", std::process::id()));
        crate::saveio::save_in(&dir, &world, me, map.known(), &Default::default()).unwrap();
        let restored = crate::saveio::load_in(&dir, me).unwrap().unwrap();
        assert_eq!(restored.world.hash(), world.hash());
        assert_eq!(CaveMap::from_known(restored.seen).known(), map.known());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
