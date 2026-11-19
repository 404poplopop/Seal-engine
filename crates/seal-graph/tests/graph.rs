use seal_graph::ecs_host::EcsHost;
use seal_graph::rt::{GraphInstance, MapHost, RunState, Value};
use seal_graph::{node_sig, node_template, Graph, Interp, Node, Program, KINDS};
use seal_sealtext::SealValue;

fn s(v: &str) -> SealValue {
    SealValue::String(v.into())
}
fn n(v: i64) -> SealValue {
    SealValue::Number(v.to_string())
}

/// start -> sequence(3): set a = 1, set a = a + 10 (with a breakpoint), log a.
fn sample() -> Graph {
    Graph::new("sample")
        .var("a", Value::Int(0))
        .node(Node::new(1, "event").with("name", s("start")))
        .node(Node::new(2, "sequence").with("count", n(3)))
        .node(
            Node::new(3, "set")
                .with("name", s("a"))
                .with("defaults", seal_sealtext::parse("value = 1").unwrap()),
        )
        .node(Node::new(4, "get").with("name", s("a")))
        .node(
            Node::new(5, "add")
                .with("type", s("int"))
                .with("defaults", seal_sealtext::parse("b = 10").unwrap()),
        )
        .node(Node::new(6, "set").with("name", s("a")))
        .node(Node::new(7, "to_str").with("type", s("int")))
        .node(Node::new(8, "log"))
        .link(1, "then", 2, "in")
        .link(2, "then0", 3, "in")
        .link(2, "then1", 6, "in")
        .link(2, "then2", 8, "in")
        .link(4, "out", 5, "a")
        .link(5, "out", 6, "value")
        .link(4, "out", 7, "a")
        .link(7, "out", 8, "message")
}

fn interp(g: &Graph) -> Interp {
    Interp::new(Program::build(g).unwrap(), 0)
}

#[test]
fn text_round_trip_keeps_everything() {
    let mut g = sample();
    g.nodes[0].set("x", n(120));
    let text = g.to_text();
    let back = Graph::from_text(&text).unwrap();
    assert_eq!(back.to_text(), text);
    assert_eq!(Graph::from_text(&back.to_text()).unwrap(), back);
    assert_eq!(back.nodes[0].param("x"), Some(&n(120)));
    assert!(text.contains("seal_graph = \"sample\""));
    assert!(text.contains("1.then -> 2.in"));
}

#[test]
fn newer_schema_is_refused() {
    let text = sample().to_text().replace("schema_version = 1", "schema_version = 9");
    assert!(Graph::from_text(&text).unwrap_err().message.contains("newer"));
}

#[test]
fn checker_reports_every_problem() {
    let g = Graph::new("bad")
        .var("x", Value::Int(0))
        .node(Node::new(1, "event").with("name", s("start")))
        .node(Node::new(2, "log"))
        .node(Node::new(3, "log"))
        .node(Node::new(4, "get").with("name", s("x")))
        .node(Node::new(5, "frobnicate"))
        .link(1, "then", 2, "in")
        .link(1, "then", 3, "in")
        .link(4, "out", 2, "message");
    let errs = Program::build(&g).unwrap_err();
    let text: Vec<String> = errs.iter().map(|e| e.to_string()).collect();
    assert!(text.iter().any(|e| e.contains("unknown node kind frobnicate")), "{text:?}");

    let g = Graph::new("bad")
        .var("x", Value::Int(0))
        .node(Node::new(1, "event").with("name", s("start")))
        .node(Node::new(2, "log"))
        .node(Node::new(3, "log"))
        .node(Node::new(4, "get").with("name", s("x")))
        .link(1, "then", 2, "in")
        .link(1, "then", 3, "in")
        .link(4, "out", 2, "message");
    let text: Vec<String> = Program::build(&g).unwrap_err().iter().map(|e| e.to_string()).collect();
    assert!(text.iter().any(|e| e.contains("linked twice; add a sequence")), "{text:?}");
    assert!(text.iter().any(|e| e.contains("int into str; add a conversion node")), "{text:?}");
}

#[test]
fn data_loops_and_missing_entities_are_caught() {
    let g = Graph::new("loop")
        .node(Node::new(1, "add").with("type", s("int")))
        .node(Node::new(2, "add").with("type", s("int")))
        .link(1, "out", 2, "a")
        .link(2, "out", 1, "a");
    let errs = Program::build(&g).unwrap_err();
    assert!(errs.iter().any(|e| e.message.contains("form a loop")));
    let g = Graph::new("ent").node(
        Node::new(1, "world_get")
            .with("component", s("c"))
            .with("field", s("f"))
            .with("type", s("int")),
    );
    let errs = Program::build(&g).unwrap_err();
    assert!(errs[0].message.contains("needs an entity"));
}

#[test]
fn breakpoint_pause_inspect_step_resume() {
    let g = sample();
    let mut plain = interp(&g);
    let mut host = MapHost::default();
    plain.fire("start", &[], &mut host).unwrap();

    let mut dbg = interp(&g);
    dbg.set_breakpoint(6, true);
    dbg.watch(true);
    let mut h = MapHost::default();
    assert_eq!(dbg.fire("start", &[], &mut h).unwrap(), RunState::Paused(6));
    assert_eq!(dbg.paused_at(), Some(6));
    assert_eq!(dbg.call_stack(), vec![6, 2]);
    assert_eq!(dbg.var("a"), Some(&Value::Int(1)));
    // Other events wait while paused.
    assert!(dbg.fire("start", &[], &mut h).is_err());
    // Edit a value from the debugger, then step: set runs, next stop is the sequence.
    dbg.set_var("a", Value::Int(5)).unwrap();
    assert_eq!(dbg.step(&mut h).unwrap(), RunState::Paused(2));
    assert_eq!(dbg.var("a"), Some(&Value::Int(15)));
    assert_eq!(dbg.watched(5, 0), Some(&Value::Int(15)));
    assert_eq!(dbg.resume(&mut h).unwrap(), RunState::Done);
    assert_eq!(h.logs, vec!["15"]);
    assert_eq!(host.logs, vec!["11"]);

    // Without the edit, a debugged run ends exactly like a plain one.
    let mut dbg = interp(&g);
    dbg.set_breakpoint(6, true);
    dbg.set_breakpoint(8, true);
    let mut h = MapHost::default();
    let mut state = dbg.fire("start", &[], &mut h).unwrap();
    let mut stops = Vec::new();
    while let RunState::Paused(id) = state {
        stops.push(id);
        state = dbg.resume(&mut h).unwrap();
    }
    assert_eq!(stops, vec![6, 8]);
    assert_eq!(h, host);
    assert_eq!(dbg.vars(), plain.vars());
}

#[test]
fn trace_lists_nodes_in_order() {
    let mut i = interp(&sample());
    i.trace(true);
    i.fire("start", &[], &mut MapHost::default()).unwrap();
    // The sequence shows once on entry and again before each branch.
    assert_eq!(i.take_trace(), vec![2, 2, 3, 2, 6, 2, 8]);
}

#[test]
fn live_reload_keeps_values_and_waiting_delays() {
    let base = Graph::new("live")
        .var("hits", Value::Int(0))
        .var("speed", Value::Float(1.0))
        .node(Node::new(1, "event").with("name", s("start")))
        .node(
            Node::new(2, "delay").with("defaults", seal_sealtext::parse("seconds = 1.0").unwrap()),
        )
        .node(Node::new(3, "get").with("name", s("hits")))
        .node(
            Node::new(4, "add")
                .with("type", s("int"))
                .with("defaults", seal_sealtext::parse("b = 1").unwrap()),
        )
        .node(Node::new(5, "set").with("name", s("hits")))
        .link(1, "then", 2, "in")
        .link(2, "then", 5, "in")
        .link(3, "out", 4, "a")
        .link(4, "out", 5, "value");
    let mut i = interp(&base);
    let mut h = MapHost::default();
    i.set_var("hits", Value::Int(7)).unwrap();
    i.fire("start", &[], &mut h).unwrap();
    assert_eq!(i.pending_delays(), 1);
    // Edit while playing: add 100 instead of 1, and change speed to an int.
    let mut edited = base.clone();
    edited.nodes[3].set("defaults", seal_sealtext::parse("b = 100").unwrap());
    edited.vars[1] = seal_graph::VarDef {
        name: "speed".into(),
        ty: seal_graph::Ty::Int,
        default: Value::Int(3),
    };
    let report = i.reload(Program::build(&edited).unwrap());
    assert_eq!(report.kept_vars, vec!["hits"]);
    assert_eq!(report.reset_vars, vec!["speed"]);
    assert_eq!((report.kept_delays, report.dropped_delays), (1, 0));
    i.advance(1.0, &mut h).unwrap();
    assert_eq!(i.var("hits"), Some(&Value::Int(107)));
    // Removing the delay node drops its wait.
    i.fire("start", &[], &mut h).unwrap();
    let mut gone = edited.clone();
    gone.nodes.remove(1);
    gone.links.retain(|l| l.from.node != 2 && l.to.node != 2);
    let report = i.reload(Program::build(&gone).unwrap());
    assert_eq!((report.kept_delays, report.dropped_delays), (0, 1));
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Health {
    hp: i64,
    regen: f32,
    at: [f32; 2],
    label: String,
}
seal_ecs::text_component!(Health as "health" { hp, regen, at, label });

#[test]
fn graphs_read_and_write_world_components() {
    let g = Graph::new("heal")
        .node(Node::new(1, "event").with("name", s("tick")))
        .node(Node::new(2, "self"))
        .node(
            Node::new(3, "world_get")
                .with("component", s("health"))
                .with("field", s("hp"))
                .with("type", s("int")),
        )
        .node(
            Node::new(4, "add")
                .with("type", s("int"))
                .with("defaults", seal_sealtext::parse("b = 5").unwrap()),
        )
        .node(
            Node::new(5, "world_set")
                .with("component", s("health"))
                .with("field", s("hp"))
                .with("type", s("int")),
        )
        .node(
            Node::new(6, "world_get")
                .with("component", s("health"))
                .with("field", s("at"))
                .with("type", s("vec2")),
        )
        .node(
            Node::new(7, "add")
                .with("type", s("vec2"))
                .with("defaults", seal_sealtext::parse("b = [1.5, -2.0]").unwrap()),
        )
        .node(
            Node::new(8, "world_set")
                .with("component", s("health"))
                .with("field", s("at"))
                .with("type", s("vec2")),
        )
        .node(Node::new(9, "to_str").with("type", s("entity")))
        .node(Node::new(10, "log"))
        .link(1, "then", 5, "in")
        .link(5, "then", 8, "in")
        .link(8, "then", 10, "in")
        .link(2, "out", 3, "entity")
        .link(2, "out", 5, "entity")
        .link(2, "out", 6, "entity")
        .link(2, "out", 8, "entity")
        .link(2, "out", 9, "a")
        .link(3, "out", 4, "a")
        .link(4, "out", 5, "value")
        .link(6, "out", 7, "a")
        .link(7, "out", 8, "value")
        .link(9, "out", 10, "message");
    let mut world = seal_ecs::World::new();
    world.register_text::<Health>();
    let e = world.spawn((Health { hp: 10, regen: 0.5, at: [1.0, 1.0], label: "x".into() },));
    let mut i = Interp::new(Program::build(&g).unwrap(), e.to_bits());
    let mut host = EcsHost::new(&mut world);
    i.advance(0.016, &mut host).unwrap();
    i.advance(0.016, &mut host).unwrap();
    assert_eq!(host.logs, vec![format!("{e:?}"), format!("{e:?}")]);
    let h = world.get::<Health>(e).unwrap();
    assert_eq!(h.hp, 20);
    assert_eq!(h.at, [4.0, -3.0]);
    assert_eq!(h.label, "x");
}

/// spawn a "Coin" prefab into a real world, raise its hp, then despawn it.
#[test]
fn spawn_and_despawn_in_a_world() {
    use seal_ecs::{Prefab, PrefabLibrary};
    let g = Graph::new("coins")
        .var("last", Value::Entity(seal_graph::rt::NO_ENTITY))
        .node(Node::new(1, "event").with("name", s("make")))
        .node(Node::new(2, "spawn").with("prefab", s("Coin")))
        .node(Node::new(3, "set").with("name", s("last")))
        .node(
            Node::new(4, "world_set")
                .with("component", s("health"))
                .with("field", s("hp"))
                .with("type", s("int"))
                .with("defaults", seal_sealtext::parse("value = 7").unwrap()),
        )
        .node(Node::new(10, "event").with("name", s("kill")))
        .node(Node::new(11, "get").with("name", s("last")))
        .node(Node::new(12, "despawn"))
        .node(Node::new(13, "is_valid"))
        .node(Node::new(14, "to_str").with("type", s("bool")))
        .node(Node::new(15, "log"))
        .link(2, "entity", 3, "value")
        .link(1, "then", 2, "in")
        .link(2, "then", 3, "in")
        .link(3, "then", 4, "in")
        .link(3, "value", 4, "entity")
        .link(10, "then", 12, "in")
        .link(11, "out", 12, "entity")
        .link(12, "then", 15, "in")
        .link(11, "out", 13, "entity")
        .link(13, "out", 14, "a")
        .link(14, "out", 15, "message");
    let mut lib = PrefabLibrary::new();
    lib.insert(Prefab::from_text("seal_prefab = 1\nprefab = \"Coin\"\ncomponents = { health = { hp = 1, regen = 0, at = [0, 0], label = \"coin\" } }\n").unwrap());
    let mut world = seal_ecs::World::new();
    world.register_text::<Health>();
    let mut i = Interp::new(Program::build(&g).unwrap(), 0);
    let e = {
        let mut host = EcsHost::new(&mut world).with_prefabs(&lib);
        i.fire("make", &[], &mut host).unwrap();
        let e = seal_ecs::Entity::from_bits(i.var("last").unwrap().entity());
        assert_eq!(host.world.get::<Health>(e).unwrap().hp, 7);
        assert_eq!(host.world.get::<Health>(e).unwrap().label, "coin");
        i.fire("kill", &[], &mut host).unwrap();
        assert_eq!(host.logs, ["false"]);
        i.fire("kill", &[], &mut host).unwrap();
        e
    };
    assert!(!world.contains(e));
    // Without prefabs a spawn is an error at the node, not a panic.
    let mut host = EcsHost::new(&mut world);
    let err = i.fire("make", &[], &mut host).unwrap_err();
    assert!(format!("{err:?}").contains("no prefabs"), "{err:?}");
}

#[test]
fn empty_entity_variables_round_trip_through_text() {
    let g = Graph::new("e")
        .var("who", Value::Entity(seal_graph::rt::NO_ENTITY))
        .node(Node::new(1, "event").with("name", s("start")));
    let text = g.to_text();
    assert!(text.contains("\"none\""), "{text}");
    let back = Graph::from_text(&text).unwrap();
    let i = Interp::new(Program::build(&back).unwrap(), 0);
    assert_eq!(i.var("who"), Some(&Value::Entity(seal_graph::rt::NO_ENTITY)));
}

#[test]
fn vec3_fields_move_a_real_transform() {
    use seal_ecs::core::Transform;
    let tr = |k: &str| {
        Node::new(0, k)
            .with("component", s("transform"))
            .with("field", s("translation"))
            .with("type", s("vec3"))
    };
    let mut get = tr("world_get");
    get.id = 2;
    let mut set = tr("world_set");
    set.id = 4;
    let g = Graph::new("mover3")
        .node(Node::new(1, "event").with("name", s("step")))
        .node(get)
        .node(
            Node::new(3, "add")
                .with("type", s("vec3"))
                .with("defaults", seal_sealtext::parse("b = [0.5, -1.0, 2.0]").unwrap()),
        )
        .node(set)
        .node(Node::new(5, "self"))
        .link(5, "out", 2, "entity")
        .link(2, "out", 3, "a")
        .link(1, "then", 4, "in")
        .link(5, "out", 4, "entity")
        .link(3, "out", 4, "value");
    let mut world = seal_ecs::World::new();
    world.register_core();
    let e = world.spawn((Transform { translation: [1.0, 2.0, 3.0], ..Transform::default() },));
    let mut i = Interp::new(Program::build(&g).unwrap(), e.to_bits());
    let mut host = EcsHost::new(&mut world);
    i.fire("step", &[], &mut host).unwrap();
    i.fire("step", &[], &mut host).unwrap();
    assert_eq!(world.get::<Transform>(e).unwrap().translation, [2.0, 0.0, 7.0]);
}

/// `is_a` in a real world follows the prefab base chain.
#[test]
fn is_a_in_a_world_follows_prefab_bases() {
    use seal_ecs::{Prefab, PrefabLibrary};
    let g = Graph::new("kinds")
        .var("last", Value::Entity(seal_graph::rt::NO_ENTITY))
        .node(Node::new(1, "event").with("name", s("make")))
        .node(Node::new(2, "spawn").with("prefab", s("Boss")))
        .node(Node::new(3, "set").with("name", s("last")))
        .node(Node::new(10, "event").with("name", s("check")))
        .node(Node::new(11, "get").with("name", s("last")))
        .node(Node::new(12, "is_a").with("prefab", s("Enemy")))
        .node(Node::new(13, "is_a").with("prefab", s("Coin")))
        .node(Node::new(14, "to_str").with("type", s("bool")))
        .node(Node::new(15, "to_str").with("type", s("bool")))
        .node(Node::new(16, "concat"))
        .node(Node::new(17, "log"))
        .link(1, "then", 2, "in")
        .link(2, "then", 3, "in")
        .link(2, "entity", 3, "value")
        .link(10, "then", 17, "in")
        .link(11, "out", 12, "entity")
        .link(11, "out", 13, "entity")
        .link(12, "out", 14, "a")
        .link(13, "out", 15, "a")
        .link(14, "out", 16, "a")
        .link(15, "out", 16, "b")
        .link(16, "out", 17, "message");
    let mut lib = PrefabLibrary::new();
    lib.insert(
        Prefab::from_text("seal_prefab = 1\nprefab = \"Enemy\"\ncomponents = {}\n").unwrap(),
    );
    lib.insert(
        Prefab::from_text(
            "seal_prefab = 1\nprefab = \"Boss\"\nbase = \"Enemy\"\ncomponents = {}\n",
        )
        .unwrap(),
    );
    lib.insert(Prefab::from_text("seal_prefab = 1\nprefab = \"Coin\"\ncomponents = {}\n").unwrap());
    let mut world = seal_ecs::World::new();
    let mut i = Interp::new(Program::build(&g).unwrap(), 0);
    let mut host = EcsHost::new(&mut world).with_prefabs(&lib);
    i.fire("check", &[], &mut host).unwrap();
    i.fire("make", &[], &mut host).unwrap();
    i.fire("check", &[], &mut host).unwrap();
    assert_eq!(host.logs, ["falsefalse", "truefalse"]);
}

#[test]
fn find_all_in_a_world_lists_entities_by_prefab_base() {
    use seal_ecs::{Prefab, PrefabLibrary};
    let g = Graph::new("census")
        .node(Node::new(1, "event").with("name", s("boss")))
        .node(Node::new(2, "spawn").with("prefab", s("Boss")))
        .node(Node::new(3, "event").with("name", s("coin")))
        .node(Node::new(4, "spawn").with("prefab", s("Coin")))
        .node(Node::new(10, "event").with("name", s("count")))
        .node(Node::new(11, "find_all").with("prefab", s("Enemy")))
        .node(Node::new(12, "list_len").with("type", s("entity")))
        .node(Node::new(13, "to_str").with("type", s("int")))
        .node(Node::new(14, "log"))
        .link(1, "then", 2, "in")
        .link(3, "then", 4, "in")
        .link(10, "then", 14, "in")
        .link(11, "out", 12, "list")
        .link(12, "out", 13, "a")
        .link(13, "out", 14, "message");
    let mut lib = PrefabLibrary::new();
    lib.insert(
        Prefab::from_text("seal_prefab = 1\nprefab = \"Enemy\"\ncomponents = {}\n").unwrap(),
    );
    lib.insert(
        Prefab::from_text(
            "seal_prefab = 1\nprefab = \"Boss\"\nbase = \"Enemy\"\ncomponents = {}\n",
        )
        .unwrap(),
    );
    lib.insert(Prefab::from_text("seal_prefab = 1\nprefab = \"Coin\"\ncomponents = {}\n").unwrap());
    let mut world = seal_ecs::World::new();
    let mut i = Interp::new(Program::build(&g).unwrap(), 0);
    let mut host = EcsHost::new(&mut world).with_prefabs(&lib);
    for e in ["count", "boss", "coin", "boss", "count"] {
        i.fire(e, &[], &mut host).unwrap();
    }
    assert_eq!(host.logs, ["0", "2"]);
}

#[test]
fn float_text_matches_the_classic_display() {
    use seal_graph::rt::ops::float_text;
    let cases = [
        (5.0, "5.0"),
        (0.1, "0.1"),
        (-0.0, "-0.0"),
        (3.00001, "3.00001"),
        (1e-7, "0.0"),
        (-2.5, "-2.5"),
        (1234567.125, "1234567.125"),
        (f64::INFINITY, "inf"),
        (f64::NAN, "nan"),
    ];
    for (v, want) in cases {
        assert_eq!(float_text(v), want, "{v}");
    }
}

#[test]
fn every_node_template_has_pins_once_its_event_exists() {
    let base = Graph::new("t").var("v", Value::Float(0.0)).node(
        Node::new(9, "event").with("name", seal_sealtext::SealValue::String("my_event".into())),
    );
    for (kind, _) in KINDS {
        let node = node_template(&base, 1, kind);
        let g = base.clone().node(node);
        assert!(node_sig(&g, 1).is_ok(), "{kind}: {:?}", node_sig(&g, 1).err());
    }
    let fresh = Graph::new("t");
    assert_eq!(
        node_template(&fresh, 1, "event").param("name"),
        Some(&seal_sealtext::SealValue::String("start".into()))
    );
    let with_start = fresh.node(node_template(&Graph::new("t"), 1, "event"));
    assert_eq!(
        node_template(&with_start, 2, "event").param("name"),
        Some(&seal_sealtext::SealValue::String("tick".into()))
    );
}

#[test]
fn music_nodes_reach_the_host() {
    let defaults = |text: &str| seal_sealtext::parse(text).unwrap();
    let g = Graph::new("music")
        .node(Node::new(1, "event").with("name", s("start")))
        .node(
            Node::new(2, "play_music").with(
                "defaults",
                defaults("file = \"Audio/Theme\"\nlooping = true\nfade = 1.5\nloop_start = 2.25"),
            ),
        )
        .node(Node::new(3, "set_music_volume").with("defaults", defaults("volume = 0.4")))
        .node(Node::new(4, "stop_music").with("defaults", defaults("fade = 0.25")))
        .link(1, "then", 2, "in")
        .link(2, "then", 3, "in")
        .link(3, "then", 4, "in");
    let mut host = MapHost::default();
    interp(&g).fire("start", &[], &mut host).unwrap();
    assert_eq!(
        host.music,
        vec![
            "play Audio/Theme true 1.5 2.25".to_string(),
            "volume 0.4".to_string(),
            "stop 0.25".to_string(),
        ]
    );
}

#[test]
fn camera_shake_node_reaches_the_host() {
    let g = Graph::new("shake")
        .node(Node::new(1, "event").with("name", s("start")))
        .node(
            Node::new(2, "camera_shake")
                .with("defaults", seal_sealtext::parse("strength = 0.5\nduration = 0.25").unwrap()),
        )
        .link(1, "then", 2, "in");
    let mut host = MapHost::default();
    interp(&g).fire("start", &[], &mut host).unwrap();
    assert_eq!(host.shakes, vec![(0.5, 0.25)]);
}
