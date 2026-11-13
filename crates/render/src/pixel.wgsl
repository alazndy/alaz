// The small picture is enlarged with hard edges (sharp bilinear: even blocks at any scale) and given a light
// film grade. Nothing here touches individual texels, so the art stays exactly as painted.
struct P {
    a: vec4<f32>,   // x: screen px per texel, y: unused, zw: shift of the window over the picture (screen px)
    b: vec4<f32>,   // x: window width, y: window height, z: night 0..1, w: cinema 0..1
    c: vec4<f32>,   // x: 1 rain / 2 snow, y: how much of it is falling 0..1, z: clock in seconds, w: wind across the screen
};
@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var<uniform> p: P;

fn hash2(a: f32, b: f32) -> f32 {
    return fract(sin(a * 12.9898 + b * 78.233) * 43758.5453);
}

// Rain, drawn over the picture on its own texel grid.
// The sky is overcast: colours drain toward a cool grey and the far (upper) part of the view sinks into a veil of
// rain. The drops fall in three layers: far ones short, faint and slow, near ones long, bright and fast, so the
// rain has depth. Each is a slanted pixel line (the wind sets the slant), and showers come and go.
fn rain(g0: vec3<f32>, q: vec2<i32>, wa: f32, up_down: f32) -> vec3<f32> {
    var g = g0;
    let t = p.c.z;
    let night = p.b.z;
    // showers: the rain swells and eases over tens of seconds
    let shower = wa * (0.6 + 0.4 * (0.5 + 0.5 * sin(t * 0.071) * sin(t * 0.029 + 1.3)));
    // overcast: less colour, cooler, darker
    let l = dot(g, vec3<f32>(0.30, 0.59, 0.11));
    g = mix(g, vec3<f32>(l) * vec3<f32>(0.93, 0.98, 1.05), 0.38 * wa);
    g = g * (1.0 - 0.12 * wa);
    // the veil: further away (up the screen) the rain hides more of the land
    let far = 1.0 - up_down;
    let veil = vec3<f32>(0.58, 0.63, 0.69) * (1.0 - 0.75 * night);
    g = mix(g, veil, 0.22 * shower * far * far);
    // drops: (length in texels, speed in texels a second, how many, brightness, how much the wind slants it)
    let lens = array<f32, 3>(2.0, 4.0, 7.0);
    let speeds = array<f32, 3>(55.0, 95.0, 150.0);
    let counts = array<f32, 3>(0.10, 0.055, 0.03);
    let bright = array<f32, 3>(0.22, 0.38, 0.6);
    let slants = array<f32, 3>(0.55, 0.8, 1.0);
    let drop_col = vec3<f32>(0.80, 0.87, 0.96) * (1.0 - 0.6 * night);
    for (var k = 0; k < 3; k = k + 1) {
        let sl = p.c.w * 0.7 * slants[k] + 0.12;
        let fk = f32(k);
        // shear the grid so a column runs along the slant: a drop is a clean pixel diagonal
        let x = floor(f32(q.x) - f32(q.y) * sl);
        let hx = hash2(x, fk * 17.0 + 3.0);
        let speed = speeds[k] * (0.85 + 0.3 * hx);
        // (screen y grows downward: subtracting time moves the pattern down, so the drops fall)
        let y = f32(q.y) - t * speed + hx * 977.0;
        let span = lens[k] * 6.0;
        let cell = floor(y / span);
        let inside = y - cell * span;
        if (inside < lens[k] && hash2(x * 1.31 + fk * 7.7, cell) < counts[k] * shower) {
            // the head (lower end) of the streak is the brightest
            let head = select(0.7, 1.0, inside >= lens[k] - 1.0);
            g = mix(g, drop_col, bright[k] * head);
        }
    }
    return g;
}

@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

@fragment fn fs(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let lim = vec2<i32>(textureDimensions(scene)) - vec2<i32>(1);
    let q = vec2<i32>(floor((frag.xy + p.a.zw) / p.a.x));
    // sharp bilinear: a texel is a hard block, blended with its neighbour only across the one screen pixel where
    // their edge falls. A texel is not a whole number of screen pixels at every zoom (2.25 at the default), and
    // picking the nearest texel then makes blocks 2 and 3 pixels wide in a pattern fixed to the screen, which the
    // world crawls under as the camera moves; this way every block is the same width and the picture glides on
    // smoothly, a fraction of a pixel at a time
    let t = (frag.xy + p.a.zw) / p.a.x - vec2<f32>(0.5);
    let i = floor(t);
    let f = t - i;
    let w = clamp((f - vec2<f32>(0.5)) * p.a.x + vec2<f32>(0.5), vec2<f32>(0.0), vec2<f32>(1.0));
    let i0 = clamp(vec2<i32>(i), vec2<i32>(0), lim);
    let i1 = clamp(vec2<i32>(i) + vec2<i32>(1), vec2<i32>(0), lim);
    let c00 = textureLoad(scene, i0, 0).rgb;
    let c10 = textureLoad(scene, vec2<i32>(i1.x, i0.y), 0).rgb;
    let c01 = textureLoad(scene, vec2<i32>(i0.x, i1.y), 0).rgb;
    let c11 = textureLoad(scene, i1, 0).rgb;
    let lin = mix(mix(c00, c10, w.x), mix(c01, c11, w.x), w.y);
    var g = pow(lin, vec3<f32>(1.0 / 2.2));
    let cine = p.b.w;
    // cinematic grade: richer colour, warm lights over cool shadows, a filmic S-curve, a lifted teal black
    let l = dot(g, vec3<f32>(0.30, 0.59, 0.11));
    g = mix(vec3<f32>(l), g, 1.0 + 0.22 * cine);
    let shade = mix(vec3<f32>(0.86, 0.98, 1.10), vec3<f32>(1.10, 1.02, 0.88), smoothstep(0.12, 0.70, l));
    g = clamp(mix(g, g * shade, cine), vec3<f32>(0.0), vec3<f32>(1.0));
    g = mix(g, g * g * (3.0 - 2.0 * g), 0.42 * cine);
    g = g + vec3<f32>(0.010, 0.016, 0.022) * cine * (1.0 - l);
    // a soft bloom: the brightest picture parts glow into their surroundings
    let bl = max(l - 0.78, 0.0);
    g = g + vec3<f32>(1.0, 0.92, 0.75) * bl * 0.35 * cine;
    // weather: rain streaks or snowflakes over the picture, on the texel grid
    let wa = p.c.y;
    if (wa > 0.01) {
        // the wind slants it: each row is shifted sideways by how far down it is, and the shift creeps on with time
        let q0 = q;
        let q = vec2<i32>(q0.x + i32(floor((f32(q0.y) + p.c.z * 40.0) * p.c.w * 0.7)), q0.y);
        if (p.c.x < 1.5) {
            g = rain(g, q0, wa, frag.y / p.b.y);
        } else {
            let cell = vec2<i32>(q.x / 5, q.y / 5);
            let fall = i32(floor(p.c.z * (6.0 + 5.0 * fract(f32(cell.x) * 0.37))));
            // (screen y grows downward: the flakes' grid moves down with time; the offset keeps it positive)
            let yy = q.y - fall + 100000;
            let c2 = vec2<i32>(cell.x, yy / 5);
            let r = fract(sin(vec2<f32>(c2) * vec2<f32>(12.9898, 78.233)) * 43758.5453);
            let ix = i32(fract(r.y * 17.0) * 5.0);
            let iy = i32(r.y * 5.0);
            if (r.x < 0.12 + 0.3 * wa && (q.x % 5) == ix && (yy % 5) == iy) {
                g = vec3<f32>(0.95, 0.97, 1.0);
            }
            g = mix(g, vec3<f32>(0.88, 0.92, 1.0), 0.06 * wa);
        }
    }
    // vignette
    let uv = frag.xy / p.b.xy - vec2<f32>(0.5);
    let v = uv * vec2<f32>(p.b.x / p.b.y, 1.0) * 1.15;
    g = g * (1.0 - 0.42 * cine * smoothstep(0.32, 1.05, length(v)) * (0.8 + 0.4 * p.b.z));
    return vec4<f32>(pow(clamp(g, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(2.2)), 1.0);
}
