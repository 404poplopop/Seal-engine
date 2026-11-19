//! Point handles for a polygon drawn in the flat view.

use egui::{Color32, Pos2, Stroke};

/// A point drag, in scene units.
#[derive(Clone, Debug)]
pub struct PolyEdit {
    pub mark: usize,
    pub index: usize,
    pub x: f32,
    pub y: f32,
    pub began: bool,
}

/// Drag kept while the pointer is down.
#[derive(Clone, Copy, Debug)]
pub struct PolyGrab {
    pub mark: usize,
    pub index: usize,
    pub fresh: bool,
}

const HIT_PX: f32 = 8.0;

pub fn paint(painter: &egui::Painter, points: &[[f32; 2]], to_screen: &impl Fn(f32, f32) -> Pos2) {
    for point in points {
        let at = to_screen(point[0], point[1]);
        painter.circle_stroke(at, 5.0, Stroke::new(1.5, Color32::WHITE));
        painter.circle_filled(at, 3.0, Color32::from_rgb(255, 196, 64));
    }
}

pub fn hit(points: &[[f32; 2]], at: Pos2, to_screen: &impl Fn(f32, f32) -> Pos2) -> Option<usize> {
    points.iter().enumerate().rev().find_map(|(index, point)| {
        let screen = to_screen(point[0], point[1]);
        (screen.distance(at) <= HIT_PX).then_some(index)
    })
}
