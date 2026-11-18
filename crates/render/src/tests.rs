use crate::mesh::*;
use crate::renderer::*;
use crate::sprites;
use crate::*;
use alaz_sim::{AgentKind, BuildingKind, NodeKind, World};

#[cfg(test)]
mod vertex_census {
    use super::*;

    /// How many vertices each kind of thing costs around the spawn (`cargo test -p alaz-render census -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn census() {
        let mut world = World::with_size(alaz_sim::map::tag_seed_v2(7), 4096, 4096);
        let (sx, sy) = world.spawn_mm;
        world.spawn_agent(sx, sy);
        let (cx, cy) = (sx / 1000 / CHUNK_TILES as i64, sy / 1000 / CHUNK_TILES as i64);
        let (x0, y0, x1, y1) = ((cx as i32 - 2) * CHUNK_TILES, (cy as i32 - 2) * CHUNK_TILES, (cx as i32 + 3) * CHUNK_TILES, (cy as i32 + 3) * CHUNK_TILES);
        let (art, _) = sprites::build();
        let mut terrain = Mesh::new();
        build_terrain(&mut terrain, &art, &world, (x0, y0, x1, y1));
        let mut decor: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
        for ty in y0..y1 {
            for tx in x0..x1 {
                let mut v = Mesh::fixed();
                build_decor(&mut v, &world, (tx, ty, tx + 1, ty + 1));
                if !v.is_empty() {
                    let e = decor.entry(format!("decor {:?}", world.map.terrain(tx, ty).unwrap())).or_default();
                    e.0 += 1;
                    e.1 += v.len();
                }
            }
        }
        let mut nodes: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
        for n in world.nodes_in(x0, y0, x1, y1) {
            let mut v = Mesh::fixed();
            push_node(&mut v, &world, n, 0);
            let e = nodes.entry(format!("node {:?}", n.kind)).or_default();
            e.0 += 1;
            e.1 += v.len();
        }
        let tiles = ((x1 - x0) * (y1 - y0)) as usize;
        println!("5x5 chunks = {tiles} tiles; terrain verts {}", terrain.len());
        let mut total = terrain.len();
        for (k, (n, v)) in decor.iter().chain(nodes.iter()) {
            println!("{k:28} count {n:6}  verts {v:8}  per item {:6.0}", *v as f32 / *n as f32);
            total += v;
        }
        println!("total {total} ({:.0} per chunk)", total as f32 / 25.0);
    }

    /// Vertices of one standing person by stage and outfit (`cargo test --release -p alaz-render human_verts -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn human_verts() {
        for outfit in [0u16, 0xFFFF] {
            let row: Vec<usize> = (0..10u8)
                .map(|stage| {
                    let mut o = Mesh::new();
                    push_human(&mut o, 5.0, 9.0, Pose { outfit, evo: stage as f32, id: 5, ..Pose::idle(1.0) });
                    o.len()
                })
                .collect();
            println!("outfit {outfit:#06x}: verts by stage 0..9 = {row:?}");
        }
    }
}

#[cfg(test)]
mod mesh_hashes {
    use super::*;

    type Out = Mesh;

    /// A freshly made mesh with the test's wind.
    fn fresh(rooted: bool) -> Out {
        Mesh { rooted, wind: [0.6, -0.3, 0.7], ..Mesh::new() }
    }

    fn fnv(v: &[Vertex]) -> u64 {
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        for b in bytemuck::cast_slice::<Vertex, u8>(v) {
            h = (h ^ *b as u64).wrapping_mul(0x0000_0100_0000_01b3);
        }
        h ^ (v.len() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
    }

    fn pose(i: u32, work: Work, tool: Tool) -> Pose {
        let f = i as f32;
        Pose { heading: f * 0.7, phase: f * 1.3, amp: 0.2 + 0.1 * f, t: 3.0 + f, work, tool, outfit: ((i * 53) % 256) as u16, id: i * 5 + 1, evo: (i % 10) as f32 * 0.9 + 0.3, slope: 0.1 * f - 0.3, hurt: if i % 3 == 0 { 0.5 } else { 0.0 }, stride: 0.3 + 0.05 * f, cyc: 2.7 + 0.01 * f, air: [0.0, 0.6, -0.4][(i % 3) as usize], lod: false, sit: false }
    }

    fn cases() -> Vec<(String, u64)> {
        let mut world = World::with_size(alaz_sim::map::tag_seed_v2(7), 4096, 4096);
        let (sx, sy) = world.spawn_mm;
        for k in 0..4 {
            world.spawn_agent(sx + k * 1500, sy + k * 700);
        }
        let (cx, cy) = ((sx / 1000) as i32 / CHUNK_TILES, (sy / 1000) as i32 / CHUNK_TILES);
        let (art, _) = sprites::build();
        {
            use alaz_sim::physics::{Body, BodyKind, Q};
            let tree_tile = world.nodes.iter().find(|n| n.kind == NodeKind::Tree).map(|n| (n.tx, n.ty)).unwrap_or((0, 0));
            for (i, kind) in [BodyKind::Tree, BodyKind::Tree, BodyKind::Rock, BodyKind::Piece, BodyKind::Piece].into_iter().enumerate() {
                let i = i as i64;
                world.bodies.push(Body { id: i as u32, kind, tx: tree_tile.0, ty: tree_tile.1, x: (sx + 800 * i) * Q, y: (sy + 500) * Q, z: 0, vx: 3 * Q, vy: Q, vz: 0, dx: 800, dy: 600, angle: 300_000 * i, spin: 1000, by: alaz_sim::command::AgentId(u32::MAX), size: 4000 - 500 * i as i32, age: 10, rest: 0, wood: 3, full: 5, item: (i * 3) as u8 });
            }
        }
        let mut r: Vec<(String, u64)> = Vec::new();
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (-1, -1)] {
            let (x0, y0) = ((cx + dx) * CHUNK_TILES, (cy + dy) * CHUNK_TILES);
            let b = (x0, y0, x0 + CHUNK_TILES, y0 + CHUNK_TILES);
            let mut o = fresh(false);
            build_terrain(&mut o, &art, &world, b);
            r.push((format!("terrain {dx},{dy}"), fnv(&o)));
            let mut o = fresh(true);
            build_decor(&mut o, &world, b);
            pre_lift(&world, &mut o);
            r.push((format!("decor {dx},{dy}"), fnv(&o)));
            for level in [0u8, 3, 5] {
                let mut o = fresh(true);
                build_nodes(&mut o, &world, b, level);
                pre_lift(&world, &mut o);
                r.push((format!("nodes {dx},{dy} L{level}"), fnv(&o)));
            }
        }
        for kind in BuildingKind::ALL {
            for (active, tint) in [(false, None), (true, None), (true, Some([0.35, 1.0, 0.45]))] {
                let mut o = fresh(false);
                push_building(&mut o, kind, 12.5, 7.25, active, 1234, tint);
                turn_quarters(&mut o, 0, 12.5, 7.25, 1);
                r.push((format!("building {kind:?} {active} {}", tint.is_some()), fnv(&o)));
            }
        }
        for fuel in [0.0, 0.4, 1.0] {
            let mut o = fresh(false);
            push_campfire(&mut o, 3.5, 4.5, fuel, 17.3, None);
            r.push((format!("campfire {fuel}"), fnv(&o)));
        }
        let works = [Work::None, Work::Chop, Work::Mine, Work::Pick, Work::Dig, Work::Drink, Work::Hammer, Work::Throw, Work::Offer];
        let tools = [Tool::Hand, Tool::StoneAxe, Tool::MetalAxe, Tool::MetalPick, Tool::Spear];
        for i in 0..18u32 {
            let (w, t) = (works[i as usize % works.len()], tools[i as usize % tools.len()]);
            let mut o = fresh(false);
            push_human(&mut o, 5.0 + i as f32, 9.0, pose(i, w, t));
            r.push((format!("human {i}"), fnv(&o)));
            let mut o = fresh(false);
            push_chips(&mut o, if i % 2 == 0 { Work::Chop } else { Work::Mine }, (6.0, 9.5), (5.0, 9.0), 1.65 + 0.015 * i as f32, t);
            r.push((format!("chips {i}"), fnv(&o)));
        }
        for kind in [AgentKind::Human, AgentKind::Dog, AgentKind::Ox, AgentKind::Horse, AgentKind::Bot] {
            for (i, marked) in [(1u32, false), (2, true)] {
                let mut o = fresh(false);
                push_creature(&mut o, kind, 8.0, 8.0, marked, pose(i, Work::None, Tool::Hand));
                r.push((format!("creature {kind:?} {marked}"), fnv(&o)));
            }
        }
        for wolf in [false, true] {
            let mut o = fresh(false);
            push_tracks(&mut o, 7, 4.0, 4.0, (700, -300), wolf);
            r.push((format!("tracks {wolf}"), fnv(&o)));
        }
        let mut o = fresh(false);
        push_bodies(&mut o, &world, 0.5, &|_, _| true);
        r.push(("bodies".into(), fnv(&o)));
        let (fx, fz) = (sx as f32 / 1000.0, sy as f32 / 1000.0);
        let mut o = fresh(false);
        push_fish(&mut o, &world, (fx - 40.0, fz - 40.0, fx + 40.0, fz + 40.0), 12.0, 0.3);
        r.push(("fish".into(), fnv(&o)));
        let mut o = fresh(false);
        push_hover_rim(&mut o, &world, (0, 0), 3, &Camera::default(), 2.0);
        r.push(("rim-empty".into(), fnv(&o)));
        if let Some(n) = world.nodes.iter().find(|n| n.kind == NodeKind::Tree) {
            let mut o = fresh(false);
            push_hover_rim(&mut o, &world, (n.tx, n.ty), 3, &Camera::default(), 2.0);
            r.push(("rim-tree".into(), fnv(&o)));
        }
        // a cave: its blocks on every level (corner, middle) and the torch in three states
        let cave = alaz_sim::Cave::generate(world.seed, 40, 60);
        for level in 1..=4u8 {
            let lv = cave.level(level).unwrap();
            for (bx, by) in [(4, 3), (3, 4)] {
                r.push((format!("cave L{level} block {bx},{by}"), fnv(&build_cave_block(lv, world.seed, 5, level - 1, bx, by))));
            }
        }
        for (i, frac) in [0.0f32, 0.1, 0.9].into_iter().enumerate() {
            let mut o = fresh(false);
            push_torch(&mut o, 3.5, 4.5, 12.0 + i as f32, frac);
            r.push((format!("torch {frac}"), fnv(&o)));
        }
        r
    }

    /// Hashes of the vertex bytes of every model, taken before the builders lost their thread-local state.
    const EXPECTED: &[(&str, u64)] = &[
        ("terrain 0,0", 0x9c9551babaec5875),
        ("decor 0,0", 0xa620afd3f7ed5108),
        ("nodes 0,0 L0", 0x241a1a3a7bbfa473),
        ("nodes 0,0 L3", 0x3e44d76954f3710d),
        ("nodes 0,0 L5", 0x3e44d76954f3710d),
        ("terrain 1,0", 0xf4c4452c2230f795),
        ("decor 1,0", 0x342de8029224578f),
        ("nodes 1,0 L0", 0x88e0c11b5b663772),
        ("nodes 1,0 L3", 0xe968fee166d4c4cc),
        ("nodes 1,0 L5", 0xe968fee166d4c4cc),
        ("terrain 0,1", 0x63daca5654c464e5),
        ("decor 0,1", 0xb3afbe8ed1903b42),
        ("nodes 0,1 L0", 0xa86cdf6def8a9c93),
        ("nodes 0,1 L3", 0x8c2b448e6d67afe3),
        ("nodes 0,1 L5", 0x8c2b448e6d67afe3),
        ("terrain -1,-1", 0xe63140e8c7f056d1),
        ("decor -1,-1", 0x4a389595c2b36e1d),
        ("nodes -1,-1 L0", 0x954e09229d91dcf7),
        ("nodes -1,-1 L3", 0x47a03ef0e09f6eb3),
        ("nodes -1,-1 L5", 0x47a03ef0e09f6eb3),
        ("building Campfire false false", 0xeafd7aa814c9d824),
        ("building Campfire true false", 0xeafd7aa814c9d824),
        ("building Campfire true true", 0xcd4eb31101aa821e),
        ("building Shelter false false", 0xca32ae3edebbdbcb),
        ("building Shelter true false", 0xca32ae3edebbdbcb),
        ("building Shelter true true", 0xe49030bc09ca84aa),
        ("building CharcoalKiln false false", 0xb7a442ae5d18b3bd),
        ("building CharcoalKiln true false", 0x2b76ff54c1b2b079),
        ("building CharcoalKiln true true", 0xa3a9a4ab56873deb),
        ("building Furnace false false", 0xee53917d7734fa0a),
        ("building Furnace true false", 0xd55229039e53abce),
        ("building Furnace true true", 0x736480fa552fbbe5),
        ("building Depot false false", 0x19b7161c9aebeba6),
        ("building Depot true false", 0x19b7161c9aebeba6),
        ("building Depot true true", 0xa5fa4c64a55a3c9f),
        ("building Workshop false false", 0x9b107545242df94c),
        ("building Workshop true false", 0x97d4e7c8d594d054),
        ("building Workshop true true", 0xf62ff26dc2a36c85),
        ("building Workbench false false", 0x0ca1849eee729ffd),
        ("building Workbench true false", 0x9e98e03c0be22c05),
        ("building Workbench true true", 0xdbfe27ce7e3b7e85),
        ("building WaterWheel false false", 0xbb5732fbbf9dc2f7),
        ("building WaterWheel true false", 0x45c4993101182476),
        ("building WaterWheel true true", 0x3f8706df02682770),
        ("building Field false false", 0xf62bd65eb6fa510d),
        ("building Field true false", 0x568277e114482275),
        ("building Field true true", 0x7e08c17d7dc6e899),
        ("building Loom false false", 0x0baee6a1564bf2e0),
        ("building Loom true false", 0x240af1cf908ef290),
        ("building Loom true true", 0xa8884e49c172f194),
        ("building Pen false false", 0x36eed2e15be692b5),
        ("building Pen true false", 0x6a93bee7504577fd),
        ("building Pen true true", 0x45300d1e296031cd),
        ("building PitKiln false false", 0x9fa76a7cc065535a),
        ("building PitKiln true false", 0x3c71def7a69e73b6),
        ("building PitKiln true true", 0x8703c47a8aef5e80),
        ("building Anvil false false", 0x28eae4ae6553e6ba),
        ("building Anvil true false", 0x28eae4ae6553e6ba),
        ("building Anvil true true", 0x2818ead46e34082f),
        ("building HandMill false false", 0x76ca2c9ac84d2c25),
        ("building HandMill true false", 0xa1d7cd6431dca2a1),
        ("building HandMill true true", 0xf9bc641b7636eb12),
        ("building AnimalGin false false", 0x830f2ff4d0fc3fec),
        ("building AnimalGin true false", 0x830f2ff4d0fc3fec),
        ("building AnimalGin true true", 0xa97a573b1ae5b750),
        ("building Bloomery false false", 0x21d9fd29d75c4b98),
        ("building Bloomery true false", 0x36b524521a374120),
        ("building Bloomery true true", 0xf2f48191f9a4e9ba),
        ("building Bellows false false", 0x065cb87b5287609e),
        ("building Bellows true false", 0x065cb87b5287609e),
        ("building Bellows true true", 0x5ab195b1efca8bfb),
        ("building TripHammer false false", 0x311212c928f7eae3),
        ("building TripHammer true false", 0x6ef90d4ecc4013d7),
        ("building TripHammer true true", 0xd81d79c4e8cbe4f3),
        ("building IronAnvil false false", 0x7918df0e3f125f92),
        ("building IronAnvil true false", 0x7918df0e3f125f92),
        ("building IronAnvil true true", 0x04b5fe28835ae74b),
        ("building BlastFurnace false false", 0xf5e7c125ff1c8991),
        ("building BlastFurnace true false", 0xa6f03a6b33b276bd),
        ("building BlastFurnace true true", 0x64220313603a0101),
        ("building SteelAnvil false false", 0x01848c56ac24ce02),
        ("building SteelAnvil true false", 0x01848c56ac24ce02),
        ("building SteelAnvil true true", 0x5c1650c2360784b3),
        ("building Windmill false false", 0xdb70e8e9d90fe611),
        ("building Windmill true false", 0xdb70e8e9d90fe611),
        ("building Windmill true true", 0xe1f45de339694585),
        ("building Conveyor false false", 0x012f7cef8ed44bf1),
        ("building Conveyor true false", 0x012f7cef8ed44bf1),
        ("building Conveyor true true", 0x577890e95b855715),
        ("building SteamEngine false false", 0xccc7d170d9af7189),
        ("building SteamEngine true false", 0x1cf6eb074bbf041c),
        ("building SteamEngine true true", 0x47207620a943ee11),
        ("building Pump false false", 0xebb5a783d35335d5),
        ("building Pump true false", 0xb6bf5b9f3d910330),
        ("building Pump true true", 0xe91a36ed561cd690),
        ("building RollingMill false false", 0x3753febe276040de),
        ("building RollingMill true false", 0x3753febe276040de),
        ("building RollingMill true true", 0xc7bb276cc910cc29),
        ("building Dynamo false false", 0x5277f5289e06bea9),
        ("building Dynamo true false", 0x5277f5289e06bea9),
        ("building Dynamo true true", 0x7ab577552e505986),
        ("building Pole false false", 0x8c1fdd124e0a9f95),
        ("building Pole true false", 0x8c1fdd124e0a9f95),
        ("building Pole true true", 0xfa794bfc036ab9d9),
        ("building ElectricMotor false false", 0x5bddc1020e63bf8e),
        ("building ElectricMotor true false", 0x5bddc1020e63bf8e),
        ("building ElectricMotor true true", 0xb4b1c2b14adade95),
        ("building ElectricLamp false false", 0x4b6d8eef657b9288),
        ("building ElectricLamp true false", 0x4b6d8eef657b9288),
        ("building ElectricLamp true true", 0x8dd365e30f19f16f),
        ("building Computer false false", 0x6273bd20795dc455),
        ("building Computer true false", 0x6273bd20795dc455),
        ("building Computer true true", 0x4d07cf3a402e06c9),
        ("building Ruin false false", 0x1c591db647beb523),
        ("building Ruin true false", 0x1c591db647beb523),
        ("building Ruin true true", 0xe75ebeb1f8baac7d),
        ("building Cellar false false", 0xe648e795e6e149e0),
        ("building Cellar true false", 0xe648e795e6e149e0),
        ("building Cellar true true", 0x576c0385db920924),
        ("building MineShaft false false", 0xae625931fd89300b),
        ("building MineShaft true false", 0xae625931fd89300b),
        ("building MineShaft true true", 0x369914481bdd0834),
        ("building CokeOven false false", 0x1d9b3dd9df0aac41),
        ("building CokeOven true false", 0xd3ab6a3be6950758),
        ("building CokeOven true true", 0x858fb9841e5dca52),
        ("building LimeKiln false false", 0xb2a2be6809d6da93),
        ("building LimeKiln true false", 0x84fcc6a7eed2da51),
        ("building LimeKiln true true", 0x7aad36a74ab0e494),
        ("building CementKiln false false", 0x649ecd294428fda2),
        ("building CementKiln true false", 0x7f2eff9b964ac900),
        ("building CementKiln true true", 0xf378562ba49cd9d2),
        ("building BrickKiln false false", 0x8fe140613a1bf363),
        ("building BrickKiln true false", 0xd969b6ac2b52ee13),
        ("building BrickKiln true true", 0x4d51de8720c66e21),
        ("building PuddlingFurnace false false", 0xca61c27ddf747929),
        ("building PuddlingFurnace true false", 0xd590a616b0181939),
        ("building PuddlingFurnace true true", 0x38cb8748a107f806),
        ("building Bessemer false false", 0xa0593ace0394a19b),
        ("building Bessemer true false", 0x27f2ff441a095a2f),
        ("building Bessemer true true", 0x07e3b0a3fbc313b9),
        ("building Cupola false false", 0x2cc3d48046902dbe),
        ("building Cupola true false", 0xa85f44b7b5337e0a),
        ("building Cupola true true", 0xbf80b9138acc3922),
        ("building MoldingBench false false", 0x80ab9151949441ac),
        ("building MoldingBench true false", 0xa1ea300079e79a28),
        ("building MoldingBench true true", 0xaae4dc0a7a61c6e1),
        ("building Lathe false false", 0x4a4cf4ffa7019efc),
        ("building Lathe true false", 0x2a0122d345e92f35),
        ("building Lathe true true", 0x5d2640f72da01ba9),
        ("building DrillMill false false", 0xe3066a474d0ac09b),
        ("building DrillMill true false", 0x3ec119adbf1d467d),
        ("building DrillMill true true", 0xaa6cd50afad757dd),
        ("building Press false false", 0x92c7af9aced95fa6),
        ("building Press true false", 0xd909364d83e63901),
        ("building Press true true", 0x4095dde188cca2e5),
        ("building Sawmill false false", 0xb7e012411c4434ae),
        ("building Sawmill true false", 0xecdbcdb4d342f58b),
        ("building Sawmill true true", 0xeabac34ec9b08eed),
        ("building SpinningWheel false false", 0x51dcbb32e1d8232c),
        ("building SpinningWheel true false", 0x5b453b61b1d1a216),
        ("building SpinningWheel true true", 0xc0d9fc92b46a91ca),
        ("building PowerLoom false false", 0xd245217c8385e097),
        ("building PowerLoom true false", 0x1794b6c255ca5410),
        ("building PowerLoom true true", 0x48eb99a495c25474),
        ("building Boiler false false", 0xbef8e51364dc9941),
        ("building Boiler true false", 0x35ab7de9674be7f4),
        ("building Boiler true true", 0xa4af13cd864bcab4),
        ("building SteamHammer false false", 0x6044b98be61903f2),
        ("building SteamHammer true false", 0x4aa49b8eef5b4016),
        ("building SteamHammer true true", 0x47d6f30b620d0412),
        ("building RollingLine false false", 0x2b4d39b3ba430036),
        ("building RollingLine true false", 0xae34ef605223c823),
        ("building RollingLine true true", 0xec8f6cb6f98b5a73),
        ("building PotterWheel false false", 0x0e470ae4627835b4),
        ("building PotterWheel true false", 0x71853377b57474c4),
        ("building PotterWheel true true", 0x8463efc5d602a7d8),
        ("building OilPress false false", 0x845516f0bbab5832),
        ("building OilPress true false", 0x1bbb197983e3d6eb),
        ("building OilPress true true", 0xe7f7b4c4f325b61e),
        ("building Tannery false false", 0xa3534ba2155593f9),
        ("building Tannery true false", 0x8974c66e02bd344d),
        ("building Tannery true true", 0x6647fd0edcd456d1),
        ("building DryingRack false false", 0xeff2ea5be79d5151),
        ("building DryingRack true false", 0x370d56b110ccfc81),
        ("building DryingRack true true", 0x3f5232901f38b5a1),
        ("building Smokehouse false false", 0xa5bf228279df3452),
        ("building Smokehouse true false", 0x6564e730137d0f3c),
        ("building Smokehouse true true", 0x4dac8cfc80dd9a8a),
        ("building Well false false", 0x01ac745b95250135),
        ("building Well true false", 0x3a66d1e85f3a2f85),
        ("building Well true true", 0xdaee8f160bef247d),
        ("building StoneMortar false false", 0xf97bcd32b80125f9),
        ("building StoneMortar true false", 0xf8034424dd594538),
        ("building StoneMortar true true", 0xe816fb2941fc90ec),
        ("building WaterMill false false", 0x79cb47bbb53d3a37),
        ("building WaterMill true false", 0xd2aa7c29dc212289),
        ("building WaterMill true true", 0x27f9f3909a985271),
        ("building ForgeHammer false false", 0x972081d85f654504),
        ("building ForgeHammer true false", 0x9fea3fe21f9c5528),
        ("building ForgeHammer true true", 0x9698670cf99928c0),
        ("building FeltMallet false false", 0x4b3f4b3afc13498e),
        ("building FeltMallet true false", 0xf65affa74bd0c662),
        ("building FeltMallet true true", 0x0b79b63518d45469),
        ("building Ropewalk false false", 0x4c731f1a3376409d),
        ("building Ropewalk true false", 0x7680c507ec588b33),
        ("building Ropewalk true true", 0xdc4549210b668a98),
        ("building Crane false false", 0x7cf45b3657036c1c),
        ("building Crane true false", 0x1f5207305d2463b2),
        ("building Crane true true", 0x67c47790f7660a60),
        ("building GlassFurnace false false", 0x83750d9f32eac5c5),
        ("building GlassFurnace true false", 0xa1185644f7aabf83),
        ("building GlassFurnace true true", 0x49ecc8b01778aeb9),
        ("building PaperVat false false", 0xd50517552fea2bb7),
        ("building PaperVat true false", 0xf39fe021b3bc5cab),
        ("building PaperVat true true", 0x0a871aa427078df3),
        ("building PrintingPress false false", 0xb01d2e6d458ff83c),
        ("building PrintingPress true false", 0x5189ade3da6d544e),
        ("building PrintingPress true true", 0xdb339526ff954210),
        ("building CottonGin false false", 0x564628b3fa40a95c),
        ("building CottonGin true false", 0x7247b1feffa3d6ac),
        ("building CottonGin true true", 0x90b7005b0b9a504f),
        ("building SpinningJenny false false", 0xefecb232e7cbf406),
        ("building SpinningJenny true false", 0x7c262dd740a6c082),
        ("building SpinningJenny true true", 0x4a3d581b7f2ddcb2),
        ("building ArcFurnace false false", 0xd9bda7feb419c493),
        ("building ArcFurnace true false", 0x546fbdfc065a7c35),
        ("building ArcFurnace true true", 0x2292d6af5999fea7),
        ("building Refinery false false", 0x019642f4ea14615f),
        ("building Refinery true false", 0x22274f928836b8b2),
        ("building Refinery true true", 0x243c14cfe3521e93),
        ("building Still false false", 0x7bfc63899e88e04d),
        ("building Still true false", 0xee533d89b0527c76),
        ("building Still true true", 0x735a6f75e065a2cb),
        ("building Electrolysis false false", 0x41b4973949c9bcd3),
        ("building Electrolysis true false", 0xf8d43a310afa6367),
        ("building Electrolysis true true", 0xa82da4e0162706fb),
        ("building Compressor false false", 0x7700fc1e86d1cab1),
        ("building Compressor true false", 0x5a6e2f7d4d555ad4),
        ("building Compressor true true", 0xf08ee7e2cd57df50),
        ("building Railway false false", 0x97cf1aefec41a817),
        ("building Railway true false", 0x9097541c2098c5f4),
        ("building Railway true true", 0x421ef16777b2bc02),
        ("building SteamPump false false", 0x9bd16b9d030ddbd2),
        ("building SteamPump true false", 0x42a91cb41ff4e9f0),
        ("building SteamPump true true", 0x24594f3ea77a47ee),
        ("building Transformer false false", 0xcc536613ad67adcc),
        ("building Transformer true false", 0x411c0926612d37e4),
        ("building Transformer true true", 0x9d6e5a772f428590),
        ("building Accumulator false false", 0x144d5979939fed4e),
        ("building Accumulator true false", 0x4b56355335435d2f),
        ("building Accumulator true true", 0x4f9faa868c537ec7),
        ("building Welder false false", 0x3679b8f66ce02c20),
        ("building Welder true false", 0x871a0a30244f8869),
        ("building Welder true true", 0x64be46ad29866bdd),
        ("building Reactor false false", 0x0a60dc9dd98c749e),
        ("building Reactor true false", 0xf3e92d1dd4d93282),
        ("building Reactor true true", 0xcb2759b3bed23fbd),
        ("building Cooler false false", 0x174651852772b289),
        ("building Cooler true false", 0xfabaf31af7c5196f),
        ("building Cooler true true", 0xa02d7b0e06fe0da7),
        ("building Telegraph false false", 0xd4a5f392aeb98c45),
        ("building Telegraph true false", 0x76c9304566be5185),
        ("building Telegraph true true", 0x4129fedb521b2bd5),
        ("building Radio false false", 0x2190ec36e80b8768),
        ("building Radio true false", 0x13e778e9c8ac24ca),
        ("building Radio true true", 0x4e4bdab85e86bff6),
        ("building VacuumPump false false", 0x22e1162e8f15b1f5),
        ("building VacuumPump true false", 0x3d48196b4eb46232),
        ("building VacuumPump true true", 0x12937431145d9035),
        ("building CrystalPuller false false", 0x3d39752940d9369a),
        ("building CrystalPuller true false", 0x61f34deeacfd0666),
        ("building CrystalPuller true true", 0xa08baa28452db696),
        ("building Photolith false false", 0x7475be00147b7dd9),
        ("building Photolith true false", 0xa7d21eabe58bb26f),
        ("building Photolith true true", 0xdfc55481d79b33c3),
        ("building CncMachine false false", 0xcc47519010e992a5),
        ("building CncMachine true false", 0x4f896d37fc6a12aa),
        ("building CncMachine true true", 0x7fd4d7575f19c9e2),
        ("building AssemblyRobot false false", 0x242bce00dbd70565),
        ("building AssemblyRobot true false", 0xb9201947e332d82e),
        ("building AssemblyRobot true true", 0x34eb404d0e9e4843),
        ("campfire 0", 0xb81451d68c550836),
        ("campfire 0.4", 0xb3ff0ed12254b8e3),
        ("campfire 1", 0x39408f657b7c3591),
        ("human 0", 0x1cac49b4cef69ad6),
        ("chips 0", 0xf1ee7d13c71175fd),
        ("human 1", 0x8106b41d797eba07),
        ("chips 1", 0x4f90e872f7b9bb49),
        ("human 2", 0x9383d9f22762936a),
        ("chips 2", 0x29212fa195674fb1),
        ("human 3", 0xe3e2159d41692ccb),
        ("chips 3", 0x24a4650a3d23774d),
        ("human 4", 0x446ef08e8e6afa2a),
        ("chips 4", 0x80890b727ab91b75),
        ("human 5", 0xf428dd76c3e63fdb),
        ("chips 5", 0xb0b82e4a8b2b2ce5),
        ("human 6", 0xc0f077d08abd914b),
        ("chips 6", 0x208689db08b27ee5),
        ("human 7", 0x1c20abc6c3e5c4d2),
        ("chips 7", 0xb01cc00bdd0007b1),
        ("human 8", 0x104ecfcd998a7eff),
        ("chips 8", 0x8f35433f8bfe4225),
        ("human 9", 0x2737ba4c2dc2e6ea),
        ("chips 9", 0xa0d90ffb37f89491),
        ("human 10", 0x41f9a8bb4b0cbd68),
        ("chips 10", 0x74cdbe2ae6a4fb15),
        ("human 11", 0xc94c78b50abc2100),
        ("chips 11", 0x5d40cb8c63cb9efd),
        ("human 12", 0xce1c816a7c5d16db),
        ("chips 12", 0x3d32ec87c4f000e1),
        ("human 13", 0xda50deae38eaa141),
        ("chips 13", 0xdfec6e628fa8f5b1),
        ("human 14", 0x770efb070dfeff59),
        ("chips 14", 0xcbe30d69dd14f68d),
        ("human 15", 0xeab78557f717a537),
        ("chips 15", 0x1b7bb2ed30491225),
        ("human 16", 0x632e1bfa6a03bfac),
        ("chips 16", 0xdf641d4e5def6905),
        ("human 17", 0xd6077034f1557dd3),
        ("chips 17", 0xd12ff22a885b3e59),
        ("creature Human false", 0x93eb1b4c41b928a5),
        ("creature Human true", 0xaab18dc051750d7a),
        ("creature Dog false", 0x6fc52b1f7d1ebda1),
        ("creature Dog true", 0x53e477bd6757afd6),
        ("creature Ox false", 0xcdf0b26b008a7aff),
        ("creature Ox true", 0xdf878f802d7a55ad),
        ("creature Horse false", 0x6a7604b438a1eb53),
        ("creature Horse true", 0x32226edc8562bd3f),
        ("creature Bot false", 0x6644cb396f8d278e),
        ("creature Bot true", 0xa7a2bdd7e9f35c72),
        ("tracks false", 0xb831020fc0f2b825),
        ("tracks true", 0x20419d2ff88b7f95),
        ("bodies", 0x8f98cf8bbcbcc498),
        ("fish", 0x47ee7cb2e191bc12),
        ("rim-empty", 0xcbf29ce484222325),
        ("rim-tree", 0xa1366b6a0db5bd77),
        ("cave L1 block 4,3", 0x1d906b027c1cc3e5),
        ("cave L1 block 3,4", 0xca060e6e0bcfdb30),
        ("cave L2 block 4,3", 0x5554431eebd18fd1),
        ("cave L2 block 3,4", 0x34cf2a0dba3ee948),
        ("cave L3 block 4,3", 0x276c1cb066036625),
        ("cave L3 block 3,4", 0xf06e5d85128be82c),
        ("cave L4 block 4,3", 0x1c28f519097787b3),
        ("cave L4 block 3,4", 0xa4ca580d40ec43a2),
        ("torch 0", 0x709f6551d60aba86),
        ("torch 0.1", 0x64f1c54900cd1336),
        ("torch 0.9", 0xebbcaeb29ee820a9),
    ];

    #[test]
    fn mesh_output_is_unchanged() {
        let got = cases();
        if std::env::var_os("ALAZ_PRINT_HASHES").is_some() {
            for (k, h) in &got {
                println!("(\"{k}\", {h:#018x}),");
            }
        }
        assert_eq!(got.len(), EXPECTED.len());
        let bad: Vec<&str> = got.iter().zip(EXPECTED).filter(|(g, e)| g.0 != e.0 || g.1 != e.1).map(|(g, _)| g.0.as_str()).collect();
        assert!(bad.is_empty(), "meshes changed: {bad:?}");
    }

    /// Vertices of every building at work (`cargo test --release -p alaz-render machine_vertex_counts -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn machine_vertex_counts() {
        for kind in BuildingKind::ALL {
            let mut o = fresh(false);
            push_building(&mut o, kind, 12.5, 7.25, true, 1234, None);
            println!("{kind:?}: {} verts", o.len());
        }
    }

    /// Cost of building a crowd's bodies, full against simple build (`cargo test --release -p alaz-render crowd_cost -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn crowd_cost() {
        for outfit in [0u16, 0xFFFF] {
            for n in [1u32, 10, 30, 60] {
                let mut res = [(0usize, 0.0f64); 2];
                for (k, lod) in [false, true].into_iter().enumerate() {
                    let reps = 400 / n.max(1) + 20;
                    let mut verts = 0;
                    let t0 = std::time::Instant::now();
                    for r in 0..reps {
                        let mut o = Mesh::new();
                        for i in 0..n {
                            let f = (i + r) as f32;
                            let walking = i % 3 != 0;
                            let pose = Pose { heading: f * 0.7, phase: f * 1.3, amp: if walking { 1.0 } else { 0.0 }, outfit, evo: 8.0, id: i * 5 + 1, lod, ..Pose::idle(3.0) };
                            push_creature(&mut o, AgentKind::Human, 5.0 + i as f32, 9.0, false, pose);
                        }
                        verts = std::hint::black_box(o.len());
                    }
                    res[k] = (verts, t0.elapsed().as_secs_f64() * 1000.0 / reps as f64);
                }
                println!("outfit {outfit:#06x} N={n:3}: full {:7} verts {:7.3} ms | lod {:7} verts {:7.3} ms | verts x{:.2} time x{:.2}", res[0].0, res[0].1, res[1].0, res[1].1, res[1].0 as f64 / res[0].0 as f64, res[1].1 / res[0].1);
            }
        }
    }

    fn crowd_jobs(n: u32, outfit: u16, lod: bool) -> Vec<BodyJob> {
        (0..n)
            .map(|i| {
                let f = i as f32;
                let pose = Pose { heading: f * 0.7, phase: f * 1.3, amp: if i % 3 != 0 { 1.0 } else { 0.0 }, outfit, evo: 8.0, id: i * 5 + 1, lod, hurt: if i % 7 == 0 { 0.6 } else { 0.0 }, ..Pose::idle(3.0) };
                BodyJob {
                    kind: AgentKind::Human,
                    x: 5.0 + f,
                    z: 9.0 + (i % 5) as f32,
                    marked: i % 2 == 0,
                    is_me: i == 13,
                    pose,
                    chips: None,
                    lift: if i % 11 == 0 { Some(0.4) } else { None },
                    lean: Some((0.1, 0.05)),
                    heading: pose.heading,
                    depth: (i % 3) as u8 * (i % 2) as u8,
                    wave: 0.2,
                    secs: 3.0,
                    seed: i,
                }
            })
            .collect()
    }

    /// A crowd built on several threads is the same bytes as one built a body at a time.
    #[test]
    fn parallel_bodies_match_serial() {
        let (art, _) = sprites::build();
        let wind = [0.6, -0.3, 0.7];
        for n in [3u32, 8, 21, 60] {
            let jobs = crowd_jobs(n, 0xFFFF, false);
            let (par, par_me) = build_bodies(&jobs, &art, wind);
            let mut one = Vec::new();
            let mut one_me = None;
            for j in &jobs {
                let (m, r) = build_bodies(std::slice::from_ref(j), &art, wind);
                if let Some(r) = r {
                    one_me = Some(one.len() as u32 + r.start..one.len() as u32 + r.end);
                }
                one.extend_from_slice(&m);
            }
            assert!(bytemuck::cast_slice::<Vertex, u8>(&par) == bytemuck::cast_slice::<Vertex, u8>(&one), "n={n}");
            assert_eq!(par_me, one_me);
        }
    }

    /// The same crowd, one thread against the threads `build_bodies` uses.
    #[test]
    #[ignore]
    fn crowd_cost_parallel() {
        let (art, _) = sprites::build();
        for outfit in [0u16, 0xFFFF] {
            for n in [10u32, 30, 60] {
                let jobs = crowd_jobs(n, outfit, false);
                let reps = 400 / n + 20;
                let time = |f: &dyn Fn() -> usize| {
                    let t0 = std::time::Instant::now();
                    let mut v = 0;
                    for _ in 0..reps {
                        v = std::hint::black_box(f());
                    }
                    (v, t0.elapsed().as_secs_f64() * 1000.0 / reps as f64)
                };
                let (v1, t1) = time(&|| {
                    let mut o = Mesh::new();
                    for j in &jobs {
                        let (m, _) = build_bodies(std::slice::from_ref(j), &art, [0.0; 3]);
                        o.extend_from_slice(&m);
                    }
                    o.len()
                });
                let (v2, t2) = time(&|| build_bodies(&jobs, &art, [0.0; 3]).0.len());
                println!("outfit {outfit:#06x} N={n:3}: serial {v1} verts {t1:.3} ms | parallel {v2} verts {t2:.3} ms | x{:.2}", t2 / t1);
            }
        }
    }
}

#[cfg(test)]
mod evolution {
    use super::*;

    /// A standing or walking person at a stage coordinate, built in the full or the simple build.
    fn person(evo: f32, outfit: u16, id: u32, walking: bool, lod: bool) -> Mesh {
        let mut o = Mesh::new();
        let pose = Pose { evo, outfit, id, heading: 0.4, phase: 1.1, amp: if walking { 1.0 } else { 0.0 }, lod, ..Pose::idle(2.0) };
        push_human(&mut o, 5.0, 9.0, pose);
        o
    }

    /// Where a mesh is and what colour it is, in four numbers: box corners, centroid and mean colour.
    fn summary(m: &Mesh) -> ([f32; 3], [f32; 3], [f32; 3], [f32; 3]) {
        let (mut lo, mut hi, mut c, mut col) = ([f32::MAX; 3], [f32::MIN; 3], [0.0f32; 3], [0.0f32; 3]);
        for v in m.iter() {
            for i in 0..3 {
                lo[i] = lo[i].min(v.pos[i]);
                hi[i] = hi[i].max(v.pos[i]);
                c[i] += v.pos[i];
                col[i] += v.color[i];
            }
        }
        let n = m.len().max(1) as f32;
        (lo, hi, c.map(|v| v / n), col.map(|v| v / n))
    }

    /// Between two stage coordinates a hundredth apart the body hardly moves: the box, centroid and colour stay
    /// within a hair of each other (a few centimetres: a wide limb's ring of corners turns in steps as its axis
    /// swings), over the whole run from ape to sage. (A trinket joins the mesh while it is
    /// still inside the body or a few millimetres across, so it can only nudge the mean colour a little.)
    #[test]
    fn evolution_is_continuous() {
        for (outfit, id, walking, lod) in [(0u16, 12u32, false, false), (0, 5, true, false), (0x3FF, 12, false, false), (7, 8, true, true), (28, 1, false, false), (0, 10, false, true)] {
            let mut last = summary(&person(0.0, outfit, id, walking, lod));
            let mut last_len = person(0.0, outfit, id, walking, lod).len();
            let mut worst = 0.0f32;
            let mut at = 0.0f32;
            let mut v = 0.0f32;
            while v < 9.0 {
                v = (v + 0.01).min(9.0);
                let m = person(v, outfit, id, walking, lod);
                let s = summary(&m);
                for i in 0..3 {
                    let step = (s.0[i] - last.0[i]).abs().max((s.1[i] - last.1[i]).abs());
                    if step > worst {
                        worst = step;
                        at = v;
                    }
                    assert!((s.0[i] - last.0[i]).abs() < 0.05 && (s.1[i] - last.1[i]).abs() < 0.05, "box jumps at {v} (outfit {outfit:#x} id {id})");
                    // (the simple build has a fifth of the vertices, so a trinket weighs more in its centroid)
                    assert!(lod || (s.2[i] - last.2[i]).abs() < 0.03, "centroid jumps at {v} (outfit {outfit:#x} id {id}): {:?} -> {:?}", last.2, s.2);
                    assert!((s.3[i] - last.3[i]).abs() < if lod { 0.04 } else { 0.02 }, "colour jumps at {v} (outfit {outfit:#x} id {id}): {:?} -> {:?} ({} -> {} vertices)", last.3, s.3, last_len, m.len());
                }
                last = s;
                last_len = m.len();
            }
            println!("outfit {outfit:#x} id {id} walking {walking} lod {lod}: largest box step {worst:.4} m per 0.01 stage (at {at:.2})");
        }
    }

    /// Every named stage has a body of its own (each differs from the next by a clear amount), the ape stands
    /// shorter and more bent than the sage, and the sage is taller than any earlier stage.
    #[test]
    fn stages_read_apart() {
        let mut heights = Vec::new();
        for i in 0..EVO_STAGES {
            let a = person(i as f32, 0, 12, false, false);
            if i + 1 < EVO_STAGES {
                let b = person(i as f32 + 1.0, 0, 12, false, false);
                let differing = a.iter().zip(b.iter()).filter(|(p, q)| p.pos != q.pos || p.color != q.color).count() + a.len().abs_diff(b.len());
                assert!(differing > a.len() / 5, "stage {i} and {} look alike ({differing} of {} vertices differ)", i + 1, a.len());
            }
            heights.push(summary(&a).1[1]);
        }
        assert!(heights[9] > heights[0] + 0.2, "{heights:?}");
        assert!(heights.windows(2).all(|w| w[1] >= w[0] - 0.02), "{heights:?}");
    }

    /// A far (simple build) person reflects the stage as the full one does: the same height and bend.
    #[test]
    fn lod_follows_the_stage() {
        for v in [0.0, 2.5, 5.0, 9.0] {
            let (a, b) = (summary(&person(v, 0, 12, false, false)), summary(&person(v, 0, 12, false, true)));
            assert!((a.1[1] - b.1[1]).abs() < 0.08, "height at {v}: {} vs {}", a.1[1], b.1[1]);
        }
        assert!(summary(&person(0.0, 0, 12, false, true)).1[1] < summary(&person(9.0, 0, 12, false, true)).1[1] - 0.2);
    }

    /// Sitting at the fire works at every stage: the hips are down and nothing is left standing above the head.
    #[test]
    fn sitting_at_every_stage() {
        for i in 0..=18 {
            let v = i as f32 * 0.5;
            let mut o = Mesh::new();
            push_human(&mut o, 5.0, 9.0, Pose { evo: v, id: 12, sit: true, ..Pose::idle(2.0) });
            let mut st = Mesh::new();
            push_human(&mut st, 5.0, 9.0, Pose { evo: v, id: 12, ..Pose::idle(2.0) });
            assert!(summary(&o).1[1] < summary(&st).1[1] - 0.1, "sitting at {v}");
            assert!(o.iter().all(|p| p.pos.iter().all(|c| c.is_finite())));
        }
    }
}
