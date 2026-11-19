//! Package tests: isolate toggle and overlay layout persistence.

use seal_editor_core::{CommandArgs, EditorSession, Layout};

#[test]
fn isolate_hides_unselected_entities() {
    let mut session = EditorSession::new(Layout::default());
    session.dispatch("outliner.select-set", &CommandArgs::new(&[("ids", "hero")])).unwrap();
    session.dispatch("viewport.isolate", &CommandArgs::default()).unwrap();
    assert!(session.world.entities.iter().any(|e| e.id == "crate" && e.hidden));
    session.dispatch("viewport.isolate", &CommandArgs::default()).unwrap();
    assert!(session.world.entities.iter().any(|e| e.id == "crate" && !e.hidden));
}

#[test]
fn collapsed_toolbar_toggles_panel_state() {
    let mut session = EditorSession::new(Layout::default());
    session
        .dispatch(
            "viewport.overlay",
            &CommandArgs::new(&[("target", "toolbar"), ("action", "collapse")]),
        )
        .unwrap();
    let text = session.models.panel_states.get("viewport.overlay_layout").cloned().unwrap();
    assert!(text.contains("toolbar=tl:1") || text.contains(":1"));
    session
        .dispatch(
            "viewport.overlay",
            &CommandArgs::new(&[("target", "toolbar"), ("action", "collapse")]),
        )
        .unwrap();
    let again = session.models.panel_states.get("viewport.overlay_layout").cloned().unwrap();
    assert!(again.contains("toolbar=tl:0") || again.contains(":0"));
}

#[test]
fn overlay_layout_persists_in_panel_state() {
    let mut session = EditorSession::new(Layout::default());
    session
        .dispatch(
            "viewport.overlay",
            &CommandArgs::new(&[("target", "stats"), ("action", "edge"), ("edge", "bl")]),
        )
        .unwrap();
    let text = session.models.panel_states.get("viewport.overlay_layout").cloned().unwrap();
    assert!(text.contains("stats=bl"));
}
