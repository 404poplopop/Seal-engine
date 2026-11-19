//! Property row context menu.

use seal_editor_core::EditorSession;
use seal_ui::{ContextMenuEntry, ContextMenuRow, Icon};

fn text(session: &EditorSession, key: &str) -> String {
    session.messages.get(key)
}

fn row(session: &EditorSession, command: &str, label_key: &str, icon: Icon) -> ContextMenuEntry {
    ContextMenuEntry::Row(ContextMenuRow {
        label: text(session, label_key),
        icon,
        shortcut: String::new(),
        enabled: true,
        reason: String::new(),
        command: Some((command.into(), Vec::new())),
        children: Vec::new(),
    })
}

pub fn inspector_property_context_entries(session: &EditorSession) -> Vec<ContextMenuEntry> {
    vec![
        row(session, "inspector.property_copy", "inspector-property-menu-copy", Icon::EditCopy),
        row(session, "inspector.property_paste", "inspector-property-menu-paste", Icon::EditPaste),
        row(
            session,
            "inspector.property_copy_name",
            "inspector-property-menu-copy-name",
            Icon::EditCopy,
        ),
        row(session, "inspector.property_reset", "inspector-property-menu-reset", Icon::ViewReset),
        row(
            session,
            "inspector.property_favourite",
            "inspector-property-menu-favourite",
            Icon::ViewStar,
        ),
        row(
            session,
            "inspector.property_settings_search",
            "inspector-property-menu-settings",
            Icon::EditSearch,
        ),
    ]
}
