//! Platform override rows in the Settings panel.

use seal_ui::PanelFrame;

use crate::{command, PanelEvent};

pub fn preview_platform_row(frame: &mut PanelFrame, session: &mut seal_editor_core::EditorSession) -> PanelEvent {
    if session.project.is_none() {
        return PanelEvent::None;
    }
    let platforms: Vec<_> = seal_editor_core::EXPORT_PLATFORMS
        .iter()
        .map(|(id, key)| (*id, session.messages.get(key)))
        .collect();
    let current = session.models.settings_preview_platform.as_str();
    if let Some(picked) = frame.choice(
        &session.messages.get("settings-preview-platform"),
        current,
        &platforms.iter().map(|(id, label)| (*id, label.clone())).collect::<Vec<_>>(),
    ) {
        return command("settings.preview_platform", &[("platform", &picked)]);
    }
    PanelEvent::None
}

pub fn override_row(
    frame: &mut PanelFrame,
    session: &mut seal_editor_core::EditorSession,
    setting_id: &str,
    base_value: &str,
) -> Option<PanelEvent> {
    if session.project.is_none() {
        return None;
    }
    let platform = session.models.settings_preview_platform.as_str();
    let enabled = session.models.platform_overrides.enabled(setting_id, platform);
    let effective = seal_editor_core::effective_from_session(session, setting_id, platform);
    frame.muted(&session.messages.format(
        "setting-effective-value",
        &[("value", effective.as_str()), ("platform", platform)],
    ));
    let title = session.messages.get("setting-platform-override");
    if frame.toggle(&title, enabled) {
        let next = !enabled;
        let value = session
            .models
            .platform_overrides
            .get(setting_id, platform)
            .unwrap_or(base_value);
        return Some(command(
            "settings.platform.override",
            &[
                ("id", setting_id),
                ("platform", platform),
                ("enabled", if next { "true" } else { "false" }),
                ("value", value),
            ],
        ));
    }
    if enabled {
        let mut value = session
            .models
            .platform_overrides
            .get(setting_id, platform)
            .unwrap_or(base_value)
            .to_string();
        if frame.property(&session.messages.get("setting-platform-override-value"), &mut value) {
            return Some(command(
                "settings.platform.override",
                &[
                    ("id", setting_id),
                    ("platform", platform),
                    ("enabled", "true"),
                    ("value", value.as_str()),
                ],
            ));
        }
    }
    None
}