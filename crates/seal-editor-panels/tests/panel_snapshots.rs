//! Headless panel title / AccessKit snapshots at two themes and scales.

use seal_editor_core::{body_font_px, theme_rgb, EditorSession, Layout};
use seal_editor_panels::{builtin_descs, show};
use seal_ui::{panel_accesskit_snapshot, PanelFrame, ThemeColors};
use std::fs;

const SNAPSHOT_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots/panel_trees.txt");

#[test]
fn panel_theme_snapshots_match_committed_text() {
    // The default projects folder depends on the machine, so it is written as
    // <projects>; line endings are compared without carriage returns.
    let projects = seal_editor_core::default_projects_dir().display().to_string();
    // The Assistant panel's setup snippet names this test binary; the JSON doubles backslashes.
    let exe = std::env::current_exe().unwrap().display().to_string().replace('\\', "\\\\");
    // The build hash changes with every commit; the snapshot keeps a placeholder.
    let actual = render_all_snapshots()
        .replace(&projects, "<projects>")
        .replace(&exe, "<exe>")
        .replace(seal_editor_core::ENGINE_HASH, "<hash>");
    let path = std::path::Path::new(SNAPSHOT_PATH);
    if std::env::var("SEAL_UPDATE_SNAPSHOTS").is_ok() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("snapshot dir");
        }
        fs::write(path, &actual).expect("write snapshot");
        return;
    }
    let expected =
        fs::read_to_string(path).expect("missing committed snapshot file").replace('\r', "");
    if actual != expected {
        eprintln!("snapshot mismatch; set SEAL_UPDATE_SNAPSHOTS=1 to refresh");
    }
    assert_eq!(actual, expected);
}

fn render_all_snapshots() -> String {
    let session = std::cell::RefCell::new(EditorSession::new(Layout::default()));
    let body_px = body_font_px(1.0);
    let mut out = String::new();
    for desc in builtin_descs() {
        for theme in ["dark", "light"] {
            let (background, text, accent) = theme_rgb(theme);
            let colors = ThemeColors { background, text, accent, look: None };
            for scale in [1.0f32, 2.0] {
                let panel_id = desc.id.clone();
                let snapshot = panel_accesskit_snapshot(theme, scale, body_px, colors, |ui| {
                    let mut frame = PanelFrame::new(ui);
                    let _ = show(&mut frame, &panel_id, &mut session.borrow_mut());
                });
                out.push_str(&section_header(&desc.id, theme, scale));
                out.push_str(&snapshot);
                if !out.ends_with("\n\n") {
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                    out.push('\n');
                }
            }
        }
    }
    out.push_str("=== dock layout default ===\n");
    out.push_str(&Layout::default().to_value().to_text());
    out.push('\n');
    out
}

fn section_header(panel: &str, theme: &str, scale: f32) -> String {
    format!("=== panel {panel} {theme} {scale:.2} ===\n")
}
