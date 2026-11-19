//! Collider tool: drag, add and delete polygon and edge points in an XY view.

use seal_editor_core::{collider_points, measure_math, CommandArgs, EditorSession, PlayMode};
use seal_ui::{MeasurePointer, PanelFrame};

use crate::PanelEvent;

const GRAB: &str = "collider-grab";
const INDEX: &str = "collider-index";

fn draft(key: &str, value: &str) -> (String, CommandArgs) {
    ("draft.set".into(), CommandArgs::new(&[("key", key), ("value", value)]))
}

fn selected(session: &EditorSession) -> Option<String> {
    let ids = &session.foundation.selection;
    if ids.len() == 1 {
        return Some(ids[0].clone());
    }
    if ids.is_empty() {
        return session.models.selection.clone();
    }
    None
}

fn xy_view(view: &str, perspective: bool) -> bool {
    !perspective && matches!(view, "front" | "back" | "2d" | "")
}

fn view_of(pointer: &MeasurePointer, flip: [f32; 2]) -> Option<[f32; 2]> {
    let pos = pointer.pos?;
    let (x, y) = measure_math::flat_pick_to_scene(
        pos.x,
        pos.y,
        pointer.origin.x,
        pointer.origin.y,
        pointer.cell,
        0.0,
    );
    Some([x * flip[0], y * flip[1]])
}

fn screen_of(pointer: &MeasurePointer, world: [f32; 2], flip: [f32; 2]) -> egui::Pos2 {
    let x = world[0] * flip[0];
    let y = world[1] * flip[1];
    egui::pos2(pointer.origin.x + x * pointer.cell, pointer.origin.y - y * pointer.cell)
}

/// Handle the collider tool. `None` when this tool is not active.
pub fn handle(
    session: &EditorSession,
    pointer: Option<&MeasurePointer>,
    view: &str,
    perspective: bool,
    flip: [f32; 2],
) -> Option<PanelEvent> {
    if session.models.viewport_tool != "collider" || session.world.mode != PlayMode::Editing {
        return None;
    }
    if !xy_view(view, perspective) {
        return Some(PanelEvent::None);
    }
    let Some(pointer) = pointer else {
        return Some(PanelEvent::None);
    };
    let Some(id) = selected(session) else {
        return Some(PanelEvent::None);
    };
    let Some(entity) = session.world.entity(&id) else {
        return Some(PanelEvent::None);
    };
    if !matches!(entity.phys2d.shape.as_str(), "polygon" | "edge") {
        return Some(PanelEvent::None);
    }
    let closed = entity.phys2d.shape == "polygon";
    let world_pts: Vec<[f32; 2]> =
        entity.phys2d.points.iter().map(|p| collider_points::to_world(entity, *p)).collect();
    let Some(at) = view_of(pointer, flip) else {
        return Some(PanelEvent::None);
    };
    let reach = 10.0 / pointer.cell.max(1.0);
    let near = |p: [f32; 2]| {
        let d = [p[0] - at[0], p[1] - at[1]];
        (d[0] * d[0] + d[1] * d[1]).sqrt() <= reach
    };
    if pointer.primary_released {
        return Some(PanelEvent::Commands(vec![draft(GRAB, ""), draft(INDEX, "")]));
    }
    let grab = session.draft(GRAB).unwrap_or_default();
    if pointer.primary_down && !pointer.primary_pressed {
        if let Some((grab_id, index)) = grab.split_once('/') {
            if grab_id == id {
                if let Ok(index) = index.parse::<usize>() {
                    return Some(edit(&id, "point", index, at, "0"));
                }
            }
        }
    }
    if pointer.double_clicked {
        if let Some(index) = closest_edge(&world_pts, at, reach * 1.5, closed) {
            return Some(edit(&id, "insert", index, at, "1"));
        }
    }
    if pointer.primary_pressed {
        if let Some(index) = world_pts.iter().position(|p| near(*p)) {
            let mut cmds =
                vec![draft(GRAB, &format!("{id}/{index}")), draft(INDEX, &index.to_string())];
            if let PanelEvent::Command(name, args) = edit(&id, "point", index, at, "1") {
                cmds.push((name, args));
            }
            return Some(PanelEvent::Commands(cmds));
        }
    }
    if let Some(index) = world_pts.iter().position(|p| near(*p)) {
        if session.draft(INDEX).as_deref() != Some(&index.to_string()) {
            return Some(PanelEvent::Command(
                "draft.set".into(),
                CommandArgs::new(&[("key", INDEX), ("value", &index.to_string())]),
            ));
        }
    }
    Some(PanelEvent::None)
}

/// Delete uses the viewport key, not the pointer.
pub fn delete_key(session: &EditorSession, key: &str) -> Option<PanelEvent> {
    if session.models.viewport_tool != "collider" {
        return None;
    }
    if !matches!(key, "Delete" | "KeyDelete" | "Backspace" | "KeyBackspace") {
        return None;
    }
    let id = selected(session)?;
    let index: usize = session.draft(INDEX)?.parse().ok()?;
    Some(PanelEvent::Commands(vec![
        (
            "collider.edit".into(),
            CommandArgs::new(&[
                ("id", id.as_str()),
                ("op", "delete"),
                ("index", &index.to_string()),
                ("begin", "1"),
            ]),
        ),
        draft(INDEX, ""),
        draft(GRAB, ""),
    ]))
}

fn edit(id: &str, op: &str, index: usize, at: [f32; 2], begin: &str) -> PanelEvent {
    PanelEvent::Command(
        "collider.edit".into(),
        CommandArgs::new(&[
            ("id", id),
            ("op", op),
            ("index", &index.to_string()),
            ("x", &at[0].to_string()),
            ("y", &at[1].to_string()),
            ("begin", begin),
        ]),
    )
}

fn closest_edge(points: &[[f32; 2]], at: [f32; 2], reach: f32, closed: bool) -> Option<usize> {
    if points.len() < 2 {
        return None;
    }
    let mut best = None;
    let mut best_d = reach;
    let n = points.len();
    let edges = if closed && n >= 3 { n } else { n - 1 };
    for i in 0..edges {
        let a = points[i];
        let b = points[(i + 1) % n];
        let ab = [b[0] - a[0], b[1] - a[1]];
        let len2 = ab[0] * ab[0] + ab[1] * ab[1];
        let t = if len2 < 1e-8 {
            0.0
        } else {
            ((at[0] - a[0]) * ab[0] + (at[1] - a[1]) * ab[1]) / len2
        };
        let t = t.clamp(0.0, 1.0);
        let q = [a[0] + ab[0] * t, a[1] + ab[1] * t];
        let d = ((q[0] - at[0]).powi(2) + (q[1] - at[1]).powi(2)).sqrt();
        if d <= best_d {
            best_d = d;
            best = Some(i);
        }
    }
    best
}

/// Draw the points of the selected polygon or edge.
pub fn paint(
    frame: &mut PanelFrame,
    session: &EditorSession,
    pointer: Option<&MeasurePointer>,
    view: &str,
    perspective: bool,
    flip: [f32; 2],
) {
    if session.models.viewport_tool != "collider" || !xy_view(view, perspective) {
        return;
    }
    let Some(pointer) = pointer else { return };
    let Some(id) = selected(session) else { return };
    let Some(entity) = session.world.entity(&id) else { return };
    if !matches!(entity.phys2d.shape.as_str(), "polygon" | "edge") {
        return;
    }
    let painter = frame.ui_mut().painter();
    let pts: Vec<egui::Pos2> = entity
        .phys2d
        .points
        .iter()
        .map(|p| screen_of(pointer, collider_points::to_world(entity, *p), flip))
        .collect();
    if pts.len() >= 2 {
        let closed = entity.phys2d.shape == "polygon" && pts.len() >= 3;
        let stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(120, 200, 255));
        for i in 0..pts.len() - 1 {
            painter.line_segment([pts[i], pts[i + 1]], stroke);
        }
        if closed {
            painter.line_segment([pts[pts.len() - 1], pts[0]], stroke);
        }
    }
    for p in &pts {
        painter.circle_filled(*p, 4.0, egui::Color32::from_rgb(255, 220, 80));
        painter.circle_stroke(*p, 4.0, egui::Stroke::new(1.0, egui::Color32::BLACK));
    }
}
