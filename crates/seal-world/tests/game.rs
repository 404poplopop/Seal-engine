//! Game rules: possess, start points, scores and state, all thrown away by stop.

use seal_world::World;

const RULES: &str = r#"
schema_version = 1
seal_graph = "rules"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 3, kind = "self" },
  { id = 4, kind = "possess", defaults = { index = 0 } },
  { id = 5, kind = "spawn_player", defaults = { index = 0 } },
  { id = 6, kind = "set_game_state", defaults = { state = "playing" } },
  { id = 7, kind = "event", name = "key_K_pressed" },
  { id = 8, kind = "add_score", defaults = { name = "coins", amount = 5 } },
  { id = 9, kind = "score", defaults = { name = "coins" } },
  { id = 10, kind = "to_str", type = "int" },
  { id = 11, kind = "log" },
  { id = 12, kind = "player", defaults = { index = 0 } },
  { id = 13, kind = "compare", op = "eq", type = "entity" },
  { id = 14, kind = "to_str", type = "bool" },
  { id = 15, kind = "log" },
  { id = 16, kind = "game_state" },
  { id = 17, kind = "log" },
]
links = [
  "1.then -> 4.in", "3.out -> 4.entity", "4.then -> 5.in", "5.then -> 6.in",
  "6.then -> 15.in", "12.out -> 13.a", "3.out -> 13.b", "13.out -> 14.a", "14.out -> 15.message",
  "15.then -> 17.in", "16.out -> 17.message",
  "7.then -> 8.in", "8.then -> 11.in", "9.out -> 10.a", "10.out -> 11.message",
]
"#;

#[test]
fn a_script_possesses_spawns_scores_and_sets_the_state() {
    let dir = std::env::temp_dir().join(format!("seal-world-game-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("logic")).unwrap();
    std::fs::write(dir.join("logic/rules.graph"), RULES).unwrap();
    let mut w = World::starter();
    for e in &mut w.entities {
        e.dynamic = false;
        e.tag.clear();
        e.script.clear();
    }
    let hero = w.add("capsule");
    let start = w.add("empty");
    let other = w.add("cube");
    for e in &mut w.entities {
        if e.id == start {
            e.tag = "player_start".into();
            e.translation = [3.0, 1.0, -2.0];
            e.rotation = [0.0, 90.0, 0.0];
        } else if e.id == other {
            e.tag = "player".into();
        } else if e.id == hero {
            e.translation = [0.0, 5.0, 0.0];
            e.velocity = [1.0, 0.0, 0.0];
            e.script = "logic/rules.graph".into();
        }
    }
    let before = w.entities.clone();
    w.project_dir = Some(dir.clone());
    w.play();
    w.step(0.0);
    let h = w.entities.iter().find(|e| e.id == hero).unwrap();
    assert_eq!(h.translation, [3.0, 1.0, -2.0], "{:?}", w.play_log);
    assert_eq!((h.rotation, h.velocity), ([0.0, 90.0, 0.0], [0.0; 3]));
    assert_eq!(
        w.play_log,
        ["true", "playing"],
        "player 0 is the possessed object, not the tagged one"
    );
    w.play_keys.push(("K".into(), true));
    w.step(0.1);
    w.play_keys.push(("K".into(), false));
    w.play_keys.push(("K".into(), true));
    w.step(0.1);
    assert_eq!(&w.play_log[2..], ["5", "10"]);
    let g = w.play_game().unwrap();
    assert_eq!(g.possessed.get(&0), Some(&hero));
    assert_eq!(g.summary().as_deref(), Some("playing · coins 10"));
    w.stop();
    assert!(w.play_game().is_none());
    assert_eq!(w.entities, before);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn spawning_without_a_start_point_or_a_player_says_why() {
    use seal_world::game::Game;
    let mut w = World::starter();
    w.entities.retain(|e| e.tag != "player" && e.tag != "player_start");
    let g = Game::default();
    assert_eq!(g.spawn_player(&mut w.entities, 0).unwrap_err(), "player 0 controls no object");
    let a = w.add("cube");
    w.entities.iter_mut().find(|e| e.id == a).unwrap().tag = "player".into();
    assert_eq!(g.spawn_player(&mut w.entities, 0).unwrap_err(), "no object is tagged player_start");
    // Two players share one start point in turn.
    let s = w.add("empty");
    w.entities.iter_mut().find(|e| e.id == s).unwrap().tag = "player_start".into();
    let b = w.add("cube");
    w.entities.iter_mut().find(|e| e.id == b).unwrap().tag = "player".into();
    assert!(g.spawn_player(&mut w.entities, 1).is_ok());
    assert_eq!(Game::default().summary(), None);
}
