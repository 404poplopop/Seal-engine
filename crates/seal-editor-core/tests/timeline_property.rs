use seal_editor_core::{CommandArgs, EditorSession, Layout};
use seal_world::seal_scene::sequence::Sequence;

fn session_with_sequence() -> (EditorSession, std::path::PathBuf, String) {
    let dir = std::env::temp_dir().join(format!("seal-prop-timeline-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let project = seal_editor_core::create_project(&dir, "TimelineProps").unwrap();
    std::fs::create_dir_all(dir.join("cutscenes")).unwrap();
    let mut session = EditorSession::new(Layout::default());
    let id = session.world.entities[1].id.clone();
    let name = session.world.entities[1].name.clone();
    std::fs::write(
        dir.join("cutscenes/intro.seq"),
        format!("length 2\nobject {name}\ntrack position.x linear\nkey 0 0\nkey 2 8\n"),
    )
    .unwrap();
    session.project = Some(project);
    session.world.entities[1].camera2d.zoom = 1.0;
    (session, dir, id)
}

#[test]
fn property_tracks_key_move_ease_delete_undo_and_stay_on_disk() {
    let (mut session, dir, id) = session_with_sequence();
    let path = "cutscenes/intro.seq";
    session
        .dispatch(
            "sequence.property_add",
            &CommandArgs::new(&[("path", path), ("entity", &id), ("field", "camera2d.zoom")]),
        )
        .unwrap();
    session
        .dispatch(
            "sequence.property_key",
            &CommandArgs::new(&[
                ("path", path),
                ("entity", &id),
                ("field", "camera2d.zoom"),
                ("t", "0"),
            ]),
        )
        .unwrap();
    session.world.entities[1].camera2d.zoom = 4.0;
    session
        .dispatch(
            "sequence.property_key",
            &CommandArgs::new(&[
                ("path", path),
                ("entity", &id),
                ("field", "camera2d.zoom"),
                ("t", "2"),
            ]),
        )
        .unwrap();
    session
        .dispatch(
            "sequence.property_ease",
            &CommandArgs::new(&[
                ("path", path),
                ("entity", &id),
                ("field", "camera2d.zoom"),
                ("t", "2"),
                ("ease", "ease-in"),
            ]),
        )
        .unwrap();
    session
        .dispatch(
            "sequence.property_move",
            &CommandArgs::new(&[
                ("path", path),
                ("entity", &id),
                ("field", "camera2d.zoom"),
                ("from", "2"),
                ("to", "1.5"),
            ]),
        )
        .unwrap();

    let saved = std::fs::read_to_string(dir.join(path)).unwrap();
    let seq = Sequence::parse(&saved).unwrap();
    assert!(seq.sample(1.0).iter().any(|(object, target, _)| {
        *target == "position.x" && session.world.entities.iter().any(|e| e.name == *object)
    }));
    let track = seq.properties.iter().find(|track| track.path == "camera2d.zoom").unwrap();
    assert_eq!(track.keys.len(), 2);
    assert!((track.keys[1].time - 1.5).abs() < 1e-4);
    assert_eq!(track.keys[1].ease, seal_world::seal_scene::sequence::Ease::EaseIn);

    session.dispatch("edit.undo", &CommandArgs::default()).unwrap();
    let undone = Sequence::parse(&std::fs::read_to_string(dir.join(path)).unwrap()).unwrap();
    let track = undone.properties.iter().find(|track| track.path == "camera2d.zoom").unwrap();
    assert!((track.keys[1].time - 2.0).abs() < 1e-4);

    session.dispatch("edit.redo", &CommandArgs::default()).unwrap();
    let redone = Sequence::parse(&std::fs::read_to_string(dir.join(path)).unwrap()).unwrap();
    assert!((redone.properties[0].keys[1].time - 1.5).abs() < 1e-4);

    session
        .dispatch(
            "sequence.property_delete",
            &CommandArgs::new(&[
                ("path", path),
                ("entity", &id),
                ("field", "camera2d.zoom"),
                ("t", "0"),
            ]),
        )
        .unwrap();
    session
        .dispatch(
            "sequence.property_delete",
            &CommandArgs::new(&[
                ("path", path),
                ("entity", &id),
                ("field", "camera2d.zoom"),
                ("t", "1.5"),
            ]),
        )
        .unwrap();
    let cleared = Sequence::parse(&std::fs::read_to_string(dir.join(path)).unwrap()).unwrap();
    assert!(cleared.properties.is_empty());
    assert_eq!(cleared.tracks.len(), 1);

    drop(session);
    let mut again = EditorSession::new(Layout::default());
    again.project = Some(seal_editor_core::open_project(&dir).unwrap());
    let loaded = again.load_sequence(path).unwrap();
    assert!(loaded.properties.is_empty());
    assert!(loaded.sample(1.0).iter().any(|(_, target, value)| *target == "position.x" && *value > 0.0));
    let _ = std::fs::remove_dir_all(&dir);
}
