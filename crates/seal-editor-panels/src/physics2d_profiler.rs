//! Physics 2D profiler panel shown while play is running.

use seal_editor_core::{EditorSession, PlayMode};
use seal_ui::PanelFrame;

pub const PHYSICS2D_PROFILER: &str = "physics2d-profiler";

pub fn paint(frame: &mut PanelFrame, session: &EditorSession) {
    let text = |key: &str| session.messages.get(key);
    frame.scroll(|frame| {
        frame.padded(|frame| {
            frame.title(&text("panel-physics2d-profiler-title"));
            let play = matches!(session.world.mode, PlayMode::Playing | PlayMode::Paused);
            if !play {
                frame.muted(&text("physics2d-profiler-play-hint"));
                return;
            }
            let Some(stats) = session.world.physics2d_profiler() else {
                frame.muted(&text("physics2d-profiler-no-sim"));
                return;
            };
            frame.section(&text("physics2d-profiler-counts"), |frame| {
                frame.readout(&text("physics2d-profiler-bodies"), &stats.bodies.to_string());
                frame.readout(&text("physics2d-profiler-colliders"), &stats.colliders.to_string());
                frame.readout(&text("physics2d-profiler-contacts"), &stats.contacts.to_string());
                frame.readout(&text("physics2d-profiler-joints"), &stats.joints.to_string());
            });
            frame.section(&text("physics2d-profiler-step"), |frame| {
                frame
                    .readout(&text("physics2d-profiler-step-ms"), &format!("{:.2}", stats.step_ms));
                let tail: String = stats
                    .step_history
                    .iter()
                    .rev()
                    .take(24)
                    .rev()
                    .map(|ms| format!("{ms:.1}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                frame.readout(&text("physics2d-profiler-history"), &tail);
            });
        });
    });
}
