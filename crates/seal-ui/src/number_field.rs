//! Number property field: drag scrub, typed values, simple expressions, optional range fill.

use egui::{Color32, Sense, Ui};

use crate::NumberDragSignals;

/// Optional range, unit suffix, and fill bar for a number field row.
#[derive(Clone, Debug, Default)]
pub struct NumberFieldSpec {
    pub min: Option<f32>,
    pub max: Option<f32>,
    pub unit_suffix: Option<String>,
    pub range_bar: bool,
    /// When set, seed the typed draft (look demos and expression samples).
    pub initial_text: Option<String>,
    /// Show a light-level LED on the left edge, driven by the value (lux).
    pub level_led: bool,
}

/// Parse and evaluate a small arithmetic expression (`2*3+1`, parentheses, unary minus).
pub fn eval_number_expression(text: &str) -> Option<f32> {
    let bytes = text.trim().as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let mut pos = 0usize;
    match parse_expr(bytes, &mut pos) {
        Ok(value) if pos == bytes.len() || is_space(bytes[pos]) => Some(value),
        _ => text.trim().parse::<f32>().ok(),
    }
}

fn is_space(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn skip_space(bytes: &[u8], pos: &mut usize) {
    while *pos < bytes.len() && is_space(bytes[*pos]) {
        *pos += 1;
    }
}

fn parse_expr(bytes: &[u8], pos: &mut usize) -> Result<f32, ()> {
    let mut value = parse_term(bytes, pos)?;
    loop {
        skip_space(bytes, pos);
        if *pos >= bytes.len() {
            break;
        }
        match bytes[*pos] {
            b'+' => {
                *pos += 1;
                value += parse_term(bytes, pos)?;
            }
            b'-' => {
                *pos += 1;
                value -= parse_term(bytes, pos)?;
            }
            _ => break,
        }
    }
    Ok(value)
}

fn parse_term(bytes: &[u8], pos: &mut usize) -> Result<f32, ()> {
    let mut value = parse_factor(bytes, pos)?;
    loop {
        skip_space(bytes, pos);
        if *pos >= bytes.len() {
            break;
        }
        match bytes[*pos] {
            b'*' => {
                *pos += 1;
                value *= parse_factor(bytes, pos)?;
            }
            b'/' => {
                *pos += 1;
                let rhs = parse_factor(bytes, pos)?;
                if rhs == 0.0 {
                    return Err(());
                }
                value /= rhs;
            }
            _ => break,
        }
    }
    Ok(value)
}

fn parse_factor(bytes: &[u8], pos: &mut usize) -> Result<f32, ()> {
    skip_space(bytes, pos);
    if *pos >= bytes.len() {
        return Err(());
    }
    if bytes[*pos] == b'(' {
        *pos += 1;
        let value = parse_expr(bytes, pos)?;
        skip_space(bytes, pos);
        if *pos >= bytes.len() || bytes[*pos] != b')' {
            return Err(());
        }
        *pos += 1;
        return Ok(value);
    }
    if bytes[*pos] == b'-' {
        *pos += 1;
        return Ok(-parse_factor(bytes, pos)?);
    }
    if bytes[*pos] == b'+' {
        *pos += 1;
    }
    let start = *pos;
    while *pos < bytes.len() && (bytes[*pos].is_ascii_digit() || bytes[*pos] == b'.') {
        *pos += 1;
    }
    if start == *pos {
        return Err(());
    }
    std::str::from_utf8(&bytes[start..*pos])
        .ok()
        .and_then(|text| text.parse::<f32>().ok())
        .ok_or(())
}

pub(crate) fn drag_speed(step: f32, ui: &Ui) -> f32 {
    ui.input(|input| {
        if input.modifiers.ctrl {
            step * 0.1
        } else if input.modifiers.shift {
            step * 10.0
        } else {
            step
        }
    })
}

fn clamp_value(value: f32, spec: &NumberFieldSpec) -> f32 {
    let mut out = value;
    if let Some(min) = spec.min {
        out = out.max(min);
    }
    if let Some(max) = spec.max {
        out = out.min(max);
    }
    out
}

pub(crate) fn format_number(value: f32) -> String {
    if value.fract().abs() < 1e-4 {
        // Whole numbers have no decimals to trim: 60 must stay 60.
        let text = format!("{value:.0}");
        return if text == "-0" { "0".into() } else { text };
    }
    let text = format!("{value:.4}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" || trimmed == "-0" {
        "0".into()
    } else {
        trimmed.into()
    }
}

#[cfg(test)]
mod format_tests {
    use super::format_number;

    #[test]
    fn whole_numbers_keep_their_zeros() {
        assert_eq!(format_number(60.0), "60");
        assert_eq!(format_number(1000.0), "1000");
        assert_eq!(format_number(0.0), "0");
        assert_eq!(format_number(-0.0), "0");
        assert_eq!(format_number(0.1), "0.1");
        assert_eq!(format_number(2.5), "2.5");
    }
}

/// Paint the value half of a number row. Returns whether value, drag, or focus changed.
pub fn paint_number_field_value(
    ui: &mut Ui,
    id_salt: &str,
    value: &mut f32,
    step: f32,
    spec: &NumberFieldSpec,
    reset_tip: Option<&str>,
) -> (bool, NumberDragSignals, bool) {
    let id = ui.id().with("seal_number_field").with(id_salt);
    let mut drag = NumberDragSignals::default();

    let has_focus = ui.memory(|mem| mem.has_focus(id));
    let mut draft = ui.data_mut(|data| {
        let slot = data.get_temp_mut_or_insert_with(id, || {
            spec.initial_text.clone().unwrap_or_else(|| format_number(*value))
        });
        if !has_focus {
            if let Some(seed) = &spec.initial_text {
                if eval_number_expression(slot).is_none()
                    || eval_number_expression(slot).is_some_and(|v| (v - *value).abs() < 1e-4)
                {
                    *slot = seed.clone();
                }
            } else {
                *slot = format_number(*value);
            }
        }
        slot.clone()
    });

    let h = ui.spacing().interact_size.y;
    let (cell, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width().max(48.0), h), Sense::hover());
    let slot = ui.painter().add(egui::Shape::Noop);
    let small = egui::TextStyle::Small.resolve(ui.style());
    let unit_w = spec
        .unit_suffix
        .as_ref()
        .map(|unit| {
            ui.painter().layout_no_wrap(unit.clone(), small.clone(), Color32::GRAY).size().x + 8.0
        })
        .unwrap_or(0.0);
    let reset_w = if reset_tip.is_some() { crate::crisp::RESET_PX + 4.0 } else { 0.0 };
    // Every number field carries an LED bar and starts its text where the vector
    // cells do, so single values and X/Y/Z rows read as one family.
    let rect = egui::Rect::from_min_max(
        egui::pos2(cell.left() + 1.0 + crate::crisp::LED_BAR_PX + 6.0, cell.top()),
        egui::pos2(cell.right() - 4.0 - unit_w - reset_w, cell.bottom()),
    );

    if spec.range_bar {
        if let (Some(min), Some(max)) = (spec.min, spec.max) {
            let span = (max - min).max(1e-6);
            let t = ((*value - min) / span).clamp(0.0, 1.0);
            let accent = ui.visuals().selection.bg_fill;
            // A thin level strip along the bottom edge, so it never reads as focus.
            let fill_w = ((cell.width() - 2.0) * t).round();
            if fill_w > 0.5 {
                let fill = egui::Rect::from_min_size(
                    egui::pos2(
                        cell.left() + 1.0 + crate::crisp::LED_BAR_PX + 1.0,
                        cell.bottom() - 3.0,
                    ),
                    egui::vec2((fill_w - crate::crisp::LED_BAR_PX - 2.0).max(1.0), 2.0),
                );
                ui.painter().rect_filled(fill, 0.0, accent.gamma_multiply(0.8));
            }
        }
    }

    let response = ui.put(
        rect,
        egui::TextEdit::singleline(&mut draft)
            .id(id)
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(0, 2)))
            .vertical_align(egui::Align::Center)
            .desired_width(rect.width()),
    );
    let focused = response.has_focus();
    {
        let visuals = ui.visuals().clone();
        let mut shapes: Vec<egui::Shape> = Vec::new();
        crate::crisp::sunken(&mut shapes, cell, &visuals, focused, response.hovered());
        if spec.level_led {
            let (color, glow) = crate::crisp::lux_led(*value);
            crate::crisp::led_bar(&mut shapes, cell, color, glow);
        } else {
            crate::crisp::led_bar(&mut shapes, cell, crate::crisp::NEUTRAL_LED, 0.0);
        }
        if focused {
            let tw = ui
                .painter()
                .layout_no_wrap(
                    draft.clone(),
                    egui::TextStyle::Body.resolve(ui.style()),
                    Color32::WHITE,
                )
                .size()
                .x;
            let back = egui::Rect::from_min_max(
                egui::pos2(rect.left() - 3.0, cell.top() + 4.0),
                egui::pos2((rect.left() + tw + 7.0).min(rect.right()), cell.bottom() - 4.0),
            );
            crate::crisp::backing(&mut shapes, back, &visuals);
            if unit_w > 0.0 {
                let ux = cell.right() - 4.0 - reset_w;
                let back = egui::Rect::from_min_max(
                    egui::pos2(ux - unit_w + 2.0, cell.top() + 4.0),
                    egui::pos2(ux + 2.0, cell.bottom() - 4.0),
                );
                crate::crisp::backing(&mut shapes, back, &visuals);
            }
        }
        crate::crisp::place_behind(ui.painter(), slot, shapes);
    }
    // Scrub by dragging an unfocused field; a click types.
    let mut scrubbed = false;
    if !focused {
        let scrub = ui.interact(rect, id.with("scrub"), Sense::click_and_drag());
        if scrub.hovered() || scrub.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if scrub.drag_started() {
            drag.drag_started = true;
        }
        if scrub.dragged() {
            let speed = drag_speed(step, ui);
            let base = eval_number_expression(&draft).unwrap_or(*value);
            let next = clamp_value(base + scrub.drag_delta().x * speed, spec);
            if (next - *value).abs() > f32::EPSILON {
                *value = next;
                draft = format_number(next);
                scrubbed = true;
            }
        }
        if scrub.drag_stopped() {
            drag.drag_stopped = true;
        }
        if scrub.clicked() {
            focus_select_all(ui, id);
        }
    }
    if let Some(suffix) = &spec.unit_suffix {
        ui.painter().text(
            egui::pos2(cell.right() - 6.0 - reset_w, cell.center().y),
            egui::Align2::RIGHT_CENTER,
            suffix,
            small,
            ui.visuals().weak_text_color(),
        );
    }
    let mut reset = false;
    if let Some(tip) = reset_tip {
        let r = egui::Rect::from_center_size(
            egui::pos2(cell.right() - 3.0 - crate::crisp::RESET_PX / 2.0, cell.center().y),
            egui::vec2(crate::crisp::RESET_PX, crate::crisp::RESET_PX),
        );
        reset = crate::crisp::reset_button(ui, r, id.with("reset"), tip);
    }
    let mut changed = response.changed() || scrubbed;
    if response.dragged() {
        let speed = drag_speed(step, ui);
        let base = eval_number_expression(&draft).unwrap_or(*value);
        let next = clamp_value(base + response.drag_delta().x * speed, spec);
        if (next - *value).abs() > f32::EPSILON {
            *value = next;
            draft = format_number(next);
            changed = true;
        }
        drag.drag_started = true;
    }

    if response.lost_focus() {
        if let Some(parsed) = eval_number_expression(&draft) {
            let next = clamp_value(parsed, spec);
            if (next - *value).abs() > f32::EPSILON {
                *value = next;
                changed = true;
            }
            draft = format_number(next);
        }
    }

    ui.data_mut(|data| *data.get_temp_mut_or_insert_with(id, String::new) = draft);

    (changed, drag, reset)
}

/// Focus a text field and select its whole value, so typing replaces it.
/// The end index is clamped to the text by the field itself.
pub fn focus_select_all(ui: &mut egui::Ui, id: egui::Id) {
    ui.memory_mut(|mem| mem.request_focus(id));
    let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
    state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
        egui::text::CCursor::new(0),
        egui::text::CCursor::new(usize::MAX / 2),
    )));
    state.store(ui.ctx(), id);
}

#[cfg(test)]
mod select_all_tests {
    #[test]
    fn typing_after_click_replaces_the_value() {
        let ctx = egui::Context::default();
        let id = egui::Id::new("field");
        let mut value = String::from("12.5");
        let frame = |events: Vec<egui::Event>, select: bool, value: &mut String| {
            let input = egui::RawInput { events, ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                ui.add(egui::TextEdit::singleline(value).id(id));
                if select {
                    super::focus_select_all(ui, id);
                }
            });
            out.textures_delta.clear();
        };
        frame(vec![], false, &mut value);
        frame(vec![], true, &mut value);
        frame(vec![egui::Event::Text("7".into())], false, &mut value);
        assert_eq!(value, "7");
    }
}
