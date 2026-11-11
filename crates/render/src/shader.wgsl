// Low-poly pixel scene. Models are flat-shaded and cut into a few light bands (cel shading); the ground and decals
// read whole texels from the painted atlas. Nothing is filtered.
struct Globals {
    view_proj: mat4x4<f32>,
    light: vec4<f32>,   // rgb: time-of-day tint, w: perception 0..1
    cam: vec4<f32>,     // xy: ground point the camera looks at
    hwin: vec4<f32>,    // xy: tile corner of the height window's first texel
    env: vec4<f32>,     // x: clock (s), y: wind 0..2, z: rain 0..1, w: snow cover 0..1
    sun_vp: mat4x4<f32>, // world to the sun's shadow map
    sun: vec4<f32>,     // xyz: toward the light, w: shadow strength 0..1
    cloud: vec4<f32>,   // x: cover 0..1, yz: drift, w: the wind's heading (radians, 0 = +x)
    peek: vec4<f32>,    // xyz: middle of the player's body, w: 1 if there is one
    cam_r: vec4<f32>,   // camera right
    cam_u: vec4<f32>,   // camera up (on screen)
    fires: array<vec4<f32>, 8>, // burning fires: xyz position, w reach in metres (0 = none)
    occ: array<vec4<f32>, 16>,  // objects hiding the player: xy anchor, z how far faded open (0..1)
};
@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var atlas: texture_2d<f32>;
// ground height in metres at tile corners, a 256x256 window around the camera
@group(0) @binding(2) var hmap: texture_2d<f32>;
// depth seen from the sun (only bound in the main pass)
@group(0) @binding(3) var shadow_map: texture_depth_2d;
// per tile of the height window: (atlas x, atlas y, ground kind, shade) (main pass only)
@group(0) @binding(4) var tilemap: texture_2d<f32>;

fn corner(i: vec2<i32>) -> vec4<f32> {
    return textureLoad(hmap, clamp(i, vec2<i32>(0), vec2<i32>(255)), 0);
}
fn corner_h(i: vec2<i32>) -> f32 {
    return corner(i).r;
}

// climate at a point: x warmth, y wetness, each 0..1
fn climate_at(p: vec2<f32>) -> vec2<f32> {
    let q = p - globals.hwin.xy;
    let f = floor(q);
    let t = q - f;
    let i = vec2<i32>(f);
    let a = mix(corner(i).gb, corner(i + vec2<i32>(1, 0)).gb, t.x);
    let b = mix(corner(i + vec2<i32>(0, 1)).gb, corner(i + vec2<i32>(1, 1)).gb, t.x);
    return mix(a, b, t.y);
}

// distance to the water's edge in tiles: positive out on the water, negative on land
fn shore_at(p: vec2<f32>) -> f32 {
    let q = p - globals.hwin.xy;
    let f = floor(q);
    let t = q - f;
    let i = vec2<i32>(f);
    let a = mix(corner(i).a, corner(i + vec2<i32>(1, 0)).a, t.x);
    let b = mix(corner(i + vec2<i32>(0, 1)).a, corner(i + vec2<i32>(1, 1)).a, t.x);
    return mix(a, b, t.y);
}

// bilinear ground height at a point on the map (the same surface the terrain tiles are cut from, up to the triangle split)
fn ground_h(p: vec2<f32>) -> f32 {
    let q = p - globals.hwin.xy;
    let f = floor(q);
    let t = q - f;
    let i = vec2<i32>(f);
    let a = mix(corner_h(i), corner_h(i + vec2<i32>(1, 0)), t.x);
    let b = mix(corner_h(i + vec2<i32>(0, 1)), corner_h(i + vec2<i32>(1, 1)), t.x);
    return mix(a, b, t.y);
}

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) glow: f32,
    @location(3) uv: vec2<f32>,
    @location(4) off: vec2<f32>,
    @location(5) kind: f32,
};
struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(flat, first) color: vec3<f32>,
    @location(1) @interpolate(flat, first) glow: f32,
    @location(2) wpos: vec2<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) @interpolate(flat, first) kind: f32,
    @location(5) p3: vec3<f32>,
    @location(6) @interpolate(flat, first) mat: f32,
    @location(7) @interpolate(flat, first) water: f32,
    @location(8) world: vec3<f32>,
    @location(9) @interpolate(flat, first) anchor: vec2<f32>,
};

fn hash21(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}
fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash21(i), hash21(i + vec2<f32>(1.0, 0.0)), u.x), mix(hash21(i + vec2<f32>(0.0, 1.0)), hash21(i + vec2<f32>(1.0, 1.0)), u.x), u.y);
}

// the sea's swell: a few travelling waves of different length and heading (Gerstner-like sums of sines), small in
// the shallows and dying to nothing at the shore so the water still meets the land; the wind raises them.
// returns (height, d height / dx, d height / dz)
fn sea_wave(p: vec2<f32>, t: f32) -> vec3<f32> {
    let r = sea_swell(p, t);
    return vec3<f32>(r[0], r[1], r[2]);
}

// the swell as Gerstner waves: (height, d/dx, d/dz, horizontal pull x, horizontal pull z). The water at a crest is
// drawn together so crests come sharp and troughs broad, as on a real sea. Bigger out in deep water, and the wind
// raises the sea from a gentle roll to a choppy one; it all dies away at the shore so the water meets the land.
fn sea_swell(p: vec2<f32>, t: f32) -> array<f32, 6> {
    let depth = shore_at(p);
    let open_sea = smoothstep(1.5, 8.0, depth);
    let amp = smoothstep(0.0, 2.5, depth) * (0.5 + 1.1 * globals.env.y) * (1.0 + 1.2 * open_sea);
    var h = 0.0;
    var g = vec2<f32>(0.0);
    var pull = vec2<f32>(0.0);
    var most = 0.0;
    // six wave trains that never turn (turning a wave pattern swings it round the far-off world origin); the
    // wind picks which of them run high, so a change of wind is a slow hand-over from one set to another.
    // Each one's phase at the height window's corner comes from the CPU in double precision, so the GPU only
    // ever works with a few hundred metres.
    let ds = array<vec2<f32>, 6>(vec2<f32>(1.0000, 0.0000), vec2<f32>(0.4540, 0.8910), vec2<f32>(-0.4695, 0.8829), vec2<f32>(-0.9994, -0.0349), vec2<f32>(-0.5299, -0.8480), vec2<f32>(0.5150, -0.8572));
    let ls = array<f32, 6>(14.0, 11.0, 8.5, 6.5, 5.3, 3.1);
    let hs = array<f32, 6>(0.17, 0.14, 0.11, 0.08, 0.05, 0.03);
    let off = array<f32, 6>(globals.cam.z, globals.cam.w, globals.hwin.z, globals.hwin.w, globals.cam_r.w, globals.cam_u.w);
    let q = p - globals.hwin.xy;
    let wd = wind_dir();
    for (var i = 0; i < 6; i = i + 1) {
        let d = ds[i];
        let k = 6.2832 / ls[i];
        let ph = k * dot(d, q) + off[i];
        let along = max(dot(d, wd), 0.0);
        let a = hs[i] * amp * (0.12 + 0.88 * along * along) * 1.6;
        h = h + a * sin(ph);
        most = most + a;
        g = g + a * k * cos(ph) * d;
        // steepness: how far the water is drawn toward each crest (kept below breaking)
        pull = pull - d * a * 0.7 * cos(ph);
    }
    return array<f32, 6>(h, g.x, g.y, pull.x, pull.y, most);
}

// which way the wind blows (unit vector on the ground plane, x and z)
fn wind_dir() -> vec2<f32> {
    return vec2<f32>(cos(globals.cloud.w), sin(globals.cloud.w));
}

// kinds: 1 ground tile, 4 model face (uv.x = light level), 5 decal on the ground (shadow, pool of light)
// where a vertex really stands: lifted onto the ground under its anchor, and swaying if it is a leaf or blade
fn place(v: VIn) -> vec3<f32> {
    // everything stands on the ground: models ride up on their anchor point, ground and decals follow the surface
    let anchored = v.kind == 4.0 && (v.off.x != 0.0 || v.off.y != 0.0);
    let lift = ground_h(select(v.pos.xz, v.off, anchored)) + select(0.0, 0.02, anchored);
    // leaves and grass blades lean and swing in the wind: the whole plant by its anchor's phase, more the higher the part
    var sway = vec3<f32>(0.0);
    if (v.kind == 4.0 && v.uv.y > 1.5 && v.uv.y < 2.5 && (v.glow < 0.5 || v.glow > 1.5)) {
        let a = select(v.pos.xz, v.off, anchored);
        let h = max(v.pos.y - 0.1, 0.0);
        // gusts run across the land with the wind: the phase travels downwind
        // (a patch of noise the wind carries along with it: globals.cloud.yz is how far the air has moved)
        let wd = wind_dir();
        let gust = vnoise(a * 0.12 + globals.cloud.yz * 5.45) * 0.7 + vnoise(a * 0.37 + globals.cloud.yz * 16.8) * 0.3;
        let flutter = sin(globals.env.x * 3.1 + hash21(floor(a * 4.0)) * 6.28);
        let amp = 0.03 * (0.4 + globals.env.y) * h;
        // everything leans downwind, more in a strong wind and further as a gust passes; a little flutter on top
        let along = amp * (0.5 * globals.env.y + 1.4 * (gust - 0.35) + 0.15 * flutter);
        let across = amp * 0.2 * flutter;
        sway = vec3<f32>(wd.x * along - wd.y * across, 0.0, wd.y * along + wd.x * across);
    }
    // the sea surface rises and falls with the swell
    var swell = vec3<f32>(0.0);
    if (v.kind == 1.0 && v.off.x > 0.5) {
        let sw = sea_swell(v.pos.xz, globals.env.x);
        swell = vec3<f32>(sw[3], sw[0], sw[4]);
    }
    return vec3<f32>(v.pos.x + sway.x + swell.x, v.pos.y + lift + swell.y, v.pos.z + sway.z + swell.z);
}

// tree crowns are not solid, but the gaps are a few large openings between leaf masses, not a speckle:
// fine noise over every crown tired the eye and gave each little hole its own ink ring
fn leaf_gap(p3: vec3<f32>) -> bool {
    let a = vnoise(vec2<f32>(p3.x + p3.z * 0.6, p3.y * 1.1 - p3.z * 0.5) * 1.6);
    let b = vnoise(vec2<f32>(p3.z - p3.x * 0.4, p3.y * 0.9 + p3.x * 0.7) * 1.3 + vec2<f32>(7.1, 3.3));
    return a * b < 0.07;
}

struct SOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) p3: vec3<f32>,
    @location(1) @interpolate(flat, first) mat: f32,
};

// the sun's view: only solid models cast (the ground, decals and lights do not)
@vertex fn vs_shadow(v: VIn) -> SOut {
    var o: SOut;
    let casts = v.kind == 4.0 && v.glow < 0.5;
    o.clip = select(vec4<f32>(0.0, 0.0, -1.0, 1.0), globals.sun_vp * vec4<f32>(place(v), 1.0), casts);
    o.p3 = vec3<f32>(v.pos.x - v.off.x, v.pos.y, v.pos.z - v.off.y);
    o.mat = v.uv.y;
    return o;
}

@fragment fn fs_shadow(o: SOut) {
    // light falls through the same gaps in the crowns that the eye sees through
    if (o.mat > 1.5 && o.mat < 2.5 && o.p3.y > 1.15 && leaf_gap(o.p3)) { discard; }
}

// 1 where the sun is blocked, 0 where it reaches; outside the shadow box everything is lit
fn sun_shadow(wp: vec3<f32>) -> f32 {
    if (globals.sun.w < 0.01) { return 0.0; }
    let c = globals.sun_vp * vec4<f32>(wp, 1.0);
    let n = c.xyz / c.w;
    let uv = vec2<f32>(n.x * 0.5 + 0.5, 0.5 - n.y * 0.5);
    if (uv.x <= 0.0 || uv.y <= 0.0 || uv.x >= 1.0 || uv.y >= 1.0 || n.z >= 1.0) { return 0.0; }
    let dim = vec2<f32>(textureDimensions(shadow_map));
    let q = vec2<i32>(uv * dim);
    let d = textureLoad(shadow_map, q, 0);
    return select(0.0, 1.0, n.z - 0.0008 > d);
}

// soft cloud shadows drifting over the land
fn cloud_shade(w: vec2<f32>) -> f32 {
    let q = w * 0.022 + globals.cloud.yz;
    let n = vnoise(q) * 0.65 + vnoise(q * 2.3 + vec2<f32>(3.7, 1.9)) * 0.35;
    let cover = globals.cloud.x;
    return smoothstep(0.72 - 0.3 * cover, 0.82 - 0.3 * cover, n);
}

@vertex fn vs(v: VIn) -> VOut {
    return vs_body(v);
}
fn vs_body(v: VIn) -> VOut {
    var o: VOut;
    let wp = place(v);
    o.clip = globals.view_proj * vec4<f32>(wp, 1.0);
    o.world = wp;
    o.anchor = select(vec2<f32>(-1.0e6), v.off, v.kind == 4.0 && (v.off.x != 0.0 || v.off.y != 0.0));
    o.water = select(0.0, 1.0, v.kind == 1.0 && v.off.x > 0.5);
    o.color = v.color;
    o.glow = v.glow;
    o.wpos = v.pos.xz;
    o.uv = v.uv;
    o.kind = v.kind;
    // position relative to the object the face belongs to, so its texture travels with it
    o.p3 = vec3<f32>(v.pos.x - v.off.x * select(1.0, 0.0, v.kind == 1.0), v.pos.y, v.pos.z - v.off.y);
    o.mat = v.uv.y;
    return o;
}

// painted material texel for a model surface: projected along the face's dominant axis, whole texels, 16 per metre
fn material(v: VOut) -> vec3<f32> {
    let n = abs(cross(dpdx(v.p3), dpdy(v.p3)));
    var uv = v.p3.xz;
    if (n.x > n.y && n.x > n.z) {
        uv = v.p3.zy;
    } else if (n.z > n.y) {
        uv = v.p3.xy;
    }
    let t = vec2<i32>(floor(uv * 16.0)) & vec2<i32>(15);
    let m = i32(v.mat + 0.5) - 1;
    // painted as multipliers around 1.0; widened so the grain reads clearly at this resolution
    let k = textureLoad(atlas, vec2<i32>(m * 16 + t.x, 1008 + t.y), 0).rgb * 2.0;
    // leaves get a gentler grain: a crown is a big surface and strong per-texel noise on it is tiring
    let grain = select(2.4, 1.1, m == 1);
    return clamp(vec3<f32>(1.0) + (k - vec3<f32>(1.0)) * grain, vec3<f32>(0.35), vec3<f32>(1.7));
}

// surface direction of the ground at a point, from the height map
fn ground_n(w: vec2<f32>) -> vec3<f32> {
    let e = 0.5;
    let gx = (ground_h(w + vec2<f32>(e, 0.0)) - ground_h(w - vec2<f32>(e, 0.0))) / (2.0 * e);
    let gz = (ground_h(w + vec2<f32>(0.0, e)) - ground_h(w - vec2<f32>(0.0, e))) / (2.0 * e);
    return normalize(vec3<f32>(-gx, 1.0, -gz));
}

// light from the sky and the sun on a surface facing `n`: a cool sky above and warm bounce from the ground below,
// plus the sun (warm and low in the morning and evening, white at noon, blue moonlight at night), which the
// shadows and clouds hold back. About 1.0 on flat ground in full sun.
fn env_light(n: vec3<f32>, sh: f32, cs: f32) -> vec3<f32> {
    let s = clamp(globals.sun.w / 0.8, 0.0, 1.0);
    let sd = normalize(globals.sun.xyz);
    let night = clamp((1.0 - globals.light.r) / 0.7, 0.0, 1.0);
    let low = mix(vec3<f32>(1.0, 0.66, 0.42), vec3<f32>(1.0, 0.95, 0.86), smoothstep(0.25, 0.7, sd.y));
    let sun_col = mix(low, vec3<f32>(0.62, 0.72, 1.0), night);
    let sky = mix(vec3<f32>(0.54, 0.47, 0.40), vec3<f32>(0.66, 0.74, 0.90), n.y * 0.5 + 0.5);
    let amb = sky * (0.62 + 0.4 * (1.0 - s));
    // a low sun is weaker on flat ground, but not as much as it really would be: mornings stay bright and golden
    let lift = 0.91 / max(sd.y, 0.6);
    let direct = sun_col * 0.66 * s * lift * max(dot(n, sd), 0.0) * (1.0 - sh) * (1.0 - 0.5 * cs);
    return amb + direct;
}

// warm light of the fires nearby, flickering; it is not dimmed by the night
fn fire_light(n: vec3<f32>, wp: vec3<f32>) -> vec3<f32> {
    var f = vec3<f32>(0.0);
    // by day the sun drowns it out
    let night = clamp((1.0 - globals.light.r) / 0.7, 0.0, 1.0);
    let k = 0.25 + 0.75 * night;
    for (var i = 0; i < 8; i = i + 1) {
        let fl = globals.fires[i];
        if (fl.w < 0.5) { continue; }
        let d = fl.xyz - wp;
        let dist = length(d);
        if (dist > fl.w) { continue; }
        let att = pow(1.0 - dist / fl.w, 2.0);
        let ndl = max(dot(n, d / max(dist, 0.001)), 0.0) * 0.75 + 0.25;
        let flick = 0.88 + 0.08 * sin(globals.env.x * 13.0 + f32(i) * 2.1) + 0.04 * sin(globals.env.x * 29.0 + f32(i));
        f = f + vec3<f32>(1.0, 0.58, 0.26) * att * ndl * flick * 1.5 * k;
    }
    return f;
}

// cut a light level into whole steps so faces read as flat colour bands, as painted pixel art would
fn quantize(l: vec3<f32>, step: f32) -> vec3<f32> {
    let lum = max(dot(l, vec3<f32>(0.30, 0.59, 0.11)), 0.001);
    let q = max(round(lum / step) * step, step * 0.5);
    return l * (q / lum);
}

@fragment fn fs(v: VOut) -> @location(0) vec4<f32> {
    let p = globals.light.w;
    if (v.kind == 4.0 && v.mat > 1.5 && v.mat < 2.5 && (v.glow < 0.5 || v.glow > 1.5) && v.p3.y > 1.15 && leaf_gap(v.p3)) { discard; }
    // the bright rim around whatever the pointer is on: one flat colour, no light, no haze
    if (v.glow > 1.5) {
        return vec4<f32>(pow(v.color, vec3<f32>(2.2)), 1.0);
    }
    let tex = select(vec3<f32>(1.0), material(v), v.mat > 0.5 && v.kind == 4.0);
    // the face's own direction, turned toward the camera
    var nm = normalize(cross(dpdx(v.world), dpdy(v.world)));
    let to_cam = -cross(globals.cam_u.xyz, globals.cam_r.xyz);
    if (dot(nm, to_cam) < 0.0) { nm = -nm; }
    var n = vec3<f32>(0.0, 1.0, 0.0);
    var col = v.color;
    var alpha = 1.0;
    if (v.kind == 1.0) {
        let clock = globals.env.x;
        let wnd = globals.env.y;
        let rain = globals.env.z;
        let snow = globals.env.w;
        let w = v.wpos;
        let tl = min(floor(fract(w) * 21.0), vec2<f32>(20.0));
        // near the shore, water or land is decided by the smooth shore field, not by whole tiles: a coast that runs
        // across the grid is a curve, not a staircase of tile corners
        let sh_here = shore_at(floor(w * 21.0) / 21.0);
        let wet_here = select(v.water > 0.5, sh_here > 0.0, abs(sh_here) < 1.2);
        if (wet_here) {
            // water, drawn on the ground's own texel grid: deep blue out in the open, turquoise in the shallows,
            // wave bands drifting with the wind, foam lapping at the shore and the sun sparkling on the ripples
            let q = floor(w * 21.0) / 21.0;
            let depth = shore_at(q);
            let deep = smoothstep(0.0, 3.5, depth);
            col = mix(vec3<f32>(0.22, 0.58, 0.62), vec3<f32>(0.13, 0.36, 0.56), smoothstep(0.0, 1.2, depth));
            col = mix(col, vec3<f32>(0.08, 0.22, 0.42), smoothstep(1.2, 4.0, depth));
            // drifting swell: two moving noise fields; where they cross, a lighter band, where they part, a darker one
            // the surface ripples are carried downwind by how far the air has moved
            let drift = globals.cloud.yz * 4.0;
            let n1 = vnoise(q * vec2<f32>(0.9, 1.7) + drift);
            let n2 = vnoise(q * vec2<f32>(1.6, 0.8) - drift * 1.3 + vec2<f32>(5.2, 1.3));
            // the swell: lit by its own slope, the crests whitening where the wind is up
            let sw6 = sea_swell(w, clock);
            let sw = vec3<f32>(sw6[0], sw6[1], sw6[2]);
            n = normalize(vec3<f32>(-sw.y, 1.0, -sw.z));
            // how near this point is to the highest the waves here can reach (0..1)
            let crest = sw.x / max(sw6[5], 0.01);
            // surf: breakers rolling in toward the beach, each a foam band that runs up the sand and drains back
            let surf_ph = fract(depth * 0.55 + clock * 0.12 + vnoise(q * 0.3) * 0.5);
            if (depth < 2.2 && depth > 0.15 && surf_ph < 0.06 + 0.05 * wnd) {
                col = mix(col, vec3<f32>(0.9, 0.96, 0.97), 0.75 * (1.0 - depth / 2.2));
            }
            // the side of each wave facing the sun lighter, the far side darker: shows its shape
            col = col * (1.0 + 0.9 * dot(vec2<f32>(sw.y, sw.z), normalize(globals.sun.xz + vec2<f32>(0.001))));
            // whitecaps only on the highest crests, and only once the wind is fresh
            if (crest > 0.8 && wnd > 0.7 && deep > 0.3 && hash21(floor(w * 21.0) + floor(clock * 4.0)) > 0.9 - 0.2 * (wnd - 0.7)) {
                col = mix(col, vec3<f32>(0.88, 0.95, 0.97), 0.7);
            }
            let band = abs(n1 - n2);
            if (band < 0.05) { col = col * 1.16 + vec3<f32>(0.03, 0.05, 0.05); }
            else if (band > 0.42) { col = col * 0.86; }
            // the sea floor shows through the shallows
            if (depth < 0.9 && vnoise(q * 3.0) > 0.62) { col = mix(col, vec3<f32>(0.46, 0.62, 0.52), 0.35 * (1.0 - depth / 0.9)); }
            // foam: a line that runs up the shore and back, and a fainter one further out
            let wave = sin(clock * 1.1 + vnoise(q * 0.6) * 6.0) * 0.5 + 0.5;
            let line1 = 0.06 + 0.32 * wave;
            if (abs(depth - line1) < 0.07 || depth < line1 - 0.07) {
                if (depth < line1 + 0.07) { col = mix(col, vec3<f32>(0.86, 0.94, 0.95), select(0.85, 0.55, depth < line1 - 0.07)); }
            }
            let line2 = 0.55 + 0.35 * fract(clock * 0.16 + vnoise(q * 0.4) * 2.0);
            if (abs(depth - line2) < 0.045 && hash21(floor(w * 21.0)) > 0.35) { col = mix(col, vec3<f32>(0.75, 0.88, 0.92), 0.45 * (1.0 - deep)); }
            // sun glitter: a ripple surface reflecting the sun toward the eye, cut to single bright texels
            let sd = normalize(globals.sun.xyz);
            let e = 0.12;
            let nx = vnoise((q + vec2<f32>(e, 0.0)) * 2.7 + drift * 2.0) - vnoise((q - vec2<f32>(e, 0.0)) * 2.7 + drift * 2.0);
            let nz = vnoise((q + vec2<f32>(0.0, e)) * 2.7 - drift * 1.7) - vnoise((q - vec2<f32>(0.0, e)) * 2.7 - drift * 1.7);
            let rn = normalize(vec3<f32>(-nx * 2.2 - sw.y * 3.0, 1.0, -nz * 2.2 - sw.z * 3.0));
            let to_eye = -cross(globals.cam_u.xyz, globals.cam_r.xyz);
            let spec = pow(max(dot(reflect(-sd, rn), to_eye), 0.0), 40.0) * clamp(globals.sun.w / 0.8, 0.0, 1.0);
            if (spec > 0.35) { col = col + vec3<f32>(0.55, 0.52, 0.42) * select(0.5, 1.0, spec > 0.7); }
            // raindrops: rings that widen and fade
            if (rain > 0.05) {
                let cell = floor(w * 1.5);
                let hh = fract(sin(dot(cell, vec2<f32>(12.9898, 78.233))) * 43758.5453);
                let hy = fract(hh * 91.7);
                let phase = fract(clock * 0.8 + hh * 7.0);
                let c0 = (cell + vec2<f32>(0.2 + 0.6 * hh, 0.2 + 0.6 * hy)) / 1.5;
                if (hh < rain * 0.7) {
                    let dd = abs(length(q - c0) - phase * 0.33);
                    if (dd < 0.035) { col = col + vec3<f32>(0.35, 0.40, 0.45) * (1.0 - phase); }
                }
            }
        } else {
            col = textureLoad(atlas, vec2<i32>(floor(v.uv + tl)), 0).rgb * v.color;
            // ground boundaries wander: each texel looks a little way off along a slow noise and, if that falls on a
            // tile of another kind of land, wears that tile's ground; edges become ragged lines, not tile staircases
            if (v.water < 0.5) {
                let q = floor(w * 21.0) / 21.0;
                let jit = vec2<f32>(vnoise(q * 1.7 + vec2<f32>(17.3, 4.1)), vnoise(q * 1.7 + vec2<f32>(3.7, 91.2))) - vec2<f32>(0.5);
                let fine = vec2<f32>(vnoise(q * 6.0 + vec2<f32>(51.0, 7.0)), vnoise(q * 6.0 + vec2<f32>(9.0, 33.0))) - vec2<f32>(0.5);
                let wl = q + jit * 0.9 + fine * 0.25;
                let own = floor(w);
                let other = floor(wl);
                if (other.x != own.x || other.y != own.y) {
                    let lim = vec2<i32>(255);
                    let mine = textureLoad(tilemap, clamp(vec2<i32>(own - globals.hwin.xy), vec2<i32>(0), lim), 0);
                    let them = textureLoad(tilemap, clamp(vec2<i32>(other - globals.hwin.xy), vec2<i32>(0), lim), 0);
                    // land over land only (the water's edge has its own smooth line)
                    if (them.z > 0.5 && them.z != mine.z) {
                        col = textureLoad(atlas, vec2<i32>(floor(them.xy + tl)), 0).rgb * vec3<f32>(them.w);
                    }
                }
            }
            // the land edge of a water tile: its own picture is water, so it is drawn as wet sand
            if (v.water > 0.5) {
                col = vec3<f32>(0.80, 0.70, 0.48) * (0.93 + 0.07 * hash21(floor(w * 21.0)));
            }
            // the ground right by the water is wet and dark, in a band that the waves keep washing
            let sh0 = shore_at(floor(w * 21.0) / 21.0);
            if (sh0 > -0.55) { col = col * mix(0.72, 1.0, smoothstep(-0.25, -0.55, sh0)); }
            // the shape of the land: which way it faces
            let e = 0.5;
            let gx = (ground_h(w + vec2<f32>(e, 0.0)) - ground_h(w - vec2<f32>(e, 0.0))) / (2.0 * e);
            let gz = (ground_h(w + vec2<f32>(0.0, e)) - ground_h(w - vec2<f32>(0.0, e))) / (2.0 * e);
            n = normalize(vec3<f32>(-gx, 1.0, -gz));
            let alt = ground_h(w);
            let steep = clamp(length(vec2<f32>(gx, gz)) * 2.5, 0.0, 1.0);
            let lum0 = dot(col, vec3<f32>(0.30, 0.59, 0.11));
            let green = clamp((col.g - max(col.r, col.b) * 0.95) * 6.0, 0.0, 1.0);
            // biomes: warmth and wetness pick the country: tundra, steppe, jungle, autumn wood, rich meadow
            let cl = climate_at(w);
            let cold = smoothstep(0.40, 0.18, cl.x);
            let hot = smoothstep(0.58, 0.80, cl.x);
            let wet = smoothstep(0.48, 0.72, cl.y);
            let dry = smoothstep(0.52, 0.28, cl.y);
            let steppe = clamp(hot * dry * 1.6 + dry * 0.35, 0.0, 1.0) * green * (1.0 - cold);
            let jungle = hot * wet * green;
            let autumn = smoothstep(0.30, 0.45, cl.x) * smoothstep(0.62, 0.50, cl.x) * smoothstep(0.35, 0.55, cl.y) * green;
            let meadow = wet * (1.0 - hot) * (1.0 - cold) * green;
            col = mix(col, vec3<f32>(lum0 * 1.75, lum0 * 1.5, lum0 * 0.62), steppe * 0.75);
            col = mix(col, vec3<f32>(lum0 * 0.62, lum0 * 1.55, lum0 * 0.55), jungle * 0.6);
            col = mix(col, vec3<f32>(lum0 * 1.8, lum0 * 1.05, lum0 * 0.45), autumn * 0.7);
            col = mix(col, vec3<f32>(lum0 * 0.82, lum0 * 1.72, lum0 * 0.62), meadow * 0.35);
            // desert: hot and dry enough that the grass gives way to bare sand and cracked earth
            let tx0 = floor(w * 21.0) / 21.0;
            let desert = smoothstep(0.62, 0.70, cl.x) * smoothstep(0.38, 0.30, cl.y);
            let dune = vnoise(tx0 * vec2<f32>(0.9, 2.6) + vec2<f32>(3.1, 0.7));
            col = mix(col, vec3<f32>(0.80, 0.66, 0.42) * (0.92 + 0.16 * dune), desert * (0.35 + 0.6 * green));
            if (desert > 0.5 && vnoise(tx0 * 3.1) > 0.78 && vnoise(tx0 * 7.3 + vec2<f32>(1.3, 4.1)) > 0.55) { col = col * 0.82; }
            // swamp: wet, mild and low; dark olive moss with pools of standing water
            let swamp = smoothstep(0.66, 0.74, cl.y) * smoothstep(0.36, 0.44, cl.x) * smoothstep(0.64, 0.56, cl.x) * smoothstep(3.0, 1.5, ground_h(w)) * green;
            col = mix(col, vec3<f32>(lum0 * 0.85, lum0 * 1.05, lum0 * 0.55), swamp * 0.75);
            let pool = vnoise(tx0 * 0.85 + vec2<f32>(11.0, 5.0)) * 0.7 + vnoise(tx0 * 2.4) * 0.3;
            if (swamp > 0.45 && pool > 0.66) {
                col = select(vec3<f32>(0.26, 0.40, 0.42), vec3<f32>(0.48, 0.56, 0.42), pool < 0.675);
                if (sin(w.x * 9.0 + clock * 1.3) * sin(w.y * 8.0 - clock) > 0.92) { col = col + vec3<f32>(0.18, 0.2, 0.16); }
            }
            col = mix(col, vec3<f32>(lum0 * 1.18, lum0 * 1.28, lum0 * 1.25), cold * green * 0.7);
            col = mix(col, vec3<f32>(0.90, 0.94, 1.0), cold * green * 0.25 * vnoise(w * 0.5));
            // mountain: pale meadow, bare scree on steep ground, snow on the high flat parts
            let alpine = smoothstep(3.5, 7.5, alt);
            col = mix(col, vec3<f32>(lum0 * 1.15, lum0 * 1.18, lum0 * 0.95), alpine * green * 0.6);
            col = mix(col, vec3<f32>(lum0 * 0.95), steep * smoothstep(5.0, 9.0, alt) * 0.5);
            let cap = smoothstep(11.5, 14.5, alt - steep * 3.0 + 1.2 * vnoise(w * 0.4));
            col = mix(col, vec3<f32>(0.90, 0.94, 1.0), cap * 0.92);
            // wind runs over the grass in light and dark waves
            // ... running downwind
            let wave = (vnoise(w * 0.12 + globals.cloud.yz * 5.45) - 0.5) * 3.0 + (vnoise(w * 0.37 + globals.cloud.yz * 16.8) - 0.5) * 1.5;
            col = col * (1.0 + 0.035 * (0.3 + wnd) * wave);
            // wet earth is darker
            col = col * (1.0 - 0.2 * rain);
            // snow settles in patches
            if (snow > 0.02) {
                let tx = floor(w * 21.0);
                let nt = fract(sin(dot(tx, vec2<f32>(12.9898, 78.233))) * 43758.5453);
                let lo = floor(w * 1.7);
                let nl = fract(sin(dot(lo, vec2<f32>(39.3468, 11.135))) * 24634.6345);
                if (nt < snow * (0.55 + 0.9 * nl) - 0.05) { col = mix(col, vec3<f32>(0.90, 0.94, 1.0), 0.88); }
            }
        }
    } else if (v.kind == 5.0) {
        let t = textureLoad(atlas, vec2<i32>(floor(v.uv)), 0);
        alpha = t.a;
        if (alpha < 0.02) { discard; }
        let night = clamp((1.0 - globals.light.r) / 0.7, 0.0, 1.0);
        if (v.glow > 0.5) { alpha = alpha * night; }
        col = t.rgb * v.color;
        n = ground_n(v.wpos);
    } else {
        col = v.color * tex;
        n = nm;
        // snow on the upper faces of leaves, a wet sheen darkening of everything outdoors
        if (v.mat > 1.5 && v.mat < 2.5 && v.glow < 0.5) {
            col = mix(col, vec3<f32>(0.9, 0.94, 1.0), globals.env.w * 0.45 * smoothstep(0.45, 0.8, nm.y));
            col = col * (1.0 - 0.12 * globals.env.z);
        }
    }
    // light: sky, sun (held back by shadows and clouds) and fires; things that glow light themselves
    var lit: vec3<f32>;
    if (v.glow < 0.5) {
        var sh = sun_shadow(v.world);
        // the player keeps most of its colour in the shade, so it can always be picked out
        let pd = v.world - globals.peek.xyz;
        if (v.kind == 4.0 && globals.peek.w > 0.5 && abs(pd.x) < 0.45 && abs(pd.z) < 0.45 && abs(pd.y) < 1.1) { sh = sh * 0.4; }
        let cs = cloud_shade(v.world.xz);
        var l = env_light(n, sh, cs);
        if (v.kind == 4.0) {
            // models: a touch brighter than the ground so they stand out, in clear bands
            l = quantize(l * 1.12, 0.13);
        } else {
            l = quantize(l, 0.06);
        }
        let fire = quantize(fire_light(n, v.world), 0.12);
        lit = col * (l * globals.light.rgb + fire);
    } else {
        lit = col * mix(globals.light.rgb, vec3<f32>(1.0), v.glow);
    }
    // low perception: colours run together
    let lum = dot(lit, vec3<f32>(0.30, 0.59, 0.11));
    lit = mix(vec3<f32>(lum), lit, 0.28 + 0.72 * p);
    // ... and the world fades into haze not far from where you stand
    let d = distance(v.wpos, globals.cam.xy);
    let r0 = mix(9.0, 95.0, p * p);
    let f = smoothstep(r0, r0 + mix(7.0, 35.0, p), d) * (1.0 - v.glow) * (1.0 - 0.5 * p);
    let haze = vec3<f32>(0.50, 0.54, 0.56) * globals.light.rgb;
    return vec4<f32>(pow(max(mix(lit, haze, f), vec3<f32>(0.0)), vec3<f32>(2.2)), alpha);
}

// Never lose the player behind a tree or a wall: its body is drawn once more into the picture's alpha alone,
// first all of it (0.5), then only where something stands in front of it (0). The outline pass rings that hidden
// part with a bright line. The second draw is pulled toward the camera a little, so the body's own front does not
// count as something hiding its back.
@vertex fn vs_hidden(v: VIn) -> VOut {
    var o = vs_body(v);
    let to_cam = -cross(globals.cam_u.xyz, globals.cam_r.xyz);
    o.clip = globals.view_proj * vec4<f32>(o.world + normalize(to_cam) * 0.6, 1.0);
    return o;
}
@fragment fn fs_mark() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 0.5);
}
@fragment fn fs_hidden() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0);
}
