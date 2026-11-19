//! Graph nodes for raw pointer motion, pen axes, and gamepad light and sensors.

use crate::rt::{Host, Value};

/// Which device reading a [`crate::Op::Device`] node asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceKind {
    RawMouse,
    MouseLook,
    PenPressure,
    PenTilt,
    PenEraser,
    PadGyro,
    PadAccel,
    PadNote,
    SetLight,
}

impl DeviceKind {
    pub fn eval(self, host: &mut dyn Host) -> Value {
        match self {
            Self::RawMouse => Value::Vec2(host.raw_mouse_delta()),
            Self::MouseLook => Value::Vec2(host.mouse_look()),
            Self::PenPressure => Value::Float(host.pen_pressure()),
            Self::PenTilt => Value::Vec2(host.pen_tilt()),
            Self::PenEraser => Value::Bool(host.pen_eraser()),
            Self::PadGyro => Value::Vec3(host.pad_gyro()),
            Self::PadAccel => Value::Vec3(host.pad_accel()),
            Self::PadNote => Value::Str(host.pad_motion_note()),
            Self::SetLight => Value::Float(0.0),
        }
    }

    pub fn codegen(self) -> &'static str {
        match self {
            Self::RawMouse => "host.raw_mouse_delta()",
            Self::MouseLook => "host.mouse_look()",
            Self::PenPressure => "host.pen_pressure()",
            Self::PenTilt => "host.pen_tilt()",
            Self::PenEraser => "host.pen_eraser()",
            Self::PadGyro => "host.pad_gyro()",
            Self::PadAccel => "host.pad_accel()",
            Self::PadNote => "host.pad_motion_note()",
            Self::SetLight => "0.0",
        }
    }
}
