//! Arrays: changed in place through a variable, read, looped over with
//! For Each (with and without Break), built with Make Array, and cleared.

use seal_graph::{Elem, GraphInstance, Interp, MapHost, Program, Value};
use seal_migrate::{import, Options};

const ARRAYS: &str = include_str!("fixtures/arrays.txt");

fn run() -> (Interp, MapHost) {
    let imp = import(ARRAYS, &Options { name: "arrays".into(), ..Options::default() })
        .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    let program = Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}"));
    (Interp::new(program, 1), MapHost::default())
}

fn fire(vm: &mut Interp, host: &mut MapHost, event: &str, args: &[Value]) -> Vec<String> {
    let before = host.logs.len();
    vm.fire(event, args, host).unwrap();
    host.logs[before..].to_vec()
}

fn scores(vm: &Interp) -> Value {
    vm.vars().into_iter().find(|(n, _)| n == "Scores").unwrap().1
}

fn ints(v: &[i64]) -> Value {
    Value::List(Elem::Int, v.iter().map(|x| Value::Int(*x)).collect())
}

#[test]
fn arrays_change_in_place_and_loop() {
    let (mut vm, mut h) = run();
    let i = Value::Int;
    assert_eq!(scores(&vm), ints(&[]));
    // Add returns the new item's index; Length follows.
    assert_eq!(fire(&mut vm, &mut h, "Fill", &[i(5)]), ["0", "1"]);
    assert_eq!(fire(&mut vm, &mut h, "Fill", &[i(7)]), ["1", "2"]);
    assert_eq!(fire(&mut vm, &mut h, "Fill", &[i(9)]), ["2", "3"]);
    assert_eq!(fire(&mut vm, &mut h, "Sum", &[]), ["21"]);
    // Find, Contains, Get (out of range reads zero) and Last Index.
    assert_eq!(fire(&mut vm, &mut h, "Query", &[i(7)]), ["1", "true", "0", "2"]);
    assert_eq!(fire(&mut vm, &mut h, "Query", &[i(1)]), ["-1", "false", "7", "2"]);
    // Insert 4 at 1, remove index 0, set index 2 to 4, remove every 7.
    fire(&mut vm, &mut h, "Edit", &[i(1), i(4)]);
    assert_eq!(scores(&vm), ints(&[4, 4]));
    assert_eq!(fire(&mut vm, &mut h, "Sum", &[]), ["8"]);
    assert_eq!(fire(&mut vm, &mut h, "FindFirst", &[i(4)]), ["0", "done"]);
    assert_eq!(fire(&mut vm, &mut h, "FindFirst", &[i(100)]), ["done"]);
    fire(&mut vm, &mut h, "Reset", &[i(8)]);
    assert_eq!(scores(&vm), ints(&[7, 8, 9]));
    assert_eq!(fire(&mut vm, &mut h, "FindFirst", &[i(9)]), ["2", "done"]);
    // A break in an earlier run does not stop the next one.
    assert_eq!(fire(&mut vm, &mut h, "FindFirst", &[i(8)]), ["1", "done"]);
    assert_eq!(fire(&mut vm, &mut h, "Sum", &[]), ["24"]);
    assert_eq!(fire(&mut vm, &mut h, "Wipe", &[]), ["0"]);
    assert_eq!(fire(&mut vm, &mut h, "Sum", &[]), ["0"]);
}

#[test]
fn arrays_compile_to_the_same_rust() {
    let imp = import(ARRAYS, &Options { name: "arrays".into(), ..Options::default() }).unwrap();
    let program = Program::build(&imp.graph).unwrap();
    let code = seal_graph::codegen::compile_all(&[program]);
    assert!(
        code.contains("list_push") && code.contains("list_get"),
        "lists reach the generated code"
    );
}
