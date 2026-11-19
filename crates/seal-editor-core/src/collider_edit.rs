//! Viewport edits of a polygon or edge collider, and an outline taken from a sprite.
//!
//! One drag is one undo step. The scene autosaves.

use std::path::Path;

use crate::command::CommandArgs;
use crate::error::CoreError;
use crate::models::Past;
use crate::session::EditorSession;

/// `id` `op` (`point`, `insert`, `delete`) `index` `x` `y` `begin`.
/// `x` and `y` are world positions for `point` and `insert`.
pub(crate) fn edit_cmd(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    if session.world.mode != seal_world::PlayMode::Editing {
        return Err(CoreError::new("stop play before editing the scene"));
    }
    let id = args.get("id").unwrap_or("").to_string();
    let op = args.get("op").unwrap_or("");
    let index: usize = args
        .get("index")
        .unwrap_or("0")
        .parse()
        .map_err(|_| CoreError::new("point index must be a number"))?;
    let coord = |key: &str| {
        args.get(key).unwrap_or("0").parse::<f32>().ok().filter(|n| n.is_finite()).unwrap_or(0.0)
    };
    let world = [coord("x"), coord("y")];
    let begin = args.get("begin") == Some("1");
    let Some(entity) = session.world.entity(&id) else {
        return Err(CoreError::new("unknown entity"));
    };
    if entity.locked {
        return Err(CoreError::new("the entity is locked"));
    }
    if begin || !matches!(session.past.last(), Some(Past::Scene { .. })) {
        let previous = session.world.to_text();
        let label = session.undo_step_label_now();
        session.push_past(Past::Scene { previous: previous.clone(), next: previous, label });
        session.future.clear();
    }
    let changed = {
        let Some(entity) = session.world.entity_mut(&id) else {
            return Err(CoreError::new("unknown entity"));
        };
        match op {
            "point" => seal_world::collider_edit::move_point(entity, index, world),
            "insert" => seal_world::collider_edit::insert_point(entity, index, world),
            "delete" => seal_world::collider_edit::delete_point(entity, index),
            _ => false,
        }
    };
    if !changed {
        session.past.pop();
        return Err(CoreError::new("could not edit the collider"));
    }
    let next = session.world.to_text();
    if let Some(Past::Scene { next: slot, .. }) = session.past.last_mut() {
        *slot = next;
    }
    session.adopt_world();
    session.mark_scene();
    Ok(())
}

/// `id`: replace the polygon with the opaque outline of the object's sprite.
pub(crate) fn outline_cmd(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    if session.world.mode != seal_world::PlayMode::Editing {
        return Err(CoreError::new("stop play before editing the scene"));
    }
    let id = args.get("id").unwrap_or("").to_string();
    let Some(entity) = session.world.entity(&id) else {
        return Err(CoreError::new("unknown entity"));
    };
    if entity.locked {
        return Err(CoreError::new("the entity is locked"));
    }
    let sprite = entity.sprite.trim().to_string();
    if sprite.is_empty() {
        return Err(CoreError::new("the object has no sprite"));
    }
    let root = session
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .or_else(|| session.world.project_dir.clone())
        .ok_or_else(|| CoreError::new("no project folder"))?;
    let path = root.join(sprite.trim_start_matches(['/', '\\']));
    let (pixels, width, height) =
        decode_picture(&path).ok_or_else(|| CoreError::new("the picture could not be read"))?;
    let points =
        seal_world::collider_edit::trace_alpha(&pixels, width, height).map_err(CoreError::new)?;
    let previous = session.world.to_text();
    let label = session.undo_step_label_now();
    session.push_past(Past::Scene { previous: previous.clone(), next: previous, label });
    session.future.clear();
    {
        let Some(entity) = session.world.entity_mut(&id) else {
            session.past.pop();
            return Err(CoreError::new("unknown entity"));
        };
        entity.phys2d.shape = "polygon".into();
        entity.phys2d.points = points;
        if entity.phys2d.body.is_empty() {
            entity.phys2d.part = true;
        }
    }
    let next = session.world.to_text();
    if let Some(Past::Scene { next: slot, .. }) = session.past.last_mut() {
        *slot = next;
    }
    session.adopt_world();
    session.mark_scene();
    Ok(())
}

fn decode_picture(path: &Path) -> Option<(Vec<u8>, u32, u32)> {
    crate::asset_editor::decode_png(path)
}
