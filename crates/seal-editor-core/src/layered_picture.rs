//! Layered picture files (`.psd`).
//!
//! Import writes a flattened PNG beside the file. With **Layers as sprites** it
//! also writes one trimmed picture per visible layer and a prefab that places
//! each sprite. Groups become folders and parent objects. A newer source
//! updates those pictures in place and keeps the same object ids.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime};

use seal_sealtext::{parse as parse_text, SealValue};

use crate::command::CommandArgs;
use crate::error::CoreError;
use crate::session::EditorSession;

const MAX_FILE: usize = 32 * 1024 * 1024;
const MAX_EDGE: u32 = 8192;
const MAX_LAYERS: usize = 512;
const MAX_CHANNELS: usize = 56;

/// One row in the Asset Editor layer list, top of the stack first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerLine {
    pub name: String,
    pub visible: bool,
    pub group: bool,
    pub depth: u32,
}

/// A layered picture: the flattened RGBA8 image and the layer list.
#[derive(Clone, Debug)]
pub struct Source {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub layers: Vec<LayerLine>,
    pub notes: Vec<String>,
    nodes: Vec<Node>,
}

#[derive(Clone, Debug)]
enum Node {
    Sprite(SpriteLayer),
    Group(GroupLayer),
}

#[derive(Clone, Debug)]
struct SpriteLayer {
    name: String,
    id: u32,
    visible: bool,
    opacity: u8,
    blend_normal: bool,
    clipped: bool,
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

#[derive(Clone, Debug)]
struct GroupLayer {
    name: String,
    id: u32,
    visible: bool,
    opacity: u8,
    blend_normal: bool,
    children: Vec<Node>,
}

#[derive(Clone, Debug)]
pub struct ImportReport {
    pub flat_name: String,
    pub prefab_name: String,
    pub layer_count: usize,
    pub notes: Vec<String>,
}

#[derive(Default)]
struct Settings {
    layers: bool,
    source: String,
    flat: String,
    prefab: String,
}

struct Place {
    id: String,
    name: String,
    parent: String,
    x: f32,
    y: f32,
    sx: f32,
    sy: f32,
    sprite: String,
    order: i32,
}

/// Whether `name` is a `.psd` file.
pub fn is_source_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("psd"))
}

/// Flattened RGBA8 for thumbnails.
pub fn preview(path: &Path) -> Option<(Vec<u8>, u32, u32)> {
    let src = load(path).ok()?;
    Some((src.rgba, src.width, src.height))
}

pub fn load(path: &Path) -> Result<Source, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FILE {
        return Err("the file is too large".into());
    }
    parse(&bytes)
}

pub fn parse(bytes: &[u8]) -> Result<Source, String> {
    if bytes.len() > MAX_FILE {
        return Err("the file is too large".into());
    }
    if bytes.len() < 26 || &bytes[..4] != b"8BPS" {
        return Err("this is not a layered picture file".into());
    }
    let mut r = Reader { data: bytes, i: 0 };
    let file = read_file(&mut r)?;
    let mut notes = Vec::new();
    let rgba = if file.nodes.is_empty() {
        file.composite
    } else {
        let mut canvas = vec![0u8; file.rgba_len()?];
        paint(&file.nodes, true, 255, file.width, file.height, &mut canvas, &mut notes);
        canvas
    };
    let mut layers = Vec::new();
    list_layers(&file.nodes, 0, &mut layers);
    Ok(Source { width: file.width, height: file.height, rgba, layers, notes, nodes: file.nodes })
}

/// Whether Layers as sprites is saved for this file.
pub fn layers_as_sprites(path: &Path) -> bool {
    read_settings(path).layers
}

/// Save the Layers as sprites checkbox. Other saved names are kept.
pub fn set_layers_as_sprites(path: &Path, on: bool) -> Result<(), String> {
    let mut settings = read_settings(path);
    settings.layers = on;
    write_settings(path, &settings)
}

/// Rebuild generated files when this source already has them and is newer.
pub fn refresh_source(path: &Path) {
    match reimport_source(path) {
        Ok(notes) if notes.is_empty() => {
            if let Ok(src) = load(path) {
                emit_once(path, &src.notes);
            }
        }
        Ok(_) | Err(_) => {}
    }
}

/// While a layered picture is open, rebuild its generated files after it changes.
pub fn poll_open(session: &EditorSession) {
    let Some(rel) = session.draft(crate::asset_editor::ASSET_DRAFT) else {
        return;
    };
    if !is_source_name(&rel) {
        return;
    }
    let Some(root) = session.project.as_ref().map(|p| p.root.clone()) else {
        return;
    };
    let path = root.join(&rel);
    let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    let seen = open_seen().lock().unwrap_or_else(|e| e.into_inner());
    if seen.get(&path) == Some(&mtime) {
        return;
    }
    drop(seen);
    refresh_source(&path);
    if let Ok(mut seen) = open_seen().lock() {
        seen.insert(path, mtime);
    }
}

/// One layered picture the folder watcher rebuilt or failed to rebuild.
pub struct FolderReimport {
    pub name: String,
    pub result: Result<(), String>,
    pub started: Instant,
}

enum RebuildGate {
    Skip,
    Go(SystemTime),
    Broken(String),
}

fn rebuild_gate(path: &Path) -> RebuildGate {
    let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        return RebuildGate::Skip;
    };
    if !is_source_name(&name) {
        return RebuildGate::Skip;
    }
    let settings = read_settings(path);
    if settings.source != name || settings.flat.is_empty() {
        return RebuildGate::Skip;
    }
    let Some(dir) = path.parent() else {
        return RebuildGate::Skip;
    };
    let flat = dir.join(&settings.flat);
    let src_m = match std::fs::metadata(path).and_then(|m| m.modified()) {
        Ok(time) => time,
        Err(err) => return RebuildGate::Broken(err.to_string()),
    };
    let out_m =
        std::fs::metadata(&flat).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
    if src_m <= out_m || fail_seen(path, src_m) {
        RebuildGate::Skip
    } else {
        RebuildGate::Go(src_m)
    }
}

/// Rebuild generated files in this folder whose source is newer.
pub fn reimport_dir_outcomes(dir: &Path) -> Vec<FolderReimport> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_string) else {
            continue;
        };
        if !is_source_name(&name) {
            continue;
        }
        match rebuild_gate(&path) {
            RebuildGate::Skip => {}
            RebuildGate::Go(_) | RebuildGate::Broken(_) => {
                let started = Instant::now();
                let result = reimport_source(&path).map(|_| ());
                out.push(FolderReimport { name, result, started });
            }
        }
    }
    out
}

pub fn reimport_source(path: &Path) -> Result<Vec<String>, String> {
    let src_m = match rebuild_gate(path) {
        RebuildGate::Skip => return Ok(Vec::new()),
        RebuildGate::Broken(err) => return Err(err),
        RebuildGate::Go(time) => time,
    };
    let project = project_of(path)?;
    let layers = read_settings(path).layers;
    match import_path(&project, path, layers) {
        Ok(report) => {
            clear_fail(path);
            emit_once(path, &report.notes);
            Ok(report.notes)
        }
        Err(e) => {
            mark_fail(path, src_m);
            emit_once(path, std::slice::from_ref(&e));
            Err(e)
        }
    }
}

type CommandFn = fn(&mut EditorSession, &CommandArgs) -> Result<(), CoreError>;

pub const COMMANDS: &[(&str, &str, &str, CommandFn)] = &[
    ("asset.layered_picture_import", "command-asset-layered-picture-import", "file", cmd_import),
    ("asset.layered_picture_option", "command-asset-layered-picture-option", "edit", cmd_option),
];

/// `asset.layered_picture_option path= value=yes|no`: save Layers as sprites.
pub(crate) fn cmd_option(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let (_rel, full) = open_source(session, args)?;
    let on = yes(args.get("value").unwrap_or("no"));
    set_layers_as_sprites(&full, on).map_err(CoreError::new)?;
    Ok(())
}

/// `asset.layered_picture_import [path=] [layers=yes|no]`: write the flattened
/// picture. `layers` overrides the saved checkbox for this run and is saved.
pub(crate) fn cmd_import(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let (rel, full) = open_source(session, args)?;
    let root = session
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .ok_or_else(|| CoreError::new("open a project first"))?;
    let layers = match args.get("layers") {
        Some(value) => yes(value),
        None => layers_as_sprites(&full),
    };
    set_layers_as_sprites(&full, layers).map_err(CoreError::new)?;
    let started = Instant::now();
    let report = import_path(&root, &full, layers).map_err(CoreError::new)?;
    emit_once(&full, &report.notes);
    let notice = if report.prefab_name.is_empty() {
        session
            .messages
            .format("asset-layered-picture-imported", &[("flat", report.flat_name.as_str())])
    } else {
        session.messages.format(
            "asset-layered-picture-imported-layers",
            &[
                ("flat", report.flat_name.as_str()),
                ("count", &report.layer_count.to_string()),
                ("prefab", report.prefab_name.as_str()),
            ],
        )
    };
    session.notify(notice, false);
    crate::content::refresh_content_tree(session);
    crate::asset_index::record_import_activity(session, &rel, "layered picture", "ok", started);
    Ok(())
}

pub fn import_path(project: &Path, path: &Path, layers: bool) -> Result<ImportReport, String> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| "choose a file".to_string())?;
    if !is_source_name(&name) {
        return Err("choose a .psd file".into());
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| plain_name(s))
        .ok_or_else(|| "the file name cannot be used".to_string())?;
    let dir = path.parent().ok_or_else(|| "the file has no folder".to_string())?;
    let rel_dir = relative_dir(project, dir)?;
    let source = load(path)?;
    let mut settings = read_settings(path);
    let flat_name = format!("{stem}.png");
    let flat_path = dir.join(&flat_name);
    if flat_path.exists() && !(settings.source == name && settings.flat == flat_name) {
        return Err(format!("{flat_name} already exists"));
    }
    let mut notes = source.notes.clone();
    let (prefab_name, layer_count) = if layers {
        let (prefab_name, count, layer_notes) =
            write_layers(project, &rel_dir, dir, &stem, &name, &source)?;
        for note in layer_notes {
            push_note(&mut notes, &note);
        }
        (prefab_name, count)
    } else {
        (String::new(), 0)
    };
    let png = encode_png(&source.rgba, source.width, source.height)?;
    crate::persist::atomic_write(&flat_path, &png).map_err(|e| e.to_string())?;
    settings.layers = layers;
    settings.source = name;
    settings.flat = flat_name.clone();
    settings.prefab = prefab_name.clone();
    write_settings(path, &settings)?;
    Ok(ImportReport { flat_name, prefab_name, layer_count, notes })
}

fn write_layers(
    project: &Path,
    rel_dir: &str,
    dir: &Path,
    stem: &str,
    source_name: &str,
    source: &Source,
) -> Result<(String, usize, Vec<String>), String> {
    let prefab_name = format!("{stem}.prefab");
    let prefab_path = dir.join(&prefab_name);
    let mut kept: HashMap<String, String> = HashMap::new();
    if prefab_path.is_file() {
        let text = std::fs::read_to_string(&prefab_path).unwrap_or_default();
        match marker_from(&text) {
            Some(from) if from == source_name => kept = kept_sprites(&text),
            Some(from) => return Err(format!("{prefab_name} was generated from {from}")),
            None => return Err(format!("{prefab_name} already exists")),
        }
    }
    let folder_rel = join_rel(rel_dir, &format!("{stem}_layers"));
    let mut used_ids = HashSet::new();
    used_ids.insert("lyrroot".to_string());
    let mut used_names = HashSet::new();
    let mut order = 0i32;
    let mut extra_notes = Vec::new();
    let mut job = ExportJob {
        project,
        kept: &kept,
        used_ids: &mut used_ids,
        used_names: &mut used_names,
        order: &mut order,
        doc_h: source.height,
        notes: &mut extra_notes,
    };
    let mut places = export_nodes(&source.nodes, "lyrroot", true, 255, &folder_rel, &mut job)?;
    if places.is_empty() {
        extra_notes.push("No visible layer had pixels, so no sprites were written.".into());
    }
    let root = Place {
        id: "lyrroot".into(),
        name: stem.to_string(),
        parent: String::new(),
        x: source.width as f32 / 2.0,
        y: source.height as f32 / 2.0,
        sx: 1.0,
        sy: 1.0,
        sprite: String::new(),
        order: 0,
    };
    places.insert(0, root);
    let text = prefab_text(source_name, &places);
    crate::persist::atomic_write(&prefab_path, text.as_bytes()).map_err(|e| e.to_string())?;
    let count = places.iter().filter(|p| !p.sprite.is_empty()).count();
    Ok((prefab_name, count, extra_notes))
}

struct ExportJob<'a> {
    project: &'a Path,
    kept: &'a HashMap<String, String>,
    used_ids: &'a mut HashSet<String>,
    used_names: &'a mut HashSet<String>,
    order: &'a mut i32,
    doc_h: u32,
    notes: &'a mut Vec<String>,
}

fn export_nodes(
    nodes: &[Node],
    parent: &str,
    parent_vis: bool,
    parent_op: u8,
    folder_rel: &str,
    job: &mut ExportJob<'_>,
) -> Result<Vec<Place>, String> {
    let mut out = Vec::new();
    for node in nodes {
        match node {
            Node::Sprite(sprite) => {
                let draw = parent_vis && sprite.visible && sprite.opacity > 0 && parent_op > 0;
                if !draw {
                    continue;
                }
                if sprite.clipped {
                    push_note(
                        job.notes,
                        "A clipped layer was drawn in full. Clipping was not applied.",
                    );
                }
                if !sprite.blend_normal {
                    push_note(
                        job.notes,
                        "A layer uses a blend other than normal and was drawn as normal.",
                    );
                }
                let opacity = mul_op(parent_op, sprite.opacity);
                let baked = bake_opacity(&sprite.rgba, opacity);
                let Some((dx, dy, tw, th, pixels)) = trim(&baked, sprite.width, sprite.height)
                else {
                    push_note(job.notes, "A layer has no visible pixels and was skipped.");
                    continue;
                };
                let id = entity_id(job.used_ids, sprite.id);
                let sprite_rel = sprite_path(
                    job.project,
                    folder_rel,
                    &sprite.name,
                    &id,
                    job.kept,
                    job.used_names,
                )?;
                let abs = job.project.join(&sprite_rel);
                if let Some(parent_dir) = abs.parent() {
                    std::fs::create_dir_all(parent_dir).map_err(|e| e.to_string())?;
                }
                let png = encode_png(&pixels, tw, th)?;
                crate::persist::atomic_write(&abs, &png).map_err(|e| e.to_string())?;
                let left = sprite.left + dx;
                let top = sprite.top + dy;
                let x = left as f32 + tw as f32 / 2.0;
                let y = job.doc_h as f32 - (top as f32 + th as f32 / 2.0);
                let placed_order = *job.order;
                *job.order += 1;
                out.push(Place {
                    id,
                    name: sprite.name.clone(),
                    parent: parent.to_string(),
                    x,
                    y,
                    sx: tw as f32,
                    sy: th as f32,
                    sprite: sprite_rel,
                    order: placed_order,
                });
            }
            Node::Group(group) => {
                let draw = parent_vis && group.visible && group.opacity > 0 && parent_op > 0;
                if !group.blend_normal && draw {
                    push_note(
                        job.notes,
                        "A layer uses a blend other than normal and was drawn as normal.",
                    );
                }
                let id = entity_id(job.used_ids, group.id);
                let child_rel = join_rel(folder_rel, &file_token(&group.name));
                let children = export_nodes(
                    &group.children,
                    &id,
                    draw,
                    mul_op(parent_op, group.opacity),
                    &child_rel,
                    job,
                )?;
                if children.is_empty() {
                    continue;
                }
                let (x, y) = sprite_bounds_center(&children).unwrap_or((0.0, 0.0));
                out.push(Place {
                    id,
                    name: group.name.clone(),
                    parent: parent.to_string(),
                    x,
                    y,
                    sx: 1.0,
                    sy: 1.0,
                    sprite: String::new(),
                    order: 0,
                });
                out.extend(children);
            }
        }
    }
    Ok(out)
}

fn sprite_path(
    project: &Path,
    folder_rel: &str,
    layer_name: &str,
    id: &str,
    kept: &HashMap<String, String>,
    used_names: &mut HashSet<String>,
) -> Result<String, String> {
    if let Some(existing) = kept.get(id) {
        if safe_rel(existing) {
            used_names.insert(existing.clone());
            return Ok(existing.clone());
        }
    }
    let base = file_token(layer_name);
    let mut name = format!("{base}.png");
    let mut n = 2u32;
    loop {
        let rel = join_rel(folder_rel, &name);
        if !used_names.contains(&rel) && !project.join(&rel).exists() {
            used_names.insert(rel.clone());
            return Ok(rel);
        }
        name = format!("{base}-{n}.png");
        n += 1;
        if n > 10_000 {
            return Err("too many layers share a name".into());
        }
    }
}

fn sprite_bounds_center(places: &[Place]) -> Option<(f32, f32)> {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    let mut any = false;
    for place in places {
        if place.sprite.is_empty() {
            continue;
        }
        any = true;
        let hx = place.sx.abs() / 2.0;
        let hy = place.sy.abs() / 2.0;
        min_x = min_x.min(place.x - hx);
        max_x = max_x.max(place.x + hx);
        min_y = min_y.min(place.y - hy);
        max_y = max_y.max(place.y + hy);
    }
    any.then_some(((min_x + max_x) / 2.0, (min_y + max_y) / 2.0))
}

fn prefab_text(source_name: &str, places: &[Place]) -> String {
    let rows = places.iter().map(place_value).collect();
    SealValue::Map(
        vec![
            ("seal_prefab".into(), SealValue::Number("1".into())),
            ("generated_from".into(), SealValue::String(source_name.to_string())),
            ("root".into(), SealValue::String("lyrroot".into())),
            ("entities".into(), SealValue::Array(rows)),
        ]
        .into(),
    )
    .to_text()
}

fn place_value(place: &Place) -> SealValue {
    let mut entries = vec![
        ("id".into(), SealValue::String(place.id.clone())),
        ("name".into(), SealValue::String(place.name.clone())),
        (
            "parent".into(),
            if place.parent.is_empty() {
                SealValue::Null
            } else {
                SealValue::String(place.parent.clone())
            },
        ),
        ("x".into(), num(place.x)),
        ("y".into(), num(place.y)),
        ("z".into(), num(0.0)),
        ("sx".into(), num(place.sx)),
        ("sy".into(), num(place.sy)),
        ("sz".into(), num(1.0)),
    ];
    if !place.sprite.is_empty() {
        entries.push(("sprite".into(), SealValue::String(place.sprite.clone())));
    }
    if place.order != 0 {
        entries.push(("order".into(), num(place.order as f32)));
    }
    SealValue::Map(entries.into())
}

fn marker_from(text: &str) -> Option<String> {
    let value = parse_text(text).ok()?;
    match value.get("generated_from") {
        Some(SealValue::String(name)) if plain_name(name) => Some(name.clone()),
        _ => None,
    }
}

fn kept_sprites(text: &str) -> HashMap<String, String> {
    let Ok(value) = parse_text(text) else {
        return HashMap::new();
    };
    let Some(SealValue::Array(rows)) = value.get("entities") else {
        return HashMap::new();
    };
    let mut out = HashMap::new();
    for row in rows {
        let Some(map) = row.as_map() else { continue };
        let Some(SealValue::String(id)) = field(map, "id") else { continue };
        let Some(SealValue::String(sprite)) = field(map, "sprite") else { continue };
        if safe_rel(sprite) {
            out.insert(id.clone(), sprite.clone());
        }
    }
    out
}

fn field<'a>(map: &'a [(String, SealValue)], key: &str) -> Option<&'a SealValue> {
    map.iter().find(|(name, _)| name == key).map(|(_, value)| value)
}

fn read_settings(path: &Path) -> Settings {
    let Ok(text) = std::fs::read_to_string(sidecar(path)) else {
        return Settings::default();
    };
    let Ok(value) = parse_text(&text) else {
        return Settings::default();
    };
    if value.get("seal_layered_picture").is_none() {
        return Settings::default();
    }
    Settings {
        layers: matches!(value.get("layers_as_sprites"), Some(SealValue::Bool(true))),
        source: string_field(&value, "source"),
        flat: string_field(&value, "flat"),
        prefab: string_field(&value, "prefab"),
    }
}

fn write_settings(path: &Path, settings: &Settings) -> Result<(), String> {
    let mut entries = vec![
        ("seal_layered_picture".into(), SealValue::Number("1".into())),
        ("layers_as_sprites".into(), SealValue::Bool(settings.layers)),
    ];
    if !settings.source.is_empty() {
        entries.push(("source".into(), SealValue::String(settings.source.clone())));
    }
    if !settings.flat.is_empty() {
        entries.push(("flat".into(), SealValue::String(settings.flat.clone())));
    }
    if !settings.prefab.is_empty() {
        entries.push(("prefab".into(), SealValue::String(settings.prefab.clone())));
    }
    let text = SealValue::Map(entries.into()).to_text();
    crate::persist::atomic_write(&sidecar(path), text.as_bytes()).map_err(|e| e.to_string())
}

fn sidecar(path: &Path) -> PathBuf {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    path.parent().unwrap_or_else(|| Path::new(".")).join(format!(".{name}.import"))
}

fn string_field(value: &SealValue, key: &str) -> String {
    match value.get(key) {
        Some(SealValue::String(text)) => text.clone(),
        _ => String::new(),
    }
}

fn open_source(
    session: &EditorSession,
    args: &CommandArgs,
) -> Result<(String, PathBuf), CoreError> {
    let rel = match args.get("path").map(str::trim).filter(|p| !p.is_empty()) {
        Some(path) => checked_rel(path)?,
        None => session
            .draft(crate::asset_editor::ASSET_DRAFT)
            .filter(|r| !r.is_empty())
            .ok_or_else(|| CoreError::new("open a layered picture in the Asset Editor"))?,
    };
    if !is_source_name(&rel) {
        return Err(CoreError::new("choose a .psd file"));
    }
    let root = session
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .ok_or_else(|| CoreError::new("open a project first"))?;
    let full = root.join(&rel);
    if !full.is_file() {
        return Err(CoreError::new(format!("{rel} was not found")));
    }
    Ok((rel, full))
}

fn project_of(path: &Path) -> Result<PathBuf, String> {
    let mut dir = path.parent().ok_or_else(|| "the file has no folder".to_string())?;
    loop {
        if dir.join("project.seal").is_file() {
            return Ok(dir.to_path_buf());
        }
        let Some(parent) = dir.parent() else {
            return Err("open a project first".into());
        };
        if parent == dir {
            return Err("open a project first".into());
        }
        dir = parent;
    }
}

fn checked_rel(rel: &str) -> Result<String, CoreError> {
    let rel = rel.trim().trim_start_matches("./").replace('\\', "/");
    if rel.is_empty() || rel.starts_with('/') || rel.split('/').any(|p| p == "..") {
        return Err(CoreError::new(format!("'{rel}' is not a path inside the project")));
    }
    Ok(rel)
}

fn yes(value: &str) -> bool {
    matches!(value.trim(), "yes" | "1" | "true")
}

fn plain_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0'])
}

fn safe_rel(rel: &str) -> bool {
    !rel.is_empty()
        && !rel.starts_with('/')
        && !rel.starts_with('\\')
        && !rel.split(['/', '\\']).any(|p| p.is_empty() || p == "." || p == "..")
}

fn relative_dir(project: &Path, dir: &Path) -> Result<String, String> {
    if dir == project {
        return Ok(String::new());
    }
    let rel =
        dir.strip_prefix(project).map_err(|_| "the file is outside the project".to_string())?;
    let text = rel.to_string_lossy().replace('\\', "/");
    if !safe_rel(&text) && !text.is_empty() {
        return Err("the file is outside the project".into());
    }
    Ok(text)
}

fn join_rel(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

fn file_token(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else if ch.is_whitespace() && !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "layer".into()
    } else {
        out
    }
}

fn entity_id(used: &mut HashSet<String>, raw: u32) -> String {
    let mut id = format!("lyr{raw}");
    while !used.insert(id.clone()) {
        id.push('x');
    }
    id
}

fn num(value: f32) -> SealValue {
    let text = format!("{value:.4}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    let trimmed = if trimmed.is_empty() || trimmed == "-" { "0" } else { trimmed };
    SealValue::Number(trimmed.into())
}

fn push_note(notes: &mut Vec<String>, text: &str) {
    if !notes.iter().any(|note| note == text) {
        notes.push(text.to_string());
    }
}

fn paint(
    nodes: &[Node],
    parent_vis: bool,
    parent_op: u8,
    width: u32,
    height: u32,
    canvas: &mut [u8],
    notes: &mut Vec<String>,
) {
    for node in nodes {
        match node {
            Node::Sprite(sprite) => {
                let draw = parent_vis && sprite.visible && sprite.opacity > 0 && parent_op > 0;
                if !draw {
                    continue;
                }
                if sprite.clipped {
                    push_note(
                        notes,
                        "A clipped layer was drawn in full. Clipping was not applied.",
                    );
                }
                if !sprite.blend_normal {
                    push_note(
                        notes,
                        "A layer uses a blend other than normal and was drawn as normal.",
                    );
                }
                blit(
                    canvas,
                    (width, height),
                    (sprite.left, sprite.top),
                    &sprite.rgba,
                    (sprite.width, sprite.height),
                    mul_op(parent_op, sprite.opacity),
                );
            }
            Node::Group(group) => {
                let draw = parent_vis && group.visible && group.opacity > 0 && parent_op > 0;
                if !group.blend_normal && draw {
                    push_note(
                        notes,
                        "A layer uses a blend other than normal and was drawn as normal.",
                    );
                }
                paint(
                    &group.children,
                    draw,
                    mul_op(parent_op, group.opacity),
                    width,
                    height,
                    canvas,
                    notes,
                );
            }
        }
    }
}

fn list_layers(nodes: &[Node], depth: u32, out: &mut Vec<LayerLine>) {
    for node in nodes.iter().rev() {
        match node {
            Node::Group(group) => {
                out.push(LayerLine {
                    name: group.name.clone(),
                    visible: group.visible,
                    group: true,
                    depth,
                });
                list_layers(&group.children, depth + 1, out);
            }
            Node::Sprite(sprite) => out.push(LayerLine {
                name: sprite.name.clone(),
                visible: sprite.visible,
                group: false,
                depth,
            }),
        }
    }
}

fn blit(
    canvas: &mut [u8],
    size: (u32, u32),
    at: (i32, i32),
    src: &[u8],
    src_size: (u32, u32),
    opacity: u8,
) {
    let (width, height) = size;
    let (left, top) = at;
    let (sw, sh) = src_size;
    if sw == 0 || sh == 0 || opacity == 0 {
        return;
    }
    for sy in 0..sh {
        let y = top + sy as i32;
        if y < 0 || y as u32 >= height {
            continue;
        }
        for sx in 0..sw {
            let x = left + sx as i32;
            if x < 0 || x as u32 >= width {
                continue;
            }
            let si = ((sy * sw + sx) * 4) as usize;
            if si + 4 > src.len() {
                continue;
            }
            let mut pixel = [src[si], src[si + 1], src[si + 2], src[si + 3]];
            if opacity != 255 {
                pixel[3] = ((u16::from(pixel[3]) * u16::from(opacity)) / 255) as u8;
            }
            let di = ((y as u32 * width + x as u32) * 4) as usize;
            if di + 4 <= canvas.len() {
                over(&mut canvas[di..di + 4], pixel);
            }
        }
    }
}

fn over(dst: &mut [u8], src: [u8; 4]) {
    let sa = u32::from(src[3]);
    if sa == 0 {
        return;
    }
    if sa == 255 {
        dst[..4].copy_from_slice(&src);
        return;
    }
    let da = u32::from(dst[3]);
    let inv = 255 - sa;
    let out_a = sa + da * inv / 255;
    if out_a == 0 {
        dst[..4].fill(0);
        return;
    }
    for c in 0..3 {
        let s = u32::from(src[c]) * sa;
        let d = u32::from(dst[c]) * da * inv / 255;
        dst[c] = ((s + d) / out_a).min(255) as u8;
    }
    dst[3] = out_a.min(255) as u8;
}

fn bake_opacity(rgba: &[u8], opacity: u8) -> Vec<u8> {
    if opacity == 255 {
        return rgba.to_vec();
    }
    let mut out = rgba.to_vec();
    for pixel in out.chunks_mut(4) {
        if pixel.len() == 4 {
            pixel[3] = ((u16::from(pixel[3]) * u16::from(opacity)) / 255) as u8;
        }
    }
    out
}

fn mul_op(a: u8, b: u8) -> u8 {
    ((u16::from(a) * u16::from(b)) / 255) as u8
}

fn trim(rgba: &[u8], width: u32, height: u32) -> Option<(i32, i32, u32, u32, Vec<u8>)> {
    if width == 0 || height == 0 {
        return None;
    }
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    for y in 0..height {
        for x in 0..width {
            let i = ((y * width + x) * 4 + 3) as usize;
            if rgba.get(i).copied().unwrap_or(0) > 0 {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x + 1);
                max_y = max_y.max(y + 1);
            }
        }
    }
    if max_x <= min_x || max_y <= min_y {
        return None;
    }
    let tw = max_x - min_x;
    let th = max_y - min_y;
    let mut pixels = vec![0u8; (tw * th * 4) as usize];
    for y in 0..th {
        for x in 0..tw {
            let si = (((min_y + y) * width + (min_x + x)) * 4) as usize;
            let di = ((y * tw + x) * 4) as usize;
            if si + 4 <= rgba.len() && di + 4 <= pixels.len() {
                pixels[di..di + 4].copy_from_slice(&rgba[si..si + 4]);
            }
        }
    }
    Some((min_x as i32, min_y as i32, tw, th, pixels))
}

fn encode_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let need = (width as usize).checked_mul(height as usize).and_then(|n| n.checked_mul(4));
    if need != Some(rgba.len()) || width == 0 || height == 0 {
        return Err("the picture size is not usable".into());
    }
    let mut buf = Vec::new();
    let mut encoder = png::Encoder::new(&mut buf, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    drop(writer);
    Ok(buf)
}

struct FileData {
    width: u32,
    height: u32,
    composite: Vec<u8>,
    nodes: Vec<Node>,
}

impl FileData {
    fn rgba_len(&self) -> Result<usize, String> {
        (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(|| "the picture size is not usable".to_string())
    }
}

struct RawLayer {
    name: String,
    id: u32,
    visible: bool,
    opacity: u8,
    blend_normal: bool,
    clipped: bool,
    section: u32,
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    channels: Vec<(i16, u32)>,
    rgba: Vec<u8>,
}

fn read_file(r: &mut Reader<'_>) -> Result<FileData, String> {
    let sig = r.bytes(4)?;
    if sig != b"8BPS" {
        return Err("this is not a layered picture file".into());
    }
    let version = r.u16()?;
    if version != 1 {
        return Err("only a standard layered picture can be imported".into());
    }
    r.skip(6)?;
    let channels = r.u16()? as usize;
    let height = r.u32()?;
    let width = r.u32()?;
    let depth = r.u16()?;
    let mode = r.u16()?;
    if depth != 8 || mode != 3 || !(3..=MAX_CHANNELS).contains(&channels) {
        return Err(format!(
            "only 8-bit RGB pictures can be imported (this file is {}, {depth}-bit)",
            mode_name(mode)
        ));
    }
    if width == 0 || height == 0 || width > MAX_EDGE || height > MAX_EDGE {
        return Err("the picture size is not usable".into());
    }
    let area = (width as usize)
        .checked_mul(height as usize)
        .filter(|n| *n <= MAX_EDGE as usize * MAX_EDGE as usize)
        .ok_or_else(|| "the picture size is not usable".to_string())?;
    let color_len = r.u32()? as usize;
    r.skip(color_len)?;
    let resource_len = r.u32()? as usize;
    r.skip(resource_len)?;
    let section_len = r.u32()? as usize;
    if r.rest() < section_len {
        return Err("the file ended early".into());
    }
    let section_end = r.i + section_len;
    let mut raw_layers = Vec::new();
    if section_len > 0 {
        let info_len = r.u32()? as usize;
        let info_end = r.i + info_len;
        if info_end > section_end {
            return Err("the layer list is damaged".into());
        }
        if info_len > 0 {
            raw_layers = read_layers(r, info_end)?;
            if r.i > info_end {
                return Err("the layer list is damaged".into());
            }
            r.i = info_end;
        }
        r.i = section_end;
    }
    let composite = read_composite(r, channels, width, height, area)?;
    let nodes = fold_groups(raw_layers)?;
    Ok(FileData { width, height, composite, nodes })
}

fn read_layers(r: &mut Reader<'_>, info_end: usize) -> Result<Vec<RawLayer>, String> {
    let count_raw = r.i16()?;
    let count = count_raw.unsigned_abs() as usize;
    if count > MAX_LAYERS {
        return Err("the file has too many layers".into());
    }
    let mut layers = Vec::with_capacity(count);
    for index in 0..count {
        if r.i > info_end {
            return Err("the layer list ended early".into());
        }
        layers.push(read_layer_record(r, index)?);
    }
    for layer in &mut layers {
        let mut planes: Vec<(i16, Vec<u8>)> = Vec::new();
        for &(id, length) in &layer.channels {
            let bytes = r.bytes(length as usize)?.to_vec();
            if layer.width == 0 || layer.height == 0 {
                continue;
            }
            let plane = decode_channel(&bytes, layer.width, layer.height)?;
            planes.push((id, plane));
        }
        if layer.width > 0 && layer.height > 0 {
            layer.rgba = pack_rgba(&planes, layer.width, layer.height)?;
        }
    }
    Ok(layers)
}

fn read_layer_record(r: &mut Reader<'_>, index: usize) -> Result<RawLayer, String> {
    let top = r.i32()?;
    let left = r.i32()?;
    let bottom = r.i32()?;
    let right = r.i32()?;
    if bottom < top || right < left {
        return Err("a layer has a damaged rectangle".into());
    }
    let height = (bottom - top) as u32;
    let width = (right - left) as u32;
    if width > MAX_EDGE || height > MAX_EDGE {
        return Err("a layer is too large".into());
    }
    let channel_count = r.u16()? as usize;
    if channel_count > MAX_CHANNELS {
        return Err("a layer has too many channels".into());
    }
    let mut channels = Vec::with_capacity(channel_count);
    for _ in 0..channel_count {
        let id = r.i16()?;
        let length = r.u32()?;
        if length as usize > MAX_FILE {
            return Err("a channel is too large".into());
        }
        channels.push((id, length));
    }
    let sig = r.bytes(4)?;
    if sig != b"8BIM" {
        return Err("a layer is damaged".into());
    }
    let blend = r.bytes(4)?;
    let blend_normal = blend == b"norm" || blend == b"pass";
    let opacity = r.u8()?;
    let clipped = r.u8()? != 0;
    let flags = r.u8()?;
    let _filler = r.u8()?;
    let visible = flags & 0x02 == 0;
    let extra_len = r.u32()? as usize;
    let extra = r.bytes(extra_len)?.to_vec();
    let (name, id, section) = read_extra(&extra, index)?;
    Ok(RawLayer {
        name,
        id,
        visible,
        opacity,
        blend_normal,
        clipped,
        section,
        left,
        top,
        width,
        height,
        channels,
        rgba: Vec::new(),
    })
}

fn read_extra(extra: &[u8], index: usize) -> Result<(String, u32, u32), String> {
    let mut r = Reader { data: extra, i: 0 };
    if r.rest() < 8 {
        return Ok((format!("Layer {}", index + 1), (index as u32) + 1, 0));
    }
    let mask_len = r.u32()? as usize;
    r.skip(mask_len)?;
    let range_len = r.u32()? as usize;
    r.skip(range_len)?;
    let mut name = r.pascal4()?;
    let mut id = (index as u32) + 1;
    let mut section = 0u32;
    let mut unicode = None;
    while r.rest() >= 12 {
        let sig = r.bytes(4)?;
        if sig != b"8BIM" && sig != b"8B64" {
            break;
        }
        let key = r.bytes(4)?.to_vec();
        let len = r.u32()? as usize;
        if len > r.rest() {
            return Err("a layer is damaged".into());
        }
        let data = r.bytes(len)?.to_vec();
        let align = block_align(&key);
        let padded = (len + align - 1) & !(align - 1);
        if padded > len {
            r.skip(padded - len)?;
        }
        let key = String::from_utf8_lossy(&key);
        if (key == "lsct" || key == "lsdk") && data.len() >= 4 {
            section = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        } else if key == "lyid" && data.len() >= 4 {
            id = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        } else if key == "luni" {
            unicode = decode_utf16be(&data);
        }
    }
    if let Some(text) = unicode.filter(|text| !text.is_empty()) {
        name = text;
    }
    if name.is_empty() {
        name = format!("Layer {}", index + 1);
    }
    Ok((name, id, section))
}

fn block_align(key: &[u8]) -> usize {
    if matches!(
        key,
        b"LMsk"
            | b"Lr16"
            | b"Lr32"
            | b"Layr"
            | b"Mt16"
            | b"Mt32"
            | b"Mtrn"
            | b"Alph"
            | b"FMsk"
            | b"lnk2"
            | b"FEid"
            | b"FXid"
            | b"PxSD"
    ) {
        4
    } else {
        2
    }
}

fn decode_utf16be(data: &[u8]) -> Option<String> {
    if data.len() < 4 {
        return None;
    }
    let count = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let bytes = data.get(4..4 + count * 2)?;
    let (chunks, _) = bytes.as_chunks::<2>();
    let units: Vec<u16> = chunks.iter().copied().map(u16::from_be_bytes).collect();
    String::from_utf16(&units).ok()
}

fn decode_channel(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    if data.len() < 2 {
        return Err("a channel ended early".into());
    }
    let compression = u16::from_be_bytes([data[0], data[1]]);
    let body = &data[2..];
    match compression {
        0 => {
            let need =
                (width as usize).checked_mul(height as usize).ok_or("a layer is too large")?;
            if body.len() < need {
                return Err("a channel ended early".into());
            }
            Ok(body[..need].to_vec())
        }
        1 => decode_rle(body, width, height),
        2 | 3 => decode_zip(body, width, height, compression == 3),
        _ => Err("this file uses an unknown channel compression".into()),
    }
}

/// ZIP channel data (compression 2) or ZIP with prediction (3: each byte stores the
/// difference to the byte on its left in the same row).
fn decode_zip(data: &[u8], width: u32, height: u32, predicted: bool) -> Result<Vec<u8>, String> {
    let need = (width as usize).checked_mul(height as usize).ok_or("a layer is too large")?;
    let mut out = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(data, need)
        .map_err(|_| "a ZIP-compressed channel could not be read".to_string())?;
    if out.len() < need {
        return Err("a channel ended early".into());
    }
    out.truncate(need);
    if predicted && width > 0 {
        for row in out.chunks_mut(width as usize) {
            for i in 1..row.len() {
                row[i] = row[i].wrapping_add(row[i - 1]);
            }
        }
    }
    Ok(out)
}

fn decode_rle(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let rows = height as usize;
    let row_bytes = width as usize;
    if data.len() < rows * 2 {
        return Err("a channel ended early".into());
    }
    let mut counts = Vec::with_capacity(rows);
    for row in 0..rows {
        let n = u16::from_be_bytes([data[row * 2], data[row * 2 + 1]]) as usize;
        counts.push(n);
    }
    let mut p = rows * 2;
    let mut out = Vec::with_capacity(rows * row_bytes);
    for count in counts {
        if p + count > data.len() {
            return Err("a channel ended early".into());
        }
        out.extend(unpack_row(&data[p..p + count], row_bytes)?);
        p += count;
    }
    Ok(out)
}

fn unpack_row(src: &[u8], width: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(width);
    let mut i = 0;
    while out.len() < width {
        if i >= src.len() {
            return Err("a channel ended early".into());
        }
        let n = src[i];
        i += 1;
        if n == 128 {
            continue;
        } else if n < 128 {
            let count = n as usize + 1;
            if out.len() + count > width || i + count > src.len() {
                return Err("a channel is the wrong size".into());
            }
            out.extend_from_slice(&src[i..i + count]);
            i += count;
        } else {
            let count = 257 - n as usize;
            if out.len() + count > width || i >= src.len() {
                return Err("a channel is the wrong size".into());
            }
            out.extend(std::iter::repeat_n(src[i], count));
            i += 1;
        }
    }
    Ok(out)
}

fn pack_rgba(planes: &[(i16, Vec<u8>)], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let pixels = (width as usize) * (height as usize);
    let mut rgba = vec![0u8; pixels * 4];
    for pixel in rgba.chunks_mut(4) {
        pixel[3] = 255;
    }
    let mut saw = [false; 3];
    for (id, plane) in planes {
        if plane.len() < pixels {
            return Err("a channel is the wrong size".into());
        }
        let slot = match *id {
            0 => 0,
            1 => 1,
            2 => 2,
            -1 => 3,
            _ => continue,
        };
        if slot < 3 {
            saw[slot] = true;
        }
        for i in 0..pixels {
            rgba[i * 4 + slot] = plane[i];
        }
    }
    if saw != [true, true, true] {
        return Err("a layer is missing a colour channel".into());
    }
    Ok(rgba)
}

fn read_composite(
    r: &mut Reader<'_>,
    channels: usize,
    width: u32,
    height: u32,
    area: usize,
) -> Result<Vec<u8>, String> {
    let compression = r.u16()?;
    let mut planes = Vec::with_capacity(channels);
    match compression {
        0 => {
            for _ in 0..channels {
                let plane = r.bytes(area)?.to_vec();
                planes.push(plane);
            }
        }
        1 => {
            let counts_len = height as usize * channels * 2;
            let counts = r.bytes(counts_len)?.to_vec();
            for channel in 0..channels {
                let mut packed = Vec::new();
                for row in 0..height as usize {
                    let at = (channel * height as usize + row) * 2;
                    let n = u16::from_be_bytes([counts[at], counts[at + 1]]) as usize;
                    packed.extend_from_slice(r.bytes(n)?);
                }
                // Row counts sit in front of the packed bytes, matching a layer channel.
                let mut body = Vec::with_capacity(height as usize * 2 + packed.len());
                for row in 0..height as usize {
                    let at = (channel * height as usize + row) * 2;
                    body.extend_from_slice(&counts[at..at + 2]);
                }
                body.extend(packed);
                planes.push(decode_rle(&body, width, height)?);
            }
        }
        2 | 3 => {
            // One zlib stream holds every channel plane, one after another.
            let rest = r.bytes(r.rest())?.to_vec();
            let all = decode_zip(&rest, width, height * channels as u32, compression == 3)?;
            for plane in all.chunks(area).take(channels) {
                planes.push(plane.to_vec());
            }
        }
        _ => return Err("this file uses an unknown image data compression".into()),
    }
    let mut rgba = vec![0u8; area * 4];
    for pixel in rgba.chunks_mut(4) {
        pixel[3] = 255;
    }
    for (slot, plane) in planes.iter().take(4).enumerate() {
        for i in 0..area {
            rgba[i * 4 + slot] = plane[i];
        }
    }
    Ok(rgba)
}

fn fold_groups(layers: Vec<RawLayer>) -> Result<Vec<Node>, String> {
    let mut stack: Vec<Vec<Node>> = vec![Vec::new()];
    for layer in layers {
        match layer.section {
            3 => stack.push(Vec::new()),
            1 | 2 => {
                let children = stack.pop().ok_or_else(|| "a layer group is damaged".to_string())?;
                if stack.is_empty() {
                    return Err("a layer group is damaged".into());
                }
                let group = Node::Group(GroupLayer {
                    name: layer.name,
                    id: layer.id,
                    visible: layer.visible,
                    opacity: layer.opacity,
                    blend_normal: layer.blend_normal,
                    children,
                });
                stack.last_mut().ok_or_else(|| "a layer group is damaged".to_string())?.push(group);
            }
            _ => {
                let sprite = Node::Sprite(SpriteLayer {
                    name: layer.name,
                    id: layer.id,
                    visible: layer.visible,
                    opacity: layer.opacity,
                    blend_normal: layer.blend_normal,
                    clipped: layer.clipped,
                    left: layer.left,
                    top: layer.top,
                    width: layer.width,
                    height: layer.height,
                    rgba: layer.rgba,
                });
                stack
                    .last_mut()
                    .ok_or_else(|| "a layer group is damaged".to_string())?
                    .push(sprite);
            }
        }
    }
    if stack.len() != 1 {
        return Err("a layer group is not closed".into());
    }
    Ok(stack.pop().unwrap_or_default())
}

fn mode_name(mode: u16) -> &'static str {
    match mode {
        0 => "bitmap",
        1 => "grayscale",
        2 => "indexed colour",
        3 => "RGB",
        4 => "CMYK",
        7 => "multichannel",
        8 => "duotone",
        9 => "lab",
        _ => "an unsupported colour mode",
    }
}

struct Reader<'a> {
    data: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn rest(&self) -> usize {
        self.data.len().saturating_sub(self.i)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.bytes(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        let b = self.bytes(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, String> {
        let b = self.bytes(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn i16(&mut self) -> Result<i16, String> {
        Ok(self.u16()? as i16)
    }

    fn i32(&mut self) -> Result<i32, String> {
        Ok(self.u32()? as i32)
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8], String> {
        if n > MAX_FILE || self.rest() < n {
            return Err(if n > MAX_FILE {
                "the file is too large".into()
            } else {
                "the file ended early".into()
            });
        }
        let start = self.i;
        self.i += n;
        Ok(&self.data[start..self.i])
    }

    fn skip(&mut self, n: usize) -> Result<(), String> {
        self.bytes(n).map(|_| ())
    }

    fn pascal4(&mut self) -> Result<String, String> {
        let n = self.u8()? as usize;
        let bytes = self.bytes(n)?.to_vec();
        let pad = (4 - ((1 + n) % 4)) % 4;
        self.skip(pad)?;
        Ok(latin1(&bytes))
    }
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|b| char::from(*b)).collect()
}

fn emit_once(path: &Path, notes: &[String]) {
    if notes.is_empty() {
        return;
    }
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let key = format!("{:?}|{:?}|{}", path, mtime, notes.join("\n"));
    let mut seen = note_seen().lock().unwrap_or_else(|e| e.into_inner());
    if seen.len() > 512 {
        seen.clear();
    }
    if !seen.insert(key) {
        return;
    }
    drop(seen);
    for note in notes {
        seal_log::warn!("{note}");
    }
}

fn note_seen() -> &'static Mutex<HashSet<String>> {
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    SEEN.get_or_init(|| Mutex::new(HashSet::new()))
}

fn open_seen() -> &'static Mutex<HashMap<PathBuf, Option<SystemTime>>> {
    static SEEN: OnceLock<Mutex<HashMap<PathBuf, Option<SystemTime>>>> = OnceLock::new();
    SEEN.get_or_init(|| Mutex::new(HashMap::new()))
}

fn fails() -> &'static Mutex<HashMap<PathBuf, SystemTime>> {
    static FAILS: OnceLock<Mutex<HashMap<PathBuf, SystemTime>>> = OnceLock::new();
    FAILS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn fail_seen(path: &Path, mtime: SystemTime) -> bool {
    fails().lock().ok().is_some_and(|map| map.get(path) == Some(&mtime))
}

fn mark_fail(path: &Path, mtime: SystemTime) {
    if let Ok(mut map) = fails().lock() {
        map.insert(path.to_path_buf(), mtime);
    }
}

fn clear_fail(path: &Path) {
    if let Ok(mut map) = fails().lock() {
        map.remove(path);
    }
}

#[cfg(test)]
mod zip_channel_tests {
    use super::decode_channel;

    fn channel(compression: u16, body: &[u8]) -> Vec<u8> {
        let mut out = compression.to_be_bytes().to_vec();
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn zip_and_zip_with_prediction_channels_decode() {
        let pixels: Vec<u8> = vec![10, 20, 30, 40, 5, 6, 7, 8];
        let zip = miniz_oxide::deflate::compress_to_vec_zlib(&pixels, 6);
        assert_eq!(decode_channel(&channel(2, &zip), 4, 2).unwrap(), pixels);
        // Prediction stores each byte as the difference to its left neighbour in the row.
        let deltas: Vec<u8> = pixels
            .chunks(4)
            .flat_map(|row| {
                let mut prev = 0u8;
                row.iter()
                    .enumerate()
                    .map(move |(i, &v)| {
                        let d = if i == 0 { v } else { v.wrapping_sub(prev) };
                        prev = v;
                        d
                    })
                    .collect::<Vec<u8>>()
            })
            .collect();
        let zip = miniz_oxide::deflate::compress_to_vec_zlib(&deltas, 6);
        assert_eq!(decode_channel(&channel(3, &zip), 4, 2).unwrap(), pixels);
        assert!(decode_channel(&channel(2, b"not zlib"), 4, 2).is_err());
        assert!(decode_channel(&channel(9, &[0; 8]), 4, 2).is_err());
    }
}
