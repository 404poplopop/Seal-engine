use seal_editor_core::{CommandArgs, EditorSession, Layout, PlayMode};

fn session_with_sequence() -> (EditorSession, std::path::PathBuf, String) {
    let dir = std::env::temp_dir().join(format!("seal-timeline-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let project = seal_editor_core::create_project(&dir, "TimelineTest").unwrap();
    std::fs::create_dir_all(dir.join("cutscenes")).unwrap();
    let mut session = EditorSession::new(Layout::default());
    let name = session.world.entities[1].name.clone();
    std::fs::write(
        dir.join("cutscenes/intro.seq"),
        format!(
            "length 2\nobject {name}\ntrack position.x linear\nkey 0 0\nkey 2 8\nevent 1 half\n"
        ),
    )
    .unwrap();
    session.project = Some(project);
    (session, dir, name)
}

#[test]
fn the_timeline_finds_plays_and_scrubs_a_sequence_without_touching_the_saved_scene() {
    let (mut session, dir, name) = session_with_sequence();
    assert_eq!(session.sequence_files(), vec!["cutscenes/intro.seq".to_string()]);
    let before = session.world.entities[1].translation;

    session
        .dispatch(
            "sequence.scrub",
            &CommandArgs::new(&[("path", "cutscenes/intro.seq"), ("t", "1")]),
        )
        .unwrap();
    assert_eq!(session.world.mode, PlayMode::Editing);
    assert_eq!(
        session.world.entities[1].translation, before,
        "scrubbing outside play never edits the scene"
    );
    assert_eq!(session.draft("timeline-time").as_deref(), Some("1.000"));

    session
        .dispatch("sequence.play", &CommandArgs::new(&[("path", "cutscenes/intro.seq")]))
        .unwrap();
    assert_eq!(session.world.mode, PlayMode::Playing);
    for _ in 0..70 {
        session.world.step(1.0 / 60.0);
    }
    let _ = &dir;
    let e = session.world.entities.iter().find(|e| e.name == name).unwrap();
    assert!(e.translation[0] > 3.0, "the sequence moves the object: {}", e.translation[0]);
    assert_eq!(session.world.sequence_events, vec!["half"]);

    session
        .dispatch(
            "sequence.scrub",
            &CommandArgs::new(&[("path", "cutscenes/intro.seq"), ("t", "0.5")]),
        )
        .unwrap();
    assert_eq!(session.world.mode, PlayMode::Paused);
    let e = session.world.entities.iter().find(|e| e.name == name).unwrap();
    assert!((e.translation[0] - 2.0).abs() < 1e-3);

    session.dispatch("play.stop", &CommandArgs::default()).unwrap();
    assert_eq!(session.world.entities[1].translation, before, "stop puts everything back");
    let _ = std::fs::remove_dir_all(&dir);
}
