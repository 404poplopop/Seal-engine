//! A function body agrees on the interpreter and on compiled Rust.

use seal_graph::rt::{GraphInstance, MapHost, RunState};
use seal_graph::{Graph, Interp, Program};

const ME: u64 = 9;

#[test]
fn function_calls_match_across_backends() {
    let src = seal_graph_tests::SOURCES.iter().find(|(n, _)| *n == "fnbody").unwrap().1;
    let program = Program::build(&Graph::from_text(src).unwrap()).unwrap();
    let mut interp = Interp::new(program, ME);
    let mut compiled = seal_graph_tests::create("fnbody", ME).unwrap();
    let mut left = MapHost::default();
    let mut right = MapHost::default();
    let a = interp.fire("start", &[], &mut left).unwrap();
    let b = compiled.fire("start", &[], &mut right).unwrap();
    assert_eq!(a, RunState::Done);
    assert_eq!(a, b);
    assert_eq!(left.logs, ["10", "11", "5"]);
    assert_eq!(left.logs, right.logs);
    assert_eq!(interp.vars(), compiled.vars());
}
