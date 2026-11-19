use seal_editor_core::{CommandArgs, EditorSession, Layout};
use std::fs;

#[test]
fn a_second_scene_opens_saves_the_first_and_is_remembered_on_reopen() {
    let dir = std::env::temp_dir().join(format!("seal-open-scene-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let project = dir.join("P");
    fs::create_dir_all(&project).unwrap();
    let id = format!("{:08x}-0000-7000-9044-{:012x}", std::process::id(), 31);
    fs::write(project.join("project.seal"), format!("schema_version = 1\nname = \"P\"\nid = \"{id}\"\ncreated = 1794814358\neditor_version = \"0.01+7bdf2aadb05a\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n")).unwrap();
    fs::write(project.join("scene.seal"), "scene\n").unwrap();
    let path = project.to_string_lossy().to_string();
    let mut s = EditorSession::new(Layout::default());
    s.dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())])).unwrap();
    s.dispatch("content.new_scene", &CommandArgs::new(&[])).unwrap();
    assert!(project.join("New Scene.seal").is_file());
    s.dispatch("scene.add", &CommandArgs::new(&[("kind", "cube")])).unwrap();
    let main_count = s.world.entities.len();
    s.dispatch("scene.open", &CommandArgs::new(&[("path", "New Scene.seal")])).unwrap();
    assert_eq!(s.scene_file_name(), "New Scene.seal");
    assert!(s.world.entities.len() < main_count, "the new scene is shown");
    let saved = fs::read_to_string(project.join("scene.seal")).unwrap();
    assert!(saved.len() > "scene\n".len(), "the main scene was saved before switching");
    s.dispatch("scene.add", &CommandArgs::new(&[("kind", "sphere")])).unwrap();
    s.save_scene().unwrap();
    assert!(fs::read_to_string(project.join("New Scene.seal")).unwrap().contains("Sphere"));
    assert!(s.dispatch("scene.open", &CommandArgs::new(&[("path", "../x.seal")])).is_err());
    drop(s);

    let mut again = EditorSession::new(Layout::default());
    again.dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())])).unwrap();
    assert_eq!(again.scene_file_name(), "New Scene.seal", "reopens on the last scene");
    again.dispatch("scene.open", &CommandArgs::new(&[("path", "scene.seal")])).unwrap();
    assert_eq!(again.world.entities.len(), main_count);
    let _ = fs::remove_dir_all(&dir);
}
