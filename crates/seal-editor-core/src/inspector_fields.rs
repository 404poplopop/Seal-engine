//! Inspector field metadata: kind, bounds, units, and enum sets in one table.

use crate::models::inspector_field_default;

/// How a row in the inspector edits a field value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InspectorFieldKind {
    Enum,
    Flags,
    Number,
    YesNo,
}

/// Unit suffix key for localised labels (`inspector-unit-*` in en.ftl).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum InspectorUnit {
    #[default]
    None,
    Degrees,
    Seconds,
    Pixels,
    Lux,
    Unit,
}

/// One inspector field row definition.
#[derive(Clone, Copy, Debug)]
pub struct InspectorFieldSpec {
    pub key: &'static str,
    pub kind: InspectorFieldKind,
    pub min: Option<f32>,
    pub max: Option<f32>,
    pub step: f32,
    pub unit: InspectorUnit,
    pub range_bar: bool,
    pub enum_values: &'static [&'static str],
}

const SHAPE_VALUES: &[&str] = &["none", "cube", "sphere", "cylinder", "cone", "capsule", "plane"];
const MOTION_MODE: &[&str] = &["static", "mixed", "dynamic"];
const MOTION: &[&str] = &["static", "dynamic"];
const LIGHT2D: &[&str] = &["", "global", "point", "spot"];
const YES_NO: &[&str] = &["no", "yes"];
const GAME_ROLE: &[&str] = &["", "player", "ground", "coin", "goal"];

/// Every field this module knows how to treat as enum, flags, or bounded number.
pub static INSPECTOR_FIELD_SPECS: &[InspectorFieldSpec] = &[
    InspectorFieldSpec {
        key: "shape",
        kind: InspectorFieldKind::Enum,
        min: None,
        max: None,
        step: 0.0,
        unit: InspectorUnit::None,
        range_bar: false,
        enum_values: SHAPE_VALUES,
    },
    InspectorFieldSpec {
        key: "motion",
        kind: InspectorFieldKind::Enum,
        min: None,
        max: None,
        step: 0.0,
        unit: InspectorUnit::None,
        range_bar: false,
        enum_values: MOTION,
    },
    InspectorFieldSpec {
        key: "motion_mode",
        kind: InspectorFieldKind::Enum,
        min: None,
        max: None,
        step: 0.0,
        unit: InspectorUnit::None,
        range_bar: false,
        enum_values: MOTION_MODE,
    },
    InspectorFieldSpec {
        key: "light2d",
        kind: InspectorFieldKind::Enum,
        min: None,
        max: None,
        step: 0.0,
        unit: InspectorUnit::None,
        range_bar: false,
        enum_values: LIGHT2D,
    },
    InspectorFieldSpec {
        key: "phys_body",
        kind: InspectorFieldKind::Enum,
        min: None,
        max: None,
        step: 0.0,
        unit: InspectorUnit::None,
        range_bar: false,
        enum_values: seal_world::phys2d::BODIES,
    },
    InspectorFieldSpec {
        key: "phys_shape",
        kind: InspectorFieldKind::Enum,
        min: None,
        max: None,
        step: 0.0,
        unit: InspectorUnit::None,
        range_bar: false,
        enum_values: seal_world::phys2d::SHAPES,
    },
    InspectorFieldSpec {
        key: "phys_surface",
        kind: InspectorFieldKind::Enum,
        min: None,
        max: None,
        step: 0.0,
        unit: InspectorUnit::None,
        range_bar: false,
        enum_values: &["custom", "ice", "rubber", "metal", "wood", "stone", "bouncy"],
    },
    InspectorFieldSpec {
        key: "joint",
        kind: InspectorFieldKind::Enum,
        min: None,
        max: None,
        step: 0.0,
        unit: InspectorUnit::None,
        range_bar: false,
        enum_values: &["none", "fixed", "hinge", "slider", "distance", "rope", "spring", "wheel"],
    },
    InspectorFieldSpec {
        key: "shape2d",
        kind: InspectorFieldKind::Enum,
        min: None,
        max: None,
        step: 0.0,
        unit: InspectorUnit::None,
        range_bar: false,
        enum_values: seal_world::shape2d::KINDS,
    },
    InspectorFieldSpec {
        key: "follow_mode",
        kind: InspectorFieldKind::Enum,
        min: None,
        max: None,
        step: 0.0,
        unit: InspectorUnit::None,
        range_bar: false,
        enum_values: seal_world::shape2d::MODES,
    },
    InspectorFieldSpec {
        key: "material2d",
        kind: InspectorFieldKind::Enum,
        min: None,
        max: None,
        step: 0.0,
        unit: InspectorUnit::None,
        range_bar: false,
        enum_values: seal_world::material2d::EFFECTS,
    },
    InspectorFieldSpec {
        key: "mat_amount",
        kind: InspectorFieldKind::Number,
        min: Some(0.0),
        max: Some(1.0),
        step: 0.01,
        unit: InspectorUnit::Unit,
        range_bar: true,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "mat_speed",
        kind: InspectorFieldKind::Number,
        min: Some(0.0),
        max: Some(100.0),
        step: 0.1,
        unit: InspectorUnit::Unit,
        range_bar: true,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "role",
        kind: InspectorFieldKind::Enum,
        min: None,
        max: None,
        step: 0.0,
        unit: InspectorUnit::None,
        range_bar: false,
        enum_values: GAME_ROLE,
    },
    InspectorFieldSpec {
        key: "flip_x",
        kind: InspectorFieldKind::YesNo,
        min: None,
        max: None,
        step: 0.0,
        unit: InspectorUnit::None,
        range_bar: false,
        enum_values: YES_NO,
    },
    InspectorFieldSpec {
        key: "flip_y",
        kind: InspectorFieldKind::YesNo,
        min: None,
        max: None,
        step: 0.0,
        unit: InspectorUnit::None,
        range_bar: false,
        enum_values: YES_NO,
    },
    InspectorFieldSpec {
        key: "repeat_x",
        kind: InspectorFieldKind::YesNo,
        min: None,
        max: None,
        step: 0.0,
        unit: InspectorUnit::None,
        range_bar: false,
        enum_values: YES_NO,
    },
    InspectorFieldSpec {
        key: "light_shadows",
        kind: InspectorFieldKind::YesNo,
        min: None,
        max: None,
        step: 0.0,
        unit: InspectorUnit::None,
        range_bar: false,
        enum_values: YES_NO,
    },
    InspectorFieldSpec {
        key: "camera2d",
        kind: InspectorFieldKind::YesNo,
        min: None,
        max: None,
        step: 0.0,
        unit: InspectorUnit::None,
        range_bar: false,
        enum_values: YES_NO,
    },
    InspectorFieldSpec {
        key: "phys_layer",
        kind: InspectorFieldKind::Flags,
        min: None,
        max: None,
        step: 0.0,
        unit: InspectorUnit::None,
        range_bar: false,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "phys_mask",
        kind: InspectorFieldKind::Flags,
        min: None,
        max: None,
        step: 0.0,
        unit: InspectorUnit::None,
        range_bar: false,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "roughness",
        kind: InspectorFieldKind::Number,
        min: Some(0.0),
        max: Some(1.0),
        step: 0.01,
        unit: InspectorUnit::Unit,
        range_bar: true,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "metal",
        kind: InspectorFieldKind::Number,
        min: Some(0.0),
        max: Some(1.0),
        step: 0.01,
        unit: InspectorUnit::Unit,
        range_bar: true,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "opacity",
        kind: InspectorFieldKind::Number,
        min: Some(0.0),
        max: Some(1.0),
        step: 0.01,
        unit: InspectorUnit::Unit,
        range_bar: true,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "fov",
        kind: InspectorFieldKind::Number,
        min: Some(1.0),
        max: Some(179.0),
        step: 1.0,
        unit: InspectorUnit::Degrees,
        range_bar: true,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "near",
        kind: InspectorFieldKind::Number,
        min: Some(0.001),
        max: Some(10.0),
        step: 0.01,
        unit: InspectorUnit::Unit,
        range_bar: false,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "far",
        kind: InspectorFieldKind::Number,
        min: Some(10.0),
        max: Some(100_000.0),
        step: 1.0,
        unit: InspectorUnit::Unit,
        range_bar: false,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "light_range",
        kind: InspectorFieldKind::Number,
        min: Some(0.01),
        max: Some(10_000.0),
        step: 0.1,
        unit: InspectorUnit::Unit,
        range_bar: true,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "light_cone",
        kind: InspectorFieldKind::Number,
        min: Some(1.0),
        max: Some(360.0),
        step: 1.0,
        unit: InspectorUnit::Degrees,
        range_bar: true,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "cam_smoothing",
        kind: InspectorFieldKind::Number,
        min: Some(0.0),
        max: Some(10.0),
        step: 0.01,
        unit: InspectorUnit::Unit,
        range_bar: true,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "cam_zoom",
        kind: InspectorFieldKind::Number,
        min: Some(0.05),
        max: Some(50.0),
        step: 0.05,
        unit: InspectorUnit::Unit,
        range_bar: true,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "cam_pixels",
        kind: InspectorFieldKind::Number,
        min: Some(0.0),
        max: Some(1024.0),
        step: 1.0,
        unit: InspectorUnit::Pixels,
        range_bar: false,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "cam_drag_left",
        kind: InspectorFieldKind::Number,
        min: Some(0.0),
        max: Some(1.0),
        step: 0.01,
        unit: InspectorUnit::None,
        range_bar: true,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "cam_drag_right",
        kind: InspectorFieldKind::Number,
        min: Some(0.0),
        max: Some(1.0),
        step: 0.01,
        unit: InspectorUnit::None,
        range_bar: true,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "cam_drag_top",
        kind: InspectorFieldKind::Number,
        min: Some(0.0),
        max: Some(1.0),
        step: 0.01,
        unit: InspectorUnit::None,
        range_bar: true,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "cam_drag_bottom",
        kind: InspectorFieldKind::Number,
        min: Some(0.0),
        max: Some(1.0),
        step: 0.01,
        unit: InspectorUnit::None,
        range_bar: true,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "rx",
        kind: InspectorFieldKind::Number,
        min: None,
        max: None,
        step: 1.0,
        unit: InspectorUnit::Degrees,
        range_bar: false,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "ry",
        kind: InspectorFieldKind::Number,
        min: None,
        max: None,
        step: 1.0,
        unit: InspectorUnit::Degrees,
        range_bar: false,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "rz",
        kind: InspectorFieldKind::Number,
        min: None,
        max: None,
        step: 1.0,
        unit: InspectorUnit::Degrees,
        range_bar: false,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "frames",
        kind: InspectorFieldKind::Number,
        min: Some(1.0),
        max: Some(256.0),
        step: 1.0,
        unit: InspectorUnit::Unit,
        range_bar: false,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "fps",
        kind: InspectorFieldKind::Number,
        min: Some(0.0),
        max: Some(120.0),
        step: 0.5,
        unit: InspectorUnit::Unit,
        range_bar: true,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "intensity",
        kind: InspectorFieldKind::Number,
        min: Some(0.0),
        max: Some(1_000_000.0),
        step: 0.05,
        unit: InspectorUnit::Lux,
        range_bar: false,
        enum_values: &[],
    },
    InspectorFieldSpec {
        key: "sort_mode",
        kind: InspectorFieldKind::Enum,
        min: None,
        max: None,
        step: 0.0,
        unit: InspectorUnit::None,
        range_bar: false,
        enum_values: seal_world::sprite_sort::SORT_MODES,
    },
    InspectorFieldSpec {
        key: "slice_mode",
        kind: InspectorFieldKind::Enum,
        min: None,
        max: None,
        step: 0.0,
        unit: InspectorUnit::None,
        range_bar: false,
        enum_values: seal_world::sprite_slice::MODES,
    },
];

pub fn field_spec(key: &str) -> Option<&'static InspectorFieldSpec> {
    INSPECTOR_FIELD_SPECS.iter().find(|spec| spec.key == key)
}

/// Bounds and suffix the number row must use. Rotation axes stay on the triple row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumberWidget {
    pub min: Option<f32>,
    pub max: Option<f32>,
    pub step: f32,
    pub unit: InspectorUnit,
    pub range_bar: bool,
}

/// Widget the inspector paints for a table field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MetadataRow {
    Dropdown,
    Flags,
    YesNo,
    Number(NumberWidget),
}

/// Row widget taken from the metadata table. `None` means the key is not in the table.
pub fn metadata_row(key: &str) -> Option<MetadataRow> {
    let spec = field_spec(key)?;
    Some(match spec.kind {
        InspectorFieldKind::Enum => MetadataRow::Dropdown,
        InspectorFieldKind::Flags => MetadataRow::Flags,
        InspectorFieldKind::YesNo => MetadataRow::YesNo,
        InspectorFieldKind::Number => MetadataRow::Number(NumberWidget {
            min: spec.min,
            max: spec.max,
            step: spec.step,
            unit: spec.unit,
            range_bar: spec.range_bar,
        }),
    })
}

/// True when `value` is not the field's built-in default (the changed marker).
pub fn changed_from_default(key: &str, value: &str) -> bool {
    value != inspector_field_default(key)
}

/// Fluent message key for a unit, if any.
pub fn unit_message_key(unit: InspectorUnit) -> Option<&'static str> {
    match unit {
        InspectorUnit::None => None,
        InspectorUnit::Degrees => Some("inspector-unit-degrees"),
        InspectorUnit::Seconds => Some("inspector-unit-seconds"),
        InspectorUnit::Pixels => Some("inspector-unit-pixels"),
        InspectorUnit::Lux => Some("inspector-unit-lux"),
        InspectorUnit::Unit => Some("inspector-unit-scalar"),
    }
}

/// Reject unknown enum text and keep the default; returns an error message key when rewritten.
pub fn sanitize_enum_field(key: &str, raw: &str) -> (String, Option<&'static str>) {
    let Some(spec) = field_spec(key) else {
        return (raw.to_string(), None);
    };
    if spec.kind != InspectorFieldKind::Enum {
        return (raw.to_string(), None);
    }
    // Role is an open tag (camera, light, and names the user types). The dropdown
    // lists the known ones and keeps anything else.
    if key == "role" {
        return (raw.trim().to_string(), None);
    }
    let trimmed = raw.trim();
    if trimmed.is_empty() && spec.enum_values.contains(&"") {
        return (String::new(), None);
    }
    if spec.enum_values.iter().any(|v| *v == trimmed) {
        return (trimmed.to_string(), None);
    }
    let default = inspector_field_default(key);
    let rewritten = if default.is_empty() { String::new() } else { default.to_string() };
    (rewritten, Some("inspector-enum-unknown"))
}

/// Clamp a numeric field using metadata bounds.
pub fn clamp_number_field(key: &str, raw: &str) -> String {
    let Some(spec) = field_spec(key) else {
        return raw.to_string();
    };
    if spec.kind != InspectorFieldKind::Number {
        return raw.to_string();
    }
    let Ok(mut value) = raw.trim().parse::<f32>() else {
        return inspector_field_default(key).to_string();
    };
    if let Some(min) = spec.min {
        value = value.max(min);
    }
    if let Some(max) = spec.max {
        value = value.min(max);
    }
    format_number(value)
}

fn format_number(value: f32) -> String {
    if value.fract().abs() < 1e-4 {
        format!("{value:.0}")
    } else {
        let text = format!("{value:.4}");
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Names for collision layers 1–32 from project settings, with numeric fallbacks.
pub fn collision_layer_labels(project_setting: Option<&str>) -> [String; 32] {
    let mut out: [String; 32] = std::array::from_fn(|i| format!("Layer {}", i + 1));
    let Some(text) = project_setting else {
        return out;
    };
    for (index, part) in text.split([';', ',', '\n']).enumerate().take(32) {
        let name = part.trim();
        if !name.is_empty() {
            out[index] = name.to_string();
        }
    }
    out
}

/// Project setting id for the 32 collision layer names, stored in `project.seal`.
pub const COLLISION_LAYER_SETTING: &str = "physics.collision_layer_names";

/// Project setting id for sorting layer names shown on sprites and camera cull masks.
pub const SORTING_LAYER_SETTING: &str = "render.sorting_layer_names";

pub fn sorting_layer_labels(project_setting: Option<&str>) -> [String; 32] {
    collision_layer_labels(project_setting)
}

pub fn project_sorting_layer_names(settings: &[crate::models::Setting]) -> Option<String> {
    settings
        .iter()
        .find(|row| row.id == SORTING_LAYER_SETTING)
        .map(|row| row.value().to_string())
        .filter(|text| !text.trim().is_empty())
}

pub fn load_sorting_layer_names(root: &std::path::Path) -> String {
    let Ok(text) = std::fs::read_to_string(root.join("project.seal")) else {
        return String::new();
    };
    let Ok(raw) = seal_sealtext::parse(&text) else {
        return String::new();
    };
    let Some(settings) = raw.get("settings") else {
        return String::new();
    };
    match settings.get(SORTING_LAYER_SETTING) {
        Some(seal_sealtext::SealValue::String(value)) => {
            join_layer_names(&split_layer_names(value))
        }
        _ => String::new(),
    }
}

pub fn store_sorting_layer_names(
    root: &std::path::Path,
    names: &str,
) -> Result<(), crate::error::CoreError> {
    let joined = join_layer_names(&split_layer_names(names));
    crate::project::set_setting(
        root,
        SORTING_LAYER_SETTING,
        seal_sealtext::SealValue::String(joined),
    )
}

pub fn project_collision_layer_names(settings: &[crate::models::Setting]) -> Option<String> {
    settings
        .iter()
        .find(|row| row.id == COLLISION_LAYER_SETTING)
        .map(|row| row.value().to_string())
        .filter(|text| !text.trim().is_empty())
}

/// Split a stored name list into 32 slots. Empty slots stay empty.
pub fn split_layer_names(text: &str) -> [String; 32] {
    let mut out: [String; 32] = std::array::from_fn(|_| String::new());
    if text.trim().is_empty() {
        return out;
    }
    for (index, part) in text.split([';', ',', '\n']).take(32).enumerate() {
        out[index] = part.trim().to_string();
    }
    out
}

/// Join 32 slots, keeping blanks between names and dropping blanks at the end.
pub fn join_layer_names(names: &[String; 32]) -> String {
    let mut last = 0;
    for (index, name) in names.iter().enumerate() {
        if !name.trim().is_empty() {
            last = index + 1;
        }
    }
    names[..last].iter().map(|name| name.trim()).collect::<Vec<_>>().join(";")
}

/// Names from `project.seal`. A missing file, a missing key, or a value that is
/// not text falls back to the empty list (dropdowns then use Layer N).
pub fn load_collision_layer_names(root: &std::path::Path) -> String {
    let Ok(text) = std::fs::read_to_string(root.join("project.seal")) else {
        return String::new();
    };
    let Ok(raw) = seal_sealtext::parse(&text) else {
        return String::new();
    };
    let Some(settings) = raw.get("settings") else {
        return String::new();
    };
    match settings.get(COLLISION_LAYER_SETTING) {
        Some(seal_sealtext::SealValue::String(value)) => {
            join_layer_names(&split_layer_names(value))
        }
        _ => String::new(),
    }
}

/// Write the name list into the project's `project.seal` settings table.
pub fn store_collision_layer_names(
    root: &std::path::Path,
    names: &str,
) -> Result<(), crate::error::CoreError> {
    let joined = join_layer_names(&split_layer_names(names));
    crate::project::set_setting(
        root,
        COLLISION_LAYER_SETTING,
        seal_sealtext::SealValue::String(joined),
    )
}

/// Whether every named field in the table is reachable on mock nodes (sanity for tests).
pub fn spec_keys() -> impl Iterator<Item = &'static str> {
    INSPECTOR_FIELD_SPECS.iter().map(|spec| spec.key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_covers_declared_keys() {
        for key in spec_keys() {
            assert!(field_spec(key).is_some(), "missing spec for {key}");
        }
    }

    #[test]
    fn enum_unknown_keeps_default() {
        let (value, err) = sanitize_enum_field("light2d", "laser");
        assert_eq!(err, Some("inspector-enum-unknown"));
        assert!(value.is_empty());
        let (value, err) = sanitize_enum_field("phys_shape", "star");
        assert_eq!(err, Some("inspector-enum-unknown"));
        assert_eq!(value, "box");
    }

    #[test]
    fn clamp_respects_bounds() {
        assert_eq!(clamp_number_field("opacity", "2"), "1");
        assert_eq!(clamp_number_field("opacity", "-1"), "0");
        assert_eq!(clamp_number_field("fov", "500"), "179");
    }

    #[test]
    fn flags_round_trip_via_layers_text() {
        use crate::phys2d::{layers_text, parse_layers};
        let all = layers_text(u32::MAX);
        assert_eq!(parse_layers(&all), Some(u32::MAX));
        let none = layers_text(0);
        assert_eq!(parse_layers(&none), Some(0));
        let bits = layers_text(0b101);
        assert_eq!(bits, "1, 3");
    }

    #[test]
    fn layer_names_from_project() {
        let labels = collision_layer_labels(Some("Hero;Ground;;;Enemy"));
        assert_eq!(labels[0], "Hero");
        assert_eq!(labels[1], "Ground");
        assert_eq!(labels[2], "Layer 3");
        assert_eq!(labels[4], "Enemy");
    }

    #[test]
    fn every_table_field_has_a_metadata_row() {
        for spec in INSPECTOR_FIELD_SPECS {
            let row = metadata_row(spec.key).unwrap_or_else(|| panic!("{}", spec.key));
            match spec.kind {
                InspectorFieldKind::Enum => assert_eq!(row, MetadataRow::Dropdown),
                InspectorFieldKind::Flags => assert_eq!(row, MetadataRow::Flags),
                InspectorFieldKind::YesNo => assert_eq!(row, MetadataRow::YesNo),
                InspectorFieldKind::Number => {
                    assert_eq!(
                        row,
                        MetadataRow::Number(NumberWidget {
                            min: spec.min,
                            max: spec.max,
                            step: spec.step,
                            unit: spec.unit,
                            range_bar: spec.range_bar,
                        })
                    );
                }
            }
        }
    }

    #[test]
    fn layer_names_round_trip_and_a_damaged_value_falls_back() {
        let dir = std::env::temp_dir().join(format!(
            "seal-layers-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let folder = dir.join("demo");
        crate::project::create_project(&folder, "Demo").unwrap();
        store_collision_layer_names(&folder, "Hero;;Mud").unwrap();
        let loaded = load_collision_layer_names(&folder);
        let labels = collision_layer_labels(Some(&loaded));
        assert_eq!(labels[0], "Hero");
        assert_eq!(labels[1], "Layer 2");
        assert_eq!(labels[2], "Mud");
        let path = folder.join("project.seal");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("Hero"), "{text}");
        let mut raw = seal_sealtext::parse(&text).unwrap();
        let mut settings = raw
            .get("settings")
            .cloned()
            .unwrap_or(seal_sealtext::SealValue::Map(Vec::new().into()));
        settings.insert(COLLISION_LAYER_SETTING, seal_sealtext::SealValue::Number("5".into()));
        raw.insert("settings", settings);
        std::fs::write(&path, raw.to_text()).unwrap();
        assert_eq!(load_collision_layer_names(&folder), "");
        assert_eq!(collision_layer_labels(None)[0], "Layer 1");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
