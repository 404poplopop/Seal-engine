//! Scene and asset-editor tabs under the menu bar.

use seal_editor_core::EditorSession;
use seal_ui::{paint_icon, Icon};

fn drop_rect_id() -> egui::Id {
    egui::Id::new("seal_doc_drop_screen")
}

/// Where the document tab bar is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocBarMode {
    /// Full editor window.
    Main,
    /// Scene pulled out into its own viewport.
    Detached,
}

/// Home, then the open scene. Asset editors join this row when one is open.
pub fn show(ui: &mut egui::Ui, session: &mut EditorSession, mode: DocBarMode) {
    // On the main window the tabs sit in the menu bar, right after the menus.
    let slot = ui.ctx().data_mut(|d| {
        let slot = d.get_temp::<egui::Rect>(slot_id());
        d.remove::<egui::Rect>(slot_id());
        slot
    });
    if let (DocBarMode::Main, Some(slot)) = (mode, slot) {
        let area = slot.shrink2(egui::vec2(10.0, 0.0));
        if area.width() > 60.0 {
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(area)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            child.spacing_mut().item_spacing.x = 4.0;
            let height = (area.height() - 2.0).min(seal_ui::TAB_PX);
            main_bar(&mut child, session, height);
            store_drop_rect_at(&child, area);
            return;
        }
    }
    let visuals = ui.visuals().clone();
    let bar = egui::Frame::new()
        .fill(visuals.extreme_bg_color)
        .inner_margin(egui::Margin::symmetric(6, 2));
    egui::Panel::top("documents").frame(bar).show_separator_line(false).show(ui, |ui| {
        ui.set_min_height(seal_ui::TAB_PX);
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.horizontal(|ui| match mode {
            DocBarMode::Main => main_bar(ui, session, seal_ui::TAB_PX),
            DocBarMode::Detached => {
                home_button(ui, session);
                if session.scene_tab_open {
                    scene_tab(ui, session, mode, seal_ui::TAB_PX);
                }
            }
        });
        if mode == DocBarMode::Main {
            store_drop_rect(ui);
        }
    });
}

fn slot_id() -> egui::Id {
    egui::Id::new("seal-doc-slot")
}

fn main_bar(ui: &mut egui::Ui, session: &mut EditorSession, height: f32) {
    home_button(ui, session);
    if session.scene_tab_detached {
        dock_slot(ui, session, height);
    } else if session.scene_tab_open {
        scene_tab(ui, session, DocBarMode::Main, height);
    }
}

fn store_drop_rect(ui: &egui::Ui) {
    store_drop_rect_at(ui, ui.min_rect());
}

fn store_drop_rect_at(ui: &egui::Ui, local: egui::Rect) {
    let screen = ui.ctx().input(|input| {
        let outer = input.viewport().outer_rect?;
        Some(egui::Rect::from_min_max(
            outer.min + local.min.to_vec2(),
            outer.min + local.max.to_vec2(),
        ))
    });
    if let Some(screen) = screen {
        ui.ctx().data_mut(|d| d.insert_temp(drop_rect_id(), screen));
    }
}

fn dock_slot(ui: &mut egui::Ui, session: &mut EditorSession, height: f32) {
    let tip = session.messages.get("doc-dock-scene");
    let (rect, response) = ui.allocate_exact_size(egui::vec2(220.0, height), egui::Sense::hover());
    let stroke = if response.hovered() {
        ui.visuals().selection.stroke
    } else {
        egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color)
    };
    ui.painter().rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Inside);
    let font = egui::TextStyle::Small.resolve(ui.style());
    let ink = ui.visuals().weak_text_color();
    let galley = ui.painter().layout_no_wrap(tip, font, ink);
    ui.painter().galley(
        egui::pos2(rect.left() + 8.0, rect.center().y - galley.size().y * 0.5),
        galley,
        ink,
    );
    try_dock_from_pointer(ui, session);
    if response.clicked() {
        session.dock_scene_tab();
    }
}

fn try_dock_from_pointer(ui: &egui::Ui, session: &mut EditorSession) {
    if !session.scene_tab_detached {
        return;
    }
    let released = ui.ctx().input(|input| input.pointer.any_released());
    if !released {
        return;
    }
    let Some(screen) = ui.ctx().input(|input| {
        let pointer = input.pointer.latest_pos()?;
        let outer = input.viewport().outer_rect?;
        Some(outer.min + pointer.to_vec2())
    }) else {
        return;
    };
    let docked = ui.ctx().data(|d| {
        d.get_temp::<egui::Rect>(drop_rect_id()).is_some_and(|rect| rect.contains(screen))
    });
    if docked {
        session.dock_scene_tab();
    }
}

fn home_button(ui: &mut egui::Ui, session: &mut EditorSession) {
    let tip = session.messages.get("doc-home");
    let (rect, response) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::click());
    let response = response.on_hover_text(tip);
    seal_ui::crisp::plate(ui, rect, &response, false, false);
    paint_icon(ui.painter(), rect.center(), Icon::ViewHome, 14.0, ui.visuals().text_color());
    if response.clicked() {
        session.dock_scene_tab();
    }
}

fn scene_tab(ui: &mut egui::Ui, session: &mut EditorSession, mode: DocBarMode, height: f32) {
    let name = session.scene_file_name();
    let dirty = session.scene_dirty;
    let label = if dirty { format!("{name}*") } else { name };
    let ink = ui.visuals().text_color();
    let font = egui::TextStyle::Body.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(label, font, ink);
    let close_w = 18.0;
    let width = 12.0 + 16.0 + 6.0 + galley.size().x + 6.0 + close_w + 4.0;
    let (rect, tab) =
        ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click_and_drag());
    if tab.middle_clicked() {
        session.begin_close_scene_tab();
    }
    if tab.drag_stopped() {
        try_dock_from_pointer(ui, session);
    } else if tab.dragged() && mode == DocBarMode::Main && !session.scene_tab_detached {
        let d = tab.drag_delta();
        if d.x * d.x + d.y * d.y > 36.0 * 36.0 {
            session.scene_tab_detached = true;
        }
    }
    let tab_state = if tab.is_pointer_button_down_on() {
        seal_ui::crisp::Raised::Pressed
    } else if tab.hovered() {
        seal_ui::crisp::Raised::Hover
    } else {
        seal_ui::crisp::Raised::Normal
    };
    seal_ui::crisp::raised(&mut ui.painter(), rect, ui.visuals(), tab_state);
    let rgb = seal_ui::asset_tint("scene");
    let stripe = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
    let stripe_rect =
        egui::Rect::from_min_max(rect.left_top(), egui::pos2(rect.left() + 6.0, rect.bottom()));
    ui.painter().rect_filled(stripe_rect, 0.0, stripe);
    if dirty {
        ui.painter().circle_filled(egui::pos2(stripe_rect.center().x, rect.center().y), 2.0, ink);
    }
    paint_icon(
        ui.painter(),
        egui::pos2(rect.left() + 8.0 + 8.0, rect.center().y),
        Icon::AssetScene,
        16.0,
        ink,
    );
    let text_x = rect.left() + 8.0 + 16.0 + 6.0;
    ui.painter().galley(egui::pos2(text_x, rect.center().y - galley.size().y * 0.5), galley, ink);
    let close = egui::Rect::from_center_size(
        egui::pos2(rect.right() - close_w * 0.5 - 2.0, rect.center().y),
        egui::vec2(close_w, height - 4.0),
    );
    let close_id = ui.id().with("scene-close");
    let close_response = ui.interact(close, close_id, egui::Sense::click());
    if dirty || tab.hovered() || close_response.hovered() {
        paint_icon(ui.painter(), close.center(), Icon::ViewClose, 12.0, ink);
    }
    close_response.clone().on_hover_text(session.messages.get("doc-close"));
    if close_response.clicked() {
        session.begin_close_scene_tab();
    }
    if mode == DocBarMode::Detached {
        let drag_tip = session.messages.get("doc-drag-back");
        tab.on_hover_text(drag_tip);
    }
}
