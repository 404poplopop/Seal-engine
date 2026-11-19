//! GPU upload checks layer counts on a headless device when available.

use seal_render::{Device, TextureDesc};
use seal_render_wgpu::WgpuDevice;

fn gpu() -> Option<WgpuDevice> {
    WgpuDevice::headless().ok()
}

#[test]
fn array_texture_has_four_layers() {
    let Some(mut device) = gpu() else {
        eprintln!("skipping: no GPU");
        return;
    };
    let id = device.create_texture(TextureDesc::array_rgba8(4, 4, 4));
    assert_eq!(device.texture_layer_count(id), 4);
    assert_eq!(device.texture_dimension(id), wgpu::TextureDimension::D2);
}

#[test]
fn volume_texture_is_d3() {
    let Some(mut device) = gpu() else {
        eprintln!("skipping: no GPU");
        return;
    };
    let id = device.create_texture(TextureDesc::volume_rgba8(4, 4, 3));
    assert_eq!(device.texture_layer_count(id), 3);
    assert_eq!(device.texture_dimension(id), wgpu::TextureDimension::D3);
}

#[test]
fn cubemap_has_six_faces() {
    let Some(mut device) = gpu() else {
        eprintln!("skipping: no GPU");
        return;
    };
    let id = device.create_texture(TextureDesc::cubemap_rgba8(8));
    assert_eq!(device.texture_layer_count(id), 6);
}
