//! Live pointer, pen and gamepad readings the input page shows.
//! The window fills one of these each frame; it is not saved.

/// Project setting: mouse look uses unaccelerated pointer motion.
pub const MOUSE_LOOK_RAW: &str = "input.mouse_look_raw";
/// Project setting: gamepad light bar colour, `red,green,blue` from 0 to 255, or empty.
pub const PAD_LIGHT: &str = "input.pad_light";
/// Key for the live reading stored on the interface for one frame.
pub const INPUT_LIVE_KEY: &str = "seal-input-live";

/// A device the window has seen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputDevice {
    pub id: String,
    /// `mouse`, `pen` or `pad`.
    pub kind: String,
}

/// What the input page draws for this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct InputLive {
    pub cursor: [f32; 2],
    pub raw_mouse: [f32; 2],
    pub raw_known: bool,
    pub devices: Vec<InputDevice>,
    pub pen_pressure: f32,
    pub pen_tilt: [f32; 2],
    pub pen_eraser: bool,
    pub pen_down: bool,
    pub pen_note: String,
    pub gyro: [f32; 3],
    pub accel: [f32; 3],
    pub motion_note: String,
    pub light_note: String,
}

impl Default for InputLive {
    fn default() -> Self {
        Self {
            cursor: [0.0; 2],
            raw_mouse: [0.0; 2],
            raw_known: false,
            devices: Vec::new(),
            pen_pressure: 0.0,
            pen_tilt: [0.0; 2],
            pen_eraser: false,
            pen_down: false,
            pen_note: String::new(),
            gyro: [0.0; 3],
            accel: [0.0; 3],
            motion_note: String::new(),
            light_note: String::new(),
        }
    }
}
