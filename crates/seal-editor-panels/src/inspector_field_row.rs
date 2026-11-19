//! Metadata-driven inspector rows: slider, number box, unit, clamp, and one undo step per drag.

use seal_editor_core::{
    changed_from_default, clamp_number_field, field_spec, metadata_row, unit_message_key,
    MetadataRow,
};
use seal_ui::{ContextMenuEntry, ContextMenuPick, NumberFieldSpec, PanelFrame};

use crate::inspector_panel;
use crate::{command, PanelEvent};

/// Number keys painted by [`paint_metadata_number`] (the rotation triple is separate).
pub(crate) const PAINTED_NUMBERS: &[&str] = &[
    "roughness",
    "metal",
    "opacity",
    "fov",
    "near",
    "far",
    "light_range",
    "light_cone",
    "cam_smoothing",
    "cam_zoom",
    "cam_pixels",
    "cam_drag_left",
    "cam_drag_right",
    "cam_drag_top",
    "cam_drag_bottom",
    "frames",
    "fps",
    "intensity",
    "mat_amount",
    "mat_speed",
];

/// Closed sets painted as dropdowns.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const PAINTED_DROPDOWNS: &[&str] = &[
    "shape",
    "motion",
    "motion_mode",
    "light2d",
    "phys_body",
    "phys_shape",
    "phys_surface",
    "joint",
    "shape2d",
    "follow_mode",
    "material2d",
    "role",
    "sort_mode",
    "slice_mode",
];

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const PAINTED_FLAGS: &[&str] = &["phys_layer", "phys_mask"];

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const PAINTED_YES_NO: &[&str] =
    &["flip_x", "flip_y", "repeat_x", "light_shadows", "camera2d"];

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const PAINTED_ROTATION: &[&str] = &["rx", "ry", "rz"];

/// `Some(event)` when `key` is a metadata number (including a no-op event).
/// `None` when this row is not a slider/number field.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_metadata_number(
    frame: &mut PanelFrame,
    session: &seal_editor_core::EditorSession,
    targets: &[&seal_editor_core::MockNode],
    key: &str,
    label: &str,
    set_target: (&str, &str),
    reset_target: (&str, &str),
    prop_menu: &[ContextMenuEntry],
    prop_menu_pick: &mut Option<ContextMenuPick>,
) -> Option<PanelEvent> {
    if !PAINTED_NUMBERS.contains(&key) {
        return None;
    }
    let MetadataRow::Number(widget) = metadata_row(key)? else {
        return None;
    };
    let text = |id: &str| session.messages.get(id);
    let mixed_label = text("inspector-mixed");
    let modified = targets.iter().any(|node| changed_from_default(key, crate::field(node, key)));
    let tip_owned = modified.then(|| {
        session.messages.format(
            "inspector-reset-default",
            &[("value", seal_editor_core::inspector_field_default(key))],
        )
    });
    let shared = crate::shared_field(targets, key);
    if shared.is_none() && targets.len() > 1 {
        let mut draft = mixed_label;
        let _ = frame.property_validated_row(
            label,
            &mut draft,
            None,
            modified,
            tip_owned.as_deref(),
            None,
            Some(prop_menu),
            prop_menu_pick,
            Some(key),
        );
        return Some(PanelEvent::None);
    }
    let raw = shared.unwrap_or_else(|| seal_editor_core::inspector_field_default(key).to_string());
    let fallback = seal_editor_core::inspector_field_default(key).parse::<f32>().unwrap_or(0.0);
    let mut value = raw.parse::<f32>().unwrap_or(fallback);
    if let Some(min) = widget.min {
        value = value.max(min);
    }
    if let Some(max) = widget.max {
        value = value.min(max);
    }
    let unit_suffix = unit_message_key(widget.unit).map(text);
    let spec = NumberFieldSpec {
        min: widget.min,
        max: widget.max,
        unit_suffix,
        range_bar: widget.range_bar,
        initial_text: None,
        level_led: key == "intensity",
    };
    let (changed, reset, drag) = frame.number_field_row(
        label,
        &mut value,
        widget.step.max(0.001),
        modified,
        tip_owned.as_deref(),
        None,
        Some(prop_menu),
        prop_menu_pick,
        &spec,
        Some(key),
    );
    if changed || drag.drag_started || drag.drag_stopped || drag.escape_cancel || drag.commit_focus
    {
        let stored = clamp_number_field(key, &value.to_string());
        return Some(inspector_panel::inspector_field_edit_event(
            session,
            set_target,
            key,
            &stored,
            changed,
            drag.drag_started,
            drag.drag_stopped,
            drag.escape_cancel,
            drag.commit_focus,
        ));
    }
    if reset {
        return Some(command("inspector.reset_field", &[reset_target, ("field", key)]));
    }
    Some(PanelEvent::None)
}

/// Degree suffix for a rotation axis, from the metadata table.
pub(crate) fn rotation_unit_key(axis: &str) -> Option<&'static str> {
    field_spec(axis).and_then(|spec| unit_message_key(spec.unit))
}

#[cfg(test)]
mod tests {
    use seal_editor_core::{InspectorFieldKind, INSPECTOR_FIELD_SPECS};

    use super::*;

    #[test]
    fn every_table_field_is_wired_to_a_metadata_row() {
        let mut wired: Vec<&str> = PAINTED_NUMBERS
            .iter()
            .chain(PAINTED_DROPDOWNS)
            .chain(PAINTED_FLAGS)
            .chain(PAINTED_YES_NO)
            .chain(PAINTED_ROTATION)
            .copied()
            .collect();
        wired.sort_unstable();
        let mut table: Vec<&str> = INSPECTOR_FIELD_SPECS.iter().map(|spec| spec.key).collect();
        table.sort_unstable();
        assert_eq!(wired, table);
        let source = include_str!("lib.rs");
        assert!(source.contains("paint_metadata_number"));
        for key in PAINTED_NUMBERS {
            if matches!(*key, "mat_amount" | "mat_speed") {
                assert!(
                    seal_editor_core::material2d::KEYS.contains(key),
                    "{key} is in the table but the material rows never pass it"
                );
                continue;
            }
            assert!(
                source.contains(&format!("\"{key}\"")),
                "{key} is in the table but no inspector row names it"
            );
        }
        for key in PAINTED_DROPDOWNS
            .iter()
            .chain(PAINTED_FLAGS)
            .chain(PAINTED_YES_NO)
            .chain(PAINTED_ROTATION)
        {
            assert!(
                source.contains(&format!("\"{key}\"")),
                "{key} is in the table but no inspector row names it"
            );
        }
        for spec in INSPECTOR_FIELD_SPECS {
            match spec.kind {
                InspectorFieldKind::Number if PAINTED_ROTATION.contains(&spec.key) => {
                    assert_eq!(rotation_unit_key(spec.key), Some("inspector-unit-degrees"));
                }
                InspectorFieldKind::Number => {
                    assert!(PAINTED_NUMBERS.contains(&spec.key), "{}", spec.key);
                    assert!(
                        matches!(metadata_row(spec.key), Some(MetadataRow::Number(row)) if row.range_bar == spec.range_bar)
                    );
                }
                InspectorFieldKind::Enum => assert!(PAINTED_DROPDOWNS.contains(&spec.key)),
                InspectorFieldKind::Flags => assert!(PAINTED_FLAGS.contains(&spec.key)),
                InspectorFieldKind::YesNo => assert!(PAINTED_YES_NO.contains(&spec.key)),
            }
        }
    }
}
