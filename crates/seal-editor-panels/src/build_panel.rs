//! Project Settings > Build: scenes included in Export Game.

use seal_editor_core::{discover_scenes, read_build_scenes, EditorSession};
use seal_ui::PanelFrame;

use crate::PanelEvent;

fn command(id: &str, pairs: &[(&str, &str)]) -> PanelEvent {
    PanelEvent::Command(id.to_string(), seal_editor_core::CommandArgs::new(pairs))
}

pub fn paint_build(frame: &mut PanelFrame, session: &mut EditorSession) -> PanelEvent {
    let mut event = PanelEvent::None;
    let Ok(root) = session.project.as_ref().map(|p| p.root.clone()).ok_or(()) else {
        frame.label("Open a project to edit the build list.");
        return event;
    };
    let scenes = read_build_scenes(&root).unwrap_or_default();
    let discovered = discover_scenes(&root);
    frame.scroll(|frame| {
        frame.padded(|frame| {
            frame.muted(&session.messages.get("build-scenes-help"));
            for (index, path) in scenes.iter().enumerate() {
                frame.row(|frame| {
                    let label = if index == 0 {
                        session.messages.format("build-scenes-start", &[("path", path)])
                    } else {
                        path.to_string()
                    };
                    frame.label(&label);
                    if frame.button(&session.messages.get("build-scenes-up")) {
                        event = command(
                            "build.scenes_move",
                            &[("index", &index.to_string()), ("direction", "up")],
                        );
                    }
                    if frame.button(&session.messages.get("build-scenes-down")) {
                        event = command(
                            "build.scenes_move",
                            &[("index", &index.to_string()), ("direction", "down")],
                        );
                    }
                    if frame.button(&session.messages.get("build-scenes-remove")) {
                        event = command("build.scenes_remove", &[("index", &index.to_string())]);
                    }
                });
            }
            frame.section(&session.messages.get("build-scenes-add-title"), |frame| {
                for path in discovered.iter().filter(|p| !scenes.iter().any(|s| s == *p)) {
                    if frame.button(path) {
                        event = command("build.scenes_add", &[("path", path)]);
                    }
                }
            });
        });
    });
    event
}
