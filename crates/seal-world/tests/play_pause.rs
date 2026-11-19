//! Script control of play time, and leaving play from a script.

use seal_world::{PlayMode, World};

fn project(name: &str, graph: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("seal-play-pause-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("logic")).unwrap();
    std::fs::write(dir.join("logic/g.graph"), graph).unwrap();
    dir
}

#[test]
fn a_script_can_stop_and_resume_play_time() {
    let dir = project(
        "scale",
        r#"
schema_version = 1
seal_graph = "scale"
vars = [{ name = "went", type = "bool", default = false }]
nodes = [
  { id = 1, kind = "event", name = "tick" },
  { id = 2, kind = "get", name = "went" },
  { id = 3, kind = "not" },
  { id = 4, kind = "branch" },
  { id = 5, kind = "self" },
  { id = 6, kind = "world_set", component = "play", field = "time_scale", type = "float", defaults = { value = 0.0 } },
  { id = 7, kind = "const", type = "bool", value = true },
  { id = 8, kind = "set", name = "went" },
]
links = [
  "1.then -> 4.in", "2.out -> 3.a", "3.out -> 4.cond", "4.true -> 6.in",
  "5.out -> 6.entity", "6.then -> 8.in", "7.out -> 8.value",
]
"#,
    );
    let mut w = World::from_text(&World::empty_scene_text("T")).unwrap();
    w.gravity = 0.0;
    w.entities[0].script = "logic/g.graph".into();
    w.project_dir = Some(dir.clone());
    w.play();
    w.step(1.0 / 60.0);
    assert_eq!(w.time_scale, 0.0, "script paused play time: {:?}", w.play_log);
    let t = w.play_time;
    w.step(1.0 / 60.0);
    w.step(1.0 / 60.0);
    assert_eq!(w.play_time, t, "paused play does not advance time");
    w.stop();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn quit_state_leaves_play() {
    let dir = project(
        "quit",
        r#"
schema_version = 1
seal_graph = "quit"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "set_game_state", defaults = { state = "quit" } },
]
links = ["1.then -> 2.in"]
"#,
    );
    let mut w = World::from_text(&World::empty_scene_text("T")).unwrap();
    w.entities[0].script = "logic/g.graph".into();
    w.project_dir = Some(dir.clone());
    w.play();
    w.step(1.0 / 60.0);
    assert!(w.quit);
    assert_eq!(w.mode, PlayMode::Editing);
    w.stop();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_save_remembers_the_open_scene() {
    let dir = std::env::temp_dir().join(format!("seal-play-scene-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("project.seal"), "schema_version = 1\nname = \"P\"\n").unwrap();
    let mut w = World::from_text(&World::empty_scene_text("T")).unwrap();
    w.project_dir = Some(dir.clone());
    w.play();
    w.scenes.open = "scenes/cave.seal".into();
    w.entities[0].translation = [1.0, 2.0, 0.0];
    w.save_game(&dir, "shrine").unwrap();
    w.scenes.open = "scene.seal".into();
    w.entities[0].translation = [9.0, 9.0, 0.0];
    w.load_game(&dir, "shrine").unwrap();
    assert_eq!(w.scenes.open, "scenes/cave.seal");
    assert_eq!(w.entities[0].translation[0], 1.0);
    w.stop();
    let _ = std::fs::remove_dir_all(&dir);
}
