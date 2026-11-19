//! Asset Editor form for `.tiles` rule files: picture metadata rows.

use std::path::Path;

use crate::tiles::{RuleSet, TileDef, TileNavigation};

/// One editable row per picture that has metadata.
#[derive(Clone, Debug)]
pub struct TileSetRow {
    pub picture: String,
    pub def: TileDef,
}

pub fn read_rows(path: &Path) -> Result<Vec<TileSetRow>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let set = RuleSet::parse(&text)?;
    let mut rows: Vec<TileSetRow> =
        set.defs.into_iter().map(|(picture, def)| TileSetRow { picture, def }).collect();
    rows.sort_by(|a, b| a.picture.cmp(&b.picture));
    if rows.is_empty() {
        for (_, pic) in &set.rules {
            rows.push(TileSetRow { picture: pic.clone(), def: TileDef::default() });
        }
    }
    Ok(rows)
}

pub fn write_row(path: &Path, picture: &str, def: &TileDef) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let header = format!("tile {picture}");
    let start = lines.iter().position(|l| l.trim() == header);
    if let Some(ix) = start {
        let mut end = ix + 1;
        while end < lines.len() && (lines[end].starts_with(' ') || lines[end].starts_with('\t')) {
            end += 1;
        }
        lines.drain(ix..end);
    }
    let mut block = vec![header];
    for (k, v) in &def.custom_data {
        block.push(format!("  data {k}={v}"));
    }
    if def.animate_frames.len() >= 2 {
        let mut row = String::from("  animate");
        for f in &def.animate_frames {
            row.push(' ');
            row.push_str(f);
        }
        row.push_str(&format!(" fps {}", def.animate_fps));
        block.push(row);
    }
    if def.occluder {
        block.push("  occluder yes".into());
    }
    if def.navigation == TileNavigation::Blocked {
        block.push("  navigation blocked".into());
    }
    if !def.prefab.is_empty() {
        block.push(format!("  prefab {}", def.prefab));
    }
    if block.len() > 1 {
        lines.extend(block);
    }
    std::fs::write(path, format!("{}\n", lines.join("\n"))).map_err(|e| e.to_string())
}
