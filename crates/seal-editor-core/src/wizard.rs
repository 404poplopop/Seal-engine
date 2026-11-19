use std::fs;
use std::path::{Path, PathBuf};

use crate::error::CoreError;
use crate::messages::Messages;
use crate::project::{self, CreateProjectOptions, OpenProject};
use crate::session::EditorSession;

pub const DRAFT_NAME: &str = "wizard-name";
pub const DRAFT_FOLDER: &str = "wizard-folder";
pub const DRAFT_GIT: &str = "wizard-git";
pub const DRAFT_LFS: &str = "wizard-lfs";
pub const DRAFT_CONFIRM_NONEMPTY: &str = "wizard-confirm-nonempty";
/// Which starter a new project gets. The id is one of the registered templates.
pub const DRAFT_TEMPLATE: &str = "wizard-template";

/// The template id the wizard will use (`2d` by default).
pub fn wizard_template(session: &EditorSession) -> String {
    let id = session.draft(DRAFT_TEMPLATE).unwrap_or_else(|| "2d".into());
    if crate::prefab_data_cmds::wizard_user_template(&id).is_some() {
        return id;
    }
    session
        .draft(DRAFT_TEMPLATE)
        .as_deref()
        .and_then(crate::template_registry::template_by_id)
        .map(|template| template.id.to_string())
        .unwrap_or_else(|| "2d".into())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WizardFieldError {
    pub key: String,
    pub args: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default)]
pub struct WizardValidation {
    pub name: Option<WizardFieldError>,
    pub folder: Option<WizardFieldError>,
}

pub fn default_projects_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(profile) = std::env::var("USERPROFILE") {
            return PathBuf::from(profile).join("Documents").join("Seal Projects");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join("Documents").join("Seal Projects");
    }
    std::env::temp_dir().join("Seal Projects")
}

pub fn flag_from_draft(session: &EditorSession, key: &str, default: bool) -> bool {
    match session.draft(key).as_deref() {
        Some("1" | "true" | "on" | "yes") => true,
        Some("0" | "false" | "off" | "no") => false,
        Some(_) => default,
        None => default,
    }
}

pub fn validate_wizard(
    messages: &Messages,
    name: &str,
    folder: &Path,
    confirm_nonempty: bool,
) -> WizardValidation {
    let mut out = WizardValidation::default();
    let trimmed = name.trim();
    if trimmed.is_empty() {
        out.name = Some(field_error("wizard-error-name-empty", messages, &[]));
    } else if trimmed.len() > 64 {
        out.name = Some(field_error("wizard-error-name-length", messages, &[("max", "64")]));
    } else if trimmed == "." || trimmed == ".." {
        out.name = Some(field_error("wizard-error-name-reserved", messages, &[]));
    } else if trimmed
        .chars()
        .any(|ch| matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'))
    {
        out.name = Some(field_error("wizard-error-name-chars", messages, &[]));
    }
    if folder.as_os_str().is_empty() {
        out.folder = Some(field_error("wizard-error-folder-empty", messages, &[]));
        return out;
    }
    if folder.as_os_str().len() > 240 {
        out.folder = Some(field_error("wizard-error-folder-length", messages, &[]));
        return out;
    }
    if folder.exists() {
        if !folder_is_writable(folder) {
            out.folder = Some(field_error("wizard-error-folder-readonly", messages, &[]));
            return out;
        }
        if project::folder_has_files(folder) && !confirm_nonempty {
            out.folder = Some(field_error("wizard-error-folder-not-empty", messages, &[]));
        }
    } else if let Some(parent) = folder.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            if let Err(err) = fs::create_dir_all(parent) {
                out.folder = Some(field_error(
                    "wizard-error-folder-create",
                    messages,
                    &[("reason", &err.to_string())],
                ));
                return out;
            }
        }
        if let Some(parent) = folder.parent() {
            if !folder_is_writable(parent) {
                out.folder = Some(field_error("wizard-error-folder-readonly", messages, &[]));
            }
        }
    }
    out
}

fn field_error(key: &str, messages: &Messages, args: &[(&str, &str)]) -> WizardFieldError {
    let owned: Vec<(String, String)> =
        args.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    let _ = messages.format(key, args);
    WizardFieldError { key: key.into(), args: owned }
}

fn folder_is_writable(folder: &Path) -> bool {
    project::folder_is_writable(folder)
}

pub fn wizard_folder(session: &EditorSession) -> PathBuf {
    session
        .draft(DRAFT_FOLDER)
        .filter(|text| !text.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_projects_dir)
}

pub fn wizard_name(session: &EditorSession) -> String {
    // An untouched name follows the folder, so a new project in .../Hop3 is called Hop3.
    let typed =
        session.draft(DRAFT_NAME).filter(|name| !name.trim().is_empty() && name != "project");
    typed.unwrap_or_else(|| {
        wizard_folder(session)
            .file_name()
            .and_then(|part| part.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| "project".into())
    })
}

pub fn wizard_options(session: &EditorSession) -> CreateProjectOptions {
    let git = flag_from_draft(session, DRAFT_GIT, false);
    CreateProjectOptions {
        init_git: git,
        git_lfs: git && flag_from_draft(session, DRAFT_LFS, true),
        confirm_nonempty: flag_from_draft(session, DRAFT_CONFIRM_NONEMPTY, false),
    }
}

pub fn create_from_wizard(session: &EditorSession) -> Result<OpenProject, CoreError> {
    let messages = &session.messages;
    let name = wizard_name(session);
    let folder = wizard_folder(session);
    let options = wizard_options(session);
    if !folder.is_absolute() {
        let shown = folder.to_string_lossy();
        return Err(CoreError::new(
            messages.format("path-prompt-absolute", &[("path", shown.as_ref())]),
        ));
    }
    let checks = validate_wizard(messages, &name, &folder, options.confirm_nonempty);
    if checks.name.is_some() || checks.folder.is_some() {
        let first = checks.name.or(checks.folder).expect("validation should have a field error");
        return Err(CoreError::new(messages.format(&first.key, &arg_refs(&first.args))));
    }
    let project = project::create_project_with_options(&folder, &name, &options)?;
    let id = wizard_template(session);
    crate::template_registry::apply_template(&folder, &name, &id, &options)?;
    Ok(project)
}

fn arg_refs(args: &[(String, String)]) -> Vec<(&str, &str)> {
    args.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect()
}

pub fn open_wizard(session: &mut EditorSession) {
    session.wizard_open = true;
    session.reveal_panel = Some("wizard.project".into());
    if session.draft(DRAFT_NAME).is_none() {
        let _ = session.set_draft(DRAFT_NAME, "project");
    }
    if session.draft(DRAFT_FOLDER).is_none() {
        // A new project gets its own new folder inside the projects folder (project, project2, ...).
        let projects = default_projects_dir();
        let folder = (1..)
            .map(|n| projects.join(if n == 1 { "project".into() } else { format!("project{n}") }))
            .find(|dir| !dir.exists())
            .unwrap_or_else(|| projects.clone());
        let _ = session.set_draft(DRAFT_FOLDER, folder.to_string_lossy().as_ref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("seal-wizard-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn wizard_rejects_invalid_names_and_nonempty_folders() {
        let messages = Messages::english();
        let dir = scratch("validate");
        let folder = dir.join("target");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("leftover.txt"), b"x").unwrap();
        let bad = validate_wizard(&messages, "bad?name", &folder, false);
        assert_eq!(bad.name.as_ref().map(|e| e.key.as_str()), Some("wizard-error-name-chars"));
        let nonempty = validate_wizard(&messages, "Game", &folder, false);
        assert_eq!(
            nonempty.folder.as_ref().map(|e| e.key.as_str()),
            Some("wizard-error-folder-not-empty")
        );
        let ok = validate_wizard(&messages, "Game", &folder, true);
        assert!(ok.name.is_none() && ok.folder.is_none());
        let _ = fs::remove_dir_all(&dir);
    }
}
