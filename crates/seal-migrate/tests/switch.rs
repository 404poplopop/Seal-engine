//! Switch on Int, Switch on String and Select.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value};
use seal_migrate::{import, Options};

const SWITCH: &str = include_str!("fixtures/switch.txt");

fn run() -> (Interp, MapHost) {
    let imp = import(SWITCH, &Options { name: "switch".into(), ..Options::default() })
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
fn switch_on_int_takes_the_matching_case_or_default() {
    let (mut vm, mut h) = run();
    for (n, want) in [(0, "zero"), (1, "one"), (2, "two"), (3, "other"), (-1, "other")] {
        assert_eq!(fire(&mut vm, &mut h, "Pick", &[Value::Int(n)]), [want]);
    }
}

#[test]
fn switch_on_string_is_case_sensitive() {
    let (mut vm, mut h) = run();
    for (s, want) in [("a", "A"), ("b", "B"), ("A", "?"), ("", "?")] {
        assert_eq!(fire(&mut vm, &mut h, "Word", &[Value::Str(s.into())]), [want]);
    }
}

#[test]
fn select_by_int_and_by_bool() {
    let (mut vm, mut h) = run();
    let mut go = |i, f| fire(&mut vm, &mut h, "Choose", &[Value::Int(i), Value::Bool(f)]);
    assert_eq!(go(0, false), ["x", "no"]);
    assert_eq!(go(2, true), ["z", "yes"]);
    assert_eq!(go(1, true), ["y", "yes"]);
    assert_eq!(go(7, false), ["", "no"]);
}
