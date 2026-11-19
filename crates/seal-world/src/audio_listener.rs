//! Resolve the active audio listener while a scene is playing.

use crate::Entity;
use crate::World;

/// Where the listener sits in the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListenerPose {
    pub position: [f32; 3],
    pub forward: [f32; 3],
}

/// Distance falloff parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListenerTuning {
    pub max_distance: f32,
    pub rolloff: f32,
}

impl Default for ListenerTuning {
    fn default() -> Self {
        Self { max_distance: 50.0, rolloff: 1.0 }
    }
}

/// Full listener state passed to the sound output.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActiveListener {
    pub pose: ListenerPose,
    pub tuning: ListenerTuning,
}

impl Default for ActiveListener {
    fn default() -> Self {
        Self {
            pose: ListenerPose { position: [0.0; 3], forward: [0.0, 0.0, 1.0] },
            tuning: ListenerTuning { max_distance: 50.0, rolloff: 1.0 },
        }
    }
}

/// The listener used for positional sounds during play, if any.
pub fn active_listener(world: &World) -> Option<ActiveListener> {
    if world.mode == crate::PlayMode::Editing {
        return None;
    }
    if let Some(entity) = pick_listener_entity(world) {
        return Some(entity_listener(entity));
    }
    world.play_view_target().map(|position| ActiveListener {
        pose: ListenerPose { position, forward: forward_from_entity_at(world, position) },
        tuning: ListenerTuning::default(),
    })
}

fn pick_listener_entity(world: &World) -> Option<&Entity> {
    if let Some(cam) = world.camera2d_object().filter(|e| listener_enabled(e)) {
        return Some(cam);
    }
    if let Some(cam) =
        world.entities.iter().find(|e| e.tag == "camera" && !e.hidden && listener_enabled(e))
    {
        return Some(cam);
    }
    world.entities.iter().find(|e| !e.hidden && e.audio_listener.on)
}

fn listener_enabled(entity: &Entity) -> bool {
    entity.audio_listener.on || entity.tag == "camera" || entity.camera2d.on
}

fn entity_listener(entity: &Entity) -> ActiveListener {
    let yaw = entity.rotation[1].to_radians();
    let pitch = entity.rotation[0].to_radians();
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    // Same facing as the play camera the view looks through (yaw 0 = +z, pitch up positive).
    let forward = [sy * cp, sp, cy * cp];
    ActiveListener {
        pose: ListenerPose { position: entity.translation, forward },
        tuning: ListenerTuning {
            max_distance: entity.audio_listener.max_distance.max(0.1),
            rolloff: entity.audio_listener.rolloff.max(0.01),
        },
    }
}

fn forward_from_entity_at(world: &World, _position: [f32; 3]) -> [f32; 3] {
    if let Some(cam) =
        world.entities.iter().find(|e| (e.tag == "camera" || e.camera2d.on) && !e.hidden)
    {
        let yaw = cam.rotation[1].to_radians();
        let (sy, cy) = yaw.sin_cos();
        return [sy, 0.0, cy];
    }
    [0.0, 0.0, 1.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_2d_camera_is_the_default_listener() {
        let mut world = World::starter();
        world.mode = crate::PlayMode::Playing;
        for e in &mut world.entities {
            e.tag.clear();
        }
        let id = world.add("camera");
        if let Some(cam) = world.entity_mut(&id) {
            cam.camera2d.on = true;
            cam.translation = [1.0, 2.0, 3.0];
        }
        let listener = active_listener(&world).expect("listener");
        assert!((listener.pose.position[0] - 1.0).abs() < 0.01);
    }
}
