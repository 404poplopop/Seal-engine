//! Scene queries against the live simulation: every hit on a ray, a swept
//! shape, points and overlaps, and whether a body can move.

use rapier2d::parry::query::ShapeCastOptions;
use rapier2d::prelude::*;
use seal_ecs::Entity;

use super::Physics2D;

/// One contact from a ray, a sweep, or a test move.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QueryHit {
    pub entity: Entity,
    pub distance: f32,
    pub point: [f32; 2],
    pub normal: [f32; 2],
}

fn groups(mask: i64) -> QueryFilter<'static> {
    let filter = QueryFilter::default();
    if mask <= 0 {
        return filter;
    }
    let bits = if mask > u32::MAX as i64 { u32::MAX } else { mask as u32 };
    filter.groups(InteractionGroups::new(
        Group::ALL,
        Group::from_bits_retain(bits),
        InteractionTestMode::And,
    ))
}

fn entity_of(collider: &Collider) -> Entity {
    Entity::from_bits(collider.user_data as u64)
}

impl Physics2D {
    /// Every collider the ray meets, nearest first. `mask` 0 or less is every layer.
    pub fn raycast_all(&self, origin: [f32; 2], dir: [f32; 2], max_distance: f32, mask: i64) -> Vec<QueryHit> {
        let d = Vector::new(dir[0], dir[1]);
        let len = d.length();
        if len == 0.0 || max_distance < 0.0 {
            return Vec::new();
        }
        let ray = Ray::new(Vector::new(origin[0], origin[1]), d / len);
        let mut hits: Vec<QueryHit> = self
            .sim
            .intersect_ray(ray, max_distance, true, groups(mask))
            .map(|(_, collider, hit)| {
                let point = ray.point_at(hit.time_of_impact);
                QueryHit {
                    entity: entity_of(collider),
                    distance: hit.time_of_impact,
                    point: [point.x, point.y],
                    normal: [hit.normal.x, hit.normal.y],
                }
            })
            .collect();
        hits.sort_by(|a, b| {
            a.distance.total_cmp(&b.distance).then(a.entity.to_bits().cmp(&b.entity.to_bits()))
        });
        hits
    }

    /// First hit of a circle (`shape` = `circle`, `size.x` is the diameter) or a
    /// box (`size` is the full width and height) swept along `dir`.
    pub fn shape_cast(
        &self,
        shape: &str,
        size: [f32; 2],
        from: [f32; 2],
        dir: [f32; 2],
        max_distance: f32,
        mask: i64,
    ) -> Option<QueryHit> {
        let d = Vector::new(dir[0], dir[1]);
        let len = d.length();
        if len == 0.0 || max_distance < 0.0 {
            return None;
        }
        let unit = d / len;
        let cast: SharedShape = if shape == "circle" {
            SharedShape::ball((size[0].abs() * 0.5).max(0.0))
        } else if shape == "box" {
            SharedShape::cuboid((size[0].abs() * 0.5).max(0.0), (size[1].abs() * 0.5).max(0.0))
        } else {
            return None;
        };
        let pose = Pose::new(Vector::new(from[0], from[1]), 0.0);
        let options = ShapeCastOptions {
            max_time_of_impact: max_distance,
            ..ShapeCastOptions::default()
        };
        let (handle, hit) =
            self.sim.cast_shape(&pose, unit, cast.as_ref(), options, groups(mask).exclude_sensors())?;
        let collider = self.sim.colliders.get(handle)?;
        Some(QueryHit {
            entity: entity_of(collider),
            distance: hit.time_of_impact,
            point: [hit.witness1.x, hit.witness1.y],
            normal: [hit.normal1.x, hit.normal1.y],
        })
    }

    /// Entities whose colliders contain `at`, lowest id first.
    pub fn point_query(&self, at: [f32; 2], mask: i64) -> Vec<Entity> {
        let mut ids: Vec<Entity> = self
            .sim
            .intersect_point(Vector::new(at[0], at[1]), groups(mask))
            .map(|(_, collider)| entity_of(collider))
            .collect();
        ids.sort_by_key(|e| e.to_bits());
        ids.dedup();
        ids
    }

    /// Entities overlapping an axis-aligned box. `size` is the full width and height.
    pub fn overlap_box(&self, center: [f32; 2], size: [f32; 2], mask: i64) -> Vec<Entity> {
        let shape = Cuboid::new(Vector::new(size[0].abs() * 0.5, size[1].abs() * 0.5));
        self.overlap_shape(center, &shape, mask)
    }

    /// Entities overlapping a circle.
    pub fn overlap_circle(&self, center: [f32; 2], radius: f32, mask: i64) -> Vec<Entity> {
        let shape = Ball::new(radius.abs());
        self.overlap_shape(center, &shape, mask)
    }

    fn overlap_shape(&self, center: [f32; 2], shape: &dyn Shape, mask: i64) -> Vec<Entity> {
        self.overlap_posed(Pose::new(Vector::new(center[0], center[1]), 0.0), shape, mask)
    }

    fn overlap_posed(&self, pose: Pose, shape: &dyn Shape, mask: i64) -> Vec<Entity> {
        let mut ids: Vec<Entity> = self
            .sim
            .intersect_shape(pose, shape, groups(mask))
            .map(|(_, collider)| entity_of(collider))
            .collect();
        ids.sort_by_key(|e| e.to_bits());
        ids.dedup();
        ids
    }

    /// Every collider overlapping `shape` at `center`, turned by `angle` radians.
    /// `size` is the full width and height. A circle uses the width as its diameter.
    /// `points` are polygon corners in local units. An unknown shape, or a polygon
    /// with fewer than three points, returns nothing. Lowest id first.
    pub fn overlap_all(
        &self,
        shape: &str,
        size: [f32; 2],
        points: &[[f32; 2]],
        center: [f32; 2],
        angle: f32,
        mask: i64,
    ) -> Vec<Entity> {
        let shared = match shape {
            "box" => SharedShape::cuboid(size[0].abs() * 0.5, size[1].abs() * 0.5),
            "circle" => SharedShape::ball(size[0].abs() * 0.5),
            "capsule" => {
                let radius = size[0].abs() * 0.5;
                SharedShape::capsule_y((size[1].abs() * 0.5 - radius).max(0.0), radius)
            }
            "polygon" => {
                if points.len() < 3 {
                    return Vec::new();
                }
                let pts: Vec<Vector> = points.iter().map(|p| Vector::new(p[0], p[1])).collect();
                match SharedShape::convex_hull(&pts) {
                    Some(shape) => shape,
                    None => return Vec::new(),
                }
            }
            _ => return Vec::new(),
        };
        let pose = Pose::new(Vector::new(center[0], center[1]), angle);
        self.overlap_posed(pose, shared.as_ref(), mask)
    }

    /// Whether `entity` would hit another solid if it moved by `delta`.
    /// The body's own colliders are ignored. Sensors are ignored.
    pub fn test_move(&self, entity: Entity, delta: [f32; 2], mask: i64) -> Option<QueryHit> {
        let link = self.links.get(&entity)?;
        let vel = Vector::new(delta[0], delta[1]);
        if vel.length() < 1e-8 {
            return None;
        }
        let handles = self.sim.bodies.get(link.body)?.colliders().to_vec();
        let filter = groups(mask).exclude_sensors().exclude_rigid_body(link.body);
        let options = ShapeCastOptions { max_time_of_impact: 1.0, ..ShapeCastOptions::default() };
        let mut best: Option<QueryHit> = None;
        for handle in handles {
            let (pose, shape) = {
                let Some(collider) = self.sim.colliders.get(handle) else { continue };
                (*collider.position(), collider.shared_shape().clone())
            };
            let Some((hit_handle, hit)) = self.sim.cast_shape(&pose, vel, shape.as_ref(), options, filter) else {
                continue;
            };
            let Some(other) = self.sim.colliders.get(hit_handle) else { continue };
            let found = QueryHit {
                entity: entity_of(other),
                distance: hit.time_of_impact * vel.length(),
                point: [hit.witness1.x, hit.witness1.y],
                normal: [hit.normal1.x, hit.normal1.y],
            };
            if best.is_none_or(|b| found.distance < b.distance) {
                best = Some(found);
            }
        }
        best
    }
}
