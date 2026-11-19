//! Inspector list fields: undo steps and multi-selection.

use seal_editor_core::{CommandArgs, EditorSession, Layout};

fn field_value(session: &EditorSession, id: &str, field: &str) -> String {
    session
        .models
        .nodes
        .iter()
        .find(|n| n.id == id)
        .and_then(|n| n.fields.iter().find(|(k, _)| k == field).map(|(_, v)| v.clone()))
        .unwrap_or_default()
}

#[test]
fn list_edits_each_push_one_undo_step() {
    let mut session = EditorSession::new(Layout::default());
    let id = session.world.entities[2].id.clone();
    session
        .dispatch(
            "inspector.set",
            &CommandArgs::new(&[
                ("id", id.as_str()),
                ("field", "shape_points"),
                ("value", "0 0; 1 0"),
            ]),
        )
        .unwrap();
    let run = |session: &mut EditorSession, op: &str, index: usize| {
        session
            .dispatch(
                "inspector.list_edit",
                &CommandArgs::new(&[
                    ("id", id.as_str()),
                    ("field", "shape_points"),
                    ("op", op),
                    ("index", &index.to_string()),
                ]),
            )
            .unwrap();
    };
    run(&mut session, "add", 0);
    assert_eq!(session.past.len(), 2);
    run(&mut session, "remove", 2);
    assert_eq!(session.past.len(), 3);
    session.dispatch("edit.undo", &CommandArgs::default()).unwrap();
    session.dispatch("edit.undo", &CommandArgs::default()).unwrap();
    assert_eq!(field_value(&session, &id, "shape_points"), "0 0; 1 0");
}

#[test]
fn mixed_multi_selection_gets_the_same_list_edit() {
    let mut session = EditorSession::new(Layout::default());
    let a = session.world.entities[2].id.clone();
    let b = session.world.entities[3].id.clone();
    session
        .dispatch(
            "inspector.set",
            &CommandArgs::new(&[("id", a.as_str()), ("field", "phys_points"), ("value", "0 0")]),
        )
        .unwrap();
    session
        .dispatch(
            "inspector.set",
            &CommandArgs::new(&[("id", b.as_str()), ("field", "phys_points"), ("value", "1 0")]),
        )
        .unwrap();
    session
        .dispatch(
            "inspector.list_edit",
            &CommandArgs::new(&[
                ("ids", &format!("{a},{b}")),
                ("field", "phys_points"),
                ("op", "add"),
                ("index", "0"),
            ]),
        )
        .unwrap();
    assert!(field_value(&session, &a, "phys_points").contains("0, 0"));
    assert!(field_value(&session, &b, "phys_points").contains("0, 0"));
}

#[test]
fn damaged_point_still_serializes_valid_neighbors() {
    use seal_editor_core::{list_spec_for_field, parse_list_field, serialize_list_field};
    let spec = list_spec_for_field("phys_points").unwrap();
    let rows = parse_list_field(spec, "0 0; bad; 2 1");
    assert_eq!(rows.len(), 3);
    assert!(rows[1].damaged);
    let text = serialize_list_field(&rows);
    assert_eq!(text, "0 0; bad; 2 1");
    let restored: Vec<[f32; 2]> =
        text.split(';').filter_map(|p| seal_world::phys2d::parse_pair(p.trim())).collect();
    assert_eq!(restored, vec![[0.0, 0.0], [2.0, 1.0]]);
}
