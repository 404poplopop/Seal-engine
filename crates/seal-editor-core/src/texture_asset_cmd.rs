//! Commands for composite texture assets (`.texture` files).

use std::path::{Path, PathBuf};

use seal_render::{GpuBlockFormat, TextureAsset, TextureAssetKind};

use crate::asset_editor::{open_asset, show_asset, AssetKind};

fn checked_rel(rel: &str) -> Result<String, CoreError> {
    let rel = rel.trim().replace('\\', "/");
    if rel.is_empty() || rel.starts_with('/') || rel.split('/').any(|p| p == "..") {
        return Err(CoreError::new(format!("'{rel}' is not a path inside the project")));
    }
    Ok(rel)
}

fn project_root(session: &EditorSession) -> Result<PathBuf, CoreError> {
    session
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .ok_or_else(|| CoreError::new("open a project first"))
}
use crate::command::CommandArgs;
use crate::error::CoreError;
use crate::session::EditorSession;

pub const TEXTURE_SLICE_DRAFT: &str = "asset.texture.slice";
pub const TEXTURE_ENCODE_BYTES: &str = "asset.texture.encode.bytes";
pub const TEXTURE_ENCODE_PSNR: &str = "asset.texture.encode.psnr";

fn open_texture(session: &EditorSession) -> Result<(String, PathBuf, TextureAsset), CoreError> {
    let (rel, full) = open_asset(session)
        .ok_or_else(|| CoreError::new("no asset is open in the Asset Editor"))?;
    if crate::asset_editor::asset_kind(&rel) != AssetKind::TextureComposite {
        return Err(CoreError::new("the open file is not a composite texture"));
    }
    let text = std::fs::read_to_string(&full).map_err(|e| CoreError::new(e.to_string()))?;
    let asset = TextureAsset::parse(&text).map_err(|e| CoreError::new(e.to_string()))?;
    Ok((rel, full, asset))
}

fn write_texture(full: &Path, asset: &TextureAsset) -> Result<(), CoreError> {
    crate::persist::atomic_write(full, asset.to_text().as_bytes())?;
    Ok(())
}

fn unique_path(root: &Path, mut rel: String, suffix: &str) -> Result<(String, PathBuf), CoreError> {
    let mut full = root.join(&rel);
    if full.exists() {
        let stem = rel.trim_end_matches(suffix).to_string();
        let mut n = 2;
        while full.exists() {
            rel = format!("{stem}_{n}{suffix}");
            full = root.join(&rel);
            n += 1;
        }
    }
    Ok((rel, full))
}

fn cmd_texture_new(
    session: &mut EditorSession,
    args: &CommandArgs,
    kind: TextureAssetKind,
) -> Result<(), CoreError> {
    let default = match kind {
        TextureAssetKind::Array2d => "textures/array",
        TextureAssetKind::Volume3d => "textures/volume",
        TextureAssetKind::Cubemap => "textures/cubemap",
    };
    let mut rel = checked_rel(args.get("path").unwrap_or(default))?;
    if !rel.ends_with(".texture") {
        rel.push_str(".texture");
    }
    let root = project_root(session)?;
    let unique = args.get("unique") == Some("yes");
    let (rel, full) = if unique {
        unique_path(&root, rel, ".texture")?
    } else if root.join(&rel).exists() {
        return Err(CoreError::new(format!("{rel} already exists")));
    } else {
        (rel.clone(), root.join(&rel))
    };
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut asset = TextureAsset {
        kind,
        ..TextureAsset::default()
    };
    if let Some(pictures) = args.get("pictures") {
        asset.pictures = pictures.split(';').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect();
    }
    if let Some(w) = args.get("width").and_then(|v| v.parse::<u32>().ok()) {
        asset.width = w.max(1);
    }
    if let Some(h) = args.get("height").and_then(|v| v.parse::<u32>().ok()) {
        asset.height = h.max(1);
    }
    if kind == TextureAssetKind::Volume3d {
        if let Some(d) = args.get("depth").and_then(|v| v.parse::<u32>().ok()) {
            asset.depth = d.max(1);
        }
    }
    write_texture(&full, &asset)?;
    show_asset(session, &rel)
}

pub(crate) fn cmd_texture_array_new(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    cmd_texture_new(session, args, TextureAssetKind::Array2d)
}

pub(crate) fn cmd_texture_volume_new(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    cmd_texture_new(session, args, TextureAssetKind::Volume3d)
}

pub(crate) fn cmd_texture_cubemap_new(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    cmd_texture_new(session, args, TextureAssetKind::Cubemap)
}

pub(crate) fn cmd_texture_set(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let (rel, full, mut asset) = open_texture(session)?;
    let root = project_root(session)?;
    if let Some(v) = args.get("gpu_format") {
        asset.gpu_format = GpuBlockFormat::parse(v)
            .ok_or_else(|| CoreError::new(format!("unknown gpu format {v}")))?;
    }
    if let Some(v) = args
        .get("memory_budget_mb")
        .and_then(|v| v.parse::<u32>().ok())
    {
        asset.memory_budget_mb = v.max(1);
    }
    if let Some(v) = args.get("streaming") {
        asset.streaming = matches!(v, "1" | "yes" | "true");
    }
    if let Some(v) = args.get("width").and_then(|v| v.parse::<u32>().ok()) {
        asset.width = v.max(1);
    }
    if let Some(v) = args.get("height").and_then(|v| v.parse::<u32>().ok()) {
        asset.height = v.max(1);
    }
    if let Some(ix) = args.get("layer").and_then(|v| v.parse::<usize>().ok()) {
        if let Some(picture) = args.get("picture") {
            let picture = checked_rel(picture)?;
            while asset.pictures.len() <= ix {
                asset.pictures.push(String::new());
            }
            let rel_pic = picture.clone();
            asset.pictures[ix] = picture;
            if root.join(&rel_pic).is_file() {
                if let Some((_, w, h)) = crate::asset_editor::decode_png(&root.join(&rel_pic)) {
                    asset.width = w;
                    asset.height = h;
                }
            }
        }
    }
    if let Some(pictures) = args.get("pictures") {
        asset.pictures =
            pictures.split(';').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect();
    }
    write_texture(&full, &asset)?;
    session.notify(&format!("Saved {rel}"), false);
    Ok(())
}

/// Encode export pixels for the chosen block format using current adapter feature tokens from args.
pub(crate) fn cmd_texture_export_encode(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let (rel, full, asset) = open_texture(session)?;
    let root = project_root(session)?;
  let quality = args
    .get("quality")
    .and_then(|v| seal_render::EncodeQuality::parse(v))
    .unwrap_or(seal_render::EncodeQuality::Normal);
    let format = crate::texture_ktx_export::format_for_texture_asset(asset.gpu_format, quality);
    let mut total_bytes = 0usize;
    let mut worst_psnr = 99.0_f64;
    for (layer, picture) in asset.pictures.iter().enumerate() {
        if picture.trim().is_empty() {
            continue;
        }
        let pic_path = root.join(picture);
        let (rgba, w, h) = crate::asset_editor::decode_png(&pic_path)
            .ok_or_else(|| CoreError::new(format!("{picture}: could not read picture")))?;
        let encoded = seal_render::texture_encode::encode_rgba(format, quality, w, h, &rgba)
            .map_err(|e| CoreError::new(e.to_string()))?;
        let decoded = seal_render::texture_encode::decode_to_rgba(format, w, h, &encoded)
            .map_err(|e| CoreError::new(e.to_string()))?;
        let psnr = seal_render::psnr_rgba(&rgba, &decoded);
        worst_psnr = worst_psnr.min(psnr);
        total_bytes += encoded.len();
        let ktx = seal_render::write_ktx2(format, true, w, h, &[encoded])
            .map_err(|e| CoreError::new(e.to_string()))?;
        let stem = full.file_stem().and_then(|s| s.to_str()).unwrap_or("texture");
        let out = full
            .parent()
            .unwrap_or(&root)
            .join(format!("{stem}.layer{layer}.ktx2"));
        crate::persist::atomic_write(&out, &ktx)?;
    }
    session.set_draft(TEXTURE_ENCODE_BYTES, &total_bytes.to_string())?;
    session.set_draft(TEXTURE_ENCODE_PSNR, &format!("{worst_psnr:.1}"))?;
    session.notify(&format!("Encoded {rel} for export"), false);
    Ok(())
}