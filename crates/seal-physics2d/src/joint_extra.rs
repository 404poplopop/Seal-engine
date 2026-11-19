//! Extra 2D joint kinds (friction, relative, target), break limits, and target pulls.

use rapier2d::prelude::*;

use crate::{Joint2D, Physics2D, DEG};

/// A joint that broke on the last step. The ECS entity that carried the joint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JointBreak {
    pub entity: seal_ecs::Entity,
}

impl Physics2D {
  /// Pull target joints toward their world point before the solver step.
  pub(crate) fn drive_target_joints(&mut self, dt: f32) {
    if dt <= 0.0 {
      return;
    }
    let rows: Vec<_> = self
      .joints
      .iter()
      .filter(|(_, l)| l.settings.kind == "target")
      .map(|(e, l)| (*e, l.bodies, l.settings.clone()))
      .collect();
    for (_entity, (body, _), j) in rows {
      let rb = &self.sim.bodies[body];
      let pose = *rb.position();
      let anchor =
        pose * Vector::new(j.anchor[0], j.anchor[1]);
      let target = Vector::new(j.target[0], j.target[1]);
      let delta = target - anchor;
      let vel = rb.linvel();
      let freq = j.stiffness.max(0.0);
      let damp = j.damping.max(0.0);
      let mut force = delta * freq - vel * damp;
      let cap = j.max_force;
      if cap > 0.0 {
        let len = force.length();
        if len > cap {
          force = force / len * cap;
        }
      }
      let rb = &mut self.sim.bodies[body];
      rb.add_force(force, true);
    }
  }

  /// Remove joints whose reaction exceeded break limits; record each once.
  pub(crate) fn break_overloaded_joints(
    &mut self,
    world: &mut seal_ecs::World,
    dt: f32,
  ) {
    if dt <= 0.0 {
      return;
    }
    let mut broken = Vec::new();
    for (entity, link) in self.joints.iter() {
      let j = &link.settings;
      if j.break_force <= 0.0 && j.break_torque <= 0.0 {
        continue;
      }
      let (mut lin, mut ang) = match link.handle {
        Some(h) => joint_reaction(&self.sim, h, dt),
        None => estimate_target_load(&self.sim, link.bodies.0, j),
      };
      let est = joint_load_estimate(&self.sim, link.bodies, j);
      lin = lin.max(est.0).max(link.load_lin);
      ang = ang.max(est.1).max(link.load_ang);
      if (j.break_force > 0.0 && lin > j.break_force)
        || (j.break_torque > 0.0 && ang > j.break_torque)
      {
        broken.push(*entity);
      }
    }
    for entity in broken {
      let Some(link) = self.joints.remove(&entity) else { continue };
      if let Some(h) = link.handle {
        self.sim.remove_impulse_joint(h);
      }
      world.remove::<Joint2D>(entity);
      self.joint_breaks.push(JointBreak { entity });
    }
  }
}

fn joint_reaction(sim: &PhysicsWorld, handle: ImpulseJointHandle, dt: f32) -> (f32, f32) {
  let Some(joint) = sim.impulse_joints.get(handle) else {
    return (0.0, 0.0);
  };
  let imp = joint.impulses;
  let scale = 1.0 / dt.max(1e-4);
  let lin = (imp[0] * imp[0] + imp[1] * imp[1]).sqrt() * scale;
  let ang = imp[2].abs() * scale;
  (lin, ang)
}

fn joint_load_estimate(
  sim: &PhysicsWorld,
  bodies: (RigidBodyHandle, RigidBodyHandle),
  j: &Joint2D,
) -> (f32, f32) {
  let p1 = *sim.bodies[bodies.1].position();
  let p2 = *sim.bodies[bodies.0].position();
  let a1 = p1 * Vector::new(j.other_anchor[0], j.other_anchor[1]);
  let a2 = p2 * Vector::new(j.anchor[0], j.anchor[1]);
  let stretch = (a1 - a2).length();
  let target_len = if j.length > 0.0 { j.length } else { stretch };
  let lin = (stretch - target_len).abs() * j.stiffness.max(40.0);
  let ang = (p2.rotation.angle() - p1.rotation.angle() - j.limits[0] * DEG).abs()
    * j.stiffness.max(40.0);
  (lin, ang)
}

fn estimate_target_load(
  sim: &PhysicsWorld,
  body: RigidBodyHandle,
  j: &Joint2D,
) -> (f32, f32) {
  let rb = &sim.bodies[body];
  let pose = rb.position();
  let anchor = pose * Vector::new(j.anchor[0], j.anchor[1]);
  let target = Vector::new(j.target[0], j.target[1]);
  let delta = target - anchor;
  let freq = j.stiffness.max(0.0);
  let lin = (delta * freq).length();
  (lin, 0.0)
}

/// Build a friction joint: resists relative motion up to max force and torque.
pub fn friction_joint(j: &Joint2D, a1: Vector, a2: Vector) -> GenericJoint {
  let lin = j.max_force.max(0.0);
  let ang = j.max_torque.max(0.0);
  let mut b = GenericJointBuilder::new(JointAxesMask::empty())
    .local_anchor1(a1)
    .local_anchor2(a2);
  if lin > 0.0 {
    b = b
      .motor_velocity(JointAxis::LinX, 0.0, 1.0)
      .motor_max_force(JointAxis::LinX, lin)
      .motor_velocity(JointAxis::LinY, 0.0, 1.0)
      .motor_max_force(JointAxis::LinY, lin);
  }
  if ang > 0.0 {
    b = b
      .motor_velocity(JointAxis::AngX, 0.0, 1.0)
      .motor_max_force(JointAxis::AngX, ang);
  }
  b.build()
}

/// Build a relative joint that keeps a linear and angular offset.
pub fn relative_joint(j: &Joint2D, a1: Vector, a2: Vector, p1: Pose, p2: Pose) -> GenericJoint {
  let angle = j.limits[0] * DEG;
  let offset = Vector::new(j.offset[0], j.offset[1]);
  let stiff = (j.correction.max(0.01)) * 80.0;
  let damp = j.damping.max(0.0);
  let lin_cap = if j.max_force > 0.0 { j.max_force } else { f32::INFINITY };
  let ang_cap = if j.max_torque > 0.0 { j.max_torque } else { f32::INFINITY };
  if j.correction >= 0.95 {
    let rel = p2.rotation.angle() - p1.rotation.angle();
    return FixedJointBuilder::new()
      .local_frame1(Pose::new(a1, rel - angle))
      .local_frame2(Pose::new(a2 + offset, 0.0))
      .build()
      .into();
  }
  let mut g: GenericJoint = FixedJointBuilder::new()
    .local_frame1(Pose::new(a1, angle))
    .local_frame2(Pose::new(a2 + offset, 0.0))
    .build()
    .into();
  g.set_motor_position(JointAxis::AngX, angle, stiff, damp);
  g.set_motor_max_force(JointAxis::AngX, ang_cap);
  g.set_motor_position(JointAxis::LinX, 0.0, stiff, damp);
  g.set_motor_max_force(JointAxis::LinX, lin_cap);
  g.set_motor_position(JointAxis::LinY, 0.0, stiff, damp);
  g.set_motor_max_force(JointAxis::LinY, lin_cap);
  g
}