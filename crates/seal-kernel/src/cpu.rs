//! The CPU backend. Rows are split into jobs; inside a job the program runs
//! over blocks of `BLOCK` elements, one instruction at a time, so each
//! instruction is a branch-free loop over a register's lanes that the
//! compiler turns into SIMD. Results are merged in row order, so they never
//! depend on the thread count or timing.

use crate::{ops, BinOp, Checked, CmpOp, Data, Events, Instr, Ty, UnOp};
use seal_jobs::JobPool;

/// Elements per block. Registers for one block stay in L1.
pub const BLOCK: usize = 256;

#[derive(Clone, Copy, Debug)]
pub struct CpuOptions {
    /// Rows per job. Rounded up to whole blocks.
    pub rows_per_job: usize,
}

impl Default for CpuOptions {
    fn default() -> Self {
        Self { rows_per_job: 16 * BLOCK }
    }
}

struct ChunkOut {
    events: Vec<Vec<(u64, u64)>>,
    overflow: Vec<u64>,
}

macro_rules! lanes2 {
    ($dst:expr, $a:expr, $b:expr, |$x:ident, $y:ident| $body:expr) => {
        for ((d, $x), $y) in $dst.iter_mut().zip($a.iter().copied()).zip($b.iter().copied()) {
            *d = $body;
        }
    };
}

macro_rules! lanes1 {
    ($dst:expr, $a:expr, |$x:ident| $body:expr) => {
        for (d, $x) in $dst.iter_mut().zip($a.iter().copied()) {
            *d = $body;
        }
    };
}

fn bin_block(op: BinOp, ty: Ty, d: &mut [u64], a: &[u64], b: &[u64]) {
    use BinOp::*;
    match ty {
        Ty::Int => match op {
            Add => lanes2!(d, a, b, |x, y| (x as i64).wrapping_add(y as i64) as u64),
            Sub => lanes2!(d, a, b, |x, y| (x as i64).wrapping_sub(y as i64) as u64),
            Mul => lanes2!(d, a, b, |x, y| (x as i64).wrapping_mul(y as i64) as u64),
            Min => lanes2!(d, a, b, |x, y| (x as i64).min(y as i64) as u64),
            Max => lanes2!(d, a, b, |x, y| (x as i64).max(y as i64) as u64),
            _ => lanes2!(d, a, b, |x, y| ops::bin(op, ty, x, y)),
        },
        Ty::Float => {
            let f = f64::from_bits;
            match op {
                Add => lanes2!(d, a, b, |x, y| (f(x) + f(y)).to_bits()),
                Sub => lanes2!(d, a, b, |x, y| (f(x) - f(y)).to_bits()),
                Mul => lanes2!(d, a, b, |x, y| (f(x) * f(y)).to_bits()),
                Div => lanes2!(d, a, b, |x, y| (f(x) / f(y)).to_bits()),
                _ => lanes2!(d, a, b, |x, y| ops::bin(op, ty, x, y)),
            }
        }
        Ty::Bool => match op {
            And => lanes2!(d, a, b, |x, y| x & y),
            _ => lanes2!(d, a, b, |x, y| x | y),
        },
    }
}

fn un_block(op: UnOp, ty: Ty, d: &mut [u64], a: &[u64]) {
    match (op, ty) {
        (UnOp::Neg, Ty::Float) => lanes1!(d, a, |x| x ^ (1 << 63)),
        (UnOp::Abs, Ty::Float) => lanes1!(d, a, |x| x & !(1 << 63)),
        (UnOp::Not, _) => lanes1!(d, a, |x| x ^ 1),
        _ => lanes1!(d, a, |x| ops::un(op, ty, x)),
    }
}

fn cmp_block(op: CmpOp, ty: Ty, d: &mut [u64], a: &[u64], b: &[u64]) {
    if ty == Ty::Float {
        let f = f64::from_bits;
        match op {
            CmpOp::Lt => lanes2!(d, a, b, |x, y| (f(x) < f(y)) as u64),
            CmpOp::Gt => lanes2!(d, a, b, |x, y| (f(x) > f(y)) as u64),
            _ => lanes2!(d, a, b, |x, y| ops::cmp(op, ty, x, y)),
        }
    } else {
        match op {
            CmpOp::Lt => lanes2!(d, a, b, |x, y| ((x as i64) < (y as i64)) as u64),
            CmpOp::Eq => lanes2!(d, a, b, |x, y| (x == y) as u64),
            _ => lanes2!(d, a, b, |x, y| ops::cmp(op, ty, x, y)),
        }
    }
}

/// Run a checked kernel over `data` on `pool`. Bit-identical to
/// [`crate::run_scalar`] for every thread count and job size.
///
/// Columns the kernel stores to are split into disjoint row ranges and
/// written in place by each job; a job only ever reads its own rows of
/// them, and only before it stores (the checker guarantees that order).
pub fn run_cpu(k: &Checked, data: &mut Data, pool: &JobPool, options: CpuOptions) -> Events {
    let kernel = &k.kernel;
    let _zone = seal_trace::zone!(seal_trace::Channel::CPU, "kernel");
    let rows = data.columns.first().map_or(0, Vec::len);
    // Straight-line code runs every store for every row, so a stored
    // column is fully overwritten and is the only kind that needs writing.
    let mut slot_of = vec![usize::MAX; kernel.columns.len()];
    let mut stored = Vec::new();
    for instr in &kernel.body {
        if let Instr::Store(c, _) = instr {
            slot_of[*c as usize] = stored.len();
            stored.push(*c as usize);
        }
    }
    let per_job = options.rows_per_job.max(1).div_ceil(BLOCK) * BLOCK;
    let mut taken: Vec<Vec<u64>> =
        stored.iter().map(|c| std::mem::take(&mut data.columns[*c])).collect();
    let jobs = rows.div_ceil(per_job);
    let mut parts: Vec<Vec<&mut [u64]>> = (0..jobs).map(|_| Vec::new()).collect();
    for col in taken.iter_mut() {
        for (j, part) in col.chunks_mut(per_job).enumerate() {
            parts[j].push(part);
        }
    }
    let mut results: Vec<Option<ChunkOut>> = (0..jobs).map(|_| None).collect();
    let input = &*data;
    let slot_of = &slot_of;
    pool.scope(|scope| {
        for (j, (mut own, slot)) in parts.into_iter().zip(results.iter_mut()).enumerate() {
            scope.spawn(seal_jobs::Priority::Normal, move || {
                let range = j * per_job..((j + 1) * per_job).min(rows);
                *slot = Some(run_chunk(k, input, &mut own, slot_of, range));
            });
        }
    });
    for (c, col) in stored.iter().zip(taken) {
        data.columns[*c] = col;
    }
    let mut out = Events {
        buffers: vec![Vec::new(); kernel.buffers.len()],
        overflow: vec![0; kernel.buffers.len()],
    };
    for chunk in results.into_iter().map(|r| r.expect("every job ran")) {
        for (b, list) in chunk.events.into_iter().enumerate() {
            let room = kernel.buffers[b].capacity - out.buffers[b].len();
            let keep = list.len().min(room);
            out.overflow[b] += (list.len() - keep) as u64 + chunk.overflow[b];
            out.buffers[b].extend_from_slice(&list[..keep]);
        }
    }
    out
}

fn run_chunk(
    k: &Checked,
    input: &Data,
    own: &mut [&mut [u64]],
    slot_of: &[usize],
    range: std::ops::Range<usize>,
) -> ChunkOut {
    let kernel = &k.kernel;
    let len = range.len();
    let mut events = vec![Vec::new(); kernel.buffers.len()];
    let mut overflow = vec![0u64; kernel.buffers.len()];
    let mut regs = vec![0u64; kernel.body.len() * BLOCK];
    let mut start = 0;
    while start < len {
        let n = BLOCK.min(len - start);
        let row0 = range.start + start;
        for (i, instr) in kernel.body.iter().enumerate() {
            let (before, rest) = regs.split_at_mut(i * BLOCK);
            let dst = &mut rest[..n];
            let r = |reg: u32| &before[reg as usize * BLOCK..reg as usize * BLOCK + n];
            let ty = |reg: u32| k.types[reg as usize].unwrap();
            match *instr {
                Instr::Load(c) => {
                    let s = slot_of[c as usize];
                    if s == usize::MAX {
                        dst.copy_from_slice(&input.columns[c as usize][row0..row0 + n]);
                    } else {
                        dst.copy_from_slice(&own[s][start..start + n]);
                    }
                }
                Instr::Param(p) => dst.fill(input.params[p as usize]),
                Instr::Const(_, bits) => dst.fill(bits),
                Instr::Row => {
                    for (j, d) in dst.iter_mut().enumerate() {
                        *d = (row0 + j) as u64;
                    }
                }
                Instr::Bin(op, a, b) => bin_block(op, ty(a), dst, r(a), r(b)),
                Instr::Un(op, a) => un_block(op, ty(a), dst, r(a)),
                Instr::Cmp(op, a, b) => cmp_block(op, ty(a), dst, r(a), r(b)),
                Instr::Select(c, a, b) => {
                    for (((d, c), a), b) in dst.iter_mut().zip(r(c)).zip(r(a)).zip(r(b)) {
                        // Branch-free: the mask is all ones when c is 1.
                        let mask = c.wrapping_neg();
                        *d = (a & mask) | (b & !mask);
                    }
                }
                Instr::Store(c, a) => {
                    own[slot_of[c as usize]][start..start + n].copy_from_slice(r(a));
                }
                Instr::Emit { buffer, cond, value } => {
                    let b = buffer as usize;
                    let cap = kernel.buffers[b].capacity;
                    for (j, (c, v)) in r(cond).iter().zip(r(value)).enumerate() {
                        if *c != 0 {
                            if events[b].len() < cap {
                                events[b].push(((row0 + j) as u64, *v));
                            } else {
                                overflow[b] += 1;
                            }
                        }
                    }
                }
            }
        }
        start += n;
    }
    ChunkOut { events, overflow }
}
