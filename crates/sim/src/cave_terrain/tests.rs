use super::{BEDROCK_Z_MM, CaveTerrain, DensityField, DensitySample, Material, WorldPoint};
use crate::map::{CHUNK, TILE_MM, elevation_at_mm, tag_seed_v2, tag_seed_v3};

#[test]
fn new_small_worlds_have_an_accessible_cave_gallery() {
    use super::CaveNav;
    use crate::map::{NodeKind, tag_seed_new_world};
    use crate::{Command, CommandKind, ItemKind, World};
    use std::collections::BTreeSet;

    for size in [128, 256] {
        for seed in (0..16).chain([40]) {
            let mut world = World::with_size(tag_seed_new_world(seed, size), size, size);
            let map = &world.map;
            let (sx, sy) = world.spawn_mm;
            assert!(
                map.cave_accessible_spawn((sx / TILE_MM) as i32, (sy / TILE_MM) as i32),
                "seed {seed}, size {size}: spawn is disconnected from the cave at ({sx}, {sy})"
            );
            let dug = BTreeSet::new();
            let mut nav = CaveNav::new(map, &dug);
            let (tx, ty) = map
                .cave_spawn_fallback()
                .unwrap_or_else(|| panic!("seed {seed}, size {size}: no accessible gallery"));
            let mouth = map.natural_cave_mouth(tx, ty).unwrap();
            let node = world.node_at(tx, ty).unwrap();
            assert_eq!(world.nodes[node.0 as usize].kind, NodeKind::CaveMouth);
            assert!(nav.has_shallow_gallery(mouth));
            assert!(!map.surface_cave_gap(tx, ty));
            assert!(map.surface_cave_gap(mouth.threshold.x, mouth.threshold.y));
            assert_eq!(
                nav.next_floor(mouth.landing, mouth.threshold.x, mouth.threshold.y),
                Some(mouth.threshold)
            );
            if [(128, 4), (256, 13)].contains(&(size, seed)) {
                let agent = world.spawn_agent(sx, sy);
                world.settings.difficulty = crate::world::Difficulty::Creative;
                world.wild.clear();
                world.agents[agent.0 as usize].inventory.counts[ItemKind::Torch as usize] = 1;
                world.step(&[Command {
                    agent,
                    kind: CommandKind::EnterCave { node },
                }]);
                for _ in 0..20_000 {
                    let a = &world.agents[agent.0 as usize];
                    if a.cave.is_some() || a.task == crate::world::Task::Idle {
                        break;
                    }
                    world.step(&[]);
                }
                let cave = world.agents[agent.0 as usize].cave.unwrap_or_else(|| {
                    let a = &world.agents[agent.0 as usize];
                    panic!("seed {seed}, size {size}: walking from ({sx}, {sy}) to ({tx}, {ty}) ended at ({}, {}) on tick {} with {:?}: {:?}", a.x, a.y, world.tick, a.task, world.events)
                });
                assert_eq!(cave.natural_tile(), Some(mouth.landing));
                let loaded = World::from_bytes(&world.to_bytes()).unwrap();
                assert_eq!(world.hash(), loaded.hash());
                assert_eq!(loaded.map.natural_cave_mouth(tx, ty), Some(mouth));
            }
        }
    }
}

#[test]
fn walking_onto_a_cave_lip_goes_in_without_a_command_or_a_light() {
    use super::CaveTile;
    use crate::map::tag_seed_new_world;
    use crate::{Command, CommandKind, World};

    let mut world = World::with_size(tag_seed_new_world(4, 128), 128, 128);
    world.wild.clear();
    let (tx, ty) = world.map.cave_spawn_fallback().expect("a gallery");
    let mouth = world.map.natural_cave_mouth(tx, ty).unwrap();
    let (ax, ay) = CaveTile { x: tx, y: ty, z: 0 }.center_mm();
    let agent = world.spawn_agent(ax, ay);
    let (gx, gy) = mouth.threshold.center_mm();
    world.step(&[Command { agent, kind: CommandKind::MoveTo { x: gx, y: gy } }]);
    for _ in 0..2_000 {
        if world.agents[agent.0 as usize].cave.is_some() {
            break;
        }
        world.step(&[]);
    }
    let cave = world.agents[agent.0 as usize].cave.expect("the walker went in");
    assert_eq!(cave.natural_tile(), Some(mouth.landing));
    assert_eq!(cave.light, 0);
    assert!(world.natural_cave_sight(cave).keys().all(|t| (t.x - mouth.landing.x).abs().max((t.y - mouth.landing.y).abs()) <= 2));

    // and walking back out to the lip leaves the cave again, in front of the mouth
    world.step(&[Command { agent, kind: CommandKind::MoveTo { x: gx, y: gy } }]);
    for _ in 0..2_000 {
        if world.agents[agent.0 as usize].cave.is_none() {
            break;
        }
        world.step(&[]);
    }
    let a = &world.agents[agent.0 as usize];
    assert!(a.cave.is_none(), "still underground at {:?} with {:?}", a.cave, a.task);
    assert_eq!((a.x, a.y), (ax, ay));
}

#[test]
fn natural_entrances_follow_the_slope_and_rock_roof() {
    use super::{CaveEntrance, CaveNav};
    use crate::map::{Map, tag_seed_natural};
    use std::collections::BTreeSet;

    for (plain_seed, anchor, hillside) in [(4, (38, 230), true), (42, (222, 150), false)] {
        let seed = tag_seed_natural(plain_seed, 256);
        let map = Map::generate(seed, 256, 256);
        let mouth = map.natural_cave_mouth(anchor.0, anchor.1).unwrap();
        let dug = BTreeSet::new();
        let mut nav = CaveNav::new(&map, &dug);
        assert!(map.surface_cave_gap(mouth.threshold.x, mouth.threshold.y));
        assert_eq!(
            nav.next_floor(mouth.threshold, mouth.landing.x, mouth.landing.y),
            Some(mouth.landing)
        );
        match (hillside, mouth.entrance(&map)) {
            (true, CaveEntrance::Hillside { roof_mm }) => {
                let (x, y) = mouth.landing.center_mm();
                let column = map.cave_field().column(x, y);
                let clearance = roof_mm - i64::from(mouth.landing.z.max(mouth.threshold.z)) * 1_000;
                assert!(clearance >= 2_000);
                assert_eq!(column.sample(roof_mm + 250).material(), Material::Rock);
            }
            (false, CaveEntrance::Descent) => assert!(mouth.landing.z < mouth.threshold.z),
            (_, profile) => panic!("unexpected entrance at seed {plain_seed}: {profile:?}"),
        }
    }
}

#[test]
fn natural_mouths_lead_into_the_same_walkable_network() {
    use super::{CaveNav, cave_mouth};
    use crate::map::{Map, NodeKind, tag_seed_natural};
    use std::collections::BTreeSet;

    let seed = tag_seed_natural(42, 256);
    let map = Map::generate(seed, 256, 256);
    let mouths: Vec<_> = (2..256)
        .step_by(4)
        .flat_map(|y| (2..256).step_by(4).map(move |x| (x, y)))
        .filter_map(|(x, y)| cave_mouth(&map, x, y).map(|m| ((x, y), m)))
        .collect();
    println!("{} natural entrances in seed 42", mouths.len());
    let dug = BTreeSet::new();
    let mut nav = CaveNav::new(&map, &dug);
    for &((x, y), mouth) in &mouths {
        assert_eq!(map.node_for(seed, x, y), Some(NodeKind::CaveMouth));
        assert!(nav.floor(mouth.landing));
        for &((xx, yy), target) in &mouths {
            if (xx - x).abs().max((yy - y).abs()) < 12 {
                continue;
            }
            if let Some(route) =
                nav.route(mouth.landing, target.landing.xy(), Some(target.landing.z))
            {
                println!(
                    "connected mouths at ({x}, {y}) and ({xx}, {yy}), {} steps",
                    route.len()
                );
                return;
            }
        }
    }
    panic!("expected two separated entrances on the same walkable network");
}

#[test]
fn natural_caves_share_digging_exits_and_saved_state() {
    use crate::map::tag_seed_natural;
    use crate::{Command, CommandKind, ItemKind, NodeId, World};

    let mut world = World::with_size(tag_seed_natural(42, 256), 256, 256);
    let start = world.node_at(222, 150).expect("the first mouth");
    let exit = world.node_at(218, 166).expect("the other mouth");
    let exit_tile = world.natural_mouth(exit.0).unwrap().threshold;
    assert!(world.map.surface_cave_gap(exit_tile.x, exit_tile.y));
    let enter = |w: &mut World, node: NodeId| {
        let (x, y) = w.nodes[node.0 as usize].center_mm();
        let id = w.spawn_agent(x, y);
        let a = &mut w.agents[id.0 as usize];
        a.inventory.counts[ItemKind::Torch as usize] = 2;
        a.inventory.counts[ItemKind::IronPickaxe as usize] = 1;
        w.step(&[Command {
            agent: id,
            kind: CommandKind::EnterCave { node },
        }]);
        id
    };
    let first = enter(&mut world, start);
    for _ in 0..95 {
        world.step(&[]);
    }
    let first_spot = world.agents[first.0 as usize]
        .cave
        .expect("arriving leaves space inside the cave");
    let (tx, ty) = world
        .cave_dig_target(first)
        .expect("a wall beside the landing");
    world.step(&[Command {
        agent: first,
        kind: CommandKind::CaveDig { tx, ty },
    }]);
    for _ in 0..180 {
        world.step(&[]);
    }
    let carved = super::CaveTile {
        x: tx,
        y: ty,
        z: first_spot.elevation_m().unwrap(),
    };
    assert!(world.natural_cave_revision() > 0);
    assert_eq!(
        world.natural_cave_cell(first_spot, tx, ty),
        Some(crate::caves::Cell::Floor)
    );
    let bytes = world.to_bytes();
    let mut loaded = World::from_bytes(&bytes).expect("natural cave save should load");
    assert_eq!(world.hash(), loaded.hash());
    assert_eq!(
        loaded.agents[first.0 as usize].cave,
        world.agents[first.0 as usize].cave
    );
    let second = enter(&mut loaded, exit);
    let second_spot = loaded.agents[second.0 as usize]
        .cave
        .expect("enter from the other mouth");
    assert!(loaded.natural_cave_tiles(second_spot, 24).contains(&carved));

    let (x, y) = exit_tile.center_mm();
    loaded.step(&[Command {
        agent: first,
        kind: CommandKind::MoveTo { x, y },
    }]);
    for _ in 0..1400 {
        if loaded.agents[first.0 as usize].cave.is_none() {
            break;
        }
        loaded.step(&[]);
    }
    let agent = &loaded.agents[first.0 as usize];
    assert!(
        agent.cave.is_none(),
        "walked to the second exit: {:?} {:?}",
        agent.cave,
        agent.task
    );
    assert_eq!(
        (agent.x, agent.y),
        loaded.nodes[exit.0 as usize].center_mm()
    );
}
const SIDE: usize = 129;
const HEIGHT: usize = 63;
const STEP: i64 = 2 * TILE_MM;
const PLANE: usize = SIDE * SIDE;
const LOWEST_SAMPLE_MM: i64 = -92_000;
const SKY: [u8; 3] = [222, 235, 241];
const ROCK: [u8; 3] = [47, 53, 61];
const CAVE: [u8; 3] = [243, 179, 95];

#[test]
fn material_distinguishes_surface_rock_and_cave() {
    assert_eq!(
        DensitySample {
            ground: 0,
            cave: -1
        }
        .material(),
        Material::OpenAir
    );
    assert_eq!(
        DensitySample { ground: 1, cave: 1 }.material(),
        Material::Rock
    );
    assert_eq!(
        DensitySample { ground: 1, cave: 0 }.material(),
        Material::CaveAir
    );
}

#[test]
fn cave_samples_are_shared_between_chunks_and_repeatable() {
    let field = CaveTerrain::new(tag_seed_v3(7));
    let seam_x = i64::from(CHUNK) * TILE_MM;
    let sample = |x_mm, y_mm, z_mm| field.sample(WorldPoint { x_mm, y_mm, z_mm });
    let mut left = Vec::new();
    for y in -4..=4 {
        for z in [-65, -45, -25, -5] {
            left.push(sample(seam_x, y * TILE_MM, z * TILE_MM));
        }
    }
    let mut right = Vec::new();
    for y in (-4..=4).rev() {
        let column = field.column(seam_x, y * TILE_MM);
        for z in [-5, -25, -45, -65] {
            right.push(column.sample(z * TILE_MM));
        }
    }
    right.reverse();
    assert_eq!(left, right);
    assert!((-24..=24).any(|x| {
        let point = WorldPoint {
            x_mm: x * TILE_MM,
            y_mm: -12 * TILE_MM,
            z_mm: -20 * TILE_MM,
        };
        CaveTerrain::new(tag_seed_v3(8)).sample(point).cave != field.sample(point).cave
    }));
}

#[test]
fn cave_volume_meets_the_existing_surface_and_has_a_solid_base() {
    let seed = tag_seed_v2(42);
    let field = CaveTerrain::new(seed);
    let mut openings = 0;
    let mut solid = 0;
    for x in (0..256).step_by(2) {
        for y in (0..256).step_by(2) {
            let (x_mm, y_mm) = (x * TILE_MM, y * TILE_MM);
            let column = field.column(x_mm, y_mm);
            let surface = i64::from(elevation_at_mm(seed, x_mm, y_mm));
            let below = column.sample(surface - 250);
            assert_eq!(below.ground, 250);
            assert_eq!(column.sample(surface).material(), Material::OpenAir);
            assert_eq!(column.sample(BEDROCK_Z_MM).material(), Material::Rock);
            match below.material() {
                Material::CaveAir => openings += 1,
                Material::Rock => solid += 1,
                Material::OpenAir => unreachable!("sample is below the surface"),
            }
        }
    }
    assert!(
        openings > 0 && openings * 8 < solid,
        "surface openings: {openings}, solid: {solid}"
    );
    println!(
        "seed 42: {openings}/{} surface samples meet cave air",
        openings + solid
    );
}

fn volume_index(x: usize, y: usize, z: usize) -> usize {
    z * PLANE + y * SIDE + x
}

fn sample_volume(field: CaveTerrain) -> Vec<bool> {
    let mut air = vec![false; PLANE * HEIGHT];
    for y in 0..SIDE {
        for x in 0..SIDE {
            let column = field.column(x as i64 * STEP, y as i64 * STEP);
            for z in 0..HEIGHT {
                air[volume_index(x, y, z)] =
                    column.sample(LOWEST_SAMPLE_MM + z as i64 * STEP).material()
                        == Material::CaveAir;
            }
        }
    }
    air
}

fn largest_cave(mut air: Vec<bool>) -> Vec<usize> {
    let mut largest = Vec::new();
    let mut component = Vec::new();
    for start in 0..air.len() {
        if !air[start] {
            continue;
        }
        air[start] = false;
        component.push(start);
        let mut next = 0;
        while let Some(&i) = component.get(next) {
            next += 1;
            let (x, y, z) = (i % SIDE, i / SIDE % SIDE, i / PLANE);
            for (nx, ny, nz) in [
                (x.wrapping_sub(1), y, z),
                (x + 1, y, z),
                (x, y.wrapping_sub(1), z),
                (x, y + 1, z),
                (x, y, z.wrapping_sub(1)),
                (x, y, z + 1),
            ] {
                if nx < SIDE && ny < SIDE && nz < HEIGHT {
                    let j = volume_index(nx, ny, nz);
                    if air[j] {
                        air[j] = false;
                        component.push(j);
                    }
                }
            }
        }
        if component.len() > largest.len() {
            std::mem::swap(&mut largest, &mut component);
        }
        component.clear();
    }
    largest
}

#[test]
fn cave_networks_cross_chunk_boundaries() {
    for seed in [tag_seed_v2(7), tag_seed_v2(42), tag_seed_v3(2026)] {
        let air = sample_volume(CaveTerrain::new(seed));
        let total = air.iter().filter(|&&open| open).count();
        let largest = largest_cave(air);
        let lengths = [SIDE, SIDE, HEIGHT];
        let mut low = lengths;
        let mut high = [0; 3];
        for &i in &largest {
            let point = [i % SIDE, i / SIDE % SIDE, i / PLANE];
            for axis in 0..3 {
                low[axis] = low[axis].min(point[axis]);
                high[axis] = high[axis].max(point[axis]);
            }
        }
        let span = |axis: usize| (high[axis].saturating_sub(low[axis])) as i64 * STEP / TILE_MM;
        println!(
            "seed {}: {}/{} connected samples, span {} × {} × {} m",
            seed & 0xFFFF_FFFF_FFFF,
            largest.len(),
            total,
            span(0),
            span(1),
            span(2)
        );
        assert!(largest.len() > total / 4);
        assert!(span(0).max(span(1)) > i64::from(CHUNK) && span(2) > 12);
    }
}

/// Dev aid, like the surface map dump. Set `ALAZ_CAVE_SLICE_DIR` to an existing directory.
#[test]
#[ignore = "writes seeded cave previews"]
fn dump_cave_slices() {
    let Some(dir) = std::env::var_os("ALAZ_CAVE_SLICE_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let parameter = |name, default| {
        std::env::var(name)
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(default)
    };
    let seed = std::env::var("ALAZ_CAVE_SEED")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(42);
    let tagged = if parameter("ALAZ_CAVE_GEN", 2) == 3 {
        tag_seed_v3(seed)
    } else {
        tag_seed_v2(seed)
    };
    let field = CaveTerrain::new(tagged);
    let xy_z = parameter("ALAZ_CAVE_XY_Z", -30) * TILE_MM;
    let xz_y = parameter("ALAZ_CAVE_XZ_Y", 128) * TILE_MM;
    let width = 257;
    for (name, height) in [("xy.ppm", 257), ("xz.ppm", 126)] {
        let horizontal = name == "xy.ppm";
        let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
        for row in 0..height {
            for x in 0..width {
                let (y, z) = if horizontal {
                    ((height - 1 - row) * TILE_MM, xy_z)
                } else {
                    (xz_y, (32 - row) * TILE_MM)
                };
                let color = match field
                    .sample(WorldPoint {
                        x_mm: x * TILE_MM,
                        y_mm: y,
                        z_mm: z,
                    })
                    .material()
                {
                    Material::OpenAir => SKY,
                    Material::Rock => ROCK,
                    Material::CaveAir => CAVE,
                };
                ppm.extend_from_slice(&color);
            }
        }
        std::fs::write(dir.join(name), ppm).expect("write cave slice");
    }
    let mut projected = vec![false; PLANE];
    for i in largest_cave(sample_volume(field)) {
        projected[i % PLANE] = true;
    }
    let mut ppm = format!("P6\n{SIDE} {SIDE}\n255\n").into_bytes();
    for row in projected.as_chunks::<SIDE>().0.iter().rev() {
        for &open in row {
            ppm.extend_from_slice(if open { &CAVE } else { &ROCK });
        }
    }
    std::fs::write(dir.join("network.ppm"), ppm).expect("write connected cave projection");
}
