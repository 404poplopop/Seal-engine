//! Merge Objects, and the scene option that stores each top-level object in its own file.

use crate::command::CommandArgs;
use crate::error::CoreError;
use crate::models;
use crate::session::EditorSession;

type CommandFn = fn(&mut EditorSession, &CommandArgs) -> Result<(), CoreError>;

pub const COMMANDS: &[(&str, &str, &str, CommandFn)] = &[
    ("scene.merge_objects", "command-scene-merge-objects", "edit", merge_objects),
    ("scene.object_files", "command-scene-object-files", "edit", object_files),
];

fn editing(session: &EditorSession) -> Result<(), CoreError> {
    if session.world.mode != seal_world::PlayMode::Editing {
        Err(CoreError::new("stop play before editing the scene"))
    } else {
        Ok(())
    }
}

fn merge_objects(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let mut ids = session.foundation.selection.clone();
    if let Some(id) = &session.models.selection {
        if !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    ids.retain(|id| id != "root");
    let previous = session.world.to_text();
    let id = session.world.merge_objects(&ids).map_err(CoreError::new)?;
    session.adopt_world();
    session.models.selection = Some(id.clone());
    session.foundation.selection = vec![id];
    models::push_scene_past(session, previous);
    session.mark_scene();
    Ok(())
}

fn object_files(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let on = match args.get("value") {
        Some("yes") | Some("1") | Some("true") => true,
        Some("no") | Some("0") | Some("false") => false,
        _ => !session.world.objects_per_file,
    };
    let previous = session.world.to_text();
    session.world.objects_per_file = on;
    session.adopt_world();
    models::push_scene_past(session, previous);
    session.mark_scene();
    if let Some(path) = session.scene_path() {
        let text = session.world.to_text();
        seal_world::object_files::write(&path, &text).map_err(CoreError::new)?;
        session.scene_dirty = false;
    }
    Ok(())
}
