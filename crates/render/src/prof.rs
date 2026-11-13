/// GPU time of each pass, from timestamp queries (`ALAZ_GPUPROF`). Reading them back waits for the GPU every
/// frame, so the frame rate under it is lower than normal: only the split between passes is meant to be read.
pub(crate) struct GpuProf {
    pub(crate) set: wgpu::QuerySet,
    pub(crate) resolve: wgpu::Buffer,
    pub(crate) read: wgpu::Buffer,
    pub(crate) period_ns: f32,
    pub(crate) sums: [f64; GpuProf::PASSES],
    /// CPU side of the same frames: waiting for the swapchain, building the scene, the interface (run + tessellate),
    /// recording and submitting
    pub(crate) cpu: [f64; 4],
    pub(crate) frames: u32,
    /// Whether the static shadow map was drawn this frame (its timestamps are only written then), and how often.
    pub(crate) static_ran: bool,
    pub(crate) static_draws: u32,
    pub(crate) static_max: f64,
    /// Slowest frame's GPU total in this window, and the frames above 1.5x the window's last average (spikes).
    pub(crate) frame_max: f64,
    pub(crate) since: std::time::Instant,
}

impl GpuProf {
    const PASSES: usize = 9;
    pub(crate) const NAMES: [&'static str; GpuProf::PASSES] = ["shadow", "terrain", "nodes", "dynamic", "outline", "pixel", "egui", "bake", "shadow_static"];

    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> GpuProf {
        let n = (Self::PASSES * 2) as u32;
        let size = n as u64 * 8;
        GpuProf {
            set: device.create_query_set(&wgpu::QuerySetDescriptor { label: Some("gpu prof"), ty: wgpu::QueryType::Timestamp, count: n }),
            resolve: device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false }),
            read: device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }),
            period_ns: queue.get_timestamp_period(),
            sums: [0.0; Self::PASSES],
            cpu: [0.0; 4],
            frames: 0,
            static_ran: false,
            static_draws: 0,
            static_max: 0.0,
            frame_max: 0.0,
            since: std::time::Instant::now(),
        }
    }

    pub(crate) fn writes(&self, pass: usize) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        Some(wgpu::RenderPassTimestampWrites { query_set: &self.set, beginning_of_pass_write_index: Some(pass as u32 * 2), end_of_pass_write_index: Some(pass as u32 * 2 + 1) })
    }

    pub(crate) fn resolve(&self, enc: &mut wgpu::CommandEncoder) {
        enc.resolve_query_set(&self.set, 0..(Self::PASSES * 2) as u32, &self.resolve, 0);
        enc.copy_buffer_to_buffer(&self.resolve, 0, &self.read, 0, (Self::PASSES * 16) as u64);
    }

    /// After the frame is submitted: wait for it, add its pass times, print every 5 s.
    pub(crate) fn collect(&mut self, device: &wgpu::Device) {
        let slice = self.read.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        if let Ok(data) = slice.get_mapped_range() {
            let ts: &[u64] = bytemuck::cast_slice(&data);
            let mut frame_ms = 0.0;
            for i in 0..Self::PASSES {
                if i == 8 && !self.static_ran {
                    continue;
                }
                let ms = ts[i * 2 + 1].saturating_sub(ts[i * 2]) as f64 * self.period_ns as f64 / 1e6;
                self.sums[i] += ms;
                frame_ms += ms;
                if i == 8 {
                    self.static_max = self.static_max.max(ms);
                }
            }
            self.frame_max = self.frame_max.max(frame_ms);
            self.frames += 1;
        }
        self.read.unmap();
        if self.since.elapsed().as_secs_f32() >= 5.0 && self.frames > 0 {
            let n = self.frames as f64;
            let parts: Vec<String> = (0..Self::PASSES).map(|i| format!("{} {:.2}", Self::NAMES[i], self.sums[i] / n)).collect();
            println!("[gpu] ms/frame: {} | total {:.2} | static shadow drawn {} times (slowest {:.2} ms) | slowest frame {:.2}", parts.join(" "), self.sums.iter().sum::<f64>() / n, self.static_draws, self.static_max, self.frame_max);
            println!("[cpu] ms/frame: acquire {:.2} scene {:.2} ui {:.2} submit {:.2}", self.cpu[0] / n, self.cpu[1] / n, self.cpu[2] / n, self.cpu[3] / n);
            self.sums = [0.0; Self::PASSES];
            self.cpu = [0.0; 4];
            self.frames = 0;
            self.static_draws = 0;
            self.static_max = 0.0;
            self.frame_max = 0.0;
            self.since = std::time::Instant::now();
        }
    }
}
