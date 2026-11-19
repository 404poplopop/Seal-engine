//! Sprite draw order: depth, Y-sort, a custom axis, sort groups, and per-layer rules.

use std::collections::HashMap;

use crate::Entity;

pub use seal_ecs::sprite_draw::{
    group_root, parse_sort_axis, parse_sort_layers, sort_axis_text, sort_indices, sort_layers_text,
    sort_pivot, LayerSortMode, ObjectSortMode, SortEntry, LAYER_SORTS, SORT_MODES,
};

pub const KEYS: &[&str] = &["sort_mode"];

pub fn field_text(entity: &Entity, key: &str) -> Option<String> {
    match key {
        "sort_mode" if !entity.sprite.is_empty() => Some(entity.sort_mode.name().into()),
        _ => None,
    }
}

pub fn check(key: &str, value: &str) -> Option<&'static str> {
    if key == "sort_mode" && !SORT_MODES.contains(&value.trim()) {
        return Some("sprite-sort-mode-invalid");
    }
    None
}

pub fn apply(entity: &mut Entity, key: &str, value: &str) {
    if key == "sort_mode" {
        entity.sort_mode = ObjectSortMode::parse(value);
    }
}

/// Map entity ids to indices and attach group parents for sort groups.
pub fn group_parents(
    entities: &[Entity],
    entry_entity: impl Fn(usize) -> usize,
) -> Vec<Option<usize>> {
    let id_to_ix: HashMap<&str, usize> =
        entities.iter().enumerate().map(|(i, e)| (e.id.as_str(), i)).collect();
    let mut parent_mark = vec![None; entities.len()];
    for (mark_ix, ent_ix) in (0..entities.len()).map(entry_entity).enumerate() {
        let mut walk = entities.get(ent_ix).and_then(|e| e.parent.as_deref());
        while let Some(pid) = walk {
            if let Some(&p_ix) = id_to_ix.get(pid) {
                if entities[p_ix].sort_mode == ObjectSortMode::SortGroup {
                    parent_mark[mark_ix] = Some(mark_ix); // filled below from group root mark
                    break;
                }
                walk = entities[p_ix].parent.as_deref();
            } else {
                break;
            }
        }
    }
    parent_mark
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(layer: i32, order: i32, y: f32, depth: f32) -> SortEntry {
        SortEntry {
            index: 0,
            entity_index: 0,
            layer,
            order,
            depth,
            pivot_x: 0.0,
            pivot_y: y,
            object_mode: ObjectSortMode::Default,
            group_parent: None,
        }
    }

    #[test]
    fn y_sort_orders_by_pivot_height_within_a_layer() {
        let mut low = entry(0, 0, 0.0, 1.0);
        let mut high = entry(0, 0, 2.0, 0.0);
        low.object_mode = ObjectSortMode::YSort;
        high.object_mode = ObjectSortMode::YSort;
        let rules = HashMap::new();
        let order = sort_indices(&[low, high], &rules, [0.0, 1.0]);
        assert_eq!(order, vec![0, 1]);
    }

    #[test]
    fn custom_axis_can_favour_horizontal_position() {
        let mut left = entry(0, 0, 1.0, 0.0);
        left.pivot_x = 0.0;
        left.object_mode = ObjectSortMode::Axis;
        let mut right = entry(0, 0, 1.0, 1.0);
        right.pivot_x = 3.0;
        right.index = 1;
        right.object_mode = ObjectSortMode::Axis;
        let rules = HashMap::new();
        let order = sort_indices(&[right, left], &rules, [1.0, 0.0]);
        assert_eq!(order, vec![1, 0]);
    }

    #[test]
    fn sort_group_keeps_children_together_by_parent_order() {
        let parent = SortEntry {
            index: 2,
            entity_index: 0,
            layer: 0,
            order: 5,
            depth: 0.0,
            pivot_x: 0.0,
            pivot_y: 0.0,
            object_mode: ObjectSortMode::SortGroup,
            group_parent: None,
        };
        let mut child_a = entry(0, 1, 2.0, 0.0);
        child_a.index = 0;
        child_a.group_parent = Some(2);
        let mut child_b = entry(0, 2, 0.0, 1.0);
        child_b.index = 1;
        child_b.group_parent = Some(2);
        let other = SortEntry {
            index: 3,
            entity_index: 3,
            layer: 0,
            order: 6,
            depth: 0.0,
            pivot_x: 0.0,
            pivot_y: 0.0,
            object_mode: ObjectSortMode::Default,
            group_parent: None,
        };
        let rules = HashMap::new();
        let order = sort_indices(&[child_a, child_b, parent, other], &rules, [0.0, 1.0]);
        assert_eq!(order, vec![0, 1, 2, 3]);
    }

    #[test]
    fn per_layer_y_sort_parses_and_applies() {
        let mut rules = parse_sort_layers("0:y_sort, 3:axis");
        assert_eq!(rules.get(&0), Some(&LayerSortMode::YSort));
        assert_eq!(rules.get(&3), Some(&LayerSortMode::Axis));
        rules.remove(&0);
        let round = parse_sort_layers(&sort_layers_text(&rules));
        assert_eq!(round.get(&3), Some(&LayerSortMode::Axis));
    }

    #[test]
    fn damaged_sort_layers_are_ignored() {
        let rules = parse_sort_layers("bad, 2:what, 1:y_sort");
        assert_eq!(rules.get(&1), Some(&LayerSortMode::YSort));
        assert!(!rules.contains_key(&2));
    }
}
