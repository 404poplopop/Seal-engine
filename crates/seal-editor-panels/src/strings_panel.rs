//! Two-column editor for a string table.

use std::path::Path;

use egui::TextEdit;
use seal_editor_core::{EditorSession, StringTable};
use seal_ui::PanelFrame;

use crate::{command, PanelEvent};

pub(crate) fn strings_editor(
    frame: &mut PanelFrame,
    session: &EditorSession,
    _rel: &str,
    full: &Path,
) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let mut event = PanelEvent::None;
    let raw = match std::fs::read_to_string(full) {
        Ok(raw) => raw,
        Err(e) => {
            frame.padded(|frame| frame.muted(&e.to_string()));
            return event;
        }
    };
    let table = match StringTable::parse(&raw) {
        Ok(table) => table,
        Err(e) => {
            frame.padded(|frame| frame.muted(&e));
            return event;
        }
    };
    frame.bar(|frame| {
        if frame.button(&text("asset-strings-add")) {
            event = command("asset.strings_add", &[]);
        }
    });
    let ctx = frame.ui_mut().ctx().clone();
    frame.scroll(|frame| {
        frame.padded(|frame| {
            for (index, (key, value)) in table.rows.iter().enumerate() {
                let id = egui::Id::new(("string-key", full.to_path_buf(), index));
                let mut draft: String = ctx.data(|d| d.get_temp(id)).unwrap_or_else(|| key.clone());
                let key_response = frame.ui_mut().horizontal(|ui| {
                    let response =
                        ui.add(TextEdit::singleline(&mut draft).desired_width(160.0).id(id));
                    if ui.button(text("asset-strings-remove")).clicked() {
                        event = command("asset.strings_remove", &[("key", key.as_str())]);
                    }
                    response
                });
                let key_response = key_response.inner;
                if key_response.changed() {
                    ctx.data_mut(|d| d.insert_temp(id, draft.clone()));
                }
                if key_response.lost_focus() && draft.trim() != key {
                    ctx.data_mut(|d| d.remove::<String>(id));
                    event = command(
                        "asset.strings_rename",
                        &[("from", key.as_str()), ("to", draft.trim())],
                    );
                }
                let value_id = egui::Id::new(("string-value", full.to_path_buf(), index));
                let mut line: String =
                    ctx.data(|d| d.get_temp(value_id)).unwrap_or_else(|| value.clone());
                let value_response = frame
                    .ui_mut()
                    .add(TextEdit::singleline(&mut line).desired_width(f32::INFINITY).id(value_id));
                if value_response.changed() && &line != value {
                    event = command(
                        "asset.strings_set",
                        &[("key", key.as_str()), ("value", line.as_str())],
                    );
                }
                if value_response.lost_focus() {
                    ctx.data_mut(|d| d.remove::<String>(value_id));
                } else if value_response.changed() {
                    ctx.data_mut(|d| d.insert_temp(value_id, line));
                }
            }
            if table.rows.is_empty() {
                frame.empty(&text("asset-strings-empty"));
            }
        });
    });
    event
}
