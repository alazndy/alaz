use super::{CaveTerrain, Material};
use crate::map::{Map, TILE_MM, Terrain};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, VecDeque};

pub(crate) const SIDES: [(i32, i32); 4] = [(0, 1), (1, 0), (0, -1), (-1, 0)];
const VOXEL_H_MM: i64 = 1_000;

/// A one-metre voxel. Its bottom face is at Z metres above sea level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CaveTile {
    pub x: i32,
    pub y: i32,
    pub z: i16,
}

impl CaveTile {
    pub fn xy(self) -> (i32, i32) {
        (self.x, self.y)
    }

    pub fn center_mm(self) -> (i64, i64) {
        (
            (i64::from(self.x) * TILE_MM) + TILE_MM / 2,
            (i64::from(self.y) * TILE_MM) + TILE_MM / 2,
        )
    }

    fn at_height(self, z: i16) -> Self {
        Self { z, ..self }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaveMouth {
    pub landing: CaveTile,
    pub threshold: CaveTile,
    /// Cardinal approach from the surface anchor into the opening.
    pub direction: (i32, i32),
}

/// The profile made by the ground and the first two walkable cave floors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaveEntrance {
    Hillside { roof_mm: i64 },
    Descent,
    Open,
}

impl CaveMouth {
    /// Describe an existing entrance without moving its floors or surface anchor.
    pub fn entrance(self, map: &Map) -> CaveEntrance {
        let (x, y) = self.threshold.center_mm();
        let (dx, dy) = (i64::from(self.direction.0), i64::from(self.direction.1));
        let approach = i64::from(crate::map::elevation_at_mm(
            map.seed,
            x - dx * TILE_MM,
            y - dy * TILE_MM,
        ));
        let threshold = i64::from(self.threshold.z) * VOXEL_H_MM;
        let (x, y) = self.landing.center_mm();
        let ridge = i64::from(crate::map::elevation_at_mm(map.seed, x, y));
        let landing = i64::from(self.landing.z) * VOXEL_H_MM;

        // A lateral opening needs an approach near floor level and a solid roof behind its lip.
        // Test thickness as well as clearance so a thin surface skin cannot make a rock brow.
        if (threshold - approach).abs() <= 750 && ridge - approach >= 1_750 {
            let column = map.cave_field().column(x, y);
            let first_clear_roof = threshold.max(landing) + 2_000;
            if let Some(roof_mm) = (first_clear_roof..=ridge - 250).step_by(250).find(|&z| {
                column.sample(z).material() == Material::Rock
                    && column.sample(z + 250).material() == Material::Rock
            }) {
                return CaveEntrance::Hillside { roof_mm };
            }
        }
        if approach - threshold.min(landing) >= 750 {
            CaveEntrance::Descent
        } else {
            CaveEntrance::Open
        }
    }
}

/// A short-lived cache for a navigation query. Excavated cells are shared by world position.
pub(crate) struct CaveNav<'a> {
    map: &'a Map,
    terrain: CaveTerrain,
    dug: &'a BTreeSet<CaveTile>,
    rubble: Option<&'a BTreeSet<CaveTile>>,
    air: HashMap<CaveTile, Material>,
}

impl<'a> CaveNav<'a> {
    pub(crate) fn new(map: &'a Map, dug: &'a BTreeSet<CaveTile>) -> Self {
        Self::with_field(map, dug, map.cave_field())
    }

    pub(super) fn with_field(
        map: &'a Map,
        dug: &'a BTreeSet<CaveTile>,
        terrain: CaveTerrain,
    ) -> Self {
        Self {
            map,
            terrain,
            dug,
            rubble: None,
            air: HashMap::new(),
        }
    }

    pub(crate) fn with_rubble(mut self, rubble: &'a BTreeSet<CaveTile>) -> Self {
        self.rubble = Some(rubble);
        self
    }

    fn material(&mut self, tile: CaveTile) -> Material {
        *self.air.entry(tile).or_insert_with(|| {
            if !self.map.in_bounds(tile.x, tile.y) {
                return Material::Rock;
            }
            if self.dug.contains(&tile) {
                return Material::CaveAir;
            }
            let (x_mm, y_mm) = tile.center_mm();
            self.terrain
                .column(x_mm, y_mm)
                .sample(i64::from(tile.z) * VOXEL_H_MM + VOXEL_H_MM / 2)
                .material()
        })
    }

    pub(crate) fn floor(&mut self, tile: CaveTile) -> bool {
        self.material(tile) == Material::CaveAir
            && self.material(tile.at_height(tile.z + 1)) != Material::Rock
            && self.material(tile.at_height(tile.z - 1)) == Material::Rock
    }

    pub(crate) fn is_rock(&mut self, tile: CaveTile) -> bool {
        self.material(tile) == Material::Rock
    }

    pub(crate) fn next_floor(&mut self, from: CaveTile, x: i32, y: i32) -> Option<CaveTile> {
        self.adjacent_floor(from, x, y)
            .filter(|tile| self.rubble.is_none_or(|rubble| !rubble.contains(tile)))
    }

    pub(crate) fn adjacent_rubble(&mut self, from: CaveTile, x: i32, y: i32) -> Option<CaveTile> {
        self.adjacent_floor(from, x, y)
            .filter(|tile| self.rubble.is_some_and(|rubble| rubble.contains(tile)))
    }

    fn adjacent_floor(&mut self, from: CaveTile, x: i32, y: i32) -> Option<CaveTile> {
        if (x - from.x).abs() + (y - from.y).abs() != 1 {
            return None;
        }
        [from.z, from.z - 1, from.z + 1].into_iter().find_map(|z| {
            let to = CaveTile { x, y, z };
            if !self.floor(to) {
                return None;
            }
            let lower = if from.z < z { from } else { to };
            (from.z == z || self.material(lower.at_height(lower.z + 2)) != Material::Rock)
                .then_some(to)
        })
    }

    pub(crate) fn walls(&mut self, from: CaveTile, x: i32, y: i32) -> bool {
        (x - from.x).abs() + (y - from.y).abs() == 1
            && self.map.in_bounds(x, y)
            && self.rubble.is_none_or(|rubble| {
                [from.z + 1, from.z + 2]
                    .into_iter()
                    .all(|z| !rubble.contains(&CaveTile { x, y, z }))
            })
            && self.material(CaveTile {
                x,
                y,
                z: from.z - 1,
            }) == Material::Rock
            && [from.z, from.z + 1]
                .into_iter()
                .any(|z| self.material(CaveTile { x, y, z }) == Material::Rock)
            && self.adjacent_floor(from, x, y).is_none()
    }

    /// The first matching floor in walking distance. Stable neighbor order also breaks route ties.
    pub(crate) fn route_to(
        &mut self,
        start: CaveTile,
        mut goal: impl FnMut(CaveTile) -> bool,
        mut admits: impl FnMut(CaveTile, CaveTile) -> bool,
    ) -> Option<Vec<CaveTile>> {
        let mut parents = HashMap::from([(start, start)]);
        let mut queue = VecDeque::from([start]);
        while let Some(tile) = queue.pop_front() {
            if goal(tile) {
                let mut current = tile;
                let mut path = Vec::new();
                while current != start {
                    path.push(current);
                    current = parents[&current];
                }
                return Some(path);
            }
            for (dx, dy) in SIDES {
                if let Some(next) = self.next_floor(tile, tile.x + dx, tile.y + dy) {
                    // Keep checking floors already queued when the search budget is exhausted.
                    if parents.len() >= 8_192 {
                        break;
                    }
                    if let std::collections::hash_map::Entry::Vacant(entry) = parents.entry(next)
                        && admits(tile, next)
                    {
                        entry.insert(tile);
                        queue.push_back(next);
                    }
                }
            }
        }
        None
    }

    pub(crate) fn route(
        &mut self,
        start: CaveTile,
        goal: (i32, i32),
        height: Option<i16>,
    ) -> Option<Vec<CaveTile>> {
        self.route_to(
            start,
            |tile| tile.xy() == goal && height.is_none_or(|z| z == tile.z),
            |_, _| true,
        )
    }

    pub(super) fn has_shallow_gallery(&mut self, mouth: CaveMouth) -> bool {
        let start = mouth.landing;
        let distance = |tile: CaveTile| tile.x.abs_diff(start.x).max(tile.y.abs_diff(start.y));
        self.next_floor(start, mouth.threshold.x, mouth.threshold.y) == Some(mouth.threshold)
            && self
                .route_to(
                    start,
                    |tile| distance(tile) >= 5,
                    |_, to| crate::caves::CaveSelf::natural_depth_at(to.z) == 0,
                )
                .is_some()
    }

    pub(crate) fn nearby_floors(&mut self, start: CaveTile, radius: i32) -> Vec<CaveTile> {
        // Restrict the cutaway to the nearest traversable floor in a column. Other floors are drawn after descent.
        let diameter = 2 * radius + 1;
        let mut queue = VecDeque::from([start]);
        let mut seen = vec![false; (diameter * diameter) as usize];
        let index = |x, y| ((y - start.y + radius) * diameter + x - start.x + radius) as usize;
        seen[index(start.x, start.y)] = true;
        let mut tiles = Vec::new();
        while let Some(tile) = queue.pop_front() {
            tiles.push(tile);
            for (dx, dy) in SIDES {
                let (x, y) = (tile.x + dx, tile.y + dy);
                if (x - start.x).abs().max((y - start.y).abs()) > radius || seen[index(x, y)] {
                    continue;
                }
                if let Some(next) = self.next_floor(tile, x, y) {
                    seen[index(x, y)] = true;
                    queue.push_back(next);
                }
            }
        }
        tiles
    }
}

pub(crate) fn cave_mouth(map: &Map, tx: i32, ty: i32) -> Option<CaveMouth> {
    // A small lattice keeps world generation cheap. The opening itself must come from the density field.
    if !mouth_grid(tx, ty) {
        return None;
    }
    find_mouth(map, map.cave_field(), tx, ty)
}

pub(super) fn mouth_grid(x: i32, y: i32) -> bool {
    x.rem_euclid(4) == 2 && y.rem_euclid(4) == 2
}

pub(super) fn find_mouth(map: &Map, field: CaveTerrain, tx: i32, ty: i32) -> Option<CaveMouth> {
    if !dry(map, tx, ty) {
        return None;
    }
    let at_surface = |x, y| {
        let (x_mm, y_mm) = CaveTile { x, y, z: 0 }.center_mm();
        let surface = i64::from(crate::map::elevation_at_mm(map.seed, x_mm, y_mm));
        let column = field.column(x_mm, y_mm);
        (surface, column.sample(surface - 350).material())
    };
    let (approach, support) = at_surface(tx, ty);
    if support != Material::Rock {
        return None;
    }
    let dug = BTreeSet::new();
    let mut nav = CaveNav::with_field(map, &dug, field);
    for (dx, dy) in SIDES {
        let (x, y) = (tx + dx, ty + dy);
        if !dry(map, x, y) {
            continue;
        }
        let (rim, material) = at_surface(x, y);
        if material != Material::CaveAir || (rim - approach).abs() > 2_000 {
            continue;
        }
        let top = rim.div_euclid(VOXEL_H_MM) as i16;
        for z in (top - 3..=top).rev() {
            let threshold = CaveTile { x, y, z };
            let drop = approach - i64::from(z) * VOXEL_H_MM;
            if (-1_000..=2_000).contains(&drop) && nav.floor(threshold)
                && let Some(landing) = nav.next_floor(threshold, x + dx, y + dy) {
                    return Some(CaveMouth {
                        landing,
                        threshold,
                        direction: (dx, dy),
                    });
                }
        }
    }
    None
}

fn dry(map: &Map, x: i32, y: i32) -> bool {
    map.terrain(x, y)
        .is_some_and(|t| !matches!(t, Terrain::Water))
}
