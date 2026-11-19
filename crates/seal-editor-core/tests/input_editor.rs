//! The input bindings editor: Edit > Input Bindings opens the project's
//! `input.sealinput`, and every rebind, add, remove and dead-zone change is
//! written to the file at once.

use seal_editor_core::input_editor::{capturing, read_map, Button, INPUT_FILE, INPUT_NOTE_DRAFT};
use seal_editor_core::{CommandArgs, EditorSession, Layout};
use std::fs;

fn run(s: &mut EditorSession, id: &str, pairs: &[(&str, &str)]) {
    s.dispatch(id, &CommandArgs::new(pairs)).unwrap_or_else(|e| panic!("{id}: {e}"));
}

fn names(s: &[Button]) -> Vec<&str> {
    s.iter().map(Button::name).collect()
}

#[test]
fn bindings_are_edited_and_saved_at_once() {
    let dir = std::env::temp_dir().join(format!("seal-input-editor-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let folder = dir.join("Game");
    let mut s = EditorSession::new(Layout::default());
    s.recent_path = Some(dir.join("recent.seal"));
    run(&mut s, "file.new-project", &[("path", folder.to_str().unwrap()), ("name", "Game")]);
    let file = folder.join(INPUT_FILE);
    assert!(!file.exists());

    // The menu command writes the defaults and opens them in the Asset Editor.
    run(&mut s, "project.input", &[]);
    assert!(file.is_file());
    assert_eq!(s.draft("asset-path").as_deref(), Some(INPUT_FILE));
    assert_eq!(
        seal_editor_core::asset_editor::asset_kind(INPUT_FILE),
        seal_editor_core::asset_editor::AssetKind::Input
    );
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.starts_with('#'), "the header explains the format");
    assert_eq!(
        names(read_map(&file).unwrap().bindings("jump")),
        ["key.space", "key.w", "key.arrowup", "pad.south"]
    );

    // Click a chip: it waits for a key. The key then lands in that slot.
    run(&mut s, "asset.input_capture", &[("action", "jump"), ("slot", "0")]);
    assert_eq!(capturing(&s), Some(("jump".to_string(), 0)));
    run(&mut s, "asset.input_bind", &[("button", "key.k")]);
    assert_eq!(capturing(&s), None);
    let map = read_map(&file).unwrap();
    assert_eq!(names(map.bindings("jump")), ["key.k", "key.w", "key.arrowup", "pad.south"]);
    assert_eq!(s.draft(INPUT_NOTE_DRAFT).as_deref(), Some("jump is now K"));

    // A button drives one action: taking pad.south moves it off jump.
    run(&mut s, "asset.input_capture", &[("action", "move_left"), ("slot", "9")]);
    run(&mut s, "asset.input_bind", &[("button", "pad.south")]);
    let map = read_map(&file).unwrap();
    assert!(names(map.bindings("move_left")).ends_with(&["pad.south"]));
    assert!(!names(map.bindings("jump")).contains(&"pad.south"));
    let note = s.draft(INPUT_NOTE_DRAFT).unwrap();
    assert!(note.contains("moved from jump to move_left"), "{note}");

    // Esc cancels without changing anything.
    let before = fs::read_to_string(&file).unwrap();
    run(&mut s, "asset.input_capture", &[("action", "pause"), ("slot", "0")]);
    run(&mut s, "asset.input_cancel", &[]);
    assert_eq!(capturing(&s), None);
    assert!(s.dispatch("asset.input_bind", &CommandArgs::new(&[("button", "key.q")])).is_err());
    assert_eq!(fs::read_to_string(&file).unwrap(), before);

    // Remove a binding, add and delete actions.
    run(&mut s, "asset.input_unbind", &[("action", "jump"), ("slot", "0")]);
    assert_eq!(names(read_map(&file).unwrap().bindings("jump")), ["key.w", "key.arrowup"]);
    run(&mut s, "asset.input_add", &[("name", "dash")]);
    assert!(read_map(&file).unwrap().has_action("dash"));
    assert!(s.dispatch("asset.input_add", &CommandArgs::new(&[("name", "dash")])).is_err());
    assert!(s.dispatch("asset.input_add", &CommandArgs::new(&[("name", "bad name!")])).is_err());
    run(&mut s, "asset.input_bind", &[("action", "dash"), ("slot", "0"), ("button", "key.shift")]);
    assert_eq!(names(read_map(&file).unwrap().bindings("dash")), ["key.shift"]);
    run(&mut s, "asset.input_remove", &[("action", "pause")]);
    assert!(!read_map(&file).unwrap().has_action("pause"));
    assert!(s
        .dispatch("asset.input_capture", &CommandArgs::new(&[("action", "pause"), ("slot", "0")]))
        .is_err());

    // Dead zone of the stick, clamped.
    run(&mut s, "asset.input_deadzone", &[("axis", "move_x"), ("value", "0.4")]);
    assert!(fs::read_to_string(&file).unwrap().contains("deadzone 0.4"));
    run(&mut s, "asset.input_deadzone", &[("axis", "move_x"), ("value", "3")]);
    assert!(fs::read_to_string(&file).unwrap().contains("deadzone 0.95"));
    assert!(s
        .dispatch("asset.input_deadzone", &CommandArgs::new(&[("axis", "nope"), ("value", "0.1")]))
        .is_err());

    // A play session reads the edited file.
    let controls = seal_editor_core::controls::load_project_map(&folder).unwrap();
    assert_eq!(names(controls.bindings("dash")), ["key.shift"]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_broken_bindings_file_is_reported_not_overwritten() {
    let dir = std::env::temp_dir().join(format!("seal-input-broken-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let folder = dir.join("Game");
    let mut s = EditorSession::new(Layout::default());
    s.recent_path = Some(dir.join("recent.seal"));
    run(&mut s, "file.new-project", &[("path", folder.to_str().unwrap()), ("name", "Game")]);
    fs::write(folder.join(INPUT_FILE), "action = = nonsense\n").unwrap();
    run(&mut s, "project.input", &[]);
    assert!(read_map(&folder.join(INPUT_FILE)).is_err());
    assert!(s.dispatch("asset.input_add", &CommandArgs::new(&[("name", "dash")])).is_err());
    assert_eq!(fs::read_to_string(folder.join(INPUT_FILE)).unwrap(), "action = = nonsense\n");
    let _ = fs::remove_dir_all(&dir);
}
