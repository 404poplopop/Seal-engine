//! Inspector panel for a `.data` asset picked in the Asset Browser.

use seal_editor_core::{
    data_editor::load_data_asset, hex_to_rgba, rgba_to_hex, save_recent_colours, recent_colours,
    EditorSession, Rgba,
};
use seal_ui::colour_picker::{self, ColourLabels};
use seal_ui::PanelFrame;
use seal_editor_core::{DataFieldDef, FieldKind};

use crate::inspector_pickers::{paint_asset_row, AssetRow};
use crate::{command, PanelEvent};

pub fn paint_data_inspector(
    frame: &mut PanelFrame,
    session: &EditorSession,
    rel: &str,
) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let Some(root) = session
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .or_else(|| session.models.content_tree_root.clone())
    else {
        frame.empty(&text("inspector-empty"));
        return PanelEvent::None;
    };
    let Ok((_, asset, type_def)) = load_data_asset(&root, rel) else {
        frame.empty(&text("inspector-data-load-failed"));
        return PanelEvent::None;
    };
    let mut event = PanelEvent::None;
    frame.padded(|outer| {
        outer.label(&text("inspector-data-title"));
        outer.muted(&format!("{} · {}", asset.name, type_def.name));
        outer.space(8.0);
        outer.scroll(|scroll| {
            for field_def in &type_def.fields {
                let value = asset
                    .values
                    .iter()
                    .find(|(name, _)| name == &field_def.name)
                    .map(|(_, v)| v.as_str())
                    .unwrap_or(field_def.default.as_str());
                if let Some(next) = paint_data_field(scroll, session, rel, field_def, value, &text) {
                    event = next;
                }
            }
        });
    });
    event
}

fn paint_data_field(
    frame: &mut PanelFrame,
    session: &EditorSession,
    rel: &str,
    field: &DataFieldDef,
    value: &str,
    text: &dyn Fn(&str) -> String,
) -> Option<PanelEvent> {
    let label = &field.name;
    match field.kind {
        FieldKind::Number => {
            let mut draft = value.to_string();
            let (changed, _, commit) = frame.property_validated_row(
                label, &mut draft, None, false, None, None, None, &mut None, None,
            );
            if (changed || commit) && draft != value {
                if let Ok(n) = draft.parse::<f64>() {
                    if n.is_finite() {
                        return Some(command(
                            "asset.data_set",
                            &[("path", rel), ("field", &field.name), ("value", draft.as_str())],
                        ));
                    }
                }
            }
        }
        FieldKind::Text | FieldKind::List => {
            let mut draft = value.to_string();
            let (changed, _, commit) = frame.property_validated_row(
                label, &mut draft, None, false, None, None, None, &mut None, None,
            );
            if (changed || commit) && draft != value {
                return Some(command(
                    "asset.data_set",
                    &[("path", rel), ("field", &field.name), ("value", draft.as_str())],
                ));
            }
        }
        FieldKind::Bool => {
            let checked = if matches!(value, "true" | "yes" | "1") {
                seal_ui::PropertyCheckbox::On
            } else {
                seal_ui::PropertyCheckbox::Off
            };
            if let Some(next) =
                frame.property_checkbox_row(label, checked, false, None, None, &mut None, None)
            {
                let stored = match next {
                    seal_ui::PropertyCheckbox::On => "yes",
                    _ => "no",
                };
                return Some(command(
                    "asset.data_set",
                    &[("path", rel), ("field", &field.name), ("value", stored)],
                ));
            }
        }
        FieldKind::Choice => {
            let options: Vec<(&str, String)> = field
                .options
                .split('|')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|opt| (opt, opt.to_string()))
                .collect();
            if let Some(picked) = frame
                .choice_row(label, value, &options, false, None, None, None, &mut None, None)
                .0
            {
                if picked != value {
                    return Some(command(
                        "asset.data_set",
                        &[("path", rel), ("field", &field.name), ("value", picked.as_str())],
                    ));
                }
            }
        }
        FieldKind::Colour => {
            if let Some(next) = paint_data_colour(frame, session, rel, &field.name, label, value) {
                return Some(next);
            }
        }
        FieldKind::Asset => {
            match paint_asset_row(frame, session, "data_ref", &field.name, label, value, false) {
                AssetRow::Set(path) => {
                    return Some(command(
                        "asset.data_set",
                        &[("path", rel), ("field", &field.name), ("value", path.as_str())],
                    ));
                }
                AssetRow::Clear => {
                    return Some(command(
                        "asset.data_set",
                        &[("path", rel), ("field", &field.name), ("value", "")],
                    ));
                }
                AssetRow::Show => {
                    return Some(command("picker.show_asset", &[("path", value)]));
                }
                AssetRow::Idle => {}
            }
        }
    }
    let _ = text("inspector-data-field");
    None
}

fn paint_data_colour(
    frame: &mut PanelFrame,
    session: &EditorSession,
    rel: &str,
    field: &str,
    label: &str,
    current: &str,
) -> Option<PanelEvent> {
    let ui = frame.ui_mut();
    colour_picker::seed_recent(ui, &recent_colours(session));
    let draft_id = ui.id().with((rel, field, "data-colour"));
    let parsed =
        hex_to_rgba(current).unwrap_or(Rgba { r: 1.0, g: 1.0, b: 1.0, a: 1.0 });
    let mut rgba = ui
        .data(|data| data.get_temp::<[f32; 4]>(draft_id))
        .unwrap_or([parsed.r, parsed.g, parsed.b, parsed.a]);
    let mut recent = colour_picker::recent_now(ui);
    let mut change = colour_picker::ColourChange { changed: false, commit: false };
    ui.horizontal(|ui| {
        seal_ui::property_name(ui, label, false, None);
        let (rect, response) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
        let fill = egui::Color32::from_rgba_unmultiplied(
            (rgba[0] * 255.0) as u8,
            (rgba[1] * 255.0) as u8,
            (rgba[2] * 255.0) as u8,
            (rgba[3] * 255.0) as u8,
        );
        ui.painter().rect_filled(rect.shrink(1.0), 2.0, fill);
        let text = |id: &str| session.messages.get(id);
        egui::Popup::menu(&response).show(|ui| {
            change = colour_picker::edit(
                ui,
                &mut rgba,
                &mut recent,
                &ColourLabels {
                    hex: text("picker-hex"),
                    alpha: text("picker-alpha"),
                    recent: text("picker-recent"),
                },
            );
        });
        ui.label(rgba_to_hex(Rgba {
            r: rgba[0],
            g: rgba[1],
            b: rgba[2],
            a: rgba[3],
        }));
    });
    if change.changed {
        ui.data_mut(|data| data.insert_temp(draft_id, rgba));
    }
    colour_picker::store_recent(ui, recent.clone());
    if change.commit {
        ui.data_mut(|data| data.remove::<[f32; 4]>(draft_id));
        let hex = rgba_to_hex(Rgba { r: rgba[0], g: rgba[1], b: rgba[2], a: rgba[3] });
        let _ = save_recent_colours(&recent);
        return Some(command(
            "asset.data_set",
            &[("path", rel), ("field", field), ("value", hex.as_str())],
        ));
    }
    None
}