//! The 2D camera: inspector edits, saving and loading, and the play view.

use seal_editor_core::{EditorSession, Layout};

fn camera_session() -> (EditorSession, String, String) {
    let mut session = EditorSession::new(Layout::default());
    let cam = session.world.entities[2].id.clone();
    let target = session.world.entities[3].clone();
    for (field, value) in [
        ("camera2d", "yes"),
        ("cam_follow", &format!("  {} ", target.name)),
        ("cam_smoothing", "0"),
        ("cam_zoom", "2"),
        ("cam_limits", "-5, 5, -5, 5"),
        ("cam_pixels", "16"),
    ] {
        session.models.set_field(&cam, field, value);
        session.mirror_node(&cam);
    }
    (session, cam, target.id)
}

#[test]
fn camera2d_settings_edit_save_and_load() {
    let (mut session, cam, _) = camera_session();
    // Bad values are refused or clamped.
    for (field, value) in
        [("cam_zoom", "500"), ("cam_limits", "5, -5, 0, 1"), ("cam_smoothing", "x")]
    {
        session.models.set_field(&cam, field, value);
        session.mirror_node(&cam);
    }
    let c = session.world.entity(&cam).unwrap().camera2d.clone();
    assert!(c.on);
    assert_eq!(c.zoom, 50.0);
    assert_eq!(c.smoothing, 0.0);
    assert_eq!(c.limits, [-5.0, 5.0, -5.0, 5.0]);
    assert_eq!(c.pixel_snap, 16.0);
    assert!(!c.follow.starts_with(' '));
    let loaded = seal_world::World::from_text(&session.world.to_text()).unwrap();
    assert_eq!(loaded.entity(&cam).unwrap().camera2d, c);
    assert_eq!(seal_editor_core::parse_limits("0, 0, 0, 0"), Some([0.0; 4]));
    assert_eq!(seal_editor_core::parse_limits("1, 2"), None);
}

#[test]
fn play_view_follows_target_inside_limits_and_shakes() {
    let (mut session, cam, target) = camera_session();
    session.world.entity_mut(&target).unwrap().translation = [3.0, 1.0, 0.0];
    session.world.play();
    session.world.step(1.0 / 60.0);
    let view = session.world.play_camera2d().expect("2D camera view while playing");
    assert_eq!(view.center, [3.0, 1.0]);
    assert_eq!(view.zoom, 2.0);
    // Limits keep a 4-wide view inside -5..5.
    let c = seal_world::camera2d::clamp_center([9.0, 0.0], [2.0, 2.0], view.limits);
    assert_eq!(c, [3.0, 0.0]);
    assert_eq!(session.world.play_shake(), [0.0, 0.0]);
    session.world.camera_shake(1.0, 0.5);
    session.world.step(1.0 / 60.0);
    let s = session.world.play_shake();
    assert!(s[0] != 0.0 || s[1] != 0.0);
    assert!(s[0].abs() <= 1.0 && s[1].abs() <= 1.0);
    for _ in 0..60 {
        session.world.step(1.0 / 60.0);
    }
    assert_eq!(session.world.play_shake(), [0.0, 0.0]);
    session.world.stop();
    assert!(session.world.play_camera2d().is_none());
    let _ = cam;
}

#[test]
fn drag_margins_edit_save_and_load() {
    let (mut session, cam, _) = camera_session();
    for (field, value) in [
        ("cam_drag_left", "0.2"),
        ("cam_drag_right", "1.5"),
        ("cam_drag_top", "nope"),
        ("cam_drag_bottom", "0.1"),
    ] {
        session.models.set_field(&cam, field, value);
        session.mirror_node(&cam);
    }
    let drag = session.world.entity(&cam).unwrap().camera2d.drag;
    assert_eq!(drag, [0.2, 1.0, 0.0, 0.1]);
    let loaded = seal_world::World::from_text(&session.world.to_text()).unwrap();
    assert_eq!(loaded.entity(&cam).unwrap().camera2d.drag, drag);
}
