//! Area effects and moving surfaces on scene objects.
//!
//! The inspector, the scene file and copy-paste share [`fields`], [`check`] and
//! [`apply`]. Create entries call [`configure`]. During play the collider of an
//! area is a trigger, and [`gizmos`] draws its direction, range or surface in
//! the flat view.

use std::collections::BTreeMap;

use seal_ecs::Entity as Id;
use seal_physics2d::{AreaEffect, Physics2D, SurfaceEffect};

use crate::{phys2d, Entity};

/// Area modes the inspector offers. `none` clears the effect.
pub const MODES: &[&str] = &["none", "directional", "point", "buoyancy", "gravity", "drag"];
pub const FORCE_KINDS: &[&str] = &["acceleration", "force"];
pub const SIGNS: &[&str] = &["attract", "repel"];
pub const FALLOFFS: &[&str] = &["none", "linear", "inverse"];
pub const GRAVITY_MODES: &[&str] = &["replace", "scale"];

/// Inspector fields, in order. Rows for a mode are hidden until that mode is on.
pub const KEYS: &[&str] = &[
    "area_mode",
    "area_mask",
    "area_priority",
    "area_force",
    "area_direction",
    "area_strength",
    "area_variation",
    "area_sign",
    "area_falloff",
    "area_range",
    "area_surface",
    "area_density",
    "area_linear_drag",
    "area_angular_drag",
    "area_flow",
    "area_gravity_mode",
    "area_gravity",
    "area_gravity_scale",
    "area_linear_damping",
    "area_angular_damping",
    "surface_speed",
];

fn num(value: f32) -> String {
    let text = format!("{value:.4}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".into()
    } else {
        trimmed.into()
    }
}

fn finite(text: &str) -> Option<f32> {
    text.trim().parse::<f32>().ok().filter(|n| n.is_finite())
}

/// The fields the inspector shows for this object.
pub fn fields(area: &AreaEffect, surface: &SurfaceEffect) -> Vec<(String, String)> {
    let mut out = vec![(
        "area_mode".into(),
        if area.mode.is_empty() { "none".into() } else { area.mode.clone() },
    )];
    let mut push = |k: &str, v: String| out.push((k.into(), v));
    match area.mode.as_str() {
        "directional" => {
            push("area_mask", phys2d::layers_text(area.mask));
            push("area_priority", area.priority.to_string());
            push("area_force", area.force_kind.clone());
            push("area_direction", phys2d::pair_text(&area.direction));
            push("area_strength", num(area.strength));
            push("area_variation", num(area.variation));
        }
        "point" => {
            push("area_mask", phys2d::layers_text(area.mask));
            push("area_priority", area.priority.to_string());
            push("area_sign", area.point_sign.clone());
            push("area_falloff", area.falloff.clone());
            push("area_strength", num(area.strength));
            push("area_variation", num(area.variation));
            push("area_range", num(area.range));
        }
        "buoyancy" => {
            push("area_mask", phys2d::layers_text(area.mask));
            push("area_priority", area.priority.to_string());
            push("area_surface", num(area.surface));
            push("area_density", num(area.density));
            push("area_linear_drag", num(area.linear_drag));
            push("area_angular_drag", num(area.angular_drag));
            push("area_flow", phys2d::pair_text(&area.flow));
        }
        "gravity" => {
            push("area_mask", phys2d::layers_text(area.mask));
            push("area_priority", area.priority.to_string());
            push("area_gravity_mode", area.gravity_mode.clone());
            if area.gravity_mode == "scale" {
                push("area_gravity_scale", num(area.gravity_scale));
            } else {
                push("area_gravity", phys2d::pair_text(&area.gravity));
            }
        }
        "drag" => {
            push("area_mask", phys2d::layers_text(area.mask));
            push("area_priority", area.priority.to_string());
            push("area_linear_damping", num(area.linear_damping));
            push("area_angular_damping", num(area.angular_damping));
        }
        _ => {}
    }
    push("surface_speed", num(surface.speed));
    out
}

/// Why `value` is refused for `key`, or `None` when it is fine.
pub fn check(key: &str, value: &str) -> Option<&'static str> {
    let v = value.trim();
    let ok = match key {
        "area_mode" => MODES.contains(&v),
        "area_mask" => phys2d::parse_layers(v).is_some(),
        "area_priority" => v.parse::<i32>().is_ok(),
        "area_force" => FORCE_KINDS.contains(&v),
        "area_direction" | "area_flow" | "area_gravity" => phys2d::parse_pair(v).is_some(),
        "area_sign" => SIGNS.contains(&v),
        "area_falloff" => FALLOFFS.contains(&v),
        "area_gravity_mode" => GRAVITY_MODES.contains(&v),
        "area_strength" | "area_surface" | "area_gravity_scale" | "surface_speed" => {
            finite(v).is_some()
        }
        "area_variation" => finite(v).is_some_and(|n| (0.0..=1.0).contains(&n)),
        "area_range"
        | "area_density"
        | "area_linear_drag"
        | "area_angular_drag"
        | "area_linear_damping"
        | "area_angular_damping" => finite(v).is_some_and(|n| n >= 0.0),
        _ => return Some("unknown"),
    };
    (!ok).then_some(match key {
        "area_mask" => "inspector-area-layers-invalid",
        "area_variation" => "inspector-area-variation-invalid",
        "area_direction" | "area_flow" | "area_gravity" => "inspector-area-pair-invalid",
        _ => "inspector-area-invalid",
    })
}

/// Set one field. Returns false, and changes nothing, when the text is refused.
pub fn apply(area: &mut AreaEffect, surface: &mut SurfaceEffect, key: &str, value: &str) -> bool {
    if !KEYS.contains(&key) || check(key, value).is_some() {
        return false;
    }
    let v = value.trim();
    match key {
        "area_mode" => area.mode = if v == "none" { String::new() } else { v.into() },
        "area_mask" => area.mask = phys2d::parse_layers(v).expect("checked"),
        "area_priority" => area.priority = v.parse().expect("checked"),
        "area_force" => area.force_kind = v.into(),
        "area_direction" => area.direction = phys2d::parse_pair(v).expect("checked"),
        "area_strength" => area.strength = finite(v).expect("checked"),
        "area_variation" => area.variation = finite(v).expect("checked"),
        "area_sign" => area.point_sign = v.into(),
        "area_falloff" => area.falloff = v.into(),
        "area_range" => area.range = finite(v).expect("checked"),
        "area_surface" => area.surface = finite(v).expect("checked"),
        "area_density" => area.density = finite(v).expect("checked"),
        "area_linear_drag" => area.linear_drag = finite(v).expect("checked"),
        "area_angular_drag" => area.angular_drag = finite(v).expect("checked"),
        "area_flow" => area.flow = phys2d::parse_pair(v).expect("checked"),
        "area_gravity_mode" => area.gravity_mode = v.into(),
        "area_gravity" => area.gravity = phys2d::parse_pair(v).expect("checked"),
        "area_gravity_scale" => area.gravity_scale = finite(v).expect("checked"),
        "area_linear_damping" => area.linear_damping = finite(v).expect("checked"),
        "area_angular_damping" => area.angular_damping = finite(v).expect("checked"),
        "surface_speed" => surface.speed = finite(v).expect("checked"),
        _ => return false,
    }
    true
}

pub fn field_text(area: &AreaEffect, surface: &SurfaceEffect, key: &str) -> Option<String> {
    fields(area, surface).into_iter().find(|(k, _)| k == key).map(|(_, v)| v).or_else(|| {
        // A field hidden by the current mode still has a text form, for copy and save.
        let shown = fields(
            &AreaEffect {
                mode: match key {
                    "area_force" | "area_direction" | "area_strength" | "area_variation" => {
                        "directional".into()
                    }
                    "area_sign" | "area_falloff" | "area_range" => "point".into(),
                    "area_surface" | "area_density" | "area_linear_drag" | "area_angular_drag"
                    | "area_flow" => "buoyancy".into(),
                    "area_gravity_mode" | "area_gravity" | "area_gravity_scale" => "gravity".into(),
                    "area_linear_damping" | "area_angular_damping" => "drag".into(),
                    _ => area.mode.clone(),
                },
                ..area.clone()
            },
            surface,
        );
        shown.into_iter().find(|(k, _)| k == key).map(|(_, v)| v)
    })
}

/// A line to draw in the flat view when this object is selected.
#[derive(Clone, Debug)]
pub struct Gizmo {
    pub points: Vec<[f32; 2]>,
    pub closed: bool,
}

fn rect(entity: &Entity) -> [f32; 2] {
    [entity.phys2d.size[0] * entity.scale[0].abs(), entity.phys2d.size[1] * entity.scale[1].abs()]
}

fn arrow(from: [f32; 2], dir: [f32; 2], length: f32) -> Vec<[f32; 2]> {
    let len = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
    if len < 1e-4 || length <= 0.0 {
        return Vec::new();
    }
    let (x, y) = (dir[0] / len, dir[1] / len);
    let tip = [from[0] + x * length, from[1] + y * length];
    let back = length.min(0.35);
    let wing = 0.18;
    vec![
        from,
        tip,
        [tip[0] - x * back + -y * wing, tip[1] - y * back + x * wing],
        tip,
        [tip[0] - x * back - -y * wing, tip[1] - y * back - x * wing],
    ]
}

/// Direction, range or surface marks, in world units, for a selected area.
pub fn gizmos(entity: &Entity) -> Vec<Gizmo> {
    let c = [entity.translation[0], entity.translation[1]];
    let mut out = Vec::new();
    match entity.area.mode.as_str() {
        "directional" => {
            let length = 0.6 + entity.area.strength.abs() * 0.05;
            let points = arrow(c, entity.area.direction, length);
            if points.len() >= 2 {
                out.push(Gizmo { points, closed: false });
            }
        }
        "point" => {
            let radius = if entity.area.range > 0.0 {
                entity.area.range
            } else {
                rect(entity)[0].max(rect(entity)[1]) * 0.5
            };
            if radius > 0.05 {
                let points = (0..24)
                    .map(|i| {
                        let a = i as f32 / 24.0 * std::f32::consts::TAU;
                        [c[0] + a.cos() * radius, c[1] + a.sin() * radius]
                    })
                    .collect();
                out.push(Gizmo { points, closed: true });
            }
            let spoke = if entity.area.point_sign == "repel" { 0.8 } else { -0.8 };
            out.push(Gizmo { points: vec![c, [c[0] + spoke, c[1]]], closed: false });
        }
        "buoyancy" => {
            let half = rect(entity)[0] * 0.5;
            let y = c[1] + entity.area.surface;
            out.push(Gizmo { points: vec![[c[0] - half, y], [c[0] + half, y]], closed: false });
        }
        "gravity" => {
            let dir = if entity.area.gravity_mode == "scale" {
                [0.0, -entity.area.gravity_scale]
            } else {
                entity.area.gravity
            };
            let points = arrow(c, dir, 1.2);
            if points.len() >= 2 {
                out.push(Gizmo { points, closed: false });
            } else {
                let radius = 0.45;
                let points = (0..16)
                    .map(|i| {
                        let a = i as f32 / 16.0 * std::f32::consts::TAU;
                        [c[0] + a.cos() * radius, c[1] + a.sin() * radius]
                    })
                    .collect();
                out.push(Gizmo { points, closed: true });
            }
        }
        "drag" => {
            out.push(Gizmo {
                points: vec![
                    [c[0] - 0.4, c[1]],
                    [c[0] + 0.4, c[1]],
                    [c[0], c[1] - 0.4],
                    [c[0], c[1] + 0.4],
                ],
                closed: false,
            });
        }
        _ => {}
    }
    if entity.surface.speed != 0.0 {
        let deg = entity.rotation[2].to_radians();
        let (x, y) = (deg.cos(), deg.sin());
        let half = rect(entity);
        let top = [c[0] - y * half[1] * 0.5, c[1] + x * half[1] * 0.5];
        let length = entity.surface.speed.abs().max(0.4);
        let dir = [x * entity.surface.speed.signum(), y * entity.surface.speed.signum()];
        let points = arrow(top, dir, length);
        if points.len() >= 2 {
            out.push(Gizmo { points, closed: false });
        }
    }
    out
}

fn outline(size: [f32; 2], color: [f32; 4]) -> seal_scene::Shape2d {
    let (w, h) = (size[0] * 0.5, size[1] * 0.5);
    seal_scene::Shape2d {
        kind: "line".into(),
        points: vec![[-w, -h], [w, -h], [w, h], [-w, h]],
        width: 0.06,
        color,
        closed: true,
        ..Default::default()
    }
}

/// Fill in a freshly added object when `kind` is an area preset.
pub fn configure(entity: &mut Entity, kind: &str) -> bool {
    let (_mode, size, color, extra) = match kind {
        "wind_area" => (
            "directional",
            [6.0, 3.0],
            [0.55, 0.78, 0.95],
            AreaEffect {
                mode: "directional".into(),
                force_kind: "acceleration".into(),
                direction: [1.0, 0.0],
                strength: 12.0,
                variation: 0.0,
                ..AreaEffect::default()
            },
        ),
        "water_area" => (
            "buoyancy",
            [8.0, 4.0],
            [0.25, 0.55, 0.85],
            AreaEffect {
                mode: "buoyancy".into(),
                surface: 0.0,
                density: 1.0,
                linear_drag: 4.0,
                angular_drag: 1.0,
                flow: [0.0, 0.0],
                ..AreaEffect::default()
            },
        ),
        "gravity_area" => (
            "gravity",
            [5.0, 5.0],
            [0.65, 0.55, 0.9],
            AreaEffect {
                mode: "gravity".into(),
                gravity_mode: "replace".into(),
                gravity: [0.0, 0.0],
                priority: 1,
                ..AreaEffect::default()
            },
        ),
        "attractor" => (
            "point",
            [8.0, 8.0],
            [0.9, 0.55, 0.4],
            AreaEffect {
                mode: "point".into(),
                point_sign: "attract".into(),
                falloff: "linear".into(),
                strength: 20.0,
                range: 4.0,
                ..AreaEffect::default()
            },
        ),
        _ => return false,
    };
    entity.color = color;
    entity.phys2d.body = "fixed".into();
    entity.phys2d.shape = if kind == "attractor" { "circle".into() } else { "box".into() };
    entity.phys2d.size = size;
    entity.area = extra;
    entity.shape2d = outline(size, [color[0], color[1], color[2], 0.9]);
    true
}

/// Scene ids of who entered, left, and who is inside each area.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub entered: Vec<(String, String)>,
    pub exited: Vec<(String, String)>,
    pub inside: BTreeMap<String, Vec<String>>,
}

/// Turn physics entity ids into scene ids. Bodies with no scene id are dropped.
pub fn report(physics: &Physics2D, scene_of: &BTreeMap<Id, String>) -> Report {
    let name = |id: Id| scene_of.get(&id).cloned();
    let mut entered = Vec::new();
    let mut exited = Vec::new();
    let (ent, ext) = physics.area_events();
    for (area, body) in ent {
        if let (Some(a), Some(b)) = (name(*area), name(*body)) {
            entered.push((a, b));
        }
    }
    for (area, body) in ext {
        if let (Some(a), Some(b)) = (name(*area), name(*body)) {
            exited.push((a, b));
        }
    }
    let mut inside = BTreeMap::new();
    for (area, scene) in scene_of {
        let names: Vec<String> =
            physics.bodies_inside(*area).iter().filter_map(|b| name(*b)).collect();
        if !names.is_empty() {
            inside.insert(scene.clone(), names);
        }
    }
    Report { entered, exited, inside }
}
