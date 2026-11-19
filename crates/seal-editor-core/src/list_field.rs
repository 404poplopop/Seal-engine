//! Inspector list fields: parse, edit, and serialize multi-value component text.

use seal_world::phys2d::parse_pair;
use seal_world::shape2d;

/// Element type for one list row in the inspector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListElemKind {
    Vec2,
    Text,
    Number,
    Bool,
    Color,
}

/// How a scene field is stored as one inspector string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListFieldSpec {
    pub kind: ListElemKind,
}

/// One parsed row; `damaged` is true when the segment could not be read.
#[derive(Clone, Debug, PartialEq)]
pub struct ListRow {
    pub text: String,
    pub damaged: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListEditOp {
    Add,
    Remove,
    Duplicate,
    Reorder { to: usize },
    Clear,
}

/// Replace one element's text.
pub fn set_list_element(
    spec: ListFieldSpec,
    text: &str,
    index: usize,
    value: &str,
) -> Option<String> {
    let mut rows = parse_list_field(spec, text);
    let row = rows.get_mut(index)?;
    row.text = value.trim().to_string();
    row.damaged = !element_valid(spec.kind, &row.text);
    Some(serialize_list_field(&rows))
}

pub fn parse_list_edit_op(op: &str, to: usize) -> Option<ListEditOp> {
    match op {
        "add" => Some(ListEditOp::Add),
        "remove" => Some(ListEditOp::Remove),
        "duplicate" => Some(ListEditOp::Duplicate),
        "reorder" => Some(ListEditOp::Reorder { to }),
        "clear" => Some(ListEditOp::Clear),
        _ => None,
    }
}

/// Scene fields edited as foldable lists in the inspector.
pub fn list_spec_for_field(field: &str) -> Option<ListFieldSpec> {
    match field {
        "phys_points" | "shape_points" | "light_points" => {
            Some(ListFieldSpec { kind: ListElemKind::Vec2 })
        }
        _ => None,
    }
}

/// Split one list field into rows, keeping damaged segments for display.
pub fn parse_list_field(spec: ListFieldSpec, text: &str) -> Vec<ListRow> {
    let sep = ';';
    if text.trim().is_empty() {
        return Vec::new();
    }
    text.split(sep)
        .filter(|p| !p.trim().is_empty())
        .map(|part| {
            let trimmed = part.trim();
            let damaged = !element_valid(spec.kind, trimmed);
            ListRow { text: trimmed.to_string(), damaged }
        })
        .collect()
}

fn element_valid(kind: ListElemKind, text: &str) -> bool {
    match kind {
        ListElemKind::Vec2 => parse_pair(text).is_some(),
        ListElemKind::Text => true,
        ListElemKind::Number => text.parse::<f32>().is_ok_and(|v| v.is_finite()),
        ListElemKind::Bool => matches!(text, "yes" | "no" | "true" | "false"),
        ListElemKind::Color => crate::parse_tint(text).is_some(),
    }
}

/// Serialize rows back to the field's text form.
pub fn serialize_list_field(rows: &[ListRow]) -> String {
    rows.iter().map(|row| row.text.as_str()).collect::<Vec<_>>().join("; ")
}

/// Default text for a new element.
pub fn default_element(spec: ListFieldSpec) -> &'static str {
    match spec.kind {
        ListElemKind::Vec2 => "0, 0",
        ListElemKind::Text => "",
        ListElemKind::Number => "0",
        ListElemKind::Bool => "no",
        ListElemKind::Color => "#FFFFFF",
    }
}

/// Apply one list edit to the stored text; `None` when the operation is invalid.
pub fn apply_list_edit(
    spec: ListFieldSpec,
    text: &str,
    index: usize,
    op: ListEditOp,
) -> Option<String> {
    let mut rows = parse_list_field(spec, text);
    match op {
        ListEditOp::Add => {
            rows.push(ListRow { text: default_element(spec).to_string(), damaged: false });
        }
        ListEditOp::Remove => {
            rows.get(index)?;
            rows.remove(index);
        }
        ListEditOp::Duplicate => {
            let row = rows.get(index)?.clone();
            rows.insert(index + 1, row);
        }
        ListEditOp::Reorder { to } => {
            let row = rows.get(index)?.clone();
            rows.remove(index);
            let to = to.min(rows.len());
            rows.insert(to, row);
        }
        ListEditOp::Clear => rows.clear(),
    }
    Some(serialize_list_field(&rows))
}

/// Whether the whole field text is accepted by world `check` hooks.
pub fn field_check(field: &str, value: &str) -> Option<&'static str> {
    if seal_world::phys2d::KEYS.contains(&field) {
        return seal_world::phys2d::check(field, value);
    }
    if shape2d::known(field) {
        return shape2d::check(field, value);
    }
    if field == "light_points"
        && !value.trim().is_empty()
        && seal_world::phys2d::parse_points(value).is_none()
    {
        return Some("inspector-shape-points-invalid");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use seal_world::phys2d::points_text;

    fn spec() -> ListFieldSpec {
        ListFieldSpec { kind: ListElemKind::Vec2 }
    }

    #[test]
    fn add_remove_duplicate_and_reorder_round_trip() {
        let s = spec();
        let mut text = "0 0; 1 0".to_string();
        text = apply_list_edit(s, &text, 0, ListEditOp::Add).unwrap();
        assert_eq!(text, "0 0; 1 0; 0, 0");
        text = apply_list_edit(s, &text, 1, ListEditOp::Duplicate).unwrap();
        assert_eq!(text, "0 0; 1 0; 1 0; 0, 0");
        text = apply_list_edit(s, &text, 2, ListEditOp::Remove).unwrap();
        assert_eq!(text, "0 0; 1 0; 0, 0");
        text = apply_list_edit(s, &text, 0, ListEditOp::Reorder { to: 2 }).unwrap();
        assert_eq!(text, "1 0; 0, 0; 0 0");
    }

    #[test]
    fn clear_empties_the_field() {
        let s = spec();
        let out = apply_list_edit(s, "1 1; 2 2", 0, ListEditOp::Clear).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn damaged_segment_stays_reported() {
        let rows = parse_list_field(spec(), "0 0; not-a-point; 2 1");
        assert_eq!(rows.len(), 3);
        assert!(!rows[0].damaged);
        assert!(rows[1].damaged);
        assert!(!rows[2].damaged);
    }

    #[test]
    fn points_text_round_trip() {
        let pairs: Option<Vec<[f32; 2]>> =
            "0 0; 2 1".split(';').filter(|p| !p.trim().is_empty()).map(parse_pair).collect();
        let canon = points_text(&pairs.unwrap());
        assert_eq!(canon, "0 0; 2 1");
    }
}
