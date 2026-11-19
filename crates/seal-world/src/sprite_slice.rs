//! Nine-slice draw mode and explicit layout size.

use crate::{format_num, Entity};

pub use seal_ecs::sprite_draw::{nine_cuts, tile_pieces, SlicePiece};

pub const MODES: &[&str] = &["stretch", "tile"];

pub const KEYS: &[&str] = &["slice_mode", "slice_size"];

pub fn slice_size_text(size: [f32; 2]) -> String {
    format!("{}, {}", format_num(size[0]), format_num(size[1]))
}

pub fn parse_slice_size(text: &str) -> Option<[f32; 2]> {
    let parts: Vec<f32> = text
        .split([',', ' '])
        .filter_map(|p| p.trim().parse::<f32>().ok())
        .filter(|v: &f32| v.is_finite() && *v >= 0.0)
        .collect();
    match parts.as_slice() {
        [w] => Some([*w, *w]),
        [w, h, ..] => Some([*w, *h]),
        _ => None,
    }
}

pub fn field_text(entity: &Entity, key: &str) -> Option<String> {
    if entity.sprite.is_empty() {
        return None;
    }
    match key {
        "slice_mode" => Some(entity.slice_mode.clone()),
        "slice_size" if entity.slice_size != [0.0; 2] => Some(slice_size_text(entity.slice_size)),
        "slice_size" => Some(String::new()),
        _ => None,
    }
}

pub fn check(key: &str, value: &str) -> Option<&'static str> {
    match key {
        "slice_mode" if !MODES.contains(&value.trim()) => Some("sprite-slice-mode-invalid"),
        "slice_size" if !value.trim().is_empty() && parse_slice_size(value).is_none() => {
            Some("sprite-slice-size-invalid")
        }
        _ => None,
    }
}

pub fn apply(entity: &mut Entity, key: &str, value: &str) {
    match key {
        "slice_mode" => {
            let v = value.trim();
            if MODES.contains(&v) {
                entity.slice_mode = v.to_string();
            }
        }
        "slice_size" => {
            if value.trim().is_empty() {
                entity.slice_size = [0.0; 2];
            } else if let Some(s) = parse_slice_size(value) {
                entity.slice_size = s;
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_size_parses_one_or_two_numbers() {
        assert_eq!(parse_slice_size("64"), Some([64.0, 64.0]));
        assert_eq!(parse_slice_size("10, 20"), Some([10.0, 20.0]));
        assert_eq!(parse_slice_size("-1"), None);
    }
}
