//! The freeform light's polygon on the flat view.

use seal_editor_core::Entity;
use seal_ui::ShapeMark;

/// The polygon on the view plane, when this object is a freeform light.
pub(crate) fn mark(
    entity: &Entity,
    axes: [usize; 2],
    flip: [f32; 2],
    editing: bool,
) -> Option<ShapeMark> {
    if entity.light2d.kind != "freeform" || entity.light2d.points.len() < 3 {
        return None;
    }
    let (sin, cos) = entity.rotation[2].to_radians().sin_cos();
    let points = entity
        .light2d
        .points
        .iter()
        .map(|p| {
            let (x, y) = (p[0] * entity.scale[0], p[1] * entity.scale[1]);
            let world = [
                entity.translation[0] + x * cos - y * sin,
                entity.translation[1] + x * sin + y * cos,
                entity.translation[2],
            ];
            [world[axes[0]] * flip[0], world[axes[1]] * flip[1]]
        })
        .collect();
    let rgb = entity.color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    Some(ShapeMark {
        points,
        width: 0.04,
        color: [rgb[0], rgb[1], rgb[2], 90],
        filled: true,
        closed: true,
        edit_points: editing,
        uvs: Vec::new(),
        texture: String::new(),
        ribbon: Vec::new(),
        ribbon_tris: Vec::new(),
    })
}
