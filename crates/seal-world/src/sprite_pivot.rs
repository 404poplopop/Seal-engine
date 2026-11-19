//! Sprite pivot inside the picture, used for drawing, rotation and sorting.

use crate::{format_num, Entity};

pub use seal_ecs::sprite_draw::{card_shift, pivot_rect};

pub const PRESETS: &[(&str, [f32; 2])] = &[
    ("center", [0.5, 0.5]),
    ("bottom_left", [0.0, 0.0]),
    ("bottom_center", [0.5, 0.0]),
    ("bottom_right", [1.0, 0.0]),
    ("top_left", [0.0, 1.0]),
    ("top_center", [0.5, 1.0]),
    ("top_right", [1.0, 1.0]),
];

pub const KEYS: &[&str] = &["pivot", "pivot_x", "pivot_y"];

pub fn pivot_text(pivot: [f32; 2]) -> String {
    format!("{}, {}", format_num(pivot[0]), format_num(pivot[1]))
}

/// Read pivot from `x, y`, a preset name, or `preset x, y`.
pub fn parse_pivot(text: &str) -> Option<[f32; 2]> {
    let v = text.trim();
    if v.is_empty() {
        return None;
    }
    for (name, p) in PRESETS {
        if v.eq_ignore_ascii_case(name) {
            return Some(*p);
        }
    }
    let parts: Vec<f32> = v
        .split(|c| c == ',' || c == ' ')
        .filter_map(|p| p.trim().parse::<f32>().ok())
        .filter(|n: &f32| n.is_finite())
        .collect();
    match parts.as_slice() {
        [x, y] => Some([x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)]),
        _ => None,
    }
}

pub fn field_text(entity: &Entity, key: &str) -> Option<String> {
    if entity.sprite.is_empty() {
        return None;
    }
    match key {
        "pivot" => Some(pivot_text(entity.pivot)),
        "pivot_x" => Some(format_num(entity.pivot[0])),
        "pivot_y" => Some(format_num(entity.pivot[1])),
        _ => None,
    }
}

pub fn check(key: &str, value: &str) -> Option<&'static str> {
    match key {
        "pivot" => parse_pivot(value).is_none().then_some("sprite-pivot-invalid"),
        "pivot_x" | "pivot_y" => value
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
            .is_none()
            .then_some("sprite-pivot-invalid"),
        _ => None,
    }
}

pub fn apply(entity: &mut Entity, key: &str, value: &str) {
    match key {
        "pivot" => {
            if let Some(p) = parse_pivot(value) {
                entity.pivot = p;
            }
        }
        "pivot_x" => {
            if let Ok(v) = value.trim().parse::<f32>() {
                if v.is_finite() && (0.0..=1.0).contains(&v) {
                    entity.pivot[0] = v;
                }
            }
        }
        "pivot_y" => {
            if let Ok(v) = value.trim().parse::<f32>() {
                if v.is_finite() && (0.0..=1.0).contains(&v) {
                    entity.pivot[1] = v;
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_and_custom_pivots_parse() {
        assert_eq!(parse_pivot("center"), Some([0.5, 0.5]));
        assert_eq!(parse_pivot("0.25, 0.75"), Some([0.25, 0.75]));
        assert_eq!(parse_pivot("2, 0.5"), Some([1.0, 0.5]));
        assert_eq!(parse_pivot("nope"), None);
    }
}
