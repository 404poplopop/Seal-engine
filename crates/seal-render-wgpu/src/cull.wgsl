// GPU frustum culling: twin of seal_render::cull::cull_cpu.
// One thread per instance. Visible instances append their id to their
// mesh's slice of `visible` and bump that mesh's indirect instance count.

struct Info { count: u32, meshes: u32, pad0: u32, pad1: u32, planes: array<vec4<f32>, 6> }

struct Inst { min: vec4<f32>, max: vec4<f32> } // max.w holds the mesh index as bits

@group(0) @binding(0) var<uniform> info: Info;
@group(0) @binding(1) var<storage, read> instances: array<Inst>;
// 5 words per mesh: index count, instance count, first index, base vertex, first instance.
@group(0) @binding(2) var<storage, read_write> args: array<atomic<u32>>;
@group(0) @binding(3) var<storage, read_write> visible: array<u32>;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let id = gid.x;
    if (id >= info.count) { return; }
    let inst = instances[id];
    var worst = 3.4e38;
    for (var i = 0u; i < 6u; i = i + 1u) {
        let p = info.planes[i];
        let c = select(inst.min.xyz, inst.max.xyz, p.xyz >= vec3<f32>(0.0));
        worst = min(worst, p.x * c.x + p.y * c.y + p.z * c.z + p.w);
    }
    if (worst < 0.0) { return; }
    let mesh = bitcast<u32>(inst.max.w);
    let slot = atomicAdd(&args[mesh * 5u + 1u], 1u);
    let first = atomicLoad(&args[mesh * 5u + 4u]);
    visible[first + slot] = id;
}
