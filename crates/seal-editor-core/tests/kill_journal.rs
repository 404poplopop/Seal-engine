use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use seal_editor_core::fault_io::{self, FaultPlan};
use seal_editor_core::journal;
use seal_editor_core::{JournalStep, TextDocument, TextOp};

#[test]
fn forced_kill_during_journal_append_keeps_complete_edits() {
    if std::env::var("SEAL_JOURNAL_CHILD").ok().as_deref() == Some("1") {
        let path = PathBuf::from(std::env::var("SEAL_JOURNAL_PATH").expect("path"));
        let mut text = String::new();
        for index in 0..20_000 {
            text.push(char::from(b'a' + (index % 26) as u8));
            let step = JournalStep::Edit(TextOp {
                start: index,
                deleted: String::new(),
                inserted: text[index..].to_string(),
            });
            if journal::append_step(&path, &step).is_err() {
                break;
            }
        }
        return;
    }

    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("seal-kill-journal-{nanos}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("scratch.journal");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .arg("forced_kill_during_journal_append_keeps_complete_edits")
        .arg("--exact")
        .env("SEAL_JOURNAL_CHILD", "1")
        .env("SEAL_JOURNAL_PATH", &path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    while journal::read_steps(&path).unwrap_or_default().len() < 2 {
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("journal child did not write");
        }
        if let Some(status) = child.try_wait().unwrap() {
            panic!("journal child exited early: {status}");
        }
        thread::sleep(Duration::from_millis(10));
    }
    thread::sleep(Duration::from_millis(30));
    child.kill().unwrap();
    let _ = child.wait();
    let steps = journal::read_steps(&path).unwrap();
    assert!(!steps.is_empty());
    let mut doc = TextDocument::new("scratch");
    journal::replay_steps(&mut doc, &steps);
    assert!(!doc.text.is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn fault_injection_mid_journal_append_keeps_prior_edits() {
    let dir = std::env::temp_dir().join(format!(
        "seal-journal-inject-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("scratch.journal");
    let mut live = TextDocument::new("scratch");
    let first = live.edit(0, 0, "keep-me");
    journal::append_step(&path, &JournalStep::Edit(first)).unwrap();
    let second = live.edit(7, 7, "!");
    let torn = journal::append_step_with_fault(
        &path,
        &JournalStep::Edit(second),
        &FaultPlan::fail_at_byte(4),
    );
    assert!(torn.is_err());
    let steps = journal::read_steps(&path).unwrap();
    assert_eq!(steps.len(), 1);
    let mut doc = TextDocument::new("scratch");
    journal::replay_steps(&mut doc, &steps);
    assert_eq!(doc.text, "keep-me");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn randomized_journal_kill_points() {
    let Some(runs) = fault_io::env_run_count() else {
        return;
    };

    let dir = std::env::temp_dir().join(format!(
        "seal-journal-fault-runs-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("scratch.journal");

    let mut live = TextDocument::new("scratch");
    for index in 0..64usize {
        let inserted = format!("line-{index}\n");
        let op = live.edit(live.text.len(), live.text.len(), &inserted);
        journal::append_step(&path, &JournalStep::Edit(op)).unwrap();
    }
    let bytes = fs::read(&path).unwrap();
    let whole = journal::read_steps(&path).unwrap();

    let mut state = 0x9E37_79B9u64;
    for _ in 0..runs {
        state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        let cut = (state as usize) % (bytes.len() + 1);
        if cut == 0 {
            let _ = fs::remove_file(&path);
        } else {
            fs::write(&path, &bytes[..cut]).unwrap();
        }
        let recovered = journal::read_steps(&path).unwrap();
        assert!(recovered.len() <= whole.len());
        assert_eq!(&whole[..recovered.len()], recovered.as_slice());
        let mut doc = TextDocument::new("scratch");
        journal::replay_steps(&mut doc, &recovered);
        let mut expected = TextDocument::new("scratch");
        journal::replay_steps(&mut expected, &recovered);
        assert_eq!(doc.text, expected.text);
        fs::write(&path, &bytes).unwrap();
    }

    let _ = fs::remove_dir_all(&dir);
}
