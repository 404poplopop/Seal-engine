use seal_editor_core::{CommandArgs, EditorSession, Layout};
use std::fs;

fn project() -> (std::path::PathBuf, EditorSession) {
    let dir = std::env::temp_dir().join(format!(
        "seal-scene-pack-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    let project = dir.join("P");
    fs::create_dir_all(&project).unwrap();
    let id = format!("{:08x}-0000-7000-9044-{:012x}", std::process::id(), 14);
    fs::write(
        project.join("project.seal"),
        format!(
            "schema_version = 1\nname = \"P\"\nid = \"{id}\"\ncreated = 1794814358\neditor_version = \"0.01+7bdf2aadb05a\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n"
        ),
    )
    .unwrap();
    fs::write(project.join("scene.seal"), seal_world::World::empty_scene_text("Level")).unwrap();
    let path = project.to_string_lossy().to_string();
    let mut session = EditorSession::new(Layout::default());
    session
        .dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())]))
        .unwrap();
    (project, session)
}

#[test]
fn merge_objects_is_one_undo_step() {
    let (_project, mut session) = project();
    session.dispatch("scene.add", &CommandArgs::new(&[("kind", "cube")])).unwrap();
    let a = session.models.selection.clone().unwrap();
    session.models.selection = None;
    session.dispatch("scene.add", &CommandArgs::new(&[("kind", "sphere")])).unwrap();
    let b = session.models.selection.clone().unwrap();
    session.foundation.selection = vec![a.clone(), b.clone()];
    session.dispatch("scene.merge_objects", &CommandArgs::default()).unwrap();
    let id = session.models.selection.clone().unwrap();
    assert_eq!(session.world.entity(&id).unwrap().merged.kind, "mesh");
    assert!(session.world.entity(&a).is_none());
    session.dispatch("edit.undo", &CommandArgs::default()).unwrap();
    assert!(session.world.entity(&a).is_some());
    assert!(session.world.entity(&b).is_some());
}

#[test]
fn post_process_and_object_files_are_in_the_editor() {
    let (project, mut session) = project();
    session.dispatch("scene.add", &CommandArgs::new(&[("kind", "post_process")])).unwrap();
    let id = session.models.selection.clone().unwrap();
    let entity = session.world.entity(&id).unwrap();
    assert_eq!(entity.volume.kind, "post");
    assert!(entity.inspector_fields().iter().any(|(key, _)| key == "post_tint"));
    session
        .dispatch("scene.object_files", &CommandArgs::new(&[("value", "yes")]))
        .unwrap();
    assert!(session.world.objects_per_file);
    let scene = project.join("scene.seal");
    let index = fs::read_to_string(&scene).unwrap();
    assert!(seal_world::object_files::is_index(&index), "{index}");
    assert!(seal_world::object_files::folder_for(&scene).join(".seal-objects").is_file());
    session
        .dispatch("scene.object_files", &CommandArgs::new(&[("value", "no")]))
        .unwrap();
    assert!(!session.world.objects_per_file);
    let single = fs::read_to_string(&scene).unwrap();
    assert!(single.contains("seal_scene"));
    assert!(!seal_world::object_files::folder_for(&scene).exists());
}
