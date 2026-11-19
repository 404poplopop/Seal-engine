//! Clipping sprites in the flat view, and the outline drawn for a selected mask.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use seal_editor_core::clip2d::{self as mask, Interaction, Mask};

pub use seal_editor_core::clip2d::Mask as ViewMask;

use crate::sprites::decode_image;

static PIXELS: Mutex<Option<HashMap<String, Option<Arc<(Vec<u8>, u32, u32)>>>>> = Mutex::new(None);

fn image_pixels(path: &str) -> Option<Arc<(Vec<u8>, u32, u32)>> {
    let mut guard = PIXELS.lock().ok()?;
    let map = guard.get_or_insert_with(HashMap::new);
    map.entry(path.to_string())
        .or_insert_with(|| decode_image(std::path::Path::new(path)).map(Arc::new))
        .clone()
}

/// Masks in the same plane the flat view draws, skipping hidden objects.
pub fn view_masks(
    entities: &[seal_editor_core::Entity],
    axes: [usize; 2],
    flip: [f32; 2],
    spin_deg: impl Fn(&seal_editor_core::Entity) -> f32,
    root: Option<&std::path::Path>,
) -> Vec<Mask> {
    let mut out = Vec::new();
    for entity in entities {
        if entity.hidden {
            continue;
        }
        let Some(kind) = mask::Kind::parse(&entity.sprite_mask.shape) else {
            continue;
        };
        let half = [0.5 * entity.scale[axes[0]].abs(), 0.5 * entity.scale[axes[1]].abs()];
        let center = [entity.translation[axes[0]] * flip[0], entity.translation[axes[1]] * flip[1]];
        let alpha = if kind == mask::Kind::Alpha {
            let path = std::path::Path::new(&entity.sprite_mask.image);
            let full = match root {
                Some(root) if path.is_relative() => root.join(path),
                _ => path.to_path_buf(),
            };
            image_pixels(&full.to_string_lossy()).map(|px| mask::AlphaImage {
                width: px.1,
                height: px.2,
                rgba: px.0.clone(),
            })
        } else {
            None
        };
        out.push(Mask {
            xform: mask::placed(center, spin_deg(entity).to_radians(), half),
            kind,
            alpha,
            cutoff: entity.sprite_mask.cutoff,
            layer_min: entity.sprite_mask.layer_min,
            layer_max: entity.sprite_mask.layer_max,
            order_min: entity.sprite_mask.order_min,
            order_max: entity.sprite_mask.order_max,
        });
    }
    out
}

/// Bake a masked sprite to a texture. Returns the texture id and screen rectangle.
pub fn clipped_quad(
    ctx: &egui::Context,
    clip: egui::Rect,
    corners: [egui::Pos2; 4],
    uv: [f32; 4],
    color: [u8; 4],
    picture: Option<&str>,
    world_at: impl Fn(f32, f32) -> [f32; 2],
    masks: &[Mask],
    layer: i32,
    order: i32,
    mode: u8,
    key: &str,
) -> Option<(egui::TextureId, [egui::Pos2; 4])> {
    let mode = match mode {
        1 => Interaction::Inside,
        2 => Interaction::Outside,
        _ => return None,
    };
    let local = corners.map(|p| [p.x - clip.min.x, p.y - clip.min.y]);
    let image = picture.and_then(image_pixels);
    let baked = mask::bake(
        clip.width().ceil().max(1.0) as u32,
        clip.height().ceil().max(1.0) as u32,
        local,
        uv,
        color.map(|c| c as f32 / 255.0),
        image.as_deref(),
        |sx, sy| world_at(sx + clip.min.x, sy + clip.min.y),
        masks,
        layer,
        order,
        mode,
    )?;
    let pixels = egui::ColorImage::from_rgba_unmultiplied(
        [baked.width as usize, baked.height as usize],
        &baked.rgba,
    );
    let handle = ctx.load_texture(
        format!("seal-sprite-mask:{key}"),
        pixels,
        egui::TextureOptions::NEAREST,
    );
    let min = egui::pos2(clip.min.x + baked.x as f32, clip.min.y + baked.y as f32);
    let max = min + egui::vec2(baked.width as f32, baked.height as f32);
    let rect = egui::Rect::from_min_max(min, max);
    Some((
        handle.id(),
        [
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
        ],
    ))
}

/// Draw `corners` clipped by `masks`. Returns false when nothing clips this sprite,
/// so the caller draws it the usual way.
pub fn paint_clipped(
    painter: &egui::Painter,
    clip: egui::Rect,
    corners: [egui::Pos2; 4],
    uv: [f32; 4],
    color: [u8; 4],
    picture: Option<&str>,
    world_at: impl Fn(f32, f32) -> [f32; 2],
    masks: &[Mask],
    layer: i32,
    order: i32,
    mode: u8,
    key: &str,
) -> bool {
    let Some((texture, corners)) = clipped_quad(
        painter.ctx(),
        clip,
        corners,
        uv,
        color,
        picture,
        world_at,
        masks,
        layer,
        order,
        mode,
        key,
    ) else {
        return false;
    };
    crate::sprites::paint_quad_uv(painter, texture, corners, [0.0, 0.0, 1.0, 1.0], egui::Color32::WHITE);
    true
}

/// Outline of a mask in the view. `to_screen` maps a plane position to pixels.
/// `kind` is 1 rectangle, 2 circle, 3 picture (the picture's rectangle).
pub fn paint_outline(
    painter: &egui::Painter,
    center: [f32; 2],
    angle: f32,
    half: [f32; 2],
    kind: u8,
    fill: Option<egui::Color32>,
    stroke: egui::Stroke,
    to_screen: impl Fn(f32, f32) -> egui::Pos2,
) {
    if kind == 0 {
        return;
    }
    let xform = mask::placed(center, angle, half);
    let pts: Vec<egui::Pos2> = if kind == 2 {
        (0..32)
            .map(|i| {
                let t = i as f32 / 32.0 * std::f32::consts::TAU;
                let p = xform.apply([0.5 * t.cos(), 0.5 * t.sin()]);
                to_screen(p[0], p[1])
            })
            .collect()
    } else {
        [[-0.5, 0.5], [0.5, 0.5], [0.5, -0.5], [-0.5, -0.5]]
            .map(|p| {
                let w = xform.apply(p);
                to_screen(w[0], w[1])
            })
            .to_vec()
    };
    if let Some(fill) = fill {
        if kind == 2 {
            let mut mesh = egui::Mesh::default();
            let mid = xform.apply([0.0, 0.0]);
            mesh.colored_vertex(to_screen(mid[0], mid[1]), fill);
            for p in &pts {
                mesh.colored_vertex(*p, fill);
            }
            for i in 0..pts.len() as u32 {
                mesh.add_triangle(0, i + 1, (i + 1) % pts.len() as u32 + 1);
            }
            painter.add(egui::Shape::mesh(mesh));
        } else if pts.len() == 4 {
            let mut mesh = egui::Mesh::default();
            for p in &pts {
                mesh.colored_vertex(*p, fill);
            }
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            painter.add(egui::Shape::mesh(mesh));
        }
    }
    painter.add(egui::Shape::closed_line(pts, stroke));
}
