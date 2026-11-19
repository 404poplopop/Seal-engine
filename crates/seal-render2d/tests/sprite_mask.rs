//! Sprite masks clip pixels in the same renderer the game uses.

use seal_ecs::core::Transform;
use seal_ecs::World;
use seal_render::{Device, SoftDevice, TextureDesc};
use seal_render2d::*;

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

fn world() -> World {
    let mut w = World::new();
    w.register_core();
    register_2d(&mut w);
    w
}

fn at(x: f32, y: f32) -> Transform {
    Transform { translation: [x, y, 0.0], ..Default::default() }
}

fn turned(x: f32, y: f32, degrees: f32, scale: [f32; 2]) -> Transform {
    Transform {
        translation: [x, y, 0.0],
        rotation: [0.0, 0.0, degrees],
        scale: [scale[0], scale[1], 1.0],
    }
}

fn solid(mask: &str, layer: i32, order: i32) -> Sprite {
    Sprite { size: [10.0, 10.0], color: RED, layer, order, mask: mask.into(), ..Default::default() }
}

fn rect_mask(layer_min: i32, layer_max: i32) -> SpriteMask {
    SpriteMask { shape: "rect".into(), layer_min, layer_max, ..Default::default() }
}

fn px(p: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let i = (y * width + x) * 4;
    [p[i], p[i + 1], p[i + 2], p[i + 3]]
}

fn draw(w: &mut World) -> Vec<u8> {
    let mut device = SoftDevice::new();
    let target = device.create_texture(TextureDesc::rgba8(20, 20));
    Renderer2D::new().render(&mut device, w, target);
    device.read_back(target)
}

/// World position of a pixel centre on a 20x20 target with the default camera.
fn plane(x: usize, y: usize) -> [f32; 2] {
    [x as f32 + 0.5 - 10.0, 10.0 - (y as f32 + 0.5)]
}

#[test]
fn inside_and_outside_a_rectangle() {
    let mut w = world();
    w.spawn((at(0.0, 0.0), solid("inside", 0, 0)));
    w.spawn((turned(0.0, 0.0, 0.0, [4.0, 4.0]), rect_mask(-1000, 1000)));
    let pixels = draw(&mut w);
    // Clearly inside the mask (half-size 2) and clearly in the sprite (half-size 5).
    assert_eq!(px(&pixels, 20, 10, 10), [255, 0, 0, 255], "centre stays");
    // Sprite corner, outside the mask.
    assert_eq!(px(&pixels, 20, 6, 6), [0, 0, 0, 255], "outside the rectangle is hidden");

    let mut w = world();
    w.spawn((at(0.0, 0.0), solid("outside", 0, 0)));
    w.spawn((turned(0.0, 0.0, 0.0, [4.0, 4.0]), rect_mask(-1000, 1000)));
    let pixels = draw(&mut w);
    assert_eq!(px(&pixels, 20, 10, 10), [0, 0, 0, 255], "centre is covered, so hidden");
    assert_eq!(px(&pixels, 20, 6, 6), [255, 0, 0, 255], "outside the rectangle stays");
    let _ = plane(0, 0);
}

#[test]
fn a_circle_and_a_turned_rectangle() {
    let mut w = world();
    w.spawn((at(0.0, 0.0), solid("inside", 0, 0)));
    w.spawn((
        turned(0.0, 0.0, 0.0, [4.0, 4.0]),
        SpriteMask { shape: "circle".into(), ..Default::default() },
    ));
    let pixels = draw(&mut w);
    assert_eq!(px(&pixels, 20, 10, 10), [255, 0, 0, 255]);
    // Near a corner of the bounding square, outside the circle.
    assert_eq!(px(&pixels, 20, 8, 8), [0, 0, 0, 255]);

    let mut w = world();
    w.spawn((at(0.0, 0.0), solid("inside", 0, 0)));
    // A wide flat bar turned to stand upright.
    w.spawn((
        turned(0.0, 0.0, 90.0, [6.0, 2.0]),
        SpriteMask { shape: "rect".into(), ..Default::default() },
    ));
    let pixels = draw(&mut w);
    assert_eq!(px(&pixels, 20, 10, 8), [255, 0, 0, 255], "along the turned bar");
    assert_eq!(px(&pixels, 20, 13, 10), [0, 0, 0, 255], "beside the turned bar");
}

#[test]
fn picture_alpha_and_a_union_and_a_layer_limit() {
    let rgba = vec![
        255, 0, 0, 255, 0, 0, 0, 0, //
        255, 0, 0, 255, 0, 0, 0, 0,
    ];
    let mut w = world();
    w.spawn((at(0.0, 0.0), solid("inside", 0, 0)));
    w.spawn((
        turned(0.0, 0.0, 0.0, [4.0, 4.0]),
        SpriteMask { shape: "alpha".into(), image: "hole".into(), cutoff: 0.5, ..Default::default() },
    ));
    let mut device = SoftDevice::new();
    let mut renderer = Renderer2D::new();
    renderer.load_image(&mut device, "hole", 2, 1, &rgba[..8]);
    // The 2x1 picture above is only the first row; load a 2x2 with a clear right half.
    let rgba = vec![
        255, 0, 0, 255, 0, 0, 0, 0, //
        255, 0, 0, 255, 0, 0, 0, 0,
    ];
    renderer.load_image(&mut device, "hole", 2, 2, &rgba);
    let target = device.create_texture(TextureDesc::rgba8(20, 20));
    renderer.render(&mut device, &mut w, target);
    let pixels = device.read_back(target);
    assert_eq!(px(&pixels, 20, 9, 10), [255, 0, 0, 255], "opaque half");
    assert_eq!(px(&pixels, 20, 12, 10), [0, 0, 0, 255], "clear half");

    // Two rectangles side by side: the union shows both, the gap stays hidden.
    let mut w = world();
    w.spawn((at(0.0, 0.0), solid("inside", 0, 0)));
    w.spawn((turned(-2.2, 0.0, 0.0, [2.0, 2.0]), rect_mask(0, 0)));
    w.spawn((turned(2.2, 0.0, 0.0, [2.0, 2.0]), rect_mask(0, 0)));
    let pixels = draw(&mut w);
    assert_eq!(px(&pixels, 20, 8, 10), [255, 0, 0, 255]);
    assert_eq!(px(&pixels, 20, 12, 10), [255, 0, 0, 255]);
    assert_eq!(px(&pixels, 20, 10, 10), [0, 0, 0, 255], "the gap is outside both masks");

    // A sprite on another layer is not clipped.
    let mut w = world();
    w.spawn((at(0.0, 0.0), solid("inside", 3, 0)));
    w.spawn((turned(0.0, 0.0, 0.0, [2.0, 2.0]), rect_mask(0, 0)));
    let pixels = draw(&mut w);
    assert_eq!(px(&pixels, 20, 6, 6), [255, 0, 0, 255], "layer 3 is outside the mask range");
}

#[test]
fn a_turned_picture_mask_follows_the_transform() {
    let rgba = vec![
        255, 0, 0, 255, 0, 0, 0, 0, //
        255, 0, 0, 255, 0, 0, 0, 0,
    ];
    let mut w = world();
    w.spawn((at(0.0, 0.0), solid("inside", 0, 0)));
    w.spawn((
        turned(0.0, 0.0, 90.0, [4.0, 4.0]),
        SpriteMask { shape: "alpha".into(), image: "half".into(), cutoff: 0.5, ..Default::default() },
    ));
    let mut device = SoftDevice::new();
    let mut renderer = Renderer2D::new();
    renderer.load_image(&mut device, "half", 2, 2, &rgba);
    let target = device.create_texture(TextureDesc::rgba8(20, 20));
    renderer.render(&mut device, &mut w, target);
    let pixels = device.read_back(target);
    // A quarter turn, counter-clockwise, sends the opaque left half downward.
    assert_eq!(px(&pixels, 20, 10, 11), [255, 0, 0, 255]);
    assert_eq!(px(&pixels, 20, 10, 8), [0, 0, 0, 255]);
}
