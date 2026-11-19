//! Audio Listener rows in the Details panel.

use seal_editor_core::EditorSession;
use seal_ui::PanelFrame;

use crate::{command, PanelEvent};

fn field_value(session: &EditorSession, id: &str, name: &str) -> String {
    session
        .models
        .nodes
        .iter()
        .find(|n| n.id == id)
        .and_then(|n| n.fields.iter().find(|(k, _)| k == name))
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

pub fn audio_listener_section(
    frame: &mut PanelFrame,
    session: &EditorSession,
    targets: &[&str],
) -> PanelEvent {
    let mut event = PanelEvent::None;
    for id in targets {
        if frame.toggle(
            &session.messages.get("inspector-audio-listener"),
            field_value(session, id, "audio_listener") == "yes",
        ) {
            let value = if field_value(session, id, "audio_listener") == "yes" {
                "no"
            } else {
                "yes"
            };
            event = command("inspector.field", &[("id", id), ("field", "audio_listener"), ("value", value)]);
        }
        let mut max_d = field_value(session, id, "listener_max_distance");
        if frame.property(&session.messages.get("inspector-field-listener-max-distance"), &mut max_d) {
            event = command(
                "inspector.field",
                &[("id", id), ("field", "listener_max_distance"), ("value", max_d.as_str())],
            );
        }
        let mut rolloff = field_value(session, id, "listener_rolloff");
        if frame.property(&session.messages.get("inspector-field-listener-rolloff"), &mut rolloff) {
            event = command(
                "inspector.field",
                &[("id", id), ("field", "listener_rolloff"), ("value", rolloff.as_str())],
            );
        }
        frame.muted(&session.messages.get("inspector-audio-listener-help"));
    }
    event
}