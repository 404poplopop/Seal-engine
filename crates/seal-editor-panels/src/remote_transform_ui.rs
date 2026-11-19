//! Inspector rows for a Remote Transform.

use seal_editor_core::EditorSession;
use seal_ui::{ContextMenuEntry, PanelFrame, PropertyCheckbox};

use crate::{command, inspector_pickers, shared_field, PanelEvent};

/// Remote Transform rows. Nothing shows until the component is on.
pub(crate) fn rows(
    frame: &mut PanelFrame,
    session: &EditorSession,
    targets: &[&seal_editor_core::MockNode],
    set_target: (&str, &str),
    _reset_target: (&str, &str),
    prop_menu: &[ContextMenuEntry],
    prop_menu_pick: &mut Option<seal_ui::ContextMenuPick>,
) -> Option<PanelEvent> {
    let text = |key: &str| session.messages.get(key);
    if shared_field(targets, "remote_on").as_deref() != Some("yes") {
        return None;
    }
    let mut event = None;
    frame.muted(&text("inspector-remote-hint"));
    for &key in seal_editor_core::remote_transform::KEYS {
        let Some(current) = shared_field(targets, key) else { continue };
        let label = text(&format!("inspector-{}", key.replace('_', "-")));
        if matches!(key, "remote_on" | "remote_position" | "remote_rotation" | "remote_scale") {
            let checked = current == "yes";
            let check = if checked { PropertyCheckbox::On } else { PropertyCheckbox::Off };
            if let Some(next) = frame.property_checkbox_row(
                &label,
                check,
                checked,
                None,
                Some(prop_menu),
                prop_menu_pick,
                Some(key),
            ) {
                let value = if matches!(next, PropertyCheckbox::On) { "yes" } else { "no" };
                event = Some(command(
                    "inspector.set",
                    &[set_target, ("field", key), ("value", value)],
                ));
            }
            continue;
        }
        if let Some(painted) =
            inspector_pickers::try_paint(frame, session, key, &label, Some(current.as_str()), set_target)
        {
            if !matches!(painted, PanelEvent::None) {
                event = Some(painted);
            }
        }
    }
    event
}
