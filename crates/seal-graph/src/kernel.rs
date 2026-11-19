//! Lower a graph event to a data-parallel kernel, so it runs once per entity
//! over component columns instead of once per entity through the
//! interpreter. Only straight-line logic qualifies: the event's chain may hold
//! sequences and `world_set` on `self`, and its data may use arithmetic,
//! comparisons, selects, vectors and `world_get` on `self`. The editor shows
//! such an event as kernel-capable. Results are bit-identical to running the
//! graph on each entity.

use crate::rt::{GraphError, Host, Value};
use crate::{BinOp, CmpOp, Op, Program, Src, Ty, UnOp};
use seal_kernel as k;
use std::collections::BTreeMap;

pub use seal_jobs::JobPool;

/// One kernel column: a component field, or one half of a vec2 field.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldColumn {
    pub component: String,
    pub field: String,
    pub ty: Ty,
    /// For vec2 fields: 0 for x, 1 for y.
    pub lane: Option<u8>,
}

/// A graph event lowered to a kernel.
#[derive(Clone, Debug)]
pub struct Lowered {
    pub kernel: k::Checked,
    pub columns: Vec<FieldColumn>,
    /// Event parameters, in kernel parameter order (vec2 parameters take two).
    pub params: Vec<(String, Ty, Option<u8>)>,
    /// The event's parameter names, in argument order.
    pub event_params: Vec<String>,
}

#[derive(Clone, Copy)]
enum Lv {
    S(k::Reg),
    V(k::Reg, k::Reg),
}

struct Lower<'a> {
    p: &'a Program,
    event_node: u32,
    kern: k::Kernel,
    /// (component, field, lane) -> column index.
    cols: BTreeMap<(String, String, Option<u8>), u32>,
    columns: Vec<FieldColumn>,
    /// The current value of each column: loaded, or set earlier in the chain.
    current: BTreeMap<u32, k::Reg>,
    params: Vec<(String, Ty, Option<u8>)>,
    param_regs: BTreeMap<(u32, Option<u8>), k::Reg>,
}

fn no(node: u32, why: &str) -> String {
    format!("node {node}: {why}")
}

fn kty(t: Ty, node: u32) -> Result<k::Ty, String> {
    match t {
        Ty::Int => Ok(k::Ty::Int),
        Ty::Float | Ty::Vec2 => Ok(k::Ty::Float),
        Ty::Bool => Ok(k::Ty::Bool),
        Ty::Str => Err(no(node, "text values cannot run as a kernel")),
        Ty::Entity => Err(no(node, "entity values cannot run as a kernel")),
        Ty::Vec3 => Err(no(node, "vec3 values cannot run as a kernel yet")),
        Ty::Quat | Ty::Plane => Err(no(node, "quat and plane cannot run as a kernel yet")),
        Ty::List(_) => Err(no(node, "lists cannot run as a kernel")),
    }
}

impl Lower<'_> {
    fn column(&mut self, component: &str, field: &str, ty: Ty, lane: Option<u8>) -> u32 {
        let key = (component.to_string(), field.to_string(), lane);
        if let Some(c) = self.cols.get(&key) {
            return *c;
        }
        let kt = if ty == Ty::Vec2 { k::Ty::Float } else { kty(ty, 0).unwrap() };
        let c = self.kern.column(
            &format!("{component}.{field}{}", lane_suffix(lane)),
            kt,
            k::Access::ReadWrite,
        );
        self.cols.insert(key, c);
        self.columns.push(FieldColumn {
            component: component.into(),
            field: field.into(),
            ty,
            lane,
        });
        c
    }

    fn read_column(&mut self, c: u32) -> k::Reg {
        if let Some(r) = self.current.get(&c) {
            return *r;
        }
        let r = self.kern.push(k::Instr::Load(c));
        self.current.insert(c, r);
        r
    }

    fn konst(&mut self, v: &Value, node: u32) -> Result<Lv, String> {
        Ok(match v {
            Value::Bool(b) => Lv::S(self.kern.push(k::Instr::Const(k::Ty::Bool, k::boolean(*b)))),
            Value::Int(i) => Lv::S(self.kern.push(k::Instr::Const(k::Ty::Int, k::int(*i)))),
            Value::Float(f) => Lv::S(self.kern.push(k::Instr::Const(k::Ty::Float, k::float(*f)))),
            Value::Vec2(v) => Lv::V(
                self.kern.push(k::Instr::Const(k::Ty::Float, k::float(v.x))),
                self.kern.push(k::Instr::Const(k::Ty::Float, k::float(v.y))),
            ),
            Value::Str(_) => return Err(no(node, "text values cannot run as a kernel")),
            Value::Entity(_) => return Err(no(node, "entity values cannot run as a kernel")),
            Value::Vec3(_) => return Err(no(node, "vec3 values cannot run as a kernel yet")),
            Value::Quat(_) | Value::Plane(_) => {
                return Err(no(node, "quat and plane cannot run as a kernel yet"))
            }
            Value::List(..) => return Err(no(node, "lists cannot run as a kernel")),
        })
    }

    fn require_self(&self, src: &Src, node: u32) -> Result<(), String> {
        match src {
            Src::Pin { node: n, .. } if self.p.nodes[*n as usize].op == Op::SelfRef => Ok(()),
            _ => Err(no(node, "world access must use `self` to run as a kernel")),
        }
    }

    fn eval(&mut self, src: &Src, at: u32) -> Result<Lv, String> {
        match src {
            Src::Const(v) => self.konst(v, at),
            Src::Pin { node, pin } => self.pin(*node, *pin),
        }
    }

    fn param(&mut self, pin: u32, lane: Option<u8>, ty: Ty) -> k::Reg {
        if let Some(r) = self.param_regs.get(&(pin, lane)) {
            return *r;
        }
        let ev = self.p.events.iter().find(|e| e.node == self.event_node).unwrap();
        let name = ev.params[pin as usize].0.clone();
        let kt = if ty == Ty::Vec2 { k::Ty::Float } else { kty(ty, 0).unwrap() };
        let idx = self.kern.param(&format!("{name}{}", lane_suffix(lane)), kt);
        self.params.push((name, ty, lane));
        let r = self.kern.push(k::Instr::Param(idx));
        self.param_regs.insert((pin, lane), r);
        r
    }

    fn pin(&mut self, node: u32, pin: u32) -> Result<Lv, String> {
        let n = &self.p.nodes[node as usize];
        let id = n.id;
        if n.op.is_exec() {
            if node != self.event_node {
                return Err(no(id, "only the event's own outputs can be read in a kernel"));
            }
            let ty = n.out_tys[pin as usize];
            kty(ty, id)?;
            return Ok(if ty == Ty::Vec2 {
                Lv::V(self.param(pin, Some(0), ty), self.param(pin, Some(1), ty))
            } else {
                Lv::S(self.param(pin, None, ty))
            });
        }
        let op = n.op.clone();
        let inputs = n.inputs.clone();
        let s = |lv: Lv| match lv {
            Lv::S(r) => r,
            Lv::V(..) => unreachable!("checked types"),
        };
        Ok(match op {
            Op::Const(v) => self.konst(&v, id)?,
            Op::Get(_) => return Err(no(id, "graph variables cannot be read in a kernel yet")),
            Op::SelfRef => return Err(no(id, "`self` can only feed world access in a kernel")),
            Op::GameTime | Op::FrameDt => {
                return Err(no(id, "time cannot be read in a kernel yet"))
            }
            Op::RandomInt | Op::RandomFloat => {
                return Err(no(id, "random numbers cannot be drawn in a kernel yet"))
            }
            Op::Exists
            | Op::IsA { .. }
            | Op::FindAll { .. }
            | Op::FindFirst { .. }
            | Op::Player
            | Op::InputAxis
            | Op::ActionPressed
            | Op::ActionHeld
            | Op::ActionReleased
            | Op::MouseDelta
            | Op::MouseWheel
            | Op::PointerWorld
            | Op::Device(_)
            | Op::AiArrived
            | Op::Score
            | Op::UiValue
            | Op::UiInput
            | Op::GameState
            | Op::HasSave
            | Op::Saved
            | Op::SceneLoaded
            | Op::CurrentScene
            | Op::GetString
            | Op::DataField
            | Op::ReadCsvCell
            | Op::ReadJsonValue
            | Op::ScriptCore(_)
            | Op::RayHit { .. }
            | Op::BodiesInside
            | Op::Phys(_)
            | Op::CharacterQuery(_) => return Err(no(id, "entity checks cannot run as a kernel")),
            Op::WorldGet { component, field, ty } => {
                self.require_self(&inputs[0], id)?;
                kty(ty, id)?;
                if ty == Ty::Vec2 {
                    let (cx, cy) = (
                        self.column(&component, &field, ty, Some(0)),
                        self.column(&component, &field, ty, Some(1)),
                    );
                    Lv::V(self.read_column(cx), self.read_column(cy))
                } else {
                    let c = self.column(&component, &field, ty, None);
                    Lv::S(self.read_column(c))
                }
            }
            Op::Bin(b, t) => {
                let (a, c) = (self.eval(&inputs[0], id)?, self.eval(&inputs[1], id)?);
                if b == BinOp::Dot {
                    let (Lv::V(ax, ay), Lv::V(bx, by)) = (a, c) else {
                        unreachable!("dot takes vec2s")
                    };
                    let xx = self.kern.push(k::Instr::Bin(k::BinOp::Mul, ax, bx));
                    let yy = self.kern.push(k::Instr::Bin(k::BinOp::Mul, ay, by));
                    return Ok(Lv::S(self.kern.push(k::Instr::Bin(k::BinOp::Add, xx, yy))));
                }
                let kb = match b {
                    BinOp::Add => k::BinOp::Add,
                    BinOp::Sub => k::BinOp::Sub,
                    BinOp::Mul => k::BinOp::Mul,
                    BinOp::Div => k::BinOp::Div,
                    BinOp::Mod => k::BinOp::Mod,
                    BinOp::Min => k::BinOp::Min,
                    BinOp::Max => k::BinOp::Max,
                    BinOp::And => k::BinOp::And,
                    BinOp::Or => k::BinOp::Or,
                    BinOp::Concat | BinOp::StrContains | BinOp::StrStarts | BinOp::StrEnds => {
                        return Err(no(id, "text values cannot run as a kernel"))
                    }
                    BinOp::Dot => unreachable!("handled above"),
                    BinOp::Cross => return Err(no(id, "vec3 values cannot run as a kernel yet")),
                    BinOp::Pow | BinOp::Atan2 => {
                        return Err(no(id, &format!("{b:?} cannot run as a kernel yet")))
                    }
                };
                match (t, a, c) {
                    (Ty::Vec2, Lv::V(ax, ay), Lv::V(bx, by)) => Lv::V(
                        self.kern.push(k::Instr::Bin(kb, ax, bx)),
                        self.kern.push(k::Instr::Bin(kb, ay, by)),
                    ),
                    (_, a, c) => Lv::S(self.kern.push(k::Instr::Bin(kb, s(a), s(c)))),
                }
            }
            Op::Un(u, t) => {
                let a = self.eval(&inputs[0], id)?;
                let un = |me: &mut Self, op: k::UnOp, r: k::Reg| me.kern.push(k::Instr::Un(op, r));
                match (u, a) {
                    (UnOp::Neg, Lv::V(x, y)) => {
                        Lv::V(un(self, k::UnOp::Neg, x), un(self, k::UnOp::Neg, y))
                    }
                    (UnOp::Vec2X, Lv::V(x, _)) => Lv::S(x),
                    (UnOp::Vec2Y, Lv::V(_, y)) => Lv::S(y),
                    (UnOp::Length, Lv::V(x, y)) => {
                        let xx = self.kern.push(k::Instr::Bin(k::BinOp::Mul, x, x));
                        let yy = self.kern.push(k::Instr::Bin(k::BinOp::Mul, y, y));
                        let sum = self.kern.push(k::Instr::Bin(k::BinOp::Add, xx, yy));
                        Lv::S(un(self, k::UnOp::Sqrt, sum))
                    }
                    (UnOp::ToStr, _) => return Err(no(id, "text values cannot run as a kernel")),
                    (u, a) => {
                        let op = match u {
                            UnOp::Neg => k::UnOp::Neg,
                            UnOp::Abs => k::UnOp::Abs,
                            UnOp::Sqrt => k::UnOp::Sqrt,
                            UnOp::Not => k::UnOp::Not,
                            UnOp::ToFloat => k::UnOp::ToFloat,
                            UnOp::ToInt => k::UnOp::ToInt,
                            _ => {
                                return Err(no(
                                    id,
                                    &format!("{u:?} of {} cannot run as a kernel", t.name()),
                                ))
                            }
                        };
                        Lv::S(un(self, op, s(a)))
                    }
                }
            }
            Op::Compare(c, t) => {
                let (a, b) = (self.eval(&inputs[0], id)?, self.eval(&inputs[1], id)?);
                let kc = match c {
                    CmpOp::Lt => k::CmpOp::Lt,
                    CmpOp::Le => k::CmpOp::Le,
                    CmpOp::Gt => k::CmpOp::Gt,
                    CmpOp::Ge => k::CmpOp::Ge,
                    CmpOp::Eq => k::CmpOp::Eq,
                    CmpOp::Ne => k::CmpOp::Ne,
                };
                match (a, b) {
                    (Lv::V(ax, ay), Lv::V(bx, by)) => {
                        if !matches!(c, CmpOp::Eq | CmpOp::Ne) {
                            return Err(no(id, "vec2 ordering cannot run as a kernel"));
                        }
                        let ex = self.kern.push(k::Instr::Cmp(k::CmpOp::Eq, ax, bx));
                        let ey = self.kern.push(k::Instr::Cmp(k::CmpOp::Eq, ay, by));
                        let both = self.kern.push(k::Instr::Bin(k::BinOp::And, ex, ey));
                        Lv::S(if c == CmpOp::Eq {
                            both
                        } else {
                            self.kern.push(k::Instr::Un(k::UnOp::Not, both))
                        })
                    }
                    (a, b) => {
                        kty(t, id)?;
                        Lv::S(self.kern.push(k::Instr::Cmp(kc, s(a), s(b))))
                    }
                }
            }
            Op::Select(_) => {
                let c = s(self.eval(&inputs[0], id)?);
                match (self.eval(&inputs[1], id)?, self.eval(&inputs[2], id)?) {
                    (Lv::V(ax, ay), Lv::V(bx, by)) => Lv::V(
                        self.kern.push(k::Instr::Select(c, ax, bx)),
                        self.kern.push(k::Instr::Select(c, ay, by)),
                    ),
                    (a, b) => Lv::S(self.kern.push(k::Instr::Select(c, s(a), s(b)))),
                }
            }
            Op::GetState | Op::TimeInState => {
                return Err(no(id, "state machines cannot run as a kernel"))
            }
            Op::Poly(_) => return Err(no(id, "polygon shapes cannot run as a kernel")),
            Op::Server(_) => return Err(no(id, "physics handles cannot run as a kernel")),
            Op::MakeVec2 => Lv::V(s(self.eval(&inputs[0], id)?), s(self.eval(&inputs[1], id)?)),
            Op::Scale(_) => {
                let v = self.eval(&inputs[0], id)?;
                let f = s(self.eval(&inputs[1], id)?);
                let Lv::V(x, y) = v else { unreachable!("checked types") };
                Lv::V(
                    self.kern.push(k::Instr::Bin(k::BinOp::Mul, x, f)),
                    self.kern.push(k::Instr::Bin(k::BinOp::Mul, y, f)),
                )
            }
            _ => unreachable!("execution ops handled above"),
        })
    }

    /// Walk the execution chain in the interpreter's order.
    fn chain(&mut self, start: Option<u32>, depth: usize) -> Result<(), String> {
        let mut at = start;
        while let Some(i) = at {
            let n = &self.p.nodes[i as usize];
            let id = n.id;
            match n.op.clone() {
                Op::Sequence(count) => {
                    if depth > 64 {
                        return Err(no(id, "sequences nest too deep"));
                    }
                    let next = n.next.clone();
                    for b in next.into_iter().take(count as usize) {
                        self.chain(b, depth + 1)?;
                    }
                    return Ok(());
                }
                Op::WorldSet { component, field, ty } => {
                    let inputs = n.inputs.clone();
                    let next = n.next[0];
                    self.require_self(&inputs[0], id)?;
                    kty(ty, id)?;
                    match self.eval(&inputs[1], id)? {
                        Lv::V(x, y) => {
                            let (cx, cy) = (
                                self.column(&component, &field, ty, Some(0)),
                                self.column(&component, &field, ty, Some(1)),
                            );
                            self.current.insert(cx, x);
                            self.current.insert(cy, y);
                        }
                        Lv::S(r) => {
                            let c = self.column(&component, &field, ty, None);
                            self.current.insert(c, r);
                        }
                    }
                    at = next;
                }
                Op::Branch => {
                    return Err(no(id, "branches cannot run as a kernel yet; use select"))
                }
                Op::For => return Err(no(id, "loops cannot run as a kernel")),
                Op::Delay => return Err(no(id, "delays cannot run as a kernel")),
                Op::Set(_) => return Err(no(id, "setting graph variables cannot run as a kernel")),
                Op::Call(_) => return Err(no(id, "event calls cannot run as a kernel")),
                Op::Return(_) => return Err(no(id, "returns cannot run as a kernel")),
                Op::SetTimer(_) | Op::ClearTimer(_) => {
                    return Err(no(id, "timers cannot run as a kernel"))
                }
                Op::Log => return Err(no(id, "logging cannot run as a kernel")),
                Op::Spawn { .. } | Op::Despawn => {
                    return Err(no(id, "spawning and despawning cannot run as a kernel"))
                }
                Op::CameraShake => return Err(no(id, "the camera cannot shake from a kernel")),
                Op::Rumble | Op::RebindAction | Op::Device(_) => {
                    return Err(no(id, "gamepads cannot be used from a kernel"))
                }
                Op::UiShow | Op::UiText | Op::Typewriter => {
                    return Err(no(id, "on-screen widgets cannot change from a kernel"))
                }
                Op::PlayAnim | Op::StopAnim | Op::PlayClip | Op::PlaySequence => {
                    return Err(no(id, "animations cannot start from a kernel"))
                }
                Op::Ai(_) => return Err(no(id, "AI orders cannot be given from a kernel")),
                Op::Game(_) => return Err(no(id, "game rules cannot change from a kernel")),
                Op::ApplyImpulse => return Err(no(id, "impulses cannot be applied from a kernel")),
                Op::Character(_) => return Err(no(id, "a character cannot move from a kernel")),
                Op::ApplyForce2d | Op::ApplyTorque2d | Op::SetVelocity2d | Op::SetJointTarget2d => {
                    return Err(no(id, "2D forces cannot be applied from a kernel"))
                }
                Op::SetColliderShape | Op::SetColliderEnabled => {
                    return Err(no(id, "colliders cannot change from a kernel"))
                }
                Op::Mass2d => return Err(no(id, "mass cannot be read from a kernel")),
                Op::PlaySound { .. } => return Err(no(id, "sounds cannot play from a kernel")),
                Op::PlayMusic | Op::StopMusic | Op::SetMusicVolume => {
                    return Err(no(id, "music cannot play from a kernel"))
                }
                Op::OpenFileDialog
                | Op::SaveFileDialog
                | Op::Encrypt
                | Op::Decrypt
                | Op::ShowDialog
                | Op::CloudUpload
                | Op::CloudDownload
                | Op::CloudFlush
                | Op::CloudReply => {
                    return Err(no(
                        id,
                        "file dialogs, encryption, dialogs and cloud saves cannot run as a kernel",
                    ))
                }
                Op::SetState | Op::Trigger => {
                    return Err(no(id, "state machines cannot run as a kernel"))
                }
                Op::Server(_) => return Err(no(id, "physics handles cannot run as a kernel")),
                Op::Event { .. } => unreachable!("events have no execution input"),
                _ => unreachable!("only execution nodes are chained"),
            }
        }
        Ok(())
    }
}

fn lane_suffix(lane: Option<u8>) -> &'static str {
    match lane {
        Some(0) => ".x",
        Some(_) => ".y",
        None => "",
    }
}

/// Lower `event` of `program` to a kernel, or say why it cannot be.
pub fn lower(program: &Program, event: &str) -> Result<Lowered, String> {
    let ev = program.events.iter().find(|e| e.name == event).ok_or(format!("no event {event}"))?;
    let mut l = Lower {
        p: program,
        event_node: ev.node,
        kern: k::Kernel::new(&format!("{}.{}", program.name, event), k::ResultClass::Exact),
        cols: BTreeMap::new(),
        columns: Vec::new(),
        current: BTreeMap::new(),
        params: Vec::new(),
        param_regs: BTreeMap::new(),
    };
    l.chain(program.nodes[ev.node as usize].next[0], 0)?;
    // Store each column whose value changed, once, at the end.
    let changed: Vec<(u32, k::Reg)> = l
        .current
        .iter()
        .filter(|(c, r)| l.kern.body.get(**r as usize) != Some(&k::Instr::Load(**c)))
        .map(|(c, r)| (*c, *r))
        .collect();
    for (c, r) in changed {
        l.kern.push(k::Instr::Store(c, r));
    }
    let kernel = l.kern.check().map_err(|e| format!("lowered kernel is invalid: {e}"))?;
    let event_params = ev.params.iter().map(|(n, _)| n.clone()).collect();
    Ok(Lowered { kernel, columns: l.columns, params: l.params, event_params })
}

impl Lowered {
    /// Run over `entities` through `host`: read each column, run the kernel
    /// on `pool`, write the columns back. `args` are the event's arguments.
    pub fn run(
        &self,
        entities: &[u64],
        args: &[Value],
        host: &mut dyn Host,
        pool: &seal_jobs::JobPool,
    ) -> Result<k::Events, GraphError> {
        let mut columns = Vec::with_capacity(self.columns.len());
        for col in &self.columns {
            let mut data = Vec::with_capacity(entities.len());
            for e in entities {
                let v = crate::rt::world_get(host, *e, &col.component, &col.field, col.ty, 0)?;
                data.push(bits(&v, col.lane));
            }
            columns.push(data);
        }
        let mut params = Vec::with_capacity(self.params.len());
        for (name, _, lane) in &self.params {
            let at = self
                .param_index(name)
                .ok_or_else(|| GraphError { node: None, message: format!("no argument {name}") })?;
            let v = args.get(at).ok_or_else(|| GraphError {
                node: None,
                message: format!("missing argument {name}"),
            })?;
            params.push(bits(v, *lane));
        }
        let mut data = k::Data { columns, params };
        let events = k::run_cpu(&self.kernel, &mut data, pool, k::CpuOptions::default());
        // Write back only what the kernel stores, field by field.
        let stored: Vec<u32> = self
            .kernel
            .kernel
            .body
            .iter()
            .filter_map(|i| if let k::Instr::Store(c, _) = i { Some(*c) } else { None })
            .collect();
        let mut done = Vec::new();
        for c in &stored {
            let col = &self.columns[*c as usize];
            if done.contains(&(&col.component, &col.field)) {
                continue;
            }
            done.push((&col.component, &col.field));
            for (row, e) in entities.iter().enumerate() {
                let v = match col.ty {
                    Ty::Vec2 => {
                        let lane_of = |lane: u8| {
                            self.columns.iter().position(|o| {
                                o.component == col.component
                                    && o.field == col.field
                                    && o.lane == Some(lane)
                            })
                        };
                        let x = f64::from_bits(data.columns[lane_of(0).unwrap()][row]);
                        let y = f64::from_bits(data.columns[lane_of(1).unwrap()][row]);
                        Value::Vec2(crate::rt::V2 { x, y })
                    }
                    Ty::Int => Value::Int(data.columns[*c as usize][row] as i64),
                    Ty::Float => Value::Float(f64::from_bits(data.columns[*c as usize][row])),
                    Ty::Bool => Value::Bool(data.columns[*c as usize][row] != 0),
                    _ => unreachable!("checked when lowering"),
                };
                crate::rt::world_set(host, *e, &col.component, &col.field, v, 0)?;
            }
        }
        Ok(events)
    }

    fn param_index(&self, name: &str) -> Option<usize> {
        self.event_params.iter().position(|p| p == name)
    }
}

impl Lowered {
    /// Closed-form patterns the proof pass finds in this kernel, keyed by
    /// "component.field" (with a lane suffix for a vec2 lane).
    pub fn patterns(&self) -> Vec<(String, seal_predict::Pattern)> {
        seal_predict::analyze(&self.kernel)
            .into_iter()
            .filter_map(|(col, p)| {
                let c = self.columns.get(col as usize)?;
                let lane = lane_suffix(c.lane);
                Some((format!("{}.{}{}", c.component, c.field, lane), p))
            })
            .collect()
    }
}

/// A kernel whose whole effect is one proven timer, held on a timing wheel.
pub struct WheelRun {
    pub bank: seal_predict::TimerBank,
    column: usize,
}

impl Lowered {
    /// Take over this event's per-tick work for `entities` as one timer bank.
    /// Returns `None` unless the kernel has no parameters, no events, and
    /// writes exactly one int column that follows the timer closed form.
    pub fn wheel_run(
        &self,
        entities: &[u64],
        host: &mut dyn Host,
    ) -> Result<Option<WheelRun>, GraphError> {
        let body = &self.kernel.kernel.body;
        let mut stored =
            body.iter().filter_map(|i| if let k::Instr::Store(c, _) = i { Some(*c) } else { None });
        let (Some(col), None) = (stored.next(), stored.next()) else { return Ok(None) };
        if !self.params.is_empty() || body.iter().any(|i| matches!(i, k::Instr::Emit { .. })) {
            return Ok(None);
        }
        let Some((_, pattern)) =
            seal_predict::analyze(&self.kernel).into_iter().find(|(c, _)| *c == col)
        else {
            return Ok(None);
        };
        let c = &self.columns[col as usize];
        let mut values = Vec::with_capacity(entities.len());
        for e in entities {
            let v = crate::rt::world_get(host, *e, &c.component, &c.field, c.ty, 0)?;
            values.push(bits(&v, c.lane));
        }
        Ok(seal_predict::TimerBank::new(&pattern, &values, &[])
            .map(|bank| WheelRun { bank, column: col as usize }))
    }
}

impl WheelRun {
    /// Run `ticks` ticks on the wheel and write each entity's value back.
    pub fn advance(
        &mut self,
        lowered: &Lowered,
        entities: &[u64],
        ticks: u64,
        host: &mut dyn Host,
    ) -> Result<(), GraphError> {
        self.bank.advance(ticks);
        let c = &lowered.columns[self.column];
        for (e, bits) in entities.iter().zip(self.bank.values()) {
            host.set(*e, &c.component, &c.field, Value::Int(bits as i64))
                .map_err(|m| GraphError::at(0, m))?;
        }
        Ok(())
    }
}

fn bits(v: &Value, lane: Option<u8>) -> u64 {
    match (v, lane) {
        (Value::Vec2(p), Some(0)) => p.x.to_bits(),
        (Value::Vec2(p), _) => p.y.to_bits(),
        (Value::Int(i), _) => *i as u64,
        (Value::Float(f), _) => f.to_bits(),
        (Value::Bool(b), _) => *b as u64,
        _ => 0,
    }
}
