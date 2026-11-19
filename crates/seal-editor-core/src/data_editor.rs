//! Commands and helpers for Data Type (`.datatype`) and Data (`.data`) assets.

use seal_world::data_asset::{DataAsset, DataTypeDef, DATATYPE_EXT, DATA_EXT};

use crate::asset_editor::{open_asset, show_asset};
use crate::command::CommandArgs;
use crate::content::refresh_content_tree;
use crate::error::CoreError;
use crate::models::Past;
use crate::session::EditorSession;

fn project_root(session: &EditorSession) -> Result<std::path::PathBuf, CoreError> {
    session
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .ok_or_else(|| CoreError::new("open a project first"))
}

fn checked_rel(rel: &str) -> Result<String, CoreError> {
    let rel = rel.trim().replace('\\', "/");
    if rel.is_empty() || rel.starts_with('/') || rel.split('/').any(|p| p == "..") {
        return Err(CoreError::new(format!("'{rel}' is not a path inside the project")));
    }
    Ok(rel)
}

fn open_datatype(
    session: &EditorSession,
) -> Result<(String, std::path::PathBuf, DataTypeDef), CoreError> {
    let (rel, full) =
        open_asset(session).ok_or_else(|| CoreError::new("no file is open in the Asset Editor"))?;
    let text = std::fs::read_to_string(&full)?;
    let def = DataTypeDef::parse(&text).map_err(|e| CoreError::new(format!("{rel}: {e}")))?;
    Ok((rel, full, def))
}

/// Load a `.data` file and its type from a project-relative path.
pub fn load_data_asset(
    root: &std::path::Path,
    rel: &str,
) -> Result<(std::path::PathBuf, DataAsset, DataTypeDef), CoreError> {
    let rel = rel.trim().replace('\\', "/");
    if rel.is_empty() || rel.starts_with('/') || rel.split('/').any(|p| p == "..") {
        return Err(CoreError::new(format!("'{rel}' is not a path inside the project")));
    }
    let full = root.join(&rel);
    if !full.is_file() {
        return Err(CoreError::new(format!("{rel} was not found")));
    }
    let text = std::fs::read_to_string(&full)?;
    let mut raw = parse_data_type_only(&text).map_err(|e| CoreError::new(format!("{rel}: {e}")))?;
    let type_full = root.join(&raw.type_path);
    let type_text = std::fs::read_to_string(&type_full)
        .map_err(|_| CoreError::new(format!("data type {} was not found", raw.type_path)))?;
    let type_def = DataTypeDef::parse(&type_text)
        .map_err(|e| CoreError::new(format!("{}: {e}", raw.type_path)))?;
    raw = DataAsset::parse(&text, &type_def).map_err(|e| CoreError::new(format!("{rel}: {e}")))?;
    Ok((full, raw, type_def))
}

fn open_data(
    session: &EditorSession,
) -> Result<(String, std::path::PathBuf, DataAsset, DataTypeDef), CoreError> {
    let (rel, full) =
        open_asset(session).ok_or_else(|| CoreError::new("no file is open in the Asset Editor"))?;
    let text = std::fs::read_to_string(&full)?;
    let mut raw = parse_data_type_only(&text).map_err(|e| CoreError::new(format!("{rel}: {e}")))?;
    let root = project_root(session)?;
    let type_path = raw.type_path.clone();
    let type_full = root.join(&type_path);
    let type_text = std::fs::read_to_string(&type_full)
        .map_err(|_| CoreError::new(format!("data type {type_path} was not found")))?;
    let type_def =
        DataTypeDef::parse(&type_text).map_err(|e| CoreError::new(format!("{type_path}: {e}")))?;
    raw = DataAsset::parse(&text, &type_def).map_err(|e| CoreError::new(format!("{rel}: {e}")))?;
    Ok((rel, full, raw, type_def))
}

fn parse_data_type_only(text: &str) -> Result<DataAsset, seal_world::data_asset::DataAssetError> {
    let mut name = String::new();
    let mut type_path = String::new();
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        let mut parts = line.split_whitespace();
        let key = parts.next().unwrap_or("");
        match key {
            "name" => name = line.split_once('=').map(|(_, v)| v.trim()).unwrap_or("").to_string(),
            "type" => {
                type_path = line.split_once('=').map(|(_, v)| v.trim()).unwrap_or("").to_string();
            }
            _ => {}
        }
    }
    Ok(DataAsset { name, type_path, values: Vec::new() })
}

pub fn cmd_datatype_new(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let mut rel = checked_rel(args.get("path").unwrap_or("data/unit"))?;
    if !rel.ends_with(DATATYPE_EXT) {
        rel.push('.');
        rel.push_str(DATATYPE_EXT);
    }
    let root = project_root(session)?;
    let mut full = root.join(&rel);
    if full.exists() && args.get("unique") != Some("yes") {
        return Err(CoreError::new(format!("{rel} already exists")));
    }
    let mut n = 2;
    while full.exists() {
        let stem = rel.trim_end_matches(&format!(".{DATATYPE_EXT}"));
        rel = format!("{stem}_{n}.{DATATYPE_EXT}");
        full = root.join(&rel);
        n += 1;
    }
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let name = full.file_stem().and_then(|s| s.to_str()).unwrap_or("Type");
    let def = DataTypeDef {
        name: name.to_string(),
        fields: vec![seal_world::data_asset::DataFieldDef {
            name: "value".into(),
            kind: seal_world::data_asset::FieldKind::Number,
            element: None,
            options: String::new(),
            default: "0".into(),
        }],
    };
    crate::persist::atomic_write(&full, def.to_text().as_bytes())?;
    show_asset(session, &rel)?;
    refresh_content_tree(session);
    Ok(())
}

pub fn cmd_data_new(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let type_rel =
        checked_rel(args.get("type").ok_or_else(|| CoreError::new("choose a Data Type"))?)?;
    let mut rel = checked_rel(args.get("path").unwrap_or("data/value"))?;
    if !rel.ends_with(DATA_EXT) {
        rel.push('.');
        rel.push_str(DATA_EXT);
    }
    let root = project_root(session)?;
    let type_full = root.join(&type_rel);
    let type_text = std::fs::read_to_string(&type_full)
        .map_err(|_| CoreError::new(format!("data type {type_rel} was not found")))?;
    let type_def = DataTypeDef::parse(&type_text).map_err(|e| CoreError::new(e.to_string()))?;
    let mut full = root.join(&rel);
    if full.exists() && args.get("unique") != Some("yes") {
        return Err(CoreError::new(format!("{rel} already exists")));
    }
    let mut n = 2;
    while full.exists() {
        let stem = rel.trim_end_matches(&format!(".{DATA_EXT}"));
        rel = format!("{stem}_{n}.{DATA_EXT}");
        full = root.join(&rel);
        n += 1;
    }
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut asset = DataAsset {
        name: full.file_stem().and_then(|s| s.to_str()).unwrap_or("Data").into(),
        type_path: type_rel,
        values: Vec::new(),
    };
    for field in &type_def.fields {
        asset.values.push((field.name.clone(), field.default.clone()));
    }
    crate::persist::atomic_write(&full, asset.to_text().as_bytes())?;
    show_asset(session, &rel)?;
    refresh_content_tree(session);
    Ok(())
}

pub fn cmd_datatype_set(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let key = args.get("key").ok_or_else(|| CoreError::new("choose a field"))?;
    let value = args.get("value").unwrap_or_default();
    let (_, full, mut def) = open_datatype(session)?;
    match key.strip_prefix("field.") {
        Some(rest) => {
            let (index_s, field_key) =
                rest.split_once('.').ok_or_else(|| CoreError::new("choose a field property"))?;
            let index: usize = index_s.parse().map_err(|_| CoreError::new("choose a field"))?;
            let field = def
                .fields
                .get_mut(index)
                .ok_or_else(|| CoreError::new("that field does not exist"))?;
            match field_key {
                "name" => field.name = value.to_string(),
                "kind" => {
                    field.kind = seal_world::data_asset::FieldKind::parse(value)
                        .ok_or_else(|| CoreError::new("unknown field type"))?;
                }
                "default" => field.default = value.to_string(),
                _ => return Err(CoreError::new("unknown field property")),
            }
        }
        _ => return Err(CoreError::new("unknown setting")),
    }
    crate::persist::atomic_write(&full, def.to_text().as_bytes())?;
    Ok(())
}

pub fn cmd_datatype_field_add(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    let (_, full, mut def) = open_datatype(session)?;
    def.fields.push(seal_world::data_asset::DataFieldDef {
        name: format!("field{}", def.fields.len() + 1),
        kind: seal_world::data_asset::FieldKind::Text,
        element: None,
        options: String::new(),
        default: String::new(),
    });
    crate::persist::atomic_write(&full, def.to_text().as_bytes())?;
    Ok(())
}

pub fn cmd_data_set(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let field = args.get("field").ok_or_else(|| CoreError::new("choose a field"))?;
    let value = args.get("value").unwrap_or_default();
    let (full, mut asset, type_def) = match args.get("path") {
        Some(rel) => {
            let root = project_root(session)?;
            let (full, asset, type_def) = load_data_asset(&root, rel)?;
            (full, asset, type_def)
        }
        None => {
            let (_, full, asset, type_def) = open_data(session)?;
            (full, asset, type_def)
        }
    };
    let previous = std::fs::read_to_string(&full)?;
    seal_world::data_asset::validate_value(&type_def, field, value)
        .map_err(|e| CoreError::new(e.to_string()))?;
    if let Some((_, slot)) = asset.values.iter_mut().find(|(n, _)| n == field) {
        *slot = value.to_string();
    } else {
        asset.values.push((field.to_string(), value.to_string()));
    }
    let next = asset.to_text();
    if next != previous {
        crate::persist::atomic_write(&full, next.as_bytes())?;
        session.push_past(Past::File { path: full.to_string_lossy().to_string(), previous, next });
        session.future.clear();
    }
    Ok(())
}

#[cfg(test)]
mod data_set_tests {
    use crate::{create_project, CommandArgs, EditorSession, Layout};

    #[test]
    fn data_set_accepts_each_field_kind() {
        let dir = std::env::temp_dir().join(format!("seal-data-kinds-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        create_project(&dir, "Game").unwrap();
        let type_text = "\
seal_datatype = 1
name = All
field amount number default=0
field t text
field b bool
field c choice options=a|b
field tint colour
field path asset
field lst list list=text
";
        std::fs::create_dir_all(dir.join("data")).unwrap();
        std::fs::write(dir.join("data/all.datatype"), type_text).unwrap();
        let mut session = EditorSession::new(Layout::default());
        session
            .dispatch("file.open_project", &CommandArgs::new(&[("path", dir.to_str().unwrap())]))
            .unwrap();
        session
            .dispatch(
                "asset.data_new",
                &CommandArgs::new(&[
                    ("path", "data/row"),
                    ("type", "data/all.datatype"),
                    ("unique", "yes"),
                ]),
            )
            .unwrap();
        let rel = "data/row.data";
        for (field, value) in [
            ("amount", "3"),
            ("t", "hi"),
            ("b", "yes"),
            ("c", "b"),
            ("tint", "1,0,0,1"),
            ("path", "media/x.png"),
            ("lst", "[one,two]"),
        ] {
            session
                .dispatch(
                    "asset.data_set",
                    &CommandArgs::new(&[("path", rel), ("field", field), ("value", value)]),
                )
                .unwrap_or_else(|e| panic!("{field}: {e}"));
        }
    }
}
