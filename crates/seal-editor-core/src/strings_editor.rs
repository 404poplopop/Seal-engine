//! Commands for editing a string table in the Asset Editor.

use std::path::Path;

use seal_world::text_sprites::{self, StringTable, STRINGS_EXT};

use crate::asset_editor::{asset_kind, open_asset, show_asset, AssetKind};
use crate::command::CommandArgs;
use crate::error::CoreError;
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

fn open_table(
    session: &EditorSession,
) -> Result<(String, std::path::PathBuf, StringTable), CoreError> {
    let (rel, full) = open_asset(session)
        .ok_or_else(|| CoreError::new("no string table is open in the Asset Editor"))?;
    if asset_kind(&rel) != AssetKind::Strings {
        return Err(CoreError::new("the open file is not a string table"));
    }
    let text = std::fs::read_to_string(&full)?;
    let table = StringTable::parse(&text).map_err(|e| CoreError::new(format!("{rel}: {e}")))?;
    Ok((rel, full, table))
}

fn save_table(full: &Path, table: &StringTable) -> Result<(), CoreError> {
    crate::persist::atomic_write(full, table.to_text().as_bytes())?;
    text_sprites::invalidate_table(full);
    Ok(())
}

fn key_of(args: &CommandArgs) -> Result<String, CoreError> {
    let key = args.get("key").unwrap_or("").trim();
    if !text_sprites::is_table_key(key) {
        return Err(CoreError::new("a key starts with a letter or _"));
    }
    Ok(key.to_string())
}

/// `asset.strings_new path= [unique=yes]`: create a string table and open it.
pub(crate) fn cmd_strings_new(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let mut rel = checked_rel(args.get("path").unwrap_or("ui/text"))?;
    if !rel.to_ascii_lowercase().ends_with(&format!(".{STRINGS_EXT}")) {
        rel.push('.');
        rel.push_str(STRINGS_EXT);
    }
    let root = project_root(session)?;
    let mut full = root.join(&rel);
    if full.exists() {
        if args.get("unique") != Some("yes") {
            return Err(CoreError::new(format!("{rel} already exists")));
        }
        let stem = rel.trim_end_matches(&format!(".{STRINGS_EXT}")).to_string();
        let mut n = 2;
        while full.exists() {
            rel = format!("{stem}_{n}.{STRINGS_EXT}");
            full = root.join(&rel);
            n += 1;
        }
    }
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut table = StringTable::default();
    table.rows.push(("hello".into(), "Hello".into()));
    save_table(&full, &table)?;
    show_asset(session, &rel)
}

/// `asset.strings_set key= value=`: change one line and save.
pub(crate) fn cmd_strings_set(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let key = key_of(args)?;
    let value = args.get("value").unwrap_or_default();
    let (_, full, mut table) = open_table(session)?;
    let row = table
        .rows
        .iter_mut()
        .find(|(k, _)| k == &key)
        .ok_or_else(|| CoreError::new(format!("there is no line called '{key}'")))?;
    row.1 = value.to_string();
    save_table(&full, &table)
}

/// `asset.strings_add`: append a new line.
pub(crate) fn cmd_strings_add(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    let (_, full, mut table) = open_table(session)?;
    let mut n = table.rows.len() + 1;
    let mut key = format!("line{n}");
    while table.rows.iter().any(|(k, _)| k == &key) {
        n += 1;
        key = format!("line{n}");
    }
    table.rows.push((key, String::new()));
    save_table(&full, &table)
}

/// `asset.strings_remove key=`: delete one line.
pub(crate) fn cmd_strings_remove(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let key = key_of(args)?;
    let (_, full, mut table) = open_table(session)?;
    let before = table.rows.len();
    table.rows.retain(|(k, _)| k != &key);
    if table.rows.len() == before {
        return Err(CoreError::new(format!("there is no line called '{key}'")));
    }
    save_table(&full, &table)
}

/// `asset.strings_rename from= to=`: rename a key and save.
pub(crate) fn cmd_strings_rename(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let from = args.get("from").unwrap_or("").trim();
    if !text_sprites::is_table_key(from) {
        return Err(CoreError::new("a key starts with a letter or _"));
    }
    let to = args.get("to").unwrap_or("").trim();
    if !text_sprites::is_table_key(to) {
        return Err(CoreError::new("a key starts with a letter or _"));
    }
    let (_, full, mut table) = open_table(session)?;
    if table.rows.iter().any(|(k, _)| k == to) && from != to {
        return Err(CoreError::new(format!("there is already a line called '{to}'")));
    }
    let row = table
        .rows
        .iter_mut()
        .find(|(k, _)| k == from)
        .ok_or_else(|| CoreError::new(format!("there is no line called '{from}'")))?;
    row.0 = to.to_string();
    save_table(&full, &table)
}
