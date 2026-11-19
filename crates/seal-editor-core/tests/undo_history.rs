use seal_editor_core::{CommandArgs, EditorSession, Layout};

fn add_cube(session: &mut EditorSession) {
    session.dispatch("scene.add", &CommandArgs::new(&[("kind", "cube")])).unwrap();
}

#[test]
fn jump_matches_undo_then_redo_scene_text() {
    let mut session = EditorSession::new(Layout::default());
    add_cube(&mut session);
    add_cube(&mut session);
    add_cube(&mut session);
    let mut via_undo = EditorSession::new(Layout::default());
    add_cube(&mut via_undo);
    add_cube(&mut via_undo);
    add_cube(&mut via_undo);
    let all = via_undo.world.to_text();
    via_undo.dispatch("edit.undo", &CommandArgs::default()).unwrap();
    via_undo.dispatch("edit.undo", &CommandArgs::default()).unwrap();
    let one_cube = via_undo.world.to_text();
    session.jump_undo_history(1).unwrap();
    assert_eq!(session.world.to_text(), one_cube);
    session.jump_undo_history(3).unwrap();
    assert_eq!(session.world.to_text(), all);
}

#[test]
fn a_new_edit_after_jumping_back_clears_redo() {
    let mut session = EditorSession::new(Layout::default());
    add_cube(&mut session);
    add_cube(&mut session);
    session.jump_undo_history(1).unwrap();
    assert!(!session.future.is_empty());
    add_cube(&mut session);
    assert!(session.future.is_empty());
}

#[test]
fn scene_steps_use_the_command_title() {
    let mut session = EditorSession::new(Layout::default());
    add_cube(&mut session);
    let Some(item) = session.past.last() else {
        panic!("expected a scene undo step");
    };
    let label = session.past_action_label(item);
    let generic = session.messages.get("menu-act-scene");
    assert_ne!(label, generic);
    // An added object's step names the object it made.
    let name = session.world.entities.last().map(|e| e.name.clone()).unwrap();
    assert_eq!(label.replace(['\u{2068}', '\u{2069}'], ""), format!("Add {name}"));
}

#[test]
fn rename_steps_use_the_new_name() {
    let mut session = EditorSession::new(Layout::default());
    add_cube(&mut session);
    let id = session.world.entities.last().unwrap().id.clone();
    session
        .dispatch("outliner.rename", &CommandArgs::new(&[("id", id.as_str()), ("name", "Lamp")]))
        .unwrap();
    let label = session.past_action_label(session.past.last().unwrap());
    assert!(label.contains("Lamp"));
}
