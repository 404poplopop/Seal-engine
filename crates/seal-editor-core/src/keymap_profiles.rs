//! Named keyboard shortcut profiles: save, switch, export, import, and chord conflicts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use seal_sealtext::{parse, SealValue};

use crate::editor_tools::{diff_tool_argv, external_editor_argv};
use crate::error::CoreError;
use crate::keymap::{Chord, Keymap};
use crate::persist::atomic_write;

const INDEX_SCHEMA: &str = "1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeymapConflict {
    pub chord: Chord,
    pub commands: Vec<String>,
}

/// Two or more commands bound to the same single-key chord.
pub fn binding_conflicts(keymap: &Keymap) -> Vec<KeymapConflict> {
    let mut by_chord: HashMap<Chord, Vec<String>> = HashMap::new();
    for (command, chord) in keymap.all_single_bindings() {
        by_chord.entry(chord).or_default().push(command);
    }
    let mut out: Vec<KeymapConflict> = by_chord
        .into_iter()
        .filter(|(_, commands)| commands.len() > 1)
        .map(|(chord, mut commands)| {
            commands.sort();
            KeymapConflict { chord, commands }
        })
        .collect();
    out.sort_by(|left, right| left.chord.token().cmp(&right.chord.token()));
    out
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProfileIndex {
    pub active: String,
    pub names: Vec<String>,
}

impl ProfileIndex {
    pub fn from_text(text: &str) -> Self {
        let Ok(value) = parse(text) else {
            return Self::default();
        };
        let Some(map) = value.as_map() else {
            return Self::default();
        };
        let active = map
            .iter()
            .find(|(k, _)| k == "active")
            .and_then(|(_, v)| match v {
                SealValue::String(text) => Some(text.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let names = map
            .iter()
            .find(|(k, _)| k == "profiles")
            .and_then(|(_, v)| match v {
                SealValue::Array(items) => Some(
                    items
                        .iter()
                        .filter_map(|item| match item {
                            SealValue::String(text) => Some(text.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .unwrap_or_default();
        Self { active, names }
    }

    pub fn to_text(&self) -> String {
        let profiles =
            SealValue::Array(self.names.iter().cloned().map(SealValue::String).collect());
        let map = vec![
            ("schema_version".into(), SealValue::Number(INDEX_SCHEMA.into())),
            ("active".into(), SealValue::String(self.active.clone())),
            ("profiles".into(), profiles),
        ];
        SealValue::Map(map.into()).to_text()
    }

    pub fn load(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        std::fs::read_to_string(path).ok().map(|text| Self::from_text(&text)).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), CoreError> {
        atomic_write(path, self.to_text().as_bytes())
    }
}

pub fn profiles_dir(user_data: &Path) -> PathBuf {
    user_data.join("keymap-profiles")
}

pub fn profile_index_path(user_data: &Path) -> PathBuf {
    user_data.join("keymap-profiles.seal")
}

pub fn profile_file_path(user_data: &Path, name: &str) -> PathBuf {
    profiles_dir(user_data).join(format!("{name}.seal"))
}

fn sanitize_name(name: &str) -> Result<String, CoreError> {
    let name = name.trim();
    if name.is_empty() || name.contains(['/', '\\', '\0']) {
        return Err(CoreError::new("profile name is invalid"));
    }
    Ok(name.to_string())
}

/// Save the current user overlay into a named profile file.
pub fn save_profile(user_data: &Path, keymap: &Keymap, name: &str) -> Result<(), CoreError> {
    let name = sanitize_name(name)?;
    let dir = profiles_dir(user_data);
    std::fs::create_dir_all(&dir).map_err(|e| CoreError::new(e.to_string()))?;
    let path = profile_file_path(user_data, &name);
    keymap.save(&path)?;
    let mut index = ProfileIndex::load(&profile_index_path(user_data));
    if !index.names.iter().any(|n| n == &name) {
        index.names.push(name.clone());
        index.names.sort();
    }
    index.active = name;
    index.save(&profile_index_path(user_data))?;
    Ok(())
}

pub fn switch_profile(user_data: &Path, _keymap: &Keymap, name: &str) -> Result<Keymap, CoreError> {
    let name = sanitize_name(name)?;
    let path = profile_file_path(user_data, &name);
    if !path.is_file() {
        return Err(CoreError::new(format!("profile '{name}' was not found")));
    }
    let loaded = Keymap::load(&path)?;
    let mut index = ProfileIndex::load(&profile_index_path(user_data));
    index.active = name;
    index.save(&profile_index_path(user_data))?;
    Ok(loaded)
}

pub fn export_profile(user_data: &Path, name: &str, dest: &Path) -> Result<(), CoreError> {
    let name = sanitize_name(name)?;
    let src = profile_file_path(user_data, &name);
    if !src.is_file() {
        return Err(CoreError::new(format!("profile '{name}' was not found")));
    }
    let text = std::fs::read_to_string(&src).map_err(|e| CoreError::new(e.to_string()))?;
    atomic_write(dest, text.as_bytes())
}

pub fn import_profile(user_data: &Path, src: &Path, name: &str) -> Result<(), CoreError> {
    let name = sanitize_name(name)?;
    let overlay = Keymap::load(src)?;
    let dir = profiles_dir(user_data);
    std::fs::create_dir_all(&dir).map_err(|e| CoreError::new(e.to_string()))?;
    let dest = profile_file_path(user_data, &name);
    overlay.save(&dest)?;
    let mut index = ProfileIndex::load(&profile_index_path(user_data));
    if !index.names.iter().any(|n| n == &name) {
        index.names.push(name.clone());
        index.names.sort();
    }
    index.active = name;
    index.save(&profile_index_path(user_data))?;
    Ok(())
}

/// Drop the first listed command from a conflicting chord and return the new keymap text overlay.
pub fn fix_conflict_drop_first(keymap: &Keymap, chord: &Chord) -> Result<Keymap, CoreError> {
    let conflicts = binding_conflicts(keymap);
    let Some(conflict) = conflicts.iter().find(|item| &item.chord == chord) else {
        return Err(CoreError::new("that chord is not in conflict"));
    };
    let drop =
        conflict.commands.first().ok_or_else(|| CoreError::new("conflict has no commands"))?;
    let mut next = keymap.clone();
    next.remove_command(drop);
    Ok(next)
}

use crate::command::CommandArgs;
use crate::session::{EditorSession, HostAction};

pub fn cmd_profile_save(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let name = args.get("name").ok_or_else(|| CoreError::new("profile save needs a name"))?;
    let user_data = session
        .keymap_path
        .as_ref()
        .and_then(|p| p.parent())
        .ok_or_else(|| CoreError::new("keymap path is not set"))?;
    save_profile(user_data, &session.keymap, name)?;
    session.models.keymap_profile_name = name.to_string();
    session.notify(session.messages.get("keymap-profile-saved"), false);
    Ok(())
}

pub fn cmd_profile_switch(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let name = args.get("name").ok_or_else(|| CoreError::new("profile switch needs a name"))?;
    let user_data = session
        .keymap_path
        .as_ref()
        .and_then(|p| p.parent())
        .ok_or_else(|| CoreError::new("keymap path is not set"))?;
    session.keymap = switch_profile(user_data, &session.keymap, name)?;
    session.save_keymap()?;
    session.models.keymap_profile_name = name.to_string();
    Ok(())
}

pub fn cmd_profile_export(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let name = args.get("name").unwrap_or(&session.models.keymap_profile_name);
    if name.trim().is_empty() {
        return Err(CoreError::new("pick a profile to export"));
    }
    let Some(path) = args.get("path").filter(|path| !path.trim().is_empty()) else {
        session.host_action = Some(HostAction::SaveProfile { name: name.trim().to_string() });
        return Ok(());
    };
    let user_data = session
        .keymap_path
        .as_ref()
        .and_then(|p| p.parent())
        .ok_or_else(|| CoreError::new("keymap path is not set"))?;
    export_profile(user_data, name, std::path::Path::new(path))?;
    session.notify(session.messages.get("keymap-profile-exported"), false);
    Ok(())
}

pub fn cmd_profile_import(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let Some(path) = args.get("path").filter(|path| !path.trim().is_empty()) else {
        let name = args.get("name").unwrap_or("").trim().to_string();
        session.host_action = Some(HostAction::OpenProfile { name });
        return Ok(());
    };
    let name = args.get("name").map(str::to_string).unwrap_or_else(|| {
        std::path::Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "imported".into())
    });
    let user_data = session
        .keymap_path
        .as_ref()
        .and_then(|p| p.parent())
        .ok_or_else(|| CoreError::new("keymap path is not set"))?;
    import_profile(user_data, std::path::Path::new(path), &name)?;
    session.models.keymap_profile_name = name;
    Ok(())
}

pub fn cmd_conflict_fix(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let chord = Chord::parse(
        args.get("chord").ok_or_else(|| CoreError::new("conflict fix needs a chord"))?,
    )?;
    session.keymap = fix_conflict_drop_first(&session.keymap, &chord)?;
    session.save_keymap()?;
    Ok(())
}

/// Text files the Asset Editor can hand to an external editor or a diff tool.
pub fn text_tool_file(name: &str) -> bool {
    matches!(
        crate::asset_editor::asset_kind(name),
        crate::asset_editor::AssetKind::Text
            | crate::asset_editor::AssetKind::Effect
            | crate::asset_editor::AssetKind::Screen
            | crate::asset_editor::AssetKind::Theme
    )
}

fn preference_set(session: &EditorSession, id: &str) -> bool {
    session.foundation.settings.get(id).is_some_and(|value| !value.trim().is_empty())
}

/// Open in External Editor is on when a text file is open and the editor path is set.
pub fn open_external_enabled(session: &EditorSession) -> bool {
    let Some((rel, _)) = crate::asset_editor::open_asset(session) else {
        return false;
    };
    text_tool_file(&rel) && preference_set(session, crate::editor_tools::EXTERNAL_EDITOR_PATH)
}

/// Compare with Saved is on when a diff tool is set and the open text differs from the opened copy.
pub fn compare_saved_enabled(session: &EditorSession) -> bool {
    let Some((rel, full)) = crate::asset_editor::open_asset(session) else {
        return false;
    };
    if !text_tool_file(&rel) || !preference_set(session, crate::editor_tools::DIFF_TOOL_PATH) {
        return false;
    }
    let Some((saved_rel, saved)) = &session.asset_saved_text else {
        return false;
    };
    if saved_rel != &rel {
        return false;
    }
    std::fs::read_to_string(full).unwrap_or_default() != *saved
}

pub fn cmd_open_external_editor(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let rel =
        args.get("path").ok_or_else(|| CoreError::new("open in external editor needs a path"))?;
    let root = session
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .ok_or_else(|| CoreError::new("open a project first"))?;
    let file = root.join(rel);
    let line: u32 = args.get("line").and_then(|t| t.parse().ok()).unwrap_or(1);
    let Some((exe, argv)) = external_editor_argv(&session.foundation.settings, &file, line) else {
        return Err(CoreError::new(session.messages.get("tools-external-editor-missing")));
    };
    session.host_action = Some(HostAction::RunExternal { exe, args: argv });
    Ok(())
}

pub fn cmd_compare_with_saved(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let rel = args.get("path").ok_or_else(|| CoreError::new("compare needs a path"))?;
    let root = session
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .ok_or_else(|| CoreError::new("open a project first"))?;
    let disk = root.join(rel);
    let Ok(draft) = std::fs::read_to_string(&disk) else {
        return Err(CoreError::new(session.messages.get("asset-compare-missing")));
    };
    let saved = if let Some(path) = args.get("saved").filter(|path| !path.is_empty()) {
        std::fs::read_to_string(path).map_err(|e| CoreError::new(e.to_string()))?
    } else if let Some((saved_rel, text)) = &session.asset_saved_text {
        if saved_rel != rel {
            return Err(CoreError::new(session.messages.get("asset-compare-missing")));
        }
        text.clone()
    } else {
        return Err(CoreError::new(session.messages.get("asset-compare-missing")));
    };
    if draft == saved {
        return Err(CoreError::new(session.messages.get("asset-compare-missing")));
    }
    let dir = std::env::temp_dir().join(format!("seal-diff-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let disk_path = dir.join("saved.txt");
    let draft_path = dir.join("draft.txt");
    std::fs::write(&disk_path, saved.as_bytes()).map_err(|e| CoreError::new(e.to_string()))?;
    std::fs::write(&draft_path, draft.as_bytes()).map_err(|e| CoreError::new(e.to_string()))?;
    let Some((exe, argv)) = diff_tool_argv(&session.foundation.settings, &disk_path, &draft_path)
    else {
        return Err(CoreError::new(session.messages.get("tools-diff-tool-missing")));
    };
    session.host_action = Some(HostAction::RunExternal { exe, args: argv });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Keymap;

    #[test]
    fn profile_round_trip_and_conflict_detection() {
        let dir = std::env::temp_dir().join(format!("seal-km-prof-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut base = Keymap::defaults().unwrap();
        base.override_bind("file.quit", Chord::parse("ctrl+shift+q").unwrap()).unwrap();
        save_profile(&dir, &base, "work").unwrap();
        assert!(base.bind("file.save", Chord::parse("ctrl+shift+q").unwrap()).is_err());
        base.push_binding("window.save-layout", Chord::parse("ctrl+shift+q").unwrap());
        let conflicts = binding_conflicts(&base);
        assert!(conflicts.iter().any(|c| c.commands.contains(&"file.quit".into())));
        let fixed = fix_conflict_drop_first(&base, &Chord::parse("ctrl+shift+q").unwrap()).unwrap();
        assert!(binding_conflicts(&fixed).is_empty());
        let export = dir.join("export.seal");
        export_profile(&dir, "work", &export).unwrap();
        import_profile(&dir, &export, "imported").unwrap();
        let merged = switch_profile(&dir, &Keymap::defaults().unwrap(), "imported").unwrap();
        assert_eq!(merged.command_for(&Chord::parse("ctrl+shift+q").unwrap()), Some("file.quit"),);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn damaged_index_falls_back_to_empty() {
        let index = ProfileIndex::from_text("not a map");
        assert!(index.names.is_empty());
        assert!(index.active.is_empty());
    }
}
