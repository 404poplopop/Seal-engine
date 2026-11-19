//! Lit viewport triangles on the game window's graphics device.
//!
//! Used when the frame is drawn by that device. The ordinary path is unchanged
//! and still draws when this one is off.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use egui::Rect;

use crate::viewport_mesh::LitTriangle;

/// Floats in one vertex. Must match [`vertex_data`].
pub const VERTEX_FLOATS: usize = 22;

/// Bytes in one vertex.
pub const VERTEX_STRIDE: u64 = (VERTEX_FLOATS * 4) as u64;

/// Scene target the mesh pass draws into. Values above 1 are kept.
pub const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Depth format shared with the frame.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;

pub const MESH_WGSL: &str = include_str!("mesh_viewport.wgsl");
pub const COMPOSITE_WGSL: &str = include_str!("frame_composite.wgsl");


static ACTIVE: AtomicBool = AtomicBool::new(false);
static HDR: AtomicBool = AtomicBool::new(false);

/// The frame is being drawn by the graphics device, so lit triangles use this pass.
pub fn set_active(on: bool) {
    ACTIVE.store(on, Ordering::Relaxed);
}

pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// When set, mesh colours above 1 are kept. When clear, they are clipped to 1.
pub fn set_hdr(on: bool) {
    HDR.store(on, Ordering::Relaxed);
}

pub fn hdr() -> bool {
    HDR.load(Ordering::Relaxed)
}


/// Draw `tris` with a depth test, albedo textures and the shadow map.
pub fn paint_callback(
    rect: Rect,
    tris: &[LitTriangle],
    shadow: Option<&seal_editor_core::shadow::ShadowMap>,
    frame: crate::gpu_viewport::GpuFrame,
) -> egui::PaintCallback {
    crate::gpu_mesh_pass::paint_callback(rect, tris, shadow, frame)
}

pub fn sky_callback(rect: Rect, frame: crate::gpu_viewport::GpuFrame) -> egui::PaintCallback {
    crate::gpu_mesh_pass::sky_callback(rect, frame)
}

pub fn preview_callback(
    rect: Rect,
    rgba: Arc<Vec<u8>>,
    width: u32,
    height: u32,
    format: seal_editor_core::GpuBlockFormat,
) -> egui::PaintCallback {
    crate::gpu_mesh_pass::preview_callback(rect, rgba, width, height, format)
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu_viewport::vertex_data;
    use egui::{Color32, Pos2};

    fn one_triangle() -> (egui::Rect, LitTriangle) {
        let rect = egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(80.0, 40.0));
        let tri = LitTriangle {
            points: [Pos2::ZERO, Pos2::new(80.0, 0.0), Pos2::new(80.0, 40.0)],
            depth: 1.0,
            depths: [1.0, 2.0, 4.0],
            color: Color32::WHITE,
            dark: Color32::BLACK,
            shadow_uvz: [[0.2, 0.3, 0.4]; 3],
            shadow_bias: 0.02,
            uv: [[0.0; 2]; 3],
            tex_layer: 0,
            tex_slice: 0.0,
            tex_path: String::new(),
            tex_kind: crate::gpu_textures::SampleKind::None,
            normal: [0.0, 0.0, 1.0],
            light: 2.5,
        };
        (rect, tri)
    }

    fn parses(source: &str) {
        let module = naga::front::wgsl::parse_str(source).expect("parse");
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        );
        validator.validate(&module).expect("validate");
        let names: Vec<_> = module.entry_points.iter().map(|entry| entry.name.as_str()).collect();
        assert!(names.contains(&"vs"));
        assert!(names.contains(&"fs"));
    }

    #[test]
    fn mesh_and_composite_shaders_are_valid() {
        parses(MESH_WGSL);
        parses(COMPOSITE_WGSL);
    }

    #[test]
    fn vertex_layout_matches_vertex_data() {
        let (rect, tri) = one_triangle();
        let data = vertex_data(&[tri], rect).0;
        assert_eq!(data.len(), 3 * VERTEX_FLOATS);
        assert_eq!(VERTEX_STRIDE, (VERTEX_FLOATS * std::mem::size_of::<f32>()) as u64);
        assert_eq!(crate::gpu_mesh_pass::MESH_ATTRS[0].offset, 0);
        assert_eq!(crate::gpu_mesh_pass::MESH_ATTRS[3].offset, 11 * 4);
        assert_eq!(crate::gpu_mesh_pass::MESH_ATTRS[4].offset, 15 * 4);
        assert_eq!(crate::gpu_mesh_pass::MESH_ATTRS[6].offset, 22 * 4);
    }

    fn callback_is_glow(callback: &egui::PaintCallback) -> bool {
        callback.callback.downcast_ref::<eframe::egui_glow::CallbackFn>().is_some()
    }

    fn callback_is_device(callback: &egui::PaintCallback) -> bool {
        callback.callback.downcast_ref::<egui_wgpu::Callback>().is_some()
    }

    #[test]
    fn every_glow_callback_switches_with_the_device_frame() {
        let (rect, tri) = one_triangle();
        let frame = crate::gpu_viewport::GpuFrame {
            anisotropy: 4,
            sky: String::new(),
            project: None,
            yaw_deg: 10.0,
            pitch_deg: -5.0,
            look_through: false,
            render_scale: 100,
        };
        set_active(false);
        assert!(callback_is_glow(&crate::gpu_viewport::paint_callback(rect, &[tri], None, frame.clone())));
        assert!(callback_is_glow(&crate::gpu_viewport::sky_callback(rect, frame.clone())));
        assert!(callback_is_glow(&crate::gpu_sprite_blend::paint_callback(rect, Vec::new())));
        assert!(callback_is_glow(&crate::gpu_textures::preview_callback(
            rect,
            std::sync::Arc::new(vec![255, 0, 0, 255]),
            1,
            1,
            seal_editor_core::GpuBlockFormat::Rgba8,
        )));
        let (rect, tri) = one_triangle();
        set_active(true);
        assert!(callback_is_device(&crate::gpu_viewport::paint_callback(rect, &[tri], None, frame.clone())));
        assert!(callback_is_device(&crate::gpu_viewport::sky_callback(rect, frame)));
        assert!(callback_is_device(&crate::gpu_sprite_blend::paint_callback(rect, Vec::new())));
        assert!(callback_is_device(&crate::gpu_textures::preview_callback(
            rect,
            std::sync::Arc::new(vec![255, 0, 0, 255]),
            1,
            1,
            seal_editor_core::GpuBlockFormat::Bc1,
        )));
        set_active(false);
    }

    #[test]
    fn sky_preview_and_sprite_shaders_are_valid() {
        parses(crate::gpu_mesh_pass::SKY_WGSL);
        parses(crate::gpu_mesh_pass::PREVIEW_WGSL);
        parses(crate::gpu_sprite_wgpu::SPRITE_WGSL);
    }

    #[test]
    fn a_bright_light_is_carried_beside_the_clamped_colour() {
        assert_eq!(crate::scene_light_gain(1.0, false, 4.0), 1.0);
        assert_eq!(crate::scene_light_gain(6.0, false, 1.0), 6.0);
        assert_eq!(crate::scene_light_gain(2.0, true, 4.0), 4.0);
        let (rect, tri) = one_triangle();
        assert_eq!(tri.color, egui::Color32::WHITE);
        assert_eq!(tri.light, 2.5);
        let (ext, _) = crate::gpu_mesh_pass::with_light(&[tri], rect);
        assert_eq!(ext.len(), 3 * 23);
        assert_eq!(ext[22], 2.5);
    }

    #[test]
    fn the_frame_selects_the_paint_callback() {
        let (rect, tri) = one_triangle();
        set_active(false);
        let standard = crate::gpu_viewport::paint_callback(rect, &[tri], None, crate::gpu_viewport::GpuFrame {
            anisotropy: 1,
            sky: String::new(),
            project: None,
            yaw_deg: 0.0,
            pitch_deg: 0.0,
            look_through: false,
            render_scale: 100,
        });
        assert!(standard.callback.downcast_ref::<eframe::egui_glow::CallbackFn>().is_some());
        let (rect, tri) = one_triangle();
        set_active(true);
        set_hdr(true);
        let device = crate::gpu_viewport::paint_callback(rect, &[tri], None, crate::gpu_viewport::GpuFrame {
            anisotropy: 1,
            sky: String::new(),
            project: None,
            yaw_deg: 0.0,
            pitch_deg: 0.0,
            look_through: false,
            render_scale: 100,
        });
        assert!(device.callback.downcast_ref::<egui_wgpu::Callback>().is_some());
        set_active(false);
        set_hdr(false);
    }
}
