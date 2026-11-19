//! Curve field: a preview graph and a popup editor.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};

use seal_editor_core::curve::{
    add_key, apply_preset, delete_key, evaluate, move_key, set_key, Curve, CurveEase, CurvePreset,
};

use crate::crisp::CrispCombo as _;
use crate::widgets::PanelFrame;
use crate::NumberDragSignals;

/// Labels for the curve popup. The panel fills these from the message catalog.
pub struct CurveUiText<'a> {
    pub delete_key: &'a str,
    pub time: &'a str,
    pub value: &'a str,
    pub in_tangent: &'a str,
    pub out_tangent: &'a str,
    pub easing: &'a str,
    pub constant: &'a str,
    pub linear: &'a str,
    pub ease_in: &'a str,
    pub ease_out: &'a str,
    pub ease_in_out: &'a str,
    pub tangent: &'a str,
    pub hint: &'a str,
    pub reset: &'a str,
}

/// What the user did to a curve field this frame.
#[derive(Clone, Debug)]
pub struct CurveFieldEdit {
    pub changed: bool,
    pub curve: Curve,
    pub drag_started: bool,
    pub drag_stopped: bool,
    pub escape_cancel: bool,
    pub commit_focus: bool,
    pub reset: bool,
}

impl CurveFieldEdit {
    fn quiet(curve: &Curve) -> Self {
        Self {
            changed: false,
            curve: curve.clone(),
            drag_started: false,
            drag_stopped: false,
            escape_cancel: false,
            commit_focus: false,
            reset: false,
        }
    }
}

/// Time and value covered by the graph. `time_limit` is included even when no key sits on it.
pub fn curve_span(curve: &Curve, time_limit: Option<(f32, f32)>) -> (f32, f32, f32, f32) {
    let (mut t0, mut t1) = time_limit.unwrap_or((0.0, 1.0));
    let (mut v0, mut v1) = (0.0_f32, 1.0_f32);
    for key in &curve.keys {
        t0 = t0.min(key.time);
        t1 = t1.max(key.time);
        v0 = v0.min(key.value);
        v1 = v1.max(key.value);
    }
    if t1 - t0 < 1.0e-4 {
        t1 = t0 + 1.0;
    }
    if v1 - v0 < 1.0e-4 {
        v0 -= 0.5;
        v1 += 0.5;
    }
    let pad = (v1 - v0) * 0.08;
    (t0, t1, v0 - pad, v1 + pad)
}

/// Map a time and value into `rect`. Y grows upward on the graph.
pub fn curve_to_plot(rect: Rect, t: f32, value: f32, span: (f32, f32, f32, f32)) -> Pos2 {
    let (t0, t1, v0, v1) = span;
    let x = rect.left() + (t - t0) / (t1 - t0) * rect.width();
    let y = rect.bottom() - (value - v0) / (v1 - v0) * rect.height();
    Pos2::new(x, y)
}

/// Map a pointer position back to time and value.
pub fn plot_to_curve(rect: Rect, pos: Pos2, span: (f32, f32, f32, f32)) -> (f32, f32) {
    let (t0, t1, v0, v1) = span;
    let t = t0 + (pos.x - rect.left()) / rect.width().max(1.0) * (t1 - t0);
    let value = v0 + (rect.bottom() - pos.y) / rect.height().max(1.0) * (v1 - v0);
    (t, value)
}

fn paint_curve(
    ui: &egui::Ui,
    rect: Rect,
    curve: &Curve,
    span: (f32, f32, f32, f32),
    selected: Option<usize>,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
    painter.rect_stroke(
        rect,
        2.0,
        Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        egui::StrokeKind::Inside,
    );
    let ink = ui.visuals().selection.bg_fill;
    let mut points = Vec::with_capacity(48);
    for step in 0..48 {
        let t = span.0 + (span.1 - span.0) * (step as f32 / 47.0);
        points.push(curve_to_plot(rect, t, evaluate(curve, t), span));
    }
    painter.add(egui::Shape::line(points, Stroke::new(1.5, ink)));
    for (index, key) in curve.keys.iter().enumerate() {
        let pos = curve_to_plot(rect, key.time, key.value, span);
        let radius = if selected == Some(index) { 5.0 } else { 3.5 };
        painter.circle_filled(pos, radius, Color32::WHITE);
        painter.circle_stroke(pos, radius, Stroke::new(1.0, ink));
    }
}

fn nearest_key(curve: &Curve, rect: Rect, pos: Pos2, span: (f32, f32, f32, f32)) -> Option<usize> {
    let mut best = None;
    let mut best_d = 10.0_f32;
    for (index, key) in curve.keys.iter().enumerate() {
        let mark = curve_to_plot(rect, key.time, key.value, span);
        let d = mark.distance(pos);
        if d <= best_d {
            best_d = d;
            best = Some(index);
        }
    }
    best
}

fn note_drag(edit: &mut CurveFieldEdit, drag: &NumberDragSignals, changed: bool) {
    edit.changed |= changed;
    edit.drag_started |= drag.drag_started;
    edit.drag_stopped |= drag.drag_stopped;
    edit.escape_cancel |= drag.escape_cancel;
    edit.commit_focus |= drag.commit_focus;
}

/// Preview row. Clicking the graph opens the editor.
///
/// `explicit` shows Reset, which drops a stored curve so the straight fade is used again.
/// `time_limit` clamps dragged and new keys (particle life is 0 to 1). `None` leaves time free.
pub fn curve_row(
    ui: &mut egui::Ui,
    salt: &str,
    label: &str,
    curve: &Curve,
    explicit: bool,
    time_limit: Option<(f32, f32)>,
    text: &CurveUiText<'_>,
) -> CurveFieldEdit {
    let id = ui.id().with("seal-curve").with(salt);
    let mut open = ui.data(|data| data.get_temp::<bool>(id).unwrap_or(false));
    let mut edit = CurveFieldEdit::quiet(curve);
    ui.horizontal(|ui| {
        // Same label column as the other property rows; the strip fills the value column.
        crate::widgets::property_name(ui, label, false, None);
        let width = (ui.available_width() - if explicit { 26.0 } else { 4.0 }).max(48.0);
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(width, ui.spacing().interact_size.y.max(22.0)),
            Sense::click(),
        );
        let response = response.on_hover_text(label);
        paint_curve(ui, rect, curve, curve_span(curve, time_limit), None);
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
        let mut edited = curve.clone();
        let mut still = true;
        let ctx = ui.ctx().clone();
        egui::Window::new(label)
            .id(id.with("popup"))
            .collapsible(false)
            .resizable(true)
            .default_width(340.0)
            .open(&mut still)
            .show(&ctx, |ui| {
                curve_popup(ui, id, &mut edited, &mut edit, time_limit, text);
            });
        if !still {
            open = false;
        }
        if edit.changed {
            edit.curve = edited;
        }
    }
    if edit.escape_cancel {
        open = false;
    }
    ui.data_mut(|data| data.insert_temp(id, open));
    edit
}

fn curve_popup(
    ui: &mut egui::Ui,
    id: egui::Id,
    curve: &mut Curve,
    edit: &mut CurveFieldEdit,
    time_limit: Option<(f32, f32)>,
    text: &CurveUiText<'_>,
) {
    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        edit.escape_cancel = true;
    }
    ui.label(text.hint);
    let span = curve_span(curve, time_limit);
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(280.0), 140.0),
        Sense::click_and_drag(),
    );
    let mut selected = ui.data(|data| data.get_temp::<usize>(id.with("selected")));
    paint_curve(ui, rect, curve, span, selected);
    let drag_id = id.with("dragging");
    let mut dragging = ui.data(|data| data.get_temp::<usize>(drag_id));
    if response.double_clicked() {
        let pointer = response.interact_pointer_pos().unwrap_or(rect.center());
        let (mut t, value) = plot_to_curve(rect, pointer, span);
        if let Some((lo, hi)) = time_limit {
            t = t.clamp(lo, hi);
        }
        selected = Some(add_key(curve, t, value));
        edit.changed = true;
        edit.commit_focus = true;
    } else if response.drag_started() {
        let pointer = response.interact_pointer_pos().unwrap_or(rect.center());
        if let Some(index) = nearest_key(curve, rect, pointer, span) {
            dragging = Some(index);
            selected = Some(index);
            edit.drag_started = true;
            edit.changed = true;
        }
    }
    if let Some(index) = dragging {
        if response.dragged() {
            if let Some(pointer) = response.interact_pointer_pos() {
                let (mut t, value) = plot_to_curve(rect, pointer, span);
                if let Some((lo, hi)) = time_limit {
                    t = t.clamp(lo, hi);
                }
                move_key(curve, index, t, value);
                edit.changed = true;
            }
        }
        if response.drag_stopped() {
            edit.drag_stopped = true;
            dragging = None;
        }
    } else if response.clicked() {
        if let Some(pointer) = response.interact_pointer_pos() {
            selected = nearest_key(curve, rect, pointer, span);
        }
    }
    ui.horizontal(|ui| {
        for (label, preset) in [
            (text.constant, CurvePreset::Constant),
            (text.linear, CurvePreset::Linear),
            (text.ease_in, CurvePreset::EaseIn),
            (text.ease_out, CurvePreset::EaseOut),
            (text.ease_in_out, CurvePreset::EaseInOut),
        ] {
            if ui.button(label).clicked() {
                apply_preset(curve, preset);
                selected = if curve.keys.is_empty() { None } else { Some(0) };
                edit.changed = true;
                edit.commit_focus = true;
            }
        }
    });
    if let Some(index) = selected {
        if ui.button(text.delete_key).clicked() && delete_key(curve, index) {
            selected =
                if curve.keys.is_empty() { None } else { Some(index.min(curve.keys.len() - 1)) };
            edit.changed = true;
            edit.commit_focus = true;
        }
    }
    if let Some(index) = selected {
        if let Some(key) = curve.keys.get(index).copied() {
            let mut time = key.time;
            let mut value = key.value;
            let mut in_tangent = key.in_tangent;
            let mut out_tangent = key.out_tangent;
            let mut ease = key.ease;
            let mut pick = None;
            {
                let mut frame = PanelFrame::from_ui(ui);
                let (changed, _, drag) = frame
                    .number_row(text.time, &mut time, 0.01, false, None, None, None, &mut pick);
                note_drag(edit, &drag, changed);
                let (changed, _, drag) = frame
                    .number_row(text.value, &mut value, 0.01, false, None, None, None, &mut pick);
                note_drag(edit, &drag, changed);
                let (changed, _, drag) = frame.number_row(
                    text.in_tangent,
                    &mut in_tangent,
                    0.01,
                    false,
                    None,
                    None,
                    None,
                    &mut pick,
                );
                note_drag(edit, &drag, changed);
                let (changed, _, drag) = frame.number_row(
                    text.out_tangent,
                    &mut out_tangent,
                    0.01,
                    false,
                    None,
                    None,
                    None,
                    &mut pick,
                );
                note_drag(edit, &drag, changed);
            }
            let names = [
                (text.tangent, CurveEase::Tangent),
                (text.constant, CurveEase::Constant),
                (text.linear, CurveEase::Linear),
                (text.ease_in, CurveEase::EaseIn),
                (text.ease_out, CurveEase::EaseOut),
                (text.ease_in_out, CurveEase::EaseInOut),
            ];
            ui.label(text.easing);
            let shown = names
                .iter()
                .find(|(_, ease_id)| *ease_id == ease)
                .map(|(name, _)| *name)
                .unwrap_or(text.linear);
            egui::ComboBox::from_id_salt(id.with("ease")).selected_text(shown).show_crisp(
                ui,
                |ui| {
                    for (name, ease_id) in names {
                        if ui.selectable_label(ease == ease_id, name).clicked() {
                            ease = ease_id;
                            edit.changed = true;
                            edit.commit_focus = true;
                        }
                    }
                },
            );
            if edit.changed
                || time != key.time
                || value != key.value
                || in_tangent != key.in_tangent
                || out_tangent != key.out_tangent
                || ease != key.ease
            {
                if let Some((lo, hi)) = time_limit {
                    time = time.clamp(lo, hi);
                }
                set_key(curve, index, time, value, in_tangent, out_tangent, ease);
            }
        }
    }
    ui.data_mut(|data| {
        if let Some(index) = selected {
            data.insert_temp(id.with("selected"), index);
        } else {
            data.remove::<usize>(id.with("selected"));
        }
        if let Some(index) = dragging {
            data.insert_temp(drag_id, index);
        } else {
            data.remove::<usize>(drag_id);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plot_mapping_round_trips_the_corners() {
        let rect = Rect::from_min_max(Pos2::new(10.0, 20.0), Pos2::new(110.0, 120.0));
        let span = (0.0, 1.0, 0.0, 10.0);
        let pos = curve_to_plot(rect, 0.0, 0.0, span);
        let (t, value) = plot_to_curve(rect, pos, span);
        assert!((t - 0.0).abs() < 1.0e-4 && (value - 0.0).abs() < 1.0e-3);
        let pos = curve_to_plot(rect, 1.0, 10.0, span);
        let (t, value) = plot_to_curve(rect, pos, span);
        assert!((t - 1.0).abs() < 1.0e-4 && (value - 10.0).abs() < 1.0e-3);
    }

    #[test]
    fn the_row_paints_without_a_window() {
        let curve = Curve::linear(1.0, 0.0);
        let text = CurveUiText {
            delete_key: "Delete key",
            time: "Time",
            value: "Value",
            in_tangent: "In tangent",
            out_tangent: "Out tangent",
            easing: "Easing",
            constant: "Constant",
            linear: "Linear",
            ease_in: "Ease in",
            ease_out: "Ease out",
            ease_in_out: "Ease in-out",
            tangent: "Tangent",
            hint: "Double-click to add a key.",
            reset: "Use straight fade",
        };
        let _ = crate::run_frame(crate::default_input(vec![]), |ui| {
            let edit =
                curve_row(ui, "size", "Size over life", &curve, false, Some((0.0, 1.0)), &text);
            assert!(!edit.changed);
        });
    }
}
