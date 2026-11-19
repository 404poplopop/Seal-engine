//! Named import presets, per-file import settings, folder rules and defaults.
//!
//! Each imported picture, sound or font keeps a sidecar `{rel}.import.seal`.
//! Named presets and project rules live in `import-presets.seal` at the project root.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use seal_sealtext::{parse, SealValue};

use crate::asset_browser::kind_of;
use crate::asset_editor::asset_kind;
use crate::command::CommandArgs;
use crate::error::CoreError;
use crate::models::Past;
use crate::persist::atomic_write;
use crate::session::EditorSession;

pub const BOOK_FILE: &str = "import-presets.seal";
pub const BOOK_SCHEMA: u64 = 1;
pub const SIDECAR_SCHEMA: u64 = 1;

/// Asset kind for import settings (matches browser filter names where possible).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ImportKind {
    Picture,
    Sound,
    Font,
}

impl ImportKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::Picture => "picture",
            Self::Sound => "sound",
            Self::Font => "font",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "picture" => Some(Self::Picture),
            "sound" => Some(Self::Sound),
            "font" => Some(Self::Font),
            _ => None,
        }
    }

    pub fn from_asset_path(rel: &str) -> Option<Self> {
        if kind_of(rel, false) == "font" {
            return Some(Self::Font);
        }
        match asset_kind(rel) {
            crate::asset_editor::AssetKind::Image => Some(Self::Picture),
            crate::asset_editor::AssetKind::Sound => Some(Self::Sound),
            crate::asset_editor::AssetKind::Font => Some(Self::Font),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PictureSettings {
    /// `nearest` or `linear`.
    pub filter: String,
    /// Picture pixels per world unit. `0` keeps the object's scale as its size,
    /// which is how every picture was drawn before this setting existed.
    pub pixels_per_unit: f32,
    /// Longest side after import. `0` keeps the file's size. Other values are powers of two.
    pub max_size: u32,
    /// Sample a smaller mip when the picture is drawn below half size.
    pub mipmaps: bool,
    /// `srgb` or `linear`.
    pub colour_space: String,
    /// `clamp`, `repeat` or `mirror`.
    pub wrap: String,
    /// `single` or `multiple` (a horizontal sheet of square cells).
    pub sprite_mode: String,
    /// Named pivot, or `custom`.
    pub pivot: String,
    pub pivot_x: f32,
    pub pivot_y: f32,
    /// Longer side, in pixels, when this picture is SVG. `256` is the default.
    pub svg_size: u32,
    /// `none`, `lossless`, `low`, `medium` or `high`. Applied when the game is exported.
    pub compression: String,
    /// `full` or `tight`.
    pub mesh: String,
}

impl Default for PictureSettings {
    fn default() -> Self {
        Self {
            filter: "nearest".into(),
            pixels_per_unit: 0.0,
            max_size: 0,
            mipmaps: false,
            colour_space: "srgb".into(),
            wrap: "clamp".into(),
            sprite_mode: "single".into(),
            pivot: "center".into(),
            pivot_x: 0.5,
            pivot_y: 0.5,
            svg_size: 256,
            compression: "none".into(),
            mesh: "full".into(),
        }
    }
}

pub const PICTURE_MAX_SIZES: &[u32] = seal_render::picture::MAX_SIZES;

const PIVOTS: &[&str] = &[
    "center",
    "top_left",
    "top",
    "top_right",
    "left",
    "right",
    "bottom_left",
    "bottom",
    "bottom_right",
    "custom",
];

/// Pivot from picture import settings; centre when unset.
pub fn picture_pivot(path: &std::path::Path) -> [f32; 2] {
    picture_look(path).pivot
}

#[derive(Clone, Debug, PartialEq)]
pub struct SoundSettings {
    /// Peak-normalise the samples when the sound is loaded.
    pub normalize: bool,
    /// Export sample format: pcm, adpcm or vorbis.
    pub compression: String,
    /// Vorbis quality step from 0 (small) to 10 (best).
    pub vorbis_quality: u8,
    /// memory keeps decoded samples; stream reads from disk each play.
    pub load_type: String,
}

impl Default for SoundSettings {
    fn default() -> Self {
        Self {
            normalize: false,
            compression: "pcm".into(),
            vorbis_quality: 5,
            load_type: "memory".into(),
        }
    }
}

/// Fonts have no import fields: the text drawer reads the font file as it is.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FontSettings {}

#[derive(Clone, Debug, PartialEq)]
pub enum ImportSettings {
    Picture(PictureSettings),
    Sound(SoundSettings),
    Font(FontSettings),
}

impl ImportSettings {
    pub fn kind(&self) -> ImportKind {
        match self {
            Self::Picture(_) => ImportKind::Picture,
            Self::Sound(_) => ImportKind::Sound,
            Self::Font(_) => ImportKind::Font,
        }
    }

    pub fn default_for(kind: ImportKind) -> Self {
        match kind {
            ImportKind::Picture => Self::Picture(PictureSettings::default()),
            ImportKind::Sound => Self::Sound(SoundSettings::default()),
            ImportKind::Font => Self::Font(FontSettings::default()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImportPreset {
    pub name: String,
    pub kind: ImportKind,
    pub builtin: bool,
    pub settings: ImportSettings,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderRule {
    pub path: String,
    pub kind: ImportKind,
    pub preset: String,
}

#[derive(Clone, Debug, Default)]
pub struct ImportPresetBook {
    pub presets: Vec<ImportPreset>,
    pub defaults: BTreeMap<String, String>,
    pub folder_rules: Vec<FolderRule>,
    pub atlas_platform_overrides:
        BTreeMap<crate::import_platform::ImportPlatform, crate::import_platform::AtlasPlatformOverride>,
    pub load_warnings: Vec<String>,
}

impl ImportPresetBook {
    pub fn with_builtins() -> Self {
        let mut book = Self::default();
        book.presets = builtin_presets();
        book
    }

    pub fn preset_by_name(&self, name: &str) -> Option<&ImportPreset> {
        self.presets.iter().find(|p| p.name == name)
    }

    pub fn presets_for_kind(&self, kind: ImportKind) -> Vec<&ImportPreset> {
        self.presets.iter().filter(|p| p.kind == kind).collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FileImportState {
    pub settings: ImportSettings,
    /// When true, folder-rule and default-preset changes do not overwrite this file.
    pub customized: bool,
    pub platform_overrides: crate::import_platform::PlatformOverrideMap,
}

pub fn builtin_presets() -> Vec<ImportPreset> {
    vec![
        ImportPreset {
            name: "Pixel Art".into(),
            kind: ImportKind::Picture,
            builtin: true,
            settings: ImportSettings::Picture(PictureSettings {
                filter: "nearest".into(),
                ..PictureSettings::default()
            }),
        },
        ImportPreset {
            name: "Smooth".into(),
            kind: ImportKind::Picture,
            builtin: true,
            settings: ImportSettings::Picture(PictureSettings {
                filter: "linear".into(),
                ..PictureSettings::default()
            }),
        },
        ImportPreset {
            name: "Music".into(),
            kind: ImportKind::Sound,
            builtin: true,
            settings: ImportSettings::Sound(SoundSettings { normalize: false, ..Default::default() }),
        },
        ImportPreset {
            name: "Effect".into(),
            kind: ImportKind::Sound,
            builtin: true,
            settings: ImportSettings::Sound(SoundSettings { normalize: true, ..Default::default() }),
        },
    ]
}

pub fn sidecar_path(root: &Path, rel: &str) -> PathBuf {
    root.join(format!("{rel}.import.seal"))
}

pub fn book_path(root: &Path) -> PathBuf {
    root.join(BOOK_FILE)
}

pub fn load_book(root: &Path) -> ImportPresetBook {
    let mut book = ImportPresetBook::with_builtins();
    let path = book_path(root);
    let Ok(text) = fs::read_to_string(&path) else {
        return book;
    };
    let Ok(value) = parse(&text) else {
        book.load_warnings.push(format!("{} could not be parsed", BOOK_FILE));
        return book;
    };
    merge_book_file(&mut book, &value);
    book
}

fn merge_book_file(book: &mut ImportPresetBook, value: &SealValue) {
    let Some(map) = value.as_map() else {
        book.load_warnings.push(format!("{} must be a map", BOOK_FILE));
        return;
    };
    if let Some(SealValue::Map(defaults)) = value.get("defaults") {
        for (key, item) in &defaults.entries {
            if let SealValue::String(name) = item {
                book.defaults.insert(key.clone(), name.clone());
            }
        }
    }
    if let Some(SealValue::Array(rules)) = value.get("folder_rules") {
        for item in rules {
            if let Some(rule) = parse_folder_rule(item) {
                book.folder_rules.push(rule);
            } else {
                book.load_warnings.push("skipped a folder rule with bad fields".into());
            }
        }
    }
    if let Some(SealValue::Array(entries)) = value.get("presets") {
        for item in entries {
            match parse_user_preset(item) {
                Ok(preset) => book.presets.push(preset),
                Err(msg) => book.load_warnings.push(msg),
            }
        }
    }
    book.atlas_platform_overrides = crate::import_platform::parse_atlas_platform_overrides(value);
    let _ = map;
}

fn parse_folder_rule(value: &SealValue) -> Option<FolderRule> {
    let path = match value.get("path")? {
        SealValue::String(text) => text.clone(),
        _ => return None,
    };
    let kind = match value.get("kind")? {
        SealValue::String(text) => ImportKind::from_id(text)?,
        _ => return None,
    };
    let preset = match value.get("preset")? {
        SealValue::String(text) => text.clone(),
        _ => return None,
    };
    Some(FolderRule { path: normalize_folder(&path), kind, preset })
}

fn parse_user_preset(value: &SealValue) -> Result<ImportPreset, String> {
    let map = value.as_map().ok_or_else(|| "preset entry is not a map".to_string())?;
    let name = match value.get("name") {
        Some(SealValue::String(text)) if !text.is_empty() => text.clone(),
        _ => return Err("skipped a preset with no name".into()),
    };
    let kind = match value.get("kind") {
        Some(SealValue::String(text)) => ImportKind::from_id(text)
            .ok_or_else(|| format!("skipped preset '{name}' with unknown kind"))?,
        _ => return Err(format!("skipped preset '{name}' with no kind")),
    };
    let settings = read_settings_map(kind, map)?;
    Ok(ImportPreset { name, kind, builtin: false, settings })
}

pub fn save_book(root: &Path, book: &ImportPresetBook) -> Result<(), CoreError> {
    let mut map: Vec<(String, SealValue)> =
        vec![("schema_version".into(), SealValue::Number(BOOK_SCHEMA.to_string()))];
    let defaults: Vec<(String, SealValue)> =
        book.defaults.iter().map(|(k, v)| (k.clone(), SealValue::String(v.clone()))).collect();
    map.push(("defaults".into(), SealValue::Map(defaults.into())));
    let rules: Vec<SealValue> = book
        .folder_rules
        .iter()
        .map(|rule| {
            SealValue::Map(
                vec![
                    ("path".into(), SealValue::String(rule.path.clone())),
                    ("kind".into(), SealValue::String(rule.kind.id().into())),
                    ("preset".into(), SealValue::String(rule.preset.clone())),
                ]
                .into(),
            )
        })
        .collect();
    map.push(("folder_rules".into(), SealValue::Array(rules)));
    let user: Vec<SealValue> =
        book.presets.iter().filter(|p| !p.builtin).map(preset_to_value).collect();
    map.push(("presets".into(), SealValue::Array(user)));
    if !book.atlas_platform_overrides.is_empty() {
        map.push((
            "atlas_platform_overrides".into(),
            crate::import_platform::atlas_platform_overrides_to_value(&book.atlas_platform_overrides),
        ));
    }
    atomic_write(&book_path(root), SealValue::Map(map.into()).to_text().as_bytes())
}

fn preset_to_value(preset: &ImportPreset) -> SealValue {
    let mut fields = vec![
        ("name".into(), SealValue::String(preset.name.clone())),
        ("kind".into(), SealValue::String(preset.kind.id().into())),
    ];
    fields.extend(settings_fields(&preset.settings));
    SealValue::Map(fields.into())
}

fn settings_fields(settings: &ImportSettings) -> Vec<(String, SealValue)> {
    match settings {
        ImportSettings::Picture(p) => vec![
            ("filter".into(), SealValue::String(p.filter.clone())),
            ("pixels_per_unit".into(), SealValue::Number(format_num(p.pixels_per_unit))),
            ("max_size".into(), SealValue::Number(p.max_size.to_string())),
            ("mipmaps".into(), SealValue::Bool(p.mipmaps)),
            ("colour_space".into(), SealValue::String(p.colour_space.clone())),
            ("wrap".into(), SealValue::String(p.wrap.clone())),
            ("sprite_mode".into(), SealValue::String(p.sprite_mode.clone())),
            ("pivot".into(), SealValue::String(p.pivot.clone())),
            ("pivot_x".into(), SealValue::Number(format_num(p.pivot_x))),
            ("pivot_y".into(), SealValue::Number(format_num(p.pivot_y))),
            ("svg_size".into(), SealValue::Number(p.svg_size.to_string())),
            ("compression".into(), SealValue::String(p.compression.clone())),
            ("mesh".into(), SealValue::String(p.mesh.clone())),
        ],
        ImportSettings::Sound(s) => vec![
            ("normalize".into(), SealValue::Bool(s.normalize)),
            ("compression".into(), SealValue::String(s.compression.clone())),
            ("vorbis_quality".into(), SealValue::Number(s.vorbis_quality.to_string())),
            ("load_type".into(), SealValue::String(s.load_type.clone())),
        ],
        ImportSettings::Font(_) => Vec::new(),
    }
}

pub fn load_file_state(root: &Path, rel: &str, kind: ImportKind) -> FileImportState {
    let path = sidecar_path(root, rel);
    let Ok(text) = fs::read_to_string(&path) else {
        return FileImportState {
            settings: ImportSettings::default_for(kind),
            customized: false,
            platform_overrides: Default::default(),
        };
    };
    let Ok(value) = parse(&text) else {
        return FileImportState {
            settings: ImportSettings::default_for(kind),
            customized: false,
            platform_overrides: Default::default(),
        };
    };
    let customized = matches!(value.get("customized"), Some(SealValue::Bool(true)));
    let empty: &[(String, SealValue)] = &[];
    let settings = read_settings_map(kind, value.as_map().unwrap_or(empty))
        .unwrap_or_else(|_| ImportSettings::default_for(kind));
    let platform_overrides = if kind == ImportKind::Picture {
        crate::import_platform::parse_platform_overrides(&value)
    } else {
        Default::default()
    };
    FileImportState { settings, customized, platform_overrides }
}

pub fn save_file_state(root: &Path, rel: &str, state: &FileImportState) -> Result<(), CoreError> {
    let mut map: Vec<(String, SealValue)> = vec![
        ("schema_version".into(), SealValue::Number(SIDECAR_SCHEMA.to_string())),
        ("kind".into(), SealValue::String(state.settings.kind().id().into())),
        ("customized".into(), SealValue::Bool(state.customized)),
    ];
    map.extend(settings_fields(&state.settings));
    if !state.platform_overrides.is_empty() {
        map.push((
            "platform_overrides".into(),
            crate::import_platform::platform_overrides_to_value(&state.platform_overrides),
        ));
    }
    atomic_write(&sidecar_path(root, rel), SealValue::Map(map.into()).to_text().as_bytes())
}

fn read_settings_map(
    kind: ImportKind,
    map: &[(String, SealValue)],
) -> Result<ImportSettings, String> {
    match kind {
        ImportKind::Picture => {
            let mut p = PictureSettings::default();
            for (key, value) in map {
                match (key.as_str(), value) {
                    ("filter", SealValue::String(text)) => p.filter = text.clone(),
                    ("pixels_per_unit", SealValue::Number(text)) => {
                        p.pixels_per_unit = text.parse().unwrap_or(p.pixels_per_unit);
                    }
                    ("max_size", SealValue::Number(text)) => {
                        if let Ok(n) = text.parse::<u32>() {
                            p.max_size = n;
                        }
                    }
                    ("mipmaps", SealValue::Bool(flag)) => p.mipmaps = *flag,
                    ("colour_space", SealValue::String(text)) => p.colour_space = text.clone(),
                    ("wrap", SealValue::String(text)) => p.wrap = text.clone(),
                    ("sprite_mode", SealValue::String(text)) => p.sprite_mode = text.clone(),
                    ("pivot", SealValue::String(text)) => p.pivot = text.clone(),
                    ("pivot_x", SealValue::Number(text)) => {
                        if let Ok(n) = text.parse::<f32>() {
                            p.pivot_x = n;
                        }
                    }
                    ("pivot_y", SealValue::Number(text)) => {
                        if let Ok(n) = text.parse::<f32>() {
                            p.pivot_y = n;
                        }
                    }
                    ("svg_size", SealValue::Number(text)) => {
                        if let Ok(n) = text.parse::<u32>() {
                            p.svg_size = n;
                        }
                    }
                    ("compression", SealValue::String(text)) => p.compression = text.clone(),
                    ("mesh", SealValue::String(text)) => p.mesh = text.clone(),
                    _ => {}
                }
            }
            Ok(ImportSettings::Picture(p))
        }
        ImportKind::Sound => {
            let mut s = SoundSettings::default();
            for (key, value) in map {
                match (key.as_str(), value) {
                    ("normalize", SealValue::Bool(flag)) => s.normalize = *flag,
                    ("compression", SealValue::String(text)) => s.compression = text.clone(),
                    ("vorbis_quality", SealValue::Number(text)) => {
                        s.vorbis_quality = text.parse().unwrap_or(s.vorbis_quality).clamp(0, 10);
                    }
                    ("load_type", SealValue::String(text)) => s.load_type = text.clone(),
                    _ => {}
                }
            }
            Ok(ImportSettings::Sound(s))
        }
        ImportKind::Font => {
            let _ = map;
            Ok(ImportSettings::Font(FontSettings::default()))
        }
    }
}

fn format_num(value: f32) -> String {
    if value.fract().abs() < 1e-6 {
        format!("{}", value as i64)
    } else {
        format!("{value:.6}")
    }
}

/// What a picture contributes when something draws it.
///
/// A missing sidecar uses today's behaviour: nearest sampling and the object's
/// scale as its world size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PictureLook {
    pub nearest: bool,
    /// `0` means the sprite keeps its scale as its world size.
    pub pixels_per_unit: f32,
    pub max_size: u32,
    pub mipmaps: bool,
    pub linear_colour: bool,
    /// The sidecar named a colour space. Missing means today's sRGB path, except a Radiance file.
    pub colour_explicit: bool,
    pub wrap: seal_render::TextureWrap,
    pub multiple: bool,
    /// 0,0 is the bottom left, 0.5,0.5 the centre.
    pub pivot: [f32; 2],
    /// Longer side used when rasterising an SVG. `256` when the sidecar does not say.
    pub svg_size: u32,
    /// Draw a tight outline instead of the full rectangle.
    pub mesh_tight: bool,
}

impl PictureLook {
    pub fn legacy() -> Self {
        Self {
            nearest: true,
            pixels_per_unit: 0.0,
            max_size: 0,
            mipmaps: false,
            linear_colour: false,
            colour_explicit: false,
            wrap: seal_render::TextureWrap::Clamp,
            multiple: false,
            pivot: [0.5, 0.5],
            svg_size: 256,
            mesh_tight: false,
        }
    }

    pub fn sampling(&self) -> seal_render::picture::PictureSampling {
        seal_render::picture::PictureSampling {
            filter: if self.nearest {
                seal_render::TextureFilter::Nearest
            } else {
                seal_render::TextureFilter::Linear
            },
            wrap: self.wrap,
            colour_space: if self.linear_colour {
                seal_render::ColourSpace::Linear
            } else {
                seal_render::ColourSpace::Srgb
            },
            mipmaps: self.mipmaps,
        }
    }
}

impl From<&PictureSettings> for PictureLook {
    fn from(p: &PictureSettings) -> Self {
        Self {
            nearest: p.filter != "linear",
            pixels_per_unit: if p.pixels_per_unit.is_finite() && p.pixels_per_unit > 0.0 {
                p.pixels_per_unit
            } else {
                0.0
            },
            max_size: p.max_size,
            mipmaps: p.mipmaps,
            linear_colour: p.colour_space == "linear",
            colour_explicit: false,
            wrap: match p.wrap.as_str() {
                "repeat" => seal_render::TextureWrap::Repeat,
                "mirror" => seal_render::TextureWrap::Mirror,
                _ => seal_render::TextureWrap::Clamp,
            },
            multiple: p.sprite_mode == "multiple",
            pivot: seal_render::picture::pivot_xy(&p.pivot, [p.pivot_x, p.pivot_y]),
            svg_size: if p.svg_size == 0 { 256 } else { p.svg_size },
            mesh_tight: p.mesh == "tight",
        }
    }
}

/// Import settings for the picture file at `path` (`path` is the picture, not the sidecar).
pub fn picture_look(path: &Path) -> PictureLook {
    let sidecar = PathBuf::from(format!("{}.import.seal", path.display()));
    let Ok(text) = fs::read_to_string(&sidecar) else {
        return PictureLook::legacy();
    };
    let Ok(value) = parse(&text) else {
        return PictureLook::legacy();
    };
    let empty: &[(String, SealValue)] = &[];
    let map = value.as_map().unwrap_or(empty);
    match read_settings_map(ImportKind::Picture, map) {
        Ok(ImportSettings::Picture(p)) => {
            let mut look = PictureLook::from(&p);
            look.colour_explicit = map.iter().any(|(key, _)| key == "colour_space");
            look
        }
        _ => PictureLook::legacy(),
    }
}

/// World size of a sprite that shows `picture_px`.
///
/// Today a sprite is `scale` world units on each axis (default 1 by 1) no matter
/// how many pixels the picture has. `pixels_per_unit` of 0 keeps that. A positive
/// value applies only while both scales are still the default 1: the world size
/// is then picture pixels divided by pixels per unit.
pub fn sprite_world_size(scale: [f32; 2], picture_px: [f32; 2], pixels_per_unit: f32) -> [f32; 2] {
    let explicit = (scale[0].abs() - 1.0).abs() > 1.0e-4 || (scale[1].abs() - 1.0).abs() > 1.0e-4;
    if explicit
        || !(pixels_per_unit.is_finite() && pixels_per_unit > 0.0)
        || picture_px[0] <= 0.0
        || picture_px[1] <= 0.0
    {
        return scale;
    }
    let sx = if scale[0] < 0.0 { -1.0 } else { 1.0 };
    let sy = if scale[1] < 0.0 { -1.0 } else { 1.0 };
    [sx * picture_px[0] / pixels_per_unit, sy * picture_px[1] / pixels_per_unit]
}

/// Whether the sound file at `path` should be peak-normalised on load.
pub fn sound_compression(path: &Path) -> seal_audio::compress::SoundCompression {
    let sidecar = PathBuf::from(format!("{}.import.seal", path.display()));
    let Ok(text) = fs::read_to_string(&sidecar) else {
        return seal_audio::compress::SoundCompression::Pcm;
    };
    let Ok(value) = parse(&text) else {
        return seal_audio::compress::SoundCompression::Pcm;
    };
    match value.get("compression") {
        Some(SealValue::String(text)) => seal_audio::compress::SoundCompression::parse(text),
        _ => seal_audio::compress::SoundCompression::Pcm,
    }
}

pub fn sound_load_type(path: &Path) -> seal_audio::compress::SoundLoadType {
    let sidecar = PathBuf::from(format!("{}.import.seal", path.display()));
    let Ok(text) = fs::read_to_string(&sidecar) else {
        return seal_audio::compress::SoundLoadType::Memory;
    };
    let Ok(value) = parse(&text) else {
        return seal_audio::compress::SoundLoadType::Memory;
    };
    match value.get("load_type") {
        Some(SealValue::String(text)) => seal_audio::compress::SoundLoadType::parse(text),
        _ => seal_audio::compress::SoundLoadType::Memory,
    }
}

pub fn sound_normalize(path: &Path) -> bool {
    let sidecar = PathBuf::from(format!("{}.import.seal", path.display()));
    let Ok(text) = fs::read_to_string(&sidecar) else {
        return false;
    };
    let Ok(value) = parse(&text) else {
        return false;
    };
    matches!(value.get("normalize"), Some(SealValue::Bool(true)))
}

/// Peak-normalise `sound` when the sidecar next to `path` asks for it.
pub fn finish_sound(path: &Path, mut sound: seal_audio::Sound) -> seal_audio::Sound {
    if sound_normalize(path) {
        sound.peak_normalize();
    }
    sound
}

pub fn settings_match_preset(settings: &ImportSettings, preset: &ImportPreset) -> bool {
    settings == &preset.settings
}

pub fn matching_preset_label(book: &ImportPresetBook, settings: &ImportSettings) -> String {
    for preset in &book.presets {
        if preset.kind == settings.kind() && settings_match_preset(settings, preset) {
            return preset.name.clone();
        }
    }
    "Custom".into()
}

pub fn deepest_folder_rule<'a>(
    book: &'a ImportPresetBook,
    rel: &str,
    kind: ImportKind,
) -> Option<&'a FolderRule> {
    let mut best: Option<&FolderRule> = None;
    let mut best_len = 0usize;
    for rule in &book.folder_rules {
        if rule.kind != kind {
            continue;
        }
        let prefix = normalize_folder(&rule.path);
        if prefix.is_empty() {
            continue;
        }
        if rel == prefix || rel.starts_with(&format!("{prefix}/")) {
            let len = prefix.len();
            if len >= best_len {
                best = Some(rule);
                best_len = len;
            }
        }
    }
    best
}

fn normalize_folder(text: &str) -> String {
    text.trim().trim_matches('/').replace('\\', "/")
}

pub fn preset_for_new_file<'a>(
    book: &'a ImportPresetBook,
    rel: &str,
    kind: ImportKind,
) -> Option<&'a ImportPreset> {
    if let Some(rule) = deepest_folder_rule(book, rel, kind) {
        return book.preset_by_name(&rule.preset);
    }
    book.defaults.get(kind.id()).and_then(|name| book.preset_by_name(name))
}

pub fn apply_preset_to_settings(preset: &ImportPreset, customized: bool) -> FileImportState {
    FileImportState {
        settings: preset.settings.clone(),
        customized,
        platform_overrides: Default::default(),
    }
}

pub fn reimport_asset(root: &Path, rel: &str, state: &FileImportState) -> Result<(), CoreError> {
    let source = root.join(rel);
    if !source.is_file() {
        return Err(CoreError::new(format!("{rel} was not found")));
    }
    save_file_state(root, rel, state)?;
    if let ImportSettings::Picture(picture) = &state.settings {
        let _ = touch_picture_cache(root, rel);
        let _ = cache_imported_size(root, rel, picture);
        crate::picture_pack::sync_mesh(root, rel, picture);
    }
    Ok(())
}

fn cache_imported_size(
    root: &Path,
    rel: &str,
    settings: &PictureSettings,
) -> Result<(), CoreError> {
    let full = root.join(rel);
    let picture = seal_render::picture::decode_path_with(&full, settings.svg_size)
        .map_err(|e| CoreError::new(e.to_string()))?;
    let (_, width, height) = seal_render::picture::downscale(
        &picture.rgba,
        picture.width,
        picture.height,
        settings.max_size,
    );
    let cache = root.join(".seal-editor").join("import-cache");
    fs::create_dir_all(&cache).map_err(|e| CoreError::new(e.to_string()))?;
    let name = rel.replace(['/', '\\'], "__");
    atomic_write(&cache.join(format!("{name}.size")), format!("{width} {height}\n").as_bytes())
}

fn touch_picture_cache(root: &Path, rel: &str) -> Result<(), CoreError> {
    let full = root.join(rel);
    let _ = fs::metadata(&full)?;
    let cache = root.join(".seal-editor").join("import-cache");
    fs::create_dir_all(&cache).map_err(|e| CoreError::new(e.to_string()))?;
    let stamp = full
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let note = format!("{rel}\t{stamp}\n");
    atomic_write(&cache.join("last-reimport.txt"), note.as_bytes())
}

pub fn load_for_project(session: &mut EditorSession) {
    let Some(root) = session.project.as_ref().map(|p| p.root.clone()) else {
        session.models.import_presets = ImportPresetBook::with_builtins();
        return;
    };
    let book = load_book(&root);
    for warning in &book.load_warnings {
        session.notify(warning.clone(), true);
    }
    session.models.import_presets = book;
}

fn project_root(session: &EditorSession) -> Result<PathBuf, CoreError> {
    session
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .ok_or_else(|| CoreError::new("open a project first"))
}

fn parse_paths(session: &EditorSession, args: &CommandArgs) -> Vec<String> {
    if let Some(raw) = args.get("paths").filter(|t| !t.is_empty()) {
        return raw
            .split(['\n', ';'])
            .map(|p| p.trim().replace('\\', "/"))
            .filter(|p| !p.is_empty())
            .collect();
    }
    session.models.content_pick.clone().into_iter().filter(|p| !p.is_empty()).collect()
}

pub fn apply_preset_on_import(
    root: &Path,
    book: &ImportPresetBook,
    rel: &str,
) -> Result<(), CoreError> {
    let Some(kind) = ImportKind::from_asset_path(rel) else {
        return Ok(());
    };
    let mut state = load_file_state(root, rel, kind);
    if state.customized {
        return Ok(());
    }
    if let Some(preset) = preset_for_new_file(book, rel, kind) {
        state.settings = preset.settings.clone();
    }
    prefer_linear_hdr(root, rel, &mut state);
    reimport_asset(root, rel, &state)?;
    Ok(())
}

fn prefer_linear_hdr(root: &Path, rel: &str, state: &mut FileImportState) {
    let ImportSettings::Picture(picture) = &mut state.settings else {
        return;
    };
    if picture.colour_space != "srgb" {
        return;
    }
    if seal_render::picture::decode_path(&root.join(rel)).is_ok_and(|decoded| decoded.linear_file) {
        picture.colour_space = "linear".into();
    }
}

fn cmd_apply(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let preset_name = args
        .get("preset")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| CoreError::new("pick a preset to apply"))?;
    let root = project_root(session)?;
    let preset = session
        .models
        .import_presets
        .preset_by_name(preset_name)
        .ok_or_else(|| CoreError::new(format!("no preset named {preset_name}")))?
        .clone();
    let paths: Vec<String> = parse_paths(session, args);
    if paths.is_empty() {
        return Err(CoreError::new("select a file first"));
    }
    let mut snapshots = Vec::new();
    let mut applied = 0usize;
    for rel in &paths {
        let Some(kind) = ImportKind::from_asset_path(rel) else {
            continue;
        };
        if kind != preset.kind {
            continue;
        }
        let before = sidecar_snapshot(&root, rel, kind);
        let state = apply_preset_to_settings(&preset, false);
        reimport_asset(&root, rel, &state)?;
        let after = sidecar_snapshot(&root, rel, kind);
        snapshots.push((rel.clone(), before, after));
        applied += 1;
    }
    if applied == 0 {
        return Err(CoreError::new("no selected files match the preset kind"));
    }
    for (rel, _, _) in &snapshots {
        crate::asset_index::record_import_activity(
            session,
            rel,
            "reimport",
            "ok",
            std::time::Instant::now(),
        );
    }
    session
        .push_past(Past::ImportBatch { label: format!("Apply preset {preset_name}"), snapshots });
    session.future.clear();
    session.models.content_generation = session.models.content_generation.wrapping_add(1);
    session.notify(format!("Applied preset {preset_name}"), false);
    Ok(())
}

fn sidecar_snapshot(root: &Path, rel: &str, _kind: ImportKind) -> Option<String> {
    let path = sidecar_path(root, rel);
    path.is_file().then(|| fs::read_to_string(&path).unwrap_or_default())
}

fn restore_snapshots(
    root: &Path,
    snapshots: &[(String, Option<String>, Option<String>)],
    use_after: bool,
) {
    for (rel, before, after) in snapshots {
        let pick = if use_after { after.as_ref() } else { before.as_ref() };
        let path = if rel == BOOK_FILE {
            book_path(root)
        } else {
            sidecar_path(root, rel)
        };
        match pick {
            Some(text) => {
                let _ = atomic_write(&path, text.as_bytes());
            }
            None if path.exists() => {
                let _ = fs::remove_file(&path);
            }
            None => {}
        }
    }
}

pub fn undo_import_batch(session: &mut EditorSession, past: &Past) {
    let Past::ImportBatch { snapshots, .. } = past else {
        return;
    };
    if let Ok(root) = project_root(session) {
        restore_snapshots(&root, snapshots, false);
        session.models.content_generation = session.models.content_generation.wrapping_add(1);
    }
}

pub fn redo_import_batch(session: &mut EditorSession, past: &Past) {
    let Past::ImportBatch { snapshots, .. } = past else {
        return;
    };
    if let Ok(root) = project_root(session) {
        restore_snapshots(&root, snapshots, true);
        session.models.content_generation = session.models.content_generation.wrapping_add(1);
    }
}

fn cmd_save_preset(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let name = args
        .get("name")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| CoreError::new("give the new preset a name"))?;
    let rel = args
        .get("path")
        .or(session.models.content_pick.as_deref())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| CoreError::new("open a file in the Asset Editor first"))?;
    let kind = ImportKind::from_asset_path(rel)
        .ok_or_else(|| CoreError::new("this file has no import settings"))?;
    let root = project_root(session)?;
    let state = load_file_state(&root, rel, kind);
    if session.models.import_presets.preset_by_name(name).is_some() {
        return Err(CoreError::new(format!("a preset named {name} already exists")));
    }
    session.models.import_presets.presets.push(ImportPreset {
        name: name.to_string(),
        kind,
        builtin: false,
        settings: state.settings.clone(),
    });
    save_book(&root, &session.models.import_presets)?;
    session.notify(format!("Saved preset {name}"), false);
    Ok(())
}

fn cmd_rename_preset(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let from = required_name(args, "from")?;
    let to = required_name(args, "to")?;
    let root = project_root(session)?;
    let Some(ix) = session.models.import_presets.presets.iter().position(|p| p.name == from) else {
        return Err(CoreError::new(format!("no preset named {from}")));
    };
    if session.models.import_presets.presets[ix].builtin {
        return Err(CoreError::new("built-in presets cannot be renamed"));
    }
    if session.models.import_presets.presets.iter().any(|p| p.name == to) {
        return Err(CoreError::new(format!("a preset named {to} already exists")));
    }
    session.models.import_presets.presets[ix].name = to.clone();
    for rule in &mut session.models.import_presets.folder_rules {
        if rule.preset == from {
            rule.preset = to.clone();
        }
    }
    for value in session.models.import_presets.defaults.values_mut() {
        if value == &from {
            *value = to.clone();
        }
    }
    save_book(&root, &session.models.import_presets)?;
    Ok(())
}

fn cmd_delete_preset(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let name = required_name(args, "name")?;
    let root = project_root(session)?;
    let Some(ix) = session.models.import_presets.presets.iter().position(|p| p.name == name) else {
        return Err(CoreError::new(format!("no preset named {name}")));
    };
    if session.models.import_presets.presets[ix].builtin {
        return Err(CoreError::new("built-in presets cannot be deleted"));
    }
    session.models.import_presets.presets.remove(ix);
    session.models.import_presets.folder_rules.retain(|r| r.preset != name);
    session.models.import_presets.defaults.retain(|_, v| v != &name);
    save_book(&root, &session.models.import_presets)?;
    Ok(())
}

fn cmd_duplicate_preset(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let name = required_name(args, "name")?;
    let to = required_name(args, "to")?;
    let root = project_root(session)?;
    let preset = session
        .models
        .import_presets
        .preset_by_name(&name)
        .ok_or_else(|| CoreError::new(format!("no preset named {name}")))?
        .clone();
    if session.models.import_presets.preset_by_name(&to).is_some() {
        return Err(CoreError::new(format!("a preset named {to} already exists")));
    }
    session.models.import_presets.presets.push(ImportPreset {
        name: to,
        kind: preset.kind,
        builtin: false,
        settings: preset.settings,
    });
    save_book(&root, &session.models.import_presets)?;
    Ok(())
}

fn cmd_set_default(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let kind = ImportKind::from_id(required(args, "kind")?)
        .ok_or_else(|| CoreError::new("kind must be picture, sound or font"))?;
    let preset = required_name(args, "preset")?;
    if session.models.import_presets.preset_by_name(&preset).is_none() {
        return Err(CoreError::new(format!("no preset named {preset}")));
    }
    let root = project_root(session)?;
    session.models.import_presets.defaults.insert(kind.id().into(), preset);
    save_book(&root, &session.models.import_presets)?;
    apply_defaults_to_uncustomized(session, kind)?;
    Ok(())
}

fn cmd_folder_rule(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let path = normalize_folder(args.get("path").unwrap_or(""));
    let kind = ImportKind::from_id(required(args, "kind")?)
        .ok_or_else(|| CoreError::new("kind must be picture, sound or font"))?;
    let preset = required_name(args, "preset")?;
    if session.models.import_presets.preset_by_name(&preset).is_none() {
        return Err(CoreError::new(format!("no preset named {preset}")));
    }
    let root = project_root(session)?;
    session.models.import_presets.folder_rules.retain(|r| r.path != path || r.kind != kind);
    session.models.import_presets.folder_rules.push(FolderRule { path, kind, preset });
    save_book(&root, &session.models.import_presets)?;
    apply_folder_rules(session, kind)?;
    Ok(())
}

fn cmd_platform_field(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let open = session.draft(crate::asset_editor::ASSET_DRAFT);
    let rel = args
        .get("path")
        .or(open.as_deref())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| CoreError::new("no file is open"))?
        .to_string();
    let platform = crate::import_platform::ImportPlatform::from_id(required(args, "platform")?)
        .ok_or_else(|| CoreError::new("platform must be windows, linux, macos, web, android or ios"))?;
    let field = required(args, "field")?;
    let value = required(args, "value")?;
    let kind = ImportKind::from_asset_path(&rel)
        .ok_or_else(|| CoreError::new("this file has no import settings"))?;
    if kind != ImportKind::Picture {
        return Err(CoreError::new("platform overrides apply only to pictures"));
    }
    let root = project_root(session)?;
    let before = sidecar_snapshot(&root, &rel, kind);
    let mut state = load_file_state(&root, &rel, kind);
    let entry = state.platform_overrides.entry(platform).or_default();
    crate::import_platform::set_picture_override_field(entry, field, value)?;
    if crate::import_platform::picture_override_is_empty(entry) {
        state.platform_overrides.remove(&platform);
    }
    state.customized = true;
    reimport_asset(&root, &rel, &state)?;
    let after = sidecar_snapshot(&root, &rel, kind);
    session.push_past(Past::ImportBatch {
        label: format!("Platform import {field}"),
        snapshots: vec![(rel, before, after)],
    });
    session.future.clear();
    session.models.content_generation = session.models.content_generation.wrapping_add(1);
    Ok(())
}

fn cmd_atlas_platform_field(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let platform = crate::import_platform::ImportPlatform::from_id(required(args, "platform")?)
        .ok_or_else(|| CoreError::new("platform must be windows, linux, macos, web, android or ios"))?;
    let field = required(args, "field")?;
    let value = required(args, "value")?;
    let root = project_root(session)?;
    let before = fs::read_to_string(book_path(&root)).unwrap_or_default();
    let entry = session.models.import_presets.atlas_platform_overrides.entry(platform).or_default();
    crate::import_platform::set_atlas_override_field(entry, field, value)?;
    if entry.page_size.is_none() {
        session.models.import_presets.atlas_platform_overrides.remove(&platform);
    }
    save_book(&root, &session.models.import_presets)?;
    let after = fs::read_to_string(book_path(&root)).unwrap_or_default();
    session.push_past(Past::ImportBatch {
        label: format!("Atlas page size {field}"),
        snapshots: vec![(BOOK_FILE.to_string(), Some(before), Some(after))],
    });
    session.future.clear();
    Ok(())
}

fn cmd_field_set(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let open = session.draft(crate::asset_editor::ASSET_DRAFT);
    let rel = args
        .get("path")
        .or(open.as_deref())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| CoreError::new("no file is open"))?
        .to_string();
    let field = required(args, "field")?;
    let value = required(args, "value")?;
    let kind = ImportKind::from_asset_path(&rel)
        .ok_or_else(|| CoreError::new("this file has no import settings"))?;
    let root = project_root(session)?;
    let before = sidecar_snapshot(&root, &rel, kind);
    let mut state = load_file_state(&root, &rel, kind);
    apply_field(&mut state.settings, field, value)?;
    state.customized = true;
    reimport_asset(&root, &rel, &state)?;
    let after = sidecar_snapshot(&root, &rel, kind);
    session.push_past(Past::ImportBatch {
        label: format!("Import setting {field}"),
        snapshots: vec![(rel, before, after)],
    });
    session.future.clear();
    session.models.content_generation = session.models.content_generation.wrapping_add(1);
    Ok(())
}

fn apply_field(settings: &mut ImportSettings, field: &str, value: &str) -> Result<(), CoreError> {
    match settings {
        ImportSettings::Picture(p) => match field {
            "filter" if matches!(value, "nearest" | "linear") => p.filter = value.into(),
            "pixels_per_unit" => {
                let n: f32 = value
                    .parse()
                    .map_err(|_| CoreError::new("pixels_per_unit must be a number"))?;
                if !n.is_finite() || n < 0.0 {
                    return Err(CoreError::new("pixels_per_unit must be zero or greater"));
                }
                p.pixels_per_unit = n;
            }
            "max_size" => {
                let n: u32 =
                    value.parse().map_err(|_| CoreError::new("max size must be a number"))?;
                if n != 0 && !PICTURE_MAX_SIZES.contains(&n) {
                    return Err(CoreError::new(
                        "max size must be 0 or a power of two from 32 to 4096",
                    ));
                }
                p.max_size = n;
            }
            "mipmaps" => p.mipmaps = value == "true",
            "colour_space" if matches!(value, "srgb" | "linear") => p.colour_space = value.into(),
            "wrap" if matches!(value, "clamp" | "repeat" | "mirror") => p.wrap = value.into(),
            "sprite_mode" if matches!(value, "single" | "multiple") => p.sprite_mode = value.into(),
            "pivot" if PIVOTS.contains(&value) => {
                p.pivot = value.into();
                if value != "custom" {
                    let point = seal_render::picture::pivot_xy(value, [p.pivot_x, p.pivot_y]);
                    p.pivot_x = point[0];
                    p.pivot_y = point[1];
                }
            }
            "pivot_x" | "pivot_y" => {
                let n: f32 = value.parse().map_err(|_| CoreError::new("pivot must be a number"))?;
                if !n.is_finite() {
                    return Err(CoreError::new("pivot must be a number"));
                }
                let n = n.clamp(0.0, 1.0);
                if field == "pivot_x" {
                    p.pivot_x = n;
                } else {
                    p.pivot_y = n;
                }
            }
            "svg_size" => {
                let n: u32 =
                    value.parse().map_err(|_| CoreError::new("svg size must be a number"))?;
                if n != 0 && !PICTURE_MAX_SIZES.contains(&n) {
                    return Err(CoreError::new("svg size must be a power of two from 32 to 4096"));
                }
                p.svg_size = if n == 0 { 256 } else { n };
            }
            "compression" if matches!(value, "none" | "lossless" | "low" | "medium" | "high") => {
                p.compression = value.into();
            }
            "mesh" if matches!(value, "full" | "tight") => p.mesh = value.into(),
            other => return Err(CoreError::new(format!("unknown picture field {other}"))),
        },
        ImportSettings::Sound(s) => match field {
            "normalize" => s.normalize = value == "true",
            "compression" if matches!(value, "pcm" | "adpcm" | "vorbis") => s.compression = value.into(),
            "vorbis_quality" => {
                s.vorbis_quality = value
                    .parse::<u8>()
                    .map_err(|_| CoreError::new("vorbis quality must be 0 to 10"))?
                    .clamp(0, 10);
            }
            "load_type" if matches!(value, "memory" | "stream") => s.load_type = value.into(),
            other => return Err(CoreError::new(format!("unknown sound field {other}"))),
        },
        ImportSettings::Font(_) => {
            return Err(CoreError::new(format!("fonts have no import field {field}")));
        }
    }
    Ok(())
}

fn apply_defaults_to_uncustomized(
    session: &mut EditorSession,
    kind: ImportKind,
) -> Result<(), CoreError> {
    crate::editor_tools::note_worker_budget(&session.foundation.settings);
    let root = project_root(session)?;
    let book = session.models.import_presets.clone();
    let Some(preset) = book.defaults.get(kind.id()).and_then(|n| book.preset_by_name(n)) else {
        return Ok(());
    };
    let settings = preset.settings.clone();
    let entries = list_import_files(&root, kind);
    let results = crate::editor_tools::map_in_order(
        crate::editor_tools::active_workers(),
        entries,
        |entry| {
            let mut state = load_file_state(&root, &entry, kind);
            if state.customized {
                return Ok(());
            }
            state.settings = settings.clone();
            reimport_asset(&root, &entry, &state)
        },
    );
    for result in results {
        result?;
    }
    Ok(())
}

fn apply_folder_rules(session: &mut EditorSession, kind: ImportKind) -> Result<(), CoreError> {
    crate::editor_tools::note_worker_budget(&session.foundation.settings);
    let root = project_root(session)?;
    let book = session.models.import_presets.clone();
    let entries = list_import_files(&root, kind);
    let results = crate::editor_tools::map_in_order(
        crate::editor_tools::active_workers(),
        entries,
        |entry| {
            let mut state = load_file_state(&root, &entry, kind);
            if state.customized {
                return Ok(());
            }
            let Some(preset) = deepest_folder_rule(&book, &entry, kind)
                .and_then(|rule| book.preset_by_name(&rule.preset))
            else {
                return Ok(());
            };
            state.settings = preset.settings.clone();
            reimport_asset(&root, &entry, &state)
        },
    );
    for result in results {
        result?;
    }
    Ok(())
}

fn list_import_files(root: &Path, kind: ImportKind) -> Vec<String> {
    let mut out = Vec::new();
    collect_import_files(root, root, kind, &mut out, 0);
    out
}

fn collect_import_files(
    root: &Path,
    dir: &Path,
    kind: ImportKind,
    out: &mut Vec<String>,
    depth: usize,
) {
    if depth > 32 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == ".seal-editor" || n == ".git") {
                continue;
            }
            collect_import_files(root, &path, kind, out, depth + 1);
        } else if let Ok(rel) = path.strip_prefix(root) {
            let rel = rel.to_string_lossy().replace('\\', "/");
            if ImportKind::from_asset_path(&rel) == Some(kind) {
                out.push(rel);
            }
        }
    }
}

fn required<'a>(args: &'a CommandArgs, key: &str) -> Result<&'a str, CoreError> {
    args.get(key)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| CoreError::new(format!("{key}= is required")))
}

fn required_name(args: &CommandArgs, key: &str) -> Result<String, CoreError> {
    let name = required(args, key)?.to_string();
    if name == "Custom" {
        return Err(CoreError::new("that name is reserved"));
    }
    Ok(name)
}

type CommandFn = fn(&mut EditorSession, &CommandArgs) -> Result<(), CoreError>;

pub const COMMANDS: &[(&str, &str, &str, CommandFn)] = &[
    ("import.preset.apply", "command-import-preset-apply", "file", cmd_apply),
    ("import.preset.save", "command-import-preset-save", "file", cmd_save_preset),
    ("import.preset.rename", "command-import-preset-rename", "file", cmd_rename_preset),
    ("import.preset.delete", "command-import-preset-delete", "file", cmd_delete_preset),
    ("import.preset.duplicate", "command-import-preset-duplicate", "file", cmd_duplicate_preset),
    ("import.preset.default", "command-import-preset-default", "tools", cmd_set_default),
    ("import.preset.folder_rule", "command-import-preset-folder-rule", "tools", cmd_folder_rule),
    ("import.preset.field", "command-import-preset-field", "edit", cmd_field_set),
    ("import.platform.field", "command-import-platform-field", "edit", cmd_platform_field),
    (
        "import.atlas.platform_field",
        "command-import-atlas-platform-field",
        "tools",
        cmd_atlas_platform_field,
    ),
];

pub fn browser_preset_labels(session: &EditorSession, rel: &str) -> Vec<String> {
    let Some(kind) = ImportKind::from_asset_path(rel) else {
        return Vec::new();
    };
    session
        .models
        .import_presets
        .presets_for_kind(kind)
        .into_iter()
        .map(|p| p.name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "seal-import-presets-{}-{label}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn apply_preset_sets_exact_fields() {
        let dir = scratch("apply");
        fs::create_dir_all(dir.join("sprites")).unwrap();
        fs::write(dir.join("sprites/a.png"), b"png").unwrap();
        let book = ImportPresetBook::with_builtins();
        let preset = book.preset_by_name("Pixel Art").unwrap();
        let state = apply_preset_to_settings(preset, false);
        reimport_asset(&dir, "sprites/a.png", &state).unwrap();
        let loaded = load_file_state(&dir, "sprites/a.png", ImportKind::Picture);
        assert_eq!(loaded.settings, preset.settings);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn folder_rule_deepest_wins_and_kind_must_match() {
        let mut book = ImportPresetBook::with_builtins();
        book.folder_rules = vec![
            FolderRule {
                path: "sprites".into(),
                kind: ImportKind::Picture,
                preset: "Smooth".into(),
            },
            FolderRule {
                path: "sprites/pixel".into(),
                kind: ImportKind::Picture,
                preset: "Pixel Art".into(),
            },
        ];
        let rule =
            deepest_folder_rule(&book, "sprites/pixel/coin.png", ImportKind::Picture).unwrap();
        assert_eq!(rule.preset, "Pixel Art");
        assert!(deepest_folder_rule(&book, "sprites/pixel/coin.png", ImportKind::Sound).is_none());
    }

    #[test]
    fn default_preset_used_for_new_files() {
        let dir = scratch("default");
        let mut book = ImportPresetBook::with_builtins();
        book.defaults.insert("picture".into(), "Pixel Art".into());
        fs::create_dir_all(dir.join("sprites")).unwrap();
        fs::write(dir.join("sprites/new.png"), b"x").unwrap();
        apply_preset_on_import(&dir, &book, "sprites/new.png").unwrap();
        let loaded = load_file_state(&dir, "sprites/new.png", ImportKind::Picture);
        let preset = book.preset_by_name("Pixel Art").unwrap();
        assert_eq!(loaded.settings, preset.settings);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn hand_edited_file_not_overwritten_by_rule_change() {
        let dir = scratch("custom");
        fs::create_dir_all(dir.join("sprites/pixel")).unwrap();
        fs::write(dir.join("sprites/pixel/a.png"), b"x").unwrap();
        let mut state = load_file_state(&dir, "sprites/pixel/a.png", ImportKind::Picture);
        state.customized = true;
        if let ImportSettings::Picture(p) = &mut state.settings {
            p.filter = "linear".into();
        }
        save_file_state(&dir, "sprites/pixel/a.png", &state).unwrap();
        let mut book = ImportPresetBook::with_builtins();
        book.folder_rules.push(FolderRule {
            path: "sprites/pixel".into(),
            kind: ImportKind::Picture,
            preset: "Pixel Art".into(),
        });
        let before = load_file_state(&dir, "sprites/pixel/a.png", ImportKind::Picture);
        if let Some(rule) = deepest_folder_rule(&book, "sprites/pixel/a.png", ImportKind::Picture) {
            if let Some(preset) = book.preset_by_name(&rule.preset) {
                if !before.customized {
                    let next = apply_preset_to_settings(preset, false);
                    reimport_asset(&dir, "sprites/pixel/a.png", &next).unwrap();
                }
            }
        }
        let after = load_file_state(&dir, "sprites/pixel/a.png", ImportKind::Picture);
        assert_eq!(after.settings, before.settings);
        assert!(after.customized);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn builtins_cannot_be_deleted_or_renamed_by_logic() {
        let book = ImportPresetBook::with_builtins();
        let preset = book.preset_by_name("Smooth").unwrap();
        assert!(preset.builtin);
    }

    #[test]
    fn round_trip_book_and_sidecar() {
        let dir = scratch("roundtrip");
        let mut book = ImportPresetBook::with_builtins();
        book.defaults.insert("sound".into(), "Effect".into());
        book.folder_rules.push(FolderRule {
            path: "sounds".into(),
            kind: ImportKind::Sound,
            preset: "Music".into(),
        });
        book.presets.push(ImportPreset {
            name: "Loud".into(),
            kind: ImportKind::Sound,
            builtin: false,
            settings: ImportSettings::Sound(SoundSettings { normalize: false, ..Default::default() }),
        });
        save_book(&dir, &book).unwrap();
        let loaded = load_book(&dir);
        assert_eq!(loaded.defaults.get("sound").map(String::as_str), Some("Effect"));
        assert_eq!(loaded.folder_rules.len(), 1);
        assert!(loaded.preset_by_name("Loud").is_some());
        fs::create_dir_all(dir.join("sounds")).unwrap();
        fs::write(dir.join("sounds/a.wav"), b"wav").unwrap();
        let state = FileImportState {
            settings: ImportSettings::Sound(SoundSettings::default()),
            customized: false,
            platform_overrides: Default::default(),
        };
        save_file_state(&dir, "sounds/a.wav", &state).unwrap();
        let again = load_file_state(&dir, "sounds/a.wav", ImportKind::Sound);
        assert_eq!(again, state);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn damaged_preset_skipped_others_load() {
        let bad = parse_user_preset(&SealValue::Map(
            vec![("kind".into(), SealValue::String("sound".into()))].into(),
        ));
        assert!(bad.is_err());
        let dir = scratch("damaged");
        let mut book = ImportPresetBook::with_builtins();
        book.presets.push(ImportPreset {
            name: "Team".into(),
            kind: ImportKind::Picture,
            builtin: false,
            settings: ImportSettings::Picture(PictureSettings::default()),
        });
        save_book(&dir, &book).unwrap();
        fs::write(dir.join(BOOK_FILE), "schema_version = 1\npresets = [").unwrap();
        let broken = load_book(&dir);
        assert!(!broken.load_warnings.is_empty());
        assert!(broken.preset_by_name("Pixel Art").is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn custom_label_when_no_preset_matches() {
        let book = ImportPresetBook::with_builtins();
        let custom = ImportSettings::Picture(PictureSettings {
            filter: "linear".into(),
            pixels_per_unit: 16.0,
            ..Default::default()
        });
        assert_eq!(matching_preset_label(&book, &custom), "Custom");
    }

    #[test]
    fn pixels_per_unit_sizes_a_default_sprite_and_leaves_today_alone() {
        // Today a sprite's world size is its scale. Scale 1 is 1 by 1 world units,
        // whatever the picture's pixel size. That is pixels-per-unit left unset (0).
        assert_eq!(sprite_world_size([1.0, 1.0], [32.0, 16.0], 0.0), [1.0, 1.0]);
        assert_eq!(sprite_world_size([2.0, 0.5], [32.0, 16.0], 16.0), [2.0, 0.5]);
        assert_eq!(sprite_world_size([1.0, 1.0], [32.0, 16.0], 16.0), [2.0, 1.0]);
    }

    #[test]
    fn a_picture_without_a_sidecar_keeps_today_and_normalize_changes_the_peak() {
        let missing = picture_look(Path::new("/no/such/picture.png"));
        assert_eq!(missing, PictureLook::legacy());
        let dir = scratch("live");
        fs::write(dir.join("a.png"), b"png").unwrap();
        assert_eq!(picture_look(&dir.join("a.png")), PictureLook::legacy());
        let tone = seal_audio::Sound::tone(8_000, 440.0, 0.02, 0.25);
        fs::write(dir.join("a.wav"), tone.to_wav16()).unwrap();
        let plain = seal_audio::Sound::from_wav(&fs::read(dir.join("a.wav")).unwrap()).unwrap();
        let peak = |s: &seal_audio::Sound| s.samples.iter().map(|v| v.abs()).fold(0.0f32, f32::max);
        assert!(peak(&plain) < 0.5);
        let state = FileImportState {
            settings: ImportSettings::Sound(SoundSettings { normalize: true, ..Default::default() }),
            customized: true,
            platform_overrides: Default::default(),
        };
        save_file_state(&dir, "a.wav", &state).unwrap();
        let loud = finish_sound(
            &dir.join("a.wav"),
            seal_audio::Sound::from_wav(&fs::read(dir.join("a.wav")).unwrap()).unwrap(),
        );
        assert!((peak(&loud) - 1.0).abs() < 1.0e-3);
        let quiet = finish_sound(&dir.join("missing-sidecar.wav"), plain);
        assert!(peak(&quiet) < 0.5);
        let _ = fs::remove_dir_all(&dir);
    }
}
