//! Top-down adventure starter template: files, loading, play and determinism.

use seal_editor_core::collect_log_lines;
use seal_editor_core::template_topdown;
use seal_editor_core::{create_project, CommandArgs, EditorSession, Layout, PlayMode};
use seal_world::World;
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

const ASSETS: &[&str] = &[
    "scene.seal",
    "scenes/cave.seal",
    "input.sealinput",
    "sprites/grass.png",
    "sprites/hero_sheet.png",
    "sprites/heart_empty.png",
    "sprites/enemy_sheet.png",
    "sprites/npc_sheet.png",
    "anims/hero.frames",
    "anims/enemy.frames",
    "anims/npc.frames",
    "ui/Hud.sealui",
    "ui/Pause.sealui",
    "ui/Dialog.sealui",
    "logic/player.graph",
    "logic/pickup.graph",
    "logic/enemy.graph",
    "logic/npc.graph",
    "logic/door_village.graph",
    "logic/door_cave.graph",
    "logic/save.graph",
    "logic/game.graph",
    "audio/pickup.wav",
    "audio/hurt.wav",
    "audio/save.wav",
];

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("seal-template-topdown-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn fnv(bytes: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut h = FNV_OFFSET;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

fn play_digest(w: &World) -> u64 {
    let mut text = String::new();
    if let Some(game) = w.play_game() {
        for (k, v) in &game.scores {
            text.push_str(&format!("{k}={v};"));
        }
    }
    let mut names: Vec<_> = w.entities.iter().map(|e| e.name.as_str()).collect();
    names.sort();
    for name in names {
        if let Some(e) = w.entities.iter().find(|e| e.name == name) {
            if e.hidden {
                continue;
            }
            text.push_str(&format!(
                "{}@{:.2},{:.2};",
                e.name, e.translation[0], e.translation[1]
            ));
        }
    }
    fnv(text.as_bytes())
}

fn script_ok(w: &World) {
    let bad: Vec<_> = w
        .play_log
        .iter()
        .filter(|l| l.contains("script on") || l.contains("cannot set") || l.contains("play_clip:"))
        .cloned()
        .collect();
    assert!(bad.is_empty(), "{bad:?}");
}

fn score(w: &World, name: &str) -> i64 {
    w.play_game().and_then(|g| g.scores.get(name)).copied().unwrap_or(0)
}

fn hero(w: &World) -> &seal_world::Entity {
    w.entities.iter().find(|e| e.tag == "player").expect("hero")
}

fn place(w: &mut World, name: &str, x: f32, y: f32) {
    let id = w.entities.iter().find(|e| e.name == name).map(|e| e.id.clone()).expect(name);
    let e = w.entity_mut(&id).unwrap();
    e.translation = [x, y, 0.0];
    e.velocity = [0.0, 0.0, 0.0];
}

fn pump(w: &mut World, keys: &[&str], frames: u32) {
    for _ in 0..frames {
        w.controls.set_keys(keys.iter().copied());
        w.read_controls();
        w.step(1.0 / 60.0);
    }
}

fn shown(w: &World, widget: &str) -> bool {
    w.game_screens.shown.iter().any(|(name, _)| name == widget)
}

fn node_text(w: &World, widget: &str, node: &str) -> String {
    w.game_screens
        .shown
        .iter()
        .find(|(name, _)| name == widget)
        .and_then(|(_, live)| live.screen.find(node).map(|i| live.screen.nodes[i].text.clone()))
        .unwrap_or_default()
}

fn node_image(w: &World, widget: &str, node: &str) -> String {
    w.game_screens
        .shown
        .iter()
        .find(|(name, _)| name == widget)
        .and_then(|(_, live)| live.screen.find(node).map(|i| live.screen.nodes[i].image.clone()))
        .unwrap_or_default()
}

fn focus_name(w: &World) -> String {
    let Some((_, live)) = w.game_screens.shown.last() else { return String::new() };
    live.focus.map(|i| live.screen.nodes[i].name.clone()).unwrap_or_default()
}

fn project(tag: &str) -> (PathBuf, World) {
    let dir = scratch(tag);
    create_project(&dir, "Adventure").unwrap();
    template_topdown::create(&dir, "Adventure").unwrap();
    let mut w = World::from_text(&fs::read_to_string(dir.join("scene.seal")).unwrap()).unwrap();
    w.project_dir = Some(dir.clone());
    (dir, w)
}

fn walk_pickup(w: &mut World) {
    w.play();
    assert_eq!(w.mode, PlayMode::Playing);
    for _ in 0..180 {
        w.controls.set_keys(["d"]);
        w.read_controls();
        w.step(1.0 / 60.0);
        let gems = w.play_game().and_then(|g| g.scores.get("gems")).copied().unwrap_or(0);
        if gems >= 1 {
            break;
        }
    }
    let gems = w.play_game().and_then(|g| g.scores.get("gems")).copied().unwrap_or(0);
    assert!(gems >= 1, "collected a gem: gems={gems}, log={:?}", w.play_log);
}

#[test]
fn template_writes_every_file_and_opens_cleanly() {
    let dir = scratch("open");
    create_project(&dir, "Adventure").unwrap();
    template_topdown::create(&dir, "Adventure").unwrap();
    for rel in ASSETS {
        assert!(dir.join(rel).is_file(), "missing {rel}");
    }
    let scene = fs::read_to_string(dir.join("scene.seal")).unwrap();
    World::from_text(&scene).expect("village scene loads");
    World::from_text(&fs::read_to_string(dir.join("scenes/cave.seal")).unwrap()).expect("cave loads");
    let mut session = EditorSession::new(Layout::default());
    let path = dir.to_string_lossy();
    session
        .dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_ref())]))
        .expect("open project");
    let console: Vec<_> = collect_log_lines(session.foundation.console_clear_before);
    let bad: Vec<_> = console
        .iter()
        .filter(|l| l.level == "error" || l.level == "warn")
        .collect();
    assert!(bad.is_empty(), "console: {bad:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn play_collects_a_gem_deterministically() {
    let dir = scratch("play");
    create_project(&dir, "Adventure").unwrap();
    template_topdown::create(&dir, "Adventure").unwrap();
    assert!(dir.join("logic/player.graph").is_file());
    let mut a = World::from_text(&fs::read_to_string(dir.join("scene.seal")).unwrap()).unwrap();
    a.project_dir = Some(dir.clone());
    walk_pickup(&mut a);
    let hash_a = play_digest(&a);
    a.stop();

    let mut b = World::from_text(&fs::read_to_string(dir.join("scene.seal")).unwrap()).unwrap();
    b.project_dir = Some(dir.clone());
    walk_pickup(&mut b);
    let hash_b = play_digest(&b);
    assert_eq!(hash_a, hash_b, "same play state twice");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_village_door_opens_the_cave_scene() {
    let dir = scratch("door");
    create_project(&dir, "Adventure").unwrap();
    template_topdown::create(&dir, "Adventure").unwrap();
    let mut w = World::from_text(&fs::read_to_string(dir.join("scene.seal")).unwrap()).unwrap();
    w.project_dir = Some(dir.clone());
    w.play();
  // Nudge the hero into the cave door on the east path.
    if let Some(hero) = w.entities.iter().find(|e| e.tag == "player").map(|e| e.id.clone()) {
        w.entity_mut(&hero).unwrap().translation = [5.2, 0.0, 0.0];
    }
    for _ in 0..30 {
        w.step(1.0 / 60.0);
    }
    assert_eq!(
        w.scenes.open,
        "scenes/cave.seal",
        "door opened the cave: open={} log={:?}",
        w.scenes.open,
        w.play_log
    );
    let names: BTreeSet<_> = w.entities.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains("Exit"), "cave scene objects: {names:?}");
    assert!(names.contains("Hero"), "the player stays across the door");
    let x = hero(&w).translation[0];
    assert!((x - -2.4).abs() < 0.35, "arrived in front of the exit, x={x}");
    place(&mut w, "Hero", -3.2, 0.0);
    for _ in 0..30 {
        w.step(1.0 / 60.0);
    }
    assert_eq!(w.scenes.open, "scene.seal", "the exit returns to the village: {:?}", w.play_log);
    let x = hero(&w).translation[0];
    assert!((x - 4.4).abs() < 0.35, "arrived in front of the village door, x={x}");
    assert!(w.entities.iter().any(|e| e.name == "Cave Door"));
    script_ok(&w);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn walking_each_way_selects_that_clip_and_idle_keeps_the_facing() {
    let (dir, mut w) = project("clips");
    w.play();
    pump(&mut w, &[], 3);
    assert_eq!(hero(&w).animation, "idle_down", "log={:?}", w.play_log);
    pump(&mut w, &["d"], 3);
    assert_eq!(hero(&w).animation, "walk_right");
    pump(&mut w, &[], 3);
    assert_eq!(hero(&w).animation, "idle_right");
    pump(&mut w, &["a"], 3);
    assert_eq!(hero(&w).animation, "walk_left");
    pump(&mut w, &[], 3);
    assert_eq!(hero(&w).animation, "idle_left");
    pump(&mut w, &["w"], 3);
    assert_eq!(hero(&w).animation, "walk_up");
    pump(&mut w, &[], 3);
    assert_eq!(hero(&w).animation, "idle_up");
    pump(&mut w, &["s"], 3);
    assert_eq!(hero(&w).animation, "walk_down");
    pump(&mut w, &[], 3);
    assert_eq!(hero(&w).animation, "idle_down");
    let before = hero(&w).translation[0];
    w.controls.raw.set_axis(seal_world::seal_input::Axis::parse("pad.left_x").unwrap(), 1.0);
    pump(&mut w, &[], 20);
    w.controls.raw.set_axis(seal_world::seal_input::Axis::parse("pad.left_x").unwrap(), 0.0);
    assert!(hero(&w).translation[0] > before + 0.4, "the stick moves the player");
    script_ok(&w);
    w.stop();
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn pause_stops_the_world_and_resume_restart_and_quit_work() {
    let (dir, mut w) = project("pause");
    w.play();
    pump(&mut w, &[], 2);
    pump(&mut w, &["escape"], 1);
    pump(&mut w, &[], 1);
    assert_eq!(w.time_scale, 0.0, "pause stops time: {:?}", w.play_log);
    assert!(shown(&w, "Pause"));
    assert_eq!(focus_name(&w), "Resume");
    let x = hero(&w).translation[0];
    let t = w.play_time;
    pump(&mut w, &["d"], 20);
    assert_eq!(hero(&w).translation[0], x, "a held move does nothing while paused");
    assert_eq!(w.play_time, t);
    w.game_screens.key("Down");
    assert_eq!(focus_name(&w), "Restart");
    w.game_screens.key("Up");
    assert_eq!(focus_name(&w), "Resume");
    let down = seal_world::seal_input::Button::parse("pad.dpad_down").unwrap();
    w.controls.raw.press(down.clone());
    pump(&mut w, &[], 1);
    w.controls.raw.release(&down);
    pump(&mut w, &[], 1);
    assert_eq!(focus_name(&w), "Restart");
    let south = seal_world::seal_input::Button::parse("pad.south").unwrap();
    w.controls.raw.press(south.clone());
    pump(&mut w, &[], 2);
    w.controls.raw.release(&south);
    assert_eq!(w.time_scale, 1.0, "restart clears the pause: {:?}", w.play_log);
    assert!(!shown(&w, "Pause"));
    assert_eq!(score(&w, "hearts"), 3);
    let x = hero(&w).translation[0];
    assert!((x - -4.5).abs() < 0.4, "restart puts the player at the village start, x={x}");

    pump(&mut w, &["escape"], 1);
    pump(&mut w, &[], 1);
    assert_eq!(w.time_scale, 0.0);
    pump(&mut w, &["escape"], 1);
    pump(&mut w, &[], 2);
    assert_eq!(w.time_scale, 1.0, "the pause action resumes");
    let x = hero(&w).translation[0];
    pump(&mut w, &["d"], 15);
    assert!(hero(&w).translation[0] > x + 0.3, "resume lets the player walk");

    pump(&mut w, &["escape"], 1);
    pump(&mut w, &[], 1);
    w.game_screens.key("Down");
    w.game_screens.key("Down");
    assert_eq!(focus_name(&w), "Quit");
    w.game_screens.key("Enter");
    pump(&mut w, &[], 2);
    assert!(w.quit, "quit leaves play");
    assert_eq!(w.mode, PlayMode::Editing);
    script_ok(&w);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn hearts_follow_damage_and_zero_restarts_without_a_save() {
    let (dir, mut w) = project("hearts");
    w.play();
    pump(&mut w, &[], 2);
    assert_eq!(score(&w, "hearts"), 3);
    assert_eq!(node_image(&w, "Hud", "H3"), "sprites/heart.png");
    hurt(&mut w);
    assert_eq!(score(&w, "hearts"), 2, "log={:?}", w.play_log);
    pump(&mut w, &[], 2);
    assert_eq!(node_image(&w, "Hud", "H3"), "sprites/heart_empty.png");
    let hearts = score(&w, "hearts");
    hurt(&mut w);
    assert_eq!(score(&w, "hearts"), hearts, "a second touch inside the safe time does nothing");
    stand_off(&mut w, 40);
    hurt(&mut w);
    assert_eq!(score(&w, "hearts"), 1);
    stand_off(&mut w, 40);
    let at = enemy_at(&w, "Enemy 1");
    place(&mut w, "Hero", at[0], at[1]);
    pump(&mut w, &[], 2);
    assert_eq!(score(&w, "hearts"), 0, "the third hit empties the hearts: {:?}", w.play_log);
    for _ in 0..20 {
        w.read_controls();
        w.step(1.0 / 60.0);
        if score(&w, "hearts") == 3 {
            break;
        }
    }
    assert_eq!(score(&w, "hearts"), 3, "zero hearts restarts: {:?}", w.play_log);
    assert_eq!(w.scenes.open, "scene.seal");
    let at = hero(&w).translation;
    assert!((at[0] - -4.5).abs() < 0.4 && at[1].abs() < 0.4, "back at the start, at={at:?}");
    assert!(w.entities.iter().any(|e| e.name == "Gem 1"));
    script_ok(&w);
    w.stop();
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_start_is_fair_and_every_actor_stands_on_the_tiles() {
    let (dir, mut w) = project("fair");
    let folder = w.entities.iter().find(|e| e.name == "Tiles" && e.tag == "folder").expect("Tiles folder").id.clone();
    let tiles: Vec<_> = w.entities.iter().filter(|e| e.tag == "floor").collect();
    assert_eq!(tiles.len(), 13 * 7);
    assert!(tiles.iter().all(|e| e.parent.as_deref() == Some(folder.as_str())), "every tile sits in the Tiles folder");
    let (lo, hi) = tiles.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), e| {
        let t = e.translation;
        ([lo[0].min(t[0] - 0.5), lo[1].min(t[1] - 0.5)], [hi[0].max(t[0] + 0.5), hi[1].max(t[1] + 0.5)])
    });
    for e in w.entities.iter().filter(|e| matches!(e.tag.as_str(), "player" | "enemy" | "pickup" | "npc" | "door" | "save")) {
        let t = e.translation;
        assert!(t[0] > lo[0] && t[0] < hi[0] && t[1] > lo[1] && t[1] < hi[1], "{} at {t:?} is off the tiles {lo:?}..{hi:?}", e.name);
    }
    w.play();
    pump(&mut w, &[], 240);
    assert_eq!(score(&w, "hearts"), 3, "four idle seconds cost no hearts: {:?}", w.play_log);
    w.stop();
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_soundset_plays_one_of_its_files_during_play() {
    let (dir, mut w) = project("soundset");
    fs::write(dir.join("audio/pickup.soundset"), "# Seal sound container\nmode random\nseed 7\nvolume 1\nentry pickup.wav\nentry save.wav\n").unwrap();
    let graph = dir.join("logic/pickup.graph");
    let text = fs::read_to_string(&graph).unwrap();
    assert!(text.contains("audio/pickup.wav"));
    fs::write(&graph, text.replace("audio/pickup.wav", "audio/pickup.soundset")).unwrap();
    walk_pickup(&mut w);
    let sounds: Vec<_> = w.play_sounds.iter().map(|c| c.sound.replace('\\', "/")).collect();
    assert!(
        sounds.iter().any(|s| s == "audio/pickup.wav" || s == "audio/save.wav"),
        "the set resolved to a file next to it: {sounds:?} log={:?}",
        w.play_log
    );
    assert!(sounds.iter().all(|s| !s.ends_with(".soundset")), "{sounds:?}");
    script_ok(&w);
    w.stop();
    let _ = fs::remove_dir_all(&dir);
}

fn enemy_at(w: &World, name: &str) -> [f32; 3] {
    w.entities.iter().find(|e| e.name == name).expect(name).translation
}

fn stand_off(w: &mut World, frames: u32) {
    place(w, "Hero", -20.0, 20.0);
    pump(w, &[], frames);
}

fn hurt(w: &mut World) {
    let at = enemy_at(w, "Enemy 1");
    place(w, "Hero", at[0], at[1]);
    pump(w, &[], 4);
    place(w, "Hero", at[0] + 3.0, at[1] + 3.0);
    pump(w, &[], 8);
}

#[test]
fn gems_npc_and_enemies_play() {
    let (dir, mut w) = project("actors");
    w.play();
    pump(&mut w, &[], 2);
    for (name, x, y) in [("Gem 1", -1.5, 0.5), ("Gem 2", 0.5, 1.5), ("Gem 3", 2.0, -0.5)] {
        place(&mut w, "Hero", x, y);
        pump(&mut w, &[], 3);
        assert!(w.entities.iter().all(|e| e.name != name), "{name} stays after a touch");
    }
    pump(&mut w, &[], 2);
    assert_eq!(score(&w, "gems"), 3, "log={:?}", w.play_log);
    assert_eq!(node_text(&w, "Hud", "Gems"), "Gems: 3");

    place(&mut w, "Hero", 1.0, 2.2);
    pump(&mut w, &[], 2);
    pump(&mut w, &["e"], 2);
    assert!(shown(&w, "Dialog"), "interact opens the box: {:?}", w.play_log);
    assert!(node_text(&w, "Dialog", "Talk").contains("cave"));
    pump(&mut w, &[], 2);
    pump(&mut w, &["e"], 2);
    assert!(!shown(&w, "Dialog"), "interact again closes it");

    place(&mut w, "Hero", -4.5, 0.0);
    let start = {
        let h = enemy_at(&w, "Enemy 1");
        let p = hero(&w).translation;
        ((h[0] - p[0]).powi(2) + (h[1] - p[1]).powi(2)).sqrt()
    };
    pump(&mut w, &[], 30);
    let end = {
        let h = enemy_at(&w, "Enemy 1");
        let p = hero(&w).translation;
        ((h[0] - p[0]).powi(2) + (h[1] - p[1]).powi(2)).sqrt()
    };
    assert!(end < start - 0.5, "enemy closed in: {start} -> {end}");
    let enemy_clip = w.entities.iter().find(|e| e.name == "Enemy 1").unwrap().animation.clone();
    assert!(enemy_clip.starts_with("walk_"), "enemy clip {enemy_clip}");
    script_ok(&w);
    w.stop();
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_save_restores_place_hearts_gems_and_scene() {
    let (dir, mut w) = project("save");
    w.play();
    pump(&mut w, &[], 2);
    place(&mut w, "Hero", -5.5, -2.0);
    pump(&mut w, &[], 12);
    let save = dir.join("saved/games/shrine.sealsave");
    assert!(save.is_file(), "shrine wrote a save: {:?}", w.play_log);
    let text = fs::read_to_string(&save).unwrap();
    assert!(text.contains("scene.seal"), "{text}");
    place(&mut w, "Hero", -1.5, 0.5);
    pump(&mut w, &[], 4);
    assert_eq!(score(&w, "gems"), 1);
    hurt(&mut w);
    assert_eq!(score(&w, "hearts"), 2);
    place(&mut w, "Hero", 5.2, 0.0);
    for _ in 0..20 {
        w.read_controls();
        w.step(1.0 / 60.0);
        if w.scenes.open == "scenes/cave.seal" {
            break;
        }
    }
    assert_eq!(w.scenes.open, "scenes/cave.seal");
    for _ in 0..4 {
        if score(&w, "hearts") == 0 || w.scenes.open != "scenes/cave.seal" {
            break;
        }
        let at = enemy_at(&w, "Enemy 2");
        place(&mut w, "Hero", at[0], at[1]);
        pump(&mut w, &[], 2);
        if score(&w, "hearts") == 0 || w.scenes.open != "scenes/cave.seal" {
            break;
        }
        place(&mut w, "Hero", at[0] + 4.0, at[1]);
        pump(&mut w, &[], 40);
    }
    for _ in 0..30 {
        w.read_controls();
        w.step(1.0 / 60.0);
        if w.scenes.open == "scene.seal" && score(&w, "hearts") == 3 {
            break;
        }
    }
    assert_eq!(w.scenes.open, "scene.seal", "load returns to the village: {:?}", w.play_log);
    assert_eq!(score(&w, "hearts"), 3);
    assert_eq!(score(&w, "gems"), 0, "collected gems were not in the save");
    assert!(w.entities.iter().any(|e| e.name == "Gem 1"));
    let at = hero(&w).translation;
    assert!((at[0] - -5.5).abs() < 0.6 && (at[1] - -2.0).abs() < 0.6, "back at the shrine, at={at:?}");
    script_ok(&w);
    w.stop();
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn damaged_frames_file_is_rejected() {
    let err = seal_world::frames::SpriteFrames::parse("not a frames file\n").unwrap_err();
    assert!(err.to_string().contains("line") || !err.to_string().is_empty());
}