//! Pure geometry for the viewport Measure tool (length, snap, screen mapping).

use std::f32::consts::PI;

/// A committed or in-progress point in scene space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasurePoint(pub [f32; 3]);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmentReadout {
    pub length: f32,
    pub delta: [f32; 3],
    pub angle_deg: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PathReadout {
    pub segments: Vec<SegmentReadout>,
    pub total_length: f32,
    pub pixels: Option<f32>,
    pub perspective_note: bool,
}

/// Length, axis deltas and heading of one segment (3D euclidean length).
#[must_use]
pub fn segment_readout(from: [f32; 3], to: [f32; 3]) -> SegmentReadout {
    let delta = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let length = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
    let angle_deg = segment_angle_deg(from, to);
    SegmentReadout { length, delta, angle_deg }
}

#[must_use]
pub fn segment_angle_deg(from: [f32; 3], to: [f32; 3]) -> f32 {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    // Counter-clockwise from the +X axis, so a flat segment reads 0°.
    dy.atan2(dx).to_degrees()
}

/// When `shift` is held, keep length but snap heading to 0°, 45°, 90°, … in the view plane.
#[must_use]
pub fn constrain_plane_end(
    from: [f32; 3],
    to: [f32; 3],
    plane_first: usize,
    plane_second: usize,
    shift: bool,
) -> [f32; 3] {
    if !shift {
        return to;
    }
    let dx = to[plane_first] - from[plane_first];
    let dy = to[plane_second] - from[plane_second];
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-6 {
        return to;
    }
    let angle = dx.atan2(dy);
    let step = PI / 4.0;
    let snapped = (angle / step).round() * step;
    let mut out = to;
    out[plane_first] = from[plane_first] + len * snapped.sin();
    out[plane_second] = from[plane_second] + len * snapped.cos();
    out
}

/// Pick the candidate nearest to `point` within `threshold`, or return `point` unchanged.
#[must_use]
pub fn snap_to_nearest(point: [f32; 3], candidates: &[[f32; 3]], threshold: f32) -> [f32; 3] {
    let thr2 = threshold * threshold;
    let mut best = None;
    let mut best_d2 = thr2;
    for cand in candidates {
        let d0 = point[0] - cand[0];
        let d1 = point[1] - cand[1];
        let d2 = point[2] - cand[2];
        let dist = d0 * d0 + d1 * d1 + d2 * d2;
        if dist <= best_d2 {
            best_d2 = dist;
            best = Some(*cand);
        }
    }
    best.unwrap_or(point)
}

/// Snap to the scene grid when enabled.
#[must_use]
pub fn snap_grid(point: [f32; 3], grid_on: bool, step: f32) -> [f32; 3] {
    if !grid_on || step <= 0.0 {
        return point;
    }
    [
        (point[0] / step).round() * step,
        (point[1] / step).round() * step,
        (point[2] / step).round() * step,
    ]
}

#[must_use]
pub fn path_total_length(points: &[[f32; 3]]) -> f32 {
    points.windows(2).map(|pair| segment_readout(pair[0], pair[1]).length).sum()
}

#[must_use]
pub fn path_readout(
    points: &[[f32; 3]],
    pixels_per_unit: f32,
    flat_2d: bool,
    perspective: bool,
) -> PathReadout {
    let segments: Vec<SegmentReadout> =
        points.windows(2).map(|pair| segment_readout(pair[0], pair[1])).collect();
    let total_length = segments.iter().map(|s| s.length).sum();
    let pixels =
        if flat_2d && pixels_per_unit > 0.0 { Some(total_length * pixels_per_unit) } else { None };
    PathReadout { segments, total_length, pixels, perspective_note: perspective }
}

#[must_use]
pub fn length_pixels(length_units: f32, pixels_per_unit: f32) -> f32 {
    if pixels_per_unit > 0.0 {
        length_units * pixels_per_unit
    } else {
        0.0
    }
}

/// Ortho view: map scene XY (the plane the grid uses) to normalised screen offset from centre.
#[must_use]
pub fn ortho_scene_to_screen(x: f32, y: f32, cell: f32, yaw_deg: f32) -> (f32, f32) {
    let yaw = yaw_deg.to_radians();
    let (sin_yaw, cos_yaw) = yaw.sin_cos();
    let rx = x * cos_yaw - y * sin_yaw;
    let ry = x * sin_yaw + y * cos_yaw;
    (rx * cell, -ry * cell)
}

/// Inverse of [`ortho_scene_to_screen`] without pan/origin (relative to view centre).
#[must_use]
pub fn ortho_screen_to_scene(dx: f32, dy: f32, cell: f32, yaw_deg: f32) -> (f32, f32) {
    if cell.abs() < 1e-6 {
        return (0.0, 0.0);
    }
    let rx = dx / cell;
    let ry = -dy / cell;
    let yaw = yaw_deg.to_radians();
    let (sin_yaw, cos_yaw) = yaw.sin_cos();
    let sx = rx * cos_yaw + ry * sin_yaw;
    let sy = -rx * sin_yaw + ry * cos_yaw;
    (sx, sy)
}

/// Map a screen pick on the flat grid to scene plane coordinates (matches the viewport grid).
#[must_use]
pub fn flat_pick_to_scene(
    pos_x: f32,
    pos_y: f32,
    origin_x: f32,
    origin_y: f32,
    cell: f32,
    yaw_deg: f32,
) -> (f32, f32) {
    ortho_screen_to_scene(pos_x - origin_x, pos_y - origin_y, cell, yaw_deg)
}

/// Perspective pick on the working ground plane (z = 0), matching the editor viewport.
#[must_use]
pub fn perspective_pick_to_scene(
    pos_x: f32,
    pos_y: f32,
    origin_x: f32,
    origin_y: f32,
    cell: f32,
    yaw_deg: f32,
    pitch_deg: f32,
) -> (f32, f32) {
    let dx = (pos_x - origin_x) / cell;
    let dy = -(pos_y - origin_y) / cell;
    let yaw = yaw_deg.to_radians();
    let (sin_yaw, cos_yaw) = yaw.sin_cos();
    let (sin_pitch, cos_pitch) = pitch_deg.to_radians().sin_cos();
    if cos_yaw.abs() > 0.05 && cos_pitch.abs() > 0.05 {
        let x = dx / cos_yaw;
        let y = (dy + x * sin_yaw * sin_pitch) / cos_pitch;
        return (x, y);
    }
    let sx = dx * cos_yaw + dy * sin_yaw;
    let sy = -dx * sin_yaw + dy * cos_yaw;
    (sx, sy)
}

/// Encode points for session autosave (`x,y,z` tuples separated by `|`).
pub fn encode_path(points: &[[f32; 3]]) -> String {
    points.iter().map(|p| format!("{},{},{}", p[0], p[1], p[2])).collect::<Vec<_>>().join("|")
}

pub fn decode_path(text: &str) -> Vec<[f32; 3]> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    text.split('|')
        .filter_map(|chunk| {
            let parts: Vec<f32> = chunk.split(',').filter_map(|s| s.trim().parse().ok()).collect();
            if parts.len() == 3 {
                Some([parts[0], parts[1], parts[2]])
            } else {
                None
            }
        })
        .collect()
}

pub fn plane_axes(view: &str) -> (usize, usize, usize) {
    match view {
        "top" | "bottom" => (0, 2, 1),
        "left" | "right" => (2, 1, 0),
        _ => (0, 1, 2),
    }
}

pub fn plane_point_to_world(sx: f32, sy: f32, axes: (usize, usize, usize), depth: f32) -> [f32; 3] {
    let (first, second, spin) = axes;
    let mut p = [0.0; 3];
    p[first] = sx;
    p[second] = sy;
    p[spin] = depth;
    p
}

pub fn snap_candidates_from_entities(
    entities: &[seal_world::Entity],
    half_unit: f32,
) -> Vec<[f32; 3]> {
    let mut out = Vec::new();
    for entity in entities {
        if entity.id == "root" || entity.hidden || entity.tag == "folder" {
            continue;
        }
        let t = entity.translation;
        out.push(t);
        let s = entity.scale;
        let (hx, hy, hz) = if entity.shape.is_some() {
            (0.5 * s[0].abs(), 0.5 * s[1].abs(), 0.5 * s[2].abs())
        } else {
            (half_unit, half_unit, 0.0)
        };
        for dx in [-1.0, 1.0] {
            for dy in [-1.0, 1.0] {
                for dz in [-1.0, 1.0] {
                    if dx == 0.0 && dy == 0.0 && dz == 0.0 {
                        continue;
                    }
                    out.push([t[0] + dx * hx, t[1] + dy * hy, t[2] + dz * hz]);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_delta_angle_for_axis_segment() {
        let read = segment_readout([0.0, 0.0, 0.0], [3.0, 4.0, 0.0]);
        assert!((read.length - 5.0).abs() < 1e-5);
        assert_eq!(read.delta, [3.0, 4.0, 0.0]);
        assert!((read.angle_deg - 53.130_1).abs() < 0.01);
        let flat = segment_readout([0.0, 0.0, 0.0], [20.0, 3.5, 0.0]);
        assert!((flat.angle_deg - 9.926).abs() < 0.01);
    }

    #[test]
    fn shift_constrains_to_forty_five_steps() {
        let end = constrain_plane_end([0.0, 0.0, 0.0], [10.0, 1.0, 0.0], 0, 1, true);
        assert!((end[0] - 10.0).abs() < 0.05);
        assert!(end[1].abs() < 0.05);
    }

    #[test]
    fn snap_picks_nearest_inside_threshold() {
        let p = snap_to_nearest([1.0, 1.0, 0.0], &[[0.0, 0.0, 0.0], [5.0, 5.0, 0.0]], 1.5);
        assert_eq!(p, [0.0, 0.0, 0.0]);
        let miss = snap_to_nearest([9.0, 9.0, 0.0], &[[0.0, 0.0, 0.0]], 1.0);
        assert_eq!(miss, [9.0, 9.0, 0.0]);
    }

    #[test]
    fn multi_segment_total_sums_lengths() {
        let pts = [[0.0, 0.0, 0.0], [3.0, 0.0, 0.0], [3.0, 4.0, 0.0]];
        assert!((path_total_length(&pts) - 7.0).abs() < 1e-5);
    }

    #[test]
    fn pixel_conversion_uses_ppu() {
        assert!((length_pixels(2.5, 16.0) - 40.0).abs() < 1e-5);
        assert_eq!(length_pixels(2.5, 0.0), 0.0);
    }

    #[test]
    fn ortho_screen_round_trip() {
        let cell = 20.0;
        let yaw = 15.0;
        for (x, y) in [(0.0, 0.0), (3.0, -2.0), (-1.5, 4.0)] {
            let (sx, sy) = ortho_scene_to_screen(x, y, cell, yaw);
            let (back_x, back_y) = ortho_screen_to_scene(sx, sy, cell, yaw);
            assert!((back_x - x).abs() < 1e-4, "x {x}");
            assert!((back_y - y).abs() < 1e-4, "y {y}");
        }
    }

    #[test]
    fn decode_rejects_damaged_tuples() {
        let pts = decode_path("1,2,3|bad|4,5");
        assert_eq!(pts.len(), 1);
        assert_eq!(pts[0], [1.0, 2.0, 3.0]);
    }
}
