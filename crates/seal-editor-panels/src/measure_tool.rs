//! Viewport Measure tool: click-drag rulers, multi-segment paths, status readout.

use egui::{Color32, Stroke, TextStyle};
use seal_editor_core::{measure_math, CommandArgs, EditorSession, PlayMode};
use seal_ui::{MeasurePointer, PanelFrame, ViewportCamera};

pub const MEASURE_PATH: &str = "measure-path";
pub const MEASURE_DRAG: &str = "measure-drag";
pub const MEASURE_STATUS: &str = "measure-status";
pub const MEASURE_ANCHOR: &str = "measure-anchor";

use crate::PanelEvent;

fn draft_pair(key: &str, value: &str) -> (String, CommandArgs) {
    ("draft.set".into(), CommandArgs::new(&[("key", key), ("value", value)]))
}

fn draft_cmd(key: &str, value: &str) -> PanelEvent {
    let (id, args) = draft_pair(key, value);
    PanelEvent::Command(id, args)
}

#[derive(Clone, Copy)]
pub struct MeasureViewAxes {
    pub across: usize,
    pub up: usize,
    pub _spin: usize,
    pub flip_across: f32,
    pub flip_up: f32,
}

pub fn handle_viewport(
    session: &EditorSession,
    pointer: Option<&MeasurePointer>,
    escape: bool,
    flat_2d: bool,
    perspective: bool,
    pixels_per_unit: f32,
    camera: &ViewportCamera,
    view: &str,
) -> PanelEvent {
    if session.models.viewport_tool != "measure" {
        return PanelEvent::None;
    }
    if session.world.mode != PlayMode::Editing || session.game_view {
        return PanelEvent::None;
    }
    if escape {
        return PanelEvent::Commands(vec![
            draft_pair(MEASURE_PATH, ""),
            draft_pair(MEASURE_DRAG, ""),
            draft_pair(MEASURE_STATUS, ""),
            draft_pair(MEASURE_ANCHOR, ""),
        ]);
    }
    let Some(p) = pointer else {
        return PanelEvent::None;
    };
    let axes = measure_math::plane_axes(view);
    let path = measure_math::decode_path(&session.draft(MEASURE_PATH).unwrap_or_default());
    let Some(mut world) = pick_world(p, session, perspective, camera, &path, axes) else {
        return PanelEvent::None;
    };
    let mut cmds = Vec::new();
    if p.primary_pressed {
        let anchor = path.last().copied().or_else(|| {
            session
                .draft(MEASURE_ANCHOR)
                .and_then(|t| measure_math::decode_path(&t).first().copied())
        });
        if let Some(start) = anchor {
            world = measure_math::constrain_plane_end(start, world, axes.0, axes.1, p.shift);
        }
        let encoded = measure_math::encode_path(&[world]);
        cmds.push(draft_cmd(MEASURE_ANCHOR, &encoded));
        cmds.push(draft_cmd(MEASURE_DRAG, &encoded));
    }
    if p.primary_down {
        let anchor = path.last().copied().or_else(|| {
            session
                .draft(MEASURE_ANCHOR)
                .and_then(|t| measure_math::decode_path(&t).first().copied())
        });
        if let Some(start) = anchor {
            let end = measure_math::constrain_plane_end(start, world, axes.0, axes.1, p.shift);
            cmds.push(draft_cmd(
                MEASURE_STATUS,
                &format_status(session, &[start, end], flat_2d, perspective, pixels_per_unit),
            ));
            cmds.push(draft_cmd(MEASURE_DRAG, &measure_math::encode_path(&[end])));
        }
    }
    if p.primary_released {
        cmds.push(draft_cmd(MEASURE_DRAG, ""));
        let anchor = path.last().copied().or_else(|| {
            session
                .draft(MEASURE_ANCHOR)
                .and_then(|t| measure_math::decode_path(&t).first().copied())
        });
        if let Some(start) = anchor {
            let end = measure_math::constrain_plane_end(start, world, axes.0, axes.1, p.shift);
            let distinct = (start[0] - end[0]).abs() > 1e-4
                || (start[1] - end[1]).abs() > 1e-4
                || (start[2] - end[2]).abs() > 1e-4;
            if distinct {
                let mut next = path;
                if next.is_empty() {
                    next.push(start);
                }
                if next.last().copied() != Some(end) {
                    next.push(end);
                    let encoded = measure_math::encode_path(&next);
                    cmds.push(draft_cmd(MEASURE_PATH, &encoded));
                    cmds.push(draft_cmd(
                        MEASURE_STATUS,
                        &format_status(session, &next, flat_2d, perspective, pixels_per_unit),
                    ));
                }
            }
        } else {
            let encoded = measure_math::encode_path(&[world]);
            cmds.push(draft_cmd(MEASURE_PATH, &encoded));
            cmds.push(draft_cmd(MEASURE_ANCHOR, &encoded));
            cmds.push(draft_cmd(
                MEASURE_STATUS,
                &format_status(session, &[world], flat_2d, perspective, pixels_per_unit),
            ));
        }
    }
    if cmds.is_empty() {
        PanelEvent::None
    } else if cmds.len() == 1 {
        cmds.into_iter().next().unwrap()
    } else {
        PanelEvent::Commands(
            cmds.into_iter()
                .filter_map(|event| {
                    if let PanelEvent::Command(id, args) = event {
                        Some((id, args))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>(),
        )
    }
}

fn pick_world(
    p: &MeasurePointer,
    session: &EditorSession,
    perspective: bool,
    camera: &ViewportCamera,
    path: &[[f32; 3]],
    axes: (usize, usize, usize),
) -> Option<[f32; 3]> {
    let pos = p.pos?;
    let (sx, sy) = if perspective {
        measure_math::perspective_pick_to_scene(
            pos.x,
            pos.y,
            p.origin.x,
            p.origin.y,
            p.cell,
            camera.yaw_deg,
            camera.pitch_deg,
        )
    } else {
        measure_math::flat_pick_to_scene(
            pos.x,
            pos.y,
            p.origin.x,
            p.origin.y,
            p.cell,
            camera.yaw_deg,
        )
    };
    let depth = path.last().map(|pt| pt[axes.2]).unwrap_or(0.0);
    let mut world = measure_math::plane_point_to_world(sx, sy, axes, depth);
    let grid_step = session.models.viewport_grid_snap_step.max(0.001);
    if session.models.viewport_grid_snap {
        world = measure_math::snap_grid(world, true, grid_step);
    }
    let candidates = measure_math::snap_candidates_from_entities(&session.world.entities, 0.5);
    let threshold = if session.models.viewport_grid_snap || session.models.viewport_vertex_snap {
        grid_step
    } else {
        grid_step * 0.35
    };
    Some(measure_math::snap_to_nearest(world, &candidates, threshold))
}

fn format_status(
    session: &EditorSession,
    path: &[[f32; 3]],
    flat_2d: bool,
    perspective: bool,
    pixels_per_unit: f32,
) -> String {
    if path.len() < 2 {
        return session.messages.get("measure-status-idle");
    }
    let readout = measure_math::path_readout(path, pixels_per_unit, flat_2d, perspective);
    let seg = match readout.segments.last() {
        Some(s) => s,
        None => return session.messages.get("measure-status-idle"),
    };
    let mut parts = vec![session
        .messages
        .format("measure-status-length", &[("len", &format_units(seg.length))])];
    parts.push(session.messages.format(
        "measure-status-delta",
        &[
            ("dx", &format_units(seg.delta[0])),
            ("dy", &format_units(seg.delta[1])),
            ("dz", &format_units(seg.delta[2])),
        ],
    ));
    parts.push(
        session
            .messages
            .format("measure-status-angle", &[("deg", &format!("{:.1}", seg.angle_deg))]),
    );
    if readout.segments.len() > 1 {
        parts.push(
            session
                .messages
                .format("measure-status-total", &[("len", &format_units(readout.total_length))]),
        );
    }
    if let Some(px) = readout.pixels.filter(|_| flat_2d) {
        parts.push(
            session.messages.format("measure-status-pixels", &[("px", &format!("{:.0}", px))]),
        );
    }
    if readout.perspective_note {
        parts.push(session.messages.get("measure-status-perspective"));
    }
    parts.join("  ")
}

fn format_units(value: f32) -> String {
    if value.fract().abs() < 0.005 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}

pub fn paint_overlay(
    frame: &mut PanelFrame,
    session: &EditorSession,
    camera: &ViewportCamera,
    perspective: bool,
    axes: MeasureViewAxes,
    pointer: Option<&MeasurePointer>,
    view: &str,
) {
    if session.models.viewport_tool != "measure" {
        return;
    }
    let path = measure_math::decode_path(&session.draft(MEASURE_PATH).unwrap_or_default());
    let drag =
        session.draft(MEASURE_DRAG).and_then(|t| measure_math::decode_path(&t).first().copied());
    let anchor =
        session.draft(MEASURE_ANCHOR).and_then(|t| measure_math::decode_path(&t).first().copied());
    let live_end = pointer.and_then(|p| {
        pick_world(p, session, perspective, camera, &path, measure_math::plane_axes(view))
    });
    if path.is_empty() && drag.is_none() && anchor.is_none() && live_end.is_none() {
        return;
    }
    let ui = frame.ui_mut();
    let clip = ui.clip_rect();
    let max = ui.max_rect();
    let cursor = ui.cursor().min;
    let rect = egui::Rect::from_min_max(
        cursor,
        egui::pos2(clip.right().min(max.right()), clip.bottom().min(max.bottom())),
    );
    let mut cell = camera_cell(camera, perspective);
    let mut origin = rect.center() + egui::vec2(camera.pan_x, camera.pan_y);
    // The viewport widget reports its own rect and mapping; the frame cursor is already below it.
    let rect = match pointer {
        Some(p) => {
            origin = p.origin;
            cell = p.cell;
            p.rect
        }
        None => rect,
    };
    let painter = ui.painter().with_clip_rect(rect);
    let ink = ui.visuals().text_color();
    let accent = Color32::from_rgb(255, 200, 80);
    let stroke = Stroke::new(2.0, accent);
    let to_screen = |pt: [f32; 3]| {
        if perspective {
            seal_ui::project_scene_point(pt, camera.yaw_deg, camera.pitch_deg, origin, cell)
        } else {
            let sx = pt[axes.across] * axes.flip_across;
            let sy = pt[axes.up] * axes.flip_up;
            let (dx, dy) = measure_math::ortho_scene_to_screen(sx, sy, cell, camera.yaw_deg);
            egui::pos2(origin.x + dx, origin.y + dy)
        }
    };
    for window in path.windows(2) {
        painter.line_segment([to_screen(window[0]), to_screen(window[1])], stroke);
        let read = measure_math::segment_readout(window[0], window[1]);
        let mid = (to_screen(window[0]).to_vec2() + to_screen(window[1]).to_vec2()) * 0.5;
        let label = session
            .messages
            .format("measure-overlay-label", &[("len", &format_units(read.length))]);
        painter.text(
            mid.to_pos2() + egui::vec2(6.0, -6.0),
            egui::Align2::LEFT_BOTTOM,
            label,
            TextStyle::Small.resolve(ui.style()),
            ink,
        );
    }
    if let (Some(from), Some(to)) = (path.last().copied().or(anchor), drag.or(live_end)) {
        painter.line_segment([to_screen(from), to_screen(to)], stroke);
    }
    for pt in &path {
        painter.circle_filled(to_screen(*pt), 4.0, accent);
    }
    if let Some(status) = session.draft(MEASURE_STATUS) {
        if status != session.messages.get("measure-status-idle") {
            let band = egui::Rect::from_min_max(
                egui::pos2(rect.left() + 8.0, rect.top() + 36.0),
                egui::pos2(rect.right() - 8.0, rect.top() + 56.0),
            );
            painter.rect_filled(band, 2.0, ui.visuals().panel_fill.gamma_multiply(0.85));
            painter.text(
                band.left_center() + egui::vec2(6.0, 0.0),
                egui::Align2::LEFT_CENTER,
                status,
                TextStyle::Small.resolve(ui.style()),
                ink,
            );
        }
    }
}

fn camera_cell(camera: &ViewportCamera, perspective: bool) -> f32 {
    const DEFAULT_ORBIT_DISTANCE: f32 = 10.0;
    let dolly = if perspective { DEFAULT_ORBIT_DISTANCE / camera.distance.max(1.0) } else { 1.0 };
    let pitch_scale =
        if perspective { camera.pitch_deg.to_radians().cos().abs().max(0.35) } else { 1.0 };
    20.0 * camera.zoom * dolly * pitch_scale
}
