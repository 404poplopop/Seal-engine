//! Viewport edits of a freeform 2D light: one drag is one undo step.

use crate::command::CommandArgs;
use crate::error::CoreError;
use crate::models::Past;
use crate::session::EditorSession;

/// `id` `op` (`point`) `index` `x` `y` `begin`. `x` and `y` are scene units.
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
    let at = [coord("x"), coord("y")];
    let begin = args.get("begin") == Some("1");
    let Some(entity) = session.world.entity(&id) else {
        return Err(CoreError::new("unknown entity"));
    };
    if entity.locked || entity.light2d.kind != "freeform" {
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
        let local = seal_world::light_fields::to_local(entity, at);
        match op {
            "point" => {
                if index >= entity.light2d.points.len() {
                    false
                } else {
                    entity.light2d.points[index] = local;
                    true
                }
            }
            _ => false,
        }
    };
    if !changed {
        session.past.pop();
        return Err(CoreError::new("could not edit the light"));
    }
    let next = session.world.to_text();
    if let Some(Past::Scene { next: slot, .. }) = session.past.last_mut() {
        *slot = next;
    }
    session.adopt_world();
    session.mark_scene();
    Ok(())
}
