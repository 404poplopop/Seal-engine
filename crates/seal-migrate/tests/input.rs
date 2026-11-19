//! Key, input action and enhanced input action events become named Seal
//! events that the game loop fires.

use seal_graph::{GraphInstance, Interp, MapHost, Program};
use seal_migrate::{import, Options};

const INPUT: &str = include_str!("fixtures/input.txt");

#[test]
fn input_events_become_named_events() {
    let imp = import(INPUT, &Options { name: "input".into(), ..Options::default() })
        .expect("fixture parses");
    let text = imp.report.to_text();
    assert_eq!(imp.report.unsupported(), 1, "only the second space bar event is refused: {text}");
    assert!(text.contains("second event for the same input"), "{text}");
    let p = Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}"));
    let names: Vec<&str> = p.events.iter().map(|e| e.name.as_str()).collect();
    for want in [
        "key_SpaceBar_pressed",
        "key_SpaceBar_released",
        "action_Fire_pressed",
        "action_IA_Move_triggered",
        "action_IA_Move_pressed",
    ] {
        assert!(names.contains(&want), "{want} missing from {names:?}");
    }
    let mut vm = Interp::new(p, 1);
    let mut h = MapHost::default();
    for e in [
        "key_SpaceBar_pressed",
        "action_IA_Move_triggered",
        "action_Fire_pressed",
        "action_Fire_released",
        "key_SpaceBar_released",
    ] {
        vm.fire(e, &[], &mut h).unwrap();
    }
    assert_eq!(h.logs, ["jump", "move", "bang", "land"]);
}
