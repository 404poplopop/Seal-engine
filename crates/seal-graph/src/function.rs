//! Named functions inside a logic graph.
//!
//! A function has typed inputs, typed outputs, its own nodes and local
//! variables. The event graph and other functions call it with a `call_fn`
//! node. [`lower`] turns each function into an ordinary event so the
//! interpreter, the runtime and Rust codegen all run it the same way. A
//! function that can reach itself is rejected: locals are one slot each, and
//! a second call of the same function would overwrite the first.

use crate::{
    value_from_text, value_to_text, BuildError, Graph, Link, Node, Sig, Ty, Value, VarDef,
};
use seal_sealtext::{SealMap, SealValue};
use std::collections::BTreeSet;

/// Node kind: the start of a function. Its outputs are the function inputs.
pub const ENTRY: &str = "entry";
/// Node kind: hand the function outputs back to the caller and stop.
pub const FN_RETURN: &str = "fn_return";
/// Node kind: call a function. `name` selects it; pins match the signature.
pub const CALL: &str = "call_fn";

const PREFIX: &str = "__fn_";

/// One input or output pin on a function.
#[derive(Clone, Debug, PartialEq)]
pub struct PinDef {
    pub name: String,
    pub ty: Ty,
}

/// A function saved with the graph: signature, locals and its own body.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphFn {
    pub name: String,
    pub inputs: Vec<PinDef>,
    pub outputs: Vec<PinDef>,
    pub locals: Vec<VarDef>,
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
}

fn synth(name: &str) -> String {
    format!("{PREFIX}{name}")
}

fn local_var(func: &str, local: &str) -> String {
    format!("{PREFIX}{func}_{local}")
}

fn param_str<'a>(node: &'a Node, key: &str) -> Option<&'a str> {
    match node.param(key) {
        Some(SealValue::String(s)) => Some(s.as_str()),
        _ => None,
    }
}

fn letter_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn check_fn_name(name: &str) -> Result<(), String> {
    if letter_name(name) {
        Ok(())
    } else {
        Err(format!(
            "function name `{name}` must start with a letter and use only letters, digits and underscores"
        ))
    }
}

fn check_pin_name(name: &str) -> Result<(), String> {
    if letter_name(name) && name != "in" && name != "then" {
        Ok(())
    } else {
        Err(format!("pin `{name}` must start with a letter and must not be `in` or `then`"))
    }
}

fn check_local_name(name: &str) -> Result<(), String> {
    if letter_name(name) {
        Ok(())
    } else {
        Err(format!("variable `{name}` must start with a letter"))
    }
}

fn pairs(pins: &[PinDef]) -> Vec<(String, Ty)> {
    pins.iter().map(|p| (p.name.clone(), p.ty)).collect()
}

fn fresh_name(taken: &[String], prefix: &str) -> String {
    let mut i = 1u32;
    loop {
        let name = if i == 1 { prefix.to_string() } else { format!("{prefix}{i}") };
        if !taken.iter().any(|n| n == &name) {
            return name;
        }
        i += 1;
    }
}

fn pos(x: f32, y: f32) -> SealValue {
    let n = |v: f32| SealValue::Number(format!("{v:.1}"));
    SealValue::Array(vec![n(x), n(y)])
}

fn pins_text(pins: &[PinDef]) -> SealValue {
    SealValue::Array(
        pins.iter()
            .map(|p| {
                SealValue::Map(SealMap::new(vec![
                    ("name".into(), SealValue::String(p.name.clone())),
                    ("type".into(), SealValue::String(p.ty.name().into())),
                ]))
            })
            .collect(),
    )
}

fn write_node(n: &Node) -> SealValue {
    let mut entries = vec![
        ("id".into(), SealValue::Number(n.id.to_string())),
        ("kind".into(), SealValue::String(n.kind.clone())),
    ];
    entries.extend(n.params.iter().cloned());
    SealValue::Map(SealMap::new(entries))
}

fn write_var(v: &VarDef) -> SealValue {
    SealValue::Map(SealMap::new(vec![
        ("name".into(), SealValue::String(v.name.clone())),
        ("type".into(), SealValue::String(v.ty.name().into())),
        ("default".into(), value_to_text(&v.default)),
    ]))
}

/// SealText for the `functions` list. Absent when the graph has none.
pub fn write_functions(functions: &[GraphFn]) -> SealValue {
    SealValue::Array(
        functions
            .iter()
            .map(|f| {
                SealValue::Map(SealMap::new(vec![
                    ("name".into(), SealValue::String(f.name.clone())),
                    ("inputs".into(), pins_text(&f.inputs)),
                    ("outputs".into(), pins_text(&f.outputs)),
                    ("locals".into(), SealValue::Array(f.locals.iter().map(write_var).collect())),
                    ("nodes".into(), SealValue::Array(f.nodes.iter().map(write_node).collect())),
                    (
                        "links".into(),
                        SealValue::Array(
                            f.links.iter().map(|l| SealValue::String(l.text())).collect(),
                        ),
                    ),
                ]))
            })
            .collect(),
    )
}

fn read_pins(v: Option<&SealValue>, what: &str) -> Result<Vec<PinDef>, BuildError> {
    let Some(v) = v else { return Ok(Vec::new()) };
    let SealValue::Array(items) = v else {
        return Err(BuildError::general(format!("function `{what}` must be a list")));
    };
    let mut out = Vec::new();
    for item in items {
        let name = match item.get("name") {
            Some(SealValue::String(s)) => s.clone(),
            _ => return Err(BuildError::general(format!("a {what} pin has no name"))),
        };
        let ty = match item.get("type") {
            Some(SealValue::String(s)) => Ty::parse(s)
                .ok_or_else(|| BuildError::general(format!("pin `{name}` has unknown type {s}")))?,
            _ => return Err(BuildError::general(format!("pin `{name}` has no type"))),
        };
        out.push(PinDef { name, ty });
    }
    Ok(out)
}

fn read_locals(v: Option<&SealValue>, func: &str) -> Result<Vec<VarDef>, BuildError> {
    let Some(v) = v else { return Ok(Vec::new()) };
    let SealValue::Array(items) = v else {
        return Err(BuildError::general(format!("function `{func}` locals must be a list")));
    };
    let mut out = Vec::new();
    for item in items {
        let name = match item.get("name") {
            Some(SealValue::String(s)) => s.clone(),
            _ => return Err(BuildError::general(format!("a local in `{func}` has no name"))),
        };
        let ty = match item.get("type") {
            Some(SealValue::String(s)) => Ty::parse(s).ok_or_else(|| {
                BuildError::general(format!("local `{name}` in `{func}` has unknown type {s}"))
            })?,
            _ => {
                return Err(BuildError::general(format!("local `{name}` in `{func}` has no type")))
            }
        };
        let default = match item.get("default") {
            None => ty.zero(),
            Some(d) => value_from_text(ty, d).ok_or_else(|| {
                BuildError::general(format!("local `{name}` in `{func}`: default is not a {ty}"))
            })?,
        };
        out.push(VarDef { name, ty, default });
    }
    Ok(out)
}

fn read_nodes(v: Option<&SealValue>, func: &str) -> Result<Vec<Node>, BuildError> {
    let Some(v) = v else { return Ok(Vec::new()) };
    let SealValue::Array(items) = v else {
        return Err(BuildError::general(format!("function `{func}` nodes must be a list")));
    };
    let mut nodes = Vec::new();
    for node in items {
        let entries = node.as_map().ok_or_else(|| {
            BuildError::general(format!("a node in function `{func}` must be a map"))
        })?;
        let id =
            node.get("id").and_then(crate::num).and_then(|n| n.parse::<u32>().ok()).ok_or_else(
                || BuildError::general(format!("a node in function `{func}` has no id")),
            )?;
        let kind = match node.get("kind") {
            Some(SealValue::String(s)) => s.clone(),
            _ => return Err(BuildError::node(id, "no kind")),
        };
        let params = entries.iter().filter(|(k, _)| k != "id" && k != "kind").cloned().collect();
        nodes.push(Node { id, kind, params });
    }
    Ok(nodes)
}

fn read_links(v: Option<&SealValue>, func: &str) -> Result<Vec<Link>, BuildError> {
    let Some(v) = v else { return Ok(Vec::new()) };
    let SealValue::Array(items) = v else {
        return Err(BuildError::general(format!("function `{func}` links must be a list")));
    };
    let mut links = Vec::new();
    for link in items {
        let SealValue::String(s) = link else {
            return Err(BuildError::general(format!("a link in function `{func}` must be text")));
        };
        links.push(Link::parse(s).ok_or_else(|| {
            BuildError::general(format!(
                "bad link `{s}` in function `{func}`; write `1.then -> 2.in`"
            ))
        })?);
    }
    Ok(links)
}

/// Read `functions` from a graph file. Missing means there are none.
pub fn read_functions(v: &SealValue) -> Result<Vec<GraphFn>, BuildError> {
    let Some(list) = v.get("functions") else { return Ok(Vec::new()) };
    let SealValue::Array(items) = list else {
        return Err(BuildError::general("`functions` must be a list"));
    };
    let mut functions = Vec::new();
    for item in items {
        if item.as_map().is_none() {
            return Err(BuildError::general("a function must be a map"));
        }
        let name = match item.get("name") {
            Some(SealValue::String(s)) => s.clone(),
            _ => return Err(BuildError::general("a function has no name")),
        };
        if !letter_name(&name) {
            return Err(BuildError::general(format!("function name `{name}` is not a name")));
        }
        let inputs = item
            .get("inputs")
            .ok_or_else(|| BuildError::general(format!("function `{name}` has no inputs")))?;
        let outputs = item
            .get("outputs")
            .ok_or_else(|| BuildError::general(format!("function `{name}` has no outputs")))?;
        let nodes = item
            .get("nodes")
            .ok_or_else(|| BuildError::general(format!("function `{name}` has no nodes")))?;
        let links = item
            .get("links")
            .ok_or_else(|| BuildError::general(format!("function `{name}` has no links")))?;
        functions.push(GraphFn {
            inputs: read_pins(Some(inputs), "inputs")?,
            outputs: read_pins(Some(outputs), "outputs")?,
            locals: read_locals(item.get("locals"), &name)?,
            nodes: read_nodes(Some(nodes), &name)?,
            links: read_links(Some(links), &name)?,
            name,
        });
    }
    Ok(functions)
}

/// The function the editor had open, if the file stored one.
pub fn read_open(v: &SealValue) -> Result<Option<String>, BuildError> {
    match v.get("open_function") {
        None => Ok(None),
        Some(SealValue::String(s)) if s.is_empty() => Ok(None),
        Some(SealValue::String(s)) if letter_name(s) => Ok(Some(s.clone())),
        Some(SealValue::String(s)) => {
            Err(BuildError::general(format!("open_function `{s}` is not a name")))
        }
        Some(_) => Err(BuildError::general("open_function must be text")),
    }
}

/// True when [`lower`] would change the graph.
pub fn needs_lower(graph: &Graph) -> bool {
    !graph.functions.is_empty()
        || graph.nodes.iter().any(|n| matches!(n.kind.as_str(), ENTRY | FN_RETURN | CALL))
}

fn event_names(graph: &Graph) -> Vec<String> {
    graph
        .nodes
        .iter()
        .filter(|n| n.kind == "event")
        .filter_map(|n| param_str(n, "name").map(str::to_string))
        .collect()
}

fn check_closed(nodes: &[Node], links: &[Link], place: &str, errors: &mut Vec<BuildError>) {
    let ids: BTreeSet<u32> = nodes.iter().map(|n| n.id).collect();
    for link in links {
        if !ids.contains(&link.from.node) || !ids.contains(&link.to.node) {
            errors.push(BuildError::general(format!(
                "link {} in {place} points at a node that is not there",
                link.text()
            )));
        }
    }
}

fn check_pins(pins: &[PinDef], place: &str, errors: &mut Vec<BuildError>) {
    let mut seen = BTreeSet::new();
    for pin in pins {
        if let Err(message) = check_pin_name(&pin.name) {
            errors.push(BuildError::general(format!("{place}: {message}")));
        }
        if !seen.insert(pin.name.clone()) {
            errors.push(BuildError::general(format!("{place}: pin `{}` is repeated", pin.name)));
        }
    }
}

fn validate(graph: &Graph, errors: &mut Vec<BuildError>) {
    let mut names = BTreeSet::new();
    let events = event_names(graph);
    let mut ids = BTreeSet::new();
    for node in &graph.nodes {
        if !ids.insert(node.id) {
            errors.push(BuildError::node(node.id, "duplicate node id"));
        }
        match node.kind.as_str() {
            ENTRY | FN_RETURN => errors.push(BuildError::node(
                node.id,
                format!("{} belongs inside a function", node.kind),
            )),
            CALL => match param_str(node, "name") {
                None => errors.push(BuildError::node(node.id, "call_fn needs a text `name`")),
                Some(name) if !graph.functions.iter().any(|f| f.name == name) => {
                    errors.push(BuildError::node(node.id, format!("no function `{name}`")));
                }
                Some(_) => {}
            },
            _ => {}
        }
    }
    check_closed(&graph.nodes, &graph.links, "the event graph", errors);
    let mut mangled = BTreeSet::new();
    for var in &graph.vars {
        mangled.insert(var.name.clone());
    }
    for func in &graph.functions {
        if let Err(message) = check_fn_name(&func.name) {
            errors.push(BuildError::general(message));
        }
        if !names.insert(func.name.clone()) {
            errors.push(BuildError::general(format!("duplicate function {}", func.name)));
        }
        let internal = synth(&func.name);
        if events.iter().any(|e| e == &internal) {
            errors.push(BuildError::general(format!(
                "function `{}` collides with an event named `{internal}`",
                func.name
            )));
        }
        check_pins(&func.inputs, &format!("function `{}` inputs", func.name), errors);
        check_pins(&func.outputs, &format!("function `{}` outputs", func.name), errors);
        let mut local_names = BTreeSet::new();
        for local in &func.locals {
            if let Err(message) = check_local_name(&local.name) {
                errors.push(BuildError::general(format!("function `{}`: {message}", func.name)));
            }
            if !local_names.insert(local.name.clone()) {
                errors.push(BuildError::general(format!(
                    "function `{}` has two locals named {}",
                    func.name, local.name
                )));
            }
            if local.default.ty() != local.ty {
                errors.push(BuildError::general(format!(
                    "local `{}` of function `{}`: default is not a {}",
                    local.name, func.name, local.ty
                )));
            }
            let slot = local_var(&func.name, &local.name);
            if !mangled.insert(slot.clone()) {
                errors.push(BuildError::general(format!(
                    "local `{}` of function `{}` collides with another variable",
                    local.name, func.name
                )));
            }
        }
        let entries = func.nodes.iter().filter(|n| n.kind == ENTRY).count();
        if entries != 1 {
            errors.push(BuildError::general(format!(
                "function `{}` needs one entry node",
                func.name
            )));
        }
        for node in &func.nodes {
            if !ids.insert(node.id) {
                errors.push(BuildError::node(node.id, "duplicate node id"));
            }
            match node.kind.as_str() {
                "event" => errors.push(BuildError::node(
                    node.id,
                    format!("an event belongs on the event graph, not inside `{}`", func.name),
                )),
                "return" => errors.push(BuildError::node(
                    node.id,
                    format!("use {FN_RETURN} inside function `{}`", func.name),
                )),
                CALL => match param_str(node, "name") {
                    None => errors.push(BuildError::node(node.id, "call_fn needs a text `name`")),
                    Some(name) if !graph.functions.iter().any(|f| f.name == name) => {
                        errors.push(BuildError::node(node.id, format!("no function `{name}`")));
                    }
                    Some(_) => {}
                },
                _ => {}
            }
        }
        check_closed(&func.nodes, &func.links, &format!("function `{}`", func.name), errors);
    }
}

fn rewrite(node: &Node, func: Option<&GraphFn>) -> Node {
    let mut out = node.clone();
    let Some(func) = func else {
        if out.kind == CALL {
            if let Some(name) = param_str(&out, "name").map(str::to_string) {
                out.kind = "call".into();
                out.set("name", SealValue::String(synth(&name)));
            }
        }
        return out;
    };
    match out.kind.as_str() {
        FN_RETURN => {
            out.kind = "return".into();
            out.set("name", SealValue::String(synth(&func.name)));
        }
        CALL => {
            if let Some(name) = param_str(&out, "name").map(str::to_string) {
                out.kind = "call".into();
                out.set("name", SealValue::String(synth(&name)));
            }
        }
        "get" | "set" => {
            if let Some(name) = param_str(&out, "name").map(str::to_string) {
                if func.locals.iter().any(|l| l.name == name) {
                    out.set("name", SealValue::String(local_var(&func.name, &name)));
                }
            }
        }
        _ => {}
    }
    out
}

fn reset_set(id: u32, var: &str, default: &Value) -> Node {
    Node::new(id, "set").with("name", SealValue::String(var.into())).with(
        "defaults",
        SealValue::Map(SealMap::new(vec![("value".into(), value_to_text(default))])),
    )
}

fn flatten(graph: &Graph) -> Graph {
    let mut flat = Graph::new(&graph.name);
    flat.vars.clone_from(&graph.vars);
    for func in &graph.functions {
        for local in &func.locals {
            flat.vars.push(VarDef {
                name: local_var(&func.name, &local.name),
                ty: local.ty,
                default: local.default.clone(),
            });
        }
    }
    for node in &graph.nodes {
        flat.nodes.push(rewrite(node, None));
    }
    flat.links.clone_from(&graph.links);
    let mut next_id = graph.next_id();
    for func in &graph.functions {
        let Some(entry) = func.nodes.iter().find(|n| n.kind == ENTRY) else { continue };
        let entry_id = entry.id;
        let mut event = entry.clone();
        event.kind = "event".into();
        event.set("name", SealValue::String(synth(&func.name)));
        event.set("params", pins_text(&func.inputs));
        event.set("returns", pins_text(&func.outputs));
        flat.nodes.push(event);
        for node in &func.nodes {
            if node.kind == ENTRY {
                continue;
            }
            flat.nodes.push(rewrite(node, Some(func)));
        }
        flat.links.extend(func.links.iter().cloned());
        if func.locals.is_empty() {
            continue;
        }
        let Some(pos) =
            flat.links.iter().position(|l| l.from.node == entry_id && l.from.pin == "then")
        else {
            continue;
        };
        let target = flat.links[pos].to.clone();
        flat.links.remove(pos);
        let mut prev = entry_id;
        let mut prev_pin = "then".to_string();
        for local in &func.locals {
            let id = next_id;
            next_id += 1;
            flat.nodes.push(reset_set(id, &local_var(&func.name, &local.name), &local.default));
            flat.links.push(Link::new(prev, &prev_pin, id, "in"));
            prev = id;
            prev_pin = "then".into();
        }
        flat.links.push(Link::new(prev, &prev_pin, target.node, &target.pin));
    }
    flat
}

/// Replace functions with ordinary events, calls, returns and local resets.
/// Graphs with no functions are returned unchanged.
pub fn lower(graph: &Graph) -> Result<Graph, Vec<BuildError>> {
    if !needs_lower(graph) {
        return Ok(graph.clone());
    }
    let mut errors = Vec::new();
    validate(graph, &mut errors);
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(flatten(graph))
}

/// Functions that can reach themselves, including through another function or
/// an event. The program already allows event recursion; function locals do not.
pub fn recursion_errors(program: &crate::Program) -> Vec<BuildError> {
    let flags = program.reentrant();
    program
        .events
        .iter()
        .enumerate()
        .filter(|(i, event)| flags[*i] && event.name.starts_with(PREFIX))
        .map(|(_, event)| {
            let name = event.name.trim_start_matches(PREFIX);
            BuildError::general(format!("function `{name}` calls itself; recursion is not allowed"))
        })
        .collect()
}

/// The node with this id, in the event graph or in a function body.
pub fn find_node(graph: &Graph, id: u32) -> Option<&Node> {
    graph
        .nodes
        .iter()
        .chain(graph.functions.iter().flat_map(|f| f.nodes.iter()))
        .find(|n| n.id == id)
}

fn function_of(graph: &Graph, id: u32) -> Option<&GraphFn> {
    graph.functions.iter().find(|f| f.nodes.iter().any(|n| n.id == id))
}

fn pure_get(ty: Ty) -> Sig {
    Sig { exec_in: false, exec_outs: Vec::new(), ins: Vec::new(), outs: vec![("out".into(), ty)] }
}

fn flow_set(ty: Ty) -> Sig {
    Sig {
        exec_in: true,
        exec_outs: vec!["then".into()],
        ins: vec![("value".into(), ty)],
        outs: vec![("value".into(), ty)],
    }
}

/// Pins for function nodes, and for get/set of a local. `None` when the
/// ordinary node rules apply.
pub fn sig_override(graph: &Graph, id: u32) -> Option<Result<Sig, BuildError>> {
    let node = find_node(graph, id)?;
    match node.kind.as_str() {
        ENTRY => {
            let outs = function_of(graph, id).map(|f| pairs(&f.inputs)).unwrap_or_default();
            Some(Ok(Sig { exec_in: false, exec_outs: vec!["then".into()], ins: Vec::new(), outs }))
        }
        FN_RETURN => {
            let ins = function_of(graph, id).map(|f| pairs(&f.outputs)).unwrap_or_default();
            Some(Ok(Sig { exec_in: true, exec_outs: Vec::new(), ins, outs: Vec::new() }))
        }
        CALL => {
            let Some(name) = param_str(node, "name") else {
                return Some(Err(BuildError::node(id, "call_fn needs a text `name`")));
            };
            let Some(func) = graph.functions.iter().find(|f| f.name == name) else {
                return Some(Err(BuildError::node(id, format!("no function `{name}`"))));
            };
            Some(Ok(Sig {
                exec_in: true,
                exec_outs: vec!["then".into()],
                ins: pairs(&func.inputs),
                outs: pairs(&func.outputs),
            }))
        }
        "get" | "set" => {
            let func = function_of(graph, id)?;
            let name = param_str(node, "name")?;
            let local = func.locals.iter().find(|v| v.name == name)?;
            Some(Ok(if node.kind == "get" { pure_get(local.ty) } else { flow_set(local.ty) }))
        }
        _ => None,
    }
}

/// Nodes of the event graph, or of the function named by `page`.
pub fn page_nodes<'a>(graph: &'a Graph, page: Option<&str>) -> &'a [Node] {
    match page {
        None => &graph.nodes,
        Some(name) => graph
            .functions
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.nodes.as_slice())
            .unwrap_or(&[]),
    }
}

/// Links of the event graph, or of the function named by `page`.
pub fn page_links<'a>(graph: &'a Graph, page: Option<&str>) -> &'a [Link] {
    match page {
        None => &graph.links,
        Some(name) => graph
            .functions
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.links.as_slice())
            .unwrap_or(&[]),
    }
}

/// Mutable nodes and links for the page the editor is showing.
pub fn page_parts<'a>(
    graph: &'a mut Graph,
    page: Option<&str>,
) -> Option<(&'a mut Vec<Node>, &'a mut Vec<Link>)> {
    match page {
        None => Some((&mut graph.nodes, &mut graph.links)),
        Some(name) => {
            let func = graph.functions.iter_mut().find(|f| f.name == name)?;
            Some((&mut func.nodes, &mut func.links))
        }
    }
}

fn each_page(graph: &mut Graph, mut visit: impl FnMut(&mut Vec<Node>, &mut Vec<Link>)) {
    visit(&mut graph.nodes, &mut graph.links);
    for func in &mut graph.functions {
        visit(&mut func.nodes, &mut func.links);
    }
}

impl Graph {
    /// Add a function with one integer input, one integer output, an entry and a return.
    pub fn add_function(&mut self) -> String {
        let taken: Vec<String> = self.functions.iter().map(|f| f.name.clone()).collect();
        let name = fresh_name(&taken, "function");
        let id = self.next_id();
        let entry = Node::new(id, ENTRY).with("pos", pos(40.0, 80.0));
        let ret = Node::new(id + 1, FN_RETURN).with("pos", pos(420.0, 80.0));
        self.functions.push(GraphFn {
            name: name.clone(),
            inputs: vec![PinDef { name: "value".into(), ty: Ty::Int }],
            outputs: vec![PinDef { name: "result".into(), ty: Ty::Int }],
            locals: Vec::new(),
            nodes: vec![entry, ret],
            links: Vec::new(),
        });
        name
    }

    /// Remove a function. Call nodes that named it stay, and the graph then reports them.
    pub fn remove_function(&mut self, name: &str) -> bool {
        let before = self.functions.len();
        self.functions.retain(|f| f.name != name);
        if self.open_function.as_deref() == Some(name) {
            self.open_function = None;
        }
        self.functions.len() != before
    }

    /// Rename a function and every Call node that uses it.
    pub fn rename_function(&mut self, from: &str, to: &str) -> Result<(), String> {
        check_fn_name(to)?;
        if from == to {
            return Ok(());
        }
        if !self.functions.iter().any(|f| f.name == from) {
            return Err(format!("no function {from}"));
        }
        if self.functions.iter().any(|f| f.name == to) {
            return Err(format!("function `{to}` already exists"));
        }
        let index = self.functions.iter().position(|f| f.name == from).unwrap();
        self.functions[index].name = to.to_string();
        if self.open_function.as_deref() == Some(from) {
            self.open_function = Some(to.to_string());
        }
        let to = to.to_string();
        each_page(self, |nodes, _| {
            for node in nodes.iter_mut() {
                if node.kind == CALL && param_str(node, "name") == Some(from) {
                    node.set("name", SealValue::String(to.clone()));
                }
            }
        });
        Ok(())
    }

    pub fn add_pin(&mut self, func: &str, output: bool) -> bool {
        let Some(func) = self.functions.iter_mut().find(|f| f.name == func) else { return false };
        let pins = if output { &mut func.outputs } else { &mut func.inputs };
        let taken: Vec<String> = pins.iter().map(|p| p.name.clone()).collect();
        let name = fresh_name(&taken, if output { "out" } else { "a" });
        pins.push(PinDef { name, ty: Ty::Int });
        true
    }

    pub fn remove_pin(&mut self, func: &str, output: bool, index: usize) -> bool {
        let Some(func) = self.functions.iter_mut().find(|f| f.name == func) else { return false };
        let pins = if output { &mut func.outputs } else { &mut func.inputs };
        if index >= pins.len() {
            return false;
        }
        pins.remove(index);
        true
    }

    /// Rename a pin and retarget links on the entry, the return and every Call node.
    pub fn rename_pin(
        &mut self,
        func_name: &str,
        output: bool,
        index: usize,
        new_name: &str,
    ) -> Result<(), String> {
        check_pin_name(new_name)?;
        let func = self
            .functions
            .iter()
            .find(|f| f.name == func_name)
            .ok_or_else(|| format!("no function {func_name}"))?;
        let pins = if output { &func.outputs } else { &func.inputs };
        if index >= pins.len() {
            return Err("no such pin".into());
        }
        let old = pins[index].name.clone();
        if old == new_name {
            return Ok(());
        }
        if pins.iter().any(|p| p.name == new_name) {
            return Err(format!("pin `{new_name}` is already used"));
        }
        let entry_ids: Vec<u32> =
            func.nodes.iter().filter(|n| n.kind == ENTRY).map(|n| n.id).collect();
        let return_ids: Vec<u32> =
            func.nodes.iter().filter(|n| n.kind == FN_RETURN).map(|n| n.id).collect();
        {
            let func = self.functions.iter_mut().find(|f| f.name == func_name).unwrap();
            let pins = if output { &mut func.outputs } else { &mut func.inputs };
            pins[index].name = new_name.to_string();
        }
        let new_name = new_name.to_string();
        let func_name = func_name.to_string();
        each_page(self, |nodes, links| {
            let call_ids: Vec<u32> = nodes
                .iter()
                .filter(|n| n.kind == CALL && param_str(n, "name") == Some(func_name.as_str()))
                .map(|n| n.id)
                .collect();
            for link in links.iter_mut() {
                if output {
                    let from_call = call_ids.contains(&link.from.node) && link.from.pin == old;
                    let into_return = return_ids.contains(&link.to.node) && link.to.pin == old;
                    if from_call {
                        link.from.pin.clone_from(&new_name);
                    }
                    if into_return {
                        link.to.pin.clone_from(&new_name);
                    }
                } else {
                    let into_call = call_ids.contains(&link.to.node) && link.to.pin == old;
                    let from_entry = entry_ids.contains(&link.from.node) && link.from.pin == old;
                    if into_call {
                        link.to.pin.clone_from(&new_name);
                    }
                    if from_entry {
                        link.from.pin.clone_from(&new_name);
                    }
                }
            }
        });
        Ok(())
    }

    pub fn set_pin_type(
        &mut self,
        func: &str,
        output: bool,
        index: usize,
        ty_name: &str,
    ) -> Result<(), String> {
        let ty = Ty::parse(ty_name).ok_or_else(|| format!("unknown type {ty_name}"))?;
        let func = self
            .functions
            .iter_mut()
            .find(|f| f.name == func)
            .ok_or_else(|| format!("no function {func}"))?;
        let pins = if output { &mut func.outputs } else { &mut func.inputs };
        let pin = pins.get_mut(index).ok_or_else(|| "no such pin".to_string())?;
        pin.ty = ty;
        Ok(())
    }

    pub fn add_local(&mut self, func: &str) -> bool {
        let Some(func) = self.functions.iter_mut().find(|f| f.name == func) else { return false };
        let taken: Vec<String> = func.locals.iter().map(|v| v.name.clone()).collect();
        let name = fresh_name(&taken, "local");
        func.locals.push(VarDef { name, ty: Ty::Int, default: Value::Int(0) });
        true
    }

    pub fn remove_local(&mut self, func: &str, index: usize) -> bool {
        let Some(func) = self.functions.iter_mut().find(|f| f.name == func) else { return false };
        if index >= func.locals.len() {
            return false;
        }
        func.locals.remove(index);
        true
    }

    pub fn rename_local(&mut self, func: &str, index: usize, new_name: &str) -> Result<(), String> {
        check_local_name(new_name)?;
        let func = self
            .functions
            .iter_mut()
            .find(|f| f.name == func)
            .ok_or_else(|| format!("no function {func}"))?;
        if index >= func.locals.len() {
            return Err("no such variable".into());
        }
        if func.locals.iter().enumerate().any(|(i, v)| i != index && v.name == new_name) {
            return Err(format!("variable `{new_name}` is already used"));
        }
        let old = func.locals[index].name.clone();
        if old == new_name {
            return Ok(());
        }
        func.locals[index].name = new_name.to_string();
        for node in &mut func.nodes {
            let uses = matches!(node.kind.as_str(), "get" | "set")
                && param_str(node, "name") == Some(old.as_str());
            if uses {
                node.set("name", SealValue::String(new_name.to_string()));
            }
        }
        Ok(())
    }

    pub fn set_local_type(
        &mut self,
        func: &str,
        index: usize,
        ty_name: &str,
    ) -> Result<(), String> {
        let ty = Ty::parse(ty_name).ok_or_else(|| format!("unknown type {ty_name}"))?;
        let func = self
            .functions
            .iter_mut()
            .find(|f| f.name == func)
            .ok_or_else(|| format!("no function {func}"))?;
        let local = func.locals.get_mut(index).ok_or_else(|| "no such variable".to_string())?;
        local.ty = ty;
        local.default = ty.zero();
        Ok(())
    }
}
