//! Rays, sweeps, points, overlaps and test moves on a known layout.

use seal_ecs::core::{SceneSettings, Transform};
use seal_ecs::World;
use seal_physics2d::*;

fn world() -> World {
    let mut w = World::new();
    w.register_core();
    register_physics2d(&mut w);
    w.insert_resource(SceneSettings { gravity: 0.0, time_scale: 1.0, music: String::new() });
    w
}

fn at(x: f32, y: f32) -> Transform {
    Transform { translation: [x, y, 0.0], ..Default::default() }
}

fn fixed_box(w: &mut World, x: f32, layer: u32) -> seal_ecs::Entity {
    w.spawn((
        at(x, 0.0),
        Body2D::fixed(),
        Collider2D { size: [1.0, 1.0], layer, ..Default::default() },
    ))
}

fn layout() -> (World, Physics2D, seal_ecs::Entity, seal_ecs::Entity, seal_ecs::Entity) {
    let mut w = world();
    let a = fixed_box(&mut w, 0.0, 1);
    let c = fixed_box(&mut w, 1.5, 2);
    let b = fixed_box(&mut w, 3.0, 1);
    let mut p = Physics2D::new();
    p.step(&mut w, 1.0 / 60.0).unwrap();
    (w, p, a, c, b)
}

#[test]
fn raycast_all_is_sorted_and_masked() {
    let (_w, p, a, c, b) = layout();
    let hits = p.raycast_all([-2.0, 0.0], [1.0, 0.0], 10.0, 0);
    let ids: Vec<_> = hits.iter().map(|h| h.entity).collect();
    assert_eq!(ids, vec![a, c, b]);
    assert!((hits[0].distance - 1.5).abs() < 1e-3, "{}", hits[0].distance);
    assert!((hits[0].point[0] + 0.5).abs() < 1e-3);
    let masked = p.raycast_all([-2.0, 0.0], [1.0, 0.0], 10.0, 1);
    let ids: Vec<_> = masked.iter().map(|h| h.entity).collect();
    assert_eq!(ids, vec![a, b]);
    assert!(p.raycast_all([0.0, 0.0], [0.0, 0.0], 5.0, 0).is_empty());
}

#[test]
fn overlap_all_lists_a_shape_and_honours_the_mask() {
    let (_w, p, a, c, b) = layout();
    let wide = p.overlap_all("box", [4.0, 1.0], &[], [0.0, 0.0], 0.0, 0);
    assert_eq!(wide, vec![a, c]);
    let masked = p.overlap_all("box", [8.0, 2.0], &[], [1.5, 0.0], 0.0, 1);
    assert_eq!(masked, vec![a, b]);
    let cap = p.overlap_all("capsule", [0.4, 2.0], &[], [0.0, 0.0], 0.0, 0);
    assert_eq!(cap, vec![a]);
    let poly = [[-0.2, -0.2], [0.2, -0.2], [0.2, 0.2], [-0.2, 0.2]];
    assert_eq!(p.overlap_all("polygon", [1.0, 1.0], &poly, [3.0, 0.0], 0.0, 0), vec![b]);
    assert!(p.overlap_all("triangle", [1.0, 1.0], &[], [0.0, 0.0], 0.0, 0).is_empty());
    assert!(p.overlap_all("polygon", [1.0, 1.0], &[[0.0, 0.0], [1.0, 0.0]], [0.0, 0.0], 0.0, 0).is_empty());
}

#[test]
fn shape_point_and_overlap_queries_match_the_layout() {
    let (_w, p, a, c, b) = layout();
    let cast = p.shape_cast("circle", [0.5, 0.5], [-2.0, 0.0], [1.0, 0.0], 10.0, 0).unwrap();
    assert_eq!(cast.entity, a);
    assert!((cast.distance - 1.25).abs() < 1e-2, "{}", cast.distance);
    assert!(p.shape_cast("triangle", [1.0, 1.0], [0.0, 0.0], [1.0, 0.0], 4.0, 0).is_none());
    assert_eq!(p.point_query([0.0, 0.0], 0), vec![a]);
    assert!(p.point_query([10.0, 10.0], 0).is_empty());
    assert_eq!(p.overlap_box([0.0, 0.0], [4.5, 4.0], 0), vec![a, c]);
    assert_eq!(p.overlap_circle([3.0, 0.0], 0.6, 0), vec![b]);
}

#[test]
fn test_move_reports_the_body_in_the_way() {
    let mut w = world();
    let mover = w.spawn((
        at(0.0, 0.0),
        Body2D { kind: "kinematic".into(), ..Default::default() },
        Collider2D { size: [1.0, 1.0], ..Default::default() },
    ));
    let wall = fixed_box(&mut w, 1.5, 1);
    let mut p = Physics2D::new();
    p.step(&mut w, 1.0 / 60.0).unwrap();
    let hit = p.test_move(mover, [1.2, 0.0], 0).unwrap();
    assert_eq!(hit.entity, wall);
    assert!(hit.distance > 0.4 && hit.distance < 0.6, "{}", hit.distance);
    assert!(p.test_move(mover, [0.2, 0.0], 0).is_none());
}

#[test]
fn a_child_collider_adds_mass_and_blocks_with_its_parent() {
    let mut w = world();
    w.resource_mut::<SceneSettings>().unwrap().gravity = 30.0;
    let parent = w.spawn((
        at(0.0, 3.0),
        Body2D { lock_rotation: true, ..Default::default() },
        Collider2D { size: [0.2, 0.2], ..Default::default() },
        Velocity2D::default(),
    ));
    let child = w.spawn((
        Transform { translation: [2.0, 0.0, 0.0], ..Default::default() },
        Collider2D { size: [1.0, 1.0], ..Default::default() },
    ));
    w.set_parent(child, Some(parent)).unwrap();
    let _floor = w.spawn((
        at(2.0, 0.0),
        Body2D::fixed(),
        Collider2D { size: [1.0, 1.0], ..Default::default() },
    ));
    let mut p = Physics2D::new();
    p.step(&mut w, 1.0 / 60.0).unwrap();
    let mass = {
        let (body, _) = p.handles(parent).unwrap();
        p.rapier().bodies[body].mass()
    };
    assert!(mass > 0.9, "the child collider counts toward the mass, mass={mass}");
    let mut named_parent = false;
    let mut named_child = false;
    for _ in 0..180 {
        p.step(&mut w, 1.0 / 60.0).unwrap();
        for contact in &p.contacts {
            if contact.a == parent || contact.b == parent {
                named_parent = true;
            }
            if contact.a == child || contact.b == child {
                named_child = true;
            }
        }
    }
    let y = w.get::<Transform>(parent).unwrap().translation[1];
    assert!(y > 0.4 && y < 1.8, "the offset child held the parent up, y={y}");
    assert!(named_parent, "contacts name the parent");
    assert!(!named_child, "contacts do not name the child collider");
}
