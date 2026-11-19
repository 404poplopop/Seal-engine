//! Volumes, tick order, spectate and local join.

use seal_input::{Button, RawInput};
use seal_world::game::Game;
use seal_world::players::{self, Roster};
use seal_world::volumes::{self, Memory, Volume};
use seal_world::{Entity, World};

fn body(id: &str, tag: &str, at: [f32; 3]) -> Entity {
    let world = World::starter();
    let mut entity = world.entities.into_iter().find(|e| e.id == "sun").unwrap();
    entity.id = id.into();
    entity.name = id.into();
    entity.tag = tag.into();
    entity.translation = at;
    entity.hidden = false;
    entity.volume = Volume::default();
    entity.phys2d.size = [1.0, 1.0];
    entity.scale = [1.0, 1.0, 1.0];
    entity
}

fn zone(id: &str, volume: Volume, at: [f32; 3], size: [f32; 2]) -> Entity {
    let mut entity = body(id, "", at);
    entity.volume = volume;
    entity.phys2d.size = size;
    entity
}

#[test]
fn a_kill_zone_respawns_at_the_checkpoint_or_destroys() {
    let mut entities = vec![
        zone(
            "flag",
            Volume { kind: "checkpoint".into(), ..Volume::default() },
            [4.0, 1.0, 0.0],
            [2.0, 2.0],
        ),
        zone(
            "pit",
            Volume { kind: "kill".into(), mode: "respawn".into(), ..Volume::default() },
            [0.0, 0.0, 0.0],
            [2.0, 2.0],
        ),
        body("hero", "player", [4.0, 1.0, 0.0]),
    ];
    let mut game = Game::default();
    let mut memory = Memory::default();
    volumes::step(&mut entities, &mut game, &mut memory, 0.1);
    assert_eq!(game.checkpoint, Some([4.0, 1.0, 0.0]));
    entities.iter_mut().find(|e| e.id == "hero").unwrap().translation = [0.0, 0.0, 0.0];
    let outcome = volumes::step(&mut entities, &mut game, &mut memory, 0.1);
    assert!(outcome.cues.iter().any(|c| c.event == "killed" && c.other == "hero"));
    assert_eq!(entities.iter().find(|e| e.id == "hero").unwrap().translation, [4.0, 1.0, 0.0]);
    entities.iter_mut().find(|e| e.id == "pit").unwrap().volume.mode = "destroy".into();
    entities.iter_mut().find(|e| e.id == "hero").unwrap().translation = [9.0, 9.0, 0.0];
    volumes::step(&mut entities, &mut game, &mut memory, 0.1);
    entities.iter_mut().find(|e| e.id == "hero").unwrap().translation = [0.0, 0.0, 0.0];
    let outcome = volumes::step(&mut entities, &mut game, &mut memory, 0.1);
    assert!(outcome.remove.iter().any(|id| id == "hero"));
}

#[test]
fn damage_removes_points_at_the_given_rate() {
    let mut entities = vec![
        zone(
            "lava",
            Volume {
                kind: "damage".into(),
                variable: "health".into(),
                rate: 10.0,
                ..Volume::default()
            },
            [0.0, 0.0, 0.0],
            [4.0, 4.0],
        ),
        body("hero", "player", [0.0, 0.0, 0.0]),
    ];
    let mut game = Game::default();
    game.scores.insert("health".into(), 100);
    game.values.insert("health".into(), "100".into());
    let mut memory = Memory::default();
    for _ in 0..5 {
        volumes::step(&mut entities, &mut game, &mut memory, 0.1);
    }
    assert_eq!(game.scores["health"], 95);
    assert_eq!(game.values["health"], "95");
    let again = {
        let mut game = Game::default();
        game.scores.insert("health".into(), 100);
        let mut memory = Memory::default();
        let mut entities = entities.clone();
        for _ in 0..5 {
            volumes::step(&mut entities, &mut game, &mut memory, 0.1);
        }
        game.scores["health"]
    };
    assert_eq!(again, 95);
}

#[test]
fn a_trigger_ignores_tags_outside_its_filter() {
    let mut entities = vec![
        zone(
            "door",
            Volume { kind: "trigger".into(), tag: "player, npc".into(), ..Volume::default() },
            [0.0, 0.0, 0.0],
            [3.0, 3.0],
        ),
        body("hero", "player", [0.0, 0.0, 0.0]),
        body("crate", "prop", [0.0, 0.0, 0.0]),
    ];
    let mut game = Game::default();
    let mut memory = Memory::default();
    let outcome = volumes::step(&mut entities, &mut game, &mut memory, 0.1);
    let others: Vec<_> = outcome.cues.iter().map(|c| c.other.as_str()).collect();
    assert!(others.contains(&"hero"), "{outcome:?}");
    assert!(!others.contains(&"crate"), "{outcome:?}");
    assert!(outcome.cues.iter().any(|c| c.event == "trigger_enter"));
    let stay = volumes::step(&mut entities, &mut game, &mut memory, 0.1);
    assert!(stay.cues.iter().any(|c| c.event == "trigger_stay" && c.other == "hero"));
    entities.iter_mut().find(|e| e.id == "hero").unwrap().translation = [20.0, 0.0, 0.0];
    let leave = volumes::step(&mut entities, &mut game, &mut memory, 0.1);
    assert!(leave.cues.iter().any(|c| c.event == "trigger_exit" && c.other == "hero"));
}

#[test]
fn volumes_round_trip_and_a_bad_rate_is_refused() {
    let mut world = World::starter();
    let id = world.add("kill_zone");
    let before = world.entities.len();
    let text = world.to_text();
    assert!(text.contains("volume"), "{text}");
    let back = World::from_text(&text).unwrap();
    let loaded = back.entities.iter().find(|e| e.id == id).unwrap();
    assert_eq!(loaded.volume.kind, "kill");
    assert_eq!(loaded.volume.mode, "respawn");
    assert_eq!(back.entities.len(), before);
    let plain = World::starter().to_text();
    assert!(!plain.contains("\nvolume "), "a scene with no volume must stay unchanged");
    let damaged = text.replace("rate = 10", "rate = \"nope\"");
    let broken = if damaged != text {
        damaged
    } else {
        text.replacen("kind = \"kill\"", "kind = \"kill\", rate = \"nope\"", 1)
    };
    match World::from_text(&broken) {
        Ok(_) => panic!("a volume rate that is not a number must fail to load"),
        Err(err) => assert!(err.contains("rate") || err.contains("volume"), "{err}"),
    }
}

#[test]
fn fixed_and_late_ticks_follow_script_order() {
    let dir = std::env::temp_dir().join(format!("seal-world-ticks-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("logic")).unwrap();
    let write = |name: &str, tick: &str, fixed: &str, late: &str| {
        let text = format!(
            "schema_version = 1\nseal_graph = \"{name}\"\nvars = []\nnodes = [\n\
             {{ id = 1, kind = \"event\", name = \"tick\" }},\n\
             {{ id = 2, kind = \"log\", defaults = {{ message = \"{tick}\" }} }},\n\
             {{ id = 3, kind = \"event\", name = \"fixed_tick\" }},\n\
             {{ id = 4, kind = \"log\", defaults = {{ message = \"{fixed}\" }} }},\n\
             {{ id = 5, kind = \"event\", name = \"late_tick\" }},\n\
             {{ id = 6, kind = \"log\", defaults = {{ message = \"{late}\" }} }},\n\
             ]\nlinks = [\n\"1.then -> 2.in\", \"3.then -> 4.in\", \"5.then -> 6.in\",\n]\n"
        );
        std::fs::write(dir.join(format!("logic/{name}.graph")), text).unwrap();
    };
    write("low", "tick-low", "fixed-low", "late-low");
    write("high", "tick-high", "fixed-high", "late-high");
    let mut world = World::starter();
    for entity in &mut world.entities {
        entity.script.clear();
    }
    let low = world.add("empty");
    let high = world.add("empty");
    for entity in &mut world.entities {
        if entity.id == low {
            entity.script = "logic/low.graph".into();
            entity.script_order = 0;
        }
        if entity.id == high {
            entity.script = "logic/high.graph".into();
            entity.script_order = 5;
        }
    }
    world.project_dir = Some(dir.clone());
    world.play();
    world.play_log.clear();
    assert!(world.step(0.05), "{:?}", world.play_log);
    assert_eq!(
        world.play_log,
        ["tick-low", "tick-high", "fixed-low", "fixed-high", "late-low", "late-high",],
        "{:?}",
        world.play_log
    );
    let text = world.to_text();
    assert!(text.contains("script_order"), "{text}");
    let back = World::from_text(&text).unwrap();
    assert_eq!(back.entities.iter().find(|e| e.id == high).unwrap().script_order, 5);
    assert_eq!(back.entities.iter().find(|e| e.id == low).unwrap().script_order, 0);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn spectate_points_the_camera_and_unpossess_lets_go() {
    let dir = std::env::temp_dir().join(format!("seal-world-spectate-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("logic")).unwrap();
    std::fs::write(
        dir.join("logic/watch.graph"),
        r#"schema_version = 1
seal_graph = "watch"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "self" },
  { id = 3, kind = "spectate", defaults = { on = true } },
]
links = [ "1.then -> 3.in", "2.out -> 3.entity" ]
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("logic/drop.graph"),
        r#"schema_version = 1
seal_graph = "drop"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "self" },
  { id = 3, kind = "possess", defaults = { index = 0 } },
  { id = 4, kind = "unpossess", defaults = { index = 0 } },
]
links = [ "1.then -> 3.in", "2.out -> 3.entity", "3.then -> 4.in" ]
"#,
    )
    .unwrap();
    let mut world = World::starter();
    for entity in &mut world.entities {
        entity.script.clear();
    }
    let target = world.add("empty");
    let at = [6.0, 3.0, 0.0];
    for entity in &mut world.entities {
        if entity.id == target {
            entity.script = "logic/watch.graph".into();
            entity.translation = at;
        }
        if entity.tag == "camera" || entity.camera2d.on {
            entity.camera2d.on = true;
            entity.camera2d.smoothing = 0.0;
            entity.camera2d.follow.clear();
            entity.hidden = false;
        }
    }
    if !world.entities.iter().any(|e| e.camera2d.on) {
        let cam = world.add("camera");
        let camera = world.entities.iter_mut().find(|e| e.id == cam).unwrap();
        camera.camera2d.on = true;
        camera.camera2d.smoothing = 0.0;
    }
    world.project_dir = Some(dir.clone());
    world.play();
    assert!(world.step(0.05), "{:?}", world.play_log);
    let view = world.play_camera2d().expect("a 2D camera");
    assert!((view.center[0] - at[0]).abs() < 0.2, "{view:?} {:?}", world.play_log);
    assert!((view.center[1] - at[1]).abs() < 0.2, "{view:?}");
    world.stop();
    for entity in &mut world.entities {
        entity.script.clear();
        if entity.tag == "player" {
            entity.script = "logic/drop.graph".into();
        }
    }
    world.play();
    assert!(world.play_game().unwrap().possessed.is_empty(), "{:?}", world.play_log);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn press_start_assigns_the_next_pad_to_the_next_player() {
    let mut game = Game::default();
    let mut roster = Roster::default();
    let mut world = World::starter();
    let players: Vec<_> =
        world.entities.iter().filter(|e| e.tag == "player").map(|e| e.id.clone()).collect();
    assert!(!players.is_empty());
    let first = players::join_pad(&mut game, &mut roster, &world.entities, 0).unwrap();
    let second = players::join_pad(&mut game, &mut roster, &world.entities, 1).unwrap();
    assert_eq!((first, second), (0, 1));
    assert!(players::join_pad(&mut game, &mut roster, &world.entities, 0).is_none());
    assert_eq!(game.devices.get(&0), Some(&0));
    assert_eq!(game.devices.get(&1), Some(&1));
    assert_eq!(game.possessed.get(&0).map(String::as_str), Some(players[0].as_str()));

    world.join_on = true;
    world.play();
    let start = Button::parse("pad.start").unwrap();
    world.controls.raw.press(start.clone());
    world.read_controls();
    assert_eq!(world.play_game().unwrap().devices.get(&0), Some(&0));
    world.controls.raw.release(&start);
    world.read_controls();
    let mut extra = RawInput::default();
    extra.press(start);
    world.controls.extra_pads = vec![extra];
    world.read_controls();
    assert_eq!(world.play_game().unwrap().devices.get(&1), Some(&1));
    let text = world.to_text();
    assert!(text.contains("local_join"), "{text}");
    let back = World::from_text(&text).unwrap();
    assert!(back.join_on);
    let plain = World::starter().to_text();
    assert!(!plain.contains("local_join"));
}
