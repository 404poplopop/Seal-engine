//! Project rendering quality, platform overrides, 2D mode and editor language.

use std::fs;
use std::path::PathBuf;

use seal_editor_core::{
    apply_level_to_session, load_platform_overrides, render_quality_from_session,
    store_platform_overrides, CommandArgs, EditorSession, Layout, PlatformOverrides,
    PROJECT_DIMENSION_SETTING,
};

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("seal-proj-settings-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn export_uses_platform_override_for_display_width() {
    let dir = scratch("override");
    let project = dir.join("Demo");
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join("project.seal"),
        "schema_version = 1\nname = \"Demo\"\nid = \"demoid01-0000-7000-8000-000000000001\"\ncreated = 1\neditor_version = \"0.01\"\nmodules = []\nsettings = { \"display.width\" = \"1920\" }\nignore = []\n",
    )
    .unwrap();
    let mut book = PlatformOverrides::default();
    book.set("display.width", "android", "720");
    store_platform_overrides(&project, &book).unwrap();
    let mut session = EditorSession::new(Layout::default());
    session
        .dispatch("file.open-project", &CommandArgs::new(&[("path", project.to_str().unwrap())]))
        .unwrap();
    assert_eq!(
        seal_editor_core::effective_from_session(&session, "display.width", "android"),
        "720"
    );
    assert_eq!(
        seal_editor_core::effective_from_session(&session, "display.width", "linux"),
        "1920"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn opening_old_project_leaves_settings_table_unchanged_on_disk() {
    let dir = scratch("old");
    let project = dir.join("Old");
    fs::create_dir_all(&project).unwrap();
    let manifest = "schema_version = 1\nname = \"Old\"\nid = \"old-id\"\ncreated = 1\neditor_version = \"0.01\"\nmodules = []\nsettings = {}\nignore = []\n";
    fs::write(project.join("project.seal"), manifest).unwrap();
    let before = fs::read_to_string(project.join("project.seal")).unwrap();
    let mut session = EditorSession::new(Layout::default());
    session
        .dispatch("file.open-project", &CommandArgs::new(&[("path", project.to_str().unwrap())]))
        .unwrap();
    let dim = session
        .models
        .settings
        .iter()
        .find(|row| row.id == PROJECT_DIMENSION_SETTING)
        .map(|row| row.value())
        .unwrap_or("3d");
    assert_eq!(dim, "3d");
    assert_eq!(fs::read_to_string(project.join("project.seal")).unwrap(), before);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn editor_language_rejects_unknown_catalog() {
    let mut session = EditorSession::new(Layout::default());
    assert!(session
        .dispatch(
            "settings.set",
            &CommandArgs::new(&[("id", "ui.editor_language"), ("value", "xx")]),
        )
        .is_err());
}

#[test]
fn platform_overrides_persist_in_project_seal() {
    let dir = scratch("persist");
    let project = dir.join("Save");
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join("project.seal"),
        "schema_version = 1\nname = \"Save\"\nid = \"save-id\"\ncreated = 1\neditor_version = \"0.01\"\nmodules = []\nsettings = {}\nignore = []\n",
    )
    .unwrap();
    let mut book = PlatformOverrides::default();
    book.set("display.height", "ios", "480");
    store_platform_overrides(&project, &book).unwrap();
    let loaded = load_platform_overrides(&project);
    assert_eq!(loaded.get("display.height", "ios"), Some("480"));
    let text = fs::read_to_string(project.join("project.seal")).unwrap();
    assert!(text.contains("settings.platform_overrides"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn quality_lowering_turns_off_runtime_glow_flag() {
    let mut session = EditorSession::new(Layout::default());
    apply_level_to_session(&mut session, "low").unwrap();
    let quality = render_quality_from_session(&session);
    assert!(!quality.glow);
}
