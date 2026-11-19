//! Get All Actors Of Class: every actor made from a class's prefab, or one
//! based on it, kept as a list when the node runs.

use seal_graph::{GraphInstance, Interp, MapHost, Program};
use seal_migrate::{import, Options};

const FINDS: &str = include_str!("fixtures/finds.txt");
const BAD: &str = include_str!("fixtures/finds_bad.txt");
const ME: u64 = 1;

fn run() -> (Interp, MapHost) {
    let imp = import(FINDS, &Options { name: "finds".into(), ..Options::default() })
        .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let program = Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}"));
    let host = MapHost::default()
        .prefab("BP_Enemy", &[])
        .prefab("BP_Boss", &[])
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
fn finds_every_actor_based_on_a_class() {
    let (mut vm, mut h) = run();
    assert_eq!(fire(&mut vm, &mut h, "Count"), ["0"]);
    for e in ["MakeBoss", "MakeCoin", "MakeBoss", "MakeBoss"] {
        fire(&mut vm, &mut h, e);
    }
    assert_eq!(fire(&mut vm, &mut h, "Count"), ["3"], "bosses are enemies, the coin is not");
}

#[test]
fn destroying_inside_the_loop_skips_nobody() {
    let (mut vm, mut h) = run();
    for e in ["MakeBoss", "MakeCoin", "MakeBoss", "MakeBoss"] {
        fire(&mut vm, &mut h, e);
    }
    fire(&mut vm, &mut h, "Purge");
    assert_eq!(h.alive.len(), 1, "only the coin is left");
    assert_eq!(fire(&mut vm, &mut h, "Count"), ["0"]);
}

#[test]
fn get_actor_of_class_takes_one_or_nothing() {
    let (mut vm, mut h) = run();
    fire(&mut vm, &mut h, "KillOne");
    assert!(h.alive.is_empty(), "nothing to find, nothing destroyed");
    for e in ["MakeCoin", "MakeBoss", "MakeBoss"] {
        fire(&mut vm, &mut h, e);
    }
    fire(&mut vm, &mut h, "KillOne");
    assert_eq!(fire(&mut vm, &mut h, "Count"), ["1"], "one boss destroyed");
    fire(&mut vm, &mut h, "KillOne");
    fire(&mut vm, &mut h, "KillOne");
    assert_eq!(fire(&mut vm, &mut h, "Count"), ["0"]);
    assert_eq!(h.alive.len(), 1, "the coin is never found");
}

#[test]
fn finds_of_built_in_classes_are_reported() {
    let imp =
        import(BAD, &Options { name: "bad".into(), ..Options::default() }).expect("fixture parses");
    let text = imp.report.to_text();
    assert_eq!(imp.report.unsupported(), 1, "{text}");
    assert!(text.contains("find of the built-in class Character"), "{text}");
}

#[test]
fn finds_compile_to_the_same_rust() {
    let imp = import(FINDS, &Options { name: "finds".into(), ..Options::default() }).unwrap();
    let program = Program::build(&imp.graph).unwrap();
    let code = seal_graph::codegen::compile_all(&[program]);
    assert!(
        code.contains("rt::find_all(host, \"BP_Enemy\")"),
        "the find reaches the generated code"
    );
}
