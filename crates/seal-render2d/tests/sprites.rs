use seal_ecs::core::Transform;
use seal_ecs::World;
use seal_render::{Device, SoftDevice, TextureDesc};
use seal_render2d::*;

fn world() -> World {
    let mut w = World::new();
    w.register_core();
    register_2d(&mut w);
    w
}

fn at(x: f32, y: f32) -> Transform {
    Transform { translation: [x, y, 0.0], ..Default::default() }
}

fn solid(color: [f32; 4], size: f32, layer: i32) -> Sprite {
    Sprite { size: [size, size], color, layer, ..Default::default() }
}

fn px(p: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let i = (y * width + x) * 4;
    [p[i], p[i + 1], p[i + 2], p[i + 3]]
}

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

#[test]
fn a_sprite_covers_exactly_its_rectangle() {
    let mut w = world();
    w.spawn((at(0.0, 0.0), solid(RED, 10.0, 0)));
    let mut device = SoftDevice::new();
    let target = device.create_texture(TextureDesc::rgba8(20, 20));
    let stats = Renderer2D::new().render(&mut device, &mut w, target);
    assert_eq!(stats, FrameStats { sprites: 1, offscreen: 0, batches: 1, missing_images: 0 });
    let p = device.read_back(target);
    for y in 0..20 {
        for x in 0..20 {
            let inside = (5..15).contains(&x) && (5..15).contains(&y);
            assert_eq!(
                px(&p, 20, x, y),
                if inside { [255, 0, 0, 255] } else { [0, 0, 0, 255] },
                "{x},{y}"
            );
        }
    }
}

#[test]
fn higher_layers_draw_on_top_whatever_the_spawn_order() {
    let mut w = world();
    w.spawn((at(0.0, 0.0), solid(BLUE, 10.0, 1)));
    w.spawn((at(0.0, 0.0), solid(RED, 10.0, 0)));
    let mut device = SoftDevice::new();
    let target = device.create_texture(TextureDesc::rgba8(20, 20));
    Renderer2D::new().render(&mut device, &mut w, target);
    assert_eq!(px(&device.read_back(target), 20, 10, 10), [0, 0, 255, 255]);
}

#[test]
fn children_follow_their_rotated_parent() {
    let mut w = world();
    let parent = w.spawn((Transform { rotation: [0.0, 0.0, 90.0], ..Default::default() },));
    let child = w.spawn((at(6.0, 0.0), solid(RED, 2.0, 0)));
    w.set_parent(child, Some(parent)).unwrap();
    let m = world_transforms(&mut w)[&child];
    let p = m.apply([0.0, 0.0]);
    assert!((p[0] - 0.0).abs() < 1e-5 && (p[1] - 6.0).abs() < 1e-5, "{p:?}");
    // Up in the world is up on screen: 6 units above the centre of a 20x20 target.
    let mut device = SoftDevice::new();
    let target = device.create_texture(TextureDesc::rgba8(20, 20));
    Renderer2D::new().render(&mut device, &mut w, target);
    let pixels = device.read_back(target);
    assert_eq!(px(&pixels, 20, 10, 4), [255, 0, 0, 255]);
    assert_eq!(px(&pixels, 20, 10, 15), [0, 0, 0, 255]);
}

#[test]
fn batching_groups_images_when_allowed() {
    let mut w = world();
    for i in 0..100 {
        let image = if i % 2 == 0 { "a" } else { "b" };
        w.spawn((at(i as f32 % 10.0, 0.0), Sprite { image: image.into(), ..Default::default() }));
    }
    let mut device = SoftDevice::new();
    let mut renderer = Renderer2D::new();
    renderer.load_image(&mut device, "a", 1, 1, &[255; 4]);
    renderer.load_image(&mut device, "b", 1, 1, &[0, 0, 0, 255]);
    let (_, stable) = renderer.build_batches(&mut w, 64, 64);
    assert_eq!(stable.batches, 100, "alternating images cannot merge without reordering");
    renderer.sort = SortMode::ByImage;
    let (batches, grouped) = renderer.build_batches(&mut w, 64, 64);
    assert_eq!(grouped.batches, 2);
    assert_eq!(batches.iter().map(|b| b.instances.len()).sum::<usize>(), 100);
}

#[test]
fn offscreen_and_missing_images_are_reported() {
    let mut w = world();
    w.spawn((at(1000.0, 0.0), solid(RED, 1.0, 0)));
    w.spawn((at(0.0, 0.0), Sprite { image: "nope".into(), ..Default::default() }));
    let (_, stats) = Renderer2D::new().build_batches(&mut w, 32, 32);
    assert_eq!(stats, FrameStats { sprites: 2, offscreen: 1, batches: 1, missing_images: 1 });
}

#[test]
fn camera_zoom_and_position_move_the_view() {
    let mut w = world();
    w.insert_resource(Camera2D { center: [10.0, 0.0], zoom: 2.0, clear: [0.0, 1.0, 0.0, 1.0] });
    w.spawn((at(10.0, 0.0), solid(RED, 4.0, 0)));
    let mut device = SoftDevice::new();
    let target = device.create_texture(TextureDesc::rgba8(20, 20));
    Renderer2D::new().render(&mut device, &mut w, target);
    let pixels = device.read_back(target);
    // 4 units at zoom 2 is 8 pixels, centred: 6..14.
    assert_eq!(px(&pixels, 20, 6, 6), [255, 0, 0, 255]);
    assert_eq!(px(&pixels, 20, 13, 13), [255, 0, 0, 255]);
    assert_eq!(px(&pixels, 20, 5, 10), [0, 255, 0, 255]);
    assert_eq!(px(&pixels, 20, 14, 10), [0, 255, 0, 255]);
}

#[test]
fn flipping_mirrors_the_image() {
    let mut w = world();
    w.spawn((
        at(0.0, 0.0),
        Sprite { image: "lr".into(), size: [2.0, 1.0], flip: [true, false], ..Default::default() },
    ));
    let mut device = SoftDevice::new();
    let mut renderer = Renderer2D::new();
    renderer.load_image(&mut device, "lr", 2, 1, &[255, 0, 0, 255, 0, 0, 255, 255]);
    let target = device.create_texture(TextureDesc::rgba8(2, 1));
    renderer.render(&mut device, &mut w, target);
    assert_eq!(device.read_back(target), [0, 0, 255, 255, 255, 0, 0, 255]);
}

#[test]
fn sprites_and_camera_save_with_the_scene() {
    let mut w = world();
    w.insert_resource(Camera2D { center: [3.0, 4.0], zoom: 8.0, ..Default::default() });
    w.spawn((
        at(1.0, 2.0),
        Sprite { image: "hero".into(), layer: 3, flip: [true, false], ..Default::default() },
    ));
    let text = w.save_scene(None).text;
    let mut back = world();
    back.load_scene(&text).unwrap();
    assert_eq!(back.resource::<Camera2D>().unwrap().zoom, 8.0);
    let sprites = back.query::<&Sprite>().collect(|s| s.clone());
    assert_eq!(sprites[0].image, "hero");
    assert_eq!(sprites[0].layer, 3);
    assert_eq!(back.save_scene(None).text, text);
}

#[test]
fn a_busy_scene_draws_the_same_bytes_everywhere() {
    let mut w = world();
    w.insert_resource(Camera2D { center: [0.0, 0.0], zoom: 3.0, clear: [0.05, 0.05, 0.1, 1.0] });
    for i in 0..60 {
        let f = i as f32;
        w.spawn((
            Transform {
                translation: [f % 12.0 * 2.0 - 12.0, f / 12.0 * 3.0 - 7.0, 0.0],
                rotation: [0.0, 0.0, f * 17.0],
                scale: [1.0 + f % 3.0 * 0.25, 1.0, 1.0],
            },
            Sprite {
                image: if i % 3 == 0 { "checker".into() } else { String::new() },
                size: [3.0, 2.0],
                color: [f % 5.0 / 5.0, f % 7.0 / 7.0, 0.8, 0.7],
                layer: i % 4,
                additive: i % 5 == 0,
                ..Default::default()
            },
        ));
    }
    let mut device = SoftDevice::new();
    let mut renderer = Renderer2D::new();
    let checker: Vec<u8> = (0..16)
        .flat_map(|i| if (i + i / 4) % 2 == 0 { [255, 255, 255, 255] } else { [40, 40, 40, 255] })
        .collect();
    renderer.load_image(&mut device, "checker", 4, 4, &checker);
    let target = device.create_texture(TextureDesc::rgba8(96, 64));
    renderer.render(&mut device, &mut w, target);
    let hash = device
        .read_back(target)
        .iter()
        .fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x0100_0000_01b3));
    assert_eq!(hash, GOLDEN, "frame hash {hash:#x}");
}

const GOLDEN: u64 = 0xfa36_86a1_6386_1dff;

#[test]
fn a_hidden_sprite_is_not_drawn_and_saves_its_flag() {
    let mut w = world();
    w.spawn((at(0.0, 0.0), Sprite { hidden: true, ..solid(RED, 10.0, 0) }));
    w.spawn((at(0.0, 0.0), solid(BLUE, 4.0, 0)));
    let mut device = SoftDevice::new();
    let target = device.create_texture(TextureDesc::rgba8(20, 20));
    let stats = Renderer2D::new().render(&mut device, &mut w, target);
    assert_eq!(stats.sprites, 1, "only the visible sprite is drawn");
    let p = device.read_back(target);
    assert_eq!(px(&p, 20, 6, 6), [0, 0, 0, 255], "the hidden red square leaves no trace");
    assert_eq!(px(&p, 20, 10, 10), [0, 0, 255, 255]);
    let e = w.query::<(seal_ecs::Entity, &Sprite)>().collect(|(e, s)| (e, s.hidden));
    let hidden = e.iter().find(|(_, h)| *h).unwrap().0;
    let saved = w.component_value(hidden, "sprite").unwrap();
    assert_eq!(saved.get("hidden"), Some(&seal_ecs::SealValue::Bool(true)));
}

#[test]
fn render_scale_draws_the_same_view_at_a_larger_size() {
    let mut w = world();
    w.spawn((at(0.0, 0.0), solid(RED, 10.0, 0)));
    let mut device = SoftDevice::new();
    let target = device.create_texture(TextureDesc::rgba8(40, 40));
    let mut renderer = Renderer2D::new();
    renderer.pixel_scale = 2.0;
    renderer.render(&mut device, &mut w, target);
    let p = device.read_back(target);
    // The 10-unit sprite at 1 pixel per unit fills 20x20 in the middle of a 2x frame.
    for y in 0..40 {
        for x in 0..40 {
            let inside = (10..30).contains(&x) && (10..30).contains(&y);
            assert_eq!(
                px(&p, 40, x, y),
                if inside { [255, 0, 0, 255] } else { [0, 0, 0, 255] },
                "{x},{y}"
            );
        }
    }
}
