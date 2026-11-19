use egui::{Align2, Color32, Pos2, Stroke, StrokeKind, TextStyle, Vec2};

use crate::look::{AXIS_X, AXIS_Y, AXIS_Z};
use crate::viewport_mesh::project_point_angled;

const GIZMO_LEN: f32 = 72.0;
const HIT_SLACK: f32 = 9.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveHandle {
    AxisX,
    AxisY,
    AxisZ,
    PlaneXy,
    PlaneYz,
    PlaneXz,
    Center,
}

pub fn parse_move_handle(value: &str) -> Option<MoveHandle> {
    match value.trim().to_ascii_lowercase().as_str() {
        "x" | "axis_x" => Some(MoveHandle::AxisX),
        "y" | "axis_y" => Some(MoveHandle::AxisY),
        "z" | "axis_z" => Some(MoveHandle::AxisZ),
        "xy" | "plane_xy" => Some(MoveHandle::PlaneXy),
        "yz" | "plane_yz" => Some(MoveHandle::PlaneYz),
        "xz" | "plane_xz" => Some(MoveHandle::PlaneXz),
        "center" => Some(MoveHandle::Center),
        _ => None,
    }
}

pub struct MoveGizmoDirs {
    pub x: Vec2,
    pub y: Vec2,
    pub z: Vec2,
}

fn rotate_euler_xyz(v: [f32; 3], euler: [f32; 3]) -> [f32; 3] {
    let (sx, cx) = euler[0].sin_cos();
    let (sy, cy) = euler[1].sin_cos();
    let (sz, cz) = euler[2].sin_cos();
    let y1 = v[1] * cx - v[2] * sx;
    let z1 = v[1] * sx + v[2] * cx;
    let x2 = v[0] * cy + z1 * sy;
    let z2 = -v[0] * sy + z1 * cy;
    [x2 * cz - y1 * sz, x2 * sz + y1 * cz, z2]
}

fn local_axis(axis: usize, euler: [f32; 3]) -> [f32; 3] {
    let base = match axis {
        0 => [1.0, 0.0, 0.0],
        1 => [0.0, 1.0, 0.0],
        _ => [0.0, 0.0, 1.0],
    };
    rotate_euler_xyz(base, euler)
}

pub fn move_gizmo_dirs(
    yaw_deg: f32,
    pitch_deg: f32,
    origin: Pos2,
    cell: f32,
    space: &str,
    local_rot: Option<[f32; 3]>,
) -> MoveGizmoDirs {
    let unit = |axis: usize| -> Vec2 {
        let world = if space == "local" {
            local_rot.map(|rot| local_axis(axis, rot)).unwrap_or_else(|| match axis {
                0 => [1.0, 0.0, 0.0],
                1 => [0.0, 1.0, 0.0],
                _ => [0.0, 0.0, 1.0],
            })
        } else {
            match axis {
                0 => [1.0, 0.0, 0.0],
                1 => [0.0, 1.0, 0.0],
                _ => [0.0, 0.0, 1.0],
            }
        };
        let (tip, _) = project_point_angled(world, yaw_deg, pitch_deg, origin, cell);
        let (base, _) = project_point_angled([0.0, 0.0, 0.0], yaw_deg, pitch_deg, origin, cell);
        let v = tip - base;
        if v.length_sq() < f32::EPSILON {
            Vec2::ZERO
        } else {
            v.normalized()
        }
    };
    MoveGizmoDirs { x: unit(0), y: unit(1), z: unit(2) }
}

fn dist_point_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_sq();
    if len_sq < f32::EPSILON {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

pub fn hit_move_gizmo(pointer: Pos2, pivot: Pos2, dirs: &MoveGizmoDirs) -> Option<MoveHandle> {
    let tip_x = pivot + dirs.x * GIZMO_LEN;
    let tip_y = pivot + dirs.y * GIZMO_LEN;
    let tip_z = pivot + dirs.z * GIZMO_LEN;
    if (pointer - pivot).length() <= HIT_SLACK {
        return Some(MoveHandle::Center);
    }
    let plane_center = |a: Vec2, b: Vec2| {
        let sum = a + b;
        if sum.length_sq() < f32::EPSILON {
            pivot
        } else {
            pivot + sum.normalized() * (GIZMO_LEN * 0.42)
        }
    };
    let plane_xy = plane_center(dirs.x, dirs.y);
    let plane_yz = plane_center(dirs.y, dirs.z);
    let plane_xz = plane_center(dirs.x, dirs.z);
    for (handle, center) in [
        (MoveHandle::PlaneXy, plane_xy),
        (MoveHandle::PlaneYz, plane_yz),
        (MoveHandle::PlaneXz, plane_xz),
    ] {
        if (pointer - center).length() <= HIT_SLACK + 2.0 && (center - pivot).length_sq() > 1.0 {
            return Some(handle);
        }
    }
    let mut best = None;
    let mut best_dist = HIT_SLACK;
    for (handle, tip) in
        [(MoveHandle::AxisX, tip_x), (MoveHandle::AxisY, tip_y), (MoveHandle::AxisZ, tip_z)]
    {
        let d = dist_point_segment(pointer, pivot, tip);
        if d < best_dist {
            best_dist = d;
            best = Some(handle);
        }
    }
    best
}

fn ink(handle: MoveHandle, hover: Option<MoveHandle>) -> Color32 {
    let yellow = Color32::from_rgb(255, 220, 48);
    if hover == Some(handle) {
        return yellow;
    }
    match handle {
        MoveHandle::AxisX => AXIS_X,
        MoveHandle::AxisY => AXIS_Y,
        MoveHandle::AxisZ => AXIS_Z,
        MoveHandle::Center => Color32::from_gray(210),
        MoveHandle::PlaneXy | MoveHandle::PlaneYz | MoveHandle::PlaneXz => Color32::from_gray(170),
    }
}

pub fn paint_move_gizmo(
    painter: &egui::Painter,
    pivot: Pos2,
    dirs: &MoveGizmoDirs,
    hover: Option<MoveHandle>,
    text: &str,
    style: &egui::Style,
) {
    let small = TextStyle::Small.resolve(style);
    let stroke =
        |handle| Stroke::new(if hover == Some(handle) { 3.0 } else { 2.0 }, ink(handle, hover));
    painter.arrow(pivot, dirs.x * GIZMO_LEN, stroke(MoveHandle::AxisX));
    painter.arrow(pivot, dirs.y * GIZMO_LEN, stroke(MoveHandle::AxisY));
    if dirs.z.length_sq() > 0.01 {
        painter.arrow(pivot, dirs.z * GIZMO_LEN, stroke(MoveHandle::AxisZ));
    }
    let plane = |handle, normal_sum: Vec2| {
        if normal_sum.length_sq() < f32::EPSILON {
            return;
        }
        let center = pivot + normal_sum.normalized() * (GIZMO_LEN * 0.42);
        let half = 9.0;
        let dir = normal_sum.normalized();
        let ortho = Vec2::new(-dir.y, dir.x);
        let poly = [
            center + dir * half + ortho * half,
            center + dir * half - ortho * half,
            center - dir * half - ortho * half,
            center - dir * half + ortho * half,
        ];
        painter.add(egui::Shape::convex_polygon(
            poly.to_vec(),
            ink(handle, hover).gamma_multiply(0.55),
            Stroke::new(1.0, ink(handle, hover)),
        ));
    };
    plane(MoveHandle::PlaneXy, dirs.x + dirs.y);
    if dirs.z.length_sq() > 0.01 {
        plane(MoveHandle::PlaneYz, dirs.y + dirs.z);
        plane(MoveHandle::PlaneXz, dirs.x + dirs.z);
    }
    painter.circle_filled(pivot, 6.0, ink(MoveHandle::Center, hover));
    painter.circle_stroke(pivot, 6.0, Stroke::new(1.5, ink(MoveHandle::Center, hover)));
    if !text.is_empty() {
        painter.text(
            pivot + Vec2::new(80.0, -18.0),
            Align2::LEFT_CENTER,
            text,
            small,
            Color32::WHITE,
        );
    }
}

pub fn scene_delta_from_screen(
    delta: Vec2,
    handle: MoveHandle,
    yaw_deg: f32,
    cell: f32,
) -> [f32; 3] {
    let yaw = yaw_deg.to_radians();
    let (sin_y, cos_y) = yaw.sin_cos();
    let dx = delta.x / cell;
    let dy = -delta.y / cell;
    let sx = dx * cos_y + dy * sin_y;
    let sz = -dx * sin_y + dy * cos_y;
    let mut out = [0.0, 0.0, 0.0];
    match handle {
        MoveHandle::AxisX => out[0] = sx,
        MoveHandle::AxisY => out[1] = dy,
        MoveHandle::AxisZ => out[2] = sz,
        MoveHandle::PlaneXy => {
            out[0] = sx;
            out[1] = dy;
        }
        MoveHandle::PlaneXz => {
            out[0] = sx;
            out[2] = sz;
        }
        MoveHandle::PlaneYz => {
            out[1] = dy;
            out[2] = sz;
        }
        MoveHandle::Center => {
            out[0] = sx;
            out[1] = dy;
        }
    }
    out
}

pub const ROTATE_AXIS_RING_R: f32 = 86.0;
pub const ROTATE_SCREEN_R: f32 = 96.0;
const ROTATE_RING_SEGS: usize = 56;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RotateHandle {
    AxisX,
    AxisY,
    AxisZ,
    Screen,
}

pub fn parse_rotate_handle(value: &str) -> Option<RotateHandle> {
    match value.trim().to_ascii_lowercase().as_str() {
        "x" | "axis_x" => Some(RotateHandle::AxisX),
        "y" | "axis_y" => Some(RotateHandle::AxisY),
        "z" | "axis_z" => Some(RotateHandle::AxisZ),
        "screen" | "view" => Some(RotateHandle::Screen),
        _ => None,
    }
}

pub fn rotate_axis_index(handle: RotateHandle) -> usize {
    match handle {
        RotateHandle::AxisX => 0,
        RotateHandle::AxisY => 1,
        RotateHandle::AxisZ => 2,
        RotateHandle::Screen => 1,
    }
}

fn ring_ink(handle: RotateHandle, hover: Option<RotateHandle>) -> Color32 {
    let yellow = Color32::from_rgb(255, 220, 48);
    if hover == Some(handle) {
        return yellow;
    }
    match handle {
        RotateHandle::AxisX => AXIS_X,
        RotateHandle::AxisY => AXIS_Y,
        RotateHandle::AxisZ => AXIS_Z,
        RotateHandle::Screen => Color32::from_gray(200),
    }
}

fn screen_axis_ring(pivot: Pos2, a: Vec2, b: Vec2, radius: f32) -> Vec<Pos2> {
    if a.length_sq() < f32::EPSILON || b.length_sq() < f32::EPSILON {
        return Vec::new();
    }
    let au = a.normalized();
    let bu = b.normalized();
    (0..=ROTATE_RING_SEGS)
        .map(|i| {
            let t = i as f32 / ROTATE_RING_SEGS as f32 * std::f32::consts::TAU;
            pivot + (au * t.cos() + bu * t.sin()) * radius
        })
        .collect()
}

fn screen_ring_poly(pivot: Pos2) -> Vec<Pos2> {
    (0..=ROTATE_RING_SEGS)
        .map(|i| {
            let t = i as f32 / ROTATE_RING_SEGS as f32 * std::f32::consts::TAU;
            pivot + Vec2::angled(t) * ROTATE_SCREEN_R
        })
        .collect()
}

fn dist_to_polyline(pointer: Pos2, pts: &[Pos2]) -> f32 {
    if pts.len() < 2 {
        return f32::MAX;
    }
    let mut best = f32::MAX;
    for w in pts.windows(2) {
        best = best.min(dist_point_segment(pointer, w[0], w[1]));
    }
    best
}

pub fn hit_rotate_gizmo(pointer: Pos2, pivot: Pos2, dirs: &MoveGizmoDirs) -> Option<RotateHandle> {
    let mut best = None;
    let mut best_dist = HIT_SLACK + 6.0;
    for (handle, a, b) in [
        (RotateHandle::AxisX, dirs.y, dirs.z),
        (RotateHandle::AxisY, dirs.z, dirs.x),
        (RotateHandle::AxisZ, dirs.x, dirs.y),
    ] {
        let poly = screen_axis_ring(pivot, a, b, ROTATE_AXIS_RING_R);
        let d = dist_to_polyline(pointer, &poly);
        if d < best_dist {
            best_dist = d;
            best = Some(handle);
        }
    }
    let screen = screen_ring_poly(pivot);
    let d = dist_to_polyline(pointer, &screen);
    if d < best_dist {
        best = Some(RotateHandle::Screen);
    }
    best
}

fn wrap_angle(angle: f32) -> f32 {
    let mut a = angle;
    while a > std::f32::consts::PI {
        a -= std::f32::consts::TAU;
    }
    while a < -std::f32::consts::PI {
        a += std::f32::consts::TAU;
    }
    a
}

pub fn pointer_angle(pivot: Pos2, at: Pos2) -> f32 {
    (at - pivot).angle()
}

pub fn rotate_drag_delta(handle: RotateHandle, pivot: Pos2, start: Pos2, current: Pos2) -> f32 {
    let _ = handle;
    wrap_angle(pointer_angle(pivot, current) - pointer_angle(pivot, start))
}

pub struct RotateArcVisual {
    pub handle: RotateHandle,
    pub start: f32,
    pub end: f32,
    pub radius: f32,
}

fn paint_arc(
    painter: &egui::Painter,
    pivot: Pos2,
    radius: f32,
    start: f32,
    end: f32,
    color: Color32,
) {
    if (end - start).abs() < 0.02 {
        return;
    }
    let steps = 24;
    let mut pts = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = start + (end - start) * i as f32 / steps as f32;
        pts.push(pivot + Vec2::angled(t) * radius);
    }
    painter.add(egui::Shape::line(pts, Stroke::new(4.0, color)));
}

pub fn paint_rotate_gizmo(
    painter: &egui::Painter,
    pivot: Pos2,
    dirs: &MoveGizmoDirs,
    hover: Option<RotateHandle>,
    arc: Option<RotateArcVisual>,
    angle_label: &str,
    style: &egui::Style,
) {
    let small = TextStyle::Small.resolve(style);
    for (handle, a, b) in [
        (RotateHandle::AxisX, dirs.y, dirs.z),
        (RotateHandle::AxisY, dirs.z, dirs.x),
        (RotateHandle::AxisZ, dirs.x, dirs.y),
    ] {
        let poly = screen_axis_ring(pivot, a, b, ROTATE_AXIS_RING_R);
        if poly.is_empty() {
            continue;
        }
        let stroke =
            Stroke::new(if hover == Some(handle) { 3.0 } else { 1.75 }, ring_ink(handle, hover));
        painter.add(egui::Shape::closed_line(poly, stroke));
    }
    let screen = screen_ring_poly(pivot);
    painter.add(egui::Shape::closed_line(
        screen,
        Stroke::new(
            if hover == Some(RotateHandle::Screen) { 2.5 } else { 1.5 },
            ring_ink(RotateHandle::Screen, hover),
        ),
    ));
    if let Some(vis) = arc {
        let color = ring_ink(vis.handle, hover);
        paint_arc(painter, pivot, vis.radius, vis.start, vis.end, color);
    }
    if !angle_label.is_empty() {
        painter.text(
            pivot + Vec2::new(70.0, -24.0),
            Align2::LEFT_CENTER,
            angle_label,
            small,
            Color32::WHITE,
        );
    }
}

const SCALE_GIZMO_LEN: f32 = 68.0;
const SCALE_BOX_HALF: f32 = 7.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScaleHandle {
    AxisX,
    AxisY,
    AxisZ,
    PlaneXy,
    PlaneYz,
    PlaneXz,
    Center,
}

pub fn parse_scale_handle(value: &str) -> Option<ScaleHandle> {
    match value.trim().to_ascii_lowercase().as_str() {
        "x" | "axis_x" => Some(ScaleHandle::AxisX),
        "y" | "axis_y" => Some(ScaleHandle::AxisY),
        "z" | "axis_z" => Some(ScaleHandle::AxisZ),
        "xy" | "plane_xy" => Some(ScaleHandle::PlaneXy),
        "yz" | "plane_yz" => Some(ScaleHandle::PlaneYz),
        "xz" | "plane_xz" => Some(ScaleHandle::PlaneXz),
        "center" | "uniform" => Some(ScaleHandle::Center),
        _ => None,
    }
}

fn scale_ink(handle: ScaleHandle, hover: Option<ScaleHandle>) -> Color32 {
    let yellow = Color32::from_rgb(255, 220, 48);
    if hover == Some(handle) {
        return yellow;
    }
    match handle {
        ScaleHandle::AxisX => AXIS_X,
        ScaleHandle::AxisY => AXIS_Y,
        ScaleHandle::AxisZ => AXIS_Z,
        ScaleHandle::Center => Color32::from_gray(210),
        ScaleHandle::PlaneXy | ScaleHandle::PlaneYz | ScaleHandle::PlaneXz => {
            Color32::from_gray(170)
        }
    }
}

fn scale_to_move(handle: ScaleHandle) -> MoveHandle {
    match handle {
        ScaleHandle::AxisX => MoveHandle::AxisX,
        ScaleHandle::AxisY => MoveHandle::AxisY,
        ScaleHandle::AxisZ => MoveHandle::AxisZ,
        ScaleHandle::PlaneXy => MoveHandle::PlaneXy,
        ScaleHandle::PlaneYz => MoveHandle::PlaneYz,
        ScaleHandle::PlaneXz => MoveHandle::PlaneXz,
        ScaleHandle::Center => MoveHandle::Center,
    }
}

fn point_in_tri(p: Pos2, a: Pos2, b: Pos2, c: Pos2) -> bool {
    let sign = |p1: Pos2, p2: Pos2, p3: Pos2| {
        (p1.x - p3.x) * (p2.y - p3.y) - (p2.x - p3.x) * (p1.y - p3.y)
    };
    let d1 = sign(p, a, b);
    let d2 = sign(p, b, c);
    let d3 = sign(p, c, a);
    let has_neg = (d1 < 0.0) || (d2 < 0.0) || (d3 < 0.0);
    let has_pos = (d1 > 0.0) || (d2 > 0.0) || (d3 > 0.0);
    !(has_neg && has_pos)
}

pub fn hit_scale_gizmo(pointer: Pos2, pivot: Pos2, dirs: &MoveGizmoDirs) -> Option<ScaleHandle> {
    if (pointer - pivot).length() <= HIT_SLACK + SCALE_BOX_HALF {
        return Some(ScaleHandle::Center);
    }
    let plane = |handle, a: Vec2, b: Vec2| -> Option<ScaleHandle> {
        let sum = a + b;
        if sum.length_sq() < f32::EPSILON {
            return None;
        }
        let tip = pivot + sum.normalized() * (SCALE_GIZMO_LEN * 0.42);
        let ortho = Vec2::new(-sum.y, sum.x).normalized() * (SCALE_BOX_HALF + 3.0);
        let along = sum.normalized() * (SCALE_BOX_HALF + 3.0);
        let tri = [tip + along, tip - along * 0.4 + ortho, tip - along * 0.4 - ortho];
        if point_in_tri(pointer, tri[0], tri[1], tri[2]) {
            Some(handle)
        } else {
            None
        }
    };
    for (handle, pair) in [
        (ScaleHandle::PlaneXy, (dirs.x, dirs.y)),
        (ScaleHandle::PlaneYz, (dirs.y, dirs.z)),
        (ScaleHandle::PlaneXz, (dirs.x, dirs.z)),
    ] {
        if pair.0.length_sq() > 0.01 && pair.1.length_sq() > 0.01 {
            if let Some(hit) = plane(handle, pair.0, pair.1) {
                return Some(hit);
            }
        }
    }
    let mut best = None;
    let mut best_dist = HIT_SLACK + 4.0;
    for (handle, dir) in
        [(ScaleHandle::AxisX, dirs.x), (ScaleHandle::AxisY, dirs.y), (ScaleHandle::AxisZ, dirs.z)]
    {
        if dir.length_sq() < 0.01 {
            continue;
        }
        let tip = pivot + dir.normalized() * SCALE_GIZMO_LEN;
        let d = (pointer - tip).length();
        if d < best_dist {
            best_dist = d;
            best = Some(handle);
        }
    }
    best
}

pub fn scale_delta_from_screen(
    delta: Vec2,
    handle: ScaleHandle,
    yaw_deg: f32,
    cell: f32,
) -> [f32; 3] {
    let raw = scene_delta_from_screen(delta, scale_to_move(handle), yaw_deg, cell);
    [raw[0] * 0.08, raw[1] * 0.08, raw[2] * 0.08]
}

pub fn apply_scale_drag(start: [f32; 3], delta: [f32; 3], handle: ScaleHandle) -> [f32; 3] {
    let mut out = start;
    match handle {
        ScaleHandle::AxisX => out[0] = (start[0] + delta[0]).max(0.05),
        ScaleHandle::AxisY => out[1] = (start[1] + delta[1]).max(0.05),
        ScaleHandle::AxisZ => out[2] = (start[2] + delta[2]).max(0.05),
        ScaleHandle::PlaneXy => {
            out[0] = (start[0] + delta[0]).max(0.05);
            out[1] = (start[1] + delta[1]).max(0.05);
        }
        ScaleHandle::PlaneXz => {
            out[0] = (start[0] + delta[0]).max(0.05);
            out[2] = (start[2] + delta[2]).max(0.05);
        }
        ScaleHandle::PlaneYz => {
            out[1] = (start[1] + delta[1]).max(0.05);
            out[2] = (start[2] + delta[2]).max(0.05);
        }
        ScaleHandle::Center => {
            let drive = (delta[0] + delta[1] - delta[2]) * 0.33;
            let factor = (1.0 + drive).max(0.1);
            out = [start[0] * factor, start[1] * factor, start[2] * factor];
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub fn paint_scale_gizmo(
    painter: &egui::Painter,
    pivot: Pos2,
    dirs: &MoveGizmoDirs,
    hover: Option<ScaleHandle>,
    text: &str,
    style: &egui::Style,
) {
    let small = TextStyle::Small.resolve(style);
    let stroke = |handle| {
        Stroke::new(if hover == Some(handle) { 3.0 } else { 2.0 }, scale_ink(handle, hover))
    };
    for (handle, dir) in
        [(ScaleHandle::AxisX, dirs.x), (ScaleHandle::AxisY, dirs.y), (ScaleHandle::AxisZ, dirs.z)]
    {
        if dir.length_sq() < 0.01 {
            continue;
        }
        let tip = pivot + dir.normalized() * SCALE_GIZMO_LEN;
        painter.line_segment(
            [pivot, tip],
            Stroke::new(1.25, scale_ink(handle, hover).gamma_multiply(0.65)),
        );
        painter.rect_filled(
            egui::Rect::from_center_size(
                tip,
                egui::vec2(SCALE_BOX_HALF * 2.0, SCALE_BOX_HALF * 2.0),
            ),
            1.0,
            scale_ink(handle, hover),
        );
        painter.rect_stroke(
            egui::Rect::from_center_size(
                tip,
                egui::vec2(SCALE_BOX_HALF * 2.0, SCALE_BOX_HALF * 2.0),
            ),
            1.0,
            stroke(handle),
            StrokeKind::Outside,
        );
    }
    let triangle = |handle, a: Vec2, b: Vec2| {
        let sum = a + b;
        if sum.length_sq() < f32::EPSILON {
            return;
        }
        let tip = pivot + sum.normalized() * (SCALE_GIZMO_LEN * 0.42);
        let ortho = Vec2::new(-sum.y, sum.x).normalized() * (SCALE_BOX_HALF + 3.0);
        let along = sum.normalized() * (SCALE_BOX_HALF + 3.0);
        let tri = [tip + along, tip - along * 0.4 + ortho, tip - along * 0.4 - ortho];
        painter.add(egui::Shape::convex_polygon(
            tri.to_vec(),
            scale_ink(handle, hover).gamma_multiply(0.55),
            stroke(handle),
        ));
    };
    triangle(ScaleHandle::PlaneXy, dirs.x, dirs.y);
    if dirs.z.length_sq() > 0.01 {
        triangle(ScaleHandle::PlaneYz, dirs.y, dirs.z);
        triangle(ScaleHandle::PlaneXz, dirs.x, dirs.z);
    }
    let center =
        egui::Rect::from_center_size(pivot, egui::vec2(SCALE_BOX_HALF * 2.2, SCALE_BOX_HALF * 2.2));
    painter.rect_filled(center, 1.0, scale_ink(ScaleHandle::Center, hover));
    painter.rect_stroke(center, 1.0, stroke(ScaleHandle::Center), StrokeKind::Outside);
    if !text.is_empty() {
        painter.text(
            pivot + Vec2::new(76.0, -16.0),
            Align2::LEFT_CENTER,
            text,
            small,
            Color32::WHITE,
        );
    }
}

/// Bright guide for Ctrl+drag along one world axis without the gizmo.
pub fn paint_ctrl_axis_guide(
    painter: &egui::Painter,
    pivot: Pos2,
    dirs: &MoveGizmoDirs,
    axis: &str,
) {
    let vec = match axis.trim().to_ascii_lowercase().as_str() {
        "x" => dirs.x,
        "y" => dirs.y,
        "z" => dirs.z,
        _ => return,
    };
    if vec.length_sq() < f32::EPSILON {
        return;
    }
    let tip = pivot + vec.normalized() * 140.0;
    painter.line_segment([pivot, tip], Stroke::new(3.0, Color32::from_rgb(0xff, 0x44, 0xcc)));
    painter.circle_stroke(pivot, 6.0, Stroke::new(2.0, Color32::from_rgb(0xff, 0x44, 0xcc)));
}

/// Vertex snap markers near the pivot.
pub fn paint_vertex_snap_guides(painter: &egui::Painter, pivot: Pos2) {
    let ink = Color32::from_rgb(0x66, 0xee, 0xff);
    for dx in [-1, 0, 1] {
        for dy in [-1, 0, 1] {
            let off = Vec2::new(f32::from(dx as i16) * 22.0, f32::from(dy as i16) * 22.0);
            let c = pivot + off;
            painter.circle_filled(c, 3.5, ink.gamma_multiply(0.85));
        }
    }
    painter.text(
        pivot + Vec2::new(-40.0, -34.0),
        Align2::RIGHT_BOTTOM,
        "V",
        TextStyle::Small.resolve(&egui::Style::default()),
        ink,
    );
}

/// Temporary pivot offset crosshair.
pub fn paint_temp_pivot(painter: &egui::Painter, pivot: Pos2) {
    let ink = Color32::from_rgb(0x55, 0xdd, 0xff);
    let arm = 14.0;
    painter.line_segment(
        [pivot + Vec2::new(-arm, 0.0), pivot + Vec2::new(arm, 0.0)],
        Stroke::new(2.0, ink),
    );
    painter.line_segment(
        [pivot + Vec2::new(0.0, -arm), pivot + Vec2::new(0.0, arm)],
        Stroke::new(2.0, ink),
    );
    painter.circle_stroke(pivot, 18.0, Stroke::new(1.5, ink.gamma_multiply(0.7)));
}
