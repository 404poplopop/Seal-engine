//! Multi-file preset apply is a single undo step.

use std::fs;

use seal_editor_core::{CommandArgs, EditorSession, Layout};

#[test]
fn multi_file_apply_is_one_undo_step() {
    let dir = std::env::temp_dir().join(format!("seal-import-undo-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let project = dir.join("P");
    fs::create_dir_all(project.join("sprites")).unwrap();
    let id = format!("{:08x}-0000-7000-9044-{:012x}", std::process::id(), 20);
    fs::write(
        project.join("project.seal"),
        format!(
            "schema_version = 1\nname = \"P\"\nid = \"{id}\"\ncreated = 1794814358\neditor_version = \"0.01+7bdf2aadb05a\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n"
        ),
    )
    .unwrap();
    fs::write(project.join("sprites/a.png"), b"a").unwrap();
    fs::write(project.join("sprites/b.png"), b"b").unwrap();
    let path = project.to_string_lossy().to_string();
    let mut session = EditorSession::new(Layout::default());
    session.dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())])).unwrap();
    session
        .dispatch(
            "import.preset.apply",
            &CommandArgs::new(&[
                ("preset", "Pixel Art"),
                ("paths", "sprites/a.png\nsprites/b.png"),
            ]),
        )
        .unwrap();
    assert_eq!(session.past.len(), 1, "batch apply is one undo step");
    let a = seal_editor_core::import_presets::load_file_state(
        &project,
        "sprites/a.png",
        seal_editor_core::import_presets::ImportKind::Picture,
    );
    let b = seal_editor_core::import_presets::load_file_state(
        &project,
        "sprites/b.png",
        seal_editor_core::import_presets::ImportKind::Picture,
    );
    let preset = session.models.import_presets.preset_by_name("Pixel Art").unwrap();
    assert_eq!(a.settings, preset.settings);
    assert_eq!(b.settings, preset.settings);
    session.dispatch("edit.undo", &CommandArgs::default()).unwrap();
    assert!(!project.join("sprites/a.png.import.seal").exists());
    assert!(!project.join("sprites/b.png.import.seal").exists());
    session
        .dispatch(
            "import.preset.field",
            &CommandArgs::new(&[
                ("path", "sprites/a.png"),
                ("field", "filter"),
                ("value", "linear"),
            ]),
        )
        .unwrap();
    assert_eq!(session.past.len(), 1, "one field edit is one undo step");
    let edited = seal_editor_core::import_presets::picture_look(&project.join("sprites/a.png"));
    assert!(!edited.nearest);
    session.dispatch("edit.undo", &CommandArgs::default()).unwrap();
    assert!(seal_editor_core::import_presets::picture_look(&project.join("sprites/a.png")).nearest);
    let _ = fs::remove_dir_all(&dir);
}
