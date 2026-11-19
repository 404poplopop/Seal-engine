//! Single archive export matches a folder copy, and the runtime can read the pack.

use std::fs;
use std::path::PathBuf;

use seal_editor_core::export_preset::ExportPreset;
use seal_editor_core::game_pack;
use seal_editor_core::package;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("seal-gpack-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn pack_contents_match_data_folder() {
    let root = scratch("cmp");
    let project = root.join("project");
    fs::create_dir_all(project.join("sprites")).unwrap();
    fs::write(project.join("project.seal"), "id test\n").unwrap();
    fs::write(project.join("sprites/a.png"), b"pixels").unwrap();
    let folder_out = root.join("folder");
    let pack_out = root.join("pack");
    let exe = root.join("game.exe");
    fs::write(&exe, b"bin").unwrap();
    package::package_filtered_with(
        &project,
        "Cmp",
        &exe,
        &folder_out,
        &|_| true,
        &[],
        true,
        false,
        "",
        "",
        None,
        "",
        &seal_editor_core::atlas::AtlasOptions::default(),
        false,
    )
    .unwrap();
    package::package_filtered_with(
        &project,
        "Cmp",
        &exe,
        &pack_out,
        &|_| true,
        &[],
        true,
        false,
        "",
        "",
        None,
        "",
        &seal_editor_core::atlas::AtlasOptions::default(),
        true,
    )
    .unwrap();
    let packed = fs::read(pack_out.join("data.pack")).unwrap();
    let map = game_pack::read_pack(&packed).unwrap();
    for rel in ["project.seal", "sprites/a.png"] {
        let folder_bytes = fs::read(folder_out.join("data").join(rel)).unwrap();
        assert_eq!(map.get(rel).map(Vec::as_slice), Some(folder_bytes.as_slice()));
    }
    let marker = fs::read_to_string(pack_out.join("game.txt")).unwrap();
    assert!(marker.contains("project pack data.pack"));
    let mounted = package::game_project_beside(&pack_out.join("Cmp.exe")).unwrap();
    assert_eq!(fs::read(mounted.join("sprites/a.png")).unwrap(), b"pixels");
    assert_eq!(fs::read(mounted.join("project.seal")).unwrap(), b"id test\n");
}

#[test]
fn msix_target_writes_manifest_layout() {
    let root = scratch("msix");
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("project.seal"), "id test\n").unwrap();
    let exe = root.join("game.exe");
    fs::write(&exe, b"bin").unwrap();
    let out = root.join("out");
    let mut preset = ExportPreset::named("Store", &out.display().to_string());
    preset.package_target = "msix".into();
    preset.single_archive = true;
    seal_editor_core::export_preset::export_preset(&project, "Hop", &exe, &preset, "title Hop\n", "")
        .unwrap();
    let layout = out.join("msix_layout");
    let xml = fs::read_to_string(layout.join("AppxManifest.xml")).unwrap();
    assert!(xml.contains("<DisplayName>Hop</DisplayName>"));
    assert!(xml.contains(r#"Executable="Hop.exe""#));
    assert!(xml.contains("Windows.Desktop"));
    assert!(layout.join("Assets").join("Logo.png").is_file());
    assert!(layout.join("data.pack").is_file());
    assert!(layout.join("game.txt").is_file());
    let note = fs::read_to_string(out.join("msix-build.txt")).unwrap_or_default();
    let built = out.join("Hop.msix").is_file();
    assert!(built || note.contains("MakeAppx"), "{note}");
}
