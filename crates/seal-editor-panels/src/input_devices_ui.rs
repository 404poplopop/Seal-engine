//! Input devices on the bindings page and in Settings: raw mouse look,
//! the pen, the gamepad light bar and motion sensors.

use seal_editor_core::seal_input::{format_pad_light, parse_pad_light};
use seal_editor_core::{
    CommandArgs, EditorSession, InputLive, INPUT_LIVE_KEY, MOUSE_LOOK_RAW, PAD_LIGHT,
};
use seal_ui::colour_picker::{self, ColourLabels};
use seal_ui::{PanelFrame, PropertyCheckbox};

use crate::PanelEvent;

const SOURCES: &[(&str, &str)] = &[
    ("mouse.raw_dx", "input-axis-raw-x"),
    ("mouse.raw_dy", "input-axis-raw-y"),
    ("pen.pressure", "input-axis-pressure"),
    ("pen.tilt_x", "input-axis-tilt-x"),
    ("pen.tilt_y", "input-axis-tilt-y"),
    ("pad.gyro_x", "input-axis-gyro-x"),
    ("pad.gyro_y", "input-axis-gyro-y"),
    ("pad.gyro_z", "input-axis-gyro-z"),
    ("pad.accel_x", "input-axis-accel-x"),
    ("pad.accel_y", "input-axis-accel-y"),
    ("pad.accel_z", "input-axis-accel-z"),
];

/// The devices section of Edit > Input Bindings.
pub fn section(
    frame: &mut PanelFrame,
    session: &EditorSession,
    out: &mut Vec<(String, CommandArgs)>,
) {
    let text = |key: &str| session.messages.get(key);
    let live = frame
        .ui_mut()
        .data(|data| data.get_temp::<InputLive>(egui::Id::new(INPUT_LIVE_KEY)))
        .unwrap_or_default();
    frame.section(&text("input-devices-title"), |frame| {
        let look = setting_value(session, MOUSE_LOOK_RAW);
        if let Some(next) = checkbox(frame, &text("setting-input-mouse-look-raw"), look == "true") {
            out.push(setting_cmd(session, MOUSE_LOOK_RAW, if next { "true" } else { "false" }));
        }
        frame.muted(&text("setting-desc-input-mouse-look-raw"));
        frame.readout(&text("input-devices-cursor"), &pair(live.cursor));
        if live.raw_known {
            frame.readout(&text("input-devices-raw"), &pair(live.raw_mouse));
        } else {
            frame.readout(&text("input-devices-raw"), &text("input-devices-raw-missing"));
        }
        if live.devices.is_empty() {
            frame.muted(&text("input-devices-empty"));
        } else {
            for device in &live.devices {
                let kind = match device.kind.as_str() {
                    "pen" => text("input-devices-kind-pen"),
                    "pad" => text("input-devices-kind-pad"),
                    _ => text("input-devices-kind-mouse"),
                };
                frame.readout(&device.id, &kind);
            }
        }
        if live.pen_down {
            frame
                .readout(&text("input-devices-pen-pressure"), &format!("{:.2}", live.pen_pressure));
            frame.readout(&text("input-devices-pen-tilt"), &pair(live.pen_tilt));
            let tool = if live.pen_eraser {
                text("input-devices-pen-eraser")
            } else {
                text("input-devices-pen-tip")
            };
            frame.readout(&text("input-devices-pen-tool"), &tool);
            if !live.pen_note.is_empty() {
                frame.muted(&live.pen_note);
            }
        } else {
            frame.muted(&text("input-devices-pen-none"));
        }
        if live.motion_note.is_empty() {
            frame.readout(&text("input-devices-gyro"), &triple(live.gyro));
            frame.readout(&text("input-devices-accel"), &triple(live.accel));
        } else {
            frame.muted(&live.motion_note);
        }
        light_row(frame, session, &setting_value(session, PAD_LIGHT), out);
        if !live.light_note.is_empty() {
            frame.muted(&live.light_note);
        }
        bind_row(frame, session, out);
    });
}

/// Settings rows for the two input project settings. Other settings stay on the shared path.
pub fn setting_row(
    frame: &mut PanelFrame,
    session: &EditorSession,
    id: &str,
    title: &str,
    description: &str,
    value: &str,
) -> PanelEvent {
    let mut out = Vec::new();
    if id == MOUSE_LOOK_RAW {
        if let Some(next) = checkbox(frame, title, value == "true") {
            out.push(setting_cmd(session, id, if next { "true" } else { "false" }));
        }
        frame.muted(description);
    } else if id == PAD_LIGHT {
        light_row(frame, session, value, &mut out);
        frame.muted(description);
    }
    if out.is_empty() {
        PanelEvent::None
    } else {
        PanelEvent::Commands(out)
    }
}

fn checkbox(frame: &mut PanelFrame, title: &str, on: bool) -> Option<bool> {
    let value = if on { PropertyCheckbox::On } else { PropertyCheckbox::Off };
    let mut menu_pick = None;
    frame
        .property_checkbox_row(
            title,
            value,
            false,
            None,
            None,
            &mut menu_pick,
            Some(MOUSE_LOOK_RAW),
        )
        .map(|next| matches!(next, PropertyCheckbox::On))
}

fn light_row(
    frame: &mut PanelFrame,
    session: &EditorSession,
    current: &str,
    out: &mut Vec<(String, CommandArgs)>,
) {
    let text = |key: &str| session.messages.get(key);
    let parsed = parse_pad_light(current).ok().flatten().unwrap_or([0.0, 0.0, 0.0]);
    let draft_id = frame.ui_mut().id().with("input-pad-light");
    let mut rgba = frame
        .ui_mut()
        .data(|data| data.get_temp::<[f32; 4]>(draft_id))
        .unwrap_or([parsed[0], parsed[1], parsed[2], 1.0]);
    let mut recent;
    let mut change = colour_picker::ColourChange { changed: false, commit: false };
    let title = text("setting-input-pad-light");
    let labels = ColourLabels {
        hex: text("picker-hex"),
        alpha: text("picker-alpha"),
        recent: text("picker-recent"),
    };
    {
        let ui = frame.ui_mut();
        colour_picker::seed_recent(ui, &[]);
        recent = colour_picker::recent_now(ui);
        ui.horizontal(|ui| {
            seal_ui::property_name(ui, &title, false, None);
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
            let fill = egui::Color32::from_rgb(
                (rgba[0] * 255.0) as u8,
                (rgba[1] * 255.0) as u8,
                (rgba[2] * 255.0) as u8,
            );
            ui.painter().rect_filled(rect.shrink(1.0), 2.0, fill);
            egui::Popup::menu(&response).show(|ui| {
                change = colour_picker::edit(ui, &mut rgba, &mut recent, &labels);
            });
        });
    }
    if change.changed {
        frame.ui_mut().data_mut(|data| data.insert_temp(draft_id, rgba));
    }
    colour_picker::store_recent(frame.ui_mut(), recent);
    if change.commit {
        frame.ui_mut().data_mut(|data| data.remove::<[f32; 4]>(draft_id));
        let value = format_pad_light([rgba[0], rgba[1], rgba[2]]);
        out.push(setting_cmd(session, PAD_LIGHT, &value));
    }
    if !current.trim().is_empty() && frame.link(&text("command-settings-reset")) {
        out.push(("settings.reset".into(), CommandArgs::new(&[("id", PAD_LIGHT)])));
    }
}

fn bind_row(frame: &mut PanelFrame, session: &EditorSession, out: &mut Vec<(String, CommandArgs)>) {
    let text = |key: &str| session.messages.get(key);
    let ctx = frame.ui_mut().ctx().clone();
    let source_id = egui::Id::new("input-bind-axis-source");
    let name_id = egui::Id::new("input-bind-axis-name");
    let mut source: String =
        ctx.data(|data| data.get_temp(source_id)).unwrap_or_else(|| "mouse.raw_dx".into());
    let options: Vec<(&str, String)> = SOURCES.iter().map(|(id, key)| (*id, text(key))).collect();
    if let Some(pick) = frame.choice(&text("input-devices-bind-source"), &source, &options) {
        source = pick;
        ctx.data_mut(|data| data.insert_temp(source_id, source.clone()));
    }
    let mut name: String = ctx.data(|data| data.get_temp(name_id)).unwrap_or_default();
    let (changed, committed) = frame.draft_field(&text("input-devices-bind-name"), &mut name);
    if changed {
        ctx.data_mut(|data| data.insert_temp(name_id, name.clone()));
    }
    if (committed || frame.button(&text("input-devices-bind-add"))) && !name.trim().is_empty() {
        ctx.data_mut(|data| data.remove::<String>(name_id));
        out.push((
            "asset.input_bind_axis".into(),
            CommandArgs::new(&[("name", name.trim()), ("source", source.as_str())]),
        ));
    }
}

fn setting_value(session: &EditorSession, id: &str) -> String {
    session
        .models
        .settings
        .iter()
        .find(|setting| setting.id == id)
        .map(|setting| setting.value().to_string())
        .unwrap_or_default()
}

fn setting_cmd(session: &EditorSession, id: &str, value: &str) -> (String, CommandArgs) {
    let layer = if session.project.is_some() { "project" } else { "user" };
    ("settings.set".into(), CommandArgs::new(&[("id", id), ("value", value), ("layer", layer)]))
}

fn pair(v: [f32; 2]) -> String {
    format!("{:.1}, {:.1}", v[0], v[1])
}

fn triple(v: [f32; 3]) -> String {
    format!("{:.2}, {:.2}, {:.2}", v[0], v[1], v[2])
}
