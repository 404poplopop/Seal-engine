//! Timelines: Play, Reverse From End, Stop, Set New Time and Play From
//! Start drive the time; Update fires each moving frame and Finished once
//! at an end. A looping autoplay timeline wraps and shares the tick and
//! begin-play events with the script's own.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value, V3};
use seal_migrate::{import, Options};

fn program() -> (Program, String) {
    let imp = import(
        include_str!("fixtures/timeline.txt"),
        &Options { name: "timeline".into(), ..Options::default() },
    )
    .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    (Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}")), text)
}

fn var(vm: &Interp, name: &str) -> Value {
    vm.vars().into_iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("no {name}")).1
}

fn close(v: Value, want: f64) {
    match v {
        Value::Float(f) => assert!((f - want).abs() < 1e-9, "{f} != {want}"),
        v => panic!("{v:?} is not a float"),
    }
}

#[test]
fn timeline_plays_reverses_stops_and_finishes() {
    let (p, text) = program();
    assert!(text.contains("ramp from 0 to 1"), "{text}");
    let mut vm = Interp::new(p, 1);
    let mut h = MapHost::default();
    vm.fire("start", &[], &mut h).unwrap();
    assert_eq!(h.logs, ["begin"]);
    vm.advance(1.0, &mut h).unwrap();
    close(var(&vm, "Level"), 0.0);
    close(var(&vm, "Ticks"), 1.0);
    assert_eq!(var(&vm, "Where"), Value::Vec3(V3 { x: 0.2, y: 0.2, z: 0.2 }));
    vm.fire("Go", &[], &mut h).unwrap();
    vm.advance(1.0, &mut h).unwrap();
    close(var(&vm, "Level"), 0.2);
    assert_eq!(var(&vm, "Way"), Value::Str("Forward".into()));
    vm.advance(1.0, &mut h).unwrap();
    close(var(&vm, "Level"), 0.4);
    vm.advance(3.5, &mut h).unwrap();
    close(var(&vm, "Level"), 1.0);
    assert_eq!(h.logs, ["begin", "done"]);
    vm.advance(1.0, &mut h).unwrap();
    assert_eq!(h.logs, ["begin", "done"], "finished fires once");
    vm.fire("Back", &[], &mut h).unwrap();
    vm.advance(1.0, &mut h).unwrap();
    close(var(&vm, "Level"), 0.8);
    assert_eq!(var(&vm, "Way"), Value::Str("Backward".into()));
    vm.fire("Jump", &[Value::Float(-3.0)], &mut h).unwrap();
    vm.fire("Halt", &[], &mut h).unwrap();
    vm.advance(1.0, &mut h).unwrap();
    close(var(&vm, "Level"), 0.8);
    vm.fire("Again", &[], &mut h).unwrap();
    vm.advance(0.5, &mut h).unwrap();
    close(var(&vm, "Level"), 0.1);
    close(var(&vm, "Ticks"), 8.0);
    // The looping track: 10 s in, it has just wrapped back to the start.
    match var(&vm, "Where") {
        Value::Vec3(v) => assert!(v.x.abs() < 1e-9 && v.x == v.y && v.y == v.z, "{v:?}"),
        v => panic!("{v:?}"),
    }
    assert_eq!(h.logs, ["begin", "done"], "a looping timeline never finishes");
}

#[test]
fn timeline_compiles_to_rust() {
    let (p, _) = program();
    let code = seal_graph::codegen::compile_all(&[p]);
    assert!(code.contains("self.dt"), "timelines step by the frame length");
}
