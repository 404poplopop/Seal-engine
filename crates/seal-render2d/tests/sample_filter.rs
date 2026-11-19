//! Picture sampling: nearest keeps hard pixels, linear blends them.

use seal_ecs::core::Transform;
use seal_ecs::World;
use seal_render::{Device, SoftDevice, TextureDesc, TextureFilter};
use seal_render2d::{register_2d, Renderer2D, Sprite};

fn px(frame: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let i = (y * width + x) * 4;
    [frame[i], frame[i + 1], frame[i + 2], frame[i + 3]]
}

fn frame(filter: TextureFilter) -> Vec<u8> {
    let mut world = World::new();
    world.register_core();
    register_2d(&mut world);
    // 2×2 picture stretched over an 8×8 target. The default camera is 1 pixel per unit.
    let rgba = [
        255, 0, 0, 255, 0, 255, 0, 255, //
        0, 0, 255, 255, 255, 255, 255, 255,
    ];
    world.spawn((
        Transform { translation: [0.0, 0.0, 0.0], ..Default::default() },
        Sprite { image: "pix".into(), size: [8.0, 8.0], ..Default::default() },
    ));
    let mut device = SoftDevice::new();
    let target = device.create_texture(TextureDesc::rgba8(8, 8));
    let mut renderer = Renderer2D::new();
    renderer.load_image_filtered(&mut device, "pix", 2, 2, &rgba, filter);
    renderer.render(&mut device, &mut world, target);
    device.read_back(target)
}

#[test]
fn nearest_and_linear_disagree_when_a_picture_is_scaled_up() {
    let nearest = frame(TextureFilter::Nearest);
    let linear = frame(TextureFilter::Linear);
    // Historical sampling is nearest. The default import filter is nearest, so a
    // picture with no sidecar draws the same pixels as before.
    let legacy = frame(TextureFilter::default());
    assert_eq!(nearest, legacy);
    // Deep in the top-left pixel the sample is that pixel. On the shared edge it blends.
    assert_eq!(px(&nearest, 8, 1, 1), [255, 0, 0, 255]);
    assert_eq!(px(&nearest, 8, 3, 3), [255, 0, 0, 255]);
    assert_ne!(px(&linear, 8, 3, 3), [255, 0, 0, 255]);
    assert_eq!(nearest.len(), linear.len());
}
