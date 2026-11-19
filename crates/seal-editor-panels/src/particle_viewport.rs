//! Turn simulated particles into viewport marks.

use std::path::Path;

use seal_editor_core::light2d::{Blocker, Light};
use seal_editor_core::particle_lit;
use seal_ui::{GridMark, ShapeMark};
use seal_editor_core::asset_editor::{collect_draws_depth, mesh_renderer_primitive, Emitter, ParticleDraw};
use seal_ui::MeshPrimitive;

pub struct ParticleLight2d<'a> {
    pub lights: &'a [Light],
    pub blockers: &'a [Blocker],
    pub ambient: [f32; 3],
    pub project: Option<&'a Path>,
    pub resolve_sprite: bool,
    /// Depths (particle `pos[2]` axis) of opaque scene sprites and shapes, for soft particles.
    pub occluders: &'a [f32],
}

impl Default for ParticleLight2d<'_> {
    fn default() -> Self {
        Self {
            lights: &[],
            blockers: &[],
            ambient: [0.08, 0.08, 0.1],
            project: None,
            resolve_sprite: false,
            occluders: &[],
        }
    }
}

pub fn particle_marks<'m>(
    emitter: &Emitter,
    across: usize,
    up: usize,
    depth_axis: usize,
    flip_across: f32,
    flip_up: f32,
    lighting: ParticleLight2d<'_>,
    material: Option<seal_editor_core::material2d::Material2d>,
) -> Vec<GridMark<'m>> {
    let draws = collect_draws_depth(emitter, lighting.project, lighting.occluders);
    let mut marks = Vec::new();
    for draw in draws {
        match draw {
            ParticleDraw::Billboard { pos, half, color, lit } => {
                let rgba =
                    shaded_rgba(pos, across, up, flip_across, flip_up, color.rgba, lit, &lighting);
                marks.push(round_mark(
                    pos,
                    half,
                    rgba,
                    across,
                    up,
                    depth_axis,
                    flip_across,
                    flip_up,
                    0.0,
                    false,
                    None,
                    [0.0; 4],
                    material.clone(),
                ));
            }
            ParticleDraw::Stretched { pos, half, angle, stretch, color, lit } => {
                let rgba =
                    shaded_rgba(pos, across, up, flip_across, flip_up, color.rgba, lit, &lighting);
                let mut m = round_mark(
                    pos,
                    stretch.max(half),
                    rgba,
                    across,
                    up,
                    depth_axis,
                    flip_across,
                    flip_up,
                    angle,
                    false,
                    None,
                    [0.0; 4],
                    material.clone(),
                );
                m.round = false;
                m.half = [stretch, half * 0.35];
                marks.push(m);
            }
            ParticleDraw::Flipbook { pos, half, angle, picture, uv, color, lit, .. } => {
                let rgba = color.rgba;
                let (sprite, resolved_uv) = if lighting.resolve_sprite {
                    let full = lighting
                        .project
                        .map(|root| root.join(&picture))
                        .unwrap_or_else(|| Path::new(&picture).to_path_buf());
                    (full.to_string_lossy().into_owned(), uv)
                } else {
                    (picture, uv)
                };
                let mut m = round_mark(
                    pos,
                    half,
                    rgba,
                    across,
                    up,
                    depth_axis,
                    flip_across,
                    flip_up,
                    angle,
                    false,
                    Some(sprite),
                    resolved_uv,
                    material.clone(),
                );
                m.round = false;
                m.sprite_tint = rgba_bytes(rgba);
                if lit && lighting.lights.is_empty() {
                    let rgba =
                        shaded_rgba(pos, across, up, flip_across, flip_up, rgba, true, &lighting);
                    m.sprite_tint = rgba_bytes(rgba);
                }
                marks.push(m);
            }
            ParticleDraw::Mesh { pos, half, angle, mesh, color, lit } => {
                let rgba = shaded_rgba(pos, across, up, flip_across, flip_up, color.rgba, lit, &lighting);
                let primitive = match mesh_renderer_primitive(&mesh) {
                    Some("sphere") => MeshPrimitive::Sphere,
                    Some("plane") => MeshPrimitive::Plane,
                    Some("cylinder") => MeshPrimitive::Cylinder,
                    Some("cone") => MeshPrimitive::Cone,
                    _ => MeshPrimitive::Cube,
                };
                let mut m = round_mark(
                    pos,
                    half,
                    rgba,
                    across,
                    up,
                    depth_axis,
                    flip_across,
                    flip_up,
                    angle,
                    false,
                    None,
                    [0.0; 4],
                    material.clone(),
                );
                m.round = false;
                m.mesh = Some(primitive);
                m.size = [half, half, half];
                marks.push(m);
            }
            ParticleDraw::Trail { points, widths, colors, lit } => {
                if points.len() < 2 {
                    continue;
                }
                let mut prev = points[0];
                for (i, p) in points.iter().enumerate().skip(1) {
                    let w = widths.get(i).copied().unwrap_or(0.05);
                    let mut c = colors.get(i).map(|c| c.rgba).unwrap_or([1.0; 4]);
                    c = shaded_rgba(prev, across, up, flip_across, flip_up, c, lit, &lighting);
                    marks.push(GridMark {
                        x: prev[across] * flip_across,
                        y: prev[up] * flip_up,
                        half: [0.2, 0.2],
                        round: false,
                        billboard: None,
                        selected: false,
                        faint: false,
                        label: "",
                        tint: [0, 1, 2].map(|j| (c[j].clamp(0.0, 1.0) * 255.0) as u8),
                        angle: 0.0,
                        depth: prev[depth_axis] - 0.01,
                        size: [w, w, w],
                        mesh: None,
                        pickable: false,
                        sprite: None,
                        layer: i32::MAX,
                        uv: [0.0, 0.0, 1.0, 1.0],
                        order: 0,
                        sprite_tint: rgba_bytes(c),
                        slice: [0.0; 4],
                        slice_mode: "stretch",
                        slice_size: [0.0; 2],
                        pivot: [0.5, 0.5],
                        normal_map: None,
                        shape2d: Some(ShapeMark {
                            points: vec![
                                [prev[across] * flip_across, prev[up] * flip_up],
                                [p[across] * flip_across, p[up] * flip_up],
                            ],
                            width: w,
                            color: rgba_bytes(c),
                            filled: false,
                            closed: false,
                            edit_points: false,
                            uvs: Vec::new(),
                            texture: String::new(),
                            ribbon: Vec::new(),
                            ribbon_tris: Vec::new(),
                        }),
                        sprite_shape: None,
                        material: material.clone(),
                        mask_mode: 0,
                        mask_kind: 0,
                        surface: seal_ui::MeshSurface::EMPTY,
                    });
                    prev = *p;
                }
            }
        }
    }
    marks
}

#[cfg(test)]
mod tests {
    use seal_editor_core::material2d::Material2d;

    use super::*;

    #[test]
    fn particle_mark_carries_emitter_material_blend() {
        let material = Material2d {
            effect: String::new(),
            blend: "additive".into(),
            ..Material2d::default()
        };
        let mark = round_mark(
            [0.0, 0.0, 0.0],
            0.5,
            [1.0, 0.5, 0.0, 0.8],
            0,
            1,
            2,
            1.0,
            1.0,
            0.0,
            false,
            None,
            [0.0; 4],
            Some(material),
        );
        assert_eq!(mark.material.as_ref().map(|m| m.blend.as_str()), Some("additive"));
    }
}

fn shaded_rgba(
    pos: [f32; 3],
    across: usize,
    up: usize,
    flip_across: f32,
    flip_up: f32,
    rgba: [f32; 4],
    lit: bool,
    lighting: &ParticleLight2d<'_>,
) -> [f32; 4] {
    if !lit || lighting.lights.is_empty() {
        return rgba;
    }
    let at = [pos[across] * flip_across, pos[up] * flip_up];
    particle_lit::lit_rgba(at, rgba, true, lighting.lights, lighting.blockers, lighting.ambient)
}

fn rgba_bytes(rgba: [f32; 4]) -> [u8; 4] {
    rgba.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn round_mark(
    pos: [f32; 3],
    half: f32,
    rgba: [f32; 4],
    across: usize,
    up: usize,
    depth_axis: usize,
    flip_across: f32,
    flip_up: f32,
    angle: f32,
    faint: bool,
    sprite: Option<String>,
    uv: [f32; 4],
    material: Option<seal_editor_core::material2d::Material2d>,
) -> GridMark<'static> {
    GridMark {
        x: pos[across] * flip_across,
        y: pos[up] * flip_up,
        half: [half, half],
        round: sprite.is_none(),
        billboard: None,
        selected: false,
        faint,
        label: "",
        tint: [0, 1, 2].map(|i| (rgba[i].clamp(0.0, 1.0) * 255.0) as u8),
        angle,
        depth: pos[depth_axis] - 0.01,
        size: [half, half, half],
        mesh: None,
        pickable: false,
        sprite,
        layer: i32::MAX,
        uv,
        order: 0,
        sprite_tint: rgba_bytes(rgba),
        slice: [0.0; 4],
        slice_mode: "stretch",
        slice_size: [0.0; 2],
        pivot: [0.5, 0.5],
        normal_map: None,
        shape2d: None,
        sprite_shape: None,
        material,
        mask_mode: 0,
        mask_kind: 0,
        surface: seal_ui::MeshSurface::EMPTY,
    }
}
