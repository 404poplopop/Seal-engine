//! Pixel animation files (`.ase` and `.aseprite`).
//!
//! One picture per frame is flattened from the visible layers. Import writes a
//! packed PNG and a sprite animation beside the source. A sprite animation has
//! one speed per clip and no named cells, so tags become clips and slices stay
//! on the preview.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime};

use seal_world::frames::{self, Clip, Frame, SpriteFrames};

use crate::command::CommandArgs;
use crate::error::CoreError;
use crate::session::EditorSession;

/// Draft holding the frame shown in the read-only preview.
pub const FRAME_DRAFT: &str = "sprite-source-frame";

const MAGIC: u16 = 0xA5E0;
const FRAME_MAGIC: u16 = 0xF1FA;
const MAX_FILE: u32 = 32 * 1024 * 1024;
const MAX_EDGE: u32 = 4096;
const MAX_FRAME_PIXELS: usize = 4_194_304;
const MAX_TOTAL_PIXELS: usize = 8_388_608;
const MAX_FRAMES: u32 = 1024;
const MAX_LAYERS: usize = 256;
const MAX_CHUNKS: u32 = 100_000;
const MAX_STRING: usize = 1024;
const MAX_CEL_BYTES: usize = MAX_FRAME_PIXELS * 4;
const MAX_HOLD: usize = 256;
const SHEET_EDGE: u32 = 8192;

const LAYER: u16 = 0x2004;
const CEL: u16 = 0x2005;
const TAGS: u16 = 0x2018;
const PALETTE: u16 = 0x2019;
const SLICE: u16 = 0x2022;

/// How a tag walks its frames. Ping-pong is written as forward then back,
/// because a sprite animation clip only stores a list of frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Reverse,
    PingPong,
    PingPongReverse,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub from: u32,
    pub to: u32,
    pub direction: Direction,
    pub repeat: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slice {
    pub name: String,
    pub frame: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Flattened frames, tags and slices from one pixel animation file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub width: u32,
    pub height: u32,
    pub frames: Vec<Vec<u8>>,
    pub durations_ms: Vec<u32>,
    pub tags: Vec<Tag>,
    pub slices: Vec<Slice>,
    /// Blend and tile-layer notes. Each is reported in the Console once per file version.
    pub notes: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ImportReport {
    pub sheet_name: String,
    pub anim_name: String,
    pub notes: Vec<String>,
}

/// Whether `name` is a `.ase` or `.aseprite` file.
pub fn is_source_name(name: &str) -> bool {
    let ext = Path::new(name).extension().and_then(|e| e.to_str()).unwrap_or("");
    ext.eq_ignore_ascii_case("ase") || ext.eq_ignore_ascii_case("aseprite")
}

/// First frame as RGBA8, for thumbnails.
pub fn preview(path: &Path) -> Option<(Vec<u8>, u32, u32)> {
    let src = load(path).ok()?;
    let frame = src.frames.first()?.clone();
    Some((frame, src.width, src.height))
}

pub fn load(path: &Path) -> Result<Source, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    parse(&bytes)
}

pub fn parse(bytes: &[u8]) -> Result<Source, String> {
    let file = read_file(bytes)?;
    let factors = layer_factors(&file.layers);
    let mut notes = file.notes;
    if file.layers.iter().any(|layer| layer.blend != 0 && layer.kind == 0) {
        notes.push("A layer uses a blend other than normal and was drawn as normal.".to_string());
    }
    let mut out = Vec::with_capacity(file.frames.len());
    let area = (file.width as usize)
        .checked_mul(file.height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| "the picture size is not usable".to_string())?;
    for frame in &file.frames {
        let mut px = vec![0u8; area];
        let mut order: Vec<usize> = (0..frame.cels.len()).collect();
        order.sort_by_key(|&i| frame.cels[i].layer);
        for i in order {
            let cel = &frame.cels[i];
            let Some(layer_op) = factors.get(cel.layer as usize).copied().flatten() else {
                continue;
            };
            let Image { w, h, rgba } = &cel.image;
            blit(
                &mut Canvas { pixels: &mut px, width: file.width, height: file.height },
                cel.x,
                cel.y,
                rgba,
                *w,
                *h,
                cel.opacity,
                layer_op,
            );
        }
        out.push(px);
    }
    Ok(Source {
        width: file.width,
        height: file.height,
        frames: out,
        durations_ms: file.frames.iter().map(|f| f.duration_ms).collect(),
        tags: file.tags,
        slices: file.slices,
        notes,
    })
}

/// Rebuild the picture and sprite animation when this source already has them
/// and the source is newer. Blend notes are reported once per file version.
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

/// While a pixel animation is open, rebuild its generated files after the source changes.
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

/// One pixel-animation file the folder watcher actually rebuilt or failed to rebuild.
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
    let Some(frames) = generated_frames(path, &name) else {
        return RebuildGate::Skip;
    };
    let src_m = match std::fs::metadata(path).and_then(|m| m.modified()) {
        Ok(time) => time,
        Err(err) => return RebuildGate::Broken(err.to_string()),
    };
    let out_m =
        std::fs::metadata(&frames).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
    if src_m <= out_m || fail_seen(path, src_m) {
        RebuildGate::Skip
    } else {
        RebuildGate::Go(src_m)
    }
}

/// Rebuild generated files in this folder whose source is newer.
/// Each entry is a source that was rebuilt or failed, not a source left unchanged.
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

/// Rebuild generated files in this folder whose source is newer.
pub fn reimport_dir(dir: &Path) {
    let _ = reimport_dir_outcomes(dir);
}

pub fn reimport_source(path: &Path) -> Result<Vec<String>, String> {
    let src_m = match rebuild_gate(path) {
        RebuildGate::Skip => return Ok(Vec::new()),
        RebuildGate::Broken(err) => return Err(err),
        RebuildGate::Go(time) => time,
    };
    match import_path(path) {
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

/// `asset.sprite_source_import [path=]`: write a picture and a sprite animation
/// next to the pixel animation. Without a path, the file open in the Asset Editor is used.
pub(crate) fn cmd_import(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let rel = match args.get("path").map(str::trim).filter(|p| !p.is_empty()) {
        Some(path) => checked_rel(path)?,
        None => session
            .draft(crate::asset_editor::ASSET_DRAFT)
            .filter(|r| !r.is_empty())
            .ok_or_else(|| CoreError::new("open a pixel animation in the Asset Editor"))?,
    };
    if !is_source_name(&rel) {
        return Err(CoreError::new("choose a .ase or .aseprite file"));
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
    let report = import_path(&full).map_err(CoreError::new)?;
    emit_once(&full, &report.notes);
    let parent = rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let join = |name: &str| {
        if parent.is_empty() {
            name.to_string()
        } else {
            format!("{parent}/{name}")
        }
    };
    let sheet = join(&report.sheet_name);
    let anim = join(&report.anim_name);
    session.notify(
        session.messages.format(
            "asset-sprite-source-imported",
            &[("sheet", sheet.as_str()), ("anim", anim.as_str())],
        ),
        false,
    );
    crate::content::refresh_content_tree(session);
    crate::asset_index::record_import_activity(
        session,
        &rel,
        "sprite import",
        "ok",
        std::time::Instant::now(),
    );
    Ok(())
}

pub fn import_path(path: &Path) -> Result<ImportReport, String> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| "choose a file".to_string())?;
    if !is_source_name(&name) {
        return Err("choose a .ase or .aseprite file".into());
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| plain_name(s))
        .ok_or_else(|| "the file name cannot be used".to_string())?;
    let dir = path.parent().ok_or_else(|| "the file has no folder".to_string())?;
    let source = load(path)?;
    let (sheet_path, anim_path, sheet_name, anim_name) = output_paths(dir, &stem, &name)?;
    let (lib, mut notes) = animation(&source, &sheet_name)?;
    notes.extend(source.notes.iter().cloned());
    if !source.slices.is_empty() {
        notes.push(
            "Slices stay in the preview. A sprite animation has no named cells, so they were not written."
                .into(),
        );
    }
    let png = encode_png(&pack_sheet(&source)?, source_sheet_size(&source)?)?;
    // The picture is written first so the animation never names a picture that was not written.
    crate::persist::atomic_write(&sheet_path, &png).map_err(|e| e.to_string())?;
    let mut text = String::new();
    text.push_str(&format!("# generated-from {name}\n# sheet {sheet_name}\n"));
    text.push_str(
        "# timing clip speed is kept to three decimal places. A shared duration step repeats frames; past 256 frames the speed is the average duration. A zero duration counts as 100 ms, or the file default when it has one.\n",
    );
    if !source.slices.is_empty() {
        text.push_str("# slices not written; a sprite animation has no named cells\n");
    }
    text.push_str(&lib.to_text());
    if let Err(err) = crate::persist::atomic_write(&anim_path, text.as_bytes()) {
        if !anim_path.is_file() {
            let _ = std::fs::remove_file(&sheet_path);
        }
        return Err(err.to_string());
    }
    frames::invalidate(&anim_path);
    Ok(ImportReport { sheet_name, anim_name, notes })
}

fn output_paths(
    dir: &Path,
    stem: &str,
    source_name: &str,
) -> Result<(PathBuf, PathBuf, String, String), String> {
    let anim_name = format!("{stem}.frames");
    let anim_path = dir.join(&anim_name);
    let default_sheet = format!("{stem}.png");
    if anim_path.is_file() {
        let text = std::fs::read_to_string(&anim_path).unwrap_or_default();
        match marker_source(&text) {
            Some(from) if from == source_name => {
                let sheet_name = marker_sheet(&text).unwrap_or(default_sheet);
                if !plain_name(&sheet_name) {
                    return Err(format!("{anim_name} names a picture outside its folder"));
                }
                return Ok((dir.join(&sheet_name), anim_path, sheet_name, anim_name));
            }
            Some(from) => {
                return Err(format!("{anim_name} was generated from {from}"));
            }
            None => return Err(format!("{anim_name} already exists")),
        }
    }
    let sheet_path = dir.join(&default_sheet);
    if sheet_path.exists() {
        return Err(format!("{default_sheet} already exists"));
    }
    Ok((sheet_path, anim_path, default_sheet, anim_name))
}

fn generated_frames(path: &Path, source_name: &str) -> Option<PathBuf> {
    let stem = path.file_stem()?.to_str()?;
    let frames = path.with_file_name(format!("{stem}.frames"));
    if !frames.is_file() {
        return None;
    }
    let text = std::fs::read_to_string(&frames).unwrap_or_default();
    (marker_source(&text).as_deref() == Some(source_name)).then_some(frames)
}

fn marker_source(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix("# generated-from ").map(|s| s.trim().to_string()))
}

fn marker_sheet(text: &str) -> Option<String> {
    text.lines().find_map(|line| line.strip_prefix("# sheet ").map(|s| s.trim().to_string()))
}

fn plain_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0'])
}

fn checked_rel(rel: &str) -> Result<String, CoreError> {
    let rel = rel.trim().replace('\\', "/");
    if rel.is_empty() || rel.starts_with('/') || rel.split('/').any(|p| p == "..") {
        return Err(CoreError::new(format!("'{rel}' is not a path inside the project")));
    }
    Ok(rel)
}

fn animation(source: &Source, sheet: &str) -> Result<(SpriteFrames, Vec<String>), String> {
    let mut notes = Vec::new();
    let mut used = HashSet::new();
    let mut clips = Vec::new();
    let tags = if source.tags.is_empty() {
        vec![Tag {
            name: "all".into(),
            from: 0,
            to: source.frames.len().saturating_sub(1) as u32,
            direction: Direction::Forward,
            repeat: 0,
        }]
    } else {
        source.tags.clone()
    };
    let mut averaged = Vec::new();
    let mut dropped = Vec::new();
    let grid = source_sheet_grid(source)?;
    for tag in tags {
        if tag.from > tag.to || tag.to as usize >= source.frames.len() {
            return Err(format!("tag '{}' points outside the file", tag.name));
        }
        let order = frame_order(tag.from, tag.to, tag.direction);
        let (indices, fps, used_average) = expand(&order, &source.durations_ms);
        let name = clip_name(&tag.name, &mut used);
        if used_average {
            averaged.push(name.clone());
        }
        if tag.repeat > 1 {
            dropped.push(name.clone());
        }
        let frames = indices.into_iter().map(|i| Frame::cell(sheet, cell_uv(i, &grid))).collect();
        clips.push(Clip { name, fps, looping: tag.repeat == 0, frames });
    }
    if !averaged.is_empty() {
        notes.push(format!(
            "Clips {} use the average frame duration, because repeating frames to match each duration would pass 256 frames.",
            averaged.join(", ")
        ));
    }
    if !dropped.is_empty() {
        notes.push(format!(
            "Clips {} play once. Extra repeats were dropped; a sprite animation can loop or play once.",
            dropped.join(", ")
        ));
    }
    Ok((SpriteFrames { clips }, notes))
}

fn frame_order(from: u32, to: u32, direction: Direction) -> Vec<u32> {
    let forward: Vec<u32> = (from..=to).collect();
    match direction {
        Direction::Forward => forward,
        Direction::Reverse => forward.into_iter().rev().collect(),
        Direction::PingPong => {
            if forward.len() < 2 {
                return forward;
            }
            let mut seq = forward.clone();
            seq.extend(forward.iter().rev().skip(1).take(forward.len() - 2).copied());
            seq
        }
        Direction::PingPongReverse => {
            if forward.len() < 2 {
                return forward.into_iter().rev().collect();
            }
            let mut seq: Vec<u32> = forward.iter().rev().copied().collect();
            seq.extend(forward.iter().skip(1).take(forward.len() - 2).copied());
            seq
        }
    }
}

/// `(frame indices, frames per second, used the average duration)`.
fn expand(indices: &[u32], durations: &[u32]) -> (Vec<u32>, f32, bool) {
    let ds: Vec<u32> =
        indices.iter().map(|i| durations.get(*i as usize).copied().unwrap_or(100).max(1)).collect();
    if ds.is_empty() {
        return (Vec::new(), 8.0, false);
    }
    let step = ds.iter().copied().reduce(gcd).unwrap_or(1).max(1);
    let reps: Vec<u32> = ds.iter().map(|d| (d / step).max(1)).collect();
    let total: usize = reps.iter().map(|r| *r as usize).sum();
    if total <= MAX_HOLD {
        let mut out = Vec::with_capacity(total);
        for (index, n) in indices.iter().zip(reps) {
            out.extend(std::iter::repeat_n(*index, n as usize));
        }
        (out, round_fps(1000.0 / step as f32), false)
    } else {
        let mean = ds.iter().map(|d| *d as f32).sum::<f32>() / ds.len() as f32;
        (indices.to_vec(), round_fps(1000.0 / mean), true)
    }
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a.max(1)
}

fn round_fps(v: f32) -> f32 {
    let v = if v.is_finite() { v } else { 8.0 };
    ((v * 1000.0).round() / 1000.0).clamp(0.001, 1000.0)
}

fn clip_name(raw: &str, used: &mut HashSet<String>) -> String {
    let mut name: String = raw
        .chars()
        .map(|c| if c.is_whitespace() || c == '#' || c == '=' { '_' } else { c })
        .collect();
    name = name.trim_matches('_').to_string();
    if name.is_empty() {
        name = "clip".into();
    }
    let base = name.clone();
    let mut n = 2u32;
    while used.contains(&name) {
        name = format!("{base}_{n}");
        n += 1;
    }
    used.insert(name.clone());
    name
}

struct Grid {
    cols: u32,
    rows: u32,
}

fn source_sheet_grid(source: &Source) -> Result<Grid, String> {
    let n = source.frames.len() as u32;
    if n == 0 || source.width == 0 || source.height == 0 {
        return Err("the picture has no frames".into());
    }
    let cols = (SHEET_EDGE / source.width).clamp(1, n);
    let rows = n.div_ceil(cols);
    let sheet_w = cols
        .checked_mul(source.width)
        .ok_or_else(|| "the frames do not fit on one picture".to_string())?;
    let sheet_h = rows
        .checked_mul(source.height)
        .ok_or_else(|| "the frames do not fit on one picture".to_string())?;
    if sheet_w > SHEET_EDGE || sheet_h > SHEET_EDGE {
        return Err("the frames do not fit on one picture".into());
    }
    let pixels = (sheet_w as usize).saturating_mul(sheet_h as usize);
    if pixels > MAX_TOTAL_PIXELS {
        return Err("the frames do not fit on one picture".into());
    }
    Ok(Grid { cols, rows })
}

fn source_sheet_size(source: &Source) -> Result<(u32, u32), String> {
    let grid = source_sheet_grid(source)?;
    Ok((grid.cols * source.width, grid.rows * source.height))
}

fn cell_uv(index: u32, grid: &Grid) -> [f32; 4] {
    let c = (index % grid.cols) as f32;
    let r = (index / grid.cols) as f32;
    let (cols, rows) = (grid.cols as f32, grid.rows as f32);
    [c / cols, r / rows, (c + 1.0) / cols, (r + 1.0) / rows]
}

fn pack_sheet(source: &Source) -> Result<Vec<u8>, String> {
    let grid = source_sheet_grid(source)?;
    let (sheet_w, sheet_h) = (grid.cols * source.width, grid.rows * source.height);
    let mut sheet = vec![0u8; (sheet_w as usize) * (sheet_h as usize) * 4];
    let row_bytes = (source.width as usize) * 4;
    for (i, frame) in source.frames.iter().enumerate() {
        let col = (i as u32) % grid.cols;
        let row = (i as u32) / grid.cols;
        for y in 0..source.height {
            let src = (y as usize) * row_bytes;
            let dst_y = row * source.height + y;
            let dst = ((dst_y * sheet_w + col * source.width) as usize) * 4;
            let end = src + row_bytes;
            if end > frame.len() || dst + row_bytes > sheet.len() {
                return Err("a frame is the wrong size".into());
            }
            sheet[dst..dst + row_bytes].copy_from_slice(&frame[src..end]);
        }
    }
    Ok(sheet)
}

fn encode_png(rgba: &[u8], size: (u32, u32)) -> Result<Vec<u8>, String> {
    let (w, h) = size;
    let mut buf = Vec::new();
    let mut encoder = png::Encoder::new(&mut buf, w, h);
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
    depth: u16,
    transparent: u8,
    fallback_ms: u32,
    opacity_valid: bool,
    layers: Vec<Layer>,
    frames: Vec<RawFrame>,
    tags: Vec<Tag>,
    slices: Vec<Slice>,
    palette: Option<Vec<[u8; 4]>>,
    notes: Vec<String>,
}

struct Layer {
    #[allow(dead_code)]
    name: String,
    visible: bool,
    opacity: u8,
    blend: u16,
    child_level: i32,
    kind: u16,
}

struct RawFrame {
    duration_ms: u32,
    cels: Vec<Cel>,
}

struct Cel {
    layer: u16,
    x: i32,
    y: i32,
    opacity: u8,
    image: Image,
}

#[derive(Clone)]
struct Image {
    w: u32,
    h: u32,
    rgba: Vec<u8>,
}

enum Stored {
    Raw(Vec<u8>, u32, u32),
    Compressed(Vec<u8>, u32, u32),
    Link(u16),
}

struct StoredCel {
    layer: u16,
    x: i32,
    y: i32,
    opacity: u8,
    stored: Stored,
}

struct StoredFrame {
    duration_ms: u32,
    cels: Vec<StoredCel>,
}

fn read_file(bytes: &[u8]) -> Result<FileData, String> {
    if bytes.len() < 128 {
        return Err("the file is truncated".into());
    }
    let file_size = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if !(128..=MAX_FILE).contains(&file_size) {
        return Err("the file size is not usable".into());
    }
    if bytes.len() < file_size as usize {
        return Err("the file is truncated".into());
    }
    let mut r = Reader::new(&bytes[..file_size as usize]);
    r.skip(4)?;
    if r.u16()? != MAGIC {
        return Err("the file is not a pixel animation".into());
    }
    let frame_count = r.u16()? as u32;
    let width = r.u16()? as u32;
    let height = r.u16()? as u32;
    let depth = r.u16()?;
    let flags = r.u32()?;
    let speed = r.u16()? as u32;
    r.skip(8)?;
    let transparent = r.u8()?;
    r.skip(3)?;
    let _colors = r.u16()?;
    r.skip(2)?;
    r.skip(8)?;
    r.skip(84)?;
    if !matches!(depth, 8 | 16 | 32) {
        return Err("the colour depth is not usable".into());
    }
    if width == 0 || height == 0 || width > MAX_EDGE || height > MAX_EDGE {
        return Err("the picture size is not usable".into());
    }
    let area = (width as usize).saturating_mul(height as usize);
    if area > MAX_FRAME_PIXELS {
        return Err("the picture size is not usable".into());
    }
    if frame_count == 0 || frame_count > MAX_FRAMES {
        return Err("the frame count is not usable".into());
    }
    if area.saturating_mul(frame_count as usize) > MAX_TOTAL_PIXELS {
        return Err("the picture size is not usable".into());
    }
    let mut file = FileData {
        width,
        height,
        depth,
        transparent,
        fallback_ms: if speed == 0 { 100 } else { speed },
        opacity_valid: flags & 1 != 0,
        layers: Vec::new(),
        frames: Vec::new(),
        tags: Vec::new(),
        slices: Vec::new(),
        palette: None,
        notes: Vec::new(),
    };
    let mut stored = Vec::new();
    for _ in 0..frame_count {
        stored.push(read_frame(&mut r, &mut file)?);
    }
    finish(&mut file, stored)?;
    Ok(file)
}

fn read_frame(r: &mut Reader<'_>, file: &mut FileData) -> Result<StoredFrame, String> {
    let start = r.pos;
    let size = r.u32()? as usize;
    if size < 12 || start + size > r.data.len() {
        return Err("the file is truncated".into());
    }
    let frame_end = start + size;
    if r.u16()? != FRAME_MAGIC {
        return Err("a frame is damaged".into());
    }
    let old_chunks = r.u16()?;
    let duration = u32::from(r.u16()?);
    r.skip(2)?;
    let chunks = if old_chunks == 0xFFFF { r.u32()? } else { u32::from(old_chunks) };
    if chunks > MAX_CHUNKS {
        return Err("a frame has too many chunks".into());
    }
    if r.pos > frame_end {
        return Err("the file is truncated".into());
    }
    let mut cels = Vec::new();
    for _ in 0..chunks {
        if r.pos + 6 > frame_end {
            return Err("the file is truncated".into());
        }
        let csize = r.u32()? as usize;
        let kind = r.u16()?;
        if csize < 6 || r.pos + csize - 6 > frame_end {
            return Err("the file is truncated".into());
        }
        let payload = r.bytes(csize - 6)?.to_vec();
        match kind {
            LAYER => read_layer(&payload, file)?,
            CEL => {
                if let Some(cel) = read_cel(&payload, file)? {
                    cels.push(cel);
                }
            }
            TAGS => file.tags.extend(read_tags(&payload)?),
            PALETTE => file.palette = Some(read_palette(&payload, file.palette.take())?),
            SLICE => file.slices.extend(read_slices(&payload)?),
            _ => {}
        }
    }
    r.pos = frame_end;
    let duration_ms = if duration == 0 { file.fallback_ms.max(1) } else { duration };
    Ok(StoredFrame { duration_ms, cels })
}

fn read_layer(payload: &[u8], file: &mut FileData) -> Result<(), String> {
    if file.layers.len() >= MAX_LAYERS {
        return Err("the file has too many layers".into());
    }
    let mut p = Reader::new(payload);
    let flags = p.u16()?;
    let kind = p.u16()?;
    let child = p.u16()? as i32;
    p.skip(4)?;
    let blend = p.u16()?;
    let mut opacity = p.u8()?;
    if !file.opacity_valid {
        opacity = 255;
    }
    if kind == 2 && !file.notes.iter().any(|n| n.contains("tile layer")) {
        file.notes.push("A tile layer was skipped.".into());
    }
    p.skip(3)?;
    let name = p.string()?;
    file.layers.push(Layer {
        name,
        visible: flags & 1 != 0,
        opacity,
        blend,
        child_level: child,
        kind,
    });
    Ok(())
}

fn read_cel(payload: &[u8], file: &FileData) -> Result<Option<StoredCel>, String> {
    let mut p = Reader::new(payload);
    let layer = p.u16()?;
    if layer as usize >= file.layers.len() {
        return Err("a cel names a missing layer".into());
    }
    let x = i32::from(p.i16()?);
    let y = i32::from(p.i16()?);
    let opacity = p.u8()?;
    let cel_type = p.u16()?;
    p.skip(2)?;
    p.skip(5)?;
    let stored = match cel_type {
        0 | 2 => {
            let w = u32::from(p.u16()?);
            let h = u32::from(p.u16()?);
            if w > MAX_EDGE || h > MAX_EDGE {
                return Err("a cel is too large".into());
            }
            let bpp = usize::from(file.depth / 8);
            let bytes = (w as usize)
                .checked_mul(h as usize)
                .and_then(|n| n.checked_mul(bpp))
                .ok_or_else(|| "a cel is too large".to_string())?;
            if bytes > MAX_CEL_BYTES || (w as usize) * (h as usize) > MAX_FRAME_PIXELS {
                return Err("a cel is too large".into());
            }
            let rest = p.rest().to_vec();
            if cel_type == 0 {
                if rest.len() < bytes {
                    return Err("a cel is the wrong size".into());
                }
                Stored::Raw(rest[..bytes].to_vec(), w, h)
            } else {
                if rest.len() > MAX_CEL_BYTES {
                    return Err("a compressed cel is too large".into());
                }
                Stored::Compressed(rest, w, h)
            }
        }
        1 => Stored::Link(p.u16()?),
        3 => return Ok(None),
        _ => return Err("a cel type is not usable".into()),
    };
    Ok(Some(StoredCel { layer, x, y, opacity, stored }))
}

fn read_tags(payload: &[u8]) -> Result<Vec<Tag>, String> {
    let mut p = Reader::new(payload);
    let n = p.u16()? as usize;
    if n > 256 {
        return Err("the file has too many tags".into());
    }
    p.skip(8)?;
    let mut tags = Vec::with_capacity(n);
    for _ in 0..n {
        let from = u32::from(p.u16()?);
        let to = u32::from(p.u16()?);
        let direction = match p.u8()? {
            0 => Direction::Forward,
            1 => Direction::Reverse,
            2 => Direction::PingPong,
            3 => Direction::PingPongReverse,
            _ => return Err("a tag direction is not usable".into()),
        };
        let repeat = p.u16()?;
        p.skip(6)?;
        p.skip(4)?;
        let name = p.string()?;
        tags.push(Tag { name, from, to, direction, repeat });
    }
    Ok(tags)
}

fn read_palette(payload: &[u8], previous: Option<Vec<[u8; 4]>>) -> Result<Vec<[u8; 4]>, String> {
    let mut p = Reader::new(payload);
    let _size = p.u32()?;
    let first = p.u32()? as usize;
    let last = p.u32()? as usize;
    p.skip(8)?;
    if first > last || last > 255 {
        return Err("the palette is not usable".into());
    }
    let mut pal = previous.unwrap_or_else(|| vec![[0, 0, 0, 255]; 256]);
    if pal.len() < 256 {
        pal.resize(256, [0, 0, 0, 255]);
    }
    for slot in pal.iter_mut().take(last + 1).skip(first) {
        let flags = p.u16()?;
        *slot = [p.u8()?, p.u8()?, p.u8()?, p.u8()?];
        if flags & 1 != 0 {
            let _name = p.string()?;
        }
    }
    Ok(pal)
}

fn read_slices(payload: &[u8]) -> Result<Vec<Slice>, String> {
    let mut p = Reader::new(payload);
    let keys = p.u32()?;
    if keys > 4096 {
        return Err("a slice has too many keys".into());
    }
    let flags = p.u32()?;
    p.skip(4)?;
    let name = p.string()?;
    let mut slices = Vec::new();
    for _ in 0..keys {
        if slices.len() >= 256 {
            return Err("the file has too many slices".into());
        }
        let frame = p.u32()?;
        let x = p.i32()?;
        let y = p.i32()?;
        let width = p.u32()?;
        let height = p.u32()?;
        if flags & 1 != 0 {
            p.skip(16)?;
        }
        if flags & 2 != 0 {
            p.skip(8)?;
        }
        let slice_name = if name.is_empty() { "slice".into() } else { name.clone() };
        slices.push(Slice { name: slice_name, frame, x, y, width, height });
    }
    Ok(slices)
}

fn expected_len(w: u32, h: u32, depth: u16) -> Result<usize, String> {
    let bpp = usize::from(depth / 8);
    (w as usize)
        .checked_mul(h as usize)
        .and_then(|n| n.checked_mul(bpp))
        .filter(|n| *n <= MAX_CEL_BYTES)
        .ok_or_else(|| "a cel is too large".to_string())
}

fn inflate(data: &[u8], expected: usize) -> Result<Vec<u8>, String> {
    if data.len() > MAX_CEL_BYTES {
        return Err("a compressed cel is too large".into());
    }
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(data, expected)
        .map_err(|_| "compressed cel could not be read".to_string())?;
    if raw.len() != expected {
        return Err("a cel is the wrong size".into());
    }
    Ok(raw)
}

fn to_rgba(raw: &[u8], w: u32, h: u32, file: &FileData) -> Result<Image, String> {
    let pixels =
        (w as usize).checked_mul(h as usize).ok_or_else(|| "a cel is too large".to_string())?;
    let mut rgba = vec![0u8; pixels * 4];
    match file.depth {
        32 => {
            if raw.len() < pixels * 4 {
                return Err("a cel is the wrong size".into());
            }
            rgba.copy_from_slice(&raw[..pixels * 4]);
        }
        16 => {
            if raw.len() < pixels * 2 {
                return Err("a cel is the wrong size".into());
            }
            for i in 0..pixels {
                let v = raw[i * 2];
                let a = raw[i * 2 + 1];
                rgba[i * 4..i * 4 + 4].copy_from_slice(&[v, v, v, a]);
            }
        }
        8 => {
            if raw.len() < pixels {
                return Err("a cel is the wrong size".into());
            }
            let pal = file
                .palette
                .as_ref()
                .ok_or_else(|| "an indexed picture has no palette".to_string())?;
            for i in 0..pixels {
                let index = raw[i];
                let mut entry = pal.get(index as usize).copied().unwrap_or([0, 0, 0, 0]);
                if index == file.transparent {
                    entry[3] = 0;
                }
                rgba[i * 4..i * 4 + 4].copy_from_slice(&entry);
            }
        }
        _ => return Err("the colour depth is not usable".into()),
    }
    Ok(Image { w, h, rgba })
}

fn layer_factors(layers: &[Layer]) -> Vec<Option<u8>> {
    let mut stack: Vec<(i32, bool, u32)> = vec![(-1, true, 255)];
    let mut out = Vec::with_capacity(layers.len());
    for layer in layers {
        while stack.last().is_some_and(|top| top.0 >= layer.child_level) {
            stack.pop();
        }
        let (parent_vis, parent_op) = stack.last().map(|top| (top.1, top.2)).unwrap_or((true, 255));
        let visible = parent_vis && layer.visible && layer.kind != 2;
        let opacity = parent_op.saturating_mul(u32::from(layer.opacity)) / 255;
        if layer.kind == 1 {
            stack.push((layer.child_level, visible, opacity));
            out.push(None);
        } else if visible && layer.kind == 0 {
            out.push(Some(opacity.min(255) as u8));
        } else {
            out.push(None);
        }
    }
    out
}

struct Canvas<'a> {
    pixels: &'a mut [u8],
    width: u32,
    height: u32,
}

fn blit(
    dst: &mut Canvas<'_>,
    x: i32,
    y: i32,
    src: &[u8],
    sw: u32,
    sh: u32,
    cel_opacity: u8,
    layer_opacity: u8,
) {
    for sy in 0..sh {
        for sx in 0..sw {
            let dx = x + sx as i32;
            let dy = y + sy as i32;
            if dx < 0 || dy < 0 || dx as u32 >= dst.width || dy as u32 >= dst.height {
                continue;
            }
            let si = ((sy * sw + sx) * 4) as usize;
            if si + 4 > src.len() {
                return;
            }
            let mut pixel = [src[si], src[si + 1], src[si + 2], src[si + 3]];
            let a = u32::from(pixel[3]) * u32::from(cel_opacity) * u32::from(layer_opacity)
                / (255 * 255);
            pixel[3] = a.min(255) as u8;
            let di = ((dy as u32 * dst.width + dx as u32) * 4) as usize;
            if di + 4 <= dst.pixels.len() {
                over(&mut dst.pixels[di..di + 4], pixel);
            }
        }
    }
}

fn over(dst: &mut [u8], src: [u8; 4]) {
    let sa = u32::from(src[3]);
    if sa == 0 {
        return;
    }
    let da = u32::from(dst[3]);
    let inv = 255 - sa;
    let out_a = sa + da * inv / 255;
    if out_a == 0 {
        dst[..4].fill(0);
        return;
    }
    for i in 0..3 {
        let mixed = u32::from(src[i]) * sa + u32::from(dst[i]) * da * inv / 255;
        dst[i] = (mixed / out_a) as u8;
    }
    dst[3] = out_a as u8;
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn need(&self, n: usize) -> Result<(), String> {
        if self.pos.saturating_add(n) > self.data.len() {
            Err("the file is truncated".into())
        } else {
            Ok(())
        }
    }

    fn skip(&mut self, n: usize) -> Result<(), String> {
        self.need(n)?;
        self.pos += n;
        Ok(())
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8], String> {
        self.need(n)?;
        let out = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    fn rest(&self) -> &'a [u8] {
        &self.data[self.pos..]
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.bytes(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        let b = self.bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, String> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn i16(&mut self) -> Result<i16, String> {
        Ok(self.u16()? as i16)
    }

    fn i32(&mut self) -> Result<i32, String> {
        Ok(self.u32()? as i32)
    }

    fn string(&mut self) -> Result<String, String> {
        let n = self.u16()? as usize;
        if n > MAX_STRING {
            return Err("a name is too long".into());
        }
        let bytes = self.bytes(n)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| "a name is not text".to_string())
    }
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

fn finish(file: &mut FileData, stored: Vec<StoredFrame>) -> Result<(), String> {
    if file.depth == 8 && file.palette.is_none() {
        return Err("an indexed picture has no palette".into());
    }
    let mut frames = Vec::with_capacity(stored.len());
    for frame in stored {
        let mut cels = Vec::new();
        for cel in frame.cels {
            let (image, link) = match cel.stored {
                Stored::Raw(data, w, h) => (to_rgba(&data, w, h, file)?, None),
                Stored::Compressed(data, w, h) => {
                    let raw = inflate(&data, expected_len(w, h, file.depth)?)?;
                    (to_rgba(&raw, w, h, file)?, None)
                }
                Stored::Link(target) => (Image { w: 0, h: 0, rgba: Vec::new() }, Some(target)),
            };
            cels.push((
                Cel { layer: cel.layer, x: cel.x, y: cel.y, opacity: cel.opacity, image },
                link,
            ));
        }
        frames.push((frame.duration_ms, cels));
    }
    resolve_stored_links(&mut frames)?;
    file.frames = frames
        .into_iter()
        .map(|(duration_ms, cels)| RawFrame {
            duration_ms,
            cels: cels.into_iter().map(|(cel, _)| cel).collect(),
        })
        .collect();
    Ok(())
}

type LinkedCel = (Cel, Option<u16>);
type LinkedFrame = (u32, Vec<LinkedCel>);

fn resolve_stored_links(frames: &mut [LinkedFrame]) -> Result<(), String> {
    let n = frames.len();
    for _ in 0..n.saturating_add(1) {
        let mut jobs = Vec::new();
        for (fi, (_, cels)) in frames.iter().enumerate() {
            for (ci, (cel, link)) in cels.iter().enumerate() {
                if let Some(target) = *link {
                    jobs.push((fi, ci, target as usize, cel.layer));
                }
            }
        }
        if jobs.is_empty() {
            return Ok(());
        }
        let mut copies = Vec::new();
        for (fi, ci, target, layer) in jobs {
            if target >= n {
                return Err("a linked cel points outside the file".into());
            }
            let found = frames[target].1.iter().find_map(|(cel, link)| {
                (cel.layer == layer && link.is_none()).then(|| cel.image.clone())
            });
            if let Some(image) = found {
                copies.push((fi, ci, image));
            }
        }
        if copies.is_empty() {
            return Err("a linked cel has no picture".into());
        }
        for (fi, ci, image) in copies {
            frames[fi].1[ci].0.image = image;
            frames[fi].1[ci].1 = None;
        }
    }
    Err("a linked cel has no picture".into())
}
