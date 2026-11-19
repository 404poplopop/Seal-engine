//! Graph functions: calls, locals, recursion, text, and a signature change.

use seal_graph::rt::{GraphInstance, MapHost, RunState, Value};
use seal_graph::{Graph, Interp, Link, Node, Program};
use seal_sealtext::SealValue;

fn s(v: &str) -> SealValue {
    SealValue::String(v.into())
}

fn run_logs(text: &str) -> Vec<String> {
    let graph = Graph::from_text(text).unwrap_or_else(|e| panic!("{e}"));
    let program = Program::build(&graph).unwrap_or_else(|errs| {
        let list: Vec<_> = errs.iter().map(|e| e.to_string()).collect();
        panic!("{}", list.join("\n"));
    });
    let mut host = MapHost::default();
    let state = Interp::new(program, 1).fire("start", &[], &mut host).unwrap();
    assert_eq!(state, RunState::Done);
    host.logs
}

#[test]
fn calls_nest_and_locals_do_not_leak() {
    let logs = run_logs(include_str!("../../seal-graph-tests/graphs/fnbody.graph"));
    // wrap(3) = bump(3) + 7 = 10, wrap(4) = 11, bump(5) = 5.
    // A leaked local would make the second wrap 14 and the last bump 9.
    assert_eq!(logs, ["10", "11", "5"]);
}

#[test]
fn a_graph_variable_survives_a_call_and_a_local_does_not() {
    let mut g = Graph::new("keep").var("total", Value::Int(0));
    g.nodes.push(Node::new(1, "event").with("name", s("start")));
    g.nodes.push(Node::new(2, "sequence").with("count", SealValue::Number("2".into())));
    g.add_function();
    let entry = g.functions[0].nodes[0].id;
    let ret = g.functions[0].nodes[1].id;
    let base = g.next_id();
    let func = &mut g.functions[0];
    func.locals.push(seal_graph::VarDef {
        name: "seen".into(),
        ty: seal_graph::Ty::Int,
        default: Value::Int(0),
    });
    // seen = seen + 1; total = total + 1; return seen. Each call logs seen.
    func.nodes.push(Node::new(base, "get").with("name", s("seen")));
    func.nodes.push(
        Node::new(base + 1, "add")
            .with("type", s("int"))
            .with("defaults", seal_sealtext::parse("b = 1").unwrap()),
    );
    func.nodes.push(Node::new(base + 2, "set").with("name", s("seen")));
    func.nodes.push(Node::new(base + 3, "get").with("name", s("total")));
    func.nodes.push(
        Node::new(base + 4, "add")
            .with("type", s("int"))
            .with("defaults", seal_sealtext::parse("b = 1").unwrap()),
    );
    func.nodes.push(Node::new(base + 5, "set").with("name", s("total")));
    func.links.extend([
        Link::new(entry, "then", base + 2, "in"),
        Link::new(base, "out", base + 1, "a"),
        Link::new(base + 1, "out", base + 2, "value"),
        Link::new(base + 2, "then", base + 5, "in"),
        Link::new(base + 3, "out", base + 4, "a"),
        Link::new(base + 4, "out", base + 5, "value"),
        Link::new(base + 5, "then", ret, "in"),
        Link::new(base + 2, "value", ret, "result"),
    ]);
    let call = g.next_id();
    g.nodes.push(Node::new(call, "call_fn").with("name", s("function")));
    g.nodes.push(Node::new(call + 1, "to_str").with("type", s("int")));
    g.nodes.push(Node::new(call + 2, "log"));
    g.nodes.push(Node::new(call + 3, "call_fn").with("name", s("function")));
    g.nodes.push(Node::new(call + 4, "to_str").with("type", s("int")));
    g.nodes.push(Node::new(call + 5, "log"));
    g.links.extend([
        Link::new(1, "then", 2, "in"),
        Link::new(2, "then0", call, "in"),
        Link::new(call, "then", call + 2, "in"),
        Link::new(call, "result", call + 1, "a"),
        Link::new(call + 1, "out", call + 2, "message"),
        Link::new(2, "then1", call + 3, "in"),
        Link::new(call + 3, "then", call + 5, "in"),
        Link::new(call + 3, "result", call + 4, "a"),
        Link::new(call + 4, "out", call + 5, "message"),
    ]);
    let program = Program::build(&g).unwrap_or_else(|e| panic!("{e:?}"));
    let mut host = MapHost::default();
    let mut interp = Interp::new(program, 1);
    interp.fire("start", &[], &mut host).unwrap();
    assert_eq!(host.logs, ["1", "1"], "a local must start over on the second call");
    assert_eq!(interp.var("total"), Some(&Value::Int(2)));
}

#[test]
fn recursion_is_rejected() {
    let mut g = Graph::new("rec");
    g.add_function();
    let entry = g.functions[0].nodes[0].id;
    let id = g.next_id();
    g.functions[0].nodes.push(Node::new(id, "call_fn").with("name", s("function")));
    g.functions[0].links.push(Link::new(entry, "then", id, "in"));
    let errs = Program::build(&g).unwrap_err();
    let text = errs.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n");
    assert!(text.contains("recursion"), "{text}");

    let mut g = Graph::new("mutual");
    g.add_function();
    g.add_function();
    g.rename_function("function", "alpha").unwrap();
    g.rename_function("function2", "beta").unwrap();
    let a_entry = g.functions[0].nodes[0].id;
    let b_entry = g.functions[1].nodes[0].id;
    let id = g.next_id();
    g.functions[0].nodes.push(Node::new(id, "call_fn").with("name", s("beta")));
    g.functions[0].links.push(Link::new(a_entry, "then", id, "in"));
    g.functions[1].nodes.push(Node::new(id + 1, "call_fn").with("name", s("alpha")));
    g.functions[1].links.push(Link::new(b_entry, "then", id + 1, "in"));
    let errs = Program::build(&g).unwrap_err();
    assert!(errs.iter().any(|e| e.message.contains("recursion")), "{errs:?}");
}

#[test]
fn renaming_a_pin_relinks_and_a_new_type_reports_the_broken_link() {
    let mut g = Graph::new("sig");
    g.add_function();
    let entry = g.functions[0].nodes[0].id;
    let ret = g.functions[0].nodes[1].id;
    g.functions[0].links.push(Link::new(entry, "value", ret, "result"));
    let call = g.next_id();
    g.nodes.push(Node::new(call, "call_fn").with("name", s("function")));
    let shown = g.next_id();
    g.nodes.push(Node::new(shown, "to_str").with("type", s("int")));
    g.links.push(Link::new(call, "result", shown, "a"));
    g.rename_pin("function", false, 0, "n").unwrap();
    g.rename_pin("function", true, 0, "total").unwrap();
    assert!(g.functions[0].links.iter().any(|l| l.from.pin == "n" && l.to.pin == "total"));
    assert!(g.links.iter().any(|l| l.from.pin == "total"), "{:?}", g.links);
    g.rename_function("function", "double").unwrap();
    assert_eq!(g.nodes[0].param("name"), Some(&s("double")));
    assert!(Program::build(&g).is_ok(), "{:?}", Program::build(&g).err());
    g.set_pin_type("double", true, 0, "float").unwrap();
    let errs = Program::build(&g).unwrap_err();
    assert!(errs.iter().any(|e| e.message.contains("into")), "{errs:?}");
}

#[test]
fn text_round_trips_and_old_graphs_stay_plain() {
    let mut g = Graph::new("round");
    g.add_function();
    g.open_function = Some("function".into());
    let text = g.to_text();
    let back = Graph::from_text(&text).unwrap();
    assert_eq!(back, g);
    assert_eq!(back.to_text(), text);
    assert_eq!(back.open_function.as_deref(), Some("function"));

    let old = "schema_version = 1\nseal_graph = \"old\"\nvars = []\nnodes = []\nlinks = []\n";
    let loaded = Graph::from_text(old).unwrap();
    assert!(loaded.functions.is_empty());
    assert!(loaded.open_function.is_none());
    assert!(!loaded.to_text().contains("functions"));
    assert!(Program::build(&loaded).is_ok());
}

#[test]
fn damaged_function_text_is_refused() {
    let base = "schema_version = 1\nseal_graph = \"bad\"\nvars = []\nnodes = []\nlinks = []\n";
    for extra in [
        "functions = 1\n",
        "functions = [\"nope\"]\n",
        "functions = [{ name = \"A\" }]\n",
        "functions = [{ name = \"a\", inputs = [{ name = \"n\", type = \"nope\" }] }]\n",
        "functions = [{ name = \"a\", nodes = 1 }]\n",
        "functions = [{ name = \"a\", links = [\"not a link\"] }]\n",
        "functions = [{ name = \"a\", locals = [{ name = \"t\", type = \"int\", default = \"x\" }] }]\n",
        "open_function = 4\n",
        "open_function = \"1bad\"\n",
    ] {
        let err = Graph::from_text(&format!("{base}{extra}")).expect_err(extra);
        assert!(!err.message.is_empty(), "{extra}");
    }
}
