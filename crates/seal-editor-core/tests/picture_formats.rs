//! SVG raster size, tight mesh file, and export compression.

use std::fs;
use std::path::Path;

use seal_editor_core::import_presets::{
    reimport_asset, save_file_state, FileImportState, ImportSettings, PictureSettings,
};
use seal_editor_core::package;
use seal_editor_core::picture_import;
use seal_render::picture;

fn scratch(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("seal-picture-formats-{name}-{}", std::process::id()));
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
fn svg_size_rasterises_again_and_export_reads_the_packed_picture() {
    let dir = scratch("svg");
    let project = dir.join("Game");
    fs::create_dir_all(project.join("sprites")).unwrap();
    fs::write(
        project.join("project.seal"),
        "schema_version = 1\nname = \"Game\"\nid = \"00000000-0000-7000-8000-000000000001\"\ncreated = 1\neditor_version = \"0.2\"\nmodules = []\nsettings = {}\n",
    )
    .unwrap();
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="4" viewBox="0 0 8 4"><rect width="8" height="4" fill="#00ff00"/></svg>"##;
    fs::write(project.join("sprites/mark.svg"), svg).unwrap();
    let loaded = picture_import::load(&project.join("sprites/mark.svg")).unwrap();
    assert_eq!((loaded.width, loaded.height), (256, 128));

    let state = FileImportState {
        settings: ImportSettings::Picture(PictureSettings { svg_size: 64, ..Default::default() }),
        customized: true,
        platform_overrides: Default::default(),
    };
    save_file_state(&project, "sprites/mark.svg", &state).unwrap();
    let again = picture_import::load(&project.join("sprites/mark.svg")).unwrap();
    assert_eq!((again.width, again.height), (64, 32));
    assert_eq!(&again.rgba[..4], &[0, 255, 0, 255]);

    let mut blob = vec![0u8; 8 * 8 * 4];
    for y in 1..3 {
        for x in 1..3 {
            let i = (y * 8 + x) * 4;
            blob[i] = 255;
            blob[i + 3] = 255;
        }
    }
    write_png(&project.join("sprites/blob.png"), 8, 8, &blob);
    let tight = FileImportState {
        settings: ImportSettings::Picture(PictureSettings {
            mesh: "tight".into(),
            compression: "lossless".into(),
            ..Default::default()
        }),
        customized: true,
        platform_overrides: Default::default(),
    };
    reimport_asset(&project, "sprites/blob.png", &tight).unwrap();
    let mesh_text = fs::read_to_string(project.join("sprites/blob.png.spmesh")).unwrap();
    let mesh = picture::read_mesh_text(&mesh_text).unwrap();
    assert!(picture::covers_opaque(&blob, 8, 8, &mesh));
    assert!(picture::polygon_area(&mesh) < 0.5);

    let exe = dir.join("seal-editor");
    fs::write(&exe, b"program").unwrap();
    let out = dir.join("Game Out");
    package::package_project(&project, "Game", &exe, &out).unwrap();
    let packed = out.join("data/sprites/blob.png");
    let ktx = out.join("data/sprites/blob.ktx2");
    assert!(ktx.is_file());
    let decoded = picture::decode_path(&packed).unwrap();
    assert_eq!(decoded.rgba, blob);
    assert!(decoded.mesh.is_some());
    let _ = fs::remove_dir_all(&dir);
}
