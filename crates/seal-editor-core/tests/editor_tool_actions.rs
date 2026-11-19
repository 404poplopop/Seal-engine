//! Toolbar commands, shortcut profile files, and worker-count resolution.

use std::path::{Path, PathBuf};

use seal_editor_core::{
    compare_saved_enabled, import_profile, open_external_enabled, worker_thread_count, Chord,
    CommandArgs, EditorSession, HostAction, Layout, DIFF_TOOL_PATH, EXTERNAL_EDITOR_ARGS,
    EXTERNAL_EDITOR_PATH, WORKER_THREADS,
};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("seal-tools-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn project_session(dir: &Path) -> (EditorSession, PathBuf) {
    let folder = dir.join("game");
    let mut session = EditorSession::new(Layout::default());
    session.recent_path = Some(dir.join("recent.seal"));
    session.keymap_path = Some(dir.join("keymap.seal"));
    session
        .dispatch(
            "file.new-project",
            &CommandArgs::new(&[("path", folder.to_str().unwrap()), ("name", "Game")]),
        )
        .unwrap();
    (session, folder)
}

fn set_user(session: &mut EditorSession, id: &str, value: &str) {
    session
        .dispatch("settings.set", &CommandArgs::new(&[("id", id), ("value", value)]))
        .unwrap();
}

#[test]
fn toolbar_commands_build_argument_lists_and_honor_disabled_states() {
    let dir = scratch("argv");
    let (mut session, folder) = project_session(&dir);
    let notes = folder.join("notes");
    std::fs::create_dir_all(&notes).unwrap();
    let rel = "notes/readme.txt";
    std::fs::write(folder.join(rel), "hello").unwrap();
    std::fs::write(folder.join("art.png"), b"\0").unwrap();
    session.dispatch("asset.open", &CommandArgs::new(&[("path", rel)])).unwrap();
    assert!(!open_external_enabled(&session));
    assert!(!compare_saved_enabled(&session));
    set_user(&mut session, EXTERNAL_EDITOR_PATH, "/bin/editor");
    set_user(&mut session, EXTERNAL_EDITOR_ARGS, "-g {file}:{line}");
    set_user(&mut session, DIFF_TOOL_PATH, "/bin/diff");
    assert!(open_external_enabled(&session));
    assert!(!compare_saved_enabled(&session), "unchanged text stays off");
    session
        .dispatch(
            "asset.open_external",
            &CommandArgs::new(&[("path", rel), ("line", "9")]),
        )
        .unwrap();
    match session.host_action.take() {
        Some(HostAction::RunExternal { exe, args }) => {
            assert_eq!(exe, "/bin/editor");
            assert_eq!(args[0], "-g");
            assert!(args[1].ends_with("notes/readme.txt:9"), "{}", args[1]);
        }
        other => panic!("expected an external editor command, got {other:?}"),
    }
    std::fs::write(folder.join(rel), "hello\nchanged").unwrap();
    assert!(compare_saved_enabled(&session));
    session
        .dispatch("asset.compare_saved", &CommandArgs::new(&[("path", rel)]))
        .unwrap();
    match session.host_action.take() {
        Some(HostAction::RunExternal { exe, args }) => {
            assert_eq!(exe, "/bin/diff");
            assert_eq!(args.len(), 2, "{args:?}");
            assert_eq!(std::fs::read_to_string(&args[0]).unwrap(), "hello");
            assert_eq!(std::fs::read_to_string(&args[1]).unwrap(), "hello\nchanged");
        }
        other => panic!("expected a diff command, got {other:?}"),
    }
    session.dispatch("asset.open", &CommandArgs::new(&[("path", "art.png")])).unwrap();
    assert!(!open_external_enabled(&session));
    assert!(!compare_saved_enabled(&session));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn profile_export_then_import_round_trips_and_a_damaged_file_is_rejected() {
    let dir = scratch("profile");
    let (mut session, _) = project_session(&dir);
    session
        .keymap
        .override_bind("file.quit", Chord::parse("ctrl+shift+q").unwrap())
        .unwrap();
    session
        .dispatch("keymap.profile.save", &CommandArgs::new(&[("name", "work")]))
        .unwrap();
    session
        .dispatch("keymap.profile.export", &CommandArgs::new(&[("name", "work")]))
        .unwrap();
    assert!(matches!(session.host_action, Some(HostAction::SaveProfile { ref name }) if name == "work"));
    session.host_action = None;
    session
        .dispatch("keymap.profile.import", &CommandArgs::new(&[("name", "work")]))
        .unwrap();
    assert!(matches!(session.host_action, Some(HostAction::OpenProfile { .. })));
    session.host_action = None;
    let exported = dir.join("work-out.seal");
    session
        .dispatch(
            "keymap.profile.export",
            &CommandArgs::new(&[("name", "work"), ("path", exported.to_str().unwrap())]),
        )
        .unwrap();
    session
        .dispatch(
            "keymap.profile.import",
            &CommandArgs::new(&[
                ("path", exported.to_str().unwrap()),
                ("name", "again"),
            ]),
        )
        .unwrap();
    session
        .dispatch("keymap.profile.switch", &CommandArgs::new(&[("name", "again")]))
        .unwrap();
    assert_eq!(
        session.keymap.command_for(&Chord::parse("ctrl+shift+q").unwrap()),
        Some("file.quit"),
    );
    let damaged = dir.join("damaged.seal");
    std::fs::write(&damaged, "{{{ not a profile").unwrap();
    let user_data = session.keymap_path.as_ref().unwrap().parent().unwrap();
    assert!(import_profile(user_data, &damaged, "broken").is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn worker_count_resolves_auto_and_a_fixed_number() {
    let dir = scratch("workers");
    let (mut session, _) = project_session(&dir);
    set_user(&mut session, WORKER_THREADS, "3");
    assert_eq!(worker_thread_count(&session.foundation.settings), 3);
    assert_eq!(seal_editor_core::active_workers(), 3);
    set_user(&mut session, WORKER_THREADS, "auto");
    let auto = std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, 64);
    assert_eq!(worker_thread_count(&session.foundation.settings), auto);
    assert_eq!(seal_editor_core::active_workers(), auto);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn changing_a_bus_volume_marks_the_live_mixer_for_update() {
    let dir = scratch("audio");
    let (mut session, _) = project_session(&dir);
    assert!(!session.audio_apply);
    set_user(&mut session, seal_editor_core::MASTER_VOLUME, "40");
    assert!(session.audio_apply);
    session.audio_apply = false;
    session.dispatch("play.start", &CommandArgs::default()).unwrap();
    assert!(session.audio_apply);
    let _ = std::fs::remove_dir_all(&dir);
}
