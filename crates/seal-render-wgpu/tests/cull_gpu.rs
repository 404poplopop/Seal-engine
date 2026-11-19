//! GPU culling must match the CPU reference (except instances touching a
//! plane within rounding), and its shader must validate. Parity skips
//! without a GPU.

use seal_render::cull::*;
use seal_render_wgpu::cull::{cull_gpu, read_back, CULL_WGSL};
use seal_render_wgpu::WgpuDevice;

#[test]
fn the_cull_shader_validates() {
    let module = naga::front::wgsl::parse_str(CULL_WGSL).expect("parse");
    let mut v = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    );
    v.validate(&module).expect("validate");
    assert_eq!(module.entry_points[0].workgroup_size, [64, 1, 1]);
}

fn scene(n: usize) -> Vec<Instance> {
    let mut s = 12345u32;
    let mut r = move || {
        s = s.wrapping_mul(1664525).wrapping_add(1013904223);
        (s >> 8) as f32 / (1 << 24) as f32
    };
    (0..n)
        .map(|_| {
            let c = [r() * 80.0 - 40.0, r() * 80.0 - 40.0, r() * 20.0 - 10.0];
            let h = 0.2 + r() * 1.5;
            Instance {
                min: [c[0] - h, c[1] - h, c[2] - h],
                max: [c[0] + h, c[1] + h, c[2] + h],
                mesh: (r() * 3.0) as u32 % 3,
            }
        })
        .collect()
}

#[test]
fn gpu_culling_matches_the_cpu() {
    let gpu = match WgpuDevice::headless() {
        Ok(g) => g,
        Err(e) => return eprintln!("skipping GPU cull parity: {e}"),
    };
    let f = Frustum::from_matrix(&view_projection(
        [0.0, -30.0, 6.0],
        [0.0, 0.0, 0.0],
        1.0,
        1.6,
        0.1,
        60.0,
    ));
    let inst = scene(5000);
    let meshes = [
        MeshDraw { index_count: 36, first_index: 0, base_vertex: 0 },
        MeshDraw { index_count: 6, first_index: 36, base_vertex: 24 },
        MeshDraw { index_count: 120, first_index: 42, base_vertex: 28 },
    ];
    let cpu = cull_cpu(&f, &inst, &meshes);
    let g = read_back(&gpu, &cull_gpu(&gpu, &f, &inst, &meshes));
    let shown = cpu.args.iter().map(|a| a[1]).sum::<u32>();
    assert!(shown > 100 && (shown as usize) < inst.len(), "scene is partly visible: {shown}");
    for m in 0..meshes.len() {
        assert_eq!(
            [g.args[m][0], g.args[m][2], g.args[m][3], g.args[m][4]],
            [cpu.args[m][0], cpu.args[m][2], cpu.args[m][3], cpu.args[m][4]]
        );
        let (a, b) = (cpu.mesh_ids(m), g.mesh_ids(m));
        for id in a.iter().filter(|i| !b.contains(i)).chain(b.iter().filter(|i| !a.contains(i))) {
            let i = inst[*id as usize];
            assert!(
                f.margin(i.min, i.max).abs() < 1e-3,
                "instance {id} differs and is not on a plane"
            );
        }
    }
}
