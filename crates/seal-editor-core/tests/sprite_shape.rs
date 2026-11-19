//! Creating a sprite shape, editing its spline, and refusing a damaged point list.

use seal_editor_core::{sprite_shape, CommandArgs, EditorSession, Layout};

fn session() -> EditorSession {
    EditorSession::new(Layout::default())
}

fn args(pairs: &[(&str, &str)]) -> CommandArgs {
    CommandArgs::new(pairs)
}

#[test]
fn create_edit_undo_and_refuse_a_damaged_list() {
    let mut session = session();
    session
        .dispatch("scene.add", &args(&[("kind", "sprite_shape")]))
        .unwrap();
    let id = session.models.selection.clone().unwrap();
    let entity = session.world.entity(&id).unwrap();
    assert!(entity.sprite_shape.on);
    assert!(entity.sprite_shape.closed);
    assert!(entity.sprite_shape.collider);
    assert!(entity.phys2d.points.len() >= 2);
    let fields: Vec<String> = entity.inspector_fields().into_iter().map(|(k, _)| k).collect();
    for key in sprite_shape::KEYS {
        assert!(fields.iter().any(|f| f == key), "{key} shows in the inspector");
    }

    session.models.set_field(&id, "ss_tint", "#FF8800");
    session.mirror_node(&id);
    assert_eq!(
        seal_editor_core::tint_hex(&session.world.entity(&id).unwrap().sprite_shape.tint),
        "#FF8800"
    );
    let before = session.world.entity(&id).unwrap().sprite_shape.points.0.len();
    session.models.set_field(&id, "ss_points", "0 0; broken");
    session.mirror_node(&id);
    assert_eq!(
        session.world.entity(&id).unwrap().sprite_shape.points.0.len(),
        before,
        "a damaged list is refused"
    );

    let start = session.world.entity(&id).unwrap().sprite_shape.points.0[0].pos;
    session
        .dispatch(
            "sprite_shape.edit",
            &args(&[
                ("id", &id),
                ("op", "point"),
                ("index", "0"),
                ("x", "1"),
                ("y", "1"),
                ("begin", "1"),
            ]),
        )
        .unwrap();
    session
        .dispatch(
            "sprite_shape.edit",
            &args(&[
                ("id", &id),
                ("op", "point"),
                ("index", "0"),
                ("x", "2"),
                ("y", "2"),
                ("begin", "0"),
            ]),
        )
        .unwrap();
    let moved = session.world.entity(&id).unwrap().sprite_shape.points.0[0].pos;
    assert!((moved[0] - 2.0).abs() < 1e-3 && (moved[1] - 2.0).abs() < 1e-3);
    assert!(session.scene_dirty, "the edit autosaves");
    session.dispatch("edit.undo", &CommandArgs::new(&[])).unwrap();
    let back = session.world.entity(&id).unwrap().sprite_shape.points.0[0].pos;
    assert!(
        (back[0] - start[0]).abs() < 1e-3 && (back[1] - start[1]).abs() < 1e-3,
        "one drag is one undo step: {back:?} vs {start:?}"
    );

    let count = session.world.entity(&id).unwrap().sprite_shape.points.0.len();
    session
        .dispatch(
            "sprite_shape.edit",
            &args(&[
                ("id", &id),
                ("op", "insert"),
                ("index", "0"),
                ("x", "0"),
                ("y", "0.5"),
                ("begin", "1"),
            ]),
        )
        .unwrap();
    assert_eq!(session.world.entity(&id).unwrap().sprite_shape.points.0.len(), count + 1);
    session
        .dispatch(
            "sprite_shape.edit",
            &args(&[("id", &id), ("op", "delete"), ("index", "0"), ("begin", "1")]),
        )
        .unwrap();
    assert_eq!(session.world.entity(&id).unwrap().sprite_shape.points.0.len(), count);
    session
        .dispatch(
            "sprite_shape.edit",
            &args(&[
                ("id", &id),
                ("op", "corner"),
                ("index", "0"),
                ("smooth", "yes"),
                ("begin", "1"),
            ]),
        )
        .unwrap();
    assert!(session.world.entity(&id).unwrap().sprite_shape.points.0[0].smooth);
}
