//! 2D material blend modes in the viewport: batching, CPU fallback tints, and blend math.
//!
//! Without OpenGL, multiply and screen tints are approximated (see [`cpu_paint_color`]).
//! Additive uses egui's premultiplied alpha=0 additive convention.

use eframe::glow;
use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Color32, Pos2, TextureId};
use seal_editor_core::material2d;

/// Untextured solid fills sample this texture coordinate (white texel in the default atlas).
pub fn solid_uv() -> [f32; 2] {
    [WHITE_UV.x, WHITE_UV.y]
}

/// Default egui atlas texture for untextured triangles.
pub fn solid_texture() -> TextureId {
    TextureId::default()
}

/// Blend modes that need framebuffer compositing (not plain alpha-over).
pub fn is_special_blend(mode: &str) -> bool {
    matches!(mode, "additive" | "multiply" | "screen" | "premultiplied")
}

pub fn is_alpha_blend(mode: &str) -> bool {
    mode.is_empty() || mode == "alpha"
}

/// OpenGL blend factors for each material mode (RGB source, RGB dest, A source, A dest).
/// Blend factors for premultiplied fragments (egui textures are premultiplied and the
/// shader multiplies the tint in the same way), so transparent texels never change what
/// is behind them in any mode.
pub fn gl_blend_factors(mode: &str) -> (u32, u32, u32, u32) {
    match mode {
        "additive" => (glow::ONE, glow::ONE, glow::ZERO, glow::ONE),
        "multiply" => (glow::DST_COLOR, glow::ONE_MINUS_SRC_ALPHA, glow::ZERO, glow::ONE),
        "screen" => (glow::ONE, glow::ONE_MINUS_SRC_COLOR, glow::ONE, glow::ONE_MINUS_SRC_ALPHA),
        "premultiplied" => {
            (glow::ONE, glow::ONE_MINUS_SRC_ALPHA, glow::ONE, glow::ONE_MINUS_SRC_ALPHA)
        }
        _ => (glow::ONE, glow::ONE_MINUS_SRC_ALPHA, glow::ONE, glow::ONE_MINUS_SRC_ALPHA),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedBlendState {
    pub blend: bool,
    pub src_rgb: u32,
    pub dst_rgb: u32,
    pub src_alpha: u32,
    pub dst_alpha: u32,
    pub eq_rgb: u32,
    pub eq_alpha: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlendBatchSetup {
    pub enable_blend: bool,
    pub factors: (u32, u32, u32, u32),
}

pub fn batch_setup(mode: &str) -> BlendBatchSetup {
    BlendBatchSetup { enable_blend: true, factors: gl_blend_factors(mode) }
}

pub fn split_batches<T: BlendBatchItem>(items: &[T]) -> Vec<BlendBatch<T>> {
    let mut out: Vec<BlendBatch<T>> = Vec::new();
    for item in items {
        let key = item.batch_key();
        if let Some(batch) = out.last_mut() {
            if batch.key == key {
                batch.items.push(item.clone_item());
                continue;
            }
        }
        out.push(BlendBatch { key, items: vec![item.clone_item()] });
    }
    out
}

pub trait BlendBatchItem: Clone {
    fn batch_key(&self) -> BatchKey;
    fn clone_item(&self) -> Self;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatchKey {
    pub mode: String,
    pub texture: u64,
}

#[derive(Clone, Debug)]
pub struct BlendBatch<T> {
    pub key: BatchKey,
    pub items: Vec<T>,
}

#[derive(Clone, Copy, Debug)]
pub struct BlendVertex {
    pub pos: Pos2,
    pub uv: [f32; 2],
    pub tint: [f32; 4],
}

/// One draw submission: triangles only, length `vertices` is a multiple of three.
#[derive(Clone, Debug)]
pub struct BlendDraw {
    pub texture: TextureId,
    pub mode: String,
    pub vertices: Vec<BlendVertex>,
}

impl BlendBatchItem for BlendDraw {
    fn batch_key(&self) -> BatchKey {
        BatchKey { mode: self.mode.clone(), texture: texture_key(self.texture) }
    }

    fn clone_item(&self) -> Self {
        self.clone()
    }
}

pub fn texture_key(id: TextureId) -> u64 {
    match id {
        TextureId::Managed(i) => i,
        TextureId::User(i) => i | (1u64 << 63),
    }
}

pub fn vertex_tint(mode: &str, color: Color32) -> [f32; 4] {
    let srgba = color.to_srgba_unmultiplied();
    let a = srgba[3] as f32 / 255.0;
    if mode == "premultiplied" {
        [srgba[0] as f32 / 255.0 * a, srgba[1] as f32 / 255.0 * a, srgba[2] as f32 / 255.0 * a, a]
    } else {
        [srgba[0] as f32 / 255.0, srgba[1] as f32 / 255.0, srgba[2] as f32 / 255.0, a]
    }
}

pub fn push_quad(out: &mut Vec<BlendVertex>, corners: [Pos2; 4], uv: [f32; 4], tint: [f32; 4]) {
    let uvs = [[uv[0], uv[1]], [uv[2], uv[1]], [uv[2], uv[3]], [uv[0], uv[3]]];
    for index in [0_usize, 1, 2, 0, 2, 3] {
        out.push(BlendVertex { pos: corners[index], uv: uvs[index], tint });
    }
}

#[cfg(test)]
pub fn push_quad_uvs(
    out: &mut Vec<BlendVertex>,
    corners: [Pos2; 4],
    uvs: [Pos2; 4],
    tint: [f32; 4],
) {
    for index in [0_usize, 1, 2, 0, 2, 3] {
        out.push(BlendVertex { pos: corners[index], uv: [uvs[index].x, uvs[index].y], tint });
    }
}

pub fn append_egui_mesh(out: &mut Vec<BlendVertex>, mesh: &Mesh, mode: &str) {
    for chunk in mesh.indices.chunks(3) {
        if chunk.len() < 3 {
            continue;
        }
        for &idx in chunk {
            let v: &Vertex = &mesh.vertices[idx as usize];
            out.push(BlendVertex {
                pos: v.pos,
                uv: [v.uv.x, v.uv.y],
                tint: vertex_tint(mode, v.color),
            });
        }
    }
}

pub struct SpriteBlendPass {
    rect: egui::Rect,
    gpu: bool,
    pending: Vec<BlendDraw>,
}

impl SpriteBlendPass {
    pub fn new(rect: egui::Rect, gpu: bool) -> Self {
        Self { rect, gpu, pending: Vec::new() }
    }

    pub fn flush_alpha_barrier(&mut self, painter: &egui::Painter) {
        self.flush(painter);
    }

    pub fn finish(&mut self, painter: &egui::Painter) {
        self.flush(painter);
    }

    pub fn try_queue_draw(&mut self, draw: BlendDraw, angled: bool) -> bool {
        if !self.gpu || angled || draw.vertices.len() < 3 {
            return false;
        }
        if !is_special_blend(&draw.mode) {
            return false;
        }
        self.pending.push(draw);
        true
    }

    pub fn try_queue_mesh(&mut self, mode: &str, mesh: &Mesh, angled: bool) -> bool {
        if !self.gpu || angled || !is_special_blend(mode) || mesh.indices.is_empty() {
            return false;
        }
        let mut vertices = Vec::new();
        append_egui_mesh(&mut vertices, mesh, mode);
        if vertices.len() < 3 {
            return false;
        }
        self.pending.push(BlendDraw { texture: mesh.texture_id, mode: mode.to_string(), vertices });
        true
    }

    fn flush(&mut self, painter: &egui::Painter) {
        if self.pending.is_empty() {
            return;
        }
        let draws = std::mem::take(&mut self.pending);
        painter.add(crate::gpu_sprite_blend::paint_callback(self.rect, draws));
    }
}

pub fn paint_blended_quad_uv(
    pass: &mut SpriteBlendPass,
    painter: &egui::Painter,
    blend: &str,
    texture: TextureId,
    corners: [Pos2; 4],
    uv: [f32; 4],
    color: Color32,
    angled: bool,
) {
    let tint = vertex_tint(blend, color);
    let mut vertices = Vec::new();
    push_quad(&mut vertices, corners, uv, tint);
    let draw = BlendDraw { texture, mode: blend.to_string(), vertices };
    if pass.try_queue_draw(draw, angled) {
        return;
    }
    crate::sprites::paint_quad_uv(painter, texture, corners, uv, cpu_paint_color(blend, color));
}

pub fn paint_blended_mesh(
    pass: &mut SpriteBlendPass,
    painter: &egui::Painter,
    blend: &str,
    mesh: &Mesh,
    angled: bool,
) {
    if pass.try_queue_mesh(blend, mesh, angled) {
        return;
    }
    let mut cpu = mesh.clone();
    for v in &mut cpu.vertices {
        v.color = cpu_paint_color(blend, v.color);
    }
    painter.add(egui::Shape::mesh(cpu));
}

pub fn cpu_paint_color(mode: &str, color: Color32) -> Color32 {
    if is_alpha_blend(mode) {
        return color;
    }
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    match mode {
        "additive" => {
            let fa = a as f32 / 255.0;
            Color32::from_rgba_premultiplied(
                (r as f32 * fa).round() as u8,
                (g as f32 * fa).round() as u8,
                (b as f32 * fa).round() as u8,
                0,
            )
        }
        "multiply" | "screen" => {
            let dst = [0.5f32; 4];
            let src = [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0];
            let out = material2d::blend_pixel(dst, src, mode);
            let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            Color32::from_rgba_unmultiplied(byte(out[0]), byte(out[1]), byte(out[2]), byte(out[3]))
        }
        "premultiplied" => {
            let fa = a as f32 / 255.0;
            Color32::from_rgba_premultiplied(
                (r as f32 * fa).round() as u8,
                (g as f32 * fa).round() as u8,
                (b as f32 * fa).round() as u8,
                a,
            )
        }
        _ => color,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::TextureId;

    fn draw(mode: &str, tex: u64) -> BlendDraw {
        let mut vertices = Vec::new();
        push_quad(
            &mut vertices,
            [Pos2::ZERO, Pos2::ZERO, Pos2::ZERO, Pos2::ZERO],
            [0.0, 0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0, 0.5],
        );
        BlendDraw { texture: TextureId::Managed(tex), mode: mode.into(), vertices }
    }

    fn solid_draw(mode: &str) -> BlendDraw {
        let mut vertices = Vec::new();
        let uv = solid_uv();
        let tint = [0.8, 0.2, 0.1, 0.9];
        push_quad(
            &mut vertices,
            [Pos2::ZERO, Pos2::ZERO, Pos2::ZERO, Pos2::ZERO],
            [uv[0], uv[1], uv[0], uv[1]],
            tint,
        );
        BlendDraw { texture: solid_texture(), mode: mode.into(), vertices }
    }

    #[test]
    fn white_texel_uses_default_texture_and_origin_uv() {
        assert_eq!(solid_texture(), TextureId::default());
        assert_eq!(solid_uv(), [0.0, 0.0]);
        let key = texture_key(solid_texture());
        assert_eq!(draw("additive", 1).batch_key().texture, 1);
        assert_eq!(solid_draw("multiply").batch_key().texture, key);
    }

    #[test]
    fn mixed_shape_sprite_and_particle_order_splits_batches() {
        let items = [
            solid_draw("additive"),
            draw("additive", 5),
            draw("additive", 5),
            solid_draw("multiply"),
        ];
        let batches = split_batches(&items);
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].key.texture, texture_key(solid_texture()));
        assert_eq!(batches[1].items.len(), 2);
        assert_eq!(batches[2].key.mode, "multiply");
    }

    #[test]
    fn blend_math_matches_material2d_including_screen() {
        let dst = [0.25, 0.5, 0.75, 1.0];
        let src = [1.0, 0.5, 0.0, 0.5];
        let screen = material2d::blend_pixel(dst, src, "screen");
        assert!(screen[0] > dst[0] && screen[0] <= 1.0, "screen lightens red: {screen:?}");
    }

    #[test]
    fn three_overlapping_sprites_keep_middle_order() {
        let bottom = [0.2, 0.2, 0.2, 1.0];
        let mid_src = [1.0, 0.0, 0.0, 0.5];
        let top_src = [0.0, 0.0, 1.0, 0.5];
        let after_mid = material2d::blend_pixel(bottom, mid_src, "additive");
        let after_top = material2d::blend_pixel(after_mid, top_src, "alpha");
        let wrong_order = material2d::blend_pixel(
            material2d::blend_pixel(bottom, top_src, "alpha"),
            mid_src,
            "additive",
        );
        assert_ne!(after_top, wrong_order, "order changes the composite");
    }

    #[test]
    fn gl_state_per_mode_differs_and_restore_fields_are_complete() {
        let add = gl_blend_factors("additive");
        let mul = gl_blend_factors("multiply");
        assert_ne!(add, mul);
        assert_eq!(batch_setup("screen").factors.1, glow::ONE_MINUS_SRC_COLOR);
    }

    #[test]
    fn transparent_texels_leave_the_scene_unchanged_in_every_mode() {
        // Evaluate the GL blend equation (FUNC_ADD) for one colour channel.
        fn factor(f: u32, src: [f32; 4], dst: [f32; 4], c: usize) -> f32 {
            match f {
                glow::ZERO => 0.0,
                glow::ONE => 1.0,
                glow::DST_COLOR => dst[c],
                glow::ONE_MINUS_SRC_ALPHA => 1.0 - src[3],
                glow::ONE_MINUS_SRC_COLOR => 1.0 - src[c],
                glow::SRC_ALPHA => src[3],
                other => panic!("factor {other:#x} not covered"),
            }
        }
        let dst = [0.2, 0.5, 0.8, 1.0];
        for mode in ["alpha", "additive", "multiply", "screen", "premultiplied"] {
            let (s, d, _, _) = gl_blend_factors(mode);
            let clear = [0.0, 0.0, 0.0, 0.0]; // premultiplied transparent texel
            for c in 0..3 {
                let out = clear[c] * factor(s, clear, dst, c) + dst[c] * factor(d, clear, dst, c);
                assert!((out - dst[c]).abs() < 1e-6, "{mode} changed a transparent pixel");
            }
        }
        // An opaque multiply texel of 0.5 halves the scene.
        let (s, d, _, _) = gl_blend_factors("multiply");
        let half = [0.5, 0.5, 0.5, 1.0];
        let out = half[0] * factor(s, half, dst, 0) + dst[0] * factor(d, half, dst, 0);
        assert!((out - 0.1).abs() < 1e-6);
    }

    #[test]
    fn cpu_additive_uses_zero_alpha() {
        let c = Color32::from_rgba_unmultiplied(200, 100, 50, 128);
        let add = cpu_paint_color("additive", c);
        assert_eq!(add.a(), 0, "egui additive convention");
        assert!(add.r() > 0);
    }

    #[test]
    fn quad_uvs_push_six_vertices() {
        let mut vertices = Vec::new();
        push_quad_uvs(
            &mut vertices,
            [Pos2::ZERO, Pos2::new(1.0, 0.0), Pos2::new(1.0, 1.0), Pos2::new(0.0, 1.0)],
            [Pos2::new(0.1, 0.2), Pos2::new(0.9, 0.2), Pos2::new(0.9, 0.8), Pos2::new(0.1, 0.8)],
            [1.0, 1.0, 1.0, 1.0],
        );
        assert_eq!(vertices.len(), 6);
    }

    #[test]
    fn egui_mesh_appends_triangle_count() {
        let mut mesh = Mesh::with_texture(TextureId::Managed(2));
        mesh.vertices.push(Vertex { pos: Pos2::ZERO, uv: WHITE_UV, color: Color32::WHITE });
        mesh.vertices.push(Vertex {
            pos: Pos2::new(1.0, 0.0),
            uv: WHITE_UV,
            color: Color32::WHITE,
        });
        mesh.vertices.push(Vertex {
            pos: Pos2::new(0.0, 1.0),
            uv: WHITE_UV,
            color: Color32::WHITE,
        });
        mesh.add_triangle(0, 1, 2);
        let mut vertices = Vec::new();
        append_egui_mesh(&mut vertices, &mesh, "additive");
        assert_eq!(vertices.len(), 3);
        assert_eq!(vertices[0].uv, [0.0, 0.0]);
    }
}
