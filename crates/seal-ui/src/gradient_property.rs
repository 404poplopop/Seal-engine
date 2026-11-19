//! Gradient field: a colour strip and a popup stop editor.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};

use seal_editor_core::curve::{
    add_alpha_stop, add_color_stop, delete_alpha_stop, delete_color_stop, move_alpha_stop,
    move_color_stop, sample_gradient, set_alpha, set_color_rgb, Gradient,
};

use crate::widgets::PanelFrame;

/// Labels for the gradient popup.
pub struct GradientUiText<'a> {
    pub delete_stop: &'a str,
    pub colour: &'a str,
    pub alpha: &'a str,
    pub hint: &'a str,
    pub reset: &'a str,
}

/// What the user did to a gradient field this frame.
#[derive(Clone, Debug)]
pub struct GradientFieldEdit {
    pub changed: bool,
    pub gradient: Gradient,
    pub drag_started: bool,
    pub drag_stopped: bool,
    pub escape_cancel: bool,
    pub commit_focus: bool,
    pub reset: bool,
}

impl GradientFieldEdit {
    fn quiet(gradient: &Gradient) -> Self {
        Self {
            changed: false,
            gradient: gradient.clone(),
            drag_started: false,
            drag_stopped: false,
            escape_cancel: false,
            commit_focus: false,
            reset: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StopKind {
    Color,
    Alpha,
}

fn paint_strip(ui: &egui::Ui, rect: Rect, gradient: &Gradient) {
    let painter = ui.painter_at(rect);
    let squares = 8.0_f32;
    let mut x = rect.left();
    let mut toggle = false;
    while x < rect.right() {
        let w = squares.min(rect.right() - x);
        let color = if toggle { Color32::from_gray(70) } else { Color32::from_gray(110) };
        painter.rect_filled(
            Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(w, rect.height())),
            0.0,
            color,
        );
        x += w;
        toggle = !toggle;
    }
    let columns = rect.width().round().max(1.0) as i32;
    for column in 0..columns {
        let t = column as f32 / columns as f32;
        let sample = sample_gradient(gradient, t);
        let color = Color32::from_rgba_unmultiplied(
            (sample[0] * 255.0) as u8,
            (sample[1] * 255.0) as u8,
            (sample[2] * 255.0) as u8,
            (sample[3] * 255.0) as u8,
        );
        let left = rect.left() + column as f32;
        painter.rect_filled(
            Rect::from_min_size(Pos2::new(left, rect.top()), Vec2::new(1.0, rect.height())),
            0.0,
            color,
        );
    }
    painter.rect_stroke(
        rect,
        2.0,
        Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        egui::StrokeKind::Inside,
    );
}

fn time_at(rect: Rect, x: f32) -> f32 {
    ((x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0)
}

fn x_at(rect: Rect, time: f32) -> f32 {
    rect.left() + time.clamp(0.0, 1.0) * rect.width()
}

fn nearest_stop(gradient: &Gradient, rect: Rect, pos: Pos2) -> Option<(StopKind, usize)> {
    let mut best: Option<(StopKind, usize, f32)> = None;
    let consider = |best: &mut Option<(StopKind, usize, f32)>,
                    kind: StopKind,
                    index: usize,
                    time: f32,
                    y: f32| {
        let mark = Pos2::new(x_at(rect, time), y);
        let d = mark.distance(pos);
        if d <= 12.0 && best.is_none_or(|(_, _, old)| d < old) {
            *best = Some((kind, index, d));
        }
    };
    for (index, stop) in gradient.colors.iter().enumerate() {
        consider(&mut best, StopKind::Color, index, stop.time, rect.top() + 4.0);
    }
    for (index, stop) in gradient.alphas.iter().enumerate() {
        consider(&mut best, StopKind::Alpha, index, stop.time, rect.bottom() - 4.0);
    }
    best.map(|(kind, index, _)| (kind, index))
}

/// Preview strip. Clicking it opens the editor.
pub fn gradient_row(
    ui: &mut egui::Ui,
    salt: &str,
    label: &str,
    gradient: &Gradient,
    explicit: bool,
    text: &GradientUiText<'_>,
) -> GradientFieldEdit {
    let id = ui.id().with("seal-gradient").with(salt);
    let mut open = ui.data(|data| data.get_temp::<bool>(id).unwrap_or(false));
    let mut edit = GradientFieldEdit::quiet(gradient);
    ui.horizontal(|ui| {
        // Same label column as the other property rows; the strip fills the value column.
        crate::widgets::property_name(ui, label, false, None);
        let width = (ui.available_width() - if explicit { 26.0 } else { 4.0 }).max(48.0);
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(width, ui.spacing().interact_size.y.max(22.0)),
            Sense::click(),
        );
        let response = response.on_hover_text(label);
        paint_strip(ui, rect, gradient);
        if response.clicked() {
            open = !open;
        }
        if explicit && crate::widgets::paint_row_reset_icon(ui, text.reset) {
            edit.reset = true;
            edit.changed = true;
            edit.commit_focus = true;
        }
        crate::widgets::paint_row_rule(ui);
    });
    if edit.reset {
        open = false;
    }
    if open {
        let mut edited = gradient.clone();
        let mut still = true;
        let ctx = ui.ctx().clone();
        egui::Window::new(label)
            .id(id.with("popup"))
            .collapsible(false)
            .resizable(true)
            .default_width(360.0)
            .open(&mut still)
            .show(&ctx, |ui| {
                gradient_popup(ui, id, &mut edited, &mut edit, text);
            });
        if !still {
            open = false;
        }
        if edit.changed || edit.drag_stopped || edit.commit_focus {
            edit.gradient = edited;
        }
    }
    if edit.escape_cancel {
        open = false;
    }
    ui.data_mut(|data| data.insert_temp(id, open));
    edit
}

fn gradient_popup(
    ui: &mut egui::Ui,
    id: egui::Id,
    gradient: &mut Gradient,
    edit: &mut GradientFieldEdit,
    text: &GradientUiText<'_>,
) {
    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        edit.escape_cancel = true;
    }
    ui.label(text.hint);
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(300.0), 48.0),
        Sense::click_and_drag(),
    );
    paint_strip(ui, rect, gradient);
    {
        let painter = ui.painter_at(rect);
        for stop in &gradient.colors {
            let pos = Pos2::new(x_at(rect, stop.time), rect.top() + 4.0);
            painter.circle_filled(
                pos,
                5.0,
                Color32::from_rgb(
                    (stop.rgb[0].clamp(0.0, 1.0) * 255.0) as u8,
                    (stop.rgb[1].clamp(0.0, 1.0) * 255.0) as u8,
                    (stop.rgb[2].clamp(0.0, 1.0) * 255.0) as u8,
                ),
            );
        }
        for stop in &gradient.alphas {
            let pos = Pos2::new(x_at(rect, stop.time), rect.bottom() - 4.0);
            let gray = (stop.alpha.clamp(0.0, 1.0) * 255.0) as u8;
            painter.circle_filled(pos, 5.0, Color32::from_rgb(gray, gray, gray));
        }
    }
    let drag_id = id.with("drag-stop");
    let select_id = id.with("selected");
    let mut dragging = ui.data(|data| data.get_temp::<(u8, usize)>(drag_id));
    let mut selected = ui.data(|data| data.get_temp::<(u8, usize)>(select_id));
    if response.double_clicked() {
        let pointer = response.interact_pointer_pos().unwrap_or(rect.center());
        let t = time_at(rect, pointer.x);
        let sample = sample_gradient(gradient, t);
        if pointer.y < rect.center().y {
            let index = add_color_stop(gradient, t, [sample[0], sample[1], sample[2]]);
            selected = Some((0, index));
        } else {
            let index = add_alpha_stop(gradient, t, sample[3]);
            selected = Some((1, index));
        }
        edit.changed = true;
        edit.commit_focus = true;
    } else if response.drag_started() {
        if let Some(pointer) = response.interact_pointer_pos() {
            if let Some((kind, index)) = nearest_stop(gradient, rect, pointer) {
                dragging = Some((if kind == StopKind::Color { 0 } else { 1 }, index));
                selected = dragging;
                edit.drag_started = true;
                edit.changed = true;
            }
        }
    }
    if let Some((kind, index)) = dragging {
        if response.dragged() {
            if let Some(pointer) = response.interact_pointer_pos() {
                let t = time_at(rect, pointer.x);
                if kind == 0 {
                    move_color_stop(gradient, index, t);
                } else {
                    move_alpha_stop(gradient, index, t);
                }
                edit.changed = true;
            }
        }
        if response.drag_stopped() {
            edit.drag_stopped = true;
            dragging = None;
        }
    } else if response.clicked() {
        if let Some(pointer) = response.interact_pointer_pos() {
            selected = nearest_stop(gradient, rect, pointer)
                .map(|(kind, index)| (if kind == StopKind::Color { 0 } else { 1 }, index));
        }
    }
    if let Some((kind, index)) = selected {
        if ui.button(text.delete_stop).clicked() {
            let removed = if kind == 0 {
                delete_color_stop(gradient, index)
            } else {
                delete_alpha_stop(gradient, index)
            };
            if removed {
                selected = None;
                edit.changed = true;
                edit.commit_focus = true;
            }
        }
    }
    if let Some((0, index)) = selected {
        if let Some(stop) = gradient.colors.get(index).copied() {
            let mut rgb = stop.rgb;
            let pointer_down = ui.input(|input| input.pointer.primary_down());
            let pointer_released = ui.input(|input| input.pointer.primary_released());
            let colour_drag = id.with("colour-drag");
            let was_drag = ui.data(|data| data.get_temp::<bool>(colour_drag).unwrap_or(false));
            let changed = {
                let mut frame = PanelFrame::from_ui(ui);
                frame.color_rgb(text.colour, &mut rgb, false, "").is_some()
            };
            if changed {
                set_color_rgb(gradient, index, rgb);
                edit.changed = true;
                if pointer_down {
                    if !was_drag {
                        edit.drag_started = true;
                    }
                    ui.data_mut(|data| data.insert_temp(colour_drag, true));
                } else {
                    edit.commit_focus = true;
                }
            }
            if was_drag && pointer_released {
                edit.drag_stopped = true;
                ui.data_mut(|data| data.insert_temp(colour_drag, false));
            }
        }
    }
    if let Some((1, index)) = selected {
        if let Some(stop) = gradient.alphas.get(index).copied() {
            let mut alpha = stop.alpha;
            let mut pick = None;
            let mut frame = PanelFrame::from_ui(ui);
            let (changed, _, drag) =
                frame.number_row(text.alpha, &mut alpha, 0.01, false, None, None, None, &mut pick);
            if changed || drag.drag_started || drag.drag_stopped || drag.commit_focus {
                set_alpha(gradient, index, alpha.clamp(0.0, 1.0));
                edit.changed |= changed || drag.drag_started;
                edit.drag_started |= drag.drag_started;
                edit.drag_stopped |= drag.drag_stopped;
                edit.escape_cancel |= drag.escape_cancel;
                edit.commit_focus |= drag.commit_focus;
            }
        }
    }
    ui.data_mut(|data| {
        if let Some(selected) = selected {
            data.insert_temp(select_id, selected);
        } else {
            data.remove::<(u8, usize)>(select_id);
        }
        if let Some(dragging) = dragging {
            data.insert_temp(drag_id, dragging);
        } else {
            data.remove::<(u8, usize)>(drag_id);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_strip_paints_without_a_window() {
        let gradient = Gradient::from_endpoints([1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 0.0]);
        let text = GradientUiText {
            delete_stop: "Delete stop",
            colour: "Colour",
            alpha: "Alpha",
            hint: "Double-click the strip to add a stop.",
            reset: "Use straight fade",
        };
        let _ = crate::run_frame(crate::default_input(vec![]), |ui| {
            let edit = gradient_row(ui, "colour", "Colour over life", &gradient, false, &text);
            assert!(!edit.changed);
        });
    }
}
