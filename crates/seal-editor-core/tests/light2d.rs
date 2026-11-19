//! 2D lights and sprite normal maps: inspector edits, saving and loading.

use seal_editor_core::{EditorSession, Layout};

#[test]
fn light2d_settings_edit_save_and_load() {
    let mut session = EditorSession::new(Layout::default());
    let id = session.world.entities[2].id.clone();
    for (field, value) in
        [("light2d", "spot"), ("light_range", "7.5"), ("light_cone", "45"), ("light_shadows", "no")]
    {
        session.models.set_field(&id, field, value);
        session.mirror_node(&id);
    }
    // Unknown kinds and out-of-range numbers are refused or clamped.
    session.models.set_field(&id, "light2d", "laser");
    session.mirror_node(&id);
    session.models.set_field(&id, "light_cone", "999");
    session.mirror_node(&id);
    let light = session.world.entity(&id).unwrap().light2d.clone();
    assert_eq!(light.kind, "spot");
    assert_eq!(light.range, 7.5);
    assert_eq!(light.cone, 360.0);
    assert!(!light.shadows);
    let loaded = seal_world::World::from_text(&session.world.to_text()).unwrap();
    assert_eq!(loaded.entity(&id).unwrap().light2d, light);
    // Turning it off leaves an ordinary object that saves no 2D light.
    session.models.set_field(&id, "light2d", "");
    session.mirror_node(&id);
    assert!(session.world.entity(&id).unwrap().light2d.kind.is_empty());
    assert!(!session.world.to_text().contains("light2d ="));
}

#[test]
fn sprite_normal_map_saves_and_loads() {
    let mut session = EditorSession::new(Layout::default());
    let id = session.world.entities[2].id.clone();
    session.world.entities[2].sprite = "sprites/rock.png".into();
    session.adopt_world();
    session.models.set_field(&id, "normal_map", "sprites\\rock_n.png");
    session.mirror_node(&id);
    assert_eq!(session.world.entity(&id).unwrap().normal_map, "sprites/rock_n.png");
    let loaded = seal_world::World::from_text(&session.world.to_text()).unwrap();
    assert_eq!(loaded.entity(&id).unwrap().normal_map, "sprites/rock_n.png");
}

#[test]
fn old_scenes_without_2d_lights_still_load() {
    let session = EditorSession::new(Layout::default());
    let text = session.world.to_text();
    assert!(!text.contains("light2d"));
    let loaded = seal_world::World::from_text(&text).unwrap();
    assert!(loaded.entities.iter().all(|e| e.light2d.kind.is_empty() && e.normal_map.is_empty()));
}

#[test]
fn sprite_parallax_and_repeat_save_and_load() {
    let mut session = EditorSession::new(Layout::default());
    let id = session.world.entities[2].id.clone();
    session.world.entities[2].sprite = "art/hills.png".into();
    session.adopt_world();
    for (field, value) in [("parallax", "0.3, 0.5"), ("repeat_x", "yes"), ("parallax", "-2")] {
        session.models.set_field(&id, field, value);
        session.mirror_node(&id);
    }
    let e = session.world.entity(&id).unwrap();
    assert_eq!(e.parallax, [0.3, 0.5]);
    assert!(e.repeat_x);
    let loaded = seal_world::World::from_text(&session.world.to_text()).unwrap();
    assert_eq!(loaded.entity(&id).unwrap().parallax, [0.3, 0.5]);
    assert!(loaded.entity(&id).unwrap().repeat_x);
    assert_eq!(seal_editor_core::parse_parallax("0.5"), Some([0.5, 0.5]));
    assert_eq!(seal_editor_core::parse_parallax("1 2 3"), None);
}
