//! Seal kernels: logic that runs once per element over columns of data.
//!
//! A kernel is a straight-line program in SSA form: no loops, no calls, no
//! allocation, no recursion. That restriction is what lets the same kernel
//! run on any backend. This crate has the IR, its checker, a scalar
//! reference runner, and the CPU backend, which runs the program over blocks
//! of elements (each instruction is a tight loop the compiler vectorizes)
//! and splits the rows into jobs.
//!
//! Every value is 64 bits: `int` (i64, wrapping), `float` (f64, IEEE, no
//! fused multiply-add) or `bool`. Arithmetic matches visual graphs exactly,
//! so a graph lowered to a kernel gives the same bits.

mod cpu;
pub mod gpu;
pub use cpu::{run_cpu, CpuOptions};

use std::fmt;

/// A value type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    Int,
    Float,
    Bool,
}

impl Ty {
    pub fn name(self) -> &'static str {
        match self {
            Ty::Int => "int",
            Ty::Float => "float",
            Ty::Bool => "bool",
        }
    }
}

/// How identical results must be across machines and backends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultClass {
    /// Bit-identical everywhere. May feed authoritative, replayed state.
    Exact,
    /// The same within a stated tolerance.
    Bounded,
    /// Only needs to look right; must not feed authoritative state.
    Visual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Min,
    Max,
    And,
    Or,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Abs,
    Sqrt,
    Not,
    ToFloat,
    ToInt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

/// A register: the index of the instruction that defined it.
pub type Reg = u32;

#[derive(Clone, Debug, PartialEq)]
pub enum Instr {
    /// Read this element's value from a column.
    Load(u32),
    /// Read a parameter shared by all elements.
    Param(u32),
    /// A constant, as raw bits.
    Const(Ty, u64),
    /// This element's row number, as an int.
    Row,
    Bin(BinOp, Reg, Reg),
    Un(UnOp, Reg),
    Cmp(CmpOp, Reg, Reg),
    /// `cond ? a : b`
    Select(Reg, Reg, Reg),
    /// Write this element's value to a column.
    Store(u32, Reg),
    /// When `cond` holds, append `(row, value)` to an event buffer.
    Emit {
        buffer: u32,
        cond: Reg,
        value: Reg,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
    ReadWrite,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    pub name: String,
    pub ty: Ty,
    pub access: Access,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EventBuffer {
    pub name: String,
    pub ty: Ty,
    /// Appends past this are dropped and counted.
    pub capacity: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Kernel {
    pub name: String,
    pub class: ResultClass,
    pub columns: Vec<Column>,
    pub params: Vec<(String, Ty)>,
    pub buffers: Vec<EventBuffer>,
    pub body: Vec<Instr>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KernelError {
    pub instr: Option<usize>,
    pub message: String,
}

impl fmt::Display for KernelError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.instr {
            Some(i) => write!(f, "instruction {i}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for KernelError {}

fn err(instr: usize, message: impl Into<String>) -> KernelError {
    KernelError { instr: Some(instr), message: message.into() }
}

/// A kernel that passed the checker, with each register's type.
#[derive(Clone, Debug)]
pub struct Checked {
    pub kernel: Kernel,
    pub types: Vec<Option<Ty>>,
}

impl Kernel {
    pub fn new(name: &str, class: ResultClass) -> Self {
        Self {
            name: name.into(),
            class,
            columns: Vec::new(),
            params: Vec::new(),
            buffers: Vec::new(),
            body: Vec::new(),
        }
    }

    pub fn column(&mut self, name: &str, ty: Ty, access: Access) -> u32 {
        self.columns.push(Column { name: name.into(), ty, access });
        self.columns.len() as u32 - 1
    }

    pub fn param(&mut self, name: &str, ty: Ty) -> u32 {
        self.params.push((name.into(), ty));
        self.params.len() as u32 - 1
    }

    pub fn buffer(&mut self, name: &str, ty: Ty, capacity: usize) -> u32 {
        self.buffers.push(EventBuffer { name: name.into(), ty, capacity });
        self.buffers.len() as u32 - 1
    }

    /// Append an instruction and return its register.
    pub fn push(&mut self, instr: Instr) -> Reg {
        self.body.push(instr);
        self.body.len() as Reg - 1
    }

    /// Check types, register order and column access.
    pub fn check(self) -> Result<Checked, KernelError> {
        let mut types: Vec<Option<Ty>> = Vec::with_capacity(self.body.len());
        let mut stored = vec![false; self.columns.len()];
        for (i, instr) in self.body.iter().enumerate() {
            let reg = |r: Reg| -> Result<Ty, KernelError> {
                match types.get(r as usize) {
                    Some(Some(t)) => Ok(*t),
                    Some(None) => Err(err(i, format!("register {r} holds no value"))),
                    None => Err(err(i, format!("register {r} is used before it is defined"))),
                }
            };
            let ty = match instr {
                Instr::Load(c) => {
                    let col =
                        self.columns.get(*c as usize).ok_or(err(i, format!("no column {c}")))?;
                    if col.access == Access::Write {
                        return Err(err(i, format!("column {} is write-only", col.name)));
                    }
                    if stored[*c as usize] {
                        return Err(err(
                            i,
                            format!("column {} is read after it was written", col.name),
                        ));
                    }
                    Some(col.ty)
                }
                Instr::Param(p) => {
                    Some(self.params.get(*p as usize).ok_or(err(i, format!("no parameter {p}")))?.1)
                }
                Instr::Const(t, bits) => {
                    if *t == Ty::Bool && *bits > 1 {
                        return Err(err(i, "a bool constant must be 0 or 1"));
                    }
                    Some(*t)
                }
                Instr::Row => Some(Ty::Int),
                Instr::Bin(op, a, b) => {
                    let (ta, tb) = (reg(*a)?, reg(*b)?);
                    if ta != tb {
                        return Err(err(i, format!("{op:?} of {} and {}", ta.name(), tb.name())));
                    }
                    let ok = match op {
                        BinOp::And | BinOp::Or => ta == Ty::Bool,
                        BinOp::Mod => ta == Ty::Int,
                        _ => ta != Ty::Bool,
                    };
                    if !ok {
                        return Err(err(i, format!("{op:?} does not take {}", ta.name())));
                    }
                    Some(ta)
                }
                Instr::Un(op, a) => {
                    let t = reg(*a)?;
                    let out = match (op, t) {
                        (UnOp::Neg | UnOp::Abs, Ty::Int | Ty::Float) => t,
                        (UnOp::Sqrt, Ty::Float) => t,
                        (UnOp::Not, Ty::Bool) => t,
                        (UnOp::ToFloat, Ty::Int) => Ty::Float,
                        (UnOp::ToInt, Ty::Float) => Ty::Int,
                        _ => return Err(err(i, format!("{op:?} does not take {}", t.name()))),
                    };
                    Some(out)
                }
                Instr::Cmp(op, a, b) => {
                    let (ta, tb) = (reg(*a)?, reg(*b)?);
                    if ta != tb {
                        return Err(err(i, format!("compare of {} and {}", ta.name(), tb.name())));
                    }
                    if ta == Ty::Bool && !matches!(op, CmpOp::Eq | CmpOp::Ne) {
                        return Err(err(i, "bools only compare for equality"));
                    }
                    Some(Ty::Bool)
                }
                Instr::Select(c, a, b) => {
                    if reg(*c)? != Ty::Bool {
                        return Err(err(i, "select needs a bool condition"));
                    }
                    let (ta, tb) = (reg(*a)?, reg(*b)?);
                    if ta != tb {
                        return Err(err(i, format!("select of {} and {}", ta.name(), tb.name())));
                    }
                    Some(ta)
                }
                Instr::Store(c, a) => {
                    let col =
                        self.columns.get(*c as usize).ok_or(err(i, format!("no column {c}")))?;
                    if col.access == Access::Read {
                        return Err(err(i, format!("column {} is read-only", col.name)));
                    }
                    if reg(*a)? != col.ty {
                        return Err(err(
                            i,
                            format!(
                                "storing {} into {} column {}",
                                reg(*a)?.name(),
                                col.ty.name(),
                                col.name
                            ),
                        ));
                    }
                    if std::mem::replace(&mut stored[*c as usize], true) {
                        return Err(err(i, format!("column {} is written twice", col.name)));
                    }
                    None
                }
                Instr::Emit { buffer, cond, value } => {
                    let buf = self
                        .buffers
                        .get(*buffer as usize)
                        .ok_or(err(i, format!("no buffer {buffer}")))?;
                    if reg(*cond)? != Ty::Bool {
                        return Err(err(i, "emit needs a bool condition"));
                    }
                    if reg(*value)? != buf.ty {
                        return Err(err(
                            i,
                            format!(
                                "emitting {} into {} buffer {}",
                                reg(*value)?.name(),
                                buf.ty.name(),
                                buf.name
                            ),
                        ));
                    }
                    None
                }
            };
            types.push(ty);
        }
        for (c, col) in self.columns.iter().enumerate() {
            if col.access == Access::Write && !stored[c] {
                return Err(KernelError {
                    instr: None,
                    message: format!("write-only column {} is never written", col.name),
                });
            }
        }
        Ok(Checked { kernel: self, types })
    }
}

/// Raw bits for a value of each type.
pub fn int(v: i64) -> u64 {
    v as u64
}
pub fn float(v: f64) -> u64 {
    v.to_bits()
}
pub fn boolean(v: bool) -> u64 {
    v as u64
}

/// Column data for a run: one `Vec<u64>` per kernel column, all the same
/// length, in raw bits.
pub struct Data {
    pub columns: Vec<Vec<u64>>,
    pub params: Vec<u64>,
}

/// What a run produced besides column writes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Events {
    /// Per buffer: `(row, value bits)` in row order.
    pub buffers: Vec<Vec<(u64, u64)>>,
    /// Per buffer: appends dropped because the buffer was full.
    pub overflow: Vec<u64>,
}

/// The per-element operations. Visual graphs use the same definitions.
pub mod ops {
    use super::{BinOp, CmpOp, Ty, UnOp};

    #[inline(always)]
    pub fn bin(op: BinOp, ty: Ty, a: u64, b: u64) -> u64 {
        match ty {
            Ty::Int => {
                let (a, b) = (a as i64, b as i64);
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
                }) as u64
            }
            Ty::Float => {
                let (a, b) = (f64::from_bits(a), f64::from_bits(b));
                (match op {
                    BinOp::Add => a + b,
                    BinOp::Sub => a - b,
                    BinOp::Mul => a * b,
                    BinOp::Div => a / b,
                    BinOp::Min => a.min(b),
                    BinOp::Max => a.max(b),
                    BinOp::Mod | BinOp::And | BinOp::Or => unreachable!("checked"),
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

    #[inline(always)]
    pub fn un(op: UnOp, ty: Ty, a: u64) -> u64 {
        match (op, ty) {
            (UnOp::Neg, Ty::Int) => (a as i64).wrapping_neg() as u64,
            (UnOp::Abs, Ty::Int) => (a as i64).wrapping_abs() as u64,
            (UnOp::Neg, Ty::Float) => (-f64::from_bits(a)).to_bits(),
            (UnOp::Abs, Ty::Float) => f64::from_bits(a).abs().to_bits(),
            (UnOp::Sqrt, _) => f64::from_bits(a).sqrt().to_bits(),
            (UnOp::Not, _) => a ^ 1,
            (UnOp::ToFloat, _) => (a as i64 as f64).to_bits(),
            (UnOp::ToInt, _) => f64::from_bits(a) as i64 as u64,
            _ => unreachable!("checked"),
        }
    }

    #[inline(always)]
    pub fn cmp(op: CmpOp, ty: Ty, a: u64, b: u64) -> u64 {
        let r = match ty {
            Ty::Int | Ty::Bool => {
                let (a, b) = (a as i64, b as i64);
                match op {
                    CmpOp::Lt => a < b,
                    CmpOp::Le => a <= b,
                    CmpOp::Gt => a > b,
                    CmpOp::Ge => a >= b,
                    CmpOp::Eq => a == b,
                    CmpOp::Ne => a != b,
                }
            }
            Ty::Float => {
                let (a, b) = (f64::from_bits(a), f64::from_bits(b));
                match op {
                    CmpOp::Lt => a < b,
                    CmpOp::Le => a <= b,
                    CmpOp::Gt => a > b,
                    CmpOp::Ge => a >= b,
                    CmpOp::Eq => a == b,
                    CmpOp::Ne => a != b,
                }
            }
        };
        r as u64
    }
}

/// The reference runner: one element at a time, no blocks, no jobs. The CPU
/// backend must match it bit for bit.
pub fn run_scalar(k: &Checked, data: &mut Data) -> Events {
    let kernel = &k.kernel;
    let rows = data.columns.first().map_or(0, Vec::len);
    let mut events = Events {
        buffers: vec![Vec::new(); kernel.buffers.len()],
        overflow: vec![0; kernel.buffers.len()],
    };
    let mut regs = vec![0u64; kernel.body.len()];
    for row in 0..rows {
        for (i, instr) in kernel.body.iter().enumerate() {
            let ty = |r: Reg| k.types[r as usize].unwrap();
            regs[i] = match *instr {
                Instr::Load(c) => data.columns[c as usize][row],
                Instr::Param(p) => data.params[p as usize],
                Instr::Const(_, bits) => bits,
                Instr::Row => row as u64,
                Instr::Bin(op, a, b) => ops::bin(op, ty(a), regs[a as usize], regs[b as usize]),
                Instr::Un(op, a) => ops::un(op, ty(a), regs[a as usize]),
                Instr::Cmp(op, a, b) => ops::cmp(op, ty(a), regs[a as usize], regs[b as usize]),
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
                            events.buffers[b].push((row as u64, regs[value as usize]));
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
