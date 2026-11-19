//! Script core graph nodes.

use seal_graph::rt::{Host, Ty, Value};
use seal_graph::{Graph, Interp, Node, Program};

struct Stub;

impl Host for Stub {
    fn get(&mut self, _: u64, _: &str, _: &str, _: Ty) -> Result<Value, String> {
        Ok(Value::Int(0))
    }
    fn set(&mut self, _: u64, _: &str, _: &str, _: Value) -> Result<(), String> {
        Ok(())
    }
    fn log(&mut self, _: &str) {}
    fn unscaled_time(&mut self) -> f64 {
        3.5
    }
    fn sim_tick(&mut self) -> i64 {
        42
    }
}

#[test]
fn quat_euler_and_clocks() {
    let g = Graph::new("t")
        .node(
            Node::new(0, "quat_euler").with(
                "defaults",
                seal_sealtext::parse("pitch = 0\nyaw = 90\nroll = 0").unwrap(),
            ),
        )
        .node(Node::new(1, "unscaled_time"))
        .node(Node::new(2, "sim_tick"));
    let prog = Program::build(&g).unwrap();
    let mut i = Interp::new(prog, 1);
    let mut host = Stub;
    let q = i.eval_pure(0, &mut host).unwrap().quat();
    assert!(q.w.abs() > 0.5 && q.y.abs() > 0.5);
    assert_eq!(i.eval_pure(1, &mut host).unwrap().float(), 3.5);
    assert_eq!(i.eval_pure(2, &mut host).unwrap().int(), 42);
}

#[test]
fn utf8_to_utf16_units() {
    let g = Graph::new("t").node(
        Node::new(0, "utf8_to_utf16").with("defaults", seal_sealtext::parse("text = \"Hi\"").unwrap()),
    );
    let prog = Program::build(&g).unwrap();
    let mut i = Interp::new(prog, 1);
    let mut host = Stub;
    let enc = i.eval_pure(0, &mut host).unwrap().str();
    assert_eq!(enc, "72,105");
}