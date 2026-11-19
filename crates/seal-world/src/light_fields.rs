//! Inspector text for the extra 2D light settings: blend, angle, falloff and
//! the freeform polygon.

use crate::phys2d::{parse_points, points_text};
use crate::{format_num, Entity};

pub const BLENDS: &[&str] = &["additive", "multiply", "alpha"];

pub fn rows(e: &Entity) -> Vec<(String, String)> {
    let light = &e.light2d;
    if light.kind.is_empty() {
        return Vec::new();
    }
    let mut out = vec![("light_blend".into(), blend_name(&light.blend))];
    if light.kind == "directional" {
        out.push(("light_angle".into(), format_num(e.rotation[2])));
    }
    if light.kind == "freeform" {
        out.push(("light_falloff".into(), format_num(light.falloff)));
        out.push(("light_points".into(), points_text(&light.points)));
    }
    out
}

pub fn blend_name(text: &str) -> String {
    match text {
        "multiply" => "multiply".into(),
        "alpha" => "alpha".into(),
        _ => "additive".into(),
    }
}

pub fn apply(e: &mut Entity, key: &str, value: &str) -> bool {
    let v = value.trim();
    match key {
        "light_blend" => {
            if !BLENDS.contains(&v) && !v.is_empty() {
                return false;
            }
            e.light2d.blend = blend_name(v);
            true
        }
        "light_falloff" => {
            let Ok(n) = v.parse::<f32>() else { return false };
            if !n.is_finite() {
                return false;
            }
            e.light2d.falloff = n.clamp(0.0, 10000.0);
            true
        }
        "light_points" => {
            if v.is_empty() {
                e.light2d.points.clear();
                return true;
            }
            let Some(points) = parse_points(v) else { return false };
            e.light2d.points = points;
            true
        }
        "light_angle" => {
            let Ok(n) = v.parse::<f32>() else { return false };
            if !n.is_finite() {
                return false;
            }
            e.rotation[2] = n;
            true
        }
        _ => false,
    }
}

/// A square the freeform light starts with, so the handles have somewhere to sit.
pub fn seed_square(e: &mut Entity) {
    if e.light2d.points.len() >= 3 {
        return;
    }
    e.light2d.points = vec![[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]];
    if e.light2d.falloff <= 0.0 {
        e.light2d.falloff = 1.0;
    }
}

/// Turn a scene point back into the light's own units.
pub fn to_local(e: &Entity, world: [f32; 2]) -> [f32; 2] {
    let (sin, cos) = e.rotation[2].to_radians().sin_cos();
    let dx = world[0] - e.translation[0];
    let dy = world[1] - e.translation[1];
    let x = dx * cos + dy * sin;
    let y = -dx * sin + dy * cos;
    let sx = if e.scale[0].abs() < 1e-6 { 1.0 } else { e.scale[0] };
    let sy = if e.scale[1].abs() < 1e-6 { 1.0 } else { e.scale[1] };
    [x / sx, y / sy]
}

pub fn read_saved(
    kind: String,
    range: f32,
    cone: f32,
    shadows: bool,
    blend: &str,
    falloff: f32,
    points: &str,
) -> seal_scene::Light2d {
    seal_scene::Light2d {
        kind,
        range,
        cone,
        shadows,
        blend: if blend.is_empty() { "additive".into() } else { blend.to_string() },
        falloff,
        points: parse_points(points).unwrap_or_default(),
    }
}
