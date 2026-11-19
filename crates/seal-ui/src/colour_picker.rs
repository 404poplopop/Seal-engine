//! Colour popup: saturation/value square, hue, alpha, hex, and recent colours.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};

use seal_editor_core::{
    hex_to_rgba, hsv_to_rgb, remember_colour_list, rgb_to_hsv, rgba_to_hex, Rgba,
};

pub struct ColourLabels {
    pub hex: String,
    pub alpha: String,
    pub recent: String,
}

pub struct ColourChange {
    pub changed: bool,
    pub commit: bool,
}

/// Edit `rgba` (0 to 1). Recent colours update when the gesture ends or a swatch is chosen.
pub fn edit(
    ui: &mut egui::Ui,
    rgba: &mut [f32; 4],
    recent: &mut Vec<String>,
    labels: &ColourLabels,
) -> ColourChange {
    let mut changed = false;
    let mut commit = false;
    let hsv = rgb_to_hsv(rgba[0], rgba[1], rgba[2]);
    let mut hue = hsv.h;
    let mut sat = hsv.s;
    let mut val = hsv.v;
    ui.horizontal(|ui| {
        let square = Vec2::splat(140.0);
        let (rect, response) = ui.allocate_exact_size(square, Sense::click_and_drag());
        paint_square(ui, rect, hue);
        if response.dragged() || response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let local = (pos - rect.min) / rect.size();
                sat = local.x.clamp(0.0, 1.0);
                val = 1.0 - local.y.clamp(0.0, 1.0);
                changed = true;
            }
        }
        if response.drag_stopped() || response.clicked() {
            commit = true;
        }
        let (hue_rect, hue_response) =
            ui.allocate_exact_size(Vec2::new(16.0, 140.0), Sense::click_and_drag());
        paint_hue(ui, hue_rect);
        if hue_response.dragged() || hue_response.clicked() {
            if let Some(pos) = hue_response.interact_pointer_pos() {
                let t = ((pos.y - hue_rect.top()) / hue_rect.height()).clamp(0.0, 1.0);
                hue = t * 360.0;
                changed = true;
            }
        }
        if hue_response.drag_stopped() || hue_response.clicked() {
            commit = true;
        }
    });
    if changed {
        let (r, g, b) = hsv_to_rgb(seal_editor_core::Hsv { h: hue, s: sat, v: val });
        rgba[0] = r;
        rgba[1] = g;
        rgba[2] = b;
    }
    ui.horizontal(|ui| {
        ui.label(&labels.alpha);
        let mut alpha = rgba[3];
        if ui.add(egui::Slider::new(&mut alpha, 0.0..=1.0).fixed_decimals(2)).changed() {
            rgba[3] = alpha;
            changed = true;
            commit = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label(&labels.hex);
        let mut hex = rgba_to_hex(Rgba { r: rgba[0], g: rgba[1], b: rgba[2], a: rgba[3] });
        let edit = ui.add(egui::TextEdit::singleline(&mut hex).desired_width(110.0));
        if edit.changed() {
            if let Some(parsed) = hex_to_rgba(&hex) {
                rgba[0] = parsed.r;
                rgba[1] = parsed.g;
                rgba[2] = parsed.b;
                rgba[3] = parsed.a;
                changed = true;
            }
        }
        if edit.lost_focus() {
            commit = true;
        }
    });
    if !recent.is_empty() {
        ui.label(&labels.recent);
        ui.horizontal_wrapped(|ui| {
            for colour in recent.clone() {
                let parsed = hex_to_rgba(&colour);
                let fill = parsed
                    .map(|c| {
                        Color32::from_rgba_unmultiplied(byte(c.r), byte(c.g), byte(c.b), byte(c.a))
                    })
                    .unwrap_or(Color32::GRAY);
                let (rect, response) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
                ui.painter().rect_filled(rect.shrink(1.0), 2.0, fill);
                ui.painter().rect_stroke(
                    rect.shrink(1.0),
                    2.0,
                    Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
                    egui::StrokeKind::Inside,
                );
                if response.clicked() {
                    if let Some(parsed) = parsed {
                        rgba[0] = parsed.r;
                        rgba[1] = parsed.g;
                        rgba[2] = parsed.b;
                        rgba[3] = parsed.a;
                        changed = true;
                        commit = true;
                    }
                }
            }
        });
    }
    if commit {
        remember_colour_list(
            recent,
            &rgba_to_hex(Rgba { r: rgba[0], g: rgba[1], b: rgba[2], a: rgba[3] }),
        );
    }
    ColourChange { changed, commit }
}

fn byte(channel: f32) -> u8 {
    (channel.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn paint_square(ui: &egui::Ui, rect: Rect, hue: f32) {
    let steps = 16;
    let cell = rect.size() / steps as f32;
    for y in 0..steps {
        for x in 0..steps {
            let s = x as f32 / (steps - 1) as f32;
            let v = 1.0 - y as f32 / (steps - 1) as f32;
            let (r, g, b) = hsv_to_rgb(seal_editor_core::Hsv { h: hue, s, v });
            let min = rect.min + Vec2::new(x as f32 * cell.x, y as f32 * cell.y);
            ui.painter().rect_filled(
                Rect::from_min_size(min, cell + Vec2::splat(0.5)),
                0.0,
                Color32::from_rgb(byte(r), byte(g), byte(b)),
            );
        }
    }
}

fn paint_hue(ui: &egui::Ui, rect: Rect) {
    let steps = 24;
    let height = rect.height() / steps as f32;
    for step in 0..steps {
        let hue = step as f32 / (steps - 1) as f32 * 360.0;
        let (r, g, b) = hsv_to_rgb(seal_editor_core::Hsv { h: hue, s: 1.0, v: 1.0 });
        let min = Pos2::new(rect.left(), rect.top() + step as f32 * height);
        ui.painter().rect_filled(
            Rect::from_min_size(min, Vec2::new(rect.width(), height + 0.5)),
            0.0,
            Color32::from_rgb(byte(r), byte(g), byte(b)),
        );
    }
}

pub fn recent_id() -> egui::Id {
    egui::Id::new("seal.recent_colours")
}

/// Load saved colours into the frame once.
pub fn seed_recent(ui: &mut egui::Ui, stored: &[String]) {
    let id = recent_id();
    if ui.data(|data| data.get_temp::<Vec<String>>(id)).is_none() {
        ui.data_mut(|data| data.insert_temp(id, stored.to_vec()));
    }
}

pub fn recent_now(ui: &egui::Ui) -> Vec<String> {
    ui.data(|data| data.get_temp::<Vec<String>>(recent_id())).unwrap_or_default()
}

pub fn store_recent(ui: &mut egui::Ui, recent: Vec<String>) {
    ui.data_mut(|data| data.insert_temp(recent_id(), recent));
}
