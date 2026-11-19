//! Request attention sets the play flag.

use seal_world::World;

const GRAPH: &str = r#"
schema_version = 1
seal_graph = "attention"
vars = []
nodes = [
  { id = 1, kind = "event", name = "start" },
  { id = 2, kind = "request_attention" },
]
links = [
  "1.then -> 2.in",
]
"#;

#[test]
fn request_attention_sets_flag() {
    let dir = std::env::temp_dir().join(format!("seal-attn-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("logic")).unwrap();
    std::fs::write(dir.join("logic/attention.graph"), GRAPH).unwrap();
    let mut world = World::starter();
    for entity in &mut world.entities {
        entity.script.clear();
        entity.dynamic = false;
    }
    let id = world.add("empty");
    world.entity_mut(&id).unwrap().script = "logic/attention.graph".into();
    world.project_dir = Some(dir);
    world.play();
    world.step(0.0);
    assert!(world.play_game().unwrap().attention_request);
}
