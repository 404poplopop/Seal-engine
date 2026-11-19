//! Merge, migrate, and naming rules integration.

use seal_editor_core::{CommandArgs, EditorSession, Layout, ModalChoice};
use std::fs;
use std::path::PathBuf;

fn open_project(dir: &PathBuf) -> EditorSession {
    let mut s = EditorSession::new(Layout::default());
    s.dispatch(
        "file.open-project",
        &CommandArgs::new(&[("path", dir.to_string_lossy().as_ref())]),
    )
    .unwrap();
    s
}

#[test]
fn naming_scan_lists_violation_and_fix_renames() {
    let dir = std::env::temp_dir().join(format!("seal-naming-int-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("sprites")).unwrap();
    let id = format!("{:08x}-0000-7000-8000-{:012x}", std::process::id(), 77);
    fs::write(
        dir.join("project.seal"),
        format!(
            "schema_version = 1\nname = \"P\"\nid = \"{id}\"\ncreated = 1\neditor_version = \"0\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n"
        ),
    )
    .unwrap();
    seal_editor_core::naming_rules::write_template_defaults(&dir).unwrap();
    fs::write(dir.join("sprites/Hero.png"), b"pix").unwrap();
    let mut session = open_project(&dir);
    session.dispatch("naming.scan", &CommandArgs::default()).unwrap();
    assert!(!session.models.naming_rules.violations.is_empty());
    let path = session.models.naming_rules.violations[0].path.clone();
    session
        .dispatch("naming.fix", &CommandArgs::new(&[("path", path.as_str())]))
        .unwrap();
    if session.foundation.modal.is_some() {
        session.choose_modal(ModalChoice::Confirm);
    }
    assert!(dir.join("sprites/hero.png").is_file());
}

#[test]
fn new_and_imported_assets_are_checked_and_patterns_are_real_regular_expressions() {
    use seal_editor_core::naming_rules::{check_name, NamingRule, NamingRuleBook};
    let rule = |pattern: &str| NamingRuleBook {
        rules: vec![NamingRule {
            kind: "picture".into(),
            prefix: String::new(),
            suffix: String::new(),
            case: String::new(),
            pattern: pattern.into(),
        }],
        ..Default::default()
    };
    assert!(check_name(&rule(r"^[a-z]+_\d+$"), "sprites/tree_12.png").is_none());
    assert!(check_name(&rule(r"^[a-z]+_\d+$"), "sprites/tree.png").is_some());
    assert!(check_name(&rule("(["), "sprites/tree.png").unwrap().reason.contains("not a valid"));

    let dir = std::env::temp_dir().join(format!("seal-naming-new-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("sprites")).unwrap();
    let id = format!("{:08x}-0000-7000-8000-{:012x}", std::process::id(), 78);
    fs::write(
        dir.join("project.seal"),
        format!(
            "schema_version = 1\nname = \"P\"\nid = \"{id}\"\ncreated = 1\neditor_version = \"0\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n"
        ),
    )
    .unwrap();
    seal_editor_core::naming_rules::write_template_defaults(&dir).unwrap();
    let mut session = open_project(&dir);
    session
        .dispatch("asset.frames_new", &CommandArgs::new(&[("path", "sprites/walk"), ("unique", "yes")]))
        .unwrap();
    assert!(session.models.naming_rules.violations.is_empty(), "lowercase default names pass");
    session
        .dispatch("asset.frames_new", &CommandArgs::new(&[("path", "sprites/Walk"), ("unique", "yes")]))
        .unwrap();
    assert!(session.models.naming_rules.violations.iter().any(|v| v.path.ends_with("Walk.frames")));
}