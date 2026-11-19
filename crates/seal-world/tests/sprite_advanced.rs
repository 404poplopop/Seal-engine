//! Sorting, slice mode, pivot, and backward-compatible defaults.

use seal_world::sprite_pivot::parse_pivot;
use seal_world::sprite_sort::{
    parse_sort_layers, sort_indices, LayerSortMode, ObjectSortMode, SortEntry,
};
use seal_world::World;

#[test]
fn default_entity_fields_match_legacy_draw() {
    let world = World::demo();
    let e = world.entities.iter().find(|e| e.id == "crate").unwrap();
    assert_eq!(e.pivot, [0.5, 0.5]);
    assert_eq!(e.slice_mode, "stretch");
    assert_eq!(e.slice_size, [0.0; 2]);
    assert_eq!(e.sort_mode, ObjectSortMode::Default);
    let text = world.to_text();
    let loaded = World::from_text(&text).unwrap();
    let back = loaded.entity("crate").unwrap();
    assert_eq!(back.pivot, [0.5, 0.5]);
    assert_eq!(back.sort_mode, ObjectSortMode::Default);
}

#[test]
fn pivot_and_sort_fields_round_trip_in_scene_text() {
    let mut world = World::demo();
    let e = world.entities.iter_mut().find(|e| e.id == "crate").unwrap();
    e.sprite = "sprites/crate.png".into();
    e.pivot = [0.25, 0.75];
    e.sort_mode = ObjectSortMode::YSort;
    e.slice_mode = "tile".into();
    e.slice_size = [2.0, 3.0];
    let loaded = World::from_text(&world.to_text()).unwrap();
    let back = loaded.entity("crate").unwrap();
    assert_eq!(back.pivot, [0.25, 0.75]);
    assert_eq!(back.sort_mode, ObjectSortMode::YSort);
    assert_eq!(back.slice_mode, "tile");
    assert_eq!(back.slice_size, [2.0, 3.0]);
}

#[test]
fn y_sort_draw_order_uses_pivot_height() {
    let rules = parse_sort_layers("0:y_sort");
    let low = SortEntry {
        index: 0,
        entity_index: 0,
        layer: 0,
        order: 0,
        depth: 1.0,
        pivot_x: 0.0,
        pivot_y: 0.0,
        object_mode: ObjectSortMode::Default,
        group_parent: None,
    };
    let high = SortEntry {
        index: 1,
        entity_index: 1,
        layer: 0,
        order: 0,
        depth: 0.0,
        pivot_x: 0.0,
        pivot_y: 2.0,
        object_mode: ObjectSortMode::Default,
        group_parent: None,
    };
    let order = sort_indices(&[high, low], &rules, [0.0, 1.0]);
    assert_eq!(order, vec![1, 0]);
    assert_eq!(parse_pivot("bottom_center"), Some([0.5, 0.0]));
}

#[test]
fn damaged_sort_layer_table_skips_bad_rows() {
    let rules = parse_sort_layers("oops, 2:axis, -1:y_sort");
    assert_eq!(rules.get(&2), Some(&LayerSortMode::Axis));
    assert!(!rules.contains_key(&-1));
}
