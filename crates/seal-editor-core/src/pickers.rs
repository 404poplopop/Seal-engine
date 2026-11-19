//! Inspector pickers and universal search.
//!
//! Asset and object references, colours, map and struct values, and the
//! command-palette index live here so the panels only paint them.

use std::path::{Path, PathBuf};

use seal_graph::{Graph, Ty, Value, VarDef};
use seal_sealtext::{SealMap, SealValue};

use crate::command::{CommandArgs, PaletteHit};
use crate::error::CoreError;
use crate::models::{MockNode, Past};
use crate::session::EditorSession;

const SEARCH_SEP: char = '\u{1e}';
const EYE_DRAFT: &str = "picker-eyedropper";
const SAVED_KEY: &str = "picker.saved_searches";
const RECENT_KEY: &str = "picker.recent_colours";
const RECENT_LIMIT: usize = 8;

/// Project file the asset picker can offer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetCandidate {
    pub path: String,
    pub name: String,
    pub kind: String,
}

/// Scene object the object picker can offer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectCandidate {
    pub id: String,
    pub name: String,
    pub shape: String,
    pub role: String,
    pub type_name: String,
    pub tags: Vec<String>,
}

/// Stored path for an asset field: trimmed, with `/` separators.
pub fn asset_reference(path: &str) -> String {
    path.trim().replace('\\', "/")
}

/// Stored id for an object field.
pub fn object_reference(id: &str) -> String {
    id.trim().to_string()
}

/// Fields edited with the asset picker.
pub fn asset_field(field: &str) -> bool {
    asset_matches_field(field, "file.png")
        || matches!(
            field,
            "script"
                | "particles"
                | "mat_shader"
                | "font"
                | "theme"
                | "sound"
                | "music"
                | "mesh_texture"
                | "sky_cubemap"
        )
}

/// True when `path` is a file the asset field accepts.
pub fn asset_matches_field(field: &str, path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    let ext = ext.rsplit('/').next().unwrap_or(ext);
    match field {
        "sprite" | "normal_map" => {
            matches!(ext, "png" | "jpg" | "jpeg" | "bmp" | "gif" | "frames" | "ase")
        }
        "script" => ext == "graph",
        "particles" => ext == "fx",
        "mat_shader" => ext == "shade2d",
        "font" => matches!(ext, "ttf" | "otf"),
        "theme" => ext == "sealtheme",
        "sound" | "music" => matches!(ext, "wav" | "ogg" | "mp3" | "flac" | "soundset"),
        "mesh_texture" => {
            matches!(ext, "png" | "jpg" | "jpeg" | "bmp" | "gif" | "tga" | "webp" | "texture")
        }
        "sky_cubemap" => ext == "texture",
        "data_ref" => ext == "data",
        _ => false,
    }
}

/// Object-reference fields (camera follow, joints, navigation, path follow).
pub fn entity_ref_field(field: &str) -> bool {
    matches!(field, "cam_follow" | "joint_to" | "nav_target" | "follow_path" | "remote_target")
}

/// Colour stored as `#RRGGBB` or `#RRGGBBAA`.
pub fn colour_field(field: &str) -> bool {
    matches!(field, "tint" | "shape_color" | "mat_color" | "post_tint")
}

/// Keep assets of `field`'s kind whose name or path contains `query`.
pub fn filter_assets<'a>(
    items: &'a [AssetCandidate],
    field: &str,
    query: &str,
) -> Vec<&'a AssetCandidate> {
    let needle = query.trim().to_lowercase();
    items
        .iter()
        .filter(|item| {
            asset_matches_field(field, &item.path)
                && (needle.is_empty()
                    || item.name.to_lowercase().contains(&needle)
                    || item.path.to_lowercase().contains(&needle))
        })
        .collect()
}

/// Keep objects whose name, type, or tag contains `query`.
pub fn filter_objects<'a>(items: &'a [ObjectCandidate], query: &str) -> Vec<&'a ObjectCandidate> {
    let needle = query.trim().to_lowercase();
    items
        .iter()
        .filter(|item| {
            item.id != "root"
                && (needle.is_empty()
                    || item.name.to_lowercase().contains(&needle)
                    || item.type_name.to_lowercase().contains(&needle)
                    || item.tags.iter().any(|tag| tag.to_lowercase().contains(&needle))
                    || item.id.to_lowercase().contains(&needle))
        })
        .collect()
}

pub fn objects_from_nodes(nodes: &[MockNode]) -> Vec<ObjectCandidate> {
    nodes
        .iter()
        .filter(|node| node.id != "root")
        .map(|node| {
            let shape = node_field(node, "shape").to_string();
            let role = node_field(node, "role").to_string();
            let tag = node_field(node, "tag");
            let type_name = crate::outliner_query::outliner_type_text(node);
            let mut tags = Vec::new();
            if !role.is_empty() {
                tags.push(role.clone());
            }
            if !tag.is_empty() && tag != role {
                tags.push(tag.to_string());
            }
            ObjectCandidate {
                id: node.id.clone(),
                name: node.name.clone(),
                shape,
                role,
                type_name,
                tags,
            }
        })
        .collect()
}

fn node_field<'a>(node: &'a MockNode, key: &str) -> &'a str {
    node.fields.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str()).unwrap_or("")
}

/// Files under `root`, skipping dotted folders.
pub fn collect_assets(root: &Path) -> Vec<AssetCandidate> {
    let mut out = Vec::new();
    walk_assets(root, root, &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn walk_assets(dir: &Path, root: &Path, out: &mut Vec<AssetCandidate>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            walk_assets(&path, root, out);
            continue;
        }
        let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        let kind = crate::asset_browser::kind_of(&name, false).to_string();
        out.push(AssetCandidate { path: rel, name, kind });
    }
}

/// Drag payload from the Asset Browser: the place token, then the project path.
pub fn asset_drag_payload(place: &str, path: &str) -> String {
    format!("place:{place}|{}", asset_reference(path))
}

/// Project path carried on an Asset Browser drag, when one was set.
pub fn asset_path_from_drag(payload: &str) -> Option<&str> {
    if let Some(path) = payload.strip_prefix("asset:") {
        return (!path.is_empty()).then_some(path);
    }
    let rest = payload.strip_prefix("place:")?;
    let (_, path) = rest.split_once('|')?;
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

/// Place token without a trailing asset path.
pub fn place_body_from_drag(payload: &str) -> Option<&str> {
    let rest = payload.strip_prefix("place:")?;
    let body = rest.split('|').next().unwrap_or(rest);
    if body.is_empty() {
        None
    } else {
        Some(body)
    }
}

// --- colour -----------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsv {
    /// 0 to 360.
    pub h: f32,
    /// 0 to 1.
    pub s: f32,
    /// 0 to 1.
    pub v: f32,
}

pub fn rgb_to_hsv(r: f32, g: f32, b: f32) -> Hsv {
    let r = r.clamp(0.0, 1.0);
    let g = g.clamp(0.0, 1.0);
    let b = b.clamp(0.0, 1.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if (max - r).abs() < f32::EPSILON {
        60.0 * (((g - b) / d) % 6.0)
    } else if (max - g).abs() < f32::EPSILON {
        60.0 * (((b - r) / d) + 2.0)
    } else {
        60.0 * (((r - g) / d) + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    let s = if max == 0.0 { 0.0 } else { d / max };
    Hsv { h, s, v: max }
}

pub fn hsv_to_rgb(hsv: Hsv) -> (f32, f32, f32) {
    let h = hsv.h.rem_euclid(360.0);
    let s = hsv.s.clamp(0.0, 1.0);
    let v = hsv.v.clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0).floor() as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (r + m, g + m, b + m)
}

pub fn rgba_from_tint(tint: [f32; 4]) -> Rgba {
    Rgba { r: tint[0], g: tint[1], b: tint[2], a: tint[3] }
}

pub fn hex_to_rgba(text: &str) -> Option<Rgba> {
    let tint = crate::parse_tint(text)?;
    Some(rgba_from_tint(tint))
}

pub fn rgba_to_hex(colour: Rgba) -> String {
    crate::tint_hex(&[colour.r, colour.g, colour.b, colour.a])
}

/// `#RRGGBB` or `#RRGGBBAA` back to the same bytes.
pub fn hex_round_trip(text: &str) -> Option<String> {
    Some(rgba_to_hex(hex_to_rgba(text)?))
}

pub fn remember_colour_list(list: &mut Vec<String>, hex: &str) {
    let hex = rgba_to_hex(hex_to_rgba(hex).unwrap_or(Rgba { r: 1.0, g: 1.0, b: 1.0, a: 1.0 }));
    list.retain(|item| item != &hex);
    list.insert(0, hex);
    list.truncate(RECENT_LIMIT);
}

// --- records ----------------------------------------------------------------

/// A map or struct value edited in the inspector and on graph variables.
#[derive(Clone, Debug, PartialEq)]
pub enum FieldValue {
    Text(String),
    Number(String),
    Bool(bool),
    Colour(String),
    Map(Vec<(String, FieldValue)>),
    Struct {
        type_name: String,
        fields: Vec<(String, FieldValue)>,
    },
    /// Text that could not be read, kept so a save does not drop it.
    Damaged(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadNotes {
    pub damaged: Vec<String>,
}

impl FieldValue {
    pub fn empty_map() -> Self {
        Self::Map(Vec::new())
    }

    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Text(_) => "text",
            Self::Number(_) => "number",
            Self::Bool(_) => "bool",
            Self::Colour(_) => "colour",
            Self::Map(_) => "map",
            Self::Struct { .. } => "struct",
            Self::Damaged(_) => "damaged",
        }
    }
}

pub fn default_for_kind(kind: &str) -> FieldValue {
    match kind {
        "number" => FieldValue::Number("0".into()),
        "bool" => FieldValue::Bool(false),
        "colour" => FieldValue::Colour("#FFFFFF".into()),
        "map" => FieldValue::empty_map(),
        "struct" => FieldValue::Struct { type_name: "Struct".into(), fields: Vec::new() },
        _ => FieldValue::Text(String::new()),
    }
}

pub fn looks_like_record(text: &str) -> bool {
    let t = text.trim();
    t.contains("schema_version")
        && (t.contains("kind = \"map\"") || t.contains("kind = \"struct\""))
}

pub fn serialize_record(value: &FieldValue) -> String {
    let mut entries = vec![("schema_version".into(), SealValue::Number("1".into()))];
    entries.extend(value_entries(value));
    SealValue::Map(SealMap::new(entries)).to_text()
}

fn value_entries(value: &FieldValue) -> Vec<(String, SealValue)> {
    match value {
        FieldValue::Text(text) => vec![
            ("kind".into(), SealValue::String("text".into())),
            ("value".into(), SealValue::String(text.clone())),
        ],
        FieldValue::Number(text) => vec![
            ("kind".into(), SealValue::String("number".into())),
            ("value".into(), SealValue::String(text.clone())),
        ],
        FieldValue::Bool(flag) => vec![
            ("kind".into(), SealValue::String("bool".into())),
            ("value".into(), SealValue::String(if *flag { "yes" } else { "no" }.into())),
        ],
        FieldValue::Colour(text) => vec![
            ("kind".into(), SealValue::String("colour".into())),
            ("value".into(), SealValue::String(text.clone())),
        ],
        FieldValue::Map(rows) => vec![
            ("kind".into(), SealValue::String("map".into())),
            ("entries".into(), SealValue::Array(rows.iter().map(entry_seal).collect())),
        ],
        FieldValue::Struct { type_name, fields } => vec![
            ("kind".into(), SealValue::String("struct".into())),
            ("type_name".into(), SealValue::String(type_name.clone())),
            ("fields".into(), SealValue::Array(fields.iter().map(entry_seal).collect())),
        ],
        FieldValue::Damaged(raw) => vec![
            ("kind".into(), SealValue::String("damaged".into())),
            ("value".into(), SealValue::String(raw.clone())),
        ],
    }
}

fn entry_seal(row: &(String, FieldValue)) -> SealValue {
    let mut entries = vec![("key".into(), SealValue::String(row.0.clone()))];
    entries.extend(value_entries(&row.1));
    SealValue::Map(SealMap::new(entries))
}

/// Read a record. Damaged entries are kept and listed in `notes`.
pub fn load_record(text: &str) -> Result<(FieldValue, LoadNotes), String> {
    let parsed = seal_sealtext::parse(text).map_err(|err| err.to_string())?;
    let Some(map) = parsed.as_map() else {
        return Err("record is not a map".into());
    };
    let mut notes = LoadNotes { damaged: Vec::new() };
    let version = map_get(map, "schema_version").and_then(seal_num);
    if version.as_deref() != Some("1") {
        notes
            .damaged
            .push(format!("schema_version {}", version.unwrap_or_else(|| "missing".into())));
        return Ok((FieldValue::Damaged(text.to_string()), notes));
    }
    Ok((value_from_map(map, &mut notes)?, notes))
}

fn value_from_map(
    map: &[(String, SealValue)],
    notes: &mut LoadNotes,
) -> Result<FieldValue, String> {
    let kind = map_str(map, "kind").unwrap_or("");
    match kind {
        "text" => Ok(FieldValue::Text(map_str(map, "value").unwrap_or("").to_string())),
        "number" => Ok(FieldValue::Number(map_str(map, "value").unwrap_or("0").to_string())),
        "bool" => {
            Ok(FieldValue::Bool(matches!(map_str(map, "value").unwrap_or("no"), "yes" | "true")))
        }
        "colour" => Ok(FieldValue::Colour(map_str(map, "value").unwrap_or("#FFFFFF").to_string())),
        "map" => Ok(FieldValue::Map(read_rows(map, "entries", notes)?)),
        "struct" => Ok(FieldValue::Struct {
            type_name: map_str(map, "type_name").unwrap_or("Struct").to_string(),
            fields: read_rows(map, "fields", notes)?,
        }),
        "damaged" => Ok(FieldValue::Damaged(map_str(map, "value").unwrap_or("").to_string())),
        other => {
            notes.damaged.push(format!("unknown kind {other}"));
            Ok(FieldValue::Damaged(SealValue::Map(SealMap::new(map.to_vec())).to_text()))
        }
    }
}

fn read_rows(
    map: &[(String, SealValue)],
    key: &str,
    notes: &mut LoadNotes,
) -> Result<Vec<(String, FieldValue)>, String> {
    let Some(SealValue::Array(items)) = map_get(map, key) else {
        return Ok(Vec::new());
    };
    let mut rows = Vec::new();
    for item in items {
        let Some(child) = item.as_map() else {
            notes.damaged.push("entry is not a map".into());
            rows.push(("".into(), FieldValue::Damaged(item.to_text())));
            continue;
        };
        let key = map_str(child, "key").unwrap_or("").to_string();
        if key.is_empty() {
            notes.damaged.push("entry has no key".into());
        }
        rows.push((key, value_from_map(child, notes)?));
    }
    Ok(rows)
}

fn map_get<'a>(map: &'a [(String, SealValue)], key: &str) -> Option<&'a SealValue> {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn map_str<'a>(map: &'a [(String, SealValue)], key: &str) -> Option<&'a str> {
    match map_get(map, key) {
        Some(SealValue::String(text)) => Some(text),
        _ => None,
    }
}

fn seal_num(value: &SealValue) -> Option<String> {
    match value {
        SealValue::Number(text) => Some(text.clone()),
        SealValue::String(text) => Some(text.clone()),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RecordOp {
    Set { path: Vec<String>, value: FieldValue },
    Remove { path: Vec<String> },
    Add { path: Vec<String>, key: String, value: FieldValue },
}

pub fn apply_record_op(text: &str, op: RecordOp) -> Result<String, String> {
    let mut value =
        if text.trim().is_empty() { FieldValue::empty_map() } else { load_record(text)?.0 };
    apply_op(&mut value, &op)?;
    Ok(serialize_record(&value))
}

fn apply_op(value: &mut FieldValue, op: &RecordOp) -> Result<(), String> {
    match op {
        RecordOp::Set { path, value: next } if path.is_empty() => {
            *value = next.clone();
            Ok(())
        }
        RecordOp::Set { path, value: next } => {
            let child = walk_mut(value, &path[..path.len() - 1])?;
            let key = path.last().ok_or("missing key")?.as_str();
            let rows = rows_mut(child)?;
            let slot =
                rows.iter_mut().find(|(k, _)| k == key).ok_or_else(|| format!("no key {key}"))?;
            slot.1 = next.clone();
            Ok(())
        }
        RecordOp::Remove { path } => {
            if path.is_empty() {
                return Err("cannot remove the root".into());
            }
            let child = walk_mut(value, &path[..path.len() - 1])?;
            let key = path.last().unwrap().as_str();
            let rows = rows_mut(child)?;
            let before = rows.len();
            rows.retain(|(k, _)| k != key);
            if rows.len() == before {
                return Err(format!("no key {key}"));
            }
            Ok(())
        }
        RecordOp::Add { path, key, value: next } => {
            if key.is_empty() || key.contains('/') {
                return Err("key must be a name without '/'".into());
            }
            let child = walk_mut(value, path)?;
            let rows = rows_mut(child)?;
            if rows.iter().any(|(k, _)| k == key) {
                return Err(format!("key {key} already exists"));
            }
            rows.push((key.clone(), next.clone()));
            Ok(())
        }
    }
}

fn walk_mut<'a>(value: &'a mut FieldValue, path: &[String]) -> Result<&'a mut FieldValue, String> {
    let mut cursor = value;
    for key in path {
        let rows = rows_mut(cursor)?;
        cursor = rows
            .iter_mut()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
            .ok_or_else(|| format!("no key {key}"))?;
    }
    Ok(cursor)
}

fn rows_mut(value: &mut FieldValue) -> Result<&mut Vec<(String, FieldValue)>, String> {
    match value {
        FieldValue::Map(rows) | FieldValue::Struct { fields: rows, .. } => Ok(rows),
        _ => Err("not a map or struct".into()),
    }
}

/// Point at a nested value with `/` between keys.
pub fn record_path(path: &str) -> Vec<String> {
    if path.is_empty() {
        Vec::new()
    } else {
        path.split('/').filter(|part| !part.is_empty()).map(str::to_string).collect()
    }
}

/// Write `record` into a string variable, creating it when missing.
pub fn write_graph_var_record(graph: &mut Graph, name: &str, record: &str) -> bool {
    if !var_name_ok(name) || load_record(record).is_err() {
        return false;
    }
    let next = Value::Str(record.to_string());
    if let Some(var) = graph.vars.iter_mut().find(|var| var.name == name) {
        if var.ty != Ty::Str || var.default == next {
            return false;
        }
        var.default = next;
        return true;
    }
    graph.vars.push(VarDef { name: name.to_string(), ty: Ty::Str, default: next });
    true
}

fn var_name_ok(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(ch) if ch.is_ascii_alphabetic() || ch == '_' => {}
        _ => return false,
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// String variables on a graph file that hold a map or struct, or are empty.
pub fn graph_text_records(text: &str) -> Result<Vec<(String, String)>, String> {
    let graph = Graph::from_text(text).map_err(|err| err.to_string())?;
    Ok(graph_string_records(&graph))
}

pub fn graph_string_records(graph: &Graph) -> Vec<(String, String)> {
    graph
        .vars
        .iter()
        .filter(|var| var.ty == Ty::Str)
        .filter_map(|var| match &var.default {
            Value::Str(text) if looks_like_record(text) || text.trim().is_empty() => {
                Some((var.name.clone(), text.clone()))
            }
            _ => None,
        })
        .collect()
}

// --- saved searches and recent colours --------------------------------------

pub fn load_saved_searches(text: &str) -> Result<(Vec<String>, LoadNotes), String> {
    load_string_list(text, "queries")
}

pub fn save_saved_searches(queries: &[String]) -> String {
    save_string_list("queries", queries)
}

pub fn load_recent_colours(text: &str) -> Result<(Vec<String>, LoadNotes), String> {
    let (list, mut notes) = load_string_list(text, "colours")?;
    let mut kept = Vec::new();
    for item in list {
        if hex_to_rgba(&item).is_some() {
            kept.push(rgba_to_hex(hex_to_rgba(&item).unwrap()));
        } else {
            notes.damaged.push(format!("not a colour: {item}"));
        }
    }
    Ok((kept, notes))
}

pub fn save_recent_colours(colours: &[String]) -> String {
    save_string_list("colours", colours)
}

fn load_string_list(text: &str, key: &str) -> Result<(Vec<String>, LoadNotes), String> {
    if text.trim().is_empty() {
        return Ok((Vec::new(), LoadNotes { damaged: vec!["empty file".into()] }));
    }
    let parsed = seal_sealtext::parse(text).map_err(|err| err.to_string())?;
    let Some(map) = parsed.as_map() else {
        return Err("file is not a map".into());
    };
    let mut notes = LoadNotes { damaged: Vec::new() };
    let version = map_get(map, "schema_version").and_then(seal_num);
    if version.as_deref() != Some("1") {
        notes
            .damaged
            .push(format!("schema_version {}", version.unwrap_or_else(|| "missing".into())));
        return Ok((Vec::new(), notes));
    }
    let Some(SealValue::Array(items)) = map_get(map, key) else {
        notes.damaged.push(format!("missing {key}"));
        return Ok((Vec::new(), notes));
    };
    let mut out = Vec::new();
    for item in items {
        match item {
            SealValue::String(text) if !text.trim().is_empty() => out.push(text.clone()),
            other => notes.damaged.push(format!("skipped {}", other.to_text())),
        }
    }
    Ok((out, notes))
}

fn save_string_list(key: &str, items: &[String]) -> String {
    SealValue::Map(SealMap::new(vec![
        ("schema_version".into(), SealValue::Number("1".into())),
        (key.into(), SealValue::Array(items.iter().cloned().map(SealValue::String).collect())),
    ]))
    .to_text()
}

pub fn saved_searches(session: &EditorSession) -> Vec<String> {
    session
        .models
        .panel_states
        .get(SAVED_KEY)
        .and_then(|text| load_saved_searches(text).ok())
        .map(|(list, _)| list)
        .unwrap_or_default()
}

pub fn recent_colours(session: &EditorSession) -> Vec<String> {
    session
        .models
        .panel_states
        .get(RECENT_KEY)
        .and_then(|text| load_recent_colours(text).ok())
        .map(|(list, _)| list)
        .unwrap_or_default()
}

fn store_panel(session: &mut EditorSession, key: &str, text: String) {
    session.models.panel_states.insert(key.to_string(), text);
}

// --- search -----------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchKind {
    Command,
    Asset,
    Object,
    Setting,
    Doc,
    Console,
}

impl SearchKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Asset => "asset",
            Self::Object => "object",
            Self::Setting => "setting",
            Self::Doc => "doc",
            Self::Console => "console",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Command => "Command",
            Self::Asset => "Asset",
            Self::Object => "Object",
            Self::Setting => "Setting",
            Self::Doc => "Doc",
            Self::Console => "Console",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "command" => Self::Command,
            "asset" => Self::Asset,
            "object" => Self::Object,
            "setting" => Self::Setting,
            "doc" => Self::Doc,
            "console" => Self::Console,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchItem {
    pub kind: SearchKind,
    pub id: String,
    pub title: String,
    pub type_name: String,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Default)]
struct Provider {
    fingerprint: u64,
    generation: u64,
    items: Vec<SearchItem>,
}

/// In-memory index. A provider is rebuilt only when its items change.
#[derive(Clone, Debug, Default)]
pub struct SearchIndex {
    command: Provider,
    asset: Provider,
    object: Provider,
    setting: Provider,
    doc: Provider,
    console: Provider,
}

impl SearchIndex {
    pub fn generation(&self, kind: SearchKind) -> u64 {
        self.provider(kind).generation
    }

    fn provider(&self, kind: SearchKind) -> &Provider {
        match kind {
            SearchKind::Command => &self.command,
            SearchKind::Asset => &self.asset,
            SearchKind::Object => &self.object,
            SearchKind::Setting => &self.setting,
            SearchKind::Doc => &self.doc,
            SearchKind::Console => &self.console,
        }
    }

    fn provider_mut(&mut self, kind: SearchKind) -> &mut Provider {
        match kind {
            SearchKind::Command => &mut self.command,
            SearchKind::Asset => &mut self.asset,
            SearchKind::Object => &mut self.object,
            SearchKind::Setting => &mut self.setting,
            SearchKind::Doc => &mut self.doc,
            SearchKind::Console => &mut self.console,
        }
    }

    /// Replace a provider when `items` differ from the last refresh.
    pub fn refresh(&mut self, kind: SearchKind, items: &[SearchItem]) {
        let fingerprint = fingerprint(items);
        let slot = self.provider_mut(kind);
        if slot.fingerprint == fingerprint && slot.generation > 0 {
            return;
        }
        slot.items = items.to_vec();
        slot.fingerprint = fingerprint;
        slot.generation = slot.generation.saturating_add(1);
    }

    pub fn items(&self) -> impl Iterator<Item = &SearchItem> {
        self.command
            .items
            .iter()
            .chain(self.asset.items.iter())
            .chain(self.object.items.iter())
            .chain(self.setting.items.iter())
            .chain(self.doc.items.iter())
            .chain(self.console.items.iter())
    }
}

fn fingerprint(items: &[SearchItem]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for item in items {
        for byte in item.kind.name().bytes().chain(item.id.bytes()).chain(item.title.bytes()) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= items.len() as u64;
    }
    if items.is_empty() {
        1
    } else {
        hash
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchQuery {
    pub types: Vec<String>,
    pub tags: Vec<String>,
    pub text: String,
}

impl SearchQuery {
    pub fn has_filter(&self) -> bool {
        !self.types.is_empty() || !self.tags.is_empty()
    }
}

pub fn parse_search_query(raw: &str) -> SearchQuery {
    let mut types = Vec::new();
    let mut tags = Vec::new();
    let mut words = Vec::new();
    for token in raw.split_whitespace() {
        if let Some(rest) = token.strip_prefix("type:") {
            if !rest.is_empty() {
                types.push(rest.to_lowercase());
            }
        } else if let Some(rest) = token.strip_prefix("tag:") {
            if !rest.is_empty() {
                tags.push(rest.to_lowercase());
            }
        } else {
            words.push(token.to_lowercase());
        }
    }
    SearchQuery { types, tags, text: words.join(" ") }
}

pub fn search_index(index: &SearchIndex, query: &SearchQuery) -> Vec<SearchItem> {
    index.items().filter(|item| item_matches(item, query)).cloned().collect()
}

fn item_matches(item: &SearchItem, query: &SearchQuery) -> bool {
    if !query.types.is_empty()
        && !query
            .types
            .iter()
            .any(|ty| ty == &item.type_name.to_lowercase() || ty == item.kind.name())
    {
        return false;
    }
    if !query.tags.is_empty()
        && !query.tags.iter().all(|tag| {
            item.tags.iter().any(|have| have.eq_ignore_ascii_case(tag))
                || item.type_name.eq_ignore_ascii_case(tag)
        })
    {
        return false;
    }
    if query.text.is_empty() {
        return true;
    }
    let hay = format!("{} {}", item.title, item.id).to_lowercase();
    query.text.split_whitespace().all(|word| hay.contains(word))
}

pub fn search_hit_id(kind: SearchKind, id: &str) -> String {
    format!("search{SEARCH_SEP}{}{SEARCH_SEP}{id}", kind.name())
}

pub fn split_search_activation(id: &str) -> Option<(String, String)> {
    let rest = id.strip_prefix(&format!("search{SEARCH_SEP}"))?;
    let (kind, target) = rest.split_once(SEARCH_SEP)?;
    if SearchKind::from_name(kind).is_none() || target.is_empty() {
        None
    } else {
        Some((kind.to_string(), target.to_string()))
    }
}

pub fn doc_pages(book: &Path) -> Vec<SearchItem> {
    let mut out = Vec::new();
    walk_docs(book, book, &mut out);
    out.sort_by(|a, b| a.title.cmp(&b.title));
    out
}

fn walk_docs(dir: &Path, root: &Path, out: &mut Vec<SearchItem>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_docs(&path, root, out);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let title = text
            .lines()
            .find_map(|line| line.strip_prefix("# ").map(str::trim))
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                path.file_stem().and_then(|s| s.to_str()).unwrap_or("doc").to_string()
            });
        let folder = path
            .parent()
            .and_then(|dir| dir.strip_prefix(root).ok())
            .map(|rel| rel.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let mut tags = Vec::new();
        if !folder.is_empty() {
            tags.push(folder);
        }
        out.push(SearchItem {
            kind: SearchKind::Doc,
            id: path.to_string_lossy().replace('\\', "/"),
            title,
            type_name: "doc".into(),
            tags,
        });
    }
}

fn session_sources(session: &EditorSession) -> [Vec<SearchItem>; 6] {
    let commands = session
        .commands
        .iter()
        .filter(|spec| spec.alias_of.is_none())
        .map(|spec| SearchItem {
            kind: SearchKind::Command,
            id: spec.id.clone(),
            title: session.messages.get(&spec.title_key),
            type_name: "command".into(),
            tags: vec![spec.category.clone()],
        })
        .collect();
    let root = session
        .project
        .as_ref()
        .map(|project| project.root.clone())
        .or_else(|| session.models.content_tree_root.clone());
    let assets = root
        .as_ref()
        .map(|root| {
            collect_assets(root)
                .into_iter()
                .map(|item| SearchItem {
                    kind: SearchKind::Asset,
                    id: item.path.clone(),
                    title: item.name,
                    type_name: "asset".into(),
                    tags: vec![item.kind],
                })
                .collect()
        })
        .unwrap_or_default();
    let objects = objects_from_nodes(&session.models.nodes)
        .into_iter()
        .map(|item| SearchItem {
            kind: SearchKind::Object,
            id: item.id,
            title: item.name,
            type_name: "object".into(),
            tags: item.tags,
        })
        .collect();
    let settings = session
        .models
        .settings
        .iter()
        .map(|setting| SearchItem {
            kind: SearchKind::Setting,
            id: setting.id.clone(),
            title: session.messages.get(&setting.title_key),
            type_name: "setting".into(),
            tags: vec![setting.category.clone()],
        })
        .collect();
    let book = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/book");
    let docs = if book.is_dir() { doc_pages(&book) } else { Vec::new() };
    let console = crate::models::collect_log_lines(0)
        .into_iter()
        .enumerate()
        .map(|(index, line)| SearchItem {
            kind: SearchKind::Console,
            id: format!("{index}:{}", line.message),
            title: line.message,
            type_name: "console".into(),
            tags: vec![line.level],
        })
        .collect();
    [commands, assets, objects, settings, docs, console]
}

/// Palette rows: commands keep their ranking; other providers follow.
/// `type:` and `tag:` limit the index. The index skips providers that did not change.
pub fn palette_search_hits(session: &EditorSession) -> Vec<PaletteHit> {
    let sources = session_sources(session);
    {
        let mut index = session.search_index.borrow_mut();
        for (offset, kind) in [
            SearchKind::Command,
            SearchKind::Asset,
            SearchKind::Object,
            SearchKind::Setting,
            SearchKind::Doc,
            SearchKind::Console,
        ]
        .into_iter()
        .enumerate()
        {
            index.refresh(kind, &sources[offset]);
        }
    }
    let (mode, query) = crate::command::PaletteMode::split(&session.palette_query);
    if !matches!(mode, crate::command::PaletteMode::Commands) {
        return session.palette_hits();
    }
    let parsed = parse_search_query(query);
    if parsed.has_filter() {
        let index = session.search_index.borrow();
        return search_index(&index, &parsed)
            .into_iter()
            .map(|item| PaletteHit {
                id: if item.kind == SearchKind::Command {
                    item.id
                } else {
                    search_hit_id(item.kind, &item.id)
                },
                title: item.title,
                shortcut: item.kind.label().to_string(),
                score: 1,
            })
            .collect();
    }
    if parsed.text.is_empty() {
        return session.palette_hits();
    }
    let narrowed = SearchQuery { types: Vec::new(), tags: Vec::new(), text: parsed.text.clone() };
    let extras: Vec<PaletteHit> = {
        let index = session.search_index.borrow();
        search_index(&index, &narrowed)
            .into_iter()
            .filter(|item| item.kind != SearchKind::Command)
            .map(|item| PaletteHit {
                id: search_hit_id(item.kind, &item.id),
                title: item.title,
                shortcut: item.kind.label().to_string(),
                score: 1,
            })
            .collect()
    };
    let mut hits = session.palette_hits();
    for hit in &mut hits {
        hit.score += 200;
    }
    hits.extend(extras);
    hits.sort_by(|left, right| {
        right.score.cmp(&left.score).then_with(|| left.title.cmp(&right.title))
    });
    hits
}

// --- commands ---------------------------------------------------------------

type CommandFn = fn(&mut EditorSession, &CommandArgs) -> Result<(), CoreError>;

pub const COMMANDS: &[(&str, &str, &str, CommandFn)] = &[
    ("search.pin", "command-search-pin", "view", pin_search),
    ("search.activate", "command-search-activate", "view", activate_search),
    ("picker.remember_colours", "command-picker-remember", "edit", remember_colours_cmd),
    ("picker.eyedropper", "command-picker-eyedropper", "edit", arm_eyedropper),
    ("picker.show_asset", "command-picker-show-asset", "view", show_asset),
    ("inspector.set_colour", "command-inspector-set-colour", "edit", set_colour),
    ("inspector.record_edit", "command-inspector-record-edit", "edit", record_edit),
    ("graph.var_record", "command-graph-var-record", "edit", graph_var_record),
];

fn pin_search(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    let query = session.palette_query.trim().to_string();
    if query.is_empty() {
        return Err(CoreError::new("nothing to pin"));
    }
    let mut list = saved_searches(session);
    if let Some(index) = list.iter().position(|item| item == &query) {
        list.remove(index);
    } else {
        list.insert(0, query);
    }
    store_panel(session, SAVED_KEY, save_saved_searches(&list));
    Ok(())
}

fn activate_search(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let kind = args.get("kind").unwrap_or("");
    let target = args.get("target").unwrap_or("").to_string();
    if target.is_empty() {
        return Err(CoreError::new("missing search target"));
    }
    match kind {
        "asset" => {
            session.models.content_pick = Some(target.clone());
            if let Some(root) = session
                .project
                .as_ref()
                .map(|project| project.root.clone())
                .or_else(|| session.models.content_tree_root.clone())
            {
                let full = root.join(&target);
                if let Some(folder) = full.parent() {
                    crate::content::open_content_folder(session, folder);
                }
            }
            session.reveal_panel = Some("content".into());
        }
        "object" => {
            session.foundation.selection = vec![target.clone()];
            session.models.selection = Some(target);
            session.reveal_panel = Some("outliner".into());
        }
        "setting" => {
            session.models.settings_query = target;
            session.reveal_panel = Some("settings".into());
        }
        "doc" => {
            session.notice = Some(
                Path::new(&target)
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("doc")
                    .to_string(),
            );
            session.host_action = Some(crate::session::HostAction::OpenPath(PathBuf::from(target)));
        }
        "console" => {
            let message = target.split_once(':').map(|(_, rest)| rest).unwrap_or(target.as_str());
            session.models.console_query = message.to_string();
            session.reveal_panel = Some("console".into());
        }
        "command" => {
            let id = target;
            session.dispatch(&id, &CommandArgs::default())?;
        }
        _ => return Err(CoreError::new("unknown search kind")),
    }
    Ok(())
}

fn remember_colours_cmd(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let value = args.get("value").unwrap_or("");
    let (list, _) = load_recent_colours(value)
        .unwrap_or_else(|_| (Vec::new(), LoadNotes { damaged: Vec::new() }));
    store_panel(session, RECENT_KEY, save_recent_colours(&list));
    Ok(())
}

fn arm_eyedropper(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let field = required(args, "field")?;
    let target = if let Some(ids) = args.get("ids") {
        format!("ids\t{ids}")
    } else {
        format!("id\t{}", required(args, "id")?)
    };
    session.set_draft(EYE_DRAFT, &format!("{field}\t{target}"))?;
    Ok(())
}

/// When an eyedropper is armed, the next selected object is written into the field.
/// One undo step for the whole pick.
pub fn complete_eyedropper(session: &mut EditorSession, picked: &str) {
    let Some(draft) = session.draft(EYE_DRAFT) else {
        return;
    };
    let mut parts = draft.split('\t');
    let Some(field) = parts.next() else {
        return;
    };
    let Some(mode) = parts.next() else {
        return;
    };
    let Some(ids) = parts.next() else {
        return;
    };
    let targets: Vec<String> = if mode == "ids" {
        ids.split(',').filter(|id| !id.is_empty()).map(str::to_string).collect()
    } else {
        vec![ids.to_string()]
    };
    let value = object_reference(picked);
    let _ = session.clear_draft(EYE_DRAFT);
    let _ = assign_field(session, &targets, field, &value);
}

fn show_asset(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let path = asset_reference(&required(args, "path")?);
    if path.is_empty() {
        return Err(CoreError::new("missing asset"));
    }
    session.models.content_pick = Some(path.clone());
    if let Some(root) = session
        .project
        .as_ref()
        .map(|project| project.root.clone())
        .or_else(|| session.models.content_tree_root.clone())
    {
        let full = root.join(&path);
        if let Some(folder) = full.parent() {
            crate::content::open_content_folder(session, folder);
        }
    }
    session.reveal_panel = Some("content".into());
    Ok(())
}

fn set_colour(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let targets = targets_of(args)?;
    let field = args.get("field").unwrap_or("");
    if colour_field(field) {
        let value = required(args, "value")?;
        return assign_field(session, &targets, field, &value);
    }
    let r = required(args, "r")?;
    let g = required(args, "g")?;
    let b = required(args, "b")?;
    let previous = session.world.to_text();
    let mut touched = false;
    for id in &targets {
        let _ = session.models.set_field(id, "r", &r);
        let _ = session.models.set_field(id, "g", &g);
        let _ = session.models.set_field(id, "b", &b);
        session.mirror_node(id);
        touched = true;
    }
    if !touched {
        return Err(CoreError::new("unknown mock node"));
    }
    let next = session.world.to_text();
    if previous != next {
        let label = session.messages.get("command-inspector-set-colour");
        session.push_past(Past::Scene { previous, next, label });
        session.future.clear();
    }
    Ok(())
}

fn record_edit(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let field = required(args, "field")?;
    let op_name = required(args, "op")?;
    let path = record_path(args.get("path").unwrap_or(""));
    let op = match op_name.as_str() {
        "set" => RecordOp::Set {
            path,
            value: scalar_value(
                args.get("kind").unwrap_or("text"),
                args.get("value").unwrap_or(""),
            ),
        },
        "remove" => RecordOp::Remove { path },
        "add" => RecordOp::Add {
            path,
            key: required(args, "key")?,
            value: default_for_kind(args.get("kind").unwrap_or("text")),
        },
        _ => return Err(CoreError::new("unknown record operation")),
    };
    let targets = targets_of(args)?;
    let mut touched = false;
    for id in &targets {
        let current = session
            .models
            .nodes
            .iter()
            .find(|node| &node.id == id)
            .and_then(|node| node.fields.iter().find(|(key, _)| key == &field))
            .map(|(_, value)| value.clone())
            .unwrap_or_default();
        let next = apply_record_op(&current, op.clone()).map_err(CoreError::new)?;
        let Some(previous) = session.models.set_field(id, &field, &next) else {
            continue;
        };
        session.push_past(Past::Field { id: id.clone(), field: field.clone(), previous, next });
        session.mirror_node(id);
        touched = true;
    }
    if !touched {
        return Err(CoreError::new("unknown mock node"));
    }
    session.future.clear();
    Ok(())
}

fn scalar_value(kind: &str, value: &str) -> FieldValue {
    match kind {
        "number" => FieldValue::Number(value.to_string()),
        "bool" => FieldValue::Bool(matches!(value, "yes" | "true")),
        "colour" => FieldValue::Colour(value.to_string()),
        "text" => FieldValue::Text(value.to_string()),
        _ => default_for_kind(kind),
    }
}

fn graph_var_record(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let path = required(args, "path")?;
    let name = required(args, "name")?;
    let value = required(args, "value")?;
    if let Some(doc) = session.graph.as_mut() {
        if doc.label == path {
            if doc.set_var_record(&name, &value) {
                return Ok(());
            }
            return Err(CoreError::new("graph variable was not changed"));
        }
    }
    let root = session
        .project
        .as_ref()
        .map(|project| project.root.clone())
        .or_else(|| session.models.content_tree_root.clone())
        .ok_or_else(|| CoreError::new("no project is open"))?;
    let full = root.join(&path);
    let previous = std::fs::read_to_string(&full).map_err(|err| CoreError::new(err.to_string()))?;
    let mut graph = Graph::from_text(&previous).map_err(|err| CoreError::new(err.to_string()))?;
    if !write_graph_var_record(&mut graph, &name, &value) {
        return Err(CoreError::new("graph variable was not changed"));
    }
    let next = graph.to_text();
    crate::persist::atomic_write(&full, next.as_bytes())?;
    session.push_past(Past::File { path: full.to_string_lossy().to_string(), previous, next });
    session.future.clear();
    Ok(())
}

fn assign_field(
    session: &mut EditorSession,
    targets: &[String],
    field: &str,
    value: &str,
) -> Result<(), CoreError> {
    let mut touched = false;
    for id in targets {
        let Some(previous) = session.models.set_field(id, field, value) else {
            continue;
        };
        if previous == value {
            session.mirror_node(id);
            touched = true;
            continue;
        }
        session.push_past(Past::Field {
            id: id.clone(),
            field: field.to_string(),
            previous,
            next: value.to_string(),
        });
        session.mirror_node(id);
        touched = true;
    }
    if !touched {
        return Err(CoreError::new("unknown mock node"));
    }
    session.future.clear();
    Ok(())
}

fn targets_of(args: &CommandArgs) -> Result<Vec<String>, CoreError> {
    if let Some(ids) = args.get("ids") {
        let list: Vec<String> =
            ids.split(',').filter(|id| !id.is_empty()).map(str::to_string).collect();
        if list.is_empty() {
            return Err(CoreError::new("nothing selected"));
        }
        return Ok(list);
    }
    Ok(vec![required(args, "id")?])
}

fn required(args: &CommandArgs, key: &str) -> Result<String, CoreError> {
    args.get(key)
        .map(str::to_string)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| CoreError::new(format!("missing argument '{key}'")))
}

pub fn eyedropper_armed(session: &EditorSession) -> bool {
    session.draft(EYE_DRAFT).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(path: &str) -> AssetCandidate {
        AssetCandidate {
            path: path.into(),
            name: path.rsplit('/').next().unwrap_or(path).into(),
            kind: "picture".into(),
        }
    }

    #[test]
    fn pickers_filter_assets_by_kind_and_name() {
        let items = vec![
            asset("art/hero.png"),
            asset("art/tree.jpg"),
            asset("logic/pawn.graph"),
            asset("fx/spark.fx"),
        ];
        let pictures = filter_assets(&items, "sprite", "her");
        assert_eq!(pictures.len(), 1);
        assert_eq!(pictures[0].path, "art/hero.png");
        assert!(filter_assets(&items, "script", "")
            .iter()
            .all(|item| item.path.ends_with(".graph")));
        assert!(filter_assets(&items, "particles", "spark").len() == 1);
        assert!(filter_assets(&items, "sprite", "pawn").is_empty());
    }

    #[test]
    fn pickers_write_asset_paths_and_object_ids() {
        assert_eq!(asset_reference(" art\\hero.png "), "art/hero.png");
        assert_eq!(object_reference("  ent-4 "), "ent-4");
        assert_eq!(
            asset_path_from_drag(&asset_drag_payload("sprite:hero.png", "art/hero.png")),
            Some("art/hero.png")
        );
        assert_eq!(
            place_body_from_drag("place:sprite:hero.png|art/hero.png"),
            Some("sprite:hero.png")
        );
    }

    #[test]
    fn pickers_filter_objects_by_name_and_tag() {
        let items = vec![
            ObjectCandidate {
                id: "a".into(),
                name: "Hero".into(),
                shape: "sprite".into(),
                role: "player".into(),
                type_name: "sprite".into(),
                tags: vec!["player".into()],
            },
            ObjectCandidate {
                id: "b".into(),
                name: "Lamp".into(),
                shape: String::new(),
                role: "light".into(),
                type_name: "light".into(),
                tags: vec!["light".into()],
            },
        ];
        assert_eq!(filter_objects(&items, "hero")[0].id, "a");
        assert_eq!(filter_objects(&items, "light")[0].id, "b");
    }

    #[test]
    fn pickers_colour_hex_and_hsv_round_trip() {
        let hex = "#336699";
        assert_eq!(hex_round_trip(hex).as_deref(), Some("#336699"));
        let with_alpha = hex_to_rgba("#33669980").unwrap();
        assert!((with_alpha.a - 128.0 / 255.0).abs() < 0.01);
        assert_eq!(rgba_to_hex(with_alpha).len(), 9);
        for (r, g, b) in
            [(1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0), (0.2, 0.4, 0.8), (1.0, 1.0, 1.0)]
        {
            let hsv = rgb_to_hsv(r, g, b);
            let (rr, gg, bb) = hsv_to_rgb(hsv);
            assert!((rr - r).abs() < 1e-4 && (gg - g).abs() < 1e-4 && (bb - b).abs() < 1e-4);
        }
        assert!((rgb_to_hsv(0.0, 1.0, 0.0).h - 120.0).abs() < 0.01);
    }

    #[test]
    fn pickers_record_edit_round_trip_and_damaged_input() {
        let mut text = serialize_record(&FieldValue::Map(vec![(
            "health".into(),
            FieldValue::Number("3".into()),
        )]));
        text = apply_record_op(
            &text,
            RecordOp::Add {
                path: Vec::new(),
                key: "stats".into(),
                value: FieldValue::Struct {
                    type_name: "Stats".into(),
                    fields: vec![("armor".into(), FieldValue::Number("1".into()))],
                },
            },
        )
        .unwrap();
        text = apply_record_op(
            &text,
            RecordOp::Set {
                path: record_path("stats/armor"),
                value: FieldValue::Number("4".into()),
            },
        )
        .unwrap();
        let (loaded, notes) = load_record(&text).unwrap();
        assert!(notes.damaged.is_empty());
        let FieldValue::Map(rows) = loaded else { panic!("map") };
        assert_eq!(rows[0].0, "health");
        match &rows[1].1 {
            FieldValue::Struct { type_name, fields } => {
                assert_eq!(type_name, "Stats");
                assert_eq!(fields[0].1, FieldValue::Number("4".into()));
            }
            other => panic!("{other:?}"),
        }
        let again = serialize_record(&FieldValue::Map(rows));
        let (loaded_again, _) = load_record(&again).unwrap();
        let (loaded_first, _) = load_record(&text).unwrap();
        assert_eq!(loaded_again, loaded_first);

        let broken = "schema_version = 1\nkind = \"map\"\nentries = [5]\n";
        let (value, notes) = load_record(broken).unwrap();
        assert!(!notes.damaged.is_empty());
        let FieldValue::Map(rows) = value else { panic!("map") };
        assert!(matches!(rows[0].1, FieldValue::Damaged(_)));
        let bad_schema = "schema_version = 9\nkind = \"map\"\n";
        let (_, notes) = load_record(bad_schema).unwrap();
        assert!(notes.damaged.iter().any(|line| line.contains("schema_version")));
        assert!(load_record("not a record").is_err());
    }

    #[test]
    fn pickers_saved_search_file_keeps_damaged_lines() {
        let text = save_saved_searches(&["type:asset hero".into(), "tag:player".into()]);
        let (list, notes) = load_saved_searches(&text).unwrap();
        assert!(notes.damaged.is_empty());
        assert_eq!(list.len(), 2);
        let mixed = "schema_version = 1\nqueries = [\"type:asset hero\", 5]\n";
        let (list, notes) = load_saved_searches(mixed).unwrap();
        assert_eq!(list, vec!["type:asset hero".to_string()]);
        assert_eq!(notes.damaged.len(), 1);
        let colours = "schema_version = 1\ncolours = [\"#336699\", \"nope\"]\n";
        let (list, notes) = load_recent_colours(colours).unwrap();
        assert_eq!(list, vec!["#336699".to_string()]);
        assert_eq!(notes.damaged.len(), 1);
    }

    #[test]
    fn pickers_search_providers_honor_type_and_tag() {
        let mut index = SearchIndex::default();
        let assets = vec![SearchItem {
            kind: SearchKind::Asset,
            id: "art/hero.png".into(),
            title: "hero.png".into(),
            type_name: "asset".into(),
            tags: vec!["picture".into()],
        }];
        index.refresh(SearchKind::Asset, &assets);
        let gen = index.generation(SearchKind::Asset);
        index.refresh(SearchKind::Asset, &assets);
        assert_eq!(index.generation(SearchKind::Asset), gen);
        index.refresh(
            SearchKind::Object,
            &[SearchItem {
                kind: SearchKind::Object,
                id: "e1".into(),
                title: "Hero".into(),
                type_name: "object".into(),
                tags: vec!["player".into()],
            }],
        );
        assert_eq!(index.generation(SearchKind::Asset), gen);
        let query = parse_search_query("type:asset tag:picture hero");
        let hits = search_index(&index, &query);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "art/hero.png");
        assert!(search_index(&index, &parse_search_query("type:object tag:player")).len() == 1);
        assert!(search_index(&index, &parse_search_query("type:asset tag:player")).is_empty());
        assert!(
            split_search_activation(&search_hit_id(SearchKind::Asset, "art/hero.png")).is_some()
        );
    }

    #[test]
    fn pickers_graph_var_record_round_trip() {
        let mut graph = Graph::new("pawn");
        let record =
            serialize_record(&FieldValue::Map(vec![("hp".into(), FieldValue::Number("5".into()))]));
        assert!(write_graph_var_record(&mut graph, "bag", &record));
        let text = graph.to_text();
        let again = Graph::from_text(&text).unwrap();
        let stored = graph_string_records(&again);
        assert_eq!(stored.len(), 1);
        let (loaded, _) = load_record(&stored[0].1).unwrap();
        assert_eq!(loaded, FieldValue::Map(vec![("hp".into(), FieldValue::Number("5".into()))]));
        assert!(!write_graph_var_record(&mut graph, "bag", "not-a-record"));
    }

    #[test]
    fn pickers_one_undo_step_per_pick() {
        use crate::command::CommandArgs;
        use crate::layout::Layout;

        let mut session = EditorSession::new(Layout::default());
        let id = session.models.nodes.iter().find(|node| node.id != "root").unwrap().id.clone();
        let before = session.past.len();
        session
            .dispatch(
                "inspector.set",
                &CommandArgs::new(&[
                    ("id", id.as_str()),
                    ("field", "cam_follow"),
                    ("value", "lamp"),
                ]),
            )
            .unwrap();
        assert_eq!(session.past.len(), before + 1);
        session.dispatch("edit.undo", &CommandArgs::default()).unwrap();
        assert_eq!(session.past.len(), before);
        let again = session.past.len();
        session
            .dispatch(
                "inspector.set_colour",
                &CommandArgs::new(&[("id", id.as_str()), ("field", "tint"), ("value", "#336699")]),
            )
            .unwrap();
        assert_eq!(session.past.len(), again + 1);
        session.dispatch("edit.undo", &CommandArgs::default()).unwrap();
        assert_eq!(session.past.len(), again);
    }
}
