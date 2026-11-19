//! Spawn Actor from Class, Destroy Actor and Is Valid.

use seal_graph::{GraphInstance, Interp, MapHost, Program, Value};
use seal_migrate::{import, Options};

const SPAWN: &str = include_str!("fixtures/spawn.txt");
const BAD: &str = include_str!("fixtures/spawn_bad.txt");
const ME: u64 = 1;

fn run() -> (Interp, MapHost) {
    let imp = import(SPAWN, &Options { name: "spawn".into(), ..Options::default() })
        .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    assert!(text.contains("BP_Coin"), "the report names the prefab to make\n{text}");
    let program = Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}"));
    let host = MapHost::default().prefab("BP_Coin", &[("coin", "value", Value::Int(1))]).with(
        ME,
        "me",
        "hp",
        Value::Int(3),
    );
    (Interp::new(program, ME), host)
}

fn fire(vm: &mut Interp, host: &mut MapHost, event: &str) -> Vec<String> {
    let before = host.logs.len();
    vm.fire(event, &[], host).unwrap();
    host.logs[before..].to_vec()
}

#[test]
fn spawned_actors_are_kept_checked_and_destroyed() {
    let (mut vm, mut h) = run();
    assert_eq!(fire(&mut vm, &mut h, "Check"), ["gone"], "nothing spawned yet");
    assert_eq!(fire(&mut vm, &mut h, "Ask"), ["no"]);
    assert_eq!(fire(&mut vm, &mut h, "Make"), ["made"]);
    assert_eq!(h.alive.len(), 1);
    assert_eq!(fire(&mut vm, &mut h, "Check"), ["valid"]);
    assert_eq!(fire(&mut vm, &mut h, "Ask"), ["yes"]);
    assert_eq!(fire(&mut vm, &mut h, "Kill"), Vec::<String>::new());
    assert!(h.alive.is_empty());
    assert_eq!(fire(&mut vm, &mut h, "Check"), ["gone"]);
    assert_eq!(fire(&mut vm, &mut h, "Kill"), Vec::<String>::new(), "destroying twice is harmless");
}

#[test]
fn destroy_actor_without_a_target_destroys_self() {
    let (mut vm, mut h) = run();
    fire(&mut vm, &mut h, "Bye");
    assert!(!h.fields.keys().any(|k| k.0 == ME));
}

#[test]
fn values_set_on_spawn_are_reported() {
    let imp =
        import(BAD, &Options { name: "bad".into(), ..Options::default() }).expect("fixture parses");
    let text = imp.report.to_text();
    assert_eq!(imp.report.unsupported(), 1, "{text}");
    assert!(text.contains("Speed set on spawn"), "{text}");
}
