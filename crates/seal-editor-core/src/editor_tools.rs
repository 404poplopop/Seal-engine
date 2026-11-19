//! Editor tool preferences: external programs, diff tool, and worker thread count.

use std::cell::Cell;
use std::path::Path;

use crate::error::CoreError;
use crate::settings::SettingsStore;

thread_local! {
    /// `0` means the count has not been read from preferences yet (same as `auto`).
    static WORKER_BUDGET: Cell<usize> = const { Cell::new(0) };
}

pub const EXTERNAL_EDITOR_PATH: &str = "tools.external_editor_path";
pub const EXTERNAL_EDITOR_ARGS: &str = "tools.external_editor_args";
pub const DIFF_TOOL_PATH: &str = "tools.diff_tool_path";
pub const DIFF_TOOL_ARGS: &str = "tools.diff_tool_args";
pub const WORKER_THREADS: &str = "jobs.worker_threads";

pub const DEFAULT_EXTERNAL_ARGS: &str = "{file}:{line}";
pub const DEFAULT_DIFF_ARGS: &str = "{left} {right}";

/// Built an argument list for an external editor invocation.
pub fn external_editor_argv(
    store: &SettingsStore,
    file: &Path,
    line: u32,
) -> Option<(String, Vec<String>)> {
    let exe = store.get(EXTERNAL_EDITOR_PATH)?.trim();
    if exe.is_empty() {
        return None;
    }
    let template = store.get(EXTERNAL_EDITOR_ARGS).unwrap_or(DEFAULT_EXTERNAL_ARGS);
    let file = file.display().to_string();
    let line = line.to_string();
    let arg_line = template.replace("{file}", &file).replace("{line}", &line);
    let args: Vec<String> = arg_line.split_whitespace().map(str::to_string).collect();
    Some((exe.to_string(), args))
}

pub fn diff_tool_argv(
    store: &SettingsStore,
    left: &Path,
    right: &Path,
) -> Option<(String, Vec<String>)> {
    let exe = store.get(DIFF_TOOL_PATH)?.trim();
    if exe.is_empty() {
        return None;
    }
    let template = store.get(DIFF_TOOL_ARGS).unwrap_or(DEFAULT_DIFF_ARGS);
    let left = left.display().to_string();
    let right = right.display().to_string();
    let arg_line = template.replace("{left}", &left).replace("{right}", &right);
    let args: Vec<String> = arg_line.split_whitespace().map(str::to_string).collect();
    Some((exe.to_string(), args))
}

pub fn worker_thread_count(store: &SettingsStore) -> usize {
    match store.get(WORKER_THREADS).unwrap_or("auto") {
        "auto" => available_workers(),
        text => text.parse::<usize>().unwrap_or(1).clamp(1, 64),
    }
}

fn available_workers() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, 64)
}

/// Remember the Worker Threads preference for import, export, and thumbnail jobs.
pub fn note_worker_budget(store: &SettingsStore) {
    WORKER_BUDGET.with(|slot| slot.set(worker_thread_count(store)));
}

/// Pool size for the current thread. Unset follows `auto` (available parallelism).
pub fn active_workers() -> usize {
    WORKER_BUDGET.with(|slot| {
        let n = slot.get();
        if n == 0 {
            available_workers()
        } else {
            n
        }
    })
}

/// Run `f` over `items` on up to `workers` threads and return results in input order.
pub fn map_in_order<T, R, F>(workers: usize, items: Vec<T>, f: F) -> Vec<R>
where
    T: Send,
    R: Send,
    F: Fn(T) -> R + Sync,
{
    if items.is_empty() {
        return Vec::new();
    }
    let workers = workers.clamp(1, items.len());
    if workers == 1 {
        return items.into_iter().map(f).collect();
    }
    let mut rest = items;
    let mut parts = Vec::with_capacity(workers);
    for index in 0..workers {
        let slots = workers - index;
        let take = rest.len().div_ceil(slots);
        let tail = rest.split_off(take.min(rest.len()));
        parts.push(rest);
        rest = tail;
    }
    std::thread::scope(|scope| {
        let mut guards = Vec::with_capacity(parts.len());
        for part in parts {
            guards.push(scope.spawn(|| part.into_iter().map(&f).collect::<Vec<R>>()));
        }
        let mut out = Vec::new();
        for guard in guards {
            out.extend(guard.join().unwrap_or_else(|payload| std::panic::resume_unwind(payload)));
        }
        out
    })
}

pub fn sanitize_worker_threads(value: &str) -> Result<String, CoreError> {
    if value == "auto" {
        return Ok(value.to_string());
    }
    let n: usize = value
        .parse()
        .map_err(|_| CoreError::new("worker_threads must be auto or a number from 1 to 64"))?;
    if !(1..=64).contains(&n) {
        return Err(CoreError::new("worker_threads must be auto or a number from 1 to 64"));
    }
    Ok(n.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::SettingsStore;

    #[test]
    fn templates_expand_and_damaged_worker_count_falls_back() {
        let mut store = SettingsStore::builtin();
        store.set_user(EXTERNAL_EDITOR_PATH, "/bin/editor").unwrap();
        store.set_user(EXTERNAL_EDITOR_ARGS, "-g {file}:{line}").unwrap();
        let (exe, args) = external_editor_argv(&store, Path::new("/proj/a.txt"), 12).unwrap();
        assert_eq!(exe, "/bin/editor");
        assert!(args.iter().any(|a| a.contains("a.txt")));
        store.set_user(DIFF_TOOL_PATH, "/bin/diff").unwrap();
        let (_, dargs) = diff_tool_argv(&store, Path::new("left"), Path::new("right")).unwrap();
        assert_eq!(dargs, vec!["left", "right"]);
        store.set_user(WORKER_THREADS, "4").unwrap();
        assert_eq!(worker_thread_count(&store), 4);
        assert!(store.set_user(WORKER_THREADS, "broken").is_err());
        assert_eq!(worker_thread_count(&store), 4);
        store.set_user(WORKER_THREADS, "auto").unwrap();
        assert_eq!(worker_thread_count(&store), available_workers());
    }

    #[test]
    fn pool_size_follows_the_preference_and_keeps_order() {
        let mut store = SettingsStore::builtin();
        store.set_user(WORKER_THREADS, "2").unwrap();
        note_worker_budget(&store);
        assert_eq!(active_workers(), 2);
        let ids = std::sync::Mutex::new(std::collections::HashSet::new());
        let doubled = map_in_order(active_workers(), (0..4).collect(), |n: u32| {
            ids.lock().unwrap().insert(std::thread::current().id());
            std::thread::sleep(std::time::Duration::from_millis(20));
            n * 2
        });
        assert_eq!(doubled, vec![0, 2, 4, 6]);
        assert_eq!(ids.lock().unwrap().len(), 2);
        store.set_user(WORKER_THREADS, "1").unwrap();
        note_worker_budget(&store);
        let here = std::thread::current().id();
        let one = map_in_order(active_workers(), vec![7u32], |n| {
            assert_eq!(std::thread::current().id(), here);
            n
        });
        assert_eq!(one, vec![7]);
        store.set_user(WORKER_THREADS, "auto").unwrap();
        note_worker_budget(&store);
        assert_eq!(active_workers(), available_workers());
    }
}
