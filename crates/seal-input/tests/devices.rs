//! Raw pointer motion, pen axes, gamepad light and sensors, and binding them.

use seal_input::{
    deliver_light, format_pad_light, look_delta, parse_pad_light, pen_from_event, write_motion,
    write_pen, write_pointer, ActionState, Axis, AxisSource, Button, InputMap, PadBackend,
    PadMotion, PenEvent, PenTool, RawInput,
};

fn axis(raw: &RawInput, name: &str) -> f32 {
    raw.axis(&Axis::parse(name).unwrap())
}

#[test]
fn raw_pointer_stays_apart_from_cursor_motion() {
    let cursor = [16.0, -8.0];
    let raw_px = [32.0, 4.0];
    assert_ne!(look_delta(false, cursor, raw_px, true), look_delta(true, cursor, raw_px, true));
    assert_eq!(look_delta(false, cursor, raw_px, true), cursor);
    assert_eq!(look_delta(true, cursor, raw_px, true), raw_px);
    // No raw report: mouse look keeps the cursor even when the setting is on.
    assert_eq!(look_delta(true, cursor, raw_px, false), cursor);

    let mut raw = RawInput::default();
    write_pointer(&mut raw, cursor, raw_px);
    let cursor_x = axis(&raw, "mouse.dx");
    let raw_x = axis(&raw, "mouse.raw_dx");
    assert!((cursor_x - 16.0 / 64.0).abs() < 1e-3);
    assert!((raw_x - 32.0 / 64.0).abs() < 1e-3);
    assert_ne!(cursor_x, raw_x);
}

#[test]
fn pen_events_fill_pressure_tilt_and_the_eraser() {
    let flat = pen_from_event(PenEvent {
        pressure: 0.5,
        altitude: Some(0.0),
        azimuth: Some(0.0),
        eraser: false,
        down: true,
    });
    assert!((flat.pressure - 0.5).abs() < 1e-4);
    assert!((flat.tilt[0] - 1.0).abs() < 1e-3);
    assert!(flat.note.is_empty());
    assert_eq!(flat.tool, PenTool::Tip);

    let upright = pen_from_event(PenEvent {
        pressure: 1.0,
        altitude: Some(std::f32::consts::FRAC_PI_2),
        azimuth: None,
        eraser: true,
        down: true,
    });
    assert!(upright.tilt[0].abs() < 1e-3);
    assert_eq!(upright.tool, PenTool::Eraser);

    let bare = pen_from_event(PenEvent {
        pressure: 0.25,
        altitude: None,
        azimuth: None,
        eraser: false,
        down: true,
    });
    assert!(bare.note.contains("tilt"));

    let mut raw = RawInput::default();
    write_pen(&mut raw, &upright);
    assert!((axis(&raw, "pen.pressure") - 1.0).abs() < 1e-3);
    assert!(raw.is_down(&Button::parse("pen.eraser").unwrap()));
    assert!(!raw.is_down(&Button::parse("pen.tip").unwrap()));

    write_pen(&mut raw, &seal_input::PenSample::default());
    assert_eq!(axis(&raw, "pen.pressure"), 0.0);
    assert!(!raw.is_down(&Button::parse("pen.eraser").unwrap()));
}

#[test]
fn action_maps_bind_the_new_axes() {
    let map = InputMap::parse(
        "\
axis look_x = mouse.raw_dx scale 1
axis pressure = pen.pressure
axis lean = pen.tilt_x
axis spin = pad.gyro_y
axis shove = pad.accel_x
",
    )
    .unwrap();
    let mut raw = RawInput::default();
    write_pointer(&mut raw, [0.0, 0.0], [32.0, 0.0]);
    write_pen(
        &mut raw,
        &pen_from_event(PenEvent {
            pressure: 0.5,
            altitude: Some(0.0),
            azimuth: Some(0.0),
            eraser: false,
            down: true,
        }),
    );
    write_motion(&mut raw, &PadMotion::ready([0.0, 2.0, 0.0], [1.0, 0.0, 0.0]));
    let mut state = ActionState::default();
    state.update(&map, &raw);
    assert!((state.axis("look_x") - 0.5).abs() < 1e-3);
    assert!((state.axis("pressure") - 0.5).abs() < 1e-3);
    assert!((state.axis("lean") - 1.0).abs() < 1e-3);
    assert!((state.axis("spin") - 0.5).abs() < 1e-3);
    assert!((state.axis("shove") - 0.5).abs() < 1e-3);
    let _ = AxisSource::Analog {
        axis: Axis::parse("mouse.raw_dy").unwrap(),
        deadzone: 0.0,
        scale: 1.0,
    };
}

#[test]
fn unsupported_motion_clears_sensor_axes() {
    let mut raw = RawInput::default();
    write_motion(&mut raw, &PadMotion::ready([4.0, 0.0, 0.0], [0.0, 0.0, 0.0]));
    assert!(axis(&raw, "pad.gyro_x") > 0.9);
    write_motion(&mut raw, &PadMotion::unsupported("motion sensors are not supported"));
    assert_eq!(axis(&raw, "pad.gyro_x"), 0.0);
}

#[derive(Default)]
struct MockPad {
    light: Option<[f32; 3]>,
    gyro: [f32; 3],
    accel: [f32; 3],
}

impl PadBackend for MockPad {
    fn set_light_bar(&mut self, rgb: [f32; 3]) -> Result<(), String> {
        self.light = Some(rgb);
        Ok(())
    }
    fn motion(&self) -> PadMotion {
        PadMotion::ready(self.gyro, self.accel)
    }
    fn light_note(&self) -> String {
        String::new()
    }
}

#[test]
fn light_bar_and_sensors_reach_the_backend() {
    let mut pad = MockPad { gyro: [0.0, 1.0, 0.0], accel: [0.0, 0.0, 2.0], ..MockPad::default() };
    deliver_light(&mut pad, [1.2, f32::NAN, 0.25]).unwrap();
    assert_eq!(pad.light, Some([1.0, 0.0, 0.25]));
    let motion = PadBackend::motion(&pad);
    assert!(motion.unsupported.is_empty());
    assert_eq!(motion.gyro[1], 1.0);
    assert_eq!(motion.accel[2], 2.0);
    assert!(pad.light_note().is_empty());
}

#[test]
fn pad_light_text_rejects_damaged_input_and_keeps_empty() {
    assert_eq!(parse_pad_light("").unwrap(), None);
    assert_eq!(parse_pad_light("  ").unwrap(), None);
    assert!(parse_pad_light("red").is_err());
    assert!(parse_pad_light("1,2").is_err());
    assert!(parse_pad_light("0,0,256").is_err());
    assert!(parse_pad_light("0,no,0").is_err());
    let rgb = parse_pad_light("255, 0, 128").unwrap().unwrap();
    assert!((rgb[0] - 1.0).abs() < 1e-4);
    assert!((rgb[2] - 128.0 / 255.0).abs() < 1e-4);
    assert_eq!(format_pad_light(rgb), "255,0,128");
}
