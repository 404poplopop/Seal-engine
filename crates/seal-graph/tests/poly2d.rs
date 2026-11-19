//! Polygon nodes: the interpreter and the compiled call agree on an offset square.

use seal_graph::rt::{GraphInstance, MapHost, RunState};
use seal_graph::{codegen, Graph, Interp, Program};

fn run(text: &str) -> Vec<String> {
    let graph = Graph::from_text(text).unwrap_or_else(|e| panic!("{e}"));
    let program = Program::build(&graph).unwrap_or_else(|errs| {
        panic!("{}", errs.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n"));
    });
    let compiled = codegen::compile(&program);
    assert!(compiled.contains("offset_values"), "{compiled}");
    let mut host = MapHost::default();
    let state = Interp::new(program, 1).fire("start", &[], &mut host).unwrap();
    assert_eq!(state, RunState::Done);
    host.logs
}

#[test]
fn offset_polygon_node_moves_the_first_corner() {
    let logs = run(
        r#"
schema_version = 1
seal_graph = "poly"
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "const", type = "list<vec2>", value = [[0, 0], [1, 0], [1, 1], [0, 1]] },
  { id = 3, kind = "offset_polygon", defaults = { distance = 0.5 } },
  { id = 4, kind = "list_get", type = "vec2", defaults = { index = 0 } },
  { id = 5, kind = "vec2_x" },
  { id = 6, kind = "float_text" },
  { id = 7, kind = "log" },
]
links = [
  "1.then -> 7.in",
  "2.out -> 3.points",
  "3.out -> 4.list",
  "4.out -> 5.v",
  "5.out -> 6.a",
  "6.out -> 7.message",
]
"#,
    );
    assert_eq!(logs, ["-0.5"]);
}
