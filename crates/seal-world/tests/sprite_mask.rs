//! Sprite mask fields survive a scene save, and older scenes stay as they were.

use seal_world::{sprite_mask, World};

fn mask_world() -> World {
    let mut world = World::demo();
    let e = &mut world.entities[2];
    e.sprite = "sprites/hero.png".into();
    e.mask = "inside".into();
    e.layer = 1;
    e.order = 4;
    let mask = &mut world.entities[1];
    mask.sprite_mask.shape = "circle".into();
    mask.sprite_mask.cutoff = 0.4;
    mask.sprite_mask.layer_min = 0;
    mask.sprite_mask.layer_max = 2;
    mask.sprite_mask.order_min = -5;
    mask.sprite_mask.order_max = 8;
    mask.rotation[2] = 30.0;
    mask.scale = [3.0, 1.5, 1.0];
    world
}

#[test]
fn mask_settings_round_trip_and_old_scenes_do_not_change() {
    let world = mask_world();
    let text = world.to_text();
    let back = World::from_text(&text).unwrap();
    assert_eq!(back.to_text(), text);
    let sprite = &back.entities[2];
    assert_eq!(sprite.mask, "inside");
    assert_eq!((sprite.layer, sprite.order), (1, 4));
    let mask = &back.entities[1];
    assert_eq!(mask.sprite_mask.shape, "circle");
    assert!((mask.sprite_mask.cutoff - 0.4).abs() < 1e-4);
    assert_eq!((mask.sprite_mask.layer_min, mask.sprite_mask.layer_max), (0, 2));
    assert_eq!((mask.sprite_mask.order_min, mask.sprite_mask.order_max), (-5, 8));
    assert!((mask.rotation[2] - 30.0).abs() < 1e-4);
    assert_eq!(mask.scale[0], 3.0);

    let old = World::demo();
    let old_text = old.to_text();
    assert!(!old_text.contains("sprite_mask"), "a scene with no mask does not mention one");
    assert!(!old_text.contains("sprite_clip"));
    let loaded = World::from_text(&old_text).unwrap();
    assert_eq!(loaded.to_text(), old_text);
    assert!(loaded.entities.iter().all(|e| e.mask.is_empty() && e.sprite_mask.shape.is_empty()));
}

#[test]
fn a_damaged_mask_value_is_refused_and_a_broken_scene_does_not_load() {
    assert!(sprite_mask::check("mask_cutoff", "nope").is_some());
    assert!(sprite_mask::check("mask_shape", "triangle").is_some());
    assert!(sprite_mask::check("mask", "both").is_some());
    let mut world = mask_world();
    assert!(!sprite_mask::apply(&mut world.entities[1], "mask_cutoff", "nope"));
    assert!((world.entities[1].sprite_mask.cutoff - 0.4).abs() < 1e-4);

    let text = world.to_text().replace("cutoff = 0.4", "cutoff = nope");
    assert!(World::from_text(&text).is_err(), "a mask with a broken number does not load");
}

#[test]
fn create_menu_kind_makes_a_rectangle_mask() {
    let mut world = World::demo();
    let id = world.add("sprite_mask");
    let made = world.entity(&id).unwrap();
    assert_eq!(made.name, "Sprite Mask");
    assert_eq!(made.sprite_mask.shape, "rect");
    assert_eq!(made.scale, [2.0, 2.0, 1.0]);
    let rows = sprite_mask::fields(made);
    assert!(rows.iter().any(|(k, v)| k == "mask_shape" && v == "rect"));
}
