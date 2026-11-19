//! Per-platform overrides for picture import and atlas page size at export.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use seal_sealtext::SealValue;

use crate::error::CoreError;
use crate::import_presets::{
    load_file_state, picture_look, ImportKind, ImportPresetBook, ImportSettings, PictureSettings,
    PICTURE_MAX_SIZES,
};

/// Target platform for export and import overrides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ImportPlatform {
    Windows,
    Linux,
    Macos,
    Web,
    Android,
    Ios,
}

pub const ALL_PLATFORMS: [ImportPlatform; 6] = [
    ImportPlatform::Windows,
    ImportPlatform::Linux,
    ImportPlatform::Macos,
    ImportPlatform::Web,
    ImportPlatform::Android,
    ImportPlatform::Ios,
];

impl ImportPlatform {
    pub fn id(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Macos => "macos",
            Self::Web => "web",
            Self::Android => "android",
            Self::Ios => "ios",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "windows" => Some(Self::Windows),
            "linux" => Some(Self::Linux),
            "macos" => Some(Self::Macos),
            "web" => Some(Self::Web),
            "android" => Some(Self::Android),
            "ios" => Some(Self::Ios),
            _ => None,
        }
    }
}

/// Optional picture fields that replace the base import settings for one platform.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PicturePlatformOverride {
    pub max_size: Option<u32>,
    pub compression: Option<String>,
    pub format: Option<String>,
}

/// Optional atlas packing fields for one platform (stored in `import-presets.seal`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AtlasPlatformOverride {
    pub page_size: Option<u32>,
}

pub type PlatformOverrideMap = BTreeMap<ImportPlatform, PicturePlatformOverride>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectivePictureExport {
    pub max_size: u32,
    pub compression: String,
    pub format: String,
}

pub fn effective_picture(
    base: &PictureSettings,
    ov: &PicturePlatformOverride,
) -> EffectivePictureExport {
    EffectivePictureExport {
        max_size: ov.max_size.unwrap_or(base.max_size),
        compression: ov
            .compression
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "balanced".into()),
        format: ov.format.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| "keep".into()),
    }
}

pub fn atlas_page_size(book: &ImportPresetBook, platform: ImportPlatform) -> u32 {
    book.atlas_platform_overrides
        .get(&platform)
        .and_then(|o| o.page_size)
        .filter(|&s| s > 0 && (s == 0 || PICTURE_MAX_SIZES.contains(&s) || s.is_power_of_two()))
        .unwrap_or(crate::atlas::PAGE_SIZE)
}

pub fn parse_platform_overrides(value: &SealValue) -> PlatformOverrideMap {
    let mut out = PlatformOverrideMap::new();
    let Some(SealValue::Map(map)) = value.get("platform_overrides") else {
        return out;
    };
    for (key, item) in &map.entries {
        let Some(platform) = ImportPlatform::from_id(key) else {
            continue;
        };
        if let Some(ov) = parse_picture_override(item) {
            if !ov_is_empty(&ov) {
                out.insert(platform, ov);
            }
        }
    }
    out
}

pub fn platform_overrides_to_value(map: &PlatformOverrideMap) -> SealValue {
    let mut entries = Vec::new();
    for platform in ALL_PLATFORMS {
        if let Some(ov) = map.get(&platform) {
            if !ov_is_empty(ov) {
                entries.push((platform.id().into(), picture_override_to_value(ov)));
            }
        }
    }
    SealValue::Map(entries.into())
}

pub fn parse_atlas_platform_overrides(
    value: &SealValue,
) -> BTreeMap<ImportPlatform, AtlasPlatformOverride> {
    let mut out = BTreeMap::new();
    let Some(SealValue::Map(map)) = value.get("atlas_platform_overrides") else {
        return out;
    };
    for (key, item) in &map.entries {
        let Some(platform) = ImportPlatform::from_id(key) else {
            continue;
        };
        if let Some(ov) = parse_atlas_override(item) {
            if ov.page_size.is_some() {
                out.insert(platform, ov);
            }
        }
    }
    out
}

pub fn atlas_platform_overrides_to_value(
    map: &BTreeMap<ImportPlatform, AtlasPlatformOverride>,
) -> SealValue {
    let mut entries = Vec::new();
    for platform in ALL_PLATFORMS {
        if let Some(ov) = map.get(&platform) {
            if let Some(size) = ov.page_size {
                entries.push((
                    platform.id().into(),
                    SealValue::Map(
                        vec![("page_size".into(), SealValue::Number(size.to_string()))].into(),
                    ),
                ));
            }
        }
    }
    SealValue::Map(entries.into())
}

fn ov_is_empty(ov: &PicturePlatformOverride) -> bool {
    ov.max_size.is_none() && ov.compression.is_none() && ov.format.is_none()
}

pub fn picture_override_is_empty(ov: &PicturePlatformOverride) -> bool {
    ov_is_empty(ov)
}

fn parse_picture_override(value: &SealValue) -> Option<PicturePlatformOverride> {
    let map = value.as_map()?;
    let mut ov = PicturePlatformOverride::default();
    for (key, item) in map {
        match (key.as_str(), item) {
            ("max_size", SealValue::Number(text)) => {
                if let Ok(n) = text.parse::<u32>() {
                    if n == 0 || PICTURE_MAX_SIZES.contains(&n) {
                        ov.max_size = Some(n);
                    }
                }
            }
            ("compression", SealValue::String(text)) if valid_compression(text) => {
                ov.compression = Some(text.clone());
            }
            ("format", SealValue::String(text)) if valid_format(text) => {
                ov.format = Some(text.clone());
            }
            _ => {}
        }
    }
    Some(ov)
}

fn parse_atlas_override(value: &SealValue) -> Option<AtlasPlatformOverride> {
    let map = value.as_map()?;
    let mut ov = AtlasPlatformOverride::default();
    for (key, item) in map {
        if key == "page_size" {
            if let SealValue::Number(text) = item {
                if let Ok(n) = text.parse::<u32>() {
                    if n == 0 || valid_atlas_page(n) {
                        ov.page_size = Some(n);
                    }
                }
            }
        }
    }
    Some(ov)
}

fn picture_override_to_value(ov: &PicturePlatformOverride) -> SealValue {
    let mut map = Vec::new();
    if let Some(n) = ov.max_size {
        map.push(("max_size".into(), SealValue::Number(n.to_string())));
    }
    if let Some(text) = &ov.compression {
        map.push(("compression".into(), SealValue::String(text.clone())));
    }
    if let Some(text) = &ov.format {
        map.push(("format".into(), SealValue::String(text.clone())));
    }
    SealValue::Map(map.into())
}

pub fn valid_compression(text: &str) -> bool {
    matches!(text, "none" | "fast" | "balanced" | "maximum")
}

pub fn valid_format(text: &str) -> bool {
    matches!(text, "keep" | "png" | "jpeg" | "webp")
}

pub fn valid_atlas_page(n: u32) -> bool {
    PICTURE_MAX_SIZES.contains(&n) || (n.is_power_of_two() && (512..=4096).contains(&n))
}

pub fn set_picture_override_field(
    ov: &mut PicturePlatformOverride,
    field: &str,
    value: &str,
) -> Result<(), CoreError> {
    if value == "default" || value.is_empty() {
        match field {
            "max_size" => ov.max_size = None,
            "compression" => ov.compression = None,
            "format" => ov.format = None,
            other => {
                return Err(CoreError::new(format!("unknown platform override field {other}")))
            }
        }
        return Ok(());
    }
    match field {
        "max_size" => {
            let n: u32 = value.parse().map_err(|_| CoreError::new("max size must be a number"))?;
            if n != 0 && !PICTURE_MAX_SIZES.contains(&n) {
                return Err(CoreError::new("max size must be 0 or a power of two from 32 to 4096"));
            }
            ov.max_size = Some(n);
        }
        "compression" if valid_compression(value) => ov.compression = Some(value.into()),
        "format" if valid_format(value) => ov.format = Some(value.into()),
        other => return Err(CoreError::new(format!("unknown platform override field {other}"))),
    }
    Ok(())
}

pub fn set_atlas_override_field(
    ov: &mut AtlasPlatformOverride,
    field: &str,
    value: &str,
) -> Result<(), CoreError> {
    if value == "default" || value.is_empty() {
        if field == "page_size" {
            ov.page_size = None;
            return Ok(());
        }
        return Err(CoreError::new(format!("unknown atlas platform field {field}")));
    }
    if field == "page_size" {
        let n: u32 = value.parse().map_err(|_| CoreError::new("page size must be a number"))?;
        if n != 0 && !valid_atlas_page(n) {
            return Err(CoreError::new(
                "page size must be 0 for default or a power of two from 512 to 4096",
            ));
        }
        ov.page_size = if n == 0 { None } else { Some(n) };
        return Ok(());
    }
    Err(CoreError::new(format!("unknown atlas platform field {field}")))
}

/// Picture look using platform overrides on top of the sidecar next to `path`.
pub fn picture_look_for_platform(
    path: &Path,
    platform: ImportPlatform,
) -> crate::import_presets::PictureLook {
    let mut look = picture_look(path);
    let sidecar = PathBuf::from(format!("{}.import.seal", path.display()));
    let Ok(text) = fs::read_to_string(&sidecar) else {
        return look;
    };
    let Ok(value) = seal_sealtext::parse(&text) else {
        return look;
    };
    let overrides = parse_platform_overrides(&value);
    let Some(ov) = overrides.get(&platform) else {
        return look;
    };
    if let Some(ms) = ov.max_size {
        look.max_size = ms;
    }
    look
}

/// Resize and re-encode one picture for the packaged `data` folder.
pub fn process_picture_for_export(
    project: &Path,
    data: &Path,
    rel: &str,
    platform: ImportPlatform,
) -> Result<Option<(u32, u32)>, CoreError> {
    let state = load_file_state(project, rel, ImportKind::Picture);
    let ImportSettings::Picture(ref base) = state.settings else {
        return Ok(None);
    };
    let ov = state.platform_overrides.get(&platform).cloned().unwrap_or_default();
    let effective = effective_picture(base, &ov);
    if effective.max_size == 0 && ov_is_empty(&ov) {
        return Ok(None);
    }
    let src = project.join(rel);
    let picture =
        seal_render::picture::decode_path(&src).map_err(|e| CoreError::new(e.to_string()))?;
    let (rgba, width, height) = seal_render::picture::downscale(
        &picture.rgba,
        picture.width,
        picture.height,
        effective.max_size,
    );
    let dest = data.join(rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| CoreError::new(e.to_string()))?;
    }
    let ext = dest.extension().and_then(|e| e.to_str()).unwrap_or("png").to_ascii_lowercase();
    let format = if effective.format == "keep" { ext.as_str() } else { effective.format.as_str() };
    seal_render::picture::write_rgba8(&dest, &rgba, width, height, format, &effective.compression)
        .map_err(|e| CoreError::new(e.to_string()))?;
    Ok(Some((width, height)))
}

/// Walk packaged picture files and apply the export platform overrides.
pub fn apply_pictures_for_platform(
    project: &Path,
    data: &Path,
    platform: ImportPlatform,
) -> Result<usize, CoreError> {
    let mut count = 0usize;
    let mut paths = Vec::new();
    collect_pictures(data, data, &mut paths)?;
    for rel in paths {
        if process_picture_for_export(project, data, &rel, platform)?.is_some() {
            count += 1;
        }
    }
    Ok(count)
}

fn collect_pictures(root: &Path, dir: &Path, out: &mut Vec<String>) -> std::io::Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            if dir == root && entry.file_name() == crate::atlas::FOLDER {
                continue;
            }
            collect_pictures(root, &path, out)?;
        } else if seal_render::picture::is_path(&path) {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import_presets::{save_file_state, FileImportState, PictureSettings};
    use std::fs;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "seal-import-platform-{}-{label}-{}",
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

    fn write_png(path: &Path, w: u32, h: u32) {
        let file = std::io::BufWriter::new(fs::File::create(path).unwrap());
        let mut encoder = png::Encoder::new(file, w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let pixels: Vec<u8> = (0..w * h).flat_map(|_| [10u8, 20, 30, 255]).collect();
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }

    #[test]
    fn platform_override_storage_round_trips_in_the_sidecar() {
        let dir = scratch("rt");
        fs::create_dir_all(dir.join("sprites")).unwrap();
        fs::write(dir.join("sprites/a.png"), b"x").unwrap();
        let mut state = FileImportState {
            settings: ImportSettings::Picture(PictureSettings::default()),
            customized: true,
            platform_overrides: PlatformOverrideMap::new(),
        };
        let ov = PicturePlatformOverride {
            max_size: Some(128),
            compression: Some("fast".into()),
            ..Default::default()
        };
        state.platform_overrides.insert(ImportPlatform::Android, ov);
        save_file_state(&dir, "sprites/a.png", &state).unwrap();
        let loaded = load_file_state(&dir, "sprites/a.png", ImportKind::Picture);
        assert_eq!(loaded.platform_overrides, state.platform_overrides);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn effective_settings_merge_base_and_override() {
        let base = PictureSettings { max_size: 512, ..PictureSettings::default() };
        let ov = PicturePlatformOverride {
            max_size: Some(64),
            compression: Some("maximum".into()),
            format: None,
        };
        let eff = effective_picture(&base, &ov);
        assert_eq!(eff.max_size, 64);
        assert_eq!(eff.compression, "maximum");
        assert_eq!(eff.format, "keep");
        let empty = effective_picture(&base, &PicturePlatformOverride::default());
        assert_eq!(empty.max_size, 512);
    }

    #[test]
    fn old_sidecar_without_platform_overrides_parses_unchanged() {
        let dir = scratch("old");
        fs::write(dir.join("pic.png"), b"png").unwrap();
        fs::write(
            dir.join("pic.png.import.seal"),
            "schema_version = 1\nkind = \"picture\"\ncustomized = true\nfilter = \"nearest\"\nmax_size = 256\n",
        )
        .unwrap();
        let state = load_file_state(&dir, "pic.png", ImportKind::Picture);
        assert!(state.platform_overrides.is_empty());
        if let ImportSettings::Picture(p) = state.settings {
            assert_eq!(p.max_size, 256);
            assert_eq!(p.filter, "nearest");
        } else {
            panic!("expected picture settings");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_applies_different_max_sizes_per_platform() {
        let dir = scratch("export");
        let data = dir.join("data");
        fs::create_dir_all(dir.join("sprites")).unwrap();
        fs::create_dir_all(data.join("sprites")).unwrap();
        write_png(&dir.join("sprites/wide.png"), 64, 32);
        fs::copy(dir.join("sprites/wide.png"), data.join("sprites/wide.png")).unwrap();
        let mut state = FileImportState {
            settings: ImportSettings::Picture(PictureSettings {
                max_size: 0,
                ..PictureSettings::default()
            }),
            customized: true,
            platform_overrides: PlatformOverrideMap::new(),
        };
        state.platform_overrides.insert(
            ImportPlatform::Windows,
            PicturePlatformOverride { max_size: Some(64), ..Default::default() },
        );
        state.platform_overrides.insert(
            ImportPlatform::Android,
            PicturePlatformOverride { max_size: Some(32), ..Default::default() },
        );
        save_file_state(&dir, "sprites/wide.png", &state).unwrap();
        let loaded = load_file_state(&dir, "sprites/wide.png", ImportKind::Picture);
        assert_eq!(loaded.platform_overrides.len(), 2);
        let win_dims =
            process_picture_for_export(&dir, &data, "sprites/wide.png", ImportPlatform::Windows)
                .unwrap()
                .expect("windows export");
        let _ = fs::remove_file(data.join("sprites/wide.png"));
        fs::copy(dir.join("sprites/wide.png"), data.join("sprites/wide.png")).unwrap();
        let android_dims =
            process_picture_for_export(&dir, &data, "sprites/wide.png", ImportPlatform::Android)
                .unwrap()
                .expect("android export");
        assert_eq!(win_dims.0, 64);
        assert_eq!(android_dims.0, 32);
        assert!(win_dims.0 > android_dims.0);
        let android = seal_render::picture::decode_path(&data.join("sprites/wide.png")).unwrap();
        assert_eq!(android.width, 32);
        let _ = fs::remove_dir_all(&dir);
    }
}
