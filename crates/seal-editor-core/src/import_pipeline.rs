//! Parallel, deterministic imports, post-import graph scripts, custom importers,
//! and cancelable progress for long jobs.
//!
//! A batch (Import…, a dropped folder, a watcher burst) is planned on the worker
//! pool from the Worker Threads preference. Planning does not write. Files are
//! then written one at a time, in source-path order, so the same sources and
//! settings produce the same bytes whichever thread count ran the plan.
//! Import Activity stores the output hash of each file.
//!
//! A project assigns a graph per asset kind under Project Settings. The graph's
//! import nodes can set the pivot, rename the file, add a tag, or replace text.
//! A project graph or a compiled-in hook can claim a file extension.
//!
//! Batch import, export, and reindex publish a progress fraction on a status-bar
//! task. Cancel stops before the next file is committed and deletes any partial
//! file left behind.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use seal_graph::{Graph, Link, Node};
use seal_sealtext::{parse, SealValue};

use crate::command::CommandArgs;
use crate::editor_tools::{self, map_in_order};
use crate::error::CoreError;
use crate::import_presets::{self, ImportKind};
use crate::persist::atomic_write;
use crate::session::EditorSession;

pub const BOOK_FILE: &str = "import-pipeline.seal";
const BOOK_SCHEMA: u64 = 1;
const TAG_FILE: &str = ".seal-editor/import-tags.seal";

const KIND_IDS: &[&str] = &["picture", "sound", "font", "effect", "file"];

/// One post-import graph, keyed by asset kind (`picture`, `sound`, `font`, `effect`, `file`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KindScript {
    pub kind: String,
    pub graph: String,
}

/// A project importer for one file extension. The extension has no leading dot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomImporter {
    pub extension: String,
    pub graph: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PipelineBook {
    pub scripts: Vec<KindScript>,
    pub importers: Vec<CustomImporter>,
    pub load_warning: Option<String>,
}

impl Default for PipelineBook {
    fn default() -> Self {
        Self { scripts: Vec::new(), importers: Vec::new(), load_warning: None }
    }
}

impl PipelineBook {
    pub fn script_for(&self, kind: &str) -> Option<&str> {
        self.scripts
            .iter()
            .find(|row| row.kind == kind && !row.graph.is_empty())
            .map(|row| row.graph.as_str())
    }

    pub fn importer_for(&self, extension: &str) -> Option<&CustomImporter> {
        let extension = normalize_ext(extension);
        self.importers.iter().find(|row| row.extension == extension && !row.graph.is_empty())
    }
}

/// Values a post-import graph or a compiled-in importer hook can read and change.
#[derive(Clone, Debug)]
pub struct Scratch {
    pub rel: String,
    pub file_name: String,
    pub kind: String,
    pub bytes: Vec<u8>,
    pub pivot_x: f32,
    pub pivot_y: f32,
    pub pivot_set: bool,
    pub tags: Vec<String>,
    pub text_set: bool,
}

struct Hook {
    extension: String,
    label: String,
    apply: fn(&mut Scratch) -> Result<(), String>,
}

fn hook_slot() -> &'static Mutex<Vec<Hook>> {
    static HOOKS: Mutex<Vec<Hook>> = Mutex::new(Vec::new());
    &HOOKS
}

/// Register a compiled-in importer for `extension` (no dot). A project importer
/// for the same extension wins.
pub fn register_importer_hook(
    extension: &str,
    label: &str,
    apply: fn(&mut Scratch) -> Result<(), String>,
) {
    let extension = normalize_ext(extension);
    let mut hooks = hook_slot().lock().unwrap_or_else(|err| err.into_inner());
    hooks.retain(|hook| hook.extension != extension);
    hooks.push(Hook { extension, label: label.to_string(), apply });
}

pub fn remove_importer_hook(extension: &str) {
    let extension = normalize_ext(extension);
    let mut hooks = hook_slot().lock().unwrap_or_else(|err| err.into_inner());
    hooks.retain(|hook| hook.extension != extension);
}

fn hook_for(extension: &str) -> Option<(String, fn(&mut Scratch) -> Result<(), String>)> {
    let extension = normalize_ext(extension);
    let hooks = hook_slot().lock().unwrap_or_else(|err| err.into_inner());
    hooks
        .iter()
        .find(|hook| hook.extension == extension)
        .map(|hook| (hook.label.clone(), hook.apply))
}

/// One file after planning. `error` skips the write. `hash` is the output hash.
#[derive(Clone, Debug, PartialEq)]
pub struct PlannedFile {
    pub source: PathBuf,
    pub dest_rel: String,
    pub bytes: Vec<u8>,
    pub file_name: String,
    pub kind: String,
    pub pivot_set: bool,
    pub pivot_x: f32,
    pub pivot_y: f32,
    pub tags: Vec<String>,
    pub text_set: bool,
    pub importer: String,
    pub hash: String,
    pub error: Option<String>,
    pub place_sprite: bool,
}

#[derive(Clone, Debug)]
pub struct ExportRequest {
    pub preset: String,
    pub exe: String,
    pub out: String,
    pub run: bool,
}

#[derive(Clone, Debug)]
enum JobKind {
    Import {
        steps: Vec<PlannedFile>,
        index: usize,
        place_a: String,
        place_b: String,
        done_names: Vec<String>,
    },
    Reindex {
        files: Vec<String>,
        index: usize,
        ignore: Vec<String>,
        stamp: String,
    },
    Export {
        request: ExportRequest,
        phase: u8,
    },
}

#[derive(Clone, Debug)]
pub struct LongJob {
    pub id: String,
    title: String,
    kind: JobKind,
    pub fraction: f32,
}

/// Stable hash of the bytes that will be written plus the import metadata.
pub fn output_hash(bytes: &[u8], meta: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes.iter().chain(meta.as_bytes()) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn meta_line(plan: &PlannedFile) -> String {
    let pivot = if plan.pivot_set {
        format!("{:.4},{:.4}", plan.pivot_x, plan.pivot_y)
    } else {
        String::new()
    };
    let mut tags = plan.tags.clone();
    tags.sort();
    format!(
        "name={}\nkind={}\npivot={pivot}\ntags={}\n",
        plan.file_name,
        plan.kind,
        tags.join(",")
    )
}

pub fn book_path(root: &Path) -> PathBuf {
    root.join(BOOK_FILE)
}

pub fn load_book(root: &Path) -> PipelineBook {
    let path = book_path(root);
    let Ok(text) = fs::read_to_string(&path) else {
        return PipelineBook::default();
    };
    match parse_book(&text) {
        Ok(book) => book,
        Err(message) => PipelineBook {
            load_warning: Some(format!("{BOOK_FILE}: {message}")),
            ..PipelineBook::default()
        },
    }
}

fn parse_book(text: &str) -> Result<PipelineBook, String> {
    let value = parse(text).map_err(|err| format!("line {}: {}", err.line, err.message))?;
    let Some(_) = value.as_map() else {
        return Err("the file must be a map".into());
    };
    if let Some(SealValue::Number(schema)) = value.get("schema_version") {
        let schema: u64 = schema.parse().unwrap_or(0);
        if schema > BOOK_SCHEMA {
            return Err(format!("schema {schema} is newer than this build reads"));
        }
    }
    let mut book = PipelineBook::default();
    match value.get("scripts") {
        None => {}
        Some(SealValue::Array(rows)) => {
            for row in rows {
                if let Some(script) = parse_script(row) {
                    book.scripts.push(script);
                } else {
                    return Err("a script row needs a kind and a graph path".into());
                }
            }
        }
        Some(_) => return Err("scripts must be a list".into()),
    }
    match value.get("importers") {
        None => {}
        Some(SealValue::Array(rows)) => {
            for row in rows {
                if let Some(importer) = parse_importer(row) {
                    book.importers.push(importer);
                } else {
                    return Err("an importer row needs an extension and a graph path".into());
                }
            }
        }
        Some(_) => return Err("importers must be a list".into()),
    }
    Ok(book)
}

fn parse_script(value: &SealValue) -> Option<KindScript> {
    let kind = match value.get("kind")? {
        SealValue::String(text) if KIND_IDS.contains(&text.as_str()) => text.clone(),
        _ => return None,
    };
    let graph = match value.get("graph") {
        Some(SealValue::String(text)) => text.replace('\\', "/"),
        _ => return None,
    };
    Some(KindScript { kind, graph })
}

fn parse_importer(value: &SealValue) -> Option<CustomImporter> {
    let extension = match value.get("extension")? {
        SealValue::String(text) if !normalize_ext(text).is_empty() => normalize_ext(text),
        _ => return None,
    };
    let graph = match value.get("graph") {
        Some(SealValue::String(text)) => text.replace('\\', "/"),
        _ => return None,
    };
    Some(CustomImporter { extension, graph })
}

pub fn save_book(root: &Path, book: &PipelineBook) -> Result<(), CoreError> {
    let mut scripts = Vec::new();
    for row in &book.scripts {
        scripts.push(SealValue::Map(
            vec![
                ("kind".into(), SealValue::String(row.kind.clone())),
                ("graph".into(), SealValue::String(row.graph.clone())),
            ]
            .into(),
        ));
    }
    let mut importers = Vec::new();
    for row in &book.importers {
        importers.push(SealValue::Map(
            vec![
                ("extension".into(), SealValue::String(row.extension.clone())),
                ("graph".into(), SealValue::String(row.graph.clone())),
            ]
            .into(),
        ));
    }
    let map = SealValue::Map(
        vec![
            ("schema_version".into(), SealValue::Number(BOOK_SCHEMA.to_string())),
            ("scripts".into(), SealValue::Array(scripts)),
            ("importers".into(), SealValue::Array(importers)),
        ]
        .into(),
    );
    atomic_write(&book_path(root), map.to_text().as_bytes())
}

fn normalize_ext(text: &str) -> String {
    text.trim().trim_start_matches('.').to_ascii_lowercase()
}

fn extension_of(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

pub fn import_folder_for(name: &str) -> Option<&'static str> {
    let ext = extension_of(name);
    match ext.as_str() {
        ext if crate::picture::is_extension(ext) || matches!(ext, "ase" | "aseprite" | "psd") => {
            Some("sprites")
        }
        "fx" => Some("fx"),
        "graph" => Some("logic"),
        "seq" => Some("sequences"),
        "wav" | "ogg" | "mp3" | "flac" => Some("sounds"),
        "ttf" | "otf" => Some("fonts"),
        _ => None,
    }
}

fn kind_of_name(name: &str) -> &'static str {
    let ext = extension_of(name);
    match import_folder_for(name) {
        Some("sprites") if crate::picture::is_extension(&ext) => "picture",
        Some("sprites") => "sprite-source",
        Some("sounds") => "sound",
        Some("fonts") => "font",
        Some("fx") => "effect",
        Some("logic") => "graph",
        Some("sequences") => "sequence",
        _ => "file",
    }
}

fn places_sprite(name: &str) -> bool {
    crate::picture::is_extension(&extension_of(name))
}

fn known_in_folder(book: &PipelineBook, name: &str) -> bool {
    if import_folder_for(name).is_some() {
        return true;
    }
    let ext = extension_of(name);
    book.importer_for(&ext).is_some() || hook_for(&ext).is_some()
}

/// Plan `sources` on `workers` threads. Nothing is written.
/// The returned files are in source-path order and carry output hashes.
pub fn plan_imports(
    root: &Path,
    sources: Vec<PathBuf>,
    workers: usize,
    book: &PipelineBook,
    allow_rename: bool,
    keep_existing_path: bool,
) -> Result<(Vec<PlannedFile>, usize), CoreError> {
    let threads = Mutex::new(HashSet::new());
    let raw = map_in_order(workers, sources, |source| {
        threads.lock().unwrap_or_else(|err| err.into_inner()).insert(std::thread::current().id());
        plan_one(root, source, book, allow_rename)
    });
    let thread_count = threads.lock().unwrap_or_else(|err| err.into_inner()).len().max(1);
    let mut planned = Vec::new();
    for item in raw {
        planned.push(item?);
    }
    planned.sort_by(|left, right| left.source.cmp(&right.source));
    if keep_existing_path {
        for plan in &mut planned {
            if let Some(rel) = crate::asset_index::relative_to_project(root, &plan.source) {
                plan.dest_rel = rel;
            }
            plan.place_sprite = false;
        }
    } else {
        assign_names(root, &mut planned);
    }
    for plan in &mut planned {
        if plan.error.is_none() {
            plan.hash = output_hash(&plan.bytes, &meta_line(plan));
        }
    }
    Ok((planned, thread_count))
}

fn plan_one(
    root: &Path,
    source: PathBuf,
    book: &PipelineBook,
    allow_rename: bool,
) -> Result<PlannedFile, CoreError> {
    let file_name = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| CoreError::new("pick a file to import"))?;
    let bytes = fs::read(&source).map_err(|err| CoreError::new(err.to_string()))?;
    let original = bytes.clone();
    let kind = kind_of_name(&file_name).to_string();
    let ext = extension_of(&file_name);
    let mut scratch = Scratch {
        rel: file_name.clone(),
        file_name: file_name.clone(),
        kind: kind.clone(),
        bytes,
        pivot_x: 0.5,
        pivot_y: 0.5,
        pivot_set: false,
        tags: Vec::new(),
        text_set: false,
    };
    let mut importer = "import".to_string();
    let mut error = None;
    if let Some(custom) = book.importer_for(&ext) {
        importer = format!("custom:{ext}");
        if let Err(message) = run_graph_file(root, &custom.graph, &mut scratch) {
            error = Some(message);
        }
    } else if let Some((_, apply)) = hook_for(&ext) {
        importer = format!("custom:{ext}");
        if let Err(message) = apply(&mut scratch) {
            error = Some(message);
        }
    }
    if error.is_none() {
        if let Some(graph) = book.script_for(&scratch.kind) {
            if let Err(message) = run_graph_file(root, graph, &mut scratch) {
                error = Some(message);
            }
        }
    }
    if !allow_rename {
        scratch.file_name = file_name;
        scratch.bytes = original;
        scratch.text_set = false;
    }
    Ok(PlannedFile {
        source,
        dest_rel: String::new(),
        bytes: scratch.bytes,
        file_name: scratch.file_name,
        kind: scratch.kind,
        pivot_set: scratch.pivot_set,
        pivot_x: scratch.pivot_x,
        pivot_y: scratch.pivot_y,
        tags: scratch.tags,
        text_set: scratch.text_set,
        importer,
        hash: String::new(),
        error,
        place_sprite: false,
    })
}

fn assign_names(root: &Path, planned: &mut [PlannedFile]) {
    for plan in planned.iter_mut() {
        if plan.error.is_some() {
            continue;
        }
        let folder_name = import_folder_for(&plan.file_name);
        let folder = match folder_name {
            Some(sub) => root.join(sub),
            None => root.to_path_buf(),
        };
        let target = free_import_name(&folder, &plan.file_name, &plan.bytes);
        plan.file_name = target
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| plan.file_name.clone());
        plan.dest_rel = target
            .strip_prefix(root)
            .map(|rel| rel.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| plan.file_name.clone());
        plan.place_sprite = folder_name == Some("sprites") && places_sprite(&plan.file_name);
    }
}

fn free_import_name(folder: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = Path::new(name);
    let stem = path.file_stem().map(|stem| stem.to_string_lossy().to_string()).unwrap_or_default();
    let ext = path.extension().map(|ext| format!(".{}", ext.to_string_lossy())).unwrap_or_default();
    for index in 1.. {
        let candidate = if index == 1 {
            folder.join(name)
        } else {
            folder.join(format!("{stem}-{index}{ext}"))
        };
        match fs::read(&candidate) {
            Ok(existing) if existing == bytes => return candidate,
            Ok(_) => continue,
            Err(_) => return candidate,
        }
    }
    unreachable!()
}

fn run_graph_file(root: &Path, rel: &str, scratch: &mut Scratch) -> Result<(), String> {
    let path = root.join(rel);
    let text = fs::read_to_string(&path).map_err(|_| format!("import script {rel} was not found"))?;
    let graph = Graph::from_text(&text).map_err(|err| format!("import script {rel}: {err}"))?;
    run_graph(&graph, scratch)
}

fn run_graph(graph: &Graph, scratch: &mut Scratch) -> Result<(), String> {
    let start = graph
        .nodes
        .iter()
        .find(|node| node.kind == "event" && param_str(node, "name").as_deref() == Some("import"))
        .or_else(|| graph.nodes.iter().find(|node| node.kind == "event"));
    let Some(start) = start else {
        let mut nodes: Vec<&Node> =
            graph.nodes.iter().filter(|node| node.kind.starts_with("import_")).collect();
        nodes.sort_by_key(|node| node.id);
        for node in nodes {
            apply_node(graph, node, scratch)?;
        }
        return Ok(());
    };
    let mut next = Some(start.id);
    let mut seen = BTreeSet::new();
    while let Some(id) = next {
        if !seen.insert(id) {
            return Err("import script loops".into());
        }
        let node = graph
            .nodes
            .iter()
            .find(|node| node.id == id)
            .ok_or_else(|| format!("import script is missing node {id}"))?;
        apply_node(graph, node, scratch)?;
        next = graph
            .links
            .iter()
            .find(|link| link.from.node == id && link.from.pin == "then")
            .map(|link| link.to.node);
    }
    Ok(())
}

fn apply_node(graph: &Graph, node: &Node, scratch: &mut Scratch) -> Result<(), String> {
    match node.kind.as_str() {
        "import_set_pivot" => {
            let x = linked_number(graph, node, "x", scratch)?;
            let y = linked_number(graph, node, "y", scratch)?;
            if !x.is_finite() || !y.is_finite() {
                return Err("pivot must be a number".into());
            }
            scratch.pivot_x = x;
            scratch.pivot_y = y;
            scratch.pivot_set = true;
        }
        "import_rename" => {
            if let Some(name) = linked_text(graph, node, "name", scratch) {
                apply_rename(scratch, &name);
            }
        }
        "import_add_tag" => {
            if let Some(tag) = linked_text(graph, node, "tag", scratch) {
                let tag = tag.trim().to_string();
                if !tag.is_empty() && !scratch.tags.iter().any(|have| have == &tag) {
                    scratch.tags.push(tag);
                }
            }
        }
        "import_set_text" => {
            if let Some(text) = linked_text(graph, node, "text", scratch) {
                scratch.bytes = text.into_bytes();
                scratch.text_set = true;
            }
        }
        _ => {}
    }
    Ok(())
}

fn apply_rename(scratch: &mut Scratch, name: &str) {
    let name = name.trim();
    if name.is_empty() || name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return;
    }
    let ext = extension_of(&scratch.file_name);
    scratch.file_name = if Path::new(name).extension().is_some() {
        name.to_string()
    } else if ext.is_empty() {
        name.to_string()
    } else {
        format!("{name}.{ext}")
    };
    scratch.rel = scratch.file_name.clone();
}

fn linked_text(graph: &Graph, node: &Node, pin: &str, scratch: &Scratch) -> Option<String> {
    if let Some(link) = graph.links.iter().find(|link| link.to.node == node.id && link.to.pin == pin)
    {
        return eval_text(graph, link.from.node, &link.from.pin, scratch);
    }
    param_str(node, pin)
}

fn linked_number(graph: &Graph, node: &Node, pin: &str, scratch: &Scratch) -> Result<f32, String> {
    let text = linked_text(graph, node, pin, scratch)
        .ok_or_else(|| format!("{} needs a number {pin}", node.kind))?;
    text.parse::<f32>().map_err(|_| format!("{} {pin} must be a number", node.kind))
}

fn eval_text(graph: &Graph, id: u32, pin: &str, scratch: &Scratch) -> Option<String> {
    let node = graph.nodes.iter().find(|node| node.id == id)?;
    match node.kind.as_str() {
        "const" => param_str(node, "value").or_else(|| param_str(node, "text")),
        "import_path" => Some(scratch.rel.clone()),
        "import_kind" => Some(scratch.kind.clone()),
        "import_name" => Some(scratch.file_name.clone()),
        "concat" => {
            let left = linked_text(graph, node, "a", scratch).unwrap_or_default();
            let right = linked_text(graph, node, "b", scratch).unwrap_or_default();
            Some(format!("{left}{right}"))
        }
        _ if pin == "out" || pin == "then" => param_str(node, "value"),
        _ => param_str(node, pin),
    }
}

fn param_str(node: &Node, key: &str) -> Option<String> {
    match node.param(key)? {
        SealValue::String(text) => Some(text.clone()),
        SealValue::Number(text) => Some(text.clone()),
        SealValue::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

/// Write `bytes` for `path`. When `cancel` is set after the partial file exists,
/// the partial file is removed and the destination is left untouched.
pub fn commit_bytes(path: &Path, bytes: &[u8], cancel: bool) -> Result<bool, CoreError> {
    let partial = partial_path(path);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|err| CoreError::new(err.to_string()))?;
        }
    }
    if let Err(err) = fs::write(&partial, bytes) {
        let _ = fs::remove_file(&partial);
        return Err(CoreError::new(err.to_string()));
    }
    if cancel {
        let _ = fs::remove_file(&partial);
        return Ok(false);
    }
    if let Err(err) = atomic_write(path, bytes) {
        let _ = fs::remove_file(&partial);
        return Err(err);
    }
    let _ = fs::remove_file(&partial);
    Ok(true)
}

fn partial_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".partial");
    path.with_file_name(name)
}

pub fn sweep_partials(root: &Path) {
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let name = path.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default();
            if name.ends_with(".partial") || name.ends_with(".tmp") {
                let _ = fs::remove_file(&path);
            }
        }
    }
}

fn write_plan(root: &Path, plan: &PlannedFile, cancel: bool) -> Result<bool, CoreError> {
    if plan.error.is_some() {
        return Ok(false);
    }
    let dest = root.join(&plan.dest_rel);
    let same = fs::read(&dest).ok().is_some_and(|existing| existing == plan.bytes);
    if !same {
        if !commit_bytes(&dest, &plan.bytes, cancel)? {
            return Ok(false);
        }
    } else if cancel {
        return Ok(false);
    }
    if let Some(kind) = ImportKind::from_asset_path(&plan.dest_rel) {
        import_presets::apply_preset_on_import(root, &import_presets::load_book(root), &plan.dest_rel)?;
        if plan.pivot_set && kind == ImportKind::Picture {
            let mut state = import_presets::load_file_state(root, &plan.dest_rel, kind);
            if let import_presets::ImportSettings::Picture(picture) = &mut state.settings {
                picture.pivot = "custom".into();
                picture.pivot_x = plan.pivot_x;
                picture.pivot_y = plan.pivot_y;
            }
            state.customized = true;
            import_presets::save_file_state(root, &plan.dest_rel, &state)?;
        }
    }
    store_tags(root, &plan.dest_rel, &plan.tags)?;
    Ok(true)
}

fn store_tags(root: &Path, rel: &str, tags: &[String]) -> Result<(), CoreError> {
    let path = root.join(TAG_FILE);
    let mut map = load_tag_map(&path);
    let mut tags = tags.to_vec();
    tags.sort();
    if tags.is_empty() {
        if !map.contains_key(rel) {
            return Ok(());
        }
        map.remove(rel);
    } else if map.get(rel) == Some(&tags) {
        return Ok(());
    } else {
        map.insert(rel.to_string(), tags);
    }
    let rows = map
        .into_iter()
        .map(|(file, tags)| {
            let tags = tags.into_iter().map(SealValue::String).collect();
            SealValue::Map(
                vec![
                    ("path".into(), SealValue::String(file)),
                    ("tags".into(), SealValue::Array(tags)),
                ]
                .into(),
            )
        })
        .collect();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| CoreError::new(err.to_string()))?;
    }
    let body = SealValue::Map(
        vec![
            ("schema_version".into(), SealValue::Number("1".into())),
            ("files".into(), SealValue::Array(rows)),
        ]
        .into(),
    );
    atomic_write(&path, body.to_text().as_bytes())
}

fn load_tag_map(path: &Path) -> BTreeMap<String, Vec<String>> {
    let Ok(text) = fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    let Ok(value) = parse(&text) else {
        return BTreeMap::new();
    };
    let Some(SealValue::Array(rows)) = value.get("files") else {
        return BTreeMap::new();
    };
    let mut map = BTreeMap::new();
    for row in rows {
        let Some(SealValue::String(file)) = row.get("path") else { continue };
        let Some(SealValue::Array(tags)) = row.get("tags") else { continue };
        let tags = tags
            .iter()
            .filter_map(|tag| match tag {
                SealValue::String(text) => Some(text.clone()),
                _ => None,
            })
            .collect();
        map.insert(file.clone(), tags);
    }
    map
}

pub fn tags_of(root: &Path, rel: &str) -> Vec<String> {
    load_tag_map(&root.join(TAG_FILE)).remove(rel).unwrap_or_default()
}

pub fn begin_import_from_arg(
    session: &mut EditorSession,
    raw: &str,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let root = project_root(session)?;
    if session.models.long_job.is_some() {
        return Err(CoreError::new("a long job is already running"));
    }
    let book = load_book(&root);
    if let Some(warning) = &book.load_warning {
        session.notify(warning.clone(), true);
    }
    let sources = collect_sources(&root, &book, raw)?;
    editor_tools::note_worker_budget(&session.foundation.settings);
    let (mut planned, _) =
        plan_imports(&root, sources, editor_tools::active_workers(), &book, true, false)?;
    let place = args.get("place") == Some("1") || (args.get("a").is_some() && args.get("b").is_some());
    for plan in &mut planned {
        plan.place_sprite = plan.place_sprite && place;
    }
    let title = session.messages.get("import-job-import");
    session.start_task("import", title.clone());
    session.set_task_progress("import", 0.0);
    session.models.long_job = Some(LongJob {
        id: "import".into(),
        title,
        kind: JobKind::Import {
            steps: planned,
            index: 0,
            place_a: args.get("a").unwrap_or("").to_string(),
            place_b: args.get("b").unwrap_or("").to_string(),
            done_names: Vec::new(),
        },
        fraction: 0.0,
    });
    Ok(())
}

fn collect_sources(root: &Path, book: &PipelineBook, raw: &str) -> Result<Vec<PathBuf>, CoreError> {
    let sources: Vec<PathBuf> = raw
        .split(['\n', ';'])
        .map(|path| path.trim().trim_matches('"'))
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .collect();
    let mut files = Vec::new();
    for source in sources {
        if source.is_dir() {
            let mut found: Vec<PathBuf> = fs::read_dir(&source)
                .map_err(|err| CoreError::new(err.to_string()))?
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.is_file()
                        && path.file_name().is_some_and(|name| {
                            known_in_folder(book, &name.to_string_lossy())
                        })
                })
                .collect();
            if found.is_empty() {
                return Err(CoreError::new(format!(
                    "{} has no pictures, effects or sounds to import",
                    source.display()
                )));
            }
            found.sort();
            files.extend(found);
        } else if source.is_file() {
            files.push(source);
        } else {
            return Err(CoreError::new(format!("{} was not found", source.display())));
        }
    }
    let _ = root;
    Ok(files)
}

pub fn begin_reindex(session: &mut EditorSession) -> Result<(), CoreError> {
    let root = project_root(session)?;
    if session.models.long_job.is_some() {
        return Err(CoreError::new("a long job is already running"));
    }
    let ignore = session.project.as_ref().map(|project| project.ignore.clone()).unwrap_or_default();
    let stamp = fs::read(root.join(crate::asset_index::INDEX_FILE)).unwrap_or_default();
    let stamp = String::from_utf8_lossy(&stamp).into_owned();
    let mut files = Vec::new();
    walk_files(&root, &root, &ignore, &mut files)?;
    files.sort();
    let title = session.messages.get("import-job-reindex");
    session.start_task("reindex", title.clone());
    session.set_task_progress("reindex", 0.0);
    session.models.long_job = Some(LongJob {
        id: "reindex".into(),
        title,
        kind: JobKind::Reindex { files, index: 0, ignore, stamp },
        fraction: 0.0,
    });
    Ok(())
}

fn walk_files(base: &Path, dir: &Path, ignore: &[String], out: &mut Vec<String>) -> Result<(), CoreError> {
    for entry in fs::read_dir(dir).map_err(|err| CoreError::new(err.to_string()))? {
        let entry = entry.map_err(|err| CoreError::new(err.to_string()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".seal-editor" || name == ".git" || name == crate::asset_index::INDEX_FILE {
            continue;
        }
        let path = entry.path();
        let rel = path
            .strip_prefix(base)
            .map(|rel| rel.to_string_lossy().replace('\\', "/"))
            .unwrap_or(name);
        if ignore.iter().any(|rule| !rule.is_empty() && rel.starts_with(rule.trim_end_matches('/'))) {
            continue;
        }
        if path.is_dir() {
            walk_files(base, &path, ignore, out)?;
        } else if path.is_file() {
            out.push(rel);
        }
    }
    Ok(())
}

pub fn begin_export(session: &mut EditorSession, request: ExportRequest) -> Result<(), CoreError> {
    if session.project.is_none() {
        return Err(CoreError::new(session.messages.get("package-no-project")));
    }
    if session.models.long_job.is_some() {
        return Err(CoreError::new("a long job is already running"));
    }
    crate::export_preset::preflight_export(session, &request)?;
    let title = session.messages.get("import-job-export");
    session.start_task("export", title.clone());
    session.set_task_progress("export", 0.0);
    session.models.long_job = Some(LongJob {
        id: "export".into(),
        title,
        kind: JobKind::Export { request, phase: 0 },
        fraction: 0.0,
    });
    Ok(())
}

pub fn job_fraction(session: &EditorSession) -> Option<f32> {
    session.models.long_job.as_ref().map(|job| job.fraction)
}

pub fn job_active(session: &EditorSession) -> bool {
    session.models.long_job.is_some()
}

/// Advance the running job by up to `budget` steps.
pub fn pump_steps(session: &mut EditorSession, budget: usize) -> Result<(), CoreError> {
    for _ in 0..budget {
        if session.models.long_job.is_none() {
            return Ok(());
        }
        step_job(session)?;
    }
    Ok(())
}

pub fn pump_all(session: &mut EditorSession) -> Result<(), CoreError> {
    let mut guard = 0u32;
    while session.models.long_job.is_some() {
        step_job(session)?;
        guard = guard.saturating_add(1);
        if guard > 100_000 {
            return Err(CoreError::new("the long job did not finish"));
        }
    }
    Ok(())
}

fn task_cancelled(session: &EditorSession, id: &str) -> bool {
    session.foundation.tasks.iter().any(|task| task.id == id && task.cancelled)
}

fn step_job(session: &mut EditorSession) -> Result<(), CoreError> {
    let Some(job) = session.models.long_job.take() else {
        return Ok(());
    };
    if task_cancelled(session, &job.id) {
        abort_job(session, &job)?;
        return Ok(());
    }
    let root = project_root(session)?;
    let id = job.id.clone();
    let title = job.title.clone();
    match job.kind {
        JobKind::Import { steps, mut index, place_a, place_b, mut done_names } => {
            if index >= steps.len() {
                finish_import(session, &done_names)?;
                return Ok(());
            }
            let plan = steps[index].clone();
            let started = Instant::now();
            let wrote = if plan.error.is_some() {
                false
            } else {
                write_plan(&root, &plan, task_cancelled(session, &id))?
            };
            if wrote {
                done_names.push(plan.file_name.clone());
                if plan.place_sprite {
                    place_sprite(session, &root, &plan, &place_a, &place_b)?;
                }
            }
            let status = if plan.error.is_some() || !wrote { "failed" } else { "ok" };
            let file = if plan.dest_rel.is_empty() { plan.file_name.clone() } else { plan.dest_rel.clone() };
            crate::asset_index::record_import_activity_hashed(
                session,
                &file,
                &plan.importer,
                status,
                started,
                &plan.hash,
            );
            if !wrote && plan.error.is_none() && task_cancelled(session, &id) {
                sweep_partials(&root);
                clear_job(session, &id);
                session.notify(session.messages.get("import-job-cancelled"), false);
                return Ok(());
            }
            index += 1;
            let fraction = index as f32 / steps.len().max(1) as f32;
            if index >= steps.len() {
                session.models.long_job = Some(LongJob {
                    id: id.clone(),
                    title,
                    kind: JobKind::Import { steps, index, place_a, place_b, done_names: done_names.clone() },
                    fraction,
                });
                finish_import(session, &done_names)?;
            } else {
                session.set_task_progress(&id, fraction);
                session.models.long_job = Some(LongJob {
                    id,
                    title,
                    kind: JobKind::Import { steps, index, place_a, place_b, done_names },
                    fraction,
                });
            }
        }
        JobKind::Reindex { files, mut index, ignore, stamp } => {
            let total = files.len().saturating_add(1);
            if index < files.len() {
                let rel = files[index].clone();
                let _ = fs::read(root.join(&rel));
                index += 1;
                let fraction = index as f32 / total as f32;
                session.set_task_progress(&id, fraction);
                session.models.long_job = Some(LongJob {
                    id,
                    title,
                    kind: JobKind::Reindex { files, index, ignore, stamp },
                    fraction,
                });
            } else {
                crate::asset_index::rebuild_index(&root, &ignore)?;
                session.set_task_progress(&id, 1.0);
                clear_job(session, &id);
                session.notify(session.messages.get("import-job-reindex-done"), false);
            }
        }
        JobKind::Export { request, phase } => {
            if phase == 0 {
                session.set_task_progress(&id, 0.5);
                session.models.long_job = Some(LongJob {
                    id: id.clone(),
                    title: title.clone(),
                    kind: JobKind::Export { request: request.clone(), phase: 1 },
                    fraction: 0.5,
                });
                crate::export_preset::execute_export(session, &request)?;
                if task_cancelled(session, &id) {
                    if let Some(root) = session.project.as_ref().map(|project| project.root.clone()) {
                        sweep_partials(&root);
                    }
                    clear_job(session, &id);
                    session.notify(session.messages.get("import-job-cancelled"), false);
                    return Ok(());
                }
                session.set_task_progress(&id, 1.0);
                clear_job(session, &id);
            }
        }
    }
    Ok(())
}

fn finish_import(session: &mut EditorSession, names: &[String]) -> Result<(), CoreError> {
    crate::content::refresh_content_tree(session);
    session.models.content_generation = session.models.content_generation.wrapping_add(1);
    clear_job(session, "import");
    if names.is_empty() {
        session.notify(session.messages.get("import-job-import-none"), false);
    } else {
        session.notify(format!("Imported {}", names.join(", ")), false);
    }
    Ok(())
}

fn abort_job(session: &mut EditorSession, job: &LongJob) -> Result<(), CoreError> {
    if let Some(root) = session.project.as_ref().map(|project| project.root.clone()) {
        sweep_partials(&root);
        if let JobKind::Reindex { stamp, .. } = &job.kind {
            let path = root.join(crate::asset_index::INDEX_FILE);
            if stamp.is_empty() {
                let _ = fs::remove_file(&path);
            } else if fs::read(&path).ok().as_deref() != Some(stamp.as_bytes()) {
                atomic_write(&path, stamp.as_bytes())?;
            }
        }
    }
    clear_job(session, &job.id);
    session.notify(session.messages.get("import-job-cancelled"), false);
    Ok(())
}

fn clear_job(session: &mut EditorSession, id: &str) {
    session.models.long_job = None;
    session.foundation.tasks.retain(|task| task.id != id);
}

fn place_sprite(
    session: &mut EditorSession,
    root: &Path,
    plan: &PlannedFile,
    place_a: &str,
    place_b: &str,
) -> Result<(), CoreError> {
    let full = root.join(&plan.dest_rel);
    let kind = format!("sprite:{}", full.display());
    let mut pairs = vec![("kind", kind.as_str())];
    if !place_a.is_empty() && !place_b.is_empty() {
        pairs.push(("a", place_a));
        pairs.push(("b", place_b));
    }
    session.dispatch("scene.add", &CommandArgs::new(&pairs))
}

fn project_root(session: &EditorSession) -> Result<PathBuf, CoreError> {
    session
        .project
        .as_ref()
        .map(|project| project.root.clone())
        .ok_or_else(|| CoreError::new("open a project before importing"))
}

/// Reimport the files a watcher burst named. Planning uses the worker pool;
/// sidecars are written in path order on this thread so the result does not
/// depend on which worker finished first. Renames are not applied here, so a
/// watch does not keep moving the file.
pub fn reimport_changed(session: &mut EditorSession, dir: &Path, names: &[String]) {
    let Some(root) = session.project.as_ref().map(|project| project.root.clone()) else {
        return;
    };
    let book = load_book(&root);
    let mut sources = Vec::new();
    for name in names {
        // Layered sources reimport through their own importers above, not as plain files.
        if name.starts_with('.')
            || crate::aseprite::is_source_name(name)
            || crate::layered_picture::is_source_name(name)
        {
            continue;
        }
        let path = dir.join(name);
        if path.is_file() {
            if let Some(rel) = crate::asset_index::relative_to_project(&root, &path) {
                if ImportKind::from_asset_path(&rel).is_some() || known_in_folder(&book, name) {
                    sources.push(path);
                }
            }
        }
    }
    if sources.is_empty() {
        return;
    }
    editor_tools::note_worker_budget(&session.foundation.settings);
    let Ok((planned, _)) =
        plan_imports(&root, sources, editor_tools::active_workers(), &book, false, true)
    else {
        return;
    };
    for plan in planned {
        let started = Instant::now();
        let mut status = match write_plan(&root, &plan, false) {
            Ok(_) if plan.error.is_none() => "ok",
            _ => "failed",
        };
        if status == "ok"
            && plan.kind == "picture"
            && seal_render::picture::decode_path(&root.join(&plan.dest_rel)).is_err()
        {
            status = "failed";
        }
        crate::asset_index::record_import_activity_hashed(
            session,
            if plan.dest_rel.is_empty() { plan.file_name.as_str() } else { plan.dest_rel.as_str() },
            "reimport",
            status,
            started,
            &plan.hash,
        );
    }
}

pub fn graph_files(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == ".seal-editor" || name == ".git" {
                continue;
            }
            if path.is_dir() {
                pending.push(path);
            } else if name.ends_with(".graph") {
                if let Ok(rel) = path.strip_prefix(root) {
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    out.sort();
    out
}

fn set_script(book: &mut PipelineBook, kind: &str, graph: &str) {
    if let Some(row) = book.scripts.iter_mut().find(|row| row.kind == kind) {
        row.graph = graph.replace('\\', "/");
    } else {
        book.scripts.push(KindScript { kind: kind.into(), graph: graph.replace('\\', "/") });
    }
}

fn write_starter(root: &Path, kind: &str) -> Result<String, CoreError> {
    let rel = format!("logic/import-{kind}.graph");
    let path = root.join(&rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| CoreError::new(err.to_string()))?;
    }
    if !path.is_file() {
        let mut graph = Graph::new(&format!("Import {kind}"));
        graph.nodes.push(
            Node::new(1, "event").with("name", SealValue::String("import".into())),
        );
        graph.nodes.push(
            Node::new(2, "import_set_pivot")
                .with("x", SealValue::Number("0.5".into()))
                .with("y", SealValue::Number("0.5".into())),
        );
        graph.nodes.push(Node::new(3, "import_rename").with("name", SealValue::String(String::new())));
        graph.nodes.push(Node::new(4, "import_add_tag").with("tag", SealValue::String(String::new())));
        graph.links.push(Link::new(1, "then", 2, "in"));
        graph.links.push(Link::new(2, "then", 3, "in"));
        graph.links.push(Link::new(3, "then", 4, "in"));
        atomic_write(&path, graph.to_text().as_bytes())?;
    }
    Ok(rel)
}

fn edit_graph<F>(root: &Path, rel: &str, edit: F) -> Result<(), CoreError>
where
    F: FnOnce(&mut Graph) -> Result<(), CoreError>,
{
    let path = root.join(rel);
    let text = fs::read_to_string(&path).map_err(|err| CoreError::new(err.to_string()))?;
    let mut graph = Graph::from_text(&text).map_err(|err| CoreError::new(err.to_string()))?;
    edit(&mut graph)?;
    atomic_write(&path, graph.to_text().as_bytes())
}

fn set_node_param(graph: &mut Graph, kind: &str, key: &str, value: SealValue) -> Result<(), CoreError> {
    let Some(node) = graph.nodes.iter_mut().find(|node| node.kind == kind) else {
        return Err(CoreError::new(format!("the import script has no {kind} node")));
    };
    node.set(key, value);
    Ok(())
}

pub fn script_text(root: &Path, kind: &str, key: &str) -> String {
    let book = load_book(root);
    let Some(rel) = book.script_for(kind) else {
        return String::new();
    };
    let Ok(text) = fs::read_to_string(root.join(rel)) else {
        return String::new();
    };
    let Ok(graph) = Graph::from_text(&text) else {
        return String::new();
    };
    let node_kind = match key {
        "x" | "y" => "import_set_pivot",
        "name" => "import_rename",
        "tag" => "import_add_tag",
        _ => return String::new(),
    };
    graph
        .nodes
        .iter()
        .find(|node| node.kind == node_kind)
        .and_then(|node| param_str(node, key))
        .unwrap_or_default()
}

type CommandFn = fn(&mut EditorSession, &CommandArgs) -> Result<(), CoreError>;

pub const COMMANDS: &[(&str, &str, &str, CommandFn)] = &[
    ("project.open_import", "command-project-open-import", "tools", cmd_open_import),
    ("import.script.set", "command-import-script-set", "tools", cmd_script_set),
    ("import.script.create", "command-import-script-create", "tools", cmd_script_create),
    ("import.script.pivot", "command-import-script-pivot", "tools", cmd_script_pivot),
    ("import.script.rename", "command-import-script-rename", "tools", cmd_script_rename),
    ("import.script.tag", "command-import-script-tag", "tools", cmd_script_tag),
    ("import.importer.add", "command-import-importer-add", "tools", cmd_importer_add),
    ("import.importer.remove", "command-import-importer-remove", "tools", cmd_importer_remove),
    ("asset.reindex", "command-asset-reindex", "tools", cmd_reindex),
];

fn cmd_open_import(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.focus_panel("settings");
    Ok(())
}

fn cmd_script_set(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let root = project_root(session)?;
    let kind = required_kind(args)?;
    let graph = args.get("graph").unwrap_or("").trim().replace('\\', "/");
    let mut book = load_book(&root);
    set_script(&mut book, &kind, &graph);
    save_book(&root, &book)
}

fn cmd_script_create(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let root = project_root(session)?;
    let kind = required_kind(args)?;
    let rel = write_starter(&root, &kind)?;
    let mut book = load_book(&root);
    set_script(&mut book, &kind, &rel);
    save_book(&root, &book)?;
    session.notify(format!("Created {rel}"), false);
    Ok(())
}

fn cmd_script_pivot(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let root = project_root(session)?;
    let kind = required_kind(args)?;
    let x = args.get("x").unwrap_or("0.5");
    let y = args.get("y").unwrap_or("0.5");
    parse_finite(x)?;
    parse_finite(y)?;
    let rel = assigned_graph(&root, &kind)?;
    edit_graph(&root, &rel, |graph| {
        set_node_param(graph, "import_set_pivot", "x", SealValue::Number(x.into()))?;
        set_node_param(graph, "import_set_pivot", "y", SealValue::Number(y.into()))?;
        Ok(())
    })
}

fn cmd_script_rename(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let root = project_root(session)?;
    let kind = required_kind(args)?;
    let name = args.get("name").unwrap_or("").trim();
    let rel = assigned_graph(&root, &kind)?;
    edit_graph(&root, &rel, |graph| {
        set_node_param(graph, "import_rename", "name", SealValue::String(name.into()))
    })
}

fn cmd_script_tag(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let root = project_root(session)?;
    let kind = required_kind(args)?;
    let tag = args.get("tag").unwrap_or("").trim();
    let rel = assigned_graph(&root, &kind)?;
    edit_graph(&root, &rel, |graph| {
        set_node_param(graph, "import_add_tag", "tag", SealValue::String(tag.into()))
    })
}

fn cmd_importer_add(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let root = project_root(session)?;
    let extension = normalize_ext(args.get("extension").unwrap_or(""));
    if extension.is_empty() || extension.contains('.') || extension.len() > 16 {
        return Err(CoreError::new("enter a file extension"));
    }
    let graph = args.get("graph").unwrap_or("").trim().replace('\\', "/");
    if graph.is_empty() {
        return Err(CoreError::new("choose the importer graph"));
    }
    let mut book = load_book(&root);
    book.importers.retain(|row| row.extension != extension);
    book.importers.push(CustomImporter { extension, graph });
    book.importers.sort_by(|left, right| left.extension.cmp(&right.extension));
    save_book(&root, &book)
}

fn cmd_importer_remove(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let root = project_root(session)?;
    let extension = normalize_ext(args.get("extension").unwrap_or(""));
    let mut book = load_book(&root);
    book.importers.retain(|row| row.extension != extension);
    save_book(&root, &book)
}

fn cmd_reindex(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    begin_reindex(session)
}

fn required_kind(args: &CommandArgs) -> Result<String, CoreError> {
    let kind = args.get("kind").unwrap_or("").trim();
    if !KIND_IDS.contains(&kind) {
        return Err(CoreError::new("choose a picture, sound, font, effect, or file script"));
    }
    Ok(kind.to_string())
}

fn assigned_graph(root: &Path, kind: &str) -> Result<String, CoreError> {
    load_book(root)
        .script_for(kind)
        .map(str::to_string)
        .ok_or_else(|| CoreError::new("assign an import script first"))
}

fn parse_finite(text: &str) -> Result<f32, CoreError> {
    let value: f32 = text.parse().map_err(|_| CoreError::new("pivot must be a number"))?;
    if !value.is_finite() {
        return Err(CoreError::new("pivot must be a number"));
    }
    Ok(value)
}

/// Blocking import used by tests: plan on `workers` threads, then write in order.
pub fn import_blocking(
    root: &Path,
    sources: Vec<PathBuf>,
    workers: usize,
) -> Result<(Vec<PlannedFile>, usize), CoreError> {
    let book = load_book(root);
    let (planned, threads) = plan_imports(root, sources, workers, &book, true, false)?;
    for plan in &planned {
        write_plan(root, plan, false)?;
    }
    Ok((planned, threads))
}
