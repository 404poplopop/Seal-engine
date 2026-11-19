//! String, name and object equality, and text and name conversions.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value};
use seal_migrate::{import, Options};

const EQUAL: &str = include_str!("fixtures/equal.txt");

fn run() -> (Interp, MapHost) {
    let imp = import(EQUAL, &Options { name: "equal".into(), ..Options::default() })
        .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let program = Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}"));
    (Interp::new(program, 1), MapHost::default().prefab("BP_Coin", &[]))
}

fn same(vm: &mut Interp, h: &mut MapHost, a: &str, b: &str) -> Vec<String> {
    let before = h.logs.len();
    vm.fire("Same", &[Value::Str(a.into()), Value::Str(b.into())], h).unwrap();
    h.logs[before..].to_vec()
}

#[test]
fn strings_and_names_compare_ignoring_case() {
    let (mut vm, mut h) = run();
    assert_eq!(same(&mut vm, &mut h, "Coin", "coin"), ["true", "false"]);
    assert_eq!(same(&mut vm, &mut h, "Coin", "Coins"), ["false", "true"]);
    assert_eq!(same(&mut vm, &mut h, "", ""), ["true", "false"]);
}

#[test]
fn objects_compare_by_identity() {
    let (mut vm, mut h) = run();
    vm.fire("Make", &[], &mut h).unwrap();
    vm.fire("Make", &[], &mut h).unwrap();
    assert_eq!(h.logs, ["false", "false"], "a new actor is never the one kept before");
}

#[test]
fn equality_compiles_to_the_same_rust() {
    let imp = import(EQUAL, &Options { name: "equal".into(), ..Options::default() }).unwrap();
    let program = Program::build(&imp.graph).unwrap();
    let code = seal_graph::codegen::compile_all(&[program]);
    assert!(
        code.contains("str_lower") || code.contains("to_lowercase"),
        "case folding reaches the generated code"
    );
}
