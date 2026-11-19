//! Make Struct and Break Struct on vectors, rotations and 2D vectors.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value};
use seal_migrate::{import, Options, Status};

#[test]
fn made_structs_break_back_into_their_fields() {
    let imp = import(
        include_str!("fixtures/structs.txt"),
        &Options { name: "structs".into(), ..Options::default() },
    )
    .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let mut vm =
        Interp::new(Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}")), 1);
    let mut h = MapHost::default();
    vm.fire("Go", &[Value::Float(2.0), Value::Float(0.5)], &mut h).unwrap();
    // A + 5 + 90 + B + A
    assert_eq!(vm.var("Sum"), Some(&Value::Float(99.5)));
}

#[test]
fn other_structs_are_reported() {
    let imp = import(include_str!("fixtures/structs_color.txt"), &Options::default())
        .expect("fixture parses");
    let r = &imp.report.nodes[0];
    assert!(
        matches!(&r.status, Status::Unsupported(why) if why.contains("LinearColor")),
        "{:?}",
        r.status
    );
}
