use crate::renderer::*;
use crate::*;
use alaz_sim::cave_terrain::CaveTile;
use std::collections::{BTreeMap, BTreeSet};

/// Globals slots that hold the swell's six wave phases (the spare `w` of cam, hwin, cam_r and cam_u).
pub(crate) const WAVE_SLOTS: [usize; 6] = [22, 23, 26, 27, 63, 67];
/// Globals slots for torch strength and the local camera cut.
pub(crate) const CAVE_SLOT: usize = 120;
/// Globals slot of the small resource under the pointer (xyz, w = 1 if there is one): crowns open up around it.
pub(crate) const LOOK_SLOT: usize = 124;

#[derive(Clone, Copy, PartialEq, Eq)]
struct CaveSceneKey {
    seed: u64,
    bounds: (i32, i32, i32, i32),
    player_floor: Option<CaveTile>,
    revision: u64,
}

pub(super) struct NaturalCaveView {
    key: CaveSceneKey,
    entrances: Arc<CaveSection>,
    floors: Arc<CaveSection>,
    daylight: Arc<BTreeMap<CaveTile, f32>>,
    carve: Arc<EntranceCarve>,
    flooded: BTreeSet<CaveTile>,
    water_tick: u64,
    mesh: Option<(wgpu::Buffer, u32)>,
}

#[derive(Clone, Copy)]
pub(super) struct SurfaceCutaway {
    center: glam::Vec3,
    right: glam::Vec3,
    up: glam::Vec3,
}

impl SurfaceCutaway {
    fn contains(self, point: glam::Vec3) -> bool {
        let d = point - self.center;
        d.y > 0.25
            && d.dot(self.right.cross(self.up)) > 0.3
            && glam::Vec2::new(d.x, d.z).length_squared() < 4.2 * 4.2
            && glam::Vec2::new(d.dot(self.right), d.dot(self.up)).length_squared() < 2.1 * 2.1
    }
}

fn cave_scene_section(world: &World, entrances: &CaveSection, player_floor: Option<CaveTile>) -> CaveSection {
    let mut section = entrances.clone();
    if let Some(floor) = player_floor {
        // Include the traversable gallery around the player in the same coordinates as the entrances.
        for (tile, ceiling) in world.cave_entrance_section(floor, 24) {
            section.insert(tile, ceiling);
        }
    }
    section
}

impl Renderer {
    pub fn cave_floor_heights(&self, x: i32, y: i32) -> impl Iterator<Item = i16> + '_ {
        let low = CaveTile { x, y, z: i16::MIN };
        let high = CaveTile { x, y, z: i16::MAX };
        self.natural_cave_view.iter().flat_map(move |v| v.floors.range(low..=high).map(|(tile, _)| tile.z))
    }

    pub fn has_cave_floor(&self, x: i32, y: i32, z: i16) -> bool {
        self.natural_cave_view.as_ref().is_some_and(|v| v.floors.contains_key(&CaveTile { x, y, z }))
    }

    pub fn natural_cave_floor(&self, x: i32, y: i32) -> Option<i16> {
        if let Some(tile) = self.natural_cave_view.as_ref()?.key.player_floor {
            self.cave_floor_heights(x, y).min_by_key(|&z| tile.z.abs_diff(z))
        } else {
            self.cave_floor_heights(x, y).max()
        }
    }

    pub fn surface_cutaway_contains(&self, point: glam::Vec3) -> bool {
        self.surface_cutaway.is_some_and(|cut| cut.contains(point))
    }

    fn update_natural_cave(&mut self, world: &World, bounds: (i32, i32, i32, i32), player_floor: Option<CaveTile>) {
        if !alaz_sim::map::has_natural_caves(world.seed) {
            self.natural_cave_view = None;
            return;
        }
        let key = CaveSceneKey { seed: world.seed, bounds, player_floor, revision: world.natural_cave_revision() };
        let water_tick = world.tick / 30;
        let previous = self.natural_cave_view.as_ref();
        if previous.is_some_and(|v| v.key == key && v.water_tick == water_tick) {
            return;
        }

        let entrances = previous
            .filter(|v| v.key.seed == key.seed && v.key.bounds == key.bounds && v.key.revision == key.revision)
            .map_or_else(|| Arc::new(collect_cave_entrances(world, bounds)), |v| Arc::clone(&v.entrances));
        let floors = if let Some(v) = previous.filter(|v| v.key == key) {
            Arc::clone(&v.floors)
        } else if player_floor.is_none() {
            Arc::clone(&entrances)
        } else {
            let section = cave_scene_section(world, &entrances, player_floor);
            previous.filter(|v| *v.floors == section)
                .map_or_else(|| Arc::new(section), |v| Arc::clone(&v.floors))
        };
        let same_floors = previous.filter(|v| v.key.seed == key.seed && Arc::ptr_eq(&v.floors, &floors));
        let daylight = same_floors.map_or_else(|| Arc::new(cave_daylight(world, &floors)), |v| Arc::clone(&v.daylight));
        let carve = same_floors.map_or_else(|| Arc::new(entrance_carve(world, &floors)), |v| Arc::clone(&v.carve));
        let flooded = floors.keys().copied().filter(|&tile| world.natural_cave_flooded(tile)).collect();
        let mesh = if let Some(v) = same_floors.filter(|v| v.key.revision == key.revision && v.flooded == flooded) {
            v.mesh.clone()
        } else if floors.is_empty() {
            None
        } else {
            let vertices = build_cave_scene(world, &floors, &daylight, &flooded);
            Some((self.vertex_buffer(&vertices), vertices.len() as u32))
        };
        self.natural_cave_view = Some(NaturalCaveView { key, entrances, floors, daylight, carve, flooded, water_tick, mesh });
    }
}

/// The shade channel of a water tile: 1.0 for still water, and for a river 2.0 plus the current (mm a tick, each
/// component -16..=16) packed as (x + 16) * 40 + (y + 16). The shader decodes it in `flow_at_tile`; land never sees it
/// because `tile_at` hides anything above 1.5.
pub(super) fn water_flow_code((cx, cy): (i64, i64)) -> f32 {
    if (cx, cy) == (0, 0) {
        return 1.0;
    }
    2.0 + ((cx.clamp(-16, 16) + 16) * 40 + (cy.clamp(-16, 16) + 16)) as f32
}

const TRACK_SPACING_M: f32 = 0.8;
const TRACK_LIFETIME_TICKS: u64 = 12 * alaz_sim::time::TICKS_PER_SECOND;
const MAX_TRACK_MARKS: usize = 768;

pub(super) struct TrackMark {
    at: glam::Vec2,
    tick: u64,
    wolf: bool,
}

#[derive(Default)]
pub(super) struct TrackStepper {
    previous: Option<(u64, glam::Vec2)>,
    carried_m: f32,
    left: bool,
}

impl TrackStepper {
    fn advance(&mut self, to: glam::Vec2, frame: u64, mut stamp: impl FnMut(glam::Vec2)) {
        let Some((last_frame, from)) = self.previous.replace((frame, to)) else { return };
        let motion = to - from;
        let distance = motion.length();
        if frame.saturating_sub(last_frame) != 1 || distance >= 3.0 {
            // Culling and position corrections can skip part of a path.
            self.carried_m = 0.0;
            return;
        }
        if distance == 0.0 {
            return;
        }
        let along = motion / distance;
        let across = glam::Vec2::new(-along.y, along.x);
        let mut next = TRACK_SPACING_M - self.carried_m;
        while next <= distance {
            let offset = if self.left { 0.18 } else { -0.18 };
            stamp(from + along * next + across * offset);
            self.left = !self.left;
            next += TRACK_SPACING_M;
        }
        self.carried_m = (distance - next + TRACK_SPACING_M).max(0.0);
    }
}

#[cfg(test)]
mod track_tests {
    use super::*;
    use glam::Vec2;

    #[test]
    fn a_rivers_current_survives_the_trip_through_the_tile_texture() {
        assert_eq!(water_flow_code((0, 0)), 1.0);
        for (cx, cy) in [(5, 0), (-5, 0), (0, 7), (3, -4), (-16, 16), (99, -99)] {
            let n = water_flow_code((cx, cy)) - 2.0;
            let a = (n / 40.0).floor();
            let (dx, dy) = (a - 16.0, n - a * 40.0 - 16.0);
            assert_eq!((dx, dy), (cx.clamp(-16, 16) as f32, cy.clamp(-16, 16) as f32));
        }
    }

    #[test]
    fn paw_marks_stay_at_footfalls_when_animal_turns_or_stops() {
        let mut steps = TrackStepper::default();
        let mut marks = Vec::new();
        for (frame, to) in [
            (1, Vec2::ZERO),
            (2, Vec2::new(0.5, 0.0)),
            (3, Vec2::X),
            (4, Vec2::ONE),
            (5, Vec2::ONE),
        ] {
            steps.advance(to, frame, |at| marks.push(at));
        }
        assert_eq!(marks.len(), 2);
        assert!(marks[0].distance(Vec2::new(0.8, -0.18)) < 0.001);
        assert!(marks[1].distance(Vec2::new(0.82, 0.6)) < 0.001);
    }

    #[test]
    fn paw_marks_do_not_connect_unobserved_positions() {
        let mut steps = TrackStepper::default();
        for (frame, to) in [
            (1, Vec2::X),
            (3, Vec2::new(2.0, 0.0)),
            (4, Vec2::new(6.0, 0.0)),
        ] {
            steps.advance(to, frame, |_| panic!("no marks across a gap"));
        }
    }
}

impl Renderer {
    /// The surface layer of the tile window: mud, puddles, ice, ash and flames, kept in the kind channel as
    /// `kind + 16 * code` (code: the surface in the low three bits, for fire and ash their progress above them; see
    /// `World::surface_code`). The weather's part is worked out for the stretch of tiles around the camera when the
    /// wetness has moved a step, the cold has come or gone, or the camera has gone some way; the fire's part whenever a
    /// tile catches or goes out (and as flames and ash age). With dry ground and nothing burning it costs nothing.
    fn update_surfaces(&mut self, world: &World, hx: i32, hy: i32, cam_tile: (i32, i32)) {
        let ground = world.ground();
        let active = !ground.quiet() || world.has_burn();
        if !active && !self.surf_on {
            return;
        }
        let win = HWIN as i32;
        let cell = (cam_tile.0.div_euclid(24), cam_tile.1.div_euclid(24));
        // (flames change look every couple of seconds, ash every half minute)
        let age = if world.burning().next().is_some() { world.tick / 120 } else { world.tick / 1800 };
        let key = (world.seed, hx, hy, ground.wet / 24, ground.freezing(), world.burn_version(), age, cell.0, cell.1);
        if self.surf_key == Some(key) || self.tile_cpu.len() != (win * win * 4) as usize {
            return;
        }
        let wkey = (world.seed, hx, hy, ground.wet / 24, ground.freezing(), cell.0, cell.1);
        if self.surf_wkey != Some(wkey) {
            self.surf_base.fill(0);
            if !ground.quiet() {
                // 72 tiles either side of the camera's cell: all the screen shows at the widest zoom
                let (x0, x1) = ((cell.0 * 24 - 72).max(hx), (cell.0 * 24 + 96).min(hx + win));
                let (y0, y1) = ((cell.1 * 24 - 72).max(hy), (cell.1 * 24 + 96).min(hy + win));
                for ty in y0..y1 {
                    for tx in x0..x1 {
                        self.surf_base[((ty - hy) * win + (tx - hx)) as usize] = ground.weather_surface(&world.map, tx, ty) as u8;
                    }
                }
            }
            self.surf_wkey = Some(wkey);
        }
        for (i, px) in self.tile_cpu.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let kind = px[2] - 16.0 * (px[2] / 16.0).floor();
            px[2] = kind + 16.0 * self.surf_base[i] as f32;
        }
        // flames and ash over the weather's mud
        for (&(tx, ty), _) in world.burn_iter() {
            let (x, y) = (tx - hx, ty - hy);
            if x >= 0 && y >= 0 && x < win && y < win {
                let px = &mut self.tile_cpu[((y * win + x) * 4 + 2) as usize];
                let kind = *px - 16.0 * (*px / 16.0).floor();
                *px = kind + 16.0 * world.surface_code(&ground, tx, ty) as f32;
            }
        }
        self.queue.write_texture(
            self.tile_tex.as_image_copy(),
            bytemuck::cast_slice(&self.tile_cpu),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(HWIN * 16), rows_per_image: Some(HWIN) },
            wgpu::Extent3d { width: HWIN, height: HWIN, depth_or_array_layers: 1 },
        );
        self.surf_on = active;
        self.surf_key = Some(key);
    }

    /// Hollow the ground at the cave mouths: patch the changed tile corners into the height texture, and drop the
    /// chunks whose ground was built or baked from the old heights.
    fn apply_cave_carve(&mut self, world: &World, hx: i32, hy: i32) {
        let desired = self.natural_cave_view.as_ref().map_or_else(Default::default, |v| Arc::clone(&v.carve));
        let win = HWIN as i32;
        if Arc::ptr_eq(&desired, &self.carve_applied) || (desired.is_empty() && self.carve_applied.is_empty()) || self.height_cpu.len() != (win * win * 4) as usize {
            return;
        }
        let old = std::mem::replace(&mut self.carve_applied, Arc::clone(&desired));
        let corners: BTreeSet<(i32, i32)> = old.keys().chain(desired.keys()).copied().collect();
        let mut stale = BTreeSet::new();
        for (tx, ty) in corners {
            let (x, y) = (tx - hx, ty - hy);
            if x < 0 || y < 0 || x >= win || y >= win {
                continue;
            }
            let want = desired.get(&(tx, ty)).copied().unwrap_or_else(|| alaz_sim::elevation_mm(world.seed, tx, ty) as f32 / 1000.0);
            let i = ((y * win + x) * 4) as usize;
            if self.height_cpu[i] == want {
                continue;
            }
            self.height_cpu[i] = want;
            let mut target = self.height_tex.as_image_copy();
            target.origin = wgpu::Origin3d { x: x as u32, y: y as u32, z: 0 };
            self.queue.write_texture(
                target,
                bytemuck::cast_slice(&self.height_cpu[i..i + 4]),
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(16), rows_per_image: Some(1) },
                wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            );
            // the smooth surface reaches two tiles either side of a corner
            for (ax, ay) in [(tx - 2, ty - 2), (tx + 1, ty - 2), (tx - 2, ty + 1), (tx + 1, ty + 1)] {
                stale.insert((ax.div_euclid(CHUNK_TILES), ay.div_euclid(CHUNK_TILES)));
            }
        }
        for key in stale {
            self.chunks.remove(&key);
        }
    }

    pub fn render(
        &mut self,
        world: &World,
        cam: &Camera,
        ghost: Option<Ghost>,
        me: alaz_sim::AgentId,
        routes: &[usize],
        mut hud: impl FnMut(&egui::Context),
    ) {
        let t_start = std::time::Instant::now();
        self.hot_reload_poll();
        self.assets_poll();
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => {
                // the old frame must be gone before the surface is configured again, so skip this one
                drop(f);
                self.surface.configure(&self.device, &self.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            _ => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let t_acquired = std::time::Instant::now();
        let (win_w, win_h) = (self.config.width, self.config.height);
        let mut cam = *cam;
        if let Some(v) = &mut self.view {
            // the preview: a fixed spot (where the camera was at the start), the camera circling it
            let at = *v.at.get_or_insert([cam.target.x, cam.target.z]);
            cam.target = glam::Vec3::new(at[0], 0.0, at[1]);
            cam.yaw_deg = world.tick as f32 / 60.0 * 24.0;
            cam.pitch_deg = 38.0;
        }
        cam.win = (win_w, win_h);
        let scale = cam.scale();
        let (sw, sh) = cam.tex_dims();

        let player = world.agents.iter().find(|a| a.id == me);
        let cave = player.and_then(|a| a.cave_view());
        let player_floor = cave.map(|c| {
            let (x, y) = c.tile();
            CaveTile { x, y, z: c.elevation_m() }
        });
        let (cx0, cy0, cx1, cy1, view_m) = visible_chunks(&cam, self.aspect(), world);
        let bounds = (cx0 * CHUNK_TILES, cy0 * CHUNK_TILES, (cx1 + 1) * CHUNK_TILES, (cy1 + 1) * CHUNK_TILES);
        self.update_natural_cave(world, bounds, player_floor);
        let cave_floors = self.natural_cave_view.as_ref().map(|v| Arc::clone(&v.floors));
        let cave_daylight = self.natural_cave_view.as_ref().map(|v| Arc::clone(&v.daylight));
        let natural_mesh = self.natural_cave_view.as_ref().and_then(|v| v.mesh.clone());
        let gh = |x: f32, z: f32| ground_height(world.seed, x, z);
        // --- globals: view-projection from the texel-snapped camera + time-of-day light tint
        let l = daylight_permille(world.tick) as f32 / 1000.0;
        let t = ((1.0 - l) / 0.7).clamp(0.0, 1.0);
        // the season colours the whole picture a little: fresh spring, warm summer, rusty autumn, cold winter
        let season_tint: [f32; 3] = [[1.0, 1.03, 1.0], [1.06, 1.02, 0.95], [1.10, 0.97, 0.84], [0.94, 1.0, 1.10]][world.season() as usize];
        let tint: [f32; 3] = std::array::from_fn(|i| (1.0 + (NIGHT_TINT[i] - 1.0) * t) * season_tint[i]);
        let (snap_cam, off) = cam.snapped();
        let sky = match world.weather() {
            alaz_sim::time::Weather::Clear => 0.0,
            alaz_sim::time::Weather::Rain => 1.0,
            alaz_sim::time::Weather::Snow => 2.0,
        };
        // the weather fades in and out instead of switching
        let wtarget = if sky > 0.0 { 1.0 } else { 0.0 };
        self.weather_amount += (wtarget - self.weather_amount) * 0.01;
        if sky > 0.0 {
            self.weather_kind = sky;
        }
        // Terrain height, climate, and shoreline use the same window for the whole scene.
        let origin = |v: f32| ((v - HWIN as f32 / 2.0) / 64.0).round() as i32 * 64;
        let (hx, hy) = (origin(cam.target.x), origin(cam.target.z));
        let height_key = (world.seed, hx, hy);
        if self.height_at != Some(height_key) {
            let shore = shore_field(world, hx, hy);
            let data: Vec<f32> = (0..HWIN as i32 * HWIN as i32)
                .flat_map(|i| {
                    let (tx, ty) = (hx + i % HWIN as i32, hy + i / HWIN as i32);
                    let (warm, wet) = alaz_sim::climate(world.seed, tx, ty);
                    [alaz_sim::elevation_mm(world.seed, tx, ty) as f32 / 1000.0, warm as f32 / 1000.0, wet as f32 / 1000.0, shore[i as usize]]
                })
                .collect();
            self.queue.write_texture(
                self.height_tex.as_image_copy(),
                bytemuck::cast_slice(&data),
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(HWIN * 16), rows_per_image: Some(HWIN) },
                wgpu::Extent3d { width: HWIN, height: HWIN, depth_or_array_layers: 1 },
            );
            // and what ground each tile of the window wears
            let tiles: Vec<f32> = (0..HWIN as i32 * HWIN as i32)
                .flat_map(|i| {
                    let (tx, ty) = (hx + i % HWIN as i32, hy + i / HWIN as i32);
                    let mut t = world.map.terrain(tx, ty).unwrap_or(Terrain::Water);
                    // the gravel of a ford wears the stone ground: grey pebbles in the stream, not a bar of sand
                    if t == Terrain::Sand && alaz_sim::map::is_ford_gravel(&world.map, tx, ty) {
                        t = Terrain::Rock;
                    }
                    let h = (alaz_sim::rng::hash2(world.seed ^ 0x7772, tx, ty) >> 20) as u32;
                    let s = tile_spr(&self.art, t, h);
                    let k = if t == Terrain::Water { water_flow_code(alaz_sim::map::current_mm(&world.map, tx, ty)) } else { tile_shade(world, tx, ty) };
                    [s.x as f32, s.y as f32, t as u8 as f32, k]
                })
                .collect();
            self.queue.write_texture(
                self.tile_tex.as_image_copy(),
                bytemuck::cast_slice(&tiles),
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(HWIN * 16), rows_per_image: Some(HWIN) },
                wgpu::Extent3d { width: HWIN, height: HWIN, depth_or_array_layers: 1 },
            );
            self.tile_cpu = tiles;
            self.height_cpu = data;
            self.carve_applied = Arc::new(BTreeMap::new());
            self.height_at = Some((world.seed, hx, hy));
            // (the window was written bare: the surface layer goes on again)
            self.surf_wkey = None;
            self.surf_key = None;
            self.surf_on = false;
        }
        self.apply_cave_carve(world, hx, hy);
        self.update_surfaces(world, hx, hy, (cam.target.x as i32, cam.target.z as i32));
        let mut g = [0f32; 164];
        g[..16].copy_from_slice(&snap_cam.view_proj(1.0).to_cols_array());
        // the sun crosses the sky from morning (east, +x) to evening (west); at night a weak moon casts from above
        let (sun_dir, sun_strength) = sun_of(world.tick, self.weather_amount, world.settings.season_days as u64);
        // (the sun's view, and the middle of what the camera sees in it; the shadow boxes are settled once the
        // chunks in view are known)
        let (sun_view, box_want, box_mid) = {
            let (_, _, _, _, vm) = visible_chunks(&cam, self.aspect(), world);
            let (cx, cz) = ((vm.0 + vm.2) * 0.5, (vm.1 + vm.3) * 0.5);
            // the box grows and shrinks in 8 m steps: a size changing every frame changes the shadow texel size
            let r = ((0.5 * ((vm.2 - vm.0).powi(2) + (vm.3 - vm.1).powi(2)).sqrt() / 8.0).ceil() * 8.0).clamp(16.0, 128.0);
            let center = glam::Vec3::new(cx, gh(cx, cz), cz);
            let now_dir = glam::Vec3::from(sun_dir);
            if glam::Vec3::from(self.shadow_dir).dot(now_dir) < 0.99995 {
                self.shadow_dir = sun_dir;
            }
            let d = glam::Vec3::from(self.shadow_dir);
            let up = if d.y.abs() > 0.97 { glam::Vec3::Z } else { glam::Vec3::Y };
            // the sun's view is fixed in the world; only the box moves, in whole shadow texels, so the shadows do not
            // crawl as the camera slides
            let view = glam::camera::rh::view::look_to_mat4(glam::Vec3::ZERO, -d, up);
            (view, r, view.transform_point3(center))
        };
        g[48..51].copy_from_slice(&sun_dir);
        g[51] = sun_strength;
        // clouds: more of them in bad weather; they drift with the wind
        let clock = world.tick as f32 / 60.0;
        g[52] = 0.35 + 0.55 * self.weather_amount;
        // the one wind of the world: everything that moves in the air or on the water goes its way
        let (heading, strength) = alaz_sim::time::wind(world.seed, world.tick, world.settings.season_days as u64);
        let (wa, ws) = (heading as f32 / 1000.0, strength as f32 / 1000.0);
        let wind = [wa.cos() * ws, wa.sin() * ws, ws];
        let drift = 0.004 + 0.02 * ws;
        let cdt = self.last_frame.elapsed().as_secs_f32().min(0.1);
        self.cloud_off[0] -= wa.cos() * drift * cdt;
        self.cloud_off[1] -= wa.sin() * drift * cdt;
        g[53] = self.cloud_off[0];
        g[54] = self.cloud_off[1];
        g[55] = wa;
        // the swell's six wave trains: each one's phase at the height window's corner, in double precision
        {
            const DIRS: [f64; 6] = [0.0, 63.0, 118.0, 182.0, 238.0, 301.0];
            const LENS: [f64; 6] = [14.0, 11.0, 8.5, 6.5, 5.3, 3.1];
            let t = world.tick as f64 / 60.0 + self.sim_alpha.unwrap_or(0.0) as f64 / 60.0;
            for (i, &slot) in WAVE_SLOTS.iter().enumerate() {
                let a = DIRS[i].to_radians();
                let k = std::f64::consts::TAU / LENS[i];
                let w = (9.8 * k).sqrt();
                let ph = k * (a.cos() * hx as f64 + a.sin() * hy as f64) - w * t + i as f64 * 1.7;
                g[slot] = ph.rem_euclid(std::f64::consts::TAU) as f32;
            }
        }
        let _ = clock;
        // The camera and both render paths use the same player position.
        let player_center = player.map(|a| {
            let p = world.agent_walk_position(a);
            let v = glam::Vec3::new(p.x_mm as f32 / 1000.0, p.z_mm as f32 / 1000.0 + 0.9, p.y_mm as f32 / 1000.0);
            g[56..60].copy_from_slice(&[v.x, v.y, v.z, 1.0]);
            v
        });
        // a plant the pointer is on, under a big crown, shows through it
        if let Some((tx, ty)) = self.hover_node
            && let Some(n) = world.node_at(tx, ty).map(|id| &world.nodes[id.0 as usize]).filter(|n| n.kind != alaz_sim::NodeKind::Tree) {
                let (x, z) = (n.tx as f32 + 0.5, n.ty as f32 + 0.5);
                g[LOOK_SLOT..LOOK_SLOT + 4].copy_from_slice(&[x, gh(x, z) + 0.4, z, 1.0]);
            }
        let torch = cave.is_some_and(|c| c.light > 0) as u8 as f32;
        self.cave_light += (torch - self.cave_light) * (1.0 - (-6.0 * cdt).exp());
        g[CAVE_SLOT] = self.cave_light;
        let (pitch, yaw) = (cam.pitch_deg.to_radians(), cam.yaw_deg.to_radians());
        let forward = -glam::Vec3::new(pitch.cos() * yaw.sin(), pitch.sin(), pitch.cos() * yaw.cos());
        let right = glam::Vec3::new(yaw.cos(), 0.0, -yaw.sin());
        let up = right.cross(forward);
        g[60..63].copy_from_slice(&right.to_array());
        g[64..67].copy_from_slice(&up.to_array());
        self.surface_cutaway = player_center
            .filter(|v| cave.is_some() && gh(v.x, v.z) > v.y - 0.35)
            .map(|center| SurfaceCutaway { center, right, up });
        g[CAVE_SLOT + 1] = self.surface_cutaway.is_some() as u8 as f32;
        // fires light their surroundings: the eight burning ones nearest the camera (xyz, w = reach in metres)
        {
            let mut fires: Vec<(f32, [f32; 4])> = world
                .buildings
                .iter()
                .filter(|b| b.active())
                .filter_map(|b| {
                    let reach = match b.kind {
                        // a fire well fed throws its light further than one burning low
                        BuildingKind::Campfire => 5.0 + 4.0 * (b.fuel as f32 / alaz_sim::world::FIRE_FUEL_MAX as f32).min(1.0),
                        BuildingKind::CharcoalKiln | BuildingKind::Furnace => 5.0,
                        _ => return None,
                    };
                    let (x, z) = (b.x as f32 / 1000.0, b.y as f32 / 1000.0);
                    let d = (x - cam.target.x).powi(2) + (z - cam.target.z).powi(2);
                    Some((d, [x, ground_height(world.seed, x, z) + 0.45, z, reach]))
                })
                .collect();
            // grass fires light the ground too: the burning tiles near the camera in clumps of 4x4 tiles, each clump a light
            // (the camera sees far less than 90 m of flames at a time)
            if world.burning().next().is_some() {
                let mut clumps: std::collections::BTreeMap<(i32, i32), (f32, f32, u32)> = Default::default();
                for (tx, ty) in world.burning() {
                    let (x, z) = (tx as f32 + 0.5, ty as f32 + 0.5);
                    if (x - cam.target.x).abs() < 90.0 && (z - cam.target.z).abs() < 90.0 {
                        let c = clumps.entry((tx.div_euclid(4), ty.div_euclid(4))).or_default();
                        *c = (c.0 + x, c.1 + z, c.2 + 1);
                    }
                }
                for (_, (sx, sz, n)) in clumps {
                    let (x, z) = (sx / n as f32, sz / n as f32);
                    let d = (x - cam.target.x).powi(2) + (z - cam.target.z).powi(2);
                    fires.push((d, [x, ground_height(world.seed, x, z) + 0.5, z, 2.5 + 0.7 * (n as f32).sqrt()]));
                }
            }
            fires.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (i, (_, f)) in fires.iter().take(8).enumerate() {
                g[68 + i * 4..72 + i * 4].copy_from_slice(f);
            }
            // how many there are, so the shader's light loop stops there (most of the time there are none)
            g[100] = fires.len().min(8) as f32;
        }
        // (the shader looks for the surface layer only while it holds something)
        g[103] = self.surf_on as u8 as f32;
        g[16..19].copy_from_slice(&tint);
        g[19] = world.perception_permille() as f32 / 1000.0;
        g[20] = cam.target.x;
        g[21] = cam.target.z;
        g[24] = hx as f32;
        g[25] = hy as f32;
        let storm = if cave.is_some() { 0.0 } else if self.weather_kind < 1.5 { self.weather_amount } else { 0.0 };
        let snowing = if cave.is_some() { 0.0 } else if self.weather_kind > 1.5 { self.weather_amount } else { 0.0 };
        let winter = (world.season() == 3 && cave.is_none()) as u8 as f32 * 0.22;
        g[28] = world.tick as f32 / 60.0;
        g[29] = 0.1 + 1.8 * alaz_sim::time::wind(world.seed, world.tick, world.settings.season_days as u64).1 as f32 / 1000.0;
        self.wind = g[29];
        g[30] = storm;
        g[31] = (snowing * 0.8).max(winter);
        self.queue.write_buffer(&self.globals, 0, bytemuck::cast_slice(&g));
        {
            // the window shows the middle of the picture, shifted by the sub-texel camera remainder (screen px)
            let k = scale;
            let crop_x = (sw as f32 * scale - win_w as f32).max(0.0).div_euclid(2.0);
            let crop_y = (sh as f32 * scale - win_h as f32).max(0.0).div_euclid(2.0);
            // rain and snow slant with the wind as it crosses the screen
            let [wx, wz, _] = wind;
            let yaw = cam.yaw_deg.to_radians();
            let slant = wx * yaw.cos() - wz * yaw.sin();
            let p = [k, 0.0, crop_x + off[0] * k, crop_y - off[1] * k, win_w as f32, win_h as f32, t, self.pixel.cinema, self.weather_kind, self.weather_amount, world.tick as f32 / 60.0, slant];
            self.queue.write_buffer(&self.pixel.uniform, 0, bytemuck::cast_slice(&p));
        }

        // --- scene geometry: only chunks the camera can see are drawn. Meshes that are missing or stale are built on
        // worker threads beside the rest of the frame (the old mesh keeps showing meanwhile); the chunks one ring
        // beyond the picture are built ahead, so ground arrives before it is seen.
        self.frame_no += 1;
        let mut visible: Vec<(i32, i32)> = Vec::new();
        for cy in cy0..=cy1 {
            for cx in cx0..=cx1 {
                visible.push((cx, cy));
            }
        }
        let first_frame = self.chunks.is_empty();
        let mut wanted: Vec<((i32, i32), bool, bool)> = Vec::new();
        {
            let max = |n: i32| (n - 1) / CHUNK_TILES;
            let stamp = |key: (i32, i32)| world.node_stamp(key.0 * CHUNK_TILES, key.1 * CHUNK_TILES);
            let far_of = |k: (i32, i32)| {
                let (dx, dz) = ((k.0 * CHUNK_TILES + CHUNK_TILES / 2) as f32 - cam.target.x, (k.1 * CHUNK_TILES + CHUNK_TILES / 2) as f32 - cam.target.z);
                dx * dx + dz * dz > FAR_TREE_TILES * FAR_TREE_TILES
            };
            let fresh = |c: &Chunk, key: (i32, i32)| c.terrain.is_some() && c.far == far_of(key) && c.nodes_version == stamp(key) && c.seen == world.seen_bits() && c.level == world.perception_level();
            let mut ring = Vec::new();
            for cy in (cy0 - 1).max(0)..=(cy1 + 1).min(max(world.map.h)) {
                for cx in (cx0 - 1).max(0)..=(cx1 + 1).min(max(world.map.w)) {
                    if cx < cx0 || cx > cx1 || cy < cy0 || cy > cy1 {
                        ring.push((cx, cy));
                    }
                }
            }
            let near = |k: &(i32, i32)| {
                let (dx, dz) = ((k.0 * CHUNK_TILES + CHUNK_TILES / 2) as f32 - cam.target.x, (k.1 * CHUNK_TILES + CHUNK_TILES / 2) as f32 - cam.target.z);
                (dx * dx + dz * dz) as i64
            };
            // (missing ground first, then stale models, then what lies ahead; nearest first within each)
            let mut missing: Vec<(i32, i32)> = visible.iter().copied().filter(|k| self.chunks.get(k).is_none_or(|c| c.terrain.is_none())).collect();
            let mut stale: Vec<(i32, i32)> = visible.iter().copied().filter(|k| self.chunks.get(k).is_some_and(|c| c.terrain.is_some() && !fresh(c, *k))).collect();
            let mut ahead: Vec<(i32, i32)> = ring.into_iter().filter(|k| self.chunks.get(k).is_none_or(|c| !fresh(c, *k))).collect();
            for l in [&mut missing, &mut stale, &mut ahead] {
                l.sort_by_key(near);
            }
            // the first picture waits for everything it shows; later frames take a few chunks each
            let cap = if first_frame { usize::MAX } else { MAX_BUILDS_PER_FRAME };
            for k in missing.into_iter().chain(stale).chain(ahead).take(cap) {
                wanted.push((k, self.chunks.get(&k).is_none_or(|c| c.terrain.is_none()), far_of(k)));
            }
        }
        for key in &visible {
            if let Some(c) = self.chunks.get_mut(key) {
                c.used = self.frame_no;
            }
        }
        // a chunk's meshes, once built, replace the old ones
        let mut build_ms = 0.0;
        let mut built = 0;
        macro_rules! take_built {
            ($b:expr) => {{
                let b: BuiltChunk = $b;
                let old = self.chunks.remove(&b.key);
                self.chunk_gen += 1;
                let terrain = b.terrain.or_else(|| old.and_then(|c| c.terrain));
                self.chunks.insert(b.key, Chunk { terrain, nodes: b.nodes, decor: b.decor, nodes_version: b.nodes_version, seen: b.seen, level: b.level, far: b.far, used: self.frame_no, build: self.chunk_gen });
                built += 1;
            }};
        }
        let (device, art_ref) = (self.device.clone(), Arc::clone(&self.art));
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for &(key, with_terrain, far) in &wanted {
                handles.push({
                let (device, art) = (device.clone(), Arc::clone(&art_ref));
                scope.spawn(move || build_chunk(&device, &art, world, key, with_terrain, far))
            });
            }
            if first_frame {
                let t = std::time::Instant::now();
                for h in handles.drain(..) {
                    take_built!(h.join().expect("chunk worker"));
                }
                build_ms += t.elapsed().as_secs_f64() * 1000.0;
            }
            if self.chunks.len() > MAX_CACHED_CHUNKS {
                let frame = self.frame_no;
                let mut old: Vec<((i32, i32), u64)> = self.chunks.iter().map(|(k, c)| (*k, c.used)).filter(|(_, u)| *u != frame).collect();
                old.sort_by_key(|(_, u)| *u);
                let drop = self.chunks.len() - MAX_CACHED_CHUNKS * 3 / 4;
                for (k, _) in old.into_iter().take(drop) {
                    self.chunks.remove(&k);
                }
            }
            // --- the shadow boxes. The static map holds what stands still, over a box 1.5 times as wide as the picture's with
            // the same texel size; it is redrawn only when the sun stepped, the box size changed, the camera nears the edge
            // of its margin, or a chunk's models changed. Each frame the window of it that the picture needs is copied
            // into the dynamic map (same texels, same depth range), and what moves is drawn over that.
            let shadows_on = self.quality.shadows != crate::ShadowQuality::Off;
            let redraw_static = {
                let st = &mut self.shadow_static;
                // (a size one step larger than wanted is kept: the box does not flicker between two sizes)
                let r = if st.valid && st.radius >= box_want && st.radius <= box_want + 8.0 { st.radius } else { box_want };
                let texel = r / (self.shadow_res / 2) as f32;
                let tex = [(box_mid.x / texel).round() as i64, (box_mid.y / texel).round() as i64];
                let half_margin = ((self.shadow_static_res - self.shadow_res) / 2) as i64;
                let mut redraw = !st.valid || st.every_frame || st.dir != self.shadow_dir || st.radius != r;
                if !redraw {
                    let (ox, oy) = (tex[0] - st.tex[0], tex[1] - st.tex[1]);
                    redraw = ox.abs() > half_margin || oy.abs() > half_margin || (box_mid.z - st.z).abs() > 120.0;
                }
                // models changed or newly built (at most every few frames: chunks arrive one after another while flying)
                if !redraw && self.frame_no - st.last_draw >= 4 {
                    redraw = visible.iter().any(|k| match (self.chunks.get(k), st.drawn.get(k)) {
                        (Some(c), Some(&(g, had))) => g != c.build && (had || c.nodes.is_some()),
                        (Some(c), None) => c.nodes.is_some(),
                        _ => false,
                    });
                }
                if redraw {
                    let half = (self.shadow_static_res / 2) as f32 * texel;
                    let (sx, sy) = (tex[0] as f32 * texel, tex[1] as f32 * texel);
                    let proj = glam::camera::rh::proj::directx::orthographic(sx - half, sx + half, sy - half, sy + half, -box_mid.z - 250.0, -box_mid.z + 250.0);
                    st.vp = proj * sun_view;
                    st.dir = self.shadow_dir;
                    st.radius = r;
                    st.tex = tex;
                    st.z = box_mid.z;
                    st.valid = true;
                    st.last_draw = self.frame_no;
                    st.drawn = visible.iter().filter_map(|k| self.chunks.get(k).map(|c| (*k, (c.build, c.nodes.is_some())))).collect();
                }
                // the dynamic box: the picture's, in whole texels, sharing the static map's depth range
                let (dx, dy) = (tex[0] as f32 * texel, tex[1] as f32 * texel);
                let dproj = glam::camera::rh::proj::directx::orthographic(dx - r, dx + r, dy - r, dy + r, -st.z - 250.0, -st.z + 250.0);
                self.queue.write_buffer(&self.globals, 32 * 4, bytemuck::cast_slice(&(dproj * sun_view).to_cols_array()));
                self.queue.write_buffer(&self.globals, 104 * 4, bytemuck::cast_slice(&st.vp.to_cols_array()));
                // where the picture's window starts in the static map (y runs the other way in texture rows)
                let (ox, oy) = (tex[0] - st.tex[0], tex[1] - st.tex[1]);
                self.queue.write_buffer(&self.globals, 101 * 4, bytemuck::cast_slice(&[(half_margin + ox) as f32, (half_margin - oy) as f32]));
                // (with the shadows off nothing is drawn: the shader never looks at the maps)
                redraw && shadows_on
            };
            // the ground's static colour: bake a few blocks of the chunks in view (and just beyond) this frame
            let jobs = {
                let max = |n: i32| (n - 1) / CHUNK_TILES;
                let mut near = Vec::new();
                for cy in (cy0 - 1).max(0)..=(cy1 + 1).min(max(world.map.h)) {
                    for cx in (cx0 - 1).max(0)..=(cx1 + 1).min(max(world.map.w)) {
                        near.push((cx, cy));
                    }
                }
                self.bake.plan(world.seed, (hx, hy), &visible, &near, (cam.target.x, cam.target.z))
            };
            let mut dynamic = Mesh::new();
            dynamic.wind = wind;
            let mut me_body = 0u32..0;
            let art = &self.art;
            let tick = world.tick;
            let secs = tick as f32 / 60.0;
            let inside = |x_m: f32, z_m: f32| x_m >= view_m.0 && x_m <= view_m.2 && z_m >= view_m.1 && z_m <= view_m.3;
            if let Some(tile) = self.hover_node {
                push_hover_rim(&mut dynamic, world, tile, world.perception_level(), &cam, secs, TreeDetail::Full);
            }
            // the grid is worked out once a frame: grid machines stand still on a dead line
            let live = world.electric_live();
            for (bi, b) in world.buildings.iter().enumerate().filter(|(_, b)| inside(b.x as f32 / 1000.0, b.y as f32 / 1000.0)) {
                let (bx, bz) = (b.x as f32 / 1000.0, b.y as f32 / 1000.0);
                decal(&mut dynamic, art.shadow_l, bx + 0.2, bz + 0.15, WHITE, 0.0);
                if b.kind == BuildingKind::Campfire {
                    push_campfire(&mut dynamic, bx, bz, b.fuel as f32 / alaz_sim::world::FIRE_FUEL_MAX as f32, secs, None);
                } else {
                    let start = dynamic.len();
                    // a machine with a job but no power (or steam, or blast) stands still instead of pretending to work
                    let working = world.working_with(bi, &live);
                    push_building(&mut dynamic, b.kind, bx, bz, working, tick, None);
                    turn_quarters(&mut dynamic, start, bx, bz, b.facing);
                }
            }
            // grass fires: flames over the burning tiles in view (the nearest forty)
            if world.burning().next().is_some() {
                let mut near: Vec<(f32, i32, i32)> = world
                    .burning()
                    .filter(|&(tx, ty)| inside(tx as f32 + 0.5, ty as f32 + 0.5))
                    .map(|(tx, ty)| ((tx as f32 + 0.5 - cam.target.x).powi(2) + (ty as f32 + 0.5 - cam.target.z).powi(2), tx, ty))
                    .collect();
                near.sort_by(|a, b| a.0.total_cmp(&b.0));
                let ground = world.ground();
                for &(_, tx, ty) in near.iter().take(40) {
                    let age = (world.surface_code(&ground, tx, ty) >> 3) as f32;
                    push_grass_fire(&mut dynamic, tx as f32 + 0.5, ty as f32 + 0.5, age, secs);
                }
            }
            if let Some(g) = ghost {
                let tint = if g.valid { [0.35, 1.0, 0.45] } else { [1.0, 0.25, 0.25] };
                let r = rules().building(g.kind).radius_mm as f32 / 1000.0;
                dynamic.set_anchor(g.x_m, g.z_m);
                push_box(&mut dynamic, [g.x_m, 0.03, g.z_m], [r, 0.02, r], tint, 1.0);
                let start = dynamic.len();
                push_building(&mut dynamic, g.kind, g.x_m, g.z_m, false, tick, Some(tint));
                turn_quarters(&mut dynamic, start, g.x_m, g.z_m, g.facing);
            }
            let mm = |v: f32| (v * 1000.0) as i64;
            let now = std::time::Instant::now();
            let dt = (now - self.last_frame).as_secs_f32().min(0.1);
            self.last_frame = now;
            // how far drawing is between the last simulated tick and the next one
            if world.tick != self.tick_seen {
                let gap = (now - self.tick_at).as_secs_f32();
                if gap < 0.25 {
                    self.tick_gap += (gap - self.tick_gap) * 0.2;
                }
                self.tick_seen = world.tick;
                self.tick_at = now;
            }
            let alpha = self.sim_alpha.unwrap_or_else(|| ((now - self.tick_at).as_secs_f32() / self.tick_gap.max(1e-3)).clamp(0.0, 1.0));
            push_bodies(&mut dynamic, world, alpha, &inside);
            push_fish(&mut dynamic, world, view_m, (world.tick as f32 + alpha) / 60.0, alpha);
            let tracks_visible = world.perception_level() >= 2;
            for w in world.wild_in(mm(view_m.0), mm(view_m.1), mm(view_m.2), mm(view_m.3)) {
                let (wx, wz) = (w.x as f32 / 1000.0, w.y as f32 / 1000.0);
                if tracks_visible {
                    let steps = self.wild_track_steps.entry(w.id).or_default();
                    let wolf = w.species == alaz_sim::WildSpecies::Wolf;
                    steps.advance(glam::Vec2::new(wx, wz), self.frame_no, |at| {
                        if self.wild_tracks.len() >= MAX_TRACK_MARKS {
                            self.wild_tracks.pop_front();
                        }
                        self.wild_tracks.push_back(TrackMark { at, tick, wolf });
                    });
                }
                // wild animals walk like everyone else: they face where they go and their legs follow the ground covered
                let st = self.wild_anim.entry(w.id).or_insert(Walk::at(wx, wz, (w.id as f32 * 2.399) % std::f32::consts::TAU, 0));
                let (wx, wz) = st.smooth(wx, wz, world.tick, alpha);
                step_walk(st, wx, wz, dt, quad_stride(wild_leg(w.species)));
                let idle = Pose { heading: st.heading, phase: st.phase, amp: st.amp, id: w.id, stride: st.stride, ..Pose::idle(secs) };
                decal(&mut dynamic, art.shadow_m, wx, wz, WHITE, 0.0);
                push_wild(&mut dynamic, w.species, wx, wz, idle);
            }
            // killed animals fall over where they stood, lie a while and sink away
            self.falls.retain(|f| (now - f.at).as_secs_f32() < DEATH_SECS);
            if let Some(s) = self.demo_fall.filter(|_| self.falls.is_empty()) {
                let (x, z) = (cam.target.x + 1.5, cam.target.z + 1.5);
                self.falls.push(Fall { id: 7, species: s, x, z, heading: 0.9, stride: quad_stride(wild_leg(s)), side: 1.0, at: now });
            }
            for f in &self.falls {
                if f.x < view_m.0 || f.x > view_m.2 || f.z < view_m.1 || f.z > view_m.3 {
                    continue;
                }
                let age = (now - f.at).as_secs_f32();
                // legs stiff, head and tail still (a fixed clock), a flash as the blow lands
                let pose = Pose { heading: f.heading, id: f.id, stride: f.stride, hurt: (1.0 - age * 2.5).max(0.0), ..Pose::idle(f.id as f32 * 0.37) };
                let (tip, sink) = death_pose(f.species, age);
                let start = dynamic.len();
                push_wild(&mut dynamic, f.species, f.x, f.z, pose);
                fall_over(&mut dynamic, start, f.species, f.x, f.z, f.heading, f.side, tip, sink);
            }
            // The stray dog that came to the fire waits for a friend: a soft ring marks it until it is tamed.
            if world.milestone_done(alaz_sim::Milestone::Visitor) && !world.milestone_done(alaz_sim::Milestone::Companion) {
                let pulse = 0.5 + 0.5 * (tick as f32 / 20.0).sin();
                for w in world.wild_in(mm(view_m.0), mm(view_m.1), mm(view_m.2), mm(view_m.3)).into_iter().filter(|w| w.species == alaz_sim::WildSpecies::Dog) {
                    let (wx, wz) = (w.x as f32 / 1000.0, w.y as f32 / 1000.0);
                    let r = 1.1 + 0.15 * pulse;
                    for k in 0..16 {
                        let (a0, a1) = (k as f32 * std::f32::consts::TAU / 16.0, (k as f32 + 0.6) * std::f32::consts::TAU / 16.0);
                        push_segment(&mut dynamic, [wx + r * a0.cos(), wz + r * a0.sin()], [wx + r * a1.cos(), wz + r * a1.sin()], 0.07, 0.04, [0.95, 0.85, 0.45], 1.0);
                    }
                }
            }
            while self.wild_tracks.front().is_some_and(|m| tick.saturating_sub(m.tick) >= TRACK_LIFETIME_TICKS) {
                self.wild_tracks.pop_front();
            }
            // A mark uses its own ground height and stays visible when its animal leaves the view.
            if tracks_visible {
                for mark in &self.wild_tracks {
                    let (x, z) = (mark.at.x, mark.at.y);
                    if inside(x, z) {
                        dynamic.set_anchor(x, z);
                        push_paw_mark(&mut dynamic, x, z, mark.wolf);
                    }
                }
            }
            if self.wild_anim.len() > 512 || self.wild_track_steps.len() > 512 {
                let live: std::collections::HashSet<u32> = world.wild_in(mm(view_m.0), mm(view_m.1), mm(view_m.2), mm(view_m.3)).iter().map(|w| w.id).collect();
                self.wild_anim.retain(|k, _| live.contains(k));
                self.wild_track_steps.retain(|k, _| live.contains(k));
            }
            // a person a few dozen pixels tall (zoomed far out) is drawn in a simpler build; the player never is
            let lod_humans = human_lod(&cam);
            // how evolved the people look: the caller's smoothed value, or the world's perception as it stands
            let evo = self.evo.unwrap_or_else(|| crate::evo_pos(world.perception_permille() as f32));
            let mut body_jobs: Vec<BodyJob> = Vec::with_capacity(world.agents.len());
            for a in &world.agents {
                let c = a.cave_view();
                let layer = c.map(|c| {
                    let (x, y) = c.tile();
                    CaveTile { x, y, z: c.elevation_m() }
                });
                let position = world.agent_walk_position(a);
                let (x, z) = (position.x_mm as f32 / 1000.0, position.y_mm as f32 / 1000.0);
                if a.id != me && (!inside(x, z) || layer.is_some_and(|t| !cave_floors.as_ref().is_some_and(|f| f.contains_key(&t)))) {
                    continue;
                }
                let grounded_at = position.z_mm as f32 / 1000.0;
                let cave_exposure = layer.map(|t| cave_daylight.as_ref().and_then(|light| light.get(&t)).copied().unwrap_or(0.0));
                let custom_floor = layer.is_some() || (grounded_at - gh(x, z)).abs() > 0.015 && world.airborne(a.id).is_none();
                let floor_height = custom_floor.then_some(grounded_at);
                let w = self.anim.entry(a.id.0).or_insert(Walk::at(x, z, 0.0, a.health));
                let (x, z) = w.smooth(x, z, world.tick, alpha);
                // a blow makes the body flinch and flash
                if w.hp - a.health >= 150 {
                    w.hurt = 1.0;
                }
                w.hp = a.health;
                w.hurt = (w.hurt - dt * 2.4).max(0.0);
                step_walk(w, x, z, dt, match a.kind {
                    AgentKind::Human => 0.3,
                    AgentKind::Dog => quad_stride(0.22),
                    AgentKind::Ox => quad_stride(0.45),
                    AgentKind::Horse => quad_stride(0.7),
                    AgentKind::Bot => 0.25,
                });
                let (work, tool, target) = work_of(world, a);
                if work != Work::None && w.amp < 0.3 {
                    // turn toward what we are working on
                    let want = (target.0 - x).atan2(target.1 - z);
                    w.heading += wrap_pi(want - w.heading) * (1.0 - (-10.0 * dt).exp());
                }
                let working = work != Work::None && w.amp < 0.3;
                let cyc = work_cycle(world, a, work, w, alpha, secs, dt);
                            let outfit = outfit_bits(&a.inventory);
                let slope = if layer.is_some() {
                    0.0
                } else {
                    let (sx, sz) = (w.heading.sin() * 0.6, w.heading.cos() * 0.6);
                    (gh(x + sx, z + sz) - gh(x - sx, z - sz)) / 1.2
                };
                let pose = Pose { heading: w.heading, phase: w.phase, amp: w.amp, t: secs, work: if working { work } else { Work::None }, tool, outfit, id: a.id.0, evo, slope, hurt: w.hurt, stride: w.stride, cyc, air: 0.0, lod: lod_humans && a.id != me, sit: a.is_seated() };
                // off the ground: the body rides up the flight, tucking its legs on the way up and flailing on the way down
                let flight = world.airborne(a.id).map(|f| {
                    let q = alaz_sim::physics::Q as f32;
                    let z_m = (f.z as f32 + f.vz as f32 * alpha) / q / 1000.0;
                    (z_m - gh(x, z), (f.vz as f32 / q / 60.0).clamp(-1.0, 1.0))
                });
                let pose = match flight {
                    Some((_, v)) => Pose { air: if v.abs() < 0.05 { 0.05 } else { v }, amp: 0.0, work: Work::None, ..pose },
                    None => pose,
                };
                let depth = match c {
                    Some(c) => {
                        let (x, y) = c.tile();
                        2 * world.natural_cave_flooded(alaz_sim::cave_terrain::CaveTile { x, y, z: c.elevation_m() }) as u8
                    }
                    _ => alaz_sim::map::water_depth(&world.map, a.x.div_euclid(1000) as i32, a.y.div_euclid(1000) as i32),
                };
                // The held light uses the same support altitude as the body.
                if c.is_some_and(|c| c.light > 0) {
                    let (h, ts) = (w.heading, secs);
                    let first = dynamic.len();
                    push_torch(&mut dynamic, x + h.cos() * 0.3 + h.sin() * 0.12, z - h.sin() * 0.3 + h.cos() * 0.12, ts, 1.0);
                    for v in &mut dynamic.verts[first..] {
                        v.kind = 1000.0 + grounded_at;
                        v.uv[0] = -1.0 - 0.99 * cave_exposure.unwrap_or(0.0);
                    }
                }
                // the body leans into a turn and a little forward when it hurries (not when swimming or in the air)
                let lean = if a.kind == AgentKind::Human && flight.is_none() && depth == 0 {
                    Some(((-w.spin * w.speed * 0.07).clamp(-0.22, 0.22), (w.speed * 0.035).clamp(0.0, 0.11)))
                } else {
                    None
                };
                let wave = match depth {
                    1 if flight.is_none() => swell_height(world, x, z, alpha) * 0.5,
                    2 if flight.is_none() => swell_height(world, x, z, alpha),
                    _ => 0.0,
                };
                body_jobs.push(BodyJob {
                    kind: a.kind,
                    x,
                    z,
                    marked: a.kind.is_worker() || (a.kind == AgentKind::Human && a.id != me),
                    is_me: a.id == me,
                    pose,
                    chips: if working { Some((work, target, cyc)) } else { None },
                    lift: flight.map(|(l, _)| l),
                    floor_height,
                    cave_exposure,
                    lean,
                    heading: w.heading,
                    depth: if flight.is_none() { depth } else { 0 },
                    wave,
                    secs,
                    seed: a.id.0,
                });
            }
            if let (Some((outfit, mode)), Some(me_job)) = (self.lineup.as_ref(), body_jobs.iter().find(|j| j.is_me)) {
                // a row across the screen, facing the camera, a few metres in front of the player
                let yaw = cam.yaw_deg.to_radians();
                let (rx, rz) = (yaw.cos(), -yaw.sin());
                let (cx, cz) = (me_job.x + yaw.sin() * 2.6, me_job.z + yaw.cos() * 2.6);
                for i in 0..EVO_STAGES {
                    let o = (i as f32 - (EVO_STAGES - 1) as f32 * 0.5) * 1.12;
                    let (x, z) = (cx + rx * o, cz + rz * o);
                    let f = i as f32;
                    let walk = mode == "walk";
                    let pose = Pose { heading: yaw, phase: secs * 5.0 + f * 0.7, amp: if walk { 1.0 } else { 0.0 }, t: secs, outfit: *outfit, id: self.lineup_id, evo: f, slope: 0.0, lod: mode == "lod", sit: mode == "sit", ..Pose::idle(secs) };
                    body_jobs.push(BodyJob { kind: AgentKind::Human, x, z, marked: false, is_me: false, pose, chips: None, lift: None, floor_height: None, cave_exposure: None, lean: None, heading: yaw, depth: 0, wave: 0.0, secs, seed: i as u32 });
                }
            }
            let (body_mesh, me_range) = build_bodies(&body_jobs, art, wind);
            let base = dynamic.len() as u32;
            dynamic.extend_from_slice(&body_mesh);
            if let Some(r) = me_range {
                me_body = base + r.start..base + r.end;
            }
            if self.anim.len() > world.agents.len() * 2 + 8 {
                let live: std::collections::HashSet<u32> = world.agents.iter().map(|a| a.id.0).collect();
                self.anim.retain(|k, _| live.contains(k));
            }
            for &i in routes.iter().filter(|_| cave.is_none()) {
                if let Some(a) = world.agents.get(i) {
                    push_route(&mut dynamic, world, a, tick);
                    // a resource being gone for glows itself, as under the pointer, instead of carrying a flag
                    if let alaz_sim::world::Task::Gather { node, .. } = a.task
                        && let Some(n) = world.nodes.get(node.0 as usize) {
                            push_hover_rim(&mut dynamic, world, (n.tx, n.ty), world.perception_level(), &cam, secs, TreeDetail::Full);
                        }
                }
            }
            if let Some(v) = &self.view {
                // (nothing but the model: the one thing the artist is looking at)
                dynamic = Mesh::new();
                dynamic.wind = wind;
                me_body = 0..0;
                v.push(&mut dynamic, world.tick);
                if dynamic.is_empty() {
                    // (an empty buffer cannot be bound: one triangle with no area)
                    dynamic.extend_from_slice(&[bytemuck::Zeroable::zeroed(); 3]);
                }
            }
            let dyn_buf = self.vertex_buffer(&dynamic);

            let t_scene = std::time::Instant::now();
            let mut enc = self.device.create_command_encoder(&Default::default());
            self.bake.encode(&mut enc, &self.queue, &self.bake_pipeline, &jobs, self.prof.as_ref().and_then(|p| p.writes(7)));
            if redraw_static {
                // what stands still, seen from the sun: the chunks' models (the decor is left out: grass and flowers are
                // thinner than a shadow texel)
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("sun shadow, static"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.shadow_static_view,
                        depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: self.prof.as_ref().and_then(|p| p.writes(8)),
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&self.shadow_static_pipeline);
                pass.set_bind_group(0, Some(&self.shadow_bind_group), &[]);
                for key in visible.iter().filter(|_| self.view.is_none()) {
                    let Some(c) = self.chunks.get(key) else { continue };
                    if let Some((b, n)) = &c.nodes {
                        pass.set_vertex_buffer(0, b.slice(..));
                        pass.draw(0..*n, 0..1);
                    }
                }
            }
            if let Some(p) = &mut self.prof {
                p.static_ran = redraw_static;
                p.static_draws += redraw_static as u32;
            }
            if shadows_on {
                // what the sun sees of what moves, over the copied static shadow: people, animals, buildings, the ghost
                // (the ground and flat decals just receive)
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("sun shadow"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.shadow_view,
                        depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: self.prof.as_ref().and_then(|p| p.writes(0)),
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                // first the static map's window for this box, as the starting depth
                pass.set_pipeline(&self.shadow_copy_pipeline);
                pass.set_bind_group(0, Some(&self.shadow_copy_group), &[]);
                pass.draw(0..3, 0..1);
                pass.set_pipeline(&self.shadow_pipeline);
                pass.set_bind_group(0, Some(&self.shadow_bind_group), &[]);
                pass.set_vertex_buffer(0, dyn_buf.slice(..));
                pass.draw(0..dynamic.len() as u32, 0..1);
            }
            // the scene: the chunks' models first, then the ground, then what moves. Models first: the ground's shading
            // is the dearest in the game, and where a tree or rock already stands the depth test now skips it. One pass
            // normally; three when profiling, so each part gets its own GPU time (0 ground, 1 models, 2 what moves).
            // zoomed all the way out, grass tufts and flowers are a texel or two of noise over far more ground: they are
            // left out there (a third of the scene's vertices)
            let decor_shown = self.quality.decor && (cam.zoom * 3.0).round() > 2.0;
            let parts: &[&[usize]] = if self.prof.is_some() { &[&[1], &[0], &[2]] } else { &[&[1, 0, 2]] };
            let mut verts = 0u64;
            for (pi, &part) in parts.iter().enumerate() {
                let first = pi == 0;
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.scene,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: if first { wgpu::LoadOp::Clear(wgpu::Color { r: 0.01 * tint[0] as f64, g: 0.012 * tint[1] as f64, b: 0.02 * tint[2] as f64, a: 1.0 }) } else { wgpu::LoadOp::Load },
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth,
                        depth_ops: Some(wgpu::Operations { load: if first { wgpu::LoadOp::Clear(1.0) } else { wgpu::LoadOp::Load }, store: wgpu::StoreOp::Store }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: self.prof.as_ref().and_then(|p| p.writes(1 + part[0])),
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                // the picture is drawn into the top-left sw x sh texels of the scene texture
                pass.set_viewport(0.0, 0.0, sw as f32, sh as f32, 0.0, 1.0);
                pass.set_bind_group(0, Some(&self.bind_group), &[]);
                pass.set_pipeline(&self.tri_pipeline);
                for &what in part {
                    match what {
                        0 => {
                            // the ground: from the bake where a chunk's is complete, worked out live elsewhere
                            let mut baked = None;
                            for key in &visible {
                                let Some(c) = self.chunks.get(key) else { continue };
                                if let Some((b, n)) = &c.terrain {
                                    let ready = self.bake.ready(*key);
                                    if baked != Some(ready) {
                                        pass.set_pipeline(if ready { &self.ground_pipeline } else { &self.ground_live_pipeline });
                                        baked = Some(ready);
                                    }
                                    pass.set_vertex_buffer(0, b.slice(..));
                                    pass.draw(0..*n, 0..1);
                                    verts += *n as u64;
                                }
                            }
                        }
                        1 => {
                            pass.set_pipeline(&self.tri_pipeline);
                            for key in visible.iter().filter(|_| self.view.is_none()) {
                                let Some(c) = self.chunks.get(key) else { continue };
                                for (b, n) in c.nodes.iter().chain(c.decor.iter().filter(|_| decor_shown)) {
                                    pass.set_vertex_buffer(0, b.slice(..));
                                    pass.draw(0..*n, 0..1);
                                    verts += *n as u64;
                                }
                            }
                        }
                        _ => {
                            pass.set_pipeline(&self.tri_pipeline);
                            if let Some((buffer, count)) = natural_mesh.as_ref().filter(|_| self.view.is_none()) {
                                pass.set_vertex_buffer(0, buffer.slice(..));
                                pass.draw(0..*count, 0..1);
                                verts += u64::from(*count);
                            }
                            pass.set_vertex_buffer(0, dyn_buf.slice(..));
                            pass.draw(0..dynamic.len() as u32, 0..1);
                            if !me_body.is_empty() {
                                pass.set_pipeline(&self.mark_pipeline);
                                pass.draw(me_body.clone(), 0..1);
                                pass.set_pipeline(&self.hidden_pipeline);
                                pass.draw(me_body.clone(), 0..1);
                                pass.set_pipeline(&self.tri_pipeline);
                            }
                        }
                    }
                }
            }
            self.stats.chunks_drawn = visible.len() as u32;
            if self.prof.is_some() && self.frame_no.is_multiple_of(300) {
                eprintln!("[bake] chunks drawn {} from the bake {} (blocks baked so far {})", visible.len(), visible.iter().filter(|k| self.bake.ready(**k)).count(), self.bake.baked);
            }
            self.stats.vertices = verts + dynamic.len() as u64;
            {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("outline"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.inked,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: self.prof.as_ref().and_then(|p| p.writes(4)),
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_viewport(0.0, 0.0, sw as f32, sh as f32, 0.0, 1.0);
                pass.set_pipeline(&self.outline.pipeline);
                pass.set_bind_group(0, Some(&self.outline.group), &[]);
                pass.draw(0..3, 0..1);
            }
            {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("pixel"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: self.prof.as_ref().and_then(|p| p.writes(5)),
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&self.pixel.pipeline);
                pass.set_bind_group(0, Some(&self.pixel.group), &[]);
                pass.draw(0..3, 0..1);
            }
            // --- egui. The scene's commands go to the GPU first, and the interface is worked out while it draws: the GPU
            // would otherwise sit idle through this (about 2 ms of the CPU's time) before its first pass.
            self.queue.submit([enc.finish()]);
            let t_scene_sent = std::time::Instant::now();
            let raw = self.egui_state.take_egui_input(&self.window);
            let full = self.egui_ctx.run_pass(raw, |ctx| hud(ctx));
            self.egui_state.handle_platform_output(&self.window, full.platform_output);
            let prims = self.egui_ctx.tessellate(full.shapes, full.pixels_per_point);
            let mut tex = full.textures_delta;
            for (id, deltas) in std::mem::take(&mut tex.set) {
                for delta in &deltas {
                    self.egui_renderer.update_texture(&self.device, &self.queue, id, delta);
                }
            }
            let screen = egui_wgpu::ScreenDescriptor {
                size_in_pixels: [self.config.width, self.config.height],
                pixels_per_point: full.pixels_per_point,
            };

            let t_ui = std::time::Instant::now();
            let mut enc = self.device.create_command_encoder(&Default::default());
            let extra = self.egui_renderer.update_buffers(&self.device, &self.queue, &mut enc, &prims, &screen);
            {
                let mut pass = enc
                    .begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("egui"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: self.prof.as_ref().and_then(|p| p.writes(6)),
                        occlusion_query_set: None,
                        multiview_mask: None,
                    })
                    .forget_lifetime();
                self.egui_renderer.render(&mut pass, &prims, &screen);
            }
            if let Some(p) = &self.prof {
                p.resolve(&mut enc);
            }
            self.queue.submit(extra.into_iter().chain([enc.finish()]));
            if let Some(p) = &mut self.prof {
                let ms = |a: std::time::Instant, b: std::time::Instant| (b - a).as_secs_f64() * 1000.0;
                let now = std::time::Instant::now();
                p.cpu[0] += ms(t_start, t_acquired);
                p.cpu[1] += ms(t_acquired, t_scene);
                p.cpu[2] += ms(t_scene_sent, t_ui);
                p.cpu[3] += ms(t_scene, t_scene_sent) + ms(t_ui, now);
                p.collect(&self.device);
            }
            for id in &tex.free {
                self.egui_renderer.free_texture(id);
            }
            if let Some((path, left)) = &mut self.shot {
                if *left == 0 {
                    let path = path.clone();
                    self.write_shot(&frame.texture, &path, win_w, win_h);
                    std::process::exit(0);
                }
                *left -= 1;
            }
            if let Some(path) = self.shot_once.take() {
                self.write_shot(&frame.texture, &path, win_w, win_h);
            }
            self.window.pre_present_notify();
            self.queue.present(frame);
            // the workers have had the whole frame; what they made shows from the next one
            let t = std::time::Instant::now();
            for h in handles {
                take_built!(h.join().expect("chunk worker"));
            }
            build_ms += t.elapsed().as_secs_f64() * 1000.0;
        });
        self.stats.chunks_built += built as u64;
        self.stats.build_ms += build_ms;
    }
}

#[cfg(test)]
mod globals_slots {
    use super::{CAVE_SLOT, LOOK_SLOT, WAVE_SLOTS};

    /// The cave flag once shared slot 63 with a wave phase, so on the surface the shader often thought the player was
    /// underground and drew every model nearly black.
    #[test]
    fn cave_flag_has_its_own_slot() {
        assert!(!WAVE_SLOTS.contains(&CAVE_SLOT), "cave flag shares a slot with a wave phase");
        // `cave` follows `sun_vp_static` (slots 104..120) in the shader's Globals and fits the 656-byte buffer
        assert_eq!(CAVE_SLOT, 120);
        assert!(LOOK_SLOT == CAVE_SLOT + 4 && LOOK_SLOT + 4 <= 656 / 4);
        let shader = include_str!("../shader.wgsl");
        let globals = &shader[shader.find("struct Globals").unwrap()..];
        let globals = &globals[..globals.find("};").unwrap()];
        let tail: Vec<&str> = globals.lines().map(str::trim).filter(|l| !l.is_empty()).rev().take(2).collect();
        assert!(tail[0].starts_with("look: vec4<f32>") && tail[1].starts_with("cave: vec4<f32>"), "{tail:?}");
        assert!(!shader.contains("globals.cam_r.w > 0.5") && !shader.contains("globals.cam_r.w - 1.0"), "shader still reads the cave flag from cam_r.w");
    }

    #[test]
    fn natural_cave_shader_validates() {
        use wgpu::naga::valid::{Capabilities, ValidationFlags, Validator};

        let source = crate::quality::scene_source(include_str!("../shader.wgsl"), crate::quality::Quality::default());
        let shader = wgpu::naga::front::wgsl::parse_str(&source).unwrap_or_else(|err| panic!("{}", err.emit_to_string(&source)));
        Validator::new(ValidationFlags::all(), Capabilities::all()).validate(&shader).unwrap();
    }
}
