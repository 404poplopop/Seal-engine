//! Sprite animation frame events, the finished event, and 2D camera drag margins.

use seal_world::frames::SpriteFrames;
use seal_world::World;

const OLD: &str = "\
# hero animations
animation idle fps=4 loop=yes
frame hero.png 0 0 0.5 1
frame hero.png 0.5 0 1 1
animation jump fps=10 loop=no
frame jump.png
";

const WITH_EVENTS: &str = "\
animation run fps=10 loop=yes
frame a.png
event one
frame a.png
event two
frame a.png
event three
frame a.png
animation once fps=10 loop=no
frame b.png
event hit
frame b.png
";

const GRAPH: &str = r#"
schema_version = 1
seal_graph = "anim"
vars = []
nodes = [
  { id = 1, kind = "event", name = "animation_event", params = [{ name = "name", type = "str" }] },
  { id = 2, kind = "log" },
  { id = 3, kind = "event", name = "animation_finished" },
  { id = 4, kind = "log", defaults = { message = "finished" } },
]
links = ["1.then -> 2.in", "1.name -> 2.message", "3.then -> 4.in"]
"#;

fn dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("seal-anim-events-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("logic")).unwrap();
    d
}

#[test]
fn frames_round_trip_events_and_old_files_stay_unchanged() {
    let old = SpriteFrames::parse(OLD).unwrap();
    assert!(old.clips.iter().all(|c| c.frames.iter().all(|f| f.event.is_empty())));
    let text = old.to_text();
    assert!(!text.lines().any(|line| line.starts_with("event ")));
    assert_eq!(SpriteFrames::parse(&text).unwrap(), old);

    let lib = SpriteFrames::parse(WITH_EVENTS).unwrap();
    assert_eq!(
        lib.clip("run").unwrap().frames.iter().map(|f| f.event.as_str()).collect::<Vec<_>>(),
        vec!["one", "two", "three", ""]
    );
    let again = SpriteFrames::parse(&lib.to_text()).unwrap();
    assert_eq!(again, lib);

    assert!(SpriteFrames::parse("event boom\n").is_err(), "event before a frame");
    assert!(SpriteFrames::parse("animation a fps=8 loop=yes\nevent\n").is_err());
    assert!(
        SpriteFrames::parse("animation a fps=8 loop=yes\nframe h.png\nevent a\nevent b\n").is_err()
    );
}

#[test]
fn events_fire_in_order_across_a_long_step_and_finished_fires_once() {
    let d = dir("play");
    std::fs::write(d.join("hero.frames"), WITH_EVENTS).unwrap();
    std::fs::write(d.join("logic/anim.graph"), GRAPH).unwrap();
    let mut world = World::starter();
    for e in &mut world.entities {
        e.tag.clear();
        e.dynamic = false;
    }
    world.entities[0].sprite = "hero.frames".into();
    world.entities[0].animation = "run".into();
    world.entities[0].script = "logic/anim.graph".into();
    world.project_dir = Some(d);
    world.play();
    assert!(world.step(0.05), "{:?}", world.play_log);
    assert_eq!(names(&world), vec!["one".to_string()]);
    assert!(world.step(0.04), "{:?}", world.play_log);
    assert_eq!(names(&world), vec!["one".to_string()], "the same frame does not fire again");
    assert!(world.step(0.40), "{:?}", world.play_log);
    assert_eq!(
        names(&world),
        vec!["one".to_string(), "two".to_string(), "three".to_string(), "one".to_string()],
        "a long step fires each frame in order, then the next pass: {:?}",
        world.play_log
    );

    let now = world.play_time;
    world.entities[0].play_animation("once", now);
    let before = names(&world).len();
    assert!(world.step(0.25), "{:?}", world.play_log);
    let got = names(&world);
    assert_eq!(&got[before..], ["hit", "finished"], "log {:?}", world.play_log);
    let before = names(&world).len();
    assert!(world.step(0.25), "{:?}", world.play_log);
    assert_eq!(names(&world).len(), before, "finished does not fire again: {:?}", world.play_log);
}

fn names(world: &World) -> Vec<String> {
    world
        .play_log
        .iter()
        .filter(|line| !line.starts_with("script on") && !line.contains("paused"))
        .cloned()
        .collect()
}

#[test]
fn drag_margins_keep_the_play_view_still_inside_and_move_by_the_overshoot() {
    let mut world = World::demo();
    for e in &mut world.entities {
        e.dynamic = false;
        if e.tag == "player" {
            e.tag.clear();
        }
    }
    let target = world.add("empty");
    world.entity_mut(&target).unwrap().name = "Target".into();
    let cam = world.add("camera");
    {
        let camera = world.entity_mut(&cam).unwrap();
        camera.camera2d.on = true;
        camera.camera2d.follow = "Target".into();
        camera.camera2d.smoothing = 0.0;
        camera.camera2d.drag = [0.2, 0.2, 0.0, 0.0];
    }
    world.set_camera_view_half([5.0, 5.0]);
    world.play();
    // A scene with no script and no side-view player does not simulate, but the
    // camera still steps while play is running.
    let _ = world.step(1.0 / 60.0);
    assert_eq!(world.play_camera2d().unwrap().center, [0.0, 0.0]);
    world.entity_mut(&target).unwrap().translation = [1.0, 0.0, 0.0];
    let _ = world.step(1.0 / 60.0);
    assert_eq!(
        world.play_camera2d().unwrap().center,
        [0.0, 0.0],
        "a target inside the zone leaves the camera where it is"
    );
    world.entity_mut(&target).unwrap().translation = [4.0, 0.0, 0.0];
    let _ = world.step(1.0 / 60.0);
    assert_eq!(world.play_camera2d().unwrap().center, [2.0, 0.0]);

    world.entity_mut(&cam).unwrap().camera2d.smoothing = 0.5;
    world.entity_mut(&target).unwrap().translation = [10.0, 0.0, 0.0];
    let _ = world.step(0.5);
    let x = world.play_camera2d().unwrap().center[0];
    let follow = 1.0 - (-1.0f32).exp();
    assert!((x - (2.0 + 6.0 * follow)).abs() < 1e-3, "smoothing still eases the move, x={x}");
}

#[test]
fn a_damaged_drag_field_loads_as_zero_and_a_good_one_round_trips() {
    let mut world = World::demo();
    let cam = world.add("camera");
    world.entity_mut(&cam).unwrap().camera2d.on = true;
    world.entity_mut(&cam).unwrap().camera2d.drag = [0.25, 0.1, 0.0, 0.4];
    let loaded = World::from_text(&world.to_text()).unwrap();
    assert_eq!(loaded.entity(&cam).unwrap().camera2d.drag, [0.25, 0.1, 0.0, 0.4]);

    let legacy = world.to_legacy_text();
    assert!(legacy.contains("cam_drag"), "{legacy}");
    let loaded = World::from_text(&legacy).unwrap();
    assert_eq!(loaded.entity(&cam).unwrap().camera2d.drag, [0.25, 0.1, 0.0, 0.4]);
    let broken = legacy.replace("0.25, 0.1, 0, 0.4", "nope");
    let loaded = World::from_text(&broken).unwrap();
    assert_eq!(loaded.entity(&cam).unwrap().camera2d.drag, [0.0; 4]);

    world.entity_mut(&cam).unwrap().camera2d.drag = [0.0; 4];
    let legacy = world.to_legacy_text();
    assert!(!legacy.contains("cam_drag"), "zero margins are left out of the older file");
}
