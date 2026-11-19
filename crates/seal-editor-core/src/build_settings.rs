//! Build scene list stored in `project.seal` for Export Game.

use std::path::Path;

use seal_sealtext::{parse, SealValue};

use crate::error::CoreError;
use crate::project;

pub const BUILD_SCENES_KEY: &str = "build_scenes";

/// Ordered scenes in the build; empty means "use scene.seal only".
pub fn read_build_scenes(folder: &Path) -> Result<Vec<String>, CoreError> {
    let text = std::fs::read_to_string(folder.join("project.seal"))?;
    let raw = parse(&text).map_err(|e| CoreError::new(e.to_string()))?;
    let settings = raw.get("settings").cloned().unwrap_or(SealValue::Map(Vec::new().into()));
    let Some(SealValue::Array(items)) = settings.get(BUILD_SCENES_KEY) else {
        return Ok(default_build_scenes(folder));
    };
    let scenes: Vec<String> = items
        .iter()
        .filter_map(|v| match v {
            SealValue::String(s) if !s.is_empty() => Some(s.replace('\\', "/")),
            _ => None,
        })
        .collect();
    if scenes.is_empty() {
        Ok(default_build_scenes(folder))
    } else {
        Ok(scenes)
    }
}

pub fn write_build_scenes(folder: &Path, scenes: &[String]) -> Result<(), CoreError> {
    project::set_setting(
        folder,
        BUILD_SCENES_KEY,
        SealValue::Array(scenes.iter().cloned().map(SealValue::String).collect()),
    )
}

pub fn default_build_scenes(folder: &Path) -> Vec<String> {
    let main = folder.join("scene.seal");
    if main.is_file() {
        vec!["scene.seal".into()]
    } else {
        Vec::new()
    }
}

pub fn discover_scenes(folder: &Path) -> Vec<String> {
    let mut out = Vec::new();
    walk_scenes(folder, folder, &mut out);
    out.sort();
    out
}

fn walk_scenes(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if entry.file_name() != ".seal-editor" && entry.file_name() != "target" {
                walk_scenes(root, &path, out);
            }
        } else if path.extension().is_some_and(|e| e == "seal")
            && path.file_name() != Some(std::ffi::OsStr::new("project.seal"))
        {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::create_project;

    #[test]
    fn build_list_round_trip() {
        let dir = std::env::temp_dir().join(format!("seal-build-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        create_project(&dir, "Game").unwrap();
        write_build_scenes(&dir, &["scene.seal".into(), "levels/b.seal".into()]).unwrap();
        let list = read_build_scenes(&dir).unwrap();
        assert_eq!(list, ["scene.seal", "levels/b.seal"]);
    }
}
