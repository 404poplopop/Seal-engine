//! Extra viewport marks for an object produced by Merge Objects.
//!
//! The merged object itself is a small pick target. Its sprites and triangles are drawn
//! as separate marks that are not picked, so click indices stay lined up with the outliner.

use seal_ui::{GridMark, ShapeMark};

pub fn extra_marks<'a>(
    entities: &[&seal_editor_core::Entity],
    across: usize,
    up: usize,
    depth: usize,
    flip_a: f32,
    flip_b: f32,
) -> Vec<GridMark<'a>> {
    let mut marks = Vec::new();
    for entity in entities {
        if entity.merged.kind.is_empty() {
            continue;
        }
        for part in seal_editor_core::object_merge::drawn_sprites(entity) {
            let tint = part.tint.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
            let axis = |index: usize| match index {
                0 => part.x,
                1 => part.y,
                _ => entity.translation[index],
            };
            marks.push(blank(
                [axis(across) * flip_a, axis(up) * flip_b],
                part.half,
                entity.translation[depth],
                [tint[0], tint[1], tint[2]],
                Some(part.image),
                tint,
                part.layer,
                part.order,
                None,
            ));
        }
        for tri in seal_editor_core::object_merge::drawn_tris(entity) {
            let rgb = tri.color.map(|c| (c.clamp(0.0, 1.0) * 255.0) as u8);
            let points =
                tri.points.into_iter().map(|p| [p[across] * flip_a, p[up] * flip_b]).collect();
            marks.push(blank(
                [0.0, 0.0],
                [0.1, 0.1],
                entity.translation[depth],
                rgb,
                None,
                [255, 255, 255, 255],
                0,
                0,
                Some(ShapeMark {
                    points,
                    width: 0.02,
                    color: [rgb[0], rgb[1], rgb[2], 255],
                    filled: true,
                    closed: true,
                    edit_points: false,
                    uvs: Vec::new(),
                    texture: String::new(),
                    ribbon: Vec::new(),
                    ribbon_tris: Vec::new(),
                }),
            ));
        }
    }
    marks
}

#[allow(clippy::too_many_arguments)]
fn blank<'a>(
    at: [f32; 2],
    half: [f32; 2],
    depth: f32,
    tint: [u8; 3],
    sprite: Option<String>,
    sprite_tint: [u8; 4],
    layer: i32,
    order: i32,
    shape2d: Option<ShapeMark>,
) -> GridMark<'a> {
    GridMark {
        x: at[0],
        y: at[1],
        half,
        round: false,
        billboard: None,
        selected: false,
        faint: false,
        label: "",
        tint,
        angle: 0.0,
        depth,
        size: [half[0], half[1], 0.02],
        mesh: None,
        pickable: false,
        sprite,
        layer,
        uv: [0.0, 0.0, 1.0, 1.0],
        order,
        sprite_tint,
        slice: [0.0; 4],
        slice_mode: "stretch",
        slice_size: [0.0; 2],
        pivot: [0.5, 0.5],
        normal_map: None,
        shape2d,
        sprite_shape: None,
        material: None,
        mask_mode: 0,
        mask_kind: 0,
        surface: seal_ui::MeshSurface::EMPTY,
    }
}
