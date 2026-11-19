//! The node editor canvas: crisp square nodes, typed pins, wires, pan and zoom.
//!
//! The canvas edits a `GraphDoc` directly; the document saves itself after every change.

use crate::crisp;
use egui::epaint::CubicBezierShape;
use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};
use seal_editor_core::graph_doc::{Elem, GraphDoc, NodeView, PinSide, PinView, Ty};

/// Node width, header height and pin row height, in graph units (scaled by zoom).
pub const NODE_W: f32 = 176.0;
pub const HEADER_H: f32 = 22.0;
pub const ROW_H: f32 = 20.0;
const PIN_PX: f32 = 8.0;
const GRID: f32 = 16.0;
/// How many play steps a node stays outlined after it ran (about half a second).
const RAN_GLOW_STEPS: u64 = 30;
pub const ZOOM_MIN: f32 = 0.3;
pub const ZOOM_MAX: f32 = 2.0;

/// Words the canvas shows, already translated.
pub struct GraphCanvasText<'a> {
    pub search: &'a str,
    pub delete_node: &'a str,
    pub disconnect: &'a str,
    pub hint: &'a str,
    pub breakpoint: &'a str,
}

/// What happened on the canvas this frame.
#[derive(Default)]
pub struct GraphCanvasOutput {
    /// The user pressed on the canvas (the panel should take focus).
    pub touched: bool,
    /// A refused edit, such as a link between pins of different types.
    pub refused: Option<String>,
}

/// One pin, as a stable key.
#[derive(Clone, Debug, PartialEq)]
struct PinKey {
    node: u32,
    pin: String,
    side: PinSide,
}

#[derive(Clone, Debug, Default)]
enum Drag {
    #[default]
    None,
    Pan,
    Node {
        id: u32,
        grab: Vec2,
    },
    Wire {
        from: PinKey,
        ty: Option<Ty>,
    },
}

#[derive(Clone, Debug)]
enum Menu {
    /// Add a node at `at` (graph units); with `wire`, link it to that pin.
    Add {
        screen: Pos2,
        at: [f32; 2],
        wire: Option<PinKey>,
        search: String,
        fresh: bool,
    },
    Node {
        screen: Pos2,
        id: u32,
    },
}

#[derive(Clone, Debug, Default)]
struct CanvasState {
    drag: Drag,
    menu: Option<Menu>,
}

/// The color of a pin and its wires. Execution pins are light grey.
pub fn pin_color(ty: Option<Ty>) -> Color32 {
    let elem = |e: Elem| match e {
        Elem::Bool => Ty::Bool,
        Elem::Int => Ty::Int,
        Elem::Float => Ty::Float,
        Elem::Vec2 => Ty::Vec2,
        Elem::Vec3 => Ty::Vec3,
        Elem::Str => Ty::Str,
        Elem::Entity => Ty::Entity,
    };
    match ty {
        None => Color32::from_rgb(222, 226, 232),
        Some(Ty::Bool) => Color32::from_rgb(214, 74, 74),
        Some(Ty::Int) => Color32::from_rgb(64, 204, 172),
        Some(Ty::Float) => Color32::from_rgb(150, 214, 84),
        Some(Ty::Vec2) => Color32::from_rgb(236, 196, 70),
        Some(Ty::Vec3) => Color32::from_rgb(244, 156, 52),
        Some(Ty::Quat) => Color32::from_rgb(196, 120, 210),
        Some(Ty::Plane) => Color32::from_rgb(120, 180, 220),
        Some(Ty::Str) => Color32::from_rgb(226, 98, 206),
        Some(Ty::Entity) => Color32::from_rgb(78, 148, 244),
        Some(Ty::List(e)) => pin_color(Some(elem(e))),
    }
}

/// Graph units to screen.
pub fn to_screen(origin: Pos2, pan: [f32; 2], zoom: f32, p: [f32; 2]) -> Pos2 {
    Pos2::new(origin.x + pan[0] + p[0] * zoom, origin.y + pan[1] + p[1] * zoom)
}

/// Screen to graph units.
pub fn to_graph(origin: Pos2, pan: [f32; 2], zoom: f32, s: Pos2) -> [f32; 2] {
    [(s.x - origin.x - pan[0]) / zoom, (s.y - origin.y - pan[1]) / zoom]
}

/// Size of a node in graph units.
pub fn node_size(view: &NodeView) -> Vec2 {
    let rows = view.ins.len().max(view.outs.len()) as f32;
    Vec2::new(NODE_W, HEADER_H + rows * ROW_H + 6.0)
}

/// Center of pin `index` on `side`, in graph units.
pub fn pin_anchor(view: &NodeView, side: PinSide, index: usize) -> [f32; 2] {
    let y = view.pos[1] + HEADER_H + (index as f32 + 0.5) * ROW_H + 3.0;
    match side {
        PinSide::In => [view.pos[0], y],
        PinSide::Out => [view.pos[0] + NODE_W, y],
    }
}

fn pins(view: &NodeView, side: PinSide) -> &[PinView] {
    match side {
        PinSide::In => &view.ins,
        PinSide::Out => &view.outs,
    }
}

fn find_pin<'a>(views: &'a [NodeView], key: &PinKey) -> Option<(&'a NodeView, usize, &'a PinView)> {
    let view = views.iter().find(|v| v.id == key.node)?;
    let (i, pin) = pins(view, key.side).iter().enumerate().find(|(_, p)| p.name == key.pin)?;
    Some((view, i, pin))
}

/// Which category tints the header: events, steps that run in order, or values.
fn header_tint(view: &NodeView) -> Color32 {
    let exec = view.ins.iter().chain(&view.outs).any(|p| p.ty.is_none());
    if view.kind == "event" {
        Color32::from_rgb(170, 52, 52)
    } else if exec {
        Color32::from_rgb(52, 98, 170)
    } else {
        Color32::from_rgb(64, 120, 70)
    }
}

/// Draw the canvas and handle its input. Fills the remaining space of `ui`.
pub fn graph_canvas(
    ui: &mut Ui,
    doc: &mut GraphDoc,
    text: &GraphCanvasText<'_>,
) -> GraphCanvasOutput {
    let mut out = GraphCanvasOutput::default();
    let id = ui.id().with("graph-canvas").with(&doc.label);
    let rect = ui.available_rect_before_wrap();
    let response = ui.interact(rect, id, Sense::click_and_drag());
    ui.allocate_rect(rect, Sense::hover());
    let mut state: CanvasState = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let painter = ui.painter_at(rect);
    let v = ui.visuals().clone();
    let origin = rect.min;
    let zoom = doc.zoom.clamp(ZOOM_MIN, ZOOM_MAX);
    let views = doc.views();
    let pointer = ui.input(|i| i.pointer.hover_pos());

    // Background: a dark tray with a crisp grid.
    painter.rect_filled(rect, 0.0, crisp::mix(v.extreme_bg_color, Color32::BLACK, 0.25));
    let step = GRID * zoom;
    if step >= 6.0 {
        let minor = Color32::from_white_alpha(if v.dark_mode { 8 } else { 30 });
        let major = Color32::from_white_alpha(if v.dark_mode { 18 } else { 60 });
        let start_x = origin.x + doc.pan[0].rem_euclid(step);
        let start_y = origin.y + doc.pan[1].rem_euclid(step);
        let first_ix = ((start_x - origin.x - doc.pan[0]) / step).round() as i64;
        let first_iy = ((start_y - origin.y - doc.pan[1]) / step).round() as i64;
        let mut x = start_x;
        let mut ix = first_ix;
        while x < rect.right() {
            let c = if ix.rem_euclid(8) == 0 { major } else { minor };
            painter.vline(x.round() + 0.5, rect.y_range(), Stroke::new(1.0, c));
            x += step;
            ix += 1;
        }
        let mut y = start_y;
        let mut iy = first_iy;
        while y < rect.bottom() {
            let c = if iy.rem_euclid(8) == 0 { major } else { minor };
            painter.hline(rect.x_range(), y.round() + 0.5, Stroke::new(1.0, c));
            y += step;
            iy += 1;
        }
    }

    let pan = doc.pan;
    let screen = |p: [f32; 2]| to_screen(origin, pan, zoom, p);
    let node_rect = |view: &NodeView| Rect::from_min_size(screen(view.pos), node_size(view) * zoom);
    let pin_hit = |pos: Pos2| -> Option<PinKey> {
        let reach = (PIN_PX * zoom).max(6.0) + 2.0;
        for view in views.iter().rev() {
            for side in [PinSide::In, PinSide::Out] {
                for (i, pin) in pins(view, side).iter().enumerate() {
                    if screen(pin_anchor(view, side, i)).distance(pos) <= reach {
                        return Some(PinKey { node: view.id, pin: pin.name.clone(), side });
                    }
                }
            }
        }
        None
    };
    let node_hit =
        |pos: Pos2| views.iter().rev().find(|v| node_rect(v).contains(pos)).map(|v| v.id);

    // Wires.
    let wire = |a: Pos2, b: Pos2, color: Color32, width: f32| {
        let bend = ((b.x - a.x).abs() * 0.5).max(40.0 * zoom);
        let shape = CubicBezierShape::from_points_stroke(
            [a, a + Vec2::new(bend, 0.0), b - Vec2::new(bend, 0.0), b],
            false,
            Color32::TRANSPARENT,
            Stroke::new(width, color),
        );
        painter.add(shape);
    };
    for link in doc.active_links() {
        let from = PinKey { node: link.from.node, pin: link.from.pin.clone(), side: PinSide::Out };
        let to = PinKey { node: link.to.node, pin: link.to.pin.clone(), side: PinSide::In };
        if let (Some((fv, fi, fp)), Some((tv, ti, _))) =
            (find_pin(&views, &from), find_pin(&views, &to))
        {
            let width = if fp.ty.is_none() { 2.2 } else { 1.6 } * zoom.max(0.6);
            wire(
                screen(pin_anchor(fv, PinSide::Out, fi)),
                screen(pin_anchor(tv, PinSide::In, ti)),
                pin_color(fp.ty),
                width,
            );
        }
    }

    // Nodes.
    let small = egui::FontId::proportional((11.0 * zoom).clamp(7.0, 18.0));
    let title_font = egui::FontId::proportional((12.0 * zoom).clamp(7.0, 20.0));
    let ink = v.text_color();
    let mut header_tips: Vec<(Rect, String)> = Vec::new();
    let mut default_fields: Vec<(Rect, u32, PinView)> = Vec::new();
    for view in &views {
        let r = crisp::snap(node_rect(view));
        if !r.intersects(rect) {
            continue;
        }
        let selected = doc.selected == Some(view.id);
        let mut sink = &painter;
        // Body: a dark sunken plate.
        painter.add(Shape::rect_filled(
            r.translate(Vec2::new(0.0, 2.0)),
            0.0,
            Color32::from_black_alpha(70),
        ));
        painter.add(Shape::rect_filled(r, 0.0, crisp::mix(v.panel_fill, Color32::BLACK, 0.12)));
        // Header: raised, tinted by category.
        let head = Rect::from_min_size(r.min, Vec2::new(r.width(), HEADER_H * zoom));
        let tint = header_tint(view);
        crisp::vgrad(
            &mut sink,
            head,
            crisp::mix(tint, Color32::WHITE, 0.08),
            crisp::mix(tint, Color32::BLACK, 0.25),
        );
        painter.hline(
            head.x_range(),
            head.top() + 1.5,
            Stroke::new(1.0, Color32::from_white_alpha(30)),
        );
        painter.hline(
            head.x_range(),
            head.bottom() - 0.5,
            Stroke::new(1.0, Color32::from_black_alpha(160)),
        );
        // LED slot in the header: blue when selected, amber when the node has a problem.
        let slot = Rect::from_min_size(
            Pos2::new(head.right() - 18.0 * zoom, head.center().y - 2.0 * zoom),
            Vec2::new(12.0 * zoom, (4.0 * zoom).max(3.0)),
        );
        let amber = Color32::from_rgb(255, 170, 40);
        match (&view.problem, selected) {
            (Some(_), _) => crisp::led_slot(&mut sink, slot, true, amber),
            (None, true) => crisp::led_slot(&mut sink, slot, true, v.selection.stroke.color),
            (None, false) => crisp::led_slot(&mut sink, slot, false, Color32::BLACK),
        }
        if zoom >= 0.45 {
            painter.text(
                Pos2::new(head.left() + 7.0 * zoom, head.center().y),
                egui::Align2::LEFT_CENTER,
                &view.title,
                title_font.clone(),
                Color32::from_rgb(240, 242, 246),
            );
        }
        let edge = if view.problem.is_some() {
            amber
        } else if selected {
            v.selection.stroke.color
        } else {
            Color32::from_black_alpha(200)
        };
        painter.rect_stroke(r, 0.0, Stroke::new(1.0, edge), egui::StrokeKind::Inside);
        // Debugger: the node play is stopped before, and nodes that ran lately.
        if let Some(debug) = &doc.debug {
            let gold = Color32::from_rgb(255, 214, 64);
            if debug.paused_at == Some(view.id) {
                painter.rect_stroke(
                    r.expand(3.0),
                    0.0,
                    Stroke::new(2.5, gold),
                    egui::StrokeKind::Outside,
                );
            } else if debug
                .ran
                .get(&view.id)
                .is_some_and(|&at| debug.step.saturating_sub(at) <= RAN_GLOW_STEPS)
            {
                painter.rect_stroke(
                    r.expand(1.5),
                    0.0,
                    Stroke::new(1.0, gold.gamma_multiply(0.6)),
                    egui::StrokeKind::Outside,
                );
            }
        }
        if doc.breakpoints.contains(&view.id) {
            let c = Pos2::new(head.left() - 1.0, head.top() - 1.0);
            let rad = (5.0 * zoom).max(3.5);
            painter.circle_filled(c, rad + 1.0, Color32::from_black_alpha(200));
            painter.circle_filled(c, rad, Color32::from_rgb(220, 50, 50));
            painter.circle_filled(
                c - Vec2::splat(rad * 0.3),
                rad * 0.35,
                Color32::from_rgb(255, 140, 140),
            );
        }
        if let Some(problem) = &view.problem {
            header_tips.push((head, problem.clone()));
        }
        // Pins.
        for side in [PinSide::In, PinSide::Out] {
            let mut data_out = 0u32;
            for (i, pin) in pins(view, side).iter().enumerate() {
                let c = screen(pin_anchor(view, side, i));
                // Watched value: the last value this output produced in play.
                if side == PinSide::Out && pin.ty.is_some() {
                    let value = doc.debug.as_ref().and_then(|d| d.values.get(&(view.id, data_out)));
                    data_out += 1;
                    if let (Some(value), true) = (value, zoom >= 0.55) {
                        let shown: String = if value.chars().count() > 24 {
                            value.chars().take(23).chain(['\u{2026}']).collect()
                        } else {
                            value.clone()
                        };
                        let galley = painter.layout_no_wrap(
                            shown,
                            small.clone(),
                            Color32::from_rgb(255, 226, 120),
                        );
                        let at = c + Vec2::new(PIN_PX * zoom + 4.0, -galley.size().y * 0.5);
                        let back =
                            Rect::from_min_size(at, galley.size()).expand2(Vec2::new(3.0, 1.0));
                        painter.rect_filled(back, 0.0, Color32::from_black_alpha(210));
                        painter.rect_stroke(
                            back,
                            0.0,
                            Stroke::new(1.0, Color32::from_rgb(110, 90, 30)),
                            egui::StrokeKind::Inside,
                        );
                        painter.galley(at, galley, Color32::WHITE);
                    }
                }
                let s = (PIN_PX * zoom).max(5.0);
                let color = pin_color(pin.ty);
                if pin.ty.is_none() {
                    let pts = vec![
                        c + Vec2::new(-s * 0.5, -s * 0.6),
                        c + Vec2::new(s * 0.6, 0.0),
                        c + Vec2::new(-s * 0.5, s * 0.6),
                    ];
                    let fill = if pin.linked { color } else { Color32::TRANSPARENT };
                    painter.add(Shape::convex_polygon(pts, fill, Stroke::new(1.2, color)));
                } else {
                    let pr = crisp::snap(Rect::from_center_size(c, Vec2::splat(s)));
                    if pin.linked {
                        painter.rect_filled(pr, 0.0, color);
                    } else {
                        painter.rect_filled(pr, 0.0, Color32::from_rgb(14, 16, 20));
                        painter.rect_stroke(
                            pr,
                            0.0,
                            Stroke::new(1.2, color),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
                // Plain "in" and "then" execution pins need no label.
                let plain = pin.ty.is_none() && matches!(pin.name.as_str(), "in" | "then");
                if zoom >= 0.55 && !plain {
                    let (pos, align) = match side {
                        PinSide::In => (c + Vec2::new(s + 2.0, 0.0), egui::Align2::LEFT_CENTER),
                        PinSide::Out => (c - Vec2::new(s + 2.0, 0.0), egui::Align2::RIGHT_CENTER),
                    };
                    let label = painter.text(
                        pos,
                        align,
                        &pin.name,
                        small.clone(),
                        crisp::mix(ink, Color32::GRAY, 0.25),
                    );
                    if let (PinSide::In, Some(_)) = (side, &pin.default_text) {
                        if zoom >= 0.7 {
                            let left = label.right() + 6.0;
                            let right = r.center().x + 30.0 * zoom;
                            if right - left > 24.0 {
                                let field = Rect::from_min_max(
                                    Pos2::new(left, c.y - 8.0 * zoom),
                                    Pos2::new(right, c.y + 8.0 * zoom),
                                );
                                default_fields.push((field, view.id, pin.clone()));
                            }
                        }
                    }
                }
            }
        }
    }

    // A wire being dragged.
    if let (Drag::Wire { from, ty }, Some(p)) = (&state.drag, pointer) {
        if let Some((view, i, _)) = find_pin(&views, from) {
            let a = screen(pin_anchor(view, from.side, i));
            let (a, b) = if from.side == PinSide::Out { (a, p) } else { (p, a) };
            wire(a, b, pin_color(*ty), 1.6 * zoom.max(0.6));
        }
    }

    // Inline defaults for unlinked inputs. Text is committed when the field loses focus.
    for (field, node, pin) in default_fields {
        if pin.ty == Some(Ty::Bool) {
            let on = pin.default_text.as_deref() == Some("true");
            let fid = id.with(("bool", node, &pin.name));
            let box_r =
                Rect::from_min_size(field.left_center() - Vec2::new(0.0, 6.0), Vec2::splat(12.0));
            let resp = ui.interact(box_r, fid, Sense::click());
            crisp::checkbox(&mut &painter, box_r, &v, on, false, resp.hovered());
            if resp.clicked() {
                let _ = doc.set_default(node, &pin.name, if on { "false" } else { "true" });
            }
            continue;
        }
        let fid = id.with(("default", node, &pin.name));
        let current = pin.default_text.clone().unwrap_or_default();
        let mut buffer: String = ui.data(|d| d.get_temp(fid)).unwrap_or_else(|| current.clone());
        crisp::sunken(&mut &painter, field, &v, false, false);
        let edit = egui::TextEdit::singleline(&mut buffer)
            .id(fid)
            .font(small.clone())
            .frame(egui::Frame::NONE)
            .margin(Vec2::new(4.0, 0.0))
            .desired_width(field.width());
        let resp = ui.put(field, edit);
        if resp.has_focus() {
            ui.data_mut(|d| d.insert_temp(fid, buffer));
        } else {
            if resp.lost_focus() && buffer != current {
                if let Err(e) = doc.set_default(node, &pin.name, &buffer) {
                    out.refused = Some(e);
                }
            }
            ui.data_mut(|d| d.remove::<String>(fid));
        }
    }

    // Problem tips on node headers.
    if let Some(p) = pointer {
        if let Some((_, tip)) = header_tips.iter().find(|(r, _)| r.contains(p)) {
            response.clone().on_hover_text(tip.as_str());
        }
    }

    // ---- Input -----------------------------------------------------------
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y + i.zoom_delta().ln() * 200.0);
        if scroll != 0.0 {
            if let Some(p) = pointer {
                let before = to_graph(origin, doc.pan, zoom, p);
                let next = (zoom * (scroll * 0.0025).exp()).clamp(ZOOM_MIN, ZOOM_MAX);
                doc.zoom = next;
                doc.pan = [p.x - origin.x - before[0] * next, p.y - origin.y - before[1] * next];
            }
        }
    }
    if response.drag_started() || response.clicked() || response.secondary_clicked() {
        out.touched = true;
    }
    if response.drag_started_by(egui::PointerButton::Middle) {
        state.drag = Drag::Pan;
    } else if response.drag_started_by(egui::PointerButton::Primary) {
        let press = ui.input(|i| i.pointer.press_origin()).or(pointer).unwrap_or(rect.center());
        state.menu = None;
        if let Some(key) = pin_hit(press) {
            let ty = find_pin(&views, &key).and_then(|(_, _, p)| p.ty);
            state.drag = Drag::Wire { from: key, ty };
        } else if let Some(node) = node_hit(press) {
            doc.selected = Some(node);
            let at = to_graph(origin, doc.pan, zoom, press);
            let pos = doc.pos(node);
            doc.begin_drag();
            state.drag = Drag::Node { id: node, grab: Vec2::new(at[0] - pos[0], at[1] - pos[1]) };
        } else {
            state.drag = Drag::Pan;
        }
    }
    if response.dragged() {
        match &state.drag {
            Drag::Pan => {
                let d = response.drag_delta();
                doc.pan = [doc.pan[0] + d.x, doc.pan[1] + d.y];
            }
            Drag::Node { id: node, grab } => {
                if let Some(p) = pointer {
                    let at = to_graph(origin, doc.pan, zoom, p);
                    // Snap to the grid so layouts stay tidy.
                    let snapped = [
                        ((at[0] - grab.x) / GRID * 2.0).round() * GRID / 2.0,
                        ((at[1] - grab.y) / GRID * 2.0).round() * GRID / 2.0,
                    ];
                    doc.move_node(*node, snapped);
                }
            }
            _ => {}
        }
    }
    if response.drag_stopped() {
        match std::mem::take(&mut state.drag) {
            Drag::Node { .. } => doc.end_drag(),
            Drag::Wire { from, .. } => {
                let target = pointer.and_then(pin_hit);
                match target {
                    Some(to) if to != from => {
                        if let Err(e) = doc
                            .connect((from.node, &from.pin, from.side), (to.node, &to.pin, to.side))
                        {
                            out.refused = Some(e);
                        }
                    }
                    Some(_) => {}
                    None => {
                        // Dropped on empty space: offer a node to link to.
                        if let Some(p) = pointer {
                            state.menu = Some(Menu::Add {
                                screen: p,
                                at: to_graph(origin, doc.pan, zoom, p),
                                wire: Some(from),
                                search: String::new(),
                                fresh: true,
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if response.clicked() {
        state.menu = None;
        if let Some(p) = pointer {
            let alt = ui.input(|i| i.modifiers.alt);
            match (pin_hit(p), node_hit(p)) {
                (Some(key), _) if alt => {
                    doc.disconnect(key.node, &key.pin, key.side);
                }
                (_, Some(node)) => doc.selected = Some(node),
                _ => doc.selected = None,
            }
        }
    }
    if response.secondary_clicked() {
        if let Some(p) = pointer {
            match (pin_hit(p), node_hit(p)) {
                (Some(key), _) => {
                    doc.disconnect(key.node, &key.pin, key.side);
                    state.menu = None;
                }
                (None, Some(node)) => {
                    doc.selected = Some(node);
                    state.menu = Some(Menu::Node { screen: p, id: node });
                }
                (None, None) => {
                    state.menu = Some(Menu::Add {
                        screen: p,
                        at: to_graph(origin, doc.pan, zoom, p),
                        wire: None,
                        search: String::new(),
                        fresh: true,
                    });
                }
            }
        }
    }
    if views.is_empty() && state.menu.is_none() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text.hint,
            egui::FontId::proportional(13.0),
            crisp::mix(ink, Color32::GRAY, 0.4),
        );
    }

    // ---- Menus -----------------------------------------------------------
    let mut close = false;
    match &mut state.menu {
        Some(Menu::Add { screen, at, wire, search, fresh }) => {
            let area = egui::Area::new(id.with("add-menu"))
                .order(egui::Order::Foreground)
                .fixed_pos(*screen)
                .constrain(true);
            let inner = area.show(ui.ctx(), |ui| {
                let frame = egui::Frame::NONE
                    .fill(v.panel_fill)
                    .stroke(Stroke::new(1.0, Color32::from_black_alpha(200)))
                    .inner_margin(6.0);
                frame
                    .show(ui, |ui| {
                        ui.set_width(240.0);
                        let edit = egui::TextEdit::singleline(search)
                            .hint_text(text.search)
                            .desired_width(228.0);
                        let er = crisp::well_edit(ui, edit);
                        if *fresh {
                            er.request_focus();
                            *fresh = false;
                        }
                        let needle = search.trim().to_lowercase();
                        let matches = doc.palette(&needle);
                        let enter =
                            er.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        let mut pick =
                            if enter { matches.first().map(|row| row.kind.clone()) } else { None };
                        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                            for row in &matches {
                                let hit =
                                    crisp::menu_row(ui, None, &row.label).on_hover_text(&row.about);
                                if hit.clicked() {
                                    pick = Some(row.kind.clone());
                                }
                            }
                        });
                        pick
                    })
                    .inner
            });
            if let Some(kind) = inner.inner {
                if let Some(new_id) = doc.add_node(&kind, *at) {
                    doc.selected = Some(new_id);
                    if let Some(from) = wire.take() {
                        link_first_fitting(doc, &from, new_id);
                    }
                }
                close = true;
            }
            let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
            let outside = ui.input(|i| i.pointer.any_pressed())
                && pointer.is_some_and(|p| !inner.response.rect.contains(p))
                && !response.secondary_clicked()
                && !response.drag_stopped();
            close |= escape || outside;
        }
        Some(Menu::Node { screen, id: node }) => {
            let node = *node;
            let area = egui::Area::new(id.with("node-menu"))
                .order(egui::Order::Foreground)
                .fixed_pos(*screen)
                .constrain(true);
            let inner = area.show(ui.ctx(), |ui| {
                egui::Frame::NONE
                    .fill(v.panel_fill)
                    .stroke(Stroke::new(1.0, Color32::from_black_alpha(200)))
                    .inner_margin(4.0)
                    .show(ui, |ui| {
                        ui.set_width(170.0);
                        let mut done = false;
                        if crisp::menu_row(ui, None, text.disconnect).clicked() {
                            let names: Vec<(String, PinSide)> = views
                                .iter()
                                .find(|v| v.id == node)
                                .map(|v| {
                                    v.ins
                                        .iter()
                                        .chain(&v.outs)
                                        .map(|p| (p.name.clone(), p.side))
                                        .collect()
                                })
                                .unwrap_or_default();
                            for (pin, side) in names {
                                doc.disconnect(node, &pin, side);
                            }
                            done = true;
                        }
                        if crisp::menu_row(
                            ui,
                            Some(doc.breakpoints.contains(&node)),
                            text.breakpoint,
                        )
                        .clicked()
                        {
                            doc.toggle_breakpoint(node);
                            done = true;
                        }
                        if crisp::menu_row(ui, None, text.delete_node).clicked() {
                            doc.remove_node(node);
                            done = true;
                        }
                        done
                    })
                    .inner
            });
            let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
            let outside = ui.input(|i| i.pointer.any_pressed())
                && pointer.is_some_and(|p| !inner.response.rect.contains(p))
                && !response.secondary_clicked();
            close = inner.inner || escape || outside;
        }
        None => {}
    }
    if close {
        state.menu = None;
    }
    ui.data_mut(|d| d.insert_temp(id, state));
    out
}

/// Link `from` to the first pin of node `new_id` that accepts it.
fn link_first_fitting(doc: &mut GraphDoc, from: &PinKey, new_id: u32) {
    let Some(view) = doc.views().into_iter().find(|v| v.id == new_id) else { return };
    let side = match from.side {
        PinSide::Out => PinSide::In,
        PinSide::In => PinSide::Out,
    };
    for pin in pins(&view, side) {
        if doc.connect((from.node, &from.pin, from.side), (new_id, &pin.name, side)).is_ok() {
            return;
        }
    }
}

/// Pan and zoom so every node fits inside a canvas of `size`.
pub fn frame_all(doc: &mut GraphDoc, size: Vec2) {
    let views = doc.views();
    let Some(first) = views.first() else {
        doc.pan = [40.0, 40.0];
        doc.zoom = 1.0;
        return;
    };
    let mut bounds = Rect::from_min_size(Pos2::new(first.pos[0], first.pos[1]), node_size(first));
    for view in &views {
        bounds =
            bounds.union(Rect::from_min_size(Pos2::new(view.pos[0], view.pos[1]), node_size(view)));
    }
    let bounds = bounds.expand(40.0);
    let zoom = (size.x / bounds.width()).min(size.y / bounds.height()).clamp(ZOOM_MIN, 1.25);
    doc.zoom = zoom;
    doc.pan = [
        (size.x - bounds.width() * zoom) * 0.5 - bounds.min.x * zoom,
        (size.y - bounds.height() * zoom) * 0.5 - bounds.min.y * zoom,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_and_graph_round_trip() {
        let origin = Pos2::new(10.0, 20.0);
        let g =
            to_graph(origin, [5.0, -3.0], 1.5, to_screen(origin, [5.0, -3.0], 1.5, [42.0, 7.0]));
        assert!((g[0] - 42.0).abs() < 1e-4 && (g[1] - 7.0).abs() < 1e-4);
    }

    #[test]
    fn every_type_has_its_own_pin_color() {
        let tys = [
            None,
            Some(Ty::Bool),
            Some(Ty::Int),
            Some(Ty::Float),
            Some(Ty::Vec2),
            Some(Ty::Vec3),
            Some(Ty::Str),
            Some(Ty::Entity),
        ];
        let mut colors: Vec<Color32> = tys.iter().map(|t| pin_color(*t)).collect();
        colors.dedup();
        colors.sort_by_key(|c| c.to_array());
        colors.dedup();
        assert_eq!(colors.len(), tys.len());
    }
}
