// The small picture is enlarged with hard edges (a texel is a whole number of screen pixels) and given a light
// film grade. Nothing here touches individual texels, so the art stays exactly as painted.
struct P {
    a: vec4<f32>,   // x: screen px per texel, y: unused, zw: shift of the window over the picture (screen px)
    b: vec4<f32>,   // x: window width, y: window height, z: night 0..1, w: cinema 0..1
    c: vec4<f32>,   // x: 1 rain / 2 snow, y: how much of it is falling 0..1, z: clock in seconds, w: wind across the screen
};
@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var<uniform> p: P;

@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

@fragment fn fs(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let lim = vec2<i32>(textureDimensions(scene)) - vec2<i32>(1);
    let q = vec2<i32>(floor((frag.xy + p.a.zw) / p.a.x));
    var g = pow(textureLoad(scene, clamp(q, vec2<i32>(0), lim), 0).rgb, vec3<f32>(1.0 / 2.2));
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
        let hsh = fract(sin(vec2<f32>(f32(q.x), f32(q.x / 3)) * vec2<f32>(12.9898, 78.233)) * 43758.5453);
        if (p.c.x < 1.5) {
            // each column has its own speed; along it, short streaks fall in separate cells, so the rain reads
            // as scattered drops instead of long dashed lines down the whole screen
            let sp = 38.0 + 30.0 * hsh.x;
            let y = (f32(q.y) + p.c.z * sp + hsh.y * 200.0) / 9.0;
            let cell = floor(y);
            let pick = fract(sin(f32(q.x) * 91.17 + cell * 47.31) * 43758.5453);
            if (fract(y) < 0.45 && pick < 0.035 * wa) {
                g = mix(g, vec3<f32>(0.72, 0.82, 0.95), 0.5);
            }
            g = g * (1.0 - 0.12 * wa);
        } else {
            let cell = vec2<i32>(q.x / 5, q.y / 5);
            let fall = i32(floor(p.c.z * (6.0 + 5.0 * fract(f32(cell.x) * 0.37))));
            let c2 = vec2<i32>(cell.x, (q.y + fall) / 5);
            let r = fract(sin(vec2<f32>(c2) * vec2<f32>(12.9898, 78.233)) * 43758.5453);
            let ix = i32(fract(r.y * 17.0) * 5.0);
            let iy = i32(r.y * 5.0);
            if (r.x < 0.12 + 0.3 * wa && (q.x % 5) == ix && ((q.y + fall) % 5) == iy) {
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
