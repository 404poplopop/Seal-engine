//! A script can hold the world still, leave play, and a gamepad can drive a menu.
//! Side-view `play_input` still moves the character while that project's scripts run.

use std::path::PathBuf;

use seal_input::Button;
use seal_world::{PlayInput, PlayMode, World};

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("seal-menu-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn platformer(script: &str) -> World {
    let mut w = World::demo();
    let gone: Vec<String> = w
        .entities
        .iter()
        .filter(|e| e.parent.as_deref() == Some("root"))
        .map(|e| e.id.clone())
        .collect();
    for id in gone {
        w.remove(&id);
    }
    for x in 0..8 {
        let id = w.add("empty");
        let e = w.entity_mut(&id).unwrap();
        e.tag = "ground".into();
        e.translation = [x as f32 + 0.5, 0.5, 0.0];
    }
    let id = w.add("empty");
    let e = w.entity_mut(&id).unwrap();
    e.name = "Hero".into();
    e.tag = "player".into();
    e.sprite = "sprites/hero.png".into();
    e.translation = [1.0, 1.45, 0.0];
    e.script = script.into();
    w
}

const RULES: &str = r#"schema_version = 1
seal_graph = "rules"
vars = []
nodes = [
  { id = 1, kind = "event", name = "key_P_pressed" },
  { id = 2, kind = "set_game_state", defaults = { state = "paused" } },
  { id = 3, kind = "event", name = "key_R_pressed" },
  { id = 4, kind = "set_game_state", defaults = { state = "playing" } },
  { id = 5, kind = "event", name = "key_Q_pressed" },
  { id = 6, kind = "set_game_state", defaults = { state = "quit" } },
]
links = [ "1.then -> 2.in", "3.then -> 4.in", "5.then -> 6.in" ]
"#;

fn x_of(w: &World) -> f32 {
    w.entities.iter().find(|e| e.tag == "player").unwrap().translation[0]
}

#[test]
fn play_input_moves_while_scripts_run_and_pause_freezes_until_resume_or_quit() {
    let dir = dir("pause");
    std::fs::create_dir_all(dir.join("logic")).unwrap();
    std::fs::write(dir.join("logic/rules.graph"), RULES).unwrap();
    let mut w = platformer("logic/rules.graph");
    w.project_dir = Some(dir.clone());
    w.play();
    assert!(w.play_log.is_empty(), "{:?}", w.play_log);
    let x0 = x_of(&w);
    for _ in 0..20 {
        w.play_input = PlayInput { right: true, ..Default::default() };
        w.step(1.0 / 60.0);
    }
    let x1 = x_of(&w);
    assert!(x1 > x0 + 0.4, "scripts on, play_input still moves: {x0} -> {x1}");

    w.play_keys.push(("P".into(), true));
    w.play_input = PlayInput { right: true, ..Default::default() };
    w.step(1.0 / 60.0);
    assert_eq!(w.play_game().unwrap().state, "paused");
    let held = x_of(&w);
    let t = w.play_time;
    for _ in 0..30 {
        w.play_input = PlayInput { right: true, jump: true, ..Default::default() };
        w.step(1.0 / 60.0);
    }
    assert_eq!(x_of(&w), held, "paused world does not move");
    assert_eq!(w.play_time, t, "paused clock does not advance");

    w.play_keys.push(("R".into(), true));
    w.play_input = PlayInput { right: true, ..Default::default() };
    w.step(1.0 / 60.0);
    assert_eq!(w.play_game().unwrap().state, "playing");
    let x2 = x_of(&w);
    for _ in 0..15 {
        w.play_input = PlayInput { right: true, ..Default::default() };
        w.step(1.0 / 60.0);
    }
    assert!(x_of(&w) > x2 + 0.3, "resume moves again");

    w.play_keys.push(("Q".into(), true));
    w.step(1.0 / 60.0);
    assert_eq!(w.mode, PlayMode::Editing);
    let _ = std::fs::remove_dir_all(&dir);
}

const MENU: &str = r#"schema_version = 1
seal_graph = "menu"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "ui_show", defaults = { widget = "Menu", visible = true } },
  { id = 3, kind = "event", name = "ui_Two_clicked" },
  { id = 4, kind = "log", defaults = { message = "two" } },
]
links = [ "1.then -> 2.in", "3.then -> 4.in" ]
"#;

const MENU_UI: &str = r#"seal ui 1
panel Root
  anchor 0 0 1 1
  vbox Column
    button One
    button Two
"#;

fn press(w: &mut World, button: &str) {
    w.controls.raw.press(Button::parse(button).unwrap());
}

#[test]
fn gamepad_moves_menu_focus_and_activates_the_button() {
    let dir = dir("pad");
    std::fs::create_dir_all(dir.join("logic")).unwrap();
    std::fs::create_dir_all(dir.join("ui")).unwrap();
    std::fs::write(dir.join("logic/menu.graph"), MENU).unwrap();
    std::fs::write(dir.join("ui/Menu.sealui"), MENU_UI).unwrap();
    let mut w = World::demo();
    w.entities[0].script = "logic/menu.graph".into();
    w.project_dir = Some(dir.clone());
    w.play();
    w.step(1.0 / 60.0);
    assert!(
        w.game_screens.active(),
        "menu is open; log {:?} missing {:?}",
        w.play_log,
        w.game_screens.missing
    );
    // A menu focuses its first button when it opens.
    let live = &w.game_screens.shown.last().unwrap().1;
    let name = live.screen.nodes[live.focus.expect("focused")].name.clone();
    assert_eq!(name, "One");

    press(&mut w, "pad.dpad_down");
    w.step(1.0 / 60.0);
    let live = &w.game_screens.shown.last().unwrap().1;
    let name = live.screen.nodes[live.focus.expect("focused")].name.clone();
    assert_eq!(name, "Two");

    w.controls.raw.clear_device("pad");
    w.step(1.0 / 60.0);
    press(&mut w, "pad.south");
    w.step(1.0 / 60.0);
    assert!(w.play_log.iter().any(|l| l == "two"), "{:?}", w.play_log);
    let _ = std::fs::remove_dir_all(&dir);
}
