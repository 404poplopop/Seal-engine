//! Object field: search list, type icon, eyedropper, and clear.

use egui::{Color32, Sense, Vec2};

use crate::icons::{outliner_entity_icon, outliner_entity_tint, paint_icon};

pub struct ObjectChoice {
    pub id: String,
    pub name: String,
    pub shape: String,
    pub role: String,
}

pub struct ObjectLabels {
    pub search: String,
    pub clear: String,
    pub eyedropper: String,
    pub armed: String,
    pub empty: String,
    pub none: String,
    pub mixed: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjectPickerEvent {
    Idle,
    Set(String),
    Clear,
    Eyedropper,
}

pub fn object_picker(
    ui: &mut egui::Ui,
    salt: &str,
    label: &str,
    current: &str,
    mixed: bool,
    armed: bool,
    choices: &[ObjectChoice],
    labels: &ObjectLabels,
) -> ObjectPickerEvent {
    let mut event = ObjectPickerEvent::Idle;
    let open_id = ui.make_persistent_id((salt, "object-open"));
    let query_id = ui.make_persistent_id((salt, "object-query"));
    let current_choice = choices.iter().find(|choice| choice.id == current);
    ui.horizontal(|ui| {
        crate::widgets::property_name(ui, label, false, None);
        if let Some(choice) = current_choice {
            let (rect, _) =
                ui.allocate_exact_size(Vec2::splat(ui.spacing().interact_size.y), Sense::hover());
            let icon = outliner_entity_icon(&choice.shape, &choice.role);
            let tint = outliner_entity_tint(&choice.shape, &choice.role);
            paint_icon(
                ui.painter(),
                rect.center(),
                icon,
                rect.height() * 0.8,
                Color32::from_rgb(tint[0], tint[1], tint[2]),
            );
        }
        let name = if mixed {
            labels.mixed.clone()
        } else if let Some(choice) = current_choice {
            choice.name.clone()
        } else if current.is_empty() {
            labels.none.clone()
        } else {
            current.to_string()
        };
        let open = ui.data(|data| data.get_temp::<bool>(open_id).unwrap_or(false));
        if ui.button(name).clicked() {
            ui.data_mut(|data| data.insert_temp(open_id, !open));
        }
        let eye = if armed { labels.armed.as_str() } else { labels.eyedropper.as_str() };
        if ui.button(eye).clicked() {
            event = ObjectPickerEvent::Eyedropper;
        }
        if ui.button(&labels.clear).clicked() && !current.is_empty() {
            event = ObjectPickerEvent::Clear;
        }
        let drop = ui.interact(ui.min_rect(), ui.id().with((salt, "object-drop")), Sense::hover());
        if let Some(payload) = drop.dnd_release_payload::<String>() {
            if !payload.starts_with("place:") && !payload.starts_with("inspcomp:") {
                if choices.iter().any(|choice| choice.id == payload.as_str()) {
                    event = ObjectPickerEvent::Set(payload.to_string());
                }
            }
        }
    });
    let open = ui.data(|data| data.get_temp::<bool>(open_id).unwrap_or(false));
    if open {
        let mut query = ui.data(|data| data.get_temp::<String>(query_id).unwrap_or_default());
        ui.add(
            egui::TextEdit::singleline(&mut query).hint_text(&labels.search).desired_width(220.0),
        );
        ui.data_mut(|data| data.insert_temp(query_id, query.clone()));
        let needle = query.to_lowercase();
        let mut shown = 0;
        egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
            for choice in choices {
                if !needle.is_empty()
                    && !choice.name.to_lowercase().contains(&needle)
                    && !choice.id.to_lowercase().contains(&needle)
                {
                    continue;
                }
                shown += 1;
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                    let icon = outliner_entity_icon(&choice.shape, &choice.role);
                    let tint = outliner_entity_tint(&choice.shape, &choice.role);
                    paint_icon(
                        ui.painter(),
                        rect.center(),
                        icon,
                        14.0,
                        Color32::from_rgb(tint[0], tint[1], tint[2]),
                    );
                    if ui.selectable_label(choice.id == current, &choice.name).clicked() {
                        event = ObjectPickerEvent::Set(choice.id.clone());
                        ui.data_mut(|data| data.insert_temp(open_id, false));
                    }
                });
            }
        });
        if shown == 0 {
            ui.label(&labels.empty);
        }
    }
    event
}
