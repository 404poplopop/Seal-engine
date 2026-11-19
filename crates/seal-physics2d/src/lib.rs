//! 2D physics for Seal worlds.
//!
//! Entities with a [`Transform`], a [`Body2D`] and a [`Collider2D`] take
//! part. [`Physics2D::step`] adds new bodies, removes ones whose entity or
//! components are gone, moves fixed and kinematic bodies to their
//! transforms, steps the simulation and writes dynamic bodies back into
//! `Transform` (x, y and `rotation[2]` in degrees) and [`Velocity2D`].
//!
//! Gravity and time scale come from the world's `SceneSettings`, so the
//! scene file controls them. Rapier runs with its cross-platform
//! determinism switched on: the same scene and inputs give the same
//! positions on every machine.

use std::collections::BTreeMap;
use std::sync::mpsc;

use rapier2d::prelude::*;
use seal_ecs::core::{Transform, SceneSettings};
use seal_ecs::{text_component, Entity, World};

pub mod area_effect;
mod joint_extra;
mod queries;
pub mod profiler;
pub mod settings;
pub use joint_extra::JointBreak;
pub use area_effect::{AreaEffect, SurfaceEffect};
pub use profiler::normalize_history;
pub use rapier2d;
pub use settings::{Physics2DProfiler, Physics2DSettings};

/// How a body moves.
#[derive(Clone, Debug, PartialEq)]
pub struct Body2D {
    /// `dynamic`, `fixed` or `kinematic`.
    pub kind: String,
    pub gravity_scale: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub lock_rotation: bool,
    pub can_sleep: bool,
    pub constant_force: [f32; 2],
    pub constant_torque: f32,
    /// Extra mass in world units, added after the collider density.
    pub additional_mass: f32,
    /// Continuous collision detection, for fast small bodies.
    pub ccd: bool,
}

impl Default for Body2D {
    fn default() -> Self {
        Self {
            kind: "dynamic".into(),
            gravity_scale: 1.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            lock_rotation: false,
            can_sleep: true,
            constant_force: [0.0; 2],
            constant_torque: 0.0,
            additional_mass: 0.0,
            ccd: false,
        }
    }
}
text_component!(Body2D as "body2d" {
    kind,
    gravity_scale,
    linear_damping,
    angular_damping,
    lock_rotation,
    can_sleep,
    constant_force,
    constant_torque,
    additional_mass,
    ccd
});

impl Body2D {
    pub fn fixed() -> Self {
        Self { kind: "fixed".into(), ..Default::default() }
    }

    pub fn kinematic() -> Self {
        Self { kind: "kinematic".into(), ..Default::default() }
    }
}

/// The body's shape and surface.
#[derive(Clone, Debug, PartialEq)]
pub struct Collider2D {
    /// `box`, `circle`, `capsule` (upright), `capsule_x` (lying down),
    /// `polygon` (convex outline of `points`) or `edge` (a line through `points`).
    pub shape: String,
    /// Full width and height in world units. A circle uses the width as its diameter.
    pub size: [f32; 2],
    pub friction: f32,
    pub restitution: f32,
    pub density: f32,
    /// Reports overlaps but does not push.
    pub sensor: bool,
    /// Outline points for `polygon` and `edge`, in the object's own units (scaled with it).
    pub points: Vec<[f32; 2]>,
    /// Centre of mass offset in the object's own units, before scale.
    pub center_of_mass: [f32; 2],
    /// Layers this collider is on (bit 0 is layer 1).
    pub layer: u32,
    /// Layers it collides with. Two colliders touch only when each one's mask
    /// includes a layer of the other.
    pub mask: u32,
    /// Solid only from above (along the object's up): things pass through from below and the sides.
    pub one_way: bool,
    /// A named surface (`ice`, `rubber`, `metal`, `wood`, `stone`, `bouncy`) that
    /// replaces friction, bounce and density. Empty uses the numbers above.
    pub material: String,
    /// How two frictions mix: `average`, `min`, `max` or `multiply`.
    pub friction_combine: String,
    /// How two bounces mix: `average`, `min`, `max` or `multiply`.
    pub bounce_combine: String,
}

impl Default for Collider2D {
    fn default() -> Self {
        Self {
            shape: "box".into(),
            size: [1.0, 1.0],
            friction: 0.5,
            restitution: 0.0,
            density: 1.0,
            sensor: false,
            points: Vec::new(),
            center_of_mass: [0.0, 0.0],
            layer: 1,
            mask: u32::MAX,
            one_way: false,
            material: String::new(),
            friction_combine: "average".into(),
            bounce_combine: "average".into(),
        }
    }
}
text_component!(Collider2D as "collider2d" {
    shape,
    size,
    friction,
    restitution,
    density,
    sensor,
    points,
    center_of_mass,
    layer,
    mask,
    one_way,
    material,
    friction_combine,
    bounce_combine
});

/// The shapes a collider can take.
pub const SHAPES: &[&str] = &["box", "circle", "capsule", "capsule_x", "polygon", "edge"];

/// Built-in surfaces: name, friction, bounce, density.
pub const MATERIALS: &[(&str, f32, f32, f32)] = &[
    ("ice", 0.02, 0.05, 0.9),
    ("rubber", 1.0, 0.8, 1.1),
    ("metal", 0.4, 0.1, 7.8),
    ("wood", 0.5, 0.2, 0.7),
    ("stone", 0.7, 0.05, 2.5),
    ("bouncy", 0.3, 0.95, 0.5),
];

/// Friction, bounce and density for a named surface.
pub fn material(name: &str) -> Option<(f32, f32, f32)> {
    MATERIALS.iter().find(|m| m.0 == name).map(|m| (m.1, m.2, m.3))
}

fn combine_rule(name: &str) -> Option<CoefficientCombineRule> {
    Some(match name {
        "" | "average" => CoefficientCombineRule::Average,
        "min" => CoefficientCombineRule::Min,
        "max" => CoefficientCombineRule::Max,
        "multiply" => CoefficientCombineRule::Multiply,
        _ => return None,
    })
}

/// The joint kinds: `fixed` (welded), `hinge` (turns around the anchor),
/// `slider` (moves along `axis`), `distance` (keeps `length` apart), `rope`
/// (at most `length` apart), `spring` (pulled back to `length`), `wheel`
/// (turns freely, sprung along `axis`), `friction` (resists relative motion up
/// to max force and torque), `relative` (keeps a linear and angular offset),
/// and `target` (pulls an anchor toward a world point).
pub const JOINTS: &[&str] = &[
    "fixed", "hinge", "slider", "distance", "rope", "spring", "wheel", "friction", "relative",
    "target",
];

/// Connects this object's body to another body, or to a point in the world when `other` is empty.
#[derive(Clone, Debug, PartialEq)]
pub struct Joint2D {
    pub kind: String,
    pub other: Option<Entity>,
    /// Where it attaches on this object, in its own units.
    pub anchor: [f32; 2],
    /// Where it attaches on the other object, in its units; a world point when `other` is empty.
    pub other_anchor: [f32; 2],
    /// Slide or suspension direction for `slider` and `wheel`, in this object's frame.
    pub axis: [f32; 2],
    /// Lower and upper limit: degrees for `hinge`, units for `slider`. Equal values mean no limit.
    pub limits: [f32; 2],
    /// Rest length for `distance`, `rope` and `spring`; 0 uses the distance when play starts.
    pub length: f32,
    pub stiffness: f32,
    pub damping: f32,
    /// Motor target speed (degrees or units per second) for `hinge`, `slider` and `wheel`.
    pub motor_speed: f32,
    /// How hard the motor may push; 0 means no motor.
    pub motor_force: f32,
    /// Whether the two joined bodies still bump into each other.
    pub collide: bool,
    /// The joint snaps when reaction force exceeds this; 0 is never.
    pub break_force: f32,
    /// The joint snaps when reaction torque exceeds this; 0 is never.
    pub break_torque: f32,
    /// Max linear resistance or correction force (`friction`, `relative`, `target`).
    pub max_force: f32,
    /// Max angular resistance or correction torque (`friction`, `relative`).
    pub max_torque: f32,
    /// How strongly a `relative` joint corrects offset (0 to 1).
    pub correction: f32,
    /// Linear offset a `relative` joint keeps, in this object's units.
    pub offset: [f32; 2],
    /// World point a `target` joint pulls toward.
    pub target: [f32; 2],
}

impl Default for Joint2D {
    fn default() -> Self {
        Self {
            kind: "hinge".into(),
            other: None,
            anchor: [0.0; 2],
            other_anchor: [0.0; 2],
            axis: [1.0, 0.0],
            limits: [0.0; 2],
            length: 0.0,
            stiffness: 50.0,
            damping: 2.0,
            motor_speed: 0.0,
            motor_force: 0.0,
            collide: false,
            break_force: 0.0,
            break_torque: 0.0,
            max_force: 0.0,
            max_torque: 0.0,
            correction: 1.0,
            offset: [0.0; 2],
            target: [0.0; 2],
        }
    }
}
text_component!(Joint2D as "joint2d" {
    kind, other, anchor, other_anchor, axis, limits, length, stiffness, damping, motor_speed,
    motor_force, collide, break_force, break_torque, max_force, max_torque, correction, offset,
    target
});

/// Velocity in world units per second and degrees per second. Write to it to push a body.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Velocity2D {
    pub linear: [f32; 2],
    pub angular: f32,
}
text_component!(Velocity2D as "velocity2d" { linear, angular });

pub fn register_physics2d(world: &mut World) {
    world.register_text::<Body2D>();
    world.register_text::<Collider2D>();
    world.register_text::<Velocity2D>();
    world.register_text::<Joint2D>();
    area_effect::register(world);
}

#[derive(Clone, Debug, PartialEq)]
pub enum PhysicsError {
    BadKind { entity: Entity, kind: String },
    BadShape { entity: Entity, shape: String },
    BadPoints { entity: Entity, shape: String },
    BadJoint { entity: Entity, kind: String },
    BadSurface { entity: Entity, name: String },
}

impl std::fmt::Display for PhysicsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadKind { entity, kind } => {
                write!(f, "{entity:?}: body kind {kind:?} is not dynamic, fixed or kinematic")
            }
            Self::BadShape { entity, shape } => {
                write!(
                    f,
                    "{entity:?}: collider shape {shape:?} is not one of {}",
                    SHAPES.join(", ")
                )
            }
            Self::BadPoints { entity, shape } => {
                write!(f, "{entity:?}: a {shape} collider needs more outline points")
            }
            Self::BadJoint { entity, kind } => {
                write!(f, "{entity:?}: joint kind {kind:?} is not one of {}", JOINTS.join(", "))
            }
            Self::BadSurface { entity, name } => {
                write!(f, "{entity:?}: {name:?} is not a known surface or mix rule")
            }
        }
    }
}

impl std::error::Error for PhysicsError {}

/// Two colliders started or stopped touching during the last step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Contact2D {
    pub a: Entity,
    pub b: Entity,
    pub started: bool,
    pub sensor: bool,
}

/// A ray hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit2D {
    pub entity: Entity,
    pub distance: f32,
    pub point: [f32; 2],
    pub normal: [f32; 2],
}

struct JointLink {
    handle: Option<ImpulseJointHandle>,
    bodies: (RigidBodyHandle, RigidBodyHandle),
    settings: Joint2D,
    /// Peak drive on this body this step, for break checks.
    load_lin: f32,
    load_ang: f32,
}

/// Contact hooks: one-way platforms and moving surfaces.
#[derive(Default)]
struct Hooks {
    one_way: std::collections::HashSet<ColliderHandle>,
    surface: std::collections::HashMap<ColliderHandle, Vector>,
}

impl PhysicsHooks for Hooks {
    fn modify_solver_contacts(&self, context: &mut ContactModificationContext) {
        let (c1, c2) = (context.collider1, context.collider2);
        let up_of = |h: ColliderHandle| {
            context.colliders.get(h).map(|c| c.position().rotation * Vector::new(0.0, 1.0))
        };
        let allowed = if self.one_way.contains(&c1) {
            // Normal points out of the platform: allowed when it points up.
            Some(Vector::new(0.0, 1.0))
        } else if self.one_way.contains(&c2) {
            // Normal points out of the other collider: allowed when it points down
            // relative to the platform, written in the other collider's frame.
            match (up_of(c2), context.colliders.get(c1)) {
                (Some(up), Some(other)) => Some(other.position().rotation.inverse() * -up),
                _ => None,
            }
        } else {
            None
        };
        if let Some(n) = allowed {
            context.update_as_oneway_platform(n, 0.7);
        }
        area_effect::modify_surface(&self.surface, context);
    }
}

struct Link {
    body: RigidBodyHandle,
    collider: ColliderHandle,
    settings: (Body2D, Collider2D),
    velocity: Velocity2D,
    /// Extra colliders from child objects that have a collider and no body.
    extras: Vec<ColliderHandle>,
    /// The child poses and colliders those extras were built from.
    parts: Vec<(Transform, Collider2D)>,
    /// Pose after the previous fixed step, for render interpolation.
    prev: TransformPose,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct TransformPose {
    pos: [f32; 2],
    rot_deg: f32,
}

/// The simulation behind a world.
pub struct Physics2D {
    sim: PhysicsWorld,
    links: BTreeMap<Entity, Link>,
    /// World units per metre; gravity in `SceneSettings` is in metres per second squared.
    pub units_per_meter: f32,
    pub settings: Physics2DSettings,
    pub profiler: Physics2DProfiler,
    /// Contacts from the last step, in a stable order.
    pub contacts: Vec<Contact2D>,
    joints: BTreeMap<Entity, JointLink>,
    /// Joints removed this step because they broke.
    pub joint_breaks: Vec<JointBreak>,
    /// A fixed body at the origin that world-anchored joints hold on to.
    ground: Option<RigidBodyHandle>,
    hooks: Hooks,
    areas: area_effect::Memory,
}

impl Default for Physics2D {
    fn default() -> Self {
        Self::new()
    }
}

const DEG: f32 = std::f32::consts::PI / 180.0;

fn pose_of(t: &Transform) -> Pose {
    Pose::new(Vector::new(t.translation[0], t.translation[1]), t.rotation[2] * DEG)
}

impl Physics2D {
    pub fn new() -> Self {
        Self {
            sim: PhysicsWorld::new(),
            links: BTreeMap::new(),
            units_per_meter: 1.0,
            settings: Physics2DSettings::default(),
            profiler: Physics2DProfiler::default(),
            contacts: Vec::new(),
            joints: BTreeMap::new(),
            joint_breaks: Vec::new(),
            ground: None,
            hooks: Hooks::default(),
            areas: area_effect::Memory::default(),
        }
    }

    /// Area and body pairs that started or stopped overlapping on the last step.
    pub fn area_events(&self) -> (&[(Entity, Entity)], &[(Entity, Entity)]) {
        (&self.areas.entered, &self.areas.exited)
    }

    /// Dynamic bodies inside each area after the last step, sorted.
    pub fn bodies_inside(&self, area: Entity) -> Vec<Entity> {
        self.areas.inside.get(&area).map(|s| s.iter().copied().collect()).unwrap_or_default()
    }

    /// How many joints are in the simulation.
    pub fn joint_count(&self) -> usize {
        self.joints.len()
    }

    pub fn body_count(&self) -> usize {
        self.links.len()
    }

    fn build(
        entity: Entity,
        t: &Transform,
        body: &Body2D,
        collider: &Collider2D,
    ) -> Result<(RigidBody, Collider), PhysicsError> {
        let builder = match body.kind.as_str() {
            "dynamic" => RigidBodyBuilder::dynamic(),
            "fixed" => RigidBodyBuilder::fixed(),
            "kinematic" => RigidBodyBuilder::kinematic_position_based(),
            other => return Err(PhysicsError::BadKind { entity, kind: other.into() }),
        };
        let mut builder = builder
            .pose(pose_of(t))
            .gravity_scale(body.gravity_scale)
            .linear_damping(body.linear_damping)
            .ccd_enabled(body.ccd)
            .user_data(entity.to_bits() as u128);
        if body.angular_damping > 0.0 {
            builder = builder.angular_damping(body.angular_damping);
        }
        if body.lock_rotation {
            builder = builder.lock_rotations();
        }
        if body.additional_mass > 0.0 {
            builder = builder.additional_mass(body.additional_mass);
        }
        let collider = Self::collider_only(entity, t, collider, None)?;
        Ok((builder.build(), collider))
    }

    fn collider_only(
        entity: Entity,
        t: &Transform,
        collider: &Collider2D,
        local: Option<&Transform>,
    ) -> Result<Collider, PhysicsError> {
        let (sx, sy) = (t.scale[0].abs(), t.scale[1].abs());
        let [w, h] = collider.size;
        let points: Vec<Vector> = collider
            .points
            .iter()
            .map(|p| Vector::new(p[0] * t.scale[0], p[1] * t.scale[1]))
            .collect();
        let bad_points = || PhysicsError::BadPoints { entity, shape: collider.shape.clone() };
        let mut shape = match collider.shape.as_str() {
            "box" => ColliderBuilder::cuboid(w * sx * 0.5, h * sy * 0.5),
            "circle" => ColliderBuilder::ball(w * sx.max(sy) * 0.5),
            "capsule" => {
                let r = w * sx * 0.5;
                ColliderBuilder::capsule_y((h * sy * 0.5 - r).max(0.0), r)
            }
            "capsule_x" => {
                let r = h * sy * 0.5;
                ColliderBuilder::capsule_x((w * sx * 0.5 - r).max(0.0), r)
            }
            "polygon" => {
                if points.len() < 3 {
                    return Err(bad_points());
                }
                ColliderBuilder::convex_hull(&points).ok_or_else(bad_points)?
            }
            "edge" => {
                if points.len() < 2 {
                    return Err(bad_points());
                }
                ColliderBuilder::polyline(points, None)
            }
            other => return Err(PhysicsError::BadShape { entity, shape: other.into() }),
        };
        let (friction, restitution, density) = if collider.material.is_empty() {
            (collider.friction, collider.restitution, collider.density)
        } else {
            material(&collider.material)
                .ok_or(PhysicsError::BadSurface { entity, name: collider.material.clone() })?
        };
        let surface = |name: &str| {
            combine_rule(name).ok_or(PhysicsError::BadSurface { entity, name: name.into() })
        };
        let groups = InteractionGroups::new(
            Group::from_bits_retain(collider.layer),
            Group::from_bits_retain(collider.mask),
            InteractionTestMode::And,
        );
        let mut hooks = ActiveHooks::empty();
        if collider.one_way {
            hooks |= ActiveHooks::MODIFY_SOLVER_CONTACTS;
        }
        if let Some(local) = local {
            shape = shape.position(pose_of(local));
        }
        let mut col = shape
            .friction(friction)
            .restitution(restitution)
            .density(density);
        if collider.center_of_mass[0] != 0.0 || collider.center_of_mass[1] != 0.0 {
            col = col.translation(Vector::new(
                collider.center_of_mass[0] * sx,
                collider.center_of_mass[1] * sy,
            ));
        }
        let collider = col
            .friction_combine_rule(surface(&collider.friction_combine)?)
            .restitution_combine_rule(surface(&collider.bounce_combine)?)
            .collision_groups(groups)
            .active_hooks(hooks)
            .sensor(collider.sensor)
            .active_events(ActiveEvents::COLLISION_EVENTS)
            .user_data(entity.to_bits() as u128)
            .build();
        Ok(collider)
    }

    fn drop_link(&mut self, link: &Link) {
        self.hooks.one_way.remove(&link.collider);
        for handle in &link.extras {
            self.hooks.one_way.remove(handle);
        }
        self.sim.remove_body(link.body);
    }

    /// Attach colliders from direct children that have a collider and no body.
    fn sync_parts(&mut self, world: &World) -> Result<(), PhysicsError> {
        let parents: Vec<Entity> = self.links.keys().copied().collect();
        for parent in parents {
            let mine: Vec<(Transform, Collider2D)> = world
                .children(parent)
                .iter()
                .filter_map(|child| {
                    if world.get::<Body2D>(*child).is_some() {
                        return None;
                    }
                    Some((world.get::<Transform>(*child)?.clone(), world.get::<Collider2D>(*child)?.clone()))
                })
                .collect();
            if self.links.get(&parent).is_some_and(|link| link.parts == mine) {
                continue;
            }
            let old = self.links.get(&parent).map(|link| link.extras.clone()).unwrap_or_default();
            for handle in old {
                self.hooks.one_way.remove(&handle);
                self.sim.remove_collider(handle);
            }
            let body = self.links.get(&parent).expect("listed").body;
            let mut extras = Vec::new();
            for (pose, collider) in &mine {
                let built = Self::collider_only(parent, pose, collider, Some(pose))?;
                let handle = self.sim.insert_collider(built, Some(body));
                if collider.one_way {
                    self.hooks.one_way.insert(handle);
                }
                extras.push(handle);
            }
            let link = self.links.get_mut(&parent).expect("listed");
            link.extras = extras;
            link.parts = mine;
        }
        Ok(())
    }

    fn sync_in(&mut self, world: &mut World) -> Result<(), PhysicsError> {
        let rows = world
            .query::<(Entity, &Transform, &Body2D, &Collider2D)>()
            .collect(|(e, t, b, c)| (e, t.clone(), b.clone(), c.clone()));
        let present: std::collections::BTreeSet<Entity> = rows.iter().map(|r| r.0).collect();
        let gone: Vec<Entity> =
            self.links.keys().filter(|e| !present.contains(e)).copied().collect();
        for e in gone {
            let link = self.links.remove(&e).expect("listed");
            self.drop_link(&link);
        }
        for (entity, t, body, collider) in rows {
            // Without a Velocity2D component the body keeps its own velocity.
            let pushed = world.get::<Velocity2D>(entity).cloned();
            let velocity = pushed.clone().unwrap_or_default();
            let rebuild = self
                .links
                .get(&entity)
                .is_none_or(|l| l.settings != (body.clone(), collider.clone()));
            if rebuild {
                if let Some(old) = self.links.remove(&entity) {
                    self.drop_link(&old);
                }
                let (rb, col) = Self::build(entity, &t, &body, &collider)?;
                let one_way = collider.one_way;
                let (handle, col_handle) = self.sim.insert(rb, col);
                if one_way {
                    self.hooks.one_way.insert(col_handle);
                }
                let rb = &mut self.sim.bodies[handle];
                rb.set_linvel(Vector::new(velocity.linear[0], velocity.linear[1]), true);
                rb.set_angvel(velocity.angular * DEG, true);
                let pose = self.sim.bodies[handle].position();
                let prev = TransformPose {
                    pos: [pose.translation.x, pose.translation.y],
                    rot_deg: pose.rotation.angle() / DEG,
                };
                self.links.insert(
                    entity,
                    Link {
                        body: handle,
                        collider: col_handle,
                        settings: (body, collider),
                        velocity,
                        extras: Vec::new(),
                        parts: Vec::new(),
                        prev,
                    },
                );
                continue;
            }
            let link = self.links.get_mut(&entity).expect("present");
            let rb = &mut self.sim.bodies[link.body];
            match link.settings.0.kind.as_str() {
                "kinematic" => rb.set_next_kinematic_position(pose_of(&t)),
                "fixed" => {
                    let pose = pose_of(&t);
                    if *rb.position() != pose {
                        rb.set_position(pose, false);
                    }
                }
                _ => {
                    // Gameplay changed the velocity since we last wrote it.
                    if pushed.is_some() && velocity != link.velocity {
                        rb.set_linvel(Vector::new(velocity.linear[0], velocity.linear[1]), true);
                        rb.set_angvel(velocity.angular * DEG, true);
                    }
                    // Gameplay teleported the body.
                    let pose = pose_of(&t);
                    let current = *rb.position();
                    if (current.translation - pose.translation).length() > 1e-4
                        || (current.rotation.angle() - pose.rotation.angle()).abs() > 1e-4
                    {
                        rb.set_position(pose, true);
                    }
                }
            }
        }
        self.sync_parts(world)
    }

    fn ground(&mut self) -> RigidBodyHandle {
        if let Some(h) = self.ground.filter(|h| self.sim.bodies.get(*h).is_some()) {
            return h;
        }
        let h = self.sim.insert_body(RigidBodyBuilder::fixed());
        self.ground = Some(h);
        h
    }

    /// Body 1 is the other object (or the world), body 2 is the joint's own
    /// object, so limits and motors read as "this object relative to the other".
    fn build_joint(
        entity: Entity,
        j: &Joint2D,
        p1: Pose,
        p2: Pose,
    ) -> Result<GenericJoint, PhysicsError> {
        let a1 = Vector::new(j.other_anchor[0], j.other_anchor[1]);
        let a2 = Vector::new(j.anchor[0], j.anchor[1]);
        let axis = Vector::new(j.axis[0], j.axis[1]);
        let axis = if axis.length() > 1e-6 { axis.normalize() } else { Vector::new(1.0, 0.0) };
        // The axis is given in this object's frame; body 1 needs it in its own.
        let axis1 = p1.rotation.inverse() * (p2.rotation * axis);
        let length = if j.length > 0.0 { j.length } else { ((p1 * a1) - (p2 * a2)).length() };
        let limited = j.limits[1] > j.limits[0];
        let mut joint: GenericJoint = match j.kind.as_str() {
            "fixed" => FixedJointBuilder::new()
                .local_frame1(Pose::new(a1, p2.rotation.angle() - p1.rotation.angle()))
                .local_frame2(Pose::from_translation(a2))
                .build()
                .into(),
            "hinge" => {
                let mut b = RevoluteJointBuilder::new().local_anchor1(a1).local_anchor2(a2);
                if limited {
                    b = b.limits([j.limits[0] * DEG, j.limits[1] * DEG]);
                }
                if j.motor_force > 0.0 {
                    b = b.motor_velocity(j.motor_speed * DEG, 1.0).motor_max_force(j.motor_force);
                }
                b.build().into()
            }
            "slider" => {
                let mut b = PrismaticJointBuilder::new(axis).local_anchor1(a1).local_anchor2(a2);
                if limited {
                    b = b.limits(j.limits);
                }
                if j.motor_force > 0.0 {
                    b = b.motor_velocity(j.motor_speed, 1.0).motor_max_force(j.motor_force);
                }
                let mut g: GenericJoint = b.build().into();
                g.set_local_axis1(axis1);
                g.set_local_axis2(axis);
                g
            }
            "distance" | "rope" => {
                let mut g: GenericJoint = RopeJointBuilder::new(length)
                    .local_anchor1(a1)
                    .local_anchor2(a2)
                    .build()
                    .into();
                if j.kind == "distance" {
                    g.set_limits(JointAxis::LinX, [length, length]);
                }
                g
            }
            "spring" => SpringJointBuilder::new(length, j.stiffness, j.damping)
                .local_anchor1(a1)
                .local_anchor2(a2)
                .build()
                .into(),
            "wheel" => {
                // Locked across the axis, sprung along it, free to turn.
                let mut b = GenericJointBuilder::new(JointAxesMask::LIN_Y)
                    .local_axis1(axis1)
                    .local_axis2(axis)
                    .local_anchor1(a1)
                    .local_anchor2(a2)
                    .motor_position(JointAxis::LinX, 0.0, j.stiffness, j.damping);
                if j.motor_force > 0.0 {
                    b = b
                        .motor_velocity(JointAxis::AngX, j.motor_speed * DEG, 1.0)
                        .motor_max_force(JointAxis::AngX, j.motor_force);
                }
                b.build()
            }
            "friction" => joint_extra::friction_joint(j, a1, a2),
            "relative" => joint_extra::relative_joint(j, a1, a2, p1, p2),
            "target" => return Err(PhysicsError::BadJoint { entity, kind: j.kind.clone() }),
            other => return Err(PhysicsError::BadJoint { entity, kind: other.into() }),
        };
        joint.set_contacts_enabled(j.collide);
        Ok(joint)
    }

    fn insert_joint(
        &mut self,
        entity: Entity,
        j: &Joint2D,
        bodies: (RigidBodyHandle, RigidBodyHandle),
    ) -> Result<(), PhysicsError> {
        if j.kind == "target" {
            self.joints.insert(
                entity,
                JointLink {
                    handle: None,
                    bodies,
                    settings: j.clone(),
                    load_lin: 0.0,
                    load_ang: 0.0,
                },
            );
            return Ok(());
        }
        let p1 = *self.sim.bodies[bodies.1].position();
        let p2 = *self.sim.bodies[bodies.0].position();
        let joint = Self::build_joint(entity, j, p1, p2)?;
        let handle = self.sim.insert_impulse_joint(bodies.1, bodies.0, joint);
        self.joints.insert(
            entity,
            JointLink {
                handle: Some(handle),
                bodies,
                settings: j.clone(),
                load_lin: 0.0,
                load_ang: 0.0,
            },
        );
        Ok(())
    }

    fn sync_joints(&mut self, world: &mut World) -> Result<(), PhysicsError> {
        let rows = world.query::<(Entity, &Joint2D)>().collect(|(e, j)| (e, j.clone()));
        let mut wanted = BTreeMap::new();
        for (entity, j) in rows {
            let Some(b1) = self.links.get(&entity).map(|l| l.body) else { continue };
            let b2 = match j.other {
                Some(other) => match self.links.get(&other) {
                    Some(l) => l.body,
                    None => continue,
                },
                None => self.ground(),
            };
            wanted.insert(entity, (j, (b1, b2)));
        }
        let stale: Vec<Entity> = self
            .joints
            .iter()
            .filter(|(e, l)| wanted.get(*e).is_none_or(|w| w.0 != l.settings || w.1 != l.bodies))
            .map(|(e, _)| *e)
            .collect();
        for e in stale {
            let link = self.joints.remove(&e).expect("listed");
            if let Some(h) = link.handle {
                self.sim.remove_impulse_joint(h);
            }
        }
        for (entity, (j, bodies)) in wanted {
            if self.joints.contains_key(&entity) {
                continue;
            }
            self.insert_joint(entity, &j, bodies)?;
        }
        Ok(())
    }

    /// Set the world target of a target joint on a live body.
    pub fn set_joint_target(&mut self, entity: Entity, target: [f32; 2]) -> bool {
        let Some(link) = self.joints.get_mut(&entity) else { return false };
        if link.settings.kind != "target" {
            return false;
        }
        link.settings.target = target;
        true
    }

    pub fn collider_count(&self) -> usize {
        self.sim.colliders.len()
    }

    /// Total mass of a dynamic body (collider density plus any extra mass).
    pub fn mass_of(&self, entity: Entity) -> Option<f32> {
        let link = self.links.get(&entity)?;
        let rb = &self.sim.bodies[link.body];
        Some(rb.mass())
    }

    /// Add a force in world units for one fixed step.
    pub fn apply_force(&mut self, entity: Entity, force: [f32; 2]) {
        let Some(link) = self.links.get(&entity) else { return };
        if link.settings.0.kind != "dynamic" {
            return;
        }
        let f = Vector::new(force[0], force[1]);
        if let Some(j) = self.joints.get_mut(&entity) {
            j.load_lin = j.load_lin.max(f.length());
        }
        let rb = &mut self.sim.bodies[link.body];
        rb.add_force(f, true);
    }

    pub fn apply_torque(&mut self, entity: Entity, torque: f32) {
        let Some(link) = self.links.get(&entity) else { return };
        if link.settings.0.kind != "dynamic" {
            return;
        }
        if let Some(j) = self.joints.get_mut(&entity) {
            j.load_ang = j.load_ang.max(torque.abs());
        }
        self.sim.bodies[link.body].add_torque(torque, true);
    }

    pub fn set_velocity(&mut self, entity: Entity, linear: [f32; 2], angular: f32) {
        let Some(link) = self.links.get_mut(&entity) else { return };
        if link.settings.0.kind != "dynamic" {
            return;
        }
        let rb = &mut self.sim.bodies[link.body];
        rb.set_linvel(Vector::new(linear[0], linear[1]), true);
        rb.set_angvel(angular * DEG, true);
        link.velocity = Velocity2D { linear, angular };
    }

    /// Blend dynamic transforms toward the pose after the last step (`alpha` is 0 at a fixed tick, 1 just before the next).
    pub fn write_transforms(&self, world: &mut World, alpha: f32) {
        let a = if self.settings.render_interpolation {
            alpha.clamp(0.0, 1.0)
        } else {
            1.0
        };
        for (entity, link) in &self.links {
            if link.settings.0.kind != "dynamic" {
                continue;
            }
            let rb = &self.sim.bodies[link.body];
            let pose = rb.position();
            let curr = TransformPose {
                pos: [pose.translation.x, pose.translation.y],
                rot_deg: pose.rotation.angle() / DEG,
            };
            let (pos, rot) = if a < 1.0 {
                let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
                (
                    [
                        lerp(link.prev.pos[0], curr.pos[0], a),
                        lerp(link.prev.pos[1], curr.pos[1], a),
                    ],
                    lerp(link.prev.rot_deg, curr.rot_deg, a),
                )
            } else {
                (curr.pos, curr.rot_deg)
            };
            if let Some(t) = world.get_mut::<Transform>(*entity) {
                t.translation[0] = pos[0];
                t.translation[1] = pos[1];
                t.rotation[2] = rot;
            }
        }
    }

    /// Advance the simulation by `dt` seconds (scaled by the world's time scale).
    pub fn step(&mut self, world: &mut World, dt: f32) -> Result<(), PhysicsError> {
        let _zone = seal_trace::zone!(seal_trace::Channel::CPU, "physics2d.step");
        let started = std::time::Instant::now();
        self.sync_in(world)?;
        self.sync_joints(world)?;
        self.joint_breaks.clear();
        let scene = world.resource::<SceneSettings>().cloned().unwrap_or_default();
        if self.settings != Physics2DSettings::default() {
            self.settings.apply_integration(&mut self.sim.integration_parameters);
        }
        let grav = if self.settings.gravity_scale == 1.0 {
            scene.gravity
        } else {
            scene.gravity * self.settings.gravity_scale
        };
        self.sim.gravity = Vector::new(0.0, -grav * self.units_per_meter);
        self.sim.integration_parameters.dt = dt * scene.time_scale;
        self.sim.integration_parameters.length_unit = self.units_per_meter;
        if !self.settings.sleeping {
            for link in self.links.values() {
                self.sim.bodies[link.body].wake_up(true);
            }
        }
        for link in self.links.values_mut() {
            if link.settings.0.kind == "dynamic" {
                if !link.settings.0.can_sleep {
                    self.sim.bodies[link.body].wake_up(true);
                }
                let rb = &self.sim.bodies[link.body];
                let pose = rb.position();
                link.prev = TransformPose {
                    pos: [pose.translation.x, pose.translation.y],
                    rot_deg: pose.rotation.angle() / DEG,
                };
            }
            let body = &link.settings.0;
            if body.kind == "dynamic"
                && (body.constant_force != [0.0, 0.0] || body.constant_torque != 0.0)
            {
                let rb = &mut self.sim.bodies[link.body];
                let dt = self.sim.integration_parameters.dt;
                rb.apply_impulse(
                    Vector::new(body.constant_force[0] * dt, body.constant_force[1] * dt),
                    true,
                );
                if body.constant_torque != 0.0 {
                    rb.add_torque(body.constant_torque, true);
                }
            }
        }
        let linked: Vec<area_effect::Linked> = self
            .links
            .iter()
            .map(|(entity, link)| area_effect::Linked {
                entity: *entity,
                body: link.body,
                collider: link.collider,
                kind: link.settings.0.kind.clone(),
                layer: link.settings.1.layer,
                last_linear: link.velocity.linear,
            })
            .collect();
        let holds = area_effect::before_step(
            &mut self.sim,
            world,
            &linked,
            &mut self.hooks.surface,
            &mut self.areas,
        );
        let dt = self.sim.integration_parameters.dt;
        self.drive_target_joints(dt);
        let (tx, rx) = mpsc::channel();
        let (ftx, _frx) = mpsc::channel();
        let (stx, _srx) = mpsc::channel();
        let events = ChannelEventCollector::new(tx, ftx, stx);
        self.sim.step_with_events(&self.hooks, &events);
        self.break_overloaded_joints(world, dt);
        for link in self.joints.values_mut() {
            link.load_lin = 0.0;
            link.load_ang = 0.0;
        }
        area_effect::restore_damping(&mut self.sim, &holds);
        // Write dynamic bodies back.
        for (entity, link) in self.links.iter_mut() {
            if link.settings.0.kind != "dynamic" {
                continue;
            }
            let rb = &self.sim.bodies[link.body];
            let pose = rb.position();
            if let Some(t) = world.get_mut::<Transform>(*entity) {
                t.translation[0] = pose.translation.x;
                t.translation[1] = pose.translation.y;
                t.rotation[2] = pose.rotation.angle() / DEG;
            }
            let v = rb.linvel();
            link.velocity = Velocity2D { linear: [v.x, v.y], angular: rb.angvel() / DEG };
            if world.has::<Velocity2D>(*entity) {
                if let Some(slot) = world.get_mut::<Velocity2D>(*entity) {
                    *slot = link.velocity.clone();
                }
            }
        }
        let entity_of = |h: ColliderHandle| {
            self.sim
                .colliders
                .get(h)
                .map(|c| (Entity::from_bits(c.user_data as u64), c.is_sensor()))
        };
        let mut contacts = Vec::new();
        for event in rx.try_iter() {
            let (Some((a, sa)), Some((b, sb))) =
                (entity_of(event.collider1()), entity_of(event.collider2()))
            else {
                continue;
            };
            let (a, b) = if a <= b { (a, b) } else { (b, a) };
            contacts.push(Contact2D { a, b, started: event.started(), sensor: sa || sb });
        }
        contacts.sort_by_key(|c| (c.a, c.b, !c.started));
        self.contacts = contacts;
        self.profiler.bodies = self.body_count();
        self.profiler.colliders = self.collider_count();
        self.profiler.contacts = self.contacts.len();
        self.profiler.joints = self.joint_count();
        let ms = started.elapsed().as_secs_f32() * 1000.0;
        self.profiler.record_step(ms);
        Ok(())
    }

    /// The first collider along a ray from `origin` in direction `dir`.
    pub fn raycast(&self, origin: [f32; 2], dir: [f32; 2], max_distance: f32) -> Option<RayHit2D> {
        let d = Vector::new(dir[0], dir[1]);
        let len = d.length();
        if len == 0.0 {
            return None;
        }
        let ray = Ray::new(Vector::new(origin[0], origin[1]), d / len);
        let query = self.sim.query_pipeline();
        let (handle, hit) = query.cast_ray_and_get_normal(&ray, max_distance, true)?;
        let collider = self.sim.colliders.get(handle)?;
        let point = ray.point_at(hit.time_of_impact);
        Some(RayHit2D {
            entity: Entity::from_bits(collider.user_data as u64),
            distance: hit.time_of_impact,
            point: [point.x, point.y],
            normal: [hit.normal.x, hit.normal.y],
        })
    }

    /// Body handle and collider handle for an entity, for advanced Rapier use.
    pub fn handles(&self, entity: Entity) -> Option<(RigidBodyHandle, ColliderHandle)> {
        self.links.get(&entity).map(|l| (l.body, l.collider))
    }

    /// The underlying Rapier world, for joints and queries the wrapper doesn't cover yet.
    pub fn rapier(&mut self) -> &mut PhysicsWorld {
        &mut self.sim
    }
}
