use seal_editor_core::viewport_split::{
    pane_rects, pane_under_pointer, restore_split_text, PaneRect,
};
use seal_editor_core::{CommandArgs, EditorSession, Layout};

fn layout(session: &mut EditorSession, preset: &str) {
    session.dispatch("viewport.layout", &CommandArgs::new(&[("preset", preset)])).unwrap();
}

#[test]
fn layout_change_keeps_pane_0_camera() {
    let mut session = EditorSession::new(Layout::default());
    session.models.viewport_pan_x = 12.5;
    session.models.viewport_pan_y = 64.0;
    session.models.viewport_zoom = 2.25;
    session.set_draft("viewport-yaw", "17").unwrap();
    session.set_draft("viewport-pitch", "-9").unwrap();
    session.set_draft("viewport-distance", "22").unwrap();
    layout(&mut session, "layout_four");
    assert_eq!(session.models.viewport_layout_preset, "four");
    assert_eq!(session.models.viewport_panes.len(), 4);
    let pane = &session.models.viewport_panes[0];
    assert_eq!(pane.pan_x, 12.5);
    assert_eq!(pane.pan_y, 64.0);
    assert_eq!(pane.zoom, 2.25);
    assert_eq!(pane.yaw, 17.0);
    assert_eq!(pane.pitch, -9.0);
    assert_eq!(pane.distance, 22.0);
    assert_ne!(session.models.viewport_panes[1].pan_x, 12.5);
}

#[test]
fn pane_state_round_trips_through_the_panel_state_file() {
    let dir = std::env::temp_dir().join(format!("seal-split-round-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("session.seal");
    let mut session = EditorSession::new(Layout::default());
    layout(&mut session, "layout_two_cols");
    session
        .dispatch(
            "viewport.camera",
            &CommandArgs::new(&[
                ("pan_x", "3.5"),
                ("pan_y", "80"),
                ("zoom", "2"),
                ("yaw", "11"),
                ("pitch", "-8"),
                ("distance", "20"),
            ]),
        )
        .unwrap();
    session
        .dispatch("viewport.view", &CommandArgs::new(&[("view", "top"), ("index", "1")]))
        .unwrap();
    session
        .dispatch("viewport.shade", &CommandArgs::new(&[("mode", "unlit"), ("index", "1")]))
        .unwrap();
    session.dispatch("viewport.split", &CommandArgs::new(&[("values", "0.35")])).unwrap();
    session.save_panel_state(&path).unwrap();
    let mut loaded = EditorSession::new(Layout::default());
    loaded.load_panel_state(&path).unwrap();
    assert_eq!(loaded.models.viewport_layout_preset, "two_cols");
    assert_eq!(loaded.models.viewport_panes.len(), 2);
    assert!((loaded.models.viewport_splits[0] - 0.35).abs() < 1.0e-5);
    let pane = &loaded.models.viewport_panes[0];
    assert_eq!(pane.pan_x, 3.5);
    assert_eq!(pane.pan_y, 80.0);
    assert_eq!(pane.zoom, 2.0);
    assert_eq!(pane.yaw, 11.0);
    assert_eq!(pane.pitch, -8.0);
    assert_eq!(pane.distance, 20.0);
    assert_eq!(loaded.models.viewport_panes[1].view, "top");
    assert_eq!(loaded.models.viewport_panes[1].shade, "unlit");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn damaged_pane_state_falls_back_to_single() {
    let dir = std::env::temp_dir().join(format!("seal-split-bad-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("session.seal");
    let mut session = EditorSession::new(Layout::default());
    layout(&mut session, "layout_four");
    session.save_panel_state(&path).unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    let damaged = raw.replace("\\\"four\\\"", "\\\"not-a-layout\\\"");
    assert_ne!(damaged, raw, "saved split text had no four preset to break");
    std::fs::write(&path, damaged).unwrap();
    let mut loaded = EditorSession::new(Layout::default());
    loaded.load_panel_state(&path).unwrap();
    assert_eq!(loaded.models.viewport_layout_preset, "single");
    assert_eq!(loaded.models.viewport_panes.len(), 1);
    assert!(loaded.models.viewport_splits.is_empty());
    assert!(!loaded.models.viewport_pane_expanded);
    let mut direct = EditorSession::new(Layout::default());
    layout(&mut direct, "layout_three");
    assert!(!restore_split_text(&mut direct, "{ preset: \"nope\", panes: [] }"));
    assert_eq!(direct.models.viewport_layout_preset, "single");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn maximize_restore_returns_the_same_splitter_positions() {
    let mut session = EditorSession::new(Layout::default());
    layout(&mut session, "layout_four");
    session.dispatch("viewport.split", &CommandArgs::new(&[("values", "0.3,0.7")])).unwrap();
    let before = session.models.viewport_splits.clone();
    session.dispatch("viewport.pane_expand", &CommandArgs::default()).unwrap();
    assert!(session.models.viewport_pane_expanded);
    assert_eq!(session.models.viewport_splits, before);
    session.dispatch("viewport.pane_expand", &CommandArgs::default()).unwrap();
    assert!(!session.models.viewport_pane_expanded);
    assert_eq!(session.models.viewport_splits, before);
    assert_eq!(session.models.viewport_layout_preset, "four");
}

#[test]
fn input_routing_picks_the_pane_under_the_pointer() {
    let outer = PaneRect { x: 0.0, y: 0.0, w: 400.0, h: 300.0 };
    let rects = pane_rects(outer, "four", &[0.5, 0.5], 4.0);
    assert_eq!(rects.len(), 4);
    for (index, rect) in rects.iter().copied().enumerate() {
        let x = rect.x + rect.w * 0.5;
        let y = rect.y + rect.h * 0.5;
        assert_eq!(pane_under_pointer(&rects, x, y), Some(index));
    }
    let gap_x = rects[0].x + rects[0].w + 1.0;
    let gap_y = rects[0].y + rects[0].h * 0.5;
    assert_eq!(pane_under_pointer(&rects, gap_x, gap_y), None);
    assert_eq!(pane_under_pointer(&rects, -4.0, 10.0), None);
    let side = pane_rects(outer, "two_cols", &[0.5], 4.0);
    assert_eq!(pane_under_pointer(&side, side[1].x + 2.0, side[1].y + 2.0), Some(1));
    let stacked = pane_rects(outer, "two_rows", &[0.5], 4.0);
    assert_eq!(
        pane_under_pointer(&stacked, stacked[1].x + 2.0, stacked[1].y + 2.0),
        Some(1)
    );
}

#[test]
fn a_2d_scene_defaults_every_pane_to_the_flat_view() {
    let mut session = EditorSession::new(Layout::default());
    for entity in &mut session.world.entities {
        if entity.shape.is_some() {
            entity.sprite = "sprites/dirt.png".into();
        }
    }
    assert!(session.scene_is_2d());
    session.models.viewport_pan_x = 6.0;
    layout(&mut session, "layout_three");
    assert!(session.models.viewport_panes.len() >= 3);
    for pane in &session.models.viewport_panes {
        assert_eq!(pane.view, "2d");
        assert_eq!(pane.yaw, 0.0);
        assert_eq!(pane.pitch, 0.0);
    }
    assert_eq!(session.models.viewport_panes[0].pan_x, 6.0);
}
