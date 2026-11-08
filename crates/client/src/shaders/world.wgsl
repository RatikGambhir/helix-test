// World, water, entity and sky shaders. Colours are worked in gamma space (like the textures)
// and converted at the end when the target is an sRGB format.

struct U {
    vp: mat4x4<f32>,
    inv_vp: mat4x4<f32>,
    cam: vec4<f32>,      // eye xyz, time seconds
    sun: vec4<f32>,      // sun direction, daylight 0..1
    sky_top: vec4<f32>,
    sky_hor: vec4<f32>,  // rgb, fog end
    fog: vec4<f32>,      // rgb, fog start
    misc: vec4<f32>,     // srgb target, underwater, flash, unused
};

@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) light: vec4<f32>,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) light: vec4<f32>,
    @location(2) world: vec3<f32>,
};

fn flags_of(l: vec4<f32>) -> u32 {
    return u32(round(l.w * 255.0));
}

@vertex
fn vs_main(v: VIn) -> VOut {
    var p = v.pos;
    let f = flags_of(v.light);
    let t = u.cam.w;
    if (f == 1u) {
        p.x += sin(t * 1.6 + p.z * 0.7 + p.y * 0.5) * 0.025;
        p.z += cos(t * 1.3 + p.x * 0.6) * 0.02;
    }
    if (f == 2u) {
        p.y += sin(t * 1.8 + p.x * 0.9 + p.z * 0.7) * 0.035 - 0.03;
    }
    var o: VOut;
    o.clip = u.vp * vec4<f32>(p, 1.0);
    o.uv = v.uv;
    o.light = v.light;
    o.world = p;
    return o;
}

fn to_target(c: vec3<f32>) -> vec3<f32> {
    let c2 = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    if (u.misc.x > 0.5) {
        return pow(c2, vec3<f32>(2.2));
    }
    return c2;
}

fn fogged(c: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let d = distance(world, u.cam.xyz);
    let f = clamp((d - u.fog.w) / max(u.sky_hor.w - u.fog.w, 1.0), 0.0, 1.0);
    return mix(c, u.fog.xyz, f * f * (3.0 - 2.0 * f));
}

fn lit(rgb: vec3<f32>, light: vec4<f32>, world: vec3<f32>) -> vec3<f32> {
    let f = flags_of(light);
    if (f == 3u) {
        return rgb;
    }
    let sky = light.x * u.sun.w;
    let blk = light.y;
    let l = max(sky, blk);
    let b = pow(l, 1.45) * 0.9 + 0.07;
    let warm = clamp((blk - sky) * 2.0, 0.0, 1.0);
    let tint = mix(vec3<f32>(1.0), vec3<f32>(1.0, 0.85, 0.66), warm);
    let night = 1.0 - u.sun.w;
    let cool = mix(vec3<f32>(1.0), vec3<f32>(0.72, 0.8, 1.0), night * (1.0 - warm));
    var c = rgb * b * light.z * tint * cool;
    if (f == 4u) {
        c = mix(c, vec3<f32>(1.0, 0.1, 0.1), 0.45);
    }
    return fogged(c, world);
}

@fragment
fn fs_main(i: VOut) -> @location(0) vec4<f32> {
    let t = textureSample(atlas, samp, i.uv);
    if (t.a < 0.35) {
        discard;
    }
    let c = lit(t.rgb, i.light, i.world) + vec3<f32>(u.misc.z);
    return vec4<f32>(to_target(c), 1.0);
}

@fragment
fn fs_blend(i: VOut) -> @location(0) vec4<f32> {
    let t = textureSample(atlas, samp, i.uv);
    if (t.a < 0.02) {
        discard;
    }
    var c = lit(t.rgb, i.light, i.world);
    if (flags_of(i.light) == 2u) {
        // A little sky reflection on water.
        let view = normalize(i.world - u.cam.xyz);
        let fres = pow(1.0 - abs(view.y), 3.0);
        c = mix(c, fogged(u.sky_hor.xyz * u.sun.w, i.world), fres * 0.5);
    }
    return vec4<f32>(to_target(c + vec3<f32>(u.misc.z)), t.a);
}

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_sky(@builtin(vertex_index) i: u32) -> SkyOut {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    var o: SkyOut;
    o.clip = vec4<f32>(p, 0.0, 1.0);
    o.ndc = p;
    return o;
}

fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let s = f * f * (3.0 - 2.0 * f);
    let a = hash2(i);
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, s.x), mix(c, d, s.x), s.y);
}

@fragment
fn fs_sky(i: SkyOut) -> @location(0) vec4<f32> {
    let far = u.inv_vp * vec4<f32>(i.ndc, 1.0, 1.0);
    let near = u.inv_vp * vec4<f32>(i.ndc, 0.0, 1.0);
    let dir = normalize(far.xyz / far.w - near.xyz / near.w);
    if (u.misc.y > 0.5) {
        return vec4<f32>(to_target(u.fog.xyz), 1.0);
    }
    let h = dir.y;
    var c = mix(u.sky_hor.xyz, u.sky_top.xyz, pow(clamp(h, 0.0, 1.0), 0.55));
    if (h < 0.0) {
        c = mix(u.sky_hor.xyz, u.sky_hor.xyz * 0.75, clamp(-h * 4.0, 0.0, 1.0));
    }
    let night = 1.0 - u.sun.w;
    // Stars.
    if (h > 0.0 && night > 0.15) {
        let g = floor(dir * 260.0);
        let s = fract(sin(dot(g, vec3<f32>(12.9898, 78.233, 37.719))) * 43758.5453);
        if (s > 0.9975) {
            c += vec3<f32>(night * (s - 0.9975) * 400.0);
        }
    }
    // A square sun and moon, as blocky as everything else.
    let sun = u.sun.xyz;
    let side = normalize(cross(sun, vec3<f32>(0.0, 0.0, 1.0)));
    let up = cross(side, sun);
    let sd = dot(dir, sun);
    if (sd > 0.0) {
        let q = vec2<f32>(dot(dir, side), dot(dir, up)) / sd;
        if (max(abs(q.x), abs(q.y)) < 0.055) {
            c = vec3<f32>(1.0, 0.95, 0.75);
        }
        c += vec3<f32>(1.0, 0.65, 0.35) * pow(sd, 40.0) * 0.4;
    }
    let md = dot(dir, -sun);
    if (md > 0.0) {
        let q = vec2<f32>(dot(dir, side), dot(dir, up)) / md;
        if (max(abs(q.x), abs(q.y)) < 0.04) {
            let crater = hash2(floor(q * 60.0)) * 0.15;
            c = vec3<f32>(0.85 - crater, 0.87 - crater, 0.95 - crater);
        }
    }
    // Flat blocky clouds drifting east.
    if (dir.y > 0.02) {
        let height = 200.0;
        let tc = (height - u.cam.y) / dir.y;
        if (tc > 0.0 && tc < 3000.0) {
            let p = u.cam.xz + dir.xz * tc + vec2<f32>(u.cam.w * 1.5, 0.0);
            // Blocky 12×12 cells whose cover follows a smooth field, so clouds clump.
            let cell = floor(p / 12.0);
            let n = vnoise(cell / 7.0) * 0.75 + vnoise(cell / 2.5 + 31.0) * 0.25;
            if (n > 0.62) {
                let fade = clamp(1.0 - tc / 3000.0, 0.0, 1.0);
                let cloud = vec3<f32>(0.96) * max(u.sun.w, 0.18) + vec3<f32>(0.02);
                c = mix(c, cloud, 0.85 * fade);
            }
        }
    }
    return vec4<f32>(to_target(c + vec3<f32>(u.misc.z)), 1.0);
}
