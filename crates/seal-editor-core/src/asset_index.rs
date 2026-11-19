//! Project asset index, reference scanning, and path rewrites on moves.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Instant;

use seal_sealtext::{parse, SealValue};
use seal_world::sound_container::SoundContainerDef;

use crate::content::refresh_content_tree;
use crate::error::CoreError;
use crate::import_presets;
use crate::models::Past;
use crate::persist::atomic_write;
use crate::project::is_ignored;
use crate::session::EditorSession;

pub const INDEX_FILE: &str = "asset-index.seal";
pub const INDEX_SCHEMA: u64 = 1;

const REFERENCE_SUFFIXES: &[&str] =
    &[".seal", ".prefab", ".graph", ".sealui", ".fx", ".frames", ".soundset"];

/// One row in the asset index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexEntry {
    pub id: String,
    pub path: String,
    pub hash: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetIndex {
    pub entries: BTreeMap<String, IndexEntry>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReferenceSite {
    pub file: String,
    pub detail: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReferenceReport {
    pub referencers: Vec<ReferenceSite>,
    pub dependencies: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RewritePreview {
    pub references: usize,
    pub files: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingContentPath {
    pub action: String,
    pub from: String,
    pub to: String,
    pub preview: RewritePreview,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentPathChange {
    pub from: String,
    pub to: String,
    pub file_snapshots: Vec<(String, String, String)>,
    pub sidecar_snapshots: Vec<(String, Option<String>, Option<String>)>,
    pub import_book_before: Option<String>,
    pub import_book_after: Option<String>,
}

pub fn normalize_rel(path: &str) -> String {
    path.replace('\\', "/").trim_matches('/').to_string()
}

/// Drop `.` and `..` segments so `./hero.png` matches the path other files store.
pub fn clean_project_rel(rel: &str) -> String {
    let mut parts = Vec::new();
    let flat = rel.replace('\\', "/");
    for part in flat.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// Project-relative path of `path`, with `.` segments removed. `None` when `path` is the root.
pub fn relative_to_project(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let cleaned = clean_project_rel(&rel.to_string_lossy());
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

pub fn file_content_hash(path: &Path) -> std::io::Result<u64> {
    let mut file = fs::File::open(path)?;
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash = fnv_continue(hash, &buf[..n]);
    }
    Ok(hash)
}

fn fnv_continue(hash: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(hash, |h, b| (h ^ *b as u64).wrapping_mul(0x0100_0000_01b3))
}

fn index_path(root: &Path) -> PathBuf {
    root.join(INDEX_FILE)
}

pub fn load_index(root: &Path) -> Result<AssetIndex, String> {
    let path = index_path(root);
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(AssetIndex::default());
    };
    let value = parse(&text).map_err(|err| format!("{INDEX_FILE} is unreadable ({err})"))?;
    Ok(index_from_value(&value)?)
}

fn index_from_value(value: &SealValue) -> Result<AssetIndex, String> {
    let mut index = AssetIndex::default();
    let Some(SealValue::Array(items)) = value.get("entries") else {
        return Err("asset index has no entries array".into());
    };
    for item in items {
        let SealValue::Array(parts) = item else {
            continue;
        };
        if parts.len() < 3 {
            continue;
        }
        let id = sealtext_string(&parts[0])?;
        let path = normalize_rel(&sealtext_string(&parts[1])?);
        let hash = match &parts[2] {
            SealValue::String(text) => text.parse().map_err(|_| "bad hash".to_string())?,
            SealValue::Number(text) => text.parse().map_err(|_| "bad hash".to_string())?,
            _ => return Err("bad hash".into()),
        };
        index.entries.insert(id.clone(), IndexEntry { id, path, hash });
    }
    Ok(index)
}

fn sealtext_string(value: &SealValue) -> Result<String, String> {
    match value {
        SealValue::String(text) => Ok(text.clone()),
        _ => Err("expected string".into()),
    }
}

pub fn save_index(root: &Path, index: &AssetIndex) -> Result<(), CoreError> {
    let mut rows: Vec<SealValue> = index
        .entries
        .values()
        .map(|entry| {
            SealValue::Array(vec![
                SealValue::String(entry.id.clone()),
                SealValue::String(entry.path.clone()),
                SealValue::Number(entry.hash.to_string()),
            ])
        })
        .collect();
    rows.sort_by(|a, b| {
        let path = |v: &SealValue| -> String {
            match v {
                SealValue::Array(p) => match p.get(1) {
                    Some(SealValue::String(s)) => s.clone(),
                    _ => String::new(),
                },
                _ => String::new(),
            }
        };
        path(a).cmp(&path(b))
    });
    let map = SealValue::Map(
        vec![
            ("schema_version".into(), SealValue::Number(INDEX_SCHEMA.to_string())),
            ("entries".into(), SealValue::Array(rows)),
        ]
        .into(),
    );
    atomic_write(&index_path(root), map.to_text().as_bytes())
}

fn is_indexable_file(path: &Path) -> bool {
    path.is_file() && !path.file_name().is_some_and(|n| n == INDEX_FILE)
}

pub fn rebuild_index(root: &Path, ignore: &[String]) -> Result<AssetIndex, CoreError> {
    let mut index = AssetIndex::default();
    let mut paths = Vec::new();
    walk_files(root, root, ignore, &mut paths)?;
    paths.sort();
    for rel in paths {
        let full = root.join(&rel);
        if !is_indexable_file(&full) {
            continue;
        }
        let hash = file_content_hash(&full).unwrap_or(0);
        let id = new_asset_id(&index);
        index.entries.insert(id.clone(), IndexEntry { id, path: rel, hash });
    }
    save_index(root, &index)?;
    Ok(index)
}

/// Project files for scanners (index, naming rules, migration).
pub fn walk_for_scan(
    base: &Path,
    dir: &Path,
    ignore: &[String],
    out: &mut Vec<String>,
) -> Result<(), CoreError> {
    walk_files(base, dir, ignore, out)
}

fn walk_files(base: &Path, dir: &Path, ignore: &[String], out: &mut Vec<String>) -> Result<(), CoreError> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = if dir == base {
            name.clone()
        } else {
            format!("{}/{}", dir.strip_prefix(base).unwrap().to_string_lossy(), name)
        };
        let rel = normalize_rel(&rel);
        if is_ignored(&rel, ignore) || name == ".seal-editor" || name == ".git" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            walk_files(base, &path, ignore, out)?;
        } else if is_indexable_file(&path) {
            out.push(rel);
        }
    }
    Ok(())
}

fn new_asset_id(index: &AssetIndex) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    loop {
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let millis =
            SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        let mut bytes = [0u8; 16];
        bytes[0..6].copy_from_slice(&(millis << 16).to_be_bytes()[2..8]);
        bytes[6] = (bytes[6] & 0x0f) | 0x70;
        bytes[7] = sequence as u8;
        let noise = millis
            ^ (std::process::id() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (millis >> 17)
            ^ sequence.wrapping_mul(0x6C62_272E_07BB_0142);
        bytes[8..16].copy_from_slice(&noise.to_be_bytes());
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        let id = format_uuid(bytes);
        if !index.entries.contains_key(&id) {
            return id;
        }
    }
}

fn format_uuid(bytes: [u8; 16]) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

pub fn sync_index(root: &Path, index: &mut AssetIndex, ignore: &[String]) -> Result<(), CoreError> {
    let mut disk_paths = BTreeSet::new();
    let mut walk = Vec::new();
    walk_files(root, root, ignore, &mut walk)?;
    for rel in walk {
        let full = root.join(&rel);
        if is_indexable_file(&full) {
            disk_paths.insert(rel);
        }
    }
    let mut by_path: HashMap<String, String> =
        index.entries.values().map(|e| (e.path.clone(), e.id.clone())).collect();
    let mut by_hash: HashMap<u64, Vec<String>> = HashMap::new();
    for entry in index.entries.values() {
        by_hash.entry(entry.hash).or_default().push(entry.path.clone());
    }
    for entry in index.entries.values_mut() {
        let full = root.join(&entry.path);
        if !full.is_file() {
            continue;
        }
        let Ok(hash) = file_content_hash(&full) else {
            continue;
        };
        if hash != entry.hash {
            entry.hash = hash;
        }
    }
    for path in disk_paths.iter() {
        if by_path.contains_key(path) {
            continue;
        }
        let full = root.join(path);
        let hash = file_content_hash(&full).unwrap_or(0);
        if let Some(candidates) = by_hash.get(&hash) {
            if let Some(old) = candidates.iter().find(|p| !disk_paths.contains(*p)) {
                if let Some(id) = by_path.remove(old) {
                    if let Some(entry) = index.entries.get_mut(&id) {
                        entry.path = path.clone();
                        by_path.insert(path.clone(), id);
                        continue;
                    }
                }
            }
        }
        let id = new_asset_id(index);
        index.entries.insert(id.clone(), IndexEntry { id: id.clone(), path: path.clone(), hash });
        by_path.insert(path.clone(), id);
        by_hash.entry(hash).or_default().push(path.clone());
    }
    for entry in index.entries.values() {
        if !disk_paths.contains(&entry.path) && !root.join(&entry.path).is_file() {
            // stale path left until hash match or manual rebuild
        }
    }
    save_index(root, index)
}

pub fn ensure_index_on_open(session: &mut EditorSession) {
    let Some(project) = session.project.clone() else {
        return;
    };
    let root = project.root.clone();
    let ignore = project.ignore.clone();
    let damaged = load_index(&root).is_err();
    if damaged {
        session
            .notify(format!("{INDEX_FILE} was damaged and was rebuilt from project files"), false);
    }
    match load_index(&root) {
        Ok(mut index) if !index.entries.is_empty() && !damaged => {
            if let Err(err) = sync_index(&root, &mut index, &ignore) {
                session.notify(err.to_string(), true);
            }
            session.models.asset_index = index;
        }
        _ => match rebuild_index(&root, &ignore) {
            Ok(index) => session.models.asset_index = index,
            Err(err) => session.notify(err.to_string(), true),
        },
    }
}

pub fn sync_index_from_watcher(session: &mut EditorSession) {
    let Some(project) = session.project.clone() else {
        return;
    };
    let root = project.root.clone();
    let ignore = project.ignore.clone();
    let mut index = session.models.asset_index.clone();
    if let Err(err) = sync_index(&root, &mut index, &ignore) {
        session.notify(err.to_string(), true);
    } else {
        session.models.asset_index = index;
    }
}

fn is_reference_file(rel: &str) -> bool {
    REFERENCE_SUFFIXES.iter().any(|suf| rel.ends_with(suf))
}

fn collect_reference_files(root: &Path, ignore: &[String]) -> Vec<String> {
    let mut paths = Vec::new();
    let _ = walk_files(root, root, ignore, &mut paths);
    paths.retain(|p| is_reference_file(p) && p != INDEX_FILE && p != "project.seal");
    paths
}

pub fn find_references(root: &Path, target: &str, ignore: &[String]) -> ReferenceReport {
    let target = normalize_rel(target);
    let is_dir = root.join(&target).is_dir();
    let mut report = ReferenceReport::default();
    for file in collect_reference_files(root, ignore) {
        let full = root.join(&file);
        let Ok(text) = fs::read_to_string(&full) else {
            continue;
        };
        let hits = references_in_text(&file, &text, &target, is_dir);
        for hit in hits {
            report.referencers.push(ReferenceSite { file: file.clone(), detail: hit });
        }
    }
    if !is_dir {
        if let Ok(text) = fs::read_to_string(root.join(&target)) {
            report.dependencies = dependencies_in_text(&text, root);
        }
    }
    report
}

/// One path-like token in a project text file: its byte range (without quotes) and its text.
struct Token {
    range: std::ops::Range<usize>,
    text: String,
    line: usize,
}

/// Quoted strings everywhere; bare words too in files that are not SealText (line formats such as
/// `.sealui`, `.fx`, `.frames`, `.soundset`), where paths are written without quotes.
fn path_tokens(text: &str, bare: bool) -> Vec<Token> {
    let mut out = Vec::new();
    let mut line = 1;
    let mut chars = text.char_indices().peekable();
    while let Some((i, ch)) = chars.next() {
        match ch {
            '\n' => line += 1,
            '"' => {
                let start = i + 1;
                let mut value = String::new();
                let mut end = text.len();
                while let Some((j, c)) = chars.next() {
                    if c == '\\' {
                        if let Some((_, n)) = chars.next() {
                            value.push(n);
                        }
                        continue;
                    }
                    if c == '"' {
                        end = j;
                        break;
                    }
                    if c == '\n' {
                        line += 1;
                    }
                    value.push(c);
                }
                out.push(Token { range: start..end, text: value, line });
            }
            '#' if bare => {
                // A comment runs to the end of the line.
                while let Some(&(_, c)) = chars.peek() {
                    if c == '\n' {
                        break;
                    }
                    chars.next();
                }
            }
            c if bare && !c.is_whitespace() => {
                let mut end = i + c.len_utf8();
                while let Some(&(j, n)) = chars.peek() {
                    if n.is_whitespace() || n == '"' {
                        break;
                    }
                    end = j + n.len_utf8();
                    chars.next();
                }
                out.push(Token { range: i..end, text: text[i..end].to_string(), line });
            }
            _ => {}
        }
    }
    out
}

fn bare_paths(file: &str, text: &str) -> bool {
    file.ends_with(".soundset") || parse(text).is_err()
}

/// Folder of `file` when its paths may be relative to it (sound sets resolve entries there first).
fn relative_base(file: &str) -> Option<&str> {
    if file.ends_with(".soundset") {
        Some(file.rsplit_once('/').map(|(dir, _)| dir).unwrap_or(""))
    } else {
        None
    }
}

/// The project-relative form of a token written in `file`.
fn resolve_token(file: &str, token: &str) -> String {
    match relative_base(file) {
        Some(dir) if !dir.is_empty() && !token.contains('/') => format!("{dir}/{token}"),
        _ => token.to_string(),
    }
}

fn references_in_text(file: &str, text: &str, target: &str, is_dir: bool) -> Vec<String> {
    path_tokens(text, bare_paths(file, text))
        .into_iter()
        .filter(|t| path_matches_reference(&resolve_token(file, &t.text), target, is_dir))
        .map(|t| format!("line {}: {}", t.line, t.text))
        .collect()
}

fn path_matches_reference(path: &str, target: &str, is_dir: bool) -> bool {
    let path = normalize_rel(path);
    if is_dir {
        path == target || path.starts_with(&format!("{target}/"))
    } else if let Some(rest) = path.strip_prefix("prefab:") {
        normalize_rel(rest) == target
    } else {
        path == target
    }
}

fn dependencies_in_text(text: &str, root: &Path) -> Vec<String> {
    if text.contains("\nentry ") {
        if let Ok(def) = SoundContainerDef::parse(text) {
            return def.entries.iter().map(|e| e.path.clone()).collect();
        }
    }
    let Ok(value) = parse(text) else {
        return Vec::new();
    };
    let mut deps = BTreeSet::new();
    collect_string_paths(&value, root, &mut deps);
    deps.into_iter().collect()
}

fn collect_string_paths(value: &SealValue, root: &Path, out: &mut BTreeSet<String>) {
    match value {
        SealValue::String(text) => {
            let text = normalize_rel(text);
            if text.contains('/') || root.join(&text).exists() {
                if let Some(rest) = text.strip_prefix("prefab:") {
                    out.insert(normalize_rel(rest));
                } else if !text.is_empty() {
                    out.insert(text);
                }
            }
        }
        SealValue::Array(items) => items.iter().for_each(|v| collect_string_paths(v, root, out)),
        SealValue::Map(map) => {
            map.entries.iter().for_each(|(_, v)| collect_string_paths(v, root, out))
        }
        _ => {}
    }
}

pub fn preview_rewrite(root: &Path, from: &str, to: &str, ignore: &[String]) -> RewritePreview {
    let from = normalize_rel(from);
    let to = normalize_rel(to);
    let is_dir = root.join(&from).is_dir();
    let mut files = 0usize;
    let mut references = 0usize;
    for file in collect_reference_files(root, ignore) {
        let full = root.join(&file);
        let Ok(text) = fs::read_to_string(&full) else {
            continue;
        };
        let hits = references_in_text(&file, &text, &from, is_dir);
        if !hits.is_empty() {
            files += 1;
            references += hits.len();
        }
    }
    if let Ok(book_text) = fs::read_to_string(root.join(import_presets::BOOK_FILE)) {
        if book_text.contains(&from) {
            files += 1;
            references += 1;
        }
    }
    let _ = to;
    RewritePreview { references, files }
}

pub fn rewrite_project_paths(
    root: &Path,
    from: &str,
    to: &str,
    ignore: &[String],
) -> Result<Vec<(String, String, String)>, CoreError> {
    let from = normalize_rel(from);
    let to = normalize_rel(to);
    let is_dir = root.join(&from).is_dir();
    let mut snapshots = Vec::new();
    for file in collect_reference_files(root, ignore) {
        let full = root.join(&file);
        let Ok(before) = fs::read_to_string(&full) else {
            continue;
        };
        let after = rewrite_text_for(&file, &before, &from, &to, is_dir);
        if after != before {
            atomic_write(&full, after.as_bytes())?;
            snapshots.push((file, before, after));
        }
    }
    let book_path = root.join(import_presets::BOOK_FILE);
    if book_path.is_file() {
        if let Ok(before) = fs::read_to_string(&book_path) {
            let after = rewrite_import_book_text(&before, &from, &to, is_dir);
            if after != before {
                atomic_write(&book_path, after.as_bytes())?;
                snapshots.push((import_presets::BOOK_FILE.into(), before, after));
            }
        }
    }
    Ok(snapshots)
}

/// Replace matching path tokens in place; every other byte of the file stays as it was.
fn rewrite_text_for(file: &str, text: &str, from: &str, to: &str, is_dir: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for token in path_tokens(text, bare_paths(file, text)) {
        let resolved = resolve_token(file, &token.text);
        let Some(next) = rewrite_path_token(&resolved, from, to, is_dir) else { continue };
        // Keep a set-relative entry relative when it stays in the set's folder.
        let written = match relative_base(file) {
            Some(dir) if resolved != token.text => next
                .strip_prefix(&format!("{dir}/"))
                .filter(|r| !r.contains('/'))
                .map(str::to_string)
                .unwrap_or(next),
            _ => next,
        };
        out.push_str(&text[at..token.range.start]);
        out.push_str(&written);
        at = token.range.end;
    }
    out.push_str(&text[at..]);
    out
}

fn rewrite_import_book_text(text: &str, from: &str, to: &str, is_dir: bool) -> String {
    let Ok(mut value) = parse(text) else {
        return text.to_string();
    };
    rewrite_value(&mut value, from, to, is_dir);
    value.to_text()
}

fn rewrite_value(value: &mut SealValue, from: &str, to: &str, is_dir: bool) {
    match value {
        SealValue::String(text) => {
            if let Some(next) = rewrite_path_token(text, from, to, is_dir) {
                *text = next;
            }
        }
        SealValue::Array(items) => {
            items.iter_mut().for_each(|v| rewrite_value(v, from, to, is_dir))
        }
        SealValue::Map(map) => {
            map.entries.iter_mut().for_each(|(_, v)| rewrite_value(v, from, to, is_dir))
        }
        _ => {}
    }
}

fn rewrite_path_token(path: &str, from: &str, to: &str, is_dir: bool) -> Option<String> {
    let norm = normalize_rel(path);
    if let Some(rest) = norm.strip_prefix("prefab:") {
        let rest = normalize_rel(rest);
        if is_dir {
            if rest == from || rest.starts_with(&format!("{from}/")) {
                let suffix = rest.strip_prefix(from).unwrap_or("");
                return Some(format!("prefab:{to}{suffix}"));
            }
        } else if rest == from {
            return Some(format!("prefab:{to}"));
        }
        return None;
    }
    if is_dir {
        if norm == from {
            return Some(to.to_string());
        }
        if let Some(suffix) = norm.strip_prefix(&format!("{from}/")) {
            return Some(format!("{to}/{suffix}"));
        }
    } else if norm == from {
        return Some(to.to_string());
    }
    None
}

fn sidecar_snapshot(root: &Path, rel: &str) -> Option<String> {
    let path = import_presets::sidecar_path(root, rel);
    path.is_file().then(|| fs::read_to_string(&path).unwrap_or_default())
}

pub fn move_import_sidecar(
    root: &Path,
    from: &str,
    to: &str,
) -> Result<(Option<String>, Option<String>), CoreError> {
    let from_path = import_presets::sidecar_path(root, from);
    let to_path = import_presets::sidecar_path(root, to);
    let before = sidecar_snapshot(root, from);
    if from_path.is_file() {
        if let Some(parent) = to_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&from_path, &to_path)?;
    }
    let after = sidecar_snapshot(root, to);
    Ok((before, after))
}

fn update_index_paths(index: &mut AssetIndex, from: &str, to: &str) {
    let from = normalize_rel(from);
    let to = normalize_rel(to);
    let is_dir = from.contains('/')
        && index.entries.values().any(|e| e.path.starts_with(&format!("{from}/")));
    let mut next = BTreeMap::new();
    for entry in index.entries.values() {
        let mut path = entry.path.clone();
        if let Some(updated) = rewrite_path_token(
            &path,
            &from,
            &to,
            is_dir || entry.path.starts_with(&format!("{from}/")),
        ) {
            path = updated;
        }
        let mut row = entry.clone();
        row.path = path;
        next.insert(row.id.clone(), row);
    }
    index.entries = next;
}

pub fn begin_content_path_change(
    session: &mut EditorSession,
    action: &str,
    from: &str,
    to: &str,
) -> Result<(), CoreError> {
    let project = session.project.clone().ok_or_else(|| CoreError::new("no project is open"))?;
    if project.read_only {
        return Err(CoreError::new(session.messages.get("content-error-read-only")));
    }
    let root = project.root.clone();
    let from = normalize_rel(from);
    let to = normalize_rel(to);
    let preview = preview_rewrite(&root, &from, &to, &project.ignore);
    if preview.references > 0 {
        let refs = preview.references;
        let files = preview.files;
        session.models.pending_content_path =
            Some(PendingContentPath { action: action.into(), from, to, preview });
        let body = session.messages.format(
            "modal-content-path-body",
            &[("references", &refs.to_string()), ("files", &files.to_string())],
        );
        session.foundation.modal = Some(crate::foundation::Modal {
            kind: crate::foundation::ModalKind::ContentPathConfirm,
            title: session.messages.get("modal-content-path-title"),
            body,
            confirm: session.messages.get("modal-content-path-confirm"),
            alternate: String::new(),
            cancel: session.messages.get("modal-cancel"),
        });
        return Ok(());
    }
    apply_content_path_change(session, &from, &to)
}

pub fn apply_content_path_change(
    session: &mut EditorSession,
    from: &str,
    to_rel: &str,
) -> Result<(), CoreError> {
    let project = session.project.clone().ok_or_else(|| CoreError::new("no project is open"))?;
    let root = project.root.clone();
    let from = normalize_rel(from);
    let to_rel = normalize_rel(to_rel);
    let from_path = root.join(&from);
    let to_path = root.join(&to_rel);
    let snapshots = rewrite_project_paths(&root, &from, &to_rel, &project.ignore)?;
    let import_before = fs::read_to_string(root.join(import_presets::BOOK_FILE)).ok();
    let mut sidecar_snaps = Vec::new();
    if let Ok((before, after)) = move_import_sidecar(&root, &from, &to_rel) {
        if before.is_some() || after.is_some() {
            sidecar_snaps.push((from.clone(), before, after));
        }
    }
    fs::rename(&from_path, &to_path).map_err(CoreError::from)?;
    let import_after = fs::read_to_string(root.join(import_presets::BOOK_FILE)).ok();
    update_index_paths(&mut session.models.asset_index, &from, &to_rel);
    save_index(&root, &session.models.asset_index)?;
    if session.models.content_pick.as_deref() == Some(from.as_str()) {
        session.models.content_pick = Some(to_rel.clone());
    }
    refresh_content_tree(session);
    let change = ContentPathChange {
        from: from.to_string(),
        to: to_rel,
        file_snapshots: snapshots,
        sidecar_snapshots: sidecar_snaps,
        import_book_before: import_before,
        import_book_after: import_after,
    };
    session.push_past(Past::ContentPathChange { change: change });
    session.future.clear();
    session.models.pending_content_path = None;
    Ok(())
}

pub fn confirm_pending_content_path(session: &mut EditorSession) -> Result<(), CoreError> {
    let Some(pending) = session.models.pending_content_path.clone() else {
        return Ok(());
    };
    apply_content_path_change(session, &pending.from, &pending.to)
}

pub fn undo_content_path_change(session: &mut EditorSession, past: &Past) {
    let Past::ContentPathChange { change } = past else {
        return;
    };
    let Some(project) = session.project.clone() else {
        return;
    };
    let root = project.root.clone();
    for (file, before, _) in &change.file_snapshots {
        let _ = atomic_write(&root.join(file), before.as_bytes());
    }
    if let Some(text) = &change.import_book_before {
        let _ = atomic_write(&root.join(import_presets::BOOK_FILE), text.as_bytes());
    }
    for (rel, before, _) in &change.sidecar_snapshots {
        let path = import_presets::sidecar_path(&root, rel);
        match before {
            Some(text) => {
                let _ = atomic_write(&path, text.as_bytes());
            }
            None if path.exists() => {
                let _ = fs::remove_file(path);
            }
            _ => {}
        }
    }
    let from_path = root.join(&change.to);
    let to_path = root.join(&change.from);
    let _ = fs::rename(from_path, to_path);
    update_index_paths(&mut session.models.asset_index, &change.to, &change.from);
    let _ = save_index(&root, &session.models.asset_index);
    refresh_content_tree(session);
}

pub fn redo_content_path_change(session: &mut EditorSession, past: &Past) {
    let Past::ContentPathChange { change } = past else {
        return;
    };
    let Some(project) = session.project.clone() else {
        return;
    };
    let root = project.root.clone();
    for (file, _, after) in &change.file_snapshots {
        let _ = atomic_write(&root.join(file), after.as_bytes());
    }
    if let Some(text) = &change.import_book_after {
        let _ = atomic_write(&root.join(import_presets::BOOK_FILE), text.as_bytes());
    }
    for (rel, _, after) in &change.sidecar_snapshots {
        let path = import_presets::sidecar_path(&root, rel);
        match after {
            Some(text) => {
                let _ = atomic_write(&path, text.as_bytes());
            }
            None if path.exists() => {
                let _ = fs::remove_file(path);
            }
            _ => {}
        }
    }
    let from_path = root.join(&change.from);
    let to_path = root.join(&change.to);
    let _ = fs::rename(from_path, to_path);
    update_index_paths(&mut session.models.asset_index, &change.from, &change.to);
    let _ = save_index(&root, &session.models.asset_index);
    refresh_content_tree(session);
}

pub fn delete_reference_check(session: &mut EditorSession, rel: &str) -> bool {
    let Some(project) = session.project.clone() else {
        return false;
    };
    let rel = clean_project_rel(rel);
    if rel.is_empty() {
        return false;
    }
    let report = find_references(&project.root, &rel, &project.ignore);
    if report.referencers.is_empty() {
        return false;
    }
    const MAX_ROWS: usize = 20;
    let total = report.referencers.len();
    let mut lines: Vec<String> = report
        .referencers
        .iter()
        .take(MAX_ROWS)
        .map(|site| format!("{} — {}", site.file, site.detail))
        .collect();
    if total > MAX_ROWS {
        let more = (total - MAX_ROWS).to_string();
        lines.push(session.messages.format("modal-delete-asset-more", &[("count", more.as_str())]));
    }
    session.models.pending_delete_path = Some(rel);
    let body = format!(
        "{}\n\n{}",
        session.messages.format("modal-delete-asset-body", &[("count", &total.to_string())]),
        lines.join("\n")
    );
    session.foundation.modal = Some(crate::foundation::Modal {
        kind: crate::foundation::ModalKind::DeleteAssetInUse,
        title: session.messages.get("modal-delete-asset-title"),
        body,
        confirm: session.messages.get("modal-delete-asset-confirm"),
        alternate: String::new(),
        cancel: session.messages.get("modal-cancel"),
    });
    true
}

pub fn cmd_find_references(session: &mut EditorSession, rel: &str) {
    let Some(project) = session.project.clone() else {
        return;
    };
    session.models.asset_reference_target = Some(normalize_rel(rel));
    session.models.asset_reference_report = find_references(&project.root, rel, &project.ignore);
    session.reveal_panel = Some("asset-references".into());
    if !session.foundation.open_panels.iter().any(|p| p == "asset-references") {
        session.foundation.open_panels.push("asset-references".into());
    }
}

pub fn record_import_activity(
    session: &mut EditorSession,
    file: &str,
    importer: &str,
    result: &str,
    started: Instant,
) {
    record_import_activity_hashed(session, file, importer, result, started, "");
}

pub fn record_import_activity_hashed(
    session: &mut EditorSession,
    file: &str,
    importer: &str,
    result: &str,
    started: Instant,
    output_hash: &str,
) {
    let duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    if result == "ok" && importer != "reimport" {
        crate::naming_rules::note_violation(session, file);
    }
    session.models.import_activity.insert(
        0,
        crate::import_activity::ImportActivityEntry {
            at_ms: session.uptime_ms,
            file: file.to_string(),
            importer: importer.to_string(),
            result: result.to_string(),
            duration_ms,
            output_hash: output_hash.to_string(),
        },
    );
    const CAP: usize = 200;
    if session.models.import_activity.len() > CAP {
        session.models.import_activity.truncate(CAP);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tiny_project(tag: u64) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("seal-asset-db-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("project.seal"),
            "schema_version = 1\nname = \"P\"\nid = \"00000000-0000-7000-8000-000000000001\"\ncreated = 1\neditor_version = \"0\"\nmodules = []\nsettings = {}\nignore = [\".seal-editor/\"]\n",
        )
        .unwrap();
        fs::create_dir_all(dir.join("sprites")).unwrap();
        fs::write(dir.join("sprites/a.png"), b"aaa").unwrap();
        dir
    }

    #[test]
    fn index_create_and_hash_move() {
        let dir = tiny_project(1);
        let ignore = vec![".seal-editor/".into()];
        let index = rebuild_index(&dir, &ignore).unwrap();
        assert!(index.entries.values().any(|e| e.path == "sprites/a.png"));
        let id = index.entries.values().find(|e| e.path == "sprites/a.png").unwrap().id.clone();
        fs::rename(dir.join("sprites/a.png"), dir.join("sprites/b.png")).unwrap();
        let mut index2 = load_index(&dir).unwrap();
        sync_index(&dir, &mut index2, &ignore).unwrap();
        assert!(index2.entries.get(&id).is_some_and(|e| e.path == "sprites/b.png"));
    }

    #[test]
    fn damaged_index_rebuild_message() {
        let dir = tiny_project(2);
        fs::write(dir.join(INDEX_FILE), "entries = not-a-map\n").unwrap();
        assert!(load_index(&dir).is_err());
        let ignore = vec![".seal-editor/".into()];
        let index = rebuild_index(&dir, &ignore).unwrap();
        assert!(!index.entries.is_empty());
    }

    #[test]
    fn rewrite_touches_only_the_path_and_keeps_every_other_byte() {
        let dir = tiny_project(11);
        let scene = "# Village layout, keep this note\nentity {\n    name   = \"Héros ✓\"\n    sprite = \"sprites/a.png\"   # the hero\n    other  = \"sprites/a.png.bak\"\n}\n";
        fs::write(dir.join("scene.seal"), scene).unwrap();
        fs::create_dir_all(dir.join("ui")).unwrap();
        let ui = "seal ui 1\npanel Hud\n  image Icon\n    picture sprites/a.png\n    size 20 20\n  label Name\n    text Café sprites/a.png.old\n";
        fs::write(dir.join("ui/Hud.sealui"), ui).unwrap();
        fs::write(dir.join("burst.fx"), "rate 10\nrenderer flipbook sprites/a.png over_life\n")
            .unwrap();
        let ignore = vec![".seal-editor/".into()];
        rewrite_project_paths(&dir, "sprites/a.png", "sprites/hero.png", &ignore).unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("scene.seal")).unwrap(),
            scene.replace("\"sprites/a.png\"", "\"sprites/hero.png\"")
        );
        assert_eq!(
            fs::read_to_string(dir.join("ui/Hud.sealui")).unwrap(),
            ui.replace("picture sprites/a.png\n", "picture sprites/hero.png\n")
        );
        assert_eq!(
            fs::read_to_string(dir.join("burst.fx")).unwrap(),
            "rate 10\nrenderer flipbook sprites/hero.png over_life\n"
        );
        let found = find_references(&dir, "sprites/hero.png", &ignore);
        assert_eq!(found.referencers.len(), 3, "{:?}", found.referencers);
    }

    #[test]
    fn sound_set_entries_relative_to_the_set_are_found_and_kept_relative() {
        let dir = tiny_project(12);
        fs::create_dir_all(dir.join("audio")).unwrap();
        fs::write(dir.join("audio/hit.wav"), b"RIFF").unwrap();
        fs::write(
            dir.join("audio/hits.soundset"),
            "# Seal sound container\nmode random\nentry hit.wav weight 2\n",
        )
        .unwrap();
        let ignore = vec![".seal-editor/".into()];
        assert_eq!(find_references(&dir, "audio/hit.wav", &ignore).referencers.len(), 1);
        rewrite_project_paths(&dir, "audio/hit.wav", "audio/punch.wav", &ignore).unwrap();
        assert!(fs::read_to_string(dir.join("audio/hits.soundset"))
            .unwrap()
            .contains("entry punch.wav weight 2"));
        rewrite_project_paths(&dir, "audio/punch.wav", "sfx/punch.wav", &ignore).unwrap();
        assert!(fs::read_to_string(dir.join("audio/hits.soundset"))
            .unwrap()
            .contains("entry sfx/punch.wav weight 2"));
    }

    #[test]
    fn rewrite_scene_and_skip_unrelated() {
        let dir = tiny_project(3);
        fs::write(
            dir.join("scene.seal"),
            "entity { sprite = \"sprites/a.png\" label = \"sprites/a.png in text\" }\n",
        )
        .unwrap();
        fs::write(dir.join("notes.txt"), "sprites/a.png should not change\n").unwrap();
        let ignore = vec![".seal-editor/".into()];
        let snaps =
            rewrite_project_paths(&dir, "sprites/a.png", "sprites/hero.png", &ignore).unwrap();
        assert_eq!(snaps.len(), 1);
        let scene = fs::read_to_string(dir.join("scene.seal")).unwrap();
        assert!(scene.contains("sprites/hero.png"));
        assert!(scene.contains("sprites/a.png in text"));
        let notes = fs::read_to_string(dir.join("notes.txt")).unwrap();
        assert!(notes.contains("sprites/a.png should not change"));
    }

    #[test]
    fn rewrite_prefab_graph_soundset() {
        let dir = tiny_project(4);
        fs::write(dir.join("sprites/p.prefab"), "entity { sprite = \"sprites/a.png\" }\n").unwrap();
        fs::write(dir.join("sprites/g.graph"), "node { sound = \"sprites/a.png\" }\n").unwrap();
        fs::write(
            dir.join("sprites/c.soundset"),
            "# Seal sound container\nmode random\nentry sprites/a.png\nentry other.wav\n",
        )
        .unwrap();
        fs::write(dir.join("sprites/u.sealui"), "pic = \"sprites/a.png\"\n").unwrap();
        fs::write(dir.join("sprites/f.fx"), "sprite = \"sprites/a.png\"\n").unwrap();
        fs::write(dir.join("sprites/fr.frames"), "image = \"sprites/a.png\"\n").unwrap();
        let ignore = vec![".seal-editor/".into()];
        let snaps = rewrite_project_paths(&dir, "sprites/a.png", "sprites/x.png", &ignore).unwrap();
        assert!(snaps.len() >= 6);
        for (_, _, after) in snaps {
            assert!(after.contains("sprites/x.png"));
            assert!(!after.contains("\"sprites/a.png\""));
        }
    }
}
