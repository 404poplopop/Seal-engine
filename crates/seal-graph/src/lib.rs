//! Visual logic graphs.
//!
//! A graph is saved as SealText: variables, nodes and links. [`Program::build`]
//! checks it and resolves every pin. The same [`Program`] then runs in the
//! editor play [`Interp`] (with breakpoints, stepping, watches and live
//! reload) or is turned into Rust by [`codegen::compile`]. Both call the
//! functions in [`rt`] for every operation, so they give identical results.

pub mod codegen;
mod device_nodes;
pub mod crypto;
pub mod ecs_host;
pub mod function;
mod interp;
pub mod inspector_layout;
pub mod kernel;
pub mod poly2d;
pub mod query;
pub mod replay;
pub mod rt;
pub mod state_machine;
pub mod script_core;

pub use interp::{Interp, ReloadReport};
pub use rt::{Elem, GraphError, GraphInstance, Host, MapHost, Plane, Quat, RunState, Ty, Value, V2, V3};

use seal_sealtext::{SealMap, SealValue};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const SCHEMA_VERSION: i64 = 1;

/// A variable declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct VarDef {
    pub name: String,
    pub ty: Ty,
    pub default: Value,
}

/// A node as saved. `params` keeps every field (including editor-only ones
/// like positions) so a load and save round trip loses nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: u32,
    pub kind: String,
    pub params: Vec<(String, SealValue)>,
}

impl Node {
    pub fn new(id: u32, kind: &str) -> Self {
        Self { id, kind: kind.into(), params: Vec::new() }
    }
    pub fn with(mut self, key: &str, value: SealValue) -> Self {
        self.set(key, value);
        self
    }
    pub fn set(&mut self, key: &str, value: SealValue) {
        match self.params.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = value,
            None => self.params.push((key.into(), value)),
        }
    }
    pub fn param(&self, key: &str) -> Option<&SealValue> {
        self.params.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    fn str_param(&self, key: &str) -> Result<&str, BuildError> {
        match self.param(key) {
            Some(SealValue::String(s)) => Ok(s),
            _ => Err(BuildError::node(self.id, format!("{} needs a text `{key}`", self.kind))),
        }
    }
    /// The `type` param, or `default` when the node leaves it out.
    fn ty_param_or(&self, default: Ty) -> Result<Ty, BuildError> {
        if self.param("type").is_none() {
            Ok(default)
        } else {
            self.ty_param()
        }
    }
    fn ty_param(&self) -> Result<Ty, BuildError> {
        let name = self.str_param("type")?;
        Ty::parse(name).ok_or_else(|| BuildError::node(self.id, format!("unknown type {name}")))
    }
}

/// One end of a link: node id and pin name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PinRef {
    pub node: u32,
    pub pin: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub from: PinRef,
    pub to: PinRef,
}

impl Link {
    pub fn new(from: u32, from_pin: &str, to: u32, to_pin: &str) -> Self {
        Self {
            from: PinRef { node: from, pin: from_pin.into() },
            to: PinRef { node: to, pin: to_pin.into() },
        }
    }
    fn text(&self) -> String {
        format!("{}.{} -> {}.{}", self.from.node, self.from.pin, self.to.node, self.to.pin)
    }
    fn parse(text: &str) -> Option<Link> {
        let (a, b) = text.split_once("->")?;
        let pin = |s: &str| -> Option<PinRef> {
            let (n, p) = s.trim().split_once('.')?;
            Some(PinRef { node: n.trim().parse().ok()?, pin: p.trim().to_string() })
        };
        Some(Link { from: pin(a)?, to: pin(b)? })
    }
}

/// A graph document.
#[derive(Clone, Debug, PartialEq)]
pub struct Graph {
    pub name: String,
    pub vars: Vec<VarDef>,
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
    /// Named functions. Empty when the file was saved without any.
    pub functions: Vec<function::GraphFn>,
    /// Function open in the editor. Ignored while the graph runs.
    pub open_function: Option<String>,
    /// Gameplay state machine for this graph, when it has one.
    pub machine: Option<state_machine::Machine>,
    /// Custom inspector layout for objects that use this script.
    pub inspector: Option<inspector_layout::InspectorLayout>,
}

/// A problem found while loading or checking a graph.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildError {
    pub node: Option<u32>,
    pub message: String,
}

impl BuildError {
    fn node(node: u32, message: impl Into<String>) -> Self {
        Self { node: Some(node), message: message.into() }
    }
    fn general(message: impl Into<String>) -> Self {
        Self { node: None, message: message.into() }
    }
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.node {
            Some(n) => write!(f, "node {n}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for BuildError {}

fn num(v: &SealValue) -> Option<&str> {
    if let SealValue::Number(n) = v {
        Some(n)
    } else {
        None
    }
}

/// Read a literal of a given type from SealText.
pub fn value_from_text(ty: Ty, v: &SealValue) -> Option<Value> {
    Some(match ty {
        Ty::Bool => Value::Bool(
            matches!(v, SealValue::Bool(true)) || {
                if let SealValue::Bool(false) = v {
                    false
                } else {
                    return None;
                }
            },
        ),
        Ty::Int => Value::Int(num(v)?.parse().ok()?),
        Ty::Float => Value::Float(num(v)?.parse().ok()?),
        Ty::Vec2 => match v {
            SealValue::Array(a) if a.len() == 2 => {
                Value::Vec2(V2 { x: num(&a[0])?.parse().ok()?, y: num(&a[1])?.parse().ok()? })
            }
            _ => return None,
        },
        Ty::Vec3 => match v {
            SealValue::Array(a) if a.len() == 3 => Value::Vec3(V3 {
                x: num(&a[0])?.parse().ok()?,
                y: num(&a[1])?.parse().ok()?,
                z: num(&a[2])?.parse().ok()?,
            }),
            _ => return None,
        },
        Ty::Quat => match v {
            SealValue::Array(a) if a.len() == 4 => Value::Quat(Quat {
                x: num(&a[0])?.parse().ok()?,
                y: num(&a[1])?.parse().ok()?,
                z: num(&a[2])?.parse().ok()?,
                w: num(&a[3])?.parse().ok()?,
            }),
            _ => return None,
        },
        Ty::Plane => match v {
            SealValue::Array(a) if a.len() == 4 => Value::Plane(Plane {
                nx: num(&a[0])?.parse().ok()?,
                ny: num(&a[1])?.parse().ok()?,
                nz: num(&a[2])?.parse().ok()?,
                d: num(&a[3])?.parse().ok()?,
            }),
            _ => return None,
        },
        Ty::Str => match v {
            SealValue::String(s) => Value::Str(s.clone()),
            _ => return None,
        },
        // Only the empty entity can be written down; real ids come from the world.
        Ty::Entity => match v {
            SealValue::String(s) if s == "none" => Value::Entity(rt::NO_ENTITY),
            SealValue::Null => Value::Entity(rt::NO_ENTITY),
            _ => return None,
        },
        Ty::List(e) => match v {
            SealValue::Array(a) => {
                Value::List(e, a.iter().map(|x| value_from_text(e.ty(), x)).collect::<Option<_>>()?)
            }
            _ => return None,
        },
    })
}

fn float_text(f: f64) -> String {
    let s = format!("{f:?}");
    if s.contains('.') || s.contains('e') || s.contains("inf") || s.contains("NaN") {
        s
    } else {
        format!("{s}.0")
    }
}

/// Write a literal as SealText. Entities have no literal form.
pub fn value_to_text(v: &Value) -> SealValue {
    match v {
        Value::Bool(b) => SealValue::Bool(*b),
        Value::Int(i) => SealValue::Number(i.to_string()),
        Value::Float(f) => SealValue::Number(float_text(*f)),
        Value::Vec2(v) => SealValue::Array(vec![
            SealValue::Number(float_text(v.x)),
            SealValue::Number(float_text(v.y)),
        ]),
        Value::Vec3(v) => SealValue::Array(vec![
            SealValue::Number(float_text(v.x)),
            SealValue::Number(float_text(v.y)),
            SealValue::Number(float_text(v.z)),
        ]),
        Value::Quat(v) => SealValue::Array(vec![
            SealValue::Number(float_text(v.x)),
            SealValue::Number(float_text(v.y)),
            SealValue::Number(float_text(v.z)),
            SealValue::Number(float_text(v.w)),
        ]),
        Value::Plane(v) => SealValue::Array(vec![
            SealValue::Number(float_text(v.nx)),
            SealValue::Number(float_text(v.ny)),
            SealValue::Number(float_text(v.nz)),
            SealValue::Number(float_text(v.d)),
        ]),
        Value::Str(s) => SealValue::String(s.clone()),
        Value::Entity(e) if *e == rt::NO_ENTITY => SealValue::String("none".into()),
        Value::Entity(_) => SealValue::Null,
        Value::List(_, items) => SealValue::Array(items.iter().map(value_to_text).collect()),
    }
}

impl Graph {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            vars: Vec::new(),
            nodes: Vec::new(),
            links: Vec::new(),
            functions: Vec::new(),
            open_function: None,
            machine: None,
            inspector: None,
        }
    }

    pub fn var(mut self, name: &str, default: Value) -> Self {
        self.vars.push(VarDef { name: name.into(), ty: default.ty(), default });
        self
    }
    pub fn node(mut self, node: Node) -> Self {
        self.nodes.push(node);
        self
    }
    pub fn link(mut self, from: u32, from_pin: &str, to: u32, to_pin: &str) -> Self {
        self.links.push(Link::new(from, from_pin, to, to_pin));
        self
    }
    /// The smallest unused node id, across the event graph and every function.
    pub fn next_id(&self) -> u32 {
        self.nodes
            .iter()
            .map(|n| n.id + 1)
            .chain(self.functions.iter().flat_map(|f| f.nodes.iter().map(|n| n.id + 1)))
            .max()
            .unwrap_or(1)
    }

    pub fn to_value(&self) -> SealValue {
        let vars = self
            .vars
            .iter()
            .map(|v| {
                SealValue::Map(SealMap::new(vec![
                    ("name".into(), SealValue::String(v.name.clone())),
                    ("type".into(), SealValue::String(v.ty.name().into())),
                    ("default".into(), value_to_text(&v.default)),
                ]))
            })
            .collect();
        let nodes = self
            .nodes
            .iter()
            .map(|n| {
                let mut e = vec![
                    ("id".into(), SealValue::Number(n.id.to_string())),
                    ("kind".into(), SealValue::String(n.kind.clone())),
                ];
                e.extend(n.params.iter().cloned());
                SealValue::Map(SealMap::new(e))
            })
            .collect();
        let links = self.links.iter().map(|l| SealValue::String(l.text())).collect();
        let mut entries = vec![
            ("schema_version".into(), SealValue::Number(SCHEMA_VERSION.to_string())),
            ("seal_graph".into(), SealValue::String(self.name.clone())),
            ("vars".into(), SealValue::Array(vars)),
            ("nodes".into(), SealValue::Array(nodes)),
            ("links".into(), SealValue::Array(links)),
        ];
        if !self.functions.is_empty() {
            entries.push(("functions".into(), function::write_functions(&self.functions)));
        }
        if let Some(open) = &self.open_function {
            entries.push(("open_function".into(), SealValue::String(open.clone())));
        }
        if let Some(machine) = &self.machine {
            entries.push(("machine".into(), machine.to_seal()));
        }
        if let Some(inspector) = &self.inspector {
            entries.push(("inspector".into(), inspector.to_seal()));
        }
        SealValue::Map(SealMap::new(entries))
    }

    pub fn to_text(&self) -> String {
        self.to_value().to_text()
    }

    pub fn from_text(text: &str) -> Result<Graph, BuildError> {
        let v = seal_sealtext::parse(text)
            .map_err(|e| BuildError::general(format!("line {}: {}", e.line, e.message)))?;
        Graph::from_value(&v)
    }

    pub fn from_value(v: &SealValue) -> Result<Graph, BuildError> {
        let bad = |m: &str| BuildError::general(m.to_string());
        match v.get("schema_version").and_then(num).and_then(|n| n.parse::<i64>().ok()) {
            Some(SCHEMA_VERSION) => {}
            Some(other) => {
                return Err(bad(&format!(
                    "graph schema {other} is newer than this build reads ({SCHEMA_VERSION})"
                )));
            }
            None => return Err(bad("not a graph: no schema_version")),
        }
        let name = match v.get("seal_graph") {
            Some(SealValue::String(s)) => s.clone(),
            _ => return Err(bad("not a graph: no seal_graph name")),
        };
        let mut g = Graph::new(&name);
        let list = |key: &str| -> Result<Vec<SealValue>, BuildError> {
            match v.get(key) {
                None => Ok(Vec::new()),
                Some(SealValue::Array(a)) => Ok(a.clone()),
                Some(_) => Err(bad(&format!("`{key}` must be a list"))),
            }
        };
        for var in list("vars")? {
            let name = match var.get("name") {
                Some(SealValue::String(s)) => s.clone(),
                _ => return Err(bad("a variable has no name")),
            };
            let ty = match var.get("type") {
                Some(SealValue::String(s)) => Ty::parse(s)
                    .ok_or_else(|| bad(&format!("variable {name} has unknown type {s}")))?,
                _ => return Err(bad(&format!("variable {name} has no type"))),
            };
            let default = match var.get("default") {
                None => ty.zero(),
                Some(d) => value_from_text(ty, d)
                    .ok_or_else(|| bad(&format!("variable {name}: default is not a {ty}")))?,
            };
            g.vars.push(VarDef { name, ty, default });
        }
        for node in list("nodes")? {
            let entries = node.as_map().ok_or_else(|| bad("a node must be a map"))?;
            let id = node
                .get("id")
                .and_then(num)
                .and_then(|n| n.parse::<u32>().ok())
                .ok_or_else(|| bad("a node has no id"))?;
            let kind = match node.get("kind") {
                Some(SealValue::String(s)) => s.clone(),
                _ => return Err(BuildError::node(id, "no kind")),
            };
            let params =
                entries.iter().filter(|(k, _)| k != "id" && k != "kind").cloned().collect();
            g.nodes.push(Node { id, kind, params });
        }
        for link in list("links")? {
            let SealValue::String(s) = &link else { return Err(bad("a link must be text")) };
            g.links.push(
                Link::parse(s)
                    .ok_or_else(|| bad(&format!("bad link `{s}`; write `1.then -> 2.in`")))?,
            );
        }
        g.functions = function::read_functions(v)?;
        g.open_function = function::read_open(v)?;
        if let Some(machine) = v.get("machine") {
            g.machine = Some(state_machine::Machine::from_seal(machine, &g.vars)?);
        }
        if let Some(inspector) = v.get("inspector") {
            g.inspector = Some(
                inspector_layout::InspectorLayout::from_seal(inspector)
                    .map_err(BuildError::general)?,
            );
        }
        Ok(g)
    }
}

/// Binary operators.
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
    Concat,
    /// Whether a contains b.
    StrContains,
    /// Whether a starts with b.
    StrStarts,
    /// Whether a ends with b.
    StrEnds,
    /// Dot product of two vectors.
    Dot,
    /// Cross product of two vec3s.
    Cross,
    /// a to the power b.
    Pow,
    /// Angle of the point (x = b, y = a) in radians.
    Atan2,
}

/// List operations. Lists are values: an operation that changes a list
/// returns the changed copy, and a variable keeps it through `set`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListOp {
    /// A list of `n` items, `item0` to `item{n-1}`.
    Make(u32),
    Len,
    Get,
    Push,
    Set,
    Insert,
    RemoveAt,
    RemoveItem,
    Find,
    Contains,
}

/// Unary operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Abs,
    Sqrt,
    /// Number of characters in a text.
    StrLen,
    /// Upper-case copy of a text.
    StrUpper,
    /// Lower-case copy of a text.
    StrLower,
    /// A text without white space at both ends.
    StrTrim,
    /// A text without white space at the start.
    StrTrimStart,
    /// A text without white space at the end.
    StrTrimEnd,
    /// The whole number a text starts with, or 0.
    ParseInt,
    /// The number a text starts with, or 0.
    ParseFloat,
    /// A number as display text: thousands separated by commas, floats
    /// rounded to at most three decimals.
    NumText,
    /// A vector as `X=1.000 Y=2.000 Z=3.000`: three fixed decimals.
    VecText,
    /// A float as classic display text: six decimals with trailing zeros
    /// dropped, keeping at least one (`5.0`, `0.25`).
    FloatText,
    Not,
    ToFloat,
    ToInt,
    ToStr,
    Vec2X,
    Vec2Y,
    Vec3X,
    Vec3Y,
    Vec3Z,
    Length,
    Sin,
    Cos,
    /// Float to int, rounding down, to nearest (halves away from zero) or up.
    Floor,
    Round,
    Ceil,
    /// The vec2 scaled to length 1 (zero stays zero).
    Normalize,
    Exp,
    /// Natural logarithm.
    Ln,
    Atan,
    Asin,
    Acos,
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

impl CmpOp {
    pub fn parse(s: &str) -> Option<CmpOp> {
        Some(match s {
            "lt" | "<" => CmpOp::Lt,
            "le" | "<=" => CmpOp::Le,
            "gt" | ">" => CmpOp::Gt,
            "ge" | ">=" => CmpOp::Ge,
            "eq" | "==" => CmpOp::Eq,
            "ne" | "!=" => CmpOp::Ne,
            _ => return None,
        })
    }
    pub fn name(self) -> &'static str {
        match self {
            CmpOp::Lt => "lt",
            CmpOp::Le => "le",
            CmpOp::Gt => "gt",
            CmpOp::Ge => "ge",
            CmpOp::Eq => "eq",
            CmpOp::Ne => "ne",
        }
    }
}

/// One call on the handle-based physics server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerCall {
    Body,
    Shape,
    Joint,
    Step,
    Position,
}

impl ServerCall {
    pub fn name(self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::Shape => "shape",
            Self::Joint => "joint",
            Self::Step => "step",
            Self::Position => "position",
        }
    }

    pub fn is_exec(self) -> bool {
        !matches!(self, Self::Position)
    }
}

/// A resolved node operation.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    // Execution nodes.
    Event {
        name: String,
    },
    Branch,
    Sequence(u32),
    For,
    Delay,
    Set(u32),
    Call(u32),
    /// End the event and hand values back to the `call` that ran it.
    Return(u32),
    /// Start (or restart) the timer that runs an event after `seconds`.
    SetTimer(u32),
    /// Stop an event's timer.
    ClearTimer(u32),
    Log,
    WorldSet {
        component: String,
        field: String,
        ty: Ty,
    },
    // Pure nodes.
    Const(Value),
    Get(u32),
    SelfRef,
    /// Seconds of game time this instance has run.
    GameTime,
    /// The length in seconds of the last frame.
    FrameDt,
    /// A seeded random whole number from `a` to `b`, both included.
    RandomInt,
    /// A seeded random number from `a` up to but not including `b`.
    RandomFloat,
    Bin(BinOp, Ty),
    Un(UnOp, Ty),
    Compare(CmpOp, Ty),
    Select(Ty),
    MakeVec2,
    MakeVec3,
    Scale(Ty),
    WorldGet {
        component: String,
        field: String,
        ty: Ty,
    },
    List(ListOp, Elem),
    /// Whether an entity still exists (pure, asks the host).
    Exists,
    /// Whether the entity was made from this prefab or one based on it.
    IsA {
        prefab: String,
    },
    /// Every live entity made from this prefab or one based on it.
    FindAll {
        prefab: String,
    },
    /// The live entity with the lowest id made from this prefab or one based on it.
    FindFirst {
        prefab: String,
    },
    /// The entity a local player controls (pure, asks the host).
    Player,
    InputAxis,
    /// Whether the input action `name` is held this step (pure, asks the host).
    ActionPressed,
    /// Seconds the action has been held after its modifiers.
    ActionHeld,
    /// Whether the action went up this step.
    ActionReleased,
    /// Pointer movement this step.
    MouseDelta,
    /// Mouse wheel this step.
    MouseWheel,
    /// Pointer position in the flat scene.
    PointerWorld,
    /// Raw pointer motion, pen contact, or a gamepad light and sensor reading.
    Device(device_nodes::DeviceKind),
    /// Nearest object on a ray: the object (dist false) or its distance (dist true).
    RayHit {
        dist: bool,
    },
    /// Dynamic bodies inside a trigger area.
    BodiesInside,
    /// A physics query: every ray hit, a sweep, a point, an overlap, or a test move.
    Phys(query::Kind),
    /// Create, step or read a body in the physics server, by handle.
    Server(ServerCall),
    // Execution nodes that change which entities exist.
    Spawn {
        prefab: String,
    },
    Despawn,
    /// `at`: the sound has a world position (input 3).
    PlaySound {
        at: bool,
    },
    /// Start a music file on the music bus. Inputs: file, looping, fade seconds, loop start seconds.
    PlayMusic,
    /// Fade the music bus out. Input: fade seconds.
    StopMusic,
    /// Set the music bus volume. Input: volume.
    SetMusicVolume,
    /// Push a dynamic object: inputs entity, impulse (Vec3).
    ApplyImpulse,
    /// Move, jump, or set the mode of a Character 2D.
    Character(rt::CharacterKind),
    /// Floor, wall or velocity of a Character 2D.
    CharacterQuery(rt::CharacterQuery),
    ApplyForce2d,
    ApplyTorque2d,
    SetVelocity2d,
    SetJointTarget2d,
    Mass2d,
    /// Start a keyframe animation (.anim file) on an object: inputs entity, anim (path).
    PlayAnim,
    /// Start a whole-scene sequence (.seq file): input sequence (path).
    PlaySequence,
    /// An AI order for an object: move to a point, chase, patrol, stop.
    Ai(rt::AiKind),
    /// Whether an object's AI order has finished.
    AiArrived,
    Game(rt::GameKind),
    Score,
    /// Whether a save slot holds a game (pure, asks the host).
    HasSave,
    /// A value kept for saving, by key (pure, asks the host).
    Saved,
    /// Whether a scene file is open or added (pure, asks the host).
    SceneLoaded,
    /// The open scene file (pure, asks the host).
    CurrentScene,
    /// Read a `.data` asset (flow).
    LoadData,
    /// One field from a Data asset.
    DataField,
    /// One CSV cell from a project file.
    ReadCsvCell,
    /// One JSON value from a project file.
    ReadJsonValue,
    /// Script-core pure nodes (quaternions, planes, text, typed JSON, clocks).
    ScriptCore(script_core::ScriptCoreOp),
    UiValue,
    UiInput,
    GameState,
    /// Stop the animation running on an object.
    StopAnim,
    /// Switch a sprite object to a named clip of its .frames animation.
    PlayClip,
    UiShow,
    UiText,
    /// Read one entry of a string table. Inputs table, key, values.
    GetString,
    /// Restart a label's typewriter, or skip it when `skip` is true.
    Typewriter,
    /// Shake the play view: inputs strength (units), duration (seconds).
    CameraShake,
    /// Rumble the gamepad: inputs strong, weak (0..1), duration (seconds).
    Rumble,
    /// Wait for the player's next button and bind it to an action: inputs action, slot.
    RebindAction,
    /// Switch to state `state` now. An unknown name is an error.
    SetState,
    /// Note event name `event` for this step's transition check.
    Trigger,
    /// Ask the player for an existing file. Output `path` is empty when cancelled.
    OpenFileDialog,
    /// Ask the player where to write a file. Output `path` is empty when cancelled.
    SaveFileDialog,
    /// Seal text with the project key. Output `text` is the envelope.
    Encrypt,
    /// Open a sealed envelope. Fails when it was changed or the key is wrong.
    Decrypt,
    /// Show the built-in message, confirm, or input dialog.
    ShowDialog,
    /// Button and text from the last dialog answer. Pin 0 is the button.
    DialogAnswer,
    /// Queue a save slot for upload.
    CloudUpload,
    /// Queue a download of a save slot.
    CloudDownload,
    /// Try the cloud queue now.
    CloudFlush,
    /// Finish a script-hook cloud job.
    CloudReply,
    /// The cloud job a script hook is handling. Pins: kind, slot, saved_at, body.
    CloudJob,
    /// The name of the current state, or empty before the machine starts.
    GetState,
    /// Seconds spent in the current state.
    TimeInState,
    /// Offset, union, difference or overlap of a polygon outline.
    Poly(PolyOp),
    /// Replace a 2D collider's shape. The physics body rebuilds on the next step.
    SetColliderShape,
    /// Turn a 2D collider on or off.
    SetColliderEnabled,
}

/// Polygon outline operations. Points are a list of vec2 in order around the shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolyOp {
    Offset,
    Merge,
    Clip,
    Intersect,
}

impl Op {
    pub fn is_exec(&self) -> bool {
        matches!(
            self,
            Op::Event { .. }
                | Op::Branch
                | Op::Sequence(_)
                | Op::For
                | Op::Delay
                | Op::Set(_)
                | Op::Call(_)
                | Op::Return(_)
                | Op::SetTimer(_)
                | Op::ClearTimer(_)
                | Op::Log
                | Op::WorldSet { .. }
                | Op::Spawn { .. }
                | Op::Despawn
                | Op::PlaySound { .. }
                | Op::PlayMusic
                | Op::StopMusic
                | Op::SetMusicVolume
                | Op::ApplyImpulse
                | Op::Character(_)
                | Op::ApplyForce2d
                | Op::ApplyTorque2d
                | Op::SetVelocity2d
                | Op::SetColliderShape
                | Op::SetColliderEnabled
                | Op::SetJointTarget2d
                | Op::LoadData
                | Op::PlayAnim
                | Op::PlaySequence
                | Op::Ai(_)
                | Op::Game(_)
                | Op::StopAnim
                | Op::PlayClip
                | Op::UiShow
                | Op::UiText
                | Op::Typewriter
                | Op::CameraShake
                | Op::Rumble
                | Op::RebindAction
                | Op::Device(device_nodes::DeviceKind::SetLight)
                | Op::SetState
                | Op::Trigger
                | Op::OpenFileDialog
                | Op::SaveFileDialog
                | Op::Encrypt
                | Op::Decrypt
                | Op::ShowDialog
                | Op::CloudUpload
                | Op::CloudDownload
                | Op::CloudFlush
                | Op::CloudReply
                | Op::Server(ServerCall::Body)
                | Op::Server(ServerCall::Shape)
                | Op::Server(ServerCall::Joint)
                | Op::Server(ServerCall::Step)
        )
    }
}

/// Where a data input comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum Src {
    /// Output `pin` of node `node` (an index into [`Program::nodes`]).
    Pin { node: u32, pin: u32 },
    /// The input is not linked; this is its value.
    Const(Value),
}

/// A checked node with every pin resolved.
#[derive(Clone, Debug, PartialEq)]
pub struct PNode {
    pub id: u32,
    pub op: Op,
    pub inputs: Vec<Src>,
    /// The node each execution output continues at, by output index.
    pub next: Vec<Option<u32>>,
    /// For execution nodes: the slot each data output is stored in.
    pub slots: Vec<u32>,
    pub out_tys: Vec<Ty>,
}

/// An event entry point.
#[derive(Clone, Debug, PartialEq)]
pub struct EventInfo {
    pub name: String,
    pub node: u32,
    pub params: Vec<(String, Ty)>,
    /// Values a `return` node hands back to the `call` that ran this event.
    pub returns: Vec<(String, Ty)>,
}

/// A checked graph, ready to run or compile.
#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    pub name: String,
    pub vars: Vec<VarDef>,
    pub nodes: Vec<PNode>,
    pub events: Vec<EventInfo>,
    pub slots: Vec<Ty>,
    /// The graph's state machine, when it has one.
    pub machine: Option<state_machine::MachineProg>,
}

/// The pins of a node kind.
#[derive(Clone, Debug, Default)]
pub struct Sig {
    pub exec_in: bool,
    pub exec_outs: Vec<String>,
    pub ins: Vec<(String, Ty)>,
    pub outs: Vec<(String, Ty)>,
}

/// Every node kind, with a one-line description, for palettes and docs.
pub const KINDS: &[(&str, &str)] = &[
    ("event", "Entry point. `name` = start, tick (gives dt) or your own with `params`."),
    ("branch", "Continue at `true` or `false`."),
    ("sequence", "Run `then0`, `then1`, ... in order (`count` outputs)."),
    ("for", "Run `body` for each `index` from `first` to `last`, then `done`; `stop`, checked before each pass, ends it early."),
    ("delay", "Continue at `then` after `seconds`; re-triggering while waiting is ignored."),
    ("set", "Set variable `name` to `value`."),
    ("call", "Fire your own event `name` with its parameters, then continue."),
    ("entry", "Start of a function. Its outputs are the function's inputs."),
    ("fn_return", "End a function and pass its outputs back to the caller."),
    ("set_timer", "Run event `name` (no parameters) after `seconds`, again every `seconds` if `looping`; replaces its running timer, and zero or less stops it."),
    ("clear_timer", "Stop the timer of event `name`."),
    ("log", "Write `message` to the log."),
    ("world_set", "Set `component`.`field` (`type`) on `entity`."),
    ("const", "A fixed `value` of `type`."),
    ("get", "Read variable `name`."),
    ("set_state", "Switch to state `state` immediately. The name must be one of this graph's states."),
    ("get_state", "The name of the state machine's current state, or empty before it starts."),
    ("time_in_state", "Seconds spent in the current state since it was entered."),
    ("trigger", "Note event `event` so a transition waiting for that name can fire this step."),
    ("self", "The entity this graph runs on."),
    ("add", "a + b (int, float, vec2)."),
    ("sub", "a - b (int, float, vec2)."),
    ("mul", "a * b (int, float)."),
    ("div", "a / b (int, float); integer division by zero gives 0."),
    ("mod", "a % b (int or float); the sign follows a, and a divisor of zero (or under 1e-8 for floats) gives 0."),
    ("min", "Smaller of a and b (int, float)."),
    ("max", "Larger of a and b (int, float)."),
    ("and", "a and b."),
    ("or", "a or b."),
    ("not", "not a."),
    ("neg", "-a (int, float, vec2)."),
    ("abs", "|a| (int, float)."),
    ("sqrt", "Square root."),
    ("str_contains", "Whether text a contains text b."),
    ("str_starts", "Whether text a starts with text b."),
    ("str_ends", "Whether text a ends with text b."),
    ("str_len", "Number of characters in a text."),
    ("str_upper", "Upper-case copy of a text."),
    ("str_lower", "Lower-case copy of a text."),
    ("str_trim", "Text a without spaces, tabs and line breaks at both ends."),
    ("str_trim_start", "Text a without spaces, tabs and line breaks at the start."),
    ("str_trim_end", "Text a without spaces, tabs and line breaks at the end."),
    ("parse_int", "The whole number a text starts with, after any spaces and a sign: \"42abc\" gives 42, text with no number gives 0."),
    ("parse_float", "The number a text starts with, after any spaces and a sign, with an optional fraction and exponent: \"2.5e1m\" gives 25, text with no number gives 0."),
    ("compare", "a `op` b: lt, le, gt, ge (int, float) or eq, ne (any type)."),
    ("select", "a when cond, else b."),
    ("to_float", "int to float."),
    ("to_int", "float to int, toward zero."),
    ("to_str", "Any value as text."),
    ("float_text", "A float as classic display text: six decimals, trailing zeros dropped but one kept: 5 gives \"5.0\", 0.1 gives \"0.1\"."),
    ("vec_text", "A vector as text with three fixed decimals: \"X=1.000 Y=2.000 Z=3.000\" (no Z for a 2D vector)."),
    ("num_text", "A number as display text: 1234567 gives \"1,234,567\", 2.71828 gives \"2.718\"; at most three decimals, trailing zeros dropped."),
    ("concat", "Join two texts."),
    ("vec2", "Make a vec2 from x and y."),
    ("vec2_x", "x of a vec2."),
    ("vec2_y", "y of a vec2."),
    ("scale", "vec2 times a float."),
    ("length", "Length of a vec2."),
    ("normalize", "A vec2 scaled to length 1; zero stays zero."),
    ("dot", "Dot product of two vec2s."),
    ("sin", "Sine of an angle in radians; the same on every machine."),
    ("cos", "Cosine of an angle in radians; the same on every machine."),
    ("exp", "e to the power a; the same on every machine."),
    ("ln", "Natural logarithm of a; the same on every machine."),
    ("pow", "a to the power b; the same on every machine."),
    ("atan", "Arctangent in radians; the same on every machine."),
    ("asin", "Arcsine in radians; NaN outside -1..1; the same on every machine."),
    ("acos", "Arccosine in radians; NaN outside -1..1; the same on every machine."),
    ("atan2", "Angle in radians of the point x = b, y = a; the same on every machine."),
    ("floor", "float to int, rounding down."),
    ("round", "float to int, rounding to nearest (halves away from zero)."),
    ("ceil", "float to int, rounding up."),
    ("world_get", "Read `component`.`field` (`type`) from `entity`."),
    ("spawn", "Create an entity from prefab `prefab`; `entity` is the new one."),
    ("play_sound", "Play the sound file `sound` (a project path) once at `volume` (1 is as recorded) and `pitch` (2 is an octave up)."),
    ("play_music", "Play music file `file` on the music bus. `looping` restarts it at `loop_start` seconds. `fade` is how many seconds the new track takes to replace the one already playing (0 starts at once)."),
    ("stop_music", "Fade the music bus out over `fade` seconds. Zero stops at once."),
    ("set_music_volume", "Set the music bus volume. 1 is as recorded, 0 is silent."),
    ("ui_show", "Show or hide the on-screen widget `widget` during play."),
    ("ui_text", "Set the on-screen text element `element` to `text`; shown while any widget is."),
    ("get_string", "Read `key` from the string table `table`. `values` fills {name} placeholders as name=value pairs."),
    ("typewriter", "Restart the typewriter on `element`, or skip to the end when `skip` is on."),
    ("camera_shake", "Shake the play view by up to `strength` units, fading out over `duration` seconds; shakes add up while they overlap."),
    ("play_sound_at", "Like `play_sound`, at the world point `location`."),
    ("play_sequence", "Start the sequence file `sequence` (a project path to a .seq file): its tracks move the objects it names and its events fire as it plays."),
    ("play_anim", "Start the keyframe animation file `anim` (a project path to an .anim file) on the object `entity`."),
    ("ai_move_to", "Walk the object `entity` to the point `target` at `speed` units per second, around solid objects (path on the ground grid)."),
    ("ai_chase", "Keep the object `entity` walking toward the object `target` at `speed`, re-planning as it moves; it stops next to it."),
    ("ai_patrol", "Walk the object `entity` back and forth between the points `a` and `b` at `speed`, until another order."),
    ("ai_stop", "Cancel the AI order of the object `entity`; it stays where it is."),
    ("possess", "Player `index` (0 is the first) now controls `entity`: `player` returns it and its keys move it."),
    ("spawn_player", "Put player `index`'s object on a `player_start` object (start points are shared in turn) and stop it."),
    ("add_score", "Add `amount` to the score called `name` (negative takes away). Scores start at 0 each play."),
    ("set_game_state", "Set the game's state, such as `playing`, `won` or `lost`; the play bar shows it."),
    ("ui_value", "The value of the on-screen control called `element`: a slider's or bar's value, 1 or 0 for a toggle, the choice number of a dropdown or list."),
    ("ui_input", "The text of the on-screen control called `element`: what was typed in a text field, or the chosen entry of a dropdown or list."),
    ("score", "The score called `name`, 0 when nothing was added yet."),
    ("game_state", "The game's state, empty until a script sets one."),
    ("save_game", "Save the running game in the slot `slot` (letters, digits, spaces, - and _): every object, the scores, the game's state and the values kept with `set_saved`. The project's save version goes with it."),
    ("load_game", "Put the game saved in `slot` back: objects, scores, state and kept values, then fire the game_loaded event. Saves from an older save version load too; `saved` of `save_version` tells which."),
    ("delete_save", "Remove the save in the slot `slot`; a missing one is ignored."),
    ("has_save", "Whether the slot `slot` holds a saved game."),
    ("set_saved", "Keep `value` under `key` for saving; `saved` reads it back, and `load_game` restores it."),
    ("saved", "The value kept under `key` with `set_saved` or loaded from a save, empty when there is none."),
    ("open_scene", "Replace the running scene with the scene file `path` (project-relative, like scenes/level2.seal). Objects marked with keep_across_scenes stay, scores and kept values stay, and scene_loaded fires."),
    ("add_scene", "Add the objects of the scene file `path` to the running scene, next to what is there. unload_scene takes them out again."),
    ("load_scene_async", "Read the scene file `path` in the background so play does not stall, then open it (or add it when `additive` is on) on a later frame and fire scene_loaded."),
    ("unload_scene", "Take out the objects that add_scene brought in from `path`."),
    ("keep_across_scenes", "Mark `entity` and everything under it to stay when open_scene switches scenes (a player, music, a score keeper). Turn `keep` off to let it go with the scene again."),
    ("scene_loaded", "Whether the scene file `path` is open or added right now."),
    ("current_scene", "The scene file that is open, project-relative."),
    ("ai_arrived", "Whether the object `entity` finished its last move-to order (or is next to the object it chases)."),
    ("stop_anim", "Stop the keyframe animation running on the object `entity`."),
    ("play_clip", "Switch the sprite object `entity` to the clip `clip` (such as idle, run or jump) of its sprite animation; the clip already playing keeps going."),
    ("apply_force_2d", "Add `force` to the dynamic 2D body `entity` for one step (world units per second squared times mass)."),
    ("apply_torque_2d", "Add `torque` to the dynamic 2D body `entity` for one step."),
    ("set_velocity_2d", "Set the linear and angular velocity of dynamic 2D body `entity`."),
    ("set_joint_target_2d", "Set the world pull point of a Target joint on `entity`."),
    ("get_mass_2d", "Total mass of dynamic 2D body `entity` (collider plus extra mass)."),
    ("apply_impulse", "Add `impulse` to the velocity of the dynamic object `entity` (a one-off push)."),
    ("physics_body", "Make a body in physics world `world` at `x` `y` `z`. `dynamic` falls under that world's gravity. The output is a handle, not a scene object."),
    ("physics_shape", "Attach a shape to body handle `body` in `world`. `kind` is cylinder, trimesh, heightfield or convex. Cylinder uses `a` as radius and `b` as half height. A mesh or convex piece reads `mesh` (a project path or mesh text). A heightfield reads `mesh` as a picture path and `a` `b` `c` as width, height and depth. Convex uses `c` as the most hulls."),
    ("physics_joint", "Join body handles `body_a` and `body_b` in `world`. `kind` is fixed, distance or hinge. `length` of 0 keeps the distance they have now."),
    ("physics_step", "Step physics world `world` forward by `dt` seconds. Scene worlds also step during play; this steps a server world you drive yourself."),
    ("physics_position", "Position of body handle `body` in physics world `world`."),
    ("move_character", "Slide the Character 2D on `entity` by `wish` this step. Each axis is usually from -1 to 1. Walk uses the horizontal axis."),
    ("jump", "Ask the Character 2D on `entity` to jump. A press just before landing, or just after leaving a floor, still counts."),
    ("is_on_floor", "Whether the Character 2D on `entity` stands on a walkable floor."),
    ("is_on_wall", "Whether the Character 2D on `entity` is touching a wall or a slope too steep to walk."),
    ("set_movement_mode", "Set the Character 2D on `entity` to `mode`: walk, fly, swim or climb. A water area or a climb area can still switch Walk."),
    ("get_velocity", "Velocity of the Character 2D on `entity`, in scene units per second."),
    ("ray_hit", "The nearest object a ray from `from` along `direction` hits within `max_distance` (boxes and spheres from their scale), or no entity."),
    ("bodies_inside", "Every dynamic body inside the trigger area on `entity`, lowest id first. Wire an On Body Entered or On Body Exited event (`body_entered`, `body_exited`) to hear one body at a time."),
    ("ray_distance", "How far along the ray `ray_hit` finds its object; `max_distance` when nothing is hit."),
    ("raycast_all", "Every object a ray from `from` along `direction` meets within `max_distance`, nearest first, with the hit point, the normal and the distance. `mask` is layer bits; 0 or less is every layer."),
    ("overlap_shape", "Every object overlapping `shape` (`box`, `circle`, `capsule` or `polygon`) at `center`, turned by `angle` radians. `size` is the full width and height. `points` are polygon corners in local units. Lowest id first. `mask` 0 or less is every layer. An unknown shape returns nothing."),
    ("set_collider_shape", "Set the 2D collider on `entity` to `shape` (`box`, `circle`, `capsule` or `polygon`). `size` is the full width and height; a circle uses x as its diameter. `points` are polygon corners in the object's own units. The physics body updates on the next step."),
    ("set_collider_enabled", "Turn the 2D collider on `entity` on or off. Off, it no longer blocks and queries skip it."),
    ("shape_cast", "The first object a circle or a box swept from `from` along `direction` meets within `max_distance`. `shape` is circle or box. A circle's size x is its diameter. Sensors are skipped. `mask` 0 or less is every layer."),
    ("point_query", "Every object whose collider contains `point`, lowest id first. `mask` is layer bits; 0 or less is every layer."),
    ("overlap_box", "Every object overlapping an axis-aligned box at `center` with full width and height `size`, lowest id first. `mask` 0 or less is every layer."),
    ("overlap_circle", "Every object overlapping a circle at `center` with `radius`, lowest id first. `mask` 0 or less is every layer."),
    ("test_move", "Whether `entity` would hit another solid if it moved by `delta`. Sensors and the object itself are skipped. `mask` 0 or less is every layer."),
    ("despawn", "Remove `entity` (and its children) from the world; a missing one is ignored."),
    ("is_valid", "Whether `entity` still exists."),
    ("is_a", "Whether `entity` exists and was made from prefab `prefab` or one based on it."),
    ("game_time", "Seconds of game time this graph has run, the sum of every frame so far."),
    ("frame_dt", "The length in seconds of the last frame, 0 before the first one."),
    ("random_int", "A random whole number from `a` to `b`, both included (`a` when `b` is smaller). Each instance has its own seeded sequence, so runs repeat exactly."),
    ("random_float", "A random number from `a` up to `b`. Each instance has its own seeded sequence, so runs repeat exactly."),
    ("action_pressed", "Whether the input action `name` (from the project's input bindings) is held this step."),
    ("action_held_time", "Seconds the input action `name` has been held, after a hold, double tap or toggle modifier. 0 while it is up."),
    ("action_released", "Whether the input action `name` went up this step."),
    ("mouse_delta", "How far the pointer moved this step, in screen pixels. x is to the right, y is down."),
    ("raw_mouse_delta", "Unaccelerated pointer motion this step, in the device's own units. x is to the right, y is down. Zero when the window did not report it."),
    ("mouse_look", "Pointer motion for mouse look, in the same units as raw_mouse_delta. Uses raw motion when the project setting Raw mouse look is on and raw motion was reported; otherwise the cursor motion."),
    ("pen_pressure", "How hard the pen is pressing, from 0 to 1. Zero when no pen is down."),
    ("pen_tilt", "How far the pen leans, from -1 to 1 on each axis. Zero is upright. Zero when the event did not include tilt."),
    ("pen_eraser", "Whether the eraser end of the pen is down."),
    ("pad_gyro", "Gamepad rotation speed, in radians per second. Zero when the pad has no motion sensors."),
    ("pad_accel", "Gamepad acceleration, in gravities. Zero when the pad has no motion sensors."),
    ("pad_motion_note", "Why gamepad motion sensors are missing, or empty when they were read."),
    ("set_pad_light", "Set the gamepad light bar to red, green and blue, each from 0 to 1. A pad with no light bar reports that and leaves the colour unchanged."),
    ("mouse_wheel", "Mouse wheel this step. Positive is away from the user."),
    ("pointer_world", "Where the pointer is in the flat scene, in scene units."),
    ("spectate", "Point the play camera at `entity` without changing who a player controls. Turn `on` off to follow the usual target again."),
    ("unpossess", "Player `index` (0 is the first) lets go of the object it controls."),
    ("rumble", "Rumble the gamepad: `strong` and `weak` motors from 0 to 1 for `duration` seconds; overlapping rumbles use the strongest."),
    ("rebind_action", "Wait for the player's next key or pad button and bind it to `action` in binding `slot` (0 is the first); Esc cancels. Fires the input_rebound event when done; the choice is saved for the player."),
    ("input_axis", "How far the input axis `name` is pushed this step, from -1 to 1; 0 when nothing drives it."),
    ("player", "The entity local player `index` controls (0 is the first player), or no entity."),
    ("find_first", "The live entity with the lowest id made from prefab `prefab` or one based on it, or no entity."),
    ("find_all", "Every live entity made from prefab `prefab` or one based on it, lowest id first."),
    ("set_window_mode", "Set the game window to `mode`: windowed, fullscreen, or borderless."),
    ("open_game_window", "Open another game window with `title`, `width`, `height`, `x`, `y`, optional `camera` entity, and optional `ui_screen` name. `id` 0 assigns the next id, starting at 1."),
    ("move_game_window", "Move open window `id` to `x` and `y` in screen pixels."),
    ("resize_game_window", "Resize open window `id` to `width` and `height` in pixels."),
    ("close_game_window", "Close open window `id`."),
    ("request_attention", "Flash the taskbar icon to ask for the player's attention."),
    ("offset_polygon", "Grow or shrink the outline `points` by `distance`. A positive distance grows a counter-clockwise outline. A shrink that passes the middle returns an empty list."),
    ("merge_polygon", "The outline of `a` and `b` joined into one shape."),
    ("clip_polygon", "The outline of `a` with `b` cut out of it."),
    ("intersect_polygon", "The outline of the overlap of `a` and `b`."),
    ("import_set_pivot", "After an import, set the picture pivot. x and y are 0 at the bottom left and 1 at the top right."),
    ("import_rename", "After an import, rename the file. The extension stays when the new name has none."),
    ("import_add_tag", "After an import, add a tag to the file."),
    ("import_set_text", "After an import, replace the file with this text."),
    ("import_path", "The project path of the file being imported."),
    ("import_kind", "The kind of the file being imported, such as picture or sound."),
    ("import_name", "The file name being imported."),
    ("open_file_dialog", "Open File Dialog: ask the player for a file. `path` is what they picked, or empty if they cancelled."),
    ("save_file_dialog", "Save File Dialog: ask the player where to write a file. `path` is what they picked, or empty if they cancelled."),
    ("encrypt", "Seal `text` with the project's key. The result is unreadable without that key."),
    ("decrypt", "Open text sealed by `encrypt`. Fails when the text was changed or the project key is wrong."),
    ("show_dialog", "Show the built-in dialog. `kind` is message, confirm, or input. The answer arrives as the dialog_answered event."),
    ("dialog_answer", "The button (yes, no, or ok) and the text from the last dialog_answered event."),
    ("cloud_upload", "Queue the save in `slot` for upload. A newer remote copy wins, and the job waits while offline."),
    ("cloud_download", "Queue a download of `slot`. A newer local copy is kept."),
    ("cloud_flush", "Try the cloud queue now. Jobs that cannot reach the provider stay queued."),
    ("cloud_reply", "Finish a script-hook cloud job. `saved_at` is the remote time; `body` is the remote save for a download."),
    ("cloud_job", "The cloud job a script hook should handle: kind, slot, saved_at, and the upload body."),
];

fn ins(list: &[(&str, Ty)]) -> Vec<(String, Ty)> {
    list.iter().map(|(n, t)| (n.to_string(), *t)).collect()
}

impl Program {
    /// For each event, whether a run of it can call it again before it ends.
    /// Such events keep each call's parameters in a frame of their own.
    pub fn reentrant(&self) -> Vec<bool> {
        let calls: Vec<Vec<usize>> = self
            .events
            .iter()
            .map(|e| {
                let mut seen = vec![false; self.nodes.len()];
                let mut todo = vec![e.node as usize];
                let mut out = Vec::new();
                while let Some(i) = todo.pop() {
                    if std::mem::replace(&mut seen[i], true) {
                        continue;
                    }
                    if let Op::Call(c) = self.nodes[i].op {
                        out.push(c as usize);
                    }
                    todo.extend(self.nodes[i].next.iter().flatten().map(|t| *t as usize));
                }
                out
            })
            .collect();
        (0..self.events.len())
            .map(|e| {
                let mut seen = vec![false; self.events.len()];
                let mut todo = calls[e].clone();
                while let Some(c) = todo.pop() {
                    if c == e {
                        return true;
                    }
                    if !std::mem::replace(&mut seen[c], true) {
                        todo.extend(&calls[c]);
                    }
                }
                false
            })
            .collect()
    }

    /// Check a graph and resolve it. Reports every problem it finds.
    pub fn build(graph: &Graph) -> Result<Program, Vec<BuildError>> {
        let check_recursion = function::needs_lower(graph);
        let lowered;
        let graph = if check_recursion {
            lowered = function::lower(graph)?;
            &lowered
        } else {
            graph
        };
        let mut errors = Vec::new();
        let mut index: BTreeMap<u32, u32> = BTreeMap::new();
        for (i, n) in graph.nodes.iter().enumerate() {
            if index.insert(n.id, i as u32).is_some() {
                errors.push(BuildError::node(n.id, "duplicate node id"));
            }
        }
        let mut var_names = BTreeSet::new();
        for v in &graph.vars {
            if !is_ident(&v.name) {
                errors.push(BuildError::general(format!(
                    "variable name `{}` must be a plain identifier",
                    v.name
                )));
            }
            if !var_names.insert(v.name.clone()) {
                errors.push(BuildError::general(format!("duplicate variable {}", v.name)));
            }
            if v.default.ty() != v.ty {
                errors.push(BuildError::general(format!(
                    "variable {} default is not a {}",
                    v.name, v.ty
                )));
            }
        }
        // Events first, so `call` nodes can find their parameters.
        let events = collect_events(graph, &index, &mut errors);
        // Resolve each node's operation and signature.
        let mut ops = Vec::new();
        let mut sigs = Vec::new();
        for n in &graph.nodes {
            match resolve(n, graph, &events) {
                Ok((op, sig)) => {
                    ops.push(op);
                    sigs.push(sig);
                }
                Err(e) => {
                    errors.push(e);
                    ops.push(Op::Const(Value::Int(0)));
                    sigs.push(Sig::default());
                }
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        // Links.
        let mut next: Vec<Vec<Option<u32>>> =
            sigs.iter().map(|s| vec![None; s.exec_outs.len()]).collect();
        let mut inputs: Vec<Vec<Option<Src>>> =
            sigs.iter().map(|s| vec![None; s.ins.len()]).collect();
        for l in &graph.links {
            let (Some(&fi), Some(&ti)) = (index.get(&l.from.node), index.get(&l.to.node)) else {
                errors.push(BuildError::general(format!("link {} names a missing node", l.text())));
                continue;
            };
            let (fs, ts) = (&sigs[fi as usize], &sigs[ti as usize]);
            if let Some(o) = fs.exec_outs.iter().position(|p| *p == l.from.pin) {
                if l.to.pin != "in" || !ts.exec_in {
                    errors.push(BuildError::node(
                        l.to.node,
                        format!("link {}: an execution output must go to `in`", l.text()),
                    ));
                } else if next[fi as usize][o].is_some() {
                    errors.push(BuildError::node(
                        l.from.node,
                        format!("`{}` is linked twice; add a sequence", l.from.pin),
                    ));
                } else {
                    next[fi as usize][o] = Some(ti);
                }
                continue;
            }
            let Some(o) = fs.outs.iter().position(|(p, _)| *p == l.from.pin) else {
                errors.push(BuildError::node(
                    l.from.node,
                    format!("{} has no output `{}`", graph.nodes[fi as usize].kind, l.from.pin),
                ));
                continue;
            };
            let Some(i) = ts.ins.iter().position(|(p, _)| *p == l.to.pin) else {
                errors.push(BuildError::node(
                    l.to.node,
                    format!("{} has no input `{}`", graph.nodes[ti as usize].kind, l.to.pin),
                ));
                continue;
            };
            let (ft, tt) = (fs.outs[o].1, ts.ins[i].1);
            if ft != tt {
                errors.push(BuildError::node(
                    l.to.node,
                    format!("link {}: {ft} into {tt}; add a conversion node", l.text()),
                ));
            } else if inputs[ti as usize][i].is_some() {
                errors.push(BuildError::node(
                    l.to.node,
                    format!("input `{}` is linked twice", l.to.pin),
                ));
            } else {
                inputs[ti as usize][i] = Some(Src::Pin { node: fi, pin: o as u32 });
            }
        }
        // Unlinked inputs take the node's `defaults` entry or the type's zero.
        let mut nodes = Vec::new();
        let mut slots = Vec::new();
        for (i, n) in graph.nodes.iter().enumerate() {
            let sig = &sigs[i];
            let mut resolved = Vec::new();
            for (k, (pin, ty)) in sig.ins.iter().enumerate() {
                resolved.push(match inputs[i][k].take() {
                    Some(s) => s,
                    None => match n.param("defaults").and_then(|d| d.get(pin)) {
                        Some(v) => match value_from_text(*ty, v) {
                            Some(v) => Src::Const(v),
                            None => {
                                errors.push(BuildError::node(
                                    n.id,
                                    format!("default for `{pin}` is not a {ty}"),
                                ));
                                Src::Const(ty.zero())
                            }
                        },
                        None if *ty == Ty::Entity => {
                            errors.push(BuildError::node(
                                n.id,
                                format!("`{pin}` needs an entity; link `self` or another entity"),
                            ));
                            Src::Const(ty.zero())
                        }
                        None => Src::Const(ty.zero()),
                    },
                });
            }
            let mut node_slots = Vec::new();
            if ops[i].is_exec() {
                for (_, ty) in &sig.outs {
                    node_slots.push(slots.len() as u32);
                    slots.push(*ty);
                }
            }
            nodes.push(PNode {
                id: n.id,
                op: ops[i].clone(),
                inputs: resolved,
                next: next[i].clone(),
                slots: node_slots,
                out_tys: sig.outs.iter().map(|(_, t)| *t).collect(),
            });
        }
        // Pure nodes must not loop back on themselves.
        let mut state = vec![0u8; nodes.len()];
        fn visit(i: usize, nodes: &[PNode], state: &mut [u8], errors: &mut Vec<BuildError>) {
            if nodes[i].op.is_exec() || state[i] == 2 {
                return;
            }
            if state[i] == 1 {
                errors.push(BuildError::node(nodes[i].id, "data links form a loop"));
                return;
            }
            state[i] = 1;
            for s in &nodes[i].inputs {
                if let Src::Pin { node, .. } = s {
                    visit(*node as usize, nodes, state, errors);
                }
            }
            state[i] = 2;
        }
        for i in 0..nodes.len() {
            visit(i, &nodes, &mut state, &mut errors);
        }
        let machine = match state_machine::compile(graph, &events) {
            Ok(m) => m,
            Err(more) => {
                errors.extend(more);
                None
            }
        };
        if !errors.is_empty() {
            errors.dedup();
            return Err(errors);
        }
        let program = Program {
            name: graph.name.clone(),
            vars: graph.vars.clone(),
            nodes,
            events,
            slots,
            machine,
        };
        if check_recursion {
            let recursed = function::recursion_errors(&program);
            if !recursed.is_empty() {
                return Err(recursed);
            }
        }
        Ok(program)
    }

    pub fn event(&self, name: &str) -> Option<usize> {
        self.events.iter().position(|e| e.name == name)
    }

    pub fn index_of(&self, id: u32) -> Option<u32> {
        self.nodes.iter().position(|n| n.id == id).map(|i| i as u32)
    }

    pub fn var(&self, name: &str) -> Option<usize> {
        self.vars.iter().position(|v| v.name == name)
    }
}

pub(crate) fn is_ident(s: &str) -> bool {
    let mut c = s.chars();
    matches!(c.next(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_')
        && c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn event_params(n: &Node) -> Result<Vec<(String, Ty)>, BuildError> {
    typed_list(n, "params")
}

/// A list of `{name, type}` maps, such as an event's `params` or `returns`.
fn typed_list(n: &Node, key: &str) -> Result<Vec<(String, Ty)>, BuildError> {
    let mut out = Vec::new();
    match n.param(key) {
        None => {}
        Some(SealValue::Array(items)) => {
            for p in items {
                let (Some(SealValue::String(name)), Some(SealValue::String(ty))) =
                    (p.get("name"), p.get("type"))
                else {
                    return Err(BuildError::node(
                        n.id,
                        format!("each entry of `{key}` needs `name` and `type`"),
                    ));
                };
                let ty = Ty::parse(ty)
                    .ok_or_else(|| BuildError::node(n.id, format!("unknown type {ty}")))?;
                if !is_ident(name) || name == "then" {
                    return Err(BuildError::node(n.id, format!("bad parameter name `{name}`")));
                }
                out.push((name.clone(), ty));
            }
        }
        Some(_) => return Err(BuildError::node(n.id, format!("`{key}` must be a list"))),
    }
    Ok(out)
}

/// Every event node, in graph order. Problems go to `errors`.
fn collect_events(
    graph: &Graph,
    index: &BTreeMap<u32, u32>,
    errors: &mut Vec<BuildError>,
) -> Vec<EventInfo> {
    let mut events: Vec<EventInfo> = Vec::new();
    for n in &graph.nodes {
        if n.kind != "event" {
            continue;
        }
        let name = match n.str_param("name") {
            Ok(s) => s.to_string(),
            Err(e) => {
                errors.push(e);
                continue;
            }
        };
        if !is_ident(&name) {
            errors.push(BuildError::node(
                n.id,
                format!("event name `{name}` must be a plain identifier"),
            ));
        }
        let params = match name.as_str() {
            "start" => Vec::new(),
            "tick" | "fixed_tick" | "late_tick" => vec![("dt".to_string(), Ty::Float)],
            _ => match event_params(n) {
                Ok(p) => p,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            },
        };
        let returns = match typed_list(n, "returns") {
            Ok(r) => r,
            Err(e) => {
                errors.push(e);
                continue;
            }
        };
        if events.iter().any(|e| e.name == name) {
            errors.push(BuildError::node(n.id, format!("a second `{name}` event")));
            continue;
        }
        events.push(EventInfo { name, node: index[&n.id], params, returns });
    }
    events
}

/// A new node of `kind` with the settings it needs to work, picked to fit `graph`:
/// a fresh event name, the first variable, `float` for typed math, and so on.
pub fn node_template(graph: &Graph, id: u32, kind: &str) -> Node {
    let text = |s: &str| SealValue::String(s.into());
    let node = Node::new(id, kind);
    let first_var = graph.vars.first().map(|v| v.name.clone()).unwrap_or_else(|| "value".into());
    let events: Vec<String> = graph
        .nodes
        .iter()
        .filter(|n| n.kind == "event")
        .filter_map(|n| match n.param("name") {
            Some(SealValue::String(s)) => Some(s.clone()),
            _ => None,
        })
        .collect();
    let own_event = events
        .iter()
        .find(|e| *e != "start" && *e != "tick")
        .cloned()
        .unwrap_or_else(|| "my_event".into());
    match kind {
        "event" => {
            let name = ["start", "tick"]
                .into_iter()
                .map(String::from)
                .chain((1..).map(|i| {
                    if i == 1 {
                        "my_event".to_string()
                    } else {
                        format!("my_event{i}")
                    }
                }))
                .find(|n| !events.contains(n))
                .unwrap_or_default();
            node.with("name", text(&name))
        }
        "sequence" => node.with("count", SealValue::Number("2".into())),
        "set" | "get" => node.with("name", text(&first_var)),
        "set_window_mode" => node.with(
            "defaults",
            SealValue::Map(SealMap::new(vec![("mode".into(), text("windowed"))])),
        ),
        "open_game_window" => node.with(
            "defaults",
            SealValue::Map(SealMap::new(vec![
                ("id".into(), SealValue::Number("0".into())),
                ("title".into(), text("Window")),
                ("width".into(), SealValue::Number("640".into())),
                ("height".into(), SealValue::Number("480".into())),
                ("x".into(), SealValue::Number("100".into())),
                ("y".into(), SealValue::Number("100".into())),
                ("camera".into(), text("none")),
                ("ui_screen".into(), text("")),
            ])),
        ),
        "set_state" => {
            let state = graph
                .machine
                .as_ref()
                .and_then(|m| m.states.first().map(|s| s.name.clone()))
                .unwrap_or_else(|| "idle".into());
            node.with(
                "defaults",
                SealValue::Map(SealMap::new(vec![("state".into(), text(&state))])),
            )
        }
        "trigger" => {
            node.with("defaults", SealValue::Map(SealMap::new(vec![("event".into(), text("go"))])))
        }
        "call" | "set_timer" | "clear_timer" => node.with("name", text(&own_event)),
        "world_set" | "world_get" => node
            .with("component", text("agent"))
            .with("field", text("value"))
            .with("type", text("float")),
        "spectate" => node.with(
            "defaults",
            SealValue::Map(SealMap::new(vec![("on".into(), SealValue::Bool(true))])),
        ),
        "const" => node.with("type", text("float")).with("value", SealValue::Number("0.0".into())),
        "compare" => node.with("type", text("float")).with("op", text("lt")),
        "add" | "sub" | "mul" | "div" | "min" | "max" | "neg" | "abs" | "select" | "to_str"
        | "num_text" => node.with("type", text("float")),
        "vec_text" => node.with("type", text("vec2")),
        "offset_polygon" => node.with(
            "defaults",
            SealValue::Map(SealMap::new(vec![(
                "distance".into(),
                SealValue::Number("0.1".into()),
            )])),
        ),
        "spawn" | "is_a" | "find_first" | "find_all" => node.with("prefab", text("thing")),
        "import_set_pivot" => node
            .with("x", SealValue::Number("0.5".into()))
            .with("y", SealValue::Number("0.5".into())),
        "import_rename" => node.with("name", text("")),
        "import_add_tag" => node.with("tag", text("")),
        "import_set_text" => node.with("text", text("")),
        "open_file_dialog" => node.with(
            "defaults",
            SealValue::Map(SealMap::new(vec![
                ("title".into(), text("Open")),
                ("filter".into(), text("")),
            ])),
        ),
        "save_file_dialog" => node.with(
            "defaults",
            SealValue::Map(SealMap::new(vec![
                ("title".into(), text("Save")),
                ("name".into(), text("save.txt")),
            ])),
        ),
        "show_dialog" => node.with(
            "defaults",
            SealValue::Map(SealMap::new(vec![
                ("kind".into(), text("confirm")),
                ("title".into(), text("Confirm")),
                ("message".into(), text("Are you sure?")),
                ("placeholder".into(), text("")),
                ("theme".into(), text("")),
            ])),
        ),
        "cloud_upload" | "cloud_download" | "cloud_reply" => node.with(
            "defaults",
            SealValue::Map(SealMap::new(vec![("slot".into(), text("Slot 1"))])),
        ),
        _ => node,
    }
}

/// The pins node `id` has in `graph`, for editors. Works on graphs that do not build yet.
pub fn node_sig(graph: &Graph, id: u32) -> Result<Sig, BuildError> {
    if let Some(sig) = function::sig_override(graph, id) {
        return sig;
    }
    let node = function::find_node(graph, id)
        .ok_or_else(|| BuildError::general(format!("no node {id}")))?;
    let index: BTreeMap<u32, u32> =
        graph.nodes.iter().enumerate().map(|(i, n)| (n.id, i as u32)).collect();
    let events = collect_events(graph, &index, &mut Vec::new());
    resolve(node, graph, &events).map(|(_, sig)| sig)
}

fn resolve(n: &Node, g: &Graph, events: &[EventInfo]) -> Result<(Op, Sig), BuildError> {
    use Ty::*;
    let flow = |outs: &[&str], i: Vec<(String, Ty)>, o: Vec<(String, Ty)>| Sig {
        exec_in: true,
        exec_outs: outs.iter().map(|s| s.to_string()).collect(),
        ins: i,
        outs: o,
    };
    let pure = |i: Vec<(String, Ty)>, out: Ty| Sig {
        exec_in: false,
        exec_outs: Vec::new(),
        ins: i,
        outs: vec![("out".into(), out)],
    };
    let many = |i: Vec<(String, Ty)>, o: Vec<(&str, Ty)>| Sig {
        exec_in: false,
        exec_outs: Vec::new(),
        ins: i,
        outs: o.into_iter().map(|(name, ty)| (name.to_string(), ty)).collect(),
    };
    let var = |n: &Node| -> Result<(u32, Ty), BuildError> {
        let name = n.str_param("name")?;
        g.vars
            .iter()
            .position(|v| v.name == name)
            .map(|i| (i as u32, g.vars[i].ty))
            .ok_or_else(|| BuildError::node(n.id, format!("no variable {name}")))
    };
    let field = |n: &Node| -> Result<(String, String, Ty), BuildError> {
        Ok((
            n.str_param("component")?.to_string(),
            n.str_param("field")?.to_string(),
            n.ty_param()?,
        ))
    };
    let need = |ty: Ty, allowed: &[Ty]| -> Result<Ty, BuildError> {
        if allowed.contains(&ty) {
            Ok(ty)
        } else {
            let names: Vec<_> = allowed.iter().map(|t| t.name()).collect();
            Err(BuildError::node(
                n.id,
                format!("{} works on {}, not {ty}", n.kind, names.join(", ")),
            ))
        }
    };
    let ab = |t: Ty| ins(&[("a", t), ("b", t)]);
    Ok(match n.kind.as_str() {
        "event" => {
            let name = n.str_param("name")?.to_string();
            let info = events.iter().find(|e| e.name == name).expect("events are collected first");
            let sig = Sig {
                exec_in: false,
                exec_outs: vec!["then".into()],
                ins: Vec::new(),
                outs: info.params.clone(),
            };
            (Op::Event { name }, sig)
        }
        "branch" => (Op::Branch, flow(&["true", "false"], ins(&[("cond", Bool)]), vec![])),
        "sequence" => {
            let count = n
                .param("count")
                .and_then(num)
                .and_then(|c| c.parse::<u32>().ok())
                .filter(|c| (1..=64).contains(c))
                .ok_or_else(|| BuildError::node(n.id, "sequence needs `count` from 1 to 64"))?;
            let outs: Vec<String> = (0..count).map(|i| format!("then{i}")).collect();
            let refs: Vec<&str> = outs.iter().map(|s| s.as_str()).collect();
            (Op::Sequence(count), flow(&refs, vec![], vec![]))
        }
        "for" => (
            Op::For,
            flow(
                &["body", "done"],
                ins(&[("first", Int), ("last", Int), ("stop", Bool)]),
                ins(&[("index", Int)]),
            ),
        ),
        "delay" => (Op::Delay, flow(&["then"], ins(&[("seconds", Float)]), vec![])),
        "set" => {
            let (v, t) = var(n)?;
            (Op::Set(v), flow(&["then"], ins(&[("value", t)]), ins(&[("value", t)])))
        }
        "call" => {
            let name = n.str_param("name")?;
            let e = events
                .iter()
                .position(|e| e.name == name)
                .ok_or_else(|| BuildError::node(n.id, format!("no event {name} to call")))?;
            (
                Op::Call(e as u32),
                flow(&["then"], events[e].params.clone(), events[e].returns.clone()),
            )
        }
        "set_timer" | "clear_timer" => {
            let name = n.str_param("name")?;
            let e = events
                .iter()
                .position(|e| e.name == name)
                .ok_or_else(|| BuildError::node(n.id, format!("no event {name} for a timer")))?;
            if !events[e].params.is_empty() {
                return Err(BuildError::node(
                    n.id,
                    format!("a timer can only run an event without parameters; {name} has some"),
                ));
            }
            if n.kind == "set_timer" {
                (
                    Op::SetTimer(e as u32),
                    flow(&["then"], ins(&[("seconds", Float), ("looping", Bool)]), vec![]),
                )
            } else {
                (Op::ClearTimer(e as u32), flow(&["then"], vec![], vec![]))
            }
        }
        "return" => {
            let name = n.str_param("name")?;
            let e = events
                .iter()
                .position(|e| e.name == name)
                .ok_or_else(|| BuildError::node(n.id, format!("no event {name} to return from")))?;
            (Op::Return(e as u32), flow(&[], events[e].returns.clone(), vec![]))
        }
        "log" => (Op::Log, flow(&["then"], ins(&[("message", Str)]), vec![])),
        "world_set" => {
            let (component, field, ty) = field(n)?;
            (
                Op::WorldSet { component, field, ty },
                flow(&["then"], ins(&[("entity", Entity), ("value", ty)]), vec![]),
            )
        }
        "const" => {
            let ty = n.ty_param()?;
            let v = n
                .param("value")
                .and_then(|v| value_from_text(ty, v))
                .ok_or_else(|| BuildError::node(n.id, format!("const needs a {ty} `value`")))?;
            (Op::Const(v), pure(vec![], ty))
        }
        "get" => {
            let (v, t) = var(n)?;
            (Op::Get(v), pure(vec![], t))
        }
        "set_state" => (Op::SetState, flow(&["then"], ins(&[("state", Str)]), vec![])),
        "get_state" => (Op::GetState, pure(vec![], Str)),
        "time_in_state" => (Op::TimeInState, pure(vec![], Float)),
        "trigger" => (Op::Trigger, flow(&["then"], ins(&[("event", Str)]), vec![])),
        "self" => (Op::SelfRef, pure(vec![], Entity)),
        "game_time" => (Op::GameTime, pure(vec![], Float)),
        "frame_dt" => (Op::FrameDt, pure(vec![], Float)),
        "random_int" => (Op::RandomInt, pure(ab(Int), Int)),
        "random_float" => (Op::RandomFloat, pure(ab(Float), Float)),
        "add" | "sub" => {
            let t = need(n.ty_param()?, &[Int, Float, Vec2, Vec3])?;
            let op = if n.kind == "add" { BinOp::Add } else { BinOp::Sub };
            (Op::Bin(op, t), pure(ab(t), t))
        }
        "mul" | "div" | "min" | "max" => {
            let t = need(n.ty_param()?, &[Int, Float])?;
            let op = match n.kind.as_str() {
                "mul" => BinOp::Mul,
                "div" => BinOp::Div,
                "min" => BinOp::Min,
                _ => BinOp::Max,
            };
            (Op::Bin(op, t), pure(ab(t), t))
        }
        "mod" => {
            let t = need(n.ty_param_or(Int)?, &[Int, Float])?;
            (Op::Bin(BinOp::Mod, t), pure(ab(t), t))
        }
        "and" => (Op::Bin(BinOp::And, Bool), pure(ab(Bool), Bool)),
        "or" => (Op::Bin(BinOp::Or, Bool), pure(ab(Bool), Bool)),
        "concat" => (Op::Bin(BinOp::Concat, Str), pure(ab(Str), Str)),
        "str_contains" => (Op::Bin(BinOp::StrContains, Str), pure(ab(Str), Bool)),
        "str_starts" => (Op::Bin(BinOp::StrStarts, Str), pure(ab(Str), Bool)),
        "str_ends" => (Op::Bin(BinOp::StrEnds, Str), pure(ab(Str), Bool)),
        "not" => (Op::Un(UnOp::Not, Bool), pure(ins(&[("a", Bool)]), Bool)),
        "neg" => {
            let t = need(n.ty_param()?, &[Int, Float, Vec2, Vec3])?;
            (Op::Un(UnOp::Neg, t), pure(ins(&[("a", t)]), t))
        }
        "abs" => {
            let t = need(n.ty_param()?, &[Int, Float])?;
            (Op::Un(UnOp::Abs, t), pure(ins(&[("a", t)]), t))
        }
        "sqrt" => (Op::Un(UnOp::Sqrt, Float), pure(ins(&[("a", Float)]), Float)),
        "str_len" => (Op::Un(UnOp::StrLen, Str), pure(ins(&[("a", Str)]), Int)),
        "str_upper" => (Op::Un(UnOp::StrUpper, Str), pure(ins(&[("a", Str)]), Str)),
        "str_lower" => (Op::Un(UnOp::StrLower, Str), pure(ins(&[("a", Str)]), Str)),
        "str_trim" => (Op::Un(UnOp::StrTrim, Str), pure(ins(&[("a", Str)]), Str)),
        "str_trim_start" => (Op::Un(UnOp::StrTrimStart, Str), pure(ins(&[("a", Str)]), Str)),
        "str_trim_end" => (Op::Un(UnOp::StrTrimEnd, Str), pure(ins(&[("a", Str)]), Str)),
        "parse_int" => (Op::Un(UnOp::ParseInt, Str), pure(ins(&[("a", Str)]), Int)),
        "parse_float" => (Op::Un(UnOp::ParseFloat, Str), pure(ins(&[("a", Str)]), Float)),
        "to_float" => (Op::Un(UnOp::ToFloat, Int), pure(ins(&[("a", Int)]), Float)),
        "to_int" => (Op::Un(UnOp::ToInt, Float), pure(ins(&[("a", Float)]), Int)),
        "num_text" => {
            let t = n.ty_param()?;
            if !matches!(t, Int | Float) {
                return Err(BuildError::node(n.id, "num_text works on int or float"));
            }
            (Op::Un(UnOp::NumText, t), pure(ins(&[("a", t)]), Str))
        }
        "float_text" => (Op::Un(UnOp::FloatText, Float), pure(ins(&[("a", Float)]), Str)),
        "vec_text" => {
            let t = n.ty_param()?;
            if !matches!(t, Vec2 | Vec3) {
                return Err(BuildError::node(n.id, "vec_text works on vec2 or vec3"));
            }
            (Op::Un(UnOp::VecText, t), pure(ins(&[("a", t)]), Str))
        }
        "to_str" => {
            let t = n.ty_param()?;
            if t.is_list() {
                return Err(BuildError::node(n.id, "to_str works on plain values, not lists"));
            }
            (Op::Un(UnOp::ToStr, t), pure(ins(&[("a", t)]), Str))
        }
        "vec2_x" => (Op::Un(UnOp::Vec2X, Vec2), pure(ins(&[("v", Vec2)]), Float)),
        "vec2_y" => (Op::Un(UnOp::Vec2Y, Vec2), pure(ins(&[("v", Vec2)]), Float)),
        "vec3_x" => (Op::Un(UnOp::Vec3X, Vec3), pure(ins(&[("v", Vec3)]), Float)),
        "vec3_y" => (Op::Un(UnOp::Vec3Y, Vec3), pure(ins(&[("v", Vec3)]), Float)),
        "vec3_z" => (Op::Un(UnOp::Vec3Z, Vec3), pure(ins(&[("v", Vec3)]), Float)),
        "length" => {
            let t = need(n.ty_param_or(Vec2)?, &[Vec2, Vec3])?;
            (Op::Un(UnOp::Length, t), pure(ins(&[("v", t)]), Float))
        }
        "normalize" => {
            let t = need(n.ty_param_or(Vec2)?, &[Vec2, Vec3])?;
            (Op::Un(UnOp::Normalize, t), pure(ins(&[("v", t)]), t))
        }
        "dot" => {
            let t = need(n.ty_param_or(Vec2)?, &[Vec2, Vec3])?;
            (Op::Bin(BinOp::Dot, t), pure(ab(t), Float))
        }
        "cross" => (Op::Bin(BinOp::Cross, Vec3), pure(ab(Vec3), Vec3)),
        "sin" => (Op::Un(UnOp::Sin, Float), pure(ins(&[("a", Float)]), Float)),
        "cos" => (Op::Un(UnOp::Cos, Float), pure(ins(&[("a", Float)]), Float)),
        "exp" => (Op::Un(UnOp::Exp, Float), pure(ins(&[("a", Float)]), Float)),
        "ln" => (Op::Un(UnOp::Ln, Float), pure(ins(&[("a", Float)]), Float)),
        "atan" => (Op::Un(UnOp::Atan, Float), pure(ins(&[("a", Float)]), Float)),
        "asin" => (Op::Un(UnOp::Asin, Float), pure(ins(&[("a", Float)]), Float)),
        "acos" => (Op::Un(UnOp::Acos, Float), pure(ins(&[("a", Float)]), Float)),
        "pow" => (Op::Bin(BinOp::Pow, Float), pure(ab(Float), Float)),
        "atan2" => (Op::Bin(BinOp::Atan2, Float), pure(ab(Float), Float)),
        "floor" => (Op::Un(UnOp::Floor, Float), pure(ins(&[("a", Float)]), Int)),
        "round" => (Op::Un(UnOp::Round, Float), pure(ins(&[("a", Float)]), Int)),
        "ceil" => (Op::Un(UnOp::Ceil, Float), pure(ins(&[("a", Float)]), Int)),
        "vec2" => (Op::MakeVec2, pure(ins(&[("x", Float), ("y", Float)]), Vec2)),
        "vec3" => (Op::MakeVec3, pure(ins(&[("x", Float), ("y", Float), ("z", Float)]), Vec3)),
        "scale" => {
            let t = need(n.ty_param_or(Vec2)?, &[Vec2, Vec3])?;
            (Op::Scale(t), pure(ins(&[("v", t), ("s", Float)]), t))
        }
        "compare" => {
            let t = n.ty_param()?;
            let op_name = n.str_param("op")?;
            let op = CmpOp::parse(op_name)
                .ok_or_else(|| BuildError::node(n.id, format!("unknown compare op {op_name}")))?;
            if matches!(op, CmpOp::Lt | CmpOp::Le | CmpOp::Gt | CmpOp::Ge) {
                need(t, &[Int, Float])?;
            }
            if t.is_list() {
                return Err(BuildError::node(n.id, "compare works on plain values, not lists"));
            }
            (Op::Compare(op, t), pure(ab(t), Bool))
        }
        "select" => {
            let t = n.ty_param()?;
            (Op::Select(t), pure(ins(&[("cond", Bool), ("a", t), ("b", t)]), t))
        }
        "world_get" => {
            let (component, field, ty) = field(n)?;
            (Op::WorldGet { component, field, ty }, pure(ins(&[("entity", Entity)]), ty))
        }
        "spawn" => {
            let prefab = n.str_param("prefab")?.to_string();
            (Op::Spawn { prefab }, flow(&["then"], vec![], ins(&[("entity", Entity)])))
        }
        "play_sequence" => (Op::PlaySequence, flow(&["then"], ins(&[("sequence", Str)]), vec![])),
        "play_anim" => {
            (Op::PlayAnim, flow(&["then"], ins(&[("entity", Entity), ("anim", Str)]), vec![]))
        }
        "ai_move_to" => (
            Op::Ai(rt::AiKind::MoveTo),
            flow(&["then"], ins(&[("entity", Entity), ("target", Vec3), ("speed", Float)]), vec![]),
        ),
        "ai_chase" => (
            Op::Ai(rt::AiKind::Chase),
            flow(
                &["then"],
                ins(&[("entity", Entity), ("target", Entity), ("speed", Float)]),
                vec![],
            ),
        ),
        "ai_patrol" => (
            Op::Ai(rt::AiKind::Patrol),
            flow(
                &["then"],
                ins(&[("entity", Entity), ("a", Vec3), ("b", Vec3), ("speed", Float)]),
                vec![],
            ),
        ),
        "ai_stop" => {
            (Op::Ai(rt::AiKind::Stop), flow(&["then"], ins(&[("entity", Entity)]), vec![]))
        }
        "ai_arrived" => (Op::AiArrived, pure(ins(&[("entity", Entity)]), Bool)),
        "possess" => (
            Op::Game(rt::GameKind::Possess),
            flow(&["then"], ins(&[("index", Int), ("entity", Entity)]), vec![]),
        ),
        "spawn_player" => {
            (Op::Game(rt::GameKind::SpawnPlayer), flow(&["then"], ins(&[("index", Int)]), vec![]))
        }
        "add_score" => (
            Op::Game(rt::GameKind::AddScore),
            flow(&["then"], ins(&[("name", Str), ("amount", Int)]), vec![]),
        ),
        "set_game_state" => {
            (Op::Game(rt::GameKind::SetState), flow(&["then"], ins(&[("state", Str)]), vec![]))
        }
        "set_window_mode" => {
            (Op::Game(rt::GameKind::SetWindowMode), flow(&["then"], ins(&[("mode", Str)]), vec![]))
        }
        "open_game_window" => (
            Op::Game(rt::GameKind::OpenGameWindow),
            flow(
                &["then"],
                ins(&[
                    ("id", Int),
                    ("title", Str),
                    ("width", Int),
                    ("height", Int),
                    ("x", Int),
                    ("y", Int),
                    ("camera", Entity),
                    ("ui_screen", Str),
                ]),
                vec![],
            ),
        ),
        "move_game_window" => (
            Op::Game(rt::GameKind::MoveGameWindow),
            flow(&["then"], ins(&[("id", Int), ("x", Int), ("y", Int)]), vec![]),
        ),
        "resize_game_window" => (
            Op::Game(rt::GameKind::ResizeGameWindow),
            flow(&["then"], ins(&[("id", Int), ("width", Int), ("height", Int)]), vec![]),
        ),
        "close_game_window" => (
            Op::Game(rt::GameKind::CloseGameWindow),
            flow(&["then"], ins(&[("id", Int)]), vec![]),
        ),
        "request_attention" => {
            (Op::Game(rt::GameKind::RequestAttention), flow(&["then"], ins(&[]), vec![]))
        }
        "ui_value" => (Op::UiValue, pure(ins(&[("element", Str)]), Float)),
        "ui_input" => (Op::UiInput, pure(ins(&[("element", Str)]), Str)),
        "score" => (Op::Score, pure(ins(&[("name", Str)]), Int)),
        "game_state" => (Op::GameState, pure(ins(&[]), Str)),
        "save_game" => {
            (Op::Game(rt::GameKind::SaveGame), flow(&["then"], ins(&[("slot", Str)]), vec![]))
        }
        "open_file_dialog" => (
            Op::OpenFileDialog,
            flow(&["then"], ins(&[("title", Str), ("filter", Str)]), ins(&[("path", Str)])),
        ),
        "save_file_dialog" => (
            Op::SaveFileDialog,
            flow(&["then"], ins(&[("title", Str), ("name", Str)]), ins(&[("path", Str)])),
        ),
        "encrypt" => (Op::Encrypt, flow(&["then"], ins(&[("text", Str)]), ins(&[("text", Str)]))),
        "decrypt" => (Op::Decrypt, flow(&["then"], ins(&[("data", Str)]), ins(&[("text", Str)]))),
        "show_dialog" => (
            Op::ShowDialog,
            flow(
                &["then"],
                ins(&[
                    ("kind", Str),
                    ("title", Str),
                    ("message", Str),
                    ("placeholder", Str),
                    ("theme", Str),
                ]),
                vec![],
            ),
        ),
        "dialog_answer" => (
            Op::DialogAnswer,
            many(vec![], vec![("button", Str), ("text", Str)]),
        ),
        "cloud_upload" => {
            (Op::CloudUpload, flow(&["then"], ins(&[("slot", Str)]), vec![]))
        }
        "cloud_download" => {
            (Op::CloudDownload, flow(&["then"], ins(&[("slot", Str)]), vec![]))
        }
        "cloud_flush" => (Op::CloudFlush, flow(&["then"], vec![], vec![])),
        "cloud_reply" => (
            Op::CloudReply,
            flow(&["then"], ins(&[("slot", Str), ("saved_at", Int), ("body", Str)]), vec![]),
        ),
        "cloud_job" => (
            Op::CloudJob,
            many(vec![], vec![("kind", Str), ("slot", Str), ("saved_at", Int), ("body", Str)]),
        ),
        "load_game" => {
            (Op::Game(rt::GameKind::LoadGame), flow(&["then"], ins(&[("slot", Str)]), vec![]))
        }
        "delete_save" => {
            (Op::Game(rt::GameKind::DeleteSave), flow(&["then"], ins(&[("slot", Str)]), vec![]))
        }
        "set_saved" => (
            Op::Game(rt::GameKind::SetSaved),
            flow(&["then"], ins(&[("key", Str), ("value", Str)]), vec![]),
        ),
        "has_save" => (Op::HasSave, pure(ins(&[("slot", Str)]), Bool)),
        "saved" => (Op::Saved, pure(ins(&[("key", Str)]), Str)),
        "open_scene" => {
            (Op::Game(rt::GameKind::OpenScene), flow(&["then"], ins(&[("path", Str)]), vec![]))
        }
        "add_scene" => {
            (Op::Game(rt::GameKind::AddScene), flow(&["then"], ins(&[("path", Str)]), vec![]))
        }
        "load_scene_async" => (
            Op::Game(rt::GameKind::LoadSceneAsync),
            flow(&["then"], ins(&[("path", Str), ("additive", Bool)]), vec![]),
        ),
        "unload_scene" => {
            (Op::Game(rt::GameKind::UnloadScene), flow(&["then"], ins(&[("path", Str)]), vec![]))
        }
        "keep_across_scenes" => (
            Op::Game(rt::GameKind::KeepAcrossScenes),
            flow(&["then"], ins(&[("entity", Entity), ("keep", Bool)]), vec![]),
        ),
        "scene_loaded" => (Op::SceneLoaded, pure(ins(&[("path", Str)]), Bool)),
        "current_scene" => (Op::CurrentScene, pure(ins(&[]), Str)),
        "load_data" => {
            (Op::LoadData, flow(&["then"], ins(&[("path", Str)]), vec![("path".to_string(), Str)]))
        }
        "get_field" => (Op::DataField, pure(ins(&[("path", Str), ("field", Str)]), Str)),
        "read_csv_cell" => (
            Op::ReadCsvCell,
            pure(ins(&[("path", Str), ("row", Int), ("col", Int)]), Str),
        ),
        "read_json_value" => (
            Op::ReadJsonValue,
            pure(ins(&[("path", Str), ("json_path", Str)]), Str),
        ),
        "quat_euler" => (
            Op::ScriptCore(script_core::ScriptCoreOp::QuatEuler),
            pure(
                ins(&[("pitch", Float), ("yaw", Float), ("roll", Float)]),
                Ty::Quat,
            ),
        ),
        "quat_axis_angle" => (
            Op::ScriptCore(script_core::ScriptCoreOp::QuatAxisAngle),
            pure(ins(&[("axis", Vec3), ("angle", Float)]), Ty::Quat),
        ),
        "quat_mul" => (
            Op::ScriptCore(script_core::ScriptCoreOp::QuatMul),
            pure(ab(Ty::Quat), Ty::Quat),
        ),
        "quat_slerp" => (
            Op::ScriptCore(script_core::ScriptCoreOp::QuatSlerp),
            pure(ins(&[("a", Ty::Quat), ("b", Ty::Quat), ("t", Float)]), Ty::Quat),
        ),
        "quat_rotate_vec" => (
            Op::ScriptCore(script_core::ScriptCoreOp::QuatRotateVec),
            pure(ins(&[("rotation", Ty::Quat), ("vector", Vec3)]), Vec3),
        ),
        "plane_point_normal" => (
            Op::ScriptCore(script_core::ScriptCoreOp::PlanePointNormal),
            pure(ins(&[("point", Vec3), ("normal", Vec3)]), Ty::Plane),
        ),
        "plane_distance" => (
            Op::ScriptCore(script_core::ScriptCoreOp::PlaneDistance),
            pure(ins(&[("plane", Ty::Plane), ("point", Vec3)]), Float),
        ),
        "plane_ray_hit" => (
            Op::ScriptCore(script_core::ScriptCoreOp::PlaneRayHit),
            pure(
                ins(&[("plane", Ty::Plane), ("origin", Vec3), ("direction", Vec3)]),
                Float,
            ),
        ),
        "utf8_to_utf16" => (
            Op::ScriptCore(script_core::ScriptCoreOp::Utf8ToUtf16),
            pure(ins(&[("text", Str)]), Str),
        ),
        "utf16_to_utf8" => (
            Op::ScriptCore(script_core::ScriptCoreOp::Utf16ToUtf8),
            pure(ins(&[("units", Str)]), Str),
        ),
        "layout_lines" => (
            Op::ScriptCore(script_core::ScriptCoreOp::LayoutLines),
            pure(ins(&[("text", Str), ("width", Float), ("font_size", Float)]), Str),
        ),
        "read_typed_json" => (
            Op::ScriptCore(script_core::ScriptCoreOp::ReadTypedJson),
            pure(ins(&[("json", Str), ("type_path", Str)]), Str),
        ),
        "write_typed_json" => (
            Op::ScriptCore(script_core::ScriptCoreOp::WriteTypedJson),
            pure(ins(&[("data_path", Str)]), Str),
        ),
        "unscaled_time" => (
            Op::ScriptCore(script_core::ScriptCoreOp::UnscaledTime),
            pure(vec![], Float),
        ),
        "unscaled_dt" => (
            Op::ScriptCore(script_core::ScriptCoreOp::UnscaledDt),
            pure(vec![], Float),
        ),
        "sim_tick" => (Op::ScriptCore(script_core::ScriptCoreOp::SimTick), pure(vec![], Int)),
        "stop_anim" => (Op::StopAnim, flow(&["then"], ins(&[("entity", Entity)]), vec![])),
        "play_clip" => {
            (Op::PlayClip, flow(&["then"], ins(&[("entity", Entity), ("clip", Str)]), vec![]))
        }
        "apply_impulse" => (
            Op::ApplyImpulse,
            flow(&["then"], ins(&[("entity", Entity), ("impulse", Vec3)]), vec![]),
        ),
        "move_character" => (
            Op::Character(rt::CharacterKind::Move),
            flow(&["then"], ins(&[("entity", Entity), ("wish", Vec2)]), vec![]),
        ),
        "jump" => (
            Op::Character(rt::CharacterKind::Jump),
            flow(&["then"], ins(&[("entity", Entity)]), vec![]),
        ),
        "is_on_floor" => (
            Op::CharacterQuery(rt::CharacterQuery::OnFloor),
            pure(ins(&[("entity", Entity)]), Bool),
        ),
        "is_on_wall" => {
            (Op::CharacterQuery(rt::CharacterQuery::OnWall), pure(ins(&[("entity", Entity)]), Bool))
        }
        "set_movement_mode" => (
            Op::Character(rt::CharacterKind::SetMode),
            flow(&["then"], ins(&[("entity", Entity), ("mode", Str)]), vec![]),
        ),
        "get_velocity" => (
            Op::CharacterQuery(rt::CharacterQuery::Velocity),
            pure(ins(&[("entity", Entity)]), Vec2),
        ),
        "apply_force_2d" => {
            (Op::ApplyForce2d, flow(&["then"], ins(&[("entity", Entity), ("force", Vec2)]), vec![]))
        }
        "apply_torque_2d" => (
            Op::ApplyTorque2d,
            flow(&["then"], ins(&[("entity", Entity), ("torque", Float)]), vec![]),
        ),
        "set_velocity_2d" => (
            Op::SetVelocity2d,
            flow(
                &["then"],
                ins(&[("entity", Entity), ("linear", Vec2), ("angular", Float)]),
                vec![],
            ),
        ),
        "set_collider_shape" => (
            Op::SetColliderShape,
            flow(
                &["then"],
                ins(&[
                    ("entity", Entity),
                    ("shape", Str),
                    ("size", Vec2),
                    ("points", Ty::List(Elem::Vec2)),
                ]),
                vec![],
            ),
        ),
        "set_collider_enabled" => (
            Op::SetColliderEnabled,
            flow(&["then"], ins(&[("entity", Entity), ("enabled", Bool)]), vec![]),
        ),
        "set_joint_target_2d" => (
            Op::SetJointTarget2d,
            flow(&["then"], ins(&[("entity", Entity), ("target", Vec2)]), vec![]),
        ),
        "get_mass_2d" => (Op::Mass2d, pure(ins(&[("entity", Entity)]), Float)),
        "physics_body" => (
            Op::Server(ServerCall::Body),
            flow(
                &["then"],
                ins(&[("world", Str), ("x", Float), ("y", Float), ("z", Float), ("dynamic", Bool)]),
                ins(&[("body", Float)]),
            ),
        ),
        "physics_shape" => (
            Op::Server(ServerCall::Shape),
            flow(
                &["then"],
                ins(&[
                    ("world", Str),
                    ("body", Float),
                    ("kind", Str),
                    ("a", Float),
                    ("b", Float),
                    ("c", Float),
                    ("mesh", Str),
                ]),
                ins(&[("shape", Float)]),
            ),
        ),
        "physics_joint" => (
            Op::Server(ServerCall::Joint),
            flow(
                &["then"],
                ins(&[
                    ("world", Str),
                    ("kind", Str),
                    ("body_a", Float),
                    ("body_b", Float),
                    ("length", Float),
                ]),
                ins(&[("joint", Float)]),
            ),
        ),
        "physics_step" => (
            Op::Server(ServerCall::Step),
            flow(&["then"], ins(&[("world", Str), ("dt", Float)]), vec![]),
        ),
        "physics_position" => (
            Op::Server(ServerCall::Position),
            pure(ins(&[("world", Str), ("body", Float)]), Vec3),
        ),
        "despawn" => (Op::Despawn, flow(&["then"], ins(&[("entity", Entity)]), vec![])),
        "play_sound" => (
            Op::PlaySound { at: false },
            flow(&["then"], ins(&[("sound", Str), ("volume", Float), ("pitch", Float)]), vec![]),
        ),
        "play_music" => (
            Op::PlayMusic,
            flow(
                &["then"],
                ins(&[("file", Str), ("looping", Bool), ("fade", Float), ("loop_start", Float)]),
                vec![],
            ),
        ),
        "stop_music" => (Op::StopMusic, flow(&["then"], ins(&[("fade", Float)]), vec![])),
        "set_music_volume" => {
            (Op::SetMusicVolume, flow(&["then"], ins(&[("volume", Float)]), vec![]))
        }
        "ui_show" => {
            (Op::UiShow, flow(&["then"], ins(&[("widget", Str), ("visible", Bool)]), vec![]))
        }
        "ui_text" => (Op::UiText, flow(&["then"], ins(&[("element", Str), ("text", Str)]), vec![])),
        "get_string" => {
            (Op::GetString, pure(ins(&[("table", Str), ("key", Str), ("values", Str)]), Str))
        }
        "typewriter" => {
            (Op::Typewriter, flow(&["then"], ins(&[("element", Str), ("skip", Bool)]), vec![]))
        }
        "camera_shake" => (
            Op::CameraShake,
            flow(&["then"], ins(&[("strength", Float), ("duration", Float)]), vec![]),
        ),
        "play_sound_at" => (
            Op::PlaySound { at: true },
            flow(
                &["then"],
                ins(&[("sound", Str), ("volume", Float), ("pitch", Float), ("location", Vec3)]),
                vec![],
            ),
        ),
        "is_valid" => (Op::Exists, pure(ins(&[("entity", Entity)]), Bool)),
        "is_a" => {
            let prefab = n.str_param("prefab")?;
            (Op::IsA { prefab: prefab.to_string() }, pure(ins(&[("entity", Entity)]), Bool))
        }
        "find_first" => {
            let prefab = n.str_param("prefab")?;
            (Op::FindFirst { prefab: prefab.to_string() }, pure(vec![], Entity))
        }
        "ray_hit" => (
            Op::RayHit { dist: false },
            pure(ins(&[("from", Vec3), ("direction", Vec3), ("max_distance", Float)]), Entity),
        ),
        "bodies_inside" => {
            (Op::BodiesInside, pure(ins(&[("entity", Entity)]), Ty::List(Elem::Entity)))
        }
        "ray_distance" => (
            Op::RayHit { dist: true },
            pure(ins(&[("from", Vec3), ("direction", Vec3), ("max_distance", Float)]), Float),
        ),
        "raycast_all" => (
            Op::Phys(query::Kind::RaycastAll),
            many(
                ins(&[("from", Vec3), ("direction", Vec3), ("max_distance", Float), ("mask", Int)]),
                vec![
                    ("entities", Ty::List(Elem::Entity)),
                    ("points", Ty::List(Elem::Vec3)),
                    ("normals", Ty::List(Elem::Vec3)),
                    ("distances", Ty::List(Elem::Float)),
                ],
            ),
        ),
        "shape_cast" => (
            Op::Phys(query::Kind::ShapeCast),
            many(
                ins(&[
                    ("shape", Str),
                    ("size", Vec2),
                    ("from", Vec3),
                    ("direction", Vec3),
                    ("max_distance", Float),
                    ("mask", Int),
                ]),
                vec![
                    ("entity", Entity),
                    ("distance", Float),
                    ("normal", Vec3),
                    ("point", Vec3),
                    ("hit", Bool),
                ],
            ),
        ),
        "point_query" => (
            Op::Phys(query::Kind::Point),
            pure(ins(&[("point", Vec3), ("mask", Int)]), Ty::List(Elem::Entity)),
        ),
        "overlap_box" => (
            Op::Phys(query::Kind::OverlapBox),
            pure(ins(&[("center", Vec3), ("size", Vec2), ("mask", Int)]), Ty::List(Elem::Entity)),
        ),
        "overlap_circle" => (
            Op::Phys(query::Kind::OverlapCircle),
            pure(
                ins(&[("center", Vec3), ("radius", Float), ("mask", Int)]),
                Ty::List(Elem::Entity),
            ),
        ),
        "overlap_shape" => (
            Op::Phys(query::Kind::OverlapShape),
            pure(
                ins(&[
                    ("shape", Str),
                    ("size", Vec2),
                    ("points", Ty::List(Elem::Vec2)),
                    ("center", Vec3),
                    ("angle", Float),
                    ("mask", Int),
                ]),
                Ty::List(Elem::Entity),
            ),
        ),
        "test_move" => (
            Op::Phys(query::Kind::TestMove),
            many(
                ins(&[("entity", Entity), ("delta", Vec3), ("mask", Int)]),
                vec![
                    ("hit", Bool),
                    ("blocker", Entity),
                    ("distance", Float),
                    ("normal", Vec3),
                    ("point", Vec3),
                ],
            ),
        ),
        "player" => (Op::Player, pure(ins(&[("index", Int)]), Entity)),
        "input_axis" => (Op::InputAxis, pure(ins(&[("name", Str)]), Float)),
        "action_pressed" => (Op::ActionPressed, pure(ins(&[("name", Str)]), Bool)),
        "action_held_time" => (Op::ActionHeld, pure(ins(&[("name", Str)]), Float)),
        "action_released" => (Op::ActionReleased, pure(ins(&[("name", Str)]), Bool)),
        "mouse_delta" => (Op::MouseDelta, pure(ins(&[]), Vec2)),
        "raw_mouse_delta" => (Op::Device(device_nodes::DeviceKind::RawMouse), pure(ins(&[]), Vec2)),
        "mouse_look" => (Op::Device(device_nodes::DeviceKind::MouseLook), pure(ins(&[]), Vec2)),
        "pen_pressure" => {
            (Op::Device(device_nodes::DeviceKind::PenPressure), pure(ins(&[]), Float))
        }
        "pen_tilt" => (Op::Device(device_nodes::DeviceKind::PenTilt), pure(ins(&[]), Vec2)),
        "pen_eraser" => (Op::Device(device_nodes::DeviceKind::PenEraser), pure(ins(&[]), Bool)),
        "pad_gyro" => (Op::Device(device_nodes::DeviceKind::PadGyro), pure(ins(&[]), Vec3)),
        "pad_accel" => (Op::Device(device_nodes::DeviceKind::PadAccel), pure(ins(&[]), Vec3)),
        "pad_motion_note" => (Op::Device(device_nodes::DeviceKind::PadNote), pure(ins(&[]), Str)),
        "set_pad_light" => (
            Op::Device(device_nodes::DeviceKind::SetLight),
            flow(&["then"], ins(&[("red", Float), ("green", Float), ("blue", Float)]), vec![]),
        ),
        "mouse_wheel" => (Op::MouseWheel, pure(ins(&[]), Float)),
        "pointer_world" => (Op::PointerWorld, pure(ins(&[]), Vec2)),
        "spectate" => (
            Op::Game(rt::GameKind::Spectate),
            flow(&["then"], ins(&[("entity", Entity), ("on", Bool)]), vec![]),
        ),
        "unpossess" => {
            (Op::Game(rt::GameKind::Unpossess), flow(&["then"], ins(&[("index", Int)]), vec![]))
        }
        "rumble" => (
            Op::Rumble,
            flow(
                &["then"],
                ins(&[("strong", Float), ("weak", Float), ("duration", Float)]),
                vec![],
            ),
        ),
        "rebind_action" => {
            (Op::RebindAction, flow(&["then"], ins(&[("action", Str), ("slot", Int)]), vec![]))
        }
        "find_all" => {
            let prefab = n.str_param("prefab")?;
            (Op::FindAll { prefab: prefab.to_string() }, pure(vec![], Ty::List(Elem::Entity)))
        }
        "offset_polygon" => (
            Op::Poly(PolyOp::Offset),
            pure(
                ins(&[("points", Ty::List(Elem::Vec2)), ("distance", Float)]),
                Ty::List(Elem::Vec2),
            ),
        ),
        "merge_polygon" => (
            Op::Poly(PolyOp::Merge),
            pure(
                ins(&[("a", Ty::List(Elem::Vec2)), ("b", Ty::List(Elem::Vec2))]),
                Ty::List(Elem::Vec2),
            ),
        ),
        "clip_polygon" => (
            Op::Poly(PolyOp::Clip),
            pure(
                ins(&[("a", Ty::List(Elem::Vec2)), ("b", Ty::List(Elem::Vec2))]),
                Ty::List(Elem::Vec2),
            ),
        ),
        "intersect_polygon" => (
            Op::Poly(PolyOp::Intersect),
            pure(
                ins(&[("a", Ty::List(Elem::Vec2)), ("b", Ty::List(Elem::Vec2))]),
                Ty::List(Elem::Vec2),
            ),
        ),
        k if k.starts_with("list_") => {
            let t = n.ty_param()?;
            let e = rt::Elem::of(t).ok_or_else(|| {
                BuildError::node(n.id, format!("{k} holds plain values, not {t}"))
            })?;
            let l = Ty::List(e);
            let (op, i, out) = match k {
                "list_make" => {
                    let count = n
                        .param("count")
                        .and_then(num)
                        .and_then(|c| c.parse::<u32>().ok())
                        .filter(|c| *c <= 64)
                        .ok_or_else(|| {
                            BuildError::node(n.id, "list_make needs `count` from 0 to 64")
                        })?;
                    let i = (0..count).map(|k| (format!("item{k}"), t)).collect();
                    (ListOp::Make(count), i, l)
                }
                "list_len" => (ListOp::Len, ins(&[("list", l)]), Int),
                "list_get" => (ListOp::Get, ins(&[("list", l), ("index", Int)]), t),
                "list_push" => (ListOp::Push, ins(&[("list", l), ("item", t)]), l),
                "list_set" => (ListOp::Set, ins(&[("list", l), ("index", Int), ("item", t)]), l),
                "list_insert" => {
                    (ListOp::Insert, ins(&[("list", l), ("item", t), ("index", Int)]), l)
                }
                "list_remove_at" => (ListOp::RemoveAt, ins(&[("list", l), ("index", Int)]), l),
                "list_remove_item" => (ListOp::RemoveItem, ins(&[("list", l), ("item", t)]), l),
                "list_find" => (ListOp::Find, ins(&[("list", l), ("item", t)]), Int),
                "list_contains" => (ListOp::Contains, ins(&[("list", l), ("item", t)]), Bool),
                other => return Err(BuildError::node(n.id, format!("unknown node kind {other}"))),
            };
            (Op::List(op, e), pure(i, out))
        }
        "import_set_pivot" => (
            Op::Sequence(1),
            flow(&["then"], ins(&[("x", Float), ("y", Float)]), vec![]),
        ),
        "import_rename" => (Op::Sequence(1), flow(&["then"], ins(&[("name", Str)]), vec![])),
        "import_add_tag" => (Op::Sequence(1), flow(&["then"], ins(&[("tag", Str)]), vec![])),
        "import_set_text" => (Op::Sequence(1), flow(&["then"], ins(&[("text", Str)]), vec![])),
        "import_path" | "import_kind" | "import_name" => {
            (Op::Const(Value::Str(String::new())), pure(vec![], Str))
        }
        other => return Err(BuildError::node(n.id, format!("unknown node kind {other}"))),
    })
}

/// Evaluate a pure operation on already-evaluated inputs. Shared by the
/// interpreter; compiled code calls the same `rt::ops` functions directly.
pub(crate) fn apply_pure(op: &Op, a: &[Value]) -> Value {
    use rt::ops::*;
    match op {
        Op::Bin(b, t) => match (b, t) {
            (BinOp::Add, Ty::Int) => Value::Int(int_add(a[0].int(), a[1].int())),
            (BinOp::Sub, Ty::Int) => Value::Int(int_sub(a[0].int(), a[1].int())),
            (BinOp::Mul, Ty::Int) => Value::Int(int_mul(a[0].int(), a[1].int())),
            (BinOp::Div, Ty::Int) => Value::Int(int_div(a[0].int(), a[1].int())),
            (BinOp::Mod, Ty::Int) => Value::Int(int_mod(a[0].int(), a[1].int())),
            (BinOp::Mod, _) => Value::Float(float_mod(a[0].float(), a[1].float())),
            (BinOp::Min, Ty::Int) => Value::Int(int_min(a[0].int(), a[1].int())),
            (BinOp::Max, Ty::Int) => Value::Int(int_max(a[0].int(), a[1].int())),
            (BinOp::Add, Ty::Float) => Value::Float(float_add(a[0].float(), a[1].float())),
            (BinOp::Sub, Ty::Float) => Value::Float(float_sub(a[0].float(), a[1].float())),
            (BinOp::Mul, Ty::Float) => Value::Float(float_mul(a[0].float(), a[1].float())),
            (BinOp::Div, Ty::Float) => Value::Float(float_div(a[0].float(), a[1].float())),
            (BinOp::Min, Ty::Float) => Value::Float(float_min(a[0].float(), a[1].float())),
            (BinOp::Max, Ty::Float) => Value::Float(float_max(a[0].float(), a[1].float())),
            (BinOp::Add, Ty::Vec2) => Value::Vec2(vec2_add(a[0].vec2(), a[1].vec2())),
            (BinOp::Sub, Ty::Vec2) => Value::Vec2(vec2_sub(a[0].vec2(), a[1].vec2())),
            (BinOp::And, _) => Value::Bool(and(a[0].bool(), a[1].bool())),
            (BinOp::Or, _) => Value::Bool(or(a[0].bool(), a[1].bool())),
            (BinOp::Concat, _) => Value::Str(concat(a[0].str(), a[1].str())),
            (BinOp::StrContains, _) => Value::Bool(str_contains(a[0].str(), a[1].str())),
            (BinOp::StrStarts, _) => Value::Bool(str_starts(a[0].str(), a[1].str())),
            (BinOp::StrEnds, _) => Value::Bool(str_ends(a[0].str(), a[1].str())),
            (BinOp::Add, Ty::Vec3) => Value::Vec3(vec3_add(a[0].vec3(), a[1].vec3())),
            (BinOp::Sub, Ty::Vec3) => Value::Vec3(vec3_sub(a[0].vec3(), a[1].vec3())),
            (BinOp::Dot, Ty::Vec3) => Value::Float(vec3_dot(a[0].vec3(), a[1].vec3())),
            (BinOp::Dot, _) => Value::Float(vec2_dot(a[0].vec2(), a[1].vec2())),
            (BinOp::Cross, _) => Value::Vec3(vec3_cross(a[0].vec3(), a[1].vec3())),
            (BinOp::Pow, _) => Value::Float(float_pow(a[0].float(), a[1].float())),
            (BinOp::Atan2, _) => Value::Float(float_atan2(a[0].float(), a[1].float())),
            _ => unreachable!("checked by Program::build"),
        },
        Op::Un(u, t) => match (u, t) {
            (UnOp::Neg, Ty::Int) => Value::Int(int_neg(a[0].int())),
            (UnOp::Neg, Ty::Float) => Value::Float(float_neg(a[0].float())),
            (UnOp::Neg, Ty::Vec2) => Value::Vec2(vec2_neg(a[0].vec2())),
            (UnOp::Neg, Ty::Vec3) => Value::Vec3(vec3_neg(a[0].vec3())),
            (UnOp::Abs, Ty::Int) => Value::Int(int_abs(a[0].int())),
            (UnOp::Abs, Ty::Float) => Value::Float(float_abs(a[0].float())),
            (UnOp::Sqrt, _) => Value::Float(float_sqrt(a[0].float())),
            (UnOp::StrLen, _) => Value::Int(str_len(a[0].str())),
            (UnOp::StrUpper, _) => Value::Str(str_upper(a[0].str())),
            (UnOp::StrLower, _) => Value::Str(str_lower(a[0].str())),
            (UnOp::StrTrim, _) => Value::Str(str_trim(a[0].str())),
            (UnOp::StrTrimStart, _) => Value::Str(str_trim_start(a[0].str())),
            (UnOp::StrTrimEnd, _) => Value::Str(str_trim_end(a[0].str())),
            (UnOp::ParseInt, _) => Value::Int(parse_int(a[0].str())),
            (UnOp::ParseFloat, _) => Value::Float(parse_float(a[0].str())),
            (UnOp::NumText, Ty::Int) => Value::Str(int_grouped(a[0].int())),
            (UnOp::NumText, _) => Value::Str(float_grouped(a[0].float())),
            (UnOp::FloatText, _) => Value::Str(float_text(a[0].float())),
            (UnOp::VecText, Ty::Vec2) => Value::Str(vec2_text(a[0].vec2())),
            (UnOp::VecText, _) => Value::Str(vec3_text(a[0].vec3())),
            (UnOp::Not, _) => Value::Bool(not(a[0].bool())),
            (UnOp::ToFloat, _) => Value::Float(to_float(a[0].int())),
            (UnOp::ToInt, _) => Value::Int(to_int(a[0].float())),
            (UnOp::ToStr, t) => Value::Str(match t {
                Ty::Bool => bool_str(a[0].bool()),
                Ty::Int => int_str(a[0].int()),
                Ty::Float => float_str(a[0].float()),
                Ty::Vec2 => vec2_str(a[0].vec2()),
                Ty::Str => str_str(a[0].str()),
                Ty::Entity => entity_str(a[0].entity()),
                Ty::Vec3 => vec3_str(a[0].vec3()),
                Ty::Quat | Ty::Plane | Ty::List(_) => unreachable!("checked by Program::build"),
            }),
            (UnOp::Vec2X, _) => Value::Float(vec2_x(a[0].vec2())),
            (UnOp::Vec2Y, _) => Value::Float(vec2_y(a[0].vec2())),
            (UnOp::Vec3X, _) => Value::Float(vec3_x(a[0].vec3())),
            (UnOp::Vec3Y, _) => Value::Float(vec3_y(a[0].vec3())),
            (UnOp::Vec3Z, _) => Value::Float(vec3_z(a[0].vec3())),
            (UnOp::Length, Ty::Vec3) => Value::Float(vec3_length(a[0].vec3())),
            (UnOp::Normalize, Ty::Vec3) => Value::Vec3(vec3_normalize(a[0].vec3())),
            (UnOp::Length, _) => Value::Float(vec2_length(a[0].vec2())),
            (UnOp::Normalize, _) => Value::Vec2(vec2_normalize(a[0].vec2())),
            (UnOp::Sin, _) => Value::Float(float_sin(a[0].float())),
            (UnOp::Exp, _) => Value::Float(float_exp(a[0].float())),
            (UnOp::Ln, _) => Value::Float(float_ln(a[0].float())),
            (UnOp::Atan, _) => Value::Float(float_atan(a[0].float())),
            (UnOp::Asin, _) => Value::Float(float_asin(a[0].float())),
            (UnOp::Acos, _) => Value::Float(float_acos(a[0].float())),
            (UnOp::Cos, _) => Value::Float(float_cos(a[0].float())),
            (UnOp::Floor, _) => Value::Int(floor_int(a[0].float())),
            (UnOp::Round, _) => Value::Int(round_int(a[0].float())),
            (UnOp::Ceil, _) => Value::Int(ceil_int(a[0].float())),
            _ => unreachable!("checked by Program::build"),
        },
        Op::Compare(c, t) => {
            macro_rules! cmp {
                ($x:expr, $y:expr) => {
                    match c {
                        CmpOp::Lt => lt($x, $y),
                        CmpOp::Le => le($x, $y),
                        CmpOp::Gt => gt($x, $y),
                        CmpOp::Ge => ge($x, $y),
                        CmpOp::Eq => eq($x, $y),
                        CmpOp::Ne => ne($x, $y),
                    }
                };
            }
            Value::Bool(match t {
                Ty::Bool => cmp!(a[0].bool(), a[1].bool()),
                Ty::Int => cmp!(a[0].int(), a[1].int()),
                Ty::Float => cmp!(a[0].float(), a[1].float()),
                Ty::Vec2 => cmp!(a[0].vec2(), a[1].vec2()),
                Ty::Str => cmp!(a[0].str(), a[1].str()),
                Ty::Entity => cmp!(a[0].entity(), a[1].entity()),
                Ty::Vec3 => cmp!(a[0].vec3(), a[1].vec3()),
                Ty::Quat => cmp!(a[0].quat(), a[1].quat()),
                Ty::Plane => cmp!(a[0].plane(), a[1].plane()),
                Ty::List(_) => unreachable!("checked by Program::build"),
            })
        }
        Op::Select(_) => select(a[0].bool(), a[1].clone(), a[2].clone()),
        Op::MakeVec2 => Value::Vec2(vec2_make(a[0].float(), a[1].float())),
        Op::MakeVec3 => Value::Vec3(vec3_make(a[0].float(), a[1].float(), a[2].float())),
        Op::Scale(Ty::Vec3) => Value::Vec3(vec3_scale(a[0].vec3(), a[1].float())),
        Op::Scale(_) => Value::Vec2(vec2_scale(a[0].vec2(), a[1].float())),
        Op::List(l, e) => {
            let list = |v: Vec<Value>| Value::List(*e, v);
            match l {
                ListOp::Make(_) => list(a.to_vec()),
                ListOp::Len => Value::Int(list_len(a[0].list())),
                ListOp::Get => list_get(a[0].list(), a[1].int(), e.ty().zero()),
                ListOp::Push => list(list_push(a[0].list(), a[1].clone())),
                ListOp::Set => list(list_set(a[0].list(), a[1].int(), a[2].clone())),
                ListOp::Insert => list(list_insert(a[0].list(), a[1].clone(), a[2].int())),
                ListOp::RemoveAt => list(list_remove_at(a[0].list(), a[1].int())),
                ListOp::RemoveItem => list(list_remove_item(a[0].list(), a[1].clone())),
                ListOp::Find => Value::Int(list_find(a[0].list(), a[1].clone())),
                ListOp::Contains => Value::Bool(list_contains(a[0].list(), a[1].clone())),
            }
        }
        Op::Poly(kind) => {
            let list = |v: Vec<Value>| Value::List(poly2d::list_elem(), v);
            match kind {
                PolyOp::Offset => list(poly2d::offset_values(a[0].list(), a[1].float())),
                PolyOp::Merge => list(poly2d::merge_values(a[0].list(), a[1].list())),
                PolyOp::Clip => list(poly2d::clip_values(a[0].list(), a[1].list())),
                PolyOp::Intersect => list(poly2d::intersect_values(a[0].list(), a[1].list())),
            }
        }
        _ => unreachable!("not a plain pure op"),
    }
}
