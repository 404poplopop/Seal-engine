//! Update groups, remote transform, runtime collider shapes, and multi-hit queries.

use seal_world::World;

fn project(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("seal-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("logic")).unwrap();
    dir
}

fn write_graph(dir: &std::path::Path, name: &str, body: &str) {
    std::fs::write(dir.join(format!("logic/{name}.graph")), body).unwrap();
}

fn empty(name: &str) -> World {
    World::from_text(&World::empty_scene_text(name)).unwrap()
}

fn body(world: &mut World, kind: &str, at: [f32; 3], shape: &str, size: [f32; 2]) -> String {
    let id = world.add("empty");
    let entity = world.entity_mut(&id).unwrap();
    entity.translation = at;
    entity.phys2d.body = kind.into();
    entity.phys2d.shape = shape.into();
    entity.phys2d.size = size;
    id
}

#[test]
fn three_scripts_run_by_group_then_order_on_frame_and_fixed_ticks() {
    let dir = project("order");
    let graph = |name: &str, tick: &str, fixed: &str| {
        format!(
            "schema_version = 1\nseal_graph = \"{name}\"\nvars = []\nnodes = [\n\
             {{ id = 1, kind = \"event\", name = \"tick\" }},\n\
             {{ id = 2, kind = \"log\", defaults = {{ message = \"{tick}\" }} }},\n\
             {{ id = 3, kind = \"event\", name = \"fixed_tick\" }},\n\
             {{ id = 4, kind = \"log\", defaults = {{ message = \"{fixed}\" }} }},\n\
             ]\nlinks = [ \"1.then -> 2.in\", \"3.then -> 4.in\" ]\n"
        )
    };
    write_graph(&dir, "before", &graph("before", "B", "Bf"));
    write_graph(&dir, "after", &graph("after", "A", "Af"));
    write_graph(&dir, "late", &graph("late", "L", "Lf"));
    let mut world = empty("Order");
    let before = world.add("empty");
    let after = world.add("empty");
    let late = world.add("empty");
    {
        let entity = world.entity_mut(&before).unwrap();
        entity.script = "logic/before.graph".into();
        entity.script_order = 5;
        entity.script_group.clear();
    }
    {
        let entity = world.entity_mut(&after).unwrap();
        entity.script = "logic/after.graph".into();
        entity.script_order = 0;
        entity.script_group = "after_physics".into();
    }
    {
        let entity = world.entity_mut(&late).unwrap();
        entity.script = "logic/late.graph".into();
        entity.script_order = -10;
        entity.script_group = "late".into();
    }
    world.project_dir = Some(dir.clone());
    world.play();
    world.play_log.clear();
    assert!(world.step(1.0 / 60.0), "{:?}", world.play_log);
    assert_eq!(
        world.play_log,
        ["B", "Bf", "A", "Af", "L", "Lf"],
        "groups beat a lower order number: {:?}",
        world.play_log
    );
    let text = world.to_text();
    assert!(text.contains("after_physics"), "{text}");
    assert!(text.contains("late"), "{text}");
    assert!(!text.contains("before_physics"), "the default group is not stored: {text}");
    let back = World::from_text(&text).unwrap();
    assert_eq!(back.entity(&after).unwrap().script_group, "after_physics");
    assert_eq!(back.entity(&late).unwrap().script_group, "late");
    assert!(back.entity(&before).unwrap().script_group.is_empty());
    let broken = text.replacen("\"late\"", "4", 1);
    assert_ne!(broken, text);
    match World::from_text(&broken) {
        Ok(_) => panic!("a group that is not text must fail to load"),
        Err(err) => assert!(err.contains("group") || err.contains("script_group"), "{err}"),
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn remote_transform_follows_its_source_and_respects_toggles() {
    let mut world = empty("Remote");
    let source = world.add("empty");
    let target = world.add("empty");
    {
        let entity = world.entity_mut(&source).unwrap();
        entity.name = "Source".into();
        entity.translation = [2.0, 3.0, 0.0];
        entity.rotation = [0.0, 0.0, 40.0];
        entity.scale = [2.0, 3.0, 1.0];
        entity.remote.on = true;
        entity.remote.target = target.clone();
        entity.remote.position = true;
        entity.remote.rotation = false;
        entity.remote.scale = true;
    }
    world.entity_mut(&target).unwrap().translation = [9.0, 9.0, 0.0];
    world.entity_mut(&target).unwrap().rotation = [0.0, 0.0, 7.0];
    world.play();
    assert!(world.step(1.0 / 60.0) || world.mode == seal_world::PlayMode::Playing);
    let copied = world.entity(&target).unwrap();
    assert_eq!(copied.translation, [2.0, 3.0, 0.0]);
    assert_eq!(copied.rotation, [0.0, 0.0, 7.0], "rotation was switched off");
    assert_eq!(copied.scale, [2.0, 3.0, 1.0]);
    world.entity_mut(&source).unwrap().translation = [4.0, 1.0, 0.0];
    world.entity_mut(&source).unwrap().remote.rotation = true;
    world.entity_mut(&source).unwrap().rotation = [0.0, 0.0, 15.0];
    world.entity_mut(&source).unwrap().remote.scale = false;
    world.entity_mut(&source).unwrap().scale = [8.0, 8.0, 1.0];
    let _ = world.step(1.0 / 60.0);
    let copied = world.entity(&target).unwrap();
    assert_eq!(copied.translation, [4.0, 1.0, 0.0]);
    assert_eq!(copied.rotation[2], 15.0);
    assert_eq!(copied.scale, [2.0, 3.0, 1.0], "scale was switched off");
    let text = world.to_text();
    let back = World::from_text(&text).unwrap();
    assert!(back.entity(&source).unwrap().remote.on);
    assert_eq!(back.entity(&source).unwrap().remote.target, target);
    assert!(!back.entity(&source).unwrap().remote.scale);
    let broken = text.replacen("position = true", "position = \"nope\"", 1);
    if broken == text {
        let broken = text.replacen("position = yes", "position = \"nope\"", 1);
        assert_ne!(broken, text, "remote transform was not saved:\n{text}");
        assert!(World::from_text(&broken).is_err(), "{broken}");
    } else {
        assert!(World::from_text(&broken).is_err(), "{broken}");
    }
}

#[test]
fn set_shape_changes_collisions_in_a_stepped_world() {
    let dir = project("shape");
    write_graph(
        &dir,
        "tall",
        r#"schema_version = 1
seal_graph = "tall"
vars = []
nodes = [
  { id = 1, kind = "event", name = "tick" },
  { id = 2, kind = "self" },
  { id = 3, kind = "const", type = "float", value = 1 },
  { id = 4, kind = "const", type = "float", value = 2 },
  { id = 5, kind = "vec2" },
  { id = 6, kind = "set_collider_shape", defaults = { shape = "box" } },
]
links = [
  "1.then -> 6.in",
  "2.out -> 6.entity",
  "3.out -> 5.x",
  "4.out -> 5.y",
  "5.out -> 6.size",
]
"#,
    );
    write_graph(
        &dir,
        "off",
        r#"schema_version = 1
seal_graph = "off"
vars = []
nodes = [
  { id = 1, kind = "event", name = "tick" },
  { id = 2, kind = "self" },
  { id = 3, kind = "set_collider_enabled", defaults = { enabled = false } },
]
links = [ "1.then -> 3.in", "2.out -> 3.entity" ]
"#,
    );
    write_graph(
        &dir,
        "poly",
        r#"schema_version = 1
seal_graph = "poly"
vars = []
nodes = [
  { id = 1, kind = "event", name = "tick" },
  { id = 2, kind = "self" },
  { id = 3, kind = "const", type = "list<vec2>", value = [[-0.5, -1], [0.5, -1], [0.5, 1], [-0.5, 1]] },
  { id = 4, kind = "set_collider_shape", defaults = { shape = "polygon" } },
]
links = [
  "1.then -> 4.in",
  "2.out -> 4.entity",
  "3.out -> 4.points",
]
"#,
    );
    write_graph(
        &dir,
        "bad",
        r#"schema_version = 1
seal_graph = "bad"
vars = []
nodes = [
  { id = 1, kind = "event", name = "tick" },
  { id = 2, kind = "self" },
  { id = 3, kind = "set_collider_shape", defaults = { shape = "triangle" } },
]
links = [ "1.then -> 3.in", "2.out -> 3.entity" ]
"#,
    );

    let run = |script: &str, shape: &str, size: [f32; 2]| {
        let mut world = empty("Hit");
        world.gravity = 0.0;
        let id = body(&mut world, "dynamic", [0.0, 0.0, 0.0], shape, size);
        let _wall = body(&mut world, "fixed", [1.2, 0.8, 0.0], "box", [1.0, 0.6]);
        world.entity_mut(&id).unwrap().script = format!("logic/{script}.graph");
        world.project_dir = Some(dir.clone());
        world.play();
        world.entity_mut(&id).unwrap().velocity = [3.0, 0.0, 0.0];
        for _ in 0..60 {
            world.step(1.0 / 60.0);
        }
        let x = world.entity(&id).unwrap().translation[0];
        let log = world.play_log.clone();
        (x, log)
    };

    let (open, _) = run("tall", "circle", [0.2, 0.2]);
    // The tall script replaces the small circle, so this call is the blocked one.
    // A scene with no script is the one that slips under the wall.
    let mut world = empty("Open");
    world.gravity = 0.0;
    let id = body(&mut world, "dynamic", [0.0, 0.0, 0.0], "circle", [0.2, 0.2]);
    let _wall = body(&mut world, "fixed", [1.2, 0.8, 0.0], "box", [1.0, 0.6]);
    world.play();
    world.entity_mut(&id).unwrap().velocity = [3.0, 0.0, 0.0];
    for _ in 0..60 {
        world.step(1.0 / 60.0);
    }
    let slipped = world.entity(&id).unwrap().translation[0];
    assert!(slipped > 2.0, "a short circle slips under the wall, x={slipped}");
    assert!(open < 1.0, "a tall box set by the script hits the wall, x={open}");

    let (poly, poly_log) = run("poly", "circle", [0.2, 0.2]);
    assert!(poly < 1.0, "a polygon set by the script hits the wall, x={poly} {:?}", poly_log);

    let (off, off_log) = run("off", "box", [1.0, 2.0]);
    assert!(off > 2.0, "a disabled collider no longer blocks, x={off} {:?}", off_log);

    let (bad, bad_log) = run("bad", "circle", [0.2, 0.2]);
    assert!(bad > 2.0, "a refused shape leaves the circle, x={bad}");
    assert!(bad_log.iter().any(|line| line.contains("unknown collider shape")), "{bad_log:?}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn raycast_all_and_overlap_shape_honour_distance_and_mask() {
    let dir = project("query");
    write_graph(
        &dir,
        "rays",
        r#"schema_version = 1
seal_graph = "rays"
vars = []
nodes = [
  { id = 1, kind = "event", name = "tick" },
  { id = 2, kind = "const", type = "float", value = -2 },
  { id = 3, kind = "const", type = "float", value = 1 },
  { id = 4, kind = "vec3" },
  { id = 5, kind = "vec3" },
  { id = 6, kind = "const", type = "float", value = 10 },
  { id = 7, kind = "const", type = "int", value = 0 },
  { id = 8, kind = "raycast_all" },
  { id = 9, kind = "list_len", type = "entity" },
  { id = 10, kind = "to_str", type = "int" },
  { id = 11, kind = "log" },
  { id = 12, kind = "const", type = "int", value = 1 },
  { id = 13, kind = "raycast_all" },
  { id = 14, kind = "list_len", type = "entity" },
  { id = 15, kind = "to_str", type = "int" },
  { id = 16, kind = "log" },
  { id = 17, kind = "list_get", type = "float" },
  { id = 18, kind = "const", type = "int", value = 0 },
  { id = 19, kind = "to_str", type = "float" },
  { id = 20, kind = "log" },
  { id = 21, kind = "list_get", type = "float" },
  { id = 22, kind = "const", type = "int", value = 1 },
  { id = 23, kind = "to_str", type = "float" },
  { id = 24, kind = "log" },
  { id = 25, kind = "const", type = "str", value = "box" },
  { id = 26, kind = "const", type = "float", value = 4 },
  { id = 27, kind = "const", type = "float", value = 1 },
  { id = 28, kind = "vec2" },
  { id = 29, kind = "overlap_shape" },
  { id = 30, kind = "list_len", type = "entity" },
  { id = 31, kind = "to_str", type = "int" },
  { id = 32, kind = "log" },
  { id = 33, kind = "overlap_shape" },
  { id = 34, kind = "list_len", type = "entity" },
  { id = 35, kind = "to_str", type = "int" },
  { id = 36, kind = "log" },
  { id = 37, kind = "vec3" },
]
links = [
  "1.then -> 11.in",
  "11.then -> 16.in",
  "16.then -> 20.in",
  "20.then -> 24.in",
  "24.then -> 32.in",
  "32.then -> 36.in",
  "2.out -> 4.x",
  "3.out -> 5.x",
  "4.out -> 8.from",
  "5.out -> 8.direction",
  "6.out -> 8.max_distance",
  "7.out -> 8.mask",
  "8.entities -> 9.list",
  "9.out -> 10.a",
  "10.out -> 11.message",
  "4.out -> 13.from",
  "5.out -> 13.direction",
  "6.out -> 13.max_distance",
  "12.out -> 13.mask",
  "13.entities -> 14.list",
  "14.out -> 15.a",
  "15.out -> 16.message",
  "8.distances -> 17.list",
  "18.out -> 17.index",
  "17.out -> 19.a",
  "19.out -> 20.message",
  "8.distances -> 21.list",
  "22.out -> 21.index",
  "21.out -> 23.a",
  "23.out -> 24.message",
  "25.out -> 29.shape",
  "26.out -> 28.x",
  "27.out -> 28.y",
  "28.out -> 29.size",
  "37.out -> 29.center",
  "7.out -> 29.mask",
  "29.out -> 30.list",
  "30.out -> 31.a",
  "31.out -> 32.message",
  "25.out -> 33.shape",
  "28.out -> 33.size",
  "37.out -> 33.center",
  "12.out -> 33.mask",
  "33.out -> 34.list",
  "34.out -> 35.a",
  "35.out -> 36.message",
]
"#,
    );
    let mut world = empty("Rays");
    let probe = world.add("empty");
    world.entity_mut(&probe).unwrap().script = "logic/rays.graph".into();
    for (x, layer) in [(0.0, 1u32), (2.0, 2), (4.0, 1)] {
        let id = body(&mut world, "fixed", [x, 0.0, 0.0], "box", [1.0, 1.0]);
        world.entity_mut(&id).unwrap().phys2d.layer = layer;
    }
    world.project_dir = Some(dir.clone());
    world.play();
    world.play_log.clear();
    assert!(world.step(1.0 / 60.0), "{:?}", world.play_log);
    assert!(world.play_log.len() >= 6, "ray and overlap logs: {:?}", world.play_log);
    assert_eq!(world.play_log[0], "3", "{:?}", world.play_log);
    assert_eq!(world.play_log[1], "2", "mask 1 skips the middle layer: {:?}", world.play_log);
    let first: f64 = world.play_log[2].parse().expect(&world.play_log[2]);
    let second: f64 = world.play_log[3].parse().expect(&world.play_log[3]);
    assert!(first < second, "nearest first: {first} then {second}");
    assert!((first - 1.5).abs() < 1e-3, "first hit distance {first}");
    assert_eq!(world.play_log[4], "2", "wide box covers the first two: {:?}", world.play_log);
    assert_eq!(
        world.play_log[5], "1",
        "mask keeps only layer 1 inside the box: {:?}",
        world.play_log
    );
    std::fs::remove_dir_all(&dir).ok();
}
