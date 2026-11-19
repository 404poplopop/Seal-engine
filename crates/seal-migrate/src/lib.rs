//! Migration of classic visual scripts into Seal graphs.
//!
//! Select nodes in the classic editor, copy them, and the clipboard holds a
//! text export. [`import`] reads that text and builds a Seal [`Graph`] node
//! for node, plus a [`Report`] that lists every node that was mapped, skipped
//! (reroutes, comments) or not supported yet, and every link it had to drop.
//! Nothing is guessed: a node Seal cannot express is left out and reported,
//! never replaced with something that behaves differently.

pub mod export;

use export::{ref_path, Object, ParseError, Pin};
use seal_graph::{Elem, Graph, Node, Ty, Value};
use seal_sealtext::{SealMap, SealValue};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

/// What happened to one exported node.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Became a Seal node of this kind.
    Mapped(String),
    /// Has no behaviour and was folded away (reroute knots, comments).
    Skipped(String),
    /// Seal has no equivalent yet; the reason says what is missing.
    Unsupported(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeReport {
    /// The node's name in the export, e.g. `K2Node_IfThenElse_0`.
    pub name: String,
    pub class: String,
    /// The function, event, variable or macro the node refers to, if any.
    pub member: Option<String>,
    /// Seal node id when mapped.
    pub id: Option<u32>,
    pub status: Status,
    pub line: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub nodes: Vec<NodeReport>,
    /// Links that could not be kept, and other things worth a look.
    pub notes: Vec<String>,
    pub dropped_links: usize,
}

impl Report {
    pub fn count(&self, f: impl Fn(&Status) -> bool) -> usize {
        self.nodes.iter().filter(|n| f(&n.status)).count()
    }
    pub fn mapped(&self) -> usize {
        self.count(|s| matches!(s, Status::Mapped(_)))
    }
    pub fn unsupported(&self) -> usize {
        self.count(|s| matches!(s, Status::Unsupported(_)))
    }
    /// Share of behaviour-carrying nodes that were mapped (1.0 when there are none).
    pub fn coverage(&self) -> f64 {
        let total = self.mapped() + self.unsupported();
        if total == 0 {
            1.0
        } else {
            self.mapped() as f64 / total as f64
        }
    }
    /// True when every node was mapped or safely skipped and no link was lost.
    pub fn complete(&self) -> bool {
        self.unsupported() == 0 && self.dropped_links == 0
    }
    /// Unsupported nodes grouped by what they refer to, most common first.
    pub fn missing(&self) -> Vec<(String, usize)> {
        let mut by: BTreeMap<String, usize> = BTreeMap::new();
        for n in &self.nodes {
            if let Status::Unsupported(_) = n.status {
                let key = match &n.member {
                    Some(m) => format!("{} {}", n.class, m),
                    None => n.class.clone(),
                };
                *by.entry(key).or_default() += 1;
            }
        }
        let mut v: Vec<_> = by.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    }

    pub fn to_text(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(
            s,
            "Coverage: {:.1}% ({} mapped, {} unsupported, {} skipped, {} links dropped)",
            self.coverage() * 100.0,
            self.mapped(),
            self.unsupported(),
            self.count(|s| matches!(s, Status::Skipped(_))),
            self.dropped_links
        );
        for n in &self.nodes {
            let what = n.member.as_deref().map(|m| format!(" {m}")).unwrap_or_default();
            let line = match &n.status {
                Status::Mapped(kind) => {
                    format!("  ok    {}{what} -> {kind} #{}", n.class, n.id.unwrap_or(0))
                }
                Status::Skipped(why) => format!("  skip  {}{what}: {why}", n.class),
                Status::Unsupported(why) => {
                    format!("  MISS  {}{what} (line {}): {why}", n.class, n.line)
                }
            };
            let _ = writeln!(s, "{line}");
        }
        if !self.missing().is_empty() {
            let _ = writeln!(s, "Missing, most common first:");
            for (k, c) in self.missing() {
                let _ = writeln!(s, "  {c} x {k}");
            }
        }
        for note in &self.notes {
            let _ = writeln!(s, "Note: {note}");
        }
        s
    }
}

#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Name of the Seal graph.
    pub name: String,
    /// Variable defaults by their original name. A copied selection does not
    /// carry the class defaults, so unlisted variables start at zero.
    pub var_defaults: Vec<(String, Value)>,
}

#[derive(Clone, Debug)]
pub struct Import {
    pub graph: Graph,
    pub report: Report,
}

/// Turn any name into a Seal identifier: letters, digits and `_`.
pub fn ident(name: &str) -> String {
    let mut s: String =
        name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    if s.is_empty() || s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert(0, '_');
    }
    s
}

/// The Seal type of a pin, if it has one.
pub fn pin_ty(p: &Pin) -> Option<Ty> {
    match p.container.as_str() {
        "" | "None" => single_ty(p),
        // An array of plain values is a Seal list; arrays of arrays, sets and maps are not.
        "Array" => Some(Ty::List(Elem::of(single_ty(p)?)?)),
        _ => None,
    }
}

fn single_ty(p: &Pin) -> Option<Ty> {
    Some(match p.category.as_str() {
        // An enum value is its enumerator's name; the enum's numbering is
        // not in the copied text.
        "byte" if !matches!(p.sub_object.trim(), "" | "None") => Ty::Str,
        "int" | "int64" | "byte" => Ty::Int,
        "real" | "float" | "double" => Ty::Float,
        "bool" => Ty::Bool,
        "string" | "name" | "text" => Ty::Str,
        // Actor and object references are entities in Seal.
        "object" | "softobject" => Ty::Entity,
        "struct" if p.sub_object.trim_end_matches(['\'', '"']).ends_with(".Vector2D") => Ty::Vec2,
        "struct" if p.sub_object.trim_end_matches(['\'', '"']).ends_with(".Vector") => Ty::Vec3,
        // A rotation is pitch, yaw and roll in degrees, as a 3D vector.
        "struct" if p.sub_object.trim_end_matches(['\'', '"']).ends_with(".Rotator") => Ty::Vec3,
        // A timer handle is the name of the event its timer runs ("" for none).
        "struct" if p.sub_object.trim_end_matches(['\'', '"']).ends_with(".TimerHandle") => Ty::Str,
        _ => return None,
    })
}

/// `(X=1.000000,Y=2.000000)`, the export text of a 2D vector.
fn vec2_literal(t: &str) -> Option<(f64, f64)> {
    let inner = t.trim().strip_prefix('(')?.strip_suffix(')')?;
    let (mut x, mut y) = (0.0, 0.0);
    for part in inner.split(',').filter(|p| !p.trim().is_empty()) {
        let (k, v) = part.split_once('=')?;
        let v: f64 = v.trim().parse().ok()?;
        match k.trim() {
            "X" => x = v,
            "Y" => y = v,
            _ => return None,
        }
    }
    Some((x, y))
}

/// A 3D vector default: `1.000000,2.000000,3.000000` on pins, or the
/// `(X=..,Y=..,Z=..)` form used elsewhere. Rotations use the same comma
/// form in pitch, yaw, roll order, or `(Pitch=..,Yaw=..,Roll=..)`.
fn vec3_literal(t: &str) -> Option<(f64, f64, f64)> {
    let t = t.trim();
    if t.is_empty() {
        return Some((0.0, 0.0, 0.0));
    }
    if let Some(inner) = t.strip_prefix('(') {
        let inner = inner.strip_suffix(')')?;
        let (mut x, mut y, mut z) = (0.0, 0.0, 0.0);
        for part in inner.split(',').filter(|p| !p.trim().is_empty()) {
            let (k, v) = part.split_once('=')?;
            let v: f64 = v.trim().parse().ok()?;
            match k.trim() {
                "X" | "Pitch" => x = v,
                "Y" | "Yaw" => y = v,
                "Z" | "Roll" => z = v,
                _ => return None,
            }
        }
        return Some((x, y, z));
    }
    let v: Vec<f64> = t.split(',').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
    match v.as_slice() {
        [x, y, z] => Some((*x, *y, *z)),
        _ => None,
    }
}

fn float_text(f: f64) -> String {
    let s = format!("{f:?}");
    if s.contains(['.', 'e', 'i', 'N']) {
        s
    } else {
        format!("{s}.0")
    }
}

/// A pin's literal default as SealText.
fn literal(ty: Ty, text: &str) -> Option<SealValue> {
    let t = text.trim();
    Some(match ty {
        Ty::Int => SealValue::Number(t.parse::<i64>().ok()?.to_string()),
        Ty::Float => SealValue::Number(float_text(t.parse::<f64>().ok()?)),
        Ty::Bool => SealValue::Bool(t.eq_ignore_ascii_case("true")),
        Ty::Str => SealValue::String(text.to_string()),
        Ty::Vec2 => {
            let (x, y) = vec2_literal(t)?;
            SealValue::Array(vec![
                SealValue::Number(float_text(x)),
                SealValue::Number(float_text(y)),
            ])
        }
        Ty::Vec3 => {
            let (x, y, z) = vec3_literal(t)?;
            SealValue::Array(vec![
                SealValue::Number(float_text(x)),
                SealValue::Number(float_text(y)),
                SealValue::Number(float_text(z)),
            ])
        }
        // Array pins carry no literal items; an empty one is an empty list.
        Ty::List(_) if t.is_empty() || t == "()" => SealValue::Array(Vec::new()),
        // An object literal can only be "nothing".
        Ty::Entity if t.is_empty() || t.eq_ignore_ascii_case("none") => {
            SealValue::String("none".into())
        }
        _ => return None,
    })
}

/// Pins that only exist for the classic runtime and carry no logic.
const PLUMBING: &[&str] = &["self", "WorldContextObject", "LatentInfo", "OutputDelegate"];

/// A library function Seal maps directly.
struct LibFn {
    kind: &'static str,
    ty: Option<Ty>,
    op: Option<&'static str>,
    pins: Vec<(&'static str, &'static str)>,
}

fn suffix_ty(s: &str) -> Option<Ty> {
    Some(match s {
        "IntInt" | "Int64Int64" | "ByteByte" => Ty::Int,
        "DoubleDouble" | "FloatFloat" => Ty::Float,
        "BoolBool" => Ty::Bool,
        "StrStr" | "NameName" | "TextText" => Ty::Str,
        _ => return None,
    })
}

fn library(member: &str) -> Option<LibFn> {
    const AB: [(&str, &str); 3] = [("A", "a"), ("B", "b"), ("ReturnValue", "out")];
    const A: [(&str, &str); 2] = [("A", "a"), ("ReturnValue", "out")];
    let f = |kind, ty, op, pins: &[(&'static str, &'static str)]| {
        Some(LibFn { kind, ty, op, pins: pins.to_vec() })
    };
    match member {
        "PrintString" | "PrintText" => {
            return f(
                "log",
                None,
                None,
                &[
                    ("execute", "in"),
                    ("then", "then"),
                    ("InString", "message"),
                    ("InText", "message"),
                ],
            )
        }
        "Delay" => {
            return f(
                "delay",
                None,
                None,
                &[("execute", "in"), ("then", "then"), ("Duration", "seconds")],
            )
        }
        "BooleanAND" => return f("and", None, None, &AB),
        "BooleanOR" => return f("or", None, None, &AB),
        "Not_PreBool" => return f("not", None, None, &A),
        "Percent_IntInt" | "Percent_Int64Int64" | "Percent_ByteByte" => {
            return f("mod", None, None, &AB)
        }
        "Percent_FloatFloat" | "Percent_DoubleDouble" => {
            return f("mod", Some(Ty::Float), None, &AB)
        }
        "Max" => return f("max", Some(Ty::Int), None, &AB),
        "Min" => return f("min", Some(Ty::Int), None, &AB),
        "FMax" => return f("max", Some(Ty::Float), None, &AB),
        "FMin" => return f("min", Some(Ty::Float), None, &AB),
        "Abs_Int" => return f("abs", Some(Ty::Int), None, &A),
        "Abs" => return f("abs", Some(Ty::Float), None, &A),
        "Sqrt" => return f("sqrt", None, None, &A),
        "FTrunc" => return f("to_int", None, None, &A),
        "Conv_IntToDouble" | "Conv_IntToFloat" => {
            return f("to_float", None, None, &[("InInt", "a"), ("ReturnValue", "out")])
        }
        "Conv_IntToString" => {
            return f("to_str", Some(Ty::Int), None, &[("InInt", "a"), ("ReturnValue", "out")])
        }
        "Conv_DoubleToString" | "Conv_FloatToString" => {
            return f(
                "float_text",
                None,
                None,
                &[("InDouble", "a"), ("InFloat", "a"), ("ReturnValue", "out")],
            )
        }
        "Conv_VectorToString" => {
            return f("vec_text", Some(Ty::Vec3), None, &[("InVec", "a"), ("ReturnValue", "out")])
        }
        "Conv_Vector2dToString" => {
            return f("vec_text", Some(Ty::Vec2), None, &[("InVec", "a"), ("ReturnValue", "out")])
        }
        "Conv_BoolToString" => {
            return f("to_str", Some(Ty::Bool), None, &[("InBool", "a"), ("ReturnValue", "out")])
        }
        "Concat_StrStr" => return f("concat", None, None, &AB),
        "Len" => return f("str_len", None, None, &[("S", "a"), ("ReturnValue", "out")]),
        "ToUpper" => {
            return f("str_upper", None, None, &[("SourceString", "a"), ("ReturnValue", "out")])
        }
        "ToLower" => {
            return f("str_lower", None, None, &[("SourceString", "a"), ("ReturnValue", "out")])
        }
        "TextToUpper" => {
            return f("str_upper", None, None, &[("InText", "a"), ("ReturnValue", "out")])
        }
        "TextToLower" => {
            return f("str_lower", None, None, &[("InText", "a"), ("ReturnValue", "out")])
        }
        "TextTrimPreceding" => {
            return f("str_trim_start", None, None, &[("InText", "a"), ("ReturnValue", "out")])
        }
        "TextTrimTrailing" => {
            return f("str_trim_end", None, None, &[("InText", "a"), ("ReturnValue", "out")])
        }
        "TextTrimPrecedingAndTrailing" => {
            return f("str_trim", None, None, &[("InText", "a"), ("ReturnValue", "out")])
        }
        "Trim" => {
            return f(
                "str_trim_start",
                None,
                None,
                &[("SourceString", "a"), ("ReturnValue", "out")],
            )
        }
        "TrimTrailing" => {
            return f("str_trim_end", None, None, &[("SourceString", "a"), ("ReturnValue", "out")])
        }
        "SelectInt" | "SelectFloat" | "SelectString" | "SelectBool" => {
            let ty = match member {
                "SelectInt" => Ty::Int,
                "SelectFloat" => Ty::Float,
                "SelectString" => Ty::Str,
                _ => Ty::Bool,
            };
            return f(
                "select",
                Some(ty),
                None,
                &[("A", "a"), ("B", "b"), ("bPickA", "cond"), ("ReturnValue", "out")],
            );
        }
        _ => {}
    }
    let (prefix, suffix) = member.split_once('_')?;
    let ty = suffix_ty(suffix)?;
    let (kind, op) = match prefix {
        "Add" => ("add", None),
        "Subtract" => ("sub", None),
        "Multiply" => ("mul", None),
        "Divide" => ("div", None),
        "Percent" if ty == Ty::Int => ("mod", None),
        "Less" => ("compare", Some("lt")),
        "LessEqual" => ("compare", Some("le")),
        "Greater" => ("compare", Some("gt")),
        "GreaterEqual" => ("compare", Some("ge")),
        "EqualEqual" => ("compare", Some("eq")),
        "NotEqual" => ("compare", Some("ne")),
        _ => return None,
    };
    let arith = op.is_none();
    if arith && !matches!(ty, Ty::Int | Ty::Float)
        || !arith && op != Some("eq") && op != Some("ne") && !matches!(ty, Ty::Int | Ty::Float)
    {
        return None;
    }
    f(kind, Some(ty), op, &AB)
}

/// Contains, StartsWith and EndsWith. The classic nodes ignore case unless
/// told otherwise, so the default expands to lower-casing both texts.
fn string_search(o: &Object, member: &str) -> Result<Mapped, Status> {
    let (kind, a, b) = match member {
        "Contains" => ("str_contains", "SearchIn", "Substring"),
        "StartsWith" => ("str_starts", "SourceString", "InPrefix"),
        _ => ("str_ends", "SourceString", "InSuffix"),
    };
    let (flag, exact) =
        if member == "Contains" { ("bUseCase", "true") } else { ("SearchCase", "casesensitive") };
    let sensitive = match o.pin(flag) {
        None => false,
        Some(p) if !p.linked.is_empty() => {
            return Err(Status::Unsupported(format!("{flag} driven by a link")))
        }
        Some(p) => p.default.as_deref().is_some_and(|t| t.trim().eq_ignore_ascii_case(exact)),
    };
    if member == "Contains" && bool_input(o, "bSearchFromEnd")? {
        // Only the match position changes, never whether there is one; still, say so.
        return Err(Status::Unsupported("bSearchFromEnd is not mapped".into()));
    }
    let mut m = Mapped::new(kind);
    if sensitive {
        return Ok(m.pin(a, "a").pin(b, "b").pin("ReturnValue", "out"));
    }
    let la = m.add(Node::new(0, "str_lower"));
    let lb = m.add(Node::new(0, "str_lower"));
    m.wire(la, "out", 0, "a");
    m.wire(lb, "out", 0, "b");
    Ok(m.at(a, la, "a").at(b, lb, "a").pin("ReturnValue", "out"))
}

/// What one exported node became: one Seal node, or a few wired together
/// (macros such as Do Once expand into a branch, a flag and its setters).
struct Mapped {
    nodes: Vec<Node>,
    /// Export pin name -> (index into `nodes`, Seal pin).
    pins: HashMap<String, (usize, String)>,
    /// Links inside the expansion, by index into `nodes`.
    links: Vec<(usize, String, usize, String)>,
    /// Hidden state variables the expansion needs.
    vars: Vec<(String, Value)>,
}

impl Mapped {
    fn new(kind: &str) -> Self {
        Self {
            nodes: vec![Node::new(0, kind)],
            pins: HashMap::new(),
            links: Vec::new(),
            vars: Vec::new(),
        }
    }
    fn s(mut self, key: &str, v: &str) -> Self {
        self.nodes[0].set(key, SealValue::String(v.into()));
        self
    }
    fn pin(self, from: &str, to: &str) -> Self {
        self.at(from, 0, to)
    }
    fn at(mut self, from: &str, node: usize, to: &str) -> Self {
        self.pins.insert(from.into(), (node, to.into()));
        self
    }
    fn add(&mut self, node: Node) -> usize {
        self.nodes.push(node);
        self.nodes.len() - 1
    }
    fn wire(&mut self, a: usize, ap: &str, b: usize, bp: &str) {
        self.links.push((a, ap.into(), b, bp.into()));
    }
}

fn named(kind: &str, name: &str) -> Node {
    Node::new(0, kind).with("name", SealValue::String(name.into()))
}

/// Put a literal on an unlinked input, keeping literals already there.
fn put_default(node: &mut Node, pin: &str, v: SealValue) {
    if let Some((_, m @ SealValue::Map(_))) = node.params.iter_mut().find(|(k, _)| k == "defaults")
    {
        m.insert(pin, v);
    } else {
        node.set("defaults", SealValue::Map(SealMap::new(vec![(pin.into(), v)])));
    }
}

/// A `set` of a bool flag to a fixed value.
fn set_flag(var: &str, on: bool) -> Node {
    let mut n = named("set", var);
    put_default(&mut n, "value", SealValue::Bool(on));
    n
}

/// Exec input names macros use, depending on the engine version.
const EXEC_IN: &[&str] = &["execute", "Exec", "Execute", "In", "Enter"];

/// The literal of a bool macro input such as `Start Closed`, or why it can't be read.
fn bool_input(o: &Object, name: &str) -> Result<bool, Status> {
    match o.pin(name) {
        None => Ok(false),
        Some(p) if !p.linked.is_empty() => {
            Err(Status::Unsupported(format!("{name} driven by a link")))
        }
        Some(p) => Ok(p.default.as_deref().is_some_and(|t| t.trim().eq_ignore_ascii_case("true"))),
    }
}

fn typed(kind: &str, ty: Ty) -> Node {
    Node::new(0, kind).with("type", SealValue::String(ty.name().into()))
}

/// A node that passes its input through unchanged (`select` with the
/// condition fixed to true), so one exported input can feed several nodes.
fn pass(ty: Ty) -> Node {
    let mut n = typed("select", ty);
    put_default(&mut n, "cond", SealValue::Bool(true));
    if ty == Ty::Entity {
        // Never taken, but an entity input must name one.
        put_default(&mut n, "b", SealValue::String("none".into()));
    }
    n
}

fn one(kind: &str, ty: Option<Ty>, pins: &[(&str, &str)]) -> Mapped {
    let mut m = Mapped::new(kind);
    if let Some(t) = ty {
        m = m.s("type", t.name());
    }
    for (a, b) in pins {
        m = m.pin(a, b);
    }
    m
}

/// Math library functions that need more than one Seal node, or a node
/// the one-to-one table cannot describe (vectors, trig, rounding).
/// One operand of an [`Expr`] node.
#[derive(Clone, Copy)]
enum Arg<'a> {
    /// The classic node's input pin of this name and type.
    In(&'a str, Ty),
    /// The output of an earlier node in the expression.
    N(usize),
    /// A constant float.
    F(f64),
}

/// Builds a pure expression out of Seal nodes. Each classic input goes
/// through one pass node, so a pin can feed several places.
struct Expr {
    m: Mapped,
    ins: HashMap<String, usize>,
}

/// One axis of a rotation in pitch, yaw, roll degrees, as the classic
/// rotation matrix gives it: 0 forward (X), 1 right (Y), 2 up (Z).
fn rot_axis(e: &mut Expr, rot: Arg, axis: u8) -> usize {
    use Arg::*;
    let deg = std::f64::consts::PI / 180.0;
    let part = |e: &mut Expr, k: &str| {
        let c = e.node(Node::new(0, k), &[("v", rot)]);
        let r = e.op("mul", Ty::Float, &[("a", N(c)), ("b", F(deg))]);
        (e.op("sin", Ty::Float, &[("a", N(r))]), e.op("cos", Ty::Float, &[("a", N(r))]))
    };
    let (sp, cp) = part(e, "vec3_x");
    let (sy, cy) = part(e, "vec3_y");
    let (sr, cr) = part(e, "vec3_z");
    let mul =
        |e: &mut Expr, a: usize, b: usize| e.op("mul", Ty::Float, &[("a", N(a)), ("b", N(b))]);
    let (x, y, z) = match axis {
        0 => (mul(e, cp, cy), mul(e, cp, sy), sp),
        1 => {
            let srsp = mul(e, sr, sp);
            let a = mul(e, srsp, cy);
            let b = mul(e, cr, sy);
            let x = e.op("sub", Ty::Float, &[("a", N(a)), ("b", N(b))]);
            let a = mul(e, srsp, sy);
            let b = mul(e, cr, cy);
            let y = e.op("add", Ty::Float, &[("a", N(a)), ("b", N(b))]);
            let m = mul(e, sr, cp);
            (x, y, e.op("neg", Ty::Float, &[("a", N(m))]))
        }
        _ => {
            let crsp = mul(e, cr, sp);
            let a = mul(e, crsp, cy);
            let b = mul(e, sr, sy);
            let s = e.op("add", Ty::Float, &[("a", N(a)), ("b", N(b))]);
            let x = e.op("neg", Ty::Float, &[("a", N(s))]);
            let a = mul(e, cy, sr);
            let b = mul(e, crsp, sy);
            let y = e.op("sub", Ty::Float, &[("a", N(a)), ("b", N(b))]);
            (x, y, mul(e, cr, cp))
        }
    };
    e.node(Node::new(0, "vec3"), &[("x", N(x)), ("y", N(y)), ("z", N(z))])
}

impl Expr {
    fn new() -> Self {
        Self { m: Mapped { nodes: Vec::new(), ..Mapped::new("select") }, ins: HashMap::new() }
    }
    fn input(&mut self, pin: &str, ty: Ty) -> usize {
        if let Some(&i) = self.ins.get(pin) {
            return i;
        }
        let i = self.m.add(pass(ty));
        self.m.pins.insert(pin.into(), (i, "a".into()));
        self.ins.insert(pin.into(), i);
        i
    }
    fn node(&mut self, mut node: Node, args: &[(&str, Arg)]) -> usize {
        let mut wires = Vec::new();
        for (port, a) in args {
            match *a {
                Arg::In(pin, ty) => wires.push((self.input(pin, ty), *port)),
                Arg::N(i) => wires.push((i, *port)),
                Arg::F(v) => put_default(&mut node, port, SealValue::Number(format!("{v:?}"))),
            }
        }
        let n = self.m.add(node);
        for (from, port) in wires {
            self.m.wire(from, "out", n, port);
        }
        n
    }
    fn op(&mut self, kind: &str, ty: Ty, args: &[(&str, Arg)]) -> usize {
        self.node(typed(kind, ty), args)
    }
    fn cmp(&mut self, op: &str, ty: Ty, a: Arg, b: Arg) -> usize {
        self.node(
            typed("compare", ty).with("op", SealValue::String(op.into())),
            &[("a", a), ("b", b)],
        )
    }
    fn sel(&mut self, ty: Ty, cond: usize, a: Arg, b: Arg) -> usize {
        self.node(typed("select", ty), &[("cond", Arg::N(cond)), ("a", a), ("b", b)])
    }
    /// `x < 0 ? 0 : (x < 1 ? x : 1)`, the classic clamp to 0..1.
    fn clamp01(&mut self, x: usize) -> usize {
        let under = self.cmp("lt", Ty::Float, Arg::N(x), Arg::F(1.0));
        let inner = self.sel(Ty::Float, under, Arg::N(x), Arg::F(1.0));
        let below = self.cmp("lt", Ty::Float, Arg::N(x), Arg::F(0.0));
        self.sel(Ty::Float, below, Arg::F(0.0), Arg::N(inner))
    }
    fn out(mut self, pin: &str, node: usize) -> Mapped {
        self.m.pins.insert(pin.into(), (node, "out".into()));
        self.m
    }
}

/// FInterpTo and VInterpTo: move Current towards Target by
/// `clamp(DeltaTime * InterpSpeed, 0, 1)` of the way; a speed of zero or
/// less, or a gap whose square is under 1e-8, gives Target.
fn interp_to(ty: Ty) -> Mapped {
    use Arg::*;
    let mut e = Expr::new();
    let (cur, tgt) = (In("Current", ty), In("Target", ty));
    let dist = e.op("sub", ty, &[("a", tgt), ("b", cur)]);
    let d2 = if ty == Ty::Float {
        e.op("mul", ty, &[("a", N(dist)), ("b", N(dist))])
    } else {
        e.op("dot", ty, &[("a", N(dist)), ("b", N(dist))])
    };
    let step = e.op(
        "mul",
        Ty::Float,
        &[("a", In("DeltaTime", Ty::Float)), ("b", In("InterpSpeed", Ty::Float))],
    );
    let alpha = e.clamp01(step);
    let delta = if ty == Ty::Float {
        e.op("mul", ty, &[("a", N(dist)), ("b", N(alpha))])
    } else {
        e.op("scale", ty, &[("v", N(dist)), ("s", N(alpha))])
    };
    let moved = e.op("add", ty, &[("a", cur), ("b", N(delta))]);
    let near = e.cmp("lt", Ty::Float, N(d2), F(1e-8));
    let r1 = e.sel(ty, near, tgt, N(moved));
    let slow = e.cmp("le", Ty::Float, In("InterpSpeed", Ty::Float), F(0.0));
    let r = e.sel(ty, slow, tgt, N(r1));
    e.out("ReturnValue", r)
}

/// MapRangeClamped: where Value sits between the in-range ends, clamped to
/// 0..1, then the same point between the out-range ends. An in-range whose
/// length is within 1e-8 of zero gives 1 at or past its end, else 0.
fn map_range_clamped() -> Mapped {
    use Arg::*;
    let f = Ty::Float;
    let mut e = Expr::new();
    let (v, a, b) = (In("Value", f), In("InRangeA", f), In("InRangeB", f));
    let span = e.op("sub", f, &[("a", b), ("b", a)]);
    let off = e.op("sub", f, &[("a", v), ("b", a)]);
    let pct = e.op("div", f, &[("a", N(off)), ("b", N(span))]);
    let size = e.op("abs", f, &[("a", N(span))]);
    let flat = e.cmp("le", f, N(size), F(1e-8));
    let past = e.cmp("ge", f, v, b);
    let step = e.sel(f, past, F(1.0), F(0.0));
    let p = e.sel(f, flat, N(step), N(pct));
    let alpha = e.clamp01(p);
    let (oa, ob) = (In("OutRangeA", f), In("OutRangeB", f));
    let d = e.op("sub", f, &[("a", ob), ("b", oa)]);
    let t = e.op("mul", f, &[("a", N(alpha)), ("b", N(d))]);
    let r = e.op("add", f, &[("a", oa), ("b", N(t))]);
    e.out("ReturnValue", r)
}

fn expand_math(member: &str) -> Option<Mapped> {
    use Ty::{Float, Int, Vec2, Vec3};
    const A: [(&str, &str); 2] = [("A", "a"), ("ReturnValue", "out")];
    const AB: [(&str, &str); 3] = [("A", "a"), ("B", "b"), ("ReturnValue", "out")];
    Some(match member {
        "EqualEqual_StrStr"
        | "NotEqual_StrStr"
        | "EqualEqual_NameName"
        | "NotEqual_NameName"
        | "EqualEqual_IgnoreCase_TextText"
        | "NotEqual_IgnoreCase_TextText" => {
            // The classic string and name comparisons ignore case.
            let op = if member.starts_with("Equal") { "eq" } else { "ne" };
            let mut m = Mapped::new("compare").s("type", "str").s("op", op);
            let la = m.add(Node::new(0, "str_lower"));
            let lb = m.add(Node::new(0, "str_lower"));
            m.wire(la, "out", 0, "a");
            m.wire(lb, "out", 0, "b");
            m.at("A", la, "a").at("B", lb, "a").pin("ReturnValue", "out")
        }
        "TextIsEmpty" | "TextIsTransient" | "TextIsCultureInvariant" => {
            // Seal text is a plain string: empty is "", nothing is transient,
            // and every text reads the same in every language.
            let mut m = Mapped::new("compare").s("type", "str").s("op", "eq");
            put_default(&mut m.nodes[0], "b", SealValue::String(String::new()));
            if member != "TextIsEmpty" {
                m = Mapped::new("const").s("type", "bool");
                m.nodes[0].set("value", SealValue::Bool(member == "TextIsCultureInvariant"));
                return Some(m.pin("ReturnValue", "out"));
            }
            m.pin("InText", "a").pin("ReturnValue", "out")
        }
        "EqualEqual_ObjectObject" | "NotEqual_ObjectObject" => {
            let op = if member.starts_with("Equal") { "eq" } else { "ne" };
            one("compare", Some(Ty::Entity), &AB).s("op", op)
        }
        "Conv_TextToString" | "Conv_StringToText" | "Conv_NameToString" | "Conv_StringToName"
        | "Conv_NameToText" => {
            // Text and names are plain strings in Seal.
            let m = Mapped { nodes: vec![pass(Ty::Str)], ..Mapped::new("select") };
            m.pin("InText", "a").pin("InString", "a").pin("InName", "a").pin("ReturnValue", "out")
        }
        "Conv_StringToInt" | "Conv_StringToInt64" => {
            one("parse_int", None, &[("InString", "a"), ("ReturnValue", "out")])
        }
        "Conv_StringToFloat" | "Conv_StringToDouble" => {
            one("parse_float", None, &[("InString", "a"), ("ReturnValue", "out")])
        }
        "RandomIntegerInRange" => {
            one("random_int", None, &[("Min", "a"), ("Max", "b"), ("ReturnValue", "out")])
        }
        "RandomFloatInRange" => {
            one("random_float", None, &[("Min", "a"), ("Max", "b"), ("ReturnValue", "out")])
        }
        "RandomInteger" => {
            // From 0 up to but not including Max; 0 when Max is not positive.
            let mut m = Mapped { nodes: vec![typed("sub", Int)], ..Mapped::new("select") };
            put_default(&mut m.nodes[0], "b", SealValue::Number("1".into()));
            let mut r = Node::new(0, "random_int");
            put_default(&mut r, "a", SealValue::Number("0".into()));
            let r = m.add(r);
            m.wire(0, "out", r, "b");
            m.pin("Max", "a").at("ReturnValue", r, "out")
        }
        "RandomBool" => {
            let mut r = Node::new(0, "random_int");
            put_default(&mut r, "a", SealValue::Number("0".into()));
            put_default(&mut r, "b", SealValue::Number("1".into()));
            let mut m = Mapped { nodes: vec![r], ..Mapped::new("select") };
            let mut eq = typed("compare", Int).with("op", SealValue::String("eq".into()));
            put_default(&mut eq, "b", SealValue::Number("1".into()));
            let eq = m.add(eq);
            m.wire(0, "out", eq, "a");
            m.at("ReturnValue", eq, "out")
        }
        "RandomFloat" => {
            let mut m = one("random_float", None, &[("ReturnValue", "out")]);
            put_default(&mut m.nodes[0], "a", SealValue::Number("0.0".into()));
            put_default(&mut m.nodes[0], "b", SealValue::Number("1.0".into()));
            m
        }
        "GetWorldDeltaSeconds" => one("frame_dt", None, &[("ReturnValue", "out")]),
        "GetGameTimeInSeconds" | "GetTimeSeconds" => {
            one("game_time", None, &[("ReturnValue", "out")])
        }
        "Sin" => one("sin", None, &A),
        "Cos" => one("cos", None, &A),
        "DegSin" | "DegCos" => {
            // Degrees to radians, then the radian function.
            let mut m = Mapped::new("mul").s("type", "float");
            put_default(
                &mut m.nodes[0],
                "b",
                SealValue::Number(format!("{:?}", std::f64::consts::PI / 180.0)),
            );
            let f = m.add(Node::new(0, if member == "DegSin" { "sin" } else { "cos" }));
            m.wire(0, "out", f, "a");
            m.pin("A", "a").at("ReturnValue", f, "out")
        }
        "Square" => {
            let mut m = Mapped { nodes: vec![pass(Float)], ..Mapped::new("select") };
            let r = m.add(typed("mul", Float));
            m.wire(0, "out", r, "a");
            m.wire(0, "out", r, "b");
            m.pin("A", "a").at("ReturnValue", r, "out")
        }
        "BooleanXOR" => {
            let mut m = Mapped::new("compare").s("type", "bool").s("op", "ne");
            m = m.pin("A", "a").pin("B", "b").pin("ReturnValue", "out");
            m
        }
        "BooleanNAND" | "BooleanNOR" => {
            let mut m = Mapped::new(if member == "BooleanNAND" { "and" } else { "or" });
            let n = m.add(Node::new(0, "not"));
            m.wire(0, "out", n, "a");
            m.pin("A", "a").pin("B", "b").at("ReturnValue", n, "out")
        }
        "Tan" => {
            let mut m = Mapped { nodes: vec![pass(Float)], ..Mapped::new("select") };
            let s = m.add(Node::new(0, "sin"));
            let c = m.add(Node::new(0, "cos"));
            let d = m.add(typed("div", Float));
            m.wire(0, "out", s, "a");
            m.wire(0, "out", c, "a");
            m.wire(s, "out", d, "a");
            m.wire(c, "out", d, "b");
            m.pin("A", "a").at("ReturnValue", d, "out")
        }
        "SignOfInteger" | "SignOfFloat" => {
            // A > 0 ? 1 : (A < 0 ? -1 : 0)
            let ty = if member == "SignOfInteger" { Int } else { Float };
            let lit = |v: i32| {
                SealValue::Number(if ty == Int {
                    v.to_string()
                } else {
                    format!("{:?}", f64::from(v))
                })
            };
            let mut m = Mapped { nodes: vec![pass(ty)], ..Mapped::new("select") };
            let mut gt = typed("compare", ty);
            gt.set("op", SealValue::String("gt".into()));
            put_default(&mut gt, "b", lit(0));
            let mut lt = typed("compare", ty);
            lt.set("op", SealValue::String("lt".into()));
            put_default(&mut lt, "b", lit(0));
            let gt = m.add(gt);
            let lt = m.add(lt);
            let mut neg = typed("select", ty);
            put_default(&mut neg, "a", lit(-1));
            put_default(&mut neg, "b", lit(0));
            let neg = m.add(neg);
            let mut top = typed("select", ty);
            put_default(&mut top, "a", lit(1));
            let top = m.add(top);
            m.wire(0, "out", gt, "a");
            m.wire(0, "out", lt, "a");
            m.wire(lt, "out", neg, "cond");
            m.wire(gt, "out", top, "cond");
            m.wire(neg, "out", top, "b");
            m.pin("A", "a").at("ReturnValue", top, "out")
        }
        "Conv_BoolToInt" => {
            let mut m = Mapped::new("select").s("type", "int");
            put_default(&mut m.nodes[0], "a", SealValue::Number("1".into()));
            put_default(&mut m.nodes[0], "b", SealValue::Number("0".into()));
            m.pin("InBool", "cond").pin("ReturnValue", "out")
        }
        "Conv_IntToBool" => {
            let mut m = Mapped::new("compare").s("type", "int").s("op", "ne");
            put_default(&mut m.nodes[0], "b", SealValue::Number("0".into()));
            m.pin("InInt", "a").pin("ReturnValue", "out")
        }
        "NearlyEqual_FloatFloat" => {
            // |A - B| <= ErrorTolerance
            let mut m = Mapped { nodes: vec![typed("sub", Float)], ..Mapped::new("sub") };
            let a = m.add(typed("abs", Float));
            let mut le = typed("compare", Float);
            le.set("op", SealValue::String("le".into()));
            let c = m.add(le);
            m.wire(0, "out", a, "a");
            m.wire(a, "out", c, "a");
            m.pin("A", "a").pin("B", "b").at("ErrorTolerance", c, "b").at("ReturnValue", c, "out")
        }
        "Conv_ByteToInt" | "Conv_Int64ToInt" | "Conv_IntToInt64" | "Conv_ByteToInt64" => {
            let mut m = Mapped { nodes: vec![pass(Int)], ..Mapped::new("select") };
            m = m
                .pin("InByte", "a")
                .pin("InInt", "a")
                .pin("InInt64", "a")
                .pin("ReturnValue", "out");
            m
        }
        "Exp" => one("exp", None, &A),
        "Loge" => one("ln", None, &A),
        "Asin" => one("asin", None, &A),
        "Acos" => one("acos", None, &A),
        "Atan" => one("atan", None, &A),
        "MultiplyMultiply_FloatFloat" => {
            one("pow", None, &[("Base", "a"), ("Exp", "b"), ("ReturnValue", "out")])
        }
        "Atan2" => one("atan2", None, &[("Y", "a"), ("X", "b"), ("ReturnValue", "out")]),
        "DegAsin" | "DegAcos" | "DegAtan" | "DegAtan2" => {
            // The radian function, then radians to degrees.
            let kind = &member[3..].to_lowercase();
            let mut m = Mapped::new(kind);
            let mut deg = typed("mul", Float);
            put_default(
                &mut deg,
                "b",
                SealValue::Number(format!("{:?}", 180.0 / std::f64::consts::PI)),
            );
            let d = m.add(deg);
            m.wire(0, "out", d, "a");
            if member == "DegAtan2" {
                m = m.pin("Y", "a").pin("X", "b");
            } else {
                m = m.pin("A", "a");
            }
            m.at("ReturnValue", d, "out")
        }
        "Log" => {
            // ln(A) / ln(Base)
            let mut m = Mapped::new("ln");
            let lb = m.add(Node::new(0, "ln"));
            let d = m.add(typed("div", Float));
            m.wire(0, "out", d, "a");
            m.wire(lb, "out", d, "b");
            m.pin("A", "a").at("Base", lb, "a").at("ReturnValue", d, "out")
        }
        "Hypotenuse" => {
            let mut m = Mapped { nodes: vec![pass(Float)], ..Mapped::new("select") };
            let b = m.add(pass(Float));
            let aa = m.add(typed("mul", Float));
            let bb = m.add(typed("mul", Float));
            let sum = m.add(typed("add", Float));
            let root = m.add(Node::new(0, "sqrt"));
            m.wire(0, "out", aa, "a");
            m.wire(0, "out", aa, "b");
            m.wire(b, "out", bb, "a");
            m.wire(b, "out", bb, "b");
            m.wire(aa, "out", sum, "a");
            m.wire(bb, "out", sum, "b");
            m.wire(sum, "out", root, "a");
            m.pin("Width", "a").at("Height", b, "a").at("ReturnValue", root, "out")
        }
        "Conv_Int64ToDouble" | "Conv_ByteToFloat" | "Conv_ByteToDouble" | "Conv_Int64ToFloat" => {
            one("to_float", None, &[("InInt64", "a"), ("InByte", "a"), ("ReturnValue", "out")])
        }
        "Conv_FloatToDouble" | "Conv_DoubleToFloat" => {
            let m = Mapped { nodes: vec![pass(Float)], ..Mapped::new("select") };
            m.pin("InFloat", "a").pin("InDouble", "a").pin("ReturnValue", "out")
        }
        "Conv_IntToByte" => {
            let m = Mapped { nodes: vec![pass(Int)], ..Mapped::new("select") };
            m.pin("InInt", "a").pin("ReturnValue", "out")
        }
        "Floor" => one("floor", None, &A),
        "Round" => one("round", None, &A),
        "Ceil" => one("ceil", None, &A),
        "VSize2D" => one("length", None, &[("A", "v"), ("ReturnValue", "out")]),
        "Normal2D" => one("normalize", None, &[("A", "v"), ("ReturnValue", "out")]),
        "DotProduct2D" => one("dot", None, &AB),
        "Negated2D" => one("neg", Some(Vec2), &A),
        "Add_Vector2DVector2D" => one("add", Some(Vec2), &AB),
        "Subtract_Vector2DVector2D" => one("sub", Some(Vec2), &AB),
        "Multiply_Vector2DFloat" => {
            one("scale", None, &[("A", "v"), ("B", "s"), ("ReturnValue", "out")])
        }
        "MakeVector2D" => one("vec2", None, &[("X", "x"), ("Y", "y"), ("ReturnValue", "out")]),
        "BreakVector2D" => {
            let mut m = Mapped::new("select").s("type", "vec2");
            put_default(&mut m.nodes[0], "cond", SealValue::Bool(true));
            let x = m.add(Node::new(0, "vec2_x"));
            let y = m.add(Node::new(0, "vec2_y"));
            m.wire(0, "out", x, "v");
            m.wire(0, "out", y, "v");
            m.pin("InVec", "a").at("X", x, "out").at("Y", y, "out")
        }
        "VSize" => one("length", Some(Vec3), &[("A", "v"), ("ReturnValue", "out")]),
        "Normal" => one("normalize", Some(Vec3), &[("A", "v"), ("ReturnValue", "out")]),
        "Dot_VectorVector" => one("dot", Some(Vec3), &AB),
        "Cross_VectorVector" => one("cross", None, &AB),
        "NegateVector" => one("neg", Some(Vec3), &[("A", "a"), ("ReturnValue", "out")]),
        "Add_VectorVector" => one("add", Some(Vec3), &AB),
        "Subtract_VectorVector" => one("sub", Some(Vec3), &AB),
        "Multiply_VectorFloat" => {
            one("scale", Some(Vec3), &[("A", "v"), ("B", "s"), ("ReturnValue", "out")])
        }
        "MakeVector" => {
            one("vec3", None, &[("X", "x"), ("Y", "y"), ("Z", "z"), ("ReturnValue", "out")])
        }
        // Rotators are (pitch, yaw, roll) 3D vectors.
        "MakeRotator" => one(
            "vec3",
            None,
            &[("Pitch", "x"), ("Yaw", "y"), ("Roll", "z"), ("ReturnValue", "out")],
        ),
        "BreakRotator" => {
            let mut m = Mapped::new("select").s("type", "vec3");
            put_default(&mut m.nodes[0], "cond", SealValue::Bool(true));
            let x = m.add(Node::new(0, "vec3_x"));
            let y = m.add(Node::new(0, "vec3_y"));
            let z = m.add(Node::new(0, "vec3_z"));
            for n in [x, y, z] {
                m.wire(0, "out", n, "v");
            }
            m.pin("InRot", "a").at("Pitch", x, "out").at("Yaw", y, "out").at("Roll", z, "out")
        }
        "BreakVector" => {
            let mut m = Mapped::new("select").s("type", "vec3");
            put_default(&mut m.nodes[0], "cond", SealValue::Bool(true));
            let x = m.add(Node::new(0, "vec3_x"));
            let y = m.add(Node::new(0, "vec3_y"));
            let z = m.add(Node::new(0, "vec3_z"));
            for n in [x, y, z] {
                m.wire(0, "out", n, "v");
            }
            m.pin("InVec", "a").at("X", x, "out").at("Y", y, "out").at("Z", z, "out")
        }
        "FInterpTo" => interp_to(Float),
        "FMod" | "FMod64" => {
            // Whole quotient toward zero, and the remainder; both 0 for a
            // zero divisor.
            use Arg::*;
            let mut e = Expr::new();
            let (a, b) = (In("Dividend", Float), In("Divisor", Float));
            let rem = e.op("mod", Float, &[("a", a), ("b", b)]);
            let q = e.op("div", Float, &[("a", a), ("b", b)]);
            let nz = e.cmp("ne", Float, b, F(0.0));
            let safe = e.sel(Float, nz, N(q), F(0.0));
            let whole = e.node(Node::new(0, "to_int"), &[("a", N(safe))]);
            let mut m = e.out("ReturnValue", whole);
            m.pins.insert("Remainder".into(), (rem, "out".into()));
            m
        }
        "GetForwardVector" | "GetRightVector" | "GetUpVector" => {
            let axis = match member {
                "GetForwardVector" => 0,
                "GetRightVector" => 1,
                _ => 2,
            };
            let mut e = Expr::new();
            let v = rot_axis(&mut e, Arg::In("InRot", Ty::Vec3), axis);
            e.out("ReturnValue", v)
        }
        "VInterpTo" => interp_to(Vec3),
        "MapRangeClamped" => map_range_clamped(),
        "Lerp" => {
            // A + Alpha * (B - A), in the classic order of operations.
            let mut m = Mapped { nodes: vec![pass(Float)], ..Mapped::new("select") };
            let d = m.add(typed("sub", Float));
            let t = m.add(typed("mul", Float));
            let r = m.add(typed("add", Float));
            m.wire(0, "out", d, "b");
            m.wire(d, "out", t, "b");
            m.wire(0, "out", r, "a");
            m.wire(t, "out", r, "b");
            m.pin("A", "a").at("B", d, "a").at("Alpha", t, "a").at("ReturnValue", r, "out")
        }
        "Clamp" | "FClamp" => {
            // X < Min ? Min : (X < Max ? X : Max), exactly as the classic clamp.
            let ty = if member == "Clamp" { Int } else { Float };
            let mut m = Mapped { nodes: vec![pass(ty)], ..Mapped::new("select") };
            let lo = m.add(pass(ty));
            let hi = m.add(pass(ty));
            let mut lt = typed("compare", ty);
            lt.set("op", SealValue::String("lt".into()));
            let below = m.add(lt.clone());
            let under = m.add(lt);
            let inner = m.add(typed("select", ty));
            let outer = m.add(typed("select", ty));
            m.wire(0, "out", below, "a");
            m.wire(lo, "out", below, "b");
            m.wire(0, "out", under, "a");
            m.wire(hi, "out", under, "b");
            m.wire(under, "out", inner, "cond");
            m.wire(0, "out", inner, "a");
            m.wire(hi, "out", inner, "b");
            m.wire(below, "out", outer, "cond");
            m.wire(lo, "out", outer, "a");
            m.wire(inner, "out", outer, "b");
            m.pin("Value", "a").at("Min", lo, "a").at("Max", hi, "a").at(
                "ReturnValue",
                outer,
                "out",
            )
        }
        _ => return None,
    })
}

/// The entity an actor function acts on: its linked `self` pin, or this object.
fn target(m: &mut Mapped, node: usize, pin: &str, o: &Object) {
    if o.pin("self").is_some_and(|p| !p.linked.is_empty()) {
        m.pins.insert("self".into(), (node, pin.into()));
    } else {
        let me = m.add(Node::new(0, "self"));
        m.wire(me, "out", node, pin);
    }
}

/// Overlap, hit and destroyed events. Seal fires `overlap_begin`,
/// `overlap_end` and `hit` with the other object, and `destroyed` as an
/// object is removed. A hit also carries where the objects met and the
/// normal; other hit details have no Seal value yet and are reported.
fn touch_event(o: &Object, m: &str, seen: &mut Vec<String>) -> Result<Mapped, Status> {
    // Each event's carried outputs: (classic pin, Seal parameter, type).
    let (name, carried): (&str, &[(&str, &str, &str)]) = match m {
        "ReceiveActorBeginOverlap" => ("overlap_begin", &[("OtherActor", "other", "entity")]),
        "ReceiveActorEndOverlap" => ("overlap_end", &[("OtherActor", "other", "entity")]),
        "ReceiveHit" => (
            "hit",
            &[
                ("Other", "other", "entity"),
                ("HitLocation", "location", "vec3"),
                ("HitNormal", "normal", "vec3"),
            ],
        ),
        _ => ("destroyed", &[]),
    };
    if seen.iter().any(|s| s == name) {
        return Err(Status::Unsupported("a second copy of this event".into()));
    }
    if let Some(p) = o.pins.iter().find(|p| {
        p.output
            && !p.linked.is_empty()
            && p.name != "then"
            && !carried.iter().any(|c| c.0 == p.name)
    }) {
        return Err(Status::Unsupported(format!(
            "reads {} from the event, which Seal does not carry",
            p.name
        )));
    }
    seen.push(name.to_string());
    let mut ev = Mapped::new("event").s("name", name).pin("then", "then");
    if !carried.is_empty() {
        let params = carried
            .iter()
            .map(|(_, n, t)| {
                SealValue::Map(SealMap::new(vec![
                    ("name".into(), SealValue::String((*n).into())),
                    ("type".into(), SealValue::String((*t).into())),
                ]))
            })
            .collect();
        ev.nodes[0].set("params", SealValue::Array(params));
        for (pin, n, _) in carried {
            ev = ev.pin(pin, n);
        }
    }
    Ok(ev)
}

/// How fast movement input moves an object at full push, in scene units per
/// second (the classic default walking speed).
pub const MOVE_SPEED: f64 = 600.0;
/// The upward speed a jump starts with (the classic default).
pub const JUMP_SPEED: f64 = 420.0;

/// The entity an actor function acts on, through a pass-through whose second
/// input is never picked, so one target can feed several nodes.
fn target_pass(m: &mut Mapped, o: &Object) -> usize {
    let who = m.add(pass(Ty::Entity));
    target(m, who, "a", o);
    let unused = m.add(Node::new(0, "self"));
    m.wire(unused, "out", who, "b");
    who
}

/// AddMovementInput moves the target straight away by
/// `WorldDirection * ScaleValue * MOVE_SPEED * frame_dt`. There is no walking
/// physics yet, so speed limits, friction and the force flag do nothing.
fn movement_input(o: &Object) -> Mapped {
    let mut set = Mapped { nodes: vec![location_field("world_set")], ..Mapped::new("world_set") };
    set = set.pin("execute", "in").pin("then", "then");
    let who = target_pass(&mut set, o);
    let get = set.add(location_field("world_get"));
    set.wire(who, "out", get, "entity");
    set.wire(who, "out", 0, "entity");
    let dir = set.add(pass(Ty::Vec3));
    put_vec3_default(&mut set, o, "WorldDirection", dir, "a");
    let amount = set.add(pass(Ty::Float));
    if o.pin("ScaleValue").and_then(|p| p.default.as_deref()).is_none_or(|t| t.trim().is_empty()) {
        put_default(&mut set.nodes[amount], "a", SealValue::Number("1.0".into()));
    }
    let speed = set.add(typed("mul", Ty::Float));
    set.wire(amount, "out", speed, "a");
    put_default(&mut set.nodes[speed], "b", SealValue::Number(format!("{MOVE_SPEED:?}")));
    let dt = set.add(Node::new(0, "frame_dt"));
    let step = set.add(typed("mul", Ty::Float));
    set.wire(speed, "out", step, "a");
    set.wire(dt, "out", step, "b");
    let delta = set.add(typed("scale", Ty::Vec3));
    set.wire(dir, "out", delta, "v");
    set.wire(step, "out", delta, "s");
    let add = set.add(typed("add", Ty::Vec3));
    set.wire(get, "out", add, "a");
    set.wire(delta, "out", add, "b");
    set.wire(add, "out", 0, "value");
    set.at("WorldDirection", dir, "a").at("ScaleValue", amount, "a")
}

/// Jump sets the target's upward (z) speed to JUMP_SPEED and keeps the rest;
/// StopJumping has nothing to stop, since a jump is one push.
fn jump(o: &Object) -> Mapped {
    let mut set = Mapped {
        nodes: vec![field_node("world_set", "body", "velocity", "vec3")],
        ..Mapped::new("world_set")
    };
    set = set.pin("execute", "in").pin("then", "then");
    let who = target_pass(&mut set, o);
    let get = set.add(field_node("world_get", "body", "velocity", "vec3"));
    set.wire(who, "out", get, "entity");
    set.wire(who, "out", 0, "entity");
    let x = set.add(Node::new(0, "vec3_x"));
    let y = set.add(Node::new(0, "vec3_y"));
    set.wire(get, "out", x, "v");
    set.wire(get, "out", y, "v");
    let v = set.add(Node::new(0, "vec3"));
    set.wire(x, "out", v, "x");
    set.wire(y, "out", v, "y");
    put_default(&mut set.nodes[v], "z", SealValue::Number(format!("{JUMP_SPEED:?}")));
    set.wire(v, "out", 0, "value");
    set
}

/// The project path of a sound asset reference: `/Game/Audio/Jump.Jump`
/// becomes `Audio/Jump` (the player adds `.wav`).
pub fn sound_path(asset: &str) -> String {
    // Also accepts `SoundWave'/Game/Audio/Jump.Jump'`.
    let t = asset.trim().trim_matches('"');
    let t = t.split_once('\'').map_or(t, |(_, rest)| rest.split('\'').next().unwrap_or(""));
    let t = t.split_once('.').map_or(t, |(p, _)| p);
    t.strip_prefix("/Game/").unwrap_or(t.trim_start_matches('/')).to_string()
}

/// The widget name of a widget class reference:
/// `WidgetBlueprintGeneratedClass'/Game/UI/WBP_Hud.WBP_Hud_C'` is `WBP_Hud`.
pub fn widget_name(class: &str) -> String {
    let p = sound_path(class);
    p.rsplit('/').next().unwrap_or("").to_string()
}

/// CreateWidget passes execution on and yields the widget's name, which
/// the on-screen widget nodes use as its handle.
fn create_widget(o: &Object) -> Result<Mapped, Status> {
    let Some(class) = o.pin("Class") else {
        return Err(Status::Unsupported("no widget class".into()));
    };
    if !class.linked.is_empty() {
        return Err(Status::Unsupported("widget class driven by a link".into()));
    }
    let name = class.default_object.as_deref().map(widget_name).unwrap_or_default();
    if name.is_empty() || name == "None" {
        return Err(Status::Unsupported("no widget class chosen".into()));
    }
    let mut m = Mapped::new("sequence").pin("execute", "in").pin("then", "then0");
    m.nodes[0].set("count", SealValue::Number("1".into()));
    let c = m.add(typed("const", Ty::Str).with("value", SealValue::String(name)));
    Ok(m.at("ReturnValue", c, "out"))
}

/// A widget-element variable read whose every use is the target of SetText.
fn feeds_only_set_text(o: &Object, objects: &[Object]) -> bool {
    let links: Vec<_> = o.pins.iter().flat_map(|p| p.linked.iter()).collect();
    !links.is_empty()
        && links.iter().all(|(node, pin)| {
            objects.iter().find(|x| &x.name == node).is_some_and(|x| {
                x.pin("self").is_some_and(|p| &p.id == pin)
                    && x.short_class() == "K2Node_CallFunction"
                    && member(x, "FunctionReference").as_deref() == Some("SetText")
                    && x.pin("InText").is_some()
            })
        })
}

/// AddToViewport / RemoveFromParent show and hide a widget; SetText on a
/// text element read from a widget variable sets that element's text.
fn widget_call(o: &Object, m: &str, objects: &[Object], own: &str) -> Result<Mapped, Status> {
    let linked_self = o.pin("self").is_some_and(|p| !p.linked.is_empty());
    if m == "SetText" {
        let element = o
            .pin("self")
            .filter(|p| p.linked.len() == 1)
            .and_then(|p| objects.iter().find(|x| x.name == p.linked[0].0))
            .filter(|x| x.short_class() == "K2Node_VariableGet")
            .and_then(|x| member(x, "VariableReference"))
            .ok_or_else(|| {
                Status::Unsupported(
                    "setting text on an element not read straight from a widget variable".into(),
                )
            })?;
        let mut t =
            Mapped::new("ui_text").pin("execute", "in").pin("then", "then").pin("InText", "text");
        put_default(&mut t.nodes[0], "element", SealValue::String(element));
        return Ok(t);
    }
    let show = matches!(m, "AddToViewport" | "AddToPlayerScreen");
    let mut s = Mapped::new("ui_show").pin("execute", "in").pin("then", "then");
    put_default(&mut s.nodes[0], "visible", SealValue::Bool(show));
    if linked_self {
        s = s.pin("self", "widget");
    } else {
        put_default(&mut s.nodes[0], "widget", SealValue::String(own.to_string()));
    }
    Ok(s)
}

/// PlaySound2D, PlaySoundAtLocation and the SpawnSound variants play the
/// sound once. Settings beyond volume and pitch are not carried.
fn sound_call(o: &Object, m: &str) -> Result<Mapped, Status> {
    let at = m.contains("AtLocation");
    if m.starts_with("SpawnSound") && o.pin("ReturnValue").is_some_and(|p| !p.linked.is_empty()) {
        return Err(Status::Unsupported("using the spawned sound component".into()));
    }
    for p in ["AttenuationSettings", "ConcurrencySettings", "OwningActor", "InitialParams"] {
        if o.pin(p).is_some_and(|p| !p.linked.is_empty()) {
            return Err(Status::Unsupported(format!("{p} driven by a link")));
        }
    }
    let Some(sound) = o.pin("Sound") else {
        return Err(Status::Unsupported("no Sound pin".into()));
    };
    let mut s = Mapped::new(if at { "play_sound_at" } else { "play_sound" })
        .pin("execute", "in")
        .pin("then", "then")
        .pin("VolumeMultiplier", "volume")
        .pin("PitchMultiplier", "pitch");
    if at {
        s = s.pin("Location", "location");
    }
    if sound.linked.is_empty() {
        let path = sound.default_object.as_deref().map(sound_path).unwrap_or_default();
        if path.is_empty() || path == "None" {
            return Err(Status::Unsupported("no sound chosen".into()));
        }
        put_default(&mut s.nodes[0], "sound", SealValue::String(path));
    } else {
        s = s.pin("Sound", "sound");
    }
    put_default(&mut s.nodes[0], "volume", SealValue::Number("1.0".into()));
    put_default(&mut s.nodes[0], "pitch", SealValue::Number("1.0".into()));
    Ok(s)
}

/// AttachActorToActor and K2_AttachToActor set `attach.<rule>` on the
/// target to the parent; DetachFromActor sets `attach.detach`. Only the
/// location rule matters: attached objects follow their parent's position.
fn attach_call(o: &Object, m: &str) -> Result<Mapped, Status> {
    let rule_of = |pin: &str| -> Result<&'static str, Status> {
        let Some(p) = o.pin(pin) else { return Ok("keep_relative") };
        if !p.linked.is_empty() {
            return Err(Status::Unsupported(format!("{pin} driven by a link")));
        }
        Ok(match p.default.as_deref().map(str::trim).unwrap_or("KeepRelative") {
            "SnapToTarget" => "snap",
            "KeepWorld" => "keep_world",
            "" | "KeepRelative" => "keep_relative",
            t => return Err(Status::Unsupported(format!("{pin} {t:?}"))),
        })
    };
    if m == "GetAttachParentActor" {
        let mut g = Mapped {
            nodes: vec![field_node("world_get", "attach", "parent", "entity")],
            ..Mapped::new("world_get")
        };
        target(&mut g, 0, "entity", o);
        return Ok(g.pin("ReturnValue", "out"));
    }
    if o.pin("SocketName")
        .and_then(|p| p.default.as_deref())
        .is_some_and(|t| !matches!(t.trim(), "" | "None"))
    {
        return Err(Status::Unsupported("attaching at a socket".into()));
    }
    let detach = m.contains("Detach");
    let rule = if detach { "detach" } else { rule_of("LocationRule")? };
    let mut set = Mapped {
        nodes: vec![field_node("world_set", "attach", rule, "entity")],
        ..Mapped::new("world_set")
    };
    set = set.pin("execute", "in").pin("then", "then");
    target(&mut set, 0, "entity", o);
    if detach {
        let unused = set.add(Node::new(0, "self"));
        set.wire(unused, "out", 0, "value");
        Ok(set)
    } else {
        Ok(set.pin("ParentActor", "value"))
    }
}

/// A `world_get` or `world_set` node for one component field.
fn field_node(kind: &str, component: &str, field: &str, ty: &str) -> Node {
    Node::new(0, kind)
        .with("component", SealValue::String(component.into()))
        .with("field", SealValue::String(field.into()))
        .with("type", SealValue::String(ty.into()))
}

fn location_field(kind: &str) -> Node {
    transform_field(kind, "translation")
}

/// A `world_get` or `world_set` of one `transform` field: translation,
/// rotation (pitch, yaw, roll in degrees) or scale.
fn transform_field(kind: &str, field: &str) -> Node {
    Node::new(0, kind)
        .with("component", SealValue::String("transform".into()))
        .with("field", SealValue::String(field.into()))
        .with("type", SealValue::String("vec3".into()))
}

/// Actor rotation and scale reads and writes, as the `transform`
/// component's `rotation` and `scale`. Rotations are pitch, yaw and roll.
fn rotation_scale_call(o: &Object, m: &str) -> Result<Mapped, Status> {
    let (field, input) = match m {
        "K2_GetActorRotation" => ("rotation", None),
        "GetActorScale3D" => ("scale", None),
        "K2_SetActorRotation" => ("rotation", Some("NewRotation")),
        "SetActorScale3D" => ("scale", Some("NewScale3D")),
        _ => return Err(Status::Unsupported(format!("{m} is not mapped yet"))),
    };
    let Some(input) = input else {
        let mut g =
            Mapped { nodes: vec![transform_field("world_get", field)], ..Mapped::new("world_get") };
        target(&mut g, 0, "entity", o);
        return Ok(g.pin("ReturnValue", "out"));
    };
    if o.pin("ReturnValue").is_some_and(|p| !p.linked.is_empty()) {
        return Err(Status::Unsupported("ReturnValue is used".into()));
    }
    // Teleporting only matters to physics, which follows the transform anyway.
    let mut set =
        Mapped { nodes: vec![transform_field("world_set", field)], ..Mapped::new("world_set") };
    set = set.pin("execute", "in").pin("then", "then");
    target(&mut set, 0, "entity", o);
    put_vec3_default(&mut set, o, input, 0, "value");
    Ok(set.pin(input, "value"))
}

/// Actor location reads and moves, as the `transform` component's
/// `translation`. Sweeping (stopping at collisions) is not mapped.
fn location_call(o: &Object, m: &str) -> Result<Mapped, Status> {
    if m == "K2_GetActorLocation" {
        let mut g = Mapped { nodes: vec![location_field("world_get")], ..Mapped::new("world_get") };
        target(&mut g, 0, "entity", o);
        return Ok(g.pin("ReturnValue", "out"));
    }
    if bool_input(o, "bSweep")? {
        return Err(Status::Unsupported("sweeping moves are not mapped".into()));
    }
    for out in ["SweepHitResult", "ReturnValue"] {
        if o.pin(out).is_some_and(|p| !p.linked.is_empty()) {
            return Err(Status::Unsupported(format!("{out} is used")));
        }
    }
    let mut set = Mapped { nodes: vec![location_field("world_set")], ..Mapped::new("world_set") };
    set = set.pin("execute", "in").pin("then", "then");
    if m == "K2_SetActorLocation" {
        target(&mut set, 0, "entity", o);
        put_vec3_default(&mut set, o, "NewLocation", 0, "value");
        return Ok(set.pin("NewLocation", "value"));
    }
    // Add the offset to where the actor is now. One `self`/target feeds both ends.
    let who = set.add(pass(Ty::Entity));
    target(&mut set, who, "a", o);
    // The pass-through never picks `b`, but every entity input needs a link.
    let unused = set.add(Node::new(0, "self"));
    set.wire(unused, "out", who, "b");
    let get = set.add(location_field("world_get"));
    let add = set.add(typed("add", Ty::Vec3));
    set.wire(who, "out", get, "entity");
    set.wire(who, "out", 0, "entity");
    set.wire(get, "out", add, "a");
    set.wire(add, "out", 0, "value");
    put_vec3_default(&mut set, o, "DeltaLocation", add, "b");
    Ok(set.at("DeltaLocation", add, "b"))
}

/// Rotation and scale are not carried yet, so a transform must keep them at
/// rest (no rotation, unit scale). Its location is all that is left.
fn rest_check(o: &Object, pin: &str, rest: (f64, f64, f64)) -> Result<(), Status> {
    let Some(p) = o.pin(pin) else { return Ok(()) };
    if !p.linked.is_empty() {
        return Err(Status::Unsupported(format!("transform {pin} driven by a link")));
    }
    let v = match p.default.as_deref().map(str::trim) {
        None | Some("") => rest,
        Some(t) => vec3_literal(t)
            .ok_or_else(|| Status::Unsupported(format!("transform {pin} {t:?} not readable")))?,
    };
    if v != rest {
        return Err(Status::Unsupported(format!("transform {pin} is not mapped yet")));
    }
    Ok(())
}

/// MakeTransform at rest becomes its location, a 3D vector.
fn make_transform(o: &Object) -> Result<Mapped, Status> {
    rest_check(o, "Rotation", (0.0, 0.0, 0.0))?;
    rest_check(o, "Scale", (1.0, 1.0, 1.0))?;
    Ok(Mapped { nodes: vec![pass(Ty::Vec3)], ..Mapped::new("select") }
        .pin("Location", "a")
        .pin("ReturnValue", "out"))
}

type V3t = (f64, f64, f64);

/// A transform default, `x,y,z|pitch,yaw,roll|sx,sy,sz`: location, rotation
/// and scale.
fn transform_literal(t: &str) -> Result<[V3t; 3], Status> {
    let bad = || Status::Unsupported(format!("spawn transform {t:?} not readable"));
    let parts: Vec<&str> = t.trim().split('|').collect();
    let [loc, rot, scale] = parts.as_slice() else { return Err(bad()) };
    Ok([
        vec3_literal(loc).ok_or_else(bad)?,
        vec3_literal(rot).ok_or_else(bad)?,
        vec3_literal(scale).ok_or_else(bad)?,
    ])
}

fn vec3_value((x, y, z): V3t) -> SealValue {
    SealValue::Array([x, y, z].map(|v| SealValue::Number(float_text(v))).to_vec())
}

fn put_vec3_default(m: &mut Mapped, o: &Object, pin: &str, node: usize, to: &str) {
    if let Some(v) =
        o.pin(pin).and_then(|p| p.default.as_deref()).and_then(|t| literal(Ty::Vec3, t))
    {
        put_default(&mut m.nodes[node], to, v);
    }
}

/// Switch on Int or String: one `branch` per case, tested in order, each
/// comparing the selection with that case; the last one falls to Default.
/// Set or clear a timer by function name. Seal timers run a custom event
/// with no inputs on this object, so the name must be a literal naming one.
/// The custom event a timer runs: by function name, or by the event wired
/// into its delegate pin (a custom event's own delegate or a Create Event
/// node naming one). It must take no inputs and return nothing.
fn timer_event<'a>(
    o: &Object,
    objects: &[Object],
    customs: &'a HashMap<String, CustomEvent>,
) -> Result<&'a CustomEvent, Status> {
    let fname = if let Some(p) = o.pin("FunctionName") {
        if !p.linked.is_empty() {
            return Err(Status::Unsupported("timer function name driven by a link".into()));
        }
        p.default.as_deref().unwrap_or("").trim().to_string()
    } else {
        let p = o
            .pin("Event")
            .or_else(|| o.pin("Delegate"))
            .filter(|p| p.linked.len() == 1)
            .ok_or_else(|| Status::Unsupported("timer without an event wired in".into()))?;
        let src = objects
            .iter()
            .find(|x| x.name == p.linked[0].0)
            .ok_or_else(|| Status::Unsupported("timer event is not in the selection".into()))?;
        match src.short_class() {
            "K2Node_CustomEvent" => declared_name(src).unwrap_or_default(),
            "K2Node_CreateDelegate" => {
                src.prop("SelectedFunctionName").and_then(|v| v.text()).unwrap_or("").to_string()
            }
            _ => {
                return Err(Status::Unsupported(
                    "timer event comes from a node Seal cannot follow".into(),
                ))
            }
        }
    };
    let ev = customs.get(&fname).ok_or_else(|| {
        Status::Unsupported(format!("timer runs {fname:?}, which is not a custom event here"))
    })?;
    if !ev.params.is_empty() || !ev.returns.is_empty() {
        return Err(Status::Unsupported(format!(
            "timer runs {fname:?}, which takes inputs or returns values"
        )));
    }
    Ok(ev)
}

/// Set or clear a timer. A timer handle is the text name of the event the
/// timer runs, so a handle can be kept in a variable and cleared later.
fn timer_call(
    o: &Object,
    set: bool,
    objects: &[Object],
    customs: &HashMap<String, CustomEvent>,
) -> Result<Mapped, Status> {
    let ev = timer_event(o, objects, customs)?;
    if o.pin("Object").is_some_and(|p| !p.linked.is_empty()) {
        return Err(Status::Unsupported("timer on another object".into()));
    }
    let mut m = Mapped::new(if set { "set_timer" } else { "clear_timer" })
        .s("name", &ev.seal)
        .pin("execute", "in")
        .pin("then", "then");
    if set {
        for delay in ["InitialStartDelay", "InitialStartDelayVariance"] {
            if let Some(p) = o.pin(delay) {
                let zero = p
                    .default
                    .as_deref()
                    .map_or(true, |t| t.trim().parse::<f64>().is_ok_and(|v| v == 0.0));
                if !p.linked.is_empty() || !zero {
                    return Err(Status::Unsupported(format!("{delay} is not zero")));
                }
            }
        }
        m = m.pin("Time", "seconds").pin("bLooping", "looping");
        let h = m.add(
            Node::new(0, "const")
                .with("type", SealValue::String("str".into()))
                .with("value", SealValue::String(ev.seal.clone())),
        );
        m = m.at("ReturnValue", h, "out");
    }
    Ok(m)
}

/// Clear Timer by Handle: clears the timer whose event the handle names,
/// checking each event a timer in this script can run. Clear and
/// Invalidate also empties the handle variable it was read from.
fn clear_by_handle(
    o: &Object,
    invalidate: bool,
    timers: &[String],
    objects: &[Object],
    vars: &[(String, String, Ty)],
) -> Result<Mapped, Status> {
    let mut m = Mapped::new("sequence").pin("execute", "in");
    m.nodes[0].set("count", SealValue::Number("2".into()));
    let handle = m.add(pass(Ty::Str));
    m = m.at("Handle", handle, "a");
    let mut prev: Option<(usize, &str)> = Some((0, "then0"));
    for t in timers {
        let mut cmp = typed("compare", Ty::Str).with("op", SealValue::String("eq".into()));
        put_default(&mut cmp, "b", SealValue::String(t.clone()));
        let c = m.add(cmp);
        m.wire(handle, "out", c, "a");
        let b = m.add(Node::new(0, "branch"));
        m.wire(c, "out", b, "cond");
        if let Some((n, pin)) = prev {
            m.wire(n, pin, b, "in");
        }
        let clear = m.add(named("clear_timer", t));
        m.wire(b, "true", clear, "in");
        prev = Some((b, "false"));
    }
    let mut out = (0, "then1");
    if invalidate {
        let src = o
            .pin("Handle")
            .filter(|p| p.linked.len() == 1)
            .and_then(|p| objects.iter().find(|x| x.name == p.linked[0].0))
            .filter(|x| {
                x.short_class() == "K2Node_VariableGet" && self_context(x, "VariableReference")
            })
            .and_then(|x| member(x, "VariableReference"))
            .and_then(|n| vars.iter().find(|v| v.0 == n).map(|v| v.1.clone()));
        if let Some(var) = src {
            let mut set = named("set", &var);
            put_default(&mut set, "value", SealValue::String(String::new()));
            let s = m.add(set);
            m.wire(0, "then1", s, "in");
            out = (s, "then");
        }
    }
    Ok(m.at("then", out.0, out.1))
}

/// Cast To: a check of what the entity was made from. Seal entities have
/// no separate class objects, so the cast result is the same entity and the
/// check is `is_a` over the prefab base chain.
fn expand_cast(o: &Object) -> Result<Mapped, Status> {
    let target = o
        .prop("TargetType")
        .and_then(|v| v.text())
        .ok_or_else(|| Status::Unsupported("cast without a target class".into()))?;
    // `/Script/Engine.BlueprintGeneratedClass'/Game/...'` names a made class;
    // a path that is itself under /Script/ is a built-in one.
    let inner = target.trim().trim_matches(['"', '\'']);
    let inner = inner.rsplit_once('\'').map_or(inner, |(_, r)| r);
    if inner.trim_start_matches(['"', '\'']).starts_with("/Script/") {
        let name = prefab_name(target).unwrap_or_default();
        return Err(Status::Unsupported(format!("cast to the built-in class {name}")));
    }
    let prefab = prefab_name(target)
        .ok_or_else(|| Status::Unsupported("cast without a target class".into()))?;
    let mut m = Mapped { nodes: vec![pass(Ty::Entity)], ..Mapped::new("select") };
    let mut check = Node::new(0, "is_a");
    check.set("prefab", SealValue::String(prefab));
    let check = m.add(check);
    m.wire(0, "out", check, "entity");
    m = m.pin("Object", "a").at("bSuccess", check, "out");
    if let Some(out) = o.pins.iter().find(|p| p.output && p.name.starts_with("As")) {
        // A failed cast gives the empty entity, as the classic one gives None.
        let mut pick = typed("select", Ty::Entity);
        put_default(&mut pick, "b", SealValue::String("none".into()));
        let pick = m.add(pick);
        m.wire(check, "out", pick, "cond");
        m.wire(0, "out", pick, "a");
        m = m.at(&out.name, pick, "out");
    }
    if o.pin("execute").is_some() {
        let br = m.add(Node::new(0, "branch"));
        m.wire(check, "out", br, "cond");
        m = m.at("execute", br, "in").at("then", br, "true").at("CastFailed", br, "false");
    }
    Ok(m)
}

/// Get All Actors Of Class (or Get Actor Of Class for the first): the
/// entities made from the class's prefab, or one based on it, kept in a
/// hidden variable when the node runs so later spawns and destroys do not
/// change what the graph sees.
fn find_call(o: &Object, state: &str, first: bool) -> Result<Mapped, Status> {
    let class =
        o.pin("ActorClass").ok_or_else(|| Status::Unsupported("find without a class".into()))?;
    if !class.linked.is_empty() {
        return Err(Status::Unsupported("find class driven by a link".into()));
    }
    let raw = class.default_object.as_deref().or(class.default.as_deref()).unwrap_or("");
    if raw.trim_start_matches(['"', '\'']).starts_with("/Script/") {
        let name = prefab_name(raw).unwrap_or_default();
        return Err(Status::Unsupported(format!("find of the built-in class {name}")));
    }
    let prefab =
        prefab_name(raw).ok_or_else(|| Status::Unsupported("find without a class".into()))?;
    let mut m = Mapped::new("set").s("name", state);
    let mut find = Node::new(0, if first { "find_first" } else { "find_all" });
    find.set("prefab", SealValue::String(prefab));
    let find = m.add(find);
    m.wire(find, "out", 0, "value");
    let read = m.add(named("get", state));
    let (start, out) = if first {
        (Value::Entity(seal_graph::rt::NO_ENTITY), "ReturnValue")
    } else {
        (Value::List(Elem::Entity, Vec::new()), "OutActors")
    };
    m.vars.push((state.into(), start));
    Ok(m.pin("execute", "in").pin("then", "then").at(out, read, "out"))
}

/// The prefab a class reference names: `/Game/Things/BP_Coin.BP_Coin_C`
/// gives `BP_Coin`.
fn prefab_name(class_path: &str) -> Option<String> {
    let t = class_path.trim().trim_matches(['"', '\'']);
    let t = t.rsplit_once('\'').map_or(t, |(_, r)| r);
    let last = t.rsplit(['.', '/']).next()?.trim_matches(['"', '\'']);
    let name = last.strip_suffix("_C").unwrap_or(last);
    (!name.is_empty() && name != "None").then(|| name.to_string())
}

/// Spawn Actor from Class: a `spawn` of the prefab named after the class.
/// Seal spawns at the prefab's own transform, so a spawn transform other
/// than the default, an owner, or values set on spawn are not carried yet.
fn expand_spawn(o: &Object) -> Result<Mapped, Status> {
    let class =
        o.pin("Class").ok_or_else(|| Status::Unsupported("spawn without a class".into()))?;
    if !class.linked.is_empty() {
        return Err(Status::Unsupported("spawn class driven by a link".into()));
    }
    let prefab = class
        .default
        .as_deref()
        .and_then(prefab_name)
        .or_else(|| class.default_object.as_deref().and_then(prefab_name))
        .ok_or_else(|| Status::Unsupported("spawn without a class".into()))?;
    for p in &o.pins {
        if p.output || p.category == "exec" || PLUMBING.contains(&p.name.as_str()) {
            continue;
        }
        match p.name.as_str() {
            "Class" | "CollisionHandlingOverride" | "TransformScaleMethod" => {}
            "SpawnTransform" => {}
            other => {
                let set = p
                    .default
                    .as_deref()
                    .is_some_and(|d| !d.trim().is_empty() && d.trim() != "None");
                if !p.linked.is_empty() || set {
                    return Err(Status::Unsupported(format!("{other} set on spawn")));
                }
            }
        }
    }
    let m =
        Mapped::new("spawn").s("prefab", &prefab).pin("execute", "in").pin("ReturnValue", "entity");
    // The spawn transform places the new entity: spawn, then set where it is,
    // and its rotation and scale when a literal transform moves them off rest.
    let at = o.pin("SpawnTransform");
    let linked = at.is_some_and(|p| !p.linked.is_empty());
    let lit = match at.and_then(|p| p.default.as_deref()).filter(|t| !t.trim().is_empty()) {
        Some(t) if !linked => Some(transform_literal(t)?),
        _ => None,
    };
    if !linked && lit.is_none() {
        return Ok(m.pin("then", "then"));
    }
    let mut m = m;
    let set = m.add(location_field("world_set"));
    m.wire(0, "then", set, "in");
    m.wire(0, "entity", set, "entity");
    let mut last = set;
    if let Some([loc, rot, scale]) = lit {
        put_default(&mut m.nodes[set], "value", vec3_value(loc));
        for (field, v, rest) in
            [("rotation", rot, (0.0, 0.0, 0.0)), ("scale", scale, (1.0, 1.0, 1.0))]
        {
            if v == rest {
                continue;
            }
            let mut node = transform_field("world_set", field);
            put_default(&mut node, "value", vec3_value(v));
            let n = m.add(node);
            m.wire(last, "then", n, "in");
            m.wire(0, "entity", n, "entity");
            last = n;
        }
    }
    let m = m.at("then", last, "then");
    Ok(if linked { m.at("SpawnTransform", set, "value") } else { m })
}

/// Length in seconds a timeline gets on import: copied nodes do not carry
/// their curves or length, so tracks start as a 0 to 1 ramp over this.
pub const TIMELINE_LENGTH: f64 = 5.0;

fn set_num(var: &str, v: f64) -> Node {
    let mut n = named("set", var);
    put_default(&mut n, "value", SealValue::Number(float_text(v)));
    n
}

/// Timeline: two hidden variables, `<state>_time` and `<state>_dir`
/// (1 forward, -1 backward, 0 stopped). The control pins set them; a
/// `tick` event moves the time by `frame_dt * dir`, fires Update, then
/// Finished when a non-looping timeline reaches an end. A looping one
/// wraps around and never finishes. Float tracks read `time / length`;
/// vector tracks read that on every axis.
fn expand_timeline(o: &Object, state: &str) -> Result<Mapped, Status> {
    use Arg::*;
    let f = Ty::Float;
    let flag =
        |k: &str| o.prop(k).and_then(|v| v.text()).is_some_and(|t| t.eq_ignore_ascii_case("true"));
    let (looping, auto) = (flag("bLoop"), flag("bAutoPlay"));
    let len = TIMELINE_LENGTH;
    let (tv, dv) = (format!("{state}_time"), format!("{state}_dir"));
    let mut e = Expr::new();
    // Controls. Node 0 names the expansion in the report.
    let play = e.m.add(set_num(&dv, 1.0));
    let from_start = e.m.add(set_num(&tv, 0.0));
    let fs_go = e.m.add(set_num(&dv, 1.0));
    e.m.wire(from_start, "then", fs_go, "in");
    let stop = e.m.add(set_num(&dv, 0.0));
    let reverse = e.m.add(set_num(&dv, -1.0));
    let from_end = e.m.add(set_num(&tv, len));
    let fe_go = e.m.add(set_num(&dv, -1.0));
    e.m.wire(from_end, "then", fe_go, "in");
    let new_lo = e.op("max", f, &[("a", In("NewTime", f)), ("b", F(0.0))]);
    let new_t = e.op("min", f, &[("a", N(new_lo)), ("b", F(len))]);
    let set_new = e.m.add(named("set", &tv));
    e.m.wire(new_t, "out", set_new, "value");
    // Stepping.
    let tick = e.m.add(Node::new(0, "event").with("name", SealValue::String("tick".into())));
    let dir = e.m.add(named("get", &dv));
    let now = e.m.add(named("get", &tv));
    let moving = e.cmp("ne", f, N(dir), F(0.0));
    let br = e.m.add(Node::new(0, "branch"));
    e.m.wire(tick, "then", br, "in");
    e.m.wire(moving, "out", br, "cond");
    let dt = e.m.add(Node::new(0, "frame_dt"));
    let step = e.op("mul", f, &[("a", N(dt)), ("b", N(dir))]);
    let raw = e.op("add", f, &[("a", N(now)), ("b", N(step))]);
    let next = if looping {
        let over = e.cmp("ge", f, N(raw), F(len));
        let back = e.op("sub", f, &[("a", N(raw)), ("b", F(len))]);
        let a = e.sel(f, over, N(back), N(raw));
        let under = e.cmp("lt", f, N(a), F(0.0));
        let fwd = e.op("add", f, &[("a", N(a)), ("b", F(len))]);
        e.sel(f, under, N(fwd), N(a))
    } else {
        let lo = e.op("max", f, &[("a", N(raw)), ("b", F(0.0))]);
        e.op("min", f, &[("a", N(lo)), ("b", F(len))])
    };
    let set_t = e.m.add(named("set", &tv));
    e.m.wire(next, "out", set_t, "value");
    e.m.wire(br, "true", set_t, "in");
    let seq = e.m.add(
        Node::new(0, "sequence")
            .with("count", SealValue::Number(if looping { "1" } else { "2" }.into())),
    );
    e.m.wire(set_t, "then", seq, "in");
    let mut m_finish = None;
    if !looping {
        let d = e.m.add(named("get", &dv));
        let t = e.m.add(named("get", &tv));
        let fwd = e.cmp("gt", f, N(d), F(0.0));
        let at_end = e.cmp("ge", f, N(t), F(len));
        let a = e.op("and", Ty::Bool, &[("a", N(fwd)), ("b", N(at_end))]);
        let bwd = e.cmp("lt", f, N(d), F(0.0));
        let at_start = e.cmp("le", f, N(t), F(0.0));
        let b = e.op("and", Ty::Bool, &[("a", N(bwd)), ("b", N(at_start))]);
        let done = e.op("or", Ty::Bool, &[("a", N(a)), ("b", N(b))]);
        let fb = e.m.add(Node::new(0, "branch"));
        e.m.wire(seq, "then1", fb, "in");
        e.m.wire(done, "out", fb, "cond");
        let halt = e.m.add(set_num(&dv, 0.0));
        e.m.wire(fb, "true", halt, "in");
        m_finish = Some(halt);
    }
    if auto {
        let start = e.m.add(Node::new(0, "event").with("name", SealValue::String("start".into())));
        let go = e.m.add(set_num(&dv, 1.0));
        e.m.wire(start, "then", go, "in");
    }
    // Outputs.
    let cur = e.m.add(named("get", &tv));
    let alpha = e.op("div", f, &[("a", N(cur)), ("b", F(len))]);
    let d = e.m.add(named("get", &dv));
    let back = e.cmp("lt", f, N(d), F(0.0));
    let mut sel = typed("select", Ty::Str);
    put_default(&mut sel, "a", SealValue::String("Backward".into()));
    put_default(&mut sel, "b", SealValue::String("Forward".into()));
    let dir_out = e.m.add(sel);
    e.m.wire(back, "out", dir_out, "cond");
    let mut m = e.m;
    for p in o.pins.iter().filter(|p| p.output) {
        match p.name.as_str() {
            "Update" | "Finished" | "Direction" => {}
            _ if p.category == "exec" => {}
            name => match pin_ty(p) {
                Some(Ty::Float) => {
                    m.pins.insert(name.into(), (alpha, "out".into())).map_or((), |_| ())
                }
                Some(Ty::Vec3) => {
                    let v = m.add(Node::new(0, "vec3"));
                    for axis in ["x", "y", "z"] {
                        m.wire(alpha, "out", v, axis);
                    }
                    m.pins.insert(name.into(), (v, "out".into()));
                }
                _ => {
                    return Err(Status::Unsupported(format!(
                        "timeline track {name} is not a float or vector track"
                    )));
                }
            },
        }
    }
    m.vars.push((tv, Value::Float(0.0)));
    m.vars.push((dv, Value::Float(0.0)));
    m = m
        .at("Play", play, "in")
        .at("PlayFromStart", from_start, "in")
        .at("Stop", stop, "in")
        .at("Reverse", reverse, "in")
        .at("ReverseFromEnd", from_end, "in")
        .at("SetNewTime", set_new, "in")
        .at("Update", seq, "then0")
        .at("Direction", dir_out, "out");
    if let Some(h) = m_finish {
        m = m.at("Finished", h, "then");
    }
    Ok(m)
}

/// Several `name` events (an engine tick plus timelines, say) become one:
/// the first keeps its place and runs the others' chains after its own
/// through a sequence; links from their parameters move to the first.
fn merge_events(graph: &mut Graph, name: &str) {
    let is_it = |n: &Node| {
        n.kind == "event" && matches!(n.param("name"), Some(SealValue::String(s)) if s == name)
    };
    let mut evs: Vec<u32> = graph.nodes.iter().filter(|n| is_it(n)).map(|n| n.id).collect();
    if evs.len() < 2 {
        return;
    }
    evs.sort();
    let first = evs[0];
    let seq = graph.next_id();
    let mut outs = 0;
    for &ev in &evs {
        let mut used = false;
        for l in graph.links.iter_mut().filter(|l| l.from.node == ev) {
            if l.from.pin == "then" {
                l.from = seal_graph::PinRef { node: seq, pin: format!("then{outs}") };
                used = true;
            } else {
                l.from.node = first;
            }
        }
        if used {
            outs += 1;
        }
    }
    graph.nodes.retain(|n| n.id == first || !evs.contains(&n.id));
    if outs > 0 {
        graph
            .nodes
            .push(Node::new(seq, "sequence").with("count", SealValue::Number(outs.to_string())));
        graph.links.push(seal_graph::Link::new(first, "then", seq, "in"));
    }
}

fn expand_switch(o: &Object, class: &str) -> Result<Mapped, Status> {
    let ty = if class == "K2Node_SwitchInteger" { Ty::Int } else { Ty::Str };
    if class == "K2Node_SwitchString"
        && !o
            .prop("bIsCaseSensitive")
            .and_then(|v| v.text())
            .is_some_and(|t| t.eq_ignore_ascii_case("true"))
    {
        return Err(Status::Unsupported("switch on string that ignores case".into()));
    }
    let cases: Vec<&Pin> =
        o.pins.iter().filter(|p| p.output && p.category == "exec" && p.name != "Default").collect();
    if cases.is_empty() {
        let mut m = Mapped::new("sequence").pin("execute", "in").pin("Default", "then0");
        m.nodes[0].set("count", SealValue::Number("1".into()));
        return Ok(m);
    }
    let mut m = Mapped::new("branch");
    let sel = m.add(pass(ty));
    let mut prev: Option<usize> = None;
    for (k, case) in cases.iter().enumerate() {
        let b = if k == 0 { 0 } else { m.add(Node::new(0, "branch")) };
        let mut cmp = typed("compare", ty).with("op", SealValue::String("eq".into()));
        let lit = if ty == Ty::Int {
            let n: i64 =
                case.name.trim().parse().map_err(|_| {
                    Status::Unsupported(format!("case {} is not a number", case.name))
                })?;
            SealValue::Number(n.to_string())
        } else {
            SealValue::String(case.name.clone())
        };
        put_default(&mut cmp, "b", lit);
        let c = m.add(cmp);
        m.wire(sel, "out", c, "a");
        m.wire(c, "out", b, "cond");
        if let Some(p) = prev {
            m.wire(p, "false", b, "in");
        }
        m = m.at(&case.name, b, "true");
        prev = Some(b);
    }
    let last = prev.expect("at least one case");
    Ok(m.pin("execute", "in").at("Selection", sel, "a").at("Default", last, "false"))
}

/// Select: picks `Option k` by Index. With a bool index Option 0 is false
/// and Option 1 true; an int index outside the options gives zero.
fn expand_select(o: &Object) -> Result<Mapped, Status> {
    let index =
        o.pin("Index").ok_or_else(|| Status::Unsupported("select without an index".into()))?;
    let ity = pin_ty(index).filter(|t| matches!(t, Ty::Int | Ty::Bool));
    let ity = ity.ok_or_else(|| Status::Unsupported("select on an index of this type".into()))?;
    let ret = o.pin("ReturnValue").and_then(pin_ty);
    let ty = ret.ok_or_else(|| {
        Status::Unsupported("select of a type Seal graphs do not have yet".into())
    })?;
    let opts: Vec<&Pin> =
        o.pins.iter().filter(|p| !p.output && p.name.starts_with("Option ")).collect();
    if ity == Ty::Bool {
        let f = opts.iter().find(|p| p.name == "Option 0");
        let t = opts.iter().find(|p| p.name == "Option 1");
        let (Some(f), Some(t)) = (f, t) else {
            return Err(Status::Unsupported("bool select without both options".into()));
        };
        return Ok(Mapped::new("select")
            .s("type", ty.name())
            .pin("Index", "cond")
            .pin(&t.name, "a")
            .pin(&f.name, "b")
            .pin("ReturnValue", "out"));
    }
    // select(i == 0, o0, select(i == 1, o1, ... zero))
    let mut m = Mapped { nodes: vec![pass(Ty::Int)], ..Mapped::new("select") };
    let mut out: Option<usize> = None;
    for (k, p) in opts.iter().enumerate().rev() {
        let n: usize = p.name["Option ".len()..].trim().parse().unwrap_or(k);
        let s = m.add(typed("select", ty));
        let mut cmp = typed("compare", Ty::Int).with("op", SealValue::String("eq".into()));
        put_default(&mut cmp, "b", SealValue::Number(n.to_string()));
        let c = m.add(cmp);
        m.wire(0, "out", c, "a");
        m.wire(c, "out", s, "cond");
        if let Some(o) = out {
            m.wire(o, "out", s, "b");
        }
        m = m.at(&p.name, s, "a");
        out = Some(s);
    }
    let out = out.ok_or_else(|| Status::Unsupported("select without options".into()))?;
    Ok(m.pin("Index", "a").at("ReturnValue", out, "out"))
}

/// Expand a standard flow-control macro into plain Seal nodes.
fn expand_macro(name: &str, o: &Object, state: &str) -> Result<Mapped, Status> {
    match name {
        "ForLoop" => {
            let mut m = Mapped::new("for")
                .pin("FirstIndex", "first")
                .pin("LastIndex", "last")
                .pin("LoopBody", "body")
                .pin("Index", "index")
                .pin("Completed", "done");
            for e in EXEC_IN {
                m = m.pin(e, "in");
            }
            Ok(m)
        }
        "DoOnce" => {
            // branch(closed) false -> closed = true -> Completed; Reset -> closed = false.
            let closed = bool_input(o, "Start Closed")?;
            let mut m = Mapped::new("branch");
            let get = m.add(named("get", state));
            let close = m.add(set_flag(state, true));
            let reset = m.add(set_flag(state, false));
            m.wire(get, "out", 0, "cond");
            m.wire(0, "false", close, "in");
            m.vars.push((state.into(), Value::Bool(closed)));
            for e in EXEC_IN {
                m = m.pin(e, "in");
            }
            Ok(m.at("Completed", close, "then").at("Reset", reset, "in"))
        }
        "DoN" => {
            // branch(counter < N): count up, then Exit; Reset zeroes the counter.
            let mut m = Mapped::new("branch");
            let get = m.add(named("get", state));
            let mut lt = typed("compare", Ty::Int).with("op", SealValue::String("lt".into()));
            put_default(&mut lt, "b", SealValue::Number("1".into()));
            let cmp = m.add(lt);
            let cur = m.add(named("get", state));
            let mut inc = typed("add", Ty::Int);
            put_default(&mut inc, "b", SealValue::Number("1".into()));
            let inc = m.add(inc);
            let up = m.add(named("set", state));
            let mut zero = named("set", state);
            put_default(&mut zero, "value", SealValue::Number("0".into()));
            let reset = m.add(zero);
            let now = m.add(named("get", state));
            m.wire(get, "out", cmp, "a");
            m.wire(cmp, "out", 0, "cond");
            m.wire(cur, "out", inc, "a");
            m.wire(inc, "out", up, "value");
            m.wire(0, "true", up, "in");
            m.vars.push((state.into(), Value::Int(0)));
            for e in EXEC_IN {
                m = m.pin(e, "in");
            }
            Ok(m.at("N", cmp, "b")
                .at("Exit", up, "then")
                .at("Reset", reset, "in")
                .at("Counter", now, "out"))
        }
        "FlipFlop" => {
            // `a_next` starts true: the first run goes out of A.
            let mut m = Mapped::new("branch");
            let get = m.add(named("get", state));
            let to_b = m.add(set_flag(state, false));
            let to_a = m.add(set_flag(state, true));
            let after = m.add(named("get", state));
            let is_a = m.add(Node::new(0, "not"));
            m.wire(get, "out", 0, "cond");
            m.wire(0, "true", to_b, "in");
            m.wire(0, "false", to_a, "in");
            m.wire(after, "out", is_a, "a");
            m.vars.push((state.into(), Value::Bool(true)));
            for e in EXEC_IN {
                m = m.pin(e, "in");
            }
            Ok(m.at("A", to_b, "then").at("B", to_a, "then").at("Is A", is_a, "out"))
        }
        "Gate" => {
            let closed = bool_input(o, "Start Closed")?;
            let mut m = Mapped::new("branch");
            let get = m.add(named("get", state));
            let open = m.add(set_flag(state, true));
            let close = m.add(set_flag(state, false));
            let toggle = m.add(named("set", state));
            let get2 = m.add(named("get", state));
            let not = m.add(Node::new(0, "not"));
            m.wire(get, "out", 0, "cond");
            m.wire(get2, "out", not, "a");
            m.wire(not, "out", toggle, "value");
            m.vars.push((state.into(), Value::Bool(!closed)));
            Ok(m.pin("Enter", "in")
                .pin("Exit", "true")
                .at("Open", open, "in")
                .at("Close", close, "in")
                .at("Toggle", toggle, "in"))
        }
        "ForLoopWithBreak" => {
            // broken = false -> for (stop = broken); Break -> broken = true.
            let mut m = Mapped::new("set").s("name", state);
            put_default(&mut m.nodes[0], "value", SealValue::Bool(false));
            let lp = m.add(Node::new(0, "for"));
            let get = m.add(named("get", state));
            let brk = m.add(set_flag(state, true));
            m.wire(0, "then", lp, "in");
            m.wire(get, "out", lp, "stop");
            m.vars.push((state.into(), Value::Bool(false)));
            for e in EXEC_IN {
                m = m.pin(e, "in");
            }
            Ok(m.at("FirstIndex", lp, "first")
                .at("LastIndex", lp, "last")
                .at("LoopBody", lp, "body")
                .at("Index", lp, "index")
                .at("Completed", lp, "done")
                .at("Break", brk, "in"))
        }
        "ForEachLoop" | "ForEachLoopWithBreak" => {
            for_each(o, state, name == "ForEachLoopWithBreak")
        }
        "WhileLoop" => {
            // A loop with no fixed end that stops once Condition is false;
            // the step budget still ends a loop that never does.
            let mut m = Mapped::new("for");
            put_default(&mut m.nodes[0], "last", SealValue::Number(i64::MAX.to_string()));
            let not = m.add(Node::new(0, "not"));
            m.wire(not, "out", 0, "stop");
            for e in EXEC_IN {
                m = m.pin(e, "in");
            }
            Ok(m.at("Condition", not, "a").pin("LoopBody", "body").pin("Completed", "done"))
        }
        "IsValid" => {
            // Branch on whether the input object still exists.
            let mut m = Mapped::new("branch");
            let valid = m.add(Node::new(0, "is_valid"));
            m.wire(valid, "out", 0, "cond");
            for e in EXEC_IN.iter().chain(&["exec"]) {
                m = m.pin(e, "in");
            }
            Ok(m.at("InputObject", valid, "entity")
                .pin("Is Valid", "true")
                .pin("Is Not Valid", "false"))
        }
        _ => Err(Status::Unsupported("macro not in the node library yet".into())),
    }
}

/// The element type of an array pin, or why Seal cannot hold it.
fn list_elem(o: &Object, pin: &str) -> Result<Elem, Status> {
    match o.pin(pin).and_then(pin_ty) {
        Some(Ty::List(e)) => Ok(e),
        _ => Err(Status::Unsupported(format!("{pin} is not an array of a type Seal lists hold"))),
    }
}

fn int_node(kind: &str, pin: &str, v: i64) -> Node {
    let mut n = typed(kind, Ty::Int);
    put_default(&mut n, pin, SealValue::Number(v.to_string()));
    n
}

/// `[0]`, `[1]`, ... into one list.
fn make_array(o: &Object) -> Result<Mapped, Status> {
    let e = list_elem(o, "Array")?;
    let items: Vec<&Pin> = o.pins.iter().filter(|p| !p.output && p.category != "exec").collect();
    if items.len() > 64 {
        return Err(Status::Unsupported("more than 64 items".into()));
    }
    let mut m = Mapped::new("list_make").s("type", e.ty().name()).pin("Array", "out");
    m.nodes[0].set("count", SealValue::Number(items.len().to_string()));
    for (k, p) in items.iter().enumerate() {
        m = m.pin(&p.name, &format!("item{k}"));
    }
    Ok(m)
}

/// Number to Text with its default look (grouped, at most three decimals)
/// is `num_text`; any other sign, digit count or rounding is reported.
fn number_text(o: &Object, m: &str) -> Result<Mapped, Status> {
    let rest: &[(&str, &str)] = &[
        ("bAlwaysSign", "false"),
        ("bUseGrouping", "true"),
        ("MinimumIntegralDigits", "1"),
        ("MaximumIntegralDigits", "324"),
        ("MinimumFractionalDigits", "0"),
        ("MaximumFractionalDigits", "3"),
        ("RoundingMode", "HalfToEven"),
    ];
    for (pin, want) in rest {
        let Some(p) = o.pin(pin) else { continue };
        if !p.linked.is_empty() {
            return Err(Status::Unsupported(format!("{pin} driven by a link")));
        }
        let got = p.default.as_deref().unwrap_or("").trim();
        let same = got.eq_ignore_ascii_case(want)
            || matches!((got.parse::<f64>(), want.parse::<f64>()), (Ok(a), Ok(b)) if a == b);
        if !got.is_empty() && !same {
            return Err(Status::Unsupported(format!("{pin} = {got} (only {want} is mapped)")));
        }
    }
    let ty = if m.contains("Double") || m.contains("Float") { Ty::Float } else { Ty::Int };
    Ok(one(
        "num_text",
        Some(ty),
        &[("Value", "a"), ("InFloat", "a"), ("InDouble", "a"), ("ReturnValue", "out")],
    ))
}

/// Make Struct and Break Struct for the structs Seal has as values: 2D and
/// 3D vectors, and rotations as (pitch, yaw, roll).
fn struct_node(o: &Object, make: bool) -> Result<Mapped, Status> {
    let st = o
        .prop("StructType")
        .and_then(|v| v.text())
        .unwrap_or("")
        .trim_end_matches(['\'', '"'])
        .to_string();
    let short = st.rsplit(['.', '/']).next().unwrap_or("").to_string();
    let (ty, fields): (&str, &[&str]) = match short.as_str() {
        "Vector" => ("vec3", &["X", "Y", "Z"]),
        "Rotator" => ("vec3", &["Pitch", "Yaw", "Roll"]),
        "Vector2D" => ("vec2", &["X", "Y"]),
        _ => return Err(Status::Unsupported(format!("struct {short} has no Seal value type"))),
    };
    let whole = o
        .pins
        .iter()
        .find(|p| p.output == make && p.category == "struct")
        .ok_or_else(|| Status::Unsupported("struct node without its struct pin".into()))?;
    let seal = ["x", "y", "z"];
    if make {
        let mut m = Mapped::new(ty).pin(&whole.name, "out");
        for (f, s) in fields.iter().zip(seal) {
            m = m.pin(f, s);
        }
        return Ok(m);
    }
    let mut m = Mapped::new("select").s("type", ty);
    put_default(&mut m.nodes[0], "cond", SealValue::Bool(true));
    m = m.pin(&whole.name, "a");
    for (f, s) in fields.iter().zip(seal) {
        let n = m.add(Node::new(0, &format!("{ty}_{s}")));
        m.wire(0, "out", n, "v");
        m = m.at(f, n, "out");
    }
    Ok(m)
}

enum Piece {
    Lit(String),
    Arg(String),
}

/// Split a format template into literal text and `{Name}` arguments. A
/// backtick escapes the next character; a `{` with no closing `}` is text.
fn format_pieces(t: &str) -> Result<Vec<Piece>, Status> {
    let mut out = Vec::new();
    let mut lit = String::new();
    let mut chars = t.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '`' => lit.extend(chars.next()),
            '{' => {
                let rest: String = chars.clone().collect();
                match rest.find('}') {
                    Some(end) if !rest[..end].contains('{') => {
                        let name = rest[..end].trim().to_string();
                        for _ in 0..=rest[..end].chars().count() {
                            chars.next();
                        }
                        if chars.peek() == Some(&'|') {
                            return Err(Status::Unsupported(format!(
                                "argument modifier after {{{name}}}"
                            )));
                        }
                        if !lit.is_empty() {
                            out.push(Piece::Lit(std::mem::take(&mut lit)));
                        }
                        out.push(Piece::Arg(name));
                    }
                    _ => lit.push('{'),
                }
            }
            c => lit.push(c),
        }
    }
    if !lit.is_empty() {
        out.push(Piece::Lit(lit));
    }
    Ok(out)
}

/// Format Text: the template's literal pieces and each argument as text,
/// joined left to right. Numbers show grouped with at most three decimals
/// (`num_text`); a template argument with no pin stays as written.
fn format_text(o: &Object) -> Result<Mapped, Status> {
    let fmt = o.pin("Format").ok_or_else(|| Status::Unsupported("no Format pin".into()))?;
    if !fmt.linked.is_empty() {
        return Err(Status::Unsupported("format template driven by a link".into()));
    }
    let pieces = format_pieces(fmt.default.as_deref().unwrap_or(""))?;
    let mut m = Mapped::new("concat").pin("Result", "out");
    put_default(&mut m.nodes[0], "b", SealValue::String(String::new()));
    let mut args: HashMap<String, usize> = HashMap::new();
    let mut parts = Vec::new();
    for piece in pieces {
        let text = match piece {
            Piece::Lit(t) => t,
            Piece::Arg(name) => {
                let Some(p) = o.pins.iter().find(|p| {
                    !p.output && p.category != "exec" && p.name == name && name != "Format"
                }) else {
                    parts.push(
                        m.add(
                            typed("const", Ty::Str)
                                .with("value", SealValue::String(format!("{{{name}}}"))),
                        ),
                    );
                    continue;
                };
                if let Some(&n) = args.get(&name) {
                    parts.push(n);
                    continue;
                }
                let node = match pin_ty(p) {
                    Some(t @ (Ty::Int | Ty::Float)) => typed("num_text", t),
                    Some(Ty::Bool) => typed("to_str", Ty::Bool),
                    Some(Ty::Str) => pass(Ty::Str),
                    None if p.linked.is_empty() => {
                        // An unset argument with no type formats as nothing.
                        continue;
                    }
                    _ => {
                        return Err(Status::Unsupported(format!(
                            "argument {name} of type {}",
                            p.category
                        )))
                    }
                };
                let n = m.add(node);
                m = m.at(&p.name, n, "a");
                args.insert(name, n);
                parts.push(n);
                continue;
            }
        };
        parts.push(m.add(typed("const", Ty::Str).with("value", SealValue::String(text))));
    }
    // Fold left: ((p0 + p1) + p2) ... with the last join being node 0.
    match parts.len() {
        0 => {}
        1 => m.wire(parts[0], "out", 0, "a"),
        n => {
            let mut acc = parts[0];
            for &p in &parts[1..n - 1] {
                let j = m.add(Node::new(0, "concat"));
                m.wire(acc, "out", j, "a");
                m.wire(p, "out", j, "b");
                acc = j;
            }
            m.wire(acc, "out", 0, "a");
            m.wire(parts[n - 1], "out", 0, "b");
        }
    }
    Ok(m)
}

/// The array variable a changing array function works on: classic arrays
/// change in place, Seal lists are values, so the change is written back.
fn array_var(
    o: &Object,
    objects: &[Object],
    vars: &[(String, String, Ty)],
) -> Result<String, Status> {
    let p = o.pin("TargetArray").filter(|p| p.linked.len() == 1);
    let src = p.and_then(|p| objects.iter().find(|x| x.name == p.linked[0].0));
    let src = src.filter(|x| {
        x.short_class() == "K2Node_VariableGet" && self_context(x, "VariableReference")
    });
    let name = src.and_then(|x| member(x, "VariableReference"));
    name.and_then(|n| vars.iter().find(|v| v.0 == n).map(|v| v.1.clone())).ok_or_else(|| {
        Status::Unsupported("changes an array that is not read straight from a variable".into())
    })
}

fn array_call(
    o: &Object,
    m: &str,
    objects: &[Object],
    vars: &[(String, String, Ty)],
) -> Result<Mapped, Status> {
    let e = list_elem(o, "TargetArray")?;
    let t = Some(e.ty());
    let used = |pin: &str| o.pin(pin).is_some_and(|p| !p.linked.is_empty());
    let pure =
        |kind: &str, pins: &[(&str, &str)]| Ok(one(kind, t, pins).pin("TargetArray", "list"));
    match m {
        "Array_Length" => pure("list_len", &[("ReturnValue", "out")]),
        "Array_Get" => pure("list_get", &[("Index", "index"), ("Item", "out")]),
        "Array_Find" => pure("list_find", &[("ItemToFind", "item"), ("ReturnValue", "out")]),
        "Array_Contains" => {
            pure("list_contains", &[("ItemToFind", "item"), ("ReturnValue", "out")])
        }
        "Array_LastIndex" | "Array_IsEmpty" | "Array_IsNotEmpty" => {
            let mut m2 = one("list_len", t, &[("TargetArray", "list")]);
            let n = match m {
                "Array_LastIndex" => int_node("sub", "b", 1),
                "Array_IsEmpty" => {
                    int_node("compare", "b", 0).with("op", SealValue::String("eq".into()))
                }
                _ => int_node("compare", "b", 0).with("op", SealValue::String("ne".into())),
            };
            let n = m2.add(n);
            m2.wire(0, "out", n, "a");
            Ok(m2.at("ReturnValue", n, "out"))
        }
        "Array_IsValidIndex" => {
            // 0 <= index < length
            let mut m2 = Mapped { nodes: vec![pass(Ty::Int)], ..Mapped::new("select") };
            let len = m2.add(typed("list_len", e.ty()));
            let lo = m2.add(int_node("compare", "b", 0).with("op", SealValue::String("ge".into())));
            let hi = m2.add(typed("compare", Ty::Int).with("op", SealValue::String("lt".into())));
            let and = m2.add(Node::new(0, "and"));
            m2.wire(0, "out", lo, "a");
            m2.wire(0, "out", hi, "a");
            m2.wire(len, "out", hi, "b");
            m2.wire(lo, "out", and, "a");
            m2.wire(hi, "out", and, "b");
            Ok(m2.pin("IndexToTest", "a").at("TargetArray", len, "list").at(
                "ReturnValue",
                and,
                "out",
            ))
        }
        "Array_Add" | "Array_Insert" | "Array_Set" | "Array_Remove" | "Array_RemoveItem"
        | "Array_Clear" => {
            let var = array_var(o, objects, vars)?;
            if m == "Array_Set" && bool_input(o, "bSizeToFit")? {
                return Err(Status::Unsupported("bSizeToFit is not mapped".into()));
            }
            if m == "Array_RemoveItem" && used("ReturnValue") {
                return Err(Status::Unsupported(
                    "the removed-anything result is not mapped yet".into(),
                ));
            }
            let mut w = Mapped::new("set").s("name", &var).pin("execute", "in").pin("then", "then");
            if m == "Array_Clear" {
                // The old items are not needed; the input link ends in a pass-through.
                put_default(&mut w.nodes[0], "value", SealValue::Array(Vec::new()));
                let sink = w.add(pass(Ty::List(e)));
                return Ok(w.at("TargetArray", sink, "a"));
            }
            let (kind, pins): (&str, &[(&str, &str)]) = match m {
                "Array_Add" => ("list_push", &[("NewItem", "item")]),
                "Array_Insert" => ("list_insert", &[("NewItem", "item"), ("Index", "index")]),
                "Array_Set" => ("list_set", &[("Index", "index"), ("Item", "item")]),
                "Array_Remove" => ("list_remove_at", &[("IndexToRemove", "index")]),
                _ => ("list_remove_item", &[("Item", "item")]),
            };
            let op = w.add(typed(kind, e.ty()));
            w.wire(op, "out", 0, "value");
            w = w.at("TargetArray", op, "list");
            for (a, b) in pins {
                w = w.at(a, op, b);
            }
            if m == "Array_Add" {
                // The new item's index: the new length minus one.
                let len = w.add(typed("list_len", e.ty()));
                let sub = w.add(int_node("sub", "b", 1));
                w.wire(0, "value", len, "list");
                w.wire(len, "out", sub, "a");
                w = w.at("ReturnValue", sub, "out");
            }
            Ok(w)
        }
        _ => Err(Status::Unsupported("array function not in the node library yet".into())),
    }
}

/// ForEachLoop: like the classic macro, the length is read again before
/// every pass, so a body that changes the array changes the loop.
fn for_each(o: &Object, state: &str, with_break: bool) -> Result<Mapped, Status> {
    let e = list_elem(o, "Array")?;
    // next = 0 -> for (stop = next >= len); body: next = index + 1 -> LoopBody
    let mut m = Mapped::new("set").s("name", state);
    put_default(&mut m.nodes[0], "value", SealValue::Number("0".into()));
    let lp = m.add(int_node("for", "last", i64::MAX));
    put_default(&mut m.nodes[lp], "first", SealValue::Number("0".into()));
    let arr = m.add(pass(Ty::List(e)));
    let len = m.add(typed("list_len", e.ty()));
    let next = m.add(named("get", state));
    let done = m.add(typed("compare", Ty::Int).with("op", SealValue::String("ge".into())));
    let inc = m.add(int_node("add", "b", 1));
    let step = m.add(named("set", state));
    let item = m.add(typed("list_get", e.ty()));
    m.wire(0, "then", lp, "in");
    m.wire(arr, "out", len, "list");
    m.wire(arr, "out", item, "list");
    m.wire(next, "out", done, "a");
    m.wire(len, "out", done, "b");
    m.wire(lp, "index", inc, "a");
    m.wire(inc, "out", step, "value");
    m.wire(lp, "body", step, "in");
    m.wire(lp, "index", item, "index");
    m.vars.push((state.into(), Value::Int(0)));
    if with_break {
        let broken = format!("{state}_broken");
        let reset = m.add(set_flag(&broken, false));
        let get = m.add(named("get", &broken));
        let or = m.add(Node::new(0, "or"));
        let brk = m.add(set_flag(&broken, true));
        // The flag is cleared first, then the counter, then the loop starts.
        m.wire(reset, "then", 0, "in");
        m.wire(done, "out", or, "a");
        m.wire(get, "out", or, "b");
        m.wire(or, "out", lp, "stop");
        m.vars.push((broken, Value::Bool(false)));
        for x in EXEC_IN {
            m = m.at(x, reset, "in");
        }
        m = m.at("Break", brk, "in");
    } else {
        m.wire(done, "out", lp, "stop");
        for x in EXEC_IN {
            m = m.pin(x, "in");
        }
    }
    Ok(m.at("Array", arr, "a")
        .at("LoopBody", step, "then")
        .at("Array Element", item, "out")
        .at("Array Index", lp, "index")
        .at("Completed", lp, "done"))
}

/// Input pins of a commutative operator with extra inputs: A, B, C, ...
fn operand_pins(o: &Object) -> Vec<String> {
    o.pins
        .iter()
        .filter(|p| !p.output && p.category != "exec" && !PLUMBING.contains(&p.name.as_str()))
        .map(|p| p.name.clone())
        .collect()
}

fn member(o: &Object, prop: &str) -> Option<String> {
    o.prop(prop)?.get("MemberName")?.text().map(str::to_string)
}

fn self_context(o: &Object, prop: &str) -> bool {
    o.prop(prop)
        .map(|r| {
            r.get("bSelfContext")
                .and_then(|v| v.text())
                .is_some_and(|t| t.eq_ignore_ascii_case("true"))
                || r.get("MemberParent").is_none()
        })
        .unwrap_or(false)
}

/// A custom event or a function of this script: both become Seal events.
struct CustomEvent {
    seal: String,
    params: Vec<(String, String, Ty)>,
    /// Values a function hands back, from the inputs of its result node.
    returns: Vec<(String, String, Ty)>,
    /// A function whose results Seal cannot carry (an array, a struct, ...).
    unsupported_returns: bool,
}

/// A key or input action event becomes one Seal event per phase, named
/// `key_<Key>_pressed`, `action_<Name>_released` and so on. The game loop
/// fires them; each name can only be imported once.
fn input_events(o: &Object, seen: &mut Vec<String>) -> Result<(String, Mapped), Status> {
    let clean = |v: &str| {
        let v = v.trim_matches(|c| c == '"' || c == '\'');
        let v = v.rsplit(['.', '/']).next().unwrap_or(v);
        v.trim_matches(|c| c == '"' || c == '\'').to_string()
    };
    let text = |k: &str| o.prop(k).and_then(|v| v.text()).map(clean).filter(|v| !v.is_empty());
    let (base, phases): (String, &[(&str, &str)]) = match o.short_class() {
        "K2Node_InputKey" => match text("InputKey") {
            Some(k) => {
                (format!("key_{}", ident(&k)), &[("Pressed", "pressed"), ("Released", "released")])
            }
            None => return Err(Status::Unsupported("key event without a key".into())),
        },
        "K2Node_InputAction" => match text("InputActionName") {
            Some(k) => (
                format!("action_{}", ident(&k)),
                &[("Pressed", "pressed"), ("Released", "released")],
            ),
            None => return Err(Status::Unsupported("input action event without a name".into())),
        },
        _ => match text("InputAction") {
            Some(k) => (
                format!("action_{}", ident(&k)),
                &[
                    ("Started", "pressed"),
                    ("Completed", "released"),
                    ("Triggered", "triggered"),
                    ("Canceled", "canceled"),
                    ("Ongoing", "ongoing"),
                ],
            ),
            None => return Err(Status::Unsupported("input action event without an action".into())),
        },
    };
    if o.pins
        .iter()
        .any(|p| p.output && !p.linked.is_empty() && !p.category.eq_ignore_ascii_case("exec"))
    {
        return Err(Status::Unsupported("reading which key was pressed".into()));
    }
    if seen.contains(&base) {
        return Err(Status::Unsupported("a second event for the same input".into()));
    }
    seen.push(base.clone());
    let mut m = Mapped { nodes: Vec::new(), ..Mapped::new("event") };
    for (pin, phase) in phases {
        let k =
            m.add(Node::new(0, "event").with("name", SealValue::String(format!("{base}_{phase}"))));
        m = m.at(pin, k, "then");
    }
    Ok((base, m))
}

/// The name a custom event or function entry declares.
fn declared_name(o: &Object) -> Option<String> {
    match o.short_class() {
        "K2Node_CustomEvent" => o.prop("CustomFunctionName").and_then(|v| v.text()).map(Into::into),
        "K2Node_FunctionEntry" => member(o, "FunctionReference").or_else(|| {
            o.prop("CustomGeneratedFunctionName").and_then(|v| v.text()).map(Into::into)
        }),
        _ => None,
    }
}

/// Read a copied selection and build the Seal graph.
pub fn import(text: &str, opts: &Options) -> Result<Import, ParseError> {
    import_all(&[text], opts)
}

/// Import several copied selections into one Seal graph, for example the
/// event graph and each function graph of one script. Node names only need
/// to be unique within each selection.
pub fn import_all(texts: &[&str], opts: &Options) -> Result<Import, ParseError> {
    let mut objects = Vec::new();
    let mut scope = Vec::new();
    for (k, text) in texts.iter().enumerate() {
        let mut parsed = export::parse(text).map_err(|mut e| {
            if texts.len() > 1 {
                e.message = format!("selection {}: {}", k + 1, e.message);
            }
            e
        })?;
        if texts.len() > 1 {
            for o in &mut parsed {
                o.name = format!("{}/{}", k + 1, o.name);
                for p in &mut o.pins {
                    for l in &mut p.linked {
                        l.0 = format!("{}/{}", k + 1, l.0);
                    }
                }
            }
        }
        scope.extend(std::iter::repeat_n(k, parsed.len()));
        objects.extend(parsed);
    }
    let mut report = Report::default();

    // Custom events and functions first, so calls to them can be mapped.
    let mut customs: HashMap<String, CustomEvent> = HashMap::new();
    let mut taken: Vec<String> = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        let Some(name) = declared_name(o) else { continue };
        if customs.contains_key(&name) {
            continue;
        }
        let mut seal = ident(&name);
        if matches!(seal.as_str(), "start" | "tick") || taken.contains(&seal) {
            seal.push_str("_event");
        }
        taken.push(seal.clone());
        let params = o
            .pins
            .iter()
            .filter(|p| p.output && p.category != "exec" && !PLUMBING.contains(&p.name.as_str()))
            .filter_map(|p| {
                let mut n = ident(&p.name);
                if n == "then" {
                    n.push('_');
                }
                Some((p.name.clone(), n, pin_ty(p)?))
            })
            .collect();
        let mut returns = Vec::new();
        let mut unsupported_returns = false;
        if o.short_class() == "K2Node_FunctionEntry" {
            let result = objects
                .iter()
                .zip(&scope)
                .find(|(r, s)| **s == scope[i] && r.short_class() == "K2Node_FunctionResult");
            if let Some((r, _)) = result {
                for p in r.pins.iter().filter(|p| {
                    !p.output
                        && p.category != "exec"
                        && !p.hidden
                        && !PLUMBING.contains(&p.name.as_str())
                }) {
                    match pin_ty(p) {
                        Some(t) => returns.push((p.name.clone(), ident(&p.name), t)),
                        None => unsupported_returns = true,
                    }
                }
            }
        }
        customs.insert(name, CustomEvent { seal, params, returns, unsupported_returns });
    }

    // Variables, typed from the pins of their get and set nodes.
    let mut vars: Vec<(String, String, Ty)> = Vec::new();
    for o in &objects {
        if !matches!(o.short_class(), "K2Node_VariableGet" | "K2Node_VariableSet")
            || !self_context(o, "VariableReference")
        {
            continue;
        }
        let Some(name) = member(o, "VariableReference") else { continue };
        let Some(ty) = o.pin(&name).and_then(pin_ty) else { continue };
        match vars.iter().find(|v| v.0 == name) {
            Some(v) if v.2 != ty => {
                report.notes.push(format!("variable {name} is used as both {} and {ty}", v.2))
            }
            Some(_) => {}
            None => vars.push((name.clone(), ident(&name), ty)),
        }
    }

    let mut graph = Graph::new(&opts.name);
    for (orig, seal, ty) in &vars {
        let default = match opts.var_defaults.iter().find(|(n, _)| n == orig) {
            Some((_, v)) if v.ty() == *ty => v.clone(),
            Some(_) => {
                report.notes.push(format!("default for {orig} is not a {ty}; it starts at zero"));
                ty.zero()
            }
            None => ty.zero(),
        };
        graph = graph.var(seal, default);
    }
    let mut var_names: Vec<String> = vars.iter().map(|v| v.1.clone()).collect();

    // Every event a timer here can run, for clearing timers by handle.
    let mut timers: Vec<String> = Vec::new();
    for o in &objects {
        if o.short_class() == "K2Node_CallFunction"
            && matches!(
                member(o, "FunctionReference").as_deref(),
                Some("K2_SetTimer" | "K2_SetTimerDelegate")
            )
        {
            if let Ok(ev) = timer_event(o, &objects, &customs) {
                if !timers.contains(&ev.seal) {
                    timers.push(ev.seal.clone());
                }
            }
        }
    }
    let mut has_start = false;
    let mut has_tick = false;
    let mut touch_events: Vec<String> = Vec::new();
    let mut input_names: Vec<String> = Vec::new();
    // export index -> export pin -> (Seal node id, Seal pin), for linking.
    let mut maps: Vec<Option<HashMap<String, (u32, String)>>> = vec![None; objects.len()];
    let mut ids: Vec<Option<u32>> = vec![None; objects.len()];
    let mut next_id = 1u32;

    for (i, o) in objects.iter().enumerate() {
        let class = o.short_class().to_string();
        let mut what: Option<String> = None;
        let result: Result<Mapped, Status> = match class.as_str() {
            "K2Node_Knot" => Err(Status::Skipped("reroute".into())),
            "K2Node_Self" => Ok(Mapped::new("self").pin("self", "out")),
            "EdGraphNode_Comment" => Err(Status::Skipped("comment".into())),
            "K2Node_CreateDelegate" => {
                Err(Status::Skipped("event reference, read by the timer it feeds".into()))
            }
            "K2Node_FunctionResult" => {
                what = objects
                    .iter()
                    .zip(&scope)
                    .find(|(e, s)| **s == scope[i] && e.short_class() == "K2Node_FunctionEntry")
                    .and_then(|(e, _)| declared_name(e));
                match what.as_deref().and_then(|n| customs.get(n)) {
                    Some(f) if f.unsupported_returns => Err(Status::Unsupported(
                        "returns a value of a type Seal graphs do not have yet".into(),
                    )),
                    // A return also leaves any loop the function is in.
                    Some(f) => {
                        let mut m = Mapped::new("return").s("name", &f.seal).pin("execute", "in");
                        for (orig, n, _) in &f.returns {
                            m = m.pin(orig, n);
                        }
                        Ok(m)
                    }
                    None => Err(Status::Skipped("end of function".into())),
                }
            }
            "K2Node_Event" => {
                let m = member(o, "EventReference").unwrap_or_default();
                what = Some(m.clone());
                match m.as_str() {
                    "ReceiveBeginPlay" if !has_start => {
                        has_start = true;
                        Ok(Mapped::new("event").s("name", "start").pin("then", "then"))
                    }
                    "ReceiveTick" if !has_tick => {
                        has_tick = true;
                        Ok(Mapped::new("event")
                            .s("name", "tick")
                            .pin("then", "then")
                            .pin("DeltaSeconds", "dt"))
                    }
                    "ReceiveBeginPlay" | "ReceiveTick" => {
                        Err(Status::Unsupported("a second copy of this event".into()))
                    }
                    "ReceiveActorBeginOverlap"
                    | "ReceiveActorEndOverlap"
                    | "ReceiveHit"
                    | "ReceiveDestroyed" => touch_event(o, &m, &mut touch_events),
                    _ => Err(Status::Unsupported(
                        "this engine event has no Seal equivalent yet".into(),
                    )),
                }
            }
            "K2Node_InputKey" | "K2Node_InputAction" | "K2Node_EnhancedInputAction" => {
                input_events(o, &mut input_names).map(|(name, m)| {
                    what = Some(name);
                    m
                })
            }
            "K2Node_CustomEvent" | "K2Node_FunctionEntry" => {
                let name = declared_name(o).unwrap_or_default();
                what = Some(name.clone());
                match customs.get(&name) {
                    Some(ev) => {
                        let mut m = Mapped::new("event").s("name", &ev.seal).pin("then", "then");
                        let params = ev
                            .params
                            .iter()
                            .map(|(_, n, t)| {
                                SealValue::Map(SealMap::new(vec![
                                    ("name".into(), SealValue::String(n.clone())),
                                    ("type".into(), SealValue::String(t.name().into())),
                                ]))
                            })
                            .collect();
                        m.nodes[0].set("params", SealValue::Array(params));
                        if !ev.returns.is_empty() {
                            let rets = ev
                                .returns
                                .iter()
                                .map(|(_, n, t)| {
                                    SealValue::Map(SealMap::new(vec![
                                        ("name".into(), SealValue::String(n.clone())),
                                        ("type".into(), SealValue::String(t.name().into())),
                                    ]))
                                })
                                .collect();
                            m.nodes[0].set("returns", SealValue::Array(rets));
                        }
                        for (orig, n, _) in &ev.params {
                            m = m.pin(orig, n);
                        }
                        Ok(m)
                    }
                    None => Err(Status::Unsupported("custom event without a name".into())),
                }
            }
            "K2Node_IfThenElse" => Ok(Mapped::new("branch")
                .pin("execute", "in")
                .pin("Condition", "cond")
                .pin("then", "true")
                .pin("else", "false")),
            "K2Node_ExecutionSequence" => {
                let outs: Vec<&Pin> =
                    o.pins.iter().filter(|p| p.output && p.category == "exec").collect();
                let mut m = Mapped::new("sequence").pin("execute", "in");
                m.nodes[0].set("count", SealValue::Number(outs.len().to_string()));
                for (k, p) in outs.iter().enumerate() {
                    m = m.pin(&p.name, &format!("then{k}"));
                }
                Ok(m)
            }
            "K2Node_SwitchInteger" | "K2Node_SwitchString" | "K2Node_SwitchEnum" => {
                expand_switch(o, &class)
            }
            "K2Node_EnumEquality" | "K2Node_EnumInequality" => {
                let op = if class == "K2Node_EnumEquality" { "eq" } else { "ne" };
                Ok(Mapped::new("compare")
                    .s("type", "str")
                    .s("op", op)
                    .pin("A", "a")
                    .pin("B", "b")
                    .pin("ReturnValue", "out"))
            }
            "K2Node_DynamicCast" => expand_cast(o),
            "K2Node_Select" => expand_select(o),
            "K2Node_MakeArray" => make_array(o),
            "K2Node_FormatText" => format_text(o),
            "K2Node_MakeStruct" | "K2Node_BreakStruct" => {
                struct_node(o, class == "K2Node_MakeStruct")
            }
            "K2Node_GetArrayItem" => list_elem(o, "Array").map(|e| {
                one(
                    "list_get",
                    Some(e.ty()),
                    &[("Array", "list"), ("Dimension 1", "index"), ("Output", "out")],
                )
            }),
            "K2Node_SpawnActorFromClass" => {
                let r = expand_spawn(o);
                if let Ok(m) = &r {
                    if let Some(seal_sealtext::SealValue::String(p)) = m.nodes[0].param("prefab") {
                        what = Some(p.clone());
                    }
                }
                r
            }
            "K2Node_VariableGet" | "K2Node_VariableSet" => {
                let name = member(o, "VariableReference").unwrap_or_default();
                what = Some(name.clone());
                match vars.iter().find(|v| v.0 == name) {
                    _ if class == "K2Node_VariableGet" && feeds_only_set_text(o, &objects) => {
                        Err(Status::Skipped("text element, read by the Set Text it feeds".into()))
                    }
                    Some((_, seal, _)) if self_context(o, "VariableReference") => {
                        if class == "K2Node_VariableGet" {
                            Ok(Mapped::new("get").s("name", seal).pin(&name, "out"))
                        } else {
                            Ok(Mapped::new("set")
                                .s("name", seal)
                                .pin("execute", "in")
                                .pin("then", "then")
                                .pin(&name, "value")
                                .pin("Output_Get", "value"))
                        }
                    }
                    Some(_) => Err(Status::Unsupported("variable on another object".into())),
                    None => Err(Status::Unsupported(
                        "variable of a type Seal graphs do not have yet".into(),
                    )),
                }
            }
            "K2Node_CreateWidget" => create_widget(o),
            "K2Node_InputAxisEvent" => {
                // Every step, with how far the axis is pushed.
                let axis = o.prop("InputAxisName").and_then(|v| v.text()).unwrap_or("").to_string();
                what = Some(axis.clone());
                let mut ev = Mapped::new("event").s("name", "tick").pin("then", "then");
                let a = ev.add(Node::new(0, "input_axis"));
                put_default(&mut ev.nodes[a], "name", SealValue::String(axis));
                Ok(ev.at("AxisValue", a, "out"))
            }
            "K2Node_Timeline" => {
                let tl =
                    o.prop("TimelineName").and_then(|v| v.text()).unwrap_or("Timeline").to_string();
                what = Some(tl.clone());
                let mut state = ident(&tl);
                while var_names
                    .iter()
                    .any(|v| *v == format!("{state}_time") || *v == format!("{state}_dir"))
                {
                    state.push('_');
                }
                report.notes.push(format!(
                    "{}: copied timelines carry no curves; tracks of {tl} ramp from 0 to 1 over {} seconds",
                    o.name, TIMELINE_LENGTH
                ));
                expand_timeline(o, &state)
            }
            "K2Node_MacroInstance" => {
                let path = o
                    .prop("MacroGraphReference")
                    .and_then(|r| r.get("MacroGraph"))
                    .and_then(|v| v.text())
                    .map(ref_path)
                    .unwrap_or("");
                let macro_name = path.rsplit(':').next().unwrap_or(path).to_string();
                what = Some(macro_name.clone());
                let mut state = format!(
                    "{}_{}",
                    ident(&macro_name),
                    ident(o.name.rsplit('/').next().unwrap_or(&o.name))
                );
                while var_names.contains(&state) {
                    state.push('_');
                }
                expand_macro(&macro_name, o, &state)
            }
            "K2Node_CallFunction" | "K2Node_CommutativeAssociativeBinaryOperator" => {
                let m = member(o, "FunctionReference").unwrap_or_default();
                what = Some(m.clone());
                let extra = o
                    .prop("NumAdditionalInputs")
                    .and_then(|v| v.text())
                    .and_then(|t| t.parse::<u32>().ok())
                    .unwrap_or(0);
                if let (true, Some(ev)) = (self_context(o, "FunctionReference"), customs.get(&m)) {
                    if ev.unsupported_returns {
                        Err(Status::Unsupported(
                            "calls a function whose result Seal cannot carry".into(),
                        ))
                    } else {
                        let mut mapped = Mapped::new("call")
                            .s("name", &ev.seal)
                            .pin("execute", "in")
                            .pin("then", "then");
                        for (orig, n, _) in ev.params.iter().chain(&ev.returns) {
                            mapped = mapped.pin(orig, n);
                        }
                        Ok(mapped)
                    }
                } else if matches!(
                    m.as_str(),
                    "AttachActorToActor"
                        | "K2_AttachToActor"
                        | "DetachFromActor"
                        | "K2_DetachFromActor"
                        | "GetAttachParentActor"
                ) {
                    attach_call(o, &m)
                } else if matches!(
                    m.as_str(),
                    "K2_AttachToComponent" | "AttachToComponent" | "K2_AttachRootComponentTo"
                ) {
                    Err(Status::Unsupported(
                        "attaching to a component; attach to its object with Attach Actor To Actor"
                            .into(),
                    ))
                } else if matches!(
                    m.as_str(),
                    "AddToViewport"
                        | "AddToPlayerScreen"
                        | "RemoveFromParent"
                        | "RemoveFromViewport"
                ) || (m == "SetText" && o.pin("InText").is_some())
                {
                    widget_call(o, &m, &objects, &opts.name)
                } else if matches!(
                    m.as_str(),
                    "PlaySound2D" | "PlaySoundAtLocation" | "SpawnSound2D" | "SpawnSoundAtLocation"
                ) {
                    sound_call(o, &m)
                } else if m == "AddMovementInput" {
                    Ok(movement_input(o))
                } else if m == "Jump" {
                    Ok(jump(o))
                } else if m == "StopJumping" {
                    {
                        let mut m =
                            Mapped::new("sequence").pin("execute", "in").pin("then", "then0");
                        m.nodes[0].set("count", SealValue::Number("1".into()));
                        Ok(m)
                    }
                } else if m == "GetInputAxisValue" {
                    Ok(Mapped::new("input_axis")
                        .pin("InputAxisName", "name")
                        .pin("ReturnValue", "out"))
                } else if m == "K2_DestroyActor" {
                    // Destroy the target, which is this object unless linked.
                    let mut d = Mapped::new("despawn").pin("execute", "in").pin("then", "then");
                    if o.pin("self").is_some_and(|p| !p.linked.is_empty()) {
                        d = d.pin("self", "entity");
                    } else {
                        let me = d.add(Node::new(0, "self"));
                        d.wire(me, "out", 0, "entity");
                    }
                    Ok(d)
                } else if matches!(
                    m.as_str(),
                    "K2_GetActorLocation" | "K2_SetActorLocation" | "K2_AddActorWorldOffset"
                ) {
                    location_call(o, &m)
                } else if matches!(
                    m.as_str(),
                    "K2_GetActorRotation"
                        | "K2_SetActorRotation"
                        | "GetActorScale3D"
                        | "SetActorScale3D"
                ) {
                    rotation_scale_call(o, &m)
                } else if m.starts_with("Array_") {
                    array_call(o, &m, &objects, &vars)
                } else if matches!(m.as_str(), "Contains" | "StartsWith" | "EndsWith") {
                    string_search(o, &m)
                } else if m == "MakeTransform" {
                    make_transform(o)
                } else if matches!(m.as_str(), "GetControlledPawn" | "K2_GetPawn" | "GetController")
                {
                    // A player and the entity it controls are one entity in
                    // Seal, so going between them keeps the same entity.
                    let mut same = Mapped::new("select").s("type", "entity");
                    put_default(&mut same.nodes[0], "cond", SealValue::Bool(true));
                    let unused = same.add(Node::new(0, "self"));
                    same.wire(unused, "out", 0, "b");
                    Ok(same.pin("self", "a").pin("ReturnValue", "out"))
                } else if matches!(
                    m.as_str(),
                    "GetPlayerPawn" | "GetPlayerCharacter" | "GetPlayerController"
                ) {
                    // The entity the local player controls; the host says which.
                    Ok(one("player", None, &[("PlayerIndex", "index"), ("ReturnValue", "out")]))
                } else if m == "GetAllActorsOfClass" || m == "GetActorOfClass" {
                    let mut state =
                        format!("found_{}", ident(o.name.rsplit('/').next().unwrap_or(&o.name)));
                    while var_names.contains(&state) {
                        state.push('_');
                    }
                    find_call(o, &state, m == "GetActorOfClass")
                } else if matches!(
                    m.as_str(),
                    "Conv_IntToText"
                        | "Conv_Int64ToText"
                        | "Conv_ByteToText"
                        | "Conv_DoubleToText"
                        | "Conv_FloatToText"
                ) {
                    number_text(o, &m)
                } else if m == "Conv_BoolToText" {
                    Ok(one("to_str", Some(Ty::Bool), &[("InBool", "a"), ("ReturnValue", "out")]))
                } else if m == "GetVelocity" {
                    let mut get = Mapped {
                        nodes: vec![field_node("world_get", "body", "velocity", "vec3")],
                        ..Mapped::new("world_get")
                    };
                    target(&mut get, 0, "entity", o);
                    Ok(get.pin("ReturnValue", "out"))
                } else if let Some(axis) =
                    ["GetActorForwardVector", "GetActorRightVector", "GetActorUpVector"]
                        .iter()
                        .position(|n| *n == m)
                {
                    let mut e = Expr::new();
                    let rot = e.node(transform_field("world_get", "rotation"), &[]);
                    let v = rot_axis(&mut e, Arg::N(rot), axis as u8);
                    let mut m = e.out("ReturnValue", v);
                    target(&mut m, rot, "entity", o);
                    Ok(m)
                } else if m == "GetDistanceTo" {
                    // Zero when there is no other actor, as in the classic call.
                    use Arg::*;
                    let mut e = Expr::new();
                    let here = e.node(transform_field("world_get", "translation"), &[]);
                    let other = In("OtherActor", Ty::Entity);
                    let ok = e.node(Node::new(0, "is_valid"), &[("entity", other)]);
                    // Graphs read every input, so a missing actor reads this
                    // object's place instead; the result is zero either way.
                    let me = e.node(Node::new(0, "self"), &[]);
                    let who = e.sel(Ty::Entity, ok, other, N(me));
                    let there =
                        e.node(transform_field("world_get", "translation"), &[("entity", N(who))]);
                    let d = e.op("sub", Ty::Vec3, &[("a", N(here)), ("b", N(there))]);
                    let len = e.op("length", Ty::Vec3, &[("v", N(d))]);
                    let r = e.sel(Ty::Float, ok, N(len), F(0.0));
                    let mut m = e.out("ReturnValue", r);
                    target(&mut m, here, "entity", o);
                    Ok(m)
                } else if m == "GetDisplayName" || m == "GetObjectName" {
                    let mut get = Mapped {
                        nodes: vec![field_node("world_get", "editor", "name", "str")],
                        ..Mapped::new("world_get")
                    };
                    if o.pin("Object").is_some_and(|p| !p.linked.is_empty()) {
                        get = get.pin("Object", "entity");
                    } else {
                        let me = get.add(Node::new(0, "self"));
                        get.wire(me, "out", 0, "entity");
                    }
                    Ok(get.pin("ReturnValue", "out"))
                } else if m == "ActorHasTag" {
                    // Seal objects carry one tag, so having a tag means it is that tag.
                    let mut has = Mapped::new("compare").s("type", "str").s("op", "eq");
                    let get = has.add(field_node("world_get", "editor", "tag", "str"));
                    has.wire(get, "out", 0, "a");
                    target(&mut has, get, "entity", o);
                    Ok(has.pin("Tag", "b").pin("ReturnValue", "out"))
                } else if m == "SetActorHiddenInGame" {
                    // Hiding hides the actor's sprite; it keeps running.
                    let mut set = Mapped {
                        nodes: vec![Node::new(0, "world_set")
                            .with("component", SealValue::String("sprite".into()))
                            .with("field", SealValue::String("hidden".into()))
                            .with("type", SealValue::String("bool".into()))],
                        ..Mapped::new("world_set")
                    };
                    target(&mut set, 0, "entity", o);
                    Ok(set.pin("execute", "in").pin("then", "then").pin("bNewHidden", "value"))
                } else if m == "IsValid" {
                    Ok(Mapped::new("is_valid").pin("Object", "entity").pin("ReturnValue", "out"))
                } else if matches!(
                    m.as_str(),
                    "K2_SetTimer"
                        | "K2_ClearTimer"
                        | "K2_SetTimerDelegate"
                        | "K2_ClearTimerDelegate"
                ) {
                    timer_call(o, m.starts_with("K2_Set"), &objects, &customs)
                } else if m == "K2_ClearTimerHandle" || m == "K2_ClearAndInvalidateTimerHandle" {
                    clear_by_handle(
                        o,
                        m == "K2_ClearAndInvalidateTimerHandle",
                        &timers,
                        &objects,
                        &vars,
                    )
                } else if let Some(mapped) = expand_math(&m) {
                    Ok(mapped)
                } else if let Some(f) = library(&m) {
                    let chainable = f.pins.len() == 3
                        && matches!(f.kind, "add" | "mul" | "min" | "max" | "and" | "or");
                    if extra > 0 && !chainable {
                        Err(Status::Unsupported("more than two inputs".into()))
                    } else {
                        let mut mapped = Mapped::new(f.kind);
                        if let Some(t) = f.ty {
                            mapped = mapped.s("type", t.name());
                        }
                        if let Some(op) = f.op {
                            mapped = mapped.s("op", op);
                        }
                        for (a, b) in &f.pins {
                            mapped = mapped.pin(a, b);
                        }
                        if extra > 0 {
                            // A + B + C + ...: a chain of two-input nodes, left to right
                            // like the classic node adds them.
                            let operands = operand_pins(o);
                            let mut last = 0;
                            for name in operands.iter().skip(2) {
                                let next = mapped.add(mapped.nodes[0].clone());
                                mapped.wire(last, "out", next, "a");
                                mapped = mapped.at(name, next, "b");
                                last = next;
                            }
                            mapped = mapped.at("ReturnValue", last, "out");
                        }
                        if f.kind == "to_str" && f.ty == Some(Ty::Float) {
                            report.notes.push(format!(
                                "{}: very large or very small floats may print with different digits",
                                o.name
                            ));
                        }
                        Ok(mapped)
                    }
                } else {
                    Err(Status::Unsupported("function not in the node library yet".into()))
                }
            }
            _ => Err(Status::Unsupported("node type not in the node library yet".into())),
        };
        let status = match result {
            Ok(mut m) => {
                let base = next_id;
                next_id += m.nodes.len() as u32;
                for (k, n) in m.nodes.iter_mut().enumerate() {
                    n.id = base + k as u32;
                }
                // Literal defaults on unlinked inputs.
                for p in &o.pins {
                    if p.output || p.category == "exec" || !p.linked.is_empty() {
                        continue;
                    }
                    if let (Some((k, seal)), Some(text), Some(ty)) =
                        (m.pins.get(&p.name), &p.default, pin_ty(p))
                    {
                        match literal(ty, text) {
                            Some(v) => put_default(&mut m.nodes[*k], seal, v),
                            None => report.notes.push(format!(
                                "{}: could not read default {text:?} of {}",
                                o.name, p.name
                            )),
                        }
                    }
                    // Unmapped literals are display options (print colour and
                    // the like) that have no effect in Seal.
                }
                for (name, v) in m.vars.drain(..) {
                    var_names.push(name.clone());
                    graph = graph.var(&name, v);
                }
                for (a, ap, b, bp) in &m.links {
                    graph = graph.link(base + *a as u32, ap, base + *b as u32, bp);
                }
                ids[i] = Some(base);
                maps[i] =
                    Some(m.pins.into_iter().map(|(k, (n, p))| (k, (base + n as u32, p))).collect());
                let kind = m.nodes[0].kind.clone();
                for n in m.nodes {
                    graph = graph.node(n);
                }
                Status::Mapped(kind)
            }
            Err(s) => s,
        };
        report.nodes.push(NodeReport {
            name: o.name.clone(),
            class,
            member: what,
            id: ids[i],
            status,
            line: o.line,
        });
    }

    let ends: Vec<bool> = report
        .nodes
        .iter()
        .map(|n| n.status == Status::Skipped("end of function".into()))
        .collect();
    // Links, from each mapped output pin, followed through reroute knots.
    let by_name: HashMap<&str, usize> =
        objects.iter().enumerate().map(|(i, o)| (o.name.as_str(), i)).collect();
    let mut seen = std::collections::BTreeSet::new();
    for (i, o) in objects.iter().enumerate() {
        let Some(map) = &maps[i] else { continue };
        for p in o.pins.iter().filter(|p| p.output) {
            if p.linked.is_empty() {
                continue;
            }
            let Some((from_id, from_pin)) = map.get(&p.name) else {
                if !PLUMBING.contains(&p.name.as_str()) {
                    report.dropped_links += p.linked.len();
                    report.notes.push(format!(
                        "{}: output {} has no Seal pin; its links were dropped",
                        o.name, p.name
                    ));
                }
                continue;
            };
            let mut stack: Vec<(String, String, u32)> =
                p.linked.iter().map(|(n, id)| (n.clone(), id.clone(), 0)).collect();
            while let Some((node, pin_id, depth)) = stack.pop() {
                let Some(&j) = by_name.get(node.as_str()) else {
                    report.dropped_links += 1;
                    report.notes.push(format!(
                        "{}.{} links to {node}, which is not in the selection",
                        o.name, p.name
                    ));
                    continue;
                };
                let target = &objects[j];
                if target.short_class() == "K2Node_Knot" {
                    if depth < 64 {
                        for kp in target.pins.iter().filter(|kp| kp.output) {
                            stack.extend(
                                kp.linked.iter().map(|(n, id)| (n.clone(), id.clone(), depth + 1)),
                            );
                        }
                    }
                    continue;
                }
                if ends[j] {
                    // Into the end of a function that returns nothing: nothing to keep.
                    continue;
                }
                let tp = target.pins.iter().find(|tp| tp.id == pin_id);
                match (&maps[j], tp) {
                    (Some(tmap), Some(tp)) => match tmap.get(&tp.name) {
                        Some((to_id, to_pin)) => {
                            if seen.insert((*from_id, from_pin.clone(), *to_id, to_pin.clone())) {
                                graph = graph.link(*from_id, from_pin, *to_id, to_pin);
                            }
                        }
                        None => {
                            report.dropped_links += 1;
                            report.notes.push(format!(
                                "{}.{} links to {}.{}, which has no Seal pin",
                                o.name, p.name, target.name, tp.name
                            ));
                        }
                    },
                    _ => {
                        report.dropped_links += 1;
                        if ids[j].is_some() {
                            report.notes.push(format!(
                                "{}.{} links to a missing pin on {}",
                                o.name, p.name, target.name
                            ));
                        }
                    }
                }
            }
        }
    }
    // Links leaving nodes that were not mapped (links into them were counted above).
    for (j, t) in objects.iter().enumerate() {
        if ids[j].is_some()
            || matches!(
                t.short_class(),
                "K2Node_Knot"
                    | "EdGraphNode_Comment"
                    | "K2Node_FunctionResult"
                    | "K2Node_CreateDelegate"
            )
            || (t.short_class() == "K2Node_VariableGet" && feeds_only_set_text(t, &objects))
        {
            continue;
        }
        report.dropped_links +=
            t.pins.iter().filter(|p| p.output).map(|p| p.linked.len()).sum::<usize>();
    }
    merge_events(&mut graph, "tick");
    merge_events(&mut graph, "start");
    Ok(Import { graph, report })
}

#[cfg(test)]
mod string_nodes {
    use super::library;

    #[test]
    fn text_length_and_case_map_to_seal_nodes() {
        for (member, kind) in
            [("Len", "str_len"), ("ToUpper", "str_upper"), ("ToLower", "str_lower")]
        {
            let f = library(member).unwrap_or_else(|| panic!("{member} is not mapped"));
            assert_eq!(f.kind, kind);
            assert!(f.pins.iter().any(|(_, to)| *to == "out"));
        }
    }
}
