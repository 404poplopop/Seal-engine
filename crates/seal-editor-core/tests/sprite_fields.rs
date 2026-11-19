use seal_editor_core::{EditorSession, Layout};

#[test]
fn sprite_layer_frames_and_fps_edit_through_the_inspector_fields() {
    let mut session = EditorSession::new(Layout::default());
    let id = session.world.entities[2].id.clone();
    session.world.entities[2].sprite = "sprites/walk.png".into();
    session.adopt_world();
    let fields = &session.models.nodes.iter().find(|n| n.id == id).unwrap().fields;
    for (key, value) in [("layer", "0"), ("frames", "1"), ("fps", "8")] {
        assert!(fields.iter().any(|(k, v)| k == key && v == value), "{key} missing: {fields:?}");
    }
    session.models.set_field(&id, "layer", "-2");
    session.models.set_field(&id, "frames", "6");
    session.models.set_field(&id, "fps", "12.5");
    session.mirror_node(&id);
    let e = session.world.entity(&id).unwrap();
    assert_eq!((e.layer, e.frames, e.fps), (-2, 6, 12.5));
    // Nonsense is clamped instead of breaking the sheet.
    session.models.set_field(&id, "frames", "0");
    session.models.set_field(&id, "fps", "-3");
    session.mirror_node(&id);
    let e = session.world.entity(&id).unwrap();
    assert_eq!((e.frames, e.fps), (1, 0.0));
}

#[test]
fn sprite_tint_opacity_and_order_edit_save_and_load() {
    let mut session = EditorSession::new(Layout::default());
    let id = session.world.entities[2].id.clone();
    session.world.entities[2].sprite = "sprites/walk.png".into();
    session.adopt_world();
    let fields = &session.models.nodes.iter().find(|n| n.id == id).unwrap().fields;
    for (key, value) in [("tint", "#FFFFFF"), ("opacity", "1"), ("order", "0")] {
        assert!(fields.iter().any(|(k, v)| k == key && v == value), "{key} missing: {fields:?}");
    }
    session.models.set_field(&id, "tint", "#ff8000");
    session.models.set_field(&id, "opacity", "0.5");
    session.models.set_field(&id, "order", "3");
    session.mirror_node(&id);
    let e = session.world.entity(&id).unwrap();
    assert_eq!(e.order, 3);
    assert_eq!(seal_editor_core::tint_hex(&e.tint), "#FF800080");
    // A typo keeps the old colour; opacity is clamped.
    session.models.set_field(&id, "tint", "orange");
    session.models.set_field(&id, "opacity", "7");
    session.mirror_node(&id);
    let e = session.world.entity(&id).unwrap();
    assert_eq!(seal_editor_core::tint_hex(&e.tint), "#FF8000");
    session.models.set_field(&id, "opacity", "0.25");
    session.mirror_node(&id);

    let text = session.world.to_text();
    let loaded = seal_world::World::from_text(&text).unwrap();
    let back = loaded.entity(&id).unwrap();
    assert_eq!(back.order, 3);
    assert_eq!(seal_editor_core::tint_hex(&back.tint), "#FF800040");
}

#[test]
fn tint_text_accepts_short_long_and_see_through_colours() {
    use seal_editor_core::parse_tint;
    assert_eq!(parse_tint("#fff"), Some([1.0; 4]));
    assert_eq!(parse_tint("000000"), Some([0.0, 0.0, 0.0, 1.0]));
    assert_eq!(parse_tint("#00000000"), Some([0.0; 4]));
    assert_eq!(parse_tint("#12345"), None);
    assert_eq!(parse_tint("red"), None);
}

#[test]
fn nine_slice_borders_edit_save_and_load() {
    let mut session = EditorSession::new(Layout::default());
    let id = session.world.entities[2].id.clone();
    session.world.entities[2].sprite = "sprites/panel.png".into();
    session.adopt_world();
    session.models.set_field(&id, "slice", "8");
    session.mirror_node(&id);
    assert_eq!(session.world.entity(&id).unwrap().slice, [8.0; 4]);
    session.models.set_field(&id, "slice", "4, 6 2 3");
    session.mirror_node(&id);
    session.models.set_field(&id, "slice", "-1"); // refused, keeps the last good value
    session.mirror_node(&id);
    assert_eq!(session.world.entity(&id).unwrap().slice, [4.0, 6.0, 2.0, 3.0]);
    let loaded = seal_world::World::from_text(&session.world.to_text()).unwrap();
    assert_eq!(loaded.entity(&id).unwrap().slice, [4.0, 6.0, 2.0, 3.0]);
    assert_eq!(seal_editor_core::slice_text(&[4.0, 6.0, 2.0, 3.0]), "4, 6, 2, 3");
}
