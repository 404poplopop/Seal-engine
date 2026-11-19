//! Viewport right-click menus: entity list matches the Entity menu.

use seal_editor_core::EditorSession;
use seal_ui::{ContextMenuEntry, ContextMenuRow, Icon};

fn text(session: &EditorSession, key: &str) -> String {
    session.messages.get(key)
}

fn shortcut(session: &EditorSession, id: &str) -> String {
    session.keymap.shortcut(id)
}

fn off(session: &EditorSession, label: &str, icon: Icon) -> ContextMenuEntry {
    off_reason(session, label, icon, "menu-unavailable")
}

fn off_reason(session: &EditorSession, label: &str, icon: Icon, reason: &str) -> ContextMenuEntry {
    ContextMenuEntry::Row(ContextMenuRow {
        label: text(session, label),
        icon,
        shortcut: String::new(),
        enabled: false,
        reason: text(session, reason),
        command: None,
        children: Vec::new(),
    })
}

fn cmd(
    session: &EditorSession,
    label: &str,
    icon: Icon,
    id: &str,
    needs_selection: bool,
) -> ContextMenuEntry {
    let exists = session.commands.get(id).is_some();
    let live = exists && session.commands.is_enabled(id, session);
    let selected =
        session.foundation.selection.last().is_some() || session.models.selection.is_some();
    let enabled = live && (!needs_selection || selected);
    let reason = if !live {
        text(session, "menu-unavailable")
    } else if needs_selection && !selected {
        text(session, "menu-needs-selection")
    } else {
        String::new()
    };
    ContextMenuEntry::Row(ContextMenuRow {
        label: text(session, label),
        icon,
        shortcut: shortcut(session, id),
        enabled,
        reason,
        command: if enabled { Some((id.to_string(), Vec::new())) } else { None },
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

/// Entity commands (viewport and outliner ENTITY OPTIONS).
pub fn entity_entries(session: &EditorSession) -> Vec<ContextMenuEntry> {
    vec![
        // Rename, Duplicate and Delete sit on the top level: they are the
        // everyday entity actions and must be found without opening a submenu.
        cmd(session, "menu-rename", Icon::EditRename, "edit.rename", true),
        cmd(session, "menu-duplicate", Icon::EditDuplicate, "edit.duplicate", true),
        cmd(session, "menu-delete", Icon::EditDelete, "edit.delete", true),
        branch(
            session,
            "menu-visibility",
            Icon::ViewEye,
            vec![cmd(session, "menu-hide-selected", Icon::ViewEyeOff, "scene.hide", true)],
        ),
        branch(
            session,
            "menu-transform",
            Icon::ToolTransform,
            vec![
                off(session, "menu-reset-location", Icon::ToolMove),
                off(session, "menu-reset-rotation", Icon::ToolRotate),
                off(session, "menu-reset-scale", Icon::ToolScale),
                off(session, "menu-mirror-x", Icon::ToolMove),
                off(session, "menu-mirror-y", Icon::ToolMove),
                off(session, "menu-mirror-z", Icon::ToolMove),
                cmd(session, "menu-align-floor", Icon::ViewChevronDown, "scene.floor", true),
            ],
        ),
        branch(
            session,
            "menu-snapping",
            Icon::ToolGridSnap,
            vec![
                off(session, "menu-snap-grid", Icon::ToolGridSnap),
                off(session, "menu-snap-rotation", Icon::ToolAngleSnap),
                off(session, "menu-snap-scale", Icon::ToolScaleSnap),
                off(session, "menu-snap-surface", Icon::ToolSnapSurface),
            ],
        ),
        branch(
            session,
            "menu-pivot",
            Icon::ToolMove,
            vec![
                off(session, "menu-pivot-center", Icon::ToolMove),
                off(session, "menu-pivot-object", Icon::EntityCube),
                off(session, "menu-pivot-world", Icon::ToolWorld),
            ],
        ),
        shape_branch(session),
        cmd(session, "menu-group", Icon::EntityGroup, "scene.group", true),
        cmd(session, "menu-merge-objects", Icon::EntityGroup, "scene.merge_objects", true),
        cmd(session, "menu-ungroup", Icon::EditUnlink, "scene.ungroup", true),
        cmd(session, "menu-create-prefab", Icon::EntityGroup, "scene.make_prefab", true),
        branch(
            session,
            "menu-attach",
            Icon::EditLink,
            vec![off(session, "menu-attach-none", Icon::EditLink)],
        ),
        off(session, "menu-detach", Icon::EditUnlink),
        branch(
            session,
            "menu-convert",
            Icon::ViewRefresh,
            vec![
                off(session, "menu-convert-empty", Icon::EntityEmpty),
                off(session, "menu-convert-cube", Icon::EntityCube),
                off(session, "menu-convert-sphere", Icon::EntitySphere),
                off(session, "menu-convert-plane", Icon::EntityPlane),
            ],
        ),
        branch(
            session,
            "menu-replace",
            Icon::EntityCube,
            vec![off(session, "menu-replace-browse", Icon::FileOpen)],
        ),
        branch(
            session,
            "menu-move-folder",
            Icon::EntityFolder,
            vec![off(session, "menu-folder-none", Icon::EntityFolder)],
        ),
        off(session, "menu-pin", Icon::EditPin),
        off(session, "menu-snap-view", Icon::ToolCamera),
        off(session, "menu-snap-object", Icon::ToolCamera),
        off(session, "menu-pilot", Icon::Play),
    ]
}

fn shape_branch(session: &EditorSession) -> ContextMenuEntry {
    let polygons = seal_editor_core::shape_ops::selected_polygon_count(session);
    let item = |label: &str, id: &str, ready: bool| {
        let live = session.commands.get(id).is_some();
        ContextMenuEntry::Row(ContextMenuRow {
            label: text(session, label),
            icon: Icon::EntityPlane,
            shortcut: String::new(),
            enabled: live && ready,
            reason: if !ready { text(session, "menu-needs-selection") } else { String::new() },
            command: if live && ready { Some((id.to_string(), Vec::new())) } else { None },
            children: Vec::new(),
        })
    };
    branch(
        session,
        "menu-shape",
        Icon::EntityPlane,
        vec![
            item("menu-shape-merge", "shape.merge", polygons >= 2),
            item("menu-shape-subtract", "shape.subtract", polygons >= 2),
            item("menu-shape-intersect", "shape.intersect", polygons >= 2),
            item("menu-shape-offset", "shape.offset", polygons >= 1),
            item(
                "menu-sprite-collider",
                "sprite.collider_from_sprite",
                session.primary_selection().is_some(),
            ),
        ],
    )
}

fn place_shapes(session: &EditorSession) -> Vec<ContextMenuEntry> {
    ["cube", "sphere", "plane", "sprite_shape"]
        .into_iter()
        .map(|kind| {
            let enabled = session.commands.get("scene.add").is_some();
            ContextMenuEntry::Row(ContextMenuRow {
                label: text(session, &format!("place-{kind}")),
                icon: match kind {
                    "sphere" => Icon::EntitySphere,
                    "plane" | "sprite_shape" => Icon::EntityPlane,
                    _ => Icon::EntityCube,
                },
                shortcut: String::new(),
                enabled,
                reason: text(session, "menu-unavailable"),
                command: if enabled {
                    Some(("scene.add".into(), vec![("kind".into(), kind.into())]))
                } else {
                    None
                },
                children: Vec::new(),
            })
        })
        .collect()
}

/// Empty viewport: Place Entity, Play From Here, Go Here.
pub fn empty_entries(session: &EditorSession) -> Vec<ContextMenuEntry> {
    let go_enabled = session.commands.get("viewport.go-here").is_some();
    vec![
        branch(
            session,
            "viewport-place-entity",
            Icon::ViewPlus,
            vec![
                branch(session, "toolbar-add-shapes", Icon::EntityCube, place_shapes(session)),
                ContextMenuEntry::Row(ContextMenuRow {
                    label: text(session, "place-light"),
                    icon: Icon::EntityLightDirectional,
                    shortcut: String::new(),
                    enabled: session.commands.get("scene.add").is_some(),
                    reason: text(session, "menu-unavailable"),
                    command: Some(("scene.add".into(), vec![("kind".into(), "light".into())])),
                    children: Vec::new(),
                }),
            ],
        ),
        off_reason(session, "play-from-here", Icon::Play, "menu-unavailable"),
        ContextMenuEntry::Row(ContextMenuRow {
            label: text(session, "viewport-go-here"),
            icon: Icon::ToolCamera,
            shortcut: shortcut(session, "viewport.go-here"),
            enabled: go_enabled,
            reason: text(session, "menu-unavailable"),
            command: if go_enabled { Some(("viewport.go-here".into(), Vec::new())) } else { None },
            children: Vec::new(),
        }),
    ]
}
