/// Pixel size of the surface the shell owns. The host never creates a window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicalSize {
    pub width: u32,
    pub height: u32,
}

/// Input the shell forwards into a viewport. The shell does not interpret it as gameplay.
#[derive(Clone, Debug, PartialEq)]
pub enum ViewportInput {
    Pointer { x: f32, y: f32 },
    Key { text: String },
}

/// Reserved for the future renderer. The shell does not create a graphics device.
pub trait ViewportHost {
    fn native_surface(&self) -> Option<NativeSurfaceId>;

    fn attach(&mut self, _surface: NativeSurfaceId, _size: PhysicalSize, _scale: f32) {}

    fn resize(&mut self, _size: PhysicalSize, _scale: f32) {}

    fn input(&mut self, _event: ViewportInput) {}

    fn detach(&mut self) {}
}

/// Opaque id a later renderer can turn into a native window handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeSurfaceId(pub u64);

const ORBIT_PITCH_MIN_DEG: f32 = -89.0;
const ORBIT_PITCH_MAX_DEG: f32 = 89.0;
const ZOOM_DISTANCE_MIN: f32 = 0.2;
const ZOOM_DISTANCE_MAX: f32 = 500.0;
const ZOOM_DOLLY_STEP: f32 = 1.1;

/// Applies orbit deltas and clamps pitch so the camera cannot flip over the poles.
#[allow(dead_code)]
#[must_use]
pub fn orbit(yaw: f32, pitch: f32, delta_yaw: f32, delta_pitch: f32) -> (f32, f32) {
    let yaw = yaw + delta_yaw;
    let pitch = (pitch + delta_pitch).clamp(ORBIT_PITCH_MIN_DEG, ORBIT_PITCH_MAX_DEG);
    (yaw, pitch)
}

/// Dolly distance from scroll input and clamp to a usable range.
#[allow(dead_code)]
#[must_use]
pub fn zoom_distance(distance: f32, wheel: f32) -> f32 {
    (distance * ZOOM_DOLLY_STEP.powf(-wheel)).clamp(ZOOM_DISTANCE_MIN, ZOOM_DISTANCE_MAX)
}

/// Applies pan deltas in surface space.
#[cfg_attr(not(test), allow(dead_code))]
#[must_use]
pub fn pan(x: f32, y: f32, dx: f32, dy: f32) -> (f32, f32) {
    (x + dx, y + dy)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ReservedViewport;

impl ViewportHost for ReservedViewport {
    fn native_surface(&self) -> Option<NativeSurfaceId> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_viewport_has_no_surface_yet() {
        let host = ReservedViewport;
        assert!(host.native_surface().is_none());
    }

    #[test]
    fn pan_adds_the_deltas() {
        assert_eq!(pan(1.0, 2.0, 3.0, -1.5), (4.0, 0.5));
    }
}
