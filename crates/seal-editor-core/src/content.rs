use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::error::CoreError;
use crate::project::is_ignored;
use crate::session::EditorSession;

pub const WATCH_DEBOUNCE_MS: u64 = 200;

/// Inline panel error backed by an English message id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentInlineError {
    pub key: String,
    pub args: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentEntry {
    pub name: String,
    /// Path relative to the tree root, using `/` separators.
    pub rel: String,
    pub depth: usize,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub open: bool,
    pub has_children: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ContentWatchState {
    pub last_check_ms: u64,
    pub fingerprint: u64,
    /// Modification time of each entry in the watched folder at the last poll.
    pub entry_mtime: BTreeMap<String, SystemTime>,
}

pub fn set_content_error(session: &mut EditorSession, key: impl Into<String>) {
    session.models.content_error = Some(ContentInlineError { key: key.into(), args: Vec::new() });
}

pub fn set_content_error_fmt(
    session: &mut EditorSession,
    key: impl Into<String>,
    args: Vec<(String, String)>,
) {
    session.models.content_error = Some(ContentInlineError { key: key.into(), args });
}

pub fn clear_content_error(session: &mut EditorSession) {
    session.models.content_error = None;
}

pub fn map_io_error(err: &CoreError) -> &'static str {
    if err.message.contains("permission") || err.message.contains("Permission") {
        "content-error-permission"
    } else if err.message.contains("not found") || err.message.contains("No such file") {
        "content-error-missing"
    } else {
        "content-error-generic"
    }
}

fn forced_hidden(name: &str) -> bool {
    name == ".git" || name == ".seal-editor"
}

pub fn visible_name(name: &str, show_hidden: bool) -> bool {
    if forced_hidden(name) {
        return false;
    }
    // Autosave backups (`name.bak`) are kept beside each file but listed only
    // with hidden files shown.
    // The lock file and the asset id index are editor bookkeeping, not assets.
    show_hidden
        || !(name.starts_with('.')
            || name.ends_with(".bak")
            || name == "project.lock"
            || name == crate::asset_index::INDEX_FILE)
}

#[cfg(test)]
mod visible_name_tests {
    #[test]
    fn backups_are_hidden_unless_hidden_files_are_shown() {
        assert!(!super::visible_name("Sparks.fx.bak", false));
        assert!(super::visible_name("Sparks.fx.bak", true));
        assert!(super::visible_name("Sparks.fx", false));
        assert!(!super::visible_name("project.lock", false));
        assert!(!super::visible_name("asset-index.seal", false));
        assert!(super::visible_name("asset-index.seal", true));
    }
}

pub fn fingerprint_dir(path: &Path) -> Result<u64, CoreError> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.display().to_string().hash(&mut hasher);
    if let Ok(meta) = fs::metadata(path) {
        if let Ok(modified) = meta.modified() {
            modified.hash(&mut hasher);
        }
    }
    let entries = fs::read_dir(path)?;
    let mut count = 0u64;
    for entry in entries {
        let entry = entry?;
        count = count.saturating_add(1);
        entry.file_name().to_string_lossy().hash(&mut hasher);
        if let Ok(meta) = entry.metadata() {
            if let Ok(modified) = meta.modified() {
                modified.hash(&mut hasher);
            }
            meta.is_dir().hash(&mut hasher);
        }
    }
    count.hash(&mut hasher);
    Ok(hasher.finish())
}

pub fn poll_content_watcher(session: &mut EditorSession) {
    let Some(root) = session.models.content_root.clone() else {
        return;
    };
    let elapsed = session.uptime_ms.saturating_sub(session.models.content_watch.last_check_ms);
    if elapsed < WATCH_DEBOUNCE_MS {
        return;
    }
    session.models.content_watch.last_check_ms = session.uptime_ms;
    match fingerprint_dir(&root) {
        Ok(next) => {
            let changed = session.models.content_watch.fingerprint != 0
                && next != session.models.content_watch.fingerprint;
            if changed {
                let previous = session.models.content_watch.entry_mtime.clone();
                let names = changed_entry_names(&previous, &dir_mtimes(&root));
                session.models.content_generation =
                    session.models.content_generation.saturating_add(1);
                clear_content_error(session);
                crate::watcher_reimport::reimport_external(session, &root, &names);
                crate::asset_index::sync_index_from_watcher(session);
                session.models.content_watch.entry_mtime = dir_mtimes(&root);
                session.models.content_watch.fingerprint = fingerprint_dir(&root).unwrap_or(next);
            } else {
                session.models.content_watch.fingerprint = next;
                if session.models.content_watch.entry_mtime.is_empty() {
                    session.models.content_watch.entry_mtime = dir_mtimes(&root);
                }
            }
        }
        Err(err) => {
            set_content_error_fmt(
                session,
                map_io_error(&err),
                vec![("reason".into(), err.to_string())],
            );
        }
    }
}

pub fn refresh_content_tree(session: &mut EditorSession) {
    if let Some(root) = session.models.content_root.clone() {
        session.models.content_watch.fingerprint = fingerprint_dir(&root).unwrap_or(0);
        session.models.content_watch.entry_mtime = dir_mtimes(&root);
    }
    session.models.content_generation = session.models.content_generation.saturating_add(1);
    clear_content_error(session);
}

fn dir_mtimes(path: &Path) -> BTreeMap<String, SystemTime> {
    let mut out = BTreeMap::new();
    let Ok(entries) = fs::read_dir(path) else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Ok(modified) = entry.metadata().and_then(|meta| meta.modified()) {
            out.insert(name, modified);
        }
    }
    out
}

/// Names whose modification time changed. An empty baseline means this is the first look.
fn changed_entry_names(
    previous: &BTreeMap<String, SystemTime>,
    current: &BTreeMap<String, SystemTime>,
) -> Vec<String> {
    if previous.is_empty() {
        return Vec::new();
    }
    current
        .iter()
        .filter(|(name, time)| previous.get(*name) != Some(*time))
        .map(|(name, _)| name.clone())
        .collect()
}

fn entry_meta(path: &Path) -> (bool, bool) {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return (false, false);
    };
    let is_symlink = meta.file_type().is_symlink();
    let is_dir = if is_symlink {
        fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false)
    } else {
        meta.is_dir()
    };
    (is_dir, is_symlink)
}

fn list_children(
    dir: &Path,
    rel_prefix: &str,
    depth: usize,
    show_hidden: bool,
    ignore: &[String],
    expanded: &BTreeSet<String>,
    seen_dirs: &mut HashSet<PathBuf>,
    out: &mut Vec<ContentEntry>,
) -> Result<(), CoreError> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Err(CoreError::new(format!("cannot read {}", dir.display())));
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| visible_name(name, show_hidden))
        .filter(|name| {
            let rel =
                if rel_prefix.is_empty() { name.clone() } else { format!("{rel_prefix}/{name}") };
            !is_ignored(&rel, ignore)
        })
        .collect();
    names.sort_by(|a, b| crate::outliner_query::natural_cmp(&a.to_lowercase(), &b.to_lowercase()));
    for name in names {
        let rel = if rel_prefix.is_empty() { name.clone() } else { format!("{rel_prefix}/{name}") };
        let path = dir.join(&name);
        let (is_dir, is_symlink) = entry_meta(&path);
        let mut has_children = false;
        let mut may_expand = false;
        if is_dir && !is_symlink {
            let canonical = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if seen_dirs.insert(canonical) {
                may_expand = true;
                has_children = fs::read_dir(&path)
                    .map(|dir| dir.filter_map(|e| e.ok()).next().is_some())
                    .unwrap_or(false);
            }
        } else if is_dir && is_symlink {
            has_children = false;
        }
        let open = expanded.contains(&rel);
        out.push(ContentEntry {
            name,
            rel: rel.clone(),
            depth,
            is_dir,
            is_symlink,
            open,
            has_children: has_children || (is_dir && is_symlink),
        });
        if is_dir && !is_symlink && open && may_expand {
            list_children(
                &path,
                &rel,
                depth.saturating_add(1),
                show_hidden,
                ignore,
                expanded,
                seen_dirs,
                out,
            )?;
        }
    }
    Ok(())
}

pub fn content_tree_rows(session: &EditorSession) -> Result<Vec<ContentEntry>, CoreError> {
    let out = content_tree_rows_unfiltered(session)?;
    let query = session.models.content_query.to_lowercase();
    if query.is_empty() {
        return Ok(out);
    }
    Ok(out
        .into_iter()
        .filter(|row| row.depth == 0 || row.name.to_lowercase().contains(&query))
        .collect())
}

/// Every row of the folder tree, ignoring the search box.
pub fn content_tree_rows_unfiltered(
    session: &EditorSession,
) -> Result<Vec<ContentEntry>, CoreError> {
    let _generation = session.models.content_generation;
    let Some(root) = &session.models.content_tree_root else {
        return Ok(Vec::new());
    };
    let show_hidden = session.foundation.settings.get("content.show_hidden") == Some("true");
    let ignore = session.project.as_ref().map(|project| project.ignore.clone()).unwrap_or_default();
    let mut out = Vec::new();
    let root_label = root.file_name().and_then(|n| n.to_str()).unwrap_or(".");
    out.push(ContentEntry {
        name: root_label.to_string(),
        rel: String::new(),
        depth: 0,
        is_dir: true,
        is_symlink: false,
        open: true,
        has_children: true,
    });
    let mut seen = HashSet::new();
    let _ = fs::canonicalize(root).map(|c| seen.insert(c));
    list_children(
        root,
        "",
        1,
        show_hidden,
        &ignore,
        &session.models.content_expanded,
        &mut seen,
        &mut out,
    )?;
    Ok(out)
}

pub fn list_names_in_folder(
    root: &Path,
    show_hidden: bool,
    query: &str,
    ignore: &[String],
) -> Vec<String> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let q = query.to_lowercase();
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| visible_name(name, show_hidden))
        .filter(|name| !is_ignored(name, ignore))
        .filter(|name| q.is_empty() || name.to_lowercase().contains(&q))
        .collect();
    names.sort();
    names
}

pub fn list_current_folder(
    session: &EditorSession,
) -> Result<Vec<(String, bool, bool)>, CoreError> {
    let _generation = session.models.content_generation;
    let Some(root) = &session.models.content_root else {
        return Ok(Vec::new());
    };
    let show_hidden = session.foundation.settings.get("content.show_hidden") == Some("true");
    let query = session.models.content_query.to_lowercase();
    let ignore = session.project.as_ref().map(|project| project.ignore.clone()).unwrap_or_default();
    let entries = fs::read_dir(root).map_err(|err| CoreError::new(err.to_string()))?;
    let mut names: Vec<(String, bool, bool)> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            let (is_dir, is_symlink) = entry_meta(&path);
            (name, is_dir, is_symlink)
        })
        .filter(|(name, _, _)| visible_name(name, show_hidden))
        .filter(|(name, _, _)| !is_ignored(name, &ignore))
        .filter(|(name, _, _)| query.is_empty() || name.to_lowercase().contains(&query))
        .collect();
    names.sort_by(|a, b| {
        crate::outliner_query::natural_cmp(&a.0.to_lowercase(), &b.0.to_lowercase())
    });
    Ok(names)
}

pub fn open_content_folder(session: &mut EditorSession, path: &Path) {
    session.models.content_root = Some(path.to_path_buf());
    session.models.content_tree_root = Some(path.to_path_buf());
    session.models.content_expanded.clear();
    session.models.content_pick = None;
    crate::asset_browser::load(session);
    refresh_content_tree(session);
}

pub fn toggle_tree_dir(session: &mut EditorSession, rel: &str) {
    if rel.is_empty() {
        return;
    }
    if session.models.content_expanded.contains(rel) {
        session.models.content_expanded.remove(rel);
    } else {
        session.models.content_expanded.insert(rel.to_string());
    }
}

pub fn enter_content_dir(session: &mut EditorSession, rel: &str) {
    let Some(tree_root) = session.models.content_tree_root.clone() else {
        return;
    };
    let next = if rel.is_empty() { tree_root } else { tree_root.join(rel) };
    session.models.content_root = Some(next);
    refresh_content_tree(session);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Layout;
    use crate::session::EditorSession;
    use std::fs;
    use std::time::SystemTime;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("seal-content-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn hidden_git_and_dot_rules_apply() {
        assert!(!visible_name(".git", true));
        assert!(!visible_name(".seal-editor", true));
        assert!(!visible_name(".secret", false));
        assert!(visible_name(".secret", true));
        assert!(visible_name("readme.txt", false));
    }

    #[test]
    fn tree_lists_nested_folders_and_symlinks() {
        let dir = scratch("tree");
        fs::create_dir_all(dir.join("nested/child")).unwrap();
        fs::write(dir.join("nested/child/note.txt"), b"x").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.join("nested"), dir.join("link")).unwrap();
        }
        let mut session = EditorSession::new(Layout::default());
        open_content_folder(&mut session, &dir);
        session.models.content_expanded.insert("nested".into());
        let rows = content_tree_rows(&session).unwrap();
        assert!(rows.iter().any(|row| row.rel == "nested" && row.is_dir));
        assert!(rows.iter().any(|row| row.rel == "nested/child"));
        #[cfg(unix)]
        {
            let link = rows.iter().find(|row| row.rel == "link").unwrap();
            assert!(link.is_symlink);
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn watcher_refresh_detects_external_renames() {
        let dir = scratch("watch");
        fs::write(dir.join("a.txt"), b"a").unwrap();
        let mut session = EditorSession::new(Layout::default());
        open_content_folder(&mut session, &dir);
        session.uptime_ms = WATCH_DEBOUNCE_MS;
        poll_content_watcher(&mut session);
        let before = session.models.content_generation;
        session.uptime_ms = WATCH_DEBOUNCE_MS * 3;
        fs::rename(dir.join("a.txt"), dir.join("b.txt")).unwrap();
        poll_content_watcher(&mut session);
        assert!(session.models.content_generation > before);
        let names = list_current_folder(&session).unwrap();
        assert!(names.iter().any(|(name, _, _)| name == "b.txt"));
        let _ = fs::remove_dir_all(&dir);
    }

    // Budget: listing 100,000 entries must stay under 500 ms on CI hardware.
    // Run with `SEAL_CONTENT_STRESS=1 cargo test content_list_100k_budget -- --ignored`.
    #[test]
    #[ignore = "set SEAL_CONTENT_STRESS=1 to run the large-folder budget test"]
    fn content_list_100k_budget() {
        if std::env::var("SEAL_CONTENT_STRESS").ok().as_deref() != Some("1") {
            return;
        }
        let dir = scratch("100k");
        for index in 0..100_000 {
            fs::write(dir.join(format!("file-{index:06}.txt")), b"x").unwrap();
        }
        let mut session = EditorSession::new(Layout::default());
        open_content_folder(&mut session, &dir);
        let start = SystemTime::now();
        let listed = list_current_folder(&session).unwrap();
        let elapsed = start.elapsed().unwrap_or_default();
        assert_eq!(listed.len(), 100_000);
        assert!(elapsed.as_millis() <= 500, "elapsed {:?}", elapsed);
        let _ = fs::remove_dir_all(&dir);
    }
}
