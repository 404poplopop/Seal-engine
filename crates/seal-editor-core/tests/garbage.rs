//! Damaged themes and recovery journals must be refused or trimmed, not crash the editor.

use seal_editor_core::journal_crc::decode_complete;
use seal_editor_core::{append_record, parse_theme, theme_text, JournalRecord, BUILTIN_THEME_IDS};
use seal_testkit::garbage::{survives, text};

#[test]
fn broken_theme_files_are_refused_not_crashed_on() {
    for id in BUILTIN_THEME_IDS {
        survives(id, theme_text(id).as_bytes(), text(|t| drop(parse_theme(t))));
    }
}

#[test]
fn a_damaged_recovery_journal_keeps_its_whole_records_and_never_crashes() {
    let dir = std::env::temp_dir().join(format!("seal-garbage-journal-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("journal.bin");
    for seq in 0..4u64 {
        append_record(
            &path,
            &JournalRecord { kind: 1, seq, payload: format!("edit {seq}").into_bytes() },
        )
        .unwrap();
    }
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(decode_complete(&bytes).len(), 4);
    survives("journal", &bytes, |b| {
        let records = decode_complete(b);
        assert!(records.len() <= 4);
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn broken_shortcut_files_are_refused_not_crashed_on() {
    let session = seal_editor_core::EditorSession::new(seal_editor_core::Layout::default());
    let mut seed = session.keymap.to_text();
    if seed.trim().is_empty() {
        seed = "bind \"file.save\" \"ctrl+s\"\n".into();
    }
    survives("keymap", seed.as_bytes(), text(|t| drop(seal_editor_core::Keymap::from_text(t))));
}

#[test]
fn a_damaged_layout_file_falls_back_instead_of_crashing() {
    let dir = std::env::temp_dir().join(format!("seal-garbage-layout-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("workspace.seal");
    seal_editor_core::WorkspaceFile::fresh().save(&path).unwrap();
    let seed = std::fs::read(&path).unwrap();
    survives("layout", &seed, |bytes| {
        std::fs::write(&path, bytes).unwrap();
        let _ = seal_editor_core::WorkspaceFile::load(&path);
    });
    let _ = std::fs::remove_dir_all(&dir);
}
