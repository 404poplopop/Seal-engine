//! The manual's command and panel reference is generated from the editor itself,
//! so every command and panel the editor has is named in the manual.
//! Refresh with `SEAL_UPDATE_SNAPSHOTS=1 cargo test -p seal-editor-panels --test manual_reference`.

use seal_editor_core::{EditorSession, Layout, PanelRegistry};
use std::path::Path;

fn reference() -> String {
    let session = EditorSession::new(Layout::default());
    let mut out = String::from(
        "# Command and panel reference\n\nThis page is generated from the editor. Every command can be run from the command palette (Ctrl+Shift+P); commands marked with a menu name are also in that menu.\n",
    );
    let mut categories: Vec<String> = Vec::new();
    for spec in session.commands.specs() {
        if spec.alias_of.is_none() && !categories.contains(&spec.category) {
            categories.push(spec.category.clone());
        }
    }
    for category in &categories {
        let mut name = category.clone();
        if let Some(first) = name.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        out += &format!("\n## {name}\n\n| Command | Id | Shortcut | In menu |\n| ------- | -- | -------- | ------- |\n");
        for spec in
            session.commands.specs().filter(|s| s.alias_of.is_none() && &s.category == category)
        {
            let title = session.messages.get(&spec.title_key);
            let shortcut = session.keymap.shortcut(&spec.id);
            let menu = if spec.in_menu { name.as_str() } else { "" };
            out += &format!(
                "| {title} | `{}` | {} | {menu} |\n",
                spec.id,
                shortcut.replace('|', "\\|")
            );
        }
    }
    let mut panels = PanelRegistry::default();
    seal_editor_panels::register_builtin(&mut panels).unwrap();
    out +=
        "\n## Panels\n\nOpen any panel from the Window menu.\n\n| Panel | Id |\n| ----- | -- |\n";
    for panel in panels.iter() {
        out += &format!("| {} | `{}` |\n", session.messages.get(&panel.title_key), panel.id);
    }
    out
}

#[test]
fn the_manual_names_every_command_and_panel() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/book/src/user/reference.md");
    let text = reference();
    if std::env::var_os("SEAL_UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    let saved = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
    assert!(
        saved == text,
        "docs/book/src/user/reference.md is out of date; refresh it with SEAL_UPDATE_SNAPSHOTS=1"
    );
}

#[test]
fn every_user_guide_page_is_in_the_table_of_contents() {
    let book = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/book/src");
    let summary = std::fs::read_to_string(book.join("SUMMARY.md")).unwrap();
    for entry in std::fs::read_dir(book.join("user")).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().to_string();
        assert!(
            summary.contains(&format!("(user/{name})")),
            "user/{name} is missing from SUMMARY.md"
        );
    }
}
