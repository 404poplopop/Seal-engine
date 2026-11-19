//! Spawn transforms: the new entity starts at the transform's location,
//! rotation and scale.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value, V3};
use seal_migrate::{import, Options};

const AT: &str = include_str!("fixtures/spawn_at.txt");
const BAD: &str = include_str!("fixtures/spawn_at_bad.txt");

fn run() -> (Interp, MapHost) {
    let imp =
        import(AT, &Options { name: "at".into(), ..Options::default() }).expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let program = Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}"));
    let zero = Value::Vec3(V3 { x: 0.0, y: 0.0, z: 0.0 });
    let host = MapHost::default().prefab("BP_Coin", &[("transform", "translation", zero)]);
    (Interp::new(program, 1), host)
}

fn newest(h: &MapHost) -> Value {
    field(h, "translation")
}

fn field(h: &MapHost, f: &str) -> Value {
    let e = *h.alive.iter().next_back().expect("something spawned");
    h.fields[&(e, "transform".to_string(), f.to_string())].clone()
}

#[test]
fn spawn_at_a_literal_transform() {
    let (mut vm, mut h) = run();
    vm.fire("Place", &[], &mut h).unwrap();
    assert_eq!(newest(&h), Value::Vec3(V3 { x: 5.0, y: 6.0, z: 7.0 }));
}

#[test]
fn spawn_at_a_made_transform() {
    let (mut vm, mut h) = run();
    vm.fire("Put", &[Value::Float(-3.0)], &mut h).unwrap();
    assert_eq!(newest(&h), Value::Vec3(V3 { x: -3.0, y: 2.5, z: -1.0 }));
    vm.fire("Put", &[Value::Float(8.0)], &mut h).unwrap();
    assert_eq!(h.alive.len(), 2);
    assert_eq!(newest(&h), Value::Vec3(V3 { x: 8.0, y: 2.5, z: -1.0 }));
}

#[test]
fn spawn_with_a_literal_rotation_and_scale() {
    let (mut vm, mut h) = run();
    vm.fire("Tilt", &[], &mut h).unwrap();
    assert_eq!(newest(&h), Value::Vec3(V3 { x: 1.0, y: 2.0, z: 3.0 }));
    assert_eq!(field(&h, "rotation"), Value::Vec3(V3 { x: 0.0, y: 90.0, z: 0.0 }));
    assert_eq!(field(&h, "scale"), Value::Vec3(V3 { x: 2.0, y: 2.0, z: 2.0 }));
}

#[test]
fn a_made_transform_with_scale_is_reported() {
    let imp =
        import(BAD, &Options { name: "bad".into(), ..Options::default() }).expect("fixture parses");
    let text = imp.report.to_text();
    assert_eq!(imp.report.unsupported(), 1, "{text}");
    assert!(text.contains("transform Scale is not mapped yet"), "{text}");
}
