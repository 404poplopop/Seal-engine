//! One frame of pointer, pen and gamepad-extra state for play and the
//! input bindings page.

use seal_editor_core::controls::Controls;
use seal_editor_core::seal_input::{
    look_delta, pen_from_event, write_motion, write_pen, write_pointer, PadMotion, PenEvent,
    PenSample,
};
use seal_editor_core::{InputDevice, InputLive};

pub fn read_frame(ctx: &egui::Context) -> (PointerBits, PenSample, Vec<InputDevice>) {
    ctx.input(|input| {
        let delta = input.pointer.delta();
        let motion = input.pointer.motion();
        let mut pen = PenSample::default();
        let mut devices = vec![InputDevice { id: "System pointer".into(), kind: "mouse".into() }];
        for event in &input.events {
            let egui::Event::Touch { force: Some(force), phase, device_id, id, .. } = event else {
                continue;
            };
            let down = !matches!(phase, egui::TouchPhase::End | egui::TouchPhase::Cancel);
            pen = pen_from_event(PenEvent {
                pressure: *force,
                altitude: None,
                azimuth: None,
                eraser: false,
                down,
            });
            let name = format!("Pen {device_id:?}-{id:?}");
            if !devices.iter().any(|device| device.id == name) {
                devices.push(InputDevice { id: name, kind: "pen".into() });
            }
        }
        let held = |button| input.pointer.button_down(button);
        (
            PointerBits {
                buttons: [
                    held(egui::PointerButton::Primary),
                    held(egui::PointerButton::Secondary),
                    held(egui::PointerButton::Middle),
                ],
                cursor: [delta.x, delta.y],
                raw: motion.map(|m| [m.x, m.y]).unwrap_or([0.0, 0.0]),
                raw_known: motion.is_some(),
            },
            pen,
            devices,
        )
    })
}

pub struct PointerBits {
    /// Left, right and middle mouse buttons held, for `mouse.left` style bindings.
    pub buttons: [bool; 3],
    pub cursor: [f32; 2],
    pub raw: [f32; 2],
    pub raw_known: bool,
}

/// Copy this frame into the controls the game reads. Axes land on `raw`
/// after the caller has polled the pad, so a pad clear does not drop them.
pub fn write_controls(
    controls: &mut Controls,
    pointer: &PointerBits,
    pen: &PenSample,
    motion: &PadMotion,
    light_note: &str,
    use_raw_look: bool,
) {
    controls.mouse_delta = pointer.cursor;
    controls.raw_mouse = pointer.raw;
    controls.raw_mouse_known = pointer.raw_known;
    controls.look_delta = look_delta(use_raw_look, pointer.cursor, pointer.raw, pointer.raw_known);
    controls.pen_pressure = if pen.down { pen.pressure } else { 0.0 };
    controls.pen_tilt = pen.tilt;
    controls.pen_eraser = pen.down && pen.tool == seal_editor_core::seal_input::PenTool::Eraser;
    controls.pen_note.clone_from(&pen.note);
    controls.pad_gyro = motion.gyro;
    controls.pad_accel = motion.accel;
    controls.pad_motion_note.clone_from(&motion.unsupported);
    controls.pad_light_note = light_note.to_string();
    let raw_px = if pointer.raw_known { pointer.raw } else { [0.0, 0.0] };
    write_pointer(&mut controls.raw, pointer.cursor, raw_px);
    for (name, held) in ["mouse.left", "mouse.right", "mouse.middle"].iter().zip(pointer.buttons) {
        if let Ok(button) = seal_editor_core::seal_input::Button::parse(name) {
            if held {
                controls.raw.press(button);
            } else {
                controls.raw.release(&button);
            }
        }
    }
    write_pen(&mut controls.raw, pen);
    write_motion(&mut controls.raw, motion);
}

pub fn live(
    pointer: &PointerBits,
    pen: &PenSample,
    mut devices: Vec<InputDevice>,
    pads: Vec<(String, String)>,
    motion: &PadMotion,
    light_note: &str,
) -> InputLive {
    for (name, kind) in pads {
        if !devices.iter().any(|device| device.id == name) {
            devices.push(InputDevice { id: name, kind });
        }
    }
    InputLive {
        cursor: pointer.cursor,
        raw_mouse: pointer.raw,
        raw_known: pointer.raw_known,
        devices,
        pen_pressure: if pen.down { pen.pressure } else { 0.0 },
        pen_tilt: pen.tilt,
        pen_eraser: pen.down && pen.tool == seal_editor_core::seal_input::PenTool::Eraser,
        pen_down: pen.down,
        pen_note: pen.note.clone(),
        gyro: motion.gyro,
        accel: motion.accel,
        motion_note: motion.unsupported.clone(),
        light_note: light_note.to_string(),
    }
}

pub fn store(ctx: &egui::Context, live: InputLive) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new(seal_editor_core::INPUT_LIVE_KEY), live));
}
