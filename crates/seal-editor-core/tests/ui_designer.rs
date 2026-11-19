//! The screen designer: every edit is written to the .sealui / .sealtheme file at once.

use seal_editor_core::asset_editor::ASSET_DRAFT;
use seal_editor_core::gameui::{self, Screen, Theme};
use seal_editor_core::ui_designer::UI_NODE_DRAFT;
use seal_editor_core::{CommandArgs, EditorSession, Layout};
use std::fs;
use std::path::{Path, PathBuf};

fn project(tag: &str) -> (PathBuf, EditorSession) {
    let dir = std::env::temp_dir().join(format!("seal-ui-designer-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let project = dir.join("P");
    fs::create_dir_all(&project).unwrap();
    let id = format!("{:08x}-0000-7000-9045-{:012x}", std::process::id(), tag.len());
    fs::write(project.join("project.seal"), format!("schema_version = 1\nname = \"P\"\nid = \"{id}\"\ncreated = 1794814358\neditor_version = \"0.01+7bdf2aadb05a\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n")).unwrap();
    let path = project.to_string_lossy().to_string();
    let mut s = EditorSession::new(Layout::default());
    s.dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())])).unwrap();
    (project, s)
}

fn run(s: &mut EditorSession, cmd: &str, args: &[(&str, &str)]) -> Result<(), String> {
    s.dispatch(cmd, &CommandArgs::new(args)).map_err(|e| e.to_string())
}

fn screen(project: &Path, rel: &str) -> Screen {
    Screen::from_text(&fs::read_to_string(project.join(rel)).unwrap()).unwrap()
}

#[test]
fn a_new_screen_is_made_opened_and_never_overwritten() {
    let (project, mut s) = project("new");
    run(&mut s, "asset.ui_new", &[("path", "ui/pause")]).unwrap();
    assert_eq!(s.draft(ASSET_DRAFT).as_deref(), Some("ui/pause.sealui"));
    assert_eq!(s.draft(UI_NODE_DRAFT).as_deref(), Some("Menu"));
    assert_eq!(screen(&project, "ui/pause.sealui"), Screen::starter());
    // A second one with the same name gets its own file.
    run(&mut s, "asset.ui_new", &[("path", "ui/pause")]).unwrap();
    assert_eq!(s.draft(ASSET_DRAFT).as_deref(), Some("ui/pause_2.sealui"));
    assert!(project.join("ui/pause.sealui").exists());
    assert!(run(&mut s, "asset.ui_new", &[("path", "../outside")]).is_err());
    assert!(!project.parent().unwrap().join("outside.sealui").exists());
}

#[test]
fn every_designer_edit_is_saved_at_once() {
    let (project, mut s) = project("edit");
    run(&mut s, "asset.ui_new", &[("path", "ui/menu")]).unwrap();
    let rel = "ui/menu.sealui";
    // Add into the picked column; the new node becomes the pick.
    run(&mut s, "asset.ui_add", &[("kind", "slider")]).unwrap();
    let picked = s.draft(UI_NODE_DRAFT).unwrap();
    let sc = screen(&project, rel);
    let i = sc.find(&picked).unwrap();
    assert_eq!(sc.nodes[i].kind, "slider");
    assert_eq!(sc.nodes[i].parent, sc.find("Menu"));

    run(&mut s, "asset.ui_set", &[("key", "name"), ("value", "Volume")]).unwrap();
    assert_eq!(s.draft(UI_NODE_DRAFT).as_deref(), Some("Volume"));
    run(&mut s, "asset.ui_set", &[("node", "Volume"), ("key", "value"), ("value", "0.25")])
        .unwrap();
    assert_eq!(
        screen(&project, rel).nodes[screen(&project, rel).find("Volume").unwrap()].value,
        0.25
    );

    // Bad values are refused with a plain reason and the file is unchanged.
    let before = fs::read_to_string(project.join(rel)).unwrap();
    let e = run(&mut s, "asset.ui_set", &[("node", "Volume"), ("key", "min"), ("value", "lots")])
        .unwrap_err();
    assert!(e.contains("a number"), "{e}");
    let e = run(&mut s, "asset.ui_set", &[("node", "Volume"), ("key", "name"), ("value", "Title")])
        .unwrap_err();
    assert!(e.contains("already"), "{e}");
    assert!(run(&mut s, "asset.ui_set", &[("node", "Nobody"), ("key", "text"), ("value", "x")])
        .is_err());
    assert_eq!(fs::read_to_string(project.join(rel)).unwrap(), before);

    // Reorder inside the column.
    run(&mut s, "asset.ui_move", &[("node", "Volume"), ("dir", "up")]).unwrap();
    let sc = screen(&project, rel);
    let order: Vec<_> = sc.children(sc.find("Menu")).map(|j| sc.nodes[j].name.clone()).collect();
    assert_eq!(order, ["Title", "Volume", "Play"]);

    // Pin the column to the top-right corner and drag it.
    run(&mut s, "asset.ui_set", &[("node", "Menu"), ("key", "preset"), ("value", "top-right")])
        .unwrap();
    let sc = screen(&project, rel);
    let m = &sc.nodes[sc.find("Menu").unwrap()];
    assert_eq!(m.anchor, [1.0, 0.0, 1.0, 0.0]);
    assert_eq!(m.offset, [-280.0, 0.0, 0.0, 160.0]);
    run(&mut s, "asset.ui_nudge", &[("node", "Menu"), ("dx", "-12"), ("dy", "8")]).unwrap();
    run(&mut s, "asset.ui_nudge", &[("node", "Menu"), ("dx", "30"), ("edge", "right")]).unwrap();
    let sc = screen(&project, rel);
    assert_eq!(sc.nodes[sc.find("Menu").unwrap()].offset, [-292.0, 8.0, 18.0, 168.0]);

    // Removing takes the children too and picks the parent.
    run(&mut s, "asset.ui_remove", &[("node", "Menu")]).unwrap();
    assert_eq!(s.draft(UI_NODE_DRAFT).as_deref(), Some("Screen"));
    let sc = screen(&project, rel);
    assert_eq!(sc.nodes.len(), 1);

    // What the file holds is what a running game loads.
    let live = gameui::load(&project, rel).unwrap();
    assert_eq!(live.screen, sc);
}

#[test]
fn themes_are_made_and_edited_from_the_screen_or_on_their_own() {
    let (project, mut s) = project("theme");
    run(&mut s, "asset.ui_new", &[("path", "ui/hud")]).unwrap();
    assert!(run(&mut s, "asset.theme_set", &[("key", "accent"), ("value", "#FF0000")]).is_err());
    run(&mut s, "asset.theme_new", &[]).unwrap();
    assert_eq!(screen(&project, "ui/hud.sealui").theme, "ui/hud.sealtheme");
    run(&mut s, "asset.theme_set", &[("key", "accent"), ("value", "#FF8800")]).unwrap();
    run(&mut s, "asset.theme_set", &[("key", "font_size"), ("value", "22")]).unwrap();
    let e = run(&mut s, "asset.theme_set", &[("key", "font_size"), ("value", "2")]).unwrap_err();
    assert!(e.contains("4 to 400"), "{e}");
    let t =
        Theme::from_text(&fs::read_to_string(project.join("ui/hud.sealtheme")).unwrap()).unwrap();
    assert_eq!(t.get("accent").as_deref(), Some("#FF8800"));
    assert_eq!(t.get("font_size").as_deref(), Some("22"));
    let live = gameui::load(&project, "ui/hud.sealui").unwrap();
    assert_eq!(live.theme, t);
    // Opened on its own, the theme file is edited directly.
    run(&mut s, "asset.open", &[("path", "ui/hud.sealtheme")]).unwrap();
    run(&mut s, "asset.theme_set", &[("key", "corner"), ("value", "6")]).unwrap();
    let t =
        Theme::from_text(&fs::read_to_string(project.join("ui/hud.sealtheme")).unwrap()).unwrap();
    assert_eq!(t.get("corner").as_deref(), Some("6"));
    // A screen can point at another theme, but only a theme file.
    assert!(run(&mut s, "asset.open", &[("path", "ui/hud.sealui")]).is_ok());
    assert!(run(&mut s, "asset.ui_set", &[("key", "theme"), ("value", "ui/look.png")]).is_err());
    run(&mut s, "asset.ui_set", &[("key", "theme"), ("value", "")]).unwrap();
    assert_eq!(screen(&project, "ui/hud.sealui").theme, "");
}

#[test]
fn every_file_kind_has_a_filter_and_a_label() {
    for name in [
        "a.seal",
        "a.prefab",
        "a.png",
        "a.wav",
        "a.fx",
        "a.txt",
        "a.sealui",
        "a.sealtheme",
        "a.anim",
        "a.bin",
        "project.seal",
        "a.ttf",
        "a.OTF",
    ] {
        let kind = seal_editor_core::kind_of(name, false);
        assert!(
            seal_editor_core::FILTER_KINDS.contains(&kind),
            "{name} has kind {kind} with no filter"
        );
        assert!(
            seal_editor_core::referenced_dynamic_ids().contains(&format!("content-kind-{kind}")),
            "{name}: no label for {kind}"
        );
    }
    seal_editor_core::verify_english_catalog().expect("catalog");
}
