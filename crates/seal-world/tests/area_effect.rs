//! Scene areas: save and load, the moving surface under the character, and script events.

use seal_world::{area_effect, World};

#[test]
fn areas_round_trip_and_old_scenes_stay_unchanged() {
    let demo = World::demo().to_text();
    assert!(!demo.contains("area_effect"), "{demo}");
    assert!(!demo.contains("surface_effect"));
    let loaded = World::from_text(&demo).unwrap();
    assert!(loaded.entities.iter().all(|e| e.area.mode.is_empty() && e.surface.speed == 0.0));

    let mut world = World::starter();
    let id = world.add("water_area");
    {
        let e = world.entity_mut(&id).unwrap();
        e.area.density = 2.5;
        e.area.flow = [1.0, 0.0];
        e.surface.speed = 1.5;
    }
    let text = world.to_text();
    assert!(text.contains("area_effect"));
    assert!(text.contains("surface_effect"));
    let back = World::from_text(&text).unwrap();
    let e = back.entity(&id).unwrap();
    assert_eq!(e.area.mode, "buoyancy");
    assert_eq!(e.area.density, 2.5);
    assert_eq!(e.area.flow, [1.0, 0.0]);
    assert_eq!(e.surface.speed, 1.5);
    assert!(!area_effect::gizmos(e).is_empty());

    world.entity_mut(&id).unwrap().area.mode.clear();
    world.entity_mut(&id).unwrap().surface.speed = 0.0;
    let cleared = world.to_text();
    assert!(!cleared.contains("area_effect"));
    assert!(!cleared.contains("surface_effect"));
}

#[test]
fn a_bad_mode_is_refused_and_presets_match_their_names() {
    let mut world = World::starter();
    let id = world.entities[0].id.clone();
    {
        let e = world.entity_mut(&id).unwrap();
        assert!(!area_effect::apply(&mut e.area, &mut e.surface, "area_mode", "sideways"));
        assert!(e.area.mode.is_empty());
        assert!(area_effect::apply(&mut e.area, &mut e.surface, "area_mode", "drag"));
        assert_eq!(e.area.mode, "drag");
    }

    for (kind, mode) in [
        ("wind_area", "directional"),
        ("water_area", "buoyancy"),
        ("gravity_area", "gravity"),
        ("attractor", "point"),
    ] {
        let id = world.add(kind);
        let e = world.entity(&id).unwrap();
        assert_eq!(e.area.mode, mode, "{kind}");
        assert_eq!(e.phys2d.body, "fixed");
        assert!(e.shape2d.closed);
        assert!(!area_effect::gizmos(e).is_empty(), "{kind}");
    }
}

#[test]
fn a_moving_surface_carries_the_character() {
    let mut world = World::starter();
    let player = world.entities.iter().find(|e| e.tag == "player").unwrap().id.clone();
    {
        let e = world.entity_mut(&player).unwrap();
        e.sprite = "hero.png".into();
        e.translation = [0.0, 0.95, 0.0];
    }
    let belt = world.add("empty");
    {
        let e = world.entity_mut(&belt).unwrap();
        e.translation = [0.0, 0.0, 0.0];
        e.phys2d.body = "fixed".into();
        e.phys2d.size = [20.0, 1.0];
        e.surface.speed = 4.0;
    }
    world.play();
    for _ in 0..120 {
        assert!(world.step(1.0 / 60.0));
    }
    let x = world.entity(&player).unwrap().translation[0];
    assert!(x > 3.0, "character x {x}");
}

const AREA_GRAPH: &str = r#"
schema_version = 1
seal_graph = "area"
vars = []
nodes = [
  { id = 1, kind = "event", name = "tick" },
  { id = 2, kind = "self" },
  { id = 3, kind = "bodies_inside" },
  { id = 4, kind = "list_len", type = "entity" },
  { id = 5, kind = "to_str", type = "int" },
  { id = 6, kind = "log" },
  { id = 7, kind = "event", name = "body_entered", params = [{ name = "body", type = "entity" }] },
  { id = 8, kind = "log", defaults = { message = "entered" } },
  { id = 9, kind = "event", name = "body_exited", params = [{ name = "body", type = "entity" }] },
  { id = 10, kind = "log", defaults = { message = "exited" } },
]
links = [
  "1.then -> 6.in",
  "2.out -> 3.entity",
  "3.out -> 4.list",
  "4.out -> 5.a",
  "5.out -> 6.message",
  "7.then -> 8.in",
  "9.then -> 10.in",
]
"#;

#[test]
fn scripts_hear_bodies_entering_and_can_list_who_is_inside() {
    let dir = std::env::temp_dir().join(format!("seal-area-script-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("logic")).unwrap();
    std::fs::write(dir.join("logic/area.graph"), AREA_GRAPH).unwrap();

    let mut world = World::starter();
    world.gravity = 0.0;
    let area = world.add("wind_area");
    {
        let e = world.entity_mut(&area).unwrap();
        e.phys2d.size = [1.0, 4.0];
        e.area.strength = 80.0;
        e.script = "logic/area.graph".into();
    }
    let box_id = world.add("cube");
    {
        let e = world.entity_mut(&box_id).unwrap();
        e.translation = [0.0, 0.0, 0.0];
        e.phys2d.body = "dynamic".into();
    }
    world.project_dir = Some(dir.clone());
    world.play();
    assert!(world.step(1.0 / 60.0));
    assert!(world.step(1.0 / 60.0));
    assert!(world.play_log.windows(2).any(|w| w == ["0", "entered"]), "{:?}", world.play_log);
    assert!(world.play_log.iter().any(|line| line == "1"), "{:?}", world.play_log);
    for _ in 0..40 {
        assert!(world.step(1.0 / 60.0));
    }
    assert!(world.play_log.iter().any(|line| line == "exited"), "{:?}", world.play_log);
    let _ = box_id;
    std::fs::remove_dir_all(dir).ok();
}
