//! Help recipes: import small examples into the open project.

use seal_editor_core::{CommandArgs, EditorSession};
use seal_ui::PanelFrame;

use crate::PanelEvent;

pub const RECIPES: &str = "recipes";

fn command(id: &str, pairs: &[(&str, &str)]) -> PanelEvent {
    PanelEvent::Command(id.to_string(), CommandArgs::new(pairs))
}

pub fn paint_recipes(frame: &mut PanelFrame, session: &EditorSession) -> PanelEvent {
    let mut event = PanelEvent::None;
    if session.project.is_none() {
        frame.muted(&session.messages.get("recipes-no-project"));
        return event;
    }
    frame.muted(&session.messages.get("recipes-help"));
    frame.scroll(|frame| {
        for recipe in seal_editor_core::RECIPES {
            frame.section(&session.messages.get(recipe.title_key), |frame| {
                frame.label(&session.messages.get(recipe.description_key));
                if frame.button(&session.messages.get("recipes-import-button")) {
                    event = command("recipe.import", &[("id", recipe.id)]);
                }
            });
        }
    });
    event
}
