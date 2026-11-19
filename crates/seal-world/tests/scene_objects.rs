//! Merge, one object per file, nested prefabs, and post-process volumes.

use std::path::Path;

use seal_world::object_files;
use seal_world::object_merge;
use seal_world::post_volume;
use seal_world::prefab_nest;
use seal_world::World;

fn empty(name: &str) -> World {
    World::from_text(&World::empty_scene_text(name)).unwrap()
}

#[test]
fn merging_sprites_keeps_the_pixels_in_one_batch_and_undoes() {
    let mut world = empty("Merge");
    let a = world.add("empty");
    {
        let entity = world.entity_mut(&a).unwrap();
        entity.sprite = "sprites/coin.png".into();
        entity.translation = [1.0, 0.5, 0.0];
        entity.tint = [1.0, 0.2, 0.2, 1.0];
        entity.scale = [1.0, 1.0, 1.0];
    }
    let b = world.add("empty");
    {
        let entity = world.entity_mut(&b).unwrap();
        entity.sprite = "sprites/coin.png".into();
        entity.translation = [3.0, 1.0, 0.0];
        entity.scale = [1.0, 1.0, 1.0];
    }
    let before = object_merge::collect_marks(&world.entities, &[a.clone(), b.clone()]);
    let before_pixels = object_merge::raster(&before.0, &before.1);
    let previous = world.to_text();
    let id = world.merge_objects(&[a.clone(), b.clone()]).unwrap();
    let entity = world.entity(&id).unwrap();
    assert_eq!(entity.merged.kind, "sprites");
    assert_eq!(object_merge::sprite_batches(entity), 1);
    let after = object_merge::collect_marks(&world.entities, std::slice::from_ref(&id));
    assert_eq!(object_merge::raster(&after.0, &after.1), before_pixels);
    let restored = World::from_text(&previous).unwrap();
    assert!(restored.entity(&id).is_none());
    assert_eq!(restored.entities.iter().filter(|e| e.sprite == "sprites/coin.png").count(), 2);
}

#[test]
fn merging_shapes_keeps_the_same_triangles() {
    let mut world = empty("Mesh");
    let a = world.add("cube");
    world.entity_mut(&a).unwrap().translation = [-1.0, 0.0, 0.0];
    world.entity_mut(&a).unwrap().color = [0.2, 0.4, 0.8];
    let b = world.add("sphere");
    world.entity_mut(&b).unwrap().translation = [2.0, 1.0, 0.0];
    world.entity_mut(&b).unwrap().color = [0.8, 0.3, 0.1];
    let before = object_merge::collect_marks(&world.entities, &[a.clone(), b.clone()]);
    let before_pixels = object_merge::raster(&before.0, &before.1);
    let id = world.merge_objects(&[a, b]).unwrap();
    assert_eq!(world.entity(&id).unwrap().merged.kind, "mesh");
    let after = object_merge::collect_marks(&world.entities, &[id]);
    assert_eq!(object_merge::raster(&after.0, &after.1), before_pixels);
    assert!(world.merge_objects(&["missing".into(), "also".into()]).is_err());
}

#[test]
fn one_object_per_file_round_trips_and_rejects_a_damaged_part() {
    let dir = std::env::temp_dir().join(format!("seal-objects-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("scenes").join("level.seal");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut world = empty("Level");
    let cube = world.add("cube");
    world.entity_mut(&cube).unwrap().translation[0] = 4.0;
    world.entity_mut(&cube).unwrap().name = "Crate".into();
    let sprite = world.add("empty");
    world.entity_mut(&sprite).unwrap().sprite = "sprites/coin.png".into();
    world.entity_mut(&sprite).unwrap().translation = [1.0, 2.0, 0.0];
    world.objects_per_file = true;
    let text = world.to_text();
    object_files::write(&path, &text).unwrap();
    let folder = object_files::folder_for(&path);
    assert!(folder.join(".seal-objects").is_file());
    assert!(folder
        .read_dir()
        .unwrap()
        .flatten()
        .any(|e| e.path().extension().is_some_and(|x| x == "seal")));
    let joined = object_files::read(&path).unwrap();
    let canon = |text: &str| World::from_text(text).unwrap().to_text();
    assert_eq!(canon(&text), canon(&joined));
    let back = World::from_text(&joined).unwrap();
    assert!(back.objects_per_file);
    assert!(back
        .entities
        .iter()
        .any(|e| e.name == "Crate" && (e.translation[0] - 4.0).abs() < 1e-4));
    assert!(back.entities.iter().any(|e| e.sprite == "sprites/coin.png"));

    world.objects_per_file = false;
    let single = world.to_text();
    object_files::write(&path, &single).unwrap();
    assert!(!folder.join(".seal-objects").exists());
    let loaded = object_files::read(&path).unwrap();
    assert!(loaded.contains("seal_scene"));
    assert!(!object_files::is_index(&loaded));

    world.objects_per_file = true;
    object_files::write(&path, &world.to_text()).unwrap();
    std::fs::write(folder.join("root.seal"), "this file is damaged").unwrap();
    let err = object_files::read(&path).unwrap_err();
    assert!(err.contains("damaged"), "{err}");
}

#[test]
fn nested_prefab_edits_spread_and_overrides_stay() {
    let dir = std::env::temp_dir().join(format!("seal-nested-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let mut inner = empty("Gem");
    let gem = inner.add("cube");
    {
        let entity = inner.entity_mut(&gem).unwrap();
        entity.name = "Gem".into();
        entity.tag = "wood".into();
        entity.metal = 0.2;
    }
    let inner_text = inner.prefab_text(&gem).unwrap();
    std::fs::write(dir.join("Gem.prefab"), &inner_text).unwrap();

    let mut outer = empty("Lamp");
    outer.project_dir = Some(dir.clone());
    let placed = outer.add_prefab_text(&inner_text, Some("root")).unwrap();
    {
        let entity = outer.entity_mut(&placed).unwrap();
        entity.source_kind = "prefab".into();
        entity.source_asset = "Gem".into();
    }
    let lamp = outer.add("empty");
    outer.entity_mut(&lamp).unwrap().name = "Lamp".into();
    outer.entity_mut(&placed).unwrap().parent = Some(lamp.clone());
    let outer_text = outer.prefab_text(&lamp).unwrap();
    assert!(outer_text.contains("Gem"), "the outer prefab keeps the inner instance");
    std::fs::write(dir.join("Lamp.prefab"), &outer_text).unwrap();

    let mut scene = empty("Level");
    scene.project_dir = Some(dir.clone());
    let place = |scene: &mut World| {
        let id = scene.add_prefab_text(&outer_text, None).unwrap();
        let entity = scene.entity_mut(&id).unwrap();
        entity.source_kind = "prefab".into();
        entity.source_asset = "Lamp".into();
        id
    };
    let lamp_a = place(&mut scene);
    let _lamp_b = place(&mut scene);
    let gems: Vec<String> =
        scene.entities.iter().filter(|e| e.source_asset == "Gem").map(|e| e.id.clone()).collect();
    assert_eq!(gems.len(), 2, "each lamp holds a gem");
    {
        let entity = scene.entity_mut(&gems[0]).unwrap();
        entity.tag = "painted".into();
        entity.overrides = "role".into();
    }

    let mut edited = empty("Gem");
    let id = edited.add("cube");
    {
        let entity = edited.entity_mut(&id).unwrap();
        entity.name = "Gem".into();
        entity.tag = "stone".into();
        entity.metal = 0.8;
    }
    std::fs::write(dir.join("Gem.prefab"), edited.prefab_text(&id).unwrap()).unwrap();
    refresh_prefabs(&mut scene, &dir);

    let painted = scene.entities.iter().find(|e| e.overrides.contains("role")).unwrap();
    assert_eq!(painted.tag, "painted");
    assert!((painted.metal - 0.8).abs() < 1e-3, "metal follows the inner prefab");
    let other = scene
        .entities
        .iter()
        .find(|e| e.source_asset == "Gem" && !e.overrides.contains("role"))
        .unwrap();
    assert_eq!(other.tag, "stone");
    assert!((other.metal - 0.8).abs() < 1e-3);
    let lines = prefab_nest::override_lines(&scene.entities, &lamp_a);
    assert!(lines.iter().any(|line| line.contains("role")), "{lines:?}");
}

fn refresh_prefabs(world: &mut World, dir: &Path) {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "prefab"))
        .collect();
    files.sort();
    for file in files {
        let stem = file.file_stem().unwrap().to_string_lossy().to_string();
        let text = std::fs::read_to_string(&file).unwrap();
        let expanded =
            seal_world::prefab_variant::read_prefab_for_world(Some(dir), &text, false).unwrap();
        world.refresh_prefab_instances(&stem, &expanded).unwrap();
    }
    prefab_nest::refresh_nested(world, dir);
}

#[test]
fn post_volume_weight_covers_inside_outside_the_blend_band_and_priority() {
    let mut world = empty("Grade");
    let low = world.add("post_process");
    {
        let entity = world.entity_mut(&low).unwrap();
        entity.translation = [0.0, 0.0, 0.0];
        entity.post.glow_intensity = 1.0;
        entity.post.blend = 1.0;
        entity.post.priority = 0;
        entity.post.tint = [1.0, 0.0, 0.0];
    }
    assert!((post_volume::weight(world.entity(&low).unwrap(), [0.0, 0.0]) - 1.0).abs() < 1e-4);
    assert_eq!(post_volume::weight(world.entity(&low).unwrap(), [20.0, 0.0]), 0.0);
    let edge = post_volume::weight(world.entity(&low).unwrap(), [2.7, 0.0]);
    assert!(edge > 0.05 && edge < 0.95, "blend band {edge}");

    let high = world.add("post_process");
    {
        let entity = world.entity_mut(&high).unwrap();
        entity.translation = [0.0, 0.0, 0.0];
        entity.post.glow_intensity = 4.0;
        entity.post.blend = 1.0;
        entity.post.priority = 5;
        entity.post.tint = [0.0, 1.0, 0.0];
    }
    // Pull the high volume's camera sample into its blend band by shrinking it.
    world.entity_mut(&high).unwrap().phys2d.size = [1.2, 1.2];
    let band = post_volume::weight(world.entity(&high).unwrap(), [0.0, 0.0]);
    assert!(band > 0.05 && band < 1.0, "high volume is in its blend band {band}");
    let resolved =
        post_volume::resolve(&world.entities, [0.0, 0.0], &seal_scene::Glow2d::default());
    assert!(resolved.active);
    let expected = 1.0 + (4.0 - 1.0) * band;
    assert!((resolved.intensity - expected).abs() < 1e-3, "{} vs {expected}", resolved.intensity);
    assert!(resolved.tint[1] > resolved.tint[0]);

    let plain = post_volume::grade_rgb([0.4, 0.2, 0.2], [1.0, 1.0, 1.0], 1.0, 1.0);
    assert!((plain[0] - 0.4).abs() < 1e-6);
    assert!(post_volume::check("post_blend", "-1").is_some());
    let entity = world.entity_mut(&low).unwrap();
    assert!(post_volume::apply(&mut entity.post, "post_exposure", "2"));
}
