//! Inspector rows for a mesh base colour texture.

use seal_editor_core::{EditorSession, TextureAsset, TextureAssetKind};
use seal_ui::PanelFrame;

use crate::{command, inspector_pickers, shared_field, PanelEvent};

pub(crate) fn paint(
    frame: &mut PanelFrame,
    session: &EditorSession,
    targets: &[&seal_editor_core::MockNode],
    set_target: (&str, &str),
    event: &mut PanelEvent,
) {
    let text = |key: &str| session.messages.get(key);
    let texture = shared_field(targets, "mesh_texture").unwrap_or_default();
    let mixed = shared_field(targets, "mesh_texture").is_none() && targets.len() > 1;
    match inspector_pickers::paint_asset_row(
        frame,
        session,
        "mesh_texture",
        "mesh-texture",
        &text("inspector-mesh-texture"),
        &texture,
        mixed,
    ) {
        inspector_pickers::AssetRow::Set(path) => {
            *event = command(
                "inspector.set",
                &[set_target, ("field", "mesh_texture"), ("value", path.as_str())],
            );
        }
        inspector_pickers::AssetRow::Clear => {
            *event =
                command("inspector.set", &[set_target, ("field", "mesh_texture"), ("value", "")]);
        }
        inspector_pickers::AssetRow::Show => {
            *event = command("picker.show_asset", &[("path", texture.as_str())]);
        }
        inspector_pickers::AssetRow::Idle => {}
    }
    let kind = texture_kind(session, &texture);
    if kind == Some(TextureAssetKind::Array2d) {
        number_row(frame, &text("inspector-mesh-layer"), "mesh_layer", targets, set_target, event);
    }
    if kind == Some(TextureAssetKind::Volume3d) {
        number_row(frame, &text("inspector-mesh-slice"), "mesh_slice", targets, set_target, event);
    }
    if !texture.is_empty() {
        number_row(
            frame,
            &text("inspector-mesh-tile-u"),
            "mesh_tile_u",
            targets,
            set_target,
            event,
        );
        number_row(
            frame,
            &text("inspector-mesh-tile-v"),
            "mesh_tile_v",
            targets,
            set_target,
            event,
        );
        frame.muted(&text("inspector-mesh-texture-hint"));
    }
}

fn number_row(
    frame: &mut PanelFrame,
    label: &str,
    key: &str,
    targets: &[&seal_editor_core::MockNode],
    set_target: (&str, &str),
    event: &mut PanelEvent,
) {
    let mut menu_pick = None;
    let shared = shared_field(targets, key);
    let mut draft = shared.clone().unwrap_or_else(|| "—".into());
    let modified = shared.as_deref().is_some_and(|value| {
        !value.is_empty() && value != seal_editor_core::inspector_field_default(key)
    });
    let (changed, _, commit) = frame.property_validated_row(
        label,
        &mut draft,
        None,
        modified,
        None,
        None,
        None,
        &mut menu_pick,
        None,
    );
    if (changed || commit) && shared.is_some() && draft.parse::<f32>().is_ok() {
        *event = command("inspector.set", &[set_target, ("field", key), ("value", draft.trim())]);
    }
}

fn texture_kind(session: &EditorSession, path: &str) -> Option<TextureAssetKind> {
    if !path.rsplit('.').next().is_some_and(|ext| ext.eq_ignore_ascii_case("texture")) {
        return None;
    }
    let root = session.project.as_ref().map(|project| project.root.clone())?;
    let text = std::fs::read_to_string(root.join(path)).ok()?;
    TextureAsset::parse(&text).ok().map(|asset| asset.kind)
}
