//! Inspector row that attaches an object to a socket on its parent.

use seal_editor_core::{EditorSession, MockNode};
use seal_ui::PanelFrame;

use crate::{command, PanelEvent};

pub(crate) fn row(
    frame: &mut PanelFrame,
    session: &EditorSession,
    target: &MockNode,
    set_target: (&str, &str),
) -> Option<PanelEvent> {
    let text = |key: &str| session.messages.get(key);
    let parent_id = target.parent.as_deref().unwrap_or("");
    let parent = session.world.entities.iter().find(|e| e.id == parent_id);
    let project = session.project.as_ref().map(|p| p.root.as_path());
    let names = parent
        .map(|parent| seal_editor_core::sprite_socket::names_of(parent, project))
        .unwrap_or_default();
    let current = session
        .world
        .entities
        .iter()
        .find(|e| e.id == target.id)
        .map(|e| e.socket.clone())
        .unwrap_or_default();
    let none = text("inspector-sprite-socket-none");
    let mut options: Vec<(&str, String)> = vec![("", none.clone())];
    for name in &names {
        options.push((name.as_str(), name.clone()));
    }
    let mut menu_pick = None;
    let tip = text("inspector-sprite-socket-none");
    let (picked, reset) = frame.choice_row(
        &text("inspector-sprite-socket"),
        &current,
        &options,
        !current.is_empty(),
        (!current.is_empty()).then_some(tip.as_str()),
        None,
        None,
        &mut menu_pick,
        Some("socket"),
    );
    if reset {
        return Some(command("inspector.set", &[set_target, ("field", "socket"), ("value", "")]));
    }
    if let Some(value) = picked {
        return Some(command(
            "inspector.set",
            &[set_target, ("field", "socket"), ("value", value.as_str())],
        ));
    }
    None
}
