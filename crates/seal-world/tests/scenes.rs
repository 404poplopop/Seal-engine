//! Scenes at runtime: open, add, unload and background loads, with objects
//! kept across scene switches.

use seal_world::scenes;
use seal_world::{Entity, PlayMode, World};
use std::path::{Path, PathBuf};

fn obj(id: &str, parent: Option<&str>) -> Entity {
    let mut e = World::starter().entities.into_iter().find(|e| e.id == "sun").unwrap();
    e.id = id.into();
    e.name = id.into();
    e.parent = parent.map(str::to_string);
    e
}

fn scene(objs: Vec<Entity>) -> String {
    let mut w = World::starter();
    w.entities = objs;
    w.to_text()
}

fn project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("seal-world-scenes-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for sub in ["logic", "levels", "rooms"] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
    }
    std::fs::write(dir.join("project.seal"), "schema_version = 1\nname = \"P\"\nsettings = {}\n")
        .unwrap();
    dir
}

fn graph(dir: &Path, name: &str, nodes: &str, links: &str) {
    let text = format!(
        "schema_version = 1\nseal_graph = \"{name}\"\nvars = []\nnodes = [\n{nodes}\n]\nlinks = [{links}]\n"
    );
    std::fs::write(dir.join(format!("logic/{name}.graph")), text).unwrap();
}

fn playing(dir: &Path, objs: Vec<Entity>) -> World {
    let mut world = World::starter();
    world.entities = objs;
    world.project_dir = Some(dir.to_path_buf());
    world.scenes.open = "scene.seal".into();
    world.play();
    assert_eq!(world.mode, PlayMode::Playing);
    world
}

#[test]
fn opening_a_scene_keeps_the_kept_player_and_its_script() {
    let dir = project("open");
    graph(
        &dir,
        "player",
        r#"{ id = 1, kind = "event", name = "start" },
  { id = 2, kind = "self" },
  { id = 3, kind = "keep_across_scenes", defaults = { keep = true } },
  { id = 4, kind = "log", defaults = { message = "player started" } },
  { id = 5, kind = "event", name = "scene_loaded" },
  { id = 6, kind = "current_scene" },
  { id = 7, kind = "log" },"#,
        r#""1.then -> 3.in", "2.out -> 3.entity", "3.then -> 4.in", "5.then -> 7.in", "6.out -> 7.message""#,
    );
    graph(
        &dir,
        "door",
        r#"{ id = 1, kind = "event", name = "start" },
  { id = 2, kind = "open_scene", defaults = { path = "levels/two.seal" } },"#,
        r#""1.then -> 2.in""#,
    );
    let mut double = obj("player", Some("root2"));
    double.translation = [9.0, 9.0, 9.0];
    std::fs::write(
        dir.join("levels/two.seal"),
        scene(vec![obj("root2", None), double, obj("rock", Some("root2"))]),
    )
    .unwrap();

    let mut player = obj("player", Some("root"));
    player.script = "logic/player.graph".into();
    player.translation = [1.0, 2.0, 3.0];
    let mut door = obj("door", Some("root"));
    door.script = "logic/door.graph".into();
    let mut world =
        playing(&dir, vec![obj("root", None), player, door, obj("crate", Some("root"))]);
    assert_eq!(world.scenes.open, "scene.seal");
    world.step(0.1);

    let ids: Vec<&str> = world.entities.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["player", "root2", "rock"], "{:?}", world.play_log);
    let p = &world.entities[0];
    assert_eq!(p.translation, [1.0, 2.0, 3.0], "the kept player, not the new scene's copy");
    assert_eq!(p.parent, None, "its old parent went with the old scene");
    assert_eq!(world.scenes.open, "levels/two.seal");
    let log = &world.play_log;
    assert_eq!(
        log.iter().filter(|l| *l == "player started").count(),
        1,
        "its script ran on: {log:?}"
    );
    assert!(
        log.iter().any(|l| l
            == "opened levels/two.seal (1 kept); 1 of its objects were already kept and left out"),
        "{log:?}"
    );
    assert_eq!(log.last().map(String::as_str), Some("levels/two.seal"), "scene_loaded fired");

    // Stopping puts the first scene back.
    world.stop();
    assert_eq!(world.entities.len(), 4);
    assert_eq!(world.scenes.open, "scene.seal");
    assert!(world.scenes.kept.is_empty());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn added_scenes_join_and_leave_and_background_loads_switch_later() {
    let dir = project("add");
    graph(
        &dir,
        "adder",
        r#"{ id = 1, kind = "event", name = "start" },
  { id = 2, kind = "add_scene", defaults = { path = "rooms/side.seal" } },
  { id = 3, kind = "event", name = "scene_loaded" },
  { id = 4, kind = "scene_loaded", defaults = { path = "rooms/side.seal" } },
  { id = 5, kind = "to_str", type = "bool" },
  { id = 6, kind = "log" },"#,
        r#""1.then -> 2.in", "3.then -> 6.in", "4.out -> 5.a", "5.out -> 6.message""#,
    );
    std::fs::write(
        dir.join("rooms/side.seal"),
        scene(vec![obj("root", None), obj("lamp", Some("root"))]),
    )
    .unwrap();
    std::fs::write(dir.join("levels/far.seal"), scene(vec![obj("far", None)])).unwrap();
    let mut adder = obj("adder", Some("root"));
    adder.script = "logic/adder.graph".into();
    let mut world = playing(&dir, vec![obj("root", None), adder]);
    world.step(0.1);

    let ids: Vec<&str> = world.entities.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["root", "adder", "root@side", "lamp"], "{:?}", world.play_log);
    assert_eq!(world.entities[3].parent.as_deref(), Some("root@side"), "parents follow renames");
    assert!(world.scenes.is_loaded("rooms/side.seal"));
    assert_eq!(world.scenes.loaded(), ["scene.seal", "rooms/side.seal"]);
    assert!(world.play_log.iter().any(|l| l == "added rooms/side.seal (2 objects)"));
    assert_eq!(world.play_log.last().map(String::as_str), Some("true"), "scripts see it loaded");

    world.unload_scene("rooms/side.seal");
    let ids: Vec<&str> = world.entities.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["root", "adder"]);
    assert!(!world.scenes.is_loaded("rooms/side.seal"));

    // A background load leaves play running until the file is read.
    world.scenes.load_async(&dir, "levels/far.seal", false).unwrap();
    let mut frames = 0;
    while world.scenes.open != "levels/far.seal" && frames < 400 {
        world.step(0.016);
        std::thread::sleep(std::time::Duration::from_millis(2));
        frames += 1;
    }
    assert_eq!(world.scenes.open, "levels/far.seal", "{:?}", world.play_log);
    assert_eq!(world.entities.len(), 1);
    assert!(world.scenes.loading.is_empty());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn scene_paths_are_checked_and_missing_files_are_reported() {
    assert!(scenes::check("../outside.seal").is_err());
    assert!(scenes::check("/abs/level.seal").is_err());
    assert!(scenes::check("C:/x/level.seal").is_err());
    assert!(scenes::check("project.seal").is_err());
    assert!(scenes::check("notes.txt").is_err());
    assert_eq!(scenes::check(".\\levels\\two.seal").unwrap(), "levels/two.seal");

    let dir = project("missing");
    graph(
        &dir,
        "door",
        r#"{ id = 1, kind = "event", name = "start" },
  { id = 2, kind = "open_scene", defaults = { path = "levels/nope.seal" } },"#,
        r#""1.then -> 2.in""#,
    );
    let mut door = obj("door", None);
    door.script = "logic/door.graph".into();
    let mut world = playing(&dir, vec![door]);
    world.step(0.1);
    assert_eq!(world.entities.len(), 1, "the running scene stays");
    assert!(
        world.play_log.iter().any(|l| l.starts_with("cannot open the scene levels/nope.seal")),
        "{:?}",
        world.play_log
    );
    std::fs::remove_dir_all(dir).ok();
}
