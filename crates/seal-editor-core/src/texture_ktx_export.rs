//! Write KTX2 sidecars when a game is exported.

use std::path::{Path, PathBuf};

use seal_render::{
    export_block_format, picture, write_ktx2_from_rgba, EncodeQuality, GpuBlockFormat,
};

use crate::error::CoreError;
use crate::import_platform::ImportPlatform;
use crate::import_presets::{load_file_state, ImportKind, ImportSettings};

pub fn ktx2_sidecar(path: &Path) -> PathBuf {
    path.with_extension("ktx2")
}

pub fn apply_package(
    project: &Path,
    data_root: &Path,
    platform: ImportPlatform,
) -> Result<(), CoreError> {
    if !data_root.is_dir() {
        return Ok(());
    }
    let platform_id = platform.id();
    let mut files = Vec::new();
    collect(data_root, data_root, &mut files)?;
    for rel in files {
        let ext = rel.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        if !picture::is_extension(&ext) || ext == "ktx2" || ext == "dds" {
            continue;
        }
        let state = load_file_state(project, &rel, ImportKind::Picture);
        let ImportSettings::Picture(ref base) = state.settings else {
            continue;
        };
        let ov = state.platform_overrides.get(&platform).cloned().unwrap_or_default();
        let compression = ov
            .compression
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(base.compression.as_str());
        let Some((format, quality)) = export_block_format(&platform_id, compression, None) else {
            continue;
        };
        let full = data_root.join(&rel);
        let decoded = picture::decode_path(&full).map_err(|e| CoreError::new(e.to_string()))?;
        let srgb = base.colour_space != "linear";
        let mips = base.mipmaps;
        let ktx = write_ktx2_from_rgba(
            format,
            srgb,
            decoded.width,
            decoded.height,
            &decoded.rgba,
            mips,
            quality,
        )
        .map_err(|e| CoreError::new(e.to_string()))?;
        std::fs::write(ktx2_sidecar(&full), ktx)?;
    }
    Ok(())
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), CoreError> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, out)?;
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

pub fn format_for_texture_asset(gpu: GpuBlockFormat, quality: EncodeQuality) -> GpuBlockFormat {
    if gpu != GpuBlockFormat::Rgba8 {
        gpu
    } else {
        match quality {
            EncodeQuality::Fast => GpuBlockFormat::Bc1,
            EncodeQuality::Normal => GpuBlockFormat::Bc3,
            EncodeQuality::Best => GpuBlockFormat::Bc7,
        }
    }
}