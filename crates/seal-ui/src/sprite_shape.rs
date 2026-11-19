//! Drawing and handle hits for a sprite shape in the flat viewport.

use egui::{Color32, Pos2, Stroke};

/// Three 2D points: a triangle on the view plane or its UVs.
pub type Tri2 = [[f32; 2]; 3];

/// One control, already placed on the view plane.
#[derive(Clone, Debug)]
pub struct SplineControl {
    pub at: [f32; 2],
    pub smooth: bool,
    /// Outgoing tangent tip, on the view plane.
    pub tip: [f32; 2],
}

/// Everything the viewport needs to draw and edit one sprite shape.
#[derive(Clone, Debug)]
pub struct SpriteShapeMark {
    pub controls: Vec<SplineControl>,
    pub outline: Vec<[f32; 2]>,
    /// Control segment that each outline edge belongs to.
    pub seg: Vec<usize>,
    pub fill: Vec<(Tri2, Tri2)>,
    pub edge: Vec<(u8, Tri2, Tri2)>,
    pub fill_image: Option<String>,
    pub edge_image: [Option<String>; 3],
    pub tint: [u8; 4],
    pub editing: bool,
    pub smooth_label: String,
    pub corner_label: String,
}

/// A handle the pointer is over.
#[derive(Clone, Copy, Debug)]
pub struct ControlHit {
    pub mark: usize,
    pub handle: bool,
    pub index: usize,
}

/// One edit for the scene command.
#[derive(Clone, Debug)]
pub struct SplineEdit {
    pub mark: usize,
    pub op: &'static str,
    pub index: usize,
    pub x: f32,
    pub y: f32,
    pub began: bool,
    pub smooth: bool,
}

/// Drag kept while the pointer is down.
#[derive(Clone, Copy, Debug)]
pub struct SplineGrab {
    pub mark: usize,
    pub handle: bool,
    pub index: usize,
    pub fresh: bool,
}

const HIT_PX: f32 = 8.0;

fn near(at: Pos2, screen: Pos2) -> bool {
    at.distance(screen) <= HIT_PX
}

pub fn hit_control(
    marks: &[crate::GridMark],
    at: Pos2,
    to_screen: &impl Fn(f32, f32) -> Pos2,
) -> Option<ControlHit> {
    for (mark_i, mark) in marks.iter().enumerate() {
        let Some(shape) = &mark.sprite_shape else { continue };
        if !shape.editing {
            continue;
        }
        for (index, point) in shape.controls.iter().enumerate() {
            if point.smooth && near(at, to_screen(point.tip[0], point.tip[1])) {
                return Some(ControlHit { mark: mark_i, handle: true, index });
            }
        }
        for (index, point) in shape.controls.iter().enumerate() {
            if near(at, to_screen(point.at[0], point.at[1])) {
                return Some(ControlHit { mark: mark_i, handle: false, index });
            }
        }
    }
    None
}

fn dist_to_seg(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let den = dx * dx + dy * dy;
    if den < 1e-12 {
        return ((p[0] - a[0]).powi(2) + (p[1] - a[1]).powi(2)).sqrt();
    }
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / den).clamp(0.0, 1.0);
    let (x, y) = (a[0] + dx * t, a[1] + dy * t);
    ((p[0] - x).powi(2) + (p[1] - y).powi(2)).sqrt()
}

/// Control segment under a view-plane point, when it is close to the outline.
pub fn hit_outline(shape: &SpriteShapeMark, at: [f32; 2], max_dist: f32) -> Option<usize> {
    shape
        .outline
        .windows(2)
        .enumerate()
        .filter(|(_, w)| dist_to_seg(at, w[0], w[1]) <= max_dist)
        .min_by(|(_, a), (_, b)| {
            dist_to_seg(at, a[0], a[1])
                .partial_cmp(&dist_to_seg(at, b[0], b[1]))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .and_then(|(i, _)| shape.seg.get(i).copied())
}

fn map_uv(place: [f32; 4], uv: [f32; 2]) -> [f32; 2] {
    let (u, v) = (uv[0].rem_euclid(1.0), uv[1].rem_euclid(1.0));
    let (w, h) = (place[2] - place[0], place[3] - place[1]);
    [place[0] + u * w, place[1] + v * h]
}

fn paint_tris(
    painter: &egui::Painter,
    ctx: &egui::Context,
    image: Option<&str>,
    tris: &[([Pos2; 3], [[f32; 2]; 3])],
    tint: Color32,
) {
    if tris.is_empty() {
        return;
    }
    let loaded = image.and_then(|path| crate::sprites::sprite_texture(ctx, path));
    let (mut mesh, place) = match loaded {
        Some((id, place, _)) => (egui::Mesh::with_texture(id), Some(place)),
        None => (egui::Mesh::default(), None),
    };
    for (screen, uv) in tris {
        let base = mesh.vertices.len() as u32;
        for i in 0..3 {
            let mapped = match place {
                Some(rect) => map_uv(rect, uv[i]),
                None => [0.0, 0.0],
            };
            mesh.colored_vertex(screen[i], tint);
            if let Some(v) = mesh.vertices.last_mut() {
                v.uv = egui::pos2(mapped[0], mapped[1]);
            }
        }
        mesh.add_triangle(base, base + 1, base + 2);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Fill and edge. Handles are drawn later so they sit on top.
pub fn paint_body(
    painter: &egui::Painter,
    ctx: &egui::Context,
    shape: &SpriteShapeMark,
    to_screen: &impl Fn(f32, f32) -> Pos2,
) {
    let [r, g, b, a] = shape.tint;
    let tint = Color32::from_rgba_unmultiplied(r, g, b, a);
    let screen_fill: Vec<_> = shape
        .fill
        .iter()
        .map(|(pos, uv)| ([pos[0], pos[1], pos[2]].map(|p| to_screen(p[0], p[1])), *uv))
        .collect();
    paint_tris(painter, ctx, shape.fill_image.as_deref(), &screen_fill, tint);
    for band in 0..3 {
        let tris: Vec<_> = shape
            .edge
            .iter()
            .filter(|(id, _, _)| *id == band)
            .map(|(_, pos, uv)| ([pos[0], pos[1], pos[2]].map(|p| to_screen(p[0], p[1])), *uv))
            .collect();
        paint_tris(painter, ctx, shape.edge_image[band as usize].as_deref(), &tris, tint);
    }
    if shape.fill.is_empty() && shape.outline.len() >= 2 {
        let pts: Vec<Pos2> = shape.outline.iter().copied().map(|p| to_screen(p[0], p[1])).collect();
        let stroke = Stroke::new(1.5, tint);
        if pts.first() == pts.last() && pts.len() > 2 {
            let open = pts[..pts.len() - 1].to_vec();
            painter.add(egui::Shape::closed_line(open, stroke));
        } else {
            painter.add(egui::Shape::line(pts, stroke));
        }
    }
}

pub fn paint_handles(
    painter: &egui::Painter,
    shape: &SpriteShapeMark,
    selected: Option<usize>,
    to_screen: &impl Fn(f32, f32) -> Pos2,
) {
    if !shape.editing {
        return;
    }
    let ink = Color32::from_rgb(0xff, 0x8c, 0x2a);
    let handle = Color32::from_rgb(80, 170, 255);
    for (index, point) in shape.controls.iter().enumerate() {
        let at = to_screen(point.at[0], point.at[1]);
        if point.smooth {
            let tip = to_screen(point.tip[0], point.tip[1]);
            painter.line_segment([at, tip], Stroke::new(1.0, handle));
            painter.rect_filled(
                egui::Rect::from_center_size(tip, egui::vec2(7.0, 7.0)),
                1.0,
                handle,
            );
        }
        let color = if selected == Some(index) { ink } else { Color32::WHITE };
        painter.circle_filled(at, 4.5, color);
        painter.circle_stroke(at, 4.5, Stroke::new(1.0, Color32::BLACK));
    }
}

pub fn point_menu(shape: &SpriteShapeMark) -> Vec<crate::ContextMenuEntry> {
    let row = |label: &str, command: &str| {
        crate::ContextMenuEntry::Row(crate::ContextMenuRow {
            label: label.to_string(),
            icon: crate::icons::Icon::ToolMove,
            shortcut: String::new(),
            enabled: true,
            reason: String::new(),
            command: Some((command.into(), Vec::new())),
            children: Vec::new(),
        })
    };
    vec![row(&shape.smooth_label, "ss-smooth"), row(&shape.corner_label, "ss-corner")]
}
