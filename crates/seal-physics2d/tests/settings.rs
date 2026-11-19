use seal_ecs::core::{SceneSettings, Transform};
use seal_ecs::World;
use seal_physics2d::*;

fn world() -> World {
    let mut w = World::new();
    w.register_core();
    register_physics2d(&mut w);
    w
}

#[test]
fn project_settings_reach_the_simulation() {
    let settings = Physics2DSettings {
        velocity_iterations: 12,
        position_iterations: 6,
        gravity_scale: 0.5,
        ..Default::default()
    };
    let mut w = world();
    w.insert_resource(SceneSettings { gravity: 20.0, time_scale: 1.0, music: String::new() });
    let _ground = w.spawn((
        at(0.0, 0.0),
        Body2D::fixed(),
        Collider2D { size: [40.0, 1.0], ..Default::default() },
    ));
    let b =
        w.spawn((at(0.0, 5.0), Body2D::default(), Collider2D::default(), Velocity2D::default()));
    let mut p = Physics2D::new();
    p.settings = settings.clone();
    p.step(&mut w, 1.0 / 60.0).unwrap();
    assert_eq!(
        p.rapier().integration_parameters.num_solver_iterations,
        (settings.velocity_iterations + settings.position_iterations) as usize
    );
    let mut p2 = Physics2D::new();
    p2.settings.gravity_scale = 0.0;
    p2.step(&mut w, 1.0 / 60.0).unwrap();
    let y_no_grav = w.get::<Transform>(b).unwrap().translation[1];
    let mut p3 = Physics2D::new();
    p3.settings.gravity_scale = 2.0;
    w.get_mut::<Transform>(b).unwrap().translation[1] = 5.0;
    p3.step(&mut w, 1.0 / 60.0).unwrap();
    let y_more = w.get::<Transform>(b).unwrap().translation[1];
    assert!(y_more < y_no_grav, "higher gravity scale pulls down faster");
}

#[test]
fn extra_mass_and_constant_force_move_a_body() {
    let mut w = world();
    w.insert_resource(SceneSettings { gravity: 0.0, time_scale: 1.0, music: String::new() });
    let b = w.spawn((
        at(0.0, 0.0),
        Body2D {
            additional_mass: 4.0,
            constant_force: [10.0, 0.0],
            can_sleep: false,
            ..Default::default()
        },
        Collider2D::default(),
        Velocity2D::default(),
    ));
    let mut p = Physics2D::new();
    p.settings.sleeping = false;
    for _ in 0..60 {
        p.step(&mut w, 1.0 / 60.0).unwrap();
    }
    let x = w.get::<Transform>(b).unwrap().translation[0];
    assert!(x > 0.5, "constant force accelerates the body, x={x}");
    let m = p.mass_of(b).unwrap();
    assert!(m > 4.0, "mass includes extra mass, m={m}");
}

#[test]
fn defaults_match_the_old_fixed_step() {
    let s = Physics2DSettings::default();
    assert!((s.fixed_step() - 1.0 / 60.0).abs() < 1e-6);
    assert_eq!(s.max_substeps, 5);
    assert!(!s.render_interpolation);
}

#[test]
fn render_interpolation_lerps_between_steps() {
    let mut w = world();
    w.insert_resource(SceneSettings { gravity: 0.0, time_scale: 1.0, music: String::new() });
    let b = w.spawn((
        at(0.0, 0.0),
        Body2D { constant_force: [60.0, 0.0], can_sleep: false, ..Default::default() },
        Collider2D::default(),
        Velocity2D::default(),
    ));
    let mut p = Physics2D::new();
    p.settings.render_interpolation = true;
    p.settings.sleeping = false;
    p.step(&mut w, 1.0 / 60.0).unwrap();
    let after_step = w.get::<Transform>(b).unwrap().translation[0];
    p.write_transforms(&mut w, 0.5);
    let mid = w.get::<Transform>(b).unwrap().translation[0];
    assert!(mid < after_step, "halfway draw trails the last step, mid={mid} after={after_step}");
    assert!(mid > 0.0);
}

fn at(x: f32, y: f32) -> Transform {
    Transform { translation: [x, y, 0.0], ..Default::default() }
}
