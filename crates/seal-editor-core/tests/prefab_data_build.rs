//! Prefab variants, data assets, build list, templates.

use seal_editor_core::{
    apply_user_template, create_project, create_project_with_options, package,
    save_project_as_template, user_template_by_id, write_build_scenes, CommandArgs,
    CreateProjectOptions, EditorSession, Layout,
};
use std::fs;

#[test]
fn build_list_is_written_into_game_marker() {
    let root = std::env::temp_dir().join(format!("seal-f13-build-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let dir = root.join("Hop");
    create_project(&dir, "Hop").unwrap();
    fs::write(dir.join("scene.seal"), seal_world::World::empty_scene_text("Hop")).unwrap();
    std::fs::create_dir_all(dir.join("levels")).unwrap();
    fs::write(dir.join("levels/b.seal"), seal_world::World::empty_scene_text("B")).unwrap();
    write_build_scenes(&dir, &["levels/b.seal".into(), "scene.seal".into()]).unwrap();
    let exe = root.join("seal-editor.exe");
    fs::write(&exe, b"program").unwrap();
    let out = package::default_package_folder(&dir, "Hop");
    package::package_project(&dir, "Hop", &exe, &out).unwrap();
    let marker = fs::read_to_string(out.join(package::GAME_MARKER)).unwrap();
    assert!(marker.contains("start levels/b.seal"));
}

#[test]
fn user_template_save_and_apply() {
    let src = std::env::temp_dir().join(format!("seal-f13-tpl-src-{}", std::process::id()));
    let dst = std::env::temp_dir().join(format!("seal-f13-tpl-dst-{}", std::process::id()));
    let _ = fs::remove_dir_all(&src);
    let _ = fs::remove_dir_all(&dst);
    create_project(&src, "Source").unwrap();
    fs::write(src.join("scene.seal"), seal_world::World::empty_scene_text("Source")).unwrap();
    save_project_as_template(&src, "Platform Kit", "A saved project", None).unwrap();
    assert!(user_template_by_id("platform_kit").is_some());
    create_project_with_options(&dst, "Clone", &CreateProjectOptions::default()).unwrap();
    apply_user_template(&dst, "Clone", "platform_kit").unwrap();
    assert!(dst.join("scene.seal").is_file());
}

#[test]
fn data_assets_round_trip() {
    let dir = std::env::temp_dir().join(format!("seal-f13-data-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    create_project(&dir, "Game").unwrap();
    let mut session = EditorSession::new(Layout::default());
    session
        .dispatch("file.open_project", &CommandArgs::new(&[("path", dir.to_str().unwrap())]))
        .unwrap();
    session
        .dispatch(
            "asset.datatype_new",
            &CommandArgs::new(&[("path", "data/stats"), ("unique", "yes")]),
        )
        .unwrap();
    session
        .dispatch(
            "asset.data_new",
            &CommandArgs::new(&[
                ("path", "data/hero"),
                ("type", "data/stats.datatype"),
                ("unique", "yes"),
            ]),
        )
        .unwrap();
    session
        .dispatch("asset.data_set", &CommandArgs::new(&[("field", "value"), ("value", "7")]))
        .unwrap();
    let text = fs::read_to_string(dir.join("data/hero.data")).unwrap();
    assert!(text.contains("value value = 7"));
}
