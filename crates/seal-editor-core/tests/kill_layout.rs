use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use seal_editor_core::{EditorLock, WorkspaceFile};

#[test]
fn forced_kill_during_save_keeps_a_readable_layout() {
    if std::env::var("SEAL_KILL_CHILD").ok().as_deref() == Some("1") {
        let dir = PathBuf::from(std::env::var("SEAL_KILL_DIR").expect("dir"));
        let path = dir.join("layout.seal");
        let mut revision = 0u64;
        loop {
            revision = revision.saturating_add(1);
            let mut file = WorkspaceFile::fresh();
            file.set_revision(revision.saturating_sub(1));
            let mut layout = file.layout();
            if let seal_editor_core::DockNode::Split { ratio, .. } = &mut layout.root {
                *ratio = 0.2 + ((revision % 50) as f32) / 500.0;
            }
            file.set_layout(&layout);
            if file.save(&path).is_err() {
                break;
            }
        }
        return;
    }

    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("seal-kill-{nanos}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .arg("forced_kill_during_save_keeps_a_readable_layout")
        .arg("--exact")
        .env("SEAL_KILL_CHILD", "1")
        .env("SEAL_KILL_DIR", &dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();

    let path = dir.join("layout.seal");
    let backup = dir.join("layout.seal.bak");
    let deadline = Instant::now() + Duration::from_secs(8);
    while !backup.exists() {
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("child did not complete two saves");
        }
        if let Some(status) = child.try_wait().unwrap() {
            panic!("child exited early: {status}");
        }
        thread::sleep(Duration::from_millis(10));
    }
    let pause = Duration::from_millis(20 + (nanos % 50) as u64);
    thread::sleep(pause);
    child.kill().unwrap();
    let _ = child.wait();

    let loaded = WorkspaceFile::load(&path);
    assert!(loaded.revision() >= 1);
    let _ = loaded.layout();
    let backup_text = fs::read_to_string(&backup).unwrap();
    assert!(seal_sealtext_parses(&backup_text));
    let _ = fs::remove_dir_all(&dir);
}

fn seal_sealtext_parses(text: &str) -> bool {
    // The backup is a full workspace file. Loading through the same API checks it.
    let dir = std::env::temp_dir().join(format!("seal-bak-check-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("layout.seal");
    fs::write(&path, text).unwrap();
    let loaded = WorkspaceFile::load(&path);
    let ok = loaded.origin() == seal_editor_core::LoadOrigin::Primary;
    let _ = fs::remove_dir_all(&dir);
    ok
}

#[test]
fn a_second_process_cannot_lock_the_same_state() {
    if std::env::var("SEAL_LOCK_CHILD").ok().as_deref() == Some("1") {
        let dir = PathBuf::from(std::env::var("SEAL_LOCK_DIR").expect("dir"));
        let _lock = EditorLock::acquire(&dir).unwrap();
        fs::write(dir.join("ready"), b"1").unwrap();
        while !dir.join("release").exists() {
            thread::sleep(Duration::from_millis(10));
        }
        return;
    }

    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("seal-lock-{nanos}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .arg("a_second_process_cannot_lock_the_same_state")
        .arg("--exact")
        .env("SEAL_LOCK_CHILD", "1")
        .env("SEAL_LOCK_DIR", &dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !dir.join("ready").exists() {
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("lock child did not start");
        }
        thread::sleep(Duration::from_millis(10));
    }
    let blocked = EditorLock::acquire(&dir);
    assert!(blocked.is_err(), "second process took the lock");
    fs::write(dir.join("release"), b"1").unwrap();
    let status = child.wait().unwrap();
    assert!(status.success());
    EditorLock::acquire(&dir).unwrap();
    let _ = fs::remove_dir_all(&dir);
}
