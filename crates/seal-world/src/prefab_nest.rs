//! Nested prefabs: a prefab instance may contain instances of other prefabs.
//!
//! Editing the inner prefab updates those nested copies. Overrides stored on a nested
//! copy are kept when the outer prefab is rebuilt, and [`override_lines`] lists them.

use std::collections::BTreeMap;
use std::path::Path;

use crate::{copy_field, Entity, World};

#[derive(Clone, Debug)]
pub struct KeptNested {
    pub asset: String,
    pub nth: usize,
    pub entity: Entity,
}

fn descendants(entities: &[Entity], root: &str) -> Vec<String> {
    let mut queue = vec![root.to_string()];
    let mut out = Vec::new();
    let mut i = 0;
    while i < queue.len() {
        let parent = queue[i].clone();
        i += 1;
        for entity in entities {
            if entity.parent.as_deref() == Some(parent.as_str()) {
                out.push(entity.id.clone());
                queue.push(entity.id.clone());
            }
        }
    }
    out
}

/// Nested prefab copies under `root` that carry their own overrides, in tree order.
pub fn capture(entities: &[Entity], root: &str) -> Vec<KeptNested> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut out = Vec::new();
    for id in descendants(entities, root) {
        let Some(entity) = entities.iter().find(|e| e.id == id) else { continue };
        if entity.source_kind != "prefab" || entity.source_asset.is_empty() {
            continue;
        }
        if entity.overrides.split(',').all(|f| f.trim().is_empty()) {
            continue;
        }
        let nth = counts.entry(entity.source_asset.clone()).or_insert(0);
        out.push(KeptNested {
            asset: entity.source_asset.clone(),
            nth: *nth,
            entity: entity.clone(),
        });
        *nth += 1;
    }
    out
}

/// Put captured overrides back onto the rebuilt copies of the same prefab.
pub fn restore(entities: &mut [Entity], root: &str, kept: &[KeptNested]) {
    if kept.is_empty() {
        return;
    }
    let order = descendants(entities, root);
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut edits = Vec::new();
    for id in order {
        let Some(index) = entities.iter().position(|e| e.id == id) else { continue };
        let asset = entities[index].source_asset.clone();
        let kind = entities[index].source_kind.clone();
        if kind != "prefab" || asset.is_empty() {
            continue;
        }
        let nth = *counts.entry(asset.clone()).or_insert(0);
        if let Some(saved) = kept.iter().find(|k| k.asset == asset && k.nth == nth) {
            edits.push((index, saved.entity.clone(), saved.asset.clone()));
        }
        *counts.get_mut(&asset).unwrap() += 1;
    }
    for (index, previous, asset) in edits {
        let overrides = previous.overrides.clone();
        for field in overrides.split(',').map(str::trim).filter(|f| !f.is_empty()) {
            copy_field(&previous, &mut entities[index], field);
        }
        entities[index].overrides = overrides;
        entities[index].source_kind = "prefab".into();
        entities[index].source_asset = asset;
    }
}

fn depth(entities: &[Entity], id: &str) -> u32 {
    let mut depth = 0;
    let mut current = entities.iter().find(|e| e.id == id).and_then(|e| e.parent.clone());
    let mut guard = 0;
    while let Some(parent) = current {
        guard += 1;
        if guard > 64 {
            break;
        }
        let Some(entity) = entities.iter().find(|e| e.id == parent) else { break };
        if entity.source_kind == "prefab" {
            depth += 1;
        }
        current = entity.parent.clone();
    }
    depth
}

/// Refresh prefab assets that are used inside other prefabs, outer assets first.
/// Inner edits then win, and overrides on those nested copies stay.
pub fn refresh_nested(world: &mut World, dir: &Path) -> usize {
    let mut best: BTreeMap<String, u32> = BTreeMap::new();
    for entity in &world.entities {
        if entity.source_kind != "prefab" || entity.source_asset.is_empty() {
            continue;
        }
        let d = depth(&world.entities, &entity.id);
        if d == 0 {
            continue;
        }
        let slot = best.entry(entity.source_asset.clone()).or_insert(0);
        *slot = (*slot).max(d);
    }
    let mut assets: Vec<(u32, String)> = best.into_iter().map(|(asset, d)| (d, asset)).collect();
    assets.sort();
    let mut count = 0;
    for (_, asset) in assets {
        let path = dir.join(format!("{asset}.prefab"));
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Ok(expanded) = crate::prefab_variant::read_prefab_for_world(Some(dir), &text, false)
        else {
            continue;
        };
        if let Ok(n) = world.refresh_prefab_instances(&asset, &expanded) {
            count += n;
        }
    }
    count
}

/// Lines for the inspector: `Name (Prefab): field, field`.
pub fn override_lines(entities: &[Entity], root: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for id in descendants(entities, root) {
        let Some(entity) = entities.iter().find(|e| e.id == id) else { continue };
        if entity.source_kind != "prefab" {
            continue;
        }
        let fields: Vec<&str> =
            entity.overrides.split(',').map(str::trim).filter(|f| !f.is_empty()).collect();
        if fields.is_empty() {
            continue;
        }
        let asset =
            if entity.source_asset.is_empty() { "prefab" } else { entity.source_asset.as_str() };
        lines.push(format!("{} ({asset}): {}", entity.name, fields.join(", ")));
    }
    lines
}
