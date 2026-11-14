//! The ground bake. Most of what colours a land texel never changes (picture, ragged borders, biomes, mountain and
//! snow cap, slope), yet the shader used to work it out again for every pixel of every frame. Here it is drawn once
//! into two textures instead, in blocks of 8x8 tiles within a small per-frame budget, and the ground shader only looks
//! it up and adds what moves (wind, rain, snow fall, sea, light).
//!
//! The textures are a ring of RING x RING chunks at the ground's own 27 texels per metre: a world texel lives at its
//! coordinates modulo the ring, so a chunk keeps its place while the camera stays near and nothing is copied when it
//! moves. A chunk is drawn from the bake only once all 16 of its blocks are done (until then the shader that works
//! the colour out on the spot draws it), and a chunk the ring has no room for is drawn that way for good.
use crate::renderer::CHUNK_TILES;
use std::collections::HashSet;

/// Ground texels per metre; the shader's number.
const TEXELS_PER_TILE: i32 = crate::sprites::GT;
/// Chunks on a side of the ring (shader.wgsl's RING_TEXELS is this times CHUNK_TEXELS).
pub(crate) const RING: i32 = 4;
pub(crate) const CHUNK_TEXELS: i32 = CHUNK_TILES * TEXELS_PER_TILE;
pub(crate) const RING_TEXELS: u32 = (RING * CHUNK_TEXELS) as u32;
/// A block is the unit of baking: 8x8 tiles, 216x216 texels, 16 to a chunk.
const BLOCK_TILES: i32 = 8;
const BLOCK_TEXELS: i32 = BLOCK_TILES * TEXELS_PER_TILE;
const BLOCKS_SIDE: i32 = CHUNK_TILES / BLOCK_TILES;
const FULL: u32 = (1 << (BLOCKS_SIDE * BLOCKS_SIDE)) - 1;
/// How many blocks one frame bakes at most (shader.wgsl's `bake_params` holds 32).
const BLOCKS_PER_FRAME: usize = 4;
/// Colour (rgb / 2, state / 3 in the two alpha bits) and the height gradient.
pub(crate) const COL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgb10a2Unorm;
pub(crate) const NRM_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;

/// One block to bake this frame: the world texel of its first texel, and where that lands in the ring.
pub(crate) struct Job {
    origin: [i32; 2],
    dest: [i32; 2],
}

pub(crate) struct GroundBake {
    _col: wgpu::Texture,
    _nrm: wgpu::Texture,
    pub(crate) col_view: wgpu::TextureView,
    pub(crate) nrm_view: wgpu::TextureView,
    params: wgpu::Buffer,
    pub(crate) layout: wgpu::BindGroupLayout,
    group: wgpu::BindGroup,
    /// Per ring slot: the chunk it holds, and which of its blocks are baked (one bit each).
    owner: Vec<Option<(i32, i32)>>,
    done: Vec<u32>,
    seed: Option<u64>,
    /// Blocks baked since the start (for the log)
    pub(crate) baked: u64,
}

fn slot_of(c: (i32, i32)) -> usize {
    (c.1.rem_euclid(RING) * RING + c.0.rem_euclid(RING)) as usize
}

impl GroundBake {
    pub(crate) fn new(device: &wgpu::Device, globals: &wgpu::Buffer, atlas: &wgpu::TextureView, height: &wgpu::TextureView, tiles: &wgpu::TextureView) -> GroundBake {
        let make = |label, format| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: RING_TEXELS, height: RING_TEXELS, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let col = make("ground bake colour", COL_FORMAT);
        let nrm = make("ground bake slope", NRM_FORMAT);
        let col_view = col.create_view(&Default::default());
        let nrm_view = nrm.create_view(&Default::default());
        let params = device.create_buffer(&wgpu::BufferDescriptor { label: Some("bake blocks"), size: 32 * 16, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let tex = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        };
        let uniform = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("ground bake"), entries: &[uniform(0), tex(1), tex(2), tex(4), uniform(8)] });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ground bake"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(atlas) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(height) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(tiles) },
                wgpu::BindGroupEntry { binding: 8, resource: params.as_entire_binding() },
            ],
        });
        let slots = (RING * RING) as usize;
        GroundBake { _col: col, _nrm: nrm, col_view, nrm_view, params, layout, group, owner: vec![None; slots], done: vec![0; slots], seed: None, baked: 0 }
    }

    /// Forgets everything baked (a new world, or the shader that bakes it was changed).
    pub(crate) fn invalidate(&mut self) {
        self.seed = None;
        self.owner.iter_mut().for_each(|o| *o = None);
        self.done.iter_mut().for_each(|d| *d = 0);
    }

    /// Can this chunk be drawn from the bake?
    pub(crate) fn ready(&self, chunk: (i32, i32)) -> bool {
        let s = slot_of(chunk);
        self.owner[s] == Some(chunk) && self.done[s] == FULL
    }

    /// Decides this frame's blocks. `visible` are the chunks being drawn, `near` the ones just beyond the screen
    /// (baked ahead only into room nobody wants), `target` the ground point the camera looks at and `win` the tile
    /// corner of the height window, which every block must lie well inside (its neighbours are read too).
    pub(crate) fn plan(&mut self, seed: u64, win: (i32, i32), visible: &[(i32, i32)], near: &[(i32, i32)], target: (f32, f32)) -> Vec<Job> {
        if self.seed != Some(seed) {
            self.invalidate();
            self.seed = Some(seed);
        }
        let dist = |c: &(i32, i32)| {
            let (cx, cz) = ((c.0 * CHUNK_TILES + CHUNK_TILES / 2) as f32 - target.0, (c.1 * CHUNK_TILES + CHUNK_TILES / 2) as f32 - target.1);
            cx * cx + cz * cz
        };
        let mut vis: Vec<(i32, i32)> = visible.to_vec();
        vis.sort_by(|a, b| dist(a).total_cmp(&dist(b)));
        let mut nr: Vec<(i32, i32)> = near.iter().copied().filter(|c| !visible.contains(c)).collect();
        nr.sort_by(|a, b| dist(a).total_cmp(&dist(b)));
        let wanted: HashSet<(i32, i32)> = vis.iter().chain(nr.iter()).copied().collect();
        let on_screen: HashSet<(i32, i32)> = vis.iter().copied().collect();
        let inside = |c: &(i32, i32)| {
            let (x0, y0) = (c.0 * CHUNK_TILES, c.1 * CHUNK_TILES);
            x0 - 3 >= win.0 && y0 - 3 >= win.1 && x0 + CHUNK_TILES + 3 <= win.0 + 255 && y0 + CHUNK_TILES + 3 <= win.1 + 255
        };
        let mut jobs = Vec::new();
        for c in vis.iter().chain(nr.iter()) {
            if jobs.len() >= BLOCKS_PER_FRAME {
                break;
            }
            if !inside(c) {
                continue;
            }
            let s = slot_of(*c);
            if self.owner[s] != Some(*c) {
                // room is made only by dropping a chunk that nobody wants (the ones on screen outrank the rest)
                let free = match self.owner[s] {
                    None => true,
                    Some(o) => !wanted.contains(&o) || (on_screen.contains(c) && !on_screen.contains(&o)),
                };
                if !free {
                    continue;
                }
                self.owner[s] = Some(*c);
                self.done[s] = 0;
            }
            for b in 0..BLOCKS_SIDE * BLOCKS_SIDE {
                if self.done[s] & (1 << b) != 0 || jobs.len() >= BLOCKS_PER_FRAME {
                    continue;
                }
                let (bx, by) = (b % BLOCKS_SIDE, b / BLOCKS_SIDE);
                let origin = [c.0 * CHUNK_TEXELS + bx * BLOCK_TEXELS, c.1 * CHUNK_TEXELS + by * BLOCK_TEXELS];
                let dest = [origin[0].rem_euclid(RING_TEXELS as i32), origin[1].rem_euclid(RING_TEXELS as i32)];
                jobs.push(Job { origin, dest });
                self.done[s] |= 1 << b;
                self.baked += 1;
            }
        }
        jobs
    }

    /// Records the blocks into the ring (before the passes that read it). `stamps` are the profiler's timestamps.
    pub(crate) fn encode(&self, enc: &mut wgpu::CommandEncoder, queue: &wgpu::Queue, pipeline: &wgpu::RenderPipeline, jobs: &[Job], stamps: Option<wgpu::RenderPassTimestampWrites>) {
        if jobs.is_empty() && stamps.is_none() {
            return;
        }
        let mut data = [0f32; 32 * 4];
        for (i, j) in jobs.iter().enumerate() {
            data[i * 4] = (j.origin[0] - j.dest[0]) as f32;
            data[i * 4 + 1] = (j.origin[1] - j.dest[1]) as f32;
        }
        queue.write_buffer(&self.params, 0, bytemuck::cast_slice(&data));
        let target = |view| {
            Some(wgpu::RenderPassColorAttachment { view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })
        };
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ground bake"),
            color_attachments: &[target(&self.col_view), target(&self.nrm_view)],
            depth_stencil_attachment: None,
            timestamp_writes: stamps,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, Some(&self.group), &[]);
        for (i, j) in jobs.iter().enumerate() {
            let (x, y, s) = (j.dest[0] as f32, j.dest[1] as f32, BLOCK_TEXELS as f32);
            pass.set_viewport(x, y, s, s, 0.0, 1.0);
            pass.set_scissor_rect(j.dest[0] as u32, j.dest[1] as u32, BLOCK_TEXELS as u32, BLOCK_TEXELS as u32);
            pass.draw(0..3, i as u32..i as u32 + 1);
        }
    }
}
