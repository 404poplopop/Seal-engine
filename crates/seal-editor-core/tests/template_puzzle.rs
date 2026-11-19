//! Puzzle starter template: files, loads, and scripted play of all three levels.

use std::path::{Path, PathBuf};

use seal_editor_core::{
    apply_template, collect_log_lines, create_project_with_options, open_project, template_puzzle,
    CommandArgs, CreateProjectOptions, EditorSession, Layout, World,
};

#[test]
fn puzzle_template_graph_parses_and_is_stable() {
    let source = include_str!("../src/template_puzzle.rs");
    assert!(source.contains("fn build_game_graph()"));
    assert!(!source.contains("GraphGen"));
    let first = template_puzzle::build_game_graph();
    let second = template_puzzle::build_game_graph();
    assert_eq!(first, second, "graph generation is deterministic");
    assert!(first.contains("list<int>"), "board is a list");
    assert!(first.contains("kind = \"for\""), "cells are visited in a loop");
    if let Some(err) = template_puzzle::game_graph_error() {
        panic!("{err}");
    }
    let tpl = seal_editor_core::template_by_id("puzzle").expect("registered");
    assert_eq!(tpl.id, "puzzle");
    assert_eq!(tpl.title_key, "wizard-template-puzzle");
}

#[test]
fn puzzle_project_opens_clean_and_three_levels_play() {
    let first = playthrough("a");
    let second = playthrough("b");
    assert_eq!(first, second, "two plays produce the same boards, counters and unlocks");
}

#[test]
fn puzzle_screen_loader_rejects_a_broken_copy() {
    let dir = scratch("broken-ui");
    create_puzzle_project(&dir);
    std::fs::write(dir.join("ui/main_menu.sealui"), "seal ui 1\nbutton\n").unwrap();
    let err = template_puzzle::validate_assets(&dir).unwrap_err();
    assert!(err.contains("line"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn puzzle_music_follows_the_slider_and_the_menu_quits() {
    use seal_world::scripts::MusicCue;
    let dir = scratch("music");
    create_puzzle_project(&dir);
    let mut world = load_world(&dir);
    assert_eq!(world.music, "audio/music.wav");
    let music =
        seal_audio::Sound::from_wav(&std::fs::read(dir.join("audio/music.wav")).unwrap()).unwrap();
    assert!(music.frames() > 22050 * 3, "a loop of several seconds");
    boot(&mut world);
    let volumes = |w: &World| -> Vec<f32> {
        w.play_music
            .iter()
            .filter_map(|c| if let MusicCue::Volume { volume } = c { Some(*volume) } else { None })
            .collect()
    };
    assert!(world
        .play_music
        .iter()
        .any(|c| matches!(c, MusicCue::Play { file, .. } if file == "audio/music.wav")));
    assert_eq!(volumes(&world).last().copied(), Some(0.8), "{:?}", world.play_music);
    click(&mut world, "Options");
    let slider = center(&world, "MusicVol");
    world.game_screens.pointer_down([slider[0] - 60.0, slider[1]]);
    world.game_screens.pointer_up([slider[0] - 60.0, slider[1]]);
    settle(&mut world);
    let low = volumes(&world).last().copied().unwrap();
    assert!(low < 0.8, "the slider lowered the music: {:?}", volumes(&world));
    click(&mut world, "OptionsBack");
    click(&mut world, "Quit");
    assert_eq!(world.mode, seal_world::PlayMode::Editing, "{:?}", world.play_log);
    let _ = std::fs::remove_dir_all(&dir);
}

fn playthrough(tag: &str) -> Vec<String> {
    let dir = scratch(tag);
    create_puzzle_project(&dir);
    template_puzzle::validate_assets(&dir).expect("assets");
    open_project(&dir).expect("opens");
    assert_clean_open(&dir);

    let mut world = load_world(&dir);
    let mut trace = Vec::new();
    boot(&mut world);
    assert!(shown(&world).contains(&"main_menu".to_string()), "{}", shown_list(&world));

    click(&mut world, "LevelSelect");
    assert_eq!(ui(&world, "Level1"), "Level 1");
    assert_eq!(ui(&world, "Level2"), "Level 2 Locked");
    assert_eq!(ui(&world, "Level3"), "Level 3 Locked");
    click(&mut world, "Level2");
    assert!(
        shown(&world).contains(&"level_select".to_string()),
        "locked level stays on the select screen"
    );
    assert!(!shown(&world).iter().any(|s| s == "level_play"));
    click(&mut world, "Back");

    click(&mut world, "Play");
    note(&mut trace, &world, "l1-start");
    assert_eq!(board(&world), "101000101");
    assert_eq!(ui(&world, "MovesLabel"), "Moves: 0");
    assert_eq!(ui(&world, "LevelLabel"), "Level 1");

    click(&mut world, "Cell1");
    note(&mut trace, &world, "l1-press0");
    assert_eq!(board(&world), "011100101");
    assert_eq!(ui(&world, "MovesLabel"), "Moves: 1");
    assert_eq!(var(&world, "history"), "[0]");

    click(&mut world, "UndoBtn");
    note(&mut trace, &world, "l1-undo");
    assert_eq!(board(&world), "101000101");
    assert_eq!(ui(&world, "MovesLabel"), "Moves: 0");
    assert_eq!(var(&world, "history"), "[]");

    click(&mut world, "Cell1");
    click(&mut world, "RestartBtn");
    note(&mut trace, &world, "l1-restart");
    assert_eq!(board(&world), "101000101");
    assert_eq!(ui(&world, "MovesLabel"), "Moves: 0");
    assert_eq!(var(&world, "moves"), "0");

    focus_press(&mut world, "Cell5", false);
    note(&mut trace, &world, "l1-win");
    assert_eq!(board(&world), "111111111");
    assert_eq!(ui(&world, "MovesLabel"), "Moves: 1");
    assert_eq!(ui(&world, "NextLevel"), "Next Level");
    assert_eq!(unlocked(&world), "2");
    assert_save(&dir, "2");

    click(&mut world, "NextLevel");
    note(&mut trace, &world, "l2-start");
    assert_eq!(board(&world), "000111000");
    assert_eq!(ui(&world, "MovesLabel"), "Moves: 0");
    assert_eq!(ui(&world, "LevelLabel"), "Level 2");
    assert!(!shown(&world).iter().any(|s| s == "win_panel"));

    click(&mut world, "Cell2");
    note(&mut trace, &world, "l2-a");
    assert_eq!(board(&world), "111101000");
    assert_eq!(ui(&world, "MovesLabel"), "Moves: 1");
    focus_press(&mut world, "Cell8", true);
    note(&mut trace, &world, "l2-win");
    assert_eq!(board(&world), "111111111");
    assert_eq!(ui(&world, "MovesLabel"), "Moves: 2");
    assert_eq!(unlocked(&world), "3");
    assert_save(&dir, "3");

    click(&mut world, "NextLevel");
    note(&mut trace, &world, "l3-start");
    assert_eq!(board(&world), "000010101");
    assert_eq!(ui(&world, "LevelLabel"), "Level 3");
    assert_eq!(ui(&world, "MovesLabel"), "Moves: 0");

    click(&mut world, "Cell2");
    note(&mut trace, &world, "l3-a");
    assert_eq!(board(&world), "111000101");
    assert_eq!(ui(&world, "MovesLabel"), "Moves: 1");
    click(&mut world, "Cell7");
    note(&mut trace, &world, "l3-b");
    assert_eq!(board(&world), "111100011");
    assert_eq!(ui(&world, "MovesLabel"), "Moves: 2");
    click(&mut world, "UndoBtn");
    note(&mut trace, &world, "l3-undo");
    assert_eq!(board(&world), "111000101");
    assert_eq!(ui(&world, "MovesLabel"), "Moves: 1");
    click(&mut world, "Cell7");
    click(&mut world, "Cell8");
    note(&mut trace, &world, "l3-c");
    assert_eq!(board(&world), "111110100");
    assert_eq!(ui(&world, "MovesLabel"), "Moves: 3");
    click(&mut world, "Cell9");
    note(&mut trace, &world, "l3-win");
    assert_eq!(board(&world), "111111111");
    assert_eq!(ui(&world, "MovesLabel"), "Moves: 4");
    assert_eq!(ui(&world, "NextLevel"), "Back to Menu");
    assert_eq!(unlocked(&world), "3");
    assert_save(&dir, "3");

    click(&mut world, "NextLevel");
    note(&mut trace, &world, "menu");
    assert!(shown(&world).contains(&"main_menu".to_string()), "{}", shown_list(&world));
    assert!(!shown(&world).iter().any(|s| s == "win_panel" || s == "level_play"));

    click(&mut world, "LevelSelect");
    assert_eq!(ui(&world, "Level2"), "Level 2");
    assert_eq!(ui(&world, "Level3"), "Level 3");
    note(&mut trace, &world, "select-open");

    let bad: Vec<_> = world
        .play_log
        .iter()
        .filter(|line| {
            line.contains("script on") || line.contains("cannot read") || line.contains("error")
        })
        .cloned()
        .collect();
    assert!(bad.is_empty(), "play log problems: {bad:?}\nfull={:?}", world.play_log);

    let _ = std::fs::remove_dir_all(&dir);
    trace
}

fn note(trace: &mut Vec<String>, world: &World, step: &str) {
    let board =
        if world.game_screens.text_of("Cell1").is_some() { board(world) } else { "hidden".into() };
    let moves = world.game_screens.text_of("MovesLabel").unwrap_or_default();
    trace.push(format!(
        "{step} board={board} moves={moves} unlocked={} next={} shown={}",
        unlocked(world),
        world.game_screens.text_of("NextLevel").unwrap_or_default(),
        shown_list(world)
    ));
}

fn assert_clean_open(dir: &Path) {
    let before = collect_log_lines(0).len();
    let mut session = EditorSession::new(Layout::default());
    session.recent_path = Some(dir.join("recent.seal"));
    session
        .dispatch(
            "file.open-project",
            &CommandArgs::new(&[("path", dir.to_str().expect("utf8 path"))]),
        )
        .expect("editor opens the project");
    let folder = dir.to_string_lossy();
    let bad: Vec<_> = collect_log_lines(before)
        .into_iter()
        .filter(|line| line.level == "error" || line.level == "warn")
        .filter(|line| line.message.contains(folder.as_ref()) || line.message.contains("Lights"))
        .map(|line| format!("{} {}", line.level, line.message))
        .collect();
    assert!(bad.is_empty(), "console: {bad:?}");
    let script = session
        .world
        .entities
        .iter()
        .find(|e| e.name == "Game")
        .map(|e| e.script.clone())
        .unwrap_or_default();
    assert_eq!(script, "logic/game.graph");
    session.dispatch("file.close-project", &CommandArgs::default()).ok();
}

fn assert_save(dir: &Path, unlocked_value: &str) {
    let text =
        std::fs::read_to_string(dir.join("saved/games/progress.sealsave")).expect("progress save");
    let needle = format!("unlocked = \"{unlocked_value}\"");
    assert!(text.contains(&needle), "save missing {needle}:\n{text}");
}

fn create_puzzle_project(dir: &Path) {
    create_project_with_options(dir, "Lights", &CreateProjectOptions::default()).unwrap();
    apply_template(dir, "Lights", "puzzle", &CreateProjectOptions::default()).unwrap();
}

fn load_world(dir: &Path) -> World {
    let text = std::fs::read_to_string(dir.join("scene.seal")).unwrap();
    let mut world = World::from_text(&text).unwrap();
    world.project_dir = Some(dir.to_path_buf());
    world
}

fn boot(world: &mut World) {
    world.play();
    world.game_screens.resize([1280.0, 720.0]);
    for _ in 0..3 {
        world.step(1.0 / 60.0);
    }
    assert!(world.has_scripts(), "play started the graph");
}

fn settle(world: &mut World) {
    world.step(1.0 / 60.0);
    world.step(1.0 / 60.0);
}

fn click(world: &mut World, name: &str) {
    let at = center(world, name);
    world.game_screens.pointer_down(at);
    world.game_screens.pointer_up(at);
    settle(world);
}

fn focus_press(world: &mut World, name: &str, gamepad: bool) {
    let (up, down, left, right, confirm) = if gamepad {
        ("DPadUp", "DPadDown", "DPadLeft", "DPadRight", "GamepadA")
    } else {
        ("Up", "Down", "Left", "Right", "Enter")
    };
    world.game_screens.key("Escape");
    world.game_screens.key(down);
    let dirs = [down, right, up, left];
    for _ in 0..32 {
        if focused(world).as_deref() == Some(name) {
            world.game_screens.key(confirm);
            settle(world);
            return;
        }
        let before = focused(world);
        world.game_screens.key(best_dir(world, name, up, down, left, right));
        if focused(world) == before {
            for dir in dirs {
                world.game_screens.key(dir);
                if focused(world).as_deref() == Some(name) {
                    break;
                }
                if focused(world) != before {
                    break;
                }
            }
        }
    }
    panic!("could not focus {name}, stuck on {:?} shown {}", focused(world), shown_list(world));
}

fn best_dir<'a>(
    world: &World,
    name: &str,
    up: &'a str,
    down: &'a str,
    left: &'a str,
    right: &'a str,
) -> &'a str {
    let Some(from) = focused_center(world) else { return down };
    let to = center(world, name);
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    if dy.abs() >= dx.abs() {
        if dy >= 0.0 {
            down
        } else {
            up
        }
    } else if dx >= 0.0 {
        right
    } else {
        left
    }
}

fn focused(world: &World) -> Option<String> {
    let (_, live) = world.game_screens.shown.last()?;
    let index = live.focus?;
    Some(live.screen.nodes.get(index)?.name.clone())
}

fn focused_center(world: &World) -> Option<[f32; 2]> {
    let (_, live) = world.game_screens.shown.last()?;
    let index = live.focus?;
    Some(live.layout().rect(index)?.center())
}

fn center(world: &World, name: &str) -> [f32; 2] {
    for (_, live) in world.game_screens.shown.iter().rev() {
        if let Some(index) = live.screen.find(name) {
            if let Some(rect) = live.layout().rect(index) {
                return rect.center();
            }
        }
    }
    panic!("no control {name} on {}", shown_list(world));
}

fn board(world: &World) -> String {
    let mut out = String::new();
    for cell in 1..=9 {
        match ui(world, &format!("Cell{cell}")).as_str() {
            "On" => out.push('1'),
            "Off" => out.push('0'),
            other => panic!("Cell{cell} is {other:?}, log={:?}", world.play_log),
        }
    }
    let listed = var(world, "board");
    let expect =
        format!("[{}]", out.chars().map(|ch| ch.to_string()).collect::<Vec<_>>().join(", "));
    assert_eq!(listed, expect, "list variable and cell labels diverged");
    out
}

fn ui(world: &World, name: &str) -> String {
    world
        .game_screens
        .text_of(name)
        .unwrap_or_else(|| panic!("missing text {name} on {}", shown_list(world)))
}

fn unlocked(world: &World) -> String {
    world.play_game().and_then(|game| game.values.get("unlocked").cloned()).unwrap_or_default()
}

fn var(world: &World, name: &str) -> String {
    world
        .debug_view("logic/game.graph")
        .and_then(|view| view.vars.into_iter().find(|(n, _)| n == name).map(|(_, v)| v))
        .unwrap_or_default()
}

fn shown(world: &World) -> Vec<String> {
    world.game_screens.shown.iter().map(|(name, _)| name.clone()).collect()
}

fn shown_list(world: &World) -> String {
    shown(world).join(",")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "seal-puzzle-tpl-{name}-{}-{}",
        std::process::id(),
        name.len()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}
