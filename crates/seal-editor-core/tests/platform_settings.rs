//! Save Games stores the encryption key and the cloud provider on the project.

use seal_editor_core::{CommandArgs, EditorSession, Layout};
use seal_world::savegame;

fn run(s: &mut EditorSession, id: &str, pairs: &[(&str, &str)]) {
    s.dispatch(id, &CommandArgs::new(pairs)).unwrap_or_else(|e| panic!("{id}: {e}"));
}

#[test]
fn save_games_keeps_the_key_and_the_cloud_endpoint() {
    let dir = std::env::temp_dir().join(format!("seal-platform-editor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let folder = dir.join("Game");
    let mut s = EditorSession::new(Layout::default());
    s.recent_path = Some(dir.join("recent.seal"));
    run(&mut s, "file.new-project", &[("path", folder.to_str().unwrap()), ("name", "Game")]);

    run(&mut s, "asset.save_key", &[("value", "project key")]);
    run(&mut s, "asset.save_encrypt", &[("value", "true")]);
    run(&mut s, "asset.cloud_save", &[("value", "true")]);
    run(&mut s, "asset.cloud_provider", &[("value", "http")]);
    run(&mut s, "asset.cloud_url", &[("value", "http://127.0.0.1:9")]);
    // An empty endpoint is stored and is not an error.
    run(&mut s, "asset.cloud_url", &[("value", "")]);
    run(&mut s, "asset.cloud_url", &[("value", "http://127.0.0.1:9")]);

    assert!(savegame::project_flag(&folder, "save_encrypt"));
    assert_eq!(savegame::project_text(&folder, "encryption_key"), "project key");
    assert!(savegame::project_flag(&folder, "cloud_save"));
    assert_eq!(savegame::project_text(&folder, "cloud_provider"), "http");
    assert_eq!(savegame::project_text(&folder, "cloud_url"), "http://127.0.0.1:9");
    assert!(s
        .dispatch("asset.cloud_provider", &CommandArgs::new(&[("value", "carrier")]))
        .is_err());

    let mut world = seal_world::World::starter();
    world.save_game(&folder, "Slot A").unwrap();
    let text = std::fs::read_to_string(folder.join("saved/games/Slot A.sealsave")).unwrap();
    assert!(text.starts_with("seal enc 1"), "{text}");
    run(&mut s, "project.saves", &[]);
    let view = seal_editor_core::save_editor::save_view(&s).unwrap();
    assert!(view.encrypt);
    assert_eq!(view.key, "project key");
    assert_eq!(view.provider, "http");
    assert!(view.save.is_ok(), "{:?}", view.save);
}
