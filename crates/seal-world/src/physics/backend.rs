//! The seam the world steps through. The built-in solver is the default.
//! Drift ignores gravity and slides dynamic bodies sideways, so a test can
//! prove a second backend is actually in use.

use super::shapes::{self, Geom};

#[derive(Clone, Debug)]
pub struct BodyDesc {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub dynamic: bool,
    pub gravity_scale: f32,
}

impl Default for BodyDesc {
    fn default() -> Self {
        Self { position: [0.0; 3], velocity: [0.0; 3], dynamic: true, gravity_scale: 1.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JointKind {
    Fixed,
    Distance,
    Hinge,
}

impl JointKind {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "fixed" => Some(Self::Fixed),
            "distance" => Some(Self::Distance),
            "hinge" => Some(Self::Hinge),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct JointDesc {
    pub kind: JointKind,
    pub body_a: u64,
    pub body_b: u64,
    /// 0 keeps the distance the bodies have when the joint is made.
    pub length: f32,
}

#[derive(Clone, Debug)]
pub struct RayHit {
    pub body: u64,
    pub distance: f32,
}

pub trait PhysicsBackend: Send {
    fn name(&self) -> &str;
    fn set_gravity(&mut self, gravity: [f32; 3]);
    fn gravity(&self) -> [f32; 3];
    fn create_body(&mut self, desc: BodyDesc) -> u64;
    fn create_shape(&mut self, body: u64, geom: Geom) -> Result<u64, String>;
    fn create_joint(&mut self, desc: JointDesc) -> Result<u64, String>;
    fn step(&mut self, dt: f32);
    fn translation(&self, body: u64) -> Option<[f32; 3]>;
    fn velocity(&self, body: u64) -> Option<[f32; 3]>;
    fn ray(&self, from: [f32; 3], dir: [f32; 3], max_t: f32) -> Option<RayHit>;
    fn overlap_sphere(&self, center: [f32; 3], radius: f32) -> Vec<u64>;
    fn body_count(&self) -> usize;
}

#[derive(Clone, Debug)]
struct Body {
    position: [f32; 3],
    velocity: [f32; 3],
    dynamic: bool,
    gravity_scale: f32,
}

#[derive(Clone, Debug)]
struct Shape {
    body: u64,
    geom: Geom,
}

#[derive(Clone, Copy, Debug)]
struct Joint {
    kind: JointKind,
    a: u64,
    b: u64,
    rest: [f32; 3],
    length: f32,
}

#[derive(Clone, Debug)]
pub struct BuiltinBackend {
    gravity: [f32; 3],
    bodies: Vec<(u64, Body)>,
    shapes: Vec<Shape>,
    joints: Vec<(u64, Joint)>,
    next: u64,
}

impl Default for BuiltinBackend {
    fn default() -> Self {
        Self {
            gravity: [0.0, -9.81, 0.0],
            bodies: Vec::new(),
            shapes: Vec::new(),
            joints: Vec::new(),
            next: 1,
        }
    }
}

impl BuiltinBackend {
    fn alloc(&mut self) -> u64 {
        let id = self.next;
        self.next += 1;
        id
    }

    fn body_mut(&mut self, id: u64) -> Option<&mut Body> {
        self.bodies.iter_mut().find(|(i, _)| *i == id).map(|(_, body)| body)
    }

    fn integrate(&mut self, dt: f32, gravity: [f32; 3]) {
        for (_, body) in &mut self.bodies {
            if !body.dynamic {
                continue;
            }
            let pull = shapes::scale(gravity, body.gravity_scale * dt);
            body.velocity = shapes::add(body.velocity, pull);
            body.position = shapes::add(body.position, shapes::scale(body.velocity, dt));
        }
    }

    fn nudge(&mut self, drift: [f32; 3], dt: f32) {
        for (_, body) in &mut self.bodies {
            if body.dynamic {
                body.position = shapes::add(body.position, shapes::scale(drift, dt));
            }
        }
    }

    fn solve_joints(&mut self) {
        let joints: Vec<Joint> = self.joints.iter().map(|(_, joint)| *joint).collect();
        for _ in 0..4 {
            for joint in &joints {
                let (Some(a), Some(b)) = (self.translation(joint.a), self.translation(joint.b))
                else {
                    continue;
                };
                let (a_dyn, b_dyn) = {
                    let ad = self
                        .bodies
                        .iter()
                        .find(|(id, _)| *id == joint.a)
                        .is_some_and(|(_, body)| body.dynamic);
                    let bd = self
                        .bodies
                        .iter()
                        .find(|(id, _)| *id == joint.b)
                        .is_some_and(|(_, body)| body.dynamic);
                    (ad, bd)
                };
                let (target_a, target_b) = match joint.kind {
                    JointKind::Fixed => (a, shapes::add(a, joint.rest)),
                    JointKind::Distance | JointKind::Hinge => {
                        let delta = shapes::sub(b, a);
                        let dist = shapes::len(delta);
                        if dist < 1e-6 {
                            continue;
                        }
                        let dir = shapes::scale(delta, 1.0 / dist);
                        let mid = shapes::scale(shapes::add(a, b), 0.5);
                        let half = joint.length * 0.5;
                        (
                            shapes::sub(mid, shapes::scale(dir, half)),
                            shapes::add(mid, shapes::scale(dir, half)),
                        )
                    }
                };
                if joint.kind == JointKind::Fixed {
                    if b_dyn {
                        if let Some(body) = self.body_mut(joint.b) {
                            body.position = target_b;
                        }
                    } else if a_dyn {
                        if let Some(body) = self.body_mut(joint.a) {
                            body.position = shapes::sub(b, joint.rest);
                        }
                    }
                    continue;
                }
                match (a_dyn, b_dyn) {
                    (true, true) => {
                        if let Some(body) = self.body_mut(joint.a) {
                            body.position = target_a;
                        }
                        if let Some(body) = self.body_mut(joint.b) {
                            body.position = target_b;
                        }
                    }
                    (true, false) => {
                        let delta = shapes::sub(b, a);
                        let dist = shapes::len(delta).max(1e-6);
                        let dir = shapes::scale(delta, 1.0 / dist);
                        if let Some(body) = self.body_mut(joint.a) {
                            body.position = shapes::sub(b, shapes::scale(dir, joint.length));
                        }
                    }
                    (false, true) => {
                        let delta = shapes::sub(b, a);
                        let dist = shapes::len(delta).max(1e-6);
                        let dir = shapes::scale(delta, 1.0 / dist);
                        if let Some(body) = self.body_mut(joint.b) {
                            body.position = shapes::add(a, shapes::scale(dir, joint.length));
                        }
                    }
                    (false, false) => {}
                }
            }
        }
    }

    fn pose(&self, body: u64) -> Option<[f32; 3]> {
        self.bodies.iter().find(|(id, _)| *id == body).map(|(_, body)| body.position)
    }
}

impl PhysicsBackend for BuiltinBackend {
    fn name(&self) -> &str {
        "builtin"
    }
    fn set_gravity(&mut self, gravity: [f32; 3]) {
        self.gravity = gravity;
    }
    fn gravity(&self) -> [f32; 3] {
        self.gravity
    }
    fn create_body(&mut self, desc: BodyDesc) -> u64 {
        let id = self.alloc();
        self.bodies.push((
            id,
            Body {
                position: finite3(desc.position),
                velocity: finite3(desc.velocity),
                dynamic: desc.dynamic,
                gravity_scale: if desc.gravity_scale.is_finite() {
                    desc.gravity_scale
                } else {
                    1.0
                },
            },
        ));
        id
    }
    fn create_shape(&mut self, body: u64, geom: Geom) -> Result<u64, String> {
        if self.pose(body).is_none() {
            return Err("no body for that shape".into());
        }
        let id = self.alloc();
        self.shapes.push(Shape { body, geom });
        Ok(id)
    }
    fn create_joint(&mut self, desc: JointDesc) -> Result<u64, String> {
        let (Some(a), Some(b)) = (self.translation(desc.body_a), self.translation(desc.body_b))
        else {
            return Err("a joint needs two bodies".into());
        };
        if desc.body_a == desc.body_b {
            return Err("a joint needs two different bodies".into());
        }
        let rest = shapes::sub(b, a);
        let length = if desc.length > 0.0 && desc.length.is_finite() {
            desc.length
        } else {
            shapes::len(rest).max(0.0)
        };
        let id = self.alloc();
        self.joints
            .push((id, Joint { kind: desc.kind, a: desc.body_a, b: desc.body_b, rest, length }));
        Ok(id)
    }
    fn step(&mut self, dt: f32) {
        let Some(dt) = step_dt(dt) else { return };
        let gravity = self.gravity;
        self.integrate(dt, gravity);
        self.solve_joints();
    }
    fn translation(&self, body: u64) -> Option<[f32; 3]> {
        self.pose(body)
    }
    fn velocity(&self, body: u64) -> Option<[f32; 3]> {
        self.bodies.iter().find(|(id, _)| *id == body).map(|(_, body)| body.velocity)
    }
    fn ray(&self, from: [f32; 3], dir: [f32; 3], max_t: f32) -> Option<RayHit> {
        let mut best: Option<RayHit> = None;
        for shape in &self.shapes {
            let Some(at) = self.pose(shape.body) else { continue };
            if let Some(distance) = shapes::ray(&shape.geom, at, from, dir, max_t) {
                if best.as_ref().is_none_or(|hit| distance < hit.distance) {
                    best = Some(RayHit { body: shape.body, distance });
                }
            }
        }
        best
    }
    fn overlap_sphere(&self, center: [f32; 3], radius: f32) -> Vec<u64> {
        let mut hits = Vec::new();
        for shape in &self.shapes {
            let Some(at) = self.pose(shape.body) else { continue };
            if shapes::overlap_sphere(&shape.geom, at, center, radius)
                && !hits.contains(&shape.body)
            {
                hits.push(shape.body);
            }
        }
        hits.sort_unstable();
        hits
    }
    fn body_count(&self) -> usize {
        self.bodies.len()
    }
}

/// Slides dynamic bodies by a fixed velocity and does not apply gravity.
#[derive(Clone, Debug)]
pub struct DriftBackend {
    inner: BuiltinBackend,
    drift: [f32; 3],
}

impl Default for DriftBackend {
    fn default() -> Self {
        Self { inner: BuiltinBackend::default(), drift: [3.0, 0.0, 0.0] }
    }
}

impl PhysicsBackend for DriftBackend {
    fn name(&self) -> &str {
        "drift"
    }
    fn set_gravity(&mut self, gravity: [f32; 3]) {
        self.inner.set_gravity(gravity);
    }
    fn gravity(&self) -> [f32; 3] {
        self.inner.gravity()
    }
    fn create_body(&mut self, desc: BodyDesc) -> u64 {
        self.inner.create_body(desc)
    }
    fn create_shape(&mut self, body: u64, geom: Geom) -> Result<u64, String> {
        self.inner.create_shape(body, geom)
    }
    fn create_joint(&mut self, desc: JointDesc) -> Result<u64, String> {
        self.inner.create_joint(desc)
    }
    fn step(&mut self, dt: f32) {
        let Some(dt) = step_dt(dt) else { return };
        self.inner.integrate(dt, [0.0; 3]);
        self.inner.nudge(self.drift, dt);
        self.inner.solve_joints();
    }
    fn translation(&self, body: u64) -> Option<[f32; 3]> {
        self.inner.translation(body)
    }
    fn velocity(&self, body: u64) -> Option<[f32; 3]> {
        self.inner.velocity(body)
    }
    fn ray(&self, from: [f32; 3], dir: [f32; 3], max_t: f32) -> Option<RayHit> {
        self.inner.ray(from, dir, max_t)
    }
    fn overlap_sphere(&self, center: [f32; 3], radius: f32) -> Vec<u64> {
        self.inner.overlap_sphere(center, radius)
    }
    fn body_count(&self) -> usize {
        self.inner.body_count()
    }
}

pub fn open_backend(name: &str) -> Result<Box<dyn PhysicsBackend>, String> {
    match name {
        "" | "builtin" => Ok(Box::new(BuiltinBackend::default())),
        "drift" => Ok(Box::new(DriftBackend::default())),
        other => Err(format!("unknown physics backend {other}")),
    }
}

pub const BACKENDS: &[&str] = &["builtin", "drift"];

fn finite3(v: [f32; 3]) -> [f32; 3] {
    v.map(|n| if n.is_finite() { n } else { 0.0 })
}

fn step_dt(dt: f32) -> Option<f32> {
    if !dt.is_finite() || dt <= 0.0 {
        None
    } else {
        Some(dt.min(1.0))
    }
}
