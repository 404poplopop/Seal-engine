//! Game controls: project bindings, pad input, rumble and rebinding in play.

use seal_input::{Axis, Button, InputMap};
use seal_world::controls::{self, Controls};
use seal_world::{PlayMode, World};

fn project(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("seal-world-controls-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("logic")).unwrap();
    dir
}

fn pad(name: &str) -> Button {
    Button::parse(name).unwrap()
}

#[test]
fn keys_pad_buttons_and_the_stick_all_move_the_player() {
    let mut c = Controls::load(None, &mut Vec::new());
    c.set_keys(["D"]);
    assert!(c.frame().right);
    c.set_keys(["ArrowLeft", "Space"]);
    let f = c.frame();
    assert!(f.left && f.jump && !f.right);
    c.set_keys([]);
    c.raw.press(pad("pad.south"));
    c.raw.press(pad("pad.dpad_right"));
    let f = c.frame();
    assert!(f.jump && f.right);
    c.raw.clear_device("pad");
    let stick = Axis::parse("pad.left_x").unwrap();
    c.raw.set_axis(stick.clone(), -0.2);
    assert!(!c.frame().left, "a resting stick inside the deadzone does nothing");
    c.raw.set_axis(stick, -0.9);
    assert!(c.frame().left);
}

#[test]
fn the_project_file_sets_the_bindings_and_a_broken_one_falls_back() {
    let dir = project("file");
    let mut map = InputMap::default_play();
    map.rebind("jump", 0, Button::key("k")).unwrap();
    controls::save_project_map(&dir, &map).unwrap();
    let text = std::fs::read_to_string(dir.join(controls::INPUT_FILE)).unwrap();
    assert!(text.starts_with("# Seal input bindings"), "{text}");
    assert!(text.contains("action jump = key.k, key.w"), "{text}");
    let mut log = Vec::new();
    let mut c = Controls::load(Some(&dir), &mut log);
    assert!(log.is_empty(), "{log:?}");
    c.set_keys(["K"]);
    assert!(c.frame().jump);
    c.set_keys(["Space"]);
    assert!(!c.frame().jump, "Space was replaced by K in slot 0");

    std::fs::write(dir.join(controls::INPUT_FILE), "jump = space\n").unwrap();
    let mut c = Controls::load(Some(&dir), &mut log);
    assert_eq!(log.len(), 1, "{log:?}");
    assert!(log[0].contains("line 1") && log[0].contains("default controls"), "{log:?}");
    c.set_keys(["Space"]);
    assert!(c.frame().jump);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn rebinding_waits_for_a_new_button_moves_it_and_is_saved_for_the_player() {
    let dir = project("rebind");
    let mut c = Controls::load(Some(&dir), &mut Vec::new());
    c.set_keys(["Space"]);
    c.frame();
    c.start_rebind("move_left", 0).unwrap();
    assert!(c.start_rebind("fly", 0).is_err());
    c.start_rebind("move_left", 0).unwrap();
    // Space was already down when the wait began, so it does not count.
    assert_eq!(c.frame(), Default::default());
    assert!(c.capture.is_some());
    c.set_keys([]);
    c.frame();
    c.set_keys(["Space"]);
    c.frame();
    assert!(c.capture.is_none());
    assert_eq!(
        c.rebound,
        [("move_left".to_string(), seal_input::Capture::Bound(Button::key("space")))]
    );
    assert_eq!(c.map.bindings("move_left")[0], Button::key("space"));
    assert!(!c.map.bindings("jump").contains(&Button::key("space")), "a key drives one action");
    // Saved apart from the project's own bindings, and read back next play.
    assert!(!dir.join(controls::INPUT_FILE).exists());
    let saved = std::fs::read_to_string(dir.join(controls::PLAYER_FILE)).unwrap();
    assert!(saved.contains("action move_left = key.space"), "{saved}");
    let mut again = Controls::load(Some(&dir), &mut Vec::new());
    again.set_keys(["Space"]);
    let f = again.frame();
    assert!(f.left && !f.jump);
    // Esc cancels and leaves the bindings alone.
    again.set_keys([]);
    again.frame();
    again.start_rebind("jump", 0).unwrap();
    again.set_keys(["Escape"]);
    again.frame();
    assert_eq!(again.rebound, [("jump".to_string(), seal_input::Capture::Cancelled)]);
    assert_eq!(again.map.bindings("jump")[0], Button::key("w"));
    std::fs::remove_dir_all(dir).ok();
}

const PADDER: &str = r#"
schema_version = 1
seal_graph = "padder"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "rumble", defaults = { strong = 1.0, weak = 0.25, duration = 0.5 } },
  { id = 3, kind = "rebind_action", defaults = { action = "jump", slot = 1 } },
  { id = 4, kind = "event", name = "input_rebound" },
  { id = 5, kind = "log", defaults = { message = "rebound" } },
  { id = 6, kind = "event", name = "tick" },
  { id = 7, kind = "action_pressed", defaults = { name = "jump" } },
  { id = 8, kind = "to_str", type = "bool" },
  { id = 9, kind = "log" },
]
links = [
  "1.then -> 2.in", "2.then -> 3.in", "4.then -> 5.in",
  "6.then -> 9.in", "7.out -> 8.a", "8.out -> 9.message",
]
"#;

#[test]
fn scripts_rumble_the_pad_read_actions_and_rebind_in_play() {
    let dir = project("script");
    std::fs::write(dir.join("logic/padder.graph"), PADDER).unwrap();
    let mut world = World::starter();
    world.entities[0].script = "logic/padder.graph".into();
    world.project_dir = Some(dir.clone());
    world.play();
    assert_eq!(world.mode, PlayMode::Playing);
    world.read_controls();
    world.step(0.1);
    assert_eq!(world.controls.motors, (1.0, 0.25), "{:?}", world.play_log);
    assert!(world.controls.capture.is_some(), "{:?}", world.play_log);
    // The pad's north button becomes jump's second binding.
    world.controls.raw.press(pad("pad.north"));
    world.read_controls();
    world.step(0.1);
    assert!(world.play_log.iter().any(|l| l == "rebound"), "{:?}", world.play_log);
    assert!(world.play_log.iter().any(|l| l == "jump is now Pad Y (top)"), "{:?}", world.play_log);
    world.read_controls();
    world.step(0.1);
    assert_eq!(world.play_log.last().map(String::as_str), Some("true"), "{:?}", world.play_log);
    for _ in 0..5 {
        world.read_controls();
        world.step(0.1);
    }
    assert_eq!(world.controls.motors, (0.0, 0.0), "the rumble ran out");
    world.controls.raw.press(pad("pad.east"));
    world.stop();
    assert_eq!(world.controls.motors, (0.0, 0.0));
    let saved = std::fs::read_to_string(dir.join(controls::PLAYER_FILE)).unwrap();
    assert!(saved.contains("pad.north"), "{saved}");
    std::fs::remove_dir_all(dir).ok();
}
