//! Kernels on the GPU must match the 32-bit CPU reference, and fall back to
//! the CPU without a GPU. The parity part skips (with a note) without a GPU.

use seal_jobs::JobPool;
use seal_kernel::gpu::{run_scalar32, sort_events, Data32};
use seal_kernel::*;
use seal_render_wgpu::compute::{run_kernel, run_kernel_gpu, Backend};
use seal_render_wgpu::WgpuDevice;

/// pos += vel * dt; bounce at |pos| > limit; count hits (int div/mod too) and report bounces.
fn bounce(class: ResultClass) -> Checked {
    let mut k = Kernel::new("bounce", class);
    let pos = k.column("pos", Ty::Float, Access::ReadWrite);
    let vel = k.column("vel", Ty::Float, Access::ReadWrite);
    let hits = k.column("hits", Ty::Int, Access::ReadWrite);
    let dt = k.param("dt", Ty::Float);
    let limit = k.param("limit", Ty::Float);
    let ev = k.buffer("bounced", Ty::Float, 1000);
    let p = k.push(Instr::Load(pos));
    let v = k.push(Instr::Load(vel));
    let h = k.push(Instr::Load(hits));
    let d = k.push(Instr::Param(dt));
    let l = k.push(Instr::Param(limit));
    let step = k.push(Instr::Bin(BinOp::Mul, v, d));
    let np = k.push(Instr::Bin(BinOp::Add, p, step));
    let ap = k.push(Instr::Un(UnOp::Abs, np));
    let out = k.push(Instr::Cmp(CmpOp::Gt, ap, l));
    let nv = k.push(Instr::Un(UnOp::Neg, v));
    let v2 = k.push(Instr::Select(out, nv, v));
    let one = k.push(Instr::Const(Ty::Int, 1));
    let zero = k.push(Instr::Const(Ty::Int, 0));
    let inc = k.push(Instr::Select(out, one, zero));
    let row = k.push(Instr::Row);
    let three = k.push(Instr::Const(Ty::Int, 3));
    let m = k.push(Instr::Bin(BinOp::Mod, row, three));
    let dz = k.push(Instr::Bin(BinOp::Div, row, zero));
    let extra = k.push(Instr::Bin(BinOp::Add, m, dz));
    let h1 = k.push(Instr::Bin(BinOp::Add, h, inc));
    let h2 = k.push(Instr::Bin(BinOp::Add, h1, extra));
    k.push(Instr::Store(pos, np));
    k.push(Instr::Store(vel, v2));
    k.push(Instr::Store(hits, h2));
    k.push(Instr::Emit { buffer: ev, cond: out, value: np });
    k.check().unwrap()
}

fn data(rows: usize) -> Data {
    Data {
        columns: vec![
            (0..rows).map(|i| float((i as f64 * 0.37).sin() * 9.0)).collect(),
            (0..rows).map(|i| float(((i * 7919) % 200) as f64 / 10.0 - 10.0)).collect(),
            (0..rows).map(|i| int((i % 5) as i64)).collect(),
        ],
        params: vec![float(0.125), float(9.5)],
    }
}

fn gpu() -> Option<WgpuDevice> {
    match WgpuDevice::headless() {
        Ok(d) => {
            eprintln!("GPU: {}", d.adapter_name);
            Some(d)
        }
        Err(e) => {
            eprintln!("skipping GPU kernel parity: {e}");
            None
        }
    }
}

#[test]
fn without_a_gpu_kernels_run_on_the_cpu_exactly() {
    let pool = JobPool::new(2);
    let k = bounce(ResultClass::Bounded);
    let mut a = data(300);
    let mut b = data(300);
    let (events, backend) = run_kernel(None, &k, &mut a, &pool);
    assert_eq!(backend, Backend::Cpu("no GPU".into()));
    assert_eq!(events, run_scalar(&k, &mut b));
    assert_eq!(a.columns, b.columns);
}

#[test]
fn exact_kernels_never_go_to_the_gpu() {
    let pool = JobPool::new(2);
    let k = bounce(ResultClass::Exact);
    let mut a = data(50);
    let (_, backend) = run_kernel(gpu().as_ref(), &k, &mut a, &pool);
    match backend {
        Backend::Cpu(why) => assert!(why.contains("Exact") || why == "no GPU", "{why}"),
        Backend::Gpu(_) => panic!("an Exact kernel ran on the GPU"),
    }
}

#[test]
fn gpu_results_match_the_32_bit_reference() {
    let Some(g) = gpu() else { return };
    let k = bounce(ResultClass::Bounded);
    let rows = 1000;
    let mut on_gpu = data(rows);
    let gpu_events = run_kernel_gpu(&g, &k, &mut on_gpu).expect("GPU run");
    let mut narrow = Data32::from_data(&k, &data(rows));
    let mut ref_events = run_scalar32(&k, &mut narrow);
    sort_events(&mut ref_events);
    let mut reference = data(rows);
    narrow.write_back(&k, &mut reference);
    assert_eq!(on_gpu.columns[2], reference.columns[2], "int column is bit-identical");
    for c in [0, 1] {
        for (row, (g, r)) in on_gpu.columns[c].iter().zip(&reference.columns[c]).enumerate() {
            let (g, r) = (f64::from_bits(*g), f64::from_bits(*r));
            assert!((g - r).abs() <= 1e-5 * r.abs().max(1.0), "column {c} row {row}: {g} vs {r}");
        }
    }
    let rows_of = |e: &Events| e.buffers[0].iter().map(|(r, _)| *r).collect::<Vec<_>>();
    assert_eq!(rows_of(&gpu_events), rows_of(&ref_events));
    assert!(!gpu_events.buffers[0].is_empty(), "some rows bounced");
    let pool = JobPool::new(2);
    let (_, backend) = run_kernel(Some(&g), &k, &mut data(rows), &pool);
    assert!(matches!(backend, Backend::Gpu(_)), "{backend:?}");
}
