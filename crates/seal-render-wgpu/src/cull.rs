//! GPU-driven instancing: frustum culling as a compute pass that writes
//! `DrawIndexedIndirectArgs` per mesh plus the visible instance list, so
//! draws need no CPU read-back. `seal_render::cull::cull_cpu` is the
//! reference it is tested against.

use seal_render::cull::{mesh_bases, Culled, Frustum, Instance, MeshDraw};
use wgpu::util::DeviceExt;

use crate::WgpuDevice;

pub const CULL_WGSL: &str = include_str!("cull.wgsl");

/// GPU buffers after a cull. `args` is usable as an indirect buffer, one
/// 20-byte record per mesh; `visible` holds instance ids per mesh slice.
pub struct GpuCull {
    pub args: wgpu::Buffer,
    pub visible: wgpu::Buffer,
    pub meshes: usize,
    pub instances: usize,
}

fn words(v: &[u32]) -> Vec<u8> {
    let v: &[u32] = if v.is_empty() { &[0] } else { v };
    v.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// Uploads instances and culls them against `frustum` on the GPU.
pub fn cull_gpu(
    gpu: &WgpuDevice,
    frustum: &Frustum,
    instances: &[Instance],
    meshes: &[MeshDraw],
) -> GpuCull {
    let device = &gpu.device;
    let mut info: Vec<u32> = vec![instances.len() as u32, meshes.len() as u32, 0, 0];
    for p in &frustum.planes {
        info.extend(p.iter().map(|f| f.to_bits()));
    }
    let mut inst: Vec<u32> = Vec::with_capacity(instances.len() * 8);
    for i in instances {
        inst.extend(i.min.iter().map(|f| f.to_bits()));
        inst.push(0);
        inst.extend(i.max.iter().map(|f| f.to_bits()));
        inst.push(i.mesh);
    }
    let bases = mesh_bases(instances, meshes.len());
    let args0: Vec<u32> = meshes
        .iter()
        .zip(&bases)
        .flat_map(|(m, b)| [m.index_count, 0, m.first_index, m.base_vertex as u32, *b])
        .collect();

    let info_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("cull info"),
        contents: &words(&info),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let inst_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("cull instances"),
        contents: &words(if inst.is_empty() { &[0; 8] } else { &inst }),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let args = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("indirect draws"),
        contents: &words(&args0),
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::INDIRECT
            | wgpu::BufferUsages::COPY_SRC,
    });
    let visible = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("visible instances"),
        contents: &words(&vec![0; instances.len()]),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("cull"),
        source: wgpu::ShaderSource::Wgsl(CULL_WGSL.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("cull"),
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("cull"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: info_buf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: inst_buf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: args.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 3, resource: visible.as_entire_binding() },
        ],
    });
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("cull") });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("cull"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        let groups = (instances.len() as u32).div_ceil(64);
        if groups > 0 {
            pass.dispatch_workgroups(groups, 1, 1);
        }
    }
    gpu.queue.submit([encoder.finish()]);
    GpuCull { args, visible, meshes: meshes.len(), instances: instances.len() }
}

/// Whether `device` can draw a cull with indirect draws (each mesh starts
/// at its own first instance, which needs `INDIRECT_FIRST_INSTANCE`).
pub fn can_draw_indirect(gpu: &WgpuDevice) -> bool {
    gpu.device.features().contains(wgpu::Features::INDIRECT_FIRST_INSTANCE)
}

/// Issues one indexed indirect draw per mesh from a cull's `args`. The
/// vertex shader finds its instance as `visible[instance_index]`.
/// Check `can_draw_indirect` first; without it, read back and draw directly.
pub fn draw_culled(pass: &mut wgpu::RenderPass<'_>, cull: &GpuCull) {
    for m in 0..cull.meshes {
        pass.draw_indexed_indirect(&cull.args, (m * 20) as u64);
    }
}

/// Reads a cull back (tests and debugging only; drawing never needs this).
pub fn read_back(gpu: &WgpuDevice, cull: &GpuCull) -> Culled {
    let device = &gpu.device;
    let stage = |src: &wgpu::Buffer| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cull read-back"),
            size: src.size(),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        })
    };
    let (a, v) = (stage(&cull.args), stage(&cull.visible));
    let mut encoder = device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("cull read-back") });
    encoder.copy_buffer_to_buffer(&cull.args, 0, &a, 0, cull.args.size());
    encoder.copy_buffer_to_buffer(&cull.visible, 0, &v, 0, cull.visible.size());
    gpu.queue.submit([encoder.finish()]);
    a.map_async(wgpu::MapMode::Read, .., |r| r.expect("map cull args"));
    v.map_async(wgpu::MapMode::Read, .., |r| r.expect("map visible"));
    device.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
    let read = |b: &wgpu::Buffer| -> Vec<u32> {
        let m = b.get_mapped_range(..).expect("mapped");
        m.chunks_exact(4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]])).collect()
    };
    let flat = read(&a);
    let mut visible = read(&v);
    visible.truncate(cull.instances);
    let args = (0..cull.meshes)
        .map(|m| [flat[m * 5], flat[m * 5 + 1], flat[m * 5 + 2], flat[m * 5 + 3], flat[m * 5 + 4]])
        .collect();
    Culled { args, visible }
}
