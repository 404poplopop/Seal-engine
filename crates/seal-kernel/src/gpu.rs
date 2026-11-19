//! The GPU form of a kernel: the same program lowered to a WGSL compute
//! shader. GPUs have no 64-bit floats in portable WGSL, so on the GPU every
//! value is 32 bits: `int` is i32 (wrapping), `float` is f32, `bool` is u32
//! 0 or 1. That changes results, so only `Bounded` and `Visual` kernels may
//! run there; `Exact` kernels always stay on the CPU (see [`gpu_eligible`]).
//!
//! [`run_scalar32`] is the CPU reference for the 32-bit semantics; a GPU run
//! must match it bit for bit for ints and bools, and within float rounding of
//! the shader compiler for floats. Event order on the GPU is unspecified;
//! [`sort_events`] puts them back into row order.
//!
//! Bindings (group 0): 0 `info: array<u32>` = [rows, params as 32-bit bits...];
//! then each column (read-only or read-write `array<u32>` of bits); then per
//! event buffer two bindings: `array<vec2<u32>>` of (row, value bits) and an
//! `array<atomic<u32>, 1>` counter.

use crate::{BinOp, Checked, CmpOp, Events, Instr, ResultClass, Ty, UnOp};

/// Threads per workgroup in the generated shader.
pub const WORKGROUP: u32 = 64;

/// Why a kernel stays on the CPU.
pub fn gpu_eligible(k: &Checked) -> Result<(), String> {
    if k.kernel.class == ResultClass::Exact {
        return Err(format!(
            "kernel {} is Exact: the GPU has no 64-bit values, so it runs on the CPU",
            k.kernel.name
        ));
    }
    Ok(())
}

/// 64-bit kernel bits to the 32-bit GPU form.
pub fn to_gpu_bits(ty: Ty, v: u64) -> u32 {
    match ty {
        Ty::Int => v as i64 as i32 as u32,
        Ty::Float => (f64::from_bits(v) as f32).to_bits(),
        Ty::Bool => (v != 0) as u32,
    }
}

/// 32-bit GPU bits back to 64-bit kernel bits.
pub fn from_gpu_bits(ty: Ty, v: u32) -> u64 {
    match ty {
        Ty::Int => v as i32 as i64 as u64,
        Ty::Float => f64::from(f32::from_bits(v)).to_bits(),
        Ty::Bool => u64::from(v != 0),
    }
}

fn wgsl_ty(t: Ty) -> &'static str {
    match t {
        Ty::Int => "i32",
        Ty::Float => "f32",
        Ty::Bool => "u32",
    }
}

fn from_bits_expr(t: Ty, bits: &str) -> String {
    match t {
        Ty::Int => format!("bitcast<i32>({bits})"),
        Ty::Float => format!("bitcast<f32>({bits})"),
        Ty::Bool => format!("({bits} & 1u)"),
    }
}

fn to_bits_expr(t: Ty, v: &str) -> String {
    match t {
        Ty::Bool => v.to_string(),
        _ => format!("bitcast<u32>({v})"),
    }
}

/// Binding number of column `c`.
pub fn column_binding(c: usize) -> u32 {
    1 + c as u32
}

/// Binding numbers (data, counter) of event buffer `b`.
pub fn buffer_bindings(k: &Checked, b: usize) -> (u32, u32) {
    let first = 1 + k.kernel.columns.len() as u32 + 2 * b as u32;
    (first, first + 1)
}

/// The kernel as a WGSL compute shader with entry point `main`.
pub fn lower_wgsl(k: &Checked) -> Result<String, String> {
    gpu_eligible(k)?;
    let kernel = &k.kernel;
    let mut s = String::new();
    s.push_str(&format!("// Seal kernel {} (32-bit GPU form)\n", kernel.name));
    s.push_str("@group(0) @binding(0) var<storage, read> info: array<u32>;\n");
    for (c, col) in kernel.columns.iter().enumerate() {
        let access = if col.access == crate::Access::Read { "read" } else { "read_write" };
        s.push_str(&format!(
            "@group(0) @binding({}) var<storage, {access}> c{c}: array<u32>;\n",
            column_binding(c)
        ));
    }
    for b in 0..kernel.buffers.len() {
        let (data, count) = buffer_bindings(k, b);
        s.push_str(&format!(
            "@group(0) @binding({data}) var<storage, read_write> e{b}: array<vec2<u32>>;\n\
             @group(0) @binding({count}) var<storage, read_write> n{b}: array<atomic<u32>, 1>;\n"
        ));
    }
    s.push_str(&format!(
        "@compute @workgroup_size({WORKGROUP})\nfn main(@builtin(global_invocation_id) gid: vec3<u32>) {{\n    let row = gid.x;\n    if (row >= info[0]) {{ return; }}\n"
    ));
    let ty = |r: u32| k.types[r as usize].expect("checked");
    for (i, instr) in kernel.body.iter().enumerate() {
        let line = match *instr {
            Instr::Load(c) => {
                let t = kernel.columns[c as usize].ty;
                format!("let r{i}: {} = {};", wgsl_ty(t), from_bits_expr(t, &format!("c{c}[row]")))
            }
            Instr::Param(p) => {
                let t = kernel.params[p as usize].1;
                format!(
                    "let r{i}: {} = {};",
                    wgsl_ty(t),
                    from_bits_expr(t, &format!("info[{}u]", p + 1))
                )
            }
            Instr::Const(t, bits) => {
                let v = to_gpu_bits(t, bits);
                let lit = match t {
                    Ty::Int => format!("bitcast<i32>({v}u)"),
                    Ty::Float => format!("bitcast<f32>({v}u)"),
                    Ty::Bool => format!("{v}u"),
                };
                format!("let r{i}: {} = {lit};", wgsl_ty(t))
            }
            Instr::Row => format!("let r{i}: i32 = i32(row);"),
            Instr::Bin(op, a, b) => {
                let t = ty(a);
                let (x, y) = (format!("r{a}"), format!("r{b}"));
                let e = match (op, t) {
                    (BinOp::Add, _) => format!("{x} + {y}"),
                    (BinOp::Sub, _) => format!("{x} - {y}"),
                    (BinOp::Mul, _) => format!("{x} * {y}"),
                    (BinOp::Div, Ty::Int) => {
                        format!("select({x} / select({y}, 1, {y} == 0), 0, {y} == 0)")
                    }
                    (BinOp::Mod, Ty::Int) => {
                        format!("select({x} % select({y}, 1, {y} == 0), 0, {y} == 0)")
                    }
                    (BinOp::Div, _) => format!("{x} / {y}"),
                    (BinOp::Min, _) => format!("min({x}, {y})"),
                    (BinOp::Max, _) => format!("max({x}, {y})"),
                    (BinOp::And, _) => format!("{x} & {y}"),
                    (BinOp::Or, _) => format!("{x} | {y}"),
                    (BinOp::Mod, _) => unreachable!("checked"),
                };
                format!("let r{i}: {} = {e};", wgsl_ty(t))
            }
            Instr::Un(op, a) => {
                let x = format!("r{a}");
                let (t, e) = match op {
                    UnOp::Neg => (ty(a), format!("-{x}")),
                    UnOp::Abs => (ty(a), format!("abs({x})")),
                    UnOp::Sqrt => (Ty::Float, format!("sqrt({x})")),
                    UnOp::Not => (Ty::Bool, format!("{x} ^ 1u")),
                    UnOp::ToFloat => (Ty::Float, format!("f32({x})")),
                    UnOp::ToInt => (Ty::Int, format!("i32({x})")),
                };
                format!("let r{i}: {} = {e};", wgsl_ty(t))
            }
            Instr::Cmp(op, a, b) => {
                let sym = match op {
                    CmpOp::Lt => "<",
                    CmpOp::Le => "<=",
                    CmpOp::Gt => ">",
                    CmpOp::Ge => ">=",
                    CmpOp::Eq => "==",
                    CmpOp::Ne => "!=",
                };
                format!("let r{i}: u32 = select(0u, 1u, r{a} {sym} r{b});")
            }
            Instr::Select(c, a, b) => {
                format!("let r{i}: {} = select(r{b}, r{a}, r{c} != 0u);", wgsl_ty(ty(a)))
            }
            Instr::Store(c, a) => {
                format!("c{c}[row] = {};", to_bits_expr(ty(a), &format!("r{a}")))
            }
            Instr::Emit { buffer, cond, value } => {
                let cap = kernel.buffers[buffer as usize].capacity;
                format!(
                    "if (r{cond} != 0u) {{ let at = atomicAdd(&n{buffer}[0], 1u); if (at < {cap}u) {{ e{buffer}[at] = vec2<u32>(row, {}); }} }}",
                    to_bits_expr(ty(value), &format!("r{value}"))
                )
            }
        };
        s.push_str("    ");
        s.push_str(&line);
        s.push('\n');
    }
    s.push_str("}\n");
    Ok(s)
}

/// Column and parameter bits for a GPU run, in the 32-bit form.
pub struct Data32 {
    pub columns: Vec<Vec<u32>>,
    pub params: Vec<u32>,
}

impl Data32 {
    pub fn from_data(k: &Checked, data: &crate::Data) -> Data32 {
        let kernel = &k.kernel;
        Data32 {
            columns: data
                .columns
                .iter()
                .zip(&kernel.columns)
                .map(|(col, c)| col.iter().map(|v| to_gpu_bits(c.ty, *v)).collect())
                .collect(),
            params: data
                .params
                .iter()
                .zip(&kernel.params)
                .map(|(v, p)| to_gpu_bits(p.1, *v))
                .collect(),
        }
    }

    /// Writes the 32-bit columns back into 64-bit kernel data.
    pub fn write_back(&self, k: &Checked, data: &mut crate::Data) {
        for ((dst, src), c) in data.columns.iter_mut().zip(&self.columns).zip(&k.kernel.columns) {
            for (d, s) in dst.iter_mut().zip(src) {
                *d = from_gpu_bits(c.ty, *s);
            }
        }
    }
}

/// Events in row order (the GPU appends in any order); values stay 32-bit.
pub fn sort_events(events: &mut Events) {
    for buf in &mut events.buffers {
        buf.sort_unstable();
    }
}

fn bin32(op: BinOp, ty: Ty, a: u32, b: u32) -> u32 {
    match ty {
        Ty::Int => {
            let (a, b) = (a as i32, b as i32);
            (match op {
                BinOp::Add => a.wrapping_add(b),
                BinOp::Sub => a.wrapping_sub(b),
                BinOp::Mul => a.wrapping_mul(b),
                BinOp::Div => {
                    if b == 0 {
                        0
                    } else {
                        a.wrapping_div(b)
                    }
                }
                BinOp::Mod => {
                    if b == 0 {
                        0
                    } else {
                        a.wrapping_rem(b)
                    }
                }
                BinOp::Min => a.min(b),
                BinOp::Max => a.max(b),
                BinOp::And | BinOp::Or => unreachable!("checked"),
            }) as u32
        }
        Ty::Float => {
            let (a, b) = (f32::from_bits(a), f32::from_bits(b));
            (match op {
                BinOp::Add => a + b,
                BinOp::Sub => a - b,
                BinOp::Mul => a * b,
                BinOp::Div => a / b,
                BinOp::Min => a.min(b),
                BinOp::Max => a.max(b),
                _ => unreachable!("checked"),
            })
            .to_bits()
        }
        Ty::Bool => match op {
            BinOp::And => a & b,
            BinOp::Or => a | b,
            _ => unreachable!("checked"),
        },
    }
}

/// The CPU reference for the GPU's 32-bit semantics (one row at a time).
/// Event values are 32-bit GPU bits.
pub fn run_scalar32(k: &Checked, data: &mut Data32) -> Events {
    let kernel = &k.kernel;
    let rows = data.columns.first().map_or(0, Vec::len);
    let mut events = Events {
        buffers: vec![Vec::new(); kernel.buffers.len()],
        overflow: vec![0; kernel.buffers.len()],
    };
    let mut regs = vec![0u32; kernel.body.len()];
    for row in 0..rows {
        for (i, instr) in kernel.body.iter().enumerate() {
            let ty = |r: u32| k.types[r as usize].unwrap();
            regs[i] = match *instr {
                Instr::Load(c) => data.columns[c as usize][row],
                Instr::Param(p) => data.params[p as usize],
                Instr::Const(t, bits) => to_gpu_bits(t, bits),
                Instr::Row => row as u32,
                Instr::Bin(op, a, b) => bin32(op, ty(a), regs[a as usize], regs[b as usize]),
                Instr::Un(op, a) => {
                    let x = regs[a as usize];
                    match (op, ty(a)) {
                        (UnOp::Neg, Ty::Int) => (x as i32).wrapping_neg() as u32,
                        (UnOp::Abs, Ty::Int) => (x as i32).wrapping_abs() as u32,
                        (UnOp::Neg, _) => (-f32::from_bits(x)).to_bits(),
                        (UnOp::Abs, _) => f32::from_bits(x).abs().to_bits(),
                        (UnOp::Sqrt, _) => f32::from_bits(x).sqrt().to_bits(),
                        (UnOp::Not, _) => x ^ 1,
                        (UnOp::ToFloat, _) => (x as i32 as f32).to_bits(),
                        (UnOp::ToInt, _) => f32::from_bits(x) as i32 as u32,
                    }
                }
                Instr::Cmp(op, a, b) => {
                    let (x, y) = (regs[a as usize], regs[b as usize]);
                    let r = match ty(a) {
                        Ty::Float => {
                            let (x, y) = (f32::from_bits(x), f32::from_bits(y));
                            match op {
                                CmpOp::Lt => x < y,
                                CmpOp::Le => x <= y,
                                CmpOp::Gt => x > y,
                                CmpOp::Ge => x >= y,
                                CmpOp::Eq => x == y,
                                CmpOp::Ne => x != y,
                            }
                        }
                        _ => {
                            let (x, y) = (x as i32, y as i32);
                            match op {
                                CmpOp::Lt => x < y,
                                CmpOp::Le => x <= y,
                                CmpOp::Gt => x > y,
                                CmpOp::Ge => x >= y,
                                CmpOp::Eq => x == y,
                                CmpOp::Ne => x != y,
                            }
                        }
                    };
                    r as u32
                }
                Instr::Select(c, a, b) => {
                    if regs[c as usize] != 0 {
                        regs[a as usize]
                    } else {
                        regs[b as usize]
                    }
                }
                Instr::Store(c, a) => {
                    data.columns[c as usize][row] = regs[a as usize];
                    0
                }
                Instr::Emit { buffer, cond, value } => {
                    if regs[cond as usize] != 0 {
                        let b = buffer as usize;
                        if events.buffers[b].len() < kernel.buffers[b].capacity {
                            events.buffers[b].push((row as u64, u64::from(regs[value as usize])));
                        } else {
                            events.overflow[b] += 1;
                        }
                    }
                    0
                }
            };
        }
    }
    events
}
