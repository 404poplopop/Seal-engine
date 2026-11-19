//! Prefab variants: a `.prefab` file may name a `base` prefab and store only entity rows that differ.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use seal_sealtext::{parse, SealValue};

use crate::{entity_from, entity_value, Entity};

/// Whether the text names a variant (has `base`, not only a full entity list).
pub fn is_variant_document(text: &str) -> bool {
    match parse(text).ok().and_then(|v| v.get("base").cloned()) {
        Some(SealValue::String(s)) => !s.is_empty(),
        _ => false,
    }
}

fn load_entity_map(text: &str) -> Result<BTreeMap<String, Entity>, String> {
    let value = parse(text).map_err(|e| e.to_string())?;
    let Some(SealValue::Array(items)) = value.get("entities") else {
        return Err("prefab is missing entities".into());
    };
    let mut out = BTreeMap::new();
    for item in items {
        let e = entity_from(item)?;
        out.insert(e.id.clone(), e);
    }
    Ok(out)
}

fn merge_maps(
    base: &BTreeMap<String, Entity>,
    patch: &BTreeMap<String, Entity>,
) -> BTreeMap<String, Entity> {
    let mut out = base.clone();
    for (id, row) in patch {
        out.insert(id.clone(), row.clone());
    }
    out
}

fn entity_row_diff(base: &Entity, current: &Entity) -> Option<Entity> {
    let b = entity_value(base);
    let c = entity_value(current);
    let patch = seal_ecs::diff_patch(&b, &c)?;
    let map = patch.as_map()?;
    let mut entries = map.to_vec();
    if !entries.iter().any(|(k, _)| k == "id") {
        entries.push(("id".into(), SealValue::String(current.id.clone())));
    }
    entity_from(&SealValue::Map(entries.into())).ok()
}

/// Load `stem.prefab` under `prefabs_dir`, following `base` links, and return merged entity text.
pub fn expand_prefab_file(prefabs_dir: &Path, text: &str) -> Result<String, String> {
    let value = parse(text).map_err(|e| e.to_string())?;
    let base = match value.get("base") {
        Some(SealValue::String(name)) if !name.is_empty() => name.clone(),
        _ => {
            return Ok(text.to_string());
        }
    };
    let base_path = prefabs_dir.join(format!("{base}.prefab"));
    let base_text = std::fs::read_to_string(&base_path)
        .map_err(|_| format!("base prefab {base} was not found"))?;
    let base_text = expand_prefab_file(prefabs_dir, &base_text)?;
    let base_map = load_entity_map(&base_text)?;
    let patch_map = load_entity_map(text).unwrap_or_default();
    let merged = merge_maps(&base_map, &patch_map);
    let root = match value.get("root") {
        Some(SealValue::String(s)) if !s.is_empty() => s.clone(),
        _ => parse(&base_text)
            .ok()
            .and_then(|bv| match bv.get("root") {
                Some(SealValue::String(s)) if !s.is_empty() => Some(s.clone()),
                _ => None,
            })
            .or_else(|| merged.keys().next().cloned())
            .ok_or("prefab has no root")?,
    };
    let rows: Vec<SealValue> = merged.values().map(entity_value).collect();
    Ok(SealValue::Map(
        vec![
            ("seal_prefab".into(), SealValue::Number("1".into())),
            (
                "prefab".into(),
                value.get("prefab").cloned().unwrap_or(SealValue::String(String::new())),
            ),
            ("root".into(), SealValue::String(root)),
            ("entities".into(), SealValue::Array(rows)),
        ]
        .into(),
    )
    .to_text())
}

/// Build variant file text that stores only rows differing from `base_stem`.
pub fn compact_variant_file(
    prefabs_dir: &Path,
    base_stem: &str,
    full_text: &str,
    prefab_name: &str,
) -> Result<String, String> {
    let base_path = prefabs_dir.join(format!("{base_stem}.prefab"));
    let base_text = expand_prefab_file(
        prefabs_dir,
        &std::fs::read_to_string(&base_path)
            .map_err(|_| format!("base prefab {base_stem} was not found"))?,
    )?;
    let base_map = load_entity_map(&base_text)?;
    let full_map = load_entity_map(full_text)?;
    let value = parse(full_text).map_err(|e| e.to_string())?;
    let root = match value.get("root") {
        Some(SealValue::String(s)) => s.clone(),
        _ => return Err("prefab has no root".into()),
    };
    let mut patch_rows = Vec::new();
    for (id, entity) in &full_map {
        match base_map.get(id) {
            Some(base_row) => {
                if let Some(diff) = entity_row_diff(base_row, entity) {
                    patch_rows.push(entity_value(&diff));
                }
            }
            None => patch_rows.push(entity_value(entity)),
        }
    }
    Ok(SealValue::Map(
        vec![
            ("seal_prefab".into(), SealValue::Number("1".into())),
            ("prefab".into(), SealValue::String(prefab_name.into())),
            ("base".into(), SealValue::String(base_stem.into())),
            ("root".into(), SealValue::String(root)),
            ("entities".into(), SealValue::Array(patch_rows)),
        ]
        .into(),
    )
    .to_text())
}

/// Read a prefab file, expanding a variant when `prefabs_dir` is given.
pub fn read_prefab_for_world(
    prefabs_dir: Option<&Path>,
    rel_or_text: &str,
    is_path: bool,
) -> Result<String, String> {
    let text = if is_path {
        std::fs::read_to_string(rel_or_text).map_err(|e| e.to_string())?
    } else {
        rel_or_text.to_string()
    };
    match prefabs_dir {
        Some(dir) if is_variant_document(&text) => expand_prefab_file(dir, &text),
        Some(dir) if !text.contains("entities") => expand_prefab_file(dir, &text),
        _ => Ok(text),
    }
}

/// Create a new variant path beside `base_rel` inside `prefabs/`.
pub fn unique_variant_path(prefabs_dir: &Path, base_stem: &str) -> PathBuf {
    let mut name = format!("{base_stem} Variant");
    let mut n = 2;
    while prefabs_dir.join(format!("{name}.prefab")).exists() {
        name = format!("{base_stem} Variant {n}");
        n += 1;
    }
    prefabs_dir.join(format!("{name}.prefab"))
}

pub fn stem_from_rel(rel: &str) -> String {
    Path::new(rel).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_prefab(dir: &Path, name: &str, body: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(format!("{name}.prefab")), body).unwrap();
    }

    #[test]
    fn variants_inherit_base_fields_and_keep_overrides() {
        let dir = std::env::temp_dir().join(format!("seal-var-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let base = r#"seal_prefab = 1
prefab = "Crate"
root = "crate"
entities = [
  { id = "crate", name = "Crate", tag = "wood", metal = 0.1 }
]
"#;
        write_prefab(&dir, "Crate", base);
        let variant = r#"seal_prefab = 1
prefab = "Metal Crate"
base = "Crate"
root = "crate"
entities = [
  { id = "crate", tag = "metal", metal = 0.9 }
]
"#;
        let expanded = expand_prefab_file(&dir, variant).unwrap();
        assert!(!expanded.contains("wood") || expanded.contains("metal"));
        assert!(expanded.contains("metal = 0.9"));
        let map = load_entity_map(&expanded).unwrap();
        let row = map.get("crate").unwrap();
        assert_eq!(row.tag, "metal");
        assert!((row.metal - 0.9).abs() < 0.001);
    }

    #[test]
    fn damaged_base_is_refused() {
        let dir = std::env::temp_dir().join(format!("seal-var-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write_prefab(
            &dir,
            "A",
            "seal_prefab = 1\nprefab = \"A\"\nbase = \"Missing\"\nroot = \"a\"\nentities = []\n",
        );
        let err = expand_prefab_file(&dir, &std::fs::read_to_string(dir.join("A.prefab")).unwrap())
            .unwrap_err();
        assert!(err.contains("not found"), "{err}");
    }
}
