//! Runtime sprites follow the same order, pivot, tiled slices and glow as the editor.

use seal_ecs::core::Transform;
use seal_ecs::sprite_draw::{self, ObjectSortMode, SortEntry};
use seal_ecs::ChildOf;
use seal_render::{Device, SoftDevice, TextureDesc};
use seal_render2d::*;

fn world() -> seal_ecs::World {
    let mut w = seal_ecs::World::new();
    w.register_core();
    register_2d(&mut w);
    w
}

fn at(x: f32, y: f32) -> Transform {
    Transform { translation: [x, y, 0.0], ..Default::default() }
}

fn px(p: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let i = (y * width + x) * 4;
    [p[i], p[i + 1], p[i + 2], p[i + 3]]
}

fn solid(color: [f32; 4]) -> Sprite {
    Sprite { size: [8.0, 8.0], color, ..Default::default() }
}

#[test]
fn default_order_stays_spawn_order_inside_a_layer() {
    let mut w = world();
    w.spawn((at(0.0, 3.0), solid([1.0, 0.0, 0.0, 1.0])));
    w.spawn((at(0.0, 0.0), solid([0.0, 0.0, 1.0, 1.0])));
    let mut device = SoftDevice::new();
    let target = device.create_texture(TextureDesc::rgba8(32, 32));
    Renderer2D::new().render(&mut device, &mut w, target);
    assert_eq!(px(&device.read_back(target), 32, 16, 16), [0, 0, 255, 255]);
}

#[test]
fn runtime_order_matches_editor_sort_for_y_axis_and_groups() {
    let mut w = world();
    w.insert_resource(SortSettings { layers: "0:y_sort, bogus, 2:nope".into(), axis: [1.0, 0.0] });
    let low =
        w.spawn((at(0.0, 0.0), solid([1.0, 0.0, 0.0, 1.0]), SpriteOrder { mode: "y_sort".into() }));
    let high =
        w.spawn((at(0.0, 2.0), solid([0.0, 1.0, 0.0, 1.0]), SpriteOrder { mode: "y_sort".into() }));
    let _ = (low, high);
    let mut device = SoftDevice::new();
    let target = device.create_texture(TextureDesc::rgba8(32, 32));
    Renderer2D::new().render(&mut device, &mut w, target);
    assert_eq!(
        px(&device.read_back(target), 32, 16, 16),
        [0, 255, 0, 255],
        "higher pivot draws on top"
    );

    let mut axis = world();
    axis.insert_resource(SortSettings { layers: String::new(), axis: [1.0, 0.0] });
    axis.spawn((at(1.0, 0.0), solid([0.0, 0.0, 1.0, 1.0]), SpriteOrder { mode: "axis".into() }));
    axis.spawn((at(0.0, 0.0), solid([1.0, 0.0, 0.0, 1.0]), SpriteOrder { mode: "axis".into() }));
    let entries =
        [entry_at(0, 1.0, 0.0, ObjectSortMode::Axis), entry_at(1, 0.0, 0.0, ObjectSortMode::Axis)];
    let rules = sprite_draw::parse_sort_layers("");
    assert_eq!(sprite_draw::sort_indices(&entries, &rules, [1.0, 0.0]), vec![1, 0]);
    let target = device.create_texture(TextureDesc::rgba8(32, 32));
    Renderer2D::new().render(&mut device, &mut axis, target);
    assert_eq!(
        px(&device.read_back(target), 32, 16, 16),
        [0, 0, 255, 255],
        "larger axis key draws on top"
    );

    let mut grouped = world();
    let parent = grouped.spawn((
        Transform { translation: [40.0, 0.0, 0.0], ..Default::default() },
        Sprite { size: [1.0, 1.0], order: 1, ..Default::default() },
        SpriteOrder { mode: "sort_group".into() },
    ));
    let child = grouped.spawn((
        at(0.0, 0.0),
        Sprite { size: [8.0, 8.0], color: [1.0, 0.0, 0.0, 1.0], order: 100, ..Default::default() },
    ));
    grouped.set_parent(child, Some(parent)).unwrap();
    grouped.spawn((
        at(0.0, 0.0),
        Sprite { size: [8.0, 8.0], color: [0.0, 0.0, 1.0, 1.0], order: 5, ..Default::default() },
    ));
    assert!(grouped.get::<ChildOf>(child).is_some());
    let target = device.create_texture(TextureDesc::rgba8(32, 32));
    Renderer2D::new().render(&mut device, &mut grouped, target);
    assert_eq!(
        px(&device.read_back(target), 32, 16, 16),
        [0, 0, 255, 255],
        "the child draws at the group's order, under the later sprite"
    );
}

fn entry_at(index: usize, x: f32, y: f32, mode: ObjectSortMode) -> SortEntry {
    let pivot = sprite_draw::sort_pivot([x, y], [0.5, 0.5], [8.0, 8.0]);
    SortEntry {
        index,
        entity_index: index,
        layer: 0,
        order: 0,
        depth: 0.0,
        pivot_x: pivot[0],
        pivot_y: pivot[1],
        object_mode: mode,
        group_parent: None,
    }
}

#[test]
fn pivot_offsets_the_runtime_quad_and_rotation_stays_on_that_point() {
    let mut w = world();
    w.spawn((at(0.0, 0.0), Sprite { size: [4.0, 2.0], anchor: [0.0, 0.0], ..Default::default() }));
    let (batches, _) = Renderer2D::new().build_batches(&mut w, 20, 20);
    let corners = batches[0].instances[0].corners;
    assert!(
        (corners[3][0] - 10.0).abs() < 1e-4 && (corners[3][1] - 10.0).abs() < 1e-4,
        "{corners:?}"
    );
    assert!(
        (corners[1][0] - 14.0).abs() < 1e-4 && (corners[1][1] - 8.0).abs() < 1e-4,
        "{corners:?}"
    );

    let mut turned = world();
    turned.spawn((
        Transform { rotation: [0.0, 0.0, 90.0], ..Default::default() },
        Sprite {
            size: [2.0, 2.0],
            anchor: [0.0, 0.0],
            color: [1.0, 0.0, 0.0, 1.0],
            ..Default::default()
        },
    ));
    let (batches, _) = Renderer2D::new().build_batches(&mut turned, 20, 20);
    let corners = batches[0].instances[0].corners;
    let pivot = corners.iter().find(|p| (p[0] - 10.0).abs() < 1e-3 && (p[1] - 10.0).abs() < 1e-3);
    assert!(pivot.is_some(), "the pivot corner stays on the entity: {corners:?}");
}

#[test]
fn slice_size_and_tile_mode_set_quad_count_and_uvs() {
    let mut w = world();
    w.spawn((
        at(0.0, 0.0),
        Sprite {
            size: [2.0, 2.0],
            anchor: [0.5, 0.5],
            image: "panel".into(),
            ..Default::default()
        },
        SpriteSlice {
            border: [8.0, 8.0, 8.0, 8.0],
            mode: "tile".into(),
            size: [6.0, 2.0],
            picture: [32.0, 32.0],
        },
    ));
    let mut device = SoftDevice::new();
    let mut renderer = Renderer2D::new();
    renderer.load_image(&mut device, "panel", 32, 32, &vec![255; 32 * 32 * 4]);
    let (batches, stats) = renderer.build_batches(&mut w, 32, 32);
    let pieces = sprite_draw::tile_pieces(
        [8.0, 8.0, 8.0, 8.0],
        [0.0, 0.0, 1.0, 1.0],
        [32.0, 32.0],
        [6.0, 2.0],
    );
    let quads: Vec<_> = batches.iter().flat_map(|b| b.instances.iter()).collect();
    assert_eq!(quads.len(), pieces.len());
    assert!(quads.len() >= 9, "tile mode splits the nine-slice: {}", quads.len());
    assert_eq!(stats.sprites, 1);
    let mut got: Vec<[f32; 4]> = quads
        .iter()
        .map(|q| {
            let [u0, v0, u1, v1] = q.uv;
            [u0.min(u1), v0.min(v1), u0.max(u1), v0.max(v1)]
        })
        .collect();
    let mut expect: Vec<[f32; 4]> = pieces
        .iter()
        .map(|p| {
            let [u0, v0, u1, v1] = p.uv;
            [u0.min(u1), v0.min(v1), u0.max(u1), v0.max(v1)]
        })
        .collect();
    got.sort_by(|a, b| a.partial_cmp(b).unwrap());
    expect.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(got, expect);
    let width =
        quads.iter().flat_map(|q| q.corners).map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max)
            - quads.iter().flat_map(|q| q.corners).map(|p| p[0]).fold(f32::INFINITY, f32::min);
    assert!((width - 6.0).abs() < 1e-3, "slice size sets the world width: {width}");
}

#[test]
fn glow_pass_changes_only_pixels_above_the_threshold() {
    let mut w = world();
    w.insert_resource(Camera2D { center: [0.0, 0.0], zoom: 1.0, clear: [0.0, 0.0, 0.0, 1.0] });
    w.insert_resource(Glow { enabled: true, threshold: 0.5, intensity: 2.0, radius: 1.0 });
    w.spawn((
        at(-6.0, 0.0),
        Sprite { size: [2.0, 2.0], color: [0.2, 0.2, 0.2, 1.0], ..Default::default() },
    ));
    w.spawn((
        at(6.0, 0.0),
        Sprite { size: [2.0, 2.0], color: [0.6, 0.6, 0.6, 1.0], ..Default::default() },
    ));
    let mut device = SoftDevice::new();
    let target = device.create_texture(TextureDesc::rgba8(32, 32));
    let mut plain = world();
    plain.insert_resource(Camera2D { center: [0.0, 0.0], zoom: 1.0, clear: [0.0, 0.0, 0.0, 1.0] });
    plain.spawn((
        at(-6.0, 0.0),
        Sprite { size: [2.0, 2.0], color: [0.2, 0.2, 0.2, 1.0], ..Default::default() },
    ));
    plain.spawn((
        at(6.0, 0.0),
        Sprite { size: [2.0, 2.0], color: [0.6, 0.6, 0.6, 1.0], ..Default::default() },
    ));
    Renderer2D::new().render(&mut device, &mut plain, target);
    let before = device.read_back(target);
    Renderer2D::new().render(&mut device, &mut w, target);
    let after = device.read_back(target);
    let dim = px(&before, 32, 10, 16);
    assert!(dim[0] < 80, "the dim sprite is under the threshold: {dim:?}");
    assert_eq!(px(&after, 32, 10, 16), dim, "a pixel under the threshold is unchanged");
    let bright = px(&before, 32, 22, 16);
    assert!(bright[0] > 100, "the bright sprite is over the threshold: {bright:?}");
    assert_ne!(px(&after, 32, 22, 16), bright, "a pixel over the threshold changes");
}
