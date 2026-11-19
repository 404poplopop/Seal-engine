//! Timers set by event, through a custom event's own delegate or a Create
//! Event node, with handles kept in variables and cleared by handle.

use seal_graph::{GraphInstance, Interp, MapHost, Program};
use seal_migrate::{import, Options};

#[test]
fn timers_by_event_clear_by_handle() {
    let imp = import(
        include_str!("fixtures/timer_handles.txt"),
        &Options { name: "th".into(), ..Options::default() },
    )
    .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let mut vm =
        Interp::new(Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}")), 1);
    let mut h = MapHost::default();
    vm.fire("StartA", &[], &mut h).unwrap();
    vm.advance(1.0, &mut h).unwrap();
    vm.advance(1.0, &mut h).unwrap();
    vm.fire("StartB", &[], &mut h).unwrap();
    vm.advance(0.5, &mut h).unwrap();
    vm.fire("StopA", &[], &mut h).unwrap();
    vm.advance(2.0, &mut h).unwrap();
    vm.fire("StartB", &[], &mut h).unwrap();
    vm.fire("StopB", &[], &mut h).unwrap();
    vm.advance(1.0, &mut h).unwrap();
    assert_eq!(h.logs, ["tick", "tick", "beat", ""]);
}
