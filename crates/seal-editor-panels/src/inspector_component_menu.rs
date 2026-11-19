//! Context menu for inspector component tree rows.

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

pub fn inspector_component_context_entries(session: &EditorSession) -> Vec<ContextMenuEntry> {
    vec![
        row(
            session,
            "inspector.component_rename",
            "inspector-component-menu-rename",
            Icon::EditRename,
        ),
        row(
            session,
            "inspector.component_duplicate",
            "inspector-component-menu-duplicate",
            Icon::EditDuplicate,
        ),
        row(
            session,
            "inspector.component_delete",
            "inspector-component-menu-delete",
            Icon::EditDelete,
        ),
        row(session, "inspector.component_copy", "inspector-component-menu-copy", Icon::EditCopy),
        row(
            session,
            "inspector.component_paste",
            "inspector-component-menu-paste",
            Icon::EditPaste,
        ),
        row(
            session,
            "inspector.component_make_root",
            "inspector-component-menu-make-root",
            Icon::EntityCube,
        ),
    ]
}
