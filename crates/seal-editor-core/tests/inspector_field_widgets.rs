//! Inspector field metadata, enum sanitise, flags text, clamp, and mixed values.

use seal_editor_core::{
    clamp_number_field, collision_layer_labels, field_spec, sanitize_enum_field, InspectorFieldKind,
};
use seal_editor_core::{EditorSession, Layout};

#[test]
fn metadata_lists_every_declared_field() {
    for key in ["shape", "motion_mode", "phys_layer", "opacity", "fov", "light2d"] {
        let spec = field_spec(key).expect(key);
        match key {
            "phys_layer" => assert_eq!(spec.kind, InspectorFieldKind::Flags),
            "opacity" => assert_eq!(spec.kind, InspectorFieldKind::Number),
            _ => {
                assert!(matches!(spec.kind, InspectorFieldKind::Enum | InspectorFieldKind::Number))
            }
        }
    }
}

#[test]
fn enum_rejects_unknown_and_keeps_default() {
    let (value, err) = sanitize_enum_field("light2d", "laser");
    assert_eq!(err, Some("inspector-enum-unknown"));
    assert!(value.is_empty());
    let (value, _) = sanitize_enum_field("phys_shape", "star");
    assert_eq!(value, "box");
}

#[test]
fn clamp_number_fields() {
    assert_eq!(clamp_number_field("opacity", "1.5"), "1");
    assert_eq!(clamp_number_field("fov", "200"), "179");
}

#[test]
fn flags_layers_all_none_and_bits() {
    use seal_editor_core::phys2d::{layers_text, parse_layers};
    assert_eq!(parse_layers(&layers_text(u32::MAX)), Some(u32::MAX));
    assert_eq!(parse_layers(&layers_text(0)), Some(0));
    assert_eq!(parse_layers("1, 3"), Some(0b101));
}

#[test]
fn mixed_selection_is_not_shared() {
    let mut session = EditorSession::new(Layout::default());
    let a = session.world.entities[2].id.clone();
    let b = session.world.entities[3].id.clone();
    session.models.set_field(&a, "opacity", "0.5");
    session.models.set_field(&b, "opacity", "0.8");
    session.mirror_node(&a);
    session.mirror_node(&b);
    let na = session.models.nodes.iter().find(|n| n.id == a).unwrap();
    let nb = session.models.nodes.iter().find(|n| n.id == b).unwrap();
    assert_ne!(
        na.fields.iter().find(|(k, _)| k == "opacity").map(|(_, v)| v.as_str()),
        nb.fields.iter().find(|(k, _)| k == "opacity").map(|(_, v)| v.as_str())
    );
}

#[test]
fn reset_property_command_restores_default() {
    let mut session = EditorSession::new(Layout::default());
    let id = session.world.entities[2].id.clone();
    session.models.set_field(&id, "opacity", "0.25");
    session.mirror_node(&id);
    session
        .dispatch(
            "inspector.property_reset",
            &seal_editor_core::CommandArgs::new(&[("id", &id), ("field", "opacity")]),
        )
        .expect("reset");
    let node = session.models.nodes.iter().find(|n| n.id == id).unwrap();
    assert_eq!(
        node.fields.iter().find(|(k, _)| k == "opacity").map(|(_, v)| v.as_str()),
        Some("1")
    );
}

#[test]
fn collision_layer_names_from_settings_text() {
    let labels = collision_layer_labels(Some("Player;Terrain;;;Hazard"));
    assert_eq!(labels[0], "Player");
    assert_eq!(labels[4], "Hazard");
    assert_eq!(labels[3], "Layer 4");
}

#[test]
fn sorting_layer_names_match_collision_format() {
    let labels = seal_editor_core::sorting_layer_labels(Some("FG;BG"));
    assert_eq!(labels[0], "FG");
    assert_eq!(labels[1], "BG");
}

#[test]
fn setting_a_layer_name_changes_the_dropdown_label() {
    let mut session = EditorSession::new(Layout::default());
    session
        .models
        .set_setting(seal_editor_core::COLLISION_LAYER_SETTING, "Hero;;Pit", "project")
        .expect("setting");
    let stored = seal_editor_core::project_collision_layer_names(&session.models.settings);
    let labels = collision_layer_labels(stored.as_deref());
    assert_eq!(labels[0], "Hero");
    assert_eq!(labels[1], "Layer 2");
    assert_eq!(labels[2], "Pit");
}

#[test]
fn unknown_enum_text_is_logged_and_keeps_the_default() {
    let mut session = EditorSession::new(Layout::default());
    let id = session.world.entities[2].id.clone();
    session.world.entity_mut(&id).unwrap().light2d.kind = "laser".into();
    session.adopt_world();
    let node = session.models.nodes.iter().find(|n| n.id == id).unwrap();
    let light = node.fields.iter().find(|(k, _)| k == "light2d").map(|(_, v)| v.as_str());
    assert_eq!(light, Some(""));
    let lines = seal_editor_core::collect_log_lines(0);
    assert!(
        lines.iter().any(|line| line.message.contains("laser") && line.message.contains("light2d")),
        "{lines:?}"
    );
}

#[test]
fn a_number_drag_is_one_undo_step() {
    let mut session = EditorSession::new(Layout::default());
    let id = session.world.entities[2].id.clone();
    session.models.set_field(&id, "opacity", "1");
    let before = session.past.len();
    let args = seal_editor_core::CommandArgs::new(&[("id", &id), ("field", "opacity")]);
    session.dispatch("inspector.field_edit_begin", &args).unwrap();
    session
        .dispatch(
            "inspector.set",
            &seal_editor_core::CommandArgs::new(&[
                ("id", &id),
                ("field", "opacity"),
                ("value", "0.2"),
                ("preview", "1"),
            ]),
        )
        .unwrap();
    session
        .dispatch(
            "inspector.set",
            &seal_editor_core::CommandArgs::new(&[
                ("id", &id),
                ("field", "opacity"),
                ("value", "0.4"),
                ("preview", "1"),
            ]),
        )
        .unwrap();
    session
        .dispatch(
            "inspector.field_edit_end",
            &seal_editor_core::CommandArgs::new(&[
                ("id", &id),
                ("field", "opacity"),
                ("value", "0.4"),
            ]),
        )
        .unwrap();
    assert_eq!(session.past.len(), before + 1);
    session.dispatch("edit.undo", &seal_editor_core::CommandArgs::default()).unwrap();
    let node = session.models.nodes.iter().find(|n| n.id == id).unwrap();
    assert_eq!(
        node.fields.iter().find(|(k, _)| k == "opacity").map(|(_, v)| v.as_str()),
        Some("1")
    );
}

#[test]
fn reset_and_changed_marker_cover_every_table_field() {
    use seal_editor_core::{changed_from_default, inspector_field_default};
    let mut session = EditorSession::new(Layout::default());
    let id = session.world.entities[2].id.clone();
    for spec in seal_editor_core::INSPECTOR_FIELD_SPECS {
        let default = inspector_field_default(spec.key);
        let changed = non_default(spec.kind, spec.key, default, spec);
        assert!(changed_from_default(spec.key, &changed), "{}", spec.key);
        assert!(!changed_from_default(spec.key, default), "{}", spec.key);
        session.models.set_field(&id, spec.key, &changed);
        session
            .dispatch(
                "inspector.property_reset",
                &seal_editor_core::CommandArgs::new(&[("id", &id), ("field", spec.key)]),
            )
            .unwrap_or_else(|err| panic!("{}: {err}", spec.key));
        let node = session.models.nodes.iter().find(|n| n.id == id).unwrap();
        assert_eq!(
            node.fields.iter().find(|(k, _)| k == spec.key).map(|(_, v)| v.as_str()),
            Some(default),
            "{}",
            spec.key
        );
    }
}

fn non_default(
    kind: seal_editor_core::InspectorFieldKind,
    key: &str,
    default: &str,
    spec: &seal_editor_core::InspectorFieldSpec,
) -> String {
    let text = match kind {
        InspectorFieldKind::Enum => spec
            .enum_values
            .iter()
            .copied()
            .find(|value| *value != default)
            .unwrap_or("other")
            .to_string(),
        InspectorFieldKind::YesNo => {
            if default == "yes" {
                "no".into()
            } else {
                "yes".into()
            }
        }
        InspectorFieldKind::Flags => {
            if default == "1" {
                "2".into()
            } else {
                "1".into()
            }
        }
        InspectorFieldKind::Number => {
            if let Some(max) = spec.max {
                let high = seal_editor_core::clamp_number_field(key, &format!("{max}"));
                if high != default {
                    high
                } else if let Some(min) = spec.min {
                    seal_editor_core::clamp_number_field(key, &format!("{min}"))
                } else {
                    "7".into()
                }
            } else {
                "45".into()
            }
        }
    };
    assert_ne!(text, default, "{key}");
    text
}
