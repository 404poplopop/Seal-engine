//! 2D lines and filled shapes, and objects following them during play.

use seal_editor_core::{shape2d, EditorSession, Layout};

fn set(session: &mut EditorSession, id: &str, field: &str, value: &str) {
    session.models.set_field(id, field, value);
    session.mirror_node(id);
}

/// A session whose objects are all hidden; returns plain (folder-free) object ids.
fn blank() -> (EditorSession, Vec<String>) {
    let mut session = EditorSession::new(Layout::default());
    for e in &mut session.world.entities {
        e.hidden = true;
    }
    let es = &session.world.entities;
    let ids = es
        .iter()
        .filter(|e| !es.iter().any(|c| c.parent.as_deref() == Some(e.id.as_str())))
        .map(|e| e.id.clone())
        .collect();
    (session, ids)
}

fn place(session: &mut EditorSession, id: &str, pos: [f32; 2]) {
    let e = session.world.entity_mut(id).unwrap();
    e.hidden = false;
    e.translation = [pos[0], pos[1], 0.0];
    e.scale = [1.0; 3];
    e.rotation = [0.0; 3];
}

#[test]
fn shape_settings_edit_save_load_and_refuse_bad_values() {
    let (mut s, ids) = blank();
    let a = ids[2].clone();
    set(&mut s, &a, "shape2d", "line");
    // A new shape starts as a visible square.
    assert_eq!(s.world.entity(&a).unwrap().shape2d.points.len(), 4);
    for (k, v) in [
        ("shape_points", "0 0; 2 0; 2 1"),
        ("shape_width", "0.25"),
        ("shape_color", "#FF8800"),
        ("shape_closed", "yes"),
        ("follow_path", " Rail "),
        ("follow_mode", "back"),
        ("follow_rotate", "yes"),
        ("follow_speed", "4"),
        ("follow_start", "1.5"),
        // Refused: an odd point, a zero width, a bad color, an unknown kind or mode.
        ("shape_points", "0 0; 1"),
        ("shape_width", "0"),
        ("shape_color", "orange-ish"),
        ("shape2d", "circle"),
        ("follow_mode", "bounce"),
    ] {
        set(&mut s, &a, k, v);
    }
    let e = s.world.entity(&a).unwrap().clone();
    assert_eq!(e.shape2d.kind, "line");
    assert_eq!(e.shape2d.points, vec![[0.0, 0.0], [2.0, 0.0], [2.0, 1.0]]);
    assert_eq!(e.shape2d.width, 0.25);
    assert!(e.shape2d.closed);
    assert_eq!(seal_editor_core::tint_hex(&e.shape2d.color), "#FF8800");
    assert_eq!(
        (e.follow.path.as_str(), e.follow.mode.as_str(), e.follow.speed),
        ("Rail", "back", 4.0)
    );
    assert!(e.follow.rotate);
    assert_eq!(e.follow.start, 1.5);
    let fields: Vec<String> = e.inspector_fields().into_iter().map(|(k, _)| k).collect();
    for k in shape2d::KEYS {
        assert!(fields.iter().any(|f| f == k), "{k} shows in the inspector");
    }
    // Both scene formats keep it.
    let loaded = seal_world::World::from_text(&s.world.to_text()).unwrap();
    let back = loaded.entity(&a).unwrap();
    assert_eq!((&back.shape2d, &back.follow), (&e.shape2d, &e.follow));
}

#[test]
fn follower_rides_a_moved_and_turned_path_and_play_stop_restores() {
    let (mut s, ids) = blank();
    let (rail, cart) = (ids[2].clone(), ids[3].clone());
    let rail_name = s.world.entity(&rail).unwrap().name.clone();
    set(&mut s, &rail, "shape2d", "line");
    set(&mut s, &rail, "shape_points", "0 0; 4 0; 4 2");
    set(&mut s, &cart, "follow_path", &rail_name);
    set(&mut s, &cart, "follow_speed", "2");
    set(&mut s, &cart, "follow_mode", "once");
    set(&mut s, &cart, "follow_rotate", "yes");
    place(&mut s, &rail, [10.0, 5.0]);
    place(&mut s, &cart, [0.0, 0.0]);
    // The rail is turned a quarter turn: its first leg points up the screen.
    s.world.entity_mut(&rail).unwrap().rotation[2] = 90.0;
    s.world.play();
    for _ in 0..60 {
        s.world.step(1.0 / 60.0);
    }
    // One second at 2 units/s: halfway along the first 4-unit leg, which now runs up from (10, 5).
    let c = s.world.entity(&cart).unwrap().clone();
    assert!(
        (c.translation[0] - 10.0).abs() < 0.05 && (c.translation[1] - 7.0).abs() < 0.05,
        "{:?}",
        c.translation
    );
    assert!((c.rotation[2] - 90.0).abs() < 0.01, "faces along the rail: {}", c.rotation[2]);
    for _ in 0..240 {
        s.world.step(1.0 / 60.0);
    }
    // Six units of track: it stops at the end, (8, 9).
    let c = s.world.entity(&cart).unwrap().clone();
    assert!(
        (c.translation[0] - 8.0).abs() < 0.05 && (c.translation[1] - 9.0).abs() < 0.05,
        "{:?}",
        c.translation
    );
    assert!(s.world.followers.finished.contains(&cart));
    s.world.stop();
    assert_eq!(s.world.entity(&cart).unwrap().translation, [0.0, 0.0, 0.0]);
}

#[test]
fn back_and_forth_and_closed_loops() {
    let (mut s, ids) = blank();
    let (ring, a, b) = (ids[2].clone(), ids[3].clone(), ids[4].clone());
    let ring_name = s.world.entity(&ring).unwrap().name.clone();
    set(&mut s, &ring, "shape2d", "polygon");
    set(&mut s, &ring, "shape_points", "0 0; 2 0; 2 2; 0 2");
    set(&mut s, &a, "follow_path", &ring_name);
    set(&mut s, &a, "follow_speed", "2");
    set(&mut s, &b, "follow_path", &ring_name);
    set(&mut s, &b, "follow_speed", "2");
    set(&mut s, &b, "follow_mode", "back");
    place(&mut s, &ring, [0.0, 0.0]);
    place(&mut s, &a, [0.0, 0.0]);
    place(&mut s, &b, [0.0, 0.0]);
    s.world.play();
    // 4.5 s at 2 units/s = 9 units: a filled shape is a closed loop of 8, so 1 unit in;
    // going back and forth, 8 there then 1 back: 7 units along, on the left side at (0, 1).
    for _ in 0..270 {
        s.world.step(1.0 / 60.0);
    }
    let pa = s.world.entity(&a).unwrap().translation;
    let pb = s.world.entity(&b).unwrap().translation;
    assert!((pa[0] - 1.0).abs() < 0.05 && pa[1].abs() < 0.05, "{pa:?}");
    assert!(pb[0].abs() < 0.05 && (pb[1] - 1.0).abs() < 0.05, "{pb:?}");
}
