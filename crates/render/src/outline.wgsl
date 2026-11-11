// Ink lines at the small picture's resolution: a texel gets ink when a neighbour behind it is much farther away,
// so the nearer object is outlined by one texel.
@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var depth: texture_depth_2d;

@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

fn d_at(q: vec2<i32>, lim: vec2<i32>) -> f32 {
    return textureLoad(depth, clamp(q, vec2<i32>(0), lim), 0);
}

@fragment fn fs(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let lim = vec2<i32>(textureDimensions(scene)) - vec2<i32>(1);
    let q = vec2<i32>(frag.xy);
    let c = textureLoad(scene, clamp(q, vec2<i32>(0), lim), 0).rgb;
    let d0 = d_at(q, lim);
    let dl = d_at(q + vec2<i32>(-1, 0), lim);
    let dr = d_at(q + vec2<i32>(1, 0), lim);
    let du = d_at(q + vec2<i32>(0, -1), lim);
    let dd = d_at(q + vec2<i32>(0, 1), lim);
    // the ground's own slope is the same on both sides, so only a real step (more than a slope) counts
    let sx = abs(dr - dl) * 0.5;
    let sy = abs(dd - du) * 0.5;
    let step_x = max(dl - d0 - sx, dr - d0 - sx);
    let step_y = max(du - d0 - sy, dd - d0 - sy);
    let e = max(step_x, step_y);
    let ink = select(0.0, 1.0, e > 0.00045);
    var col = mix(c, c * vec3<f32>(0.16, 0.13, 0.15), ink);
    // the player's hidden part (alpha 0 there, 0.5 on the rest of its body): a bright line just outside it
    let a0 = textureLoad(scene, clamp(q, vec2<i32>(0), lim), 0).a;
    if (a0 > 0.75) {
        var near = false;
        for (var j = -1; j <= 1; j = j + 1) {
            for (var i = -1; i <= 1; i = i + 1) {
                if (textureLoad(scene, clamp(q + vec2<i32>(i, j), vec2<i32>(0), lim), 0).a < 0.25) { near = true; }
            }
        }
        if (near) { col = vec3<f32>(1.0, 0.93, 0.62); }
    }
    return vec4<f32>(col, 1.0);
}
