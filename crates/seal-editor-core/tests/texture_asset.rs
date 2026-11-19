//! Composite texture asset commands and SealText round-trip.

use seal_editor_core::{create_project, asset_editor::ASSET_DRAFT, CommandArgs, EditorSession, Layout};
use seal_render::{TextureAsset, TextureAssetKind};
use std::fs;

fn project_with_png() -> (std::path::PathBuf, EditorSession) {
    // Each test gets its own folder: the tests run in parallel.
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("seal-tex-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    create_project(&dir, "Game").unwrap();
    fs::create_dir_all(dir.join("textures")).unwrap();
    let png = dir.join("textures/layer0.png");
    {
        let raw: Vec<u8> = std::iter::repeat([1u8, 2, 3, 255]).take(16).flatten().collect();
        let mut enc = png::Encoder::new(std::fs::File::create(&png).unwrap(), 4, 4);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&raw).unwrap();
    }
    let mut session = EditorSession::new(Layout::default());
    session
        .dispatch(
            "file.open-project",
            &CommandArgs::new(&[("path", dir.to_str().unwrap())]),
        )
        .unwrap();
    (dir, session)
}

#[test]
fn texture_array_new_writes_and_opens() {
    let (_dir, mut session) = project_with_png();
    session
        .dispatch(
            "asset.texture_array_new",
            &CommandArgs::new(&[
                ("path", "textures/stack"),
                ("unique", "yes"),
                ("pictures", "textures/layer0.png"),
                ("width", "4"),
                ("height", "4"),
            ]),
        )
        .unwrap();
    let rel = session.draft(ASSET_DRAFT).unwrap();
    assert!(rel.ends_with(".texture"));
    let full = session.project.as_ref().unwrap().root.join(&rel);
    let asset = TextureAsset::parse(&fs::read_to_string(full).unwrap()).unwrap();
    assert_eq!(asset.kind, TextureAssetKind::Array2d);
    assert_eq!(asset.pictures.len(), 1);
}

#[test]
fn texture_set_gpu_format() {
    let (dir, mut session) = project_with_png();
    session
        .dispatch(
            "asset.texture_array_new",
            &CommandArgs::new(&[("path", "textures/stack"), ("unique", "yes")]),
        )
        .unwrap();
    session
        .dispatch(
            "asset.texture_set",
            &CommandArgs::new(&[("gpu_format", "bc1"), ("memory_budget_mb", "64")]),
        )
        .unwrap();
    let rel = session.draft(ASSET_DRAFT).unwrap();
    let full = dir.join(rel);
    let asset = TextureAsset::parse(&fs::read_to_string(full).unwrap()).unwrap();
    assert_eq!(asset.gpu_format.name(), "bc1");
    assert_eq!(asset.memory_budget_mb, 64);
}

#[test]
fn texture_export_encode_writes_ktx2() {
    let (dir, mut session) = project_with_png();
    session
        .dispatch(
            "asset.texture_array_new",
            &CommandArgs::new(&[
                ("path", "textures/stack"),
                ("unique", "yes"),
                ("pictures", "textures/layer0.png"),
            ]),
        )
        .unwrap();
    session
        .dispatch(
            "asset.texture_set",
            &CommandArgs::new(&[("gpu_format", "bc1")]),
        )
        .unwrap();
    session
        .dispatch(
            "asset.texture_export_encode",
            &CommandArgs::new(&[("layer", "0"), ("quality", "balanced")]),
        )
        .unwrap();
    let rel = session.draft(ASSET_DRAFT).unwrap();
    let full = dir.join(&rel);
    let stem = full.file_stem().unwrap().to_string_lossy();
    let ktx = full.with_file_name(format!("{stem}.layer0.ktx2"));
    assert!(ktx.is_file());
    assert!(!fs::read(&ktx).unwrap().is_empty());
}