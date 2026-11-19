//! Get World Delta Seconds and Get Game Time In Seconds.

use seal_graph::{GraphInstance, Interp, MapHost, Program};
use seal_migrate::{import, Options};

const TIME: &str = include_str!("fixtures/time.txt");

fn program() -> Program {
    let imp = import(TIME, &Options { name: "time".into(), ..Options::default() })
        .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}"))
}

#[test]
fn time_and_frame_length_follow_the_frames() {
    let mut vm = Interp::new(program(), 1);
    let mut h = MapHost::default();
    vm.fire("Show", &[], &mut h).unwrap();
    vm.advance(0.25, &mut h).unwrap();
    vm.advance(0.5, &mut h).unwrap();
    vm.fire("Show", &[], &mut h).unwrap();
    assert_eq!(h.logs, ["0.0", "0.0", "0.75", "0.5"]);
}

#[test]
fn time_compiles_to_the_same_rust() {
    let code = seal_graph::codegen::compile_all(&[program()]);
    assert!(
        code.contains("self.dt") && code.contains("self.time"),
        "both clocks reach the generated code"
    );
}
