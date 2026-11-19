//! Import and reimport activity for the current session.

use seal_editor_core::EditorSession;
use seal_ui::PanelFrame;

use crate::PanelEvent;

pub const ID: &str = "import-activity";

pub fn paint(frame: &mut PanelFrame, session: &EditorSession) -> PanelEvent {
    frame.scroll(|frame| {
        frame.padded(|frame| {
            frame.title(&session.messages.get("panel-import-activity-title"));
            if session.models.import_activity.is_empty() {
                frame.muted(&session.messages.get("import-activity-empty"));
                return;
            }
            for row in &session.models.import_activity {
                let headline = session.messages.format(
                    "import-activity-row",
                    &[
                        ("time", &row.at_ms.to_string()),
                        ("file", row.file.as_str()),
                        ("importer", row.importer.as_str()),
                        ("result", row.result.as_str()),
                        ("duration", &row.duration_ms.to_string()),
                    ],
                );
                frame.muted(&headline);
                if !row.output_hash.is_empty() {
                    frame.muted(
                        &session
                            .messages
                            .format("import-activity-hash", &[("hash", row.output_hash.as_str())]),
                    );
                }
            }
        });
    });
    PanelEvent::None
}
