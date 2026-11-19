use seal_editor_core::{EditorSession, Layout};

/// Turns the starter scene into a sprite-only scene, like a 2D game level.
fn make_2d(session: &mut EditorSession) {
    for entity in &mut session.world.entities {
        if entity.shape.is_some() {
            entity.sprite = "sprites/dirt.png".into();
        }
    }
}

#[test]
fn a_2d_scene_never_stays_in_the_angled_perspective_view() {
    let mut session = EditorSession::new(Layout::default());
    make_2d(&mut session);
    assert!(session.scene_is_2d());
    session.models.viewport_view = "perspective".into();
    session.flatten_view_for_2d_scene();
    assert_eq!(session.models.viewport_view, "front");
    assert_eq!(session.draft("viewport-pitch").as_deref(), Some("0"));
}

#[test]
fn a_3d_scene_keeps_its_perspective_view() {
    let mut session = EditorSession::new(Layout::default());
    assert!(!session.scene_is_2d(), "starter scene has 3D shapes");
    session.models.viewport_view = "perspective".into();
    session.flatten_view_for_2d_scene();
    assert_eq!(session.models.viewport_view, "perspective");
}

#[test]
fn saved_perspective_state_is_flattened_when_loaded_for_a_2d_scene() {
    let dir = std::env::temp_dir().join(format!("seal-view2d-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("session.seal");
    let mut first = EditorSession::new(Layout::default());
    first.models.viewport_view = "perspective".into();
    first.save_panel_state(&path).unwrap();
    let mut again = EditorSession::new(Layout::default());
    make_2d(&mut again);
    again.load_panel_state(&path).unwrap();
    assert_eq!(again.models.viewport_view, "front");
    let _ = std::fs::remove_dir_all(&dir);
}
