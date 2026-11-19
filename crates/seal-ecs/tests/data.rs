//! Hierarchy, snapshots, undo and scene files.
use proptest::prelude::*;
use seal_ecs::{text_component, ChildOf, Entity, HierarchyError, History, World};

#[derive(Clone, Debug, Default, PartialEq)]
struct Transform {
    translation: [f32; 3],
    rotation: [f32; 3],
    scale: [f32; 3],
}
text_component!(Transform as "transform" { translation, rotation, scale });

#[derive(Clone, Debug, Default, PartialEq)]
struct Name(String);
text_component!(Name as "name" newtype);

#[derive(Clone, Debug, Default, PartialEq)]
struct Light {
    intensity: f32,
    color: [f32; 3],
    target: Option<Entity>,
}
text_component!(Light as "light" { intensity, color, target });

#[derive(Clone, Debug, Default, PartialEq)]
struct Player;
text_component!(Player as "player" marker);

/// Not registered for text or cloning.
struct Opaque;

fn world() -> World {
    let mut world = World::new();
    world.register_text::<Transform>();
    world.register_text::<Name>();
    world.register_text::<Light>();
    world.register_text::<Player>();
    world
}

fn t(x: f32) -> Transform {
    Transform { translation: [x, 0.5, -x], rotation: [0.0, 90.0, 0.0], scale: [1.0; 3] }
}

#[test]
fn hierarchy_links_stay_in_step_and_refuse_cycles() {
    let mut w = world();
    let root = w.spawn((Name("root".into()),));
    let a = w.spawn((Name("a".into()),));
    let b = w.spawn((Name("b".into()),));
    w.set_parent(a, Some(root)).unwrap();
    w.set_parent(b, Some(a)).unwrap();
    assert_eq!(w.children(root), &[a]);
    assert_eq!(w.ancestors(b).collect::<Vec<_>>(), vec![a, root]);
    assert_eq!(w.set_parent(root, Some(b)), Err(HierarchyError::WouldCycle));
    assert_eq!(w.set_parent(a, Some(a)), Err(HierarchyError::WouldCycle));
    w.set_parent(b, Some(root)).unwrap();
    assert_eq!(w.children(root), &[a, b]);
    assert!(w.children(a).is_empty());
    // Despawning a parent turns its children into roots.
    w.despawn(root);
    assert_eq!(w.parent(a), None);
    w.check_hierarchy().unwrap();
    w.set_parent(b, Some(a)).unwrap();
    assert_eq!(w.despawn_recursive(a), 2);
    assert!(w.is_empty());
    w.check_integrity().unwrap();
}

#[test]
fn restore_puts_the_world_back_and_new_ids_go_stale() {
    let mut w = world();
    let keep = w.spawn((t(1.0), Name("keep".into())));
    let gone = w.spawn((t(2.0),));
    let snap = w.snapshot().unwrap();
    w.get_mut::<Transform>(keep).unwrap().translation[0] = 99.0;
    w.despawn(gone);
    let reused = w.spawn((Name("new".into()),)); // reuses gone's slot
    let extra = w.spawn((Player,));
    w.restore(&snap);
    assert_eq!(w.get::<Transform>(keep), Some(&t(1.0)));
    assert_eq!(w.get::<Transform>(gone), Some(&t(2.0)));
    assert!(!w.contains(reused) && !w.contains(extra));
    let fresh = w.spawn((Player,));
    assert!(fresh != reused && fresh != extra && fresh != keep && fresh != gone);
    w.check_integrity().unwrap();
}

#[test]
fn snapshots_name_the_types_they_cannot_copy() {
    let mut w = world();
    w.spawn((Opaque,));
    let err = w.snapshot().err().unwrap();
    assert!(err.unregistered[0].ends_with("Opaque"));
}

#[test]
fn failed_transactions_roll_back_and_successful_ones_stay() {
    let mut w = world();
    let e = w.spawn((t(0.0),));
    let failed: Result<(), &str> = w
        .transaction(|w| {
            w.insert(e, Name("half done".into()));
            w.spawn((Player,));
            Err("tool failed")
        })
        .unwrap();
    assert!(failed.is_err());
    assert!(!w.has::<Name>(e));
    assert_eq!(w.len(), 1);
    w.transaction(|w| -> Result<(), ()> {
        w.insert(e, Name("done".into()));
        Ok(())
    })
    .unwrap()
    .unwrap();
    assert_eq!(w.get::<Name>(e), Some(&Name("done".into())));
}

#[test]
fn undo_and_redo_walk_through_edits() {
    let mut w = world();
    let mut history = History::with_limit(100);
    let e = w.spawn((t(0.0),));
    for step in 1..=5 {
        history.record(&w).unwrap();
        w.get_mut::<Transform>(e).unwrap().translation[0] = step as f32;
    }
    assert!(history.undo(&mut w) && history.undo(&mut w));
    assert_eq!(w.get::<Transform>(e).unwrap().translation[0], 3.0);
    assert!(history.redo(&mut w));
    assert_eq!(w.get::<Transform>(e).unwrap().translation[0], 4.0);
    history.record(&w).unwrap();
    assert!(!history.can_redo(), "a new edit clears redo");
}

#[test]
fn scenes_round_trip_with_references_and_hierarchy() {
    let mut w = world();
    let lamp = w.spawn((Name("Lamp".into()), t(1.25)));
    let bulb = w.spawn((
        Name("Bulb".into()),
        Light { intensity: 800.0, color: [1.0, 0.9, 0.7], target: Some(lamp) },
    ));
    let hero = w.spawn((Name("Hero \"quoted\"".into()), Player, t(-3.0)));
    w.set_parent(bulb, Some(lamp)).unwrap();
    let saved = w.save_scene(None);
    assert!(saved.skipped.is_empty(), "{:?}", saved.skipped);
    assert!(saved.text.contains("child_of"));

    // Load twice into another world that already has entities.
    let mut other = world();
    other.spawn((Name("existing".into()),));
    let first = other.load_scene(&saved.text).unwrap();
    let second = other.load_scene(&saved.text).unwrap();
    assert_eq!(other.len(), 7);
    let (lamp2, bulb2) = (second[0], second[1]);
    assert_eq!(other.parent(bulb2), Some(lamp2));
    assert_eq!(other.get::<Light>(bulb2).unwrap().target, Some(lamp2));
    assert_ne!(lamp2, first[0]);
    assert_eq!(other.get::<Name>(second[2]), w.get::<Name>(hero));
    other.check_hierarchy().unwrap();

    // Saving the loaded copy gives the same text.
    assert_eq!(other.save_scene(Some(&second)).text, saved.text);
}

#[test]
fn a_bad_scene_changes_nothing() {
    let mut w = world();
    w.spawn((Name("only".into()),));
    let bad_type = "seal_scene = 1\nentities = [{ id = 0, components = { name = \"a\" } }, { id = 1, components = { warp_drive = 1 } }]\n";
    let e = w.load_scene(bad_type).unwrap_err();
    assert!(e.contains("warp_drive"), "{e}");
    let bad_value = "seal_scene = 1\nentities = [{ id = 0, components = { name = \"a\" } }, { id = 1, components = { transform = { scale = [1, 2] } } }]\n";
    assert!(w.load_scene(bad_value).unwrap_err().contains("scale"));
    let newer = "seal_scene = 9\nentities = []\n";
    assert!(w.load_scene(newer).unwrap_err().contains("newer"));
    assert_eq!(w.len(), 1);
    w.check_integrity().unwrap();
}

#[test]
fn ten_thousand_entities_load_edit_undo_and_save() {
    let mut w = world();
    let mut previous = None;
    for i in 0..10_000 {
        let e = w.spawn((Name(format!("e{i}")), t(i as f32 * 0.1)));
        if i % 10 != 0 {
            w.set_parent(e, previous).unwrap();
        }
        if i % 10 == 0 {
            previous = Some(e);
        }
    }
    let text = w.save_scene(None).text;
    let mut loaded = world();
    let start = std::time::Instant::now();
    let ids = loaded.load_scene(&text).unwrap();
    let load_ms = start.elapsed().as_millis();
    assert_eq!(ids.len(), 10_000);
    let mut history = History::default();
    history.record(&loaded).unwrap();
    for &e in &ids[..500] {
        loaded.get_mut::<Transform>(e).unwrap().scale = [2.0; 3];
    }
    history.undo(&mut loaded);
    assert_eq!(loaded.save_scene(None).text, text);
    loaded.check_hierarchy().unwrap();
    loaded.check_integrity().unwrap();
    eprintln!("10k-entity scene: {} KiB, loaded in {load_ms} ms", text.len() / 1024);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    /// Any values, including awkward floats and strings, survive save then load.
    #[test]
    fn components_survive_text_round_trips(
        values in prop::collection::vec((any::<f32>(), ".{0,12}", any::<bool>()), 1..30),
        parents in prop::collection::vec(any::<prop::sample::Index>(), 1..30),
    ) {
        let mut w = world();
        let mut ids = Vec::new();
        for (x, name, is_player) in &values {
            let x = if x.is_finite() { *x } else { 0.0 };
            let e = w.spawn((Name(name.clone()), t(x)));
            if *is_player { w.insert(e, Player); }
            ids.push(e);
        }
        for (child, pick) in ids.iter().zip(&parents) {
            let _ = w.set_parent(*child, Some(*pick.get(&ids)));
        }
        let text = w.save_scene(None).text;
        let mut other = world();
        let loaded = other.load_scene(&text).unwrap();
        prop_assert_eq!(&other.save_scene(None).text, &text);
        for (a, b) in ids.iter().zip(&loaded) {
            prop_assert_eq!(w.get::<Transform>(*a), other.get::<Transform>(*b));
            prop_assert_eq!(w.get::<Name>(*a), other.get::<Name>(*b));
            prop_assert_eq!(w.has::<Player>(*a), other.has::<Player>(*b));
            let parent = w.parent(*a).map(|p| ids.iter().position(|x| *x == p).unwrap());
            let parent2 = other.parent(*b).map(|p| loaded.iter().position(|x| *x == p).unwrap());
            prop_assert_eq!(parent, parent2);
        }
        prop_assert!(other.get::<ChildOf>(loaded[0]).is_none() || other.check_hierarchy().is_ok());
    }
}

fn scene_with_ids(edit: impl FnOnce(&mut World, &[Entity])) -> String {
    let mut w = world();
    let ids: Vec<Entity> = (0..4).map(|i| w.spawn((Name(format!("e{i}")), t(i as f32)))).collect();
    w.ensure_scene_ids();
    w.set_parent(ids[1], Some(ids[0])).unwrap();
    edit(&mut w, &ids);
    w.save_scene(None).text
}

#[test]
fn scene_ids_keep_files_stable_between_sessions() {
    let base = scene_with_ids(|_, _| {});
    let mut w = world();
    w.spawn((Player,)); // shifts every entity index
    let loaded = w.load_scene(&base).unwrap();
    assert_eq!(w.save_scene(Some(&loaded)).text, base);
}

#[test]
fn edits_to_different_things_merge_cleanly() {
    let base = scene_with_ids(|_, _| {});
    let ours = scene_with_ids(|w, ids| {
        w.get_mut::<Transform>(ids[2]).unwrap().scale = [2.0; 3];
        w.get_mut::<Transform>(ids[3]).unwrap().translation[0] = 7.0;
    });
    let theirs = scene_with_ids(|w, ids| {
        w.get_mut::<Name>(ids[2]).unwrap().0 = "renamed".into();
        w.get_mut::<Transform>(ids[3]).unwrap().rotation[1] = 45.0;
        w.despawn(ids[1]);
        let lamp = w.spawn((Name("lamp".into()),));
        w.ensure_scene_ids();
        w.set_parent(lamp, Some(ids[0])).unwrap();
    });
    let merged = seal_ecs::merge_scenes(&base, &ours, &theirs).unwrap();
    assert!(merged.conflicts.is_empty(), "{:?}", merged.conflicts);
    let mut w = world();
    let e = w.load_scene(&merged.text).unwrap();
    assert_eq!(e.len(), 4);
    // Entity 1 was deleted, so the loaded order is ids 0, 2, 3 and the new lamp.
    assert_eq!(w.get::<Name>(e[1]).unwrap().0, "renamed");
    assert_eq!(w.get::<Transform>(e[1]).unwrap().scale, [2.0; 3]);
    let t3 = w.get::<Transform>(e[2]).unwrap();
    assert_eq!((t3.translation[0], t3.rotation[1]), (7.0, 45.0));
    assert_eq!(w.parent(e[3]), Some(e[0]), "new lamp kept its parent");
    w.check_hierarchy().unwrap();
}

#[test]
fn clashing_edits_are_reported() {
    let base = scene_with_ids(|_, _| {});
    let ours = scene_with_ids(|w, ids| {
        w.get_mut::<Transform>(ids[2]).unwrap().scale = [2.0; 3];
        w.get_mut::<Name>(ids[3]).unwrap().0 = "ours".into();
    });
    let theirs = scene_with_ids(|w, ids| {
        w.get_mut::<Transform>(ids[2]).unwrap().scale = [3.0; 3];
        w.despawn(ids[3]);
    });
    let merged = seal_ecs::merge_scenes(&base, &ours, &theirs).unwrap();
    assert_eq!(merged.conflicts.len(), 2, "{:?}", merged.conflicts);
    assert!(merged.conflicts[0].contains("entity 2, transform.scale"));
    assert!(merged.conflicts[1].contains("entity 3"));
}
