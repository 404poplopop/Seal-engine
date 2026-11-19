//! 2D physics on scene objects: inspector edits, saving and loading, and play.

use seal_editor_core::{EditorSession, Layout};

fn set(session: &mut EditorSession, id: &str, field: &str, value: &str) {
    session.models.set_field(id, field, value);
    session.mirror_node(id);
}

#[test]
fn physics2d_settings_edit_save_and_load() {
    let mut session = EditorSession::new(Layout::default());
    let a = session.world.entities[2].id.clone();
    let b = session.world.entities[3].clone();
    assert_eq!(
        seal_editor_core::phys2d::fields(&session.world.entity(&a).unwrap().phys2d).len(),
        2
    );
    for (field, value) in [
        ("phys_body", "dynamic"),
        ("phys_shape", "polygon"),
        ("phys_points", "0 0; 1 0; 0 1"),
        ("phys_layer", "1, 3"),
        ("phys_mask", "2"),
        ("phys_one_way", "yes"),
        ("phys_surface", "ice"),
        ("phys_ccd", "yes"),
        ("joint", "hinge"),
        ("joint_to", &format!(" {} ", b.name)),
        ("joint_motor", "1, 5"),
        ("joint_limits", "-30, 30"),
        // Refused: zero size, limits the wrong way round, a layer past 32, an unknown shape.
        ("phys_size", "0, 1"),
        ("joint_limits", "2, 1"),
        ("phys_layer", "40"),
        ("phys_shape", "star"),
    ] {
        set(&mut session, &a, field, value);
    }
    let p = session.world.entity(&a).unwrap().phys2d.clone();
    assert_eq!(p.body, "dynamic");
    assert_eq!(p.shape, "polygon");
    assert_eq!(p.points, vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
    assert_eq!((p.layer, p.mask), (0b101, 0b10));
    assert!(p.one_way && p.ccd);
    assert_eq!(p.surface, "ice");
    assert_eq!(p.joint_to, b.name);
    assert_eq!(p.motor, [1.0, 5.0]);
    assert_eq!(p.limits, [-30.0, 30.0]);
    assert_eq!(p.size, [1.0, 1.0]);
    // The inspector lists the joint rows now that there is a joint.
    let shown = seal_editor_core::phys2d::fields(&p);
    assert!(shown.iter().any(|(k, v)| k == "joint_motor" && v == "1, 5"));
    assert!(shown.iter().any(|(k, v)| k == "phys_layer" && v == "1, 3"));
    // Saved and loaded both ways.
    let loaded = seal_world::World::from_text(&session.world.to_text()).unwrap();
    assert_eq!(loaded.entity(&a).unwrap().phys2d, p);
    // Going back to no body forgets it in the saved scene.
    set(&mut session, &a, "phys_body", "none");
    let loaded = seal_world::World::from_text(&session.world.to_text()).unwrap();
    assert!(loaded.entity(&a).unwrap().phys2d.body.is_empty());
}

#[test]
fn bodies_fall_rest_and_swing_in_play() {
    let mut session = EditorSession::new(Layout::default());
    let ids: Vec<String> = session.world.entities.iter().map(|e| e.id.clone()).collect();
    let (crate_id, floor, bob) = (ids[2].clone(), ids[3].clone(), ids[4].clone());
    for (id, pos, scale) in [
        (&crate_id, [0.0, 3.0, 0.0], [1.0, 1.0, 1.0]),
        (&floor, [0.0, 0.0, 0.0], [10.0, 1.0, 1.0]),
        (&bob, [6.0, 2.0, 0.0], [0.5, 0.5, 1.0]),
    ] {
        let e = session.world.entity_mut(id).unwrap();
        e.translation = pos;
        e.rotation = [0.0; 3];
        e.scale = scale;
    }
    set(&mut session, &crate_id, "phys_body", "dynamic");
    set(&mut session, &floor, "phys_body", "fixed");
    set(&mut session, &bob, "phys_body", "dynamic");
    set(&mut session, &bob, "joint", "hinge");
    set(&mut session, &bob, "joint_other_anchor", "5, 2");
    set(&mut session, &bob, "joint_anchor", "-2, 0");
    // Undo the edits' own movement so the scene starts where the test says.
    session.world.entity_mut(&crate_id).unwrap().translation = [0.0, 3.0, 0.0];
    session.world.play();
    for _ in 0..180 {
        session.world.step(1.0 / 60.0);
    }
    let c = session.world.entity(&crate_id).unwrap().translation;
    assert!((c[1] - 1.0).abs() < 0.05, "crate rests on the floor top: {c:?}");
    assert!(c[0].abs() < 0.05);
    let f = session.world.entity(&floor).unwrap().translation;
    assert_eq!(f, [0.0, 0.0, 0.0]);
    let p = session.world.entity(&bob).unwrap().translation;
    let r = ((p[0] - 5.0).powi(2) + (p[1] - 2.0).powi(2)).sqrt();
    assert!((r - 2.0).abs() < 0.05, "pendulum keeps its length: {p:?} r={r}");
    assert!(p[1] < 2.0, "pendulum swung down: {p:?}");
    session.world.stop();
    assert_eq!(session.world.entity(&crate_id).unwrap().translation, [0.0, 3.0, 0.0]);
}
