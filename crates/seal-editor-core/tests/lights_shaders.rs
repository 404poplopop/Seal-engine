//! Directional and freeform lights, glow, and a damaged canvas shader.

use seal_editor_core::material2d::load_program;
use seal_editor_core::{CommandArgs, EditorSession, Layout};

fn set(session: &mut EditorSession, id: &str, field: &str, value: &str) {
    session.models.set_field(id, field, value);
    session.mirror_node(id);
}

fn entity_id(session: &EditorSession) -> String {
    session.world.entities.iter().find(|e| e.id != "root").unwrap().id.clone()
}

#[test]
fn directional_and_freeform_lights_and_glow_save_load_and_undo() {
    let mut session = EditorSession::new(Layout::default());
    let id = entity_id(&session);
    set(&mut session, &id, "light2d", "directional");
    set(&mut session, &id, "light_blend", "multiply");
    set(&mut session, &id, "light_angle", "45");
    let light = &session.world.entity(&id).unwrap().light2d;
    assert_eq!(light.kind, "directional");
    assert_eq!(light.blend, "multiply");
    assert!((session.world.entity(&id).unwrap().rotation[2] - 45.0).abs() < 1e-3);

    set(&mut session, &id, "light2d", "freeform");
    set(&mut session, &id, "light_falloff", "2.5");
    let light = &session.world.entity(&id).unwrap().light2d;
    assert_eq!(light.kind, "freeform");
    assert!(light.points.len() >= 3, "a freeform light starts with a polygon");
    assert!((light.falloff - 2.5).abs() < 1e-3);

    session
        .dispatch("world.glow", &CommandArgs::new(&[("field", "enabled"), ("value", "yes")]))
        .unwrap();
    session
        .dispatch("world.glow", &CommandArgs::new(&[("field", "threshold"), ("value", "0.6")]))
        .unwrap();
    assert!(session.world.glow.enabled);
    assert!((session.world.glow.threshold - 0.6).abs() < 1e-3);
    let text = session.world.to_text();
    assert!(text.contains("glow2d"), "glow is written once it is turned on");
    let loaded = seal_world::World::from_text(&text).unwrap();
    let again = loaded.entity(&id).unwrap();
    assert_eq!(again.light2d.kind, "freeform");
    assert_eq!(again.light2d.blend, "multiply");
    assert_eq!(again.light2d.points, session.world.entity(&id).unwrap().light2d.points);
    assert!(loaded.glow.enabled);
    assert!((loaded.glow.threshold - 0.6).abs() < 1e-3);

    session.dispatch("edit.undo", &CommandArgs::default()).unwrap();
    assert!(session.world.glow.enabled, "undoing the threshold keeps glow on");
    assert!((session.world.glow.threshold - 0.8).abs() < 1e-3);
    session.dispatch("edit.undo", &CommandArgs::default()).unwrap();
    assert!(!session.world.glow.enabled);
}

#[test]
fn a_damaged_shade2d_file_names_the_line() {
    let path = std::env::temp_dir().join("seal-bad.shade2d");
    std::fs::write(&path, "vertex\nx = (\n").unwrap();
    let err = load_program(&path).unwrap_err();
    assert!(err.contains("line 2"), "{err}");
    let _ = std::fs::remove_file(&path);
}
