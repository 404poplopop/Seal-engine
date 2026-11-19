use proptest::prelude::*;
use seal_jobs::JobPool;
use seal_kernel::*;

/// pos += vel * dt; bounce off |x| > limit and report each bounce.
fn bounce() -> Checked {
    let mut k = Kernel::new("bounce", ResultClass::Exact);
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
    let h2 = k.push(Instr::Bin(BinOp::Add, h, inc));
    k.push(Instr::Store(pos, np));
    k.push(Instr::Store(vel, v2));
    k.push(Instr::Store(hits, h2));
    k.push(Instr::Emit { buffer: ev, cond: out, value: np });
    k.check().unwrap()
}

fn bounce_data(n: usize) -> Data {
    Data {
        columns: vec![
            (0..n).map(|i| float((i as f64 * 0.37).sin() * 4.0)).collect(),
            (0..n).map(|i| float((i as f64 * 1.3).cos() * 9.0)).collect(),
            vec![int(0); n],
        ],
        params: vec![float(1.0 / 60.0), float(4.5)],
    }
}

#[test]
fn cpu_backend_matches_the_reference_for_any_threads_and_job_size() {
    let k = bounce();
    let mut reference = bounce_data(10_007);
    let mut ref_events = Events::default();
    for _ in 0..30 {
        ref_events = run_scalar(&k, &mut reference);
    }
    for threads in [1, 3, 8] {
        let pool = JobPool::new(threads);
        for rows_per_job in [1, 300, 4096, 1 << 20] {
            let mut d = bounce_data(10_007);
            let mut ev = Events::default();
            for _ in 0..30 {
                ev = run_cpu(&k, &mut d, &pool, CpuOptions { rows_per_job });
            }
            assert_eq!(d.columns, reference.columns, "threads {threads} job {rows_per_job}");
            assert_eq!(ev, ref_events);
        }
    }
    assert!(!ref_events.buffers[0].is_empty());
    let hits: i64 = reference.columns[2].iter().map(|h| *h as i64).sum();
    assert!(hits > 100);
}

#[test]
fn full_event_buffers_keep_the_first_rows_and_count_the_rest() {
    let mut k = Kernel::new("evens", ResultClass::Exact);
    let b = k.buffer("evens", Ty::Int, 10);
    let row = k.push(Instr::Row);
    let two = k.push(Instr::Const(Ty::Int, 2));
    let m = k.push(Instr::Bin(BinOp::Mod, row, two));
    let z = k.push(Instr::Const(Ty::Int, 0));
    let even = k.push(Instr::Cmp(CmpOp::Eq, m, z));
    k.push(Instr::Emit { buffer: b, cond: even, value: row });
    // The row count comes from the columns, so give it one.
    k.column("dummy", Ty::Int, Access::Read);
    let k = k.check().unwrap();
    let data = || Data { columns: vec![vec![0; 1000]], params: vec![] };
    let reference = run_scalar(&k, &mut data());
    let pool = JobPool::new(4);
    let ev = run_cpu(&k, &mut data(), &pool, CpuOptions { rows_per_job: 7 });
    assert_eq!(ev, reference);
    assert_eq!(
        ev.buffers[0].iter().map(|e| e.0).collect::<Vec<_>>(),
        (0..20).step_by(2).collect::<Vec<u64>>()
    );
    assert_eq!(ev.overflow[0], 490);
}

#[test]
fn checker_rejects_bad_kernels() {
    let check = |f: &dyn Fn(&mut Kernel)| {
        let mut k = Kernel::new("k", ResultClass::Exact);
        k.column("r", Ty::Float, Access::Read);
        k.column("w", Ty::Int, Access::Write);
        f(&mut k);
        k.check().unwrap_err().message
    };
    assert!(check(&|k| {
        let a = k.push(Instr::Load(0));
        let b = k.push(Instr::Const(Ty::Int, 1));
        k.push(Instr::Bin(BinOp::Add, a, b));
    })
    .contains("Add of float and int"));
    assert!(check(&|k| {
        let a = k.push(Instr::Load(0));
        k.push(Instr::Store(0, a));
    })
    .contains("read-only"));
    assert!(check(&|k| {
        k.push(Instr::Load(1));
    })
    .contains("write-only"));
    assert!(check(&|_| {}).contains("never written"));
    assert!(check(&|k| {
        let c = k.push(Instr::Const(Ty::Int, 3));
        k.push(Instr::Store(1, c));
        k.push(Instr::Store(1, c));
    })
    .contains("written twice"));
    assert!(check(&|k| {
        k.push(Instr::Un(UnOp::Neg, 5));
    })
    .contains("before it is defined"));
    assert!(check(&|k| {
        let a = k.push(Instr::Load(0));
        k.push(Instr::Bin(BinOp::Mod, a, a));
    })
    .contains("Mod does not take float"));
}

/// A random well-typed kernel over two int and two float columns.
fn random_kernel(seed: u64, len: usize) -> Checked {
    let mut s = seed | 1;
    let mut next = move |m: u64| {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s % m
    };
    let mut k = Kernel::new("random", ResultClass::Exact);
    let cols = [
        k.column("i0", Ty::Int, Access::ReadWrite),
        k.column("i1", Ty::Int, Access::Read),
        k.column("f0", Ty::Float, Access::ReadWrite),
        k.column("f1", Ty::Float, Access::Read),
    ];
    let buf = k.buffer("e", Ty::Int, 50);
    let mut regs: Vec<(Reg, Ty)> = Vec::new();
    for (c, t) in cols.iter().zip([Ty::Int, Ty::Int, Ty::Float, Ty::Float]) {
        regs.push((k.push(Instr::Load(*c)), t));
    }
    regs.push((k.push(Instr::Const(Ty::Float, 0.5f64.to_bits())), Ty::Float));
    regs.push((k.push(Instr::Const(Ty::Int, 0)), Ty::Int));
    regs.push((k.push(Instr::Row), Ty::Int));
    let pick = |regs: &Vec<(Reg, Ty)>, ty: Ty, r: u64| -> Reg {
        let of: Vec<Reg> = regs.iter().filter(|x| x.1 == ty).map(|x| x.0).collect();
        of[(r % of.len() as u64) as usize]
    };
    regs.push((k.push(Instr::Cmp(CmpOp::Lt, 0, 1)), Ty::Bool));
    for _ in 0..len {
        let ty = [Ty::Int, Ty::Float, Ty::Bool][next(3) as usize];
        let (a, b, c) = (next(1000), next(1000), next(1000));
        let instr = match (ty, next(6)) {
            (Ty::Bool, 0) => Instr::Bin(
                if a % 2 == 0 { BinOp::And } else { BinOp::Or },
                pick(&regs, ty, a),
                pick(&regs, ty, b),
            ),
            (Ty::Bool, 1) => Instr::Un(UnOp::Not, pick(&regs, ty, a)),
            (Ty::Bool, _) => {
                let t = if c % 2 == 0 { Ty::Int } else { Ty::Float };
                let op = [CmpOp::Lt, CmpOp::Le, CmpOp::Gt, CmpOp::Ge, CmpOp::Eq, CmpOp::Ne]
                    [(a % 6) as usize];
                Instr::Cmp(op, pick(&regs, t, b), pick(&regs, t, c))
            }
            (t, 0) => Instr::Select(pick(&regs, Ty::Bool, a), pick(&regs, t, b), pick(&regs, t, c)),
            (Ty::Int, 1) => Instr::Un(
                if a % 3 == 0 {
                    UnOp::ToInt
                } else if a % 3 == 1 {
                    UnOp::Neg
                } else {
                    UnOp::Abs
                },
                if a % 3 == 0 { pick(&regs, Ty::Float, b) } else { pick(&regs, Ty::Int, b) },
            ),
            (Ty::Float, 1) => Instr::Un(
                [UnOp::ToFloat, UnOp::Neg, UnOp::Abs, UnOp::Sqrt][(a % 4) as usize],
                if a % 4 == 0 { pick(&regs, Ty::Int, b) } else { pick(&regs, Ty::Float, b) },
            ),
            (t, _) => {
                let ops: &[BinOp] = if t == Ty::Int {
                    &[
                        BinOp::Add,
                        BinOp::Sub,
                        BinOp::Mul,
                        BinOp::Div,
                        BinOp::Mod,
                        BinOp::Min,
                        BinOp::Max,
                    ]
                } else {
                    &[BinOp::Add, BinOp::Sub, BinOp::Mul, BinOp::Div, BinOp::Min, BinOp::Max]
                };
                Instr::Bin(
                    ops[(a % ops.len() as u64) as usize],
                    pick(&regs, t, b),
                    pick(&regs, t, c),
                )
            }
        };
        regs.push((k.push(instr), ty));
    }
    let (i, f, bl) = (
        pick(&regs, Ty::Int, next(1000)),
        pick(&regs, Ty::Float, next(1000)),
        pick(&regs, Ty::Bool, next(1000)),
    );
    k.push(Instr::Store(0, i));
    k.push(Instr::Store(2, f));
    k.push(Instr::Emit { buffer: buf, cond: bl, value: i });
    k.check().unwrap()
}

fn random_data(seed: u64, rows: usize) -> Data {
    let specials = [0.0, -0.0, f64::NAN, f64::INFINITY, -1.5, 1e300, 3.25];
    Data {
        columns: vec![
            (0..rows).map(|r| int(((r as i64) * 7919 + seed as i64) % 23 - 11)).collect(),
            (0..rows).map(|r| int(if r % 13 == 0 { i64::MIN } else { r as i64 - 40 })).collect(),
            (0..rows).map(|r| float(specials[(r + seed as usize) % specials.len()])).collect(),
            (0..rows).map(|r| float(r as f64 * 0.1 - 3.0)).collect(),
        ],
        params: vec![],
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]
    #[test]
    fn random_kernels_agree(seed in any::<u64>(), len in 1usize..60, rows in 0usize..1500, job in 1usize..2000) {
        let k = random_kernel(seed, len);
        let mut a = random_data(seed, rows);
        let mut b = random_data(seed, rows);
        let ea = run_scalar(&k, &mut a);
        let eb = run_cpu(&k, &mut b, &JobPool::new(3), CpuOptions { rows_per_job: job });
        prop_assert_eq!(a.columns, b.columns);
        prop_assert_eq!(ea, eb);
    }
}
