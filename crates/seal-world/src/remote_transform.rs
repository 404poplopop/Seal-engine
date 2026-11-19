//! Copies this object's position, rotation and scale onto another object each
//! play frame. Each channel has its own switch. An empty target does nothing.

use seal_ecs::text_component;

use crate::Entity;

/// Inspector fields, in order. Hidden until the component is on.
pub const KEYS: &[&str] =
    &["remote_on", "remote_target", "remote_position", "remote_rotation", "remote_scale"];

/// A helper that writes its own transform onto `target` during play.
#[derive(Clone, Debug, PartialEq)]
pub struct RemoteTransform {
    pub on: bool,
    /// Object id, or its name when no id matches. Empty copies nothing.
    pub target: String,
    pub position: bool,
    pub rotation: bool,
    pub scale: bool,
}

impl Default for RemoteTransform {
    fn default() -> Self {
        Self { on: false, target: String::new(), position: true, rotation: true, scale: true }
    }
}

text_component!(RemoteTransform as "remote_transform" { on, target, position, rotation, scale });

fn yes(on: bool) -> String {
    if on { "yes" } else { "no" }.into()
}

/// Inspector rows. Empty until the component is turned on.
pub fn fields(remote: &RemoteTransform) -> Vec<(String, String)> {
    if !remote.on {
        return Vec::new();
    }
    vec![
        ("remote_on".into(), yes(true)),
        ("remote_target".into(), remote.target.clone()),
        ("remote_position".into(), yes(remote.position)),
        ("remote_rotation".into(), yes(remote.rotation)),
        ("remote_scale".into(), yes(remote.scale)),
    ]
}

pub fn field_text(remote: &RemoteTransform, key: &str) -> Option<String> {
    Some(match key {
        "remote_on" => yes(remote.on),
        "remote_target" => remote.target.clone(),
        "remote_position" => yes(remote.position),
        "remote_rotation" => yes(remote.rotation),
        "remote_scale" => yes(remote.scale),
        _ => return None,
    })
}

/// Apply one inspector value. Unknown keys are left alone and return false.
pub fn apply(remote: &mut RemoteTransform, key: &str, value: &str) -> bool {
    match key {
        "remote_on" => remote.on = value == "yes",
        "remote_target" => remote.target = value.trim().to_string(),
        "remote_position" => remote.position = value != "no",
        "remote_rotation" => remote.rotation = value != "no",
        "remote_scale" => remote.scale = value != "no",
        _ => return false,
    }
    true
}

/// Write each helper onto its target. Reads every source first, so a chain
/// moves one step per frame and two helpers cannot see each other's write.
pub fn step(entities: &mut [Entity]) {
    type Plan = (usize, [f32; 3], [f32; 3], [f32; 3], bool, bool, bool);
    let planned: Vec<Plan> = entities
        .iter()
        .enumerate()
        .filter(|(_, entity)| entity.remote.on && !entity.remote.target.trim().is_empty())
        .filter_map(|(index, entity)| {
            let target = entity.remote.target.trim();
            let found = entities
                .iter()
                .position(|other| other.id == target)
                .or_else(|| entities.iter().position(|other| other.name == target))?;
            if found == index {
                return None;
            }
            let remote = &entity.remote;
            Some((
                found,
                entity.translation,
                entity.rotation,
                entity.scale,
                remote.position,
                remote.rotation,
                remote.scale,
            ))
        })
        .collect();
    for (index, translation, rotation, scale, position, rotate, scale_on) in planned {
        if position {
            entities[index].translation = translation;
        }
        if rotate {
            entities[index].rotation = rotation;
        }
        if scale_on {
            entities[index].scale = scale;
        }
    }
}
