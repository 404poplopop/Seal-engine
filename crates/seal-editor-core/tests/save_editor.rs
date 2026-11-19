//! Save games in the editor: Project > Save Games opens the newest save in
//! the Asset Editor; saves can be deleted there and the project's save
//! version raised, and older saves keep loading.

use seal_editor_core::save_editor::{play_time, save_view, slot_rel, utc_time};
use seal_editor_core::{CommandArgs, EditorSession, Layout};
use seal_world::savegame::{self, SaveGame};
use std::fs;

fn run(s: &mut EditorSession, id: &str, pairs: &[(&str, &str)]) {
    s.dispatch(id, &CommandArgs::new(pairs)).unwrap_or_else(|e| panic!("{id}: {e}"));
}

#[test]
fn saves_open_in_the_asset_editor_and_the_version_can_be_raised() {
    let dir = std::env::temp_dir().join(format!("seal-save-editor-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let folder = dir.join("Game");
    let mut s = EditorSession::new(Layout::default());
    s.recent_path = Some(dir.join("recent.seal"));
    run(&mut s, "file.new-project", &[("path", folder.to_str().unwrap()), ("name", "Game")]);

    let e = s.dispatch("project.saves", &CommandArgs::new(&[])).unwrap_err();
    assert!(e.to_string().contains("no saved games yet"), "{e}");

    let mut world = seal_world::World::starter();
    world.play_time = 125.0;
    world.save_game(&folder, "Slot A").unwrap();
    let mut save = savegame::read(&folder, "Slot A").unwrap();
    save.slot = "Slot B".into();
    save.saved_at += 60;
    save.scores.insert("coins".into(), 7);
    save.values.insert("level".into(), "2".into());
    savegame::write(&folder, &save).unwrap();

    run(&mut s, "project.saves", &[]);
    assert_eq!(s.draft("asset-path").as_deref(), Some(slot_rel("Slot B").as_str()), "newest first");
    let view = save_view(&s).unwrap();
    let b = view.save.as_ref().unwrap();
    assert_eq!(b.scores.get("coins"), Some(&7));
    assert_eq!(b.values.get("level").map(String::as_str), Some("2"));
    assert_eq!(view.slots.iter().map(|x| x.slot.as_str()).collect::<Vec<_>>(), ["Slot B", "Slot A"]);
    assert_eq!(view.project_version, 1);
    assert_eq!(play_time(b.play_time), "2:05");

    // Raising the save version keeps the project readable and the old saves loading.
    run(&mut s, "asset.save_version", &[("value", "2")]);
    assert_eq!(savegame::project_save_version(&folder), 2);
    let manifest = fs::read_to_string(folder.join("project.seal")).unwrap();
    assert!(manifest.contains("save_version = 2") && manifest.contains("name = \"Game\""), "{manifest}");
    seal_editor_core::open_project(&folder).expect("project.seal still opens");
    let view = save_view(&s).unwrap();
    assert_eq!(view.project_version, 2);
    assert_eq!(view.save.as_ref().unwrap().game_version, 1, "an older save still loads");
    assert!(s.dispatch("asset.save_version", &CommandArgs::new(&[("value", "0")])).is_err());

    // A save from a newer game version is listed with its problem.
    let newer = SaveGame { slot: "Future".into(), game_version: 9, ..save.clone() };
    savegame::write(&folder, &newer).unwrap();
    run(&mut s, "asset.open", &[("path", &slot_rel("Future"))]);
    let view = save_view(&s).unwrap();
    assert!(view.save.unwrap_err().contains("newer version of the game"));

    // Deleting the open save opens the next one.
    run(&mut s, "asset.save_delete", &[("slot", "Future")]);
    assert!(!folder.join(slot_rel("Future")).exists());
    assert_eq!(s.draft("asset-path").as_deref(), Some(slot_rel("Slot B").as_str()));
    run(&mut s, "asset.save_delete", &[("slot", "Slot A")]);
    assert_eq!(s.draft("asset-path").as_deref(), Some(slot_rel("Slot B").as_str()), "the open save stays");
    fs::remove_dir_all(dir).ok();
}

#[test]
fn times_read_as_dates() {
    assert_eq!(utc_time(0), "1970-01-01 00:00 UTC");
    assert_eq!(utc_time(1_794_879_600), "2026-11-17 01:40 UTC");
    assert_eq!(play_time(3725.0), "1:02:05");
}
