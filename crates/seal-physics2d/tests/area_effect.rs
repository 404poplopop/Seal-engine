//! Area effects and moving surfaces, checked against hand-computed steps.

use seal_ecs::core::{SceneSettings, Transform};
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

fn gravity(w: &mut World, g: f32) {
    w.insert_resource(SceneSettings { gravity: g, time_scale: 1.0, music: String::new() });
}

fn zone(w: &mut World, pos: [f32; 2], size: [f32; 2], effect: AreaEffect) -> seal_ecs::Entity {
    w.spawn((
        at(pos[0], pos[1]),
        Body2D::fixed(),
        Collider2D { sensor: true, size, ..Default::default() },
        effect,
    ))
}

fn body_at(w: &mut World, x: f32, y: f32) -> seal_ecs::Entity {
    w.spawn((
        at(x, y),
        Body2D::default(),
        Collider2D::default(),
        Velocity2D::default(),
    ))
}

fn vel(w: &World, id: seal_ecs::Entity) -> [f32; 2] {
    w.get::<Velocity2D>(id).unwrap().linear
}

fn step(w: &mut World, p: &mut Physics2D, n: usize) {
    for _ in 0..n {
        p.step(w, 1.0 / 60.0).unwrap();
    }
}

#[test]
fn directional_acceleration_matches_one_step_and_sixty() {
    let mut w = world();
    gravity(&mut w, 0.0);
    zone(
        &mut w,
        [0.0, 0.0],
        [10.0, 10.0],
        AreaEffect {
            mode: "directional".into(),
            force_kind: "acceleration".into(),
            direction: [1.0, 0.0],
            strength: 4.0,
            ..Default::default()
        },
    );
    let b = body_at(&mut w, 0.0, 0.0);
    let mut p = Physics2D::new();
    step(&mut w, &mut p, 1);
    let v = vel(&w, b);
    assert!((v[0] - 4.0 / 60.0).abs() < 1e-4, "{v:?}");
    assert!(v[1].abs() < 1e-4, "{v:?}");
    step(&mut w, &mut p, 59);
    let v = vel(&w, b);
    let x = w.get::<Transform>(b).unwrap().translation[0];
    // v = a * N * dt. The solver splits the step into 4 substeps, so
    // x = a * dt^2 * N * (4 * N + 1) / 8.
    assert!((v[0] - 4.0).abs() < 1e-3, "{v:?}");
    assert!((x - 2.008333).abs() < 1e-3, "{x}");
}

#[test]
fn directional_force_is_divided_by_mass() {
    let mut w = world();
    gravity(&mut w, 0.0);
    zone(
        &mut w,
        [0.0, 0.0],
        [10.0, 10.0],
        AreaEffect {
            mode: "directional".into(),
            force_kind: "force".into(),
            direction: [0.0, 1.0],
            strength: 12.0,
            ..Default::default()
        },
    );
    let b = body_at(&mut w, 0.0, 0.0);
    let mut p = Physics2D::new();
    step(&mut w, &mut p, 1);
    let v = vel(&w, b);
    assert!((v[1] - 12.0 / 60.0).abs() < 1e-4, "{v:?}");
}

#[test]
fn point_falloff_matches_the_three_curves() {
    for (falloff, range, expect) in [
        ("none", 0.0, -10.0 / 60.0),
        ("linear", 4.0, -5.0 / 60.0),
        ("inverse", 0.0, -2.5 / 60.0),
        ("linear", 1.0, 0.0),
    ] {
        let mut w = world();
        gravity(&mut w, 0.0);
        zone(
            &mut w,
            [0.0, 0.0],
            [20.0, 20.0],
            AreaEffect {
                mode: "point".into(),
                point_sign: "attract".into(),
                falloff: falloff.into(),
                strength: 10.0,
                range,
                ..Default::default()
            },
        );
        let b = body_at(&mut w, 2.0, 0.0);
        let mut p = Physics2D::new();
        step(&mut w, &mut p, 1);
        let v = vel(&w, b);
        assert!((v[0] - expect).abs() < 1e-4, "{falloff} range {range}: {v:?}");
        assert!(v[1].abs() < 1e-4, "{v:?}");
    }
}

#[test]
fn a_layer_mask_skips_bodies_on_other_layers() {
    let mut w = world();
    gravity(&mut w, 0.0);
    let area = zone(
        &mut w,
        [0.0, 0.0],
        [10.0, 10.0],
        AreaEffect {
            mode: "directional".into(),
            direction: [1.0, 0.0],
            strength: 6.0,
            mask: 1,
            ..Default::default()
        },
    );
    let blocked = w.spawn((
        at(0.0, 0.0),
        Body2D::default(),
        Collider2D { layer: 2, ..Default::default() },
        Velocity2D::default(),
    ));
    let mut p = Physics2D::new();
    step(&mut w, &mut p, 1);
    assert!(vel(&w, blocked)[0].abs() < 1e-5);
    w.get_mut::<AreaEffect>(area).unwrap().mask = 2;
    step(&mut w, &mut p, 1);
    assert!((vel(&w, blocked)[0] - 6.0 / 60.0).abs() < 1e-4, "{}", vel(&w, blocked)[0]);
}

#[test]
fn drag_halves_speed_at_damping_60() {
    let mut w = world();
    gravity(&mut w, 0.0);
    zone(
        &mut w,
        [0.0, 0.0],
        [10.0, 10.0],
        AreaEffect {
            mode: "drag".into(),
            linear_damping: 60.0,
            angular_damping: 60.0,
            ..Default::default()
        },
    );
    let b = w.spawn((
        at(0.0, 0.0),
        Body2D::default(),
        Collider2D::default(),
        Velocity2D { linear: [4.0, 0.0], angular: 180.0 },
    ));
    let mut p = Physics2D::new();
    step(&mut w, &mut p, 1);
    let v = w.get::<Velocity2D>(b).unwrap();
    assert!((v.linear[0] - 2.0).abs() < 1e-3, "{:?}", v.linear);
    assert!((v.angular - 90.0).abs() < 1e-2, "{}", v.angular);
}

#[test]
fn two_areas_add_and_the_higher_gravity_priority_wins() {
    let mut w = world();
    gravity(&mut w, 0.0);
    zone(
        &mut w,
        [0.0, 0.0],
        [10.0, 10.0],
        AreaEffect {
            mode: "directional".into(),
            direction: [1.0, 0.0],
            strength: 3.0,
            ..Default::default()
        },
    );
    zone(
        &mut w,
        [0.0, 0.0],
        [10.0, 10.0],
        AreaEffect {
            mode: "directional".into(),
            direction: [1.0, 0.0],
            strength: 5.0,
            ..Default::default()
        },
    );
    let b = body_at(&mut w, 0.0, 0.0);
    let mut p = Physics2D::new();
    step(&mut w, &mut p, 1);
    assert!((vel(&w, b)[0] - 8.0 / 60.0).abs() < 1e-4, "{}", vel(&w, b)[0]);

    let mut w = world();
    gravity(&mut w, 10.0);
    zone(
        &mut w,
        [0.0, 0.0],
        [10.0, 10.0],
        AreaEffect {
            mode: "gravity".into(),
            gravity_mode: "replace".into(),
            gravity: [0.0, -20.0],
            priority: 1,
            ..Default::default()
        },
    );
    zone(
        &mut w,
        [0.0, 0.0],
        [10.0, 10.0],
        AreaEffect {
            mode: "gravity".into(),
            gravity_mode: "scale".into(),
            gravity_scale: 0.0,
            priority: 2,
            ..Default::default()
        },
    );
    let b = body_at(&mut w, 0.0, 2.0);
    let mut p = Physics2D::new();
    step(&mut w, &mut p, 1);
    assert!(vel(&w, b)[1].abs() < 1e-3, "zero gravity wins, vy {}", vel(&w, b)[1]);
}

#[test]
fn a_light_box_floats_and_a_heavy_one_sinks() {
    let mut w = world();
    gravity(&mut w, 9.81);
    zone(
        &mut w,
        [0.0, 0.0],
        [20.0, 20.0],
        AreaEffect {
            mode: "buoyancy".into(),
            surface: 0.0,
            density: 1.0,
            linear_drag: 8.0,
            angular_drag: 1.0,
            ..Default::default()
        },
    );
    let light = w.spawn((
        at(0.0, 1.0),
        Body2D::default(),
        Collider2D { density: 0.25, ..Default::default() },
        Velocity2D::default(),
    ));
    let heavy = w.spawn((
        at(4.0, 0.0),
        Body2D::default(),
        Collider2D { density: 3.0, ..Default::default() },
        Velocity2D::default(),
    ));
    let mut p = Physics2D::new();
    step(&mut w, &mut p, 400);
    let y = w.get::<Transform>(light).unwrap().translation[1];
    let vy = vel(&w, light)[1];
    assert!((y - 0.25).abs() < 0.08, "light box y {y}");
    assert!(vy.abs() < 0.3, "light box vy {vy}");
    let hy = w.get::<Transform>(heavy).unwrap().translation[1];
    assert!(hy < -0.5, "heavy box y {hy}");
}

#[test]
fn a_moving_surface_carries_a_resting_box() {
    let mut w = world();
    gravity(&mut w, 9.81);
    let belt = w.spawn((
        at(0.0, 0.0),
        Body2D::fixed(),
        Collider2D { size: [20.0, 1.0], friction: 1.0, ..Default::default() },
        SurfaceEffect { speed: 2.0 },
    ));
    let _ = belt;
    let b = w.spawn((
        at(0.0, 1.0),
        Body2D::default(),
        Collider2D::default(),
        Velocity2D::default(),
    ));
    let mut p = Physics2D::new();
    step(&mut w, &mut p, 90);
    let x0 = w.get::<Transform>(b).unwrap().translation[0];
    step(&mut w, &mut p, 60);
    let x1 = w.get::<Transform>(b).unwrap().translation[0];
    assert!((x1 - x0 - 2.0).abs() < 0.15, "{x0} -> {x1}");
}

#[test]
fn the_same_variation_is_deterministic() {
    assert_eq!(
        area_effect::strength_scale(3, seal_ecs::Entity::from_bits(9), 0.4),
        area_effect::strength_scale(3, seal_ecs::Entity::from_bits(9), 0.4)
    );
    let scale = area_effect::strength_scale(3, seal_ecs::Entity::from_bits(9), 0.4);
    assert!((0.6..=1.4).contains(&scale), "{scale}");
    assert_eq!(area_effect::strength_scale(1, seal_ecs::Entity::from_bits(1), 0.0), 1.0);
}

#[test]
fn a_damaged_area_file_is_refused() {
    let scene = r#"
seal_scene = 1
entities = [
  { id = 0, components = { area_effect = { mode = true } } }
]
"#;
    let mut w = world();
    let err = w.load_scene(scene).unwrap_err();
    assert!(err.contains("mode") || err.contains("area_effect"), "{err}");
}
