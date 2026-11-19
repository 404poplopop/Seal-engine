//! Headless performance budgets.
//!
//! **Startup:** constructing [`EditorSession`] must finish within **1.5 s** on CI (see
//! `HEADLESS_STARTUP_BUDGET` below). That is the headless equivalent of shell startup before
//! the first frame; a regression fails `cargo test`.
//!
//! **Large cases:** set `SEAL_PERF_LARGE=1` for the 1,000,000-line console buffer check and the
//! 100,000-file content query. Default `cargo test` does not allocate a million log lines or seed
//! that many files (the 100,000-row outliner case still runs by default).

use std::path::Path;
use std::time::{Duration, Instant};

use seal_editor_core::{seed_outliner_stress_rows, visible_range, EditorSession, Layout, LogLine};

/// Documented CI budget for headless session construction (milliseconds).
const HEADLESS_STARTUP_BUDGET: Duration = Duration::from_millis(1500);

const OUTLINER_QUERY_BUDGET: Duration = Duration::from_millis(750);
const CONTENT_QUERY_BUDGET: Duration = Duration::from_millis(1500);
const CONSOLE_BUFFER_BUDGET: Duration = Duration::from_millis(500);
const CONSOLE_LARGE_BUDGET: Duration = Duration::from_secs(8);

const OUTLINER_ROWS: usize = 100_000;
const CONTENT_ITEMS: usize = 2_000;
const VIEW_ROWS: usize = 40;
const DEFAULT_CONSOLE_LINES: usize = 50_000;
const LARGE_CONSOLE_LINES: usize = 1_000_000;

fn outliner_window_row_count(session: &EditorSession) -> usize {
    let query = session.models.outliner_query.to_lowercase();
    let ids: Vec<String> = if query.is_empty() {
        session.models.visible_ids()
    } else {
        session
            .models
            .nodes
            .iter()
            .filter(|node| node.name.to_lowercase().contains(&query))
            .map(|node| node.id.clone())
            .collect()
    };
    visible_range(ids.len(), session.outliner_scroll, VIEW_ROWS).len()
}

fn seed_content_dir(dir: &Path, count: usize) {
    for index in 0..count {
        let name = format!("asset_{index:06}.txt");
        std::fs::write(dir.join(name), b"seal").expect("content seed");
    }
}

fn perf_large_enabled() -> bool {
    std::env::var("SEAL_PERF_LARGE").ok().as_deref() == Some("1")
}

#[test]
fn headless_session_startup_within_one_point_five_seconds() {
    let start = Instant::now();
    let _session = EditorSession::new(Layout::default());
    let elapsed = start.elapsed();
    assert!(
        elapsed <= HEADLESS_STARTUP_BUDGET,
        "headless startup took {:?}, budget is {:?} (documented 1.5 s)",
        elapsed,
        HEADLESS_STARTUP_BUDGET,
    );
}

#[test]
fn outliner_query_and_window_on_100k_rows_stays_within_budget() {
    let mut session = EditorSession::new(Layout::default());
    seed_outliner_stress_rows(&mut session.models);
    session.models.outliner_query = "entity".into();
    session.outliner_scroll = 50_000;

    let start = Instant::now();
    let rows = outliner_window_row_count(&session);
    let elapsed = start.elapsed();

    assert_eq!(rows, VIEW_ROWS);
    assert!(
        elapsed <= OUTLINER_QUERY_BUDGET,
        "outliner query/window took {:?}, budget {:?}",
        elapsed,
        OUTLINER_QUERY_BUDGET
    );
}

#[test]
fn content_query_on_many_items_stays_within_budget() {
    let dir = std::env::temp_dir().join(format!("seal-content-perf-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("content dir");
    seed_content_dir(&dir, CONTENT_ITEMS);

    let start = Instant::now();
    let names = seal_editor_core::list_folder(&dir, "420");
    let elapsed = start.elapsed();

    assert!(!names.is_empty());
    assert!(
        elapsed <= CONTENT_QUERY_BUDGET,
        "content query took {:?}, budget {:?}",
        elapsed,
        CONTENT_QUERY_BUDGET
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn content_query_on_100k_items_when_large_perf_enabled() {
    if !perf_large_enabled() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("seal-content-large-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("content dir");
    seed_content_dir(&dir, OUTLINER_ROWS);

    let start = Instant::now();
    let names = seal_editor_core::list_folder(&dir, "420");
    let elapsed = start.elapsed();

    assert!(!names.is_empty());
    assert!(
        elapsed <= CONTENT_QUERY_BUDGET,
        "100k content query took {:?}, budget {:?}",
        elapsed,
        CONTENT_QUERY_BUDGET
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn console_buffer_filtering_stays_within_budget() {
    let records: Vec<LogLine> = (0..DEFAULT_CONSOLE_LINES)
        .map(|index| LogLine {
            level: "info".into(),
            message: format!("log line {index} marker 420"),
            source: String::new(),
            fields: Vec::new(),
        })
        .collect();
    let mut session = EditorSession::new(Layout::default());
    session.models.console_query = "420".into();

    let start = Instant::now();
    let shown = session.models.filtered_logs(&records, 5_000);
    let elapsed = start.elapsed();

    assert!(!shown.is_empty());
    assert!(
        elapsed <= CONSOLE_BUFFER_BUDGET,
        "console buffer took {:?}, budget {:?}",
        elapsed,
        CONSOLE_BUFFER_BUDGET
    );
}

#[test]
fn console_buffer_one_million_lines_within_budget_when_requested() {
    if !perf_large_enabled() {
        return;
    }
    let records: Vec<LogLine> = (0..LARGE_CONSOLE_LINES)
        .map(|index| LogLine {
            level: "info".into(),
            message: format!("log line {index}"),
            source: String::new(),
            fields: Vec::new(),
        })
        .collect();
    let session = EditorSession::new(Layout::default());

    let start = Instant::now();
    let shown = session.models.filtered_logs(&records, 10_000);
    let elapsed = start.elapsed();

    assert_eq!(shown.len(), 10_000);
    assert!(
        elapsed <= CONSOLE_LARGE_BUDGET,
        "1M console filter took {:?}, budget {:?}",
        elapsed,
        CONSOLE_LARGE_BUDGET
    );
}
