//! Prefab variants, data assets, build scene list, and user templates.

use std::path::PathBuf;

use crate::build_settings::{discover_scenes, read_build_scenes, write_build_scenes};
use crate::command::CommandArgs;
use crate::session::EditorSession;

type CommandFn = fn(&mut EditorSession, &CommandArgs) -> Result<(), CoreError>;
use crate::data_editor::{
    cmd_data_new, cmd_data_set, cmd_datatype_field_add, cmd_datatype_new, cmd_datatype_set,
};
use crate::error::CoreError;
use crate::prefab_variant::cmd_create_variant;
use crate::user_templates;

pub const COMMANDS: &[(&str, &str, &str, CommandFn)] = &[
    ("prefab.create_variant", "command-prefab-create-variant", "edit", create_variant),
    ("asset.datatype_new", "command-asset-datatype-new", "file", datatype_new),
    ("asset.data_new", "command-asset-data-new", "file", data_new),
    ("asset.datatype_set", "command-asset-datatype-set", "edit", datatype_set),
    ("asset.datatype_field_add", "command-asset-datatype-field-add", "edit", datatype_field_add),
    ("asset.data_set", "command-asset-data-set", "edit", data_set),
    ("build.scenes_add", "command-build-scenes-add", "edit", build_scenes_add),
    ("build.scenes_remove", "command-build-scenes-remove", "edit", build_scenes_remove),
    ("build.scenes_move", "command-build-scenes-move", "edit", build_scenes_move),
    ("project.save_template", "command-project-save-template", "file", save_template),
    ("project.open_build", "command-project-open-build", "tools", open_build),
];

fn project_root(session: &EditorSession) -> Result<std::path::PathBuf, CoreError> {
    session
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .ok_or_else(|| CoreError::new("open a project first"))
}

fn create_variant(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    cmd_create_variant(session, args)
}

fn datatype_new(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    cmd_datatype_new(session, args)
}

fn data_new(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    cmd_data_new(session, args)
}

fn datatype_set(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    cmd_datatype_set(session, args)
}

fn datatype_field_add(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    cmd_datatype_field_add(session, args)
}

fn data_set(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    cmd_data_set(session, args)
}

fn build_scenes_add(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let root = project_root(session)?;
    let path = args.get("path").ok_or_else(|| CoreError::new("choose a scene"))?.replace('\\', "/");
    let mut scenes = read_build_scenes(&root)?;
    if !scenes.iter().any(|s| s == &path) {
        scenes.push(path);
        write_build_scenes(&root, &scenes)?;
        session.notify("Added scene to the build list", false);
    }
    Ok(())
}

fn build_scenes_remove(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let root = project_root(session)?;
    let index: usize = args
        .get("index")
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| CoreError::new("choose a scene"))?;
    let mut scenes = read_build_scenes(&root)?;
    if index < scenes.len() {
        scenes.remove(index);
        write_build_scenes(&root, &scenes)?;
    }
    Ok(())
}

fn build_scenes_move(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let root = project_root(session)?;
    let index: usize = args
        .get("index")
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| CoreError::new("choose a scene"))?;
    let dir = args.get("direction").unwrap_or("down");
    let mut scenes = read_build_scenes(&root)?;
    let other = match dir {
        "up" if index > 0 => index - 1,
        "down" if index + 1 < scenes.len() => index + 1,
        _ => return Ok(()),
    };
    scenes.swap(index, other);
    write_build_scenes(&root, &scenes)?;
    Ok(())
}

fn save_template(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let root = project_root(session)?;
    let title = args.get("title").unwrap_or(&session.project.as_ref().unwrap().name);
    let description = args.get("description").unwrap_or_default();
    let preview = args.get("preview_path").map(PathBuf::from).and_then(|p| std::fs::read(p).ok());
    let tpl =
        user_templates::save_project_as_template(&root, title, description, preview.as_deref())?;
    session.notify(format!("Saved template {}", tpl.title), false);
    Ok(())
}

fn open_build(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.reveal_panel = Some("build".into());
    let root = project_root(session)?;
    let all = discover_scenes(&root);
    let picked = read_build_scenes(&root)?;
    session.set_draft("build.discovered", &all.join("\n"))?;
    session.set_draft("build.picked", &picked.join("\n"))?;
    Ok(())
}

/// Whether a wizard template id is a user-saved template.
pub fn wizard_user_template(id: &str) -> Option<user_templates::UserTemplate> {
    user_templates::user_template_by_id(id)
}

pub fn apply_wizard_user_template(
    folder: &std::path::Path,
    name: &str,
    id: &str,
) -> Result<(), CoreError> {
    user_templates::apply_user_template(folder, name, id)
}
