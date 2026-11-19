//! Drag margins for the 2D camera: a dead zone, as a fraction of the view,
//! inside which the followed target moves without the camera moving.

use seal_scene::Camera2d;

use crate::Entity;

/// Inspector keys, in the same order as [`Camera2d::drag`]: left, right, top, bottom.
pub const KEYS: [&str; 4] = ["cam_drag_left", "cam_drag_right", "cam_drag_top", "cam_drag_bottom"];

/// Apply one inspector value. A value that is not a number is left as it was.
/// Returns whether `key` is a drag margin.
pub fn apply(entity: &mut Entity, key: &str, value: &str) -> bool {
    let Some(index) = KEYS.iter().position(|k| *k == key) else { return false };
    if let Some(v) = value.trim().parse::<f32>().ok().filter(|v| v.is_finite()) {
        entity.camera2d.drag[index] = v.clamp(0.0, 1.0);
    }
    true
}

/// Copy one drag margin. Returns whether `key` is one.
pub fn copy_field(from: &Entity, to: &mut Entity, key: &str) -> bool {
    let Some(index) = KEYS.iter().position(|k| *k == key) else { return false };
    to.camera2d.drag[index] = from.camera2d.drag[index];
    true
}

/// Text for a scene file. `None` when every margin is 0, so older cameras
/// keep the text they had.
pub fn save_text(drag: [f32; 4]) -> Option<String> {
    if drag == [0.0; 4] {
        None
    } else {
        Some(drag.iter().map(|v| num(*v)).collect::<Vec<_>>().join(", "))
    }
}

/// Read a saved margin list. `None` when the text is damaged.
pub fn parse(text: &str) -> Option<[f32; 4]> {
    let parts: Vec<&str> = text.split(',').map(str::trim).filter(|p| !p.is_empty()).collect();
    if parts.len() != 4 {
        return None;
    }
    let mut drag = [0.0; 4];
    for (slot, part) in drag.iter_mut().zip(parts) {
        let v = part.parse::<f32>().ok().filter(|v| v.is_finite())?;
        *slot = v.clamp(0.0, 1.0);
    }
    Some(drag)
}

/// Where the camera centre should sit so `target` stays inside the dead zone.
///
/// Margins are left, right, top, bottom as a fraction of the full view.
/// All zero follows `target` exactly, whatever the view size is.
pub fn desired_center(
    center: [f32; 2],
    target: [f32; 2],
    drag: [f32; 4],
    view_half: [f32; 2],
) -> [f32; 2] {
    if drag.iter().all(|v| *v == 0.0) {
        return target;
    }
    let width = (view_half[0] * 2.0).max(0.0);
    let height = (view_half[1] * 2.0).max(0.0);
    let left = finite(drag[0]) * width;
    let right = finite(drag[1]) * width;
    let top = finite(drag[2]) * height;
    let bottom = finite(drag[3]) * height;
    let mut out = center;
    let min_x = center[0] - left;
    let max_x = center[0] + right;
    let min_y = center[1] - bottom;
    let max_y = center[1] + top;
    if target[0] < min_x {
        out[0] += target[0] - min_x;
    } else if target[0] > max_x {
        out[0] += target[0] - max_x;
    }
    if target[1] < min_y {
        out[1] += target[1] - min_y;
    } else if target[1] > max_y {
        out[1] += target[1] - max_y;
    }
    out
}

/// Corners of the dead zone around `center` (x right, y up), or nothing when
/// the zone has no area.
pub fn zone_rect(center: [f32; 2], drag: [f32; 4], view_half: [f32; 2]) -> Option<[[f32; 2]; 4]> {
    if drag.iter().all(|v| *v <= 0.0) {
        return None;
    }
    let width = (view_half[0] * 2.0).max(0.0);
    let height = (view_half[1] * 2.0).max(0.0);
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let left = finite(drag[0]) * width;
    let right = finite(drag[1]) * width;
    let top = finite(drag[2]) * height;
    let bottom = finite(drag[3]) * height;
    if left == 0.0 && right == 0.0 && top == 0.0 && bottom == 0.0 {
        return None;
    }
    let (x, y) = (center[0], center[1]);
    Some([
        [x - left, y - bottom],
        [x + right, y - bottom],
        [x + right, y + top],
        [x - left, y + top],
    ])
}

fn finite(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn num(v: f32) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" {
        "0".into()
    } else {
        s.into()
    }
}

/// Read a drag field off a saved camera, defaulting when the field is absent
/// and zeroing it when the text is damaged.
pub fn from_saved(text: Option<&str>) -> [f32; 4] {
    text.and_then(parse).unwrap_or([0.0; 4])
}

pub fn rows(cam: &Camera2d) -> Vec<(String, String)> {
    KEYS.iter().enumerate().map(|(i, key)| ((*key).to_string(), num(cam.drag[i]))).collect()
}
