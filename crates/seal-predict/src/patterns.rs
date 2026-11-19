//! Level 1 prediction: closed forms the compiler can prove from a kernel.
//!
//! [`analyze`] looks at each column of a checked kernel. A column qualifies
//! only when one unconditional store writes it, every load of it comes before
//! that store, and the stored value depends on nothing but the column itself,
//! constants and parameters. Then the per-tick update has a closed form:
//!
//! | Pattern | Update | Closed form |
//! | --- | --- | --- |
//! | [`Pattern::Unwritten`] | none | constant |
//! | [`Pattern::Step`] | `x += c` | `x0 + c t` |
//! | [`Pattern::Affine`] | `x = x a + b` | `a^t x0 + b (a^t - 1)/(a - 1)` |
//! | [`Pattern::Timer`] | `x += c; if x >= limit { fire; x = 0 }` | fire times in a timing wheel |
//!
//! Integer steps and timers are [`Exactness::Exact`]: wrapping integer
//! arithmetic makes `x0 + c t` equal to `t` additions bit for bit, so they
//! may replace per-tick work in replicated and replayed state. Float closed
//! forms round differently from repeated adds, so they are only
//! [`Exactness::Bounded`] and must not stand in for Exact kernels.

use seal_kernel::{Access, BinOp, Checked, CmpOp, Instr, ResultClass, Ty};

use crate::wheel::TimingWheel;

/// A loop-invariant operand: a constant or a kernel parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Term {
    Const(u64),
    Param(u32),
}

impl Term {
    pub fn bits(self, params: &[u64]) -> u64 {
        match self {
            Term::Const(bits) => bits,
            Term::Param(p) => params[p as usize],
        }
    }
}

/// What an emitted event carries when a timer fires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmitValue {
    Row,
    Const(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    /// No instruction writes the column.
    Unwritten,
    /// `x = x + by` (or `x - by` with `negate`).
    Step { ty: Ty, by: Term, negate: bool },
    /// `x = x * a + b`, floats only.
    Affine { a: Term, b: Term },
    /// `t = x + by; fire = t >= limit; x = fire ? 0 : t`, ints only. `emit` is
    /// the buffer that records each fire, when the kernel has exactly one
    /// emit on that condition with a value the wheel can reproduce.
    Timer { by: Term, limit: Term, emit: Option<(u32, EmitValue)> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exactness {
    /// Bit-identical to running the kernel tick by tick.
    Exact,
    /// Equal within float rounding.
    Bounded,
}

impl Pattern {
    pub fn exactness(&self) -> Exactness {
        match self {
            Pattern::Unwritten => Exactness::Exact,
            Pattern::Step { ty: Ty::Int, .. } | Pattern::Timer { .. } => Exactness::Exact,
            _ => Exactness::Bounded,
        }
    }

    /// Whether this closed form may replace per-tick work in a kernel of `class`.
    pub fn allowed_for(&self, class: ResultClass) -> bool {
        class != ResultClass::Exact || self.exactness() == Exactness::Exact
    }

    /// The column's value after `ticks` updates from `x0`, for every pattern but timers.
    pub fn advance(&self, x0: u64, ticks: u64, params: &[u64]) -> Option<u64> {
        match *self {
            Pattern::Unwritten => Some(x0),
            Pattern::Step { ty: Ty::Int, by, negate } => {
                let c = by.bits(params) as i64;
                let c = if negate { c.wrapping_neg() } else { c };
                Some((x0 as i64).wrapping_add(c.wrapping_mul(ticks as i64)) as u64)
            }
            Pattern::Step { ty: Ty::Float, by, negate } => {
                let c = f64::from_bits(by.bits(params));
                let c = if negate { -c } else { c };
                Some((f64::from_bits(x0) + c * ticks as f64).to_bits())
            }
            Pattern::Affine { a, b } => {
                let (a, b) = (f64::from_bits(a.bits(params)), f64::from_bits(b.bits(params)));
                let x = f64::from_bits(x0);
                let at = a.powf(ticks as f64);
                let tail = if a == 1.0 { b * ticks as f64 } else { b * (at - 1.0) / (a - 1.0) };
                Some((at * x + tail).to_bits())
            }
            Pattern::Step { ty: Ty::Bool, .. } | Pattern::Timer { .. } => None,
        }
    }
}

/// A register's value as a tree over loads, constants and parameters.
#[derive(Clone, Debug, PartialEq)]
enum Expr {
    Load(u32),
    Term(Term),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Cmp(CmpOp, Box<Expr>, Box<Expr>),
    Select(Box<Expr>, Box<Expr>, Box<Expr>),
    Row,
    Other,
}

fn expr(body: &[Instr], r: u32) -> Expr {
    match body[r as usize] {
        Instr::Load(c) => Expr::Load(c),
        Instr::Param(p) => Expr::Term(Term::Param(p)),
        Instr::Const(_, bits) => Expr::Term(Term::Const(bits)),
        Instr::Row => Expr::Row,
        Instr::Bin(op, a, b) => Expr::Bin(op, Box::new(expr(body, a)), Box::new(expr(body, b))),
        Instr::Cmp(op, a, b) => Expr::Cmp(op, Box::new(expr(body, a)), Box::new(expr(body, b))),
        Instr::Select(c, a, b) => {
            Expr::Select(Box::new(expr(body, c)), Box::new(expr(body, a)), Box::new(expr(body, b)))
        }
        _ => Expr::Other,
    }
}

fn term(e: &Expr) -> Option<Term> {
    match e {
        Expr::Term(t) => Some(*t),
        _ => None,
    }
}

/// `x + c`, `c + x` or `x - c` over column `col`.
fn step_of(e: &Expr, col: u32) -> Option<(Term, bool)> {
    let Expr::Bin(op, a, b) = e else { return None };
    match (op, &**a, &**b) {
        (BinOp::Add, Expr::Load(c), k) | (BinOp::Add, k, Expr::Load(c)) if *c == col => {
            term(k).map(|t| (t, false))
        }
        (BinOp::Sub, Expr::Load(c), k) if *c == col => term(k).map(|t| (t, true)),
        _ => None,
    }
}

/// `x * a + b` in any operand order, or `x * a`.
fn affine_of(e: &Expr, col: u32) -> Option<(Term, Term)> {
    let mul = |e: &Expr| -> Option<Term> {
        let Expr::Bin(BinOp::Mul, a, b) = e else { return None };
        match (&**a, &**b) {
            (Expr::Load(c), k) | (k, Expr::Load(c)) if *c == col => term(k),
            _ => None,
        }
    };
    if let Some(a) = mul(e) {
        return Some((a, Term::Const(0f64.to_bits())));
    }
    let Expr::Bin(BinOp::Add, l, r) = e else { return None };
    match (mul(l), term(r), mul(r), term(l)) {
        (Some(a), Some(b), _, _) | (_, _, Some(a), Some(b)) => Some((a, b)),
        _ => None,
    }
}

/// Find every proven closed form in `k`, one per column that has one.
pub fn analyze(k: &Checked) -> Vec<(u32, Pattern)> {
    let body = &k.kernel.body;
    let mut out = Vec::new();
    for (col, column) in k.kernel.columns.iter().enumerate() {
        let col = col as u32;
        let stores: Vec<(usize, u32)> = body
            .iter()
            .enumerate()
            .filter_map(|(i, instr)| match *instr {
                Instr::Store(c, r) if c == col => Some((i, r)),
                _ => None,
            })
            .collect();
        if stores.is_empty() {
            if column.access == Access::Read || column.access == Access::ReadWrite {
                out.push((col, Pattern::Unwritten));
            }
            continue;
        }
        let [(at, value)] = stores[..] else { continue };
        let late_load =
            body.iter().enumerate().any(|(i, instr)| i > at && *instr == Instr::Load(col));
        if late_load {
            continue;
        }
        let e = expr(body, value);
        if let Some((by, negate)) = step_of(&e, col) {
            if column.ty != Ty::Bool {
                out.push((col, Pattern::Step { ty: column.ty, by, negate }));
            }
            continue;
        }
        if column.ty == Ty::Float {
            if let Some((a, b)) = affine_of(&e, col) {
                out.push((col, Pattern::Affine { a, b }));
                continue;
            }
        }
        if column.ty == Ty::Int {
            if let Some(timer) = timer_of(k, &e, col) {
                out.push((col, timer));
            }
        }
    }
    out
}

fn timer_of(k: &Checked, e: &Expr, col: u32) -> Option<Pattern> {
    let Expr::Select(cond, zero, next) = e else { return None };
    if **zero != Expr::Term(Term::Const(0)) {
        return None;
    }
    let (by, negate) = step_of(next, col)?;
    if negate {
        return None;
    }
    let Expr::Cmp(CmpOp::Ge, lhs, limit) = &**cond else { return None };
    if lhs != next {
        return None;
    }
    let limit = term(limit)?;
    let body = &k.kernel.body;
    let mut emits = body.iter().filter_map(|instr| match *instr {
        Instr::Emit { buffer, cond: c, value } if expr(body, c) == **cond => {
            let value = match expr(body, value) {
                Expr::Row => Some(EmitValue::Row),
                Expr::Term(Term::Const(bits)) => Some(EmitValue::Const(bits)),
                _ => None,
            };
            Some(value.map(|v| (buffer, v)))
        }
        _ => None,
    });
    let emit = match (emits.next(), emits.next()) {
        (Some(Some(one)), None) => Some(one),
        _ => None,
    };
    Some(Pattern::Timer { by, limit, emit })
}

/// Many rows of one proven timer, run on a timing wheel instead of per tick.
///
/// After [`TimerBank::advance`] the fires and every row's value match what
/// running the kernel that many ticks would give, bit for bit.
pub struct TimerBank {
    by: i64,
    limit: i64,
    period: u64,
    emit: Option<EmitValue>,
    wheel: TimingWheel<()>,
    /// Per row: the value at `since` and the tick it was set.
    base: Vec<(i64, u64)>,
}

impl TimerBank {
    /// Returns `None` when the rows fall outside what the closed form covers
    /// (a non-positive step or limit, a negative start, or a value that could
    /// overflow); the caller then keeps running the kernel.
    pub fn new(pattern: &Pattern, values: &[u64], params: &[u64]) -> Option<Self> {
        let Pattern::Timer { by, limit, emit } = *pattern else { return None };
        let (by, limit) = (by.bits(params) as i64, limit.bits(params) as i64);
        if by <= 0 || limit <= 0 || limit.checked_add(by).is_none() {
            return None;
        }
        let period = (limit as u64).div_ceil(by as u64);
        let mut wheel = TimingWheel::new(0);
        let mut base = Vec::with_capacity(values.len());
        for (row, &bits) in values.iter().enumerate() {
            let x0 = bits as i64;
            if x0 < 0 || x0.checked_add(by).is_none() {
                return None;
            }
            let first =
                if x0 + by >= limit { 1 } else { ((limit - x0) as u64).div_ceil(by as u64) };
            wheel.schedule(first, row as u64, ());
            base.push((x0, 0));
        }
        Some(Self { by, limit, period, emit: emit.map(|e| e.1), wheel, base })
    }

    pub fn now(&self) -> u64 {
        self.wheel.now()
    }

    /// Run `ticks` more ticks. Returns `(tick, row, value bits)` for each
    /// fire, in the order per-tick runs would emit them (tick, then row).
    pub fn advance(&mut self, ticks: u64) -> Vec<(u64, u64, u64)> {
        let target = self.wheel.now() + ticks;
        let (period, emit) = (self.period, self.emit);
        let base = &mut self.base;
        let mut fires = Vec::new();
        self.wheel.advance(target, |wheel, tick, row, ()| {
            base[row as usize] = (0, tick);
            let value = match emit {
                Some(EmitValue::Const(bits)) => bits,
                _ => row,
            };
            fires.push((tick, row, value));
            wheel.schedule(tick + period, row, ());
        });
        fires
    }

    /// Each row's value at the current tick, as the kernel would hold it.
    pub fn values(&self) -> Vec<u64> {
        let now = self.wheel.now();
        self.base.iter().map(|&(x, since)| (x + self.by * (now - since) as i64) as u64).collect()
    }

    pub fn limit(&self) -> i64 {
        self.limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seal_kernel::{int, run_scalar, Data, Kernel};

    fn timer_kernel() -> Checked {
        let mut k = Kernel::new("spawner", ResultClass::Exact);
        let x = k.column("t", Ty::Int, Access::ReadWrite);
        let step = k.param("step", Ty::Int);
        let limit = k.param("limit", Ty::Int);
        let fired = k.buffer("fired", Ty::Int, usize::MAX);
        let lx = k.push(Instr::Load(x));
        let ls = k.push(Instr::Param(step));
        let next = k.push(Instr::Bin(BinOp::Add, lx, ls));
        let ll = k.push(Instr::Param(limit));
        let fire = k.push(Instr::Cmp(CmpOp::Ge, next, ll));
        let zero = k.push(Instr::Const(Ty::Int, 0));
        let new = k.push(Instr::Select(fire, zero, next));
        k.push(Instr::Store(x, new));
        let row = k.push(Instr::Row);
        k.push(Instr::Emit { buffer: fired, cond: fire, value: row });
        k.check().unwrap()
    }

    #[test]
    fn finds_steps_affine_constants_and_timers() {
        let mut k = Kernel::new("mix", ResultClass::Bounded);
        let count = k.column("count", Ty::Int, Access::ReadWrite);
        let heat = k.column("heat", Ty::Float, Access::ReadWrite);
        let cfg = k.column("cfg", Ty::Int, Access::Read);
        let other = k.column("other", Ty::Int, Access::ReadWrite);
        let a = k.push(Instr::Load(count));
        let one = k.push(Instr::Const(Ty::Int, int(1)));
        let s = k.push(Instr::Bin(BinOp::Add, one, a));
        k.push(Instr::Store(count, s));
        let h = k.push(Instr::Load(heat));
        let decay = k.push(Instr::Const(Ty::Float, 0.9f64.to_bits()));
        let m = k.push(Instr::Bin(BinOp::Mul, h, decay));
        let add = k.push(Instr::Const(Ty::Float, 0.5f64.to_bits()));
        let hv = k.push(Instr::Bin(BinOp::Add, m, add));
        k.push(Instr::Store(heat, hv));
        // `other` depends on `cfg`, so it has no closed form of its own.
        let o = k.push(Instr::Load(other));
        let c = k.push(Instr::Load(cfg));
        let ov = k.push(Instr::Bin(BinOp::Add, o, c));
        k.push(Instr::Store(other, ov));
        let found = analyze(&k.check().unwrap());
        assert_eq!(found.len(), 3, "{found:?}");
        assert_eq!(
            found[0],
            (count, Pattern::Step { ty: Ty::Int, by: Term::Const(int(1)), negate: false })
        );
        assert!(matches!(found[1], (c, Pattern::Affine { .. }) if c == heat));
        assert_eq!(found[2], (cfg, Pattern::Unwritten));
        assert!(found.iter().all(|(c, _)| *c != other));
        let timer = analyze(&timer_kernel());
        assert!(matches!(timer[..], [(0, Pattern::Timer { emit: Some((0, EmitValue::Row)), .. })]));
        assert!(timer[0].1.allowed_for(ResultClass::Exact));
        assert!(!found[1].1.allowed_for(ResultClass::Exact));
    }

    #[test]
    fn int_step_closed_form_is_bit_exact_even_when_it_wraps() {
        let p = Pattern::Step { ty: Ty::Int, by: Term::Param(0), negate: false };
        let params = [int(i64::MAX / 3)];
        let mut x = int(-7);
        for _ in 0..1000 {
            x = (x as i64).wrapping_add(params[0] as i64) as u64;
        }
        assert_eq!(p.advance(int(-7), 1000, &params), Some(x));
    }

    #[test]
    fn affine_closed_form_stays_within_rounding() {
        let p =
            Pattern::Affine { a: Term::Const(0.9f64.to_bits()), b: Term::Const(0.5f64.to_bits()) };
        let mut x = 10.0f64;
        for _ in 0..200 {
            x = x * 0.9 + 0.5;
        }
        let got = f64::from_bits(p.advance(10f64.to_bits(), 200, &[]).unwrap());
        assert!((got - x).abs() < 1e-9, "{got} vs {x}");
        assert_eq!(p.exactness(), Exactness::Bounded);
    }

    #[test]
    fn timer_bank_matches_running_the_kernel_tick_by_tick() {
        let k = timer_kernel();
        let rows = 2000usize;
        let start: Vec<u64> = (0..rows).map(|i| int(((i * 7919) % 97) as i64)).collect();
        let params = vec![int(3), int(97)];
        let mut data = Data { columns: vec![start.clone()], params: params.clone() };
        let mut expected = Vec::new();
        for tick in 1..=500u64 {
            let events = run_scalar(&k, &mut data);
            expected.extend(events.buffers[0].iter().map(|&(row, v)| (tick, row, v)));
        }
        let pattern = analyze(&k)[0].1;
        let mut bank = TimerBank::new(&pattern, &start, &params).unwrap();
        let mut got = bank.advance(123);
        got.extend(bank.advance(377));
        assert_eq!(got, expected);
        assert_eq!(bank.values(), data.columns[0]);
        assert!(!got.is_empty());
    }

    #[test]
    fn timer_bank_declines_what_it_cannot_prove() {
        let pattern = analyze(&timer_kernel())[0].1;
        assert!(TimerBank::new(&pattern, &[int(0)], &[int(0), int(10)]).is_none());
        assert!(TimerBank::new(&pattern, &[int(-1)], &[int(1), int(10)]).is_none());
        assert!(TimerBank::new(&pattern, &[int(0)], &[int(1), int(i64::MAX)]).is_none());
    }
}
