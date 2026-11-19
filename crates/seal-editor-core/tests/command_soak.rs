//! Random command soak.
//!
//! Default run is short and checks that resident memory stays near its start.
//! `SEAL_SOAK_LONG=1` runs 8,000 commands. `SEAL_SOAK_HOURS=8` runs until that
//! many wall-clock hours pass and requires growth under 1% per hour.
//! GitHub-hosted jobs stop at 6 hours, so the nightly job uses `SEAL_SOAK_HOURS=6`.

use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::time::{Duration, Instant};

use seal_editor_core::{CommandArgs, EditorSession, Layout};

const SHORT_ITERATIONS: usize = 160;
const LONG_ITERATIONS: usize = 8_000;

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn index(&mut self, len: usize) -> usize {
        if len == 0 {
            return 0;
        }
        (self.next_u64() as usize) % len
    }
}

fn soak_iterations() -> usize {
    if std::env::var("SEAL_SOAK_LONG").ok().as_deref() == Some("1") {
        LONG_ITERATIONS
    } else {
        SHORT_ITERATIONS
    }
}

fn soak_hours() -> Option<f64> {
    std::env::var("SEAL_SOAK_HOURS")
        .ok()
        .and_then(|text| text.parse::<f64>().ok())
        .filter(|hours| *hours > 0.0)
}

/// Resident set in bytes, when the OS reports it.
fn resident_bytes() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("VmRSS:") else {
            continue;
        };
        let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
        return Some(kb.saturating_mul(1024));
    }
    None
}

/// Memory may grow at most 1% per elapsed hour. Under a minute, allow 8 MiB or 10%.
pub fn within_hourly_memory_budget(start: u64, end: u64, elapsed: Duration) -> bool {
    if start == 0 {
        return end == 0;
    }
    let secs = elapsed.as_secs_f64();
    if secs < 60.0 {
        let slack = (start / 10).max(8 * 1024 * 1024);
        return end <= start.saturating_add(slack);
    }
    let hours = secs / 3600.0;
    let allowed = (start as f64) * (1.0 + 0.01 * hours);
    (end as f64) <= allowed
}

fn bind_user_state(session: &mut EditorSession, dir: &Path) {
    session.recent_path = Some(dir.join("recent.seal"));
    session.user_scene = Some(dir.join("scene.seal"));
    session.scene_journal = Some(dir.join("scene.journal"));
    session.undo_journal = Some(dir.join("undo.journal"));
    session.layouts_dir = Some(dir.join("layouts"));
    session.memory_path = Some(dir.join("memory.seal"));
    session.keymap_path = Some(dir.join("keymap.seal"));
}

#[test]
fn random_registered_commands_return_errors_instead_of_panicking() {
    let root = std::env::temp_dir().join(format!("seal-soak-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("soak root");
    let state = root.join("state");
    std::fs::create_dir_all(&state).expect("soak state");
    let project = root.join("project");

    let mut session = EditorSession::new(Layout::default());
    bind_user_state(&mut session, &state);
    session
        .dispatch(
            "file.new-project",
            &CommandArgs::new(&[("path", project.to_str().unwrap()), ("name", "Soak")]),
        )
        .expect("project");

    let ids: Vec<String> = session.commands.iter().map(|spec| spec.id.clone()).collect();
    assert!(ids.len() > 40, "expected many registered commands");

    let mut rng = Rng::new(0x5EAD_50A4_u64);
    let started = Instant::now();
    let deadline = soak_hours().map(|hours| Duration::from_secs_f64(hours * 3600.0));
    let start_rss = resident_bytes();
    let mut step = 0usize;
    loop {
        if let Some(limit) = deadline {
            if started.elapsed() >= limit {
                break;
            }
        } else if step >= soak_iterations() {
            break;
        }
        let id = ids[rng.index(ids.len())].clone();
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            session.dispatch(&id, &CommandArgs::default())
        }));
        assert!(result.is_ok(), "command {id} panicked on step {step}");
        let _dispatch = result.unwrap();
        step += 1;
        if step.is_multiple_of(500) {
            if let (Some(start), Some(now)) = (start_rss, resident_bytes()) {
                assert!(
                    within_hourly_memory_budget(start, now, started.elapsed()),
                    "rss grew from {start} to {now} after {step} commands"
                );
            }
        }
    }
    if let (Some(start), Some(end)) = (start_rss, resident_bytes()) {
        assert!(
            within_hourly_memory_budget(start, end, started.elapsed()),
            "rss grew from {start} to {end}"
        );
    }

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn hourly_budget_rejects_a_doubling_and_allows_half_a_percent() {
    let hour = Duration::from_secs(3600);
    assert!(!within_hourly_memory_budget(1_000_000, 2_000_000, hour));
    assert!(within_hourly_memory_budget(1_000_000, 1_005_000, hour));
    let eight = Duration::from_secs(8 * 3600);
    assert!(within_hourly_memory_budget(1_000_000, 1_080_000, eight));
    assert!(!within_hourly_memory_budget(1_000_000, 1_090_000, eight));
}
