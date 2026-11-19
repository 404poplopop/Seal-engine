//! Export writes KTX2 sidecars for platform compression presets.

use std::fs;
use std::path::Path;

use seal_editor_core::export_preset::ExportPreset;
use seal_editor_core::import_platform::ImportPlatform;
use seal_editor_core::import_presets::{
    save_file_state, FileImportState, ImportSettings, PictureSettings,
};
use seal_editor_core::package;
use seal_render::picture;

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "seal-ktx-export-{}-{name}-{}",
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

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) {
    let file = fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(rgba).unwrap();
}

#[test]
fn export_writes_ktx2_for_windows_and_mobile_presets() {
    let dir = scratch("presets");
    let project = dir.join("Game");
    fs::create_dir_all(project.join("art")).unwrap();
    fs::write(
        project.join("project.seal"),
        "schema_version = 1\nname = \"Game\"\nid = \"00000000-0000-7000-8000-000000000001\"\ncreated = 1\neditor_version = \"0.2\"\nmodules = []\nsettings = {}\n",
    )
    .unwrap();
    let rgba = vec![200u8, 40, 10, 255].repeat(16);
    write_png(&project.join("art/tile.png"), 4, 4, &rgba);

    let win_state = FileImportState {
        settings: ImportSettings::Picture(PictureSettings {
            compression: "high".into(),
            ..Default::default()
        }),
        customized: true,
        platform_overrides: Default::default(),
    };
    save_file_state(&project, "art/tile.png", &win_state).unwrap();

    let exe = dir.join("seal-editor");
    fs::write(&exe, b"program").unwrap();
    let out_win = dir.join("Win Out");
    package::package_filtered_with(
        &project,
        "Game",
        &exe,
        &out_win,
        &|_| true,
        &[],
        true,
        false,
        "release",
        "",
        Some((ImportPlatform::Windows, 0)),
        "",
        &ExportPreset::named("Default", "out").atlas_options(),
        false,
    )
    .unwrap();
    let ktx_win = out_win.join("data/art/tile.ktx2");
    assert!(ktx_win.is_file());
    let img = seal_render::read_ktx2(&fs::read(&ktx_win).unwrap()).unwrap();
    assert_eq!(img.format, seal_render::GpuBlockFormat::Bc7);

    let mobile_state = FileImportState {
        settings: ImportSettings::Picture(PictureSettings {
            compression: "low".into(),
            ..Default::default()
        }),
        customized: true,
        platform_overrides: Default::default(),
    };
    save_file_state(&project, "art/tile.png", &mobile_state).unwrap();
    let out_ios = dir.join("Ios Out");
    package::package_filtered_with(
        &project,
        "Game",
        &exe,
        &out_ios,
        &|_| true,
        &[],
        true,
        false,
        "release",
        "",
        Some((ImportPlatform::Ios, 0)),
        "",
        &ExportPreset::named("Default", "out").atlas_options(),
        false,
    )
    .unwrap();
    let ktx_ios = out_ios.join("data/art/tile.ktx2");
    assert!(ktx_ios.is_file());
    let img_ios = seal_render::read_ktx2(&fs::read(&ktx_ios).unwrap()).unwrap();
    assert_eq!(img_ios.format, seal_render::GpuBlockFormat::Etc2);

    let none_state = FileImportState {
        settings: ImportSettings::Picture(PictureSettings::default()),
        customized: false,
        platform_overrides: Default::default(),
    };
    save_file_state(&project, "art/tile.png", &none_state).unwrap();
    let png_before = fs::read(project.join("art/tile.png")).unwrap();
    let out_plain = dir.join("Plain Out");
    package::package_filtered_with(
        &project,
        "Game",
        &exe,
        &out_plain,
        &|_| true,
        &[],
        true,
        false,
        "release",
        "",
        Some((ImportPlatform::Windows, 0)),
        "",
        &ExportPreset::named("Default", "out").atlas_options(),
        false,
    )
    .unwrap();
    let png_out = out_plain.join("data/art/tile.png");
    assert_eq!(fs::read(&png_out).unwrap(), png_before);
    assert!(!out_plain.join("data/art/tile.ktx2").exists());
    let decoded = picture::decode_path(&png_out).unwrap();
    assert_eq!(decoded.rgba, rgba);
    let _ = fs::remove_dir_all(&dir);
}