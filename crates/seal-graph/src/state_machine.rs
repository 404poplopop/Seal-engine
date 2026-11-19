//! A state machine stored on a logic graph.
//!
//! One step, after the frame's `tick` event, does this in order: enter the
//! initial state on the first step, run On Update, then take at most one
//! transition. Higher priority wins; equal priorities keep file order. A
//! transition onto the current state runs On Exit and On Enter again.
//! `set_state` switches immediately and suppresses that step's transition.
//! The interpreter and the compiled graph both call the functions here.

use crate::rt::{self, GraphError, Ty, Value};
use crate::{is_ident, BuildError, CmpOp, EventInfo, VarDef};
use seal_sealtext::{SealMap, SealValue};

pub const PHASES: [&str; 3] = ["on_enter", "on_update", "on_exit"];

/// Event name for one phase of a state, such as `on_enter_idle`.
pub fn phase_event(phase: &str, state: &str) -> String {
    format!("{phase}_{state}")
}

/// A name that can be a state, variable or event: a letter or `_`, then letters, digits and `_`.
pub fn plain_name(name: &str) -> bool {
    is_ident(name)
}

/// How a transition decides to fire.
#[derive(Clone, Debug, PartialEq)]
pub enum Cond {
    /// A bool variable is true.
    BoolVar(String),
    /// Compare a variable to a literal.
    Compare { var: String, op: CmpOp, value: Value },
    /// An event name noted this step (a trigger, a timer, `tick`, or an outside fire).
    Event(String),
    /// At least this many seconds have passed in the current state.
    After(f64),
}

/// One state as saved. `pos` is the box on the state canvas.
#[derive(Clone, Debug, PartialEq)]
pub struct StateDef {
    pub name: String,
    pub pos: [f32; 2],
}

/// One transition as saved.
#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    pub from: String,
    pub to: String,
    pub priority: i32,
    pub cond: Cond,
}

/// The state machine saved in a graph file.
#[derive(Clone, Debug, PartialEq)]
pub struct Machine {
    pub initial: String,
    pub states: Vec<StateDef>,
    pub transitions: Vec<Transition>,
}

/// A condition with variable indexes resolved.
#[derive(Clone, Debug, PartialEq)]
pub enum CondProg {
    BoolVar(u32),
    Compare { var: u32, op: CmpOp, value: Value },
    Event(String),
    After(f64),
}

/// A state with the graph events it runs. `None` when that event node is absent.
#[derive(Clone, Debug, PartialEq)]
pub struct StateProg {
    pub name: String,
    pub enter: Option<u32>,
    pub update: Option<u32>,
    pub exit: Option<u32>,
}

/// A transition ready to test.
#[derive(Clone, Debug, PartialEq)]
pub struct TransProg {
    pub from: u32,
    pub to: u32,
    pub priority: i32,
    pub cond: CondProg,
}

/// A checked machine.
#[derive(Clone, Debug, PartialEq)]
pub struct MachineProg {
    pub initial: u32,
    pub states: Vec<StateProg>,
    pub transitions: Vec<TransProg>,
}

/// What one switch will run. On Exit still sees the old state; commit, then On Enter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Switch {
    pub index: u32,
    pub exit: Option<u32>,
    pub enter: Option<u32>,
}

/// Per-instance machine status. Not saved with the graph.
#[derive(Clone, Debug, PartialEq)]
pub struct MachineMemory {
    pub current: Option<u32>,
    pub time_in_state: f64,
    /// Event names noted since the last transition check, first mention wins.
    pub triggered: Vec<String>,
    /// `set_state` already changed state; the step must not also take a transition.
    pub suppress: bool,
    pub started: bool,
}

impl Default for MachineMemory {
    fn default() -> Self {
        Self {
            current: None,
            time_in_state: 0.0,
            triggered: Vec::new(),
            suppress: false,
            started: false,
        }
    }
}

impl Machine {
    pub fn to_seal(&self) -> SealValue {
        let states = self
            .states
            .iter()
            .map(|s| {
                SealValue::Map(SealMap::new(vec![
                    ("name".into(), SealValue::String(s.name.clone())),
                    ("x".into(), pos_num(s.pos[0])),
                    ("y".into(), pos_num(s.pos[1])),
                ]))
            })
            .collect();
        let transitions = self.transitions.iter().map(Transition::to_seal).collect();
        SealValue::Map(SealMap::new(vec![
            ("initial".into(), SealValue::String(self.initial.clone())),
            ("states".into(), SealValue::Array(states)),
            ("transitions".into(), SealValue::Array(transitions)),
        ]))
    }

    /// Read a machine. `vars` is the graph's variable list, used to check conditions.
    pub fn from_seal(v: &SealValue, vars: &[VarDef]) -> Result<Self, BuildError> {
        let bad = |m: &str| BuildError::general(m.to_string());
        if v.as_map().is_none() {
            return Err(bad("state machine must be a map"));
        }
        let initial = match v.get("initial") {
            Some(SealValue::String(s)) if plain_name(s) => s.clone(),
            Some(SealValue::String(s)) => {
                return Err(bad(&format!("initial state `{s}` must be a plain name")))
            }
            _ => return Err(bad("state machine needs an initial state name")),
        };
        let states_v = match v.get("states") {
            Some(SealValue::Array(a)) => a,
            _ => return Err(bad("state machine needs a states list")),
        };
        if states_v.is_empty() {
            return Err(bad("state machine has no states"));
        }
        let mut states = Vec::new();
        for (i, item) in states_v.iter().enumerate() {
            let Some(name) = item.get("name").and_then(as_str) else {
                return Err(bad(&format!("state {i} has no name")));
            };
            if !plain_name(name) {
                return Err(bad(&format!("state name `{name}` must be a plain name")));
            }
            if states.iter().any(|s: &StateDef| s.name == name) {
                return Err(bad(&format!("duplicate state {name}")));
            }
            let x = match item.get("x") {
                None => 0.0,
                Some(n) => read_pos(n).ok_or_else(|| bad(&format!("state {name} has a bad x")))?,
            };
            let y = match item.get("y") {
                None => 0.0,
                Some(n) => read_pos(n).ok_or_else(|| bad(&format!("state {name} has a bad y")))?,
            };
            states.push(StateDef { name: name.to_string(), pos: [x, y] });
        }
        if !states.iter().any(|s| s.name == initial) {
            return Err(bad(&format!("initial state {initial} is not in the machine")));
        }
        let transitions_v = match v.get("transitions") {
            None => Vec::new(),
            Some(SealValue::Array(a)) => a.clone(),
            Some(_) => return Err(bad("transitions must be a list")),
        };
        let mut transitions = Vec::new();
        for (i, item) in transitions_v.iter().enumerate() {
            transitions.push(Transition::from_seal(item, i, &states, vars)?);
        }
        Ok(Machine { initial, states, transitions })
    }
}

impl Transition {
    fn to_seal(&self) -> SealValue {
        let mut entries = vec![
            ("from".into(), SealValue::String(self.from.clone())),
            ("to".into(), SealValue::String(self.to.clone())),
            ("priority".into(), SealValue::Number(self.priority.to_string())),
        ];
        match &self.cond {
            Cond::BoolVar(var) => {
                entries.push(("cond".into(), SealValue::String("flag".into())));
                entries.push(("var".into(), SealValue::String(var.clone())));
            }
            Cond::Compare { var, op, value } => {
                entries.push(("cond".into(), SealValue::String("compare".into())));
                entries.push(("var".into(), SealValue::String(var.clone())));
                entries.push(("op".into(), SealValue::String(op.name().into())));
                entries.push(("type".into(), SealValue::String(value.ty().name().into())));
                entries.push(("value".into(), crate::value_to_text(value)));
            }
            Cond::Event(name) => {
                entries.push(("cond".into(), SealValue::String("event".into())));
                entries.push(("event".into(), SealValue::String(name.clone())));
            }
            Cond::After(seconds) => {
                entries.push(("cond".into(), SealValue::String("after".into())));
                entries.push(("seconds".into(), crate::value_to_text(&Value::Float(*seconds))));
            }
        }
        SealValue::Map(SealMap::new(entries))
    }

    fn from_seal(
        v: &SealValue,
        index: usize,
        states: &[StateDef],
        vars: &[VarDef],
    ) -> Result<Self, BuildError> {
        let bad = |m: String| BuildError::general(m);
        let at = |m: &str| bad(format!("transition {index}: {m}"));
        if v.as_map().is_none() {
            return Err(at("must be a map"));
        }
        let from = v.get("from").and_then(as_str).ok_or_else(|| at("needs from"))?.to_string();
        let to = v.get("to").and_then(as_str).ok_or_else(|| at("needs to"))?.to_string();
        if !states.iter().any(|s| s.name == from) {
            return Err(at(&format!("unknown state {from}")));
        }
        if !states.iter().any(|s| s.name == to) {
            return Err(at(&format!("unknown state {to}")));
        }
        let priority = match v.get("priority").and_then(as_num) {
            Some(n) => n.parse::<i32>().map_err(|_| at("priority must be a whole number"))?,
            None => return Err(at("needs a priority")),
        };
        let kind = v.get("cond").and_then(as_str).ok_or_else(|| at("needs a condition"))?;
        let cond = match kind {
            "flag" => {
                let var =
                    v.get("var").and_then(as_str).ok_or_else(|| at("flag needs a variable"))?;
                match vars.iter().find(|d| d.name == var) {
                    Some(d) if d.ty == Ty::Bool => Cond::BoolVar(var.to_string()),
                    Some(_) => return Err(at(&format!("{var} is not a flag"))),
                    None => return Err(at(&format!("no variable {var}"))),
                }
            }
            "compare" => {
                let var =
                    v.get("var").and_then(as_str).ok_or_else(|| at("compare needs a variable"))?;
                let def = vars
                    .iter()
                    .find(|d| d.name == var)
                    .ok_or_else(|| at(&format!("no variable {var}")))?;
                let op_name =
                    v.get("op").and_then(as_str).ok_or_else(|| at("compare needs an op"))?;
                let op = CmpOp::parse(op_name)
                    .ok_or_else(|| at(&format!("unknown compare op {op_name}")))?;
                if matches!(op, CmpOp::Lt | CmpOp::Le | CmpOp::Gt | CmpOp::Ge)
                    && !matches!(def.ty, Ty::Int | Ty::Float)
                {
                    return Err(at("ordered compare works on int or float"));
                }
                if def.ty.is_list() {
                    return Err(at("compare works on plain values"));
                }
                let raw = v.get("value").ok_or_else(|| at("compare needs a value"))?;
                let value = crate::value_from_text(def.ty, raw)
                    .ok_or_else(|| at(&format!("value is not a {}", def.ty)))?;
                Cond::Compare { var: var.to_string(), op, value }
            }
            "event" => {
                let name = v
                    .get("event")
                    .and_then(as_str)
                    .ok_or_else(|| at("event condition needs a name"))?;
                if name.is_empty() || name.chars().any(char::is_whitespace) {
                    return Err(at("event name cannot be empty or contain spaces"));
                }
                Cond::Event(name.to_string())
            }
            "after" => {
                let raw = v.get("seconds").ok_or_else(|| at("after needs seconds"))?;
                let seconds = crate::value_from_text(Ty::Float, raw)
                    .map(|v| v.float())
                    .ok_or_else(|| at("seconds must be a number"))?;
                if !seconds.is_finite() || seconds < 0.0 {
                    return Err(at("seconds must be zero or more"));
                }
                Cond::After(seconds)
            }
            other => return Err(at(&format!("unknown condition {other}"))),
        };
        Ok(Transition { from, to, priority, cond })
    }
}

fn as_str(v: &SealValue) -> Option<&str> {
    match v {
        SealValue::String(s) => Some(s),
        _ => None,
    }
}

fn as_num(v: &SealValue) -> Option<&str> {
    match v {
        SealValue::Number(n) => Some(n),
        _ => None,
    }
}

fn pos_num(v: f32) -> SealValue {
    let v = (v * 10.0).round() / 10.0;
    SealValue::Number(format!("{v:.1}"))
}

fn read_pos(v: &SealValue) -> Option<f32> {
    let n: f64 = as_num(v)?.parse().ok()?;
    n.is_finite().then_some(n as f32)
}

/// Resolve `graph.machine` against its variables and events.
pub fn compile(
    graph: &crate::Graph,
    events: &[EventInfo],
) -> Result<Option<MachineProg>, Vec<BuildError>> {
    let Some(machine) = &graph.machine else { return Ok(None) };
    let mut errors = Vec::new();
    let mut states = Vec::new();
    for state in &machine.states {
        if !plain_name(&state.name) {
            errors.push(BuildError::general(format!(
                "state name `{}` must be a plain name",
                state.name
            )));
        }
        if states.iter().any(|s: &StateProg| s.name == state.name) {
            errors.push(BuildError::general(format!("duplicate state {}", state.name)));
        }
        let mut slot = |phase: &str| -> Option<u32> {
            let name = phase_event(phase, &state.name);
            match events.iter().position(|e| e.name == name) {
                None => None,
                Some(i) if events[i].params.is_empty() && events[i].returns.is_empty() => {
                    Some(i as u32)
                }
                Some(_) => {
                    errors.push(BuildError::general(format!(
                        "state event {name} cannot take parameters or return values"
                    )));
                    None
                }
            }
        };
        states.push(StateProg {
            name: state.name.clone(),
            enter: slot("on_enter"),
            update: slot("on_update"),
            exit: slot("on_exit"),
        });
    }
    let index = |name: &str| states.iter().position(|s| s.name == name);
    let initial = match index(&machine.initial) {
        Some(i) => i as u32,
        None => {
            errors.push(BuildError::general(format!(
                "initial state {} is not in the machine",
                machine.initial
            )));
            0
        }
    };
    let mut transitions = Vec::new();
    for (n, t) in machine.transitions.iter().enumerate() {
        let (Some(from), Some(to)) = (index(&t.from), index(&t.to)) else {
            errors.push(BuildError::general(format!(
                "transition {n} names a state that is not in the machine"
            )));
            continue;
        };
        let cond = match &t.cond {
            Cond::BoolVar(var) => {
                match graph.vars.iter().position(|v| v.name == *var && v.ty == Ty::Bool) {
                    Some(i) => CondProg::BoolVar(i as u32),
                    None => {
                        errors.push(BuildError::general(format!(
                            "transition {n} needs a flag variable {var}"
                        )));
                        continue;
                    }
                }
            }
            Cond::Compare { var, op, value } => {
                match graph.vars.iter().position(|v| v.name == *var && v.ty == value.ty()) {
                    Some(i) => CondProg::Compare { var: i as u32, op: *op, value: value.clone() },
                    None => {
                        errors.push(BuildError::general(format!(
                            "transition {n} compares missing variable {var}"
                        )));
                        continue;
                    }
                }
            }
            Cond::Event(name) => CondProg::Event(name.clone()),
            Cond::After(seconds) => CondProg::After(*seconds),
        };
        transitions.push(TransProg {
            from: from as u32,
            to: to as u32,
            priority: t.priority,
            cond,
        });
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(Some(MachineProg { initial, states, transitions }))
}

/// Remember an event name until the next transition check.
pub fn note(mem: &mut MachineMemory, name: &str) {
    if name.is_empty() || mem.triggered.iter().any(|n| n == name) {
        return;
    }
    mem.triggered.push(name.to_string());
}

/// Advance the clock. Returns whether this is the first step and the initial state is not entered yet.
pub fn begin_step(mem: &mut MachineMemory, dt: f64) -> bool {
    let enter_initial = !mem.started && mem.current.is_none();
    if mem.started && !mem.suppress {
        mem.time_in_state += dt;
    }
    mem.started = true;
    enter_initial
}

/// Plan a switch to `index`. On Exit is omitted when nothing is current yet.
pub fn plan_index(
    mem: &MachineMemory,
    machine: &MachineProg,
    index: u32,
) -> Result<Switch, GraphError> {
    let state = machine
        .states
        .get(index as usize)
        .ok_or_else(|| GraphError::general(format!("unknown state index {index}")))?;
    let exit = if mem.current.is_some() {
        mem.current.and_then(|c| machine.states.get(c as usize).and_then(|s| s.exit))
    } else {
        None
    };
    Ok(Switch { index, exit, enter: state.enter })
}

/// Plan a switch to the state named `name`. An unknown name is an error.
pub fn plan_name(
    mem: &MachineMemory,
    machine: &MachineProg,
    name: &str,
    node: u32,
) -> Result<Switch, GraphError> {
    let index = machine
        .states
        .iter()
        .position(|s| s.name == name)
        .ok_or_else(|| GraphError::at(node, format!("unknown state {name}")))?
        as u32;
    plan_index(mem, machine, index)
}

/// Adopt `index` and reset the time in state. `forced` blocks this step's automatic transition.
pub fn commit(mem: &mut MachineMemory, index: u32, forced: bool) {
    mem.current = Some(index);
    mem.time_in_state = 0.0;
    mem.started = true;
    if forced {
        mem.suppress = true;
    }
}

/// Pick at most one transition out of the current state, then clear noted events.
pub fn finish(
    mem: &mut MachineMemory,
    machine: &MachineProg,
    vars: &[Value],
) -> Result<Option<Switch>, GraphError> {
    if mem.suppress {
        mem.suppress = false;
        mem.triggered.clear();
        return Ok(None);
    }
    let Some(cur) = mem.current else {
        mem.triggered.clear();
        return Ok(None);
    };
    let mut best: Option<(i32, usize, u32)> = None;
    for (i, t) in machine.transitions.iter().enumerate() {
        if t.from != cur || !holds(&t.cond, vars, mem.time_in_state, &mem.triggered) {
            continue;
        }
        let take = match best {
            None => true,
            Some((priority, index, _)) => {
                t.priority > priority || (t.priority == priority && i < index)
            }
        };
        if take {
            best = Some((t.priority, i, t.to));
        }
    }
    mem.triggered.clear();
    match best {
        Some((_, _, to)) => Ok(Some(plan_index(mem, machine, to)?)),
        None => Ok(None),
    }
}

fn holds(cond: &CondProg, vars: &[Value], time: f64, triggered: &[String]) -> bool {
    match cond {
        CondProg::BoolVar(i) => matches!(vars.get(*i as usize), Some(Value::Bool(true))),
        CondProg::Compare { var, op, value } => {
            let Some(have) = vars.get(*var as usize) else { return false };
            have.ty() == value.ty() && compare_values(*op, have, value)
        }
        CondProg::Event(name) => triggered.iter().any(|n| n == name),
        CondProg::After(seconds) => time >= *seconds,
    }
}

fn compare_values(op: CmpOp, a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(a), Value::Int(b)) => ordered(op, *a, *b),
        (Value::Float(a), Value::Float(b)) => ordered(op, *a, *b),
        (Value::Bool(a), Value::Bool(b)) => equality(op, *a, *b),
        (Value::Str(a), Value::Str(b)) => equality(op, a, b),
        (Value::Entity(a), Value::Entity(b)) => equality(op, *a, *b),
        (Value::Vec2(a), Value::Vec2(b)) => equality(op, *a, *b),
        (Value::Vec3(a), Value::Vec3(b)) => equality(op, *a, *b),
        _ => false,
    }
}

fn ordered<T: PartialOrd + PartialEq>(op: CmpOp, a: T, b: T) -> bool {
    match op {
        CmpOp::Lt => rt::ops::lt(a, b),
        CmpOp::Le => rt::ops::le(a, b),
        CmpOp::Gt => rt::ops::gt(a, b),
        CmpOp::Ge => rt::ops::ge(a, b),
        CmpOp::Eq => rt::ops::eq(a, b),
        CmpOp::Ne => rt::ops::ne(a, b),
    }
}

fn equality<T: PartialEq>(op: CmpOp, a: T, b: T) -> bool {
    match op {
        CmpOp::Eq => rt::ops::eq(a, b),
        CmpOp::Ne => rt::ops::ne(a, b),
        _ => false,
    }
}

/// The current state's name, or empty when the machine has not entered one.
pub fn current_name(mem: &MachineMemory, machine: Option<&MachineProg>) -> String {
    let Some(machine) = machine else { return String::new() };
    mem.current
        .and_then(|i| machine.states.get(i as usize).map(|s| s.name.clone()))
        .unwrap_or_default()
}

/// Keep the current state across a reload when its name still exists.
pub fn remap(old: Option<&MachineProg>, new: Option<&MachineProg>, mem: &mut MachineMemory) {
    let Some(new) = new else {
        *mem = MachineMemory::default();
        return;
    };
    let Some(old) = old else { return };
    let name = mem.current.and_then(|i| old.states.get(i as usize).map(|s| s.name.clone()));
    match name.and_then(|n| new.states.iter().position(|s| s.name == n)) {
        Some(i) => mem.current = Some(i as u32),
        None => {
            mem.current = None;
            mem.started = false;
            mem.time_in_state = 0.0;
            mem.suppress = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rt::{GraphInstance, MapHost};
    use crate::{codegen, Graph, Interp, Node, Program};
    use seal_sealtext::SealMap;

    fn s(v: &str) -> SealValue {
        SealValue::String(v.into())
    }

    fn log_on(g: Graph, id: u32, event: &str, message: &str) -> Graph {
        let defaults = SealValue::Map(SealMap::new(vec![(
            "message".into(),
            SealValue::String(message.into()),
        )]));
        g.node(Node::new(id, "event").with("name", s(event)))
            .node(Node::new(id + 1, "log").with("defaults", defaults))
            .link(id, "then", id + 1, "in")
    }

    fn idle_run() -> Graph {
        let mut g = Graph::new("sm").var("go", Value::Bool(false)).var("speed", Value::Float(0.0));
        for (id, event, message) in [
            (1, "on_enter_idle", "enter idle"),
            (3, "on_update_idle", "update idle"),
            (5, "on_exit_idle", "exit idle"),
            (7, "on_enter_run", "enter run"),
            (9, "on_update_run", "update run"),
            (11, "on_exit_run", "exit run"),
            (13, "on_enter_hop", "enter hop"),
        ] {
            g = log_on(g, id, event, message);
        }
        g.machine = Some(Machine {
            initial: "idle".into(),
            states: vec![
                StateDef { name: "idle".into(), pos: [10.0, 20.0] },
                StateDef { name: "run".into(), pos: [200.0, 20.0] },
                StateDef { name: "hop".into(), pos: [390.0, 20.0] },
            ],
            transitions: vec![
                Transition {
                    from: "idle".into(),
                    to: "run".into(),
                    priority: 5,
                    cond: Cond::BoolVar("go".into()),
                },
                Transition {
                    from: "idle".into(),
                    to: "hop".into(),
                    priority: 1,
                    cond: Cond::After(1.0),
                },
                Transition {
                    from: "run".into(),
                    to: "run".into(),
                    priority: 0,
                    cond: Cond::Event("bounce".into()),
                },
            ],
        });
        g
    }

    fn play(g: &Graph) -> (Interp, MapHost) {
        (Interp::new(Program::build(g).unwrap(), 1), MapHost::default())
    }

    #[test]
    fn priority_beats_a_later_lower_transition_and_only_one_fires() {
        let mut g = idle_run();
        g.machine.as_mut().unwrap().transitions.push(Transition {
            from: "idle".into(),
            to: "hop".into(),
            priority: 5,
            cond: Cond::BoolVar("go".into()),
        });
        // Same priority as run, but later in the file, so run wins.
        let (mut interp, mut host) = play(&g);
        interp.advance(0.5, &mut host).unwrap();
        interp.set_var("go", Value::Bool(true)).unwrap();
        interp.advance(0.5, &mut host).unwrap();
        assert_eq!(
            host.logs,
            vec![
                "enter idle".to_string(),
                "update idle".to_string(),
                "update idle".to_string(),
                "exit idle".to_string(),
                "enter run".to_string(),
            ]
        );
        assert_eq!(interp.state_name(), "run");
    }

    #[test]
    fn higher_priority_wins_even_when_it_is_listed_later() {
        let mut g = idle_run();
        let m = g.machine.as_mut().unwrap();
        m.transitions[0].priority = 1;
        m.transitions.insert(
            0,
            Transition {
                from: "idle".into(),
                to: "hop".into(),
                priority: 1,
                cond: Cond::BoolVar("go".into()),
            },
        );
        m.transitions[1].priority = 9;
        let (mut interp, mut host) = play(&g);
        interp.advance(0.1, &mut host).unwrap();
        interp.set_var("go", Value::Bool(true)).unwrap();
        interp.advance(0.1, &mut host).unwrap();
        assert_eq!(interp.state_name(), "run");
        assert!(host.logs.iter().any(|l| l == "enter run"));
        assert!(!host.logs.iter().any(|l| l == "enter hop"));
    }

    #[test]
    fn timed_transition_waits_and_then_runs_exit_before_enter() {
        let (mut interp, mut host) = play(&idle_run());
        interp.advance(0.5, &mut host).unwrap();
        interp.advance(0.5, &mut host).unwrap();
        assert_eq!(interp.state_name(), "idle");
        interp.advance(0.5, &mut host).unwrap();
        assert_eq!(
            host.logs,
            vec![
                "enter idle".to_string(),
                "update idle".to_string(),
                "update idle".to_string(),
                "update idle".to_string(),
                "exit idle".to_string(),
                "enter hop".to_string(),
            ]
        );
        assert_eq!(interp.state_name(), "hop");
    }

    #[test]
    fn self_transition_exits_and_enters_again() {
        let g = idle_run()
            .node(Node::new(30, "event").with("name", s("do_bounce")))
            .node(Node::new(31, "trigger").with(
                "defaults",
                SealValue::Map(SealMap::new(vec![("event".into(), s("bounce"))])),
            ))
            .link(30, "then", 31, "in");
        let (mut interp, mut host) = play(&g);
        interp.advance(0.1, &mut host).unwrap();
        interp.set_var("go", Value::Bool(true)).unwrap();
        interp.advance(0.1, &mut host).unwrap();
        assert_eq!(interp.state_name(), "run");
        host.logs.clear();
        interp.fire("do_bounce", &[], &mut host).unwrap();
        assert!(host.logs.is_empty(), "the trigger waits for the step");
        interp.advance(0.1, &mut host).unwrap();
        assert_eq!(
            host.logs,
            vec!["update run".to_string(), "exit run".to_string(), "enter run".to_string()]
        );
        assert_eq!(interp.state_name(), "run");
    }

    #[test]
    fn force_state_switches_immediately_and_blocks_the_transition() {
        let g =
            idle_run()
                .node(Node::new(40, "event").with("name", s("force_hop")))
                .node(Node::new(41, "set_state").with(
                    "defaults",
                    SealValue::Map(SealMap::new(vec![("state".into(), s("hop"))])),
                ))
                .node(Node::new(42, "event").with("name", s("read")))
                .node(Node::new(43, "get_state"))
                .node(Node::new(44, "log"))
                .node(Node::new(45, "event").with("name", s("force_nope")))
                .node(Node::new(46, "set_state").with(
                    "defaults",
                    SealValue::Map(SealMap::new(vec![("state".into(), s("nope"))])),
                ))
                .link(40, "then", 41, "in")
                .link(42, "then", 44, "in")
                .link(43, "out", 44, "message")
                .link(45, "then", 46, "in");
        let (mut interp, mut host) = play(&g);
        interp.advance(0.1, &mut host).unwrap();
        interp.set_var("go", Value::Bool(true)).unwrap();
        interp.advance(0.1, &mut host).unwrap();
        host.logs.clear();
        interp.fire("force_hop", &[], &mut host).unwrap();
        assert_eq!(host.logs, vec!["exit run".to_string(), "enter hop".to_string()]);
        assert_eq!(interp.state_name(), "hop");
        host.logs.clear();
        // go is still true, but the force already spent this step's change.
        interp.advance(0.1, &mut host).unwrap();
        assert_eq!(interp.state_name(), "hop");
        interp.fire("read", &[], &mut host).unwrap();
        assert_eq!(host.logs.last().map(String::as_str), Some("hop"));
        let err = interp.fire("force_nope", &[], &mut host).unwrap_err();
        assert!(err.message.contains("unknown state nope"), "{err}");
        assert_eq!(interp.state_name(), "hop");
    }

    #[test]
    fn compare_condition_uses_the_variable() {
        let mut g = idle_run();
        g.machine.as_mut().unwrap().transitions[0] = Transition {
            from: "idle".into(),
            to: "run".into(),
            priority: 3,
            cond: Cond::Compare { var: "speed".into(), op: CmpOp::Gt, value: Value::Float(2.0) },
        };
        let (mut interp, mut host) = play(&g);
        interp.advance(0.1, &mut host).unwrap();
        interp.set_var("speed", Value::Float(2.0)).unwrap();
        interp.advance(0.1, &mut host).unwrap();
        assert_eq!(interp.state_name(), "idle");
        interp.set_var("speed", Value::Float(2.5)).unwrap();
        interp.advance(0.1, &mut host).unwrap();
        assert_eq!(interp.state_name(), "run");
    }

    #[test]
    fn round_trip_keeps_the_machine() {
        let g = idle_run();
        let back = Graph::from_text(&g.to_text()).unwrap();
        assert_eq!(back.machine, g.machine);
        assert_eq!(Graph::from_text(&back.to_text()).unwrap(), back);
    }

    #[test]
    fn damaged_machine_text_is_an_error() {
        let base = "schema_version = 1\nseal_graph = \"g\"\n";
        let err = Graph::from_text(&format!("{base}machine = \"nope\"\n")).unwrap_err();
        assert!(err.message.contains("state machine"), "{err}");
        let missing = r#"
schema_version = 1
seal_graph = "g"
machine = {
  initial = "idle"
  states = [{ name = "idle" }]
  transitions = [{ from = "idle" to = "missing" priority = 0 cond = "after" seconds = 1.0 }]
}
"#;
        let err = Graph::from_text(missing).unwrap_err();
        assert!(err.message.contains("unknown state missing"), "{err}");
        let bad_seconds = r#"
schema_version = 1
seal_graph = "g"
machine = {
  initial = "idle"
  states = [{ name = "idle" }]
  transitions = [{ from = "idle" to = "idle" priority = 0 cond = "after" seconds = "soon" }]
}
"#;
        assert!(Graph::from_text(bad_seconds).is_err());
        let dup = r#"
schema_version = 1
seal_graph = "g"
machine = {
  initial = "idle"
  states = [{ name = "idle" }, { name = "idle" }]
  transitions = []
}
"#;
        let err = Graph::from_text(dup).unwrap_err();
        assert!(err.message.contains("duplicate state"), "{err}");
    }

    #[test]
    fn reload_keeps_the_current_state_when_its_name_remains() {
        let (mut interp, mut host) = play(&idle_run());
        interp.advance(0.1, &mut host).unwrap();
        interp.set_var("go", Value::Bool(true)).unwrap();
        interp.advance(0.1, &mut host).unwrap();
        assert_eq!(interp.state_name(), "run");
        let mut swapped = idle_run();
        let states = &mut swapped.machine.as_mut().unwrap().states;
        states.swap(0, 1);
        interp.reload(Program::build(&swapped).unwrap());
        assert_eq!(interp.state_name(), "run");
    }

    #[test]
    fn codegen_mentions_the_machine_helpers() {
        let g =
            idle_run()
                .node(Node::new(50, "event").with("name", s("force_hop")))
                .node(Node::new(51, "set_state").with(
                    "defaults",
                    SealValue::Map(SealMap::new(vec![("state".into(), s("hop"))])),
                ))
                .link(50, "then", 51, "in");
        let src = codegen::compile(&Program::build(&g).unwrap());
        assert!(src.contains("MachineProg"), "{src}");
        assert!(src.contains("plan_name"), "{src}");
        assert!(src.contains("begin_step"), "{src}");
    }
}
