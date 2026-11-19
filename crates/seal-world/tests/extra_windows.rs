//! Extra game window graph commands update the play model.

use seal_world::World;

const GRAPH: &str = r#"
schema_version = 1
seal_graph = "window"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "open_game_window", defaults = { id = 0, title = "Map", width = 800, height = 600, x = 40, y = 40, camera = "none", ui_screen = "" } },
  { id = 3, kind = "move_game_window", defaults = { id = 1, x = 10, y = 20 } },
  { id = 4, kind = "resize_game_window", defaults = { id = 1, width = 400, height = 300 } },
  { id = 5, kind = "close_game_window", defaults = { id = 1 } },
]
links = [
  "1.then -> 2.in",
  "2.then -> 3.in",
  "3.then -> 4.in",
  "4.then -> 5.in",
]
"#;

fn play(text: &str) -> World {
    let dir =
        std::env::temp_dir().join(format!("seal-extra-win-{}-{}", std::process::id(), text.len()));
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
fn window_nodes_create_move_and_close() {
    let world = play(GRAPH);
    let game = world.play_game().unwrap();
    assert!(
        game.extra_windows.open.is_empty(),
        "close removed the window: {:?}",
        game.extra_windows.open
    );
    assert!(
        !world.play_log.iter().any(|line| line.contains("window 1 is not open")),
        "{:?}",
        world.play_log
    );
}

const OPEN_MOVE: &str = r#"
schema_version = 1
seal_graph = "window"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "open_game_window", defaults = { id = 0, title = "Map", width = 800, height = 600, x = 40, y = 40, camera = "none", ui_screen = "Hud" } },
  { id = 3, kind = "move_game_window", defaults = { id = 1, x = 10, y = 20 } },
  { id = 4, kind = "resize_game_window", defaults = { id = 1, width = 400, height = 300 } },
]
links = [
  "1.then -> 2.in",
  "2.then -> 3.in",
  "3.then -> 4.in",
]
"#;

#[test]
fn window_nodes_keep_the_moved_window() {
    let world = play(OPEN_MOVE);
    assert!(world.play_log.is_empty(), "{:?}", world.play_log);
    let window = world.play_game().unwrap().extra_windows.open.get(&1).unwrap();
    assert_eq!(window.title, "Map");
    assert_eq!((window.x, window.y), (10, 20));
    assert_eq!((window.width, window.height), (400, 300));
    assert_eq!(window.ui_screen, "Hud");
    assert!(window.camera.is_empty());
}
