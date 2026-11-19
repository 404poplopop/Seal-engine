//! Editor actions on filled shapes: join, cut, overlap, offset, and a collider
//! taken from the sprite's physics outline.

use crate::asset_editor::decode_png;
use crate::command::CommandArgs;
use crate::error::CoreError;
use crate::models::Past;
use crate::session::EditorSession;
use seal_world::frames;
use seal_world::shape2d::{local_point, world_points};
use seal_world::sheet::{collider_points, physics_outline, tight_outline};

type CommandFn = fn(&mut EditorSession, &CommandArgs) -> Result<(), CoreError>;

pub const COMMANDS: &[(&str, &str, &str, CommandFn)] = &[
    ("shape.merge", "command-shape-merge", "edit", merge),
    ("shape.subtract", "command-shape-subtract", "edit", subtract),
    ("shape.intersect", "command-shape-intersect", "edit", intersect),
    ("shape.offset", "command-shape-offset", "edit", offset_cmd),
    ("sprite.collider_from_sprite", "command-sprite-collider", "edit", collider_from_sprite),
];

const OFFSET_DRAFT: &str = "shape-offset";

fn editing(session: &EditorSession) -> Result<(), CoreError> {
    if session.world.mode != seal_world::PlayMode::Editing {
        Err(CoreError::new("stop play before editing the scene"))
    } else {
        Ok(())
    }
}

fn polygons(session: &EditorSession) -> Vec<String> {
    session
        .selected_ids()
        .into_iter()
        .filter(|id| {
            session.world.entity(id).is_some_and(|e| {
                e.shape2d.kind == "polygon" && e.shape2d.points.len() >= 3 && !e.locked
            })
        })
        .collect()
}

fn begin(session: &mut EditorSession) {
    let previous = session.world.to_text();
    let label = session.undo_step_label_now();
    session.push_past(Past::Scene { previous: previous.clone(), next: previous, label });
    session.future.clear();
}

fn commit(session: &mut EditorSession) {
    let next = session.world.to_text();
    if let Some(Past::Scene { next: slot, .. }) = session.past.last_mut() {
        *slot = next;
    }
    session.adopt_world();
    session.mark_scene();
}

fn refuse(session: &mut EditorSession, message: &str) -> Result<(), CoreError> {
    session.past.pop();
    Err(CoreError::new(message))
}

fn write_local(session: &mut EditorSession, id: &str, world: &[[f32; 2]]) -> bool {
    if world.len() < 3 {
        return false;
    }
    let Some(entity) = session.world.entity(id) else { return false };
    let local: Vec<[f32; 2]> = world.iter().copied().map(|p| local_point(entity, p)).collect();
    let Some(entity) = session.world.entity_mut(id) else { return false };
    entity.shape2d.kind = "polygon".into();
    entity.shape2d.points = local;
    entity.shape2d.closed = true;
    true
}

fn clear_shape(session: &mut EditorSession, id: &str) {
    if let Some(entity) = session.world.entity_mut(id) {
        entity.shape2d = seal_world::seal_scene::Shape2d::default();
    }
}

fn boolean(
    session: &mut EditorSession,
    op: fn(&[[f32; 2]], &[[f32; 2]]) -> Vec<[f32; 2]>,
    must_cover: bool,
) -> Result<(), CoreError> {
    editing(session)?;
    let ids = polygons(session);
    if ids.len() < 2 {
        return Err(CoreError::new("select two filled shapes"));
    }
    begin(session);
    let mut covered = Vec::new();
    let mut acc = {
        let Some(entity) = session.world.entity(&ids[0]) else {
            return refuse(session, "select two filled shapes");
        };
        let pts = world_points(entity);
        covered.extend(pts.iter().copied());
        pts
    };
    for id in ids.iter().skip(1) {
        let Some(entity) = session.world.entity(id) else { continue };
        let other = world_points(entity);
        covered.extend(other.iter().copied());
        acc = op(&acc, &other);
        if acc.len() < 3 {
            return refuse(session, "those shapes do not leave a filled shape");
        }
    }
    if must_cover && !seal_graph::poly2d::contains_points(&acc, &covered) {
        return refuse(session, "those shapes do not join into one outline");
    }
    if !write_local(session, &ids[0], &acc) {
        return refuse(session, "those shapes do not leave a filled shape");
    }
    for id in ids.iter().skip(1) {
        clear_shape(session, id);
    }
    commit(session);
    Ok(())
}

fn merge(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    boolean(session, seal_graph::poly2d::merge, true)
}

fn subtract(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    boolean(session, seal_graph::poly2d::clip, false)
}

fn intersect(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    boolean(session, seal_graph::poly2d::intersect, false)
}

fn offset_cmd(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let ids = polygons(session);
    if ids.is_empty() {
        return Err(CoreError::new("select a filled shape"));
    }
    let distance = args
        .get("distance")
        .and_then(|v| v.trim().parse::<f32>().ok())
        .or_else(|| session.draft(OFFSET_DRAFT).and_then(|v| v.trim().parse().ok()))
        .unwrap_or(0.1);
    if !distance.is_finite() {
        return Err(CoreError::new("offset distance must be a number"));
    }
    begin(session);
    for id in &ids {
        let world = {
            let Some(entity) = session.world.entity(id) else { continue };
            world_points(entity)
        };
        let grown = seal_graph::poly2d::offset(&world, distance);
        if !write_local(session, id, &grown) {
            return refuse(session, "that offset removes the shape");
        }
    }
    commit(session);
    Ok(())
}

fn collider_from_sprite(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let id = session.primary_selection().ok_or_else(|| CoreError::new("select a sprite"))?;
    let entity =
        session.world.entity(&id).ok_or_else(|| CoreError::new("select a sprite"))?.clone();
    if entity.locked {
        return Err(CoreError::new("the entity is locked"));
    }
    if entity.sprite.is_empty() {
        return Err(CoreError::new("the selected object has no sprite"));
    }
    let root = session
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .ok_or_else(|| CoreError::new("open a project first"))?;
    let (pivot, physics, outline, picture, uv) = sprite_outline(&entity, &root)?;
    let points = if physics.len() >= 3 {
        collider_points(pivot, &physics, &[])
    } else if let Some((rgba, w, h)) = decode_png(&picture) {
        let tight = if outline.len() >= 3 { outline } else { tight_outline(&rgba, w, h, uv) };
        let hull = physics_outline(&tight);
        collider_points(pivot, &hull, &[])
    } else {
        collider_points(pivot, &[], &outline)
    };
    if points.len() < 3 {
        return Err(CoreError::new("the sprite has no outline to collide with"));
    }
    begin(session);
    let Some(entity) = session.world.entity_mut(&id) else {
        return refuse(session, "select a sprite");
    };
    if entity.phys2d.body.is_empty() {
        entity.phys2d.body = "fixed".into();
    }
    entity.phys2d.shape = "polygon".into();
    entity.phys2d.points = points;
    commit(session);
    Ok(())
}

fn sprite_outline(
    entity: &seal_world::Entity,
    root: &std::path::Path,
) -> Result<([f32; 2], Vec<[f32; 2]>, Vec<[f32; 2]>, std::path::PathBuf, [f32; 4]), CoreError> {
    let raw = std::path::Path::new(&entity.sprite);
    let full = if raw.is_relative() { root.join(raw) } else { raw.to_path_buf() };
    if frames::is_frames_path(&entity.sprite) {
        let lib = frames::load_cached(&full)
            .ok_or_else(|| CoreError::new("the sprite animation could not be read"))?;
        let clip = lib
            .clip_or_first(&entity.animation)
            .ok_or_else(|| CoreError::new("the sprite animation has no clip"))?;
        let idx = match entity.frame_override {
            Some(f) => (f as usize).min(clip.frames.len().saturating_sub(1)),
            None => clip.frame_at(0.0),
        };
        let frame = clip
            .frames
            .get(idx)
            .ok_or_else(|| CoreError::new("the sprite animation has no frame"))?;
        let dir = full.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| root.to_path_buf());
        Ok((
            frame.pivot,
            frame.physics.clone(),
            frame.outline.clone(),
            dir.join(&frame.image),
            frame.uv,
        ))
    } else {
        Ok(([0.5, 0.5], Vec::new(), Vec::new(), full, [0.0, 0.0, 1.0, 1.0]))
    }
}

/// How many selected filled shapes there are, for menus.
pub fn selected_polygon_count(session: &EditorSession) -> usize {
    polygons(session).len()
}
