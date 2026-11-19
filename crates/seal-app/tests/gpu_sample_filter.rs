#![cfg(feature = "window")]
//! Picture sampling on the GPU device: nearest vs linear.

use seal_ecs::core::Transform;
use seal_ecs::World;
use seal_render::{Device, TextureDesc, TextureFilter};
use seal_render2d::{register_2d, Renderer2D, Sprite};
use seal_render_wgpu::WgpuDevice;

fn px(frame: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let i = (y * width + x) * 4;
    [frame[i], frame[i + 1], frame[i + 2], frame[i + 3]]
}

fn frame(device: &mut WgpuDevice, filter: TextureFilter) -> Vec<u8> {
    let mut world = World::new();
    world.register_core();
    register_2d(&mut world);
    let rgba = [
        255, 0, 0, 255, 0, 255, 0, 255, //
        0, 0, 255, 255, 255, 255, 255, 255,
    ];
    world.spawn((
        Transform { translation: [0.0, 0.0, 0.0], ..Default::default() },
        Sprite { image: "pix".into(), size: [8.0, 8.0], ..Default::default() },
    ));
    let target = device.create_texture(TextureDesc::rgba8(8, 8));
    let mut renderer = Renderer2D::new();
    renderer.load_image_filtered(device, "pix", 2, 2, &rgba, filter);
    renderer.render(device, &mut world, target);
    device.read_back(target)
}

#[test]
fn nearest_and_linear_disagree_on_the_gpu() {
    let Some(mut device) = (match WgpuDevice::headless() {
        Ok(d) => Some(d),
        Err(e) => {
            eprintln!("skipping GPU filter test: {e}");
            None
        }
    }) else {
        return;
    };
    let nearest = frame(&mut device, TextureFilter::Nearest);
    let linear = frame(&mut device, TextureFilter::Linear);
    assert_eq!(px(&nearest, 8, 1, 1), [255, 0, 0, 255]);
    assert_ne!(px(&linear, 8, 3, 3), [255, 0, 0, 255]);
    assert_eq!(nearest.len(), linear.len());
}
