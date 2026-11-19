//! Picture import settings round-trip, old sidecars, and Max Size.

use std::fs;
use std::path::Path;

use seal_editor_core::import_presets::{
    load_file_state, picture_look, reimport_asset, save_file_state, FileImportState, ImportKind,
    ImportSettings, PictureLook, PictureSettings,
};
use seal_editor_core::picture_import;

fn scratch(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("seal-picture-import-{name}-{}", std::process::id()));
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
fn every_picture_field_round_trips_and_old_sidecars_stay_legacy() {
    let dir = scratch("round");
    let custom = PictureSettings {
        filter: "linear".into(),
        pixels_per_unit: 16.0,
        max_size: 64,
        mipmaps: true,
        colour_space: "linear".into(),
        wrap: "mirror".into(),
        sprite_mode: "multiple".into(),
        pivot: "bottom_left".into(),
        pivot_x: 0.0,
        pivot_y: 0.0,
        svg_size: 128,
        compression: "low".into(),
        mesh: "tight".into(),
    };
    let state = FileImportState {
        settings: ImportSettings::Picture(custom.clone()),
        customized: true,
        platform_overrides: Default::default(),
    };
    fs::write(dir.join("hero.png"), b"png").unwrap();
    save_file_state(&dir, "hero.png", &state).unwrap();
    let loaded = load_file_state(&dir, "hero.png", ImportKind::Picture);
    assert_eq!(loaded, state);

    let old = "\
schema_version = 1
kind = \"picture\"
customized = false
filter = \"nearest\"
pixels_per_unit = 0
";
    fs::write(dir.join("old.png"), b"png").unwrap();
    fs::write(dir.join("old.png.import.seal"), old).unwrap();
    let old_state = load_file_state(&dir, "old.png", ImportKind::Picture);
    assert_eq!(old_state.settings, ImportSettings::Picture(PictureSettings::default()));
    assert!(!old_state.customized);
    let look = picture_look(&dir.join("old.png"));
    assert_eq!(look, PictureLook::legacy());
    assert!(!look.colour_explicit);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn max_size_downscales_on_load_and_reimport_survives_a_damaged_file() {
    let dir = scratch("size");
    let rgba: Vec<u8> = (0..2)
        .flat_map(|_| (0..4).flat_map(|x| if x < 2 { [255, 0, 0, 255] } else { [0, 0, 255, 255] }))
        .collect();
    write_png(&dir.join("wide.png"), 4, 2, &rgba);
    let state = FileImportState {
        settings: ImportSettings::Picture(PictureSettings { max_size: 2, ..Default::default() }),
        customized: true,
        platform_overrides: Default::default(),
    };
    save_file_state(&dir, "wide.png", &state).unwrap();
    let loaded = picture_import::load(&dir.join("wide.png")).unwrap();
    assert_eq!((loaded.width, loaded.height), (2, 1));
    assert_eq!(loaded.rgba, [255, 0, 0, 255, 0, 0, 255, 255]);
    assert_eq!(loaded.look.max_size, 2);

    reimport_asset(&dir, "wide.png", &state).unwrap();
    let cached = fs::read_to_string(dir.join(".seal-editor/import-cache/wide.png.size")).unwrap();
    assert_eq!(cached, "2 1\n");

    fs::write(dir.join("bad.png"), b"png").unwrap();
    let broken = FileImportState {
        settings: ImportSettings::Picture(PictureSettings::default()),
        customized: false,
        platform_overrides: Default::default(),
    };
    reimport_asset(&dir, "bad.png", &broken).unwrap();
    let _ = fs::remove_dir_all(&dir);
}
