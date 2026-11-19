//! Save games: slots written by scripts, put back by loading, and versions.

use seal_world::savegame::{self, SaveGame};
use seal_world::{PlayMode, World};

fn project(name: &str, save_version: Option<u32>) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("seal-world-saves-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("logic")).unwrap();
    let settings = save_version.map_or("{}".to_string(), |v| format!("{{ save_version = {v} }}"));
    std::fs::write(
        dir.join("project.seal"),
        format!("schema_version = 1\nname = \"P\"\nsettings = {settings}\n"),
    )
    .unwrap();
    std::fs::write(dir.join("logic/saver.graph"), SAVER).unwrap();
    dir
}

const SAVER: &str = r#"
schema_version = 1
seal_graph = "saver"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "set_saved", defaults = { key = "level", value = "3" } },
  { id = 3, kind = "add_score", defaults = { name = "coins", amount = 5 } },
  { id = 4, kind = "set_game_state", defaults = { state = "playing" } },
  { id = 5, kind = "save_game", defaults = { slot = "Slot 1" } },
  { id = 6, kind = "event", name = "game_loaded" },
  { id = 7, kind = "saved", defaults = { key = "level" } },
  { id = 8, kind = "log" },
  { id = 9, kind = "saved", defaults = { key = "save_version" } },
  { id = 10, kind = "log" },
  { id = 11, kind = "event", name = "tick" },
  { id = 12, kind = "has_save", defaults = { slot = "Slot 1" } },
  { id = 13, kind = "to_str", type = "bool" },
  { id = 14, kind = "log" },
]
links = [
  "1.then -> 2.in", "2.then -> 3.in", "3.then -> 4.in", "4.then -> 5.in",
  "6.then -> 8.in", "7.out -> 8.message", "8.then -> 10.in", "9.out -> 10.message",
  "11.then -> 14.in", "12.out -> 13.a", "13.out -> 14.message",
]
"#;

fn playing(dir: &std::path::Path) -> World {
    let mut world = World::starter();
    world.entities[0].script = "logic/saver.graph".into();
    world.project_dir = Some(dir.to_path_buf());
    world.play();
    assert_eq!(world.mode, PlayMode::Playing);
    world
}

#[test]
fn a_script_saves_and_loading_puts_the_game_back() {
    let dir = project("round", None);
    let mut world = playing(&dir);
    world.step(0.1);
    world.step(0.1);
    let file = dir.join("saved/games/Slot 1.sealsave");
    let text = std::fs::read_to_string(&file).expect("the save file was written");
    assert!(text.contains("seal_save = 1") && text.contains("game_version = 1"), "{text}");
    assert!(text.contains("level") && text.contains("coins"), "{text}");
    assert!(world.play_log.iter().any(|l| l == "saved the game in Slot 1"), "{:?}", world.play_log);
    assert_eq!(world.play_log.last().map(String::as_str), Some("true"), "has_save sees the slot");

    let where_it_was = world.entities[0].translation;
    world.entities[0].translation = [40.0, 50.0, 60.0];
    world.entities[0].velocity = [1.0, 2.0, 3.0];
    world.load_game(&dir, "Slot 1").unwrap();
    assert_eq!(world.entities[0].translation, where_it_was);
    assert_eq!(world.entities[0].velocity, [0.0; 3]);
    let game = world.play_game().unwrap();
    assert_eq!(game.scores.get("coins"), Some(&5));
    assert_eq!(game.state, "playing");
    let log = &world.play_log;
    let i = log.iter().position(|l| l == "loaded Slot 1").expect("load note");
    assert_eq!(&log[i + 1..i + 3], ["3", "1"], "game_loaded reads the kept values: {log:?}");

    let slots = savegame::list(&dir);
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].slot, "Slot 1");
    assert!(slots[0].problem.is_none());
    world.stop();
    assert!(file.exists(), "stopping play keeps saves");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn older_saves_load_and_newer_ones_are_refused() {
    let dir = project("versions", None);
    let mut world = playing(&dir);
    world.step(0.1);
    world.stop();
    // The game moves to save version 2: the version 1 save still loads and says so.
    std::fs::write(
        dir.join("project.seal"),
        "schema_version = 1\nname = \"P\"\nsettings = { save_version = 2 }\n",
    )
    .unwrap();
    assert_eq!(savegame::project_save_version(&dir), 2);
    let mut world = playing(&dir);
    world.step(0.1);
    let old = std::fs::read_to_string(dir.join("saved/games/Slot 1.sealsave")).unwrap();
    assert!(old.contains("game_version = 2"), "the start graph saved again at version 2");
    let v1 = old.replace("game_version = 2", "game_version = 1");
    std::fs::write(dir.join("saved/games/Old.sealsave"), &v1).unwrap();
    world.load_game(&dir, "Old").unwrap();
    assert!(
        world
            .play_log
            .iter()
            .any(|l| l == "loaded Old (saved at game version 1; the game is at 2)"),
        "{:?}",
        world.play_log
    );
    assert_eq!(world.play_log.last().map(String::as_str), Some("1"), "save_version tells scripts");

    let newer_game = old.replace("game_version = 2", "game_version = 3");
    let e = SaveGame::from_text(&newer_game, 2).unwrap_err();
    assert!(e.contains("newer version of the game"), "{e}");
    let newer_format = old.replace("seal_save = 1", "seal_save = 2");
    let e = SaveGame::from_text(&newer_format, 2).unwrap_err();
    assert!(e.contains("newer Seal"), "{e}");
    std::fs::write(dir.join("saved/games/Future.sealsave"), &newer_game).unwrap();
    let before = world.entities[0].translation;
    world.entities[0].translation = [9.0, 9.0, 9.0];
    let e = world.load_game(&dir, "Future").unwrap_err();
    assert!(e.contains("cannot load the save Future"), "{e}");
    assert_eq!(world.entities[0].translation, [9.0, 9.0, 9.0], "a refused load changes nothing");
    let _ = before;
    let listed = savegame::list(&dir);
    let future = listed.iter().find(|s| s.slot == "Future").unwrap();
    assert!(future.problem.is_some(), "the list says why it cannot load");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn slot_names_are_checked_and_deleting_removes_the_file() {
    assert!(savegame::slot_name("../evil").is_err());
    assert!(savegame::slot_name("  ").is_err());
    assert_eq!(savegame::slot_name(" Quick save_2 ").unwrap(), "Quick save_2");
    let dir = project("delete", None);
    let mut world = playing(&dir);
    world.step(0.1);
    assert!(savegame::delete(&dir, "Slot 1").unwrap());
    assert!(!savegame::delete(&dir, "Slot 1").unwrap(), "a missing save is fine");
    let e = savegame::read(&dir, "Slot 1").unwrap_err();
    assert_eq!(e, "there is no save in the slot Slot 1");
    std::fs::remove_dir_all(dir).ok();
}
