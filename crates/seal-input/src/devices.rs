//! Pointer, pen and gamepad-sensor samples, written into a [`RawInput`].
//!
//! Cursor motion and raw motion stay on different axes. A pen event fills
//! pressure, tilt and the tip or eraser button. Sensor values are scaled
//! into the same -1..1 range every other axis uses.

use super::{Axis, Button, RawInput};

/// Pixels of pointer motion that become a full axis deflection.
pub const MOUSE_AXIS_PIXELS: f32 = 64.0;
/// Radians per second of gyro that become a full axis deflection.
pub const GYRO_FULL_SCALE: f32 = 4.0;
/// Gravities of acceleration that become a full axis deflection.
pub const ACCEL_FULL_SCALE: f32 = 2.0;

/// Cursor motion this frame, in screen pixels. `raw` is unaccelerated motion
/// from the pointing device. `raw_known` is false when the window did not
/// report that motion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerSample {
    pub cursor: [f32; 2],
    pub raw: [f32; 2],
    pub raw_known: bool,
}

/// Which end of a pen is touching.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PenTool {
    Tip,
    Eraser,
}

/// One pen sample. `tilt` is -1..1; 0 is upright. `note` is empty when tilt
/// was part of the event.
#[derive(Clone, Debug, PartialEq)]
pub struct PenSample {
    pub pressure: f32,
    pub tilt: [f32; 2],
    pub tool: PenTool,
    pub down: bool,
    pub note: String,
}

impl Default for PenSample {
    fn default() -> Self {
        Self {
            pressure: 0.0,
            tilt: [0.0, 0.0],
            tool: PenTool::Tip,
            down: false,
            note: String::new(),
        }
    }
}

/// A pen event before it is scaled into axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PenEvent {
    /// 0 is no pressure, 1 is the device maximum.
    pub pressure: f32,
    /// Radians from the surface. None when the event has no tilt.
    pub altitude: Option<f32>,
    /// Radians around upright. None when the event has no direction.
    pub azimuth: Option<f32>,
    pub eraser: bool,
    pub down: bool,
}

/// Gyro in radians per second and acceleration in gravities.
/// `unsupported` is empty when the numbers were read, otherwise a sentence
/// the player can be shown.
#[derive(Clone, Debug, PartialEq)]
pub struct PadMotion {
    pub gyro: [f32; 3],
    pub accel: [f32; 3],
    pub unsupported: String,
}

impl PadMotion {
    pub fn ready(gyro: [f32; 3], accel: [f32; 3]) -> Self {
        Self { gyro: finite3(gyro), accel: finite3(accel), unsupported: String::new() }
    }

    pub fn unsupported(why: impl Into<String>) -> Self {
        Self { gyro: [0.0; 3], accel: [0.0; 3], unsupported: why.into() }
    }
}

/// What a gamepad backend can do besides buttons, sticks and rumble.
pub trait PadBackend {
    fn set_light_bar(&mut self, rgb: [f32; 3]) -> Result<(), String>;
    fn motion(&self) -> PadMotion;
    /// Empty when a light bar can be set. Otherwise why it cannot.
    fn light_note(&self) -> String;
}

/// Send a light-bar colour, clamped to 0..1. Non-finite channels become 0.
pub fn deliver_light(pad: &mut dyn PadBackend, rgb: [f32; 3]) -> Result<(), String> {
    pad.set_light_bar(rgb.map(unit))
}

/// Mouse-look pixels. Raw motion is used only when the project asks for it
/// and the window reported some.
pub fn look_delta(use_raw: bool, cursor: [f32; 2], raw: [f32; 2], raw_known: bool) -> [f32; 2] {
    if use_raw && raw_known {
        finite2(raw)
    } else {
        finite2(cursor)
    }
}

/// Build a pen sample from a touch or pen event.
pub fn pen_from_event(event: PenEvent) -> PenSample {
    let pressure = unit(event.pressure);
    let (tilt, note) = tilt_from_angles(event.altitude, event.azimuth, event.down);
    PenSample {
        pressure: if event.down { pressure } else { 0.0 },
        tilt,
        tool: if event.eraser { PenTool::Eraser } else { PenTool::Tip },
        down: event.down,
        note,
    }
}

/// Write cursor and raw pointer axes. Mouse buttons are left as they are.
pub fn write_pointer(raw: &mut RawInput, cursor_px: [f32; 2], raw_px: [f32; 2]) {
    set_scaled(raw, "mouse.dx", cursor_px[0], MOUSE_AXIS_PIXELS);
    set_scaled(raw, "mouse.dy", cursor_px[1], MOUSE_AXIS_PIXELS);
    set_scaled(raw, "mouse.raw_dx", raw_px[0], MOUSE_AXIS_PIXELS);
    set_scaled(raw, "mouse.raw_dy", raw_px[1], MOUSE_AXIS_PIXELS);
}

/// Replace the pen axes and the tip or eraser button.
pub fn write_pen(raw: &mut RawInput, sample: &PenSample) {
    raw.clear_device("pen");
    if !sample.down {
        return;
    }
    set_unit(raw, "pen.pressure", sample.pressure);
    set_unit(raw, "pen.tilt_x", sample.tilt[0]);
    set_unit(raw, "pen.tilt_y", sample.tilt[1]);
    let name = if sample.tool == PenTool::Eraser { "pen.eraser" } else { "pen.tip" };
    if let Ok(button) = Button::parse(name) {
        raw.press(button);
    }
}

/// Replace gyro and accelerometer axes. An unsupported reading clears them.
pub fn write_motion(raw: &mut RawInput, motion: &PadMotion) {
    for name in
        ["pad.gyro_x", "pad.gyro_y", "pad.gyro_z", "pad.accel_x", "pad.accel_y", "pad.accel_z"]
    {
        set_unit(raw, name, 0.0);
    }
    if !motion.unsupported.is_empty() {
        return;
    }
    set_scaled(raw, "pad.gyro_x", motion.gyro[0], GYRO_FULL_SCALE);
    set_scaled(raw, "pad.gyro_y", motion.gyro[1], GYRO_FULL_SCALE);
    set_scaled(raw, "pad.gyro_z", motion.gyro[2], GYRO_FULL_SCALE);
    set_scaled(raw, "pad.accel_x", motion.accel[0], ACCEL_FULL_SCALE);
    set_scaled(raw, "pad.accel_y", motion.accel[1], ACCEL_FULL_SCALE);
    set_scaled(raw, "pad.accel_z", motion.accel[2], ACCEL_FULL_SCALE);
}

/// `text` is empty, or `red,green,blue` with each channel a whole number from 0 to 255.
/// Empty is an unset colour, not an error.
pub fn parse_pad_light(text: &str) -> Result<Option<[f32; 3]>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let parts: Vec<&str> = text.split(',').map(str::trim).collect();
    if parts.len() != 3 {
        return Err("light bar colour is red, green, blue from 0 to 255".into());
    }
    let mut rgb = [0.0; 3];
    for (i, part) in parts.iter().enumerate() {
        let channel: u16 = part
            .parse()
            .map_err(|_| "light bar colour is red, green, blue from 0 to 255".to_string())?;
        if channel > 255 {
            return Err("light bar colour is red, green, blue from 0 to 255".into());
        }
        rgb[i] = channel as f32 / 255.0;
    }
    Ok(Some(rgb))
}

/// Store a colour the way [`parse_pad_light`] reads it.
pub fn format_pad_light(rgb: [f32; 3]) -> String {
    let channel = |v: f32| (unit(v) * 255.0).round() as u16;
    format!("{},{},{}", channel(rgb[0]), channel(rgb[1]), channel(rgb[2]))
}

fn tilt_from_angles(altitude: Option<f32>, azimuth: Option<f32>, down: bool) -> ([f32; 2], String) {
    let Some(altitude) = altitude.filter(|v| v.is_finite()) else {
        let note = if down {
            "tilt was not reported by this pen event".to_string()
        } else {
            String::new()
        };
        return ([0.0, 0.0], note);
    };
    let upright = std::f32::consts::FRAC_PI_2;
    let lean = (1.0 - (altitude / upright).clamp(0.0, 1.0)).clamp(0.0, 1.0);
    let azimuth = azimuth.filter(|v| v.is_finite()).unwrap_or(0.0);
    ([lean * azimuth.cos(), lean * azimuth.sin()], String::new())
}

fn set_scaled(raw: &mut RawInput, name: &str, value: f32, full: f32) {
    let scaled = if full.abs() < f32::EPSILON { 0.0 } else { value / full };
    set_unit(raw, name, scaled);
}

fn set_unit(raw: &mut RawInput, name: &str, value: f32) {
    if let Ok(axis) = Axis::parse(name) {
        raw.set_axis(axis, unit(value));
    }
}

fn unit(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

fn finite2(v: [f32; 2]) -> [f32; 2] {
    v.map(|n| if n.is_finite() { n } else { 0.0 })
}

fn finite3(v: [f32; 3]) -> [f32; 3] {
    v.map(|n| if n.is_finite() { n } else { 0.0 })
}
