//! Cast To: what an actor was made from, following prefab bases.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value};
use seal_migrate::{import, Options};

const CASTS: &str = include_str!("fixtures/casts.txt");
const BAD: &str = include_str!("fixtures/casts_bad.txt");
const ME: u64 = 1;

fn run() -> (Interp, MapHost) {
    let imp = import(CASTS, &Options { name: "casts".into(), ..Options::default() })
        .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let program = Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}"));
    let host = MapHost::default()
        .prefab("BP_Enemy", &[])
        .prefab("BP_Boss", &[("hp", "value", Value::Int(9))])
        .based_on("BP_Boss", "BP_Enemy")
        .prefab("BP_Coin", &[]);
    (Interp::new(program, ME), host)
}

fn fire(vm: &mut Interp, host: &mut MapHost, event: &str) -> Vec<String> {
    let before = host.logs.len();
    vm.fire(event, &[], host).unwrap();
    host.logs[before..].to_vec()
}

#[test]
fn casts_follow_prefab_bases() {
    let (mut vm, mut h) = run();
    assert_eq!(fire(&mut vm, &mut h, "Check"), ["not an enemy", "false"], "nothing kept yet");
    fire(&mut vm, &mut h, "MakeBoss");
    assert_eq!(fire(&mut vm, &mut h, "Check"), ["enemy", "true"], "a boss is an enemy");
    fire(&mut vm, &mut h, "MakeCoin");
    assert_eq!(fire(&mut vm, &mut h, "Check"), ["not an enemy", "false"]);
    assert_eq!(h.alive.len(), 2);
    fire(&mut vm, &mut h, "Kill");
    assert_eq!(h.alive.len(), 2, "a coin cast to a boss gives nothing, so nothing is destroyed");
    fire(&mut vm, &mut h, "MakeBoss");
    fire(&mut vm, &mut h, "Kill");
    assert_eq!(h.alive.len(), 2, "the boss cast succeeds and the boss is destroyed");
    assert_eq!(
        fire(&mut vm, &mut h, "Check"),
        ["not an enemy", "false"],
        "a destroyed boss is nothing"
    );
}

#[test]
fn casts_to_built_in_classes_are_reported() {
    let imp =
        import(BAD, &Options { name: "bad".into(), ..Options::default() }).expect("fixture parses");
    let text = imp.report.to_text();
    assert_eq!(imp.report.unsupported(), 1, "{text}");
    assert!(text.contains("cast to the built-in class Character"), "{text}");
}

#[test]
fn casts_compile_to_the_same_rust() {
    let imp = import(CASTS, &Options { name: "casts".into(), ..Options::default() }).unwrap();
    let program = Program::build(&imp.graph).unwrap();
    let code = seal_graph::codegen::compile_all(&[program]);
    assert!(
        code.contains("host.is_a(") && code.contains("\"BP_Enemy\""),
        "the check reaches the generated code"
    );
}
