//! Prefabs, the class vocabulary view and scene-graph mode over one core.
use proptest::prelude::*;
use seal_ecs::{
    core::{Name, Transform},
    text_component, to_classic, to_scene_graph, Prefab, PrefabInstance, PrefabLibrary, PrefabSlot,
    SealValue, Vocabulary, World,
};

#[derive(Clone, Debug, Default, PartialEq)]
struct Health {
    max: f32,
    current: f32,
}
text_component!(Health as "health" { max, current });

#[derive(Clone, Debug, Default, PartialEq)]
struct Lens {
    fov: f32,
}
text_component!(Lens as "lens" { fov });

fn world() -> World {
    let mut w = World::new();
    w.register_core();
    w.register_text::<Health>();
    w.register_text::<Lens>();
    w
}

const ENTITY: &str = "seal_prefab = 1\nprefab = \"Entity\"\ncomponents = { transform = { translation = [0, 0, 0] } }\n";
const UNIT: &str = "# A thing that can be possessed.\nseal_class = 1\nclass = \"Unit\"\nparent_class = \"Entity\"\ndefaults = { health = { max = 100, current = 100 } }\n";
const CAMERA: &str = "seal_prefab = 1\nprefab = \"Camera\"\ncomponents = { lens = { fov = 90 } }\n";
const CHARACTER: &str = "seal_prefab = 1\nprefab = \"Character\"\nbase = \"Unit\"\ncomponents = { health = { max = 150 }, transform = { scale = [2, 2, 2] } }\nchildren = [{ slot = \"camera\", prefab = \"Camera\", components = { lens = { fov = 70 } } }, { slot = \"mesh\", components = { name = \"Body\" } }]\n";

fn library() -> PrefabLibrary {
    let mut lib = PrefabLibrary::new();
    for text in [ENTITY, UNIT, CAMERA, CHARACTER] {
        lib.insert(Prefab::from_text(text).unwrap());
    }
    lib
}

fn num(v: &SealValue) -> f64 {
    match v {
        SealValue::Number(n) => n.parse().unwrap(),
        other => panic!("not a number: {other:?}"),
    }
}

#[test]
fn class_chains_resolve_field_by_field() {
    let lib = library();
    assert_eq!(lib.class_chain("Character").unwrap(), ["Entity", "Unit", "Character"]);
    assert!(lib.is_a("Character", "Entity") && !lib.is_a("Unit", "Character"));
    let r = lib.resolve("Character").unwrap();
    let health = &r.components.iter().find(|(n, _)| n == "health").unwrap().1;
    // max overridden by the child class, current inherited from the parent class.
    assert_eq!(num(health.get("max").unwrap()), 150.0);
    assert_eq!(num(health.get("current").unwrap()), 100.0);
    let transform = &r.components.iter().find(|(n, _)| n == "transform").unwrap().1;
    assert!(transform.get("translation").is_some() && transform.get("scale").is_some());
    assert_eq!(r.children.len(), 2);
    assert_eq!(r.children[0].1.chain, ["Camera"]);
}

#[test]
fn instances_spawn_with_attached_parts() {
    let lib = library();
    let mut w = world();
    let hero = w.instantiate(&lib, "Character").unwrap();
    assert_eq!(w.get::<PrefabInstance>(hero).unwrap().0, "Character");
    assert_eq!(w.get::<Health>(hero).unwrap(), &Health { max: 150.0, current: 100.0 });
    assert_eq!(w.get::<Transform>(hero).unwrap().scale, [2.0; 3]);
    let kids = w.children(hero).to_vec();
    assert_eq!(kids.len(), 2);
    assert_eq!(w.get::<PrefabSlot>(kids[0]).unwrap().0, "camera");
    assert_eq!(w.get::<PrefabInstance>(kids[0]).unwrap().0, "Camera");
    assert_eq!(w.get::<Lens>(kids[0]).unwrap().fov, 70.0);
    assert_eq!(w.get::<Name>(kids[1]).unwrap().0, "Body");
    assert!(w.get::<PrefabInstance>(kids[1]).is_none());
}

#[test]
fn overrides_are_only_what_changed() {
    let lib = library();
    let mut w = world();
    let hero = w.instantiate(&lib, "Character").unwrap();
    assert_eq!(w.instance_overrides(&lib, hero).unwrap().unwrap().as_map().unwrap().len(), 0);
    w.get_mut::<Health>(hero).unwrap().current = 40.0;
    w.insert(hero, Name("Hero".into()));
    let o = w.instance_overrides(&lib, hero).unwrap().unwrap();
    let keys: Vec<&str> = o.as_map().unwrap().iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["health", "name"]);
    assert_eq!(o.get("health").unwrap().as_map().unwrap().len(), 1, "only current changed: {o:?}");
    // The camera slot compares against the slot's own override (fov 70), not the bare Camera.
    let camera = w.children(hero)[0];
    assert_eq!(w.instance_overrides(&lib, camera).unwrap().unwrap().as_map().unwrap().len(), 0);
    w.remove::<Lens>(camera);
    assert_eq!(
        w.instance_overrides(&lib, camera).unwrap().unwrap().get("lens"),
        Some(&SealValue::Null)
    );
    w.revert_to_prefab(&lib, hero, "health").unwrap();
    w.revert_to_prefab(&lib, camera, "lens").unwrap();
    assert_eq!(w.get::<Health>(hero).unwrap().current, 100.0);
    assert_eq!(w.get::<Lens>(camera).unwrap().fov, 70.0);
}

#[test]
fn compact_scenes_round_trip_and_pick_up_prefab_edits() {
    let mut lib = library();
    let mut w = world();
    let hero = w.instantiate(&lib, "Character").unwrap();
    w.get_mut::<Health>(hero).unwrap().current = 40.0;
    let loose = w.spawn((Name("Loose".into()),));
    w.set_parent(hero, Some(loose)).unwrap(); // a prefab instance inside a plain entity
    w.ensure_scene_ids();
    let compact = w.save_scene_compact(&lib, Vocabulary::SceneGraph).unwrap();
    assert!(!compact.contains("lens"), "unchanged slot values are not stored:\n{compact}");
    assert!(!compact.contains("max"), "unchanged fields are not stored:\n{compact}");

    let mut back = world();
    back.load_scene_with(&lib, &compact).unwrap();
    assert_eq!(back.save_scene(None).text, w.save_scene(None).text);

    // Edit the prefabs: a new default and a new attached part.
    let mut character = Prefab::from_text(CHARACTER).unwrap();
    character.components.push(("name".into(), SealValue::String("Unnamed".into())));
    character.children.push(seal_ecs::PrefabChild {
        slot: "light".into(),
        prefab: None,
        components: vec![],
    });
    lib.insert(character);
    let mut edited = world();
    edited.load_scene_with(&lib, &compact).unwrap();
    let hero2 = edited
        .entities()
        .into_iter()
        .find(|e| edited.get::<PrefabInstance>(*e).is_some_and(|p| p.0 == "Character"))
        .unwrap();
    assert_eq!(edited.get::<Name>(hero2).unwrap().0, "Unnamed");
    assert_eq!(edited.get::<Health>(hero2).unwrap().current, 40.0, "the override survives");
    assert_eq!(edited.children(hero2).len(), 3, "the new slot was spawned");
}

#[test]
fn both_vocabularies_are_the_same_bytes_underneath() {
    // A class file as Seal writes it, converted to scene-graph words and back,
    // is unchanged (comment included). Hand-written spacing is normalised once.
    let sg = to_scene_graph(UNIT).unwrap();
    assert!(
        sg.starts_with("# A thing")
            && sg.contains("prefab = \"Unit\"")
            && sg.contains("base = \"Entity\"")
    );
    let unit = to_classic(&sg).unwrap();
    assert_eq!(to_classic(&to_scene_graph(&unit).unwrap()).unwrap(), unit);
    assert_eq!(to_scene_graph(&unit).unwrap(), sg);
    assert!(to_classic(UNIT).is_err(), "already classic");
    let character = Prefab::from_text(CHARACTER).unwrap();
    let classic = character.to_text(Vocabulary::Classic);
    assert!(classic.contains("attached = ") && classic.contains("class = \"Camera\""));
    assert_eq!(Prefab::from_text(&classic).unwrap(), character);
    // Scenes: a classic save loads into the same world.
    let lib = library();
    let mut w = world();
    w.instantiate(&lib, "Character").unwrap();
    w.ensure_scene_ids();
    let classic_scene = w.save_scene_compact(&lib, Vocabulary::Classic).unwrap();
    assert!(classic_scene.contains("instances = ") && classic_scene.contains("slot = \"camera\""));
    assert_eq!(
        to_scene_graph(&classic_scene).unwrap(),
        w.save_scene_compact(&lib, Vocabulary::SceneGraph).unwrap()
    );
    let mut back = world();
    back.load_scene_with(&lib, &classic_scene).unwrap();
    assert_eq!(back.save_scene(None).text, w.save_scene(None).text);
    // A full save in classic words also loads without the library.
    let mut plain = world();
    plain.load_scene(&to_classic(&w.save_scene(None).text).unwrap()).unwrap();
    assert_eq!(plain.save_scene(None).text, w.save_scene(None).text);
}

#[test]
fn bad_prefabs_are_refused_cleanly() {
    let mut lib = library();
    lib.insert(Prefab::from_text("seal_prefab = 1\nprefab = \"A\"\nbase = \"B\"\n").unwrap());
    lib.insert(Prefab::from_text("seal_prefab = 1\nprefab = \"B\"\nbase = \"A\"\n").unwrap());
    assert!(lib.resolve("A").unwrap_err().contains("loops"));
    lib.insert(Prefab::from_text("seal_prefab = 1\nprefab = \"Nest\"\nchildren = [{ slot = \"me\", prefab = \"Nest\" }]\n").unwrap());
    assert!(lib.resolve("Nest").unwrap_err().contains("itself"));
    lib.insert(Prefab::from_text("seal_prefab = 1\nprefab = \"Bad\"\nchildren = [{ slot = \"x\", components = { nope = 1 } }]\n").unwrap());
    let mut w = world();
    assert!(w.instantiate(&lib, "Bad").is_err());
    assert_eq!(w.entities().len(), 0, "a failed instantiate leaves nothing behind");
}

#[test]
#[should_panic(expected = "reserved")]
fn classic_words_cannot_be_component_names() {
    #[derive(Clone, Default)]
    struct Class(u32);
    text_component!(Class as "class" newtype);
    World::new().register_text::<Class>();
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn any_scene_round_trips_between_vocabularies(
        picks in proptest::collection::vec((0usize..4, 0u8..4, -50i32..50, proptest::option::of(0usize..8)), 0..12)
    ) {
        let lib = library();
        let names = ["Entity", "Unit", "Camera", "Character"];
        let mut w = world();
        let mut made = Vec::new();
        for (kind, edit, value, parent) in picks {
            let e = w.instantiate(&lib, names[kind]).unwrap();
            match edit {
                0 => { w.insert(e, Name(format!("n{value}"))); }
                1 => { if let Some(t) = w.get_mut::<Transform>(e) { t.translation[1] = value as f32; } }
                2 => { w.insert(e, Health { max: value as f32, current: 1.0 }); }
                _ => { w.remove::<Transform>(e); }
            }
            if let Some(p) = parent.and_then(|p| made.get(p).copied()) { w.set_parent(e, Some(p)).unwrap(); }
            made.push(e);
        }
        w.ensure_scene_ids();
        let sg = w.save_scene_compact(&lib, Vocabulary::SceneGraph).unwrap();
        let classic = w.save_scene_compact(&lib, Vocabulary::Classic).unwrap();
        prop_assert_eq!(&to_classic(&sg).unwrap(), &classic);
        prop_assert_eq!(&to_scene_graph(&classic).unwrap(), &sg);
        let mut back = world();
        back.load_scene_with(&lib, &classic).unwrap();
        prop_assert_eq!(back.save_scene(None).text, w.save_scene(None).text);
    }
}
