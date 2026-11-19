//! Inspector rows and the viewport mark for a sprite shape.

use seal_editor_core::{sprite_shape, EditorSession};
use seal_ui::{ContextMenuEntry, PanelFrame, SpriteShapeMark};

use crate::{
    command, inspector_field_modified, inspector_reset_tip, inspector_text_field_edit,
    shared_field, PanelEvent,
};

fn project(
    entity: &seal_editor_core::Entity,
    local: [f32; 2],
    axes: [usize; 2],
    flip: [f32; 2],
) -> [f32; 2] {
    sprite_shape::project(entity, local, axes, flip)
}

/// The shape on the view plane, when this object is a sprite shape.
pub(crate) fn view_mark(
    session: &EditorSession,
    entity: &seal_editor_core::Entity,
    axes: [usize; 2],
    flip: [f32; 2],
    editing: bool,
) -> Option<SpriteShapeMark> {
    if !entity.sprite_shape.on || entity.sprite_shape.points.0.len() < 2 {
        return None;
    }
    let shape = &entity.sprite_shape;
    let (poly, _bands, fill, edge) = sprite_shape::build(shape);
    let root = session.project.as_ref().map(|p| p.root.as_path());
    let controls = shape
        .points
        .0
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let at = project(entity, p.pos, axes, flip);
            let tip = sprite_shape::handle_tip_view(entity, i, flip).unwrap_or(at);
            seal_ui::SplineControl { at, smooth: p.smooth, tip }
        })
        .collect();
    let outline: Vec<[f32; 2]> =
        poly.points.iter().copied().map(|p| project(entity, p, axes, flip)).collect();
    let fill = fill
        .iter()
        .map(|tri| {
            let pos = tri.pos.map(|p| project(entity, p, axes, flip));
            (pos, tri.uv)
        })
        .collect();
    let edge = edge
        .iter()
        .map(|tri| {
            let pos = tri.pos.map(|p| project(entity, p, axes, flip));
            (tri.band as u8, pos, tri.uv)
        })
        .collect();
    let picture = |rel: &str| sprite_shape::picture_path(root, rel);
    let tint = shape.tint.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    Some(SpriteShapeMark {
        controls,
        outline,
        seg: poly.seg,
        fill,
        edge,
        fill_image: picture(&shape.fill),
        edge_image: [
            picture(&shape.edge_top),
            picture(&shape.edge_side),
            picture(&shape.edge_bottom),
        ],
        tint,
        editing,
        smooth_label: session.messages.get("ss-point-smooth"),
        corner_label: session.messages.get("ss-point-corner"),
    })
}

/// Rows for the Sprite Shape section.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rows(
    frame: &mut PanelFrame,
    session: &EditorSession,
    targets: &[&seal_editor_core::MockNode],
    set_target: (&str, &str),
    reset_target: (&str, &str),
    prop_menu: &[ContextMenuEntry],
    prop_menu_pick: &mut Option<seal_ui::ContextMenuPick>,
) -> Option<PanelEvent> {
    let _ = (
        "inspector-ss-points-invalid",
        "inspector-ss-scale-invalid",
        "inspector-ss-angles-invalid",
        "inspector-ss-thickness-invalid",
        "inspector-ss-tolerance-invalid",
    );
    let text = |key: &str| session.messages.get(key);
    let mut event = None;
    for &key in sprite_shape::KEYS {
        if !targets.iter().all(|node| node.fields.iter().any(|(k, _)| k == key)) {
            continue;
        }
        let Some(current) = shared_field(targets, key) else { continue };
        let label = text(&format!("inspector-{}", key.replace('_', "-")));
        let modified = inspector_field_modified(targets, key);
        let tip_owned = modified.then(|| inspector_reset_tip(session, key));
        if key == "ss_corner" {
            let options: Vec<(&str, String)> = sprite_shape::CORNERS
                .iter()
                .map(|v| (*v, text(&format!("ss-corner-{v}"))))
                .collect();
            let (picked, reset) = frame.choice_row(
                &label,
                &current,
                &options,
                modified,
                tip_owned.as_deref(),
                None,
                Some(prop_menu),
                prop_menu_pick,
                Some(key),
            );
            if let Some(picked) = picked {
                event = Some(command(
                    "inspector.set",
                    &[set_target, ("field", key), ("value", picked.as_str())],
                ));
            } else if reset {
                event = Some(command("inspector.reset_field", &[reset_target, ("field", key)]));
            }
            continue;
        }
        if matches!(key, "ss_closed" | "ss_collider") {
            let on = current == "yes";
            let check =
                if on { seal_ui::PropertyCheckbox::On } else { seal_ui::PropertyCheckbox::Off };
            if let Some(next) = frame.property_checkbox_row(
                &label,
                check,
                modified,
                None,
                Some(prop_menu),
                prop_menu_pick,
                Some(key),
            ) {
                let value =
                    if matches!(next, seal_ui::PropertyCheckbox::On) { "yes" } else { "no" };
                event =
                    Some(command("inspector.set", &[set_target, ("field", key), ("value", value)]));
            }
            continue;
        }
        let mut draft = current;
        let field_error = sprite_shape::check(key, &draft).map(|msg| session.messages.get(msg));
        let (changed, reset, commit_focus) = frame.property_validated_row(
            &label,
            &mut draft,
            field_error.as_deref(),
            modified,
            tip_owned.as_deref(),
            None,
            Some(prop_menu),
            prop_menu_pick,
            Some(key),
        );
        let editing =
            session.models.inspector_field_edit.as_ref().is_some_and(|edit| edit.field == key);
        if changed && sprite_shape::check(key, &draft).is_none() {
            event = Some(inspector_text_field_edit(
                session,
                set_target,
                key,
                &draft,
                true,
                commit_focus,
            ));
        } else if commit_focus && editing {
            event = Some(inspector_text_field_edit(session, set_target, key, &draft, false, true));
        } else if reset {
            event = Some(command("inspector.reset_field", &[reset_target, ("field", key)]));
        }
    }
    event
}
