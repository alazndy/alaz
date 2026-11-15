//! Pixel art: every ground tile and sprite is painted here, in code, into one atlas.
//! 16 texels = 1 metre. Nothing is smoothed anywhere: what is painted is what is shown.

pub type Rgb = [u8; 3];

/// Texels per metre in the world.
pub const PPM: f32 = 16.0;
pub const ATLAS: u32 = 1024;
pub const TILE: i32 = 16;
/// Ground tiles are finer than the painted sprites: 27 texels to the metre (21 in the coarse picture), as close as
/// whole texels get to the picture's own 16 * 5/3 (16 * 4/3), so a ground texel is one picture texel and they all
/// come out the same width.
pub fn gt() -> i32 {
    crate::quality::ground_texels()
}
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
}


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
    let mut rng = Rng(seed.wrapping_mul(7919).wrapping_add(13));
    let r = ramp([88, 136, 62]);
    let mut b = Bmp::new(gt(), gt());
    fill(&mut b, r.base);
    // broad soft patches: a slightly deeper green and a sunlit one
    patches(&mut b, &mut rng, mix(r.base, r.sh, 0.55), 2, 34);
    patches(&mut b, &mut rng, mix(r.base, r.hi, 0.45), 2, 22);
    // a few tufts, each a readable little shape instead of loose specks
    for _ in 0..3 + seed as i32 % 2 {
        let (x, y) = (rng.below(gt() as u32) as i32, rng.below(gt() as u32) as i32);
        tuft(&mut b, x, y, r.dk, r.sh, r.hi);
    }
    match seed % 4 {
        // wild flowers: a few heads of one colour
        1 => {
            let col = [[240, 226, 120], [236, 238, 232], [214, 120, 180]][(seed / 4 % 3) as usize];
            let (x, y) = (rng.below(gt() as u32) as i32, rng.below(gt() as u32) as i32);
            for (dx, dy) in [(0, 0), (3, 2), (-2, 4)] {
                wput(&mut b, x + dx, y + dy + 1, r.sh);
                wput(&mut b, x + dx, y + dy, col);
            }
        }
        // clover
        3 => {
            let (x, y) = (rng.below(gt() as u32) as i32, rng.below(gt() as u32) as i32);
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (2, 1), (1, 2)] {
                wput(&mut b, x + dx, y + dy, [70, 120, 54]);
            }
        }
        _ => {}
    }
    b
}

fn tile_forest(seed: u32) -> Bmp {
    let mut rng = Rng(seed.wrapping_mul(104729).wrapping_add(7));
    let r = ramp([56, 96, 50]);
    let mut b = Bmp::new(gt(), gt());
    fill(&mut b, r.base);
    // moss and bare earth under the trees
    patches(&mut b, &mut rng, mix(r.base, r.sh, 0.7), 3, 30);
    patches(&mut b, &mut rng, [84, 70, 44], 1, 18);
    patches(&mut b, &mut rng, mix(r.base, r.hi, 0.4), 2, 14);
    // fallen leaves: two-texel leaves in autumn colours
    for _ in 0..5 {
        let (x, y) = (rng.below(gt() as u32) as i32, rng.below(gt() as u32) as i32);
        let c = [[150, 104, 46], [176, 128, 52], [120, 80, 40]][rng.below(3) as usize];
        wput(&mut b, x, y, c);
        wput(&mut b, x + 1, y, mul(c, 0.8));
    }
    if seed % 2 == 0 {
        // a fallen twig with a fork
        let (x, y) = (rng.below(12) as i32, rng.below(gt() as u32) as i32);
        for i in 0..7 {
            wput(&mut b, x + i, y + i / 3, [96, 70, 42]);
        }
        wput(&mut b, x + 4, y, [96, 70, 42]);
        wput(&mut b, x + 5, y - 1, [96, 70, 42]);
    } else {
        // a little mushroom
        let (x, y) = (rng.below(gt() as u32) as i32, rng.below(gt() as u32) as i32);
        wput(&mut b, x, y, [228, 220, 200]);
        wput(&mut b, x - 1, y - 1, [196, 64, 48]);
        wput(&mut b, x, y - 1, [214, 84, 60]);
        wput(&mut b, x + 1, y - 1, [160, 48, 40]);
    }
    b
}

fn tile_sand(seed: u32) -> Bmp {
    let mut rng = Rng(seed.wrapping_mul(15485863).wrapping_add(3));
    let r = ramp([210, 188, 134]);
    let mut b = Bmp::new(gt(), gt());
    fill(&mut b, r.base);
    // wind ripples: two smooth waves that wrap around the tile
    let tau = std::f32::consts::TAU;
    let ph = rng.below(100) as f32 / 100.0 * tau;
    for row in 0..2 {
        let y0 = 4 + row * 10 + rng.below(3) as i32;
        for x in 0..gt() {
            let wob = ((x as f32 / gt() as f32) * tau + ph + row as f32).sin() * 1.6;
            let y = y0 + wob.round() as i32;
            wput(&mut b, x, y, r.hi);
            wput(&mut b, x, y + 1, mix(r.base, r.sh, 0.6));
        }
    }
    // pebbles: a lit texel over a shadow
    for _ in 0..3 {
        let (x, y) = (rng.below(gt() as u32) as i32, rng.below(gt() as u32) as i32);
        wput(&mut b, x, y, [176, 158, 128]);
        wput(&mut b, x, y + 1, [140, 120, 92]);
    }
    if seed % 3 == 0 {
        // a shell
        let (x, y) = (rng.below(gt() as u32) as i32, rng.below(gt() as u32) as i32);
        wput(&mut b, x, y, [242, 232, 218]);
        wput(&mut b, x + 1, y, [226, 200, 186]);
        wput(&mut b, x, y + 1, [180, 160, 150]);
    }
    b
}

fn tile_rock(seed: u32) -> Bmp {
    let mut rng = Rng(seed.wrapping_mul(32452843).wrapping_add(11));
    let r = ramp([128, 126, 122]);
    let mut b = Bmp::new(gt(), gt());
    fill(&mut b, r.base);
    patches(&mut b, &mut rng, mix(r.base, r.sh, 0.6), 2, 26);
    // flat slabs set in the ground: lit top edge, shaded lower edge, a gap around them
    for _ in 0..3 {
        let (x, y) = (rng.below(gt() as u32) as i32, rng.below(gt() as u32) as i32);
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
        let (mut x, mut y) = (rng.below(gt() as u32) as i32, rng.below(8) as i32);
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
    let mut rng = Rng(seed.wrapping_mul(49979687).wrapping_add(5));
    let r = ramp([42, 98, 148]);
    let mut b = Bmp::new(gt(), gt());
    fill(&mut b, r.base);
    patches(&mut b, &mut rng, mix(r.base, r.sh, 0.6), 2, 26);
    patches(&mut b, &mut rng, [52, 112, 162], 2, 16);
    for _ in 0..2 {
        let (x, y, len) = (rng.below(gt() as u32) as i32, rng.below(gt() as u32) as i32, 3 + rng.below(4) as i32);
        for i in 0..len {
            wput(&mut b, x + i, y, [118, 176, 212]);
            wput(&mut b, x + i + 1, y + 1, r.sh);
        }
    }
    b
}

/// The ground tile `name` number `n` (1..4): the picture `assets/tiles/<name>_<n>.png` when there is one (any square
/// size, enlarged or shrunk to the ground's texel grid without smoothing), else painted by `paint`.
fn painted(name: &str, n: u32, paint: fn(u32) -> Bmp) -> Bmp {
    match tile_file(name, n) {
        Some(b) => b,
        None => paint(n),
    }
}

fn tile_file(name: &str, n: u32) -> Option<Bmp> {
    let img = crate::assets::image(&format!("tiles/{name}_{n}.png"))?;
    let g = gt();
    if img.w == 0 || img.h == 0 {
        return None;
    }
    let mut b = Bmp::new(g, g);
    for y in 0..g {
        for x in 0..g {
            let (sx, sy) = ((x as u32 * img.w / g as u32).min(img.w - 1), (y as u32 * img.h / g as u32).min(img.h - 1));
            let o = ((sy * img.w + sx) * 4) as usize;
            // (ground is opaque)
            b.d[(y * g + x) as usize] = [img.rgba[o], img.rgba[o + 1], img.rgba[o + 2], 255];
        }
    }
    Some(b)
}

// ------------------------------------------------------------------ shadows and materials

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

fn mtile(f: impl Fn(&mut Rng, i32, i32) -> [f32; 3], seed: u32) -> Bmp {
    let mut rng = Rng(seed.wrapping_mul(2654435761).wrapping_add(17));
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
    pub shadow_s: Spr,
    pub shadow_m: Spr,
    pub shadow_l: Spr,
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
    /// Continue packing from here (the rows already laid out stay as they are).
    fn seek(&mut self, x: u32, y: u32) {
        self.cx = x;
        self.cy = y;
        self.row_h = 0;
    }
}

/// The painted ground tiles as `(name_n, picture)`, ignoring any tile files (for the exporter).
pub fn export_tiles() -> Vec<(String, Bmp)> {
    let mut out = Vec::new();
    for (name, f) in [("grass", tile_grass as fn(u32) -> Bmp), ("forest", tile_forest), ("sand", tile_sand), ("rock", tile_rock), ("water", tile_water)] {
        for n in 1..=4 {
            out.push((format!("{name}_{n}"), f(n)));
        }
    }
    out
}

pub fn build() -> (Art, Atlas) {
    let mut a = Atlas::new();
    let tiles = |name: &str, f: fn(u32) -> Bmp, a: &mut Atlas| -> [Spr; 4] { std::array::from_fn(|i| a.add(&painted(name, i as u32 + 1, f), 0, 0)) };
    let grass = tiles("grass", tile_grass, &mut a);
    let forest = tiles("forest", tile_forest, &mut a);
    let sand = tiles("sand", tile_sand, &mut a);
    let rock = tiles("rock", tile_rock, &mut a);
    let water = tiles("water", tile_water, &mut a);
    let edge: Vec<Spr> = {
        let bases = [painted("water", 1, tile_water), painted("sand", 1, tile_sand), painted("grass", 1, tile_grass), painted("forest", 1, tile_forest), painted("rock", 1, tile_rock)];
        let mut v = Vec::new();
        for (t, base) in bases.iter().enumerate() {
            for dir in 0..8 {
                v.push(a.add(&edge_overlay(base, dir, t as u32 * 8 + dir as u32 + 1), 0, 0));
            }
        }
        v
    };
    // (the shadow decals keep the atlas place they had, so every mesh that carries their texture coordinates is unchanged)
    a.seek(790, 288);
    let sh = |a: &mut Atlas, rx: f32, ry: f32| {
        let b = shadow(rx, ry);
        a.add(&b, b.w / 2, b.h / 2)
    };
    let shadow_s = sh(&mut a, 6.0, 2.5);
    let shadow_m = sh(&mut a, 11.0, 4.0);
    let shadow_l = sh(&mut a, 22.0, 7.0);
    a.seek(1000, 288);
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
            shadow_s,
            shadow_m,
            shadow_l,
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

