//! Inspector rows for Character 2D.

use seal_editor_core::{character2d, EditorSession};
use seal_ui::{ContextMenuEntry, PanelFrame, PropertyCheckbox};

use crate::{
    command, inspector_field_modified, inspector_reset_tip, inspector_text_field_edit,
    shared_field, PanelEvent,
};

/// Character 2D rows. Only the on switch shows until the component is on.
pub(crate) fn rows(
    frame: &mut PanelFrame,
    session: &EditorSession,
    targets: &[&seal_editor_core::MockNode],
    set_target: (&str, &str),
    reset_target: (&str, &str),
    prop_menu: &[ContextMenuEntry],
    prop_menu_pick: &mut Option<seal_ui::ContextMenuPick>,
) -> Option<PanelEvent> {
    let text = |key: &str| session.messages.get(key);
    let on = shared_field(targets, "char_on")? == "yes";
    let keys: &[&str] = if on { character2d::KEYS } else { &character2d::KEYS[..1] };
    let mut event = None;
    for &key in keys {
        let Some(current) = shared_field(targets, key) else { continue };
        let label = text(&format!("inspector-{}", key.replace('_', "-")));
        if key == "char_on" || key == "char_crouch" {
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
                event =
                    Some(command("inspector.set", &[set_target, ("field", key), ("value", value)]));
            }
            continue;
        }
        let modified = inspector_field_modified(targets, key);
        let tip_owned = modified.then(|| inspector_reset_tip(session, key));
        if key == "char_mode" {
            let options: Vec<(&str, String)> = character2d::MODES
                .iter()
                .map(|mode| (*mode, text(&format!("inspector-char-mode-{mode}"))))
                .collect();
            let (picked, reset) = frame.choice_row(
                &label,
                &current,
                &options,
                modified,
                tip_owned.as_deref(),
                None,
                Some(prop_menu),
                prop_menu_pick,
                Some(key),
            );
            if let Some(value) = picked {
                event = Some(command(
                    "inspector.set",
                    &[set_target, ("field", key), ("value", &value)],
                ));
            } else if reset {
                event = Some(command("inspector.reset_field", &[reset_target, ("field", key)]));
            }
            continue;
        }
        let mut draft = current;
        let field_error = character2d::check(key, &draft).map(|msg| session.messages.get(msg));
        let (changed, reset, commit_focus) = frame.property_validated_row(
            &label,
            &mut draft,
            field_error.as_deref(),
            modified,
            tip_owned.as_deref(),
            None,
            Some(prop_menu),
            prop_menu_pick,
            Some(key),
        );
        let editing =
            session.models.inspector_field_edit.as_ref().is_some_and(|edit| edit.field == key);
        if changed && character2d::check(key, &draft).is_none() {
            event = Some(inspector_text_field_edit(
                session,
                set_target,
                key,
                &draft,
                true,
                commit_focus,
            ));
        } else if commit_focus && editing {
            event = Some(inspector_text_field_edit(session, set_target, key, &draft, false, true));
        } else if reset {
            event = Some(command("inspector.reset_field", &[reset_target, ("field", key)]));
        }
    }
    event
}
