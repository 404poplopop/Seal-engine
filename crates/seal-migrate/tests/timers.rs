//! Set Timer by Function Name and Clear Timer by Function Name.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value};
use seal_migrate::{import, Options};

const TIMERS: &str = include_str!("fixtures/timers.txt");
const BAD: &str = include_str!("fixtures/timers_bad.txt");

fn run() -> (Interp, MapHost) {
    let imp = import(TIMERS, &Options { name: "timers".into(), ..Options::default() })
        .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let program = Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}"));
    (Interp::new(program, 1), MapHost::default())
}

fn ticks(vm: &mut Interp, h: &mut MapHost, dt: f64) -> usize {
    let before = h.logs.len();
    vm.advance(dt, h).unwrap();
    h.logs[before..].iter().filter(|l| *l == "tick").count()
}

#[test]
fn a_one_shot_timer_fires_once() {
    let (mut vm, mut h) = run();
    vm.fire("Start", &[Value::Float(1.0), Value::Bool(false)], &mut h).unwrap();
    assert_eq!(ticks(&mut vm, &mut h, 0.5), 0);
    assert_eq!(ticks(&mut vm, &mut h, 0.6), 1);
    assert_eq!(ticks(&mut vm, &mut h, 5.0), 0);
}

#[test]
fn a_looping_timer_catches_up_and_clears() {
    let (mut vm, mut h) = run();
    vm.fire("Start", &[Value::Float(0.5), Value::Bool(true)], &mut h).unwrap();
    assert_eq!(ticks(&mut vm, &mut h, 2.1), 4);
    vm.fire("Start", &[Value::Float(1.0), Value::Bool(true)], &mut h).unwrap();
    assert_eq!(ticks(&mut vm, &mut h, 0.9), 0, "setting again restarts the timer");
    assert_eq!(ticks(&mut vm, &mut h, 0.2), 1);
    vm.fire("Stop", &[], &mut h).unwrap();
    assert_eq!(ticks(&mut vm, &mut h, 10.0), 0);
}

#[test]
fn timers_seal_cannot_run_are_reported() {
    let imp =
        import(BAD, &Options { name: "bad".into(), ..Options::default() }).expect("fixture parses");
    let text = imp.report.to_text();
    assert_eq!(imp.report.unsupported(), 3, "{text}");
    for why in ["takes inputs", "InitialStartDelay is not zero", "not a custom event"] {
        assert!(text.contains(why), "missing {why:?} in\n{text}");
    }
}
