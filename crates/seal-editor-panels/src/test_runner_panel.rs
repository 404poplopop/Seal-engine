//! Test Runner: list project test scripts, run them, and show each result.

use seal_editor_core::functional_tests::{self, ListedTest, SuiteReport, TestScript};
use seal_editor_core::EditorSession;
use seal_ui::{PropertyCheckbox, PanelFrame};

use crate::{command, PanelEvent};

const FILTER_KEY: &str = "test-runner.filter";
const WRITE_KEY: &str = "test-runner.write";
const REPORT_KEY: &str = "test-runner.report";
const RESULTS_KEY: &str = "test-runner.results";

pub fn paint(frame: &mut PanelFrame, session: &mut EditorSession) -> PanelEvent {
    let mut event = PanelEvent::None;
    frame.scroll(|frame| {
        frame.padded(|frame| {
            frame.title(&session.messages.get("panel-test-runner-title"));
            frame.muted(&session.messages.get("test-runner-hint"));
            let Some(root) = session.project.as_ref().map(|project| project.root.clone()) else {
                frame.muted(&session.messages.get("test-runner-no-project"));
                return;
            };
            let mut filter = session.models.panel_states.get(FILTER_KEY).cloned().unwrap_or_default();
            if frame.property_validated(&session.messages.get("test-runner-filter"), &mut filter, None) {
                session.models.panel_states.insert(FILTER_KEY.into(), filter.clone());
            }
            let mut write = session.models.panel_states.get(WRITE_KEY).is_some_and(|value| value == "1");
            let mut menu_pick = None;
            let write_label = session.messages.get("test-runner-write-report");
            if let Some(next) = frame.property_checkbox_row(
                &write_label,
                if write { PropertyCheckbox::On } else { PropertyCheckbox::Off },
                false,
                None,
                None,
                &mut menu_pick,
                Some("test-runner-write"),
            ) {
                write = next == PropertyCheckbox::On;
                session
                    .models
                    .panel_states
                    .insert(WRITE_KEY.into(), if write { "1".into() } else { "0".into() });
            }
            let mut report = session.models.panel_states.get(REPORT_KEY).cloned().unwrap_or_default();
            if frame.property_validated(&session.messages.get("test-runner-report"), &mut report, None)
            {
                session.models.panel_states.insert(REPORT_KEY.into(), report.clone());
            }
            if write && report.trim().is_empty() {
                frame.muted(&session.messages.get("test-runner-report-empty"));
            }
            if frame.button(&session.messages.get("test-runner-run")) {
                event = run_event(write, &report, None);
            }
            let listed = functional_tests::list_tests(&root);
            let shown: Vec<&ListedTest> = listed
                .iter()
                .filter(|item| {
                    filter.trim().is_empty()
                        || item.path.to_ascii_lowercase().contains(&filter.to_ascii_lowercase())
                        || item.name.to_ascii_lowercase().contains(&filter.to_ascii_lowercase())
                })
                .collect();
            if shown.is_empty() {
                frame.muted(&session.messages.get("test-runner-empty"));
                return;
            }
            let stored = session
                .models
                .panel_states
                .get(RESULTS_KEY)
                .and_then(|text| SuiteReport::from_state(text).ok());
            for item in shown {
                let case = stored.as_ref().and_then(|report| {
                    report.cases.iter().find(|case| case.path == item.path)
                });
                let status = match case {
                    Some(case) if case.passed => session.messages.get("test-runner-pass"),
                    Some(_) => session.messages.get("test-runner-fail"),
                    None => String::new(),
                };
                let line = if status.is_empty() {
                    item.name.clone()
                } else {
                    format!("{}  {status}", item.name)
                };
                frame.label(&line);
                if let Some(problem) = &item.problem {
                    frame.muted(problem);
                } else if let Some(case) = case {
                    if !case.passed && !case.message.is_empty() {
                        frame.muted(&case.message);
                    }
                }
                if frame.button(&session.messages.get("test-runner-run-one")) {
                    event = run_event(write, &report, Some(item.path.as_str()));
                }
                if frame.button(&session.messages.get("test-runner-open")) {
                    event = command("asset.open", &[("path", item.path.as_str())]);
                }
            }
        });
    });
    event
}

fn run_event(write: bool, report: &str, only: Option<&str>) -> PanelEvent {
    let mut pairs: Vec<(&str, &str)> = Vec::new();
    if write && !report.trim().is_empty() {
        pairs.push(("junit", report.trim()));
    }
    if let Some(only) = only {
        pairs.push(("only", only));
    }
    command("tests.run", &pairs)
}

pub(crate) fn test_asset_editor(
    frame: &mut PanelFrame,
    session: &EditorSession,
    rel: &str,
    full: &std::path::Path,
) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let raw = match std::fs::read_to_string(full) {
        Ok(raw) => raw,
        Err(err) => {
            frame.padded(|frame| {
                frame.muted(
                    &session.messages.format("asset-test-damaged", &[("error", err.to_string().as_str())]),
                );
            });
            return PanelEvent::None;
        }
    };
    let script = match functional_tests::load_script(&raw) {
        Ok(script) => script,
        Err(err) => {
            frame.padded(|frame| {
                frame.muted(&session.messages.format("asset-test-damaged", &[("error", err.as_str())]));
            });
            return PanelEvent::None;
        }
    };
    paint_script(frame, &text, rel, &script)
}

fn paint_script(
    frame: &mut PanelFrame,
    text: &impl Fn(&str) -> String,
    rel: &str,
    script: &TestScript,
) -> PanelEvent {
    let mut event = PanelEvent::None;
    let path = rel.to_string();
    frame.bar(|frame| {
        if frame.button(&text("asset-test-add-assert")) {
            event = command("tests.assert_add", &[("path", path.as_str())]);
        }
    });
    frame.scroll(|frame| {
        frame.padded(|frame| {
            let mut name = script.name.clone();
            if frame.property_validated(&text("asset-test-name"), &mut name, None) && name != script.name {
                event = command("tests.set", &[("path", path.as_str()), ("field", "name"), ("value", name.as_str())]);
            }
            let mut scene = script.scene.clone();
            if frame.property_validated(&text("asset-test-scene"), &mut scene, None) && scene != script.scene {
                event = command(
                    "tests.set",
                    &[("path", path.as_str()), ("field", "scene"), ("value", scene.as_str())],
                );
            }
            let mut frames = script.frames as f32;
            if frame.number(&text("asset-test-frames"), &mut frames, 1.0) {
                let next = frames.round().clamp(1.0, 100_000.0) as u32;
                if next != script.frames {
                    let value = next.to_string();
                    event = command(
                        "tests.set",
                        &[("path", path.as_str()), ("field", "frames"), ("value", value.as_str())],
                    );
                }
            }
            let props = property_options(text);
            let ops = compare_options(text);
            for (index, node) in script.nodes.iter().enumerate() {
                let index_text = index.to_string();
                frame.ui_mut().push_id(index, |ui| {
                    let mut frame = PanelFrame::new(ui);
                let mut object = node.object.clone();
                if frame.property_validated(&text("asset-test-object"), &mut object, None) && object != node.object
                {
                    event = command(
                        "tests.set",
                        &[
                            ("path", path.as_str()),
                            ("index", index_text.as_str()),
                            ("field", "object"),
                            ("value", object.as_str()),
                        ],
                    );
                }
                if let Some(picked) = frame.choice(&text("asset-test-property"), &node.property, &props) {
                    event = command(
                        "tests.set",
                        &[
                            ("path", path.as_str()),
                            ("index", index_text.as_str()),
                            ("field", "property"),
                            ("value", picked.as_str()),
                        ],
                    );
                }
                if let Some(picked) = frame.choice(&text("asset-test-op"), node.op.as_str(), &ops) {
                    event = command(
                        "tests.set",
                        &[
                            ("path", path.as_str()),
                            ("index", index_text.as_str()),
                            ("field", "op"),
                            ("value", picked.as_str()),
                        ],
                    );
                }
                let mut expected = node.expected.clone();
                if frame.property_validated(&text("asset-test-value"), &mut expected, None)
                    && expected != node.expected
                {
                    event = command(
                        "tests.set",
                        &[
                            ("path", path.as_str()),
                            ("index", index_text.as_str()),
                            ("field", "value"),
                            ("value", expected.as_str()),
                        ],
                    );
                }
                if frame.button(&text("asset-test-remove")) {
                    event = command(
                        "tests.assert_remove",
                        &[("path", path.as_str()), ("index", index_text.as_str())],
                    );
                }
                });
            }
        });
    });
    event
}

fn property_options(text: &impl Fn(&str) -> String) -> Vec<(&'static str, String)> {
    vec![
        ("x", text("test-prop-x")),
        ("y", text("test-prop-y")),
        ("z", text("test-prop-z")),
        ("name", text("test-prop-name")),
        ("tag", text("test-prop-tag")),
        ("hidden", text("test-prop-hidden")),
    ]
}

fn compare_options(text: &impl Fn(&str) -> String) -> Vec<(&'static str, String)> {
    vec![
        ("eq", text("test-op-eq")),
        ("ne", text("test-op-ne")),
        ("lt", text("test-op-lt")),
        ("le", text("test-op-le")),
        ("gt", text("test-op-gt")),
        ("ge", text("test-op-ge")),
    ]
}
