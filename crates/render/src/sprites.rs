//! Pixel art: every ground tile and sprite is painted here, in code, into one atlas.
//! 16 texels = 1 metre. Nothing is smoothed anywhere: what is painted is what is shown.

pub type Rgb = [u8; 3];

/// Texels per metre in the world.
pub const PPM: f32 = 16.0;
pub const ATLAS: u32 = 1024;
pub const TILE: i32 = 16;
/// Ground tiles are finer than the painted sprites: 21 texels to the metre, as close as whole texels get to the
/// picture's own 16 * 4/3, so a ground texel is one picture texel and they all come out the same width.
pub const GT: i32 = 21;
/// Material tiles (modulation textures for the 3D models) live in a fixed strip at the bottom of the atlas:
/// tile i (0 wood, 1 leaves, 2 stone, 3 skin and cloth) at x = 16 * i. A texel is a multiplier, 128 = 1.0.
pub const MAT_Y: u32 = 1008;

/// A rectangle in the atlas with its pivot (texels from the left edge and from the bottom edge).
#[derive(Clone, Copy, Default, Debug)]
pub struct Spr {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub px: i16,
    pub py: i16,
}

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n.max(1)
    }
    fn chance(&mut self, pct: u32) -> bool {
        self.below(100) < pct
    }
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    std::array::from_fn(|i| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round().clamp(0.0, 255.0) as u8)
}
fn mul(a: Rgb, k: f32) -> Rgb {
    std::array::from_fn(|i| (a[i] as f32 * k).round().clamp(0.0, 255.0) as u8)
}

/// Four tones of one material: light from the upper left, hue-shifted shadows (warm light, cool dark).
#[derive(Clone, Copy)]
pub struct Ramp {
    pub hi: Rgb,
    pub base: Rgb,
    pub sh: Rgb,
    pub dk: Rgb,
}
pub fn ramp(base: Rgb) -> Ramp {
    Ramp {
        hi: mix(mul(base, 1.22), [255, 238, 190], 0.22),
        base,
        sh: mix(mul(base, 0.72), [40, 40, 90], 0.14),
        dk: mix(mul(base, 0.46), [30, 28, 70], 0.22),
    }
}

#[derive(Clone)]
pub struct Bmp {
    pub w: i32,
    pub h: i32,
    pub d: Vec<[u8; 4]>,
}

impl Bmp {
    pub fn new(w: i32, h: i32) -> Self {
        Bmp { w, h, d: vec![[0; 4]; (w * h) as usize] }
    }
    pub fn get(&self, x: i32, y: i32) -> [u8; 4] {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            [0; 4]
        } else {
            self.d[(y * self.w + x) as usize]
        }
    }
    pub fn put(&mut self, x: i32, y: i32, c: Rgb) {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.d[(y * self.w + x) as usize] = [c[0], c[1], c[2], 255];
        }
    }
    fn put_a(&mut self, x: i32, y: i32, c: Rgb, a: u8) {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.d[(y * self.w + x) as usize] = [c[0], c[1], c[2], a];
        }
    }
    /// Only paints where something is already painted.
    fn over(&mut self, x: i32, y: i32, c: Rgb) {
        if self.get(x, y)[3] > 0 {
            self.put(x, y, c);
        }
    }
    pub fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: Rgb) {
        for y in y0..y1 {
            for x in x0..x1 {
                self.put(x, y, c);
            }
        }
    }
    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: Rgb) {
        let n = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
        for i in 0..=n {
            let t = i as f32 / n as f32;
            self.put((x0 as f32 + (x1 - x0) as f32 * t).round() as i32, (y0 as f32 + (y1 - y0) as f32 * t).round() as i32, c);
        }
    }
    /// A hard-edged ellipse shaded in four bands, light from the upper left. `wob` roughens the edge.
    pub fn blob(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, r: &Ramp, seed: u32, wob: f32) {
        let mut rng = Rng(seed.wrapping_mul(2654435761) | 1);
        let (x0, x1, y0, y1) = ((cx - rx - 2.0) as i32, (cx + rx + 2.0) as i32, (cy - ry - 2.0) as i32, (cy + ry + 2.0) as i32);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (nx, ny) = ((x as f32 + 0.5 - cx) / rx, (y as f32 + 0.5 - cy) / ry);
                let e = (rng.below(1000) as f32 / 1000.0 - 0.5) * wob;
                if nx * nx + ny * ny > 1.0 + e {
                    continue;
                }
                let l = -0.55 * nx - 0.8 * ny + (rng.below(1000) as f32 / 1000.0 - 0.5) * 0.22;
                let c = if l > 0.46 {
                    r.hi
                } else if l > -0.12 {
                    r.base
                } else if l > -0.62 {
                    r.sh
                } else {
                    r.dk
                };
                self.put(x, y, c);
            }
        }
    }
    pub fn flat_ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, c: Rgb) {
        for y in (cy - ry - 1.0) as i32..=(cy + ry + 1.0) as i32 {
            for x in (cx - rx - 1.0) as i32..=(cx + rx + 1.0) as i32 {
                let (nx, ny) = ((x as f32 + 0.5 - cx) / rx, (y as f32 + 0.5 - cy) / ry);
                if nx * nx + ny * ny <= 1.0 {
                    self.put(x, y, c);
                }
            }
        }
    }
    /// A box with a lit top-left edge and a shaded bottom-right edge.
    pub fn slab(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, r: &Ramp) {
        self.rect(x0, y0, x1, y1, r.base);
        self.rect(x0, y0, x1, y0 + 1, r.hi);
        self.rect(x0, y0, x0 + 1, y1, r.hi);
        self.rect(x0, y1 - 1, x1, y1, r.sh);
        self.rect(x1 - 1, y0 + 1, x1, y1, r.sh);
    }
    pub fn mirror(&self) -> Bmp {
        let mut m = Bmp::new(self.w, self.h);
        for y in 0..self.h {
            for x in 0..self.w {
                m.d[(y * self.w + x) as usize] = self.get(self.w - 1 - x, y);
            }
        }
        m
    }
    /// One texel of ink around everything painted.
    pub fn outline(&mut self, c: Rgb) {
        let src = self.clone();
        for y in 0..self.h {
            for x in 0..self.w {
                if src.get(x, y)[3] > 0 {
                    continue;
                }
                if [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| src.get(x + dx, y + dy)[3] == 255) {
                    self.put(x, y, c);
                }
            }
        }
    }
    fn speckle(&mut self, rng: &mut Rng, c: Rgb, n: usize) {
        for _ in 0..n {
            let (x, y) = (rng.below(self.w as u32) as i32, rng.below(self.h as u32) as i32);
            self.over(x, y, c);
        }
    }
}

const INK: Rgb = [34, 26, 28];

// ------------------------------------------------------------------ ground tiles

fn wput(b: &mut Bmp, x: i32, y: i32, c: Rgb) {
    b.put(x.rem_euclid(b.w), y.rem_euclid(b.h), c);
}

fn fill(b: &mut Bmp, c: Rgb) {
    let (w, h) = (b.w, b.h);
    b.rect(0, 0, w, h, c);
}

/// A tile texture that fades out in whole-texel dither away from an edge or corner, so one ground type
/// creeps raggedly over the next instead of meeting it along a ruler-straight line.
fn edge_overlay(base: &Bmp, dir: usize, seed: u32) -> Bmp {
    let n = base.w;
    let mut b = Bmp::new(n, n);
    for y in 0..n {
        for x in 0..n {
            let d = match dir {
                0 => y as f32,
                1 => (n - 1 - x) as f32,
                2 => (n - 1 - y) as f32,
                3 => x as f32,
                _ => {
                    let dx = if dir == 4 || dir == 5 { (n - 1 - x) as f32 } else { x as f32 };
                    let dy = if dir == 4 || dir == 7 { y as f32 } else { (n - 1 - y) as f32 };
                    dx.hypot(dy)
                }
            };
            // clumps of texels rather than single ones: the hash is taken on 2x2 cells
            let cover = (1.0 - d / (n as f32 * 0.42)).max(0.0);
            let mut h = ((x / 2) as u32).wrapping_mul(73856093) ^ ((y / 2) as u32).wrapping_mul(19349663) ^ seed.wrapping_mul(83492791);
            h ^= h >> 13;
            h = h.wrapping_mul(0x5bd1e995);
            h ^= h >> 15;
            let r = (h % 1000) as f32 / 1000.0;
            if r < cover * 1.15 {
                let c = base.get(x, y);
                b.d[(y * n + x) as usize] = [c[0], c[1], c[2], 255];
            }
        }
    }
    b
}

/// An organic patch: a short random walk of texels in `col`, wrapping around the tile.
fn clump(b: &mut Bmp, rng: &mut Rng, col: Rgb, cx: i32, cy: i32, n: i32) {
    let (mut x, mut y) = (cx, cy);
    for _ in 0..n {
        wput(b, x, y, col);
        match rng.below(4) {
            0 => x += 1,
            1 => x -= 1,
            2 => y += 1,
            _ => y -= 1,
        }
    }
}

/// Clustered patches: a few organic blobs in `col`.
fn patches(b: &mut Bmp, rng: &mut Rng, col: Rgb, count: i32, size: i32) {
    for _ in 0..count {
        let (cx, cy) = (rng.below(b.w as u32) as i32, rng.below(b.h as u32) as i32);
        clump(b, rng, col, cx, cy, size);
    }
}

/// A grass tuft: three blades fanning out of one dark root, tips lit.
fn tuft(b: &mut Bmp, x: i32, y: i32, dark: Rgb, mid: Rgb, light: Rgb) {
    wput(b, x, y, dark);
    wput(b, x - 1, y, dark);
    wput(b, x, y - 1, mid);
    wput(b, x - 1, y - 1, mid);
    wput(b, x - 2, y - 2, light);
    wput(b, x, y - 2, light);
    wput(b, x + 1, y - 1, mid);
    wput(b, x + 1, y - 2, light);
}

fn tile_grass(seed: u32) -> Bmp {
    let mut rng = Rng(seed * 7919 + 13);
    let r = ramp([88, 136, 62]);
    let mut b = Bmp::new(GT, GT);
    fill(&mut b, r.base);
    // broad soft patches: a slightly deeper green and a sunlit one
    patches(&mut b, &mut rng, mix(r.base, r.sh, 0.55), 2, 34);
    patches(&mut b, &mut rng, mix(r.base, r.hi, 0.45), 2, 22);
    // a few tufts, each a readable little shape instead of loose specks
    for _ in 0..3 + seed as i32 % 2 {
        let (x, y) = (rng.below(GT as u32) as i32, rng.below(GT as u32) as i32);
        tuft(&mut b, x, y, r.dk, r.sh, r.hi);
    }
    match seed % 4 {
        // wild flowers: a few heads of one colour
        1 => {
            let col = [[240, 226, 120], [236, 238, 232], [214, 120, 180]][(seed / 4 % 3) as usize];
            let (x, y) = (rng.below(GT as u32) as i32, rng.below(GT as u32) as i32);
            for (dx, dy) in [(0, 0), (3, 2), (-2, 4)] {
                wput(&mut b, x + dx, y + dy + 1, r.sh);
                wput(&mut b, x + dx, y + dy, col);
            }
        }
        // clover
        3 => {
            let (x, y) = (rng.below(GT as u32) as i32, rng.below(GT as u32) as i32);
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (2, 1), (1, 2)] {
                wput(&mut b, x + dx, y + dy, [70, 120, 54]);
            }
        }
        _ => {}
    }
    b
}

fn tile_forest(seed: u32) -> Bmp {
    let mut rng = Rng(seed * 104729 + 7);
    let r = ramp([56, 96, 50]);
    let mut b = Bmp::new(GT, GT);
    fill(&mut b, r.base);
    // moss and bare earth under the trees
    patches(&mut b, &mut rng, mix(r.base, r.sh, 0.7), 3, 30);
    patches(&mut b, &mut rng, [84, 70, 44], 1, 18);
    patches(&mut b, &mut rng, mix(r.base, r.hi, 0.4), 2, 14);
    // fallen leaves: two-texel leaves in autumn colours
    for _ in 0..5 {
        let (x, y) = (rng.below(GT as u32) as i32, rng.below(GT as u32) as i32);
        let c = [[150, 104, 46], [176, 128, 52], [120, 80, 40]][rng.below(3) as usize];
        wput(&mut b, x, y, c);
        wput(&mut b, x + 1, y, mul(c, 0.8));
    }
    if seed % 2 == 0 {
        // a fallen twig with a fork
        let (x, y) = (rng.below(12) as i32, rng.below(GT as u32) as i32);
        for i in 0..7 {
            wput(&mut b, x + i, y + i / 3, [96, 70, 42]);
        }
        wput(&mut b, x + 4, y, [96, 70, 42]);
        wput(&mut b, x + 5, y - 1, [96, 70, 42]);
    } else {
        // a little mushroom
        let (x, y) = (rng.below(GT as u32) as i32, rng.below(GT as u32) as i32);
        wput(&mut b, x, y, [228, 220, 200]);
        wput(&mut b, x - 1, y - 1, [196, 64, 48]);
        wput(&mut b, x, y - 1, [214, 84, 60]);
        wput(&mut b, x + 1, y - 1, [160, 48, 40]);
    }
    b
}

fn tile_sand(seed: u32) -> Bmp {
    let mut rng = Rng(seed * 15485863 + 3);
    let r = ramp([210, 188, 134]);
    let mut b = Bmp::new(GT, GT);
    fill(&mut b, r.base);
    // wind ripples: two smooth waves that wrap around the tile
    let tau = std::f32::consts::TAU;
    let ph = rng.below(100) as f32 / 100.0 * tau;
    for row in 0..2 {
        let y0 = 4 + row * 10 + rng.below(3) as i32;
        for x in 0..GT {
            let wob = ((x as f32 / GT as f32) * tau + ph + row as f32).sin() * 1.6;
            let y = y0 + wob.round() as i32;
            wput(&mut b, x, y, r.hi);
            wput(&mut b, x, y + 1, mix(r.base, r.sh, 0.6));
        }
    }
    // pebbles: a lit texel over a shadow
    for _ in 0..3 {
        let (x, y) = (rng.below(GT as u32) as i32, rng.below(GT as u32) as i32);
        wput(&mut b, x, y, [176, 158, 128]);
        wput(&mut b, x, y + 1, [140, 120, 92]);
    }
    if seed % 3 == 0 {
        // a shell
        let (x, y) = (rng.below(GT as u32) as i32, rng.below(GT as u32) as i32);
        wput(&mut b, x, y, [242, 232, 218]);
        wput(&mut b, x + 1, y, [226, 200, 186]);
        wput(&mut b, x, y + 1, [180, 160, 150]);
    }
    b
}

fn tile_rock(seed: u32) -> Bmp {
    let mut rng = Rng(seed * 32452843 + 11);
    let r = ramp([128, 126, 122]);
    let mut b = Bmp::new(GT, GT);
    fill(&mut b, r.base);
    patches(&mut b, &mut rng, mix(r.base, r.sh, 0.6), 2, 26);
    // flat slabs set in the ground: lit top edge, shaded lower edge, a gap around them
    for _ in 0..3 {
        let (x, y) = (rng.below(GT as u32) as i32, rng.below(GT as u32) as i32);
        let (w, h) = (4 + rng.below(4) as i32, 3 + rng.below(2) as i32);
        for dy in 0..h {
            for dx in 0..w {
                let corner = (dx == 0 || dx == w - 1) && (dy == 0 || dy == h - 1);
                if corner {
                    continue;
                }
                let c = if dy == 0 { r.hi } else if dy == h - 1 { r.sh } else { mix(r.base, r.hi, 0.3) };
                wput(&mut b, x + dx, y + dy, c);
            }
        }
        for dx in 1..w - 1 {
            wput(&mut b, x + dx, y + h, r.dk);
        }
    }
    if seed % 2 == 1 {
        // a crack with a speck of lichen
        let (mut x, mut y) = (rng.below(GT as u32) as i32, rng.below(8) as i32);
        for _ in 0..11 {
            wput(&mut b, x, y, r.dk);
            x += rng.below(3) as i32 - 1;
            y += 1;
        }
        clump(&mut b, &mut rng, [136, 152, 98], x + 2, y - 3, 4);
    }
    b
}

fn tile_water(seed: u32) -> Bmp {
    let mut rng = Rng(seed * 49979687 + 5);
    let r = ramp([42, 98, 148]);
    let mut b = Bmp::new(GT, GT);
    fill(&mut b, r.base);
    patches(&mut b, &mut rng, mix(r.base, r.sh, 0.6), 2, 26);
    patches(&mut b, &mut rng, [52, 112, 162], 2, 16);
    for _ in 0..2 {
        let (x, y, len) = (rng.below(GT as u32) as i32, rng.below(GT as u32) as i32, 3 + rng.below(4) as i32);
        for i in 0..len {
            wput(&mut b, x + i, y, [118, 176, 212]);
            wput(&mut b, x + i + 1, y + 1, r.sh);
        }
    }
    b
}

// ------------------------------------------------------------------ nature

fn tree(seed: u32) -> Bmp {
    let mut rng = Rng(seed * 2246822519 + 1);
    let mut b = Bmp::new(44, 60);
    let bark = ramp([108, 76, 46]);
    // trunk with roots, drawn from the base (bottom centre) up
    b.slab(19, 38, 25, 56, &bark);
    b.rect(17, 54, 19, 57, bark.sh);
    b.rect(25, 54, 28, 57, bark.sh);
    for y in 40..54 {
        if rng.chance(40) {
            b.put(21 + rng.below(3) as i32, y, bark.dk);
        }
    }
    let leaf = ramp([62, 128, 52]);
    let l2 = ramp([74, 142, 58]);
    b.blob(15.0, 30.0, 11.0, 9.0, &leaf, seed + 1, 0.30);
    b.blob(29.0, 29.0, 11.0, 9.0, &leaf, seed + 2, 0.30);
    b.blob(22.0, 20.0, 14.0, 12.0, &l2, seed + 3, 0.30);
    b.blob(22.0, 31.0, 12.0, 7.0, &leaf, seed + 4, 0.25);
    b.speckle(&mut rng, leaf.hi, 28);
    b.speckle(&mut rng, leaf.dk, 22);
    b.outline(INK);
    b
}

fn stone_rock(seed: u32, ore: Option<Rgb>) -> Bmp {
    let mut rng = Rng(seed * 3266489917 + 9);
    let mut b = Bmp::new(30, 24);
    let r = ramp([138, 136, 134]);
    b.blob(18.0, 15.0, 7.0, 5.5, &ramp([122, 120, 120]), seed + 3, 0.5);
    b.blob(12.0, 14.0, 10.0, 7.5, &r, seed + 1, 0.5);
    b.speckle(&mut rng, r.dk, 12);
    b.speckle(&mut rng, r.hi, 10);
    if let Some(o) = ore {
        let v = ramp(o);
        for (x, y) in [(9, 10), (13, 8), (15, 13), (11, 15), (8, 14)] {
            b.rect(x, y, x + 2, y + 2, v.base);
            b.put(x, y, v.hi);
            b.put(x + 1, y + 1, v.sh);
        }
    }
    b.outline(INK);
    b
}

fn bush(berries: bool) -> Bmp {
    let mut b = Bmp::new(30, 24);
    let g = ramp([58, 120, 50]);
    b.blob(11.0, 14.0, 9.0, 7.0, &g, 3, 0.3);
    b.blob(19.0, 14.0, 9.0, 7.0, &g, 4, 0.3);
    b.blob(15.0, 9.0, 8.0, 6.0, &ramp([68, 134, 54]), 5, 0.3);
    if berries {
        for (x, y) in [(8, 13), (14, 8), (20, 12), (12, 17), (18, 17), (24, 15), (16, 12)] {
            b.rect(x, y, x + 2, y + 2, [196, 32, 52]);
            b.put(x, y, [250, 120, 130]);
        }
    }
    b.outline(INK);
    b
}

fn clay(known: bool) -> Bmp {
    let mut b = Bmp::new(30, 14);
    let c = if known { ramp([158, 104, 72]) } else { ramp([196, 178, 130]) };
    b.blob(14.0, 8.0, 12.0, 4.5, &c, 7, 0.4);
    b.blob(19.0, 7.0, 6.0, 3.0, &c, 8, 0.4);
    b.outline(INK);
    b
}

fn herb(known: bool, hint: bool) -> Bmp {
    let mut b = Bmp::new(18, 16);
    let g = ramp([78, 140, 62]);
    for (i, x) in [3, 6, 9, 12, 14].iter().enumerate() {
        let h = 5 + (i as i32 * 3) % 5;
        b.line(*x, 14, *x + (i as i32 % 2) * 2 - 1, 14 - h, g.base);
        b.put(*x + (i as i32 % 2) * 2 - 1, 14 - h, g.hi);
    }
    if known {
        for (x, y, c) in [(4, 5, [226, 128, 178]), (10, 3, [250, 224, 100]), (13, 7, [226, 128, 178])] {
            b.rect(x, y, x + 2, y + 2, c);
        }
    } else if hint {
        b.put(9, 6, [210, 200, 180]);
    }
    b.outline(INK);
    b
}

fn fiber(known: bool) -> Bmp {
    let mut b = Bmp::new(18, 20);
    let g = if known { ramp([170, 188, 86]) } else { ramp([78, 140, 62]) };
    let hmax = if known { 15 } else { 8 };
    for i in 0..7 {
        let x = 3 + i * 2;
        let h = hmax - (i % 3) * 2;
        b.line(x, 18, x + (i % 3) - 1, 18 - h, if i % 2 == 0 { g.base } else { g.sh });
        b.put(x + (i % 3) - 1, 18 - h, g.hi);
    }
    b.outline(INK);
    b
}

fn deadwood(known: bool) -> Bmp {
    let mut b = Bmp::new(32, 12);
    let w = if known { ramp([120, 84, 50]) } else { ramp([100, 86, 66]) };
    b.rect(3, 6, 28, 9, w.base);
    b.rect(3, 6, 28, 7, w.hi);
    b.rect(3, 8, 28, 9, w.sh);
    b.rect(2, 6, 4, 9, w.dk);
    b.rect(27, 6, 29, 9, w.hi);
    if known {
        b.line(10, 6, 14, 2, w.base);
        b.line(20, 6, 24, 3, w.sh);
    }
    b.outline(INK);
    b
}

// ------------------------------------------------------------------ people

/// Tool held in hand: 0 hand, 1 stone axe, 2 metal axe, 3 metal pick, 4 spear.
fn draw_tool(b: &mut Bmp, tool: usize, hx: i32, hy: i32, dx: i32, dy: i32) {
    // (dx, dy) is the direction the handle points in
    let wood = [128, 90, 52];
    let (len, head): (i32, Option<Rgb>) = match tool {
        1 => (6, Some([150, 148, 146])),
        2 => (7, Some([196, 204, 214])),
        3 => (8, Some([196, 204, 214])),
        4 => (15, Some([210, 214, 220])),
        _ => return,
    };
    let (ex, ey) = (hx + dx * len, hy + dy * len);
    b.line(hx, hy, ex, ey, wood);
    if let Some(h) = head {
        match tool {
            4 => {
                b.put(ex + dx, ey + dy, h);
            }
            1 | 2 => {
                b.rect(ex - 1, ey - 1, ex + 2, ey + 1, h);
                b.put(ex - 1, ey - 1, mul(h, 1.1));
            }
            _ => {
                b.rect(ex - 2, ey - 1, ex + 3, ey, h);
                b.put(ex - 2, ey, mul(h, 0.7));
                b.put(ex + 2, ey, mul(h, 0.7));
            }
        }
    }
}

/// dir: 0 down (toward the viewer), 1 up, 2 left, 3 right. frame: 0 idle, 1 and 2 walk, 3 swing up,
/// 4 swing down, 5 bend.
pub fn human(dir: usize, frame: usize, tool: usize, tunic: Rgb) -> Bmp {
    let mut b = Bmp::new(20, 30);
    let skin = ramp([226, 174, 130]);
    let hair = [78, 52, 34];
    let pants = [96, 70, 50];
    let shoe = [52, 38, 30];
    let tn = ramp(tunic);
    let bob = if frame == 5 { 3 } else { 0 };
    let (lift_l, lift_r) = match frame {
        1 => (2, 0),
        2 => (0, 2),
        _ => (0, 0),
    };
    if dir == 0 || dir == 1 {
        let cx = 10;
        // legs
        for (x, lift) in [(cx - 3, lift_l), (cx + 1, lift_r)] {
            b.rect(x, 20 + bob, x + 3, 28 - lift, pants);
            b.rect(x, 27 - lift, x + 3, 28 - lift, shoe);
        }
        // arms swing opposite to legs
        let (al, ar) = match frame {
            1 => (-1, 1),
            2 => (1, -1),
            _ => (0, 0),
        };
        let raise = frame == 3;
        let down = frame == 4;
        b.rect(cx - 6, 11 + bob + al, cx - 4, 18 + bob + al, skin.base);
        // body
        b.slab(cx - 4, 11 + bob, cx + 4, 20 + bob, &tn);
        b.rect(cx - 4, 18 + bob, cx + 4, 19 + bob, tn.dk);
        // right arm (holds the tool)
        if raise {
            b.rect(cx + 4, 5, cx + 6, 13, skin.base);
        } else if down {
            b.rect(cx + 4, 12 + bob, cx + 6, 19 + bob, skin.base);
        } else {
            b.rect(cx + 4, 11 + bob + ar, cx + 6, 18 + bob + ar, skin.base);
        }
        // head
        b.rect(cx - 3, 3 + bob, cx + 3, 11 + bob, skin.base);
        b.rect(cx - 3, 9 + bob, cx + 3, 11 + bob, skin.sh);
        if dir == 0 {
            b.rect(cx - 3, 3 + bob, cx + 3, 5 + bob, hair);
            b.rect(cx - 3, 5 + bob, cx - 2, 7 + bob, hair);
            b.rect(cx + 2, 5 + bob, cx + 3, 7 + bob, hair);
            b.put(cx - 2, 7 + bob, INK);
            b.put(cx + 1, 7 + bob, INK);
        } else {
            b.rect(cx - 3, 3 + bob, cx + 3, 9 + bob, hair);
        }
        let (hx, hy) = if raise { (cx + 5, 5) } else if down { (cx + 5, 19 + bob) } else { (cx + 5, 18 + bob + ar) };
        let (tdx, tdy) = if raise { (0, -1) } else if down { (0, 1) } else { (0, 1) };
        draw_tool(&mut b, tool, hx, hy, tdx, tdy);
        if dir == 1 {
            b.rect(cx - 4, 11 + bob, cx + 4, 12 + bob, tn.hi);
        }
    } else {
        // facing right; left is the mirror
        let cx = 9;
        let (s1, s2) = match frame {
            1 => (3, -3),
            2 => (-3, 3),
            _ => (0, 0),
        };
        // back leg, front leg: straight down then shifted
        for (shift, col) in [(s2, mul(pants, 0.8)), (s1, pants)] {
            b.rect(cx - 1 + shift / 2, 20 + bob, cx + 2 + shift / 2, 24 + bob, col);
            b.rect(cx - 1 + shift, 24 + bob, cx + 2 + shift, 28, col);
            b.rect(cx + shift, 27, cx + 3 + shift, 28, shoe);
        }
        b.slab(cx - 3, 11 + bob, cx + 3, 20 + bob, &tn);
        b.rect(cx - 3, 18 + bob, cx + 3, 19 + bob, tn.dk);
        // head in profile with a nose
        b.rect(cx - 2, 3 + bob, cx + 3, 11 + bob, skin.base);
        b.put(cx + 3, 7 + bob, skin.base);
        b.rect(cx - 2, 3 + bob, cx + 3, 5 + bob, hair);
        b.rect(cx - 2, 5 + bob, cx, 9 + bob, hair);
        b.put(cx + 1, 7 + bob, INK);
        // arm in front
        let raise = frame == 3;
        let down = frame == 4;
        let (ax, ay0, ay1) = if raise { (cx + 1, 4, 13) } else if down { (cx + 2, 12 + bob, 18 + bob) } else { (cx + s1 / 2 - 1, 11 + bob, 18 + bob) };
        b.rect(ax, ay0, ax + 2, ay1, skin.base);
        let (hx, hy) = if raise { (ax + 1, 4) } else if down { (ax + 1, ay1) } else { (ax + 1, ay1) };
        let (tdx, tdy) = if raise { (0, -1) } else if down { (1, 1) } else { (1, 0) };
        draw_tool(&mut b, tool, hx, hy, tdx, tdy);
    }
    b.outline(INK);
    if dir == 2 {
        b = b.mirror();
    }
    b
}

pub fn bot(frame: usize) -> Bmp {
    let mut b = Bmp::new(18, 26);
    let m = ramp([142, 150, 160]);
    let lift = [0, 2, 0][frame % 3];
    let lift2 = [0, 0, 2][frame % 3];
    b.slab(4, 22 - lift, 8, 26 - lift, &ramp([96, 100, 110]));
    b.slab(10, 22 - lift2, 14, 26 - lift2, &ramp([96, 100, 110]));
    b.slab(3, 10, 15, 22, &m);
    b.slab(5, 2, 13, 10, &m);
    b.rect(7, 5, 11, 7, [255, 214, 90]);
    b.rect(0, 11, 3, 19, m.sh);
    b.rect(15, 11, 18, 19, m.sh);
    for (x, y) in [(5, 13), (12, 13), (5, 19), (12, 19)] {
        b.put(x, y, m.dk);
    }
    b.rect(6, 15, 12, 17, [60, 64, 74]);
    b.outline(INK);
    b
}

// ------------------------------------------------------------------ animals (facing right)

pub fn animal(kind: usize, frame: usize) -> Bmp {
    // 0 dog, 1 ox, 2 horse, 3 wolf, 4 deer
    let (w, h) = [(26, 18), (42, 32), (44, 36), (30, 20), (34, 36)][kind];
    let mut b = Bmp::new(w, h);
    let (lf, lb) = match frame {
        1 => (2, 0),
        2 => (0, 2),
        _ => (0, 0),
    };
    let (body, leg_len, thick): (Ramp, i32, i32) = match kind {
        0 => (ramp([184, 134, 84]), 5, 2),
        1 => (ramp([118, 80, 54]), 9, 3),
        2 => (ramp([152, 100, 62]), 13, 2),
        3 => (ramp([104, 106, 118]), 6, 2),
        _ => (ramp([176, 124, 74]), 13, 2),
    };
    let base_y = h - 2;
    let (bx, by, rx, ry): (f32, f32, f32, f32) = match kind {
        0 => (11.0, base_y as f32 - leg_len as f32 - 3.0, 8.0, 4.0),
        1 => (19.0, base_y as f32 - leg_len as f32 - 6.0, 13.0, 8.0),
        2 => (20.0, base_y as f32 - leg_len as f32 - 5.0, 12.0, 6.0),
        3 => (13.0, base_y as f32 - leg_len as f32 - 3.0, 9.0, 4.0),
        _ => (15.0, base_y as f32 - leg_len as f32 - 4.0, 9.0, 4.5),
    };
    // legs (far pair darker)
    let lx = [bx - rx * 0.6, bx - rx * 0.6 + thick as f32 + 1.0, bx + rx * 0.5, bx + rx * 0.5 + thick as f32 + 1.0];
    for (i, x) in lx.iter().enumerate() {
        let lift = if i % 2 == 0 { lf } else { lb };
        let col = if i % 2 == 0 { body.sh } else { body.base };
        let x = *x as i32;
        b.rect(x, by as i32, x + thick, base_y - lift, col);
        b.rect(x, base_y - lift - 1, x + thick, base_y - lift, body.dk);
    }
    b.blob(bx, by, rx, ry, &body, kind as u32 + 20, 0.2);
    match kind {
        0 => {
            b.blob(19.0, by - 3.0, 4.0, 3.5, &body, 31, 0.1);
            b.rect(22, by as i32 - 3, 25, by as i32 - 1, body.base);
            b.put(24, by as i32 - 3, INK);
            b.rect(17, by as i32 - 7, 19, by as i32 - 4, body.dk);
            b.line(3, by as i32 - 1, 1, by as i32 - 5, body.base);
            b.line(2, by as i32 - 1, 0, by as i32 - 5, body.base);
        }
        1 => {
            b.blob(34.0, by + 2.0, 6.0, 5.5, &body, 32, 0.1);
            b.rect(37, by as i32 + 3, 42, by as i32 + 7, body.sh);
            b.put(38, by as i32 + 1, INK);
            b.line(32, by as i32 - 3, 29, by as i32 - 6, [236, 230, 210]);
            b.line(36, by as i32 - 3, 39, by as i32 - 6, [236, 230, 210]);
            b.line(6, by as i32 - 2, 5, by as i32 + 6, body.dk);
            b.blob(26.0, by - 6.0, 5.0, 3.0, &body, 33, 0.1);
        }
        2 => {
            for i in 0..8 {
                let t = i as f32 / 7.0;
                b.rect((30.0 + t * 3.0) as i32, (by - 3.0 - t * 12.0) as i32, (35.0 + t * 3.0) as i32, (by + 2.0 - t * 12.0) as i32, body.base);
            }
            b.blob(38.0, by - 15.0, 4.5, 3.5, &body, 34, 0.1);
            b.rect(40, by as i32 - 14, 44, by as i32 - 11, body.sh);
            b.put(40, by as i32 - 16, INK);
            b.line(31, by as i32 - 14, 33, by as i32 - 4, [52, 36, 28]);
            b.line(8, by as i32 - 3, 4, by as i32 + 8, [52, 36, 28]);
            b.line(9, by as i32 - 3, 5, by as i32 + 9, [52, 36, 28]);
        }
        3 => {
            b.blob(23.0, by - 2.0, 4.5, 3.5, &body, 35, 0.1);
            b.rect(26, by as i32 - 2, 30, by as i32, body.sh);
            b.put(27, by as i32 - 4, [250, 214, 70]);
            b.rect(21, by as i32 - 8, 23, by as i32 - 5, body.dk);
            b.line(4, by as i32 - 1, 1, by as i32 + 4, body.sh);
            b.line(5, by as i32, 2, by as i32 + 5, body.sh);
        }
        _ => {
            for i in 0..9 {
                let t = i as f32 / 8.0;
                b.rect((22.0 + t * 3.0) as i32, (by - 4.0 - t * 12.0) as i32, (25.0 + t * 3.0) as i32, (by + 1.0 - t * 12.0) as i32, body.base);
            }
            b.blob(29.0, by - 17.0, 3.5, 3.0, &body, 36, 0.1);
            b.rect(31, by as i32 - 17, 34, by as i32 - 14, body.sh);
            b.put(30, by as i32 - 18, INK);
            for (x, y) in [(26, 8), (25, 6), (28, 6), (27, 4), (30, 4)] {
                b.line(x, y, x, y - 2, [226, 214, 190]);
            }
            b.rect(4, by as i32 - 3, 7, by as i32 + 1, [244, 240, 232]);
        }
    }
    b.outline(INK);
    b
}

// ------------------------------------------------------------------ buildings

fn stones_ring(b: &mut Bmp, cx: f32, cy: f32, rx: f32, ry: f32, n: usize) {
    let r = ramp([140, 138, 136]);
    for k in 0..n {
        let a = k as f32 * std::f32::consts::TAU / n as f32;
        b.blob(cx + a.cos() * rx, cy + a.sin() * ry, 3.0, 2.4, &r, 40 + k as u32, 0.3);
    }
}

pub fn campfire() -> Bmp {
    let mut b = Bmp::new(34, 24);
    stones_ring(&mut b, 17.0, 14.0, 10.0, 5.0, 9);
    b.flat_ellipse(17.0, 14.0, 7.0, 3.0, [58, 34, 24]);
    let wood = ramp([112, 78, 44]);
    b.line(9, 16, 25, 11, wood.base);
    b.line(9, 17, 25, 12, wood.sh);
    b.line(10, 11, 24, 17, wood.base);
    b.line(10, 12, 24, 18, wood.sh);
    b.outline(INK);
    b
}

/// Flames: glow overlay, drawn with full brightness.
pub fn flame(frame: usize, scale: i32) -> Bmp {
    let mut b = Bmp::new(14 * scale / 2 + 2, 20 * scale / 2 + 2);
    let hs = [10, 13, 11, 14][frame % 4] * scale / 2;
    let sway = [0, 1, 0, -1][frame % 4];
    let cx = b.w / 2;
    for y in 0..hs {
        let t = y as f32 / hs as f32;
        let wdt = ((1.0 - t) * (3.0 + 2.0 * scale as f32 / 2.0)).max(0.5) as i32;
        let x_c = cx + (sway as f32 * t * 2.0) as i32;
        let yy = b.h - 1 - y;
        let outer = [255, 140, 30];
        let mid = [255, 196, 60];
        let core = [255, 244, 170];
        b.rect(x_c - wdt, yy, x_c + wdt + 1, yy + 1, outer);
        if wdt > 1 {
            b.rect(x_c - wdt + 1, yy, x_c + wdt, yy + 1, mid);
        }
        if wdt > 2 && t < 0.55 {
            b.rect(x_c - wdt + 2, yy, x_c + wdt - 1, yy + 1, core);
        }
    }
    b
}

pub fn shelter() -> Bmp {
    let mut b = Bmp::new(64, 56);
    let wood = ramp([108, 76, 46]);
    // back wall of woven branches and the dark inside
    b.rect(10, 26, 54, 46, [44, 34, 26]);
    for x in (12..54).step_by(4) {
        b.rect(x, 26, x + 1, 46, wood.sh);
    }
    // posts
    for x in [8, 54] {
        b.slab(x, 22, x + 3, 50, &wood);
    }
    for x in [20, 42] {
        b.slab(x, 28, x + 3, 50, &wood);
    }
    // straw bed
    b.rect(14, 44, 50, 49, [186, 156, 84]);
    for x in (14..50).step_by(3) {
        b.put(x, 45, [220, 196, 112]);
    }
    // thatched roof, rows of overlapping bundles
    let thatch = ramp([150, 134, 62]);
    for row in 0..6 {
        let y = 6 + row * 4;
        let inset = (5 - row) * 2;
        b.rect(2 + inset / 2, y, 62 - inset / 2, y + 4, if row % 2 == 0 { thatch.base } else { thatch.sh });
        b.rect(2 + inset / 2, y, 62 - inset / 2, y + 1, thatch.hi);
        for x in ((4 + inset / 2)..(60 - inset / 2)).step_by(5) {
            b.rect(x, y + 1, x + 1, y + 4, thatch.dk);
        }
    }
    b.rect(2, 28, 62, 30, thatch.dk);
    b.outline(INK);
    b
}

pub fn kiln() -> Bmp {
    let mut b = Bmp::new(42, 34);
    let soot = ramp([88, 62, 48]);
    b.blob(21.0, 22.0, 18.0, 11.0, &soot, 50, 0.2);
    b.blob(21.0, 14.0, 11.0, 8.0, &ramp([70, 50, 42]), 51, 0.2);
    b.rect(18, 4, 24, 9, [40, 32, 30]);
    b.rect(18, 4, 24, 5, [90, 78, 70]);
    for (x, y) in [(10, 22), (14, 27), (28, 25), (32, 20), (20, 28)] {
        b.rect(x, y, x + 3, y + 1, soot.dk);
    }
    b.rect(16, 25, 26, 32, [28, 22, 22]);
    b.outline(INK);
    b
}

pub fn furnace() -> Bmp {
    let mut b = Bmp::new(40, 44);
    let st = ramp([136, 132, 128]);
    b.slab(6, 20, 34, 42, &st);
    b.slab(11, 8, 29, 22, &st);
    b.slab(15, 1, 25, 9, &ramp([116, 112, 110]));
    // stone courses
    for y in (24..42).step_by(5) {
        b.rect(6, y, 34, y + 1, st.sh);
    }
    for (i, y) in (24..42).step_by(5).enumerate() {
        for x in ((8 + (i as i32 % 2) * 4)..34).step_by(8) {
            b.rect(x, y, x + 1, y + 5, st.sh);
        }
    }
    b.rect(14, 30, 26, 42, [20, 16, 16]);
    b.rect(13, 29, 27, 30, st.dk);
    b.outline(INK);
    b
}

pub fn depot() -> Bmp {
    let mut b = Bmp::new(46, 44);
    let wood = ramp([120, 84, 48]);
    b.slab(4, 16, 42, 42, &wood);
    for y in (20..42).step_by(5) {
        b.rect(4, y, 42, y + 1, wood.dk);
    }
    b.rect(4, 16, 42, 20, wood.sh);
    b.slab(2, 12, 44, 18, &ramp([96, 66, 40]));
    // sacks on the roof
    let sack = ramp([196, 164, 104]);
    b.blob(13.0, 9.0, 6.0, 5.0, &sack, 60, 0.2);
    b.blob(25.0, 8.0, 6.5, 5.0, &ramp([186, 150, 92]), 61, 0.2);
    b.blob(35.0, 10.0, 5.0, 4.0, &sack, 62, 0.2);
    b.rect(17, 26, 29, 42, wood.dk);
    b.outline(INK);
    b
}

pub fn workshop() -> Bmp {
    let mut b = Bmp::new(54, 50);
    let wall = ramp([150, 142, 130]);
    b.slab(4, 20, 50, 48, &wall);
    for (i, y) in (24..48).step_by(6).enumerate() {
        b.rect(4, y, 50, y + 1, wall.sh);
        for x in ((6 + (i as i32 % 2) * 5)..50).step_by(10) {
            b.rect(x, y, x + 1, y + 6, wall.sh);
        }
    }
    let roof = ramp([84, 66, 60]);
    b.slab(1, 10, 53, 22, &roof);
    for x in (3..52).step_by(6) {
        b.rect(x, 12, x + 1, 22, roof.dk);
    }
    b.slab(38, 0, 46, 12, &ramp([120, 116, 114]));
    b.rect(10, 30, 24, 48, [30, 24, 22]);
    b.rect(9, 29, 25, 30, wall.dk);
    b.rect(32, 28, 44, 38, [34, 28, 30]);
    b.outline(INK);
    b
}

pub fn workbench() -> Bmp {
    let mut b = Bmp::new(36, 28);
    let wood = ramp([132, 92, 52]);
    for x in [4, 29] {
        b.slab(x, 14, x + 3, 26, &ramp([100, 70, 42]));
    }
    b.slab(2, 8, 34, 15, &wood);
    b.rect(2, 11, 34, 12, wood.sh);
    // tools: a hammer, a stone, a coil of fibre
    b.rect(8, 4, 17, 6, [128, 90, 52]);
    b.rect(14, 2, 19, 6, [150, 148, 146]);
    b.blob(26.0, 6.0, 3.5, 2.5, &ramp([140, 138, 136]), 70, 0.2);
    b.outline(INK);
    b
}

pub fn water_wheel(frame: usize) -> Bmp {
    let mut b = Bmp::new(50, 48);
    let wood = ramp([128, 90, 50]);
    b.slab(6, 34, 44, 46, &ramp([108, 76, 46]));
    let (cx, cy, r) = (25.0f32, 20.0f32, 17.0f32);
    for k in 0..8 {
        let a = k as f32 * std::f32::consts::TAU / 8.0 + frame as f32 * 0.19634;
        for i in 0..=17 {
            b.put((cx + a.cos() * i as f32).round() as i32, (cy + a.sin() * i as f32).round() as i32, wood.base);
        }
        let (px, py) = ((cx + a.cos() * r).round() as i32, (cy + a.sin() * r).round() as i32);
        b.rect(px - 2, py - 1, px + 2, py + 2, wood.sh);
    }
    for k in 0..48 {
        let a = k as f32 * std::f32::consts::TAU / 48.0;
        b.put((cx + a.cos() * r).round() as i32, (cy + a.sin() * r).round() as i32, wood.hi);
    }
    b.flat_ellipse(cx, cy, 3.0, 3.0, [96, 96, 108]);
    b.outline(INK);
    b
}

/// Soft shadow on the ground: dithered so it stays pixel art.
pub fn shadow(rx: f32, ry: f32) -> Bmp {
    let (w, h) = (rx as i32 * 2 + 4, ry as i32 * 2 + 4);
    let mut b = Bmp::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let (nx, ny) = ((x as f32 + 0.5 - w as f32 / 2.0) / rx, (y as f32 + 0.5 - h as f32 / 2.0) / ry);
            let d = nx * nx + ny * ny;
            if d <= 1.0 && (d < 0.55 || (x + y) % 2 == 0) {
                b.put_a(x, y, [8, 10, 24], 64);
            }
        }
    }
    b
}

/// A pool of firelight: bands that get fainter outward, dithered.
pub fn halo(rx: f32, ry: f32) -> Bmp {
    let (w, h) = (rx as i32 * 2 + 4, ry as i32 * 2 + 4);
    let mut b = Bmp::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let (nx, ny) = ((x as f32 + 0.5 - w as f32 / 2.0) / rx, (y as f32 + 0.5 - h as f32 / 2.0) / ry);
            let d = (nx * nx + ny * ny).sqrt();
            let a = if d < 0.4 {
                64
            } else if d < 0.7 {
                if (x + y) % 2 == 0 { 64 } else { 34 }
            } else if d < 1.0 {
                if (x + y) % 2 == 0 { 34 } else { 0 }
            } else {
                0
            };
            if a > 0 {
                b.put_a(x, y, [255, 170, 70], a);
            }
        }
    }
    b
}


fn mtile(f: impl Fn(&mut Rng, i32, i32) -> [f32; 3], seed: u32) -> Bmp {
    let mut rng = Rng(seed * 2654435761 + 17);
    let mut b = Bmp::new(TILE, TILE);
    for y in 0..TILE {
        for x in 0..TILE {
            let m = f(&mut rng, x, y);
            b.put(x, y, std::array::from_fn(|i| (m[i] * 128.0).round().clamp(0.0, 255.0) as u8));
        }
    }
    b
}

/// 0 wood and bark: vertical grain with dark gaps and a few knots.
fn mat_wood() -> Bmp {
    let col: Vec<f32> = (0..16).map(|x| [0.92, 1.04, 0.86, 1.10, 0.96, 0.82, 1.06, 0.98][(x * 5 % 8) as usize]).collect();
    mtile(
        |r, x, y| {
            let mut k = col[x as usize];
            if (x * 7 + y / 5) % 9 == 0 {
                k *= 0.72;
            }
            if r.chance(5) {
                k *= 1.2;
            }
            if (x == 5 && y == 6) || (x == 6 && y == 6) || (x == 5 && y == 7) || (x == 12 && y == 12) {
                k = 0.6;
            }
            [k * 1.03, k, k * 0.95]
        },
        1,
    )
}

/// 1 leaves: blotches, light on the upper left of each dab, dark under it.
fn mat_leaves() -> Bmp {
    let mut dabs = vec![[0u8; 16]; 16];
    let mut rng = Rng(77);
    for _ in 0..13 {
        let (x, y) = (rng.below(16) as usize, rng.below(16) as usize);
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            dabs[(y + dy) % 16][(x + dx) % 16] = if dx + dy == 0 { 2 } else if dx + dy == 2 { 3 } else { 1 };
        }
    }
    mtile(
        move |r, x, y| match dabs[y as usize][x as usize] {
            2 => [1.22, 1.24, 0.82],
            3 => [0.74, 0.86, 0.80],
            1 => [1.0, 1.02, 0.94],
            _ => {
                if r.chance(14) { [0.86, 0.94, 0.88] } else { [0.97, 1.0, 0.95] }
            }
        },
        2,
    )
}

/// 2 stone: pits, cracks and lit chips.
fn mat_stone() -> Bmp {
    let mut crack = vec![vec![false; 16]; 16];
    let mut rng = Rng(91);
    for _ in 0..3 {
        let (mut x, mut y) = (rng.below(16) as usize, rng.below(16) as usize);
        for _ in 0..6 {
            crack[y % 16][x % 16] = true;
            if rng.chance(50) { x += 1 } else { y += 1 }
        }
    }
    mtile(
        move |r, x, y| {
            if crack[y as usize][x as usize] {
                [0.62, 0.62, 0.70]
            } else if r.chance(8) {
                [1.22, 1.20, 1.14]
            } else if r.chance(10) {
                [0.84, 0.84, 0.9]
            } else {
                [1.0, 1.0, 1.0]
            }
        },
        3,
    )
}

/// 3 skin, hide and cloth: a woven look with flecks.
fn mat_cloth() -> Bmp {
    mtile(
        |r, x, y| {
            let k = if (x / 2 + y / 2) % 2 == 0 { 1.07 } else { 0.94 };
            let k = if r.chance(7) { k * 0.82 } else { k };
            [k, k, k * 0.98]
        },
        4,
    )
}

fn write_material_tiles(data: &mut [u8]) {
    for (i, t) in [mat_wood(), mat_leaves(), mat_stone(), mat_cloth()].iter().enumerate() {
        for y in 0..TILE {
            for x in 0..TILE {
                let p = (((MAT_Y + y as u32) * ATLAS + i as u32 * 16 + x as u32) * 4) as usize;
                data[p..p + 4].copy_from_slice(&t.d[(y * TILE + x) as usize]);
            }
        }
    }
}

// ------------------------------------------------------------------ atlas

pub struct Art {
    pub grass: [Spr; 4],
    pub forest: [Spr; 4],
    pub sand: [Spr; 4],
    pub rock: [Spr; 4],
    pub water: [Spr; 4],
    /// [terrain][direction]: the terrain's own texture, dithered away from one side or corner, laid over a lower neighbour
    /// directions: N, E, S, W edges, then NE, SE, SW, NW corners (N = smaller z)
    pub edge: Vec<Spr>,
    pub trees: [Spr; 3],
    pub stone: [Spr; 3],
    /// iron, copper, tin
    pub ore: [Spr; 3],
    pub ore_hint: Spr,
    pub bush: Spr,
    pub clay: [Spr; 2],
    pub herb: [Spr; 3],
    pub fiber: [Spr; 2],
    pub dead: [Spr; 2],
    /// [tool][dir][frame]
    pub human: Vec<Spr>,
    pub human_other: Vec<Spr>,
    pub bot: [Spr; 3],
    /// [kind][frame]
    pub animal: Vec<Spr>,
    pub campfire: Spr,
    pub flame: [Spr; 4],
    pub flame_big: [Spr; 4],
    pub shelter: Spr,
    pub kiln: Spr,
    pub furnace: Spr,
    pub depot: Spr,
    pub workshop: Spr,
    pub workbench: Spr,
    pub wheel: [Spr; 8],
    pub shadow_s: Spr,
    pub shadow_m: Spr,
    pub shadow_l: Spr,
    pub halo: Spr,
    /// one white texel, for solid rectangles
    pub white: Spr,
}

pub struct Atlas {
    pub data: Vec<u8>,
    cx: u32,
    cy: u32,
    row_h: u32,
}

impl Atlas {
    fn new() -> Self {
        Atlas { data: vec![0; (ATLAS * ATLAS * 4) as usize], cx: 1, cy: 1, row_h: 0 }
    }
    /// Pivot: (px from left, py from bottom).
    fn add(&mut self, b: &Bmp, px: i32, py: i32) -> Spr {
        let (w, h) = (b.w as u32, b.h as u32);
        if self.cx + w + 1 > ATLAS {
            self.cx = 1;
            self.cy += self.row_h + 1;
            self.row_h = 0;
        }
        assert!(self.cy + h < MAT_Y, "sprite atlas is full");
        for y in 0..h {
            for x in 0..w {
                let i = (((self.cy + y) * ATLAS + self.cx + x) * 4) as usize;
                self.data[i..i + 4].copy_from_slice(&b.d[(y * w + x) as usize]);
            }
        }
        let s = Spr { x: self.cx as u16, y: self.cy as u16, w: w as u16, h: h as u16, px: px as i16, py: py as i16 };
        self.cx += w + 1;
        self.row_h = self.row_h.max(h);
        s
    }
    /// Pivot at the bottom centre.
    fn add_foot(&mut self, b: &Bmp) -> Spr {
        self.add(b, b.w / 2, 0)
    }
}

pub fn build() -> (Art, Atlas) {
    let mut a = Atlas::new();
    let tiles = |f: fn(u32) -> Bmp, a: &mut Atlas| -> [Spr; 4] { std::array::from_fn(|i| a.add(&f(i as u32 + 1), 0, 0)) };
    let grass = tiles(tile_grass, &mut a);
    let forest = tiles(tile_forest, &mut a);
    let sand = tiles(tile_sand, &mut a);
    let rock = tiles(tile_rock, &mut a);
    let water = tiles(tile_water, &mut a);
    let edge: Vec<Spr> = {
        let bases = [tile_water(1), tile_sand(1), tile_grass(1), tile_forest(1), tile_rock(1)];
        let mut v = Vec::new();
        for (t, base) in bases.iter().enumerate() {
            for dir in 0..8 {
                v.push(a.add(&edge_overlay(base, dir, t as u32 * 8 + dir as u32 + 1), 0, 0));
            }
        }
        v
    };
    let trees = std::array::from_fn(|i| a.add_foot(&tree(i as u32 * 7 + 1)));
    let stone = std::array::from_fn(|i| a.add_foot(&stone_rock(i as u32 * 5 + 2, None)));
    let ore = [[188, 100, 52], [58, 190, 150], [226, 230, 240]].map(|c: Rgb| a.add_foot(&stone_rock(4, Some(c))));
    let ore_hint = {
        let mut b = stone_rock(4, None);
        b.put(10, 11, [220, 200, 170]);
        a.add_foot(&b)
    };
    let bush_s = a.add_foot(&bush(true));
    let clay_s = [a.add_foot(&clay(false)), a.add_foot(&clay(true))];
    let herb_s = [a.add_foot(&herb(false, false)), a.add_foot(&herb(false, true)), a.add_foot(&herb(true, false))];
    let fiber_s = [a.add_foot(&fiber(false)), a.add_foot(&fiber(true))];
    let dead_s = [a.add_foot(&deadwood(false)), a.add_foot(&deadwood(true))];
    let mut human_s = Vec::new();
    let mut human_o = Vec::new();
    for tool in 0..5 {
        for dir in 0..4 {
            for frame in 0..6 {
                human_s.push(a.add_foot(&human(dir, frame, tool, [168, 124, 76])));
                human_o.push(a.add_foot(&human(dir, frame, tool, [76, 116, 170])));
            }
        }
    }
    let bot_s = std::array::from_fn(|i| a.add_foot(&bot(i)));
    let mut animal_s = Vec::new();
    for kind in 0..5 {
        for frame in 0..3 {
            animal_s.push(a.add(&animal(kind, frame), animal(kind, 0).w / 2, 1));
        }
    }
    let campfire_s = a.add(&campfire(), 17, 5);
    let flame_s = std::array::from_fn(|i| a.add_foot(&flame(i, 2)));
    let flame_big_s = std::array::from_fn(|i| a.add_foot(&flame(i, 4)));
    let shelter_s = a.add_foot(&shelter());
    let kiln_s = a.add_foot(&kiln());
    let furnace_s = a.add_foot(&furnace());
    let depot_s = a.add_foot(&depot());
    let workshop_s = a.add_foot(&workshop());
    let workbench_s = a.add_foot(&workbench());
    let wheel_s = std::array::from_fn(|i| a.add_foot(&water_wheel(i)));
    let sh = |a: &mut Atlas, rx: f32, ry: f32| {
        let b = shadow(rx, ry);
        a.add(&b, b.w / 2, b.h / 2)
    };
    let shadow_s = sh(&mut a, 6.0, 2.5);
    let shadow_m = sh(&mut a, 11.0, 4.0);
    let shadow_l = sh(&mut a, 22.0, 7.0);
    let halo_s = {
        let b = halo(56.0, 36.0);
        a.add(&b, b.w / 2, b.h / 2)
    };
    let mut wb = Bmp::new(2, 2);
    wb.rect(0, 0, 2, 2, [255, 255, 255]);
    let white = a.add(&wb, 0, 0);
    write_material_tiles(&mut a.data);
    (
        Art {
            grass,
            forest,
            sand,
            rock,
            water,
            edge,
            trees,
            stone,
            ore,
            ore_hint,
            bush: bush_s,
            clay: clay_s,
            herb: herb_s,
            fiber: fiber_s,
            dead: dead_s,
            human: human_s,
            human_other: human_o,
            bot: bot_s,
            animal: animal_s,
            campfire: campfire_s,
            flame: flame_s,
            flame_big: flame_big_s,
            shelter: shelter_s,
            kiln: kiln_s,
            furnace: furnace_s,
            depot: depot_s,
            workshop: workshop_s,
            workbench: workbench_s,
            wheel: wheel_s,
            shadow_s,
            shadow_m,
            shadow_l,
            halo: halo_s,
            white,
        },
        a,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ALAZ_ATLAS_PNG=/path/atlas.png cargo test -p alaz-render dump_atlas` writes the atlas, 4x enlarged, to look at.
    #[test]
    fn dump_atlas() {
        let (_, a) = build();
        let Some(path) = std::env::var_os("ALAZ_ATLAS_PNG") else { return };
        let k = 3usize;
        let n = ATLAS as usize;
        let rows = (0..n).rev().find(|&y| (0..n).any(|x| a.data[(y * n + x) * 4 + 3] > 0)).unwrap_or(0) + 2;
        let (cw, ch) = (n * k, rows * k);
        let mut out = vec![0u8; cw * ch * 4];
        for y in 0..ch {
            for x in 0..cw {
                let s = ((y / k) * n + x / k) * 4;
                let px = &a.data[s..s + 4];
                // composite on a dark checker so transparent areas read as such
                let bg = if ((x / 12) + (y / 12)) % 2 == 0 { 70u32 } else { 90 };
                let al = px[3] as u32;
                for c in 0..3 {
                    out[(y * cw + x) * 4 + c] = ((px[c] as u32 * al + bg * (255 - al)) / 255) as u8;
                }
                out[(y * cw + x) * 4 + 3] = 255;
            }
        }
        let file = std::fs::File::create(path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), cw as u32, ch as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&out).unwrap();
    }
}
