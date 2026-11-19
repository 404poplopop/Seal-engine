// Percent-closer shadow lookup, the GPU twin of seal_render::shadow::ShadowMap::lit.
// One invocation per point: reads the light's depth grid, writes the lit fraction (1 lit, 0 shadowed).
struct Params {
    right: vec4<f32>,   // xyz light-space right axis
    up: vec4<f32>,      // xyz light-space up axis
    dir: vec4<f32>,     // xyz light travel direction
    center: vec4<f32>,  // xyz centre, w = radius
    grid: vec4<f32>,    // x = texels per side, y = depth bias, z = pcf radius, w = point count
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> depth: array<f32>;
@group(0) @binding(2) var<storage, read> points: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> lit: array<f32>;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let n = u32(params.grid.w);
    if (gid.x >= n) { return; }
    let d = points[gid.x].xyz - params.center.xyz;
    let r = params.center.w;
    let t = vec3<f32>(dot(d, params.right.xyz) / r, dot(d, params.up.xyz) / r, dot(d, params.dir.xyz) / r) * 0.5 + vec3<f32>(0.5);
    let size = i32(params.grid.x);
    let texel = 1.0 / params.grid.x;
    let k = i32(params.grid.z);
    var lit_count = 0.0;
    var total = 0.0;
    for (var dy = -k; dy <= k; dy = dy + 1) {
        for (var dx = -k; dx <= k; dx = dx + 1) {
            total = total + 1.0;
            let u = t.x + f32(dx) * texel;
            let v = t.y + f32(dy) * texel;
            var shadowed = false;
            if (u >= 0.0 && u < 1.0 && v >= 0.0 && v < 1.0) {
                let i = i32(v * params.grid.x) * size + i32(u * params.grid.x);
                shadowed = (t.z - params.grid.y) > depth[i];
            }
            if (!shadowed) { lit_count = lit_count + 1.0; }
        }
    }
    lit[gid.x] = lit_count / total;
}
