//! Width curve, colour gradient and offset distance for a 2D shape.

use seal_editor_core::curve::{Curve, Gradient};
use seal_editor_core::curve_text::{curve_to_text, gradient_to_text, load_curve, load_gradient};
use seal_editor_core::EditorSession;
use seal_ui::{curve_row, gradient_row, CurveUiText, GradientUiText, PanelFrame};

use crate::inspector_panel::inspector_field_edit_event;
use crate::PanelEvent;

const OFFSET_DRAFT: &str = "shape-offset";

/// Curve, gradient and offset rows under the 2D Shape section.
pub fn rows(
    frame: &mut PanelFrame,
    session: &EditorSession,
    targets: &[&seal_editor_core::MockNode],
    set_target: (&str, &str),
) -> Option<PanelEvent> {
    let id = targets.first()?.id.as_str();
    let entity = session.world.entity(id)?;
    let kind = entity.shape2d.kind.clone();
    if kind.is_empty() {
        return None;
    }
    let mut event = PanelEvent::None;
    if kind == "line" {
        event = take(event, width_curve(frame, session, id, set_target));
        event = take(event, colour_gradient(frame, session, id, set_target));
    }
    if kind == "polygon" {
        event = take(event, offset_row(frame, session));
    }
    if matches!(event, PanelEvent::None) {
        None
    } else {
        Some(event)
    }
}

fn take(current: PanelEvent, next: PanelEvent) -> PanelEvent {
    if matches!(next, PanelEvent::None) {
        current
    } else {
        next
    }
}

struct Words {
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
    gradient_delete: String,
    gradient_alpha: String,
    gradient_hint: String,
    gradient_stop: String,
    width: String,
    colour: String,
    offset: String,
    apply: String,
}

impl Words {
    fn load(session: &EditorSession) -> Self {
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
            gradient_delete: g("asset-gradient-delete"),
            gradient_alpha: g("asset-gradient-alpha"),
            gradient_hint: g("asset-gradient-hint"),
            gradient_stop: g("asset-gradient-stop"),
            width: g("inspector-shape-width-curve"),
            colour: g("inspector-shape-gradient"),
            offset: g("inspector-shape-offset"),
            apply: g("inspector-shape-offset-apply"),
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

fn width_curve(
    frame: &mut PanelFrame,
    session: &EditorSession,
    id: &str,
    set_target: (&str, &str),
) -> PanelEvent {
    let words = Words::load(session);
    let stored =
        session.world.entity(id).map(|e| e.shape2d.width_curve.clone()).unwrap_or_default();
    let (curve, error) = if stored.trim().is_empty() {
        (Curve::linear(1.0, 1.0), None)
    } else {
        let (curve, error) = load_curve(&stored);
        (curve, error)
    };
    if let Some(error) = error {
        frame.muted(&error);
    }
    let edit = curve_row(
        frame.ui_mut(),
        &format!("{id}:shape-width"),
        &words.width,
        &curve,
        !stored.trim().is_empty(),
        Some((0.0, 1.0)),
        &words.curve(),
    );
    let text = if edit.reset { String::new() } else { curve_to_text(&edit.curve) };
    inspector_field_edit_event(
        session,
        set_target,
        "shape_width_curve",
        &text,
        edit.changed || edit.reset,
        edit.drag_started,
        edit.drag_stopped,
        edit.escape_cancel,
        edit.commit_focus || edit.reset,
    )
}

fn colour_gradient(
    frame: &mut PanelFrame,
    session: &EditorSession,
    id: &str,
    set_target: (&str, &str),
) -> PanelEvent {
    let words = Words::load(session);
    let stored = session.world.entity(id).map(|e| e.shape2d.gradient.clone()).unwrap_or_default();
    let (gradient, error) = if stored.trim().is_empty() {
        (Gradient::from_endpoints([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0]), None)
    } else {
        let (gradient, error) = load_gradient(&stored);
        (gradient, error)
    };
    if let Some(error) = error {
        frame.muted(&error);
    }
    let edit = gradient_row(
        frame.ui_mut(),
        &format!("{id}:shape-gradient"),
        &words.colour,
        &gradient,
        !stored.trim().is_empty(),
        &words.gradient(),
    );
    let text = if edit.reset { String::new() } else { gradient_to_text(&edit.gradient) };
    inspector_field_edit_event(
        session,
        set_target,
        "shape_gradient",
        &text,
        edit.changed || edit.reset,
        edit.drag_started,
        edit.drag_stopped,
        edit.escape_cancel,
        edit.commit_focus || edit.reset,
    )
}

fn offset_row(frame: &mut PanelFrame, session: &EditorSession) -> PanelEvent {
    let words = Words::load(session);
    let current = session.draft(OFFSET_DRAFT).and_then(|v| v.parse().ok()).unwrap_or(0.1);
    let mut value = current;
    let mut pick = None;
    let (changed, _, _) =
        frame.number_row(&words.offset, &mut value, 0.05, false, None, None, None, &mut pick);
    let mut event = PanelEvent::None;
    if changed && value.is_finite() {
        event =
            crate::command("draft.set", &[("key", OFFSET_DRAFT), ("value", &value.to_string())]);
    }
    if frame.button(&words.apply) {
        event = crate::command("shape.offset", &[]);
    }
    event
}
