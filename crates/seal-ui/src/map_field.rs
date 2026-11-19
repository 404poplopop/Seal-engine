//! Map and nested struct rows for the inspector.

use crate::crisp::CrispCombo as _;
use seal_editor_core::FieldValue;

pub struct RecordLabels {
    pub add: String,
    pub remove: String,
    pub key: String,
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordEvent {
    Idle,
    Set { path: String, kind: String, value: String },
    Remove { path: String },
    Add { parent: String, key: String, kind: String },
}

const KINDS: &[&str] = &["text", "number", "bool", "colour", "map", "struct"];

pub fn record_editor(
    ui: &mut egui::Ui,
    salt: &str,
    value: &FieldValue,
    labels: &RecordLabels,
) -> RecordEvent {
    let mut event = RecordEvent::Idle;
    paint_value(ui, salt, "", value, labels, &mut event, 0);
    event
}

fn paint_value(
    ui: &mut egui::Ui,
    salt: &str,
    path: &str,
    value: &FieldValue,
    labels: &RecordLabels,
    event: &mut RecordEvent,
    depth: usize,
) {
    if depth > 8 {
        return;
    }
    match value {
        FieldValue::Map(rows) | FieldValue::Struct { fields: rows, .. } => {
            if let FieldValue::Struct { type_name, .. } = value {
                ui.label(type_name);
            }
            for (key, child) in rows {
                let child_path =
                    if path.is_empty() { key.clone() } else { format!("{path}/{key}") };
                ui.horizontal(|ui| {
                    ui.label(key);
                    if ui.button(&labels.remove).clicked() {
                        *event = RecordEvent::Remove { path: child_path.clone() };
                    }
                });
                ui.indent(child_path.clone(), |ui| {
                    paint_value(ui, salt, &child_path, child, labels, event, depth + 1);
                });
            }
            add_row(ui, salt, path, labels, event);
        }
        FieldValue::Bool(flag) => {
            let mut on = *flag;
            if ui.checkbox(&mut on, "").changed() {
                *event = RecordEvent::Set {
                    path: path.to_string(),
                    kind: "bool".into(),
                    value: if on { "yes" } else { "no" }.into(),
                };
            }
        }
        FieldValue::Damaged(raw) => {
            ui.label(raw);
        }
        other => {
            let kind = other.kind_name();
            let mut text = match other {
                FieldValue::Text(text) | FieldValue::Number(text) | FieldValue::Colour(text) => {
                    text.clone()
                }
                _ => String::new(),
            };
            if ui.add(egui::TextEdit::singleline(&mut text).desired_width(160.0)).changed() {
                *event =
                    RecordEvent::Set { path: path.to_string(), kind: kind.into(), value: text };
            }
        }
    }
}

fn add_row(
    ui: &mut egui::Ui,
    salt: &str,
    parent: &str,
    labels: &RecordLabels,
    event: &mut RecordEvent,
) {
    let key_id = ui.id().with((salt, parent, "new-key"));
    let kind_id = ui.id().with((salt, parent, "new-kind"));
    let mut key = ui.data(|data| data.get_temp::<String>(key_id).unwrap_or_default());
    let mut kind =
        ui.data(|data| data.get_temp::<String>(kind_id).unwrap_or_else(|| "text".into()));
    ui.horizontal(|ui| {
        ui.label(&labels.key);
        ui.add(egui::TextEdit::singleline(&mut key).desired_width(80.0).hint_text(&labels.key));
        egui::ComboBox::from_id_salt((salt, parent, "kind"))
            .selected_text(kind.clone())
            .show_crisp(ui, |ui| {
                for name in KINDS {
                    ui.selectable_value(&mut kind, (*name).to_string(), *name);
                }
            });
        if ui.button(&labels.add).clicked() && !key.trim().is_empty() {
            *event = RecordEvent::Add {
                parent: parent.to_string(),
                key: key.trim().to_string(),
                kind: kind.clone(),
            };
            key.clear();
        }
    });
    ui.data_mut(|data| {
        data.insert_temp(key_id, key);
        data.insert_temp(kind_id, kind);
    });
}
