//! Component versions, migrations and access by name.
use seal_ecs::{text_component, Entity, SealMap, SealValue, World};

// Version 1 stored a single "size"; version 2 stores per-axis "scale".
#[derive(Clone, Debug, Default, PartialEq)]
struct Box2 {
    scale: [f32; 3],
    solid: bool,
}
fn upgrade_box(from: u32, old: SealValue) -> Result<SealValue, String> {
    match from {
        1 => {
            let size = old.get("size").cloned().ok_or("size missing")?;
            let solid = old.get("solid").cloned().unwrap_or(SealValue::Bool(true));
            Ok(SealValue::Map(SealMap::new(vec![
                ("scale".into(), SealValue::Array(vec![size.clone(), size.clone(), size])),
                ("solid".into(), solid),
            ])))
        }
        _ => Err(format!("unknown box version {from}")),
    }
}
text_component!(Box2 as "box" version 2 migrate upgrade_box { scale, solid });

#[derive(Clone, Debug, Default, PartialEq)]
struct Follow {
    target: Option<Entity>,
    distance: f32,
}
text_component!(Follow as "follow" { target, distance });

fn world() -> World {
    let mut w = World::new();
    w.register_text::<Box2>();
    w.register_text::<Follow>();
    w
}

#[test]
fn old_files_are_migrated_on_load() {
    let old = "seal_scene = 1\nentities = [{ id = 0, components = { box = { size = 2.5, solid = false } } }]\n";
    let mut w = world();
    let e = w.load_scene(old).unwrap();
    assert_eq!(w.get::<Box2>(e[0]), Some(&Box2 { scale: [2.5; 3], solid: false }));
    // Saved again, the file records version 2 and loads without migrating.
    let text = w.save_scene(None).text;
    assert!(text.contains("versions"), "{text}");
    let mut again = world();
    let e2 = again.load_scene(&text).unwrap();
    assert_eq!(again.get::<Box2>(e2[0]), w.get::<Box2>(e[0]));
}

#[test]
fn newer_files_are_refused() {
    let newer = "seal_scene = 1\nversions = { box = 3 }\nentities = [{ id = 0, components = { box = {} } }]\n";
    let mut w = world();
    let err = w.load_scene(newer).unwrap_err();
    assert!(err.contains("newer version (3)"), "{err}");
    assert!(w.is_empty());
}

#[test]
fn tools_read_and_write_components_by_name() {
    let mut w = world();
    assert_eq!(
        w.text_component_names(),
        ["box", "child_of", "follow", "prefab", "prefab_slot", "scene_id"]
    );
    assert_eq!(w.component_fields("box"), Some(&["scale", "solid"][..]));
    let leader = w.spawn(());
    let e = w.spawn((Box2 { scale: [1.0; 3], solid: true },));
    w.set_component_field(e, "box", "solid", SealValue::Bool(false)).unwrap();
    assert_eq!(w.get::<Box2>(e).unwrap(), &Box2 { scale: [1.0; 3], solid: false });
    let err = w.set_component_field(e, "box", "colour", SealValue::Bool(true)).unwrap_err();
    assert!(err.contains("has no field colour; it has scale, solid"), "{err}");
    // Entity references use live ids.
    let follow = SealValue::Map(SealMap::new(vec![
        ("target".into(), SealValue::String(format!("{leader:?}"))),
        ("distance".into(), SealValue::Number("3".into())),
    ]));
    w.set_component_value(e, "follow", &follow).unwrap();
    assert_eq!(w.get::<Follow>(e).unwrap().target, Some(leader));
    assert_eq!(w.component_value(e, "follow").unwrap().get("target"), follow.get("target"));
    assert_eq!(
        w.entity_values(e).unwrap().iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        ["box", "follow"]
    );
    assert_eq!(w.remove_component_named(e, "follow"), Ok(true));
    assert!(w.set_component_field(e, "box", "solid", SealValue::Number("1".into())).is_err());
    assert!(w.set_component_value(e, "warp", &SealValue::Null).is_err());
}
