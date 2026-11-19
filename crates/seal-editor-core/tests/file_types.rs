//! Opening and dragging each kind of Asset Browser file.

use seal_editor_core::asset_editor::ASSET_DRAFT;
use seal_editor_core::{CommandArgs, EditorSession, Layout};
use std::fs;
use std::path::PathBuf;

fn project(tag: u64) -> (PathBuf, EditorSession) {
    let dir = std::env::temp_dir().join(format!("seal-file-types-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let project = dir.join("P");
    fs::create_dir_all(project.join("fx")).unwrap();
    let id = format!("{:08x}-0000-7000-9044-{:012x}", std::process::id(), 60 + tag);
    fs::write(project.join("project.seal"), format!("schema_version = 1\nname = \"P\"\nid = \"{id}\"\ncreated = 1794814358\neditor_version = \"0.01+7bdf2aadb05a\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n")).unwrap();
    fs::write(project.join("scene.seal"), "scene\n").unwrap();
    fs::write(project.join("pic.png"), b"not really a picture").unwrap();
    fs::write(project.join("notes.txt"), "hello\n").unwrap();
    fs::write(
        project.join("beep.wav"),
        seal_audio::Sound::tone(22050, 440.0, 0.25, 0.5).to_wav16(),
    )
    .unwrap();
    fs::write(project.join("fx/Sparks.fx"), seal_world::particles::EmitterDef::default().to_text())
        .unwrap();
    let path = project.to_string_lossy().to_string();
    let mut s = EditorSession::new(Layout::default());
    s.dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())])).unwrap();
    (project, s)
}

fn open(s: &mut EditorSession, path: &str) -> Result<(), String> {
    s.dispatch("content.open_file", &CommandArgs::new(&[("path", path)])).map_err(|e| e.to_string())
}

#[test]
fn media_and_text_files_open_in_the_built_in_asset_editor() {
    let (project, mut s) = project(1);
    for file in ["pic.png", "beep.wav", "fx/Sparks.fx", "notes.txt"] {
        open(&mut s, file).unwrap();
        assert_eq!(s.draft(ASSET_DRAFT).as_deref(), Some(file));
    }
    // Effects are edited value by value and saved straight away.
    open(&mut s, "fx/Sparks.fx").unwrap();
    s.dispatch("asset.fx_set", &CommandArgs::new(&[("key", "rate"), ("value", "33")])).unwrap();
    let text = fs::read_to_string(project.join("fx/Sparks.fx")).unwrap();
    let def = seal_world::particles::EmitterDef::parse(&text).unwrap();
    assert_eq!(def.rate, 33.0);
    s.dispatch("asset.fx_reset", &CommandArgs::default()).unwrap();
    let def = seal_world::particles::EmitterDef::parse(
        &fs::read_to_string(project.join("fx/Sparks.fx")).unwrap(),
    )
    .unwrap();
    assert_eq!(def, seal_world::particles::EmitterDef::default());
    // Text saves as it is typed.
    open(&mut s, "notes.txt").unwrap();
    s.dispatch("asset.text_save", &CommandArgs::new(&[("text", "changed")])).unwrap();
    assert_eq!(fs::read_to_string(project.join("notes.txt")).unwrap(), "changed");
    // Sounds play through the editor and can be stopped.
    open(&mut s, "beep.wav").unwrap();
    s.dispatch("asset.sound_play", &CommandArgs::default()).unwrap();
    assert!(s.audio_playing.is_some());
    assert!(s.audio_request.take().is_some());
    s.dispatch("asset.sound_stop", &CommandArgs::default()).unwrap();
    assert!(s.audio_playing.is_none());
    // Escaping the project is refused.
    assert!(open(&mut s, "../x.png").is_err());
}

#[test]
fn scenes_projects_and_folders_open_where_they_belong() {
    let (project, mut s) = project(2);
    fs::write(project.join("Level 2.seal"), s.world.to_text()).unwrap();
    open(&mut s, "Level 2.seal").unwrap();
    assert_eq!(s.scene_file_name(), "Level 2.seal");
    open(&mut s, "project.seal").unwrap();
    assert!(s.project.is_some(), "the open project stays open");
    open(&mut s, "fx").unwrap();
    fs::write(project.join("intro.seq"), "").unwrap();
    open(&mut s, "intro.seq").unwrap_or(());
}

#[test]
fn dropping_effects_graphs_and_prefabs_does_the_matching_thing() {
    let (project, mut s) = project(3);
    let before = s.world.entities.len();
    let fx = project.join("fx/Sparks.fx").to_string_lossy().to_string();
    s.dispatch("scene.add", &CommandArgs::new(&[("kind", &format!("fx:{fx}"))])).unwrap();
    assert_eq!(s.world.entities.len(), before + 1);
    let id = s.models.selection.clone().unwrap();
    assert_eq!(s.world.entity(&id).unwrap().particles, "fx/Sparks.fx");
    fs::write(project.join("Brain.graph"), "graph\n").unwrap();
    s.dispatch("scene.add", &CommandArgs::new(&[("kind", "graph:Brain.graph")])).unwrap();
    assert_eq!(s.world.entity(&id).unwrap().script, "Brain.graph");
    s.dispatch("scene.make_prefab", &CommandArgs::new(&[("name", "Spark")])).unwrap();
    let count = s.world.entities.len();
    s.dispatch("scene.add", &CommandArgs::new(&[("kind", "prefab:prefabs/Spark.prefab")])).unwrap();
    assert!(s.world.entities.len() > count);
}
