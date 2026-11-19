//! Parallax background layers: scroll speed while the world runs, and repeated copies.

use seal_editor_panels::{parallax_shift, repeat_copies};
use seal_ui::GridMark;

fn mark(x: f32, half: f32) -> GridMark<'static> {
    GridMark {
        x,
        y: 0.0,
        half: [half, 1.0],
        round: false,
        billboard: None,
        selected: true,
        faint: false,
        label: "Hills",
        tint: [255; 3],
        angle: 0.0,
        depth: 0.0,
        size: [half, 1.0, 0.5],
        mesh: None,
        pickable: true,
        sprite: Some("art/hills.png".into()),
        layer: -5,
        uv: [0.0, 0.0, 1.0, 1.0],
        order: 0,
        sprite_tint: [255; 4],
        slice: [0.0; 4],
        slice_mode: "stretch",
        slice_size: [0.0; 2],
        pivot: [0.5, 0.5],
        normal_map: None,
        shape2d: None,
        sprite_shape: None,
        material: None,
        mask_mode: 0,
        mask_kind: 0,
        surface: seal_ui::MeshSurface::EMPTY,
    }
}

#[test]
fn parallax_layers_scroll_at_their_own_speed() {
    let mut entity = seal_editor_core::World::starter().entities[0].clone();
    entity.parallax = [0.25, 1.0];
    // Camera moved 8 units right and 2 up: a 0.25 layer is drawn 6 units further
    // right, so on screen it moved only 2 (a quarter of the camera), and 1 on Y stays put.
    assert_eq!(parallax_shift(&entity, true, [8.0, 2.0]), [6.0, 0.0]);
    // Factor 0 rides along with the view; factor 1 or not running never shifts.
    entity.parallax = [0.0, 0.0];
    assert_eq!(parallax_shift(&entity, true, [8.0, 2.0]), [8.0, 2.0]);
    assert_eq!(parallax_shift(&entity, false, [8.0, 2.0]), [0.0, 0.0]);
    entity.parallax = [1.0, 1.0];
    assert_eq!(parallax_shift(&entity, true, [8.0, 2.0]), [0.0, 0.0]);
}

#[test]
fn repeated_backgrounds_fill_the_view_without_gaps() {
    let base = mark(1.0, 2.0); // 4 units wide
    let copies = repeat_copies(&base, 30.0, 20.0);
    // The original sits off to the left of this view, so the copies alone cover it.
    let mut xs: Vec<f32> = copies.iter().map(|c| c.x).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!(xs.first().unwrap() - 2.0 <= 10.0 && xs.last().unwrap() + 2.0 >= 50.0, "{xs:?}");
    assert!(xs.windows(2).all(|w| (w[1] - w[0] - 4.0).abs() < 1e-4), "touching copies");
    assert!(copies.iter().all(|c| !c.pickable && !c.selected && c.label.is_empty()));
    assert!(repeat_copies(&mark(0.0, 0.0), 0.0, 10.0).is_empty());
}
