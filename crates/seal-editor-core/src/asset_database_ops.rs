//! Merge duplicate assets, copy with dependencies to another project.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::asset_index::{
    self, find_references, load_index, normalize_rel, rewrite_project_paths, save_index,
    sync_index, AssetIndex, INDEX_FILE,
};
use crate::command::CommandArgs;
use crate::content::refresh_content_tree;
use crate::error::CoreError;
use crate::foundation::ModalKind;
use crate::models::Past;
use crate::persist::atomic_write;
use crate::project::open_project;
use crate::session::{EditorSession, HostAction};

type CommandFn = fn(&mut EditorSession, &CommandArgs) -> Result<(), CoreError>;

pub const COMMANDS: &[(&str, &str, &str, CommandFn)] = &[
    ("asset.merge_duplicates", "command-asset-merge-duplicates", "file", cmd_merge_duplicates),
    ("asset.copy_to_project", "command-asset-copy-to-project", "file", cmd_copy_to_project),
    (
        "asset.copy_to_project.finish",
        "command-asset-copy-to-project-finish",
        "file",
        cmd_copy_to_project_finish,
    ),
    ("naming.scan", "command-naming-scan", "tools", cmd_naming_scan),
    ("naming.fix", "command-naming-fix", "tools", cmd_naming_fix),
    ("naming.rule.set", "command-naming-rule-set", "tools", cmd_naming_rule_set),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergeDuplicatesChange {
    pub keeper: String,
    pub removed: Vec<(String, Vec<u8>)>,
    pub file_snapshots: Vec<(String, String, String)>,
    pub index_before: String,
    pub index_after: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PendingMergeDuplicates {
    pub keeper: String,
    pub others: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PendingMigrateAssets {
    pub paths: Vec<String>,
}

pub fn duplicate_paths(index: &AssetIndex, rel: &str) -> Vec<String> {
    let rel = normalize_rel(rel);
    let hash = index.entries.values().find(|e| e.path == rel).map(|e| e.hash);
    let Some(hash) = hash else {
        return Vec::new();
    };
    let mut paths: Vec<String> = index
        .entries
        .values()
        .filter(|e| e.hash == hash && e.path != rel)
        .map(|e| e.path.clone())
        .collect();
    paths.sort();
    paths
}

pub fn dependency_closure(root: &Path, seeds: &[String], ignore: &[String]) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut work: Vec<String> = seeds.iter().map(|p| normalize_rel(p)).collect();
    while let Some(path) = work.pop() {
        if !seen.insert(path.clone()) {
            continue;
        }
        let report = find_references(root, &path, ignore);
        for dep in report.dependencies {
            let dep = normalize_rel(&dep);
            if !dep.is_empty() && !seen.contains(&dep) {
                work.push(dep);
            }
        }
    }
    seen
}

fn cmd_merge_duplicates(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let project = session.project.clone().ok_or_else(|| CoreError::new("no project is open"))?;
    if project.read_only {
        return Err(CoreError::new(session.messages.get("content-error-read-only")));
    }
    let keeper = args
        .get("path")
        .map(str::to_string)
        .or_else(|| session.models.content_pick.clone())
        .filter(|p| !p.is_empty())
        .map(|p| normalize_rel(&p))
        .ok_or_else(|| CoreError::new("pick a file in the Asset Browser"))?;
    if session.project.as_ref().is_some_and(|p| p.root.join(&keeper).is_dir()) {
        return Err(CoreError::new("pick a file, not a folder"));
    }
    let others = duplicate_paths(&session.models.asset_index, &keeper);
    if others.is_empty() {
        return Err(CoreError::new(session.messages.get("asset-merge-no-duplicates")));
    }
    let mut lines: Vec<String> =
        std::iter::once(keeper.clone()).chain(others.iter().cloned()).collect();
    lines.sort();
    let body = lines.join("\n");
    session.models.pending_merge_duplicates =
        Some(PendingMergeDuplicates { keeper: keeper.clone(), others: others.clone() });
    session.foundation.modal = Some(crate::foundation::Modal {
        kind: ModalKind::MergeDuplicates,
        title: session.messages.get("modal-merge-duplicates-title"),
        body: session
            .messages
            .format("modal-merge-duplicates-body", &[("files", &body), ("keeper", &keeper)]),
        confirm: session.messages.get("modal-merge-duplicates-confirm"),
        alternate: String::new(),
        cancel: session.messages.get("modal-cancel"),
    });
    Ok(())
}

pub fn apply_merge_duplicates(session: &mut EditorSession) -> Result<(), CoreError> {
    let Some(pending) = session.models.pending_merge_duplicates.take() else {
        return Ok(());
    };
    let project = session.project.clone().ok_or_else(|| CoreError::new("no project is open"))?;
    let root = project.root.clone();
    let keeper = pending.keeper;
    let index_before = fs::read_to_string(root.join(INDEX_FILE)).unwrap_or_default();
    let mut file_snapshots = Vec::new();
    let mut removed = Vec::new();
    for other in &pending.others {
        let snaps = rewrite_project_paths(&root, other, &keeper, &project.ignore)?;
        for row in snaps {
            if !file_snapshots.iter().any(|(p, _, _)| p == &row.0) {
                file_snapshots.push(row);
            }
        }
    }
    for other in &pending.others {
        let full = root.join(other);
        if full.is_file() {
            let bytes = fs::read(&full)?;
            removed.push((other.clone(), bytes));
            fs::remove_file(&full)?;
        }
        session.models.asset_index.entries.retain(|_, e| e.path != *other);
    }
    let _ = save_index(&root, &session.models.asset_index);
    refresh_content_tree(session);
    let index_after = fs::read_to_string(root.join(INDEX_FILE)).unwrap_or_default();
    session.push_past(Past::MergeDuplicates {
        change: MergeDuplicatesChange {
            keeper,
            removed,
            file_snapshots,
            index_before,
            index_after,
        },
    });
    session.future.clear();
    session.notify(session.messages.get("asset-merge-done"), false);
    Ok(())
}

pub fn undo_merge(session: &mut EditorSession, change: &MergeDuplicatesChange) {
    let Some(project) = session.project.clone() else {
        return;
    };
    let root = project.root.clone();
    for (file, before, _) in &change.file_snapshots {
        let _ = atomic_write(&root.join(file), before.as_bytes());
    }
    for (path, bytes) in &change.removed {
        let full = root.join(path);
        if let Some(parent) = full.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&full, bytes);
    }
    let _ = atomic_write(&root.join(INDEX_FILE), change.index_before.as_bytes());
    if let Ok(index) = load_index(&root) {
        session.models.asset_index = index;
    }
    refresh_content_tree(session);
}

pub fn redo_merge(session: &mut EditorSession, change: &MergeDuplicatesChange) {
    let Some(project) = session.project.clone() else {
        return;
    };
    let root = project.root.clone();
    for (file, _, after) in &change.file_snapshots {
        let _ = atomic_write(&root.join(file), after.as_bytes());
    }
    for (path, _) in &change.removed {
        let _ = fs::remove_file(root.join(path));
    }
    let _ = atomic_write(&root.join(INDEX_FILE), change.index_after.as_bytes());
    if let Ok(index) = load_index(&root) {
        session.models.asset_index = index;
    }
    refresh_content_tree(session);
}

fn cmd_copy_to_project(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let project = session.project.clone().ok_or_else(|| CoreError::new("no project is open"))?;
    if project.read_only {
        return Err(CoreError::new(session.messages.get("content-error-read-only")));
    }
    let paths = selected_paths(session, args)?;
    session.models.pending_migrate_assets = Some(PendingMigrateAssets { paths });
    session.host_action = Some(HostAction::PickMigrateTarget);
    Ok(())
}

fn selected_paths(session: &EditorSession, args: &CommandArgs) -> Result<Vec<String>, CoreError> {
    if let Some(list) = args.get("paths") {
        let paths: Vec<String> =
            list.split(',').map(str::trim).filter(|p| !p.is_empty()).map(normalize_rel).collect();
        if !paths.is_empty() {
            return Ok(paths);
        }
    }
    if let Some(one) = args.get("path").map(str::to_string) {
        return Ok(vec![normalize_rel(&one)]);
    }
    session
        .models
        .content_pick
        .clone()
        .filter(|p| !p.is_empty())
        .map(|p| Ok(vec![normalize_rel(&p)]))
        .unwrap_or_else(|| Err(CoreError::new("select assets in the Asset Browser")))
}

fn cmd_copy_to_project_finish(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let target = args
        .get("folder")
        .map(str::to_string)
        .ok_or_else(|| CoreError::new("missing destination project folder"))?;
    let pending = session
        .models
        .pending_migrate_assets
        .take()
        .ok_or_else(|| CoreError::new("no assets were queued to copy"))?;
    let source = session.project.clone().ok_or_else(|| CoreError::new("no project is open"))?;
    migrate_assets(&source, &target, &pending.paths, session)
}

pub fn migrate_assets(
    source: &crate::project::OpenProject,
    target_folder: &str,
    seeds: &[String],
    session: &mut EditorSession,
) -> Result<(), CoreError> {
    let target_root = std::path::Path::new(target_folder);
    let target_project = open_project(target_root)?;
    if target_project.root == source.root {
        return Err(CoreError::new(session.messages.get("asset-migrate-same-project")));
    }
    let closure = dependency_closure(&source.root, seeds, &source.ignore);
    let mut conflicts = Vec::new();
    let mut copied = 0usize;
    for rel in &closure {
        let from = source.root.join(rel);
        if !from.is_file() {
            continue;
        }
        let dest = target_root.join(rel);
        if dest.is_file() {
            let same = asset_index::file_content_hash(&from).ok()
                == asset_index::file_content_hash(&dest).ok();
            if !same {
                conflicts.push(rel.clone());
            }
            continue;
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&from, &dest)?;
        copied += 1;
    }
    let mut index = load_index(target_root).unwrap_or_default();
    let _ = sync_index(target_root, &mut index, &target_project.ignore);
    save_index(target_root, &index)?;
    if !conflicts.is_empty() {
        let list = conflicts.join("\n");
        session
            .notify(session.messages.format("asset-migrate-conflicts", &[("list", &list)]), true);
    }
    session.notify(
        session.messages.format("asset-migrate-done", &[("count", &copied.to_string())]),
        false,
    );
    Ok(())
}

fn cmd_naming_scan(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    crate::naming_rules::scan_project(session)
}

fn cmd_naming_fix(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let rel = args
        .get("path")
        .map(str::to_string)
        .or_else(|| session.models.content_pick.clone())
        .ok_or_else(|| CoreError::new("pick an asset"))?;
    crate::naming_rules::fix_violation(session, &rel)
}

fn cmd_naming_rule_set(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let kind = required(args, "kind")?;
    let prefix = args.get("prefix").unwrap_or("").to_string();
    let suffix = args.get("suffix").unwrap_or("").to_string();
    let case = args.get("case").unwrap_or("none").to_string();
    let pattern = args.get("pattern").unwrap_or("").to_string();
    crate::naming_rules::upsert_rule(
        session,
        crate::naming_rules::NamingRule { kind, prefix, suffix, case, pattern },
    )
}

fn required(args: &CommandArgs, key: &str) -> Result<String, CoreError> {
    args.get(key)
        .map(str::to_string)
        .ok_or_else(|| CoreError::new(format!("missing argument '{key}'")))
}

pub fn paths_with_duplicates(index: &AssetIndex) -> BTreeSet<String> {
    let mut by_hash: BTreeMap<u64, Vec<String>> = BTreeMap::new();
    for entry in index.entries.values() {
        if entry.hash == 0 {
            continue;
        }
        by_hash.entry(entry.hash).or_default().push(entry.path.clone());
    }
    let mut out = BTreeSet::new();
    for paths in by_hash.values() {
        if paths.len() > 1 {
            for path in paths {
                out.insert(path.clone());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset_index::rebuild_index;
    use crate::{CommandArgs, EditorSession, Layout, ModalChoice};
    use std::path::PathBuf;

    fn tiny(tag: &str) -> (PathBuf, EditorSession) {
        let dir = std::env::temp_dir().join(format!("seal-adb-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let project = dir.join("P");
        fs::create_dir_all(project.join("sprites")).unwrap();
        let id = format!("{:08x}-0000-7000-8000-{:012x}", std::process::id(), 11);
        fs::write(
            project.join("project.seal"),
            format!(
                "schema_version = 1\nname = \"P\"\nid = \"{id}\"\ncreated = 1\neditor_version = \"0\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n"
            ),
        )
        .unwrap();
        fs::write(project.join("sprites/a.png"), b"same-bytes").unwrap();
        fs::write(project.join("sprites/b.png"), b"same-bytes").unwrap();
        fs::write(project.join("scene.seal"), "entity { sprite = \"sprites/b.png\" }\n").unwrap();
        let path = project.to_string_lossy().to_string();
        let mut s = EditorSession::new(Layout::default());
        s.dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())])).unwrap();
        rebuild_index(&project, &[".seal-editor/".into()]).unwrap();
        s.models.asset_index = load_index(&project).unwrap();
        (project, s)
    }

    #[test]
    fn merge_rewrites_and_removes_duplicate_undo_restores() {
        let (project, mut session) = tiny("merge");
        session
            .dispatch("asset.merge_duplicates", &CommandArgs::new(&[("path", "sprites/a.png")]))
            .unwrap();
        session.choose_modal(ModalChoice::Confirm);
        assert!(!project.join("sprites/b.png").exists());
        let scene = fs::read_to_string(project.join("scene.seal")).unwrap();
        assert!(scene.contains("sprites/a.png"));
        session.dispatch("edit.undo", &CommandArgs::default()).unwrap();
        assert!(project.join("sprites/b.png").is_file());
        let scene = fs::read_to_string(project.join("scene.seal")).unwrap();
        assert!(scene.contains("sprites/b.png"));
    }

    #[test]
    fn migrate_copies_files_and_index_ids() {
        let (src, mut session) = tiny("mig-src");
        fs::write(src.join("sprites/dep.png"), b"dep-bytes").unwrap();
        fs::write(src.join("scene.seal"), "entity { sprite = \"sprites/a.png\" }\n").unwrap();
        let dest = std::env::temp_dir().join(format!("seal-adb-mig-dst-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dest);
        fs::create_dir_all(dest.join("sprites")).unwrap();
        let id = format!("{:08x}-0000-7000-8000-{:012x}", std::process::id(), 22);
        fs::write(
            dest.join("project.seal"),
            format!(
                "schema_version = 1\nname = \"D\"\nid = \"{id}\"\ncreated = 1\neditor_version = \"0\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n"
            ),
        )
        .unwrap();
        session
            .dispatch(
                "asset.copy_to_project",
                &CommandArgs::new(&[("paths", "sprites/a.png,sprites/dep.png")]),
            )
            .unwrap();
        session
            .dispatch(
                "asset.copy_to_project.finish",
                &CommandArgs::new(&[("folder", dest.to_string_lossy().as_ref())]),
            )
            .unwrap();
        assert!(dest.join("sprites/a.png").is_file());
        assert!(dest.join("sprites/dep.png").is_file());
        let index = load_index(&dest).unwrap();
        assert!(index.entries.len() >= 2);
        let ids: std::collections::BTreeSet<String> =
            index.entries.values().map(|e| e.id.clone()).collect();
        assert_eq!(ids.len(), index.entries.len());
    }
}
