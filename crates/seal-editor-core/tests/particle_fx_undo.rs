//! Burst edit/remove on effects: file text and single undo step.

use std::fs;

use seal_editor_core::{CommandArgs, EditorSession, Layout};
use seal_world::particles::EmitterDef;

fn project() -> (std::path::PathBuf, EditorSession) {
    let dir = std::env::temp_dir().join(format!("seal-fx-burst-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let project = dir.join("P");
    fs::create_dir_all(project.join("fx")).unwrap();
    let id = format!("{:08x}-0000-7000-9044-{:012x}", std::process::id(), 901);
    fs::write(
        project.join("project.seal"),
        format!(
            "schema_version = 1\nname = \"P\"\nid = \"{id}\"\ncreated = 1794814358\neditor_version = \"0.01+7bdf2aadb05a\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n"
        ),
    )
    .unwrap();
    fs::write(project.join("scene.seal"), "scene\n").unwrap();
  let text = "rate 0\nlifetime 1\nburst 0 5\nburst 1 3\n";
    fs::write(project.join("fx/test.fx"), text).unwrap();
    let mut session = EditorSession::new(Layout::default());
    session
        .dispatch(
            "file.open-project",
            &CommandArgs::new(&[("path", project.to_str().unwrap())]),
        )
        .unwrap();
    (project, session)
}

#[test]
fn burst_edit_and_remove_change_file_text_and_undo_once() {
    let (project, mut session) = project();
    let before_undo = session.past.len();
    session
        .dispatch(
            "asset.fx_set",
            &CommandArgs::new(&[
                ("path", "fx/test.fx"),
                ("key", "burst.set"),
                ("value", "0,0.5,8"),
            ]),
        )
        .unwrap();
    let mut text = fs::read_to_string(project.join("fx/test.fx")).unwrap();
    assert!(text.contains("burst 0.5 8"));
    let def = EmitterDef::parse(&text).unwrap();
    assert!(def.extras.bursts.iter().any(|b| (b.time - 0.5).abs() < 1e-5 && b.count == 8));

    session
        .dispatch(
            "asset.fx_set",
            &CommandArgs::new(&[
                ("path", "fx/test.fx"),
                ("key", "burst.remove"),
                ("value", "1"),
            ]),
        )
        .unwrap();
    text = fs::read_to_string(project.join("fx/test.fx")).unwrap();
    assert_eq!(EmitterDef::parse(&text).unwrap().extras.bursts.len(), 1);

    assert_eq!(session.past.len(), before_undo + 2);
    session.dispatch("edit.undo", &CommandArgs::default()).unwrap();
    session.dispatch("edit.undo", &CommandArgs::default()).unwrap();
    let restored = fs::read_to_string(project.join("fx/test.fx")).unwrap();
    assert!(restored.contains("burst 0 5"));
    assert!(restored.contains("burst 1 3"));
}

#[test]
fn rotation_fields_round_trip_through_fx_set() {
    let (project, mut session) = project();
    session
        .dispatch(
            "asset.fx_set",
            &CommandArgs::new(&[
                ("path", "fx/test.fx"),
                ("key", "rotation_start.min"),
                ("value", "-15"),
            ]),
        )
        .unwrap();
    session
        .dispatch(
            "asset.fx_set",
            &CommandArgs::new(&[
                ("path", "fx/test.fx"),
                ("key", "rotation_rate.max"),
                ("value", "120"),
            ]),
        )
        .unwrap();
    let def = EmitterDef::parse(&fs::read_to_string(project.join("fx/test.fx")).unwrap()).unwrap();
    assert_eq!(def.extras.rotation_start.0, -15.0);
    assert_eq!(def.extras.rotation_rate.1, 120.0);
    let again = EmitterDef::parse(&def.to_text()).unwrap();
    assert_eq!(def.extras.rotation_start, again.extras.rotation_start);
    assert_eq!(def.extras.rotation_rate, again.extras.rotation_rate);
}