//! Display name, tag checks and velocity read the object's own fields.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value, V3};
use seal_migrate::{import, Options};

#[test]
fn name_tag_and_velocity_come_from_the_object() {
    let imp = import(
        include_str!("fixtures/fields.txt"),
        &Options { name: "fields".into(), ..Options::default() },
    )
    .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let mut vm =
        Interp::new(Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}")), 1);
    let mut h = MapHost::default();
    let f = |c: &str, n: &str| (1, c.to_string(), n.to_string());
    h.fields.insert(f("editor", "name"), Value::Str("Bat".into()));
    h.fields.insert(f("editor", "tag"), Value::Str("enemy".into()));
    let v = V3 { x: 1.0, y: -2.0, z: 0.5 };
    h.fields.insert(f("body", "velocity"), Value::Vec3(v));
    vm.fire("Show", &[], &mut h).unwrap();
    h.fields.insert(f("editor", "tag"), Value::Str("friend".into()));
    vm.fire("Show", &[], &mut h).unwrap();
    assert_eq!(h.logs, ["Battrue", "Batfalse"]);
    assert_eq!(h.fields.get(&f("transform", "translation")), Some(&Value::Vec3(v)));
}

#[test]
fn a_self_node_is_this_object() {
    let imp = import(
        include_str!("fixtures/self.txt"),
        &Options { name: "self".into(), ..Options::default() },
    )
    .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let mut vm =
        Interp::new(Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}")), 7);
    let mut h = MapHost::default();
    h.fields.insert((7, "editor".into(), "name".into()), Value::Str("Hero".into()));
    vm.fire("Me", &[], &mut h).unwrap();
    assert_eq!(h.logs, ["Hero"]);
}
