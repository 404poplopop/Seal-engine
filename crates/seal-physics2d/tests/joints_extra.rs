//! Friction, relative and target joints, break limits, and scene text.

use seal_ecs::core::Transform;
use seal_ecs::World;
use seal_physics2d::{register_physics2d, Body2D, Collider2D, Joint2D, Physics2D};

fn world() -> World {
    let mut w = World::new();
    w.register_core();
    register_physics2d(&mut w);
    w
}

fn at(x: f32, y: f32) -> Transform {
    Transform { translation: [x, y, 0.0], ..Default::default() }
}

fn run(w: &mut World, p: &mut Physics2D, steps: usize) {
    for _ in 0..steps {
        let _ = p.step(w, 1.0 / 60.0);
    }
}

fn x_of(w: &World, e: seal_ecs::Entity) -> f32 {
    w.get::<Transform>(e).unwrap().translation[0]
}

#[test]
fn friction_joint_resists_pull() {
    let mut w = world();
    let wall = w.spawn((at(0.0, 0.0), Body2D::fixed(), Collider2D::default()));
    let sled = w.spawn((at(1.0, 0.0), Body2D::default(), Collider2D::default()));
    w.insert(
        sled,
        Joint2D {
            kind: "friction".into(),
            other: Some(wall),
            max_force: 200.0,
            max_torque: 50.0,
            ..Default::default()
        },
    );
    let mut p = Physics2D::new();
    run(&mut w, &mut p, 5);
    p.apply_force(sled, [80.0, 0.0]);
    run(&mut w, &mut p, 15);
    assert!(x_of(&w, sled) < 1.2, "friction holds against a light push: {}", x_of(&w, sled));
}

#[test]
fn relative_joint_keeps_offset() {
    let mut w = world();
    let base = w.spawn((at(0.0, 0.0), Body2D::fixed(), Collider2D::default()));
    let arm = w.spawn((at(2.0, 0.0), Body2D::default(), Collider2D::default()));
    w.insert(
        arm,
        Joint2D {
            kind: "relative".into(),
            other: Some(base),
            limits: [0.0, 0.0],
            offset: [0.0, 0.0],
            correction: 1.0,
            anchor: [-2.0, 0.0],
            ..Default::default()
        },
    );
    let mut p = Physics2D::new();
    run(&mut w, &mut p, 120);
    let t = w.get::<Transform>(arm).unwrap().translation;
    assert!((t[0] - 2.0).abs() < 0.08, "keeps spacing: {:?}", t);
}

#[test]
fn target_joint_pulls_toward_point() {
    let mut w = world();
    let bob = w.spawn((at(0.0, 0.0), Body2D::default(), Collider2D::default()));
    w.insert(
        bob,
        Joint2D {
            kind: "target".into(),
            target: [4.0, 0.0],
            stiffness: 120.0,
            damping: 8.0,
            max_force: 500.0,
            ..Default::default()
        },
    );
    let mut p = Physics2D::new();
    run(&mut w, &mut p, 180);
    let x = x_of(&w, bob);
    assert!((x - 4.0).abs() < 0.2, "pulled toward target, x={x}");
}

#[test]
fn break_force_removes_joint_and_fires_once() {
    let mut w = world();
    let anchor = w.spawn((at(0.0, 5.0), Body2D::fixed(), Collider2D::default()));
    let weight = w.spawn((at(0.0, 3.0), Body2D::default(), Collider2D::default()));
    w.insert(
        weight,
        Joint2D {
            kind: "distance".into(),
            other: Some(anchor),
            other_anchor: [0.0, 5.0],
            break_force: 2.0,
            ..Default::default()
        },
    );
    let mut p = Physics2D::new();
    run(&mut w, &mut p, 30);
    assert!(p.joint_count() >= 1);
    let mut broke_at = None;
    for i in 0..60 {
        p.apply_force(weight, [0.0, -200_000.0]);
        let _ = p.step(&mut w, 1.0 / 60.0);
        if p.joint_count() == 0 {
            broke_at = Some(i);
            assert_eq!(p.joint_breaks.len(), 1, "break recorded on the step it snapped");
            break;
        }
    }
    assert!(broke_at.is_some(), "joint broke under load");
    for _ in 0..20 {
        p.apply_force(weight, [0.0, -200_000.0]);
        let _ = p.step(&mut w, 1.0 / 60.0);
        assert!(p.joint_breaks.is_empty(), "no second break record");
    }
}

#[test]
fn joint2d_extra_fields_default_and_clone() {
    let j = Joint2D {
        kind: "friction".into(),
        break_force: 10.0,
        max_force: 40.0,
        target: [2.0, 3.0],
        ..Default::default()
    };
    let j2 = j.clone();
    assert_eq!(j.break_force, j2.break_force);
    assert_eq!(j.target, j2.target);
}
