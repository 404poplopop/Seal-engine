//! Size, alpha and colour curves on an effect, in the Inspector and the Asset Editor.

use seal_editor_core::asset_editor::EmitterDef;
use seal_editor_core::curve_text::{curve_to_text, gradient_to_text};
use seal_editor_core::{CommandArgs, EditorSession};
use seal_ui::{
    curve_row, gradient_row, CurveFieldEdit, CurveUiText, GradientFieldEdit, GradientUiText,
    PanelFrame,
};

use crate::PanelEvent;

/// Curve and gradient rows for the effect at `rel` (project-relative `.fx` path).
pub fn effect_curves(frame: &mut PanelFrame, session: &EditorSession, rel: &str) -> PanelEvent {
    let Some(root) = session.project.as_ref().map(|project| project.root.clone()) else {
        return PanelEvent::None;
    };
    let Ok(raw) = std::fs::read_to_string(root.join(rel)) else {
        return PanelEvent::None;
    };
    let Ok(def) = EmitterDef::parse(&raw) else {
        return PanelEvent::None;
    };
    let labels = CurveLabels::from_session(session);
    let mut event = PanelEvent::None;
    event = take(event, size_row(frame, session, rel, &def, &labels));
    event = take(event, alpha_row(frame, session, rel, &def, &labels));
    event = take(event, colour_row(frame, session, rel, &def, &labels));
    event = take(
        event,
        optional_curve_row(frame, session, rel, &def, "velocity_curve", &labels.velocity),
    );
    event = take(
        event,
        optional_curve_row(frame, session, rel, &def, "rotation_curve", &labels.rotation),
    );
    event
}

struct CurveLabels {
    delete_key: String,
    time: String,
    value: String,
    in_tangent: String,
    out_tangent: String,
    easing: String,
    constant: String,
    linear: String,
    ease_in: String,
    ease_out: String,
    ease_in_out: String,
    tangent: String,
    hint: String,
    reset: String,
    size: String,
    alpha: String,
    colour: String,
    gradient_delete: String,
    gradient_alpha: String,
    gradient_hint: String,
    gradient_stop: String,
    velocity: String,
    rotation: String,
}

impl CurveLabels {
    fn from_session(session: &EditorSession) -> Self {
        let g = |id: &str| session.messages.get(id);
        Self {
            delete_key: g("asset-curve-delete"),
            time: g("asset-curve-time"),
            value: g("asset-curve-value"),
            in_tangent: g("asset-curve-in"),
            out_tangent: g("asset-curve-out"),
            easing: g("asset-curve-easing"),
            constant: g("asset-curve-constant"),
            linear: g("asset-curve-linear"),
            ease_in: g("asset-curve-ease-in"),
            ease_out: g("asset-curve-ease-out"),
            ease_in_out: g("asset-curve-ease-in-out"),
            tangent: g("asset-curve-tangent"),
            hint: g("asset-curve-hint"),
            reset: g("asset-curve-reset"),
            size: g("asset-curve-size"),
            alpha: g("asset-curve-alpha"),
            colour: g("asset-gradient-colour"),
            gradient_delete: g("asset-gradient-delete"),
            gradient_alpha: g("asset-gradient-alpha"),
            gradient_hint: g("asset-gradient-hint"),
            gradient_stop: g("asset-gradient-stop"),
            velocity: g("asset-curve-velocity"),
            rotation: g("asset-curve-rotation"),
        }
    }

    fn curve(&self) -> CurveUiText<'_> {
        CurveUiText {
            delete_key: &self.delete_key,
            time: &self.time,
            value: &self.value,
            in_tangent: &self.in_tangent,
            out_tangent: &self.out_tangent,
            easing: &self.easing,
            constant: &self.constant,
            linear: &self.linear,
            ease_in: &self.ease_in,
            ease_out: &self.ease_out,
            ease_in_out: &self.ease_in_out,
            tangent: &self.tangent,
            hint: &self.hint,
            reset: &self.reset,
        }
    }

    fn gradient(&self) -> GradientUiText<'_> {
        GradientUiText {
            delete_stop: &self.gradient_delete,
            colour: &self.gradient_stop,
            alpha: &self.gradient_alpha,
            hint: &self.gradient_hint,
            reset: &self.reset,
        }
    }
}

fn take(current: PanelEvent, next: PanelEvent) -> PanelEvent {
    if matches!(next, PanelEvent::None) {
        current
    } else {
        next
    }
}

fn problem(frame: &mut PanelFrame, session: &EditorSession, def: &EmitterDef, key: &str) {
    if let Some(error) = def.curve_problem(key) {
        frame.muted(&session.messages.format("asset-curve-damaged", &[("error", error.as_str())]));
    }
}

fn size_row(
    frame: &mut PanelFrame,
    session: &EditorSession,
    rel: &str,
    def: &EmitterDef,
    labels: &CurveLabels,
) -> PanelEvent {
    problem(frame, session, def, "size_curve");
    let curve = def.display_size_curve();
    let edit = curve_row(
        frame.ui_mut(),
        &format!("{rel}:size"),
        &labels.size,
        &curve,
        def.size_curve.is_some(),
        Some((0.0, 1.0)),
        &labels.curve(),
    );
    curve_edit(session, rel, "size_curve", &edit, &curve_to_text(&edit.curve))
}

fn alpha_row(
    frame: &mut PanelFrame,
    session: &EditorSession,
    rel: &str,
    def: &EmitterDef,
    labels: &CurveLabels,
) -> PanelEvent {
    problem(frame, session, def, "alpha_curve");
    let curve = def.display_alpha_curve();
    let edit = curve_row(
        frame.ui_mut(),
        &format!("{rel}:alpha"),
        &labels.alpha,
        &curve,
        def.alpha_curve.is_some(),
        Some((0.0, 1.0)),
        &labels.curve(),
    );
    curve_edit(session, rel, "alpha_curve", &edit, &curve_to_text(&edit.curve))
}

fn optional_curve_row(
    frame: &mut PanelFrame,
    session: &EditorSession,
    rel: &str,
    def: &EmitterDef,
    key: &str,
    label: &str,
) -> PanelEvent {
    problem(frame, session, def, key);
    let stored = match key {
        "velocity_curve" => def.extras.velocity_curve.as_deref(),
        "rotation_curve" => def.extras.rotation_curve.as_deref(),
        _ => None,
    };
    let endpoints = (1.0, 1.0);
    let fallback = seal_editor_core::curve::Curve::linear(endpoints.0, endpoints.1);
    let curve = if let Some(text) = stored {
        let (c, err) = seal_editor_core::curve_text::load_curve(text);
        if err.is_none() {
            c
        } else {
            fallback.clone()
        }
    } else {
        fallback.clone()
    };
    let edit = curve_row(
        frame.ui_mut(),
        &format!("{rel}:{key}"),
        label,
        &curve,
        stored.is_some(),
        Some((0.0, 1.0)),
        &CurveLabels::from_session(session).curve(),
    );
    curve_edit(session, rel, key, &edit, &curve_to_text(&edit.curve))
}

fn colour_row(
    frame: &mut PanelFrame,
    session: &EditorSession,
    rel: &str,
    def: &EmitterDef,
    labels: &CurveLabels,
) -> PanelEvent {
    problem(frame, session, def, "color_gradient");
    let gradient = def.display_gradient();
    let edit = gradient_row(
        frame.ui_mut(),
        &format!("{rel}:colour"),
        &labels.colour,
        &gradient,
        def.color_gradient.is_some(),
        &labels.gradient(),
    );
    gradient_edit(session, rel, &edit, &gradient_to_text(&edit.gradient))
}

fn editing(session: &EditorSession, rel: &str, key: &str) -> bool {
    let mark = format!("{rel}\t{key}");
    session.draft(seal_editor_core::FX_FIELD_EDIT).as_deref() == Some(mark.as_str())
}

fn curve_edit(
    session: &EditorSession,
    rel: &str,
    key: &str,
    edit: &CurveFieldEdit,
    value: &str,
) -> PanelEvent {
    field_edit(
        session,
        rel,
        key,
        if edit.reset { "" } else { value },
        edit.changed || edit.reset,
        edit.drag_started,
        edit.drag_stopped,
        edit.escape_cancel,
        edit.commit_focus || edit.reset,
    )
}

fn gradient_edit(
    session: &EditorSession,
    rel: &str,
    edit: &GradientFieldEdit,
    value: &str,
) -> PanelEvent {
    field_edit(
        session,
        rel,
        "color_gradient",
        if edit.reset { "" } else { value },
        edit.changed,
        edit.drag_started,
        edit.drag_stopped,
        edit.escape_cancel,
        edit.commit_focus || edit.reset,
    )
}

#[allow(clippy::too_many_arguments)]
fn field_edit(
    session: &EditorSession,
    rel: &str,
    key: &str,
    value: &str,
    changed: bool,
    drag_started: bool,
    drag_stopped: bool,
    escape_cancel: bool,
    commit_focus: bool,
) -> PanelEvent {
    if escape_cancel {
        return command(rel, key, value, "cancel");
    }
    let in_edit = editing(session, rel, key);
    let needs_begin = !in_edit && (drag_started || changed);
    let mut cmds = Vec::new();
    if needs_begin {
        cmds.push(pair(rel, key, value, "begin"));
    }
    if changed {
        let phase = if in_edit || needs_begin { "preview" } else { "commit" };
        cmds.push(pair(rel, key, value, phase));
    }
    if drag_stopped || (commit_focus && (in_edit || needs_begin)) {
        cmds.push(pair(rel, key, value, "end"));
    }
    match cmds.len() {
        0 => PanelEvent::None,
        1 => PanelEvent::Command(cmds[0].0.clone(), cmds[0].1.clone()),
        _ => PanelEvent::Commands(cmds),
    }
}

fn pair(rel: &str, key: &str, value: &str, phase: &str) -> (String, CommandArgs) {
    (
        "asset.fx_text".into(),
        CommandArgs::new(&[("path", rel), ("key", key), ("value", value), ("phase", phase)]),
    )
}

fn command(rel: &str, key: &str, value: &str, phase: &str) -> PanelEvent {
    let (id, args) = pair(rel, key, value, phase);
    PanelEvent::Command(id, args)
}
