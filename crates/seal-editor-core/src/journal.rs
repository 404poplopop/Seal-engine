use std::fs;
use std::path::Path;

use crate::document::{JournalStep, TextDocument, TextOp};
use crate::error::CoreError;
use crate::journal_crc::{self, JournalRecord};

const SCRATCH_KIND: u8 = 7;

/// Append one checksummed edit. An older line-format journal is converted first.
pub fn append_step(path: &Path, step: &JournalStep) -> Result<(), CoreError> {
    append_step_with_fault(path, step, &crate::fault_io::FaultPlan::none())
}

/// Append one edit while simulating a disk fault (tests only).
pub fn append_step_with_fault(
    path: &Path,
    step: &JournalStep,
    plan: &crate::fault_io::FaultPlan,
) -> Result<(), CoreError> {
    migrate_if_legacy(path)?;
    let seq = journal_crc::read_records(path)?.len() as u64;
    journal_crc::append_record_fault(path, &record(seq, step), plan)
}

pub fn read_steps(path: &Path) -> Result<Vec<JournalStep>, CoreError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    migrate_if_legacy(path)?;
    Ok(journal_crc::read_records(path)?.iter().filter_map(step_from_record).collect())
}

pub fn complete_records(text: &str) -> Vec<JournalStep> {
    let usable = if text.is_empty() {
        ""
    } else if text.ends_with('\n') {
        text
    } else {
        match text.rfind('\n') {
            Some(index) => &text[..=index],
            None => "",
        }
    };
    usable.lines().filter_map(decode_step).collect()
}

pub fn replay_steps(document: &mut TextDocument, steps: &[JournalStep]) {
    for step in steps {
        document.replay(step);
    }
}

pub fn discard_journal(path: &Path) -> Result<(), CoreError> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn migrate_if_legacy(path: &Path) -> Result<(), CoreError> {
    if !path.exists() {
        return Ok(());
    }
    let bytes = fs::read(path)?;
    if !is_legacy(&bytes) {
        return Ok(());
    }
    let text = String::from_utf8(bytes).map_err(|err| CoreError::new(err.to_string()))?;
    let steps = complete_records(&text);
    let old = legacy_path(path)?;
    fs::copy(path, &old)?;
    let records: Vec<JournalRecord> =
        steps.iter().enumerate().map(|(index, step)| record(index as u64, step)).collect();
    journal_crc::write_records(path, &records)
}

fn is_legacy(bytes: &[u8]) -> bool {
    if journal_crc::decode_complete(bytes).iter().any(|record| record.kind == SCRATCH_KIND) {
        return false;
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    !complete_records(text).is_empty()
}

fn legacy_path(path: &Path) -> Result<std::path::PathBuf, CoreError> {
    let name = path.file_name().ok_or_else(|| CoreError::new("path has no file name"))?;
    let mut old = name.to_os_string();
    old.push(".old");
    Ok(path.with_file_name(old))
}

fn record(seq: u64, step: &JournalStep) -> JournalRecord {
    JournalRecord { kind: SCRATCH_KIND, seq, payload: encode_step(step).into_bytes() }
}

fn step_from_record(record: &JournalRecord) -> Option<JournalStep> {
    if record.kind != SCRATCH_KIND {
        return None;
    }
    let text = std::str::from_utf8(&record.payload).ok()?;
    decode_step(text)
}

fn encode_step(step: &JournalStep) -> String {
    match step {
        JournalStep::Undo => "undo".to_string(),
        JournalStep::Redo => "redo".to_string(),
        JournalStep::Edit(op) => {
            format!("edit\t{}\t{}\t{}", op.start, escape(&op.deleted), escape(&op.inserted))
        }
    }
}

fn decode_step(line: &str) -> Option<JournalStep> {
    if line == "undo" {
        return Some(JournalStep::Undo);
    }
    if line == "redo" {
        return Some(JournalStep::Redo);
    }
    let mut parts = line.splitn(4, '\t');
    if parts.next()? != "edit" {
        return None;
    }
    let start = parts.next()?.parse().ok()?;
    let deleted = unescape(parts.next()?);
    let inserted = unescape(parts.next()?);
    Some(JournalStep::Edit(TextOp { start, deleted, inserted }))
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

fn unescape(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some(other) => out.push(other),
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn torn_tail_is_dropped_and_the_previous_edit_remains() {
        let raw = "edit\t0\t\thello\nedit\t5\t\t!";
        let steps = complete_records(raw);
        assert_eq!(steps.len(), 1);
        let mut doc = TextDocument::new("scratch");
        replay_steps(&mut doc, &steps);
        assert_eq!(doc.text, "hello");
    }

    #[test]
    fn journal_roundtrip_keeps_undo() {
        let dir = std::env::temp_dir().join(format!("seal-journal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("scratch.journal");
        let mut live = TextDocument::new("scratch");
        let first = live.edit(0, 0, "ab");
        append_step(&path, &JournalStep::Edit(first)).unwrap();
        let second = live.edit(2, 2, "c");
        append_step(&path, &JournalStep::Edit(second)).unwrap();
        assert!(live.undo());
        append_step(&path, &JournalStep::Undo).unwrap();
        let mut restored = TextDocument::new("scratch");
        replay_steps(&mut restored, &read_steps(&path).unwrap());
        assert_eq!(restored.text, live.text);
        assert!(restored.redo());
        assert_eq!(restored.text, "abc");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_old_line_journal_is_converted_and_kept() {
        let dir = std::env::temp_dir().join(format!("seal-journal-old-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("scratch.journal");
        fs::write(&path, "edit\t0\t\thello\nundo\n").unwrap();
        let steps = read_steps(&path).unwrap();
        assert_eq!(steps.len(), 2);
        let old = fs::read(dir.join("scratch.journal.old")).unwrap();
        assert_eq!(old, b"edit\t0\t\thello\nundo\n");
        append_step(&path, &JournalStep::Redo).unwrap();
        assert_eq!(read_steps(&path).unwrap().len(), 3);
        let mut doc = TextDocument::new("scratch");
        replay_steps(&mut doc, &read_steps(&path).unwrap());
        assert_eq!(doc.text, "hello");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
