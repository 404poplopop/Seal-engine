//! One text field per frame, for the event that frame fires during play.

use seal_editor_core::sprite_frames::Frame;
use seal_editor_core::EditorSession;
use seal_ui::PanelFrame;

use crate::PanelEvent;

/// Event name rows for the frames of the open clip. Each edit is saved at once.
pub fn rows(
    frame: &mut PanelFrame,
    session: &EditorSession,
    clip: &str,
    frames: &[Frame],
) -> PanelEvent {
    let mut event = PanelEvent::None;
    for (index, shown) in frames.iter().enumerate() {
        let label = session
            .messages
            .format("asset-anim-frame-event", &[("index", &(index + 1).to_string())]);
        let mut text = shown.event.clone();
        let mut pick = None;
        let modified = !shown.event.is_empty();
        let tip = modified.then(|| session.messages.get("asset-anim-event-clear"));
        let (changed, reset, commit_focus) = frame.property_validated_row(
            &label,
            &mut text,
            None,
            modified,
            tip.as_deref(),
            None,
            None,
            &mut pick,
            Some("asset-anim-event"),
        );
        if reset {
            text.clear();
        }
        if (changed || reset || commit_focus) && text != shown.event {
            let index_text = index.to_string();
            event = crate::command(
                "asset.frames_frame",
                &[
                    ("clip", clip),
                    ("index", index_text.as_str()),
                    ("action", "set"),
                    ("event", text.trim()),
                ],
            );
        }
    }
    event
}
