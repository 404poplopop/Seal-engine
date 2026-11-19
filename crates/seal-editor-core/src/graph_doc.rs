//! The open logic graph in the node editor: edits, undo, problems and autosave.
//!
//! Every change is written straight back to the `.graph` file, so closing the
//! editor never loses work. Node positions live in each node's `pos` field.

use seal_graph::function::{self, CALL, ENTRY, FN_RETURN};
pub use seal_graph::state_machine::{Cond, StateDef, Transition};
use seal_graph::{node_sig, node_template, state_machine, Graph, Link, Node, Program, Sig, Value};
pub use seal_graph::{CmpOp, Elem, Ty, KINDS};
use seal_sealtext::SealValue;
use std::path::{Path, PathBuf};

/// How many undo steps the editor keeps for one graph.
const UNDO_LIMIT: usize = 200;

/// Which side of a node a pin is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinSide {
    In,
    Out,
}

/// One pin as the canvas draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct PinView {
    pub name: String,
    /// `None` for execution pins.
    pub ty: Option<Ty>,
    pub side: PinSide,
    pub linked: bool,
    /// Text of the unlinked default, for data inputs the canvas can edit inline.
    pub default_text: Option<String>,
}

/// What is selected on the state machine canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineSel {
    State(usize),
    Transition(usize),
}

/// One node as the canvas draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeView {
    pub id: u32,
    pub kind: String,
    pub title: String,
    pub pos: [f32; 2],
    pub ins: Vec<PinView>,
    pub outs: Vec<PinView>,
    pub problem: Option<String>,
}

/// Readable node name for a kind id: `entry` is "Function Start", `fn_return` is
/// "Function End", others turn `add_score` into "Add Score".
pub fn kind_title(kind: &str) -> String {
    match kind {
        ENTRY => "Function Start".into(),
        FN_RETURN => "Function End".into(),
        "raycast_all" => "Ray Cast All".into(),
        _ => kind
            .split('_')
            .filter(|w| !w.is_empty())
            .map(|w| {
                let mut c = w.chars();
                c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

#[cfg(test)]
mod kind_title_tests {
    #[test]
    fn kinds_read_as_words() {
        assert_eq!(super::kind_title("entry"), "Function Start");
        assert_eq!(super::kind_title("fn_return"), "Function End");
        assert_eq!(super::kind_title("add_score"), "Add Score");
        assert_eq!(super::kind_title("event"), "Event");
    }
}

#[derive(Clone, Debug)]
pub struct GraphDoc {
    pub path: PathBuf,
    /// Path shown to the user, relative to the project when possible.
    pub label: String,
    pub graph: Graph,
    /// `None` is the event graph. `Some` is the function body on the canvas.
    pub page: Option<String>,
    pub selected: Option<u32>,
    pub pan: [f32; 2],
    pub zoom: f32,
    /// Problems from the last check, with the node they belong to.
    pub problems: Vec<(Option<u32>, String)>,
    pub save_error: Option<String>,
    /// Nodes that stop play before they run; saved in the project's editor folder.
    pub breakpoints: std::collections::BTreeSet<u32>,
    /// What this graph is doing in the running play, refreshed every frame.
    pub debug: Option<seal_world::scripts::DebugView>,
    past: Vec<Graph>,
    future: Vec<Graph>,
    /// Graph before a drag started, so a whole drag is one undo step.
    drag_start: Option<Graph>,
    /// The state canvas is showing instead of the node canvas.
    pub show_machine: bool,
    /// Inspector layout editor instead of the node canvas.
    pub show_inspector_layout: bool,
    /// State or transition selected on the state canvas.
    pub machine_pick: Option<MachineSel>,
}

fn text(s: &str) -> SealValue {
    SealValue::String(s.into())
}

fn number(v: f32) -> SealValue {
    let v = (v * 10.0).round() / 10.0;
    SealValue::Number(format!("{v:.1}"))
}

/// The text the canvas shows for a saved literal.
pub fn literal_text(v: &SealValue) -> String {
    match v {
        SealValue::Null => String::new(),
        SealValue::Bool(b) => b.to_string(),
        SealValue::Number(n) => n.clone(),
        SealValue::String(s) => s.clone(),
        SealValue::Array(items) => items.iter().map(literal_text).collect::<Vec<_>>().join(", "),
        SealValue::Map(_) => v.to_text(),
    }
}

/// Read what the user typed as a literal of `ty`. `None` when it does not fit.
pub fn parse_literal(ty: Ty, input: &str) -> Option<SealValue> {
    let input = input.trim();
    let float = |s: &str| -> Option<SealValue> {
        let v: f64 = s.trim().parse().ok()?;
        v.is_finite().then(|| {
            let t = v.to_string();
            SealValue::Number(if t.contains('.') || t.contains('e') { t } else { format!("{t}.0") })
        })
    };
    match ty {
        Ty::Bool => match input.to_ascii_lowercase().as_str() {
            "true" | "yes" | "1" | "on" => Some(SealValue::Bool(true)),
            "false" | "no" | "0" | "off" | "" => Some(SealValue::Bool(false)),
            _ => None,
        },
        Ty::Int => input.parse::<i64>().ok().map(|v| SealValue::Number(v.to_string())),
        Ty::Float => float(input),
        Ty::Str => Some(text(input)),
        Ty::Vec2 | Ty::Vec3 => {
            let want = if ty == Ty::Vec2 { 2 } else { 3 };
            let parts: Vec<&str> =
                input.split([',', ' ']).filter(|p| !p.trim().is_empty()).collect();
            if parts.len() != want {
                return None;
            }
            parts.iter().map(|p| float(p)).collect::<Option<Vec<_>>>().map(SealValue::Array)
        }
        _ => None,
    }
}

/// Write `contents` to `path` without ever leaving a half-written file.
fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("graph.tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

impl GraphDoc {
    /// Open `path`, creating an empty graph there when the file does not exist yet.
    pub fn open(path: &Path, label: &str) -> Result<Self, String> {
        let graph = if path.exists() {
            let source = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            Graph::from_text(&source).map_err(|e| e.to_string())?
        } else {
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("graph");
            let g = Graph::new(name);
            write_atomic(path, &g.to_text()).map_err(|e| e.to_string())?;
            g
        };
        let mut doc = Self {
            path: path.to_path_buf(),
            label: label.to_string(),
            graph,
            page: None,
            selected: None,
            pan: [0.0, 0.0],
            zoom: 1.0,
            problems: Vec::new(),
            save_error: None,
            breakpoints: Default::default(),
            debug: None,
            past: Vec::new(),
            future: Vec::new(),
            drag_start: None,
            show_machine: false,
            show_inspector_layout: false,
            machine_pick: None,
        };
        doc.breakpoints = doc.read_breakpoints();
        doc.sync_page();
        doc.show_machine = doc.read_machine_view();
        doc.check();
        Ok(doc)
    }

    /// The editor folder of the project this graph belongs to.
    fn breakpoints_file(&self) -> Option<PathBuf> {
        let depth = self.label.split('/').filter(|p| !p.is_empty()).count();
        let root = self.path.ancestors().nth(depth)?;
        Some(root.join(".seal-editor").join("breakpoints.txt"))
    }

    fn read_breakpoints(&self) -> std::collections::BTreeSet<u32> {
        let Some(text) = self.breakpoints_file().and_then(|f| std::fs::read_to_string(f).ok())
        else {
            return Default::default();
        };
        parse_breakpoints(&text).remove(&self.label).unwrap_or_default()
    }

    /// Turn the breakpoint on node `id` on or off and save it.
    pub fn toggle_breakpoint(&mut self, id: u32) {
        if !self.breakpoints.remove(&id) {
            self.breakpoints.insert(id);
        }
        let Some(file) = self.breakpoints_file() else { return };
        let mut all =
            std::fs::read_to_string(&file).map(|t| parse_breakpoints(&t)).unwrap_or_default();
        if self.breakpoints.is_empty() {
            all.remove(&self.label);
        } else {
            all.insert(self.label.clone(), self.breakpoints.clone());
        }
        let text: String = all
            .iter()
            .map(|(g, ids)| {
                format!("{g}\t{}\n", ids.iter().map(u32::to_string).collect::<Vec<_>>().join(","))
            })
            .collect();
        let saved = file
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|_| write_atomic(&file, &text));
        if let Err(e) = saved {
            self.save_error = Some(format!("breakpoints: {e}"));
        }
    }

    /// Check the graph and keep its problems for the canvas.
    pub fn check(&mut self) {
        self.problems = match Program::build(&self.graph) {
            Ok(_) => Vec::new(),
            Err(errors) => errors.into_iter().map(|e| (e.node, e.message)).collect(),
        };
    }

    pub fn save(&mut self) -> Result<(), String> {
        let result = write_atomic(&self.path, &self.graph.to_text()).map_err(|e| e.to_string());
        self.save_error = result.as_ref().err().cloned();
        result
    }

    fn remember(&mut self) {
        self.past.push(self.graph.clone());
        if self.past.len() > UNDO_LIMIT {
            self.past.remove(0);
        }
        self.future.clear();
    }

    /// Keep `page` on a function that still exists.
    fn sync_page(&mut self) {
        let ok = self
            .graph
            .open_function
            .as_ref()
            .is_some_and(|name| self.graph.functions.iter().any(|f| &f.name == name));
        if !ok {
            self.graph.open_function = None;
        }
        self.page = self.graph.open_function.clone();
    }

    /// After every change: check it and write it to disk.
    fn changed(&mut self) {
        self.sync_page();
        self.check();
        // A failed save stays in `save_error`, which the panel shows; the next edit retries.
        if self.save().is_err() {
            self.save_error.get_or_insert_with(|| "save failed".into());
        }
    }

    /// Replace one string variable with a map or struct record. One undo step.
    pub fn set_var_record(&mut self, name: &str, record: &str) -> bool {
        let name = name.to_string();
        let record = record.to_string();
        self.edit(move |graph| crate::pickers::write_graph_var_record(graph, &name, &record))
    }

    fn edit(&mut self, change: impl FnOnce(&mut Graph) -> bool) -> bool {
        let before = self.graph.clone();
        if change(&mut self.graph) && self.graph != before {
            self.past.push(before);
            if self.past.len() > UNDO_LIMIT {
                self.past.remove(0);
            }
            self.future.clear();
            self.changed();
            true
        } else {
            self.graph = before;
            false
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.past.pop() else { return false };
        self.future.push(std::mem::replace(&mut self.graph, prev));
        self.sync_page();
        self.prune_selection();
        self.changed();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.future.pop() else { return false };
        self.past.push(std::mem::replace(&mut self.graph, next));
        self.sync_page();
        self.prune_selection();
        self.changed();
        true
    }

    fn prune_selection(&mut self) {
        if let Some(id) = self.selected {
            if !self.active_nodes().iter().any(|n| n.id == id) {
                self.selected = None;
            }
        }
        let keep = self.graph.machine.as_ref().is_some_and(|m| match self.machine_pick {
            Some(MachineSel::State(i)) => i < m.states.len(),
            Some(MachineSel::Transition(i)) => i < m.transitions.len(),
            None => false,
        });
        if !keep {
            self.machine_pick = None;
        }
    }

    fn active_nodes(&self) -> &[Node] {
        function::page_nodes(&self.graph, self.page.as_deref())
    }

    /// Links of the event graph, or of the function currently open.
    pub fn active_links(&self) -> &[Link] {
        function::page_links(&self.graph, self.page.as_deref())
    }

    pub fn node(&self, id: u32) -> Option<&Node> {
        self.active_nodes().iter().find(|n| n.id == id)
    }

    pub fn pos(&self, id: u32) -> [f32; 2] {
        let read = |v: &SealValue| match v {
            SealValue::Number(n) => n.parse().unwrap_or(0.0),
            _ => 0.0,
        };
        match self.node(id).and_then(|n| n.param("pos")) {
            Some(SealValue::Array(a)) if a.len() == 2 => [read(&a[0]), read(&a[1])],
            _ => {
                let i = self.active_nodes().iter().position(|n| n.id == id).unwrap_or(0) as f32;
                [40.0 + 220.0 * (i % 4.0), 40.0 + 140.0 * (i / 4.0).floor()]
            }
        }
    }

    /// Add a node of `kind` at canvas position `pos`. Returns its id.
    /// `call:<function>` adds a Call node for that function.
    pub fn add_node(&mut self, kind: &str, pos: [f32; 2]) -> Option<u32> {
        let (kind, call_name) = if let Some(name) = kind.strip_prefix("call:") {
            if name.is_empty() {
                return None;
            }
            (CALL, Some(name.to_string()))
        } else {
            (kind, None)
        };
        if call_name.is_none() && !seal_graph::KINDS.iter().any(|(k, _)| *k == kind) {
            return None;
        }
        if self.page.is_none() && matches!(kind, ENTRY | FN_RETURN) {
            return None;
        }
        if self.page.is_some() && matches!(kind, "event" | "return") {
            return None;
        }
        if kind == ENTRY {
            let has_entry = self.page.as_ref().is_some_and(|name| {
                self.graph
                    .functions
                    .iter()
                    .any(|f| &f.name == name && f.nodes.iter().any(|n| n.kind == ENTRY))
            });
            if has_entry {
                return None;
            }
        }
        let id = self.graph.next_id();
        let page = self.page.clone();
        let added = self.edit(|g| {
            let mut node = node_template(g, id, kind);
            if let Some(name) = &call_name {
                node.set("name", text(name));
            }
            if matches!(kind, "get" | "set") {
                if let Some(fname) = &page {
                    if let Some(func) = g.functions.iter_mut().find(|f| &f.name == fname) {
                        if func.locals.is_empty() {
                            func.locals.push(seal_graph::VarDef {
                                name: "value".into(),
                                ty: Ty::Float,
                                default: Value::Float(0.0),
                            });
                        }
                        if let Some(local) = func.locals.first() {
                            let local_name = local.name.clone();
                            node.set("name", text(&local_name));
                        }
                    }
                } else if g.vars.is_empty() {
                    g.vars.push(seal_graph::VarDef {
                        name: "value".into(),
                        ty: Ty::Float,
                        default: Value::Float(0.0),
                    });
                }
            }
            node.set("pos", SealValue::Array(vec![number(pos[0]), number(pos[1])]));
            let Some((nodes, _)) = function::page_parts(g, page.as_deref()) else { return false };
            nodes.push(node);
            true
        });
        if added {
            self.selected = Some(id);
        }
        added.then_some(id)
    }

    /// Start dragging nodes: the whole drag becomes one undo step.
    pub fn begin_drag(&mut self) {
        if self.drag_start.is_none() {
            self.drag_start = Some(self.graph.clone());
        }
    }

    /// Move a node while dragging. Not saved until [`GraphDoc::end_drag`].
    pub fn move_node(&mut self, id: u32, pos: [f32; 2]) {
        let page = self.page.clone();
        let Some((nodes, _)) = function::page_parts(&mut self.graph, page.as_deref()) else {
            return;
        };
        if let Some(n) = nodes.iter_mut().find(|n| n.id == id) {
            n.set("pos", SealValue::Array(vec![number(pos[0]), number(pos[1])]));
        }
    }

    pub fn end_drag(&mut self) {
        if let Some(before) = self.drag_start.take() {
            if before != self.graph {
                let now = std::mem::replace(&mut self.graph, before);
                self.remember();
                self.graph = now;
                self.changed();
            }
        }
    }

    pub fn remove_node(&mut self, id: u32) -> bool {
        let page = self.page.clone();
        let removed = self.edit(|g| {
            let Some((nodes, links)) = function::page_parts(g, page.as_deref()) else {
                return false;
            };
            let before = nodes.len();
            nodes.retain(|n| n.id != id);
            links.retain(|l| l.from.node != id && l.to.node != id);
            nodes.len() != before
        });
        if removed && self.selected == Some(id) {
            self.selected = None;
        }
        removed
    }

    /// The pins of node `id`, or an empty signature when its settings are broken.
    pub fn sig(&self, id: u32) -> Sig {
        node_sig(&self.graph, id).unwrap_or_default()
    }

    /// Link two pins. Either end may be the output; the link is turned the right way.
    /// A data input keeps only its newest link, and so does an execution output.
    pub fn connect(
        &mut self,
        a: (u32, &str, PinSide),
        b: (u32, &str, PinSide),
    ) -> Result<(), String> {
        let ((from, from_pin), (to, to_pin)) = match (a.2, b.2) {
            (PinSide::Out, PinSide::In) => ((a.0, a.1), (b.0, b.1)),
            (PinSide::In, PinSide::Out) => ((b.0, b.1), (a.0, a.1)),
            _ => return Err("link an output to an input".into()),
        };
        if from == to {
            return Err("a node cannot link to itself".into());
        }
        if self.node(from).is_none() || self.node(to).is_none() {
            return Err("unknown pin".into());
        }
        let (fs, ts) = (self.sig(from), self.sig(to));
        let from_exec = fs.exec_outs.iter().any(|p| p == from_pin);
        let to_exec = to_pin == "in" && ts.exec_in;
        if from_exec != to_exec {
            return Err("execution pins link only to execution pins".into());
        }
        if !from_exec {
            let out_ty = fs.outs.iter().find(|(n, _)| n == from_pin).map(|(_, t)| *t);
            let in_ty = ts.ins.iter().find(|(n, _)| n == to_pin).map(|(_, t)| *t);
            match (out_ty, in_ty) {
                (Some(o), Some(i)) if o == i => {}
                (Some(o), Some(i)) => return Err(format!("a {o} output cannot feed a {i} input")),
                _ => return Err("unknown pin".into()),
            }
        }
        let link = Link::new(from, from_pin, to, to_pin);
        let page = self.page.clone();
        self.edit(|g| {
            let Some((_, links)) = function::page_parts(g, page.as_deref()) else { return false };
            if from_exec {
                links.retain(|l| !(l.from.node == from && l.from.pin == from_pin));
            } else {
                links.retain(|l| !(l.to.node == to && l.to.pin == to_pin));
            }
            links.push(link);
            true
        });
        Ok(())
    }

    /// Remove every link on one pin.
    pub fn disconnect(&mut self, node: u32, pin: &str, side: PinSide) -> bool {
        let page = self.page.clone();
        let pin = pin.to_string();
        self.edit(|g| {
            let Some((_, links)) = function::page_parts(g, page.as_deref()) else { return false };
            let before = links.len();
            links.retain(|l| match side {
                PinSide::In => !(l.to.node == node && l.to.pin == pin),
                PinSide::Out => !(l.from.node == node && l.from.pin == pin),
            });
            links.len() != before
        })
    }

    /// Set the default used when data input `pin` is not linked.
    pub fn set_default(&mut self, node: u32, pin: &str, input: &str) -> Result<(), String> {
        let ty = self
            .sig(node)
            .ins
            .iter()
            .find(|(n, _)| n == pin)
            .map(|(_, t)| *t)
            .ok_or_else(|| "unknown pin".to_string())?;
        let value = parse_literal(ty, input).ok_or_else(|| format!("not a {ty}"))?;
        let page = self.page.clone();
        self.edit(|g| {
            let Some((nodes, _)) = function::page_parts(g, page.as_deref()) else { return false };
            let Some(n) = nodes.iter_mut().find(|n| n.id == node) else { return false };
            let mut map = match n.param("defaults") {
                Some(SealValue::Map(m)) => m.clone(),
                _ => seal_sealtext::SealMap::new(Vec::new()),
            };
            match map.entries.iter_mut().find(|(k, _)| k == pin) {
                Some(slot) => slot.1 = value,
                None => map.entries.push((pin.to_string(), value)),
            }
            n.set("defaults", SealValue::Map(map));
            true
        });
        Ok(())
    }

    /// The node settings the side strip edits: everything but `pos`, `defaults` and `id`.
    pub fn settings(&self, id: u32) -> Vec<(String, String)> {
        self.node(id)
            .map(|n| {
                n.params
                    .iter()
                    .filter(|(k, _)| !matches!(k.as_str(), "pos" | "defaults" | "id" | "kind"))
                    .map(|(k, v)| (k.clone(), literal_text(v)))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Change one node setting. Numbers stay numbers, `true`/`false` stay flags.
    pub fn set_setting(&mut self, id: u32, key: &str, input: &str) -> bool {
        let old = self.node(id).and_then(|n| n.param(key)).cloned();
        let value = match old {
            Some(SealValue::Number(_)) => match input.trim().parse::<f64>() {
                Ok(_) => SealValue::Number(input.trim().to_string()),
                Err(_) => return false,
            },
            Some(SealValue::Bool(_)) => SealValue::Bool(input.trim() == "true"),
            // `const` keeps its value in the node's own type.
            _ if key == "value" => {
                let ty = match self.node(id).and_then(|n| n.param("type")) {
                    Some(SealValue::String(t)) => Ty::parse(t),
                    _ => None,
                };
                match ty.and_then(|t| parse_literal(t, input)) {
                    Some(v) => v,
                    None => return false,
                }
            }
            _ => text(input.trim()),
        };
        let page = self.page.clone();
        let changed = self.edit(|g| {
            let Some((nodes, _)) = function::page_parts(g, page.as_deref()) else { return false };
            let Some(n) = nodes.iter_mut().find(|n| n.id == id) else { return false };
            n.set(key, value);
            // Changing a const's type resets its value to that type's zero.
            if key == "type" && n.kind == "const" {
                if let Some(t) = Ty::parse(input.trim()) {
                    n.set("value", seal_graph::value_to_text(&t.zero()));
                }
            }
            true
        });
        changed
    }

    /// Everything the canvas needs to draw.
    pub fn views(&self) -> Vec<NodeView> {
        let nodes = self.active_nodes().to_vec();
        nodes
            .into_iter()
            .map(|n| {
                let sig = self.sig(n.id);
                let linked_in = |pin: &str| {
                    self.active_links().iter().any(|l| l.to.node == n.id && l.to.pin == pin)
                };
                let linked_out = |pin: &str| {
                    self.active_links().iter().any(|l| l.from.node == n.id && l.from.pin == pin)
                };
                let mut ins = Vec::new();
                if sig.exec_in {
                    ins.push(PinView {
                        name: "in".into(),
                        ty: None,
                        side: PinSide::In,
                        linked: linked_in("in"),
                        default_text: None,
                    });
                }
                for (name, ty) in &sig.ins {
                    let linked = linked_in(name);
                    let editable = matches!(
                        ty,
                        Ty::Bool | Ty::Int | Ty::Float | Ty::Str | Ty::Vec2 | Ty::Vec3
                    );
                    let default_text = (!linked && editable).then(|| {
                        n.param("defaults")
                            .and_then(|d| d.get(name))
                            .map(literal_text)
                            .unwrap_or_else(|| literal_text(&seal_graph::value_to_text(&ty.zero())))
                    });
                    ins.push(PinView {
                        name: name.clone(),
                        ty: Some(*ty),
                        side: PinSide::In,
                        linked,
                        default_text,
                    });
                }
                let mut outs: Vec<PinView> = sig
                    .exec_outs
                    .iter()
                    .map(|name| PinView {
                        name: name.clone(),
                        ty: None,
                        side: PinSide::Out,
                        linked: linked_out(name),
                        default_text: None,
                    })
                    .collect();
                for (name, ty) in &sig.outs {
                    outs.push(PinView {
                        name: name.clone(),
                        ty: Some(*ty),
                        side: PinSide::Out,
                        linked: linked_out(name),
                        default_text: None,
                    });
                }
                let detail = match n.param("name").or(n.param("type")).or(n.param("prefab")) {
                    Some(SealValue::String(s)) => format!(" {s}"),
                    _ => String::new(),
                };
                let problem =
                    self.problems.iter().find(|(id, _)| *id == Some(n.id)).map(|(_, m)| m.clone());
                let title = if n.kind == CALL {
                    format!("Call{detail}")
                } else {
                    format!("{}{detail}", kind_title(&n.kind))
                };
                NodeView {
                    id: n.id,
                    kind: n.kind.clone(),
                    title,
                    pos: self.pos(n.id),
                    ins,
                    outs,
                    problem,
                }
            })
            .collect()
    }

    /// Show the event graph. Saved with the graph, without an undo step.
    pub fn show_events(&mut self) {
        if self.page.is_none() && self.graph.open_function.is_none() {
            return;
        }
        self.graph.open_function = None;
        self.page = None;
        self.selected = None;
        let _saved = self.save();
    }

    /// Show one function's body. Saved with the graph, without an undo step.
    pub fn show_function(&mut self, name: &str) {
        if !self.graph.functions.iter().any(|f| f.name == name) {
            return;
        }
        if self.page.as_deref() == Some(name) {
            return;
        }
        self.graph.open_function = Some(name.to_string());
        self.page = Some(name.to_string());
        self.selected = None;
        let _saved = self.save();
    }

    /// Add a function, open it, and remember the step for undo.
    pub fn add_function(&mut self) -> Option<String> {
        let mut created = None;
        let added = self.edit(|g| {
            let name = g.add_function();
            g.open_function = Some(name.clone());
            created = Some(name);
            true
        });
        if added {
            self.page = created.clone();
            self.selected = None;
        }
        created
    }

    pub fn delete_function(&mut self, name: &str) -> bool {
        let name = name.to_string();
        let removed = self.edit(|g| g.remove_function(&name));
        if removed && self.page.as_deref() == Some(name.as_str()) {
            self.page = None;
            self.selected = None;
        }
        removed
    }

    pub fn rename_function(&mut self, from: &str, to: &str) -> Result<(), String> {
        let from = from.to_string();
        let to = to.to_string();
        let mut error = None;
        let changed = self.edit(|g| match g.rename_function(&from, &to) {
            Ok(()) => true,
            Err(message) => {
                error = Some(message);
                false
            }
        });
        if let Some(message) = error {
            return Err(message);
        }
        if changed && self.page.as_deref() == Some(from.as_str()) {
            self.page = Some(to);
        }
        Ok(())
    }

    fn edit_fn(
        &mut self,
        change: impl FnOnce(&mut Graph) -> Result<bool, String>,
    ) -> Result<bool, String> {
        let mut error = None;
        let mut did = false;
        let changed = self.edit(|g| match change(g) {
            Ok(yes) => {
                did = yes;
                yes
            }
            Err(message) => {
                error = Some(message);
                false
            }
        });
        if let Some(message) = error {
            return Err(message);
        }
        Ok(changed && did)
    }

    pub fn add_input(&mut self, func: &str) -> bool {
        let func = func.to_string();
        self.edit(|g| g.add_pin(&func, false))
    }

    pub fn add_output(&mut self, func: &str) -> bool {
        let func = func.to_string();
        self.edit(|g| g.add_pin(&func, true))
    }

    pub fn add_local(&mut self, func: &str) -> bool {
        let func = func.to_string();
        self.edit(|g| g.add_local(&func))
    }

    pub fn remove_input(&mut self, func: &str, index: usize) -> bool {
        let func = func.to_string();
        self.edit(|g| g.remove_pin(&func, false, index))
    }

    pub fn remove_output(&mut self, func: &str, index: usize) -> bool {
        let func = func.to_string();
        self.edit(|g| g.remove_pin(&func, true, index))
    }

    pub fn remove_local(&mut self, func: &str, index: usize) -> bool {
        let func = func.to_string();
        self.edit(|g| g.remove_local(&func, index))
    }

    pub fn set_input_name(&mut self, func: &str, index: usize, name: &str) -> Result<(), String> {
        self.edit_fn(|g| g.rename_pin(func, false, index, name).map(|()| true)).map(|_| ())
    }

    pub fn set_output_name(&mut self, func: &str, index: usize, name: &str) -> Result<(), String> {
        self.edit_fn(|g| g.rename_pin(func, true, index, name).map(|()| true)).map(|_| ())
    }

    pub fn set_input_type(
        &mut self,
        func: &str,
        index: usize,
        ty_name: &str,
    ) -> Result<(), String> {
        self.edit_fn(|g| g.set_pin_type(func, false, index, ty_name).map(|()| true)).map(|_| ())
    }

    pub fn set_output_type(
        &mut self,
        func: &str,
        index: usize,
        ty_name: &str,
    ) -> Result<(), String> {
        self.edit_fn(|g| g.set_pin_type(func, true, index, ty_name).map(|()| true)).map(|_| ())
    }

    pub fn set_local_name(&mut self, func: &str, index: usize, name: &str) -> Result<(), String> {
        self.edit_fn(|g| g.rename_local(func, index, name).map(|()| true)).map(|_| ())
    }

    pub fn set_local_type(
        &mut self,
        func: &str,
        index: usize,
        ty_name: &str,
    ) -> Result<(), String> {
        self.edit_fn(|g| g.set_local_type(func, index, ty_name).map(|()| true)).map(|_| ())
    }

    /// Node search rows for the canvas menu.
    pub fn palette(&self, search: &str) -> Vec<PaletteRow> {
        let needle = search.trim().to_lowercase();
        let on_fn = self.page.is_some();
        let has_entry = self.page.as_ref().is_some_and(|name| {
            self.graph
                .functions
                .iter()
                .any(|f| &f.name == name && f.nodes.iter().any(|n| n.kind == ENTRY))
        });
        let mut rows = Vec::new();
        for (kind, about) in KINDS {
            if *kind == ENTRY && (!on_fn || has_entry) {
                continue;
            }
            if *kind == FN_RETURN && !on_fn {
                continue;
            }
            if *kind == "event" && on_fn {
                continue;
            }
            if *kind == "return" && on_fn {
                continue;
            }
            if !needle.is_empty()
                && !kind.contains(&needle)
                && !kind_title(kind).to_lowercase().contains(&needle)
                && !about.to_lowercase().contains(&needle)
            {
                continue;
            }
            rows.push(PaletteRow {
                kind: (*kind).to_string(),
                label: kind_title(kind),
                about: (*about).to_string(),
            });
        }
        for func in &self.graph.functions {
            let label = format!("Call {}", func.name);
            let about = format!("Call function {}. Pins match its inputs and outputs.", func.name);
            let hay = format!("{label} {about}").to_lowercase();
            if !needle.is_empty() && !hay.contains(&needle) {
                continue;
            }
            rows.push(PaletteRow { kind: format!("call:{}", func.name), label, about });
        }
        rows
    }

    fn view_file(&self) -> Option<PathBuf> {
        let depth = self.label.split('/').filter(|p| !p.is_empty()).count();
        let root = self.path.ancestors().nth(depth)?;
        Some(root.join(".seal-editor").join("graph-view.txt"))
    }

    fn read_machine_view(&self) -> bool {
        let Some(text) = self.view_file().and_then(|f| std::fs::read_to_string(f).ok()) else {
            return false;
        };
        text.lines().any(|line| line.trim() == self.label)
    }

    fn write_machine_view(&mut self) {
        let Some(file) = self.view_file() else { return };
        let mut lines: Vec<String> = std::fs::read_to_string(&file)
            .map(|t| t.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
            .unwrap_or_default();
        lines.retain(|l| l != &self.label);
        if self.show_machine {
            lines.push(self.label.clone());
        }
        lines.sort();
        let text = if lines.is_empty() { String::new() } else { format!("{}\n", lines.join("\n")) };
        let saved = file
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|_| write_text_atomic(&file, &text));
        if let Err(e) = saved {
            self.save_error = Some(format!("state view: {e}"));
        }
    }

    /// Switch between the node canvas and the state machine canvas. The choice is kept.
    pub fn toggle_machine_view(&mut self) {
        self.show_machine = !self.show_machine;
        if self.show_machine {
            self.show_inspector_layout = false;
        }
        self.write_machine_view();
    }

    /// Open the inspector layout editor for this graph.
    pub fn toggle_inspector_layout_view(&mut self) {
        self.show_inspector_layout = !self.show_inspector_layout;
        if self.show_inspector_layout {
            self.show_machine = false;
        }
    }

    /// Ensure a section heading exists (creates layout storage when missing).
    pub fn ensure_inspector_section(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let added = self.edit(|g| {
            if g.inspector.is_none() {
                g.inspector = Some(seal_graph::inspector_layout::InspectorLayout::default());
            }
            let layout = g.inspector.as_mut().expect("just inserted");
            if layout.sections.iter().any(|s| s.name == name) {
                return false;
            }
            let order = layout.sections.len() as i32;
            layout.sections.push(seal_graph::inspector_layout::InspectorSection {
                name: name.to_string(),
                order,
            });
            true
        });
        added.then_some(()).is_some()
    }

    /// Store or replace one field rule in the layout and save the graph.
    pub fn apply_inspector_rule(
        &mut self,
        rule: seal_graph::inspector_layout::InspectorFieldRule,
    ) -> bool {
        if rule.field.trim().is_empty() {
            return false;
        }
        let applied = self.edit(|g| {
            if g.inspector.is_none() {
                g.inspector = Some(seal_graph::inspector_layout::InspectorLayout::default());
            }
            let layout = g.inspector.as_mut().expect("just inserted");
            if let Some(slot) = layout.fields.iter_mut().find(|f| f.field == rule.field) {
                *slot = rule.clone();
            } else {
                layout.fields.push(rule.clone());
            }
            true
        });
        applied.then_some(()).is_some()
    }

    /// Add a state at `pos` and select it. The first state is the initial one.
    pub fn add_state(&mut self, pos: [f32; 2]) -> Option<usize> {
        let name = fresh_state_name(&self.graph);
        let index = self.graph.machine.as_ref().map(|m| m.states.len()).unwrap_or(0);
        let added = self.edit(|g| {
            if g.machine.is_none() {
                g.machine = Some(state_machine::Machine {
                    initial: name.clone(),
                    states: Vec::new(),
                    transitions: Vec::new(),
                });
            }
            let machine = g.machine.as_mut().expect("just inserted");
            machine.states.push(StateDef { name: name.clone(), pos });
            ensure_state_events(g, &name, pos);
            true
        });
        if added {
            self.machine_pick = Some(MachineSel::State(index));
        }
        added.then_some(index)
    }

    /// Rename a state, its transitions and its three events.
    pub fn rename_state(&mut self, index: usize, name: &str) -> Result<(), String> {
        let name = name.trim();
        if !state_machine::plain_name(name) {
            return Err("state name must be a plain name".into());
        }
        let old = self
            .graph
            .machine
            .as_ref()
            .and_then(|m| m.states.get(index).map(|s| s.name.clone()))
            .ok_or_else(|| "no state".to_string())?;
        if old == name {
            return Ok(());
        }
        if self.graph.machine.as_ref().is_some_and(|m| m.states.iter().any(|s| s.name == name)) {
            return Err(format!("state {name} already exists"));
        }
        for phase in state_machine::PHASES {
            let next = state_machine::phase_event(phase, name);
            let prev = state_machine::phase_event(phase, &old);
            let clash = self.graph.nodes.iter().any(|n| {
                n.kind == "event"
                    && n.param("name").and_then(|v| match v {
                        SealValue::String(s) => Some(s.as_str()),
                        _ => None,
                    }) == Some(next.as_str())
                    && next != prev
            });
            if clash {
                return Err(format!("an event named {next} already exists"));
            }
        }
        self.edit(|g| {
            let Some(machine) = g.machine.as_mut() else { return false };
            let Some(state) = machine.states.get_mut(index) else { return false };
            state.name = name.to_string();
            if machine.initial == old {
                machine.initial = name.to_string();
            }
            for t in &mut machine.transitions {
                if t.from == old {
                    t.from = name.to_string();
                }
                if t.to == old {
                    t.to = name.to_string();
                }
            }
            for phase in state_machine::PHASES {
                let prev = state_machine::phase_event(phase, &old);
                let next = state_machine::phase_event(phase, name);
                for node in &mut g.nodes {
                    if node.kind == "event"
                        && node.param("name") == Some(&SealValue::String(prev.clone()))
                    {
                        node.set("name", text(&next));
                    }
                }
            }
            true
        });
        Ok(())
    }

    /// Remove a state, the transitions that touch it, and its three events.
    pub fn delete_state(&mut self, index: usize) -> bool {
        let removed = self.edit(|g| {
            let name = {
                let Some(machine) = g.machine.as_mut() else { return false };
                if index >= machine.states.len() {
                    return false;
                }
                let name = machine.states.remove(index).name;
                machine.transitions.retain(|t| t.from != name && t.to != name);
                if !machine.states.is_empty()
                    && !machine.states.iter().any(|s| s.name == machine.initial)
                {
                    machine.initial = machine.states[0].name.clone();
                }
                name
            };
            if g.machine.as_ref().is_some_and(|m| m.states.is_empty()) {
                g.machine = None;
            }
            for phase in state_machine::PHASES {
                let event = state_machine::phase_event(phase, &name);
                let ids: Vec<u32> = g
                    .nodes
                    .iter()
                    .filter(|n| {
                        n.kind == "event"
                            && n.param("name") == Some(&SealValue::String(event.clone()))
                    })
                    .map(|n| n.id)
                    .collect();
                for id in ids {
                    g.nodes.retain(|n| n.id != id);
                    g.links.retain(|l| l.from.node != id && l.to.node != id);
                }
            }
            true
        });
        if removed {
            self.prune_selection();
        }
        removed
    }

    /// Make this state the one entered on the first step.
    pub fn set_initial_state(&mut self, index: usize) -> bool {
        self.edit(|g| {
            let Some(machine) = g.machine.as_mut() else { return false };
            let Some(name) = machine.states.get(index).map(|s| s.name.clone()) else {
                return false;
            };
            if machine.initial == name {
                return false;
            }
            machine.initial = name;
            true
        })
    }

    /// Move a state while dragging. Saved when the drag ends.
    pub fn move_state(&mut self, index: usize, pos: [f32; 2]) {
        if let Some(state) = self.graph.machine.as_mut().and_then(|m| m.states.get_mut(index)) {
            state.pos = pos;
        }
    }

    /// Add a transition. It waits one second, at priority 0, until the form changes it.
    pub fn add_transition(&mut self, from: usize, to: usize) -> Option<usize> {
        let index = self.graph.machine.as_ref()?.transitions.len();
        let added = self.edit(|g| {
            let Some(machine) = g.machine.as_mut() else { return false };
            if from >= machine.states.len() || to >= machine.states.len() {
                return false;
            }
            let from_name = machine.states[from].name.clone();
            let to_name = machine.states[to].name.clone();
            machine.transitions.push(Transition {
                from: from_name,
                to: to_name,
                priority: 0,
                cond: Cond::After(1.0),
            });
            true
        });
        if added {
            self.machine_pick = Some(MachineSel::Transition(index));
        }
        added.then_some(index)
    }

    pub fn delete_transition(&mut self, index: usize) -> bool {
        let removed = self.edit(|g| {
            let Some(machine) = g.machine.as_mut() else { return false };
            if index >= machine.transitions.len() {
                return false;
            }
            machine.transitions.remove(index);
            true
        });
        if removed {
            self.prune_selection();
        }
        removed
    }

    pub fn set_transition_priority(&mut self, index: usize, priority: i32) -> bool {
        self.edit(|g| {
            let Some(t) = g.machine.as_mut().and_then(|m| m.transitions.get_mut(index)) else {
                return false;
            };
            if t.priority == priority {
                return false;
            }
            t.priority = priority;
            true
        })
    }

    pub fn set_transition_to(&mut self, index: usize, to: usize) -> bool {
        self.edit(|g| {
            let Some(machine) = g.machine.as_mut() else { return false };
            let Some(name) = machine.states.get(to).map(|s| s.name.clone()) else {
                return false;
            };
            let Some(t) = machine.transitions.get_mut(index) else { return false };
            if t.to == name {
                return false;
            }
            t.to = name;
            true
        })
    }

    /// The transition waits until the flag `var` is true. A new name becomes a bool variable.
    pub fn set_cond_flag(&mut self, index: usize, var: &str) -> Result<(), String> {
        let var = var.trim();
        if !state_machine::plain_name(var) {
            return Err("variable name must be a plain name".into());
        }
        if let Some(found) = self.graph.vars.iter().find(|v| v.name == var) {
            if found.ty != Ty::Bool {
                return Err(format!("{var} is not a flag"));
            }
        }
        self.edit(|g| {
            if !g.vars.iter().any(|v| v.name == var) {
                g.vars.push(seal_graph::VarDef {
                    name: var.to_string(),
                    ty: Ty::Bool,
                    default: Value::Bool(false),
                });
            }
            let Some(t) = g.machine.as_mut().and_then(|m| m.transitions.get_mut(index)) else {
                return false;
            };
            t.cond = Cond::BoolVar(var.to_string());
            true
        });
        Ok(())
    }

    /// Compare `var` to `literal`. A new name becomes a float variable.
    pub fn set_cond_compare(
        &mut self,
        index: usize,
        var: &str,
        op: &str,
        literal: &str,
    ) -> Result<(), String> {
        let var = var.trim();
        if !state_machine::plain_name(var) {
            return Err("variable name must be a plain name".into());
        }
        let op = CmpOp::parse(op).ok_or_else(|| format!("unknown compare {op}"))?;
        let existing = self.graph.vars.iter().find(|v| v.name == var).map(|v| v.ty);
        let ty = existing.unwrap_or(Ty::Float);
        if matches!(op, CmpOp::Lt | CmpOp::Le | CmpOp::Gt | CmpOp::Ge)
            && !matches!(ty, Ty::Int | Ty::Float)
        {
            return Err("ordered compare works on int or float".into());
        }
        if ty.is_list() {
            return Err("compare works on plain values".into());
        }
        let value = parse_literal(ty, literal)
            .and_then(|raw| seal_graph::value_from_text(ty, &raw))
            .ok_or_else(|| format!("not a {ty}"))?;
        self.edit(|g| {
            if !g.vars.iter().any(|v| v.name == var) {
                g.vars.push(seal_graph::VarDef { name: var.to_string(), ty, default: ty.zero() });
            }
            let Some(t) = g.machine.as_mut().and_then(|m| m.transitions.get_mut(index)) else {
                return false;
            };
            t.cond = Cond::Compare { var: var.to_string(), op, value };
            true
        });
        Ok(())
    }

    pub fn set_cond_event(&mut self, index: usize, event: &str) -> Result<(), String> {
        let event = event.trim();
        if event.is_empty() || event.chars().any(char::is_whitespace) {
            return Err("event name cannot be empty or contain spaces".into());
        }
        self.edit(|g| {
            let Some(t) = g.machine.as_mut().and_then(|m| m.transitions.get_mut(index)) else {
                return false;
            };
            t.cond = Cond::Event(event.to_string());
            true
        });
        Ok(())
    }

    pub fn set_cond_after(&mut self, index: usize, seconds: f64) -> Result<(), String> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err("seconds must be zero or more".into());
        }
        self.edit(|g| {
            let Some(t) = g.machine.as_mut().and_then(|m| m.transitions.get_mut(index)) else {
                return false;
            };
            t.cond = Cond::After(seconds);
            true
        });
        Ok(())
    }

    /// Short text drawn on a transition arrow.
    pub fn transition_label(&self, index: usize) -> String {
        let Some(t) = self.graph.machine.as_ref().and_then(|m| m.transitions.get(index)) else {
            return String::new();
        };
        match &t.cond {
            Cond::BoolVar(var) => var.clone(),
            Cond::Compare { var, op, value } => format!("{var} {} {value}", op_symbol(*op)),
            Cond::Event(name) => name.clone(),
            Cond::After(seconds) => format!("{seconds}s"),
        }
    }

    /// The compare literal as the form's text field shows it.
    pub fn compare_literal(&self, index: usize) -> String {
        match self.graph.machine.as_ref().and_then(|m| m.transitions.get(index)) {
            Some(Transition { cond: Cond::Compare { value, .. }, .. }) => {
                literal_text(&seal_graph::value_to_text(value))
            }
            _ => String::new(),
        }
    }

    /// Select the event for this phase and show the node canvas. Creates the event if it was removed.
    pub fn focus_state_event(&mut self, index: usize, phase: &str) -> bool {
        if !state_machine::PHASES.contains(&phase) {
            return false;
        }
        let Some(state_name) =
            self.graph.machine.as_ref().and_then(|m| m.states.get(index).map(|s| s.name.clone()))
        else {
            return false;
        };
        let event = state_machine::phase_event(phase, &state_name);
        let existing = self.graph.nodes.iter().find_map(|n| {
            (n.kind == "event" && n.param("name") == Some(&SealValue::String(event.clone())))
                .then_some(n.id)
        });
        let id = if let Some(id) = existing {
            id
        } else {
            let pos = self
                .graph
                .machine
                .as_ref()
                .and_then(|m| m.states.get(index).map(|s| s.pos))
                .unwrap_or([40.0, 40.0]);
            let new_id = self.graph.next_id();
            let added = self.edit(|g| {
                let mut node = Node::new(new_id, "event").with("name", text(&event));
                node.set("pos", SealValue::Array(vec![number(pos[0] + 220.0), number(pos[1])]));
                g.nodes.push(node);
                true
            });
            if !added {
                return false;
            }
            new_id
        };
        self.selected = Some(id);
        if self.show_machine {
            self.show_machine = false;
            self.write_machine_view();
        }
        true
    }
}

fn write_text_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

fn fresh_state_name(graph: &Graph) -> String {
    let taken =
        |n: &str| graph.machine.as_ref().is_some_and(|m| m.states.iter().any(|s| s.name == n));
    if !taken("state") {
        return "state".into();
    }
    let mut i = 2u32;
    loop {
        let name = format!("state{i}");
        if !taken(&name) {
            return name;
        }
        i += 1;
    }
}

fn ensure_state_events(graph: &mut Graph, state: &str, pos: [f32; 2]) {
    for (i, phase) in state_machine::PHASES.iter().enumerate() {
        let name = state_machine::phase_event(phase, state);
        let exists = graph.nodes.iter().any(|n| {
            n.kind == "event" && n.param("name") == Some(&SealValue::String(name.clone()))
        });
        if exists {
            continue;
        }
        let id = graph.next_id();
        let mut node = Node::new(id, "event").with("name", text(&name));
        node.set(
            "pos",
            SealValue::Array(vec![number(pos[0] + 220.0), number(pos[1] + 90.0 * i as f32)]),
        );
        graph.nodes.push(node);
    }
}

fn op_symbol(op: CmpOp) -> &'static str {
    match op {
        CmpOp::Lt => "<",
        CmpOp::Le => "<=",
        CmpOp::Gt => ">",
        CmpOp::Ge => ">=",
        CmpOp::Eq => "=",
        CmpOp::Ne => "!=",
    }
}

/// One row in the canvas node search.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteRow {
    pub kind: String,
    pub label: String,
    pub about: String,
}

// ---- Editor commands -------------------------------------------------------

use crate::command::CommandArgs;
use crate::error::CoreError;
use crate::session::EditorSession;

/// Where the editor remembers the open graph, inside the project.
fn open_graph_note(root: &Path) -> PathBuf {
    root.join(".seal-editor").join("open-graph.txt")
}

fn project_root(session: &EditorSession) -> Result<PathBuf, CoreError> {
    session
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .ok_or_else(|| CoreError::new("open a project first"))
}

/// Breakpoints file: one line per graph, its path, a tab, then node ids.
pub fn parse_breakpoints(
    text: &str,
) -> std::collections::BTreeMap<String, std::collections::BTreeSet<u32>> {
    text.lines()
        .filter_map(|line| {
            let (graph, ids) = line.split_once('\t')?;
            let ids: std::collections::BTreeSet<u32> =
                ids.split(',').filter_map(|i| i.trim().parse().ok()).collect();
            (!ids.is_empty()).then(|| (graph.trim().to_string(), ids))
        })
        .collect()
}

/// A file name made from an object name: letters, digits and underscores.
pub fn graph_file_stem(name: &str) -> String {
    let mut out: String = name
        .trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    let out = out.trim_matches('_').to_string();
    if out.is_empty() {
        "graph".into()
    } else {
        out
    }
}

/// Open the graph at project-relative `rel` in the Graph panel, creating it if needed.
pub fn open_graph(session: &mut EditorSession, rel: &str) -> Result<(), CoreError> {
    let root = project_root(session)?;
    let rel = rel.trim().replace('\\', "/");
    if rel.is_empty() || rel.starts_with('/') || rel.split('/').any(|part| part == "..") {
        return Err(CoreError::new(format!("'{rel}' is not a path inside the project")));
    }
    let doc = GraphDoc::open(&root.join(&rel), &rel).map_err(CoreError::new)?;
    session.graph = Some(doc);
    session.graph_restore_tried = true;
    // Remembering is a convenience; the graph itself is already safe on disk.
    let note = open_graph_note(&root);
    if let Some(dir) = note.parent() {
        if std::fs::create_dir_all(dir).and_then(|_| std::fs::write(&note, &rel)).is_err() {
            seal_log::log(
                seal_log::Level::Warn,
                "graph",
                "could not remember the open graph".to_string(),
            );
        }
    }
    if !session.foundation.open_panels.iter().any(|p| p == "graph") {
        session.foundation.open_panels.push("graph".into());
    }
    session.reveal_panel = Some("graph".into());
    session.foundation.focused_panel_id = "graph".into();
    Ok(())
}

/// Reopen the graph that was open when the editor last closed, once per project.
pub fn restore_graph(session: &mut EditorSession) {
    if session.graph.is_some() || session.graph_restore_tried {
        return;
    }
    session.graph_restore_tried = true;
    let Ok(root) = project_root(session) else { return };
    let Ok(rel) = std::fs::read_to_string(open_graph_note(&root)) else { return };
    let rel = rel.trim().to_string();
    if root.join(&rel).exists() {
        let panel = session.reveal_panel.clone();
        let focus = session.foundation.focused_panel_id.clone();
        if open_graph(session, &rel).is_ok() {
            // Restoring must not steal focus from what the user was doing.
            session.reveal_panel = panel;
            session.foundation.focused_panel_id = focus;
        }
    }
}

/// Give object `id` the script `rel` (one undo step), when it has none yet.
fn attach_script(session: &mut EditorSession, id: &str, rel: &str) {
    if let Some(previous) = session.models.set_field(id, "script", rel) {
        session.push_past(crate::models::Past::Field {
            id: id.to_string(),
            field: "script".into(),
            previous,
            next: rel.to_string(),
        });
        session.mirror_node(id);
    }
}

fn node_script(session: &EditorSession, id: &str) -> Option<(String, String)> {
    let node = session.models.nodes.iter().find(|n| n.id == id)?;
    let script = node.fields.iter().find(|(k, _)| k == "script").map(|(_, v)| v.trim().to_string());
    Some((node.name.clone(), script.unwrap_or_default()))
}

/// `graph.new`: a new graph under `logic/`. With an object selected that has no
/// script yet, it becomes that object's script.
pub(crate) fn cmd_graph_new(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let root = project_root(session)?;
    let target = args.get("entity").map(str::to_string).or_else(|| session.primary_selection());
    let owner =
        target.as_deref().and_then(|id| node_script(session, id).map(|s| (id.to_string(), s)));
    let stem = args
        .get("name")
        .map(graph_file_stem)
        .or_else(|| owner.as_ref().map(|(_, (name, _))| graph_file_stem(name)))
        .unwrap_or_else(|| "new_graph".into());
    let rel =
        (1..)
            .map(|i| {
                if i == 1 {
                    format!("logic/{stem}.graph")
                } else {
                    format!("logic/{stem}_{i}.graph")
                }
            })
            .find(|rel| !root.join(rel).exists())
            .unwrap_or_default();
    open_graph(session, &rel)?;
    if let Some((id, (_, script))) = owner {
        if script.is_empty() {
            attach_script(session, &id, &rel);
        }
    }
    Ok(())
}

/// `graph.open`: open `path`, else the selected object's script (making one if it has none).
pub(crate) fn cmd_graph_open(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    if let Some(path) = args.get("path").filter(|p| !p.trim().is_empty()) {
        return open_graph(session, path);
    }
    let target = args.get("entity").map(str::to_string).or_else(|| session.primary_selection());
    match target.as_deref().and_then(|id| node_script(session, id)) {
        Some((_, script)) if !script.is_empty() => open_graph(session, &script),
        Some(_) => cmd_graph_new(session, args),
        None => match session.graph.as_ref().map(|d| d.label.clone()) {
            Some(label) => open_graph(session, &label),
            None => cmd_graph_new(session, args),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seal_graph::GraphInstance;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("seal-graphdoc-{}-{name}", std::process::id()));
        let _ignored = std::fs::remove_dir_all(&dir);
        dir.join("logic").join("hello.graph")
    }

    #[test]
    fn building_hello_by_hand_saves_a_graph_that_runs() {
        let path = temp("hello");
        let mut doc = GraphDoc::open(&path, "logic/hello.graph").expect("open");
        assert!(path.exists(), "a new graph is written at once");
        let start = doc.add_node("event", [0.0, 0.0]).expect("event");
        let log = doc.add_node("log", [240.0, 0.0]).expect("log");
        doc.connect((log, "in", PinSide::In), (start, "then", PinSide::Out)).expect("link");
        doc.set_default(log, "message", "Hello from Seal").expect("default");
        assert!(doc.problems.is_empty(), "{:?}", doc.problems);
        let saved =
            Graph::from_text(&std::fs::read_to_string(&path).expect("read")).expect("parse");
        assert_eq!(saved.links.len(), 1);
        let program = Program::build(&saved).expect("builds");
        let mut host = seal_graph::MapHost::default();
        let mut run = seal_graph::Interp::new(program, 0);
        run.fire("start", &[], &mut host).expect("runs");
        assert!(host.logs.iter().any(|l| l.contains("Hello from Seal")), "{:?}", host.logs);
        // Reopening shows the same graph, positions included.
        let again = GraphDoc::open(&path, "x").expect("reopen");
        assert_eq!(again.pos(log), [240.0, 0.0]);
    }

    #[test]
    fn breakpoints_are_saved_per_graph_and_come_back_on_reopen() {
        let path = temp("breaks");
        let mut doc = GraphDoc::open(&path, "logic/hello.graph").expect("open");
        let start = doc.add_node("event", [0.0, 0.0]).expect("event");
        let log = doc.add_node("log", [240.0, 0.0]).expect("log");
        doc.toggle_breakpoint(log);
        doc.toggle_breakpoint(start);
        doc.toggle_breakpoint(start);
        assert_eq!(doc.breakpoints.iter().copied().collect::<Vec<_>>(), vec![log]);
        let file = path.parent().unwrap().parent().unwrap().join(".seal-editor/breakpoints.txt");
        let text = std::fs::read_to_string(&file).expect("saved at once");
        assert_eq!(text, format!("logic/hello.graph\t{log}\n"));
        // Another graph in the same project keeps its own line.
        let other = path.with_file_name("other.graph");
        let mut second = GraphDoc::open(&other, "logic/other.graph").expect("open other");
        let n = second.add_node("event", [0.0, 0.0]).expect("event");
        second.toggle_breakpoint(n);
        let again = GraphDoc::open(&path, "logic/hello.graph").expect("reopen");
        assert_eq!(again.breakpoints, doc.breakpoints);
        assert_eq!(parse_breakpoints(&std::fs::read_to_string(&file).unwrap()).len(), 2);
        // Clearing the last one drops the line.
        doc.toggle_breakpoint(log);
        assert!(!std::fs::read_to_string(&file).unwrap().contains("hello"));
    }

    #[test]
    fn links_are_type_checked_and_inputs_keep_one_link() {
        let mut doc = GraphDoc::open(&temp("types"), "t").expect("open");
        let a = doc.add_node("const", [0.0, 0.0]).expect("const");
        let b = doc.add_node("const", [0.0, 80.0]).expect("const");
        let log = doc.add_node("log", [200.0, 0.0]).expect("log");
        assert!(doc.connect((a, "out", PinSide::Out), (log, "message", PinSide::In)).is_err());
        let add = doc.add_node("add", [200.0, 80.0]).expect("add");
        doc.connect((a, "out", PinSide::Out), (add, "a", PinSide::In)).expect("a");
        doc.connect((b, "out", PinSide::Out), (add, "a", PinSide::In)).expect("replace");
        assert_eq!(doc.graph.links.len(), 1);
        assert_eq!(doc.graph.links[0].from.node, b);
        assert!(doc.connect((add, "out", PinSide::Out), (log, "in", PinSide::In)).is_err());
    }

    #[test]
    fn undo_redo_and_drag_are_single_steps_and_saved() {
        let path = temp("undo");
        let mut doc = GraphDoc::open(&path, "u").expect("open");
        let id = doc.add_node("log", [0.0, 0.0]).expect("log");
        doc.begin_drag();
        doc.move_node(id, [10.0, 10.0]);
        doc.move_node(id, [50.0, 30.0]);
        doc.end_drag();
        assert_eq!(doc.pos(id), [50.0, 30.0]);
        assert!(doc.undo());
        assert_eq!(doc.pos(id), [0.0, 0.0]);
        assert!(doc.undo());
        assert!(doc.graph.nodes.is_empty());
        let on_disk =
            Graph::from_text(&std::fs::read_to_string(&path).expect("read")).expect("parse");
        assert!(on_disk.nodes.is_empty(), "undo is saved too");
        assert!(doc.redo());
        assert_eq!(doc.graph.nodes.len(), 1);
        assert!(doc.remove_node(id));
        assert!(doc.graph.nodes.is_empty());
    }

    #[test]
    fn settings_edit_keeps_types() {
        let mut doc = GraphDoc::open(&temp("settings"), "s").expect("open");
        let c = doc.add_node("const", [0.0, 0.0]).expect("const");
        assert!(doc.set_setting(c, "type", "int"));
        assert!(!doc.set_setting(c, "value", "abc"));
        assert!(doc.set_setting(c, "value", "7"));
        assert!(doc.problems.is_empty(), "{:?}", doc.problems);
        let seq = doc.add_node("sequence", [0.0, 0.0]).expect("seq");
        assert!(doc.set_setting(seq, "count", "3"));
        assert_eq!(doc.sig(seq).exec_outs.len(), 3);
        assert_eq!(
            parse_literal(Ty::Vec2, "1, 2"),
            Some(SealValue::Array(vec![
                SealValue::Number("1.0".into()),
                SealValue::Number("2.0".into())
            ]))
        );
    }

    #[test]
    fn new_graph_attaches_to_the_object_and_reopens_after_restart() {
        use crate::command::CommandArgs;
        use crate::layout::Layout;
        use crate::session::EditorSession;
        let dir = std::env::temp_dir().join(format!("seal-graphcmd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let project = crate::project::create_project(&dir.join("demo"), "Demo").unwrap();
        let root = project.root.clone();
        let mut session = EditorSession::new(Layout::default());
        session.project = Some(project.clone());
        let id = session.models.nodes.iter().find(|n| n.id != "root").unwrap().id.clone();
        session.models.set_field(&id, "script", "");
        let past = session.past.len();
        session.dispatch("graph.new", &CommandArgs::new(&[("entity", id.as_str())])).unwrap();
        let rel = session.graph.as_ref().unwrap().label.clone();
        assert!(rel.starts_with("logic/") && rel.ends_with(".graph"), "{rel}");
        assert!(root.join(&rel).exists());
        let script = session.models.nodes.iter().find(|n| n.id == id).unwrap();
        assert!(script.fields.iter().any(|(k, v)| k == "script" && *v == rel));
        assert_eq!(session.past.len(), past + 1, "attaching is one undo step");
        assert_eq!(session.foundation.focused_panel_id, "graph");
        // Edit, then "restart": the graph comes back with the edit.
        let added = session.graph.as_mut().unwrap().add_node("log", [10.0, 20.0]).unwrap();
        let mut again = EditorSession::new(Layout::default());
        again.project = Some(project);
        restore_graph(&mut again);
        let doc = again.graph.as_ref().expect("graph reopened");
        assert_eq!(doc.label, rel);
        assert_eq!(doc.pos(added), [10.0, 20.0]);
        // Undo and Delete act on the graph while its panel has focus, not on the scene.
        again.foundation.focused_panel_id = "graph".into();
        let nodes = again.models.nodes.len();
        again.graph.as_mut().unwrap().selected = Some(added);
        again.dispatch("edit.delete", &CommandArgs::default()).unwrap();
        assert!(again.graph.as_ref().unwrap().node(added).is_none());
        assert_eq!(again.models.nodes.len(), nodes);
        // A second new graph for the same object gets its own file and keeps the first script.
        session.dispatch("graph.new", &CommandArgs::new(&[("entity", id.as_str())])).unwrap();
        assert_ne!(session.graph.as_ref().unwrap().label, rel);
        let script = session.models.nodes.iter().find(|n| n.id == id).unwrap();
        assert!(script.fields.iter().any(|(k, v)| k == "script" && *v == rel));
        // Paths outside the project are refused.
        assert!(open_graph(&mut session, "../x.graph").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn state_machine_edits_undo_save_and_reopen() {
        let path = temp("machine");
        let mut doc = GraphDoc::open(&path, "logic/hello.graph").expect("open");
        doc.add_state([10.0, 20.0]).expect("first");
        doc.add_state([200.0, 20.0]).expect("second");
        assert_eq!(doc.graph.machine.as_ref().unwrap().initial, "state");
        doc.rename_state(1, "run").expect("rename");
        doc.add_transition(0, 1).expect("transition");
        assert!(doc.set_transition_priority(0, 4));
        doc.set_cond_flag(0, "go").expect("flag");
        doc.set_initial_state(1);
        doc.toggle_machine_view();
        assert!(doc.show_machine);
        let again = GraphDoc::open(&path, "logic/hello.graph").expect("reopen");
        assert!(again.show_machine);
        let machine = again.graph.machine.as_ref().unwrap();
        assert_eq!(machine.states[1].name, "run");
        assert_eq!(machine.initial, "run");
        assert_eq!(machine.transitions[0].priority, 4);
        assert!(matches!(machine.transitions[0].cond, Cond::BoolVar(ref name) if name == "go"));
        assert!(again
            .graph
            .nodes
            .iter()
            .any(|n| { n.param("name") == Some(&SealValue::String("on_enter_run".into())) }));
        assert!(again.graph.vars.iter().any(|v| v.name == "go" && v.ty == Ty::Bool));
        assert!(doc.undo());
        assert_eq!(doc.graph.machine.as_ref().unwrap().initial, "state");
        assert!(doc.redo());
        assert_eq!(doc.graph.machine.as_ref().unwrap().initial, "run");
        assert!(doc.delete_state(0));
        let left = doc.graph.machine.as_ref().unwrap();
        assert_eq!(left.states.len(), 1);
        assert_eq!(left.initial, "run");
        assert!(left.transitions.is_empty());
    }
}
