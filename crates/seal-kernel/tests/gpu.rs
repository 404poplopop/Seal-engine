use seal_kernel::gpu::*;
use seal_kernel::*;

/// Every op once: int div/mod (with zero), float math, bools, conversions, select, emit.
fn everything(class: ResultClass) -> Checked {
    let mut k = Kernel::new("everything", class);
    let a = k.column("a", Ty::Int, Access::Read);
    let b = k.column("b", Ty::Int, Access::Read);
    let x = k.column("x", Ty::Float, Access::ReadWrite);
    let q = k.column("q", Ty::Int, Access::Write);
    let flag = k.column("flag", Ty::Bool, Access::Write);
    let scale = k.param("scale", Ty::Float);
    let ev = k.buffer("big", Ty::Float, 64);
    let ra = k.push(Instr::Load(a));
    let rb = k.push(Instr::Load(b));
    let rx = k.push(Instr::Load(x));
    let s = k.push(Instr::Param(scale));
    let div = k.push(Instr::Bin(BinOp::Div, ra, rb));
    let rem = k.push(Instr::Bin(BinOp::Mod, ra, rb));
    let row = k.push(Instr::Row);
    let sum = k.push(Instr::Bin(BinOp::Add, div, rem));
    let mx = k.push(Instr::Bin(BinOp::Max, sum, row));
    let neg = k.push(Instr::Un(UnOp::Neg, mx));
    let ab = k.push(Instr::Un(UnOp::Abs, neg));
    let fx = k.push(Instr::Bin(BinOp::Mul, rx, s));
    let sq = k.push(Instr::Un(UnOp::Abs, fx));
    let root = k.push(Instr::Un(UnOp::Sqrt, sq));
    let fa = k.push(Instr::Un(UnOp::ToFloat, ab));
    let nx = k.push(Instr::Bin(BinOp::Add, root, fa));
    let back = k.push(Instr::Un(UnOp::ToInt, nx));
    let lim = k.push(Instr::Const(Ty::Float, float(5.0)));
    let big = k.push(Instr::Cmp(CmpOp::Gt, nx, lim));
    let even = {
        let two = k.push(Instr::Const(Ty::Int, 2));
        let r = k.push(Instr::Bin(BinOp::Mod, back, two));
        let zero = k.push(Instr::Const(Ty::Int, 0));
        k.push(Instr::Cmp(CmpOp::Eq, r, zero))
    };
    let both = k.push(Instr::Bin(BinOp::And, big, even));
    let either = k.push(Instr::Bin(BinOp::Or, big, even));
    let not = k.push(Instr::Un(UnOp::Not, either));
    let pick = k.push(Instr::Select(both, back, ab));
    k.push(Instr::Store(x, nx));
    k.push(Instr::Store(q, pick));
    k.push(Instr::Store(flag, not));
    k.push(Instr::Emit { buffer: ev, cond: big, value: nx });
    k.check().expect("valid")
}

fn data() -> Data {
    let a: Vec<i64> = vec![7, -7, 100, 0, 9, -8, 13, 1];
    let b: Vec<i64> = vec![2, 3, 0, 5, -4, -1, 13, 0];
    let x: Vec<f64> = vec![1.5, -2.25, 9.0, 0.0, 16.0, 0.5, 3.0, 100.0];
    Data {
        columns: vec![
            a.iter().map(|v| int(*v)).collect(),
            b.iter().map(|v| int(*v)).collect(),
            x.iter().map(|v| float(*v)).collect(),
            vec![0; 8],
            vec![0; 8],
        ],
        params: vec![float(2.0)],
    }
}

fn validate(wgsl: &str) {
    let module = naga::front::wgsl::parse_str(wgsl).unwrap_or_else(|e| panic!("{e}\n{wgsl}"));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("{e:?}\n{wgsl}"));
}

#[test]
fn exact_kernels_stay_on_the_cpu() {
    let k = everything(ResultClass::Exact);
    assert!(gpu_eligible(&k).is_err());
    assert!(lower_wgsl(&k).unwrap_err().contains("Exact"));
}

#[test]
fn the_lowered_shader_is_valid_wgsl_with_the_documented_bindings() {
    let k = everything(ResultClass::Bounded);
    let wgsl = lower_wgsl(&k).unwrap();
    validate(&wgsl);
    assert!(wgsl.contains("@group(0) @binding(0) var<storage, read> info"));
    assert!(wgsl.contains("@binding(1) var<storage, read> c0"));
    assert!(wgsl.contains("@binding(4) var<storage, read_write> c3"));
    assert_eq!(buffer_bindings(&k, 0), (6, 7));
    assert!(wgsl.contains("@binding(7) var<storage, read_write> n0: array<atomic<u32>, 1>"));
}

#[test]
fn the_32_bit_reference_matches_the_64_bit_runner_within_float_rounding() {
    let k = everything(ResultClass::Visual);
    let mut wide = data();
    let mut narrow = Data32::from_data(&k, &wide);
    let wide_events = run_scalar(&k, &mut wide);
    let mut narrow_events = run_scalar32(&k, &mut narrow);
    sort_events(&mut narrow_events);
    let mut back = data();
    narrow.write_back(&k, &mut back);
    // Ints and bools: identical (inputs are inside the i32 range).
    assert_eq!(back.columns[3], wide.columns[3], "q");
    assert_eq!(back.columns[4], wide.columns[4], "flag");
    // Floats: same within f32 rounding.
    for (g, c) in back.columns[2].iter().zip(&wide.columns[2]) {
        let (g, c) = (f64::from_bits(*g), f64::from_bits(*c));
        assert!((g - c).abs() <= 1e-5 * c.abs().max(1.0), "{g} vs {c}");
    }
    let rows = |e: &Events| e.buffers[0].iter().map(|(r, _)| *r).collect::<Vec<_>>();
    assert_eq!(rows(&narrow_events), rows(&wide_events));
}

#[test]
fn bits_round_trip_between_the_cpu_and_gpu_forms() {
    for v in [0i64, 1, -1, i64::from(i32::MAX), i64::from(i32::MIN)] {
        assert_eq!(from_gpu_bits(Ty::Int, to_gpu_bits(Ty::Int, int(v))), int(v));
    }
    for v in [0.0f64, -1.5, 3.25, 1e10] {
        assert_eq!(f64::from_bits(from_gpu_bits(Ty::Float, to_gpu_bits(Ty::Float, float(v)))), v);
    }
    assert_eq!(from_gpu_bits(Ty::Bool, to_gpu_bits(Ty::Bool, boolean(true))), 1);
    // Ints wrap at 32 bits on the GPU, so results outside the i32 range differ
    // from the CPU (why Exact kernels never go there).
    assert_eq!(from_gpu_bits(Ty::Int, to_gpu_bits(Ty::Int, int(1 << 32))), 0);
}
