use seal_ecs::core::{Name, Transform};
use seal_ecs::ChildOf;
use seal_world::{engine_world, Camera, EditorInfo, Light, World};

fn edited_demo() -> World {
    let mut world = World::demo();
    world.gravity = 3.5;
    world.time_scale = 0.25;
    let e = &mut world.entities[2];
    e.rotation = [10.0, 20.5, -30.0];
    e.scale = [2.0, 1.0, 0.5];
    e.intensity = 250.0;
    e.fov = 75.0;
    e.hidden = true;
    e.locked = true;
    e.roughness = 0.125;
    e.metal = 1.0;
    e.source_kind = "mesh".into();
    e.source_asset = "Content/rock.mesh".into();
    e.outliner_warning_key = "outliner-warning-motion-mode".into();
    world
}

fn rows(world: &World) -> Vec<String> {
    world.entities.iter().map(|e| format!("{e:?}")).collect()
}

#[test]
fn scene_files_round_trip_through_the_engine_format() {
    let world = edited_demo();
    let text = world.to_text();
    assert!(text.contains("seal_scene"), "{text}");
    let back = World::from_text(&text).unwrap();
    assert_eq!(rows(&back), rows(&world));
    assert_eq!(back.gravity, 3.5);
    assert_eq!(back.time_scale, 0.25);
    // Saving again gives the same bytes, so saves diff cleanly.
    assert_eq!(back.to_text(), text);
}

#[test]
fn old_scene_files_still_load() {
    let world = edited_demo();
    let back = World::from_text(&world.to_legacy_text()).unwrap();
    assert_eq!(back.entities.len(), world.entities.len());
    assert_eq!(back.to_text(), World::from_text(&world.to_text()).unwrap().to_text());
}

#[test]
fn the_engine_reads_editor_scenes() {
    let world = edited_demo();
    let mut engine = engine_world();
    let order = engine.load_scene(&world.to_text()).unwrap();
    assert_eq!(order.len(), world.entities.len());
    for (e, &id) in world.entities.iter().zip(&order) {
        assert_eq!(engine.get::<Name>(id).unwrap().0, e.name);
        assert_eq!(engine.get::<Transform>(id).unwrap().translation, e.translation);
        assert_eq!(engine.get::<EditorInfo>(id).unwrap().key, e.id);
        let parent = engine.get::<ChildOf>(id).map(|c| c.parent());
        let expected = e
            .parent
            .as_ref()
            .map(|p| order[world.entities.iter().position(|x| &x.id == p).unwrap()]);
        assert_eq!(parent, expected, "{}", e.id);
    }
    let edited = order[2];
    assert_eq!(engine.get::<Light>(edited).unwrap().intensity, 250.0);
    assert_eq!(engine.get::<Camera>(edited).unwrap().fov, 75.0);
    // Defaults are not written.
    assert!(engine.get::<Light>(order[0]).is_none());
    assert!(engine.get::<Camera>(order[0]).is_none());
}

#[test]
fn engine_edits_come_back_into_the_editor() {
    let world = World::demo();
    let mut engine = engine_world();
    let order = engine.load_scene(&world.to_text()).unwrap();
    engine.get_mut::<Name>(order[1]).unwrap().0 = "Renamed".into();
    let back = World::from_ecs(&engine, &order).unwrap();
    assert_eq!(back.entities[1].name, "Renamed");
    assert_eq!(back.entities[2].parent, world.entities[2].parent);
}

#[test]
fn scene_settings_live_in_the_engine_resources_block() {
    let world = edited_demo();
    let text = world.to_text();
    assert!(text.contains("resources"), "{text}");
    let mut engine = engine_world();
    engine.load_scene(&text).unwrap();
    let settings = engine.resource::<seal_ecs::core::SceneSettings>().unwrap();
    assert_eq!((settings.gravity, settings.time_scale), (3.5, 0.25));
    // Saving from the engine keeps them, so other tools don't drop them.
    let resaved = engine.save_scene(None).text;
    let back = World::from_text(&resaved).unwrap();
    assert_eq!((back.gravity, back.time_scale), (3.5, 0.25));
    // Default settings are not written.
    assert!(!World::demo().to_text().contains("resources"));
}

#[test]
fn a_git_merge_keeps_settings_from_either_side() {
    let base = World::demo();
    let mut ours = World::demo();
    ours.entities[1].name = "Ours".into();
    let mut theirs = World::demo();
    theirs.gravity = 1.5;
    let merged =
        seal_ecs::merge_scenes(&base.to_text(), &ours.to_text(), &theirs.to_text()).unwrap();
    assert!(merged.conflicts.is_empty(), "{:?}", merged.conflicts);
    let back = World::from_text(&merged.text).unwrap();
    assert_eq!(back.entities[1].name, "Ours");
    assert_eq!(back.gravity, 1.5);
}
