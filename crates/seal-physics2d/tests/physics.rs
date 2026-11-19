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

fn ground(w: &mut World) -> seal_ecs::Entity {
    w.spawn((at(0.0, 0.0), Body2D::fixed(), Collider2D { size: [40.0, 1.0], ..Default::default() }))
}

fn run(w: &mut World, p: &mut Physics2D, steps: usize) {
    for _ in 0..steps {
        p.step(w, 1.0 / 60.0).unwrap();
    }
}

#[test]
fn a_box_falls_and_rests_on_the_ground() {
    let mut w = world();
    let g = ground(&mut w);
    let b =
        w.spawn((at(0.0, 5.0), Body2D::default(), Collider2D::default(), Velocity2D::default()));
    let mut p = Physics2D::new();
    p.step(&mut w, 1.0 / 60.0).unwrap();
    assert_eq!(p.body_count(), 2);
    assert!(w.get::<Transform>(b).unwrap().translation[1] < 5.0);
    let mut started = 0;
    for _ in 0..240 {
        p.step(&mut w, 1.0 / 60.0).unwrap();
        started += p.contacts.iter().filter(|c| c.started && c.a.min(c.b) == g.min(b)).count();
    }
    let y = w.get::<Transform>(b).unwrap().translation[1];
    assert!((y - 1.0).abs() < 0.02, "rests on top of the ground, y = {y}");
    assert_eq!(started, 1, "one touch-down event");
    assert!(w.get::<Velocity2D>(b).unwrap().linear[1].abs() < 0.05);
}

#[test]
fn sensors_report_overlaps_without_pushing() {
    let mut w = world();
    let zone = w.spawn((
        at(0.0, 2.0),
        Body2D::fixed(),
        Collider2D { sensor: true, size: [4.0, 1.0], ..Default::default() },
    ));
    let ball = w.spawn((
        at(0.0, 5.0),
        Body2D::default(),
        Collider2D { shape: "circle".into(), ..Default::default() },
    ));
    let mut p = Physics2D::new();
    let mut hits = Vec::new();
    for _ in 0..120 {
        p.step(&mut w, 1.0 / 60.0).unwrap();
        hits.extend(p.contacts.iter().copied());
    }
    assert!(hits.iter().any(|c| c.started && c.sensor && [c.a, c.b].contains(&zone)));
    assert!(hits.iter().any(|c| !c.started && c.sensor), "fell through and left the zone");
    assert!(w.get::<Transform>(ball).unwrap().translation[1] < 0.0);
}

#[test]
fn rays_hit_the_first_collider() {
    let mut w = world();
    let g = ground(&mut w);
    let mut p = Physics2D::new();
    p.step(&mut w, 1.0 / 60.0).unwrap();
    let hit = p.raycast([3.0, 10.0], [0.0, -1.0], 100.0).unwrap();
    assert_eq!(hit.entity, g);
    assert!((hit.distance - 9.5).abs() < 1e-4);
    assert!((hit.point[1] - 0.5).abs() < 1e-4 && (hit.normal[1] - 1.0).abs() < 1e-4);
    assert!(p.raycast([3.0, 10.0], [0.0, 1.0], 100.0).is_none());
}

#[test]
fn gameplay_can_push_teleport_and_drive_bodies() {
    let mut w = world();
    w.insert_resource(SceneSettings { gravity: 0.0, time_scale: 1.0, music: String::new() });
    let b =
        w.spawn((at(0.0, 0.0), Body2D::default(), Collider2D::default(), Velocity2D::default()));
    let k = w.spawn((at(10.0, 0.0), Body2D::kinematic(), Collider2D::default()));
    let mut p = Physics2D::new();
    run(&mut w, &mut p, 1);
    w.get_mut::<Velocity2D>(b).unwrap().linear = [6.0, 0.0];
    run(&mut w, &mut p, 30);
    let x = w.get::<Transform>(b).unwrap().translation[0];
    assert!((x - 3.0).abs() < 0.05, "half a second at 6 units/s, x = {x}");
    w.get_mut::<Transform>(b).unwrap().translation = [-20.0, 7.0, 0.0];
    run(&mut w, &mut p, 1);
    assert!((w.get::<Transform>(b).unwrap().translation[1] - 7.0).abs() < 1e-3, "teleport sticks");
    w.get_mut::<Transform>(k).unwrap().translation[1] = 4.0;
    run(&mut w, &mut p, 2);
    let hit = p.raycast([10.0, 10.0], [0.0, -1.0], 100.0).unwrap();
    assert_eq!(hit.entity, k);
    assert!((hit.point[1] - 4.5).abs() < 1e-3, "kinematic body followed its transform");
}

#[test]
fn settings_and_despawning_are_respected() {
    let mut w = world();
    w.insert_resource(SceneSettings { gravity: 9.81, time_scale: 0.0, music: String::new() });
    let b = w.spawn((at(0.0, 5.0), Body2D::default(), Collider2D::default()));
    let mut p = Physics2D::new();
    run(&mut w, &mut p, 30);
    assert_eq!(w.get::<Transform>(b).unwrap().translation[1], 5.0, "paused by time scale 0");
    w.despawn(b);
    run(&mut w, &mut p, 1);
    assert_eq!(p.body_count(), 0);
    let bad = w.spawn((
        at(0.0, 0.0),
        Body2D { kind: "floaty".into(), ..Default::default() },
        Collider2D::default(),
    ));
    assert_eq!(
        p.step(&mut w, 0.016),
        Err(PhysicsError::BadKind { entity: bad, kind: "floaty".into() })
    );
}

#[test]
fn physics_components_save_with_the_scene() {
    let mut w = world();
    w.spawn((
        at(1.0, 2.0),
        Body2D { lock_rotation: true, ..Default::default() },
        Collider2D { shape: "capsule".into(), size: [1.0, 2.0], ..Default::default() },
        Velocity2D { linear: [1.0, 0.0], angular: 0.0 },
    ));
    let text = w.save_scene(None).text;
    let mut back = world();
    back.load_scene(&text).unwrap();
    assert_eq!(back.save_scene(None).text, text);
    let mut p = Physics2D::new();
    p.step(&mut back, 1.0 / 60.0).unwrap();
    assert_eq!(p.body_count(), 1);
}

#[test]
fn a_pile_of_boxes_lands_the_same_everywhere() {
    let mut w = world();
    ground(&mut w);
    let mut boxes = Vec::new();
    for i in 0..30 {
        let f = i as f32;
        boxes.push(w.spawn((
            Transform {
                translation: [(f * 0.37) % 3.0 - 1.5, 2.0 + f * 1.1, 0.0],
                rotation: [0.0, 0.0, f * 13.0],
                ..Default::default()
            },
            Body2D::default(),
            Collider2D {
                shape: if i % 3 == 0 { "circle".into() } else { "box".into() },
                size: [0.8, 0.6],
                restitution: 0.2,
                ..Default::default()
            },
        )));
    }
    let mut p = Physics2D::new();
    run(&mut w, &mut p, 300);
    let hash = boxes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, e| {
        let t = w.get::<Transform>(*e).unwrap();
        [t.translation[0], t.translation[1], t.rotation[2]]
            .iter()
            .fold(h, |h, v| (h ^ v.to_bits() as u64).wrapping_mul(0x0100_0000_01b3))
    });
    assert_eq!(hash, GOLDEN, "pile hash {hash:#x}");
}

const GOLDEN: u64 = 0x6acf_2620_d0fc_8440;
