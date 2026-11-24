use crate::bake::GroundBake;
use crate::mesh::*;
use crate::prof::GpuProf;
use crate::sprites::{self, Art};
use crate::quality::{self, scene_source, Quality, ShadowQuality};
use crate::{hotreload, pipelines, Camera, DEPTH_FORMAT, NIGHT_TINT, SHADOW_FORMAT};
use alaz_sim::time::daylight_permille;
use alaz_sim::{rules, AgentKind, BuildingKind, Terrain, World};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::event::WindowEvent;
use winit::window::Window;

mod frame;

pub(crate) const CHUNK_TILES: i32 = 32;

pub(crate) const MAX_CACHED_CHUNKS: usize = 600;

/// Most chunks whose meshes are built at the same time (each on a thread of its own, while the frame goes on).
pub(crate) const MAX_BUILDS_PER_FRAME: usize = 4;

/// A killed wild animal on its way down: where it fell (metres), how it stood, which side it falls to, and when.
pub(crate) struct Fall {
    pub(crate) id: u32,
    pub(crate) species: alaz_sim::WildSpecies,
    pub(crate) x: f32,
    pub(crate) z: f32,
    pub(crate) heading: f32,
    pub(crate) stride: f32,
    pub(crate) side: f32,
    pub(crate) at: std::time::Instant,
}

pub(crate) struct Chunk {
    pub(crate) terrain: Option<(wgpu::Buffer, u32)>,
    pub(crate) nodes: Option<(wgpu::Buffer, u32)>,
    /// Grass, flowers, pebbles: too small to show in the sun's shadow map, so they are kept apart and not cast.
    pub(crate) decor: Option<(wgpu::Buffer, u32)>,
    pub(crate) nodes_version: (u64, u64),
    /// What had been seen when the nodes were built (unknown things look plain).
    pub(crate) seen: (u128, u128),
    pub(crate) level: u8,
    pub(crate) far: bool,
    pub(crate) used: u64,
    /// Which build of the meshes this is (unique per build): the static shadow map is redrawn when it differs.
    pub(crate) build: u64,
}

/// The meshes of one chunk, uploaded, as a worker thread leaves them.
pub(crate) struct BuiltChunk {
    pub(crate) key: (i32, i32),
    /// None where the chunk already had its ground (that mesh does not change).
    pub(crate) terrain: Option<(wgpu::Buffer, u32)>,
    pub(crate) nodes: Option<(wgpu::Buffer, u32)>,
    pub(crate) decor: Option<(wgpu::Buffer, u32)>,
    pub(crate) nodes_version: (u64, u64),
    pub(crate) seen: (u128, u128),
    pub(crate) level: u8,
    /// Trees were built with fewer segments because the chunk lay far from the camera.
    pub(crate) far: bool,
}

/// Chunks whose centre lies further than this many tiles from the camera target build their trees with less detail.
pub(crate) const FAR_TREE_TILES: f32 = 40.0;

/// Build one chunk's meshes and put them on the GPU. Reads the world only, so it can run beside the frame.
pub(crate) fn build_chunk(device: &wgpu::Device, art: &Art, world: &World, key: (i32, i32), with_terrain: bool, far: bool) -> BuiltChunk {
    let buffer = |v: &Mesh| (device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&v[..]), usage: wgpu::BufferUsages::VERTEX }), v.len() as u32);
    let upload = |v: &Mesh| (!v.is_empty()).then(|| buffer(v));
    let (x0, y0) = (key.0 * CHUNK_TILES, key.1 * CHUNK_TILES);
    let (x1, y1) = ((x0 + CHUNK_TILES).min(world.map.w), (y0 + CHUNK_TILES).min(world.map.h));
    let terrain = with_terrain.then(|| {
        let mut v = Mesh::new();
        build_terrain(&mut v, art, world, (x0, y0, x1, y1));
        buffer(&v)
    });
    let mut v = Mesh::fixed();
    build_decor(&mut v, world, (x0, y0, x1, y1));
    pre_lift(world, &mut v);
    let decor = upload(&v);
    let mut v = Mesh::fixed();
    build_nodes(&mut v, world, (x0, y0, x1, y1), world.perception_level(), if far { TreeDetail::Far } else { TreeDetail::Full });
    pre_lift(world, &mut v);
    let nodes = upload(&v);
    BuiltChunk { key, terrain, nodes, decor, nodes_version: world.node_stamp(x0, y0), seen: world.seen_bits(), level: world.perception_level(), far }
}

/// The sun's shadow of what stands still (chunk models), kept between frames. It is drawn into a box 1.5 times as
/// wide as the dynamic one, with the same texel size, and redrawn only when the sun has stepped, the box size
/// changed, the camera nears the edge of the margin, or a chunk's models changed.
#[derive(Default)]
pub(crate) struct StaticShadow {
    /// The map holds a finished drawing.
    pub(crate) valid: bool,
    /// Light-space view-projection it was drawn with, and the sun direction it was drawn along.
    pub(crate) vp: glam::Mat4,
    pub(crate) dir: [f32; 3],
    /// Half the dynamic box's width in metres (so the texel is `radius / (shadow_res / 2)`), the box centre in whole texels
    /// of the sun's view, and the centre's depth (the dynamic map shares the depth range).
    pub(crate) radius: f32,
    pub(crate) tex: [i64; 2],
    pub(crate) z: f32,
    /// The chunks in it: key -> (build, had models).
    pub(crate) drawn: HashMap<(i32, i32), (u64, bool)>,
    pub(crate) last_draw: u64,
    /// `ALAZ_SHADOW_EVERY=1` (a check): redraw every frame, as before the static map existed.
    pub(crate) every_frame: bool,
}

/// Cheap counters for performance testing (`ALAZ_PERF`).
#[derive(Clone, Copy, Debug, Default)]
pub struct RenderStats {
    pub chunks_drawn: u32,
    pub vertices: u64,
    pub chunks_built: u64,
    pub build_ms: f64,
}

/// (x0, z0, x1, z1) with a margin for tall objects, used to skip off-screen creatures and buildings.
pub(crate) fn visible_chunks(cam: &Camera, aspect: f32, world: &World) -> (i32, i32, i32, i32, (f32, f32, f32, f32)) {
    let (mut x0, mut z0, mut x1, mut z1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    let mut ok = true;
    for (nx, ny) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        match cam.ground_at(nx, ny, aspect) {
            Some(p) if p.x.is_finite() && p.z.is_finite() => {
                x0 = x0.min(p.x);
                x1 = x1.max(p.x);
                z0 = z0.min(p.z);
                z1 = z1.max(p.z);
            }
            _ => ok = false,
        }
    }
    if !ok || x1 - x0 > 600.0 || z1 - z0 > 600.0 {
        (x0, z0, x1, z1) = (cam.target.x - 150.0, cam.target.z - 150.0, cam.target.x + 150.0, cam.target.z + 150.0);
    }
    let m = 10.0;
    let view = (x0 - m, z0 - m, x1 + m, z1 + m);
    let c = |v: f32, max_tiles: i32| ((v.floor() as i32).div_euclid(CHUNK_TILES)).clamp(0, (max_tiles - 1) / CHUNK_TILES);
    (c(view.0, world.map.w), c(view.1, world.map.h), c(view.2, world.map.w), c(view.3, world.map.h), view)
}

/// Fullscreen pass that enlarges the small picture by a whole number of pixels and grades it.
pub(crate) struct PixelPass {
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) layout: wgpu::BindGroupLayout,
    pub(crate) uniform: wgpu::Buffer,
    pub(crate) group: wgpu::BindGroup,
    /// 0 = plain, 1 = film grade.
    pub(crate) cinema: f32,
}

/// Draws ink outlines from the scene's depth, at the small picture's resolution.
pub(crate) struct OutlinePass {
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) layout: wgpu::BindGroupLayout,
    pub(crate) group: wgpu::BindGroup,
}

pub struct Renderer {
    pub(crate) window: Arc<Window>,
    pub(crate) surface: wgpu::Surface<'static>,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) config: wgpu::SurfaceConfiguration,
    pub(crate) depth: wgpu::TextureView,
    pub(crate) scene: wgpu::TextureView,
    pub(crate) inked: wgpu::TextureView,
    pub(crate) outline: OutlinePass,
    pub(crate) pixel: PixelPass,
    /// Models and decals (and, with the mark pipelines, the player's outline).
    pub(crate) tri_pipeline: wgpu::RenderPipeline,
    /// The ground of a chunk whose bake is done, and of one whose is not (it works its colour out as it draws).
    pub(crate) ground_pipeline: wgpu::RenderPipeline,
    pub(crate) ground_live_pipeline: wgpu::RenderPipeline,
    pub(crate) bake_pipeline: wgpu::RenderPipeline,
    /// The ground's time-independent colour, drawn ahead of time (bake.rs).
    pub(crate) bake: GroundBake,
    /// Shader files watched for changes (ALAZ_HOTRELOAD).
    pub(crate) hot: Option<hotreload::HotReload>,
    /// What the player chose (and the environment forced): see `set_quality`.
    pub(crate) quality: Quality,
    pub(crate) layouts: pipelines::Layouts,
    pub(crate) atlas_view: wgpu::TextureView,
    pub(crate) height_view: wgpu::TextureView,
    pub(crate) tile_view: wgpu::TextureView,
    /// Texels on a side of the dynamic and the static shadow map.
    pub(crate) shadow_res: u32,
    pub(crate) shadow_static_res: u32,
    /// The player's body marked in the picture's alpha (all of it, then the hidden part), for the outline pass to
    /// draw a bright line round whatever of it stands behind something.
    pub(crate) mark_pipeline: wgpu::RenderPipeline,
    pub(crate) hidden_pipeline: wgpu::RenderPipeline,
    pub(crate) chunks: HashMap<(i32, i32), Chunk>,
    pub(crate) frame_no: u64,
    pub(crate) stats: RenderStats,
    pub(crate) globals: wgpu::Buffer,
    pub(crate) height_tex: wgpu::Texture,
    /// Per tile of the height window: its ground picture (atlas texel corner), ground kind and shade, so the shader
    /// can draw a neighbour's ground across a ragged boundary instead of along tile edges.
    pub(crate) tile_tex: wgpu::Texture,
    /// The tile window as the CPU wrote it (the surface layer is patched into its kind channel and the lot sent again).
    pub(crate) tile_cpu: Vec<f32>,
    /// The weather's surface code per tile of the window (mud, puddles, ice), and what it was made from.
    pub(crate) surf_base: Vec<u8>,
    pub(crate) surf_wkey: Option<(u64, i32, i32, i32, bool, i32, i32)>,
    /// What the window's surface layer was last built for (see `frame.rs`); `surf_on`: it holds any surface at all.
    pub(crate) surf_key: Option<(u64, i32, i32, i32, bool, u64, u64, i32, i32)>,
    pub(crate) surf_on: bool,
    /// (seed, tile corner of the window's first texel) of what the height texture holds now
    pub(crate) height_at: Option<(u64, i32, i32)>,
    /// The height window as the CPU wrote it, and the cave-mouth hollows patched into it (tile corner -> metres).
    pub(crate) height_cpu: Vec<f32>,
    pub(crate) carve_applied: Arc<std::collections::BTreeMap<(i32, i32), f32>>,
    pub(crate) weather_kind: f32,
    pub(crate) weather_amount: f32,
    pub(crate) wind: f32,
    pub(crate) bind_group: wgpu::BindGroup,
    pub(crate) shadow_view: wgpu::TextureView,
    pub(crate) shadow_bind_group: wgpu::BindGroup,
    pub(crate) shadow_pipeline: wgpu::RenderPipeline,
    pub(crate) shadow_static_view: wgpu::TextureView,
    pub(crate) shadow_static_pipeline: wgpu::RenderPipeline,
    pub(crate) shadow_copy_pipeline: wgpu::RenderPipeline,
    pub(crate) shadow_copy_group: wgpu::BindGroup,
    pub(crate) shadow_static: StaticShadow,
    pub(crate) chunk_gen: u64,
    /// The asset files' generation the meshes and the atlas were built from.
    pub(crate) assets_gen: u64,
    pub(crate) art: Arc<Art>,
    pub(crate) egui_ctx: egui::Context,
    pub(crate) egui_state: egui_winit::State,
    pub(crate) egui_renderer: egui_wgpu::Renderer,
    /// Per-creature walking animation state, keyed by agent id.
    pub(crate) anim: HashMap<u32, Walk>,
    /// The same for wild animals, keyed by their wild id.
    pub(crate) wild_anim: HashMap<u32, Walk>,
    /// Killed wild animals still being drawn as they fall and sink (the sim has already removed them).
    pub(crate) falls: Vec<Fall>,
    pub(crate) last_frame: std::time::Instant,
    pub(crate) tick_seen: u64,
    pub(crate) tick_at: std::time::Instant,
    pub(crate) tick_gap: f32,
    wild_track_steps: HashMap<u32, frame::TrackStepper>,
    wild_tracks: VecDeque<frame::TrackMark>,
    /// Tile of the resource node under the pointer: drawn with a bright rim so it is clear what can be taken.
    pub hover_node: Option<(i32, i32)>,
    /// `ALAZ_DEMO_VIEW=<model name>`: the preview screen shows only this model (or an item icon), the camera circling.
    pub(crate) view: Option<crate::assets::View>,
    /// How far the game is between the last simulated tick and the next (0..1), when the caller knows it
    /// exactly; otherwise it is guessed from the time between ticks.
    pub sim_alpha: Option<f32>,
    /// The stage coordinate (0..=9, see `Evo`) the people are drawn at, when the caller smooths it over time;
    /// otherwise it follows the world's perception directly.
    pub evo: Option<f32>,
    /// Dev aid, `ALAZ_DEMO_LINEUP=<outfit mask>[,lod|walk|sit]`: a row of the ten stages of the body in front of the
    /// player, all the same person (`ALAZ_DEMO_LINEUP_ID`, 12 by default), to compare them side by side.
    pub(crate) lineup: Option<(u16, String)>,
    pub(crate) lineup_id: u32,
    /// `ALAZ_DEMO_FALL=<species>`: an animal of that kind falls over again and again in front of the camera.
    pub(crate) demo_fall: Option<alaz_sim::WildSpecies>,
    /// The sun direction the shadows are cast along: it follows the sun in small steps, not every frame, so
    /// shadow edges hold still instead of shimmering as the sun moves.
    pub(crate) shadow_dir: [f32; 3],
    /// Where the cloud field has drifted to (it moves with the wind, so it is summed frame by frame).
    pub(crate) cloud_off: [f32; 2],
    /// World-space gallery geometry around visible openings and the current underground floor.
    natural_cave_view: Option<frame::NaturalCaveView>,
    /// The local roof opening used by the previous frame and by pointer selection.
    surface_cutaway: Option<frame::SurfaceCutaway>,
    /// The torch's strength as drawn (it eases toward the sim's value, so a dying torch dims instead of jumping).
    pub(crate) cave_light: f32,
    pub(crate) prof: Option<GpuProf>,
    /// `ALAZ_SHOT`: where to write the picture, and how many frames to wait first
    pub(crate) shot: Option<(String, u32)>,
    pub(crate) shot_once: Option<String>,
}

impl Renderer {
    pub fn new(window: Arc<Window>) -> Self {
        Self::with_quality(window, Quality::default())
    }

    /// `quality` is what the player saved; the `ALAZ_PIXELS`, `ALAZ_SHADOWS` and `ALAZ_DECOR` variables override it.
    pub fn with_quality(window: Arc<Window>, quality: Quality) -> Self {
        let quality = quality.with_env();
        quality::set_pixels(quality.pixels);
        let size = window.inner_size();
        // `WGPU_BACKEND` (vulkan, dx12, gl, metal) picks the graphics API: a broken driver for one of them (the Intel Vulkan driver
        // on some Windows machines crashes at start) can then be avoided
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(window.clone())));
        let surface = instance.create_surface(window.clone()).expect("create surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .expect("no adapter");
        // ALAZ_GPUPROF=1 times every pass on the GPU (when the adapter can) and prints it every 5 s
        let gpu_prof = std::env::var_os("ALAZ_GPUPROF").is_some() && adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let features = if gpu_prof { wgpu::Features::TIMESTAMP_QUERY } else { wgpu::Features::empty() };
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_features: features, ..Default::default() })).expect("request device");
        let prof = gpu_prof.then(|| GpuProf::new(&device, &queue));

        let mut config = surface.get_default_config(&adapter, size.width.max(1), size.height.max(1)).expect("surface unsupported");
        // ALAZ_NOVSYNC lifts the 60 fps cap so performance tests measure real frame cost
        config.present_mode =
            if std::env::var_os("ALAZ_NOVSYNC").is_some() { wgpu::PresentMode::AutoNoVsync } else { wgpu::PresentMode::AutoVsync };
        // ALAZ_SHOT copies the finished frame out of the swapchain
        if (std::env::var_os("ALAZ_SHOT").is_some() || std::env::var_os("ALAZ_PILOT").is_some()) && surface.get_capabilities(&adapter).usages.contains(wgpu::TextureUsages::COPY_SRC) {
            config.usage |= wgpu::TextureUsages::COPY_SRC;
        }
        surface.configure(&device, &config);

        // --- the painted atlas
        let (art, atlas) = sprites::build();
        let atlas_view = Self::upload_atlas(&device, &queue, &atlas);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(scene_source(hotreload::SHADER, quality).into()),
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: 656,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let height_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("height window"),
            size: wgpu::Extent3d { width: HWIN, height: HWIN, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let height_view = height_tex.create_view(&Default::default());
        let tile_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tile window"),
            size: wgpu::Extent3d { width: HWIN, height: HWIN, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let tile_view = tile_tex.create_view(&Default::default());
        let bake = GroundBake::new(&device, &globals, &atlas_view, &height_view, &tile_view);
        // sun shadows: depth seen from the sun, over the part of the world around the camera. The static map (what
        // stands still) is drawn rarely, over a box 1.5 times as wide with the same texel size; every frame its window
        // for the current box is copied into the dynamic map, and what moves is drawn over that.
        let (shadow_res, shadow_static_res) = quality.shadows.sizes();
        let (shadow_view, shadow_static_view) = Self::make_shadow_maps(&device, quality.shadows);
        let bgl_main = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("main"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
            ],
        });
        let shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("shadow"), bind_group_layouts: &[Some(&bgl)], immediate_size: 0 });
        let copy_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow copy"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
            ],
        });
        let copy_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("shadow copy"), bind_group_layouts: &[Some(&copy_bgl)], immediate_size: 0 });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&bgl_main)], immediate_size: 0 });
        let bake_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("ground bake"), bind_group_layouts: &[Some(&bake.layout)], immediate_size: 0 });
        let (w, h) = (config.width, config.height);
        let depth = Self::make_depth(&device, w, h);
        let scene = Self::make_color(&device, config.format, w, h);
        let inked = Self::make_color(&device, config.format, w, h);
        let (outline, outline_pl) = {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("outline"),
                source: wgpu::ShaderSource::Wgsl(hotreload::OUTLINE.into()),
            });
            let tex = |binding, sample_type| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                count: None,
            };
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("outline"),
                entries: &[tex(0, wgpu::TextureSampleType::Float { filterable: false }), tex(1, wgpu::TextureSampleType::Depth)],
            });
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
            let pipeline = pipelines::fullscreen(&device, "outline", &module, &pl, config.format);
            let group = Self::make_outline_group(&device, &layout, &scene, &depth);
            (OutlinePass { pipeline, layout, group }, pl)
        };
        let (pixel, pixel_pl) = {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("pixel"),
                source: wgpu::ShaderSource::Wgsl(hotreload::PIXEL.into()),
            });
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("pixel"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                        count: None,
                    },
                ],
            });
            let uniform = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("pixel uniform"),
                size: 48,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
            let pipeline = pipelines::fullscreen(&device, "pixel", &module, &pl, config.format);
            let group = Self::make_pixel_group(&device, &layout, &inked, &uniform);
            let cinema = if std::env::var_os("ALAZ_NOCINEMA").is_some() { 0.0 } else { 1.0 };
            (PixelPass { pipeline, layout, uniform, group, cinema }, pl)
        };

        let layouts = pipelines::Layouts { shadow_bgl: bgl, main_bgl: bgl_main, copy_bgl, shadow: shadow_layout, copy: copy_layout, scene: layout, bake: bake_layout, outline: outline_pl, pixel: pixel_pl };
        let set = pipelines::SceneSet::build(&device, &shader, &layouts, config.format);
        let (bind_group, shadow_bind_group, shadow_copy_group) = Self::make_groups(&device, &layouts, &globals, &atlas_view, &height_view, &tile_view, &shadow_view, &shadow_static_view, &bake);
        let hot = hotreload::HotReload::new();
        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            None,
            Some(device.limits().max_texture_dimension_2d as usize),
        );
        let egui_renderer = egui_wgpu::Renderer::new(&device, config.format, egui_wgpu::RendererOptions::default());

        Self {
            window,
            surface,
            device,
            queue,
            config,
            depth,
            scene,
            inked,
            outline,
            pixel,
            tri_pipeline: set.tri,
            ground_pipeline: set.ground,
            ground_live_pipeline: set.ground_live,
            bake_pipeline: set.bake,
            bake,
            hot,
            quality,
            layouts,
            atlas_view,
            height_view,
            tile_view,
            shadow_res,
            shadow_static_res,
            mark_pipeline: set.mark,
            hidden_pipeline: set.hidden,
            chunks: HashMap::new(),
            anim: HashMap::new(),
            wild_anim: HashMap::new(),
            falls: Vec::new(),
            sim_alpha: None,
            evo: None,
            lineup_id: std::env::var("ALAZ_DEMO_LINEUP_ID").ok().and_then(|v| v.parse().ok()).unwrap_or(12),
            demo_fall: std::env::var("ALAZ_DEMO_FALL").ok().and_then(|v| {
                use alaz_sim::WildSpecies as S;
                Some(match v.as_str() {
                    "dog" => S::Dog,
                    "ox" => S::Ox,
                    "horse" => S::Horse,
                    "wolf" => S::Wolf,
                    "deer" => S::Deer,
                    _ => return None,
                })
            }),
            lineup: std::env::var("ALAZ_DEMO_LINEUP").ok().map(|v| {
                let (m, mode) = v.split_once(',').unwrap_or((v.as_str(), ""));
                (m.trim().parse::<u16>().unwrap_or(0), mode.trim().to_string())
            }),
            shadow_dir: [0.0, 1.0, 0.0],
            cloud_off: [0.0, 0.0],
            natural_cave_view: None,
            surface_cutaway: None,
            cave_light: 0.0,
            prof,
            shot_once: None,
            shot: std::env::var("ALAZ_SHOT").ok().map(|p| (p, std::env::var("ALAZ_SHOT_FRAMES").ok().and_then(|v| v.parse().ok()).unwrap_or(240))),
            hover_node: None,
            view: std::env::var("ALAZ_DEMO_VIEW").ok().filter(|v| !v.is_empty()).map(crate::assets::View::new),
            last_frame: std::time::Instant::now(),
            tick_seen: 0,
            tick_at: std::time::Instant::now(),
            tick_gap: 1.0 / 60.0,
            wild_track_steps: HashMap::new(),
            wild_tracks: VecDeque::new(),
            frame_no: 0,
            stats: RenderStats::default(),
            globals,
            height_tex,
            tile_tex,
            tile_cpu: Vec::new(),
            surf_base: vec![0; (HWIN * HWIN) as usize],
            surf_wkey: None,
            surf_key: None,
            surf_on: false,
            height_at: None,
            height_cpu: Vec::new(),
            carve_applied: Arc::new(std::collections::BTreeMap::new()),
            weather_kind: 1.0,
            weather_amount: 0.0,
            wind: 0.0,
            bind_group,
            shadow_view,
            shadow_bind_group,
            shadow_pipeline: set.shadow,
            shadow_static_view,
            shadow_static_pipeline: set.shadow_static,
            shadow_copy_pipeline: set.shadow_copy,
            shadow_copy_group,
            shadow_static: StaticShadow { every_frame: std::env::var_os("ALAZ_SHADOW_EVERY").is_some(), ..Default::default() },
            chunk_gen: 0,
            assets_gen: crate::assets::generation(),
            art: Arc::new(art),
            egui_ctx,
            egui_state,
            egui_renderer,
        }
    }

    /// The painted atlas on the GPU.
    fn upload_atlas(device: &wgpu::Device, queue: &wgpu::Queue, atlas: &sprites::Atlas) -> wgpu::TextureView {
        let tex = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("atlas"),
                size: wgpu::Extent3d { width: sprites::ATLAS, height: sprites::ATLAS, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::default(),
            &atlas.data,
        );
        tex.create_view(&Default::default())
    }

    /// The sun's shadow maps: depth seen from the sun, over the part of the world around the camera. The static map
    /// (what stands still) is drawn rarely, over a box 1.5 times as wide with the same texel size; every frame its
    /// window for the current box is copied into the dynamic map, and what moves is drawn over that. (Dynamic, static.)
    fn make_shadow_maps(device: &wgpu::Device, q: ShadowQuality) -> (wgpu::TextureView, wgpu::TextureView) {
        let (dynamic, stat) = q.alloc();
        let make = |label, side| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d { width: side, height: side, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: SHADOW_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        (make("sun shadow", dynamic), make("sun shadow, static", stat))
    }

    /// The bind groups over the textures that a change of quality replaces: (main pass, shadow passes, shadow copy).
    #[allow(clippy::too_many_arguments)]
    fn make_groups(
        device: &wgpu::Device,
        l: &pipelines::Layouts,
        globals: &wgpu::Buffer,
        atlas: &wgpu::TextureView,
        height: &wgpu::TextureView,
        tiles: &wgpu::TextureView,
        shadow: &wgpu::TextureView,
        shadow_static: &wgpu::TextureView,
        bake: &GroundBake,
    ) -> (wgpu::BindGroup, wgpu::BindGroup, wgpu::BindGroup) {
        let tv = wgpu::BindingResource::TextureView;
        let main = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &l.main_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: tv(atlas) },
                wgpu::BindGroupEntry { binding: 2, resource: tv(height) },
                wgpu::BindGroupEntry { binding: 3, resource: tv(shadow) },
                wgpu::BindGroupEntry { binding: 4, resource: tv(tiles) },
                wgpu::BindGroupEntry { binding: 5, resource: tv(&bake.col_view) },
                wgpu::BindGroupEntry { binding: 6, resource: tv(&bake.nrm_view) },
            ],
        });
        // the shadow pass binds everything but the shadow map it is drawing into
        let shadow_pass = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow pass"),
            layout: &l.shadow_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: tv(atlas) },
                wgpu::BindGroupEntry { binding: 2, resource: tv(height) },
            ],
        });
        let copy = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow copy"),
            layout: &l.copy_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() }, wgpu::BindGroupEntry { binding: 7, resource: tv(shadow_static) }],
        });
        (main, shadow_pass, copy)
    }

    pub fn quality(&self) -> Quality {
        self.quality
    }

    /// Applies graphics quality while the game runs (the environment's `ALAZ_*` overrides still win). A change of pixel
    /// detail repaints the ground tiles, re-allocates the bake and drops every built chunk; a change of shadows
    /// re-allocates the shadow maps. Shaders are rebuilt in both cases. A few hundred ms of hitch, once.
    pub fn set_quality(&mut self, q: Quality) {
        let q = q.with_env();
        if q == self.quality {
            return;
        }
        let old = self.quality;
        self.quality = q;
        let pixels_changed = q.pixels != old.pixels;
        let shadows_changed = q.shadows != old.shadows;
        if pixels_changed {
            quality::set_pixels(q.pixels);
            self.repaint_atlas();
        }
        if shadows_changed {
            (self.shadow_res, self.shadow_static_res) = q.shadows.sizes();
            (self.shadow_view, self.shadow_static_view) = Self::make_shadow_maps(&self.device, q.shadows);
            self.shadow_static.valid = false;
        }
        if pixels_changed || shadows_changed {
            self.refresh_groups();
        }
    }

    /// Rebuilds the bind groups and scene pipelines after the atlas, bake or shadow maps were replaced.
    pub(crate) fn refresh_groups(&mut self) {
        let q = self.quality;
        let (main, shadow_pass, copy) = Self::make_groups(&self.device, &self.layouts, &self.globals, &self.atlas_view, &self.height_view, &self.tile_view, &self.shadow_view, &self.shadow_static_view, &self.bake);
        self.bind_group = main;
        self.shadow_bind_group = shadow_pass;
        self.shadow_copy_group = copy;
        // the scene shader's constants follow both (the file on disk when it is being watched)
        let body = self.hot.as_ref().and_then(|h| h.scene_source()).unwrap_or_else(|| hotreload::SHADER.to_string());
        let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("shader"), source: wgpu::ShaderSource::Wgsl(scene_source(&body, q).into()) });
        self.apply_scene(pipelines::SceneSet::build(&self.device, &module, &self.layouts, self.config.format));
    }

    /// Paints the atlas again (ground texel size or tile pictures changed): a new bake, every built chunk dropped.
    pub(crate) fn repaint_atlas(&mut self) {
        let (art, atlas) = sprites::build();
        self.atlas_view = Self::upload_atlas(&self.device, &self.queue, &atlas);
        self.art = Arc::new(art);
        self.bake = GroundBake::new(&self.device, &self.globals, &self.atlas_view, &self.height_view, &self.tile_view);
        self.layouts.bake = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("ground bake"), bind_group_layouts: &[Some(&self.bake.layout)], immediate_size: 0 });
        // (meshes carry atlas positions, and the tile window holds them too)
        self.chunks.clear();
        self.height_at = None;
        self.chunk_gen += 1;
    }

    /// Swaps in freshly built scene pipelines. What was drawn with the old shader is out of date.
    pub(crate) fn apply_scene(&mut self, set: pipelines::SceneSet) {
        self.shadow_pipeline = set.shadow;
        self.shadow_static_pipeline = set.shadow_static;
        self.shadow_copy_pipeline = set.shadow_copy;
        self.shadow_static.valid = false;
        self.tri_pipeline = set.tri;
        self.ground_pipeline = set.ground;
        self.ground_live_pipeline = set.ground_live;
        self.bake_pipeline = set.bake;
        self.bake.invalidate();
        self.mark_pipeline = set.mark;
        self.hidden_pipeline = set.hidden;
    }

    fn make_color(device: &wgpu::Device, format: wgpu::TextureFormat, w: u32, h: u32) -> wgpu::TextureView {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("scene"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default())
    }

    /// Dev aid (`ALAZ_SHOT=file.ppm`, `ALAZ_SHOT_FRAMES=n`): writes the finished frame (weather and interface
    /// included) to a file and quits, so a screenshot never depends on which window the desktop has in front.
    fn write_shot(&self, tex: &wgpu::Texture, path: &str, w: u32, h: u32) {
        let row = (w * 4).div_ceil(256) * 256;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (row * h) as u64, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            tex.as_image_copy(),
            wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit([enc.finish()]);
        let slice = buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let Ok(data) = slice.get_mapped_range() else { return };
        let bgra = matches!(self.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
        let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
        for y in 0..h {
            for x in 0..w {
                let i = (y * row + x * 4) as usize;
                let (r, g, b) = if bgra { (data[i + 2], data[i + 1], data[i]) } else { (data[i], data[i + 1], data[i + 2]) };
                out.extend_from_slice(&[r, g, b]);
            }
        }
        let _ = std::fs::write(path, out);
    }

    fn make_outline_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, scene: &wgpu::TextureView, depth: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("outline"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(scene) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(depth) },
            ],
        })
    }

    fn make_pixel_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, scene: &wgpu::TextureView, uniform: &wgpu::Buffer) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pixel"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(scene) },
                wgpu::BindGroupEntry { binding: 1, resource: uniform.as_entire_binding() },
            ],
        })
    }

    fn make_depth(device: &wgpu::Device, w: u32, h: u32) -> wgpu::TextureView {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default())
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
        self.depth = Self::make_depth(&self.device, w, h);
        self.scene = Self::make_color(&self.device, self.config.format, w, h);
        self.inked = Self::make_color(&self.device, self.config.format, w, h);
        self.outline.group = Self::make_outline_group(&self.device, &self.outline.layout, &self.scene, &self.depth);
        self.pixel.group = Self::make_pixel_group(&self.device, &self.pixel.layout, &self.inked, &self.pixel.uniform);
    }

    fn vertex_buffer(&self, v: &[Vertex]) -> wgpu::Buffer {
        self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(v), usage: wgpu::BufferUsages::VERTEX })
    }

    /// Drop cached scene buffers (call when the world is replaced).
    pub fn reset_scene(&mut self) {
        self.chunks.clear();
        self.natural_cave_view = None;
        self.wild_track_steps.clear();
        self.wild_tracks.clear();
    }

    /// A wild animal was killed at (`x_mm`, `y_mm`): draw it falling over there. It keeps the way it was facing and
    /// falls to a side picked from its id.
    pub fn wild_killed(&mut self, id: u32, species: alaz_sim::WildSpecies, x_mm: i64, y_mm: i64) {
        let (x, z) = (x_mm as f32 / 1000.0, y_mm as f32 / 1000.0);
        let walk = self.wild_anim.remove(&id);
        let heading = walk.map_or((id as f32 * 2.399) % std::f32::consts::TAU, |w| w.heading);
        let stride = walk.map_or(0.3, |w| w.stride);
        let side = if id.is_multiple_of(2) { 1.0 } else { -1.0 };
        self.falls.push(Fall { id, species, x, z, heading, stride, side, at: std::time::Instant::now() });
    }

    /// Where the last frame drew this person (metres): between ticks, not at the tick. The camera follows this, so
    /// it moves as evenly as the body does at any frame rate.
    pub fn drawn_pos(&self, id: alaz_sim::AgentId) -> Option<(f32, f32)> {
        self.anim.get(&id.0).map(|w| (w.x, w.z))
    }

    pub fn stats(&self) -> RenderStats {
        self.stats
    }

    pub fn aspect(&self) -> f32 {
        self.config.width as f32 / self.config.height as f32
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// Feed a window event to egui; true if the UI consumed it.
    pub fn on_window_event(&mut self, event: &WindowEvent) -> bool {
        self.egui_state.on_window_event(&self.window, event).consumed
    }

    /// True while the pointer is over (or dragging) an egui widget.
    /// Weather as it looks right now (it fades in and out): (rain 0..1, snow 0..1, wind 0..2).
    pub fn weather_now(&self) -> (f32, f32, f32) {
        let rain = if self.weather_kind < 1.5 { self.weather_amount } else { 0.0 };
        let snow = if self.weather_kind > 1.5 { self.weather_amount } else { 0.0 };
        (rain, snow, self.wind)
    }

    /// Dev aid (the game's pilot): input events as if the window had received them, seen by the next pass.
    pub fn inject_ui_events(&mut self, events: Vec<egui::Event>) {
        self.egui_state.egui_input_mut().events.extend(events);
    }

    /// Dev aid (the game's pilot): saves the next finished frame as a PPM and carries on.
    pub fn request_shot(&mut self, path: String) {
        self.shot_once = Some(path);
    }

    pub fn egui_context(&self) -> &egui::Context {
        &self.egui_ctx
    }

    pub fn pointer_over_ui(&self) -> bool {
        self.egui_ctx.egui_wants_pointer_input() || self.egui_ctx.is_pointer_over_egui()
    }
}

