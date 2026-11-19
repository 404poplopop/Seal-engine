//! Write the packed picture and the tight mesh when a picture is reimported or exported.

use std::path::Path;

use seal_render::picture;

use crate::error::CoreError;
use crate::import_presets::{load_file_state, ImportKind, ImportSettings, PictureSettings};

pub fn sync_mesh(root: &Path, rel: &str, settings: &PictureSettings) {
    let dest = root.join(format!("{rel}.spmesh"));
    if settings.mesh != "tight" {
        let _ = std::fs::remove_file(&dest);
        return;
    }
    let Ok(decoded) = picture::decode_path_with(&root.join(rel), settings.svg_size) else {
        return;
    };
    let (rgba, width, height) =
        picture::downscale(&decoded.rgba, decoded.width, decoded.height, settings.max_size);
    let mesh = picture::tight_mesh(&rgba, width, height);
    let _ = std::fs::write(dest, picture::write_mesh_text(&mesh));
}

/// Pack pictures in an exported project. SVG is rasterised at its import size.
/// A compression other than none replaces what the game reads, leaving the source file in place.
pub fn apply_package(data_root: &Path) -> Result<(), CoreError> {
    if !data_root.is_dir() {
        return Ok(());
    }
    let mut files = Vec::new();
    collect(data_root, data_root, &mut files)?;
    for rel in files {
        let ext = rel.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        if !picture::is_extension(&ext) {
            continue;
        }
        let state = load_file_state(data_root, &rel, ImportKind::Picture);
        let ImportSettings::Picture(settings) = &state.settings else { continue };
        if settings.mesh == "tight" {
            sync_mesh(data_root, &rel, settings);
        }
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
