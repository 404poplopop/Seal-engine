//! Outliner row context menu: sections with captions.

use seal_editor_core::EditorSession;
use seal_ui::{ContextMenuEntry, ContextMenuRow, Icon};

use crate::viewport_menu::entity_entries;

fn text(session: &EditorSession, key: &str) -> String {
    session.messages.get(key)
}

fn off(session: &EditorSession, label: &str, icon: Icon) -> ContextMenuEntry {
    ContextMenuEntry::Row(ContextMenuRow {
        label: text(session, label),
        icon,
        shortcut: String::new(),
        enabled: false,
        reason: text(session, "menu-unavailable"),
        command: None,
        children: Vec::new(),
    })
}

fn cmd_asset_edit(session: &EditorSession) -> ContextMenuEntry {
    let Some(id) = session.models.selection.as_deref() else {
        return off(session, "outliner-menu-edit-asset", Icon::AssetPrefab);
    };
    let Some(node) = session.models.nodes.iter().find(|node| node.id == id) else {
        return off(session, "outliner-menu-edit-asset", Icon::AssetPrefab);
    };
    let asset = node
        .fields
        .iter()
        .find(|(key, _)| key == "source_asset")
        .map(|(_, value)| value.as_str())
        .unwrap_or_default();
    let kind = node
        .fields
        .iter()
        .find(|(key, _)| key == "source_kind")
        .map(|(_, value)| value.as_str())
        .unwrap_or("prefab");
    let enabled = !asset.is_empty() && session.commands.get("asset.edit").is_some();
    ContextMenuEntry::Row(ContextMenuRow {
        label: text(session, "outliner-menu-edit-asset"),
        icon: Icon::AssetPrefab,
        shortcut: String::new(),
        enabled,
        reason: text(session, "menu-needs-selection"),
        command: if enabled {
            Some((
                "asset.edit".into(),
                vec![("asset".into(), asset.to_string()), ("kind".into(), kind.to_string())],
            ))
        } else {
            None
        },
        children: Vec::new(),
    })
}

fn branch(
    session: &EditorSession,
    label: &str,
    icon: Icon,
    children: Vec<ContextMenuEntry>,
) -> ContextMenuEntry {
    ContextMenuEntry::Row(ContextMenuRow {
        label: text(session, label),
        icon,
        shortcut: String::new(),
        enabled: true,
        reason: String::new(),
        command: None,
        children,
    })
}

/// Full outliner context menu.
pub fn outliner_context_entries(session: &EditorSession) -> Vec<ContextMenuEntry> {
    let expand_live = session
        .commands
        .get("outliner.expand_all")
        .is_some_and(|_| session.commands.is_enabled("outliner.expand_all", session));
    let collapse_live = session
        .commands
        .get("outliner.collapse_all")
        .is_some_and(|_| session.commands.is_enabled("outliner.collapse_all", session));
    let mut out = vec![
        ContextMenuEntry::Section(text(session, "outliner-menu-asset-options")),
        cmd_asset_edit(session),
        off(session, "outliner-menu-browse-content", Icon::FileOpen),
        ContextMenuEntry::Section(text(session, "outliner-menu-view-options")),
        ContextMenuEntry::Row(ContextMenuRow {
            label: text(session, "command-outliner-expand"),
            icon: Icon::EntityFolder,
            shortcut: session.keymap.shortcut("outliner.expand_all"),
            enabled: expand_live,
            reason: text(session, "menu-unavailable"),
            command: if expand_live {
                Some(("outliner.expand_all".into(), Vec::new()))
            } else {
                None
            },
            children: Vec::new(),
        }),
        ContextMenuEntry::Row(ContextMenuRow {
            label: text(session, "command-outliner-collapse"),
            icon: Icon::EntityFolder,
            shortcut: session.keymap.shortcut("outliner.collapse_all"),
            enabled: collapse_live,
            reason: text(session, "menu-unavailable"),
            command: if collapse_live {
                Some(("outliner.collapse_all".into(), Vec::new()))
            } else {
                None
            },
            children: Vec::new(),
        }),
        ContextMenuEntry::Section(text(session, "outliner-menu-entity-options")),
    ];
    out.extend(entity_entries(session));
    out.extend(vec![
        ContextMenuEntry::Section(text(session, "outliner-menu-outliner")),
        branch(
            session,
            "outliner-menu-move-to",
            Icon::EntityFolder,
            vec![off(session, "outliner-menu-move-root", Icon::EntityFolder)],
        ),
        off(session, "menu-pin", Icon::EditPin),
    ]);
    out
}
