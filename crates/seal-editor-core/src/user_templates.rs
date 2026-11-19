//! User project templates saved beside built-in wizard choices.

use std::path::{Path, PathBuf};

use seal_sealtext::{parse, SealValue};

use crate::error::CoreError;
use crate::project;
use crate::workspace::user_state_dir;

pub const TEMPLATE_MANIFEST: &str = "template.seal";
pub const PREVIEW_PNG: &str = "preview.png";

#[derive(Clone, Debug)]
pub struct UserTemplate {
    pub id: String,
    pub folder: PathBuf,
    pub title: String,
    pub description: String,
    pub preview: PathBuf,
}

pub fn templates_dir() -> PathBuf {
    user_state_dir().join("templates")
}

pub fn list_user_templates() -> Vec<UserTemplate> {
    let root = templates_dir();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Ok(tpl) = read_template(&path) {
            out.push(tpl);
        }
    }
    out.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    out
}

fn read_template(folder: &Path) -> Result<UserTemplate, CoreError> {
    let manifest = folder.join(TEMPLATE_MANIFEST);
    let text = std::fs::read_to_string(&manifest)
        .map_err(|_| CoreError::new("template manifest missing"))?;
    let raw = parse(&text).map_err(|e| CoreError::new(e.to_string()))?;
    let id = folder
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "template".into());
    let title = match raw.get("title") {
        Some(SealValue::String(s)) => s.clone(),
        _ => id.clone(),
    };
    let description = match raw.get("description") {
        Some(SealValue::String(s)) => s.clone(),
        _ => String::new(),
    };
    Ok(UserTemplate {
        id,
        folder: folder.to_path_buf(),
        title,
        description,
        preview: folder.join(PREVIEW_PNG),
    })
}

/// Copy the open project into `templates/<id>/data` and write metadata.
pub fn save_project_as_template(
    project_root: &Path,
    title: &str,
    description: &str,
    preview_png: Option<&[u8]>,
) -> Result<UserTemplate, CoreError> {
    let id = slug(title);
    let folder = templates_dir().join(&id);
    if folder.exists() {
        std::fs::remove_dir_all(&folder)?;
    }
    std::fs::create_dir_all(&folder)?;
    copy_tree(project_root, &folder.join("data"))?;
    let preview = folder.join(PREVIEW_PNG);
    if let Some(bytes) = preview_png {
        std::fs::write(&preview, bytes)?;
    } else if let Some(fallback) = first_project_png(project_root) {
        let _ = std::fs::copy(fallback, &preview);
    }
    let manifest = SealValue::Map(
        vec![
            ("title".into(), SealValue::String(title.into())),
            ("description".into(), SealValue::String(description.into())),
        ]
        .into(),
    );
    crate::persist::atomic_write(&folder.join(TEMPLATE_MANIFEST), manifest.to_text().as_bytes())?;
    read_template(&folder)
}

fn slug(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let cleaned = cleaned.trim_matches('_').to_lowercase();
    if cleaned.is_empty() {
        format!("template-{}", std::process::id())
    } else {
        cleaned
    }
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), CoreError> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == ".seal-editor" || name == "target" || name.to_string_lossy().starts_with('.') {
            continue;
        }
        let src = entry.path();
        let dst = to.join(name);
        if src.is_dir() {
            copy_tree(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

fn first_project_png(root: &Path) -> Option<PathBuf> {
    for rel in ["art/icon.png", "sprites/player.png", "art/tiles.png"] {
        let p = root.join(rel);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn create_from_user_template(
    folder: &Path,
    name: &str,
    tpl: &UserTemplate,
) -> Result<(), CoreError> {
    copy_tree(&tpl.folder.join("data"), folder)?;
    let manifest = folder.join("project.seal");
    if manifest.is_file() {
        project::set_setting(folder, "name", SealValue::String(name.into()))?;
    }
    Ok(())
}

pub fn user_template_by_id(id: &str) -> Option<UserTemplate> {
    list_user_templates().into_iter().find(|t| t.id == id)
}

pub fn apply_user_template(folder: &Path, name: &str, id: &str) -> Result<(), CoreError> {
    let tpl =
        user_template_by_id(id).ok_or_else(|| CoreError::new("that template was not found"))?;
    create_from_user_template(folder, name, &tpl)
}
