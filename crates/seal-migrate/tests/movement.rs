//! Movement input: an axis event and an axis read push the object along
//! their directions at the walking speed, and Jump sets the upward speed.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value, V3};
use seal_migrate::{import, Options, JUMP_SPEED, MOVE_SPEED};

#[test]
fn movement_input_moves_and_jump_pushes_up() {
    let imp = import(
        include_str!("fixtures/movement.txt"),
        &Options { name: "mv".into(), ..Options::default() },
    )
    .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let mut vm =
        Interp::new(Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}")), 1);
    let mut h = MapHost { alive: [1].into(), ..MapHost::default() };
    h.axes.insert("MoveForward".into(), 1.0);
    h.axes.insert("MoveRight".into(), -0.5);
    let at = |x, y, z| Value::Vec3(V3 { x, y, z });
    h.fields.insert((1, "transform".into(), "translation".into()), at(0.0, 0.0, 10.0));
    h.fields.insert((1, "body".into(), "velocity".into()), at(3.0, 4.0, 0.0));
    vm.advance(0.5, &mut h).unwrap();
    let pos = h.fields[&(1, "transform".into(), "translation".into())].clone();
    assert_eq!(pos, at(0.5 * MOVE_SPEED, -0.25 * MOVE_SPEED, 10.0));
    vm.fire("Hop", &[], &mut h).unwrap();
    assert_eq!(h.fields[&(1, "body".into(), "velocity".into())], at(3.0, 4.0, JUMP_SPEED));
    assert_eq!(h.logs, ["hopped"]);
}
