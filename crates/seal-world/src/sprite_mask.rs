//! Sprite masks and how a sprite is clipped by them.
//!
//! A mask is a rectangle, a circle, or a picture's alpha. It follows the
//! object's move, turn and scale, including while a timeline moves those.
//! It only clips sprites whose sorting layer and order in layer sit in its range.
//! Several masks combine as a union.

use crate::{format_num, Entity};

pub use seal_scene::SpriteMask;

/// Stored values of the mask shape row.
pub const SHAPES: &[&str] = &["rect", "circle", "alpha"];
/// Stored values of a sprite's mask interaction row.
pub const INTERACTIONS: &[&str] = &["none", "inside", "outside"];
/// Inclusive default range for layers and orders.
pub const RANGE: i32 = 1000;

pub const KEYS: &[&str] = &[
    "mask_shape",
    "mask_image",
    "mask_cutoff",
    "mask_layer_min",
    "mask_layer_max",
    "mask_order_min",
    "mask_order_max",
    "mask",
];

fn shape_name(mask: &SpriteMask) -> String {
    if mask.shape.is_empty() {
        "none".into()
    } else {
        mask.shape.clone()
    }
}

fn interaction_name(entity: &Entity) -> String {
    if entity.mask.is_empty() {
        "none".into()
    } else {
        entity.mask.clone()
    }
}

/// Inspector rows for a mask. Empty when this object is not a mask.
pub fn fields(entity: &Entity) -> Vec<(String, String)> {
    let mask = &entity.sprite_mask;
    if mask.shape.is_empty() {
        return Vec::new();
    }
    let mut out = vec![("mask_shape".into(), shape_name(mask))];
    if mask.shape == "alpha" {
        out.push(("mask_image".into(), mask.image.clone()));
        out.push(("mask_cutoff".into(), format_num(mask.cutoff)));
    }
    out.push(("mask_layer_min".into(), mask.layer_min.to_string()));
    out.push(("mask_layer_max".into(), mask.layer_max.to_string()));
    out.push(("mask_order_min".into(), mask.order_min.to_string()));
    out.push(("mask_order_max".into(), mask.order_max.to_string()));
    out
}

/// The sprite's mask interaction row, shown once it has a picture.
pub fn interaction_field(entity: &Entity) -> Option<(String, String)> {
    if entity.sprite.is_empty() {
        return None;
    }
    Some(("mask".into(), interaction_name(entity)))
}

pub fn field_text(entity: &Entity, key: &str) -> Option<String> {
    let mask = &entity.sprite_mask;
    Some(match key {
        "mask_shape" => shape_name(mask),
        "mask_image" => mask.image.clone(),
        "mask_cutoff" => format_num(mask.cutoff),
        "mask_layer_min" => mask.layer_min.to_string(),
        "mask_layer_max" => mask.layer_max.to_string(),
        "mask_order_min" => mask.order_min.to_string(),
        "mask_order_max" => mask.order_max.to_string(),
        "mask" => interaction_name(entity),
        _ => return None,
    })
}

fn integer(text: &str) -> Option<i32> {
    let n: f32 = text.trim().parse().ok()?;
    if !n.is_finite() || !(-10000.0..=10000.0).contains(&n) {
        return None;
    }
    Some(n.round() as i32)
}

/// Why a field's text is refused, or `None` when it is fine.
pub fn check(key: &str, value: &str) -> Option<&'static str> {
    let v = value.trim();
    let ok = match key {
        "mask_shape" => v == "none" || SHAPES.contains(&v),
        "mask" => INTERACTIONS.contains(&v),
        "mask_image" => {
            v.is_empty()
                || v.to_ascii_lowercase().ends_with(".png")
                || v.to_ascii_lowercase().ends_with(".jpg")
                || v.to_ascii_lowercase().ends_with(".jpeg")
        }
        "mask_cutoff" => v.parse::<f32>().is_ok_and(|n| n.is_finite() && (0.0..=1.0).contains(&n)),
        "mask_layer_min" | "mask_layer_max" | "mask_order_min" | "mask_order_max" => {
            integer(v).is_some()
        }
        _ => false,
    };
    if ok {
        None
    } else {
        Some("inspector-value-out-of-range")
    }
}

pub fn apply(entity: &mut Entity, key: &str, value: &str) -> bool {
    if !KEYS.contains(&key) || check(key, value).is_some() {
        return false;
    }
    let v = value.trim();
    let mask = &mut entity.sprite_mask;
    match key {
        "mask_shape" => mask.shape = if v == "none" { String::new() } else { v.into() },
        "mask_image" => mask.image = v.replace('\\', "/"),
        "mask_cutoff" => mask.cutoff = v.parse().unwrap_or(mask.cutoff),
        "mask_layer_min" => mask.layer_min = integer(v).unwrap_or(mask.layer_min),
        "mask_layer_max" => mask.layer_max = integer(v).unwrap_or(mask.layer_max),
        "mask_order_min" => mask.order_min = integer(v).unwrap_or(mask.order_min),
        "mask_order_max" => mask.order_max = integer(v).unwrap_or(mask.order_max),
        "mask" => entity.mask = if v == "none" { String::new() } else { v.into() },
        _ => return false,
    }
    true
}
