//! Damaged graph and replay files must be refused, not crash the loader.

use seal_graph::replay::Recording;
use seal_graph::rt::Value;
use seal_graph::{Graph, Node};
use seal_sealtext::SealValue;
use seal_testkit::garbage::{survives, text};

fn s(v: &str) -> SealValue {
    SealValue::String(v.into())
}
fn n(v: i64) -> SealValue {
    SealValue::Number(v.to_string())
}

/// start -> sequence(3): set a = 1, set a = a + 10 (with a breakpoint), log a.
fn sample() -> Graph {
    Graph::new("sample")
        .var("a", Value::Int(0))
        .node(Node::new(1, "event").with("name", s("start")))
        .node(Node::new(2, "sequence").with("count", n(3)))
        .node(
            Node::new(3, "set")
                .with("name", s("a"))
                .with("defaults", seal_sealtext::parse("value = 1").unwrap()),
        )
        .node(Node::new(4, "get").with("name", s("a")))
        .node(
            Node::new(5, "add")
                .with("type", s("int"))
                .with("defaults", seal_sealtext::parse("b = 10").unwrap()),
        )
        .node(Node::new(6, "set").with("name", s("a")))
        .node(Node::new(7, "to_str").with("type", s("int")))
        .node(Node::new(8, "log"))
        .link(1, "then", 2, "in")
        .link(2, "then0", 3, "in")
        .link(2, "then1", 6, "in")
        .link(2, "then2", 8, "in")
        .link(4, "out", 5, "a")
        .link(5, "out", 6, "value")
        .link(4, "out", 7, "a")
        .link(7, "out", 8, "message")
}

#[test]
fn broken_graph_files_are_refused_not_crashed_on() {
    survives(
        "graph",
        sample().to_text().as_bytes(),
        text(|t| {
            if let Ok(graph) = Graph::from_text(t) {
                let _ = seal_graph::Program::build(&graph);
            }
        }),
    );
}

#[test]
fn broken_replays_are_refused_not_crashed_on() {
    let mut recording = Recording::new("sample").with_players(&[1, 2]);
    recording.inputs.push(seal_graph::replay::Input::Advance(1.0 / 60.0));
    recording.inputs.push(seal_graph::replay::Input::Advance(0.5));
    survives("replay", recording.to_text().as_bytes(), text(|t| drop(Recording::from_text(t))));
}
