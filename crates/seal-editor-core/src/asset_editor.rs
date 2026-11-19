//! The Asset Editor: the built-in editor for media and other project files.
//!
//! Opening a picture, effect, sound or text file from the Asset Browser shows it
//! in the Asset Editor panel. The open file is kept in a draft, so it is still open
//! after a restart. Every edit is written to the file at once; nothing waits for Save.

/// Re-exported for the Asset Editor panel, which sits above the runtime crates.
pub use seal_audio::Sound;
pub use seal_world::particles::{
    collect_draws, collect_draws_depth, mesh_renderer_primitive, EmitShape, Emitter, EmitterDef,
    FlipbookMode, MeshSampleMode, ParticleDraw, RendererMode, SphereVolume,
};
pub use seal_world::sound_container::{ContainerMode, SoundContainerDef};

use std::path::{Path, PathBuf};

use crate::command::CommandArgs;
use crate::error::CoreError;
use crate::session::EditorSession;

/// Draft holding the project-relative path of the file open in the Asset Editor.
pub const ASSET_DRAFT: &str = "asset-path";
/// Draft holding the picture zoom (1 = one screen pixel per image pixel, 0 = fit).
pub const ASSET_ZOOM_DRAFT: &str = "asset-zoom";
/// Draft holding "1" when the picture pixel grid is shown.
pub const ASSET_GRID_DRAFT: &str = "asset-grid";
/// Draft holding the sound preview volume, 0 to 1.
pub const ASSET_VOLUME_DRAFT: &str = "asset-volume";
/// Draft holding the clip picked in the animation editor.
pub const ASSET_CLIP_DRAFT: &str = "asset-clip";

/// What kind of editor a file opens in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetKind {
    Image,
    Sound,
    Effect,
    Text,
    /// A sprite animation (.frames): named clips of picture frames.
    Animation,
    /// A sound container (.soundset): several sounds picked on play.
    SoundSet,
    /// A string table (.strings): named lines for labels.
    Strings,
    /// A pixel animation file (.ase or .aseprite), shown read only.
    SpriteSource,
    /// A layered picture (`.psd`), shown read only.
    LayeredPicture,
    /// An in-game UI screen (.sealui), edited in the screen designer.
    Screen,
    /// An in-game UI theme (.sealtheme).
    Theme,
    /// A TrueType or OpenType font (.ttf, .otf), shown as a preview.
    Font,
    /// Game controls (.sealinput): actions and the keys and pad buttons on them.
    Input,
    /// A saved game (.sealsave).
    Save,
    /// A tile rule file (`.tiles`).
    TileSet,
    /// A Data Type definition (`.datatype`).
    DataType,
    /// A Data asset (`.data`).
    Data,
    /// A graph test script (`.sealtest`): play a scene, then check object state.
    Test,
    /// A composite texture (`.texture`): array, volume or cubemap.
    TextureComposite,
    /// A canvas shader (`.shade2d`).
    Material,
    /// A mesh asset (`.mesh`).
    Mesh,
    /// A file with no editor of its own; its size and type are shown.
    Other,
}

const TEXT_EXTS: &[&str] = &[
    "txt", "md", "json", "toml", "ftl", "csv", "ini", "cfg", "yaml", "yml", "xml", "log", "seq",
    "graph", "lua", "rhai", "glsl", "wgsl", "hlsl", "shader", "shade2d",
];

/// The editor a file name opens in.
pub fn asset_kind(name: &str) -> AssetKind {
    let ext = Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        e if crate::picture::is_extension(e) => AssetKind::Image,
        "wav" | "ogg" | "mp3" | "flac" => AssetKind::Sound,
        "fx" => AssetKind::Effect,
        "frames" => AssetKind::Animation,
        "soundset" => AssetKind::SoundSet,
        "strings" => AssetKind::Strings,
        "ase" | "aseprite" => AssetKind::SpriteSource,
        "psd" => AssetKind::LayeredPicture,
        "sealui" => AssetKind::Screen,
        "sealtheme" => AssetKind::Theme,
        "ttf" | "otf" => AssetKind::Font,
        "sealinput" => AssetKind::Input,
        "sealsave" => AssetKind::Save,
        "tiles" => AssetKind::TileSet,
        "datatype" => AssetKind::DataType,
        "data" => AssetKind::Data,
        "sealtest" => AssetKind::Test,
        "texture" => AssetKind::TextureComposite,
        "shade2d" => AssetKind::Material,
        "mesh" => AssetKind::Mesh,
        e if TEXT_EXTS.contains(&e) => AssetKind::Text,
        _ => AssetKind::Other,
    }
}

/// A sound preview the host should start or stop.
#[derive(Clone, Debug, PartialEq)]
pub enum AudioRequest {
    Play {
        path: PathBuf,
        volume: f32,
    },
    /// Preview every pick at once (layered containers).
    PlaySet {
        paths: Vec<(PathBuf, f32)>,
        rel: String,
    },
    Stop,
}

/// The sound preview that is playing: which file, when it started and how long it is.
#[derive(Clone, Debug)]
pub struct AudioPlaying {
    pub rel: String,
    pub started: std::time::Instant,
    pub seconds: f32,
}

impl AudioPlaying {
    /// Seconds played so far, or `None` once the sound has finished.
    pub fn position(&self) -> Option<f32> {
        let t = self.started.elapsed().as_secs_f32();
        (t < self.seconds).then_some(t)
    }
}

fn project_root(session: &EditorSession) -> Result<PathBuf, CoreError> {
    session
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .ok_or_else(|| CoreError::new("open a project first"))
}

fn checked_rel(rel: &str) -> Result<String, CoreError> {
    let rel = rel.trim().replace('\\', "/");
    if rel.is_empty() || rel.starts_with('/') || rel.split('/').any(|p| p == "..") {
        return Err(CoreError::new(format!("'{rel}' is not a path inside the project")));
    }
    Ok(rel)
}

/// The file open in the Asset Editor, as (project-relative path, full path).
pub fn open_asset(session: &EditorSession) -> Option<(String, PathBuf)> {
    let rel = session.draft(ASSET_DRAFT).filter(|r| !r.is_empty())?;
    let root = session.project.as_ref()?.root.clone();
    let full = root.join(&rel);
    full.is_file().then_some((rel, full))
}

/// Show a project file in the Asset Editor.
pub fn show_asset(session: &mut EditorSession, rel: &str) -> Result<(), CoreError> {
    let rel = checked_rel(rel)?;
    let root = project_root(session)?;
    if !root.join(&rel).is_file() {
        return Err(CoreError::new(format!("{rel} was not found")));
    }
    if session.audio_playing.as_ref().is_some_and(|p| p.rel != rel) {
        stop_sound(session);
    }
    session.set_draft(ASSET_DRAFT, &rel)?;
    remember_opened_text(session, &root, &rel);
    session.focus_panel("asset");
    if asset_kind(&rel) == AssetKind::SpriteSource {
        if let Some(root) = session.project.as_ref() {
            crate::aseprite::refresh_source(&root.root.join(&rel));
        }
    }
    if asset_kind(&rel) == AssetKind::LayeredPicture {
        if let Some(root) = session.project.as_ref() {
            crate::layered_picture::refresh_source(&root.root.join(&rel));
        }
    }
    Ok(())
}

fn remember_opened_text(session: &mut EditorSession, root: &Path, rel: &str) {
    let text_file = matches!(
        asset_kind(rel),
        AssetKind::Text | AssetKind::Effect | AssetKind::Screen | AssetKind::Theme
    );
    let same = session.asset_saved_text.as_ref().is_some_and(|(open, _)| open == rel);
    if text_file && same {
        return;
    }
    session.asset_saved_text = if text_file {
        Some((rel.to_string(), std::fs::read_to_string(root.join(rel)).unwrap_or_default()))
    } else {
        None
    };
}

/// `asset.open path=`: show a project file in the Asset Editor.
pub(crate) fn cmd_asset_open(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let rel = args.get("path").unwrap_or_default().to_string();
    show_asset(session, &rel)
}

fn open_effect(session: &EditorSession) -> Result<(String, PathBuf, EmitterDef), CoreError> {
    let (rel, full) = open_asset(session)
        .ok_or_else(|| CoreError::new("no effect is open in the Asset Editor"))?;
    if asset_kind(&rel) != AssetKind::Effect {
        return Err(CoreError::new("the open file is not an effect"));
    }
    let text = std::fs::read_to_string(&full)?;
    let def = EmitterDef::parse(&text).map_err(|e| CoreError::new(format!("{rel}: {e}")))?;
    Ok((rel, full, def))
}

/// Set one effect setting on a definition. `key` is a setting name, with `.x`/`.y`/`.z`
/// for vectors, `.start`/`.end` for size, and `.start.r` .. `.end.a` for colour.
pub fn set_effect_value(def: &mut EmitterDef, key: &str, value: f32) -> Result<(), CoreError> {
    set_effect_value_f32(def, key, value)
}

pub fn set_effect_value_f32(def: &mut EmitterDef, key: &str, value: f32) -> Result<(), CoreError> {
    if !value.is_finite() {
        return Err(CoreError::new("the value must be a number"));
    }
    let axis = |name: &str| match name {
        "x" => Some(0),
        "y" => Some(1),
        "z" => Some(2),
        _ => None,
    };
    let channel = |name: &str| match name {
        "r" => Some(0),
        "g" => Some(1),
        "b" => Some(2),
        "a" => Some(3),
        _ => None,
    };
    let parts: Vec<&str> = key.split('.').collect();
    match parts.as_slice() {
        ["rate"] => def.rate = value.max(0.0),
        ["lifetime"] => def.lifetime = value.max(0.0),
        ["spread"] => def.spread = value.max(0.0),
        ["max"] => def.max = value.max(0.0) as usize,
        ["seed"] => def.seed = value.max(1.0) as u32,
        ["velocity", a] if axis(a).is_some() => def.velocity[axis(a).unwrap_or(0)] = value,
        ["gravity", a] if axis(a).is_some() => def.gravity[axis(a).unwrap_or(0)] = value,
        ["size", "start"] => def.size.0 = value.max(0.0),
        ["size", "end"] => def.size.1 = value.max(0.0),
        ["color", end, c] if channel(c).is_some() && matches!(*end, "start" | "end") => {
            let i = channel(c).unwrap_or(0);
            let v = value.clamp(0.0, 1.0);
            if *end == "start" {
                def.color.0[i] = v;
            } else {
                def.color.1[i] = v;
            }
        }
        _ => {
            if crate::particle_fx_fields::set_effect_field(def, key, &value.to_string()).is_ok() {
                return Ok(());
            }
            return Err(CoreError::new(format!("unknown effect setting {key}")));
        }
    }
    Ok(())
}

fn load_effect_at(
    session: &EditorSession,
    rel: &str,
) -> Result<(PathBuf, EmitterDef, String), CoreError> {
    let rel = rel.trim().replace('\\', "/");
    if rel.is_empty() || rel.starts_with('/') || rel.split('/').any(|part| part == "..") {
        return Err(CoreError::new(format!("'{rel}' is not a path inside the project")));
    }
    let root = project_root(session)?;
    let full = root.join(&rel);
    if !full.is_file() {
        return Err(CoreError::new(format!("{rel} was not found")));
    }
    let previous = std::fs::read_to_string(&full)?;
    let def = EmitterDef::parse(&previous).map_err(|e| CoreError::new(format!("{rel}: {e}")))?;
    Ok((full, def, previous))
}

/// `asset.fx_set key= value=`: change one setting of the open effect and save it.
/// With `path=` set, edits that project file (Inspector or Asset Browser).
pub(crate) fn cmd_fx_set(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let key = args.get("key").ok_or_else(|| CoreError::new("choose an effect setting"))?;
    let raw = args.get("value").unwrap_or("").trim();
    let (full, mut def, previous) = match args.get("path") {
        Some(rel) => load_effect_at(session, rel)?,
        None => {
            let (_, full, def) = open_effect(session)?;
            let previous = std::fs::read_to_string(&full)?;
            (full, def, previous)
        }
    };
    if let Ok(v) = raw.parse::<f32>() {
        set_effect_value_f32(&mut def, key, v)?;
    } else {
        crate::particle_fx_fields::set_effect_field(&mut def, key, raw)?;
    }
    let next = def.to_text();
    if next != previous {
        crate::persist::atomic_write(&full, next.as_bytes())?;
        session.push_past(crate::models::Past::File {
            path: full.to_string_lossy().to_string(),
            previous,
            next,
        });
        session.future.clear();
    }
    Ok(())
}

/// `asset.fx_reset`: put the open effect back to the default settings.
pub(crate) fn cmd_fx_reset(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    let (_, full, _) = open_effect(session)?;
    crate::persist::atomic_write(&full, EmitterDef::default().to_text().as_bytes())?;
    Ok(())
}

/// `asset.fx_new path= [unique=yes]`: write a default particle effect and show it.
pub(crate) fn cmd_fx_new(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let mut rel = checked_rel(args.get("path").unwrap_or("fx/effect"))?;
    if !rel.ends_with(".fx") {
        rel.push_str(".fx");
    }
    let root = project_root(session)?;
    let mut full = root.join(&rel);
    if full.exists() {
        if args.get("unique") != Some("yes") {
            return Err(CoreError::new(format!("{rel} already exists")));
        }
        let stem = rel.trim_end_matches(".fx").to_string();
        let mut n = 2;
        while full.exists() {
            rel = format!("{stem}_{n}.fx");
            full = root.join(&rel);
            n += 1;
        }
    }
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::persist::atomic_write(&full, EmitterDef::default().to_text().as_bytes())?;
    show_asset(session, &rel)
}

// ---------------------------------------------------------------- animations

use seal_world::frames::{self, Clip, Frame, Socket, SpriteFrames};

fn open_frames(session: &EditorSession) -> Result<(String, PathBuf, SpriteFrames), CoreError> {
    let (rel, full) = open_asset(session)
        .ok_or_else(|| CoreError::new("no sprite animation is open in the Asset Editor"))?;
    if asset_kind(&rel) != AssetKind::Animation {
        return Err(CoreError::new("the open file is not a sprite animation"));
    }
    let lib = SpriteFrames::load(&full).map_err(|e| CoreError::new(format!("{rel}: {e}")))?;
    Ok((rel, full, lib))
}

fn save_frames(full: &Path, lib: &SpriteFrames) -> Result<(), CoreError> {
    let text = format!("{}{}", source_header(full), lib.to_text());
    crate::persist::atomic_write(full, text.as_bytes())?;
    frames::invalidate(full);
    Ok(())
}

/// Lines that tie a generated sprite animation back to its pixel source.
/// Kept across edits so a newer source file can rebuild the picture and the animation.
fn source_header(path: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(path) else {
        return String::new();
    };
    let mut out = String::new();
    for line in text.lines() {
        if line.starts_with("# generated-from ")
            || line.starts_with("# sheet ")
            || line.starts_with("# timing ")
            || line.starts_with("# slices ")
        {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// The clip picked in the animation editor: the drafted one, else the first.
pub fn picked_clip(session: &EditorSession, lib: &SpriteFrames) -> Option<String> {
    let want = session.draft(ASSET_CLIP_DRAFT).unwrap_or_default();
    lib.clip_or_first(&want).map(|c| c.name.clone())
}

fn clip_arg<'a>(lib: &'a mut SpriteFrames, args: &CommandArgs) -> Result<&'a mut Clip, CoreError> {
    let name = args.get("clip").unwrap_or_default().to_string();
    lib.clip_mut(&name).ok_or_else(|| CoreError::new(format!("there is no clip called '{name}'")))
}

fn clean_clip_name(name: &str) -> Result<String, CoreError> {
    let name = name.trim();
    if name.is_empty() || name.contains(char::is_whitespace) || name.contains('#') {
        return Err(CoreError::new("a clip name is one word with no spaces or #"));
    }
    Ok(name.to_string())
}

/// Picture path as written in the .frames file: relative to the file's folder.
fn frame_image(anim_rel: &str, image_rel: &str) -> Result<String, CoreError> {
    let image_rel = checked_rel(image_rel)?;
    let dir = anim_rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let anim_parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    let img_parts: Vec<&str> = image_rel.split('/').collect();
    let common = anim_parts.iter().zip(&img_parts).take_while(|(a, b)| a == b).count();
    let mut out: Vec<&str> = vec![".."; anim_parts.len() - common];
    out.extend(&img_parts[common..]);
    Ok(out.join("/"))
}

/// Read a picture as RGBA8 pixels, including its import Max Size.
pub(crate) fn decode_png(path: &Path) -> Option<(Vec<u8>, u32, u32)> {
    crate::picture_import::load(path)
        .ok()
        .map(|picture| (picture.rgba, picture.width, picture.height))
}

/// `asset.frames_new path= [image=] [mode=grid|auto] [cols=] [rows=] [unique=yes]`:
/// create a sprite animation file and open it. With a picture, its first clip
/// holds the sheet's cells (a grid, or the separate pictures found on it).
/// `unique=yes` picks a free name instead of refusing an existing file.
pub(crate) fn cmd_frames_new(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let mut rel = checked_rel(args.get("path").unwrap_or_default())?;
    if !frames::is_frames_path(&rel) {
        rel.push_str(".frames");
    }
    let root = project_root(session)?;
    let mut full = root.join(&rel);
    if full.exists() {
        if args.get("unique") != Some("yes") {
            return Err(CoreError::new(format!("{rel} already exists")));
        }
        let stem = rel.trim_end_matches(".frames").to_string();
        let mut n = 2;
        while full.exists() {
            rel = format!("{stem}_{n}.frames");
            full = root.join(&rel);
            n += 1;
        }
    }
    let mut clip = Clip::new("idle");
    if let Some(image) = args.get("image").filter(|i| !i.trim().is_empty()) {
        let pic = frame_image(&rel, image)?;
        let image_rel = checked_rel(image)?;
        if args.get("mode") == Some("auto") {
            let (rgba, w, h) = decode_png(&root.join(&image_rel)).ok_or_else(|| {
                CoreError::new(format!(
                    "{image_rel}: automatic cutting needs a picture with transparency"
                ))
            })?;
            clip.frames = frames::auto_frames(&pic, &rgba, w, h);
            if clip.frames.is_empty() {
                clip.frames.push(Frame::whole(&pic));
            }
        } else {
            let cols =
                args.get("cols").and_then(|c| c.trim().parse().ok()).unwrap_or(1u32).clamp(1, 256);
            let rows =
                args.get("rows").and_then(|c| c.trim().parse().ok()).unwrap_or(1u32).clamp(1, 256);
            clip.frames = frames::grid_frames(&pic, cols, rows, 0, cols * rows);
        }
    }
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)?;
    }
    save_frames(&full, &SpriteFrames { clips: vec![clip] })?;
    show_asset(session, &rel)?;
    session.set_draft(ASSET_CLIP_DRAFT, "idle")
}

/// `asset.frames_clip_add [name=]`: add an empty clip and pick it.
pub(crate) fn cmd_frames_clip_add(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let (_, full, mut lib) = open_frames(session)?;
    let base = clean_clip_name(args.get("name").unwrap_or("clip"))?;
    let name = lib.unused_name(&base).replace(' ', "_");
    lib.clips.push(Clip::new(&name));
    save_frames(&full, &lib)?;
    session.set_draft(ASSET_CLIP_DRAFT, &name)
}

/// `asset.frames_clip_remove clip=`
pub(crate) fn cmd_frames_clip_remove(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let (_, full, mut lib) = open_frames(session)?;
    let name = clip_arg(&mut lib, args)?.name.clone();
    lib.clips.retain(|c| c.name != name);
    save_frames(&full, &lib)?;
    let next = lib.clips.first().map(|c| c.name.clone()).unwrap_or_default();
    session.set_draft(ASSET_CLIP_DRAFT, &next)
}

/// `asset.frames_clip_set clip= [name=] [fps=] [loop=yes|no]`
pub(crate) fn cmd_frames_clip_set(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let (_, full, mut lib) = open_frames(session)?;
    let rename = match args.get("name") {
        Some(n) => {
            let n = clean_clip_name(n)?;
            let current = args.get("clip").unwrap_or_default();
            if n != current && lib.clip(&n).is_some() {
                return Err(CoreError::new(format!("there is already a clip called '{n}'")));
            }
            Some(n)
        }
        None => None,
    };
    let clip = clip_arg(&mut lib, args)?;
    if let Some(fps) = args.get("fps") {
        let fps: f32 = fps.trim().parse().map_err(|_| CoreError::new("speed must be a number"))?;
        if !fps.is_finite() {
            return Err(CoreError::new("speed must be a number"));
        }
        clip.fps = fps.clamp(0.0, 120.0);
    }
    if let Some(l) = args.get("loop") {
        clip.looping = matches!(l, "yes" | "1" | "true");
    }
    let new_name = rename.clone();
    if let Some(n) = rename {
        clip.name = n;
    }
    save_frames(&full, &lib)?;
    if let Some(n) = new_name {
        session.set_draft(ASSET_CLIP_DRAFT, &n)?;
    }
    Ok(())
}

/// `asset.frames_slice clip= image= mode=grid|auto|whole|islands [cols=] [rows=] [first=] [count=] [min=] [pad=] [append=yes]`:
/// fill a clip with frames cut from a picture.
pub(crate) fn cmd_frames_slice(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let (rel, full, mut lib) = open_frames(session)?;
    let image_rel = checked_rel(args.get("image").unwrap_or_default())
        .map_err(|_| CoreError::new("choose a picture to cut frames from"))?;
    let root = project_root(session)?;
    let image_full = root.join(&image_rel);
    if !image_full.is_file() {
        return Err(CoreError::new(format!("{image_rel} was not found")));
    }
    let pic = frame_image(&rel, &image_rel)?;
    let num = |k: &str, d: u32| args.get(k).and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(d);
    let new_frames = match args.get("mode").unwrap_or("grid") {
        "whole" => vec![Frame::whole(&pic)],
        "auto" => {
            let (rgba, w, h) = decode_png(&image_full).ok_or_else(|| {
                CoreError::new(format!(
                    "{image_rel}: automatic cutting needs a picture with transparency"
                ))
            })?;
            let f = frames::auto_frames(&pic, &rgba, w, h);
            if f.is_empty() {
                return Err(CoreError::new("no separate pictures were found on that sheet"));
            }
            f
        }
        "islands" => {
            let (rgba, w, h) = decode_png(&image_full).ok_or_else(|| {
                CoreError::new(format!(
                    "{image_rel}: island cutting needs a PNG picture with transparency"
                ))
            })?;
            let f =
                seal_world::sheet::island_frames(&pic, &rgba, w, h, num("min", 1), num("pad", 0));
            if f.is_empty() {
                return Err(CoreError::new("no separate pictures were found on that sheet"));
            }
            f
        }
        _ => {
            let (cols, rows) = (num("cols", 1).clamp(1, 256), num("rows", 1).clamp(1, 256));
            frames::grid_frames(&pic, cols, rows, num("first", 0), num("count", cols * rows))
        }
    };
    let append = args.get("append").is_some_and(|a| a == "yes");
    let clip = clip_arg(&mut lib, args)?;
    if !append {
        clip.frames.clear();
    }
    clip.frames.extend(new_frames);
    save_frames(&full, &lib)
}

/// `asset.frames_frame clip= index= action=remove|left|right|copy|set|outline|physics`:
/// edit one frame. `set` reads `pivot`, `border`, `outline`, `physics` and `event` when present.
pub(crate) fn cmd_frames_frame(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let (rel, full, mut lib) = open_frames(session)?;
    let index: usize = args
        .get("index")
        .and_then(|i| i.trim().parse().ok())
        .ok_or_else(|| CoreError::new("choose a frame"))?;
    let clip = clip_arg(&mut lib, args)?;
    if index >= clip.frames.len() {
        return Err(CoreError::new("that frame does not exist"));
    }
    match args.get("action").unwrap_or_default() {
        "remove" => {
            clip.frames.remove(index);
        }
        "left" if index > 0 => clip.frames.swap(index, index - 1),
        "right" if index + 1 < clip.frames.len() => clip.frames.swap(index, index + 1),
        "copy" => {
            let f = clip.frames[index].clone();
            clip.frames.insert(index + 1, f);
        }
        "set" => {
            let frame = &mut clip.frames[index];
            if let Some(pivot) = args.get("pivot") {
                frame.pivot = pair2(pivot)?;
            }
            if let Some(border) = args.get("border") {
                frame.border = pair4(border)?;
            }
            if let Some(outline) = args.get("outline") {
                frame.outline = point_list(outline)?;
            }
            if let Some(physics) = args.get("physics") {
                frame.physics = point_list(physics)?;
            }
            if let Some(event) = args.get("event") {
                frame.event = clean_event(event);
            }
        }
        "outline" | "physics" => {
            let image = clip.frames[index].image.clone();
            let uv = clip.frames[index].uv;
            let picture = frame_file(session, &rel, &image)?;
            let (rgba, w, h) = decode_png(&picture).ok_or_else(|| {
                CoreError::new("the frame picture needs to be a PNG with transparency")
            })?;
            let tight = seal_world::sheet::tight_outline(&rgba, w, h, uv);
            if args.get("action") == Some("outline") {
                clip.frames[index].outline = tight;
            } else {
                let src = if clip.frames[index].outline.len() >= 3 {
                    clip.frames[index].outline.clone()
                } else {
                    tight
                };
                clip.frames[index].physics = seal_world::sheet::physics_outline(&src);
            }
        }
        "left" | "right" => return Ok(()),
        other => return Err(CoreError::new(format!("unknown frame action '{other}'"))),
    }
    save_frames(&full, &lib)
}

/// A frame event name. Spaces stay. A `#` would be read as a comment, so it
/// is dropped, and an empty name clears the event.
fn clean_event(name: &str) -> String {
    name.trim()
        .chars()
        .filter(|c| *c != '#' && !c.is_control())
        .collect::<String>()
        .trim()
        .to_string()
}

fn socket_word(name: &str) -> Result<String, CoreError> {
    let name = name.trim();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(CoreError::new("a socket name is one word of letters, digits, _ or -"));
    }
    Ok(name.to_string())
}

/// `asset.frames_socket clip= action=add|remove|move|rename name= [index= u= v= to=]`.
/// Add puts the name on every frame of the clip that lacks it, at the middle.
pub(crate) fn cmd_frames_socket(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let (_, full, mut lib) = open_frames(session)?;
    let name = socket_word(args.get("name").unwrap_or("socket"))?;
    let clip = clip_arg(&mut lib, args)?;
    match args.get("action").unwrap_or("move") {
        "add" => {
            let mut name = name;
            let mut n = 2;
            while clip.frames.iter().any(|f| f.sockets.iter().any(|s| s.name == name)) {
                name = format!("socket{n}");
                n += 1;
            }
            for frame in &mut clip.frames {
                frame.sockets.push(Socket { name: name.clone(), at: [0.5, 0.5] });
            }
        }
        "remove" => {
            for frame in &mut clip.frames {
                frame.sockets.retain(|s| s.name != name);
            }
        }
        "rename" => {
            let to = socket_word(args.get("to").unwrap_or(""))?;
            for frame in &mut clip.frames {
                if frame.sockets.iter().any(|s| s.name == to)
                    && frame.sockets.iter().any(|s| s.name == name)
                {
                    return Err(CoreError::new(format!(
                        "this frame already has a socket called '{to}'"
                    )));
                }
                if let Some(socket) = frame.sockets.iter_mut().find(|s| s.name == name) {
                    socket.name = to.clone();
                }
            }
        }
        "move" => {
            let index: usize = args
                .get("index")
                .and_then(|i| i.trim().parse().ok())
                .ok_or_else(|| CoreError::new("choose a frame"))?;
            let u: f32 = args
                .get("u")
                .and_then(|v| v.parse().ok())
                .filter(|n: &f32| n.is_finite())
                .ok_or_else(|| CoreError::new("u must be a number"))?;
            let v: f32 = args
                .get("v")
                .and_then(|n| n.parse().ok())
                .filter(|n: &f32| n.is_finite())
                .ok_or_else(|| CoreError::new("v must be a number"))?;
            let frame = clip
                .frames
                .get_mut(index)
                .ok_or_else(|| CoreError::new("that frame does not exist"))?;
            let socket = frame.sockets.iter_mut().find(|s| s.name == name).ok_or_else(|| {
                CoreError::new(format!("this frame has no socket called '{name}'"))
            })?;
            socket.at = [u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)];
        }
        other => return Err(CoreError::new(format!("unknown socket action '{other}'"))),
    }
    save_frames(&full, &lib)
}

fn pair2(text: &str) -> Result<[f32; 2], CoreError> {
    let v = csv(text, 2)?;
    Ok([v[0], v[1]])
}

fn pair4(text: &str) -> Result<[f32; 4], CoreError> {
    let v = csv(text, 4)?;
    Ok([v[0], v[1], v[2], v[3]])
}

fn point_list(text: &str) -> Result<Vec<[f32; 2]>, CoreError> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    text.split(';')
        .map(|part| {
            let v = csv(part, 2)?;
            Ok([v[0], v[1]])
        })
        .collect()
}

fn csv(text: &str, count: usize) -> Result<Vec<f32>, CoreError> {
    let parts: Vec<&str> = text.split(',').map(str::trim).collect();
    if parts.len() != count || parts.iter().any(|p| p.is_empty()) {
        return Err(CoreError::new("expected a list of numbers"));
    }
    parts
        .iter()
        .map(|p| {
            p.parse::<f32>()
                .ok()
                .filter(|n| n.is_finite())
                .ok_or_else(|| CoreError::new("expected a list of numbers"))
        })
        .collect()
}

fn frame_file(
    session: &EditorSession,
    anim_rel: &str,
    image: &str,
) -> Result<std::path::PathBuf, CoreError> {
    let root = project_root(session)?;
    let anim = root.join(anim_rel);
    let dir = anim.parent().unwrap_or(root.as_path());
    Ok(dir.join(image))
}

/// `asset.text_save text=`: replace the open text file's contents.
pub(crate) fn cmd_text_save(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let (rel, full) =
        open_asset(session).ok_or_else(|| CoreError::new("no file is open in the Asset Editor"))?;
    if !matches!(
        asset_kind(&rel),
        AssetKind::Text | AssetKind::Effect | AssetKind::Screen | AssetKind::Theme
    ) {
        return Err(CoreError::new("the open file is not text"));
    }
    let text = args.get("text").unwrap_or_default();
    crate::persist::atomic_write(&full, text.as_bytes())?;
    Ok(())
}

/// Length in seconds of a WAV file, or `None` when it cannot be read.
pub fn sound_seconds(path: &Path) -> Option<f32> {
    let bytes = std::fs::read(path).ok()?;
    let sound = seal_audio::Sound::from_bytes(&bytes, Some(path)).ok()?;
    Some(sound.frames() as f32 / sound.sample_rate.max(1) as f32)
}

/// `asset.sound_play`: play the open sound through the speakers.
pub(crate) fn cmd_sound_play(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    let (rel, full) = open_asset(session)
        .ok_or_else(|| CoreError::new("no sound is open in the Asset Editor"))?;
    if asset_kind(&rel) != AssetKind::Sound {
        return Err(CoreError::new("the open file is not a sound"));
    }
    let seconds = sound_seconds(&full).ok_or_else(|| {
        CoreError::new(
            session.messages.format("asset-sound-unsupported", &[("name", rel.as_str())]),
        )
    })?;
    let volume = session
        .draft(ASSET_VOLUME_DRAFT)
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.8)
        .clamp(0.0, 1.0);
    session.audio_request = Some(AudioRequest::Play { path: full, volume });
    session.audio_playing = Some(AudioPlaying { rel, started: std::time::Instant::now(), seconds });
    Ok(())
}

fn stop_sound(session: &mut EditorSession) {
    session.audio_request = Some(AudioRequest::Stop);
    session.audio_playing = None;
}

/// `asset.sound_stop`: stop the sound preview.
pub(crate) fn cmd_sound_stop(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    stop_sound(session);
    Ok(())
}

/// `asset.close`: close the file in the Asset Editor.
pub(crate) fn cmd_asset_close(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    stop_sound(session);
    session.asset_saved_text = None;
    session.set_draft(ASSET_DRAFT, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_kinds_pick_their_editor() {
        assert_eq!(asset_kind("sprites/Hero.PNG"), AssetKind::Image);
        assert_eq!(asset_kind("a.jpeg"), AssetKind::Image);
        assert_eq!(asset_kind("a.bmp"), AssetKind::Image);
        assert_eq!(asset_kind("a.gif"), AssetKind::Image);
        assert_eq!(asset_kind("a.tga"), AssetKind::Image);
        assert_eq!(asset_kind("a.webp"), AssetKind::Image);
        assert_eq!(asset_kind("sky.hdr"), AssetKind::Image);
        assert_eq!(asset_kind("sounds/jump.wav"), AssetKind::Sound);
        assert_eq!(asset_kind("fx/sparks.fx"), AssetKind::Effect);
        assert_eq!(asset_kind("notes.md"), AssetKind::Text);
        assert_eq!(asset_kind("data.bin"), AssetKind::Other);
        assert_eq!(asset_kind("anims/hero.frames"), AssetKind::Animation);
        assert_eq!(asset_kind("sounds/footsteps.soundset"), AssetKind::SoundSet);
        assert_eq!(asset_kind("sprites/hero.aseprite"), AssetKind::SpriteSource);
        assert_eq!(asset_kind("sprites/hero.ASE"), AssetKind::SpriteSource);
        assert_eq!(asset_kind("tests/holds.sealtest"), AssetKind::Test);
        assert_eq!(asset_kind("sprites/hero.psd"), AssetKind::LayeredPicture);
        assert_eq!(asset_kind("sprites/hero.PSD"), AssetKind::LayeredPicture);
    }

    #[test]
    fn frame_pictures_are_written_relative_to_the_animation_file() {
        assert_eq!(frame_image("hero.frames", "sprites/hero.png").unwrap(), "sprites/hero.png");
        assert_eq!(frame_image("anims/hero.frames", "anims/hero.png").unwrap(), "hero.png");
        assert_eq!(
            frame_image("anims/p/hero.frames", "sprites/hero.png").unwrap(),
            "../../sprites/hero.png"
        );
        assert!(frame_image("a.frames", "../x.png").is_err());
    }

    #[test]
    fn effect_settings_change_one_value() {
        let mut d = EmitterDef::default();
        set_effect_value(&mut d, "velocity.y", 4.0).unwrap();
        set_effect_value(&mut d, "color.end.a", 2.0).unwrap();
        set_effect_value(&mut d, "size.start", -1.0).unwrap();
        assert_eq!(d.velocity[1], 4.0);
        assert_eq!(d.color.1[3], 1.0);
        assert_eq!(d.size.0, 0.0);
        assert!(set_effect_value(&mut d, "colour", 1.0).is_err());
        assert!(set_effect_value(&mut d, "rate", f32::NAN).is_err());
    }
}
