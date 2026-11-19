//! Set Window Mode changes the window while play runs.

use seal_world::World;

const SWITCH: &str = r#"
schema_version = 1
seal_graph = "window"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "set_window_mode", defaults = { mode = "borderless" } },
]
links = [
  "1.then -> 2.in",
]
"#;

const BROKEN: &str = r#"
schema_version = 1
seal_graph = "window"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "set_window_mode", defaults = { mode = "sideways" } },
]
links = [
  "1.then -> 2.in",
]
"#;

fn play(text: &str) -> World {
    let dir = std::env::temp_dir().join(format!(
        "seal-window-mode-{}-{}",
        std::process::id(),
        text.len()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("logic")).unwrap();
    std::fs::write(dir.join("logic/window.graph"), text).unwrap();
    let mut world = World::starter();
    for entity in &mut world.entities {
        entity.script.clear();
        entity.dynamic = false;
    }
    let id = world.add("empty");
    world.entity_mut(&id).unwrap().script = "logic/window.graph".into();
    world.project_dir = Some(dir);
    world.play();
    world.step(0.0);
    world
}

#[test]
fn set_window_mode_reaches_the_running_game() {
    let world = play(SWITCH);
    assert_eq!(world.play_game().unwrap().window_mode, "borderless", "{:?}", world.play_log);
}

#[test]
fn a_bad_window_mode_is_refused() {
    let world = play(BROKEN);
    assert!(world.play_game().unwrap().window_mode.is_empty(), "{:?}", world.play_log);
    assert!(world.play_log.iter().any(|line| line.contains("window mode")), "{:?}", world.play_log);
}
