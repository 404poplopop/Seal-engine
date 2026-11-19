//! Inspector list property rows: foldable multi-value fields with reorder.

use egui::{Align2, Sense, TextStyle};

use crate::context_menu::ContextMenuRow;
use crate::icons::Icon;
use crate::widgets::{
    attach_property_row_context, paint_focus_ring, paint_row_reset_icon, paint_row_rule, PanelFrame,
};
use crate::{ContextMenuEntry, ContextMenuPick};

/// Labels for list row chrome; filled from locale in the panels crate.
pub struct ListPropertyLabels {
    pub mixed: String,
    pub add: String,
    pub clear: String,
    pub damaged: String,
    pub remove_tip: String,
    pub duplicate_tip: String,
    pub drag_tip: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListPropertyAction {
    Add,
    Remove(usize),
    Duplicate(usize),
    Reorder { from: usize, to: usize },
    Clear,
    Set { index: usize, value: String },
    ToggleOpen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ListDrag {
    index: usize,
}

const CLEAR_MENU_CMD: &str = "inspector.list_menu_clear";

/// One foldable list field. Returns the first action the user took this frame.
#[allow(clippy::too_many_arguments)]
pub fn paint_list_property(
    frame: &mut PanelFrame,
    field_key: &str,
    label: &str,
    value: Option<&str>,
    mixed: bool,
    open: bool,
    spec: seal_editor_core::ListFieldSpec,
    labels: &ListPropertyLabels,
    label_bright: bool,
    reset_tip: Option<&str>,
    row_menu: Option<&[ContextMenuEntry]>,
    menu_pick: &mut Option<ContextMenuPick>,
) -> Option<ListPropertyAction> {
    let rows = value.map(|text| seal_editor_core::parse_list_field(spec, text)).unwrap_or_default();
    let count_label = if mixed { labels.mixed.to_string() } else { rows.len().to_string() };
    let header = format!("{label} ({count_label})");
    let mut action = None;
    let mut menu: Vec<ContextMenuEntry> = row_menu.map(|m| m.to_vec()).unwrap_or_default();
    menu.push(ContextMenuEntry::Row(ContextMenuRow {
        label: labels.clear.to_string(),
        icon: Icon::EditDelete,
        shortcut: String::new(),
        enabled: !mixed,
        reason: String::new(),
        command: Some((CLEAR_MENU_CMD.into(), vec![("field".into(), field_key.into())])),
        children: Vec::new(),
    }));
    let ui = frame.ui_mut();
    let section_id = format!("list-{field_key}");
    ui.push_id(&section_id, |ui| {
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 22.0), Sense::click());
        let visuals = ui.visuals().clone();
        let state = if response.is_pointer_button_down_on() {
            crate::crisp::Raised::Pressed
        } else if response.hovered() {
            crate::crisp::Raised::Hover
        } else {
            crate::crisp::Raised::Normal
        };
        let painter = ui.painter().clone();
        crate::crisp::raised(&mut &painter, rect, &visuals, state);
        crate::crisp::arrow(
            &mut &painter,
            egui::pos2(rect.left() + 11.0, rect.center().y),
            open,
            visuals.strong_text_color(),
        );
        painter.text(
            egui::pos2(rect.left() + 22.0, rect.center().y),
            Align2::LEFT_CENTER,
            header.clone(),
            egui::FontId::proportional(crate::SECTION_PX),
            if label_bright { visuals.strong_text_color() } else { visuals.text_color() },
        );
        paint_focus_ring(ui, &response);
        if response.clicked() {
            action = Some(ListPropertyAction::ToggleOpen);
        }
        attach_property_row_context(ui, label, Some(&menu), menu_pick, Some(field_key));
        if menu_pick.as_ref().is_some_and(|p| p.command == CLEAR_MENU_CMD) {
            action = Some(ListPropertyAction::Clear);
            *menu_pick = None;
        }
        paint_row_rule(ui);
        if !open {
            return;
        }
        if mixed {
            ui.label(egui::RichText::new(&labels.mixed).weak());
            paint_row_rule(ui);
            return;
        }
        for (index, row) in rows.iter().enumerate() {
            ui.horizontal(|ui| {
                let (drag_rect, drag_resp) = ui.allocate_exact_size(
                    egui::vec2(18.0, ui.spacing().interact_size.y),
                    Sense::drag(),
                );
                crate::icons::paint_icon(
                    ui.painter(),
                    drag_rect.center(),
                    Icon::ViewDrag,
                    crate::ICON_PX,
                    ui.visuals().weak_text_color(),
                );
                let drag_resp = drag_resp.on_hover_text(&labels.drag_tip);
                if drag_resp.drag_started() {
                    drag_resp.dnd_set_drag_payload(ListDrag { index });
                }
                if drag_resp.dnd_hover_payload::<ListDrag>().is_some() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                }
                if let Some(payload) = drag_resp.dnd_release_payload::<ListDrag>() {
                    if payload.index != index {
                        action =
                            Some(ListPropertyAction::Reorder { from: payload.index, to: index });
                    }
                }
                let mut text = row.text.clone();
                let edit = egui::TextEdit::singleline(&mut text)
                    .id(ui.id().with(("elem", index)))
                    // Leave room for Duplicate and Remove so the row stays inside the panel.
                    .desired_width(
                        (ui.available_width() - 2.0 * (22.0 + ui.spacing().item_spacing.x))
                            .max(40.0),
                    );
                let resp = crate::crisp::value_well(ui, edit, 6).on_hover_text(label);
                if resp.lost_focus() && text != row.text {
                    action = Some(ListPropertyAction::Set { index, value: text });
                }
                paint_focus_ring(ui, &resp);
                if icon_button(ui, Icon::EditDuplicate, &labels.duplicate_tip) {
                    action = Some(ListPropertyAction::Duplicate(index));
                }
                if icon_button(ui, Icon::EditDelete, &labels.remove_tip) {
                    action = Some(ListPropertyAction::Remove(index));
                }
            });
            if row.damaged {
                ui.colored_label(ui.visuals().error_fg_color, &labels.damaged);
            }
            paint_row_rule(ui);
        }
        ui.horizontal(|ui| {
            if ui.button(&labels.add).clicked() {
                action = Some(ListPropertyAction::Add);
            }
            if let Some(tip) = reset_tip {
                if paint_row_reset_icon(ui, tip) {
                    let _ = tip;
                }
            }
        });
        paint_row_rule(ui);
    });
    action
}

fn icon_button(ui: &mut egui::Ui, icon: Icon, tip: &str) -> bool {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(22.0, ui.spacing().interact_size.y), Sense::click());
    crate::icons::paint_icon(
        ui.painter(),
        rect.center(),
        icon,
        TextStyle::Body.resolve(ui.style()).size,
        ui.visuals().text_color(),
    );
    resp.on_hover_text(tip).clicked()
}

impl PanelFrame<'_> {
    /// Foldable list with per-element editors, like polygon points.
    #[allow(clippy::too_many_arguments)]
    pub fn list_property_row(
        &mut self,
        field_key: &str,
        label: &str,
        value: Option<&str>,
        mixed: bool,
        open: bool,
        spec: seal_editor_core::ListFieldSpec,
        labels: &ListPropertyLabels,
        label_bright: bool,
        reset_tip: Option<&str>,
        row_menu: Option<&[ContextMenuEntry]>,
        menu_pick: &mut Option<ContextMenuPick>,
    ) -> Option<ListPropertyAction> {
        paint_list_property(
            self,
            field_key,
            label,
            value,
            mixed,
            open,
            spec,
            labels,
            label_bright,
            reset_tip,
            row_menu,
            menu_pick,
        )
    }
}
