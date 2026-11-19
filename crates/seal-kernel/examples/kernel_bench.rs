//! Time the bounce kernel: reference runner, CPU backend on one thread,
//! and on every core. `cargo run --release -p seal-kernel --example kernel_bench [ROWS]`

use seal_jobs::JobPool;
use seal_kernel::*;
use std::time::Instant;

fn main() {
    let rows: usize = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(1 << 20);
    let mut k = Kernel::new("bounce", ResultClass::Exact);
    let pos = k.column("pos", Ty::Float, Access::ReadWrite);
    let vel = k.column("vel", Ty::Float, Access::ReadWrite);
    let dt = k.param("dt", Ty::Float);
    let limit = k.param("limit", Ty::Float);
    let p = k.push(Instr::Load(pos));
    let v = k.push(Instr::Load(vel));
    let d = k.push(Instr::Param(dt));
    let l = k.push(Instr::Param(limit));
    let s = k.push(Instr::Bin(BinOp::Mul, v, d));
    let np = k.push(Instr::Bin(BinOp::Add, p, s));
    let a = k.push(Instr::Un(UnOp::Abs, np));
    let out = k.push(Instr::Cmp(CmpOp::Gt, a, l));
    let nv = k.push(Instr::Un(UnOp::Neg, v));
    let v2 = k.push(Instr::Select(out, nv, v));
    k.push(Instr::Store(pos, np));
    k.push(Instr::Store(vel, v2));
    let k = k.check().unwrap();
    let data = || Data {
        columns: vec![
            (0..rows).map(|i| float((i as f64 * 0.37).sin() * 4.0)).collect(),
            (0..rows).map(|i| float((i as f64 * 1.3).cos() * 9.0)).collect(),
        ],
        params: vec![float(1.0 / 60.0), float(4.5)],
    };
    let frames = 20;
    let time = |label: &str, f: &mut dyn FnMut(&mut Data)| {
        let mut d = data();
        f(&mut d);
        let t = Instant::now();
        for _ in 0..frames {
            f(&mut d);
        }
        let ns = t.elapsed().as_nanos() as f64 / (frames * rows) as f64;
        println!("{label:<24} {ns:>7.2} ns per element");
        d.columns
    };
    let reference = time("reference (scalar)", &mut |d| {
        run_scalar(&k, d);
    });
    let one = JobPool::new(1);
    let a = time("cpu backend, 1 thread", &mut |d| {
        run_cpu(&k, d, &one, CpuOptions::default());
    });
    let all = JobPool::with_hardware_threads();
    let label = format!("cpu backend, {} threads", all.workers());
    let b = time(&label, &mut |d| {
        run_cpu(&k, d, &all, CpuOptions::default());
    });
    assert!(a == reference && b == reference, "results differ");
    println!("{rows} elements x {frames} frames, results identical");
}
