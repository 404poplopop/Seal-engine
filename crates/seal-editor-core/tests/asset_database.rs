//! Asset index, reference rewrite, and undo.

use seal_editor_core::{CommandArgs, EditorSession, Layout, ModalChoice};
use std::fs;
use std::path::PathBuf;

fn project(tag: &str) -> (PathBuf, EditorSession) {
    let dir = std::env::temp_dir().join(format!("seal-asset-db-int-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let project = dir.join("P");
    fs::create_dir_all(project.join("sprites")).unwrap();
    let id = format!("{:08x}-0000-7000-8000-{:012x}", std::process::id(), 90);
    fs::write(
        project.join("project.seal"),
        format!(
            "schema_version = 1\nname = \"P\"\nid = \"{id}\"\ncreated = 1\neditor_version = \"0\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n"
        ),
    )
    .unwrap();
    fs::write(project.join("sprites/a.png"), b"aaa").unwrap();
    fs::write(project.join("scene.seal"), "scene\n").unwrap();
    let path = project.to_string_lossy().to_string();
    let mut s = EditorSession::new(Layout::default());
    s.dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())])).unwrap();
    if let Some(entity) = s.world.entities.get_mut(2) {
        entity.sprite = "sprites/a.png".into();
    }
    s.save_scene().unwrap();
    (project, s)
}

#[test]
fn reference_listing_and_undo_after_rename() {
    let (project, mut session) = project("undo");
    session
        .dispatch(
            "asset.find_references",
            &CommandArgs::new(&[("path", "sprites/a.png")]),
        )
        .unwrap();
    assert!(!session.models.asset_reference_report.referencers.is_empty());
    session
        .dispatch(
            "content.rename",
            &CommandArgs::new(&[("from", "sprites/a.png"), ("to", "b.png")]),
        )
        .unwrap();
    if session.foundation.modal.is_some() {
        session.choose_modal(ModalChoice::Confirm);
    }
    let scene = fs::read_to_string(project.join("scene.seal")).unwrap();
    assert!(scene.contains("sprites/b.png"));
    session.dispatch("edit.undo", &CommandArgs::default()).unwrap();
    let scene = fs::read_to_string(project.join("scene.seal")).unwrap();
    assert!(scene.contains("sprites/a.png"));
    assert!(project.join("sprites/a.png").is_file());
}