//! Asset reference viewer panel.

use seal_editor_core::EditorSession;
use seal_ui::PanelFrame;

use crate::{command, PanelEvent, CONTENT};

pub const ID: &str = "asset-references";

pub fn paint(frame: &mut PanelFrame, session: &mut EditorSession) -> PanelEvent {
    let mut event = PanelEvent::None;
    frame.scroll(|frame| {
        frame.padded(|frame| {
            frame.title(&session.messages.get("panel-asset-references-title"));
            let target = session.models.asset_reference_target.as_deref().unwrap_or_default();
            if target.is_empty() {
                frame.muted(&session.messages.get("asset-references-pick"));
                return;
            }
            frame.muted(&session.messages.format("asset-references-target", &[("path", target)]));
            frame.section(&session.messages.get("asset-references-used-by"), |frame| {
                let report = &session.models.asset_reference_report;
                if report.referencers.is_empty() {
                    frame.muted(&session.messages.get("asset-references-none"));
                    return;
                }
                for site in &report.referencers {
                    let label = format!("{} — {}", site.file, site.detail);
                    if frame.link(&label) {
                        event = command("content.open_file", &[("path", site.file.as_str())]);
                    }
                }
            });
            frame.section(&session.messages.get("asset-references-uses"), |frame| {
                let report = &session.models.asset_reference_report;
                if report.dependencies.is_empty() {
                    frame.muted(&session.messages.get("asset-references-no-deps"));
                    return;
                }
                for dep in &report.dependencies {
                    if frame.link(dep) {
                        event = command("content.open_file", &[("path", dep.as_str())]);
                    }
                }
            });
            if frame.button(&session.messages.get("asset-references-browse")) {
                session.models.content_pick = Some(target.to_string());
                session.reveal_panel = Some(CONTENT.into());
            }
        });
    });
    event
}
