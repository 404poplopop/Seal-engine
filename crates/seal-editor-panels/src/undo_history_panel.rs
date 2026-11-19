//! Undo History panel.

use egui::RichText;
use seal_editor_core::EditorSession;
use seal_ui::PanelFrame;

use crate::{command, PanelEvent};

pub fn paint_undo_history(frame: &mut PanelFrame, session: &mut EditorSession) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let mut event = PanelEvent::None;
    let current = session.undo_history_position();
    let limit = session.undo_step_limit();
    let total_steps = session.past.len() + session.future.len();
    let bytes = total_steps as f32 * 62_000.0;
    let mb = bytes / (1024.0 * 1024.0);
    frame.bar(|bar| {
        bar.muted(&session.messages.format(
            "undo-history-stats",
            &[
                ("count", &total_steps.to_string()),
                ("limit", &limit.to_string()),
                ("mb", &format!("{mb:.1}")),
            ],
        ));
    });
    const SCROLL_KEY: &str = "undo-history:scroll";
    let saved_scroll = session
        .models
        .panel_states
        .get(SCROLL_KEY)
        .and_then(|raw| raw.parse::<f32>().ok())
        .unwrap_or(0.0);
    let mut scroll = saved_scroll;
    frame.ui_mut().push_id("undo-history-list", |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .scroll_offset(egui::vec2(0.0, scroll))
            .show(ui, |ui| {
                let baseline = text("undo-history-baseline");
                let baseline_ink = if current == 0 {
                    RichText::new(&baseline).strong()
                } else {
                    RichText::new(&baseline)
                };
                if ui.selectable_label(false, baseline_ink).clicked() {
                    event = command("undo.history_jump", &[("index", "0")]);
                }
                for (index, item) in session.past.iter().enumerate() {
                    let row = index + 1;
                    let label = session.past_action_label(item);
                    let ink = if row == current {
                        RichText::new(&label).strong()
                    } else {
                        RichText::new(&label)
                    };
                    if ui.selectable_label(false, ink).clicked() {
                        event = command("undo.history_jump", &[("index", &row.to_string())]);
                    }
                }
                for (offset, item) in session.future.iter().rev().enumerate() {
                    let row = session.past.len() + 1 + offset;
                    let label = session.past_action_label(item);
                    let ink = RichText::new(label).weak().italics();
                    if ui.selectable_label(false, ink).clicked() {
                        event = command("undo.history_jump", &[("index", &row.to_string())]);
                    }
                }
                scroll = ui.clip_rect().top() - ui.min_rect().top();
            });
    });
    if (scroll - saved_scroll).abs() > 0.5 {
        session.models.panel_states.insert(SCROLL_KEY.to_string(), format!("{scroll:.1}"));
    }
    event
}
