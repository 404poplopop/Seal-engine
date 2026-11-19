// Lit mesh. 23 floats: the 22 the ordinary path writes, then a light scale.
// position, lit colour, shadowed colour, shadow uv/depth/bias, uv/layer/slice, normal, light.

struct Params {
    texel: f32,
    has_shadow: f32,
    hdr: f32,
    kind: f32,
    slices: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

@group(0) @binding(0) var shadow_tex: texture_2d<f32>;
@group(0) @binding(1) var shadow_samp: sampler;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var albedo2d: texture_2d<f32>;
@group(0) @binding(4) var albedo_samp: sampler;
@group(0) @binding(5) var albedo_array: texture_2d_array<f32>;
@group(0) @binding(6) var albedo_vol: texture_3d<f32>;
@group(0) @binding(7) var albedo_cube: texture_cube<f32>;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) dark: vec4<f32>,
    @location(3) shadow: vec4<f32>,
    @location(4) tex: vec4<f32>,
    @location(5) normal: vec3<f32>,
    @location(6) light: f32,
}

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) dark: vec4<f32>,
    @location(2) shadow: vec4<f32>,
    @location(3) tex: vec4<f32>,
    @location(4) normal: vec3<f32>,
    @location(5) light: f32,
}

@vertex
fn vs(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = vec4<f32>(in.pos, 1.0);
    out.color = in.color;
    out.dark = in.dark;
    out.shadow = in.shadow;
    out.tex = in.tex;
    out.normal = in.normal;
    out.light = in.light;
    return out;
}

fn nearest(uv: vec2<f32>) -> f32 {
    let c = textureSampleLevel(shadow_tex, shadow_samp, uv, 0.0).rgb;
    return dot(c, vec3<f32>(65536.0, 256.0, 1.0)) * 255.0 / 16777215.0;
}

fn albedo_of(tex: vec4<f32>, normal: vec3<f32>) -> vec4<f32> {
    let k = params.kind;
    if (k < 0.5) {
        return vec4<f32>(1.0, 1.0, 1.0, 1.0);
    }
    if (k < 1.5) {
        return textureSampleLevel(albedo2d, albedo_samp, tex.xy, 0.0);
    }
    if (k < 2.5) {
        return textureSampleLevel(albedo_array, albedo_samp, tex.xy, i32(tex.z + 0.5), 0.0);
    }
    if (k < 3.5) {
        let w = (tex.w + 0.5) / max(params.slices, 1.0);
        return textureSampleLevel(albedo_vol, albedo_samp, vec3<f32>(tex.xy, w), 0.0);
    }
    return textureSampleLevel(albedo_cube, albedo_samp, normal, 0.0);
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    var sun = 1.0;
    if (params.has_shadow > 0.5 && in.shadow.w > 0.0) {
        let grid = in.shadow.xy / params.texel - 0.5;
        let cell = floor(grid);
        let f = grid - cell;
        var lit = 0.0;
        for (var j = 0; j < 4; j = j + 1) {
            for (var i = 0; i < 4; i = i + 1) {
                var wx = 1.0;
                var wy = 1.0;
                if (i == 0) {
                    wx = 1.0 - f.x;
                } else if (i == 3) {
                    wx = f.x;
                }
                if (j == 0) {
                    wy = 1.0 - f.y;
                } else if (j == 3) {
                    wy = f.y;
                }
                let uv = (cell + vec2<f32>(f32(i) - 1.0, f32(j) - 1.0) + 0.5) * params.texel;
                let outside = uv.x < 0.0 || uv.y < 0.0 || uv.x >= 1.0 || uv.y >= 1.0;
                var hit = 0.0;
                if (outside || in.shadow.z - in.shadow.w <= nearest(uv)) {
                    hit = 1.0;
                }
                lit = lit + wx * wy * hit;
            }
        }
        sun = lit / 9.0;
    }
    let tint = albedo_of(in.tex, in.normal);
    let colour = mix(in.dark, in.color, sun) * tint;
    // `light` is 1 for an ordinary lamp and above 1 for a bright lamp or glow.
    var rgb = max(colour.rgb, vec3<f32>(0.0)) * max(in.light, 0.0);
    if (params.hdr < 0.5) {
        rgb = min(rgb, vec3<f32>(1.0));
    }
    return vec4<f32>(rgb, colour.a);
}
