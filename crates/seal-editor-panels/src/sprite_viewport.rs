//! Sprite draw order in the viewport from scene entities and sorting project settings.

use std::collections::HashMap;

use seal_editor_core::sprite_sort::{self, LayerSortMode, ObjectSortMode, SortEntry};
use seal_editor_core::EditorSession;
use seal_ui::GridMark;

pub fn layer_rules(session: &EditorSession) -> HashMap<i32, LayerSortMode> {
    let text = session
        .models
        .settings
        .iter()
        .find(|s| s.id == "sprites.sort_layers")
        .map(|s| s.value())
        .unwrap_or_default();
    sprite_sort::parse_sort_layers(text)
}

pub fn sort_axis(session: &EditorSession) -> [f32; 2] {
    let text = session
        .models
        .settings
        .iter()
        .find(|s| s.id == "sprites.sort_axis")
        .map(|s| s.value())
        .unwrap_or_default();
    sprite_sort::parse_sort_axis(text)
}

/// Build sort entries aligned with `marks` and `entity_ids` (same length).
pub fn sort_entries(
    session: &EditorSession,
    marks: &[GridMark],
    entity_ids: &[&str],
    _across: usize,
    _up: usize,
    flip_across: f32,
    flip_up: f32,
) -> Vec<SortEntry> {
    let id_to_mark: HashMap<&str, usize> =
        entity_ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let id_to_entity: HashMap<&str, usize> =
        session.world.entities.iter().enumerate().map(|(i, e)| (e.id.as_str(), i)).collect();
    marks
        .iter()
        .enumerate()
        .map(|(index, mark)| {
            let entity_ix =
                entity_ids.get(index).and_then(|id| id_to_entity.get(id)).copied().unwrap_or(0);
            let entity = &session.world.entities[entity_ix];
            let (w, h) = (mark.half[0] * 2.0, mark.half[1] * 2.0);
            let pivot = sprite_sort::sort_pivot([mark.x, mark.y], mark.pivot, [w, h]);
            let group_parent =
                group_root_mark(entity, &session.world.entities, &id_to_mark, &id_to_entity);
            SortEntry {
                index,
                entity_index: entity_ix,
                layer: mark.layer,
                order: mark.order,
                depth: mark.depth,
                pivot_x: pivot[0] * flip_across,
                pivot_y: pivot[1] * flip_up,
                object_mode: entity.sort_mode,
                group_parent,
            }
        })
        .collect()
}

fn group_root_mark(
    entity: &seal_editor_core::Entity,
    entities: &[seal_editor_core::Entity],
    id_to_mark: &HashMap<&str, usize>,
    id_to_entity: &HashMap<&str, usize>,
) -> Option<usize> {
    let &self_ix = id_to_entity.get(entity.id.as_str())?;
    let parent_of = |ix: usize| {
        entities.get(ix)?.parent.as_deref().and_then(|id| id_to_entity.get(id).copied())
    };
    let root = sprite_sort::group_root(self_ix, parent_of, |ix| {
        entities.get(ix).is_some_and(|parent| parent.sort_mode == ObjectSortMode::SortGroup)
    })?;
    id_to_mark.get(entities[root].id.as_str()).copied()
}

/// Reorder sprite-like mark indices back to front.
pub fn sort_sprite_mark_indices(
    session: &EditorSession,
    marks: &[GridMark],
    entity_ids: &[&str],
) -> Vec<usize> {
    let rules = layer_rules(session);
    let axis = sort_axis(session);
    let drawable: Vec<usize> = (0..marks.len())
        .filter(|&i| {
            marks[i].sprite.is_some()
                || marks[i].shape2d.is_some()
                || marks[i].sprite_shape.is_some()
        })
        .collect();
    let entries: Vec<SortEntry> = drawable
        .iter()
        .map(|&index| {
            let mut entry = sort_entries(session, marks, entity_ids, 0, 1, 1.0, 1.0)
                .get(index)
                .copied()
                .unwrap_or(SortEntry {
                    index,
                    entity_index: 0,
                    layer: marks[index].layer,
                    order: marks[index].order,
                    depth: marks[index].depth,
                    pivot_x: marks[index].x,
                    pivot_y: marks[index].y,
                    object_mode: ObjectSortMode::Default,
                    group_parent: None,
                });
            entry.index = index;
            entry
        })
        .collect();
    let sorted = sprite_sort::sort_indices(&entries, &rules, axis);
    sorted.into_iter().map(|i| drawable[i]).collect()
}
