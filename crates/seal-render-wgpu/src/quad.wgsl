struct Instance {
    @location(0) c01: vec4<f32>,
    @location(1) c23: vec4<f32>,
    @location(2) uv01: vec4<f32>,
    @location(3) uv23: vec4<f32>,
    @location(4) color: vec4<f32>,
};

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32, inst: Instance) -> Out {
    // Two triangles: corners 0 1 2 and 0 2 3 (top left, top right, bottom right, bottom left).
    var corner = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u)[vi];
    var p: vec2<f32>;
    var t: vec2<f32>;
    switch corner {
        case 0u: { p = inst.c01.xy; t = inst.uv01.xy; }
        case 1u: { p = inst.c01.zw; t = inst.uv01.zw; }
        case 2u: { p = inst.c23.xy; t = inst.uv23.xy; }
        default: { p = inst.c23.zw; t = inst.uv23.zw; }
    }
    var out: Out;
    out.pos = vec4<f32>(p, 0.0, 1.0);
    out.uv = t;
    out.color = inst.color;
    return out;
}

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    return in.color * textureSample(tex, samp, in.uv);
}
