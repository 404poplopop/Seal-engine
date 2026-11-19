//! Message ids built from runtime values rather than string literals in sources.

use std::path::PathBuf;

use crate::command::MENU_CATEGORIES;
use crate::settings::setting_catalog;

/// Fluent id for a built-in setting title.
pub fn setting_title_key(id: &str) -> String {
    match id {
        "ui.theme" => "setting-theme".to_string(),
        "ui.scale" => "setting-scale".to_string(),
        other => format!("setting-{}", normalize_setting_id(other)),
    }
}

/// Fluent id for a built-in setting description.
pub fn setting_description_key(id: &str) -> String {
    format!("setting-desc-{}", normalize_setting_id(id))
}

fn normalize_setting_id(id: &str) -> String {
    id.replace(['.', '_'], "-")
}

/// Ids addressed through `format!("prefix-{value}")` or setting metadata.
pub fn referenced_dynamic_ids() -> Vec<String> {
    let mut ids = Vec::new();
    for meta in setting_catalog() {
        ids.push(setting_title_key(meta.id));
        ids.push(setting_description_key(meta.id));
        ids.push(format!("settings-category-{}", meta.category));
    }
    for cat in MENU_CATEGORIES {
        ids.push(format!("menu-{cat}"));
    }
    for kind in crate::asset_browser::FILTER_KINDS.iter().chain(["folder"].iter()) {
        ids.push(format!("content-kind-{kind}"));
    }
    for (prefix, values) in [
        ("view", ["front", "top", "side"].as_slice()),
        ("viewport", &["move", "rotate", "scale"]),
        ("theme", &["dark", "light", "contrast"]),
        ("density", &["comfortable", "compact"]),
        ("quality-level", &["low", "medium", "high", "ultra"]),
        ("project-dimension", &["2d", "3d"]),
        ("editor-language", &["en"]),
        ("setting-flag", &["true", "false"]),
        ("shape", &["none", "cube", "sphere", "cylinder", "cone", "capsule", "plane"]),
        ("motion", &["static", "dynamic"]),
        (
            "place",
            &[
                "shapes", "scene", "cube", "sphere", "cylinder", "cone", "capsule", "plane",
                "empty", "camera", "light",
            ],
        ),
    ] {
        for value in values {
            ids.push(format!("{prefix}-{value}"));
        }
    }
    // Set by the world model in seal-world.
    ids.push("outliner-warning-motion-mode".into());
    // Reasons the in-game UI model (seal-world gameui) gives for refusing a value.
    for key in [
        "anchor",
        "offset",
        "size",
        "color",
        "number",
        "flag",
        "selected",
        "columns",
        "font-size",
        "font",
        "align",
        "property",
        "pair",
    ] {
        ids.push(format!("ui-error-{key}"));
    }
    // Reasons seal-world fonts gives for refusing a font file.
    for key in ["empty", "unreadable", "no-letters"] {
        ids.push(format!("font-error-{key}"));
    }
    ids.push("setting-source-user".into());
    ids.push("setting-source-project".into());
    ids
}

/// Editor sources scanned for static message id references.
pub fn english_catalog_source_roots() -> Vec<PathBuf> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    vec![
        workspace.join("crates/seal-editor-core/src"),
        workspace.join("crates/seal-editor-shell/src"),
        workspace.join("crates/seal-editor-panels/src"),
    ]
}

/// Run the English catalog check used in CI.
pub fn verify_english_catalog() -> Result<(), String> {
    let ftl = include_str!("../locales/en.ftl");
    let roots = english_catalog_source_roots();
    let extra = referenced_dynamic_ids();
    seal_i18n::audit::check_english_catalog(ftl, &roots, &extra)
}
