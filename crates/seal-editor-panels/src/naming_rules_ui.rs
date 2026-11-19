//! Naming rules in Project Settings and the violations panel.

use seal_editor_core::naming_rules::NamingRule;
use seal_editor_core::EditorSession;
use seal_ui::PanelFrame;

use crate::{command, PanelEvent};

pub fn settings_section(frame: &mut PanelFrame, session: &mut EditorSession) -> PanelEvent {
    let title = session.messages.get("naming-rules-project-title");
    let hint = session.messages.get("naming-rules-project-hint");
    let kind_l = session.messages.get("naming-rule-kind");
    let prefix_l = session.messages.get("naming-rule-prefix");
    let suffix_l = session.messages.get("naming-rule-suffix");
    let case_l = session.messages.get("naming-rule-case");
    let pattern_l = session.messages.get("naming-rule-pattern");
    let save_l = session.messages.get("naming-rule-save");
    let scan_l = session.messages.get("naming-scan");
    let mut kind = session.draft("naming-rule-kind").unwrap_or_else(|| "picture".into());
    let mut prefix = session.draft("naming-rule-prefix").unwrap_or_default();
    let mut suffix = session.draft("naming-rule-suffix").unwrap_or_default();
    let mut case = session.draft("naming-rule-case").unwrap_or_else(|| "lower".into());
    let mut pattern = session.draft("naming-rule-pattern").unwrap_or_default();
    let case_opts = [
        ("none", session.messages.get("naming-case-none")),
        ("lower", session.messages.get("naming-case-lower")),
        ("upper", session.messages.get("naming-case-upper")),
        ("snake", session.messages.get("naming-case-snake")),
    ];
    let mut event = PanelEvent::None;
    frame.title(&title);
    frame.muted(&hint);
    if frame.property(&kind_l, &mut kind) {
        let _ = session.set_draft("naming-rule-kind", &kind);
    }
    if frame.property(&prefix_l, &mut prefix) {
        let _ = session.set_draft("naming-rule-prefix", &prefix);
    }
    if frame.property(&suffix_l, &mut suffix) {
        let _ = session.set_draft("naming-rule-suffix", &suffix);
    }
    if let Some(picked) = frame.choice(&case_l, &case, &case_opts) {
        case = picked;
        let _ = session.set_draft("naming-rule-case", &case);
    }
    if frame.property(&pattern_l, &mut pattern) {
        let _ = session.set_draft("naming-rule-pattern", &pattern);
    }
    if frame.button(&save_l) {
        event = command(
            "naming.rule.set",
            &[
                ("kind", kind.as_str()),
                ("prefix", prefix.as_str()),
                ("suffix", suffix.as_str()),
                ("case", case.as_str()),
                ("pattern", pattern.as_str()),
            ],
        );
    }
    if frame.button(&scan_l) {
        event = command("naming.scan", &[]);
    }
    for rule in &session.models.naming_rules.rules {
        frame.muted(&format_rule(session, rule));
    }
    event
}

fn format_rule(session: &EditorSession, rule: &NamingRule) -> String {
    session.messages.format(
        "naming-rule-line",
        &[
            ("kind", rule.kind.as_str()),
            ("prefix", rule.prefix.as_str()),
            ("suffix", rule.suffix.as_str()),
            ("case", rule.case.as_str()),
            ("pattern", rule.pattern.as_str()),
        ],
    )
}

pub fn violations_panel(frame: &mut PanelFrame, session: &EditorSession) -> PanelEvent {
    let mut event = PanelEvent::None;
    frame.scroll(|frame| {
        frame.padded(|frame| {
            frame.title(&session.messages.get("panel-naming-violations-title"));
            let rows = &session.models.naming_rules.violations;
            if rows.is_empty() {
                frame.muted(&session.messages.get("naming-violations-empty"));
                return;
            }
            for row in rows {
                frame.label(&row.path);
                frame.muted(&row.reason);
                if frame.button(&session.messages.get("naming-fix-rename")) {
                    event = command("naming.fix", &[("path", row.path.as_str())]);
                }
            }
        });
    });
    event
}