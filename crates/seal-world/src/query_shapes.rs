//! Scene objects as the shapes graph queries ask about.
//!
//! A body, or a collider part with no body of its own, becomes one shape.
//! Triggers (areas) are marked so sweeps can skip them.

use seal_graph::query::Shape;

use crate::Entity;

/// Shapes for every visible body or collider part. `handle` is the graph id of that object.
pub fn from_entities(entities: &[Entity], mut handle: impl FnMut(usize) -> u64) -> Vec<Shape> {
    entities
        .iter()
        .enumerate()
        .filter_map(|(i, entity)| {
            if entity.hidden
                || !entity.collider_enabled
                || (entity.phys2d.body.is_empty() && !entity.phys2d.part)
            {
                return None;
            }
            Some(shape_of(entity, handle(i)))
        })
        .collect()
}

fn shape_of(entity: &Entity, id: u64) -> Shape {
    let p = &entity.phys2d;
    let angle = entity.rotation[2].to_radians() as f64;
    let (s, c) = angle.sin_cos();
    let center = [entity.translation[0] as f64, entity.translation[1] as f64];
    let points = p
        .points
        .iter()
        .map(|pt| {
            let x = pt[0] as f64 * entity.scale[0] as f64;
            let y = pt[1] as f64 * entity.scale[1] as f64;
            [center[0] + c * x - s * y, center[1] + s * x + c * y]
        })
        .collect();
    let (kind, half) = match p.shape.as_str() {
        "circle" => {
            let radius =
                p.size[0] as f64 * entity.scale[0].abs().max(entity.scale[1].abs()) as f64 * 0.5;
            ("circle", [radius, radius])
        }
        "polygon" => ("polygon", [0.0, 0.0]),
        "edge" => ("edge", [0.0, 0.0]),
        _ => (
            "box",
            [
                (p.size[0] * entity.scale[0]).abs() as f64 * 0.5,
                (p.size[1] * entity.scale[1]).abs() as f64 * 0.5,
            ],
        ),
    };
    Shape {
        entity: id,
        kind: kind.into(),
        center,
        angle,
        half,
        points,
        layer: p.layer,
        sensor: !entity.area.mode.is_empty(),
    }
}
