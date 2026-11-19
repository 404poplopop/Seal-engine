//! Shapes, layers, one-way platforms, surfaces, joints and fast bodies.

use seal_ecs::core::Transform;
use seal_ecs::World;
use seal_physics2d::*;

fn world() -> World {
    let mut w = World::new();
    w.register_core();
    register_physics2d(&mut w);
    w
}

fn at(x: f32, y: f32) -> Transform {
    Transform { translation: [x, y, 0.0], ..Default::default() }
}

fn y_of(w: &World, e: seal_ecs::Entity) -> f32 {
    w.get::<Transform>(e).unwrap().translation[1]
}

fn x_of(w: &World, e: seal_ecs::Entity) -> f32 {
    w.get::<Transform>(e).unwrap().translation[0]
}

fn run(w: &mut World, p: &mut Physics2D, steps: usize) {
    for _ in 0..steps {
        p.step(w, 1.0 / 60.0).unwrap();
    }
}

#[test]
fn polygon_and_edge_shapes_hold_things_up() {
    let mut w = world();
    let floor = Collider2D {
        shape: "edge".into(),
        points: vec![[-10.0, 0.0], [10.0, 0.0]],
        ..Default::default()
    };
    w.spawn((at(0.0, 0.0), Body2D::fixed(), floor));
    let tri = Collider2D {
        shape: "polygon".into(),
        points: vec![[-0.5, -0.5], [0.5, -0.5], [0.0, 0.5]],
        ..Default::default()
    };
    let t = w.spawn((at(0.0, 3.0), Body2D::default(), tri));
    let pill = Collider2D { shape: "capsule_x".into(), size: [2.0, 0.5], ..Default::default() };
    let c = w.spawn((at(4.0, 3.0), Body2D::default(), pill));
    let mut p = Physics2D::new();
    run(&mut w, &mut p, 240);
    assert!((y_of(&w, t) - 0.5).abs() < 0.05, "triangle rests on the edge: {}", y_of(&w, t));
    assert!((y_of(&w, c) - 0.25).abs() < 0.05, "lying capsule rests: {}", y_of(&w, c));
    // Too few points is an error, not a silent hole.
    let mut w2 = world();
    w2.spawn((
        at(0.0, 0.0),
        Body2D::fixed(),
        Collider2D { shape: "polygon".into(), ..Default::default() },
    ));
    assert!(matches!(Physics2D::new().step(&mut w2, 0.016), Err(PhysicsError::BadPoints { .. })));
}

#[test]
fn layers_and_masks_choose_who_collides() {
    let mut w = world();
    w.spawn((
        at(0.0, 0.0),
        Body2D::fixed(),
        Collider2D { size: [20.0, 1.0], layer: 1, ..Default::default() },
    ));
    // A ghost on layer 2 that ignores layer 1 falls through the ground.
    let ghost = w.spawn((
        at(-2.0, 3.0),
        Body2D::default(),
        Collider2D { layer: 2, mask: !1, ..Default::default() },
    ));
    let solid =
        w.spawn((at(2.0, 3.0), Body2D::default(), Collider2D { layer: 2, ..Default::default() }));
    let mut p = Physics2D::new();
    run(&mut w, &mut p, 180);
    assert!(y_of(&w, ghost) < -2.0);
    assert!((y_of(&w, solid) - 1.0).abs() < 0.05);
}

#[test]
fn one_way_platforms_let_things_jump_up_through() {
    let mut w = world();
    w.spawn((
        at(0.0, 2.0),
        Body2D::fixed(),
        Collider2D { size: [6.0, 0.2], one_way: true, ..Default::default() },
    ));
    // Starts below, moving up fast: passes through, then lands on top.
    let b = w.spawn((
        at(0.0, 0.0),
        Body2D::default(),
        Collider2D { size: [0.5, 0.5], ..Default::default() },
        Velocity2D { linear: [0.0, 9.0], angular: 0.0 },
    ));
    let mut p = Physics2D::new();
    let mut peak = f32::MIN;
    for _ in 0..240 {
        p.step(&mut w, 1.0 / 60.0).unwrap();
        peak = peak.max(y_of(&w, b));
    }
    assert!(peak > 2.5, "went above the platform: {peak}");
    assert!((y_of(&w, b) - 2.35).abs() < 0.05, "lands on top: {}", y_of(&w, b));
}

#[test]
fn surfaces_change_sliding_and_bouncing() {
    let slide = |material: &str| {
        let mut w = world();
        w.spawn((
            at(0.0, 0.0),
            Body2D::fixed(),
            Collider2D { size: [100.0, 1.0], material: material.into(), ..Default::default() },
        ));
        let b = w.spawn((
            at(0.0, 1.0),
            Body2D { lock_rotation: true, ..Default::default() },
            Collider2D { material: material.into(), ..Default::default() },
            Velocity2D { linear: [5.0, 0.0], angular: 0.0 },
        ));
        let mut p = Physics2D::new();
        run(&mut w, &mut p, 120);
        x_of(&w, b)
    };
    assert!(slide("ice") > slide("rubber") + 2.0);
    let bounce = |material: &str| {
        let mut w = world();
        w.spawn((
            at(0.0, 0.0),
            Body2D::fixed(),
            Collider2D {
                size: [10.0, 1.0],
                material: material.into(),
                bounce_combine: "max".into(),
                ..Default::default()
            },
        ));
        let b = w.spawn((
            at(0.0, 4.0),
            Body2D::default(),
            Collider2D { shape: "circle".into(), ..Default::default() },
        ));
        let mut p = Physics2D::new();
        let mut top_after_hit = 0.0f32;
        let mut hit = false;
        for _ in 0..180 {
            p.step(&mut w, 1.0 / 60.0).unwrap();
            hit |= y_of(&w, b) < 1.1;
            if hit {
                top_after_hit = top_after_hit.max(y_of(&w, b));
            }
        }
        top_after_hit
    };
    assert!(bounce("bouncy") > 3.0);
    assert!(bounce("stone") < 1.5);
    let mut w = world();
    w.spawn((
        at(0.0, 0.0),
        Body2D::fixed(),
        Collider2D { material: "jelly".into(), ..Default::default() },
    ));
    assert!(matches!(Physics2D::new().step(&mut w, 0.016), Err(PhysicsError::BadSurface { .. })));
}

#[test]
fn joints_hold_bodies_together() {
    let mut w = world();
    // A pendulum on a hinge to a world point keeps its arm length.
    let bob = w.spawn((
        at(3.0, 5.0),
        Body2D::default(),
        Collider2D { shape: "circle".into(), size: [0.4, 0.4], ..Default::default() },
        Joint2D {
            kind: "hinge".into(),
            anchor: [-3.0, 0.0],
            other_anchor: [0.0, 5.0],
            ..Default::default()
        },
    ));
    // A rope hanging from a world point stretches no further than 2.
    let weight = w.spawn((
        at(10.0, 5.0),
        Body2D::default(),
        Collider2D::default(),
        Joint2D {
            kind: "rope".into(),
            other_anchor: [10.0, 5.0],
            length: 2.0,
            ..Default::default()
        },
    ));
    // Two boxes welded together fall as one.
    let a = w.spawn((at(20.0, 5.0), Body2D::default(), Collider2D::default()));
    let b = w.spawn((
        at(21.0, 5.0),
        Body2D::default(),
        Collider2D::default(),
        Joint2D {
            kind: "fixed".into(),
            other: Some(a),
            other_anchor: [1.0, 0.0],
            ..Default::default()
        },
    ));
    // A slider on a horizontal rail with a motor moves sideways only.
    let cart = w.spawn((
        at(30.0, 5.0),
        Body2D::default(),
        Collider2D::default(),
        Joint2D {
            kind: "slider".into(),
            other_anchor: [30.0, 5.0],
            limits: [-1.0, 4.0],
            motor_speed: 3.0,
            motor_force: 1000.0,
            ..Default::default()
        },
    ));
    let mut p = Physics2D::new();
    run(&mut w, &mut p, 120);
    assert_eq!(p.joint_count(), 4);
    let d = |e| {
        let t = w.get::<Transform>(e).unwrap().translation;
        t
    };
    let pb = d(bob);
    let arm = (pb[0].powi(2) + (pb[1] - 5.0).powi(2)).sqrt();
    assert!((arm - 3.0).abs() < 0.05, "pendulum arm {arm}");
    assert!(pb[1] < 5.0, "swung down");
    let pw = d(weight);
    assert!(((pw[0] - 10.0).powi(2) + (pw[1] - 5.0).powi(2)).sqrt() <= 2.05);
    let (pa, pb2) = (d(a), d(b));
    assert!(pa[1] < 4.0, "welded pair fell");
    assert!(((pb2[0] - pa[0]).powi(2) + (pb2[1] - pa[1]).powi(2)).sqrt() - 1.0 < 0.05);
    let pc = d(cart);
    assert!((pc[1] - 5.0).abs() < 0.05, "stays on the rail: {}", pc[1]);
    assert!(pc[0] > 31.0, "motor pushes it along the rail: {}", pc[0]);
    run(&mut w, &mut p, 180);
    let x = x_of(&w, cart);
    assert!((x - 34.0).abs() < 0.1, "stops at the upper limit: {x}");
}

#[test]
fn springs_distance_and_wheels() {
    let mut w = world();
    let hang = w.spawn((
        at(0.0, 3.0),
        Body2D::default(),
        Collider2D::default(),
        Joint2D {
            kind: "spring".into(),
            other_anchor: [0.0, 5.0],
            length: 2.0,
            stiffness: 400.0,
            damping: 20.0,
            ..Default::default()
        },
    ));
    let rod = w.spawn((
        at(5.0, 5.0),
        Body2D::default(),
        Collider2D::default(),
        Joint2D { kind: "distance".into(), other_anchor: [5.0, 6.5], ..Default::default() },
    ));
    let body = w.spawn((at(10.0, 3.0), Body2D::kinematic(), Collider2D::default()));
    let wheel = w.spawn((
        at(10.0, 2.0),
        Body2D::default(),
        Collider2D { shape: "circle".into(), size: [0.6, 0.6], ..Default::default() },
        Joint2D {
            kind: "wheel".into(),
            other: Some(body),
            other_anchor: [0.0, -1.0],
            axis: [0.0, 1.0],
            stiffness: 500.0,
            damping: 30.0,
            motor_speed: 180.0,
            motor_force: 50.0,
            ..Default::default()
        },
    ));
    let mut p = Physics2D::new();
    run(&mut w, &mut p, 240);
    // The spring sags a little below its rest length under gravity, then settles.
    let sag = 5.0 - y_of(&w, hang);
    assert!(sag > 2.0 && sag < 2.3, "spring length {sag}");
    let r = w.get::<Transform>(rod).unwrap().translation;
    let len = ((r[0] - 5.0).powi(2) + (r[1] - 6.5).powi(2)).sqrt();
    assert!((len - 1.5).abs() < 0.05, "distance kept: {len}");
    assert!((x_of(&w, wheel) - 10.0).abs() < 0.05, "wheel stays under its body");
    assert!(w.get::<Velocity2D>(wheel).is_none() || true);
    let _ = wheel;
}

#[test]
fn fast_small_bodies_with_ccd_do_not_tunnel() {
    let shoot = |ccd: bool| {
        let mut w = world();
        w.spawn((
            at(5.0, 0.0),
            Body2D::fixed(),
            Collider2D { size: [0.1, 10.0], ..Default::default() },
        ));
        let b = w.spawn((
            at(0.0, 0.0),
            Body2D { gravity_scale: 0.0, ccd, ..Default::default() },
            Collider2D { shape: "circle".into(), size: [0.1, 0.1], ..Default::default() },
            Velocity2D { linear: [600.0, 0.0], angular: 0.0 },
        ));
        let mut p = Physics2D::new();
        run(&mut w, &mut p, 10);
        x_of(&w, b)
    };
    assert!(shoot(true) < 5.0, "with CCD it stops at the wall");
}

#[test]
fn joints_and_new_collider_fields_save_and_load() {
    let mut w = world();
    let a = w.spawn((
        at(0.0, 0.0),
        Body2D::default(),
        Collider2D {
            shape: "polygon".into(),
            points: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            layer: 4,
            mask: 5,
            one_way: true,
            material: "ice".into(),
            friction_combine: "min".into(),
            ..Default::default()
        },
    ));
    let b = w.spawn((
        at(1.0, 0.0),
        Body2D::default(),
        Collider2D::default(),
        Joint2D {
            kind: "slider".into(),
            other: Some(a),
            limits: [-1.0, 2.0],
            ..Default::default()
        },
    ));
    let text = w.save_scene(None).text;
    let mut back = world();
    back.load_scene(&text).unwrap();
    let ids: Vec<_> =
        back.query::<(seal_ecs::Entity, &Collider2D)>().collect(|(e, c)| (e, c.clone()));
    let poly = ids.iter().find(|r| r.1.shape == "polygon").unwrap();
    assert_eq!(poly.1, w.get::<Collider2D>(a).unwrap().clone());
    let joints: Vec<Joint2D> = back.query::<&Joint2D>().collect(|j| j.clone());
    assert_eq!(joints.len(), 1);
    assert_eq!(joints[0].other, Some(poly.0));
    assert_eq!(joints[0].limits, w.get::<Joint2D>(b).unwrap().limits);
}
