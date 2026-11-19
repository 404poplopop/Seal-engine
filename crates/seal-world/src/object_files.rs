//! One object per file: each top-level object of a scene lives in its own file.
//!
//! The scene file at `scenes/level.seal` becomes an index. The objects are
//! `scenes/level/<id>.seal`. Loading the index builds the same scene as the
//! single file. Turning the option off writes that single file again.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use seal_sealtext::{parse, SealValue};

use crate::{Entity, World};

const MARKER: &str = ".seal-objects";

/// Scene resource. Present only when each top-level object is stored in its own file.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ObjectFiles {
    pub on: bool,
}

seal_ecs::text_component!(ObjectFiles as "object_files" { on });

/// Folder that holds the object files: `scenes/level.seal` -> `scenes/level/`.
pub fn folder_for(scene: &Path) -> PathBuf {
    scene.with_extension("")
}

pub fn is_index(text: &str) -> bool {
    parse(text).ok().and_then(|v| v.get("seal_objects").cloned()).is_some()
}

/// Read a scene file. An index is joined with its object files. Anything else is returned as stored.
pub fn read(path: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
    expand(path, &text)
}

/// Join `text` when it is an index next to `path`. A normal scene is returned unchanged.
pub fn expand(path: &Path, text: &str) -> Result<String, String> {
    if !is_index(text) {
        return Ok(text.to_string());
    }
    join(path, text)
}

/// Write `text`. When the scene asks for one object per file, split it. Otherwise write one file
/// and remove a previous split folder.
pub fn write(path: &Path, text: &str) -> Result<(), String> {
    let world = World::from_text(text).map_err(|err| format!("damaged scene: {err}"))?;
    if !world.objects_per_file {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        std::fs::write(path, text).map_err(|err| err.to_string())?;
        clear_folder(path)?;
        return Ok(());
    }
    let folder = folder_for(path);
    std::fs::create_dir_all(&folder).map_err(|err| err.to_string())?;
    clear_parts(&folder)?;
    let mut used = BTreeSet::new();
    let mut files = Vec::new();
    let order: Vec<String> = world.entities.iter().map(|entity| entity.id.clone()).collect();
    for entity in top_level(&world.entities) {
        let name = file_name(&entity.id, &mut used);
        let rows = subtree(&world.entities, &entity.id);
        let part = part_text(&world, &rows)?;
        std::fs::write(folder.join(&name), part).map_err(|err| err.to_string())?;
        files.push(name);
    }
    std::fs::write(folder.join(MARKER), b"seal objects\n").map_err(|err| err.to_string())?;
    let index = index_text(&world, &files, &order)?;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(path, index).map_err(|err| err.to_string())?;
    Ok(())
}

pub fn clear_folder(scene: &Path) -> Result<(), String> {
    let folder = folder_for(scene);
    if !folder.join(MARKER).is_file() {
        return Ok(());
    }
    std::fs::remove_dir_all(&folder).map_err(|err| err.to_string())
}

fn clear_parts(folder: &Path) -> Result<(), String> {
    if !folder.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(folder).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.ends_with(".seal") || name == MARKER {
            std::fs::remove_file(entry.path()).map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

fn top_level(entities: &[Entity]) -> Vec<&Entity> {
    entities
        .iter()
        .filter(|entity| match entity.parent.as_deref() {
            None => true,
            Some("root") => entity.id != "root",
            Some(parent) => !entities.iter().any(|other| other.id == parent),
        })
        .collect()
}

fn subtree(entities: &[Entity], root: &str) -> Vec<Entity> {
    // The scene root is its own file. Its children each have a file, with their own children.
    // A copy of the root is included so the child's parent link still resolves.
    if root == "root" {
        return entities.iter().filter(|entity| entity.id == "root").cloned().collect();
    }
    let mut ids = vec![root.to_string()];
    let mut i = 0;
    while i < ids.len() {
        let parent = ids[i].clone();
        for entity in entities {
            if entity.parent.as_deref() == Some(parent.as_str()) && !ids.contains(&entity.id) {
                ids.push(entity.id.clone());
            }
        }
        i += 1;
    }
    let mut rows: Vec<Entity> =
        entities.iter().filter(|entity| ids.iter().any(|id| id == &entity.id)).cloned().collect();
    if rows.iter().any(|entity| entity.parent.as_deref() == Some("root")) {
        if let Some(root_entity) = entities.iter().find(|entity| entity.id == "root") {
            if rows.iter().all(|entity| entity.id != "root") {
                rows.insert(0, root_entity.clone());
            }
        }
    }
    rows
}

fn file_name(id: &str, used: &mut BTreeSet<String>) -> String {
    let mut stem: String = id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    if stem.is_empty() {
        stem = "object".into();
    }
    let mut name = format!("{stem}.seal");
    let mut n = 2;
    while !used.insert(name.clone()) {
        name = format!("{stem}-{n}.seal");
        n += 1;
    }
    name
}

fn part_text(source: &World, rows: &[Entity]) -> Result<String, String> {
    if rows.is_empty() {
        return Err("damaged scene: an object file would be empty".into());
    }
    let mut world = World::with_entities(rows.to_vec(), source.gravity, source.time_scale);
    world.music = source.music.clone();
    world.sky_cubemap = source.sky_cubemap.clone();
    world.glow = source.glow.clone();
    world.join_on = source.join_on;
    world.objects_per_file = false;
    Ok(world.to_text())
}

fn index_text(world: &World, files: &[String], order: &[String]) -> Result<String, String> {
    let files = SealValue::Array(files.iter().cloned().map(SealValue::String).collect());
    let order = SealValue::Array(order.iter().cloned().map(SealValue::String).collect());
    let glow = &world.glow;
    let mut rows = vec![
        ("seal_objects".into(), SealValue::Number("1".into())),
        ("gravity".into(), SealValue::Number(world.gravity.to_string())),
        ("time_scale".into(), SealValue::Number(world.time_scale.to_string())),
        ("music".into(), SealValue::String(world.music.clone())),
        ("join_on".into(), SealValue::Bool(world.join_on)),
        ("glow_enabled".into(), SealValue::Bool(glow.enabled)),
        ("glow_threshold".into(), SealValue::Number(glow.threshold.to_string())),
        ("glow_intensity".into(), SealValue::Number(glow.intensity.to_string())),
        ("glow_radius".into(), SealValue::Number(glow.radius.to_string())),
        ("files".into(), files),
        ("order".into(), order),
    ];
    if !world.sky_cubemap.is_empty() {
        rows.push(("sky_cubemap".into(), SealValue::String(world.sky_cubemap.clone())));
    }
    Ok(SealValue::Map(rows.into()).to_text())
}

fn join(path: &Path, text: &str) -> Result<String, String> {
    let value = parse(text).map_err(|err| format!("damaged scene index: {err}"))?;
    let files = string_list(value.get("files")).ok_or("damaged scene index: no object files")?;
    if files.is_empty() {
        return Err("damaged scene index: no object files".into());
    }
    let folder = folder_for(path);
    let mut by_id: Vec<Entity> = Vec::new();
    for file in &files {
        if file.contains("..") || file.contains('/') || file.contains('\\') {
            return Err(format!("damaged object file {file}"));
        }
        let full = folder.join(file);
        let part =
            std::fs::read_to_string(&full).map_err(|_| format!("damaged object file {file}"))?;
        let loaded =
            World::from_text(&part).map_err(|err| format!("damaged object file {file}: {err}"))?;
        for entity in loaded.entities {
            if !by_id.iter().any(|have| have.id == entity.id) {
                by_id.push(entity);
            }
        }
    }
    let order = string_list(value.get("order")).unwrap_or_default();
    let mut entities = Vec::new();
    if order.is_empty() {
        entities = by_id;
    } else {
        for id in &order {
            if let Some(index) = by_id.iter().position(|entity| entity.id == *id) {
                entities.push(by_id.remove(index));
            }
        }
        entities.append(&mut by_id);
    }
    if entities.is_empty() {
        return Err("damaged scene index: object files had no objects".into());
    }
    let gravity = number_of(value.get("gravity")).unwrap_or(crate::DEFAULT_GRAVITY);
    let time_scale = number_of(value.get("time_scale")).unwrap_or(1.0);
    let mut world = World::with_entities(entities, gravity, time_scale);
    world.music = match value.get("music") {
        Some(SealValue::String(text)) => text.clone(),
        _ => String::new(),
    };
    world.sky_cubemap = match value.get("sky_cubemap") {
        Some(SealValue::String(text)) => text.clone(),
        _ => String::new(),
    };
    world.join_on = matches!(value.get("join_on"), Some(SealValue::Bool(true)));
    world.glow.enabled = matches!(value.get("glow_enabled"), Some(SealValue::Bool(true)));
    if let Some(n) = number_of(value.get("glow_threshold")) {
        world.glow.threshold = n;
    }
    if let Some(n) = number_of(value.get("glow_intensity")) {
        world.glow.intensity = n;
    }
    if let Some(n) = number_of(value.get("glow_radius")) {
        world.glow.radius = n;
    }
    world.objects_per_file = true;
    Ok(world.to_text())
}

fn string_list(value: Option<&SealValue>) -> Option<Vec<String>> {
    match value {
        Some(SealValue::Array(items)) => Some(
            items
                .iter()
                .filter_map(|item| match item {
                    SealValue::String(text) => Some(text.clone()),
                    _ => None,
                })
                .collect(),
        ),
        _ => None,
    }
}

fn number_of(value: Option<&SealValue>) -> Option<f32> {
    match value {
        Some(SealValue::Number(text)) => text.parse().ok(),
        _ => None,
    }
}
