use rapier3d::prelude::*;

use crate::{Entity, Shape};

/// Rigid-body state for one play session. Dropped when play stops.
pub struct Sim {
    pipeline: PhysicsPipeline,
    integration: IntegrationParameters,
    islands: IslandManager,
    broad_phase: BroadPhaseBvh,
    narrow_phase: NarrowPhase,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    soft_bodies: SoftBodySet,
    ccd: CCDSolver,
    handles: Vec<(String, RigidBodyHandle)>,
}

impl Sim {
    pub fn from_entities(entities: &[Entity]) -> Self {
        Self::from_parts(entities, &[])
    }

    /// Like [`from_entities`], but `solids` replace the shape collider for those object ids.
    pub fn from_parts(entities: &[Entity], solids: &[(String, crate::physics::Geom)]) -> Self {
        let mut bodies = RigidBodySet::new();
        let mut colliders = ColliderSet::new();
        let mut handles = Vec::new();
        for entity in entities {
            if entity.hidden {
                continue;
            }
            let solid = solids.iter().find(|(id, _)| id == &entity.id).map(|(_, geom)| geom);
            let Some(collider) =
                solid.and_then(collider_from_geom).or_else(|| collider_for(entity))
            else {
                continue;
            };
            let origin =
                Vector::new(entity.translation[0], entity.translation[1], entity.translation[2]);
            let velocity = Vector::new(entity.velocity[0], entity.velocity[1], entity.velocity[2]);
            let turn = Vector::new(
                entity.rotation[0].to_radians(),
                entity.rotation[1].to_radians(),
                entity.rotation[2].to_radians(),
            );
            let body = if entity.dynamic {
                RigidBodyBuilder::dynamic()
                    .translation(origin)
                    .rotation(turn)
                    .linvel(velocity)
                    .build()
            } else {
                RigidBodyBuilder::fixed().translation(origin).rotation(turn).build()
            };
            let handle = bodies.insert(body);
            colliders.insert_with_parent(collider, handle, &mut bodies);
            handles.push((entity.id.clone(), handle));
        }
        Self {
            pipeline: PhysicsPipeline::new(),
            integration: IntegrationParameters::default(),
            islands: IslandManager::new(),
            broad_phase: BroadPhaseBvh::new(),
            narrow_phase: NarrowPhase::new(),
            bodies,
            colliders,
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            soft_bodies: SoftBodySet::new(),
            ccd: CCDSolver::new(),
            handles,
        }
    }

    /// First object hit by a ray, as (entity id, distance); None when nothing is in reach.
    pub fn ray_cast(&self, from: [f32; 3], dir: [f32; 3], max: f32) -> Option<(String, f32)> {
        let d = Vector::new(dir[0], dir[1], dir[2]);
        if d.length() < 1e-6 {
            return None;
        }
        let ray = Ray::new(Vector::new(from[0], from[1], from[2]), d.normalize());
        let mut best: Option<(String, f32)> = None;
        for (id, handle) in &self.handles {
            for c in self.bodies[*handle].colliders() {
                let Some(collider) = self.colliders.get(*c) else { continue };
                if let Some(t) = collider.shape().cast_ray(collider.position(), &ray, max, true) {
                    if best.as_ref().is_none_or(|b| t < b.1) {
                        best = Some((id.clone(), t));
                    }
                }
            }
        }
        best
    }

    /// Nearest hit of a ray against every body but `skip`: (distance, surface normal).
    fn cast_skipping(
        &self,
        skip: &str,
        from: Vector,
        dir: Vector,
        max: f32,
    ) -> Option<(f32, Vector)> {
        let ray = Ray::new(from, dir.normalize());
        let mut best: Option<(f32, Vector)> = None;
        for (id, handle) in &self.handles {
            if id == skip {
                continue;
            }
            for c in self.bodies[*handle].colliders() {
                let Some(collider) = self.colliders.get(*c) else { continue };
                if let Some(hit) =
                    collider.shape().cast_ray_and_get_normal(collider.position(), &ray, max, true)
                {
                    if best.is_none_or(|b| hit.time_of_impact < b.0) {
                        best = Some((hit.time_of_impact, hit.normal));
                    }
                }
            }
        }
        best
    }

    /// Walk, jump and climb for a capsule character during play. `wish` is the wanted
    /// ground velocity (x, z); returns whether the character stands on something.
    pub fn character_step(
        &mut self,
        id: &str,
        wish: [f32; 2],
        jump: bool,
        rules: &CharacterRules,
    ) -> bool {
        let Some((_, h)) = self.handles.iter().find(|(i, _)| i == id).cloned() else {
            return false;
        };
        let (half, radius, pos, vel) = {
            let body = &mut self.bodies[h];
            body.lock_rotations(true, true);
            let aabb = body
                .colliders()
                .iter()
                .filter_map(|c| self.colliders.get(*c))
                .map(|c| c.compute_aabb())
                .next();
            let (half, radius) = match aabb {
                Some(a) => ((a.maxs.y - a.mins.y) * 0.5, (a.maxs.x - a.mins.x) * 0.5),
                None => (0.5, 0.3),
            };
            (half, radius, body.translation(), body.linvel())
        };
        let ground =
            self.cast_skipping(id, pos, Vector::new(0.0, -1.0, 0.0), half + rules.ground_reach);
        // Rising faster than a walk means a jump is under way: not standing any more.
        let grounded =
            vel.y <= 0.5 && ground.is_some_and(|g| g.1.y >= rules.max_slope_deg.to_radians().cos());
        let mut v = vel;
        let mut want = Vector::new(wish[0], 0.0, wish[1]);
        if grounded {
            let n = ground.map_or(Vector::new(0.0, 1.0, 0.0), |g| g.1);
            // Keep the push along the slope so walking uphill is not slowed to a crawl.
            if want.length() > 1e-4 {
                want -= n * want.dot(n) / n.dot(n).max(1e-6);
                want = want.normalize() * Vector::new(wish[0], 0.0, wish[1]).length();
            }
            v.x = want.x;
            v.z = want.z;
            if want.length() < 1e-4 {
                v.y = v.y.clamp(-1.0, 0.0);
            } else {
                v.y = want.y;
            }
            if jump {
                v.y = rules.jump_speed;
            }
        } else {
            // In the air the push steers, but never beyond the walking speed.
            v.x += (want.x - v.x) * rules.air_control;
            v.z += (want.z - v.z) * rules.air_control;
        }
        // Step up a low ledge in the way of the walk.
        let flat = Vector::new(wish[0], 0.0, wish[1]);
        if grounded && flat.length() > 1e-4 {
            let dir = flat.normalize();
            let feet = pos.y - half;
            let low = Vector::new(pos.x, feet + 0.05, pos.z);
            let high = Vector::new(pos.x, feet + rules.step_height + 0.05, pos.z);
            let reach = radius + 0.15;
            if self.cast_skipping(id, low, dir, reach).is_some()
                && self.cast_skipping(id, high, dir, reach).is_none()
            {
                self.bodies[h].set_translation(
                    pos + Vector::new(0.0, rules.step_height + 0.02, 0.0) + dir * 0.05,
                    true,
                );
            }
        }
        self.bodies[h].set_linvel(v, true);
        grounded
    }

    /// Where a body is and how fast it moves.
    pub fn pose(&self, id: &str) -> Option<([f32; 3], [f32; 3])> {
        let (_, h) = self.handles.iter().find(|(i, _)| i == id)?;
        let b = &self.bodies[*h];
        let (p, v) = (b.translation(), b.linvel());
        Some(([p.x, p.y, p.z], [v.x, v.y, v.z]))
    }

    /// Instant push on a dynamic object's body (mass-scaled speed change).
    pub fn apply_impulse(&mut self, id: &str, impulse: [f32; 3]) {
        if let Some((_, h)) = self.handles.iter().find(|(i, _)| i == id) {
            let body = &mut self.bodies[*h];
            if body.is_dynamic() {
                body.apply_impulse(Vector::new(impulse[0], impulse[1], impulse[2]), true);
            }
        }
    }
    pub fn step(&mut self, dt: f32, gravity: f32, entities: &mut [Entity]) {
        self.integration.dt = dt.clamp(0.0, 0.05);
        // Scripts may have moved objects or changed their speed since the last
        // step; carry that into the bodies so the step starts from it.
        for (id, handle) in &self.handles {
            let Some(entity) = entities.iter().find(|entity| entity.id == *id) else {
                continue;
            };
            let body = &mut self.bodies[*handle];
            let at =
                Vector::new(entity.translation[0], entity.translation[1], entity.translation[2]);
            if (body.translation() - at).length() > 1e-5 {
                body.set_translation(at, true);
            }
            if body.is_dynamic() {
                let v = Vector::new(entity.velocity[0], entity.velocity[1], entity.velocity[2]);
                if (body.linvel() - v).length() > 1e-5 {
                    body.set_linvel(v, true);
                }
            }
        }
        self.pipeline.step(
            Vector::new(0.0, -gravity, 0.0),
            &self.integration,
            &mut self.islands,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.soft_bodies,
            &mut self.ccd,
            &(),
            &(),
        );
        for (id, handle) in &self.handles {
            let body = &self.bodies[*handle];
            if !body.is_dynamic() {
                continue;
            }
            let Some(entity) = entities.iter_mut().find(|entity| entity.id == *id) else {
                continue;
            };
            let position = body.translation();
            let velocity = body.linvel();
            entity.translation = [position.x, position.y, position.z];
            entity.velocity = [velocity.x, velocity.y, velocity.z];
        }
    }
}

/// How a walking character moves.
#[derive(Clone, Debug)]
pub struct CharacterRules {
    pub jump_speed: f32,
    pub max_slope_deg: f32,
    pub step_height: f32,
    /// Extra distance below the feet that still counts as standing.
    pub ground_reach: f32,
    /// 0 to 1: how much of the wanted speed is reached per call while airborne.
    pub air_control: f32,
}

impl Default for CharacterRules {
    fn default() -> Self {
        Self {
            jump_speed: 6.0,
            max_slope_deg: 50.0,
            step_height: 0.4,
            ground_reach: 0.1,
            air_control: 0.1,
        }
    }
}

fn collider_from_geom(geom: &crate::physics::Geom) -> Option<Collider> {
    use crate::physics::Geom;
    let builder = match geom {
        Geom::Cylinder { radius, half_height } => {
            ColliderBuilder::cylinder(half_height.max(0.0), radius.max(0.0))
        }
        Geom::Mesh { vertices, indices } => {
            let points = vertices.iter().map(|p| Vector::new(p[0], p[1], p[2])).collect();
            ColliderBuilder::trimesh(points, indices.clone()).ok()?
        }
        Geom::Heightfield { columns, rows, heights, size } => {
            if *columns < 2 || *rows < 2 || heights.len() < columns * rows {
                return None;
            }
            let mut data = vec![0.0; rows * columns];
            for r in 0..*rows {
                for c in 0..*columns {
                    data[r + c * *rows] = heights[r * *columns + c];
                }
            }
            let grid = rapier3d::parry::utils::Array2::new(*rows, *columns, data);
            ColliderBuilder::heightfield(
                grid,
                Vector::new(size[0].max(0.05), 1.0, size[2].max(0.05)),
            )
        }
        Geom::Hulls { parts } => {
            let mut shapes = Vec::new();
            for hull in parts {
                let points: Vec<Vector> =
                    hull.points.iter().map(|p| Vector::new(p[0], p[1], p[2])).collect();
                if let Some(shape) = SharedShape::convex_hull(&points) {
                    shapes.push((Pose::identity(), shape));
                }
            }
            if shapes.len() == 1 {
                ColliderBuilder::new(shapes.pop()?.1)
            } else if shapes.is_empty() {
                return None;
            } else {
                ColliderBuilder::compound(shapes)
            }
        }
    };
    Some(builder.build())
}

fn collider_for(entity: &Entity) -> Option<Collider> {
    let scale = entity.scale.map(|value| value.abs().max(0.05));
    let builder = match entity.shape {
        Some(Shape::Cube) => {
            ColliderBuilder::cuboid(0.5 * scale[0], 0.5 * scale[1], 0.5 * scale[2])
        }
        Some(Shape::Sphere) => ColliderBuilder::ball(0.5 * scale[0].max(scale[1]).max(scale[2])),
        Some(Shape::Plane) => {
            ColliderBuilder::cuboid(20.0 * scale[0], 0.05 * scale[1], 20.0 * scale[2])
        }
        Some(Shape::Cylinder) => {
            ColliderBuilder::cylinder(0.5 * scale[1], 0.5 * scale[0].max(scale[2]))
        }
        Some(Shape::Cone) => ColliderBuilder::cone(0.5 * scale[1], 0.5 * scale[0].max(scale[2])),
        Some(Shape::Capsule) => {
            ColliderBuilder::capsule_y(0.35 * scale[1], 0.35 * scale[0].max(scale[2]))
        }
        None if entity.dynamic => ColliderBuilder::ball(0.5),
        None => return None,
    };
    Some(
        builder
            .friction(entity.friction.max(0.0))
            .restitution(entity.restitution.clamp(0.0, 1.0))
            .build(),
    )
}
