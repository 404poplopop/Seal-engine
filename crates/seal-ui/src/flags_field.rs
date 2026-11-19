//! Multi-check dropdown for bit-set inspector fields (collision layers and masks).

use egui::Sense;

/// Popup with per-bit checkboxes plus All / None. Returns the new mask when it changes.
pub fn paint_flags_popup(
    ui: &mut egui::Ui,
    id_salt: &str,
    bits: u32,
    bit_count: u32,
    labels: &[String],
    all_label: &str,
    none_label: &str,
) -> Option<u32> {
    let mut next = bits;
    let mut changed = false;
    ui.set_min_width(220.0);
    ui.horizontal(|ui| {
        if ui.button(all_label).clicked() {
            next = u32::MAX;
            changed = true;
        }
        if ui.button(none_label).clicked() {
            next = 0;
            changed = true;
        }
    });
    if changed {
        return Some(next);
    }
    ui.separator();
    for index in 0..bit_count {
        let bit = 1u32 << index;
        let on = bits & bit != 0;
        let name = labels.get(index as usize).map(String::as_str).unwrap_or("?");
        let mut check = on;
        let box_hit =
            ui.push_id(egui::Id::new(id_salt).with(index), |ui| ui.checkbox(&mut check, name));
        if box_hit.inner.changed() {
            if check {
                next |= bit;
            } else {
                next &= !bit;
            }
            changed = true;
        }
    }
    changed.then_some(next)
}

/// Value cell: summary text and a click opens the flags popup.
pub fn paint_flags_value(
    ui: &mut egui::Ui,
    id_salt: &str,
    bits: u32,
    bit_count: u32,
    labels: &[String],
    all_label: &str,
    none_label: &str,
) -> Option<u32> {
    let h = ui.spacing().interact_size.y;
    let (cell, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width().max(72.0), h), Sense::click());
    let count_on = (0..bit_count).filter(|i| bits & (1 << i) != 0).count();
    let summary = format!("{count_on} / {bit_count}");
    ui.painter().text(
        egui::pos2(cell.left() + 4.0, cell.center().y),
        egui::Align2::LEFT_CENTER,
        summary,
        egui::FontId::proportional(crate::look::SMALL_PX),
        ui.visuals().text_color(),
    );
    let mut picked = None;
    if response.clicked() {
        egui::Popup::from_toggle_button_response(&response)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
            .show(|ui| {
                picked =
                    paint_flags_popup(ui, id_salt, bits, bit_count, labels, all_label, none_label);
            });
    }
    picked
}
