//! 2D physics bodies on scene objects: their inspector text, and the play
//! simulation that moves them.
//!
//! Each setting is one inspector field with a text form, so the inspector,
//! the old scene format and copy-paste all share [`fields`] and [`apply`].
//! During play, objects with a body join the platformer's simulation when
//! there is a player, or a simulation of their own when there is not.

use seal_ecs::core::{SceneSettings, Transform};
use seal_ecs::Entity as Id;
use seal_physics2d::{Body2D, Collider2D, Joint2D, Physics2D, Velocity2D};
use seal_scene::Physics2d;

pub use seal_physics2d::{JOINTS, SHAPES};

use crate::{format_num, Entity, DEFAULT_GRAVITY};

/// Body kinds the inspector offers; `none` clears the body.
pub const BODIES: &[&str] = &["none", "dynamic", "fixed", "kinematic"];
/// Every 2D physics field, in inspector order.
pub const KEYS: &[&str] = &[
    "phys_body",
    "phys_part",
    "phys_shape",
    "phys_size",
    "phys_points",
    "phys_layer",
    "phys_mask",
    "phys_one_way",
    "phys_surface",
    "phys_ccd",
    "phys_density",
    "phys_mass",
    "phys_com",
    "phys_gravity_scale",
    "phys_linear_damping",
    "phys_angular_damping",
    "phys_can_sleep",
    "phys_lock_rotation",
    "phys_constant_force",
    "phys_constant_torque",
    "joint",
    "joint_to",
    "joint_anchor",
    "joint_other_anchor",
    "joint_axis",
    "joint_limits",
    "joint_length",
    "joint_stiffness",
    "joint_damping",
    "joint_motor",
    "joint_break_force",
    "joint_break_torque",
    "joint_max_force",
    "joint_max_torque",
    "joint_correction",
    "joint_offset",
    "joint_target",
    "phys_part",
];

fn yes(on: bool) -> String {
    if on { "yes" } else { "no" }.into()
}

pub fn pair_text(v: &[f32; 2]) -> String {
    format!("{}, {}", format_num(v[0]), format_num(v[1]))
}

fn numbers(text: &str) -> Option<Vec<f32>> {
    text.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<f32>().ok().filter(|v| v.is_finite()))
        .collect()
}

/// Two numbers separated by a comma or space.
pub fn parse_pair(text: &str) -> Option<[f32; 2]> {
    match numbers(text)?.as_slice() {
        [a, b] => Some([*a, *b]),
        _ => None,
    }
}

/// Outline points as `x y; x y; ...`.
pub fn points_text(points: &[[f32; 2]]) -> String {
    points
        .iter()
        .map(|p| format!("{} {}", format_num(p[0]), format_num(p[1])))
        .collect::<Vec<_>>()
        .join("; ")
}

pub fn parse_points(text: &str) -> Option<Vec<[f32; 2]>> {
    text.split(';').filter(|p| !p.trim().is_empty()).map(parse_pair).collect()
}

/// Layers as numbers 1 to 32 (`1, 3`), or `all` / `none`.
pub fn layers_text(bits: u32) -> String {
    match bits {
        u32::MAX => "all".into(),
        0 => "none".into(),
        _ => (0..32)
            .filter(|i| bits & (1 << i) != 0)
            .map(|i| (i + 1).to_string())
            .collect::<Vec<_>>()
            .join(", "),
    }
}

pub fn parse_layers(text: &str) -> Option<u32> {
    match text.trim() {
        "all" => return Some(u32::MAX),
        "none" | "" => return Some(0),
        _ => {}
    }
    let mut bits = 0u32;
    for part in text.split(|c: char| c == ',' || c.is_whitespace()).filter(|p| !p.is_empty()) {
        let n: u32 = part.parse().ok().filter(|n| (1..=32).contains(n))?;
        bits |= 1 << (n - 1);
    }
    Some(bits)
}

/// The inspector fields for an object's 2D physics: just the body kind when
/// it has none, the body settings when it has one, and the joint settings
/// when it has a joint.
pub fn fields(p: &Physics2d) -> Vec<(String, String)> {
    let mut out = vec![(
        "phys_body".to_string(),
        if p.body.is_empty() { "none".into() } else { p.body.clone() },
    )];
    if p.body.is_empty() {
        out.push(("phys_part".into(), yes(p.part)));
        if !p.part {
            return out;
        }
        let mut push = |k: &str, v: String| out.push((k.to_string(), v));
        push("phys_shape", p.shape.clone());
        push("phys_size", pair_text(&p.size));
        push("phys_points", points_text(&p.points));
        push("phys_layer", layers_text(p.layer));
        push("phys_mask", layers_text(p.mask));
        return out;
    }
    let mut push = |k: &str, v: String| out.push((k.to_string(), v));
    push("phys_shape", p.shape.clone());
    push("phys_size", pair_text(&p.size));
    push("phys_points", points_text(&p.points));
    push("phys_layer", layers_text(p.layer));
    push("phys_mask", layers_text(p.mask));
    push("phys_one_way", yes(p.one_way));
    push("phys_surface", if p.surface.is_empty() { "custom".into() } else { p.surface.clone() });
    push("phys_ccd", yes(p.ccd));
    push("phys_density", format_num(p.density));
    push("phys_mass", format_num(p.mass));
    push("phys_com", pair_text(&p.com));
    push("phys_gravity_scale", format_num(p.gravity_scale));
    push("phys_linear_damping", format_num(p.linear_damping));
    push("phys_angular_damping", format_num(p.angular_damping));
    push("phys_can_sleep", yes(p.can_sleep));
    push("phys_lock_rotation", yes(p.lock_rotation));
    push("phys_constant_force", pair_text(&p.constant_force));
    push("phys_constant_torque", format_num(p.constant_torque));
    push("joint", if p.joint.is_empty() { "none".into() } else { p.joint.clone() });
    if !p.joint.is_empty() {
        push("joint_to", p.joint_to.clone());
        push("joint_anchor", pair_text(&p.anchor));
        push("joint_other_anchor", pair_text(&p.other_anchor));
        push("joint_axis", pair_text(&p.axis));
        push("joint_limits", pair_text(&p.limits));
        push("joint_length", format_num(p.length));
        push("joint_stiffness", format_num(p.stiffness));
        push("joint_damping", format_num(p.damping));
        push("joint_motor", pair_text(&p.motor));
        push("joint_break_force", format_num(p.break_force));
        push("joint_break_torque", format_num(p.break_torque));
        push("joint_max_force", format_num(p.max_force));
        push("joint_max_torque", format_num(p.max_torque));
        push("joint_correction", format_num(p.correction));
        push("joint_offset", pair_text(&p.offset));
        push("joint_target", pair_text(&p.target));
    }
    out
}

/// The text form of one field, whether or not [`fields`] would list it.
pub fn field_text(p: &Physics2d, key: &str) -> Option<String> {
    let mut all = fields(&Physics2d { body: "dynamic".into(), joint: "hinge".into(), ..p.clone() });
    if key == "phys_part" {
        return Some(yes(p.part));
    }
    if key == "phys_body" {
        return Some(if p.body.is_empty() { "none".into() } else { p.body.clone() });
    }
    if key == "joint" {
        return Some(if p.joint.is_empty() { "none".into() } else { p.joint.clone() });
    }
    all.retain(|(k, _)| k == key);
    all.pop().map(|(_, v)| v)
}

/// Why a field's text is refused, or `None` when it is fine.
pub fn check(key: &str, value: &str) -> Option<&'static str> {
    let v = value.trim();
    let ok = match key {
        "phys_body" => BODIES.contains(&v),
        "phys_shape" => seal_physics2d::SHAPES.contains(&v),
        "phys_size" => parse_pair(v).is_some_and(|p| p[0] > 0.0 && p[1] > 0.0),
        "phys_points" => parse_points(v).is_some(),
        "phys_layer" | "phys_mask" => parse_layers(v).is_some(),
        "phys_one_way" | "phys_ccd" | "phys_part" | "phys_can_sleep" | "phys_lock_rotation" => {
            matches!(v, "yes" | "no")
        }
        "phys_density"
        | "phys_mass"
        | "phys_gravity_scale"
        | "phys_linear_damping"
        | "phys_angular_damping"
        | "phys_constant_torque" => v.parse::<f32>().is_ok_and(|n| n.is_finite() && n >= 0.0),
        "phys_com" | "phys_constant_force" => parse_pair(v).is_some(),
        "phys_surface" => v == "custom" || v.is_empty() || seal_physics2d::material(v).is_some(),
        "joint" => v == "none" || v.is_empty() || seal_physics2d::JOINTS.contains(&v),
        "joint_to" => true,
        "joint_anchor" | "joint_other_anchor" | "joint_motor" => parse_pair(v).is_some(),
        "joint_axis" => parse_pair(v).is_some_and(|p| p[0] != 0.0 || p[1] != 0.0),
        "joint_limits" => parse_pair(v).is_some_and(|p| p[0] <= p[1]),
        "joint_length" | "joint_stiffness" | "joint_damping" | "joint_break_force"
        | "joint_break_torque" | "joint_max_force" | "joint_max_torque" => {
            v.parse::<f32>().is_ok_and(|n| n.is_finite() && n >= 0.0)
        }
        "joint_correction" => {
            v.parse::<f32>().is_ok_and(|n| n.is_finite() && (0.0..=1.0).contains(&n))
        }
        "joint_offset" | "joint_target" => parse_pair(v).is_some(),
        _ => return Some("unknown"),
    };
    (!ok).then_some(match key {
        "phys_size" => "inspector-phys-size-invalid",
        "phys_points" => "inspector-phys-points-invalid",
        "phys_layer" | "phys_mask" => "inspector-phys-layers-invalid",
        "joint_limits" => "inspector-joint-limits-invalid",
        _ => "inspector-phys-invalid",
    })
}

/// Set one field from its text. Returns false (and changes nothing) when the
/// key is not a 2D physics field or the text is refused.
pub fn apply(p: &mut Physics2d, key: &str, value: &str) -> bool {
    if !KEYS.contains(&key) || check(key, value).is_some() {
        return false;
    }
    let v = value.trim();
    match key {
        "phys_body" => p.body = if v == "none" { String::new() } else { v.into() },
        "phys_part" => p.part = v == "yes",
        "phys_shape" => p.shape = v.into(),
        "phys_size" => p.size = parse_pair(v).expect("checked"),
        "phys_points" => p.points = parse_points(v).expect("checked"),
        "phys_layer" => p.layer = parse_layers(v).expect("checked"),
        "phys_mask" => p.mask = parse_layers(v).expect("checked"),
        "phys_one_way" => p.one_way = v == "yes",
        "phys_surface" => p.surface = if v == "custom" { String::new() } else { v.into() },
        "phys_ccd" => p.ccd = v == "yes",
        "phys_density" => p.density = v.parse().expect("checked"),
        "phys_mass" => p.mass = v.parse().expect("checked"),
        "phys_com" => p.com = parse_pair(v).expect("checked"),
        "phys_gravity_scale" => p.gravity_scale = v.parse().expect("checked"),
        "phys_linear_damping" => p.linear_damping = v.parse().expect("checked"),
        "phys_angular_damping" => p.angular_damping = v.parse().expect("checked"),
        "phys_can_sleep" => p.can_sleep = v == "yes",
        "phys_lock_rotation" => p.lock_rotation = v == "yes",
        "phys_constant_force" => p.constant_force = parse_pair(v).expect("checked"),
        "phys_constant_torque" => p.constant_torque = v.parse().expect("checked"),
        "joint" => p.joint = if v == "none" { String::new() } else { v.into() },
        "joint_to" => p.joint_to = v.into(),
        "joint_anchor" => p.anchor = parse_pair(v).expect("checked"),
        "joint_other_anchor" => p.other_anchor = parse_pair(v).expect("checked"),
        "joint_axis" => p.axis = parse_pair(v).expect("checked"),
        "joint_limits" => p.limits = parse_pair(v).expect("checked"),
        "joint_length" => p.length = v.parse().expect("checked"),
        "joint_stiffness" => p.stiffness = v.parse().expect("checked"),
        "joint_damping" => p.damping = v.parse().expect("checked"),
        "joint_motor" => p.motor = parse_pair(v).expect("checked"),
        "joint_break_force" => p.break_force = v.parse().expect("checked"),
        "joint_break_torque" => p.break_torque = v.parse().expect("checked"),
        "joint_max_force" => p.max_force = v.parse().expect("checked"),
        "joint_max_torque" => p.max_torque = v.parse().expect("checked"),
        "joint_correction" => p.correction = v.parse().expect("checked"),
        "joint_offset" => p.offset = parse_pair(v).expect("checked"),
        "joint_target" => p.target = parse_pair(v).expect("checked"),
        _ => return false,
    }
    true
}

/// The default text for a field, as a fresh body shows it.
pub fn settings_from_project(
    values: &std::collections::BTreeMap<String, String>,
) -> seal_physics2d::Physics2DSettings {
    seal_physics2d::Physics2DSettings::from_project_settings(values)
}

/// Approximate scene mass from density, size, scale and extra mass.
pub fn scene_mass(entity: &Entity) -> f32 {
    let p = &entity.phys2d;
    if p.body != "dynamic" {
        return 0.0;
    }
    let area = p.size[0] * p.size[1] * entity.scale[0].abs() * entity.scale[1].abs();
    p.density * area + p.mass
}

pub fn default_text(key: &str) -> Option<String> {
    field_text(&Physics2d::default(), key)
}

/// A child collider that rides on a parent body.
struct PartLink {
    scene: String,
    parent: String,
    ecs: Id,
    local: [f32; 2],
    spin: f32,
}

/// Objects with a 2D body, added to a play simulation.
pub struct Bodies {
    links: Vec<(String, Id, String)>,
    parts: Vec<PartLink>,
}

fn local_offset(parent: &Entity, child: &Entity) -> [f32; 3] {
    let (s, c) = parent.rotation[2].to_radians().sin_cos();
    let dx = child.translation[0] - parent.translation[0];
    let dy = child.translation[1] - parent.translation[1];
    [c * dx + s * dy, -s * dx + c * dy, child.rotation[2] - parent.rotation[2]]
}

fn transform(e: &Entity) -> Transform {
    Transform {
        translation: [e.translation[0], e.translation[1], 0.0],
        rotation: [0.0, 0.0, e.rotation[2]],
        scale: [e.scale[0], e.scale[1], 1.0],
    }
}

impl Bodies {
    /// Spawn every visible object with a 2D body into `ecs`.
    pub fn spawn(ecs: &mut seal_ecs::World, entities: &[Entity]) -> Self {
        let mut links = Vec::new();
        for e in entities.iter().filter(|e| !e.hidden && !e.phys2d.body.is_empty()) {
            let p = &e.phys2d;
            let body = Body2D {
                kind: p.body.clone(),
                ccd: p.ccd,
                gravity_scale: p.gravity_scale,
                linear_damping: p.linear_damping,
                angular_damping: p.angular_damping,
                can_sleep: p.can_sleep,
                lock_rotation: p.lock_rotation,
                constant_force: p.constant_force,
                constant_torque: p.constant_torque,
                additional_mass: p.mass,
            };
            let collider = Collider2D {
                shape: p.shape.clone(),
                size: p.size,
                density: p.density,
                center_of_mass: p.com,
                friction: e.friction,
                restitution: e.restitution,
                points: p.points.clone(),
                layer: p.layer,
                mask: p.mask,
                one_way: p.one_way,
                material: p.surface.clone(),
                sensor: !e.area.mode.is_empty() || !e.volume.kind.is_empty(),
                ..Default::default()
            };
            let id = ecs.spawn((transform(e), body, collider));
            if !e.area.mode.is_empty() {
                ecs.insert(id, e.area.clone());
            }
            if e.surface.speed != 0.0 {
                ecs.insert(id, e.surface.clone());
            }
            if p.body == "dynamic" {
                ecs.insert(id, Velocity2D { linear: [e.velocity[0], e.velocity[1]], angular: 0.0 });
            }
            links.push((e.id.clone(), id, p.body.clone()));
        }
        // Joints once every body exists, so `joint_to` can name any of them.
        for e in entities.iter().filter(|e| !e.hidden && !e.phys2d.joint.is_empty()) {
            let Some(&(_, id, _)) = links.iter().find(|l| l.0 == e.id) else { continue };
            let p = &e.phys2d;
            let to = p.joint_to.trim();
            let other = if to.is_empty() {
                None
            } else {
                let found = entities
                    .iter()
                    .find(|o| o.name == to)
                    .or_else(|| entities.iter().find(|o| o.id == to));
                match found.and_then(|o| links.iter().find(|l| l.0 == o.id)) {
                    Some(l) => Some(l.1),
                    // Holding on to something without a body: skip rather than pin to the world.
                    None => continue,
                }
            };
            ecs.insert(
                id,
                Joint2D {
                    kind: p.joint.clone(),
                    other,
                    anchor: p.anchor,
                    other_anchor: p.other_anchor,
                    axis: p.axis,
                    limits: p.limits,
                    length: p.length,
                    stiffness: p.stiffness,
                    damping: p.damping,
                    motor_speed: p.motor[0],
                    motor_force: p.motor[1],
                    collide: false,
                    break_force: p.break_force,
                    break_torque: p.break_torque,
                    max_force: p.max_force,
                    max_torque: p.max_torque,
                    correction: p.correction,
                    offset: p.offset,
                    target: if p.joint == "target" && p.target == [0.0; 2] {
                        p.other_anchor
                    } else {
                        p.target
                    },
                },
            );
        }
        let mut parts = Vec::new();
        for e in entities.iter().filter(|e| !e.hidden && e.phys2d.part && e.phys2d.body.is_empty())
        {
            let Some(parent_id) = e.parent.as_deref() else { continue };
            let Some(parent) =
                entities.iter().find(|p| p.id == parent_id && !p.phys2d.body.is_empty())
            else {
                continue;
            };
            let Some(&(_, parent_ecs, _)) = links.iter().find(|l| l.0 == parent_id) else {
                continue;
            };
            let local = local_offset(parent, e);
            let p = &e.phys2d;
            let collider = Collider2D {
                shape: p.shape.clone(),
                size: p.size,
                friction: e.friction,
                restitution: e.restitution,
                points: p.points.clone(),
                layer: p.layer,
                mask: p.mask,
                one_way: p.one_way,
                material: p.surface.clone(),
                sensor: !e.area.mode.is_empty(),
                ..Default::default()
            };
            let child = ecs.spawn((
                Transform {
                    translation: [local[0], local[1], 0.0],
                    rotation: [0.0, 0.0, local[2]],
                    scale: [e.scale[0], e.scale[1], 1.0],
                },
                collider,
            ));
            if ecs.set_parent(child, Some(parent_ecs)).is_err() {
                ecs.despawn(child);
                continue;
            }
            parts.push(PartLink {
                scene: e.id.clone(),
                parent: parent_id.to_string(),
                ecs: child,
                local: [local[0], local[1]],
                spin: local[2],
            });
        }
        Self { links, parts }
    }

    pub fn is_empty(&self) -> bool {
        self.links.is_empty()
    }

    /// Whether this scene object is simulated here.
    pub fn has(&self, scene_id: &str) -> bool {
        self.links.iter().any(|l| l.0 == scene_id)
    }

    /// Physics entity to scene id, for area events.
    pub fn scene_of(&self) -> std::collections::BTreeMap<Id, String> {
        self.links.iter().map(|(scene, id, _)| (*id, scene.clone())).collect()
    }

    /// Copy a script's collider shape, size and enabled flag onto the live body.
    /// The next physics step rebuilds a body whose shape changed.
    pub fn sync_colliders(&self, ecs: &mut seal_ecs::World, entities: &[Entity]) {
        let write = |ecs: &mut seal_ecs::World, id: Id, entity: &Entity| {
            let Some(current) = ecs.get::<Collider2D>(id) else { return };
            let sensor = !entity.collider_enabled
                || !entity.area.mode.is_empty()
                || !entity.volume.kind.is_empty();
            if current.shape == entity.phys2d.shape
                && current.size == entity.phys2d.size
                && current.points == entity.phys2d.points
                && current.sensor == sensor
                && current.layer == entity.phys2d.layer
                && current.mask == entity.phys2d.mask
            {
                return;
            }
            if let Some(collider) = ecs.get_mut::<Collider2D>(id) {
                collider.shape = entity.phys2d.shape.clone();
                collider.size = entity.phys2d.size;
                collider.points = entity.phys2d.points.clone();
                collider.sensor = sensor;
                collider.layer = entity.phys2d.layer;
                collider.mask = entity.phys2d.mask;
            }
        };
        for (scene_id, id, _) in &self.links {
            if let Some(entity) = entities.iter().find(|entity| &entity.id == scene_id) {
                write(ecs, *id, entity);
            }
        }
        for part in &self.parts {
            if let Some(entity) = entities.iter().find(|entity| entity.id == part.scene) {
                write(ecs, part.ecs, entity);
            }
        }
    }

    /// Drop bodies whose scene objects are gone, so a collected pickup
    /// stops blocking the player.
    pub fn forget_missing(&mut self, ecs: &mut seal_ecs::World, entities: &[Entity]) {
        let live: std::collections::BTreeSet<&str> =
            entities.iter().map(|e| e.id.as_str()).collect();
        let gone: Vec<Id> = self
            .links
            .iter()
            .filter(|link| !live.contains(link.0.as_str()))
            .map(|link| link.1)
            .collect();
        self.links.retain(|link| live.contains(link.0.as_str()));
        let gone_parts: Vec<Id> = self
            .parts
            .iter()
            .filter(|part| {
                !live.contains(part.scene.as_str()) || !live.contains(part.parent.as_str())
            })
            .map(|part| part.ecs)
            .collect();
        self.parts.retain(|part| {
            live.contains(part.scene.as_str()) && live.contains(part.parent.as_str())
        });
        for id in gone.into_iter().chain(gone_parts) {
            ecs.despawn(id);
        }
    }

    /// Copy gameplay's velocity and pose onto dynamic bodies before the step.
    /// Scripts write the entity; the solver only sees the body.
    pub fn pull_dynamic(&self, ecs: &mut seal_ecs::World, entities: &[Entity]) {
        for (scene_id, id, kind) in &self.links {
            if kind != "dynamic" {
                continue;
            }
            let Some(e) = entities.iter().find(|e| &e.id == scene_id) else { continue };
            if let Some(t) = ecs.get_mut::<Transform>(*id) {
                *t = transform(e);
            }
            if let Some(v) = ecs.get_mut::<Velocity2D>(*id) {
                v.linear = [e.velocity[0], e.velocity[1]];
            }
        }
    }

    /// How far each kinematic body wants to move this step: entity position minus
    /// the body, before [`Self::pull`] copies the entity onto the body.
    pub fn kinematic_deltas(
        &self,
        ecs: &seal_ecs::World,
        entities: &[Entity],
    ) -> Vec<(Id, [f32; 2])> {
        let mut out = Vec::new();
        for (scene_id, id, kind) in &self.links {
            if kind != "kinematic" {
                continue;
            }
            let (Some(e), Some(t)) =
                (entities.iter().find(|e| &e.id == scene_id), ecs.get::<Transform>(*id))
            else {
                continue;
            };
            out.push((
                *id,
                [e.translation[0] - t.translation[0], e.translation[1] - t.translation[1]],
            ));
        }
        out
    }

    /// Move kinematic bodies to where gameplay put their objects.
    pub fn pull(&self, ecs: &mut seal_ecs::World, entities: &[Entity]) {
        for (scene_id, id, kind) in &self.links {
            if kind != "kinematic" {
                continue;
            }
            if let (Some(e), Some(t)) =
                (entities.iter().find(|e| &e.id == scene_id), ecs.get_mut::<Transform>(*id))
            {
                *t = transform(e);
            }
        }
    }

    /// Copy dynamic bodies back into their objects.
    pub fn push(&self, ecs: &seal_ecs::World, entities: &mut [Entity]) {
        for (scene_id, id, kind) in &self.links {
            if kind != "dynamic" {
                continue;
            }
            let Some(t) = ecs.get::<Transform>(*id) else { continue };
            if let Some(e) = entities.iter_mut().find(|e| &e.id == scene_id) {
                e.translation[0] = t.translation[0];
                e.translation[1] = t.translation[1];
                e.rotation[2] = t.rotation[2];
                if let Some(v) = ecs.get::<Velocity2D>(*id) {
                    e.velocity = [v.linear[0], v.linear[1], 0.0];
                }
            }
        }
        self.place_parts(entities);
    }

    fn place_parts(&self, entities: &mut [Entity]) {
        let poses: Vec<(String, [f32; 3], f32)> = self
            .parts
            .iter()
            .filter_map(|part| {
                let parent = entities.iter().find(|e| e.id == part.parent)?;
                Some((part.scene.clone(), parent.translation, parent.rotation[2]))
            })
            .collect();
        for (scene, at, rot) in poses {
            let Some(part) = self.parts.iter().find(|p| p.scene == scene) else { continue };
            let (s, c) = rot.to_radians().sin_cos();
            if let Some(entity) = entities.iter_mut().find(|e| e.id == scene) {
                entity.translation[0] = at[0] + part.local[0] * c - part.local[1] * s;
                entity.translation[1] = at[1] + part.local[0] * s + part.local[1] * c;
                entity.rotation[2] = rot + part.spin;
            }
        }
    }
}

pub fn joint_break_scene_ids(physics: &Physics2D, bodies: &Bodies) -> Vec<String> {
    physics
        .joint_breaks
        .iter()
        .filter_map(|b| bodies.links.iter().find(|l| l.1 == b.entity).map(|l| l.0.clone()))
        .collect()
}

pub fn clear_joint_on_entity(entities: &mut [Entity], scene_id: &str) {
    let Some(e) = entities.iter_mut().find(|e| e.id == scene_id) else { return };
    let d = seal_scene::Physics2d::default();
    e.phys2d.joint.clear();
    e.phys2d.joint_to = d.joint_to;
    e.phys2d.anchor = d.anchor;
    e.phys2d.other_anchor = d.other_anchor;
    e.phys2d.axis = d.axis;
    e.phys2d.limits = d.limits;
    e.phys2d.length = d.length;
    e.phys2d.stiffness = d.stiffness;
    e.phys2d.damping = d.damping;
    e.phys2d.motor = d.motor;
    e.phys2d.break_force = d.break_force;
    e.phys2d.break_torque = d.break_torque;
    e.phys2d.max_force = d.max_force;
    e.phys2d.max_torque = d.max_torque;
    e.phys2d.correction = d.correction;
    e.phys2d.offset = d.offset;
    e.phys2d.target = d.target;
}

/// Play for a scene with 2D bodies but no platformer player.
pub struct Sim2d {
    ecs: seal_ecs::World,
    physics: Physics2D,
    bodies: Bodies,
    carry: f32,
    step: f32,
    max_substeps: u32,
    /// The last simulation error, shown in the play log once.
    pub error: Option<String>,
}

impl Sim2d {
    pub fn from_entities(
        entities: &[Entity],
        gravity: f32,
        settings: seal_physics2d::Physics2DSettings,
    ) -> Option<Self> {
        let mut ecs = seal_ecs::World::new();
        ecs.register_core();
        seal_physics2d::register_physics2d(&mut ecs);
        ecs.insert_resource(SceneSettings {
            gravity: gravity.max(0.0) * 9.81 / DEFAULT_GRAVITY,
            time_scale: 1.0,
            music: String::new(),
        });
        let bodies = Bodies::spawn(&mut ecs, entities);
        if bodies.is_empty() {
            return None;
        }
        let step = settings.fixed_step();
        let max_substeps = settings.max_substeps;
        let mut physics = Physics2D::new();
        physics.settings = settings;
        Some(Self { ecs, physics, bodies, carry: 0.0, step, max_substeps, error: None })
    }

    pub fn profiler(&self) -> &seal_physics2d::Physics2DProfiler {
        &self.physics.profiler
    }

    pub fn ecs_entity(&self, scene_id: &str) -> Option<Id> {
        self.bodies.links.iter().find(|l| l.0 == scene_id).map(|l| l.1)
    }

    pub fn physics_mut(&mut self) -> &mut Physics2D {
        &mut self.physics
    }

    pub fn apply_pending(&mut self, pending: &crate::scripts::Phys2dPending) {
        for (scene_id, force) in &pending.force {
            let Some(ecs_id) = self.ecs_entity(scene_id) else { continue };
            self.physics.apply_force(ecs_id, *force);
        }
        for (scene_id, torque) in &pending.torque {
            let Some(ecs_id) = self.ecs_entity(scene_id) else { continue };
            self.physics.apply_torque(ecs_id, *torque);
        }
        for (scene_id, linear, angular) in &pending.velocity {
            let Some(ecs_id) = self.ecs_entity(scene_id) else { continue };
            self.physics.set_velocity(ecs_id, *linear, *angular);
        }
        for (scene_id, target) in &pending.joint_target {
            let Some(ecs_id) = self.ecs_entity(scene_id) else { continue };
            if self.physics.set_joint_target(ecs_id, *target) {
                if let Some(j) = self.ecs.get_mut::<Joint2D>(ecs_id) {
                    j.target = *target;
                }
            }
        }
    }

    /// Scene ids of joints that broke on the last step.
    pub fn joint_break_scene_ids(&self) -> Vec<String> {
        joint_break_scene_ids(&self.physics, &self.bodies)
    }

    /// Drop broken joints from scene objects so the inspector matches play.
    pub fn clear_broken_joints(&mut self, entities: &mut [Entity]) {
        for scene_id in self.joint_break_scene_ids() {
            clear_joint_on_entity(entities, &scene_id);
        }
    }

    pub fn step(&mut self, dt: f32, entities: &mut [Entity]) {
        self.bodies.sync_colliders(&mut self.ecs, entities);
        self.bodies.forget_missing(&mut self.ecs, entities);
        self.bodies.pull_dynamic(&mut self.ecs, entities);
        let cap = self.step * self.max_substeps as f32;
        self.carry = (self.carry + dt.max(0.0)).min(cap);
        while self.carry >= self.step {
            self.carry -= self.step;
            self.bodies.pull(&mut self.ecs, entities);
            if let Err(err) = self.physics.step(&mut self.ecs, self.step) {
                self.error = Some(err.to_string());
                return;
            }
        }
        if self.physics.settings.render_interpolation {
            let alpha = if self.step > 0.0 { self.carry / self.step } else { 0.0 };
            self.physics.write_transforms(&mut self.ecs, alpha);
        }
        self.bodies.push(&self.ecs, entities);
    }

    /// Who entered, left, and remains inside each area this step.
    pub fn area_report(&self) -> crate::area_effect::Report {
        crate::area_effect::report(&self.physics, &self.bodies.scene_of())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_joint_fields_round_trip_in_scene_text() {
        let mut w = crate::World::from_text(&crate::World::empty_scene_text("Joint")).unwrap();
        let id = w.add("empty");
        let e = w.entity_mut(&id).unwrap();
        for (k, v) in [
            ("phys_body", "dynamic"),
            ("joint", "target"),
            ("joint_target", "3, 4"),
            ("joint_max_force", "80"),
            ("joint_break_force", "100"),
        ] {
            assert!(apply(&mut e.phys2d, k, v), "{k}");
        }
        let w2 = crate::World::from_text(&w.to_text()).unwrap();
        let got = w2.entity(&id).unwrap().phys2d.clone();
        assert_eq!(got.joint, "target");
        assert_eq!(got.target, [3.0, 4.0]);
        assert_eq!(got.max_force, 80.0);
        assert_eq!(got.break_force, 100.0);
    }

    #[test]
    fn older_scene_without_joint_extras_parses_same() {
        let mut w = crate::World::from_text(&crate::World::empty_scene_text("Old")).unwrap();
        let id = w.add("empty");
        let e = w.entity_mut(&id).unwrap();
        assert!(apply(&mut e.phys2d, "phys_body", "dynamic"));
        assert!(apply(&mut e.phys2d, "joint", "hinge"));
        assert!(apply(&mut e.phys2d, "joint_anchor", "0, 0"));
        let before = e.phys2d.clone();
        let w2 = crate::World::from_text(&w.to_text()).unwrap();
        assert_eq!(w2.entity(&id).unwrap().phys2d, before);
    }

    #[test]
    fn older_scene_format_keeps_2d_physics() {
        let mut e = crate::World::demo().entities[2].clone();
        for (k, v) in [
            ("phys_body", "kinematic"),
            ("phys_shape", "edge"),
            ("phys_points", "0 0; 2 1; 4 0"),
            ("phys_mask", "none"),
            ("joint", "slider"),
            ("joint_axis", "0, 1"),
            ("joint_to", "Floor"),
        ] {
            assert!(apply(&mut e.phys2d, k, v), "{k}");
        }
        let back = crate::entity_from(&crate::entity_value(&e)).unwrap();
        assert_eq!(back.phys2d, e.phys2d);
        assert_eq!(layers_text(0b1001), "1, 4");
        assert_eq!(parse_layers("all"), Some(u32::MAX));
        assert_eq!(parse_points("1 2;"), Some(vec![[1.0, 2.0]]));
        assert!(!apply(&mut e.phys2d, "joint_damping", "-1"));
        assert!(!apply(&mut e.phys2d, "colour", "red"));
    }

    fn box_body(w: &mut crate::World, kind: &str, at: [f32; 3]) -> String {
        let id = w.add("empty");
        let e = w.entity_mut(&id).unwrap();
        e.translation = at;
        assert!(apply(&mut e.phys2d, "phys_body", kind));
        assert!(apply(&mut e.phys2d, "phys_shape", "box"));
        assert!(apply(&mut e.phys2d, "phys_size", "1, 1"));
        id
    }

    #[test]
    fn gameplay_velocity_moves_a_dynamic_body() {
        let mut w = crate::World::from_text(&crate::World::empty_scene_text("Move")).unwrap();
        w.gravity = 0.0;
        let id = box_body(&mut w, "dynamic", [0.0, 0.0, 0.0]);
        w.play();
        w.entity_mut(&id).unwrap().velocity = [2.0, 0.0, 0.0];
        for _ in 0..30 {
            w.step(1.0 / 60.0);
        }
        let x = w.entity(&id).unwrap().translation[0];
        assert!(x > 0.8, "velocity carried the body, x={x}");
    }

    #[test]
    fn gameplay_can_place_a_dynamic_body() {
        let mut w = crate::World::from_text(&crate::World::empty_scene_text("Place")).unwrap();
        w.gravity = 0.0;
        let id = box_body(&mut w, "dynamic", [0.0, 0.0, 0.0]);
        w.play();
        w.step(1.0 / 60.0);
        w.entity_mut(&id).unwrap().translation = [4.0, 1.0, 0.0];
        w.step(1.0 / 60.0);
        let at = w.entity(&id).unwrap().translation;
        assert!((at[0] - 4.0).abs() < 0.05, "placed body stays, at={at:?}");
    }

    #[test]
    fn a_removed_body_stops_blocking() {
        let mut w = crate::World::from_text(&crate::World::empty_scene_text("Gone")).unwrap();
        w.gravity = 0.0;
        let wall = box_body(&mut w, "fixed", [1.2, 0.0, 0.0]);
        let id = box_body(&mut w, "dynamic", [0.0, 0.0, 0.0]);
        w.play();
        for _ in 0..20 {
            w.entity_mut(&id).unwrap().velocity = [3.0, 0.0, 0.0];
            w.step(1.0 / 60.0);
        }
        let blocked = w.entity(&id).unwrap().translation[0];
        assert!(blocked < 1.0, "the wall held it, x={blocked}");
        w.remove(&wall);
        for _ in 0..40 {
            w.entity_mut(&id).unwrap().velocity = [3.0, 0.0, 0.0];
            w.step(1.0 / 60.0);
        }
        let x = w.entity(&id).unwrap().translation[0];
        assert!(x > 1.5, "after the wall is gone the body passes, x={x}");
    }

    #[test]
    fn a_child_collider_follows_its_parent() {
        let mut w = crate::World::from_text(&crate::World::empty_scene_text("Parts")).unwrap();
        w.gravity = 0.0;
        let parent = box_body(&mut w, "dynamic", [0.0, 0.0, 0.0]);
        let child = w.add("empty");
        {
            let e = w.entity_mut(&child).unwrap();
            e.parent = Some(parent.clone());
            e.translation = [3.0, 0.0, 0.0];
            assert!(apply(&mut e.phys2d, "phys_part", "yes"));
            assert!(apply(&mut e.phys2d, "phys_shape", "box"));
            assert!(apply(&mut e.phys2d, "phys_size", "1, 1"));
        }
        w.play();
        for _ in 0..30 {
            w.entity_mut(&parent).unwrap().velocity = [2.0, 0.0, 0.0];
            w.step(1.0 / 60.0);
        }
        let px = w.entity(&parent).unwrap().translation[0];
        let cx = w.entity(&child).unwrap().translation[0];
        assert!(px > 0.8, "the parent moved, x={px}");
        assert!(
            (cx - px - 3.0).abs() < 0.05,
            "the child stayed on the parent, parent={px} child={cx}"
        );
    }
}
