//! The editor play interpreter: runs a [`Program`] directly, with
//! breakpoints, single stepping, watched values, a trace and live reload.

use crate::rt::NO_ENTITY;
use crate::script_core;
use crate::rt::{
    self, Budget, Cont, GraphError, GraphInstance, Host, RunState, Timers, Value, DEFAULT_BUDGET,
};
use crate::{apply_pure, state_machine, Op, Program, Src};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Debug)]
enum Job {
    Resume(u32),
    /// An event timer's firing; skipped if the timer was set or cleared since.
    Timer(rt::Timer),
    Event(usize, Vec<Value>),
    /// Enter the initial state when needed, then queue update and the transition check.
    BeginMachine,
    /// On Update of whatever state is current when the job starts.
    Update,
    /// Adopt a state, then run its On Enter event.
    Commit {
        state: u32,
        forced: bool,
        enter: Option<u32>,
    },
    /// Take at most one transition.
    Finish,
}

#[derive(Clone, Debug)]
struct Paused {
    stack: Vec<Cont>,
    budget: Budget,
}

/// What a live reload kept and what it had to drop.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReloadReport {
    pub kept_vars: Vec<String>,
    pub reset_vars: Vec<String>,
    pub kept_delays: usize,
    pub dropped_delays: usize,
}

/// Runs a graph without compiling it.
#[derive(Clone, Debug)]
pub struct Interp {
    program: Program,
    me: u64,
    vars: Vec<Value>,
    slots: Vec<Value>,
    timers: Timers,
    time: f64,
    /// The length of the last frame, for `frame_dt`.
    dt: f64,
    /// The state of this instance's random sequence.
    rng: u64,
    budget: u64,
    breakpoints: BTreeSet<u32>,
    paused: Option<Paused>,
    queue: VecDeque<Job>,
    single: bool,
    watched: Option<BTreeMap<(u32, u32), Value>>,
    trace: Option<Vec<u32>>,
    /// Events that can call themselves, and the parameters each unfinished
    /// call of one of them will get back when it ends.
    reentrant: Vec<bool>,
    frames: Vec<Vec<Value>>,
    mem: state_machine::MachineMemory,
}

struct Ctx<'a> {
    program: &'a Program,
    vars: &'a [Value],
    slots: &'a [Value],
    me: u64,
    time: f64,
    dt: f64,
    rng: &'a mut u64,
    watched: Option<&'a mut BTreeMap<(u32, u32), Value>>,
    mem: &'a state_machine::MachineMemory,
}

impl Ctx<'_> {
    fn eval(&mut self, src: &Src, host: &mut dyn Host) -> Result<Value, GraphError> {
        match src {
            Src::Const(v) => Ok(v.clone()),
            Src::Pin { node, pin } => self.pin(*node, *pin, host),
        }
    }

    fn pin(&mut self, node: u32, pin: u32, host: &mut dyn Host) -> Result<Value, GraphError> {
        let n = &self.program.nodes[node as usize];
        let v = if n.op.is_exec() {
            return Ok(self.slots[n.slots[pin as usize] as usize].clone());
        } else {
            match &n.op {
                Op::Const(v) => v.clone(),
                Op::Get(v) => self.vars[*v as usize].clone(),
                Op::SelfRef => Value::Entity(self.me),
                Op::GameTime => Value::Float(self.time),
                Op::FrameDt => Value::Float(self.dt),
                Op::GetState => {
                    Value::Str(state_machine::current_name(self.mem, self.program.machine.as_ref()))
                }
                Op::TimeInState => Value::Float(self.mem.time_in_state),
                Op::RandomInt => {
                    let lo = self.eval(&n.inputs[0], host)?.int();
                    let hi = self.eval(&n.inputs[1], host)?.int();
                    Value::Int(rt::random_int(self.rng, lo, hi))
                }
                Op::RandomFloat => {
                    let lo = self.eval(&n.inputs[0], host)?.float();
                    let hi = self.eval(&n.inputs[1], host)?.float();
                    Value::Float(rt::random_float(self.rng, lo, hi))
                }
                Op::WorldGet { component, field, ty } => {
                    let e = self.eval(&n.inputs[0], host)?.entity();
                    rt::world_get(host, e, component, field, *ty, n.id)?
                }
                Op::Exists => {
                    let e = self.eval(&n.inputs[0], host)?.entity();
                    Value::Bool(host.exists(e))
                }
                Op::FindAll { prefab } => rt::find_all(host, prefab),
                Op::BodiesInside => {
                    let e = self.eval(&n.inputs[0], host)?.entity();
                    rt::bodies_inside(host, e)
                }
                Op::FindFirst { prefab } => Value::Entity(rt::find_first(host, prefab)),
                Op::Phys(kind) => {
                    let mut args = Vec::new();
                    for src in &n.inputs {
                        args.push(self.eval(src, host)?);
                    }
                    crate::query::eval(*kind, pin, host, &args)
                }
                Op::RayHit { dist } => {
                    let f = self.eval(&n.inputs[0], host)?.vec3();
                    let d = self.eval(&n.inputs[1], host)?.vec3();
                    let m = self.eval(&n.inputs[2], host)?.float();
                    let hit = host.ray_cast(f, d, m);
                    if *dist {
                        Value::Float(hit.map_or(m, |h| h.1))
                    } else {
                        Value::Entity(hit.map_or(NO_ENTITY, |h| h.0))
                    }
                }
                Op::Player => {
                    let i = self.eval(&n.inputs[0], host)?.int();
                    Value::Entity(host.player(i))
                }
                Op::ActionPressed => {
                    let name = self.eval(&n.inputs[0], host)?.str();
                    Value::Bool(host.action_pressed(&name))
                }
                Op::ActionHeld => {
                    let name = self.eval(&n.inputs[0], host)?.str();
                    Value::Float(host.action_held(&name))
                }
                Op::ActionReleased => {
                    let name = self.eval(&n.inputs[0], host)?.str();
                    Value::Bool(host.action_released(&name))
                }
                Op::MouseDelta => {
                    let d = host.mouse_delta();
                    Value::Vec2(d)
                }
                Op::Device(kind) => kind.eval(host),
                Op::MouseWheel => Value::Float(host.mouse_wheel()),
                Op::PointerWorld => Value::Vec2(host.pointer_world()),
                Op::InputAxis => {
                    let name = self.eval(&n.inputs[0], host)?.str();
                    Value::Float(host.axis(&name))
                }
                Op::IsA { prefab } => {
                    let e = self.eval(&n.inputs[0], host)?.entity();
                    Value::Bool(host.is_a(e, prefab))
                }
                Op::AiArrived => {
                    let e = self.eval(&n.inputs[0], host)?.entity();
                    Value::Bool(host.ai_arrived(e))
                }
                Op::CharacterQuery(q) => {
                    let e = self.eval(&n.inputs[0], host)?.entity();
                    match q {
                        rt::CharacterQuery::OnFloor => Value::Bool(host.character_on_floor(e)),
                        rt::CharacterQuery::OnWall => Value::Bool(host.character_on_wall(e)),
                        rt::CharacterQuery::Velocity => Value::Vec2(host.character_velocity(e)),
                    }
                }
                Op::Mass2d => {
                    let e = self.eval(&n.inputs[0], host)?.entity();
                    Value::Float(rt::mass_2d(host, e))
                }
                Op::Server(crate::ServerCall::Position) => {
                    let mut args = Vec::new();
                    for src in &n.inputs {
                        args.push(self.eval(src, host)?);
                    }
                    rt::physics_server(host, "position", &args, n.id)?
                }
                Op::Score => {
                    let name = self.eval(&n.inputs[0], host)?.str();
                    Value::Int(host.score(&name))
                }
                Op::GameState => Value::Str(host.game_state()),
                Op::HasSave => {
                    let slot = self.eval(&n.inputs[0], host)?.str();
                    Value::Bool(host.has_save(&slot))
                }
                Op::Saved => {
                    let key = self.eval(&n.inputs[0], host)?.str();
                    Value::Str(host.saved(&key))
                }
                Op::SceneLoaded => {
                    let path = self.eval(&n.inputs[0], host)?.str();
                    Value::Bool(host.scene_loaded(&path))
                }
                Op::CurrentScene => Value::Str(host.current_scene()),
                Op::DataField => {
                    let path = self.eval(&n.inputs[0], host)?.str();
                    let field = self.eval(&n.inputs[1], host)?.str();
                    Value::Str(host.data_field(&path, &field))
                }
                Op::ReadCsvCell => {
                    let path = self.eval(&n.inputs[0], host)?.str();
                    let row = self.eval(&n.inputs[1], host)?.int();
                    let col = self.eval(&n.inputs[2], host)?.int();
                    Value::Str(host.read_csv_cell(&path, row, col))
                }
                Op::ReadJsonValue => {
                    let path = self.eval(&n.inputs[0], host)?.str();
                    let json_path = self.eval(&n.inputs[1], host)?.str();
                    Value::Str(host.read_json_value(&path, &json_path))
                }
                Op::ScriptCore(op) => {
                    let mut args = Vec::with_capacity(n.inputs.len());
                    for s in &n.inputs {
                        args.push(self.eval(s, host)?);
                    }
                    script_core::eval_core(*op, &args, host)
                }
                Op::UiValue => {
                    let name = self.eval(&n.inputs[0], host)?.str();
                    Value::Float(host.ui_value(&name))
                }
                Op::UiInput => {
                    let name = self.eval(&n.inputs[0], host)?.str();
                    Value::Str(host.ui_input(&name))
                }
                Op::DialogAnswer => {
                    if pin == 0 {
                        Value::Str(host.dialog_button())
                    } else {
                        Value::Str(host.dialog_text())
                    }
                }
                Op::CloudJob => match pin {
                    0 => Value::Str(host.cloud_kind()),
                    1 => Value::Str(host.cloud_slot()),
                    2 => Value::Int(host.cloud_saved_at()),
                    _ => Value::Str(host.cloud_body()),
                },
                Op::GetString => {
                    let table = self.eval(&n.inputs[0], host)?.str();
                    let key = self.eval(&n.inputs[1], host)?.str();
                    let values = self.eval(&n.inputs[2], host)?.str();
                    Value::Str(host.get_string(&table, &key, &values))
                }
                op => {
                    let mut args = Vec::with_capacity(n.inputs.len());
                    for s in &n.inputs {
                        args.push(self.eval(s, host)?);
                    }
                    apply_pure(op, &args)
                }
            }
        };
        if let Some(w) = self.watched.as_deref_mut() {
            w.insert((n.id, pin), v.clone());
        }
        Ok(v)
    }
}

impl Interp {
    /// A fresh instance running on entity `me` (its `to_bits` form).
    pub fn new(program: Program, me: u64) -> Self {
        let vars = program.vars.iter().map(|v| v.default.clone()).collect();
        let slots = program.slots.iter().map(|t| t.zero()).collect();
        let reentrant = program.reentrant();
        Self {
            program,
            me,
            vars,
            slots,
            timers: Timers::default(),
            time: 0.0,
            dt: 0.0,
            rng: rt::rng_seed(me),
            budget: DEFAULT_BUDGET,
            breakpoints: BTreeSet::new(),
            paused: None,
            queue: VecDeque::new(),
            single: false,
            watched: None,
            trace: None,
            reentrant,
            frames: Vec::new(),
            mem: state_machine::MachineMemory::default(),
        }
    }

    /// Name of the state machine's current state, or empty when it has not entered one.
    pub fn state_name(&self) -> String {
        state_machine::current_name(&self.mem, self.program.machine.as_ref())
    }

    pub fn program(&self) -> &Program {
        &self.program
    }

    /// Evaluate a pure node's primary output without running the graph.
    pub fn eval_pure(&mut self, node_id: u32, host: &mut dyn Host) -> Result<Value, GraphError> {
        let mut cx = Ctx {
            program: &self.program,
            vars: &self.vars,
            slots: &self.slots,
            me: self.me,
            time: self.time,
            dt: self.dt,
            rng: &mut self.rng,
            watched: None,
            mem: &self.mem,
        };
        cx.pin(node_id, 0, host)
    }

    pub fn var(&self, name: &str) -> Option<&Value> {
        self.program.var(name).map(|i| &self.vars[i])
    }

    /// Change a variable from the debugger. The type must match.
    pub fn set_var(&mut self, name: &str, value: Value) -> Result<(), String> {
        let i = self.program.var(name).ok_or_else(|| format!("no variable {name}"))?;
        if value.ty() != self.program.vars[i].ty {
            return Err(format!("{name} is {}, not {}", self.program.vars[i].ty, value.ty()));
        }
        self.vars[i] = value;
        Ok(())
    }

    pub fn set_breakpoint(&mut self, node_id: u32, on: bool) {
        if on {
            self.breakpoints.insert(node_id);
        } else {
            self.breakpoints.remove(&node_id);
        }
    }

    pub fn breakpoints(&self) -> impl Iterator<Item = u32> + '_ {
        self.breakpoints.iter().copied()
    }

    /// The node id the run is paused before, if it is paused.
    pub fn paused_at(&self) -> Option<u32> {
        let p = self.paused.as_ref()?;
        p.stack.last().map(|c| self.program.nodes[cont_node(*c) as usize].id)
    }

    /// The pending work of a paused run, innermost first, as node ids.
    pub fn call_stack(&self) -> Vec<u32> {
        match &self.paused {
            None => Vec::new(),
            Some(p) => p
                .stack
                .iter()
                .rev()
                .map(|c| self.program.nodes[cont_node(*c) as usize].id)
                .collect(),
        }
    }

    /// Record every value a data pin produces.
    pub fn watch(&mut self, on: bool) {
        self.watched = if on { Some(BTreeMap::new()) } else { None };
    }

    /// The last value of output `pin` of node `node_id` (needs [`Interp::watch`]).
    pub fn watched(&self, node_id: u32, pin: u32) -> Option<&Value> {
        self.watched.as_ref()?.get(&(node_id, pin))
    }

    /// Record the id of every node run.
    pub fn trace(&mut self, on: bool) {
        self.trace = if on { Some(Vec::new()) } else { None };
    }

    pub fn take_trace(&mut self) -> Vec<u32> {
        self.trace.as_mut().map(std::mem::take).unwrap_or_default()
    }

    /// Continue a paused run.
    pub fn resume(&mut self, host: &mut dyn Host) -> Result<RunState, GraphError> {
        let p = self.paused.take().ok_or_else(|| GraphError::general("not paused"))?;
        self.continue_run(p, host)
    }

    /// Run one step of a paused run, then pause again.
    pub fn step(&mut self, host: &mut dyn Host) -> Result<RunState, GraphError> {
        let p = self.paused.take().ok_or_else(|| GraphError::general("not paused"))?;
        self.single = true;
        self.continue_run(p, host)
    }

    /// Abandon a paused run and anything queued behind it.
    pub fn stop(&mut self) {
        self.paused = None;
        self.queue.clear();
        self.single = false;
    }

    /// Swap in an edited graph while playing. Variables with the same name
    /// and type keep their values; delays whose node still exists keep waiting.
    pub fn reload(&mut self, program: Program) -> ReloadReport {
        let mut report = ReloadReport::default();
        let mut vars = Vec::new();
        for v in &program.vars {
            match self.program.var(&v.name) {
                Some(i) if self.program.vars[i].ty == v.ty => {
                    report.kept_vars.push(v.name.clone());
                    vars.push(self.vars[i].clone());
                }
                _ => {
                    report.reset_vars.push(v.name.clone());
                    vars.push(v.default.clone());
                }
            }
        }
        let before = self.timers.len();
        let old = std::mem::replace(&mut self.program, program);
        state_machine::remap(old.machine.as_ref(), self.program.machine.as_ref(), &mut self.mem);
        let new = &self.program;
        self.timers.remap_events(|e| new.event(&old.events[e as usize].name).map(|k| k as u32));
        self.timers.retain_map(|t| {
            if t.event {
                return Some(*t);
            }
            let id = old.nodes[t.node as usize].id;
            let i = new.index_of(id)?;
            let n = &new.nodes[i as usize];
            match (&n.op, n.next[0]) {
                (Op::Delay, Some(target)) => Some(rt::Timer { node: i, target, ..*t }),
                _ => None,
            }
        });
        report.kept_delays = self.timers.len();
        report.dropped_delays = before - report.kept_delays;
        self.vars = vars;
        self.slots = self.program.slots.iter().map(|t| t.zero()).collect();
        self.paused = None;
        self.queue.clear();
        self.single = false;
        report
    }

    fn event_start(&self, ev: u32) -> Option<u32> {
        let info = self.program.events.get(ev as usize)?;
        self.program.nodes.get(info.node as usize)?.next.first().copied().flatten()
    }

    fn queue_switch(&mut self, plan: state_machine::Switch) {
        if let Some(ev) = plan.exit {
            self.queue.push_back(Job::Event(ev as usize, Vec::new()));
        }
        self.queue.push_back(Job::Commit { state: plan.index, forced: false, enter: plan.enter });
    }

    fn start(&mut self, job: Job) -> Result<Vec<Cont>, GraphError> {
        self.frames.clear();
        Ok(match job {
            Job::Resume(target) => vec![Cont::Exec(target)],
            Job::Timer(t) => {
                if !self.timers.live(&t) {
                    return Ok(Vec::new());
                }
                if self.program.machine.is_some() {
                    let name = self.program.events[t.target as usize].name.clone();
                    state_machine::note(&mut self.mem, &name);
                }
                let info = &self.program.events[t.target as usize];
                self.program.nodes[info.node as usize].next[0].map(Cont::Exec).into_iter().collect()
            }
            Job::Event(e, args) => {
                let info = &self.program.events[e];
                let node = &self.program.nodes[info.node as usize];
                for (slot, a) in node.slots.iter().zip(args) {
                    self.slots[*slot as usize] = a;
                }
                node.next[0].map(Cont::Exec).into_iter().collect()
            }
            Job::BeginMachine => {
                let enter_initial = state_machine::begin_step(&mut self.mem, self.dt);
                if let Some(machine) = self.program.machine.clone() {
                    if enter_initial {
                        let plan = state_machine::plan_index(&self.mem, &machine, machine.initial)?;
                        self.queue_switch(plan);
                    }
                    self.queue.push_back(Job::Update);
                    self.queue.push_back(Job::Finish);
                }
                Vec::new()
            }
            Job::Update => {
                let ev = self.program.machine.as_ref().and_then(|m| {
                    self.mem.current.and_then(|i| m.states.get(i as usize).and_then(|s| s.update))
                });
                ev.and_then(|e| self.event_start(e)).map(Cont::Exec).into_iter().collect()
            }
            Job::Commit { state, forced, enter } => {
                state_machine::commit(&mut self.mem, state, forced);
                enter.and_then(|e| self.event_start(e)).map(Cont::Exec).into_iter().collect()
            }
            Job::Finish => {
                if let Some(machine) = self.program.machine.clone() {
                    if let Some(plan) = state_machine::finish(&mut self.mem, &machine, &self.vars)?
                    {
                        self.queue_switch(plan);
                    }
                }
                Vec::new()
            }
        })
    }

    fn drain(&mut self, host: &mut dyn Host) -> Result<RunState, GraphError> {
        while let Some(job) = self.queue.pop_front() {
            let stack = match self.start(job) {
                Ok(stack) => stack,
                Err(e) => {
                    self.queue.clear();
                    return Err(e);
                }
            };
            let state = self.run(stack, Budget::new(self.budget), host, false);
            match state {
                Ok(RunState::Done) => {}
                Ok(paused) => return Ok(paused),
                Err(e) => {
                    self.queue.clear();
                    return Err(e);
                }
            }
        }
        Ok(RunState::Done)
    }

    fn continue_run(&mut self, p: Paused, host: &mut dyn Host) -> Result<RunState, GraphError> {
        match self.run(p.stack, p.budget, host, true) {
            Ok(RunState::Done) => self.drain(host),
            Ok(paused) => Ok(paused),
            Err(e) => {
                self.queue.clear();
                Err(e)
            }
        }
    }

    fn run(
        &mut self,
        mut stack: Vec<Cont>,
        mut budget: Budget,
        host: &mut dyn Host,
        mut skip_break: bool,
    ) -> Result<RunState, GraphError> {
        while let Some(&c) = stack.last() {
            let index = cont_node(c);
            let id = self.program.nodes[index as usize].id;
            if !skip_break && matches!(c, Cont::Exec(_)) && self.breakpoints.contains(&id) {
                self.single = false;
                self.paused = Some(Paused { stack, budget });
                return Ok(RunState::Paused(id));
            }
            skip_break = false;
            stack.pop();
            budget.step(id)?;
            if let Some(t) = &mut self.trace {
                t.push(id);
            }
            self.exec(c, &mut stack, host)?;
            if self.single {
                self.single = false;
                if let Some(&next) = stack.last() {
                    let id = self.program.nodes[cont_node(next) as usize].id;
                    self.paused = Some(Paused { stack, budget });
                    return Ok(RunState::Paused(id));
                }
            }
        }
        Ok(RunState::Done)
    }

    fn exec(
        &mut self,
        c: Cont,
        stack: &mut Vec<Cont>,
        host: &mut dyn Host,
    ) -> Result<(), GraphError> {
        if let Cont::Exec(i) = c {
            match &self.program.nodes[i as usize].op {
                Op::SetState => return self.exec_set_state(i, stack, host),
                Op::Trigger => return self.exec_trigger(i, stack, host),
                _ => {}
            }
        }
        let program = &self.program;
        let push = |stack: &mut Vec<Cont>, t: Option<u32>| {
            if let Some(t) = t {
                stack.push(Cont::Exec(t));
            }
        };
        match c {
            Cont::Loop { node, next, last } => {
                let n = &program.nodes[node as usize];
                let stop = next <= last && {
                    let mut cx = Ctx {
                        program,
                        vars: &self.vars,
                        slots: &self.slots,
                        me: self.me,
                        time: self.time,
                        dt: self.dt,
                        rng: &mut self.rng,
                        watched: self.watched.as_mut(),
                        mem: &self.mem,
                    };
                    cx.eval(&n.inputs[2], host)?.bool()
                };
                if next <= last && !stop {
                    self.slots[n.slots[0] as usize] = Value::Int(next);
                    if next < last {
                        stack.push(Cont::Loop { node, next: next + 1, last });
                    } else {
                        push(stack, n.next[1]);
                    }
                    push(stack, n.next[0]);
                } else {
                    push(stack, n.next[1]);
                }
            }
            Cont::Seq { node, next } => {
                let n = &program.nodes[node as usize];
                if (next as usize) < n.next.len() {
                    if (next as usize) + 1 < n.next.len() {
                        stack.push(Cont::Seq { node, next: next + 1 });
                    }
                    push(stack, n.next[next as usize]);
                }
            }
            Cont::Ret { node } => {
                let n = &program.nodes[node as usize];
                if let Op::Call(e) = n.op {
                    if self.reentrant[e as usize] {
                        let ev = &program.nodes[program.events[e as usize].node as usize];
                        let saved = self.frames.pop().unwrap_or_default();
                        for (slot, v) in ev.slots.iter().zip(saved) {
                            self.slots[*slot as usize] = v;
                        }
                    }
                }
                push(stack, n.next[0])
            }
            Cont::Commit { state, forced, enter, .. } => {
                let start = enter.and_then(|ev| {
                    let info = program.events.get(ev as usize)?;
                    program.nodes.get(info.node as usize)?.next.first().copied().flatten()
                });
                state_machine::commit(&mut self.mem, state, forced);
                if let Some(start) = start {
                    stack.push(Cont::Exec(start));
                }
            }
            Cont::Exec(i) => {
                let n = &program.nodes[i as usize];
                let mut cx = Ctx {
                    program,
                    vars: &self.vars,
                    slots: &self.slots,
                    me: self.me,
                    time: self.time,
                    dt: self.dt,
                    rng: &mut self.rng,
                    watched: self.watched.as_mut(),
                    mem: &self.mem,
                };
                match &n.op {
                    Op::Branch => {
                        let c = cx.eval(&n.inputs[0], host)?.bool();
                        push(stack, if c { n.next[0] } else { n.next[1] });
                    }
                    Op::Sequence(_) => stack.push(Cont::Seq { node: i, next: 0 }),
                    Op::For => {
                        let first = cx.eval(&n.inputs[0], host)?.int();
                        let last = cx.eval(&n.inputs[1], host)?.int();
                        stack.push(Cont::Loop { node: i, next: first, last });
                    }
                    Op::Delay => {
                        let s = cx.eval(&n.inputs[0], host)?.float();
                        if let Some(t) = n.next[0] {
                            self.timers.schedule(i, self.time + s, t);
                        }
                    }
                    Op::Set(v) => {
                        let val = cx.eval(&n.inputs[0], host)?;
                        self.vars[*v as usize] = val.clone();
                        self.slots[n.slots[0] as usize] = val;
                        push(stack, n.next[0]);
                    }
                    Op::Call(e) => {
                        let mut args = Vec::with_capacity(n.inputs.len());
                        for s in &n.inputs {
                            args.push(cx.eval(s, host)?);
                        }
                        let ev = &program.nodes[program.events[*e as usize].node as usize];
                        if self.reentrant[*e as usize] {
                            self.frames.push(
                                ev.slots.iter().map(|s| self.slots[*s as usize].clone()).collect(),
                            );
                        }
                        for (slot, a) in ev.slots.iter().zip(args) {
                            self.slots[*slot as usize] = a;
                        }
                        // Outputs read zero unless a `return` fills them.
                        for slot in &n.slots {
                            self.slots[*slot as usize] = program.slots[*slot as usize].zero();
                        }
                        stack.push(Cont::Ret { node: i });
                        push(stack, ev.next[0]);
                    }
                    Op::Return(_) => {
                        let mut vals = Vec::with_capacity(n.inputs.len());
                        for s in &n.inputs {
                            vals.push(cx.eval(s, host)?);
                        }
                        // Drop the rest of the called event (loops, sequences)
                        // back to the call that ran it, if any.
                        while let Some(&top) = stack.last() {
                            if let Cont::Ret { node } = top {
                                let call = &program.nodes[node as usize];
                                for (slot, v) in call.slots.iter().zip(vals) {
                                    self.slots[*slot as usize] = v;
                                }
                                break;
                            }
                            stack.pop();
                        }
                    }
                    Op::SetTimer(e) => {
                        let secs = cx.eval(&n.inputs[0], host)?.float();
                        let looping = cx.eval(&n.inputs[1], host)?.bool();
                        self.timers.set_event(i, *e, self.time, secs, looping);
                        push(stack, n.next[0]);
                    }
                    Op::ClearTimer(e) => {
                        self.timers.clear_event(*e);
                        push(stack, n.next[0]);
                    }
                    Op::Log => {
                        let m = cx.eval(&n.inputs[0], host)?.str();
                        host.log(&m);
                        push(stack, n.next[0]);
                    }
                    Op::WorldSet { component, field, .. } => {
                        let e = cx.eval(&n.inputs[0], host)?.entity();
                        let v = cx.eval(&n.inputs[1], host)?;
                        rt::world_set(host, e, component, field, v, n.id)?;
                        push(stack, n.next[0]);
                    }
                    Op::Spawn { prefab } => {
                        let e = rt::spawn(host, prefab, n.id)?;
                        self.slots[n.slots[0] as usize] = Value::Entity(e);
                        push(stack, n.next[0]);
                    }
                    Op::Despawn => {
                        let e = cx.eval(&n.inputs[0], host)?.entity();
                        rt::despawn(host, e, n.id)?;
                        push(stack, n.next[0]);
                    }
                    Op::PlaySequence => {
                        let p = cx.eval(&n.inputs[0], host)?.str();
                        rt::sequence(host, &p, n.id)?;
                        push(stack, n.next[0]);
                    }
                    Op::PlayClip => {
                        let e = cx.eval(&n.inputs[0], host)?.entity();
                        let clip = format!("clip:{}", cx.eval(&n.inputs[1], host)?.str());
                        rt::anim(host, e, Some(&clip), n.id)?;
                        push(stack, n.next[0]);
                    }
                    Op::PlayAnim | Op::StopAnim => {
                        let e = cx.eval(&n.inputs[0], host)?.entity();
                        let a = if matches!(n.op, Op::PlayAnim) {
                            Some(cx.eval(&n.inputs[1], host)?.str())
                        } else {
                            None
                        };
                        rt::anim(host, e, a.as_deref(), n.id)?;
                        push(stack, n.next[0]);
                    }
                    Op::Ai(kind) => {
                        let e = cx.eval(&n.inputs[0], host)?.entity();
                        let order = match kind {
                            rt::AiKind::MoveTo => rt::AiOrder::MoveTo {
                                target: cx.eval(&n.inputs[1], host)?.vec3(),
                                speed: cx.eval(&n.inputs[2], host)?.float(),
                            },
                            rt::AiKind::Chase => rt::AiOrder::Chase {
                                target: cx.eval(&n.inputs[1], host)?.entity(),
                                speed: cx.eval(&n.inputs[2], host)?.float(),
                            },
                            rt::AiKind::Patrol => rt::AiOrder::Patrol {
                                a: cx.eval(&n.inputs[1], host)?.vec3(),
                                b: cx.eval(&n.inputs[2], host)?.vec3(),
                                speed: cx.eval(&n.inputs[3], host)?.float(),
                            },
                            rt::AiKind::Stop => rt::AiOrder::Stop,
                        };
                        rt::ai_order(host, e, order, n.id)?;
                        push(stack, n.next[0]);
                    }
                    Op::Game(kind) => {
                        let cmd = match kind {
                            rt::GameKind::Possess => rt::GameCmd::Possess {
                                index: cx.eval(&n.inputs[0], host)?.int(),
                                entity: cx.eval(&n.inputs[1], host)?.entity(),
                            },
                            rt::GameKind::SpawnPlayer => rt::GameCmd::SpawnPlayer {
                                index: cx.eval(&n.inputs[0], host)?.int(),
                            },
                            rt::GameKind::AddScore => rt::GameCmd::AddScore {
                                name: cx.eval(&n.inputs[0], host)?.str(),
                                amount: cx.eval(&n.inputs[1], host)?.int(),
                            },
                            rt::GameKind::SetState => {
                                rt::GameCmd::SetState { state: cx.eval(&n.inputs[0], host)?.str() }
                            }
                            rt::GameKind::SaveGame => {
                                rt::GameCmd::SaveGame { slot: cx.eval(&n.inputs[0], host)?.str() }
                            }
                            rt::GameKind::LoadGame => {
                                rt::GameCmd::LoadGame { slot: cx.eval(&n.inputs[0], host)?.str() }
                            }
                            rt::GameKind::DeleteSave => {
                                rt::GameCmd::DeleteSave { slot: cx.eval(&n.inputs[0], host)?.str() }
                            }
                            rt::GameKind::SetSaved => rt::GameCmd::SetSaved {
                                key: cx.eval(&n.inputs[0], host)?.str(),
                                value: cx.eval(&n.inputs[1], host)?.str(),
                            },
                            rt::GameKind::OpenScene => {
                                rt::GameCmd::OpenScene { path: cx.eval(&n.inputs[0], host)?.str() }
                            }
                            rt::GameKind::AddScene => {
                                rt::GameCmd::AddScene { path: cx.eval(&n.inputs[0], host)?.str() }
                            }
                            rt::GameKind::LoadSceneAsync => rt::GameCmd::LoadSceneAsync {
                                path: cx.eval(&n.inputs[0], host)?.str(),
                                additive: cx.eval(&n.inputs[1], host)?.bool(),
                            },
                            rt::GameKind::UnloadScene => rt::GameCmd::UnloadScene {
                                path: cx.eval(&n.inputs[0], host)?.str(),
                            },
                            rt::GameKind::KeepAcrossScenes => rt::GameCmd::KeepAcrossScenes {
                                entity: cx.eval(&n.inputs[0], host)?.entity(),
                                keep: cx.eval(&n.inputs[1], host)?.bool(),
                            },
                            rt::GameKind::SetWindowMode => rt::GameCmd::SetWindowMode {
                                mode: cx.eval(&n.inputs[0], host)?.str(),
                            },
                            rt::GameKind::OpenGameWindow => rt::GameCmd::OpenGameWindow {
                                id: cx.eval(&n.inputs[0], host)?.int(),
                                title: cx.eval(&n.inputs[1], host)?.str(),
                                width: cx.eval(&n.inputs[2], host)?.int().max(0) as u32,
                                height: cx.eval(&n.inputs[3], host)?.int().max(0) as u32,
                                x: cx.eval(&n.inputs[4], host)?.int() as i32,
                                y: cx.eval(&n.inputs[5], host)?.int() as i32,
                                camera_entity: cx.eval(&n.inputs[6], host)?.entity(),
                                ui_screen: cx.eval(&n.inputs[7], host)?.str(),
                            },
                            rt::GameKind::MoveGameWindow => rt::GameCmd::MoveGameWindow {
                                id: cx.eval(&n.inputs[0], host)?.int().max(0) as u32,
                                x: cx.eval(&n.inputs[1], host)?.int() as i32,
                                y: cx.eval(&n.inputs[2], host)?.int() as i32,
                            },
                            rt::GameKind::ResizeGameWindow => rt::GameCmd::ResizeGameWindow {
                                id: cx.eval(&n.inputs[0], host)?.int().max(0) as u32,
                                width: cx.eval(&n.inputs[1], host)?.int().max(0) as u32,
                                height: cx.eval(&n.inputs[2], host)?.int().max(0) as u32,
                            },
                            rt::GameKind::CloseGameWindow => rt::GameCmd::CloseGameWindow {
                                id: cx.eval(&n.inputs[0], host)?.int().max(0) as u32,
                            },
                            rt::GameKind::RequestAttention => rt::GameCmd::RequestAttention,
                            rt::GameKind::Spectate => rt::GameCmd::Spectate {
                                entity: cx.eval(&n.inputs[0], host)?.entity(),
                                on: cx.eval(&n.inputs[1], host)?.bool(),
                            },
                            rt::GameKind::Unpossess => {
                                rt::GameCmd::Unpossess { index: cx.eval(&n.inputs[0], host)?.int() }
                            }
                        };
                        rt::game(host, cmd, n.id)?;
                        push(stack, n.next[0]);
                    }
                    Op::LoadData => {
                        let path = cx.eval(&n.inputs[0], host)?.str();
                        rt::load_data(host, &path, n.id)?;
                        push(stack, n.next[0]);
                    }
                    Op::ApplyImpulse => {
                        let e = cx.eval(&n.inputs[0], host)?.entity();
                        let v = cx.eval(&n.inputs[1], host)?.vec3();
                        rt::apply_impulse(host, e, v, n.id)?;
                        push(stack, n.next[0]);
                    }
                    Op::Character(kind) => {
                        let e = cx.eval(&n.inputs[0], host)?.entity();
                        match kind {
                            rt::CharacterKind::Move => {
                                let w = cx.eval(&n.inputs[1], host)?.vec2();
                                rt::character_move(host, e, w, n.id)?;
                            }
                            rt::CharacterKind::Jump => rt::character_jump(host, e, n.id)?,
                            rt::CharacterKind::SetMode => {
                                let mode = cx.eval(&n.inputs[1], host)?.str();
                                rt::character_mode(host, e, &mode, n.id)?;
                            }
                        }
                        push(stack, n.next[0]);
                    }
                    Op::ApplyForce2d => {
                        let e = cx.eval(&n.inputs[0], host)?.entity();
                        let f = cx.eval(&n.inputs[1], host)?.vec2();
                        rt::apply_force_2d(host, e, f, n.id)?;
                        push(stack, n.next[0]);
                    }
                    Op::ApplyTorque2d => {
                        let e = cx.eval(&n.inputs[0], host)?.entity();
                        let t = cx.eval(&n.inputs[1], host)?.float();
                        rt::apply_torque_2d(host, e, t, n.id)?;
                        push(stack, n.next[0]);
                    }
                    Op::SetVelocity2d => {
                        let e = cx.eval(&n.inputs[0], host)?.entity();
                        let v = cx.eval(&n.inputs[1], host)?.vec2();
                        let a = cx.eval(&n.inputs[2], host)?.float();
                        rt::set_velocity_2d(host, e, v, a, n.id)?;
                        push(stack, n.next[0]);
                    }
                    Op::SetColliderShape => {
                        let e = cx.eval(&n.inputs[0], host)?.entity();
                        let shape = cx.eval(&n.inputs[1], host)?.str();
                        let size = cx.eval(&n.inputs[2], host)?.vec2();
                        let points = cx.eval(&n.inputs[3], host)?.list();
                        rt::set_collider_shape(host, e, &shape, size, &points, n.id)?;
                        push(stack, n.next[0]);
                    }
                    Op::SetColliderEnabled => {
                        let e = cx.eval(&n.inputs[0], host)?.entity();
                        let on = cx.eval(&n.inputs[1], host)?.bool();
                        rt::set_collider_enabled(host, e, on, n.id)?;
                        push(stack, n.next[0]);
                    }
                    Op::SetJointTarget2d => {
                        let e = cx.eval(&n.inputs[0], host)?.entity();
                        let t = cx.eval(&n.inputs[1], host)?.vec2();
                        rt::set_joint_target_2d(host, e, t, n.id)?;
                        push(stack, n.next[0]);
                    }
                    Op::UiShow => {
                        let w = cx.eval(&n.inputs[0], host)?.str();
                        let v = cx.eval(&n.inputs[1], host)?.bool();
                        host.ui_show(&w, v).map_err(|m| GraphError::at(n.id, m))?;
                        push(stack, n.next[0]);
                    }
                    Op::UiText => {
                        let e = cx.eval(&n.inputs[0], host)?.str();
                        let t = cx.eval(&n.inputs[1], host)?.str();
                        host.ui_text(&e, &t).map_err(|m| GraphError::at(n.id, m))?;
                        push(stack, n.next[0]);
                    }
                    Op::Typewriter => {
                        let e = cx.eval(&n.inputs[0], host)?.str();
                        let skip = cx.eval(&n.inputs[1], host)?.bool();
                        host.typewriter(&e, skip).map_err(|m| GraphError::at(n.id, m))?;
                        push(stack, n.next[0]);
                    }
                    Op::CameraShake => {
                        let s = cx.eval(&n.inputs[0], host)?.float();
                        let d = cx.eval(&n.inputs[1], host)?.float();
                        rt::camera_shake(host, s, d, n.id)?;
                        push(stack, n.next[0]);
                    }
                    Op::Rumble => {
                        let s = cx.eval(&n.inputs[0], host)?.float();
                        let w = cx.eval(&n.inputs[1], host)?.float();
                        let d = cx.eval(&n.inputs[2], host)?.float();
                        rt::rumble(host, s, w, d, n.id)?;
                        push(stack, n.next[0]);
                    }
                    Op::Device(crate::device_nodes::DeviceKind::SetLight) => {
                        let r = cx.eval(&n.inputs[0], host)?.float();
                        let g = cx.eval(&n.inputs[1], host)?.float();
                        let b = cx.eval(&n.inputs[2], host)?.float();
                        rt::set_pad_light(host, r, g, b, n.id)?;
                        push(stack, n.next[0]);
                    }
                    Op::RebindAction => {
                        let a = cx.eval(&n.inputs[0], host)?.str();
                        let slot = cx.eval(&n.inputs[1], host)?.int();
                        rt::rebind_action(host, &a, slot, n.id)?;
                        push(stack, n.next[0]);
                    }
                    Op::PlayMusic => {
                        let file = cx.eval(&n.inputs[0], host)?.str();
                        let looping = cx.eval(&n.inputs[1], host)?.bool();
                        let fade = cx.eval(&n.inputs[2], host)?.float();
                        let start = cx.eval(&n.inputs[3], host)?.float();
                        rt::play_music(host, &file, looping, fade, start, n.id)?;
                        push(stack, n.next[0]);
                    }
                    Op::StopMusic => {
                        let fade = cx.eval(&n.inputs[0], host)?.float();
                        rt::stop_music(host, fade, n.id)?;
                        push(stack, n.next[0]);
                    }
                    Op::SetMusicVolume => {
                        let volume = cx.eval(&n.inputs[0], host)?.float();
                        rt::set_music_volume(host, volume, n.id)?;
                        push(stack, n.next[0]);
                    }
                    Op::OpenFileDialog => {
                        let title = cx.eval(&n.inputs[0], host)?.str();
                        let filter = cx.eval(&n.inputs[1], host)?.str();
                        let path = host.open_file_dialog(&title, &filter).map_err(|m| GraphError::at(n.id, m))?;
                        self.slots[n.slots[0] as usize] = Value::Str(path);
                        push(stack, n.next[0]);
                    }
                    Op::SaveFileDialog => {
                        let title = cx.eval(&n.inputs[0], host)?.str();
                        let name = cx.eval(&n.inputs[1], host)?.str();
                        let path = host.save_file_dialog(&title, &name).map_err(|m| GraphError::at(n.id, m))?;
                        self.slots[n.slots[0] as usize] = Value::Str(path);
                        push(stack, n.next[0]);
                    }
                    Op::Encrypt => {
                        let text = cx.eval(&n.inputs[0], host)?.str();
                        let out = host.encrypt_text(&text).map_err(|m| GraphError::at(n.id, m))?;
                        self.slots[n.slots[0] as usize] = Value::Str(out);
                        push(stack, n.next[0]);
                    }
                    Op::Decrypt => {
                        let data = cx.eval(&n.inputs[0], host)?.str();
                        let out = host.decrypt_text(&data).map_err(|m| GraphError::at(n.id, m))?;
                        self.slots[n.slots[0] as usize] = Value::Str(out);
                        push(stack, n.next[0]);
                    }
                    Op::ShowDialog => {
                        let kind = cx.eval(&n.inputs[0], host)?.str();
                        let title = cx.eval(&n.inputs[1], host)?.str();
                        let message = cx.eval(&n.inputs[2], host)?.str();
                        let placeholder = cx.eval(&n.inputs[3], host)?.str();
                        let theme = cx.eval(&n.inputs[4], host)?.str();
                        host.show_dialog(&kind, &title, &message, &placeholder, &theme)
                            .map_err(|m| GraphError::at(n.id, m))?;
                        push(stack, n.next[0]);
                    }
                    Op::CloudUpload => {
                        let slot = cx.eval(&n.inputs[0], host)?.str();
                        host.cloud_upload(&slot).map_err(|m| GraphError::at(n.id, m))?;
                        push(stack, n.next[0]);
                    }
                    Op::CloudDownload => {
                        let slot = cx.eval(&n.inputs[0], host)?.str();
                        host.cloud_download(&slot).map_err(|m| GraphError::at(n.id, m))?;
                        push(stack, n.next[0]);
                    }
                    Op::CloudFlush => {
                        host.cloud_flush().map_err(|m| GraphError::at(n.id, m))?;
                        push(stack, n.next[0]);
                    }
                    Op::CloudReply => {
                        let slot = cx.eval(&n.inputs[0], host)?.str();
                        let saved_at = cx.eval(&n.inputs[1], host)?.int();
                        let body = cx.eval(&n.inputs[2], host)?.str();
                        host.cloud_reply(&slot, saved_at, &body).map_err(|m| GraphError::at(n.id, m))?;
                        push(stack, n.next[0]);
                    }
                    Op::PlaySound { at } => {
                        let s = cx.eval(&n.inputs[0], host)?.str();
                        let v = cx.eval(&n.inputs[1], host)?.float();
                        let p = cx.eval(&n.inputs[2], host)?.float();
                        let l = if *at { Some(cx.eval(&n.inputs[3], host)?.vec3()) } else { None };
                        rt::play_sound(host, &s, v, p, l, n.id)?;
                        push(stack, n.next[0]);
                    }
                    Op::Server(call) => {
                        let mut args = Vec::new();
                        for src in &n.inputs {
                            args.push(cx.eval(src, host)?);
                        }
                        let value = rt::physics_server(host, call.name(), &args, n.id)?;
                        if let Some(slot) = n.slots.first() {
                            self.slots[*slot as usize] = value;
                        }
                        push(stack, n.next[0]);
                    }
                    _ => unreachable!("only execution nodes are pushed"),
                }
            }
        }
        Ok(())
    }

    fn exec_set_state(
        &mut self,
        i: u32,
        stack: &mut Vec<Cont>,
        host: &mut dyn Host,
    ) -> Result<(), GraphError> {
        let (name, next, id) = {
            let n = &self.program.nodes[i as usize];
            let mut cx = Ctx {
                program: &self.program,
                vars: &self.vars,
                slots: &self.slots,
                me: self.me,
                time: self.time,
                dt: self.dt,
                rng: &mut self.rng,
                watched: self.watched.as_mut(),
                mem: &self.mem,
            };
            let name = cx.eval(&n.inputs[0], host)?.str();
            (name, n.next[0], n.id)
        };
        let machine = self
            .program
            .machine
            .clone()
            .ok_or_else(|| GraphError::at(id, "this graph has no state machine"))?;
        let plan = state_machine::plan_name(&self.mem, &machine, &name, id)?;
        if let Some(t) = next {
            stack.push(Cont::Exec(t));
        }
        stack.push(Cont::Commit { node: i, state: plan.index, forced: true, enter: plan.enter });
        if let Some(ev) = plan.exit {
            if let Some(start) = self.event_start(ev) {
                stack.push(Cont::Exec(start));
            }
        }
        Ok(())
    }

    fn exec_trigger(
        &mut self,
        i: u32,
        stack: &mut Vec<Cont>,
        host: &mut dyn Host,
    ) -> Result<(), GraphError> {
        let (name, next) = {
            let n = &self.program.nodes[i as usize];
            let mut cx = Ctx {
                program: &self.program,
                vars: &self.vars,
                slots: &self.slots,
                me: self.me,
                time: self.time,
                dt: self.dt,
                rng: &mut self.rng,
                watched: self.watched.as_mut(),
                mem: &self.mem,
            };
            (cx.eval(&n.inputs[0], host)?.str(), n.next[0])
        };
        state_machine::note(&mut self.mem, &name);
        if let Some(t) = next {
            stack.push(Cont::Exec(t));
        }
        Ok(())
    }
}

pub(crate) fn cont_node(c: Cont) -> u32 {
    match c {
        Cont::Exec(n)
        | Cont::Loop { node: n, .. }
        | Cont::Seq { node: n, .. }
        | Cont::Ret { node: n }
        | Cont::Commit { node: n, .. } => n,
    }
}

impl GraphInstance for Interp {
    fn fire(
        &mut self,
        event: &str,
        args: &[Value],
        host: &mut dyn Host,
    ) -> Result<RunState, GraphError> {
        if self.paused.is_some() {
            return Err(GraphError::general("paused at a breakpoint; resume, step or stop first"));
        }
        let Some(e) = self.program.event(event) else {
            if self.program.machine.is_some() {
                state_machine::note(&mut self.mem, event);
            }
            return Ok(RunState::Done);
        };
        let params: Vec<_> = self.program.events[e].params.iter().map(|p| p.1).collect();
        rt::check_args(event, &params, args)?;
        if self.program.machine.is_some() {
            state_machine::note(&mut self.mem, event);
        }
        self.queue.push_back(Job::Event(e, args.to_vec()));
        self.drain(host)
    }

    fn advance(&mut self, dt: f64, host: &mut dyn Host) -> Result<RunState, GraphError> {
        if self.paused.is_some() {
            return Err(GraphError::general("paused at a breakpoint; resume, step or stop first"));
        }
        self.time += dt;
        self.dt = dt;
        for t in self.timers.take_due(self.time) {
            self.queue.push_back(if t.event { Job::Timer(t) } else { Job::Resume(t.target) });
        }
        if let Some(e) = self.program.event("tick") {
            if self.program.machine.is_some() {
                state_machine::note(&mut self.mem, "tick");
            }
            self.queue.push_back(Job::Event(e, vec![Value::Float(dt)]));
        }
        if self.program.machine.is_some() {
            self.queue.push_back(Job::BeginMachine);
        }
        self.drain(host)
    }

    fn vars(&self) -> Vec<(String, Value)> {
        self.program.vars.iter().zip(&self.vars).map(|(d, v)| (d.name.clone(), v.clone())).collect()
    }

    fn time(&self) -> f64 {
        self.time
    }

    fn pending_delays(&self) -> usize {
        self.timers.len()
    }

    fn set_budget(&mut self, steps: u64) {
        self.budget = steps;
    }
}
