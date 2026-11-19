// Copy the frame onto the window. `gain` places scene white on paper white.
// Channels above 1 stay above paper white; this pass does not clip them.

struct Tone {
    gain: f32,
    _pad: vec3<f32>,
}

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> tone: Tone;

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
    let colour = textureSample(tex, samp, in.uv);
    return vec4<f32>(max(colour.rgb, vec3<f32>(0.0)) * tone.gain, colour.a);
}
