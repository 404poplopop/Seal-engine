//! Project tutorial steps shipped with templates.

use seal_editor_core::{load_done, load_guide, project_tutorials, EditorSession};
use seal_ui::PanelFrame;

use crate::PanelEvent;

pub const TUTORIAL: &str = "tutorial";

pub fn paint_tutorial(frame: &mut PanelFrame, session: &mut EditorSession) -> PanelEvent {
    project_tutorials::tick(session);
    let Some(root) = session.project.as_ref().map(|p| p.root.clone()) else {
        frame.muted(&session.messages.get("tutorial-no-project"));
        return PanelEvent::None;
    };
    let Ok(Some(guide)) = load_guide(&root) else {
        frame.muted(&session.messages.get("tutorial-none"));
        return PanelEvent::None;
    };
    let Ok(done) = load_done(&root) else {
        frame.muted(&session.messages.get("tutorial-none"));
        return PanelEvent::None;
    };
    frame.muted(&session.messages.format("tutorial-template", &[("id", &guide.template_id)]));
    frame.scroll(|frame| {
        for step in &guide.steps {
            let mark = if done.contains(&step.id) { "✓" } else { " " };
            frame.section(&format!("{mark} {}. {}", step.number, step.title), |frame| {
                frame.label(&step.body);
            });
        }
    });
    PanelEvent::None
}