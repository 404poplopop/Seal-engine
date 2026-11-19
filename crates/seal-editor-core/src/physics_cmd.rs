//! Commands for the Physics panel. Each one records undo and marks the scene dirty.

use crate::command::CommandArgs;
use crate::error::CoreError;
use crate::models::push_scene_past;
use crate::session::EditorSession;

fn arg(args: &CommandArgs, key: &str) -> Result<String, CoreError> {
    args.get(key).map(str::to_string).ok_or_else(|| CoreError::new(format!("missing {key}")))
}

fn commit(session: &mut EditorSession, previous: String) {
    push_scene_past(session, previous);
    session.mark_scene();
}

pub fn add_world(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    let previous = session.world.to_text();
    let gravity = session.world.gravity;
    session.world.physics.scene.add_world(gravity);
    commit(session, previous);
    Ok(())
}

pub fn remove_world(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let name = arg(args, "name")?;
    if name == "main" {
        return Err(CoreError::new("the main physics world stays"));
    }
    let previous = session.world.to_text();
    session.world.physics.scene.remove_world(&name);
    commit(session, previous);
    Ok(())
}

pub fn set_world(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let name = arg(args, "name")?;
    let field = arg(args, "field")?;
    let value = arg(args, "value")?;
    let gravity = session.world.gravity;
    let previous = session.world.to_text();
    session
        .world
        .physics
        .scene
        .set_world_field(&name, &field, &value, gravity)
        .map_err(CoreError::new)?;
    commit(session, previous);
    Ok(())
}

pub fn set_member(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let world = arg(args, "world")?;
    let ids = targets(session, args)?;
    if !world.is_empty() && !session.world.physics.scene.world_names().iter().any(|n| n == &world) {
        return Err(CoreError::new("that physics world is not in this scene"));
    }
    let previous = session.world.to_text();
    for id in ids {
        session.world.physics.scene.set_member(&id, &world);
    }
    commit(session, previous);
    Ok(())
}

pub fn set_collider(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let field = arg(args, "field")?;
    let value = arg(args, "value")?;
    let ids = targets(session, args)?;
    let mut trial = session.world.physics.scene.clone();
    for id in &ids {
        trial.set_collider_field(id, &field, &value).map_err(CoreError::new)?;
    }
    let previous = session.world.to_text();
    session.world.physics.scene = trial;
    commit(session, previous);
    Ok(())
}

fn targets(session: &EditorSession, args: &CommandArgs) -> Result<Vec<String>, CoreError> {
    if let Some(id) = args.get("id") {
        if id.is_empty() {
            return Err(CoreError::new("select an object"));
        }
        if session.world.entity(id).is_none() {
            return Err(CoreError::new("that object is not in the scene"));
        }
        return Ok(vec![id.to_string()]);
    }
    let ids = session.selected_ids();
    if ids.is_empty() {
        return Err(CoreError::new("select an object"));
    }
    Ok(ids)
}
