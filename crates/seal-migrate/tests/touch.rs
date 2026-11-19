//! Overlap, hit and destroyed events become Seal's touch events.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value, V3};
use seal_migrate::{import, Options};

#[test]
fn touch_events_carry_the_other_object() {
    let imp = import(
        include_str!("fixtures/touch.txt"),
        &Options { name: "touch".into(), ..Options::default() },
    )
    .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let program = Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}"));
    let names: Vec<&str> = program.events.iter().map(|e| e.name.as_str()).collect();
    for want in ["overlap_begin", "overlap_end", "hit", "destroyed"] {
        assert!(names.contains(&want), "{names:?}");
    }
    let mut vm = Interp::new(program, 1);
    let mut h = MapHost { alive: [1, 5].into(), ..MapHost::default() };
    vm.fire("overlap_begin", &[Value::Entity(5)], &mut h).unwrap();
    assert!(!h.alive.contains(&5) && h.alive.contains(&1));
    vm.fire("overlap_end", &[Value::Entity(5)], &mut h).unwrap();
    vm.fire(
        "hit",
        &[
            Value::Entity(5),
            Value::Vec3(V3 { x: 0.0, y: 0.0, z: 0.0 }),
            Value::Vec3(V3 { x: 0.0, y: 0.0, z: 0.0 }),
        ],
        &mut h,
    )
    .unwrap();
    vm.fire("destroyed", &[], &mut h).unwrap();
    assert_eq!(h.logs, ["end", "hit", "gone"]);
}

#[test]
fn hit_details_are_reported_not_dropped() {
    let imp = import(
        include_str!("fixtures/touch_bad.txt"),
        &Options { name: "touch_bad".into(), ..Options::default() },
    )
    .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(!imp.report.complete(), "{text}");
    assert!(text.contains("NormalImpulse"), "{text}");
}

#[test]
fn hits_carry_location_and_normal() {
    let imp = import(
        include_str!("fixtures/touch_hit.txt"),
        &Options { name: "touch_hit".into(), ..Options::default() },
    )
    .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let mut vm =
        Interp::new(Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}")), 1);
    let mut h = MapHost::default();
    let at = Value::Vec3(V3 { x: 1.0, y: 2.0, z: 3.0 });
    let n = Value::Vec3(V3 { x: 0.0, y: 0.0, z: 1.0 });
    vm.fire("hit", &[Value::Entity(5), at, n], &mut h).unwrap();
    assert_eq!(h.logs, ["X=1.000 Y=2.000 Z=3.000", "X=0.000 Y=0.000 Z=1.000"]);
}
