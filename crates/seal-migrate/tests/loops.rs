//! Loops that end early: For Loop with Break and While Loop.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value};
use seal_migrate::{import, Options};

const LOOPS: &str = include_str!("fixtures/loops.txt");

fn run() -> (Interp, MapHost) {
    let imp = import(LOOPS, &Options { name: "loops".into(), ..Options::default() })
        .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let program = Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}"));
    (Interp::new(program, 1), MapHost::default())
}

fn fire(vm: &mut Interp, host: &mut MapHost, event: &str, args: &[Value]) -> Vec<String> {
    let before = host.logs.len();
    vm.fire(event, args, host).unwrap();
    host.logs[before..].to_vec()
}

#[test]
fn break_leaves_the_loop_then_completes() {
    let (mut vm, mut h) = run();
    assert_eq!(fire(&mut vm, &mut h, "FindFirst", &[Value::Int(10)]), ["4", "done"]);
    assert_eq!(fire(&mut vm, &mut h, "FindFirst", &[Value::Int(0)]), ["0", "done"]);
    // Never breaks: runs every pass, then completes.
    assert_eq!(fire(&mut vm, &mut h, "FindFirst", &[Value::Int(1000)]), ["done"]);
    // A break in an earlier run does not stop the next one.
    assert_eq!(fire(&mut vm, &mut h, "FindFirst", &[Value::Int(50)]), ["8", "done"]);
}

#[test]
fn while_runs_until_its_condition_fails() {
    let (mut vm, mut h) = run();
    assert_eq!(fire(&mut vm, &mut h, "Countdown", &[Value::Int(3)]), ["3", "2", "1", "liftoff"]);
    assert_eq!(fire(&mut vm, &mut h, "Countdown", &[Value::Int(0)]), ["liftoff"]);
}
