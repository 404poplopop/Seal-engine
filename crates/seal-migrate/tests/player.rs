//! Get Player Pawn and Get Player Character ask the host for the entity a
//! local player controls.

use seal_graph::{GraphInstance, Interp, MapHost, Program};
use seal_migrate::{import, Options};

#[test]
fn each_player_index_names_its_entity() {
    let imp = import(
        include_str!("fixtures/player.txt"),
        &Options { name: "player".into(), ..Options::default() },
    )
    .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let mut vm =
        Interp::new(Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}")), 1);
    let mut h = MapHost { players: vec![4], alive: [4].into(), ..MapHost::default() };
    vm.fire("Who", &[], &mut h).unwrap();
    h.players.push(9);
    h.alive.insert(9);
    vm.fire("Who", &[], &mut h).unwrap();
    assert_eq!(h.logs, ["truefalse", "truetrue"]);
}

#[test]
fn a_player_controller_and_its_pawn_are_the_same_entity() {
    let imp = import(
        include_str!("fixtures/controller.txt"),
        &Options { name: "controller".into(), ..Options::default() },
    )
    .expect("fixture parses");
    let text = imp.report.to_text();
    assert!(imp.report.complete(), "{text}");
    assert_eq!(imp.report.dropped_links, 0, "{text}");
    let mut vm =
        Interp::new(Program::build(&imp.graph).unwrap_or_else(|e| panic!("{e:?}\n{text}")), 1);
    let mut h = MapHost::default();
    vm.fire("Who", &[], &mut h).unwrap();
    h.players.push(4);
    h.alive.insert(4);
    vm.fire("Who", &[], &mut h).unwrap();
    assert_eq!(h.logs, ["false", "true"]);
}
