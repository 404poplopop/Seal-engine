//! Delete confirmation for referenced assets, and watcher reimports in Import Activity.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use seal_editor_core::{CommandArgs, EditorSession, Layout, ModalChoice, ModalKind};

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("seal-delwatch-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let id = format!("{:08x}-0000-7000-8000-{:012x}", std::process::id(), tag.len() as u64 + 3);
    fs::write(
        dir.join("project.seal"),
        format!(
            "schema_version = 1\nname = \"P\"\nid = \"{id}\"\ncreated = 1\neditor_version = \"0\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n"
        ),
    )
    .unwrap();
    fs::write(dir.join("scene.seal"), "scene\n").unwrap();
    dir
}

fn open_project(dir: &Path) -> EditorSession {
    let path = dir.to_string_lossy().to_string();
    let mut session = EditorSession::new(Layout::default());
    session.dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())])).unwrap();
    session
}

fn project(tag: &str) -> (PathBuf, EditorSession) {
    let dir = scratch(tag);
    let session = open_project(&dir);
    (dir, session)
}

fn write_png(path: &Path, rgba: [u8; 4]) {
    let file = fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, 1, 1);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(&rgba).unwrap();
}

fn bump_mtime(path: &Path) {
    let file = fs::OpenOptions::new().write(true).open(path).unwrap();
    let next = SystemTime::now() + Duration::from_secs(8);
    file.set_modified(next).unwrap();
}

fn reference(dir: &Path, name: &str, target: &str) {
    fs::write(dir.join(name), format!("sprite = \"{target}\"\n")).unwrap();
}

/// The catalog wraps filled-in values in invisible direction marks.
fn visible(text: &str) -> String {
    text.chars().filter(|ch| *ch != '\u{2068}' && *ch != '\u{2069}').collect()
}

#[test]
fn referenced_delete_waits_until_confirm_and_cancel_keeps_the_file() {
    let (dir, mut session) = project("confirm");
    fs::write(dir.join("used-pic.png"), b"pic").unwrap();
    reference(&dir, "use-it.seal", "used-pic.png");

    session.dispatch("content.delete", &CommandArgs::new(&[("name", "used-pic.png")])).unwrap();
    assert!(dir.join("used-pic.png").is_file());
    assert_eq!(session.models.pending_delete_path.as_deref(), Some("used-pic.png"));
    let modal = session.foundation.modal.clone().expect("pending confirmation");
    assert_eq!(modal.kind, ModalKind::DeleteAssetInUse);
    assert_eq!(modal.confirm, "Delete");
    assert_eq!(modal.cancel, "Cancel");
    assert!(modal.alternate.is_empty());
    assert!(modal.body.contains("use-it.seal — line 1: used-pic.png"));
    assert!(!modal.body.contains("and "));

    session.choose_modal(ModalChoice::Cancel);
    assert!(session.foundation.modal.is_none());
    assert!(session.models.pending_delete_path.is_none());
    assert!(dir.join("used-pic.png").is_file());

    session.dispatch("content.delete", &CommandArgs::new(&[("name", "./used-pic.png")])).unwrap();
    assert!(dir.join("used-pic.png").is_file());
    assert_eq!(session.models.pending_delete_path.as_deref(), Some("./used-pic.png"));
    session.choose_modal(ModalChoice::Confirm);
    assert!(!dir.join("used-pic.png").exists());
    assert!(dir.join(".trash").join("used-pic.png").is_file());
}

#[test]
fn unreferenced_delete_is_immediate() {
    let (dir, mut session) = project("free");
    fs::write(dir.join("alone.png"), b"pic").unwrap();
    session.dispatch("content.delete", &CommandArgs::new(&[("name", "alone.png")])).unwrap();
    assert!(session.foundation.modal.is_none());
    assert!(session.models.pending_delete_path.is_none());
    assert!(!dir.join("alone.png").exists());
    assert!(dir.join(".trash").join("alone.png").is_file());
}

#[test]
fn a_referenced_folder_asks_before_delete() {
    let (dir, mut session) = project("folder");
    fs::create_dir_all(dir.join("pack")).unwrap();
    fs::write(dir.join("pack").join("a.png"), b"pic").unwrap();
    reference(&dir, "use-pack.seal", "pack/a.png");
    session.dispatch("content.delete", &CommandArgs::new(&[("name", "./pack")])).unwrap();
    assert!(dir.join("pack").join("a.png").is_file());
    assert_eq!(session.models.pending_delete_path.as_deref(), Some("./pack"));
    let modal = session.foundation.modal.clone().expect("folder confirmation");
    assert_eq!(modal.kind, ModalKind::DeleteAssetInUse);
    assert!(modal.body.contains("use-pack.seal"));
    assert!(modal.body.contains("pack/a.png"));
    session.choose_modal(ModalChoice::Cancel);
    assert!(dir.join("pack").join("a.png").is_file());
}

#[test]
fn more_than_twenty_users_end_with_and_n_more() {
    let (dir, mut session) = project("many");
    fs::write(dir.join("used-pic.png"), b"pic").unwrap();
    for index in 0..21 {
        reference(&dir, &format!("use-{index:02}.seal"), "used-pic.png");
    }
    session.dispatch("content.delete", &CommandArgs::new(&[("name", "used-pic.png")])).unwrap();
    assert!(dir.join("used-pic.png").is_file());
    let modal = session.foundation.modal.clone().expect("pending confirmation");
    let listed = modal.body.lines().filter(|line| line.contains(" — line ")).count();
    assert_eq!(listed, 20, "{}", modal.body);
    assert!(visible(&modal.body).contains("and 1 more"), "{}", modal.body);
    session.choose_modal(ModalChoice::Cancel);
    assert!(dir.join("used-pic.png").is_file());
}

#[test]
fn a_watcher_reimport_adds_one_activity_row() {
    let dir = scratch("watch");
    write_png(&dir.join("hero.png"), [1, 2, 3, 255]);
    fs::write(dir.join("note.txt"), b"note").unwrap();
    let mut session = open_project(&dir);
    session.tick(0.3);
    assert!(session.models.import_activity.is_empty());

    bump_mtime(&dir.join("note.txt"));
    session.tick(0.3);
    assert!(
        session.models.import_activity.is_empty(),
        "a text file is not a reimport: {:?}",
        session.models.import_activity
    );

    write_png(&dir.join("hero.png"), [9, 8, 7, 255]);
    bump_mtime(&dir.join("hero.png"));
    session.tick(0.3);
    assert_eq!(session.models.import_activity.len(), 1, "{:?}", session.models.import_activity);
    let row = &session.models.import_activity[0];
    assert_eq!(row.file, "hero.png");
    assert_eq!(row.importer, "reimport");
    assert_eq!(row.result, "ok");
    assert_eq!(row.at_ms, session.uptime_ms);
    assert!(dir.join("hero.png").is_file());
}

#[test]
fn a_damaged_picture_reimport_is_one_failed_row() {
    let dir = scratch("bad");
    fs::write(dir.join("bad.png"), b"this is not a picture").unwrap();
    let mut session = open_project(&dir);
    session.tick(0.3);
    bump_mtime(&dir.join("bad.png"));
    session.tick(0.3);
    assert_eq!(session.models.import_activity.len(), 1, "{:?}", session.models.import_activity);
    let row = &session.models.import_activity[0];
    assert_eq!(row.file, "bad.png");
    assert_eq!(row.importer, "reimport");
    assert_eq!(row.result, "failed");
    assert!(dir.join("bad.png").is_file());
    session.tick(0.3);
    assert_eq!(session.models.import_activity.len(), 1);
}
