//! The 2D camera's drag-margin rectangle on the flat view.

use seal_editor_core::Entity;
use seal_ui::{GridMark, ShapeMark};

/// Half the view in scene units. 20 screen pixels is one unit at zoom 1,
/// and the camera zoom scales that the same way play does.
pub fn view_half(pixels: [f32; 2], editor_zoom: f32, cam_zoom: f32) -> [f32; 2] {
    let cell = 20.0 * (editor_zoom * cam_zoom).clamp(0.05, 50.0);
    if !(cell.is_finite() && cell > 0.0) {
        return [0.0, 0.0];
    }
    [pixels[0].max(0.0) * 0.5 / cell, pixels[1].max(0.0) * 0.5 / cell]
}

/// A rectangle for the selected camera's dead zone. `None` when the camera
/// is off or every margin is zero.
pub fn mark(
    entity: &Entity,
    view_half: [f32; 2],
    axes: [usize; 2],
    flip: [f32; 2],
    depth_axis: usize,
) -> Option<GridMark<'static>> {
    if !entity.camera2d.on {
        return None;
    }
    let center = [entity.translation[0], entity.translation[1]];
    let corners = seal_editor_core::cam_drag::zone_rect(center, entity.camera2d.drag, view_half)?;
    let points: Vec<[f32; 2]> = corners
        .into_iter()
        .map(|p| {
            let world = [p[0], p[1], entity.translation[2]];
            [world[axes[0]] * flip[0], world[axes[1]] * flip[1]]
        })
        .collect();
    let (x, y) = (points[0][0], points[0][1]);
    let mut half = [0.2_f32, 0.2];
    for p in &points {
        half[0] = half[0].max((p[0] - x).abs());
        half[1] = half[1].max((p[1] - y).abs());
    }
    Some(GridMark {
        x: entity.translation[axes[0]] * flip[0],
        y: entity.translation[axes[1]] * flip[1],
        half,
        round: false,
        billboard: None,
        selected: false,
        faint: false,
        label: "",
        tint: [220, 180, 70],
        angle: 0.0,
        depth: entity.translation[depth_axis] - 0.02,
        size: [half[0], half[1], 0.2],
        mesh: None,
        pickable: false,
        sprite: None,
        layer: i32::MAX,
        uv: [0.0, 0.0, 1.0, 1.0],
        order: 2,
        sprite_tint: [255, 255, 255, 255],
        slice: [0.0, 0.0, 0.0, 0.0],
        slice_mode: "stretch",
        slice_size: [0.0, 0.0],
        pivot: [0.5, 0.5],
        normal_map: None,
        shape2d: Some(ShapeMark {
            points,
            width: 0.05,
            color: [220, 180, 70, 230],
            filled: false,
            closed: true,
            edit_points: false,
            uvs: Vec::new(),
            texture: String::new(),
            ribbon: Vec::new(),
            ribbon_tris: Vec::new(),
        }),
        sprite_shape: None,
        material: None,
        mask_mode: 0,
        mask_kind: 0,
        surface: seal_ui::MeshSurface::EMPTY,
    })
}
