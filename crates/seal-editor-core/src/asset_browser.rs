//! Asset Browser state beyond the folder listing: back and forward history,
//! type filters, sort order, favorite folders and collections.
//!
//! Favorites and collections are saved with the project in
//! `.seal-editor/content-favorites.txt` and `.seal-editor/content-collections.txt`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::command::CommandArgs;
use crate::content;
use crate::error::CoreError;
use crate::project::is_ignored;
use crate::session::EditorSession;

/// Asset types the Filters column offers, in the order shown.
pub const FILTER_KINDS: &[&str] = &[
    "scene",
    "prefab",
    "picture",
    "sound",
    "soundset",
    "effect",
    "animation",
    "ui",
    "font",
    "text",
    "material",
    "mesh",
    "other",
];

/// The asset type of a content entry, used by the filters and the tile badge.
pub fn kind_of(name: &str, is_dir: bool) -> &'static str {
    if is_dir {
        return "folder";
    }
    let lower = name.to_ascii_lowercase();
    if lower.rsplit('/').next() == Some("project.seal") {
        return "other";
    }
    if lower.ends_with(".seal") {
        return "scene";
    }
    if lower.ends_with(".prefab") {
        return "prefab";
    }
    match crate::asset_editor::asset_kind(name) {
        crate::asset_editor::AssetKind::Image => "picture",
        crate::asset_editor::AssetKind::Sound => "sound",
        crate::asset_editor::AssetKind::Effect => "effect",
        crate::asset_editor::AssetKind::Text => "text",
        crate::asset_editor::AssetKind::Animation => "animation",
        crate::asset_editor::AssetKind::SoundSet => "soundset",
        crate::asset_editor::AssetKind::Strings => "text",
        crate::asset_editor::AssetKind::SpriteSource => "animation",
        crate::asset_editor::AssetKind::LayeredPicture => "picture",
        crate::asset_editor::AssetKind::Screen => "ui",
        crate::asset_editor::AssetKind::Theme => "ui",
        crate::asset_editor::AssetKind::Font => "font",
        crate::asset_editor::AssetKind::Input => "text",
        crate::asset_editor::AssetKind::Save => "text",
        crate::asset_editor::AssetKind::TileSet => "text",
        crate::asset_editor::AssetKind::DataType => "text",
        crate::asset_editor::AssetKind::Data => "text",
        crate::asset_editor::AssetKind::Test => "text",
        crate::asset_editor::AssetKind::TextureComposite => "picture",
        crate::asset_editor::AssetKind::Material => "material",
        crate::asset_editor::AssetKind::Mesh => "mesh",
        crate::asset_editor::AssetKind::Other => "other",
    }
}

/// One tile in the browser grid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserItem {
    /// Path relative to the project root, `/` separated.
    pub rel: String,
    /// Path relative to the folder being shown (the name, or `sub/name` in a search).
    pub label: String,
    pub is_dir: bool,
    pub kind: &'static str,
    pub size: u64,
    pub modified: u64,
}

/// How the grid is ordered. Folders always come first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortBy {
    #[default]
    Name,
    Type,
    Modified,
    Size,
}

impl SortBy {
    pub const ALL: [SortBy; 4] = [SortBy::Name, SortBy::Type, SortBy::Modified, SortBy::Size];

    pub fn id(self) -> &'static str {
        match self {
            SortBy::Name => "name",
            SortBy::Type => "type",
            SortBy::Modified => "modified",
            SortBy::Size => "size",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.id() == id)
    }
}

/// Browser state kept in the session models.
#[derive(Clone, Debug, Default)]
pub struct BrowserState {
    pub back: Vec<String>,
    pub forward: Vec<String>,
    pub filters: Vec<String>,
    pub sort: SortBy,
    pub descending: bool,
    pub favorites: Vec<String>,
    pub collections: BTreeMap<String, Vec<String>>,
    /// The collection the grid shows instead of a folder.
    pub collection: Option<String>,
}

/// The folder the grid shows, relative to the project root.
pub fn current_rel(session: &EditorSession) -> String {
    match (&session.models.content_root, &session.models.content_tree_root) {
        (Some(dir), Some(root)) => dir
            .strip_prefix(root)
            .map(|rel| rel.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn join_rel(base: &str, name: &str) -> String {
    if base.is_empty() {
        name.to_string()
    } else {
        format!("{base}/{name}")
    }
}

fn stamp(meta: &fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

fn item(root: &Path, rel: &str, label: String) -> Option<BrowserItem> {
    let path = root.join(rel);
    let meta = fs::metadata(&path).ok()?;
    let name = rel.rsplit('/').next().unwrap_or(rel);
    let is_dir = meta.is_dir();
    Some(BrowserItem {
        rel: rel.to_string(),
        label,
        is_dir,
        kind: kind_of(name, is_dir),
        size: if is_dir { 0 } else { meta.len() },
        modified: stamp(&meta),
    })
}

/// Everything the grid shows, filtered and sorted. A search or an active type
/// filter looks through subfolders too; otherwise only the current folder is listed.
pub fn browser_items(session: &EditorSession) -> Vec<BrowserItem> {
    let _generation = session.models.content_generation;
    let Some(root) = session.models.content_tree_root.clone() else {
        return Vec::new();
    };
    let state = &session.models.asset_browser;
    let query = session.models.content_query.to_lowercase();
    let filters = &state.filters;
    let mut items = Vec::new();
    if let Some(name) = &state.collection {
        for rel in state.collections.get(name).into_iter().flatten() {
            let label = rel.rsplit('/').next().unwrap_or(rel).to_string();
            if let Some(found) = item(&root, rel, label) {
                items.push(found);
            }
        }
    } else {
        let show_hidden = session.foundation.settings.get("content.show_hidden") == Some("true");
        let ignore =
            session.project.as_ref().map(|project| project.ignore.clone()).unwrap_or_default();
        let here = current_rel(session);
        let deep = !query.is_empty() || !filters.is_empty();
        collect(&root, &here, "", deep, show_hidden, &ignore, &mut items, 0);
    }
    items.retain(|it| {
        let name = it.rel.rsplit('/').next().unwrap_or(&it.rel).to_lowercase();
        let matches_query = query.is_empty() || name.contains(&query);
        let matches_kind = if filters.is_empty() {
            true
        } else {
            !it.is_dir && filters.iter().any(|f| f == it.kind)
        };
        matches_query && matches_kind
    });
    sort_items(&mut items, state.sort, state.descending);
    items
}

#[allow(clippy::too_many_arguments)]
fn collect(
    root: &Path,
    dir_rel: &str,
    label_prefix: &str,
    deep: bool,
    show_hidden: bool,
    ignore: &[String],
    out: &mut Vec<BrowserItem>,
    depth: usize,
) {
    let Ok(entries) = fs::read_dir(root.join(dir_rel)) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = join_rel(dir_rel, &name);
        if !content::visible_name(&name, show_hidden) || is_ignored(&rel, ignore) {
            continue;
        }
        let label = join_rel(label_prefix, &name);
        let Some(found) = item(root, &rel, label.clone()) else {
            continue;
        };
        let is_dir = found.is_dir;
        out.push(found);
        let is_link = entry.file_type().map(|t| t.is_symlink()).unwrap_or(false);
        if deep && is_dir && !is_link && depth < 32 {
            collect(root, &rel, &label, deep, show_hidden, ignore, out, depth + 1);
        }
    }
}

pub fn sort_items(items: &mut [BrowserItem], by: SortBy, descending: bool) {
    items.sort_by(|a, b| {
        let natural =
            || crate::outliner_query::natural_cmp(&a.label.to_lowercase(), &b.label.to_lowercase());
        let order = match by {
            SortBy::Name => natural(),
            SortBy::Type => a.kind.cmp(b.kind).then_with(natural),
            SortBy::Modified => a.modified.cmp(&b.modified).then_with(natural),
            SortBy::Size => a.size.cmp(&b.size).then_with(natural),
        };
        let order = if descending { order.reverse() } else { order };
        b.is_dir.cmp(&a.is_dir).then(order)
    });
}

/// Folder rows for the sources tree: folders only, opened per the tree state.
pub fn folder_rows(session: &EditorSession) -> Vec<content::ContentEntry> {
    // The tree ignores the grid search.
    let mut rows = content::content_tree_rows_unfiltered(session).unwrap_or_default();
    rows.retain(|row| row.is_dir);
    rows
}

/// Move the grid to `rel`, remembering where it was for Back.
pub fn go_to(session: &mut EditorSession, rel: &str) {
    let here = current_rel(session);
    let leaving_collection = session.models.asset_browser.collection.take().is_some();
    if here != rel || leaving_collection {
        session.models.asset_browser.back.push(here);
        session.models.asset_browser.forward.clear();
    }
    open_rel(session, rel);
}

fn open_rel(session: &mut EditorSession, rel: &str) {
    expand_to(session, rel);
    content::enter_content_dir(session, rel);
}

/// Open every folder above `rel` in the tree so the current folder is visible.
fn expand_to(session: &mut EditorSession, rel: &str) {
    let mut at = String::new();
    for part in rel.split('/').filter(|p| !p.is_empty()) {
        at = join_rel(&at, part);
        session.models.content_expanded.insert(at.clone());
    }
}

fn back(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    if let Some(rel) = session.models.asset_browser.back.pop() {
        let here = current_rel(session);
        session.models.asset_browser.forward.push(here);
        session.models.asset_browser.collection = None;
        open_rel(session, &rel);
    }
    Ok(())
}

fn forward(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    if let Some(rel) = session.models.asset_browser.forward.pop() {
        let here = current_rel(session);
        session.models.asset_browser.back.push(here);
        session.models.asset_browser.collection = None;
        open_rel(session, &rel);
    }
    Ok(())
}

fn navigate(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let rel = args.get("path").unwrap_or("").trim_matches('/').to_string();
    go_to(session, &rel);
    Ok(())
}

fn filter(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let kind = args.get("kind").unwrap_or("");
    let filters = &mut session.models.asset_browser.filters;
    if kind.is_empty() {
        filters.clear();
    } else if let Some(at) = filters.iter().position(|f| f == kind) {
        filters.remove(at);
    } else if FILTER_KINDS.contains(&kind) {
        filters.push(kind.to_string());
    } else {
        return Err(CoreError::new(format!("unknown content type {kind}")));
    }
    Ok(())
}

fn sort(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let state = &mut session.models.asset_browser;
    match args.get("by") {
        Some(id) => {
            let by = SortBy::from_id(id)
                .ok_or_else(|| CoreError::new(format!("unknown sort order {id}")))?;
            if state.sort == by {
                state.descending = !state.descending;
            } else {
                state.sort = by;
                state.descending = false;
            }
        }
        None => state.descending = !state.descending,
    }
    Ok(())
}

fn favorite(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let rel = args.get("path").unwrap_or("").trim_matches('/').to_string();
    let favorites = &mut session.models.asset_browser.favorites;
    if let Some(at) = favorites.iter().position(|f| *f == rel) {
        favorites.remove(at);
    } else {
        favorites.push(rel);
    }
    save(session)
}

fn unique_collection_name(session: &EditorSession, base: &str) -> String {
    let taken = &session.models.asset_browser.collections;
    if !taken.contains_key(base) {
        return base.to_string();
    }
    (2..).map(|n| format!("{base} {n}")).find(|name| !taken.contains_key(name)).unwrap_or_default()
}

fn collection_new(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let base = args.get("name").filter(|n| !n.trim().is_empty()).unwrap_or("New Collection");
    let name = unique_collection_name(session, base.trim());
    session.models.asset_browser.collections.insert(name.clone(), Vec::new());
    session.set_draft("content-collection-renaming", &name)?;
    save(session)
}

fn collection_rename(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let from = args.get("from").unwrap_or("").to_string();
    let to = args.get("to").unwrap_or("").trim().to_string();
    session.set_draft("content-collection-renaming", "")?;
    if to.is_empty() || to == from || to.contains(['\n', '[', ']']) {
        return Ok(());
    }
    let to = unique_collection_name(session, &to);
    let state = &mut session.models.asset_browser;
    if let Some(items) = state.collections.remove(&from) {
        state.collections.insert(to.clone(), items);
        if state.collection.as_deref() == Some(from.as_str()) {
            state.collection = Some(to);
        }
    }
    save(session)
}

fn collection_delete(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let name = args.get("name").unwrap_or("");
    let state = &mut session.models.asset_browser;
    state.collections.remove(name);
    if state.collection.as_deref() == Some(name) {
        state.collection = None;
    }
    save(session)
}

fn collection_add(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let name = args.get("name").unwrap_or("").to_string();
    let rel = args.get("path").unwrap_or("").trim_matches('/').to_string();
    let Some(items) = session.models.asset_browser.collections.get_mut(&name) else {
        return Err(CoreError::new(format!("no collection named {name}")));
    };
    if !rel.is_empty() && !items.contains(&rel) {
        items.push(rel);
    }
    save(session)
}

fn collection_remove(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let name = args.get("name").unwrap_or("");
    let rel = args.get("path").unwrap_or("");
    if let Some(items) = session.models.asset_browser.collections.get_mut(name) {
        items.retain(|r| r != rel);
    }
    save(session)
}

fn collection_show(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let name = args.get("name").unwrap_or("");
    let state = &mut session.models.asset_browser;
    if name.is_empty() || !state.collections.contains_key(name) {
        state.collection = None;
    } else {
        state.collection = Some(name.to_string());
    }
    Ok(())
}

type CommandFn = fn(&mut EditorSession, &CommandArgs) -> Result<(), CoreError>;

/// Browser commands as `(id, title message, menu category, handler)`.
pub const COMMANDS: &[(&str, &str, &str, CommandFn)] = &[
    ("content.back", "command-content-back", "view", back),
    ("content.forward", "command-content-forward", "view", forward),
    ("content.go", "command-content-go", "view", navigate),
    ("content.filter", "command-content-filter", "view", filter),
    ("content.sort", "command-content-sort", "view", sort),
    ("content.favorite", "command-content-favorite", "view", favorite),
    ("content.collection_new", "command-content-collection-new", "view", collection_new),
    ("content.collection_rename", "command-content-collection-rename", "view", collection_rename),
    ("content.collection_delete", "command-content-collection-delete", "view", collection_delete),
    ("content.collection_add", "command-content-collection-add", "view", collection_add),
    ("content.collection_remove", "command-content-collection-remove", "view", collection_remove),
    ("content.collection_show", "command-content-collection-show", "view", collection_show),
];

fn store_dir(session: &EditorSession) -> Option<std::path::PathBuf> {
    session.models.content_tree_root.as_ref().map(|root| root.join(".seal-editor"))
}

/// Write favorites and collections next to the project.
fn save(session: &EditorSession) -> Result<(), CoreError> {
    let Some(dir) = store_dir(session) else {
        return Ok(());
    };
    fs::create_dir_all(&dir).map_err(|err| CoreError::new(err.to_string()))?;
    let state = &session.models.asset_browser;
    let mut favorites = state.favorites.join("\n");
    favorites.push('\n');
    crate::persist::atomic_write(&dir.join("content-favorites.txt"), favorites.as_bytes())?;
    let mut text = String::new();
    for (name, items) in &state.collections {
        text.push_str(&format!("[{name}]\n"));
        for rel in items {
            text.push_str(rel);
            text.push('\n');
        }
    }
    crate::persist::atomic_write(&dir.join("content-collections.txt"), text.as_bytes())
}

/// Read favorites and collections for the folder that was just opened.
pub fn load(session: &mut EditorSession) {
    let mut state = BrowserState::default();
    if let Some(dir) = store_dir(session) {
        if let Ok(text) = fs::read_to_string(dir.join("content-favorites.txt")) {
            state.favorites = text.lines().map(str::to_string).filter(|l| !l.is_empty()).collect();
        }
        if let Ok(text) = fs::read_to_string(dir.join("content-collections.txt")) {
            let mut current: Option<String> = None;
            for line in text.lines() {
                if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                    state.collections.entry(name.to_string()).or_default();
                    current = Some(name.to_string());
                } else if let (Some(name), false) = (&current, line.is_empty()) {
                    state.collections.entry(name.clone()).or_default().push(line.to_string());
                }
            }
        }
    }
    let old = std::mem::take(&mut session.models.asset_browser);
    state.sort = old.sort;
    state.descending = old.descending;
    session.models.asset_browser = state;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Layout;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("seal-browser-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("art/tiles")).unwrap();
        fs::write(dir.join("art/hero.png"), b"png").unwrap();
        fs::write(dir.join("art/tiles/grass.png"), b"png").unwrap();
        fs::write(dir.join("jump.wav"), b"wav").unwrap();
        fs::write(dir.join("level.seal"), b"").unwrap();
        dir
    }

    fn session_at(dir: &Path) -> EditorSession {
        let mut session = EditorSession::new(Layout::default());
        session
            .dispatch("content.open", &CommandArgs::new(&[("path", dir.to_str().unwrap())]))
            .unwrap();
        session
    }

    fn labels(session: &EditorSession) -> Vec<String> {
        browser_items(session).into_iter().map(|i| i.label).collect()
    }

    #[test]
    fn folders_come_first_and_back_and_forward_retrace_the_path() {
        let dir = scratch("history");
        let mut s = session_at(&dir);
        assert_eq!(labels(&s), ["art", "jump.wav", "level.seal"]);
        s.dispatch("content.go", &CommandArgs::new(&[("path", "art")])).unwrap();
        s.dispatch("content.go", &CommandArgs::new(&[("path", "art/tiles")])).unwrap();
        assert_eq!(labels(&s), ["grass.png"]);
        s.dispatch("content.back", &CommandArgs::new(&[])).unwrap();
        assert_eq!(current_rel(&s), "art");
        s.dispatch("content.back", &CommandArgs::new(&[])).unwrap();
        assert_eq!(current_rel(&s), "");
        s.dispatch("content.forward", &CommandArgs::new(&[])).unwrap();
        assert_eq!(current_rel(&s), "art");
        assert!(s.models.content_expanded.contains("art"));
    }

    #[test]
    fn a_type_filter_finds_matching_files_in_subfolders() {
        let dir = scratch("filter");
        let mut s = session_at(&dir);
        s.dispatch("content.filter", &CommandArgs::new(&[("kind", "picture")])).unwrap();
        assert_eq!(labels(&s), ["art/hero.png", "art/tiles/grass.png"]);
        s.dispatch("content.filter", &CommandArgs::new(&[("kind", "sound")])).unwrap();
        assert_eq!(labels(&s), ["art/hero.png", "art/tiles/grass.png", "jump.wav"]);
        s.dispatch("content.filter", &CommandArgs::new(&[("kind", "")])).unwrap();
        assert_eq!(labels(&s), ["art", "jump.wav", "level.seal"]);
    }

    #[test]
    fn sorting_by_type_and_again_reverses() {
        let dir = scratch("sort");
        let mut s = session_at(&dir);
        s.dispatch("content.sort", &CommandArgs::new(&[("by", "type")])).unwrap();
        assert_eq!(labels(&s), ["art", "level.seal", "jump.wav"]);
        s.dispatch("content.sort", &CommandArgs::new(&[("by", "type")])).unwrap();
        assert_eq!(labels(&s), ["art", "jump.wav", "level.seal"]);
    }

    #[test]
    fn favorites_and_collections_survive_reopening_the_project() {
        let dir = scratch("saved");
        let mut s = session_at(&dir);
        s.dispatch("content.favorite", &CommandArgs::new(&[("path", "art/tiles")])).unwrap();
        s.dispatch("content.collection_new", &CommandArgs::new(&[("name", "Level 1")])).unwrap();
        s.dispatch(
            "content.collection_add",
            &CommandArgs::new(&[("name", "Level 1"), ("path", "jump.wav")]),
        )
        .unwrap();
        let s = session_at(&dir);
        assert_eq!(s.models.asset_browser.favorites, ["art/tiles"]);
        assert_eq!(s.models.asset_browser.collections["Level 1"], ["jump.wav"]);
        let mut s = s;
        s.dispatch("content.collection_show", &CommandArgs::new(&[("name", "Level 1")])).unwrap();
        assert_eq!(labels(&s), ["jump.wav"]);
    }
}
