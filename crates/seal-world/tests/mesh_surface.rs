//! Mesh base colour and the sky cubemap round-trip in scene text.

use seal_world::seal_scene::MeshMaterial;
use seal_world::World;

#[test]
fn mesh_material_round_trips_and_old_scenes_stay_unchanged() {
    let demo = World::demo();
    let before = demo.to_text();
    assert!(!before.contains("mesh_material"), "{before}");
    let loaded = World::from_text(&before).unwrap();
    assert_eq!(loaded.to_text(), before);
    let mut world = demo;
    let mesh = world.entities.iter_mut().find(|e| e.shape.is_some()).unwrap();
    mesh.mesh_material = MeshMaterial {
        texture: "art/brick.texture".into(),
        layer: 2,
        slice: 0.0,
        tile: [2.0, 3.0],
    };
    let text = world.to_text();
    assert!(text.contains("mesh_material"), "{text}");
    assert!(text.contains("art/brick.texture"), "{text}");
    let back = World::from_text(&text).unwrap();
    let got = back.entities.iter().find(|e| e.shape.is_some()).unwrap();
    assert_eq!(got.mesh_material.texture, "art/brick.texture");
    assert_eq!(got.mesh_material.layer, 2);
    assert_eq!(got.mesh_material.tile, [2.0, 3.0]);
    assert_eq!(back.to_text(), text);
    let mut damaged = text.clone();
    damaged = damaged.replacen("layer = 2", "layer = no", 1);
    assert!(World::from_text(&damaged).is_err(), "a damaged layer is rejected");
}

#[test]
fn sky_cubemap_round_trips_and_stays_off_old_scenes() {
    let demo = World::demo();
    assert!(!demo.to_text().contains("sky_cubemap"));
    let mut world = demo;
    world.sky_cubemap = "art/sky.texture".into();
    let text = world.to_text();
    assert!(text.contains("sky_cubemap"), "{text}");
    let back = World::from_text(&text).unwrap();
    assert_eq!(back.sky_cubemap, "art/sky.texture");
    assert_eq!(back.to_text(), text);
    assert!(World::from_text("not a scene").is_err());
}
