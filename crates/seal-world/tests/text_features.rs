//! Sprites in text, string tables, typewriter nodes and sprite sockets.

use seal_graph::rt::Host;
use seal_graph::{Graph, GraphError, GraphInstance, Interp, Node, Program, RunState, Ty, Value};
use seal_sealtext::{SealMap, SealValue};
use seal_world::PlayMode;
use seal_world::World;

fn text(s: &str) -> SealValue {
    SealValue::String(s.into())
}

fn defaults(pairs: &[(&str, SealValue)]) -> SealValue {
    SealValue::Map(SealMap::new(pairs.iter().map(|(k, v)| ((*k).into(), v.clone())).collect()))
}

struct Rec {
    log: Vec<String>,
    typewriter: Vec<(String, bool)>,
}

impl Host for Rec {
    fn get(&mut self, _: u64, _: &str, _: &str, _: Ty) -> Result<Value, String> {
        Err("no object".into())
    }
    fn set(&mut self, _: u64, _: &str, _: &str, _: Value) -> Result<(), String> {
        Ok(())
    }
    fn log(&mut self, message: &str) {
        self.log.push(message.to_string());
    }
    fn get_string(&mut self, table: &str, key: &str, values: &str) -> String {
        format!("{table}|{key}|{values}")
    }
    fn typewriter(&mut self, element: &str, skip: bool) -> Result<(), String> {
        self.typewriter.push((element.to_string(), skip));
        Ok(())
    }
}

fn run(graph: Graph, host: &mut Rec) -> Result<RunState, GraphError> {
    let program = Program::build(&graph).unwrap_or_else(|e| panic!("{e:?}"));
    let mut interp = Interp::new(program, 0);
    interp.fire("start", &[], host)
}

#[test]
fn get_string_and_typewriter_nodes_reach_the_host() {
    let lookup = Graph::new("lookup")
        .node(Node::new(1, "event").with("name", text("start")))
        .node(Node::new(2, "get_string").with(
            "defaults",
            defaults(&[
                ("table", text("ui/lines.strings")),
                ("key", text("coin")),
                ("values", text("count=3")),
            ]),
        ))
        .node(Node::new(3, "log"))
        .link(1, "then", 3, "in")
        .link(2, "out", 3, "message");
    let mut host = Rec { log: Vec::new(), typewriter: Vec::new() };
    run(lookup, &mut host).unwrap();
    assert_eq!(host.log, vec!["ui/lines.strings|coin|count=3".to_string()]);

    let skip = Graph::new("skip")
        .node(Node::new(1, "event").with("name", text("start")))
        .node(Node::new(2, "typewriter").with(
            "defaults",
            defaults(&[("element", text("Title")), ("skip", SealValue::Bool(true))]),
        ))
        .link(1, "then", 2, "in");
    run(skip, &mut host).unwrap();
    assert_eq!(host.typewriter, vec![("Title".into(), true)]);
}

#[test]
fn a_child_follows_its_parents_socket_while_playing() {
    let dir = std::env::temp_dir().join(format!("seal-socket-play-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("hero.frames"),
        "# Seal sprite animations\nanimation idle fps=8 loop=yes\nframe hero.png\nsocket hand 0.8 0.2\n",
    )
    .unwrap();

    let mut world = World::demo();
    world.project_dir = Some(dir.clone());
    let hero_id = world.add("empty");
    let hat_id = world.add("empty");
    {
        let hero = world.entity_mut(&hero_id).unwrap();
        hero.sprite = "hero.frames".into();
        hero.animation.clear();
        hero.translation = [1.0, 2.0, 0.0];
        hero.scale = [2.0, 2.0, 1.0];
        hero.rotation = [0.0, 0.0, 0.0];
        hero.flip = [false, false];
        hero.dynamic = false;
    }
    {
        let hat = world.entity_mut(&hat_id).unwrap();
        hat.parent = Some(hero_id.clone());
        hat.socket = "hand".into();
        hat.translation = [9.0, 9.0, 0.0];
        hat.dynamic = false;
    }
    world.mode = PlayMode::Editing;
    world.step(1.0 / 60.0);
    assert_eq!(world.entity_mut(&hat_id).unwrap().translation, [9.0, 9.0, 0.0]);
    world.mode = PlayMode::Playing;
    world.step(1.0 / 60.0);
    let hero = world.entity_mut(&hero_id).unwrap().translation;
    let hat = world.entity_mut(&hat_id).unwrap().translation;
    assert!((hat[0] - hero[0] - 0.6).abs() < 0.001, "{hat:?} {hero:?}");
    assert!((hat[1] - hero[1] - 0.6).abs() < 0.001, "{hat:?} {hero:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
