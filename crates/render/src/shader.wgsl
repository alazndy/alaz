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
    occ: vec4<f32>,     // x: how many of `fires` burn
    sun_vp_static: mat4x4<f32>, // the static shadow map's own box (trees and rocks; the shadow map starts as a copy of a window of it)
    cave: vec4<f32>,    // x: torch strength (0..1), y: local roof cut enabled
    look: vec4<f32>,    // xyz: a small resource the pointer is on, w: 1 if there is one
};
@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var atlas: texture_2d<f32>;
// ground height in metres at tile corners, a 256x256 window around the camera
@group(0) @binding(2) var hmap: texture_2d<f32>;
// depth seen from the sun: a copy of the static map's window, with what moves drawn over it (only bound in the main pass)
@group(0) @binding(3) var shadow_map: texture_depth_2d;
// the static shadow map (only bound in the copy that starts the dynamic shadow map)
@group(0) @binding(7) var shadow_static: texture_depth_2d;
// per tile of the height window: (atlas x, atlas y, ground kind, shade) (main pass only)
@group(0) @binding(4) var tilemap: texture_2d<f32>;
// the ground's time-independent colour, baked per ground texel into a ring of chunk-sized squares (main pass only):
// rgb: colour / 2, a: state / 3 (see GStatic); and the height gradient (gx, gz) there
@group(0) @binding(5) var bake_col: texture_2d<f32>;
@group(0) @binding(6) var bake_nrm: texture_2d<f32>;
// the bake pass only: per drawn block, (world texel - ring texel) of its first texel
@group(0) @binding(8) var<uniform> bake_params: array<vec4<f32>, 32>;
// GT (ground texels to the metre), GTF, RING_TEXELS (the bake ring's side in texels: 4 chunks of 32 tiles) and SHADOWS
// (false: no shadow map is drawn) are constants put in front of this file by quality.rs

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
    let q = p - globals.hwin.xy - vec2<f32>(0.5);
    let f = floor(q);
    let t = q - f;
    let i = vec2<i32>(f);
    let a = mix(corner(i).a, corner(i + vec2<i32>(1, 0)).a, t.x);
    let b = mix(corner(i + vec2<i32>(0, 1)).a, corner(i + vec2<i32>(1, 1)).a, t.x);
    return mix(a, b, t.y);
}

// bilinear ground height at a point on the map (the same surface the terrain tiles are cut from, up to the triangle split)
fn cubic4(a: f32, b: f32, c: f32, d: f32, t: f32) -> f32 {
    return b + 0.5 * t * (c - a + t * (2.0 * a - 5.0 * b + 4.0 * c - d + t * (3.0 * (b - c) + d - a)));
}
// The ground height: a smooth (Catmull-Rom) surface through the tile corners, kept inside the range of its own cell's
// corners so a hill never overshoots. The CPU's `smooth_height` (mesh/common.rs) is the same formula.
fn ground_h(p: vec2<f32>) -> f32 {
    let q = p - globals.hwin.xy;
    let f = floor(q);
    let t = q - f;
    let i = vec2<i32>(f);
    var rows: array<f32, 4>;
    for (var j = 0; j < 4; j++) {
        let y = i.y + j - 1;
        rows[j] = cubic4(corner_h(vec2<i32>(i.x - 1, y)), corner_h(vec2<i32>(i.x, y)), corner_h(vec2<i32>(i.x + 1, y)), corner_h(vec2<i32>(i.x + 2, y)), t.x);
    }
    let h = cubic4(rows[0], rows[1], rows[2], rows[3], t.y);
    let c00 = corner_h(i);
    let c10 = corner_h(i + vec2<i32>(1, 0));
    let c01 = corner_h(i + vec2<i32>(0, 1));
    let c11 = corner_h(i + vec2<i32>(1, 1));
    return clamp(h, min(min(c00, c10), min(c01, c11)), max(max(c00, c10), max(c01, c11)));
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
    @location(7) world: vec3<f32>,
};
// what the ground needs from its vertices: where it is (the colour comes from the tile window and the bake)
struct GIn {
    @builtin(position) clip: vec4<f32>,
    @location(0) wpos: vec2<f32>,
    @location(1) world: vec3<f32>,
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
// A model in a chunk's cached mesh has its ground height worked out on the CPU when the mesh is built, carried in
// `kind` as 1000 + height; it is a model (4) in every other way.
fn unpack(v0: VIn) -> VIn {
    var v = v0;
    if (v0.kind > 500.0) { v.kind = 4.0; }
    return v;
}

fn place(v0: VIn) -> vec3<f32> {
    let v = unpack(v0);
    // everything stands on the ground: models ride up on their anchor point, ground and decals follow the surface
    let anchored = v.kind == 4.0 && (v.off.x != 0.0 || v.off.y != 0.0);
    var lift = 0.0;
    if (v0.kind > 500.0) {
        lift = v0.kind - 1000.0;
    } else {
        lift = ground_h(select(v.pos.xz, v.off, anchored)) + select(0.0, 0.02, anchored);
    }
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
@vertex fn vs_shadow(v0: VIn) -> SOut {
    let v = unpack(v0);
    var o: SOut;
    let casts = v.kind == 4.0 && v.glow < 0.5;
    o.clip = select(vec4<f32>(0.0, 0.0, -1.0, 1.0), globals.sun_vp * vec4<f32>(place(v0), 1.0), casts);
    o.p3 = vec3<f32>(v.pos.x - v.off.x, v.pos.y, v.pos.z - v.off.y);
    o.mat = v.uv.y;
    return o;
}

// the same for the static map: its own box; the plants are caught as they sway at the moment of the draw and then hold still
@vertex fn vs_shadow_static(v0: VIn) -> SOut {
    let v = unpack(v0);
    var o: SOut;
    let casts = v.kind == 4.0 && v.glow < 0.5;
    o.clip = select(vec4<f32>(0.0, 0.0, -1.0, 1.0), globals.sun_vp_static * vec4<f32>(place(v0), 1.0), casts);
    o.p3 = vec3<f32>(v.pos.x - v.off.x, v.pos.y, v.pos.z - v.off.y);
    o.mat = v.uv.y;
    return o;
}

// the dynamic shadow map starts as a window of the static one: its texels are the same size, the window starts at
// globals.occ.yz texels in
struct DepthOut {
    @builtin(frag_depth) d: f32,
};
@vertex fn vs_copy(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs_copy(@builtin(position) p: vec4<f32>) -> DepthOut {
    let q = vec2<i32>(p.xy) + vec2<i32>(globals.occ.yz);
    return DepthOut(textureLoad(shadow_static, q, 0));
}

@fragment fn fs_shadow(o: SOut) {
    // light falls through the same gaps in the crowns that the eye sees through
    if (o.mat > 1.5 && o.mat < 2.5 && o.p3.y > 1.15 && leaf_gap(o.p3)) { discard; }
}

// 1 where the sun is blocked, 0 where it reaches; outside the shadow box everything is lit
fn sun_shadow(wp: vec3<f32>) -> f32 {
    if (!SHADOWS || globals.sun.w < 0.01) { return 0.0; }
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
fn vs_body(v0: VIn) -> VOut {
    let v = unpack(v0);
    var o: VOut;
    let wp = place(v0);
    o.clip = globals.view_proj * vec4<f32>(wp, 1.0);
    o.world = wp;
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

// Fine relief of the ground, a few centimetres high: two octaves of value noise. It only bends the light (the surface
// the feet and the props stand on stays smooth), so nothing has to agree with it.
fn ground_relief(p: vec2<f32>) -> f32 {
    return (vnoise(p * 1.7) - 0.5) * 0.10 + (vnoise(p * 4.3 + vec2<f32>(7.7, 3.1)) - 0.5) * 0.05;
}
fn ground_bump(w: vec2<f32>) -> vec2<f32> {
    let e = 0.12;
    return vec2<f32>(
        ground_relief(w + vec2<f32>(e, 0.0)) - ground_relief(w - vec2<f32>(e, 0.0)),
        ground_relief(w + vec2<f32>(0.0, e)) - ground_relief(w - vec2<f32>(0.0, e)),
    ) / (2.0 * e);
}

// surface direction of the ground at a point, from the height map
fn ground_n(w: vec2<f32>) -> vec3<f32> {
    let e = 1.0;
    let gx = (ground_h(w + vec2<f32>(e, 0.0)) - ground_h(w - vec2<f32>(e, 0.0))) / (2.0 * e);
    let gz = (ground_h(w + vec2<f32>(0.0, e)) - ground_h(w - vec2<f32>(0.0, e))) / (2.0 * e);
    let b = ground_bump(w);
    return normalize(vec3<f32>(-(gx + b.x), 1.0, -(gz + b.y)));
}

// light from the sky and the sun on a surface facing `n` at `wp`: a cool sky above and warm bounce from the ground
// below, plus the sun (warm and low in the morning and evening, white at noon, blue moonlight at night), which the
// shadows and clouds hold back. About 1.0 on flat ground in full sun. `shade_k` scales the shadow (the player keeps
// most of its colour in the shade). The shadow map and the clouds are only looked up where the sun could add light:
// a face turned from the sun, or one already in full shadow, gets the same result without them.
fn env_light(n: vec3<f32>, wp: vec3<f32>, shade_k: f32) -> vec3<f32> {
    let s = clamp(globals.sun.w / 0.8, 0.0, 1.0);
    let sd = normalize(globals.sun.xyz);
    let night = clamp((1.0 - globals.light.r) / 0.7, 0.0, 1.0);
    let low = mix(vec3<f32>(1.0, 0.66, 0.42), vec3<f32>(1.0, 0.95, 0.86), smoothstep(0.25, 0.7, sd.y));
    let sun_col = mix(low, vec3<f32>(0.62, 0.72, 1.0), night);
    let sky = mix(vec3<f32>(0.54, 0.47, 0.40), vec3<f32>(0.66, 0.74, 0.90), n.y * 0.5 + 0.5);
    let amb = sky * (0.62 + 0.4 * (1.0 - s));
    var direct = vec3<f32>(0.0);
    let ndl = dot(n, sd);
    if (ndl > 0.0) {
        let sh = sun_shadow(wp) * shade_k;
        var cs = 0.0;
        if (sh < 1.0) { cs = cloud_shade(wp.xz); }
        // a low sun is weaker on flat ground, but not as much as it really would be: mornings stay bright and golden
        let lift = 0.91 / max(sd.y, 0.6);
        direct = sun_col * 0.66 * s * lift * ndl * (1.0 - sh) * (1.0 - 0.5 * cs);
    }
    return amb + direct;
}

// warm light of the fires nearby, flickering; it is not dimmed by the night
fn fire_light(n: vec3<f32>, wp: vec3<f32>) -> vec3<f32> {
    var f = vec3<f32>(0.0);
    // by day the sun drowns it out
    let night = clamp((1.0 - globals.light.r) / 0.7, 0.0, 1.0);
    let k = 0.25 + 0.75 * night;
    let count = i32(globals.occ.x);
    for (var i = 0; i < count; i = i + 1) {
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

// the fires' light in whole steps (quantize is defined below); none at all, the usual case, costs one test
fn fires_at(n: vec3<f32>, wp: vec3<f32>) -> vec3<f32> {
    if (globals.occ.x < 0.5) { return vec3<f32>(0.0); }
    return quantize(fire_light(n, wp), 0.12);
}

// cut a light level into whole steps so faces read as flat colour bands, as painted pixel art would
fn quantize(l: vec3<f32>, step: f32) -> vec3<f32> {
    let lum = max(dot(l, vec3<f32>(0.30, 0.59, 0.11)), 0.001);
    let q = max(round(lum / step) * step, step * 0.5);
    return l * (q / lum);
}

// perception, distance haze and the final gamma: the same for everything drawn in the scene
fn finish(lit0: vec3<f32>, wpos: vec2<f32>, glow: f32) -> vec3<f32> {
    let p = globals.light.w;
    // low perception: colours run together
    let lum = dot(lit0, vec3<f32>(0.30, 0.59, 0.11));
    let lit = mix(vec3<f32>(lum), lit0, 0.28 + 0.72 * p);
    // ... and the world fades into haze not far from where you stand
    let d = distance(wpos, globals.cam.xy);
    // (rain closes the distance in: the haze starts nearer and is greyer)
    let r0 = mix(9.0, 95.0, p * p) * (1.0 - 0.45 * globals.env.z);
    let f = smoothstep(r0, r0 + mix(7.0, 35.0, p), d) * (1.0 - glow) * (1.0 - 0.5 * p * (1.0 - 0.6 * globals.env.z));
    let haze = mix(vec3<f32>(0.50, 0.54, 0.56), vec3<f32>(0.50, 0.55, 0.60), globals.env.z) * globals.light.rgb;
    return pow(max(mix(lit, haze, f), vec3<f32>(0.0)), vec3<f32>(2.2));
}

// ---- the ground ------------------------------------------------------------------------------------------------
// Ground is drawn on a texel grid of GT per metre (27 or 21). Everything about a land texel that does not change with time or
// weather (its painted picture, ragged tile borders, wet shore sand, biomes, deserts, swamps, mountain and snow cap,
// and which way it faces) is worked out by `ground_static`, either live (fs_ground_live) or ahead of time into the bake
// (fs_bake -> fs_ground). Wind, rain, puddles, snow fall, sparkle, the sea, light and shadow stay per frame.

struct GStatic {
    col: vec3<f32>,
    // 0 plain land, 1 land where puddles can form, 2 water (drawn live), 3 swamp pool (its sparkle is live)
    state: f32,
    // height gradient (d height / dx, d height / dz) of the land there
    g: vec2<f32>,
};

// integer division of ground texels into tiles that rounds down
fn tile_of(t: vec2<i32>) -> vec2<i32> {
    let d = t / vec2<i32>(GT);
    return select(d, d - vec2<i32>(1), (t - d * GT) < vec2<i32>(0));
}

// the tile window's entry for a tile: (atlas x, atlas y, ground kind, shade)
// (the kind channel also carries the surface layer, `kind + 16 * code`: see surf_code)
fn tile_at(t: vec2<i32>) -> vec4<f32> {
    var v = textureLoad(tilemap, clamp(t - vec2<i32>(globals.hwin.xy), vec2<i32>(0), vec2<i32>(255)), 0);
    v.z = v.z - 16.0 * floor(v.z / 16.0);
    // a river tile's shade channel carries its current (see flow_at_tile); the ground only sees plain shade
    if (v.w > 1.5) { v.w = 1.0; }
    return v;
}

// the current of a water tile in metres a second: its shade channel holds 2 + (x + 16) * 40 + (y + 16), with x and y
// in mm a tick; 1 is still water and anything below 1.5 is land shade
fn flow_at_tile(t: vec2<i32>) -> vec2<f32> {
    let w = textureLoad(tilemap, clamp(t - vec2<i32>(globals.hwin.xy), vec2<i32>(0), vec2<i32>(255)), 0).w;
    if (w < 1.5) { return vec2<f32>(0.0); }
    let n = w - 2.0;
    let a = floor(n / 40.0);
    return vec2<f32>(a - 16.0, n - a * 40.0 - 16.0) * 0.06;
}

// the current at a point, blended from tile centre to tile centre so it does not change in steps
fn flow_vec(p: vec2<f32>) -> vec2<f32> {
    let qs = p - vec2<f32>(0.5);
    let t0 = vec2<i32>(floor(qs));
    let fr = qs - floor(qs);
    return mix(mix(flow_at_tile(t0), flow_at_tile(t0 + vec2<i32>(1, 0)), fr.x),
               mix(flow_at_tile(t0 + vec2<i32>(0, 1)), flow_at_tile(t0 + vec2<i32>(1, 1)), fr.x), fr.y);
}

// the surface layer the sim keeps for a tile: the surface in the low three bits (1 mud, 2 puddle, 3 ice, 4 ash,
// 5 fire) and, for ash and fire, how far along it is (0..3) above them
fn surf_code(t: vec2<i32>) -> i32 {
    let z = textureLoad(tilemap, clamp(t - vec2<i32>(globals.hwin.xy), vec2<i32>(0), vec2<i32>(255)), 0).z;
    return i32(floor(z / 16.0));
}

// the painted picture of a land texel, with the ragged borders between kinds of ground (and the wet sand that is the
// land edge of a water tile)
fn land_base(tx: vec2<i32>, tile: vec2<i32>, tl: vec2<i32>, mine: vec4<f32>) -> vec3<f32> {
    // each tile has its own shade; read flat, the tiles show as a grid of lighter and darker squares, so it is blended
    // across from tile centre to tile centre instead
    let qs = vec2<f32>(tx) / GTF - vec2<f32>(0.5);
    let t0 = vec2<i32>(floor(qs));
    let fr = qs - floor(qs);
    let shade = mix(mix(tile_at(t0).w, tile_at(t0 + vec2<i32>(1, 0)).w, fr.x),
                    mix(tile_at(t0 + vec2<i32>(0, 1)).w, tile_at(t0 + vec2<i32>(1, 1)).w, fr.x), fr.y);
    var col = textureLoad(atlas, vec2<i32>(floor(mine.xy)) + tl, 0).rgb * shade;
    if (mine.z > 0.5) {
        // ground boundaries wander: each texel looks a little way off along a slow noise and, if that falls on a
        // tile of another kind of land, wears that tile's ground; edges become ragged lines, not tile staircases
        let q = vec2<f32>(tx) / GTF;
        let jit = vec2<f32>(vnoise(q * 1.7 + vec2<f32>(17.3, 4.1)), vnoise(q * 1.7 + vec2<f32>(3.7, 91.2))) - vec2<f32>(0.5);
        let fine = vec2<f32>(vnoise(q * 6.0 + vec2<f32>(51.0, 7.0)), vnoise(q * 6.0 + vec2<f32>(9.0, 33.0))) - vec2<f32>(0.5);
        let wl = q + jit * 0.9 + fine * 0.25;
        let own = vec2<f32>(tile);
        let other = floor(wl);
        if (other.x != own.x || other.y != own.y) {
            let them = tile_at(vec2<i32>(other));
            // land over land only (the water's edge has its own smooth line)
            if (them.z > 0.5 && them.z != mine.z) {
                col = textureLoad(atlas, vec2<i32>(floor(them.xy)) + tl, 0).rgb * shade;
            }
        }
    } else {
        // the land edge of a water tile: its own picture is water, so it is drawn as wet sand
        col = vec3<f32>(0.80, 0.70, 0.48) * (0.93 + 0.07 * hash21(vec2<f32>(tx)));
    }
    return col;
}

fn ground_static(tx: vec2<i32>) -> GStatic {
    var r: GStatic;
    r.col = vec3<f32>(0.0);
    r.state = 2.0;
    r.g = vec2<f32>(0.0);
    let tile = tile_of(tx);
    let tl = tx - tile * GT;
    let mine = tile_at(tile);
    // Sample at the texel centre so water, wet sand and foam agree on the same shoreline.
    let w = (vec2<f32>(tx) + 0.5) / GTF;
    let q = vec2<f32>(tx) / GTF;
    let beach = shore_at(w);
    if (select((mine.z < 0.5), (beach > 0.0), (abs(beach) < 1.2))) { return r; }
    var col = land_base(tx, tile, tl, mine);
    col = col * mix(1.0, 0.72, smoothstep(-0.55, -0.25, beach));
    // the shape of the land: which way it faces, over two tiles so the light does not break at every tile edge
    let e = 1.0;
    let gx = (ground_h(w + vec2<f32>(e, 0.0)) - ground_h(w - vec2<f32>(e, 0.0))) / (2.0 * e);
    let gz = (ground_h(w + vec2<f32>(0.0, e)) - ground_h(w - vec2<f32>(0.0, e))) / (2.0 * e);
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
    let desert = smoothstep(0.62, 0.70, cl.x) * smoothstep(0.38, 0.30, cl.y);
    if (desert > 0.0) {
        let dune = vnoise(q * vec2<f32>(0.9, 2.6) + vec2<f32>(3.1, 0.7));
        col = mix(col, vec3<f32>(0.80, 0.66, 0.42) * (0.92 + 0.16 * dune), desert * (0.35 + 0.6 * green));
        if (desert > 0.5 && vnoise(q * 3.1) > 0.78 && vnoise(q * 7.3 + vec2<f32>(1.3, 4.1)) > 0.55) { col = col * 0.82; }
    }
    // swamp: wet, mild and low; dark olive moss with pools of standing water
    let swamp = smoothstep(0.66, 0.74, cl.y) * smoothstep(0.36, 0.44, cl.x) * smoothstep(0.64, 0.56, cl.x) * smoothstep(3.0, 1.5, alt) * green;
    var pooled = false;
    if (swamp > 0.0) {
        col = mix(col, vec3<f32>(lum0 * 0.85, lum0 * 1.05, lum0 * 0.55), swamp * 0.75);
        let pool = vnoise(q * 0.85 + vec2<f32>(11.0, 5.0)) * 0.7 + vnoise(q * 2.4) * 0.3;
        if (swamp > 0.45 && pool > 0.66) {
            col = select(vec3<f32>(0.26, 0.40, 0.42), vec3<f32>(0.48, 0.56, 0.42), pool < 0.675);
            pooled = true;
        }
    }
    col = mix(col, vec3<f32>(lum0 * 1.18, lum0 * 1.28, lum0 * 1.25), cold * green * 0.7);
    col = mix(col, vec3<f32>(0.90, 0.94, 1.0), cold * green * 0.25 * vnoise(w * 0.5));
    // mountain: pale meadow, bare scree on steep ground, snow on the high flat parts
    let alpine = smoothstep(3.5, 7.5, alt);
    col = mix(col, vec3<f32>(lum0 * 1.15, lum0 * 1.18, lum0 * 0.95), alpine * green * 0.6);
    col = mix(col, vec3<f32>(lum0 * 0.95), steep * smoothstep(5.0, 9.0, alt) * 0.5);
    let cap = smoothstep(11.5, 14.5, alt - steep * 3.0 + 1.2 * vnoise(w * 0.4));
    col = mix(col, vec3<f32>(0.90, 0.94, 1.0), cap * 0.92);
    r.col = col;
    r.g = vec2<f32>(gx, gz) + ground_bump(w);
    // puddles form on flat ground away from the shore, below the snow line; never on the sand of a water tile
    let puddles = steep < 0.25 && beach < -0.7 && cap < 0.5 && mine.z > 0.5;
    r.state = select(select(0.0, 1.0, puddles), 3.0, pooled);
    return r;
}

// can a puddle form here? (the same test ground_static makes, for the few texels the bake marks as pools)
fn puddle_allowed(tx: vec2<i32>, g: vec2<f32>) -> bool {
    let q = vec2<f32>(tx) / GTF;
    let w = (vec2<f32>(tx) + 0.5) / GTF;
    let steep = clamp(length(g) * 2.5, 0.0, 1.0);
    let cap = smoothstep(11.5, 14.5, ground_h(w) - steep * 3.0 + 1.2 * vnoise(w * 0.4));
    return steep < 0.25 && shore_at(q) < -0.7 && cap < 0.5;
}

struct GDyn {
    col: vec3<f32>,
    n: vec3<f32>,
    // light the ground gives off by itself (flames, embers), added after the lighting
    glow: vec3<f32>,
};

// a hash of a texel's integer position, 0..1 (the sine hash shows lines and grids on a texel lattice)
fn ihash(p: vec2<i32>) -> f32 {
    var h = bitcast<u32>(p.x) * 374761393u + bitcast<u32>(p.y) * 668265263u;
    h = (h ^ (h >> 13u)) * 1274126177u;
    h = h ^ (h >> 16u);
    return f32(h & 0xFFFFu) / 65535.0;
}

// how much of a texel the surface `k` covers: its tile and the neighbours' blended across the tile centres, like the
// shade, so patches have rounded edges instead of tile corners
fn surf_cover(k: i32, c00: i32, c10: i32, c01: i32, c11: i32, fr: vec2<f32>) -> f32 {
    let a = select(0.0, 1.0, (c00 & 7) == k);
    let b = select(0.0, 1.0, (c10 & 7) == k);
    let c = select(0.0, 1.0, (c01 & 7) == k);
    let d = select(0.0, 1.0, (c11 & 7) == k);
    return mix(mix(a, b, fr.x), mix(c, d, fr.x), fr.y);
}

// mud, standing water, ice, ash and flames over a land texel (the sim says where; `globals.occ.w` says if anywhere)
fn surface_layer(col0: vec3<f32>, tx: vec2<i32>, rain: f32) -> GDyn {
    var o: GDyn;
    o.col = col0;
    o.glow = vec3<f32>(0.0);
    o.n = vec3<f32>(0.0, 1.0, 0.0);
    let clock = globals.env.x;
    let tq = vec2<f32>(tx) / GTF;
    let qs = tq - vec2<f32>(0.5);
    let t0 = vec2<i32>(floor(qs));
    let fr = qs - floor(qs);
    let c00 = surf_code(t0);
    let c10 = surf_code(t0 + vec2<i32>(1, 0));
    let c01 = surf_code(t0 + vec2<i32>(0, 1));
    let c11 = surf_code(t0 + vec2<i32>(1, 1));
    if ((c00 | c10 | c01 | c11) == 0) { return o; }
    // the tile the texel is in: how old its ash or flames are
    let age = f32(surf_code(tile_of(tx)) >> 3);
    let n1 = vnoise(tq * 2.1 + vec2<f32>(7.3, 1.9));
    let n2 = vnoise(tq * 5.0 + vec2<f32>(2.2, 9.1));
    let hx = ihash(tx);
    var col = col0;
    // ragged patch edges: the covered share, shaken by noise, cut at a threshold
    let jag = (n1 - 0.5) * 0.7 + (n2 - 0.5) * 0.25;
    // mud: dark wet earth with a dull sheen where the sky catches it
    let mud = smoothstep(0.40, 0.60, surf_cover(1, c00, c10, c01, c11, fr) + jag);
    if (mud > 0.0) {
        var m = mix(vec3<f32>(0.25, 0.18, 0.115), vec3<f32>(0.17, 0.12, 0.08), n2);
        if (hx > 0.92) { m = m * 0.78; }
        let sheen = vnoise(tq * 8.0 + vec2<f32>(11.0, 4.0));
        if (sheen > 0.74) { m = mix(m, vec3<f32>(0.50, 0.52, 0.55), 0.35 + 0.25 * rain); }
        col = mix(col, m, 0.88 * mud);
    }
    // standing water: the grey sky mirrored, deeper toward the middle, rings where drops land
    let pud_c = surf_cover(2, c00, c10, c01, c11, fr);
    let pud = smoothstep(0.40, 0.58, pud_c + jag * 0.8);
    if (pud_c > 0.0) {
        let lip = smoothstep(0.40, 0.58, pud_c + jag * 0.8 - 0.16);
        if (pud > 0.0 && lip < 0.5) { col = mix(col, vec3<f32>(0.16, 0.115, 0.08), 0.85 * pud); }
        if (lip > 0.0) {
            var pc = mix(vec3<f32>(0.43, 0.45, 0.47), vec3<f32>(0.30, 0.32, 0.34), smoothstep(0.0, 0.5, pud_c));
            pc = mix(pc, vec3<f32>(0.60, 0.66, 0.72), 0.25 * vnoise(tq * 3.0 + vec2<f32>(clock * 0.15, 0.0)));
            if (rain > 0.05) {
                let cell = floor(tq * 2.2);
                let hh = hash21(cell + vec2<f32>(5.3, 1.7));
                let phase = fract(clock * 1.4 + hh * 7.0);
                let c0 = (cell + vec2<f32>(0.2 + 0.6 * hh, 0.2 + 0.6 * fract(hh * 91.7))) / 2.2;
                if (abs(length(tq - c0) - phase * 0.3) < 0.04) { pc = pc + vec3<f32>(0.26, 0.27, 0.28) * (1.0 - phase); }
            }
            col = mix(col, pc, lip);
        }
    }
    // ice: pale blue and white, hairline cracks, and the odd glint
    let ice = smoothstep(0.38, 0.56, surf_cover(3, c00, c10, c01, c11, fr) + jag * 0.7);
    if (ice > 0.0) {
        var ic = mix(vec3<f32>(0.74, 0.86, 0.93), vec3<f32>(0.92, 0.97, 1.0), n2);
        let crack = abs(vnoise(tq * 3.3 + vec2<f32>(21.0, 6.0)) - 0.5);
        if (crack < 0.028) { ic = vec3<f32>(0.97, 0.99, 1.0); }
        if (hx > 0.975 && fract(clock * 1.1 + hx * 31.0) < 0.45) { o.glow = o.glow + vec3<f32>(0.55, 0.62, 0.7); }
        col = mix(col, ic, 0.92 * ice);
    }
    // ash: dark grey scorch that greys and breaks up as the grass comes back; embers wink in the fresh ash
    let ash_c = surf_cover(4, c00, c10, c01, c11, fr);
    let ash = smoothstep(0.38 + 0.07 * age, 0.58 + 0.07 * age, ash_c + jag);
    if (ash > 0.0) {
        let k = age / 3.0;
        var a = mix(vec3<f32>(0.15, 0.14, 0.13), vec3<f32>(0.30, 0.29, 0.27), n2);
        if (hx > 0.8) { a = a * 1.25; }
        col = mix(col, a, (0.92 - 0.5 * k) * ash);
        if (age < 1.5 && hx > 0.975 - 0.01 * age && n2 > 0.45) {
            let pulse = 0.5 + 0.5 * sin(clock * 5.0 + hx * 90.0);
            o.glow = o.glow + vec3<f32>(0.9, 0.34, 0.07) * pulse * (0.7 - 0.3 * age) * ash;
        }
    }
    // flames: scorched ground under tongues of red, orange and yellow rising and flickering; they die back with age
    let fire_c = surf_cover(5, c00, c10, c01, c11, fr);
    let fire = smoothstep(0.34, 0.56, fire_c + jag);
    if (fire > 0.0) {
        col = mix(col, vec3<f32>(0.10, 0.07, 0.05), 0.85 * fire);
        let life = 1.0 - 0.18 * age;
        let f1 = vnoise(vec2<f32>(tq.x * 6.0, tq.y * 6.0 + clock * 2.6));
        let f2 = vnoise(vec2<f32>(tq.x * 11.0 + 5.0, tq.y * 11.0 + clock * 4.1));
        let flame = (f1 * 0.65 + f2 * 0.35) * life + 0.12 * fire_c;
        if (flame > 0.42) {
            let h = smoothstep(0.42, 0.78, flame);
            var fc = mix(vec3<f32>(0.95, 0.22, 0.04), vec3<f32>(1.0, 0.62, 0.12), h);
            fc = mix(fc, vec3<f32>(1.0, 0.92, 0.55), smoothstep(0.7, 0.9, flame));
            o.glow = o.glow + fc * (0.7 + 0.5 * h) * fire;
        }
    }
    o.col = col;
    return o;
}

// what a texel of the ground looks like this moment: `g` is its time-independent part
fn ground_dyn(w: vec2<f32>, tx: vec2<i32>, g: GStatic) -> GDyn {
    let clock = globals.env.x;
    let wnd = globals.env.y;
    let rain = globals.env.z;
    let snow = globals.env.w;
    var r: GDyn;
    r.glow = vec3<f32>(0.0);
    var col = g.col;
    var n = vec3<f32>(0.0, 1.0, 0.0);
    if (g.state > 1.5 && g.state < 2.5) {
        // water, drawn on the ground's own texel grid: deep blue r in the open, turquoise in the shallows,
        // wave bands drifting with the wind, foam lapping at the shore and the sun sparkling on the ripples
        let q = (vec2<f32>(tx) + 0.5) / GTF;
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
        let breaker = 0.06 + 0.05 * wnd;
        let surf = (1.0 - smoothstep(breaker * 0.7, breaker, surf_ph))
            * smoothstep(0.12, 0.24, depth) * (1.0 - smoothstep(0.24, 2.2, depth));
        col = mix(col, vec3<f32>(0.9, 0.96, 0.97), 0.75 * surf);
        // the side of each wave facing the sun lighter, the far side darker: shows its shape
        col = col * (1.0 + 0.9 * clamp(globals.sun.w / 0.8, 0.2, 1.0) * dot(vec2<f32>(sw.y, sw.z), normalize(globals.sun.xz + vec2<f32>(0.001))));
        // whitecaps only on the highest crests, and only once the wind is fresh
        if (crest > 0.8 && wnd > 0.7 && deep > 0.3 && hash21(vec2<f32>(tx) + floor(clock * 4.0)) > 0.9 - 0.2 * (wnd - 0.7)) {
            col = mix(col, vec3<f32>(0.88, 0.95, 0.97), 0.7);
        }
        // a river: streaks of lighter water stretched along the current and carried with it. Two copies a half cycle
        // apart fade in and out, so the pattern keeps moving without smearing as the current changes direction.
        let fv = flow_vec(q);
        let spd = length(fv);
        if (spd > 0.02) {
            let dir = fv / spd;
            let perp = vec2<f32>(-dir.y, dir.x);
            var streak = 0.0;
            for (var i = 0; i < 2; i++) {
                let cyc = fract(clock * 0.22 + f32(i) * 0.5);
                let uv = q - fv * (cyc * 4.5);
                let s = vec2<f32>(dot(uv, dir) * 0.6, dot(uv, perp) * 3.2) + f32(i) * vec2<f32>(7.3, 2.1);
                streak += (1.0 - abs(2.0 * cyc - 1.0)) * smoothstep(0.58, 0.82, vnoise(s));
            }
            col = mix(col, vec3<f32>(0.78, 0.90, 0.93), 0.4 * streak * smoothstep(0.02, 0.08, spd) * (1.0 - 0.5 * deep));
        }
        let band = abs(n1 - n2);
        if (band < 0.05) { col = col * 1.16 + vec3<f32>(0.03, 0.05, 0.05); }
        else if (band > 0.42) { col = col * 0.86; }
        // the sea floor shows through the shallows
        if (depth < 0.9 && vnoise(q * 3.0) > 0.62) { col = mix(col, vec3<f32>(0.46, 0.62, 0.52), 0.35 * (1.0 - depth / 0.9)); }
        // Feather the wash over one ground texel to keep wave fronts continuous across tile edges.
        let wave = sin(clock * 1.1 + vnoise(q * 0.6) * 6.0) * 0.5 + 0.5;
        let line1 = 0.06 + 0.32 * wave;
        let wash = 0.50 * (1.0 - smoothstep(line1 - 0.09, line1 + 0.06, depth));
        let rim = 0.30 * (1.0 - smoothstep(0.025, 0.10, abs(depth - line1)));
        col = mix(col, vec3<f32>(0.86, 0.94, 0.95), wash + rim);
        let line2 = 0.55 + 0.35 * fract(clock * 0.16 + vnoise(q * 0.4) * 2.0);
        let foam = 1.0 - smoothstep(0.02, 0.09, abs(depth - line2));
        col = mix(col, vec3<f32>(0.75, 0.88, 0.92), 0.38 * foam * (1.0 - deep) * mix(0.4, 1.0, vnoise(q * 4.0)));
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
        // Night light applies to the whole surface, including foam, whitecaps and rain.
        let sea_night = clamp((1.0 - globals.light.r) / 0.7, 0.0, 1.0);
        col = col * mix(1.0, 0.40, sea_night * sea_night);
    } else {
        n = normalize(vec3<f32>(-g.g.x, 1.0, -g.g.y));
        // the sparkle on a swamp pool
        if (g.state > 2.5 && sin(w.x * 9.0 + clock * 1.3) * sin(w.y * 8.0 - clock) > 0.92) { col = col + vec3<f32>(0.18, 0.2, 0.16); }
        // wind runs over the grass in light and dark waves
        // ... running downwind
        let wave = (vnoise(w * 0.12 + globals.cloud.yz * 5.45) - 0.5) * 3.0 + (vnoise(w * 0.37 + globals.cloud.yz * 16.8) - 0.5) * 1.5;
        col = col * (1.0 + 0.035 * (0.3 + wnd) * wave);
        // wet earth is darker
        col = col * (1.0 - 0.22 * rain);
        if (rain > 0.05) {
            let tq = vec2<f32>(tx) / GTF;
            // puddles: still water in the flat hollows, growing as the rain goes on; they hold the grey sky,
            // deeper toward the middle, with a muddy lip, and the drops ring them
            let pn = vnoise(tq * 0.45 + vec2<f32>(31.7, 12.9)) * 0.65 + vnoise(tq * 1.4 + vec2<f32>(4.2, 77.1)) * 0.35;
            let fill = 0.86 - 0.12 * rain;
            var puddle = false;
            if (pn > fill) { puddle = (g.state > 0.5 && g.state < 1.5) || (g.state > 2.5 && puddle_allowed(tx, g.g)); }
            if (puddle) {
                let deep_p = pn - fill;
                // (a warm grey: the sky light it is lit by is already blue) with a little of the ground showing
                var pc = mix(vec3<f32>(0.40, 0.40, 0.40), vec3<f32>(0.29, 0.29, 0.30), smoothstep(0.0, 0.06, deep_p));
                pc = mix(pc, col * 0.6, 0.3);
                if (deep_p < 0.01) { pc = col * 0.7; }
                let cell = floor(w * 2.2);
                let hh = hash21(cell + vec2<f32>(5.3, 1.7));
                let phase = fract(clock * 1.4 + hh * 7.0);
                let c0 = (cell + vec2<f32>(0.2 + 0.6 * hh, 0.2 + 0.6 * fract(hh * 91.7))) / 2.2;
                if (abs(length(tq - c0) - phase * 0.3) < 0.04 && deep_p > 0.01) { pc = pc + vec3<f32>(0.26, 0.27, 0.28) * (1.0 - phase); }
                col = pc;
            } else {
                // drops landing: a bright point, then a little crown of four texels around it, then gone
                let sc = floor(w * 1.7);
                let hs = hash21(sc + vec2<f32>(3.1, 9.7));
                let ph = fract(clock * (1.1 + 0.6 * hs) + hs * 13.0);
                // (the sand edge of a water tile gets no drops)
                if (hs < rain * 0.55 && ph < 0.13 && tile_at(tile_of(tx)).z > 0.5) {
                    let ctr = floor((sc + vec2<f32>(0.25 + 0.5 * fract(hs * 31.0), 0.25 + 0.5 * fract(hs * 57.0))) / 1.7 * GTF);
                    let a = abs(floor(w * GTF) - ctr);
                    let first = ph < 0.045 && a.x + a.y < 0.5;
                    let crown = ph >= 0.045 && a.x + a.y > 0.5 && a.x + a.y < 1.5;
                    if (first || crown) { col = mix(col, vec3<f32>(0.80, 0.86, 0.93), select(0.45, 0.65, first)); }
                }
            }
        }
        // snow settles in patches
        if (snow > 0.02) {
            let tx2 = floor(w * GTF);
            let nt = fract(sin(dot(tx2, vec2<f32>(12.9898, 78.233))) * 43758.5453);
            let lo = floor(w * 1.7);
            let nl = fract(sin(dot(lo, vec2<f32>(39.3468, 11.135))) * 24634.6345);
            if (nt < snow * (0.55 + 0.9 * nl) - 0.05) { col = mix(col, vec3<f32>(0.90, 0.94, 1.0), 0.88); }
        }
        // the sim's surfaces: mud, standing water, ice, ash and flames (only while there are any)
        if (globals.occ.w > 0.5 && g.state < 3.5) {
            let sl = surface_layer(col, tx, rain);
            col = sl.col;
            r.glow = sl.glow;
        }
    }
    r.col = col;
    r.n = n;
    return r;
}

// light, haze and gamma on a ground texel
fn ground_finish(d: GDyn, wpos: vec2<f32>, world: vec3<f32>) -> vec4<f32> {
    // light: sky, sun (held back by shadows and clouds) and fires
    let l = quantize(env_light(d.n, world, 1.0), 0.06);
    let lit = d.col * (l * globals.light.rgb + fires_at(d.n, world)) + d.glow;
    return vec4<f32>(finish(lit, wpos, 0.0), 1.0);
}

// Match the ray picker: hide only nearby rock above the player and between them and the eye.
fn cave_roof_cut(world: vec3<f32>) -> bool {
    if (globals.cave.y <= 0.5 || globals.peek.w <= 0.5) { return false; }
    let d = world - globals.peek.xyz;
    let toward_eye = cross(globals.cam_r.xyz, globals.cam_u.xyz);
    let screen = vec2<f32>(dot(d, globals.cam_r.xyz), dot(d, globals.cam_u.xyz));
    return d.y > 0.25 && dot(d, toward_eye) > 0.3 && length(d.xz) < 4.2 && length(screen) < 2.1;
}

@vertex fn vs_ground(v0: VIn) -> GIn {
    var o: GIn;
    let wp = place(v0);
    o.clip = globals.view_proj * vec4<f32>(wp, 1.0);
    o.world = wp;
    o.wpos = v0.pos.xz;
    return o;
}

// The ground of a chunk whose bake is complete: one lookup stands in for the whole static colour.
@fragment fn fs_ground(v: GIn) -> @location(0) vec4<f32> {
    if (cave_roof_cut(v.world)) { discard; }
    let tx = vec2<i32>(floor(v.wpos * GTF));
    let rt = ((tx % vec2<i32>(RING_TEXELS)) + vec2<i32>(RING_TEXELS)) % vec2<i32>(RING_TEXELS);
    let c = textureLoad(bake_col, rt, 0);
    var g: GStatic;
    g.col = c.rgb * 2.0;
    g.state = round(c.a * 3.0);
    g.g = textureLoad(bake_nrm, rt, 0).xy;
    return ground_finish(ground_dyn(v.wpos, tx, g), v.wpos, v.world);
}

// The ground of a chunk the bake has not reached yet (or lies beyond the ring): the same, worked out on the spot.
@fragment fn fs_ground_live(v: GIn) -> @location(0) vec4<f32> {
    if (cave_roof_cut(v.world)) { discard; }
    let tx = vec2<i32>(floor(v.wpos * GTF));
    return ground_finish(ground_dyn(v.wpos, tx, ground_static(tx)), v.wpos, v.world);
}

// ---- the ground bake: one block of texels at a time, drawn into the ring with the viewport on the block ------------
struct BakeV {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(flat) job: u32,
};
@vertex fn vs_bake(@builtin(vertex_index) i: u32, @builtin(instance_index) job: u32) -> BakeV {
    var o: BakeV;
    o.clip = vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
    o.job = job;
    return o;
}
struct BakeOut {
    @location(0) col: vec4<f32>,
    @location(1) g: vec2<f32>,
};
@fragment fn fs_bake(v: BakeV) -> BakeOut {
    let tx = vec2<i32>(floor(v.clip.xy)) + vec2<i32>(bake_params[v.job].xy);
    let s = ground_static(tx);
    var o: BakeOut;
    o.col = vec4<f32>(clamp(s.col * 0.5, vec3<f32>(0.0), vec3<f32>(1.0)), s.state / 3.0);
    o.g = s.g;
    return o;
}

// ---- models and decals --------------------------------------------------------------------------------------------
// Negative model UVs carry exposure from the connected cave mesh. Rock and actors use distinct ranges
// so the camera can open the roof without punching holes through bodies or carried models.
fn cave_daylight(tag: f32) -> f32 {
    if (tag <= -2.0) { return clamp(-tag - 2.0, 0.0, 1.0); }
    return clamp((-tag - 1.0) / 0.99, 0.0, 1.0);
}

fn cave_shade(v: VOut, col: vec3<f32>, n: vec3<f32>, exposure: f32) -> vec3<f32> {
    let strength = clamp(globals.cave.x, 0.0, 1.0);
    let torch = globals.peek.xyz + vec3<f32>(0.0, 0.55, 0.0);
    let reach = 8.0;
    var light = vec3<f32>(0.07, 0.078, 0.10);
    if (exposure > 0.0) {
        let day = env_light(n, v.world, 1.0) * globals.light.rgb + fires_at(n, v.world);
        light = mix(light, day, exposure);
    }
    if (strength > 0.0 && globals.peek.w > 0.5) {
        let d = torch - v.world;
        let dist = length(d);
        if (dist < reach) {
            let dir = d / max(dist, 0.001);
            let att = 1.0 - smoothstep(0.0, reach, dist);
            let ndl = max(dot(n, dir), 0.0) * 0.75 + 0.25;
            let flick = 0.90 + 0.06 * sin(globals.env.x * 13.0) + 0.04 * sin(globals.env.x * 29.0 + 1.3);
            light = light + vec3<f32>(1.0, 0.60, 0.28) * att * ndl * flick * (1.8 * strength);
        }
    }
    var lit: vec3<f32>;
    if (v.glow < 0.5) {
        lit = col * quantize(light, 0.05);
    } else {
        lit = col * (0.5 + 0.6 * v.glow);
    }
    // Seen from outside, a gallery fades into the haze like everything else. Underground the roof cut is on and the
    // haze is left out, so a cave's own far walls are not painted the grey of the open sky.
    return finish(lit, v.wpos, clamp(globals.cave.y, 0.0, 1.0));
}

@fragment fn fs(v: VOut) -> @location(0) vec4<f32> {
    let cave_model = v.kind == 4.0 && v.uv.x <= -1.0;
    if (cave_model && v.uv.x <= -2.0 && cave_roof_cut(v.world)) { discard; }
    let exposure = select(1.0, cave_daylight(v.uv.x), cave_model);
    if (v.kind == 4.0 && v.mat > 1.5 && v.mat < 2.5 && (v.glow < 0.5 || v.glow > 1.5) && v.p3.y > 1.15 && leaf_gap(v.p3)) { discard; }
    // a crown standing between the eye and the player opens up around them: leaves near the player on screen, and
    // nearer the camera than the player, are thinned out in a pixel pattern (more toward the middle), so the
    // player is never lost under a big tree
    if (v.kind == 4.0 && v.mat > 1.5 && v.mat < 2.5 && v.glow < 0.5 && globals.peek.w > 0.5) {
        let d = v.world - globals.peek.xyz;
        let to_eye = -cross(globals.cam_u.xyz, globals.cam_r.xyz);
        let ahead = dot(d, to_eye);
        let sx = dot(d, globals.cam_r.xyz);
        let sy = dot(d, globals.cam_u.xyz);
        let r = length(vec2<f32>(sx, sy * 0.8));
        if (ahead > 0.6 && r < 2.6) {
            let k = 1.0 - smoothstep(1.2, 2.6, r);
            // a 4x4 ordered pattern in 2-pixel squares: even, fine holes rather than blots
            let c = vec2<u32>(floor(v.clip.xy / (2.0 * SSF))) % vec2<u32>(4u);
            let bayer = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
            let dither = (bayer[c.y * 4u + c.x] + 0.5) / 16.0;
            if (dither < 0.85 * k) { discard; }
        }
    }
    // the same opening over the plant the pointer is on, so a click through a crown is a click on what shows
    if (v.kind == 4.0 && v.mat > 1.5 && v.mat < 2.5 && v.glow < 0.5 && globals.look.w > 0.5) {
        let d = v.world - globals.look.xyz;
        let to_eye = -cross(globals.cam_u.xyz, globals.cam_r.xyz);
        let ahead = dot(d, to_eye);
        let r = length(vec2<f32>(dot(d, globals.cam_r.xyz), dot(d, globals.cam_u.xyz) * 0.8));
        if (ahead > 0.6 && r < 1.8) {
            let k = 1.0 - smoothstep(0.8, 1.8, r);
            let c = vec2<u32>(floor(v.clip.xy / (2.0 * SSF))) % vec2<u32>(4u);
            let bayer = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
            if ((bayer[c.y * 4u + c.x] + 0.5) / 16.0 < 0.85 * k) { discard; }
        }
    }
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
    if (v.kind == 5.0) {
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
        // snow on the upper faces of leaves
        if (v.mat > 1.5 && v.mat < 2.5 && v.glow < 0.5) {
            col = mix(col, vec3<f32>(0.9, 0.94, 1.0), exposure * globals.env.w * 0.45 * smoothstep(0.45, 0.8, nm.y));
        }
        // in the rain everything outdoors is wet: darker, and its upper faces catch a cool sheen of the sky
        if (v.glow < 0.5) {
            let wet = exposure * globals.env.z;
            col = col * (1.0 - 0.16 * wet) + vec3<f32>(0.04, 0.05, 0.06) * wet * smoothstep(0.6, 0.9, nm.y);
        }
    }
    if (cave_model) {
        return vec4<f32>(cave_shade(v, col, n, exposure), alpha);
    }
    // light: sky, sun (held back by shadows and clouds) and fires; things that glow light themselves
    var lit: vec3<f32>;
    if (v.glow < 0.5) {
        // the player keeps most of its colour in the shade, so it can always be picked out
        var shade_k = 1.0;
        let pd = v.world - globals.peek.xyz;
        if (v.kind == 4.0 && globals.peek.w > 0.5 && abs(pd.x) < 0.45 && abs(pd.z) < 0.45 && abs(pd.y) < 1.1) { shade_k = 0.4; }
        var l = env_light(n, v.world, shade_k);
        if (v.kind == 4.0) {
            // models: a touch brighter than the ground so they stand out, in clear bands
            l = quantize(l * 1.12, 0.13);
        } else {
            l = quantize(l, 0.06);
        }
        lit = col * (l * globals.light.rgb + fires_at(n, v.world));
    } else {
        lit = col * mix(globals.light.rgb, vec3<f32>(1.0), v.glow);
    }
    return vec4<f32>(finish(lit, v.wpos, v.glow), alpha);
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
