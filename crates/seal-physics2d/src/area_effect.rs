//! Area effects and moving surfaces for 2D bodies.
//!
//! An [`AreaEffect`] sits on the same object as a trigger collider. Each step,
//! every dynamic body whose shape overlaps that trigger and whose layer is in
//! the area's mask receives the area's effect. Effects from several areas add.
//! A gravity override uses the area with the highest priority (the lower entity
//! id wins a tie). A [`SurfaceEffect`] is a moving walkway: bodies and the
//! platformer character standing on that collider are carried along it.
//!
//! The same inputs and step count produce the same motion on every machine.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rapier2d::prelude::*;
use seal_ecs::{text_component, Entity, World};

/// `directional`, `point`, `buoyancy`, `gravity` or `drag`. Empty is off.
pub const MODES: &[&str] = &["directional", "point", "buoyancy", "gravity", "drag"];
/// How a point effect weakens with distance.
pub const FALLOFFS: &[&str] = &["none", "linear", "inverse"];
/// Directional push: a force (scaled by mass) or an acceleration.
pub const FORCE_KINDS: &[&str] = &["force", "acceleration"];
/// Point effect: pull toward the area centre, or push away.
pub const POINT_SIGNS: &[&str] = &["attract", "repel"];
/// Gravity inside the area: replace the scene gravity, or multiply it.
pub const GRAVITY_MODES: &[&str] = &["replace", "scale"];

/// A region that pushes, pulls, floats or damps dynamic bodies inside it.
#[derive(Clone, Debug, PartialEq)]
pub struct AreaEffect {
    /// One of [`MODES`], or empty when this object is not an area.
    pub mode: String,
    /// Layers of bodies this area affects (bit 0 is layer 1).
    pub mask: u32,
    /// Gravity override keeps the highest value. Other modes ignore it and add.
    pub priority: i32,
    /// `force` or `acceleration` for `directional`.
    pub force_kind: String,
    /// Direction of a `directional` effect, in world units. Length is ignored.
    pub direction: [f32; 2],
    /// How hard a `directional` or `point` effect pushes.
    pub strength: f32,
    /// Fraction of `strength` that varies, from 0 to 1. The same body and step
    /// always get the same value.
    pub variation: f32,
    /// `attract` or `repel` for `point`.
    pub point_sign: String,
    /// `none`, `linear` or `inverse` for `point`.
    pub falloff: String,
    /// Distance past which a `point` effect does nothing. 0 means no limit
    /// except the trigger shape.
    pub range: f32,
    /// Fluid surface, in world units above the area's position.
    pub surface: f32,
    /// Fluid density for `buoyancy`. A body floats when its density is lower.
    pub density: f32,
    /// Linear drag inside the fluid, scaled by how much of the body is under.
    pub linear_drag: f32,
    /// Angular drag inside the fluid, in the same way.
    pub angular_drag: f32,
    /// Fluid velocity. Drag pulls submerged bodies toward it.
    pub flow: [f32; 2],
    /// `replace` or `scale` for `gravity`.
    pub gravity_mode: String,
    /// Gravity vector used when `gravity_mode` is `replace` (units per second squared).
    pub gravity: [f32; 2],
    /// Multiplier used when `gravity_mode` is `scale`. 0 is zero gravity.
    pub gravity_scale: f32,
    /// Extra linear damping for `drag`, added to the body's own.
    pub linear_damping: f32,
    /// Extra angular damping for `drag`, added to the body's own.
    pub angular_damping: f32,
}

impl Default for AreaEffect {
    fn default() -> Self {
        Self {
            mode: String::new(),
            mask: u32::MAX,
            priority: 0,
            force_kind: "acceleration".into(),
            direction: [1.0, 0.0],
            strength: 10.0,
            variation: 0.0,
            point_sign: "attract".into(),
            falloff: "none".into(),
            range: 0.0,
            surface: 0.0,
            density: 1.0,
            linear_drag: 2.0,
            angular_drag: 0.5,
            flow: [0.0, 0.0],
            gravity_mode: "replace".into(),
            gravity: [0.0, 0.0],
            gravity_scale: 0.0,
            linear_damping: 1.0,
            angular_damping: 0.5,
        }
    }
}

text_component!(AreaEffect as "area_effect" {
    mode, mask, priority, force_kind, direction, strength, variation, point_sign, falloff, range,
    surface, density, linear_drag, angular_drag, flow, gravity_mode, gravity, gravity_scale,
    linear_damping, angular_damping
});

/// Speed along a solid collider, in its local +X after the object's turn.
/// Bodies standing on it move with that speed.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceEffect {
    pub speed: f32,
}

impl Default for SurfaceEffect {
    fn default() -> Self {
        Self { speed: 0.0 }
    }
}

text_component!(SurfaceEffect as "surface_effect" { speed });

pub fn register(world: &mut World) {
    world.register_text::<AreaEffect>();
    world.register_text::<SurfaceEffect>();
}

/// One simulated body, for applying effects.
pub struct Linked {
    pub entity: Entity,
    pub body: RigidBodyHandle,
    pub collider: ColliderHandle,
    pub kind: String,
    pub layer: u32,
    pub last_linear: [f32; 2],
}

/// Bodies inside each area after the last step, and who entered or left.
#[derive(Clone, Debug, Default)]
pub struct Memory {
    pub inside: BTreeMap<Entity, BTreeSet<Entity>>,
    pub entered: Vec<(Entity, Entity)>,
    pub exited: Vec<(Entity, Entity)>,
    step: u64,
    belt: BTreeMap<Entity, Vector>,
}

/// Damping to put back after the step, so an area does not rewrite the body.
pub struct DampingHold {
    pub body: RigidBodyHandle,
    pub linear: f32,
    pub angular: f32,
}

/// Scale on a directional or point strength for this body and step.
/// `variation` 0 keeps `1`. Otherwise the scale sits in `1 - variation` .. `1 + variation`.
pub fn strength_scale(step: u64, entity: Entity, variation: f32) -> f32 {
    let v = variation.clamp(0.0, 1.0);
    if v == 0.0 {
        return 1.0;
    }
    let mut x = entity.to_bits().wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ step.wrapping_mul(0x6C62_272E_07BB_0142);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    let unit = (x >> 40) as f32 / (1u64 << 24) as f32;
    1.0 + v * (unit * 2.0 - 1.0)
}

/// `1` at no distance, `0` at `range`, or `1 / distance²`, and `0` past `range`.
pub fn falloff_at(kind: &str, distance: f32, range: f32) -> f32 {
    if range > 0.0 && distance > range {
        return 0.0;
    }
    match kind {
        "linear" => {
            if range <= 1e-6 {
                0.0
            } else {
                (1.0 - distance / range).max(0.0)
            }
        }
        "inverse" => {
            let d = distance.max(0.05);
            1.0 / (d * d)
        }
        _ => 1.0,
    }
}

#[derive(Clone, Copy)]
struct Span {
    min: [f32; 2],
    max: [f32; 2],
    up: [f32; 2],
}

fn span_of(sim: &PhysicsWorld, collider: ColliderHandle) -> Option<Span> {
    let col = sim.colliders.get(collider)?;
    let aabb = col.compute_aabb();
    let up = col.position().rotation * Vector::new(0.0, 1.0);
    Some(Span {
        min: [aabb.mins.x, aabb.mins.y],
        max: [aabb.maxs.x, aabb.maxs.y],
        up: [up.x, up.y],
    })
}

fn spans_hit(a: &Span, b: &Span) -> bool {
    a.min[0] <= b.max[0] && a.max[0] >= b.min[0] && a.min[1] <= b.max[1] && a.max[1] >= b.min[1]
}

fn spans_stand(body: &Span, surface: &Span) -> bool {
    let x_overlap = body.min[0] <= surface.max[0] && body.max[0] >= surface.min[0];
    let gap = body.min[1] - surface.max[1];
    let bc = [(body.min[0] + body.max[0]) * 0.5, (body.min[1] + body.max[1]) * 0.5];
    let sc = [(surface.min[0] + surface.max[0]) * 0.5, (surface.min[1] + surface.max[1]) * 0.5];
    let dot = (bc[0] - sc[0]) * surface.up[0] + (bc[1] - sc[1]) * surface.up[1];
    x_overlap && dot > 0.0 && (-0.5..0.3).contains(&gap)
}

/// Push areas, carry surfaces and record who is inside. Call after bodies are
/// synced and gravity is set, and before the solver step. Put `holds` back
/// with [`restore_damping`] after the step.
pub fn before_step(
    sim: &mut PhysicsWorld,
    world: &mut World,
    links: &[Linked],
    surface_hooks: &mut HashMap<ColliderHandle, Vector>,
    memory: &mut Memory,
) -> Vec<DampingHold> {
    memory.step = memory.step.wrapping_add(1);
    surface_hooks.clear();
    let scene_gravity = sim.gravity;
    let areas: Vec<(Entity, AreaEffect)> = world
        .query::<(Entity, &AreaEffect)>()
        .collect(|(e, a)| (e, a.clone()))
        .into_iter()
        .filter(|(_, a)| MODES.contains(&a.mode.as_str()))
        .collect();
    let surfaces: Vec<(Entity, f32)> = world
        .query::<(Entity, &SurfaceEffect)>()
        .collect(|(e, s)| (e, s.speed))
        .into_iter()
        .filter(|(_, speed)| *speed != 0.0)
        .collect();
    let mut spans = BTreeMap::new();
    for link in links {
        if let Some(span) = span_of(sim, link.collider) {
            spans.insert(link.entity, span);
        }
    }

    // User forces stay until cleared, so each step starts from the effects below.
    for link in links.iter().filter(|l| l.kind == "dynamic") {
        if let Some(rb) = sim.bodies.get_mut(link.body) {
            rb.reset_forces(false);
            rb.reset_torques(false);
        }
    }

    let mut now: BTreeMap<Entity, BTreeSet<Entity>> = BTreeMap::new();
    let mut force: BTreeMap<Entity, Vector> = BTreeMap::new();
    let mut damp: BTreeMap<Entity, [f32; 2]> = BTreeMap::new();
    // priority, entity id, replace vector or scale.
    let mut gravity: BTreeMap<Entity, (i32, u64, Option<Vector>, Option<f32>)> = BTreeMap::new();
    let mut torque: BTreeMap<Entity, f32> = BTreeMap::new();

    for (area_entity, effect) in &areas {
        let Some(area_link) = links.iter().find(|l| l.entity == *area_entity) else { continue };
        let Some(area_span) = spans.get(area_entity).copied() else { continue };
        let area_pos = sim.bodies[area_link.body].translation();
        let mut inside = BTreeSet::new();
        for link in links.iter().filter(|l| l.kind == "dynamic" && l.entity != *area_entity) {
            if link.layer & effect.mask == 0 {
                continue;
            }
            let Some(span) = spans.get(&link.entity) else { continue };
            if !spans_hit(&area_span, span) {
                continue;
            }
            inside.insert(link.entity);
            match effect.mode.as_str() {
                "directional" => {
                    let dir = Vector::new(effect.direction[0], effect.direction[1]);
                    if dir.length() <= 1e-6 {
                        continue;
                    }
                    let dir = dir.normalize();
                    let scale = strength_scale(memory.step, link.entity, effect.variation);
                    let mag = effect.strength * scale;
                    let rb = &sim.bodies[link.body];
                    let push = if effect.force_kind == "force" {
                        dir * mag
                    } else {
                        dir * (mag * rb.mass())
                    };
                    *force.entry(link.entity).or_insert(Vector::ZERO) += push;
                }
                "point" => {
                    let at = sim.bodies[link.body].translation();
                    let delta = area_pos - at;
                    let distance = delta.length();
                    if distance <= 1e-4 {
                        continue;
                    }
                    let amount = falloff_at(&effect.falloff, distance, effect.range);
                    if amount == 0.0 {
                        continue;
                    }
                    let sign = if effect.point_sign == "repel" { -1.0 } else { 1.0 };
                    let scale = strength_scale(memory.step, link.entity, effect.variation);
                    let push = delta.normalize() * (effect.strength * amount * scale * sign);
                    *force.entry(link.entity).or_insert(Vector::ZERO) += push;
                }
                "buoyancy" => {
                    let height = (span.max[1] - span.min[1]).max(1e-4);
                    let width = (span.max[0] - span.min[0]).max(1e-4);
                    let surface_y = area_pos.y + effect.surface;
                    let frac = ((surface_y - span.min[1]) / height).clamp(0.0, 1.0);
                    if frac <= 0.0 {
                        continue;
                    }
                    let (mass, linvel, angvel) = {
                        let rb = &sim.bodies[link.body];
                        (rb.mass().max(1e-6), rb.linvel(), rb.angvel())
                    };
                    let volume = width * height;
                    // Displaced fluid pushes opposite scene gravity.
                    let buoy = scene_gravity * -(effect.density * volume * frac);
                    let flow = Vector::new(effect.flow[0], effect.flow[1]);
                    let drag = (flow - linvel) * (effect.linear_drag * frac * mass);
                    *force.entry(link.entity).or_insert(Vector::ZERO) += buoy + drag;
                    *torque.entry(link.entity).or_insert(0.0) +=
                        -angvel * effect.angular_drag * frac;
                }
                "gravity" => {
                    let bits = link.entity.to_bits();
                    let (vec, scale) = if effect.gravity_mode == "scale" {
                        (None, Some(effect.gravity_scale))
                    } else {
                        (Some(Vector::new(effect.gravity[0], effect.gravity[1])), None)
                    };
                    let slot =
                        gravity.entry(link.entity).or_insert((i32::MIN, u64::MAX, None, None));
                    if effect.priority > slot.0 || (effect.priority == slot.0 && bits < slot.1) {
                        *slot = (effect.priority, bits, vec, scale);
                    }
                }
                "drag" => {
                    let slot = damp.entry(link.entity).or_insert([0.0, 0.0]);
                    slot[0] += effect.linear_damping.max(0.0);
                    slot[1] += effect.angular_damping.max(0.0);
                }
                _ => {}
            }
        }
        now.insert(*area_entity, inside);
    }

    for (entity, push) in force {
        if let Some(link) = links.iter().find(|l| l.entity == entity) {
            sim.bodies[link.body].add_force(push, true);
        }
    }
    for (entity, spin) in torque {
        if let Some(link) = links.iter().find(|l| l.entity == entity) {
            sim.bodies[link.body].add_torque(spin, true);
        }
    }
    for (entity, (_priority, _, replace, scale)) in gravity {
        let Some(link) = links.iter().find(|l| l.entity == entity) else { continue };
        let mass = sim.bodies[link.body].mass().max(1e-6);
        let body_scale = sim.bodies[link.body].gravity_scale();
        let extra = if let Some(g) = replace {
            scene_gravity * (-body_scale * mass) + g * mass
        } else {
            let s = scale.unwrap_or(1.0);
            scene_gravity * (body_scale * mass * (s - 1.0))
        };
        sim.bodies[link.body].add_force(extra, true);
    }

    let mut holds = Vec::new();
    for (entity, extra) in damp {
        let Some(link) = links.iter().find(|l| l.entity == entity) else { continue };
        let rb = &mut sim.bodies[link.body];
        holds.push(DampingHold {
            body: link.body,
            linear: rb.linear_damping(),
            angular: rb.angular_damping(),
        });
        rb.set_linear_damping(rb.linear_damping() + extra[0]);
        rb.set_angular_damping(rb.angular_damping() + extra[1]);
    }

    let mut carried: BTreeMap<Entity, Vector> = BTreeMap::new();
    for (surf_entity, speed) in &surfaces {
        let Some(link) = links.iter().find(|l| l.entity == *surf_entity) else { continue };
        let Some(rb) = sim.bodies.get(link.body) else { continue };
        let vel = rb.position().rotation * Vector::new(*speed, 0.0);
        surface_hooks.insert(link.collider, vel);
        if let Some(col) = sim.colliders.get_mut(link.collider) {
            col.set_active_hooks(col.active_hooks() | ActiveHooks::MODIFY_SOLVER_CONTACTS);
        }
        let Some(surf_span) = spans.get(surf_entity).copied() else { continue };
        for body in links.iter().filter(|l| l.kind == "dynamic" && l.entity != *surf_entity) {
            let Some(span) = spans.get(&body.entity) else { continue };
            if spans_stand(span, &surf_span) {
                carried.insert(body.entity, vel);
            }
        }
    }
    for (entity, belt) in &carried {
        let Some(link) = links.iter().find(|l| l.entity == *entity) else { continue };
        let rb = &mut sim.bodies[link.body];
        let now = rb.linvel();
        let last_sim = Vector::new(link.last_linear[0], link.last_linear[1]);
        let overwritten = (now - last_sim).length() > 1e-3;
        let previous = memory.belt.get(entity).copied().unwrap_or(Vector::ZERO);
        let next = if overwritten { now + belt } else { now - previous + belt };
        rb.set_linvel(next, true);
    }
    memory.belt = carried;

    let mut entered = Vec::new();
    let mut exited = Vec::new();
    for (area, bodies) in &now {
        let prev = memory.inside.get(area);
        for body in bodies {
            if prev.is_none_or(|p| !p.contains(body)) {
                entered.push((*area, *body));
            }
        }
    }
    for (area, bodies) in &memory.inside {
        let cur = now.get(area);
        for body in bodies {
            if cur.is_none_or(|c| !c.contains(body)) {
                exited.push((*area, *body));
            }
        }
    }
    entered.sort_unstable();
    exited.sort_unstable();
    memory.inside = now;
    memory.entered = entered;
    memory.exited = exited;
    holds
}

pub fn restore_damping(sim: &mut PhysicsWorld, holds: &[DampingHold]) {
    for hold in holds {
        if let Some(rb) = sim.bodies.get_mut(hold.body) {
            rb.set_linear_damping(hold.linear);
            rb.set_angular_damping(hold.angular);
        }
    }
}

/// Zero friction on a moving surface so the speed set before the step is kept.
pub fn modify_surface(
    surface: &HashMap<ColliderHandle, Vector>,
    context: &mut ContactModificationContext,
) {
    if !surface.contains_key(&context.collider1) && !surface.contains_key(&context.collider2) {
        return;
    }
    if let Some(rigid) = context.rigid_mut() {
        *rigid.friction = 0.0;
    }
}
