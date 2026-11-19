//! Sound container assets: create, edit entries, and autosave.

use seal_editor_core::asset_editor::ASSET_DRAFT;
use seal_editor_core::{CommandArgs, EditorSession, Layout};
use std::fs;
use std::path::PathBuf;

fn project() -> (PathBuf, EditorSession) {
    let dir = std::env::temp_dir().join(format!("seal-soundset-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let project = dir.join("P");
    fs::create_dir_all(&project).unwrap();
    let id = format!("{:08x}-0000-7000-9044-{:012x}", std::process::id(), 88);
    fs::write(
        project.join("project.seal"),
        format!(
            "schema_version = 1\nname = \"P\"\nid = \"{id}\"\ncreated = 1794814358\neditor_version = \"0.01+7bdf2aadb05a\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n"
        ),
    )
    .unwrap();
    let path = project.to_string_lossy().to_string();
    let mut s = EditorSession::new(Layout::default());
    s.dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())]))
        .unwrap();
    (project, s)
}

fn run(s: &mut EditorSession, cmd: &str, args: &[(&str, &str)]) {
    s.dispatch(cmd, &CommandArgs::new(args))
        .unwrap_or_else(|e| panic!("{cmd}: {e}"));
}

#[test]
fn a_sound_container_is_built_and_saved_edit_by_edit() {
    let (project, mut s) = project();
    run(
        &mut s,
        "asset.soundset_new",
        &[("path", "sounds/footsteps.soundset")],
    );
    assert_eq!(s.draft(ASSET_DRAFT).as_deref(), Some("sounds/footsteps.soundset"));
    let text = fs::read_to_string(project.join("sounds/footsteps.soundset")).unwrap();
    assert!(text.contains("mode random"));

    run(
        &mut s,
        "asset.soundset_set",
        &[("key", "mode"), ("value", "sequence")],
    );
    run(
        &mut s,
        "asset.soundset_entry_add",
        &[("path", "sounds/a.wav")],
    );
    run(
        &mut s,
        "asset.soundset_entry_add",
        &[("path", "sounds/b.wav")],
    );
    let saved = fs::read_to_string(project.join("sounds/footsteps.soundset")).unwrap();
    assert!(saved.contains("mode sequence"));
    assert!(saved.contains("entry sounds/a.wav"));
    assert!(saved.contains("entry sounds/b.wav"));

    run(
        &mut s,
        "asset.soundset_entry_remove",
        &[("index", "0")],
    );
    let after = fs::read_to_string(project.join("sounds/footsteps.soundset")).unwrap();
    assert!(!after.contains("a.wav"));
    assert!(after.contains("b.wav"));
}

#[test]
fn damaged_soundset_blocks_edits() {
    let (project, mut s) = project();
    fs::create_dir_all(project.join("sounds")).unwrap();
    fs::write(project.join("sounds/bad.soundset"), "mode broken\n").unwrap();
    run(&mut s, "asset.open", &[("path", "sounds/bad.soundset")]);
    assert!(
        s.dispatch(
            "asset.soundset_set",
            &CommandArgs::new(&[("key", "volume"), ("value", "1")])
        )
        .is_err()
    );
}