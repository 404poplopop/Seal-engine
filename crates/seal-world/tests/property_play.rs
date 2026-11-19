use seal_graph::{Graph, Node, Value};
use seal_scene::sequence::Sequence;
use seal_sealtext::SealValue;
use seal_world::gameui::{Live, Node as UiNode, Screen, Theme};
use seal_world::World;

fn seq(text: &str) -> Sequence {
    Sequence::parse(text).unwrap()
}

#[test]
fn a_property_track_applies_to_the_live_object_while_playing_and_scrubbing() {
    let mut world = World::demo();
    let id = world.entities[1].id.clone();
    world.entities[1].camera2d.zoom = 1.0;
    world.entities[1].tint = [1.0, 1.0, 1.0, 1.0];
    world.entities[1].rotation = [0.0, 350.0, 0.0];
    let text = format!(
        r#"
length = 1
loop = false
properties = [
  {{ entity = "{id}", path = "camera2d.zoom", keys = [
    {{ time = 0, ease = "linear", value = {{ type = "number", value = 1 }} }}
    {{ time = 1, ease = "linear", value = {{ type = "number", value = 3 }} }}
  ] }}
  {{ entity = "{id}", path = "sprite_image.tint", keys = [
    {{ time = 0, ease = "linear", value = {{ type = "color", value = [1, 1, 1, 1] }} }}
    {{ time = 1, ease = "linear", value = {{ type = "color", value = [0, 0, 0, 0] }} }}
  ] }}
  {{ entity = "{id}", path = "sprite_image.flip_x", keys = [
    {{ time = 0, ease = "linear", value = {{ type = "bool", value = false }} }}
    {{ time = 0.5, ease = "linear", value = {{ type = "bool", value = true }} }}
  ] }}
  {{ entity = "{id}", path = "transform.rotation.y", keys = [
    {{ time = 0, ease = "linear", value = {{ type = "angle", value = 350 }} }}
    {{ time = 1, ease = "linear", value = {{ type = "angle", value = 10 }} }}
  ] }}
  {{ entity = "{id}", path = "light.intensity", keys = [
    {{ time = 0, ease = "ease-in", value = {{ type = "number", value = 0 }} }}
    {{ time = 1, ease = "ease-in", value = {{ type = "number", value = 8 }} }}
  ] }}
]
"#
    );
    let sequence = seq(&text);
    world.play();
    world.play_sequence(sequence.clone());
    for _ in 0..20 {
        world.step(1.0 / 60.0);
    }
    let entity = world.entities.iter().find(|e| e.id == id).unwrap();
    assert!(entity.camera2d.zoom > 1.4 && entity.camera2d.zoom < 1.9, "{}", entity.camera2d.zoom);
    assert!(entity.tint[0] < 0.8 && entity.tint[0] > 0.4, "{:?}", entity.tint);
    assert!(!entity.flip[0], "a boolean steps, so it is still false before 0.5 s");
    world.scrub_sequence(&sequence, 0.5);
    let entity = world.entities.iter().find(|e| e.id == id).unwrap();
    assert!((entity.camera2d.zoom - 2.0).abs() < 1e-3);
    assert!(entity.flip[0]);
    let turn = (entity.rotation[1]).rem_euclid(360.0);
    assert!(turn < 1.0 || turn > 359.0, "short way from 350 to 10 lands on 0, got {turn}");
    assert!((entity.intensity - 2.0).abs() < 1e-3, "ease-in at halfway is a quarter of the way");
}

#[test]
fn a_missing_field_warns_once_and_does_not_stop_the_other_tracks() {
    let mut world = World::demo();
    let id = world.entities[1].id.clone();
    let text = format!(
        r#"
length = 1
properties = [
  {{ entity = "{id}", path = "camera2d.zoom", keys = [
    {{ time = 0, ease = "linear", value = {{ type = "number", value = 1 }} }}
    {{ time = 1, ease = "linear", value = {{ type = "number", value = 5 }} }}
  ] }}
  {{ entity = "{id}", path = "missing.field", keys = [
    {{ time = 0, ease = "linear", value = {{ type = "number", value = 0 }} }}
    {{ time = 1, ease = "linear", value = {{ type = "number", value = 1 }} }}
  ] }}
]
"#
    );
    world.play();
    world.play_sequence(seq(&text));
    for _ in 0..10 {
        world.step(1.0 / 60.0);
    }
    let warnings: Vec<_> =
        world.play_log.iter().filter(|line| line.starts_with("warning:")).collect();
    assert_eq!(warnings.len(), 1, "{:?}", world.play_log);
    assert!(warnings[0].contains("missing.field"));
    let zoom = world.entities.iter().find(|e| e.id == id).unwrap().camera2d.zoom;
    assert!(zoom > 1.0, "{zoom}");
}

#[test]
fn ui_controls_and_script_variables_take_the_keyed_value() {
    let dir = std::env::temp_dir().join(format!("seal-prop-play-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let graph = Graph::new("vars")
        .var("boost", Value::Float(1.0))
        .node(Node::new(1, "event").with("name", SealValue::String("start".into())));
    std::fs::write(dir.join("boost.graph"), graph.to_text()).unwrap();

    let mut world = World::demo();
    let id = world.entities[1].id.clone();
    world.entities[1].script = "boost.graph".into();
    world.project_dir = Some(dir.clone());
    let mut screen = Screen::default();
    screen.nodes.push(UiNode::new("slider", "Health"));
    world.play();
    world.game_screens.shown.push(("hud".into(), Live::new(screen, Theme::default())));
    let text = format!(
        r#"
length = 1
properties = [
  {{ entity = "{id}", path = "script.boost", keys = [
    {{ time = 0, ease = "linear", value = {{ type = "number", value = 1 }} }}
    {{ time = 1, ease = "linear", value = {{ type = "number", value = 9 }} }}
  ] }}
  {{ entity = "{id}", path = "ui.Health.value", keys = [
    {{ time = 0, ease = "linear", value = {{ type = "number", value = 0 }} }}
    {{ time = 1, ease = "linear", value = {{ type = "number", value = 1 }} }}
  ] }}
]
"#
    );
    world.play_sequence(seq(&text));
    world.scrub_sequence(&world.sequences[0].0.clone(), 0.5);
    let boost = world.read_property(&id, "script.boost");
    assert_eq!(boost, Some(seal_scene::sequence::PropValue::Number(5.0)), "{boost:?}");
    let health = world.game_screens.shown[0].1.screen.nodes[0].value;
    assert!((health - 0.5).abs() < 1e-3, "{health}");
    let _ = std::fs::remove_dir_all(&dir);
}
