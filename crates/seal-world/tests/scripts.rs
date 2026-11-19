//! Logic graphs attached to scene objects run while the scene plays.

use seal_world::{PlayMode, World};

const WALKER: &str = r#"
schema_version = 1
seal_graph = "walker"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "player", defaults = { index = 0 } },
  { id = 3, kind = "to_str", type = "entity" },
  { id = 4, kind = "log" },
  { id = 5, kind = "event", name = "tick" },
  { id = 6, kind = "self" },
  { id = 7, kind = "world_get", component = "transform", field = "translation", type = "vec3" },
  { id = 8, kind = "scale", type = "vec3", defaults = { v = [2.0, 0.0, 0.0] } },
  { id = 9, kind = "add", type = "vec3" },
  { id = 10, kind = "world_set", component = "transform", field = "translation", type = "vec3" },
]
links = [
  "1.then -> 4.in", "2.out -> 3.a", "3.out -> 4.message",
  "6.out -> 7.entity", "5.dt -> 8.s", "7.out -> 9.a", "8.out -> 9.b",
  "5.then -> 10.in", "6.out -> 10.entity", "9.out -> 10.value",
]
"#;

fn project(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("seal-world-scripts-{name}-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("logic")).unwrap();
    std::fs::write(dir.join("logic/walker.graph"), WALKER).unwrap();
    dir
}

#[test]
fn a_script_moves_its_object_during_play_and_stop_puts_it_back() {
    let dir = project("walk");
    let mut world = World::starter();
    let i = world.entities.iter().position(|e| e.shape.is_some()).expect("starter has a floor");
    let before = world.entities[i].translation;
    world.entities[i].script = "logic/walker.graph".into();
    world.project_dir = Some(dir.clone());
    world.play();
    assert_eq!(world.mode, PlayMode::Playing);
    let player = world.entities.iter().position(|e| e.tag == "player");
    let expected = player.map_or("none".to_string(), |p| format!("{}v0", p + 1));
    assert_eq!(world.play_log, [expected]);
    for _ in 0..4 {
        assert!(world.step(0.25));
    }
    let moved = world.entities[i].translation;
    assert!((moved[0] - before[0] - 2.0).abs() < 1e-5, "{before:?} -> {moved:?}");
    assert_eq!(moved[1], before[1]);
    world.stop();
    assert_eq!(world.entities[i].translation, before);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_missing_script_is_logged_and_play_goes_on() {
    let dir = project("missing");
    let mut world = World::starter();
    world.entities[0].script = "logic/nope.graph".into();
    world.project_dir = Some(dir.clone());
    world.play();
    assert_eq!(world.play_log.len(), 1);
    assert!(world.play_log[0].contains("logic/nope.graph"), "{:?}", world.play_log);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn the_script_field_survives_save_and_load() {
    let mut world = World::starter();
    world.entities[0].script = "logic/walker.graph".into();
    let text = world.to_text();
    let back = World::from_text(&text).unwrap();
    assert_eq!(back.entities[0].script, "logic/walker.graph");
}

const JUMPER: &str = r#"
schema_version = 1
seal_graph = "jumper"
vars = []
nodes = [
  { id = 1, kind = "event", name = "key_SpaceBar_pressed" },
  { id = 2, kind = "log", defaults = { message = "jump" } },
  { id = 3, kind = "event", name = "key_SpaceBar_released" },
  { id = 4, kind = "log", defaults = { message = "land" } },
]
links = ["1.then -> 2.in", "3.then -> 4.in"]
"#;

#[test]
fn scripts_hear_keys_by_their_classic_names() {
    use seal_world::scripts::key_name;
    assert_eq!(key_name("Space").as_deref(), Some("SpaceBar"));
    assert_eq!(key_name("ArrowLeft").as_deref(), Some("Left"));
    assert_eq!(key_name("a").as_deref(), Some("A"));
    assert_eq!(key_name("7").as_deref(), Some("Seven"));
    assert_eq!(key_name("F5").as_deref(), Some("F5"));
    assert_eq!(key_name("F13"), None);
    assert_eq!(key_name("Copy"), None);

    let dir = project("keys");
    std::fs::write(dir.join("logic/jumper.graph"), JUMPER).unwrap();
    let mut world = World::starter();
    world.entities[0].script = "logic/jumper.graph".into();
    world.project_dir = Some(dir.clone());
    world.play();
    assert!(world.has_scripts());
    world.play_keys.push(("SpaceBar".into(), true));
    world.play_keys.push(("A".into(), true));
    world.step(0.1);
    world.play_keys.push(("SpaceBar".into(), false));
    world.step(0.1);
    assert_eq!(world.play_log, ["jump", "land"]);
    world.stop();
    assert!(!world.has_scripts() && world.play_keys.is_empty());
    std::fs::remove_dir_all(dir).ok();
}

const SPAWNER: &str = r#"
schema_version = 1
seal_graph = "spawner"
vars = [{ name = "last", type = "entity" }]
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "spawn", prefab = "coin" },
  { id = 3, kind = "set", name = "last" },
  { id = 4, kind = "log", defaults = { message = "made" } },
  { id = 5, kind = "event", name = "key_K_pressed" },
  { id = 6, kind = "get", name = "last" },
  { id = 7, kind = "despawn" },
  { id = 8, kind = "is_valid" },
  { id = 9, kind = "to_str", type = "bool" },
  { id = 10, kind = "log" },
]
links = [
  "1.then -> 2.in", "2.then -> 3.in", "2.entity -> 3.value", "3.then -> 4.in",
  "5.then -> 7.in", "6.out -> 7.entity", "7.then -> 10.in",
  "6.out -> 8.entity", "8.out -> 9.a", "9.out -> 10.message",
]
"#;

const COIN: &str = r#"
schema_version = 1
seal_graph = "coin"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "log", defaults = { message = "coin up" } },
]
links = ["1.then -> 2.in"]
"#;

#[test]
fn scripts_spawn_and_remove_objects_and_stop_puts_the_scene_back() {
    let dir = project("spawn");
    std::fs::write(dir.join("logic/spawner.graph"), SPAWNER).unwrap();
    std::fs::write(dir.join("logic/coin.graph"), COIN).unwrap();
    let mut world = World::starter();
    assert!(world.entities.len() >= 2);
    let count = world.entities.len();
    world.entities[0].script = "logic/spawner.graph".into();
    world.entities[1].source_asset = "coin".into();
    world.entities[1].script = "logic/coin.graph".into();
    world.project_dir = Some(dir.clone());
    world.play();
    // The scene's coin starts, the spawner makes a copy, and the copy starts too.
    assert_eq!(world.play_log, ["made", "coin up", "coin up"]);
    assert_eq!(world.entities.len(), count + 1);
    let copy = world.entities.last().unwrap();
    assert!(copy.outliner_play_spawned);
    assert_eq!(copy.source_asset, "coin");
    assert_ne!(copy.id, world.entities[1].id);
    world.play_log.clear();
    world.play_keys.push(("K".into(), true));
    world.step(0.1);
    assert_eq!(world.play_log, ["false"]);
    assert_eq!(world.entities.len(), count);
    assert!(world.entities.iter().all(|e| !e.outliner_play_spawned));
    world.stop();
    assert_eq!(world.entities.len(), count);
    std::fs::remove_dir_all(dir).ok();
}

const FIELDS: &str = r#"
schema_version = 1
seal_graph = "fields"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "self" },
  { id = 3, kind = "world_get", component = "editor", field = "name", type = "str" },
  { id = 4, kind = "log" },
  { id = 5, kind = "world_set", component = "light", field = "intensity", type = "float", defaults = { value = 250.0 } },
  { id = 6, kind = "world_set", component = "editor", field = "tag", type = "str", defaults = { value = "lit" } },
  { id = 7, kind = "world_set", component = "body", field = "velocity", type = "vec3", defaults = { value = [1.0, 2.0, 3.0] } },
]
links = [
  "1.then -> 4.in", "2.out -> 3.entity", "3.out -> 4.message",
  "4.then -> 5.in", "2.out -> 5.entity", "5.then -> 6.in", "2.out -> 6.entity",
  "6.then -> 7.in", "2.out -> 7.entity",
]
"#;

#[test]
fn scripts_read_and_change_names_lights_tags_and_velocity() {
    let dir = project("fields");
    std::fs::write(dir.join("logic/fields.graph"), FIELDS).unwrap();
    let mut world = World::starter();
    world.entities[0].script = "logic/fields.graph".into();
    world.project_dir = Some(dir.clone());
    world.play();
    assert_eq!(world.play_log, [world.entities[0].name.clone()]);
    let e = &world.entities[0];
    assert_eq!((e.intensity, e.tag.as_str(), e.velocity), (250.0, "lit", [1.0, 2.0, 3.0]));
    std::fs::remove_dir_all(dir).ok();
}

const TOUCHER: &str = r#"
schema_version = 1
seal_graph = "toucher"
vars = []
nodes = [
  { id = 1, kind = "event", name = "overlap_begin", params = [{ name = "other", type = "entity" }] },
  { id = 2, kind = "log", defaults = { message = "begin" } },
  { id = 3, kind = "despawn" },
  { id = 4, kind = "event", name = "overlap_end", params = [{ name = "other", type = "entity" }] },
  { id = 5, kind = "log", defaults = { message = "end" } },
  { id = 6, kind = "event", name = "hit", params = [{ name = "other", type = "entity" }, { name = "location", type = "vec3" }, { name = "normal", type = "vec3" }] },
  { id = 7, kind = "log", defaults = { message = "hit" } },
]
links = ["1.then -> 2.in", "2.then -> 3.in", "1.other -> 3.entity", "4.then -> 5.in", "6.then -> 7.in"]
"#;

const DOOMED: &str = r#"
schema_version = 1
seal_graph = "doomed"
vars = []
nodes = [
  { id = 1, kind = "event", name = "destroyed" },
  { id = 2, kind = "self" },
  { id = 3, kind = "world_get", component = "editor", field = "name", type = "str" },
  { id = 4, kind = "log" },
]
links = ["1.then -> 4.in", "2.out -> 3.entity", "3.out -> 4.message"]
"#;

#[test]
fn touching_objects_get_overlap_events_and_removed_ones_hear_destroyed() {
    let dir = project("touch");
    std::fs::write(dir.join("logic/toucher.graph"), TOUCHER).unwrap();
    std::fs::write(dir.join("logic/doomed.graph"), DOOMED).unwrap();
    let mut world = World::starter();
    let template = world.entities.iter().find(|e| e.id != "root").unwrap().clone();
    world.entities.clear();
    let make = |id: &str, x: f32, script: &str| {
        let mut e = template.clone();
        e.id = id.into();
        e.name = id.into();
        e.parent = None;
        e.shape = Some(seal_world::Shape::Cube);
        e.dynamic = false;
        e.translation = [x, 0.0, 0.0];
        e.scale = [1.0, 1.0, 1.0];
        e.script = script.into();
        e
    };
    world.entities.push(make("hunter", 0.0, "logic/toucher.graph"));
    world.entities.push(make("gem", 3.0, "logic/doomed.graph"));
    world.project_dir = Some(dir.clone());
    world.play();
    world.step(0.1);
    assert!(world.play_log.is_empty(), "{:?}", world.play_log);
    world.entities[1].translation[0] = 0.5;
    world.step(0.1);
    // Only the hunter has a begin handler; it removes the gem, which says goodbye.
    assert_eq!(world.play_log, ["begin", "gem"]);
    assert_eq!(world.entities.len(), 1);
    world.step(0.1);
    assert_eq!(world.play_log, ["begin", "gem"], "a removed object ends no overlap");
    world.stop();
    assert_eq!(world.entities.len(), 2);
    std::fs::remove_dir_all(dir).ok();
}

const BUMPER: &str = r#"
schema_version = 1
seal_graph = "bumper"
vars = []
nodes = [
  { id = 1, kind = "event", name = "hit", params = [{ name = "other", type = "entity" }, { name = "location", type = "vec3" }, { name = "normal", type = "vec3" }] },
  { id = 2, kind = "to_str", type = "vec3" },
  { id = 3, kind = "to_str", type = "vec3" },
  { id = 4, kind = "log" },
  { id = 5, kind = "log" },
]
links = ["1.then -> 4.in", "4.then -> 5.in", "1.location -> 2.a", "1.normal -> 3.a", "2.out -> 4.message", "3.out -> 5.message"]
"#;

#[test]
fn hits_carry_where_the_objects_met_and_the_push_out_direction() {
    let dir = project("bump");
    std::fs::write(dir.join("logic/bumper.graph"), BUMPER).unwrap();
    let mut world = World::starter();
    let template = world.entities.iter().find(|e| e.id != "root").unwrap().clone();
    world.entities.clear();
    let make = |id: &str, x: f32, dynamic: bool| {
        let mut e = template.clone();
        e.id = id.into();
        e.name = id.into();
        e.parent = None;
        e.shape = Some(seal_world::Shape::Cube);
        e.dynamic = dynamic;
        e.translation = [x, 0.0, 0.0];
        e.scale = [1.0, 1.0, 1.0];
        e.script = "logic/bumper.graph".into();
        e
    };
    world.entities.push(make("ball", 0.0, true));
    world.entities.push(make("wall", 0.75, false));
    world.project_dir = Some(dir.clone());
    world.play();
    world.step(0.0);
    let want_mid =
        seal_graph::rt::Value::Vec3(seal_graph::rt::V3 { x: 0.375, y: 0.0, z: 0.0 }).to_string();
    let left =
        seal_graph::rt::Value::Vec3(seal_graph::rt::V3 { x: -1.0, y: 0.0, z: 0.0 }).to_string();
    let right =
        seal_graph::rt::Value::Vec3(seal_graph::rt::V3 { x: 1.0, y: 0.0, z: 0.0 }).to_string();
    // Both objects hear the hit at the same point, pushed apart along x.
    assert_eq!(world.play_log, [want_mid.clone(), left, want_mid, right]);
    world.stop();
    std::fs::remove_dir_all(dir).ok();
}

const RIDER: &str = r#"
schema_version = 1
seal_graph = "rider"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "self" },
  { id = 3, kind = "player", defaults = { index = 0 } },
  { id = 4, kind = "world_set", component = "attach", field = "keep_world", type = "entity" },
  { id = 5, kind = "world_get", component = "attach", field = "parent", type = "entity" },
  { id = 6, kind = "compare", type = "entity", op = "eq" },
  { id = 7, kind = "to_str", type = "bool" },
  { id = 8, kind = "log" },
  { id = 9, kind = "event", name = "key_K_pressed" },
  { id = 10, kind = "world_set", component = "attach", field = "detach", type = "entity" },
  { id = 11, kind = "event", name = "tick" },
  { id = 12, kind = "input_axis", defaults = { name = "MoveRight" } },
  { id = 13, kind = "compare", type = "float", op = "gt", defaults = { b = 0.5 } },
  { id = 14, kind = "branch" },
  { id = 15, kind = "log", defaults = { message = "right" } },
]
links = [
  "1.then -> 4.in", "2.out -> 4.entity", "3.out -> 4.value", "4.then -> 8.in",
  "2.out -> 5.entity", "5.out -> 6.a", "3.out -> 6.b", "6.out -> 7.a", "7.out -> 8.message",
  "9.then -> 10.in", "2.out -> 10.entity", "2.out -> 10.value",
  "11.then -> 14.in", "12.out -> 13.a", "13.out -> 14.cond", "14.true -> 15.in",
]
"#;

#[test]
fn attached_objects_follow_their_parent_until_detached_and_axes_read_held_keys() {
    let dir = project("attach");
    std::fs::write(dir.join("logic/rider.graph"), RIDER).unwrap();
    let mut world = World::starter();
    for e in &mut world.entities {
        e.tag.clear();
        e.dynamic = false;
    }
    let (rider, horse) = (0, 1);
    world.entities[horse].tag = "player".into();
    world.entities[horse].translation = [1.0, 2.0, 3.0];
    world.entities[rider].translation = [4.0, 2.0, 3.0];
    world.entities[rider].script = "logic/rider.graph".into();
    world.project_dir = Some(dir.clone());
    world.play();
    assert_eq!(world.play_log, ["true"]);
    assert_eq!(world.entities[rider].parent.as_deref(), Some(world.entities[horse].id.as_str()));
    world.entities[horse].translation = [10.0, 0.0, 0.0];
    world.step(0.1);
    assert_eq!(world.entities[rider].translation, [13.0, 0.0, 0.0]);
    world.play_keys.push(("K".into(), true));
    world.play_keys.push(("D".into(), true));
    world.step(0.1);
    world.entities[horse].translation = [20.0, 0.0, 0.0];
    world.step(0.1);
    assert_eq!(world.entities[rider].translation, [13.0, 0.0, 0.0]);
    assert_eq!(world.play_log, ["true", "right", "right"]);
    world.stop();
    assert_eq!(world.entities[rider].translation, [4.0, 2.0, 3.0]);
    std::fs::remove_dir_all(dir).ok();
}

const BELL: &str = r#"
schema_version = 1
seal_graph = "bell"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "play_sound", defaults = { sound = "Audio/Bell", volume = 0.5, pitch = 1.0 } },
  { id = 3, kind = "event", name = "key_K_pressed" },
  { id = 4, kind = "self" },
  { id = 5, kind = "world_get", component = "transform", field = "translation", type = "vec3" },
  { id = 6, kind = "play_sound_at", defaults = { sound = "Audio/Ding", volume = 1.0, pitch = 2.0 } },
]
links = [
  "1.then -> 2.in", "3.then -> 6.in", "4.out -> 5.entity", "5.out -> 6.location",
]
"#;

const THEME: &str = r#"
schema_version = 1
seal_graph = "theme"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "play_music", defaults = { file = "Audio/Theme", looping = true, fade = 1.5, loop_start = 2 } },
  { id = 3, kind = "event", name = "key_K_pressed" },
  { id = 4, kind = "stop_music", defaults = { fade = 0.25 } },
  { id = 5, kind = "event", name = "key_J_pressed" },
  { id = 6, kind = "set_music_volume", defaults = { volume = 0.4 } },
]
links = [ "1.then -> 2.in", "3.then -> 4.in", "5.then -> 6.in" ]
"#;

#[test]
fn script_music_is_queued_and_scene_music_starts_with_play() {
    use seal_world::scripts::MusicCue;
    let dir = project("theme");
    std::fs::write(dir.join("logic/theme.graph"), THEME).unwrap();
    let mut world = World::starter();
    for e in &mut world.entities {
        e.tag.clear();
        e.dynamic = false;
    }
    world.entities[0].script = "logic/theme.graph".into();
    world.project_dir = Some(dir.clone());
    world.music = "Audio/Level.wav".into();
    world.play();
    world.step(0.1);
    assert!(matches!(
        &world.play_music[0],
        MusicCue::Play { file, looping: true, fade, loop_start }
            if file == "Audio/Level.wav" && *fade == 0.0 && *loop_start == 0.0
    ));
    assert!(matches!(
        &world.play_music[1],
        MusicCue::Play { file, looping: true, fade, loop_start }
            if file == "Audio/Theme" && (*fade - 1.5).abs() < 1e-4 && (*loop_start - 2.0).abs() < 1e-4
    ));
    world.play_music.clear();
    world.play_keys.push(("J".into(), true));
    world.step(0.1);
    assert_eq!(world.play_music, vec![MusicCue::Volume { volume: 0.4 }]);
    world.play_music.clear();
    world.play_keys.push(("K".into(), true));
    world.step(0.1);
    assert_eq!(world.play_music, vec![MusicCue::Stop { fade: 0.25 }]);
    world.pause();
    assert_eq!(world.play_music.last(), Some(&MusicCue::Pause));
    world.play();
    assert_eq!(world.play_music.last(), Some(&MusicCue::Resume));
    world.stop();
    assert_eq!(world.play_music, vec![MusicCue::Stop { fade: 0.0 }]);
    let text = {
        let mut editing = World::starter();
        editing.music = "Audio/Level.wav".into();
        editing.to_text()
    };
    let back = World::from_text(&text).unwrap();
    assert_eq!(back.music, "Audio/Level.wav");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn script_sounds_are_queued_for_the_editor() {
    let dir = project("bell");
    std::fs::write(dir.join("logic/bell.graph"), BELL).unwrap();
    let mut world = World::starter();
    for e in &mut world.entities {
        e.tag.clear();
        e.dynamic = false;
    }
    world.entities[0].translation = [1.0, 2.0, 3.0];
    world.entities[0].script = "logic/bell.graph".into();
    world.project_dir = Some(dir.clone());
    world.play();
    world.step(0.1);
    let cues: Vec<_> = world.play_sounds.drain(..).collect();
    assert_eq!(cues.len(), 1, "{cues:?} {:?}", world.play_log);
    assert_eq!((cues[0].sound.as_str(), cues[0].volume, cues[0].at), ("Audio/Bell", 0.5, None));
    world.play_keys.push(("K".into(), true));
    world.step(0.1);
    assert_eq!(world.play_sounds.len(), 1);
    assert_eq!((world.play_sounds[0].pitch, world.play_sounds[0].at), (2.0, Some([1.0, 2.0, 3.0])));
    world.stop();
    assert!(world.play_sounds.is_empty());
}

const HUD: &str = r#"
schema_version = 1
seal_graph = "hud"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "ui_text", defaults = { element = "Score", text = "Score: 0" } },
  { id = 3, kind = "event", name = "key_K_pressed" },
  { id = 4, kind = "ui_show", defaults = { widget = "Hud", visible = true } },
  { id = 5, kind = "event", name = "key_H_pressed" },
  { id = 6, kind = "ui_show", defaults = { widget = "Hud", visible = false } },
]
links = [ "1.then -> 2.in", "3.then -> 4.in", "5.then -> 6.in" ]
"#;

#[test]
fn shown_widgets_put_their_text_on_screen() {
    let dir = project("hud");
    std::fs::write(dir.join("logic/hud.graph"), HUD).unwrap();
    let mut world = World::starter();
    for e in &mut world.entities {
        e.tag.clear();
        e.dynamic = false;
    }
    world.entities[0].script = "logic/hud.graph".into();
    world.project_dir = Some(dir.clone());
    world.play();
    world.step(0.1);
    assert!(world.play_ui_lines().is_empty(), "text waits for a shown widget");
    world.play_keys.push(("K".into(), true));
    world.step(0.1);
    assert_eq!(world.play_ui_lines(), ["Score: 0"]);
    world.play_keys.push(("H".into(), true));
    world.step(0.1);
    assert!(world.play_ui_lines().is_empty());
    world.play_keys.push(("K".into(), true));
    world.step(0.1);
    world.stop();
    assert!(world.play_ui_lines().is_empty(), "stopping clears the screen");
}

#[test]
fn a_breakpoint_pauses_play_and_continue_or_step_carries_on() {
    let dir = project("break");
    let mut world = World::starter();
    for e in &mut world.entities {
        e.tag.clear();
        e.dynamic = false;
    }
    world.entities[0].script = "logic/walker.graph".into();
    world.entities[0].translation = [0.0, 0.0, 0.0];
    world.project_dir = Some(dir.clone());
    world.set_breakpoints("logic/walker.graph", [10].into_iter().collect());
    world.play();
    assert_eq!(world.mode, PlayMode::Playing);
    world.step(0.5);
    assert_eq!(world.mode, PlayMode::Paused, "{:?}", world.play_log);
    let hit = world.at_breakpoint().expect("stopped").clone();
    assert_eq!((hit.path.as_str(), hit.node), ("logic/walker.graph", 10));
    assert_eq!(world.entities[0].translation, [0.0, 0.0, 0.0], "stopped before the set ran");
    let view = world.debug_view("logic/walker.graph").expect("view");
    assert_eq!(view.paused_at, Some(10));
    assert!(view.ran.contains_key(&4), "start ran its log: {view:?}");
    // Paused play does not advance.
    assert!(!world.step(0.5));
    // One node: the set runs, and the run is done.
    assert!(!world.debug_step());
    assert_eq!(world.entities[0].translation[0], 1.0);
    let view = world.debug_view("logic/walker.graph").expect("view");
    assert_eq!(view.paused_at, None);
    assert!(view.ran.contains_key(&10), "{view:?}");
    assert!(
        view.values.get(&(9, 0)).is_some_and(|v| v.contains('1')),
        "the new position is watched: {view:?}"
    );
    world.play();
    assert_eq!(world.mode, PlayMode::Playing);
    // The breakpoint stays on: the next step stops again.
    world.step(0.5);
    assert_eq!(world.mode, PlayMode::Paused);
    // Removing it while stopped, then continuing, finishes and keeps playing.
    world.set_breakpoints("logic/walker.graph", Default::default());
    world.play();
    assert_eq!(world.mode, PlayMode::Playing);
    assert_eq!(world.entities[0].translation[0], 2.0);
    world.step(0.5);
    assert_eq!(world.entities[0].translation[0], 3.0);
    assert!(world.breakpoints.is_empty());
    world.stop();
}
const KICK: &str = r#"
schema_version = 1
seal_graph = "kick"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "self" },
  { id = 3, kind = "apply_impulse", defaults = { impulse = [0.0, 0.0, 0.0] } },
]
links = ["1.then -> 3.in", "2.out -> 3.entity"]
"#;

#[cfg(feature = "play-preview")]
#[test]
fn a_script_can_push_its_object_with_an_impulse() {
    let run = |script: &str| {
        let dir = project("kick");
        std::fs::write(dir.join("logic/kick.graph"), script).unwrap();
        let mut world = World::starter();
        for e in &mut world.entities {
            e.tag.clear();
            e.dynamic = false;
        }
        world.entities[0].dynamic = true;
        world.entities[0].translation = [0.0, 50.0, 0.0];
        world.entities[0].script = "logic/kick.graph".into();
        world.project_dir = Some(dir);
        world.play();
        for _ in 0..30 {
            world.step(1.0 / 60.0);
        }
        world.entities[0].translation[0]
    };
    let pushed = KICK.replace("[0.0, 0.0, 0.0]", "[8.0, 0.0, 0.0]");
    assert!(run(&pushed) > run(KICK) + 1.0);
}
const RAY: &str = r#"
schema_version = 1
seal_graph = "ray"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "ray_hit", defaults = { from = [0.0, 0.0, 0.0], direction = [1.0, 0.0, 0.0], max_distance = 100.0 } },
  { id = 3, kind = "ray_distance", defaults = { from = [0.0, 0.0, 0.0], direction = [1.0, 0.0, 0.0], max_distance = 100.0 } },
  { id = 4, kind = "world_get", component = "editor", field = "name", type = "str" },
  { id = 5, kind = "log" },
  { id = 6, kind = "to_str", type = "float" },
  { id = 7, kind = "log" },
]
links = ["1.then -> 5.in", "2.out -> 4.entity", "4.out -> 5.message", "5.then -> 7.in", "3.out -> 6.a", "6.out -> 7.message"]
"#;

#[test]
fn a_script_ray_finds_the_nearest_object_and_its_distance() {
    let dir = project("ray");
    std::fs::write(dir.join("logic/ray.graph"), RAY).unwrap();
    let mut world = World::starter();
    for e in &mut world.entities {
        e.tag.clear();
        e.dynamic = false;
        e.translation = [0.0, 50.0, 0.0];
    }
    let target = world.add("cube");
    {
        let e = world.entity_mut(&target).unwrap();
        e.name = "Target".into();
        e.translation = [10.0, 0.0, 0.0];
        e.scale = [2.0, 2.0, 2.0];
    }
    let target2 = world.add("cube");
    {
        let e = world.entity_mut(&target2).unwrap();
        e.name = "Far".into();
        e.translation = [20.0, 0.0, 0.0];
    }
    world.entities[0].translation = [0.0, 0.0, 5.0];
    world.entities[0].script = "logic/ray.graph".into();
    world.project_dir = Some(dir);
    world.play();
    world.step(1.0 / 60.0);
    let log = world.play_log.join("\n");
    assert!(log.contains("Target"), "{log}");
    assert!(log.contains('9'), "distance to the near face is 9: {log}");
}
#[cfg(feature = "play-preview")]
#[test]
fn held_keys_drive_a_character_with_walk_speed() {
    let dir = project("hero");
    let mut world = World::starter();
    for e in &mut world.entities {
        e.tag.clear();
        e.dynamic = false;
        e.translation = [0.0, 80.0, 0.0];
    }
    world.entities[0].script = "logic/walker.graph".into();
    let ground = world.add("plane");
    world.entity_mut(&ground).unwrap().translation = [500.0, 0.0, 0.0];
    let hero = world.add("capsule");
    {
        let e = world.entity_mut(&hero).unwrap();
        e.dynamic = true;
        e.walk_speed = 3.0;
        e.translation = [500.0, 1.0, 0.0];
    }
    world.project_dir = Some(dir);
    world.play();
    for _ in 0..40 {
        world.step(1.0 / 60.0);
    }
    let x0 = world.entity(&hero).unwrap().translation[0];
    world.play_keys.push(("D".into(), true));
    for _ in 0..30 {
        world.step(1.0 / 60.0);
    }
    let x1 = world.entity(&hero).unwrap().translation[0];
    assert!(x1 > x0 + 1.0, "D walks right: {x0} -> {x1}");
    world.play_keys.push(("D".into(), false));
    world.play_keys.push(("SpaceBar".into(), true));
    let mut peak = 0.0f32;
    for _ in 0..30 {
        world.step(1.0 / 60.0);
        peak = peak.max(world.entity(&hero).unwrap().translation[1]);
    }
    assert!(peak > 1.2, "Space jumps: {peak}");
}

const ANIM: &str = r#"
schema_version = 1
seal_graph = "anim"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "self" },
  { id = 3, kind = "play_anim", defaults = { anim = "anims/slide.anim" } },
]
links = ["1.then -> 3.in", "2.out -> 3.entity"]
"#;

#[test]
fn a_script_can_start_a_keyframe_animation_on_its_object() {
    let dir = project("anim");
    std::fs::write(dir.join("logic/anim.graph"), ANIM).unwrap();
    std::fs::create_dir_all(dir.join("anims")).unwrap();
    std::fs::write(
        dir.join("anims/slide.anim"),
        "length 1\ntrack position.x linear\nkey 0 0\nkey 1 6\n",
    )
    .unwrap();
    let mut world = World::starter();
    for e in &mut world.entities {
        e.tag.clear();
        e.dynamic = false;
    }
    world.entities[0].script = "logic/anim.graph".into();
    world.project_dir = Some(dir);
    world.play();
    for _ in 0..90 {
        world.step(1.0 / 60.0);
    }
    assert_eq!(world.entities[0].translation[0], 6.0);
}

const SEQ_GRAPH: &str = r#"
schema_version = 1
seal_graph = "cutscene"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "play_sequence", defaults = { sequence = "sequences/intro.seq" } },
]
links = ["1.then -> 2.in"]
"#;

#[test]
fn a_script_can_start_a_whole_scene_sequence() {
    let dir = project("seq");
    std::fs::write(dir.join("logic/cutscene.graph"), SEQ_GRAPH).unwrap();
    std::fs::create_dir_all(dir.join("sequences")).unwrap();
    let mut world = World::starter();
    for e in &mut world.entities {
        e.tag.clear();
        e.dynamic = false;
    }
    let target = world.entities[1].id.clone();
    std::fs::write(
        dir.join("sequences/intro.seq"),
        format!(
            "length 1\nobject {target}\ntrack position.y linear\nkey 0 0\nkey 1 4\nevent 1 done\n"
        ),
    )
    .unwrap();
    world.entities[0].script = "logic/cutscene.graph".into();
    world.project_dir = Some(dir);
    world.play();
    for _ in 0..90 {
        world.step(1.0 / 60.0);
    }
    assert_eq!(world.entities[1].translation[1], 4.0);
    assert_eq!(world.sequence_events, vec!["done"]);
}
