//! State machine view: states as boxes, transitions as arrows, one side form.
//!
//! The canvas edits a `GraphDoc` directly. Each change is one undo step and is
//! written to the graph file, the same way node edits are.

use crate::crisp::{self, CrispCombo};
use crate::graph_canvas::{to_graph, to_screen, ZOOM_MAX, ZOOM_MIN};
use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};
use seal_editor_core::graph_doc::{Cond, GraphDoc, MachineSel};

const BOX_W: f32 = 160.0;
const BOX_H: f32 = 72.0;
const GRID: f32 = 16.0;
const GOLD: Color32 = Color32::from_rgb(255, 214, 64);

/// Words the state canvas and its form show, already translated.
pub struct MachineText<'a> {
    pub state_name: &'a str,
    pub initial: &'a str,
    pub on_enter: &'a str,
    pub on_update: &'a str,
    pub on_exit: &'a str,
    pub delete_state: &'a str,
    pub delete_transition: &'a str,
    pub from: &'a str,
    pub to: &'a str,
    pub priority: &'a str,
    pub condition: &'a str,
    pub cond_flag: &'a str,
    pub cond_compare: &'a str,
    pub cond_event: &'a str,
    pub cond_after: &'a str,
    pub seconds: &'a str,
    pub variable: &'a str,
    pub event_name: &'a str,
    pub compare_value: &'a str,
    pub hint: &'a str,
    pub compare_op: &'a str,
}

/// What happened on the state canvas this frame.
#[derive(Default)]
pub struct MachineOutput {
    pub touched: bool,
    pub refused: Option<String>,
}

#[derive(Clone, Debug, Default)]
enum Drag {
    #[default]
    None,
    Pan,
    State {
        index: usize,
        grab: Vec2,
    },
    Wire {
        from: usize,
    },
}

/// Pan and zoom so every state fits inside a canvas of `size`.
pub fn frame_states(doc: &mut GraphDoc, size: Vec2) {
    let Some(machine) = doc.graph.machine.clone() else {
        doc.pan = [40.0, 40.0];
        doc.zoom = 1.0;
        return;
    };
    let Some(first) = machine.states.first() else {
        doc.pan = [40.0, 40.0];
        doc.zoom = 1.0;
        return;
    };
    let mut bounds =
        Rect::from_min_size(Pos2::new(first.pos[0], first.pos[1]), Vec2::new(BOX_W, BOX_H));
    for state in &machine.states {
        bounds = bounds.union(Rect::from_min_size(
            Pos2::new(state.pos[0], state.pos[1]),
            Vec2::new(BOX_W, BOX_H),
        ));
    }
    let bounds = bounds.expand(40.0);
    let zoom = (size.x / bounds.width()).min(size.y / bounds.height()).clamp(ZOOM_MIN, 1.25);
    doc.zoom = zoom;
    doc.pan = [
        (size.x - bounds.width() * zoom) * 0.5 - bounds.min.x * zoom,
        (size.y - bounds.height() * zoom) * 0.5 - bounds.min.y * zoom,
    ];
}

/// Draw the state canvas and handle its input.
pub fn state_machine_canvas(
    ui: &mut Ui,
    doc: &mut GraphDoc,
    text: &MachineText<'_>,
) -> MachineOutput {
    let mut out = MachineOutput::default();
    let id = ui.id().with("state-canvas").with(&doc.label);
    let rect = ui.available_rect_before_wrap();
    let response = ui.interact(rect, id, Sense::click_and_drag());
    ui.allocate_rect(rect, Sense::hover());
    let mut drag: Drag = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals().clone();
    let origin = rect.min;
    let zoom = doc.zoom.clamp(ZOOM_MIN, ZOOM_MAX);
    let pointer = ui.input(|i| i.pointer.hover_pos());
    painter.rect_filled(rect, 0.0, crisp::mix(visuals.extreme_bg_color, Color32::BLACK, 0.25));

    let machine = doc.graph.machine.clone();
    let Some(machine) = machine else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text.hint,
            egui::FontId::proportional(14.0),
            visuals.weak_text_color(),
        );
        if response.secondary_clicked() {
            out.touched = true;
            let at = pointer.map(|p| to_graph(origin, doc.pan, zoom, p)).unwrap_or([40.0, 40.0]);
            doc.add_state(at);
        }
        ui.data_mut(|d| d.insert_temp(id, drag));
        return out;
    };

    let current = doc.debug.as_ref().map(|d| d.state.clone()).unwrap_or_default();
    let screen_box = |pan: [f32; 2], pos: [f32; 2]| {
        let min = to_screen(origin, pan, zoom, pos);
        Rect::from_min_size(min, Vec2::new(BOX_W, BOX_H) * zoom)
    };
    let port_rect = |pan: [f32; 2], pos: [f32; 2]| {
        let box_r = screen_box(pan, pos);
        Rect::from_center_size(box_r.right_center(), Vec2::splat((12.0 * zoom).max(10.0)))
    };
    let state_at = |pan: [f32; 2], p: Pos2| -> Option<usize> {
        machine
            .states
            .iter()
            .enumerate()
            .rev()
            .find(|(_, s)| screen_box(pan, s.pos).contains(p))
            .map(|(i, _)| i)
    };
    let port_at = |pan: [f32; 2], p: Pos2| -> Option<usize> {
        machine
            .states
            .iter()
            .enumerate()
            .rev()
            .find(|(_, s)| port_rect(pan, s.pos).contains(p))
            .map(|(i, _)| i)
    };

    for (ti, t) in machine.transitions.iter().enumerate() {
        let (Some(from), Some(to)) = (
            machine.states.iter().find(|s| s.name == t.from),
            machine.states.iter().find(|s| s.name == t.to),
        ) else {
            continue;
        };
        let pts = arrow_points(from.pos, to.pos, zoom, origin, doc.pan);
        let selected = doc.machine_pick == Some(MachineSel::Transition(ti));
        let color = if selected {
            Color32::from_rgb(120, 170, 255)
        } else {
            Color32::from_rgb(180, 186, 196)
        };
        painter.add(Shape::CubicBezier(egui::epaint::CubicBezierShape::from_points_stroke(
            pts,
            false,
            Color32::TRANSPARENT,
            Stroke::new(if selected { 2.5 } else { 1.5 }, color),
        )));
        let end = pts[3];
        let raw = end - pts[2];
        let dir = if raw.length_sq() < 0.01 { Vec2::new(1.0, 0.0) } else { raw.normalized() };
        let side = Vec2::new(-dir.y, dir.x);
        painter.add(Shape::convex_polygon(
            vec![end, end - dir * 9.0 + side * 4.5, end - dir * 9.0 - side * 4.5],
            color,
            Stroke::NONE,
        ));
        let mid = bezier(pts, 0.5);
        painter.text(
            mid,
            egui::Align2::CENTER_BOTTOM,
            doc.transition_label(ti),
            egui::FontId::proportional(11.0),
            color,
        );
    }

    for (i, state) in machine.states.iter().enumerate() {
        let box_r = screen_box(doc.pan, state.pos);
        let selected = doc.machine_pick == Some(MachineSel::State(i));
        let live = state.name == current;
        let stroke = if live {
            Stroke::new(2.0, GOLD)
        } else if selected {
            Stroke::new(2.0, Color32::from_rgb(120, 170, 255))
        } else {
            Stroke::new(1.0, Color32::from_rgb(90, 96, 108))
        };
        painter.rect_filled(box_r, 0.0, Color32::from_rgb(36, 40, 48));
        painter.rect_stroke(box_r, 0.0, stroke, egui::StrokeKind::Inside);
        let header =
            box_r.intersect(Rect::from_min_size(box_r.min, Vec2::new(box_r.width(), 22.0 * zoom)));
        painter.rect_filled(header, 0.0, Color32::from_rgb(48, 72, 110));
        painter.text(
            header.left_center() + Vec2::new(8.0, 0.0),
            egui::Align2::LEFT_CENTER,
            &state.name,
            egui::FontId::proportional((13.0 * zoom).clamp(9.0, 16.0)),
            Color32::WHITE,
        );
        if machine.initial == state.name {
            painter.text(
                box_r.left_top() + Vec2::new(8.0, 28.0 * zoom),
                egui::Align2::LEFT_TOP,
                text.initial,
                egui::FontId::proportional(11.0),
                GOLD,
            );
        }
        let port = port_rect(doc.pan, state.pos);
        painter.rect_filled(port, 0.0, Color32::from_rgb(222, 226, 232));
    }

    if let Drag::Wire { from } = drag {
        if let (Some(state), Some(p)) = (machine.states.get(from), pointer) {
            let a = port_rect(doc.pan, state.pos).center();
            painter.line_segment([a, p], Stroke::new(1.5, Color32::from_rgb(180, 186, 196)));
        }
    }

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
        drag = Drag::Pan;
    } else if response.drag_started_by(egui::PointerButton::Primary) {
        let press = ui.input(|i| i.pointer.press_origin()).or(pointer).unwrap_or(rect.center());
        if let Some(from) = port_at(doc.pan, press) {
            drag = Drag::Wire { from };
        } else if let Some(index) = state_at(doc.pan, press) {
            doc.machine_pick = Some(MachineSel::State(index));
            let at = to_graph(origin, doc.pan, zoom, press);
            let pos = machine.states[index].pos;
            doc.begin_drag();
            drag = Drag::State { index, grab: Vec2::new(at[0] - pos[0], at[1] - pos[1]) };
        } else if let Some(ti) =
            transition_at(&machine.states, &machine.transitions, origin, doc.pan, zoom, press)
        {
            doc.machine_pick = Some(MachineSel::Transition(ti));
            drag = Drag::None;
        } else {
            doc.machine_pick = None;
            drag = Drag::Pan;
        }
    }
    if response.dragged() {
        match drag {
            Drag::Pan => {
                let d = response.drag_delta();
                doc.pan = [doc.pan[0] + d.x, doc.pan[1] + d.y];
            }
            Drag::State { index, grab } => {
                if let Some(p) = pointer {
                    let at = to_graph(origin, doc.pan, zoom, p);
                    let snapped = [
                        ((at[0] - grab.x) / GRID * 2.0).round() * GRID / 2.0,
                        ((at[1] - grab.y) / GRID * 2.0).round() * GRID / 2.0,
                    ];
                    doc.move_state(index, snapped);
                }
            }
            Drag::Wire { .. } | Drag::None => {}
        }
    }
    if response.drag_stopped() {
        match std::mem::take(&mut drag) {
            Drag::State { .. } => doc.end_drag(),
            Drag::Wire { from } => {
                if let Some(p) = pointer {
                    let pan = doc.pan;
                    if let Some(to) = state_at(pan, p).or_else(|| port_at(pan, p)) {
                        doc.add_transition(from, to);
                    }
                }
            }
            Drag::Pan | Drag::None => {}
        }
    }
    if response.clicked() {
        if let Some(p) = pointer {
            if port_at(doc.pan, p).is_none() {
                if let Some(index) = state_at(doc.pan, p) {
                    doc.machine_pick = Some(MachineSel::State(index));
                } else if let Some(ti) =
                    transition_at(&machine.states, &machine.transitions, origin, doc.pan, zoom, p)
                {
                    doc.machine_pick = Some(MachineSel::Transition(ti));
                } else {
                    doc.machine_pick = None;
                }
            }
        }
    }
    if response.secondary_clicked() {
        if let Some(p) = pointer {
            if let Some(index) = state_at(doc.pan, p) {
                doc.machine_pick = Some(MachineSel::State(index));
            } else {
                doc.add_state(to_graph(origin, doc.pan, zoom, p));
            }
        }
    }
    let delete = response.hovered()
        && !ui.ctx().egui_wants_keyboard_input()
        && ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));
    if delete {
        match doc.machine_pick {
            Some(MachineSel::State(i)) => {
                doc.delete_state(i);
            }
            Some(MachineSel::Transition(i)) => {
                doc.delete_transition(i);
            }
            None => {}
        }
    }

    ui.data_mut(|d| d.insert_temp(id, drag));
    out
}

/// The side form for the selected state or transition. Sits under the canvas.
pub fn state_machine_form(
    ui: &mut Ui,
    doc: &mut GraphDoc,
    text: &MachineText<'_>,
) -> Option<String> {
    let Some(pick) = doc.machine_pick else {
        ui.label(egui::RichText::new(text.hint).color(ui.visuals().weak_text_color()));
        return None;
    };
    match pick {
        MachineSel::State(index) => state_form(ui, doc, text, index),
        MachineSel::Transition(index) => transition_form(ui, doc, text, index),
    }
}

fn state_form(
    ui: &mut Ui,
    doc: &mut GraphDoc,
    text: &MachineText<'_>,
    index: usize,
) -> Option<String> {
    let machine = doc.graph.machine.clone()?;
    let state = machine.states.get(index)?;
    let name = state.name.clone();
    let mut refused = None;
    ui.horizontal_wrapped(|ui| {
        ui.label(text.state_name);
        let fid = ui.id().with(("state-name", index, &doc.label));
        let mut buffer: String = ui.data(|d| d.get_temp(fid)).unwrap_or_else(|| name.clone());
        let edit = egui::TextEdit::singleline(&mut buffer).id(fid).desired_width(120.0);
        let resp = crisp::well_edit(ui, edit);
        if resp.has_focus() {
            ui.data_mut(|d| d.insert_temp(fid, buffer));
        } else {
            if resp.lost_focus() && buffer != name {
                if let Err(e) = doc.rename_state(index, &buffer) {
                    refused = Some(e);
                }
            }
            ui.data_mut(|d| d.remove::<String>(fid));
        }
        let is_initial = doc.graph.machine.as_ref().is_some_and(|m| m.initial == name);
        let mut on = is_initial;
        if ui.checkbox(&mut on, text.initial).clicked() && on && !is_initial {
            doc.set_initial_state(index);
        }
        if crisp::button(ui, egui::Button::new(text.on_enter), true, false).clicked() {
            doc.focus_state_event(index, "on_enter");
        }
        if crisp::button(ui, egui::Button::new(text.on_update), true, false).clicked() {
            doc.focus_state_event(index, "on_update");
        }
        if crisp::button(ui, egui::Button::new(text.on_exit), true, false).clicked() {
            doc.focus_state_event(index, "on_exit");
        }
        if crisp::button(ui, egui::Button::new(text.delete_state), true, false).clicked() {
            doc.delete_state(index);
        }
    });
    refused
}

fn transition_form(
    ui: &mut Ui,
    doc: &mut GraphDoc,
    text: &MachineText<'_>,
    index: usize,
) -> Option<String> {
    let machine = doc.graph.machine.clone()?;
    let t = machine.transitions.get(index)?.clone();
    let names: Vec<String> = machine.states.iter().map(|s| s.name.clone()).collect();
    let mut refused = None;
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("{} {}", text.from, t.from));
        ui.label(text.to);
        let mut chosen = None;
        egui::ComboBox::from_id_salt(("state-to", index, &doc.label))
            .selected_text(&t.to)
            .width(110.0)
            .show_crisp(ui, |ui| {
                for (i, name) in names.iter().enumerate() {
                    if ui.selectable_label(name == &t.to, name).clicked() {
                        chosen = Some(i);
                    }
                }
            });
        if let Some(i) = chosen {
            doc.set_transition_to(index, i);
        }
        ui.label(text.priority);
        let fid = ui.id().with(("state-priority", index, &doc.label));
        let current = t.priority.to_string();
        let mut buffer: String = ui.data(|d| d.get_temp(fid)).unwrap_or_else(|| current.clone());
        let edit = egui::TextEdit::singleline(&mut buffer).id(fid).desired_width(48.0);
        let resp = crisp::well_edit(ui, edit);
        if resp.has_focus() {
            ui.data_mut(|d| d.insert_temp(fid, buffer));
        } else {
            if resp.lost_focus() && buffer != current {
                match buffer.trim().parse::<i32>() {
                    Ok(priority) => {
                        doc.set_transition_priority(index, priority);
                    }
                    Err(_) => refused = Some("priority must be a whole number".into()),
                }
            }
            ui.data_mut(|d| d.remove::<String>(fid));
        }
        ui.label(text.condition);
        let kind = cond_kind(&t.cond);
        let kind_label = match kind {
            "flag" => text.cond_flag,
            "compare" => text.cond_compare,
            "event" => text.cond_event,
            _ => text.cond_after,
        };
        let mut next_kind = None;
        egui::ComboBox::from_id_salt(("state-cond", index, &doc.label))
            .selected_text(kind_label)
            .width(100.0)
            .show_crisp(ui, |ui| {
                for (id, label) in [
                    ("flag", text.cond_flag),
                    ("compare", text.cond_compare),
                    ("event", text.cond_event),
                    ("after", text.cond_after),
                ] {
                    if ui.selectable_label(id == kind, label).clicked() {
                        next_kind = Some(id);
                    }
                }
            });
        if let Some(id) = next_kind.filter(|id| *id != kind) {
            if let Err(e) = apply_kind(doc, index, id, &t) {
                refused = Some(e);
            }
        }
        if let Err(e) = cond_fields(ui, doc, text, index, &t.cond) {
            refused = Some(e);
        }
        if crisp::button(ui, egui::Button::new(text.delete_transition), true, false).clicked() {
            doc.delete_transition(index);
        }
    });
    refused
}

fn cond_kind(cond: &Cond) -> &'static str {
    match cond {
        Cond::BoolVar(_) => "flag",
        Cond::Compare { .. } => "compare",
        Cond::Event(_) => "event",
        Cond::After(_) => "after",
    }
}

fn apply_kind(
    doc: &mut GraphDoc,
    index: usize,
    kind: &str,
    current: &seal_editor_core::graph_doc::Transition,
) -> Result<(), String> {
    match kind {
        "flag" => {
            let var = match &current.cond {
                Cond::BoolVar(name) => name.clone(),
                _ => "go".into(),
            };
            doc.set_cond_flag(index, &var)
        }
        "compare" => {
            let (var, op, literal) = match &current.cond {
                Cond::Compare { var, op, .. } => {
                    (var.clone(), op.name().to_string(), doc.compare_literal(index))
                }
                _ => ("speed".into(), "gt".into(), "0".into()),
            };
            doc.set_cond_compare(index, &var, &op, &literal)
        }
        "event" => {
            let name = match &current.cond {
                Cond::Event(name) => name.clone(),
                _ => "go".into(),
            };
            doc.set_cond_event(index, &name)
        }
        _ => doc.set_cond_after(index, 1.0),
    }
}

fn cond_fields(
    ui: &mut Ui,
    doc: &mut GraphDoc,
    text: &MachineText<'_>,
    index: usize,
    cond: &Cond,
) -> Result<(), String> {
    match cond {
        Cond::BoolVar(var) => {
            ui.label(text.variable);
            commit_text(ui, ("flag", index), var, |doc, value| doc.set_cond_flag(index, value))(doc)
        }
        Cond::Compare { var, op, .. } => {
            ui.label(text.variable);
            let var_name = var.clone();
            let op_name = op.name().to_string();
            let literal = doc.compare_literal(index);
            commit_text(ui, ("cmp-var", index), &var_name, {
                let op_name = op_name.clone();
                let literal = literal.clone();
                move |doc, value| doc.set_cond_compare(index, value, &op_name, &literal)
            })(doc)?;
            ui.label(text.compare_op);
            let mut chosen = None;
            egui::ComboBox::from_id_salt(("cmp-op", index))
                .selected_text(op_symbol(*op))
                .width(52.0)
                .show_crisp(ui, |ui| {
                    for (name, symbol) in [
                        ("lt", "<"),
                        ("le", "<="),
                        ("gt", ">"),
                        ("ge", ">="),
                        ("eq", "="),
                        ("ne", "!="),
                    ] {
                        if ui.selectable_label(name == op.name(), symbol).clicked() {
                            chosen = Some(name);
                        }
                    }
                });
            if let Some(name) = chosen {
                doc.set_cond_compare(index, &var_name, name, &literal)?;
            }
            ui.label(text.compare_value);
            commit_text(ui, ("cmp-value", index), &literal, {
                let var_name = var_name.clone();
                let op_name = op_name.clone();
                move |doc, value| doc.set_cond_compare(index, &var_name, &op_name, value)
            })(doc)
        }
        Cond::Event(name) => {
            ui.label(text.event_name);
            commit_text(ui, ("event", index), name, |doc, value| doc.set_cond_event(index, value))(
                doc,
            )
        }
        Cond::After(seconds) => {
            ui.label(text.seconds);
            let shown = format!("{seconds}");
            commit_text(ui, ("after", index), &shown, |doc, value| {
                let seconds: f64 =
                    value.trim().parse().map_err(|_| "seconds must be a number".to_string())?;
                doc.set_cond_after(index, seconds)
            })(doc)
        }
    }
}

fn commit_text<'a, F>(
    ui: &mut Ui,
    salt: (&'static str, usize),
    current: &str,
    apply: F,
) -> impl FnOnce(&mut GraphDoc) -> Result<(), String> + 'a
where
    F: FnOnce(&mut GraphDoc, &str) -> Result<(), String> + 'a,
{
    let fid = ui.id().with(salt);
    let mut buffer: String = ui.data(|d| d.get_temp(fid)).unwrap_or_else(|| current.to_string());
    let edit = egui::TextEdit::singleline(&mut buffer).id(fid).desired_width(90.0);
    let resp = crisp::well_edit(ui, edit);
    let lost = resp.lost_focus() && buffer != current;
    let pending = if resp.has_focus() {
        ui.data_mut(|d| d.insert_temp(fid, buffer));
        None
    } else {
        ui.data_mut(|d| d.remove::<String>(fid));
        lost.then_some(buffer)
    };
    move |doc| {
        if let Some(value) = pending {
            apply(doc, &value)
        } else {
            Ok(())
        }
    }
}

fn op_symbol(op: seal_editor_core::graph_doc::CmpOp) -> &'static str {
    match op {
        seal_editor_core::graph_doc::CmpOp::Lt => "<",
        seal_editor_core::graph_doc::CmpOp::Le => "<=",
        seal_editor_core::graph_doc::CmpOp::Gt => ">",
        seal_editor_core::graph_doc::CmpOp::Ge => ">=",
        seal_editor_core::graph_doc::CmpOp::Eq => "=",
        seal_editor_core::graph_doc::CmpOp::Ne => "!=",
    }
}

fn arrow_points(from: [f32; 2], to: [f32; 2], zoom: f32, origin: Pos2, pan: [f32; 2]) -> [Pos2; 4] {
    let at = |p: [f32; 2]| to_screen(origin, pan, zoom, p);
    if from == to {
        let right = [from[0] + BOX_W, from[1] + BOX_H * 0.5];
        let top = [from[0] + BOX_W * 0.5, from[1]];
        [at(right), at([right[0] + 48.0, right[1] - 64.0]), at([top[0], top[1] - 56.0]), at(top)]
    } else {
        let a = [from[0] + BOX_W, from[1] + BOX_H * 0.5];
        let b = [to[0], to[1] + BOX_H * 0.5];
        [at(a), at([a[0] + 64.0, a[1]]), at([b[0] - 64.0, b[1]]), at(b)]
    }
}

fn bezier(p: [Pos2; 4], t: f32) -> Pos2 {
    let u = 1.0 - t;
    let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
    Pos2::new(
        p[0].x * w[0] + p[1].x * w[1] + p[2].x * w[2] + p[3].x * w[3],
        p[0].y * w[0] + p[1].y * w[1] + p[2].y * w[2] + p[3].y * w[3],
    )
}

fn transition_at(
    states: &[seal_editor_core::graph_doc::StateDef],
    transitions: &[seal_editor_core::graph_doc::Transition],
    origin: Pos2,
    pan: [f32; 2],
    zoom: f32,
    p: Pos2,
) -> Option<usize> {
    let mut best = None;
    let mut best_d = 8.0_f32;
    for (i, t) in transitions.iter().enumerate() {
        let (Some(from), Some(to)) =
            (states.iter().find(|s| s.name == t.from), states.iter().find(|s| s.name == t.to))
        else {
            continue;
        };
        let pts = arrow_points(from.pos, to.pos, zoom, origin, pan);
        for step in 0..24 {
            let d = bezier(pts, step as f32 / 23.0).distance(p);
            if d < best_d {
                best_d = d;
                best = Some(i);
            }
        }
    }
    best
}
