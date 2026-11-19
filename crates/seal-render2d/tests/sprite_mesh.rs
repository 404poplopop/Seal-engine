//! A tight mesh draws fewer pixels than the full rectangle and still covers its centre.

use seal_ecs::core::Transform;
use seal_ecs::World;
use seal_render::{Device, SoftDevice, TextureDesc};
use seal_render2d::{register_2d, Renderer2D, Sprite};

#[test]
fn a_tight_mesh_skips_the_empty_corners() {
    let mut world = World::new();
    world.register_core();
    register_2d(&mut world);
    world.spawn((
        Transform::default(),
        Sprite {
            image: "blob".into(),
            size: [10.0, 10.0],
            color: [1.0, 0.0, 0.0, 1.0],
            ..Default::default()
        },
    ));
    let mut device = SoftDevice::new();
    let target = device.create_texture(TextureDesc::rgba8(20, 20));
    let mut full = Renderer2D::new();
    full.load_image(&mut device, "blob", 1, 1, &[255, 0, 0, 255]);
    full.render(&mut device, &mut world, target);
    let full_red = device.read_back(target).chunks(4).filter(|px| px[0] == 255).count();

    device.clear(target, [0.0, 0.0, 0.0, 1.0]);
    let mut tight = Renderer2D::new();
    tight.load_image(&mut device, "blob", 1, 1, &[255, 0, 0, 255]);
    tight.set_mesh("blob", vec![[0.25, 0.25], [0.75, 0.25], [0.75, 0.75], [0.25, 0.75]]);
    let stats = tight.render(&mut device, &mut world, target);
    assert_eq!(stats.sprites, 1);
    let pixels = device.read_back(target);
    let red = pixels.chunks(4).filter(|px| px[0] == 255).count();
    assert!(red < full_red, "tight {red} full {full_red}");
    assert!(red > 0, "{stats:?}");
    let center = {
        let i = (10 * 20 + 10) * 4;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    };
    assert_eq!(center, [255, 0, 0, 255]);
}
