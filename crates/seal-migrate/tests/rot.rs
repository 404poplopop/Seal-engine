//! Actor rotation and scale, and rotators made and broken in a graph.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value, V3};
use seal_migrate::{import, Options};

const ROT: &str = include_str!("fixtures/rot.txt");
const ME: u64 = 1;

fn v(x: f64, y: f64, z: f64) -> Value {
    Value::Vec3(V3 { x, y, z })
}

#[test]
fn set_and_read_rotation_and_scale() {
    let imp =
        import(ROT, &Options { name: "rot".into(), ..Options::default() }).expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let program = Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}"));
    let mut vm = Interp::new(program, ME);
    let mut h = MapHost::default().with(ME, "transform", "rotation", v(0.0, 0.0, 0.0)).with(
        ME,
        "transform",
        "scale",
        v(1.0, 1.0, 1.0),
    );
    vm.fire("Turn", &[Value::Float(45.0)], &mut h).unwrap();
    // Pitch, yaw, roll, then the scale's Z.
    assert_eq!(h.logs, ["10", "45", "-5", "4"]);
    let key = |f: &str| (ME, "transform".to_string(), f.to_string());
    assert_eq!(h.fields[&key("rotation")], v(10.0, 45.0, -5.0));
    assert_eq!(h.fields[&key("scale")], v(2.0, 3.0, 4.0));
}
