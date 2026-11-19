//! Per-platform import override tabs in the Asset Editor.

use seal_editor_core::import_platform::{
    effective_picture, ALL_PLATFORMS, ImportPlatform, PicturePlatformOverride,
};
use seal_editor_core::import_presets::{load_file_state, ImportKind, PictureSettings, PICTURE_MAX_SIZES};
use seal_editor_core::EditorSession;
use seal_ui::PanelFrame;

use crate::{command, PanelEvent};

pub fn platform_overrides_block(
    frame: &mut PanelFrame,
    session: &mut EditorSession,
    rel: &str,
    base: &PictureSettings,
) -> PanelEvent {
    let title = session.messages.get("import-platform-title");
    let hint = session.messages.get("import-platform-hint");
    let tab_l = session.messages.get("import-platform-tab");
    let effective_l = session.messages.get("import-platform-effective");
    let default_l = session.messages.get("import-platform-inherit");
    let override_l = session.messages.get("import-platform-overridden");
    let max_l = session.messages.get("import-field-max-size");
    let max_original = session.messages.get("import-max-original");
    let compression_l = session.messages.get("import-platform-compression");
    let format_l = session.messages.get("import-platform-format");
    let inherit = session.messages.get("import-platform-field-inherit");
    let Some(root) = session.project.as_ref().map(|p| p.root.clone()) else {
        return PanelEvent::None;
    };
    let state = load_file_state(&root, rel, ImportKind::Picture);
    let tabs: Vec<(&str, String)> = ALL_PLATFORMS
        .iter()
        .map(|p| {
            (
                p.id(),
                session.messages.format("import-platform-name", &[("id", p.id())]),
            )
        })
        .collect();
    let mut tab = session
        .draft("import-platform-tab")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "windows".into());
    let mut event = PanelEvent::None;
    frame.section(&title, |f| {
        f.muted(&hint);
        if let Some(picked) = f.choice(&tab_l, &tab, &tabs) {
            if picked != tab {
                tab = picked;
                let _ = session.set_draft("import-platform-tab", &tab);
            }
        }
        let platform = ImportPlatform::from_id(&tab).unwrap_or(ImportPlatform::Windows);
        let ov = state.platform_overrides.get(&platform).cloned().unwrap_or_default();
        let eff = effective_picture(base, &ov);
        let eff_text = session.messages.format(
            &effective_l,
            &[
                ("max", &format_size(eff.max_size, &max_original)),
                ("compression", eff.compression.as_str()),
                ("format", eff.format.as_str()),
            ],
        );
        f.muted(&eff_text);
        if let Some(e) = platform_field_edits(f, rel, platform, &ov, &PlatformLabels {
            max: max_l,
            max_original: max_original.clone(),
            compression: compression_l,
            format: format_l,
            inherit: inherit.clone(),
            inherit_note: default_l,
            override_note: override_l,
        }) {
            event = e;
        }
    });
    event
}

struct PlatformLabels {
    max: String,
    max_original: String,
    compression: String,
    format: String,
    inherit: String,
    inherit_note: String,
    override_note: String,
}

fn platform_field_edits(
    frame: &mut PanelFrame,
    rel: &str,
    platform: ImportPlatform,
    ov: &PicturePlatformOverride,
    labels: &PlatformLabels,
) -> Option<PanelEvent> {
    let send =
        |field: &str, value: &str| {
            command(
                "import.platform.field",
                &[
                    ("path", rel),
                    ("platform", platform.id()),
                    ("field", field),
                    ("value", value),
                ],
            )
        };
    let mut sizes = vec![(labels.inherit.clone(), labels.inherit.clone())];
    sizes.push(("0".into(), labels.max_original.clone()));
    for size in PICTURE_MAX_SIZES {
        let text = size.to_string();
        sizes.push((text.clone(), text));
    }
    let size_pairs: Vec<(&str, String)> = sizes.iter().map(|(id, t)| (id.as_str(), t.clone())).collect();
    let current_size = ov
        .max_size
        .map(|n| n.to_string())
        .unwrap_or_else(|| labels.inherit.clone());
    if let Some(picked) = frame.choice(&labels.max, &current_size, &size_pairs) {
        if picked != current_size {
            let value = if picked == labels.inherit { "default" } else { picked.as_str() };
            return Some(send("max_size", value));
        }
    }
    if ov.max_size.is_some() {
        frame.muted(&labels.override_note);
    } else {
        frame.muted(&labels.inherit_note);
    }
    let compression_pairs = [
        ("default", labels.inherit.clone()),
        ("none", "None".into()),
        ("fast", "Fast".into()),
        ("balanced", "Balanced".into()),
        ("maximum", "Maximum".into()),
    ];
    let comp: Vec<(&str, String)> = compression_pairs.iter().map(|(id, t)| (*id, t.clone())).collect();
    let current_comp = ov
        .compression
        .clone()
        .unwrap_or_else(|| labels.inherit.clone());
    let comp_key = ov.compression.as_deref().unwrap_or("default");
    if let Some(picked) = frame.choice(&labels.compression, &current_comp, &comp) {
        let picked_key = if picked == labels.inherit {
            "default"
        } else {
            comp.iter().find(|(_, t)| *t == picked).map(|(id, _)| *id).unwrap_or(comp_key)
        };
        if picked_key != comp_key {
            return Some(send("compression", picked_key));
        }
    }
    let format_pairs = [
        ("default", labels.inherit.clone()),
        ("keep", "Keep source".into()),
        ("png", "PNG".into()),
        ("jpeg", "JPEG".into()),
        ("webp", "WebP".into()),
    ];
    let fmt: Vec<(&str, String)> = format_pairs.iter().map(|(id, t)| (*id, t.clone())).collect();
    let current_fmt = ov.format.clone().unwrap_or_else(|| labels.inherit.clone());
    let fmt_key = ov.format.as_deref().unwrap_or("default");
    if let Some(picked) = frame.choice(&labels.format, &current_fmt, &fmt) {
        let picked_key = if picked == labels.inherit {
            "default"
        } else {
            fmt.iter().find(|(_, t)| *t == picked).map(|(id, _)| *id).unwrap_or(fmt_key)
        };
        if picked_key != fmt_key {
            return Some(send("format", picked_key));
        }
    }
    None
}

fn format_size(max_size: u32, original: &str) -> String {
    if max_size == 0 {
        original.to_string()
    } else {
        max_size.to_string()
    }
}

pub fn atlas_platform_section(frame: &mut PanelFrame, session: &mut EditorSession) -> PanelEvent {
    let title = session.messages.get("import-atlas-platform-title");
    let hint = session.messages.get("import-atlas-platform-hint");
    let tab_l = session.messages.get("import-platform-tab");
    let page_l = session.messages.get("import-atlas-page-size");
    let inherit = session.messages.get("import-platform-field-inherit");
    let default_l = session.messages.get("import-atlas-page-default");
    let mut event = PanelEvent::None;
    let tabs: Vec<(&str, String)> = ALL_PLATFORMS
        .iter()
        .map(|p| {
            (
                p.id(),
                session.messages.format("import-platform-name", &[("id", p.id())]),
            )
        })
        .collect();
    let mut tab = session
        .draft("import-atlas-platform-tab")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "windows".into());
    frame.section(&title, |f| {
        f.muted(&hint);
        if let Some(picked) = f.choice(&tab_l, &tab, &tabs) {
            if picked != tab {
                tab = picked;
                let _ = session.set_draft("import-atlas-platform-tab", &tab);
            }
        }
        let platform = ImportPlatform::from_id(&tab).unwrap_or(ImportPlatform::Windows);
        let ov = session
            .models
            .import_presets
            .atlas_platform_overrides
            .get(&platform)
            .cloned()
            .unwrap_or_default();
        let mut sizes = vec![(inherit.clone(), inherit.clone())];
        sizes.push((default_l.clone(), default_l.clone()));
        for size in PICTURE_MAX_SIZES {
            if *size >= 512 {
                let text = size.to_string();
                sizes.push((text.clone(), text));
            }
        }
        let pairs: Vec<(&str, String)> = sizes.iter().map(|(id, t)| (id.as_str(), t.clone())).collect();
        let current = ov
            .page_size
            .map(|n| n.to_string())
            .unwrap_or_else(|| inherit.clone());
        let key = ov.page_size.map(|n| n.to_string()).unwrap_or_else(|| "default".into());
        if let Some(picked) = f.choice(&page_l, &current, &pairs) {
            let picked_key = if picked == inherit {
                "default"
            } else if picked == default_l {
                "0"
            } else {
                picked.as_str()
            };
            let current_key = if key == "default" { "default" } else { key.as_str() };
            if picked_key != current_key {
                event = command(
                    "import.atlas.platform_field",
                    &[("platform", platform.id()), ("field", "page_size"), ("value", picked_key)],
                );
            }
        }
    });
    event
}
