//! The two vocabularies for the same data: scene-graph mode (entities,
//! components, prefabs, children) and the class vocabulary (instances,
//! classes, parent classes, class defaults, attached parts).
//!
//! Storage never changes; only the words in the files do. Converting is a
//! key-for-key rename at known places in prefab and scene files, so for a
//! file as Seal writes it, classic → scene graph → classic gives back the
//! same bytes, comments included.

use seal_sealtext::{parse, SealValue};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vocabulary {
    SceneGraph,
    Classic,
}

/// Component names that mean something in the classic vocabulary and so cannot be user components.
pub(crate) const RESERVED: &[&str] = &["class", "slot"];

// (scene-graph word, classic word)
const FILE_KEYS: &[(&str, &str)] = &[
    ("seal_prefab", "seal_class"),
    ("prefab", "class"),
    ("base", "parent_class"),
    ("components", "defaults"),
    ("children", "attached"),
    ("entities", "instances"),
];
const CHILD_KEYS: &[(&str, &str)] = &[("prefab", "class"), ("components", "defaults")];
const ENTITY_COMPONENT_KEYS: &[(&str, &str)] = &[("prefab", "class"), ("prefab_slot", "slot")];

fn rename_keys(
    value: &mut SealValue,
    table: &[(&str, &str)],
    to: Vocabulary,
    place: &str,
) -> Result<(), String> {
    let Some(entries) = value.as_map_mut() else {
        return Ok(());
    };
    let (from_words, to_words): (Vec<&str>, Vec<&str>) = match to {
        Vocabulary::Classic => table.iter().map(|(s, c)| (*s, *c)).unzip(),
        Vocabulary::SceneGraph => table.iter().map(|(s, c)| (*c, *s)).unzip(),
    };
    // Refuse input that already uses the target words, or the rename would not be reversible.
    for (key, _) in entries.iter() {
        if to_words.contains(&key.as_str()) && !from_words.contains(&key.as_str()) {
            return Err(format!(
                "{place} already uses the word \"{key}\"; is the file already in that vocabulary?"
            ));
        }
    }
    for (key, _) in entries.iter_mut() {
        if let Some(i) = from_words.iter().position(|w| *w == key.as_str()) {
            *key = to_words[i].to_string();
        }
    }
    Ok(())
}

fn convert(text: &str, to: Vocabulary) -> Result<String, String> {
    let mut file = parse(text).map_err(|e| e.to_string())?;
    if file.as_map().is_none() {
        return Err("expected a prefab, class or scene file".into());
    }
    rename_keys(&mut file, FILE_KEYS, to, "the file")?;
    let (children_key, entities_key) = match to {
        Vocabulary::Classic => ("attached", "instances"),
        Vocabulary::SceneGraph => ("children", "entities"),
    };
    let entries = file.as_map_mut().expect("checked");
    for (key, value) in entries.iter_mut() {
        if key == children_key {
            convert_children(value, to)?;
        }
        if key == entities_key {
            if let SealValue::Array(rows) = value {
                for row in rows {
                    if let Some(row_entries) = row.as_map_mut() {
                        for (k, components) in row_entries.iter_mut() {
                            if k == "components" {
                                rename_keys(components, ENTITY_COMPONENT_KEYS, to, "an entity")?;
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(file.to_text())
}

fn convert_children(value: &mut SealValue, to: Vocabulary) -> Result<(), String> {
    if let SealValue::Array(children) = value {
        for child in children {
            rename_keys(child, CHILD_KEYS, to, "a child")?;
        }
    }
    Ok(())
}

/// Rewrite a prefab or scene file in the classic vocabulary.
pub fn to_classic(text: &str) -> Result<String, String> {
    convert(text, Vocabulary::Classic)
}

/// Rewrite a class or scene file in the scene-graph vocabulary.
pub fn to_scene_graph(text: &str) -> Result<String, String> {
    convert(text, Vocabulary::SceneGraph)
}

/// Which vocabulary a file is written in.
pub fn detect(value: &SealValue) -> Vocabulary {
    let classic = ["seal_class", "class", "instances", "parent_class", "defaults", "attached"];
    if value.as_map().is_some_and(|m| m.iter().any(|(k, _)| classic.contains(&k.as_str()))) {
        Vocabulary::Classic
    } else {
        Vocabulary::SceneGraph
    }
}
