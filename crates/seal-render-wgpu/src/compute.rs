//! Seal kernels on the GPU: runs a kernel's WGSL form (see
//! `seal_kernel::gpu`) as a compute pass, with the CPU backend as the
//! fallback when there is no GPU or the kernel must stay exact.

use seal_kernel::gpu::{self as kgpu, Data32};
use seal_kernel::{Access, Checked, Data, Events};
use wgpu::util::DeviceExt;

use crate::WgpuDevice;

/// Where a kernel ran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Backend {
    Gpu(String),
    /// On the CPU, and why.
    Cpu(String),
}

fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn words(v: &[u32]) -> Vec<u8> {
    // Storage buffers may not be empty.
    let v: &[u32] = if v.is_empty() { &[0] } else { v };
    v.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// Runs a checked, GPU-eligible kernel on `gpu`. Columns are converted to
/// the 32-bit form and back; events come back in row order with 32-bit
/// value bits widened to the kernel's 64-bit form.
pub fn run_kernel_gpu(gpu: &WgpuDevice, k: &Checked, data: &mut Data) -> Result<Events, String> {
    let source = kgpu::lower_wgsl(k)?;
    let kernel = &k.kernel;
    let device = &gpu.device;
    let rows = data.columns.first().map_or(0, Vec::len);
    let narrow = Data32::from_data(k, data);

    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(&kernel.name),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let mut entries = vec![storage_entry(0, true)];
    for (c, col) in kernel.columns.iter().enumerate() {
        entries.push(storage_entry(kgpu::column_binding(c), col.access == Access::Read));
    }
    for b in 0..kernel.buffers.len() {
        let (d, n) = kgpu::buffer_bindings(k, b);
        entries.push(storage_entry(d, false));
        entries.push(storage_entry(n, false));
    }
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("seal kernel"),
        entries: &entries,
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("seal kernel"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(&kernel.name),
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });

    let rw =
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST;
    let mut info = vec![rows as u32];
    info.extend_from_slice(&narrow.params);
    let info_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("info"),
        contents: &words(&info),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let col_bufs: Vec<wgpu::Buffer> = narrow
        .columns
        .iter()
        .map(|col| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("column"),
                contents: &words(col),
                usage: rw,
            })
        })
        .collect();
    let ev_bufs: Vec<(wgpu::Buffer, wgpu::Buffer)> = kernel
        .buffers
        .iter()
        .map(|b| {
            let data = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("events"),
                contents: &words(&vec![0; b.capacity.max(1) * 2]),
                usage: rw,
            });
            let count = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("event count"),
                contents: &words(&[0]),
                usage: rw,
            });
            (data, count)
        })
        .collect();
    let mut bind_entries =
        vec![wgpu::BindGroupEntry { binding: 0, resource: info_buf.as_entire_binding() }];
    for (c, buf) in col_bufs.iter().enumerate() {
        bind_entries.push(wgpu::BindGroupEntry {
            binding: kgpu::column_binding(c),
            resource: buf.as_entire_binding(),
        });
    }
    for (b, (d, n)) in ev_bufs.iter().enumerate() {
        let (db, nb) = kgpu::buffer_bindings(k, b);
        bind_entries.push(wgpu::BindGroupEntry { binding: db, resource: d.as_entire_binding() });
        bind_entries.push(wgpu::BindGroupEntry { binding: nb, resource: n.as_entire_binding() });
    }
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("seal kernel"),
        layout: &layout,
        entries: &bind_entries,
    });

    // Read back every written column, then every event buffer and counter.
    let mut readback: Vec<&wgpu::Buffer> = Vec::new();
    for (c, col) in kernel.columns.iter().enumerate() {
        if col.access != Access::Read {
            readback.push(&col_bufs[c]);
        }
    }
    for (d, n) in &ev_bufs {
        readback.push(d);
        readback.push(n);
    }
    let staging: Vec<wgpu::Buffer> = readback
        .iter()
        .map(|b| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("kernel read-back"),
                size: b.size(),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            })
        })
        .collect();
    let mut encoder = device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("seal kernel") });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(&kernel.name),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        let groups = (rows as u32).div_ceil(kgpu::WORKGROUP);
        if groups > 0 {
            pass.dispatch_workgroups(groups, 1, 1);
        }
    }
    for (src, dst) in readback.iter().zip(&staging) {
        encoder.copy_buffer_to_buffer(src, 0, dst, 0, src.size());
    }
    gpu.queue.submit([encoder.finish()]);
    for s in &staging {
        s.map_async(wgpu::MapMode::Read, .., |r| r.expect("map kernel read-back"));
    }
    device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| e.to_string())?;
    let read = |b: &wgpu::Buffer| -> Vec<u32> {
        let mapped = b.get_mapped_range(..).expect("mapped kernel read-back");
        mapped
            .as_chunks::<4>()
            .0
            .iter()
            .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
            .collect()
    };

    let mut out = Data32 { columns: narrow.columns.clone(), params: narrow.params.clone() };
    let mut next = staging.iter();
    for (c, col) in kernel.columns.iter().enumerate() {
        if col.access != Access::Read {
            let mut v = read(next.next().unwrap());
            v.truncate(rows);
            out.columns[c] = v;
        }
    }
    out.write_back(k, data);
    let mut events = Events { buffers: Vec::new(), overflow: Vec::new() };
    for b in &kernel.buffers {
        let pairs = read(next.next().unwrap());
        let count = read(next.next().unwrap())[0] as usize;
        let kept = count.min(b.capacity);
        events.buffers.push(
            (0..kept)
                .map(|i| (u64::from(pairs[2 * i]), kgpu::from_gpu_bits(b.ty, pairs[2 * i + 1])))
                .collect(),
        );
        events.overflow.push((count - kept) as u64);
    }
    kgpu::sort_events(&mut events);
    Ok(events)
}

/// Runs `k` on the GPU when one is given and the kernel may go there, else
/// on the CPU backend (bit-exact 64-bit). Says where it ran.
pub fn run_kernel(
    gpu: Option<&WgpuDevice>,
    k: &Checked,
    data: &mut Data,
    pool: &seal_jobs::JobPool,
) -> (Events, Backend) {
    let why = match (gpu, kgpu::gpu_eligible(k)) {
        (None, _) => "no GPU".to_string(),
        (Some(_), Err(e)) => e,
        (Some(g), Ok(())) => match run_kernel_gpu(g, k, data) {
            Ok(events) => return (events, Backend::Gpu(g.adapter_name.clone())),
            Err(e) => format!("GPU run failed: {e}"),
        },
    };
    let events = seal_kernel::run_cpu(k, data, pool, seal_kernel::CpuOptions::default());
    (events, Backend::Cpu(why))
}
