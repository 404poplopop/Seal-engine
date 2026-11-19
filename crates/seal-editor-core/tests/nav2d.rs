//! 2D navigation: walkers route around tiles and solid bodies, step around
//! each other, follow moving objects, and their settings save and load.

use seal_editor_core::{EditorSession, Layout};
use seal_world::nav2d;

/// A session whose objects are all hidden except the ones a test sets up.
fn blank() -> (EditorSession, Vec<String>) {
    let mut session = EditorSession::new(Layout::default());
    for e in &mut session.world.entities {
        e.hidden = true;
    }
    // Plain objects only: folders keep their own transform rules.
    let es = &session.world.entities;
    let ids = es
        .iter()
        .filter(|e| !es.iter().any(|c| c.parent.as_deref() == Some(e.id.as_str())))
        .map(|e| e.id.clone())
        .collect();
    (session, ids)
}

fn put(session: &mut EditorSession, id: &str, pos: [f32; 2], scale: [f32; 2]) {
    let e = session.world.entity_mut(id).unwrap();
    e.hidden = false;
    e.translation = [pos[0], pos[1], 0.0];
    e.scale = [scale[0], scale[1], 1.0];
}

fn set(session: &mut EditorSession, id: &str, field: &str, value: &str) {
    session.models.set_field(id, field, value);
    session.mirror_node(id);
}

#[test]
fn walker_routes_around_a_wall_and_a_tile() {
    let (mut s, ids) = blank();
    let (walker, wall, tile) = (ids[2].clone(), ids[3].clone(), ids[4].clone());
    set(&mut s, &wall, "phys_body", "fixed");
    s.world.entity_mut(&tile).unwrap().tag = "ground".into();
    set(&mut s, &walker, "nav_agent", "yes");
    set(&mut s, &walker, "nav_target", "4, 0");
    set(&mut s, &walker, "nav_avoid", "no");
    put(&mut s, &wall, [0.0, 0.0], [1.0, 4.0]);
    put(&mut s, &tile, [3.0, 3.0], [1.0, 1.0]);
    put(&mut s, &walker, [-4.0, 0.0], [0.5, 0.5]);
    let grid = nav2d::map(&s.world.entities, 0.4);
    let route = nav2d::route(&grid, [-4.0, 0.0], [4.0, 0.0]).expect("a way round the wall");
    assert!(route.len() >= 2, "it has to turn a corner: {route:?}");
    assert_eq!(*route.last().unwrap(), [4.0, 0.0]);
    assert!(nav2d::clear_line(&grid, [-4.0, 3.0], [-1.0, 3.0]));
    assert!(!nav2d::clear_line(&grid, [-4.0, 0.0], [4.0, 0.0]), "the wall blocks the straight way");
    s.world.play();
    let mut steps = 0;
    while !s.world.walkers.arrived.contains(&walker) && steps < 600 {
        s.world.step(1.0 / 60.0);
        steps += 1;
        let p = s.world.entity(&walker).unwrap().translation;
        assert!(!(p[0].abs() < 0.5 && p[1].abs() < 2.0), "walked into the wall at {p:?}");
    }
    let p = s.world.entity(&walker).unwrap().translation;
    assert!(s.world.walkers.arrived.contains(&walker), "arrived, now at {p:?} after {steps} steps");
    assert!((p[0] - 4.0).abs() < 0.1 && p[1].abs() < 0.1);
    // A straight walk would take 8 / 3 s; going round takes longer.
    assert!(steps as f32 / 60.0 > 8.0 / 3.0);
    s.world.stop();
    assert_eq!(s.world.entity(&walker).unwrap().translation, [-4.0, 0.0, 0.0]);
}

#[test]
fn walkers_meeting_head_on_step_around_each_other() {
    let (mut s, ids) = blank();
    let (a, b) = (ids[2].clone(), ids[3].clone());
    for (id, to) in [(&a, "3, 0"), (&b, "-3, 0")] {
        set(&mut s, id, "nav_agent", "yes");
        set(&mut s, id, "nav_target", to);
    }
    put(&mut s, &a, [-3.0, 0.0], [0.5, 0.5]);
    put(&mut s, &b, [3.0, 0.0], [0.5, 0.5]);
    s.world.play();
    let mut closest = f32::MAX;
    for _ in 0..600 {
        s.world.step(1.0 / 60.0);
        let pa = s.world.entity(&a).unwrap().translation;
        let pb = s.world.entity(&b).unwrap().translation;
        closest = closest.min(((pa[0] - pb[0]).powi(2) + (pa[1] - pb[1]).powi(2)).sqrt());
        if s.world.walkers.arrived.len() == 2 {
            break;
        }
    }
    assert_eq!(s.world.walkers.arrived.len(), 2, "both got through");
    assert!(closest > 0.5, "they kept apart, closest {closest}");
}

#[test]
fn walker_follows_a_moving_object() {
    let (mut s, ids) = blank();
    let (dog, ball) = (ids[2].clone(), ids[3].clone());
    let name = s.world.entity(&ball).unwrap().name.clone();
    set(&mut s, &dog, "nav_agent", "yes");
    set(&mut s, &dog, "nav_target", &name);
    put(&mut s, &dog, [0.0, 0.0], [0.5, 0.5]);
    put(&mut s, &ball, [5.0, 0.0], [0.5, 0.5]);
    s.world.play();
    for i in 0..300 {
        // The ball drifts upwards, off the edge of the first map; the dog keeps after it.
        s.world.entity_mut(&ball).unwrap().translation[1] = i as f32 / 30.0;
        s.world.step(1.0 / 60.0);
    }
    let d = s.world.entity(&dog).unwrap().translation;
    let t = s.world.entity(&ball).unwrap().translation;
    let gap = ((d[0] - t[0]).powi(2) + (d[1] - t[1]).powi(2)).sqrt();
    assert!(gap < 1.0, "caught up: dog {d:?} ball {t:?}");
    // Once the ball stops, the dog settles next to it.
    for _ in 0..30 {
        s.world.step(1.0 / 60.0);
    }
    assert!(s.world.walkers.arrived.contains(&dog));
}

#[test]
fn walker_settings_save_load_and_refuse_bad_values() {
    let (mut s, ids) = blank();
    let w = ids[2].clone();
    for (k, v) in [
        ("nav_agent", "yes"),
        ("nav_target", " Goal "),
        ("nav_speed", "5"),
        ("nav_radius", "0.3"),
        ("nav_avoid", "no"),
        ("nav_speed", "-1"),
        ("nav_radius", "0"),
    ] {
        set(&mut s, &w, k, v);
    }
    let n = s.world.entity(&w).unwrap().nav2d.clone();
    assert!(n.on && !n.avoid);
    assert_eq!((n.target.as_str(), n.speed, n.radius), ("Goal", 5.0, 0.3));
    let loaded = seal_world::World::from_text(&s.world.to_text()).unwrap();
    assert_eq!(loaded.entity(&w).unwrap().nav2d, n);
    assert_eq!(nav2d::parse_target("2, -1"), Some(nav2d::Target::Point([2.0, -1.0])));
    assert_eq!(nav2d::parse_target("Door"), Some(nav2d::Target::Object("Door".into())));
}
