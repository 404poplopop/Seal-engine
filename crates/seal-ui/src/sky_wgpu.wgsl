struct Sky {
    yaw: f32,
    pitch: f32,
    spread: f32,
    _pad: f32,
}

@group(0) @binding(0) var sky_tex: texture_cube<f32>;
@group(0) @binding(1) var sky_samp: sampler;
@group(0) @binding(2) var<uniform> sky: Sky;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> Out {
    let x = f32((vi << 1u) & 2u);
    let y = f32(vi & 2u);
    var out: Out;
    out.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, y);
    return out;
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    let ndc = vec2<f32>(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0);
    let yaw = radians(sky.yaw);
    let pitch = radians(sky.pitch);
    let forward = vec3<f32>(sin(yaw) * cos(pitch), sin(pitch), cos(yaw) * cos(pitch));
    let right = vec3<f32>(cos(yaw), 0.0, -sin(yaw));
    let up = cross(forward, right);
    let dir = normalize(forward + right * ndc.x * sky.spread + up * ndc.y * sky.spread);
    return textureSampleLevel(sky_tex, sky_samp, dir, 0.0);
}
