use super::navigation::{find_mouth, mouth_grid};
use super::{CaveMouth, CaveNav, CaveTerrain, SIDES};
use crate::map::{Map, Terrain};
use std::collections::{BTreeMap, BTreeSet};

type Tile = (i32, i32);
const FIELD_ATTEMPTS: u64 = 16;
const THIN_ROOF_ATTEMPTS: u64 = 8;

#[derive(Clone, Debug)]
pub(crate) struct SmallWorldCaves {
    pub(crate) field: CaveTerrain,
    pub(crate) mouths: BTreeMap<Tile, CaveMouth>,
    pub(crate) anchor: Tile,
    approach_land: Vec<bool>,
}

impl SmallWorldCaves {
    pub(crate) fn generate(map: &Map) -> Option<Self> {
        let mut land = largest_land(map);
        if land.is_empty() {
            return None;
        }
        land.sort_unstable_by_key(|&(x, y)| (x.abs_diff(map.w / 2) + y.abs_diff(map.h / 2), y, x));

        let dug = BTreeSet::new();
        for attempt in 0..FIELD_ATTEMPTS + THIN_ROOF_ATTEMPTS {
            // Keep the surface fixed. Retry the underground noise only when the field has no usable intersection.
            let noise_seed = map.seed ^ attempt.wrapping_mul(0x9E37_79B9_7F4A_7C15);
            let field =
                CaveTerrain::with_noise_seed(map.seed, noise_seed, attempt >= FIELD_ATTEMPTS);
            let mut nav = CaveNav::with_field(map, &dug, field);
            let mut mouths: BTreeMap<_, _> = (2..map.h)
                .step_by(4)
                .flat_map(|y| {
                    (2..map.w).step_by(4).filter_map(move |x| {
                        find_mouth(map, field, x, y).map(|mouth| ((x, y), mouth))
                    })
                })
                .collect();
            let mut gaps: BTreeSet<_> = mouths.values().map(|mouth| mouth.threshold.xy()).collect();
            let anchor = land
                .iter()
                .copied()
                .find(|tile| {
                    mouths
                        .get(tile)
                        .is_some_and(|&mouth| nav.has_shallow_gallery(mouth))
                })
                .or_else(|| {
                    for &(x, y) in land.iter().filter(|&&(x, y)| !mouth_grid(x, y)) {
                        if !gaps.contains(&(x, y))
                            && let Some(mouth) = find_mouth(map, field, x, y)
                            && !mouths.contains_key(&mouth.threshold.xy())
                            && !gaps.contains(&mouth.threshold.xy())
                            && nav.has_shallow_gallery(mouth)
                        {
                            mouths.insert((x, y), mouth);
                            gaps.insert(mouth.threshold.xy());
                            return Some((x, y));
                        }
                    }
                    None
                });
            if let Some(anchor) = anchor {
                let mut approach_land = vec![false; map.tiles.len()];
                visit_land(map, anchor, &gaps, &mut approach_land);
                return Some(Self {
                    field,
                    mouths,
                    anchor,
                    approach_land,
                });
            }
        }
        None
    }

    pub(crate) fn connected_to_entrance(&self, map: &Map, x: i32, y: i32) -> bool {
        map.in_bounds(x, y) && self.approach_land[index(map, (x, y))]
    }
}

fn index(map: &Map, (x, y): Tile) -> usize {
    (y * map.w + x) as usize
}

fn largest_land(map: &Map) -> Vec<Tile> {
    let no_gaps = BTreeSet::new();
    let mut seen = vec![false; map.tiles.len()];
    let mut largest = Vec::new();
    for y in 0..map.h {
        for x in 0..map.w {
            let tile = (x, y);
            if seen[index(map, tile)] || map.terrain(x, y) == Some(Terrain::Water) {
                continue;
            }
            let group = visit_land(map, tile, &no_gaps, &mut seen);
            if group.len() > largest.len() {
                largest = group;
            }
        }
    }
    largest
}

fn visit_land(map: &Map, start: Tile, gaps: &BTreeSet<Tile>, seen: &mut [bool]) -> Vec<Tile> {
    let mut connected = Vec::from([start]);
    seen[index(map, start)] = true;
    let mut i = 0;
    while let Some(&(x, y)) = connected.get(i) {
        i += 1;
        for (dx, dy) in SIDES {
            let next = (x + dx, y + dy);
            if map
                .terrain(next.0, next.1)
                .is_some_and(|t| t != Terrain::Water)
                && !gaps.contains(&next)
            {
                let visited = &mut seen[index(map, next)];
                if !*visited {
                    *visited = true;
                    connected.push(next);
                }
            }
        }
    }
    connected
}
