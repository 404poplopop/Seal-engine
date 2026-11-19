//! Asset field: thumbnail, name, search list, drag-and-drop, show, and clear.

use egui::{Color32, Rect, Sense, Vec2};

use crate::icons::paint_icon;
use crate::sprites::thumbnail;
use crate::widgets::asset_kind_color;
use crate::Icon;

#[derive(Clone, Debug)]
pub struct AssetChoice {
    pub path: String,
    pub name: String,
    pub kind: String,
    /// Absolute path used for the thumbnail, when the file is a picture.
    pub file: Option<String>,
}

pub struct PickerLabels {
    pub search: String,
    pub clear: String,
    pub show: String,
    pub empty: String,
    pub none: String,
    pub mixed: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetPickerEvent {
    Idle,
    Set(String),
    Clear,
    Show,
}

pub fn asset_picker(
    ui: &mut egui::Ui,
    salt: &str,
    label: &str,
    current: &str,
    mixed: bool,
    choices: &[AssetChoice],
    labels: &PickerLabels,
) -> AssetPickerEvent {
    let mut event = AssetPickerEvent::Idle;
    let open_id = ui.make_persistent_id((salt, "asset-open"));
    let query_id = ui.make_persistent_id((salt, "asset-query"));
    ui.horizontal(|ui| {
        crate::widgets::property_name(ui, label, false, None);
        let side = ui.spacing().interact_size.y;
        let (thumb, _) = ui.allocate_exact_size(Vec2::splat(side), Sense::hover());
        paint_thumb(ui, thumb, current, choices);
        let name = if mixed {
            labels.mixed.clone()
        } else if current.is_empty() {
            labels.none.clone()
        } else {
            current.rsplit('/').next().unwrap_or(current).to_string()
        };
        let open = ui.data(|data| data.get_temp::<bool>(open_id).unwrap_or(false));
        if ui.button(name).clicked() {
            ui.data_mut(|data| data.insert_temp(open_id, !open));
        }
        let mut icons = crate::PanelFrame::new(ui);
        if icons.icon_button(Icon::EditSearch, &labels.show) && !current.is_empty() {
            event = AssetPickerEvent::Show;
        }
        if icons.icon_button(Icon::ViewClose, &labels.clear) && !current.is_empty() {
            event = AssetPickerEvent::Clear;
        }
        let drop = ui.interact(ui.min_rect(), ui.id().with((salt, "asset-drop")), Sense::hover());
        if let Some(payload) = drop.dnd_release_payload::<String>() {
            if let Some(path) = seal_editor_core::asset_path_from_drag(&payload) {
                if choices.iter().any(|choice| choice.path == path) || !path.is_empty() {
                    event = AssetPickerEvent::Set(path.to_string());
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
                    && !choice.path.to_lowercase().contains(&needle)
                {
                    continue;
                }
                shown += 1;
                if ui.selectable_label(choice.path == current, &choice.name).clicked() {
                    event = AssetPickerEvent::Set(choice.path.clone());
                    ui.data_mut(|data| data.insert_temp(open_id, false));
                }
            }
        });
        if shown == 0 {
            ui.label(&labels.empty);
        }
    }
    event
}

fn paint_thumb(ui: &egui::Ui, rect: Rect, current: &str, choices: &[AssetChoice]) {
    let choice = choices.iter().find(|choice| choice.path == current);
    let kind = choice.map(|choice| choice.kind.as_str()).unwrap_or("other");
    if let Some(file) = choice.and_then(|choice| choice.file.as_deref()) {
        if let Some((texture, _)) = thumbnail(ui.ctx(), file) {
            ui.painter().image(
                texture,
                rect.shrink(2.0),
                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            return;
        }
    }
    ui.painter().rect_filled(rect.shrink(2.0), 2.0, asset_kind_color(kind));
    paint_icon(
        ui.painter(),
        rect.center(),
        Icon::AssetTexture,
        rect.height() * 0.7,
        Color32::WHITE,
    );
}
