//! Property tests for keymap, layout files, journal, and undo/redo serialization.

use std::path::PathBuf;

use proptest::prelude::*;
use seal_editor_core::{
    journal::{append_step, complete_records, read_steps, replay_steps},
    Axis, Chord, DockNode, JournalStep, Keymap, Layout, TextDocument, TextOp,
};

proptest! {
    #[test]
    fn property_keymap_file_roundtrip(extra in 0u8..12) {
        let mut keymap = Keymap::default();
        for slot in 0..extra {
            let command = format!("test.cmd.{slot}");
            let chord = Chord::parse(&format!("ctrl+alt+{slot}")).expect("chord");
            keymap.override_bind(&command, chord).expect("bind");
        }
        let text = keymap.to_text();
        let loaded = Keymap::from_text(&text).expect("load");
        prop_assert_eq!(loaded.to_text(), text);
        let reparsed = Keymap::from_text(&loaded.to_text()).expect("reload");
        prop_assert_eq!(reparsed.to_text(), loaded.to_text());
    }

    #[test]
    fn property_layout_file_roundtrip(root in dock_node(3)) {
        let layout = Layout { root, floating: Vec::new() };
        let text = layout.to_value().to_text();
        let value = seal_sealtext::parse(&text).expect("parse layout");
        let restored = Layout::from_value(&value).expect("decode layout");
        prop_assert_eq!(restored.to_value().to_text(), text);
    }

    #[test]
    fn property_journal_undo_redo_replay(ops in valid_text_ops(12)) {
        let dir = scratch_dir("journal-prop");
        let path = dir.join("scratch.journal");
        let mut live = TextDocument::new("scratch");
        let mut scripted = Vec::new();
        for (pos, delete, insert) in &ops {
            let op = live.edit(*pos, *pos + *delete, insert);
            append_step(&path, &JournalStep::Edit(op.clone())).expect("append");
            scripted.push(JournalStep::Edit(op));
        }
        if live.undo() {
            append_step(&path, &JournalStep::Undo).expect("append undo");
            scripted.push(JournalStep::Undo);
        }
        if live.redo() {
            append_step(&path, &JournalStep::Redo).expect("append redo");
            scripted.push(JournalStep::Redo);
        }
        let read = read_steps(&path).expect("read");
        prop_assert_eq!(&read, &scripted);
        let mut replayed = TextDocument::new("scratch");
        replay_steps(&mut replayed, &read);
        prop_assert_eq!(replayed.text, live.text);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn property_journal_line_encoding_roundtrip(ops in prop::collection::vec(text_op(), 0..16)) {
        let mut lines = String::new();
        for op in &ops {
            lines.push_str(&encode_edit(op));
            lines.push('\n');
        }
        let decoded: Vec<_> = complete_records(&lines);
        prop_assert_eq!(decoded.len(), ops.len());
        for (step, op) in decoded.iter().zip(ops.iter()) {
            match step {
                JournalStep::Edit(restored) => prop_assert_eq!(restored, op),
                _ => prop_assert!(false, "unexpected journal step"),
            }
        }
    }

    #[test]
    fn property_undo_redo_matches_journal_replay(ops in valid_text_ops(12)) {
        let dir = scratch_dir("undo-prop");
        let path = dir.join("undo.journal");
        let mut live = TextDocument::new("scratch");
        for (pos, delete, insert) in &ops {
            let recorded = live.edit(*pos, *pos + *delete, insert);
            append_step(&path, &JournalStep::Edit(recorded)).expect("append");
        }
        while live.undo() {
            append_step(&path, &JournalStep::Undo).expect("undo");
        }
        let mut halfway = TextDocument::new("scratch");
        replay_steps(&mut halfway, &read_steps(&path).expect("read"));
        let final_text = live.text.clone();
        prop_assert_eq!(halfway.text, final_text);
        while live.redo() {
            append_step(&path, &JournalStep::Redo).expect("redo");
        }
        let mut replayed = TextDocument::new("scratch");
        replay_steps(&mut replayed, &read_steps(&path).expect("read again"));
        prop_assert_eq!(replayed.text, live.text);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

fn valid_text_ops(max: usize) -> impl Strategy<Value = Vec<(usize, usize, String)>> {
    prop::collection::vec((0usize..4, "[a-z]{1,6}"), 1..max).prop_map(|raw| {
        let mut edits = Vec::new();
        let mut len = 0usize;
        for (delete, insert) in raw {
            let delete = delete.min(len);
            len = len - delete + insert.len();
            edits.push((0, delete, insert));
        }
        edits
    })
}

fn text_op() -> impl Strategy<Value = TextOp> {
    valid_text_ops(8).prop_map(|edits| {
        let (_, delete, insert) = edits.first().cloned().unwrap_or((0, 0, "a".into()));
        TextOp { start: 0, deleted: "x".repeat(delete), inserted: insert }
    })
}

fn dock_node(depth: u32) -> impl Strategy<Value = DockNode> {
    let panel = prop_oneof![
        Just("viewport"),
        Just("outliner"),
        Just("console"),
        Just("inspector"),
        Just("welcome"),
    ];
    let tabs = prop::collection::vec(panel, 1..4).prop_map(DockNode::tabs);
    tabs.prop_recursive(depth, 16, 4, |inner| {
        (any::<bool>(), 0.15f32..0.85f32, inner.clone(), inner).prop_map(
            |(horizontal, ratio, first, second)| DockNode::Split {
                axis: if horizontal { Axis::Horizontal } else { Axis::Vertical },
                ratio,
                first: Box::new(first),
                second: Box::new(second),
            },
        )
    })
}

fn encode_edit(op: &TextOp) -> String {
    format!("edit\t{}\t{}\t{}", op.start, escape(&op.deleted), escape(&op.inserted))
}

fn escape(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            ch => out.push(ch),
        }
    }
    out
}

fn scratch_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!("seal-{label}-{}-{nanos}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("temp dir");
    path
}
