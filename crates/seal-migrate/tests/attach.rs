//! Attaching and detaching: the location rule picks the Seal field, the
//! parent reads back, and attaching at a socket is reported.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value};
use seal_migrate::{import, Options};

#[test]
fn attach_rules_parent_and_detach_map_to_attach_fields() {
    let imp = import(
        include_str!("fixtures/attach.txt"),
        &Options { name: "attach".into(), ..Options::default() },
    )
    .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let mut vm =
        Interp::new(Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}")), 1);
    let mut h = MapHost { alive: [1, 2].into(), ..MapHost::default() };
    // The map host keeps what was written; seed the parent read-back.
    h.fields.insert((1, "attach".into(), "parent".into()), Value::Entity(2));
    vm.fire("Ride", &[Value::Entity(2)], &mut h).unwrap();
    assert_eq!(h.fields[&(1, "attach".into(), "keep_world".into())], Value::Entity(2));
    vm.fire("Snap", &[Value::Entity(2)], &mut h).unwrap();
    assert_eq!(h.fields[&(1, "attach".into(), "snap".into())], Value::Entity(2));
    vm.fire("GetOff", &[], &mut h).unwrap();
    assert!(h.fields.contains_key(&(1, "attach".into(), "detach".into())));
    assert_eq!(h.logs, ["true"]);
}

#[test]
fn attaching_at_a_socket_is_reported() {
    let imp = import(
        include_str!("fixtures/attach_socket.txt"),
        &Options { name: "sock".into(), ..Options::default() },
    )
    .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(!imp.report.complete(), "{text}");
    assert!(text.contains("socket"), "{text}");
}
