//! Built-in panels. Later milestones fill these in. The ids are stable.
#![forbid(unsafe_code)]

mod anim_event_ui;
mod asset_panel;
mod build_panel;
mod cam_drag_view;
mod character2d_ui;
mod collider_edit;
mod curve_fields;
mod data_asset_inspector;
mod export_panel;
mod graph_inspector_layout;
mod import_activity_panel;
mod import_pipeline_ui;
mod import_platform_ui;
mod import_preset_ui;
mod input_devices_ui;
mod inspector_audio_listener;
mod inspector_component_menu;
mod inspector_field_row;
mod inspector_panel;
mod inspector_pickers;
mod inspector_property_menu;
mod layered_picture_panel;
mod light2d_view;
mod measure_tool;
mod merged_view;
mod mesh_material_ui;
mod naming_rules_ui;
mod outliner_menu;
mod particle_viewport;
mod recipes_panel;
mod reference_viewer_panel;
mod remote_transform_ui;
mod settings_platform;
mod shape_style_ui;
mod sheet_editor;
mod socket_field;
mod soundset_panel;
mod sprite_shape;
mod sprite_source_panel;
mod sprite_viewport;
mod strings_panel;
mod test_runner_panel;
mod texture_asset_panel;
mod tile_brush;
mod tile_inspector;
mod tutorial_panel;
mod undo_history_panel;
mod viewport_menu;
mod viewport_multi;

use seal_editor_core::{
    collect_log_lines, content_tree_rows, panel_base, read_recent, wizard_name, CommandArgs,
    DockPreference, EditorSession, LogLine, PanelDesc, PanelRegistry, DRAFT_CONFIRM_NONEMPTY,
    DRAFT_FOLDER, DRAFT_GIT, DRAFT_LFS, DRAFT_NAME, DRAFT_TEMPLATE,
};
use seal_ui::{
    ContextMenuPick, GridMark, GridMenus, LogLineView, LogListHit, PanelFrame, TreeClick,
    ViewportCamera, ViewportContextMenu,
};

pub const WELCOME: &str = "welcome";
pub const CONSOLE: &str = "console";
pub const OUTLINER: &str = "outliner";
pub const INSPECTOR: &str = "inspector";
pub const CONTENT: &str = "content";
pub const VIEWPORT: &str = "viewport";
pub const SETTINGS: &str = "settings";
pub const PLACE: &str = "place";
pub const WORLD: &str = "world";
pub const STATS: &str = "stats";
pub const PHYSICS2D_PROFILER: &str = "physics2d-profiler";
mod fx_extra_fields;
mod physics2d_profiler;
mod physics_panel;
mod timeline_props;
mod ui_designer;
pub const RECOVERY: &str = "recovery";
pub const COMMANDS: &str = "commands";
pub const NOTICES: &str = "notices";
pub const WIZARD: &str = "wizard.project";
pub const LAYOUTS: &str = "layouts";
pub const KEYMAP: &str = "keymap";
pub const GRAPH: &str = "graph";
pub const DEV: &str = "dev";
pub const ASSISTANT: &str = "assistant";
pub const TIMELINE: &str = "timeline";
pub const ASSET: &str = "asset";
pub const UNDO_HISTORY: &str = "undo-history";
pub const ASSET_REFERENCES: &str = reference_viewer_panel::ID;
pub const IMPORT_ACTIVITY: &str = import_activity_panel::ID;
pub const NAMING_VIOLATIONS: &str = "naming-violations";
pub const EXPORT: &str = "export";
pub const BUILD: &str = "build";
pub const TEST_RUNNER: &str = "test-runner";
pub const TUTORIAL: &str = tutorial_panel::TUTORIAL;
pub const RECIPES: &str = recipes_panel::RECIPES;

pub fn register_builtin(registry: &mut PanelRegistry) -> Result<(), seal_editor_core::CoreError> {
    for desc in builtin_descs() {
        registry.register(desc)?;
    }
    Ok(())
}

/// Built-in panel descriptions.
///
/// # Examples
///
/// ```
/// use seal_editor_panels::builtin_descs;
/// assert!(builtin_descs().iter().any(|panel| panel.id == "outliner"));
/// ```
pub fn builtin_descs() -> Vec<PanelDesc> {
    vec![
        desc(WELCOME, "panel-welcome-title", DockPreference::Center),
        desc(CONSOLE, "panel-console-title", DockPreference::Bottom),
        desc(OUTLINER, "panel-outliner-title", DockPreference::Left),
        desc(INSPECTOR, "panel-inspector-title", DockPreference::Right),
        desc(CONTENT, "panel-content-title", DockPreference::Bottom),
        desc(VIEWPORT, "panel-viewport-title", DockPreference::Center),
        desc(SETTINGS, "panel-settings-title", DockPreference::Right),
        desc(PLACE, "panel-place-title", DockPreference::Left),
        desc(WORLD, "panel-world-title", DockPreference::Right),
        desc(STATS, "panel-stats-title", DockPreference::Bottom),
        desc(PHYSICS2D_PROFILER, "panel-physics2d-profiler-title", DockPreference::Bottom),
        desc(physics_panel::PHYSICS, "panel-physics-title", DockPreference::Right),
        desc(RECOVERY, "panel-recovery-title", DockPreference::Center),
        desc(COMMANDS, "panel-commands-title", DockPreference::Bottom),
        desc(NOTICES, "panel-notices-title", DockPreference::Bottom),
        desc(WIZARD, "panel-wizard-title", DockPreference::Center),
        desc(LAYOUTS, "panel-layouts-title", DockPreference::Left),
        desc(KEYMAP, "panel-keymap-title", DockPreference::Right),
        desc(GRAPH, "panel-graph-title", DockPreference::Center),
        desc(DEV, "panel-dev-title", DockPreference::Bottom),
        desc(ASSISTANT, "panel-assistant-title", DockPreference::Right),
        desc(TIMELINE, "panel-timeline-title", DockPreference::Bottom),
        desc(ASSET, "panel-asset-title", DockPreference::Center),
        desc(UNDO_HISTORY, "panel-undo-history-title", DockPreference::Bottom),
        desc(ASSET_REFERENCES, "panel-asset-references-title", DockPreference::Bottom),
        desc(IMPORT_ACTIVITY, "panel-import-activity-title", DockPreference::Bottom),
        desc(NAMING_VIOLATIONS, "panel-naming-violations-title", DockPreference::Bottom),
        desc(EXPORT, "panel-export-title", DockPreference::Center),
        desc(BUILD, "panel-build-title", DockPreference::Right),
        desc(TEST_RUNNER, "panel-test-runner-title", DockPreference::Bottom),
        desc(TUTORIAL, "panel-tutorial-title", DockPreference::Right),
        desc(RECIPES, "panel-recipes-title", DockPreference::Right),
    ]
}

fn desc(id: &str, title_key: &str, default_dock: DockPreference) -> PanelDesc {
    PanelDesc { id: id.to_string(), title_key: title_key.to_string(), default_dock }
}

pub enum PanelEvent {
    None,
    Scratch(String),
    Command(String, CommandArgs),
    /// Several commands, run in order.
    Commands(Vec<(String, CommandArgs)>),
}

pub fn show(frame: &mut PanelFrame, id: &str, session: &mut EditorSession) -> PanelEvent {
    match panel_base(id) {
        WELCOME => welcome(frame, session),
        OUTLINER => outliner(frame, session, id),
        INSPECTOR => inspector(frame, session, id),
        CONSOLE => console(frame, session),
        CONTENT => content(frame, session),
        VIEWPORT => viewport(frame, session, id),
        SETTINGS => settings(frame, session),
        PLACE => place(frame, session),
        WORLD => world(frame, session),
        STATS => stats(frame, session),
        id if panel_base(id) == physics2d_profiler::PHYSICS2D_PROFILER => {
            physics2d_profiler::paint(frame, session);
            PanelEvent::None
        }
        id if panel_base(id) == physics_panel::PHYSICS => physics_panel::paint(frame, session),
        RECOVERY => recovery(frame, session),
        COMMANDS => commands_panel(frame, session),
        NOTICES => notices_panel(frame, session),
        WIZARD => wizard_panel(frame, session),
        LAYOUTS => layouts_panel(frame, session),
        KEYMAP => keymap_panel(frame, &mut *session),
        GRAPH => graph_panel(frame, session),
        DEV => dev_panel(frame, session),
        ASSISTANT => assistant_panel(frame, session),
        TIMELINE => timeline_panel(frame, session),
        ASSET => asset_panel::asset_panel(frame, session),
        UNDO_HISTORY => undo_history_panel::paint_undo_history(frame, session),
        ASSET_REFERENCES => reference_viewer_panel::paint(frame, session),
        IMPORT_ACTIVITY => import_activity_panel::paint(frame, session),
        NAMING_VIOLATIONS => naming_rules_ui::violations_panel(frame, session),
        EXPORT => export_panel::export_panel(frame, session),
        BUILD => build_panel::paint_build(frame, session),
        TEST_RUNNER => test_runner_panel::paint(frame, session),
        TUTORIAL => tutorial_panel::paint_tutorial(frame, session),
        RECIPES => recipes_panel::paint_recipes(frame, session),
        _ => {
            frame.title(&session.messages.format("unknown-panel", &[("id", id)]));
            frame.muted(&session.messages.get("unknown-panel-kept"));
            PanelEvent::None
        }
    }
}

fn commands_panel(frame: &mut PanelFrame, session: &EditorSession) -> PanelEvent {
    let mut event = PanelEvent::None;
    frame.scroll(|frame| {
        frame.padded(|frame| {
            frame.title(&session.messages.get("panel-commands-title"));
            let mut rows: Vec<(String, String)> = session
                .commands
                .iter()
                .filter(|spec| spec.alias_of.is_none())
                .map(|spec| (spec.id.clone(), session.messages.get(&spec.title_key)))
                .collect();
            rows.sort_by(|left, right| left.0.cmp(&right.0));
            for (id, title) in rows {
                let row = session
                    .messages
                    .format("command-row", &[("title", title.as_str()), ("id", id.as_str())]);
                if frame.button(&row) {
                    event = command(&id, &[]);
                }
            }
        });
    });
    event
}

fn notices_panel(frame: &mut PanelFrame, session: &EditorSession) -> PanelEvent {
    frame.scroll(|frame| {
        frame.padded(|frame| {
            frame.title(&session.messages.get("panel-notices-title"));
            let history = session.foundation.notices.history();
            if history.is_empty() {
                frame.muted(&session.messages.get("notices-empty"));
                return;
            }
            for notice in history.iter().rev() {
                if notice.error {
                    frame.label(
                        &session
                            .messages
                            .format("notice-error-line", &[("text", notice.text.as_str())]),
                    );
                } else {
                    frame.label(&notice.text);
                }
            }
        });
    });
    PanelEvent::None
}

fn layouts_panel(frame: &mut PanelFrame, session: &EditorSession) -> PanelEvent {
    let mut event = PanelEvent::None;
    frame.scroll(|frame| {
        frame.padded(|frame| {
            frame.title(&session.messages.get("panel-layouts-title"));
            for name in ["default", "debug", "minimal"] {
                if frame.button(name) {
                    event = command("window.apply-layout", &[("name", name)]);
                }
            }
        });
    });
    event
}

fn keymap_panel(frame: &mut PanelFrame, session: &mut EditorSession) -> PanelEvent {
    let mut event = PanelEvent::None;
    frame.scroll(|frame| {
        frame.padded(|frame| {
            frame.title(&session.messages.get("panel-keymap-title"));
            frame.title(&session.messages.get("keymap-profile-title"));
            let mut name = session.models.keymap_profile_draft.clone();
            if name.is_empty() {
                name.clone_from(&session.models.keymap_profile_name);
            }
            if frame.property(&session.messages.get("keymap-profile-name"), &mut name) {
                session.models.keymap_profile_draft = name.clone();
            }
            if frame.button(&session.messages.get("keymap-profile-save")) && !name.trim().is_empty()
            {
                event = command("keymap.profile.save", &[("name", name.trim())]);
            }
            if frame.button(&session.messages.get("keymap-profile-switch"))
                && !name.trim().is_empty()
            {
                event = command("keymap.profile.switch", &[("name", name.trim())]);
            }
            if frame.button(&session.messages.get("keymap-profile-export")) {
                event = command("keymap.profile.export", &[("name", name.trim())]);
            }
            if frame.button(&session.messages.get("keymap-profile-import")) {
                event = command("keymap.profile.import", &[("name", name.trim())]);
            }
            let conflicts = seal_editor_core::binding_conflicts(&session.keymap);
            if !conflicts.is_empty() {
                frame.title(&session.messages.get("keymap-conflict-title"));
                for conflict in conflicts {
                    let commands = conflict.commands.join(", ");
                    let label = session.messages.format(
                        "keymap-conflict-row",
                        &[("chord", &conflict.chord.label()), ("commands", commands.as_str())],
                    );
                    frame.muted(&label);
                    if frame.button(&session.messages.get("keymap-conflict-fix")) {
                        event = command(
                            "keymap.conflict.fix",
                            &[("chord", conflict.chord.token().as_str())],
                        );
                    }
                }
            }
            let mut rows: Vec<(String, String)> = session
                .commands
                .iter()
                .filter(|spec| spec.alias_of.is_none())
                .map(|spec| (spec.id.clone(), session.keymap.shortcut(&spec.id)))
                .filter(|(_, chord)| !chord.is_empty())
                .collect();
            rows.sort_by(|left, right| left.0.cmp(&right.0));
            if rows.is_empty() {
                frame.muted(&session.messages.get("layouts-empty"));
                return;
            }
            for (command_id, chord) in rows {
                frame.label(&session.messages.format(
                    "keymap-row",
                    &[("command", command_id.as_str()), ("chord", chord.as_str())],
                ));
            }
        });
    });
    event
}

/// The assistant link: switch it on, allow or deny what AI tools ask to do, and see what they did.
fn assistant_panel(frame: &mut PanelFrame, session: &mut EditorSession) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let on = session.foundation.settings.get("agent.bus.enabled") == Some("true");
    let ask = session.foundation.settings.get("agent.bus.require_approval") != Some("false");
    let port = session.agent_link.as_ref().map_or(seal_editor_core::agent_link::port(), |l| l.port);
    let status = match (&session.agent_link, &session.agent_link_error) {
        (_, Some(error)) => session.messages.format("assistant-failed", &[("error", error)]),
        (Some(link), _) => session.messages.format(
            "assistant-listening",
            &[("port", &link.port.to_string()), ("count", &link.clients().to_string())],
        ),
        (None, _) => text("assistant-off"),
    };
    let (
        enable,
        asking,
        pending_title,
        allow,
        deny,
        allow_all,
        log_title,
        log_empty,
        setup_title,
        setup_hint,
        copy,
    ) = (
        text("assistant-enable"),
        text("assistant-ask"),
        text("assistant-pending"),
        text("assistant-allow"),
        text("assistant-deny"),
        text("assistant-allow-all"),
        text("assistant-activity"),
        text("assistant-activity-empty"),
        text("assistant-setup"),
        text("assistant-setup-hint"),
        text("assistant-copy"),
    );
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "seal-editor".into());
    let exe = exe.replace('\\', "\\\\").replace('"', "\\\"");
    let config = format!(
        "{{\"mcpServers\":{{\"seal\":{{\"command\":\"{exe}\",\"args\":[\"mcp\",\"--port\",\"{port}\"]}}}}}}"
    );
    let pending: Vec<(u64, String, String)> = session
        .agent_pending
        .iter()
        .map(|c| (c.serial, c.title.clone(), format!("{} {}", c.command, c.args)))
        .collect();
    let log: Vec<seal_editor_core::agent_link::AgentEntry> =
        session.agent_log.iter().rev().take(60).cloned().collect();
    let mut decide: Vec<(u64, bool)> = Vec::new();
    let mut event = PanelEvent::None;
    let set = |id: &str, value: bool| {
        (
            "settings.set".to_string(),
            CommandArgs::new(&[("id", id), ("value", if value { "true" } else { "false" })]),
        )
    };
    frame.scroll(|frame| {
        frame.padded(|frame| {
            frame.title(&text("panel-assistant-title"));
            frame.muted(&status);
            if frame.toggle(&enable, on) {
                event = PanelEvent::Command(
                    set("agent.bus.enabled", !on).0,
                    set("agent.bus.enabled", !on).1,
                );
            }
            if frame.toggle(&asking, ask) {
                event = PanelEvent::Command(
                    set("agent.bus.require_approval", !ask).0,
                    set("agent.bus.require_approval", !ask).1,
                );
            }
            frame.rule();
            if !pending.is_empty() {
                frame.label(&pending_title);
                for (serial, title, detail) in &pending {
                    let ui = frame.ui_mut();
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(title).strong()).on_hover_text(detail);
                        ui.label(
                            egui::RichText::new(detail)
                                .monospace()
                                .color(ui.visuals().weak_text_color()),
                        );
                        if seal_ui::crisp::button(ui, egui::Button::new(&allow), true, false)
                            .clicked()
                        {
                            decide.push((*serial, true));
                        }
                        if seal_ui::crisp::button(ui, egui::Button::new(&deny), true, false)
                            .clicked()
                        {
                            decide.push((*serial, false));
                        }
                    });
                }
                if pending.len() > 1 && frame.button(&allow_all) {
                    decide.extend(pending.iter().map(|(s, _, _)| (*s, true)));
                }
                frame.rule();
            }
            frame.label(&log_title);
            if log.is_empty() {
                frame.muted(&log_empty);
            }
            for entry in &log {
                let ui = frame.ui_mut();
                let color = if entry.ok {
                    ui.visuals().text_color()
                } else {
                    egui::Color32::from_rgb(240, 150, 90)
                };
                ui.label(
                    egui::RichText::new(format!("{}  {}", entry.command, entry.outcome))
                        .color(color),
                )
                .on_hover_text(&entry.args);
            }
            frame.rule();
            frame.label(&setup_title);
            frame.muted(&setup_hint);
            let ui = frame.ui_mut();
            ui.label(egui::RichText::new(&config).monospace().small());
            if seal_ui::crisp::button(ui, egui::Button::new(&copy), true, false).clicked() {
                ui.ctx().copy_text(config.clone());
            }
        });
    });
    for (serial, allow) in decide {
        session.agent_decide(serial, allow);
    }
    event
}

/// Timeline: pick a project sequence, play it, and drag the ruler to scrub.
fn timeline_panel(frame: &mut PanelFrame, session: &mut EditorSession) -> PanelEvent {
    use seal_editor_core::sequences::{TIMELINE_SEQ_DRAFT, TIMELINE_TIME_DRAFT};
    let files = session.sequence_files();
    let mut event = PanelEvent::None;
    frame.padded(|frame| {
        let new_label = session.messages.get("timeline-new");
        if files.is_empty() {
            frame.muted(&session.messages.get("timeline-none"));
            if frame.button(&new_label) {
                event = command("timeline.new_sequence", &[]);
            }
            return;
        }
        if frame.button(&new_label) {
            event = command("timeline.new_sequence", &[]);
            return;
        }
        let chosen = session
            .draft(TIMELINE_SEQ_DRAFT)
            .filter(|p| files.contains(p))
            .unwrap_or_else(|| files[0].clone());
        let options: Vec<(&str, String)> = files.iter().map(|f| (f.as_str(), f.clone())).collect();
        if let Some(picked) =
            frame.choice(&session.messages.get("timeline-pick"), &chosen, &options)
        {
            event = PanelEvent::Commands(vec![
                (
                    "draft.set".into(),
                    CommandArgs::new(&[("key", TIMELINE_SEQ_DRAFT), ("value", &picked)]),
                ),
                (
                    "draft.set".into(),
                    CommandArgs::new(&[("key", TIMELINE_TIME_DRAFT), ("value", "0")]),
                ),
            ]);
            return;
        }
        let seq = match seal_editor_core::timeline_doc::sync_timeline(session, &chosen) {
            Ok(()) => session.timeline.as_ref().map(|doc| doc.sequence.clone()),
            Err(error) => {
                frame.muted(&session.messages.format(
                    "timeline-error",
                    &[("path", chosen.as_str()), ("error", &error.message)],
                ));
                return;
            }
        };
        let Some(seq) = seq else {
            return;
        };
        if frame.button(&session.messages.get("timeline-play")) {
            event =
                PanelEvent::Command("sequence.play".into(), CommandArgs::new(&[("path", &chosen)]));
        }
        frame.muted(&session.messages.get("timeline-scrub-hint"));
        let live = session.world.mode == seal_editor_core::PlayMode::Playing;
        let time = match session.world.sequences.first() {
            Some((_, start, _)) if live => session.world.play_time - start,
            _ => session.draft(TIMELINE_TIME_DRAFT).and_then(|t| t.parse().ok()).unwrap_or(0.0),
        };
        let mut from_strip = false;
        if let Some(hit) = timeline_strip(
            frame.ui_mut(),
            session,
            &seq,
            time,
            &session.messages.get("timeline-events"),
        ) {
            from_strip = true;
            event = match hit {
                StripHit::Scrub(t) => PanelEvent::Command(
                    "sequence.scrub".into(),
                    CommandArgs::new(&[("path", &chosen), ("t", &format!("{t:.3}"))]),
                ),
                StripHit::Drag { index, from, time: to, started, finished } => {
                    drag_commands(&seq, &chosen, index, from, to, started, finished)
                }
            };
        }
        if !from_strip {
            let next = timeline_props::controls(frame, session, &chosen, &seq, time);
            if !matches!(next, PanelEvent::None) {
                event = next;
            }
        }
        if live {
            frame.ui_mut().ctx().request_repaint();
        }
    });
    event
}

enum StripHit {
    Scrub(f32),
    Drag { index: usize, from: f32, time: f32, started: bool, finished: bool },
}

#[derive(Clone, Copy)]
struct PropGrab {
    index: usize,
    time: f32,
    began: bool,
}

fn drag_commands(
    seq: &seal_editor_core::Sequence,
    path: &str,
    index: usize,
    from: f32,
    to: f32,
    started: bool,
    finished: bool,
) -> PanelEvent {
    let Some(track) = seq.properties.get(index) else {
        return PanelEvent::None;
    };
    let entity = track.entity.as_str();
    let field = track.path.as_str();
    let from = format!("{:.6}", from);
    let to = format!("{:.6}", to);
    let mut commands = Vec::new();
    if started {
        commands.push((
            "sequence.property_drag".into(),
            CommandArgs::new(&[("path", path), ("phase", "begin")]),
        ));
    }
    commands.push((
        "sequence.property_drag".into(),
        CommandArgs::new(&[
            ("path", path),
            ("phase", "move"),
            ("entity", entity),
            ("field", field),
            ("from", from.as_str()),
            ("to", to.as_str()),
        ]),
    ));
    if finished {
        commands.push((
            "sequence.property_drag".into(),
            CommandArgs::new(&[("path", path), ("phase", "end")]),
        ));
    }
    PanelEvent::Commands(commands)
}

/// Ruler, one row per track with its keys, an events row and the playhead.
/// Returns a scrub time, or a property-key drag.
fn timeline_strip(
    ui: &mut egui::Ui,
    session: &EditorSession,
    seq: &seal_editor_core::Sequence,
    time: f32,
    events_label: &str,
) -> Option<StripHit> {
    const ROW_H: f32 = 20.0;
    const RULER_H: f32 = 22.0;
    let rows = seq.tracks.len() + seq.properties.len() + 1;
    let width = ui.available_width().max(200.0);
    #[allow(non_snake_case)]
    let LABEL_W: f32 = (width * 0.3).clamp(80.0, 170.0);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(width, RULER_H + ROW_H * rows as f32),
        egui::Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals().clone();
    let ink = visuals.text_color();
    let weak = visuals.weak_text_color();
    let small = egui::TextStyle::Small.resolve(ui.style());
    let length = seq.length.max(0.001);
    let lane = egui::Rect::from_min_max(egui::pos2(rect.left() + LABEL_W, rect.top()), rect.max);
    let x_of = |t: f32| lane.left() + 6.0 + (t / length).clamp(0.0, 1.0) * (lane.width() - 12.0);
    painter.rect_filled(rect, 0.0, visuals.extreme_bg_color);
    let ruler = egui::Rect::from_min_size(lane.min, egui::vec2(lane.width(), RULER_H));
    painter.rect_filled(ruler, 0.0, visuals.faint_bg_color);
    let step = if length > 30.0 {
        5.0
    } else if length > 8.0 {
        1.0
    } else {
        0.5
    };
    let mut t = 0.0f32;
    // Labels skip ticks that would land on the previous label (narrow docks).
    let mut label_free = f32::MIN;
    while t <= length + 1e-4 {
        let x = x_of(t);
        painter.vline(
            x,
            egui::Rangef::new(ruler.bottom() - 6.0, ruler.bottom()),
            egui::Stroke::new(1.0, weak),
        );
        painter.vline(
            x,
            egui::Rangef::new(ruler.bottom(), rect.bottom()),
            egui::Stroke::new(1.0, weak.gamma_multiply(0.25)),
        );
        if x + 2.0 >= label_free {
            let shown = painter.text(
                egui::pos2(x + 2.0, ruler.top() + 2.0),
                egui::Align2::LEFT_TOP,
                format!("{}s", (t * 100.0).round() / 100.0),
                small.clone(),
                weak,
            );
            label_free = shown.right() + 6.0;
        }
        t += step;
    }
    let name_of = |id: &str| {
        session
            .world
            .entities
            .iter()
            .find(|e| e.id == id || e.name == id)
            .map_or(id.to_string(), |e| e.name.clone())
    };
    for (i, track) in seq.tracks.iter().enumerate() {
        let y = ruler.bottom() + ROW_H * i as f32;
        let row =
            egui::Rect::from_min_size(egui::pos2(rect.left(), y), egui::vec2(rect.width(), ROW_H));
        if i % 2 == 1 {
            painter.rect_filled(row, 0.0, visuals.faint_bg_color.gamma_multiply(0.5));
        }
        let label = format!("{}  {}", name_of(&track.object), track.track.target);
        painter
            .with_clip_rect(egui::Rect::from_min_max(
                row.min,
                egui::pos2(rect.left() + LABEL_W - 4.0, row.bottom()),
            ))
            .text(
                egui::pos2(rect.left() + 8.0, row.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                small.clone(),
                ink,
            );
        for (kt, _) in &track.track.keys {
            let c = egui::pos2(x_of(*kt), row.center().y);
            let d = 4.5;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    c + egui::vec2(0.0, -d),
                    c + egui::vec2(d, 0.0),
                    c + egui::vec2(0.0, d),
                    c + egui::vec2(-d, 0.0),
                ],
                seal_ui::AXIS_Z,
                egui::Stroke::new(1.0, ink.gamma_multiply(0.6)),
            ));
        }
    }
    let mut marks = Vec::new();
    for (i, track) in seq.properties.iter().enumerate() {
        let y = ruler.bottom() + ROW_H * (seq.tracks.len() + i) as f32;
        let row =
            egui::Rect::from_min_size(egui::pos2(rect.left(), y), egui::vec2(rect.width(), ROW_H));
        if (seq.tracks.len() + i) % 2 == 1 {
            painter.rect_filled(row, 0.0, visuals.faint_bg_color.gamma_multiply(0.5));
        }
        let label = format!(
            "{}  {}",
            name_of(&track.entity),
            crate::timeline_props::field_label(&track.path)
        );
        painter
            .with_clip_rect(egui::Rect::from_min_max(
                row.min,
                egui::pos2(rect.left() + LABEL_W - 4.0, row.bottom()),
            ))
            .text(
                egui::pos2(rect.left() + 8.0, row.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                small.clone(),
                ink,
            );
        for key in &track.keys {
            let c = egui::pos2(x_of(key.time), row.center().y);
            marks.push((i, key.time, c));
            let d = 4.5;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    c + egui::vec2(0.0, -d),
                    c + egui::vec2(d, 0.0),
                    c + egui::vec2(0.0, d),
                    c + egui::vec2(-d, 0.0),
                ],
                seal_ui::AXIS_X,
                egui::Stroke::new(1.0, ink.gamma_multiply(0.6)),
            ));
        }
    }
    let ev_row = egui::Rect::from_min_size(
        egui::pos2(
            rect.left(),
            ruler.bottom() + ROW_H * (seq.tracks.len() + seq.properties.len()) as f32,
        ),
        egui::vec2(rect.width(), ROW_H),
    );
    painter.text(
        egui::pos2(rect.left() + 8.0, ev_row.center().y),
        egui::Align2::LEFT_CENTER,
        events_label,
        small.clone(),
        weak,
    );
    for (et, name) in &seq.events {
        let x = x_of(*et);
        painter.vline(
            x,
            egui::Rangef::new(ev_row.top() + 3.0, ev_row.bottom() - 3.0),
            egui::Stroke::new(2.0, seal_ui::AXIS_Y),
        );
        painter.text(
            egui::pos2(x + 4.0, ev_row.center().y),
            egui::Align2::LEFT_CENTER,
            name,
            small.clone(),
            ink,
        );
    }
    painter.vline(
        rect.left() + LABEL_W,
        rect.y_range(),
        egui::Stroke::new(1.0, weak.gamma_multiply(0.5)),
    );
    let head = x_of(time);
    let orange = egui::Color32::from_rgb(0xf0, 0x9a, 0x3a);
    painter.vline(head, rect.y_range(), egui::Stroke::new(1.5, orange));
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(head - 5.0, ruler.top()),
            egui::pos2(head + 5.0, ruler.top()),
            egui::pos2(head, ruler.top() + 7.0),
        ],
        orange,
        egui::Stroke::NONE,
    ));
    painter.text(
        egui::pos2(head + 4.0, ruler.bottom() - 2.0),
        egui::Align2::LEFT_BOTTOM,
        format!("{time:.2}"),
        small,
        orange,
    );
    let time_at =
        |x: f32| ((x - lane.left() - 6.0) / (lane.width() - 12.0)).clamp(0.0, 1.0) * length;
    let grab_id = ui.id().with("timeline-prop-grab");
    if response.drag_started() {
        if let Some(pos) = response.interact_pointer_pos() {
            if let Some((index, key_time)) = marks
                .iter()
                .filter(|(_, _, center)| center.distance(pos) <= 8.0)
                .min_by(|a, b| a.2.distance(pos).total_cmp(&b.2.distance(pos)))
                .map(|(index, key_time, _)| (*index, *key_time))
            {
                ui.data_mut(|data| {
                    data.insert_temp(grab_id, PropGrab { index, time: key_time, began: false })
                });
            }
        }
    }
    if let Some(grab) = ui.data(|data| data.get_temp::<PropGrab>(grab_id)) {
        let pointer_time = response.interact_pointer_pos().map(|pos| time_at(pos.x));
        let next = pointer_time.unwrap_or(grab.time);
        let finished = response.drag_stopped();
        let started = !grab.began;
        if finished {
            ui.data_mut(|data| data.remove::<PropGrab>(grab_id));
        } else if response.dragged() {
            ui.data_mut(|data| {
                data.insert_temp(grab_id, PropGrab { index: grab.index, time: next, began: true })
            });
        }
        if response.dragged() || finished {
            return Some(StripHit::Drag {
                index: grab.index,
                from: grab.time,
                time: next,
                started,
                finished,
            });
        }
    }
    if response.clicked() || response.dragged() {
        let x = response.interact_pointer_pos()?.x;
        if x >= lane.left() {
            return Some(StripHit::Scrub(time_at(x)));
        }
    }
    None
}

fn dev_panel(frame: &mut PanelFrame, session: &EditorSession) -> PanelEvent {
    frame.scroll(|frame| {
        frame.padded(|frame| {
            frame.title(&session.messages.get("panel-dev-title"));
            let commands = session.commands.iter().count().to_string();
            let dirty = session.dirty_count().to_string();
            let rows = [
                ("version", seal_editor_core::ENGINE_VERSION_FULL),
                ("commands", commands.as_str()),
                ("dirty", dirty.as_str()),
            ];
            for (label, value) in rows {
                frame.label(
                    &session.messages.format("dev-line", &[("label", label), ("value", value)]),
                );
            }
            frame.section(&session.messages.get("dev-icons"), |frame| {
                frame.icon_gallery();
            });
        });
    });
    PanelEvent::None
}

fn welcome(frame: &mut PanelFrame, session: &EditorSession) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let mut event = PanelEvent::None;
    frame.scroll(|frame| {
        frame.padded(|frame| {
            frame.logo(72.0);
            frame.space(8.0);
            frame.title(&text("app-title"));
            match &session.project {
                Some(project) => frame.label(&project.name),
                None => frame.muted(
                    &session
                        .messages
                        .format("about-body", &[("version", seal_editor_core::ENGINE_VERSION)]),
                ),
            }
            frame.space(8.0);
            if frame.button(&text("welcome-new")) {
                event = command("file.new-project", &[]);
            }
            if frame.button(&text("welcome-open")) {
                event = command("file.open-project", &[]);
            }
            frame.space(8.0);
            frame.section(&text("welcome-recent"), |frame| {
                let recent = session
                    .recent_path
                    .as_ref()
                    .and_then(|path| read_recent(path).ok())
                    .unwrap_or_default();
                if recent.is_empty() {
                    frame.muted(&text("welcome-no-recent"));
                }
                for entry in recent {
                    let missing = !std::path::Path::new(&entry).exists();
                    let label = if missing {
                        session.messages.format("welcome-missing", &[("path", entry.as_str())])
                    } else {
                        entry.clone()
                    };
                    if frame.link(&label) {
                        if missing {
                            event = command("file.open-project", &[]);
                        } else {
                            event = command("file.open-project", &[("path", entry.as_str())]);
                        }
                    }
                }
            });
            frame.section(&text("welcome-scratch"), |frame| {
                let mut scratch = session.scratch_text();
                if frame.text_edit(&mut scratch) {
                    event = PanelEvent::Scratch(scratch);
                }
            });
        });
    });
    event
}

const OUTLINER_TYPE_FILTERS: &[(&str, seal_ui::Icon)] = &[
    ("folder", seal_ui::Icon::EntityFolder),
    ("light", seal_ui::Icon::EntityLightPoint),
    ("camera", seal_ui::Icon::EntityCamera),
    ("cube", seal_ui::Icon::EntityCube),
    ("sphere", seal_ui::Icon::EntitySphere),
    ("plane", seal_ui::Icon::EntityPlane),
    ("empty", seal_ui::Icon::EntityEmpty),
];

fn outliner_filter_type_label(text: &impl Fn(&str) -> String, kind: &str) -> String {
    text(&format!("outliner-filter-type-{kind}"))
}

fn outliner_filter_menu(
    ui: &mut egui::Ui,
    filters: &seal_editor_core::OutlinerActiveFilters,
    text: &impl Fn(&str) -> String,
) -> Option<PanelEvent> {
    ui.label(egui::RichText::new(text("outliner-filter-types-caption")).small().weak());
    let mut event = None;
    for (kind, icon) in OUTLINER_TYPE_FILTERS {
        if event.is_some() {
            break;
        }
        let label = outliner_filter_type_label(text, kind);
        let on = filters.types.contains(*kind);
        let mut check = on;
        ui.horizontal(|ui| {
            let (icon_rect, _) =
                ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
            seal_ui::paint_icon(
                ui.painter(),
                icon_rect.center(),
                *icon,
                14.0,
                ui.visuals().text_color(),
            );
            if ui.checkbox(&mut check, label).changed() {
                event = Some(command(
                    "outliner.filter_toggle",
                    &[("id", &format!("type:{kind}")), ("on", if check { "1" } else { "0" })],
                ));
            }
        });
    }
    ui.separator();
    ui.label(egui::RichText::new(text("outliner-filter-other-caption")).small().weak());
    let mut flag = |id: &'static str, key: &str, on: bool| {
        if event.is_some() {
            return;
        }
        let mut check = on;
        if ui.checkbox(&mut check, text(key)).changed() {
            event = Some(command(
                "outliner.filter_toggle",
                &[("id", &format!("flag:{id}")), ("on", if check { "1" } else { "0" })],
            ));
        }
    };
    flag("only_selected", "outliner-filter-only-selected", filters.only_selected);
    flag("only_visible", "outliner-filter-only-visible", filters.only_visible);
    flag("only_unsaved", "outliner-filter-only-unsaved", filters.only_unsaved);
    flag("only_warnings", "outliner-filter-only-warnings", filters.only_warnings);
    event
}

fn outliner_filter_chips(
    frame: &mut PanelFrame,
    filters: &seal_editor_core::OutlinerActiveFilters,
    text: &impl Fn(&str) -> String,
) -> Option<PanelEvent> {
    if !filters.any() {
        return None;
    }
    let mut event = None;
    frame.ui_mut().horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let mut chip_click = |id: &str, label: String| {
            if event.is_some() {
                return;
            }
            let chip_label = format!("{label} ×");
            let response = ui.add(
                egui::Button::new(egui::RichText::new(chip_label).size(12.0))
                    .corner_radius(0.0)
                    .min_size(egui::vec2(0.0, 20.0)),
            );
            let remove_tip = text("outliner-filter-chip-remove");
            response.clone().on_hover_text(remove_tip);
            if response.clicked() || response.secondary_clicked() {
                event = Some(command("outliner.filter_toggle", &[("id", id), ("on", "0")]));
            }
        };
        for kind in &filters.types {
            let label = outliner_filter_type_label(text, kind.as_str());
            chip_click(&format!("type:{kind}"), label);
        }
        if filters.only_selected {
            chip_click("flag:only_selected", text("outliner-filter-only-selected"));
        }
        if filters.only_visible {
            chip_click("flag:only_visible", text("outliner-filter-only-visible"));
        }
        if filters.only_unsaved {
            chip_click("flag:only_unsaved", text("outliner-filter-only-unsaved"));
        }
        if filters.only_warnings {
            chip_click("flag:only_warnings", text("outliner-filter-only-warnings"));
        }
    });
    event
}

fn outliner_view_option_menu(
    ui: &mut egui::Ui,
    view: &seal_editor_core::OutlinerViewOptions,
    text: &impl Fn(&str) -> String,
) -> Option<PanelEvent> {
    ui.label(egui::RichText::new(text("outliner-view-options-caption")).small().weak());
    let mut event = None;
    let mut toggle = |id: &'static str, key: &str, on: bool| {
        if event.is_some() {
            return;
        }
        let mut check = on;
        if ui.checkbox(&mut check, text(key)).changed() {
            event = Some(command(
                "outliner.view_option",
                &[("id", id), ("on", if check { "1" } else { "0" })],
            ));
        }
    };
    toggle("hierarchical", "outliner-view-hierarchical", view.hierarchical);
    toggle("only_selected", "outliner-view-only-selected", view.only_selected);
    toggle("hide_temporary", "outliner-view-hide-temporary", view.hide_temporary);
    toggle("only_current_level", "outliner-view-only-level", view.only_current_level);
    toggle("hide_folders", "outliner-view-hide-folders", view.hide_folders);
    toggle("stack_headers", "outliner-view-stack-headers", view.stack_headers);
    ui.separator();
    if event.is_none() && ui.button(text("command-outliner-expand")).clicked() {
        event = Some(command("outliner.expand_all", &[]));
    }
    if event.is_none() && ui.button(text("command-outliner-collapse")).clicked() {
        event = Some(command("outliner.collapse_all", &[]));
    }
    if event.is_none() {
        let _ = ui.menu_button(text("outliner-view-choose-world"), |ui| {
            ui.label(text("outliner-view-world-current"));
        });
    }
    event
}

fn outliner(frame: &mut PanelFrame, session: &mut EditorSession, panel_id: &str) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let mut event = PanelEvent::None;
    frame.bar(|frame| {
        let filter_tip = text("outliner-filter-menu");
        let filters = &session.models.outliner_filters;
        let _ = frame.icon_menu(seal_ui::Icon::EditFilter, &filter_tip, |ui| {
            ui.set_min_width(200.0);
            if let Some(pick) = outliner_filter_menu(ui, filters, &text) {
                event = pick;
            }
        });
        let mut query = session.outliner_query_for(panel_id);
        let clear = text("outliner-search-clear");
        // Room for the folder button and the settings dropdown that follow on this row.
        let trailing = 2.0 * (seal_ui::TOOL_ROW_PX + 8.0) + 16.0;
        if frame.panel_search_leaving(&mut query, &text("outliner-search-hint"), &clear, trailing) {
            event = command("outliner.filter", &[("query", query.as_str())]);
        }
        let folder_tip = text("command-outliner-new-folder");
        if frame.icon_button(seal_ui::Icon::EntityFolder, &folder_tip) {
            event = command("outliner.new_folder", &[]);
        }
        let settings_tip = text("outliner-settings-menu");
        let view = &session.models.outliner_view;
        let _ = frame.icon_menu(seal_ui::Icon::EditSettings, &settings_tip, |ui| {
            ui.set_min_width(220.0);
            if let Some(pick) = outliner_view_option_menu(ui, view, &text) {
                event = pick;
            }
        });
    });
    if session.models.outliner_filters.any() {
        frame.bar(|frame| {
            if let Some(pick) =
                outliner_filter_chips(frame, &session.models.outliner_filters, &text)
            {
                if matches!(event, PanelEvent::None) {
                    event = pick;
                }
            }
        });
    }
    let query = session.outliner_query_for(panel_id);
    // A search lists every match flat, so a hit inside a closed branch still shows.
    let selection = session.foundation.selection.clone();
    let table = session.models.outliner_table.clone();
    let entity_count = session.world.entities.len();
    let col_label = text("outliner-col-label");
    let col_type = text("outliner-col-type");
    let columns_caption = text("outliner-columns-caption");
    let optional_cols = [
        ("layer", text("outliner-col-layer")),
        ("tags", text("outliner-col-tags")),
        ("motion_mode", text("outliner-col-motion-mode")),
        ("source", text("outliner-col-source")),
        ("id", text("outliner-col-id")),
        ("unsaved", text("outliner-col-unsaved")),
        ("level", text("outliner-col-level")),
        ("unselectable", text("outliner-col-unselectable")),
    ];
    let menu_pairs: Vec<(String, String)> =
        optional_cols.iter().map(|(id, title)| (id.to_string(), title.clone())).collect();
    let menu_refs: Vec<(&str, &str)> =
        menu_pairs.iter().map(|(id, title)| (id.as_str(), title.as_str())).collect();
    let header_labels = seal_ui::OutlinerHeaderLabels {
        item_label: &col_label,
        ty: &col_type,
        columns_caption: &columns_caption,
        optional: &[],
        column_menu: &menu_refs,
    };
    frame.ui_mut().spacing_mut().interact_size.y = seal_ui::ROW_PX;
    match seal_ui::paint_outliner_header(frame.ui_mut(), &table, header_labels) {
        seal_ui::OutlinerHeaderHit::Sort(column) => {
            event = command("outliner.sort", &[("column", column.as_str())]);
        }
        seal_ui::OutlinerHeaderHit::ResizeType(width) => {
            event = command("outliner.resize_type", &[("width", &width.to_string())]);
        }
        seal_ui::OutlinerHeaderHit::ToggleColumn { id, on } => {
            event = command(
                "outliner.column",
                &[("id", id.as_str()), ("on", if on { "1" } else { "0" })],
            );
        }
        seal_ui::OutlinerHeaderHit::None => {}
    }
    let ctx_entries = outliner_menu::outliner_context_entries(session);
    let mut rename_local = String::new();
    let rename_id = session.models.selection.as_ref().and_then(|id| {
        let key = format!("rename:{id}");
        session.draft(&key).map(|draft| {
            rename_local = draft;
            id.clone()
        })
    });
    frame.space(4.0);
    let panel_focused = session.foundation.focused_panel_id.starts_with("outliner");
    let scroll_target = session.models.outliner_scroll_to.clone();
    let mut row_event = PanelEvent::None;
    let mut initial_scroll_y = session.models.take_outliner_initial_scroll_y();
    session.models.prepare_outliner_visible_rows(&query, &selection);
    let visible = session.models.outliner_visible_rows_slice();
    if initial_scroll_y.is_none() {
        if let Some(target) = scroll_target.as_deref() {
            if let Some(index) = visible.iter().position(|row| row.entity_id() == target) {
                initial_scroll_y = Some(index as f32 * seal_ui::ROW_PX);
            }
        }
    }
    let _click = frame.tree_list(visible.len(), initial_scroll_y, |frame, index| {
        let Some(row_id) = visible.get(index) else {
            return TreeClick::None;
        };
        if let seal_editor_core::OutlinerVisibleRow::Component { entity, component } = row_id {
            let (icon, rgb) = outliner_component_glyph(component.as_str());
            let label_key = format!("outliner-component-{component}");
            let type_key = format!("outliner-component-type-{component}");
            let comp_label = text(label_key.as_str());
            let comp_type = text(type_key.as_str());
            let level = depth(session, entity) + 1;
            let selected = session.models.selection.as_deref() == Some(entity.as_str());
            let args = [("id", entity.as_str())];
            let cells_ref: Vec<(&str, &str)> = Vec::new();
            match seal_ui::paint_outliner_row(
                frame.ui_mut(),
                &table,
                seal_ui::TreeRow {
                    depth: level,
                    has_children: false,
                    open: false,
                    label: &comp_label,
                    detail: "",
                    selected,
                    hidden: false,
                    locked: false,
                    menu: &[],
                    drag_id: None,
                    place_kind: None,
                    rename_text: None,
                    component_row: true,
                    thumb: None,
                    marks: true,
                },
                seal_ui::OutlinerRowExtra {
                    type_text: &comp_type,
                    type_icon: icon,
                    type_rgb: rgb,
                    type_asset: None,
                    row_marks: seal_ui::OutlinerRowMarksView {
                        unsaved: false,
                        prefab_badge: false,
                        play_spawned: false,
                        warning_row: false,
                        warning_tooltip: None,
                    },
                    label_suffix: None,
                    cells: &cells_ref,
                    rename_empty_hint: None,
                    context_menu: None,
                    context_caption: None,
                },
                panel_focused,
                false,
                &session.models,
                &session.messages,
            ) {
                TreeClick::Select => row_event = command("outliner.select", &args),
                TreeClick::SelectExtend => row_event = command("outliner.select-extend", &args),
                TreeClick::SelectRange => row_event = command("outliner.range_select", &args),
                _ => {}
            }
            return TreeClick::None;
        }
        let seal_editor_core::OutlinerVisibleRow::Entity(id) = row_id else {
            return TreeClick::None;
        };
        let id = id.as_str();
        let Some(node) = session.models.nodes.iter().find(|node| node.id == id) else {
            return TreeClick::None;
        };
        let selected = session.foundation.selection.iter().any(|item| item == id)
            || session.models.selection.as_deref() == Some(id);
        let open = session.models.expanded.iter().any(|item| item == id);
        let components = seal_editor_core::outliner_component_keys(node);
        let has_children = query.is_empty()
            && (!session.models.children(Some(id)).is_empty() || !components.is_empty());
        let level = if query.is_empty() { depth(session, id) } else { 0 };
        let shape = field(node, "shape");
        let role = field(node, "role");
        let is_folder = role == "folder";
        let mut folder_suffix = None;
        let (row_label, type_label, type_icon, type_rgb) = if node.id == "root" {
            (
                session
                    .messages
                    .format("outliner-root-label", &[("name", session.scene_file_name().as_str())]),
                text("outliner-type-world"),
                seal_ui::Icon::ToolWorld,
                [76, 141, 255],
            )
        } else if is_folder {
            let count = session.models.children(Some(id)).len();
            if has_children && !open && count > 0 {
                folder_suffix = Some(
                    session
                        .messages
                        .format("outliner-folder-child-count", &[("count", &count.to_string())]),
                );
            }
            (
                node.name.clone(),
                text("outliner-type-folder"),
                if open { seal_ui::Icon::FolderOpen } else { seal_ui::Icon::Folder },
                [212, 167, 86],
            )
        } else {
            let source_kind = field(node, "source_kind");
            let type_name = seal_editor_core::outliner_type_display(node);
            let (type_icon, type_rgb) = match source_kind {
                "prefab" => (seal_ui::Icon::EntityPrefab, [76, 141, 255]),
                "script" => (seal_ui::Icon::EntityScript, [138, 160, 184]),
                _ if !field(node, "sprite").is_empty() => {
                    (seal_ui::Icon::EntitySprite, [120, 200, 140])
                }
                _ if matches!(field(node, "mask_shape"), "rect" | "circle" | "alpha") => {
                    (seal_ui::Icon::EntitySprite, [89, 184, 219])
                }
                _ => (
                    seal_ui::outliner_entity_icon(shape, role),
                    seal_ui::outliner_entity_tint(shape, role),
                ),
            };
            (node.name.clone(), type_name, type_icon, type_rgb)
        };
        let row_mark_data = seal_editor_core::outliner_row_marks(node);
        let warning_tooltip = row_mark_data.warning_key.as_deref().map(text);
        let row_marks = seal_ui::OutlinerRowMarksView {
            unsaved: row_mark_data.unsaved,
            prefab_badge: row_mark_data.prefab_badge,
            play_spawned: row_mark_data.play_spawned,
            warning_row: row_mark_data.warning_key.is_some(),
            warning_tooltip: warning_tooltip.as_deref(),
        };
        let asset_edit = seal_editor_core::outliner_asset_edit(node);
        let mut type_link_label = String::new();
        let type_asset = asset_edit.as_ref().map(|edit| {
            type_link_label =
                session.messages.format("outliner-edit-asset", &[("asset", edit.asset)]);
            seal_ui::OutlinerTypeAssetLink {
                label: &type_link_label,
                kind: edit.kind,
                asset: edit.asset,
            }
        });
        let depth_text = level.to_string();
        let motion = field(node, "motion");
        let unselectable = field(node, "unselectable") == "yes";
        let optional_cells: Vec<(&str, String)> = table
            .visible_optional
            .iter()
            .map(|col| {
                let value = match col.as_str() {
                    "layer" => motion.to_string(),
                    "tags" => field(node, "role").to_string(),
                    "motion_mode" => node_motion_mode(node).to_string(),
                    "source" => String::new(),
                    "id" => node.id.clone(),
                    "unsaved" => {
                        if field(node, "unsaved") == "yes" {
                            "*".to_string()
                        } else {
                            String::new()
                        }
                    }
                    "level" => depth_text.clone(),
                    "unselectable" => {
                        if unselectable {
                            "yes".to_string()
                        } else {
                            String::new()
                        }
                    }
                    _ => String::new(),
                };
                (col.as_str(), value)
            })
            .collect();
        let cells_ref: Vec<(&str, &str)> =
            optional_cells.iter().map(|(id, value)| (*id, value.as_str())).collect();
        let row_context = if node.id == "root" {
            None
        } else {
            Some((ctx_entries.as_slice(), node.name.as_str()))
        };
        let args = [("id", node.id.as_str())];
        let hidden = field(node, "hidden") == "yes";
        let locked = field(node, "locked") == "yes";
        let rename_text =
            if rename_id.as_deref() == Some(id) { Some(&mut rename_local) } else { None };
        let drag_id = if node.id == "root" { None } else { Some(node.id.as_str()) };
        let scroll_into_view = scroll_target.as_deref() == Some(id);
        match seal_ui::paint_outliner_row(
            frame.ui_mut(),
            &table,
            seal_ui::TreeRow {
                depth: level,
                has_children,
                open,
                label: &row_label,
                detail: "",
                selected,
                hidden,
                locked,
                menu: &[],
                drag_id,
                place_kind: None,
                rename_text,
                component_row: false,
                thumb: None,
                marks: true,
            },
            seal_ui::OutlinerRowExtra {
                type_text: &type_label,
                type_icon,
                type_rgb,
                type_asset,
                row_marks,
                label_suffix: folder_suffix.as_deref(),
                cells: &cells_ref,
                rename_empty_hint: Some(&text("outliner-rename-empty")),
                context_menu: row_context.map(|(entries, _)| entries),
                context_caption: row_context.map(|(_, caption)| caption),
            },
            panel_focused,
            scroll_into_view,
            &session.models,
            &session.messages,
        ) {
            TreeClick::Select => row_event = command("outliner.select", &args),
            TreeClick::SelectExtend => row_event = command("outliner.select-extend", &args),
            TreeClick::SelectRange => row_event = command("outliner.range_select", &args),
            TreeClick::Eye => row_event = command("scene.hide", &args),
            TreeClick::EyeSolo => row_event = command("scene.hide_solo", &args),
            TreeClick::Lock => row_event = command("scene.lock", &args),
            TreeClick::Unselectable => row_event = command("scene.unselectable", &args),
            TreeClick::Toggle => row_event = command("outliner.toggle", &args),
            TreeClick::FrameEntity => {
                row_event = PanelEvent::Commands(vec![
                    ("outliner.select".into(), CommandArgs::new(&args)),
                    ("viewport.frame".into(), CommandArgs::new(&[("id", node.id.as_str())])),
                ]);
            }
            TreeClick::EditAsset { kind, asset } => {
                row_event =
                    command("asset.edit", &[("kind", kind.as_str()), ("asset", asset.as_str())]);
            }
            TreeClick::Reparent { child, parent } => {
                row_event = command(
                    "outliner.reparent",
                    &[("id", child.as_str()), ("parent", parent.as_str())],
                );
            }
            TreeClick::Reorder { child, before, after } => {
                let mut args = vec![("id", child.as_str())];
                if let Some(id) = before.as_deref() {
                    args.push(("before", id));
                }
                if let Some(id) = after.as_deref() {
                    args.push(("after", id));
                }
                row_event = command("outliner.reorder", &args);
            }
            TreeClick::SpawnPlace { parent, kind } => {
                row_event =
                    command("scene.add", &[("kind", kind.as_str()), ("parent", parent.as_str())]);
            }
            TreeClick::Rename(name) => {
                row_event = command(
                    "outliner.rename",
                    &[("id", node.id.as_str()), ("name", name.as_str())],
                );
            }
            TreeClick::BeginRename => {
                if node.id != "root" {
                    row_event = PanelEvent::Commands(vec![
                        ("outliner.select".into(), CommandArgs::new(&args)),
                        ("edit.rename".into(), CommandArgs::default()),
                    ]);
                }
            }
            TreeClick::RenameCancel => {
                if rename_id.as_deref() == Some(id) {
                    row_event = command("draft.forget", &[("key", &format!("rename:{}", node.id))]);
                }
            }
            TreeClick::ContextPick(pick) => {
                let args: Vec<(&str, &str)> =
                    pick.args.iter().map(|(key, value)| (key.as_str(), value.as_str())).collect();
                row_event = command(pick.command.as_str(), &args);
            }
            TreeClick::Menu(_) | TreeClick::None => {}
        };
        TreeClick::None
    });
    if matches!(event, PanelEvent::None) && !matches!(row_event, PanelEvent::None) {
        event = row_event;
    }
    if matches!(event, PanelEvent::None)
        && rename_id.is_some()
        && rename_id.as_deref() == session.models.selection.as_deref()
    {
        if let Some(id) = rename_id.as_deref() {
            event = command(
                "draft.set",
                &[("key", &format!("rename:{id}")), ("value", rename_local.as_str())],
            );
        }
    }
    let footer_event = std::cell::RefCell::new(PanelEvent::None);
    frame.footer(|frame| {
        let total = entity_count.to_string();
        let picked = session.selected_ids().len().to_string();
        let status =
            session.messages.format("outliner-status", &[("total", &total), ("picked", &picked)]);
        let ui = frame.ui_mut();
        ui.label(egui::RichText::new(status).small().weak());
        ui.allocate_space(egui::vec2(ui.available_width().max(0.0), 1.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let view_tip = text("outliner-view-options");
            let view = &session.models.outliner_view;
            ui.label(egui::RichText::new(view_tip.as_str()).small());
            let mut menu_frame = PanelFrame::new(ui);
            let _ = menu_frame.icon_menu(seal_ui::Icon::ViewEye, view_tip.as_str(), |ui| {
                ui.set_min_width(220.0);
                if let Some(pick) = outliner_view_option_menu(ui, view, &text) {
                    *footer_event.borrow_mut() = pick;
                }
            });
        });
    });
    let footer_pick = footer_event.into_inner();
    if matches!(event, PanelEvent::None) && !matches!(footer_pick, PanelEvent::None) {
        event = footer_pick;
    }
    event
}

fn field<'n>(node: &'n seal_editor_core::MockNode, name: &str) -> &'n str {
    node.fields.iter().find(|(key, _)| key == name).map_or("", |(_, value)| value.as_str())
}

fn outliner_component_glyph(component: &str) -> (seal_ui::Icon, [u8; 3]) {
    let dim = [156, 164, 176];
    let icon = match component {
        "light" => seal_ui::Icon::EntityLightDirectional,
        "camera" => seal_ui::Icon::EntityCamera,
        "mesh" => seal_ui::Icon::EntityCube,
        "sprite" => seal_ui::Icon::AssetTexture,
        "particles" => seal_ui::Icon::AssetParticles,
        "script" => seal_ui::Icon::AssetScript,
        _ => seal_ui::Icon::ToolTransform,
    };
    (icon, dim)
}

fn depth(session: &EditorSession, id: &str) -> usize {
    let parent_of = |id: &str| {
        session.models.nodes.iter().find(|node| node.id == id).and_then(|node| node.parent.clone())
    };
    let mut depth = 0;
    let mut cursor = parent_of(id);
    while let Some(parent) = cursor {
        depth += 1;
        if depth > 12 {
            break;
        }
        cursor = parent_of(&parent);
    }
    depth
}

fn inspector_target_ids(session: &EditorSession) -> Vec<String> {
    if session.foundation.selection.len() >= 2 {
        session.foundation.selection.clone()
    } else if let Some(id) = &session.models.selection {
        vec![id.clone()]
    } else {
        Vec::new()
    }
}

fn inspector_targets(session: &EditorSession) -> Vec<&seal_editor_core::MockNode> {
    inspector_target_ids(session)
        .iter()
        .filter_map(|id| session.models.nodes.iter().find(|node| &node.id == id))
        .collect()
}

/// Motion as the editor shows it: the stored value, else the physics motion when it
/// names a motion_mode, else the default. Outliner column and Inspector row agree.
fn node_motion_mode(node: &seal_editor_core::MockNode) -> &str {
    let stored = field(node, "motion_mode");
    if !stored.is_empty() {
        return stored;
    }
    match field(node, "motion") {
        motion @ ("static" | "mixed" | "dynamic") => motion,
        _ => seal_editor_core::inspector_field_default("motion_mode"),
    }
}

fn shared_field(nodes: &[&seal_editor_core::MockNode], name: &str) -> Option<String> {
    let mut values = nodes.iter().map(|node| field(node, name).to_string());
    let first = values.next()?;
    if values.all(|value| value == first) {
        Some(first)
    } else {
        None
    }
}

fn inspector_ids_arg(ids: &[String]) -> String {
    ids.join(",")
}

fn inspector_section_open(session: &EditorSession, key: &str) -> bool {
    let stored = session.models.inspector_sections.get(key).copied().unwrap_or(true);
    let q = session.models.inspector_query.to_lowercase();
    if q.is_empty() {
        return stored;
    }
    stored
        || seal_editor_core::inspector_section_match_keys(key).iter().any(|k| {
            k.to_lowercase().contains(&q) || session.messages.get(k).to_lowercase().contains(&q)
        })
}

fn validate_unit(
    text: &str,
    min: f32,
    max: f32,
    range_key: &str,
    session: &EditorSession,
) -> Option<String> {
    let Ok(value) = text.parse::<f32>() else {
        return Some(session.messages.get("inspector-invalid-number"));
    };
    if !(min..=max).contains(&value) {
        return Some(
            session
                .messages
                .format(range_key, &[("min", &min.to_string()), ("max", &max.to_string())]),
        );
    }
    None
}

fn inspector(frame: &mut PanelFrame, session: &EditorSession, panel_id: &str) -> PanelEvent {
    if !session.models.inspector_locks.contains_key(panel_id) {
        let entity_picked =
            session.models.selection.is_some() || !session.foundation.selection.is_empty();
        if let Some(rel) = session.models.content_pick.as_ref() {
            if seal_editor_core::is_data_path(rel) && !entity_picked {
                return data_asset_inspector::paint_data_inspector(frame, session, rel);
            }
        }
    }
    let targets = if let Some(locked) = session.models.inspector_locks.get(panel_id) {
        session.models.nodes.iter().find(|node| &node.id == locked).into_iter().collect::<Vec<_>>()
    } else {
        inspector_targets(session)
    };
    let Some(primary) = targets.first().copied() else {
        frame.empty(&session.messages.get("inspector-empty"));
        return PanelEvent::None;
    };
    let text = |key: &str| session.messages.get(key);
    let ids = if session.models.inspector_locks.contains_key(panel_id) {
        targets.iter().map(|node| node.id.clone()).collect::<Vec<_>>()
    } else {
        inspector_target_ids(session)
    };
    let id_arg = inspector_ids_arg(&ids);
    let multi = ids.len() > 1;
    let id = primary.id.as_str();
    let mut event = PanelEvent::None;
    let script_layout = session.project.as_ref().and_then(|project| {
        let script = shared_field(&targets, "script").unwrap_or_default();
        seal_editor_core::script_inspector::layout_for_script(&project.root, &script)
    });
    let visible = |label_key: &str, fields: &[&str]| {
        inspector_visible(session, label_key, fields, &targets)
            && fields
                .iter()
                .all(|field| !script_layout.as_ref().is_some_and(|layout| layout.is_hidden(field)))
    };
    let has_shape = targets
        .iter()
        .any(|node| field(node, "shape") != "none" && !field(node, "shape").is_empty());
    let all_lights = targets.iter().all(|node| field(node, "role") == "light");
    let all_cameras = targets.iter().all(|node| field(node, "role") == "camera");
    // Cameras and lights have no body: no shape to pick and nothing to simulate.
    let any_body = targets.iter().any(|node| !matches!(field(node, "role"), "camera" | "light"));
    let category_active = session.models.inspector_category.as_str();
    let category_ok =
        |section: &str| inspector_panel::inspector_category_matches(section, category_active);
    let highlight = session.models.inspector_query.as_str();
    let highlight_q = if highlight.is_empty() { None } else { Some(highlight) };
    frame.padded(|outer| {
        inspector_panel::inspector_header_row(
            outer, session, panel_id, primary, &ids, multi, &mut event, &text,
        );
        inspector_panel::inspector_component_tree(
            outer, session, panel_id, primary, multi, &mut event, &text,
        );
        outer.space(6.0);
        let component_pick = session.models.component_pick(panel_id, &primary.id);
        let show_section =
            |section: &str| inspector_panel::inspector_section_visible(component_pick, section);
        inspector_panel::inspector_filter_bar(outer, session, panel_id, &mut event, &text);
        inspector_panel::inspector_category_bar(
            outer,
            session,
            component_pick,
            has_shape,
            all_lights,
            all_cameras,
            &mut event,
            &text,
        );
        outer.scroll(|frame| {
            frame.begin_search_highlight(highlight);
            frame.padded(|frame| {
                let mixed = text("inspector-mixed");
                let set_target = if multi { ("ids", id_arg.as_str()) } else { ("id", id) };
                let reset_target = set_target;
                let prop_menu_entries =
                    inspector_property_menu::inspector_property_context_entries(session);
                let prop_menu = prop_menu_entries.as_slice();
                let mut prop_menu_pick: Option<ContextMenuPick> = None;
                if category_ok("inspector-transform")
                    && show_section("inspector-transform")
                    && visible(
                        "inspector-transform",
                        &["x", "y", "z", "rx", "ry", "rz", "sx", "sy", "sz", "motion_mode"],
                    )
                {
                    if let Some(open) = frame.section_keyed_with_highlight(
                        "inspector-transform",
                        &text("inspector-transform"),
                        inspector_section_open(session, "inspector-transform"),
                        |frame| {
                            if visible("inspector-visible", &["hidden"]) {
                                let check = inspector_visible_checkbox(&targets, multi);
                                let modified =
                                    targets.iter().any(|node| field(node, "hidden") == "yes");
                                if let Some(next) = frame.property_checkbox_row(
                                    &text("inspector-visible"),
                                    check,
                                    modified,
                                    None,
                                    Some(prop_menu),
                                    &mut prop_menu_pick,
                                    None,
                                ) {
                                    let hidden = match next {
                                        seal_ui::PropertyCheckbox::On => "no",
                                        seal_ui::PropertyCheckbox::Off => "yes",
                                        seal_ui::PropertyCheckbox::Mixed => "no",
                                    };
                                    event = command(
                                        "inspector.set",
                                        &[set_target, ("field", "hidden"), ("value", hidden)],
                                    );
                                }
                            }
                            if visible("inspector-location", &["x", "y", "z"]) {
                                let axes = ["x", "y", "z"];
                                let mut location = axes.map(|axis| {
                                    shared_field(&targets, axis).unwrap_or_else(|| mixed.clone())
                                });
                                let modified =
                                    axes.map(|axis| inspector_field_modified(&targets, axis));
                                let tips: [String; 3] =
                                    axes.map(|axis| inspector_reset_tip(session, axis));
                                let tip_refs =
                                    [tips[0].as_str(), tips[1].as_str(), tips[2].as_str()];
                                let loc_help = inspector_panel::inspector_property_help(
                                    session,
                                    id,
                                    "inspector-location",
                                    "x",
                                    "-1000000",
                                    "1000000",
                                );
                                let abs_label = text("inspector-location-absolute");
                                let rel_label = text("inspector-location-relative");
                                let space_options = [
                                    ("absolute", abs_label.as_str()),
                                    ("relative", rel_label.as_str()),
                                ];
                                let m_unit = text("inspector-unit-meters");
                                let vector_mode = seal_ui::VectorRowMode {
                                    current: session.models.inspector_location_space.as_str(),
                                    options: &space_options,
                                };
                                let (changed, reset, commit_focus, space_pick, _) = frame
                                    .vector_row(
                                        &text("inspector-location"),
                                        &mut location,
                                        modified.iter().any(|m| *m),
                                        Some((&modified, &tip_refs)),
                                        Some(loc_help.as_str()),
                                        Some(prop_menu),
                                        &mut prop_menu_pick,
                                        seal_ui::VectorRowStyle {
                                            mode: Some(vector_mode),
                                            unit_suffix: Some(m_unit.as_str()),
                                            ..Default::default()
                                        },
                                    );
                                if let Some(space) = space_pick {
                                    event = command(
                                        "inspector.vector_space",
                                        &[("space", space.as_str())],
                                    );
                                }
                                if let Some(pick) = prop_menu_pick.take() {
                                    event = inspector_panel::apply_property_menu_pick(pick);
                                }
                                if let Some(index) = changed {
                                    if shared_field(&targets, axes[index]).is_some() || !multi {
                                        if let Some(err) = validate_unit(
                                            &location[index],
                                            -1e6,
                                            1e6,
                                            "inspector-invalid-range",
                                            session,
                                        ) {
                                            frame.muted(&err);
                                        } else {
                                            event = inspector_text_field_edit(
                                                session,
                                                set_target,
                                                axes[index],
                                                location[index].as_str(),
                                                true,
                                                commit_focus,
                                            );
                                        }
                                    }
                                } else if commit_focus {
                                    if let Some(edit) = &session.models.inspector_field_edit {
                                        if let Some(index) =
                                            axes.iter().position(|axis| *axis == edit.field)
                                        {
                                            event = inspector_text_field_edit(
                                                session,
                                                set_target,
                                                axes[index],
                                                location[index].as_str(),
                                                false,
                                                true,
                                            );
                                        }
                                    }
                                } else if let Some(index) = reset {
                                    // Each axis has its own reset button.
                                    event = command(
                                        "inspector.reset_field",
                                        &[reset_target, ("field", axes[index])],
                                    );
                                }
                            }
                            if visible("inspector-rotation", &["rx", "ry", "rz"]) {
                                let axes = ["rx", "ry", "rz"];
                                let mut rotation = axes.map(|axis| {
                                    shared_field(&targets, axis).unwrap_or_else(|| mixed.clone())
                                });
                                let modified =
                                    axes.map(|axis| inspector_field_modified(&targets, axis));
                                let tips: [String; 3] =
                                    axes.map(|axis| inspector_reset_tip(session, axis));
                                let tip_refs =
                                    [tips[0].as_str(), tips[1].as_str(), tips[2].as_str()];
                                let degree = inspector_field_row::rotation_unit_key("rx")
                                    .map(text)
                                    .unwrap_or_else(|| text("inspector-unit-degrees"));
                                let (changed, reset, commit_focus) = frame.rotation_row(
                                    &text("inspector-rotation"),
                                    &mut rotation,
                                    modified.iter().any(|m| *m),
                                    Some((&modified, &tip_refs)),
                                    None,
                                    Some(prop_menu),
                                    &mut prop_menu_pick,
                                    degree.as_str(),
                                );
                                if let Some(pick) = prop_menu_pick.take() {
                                    event = inspector_panel::apply_property_menu_pick(pick);
                                }
                                if let Some(index) = changed {
                                    let field_name = axes[index];
                                    if shared_field(&targets, field_name).is_some() || !multi {
                                        event = inspector_text_field_edit(
                                            session,
                                            set_target,
                                            field_name,
                                            rotation[index].as_str(),
                                            true,
                                            commit_focus,
                                        );
                                    }
                                } else if commit_focus {
                                    if let Some(edit) = &session.models.inspector_field_edit {
                                        if let Some(index) =
                                            axes.iter().position(|axis| *axis == edit.field)
                                        {
                                            event = inspector_text_field_edit(
                                                session,
                                                set_target,
                                                axes[index],
                                                rotation[index].as_str(),
                                                false,
                                                true,
                                            );
                                        }
                                    }
                                } else if let Some(index) = reset {
                                    // Each axis has its own reset button.
                                    event = command(
                                        "inspector.reset_field",
                                        &[reset_target, ("field", axes[index])],
                                    );
                                }
                            }
                            if visible("inspector-scale", &["sx", "sy", "sz"]) {
                                let axes = ["sx", "sy", "sz"];
                                let mut scale = axes.map(|axis| {
                                    shared_field(&targets, axis).unwrap_or_else(|| mixed.clone())
                                });
                                let modified =
                                    axes.map(|axis| inspector_field_modified(&targets, axis));
                                let tips: [String; 3] =
                                    axes.map(|axis| inspector_reset_tip(session, axis));
                                let tip_refs =
                                    [tips[0].as_str(), tips[1].as_str(), tips[2].as_str()];
                                let lock_tip = if session.models.inspector_scale_uniform {
                                    text("inspector-scale-uniform-unlock")
                                } else {
                                    text("inspector-scale-uniform-lock")
                                };
                                let (changed, reset, commit_focus, lock_toggle) = frame.scale_row(
                                    &text("inspector-scale"),
                                    &mut scale,
                                    modified.iter().any(|m| *m),
                                    Some((&modified, &tip_refs)),
                                    None,
                                    Some(prop_menu),
                                    &mut prop_menu_pick,
                                    session.models.inspector_scale_uniform,
                                    lock_tip.as_str(),
                                );
                                if lock_toggle {
                                    let locked = if session.models.inspector_scale_uniform {
                                        "0"
                                    } else {
                                        "1"
                                    };
                                    event =
                                        command("inspector.scale_uniform", &[("locked", locked)]);
                                }
                                if let Some(pick) = prop_menu_pick.take() {
                                    event = inspector_panel::apply_property_menu_pick(pick);
                                }
                                if let Some(index) = changed {
                                    let field_name = axes[index];
                                    if shared_field(&targets, field_name).is_some() || !multi {
                                        if session.models.inspector_scale_uniform {
                                            let synced = scale[index].clone();
                                            for slot in &mut scale {
                                                *slot = synced.clone();
                                            }
                                        }
                                        if let Some(err) = validate_unit(
                                            &scale[index],
                                            0.001,
                                            1e6,
                                            "inspector-invalid-range",
                                            session,
                                        ) {
                                            frame.muted(&err);
                                        } else {
                                            event = inspector_text_field_edit(
                                                session,
                                                set_target,
                                                field_name,
                                                scale[index].as_str(),
                                                true,
                                                commit_focus,
                                            );
                                        }
                                    }
                                } else if commit_focus {
                                    if let Some(edit) = &session.models.inspector_field_edit {
                                        if let Some(index) =
                                            axes.iter().position(|axis| *axis == edit.field)
                                        {
                                            event = inspector_text_field_edit(
                                                session,
                                                set_target,
                                                axes[index],
                                                scale[index].as_str(),
                                                false,
                                                true,
                                            );
                                        }
                                    }
                                } else if let Some(index) = reset {
                                    // Each axis has its own reset button.
                                    event = command(
                                        "inspector.reset_field",
                                        &[reset_target, ("field", axes[index])],
                                    );
                                }
                            }
                            if visible("inspector-motion-mode", &["motion_mode"]) {
                                let mut motion_modes =
                                    targets.iter().map(|node| node_motion_mode(node));
                                let first = motion_modes.next().unwrap_or_default();
                                let shared_motion_mode = motion_modes
                                    .all(|value| value == first)
                                    .then(|| first.to_string());
                                let current =
                                    shared_motion_mode.clone().unwrap_or_else(|| mixed.clone());
                                let modified = inspector_field_modified(&targets, "motion_mode");
                                let static_l = text("motion-mode-static");
                                let mixed_l = text("motion-mode-mixed");
                                let dynamic_l = text("motion-mode-dynamic");
                                let options = [
                                    ("static", static_l.as_str()),
                                    ("mixed", mixed_l.as_str()),
                                    ("dynamic", dynamic_l.as_str()),
                                ];
                                if !multi || shared_motion_mode.is_some() {
                                    if let Some(pick) = frame.motion_mode_row(
                                        &text("inspector-motion-mode"),
                                        &current,
                                        &options,
                                        modified,
                                        None,
                                        Some(prop_menu),
                                        &mut prop_menu_pick,
                                        Some("motion_mode"),
                                    ) {
                                        event = command(
                                            "inspector.set",
                                            &[
                                                set_target,
                                                ("field", "motion_mode"),
                                                ("value", pick.as_str()),
                                            ],
                                        );
                                    }
                                }
                            }
                            inspector_panel::inspector_section_advanced(
                                frame,
                                session,
                                "inspector-transform",
                                &mut event,
                                &text,
                                |frame| {
                                    frame.readout(&text("inspector-advanced-pivot"), "Center");
                                },
                            );
                        },
                        highlight_q,
                    ) {
                        event = command(
                            "inspector.section",
                            &[
                                ("id", "inspector-transform"),
                                ("open", if open { "1" } else { "0" }),
                            ],
                        );
                    }
                }
                if category_ok("inspector-material")
                    && show_section("inspector-material")
                    && has_shape
                    && visible("inspector-material", &["roughness", "metal"])
                {
                    if let Some(open) = frame.section_keyed_with_highlight(
                        "inspector-material",
                        &text("inspector-material"),
                        inspector_section_open(session, "inspector-material"),
                        |frame| {
                            for (label, key) in
                                [("inspector-roughness", "roughness"), ("inspector-metal", "metal")]
                            {
                                if !visible(label, &[key]) {
                                    continue;
                                }
                                if let Some(painted) = inspector_field_row::paint_metadata_number(
                                    frame,
                                    session,
                                    &targets,
                                    key,
                                    &text(label),
                                    set_target,
                                    reset_target,
                                    prop_menu,
                                    &mut prop_menu_pick,
                                ) {
                                    if !matches!(painted, PanelEvent::None) {
                                        event = painted;
                                    }
                                    continue;
                                }
                                let mixed_field = shared_field(&targets, key).is_none();
                                let mut value = shared_field(&targets, key)
                                    .and_then(|text| text.parse().ok())
                                    .unwrap_or(0.0);
                                let mut draft = value.to_string();
                                if mixed_field {
                                    draft = mixed.clone();
                                }
                                let field_error = validate_unit(
                                    &draft,
                                    0.0,
                                    1.0,
                                    "inspector-invalid-range",
                                    session,
                                );
                                let modified = inspector_field_modified(&targets, key);
                                let tip_owned = if modified {
                                    Some(inspector_reset_tip(session, key))
                                } else {
                                    None
                                };
                                let reset_ref = tip_owned.as_deref();
                                let (changed, reset, commit_focus) = frame.property_validated_row(
                                    &text(label),
                                    &mut draft,
                                    field_error.as_deref(),
                                    modified,
                                    reset_ref,
                                    None,
                                    Some(prop_menu),
                                    &mut prop_menu_pick,
                                    None,
                                );
                                if changed && !mixed_field {
                                    if let Ok(parsed) = draft.parse::<f32>() {
                                        value = parsed.clamp(0.0, 1.0);
                                        event = inspector_text_field_edit(
                                            session,
                                            set_target,
                                            key,
                                            &value.to_string(),
                                            true,
                                            commit_focus,
                                        );
                                    }
                                } else if commit_focus && !mixed_field {
                                    if session
                                        .models
                                        .inspector_field_edit
                                        .as_ref()
                                        .is_some_and(|edit| edit.field == key)
                                    {
                                        event = inspector_text_field_edit(
                                            session,
                                            set_target,
                                            key,
                                            draft.as_str(),
                                            false,
                                            true,
                                        );
                                    }
                                } else if reset {
                                    event = command(
                                        "inspector.reset_field",
                                        &[reset_target, ("field", key)],
                                    );
                                }
                            }
                            mesh_material_ui::paint(
                                frame, session, &targets, set_target, &mut event,
                            );
                            inspector_panel::inspector_section_advanced(
                                frame,
                                session,
                                "inspector-material",
                                &mut event,
                                &text,
                                |frame| {
                                    frame.readout(&text("inspector-advanced-uv"), "Channel 0");
                                },
                            );
                        },
                        highlight_q,
                    ) {
                        event = command(
                            "inspector.section",
                            &[("id", "inspector-material"), ("open", if open { "1" } else { "0" })],
                        );
                    }
                }
                if category_ok("inspector-light")
                    && show_section("inspector-light")
                    && all_lights
                    && visible("inspector-light", &["intensity"])
                {
                    if let Some(open) = frame.section_keyed_with_highlight(
                        "inspector-light",
                        &text("inspector-light"),
                        inspector_section_open(session, "inspector-light"),
                        |frame| {
                            if let Some(painted) = inspector_field_row::paint_metadata_number(
                                frame,
                                session,
                                &targets,
                                "intensity",
                                &text("inspector-intensity"),
                                set_target,
                                reset_target,
                                prop_menu,
                                &mut prop_menu_pick,
                            ) {
                                if !matches!(painted, PanelEvent::None) {
                                    event = painted;
                                }
                            }
                            if let Some(picked) = light2d_rows(
                                frame,
                                session,
                                &targets,
                                set_target,
                                reset_target,
                                prop_menu,
                                &mut prop_menu_pick,
                            ) {
                                event = picked;
                            }
                        },
                        highlight_q,
                    ) {
                        event = command(
                            "inspector.section",
                            &[("id", "inspector-light"), ("open", if open { "1" } else { "0" })],
                        );
                    }
                }
                if category_ok("inspector-camera")
                    && show_section("inspector-camera")
                    && all_cameras
                    && visible(
                        "inspector-camera",
                        &[
                            "fov",
                            "near",
                            "far",
                            "camera2d",
                            "cam_follow",
                            "cam_smoothing",
                            "cam_zoom",
                            "cam_limits",
                            "cam_pixels",
                            "cam_drag_left",
                            "cam_drag_right",
                            "cam_drag_top",
                            "cam_drag_bottom",
                        ],
                    )
                {
                    if let Some(open) = frame.section_keyed_with_highlight(
                        "inspector-camera",
                        &text("inspector-camera"),
                        inspector_section_open(session, "inspector-camera"),
                        |frame| {
                            for (label, key, speed, min, max) in [
                                ("inspector-fov", "fov", 1.0, 1.0, 179.0),
                                ("inspector-near", "near", 0.01, 0.001, 10.0),
                                ("inspector-far", "far", 1.0, 10.0, 100_000.0),
                            ] {
                                if !visible(label, &[key]) {
                                    continue;
                                }
                                if let Some(painted) = inspector_field_row::paint_metadata_number(
                                    frame,
                                    session,
                                    &targets,
                                    key,
                                    &text(label),
                                    set_target,
                                    reset_target,
                                    prop_menu,
                                    &mut prop_menu_pick,
                                ) {
                                    if !matches!(painted, PanelEvent::None) {
                                        event = painted;
                                    }
                                    continue;
                                }
                                let mixed_field = shared_field(&targets, key).is_none();
                                let mut value = shared_field(&targets, key)
                                    .and_then(|text| text.parse().ok())
                                    .unwrap_or(0.0);
                                let modified = inspector_field_modified(&targets, key);
                                let tip_owned = if modified {
                                    Some(inspector_reset_tip(session, key))
                                } else {
                                    None
                                };
                                if mixed_field {
                                    let mut draft = mixed.clone();
                                    frame.property_validated_row(
                                        &text(label),
                                        &mut draft,
                                        None,
                                        modified,
                                        tip_owned.as_deref(),
                                        None,
                                        Some(prop_menu),
                                        &mut prop_menu_pick,
                                        None,
                                    );
                                } else {
                                    let unit_suffix = match key {
                                        "fov" => Some(text("inspector-unit-degrees")),
                                        _ => None,
                                    };
                                    let field_spec = seal_ui::NumberFieldSpec {
                                        min: Some(min),
                                        max: Some(max),
                                        unit_suffix,
                                        range_bar: true,
                                        initial_text: None,
                                        ..Default::default()
                                    };
                                    let (changed, reset, drag) = frame.number_field_row(
                                        &text(label),
                                        &mut value,
                                        speed,
                                        modified,
                                        tip_owned.as_deref(),
                                        None,
                                        Some(prop_menu),
                                        &mut prop_menu_pick,
                                        &field_spec,
                                        None,
                                    );
                                    if changed
                                        || drag.drag_started
                                        || drag.drag_stopped
                                        || drag.escape_cancel
                                        || drag.commit_focus
                                    {
                                        event = inspector_panel::inspector_field_edit_event(
                                            session,
                                            set_target,
                                            key,
                                            &value.to_string(),
                                            changed,
                                            drag.drag_started,
                                            drag.drag_stopped,
                                            drag.escape_cancel,
                                            drag.commit_focus,
                                        );
                                    } else if reset {
                                        event = command(
                                            "inspector.reset_field",
                                            &[reset_target, ("field", key)],
                                        );
                                    }
                                }
                                let _ = (min, max);
                            }
                            if let Some(picked) = camera2d_rows(
                                frame,
                                session,
                                &targets,
                                set_target,
                                reset_target,
                                prop_menu,
                                &mut prop_menu_pick,
                            ) {
                                event = picked;
                            }
                        },
                        highlight_q,
                    ) {
                        event = command(
                            "inspector.section",
                            &[("id", "inspector-camera"), ("open", if open { "1" } else { "0" })],
                        );
                    }
                }
                if category_ok("inspector-camera")
                    && show_section("inspector-camera")
                    && !targets.is_empty()
                    && targets.iter().any(|node| {
                        field(node, "audio_listener") == "yes"
                            || field(node, "role") == "camera"
                            || field(node, "camera2d") == "yes"
                    })
                {
                    if let Some(open) = frame.section_keyed(
                        "inspector-audio-listener",
                        &text("inspector-audio-listener"),
                        inspector_section_open(session, "inspector-audio-listener"),
                        |frame| {
                            let ids: Vec<&str> = targets.iter().map(|n| n.id.as_str()).collect();
                            let picked = inspector_audio_listener::audio_listener_section(
                                frame, session, &ids,
                            );
                            if !matches!(picked, PanelEvent::None) {
                                event = picked;
                            }
                        },
                    ) {
                        event = command(
                            "inspector.section",
                            &[
                                ("id", "inspector-audio-listener"),
                                ("open", if open { "1" } else { "0" }),
                            ],
                        );
                    }
                }
                if category_ok("inspector-color")
                    && show_section("inspector-color")
                    && visible("inspector-color", &["r", "g", "b"])
                {
                    if let Some(open) = frame.section_keyed_with_highlight(
                        "inspector-color",
                        &text("inspector-color"),
                        inspector_section_open(session, "inspector-color"),
                        |frame| {
                            let color_mixed = ["r", "g", "b"]
                                .iter()
                                .any(|key| shared_field(&targets, key).is_none());
                            let mut rgb = [1.0_f32; 3];
                            if !color_mixed {
                                for (index, key) in ["r", "g", "b"].iter().enumerate() {
                                    rgb[index] = shared_field(&targets, key)
                                        .and_then(|text| text.parse().ok())
                                        .unwrap_or(1.0);
                                }
                            }
                            let painted = inspector_pickers::paint_rgb(
                                frame,
                                session,
                                rgb,
                                &text("inspector-color"),
                                set_target,
                            );
                            if !matches!(painted, PanelEvent::None) {
                                event = painted;
                            }
                        },
                        highlight_q,
                    ) {
                        event = command(
                            "inspector.section",
                            &[("id", "inspector-color"), ("open", if open { "1" } else { "0" })],
                        );
                    }
                }
                if category_ok("inspector-shape")
                    && any_body
                    && show_section("inspector-shape")
                    && visible("inspector-shape", &["shape"])
                {
                    if let Some(open) = frame.section_keyed_with_highlight(
                        "inspector-shape",
                        &text("inspector-shape"),
                        inspector_section_open(session, "inspector-shape"),
                        |frame| {
                            let options =
                                ["none", "cube", "sphere", "cylinder", "cone", "capsule", "plane"]
                                    .map(|shape| (shape, text(&format!("shape-{shape}"))));
                            let current =
                                shared_field(&targets, "shape").unwrap_or_else(|| mixed.clone());
                            let modified = inspector_field_modified(&targets, "shape");
                            let tip_owned = if modified {
                                Some(inspector_reset_tip(session, "shape"))
                            } else {
                                None
                            };
                            if !multi || shared_field(&targets, "shape").is_some() {
                                let (shape, reset) = frame.choice_row(
                                    &text("inspector-shape"),
                                    &current,
                                    &options,
                                    modified,
                                    tip_owned.as_deref(),
                                    None,
                                    Some(prop_menu),
                                    &mut prop_menu_pick,
                                    None,
                                );
                                if let Some(shape) = shape {
                                    event = command(
                                        "inspector.set",
                                        &[
                                            set_target,
                                            ("field", "shape"),
                                            ("value", shape.as_str()),
                                        ],
                                    );
                                } else if reset {
                                    event = command(
                                        "inspector.reset_field",
                                        &[reset_target, ("field", "shape")],
                                    );
                                }
                            }
                        },
                        highlight_q,
                    ) {
                        event = command(
                            "inspector.section",
                            &[("id", "inspector-shape"), ("open", if open { "1" } else { "0" })],
                        );
                    }
                }
                if category_ok("inspector-physics")
                    && any_body
                    && show_section("inspector-physics")
                    && visible("inspector-physics", &["motion"])
                {
                    if let Some(open) = frame.section_keyed_with_highlight(
                        "inspector-physics",
                        &text("inspector-physics"),
                        inspector_section_open(session, "inspector-physics"),
                        |frame| {
                            let options = ["static", "dynamic"]
                                .map(|motion| (motion, text(&format!("motion-{motion}"))));
                            let current =
                                shared_field(&targets, "motion").unwrap_or_else(|| mixed.clone());
                            let modified = inspector_field_modified(&targets, "motion");
                            let tip_owned = if modified {
                                Some(inspector_reset_tip(session, "motion"))
                            } else {
                                None
                            };
                            if !multi || shared_field(&targets, "motion").is_some() {
                                let (motion, reset) = frame.choice_row(
                                    &text("inspector-motion"),
                                    &current,
                                    &options,
                                    modified,
                                    tip_owned.as_deref(),
                                    None,
                                    Some(prop_menu),
                                    &mut prop_menu_pick,
                                    None,
                                );
                                if let Some(motion) = motion {
                                    event = command(
                                        "inspector.set",
                                        &[
                                            set_target,
                                            ("field", "motion"),
                                            ("value", motion.as_str()),
                                        ],
                                    );
                                } else if reset {
                                    event = command(
                                        "inspector.reset_field",
                                        &[reset_target, ("field", "motion")],
                                    );
                                }
                            }
                        },
                        highlight_q,
                    ) {
                        event = command(
                            "inspector.section",
                            &[("id", "inspector-physics"), ("open", if open { "1" } else { "0" })],
                        );
                    }
                }
                if category_ok("inspector-physics")
                    && crate::inspector_panel::area_section_wanted(
                        shared_field(&targets, "area_mode").as_deref(),
                        shared_field(&targets, "phys_body").as_deref(),
                    )
                    && show_section("inspector-area")
                    && visible("inspector-area", seal_editor_core::area_effect::KEYS)
                {
                    if let Some(open) = frame.section_keyed_with_highlight(
                        "inspector-area",
                        &text("inspector-area"),
                        inspector_section_open(session, "inspector-area"),
                        |frame| {
                            if let Some(picked) = area_rows(
                                frame,
                                session,
                                &targets,
                                set_target,
                                reset_target,
                                prop_menu,
                                &mut prop_menu_pick,
                            ) {
                                event = picked;
                            }
                        },
                        highlight_q,
                    ) {
                        event = command(
                            "inspector.section",
                            &[("id", "inspector-area"), ("open", if open { "1" } else { "0" })],
                        );
                    }
                }
                if category_ok("inspector-physics")
                    && shared_field(&targets, "volume_kind")
                        .is_some_and(|kind| !kind.is_empty() && kind != "none")
                    && show_section("inspector-volume")
                    && visible("inspector-volume", seal_editor_core::volumes::KEYS)
                {
                    if let Some(open) = frame.section_keyed_with_highlight(
                        "inspector-volume",
                        &text("inspector-volume"),
                        inspector_section_open(session, "inspector-volume"),
                        |frame| {
                            if let Some(picked) = volume_rows(
                                frame,
                                session,
                                &targets,
                                set_target,
                                reset_target,
                                prop_menu,
                                &mut prop_menu_pick,
                            ) {
                                event = picked;
                            }
                        },
                        highlight_q,
                    ) {
                        event = command(
                            "inspector.section",
                            &[("id", "inspector-volume"), ("open", if open { "1" } else { "0" })],
                        );
                    }
                }
                if category_ok("inspector-physics")
                    && shared_field(&targets, "phys_body").is_some()
                    && show_section("inspector-physics2d")
                    && visible("inspector-physics2d", seal_editor_core::phys2d::KEYS)
                {
                    if let Some(open) = frame.section_keyed_with_highlight(
                        "inspector-physics2d",
                        &text("inspector-physics2d"),
                        inspector_section_open(session, "inspector-physics2d"),
                        |frame| {
                            if let Some(picked) = physics2d_rows(
                                frame,
                                session,
                                &targets,
                                set_target,
                                reset_target,
                                prop_menu,
                                &mut prop_menu_pick,
                            ) {
                                event = picked;
                            }
                        },
                        highlight_q,
                    ) {
                        event = command(
                            "inspector.section",
                            &[
                                ("id", "inspector-physics2d"),
                                ("open", if open { "1" } else { "0" }),
                            ],
                        );
                    }
                }
                if category_ok("inspector-shape")
                    && shared_field(&targets, "shape2d").is_some()
                    && show_section("inspector-shape2d")
                    && visible("inspector-shape2d", &seal_editor_core::shape2d::KEYS[..5])
                {
                    if let Some(open) = frame.section_keyed_with_highlight(
                        "inspector-shape2d",
                        &text("inspector-shape2d"),
                        inspector_section_open(session, "inspector-shape2d"),
                        |frame| {
                            if let Some(picked) = shape2d_rows(
                                frame,
                                session,
                                &targets,
                                &seal_editor_core::shape2d::KEYS[..5],
                                set_target,
                                reset_target,
                                prop_menu,
                                &mut prop_menu_pick,
                            ) {
                                event = picked;
                            }
                            if let Some(picked) = shape2d_rows(
                                frame,
                                session,
                                &targets,
                                &[
                                    "shape_texture",
                                    "shape_uv_offset",
                                    "shape_uv_scale",
                                    "shape_uv_rotation",
                                    "shape_cap_begin",
                                    "shape_cap_end",
                                ],
                                set_target,
                                reset_target,
                                prop_menu,
                                &mut prop_menu_pick,
                            ) {
                                event = picked;
                            }
                            if let Some(picked) =
                                shape_style_ui::rows(frame, session, &targets, set_target)
                            {
                                event = picked;
                            }
                        },
                        highlight_q,
                    ) {
                        event = command(
                            "inspector.section",
                            &[("id", "inspector-shape2d"), ("open", if open { "1" } else { "0" })],
                        );
                    }
                }
                if category_ok("inspector-sprite-shape")
                    // Only sprite shapes carry these fields; shared_field reads a missing one as "".
                    && targets.iter().all(|node| node.fields.iter().any(|(k, _)| k == "ss_points"))
                    && show_section("inspector-sprite-shape")
                    && visible("inspector-sprite-shape", seal_editor_core::sprite_shape::KEYS)
                {
                    if let Some(open) = frame.section_keyed_with_highlight(
                        "inspector-sprite-shape",
                        &text("inspector-sprite-shape"),
                        inspector_section_open(session, "inspector-sprite-shape"),
                        |frame| {
                            if let Some(picked) = sprite_shape::rows(
                                frame,
                                session,
                                &targets,
                                set_target,
                                reset_target,
                                prop_menu,
                                &mut prop_menu_pick,
                            ) {
                                event = picked;
                            }
                        },
                        highlight_q,
                    ) {
                        event = command(
                            "inspector.section",
                            &[
                                ("id", "inspector-sprite-shape"),
                                ("open", if open { "1" } else { "0" }),
                            ],
                        );
                    }
                }
                if category_ok("inspector-material2d")
                    && shared_field(&targets, "material2d").is_some()
                    && show_section("inspector-material2d")
                    && visible("inspector-material2d", seal_editor_core::material2d::KEYS)
                {
                    if let Some(open) = frame.section_keyed_with_highlight(
                        "inspector-material2d",
                        &text("inspector-material2d"),
                        inspector_section_open(session, "inspector-material2d"),
                        |frame| {
                            let mat_keys = if targets.iter().any(|node| {
                                session
                                    .world
                                    .entity(&node.id)
                                    .is_some_and(|e| e.material2d.effect == "particle_anim")
                            }) {
                                seal_editor_core::material2d::material_field_keys("particle_anim")
                            } else {
                                seal_editor_core::material2d::KEYS.to_vec()
                            };
                            if let Some(picked) = shape2d_rows(
                                frame,
                                session,
                                &targets,
                                &mat_keys,
                                set_target,
                                reset_target,
                                prop_menu,
                                &mut prop_menu_pick,
                            ) {
                                event = picked;
                            }
                        },
                        highlight_q,
                    ) {
                        event = command(
                            "inspector.section",
                            &[
                                ("id", "inspector-material2d"),
                                ("open", if open { "1" } else { "0" }),
                            ],
                        );
                    }
                }
                if category_ok("inspector-physics")
                    && shared_field(&targets, "follow_path").is_some()
                    && show_section("inspector-follow")
                    && visible("inspector-follow", &seal_editor_core::shape2d::KEYS[5..])
                {
                    if let Some(open) = frame.section_keyed_with_highlight(
                        "inspector-follow",
                        &text("inspector-follow"),
                        inspector_section_open(session, "inspector-follow"),
                        |frame| {
                            if let Some(picked) = shape2d_rows(
                                frame,
                                session,
                                &targets,
                                &seal_editor_core::shape2d::KEYS[5..],
                                set_target,
                                reset_target,
                                prop_menu,
                                &mut prop_menu_pick,
                            ) {
                                event = picked;
                            }
                        },
                        highlight_q,
                    ) {
                        event = command(
                            "inspector.section",
                            &[("id", "inspector-follow"), ("open", if open { "1" } else { "0" })],
                        );
                    }
                }
                if category_ok("inspector-physics")
                    && shared_field(&targets, "nav_agent").is_some()
                    && show_section("inspector-nav2d")
                    && visible("inspector-nav2d", seal_editor_core::nav2d::KEYS)
                {
                    if let Some(open) = frame.section_keyed_with_highlight(
                        "inspector-nav2d",
                        &text("inspector-nav2d"),
                        inspector_section_open(session, "inspector-nav2d"),
                        |frame| {
                            if let Some(picked) = nav2d_rows(
                                frame,
                                session,
                                &targets,
                                set_target,
                                reset_target,
                                prop_menu,
                                &mut prop_menu_pick,
                            ) {
                                event = picked;
                            }
                        },
                        highlight_q,
                    ) {
                        event = command(
                            "inspector.section",
                            &[("id", "inspector-nav2d"), ("open", if open { "1" } else { "0" })],
                        );
                    }
                }
                if category_ok("inspector-physics")
                    && any_body
                    && shared_field(&targets, "char_on").is_some()
                    && show_section("inspector-character2d")
                    && visible("inspector-character2d", seal_editor_core::character2d::KEYS)
                {
                    if let Some(open) = frame.section_keyed_with_highlight(
                        "inspector-character2d",
                        &text("inspector-character2d"),
                        inspector_section_open(session, "inspector-character2d"),
                        |frame| {
                            if let Some(picked) = character2d_ui::rows(
                                frame,
                                session,
                                &targets,
                                set_target,
                                reset_target,
                                prop_menu,
                                &mut prop_menu_pick,
                            ) {
                                event = picked;
                            }
                        },
                        highlight_q,
                    ) {
                        event = command(
                            "inspector.section",
                            &[
                                ("id", "inspector-character2d"),
                                ("open", if open { "1" } else { "0" }),
                            ],
                        );
                    }
                }
                if category_ok("inspector-tags") && visible("inspector-tags", &["role"]) {
                    if let Some(open) = frame.section_keyed_with_highlight(
                        "inspector-tags",
                        &text("inspector-tags"),
                        inspector_section_open(session, "inspector-tags"),
                        |frame| {
                            let current =
                                shared_field(&targets, "role").unwrap_or_else(|| mixed.clone());
                            let modified = inspector_field_modified(&targets, "role");
                            let tip_owned = modified.then(|| inspector_reset_tip(session, "role"));
                            let mut options: Vec<(&str, String)> = vec![
                                ("", text("inspector-game-role-none")),
                                ("player", text("inspector-game-role-player")),
                                ("ground", text("inspector-game-role-ground")),
                                ("coin", text("inspector-game-role-coin")),
                                ("goal", text("inspector-game-role-goal")),
                                ("camera", text("outliner-component-camera")),
                                ("light", text("outliner-component-light")),
                                ("prop", text("inspector-role-prop")),
                                ("folder", text("inspector-role-folder")),
                            ];
                            if !options.iter().any(|(id, _)| *id == current) {
                                options.push((current.as_str(), text("inspector-game-role-other")));
                            }
                            if !multi || shared_field(&targets, "role").is_some() {
                                let (picked, reset) = frame.choice_row(
                                    &text("inspector-role"),
                                    &current,
                                    &options,
                                    modified,
                                    tip_owned.as_deref(),
                                    None,
                                    Some(prop_menu),
                                    &mut prop_menu_pick,
                                    Some("role"),
                                );
                                if let Some(value) = picked {
                                    event = command(
                                        "inspector.set",
                                        &[set_target, ("field", "role"), ("value", value.as_str())],
                                    );
                                } else if reset {
                                    event = command(
                                        "inspector.reset_field",
                                        &[reset_target, ("field", "role")],
                                    );
                                }
                            } else {
                                let mut draft = mixed.clone();
                                let _ = frame.property_validated_row(
                                    &text("inspector-role"),
                                    &mut draft,
                                    None,
                                    modified,
                                    tip_owned.as_deref(),
                                    None,
                                    Some(prop_menu),
                                    &mut prop_menu_pick,
                                    Some("role"),
                                );
                            }
                        },
                        highlight_q,
                    ) {
                        event = command(
                            "inspector.section",
                            &[("id", "inspector-tags"), ("open", if open { "1" } else { "0" })],
                        );
                    }
                }
                if !multi
                    && any_body
                    && category_ok("inspector-tags")
                    && show_section("inspector-sprite")
                    && visible(
                        "inspector-sprite",
                        &[
                            "sprite",
                            "layer",
                            "order",
                            "tint",
                            "opacity",
                            "slice",
                            "normal_map",
                            "parallax",
                            "repeat_x",
                            "frames",
                            "fps",
                            "animation",
                            "flip_x",
                            "flip_y",
                            "mask",
                        ],
                    )
                {
                    if let Some(open) = frame.section_keyed_with_highlight(
                        "inspector-sprite",
                        &text("inspector-sprite"),
                        inspector_section_open(session, "inspector-sprite"),
                        |frame| {
                            let sprite = shared_field(&targets, "sprite").unwrap_or_default();
                            if let Some(painted) = inspector_pickers::try_paint(
                                frame,
                                session,
                                "sprite",
                                &text("inspector-sprite-image"),
                                Some(sprite.as_str()),
                                set_target,
                            ) {
                                if !matches!(painted, PanelEvent::None) {
                                    event = painted;
                                }
                            }
                            for (label, key) in [
                                ("inspector-sprite-layer", "layer"),
                                ("inspector-sprite-order", "order"),
                                ("inspector-sprite-tint", "tint"),
                                ("inspector-sprite-opacity", "opacity"),
                                ("inspector-sprite-slice", "slice"),
                                ("inspector-sprite-normal", "normal_map"),
                                ("inspector-sprite-parallax", "parallax"),
                                ("inspector-sprite-frames", "frames"),
                                ("inspector-sprite-fps", "fps"),
                            ] {
                                if sprite.is_empty() || !visible(label, &[key]) {
                                    continue;
                                }
                                // Clips in a .frames file carry their own speed and frames.
                                if matches!(key, "frames" | "fps")
                                    && seal_editor_core::sprite_frames::is_frames_path(&sprite)
                                {
                                    continue;
                                }
                                if let Some(painted) = inspector_field_row::paint_metadata_number(
                                    frame,
                                    session,
                                    &targets,
                                    key,
                                    &text(label),
                                    set_target,
                                    reset_target,
                                    prop_menu,
                                    &mut prop_menu_pick,
                                ) {
                                    if !matches!(painted, PanelEvent::None) {
                                        event = painted;
                                    }
                                    continue;
                                }
                                let shown = shared_field(&targets, key).unwrap_or_default();
                                if let Some(painted) = inspector_pickers::try_paint(
                                    frame,
                                    session,
                                    key,
                                    &text(label),
                                    Some(shown.as_str()),
                                    set_target,
                                ) {
                                    if !matches!(painted, PanelEvent::None) {
                                        event = painted;
                                    }
                                    continue;
                                }
                                let mut draft = shown;
                                let field_error = match key {
                                    "frames" => validate_unit(
                                        &draft,
                                        1.0,
                                        256.0,
                                        "inspector-invalid-range",
                                        session,
                                    ),
                                    "fps" => validate_unit(
                                        &draft,
                                        0.0,
                                        120.0,
                                        "inspector-invalid-range",
                                        session,
                                    ),
                                    "opacity" => validate_unit(
                                        &draft,
                                        0.0,
                                        1.0,
                                        "inspector-invalid-range",
                                        session,
                                    ),
                                    "normal_map" => None,
                                    "parallax" => seal_editor_core::parse_parallax(&draft)
                                        .is_none()
                                        .then(|| {
                                            session
                                                .messages
                                                .get("inspector-sprite-parallax-invalid")
                                        }),
                                    "slice" => {
                                        seal_editor_core::parse_slice(&draft).is_none().then(|| {
                                            session.messages.get("inspector-sprite-slice-invalid")
                                        })
                                    }
                                    "tint" => {
                                        seal_editor_core::parse_tint(&draft).is_none().then(|| {
                                            session.messages.get("inspector-sprite-tint-invalid")
                                        })
                                    }
                                    _ => validate_unit(
                                        &draft,
                                        -1000.0,
                                        1000.0,
                                        "inspector-invalid-range",
                                        session,
                                    ),
                                };
                                let modified = inspector_field_modified(&targets, key);
                                let tip_owned = modified.then(|| inspector_reset_tip(session, key));
                                let (changed, reset, commit_focus) = frame.property_validated_row(
                                    &text(label),
                                    &mut draft,
                                    field_error.as_deref(),
                                    modified,
                                    tip_owned.as_deref(),
                                    None,
                                    Some(prop_menu),
                                    &mut prop_menu_pick,
                                    None,
                                );
                                let editing = session
                                    .models
                                    .inspector_field_edit
                                    .as_ref()
                                    .is_some_and(|edit| edit.field == key);
                                if changed && field_error.is_none() {
                                    event = inspector_text_field_edit(
                                        session,
                                        set_target,
                                        key,
                                        draft.as_str(),
                                        true,
                                        commit_focus,
                                    );
                                } else if commit_focus && editing {
                                    event = inspector_text_field_edit(
                                        session,
                                        set_target,
                                        key,
                                        draft.as_str(),
                                        false,
                                        true,
                                    );
                                } else if reset {
                                    event = command(
                                        "inspector.reset_field",
                                        &[reset_target, ("field", key)],
                                    );
                                }
                            }
                            if !sprite.is_empty() {
                                if let Some(picked) = sprite_extra_rows(
                                    frame,
                                    session,
                                    &targets,
                                    &sprite,
                                    set_target,
                                    reset_target,
                                    prop_menu,
                                    &mut prop_menu_pick,
                                ) {
                                    event = picked;
                                }
                            }
                            if targets.len() == 1 {
                                if let Some(picked) =
                                    crate::socket_field::row(frame, session, targets[0], set_target)
                                {
                                    event = picked;
                                }
                            }
                        },
                        highlight_q,
                    ) {
                        event = command(
                            "inspector.section",
                            &[("id", "inspector-sprite"), ("open", if open { "1" } else { "0" })],
                        );
                    }
                }
                if !multi {
                    let tile_event =
                        tile_inspector::tile_data_section(frame, session, id, &|k| text(k));
                    if !matches!(tile_event, PanelEvent::None) {
                        event = tile_event;
                    }
                }
                if category_ok("inspector-sprite-mask")
                    && shared_field(&targets, "mask_shape")
                        .is_some_and(|v| matches!(v.as_str(), "rect" | "circle" | "alpha"))
                    && show_section("inspector-sprite-mask")
                    && visible("inspector-sprite-mask", seal_editor_core::sprite_mask::KEYS)
                {
                    if let Some(open) = frame.section_keyed_with_highlight(
                        "inspector-sprite-mask",
                        &text("inspector-sprite-mask"),
                        inspector_section_open(session, "inspector-sprite-mask"),
                        |frame| {
                            if let Some(picked) = sprite_mask_rows(
                                frame,
                                session,
                                &targets,
                                set_target,
                                reset_target,
                                prop_menu,
                                &mut prop_menu_pick,
                            ) {
                                event = picked;
                            }
                        },
                        highlight_q,
                    ) {
                        event = command(
                            "inspector.section",
                            &[
                                ("id", "inspector-sprite-mask"),
                                ("open", if open { "1" } else { "0" }),
                            ],
                        );
                    }
                }
                if !multi
                    && any_body
                    && category_ok("inspector-tags")
                    && show_section("inspector-script")
                    && (shared_field(&targets, "script").is_some()
                        || shared_field(&targets, "script_order").is_some())
                    && visible("inspector-script", &["script", "script_order", "script_group"])
                {
                    if let Some(open) = frame.section_keyed_with_highlight(
                        "inspector-script",
                        &text("inspector-script"),
                        inspector_section_open(session, "inspector-script"),
                        |frame| {
                            let script = shared_field(&targets, "script").unwrap_or_default();
                            if let Some(painted) = inspector_pickers::try_paint(
                                frame,
                                session,
                                "script",
                                &text("inspector-script-graph"),
                                Some(script.as_str()),
                                set_target,
                            ) {
                                if !matches!(painted, PanelEvent::None) {
                                    event = painted;
                                }
                            }
                            let painted =
                                inspector_pickers::paint_graph_vars(frame, session, &script);
                            if !matches!(painted, PanelEvent::None) {
                                event = painted;
                            }
                            let mut order = shared_field(&targets, "script_order")
                                .and_then(|text| text.parse().ok())
                                .unwrap_or(0.0);
                            if frame.number(&text("inspector-script-order"), &mut order, 1.0) {
                                let value = (order.round() as i32).to_string();
                                event = command(
                                    "inspector.set",
                                    &[
                                        set_target,
                                        ("field", "script_order"),
                                        ("value", value.as_str()),
                                    ],
                                );
                            }
                            let group = shared_field(&targets, "script_group")
                                .unwrap_or_else(|| "before_physics".into());
                            let groups = [
                                ("before_physics", text("inspector-script-group-before")),
                                ("after_physics", text("inspector-script-group-after")),
                                ("late", text("inspector-script-group-late")),
                            ];
                            let (picked, _reset) = frame.choice_row(
                                &text("inspector-script-group"),
                                &group,
                                &groups,
                                false,
                                None,
                                Some(&text("inspector-script-group-hint")),
                                Some(prop_menu),
                                &mut prop_menu_pick,
                                Some("script_group"),
                            );
                            if let Some(picked) = picked {
                                event = command(
                                    "inspector.set",
                                    &[
                                        set_target,
                                        ("field", "script_group"),
                                        ("value", picked.as_str()),
                                    ],
                                );
                            }
                            frame.muted(&text("inspector-script-group-hint"));
                            if frame.button(&text("graph-open-selected")) {
                                event = command("graph.open", &[]);
                            }
                        },
                        highlight_q,
                    ) {
                        event = command(
                            "inspector.section",
                            &[("id", "inspector-script"), ("open", if open { "1" } else { "0" })],
                        );
                    }
                }
                if !multi
                    && any_body
                    && category_ok("inspector-remote")
                    && show_section("inspector-remote")
                    && shared_field(&targets, "remote_on").as_deref() == Some("yes")
                    && visible("inspector-remote", seal_editor_core::remote_transform::KEYS)
                {
                    if let Some(open) = frame.section_keyed_with_highlight(
                        "inspector-remote",
                        &text("inspector-remote"),
                        inspector_section_open(session, "inspector-remote"),
                        |frame| {
                            if let Some(picked) = remote_transform_ui::rows(
                                frame,
                                session,
                                &targets,
                                set_target,
                                reset_target,
                                prop_menu,
                                &mut prop_menu_pick,
                            ) {
                                event = picked;
                            }
                        },
                        highlight_q,
                    ) {
                        event = command(
                            "inspector.section",
                            &[("id", "inspector-remote"), ("open", if open { "1" } else { "0" })],
                        );
                    }
                }
                if !multi
                    && any_body
                    && category_ok("inspector-tags")
                    && show_section("inspector-particles")
                    && visible("inspector-particles", &["particles"])
                {
                    if let Some(open) = frame.section_keyed_with_highlight(
                        "inspector-particles",
                        &text("inspector-particles"),
                        inspector_section_open(session, "inspector-particles"),
                        |frame| {
                            let current = shared_field(&targets, "particles").unwrap_or_default();
                            if let Some(painted) = inspector_pickers::try_paint(
                                frame,
                                session,
                                "particles",
                                &text("inspector-particles-effect"),
                                Some(current.as_str()),
                                set_target,
                            ) {
                                if !matches!(painted, PanelEvent::None) {
                                    event = painted;
                                }
                            }
                            if !current.is_empty() {
                                if let Some(project) = session.project.as_ref() {
                                    if let Ok(raw) =
                                        std::fs::read_to_string(project.root.join(&current))
                                    {
                                        if let Ok(fx) =
                                            seal_editor_core::asset_editor::EmitterDef::parse(&raw)
                                        {
                                            let defaults =
                                                seal_editor_core::asset_editor::EmitterDef::default(
                                                );
                                            let extra = crate::fx_extra_fields::effect_extra_fields(
                                                frame,
                                                session,
                                                &fx,
                                                &defaults,
                                                Some(current.as_str()),
                                            );
                                            if !matches!(extra, PanelEvent::None) {
                                                event = extra;
                                            }
                                        }
                                    }
                                }
                                let curves =
                                    crate::curve_fields::effect_curves(frame, session, &current);
                                if !matches!(curves, PanelEvent::None) {
                                    event = curves;
                                }
                            }
                        },
                        highlight_q,
                    ) {
                        event = command(
                            "inspector.section",
                            &[
                                ("id", "inspector-particles"),
                                ("open", if open { "1" } else { "0" }),
                            ],
                        );
                    }
                }
                if category_ok("inspector-properties")
                    && visible("inspector-properties", &["id", "parent"])
                    && !multi
                {
                    if let Some(open) = frame.section_keyed_with_highlight(
                        "inspector-properties",
                        &text("inspector-properties"),
                        inspector_section_open(session, "inspector-properties"),
                        |frame| {
                            frame.readout(&text("inspector-id"), id);
                            frame.readout(
                                &text("inspector-parent"),
                                primary.parent.as_deref().unwrap_or("-"),
                            );
                        },
                        highlight_q,
                    ) {
                        event = command(
                            "inspector.section",
                            &[
                                ("id", "inspector-properties"),
                                ("open", if open { "1" } else { "0" }),
                            ],
                        );
                    }
                }
                if let Some(pick) = prop_menu_pick.take() {
                    event = inspector_panel::apply_property_menu_pick(pick);
                }
            });
        });
    });
    event
}

fn place(frame: &mut PanelFrame, session: &EditorSession) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let mut event = PanelEvent::None;
    frame.scroll(|frame| {
        let sections: &[(&str, &[&str])] = &[
            ("place-shapes", &["cube", "sphere", "cylinder", "cone", "capsule", "plane"]),
            (
                "place-areas",
                &[
                    "wind_area",
                    "water_area",
                    "gravity_area",
                    "attractor",
                    "kill_zone",
                    "damage_zone",
                    "checkpoint",
                    "trigger_volume",
                    "post_process",
                ],
            ),
            (
                "place-scene",
                &["empty", "camera", "light", "sprite_shape", "sprite_mask", "character_2d"],
            ),
        ];
        for (title, kinds) in sections {
            frame.section(&text(title), |frame| {
                for kind in *kinds {
                    if frame.shape_button(kind, &text(&format!("place-{kind}"))) {
                        event = command("scene.add", &[("kind", kind)]);
                    }
                }
            });
        }
        // 2D: every picture in the project. Click one to make it the brush, then
        // left-drag in the flat viewport to paint it tile by tile (Esc stops).
        let pictures =
            inspector_panel::project_files_with(session, seal_editor_core::picture::EXTENSIONS);
        let brush = session.draft(PLACE_BRUSH_DRAFT).unwrap_or_default();
        // Tile brush settings: the grid it snaps to, the layer it paints on,
        // whether painted tiles are solid ground, and erase mode.
        let brush_event = tile_brush::tile_brush_section(frame, session, &|k| text(k));
        if !matches!(brush_event, PanelEvent::None) {
            event = brush_event;
        }
        // Tile rule files paint with the picture their neighbours call for.
        let rule_sets = inspector_panel::project_files_with(session, &["tiles"]);
        if !rule_sets.is_empty() {
            frame.section(&text("place-tile-rules"), |frame| {
                for rel in &rule_sets {
                    let on = brush == *rel;
                    let preview = session
                        .project
                        .as_ref()
                        .and_then(|p| {
                            let rules =
                                seal_editor_core::tiles::RuleSet::load(&p.root.join(rel)).ok()?;
                            let first = rules.rules.last()?.1.clone();
                            Some(p.root.join(first).to_string_lossy().into_owned())
                        })
                        .unwrap_or_default();
                    if frame.picture_toggle(rel, &preview, on) {
                        let value = if on { "" } else { rel.as_str() };
                        event =
                            command("draft.set", &[("key", PLACE_BRUSH_DRAFT), ("value", value)]);
                    }
                }
            });
        }
        frame.section(&text("place-sprites"), |frame| {
            if pictures.is_empty() {
                frame.label(&text("place-sprites-empty"));
            } else {
                frame.label(&text("place-sprites-hint"));
            }
            for rel in &pictures {
                let on = brush == *rel;
                let abs = session
                    .project
                    .as_ref()
                    .map(|p| p.root.join(rel).to_string_lossy().into_owned())
                    .unwrap_or_default();
                if frame.picture_toggle(rel, &abs, on) {
                    let value = if on { "" } else { rel.as_str() };
                    event = command("draft.set", &[("key", PLACE_BRUSH_DRAFT), ("value", value)]);
                }
            }
        });
        // Saved prefabs: one click places a copy under the selection (or the scene root).
        let prefabs = inspector_panel::project_files_with(session, &["prefab"]);
        frame.section(&text("place-prefabs"), |frame| {
            if prefabs.is_empty() {
                frame.label(&text("place-prefabs-empty"));
            }
            for rel in &prefabs {
                let name = std::path::Path::new(rel)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| rel.clone());
                if frame.button(&name) {
                    event = command("scene.add_prefab", &[("path", rel)]);
                }
            }
        });
    });
    event
}

fn world(frame: &mut PanelFrame, session: &EditorSession) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let mut event = PanelEvent::None;
    frame.scroll(|frame| {
        frame.padded(|frame| {
            frame.section(&text("world-physics"), |frame| {
                let mut gravity = session.world.gravity;
                if frame.number(&text("world-gravity"), &mut gravity, 0.05) {
                    event = command("world.gravity", &[("value", &gravity.to_string())]);
                }
                let mut scale = session.world.time_scale;
                if frame.number(&text("world-time-scale"), &mut scale, 0.05) {
                    event = command("world.time-scale", &[("value", &scale.to_string())]);
                }
                let join = session.world.join_on;
                if frame.toggle(&text("world-join"), join) {
                    event = command("world.join", &[("value", if join { "0" } else { "1" })]);
                }
                frame.muted(&text("world-join-hint"));
            });
            frame.section(&text("world-music"), |frame| {
                let music = session.world.music.clone();
                match inspector_pickers::paint_asset_row(
                    frame,
                    session,
                    "music",
                    "world-music",
                    &text("world-music"),
                    &music,
                    false,
                ) {
                    inspector_pickers::AssetRow::Set(path) => {
                        event = command("world.music", &[("value", path.as_str())]);
                    }
                    inspector_pickers::AssetRow::Clear => {
                        event = command("world.music", &[("value", "")]);
                    }
                    inspector_pickers::AssetRow::Show => {
                        event = command("picker.show_asset", &[("path", music.as_str())]);
                    }
                    inspector_pickers::AssetRow::Idle => {}
                }
                frame.muted(&text("world-music-hint"));
            });
            frame.section(&text("world-sky"), |frame| {
                let sky = session.world.sky_cubemap.clone();
                match inspector_pickers::paint_asset_row(
                    frame,
                    session,
                    "sky_cubemap",
                    "world-sky",
                    &text("world-sky"),
                    &sky,
                    false,
                ) {
                    inspector_pickers::AssetRow::Set(path) => {
                        event = command("world.sky", &[("value", path.as_str())]);
                    }
                    inspector_pickers::AssetRow::Clear => {
                        event = command("world.sky", &[("value", "")]);
                    }
                    inspector_pickers::AssetRow::Show => {
                        event = command("picker.show_asset", &[("path", sky.as_str())]);
                    }
                    inspector_pickers::AssetRow::Idle => {}
                }
                frame.muted(&text("world-sky-hint"));
            });
            frame.section(&text("world-glow"), |frame| {
                let on = session.world.glow.enabled;
                if frame.toggle(&text("world-glow-enabled"), on) {
                    event = command(
                        "world.glow",
                        &[("field", "enabled"), ("value", if on { "no" } else { "yes" })],
                    );
                }
                if on {
                    let mut threshold = session.world.glow.threshold;
                    if frame.number(&text("world-glow-threshold"), &mut threshold, 0.05) {
                        event = command(
                            "world.glow",
                            &[("field", "threshold"), ("value", &threshold.to_string())],
                        );
                    }
                    let mut intensity = session.world.glow.intensity;
                    if frame.number(&text("world-glow-intensity"), &mut intensity, 0.05) {
                        event = command(
                            "world.glow",
                            &[("field", "intensity"), ("value", &intensity.to_string())],
                        );
                    }
                    let mut radius = session.world.glow.radius;
                    if frame.number(&text("world-glow-radius"), &mut radius, 0.5) {
                        event = command(
                            "world.glow",
                            &[("field", "radius"), ("value", &radius.to_string())],
                        );
                    }
                }
                frame.muted(&text("world-glow-hint"));
            });
            frame.section(&text("world-scene"), |frame| {
                let file = match session.scene_path() {
                    Some(path) => path.display().to_string(),
                    None => text("world-unsaved"),
                };
                frame.readout(&text("world-file"), &file);
                frame.readout(&text("stats-entities"), &session.world.entities.len().to_string());
                let per_file = session.world.objects_per_file;
                if frame.toggle(&text("world-object-files"), per_file) {
                    event = command(
                        "scene.object_files",
                        &[("value", if per_file { "no" } else { "yes" })],
                    );
                }
                frame.muted(&text("world-object-files-hint"));
            });
        });
    });
    event
}

fn stats(frame: &mut PanelFrame, session: &EditorSession) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let entities = &session.world.entities;
    let play = match session.world.mode {
        seal_editor_core::PlayMode::Playing => "play-running",
        seal_editor_core::PlayMode::Paused => "play-paused",
        seal_editor_core::PlayMode::Editing => "play-stopped",
    };
    frame.scroll(|frame| {
        frame.padded(|frame| {
            frame.section(&text("stats-frame"), |frame| {
                let ms = session.frame_ms;
                frame.readout(&text("stats-frame-time"), &format!("{ms:.1} ms"));
                let rate = if ms > 0.0 { 1000.0 / ms } else { 0.0 };
                frame.readout(&text("stats-rate"), &format!("{rate:.0}"));
            });
            frame.section(&text("stats-scene"), |frame| {
                let count = |keep: fn(&&seal_editor_core::Entity) -> bool| {
                    entities.iter().filter(keep).count().to_string()
                };
                frame.readout(&text("stats-entities"), &entities.len().to_string());
                frame.readout(&text("stats-bodies"), &count(|entity| entity.dynamic));
                frame.readout(&text("stats-colliders"), &count(|entity| entity.shape.is_some()));
                frame.readout(&text("stats-play"), &text(play));
            });
            frame.section(&text("stats-editor"), |frame| {
                frame
                    .readout(&text("stats-commands"), &session.commands.iter().count().to_string());
                frame.readout(&text("stats-undo"), &session.past.len().to_string());
                frame.readout(
                    &text("stats-panels"),
                    &session.foundation.open_panels.len().to_string(),
                );
            });
            frame.section(&text("stats-textures"), |frame| {
                let (used, budget, pending) = seal_ui::sprites::texture_runtime_stats();
                let used_mb = used as f64 / (1024.0 * 1024.0);
                let budget_mb = budget as f64 / (1024.0 * 1024.0);
                frame.readout(
                    &text("stats-texture-memory"),
                    &format!("{used_mb:.1} / {budget_mb:.0} MB"),
                );
                frame.readout(&text("stats-texture-pending"), &pending.to_string());
            });
        });
    });
    PanelEvent::None
}

fn console(frame: &mut PanelFrame, session: &EditorSession) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let mut event = PanelEvent::None;
    // Filter gets its own full-width row so the toolbar below never wraps or squeezes it.
    frame.bar(|frame| {
        let mut query = session.models.console_query.clone();
        let entered = frame.key_pressed(egui::Key::Enter);
        if frame.panel_search(
            &mut query,
            &text("console-filter-hint"),
            &text("inspector-search-clear"),
        ) {
            event = command("console.filter", &[("query", query.as_str())]);
        } else if entered && !query.is_empty() {
            event = command("console.jump_match", &[("direction", "next")]);
        }
    });
    frame.bar(|frame| {
        for (level, key) in [
            ("all", "console-level-all"),
            ("error", "console-level-error"),
            ("warn", "console-level-warn"),
            ("info", "console-level-info"),
        ] {
            if frame.toggle(&text(key), session.models.console_level == level) {
                event = command("console.filter", &[("level", level)]);
            }
        }
        if frame.toggle(&text("console-regex"), session.models.console_regex) {
            event = command("console.toggle_regex", &[]);
        }
        let pause = text("console-pause");
        if frame.toggle(&pause, session.models.console_paused) {
            event = command("console.pause", &[]);
        }
        frame.rule();
        if frame.icon_button(seal_ui::Icon::ViewArrowForward, &text("console-jump-latest-tip")) {
            event = command("console.jump_latest", &[]);
        }
        if frame.icon_button(seal_ui::Icon::EditCopy, &text("console-copy-selection")) {
            event = command("console.copy_selection", &[]);
        }
        frame.rule();
        if frame.icon_button(seal_ui::Icon::ViewChevronLeft, &text("console-jump-prev")) {
            event = command("console.jump_match", &[("direction", "prev")]);
        }
        if frame.icon_button(seal_ui::Icon::ViewChevronRight, &text("console-jump-next")) {
            event = command("console.jump_match", &[("direction", "next")]);
        }
        if frame.icon_button(seal_ui::Icon::ViewChevronUp, &text("console-row-prev")) {
            event = command("console.select_row", &[("direction", "prev")]);
        }
        if frame.icon_button(seal_ui::Icon::ViewChevronDown, &text("console-row-next")) {
            event = command("console.select_row", &[("direction", "next")]);
        }
    });
    let records = current_log(session);
    let max_entries = session
        .foundation
        .settings
        .get("console.max_entries")
        .and_then(|text| text.parse().ok())
        .unwrap_or(5000);
    let filtered: Vec<&LogLine> = session.models.filtered_logs(&records, max_entries);
    let focus = session.models.console_match_index.min(filtered.len().saturating_sub(1));
    let lines: Vec<LogLineView<'_>> = filtered
        .iter()
        .enumerate()
        .map(|(index, line)| LogLineView {
            level: line.level.as_str(),
            message: line.message.as_str(),
            multiline: line.message.contains('\n'),
            folded: session.models.console_folded.contains(&index),
        })
        .collect();
    let menu = vec![
        session.messages.get("menu-copy"),
        session.messages.get("menu-filter"),
        session.messages.get("command-console-copy"),
    ];
    let list_hit = frame.log_list(
        &lines,
        &session.models.console_line_selection,
        focus,
        &menu,
        !session.models.console_paused,
    );
    match list_hit {
        LogListHit::Select(index) => {
            event = command("console.select_row", &[("index", &index.to_string())]);
        }
        LogListHit::SelectExtend(index) => {
            event = command("console.select_extend", &[("index", &index.to_string())]);
        }
        LogListHit::ToggleFold(index) => {
            event = command("console.fold_toggle", &[("index", &index.to_string())]);
        }
        LogListHit::Menu(index, action) => {
            let message = filtered.get(index).map(|line| line.message.as_str()).unwrap_or("");
            event = match action {
                0 => command("edit.copy", &[("text", message)]),
                1 => command("console.filter", &[("query", message)]),
                _ => command("console.copy_selection", &[]),
            };
        }
        LogListHit::None => {}
    }
    if let Some(line) = filtered.get(focus) {
        frame.section(&text("console-details-title"), |frame| {
            frame.readout(&text("console-details-level"), line.level.as_str());
            frame.readout(&text("console-details-source"), line.source.as_str());
            frame.label(line.message.as_str());
            if !line.fields.is_empty() {
                for (key, value) in &line.fields {
                    frame.readout(key, value);
                }
            }
        });
    } else {
        frame.muted(&text("console-row-none"));
    }
    let picked = if session.models.console_line_selection.is_empty() {
        usize::from(!filtered.is_empty())
    } else {
        session.models.console_line_selection.len()
    };
    frame.muted(&session.messages.format(
        "console-status-counts",
        &[("shown", &filtered.len().to_string()), ("picked", &picked.to_string())],
    ));
    let mut command_line = session.draft("console-command").unwrap_or_default();
    let (changed, committed) = frame.draft_field(&text("console-command-hint"), &mut command_line);
    if changed {
        event =
            command("draft.set", &[("key", "console-command"), ("value", command_line.as_str())]);
    }
    if committed && !command_line.is_empty() {
        event = command("console.run", &[("line", command_line.as_str())]);
    }
    event
}

fn content(frame: &mut PanelFrame, session: &EditorSession) -> PanelEvent {
    let models = &session.models;
    let text = |key: &str| session.messages.get(key);
    let mut event = PanelEvent::None;
    let Some(_root) = &models.content_root else {
        let actions = [text("content-empty-open-project"), text("content-empty-new-project")];
        let action_refs = [actions[0].as_str(), actions[1].as_str()];
        match frame.empty_with_actions(
            &text("content-empty"),
            &text("content-empty-hint"),
            &action_refs,
        ) {
            Some(0) => event = command("file.open-project", &[]),
            Some(1) => event = command("file.new-project", &[]),
            _ => {}
        }
        return event;
    };
    if let Some(err) = &models.content_error {
        let args: Vec<(&str, &str)> =
            err.args.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        frame.label(&session.messages.format(&err.key, &args));
    }
    // Three columns: folders, type filters, then the toolbar and the grid.
    let ui = frame.ui_mut();
    let full = ui.available_rect_before_wrap();
    let width_of = |ui: &egui::Ui, key: &str, default: f32| {
        ui.ctx().data(|d| d.get_temp::<f32>(egui::Id::new(key))).unwrap_or(default)
    };
    // A narrow dock gives less than the minimum; f32::clamp panics when min > max.
    let left_cap = full.width() * 0.4;
    let mid_cap = full.width() * 0.3;
    let left_w =
        width_of(ui, "content-sources-width", 230.0).clamp(140.0f32.min(left_cap), left_cap);
    let mid_w = width_of(ui, "content-filters-width", 150.0).clamp(100.0f32.min(mid_cap), mid_cap);
    let split = 5.0;
    let left = egui::Rect::from_min_size(full.min, egui::vec2(left_w, full.height()));
    let mid = egui::Rect::from_min_size(
        egui::pos2(left.right() + split, full.top()),
        egui::vec2(mid_w, full.height()),
    );
    let right = egui::Rect::from_min_max(egui::pos2(mid.right() + split, full.top()), full.max);
    for (key, at, width) in [
        ("content-sources-width", left.right(), left_w),
        ("content-filters-width", mid.right(), mid_w),
    ] {
        let bar = egui::Rect::from_min_max(
            egui::pos2(at, full.top()),
            egui::pos2(at + split, full.bottom()),
        );
        let drag = ui.interact(bar, egui::Id::new(key).with("drag"), egui::Sense::drag());
        let line = if drag.hovered() || drag.dragged() {
            ui.visuals().selection.stroke.color
        } else {
            ui.visuals().extreme_bg_color
        };
        ui.painter().rect_filled(bar, 0.0, line);
        if drag.hovered() || drag.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if drag.dragged() {
            let next = width + drag.drag_delta().x;
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(key), next));
        }
    }
    // Each column clips to itself so one column's background clicks never cover another.
    let column = |ui: &mut egui::Ui, rect: egui::Rect, id: &str| {
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(id)
                .max_rect(rect)
                .layout(egui::Layout::top_down(egui::Align::LEFT)),
        );
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        child
    };
    let mut sources = column(ui, left, "content-sources");
    content_sources(&mut PanelFrame::new(&mut sources), session, &mut event);
    let mut filters = column(ui, mid, "content-filters");
    content_filters(&mut PanelFrame::new(&mut filters), session, &mut event);
    let mut grid = column(ui, right, "content-grid");
    content_grid_side(&mut PanelFrame::new(&mut grid), session, &mut event);
    ui.advance_cursor_after_rect(full);
    event
}

/// Name a core command resolves from the project root.
fn root_name(rel: &str) -> String {
    if rel.contains('/') {
        rel.to_string()
    } else {
        format!("./{rel}")
    }
}

/// Dark band with a fold arrow and a title; returns true when it was clicked.
fn content_band(
    ui: &mut egui::Ui,
    title: &str,
    open: bool,
    extra: impl FnOnce(&mut egui::Ui),
) -> bool {
    let height = 26.0;
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), height), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, ui.visuals().faint_bg_color);
    let click = ui.interact(
        rect.with_max_x(rect.right() - 54.0),
        ui.id().with(("band", title)),
        egui::Sense::click(),
    );
    let arrow = if open { seal_ui::Icon::ViewChevronDown } else { seal_ui::Icon::ViewChevronRight };
    seal_ui::paint_icon(
        ui.painter(),
        egui::pos2(rect.left() + 12.0, rect.center().y),
        arrow,
        12.0,
        ui.visuals().text_color(),
    );
    ui.painter().text(
        egui::pos2(rect.left() + 24.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        title,
        egui::TextStyle::Button.resolve(ui.style()),
        ui.visuals().strong_text_color(),
    );
    let mut tools = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(("band-tools", title))
            .max_rect(rect.with_min_x(rect.right() - 54.0).shrink2(egui::vec2(2.0, 2.0)))
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    extra(&mut tools);
    click.clicked()
}

fn band_open(ui: &egui::Ui, key: &str) -> bool {
    ui.ctx().data(|d| d.get_temp::<bool>(egui::Id::new(key))).unwrap_or(true)
}

fn set_band_open(ui: &egui::Ui, key: &str, open: bool) {
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(key), open));
}

/// Left column: favorite folders, the project's folder tree and collections.
fn content_sources(frame: &mut PanelFrame, session: &EditorSession, event: &mut PanelEvent) {
    let models = &session.models;
    let text = |key: &str| session.messages.get(key);
    let state = &models.asset_browser;
    let here = seal_editor_core::current_rel(session);
    let in_collection = state.collection.is_some();
    let ui = frame.ui_mut();
    // Favorites.
    let fav_open = band_open(ui, "content-band-favorites");
    if content_band(ui, &text("content-favorites"), fav_open, |_| {}) {
        set_band_open(ui, "content-band-favorites", !fav_open);
    }
    if fav_open {
        let mut frame = PanelFrame::new(ui);
        if state.favorites.is_empty() {
            frame.muted(&text("content-favorites-empty"));
        }
        let menu = vec![text("content-menu-favorite-remove")];
        for rel in &state.favorites {
            let name = if rel.is_empty() {
                text("content-all")
            } else {
                rel.rsplit('/').next().unwrap_or(rel).to_string()
            };
            match frame.tree_row(seal_ui::TreeRow {
                depth: 0,
                has_children: false,
                open: false,
                label: &name,
                detail: "",
                selected: !in_collection && here == *rel,
                hidden: false,
                locked: false,
                menu: &menu,
                drag_id: None,
                place_kind: None,
                rename_text: None,
                component_row: false,
                thumb: None,
                marks: false,
            }) {
                TreeClick::Select | TreeClick::FrameEntity => {
                    *event = command("content.go", &[("path", rel.as_str())]);
                }
                TreeClick::Menu(0) => {
                    *event = command("content.favorite", &[("path", rel.as_str())]);
                }
                _ => {}
            }
        }
    }
    // Project folders.
    let ui = frame.ui_mut();
    let project = models
        .content_tree_root
        .as_ref()
        .and_then(|root| root.file_name())
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let tree_open = band_open(ui, "content-band-tree");
    let search_key = egui::Id::new("content-folder-search");
    let mut searching = ui.ctx().data(|d| d.get_temp::<bool>(search_key)).unwrap_or(false);
    if content_band(ui, &project, tree_open, |ui| {
        if PanelFrame::new(ui).icon_toggle(
            seal_ui::Icon::EditSearch,
            searching,
            &text("content-folder-search"),
        ) {
            searching = !searching;
        }
    }) {
        set_band_open(ui, "content-band-tree", !tree_open);
    }
    ui.ctx().data_mut(|d| d.insert_temp(search_key, searching));
    let coll_open = band_open(ui, "content-band-collections");
    let collections_h = if coll_open {
        (26.0 + 24.0 * state.collections.len().max(1) as f32 + 8.0).min(ui.available_height() * 0.4)
    } else {
        26.0
    };
    let tree_h = (ui.available_height() - collections_h).max(40.0);
    if tree_open {
        let mut filter = String::new();
        if searching {
            filter = ui
                .ctx()
                .data(|d| d.get_temp::<String>(search_key.with("text")))
                .unwrap_or_default();
            let mut frame = PanelFrame::new(ui);
            frame.padded(|frame| if frame.search(&mut filter, &text("content-folder-search")) {});
            ui.ctx().data_mut(|d| d.insert_temp(search_key.with("text"), filter.clone()));
        }
        let filter = filter.to_lowercase();
        let rows = seal_editor_core::folder_rows(session);
        let tree_h = (tree_h - if searching { 34.0 } else { 0.0 }).max(30.0);
        egui::ScrollArea::vertical()
            .id_salt("content-folder-tree")
            .max_height(tree_h)
            .min_scrolled_height(tree_h)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut frame = PanelFrame::new(ui);
                for row in &rows {
                    if !filter.is_empty()
                        && row.depth > 0
                        && !row.name.to_lowercase().contains(&filter)
                    {
                        continue;
                    }
                    let label =
                        if row.rel.is_empty() { text("content-all") } else { row.name.clone() };
                    let favorite = state.favorites.contains(&row.rel);
                    let mut actions = if row.rel.is_empty() {
                        CONTENT_EMPTY_ACTIONS.to_vec()
                    } else {
                        content_item_actions(true, &row.name)
                    };
                    actions.retain(|a| *a != ContentAction::Open && *a != ContentAction::Rename);
                    let mut menu = vec![text(if favorite {
                        "content-menu-favorite-remove"
                    } else {
                        "content-menu-favorite-add"
                    })];
                    menu.extend(actions.iter().map(|a| text(a.label_key())));
                    match frame.tree_row(seal_ui::TreeRow {
                        depth: row.depth,
                        has_children: row.has_children && filter.is_empty(),
                        open: row.open || row.rel.is_empty(),
                        label: &label,
                        detail: "",
                        selected: !in_collection && here == row.rel,
                        hidden: false,
                        locked: false,
                        menu: &menu,
                        drag_id: None,
                        place_kind: None,
                        rename_text: None,
                        component_row: false,
                        thumb: None,
                        marks: false,
                    }) {
                        TreeClick::Toggle if !row.rel.is_empty() => {
                            *event = command("content.toggle_tree", &[("path", row.rel.as_str())]);
                        }
                        TreeClick::Select | TreeClick::FrameEntity => {
                            *event = command("content.go", &[("path", row.rel.as_str())]);
                        }
                        TreeClick::Menu(0) => {
                            *event = command("content.favorite", &[("path", row.rel.as_str())]);
                        }
                        TreeClick::Menu(index) => {
                            let full = models.content_tree_root.as_ref().map(|r| r.join(&row.rel));
                            let name = root_name(&row.rel);
                            if let Some(action) = actions.get(index - 1).copied() {
                                *event = content_action_event(
                                    action,
                                    (!row.rel.is_empty()).then_some(name.as_str()),
                                    full.as_deref(),
                                    full.as_deref(),
                                    models.content_tree_root.as_deref(),
                                );
                            }
                        }
                        _ => {}
                    }
                }
            });
    } else {
        ui.allocate_space(egui::vec2(ui.available_width(), tree_h));
    }
    // Collections.
    let mut new_collection = false;
    if content_band(ui, &text("content-collections"), coll_open, |ui| {
        if PanelFrame::new(ui).icon_button(seal_ui::Icon::ViewPlus, &text("content-collection-new"))
        {
            new_collection = true;
        }
    }) {
        set_band_open(ui, "content-band-collections", !coll_open);
    }
    if new_collection {
        *event = command("content.collection_new", &[]);
    }
    if coll_open {
        let renaming = session.draft("content-collection-renaming").filter(|n| !n.is_empty());
        let mut rename_text = session
            .draft("content-collection-rename")
            .or_else(|| renaming.clone())
            .unwrap_or_default();
        egui::ScrollArea::vertical()
            .id_salt("content-collections")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let mut frame = PanelFrame::new(ui);
                if state.collections.is_empty() {
                    frame.muted(&text("content-collections-empty"));
                }
                let menu = vec![
                    text("content-menu-collection-rename"),
                    text("content-menu-collection-delete"),
                ];
                for (name, items) in &state.collections {
                    let detail = items.len().to_string();
                    let is_renaming = renaming.as_deref() == Some(name.as_str());
                    match frame.tree_row(seal_ui::TreeRow {
                        depth: 0,
                        has_children: false,
                        open: false,
                        label: name,
                        detail: &detail,
                        selected: state.collection.as_deref() == Some(name.as_str()),
                        hidden: false,
                        locked: false,
                        menu: &menu,
                        drag_id: None,
                        place_kind: None,
                        rename_text: if is_renaming { Some(&mut rename_text) } else { None },
                        component_row: false,
                        thumb: None,
                        marks: false,
                    }) {
                        TreeClick::Rename(value) => {
                            *event = PanelEvent::Commands(vec![
                                (
                                    "draft.set".into(),
                                    CommandArgs::new(&[
                                        ("key", "content-collection-rename"),
                                        ("value", ""),
                                    ]),
                                ),
                                (
                                    "content.collection_rename".into(),
                                    CommandArgs::new(&[
                                        ("from", name.as_str()),
                                        ("to", value.as_str()),
                                    ]),
                                ),
                            ]);
                        }
                        TreeClick::RenameCancel => {
                            *event = command(
                                "draft.set",
                                &[("key", "content-collection-renaming"), ("value", "")],
                            );
                        }
                        _ if is_renaming
                            && session
                                .draft("content-collection-rename")
                                .unwrap_or_else(|| name.clone())
                                != rename_text =>
                        {
                            *event = command(
                                "draft.set",
                                &[
                                    ("key", "content-collection-rename"),
                                    ("value", rename_text.as_str()),
                                ],
                            );
                        }
                        TreeClick::Select | TreeClick::FrameEntity => {
                            *event = command("content.collection_show", &[("name", name.as_str())]);
                        }
                        TreeClick::Menu(0) => {
                            *event = PanelEvent::Commands(vec![
                                (
                                    "draft.set".into(),
                                    CommandArgs::new(&[
                                        ("key", "content-collection-renaming"),
                                        ("value", name.as_str()),
                                    ]),
                                ),
                                (
                                    "draft.set".into(),
                                    CommandArgs::new(&[
                                        ("key", "content-collection-rename"),
                                        ("value", name.as_str()),
                                    ]),
                                ),
                            ]);
                        }
                        TreeClick::Menu(1) => {
                            *event =
                                command("content.collection_delete", &[("name", name.as_str())]);
                        }
                        _ => {}
                    }
                }
            });
    }
}

/// Middle column: one toggle per asset type.
fn content_filters(frame: &mut PanelFrame, session: &EditorSession, event: &mut PanelEvent) {
    let text = |key: &str| session.messages.get(key);
    let active = &session.models.asset_browser.filters;
    let ui = frame.ui_mut();
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, ui.visuals().faint_bg_color);
    ui.painter().text(
        egui::pos2(rect.left() + 8.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        text("content-filters"),
        egui::TextStyle::Button.resolve(ui.style()),
        ui.visuals().strong_text_color(),
    );
    ui.add_space(4.0);
    egui::ScrollArea::vertical().id_salt("content-filter-list").auto_shrink([false, false]).show(
        ui,
        |ui| {
            for kind in seal_editor_core::FILTER_KINDS {
                let on = active.iter().any(|k| k == kind);
                let label = text(&format!("content-kind-{kind}"));
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width() - 8.0, 22.0),
                    egui::Sense::click(),
                );
                let rect = rect.translate(egui::vec2(4.0, 0.0));
                let visuals = ui.visuals();
                let fill = if on {
                    visuals.selection.bg_fill
                } else if response.hovered() {
                    visuals.widgets.hovered.weak_bg_fill
                } else {
                    visuals.widgets.inactive.weak_bg_fill
                };
                ui.painter().rect_filled(rect, 1.0, fill);
                let strip = egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height()));
                ui.painter().rect_filled(strip, 0.0, seal_ui::asset_kind_color(kind));
                ui.painter().text(
                    egui::pos2(rect.left() + 10.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    &label,
                    egui::TextStyle::Body.resolve(ui.style()),
                    if on { visuals.strong_text_color() } else { visuals.text_color() },
                );
                let response = response.on_hover_text(text("content-filter-hint"));
                response.widget_info(|| {
                    egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, &label)
                });
                if response.clicked() {
                    *event = command("content.filter", &[("kind", kind)]);
                }
                ui.add_space(3.0);
            }
            if !active.is_empty() {
                ui.add_space(4.0);
                if PanelFrame::new(ui).button(&text("content-filter-clear")) {
                    *event = command("content.filter", &[("kind", "")]);
                }
            }
        },
    );
}

/// Right side: toolbar, path, search and the grid of the current folder.
/// A toolbar strip that keeps everything on one line.
fn content_bar(frame: &mut PanelFrame, body: impl FnOnce(&mut PanelFrame)) {
    let ui = frame.ui_mut();
    let fill = ui.visuals().extreme_bg_color;
    let line = ui.visuals().widgets.noninteractive.bg_stroke;
    let response = egui::Frame::new()
        .fill(fill)
        .inner_margin(egui::Margin::symmetric(6, 3))
        .show(ui, |ui| {
            let width = ui.available_width();
            ui.set_width(width);
            ui.horizontal(|ui| body(&mut PanelFrame::new(ui)));
        })
        .response;
    ui.painter().hline(response.rect.x_range(), response.rect.bottom(), line);
}

fn content_grid_side(frame: &mut PanelFrame, session: &EditorSession, event: &mut PanelEvent) {
    let models = &session.models;
    let text = |key: &str| session.messages.get(key);
    let state = &models.asset_browser;
    let here = seal_editor_core::current_rel(session);
    let here_abs = models.content_root.clone();
    // Toolbar.
    content_bar(frame, |frame| {
        let ui = frame.ui_mut();
        ui.menu_button(format!("+ {}", text("content-add")), |ui| {
            for action in [ContentAction::NewFolder, ContentAction::NewScene, ContentAction::Import]
            {
                if ui.button(text(action.label_key())).clicked() {
                    *event = content_action_event(
                        action,
                        None,
                        None,
                        here_abs.as_deref(),
                        models.content_tree_root.as_deref(),
                    );
                    ui.close();
                }
            }
            // A new game UI screen in the folder being shown (ui/ at the top).
            if ui.button(text("content-new-strings")).clicked() {
                let path =
                    if here.is_empty() { "ui/text".to_string() } else { format!("{here}/text") };
                *event =
                    command("asset.strings_new", &[("path", path.as_str()), ("unique", "yes")]);
                ui.close();
            }
            if ui.button(text("content-new-test")).clicked() {
                let path = if here.is_empty() {
                    "tests/check".to_string()
                } else {
                    format!("{here}/check")
                };
                *event = command("tests.new", &[("path", path.as_str()), ("unique", "yes")]);
                ui.close();
            }
            if ui.button(text("content-new-soundset")).clicked() {
                let path = if here.is_empty() {
                    "sounds/container".to_string()
                } else {
                    format!("{here}/container")
                };
                *event =
                    command("asset.soundset_new", &[("path", path.as_str()), ("unique", "yes")]);
                ui.close();
            }
            if ui.button(text("content-new-ui-screen")).clicked() {
                let path = if here.is_empty() {
                    "ui/screen".to_string()
                } else {
                    format!("{here}/screen")
                };
                *event = command("asset.ui_new", &[("path", path.as_str())]);
                ui.close();
            }
            // Same pattern for the other asset kinds: the folder shown, else a default folder.
            for (key, cmd, top, stem) in [
                ("content-new-effect", "asset.fx_new", "fx", "effect"),
                ("content-new-animation", "asset.frames_new", "sprites", "animation"),
                ("content-new-datatype", "asset.datatype_new", "data/types", "type"),
                ("content-new-texture-array", "asset.texture_array_new", "textures", "array"),
                ("content-new-texture-volume", "asset.texture_volume_new", "textures", "volume"),
                ("content-new-texture-cubemap", "asset.texture_cubemap_new", "textures", "cubemap"),
            ] {
                if ui.button(text(key)).clicked() {
                    let path = if here.is_empty() {
                        format!("{top}/{stem}")
                    } else {
                        format!("{here}/{stem}")
                    };
                    *event = command(cmd, &[("path", path.as_str()), ("unique", "yes")]);
                    ui.close();
                }
            }
        });
        if frame.button(&text("content-import")) {
            *event = content_action_event(
                ContentAction::Import,
                None,
                None,
                here_abs.as_deref(),
                models.content_tree_root.as_deref(),
            );
        }
        if frame.button(&text("content-save-all")) {
            *event = command("file.save_all", &[]);
        }
        if frame.icon_button(seal_ui::Icon::ViewArrowBack, &text("content-back"))
            && !state.back.is_empty()
        {
            *event = command("content.back", &[]);
        }
        if frame.icon_button(seal_ui::Icon::ViewArrowForward, &text("content-forward"))
            && !state.forward.is_empty()
        {
            *event = command("content.forward", &[]);
        }
        // Path: All > folder > folder, each part opens its folder.
        let ui = frame.ui_mut();
        let path_w = (ui.available_width() - 56.0).max(60.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(path_w, 22.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 1.0, ui.visuals().extreme_bg_color);
        let mut path_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink2(egui::vec2(6.0, 0.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        path_ui.spacing_mut().item_spacing.x = 4.0;
        let mut crumbs: Vec<(String, String)> = Vec::new();
        if let Some(name) = &state.collection {
            crumbs.push((text("content-collections"), String::new()));
            crumbs.push((name.clone(), String::new()));
        } else {
            crumbs.push((text("content-all"), String::new()));
            let mut at = String::new();
            for part in here.split('/').filter(|p| !p.is_empty()) {
                at = if at.is_empty() { part.to_string() } else { format!("{at}/{part}") };
                crumbs.push((part.to_string(), at.clone()));
            }
        }
        let last = crumbs.len().saturating_sub(1);
        for (index, (label, rel)) in crumbs.iter().enumerate() {
            let link = path_ui.add(egui::Label::new(label.as_str()).sense(egui::Sense::click()));
            if link.hovered() {
                path_ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if link.clicked() && (state.collection.is_none() || index == 0) {
                *event = command("content.go", &[("path", rel.as_str())]);
            }
            if index < last || state.collection.is_none() {
                seal_ui::paint_icon(
                    path_ui.painter(),
                    path_ui.cursor().min + egui::vec2(6.0, 11.0),
                    seal_ui::Icon::ViewChevronRight,
                    10.0,
                    path_ui.visuals().weak_text_color(),
                );
                path_ui.add_space(12.0);
            }
        }
        let mut frame2 = PanelFrame::new(ui);
        frame2.icon_menu(seal_ui::Icon::EditSettings, &text("content-settings"), |ui| {
            let list = models.content_list;
            if ui.radio(!list, text("content-view-tiles")).clicked() && list {
                *event = command("content.toggle-view", &[]);
                ui.close();
            }
            if ui.radio(list, text("content-view-list")).clicked() && !list {
                *event = command("content.toggle-view", &[]);
                ui.close();
            }
            ui.separator();
            let hidden = session.foundation.settings.get("content.show_hidden") == Some("true");
            let mut show = hidden;
            if ui.checkbox(&mut show, text("content-show-hidden")).changed() {
                let value = if show { "true" } else { "false" };
                *event =
                    command("settings.set", &[("id", "content.show_hidden"), ("value", value)]);
                ui.close();
            }
        });
    });
    // Search row: type filter menu, search, sort menu.
    content_bar(frame, |frame| {
        frame.icon_menu(seal_ui::Icon::EditFilter, &text("content-filters"), |ui| {
            for kind in seal_editor_core::FILTER_KINDS {
                let mut on = state.filters.iter().any(|k| k == kind);
                if ui.checkbox(&mut on, text(&format!("content-kind-{kind}"))).changed() {
                    *event = command("content.filter", &[("kind", kind)]);
                }
            }
            if !state.filters.is_empty() {
                ui.separator();
                if ui.button(text("content-filter-clear")).clicked() {
                    *event = command("content.filter", &[("kind", "")]);
                    ui.close();
                }
            }
        });
        let ui = frame.ui_mut();
        let search_w = (ui.available_width() - 60.0).max(80.0);
        let mut query = models.content_query.clone();
        let mut search = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_min_size(ui.cursor().min, egui::vec2(search_w, 22.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        if PanelFrame::new(&mut search).search(&mut query, &text("content-search-hint")) {
            *event = command("content.query", &[("query", query.as_str())]);
        }
        ui.advance_cursor_after_rect(egui::Rect::from_min_size(
            ui.cursor().min,
            egui::vec2(search_w, 22.0),
        ));
        let mut frame = PanelFrame::new(ui);
        frame.icon_menu(seal_ui::Icon::ViewSort, &text("content-sort"), |ui| {
            for by in seal_editor_core::SortBy::ALL {
                let label = text(&format!("content-sort-{}", by.id()));
                if ui.radio(state.sort == by, label).clicked() {
                    if state.sort != by {
                        *event = command("content.sort", &[("by", by.id())]);
                    }
                    ui.close();
                }
            }
            ui.separator();
            let mut down = state.descending;
            if ui.checkbox(&mut down, text("content-sort-descending")).changed() {
                *event = command("content.sort", &[]);
                ui.close();
            }
        });
    });
    let items = seal_editor_core::browser_items(session);
    let count = items.len();
    let picked = models.content_pick.clone().unwrap_or_default();
    let selected = items.iter().position(|it| !picked.is_empty() && it.rel == picked);
    // Footer first so the grid takes the rest.
    let ui = frame.ui_mut();
    let body = ui.available_rect_before_wrap();
    let footer = egui::Rect::from_min_max(egui::pos2(body.left(), body.bottom() - 24.0), body.max);
    let grid_rect = body.with_max_y(footer.top());
    let count_text = if count == 1 {
        text("content-items-one")
    } else {
        session.messages.format("content-items-many", &[("count", &count.to_string())])
    };
    let count_text = if selected.is_some() {
        session.messages.format("content-items-picked", &[("items", &count_text)])
    } else {
        count_text
    };
    ui.painter().rect_filled(footer, 0.0, ui.visuals().extreme_bg_color);
    ui.painter().hline(
        footer.x_range(),
        footer.top(),
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    ui.painter().text(
        egui::pos2(footer.left() + 8.0, footer.center().y),
        egui::Align2::LEFT_CENTER,
        count_text,
        egui::TextStyle::Small.resolve(ui.style()),
        ui.visuals().text_color(),
    );
    let mut grid_ui = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("content-grid-body")
            .max_rect(grid_rect)
            .layout(egui::Layout::top_down(egui::Align::LEFT)),
    );
    grid_ui.set_clip_rect(grid_rect.intersect(ui.clip_rect()));
    let mut grid_frame = PanelFrame::new(&mut grid_ui);
    if models.content_list {
        if let Some(found) = content_list_view(&mut grid_frame, session) {
            *event = found;
        }
    } else {
        if let Some(found) = content_tiles(&mut grid_frame, session, &items, selected) {
            *event = found;
        }
    }
    ui.advance_cursor_after_rect(body);
}

fn content_tiles(
    frame: &mut PanelFrame,
    session: &EditorSession,
    items: &[seal_editor_core::BrowserItem],
    selected: Option<usize>,
) -> Option<PanelEvent> {
    let models = &session.models;
    let text = |key: &str| session.messages.get(key);
    let state = &models.asset_browser;
    let root = models.content_tree_root.clone();
    if let Some(project_root) = &root {
        let jobs: Vec<(String, std::path::PathBuf, String)> = items
            .iter()
            .filter(|it| !it.is_dir)
            .map(|it| (it.rel.clone(), project_root.join(&it.rel), it.kind.to_string()))
            .collect();
        seal_editor_core::asset_thumbnails::warm(
            project_root,
            &jobs,
            seal_editor_core::worker_thread_count(&session.foundation.settings),
        );
    }
    let tiles: Vec<seal_ui::FileTile> = items
        .iter()
        .map(|it| {
            let thumb_disk = root.as_ref().and_then(|project_root| {
                if it.is_dir {
                    return None;
                }
                let full = project_root.join(&it.rel);
                seal_editor_core::asset_thumbnails::ensure_thumbnail(
                    project_root,
                    &it.rel,
                    &full,
                    it.kind,
                )
            });
            seal_ui::FileTile {
                label: it.label.clone(),
                folder: it.is_dir,
                path: root.as_ref().map(|r| r.join(&it.rel)),
                kind: it.kind,
                kind_label: text(&format!("content-kind-{}", it.kind)),
                drag_path: if it.is_dir { String::new() } else { it.rel.clone() },
                thumb_disk,
            }
        })
        .collect();
    let renaming_rel = session.draft("content-renaming").filter(|name| !name.is_empty());
    let renaming_index =
        renaming_rel.as_deref().and_then(|target| items.iter().position(|it| it.rel == target));
    let mut rename_text = session
        .draft("content-rename")
        .or_else(|| {
            renaming_index.map(|i| items[i].rel.rsplit('/').next().unwrap_or("").to_string())
        })
        .unwrap_or_default();
    let collection_names: Vec<String> = state.collections.keys().cloned().collect();
    let actions_at = |index: usize| -> Vec<ContentAction> {
        items.get(index).map_or_else(Vec::new, |it| {
            let name = it.rel.rsplit('/').next().unwrap_or(&it.rel);
            content_item_actions(it.is_dir, name)
        })
    };
    // Extra entries after the item actions: favorite (folders), collections.
    let extras_at = |index: usize| -> Vec<(String, PanelEvent)> {
        let Some(it) = items.get(index) else {
            return Vec::new();
        };
        let mut extra = Vec::new();
        if it.is_dir {
            let key = if state.favorites.contains(&it.rel) {
                "content-menu-favorite-remove"
            } else {
                "content-menu-favorite-add"
            };
            extra.push((text(key), command("content.favorite", &[("path", it.rel.as_str())])));
        } else if let Some(current) = &state.collection {
            extra.push((
                text("content-menu-remove-from-collection"),
                command(
                    "content.collection_remove",
                    &[("name", current.as_str()), ("path", it.rel.as_str())],
                ),
            ));
        } else {
            for name in &collection_names {
                extra.push((
                    session.messages.format("content-menu-add-to-collection", &[("name", name)]),
                    command(
                        "content.collection_add",
                        &[("name", name.as_str()), ("path", it.rel.as_str())],
                    ),
                ));
            }
        }
        if !it.is_dir {
            extra.push((
                text("content-menu-find-references"),
                command("asset.find_references", &[("path", it.rel.as_str())]),
            ));
            if !seal_editor_core::asset_database_ops::duplicate_paths(
                &session.models.asset_index,
                &it.rel,
            )
            .is_empty()
            {
                extra.push((
                    text("content-menu-merge-duplicates"),
                    command("asset.merge_duplicates", &[("path", it.rel.as_str())]),
                ));
            }
            extra.push((
                text("content-menu-copy-to-project"),
                command("asset.copy_to_project", &[("path", it.rel.as_str())]),
            ));
            extra.extend(crate::import_preset_ui::browser_preset_menu(session, &it.rel));
        }
        extra
    };
    let menu = |index: usize| -> Vec<String> {
        let mut entries: Vec<String> =
            actions_at(index).into_iter().map(|a| text(a.label_key())).collect();
        entries.extend(extras_at(index).into_iter().map(|(label, _)| label));
        entries
    };
    let background: Vec<String> =
        CONTENT_EMPTY_ACTIONS.iter().map(|a| text(a.label_key())).collect();
    let renaming = renaming_index.map(|index| (index, &mut rename_text));
    let hit = frame.file_browser(&tiles, selected, &menu, &background, renaming)?;
    let open = |index: usize| -> PanelEvent {
        let Some(it) = items.get(index) else {
            return PanelEvent::None;
        };
        if it.is_dir {
            command("content.go", &[("path", it.rel.as_str())])
        } else {
            match open_content_file(&it.rel) {
                PanelEvent::None => command("content.pick", &[("path", it.rel.as_str())]),
                event => event,
            }
        }
    };
    let start_rename = |index: usize| -> PanelEvent {
        let Some(it) = items.get(index) else {
            return PanelEvent::None;
        };
        let name = it.rel.rsplit('/').next().unwrap_or(&it.rel);
        PanelEvent::Commands(vec![
            ("content.pick".into(), CommandArgs::new(&[("path", it.rel.as_str())])),
            (
                "draft.set".into(),
                CommandArgs::new(&[("key", "content-renaming"), ("value", it.rel.as_str())]),
            ),
            ("draft.set".into(), CommandArgs::new(&[("key", "content-rename"), ("value", name)])),
        ])
    };
    let stop_rename = || {
        ("draft.set".to_string(), CommandArgs::new(&[("key", "content-renaming"), ("value", "")]))
    };
    let here_abs = models.content_root.clone();
    Some(match hit {
        seal_ui::FileHit::Select(index) => match items.get(index) {
            Some(it) => command("content.pick", &[("path", it.rel.as_str())]),
            None => PanelEvent::None,
        },
        seal_ui::FileHit::Open(index) => open(index),
        seal_ui::FileHit::Rename(index) => start_rename(index),
        seal_ui::FileHit::Delete(index) => match items.get(index) {
            Some(it) => command("content.delete", &[("name", root_name(&it.rel).as_str())]),
            None => PanelEvent::None,
        },
        seal_ui::FileHit::Up => {
            if state.collection.is_some() || !state.back.is_empty() {
                command("content.back", &[])
            } else if here_is_root(session) {
                PanelEvent::None
            } else {
                let here = seal_editor_core::current_rel(session);
                let parent = here.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
                command("content.go", &[("path", parent)])
            }
        }
        seal_ui::FileHit::Menu(index, action) => {
            let actions = actions_at(index);
            match actions.get(action).copied() {
                Some(ContentAction::Open) => open(index),
                Some(ContentAction::Rename) => start_rename(index),
                Some(found) => {
                    let it = &items[index];
                    let name = root_name(&it.rel);
                    let full = root.as_ref().map(|r| r.join(&it.rel));
                    let inside = if it.is_dir {
                        full.clone()
                    } else {
                        full.as_ref().and_then(|f| f.parent().map(|p| p.to_path_buf()))
                    };
                    content_action_event(
                        found,
                        Some(&name),
                        full.as_deref(),
                        inside.as_deref(),
                        root.as_deref(),
                    )
                }
                None => extras_at(index)
                    .into_iter()
                    .nth(action - actions.len())
                    .map_or(PanelEvent::None, |(_, event)| event),
            }
        }
        seal_ui::FileHit::Background(action) => match CONTENT_EMPTY_ACTIONS.get(action) {
            Some(action) => {
                content_action_event(*action, None, None, here_abs.as_deref(), root.as_deref())
            }
            None => PanelEvent::None,
        },
        seal_ui::FileHit::RenameEdit(value) => {
            command("draft.set", &[("key", "content-rename"), ("value", value.as_str())])
        }
        seal_ui::FileHit::RenameCommit(value) => {
            let from_rel = renaming_rel.clone().unwrap_or_default();
            let from_name = from_rel.rsplit('/').next().unwrap_or(&from_rel).to_string();
            let mut cmds = vec![stop_rename()];
            if !value.is_empty() && value != from_name && !from_rel.is_empty() {
                cmds.push((
                    "content.rename".into(),
                    CommandArgs::new(&[
                        ("from", root_name(&from_rel).as_str()),
                        ("to", value.as_str()),
                    ]),
                ));
            }
            PanelEvent::Commands(cmds)
        }
        seal_ui::FileHit::RenameCancel => {
            let (id, args) = stop_rename();
            PanelEvent::Command(id, args)
        }
    })
}

fn here_is_root(session: &EditorSession) -> bool {
    seal_editor_core::current_rel(session).is_empty()
}

/// The whole project as an expandable list (View Options > List).
fn content_list_view(frame: &mut PanelFrame, session: &EditorSession) -> Option<PanelEvent> {
    let models = &session.models;
    let text = |key: &str| session.messages.get(key);
    let mut event = PanelEvent::None;
    let rows = content_tree_rows(session).unwrap_or_default();
    let list_renaming = session.draft("content-list-renaming").filter(|name| !name.is_empty());
    let mut list_rename_text = session.draft("content-rename").unwrap_or_default();
    frame.scroll(|frame| {
        frame.space(4.0);
        for row in &rows {
            let selected = models.content_pick.as_deref() == Some(row.rel.as_str())
                || (row.rel.is_empty() && models.content_pick.is_none());
            let detail = if row.is_symlink {
                text("content-entry-symlink")
            } else if row.is_dir {
                text("content-entry-folder")
            } else {
                text("content-entry-file")
            };
            let place_kind = if row.is_dir {
                None
            } else {
                let full = models
                    .content_tree_root
                    .as_ref()
                    .map(|root| root.join(&row.rel).to_string_lossy().to_string())
                    .unwrap_or_else(|| row.name.clone());
                seal_ui::PanelFrame::place_kind_from_label(&full)
            };
            let row_actions = if row.rel.is_empty() {
                CONTENT_EMPTY_ACTIONS.to_vec()
            } else {
                content_item_actions(row.is_dir, &row.name)
            };
            let row_menu: Vec<String> = row_actions.iter().map(|a| text(a.label_key())).collect();
            let thumb = (!row.is_dir && seal_ui::sprites::is_image_name(&row.name))
                .then(|| {
                    models
                        .content_tree_root
                        .as_ref()
                        .map(|root| root.join(&row.rel).to_string_lossy().to_string())
                })
                .flatten();
            match frame.tree_row(seal_ui::TreeRow {
                depth: row.depth,
                has_children: row.has_children,
                open: row.open,
                label: &row.name,
                detail: &detail,
                selected,
                hidden: false,
                locked: false,
                menu: &row_menu,
                drag_id: None,
                place_kind: place_kind.as_deref(),
                rename_text: if list_renaming.as_deref() == Some(row.rel.as_str()) {
                    Some(&mut list_rename_text)
                } else {
                    None
                },
                component_row: false,
                thumb: thumb.as_deref(),
                marks: false,
            }) {
                TreeClick::Rename(value) => {
                    let mut cmds = vec![(
                        "draft.set".to_string(),
                        CommandArgs::new(&[("key", "content-list-renaming"), ("value", "")]),
                    )];
                    if !value.is_empty() && value != row.name {
                        let from = if row.rel.contains('/') {
                            row.rel.clone()
                        } else {
                            format!("./{}", row.rel)
                        };
                        cmds.push((
                            "content.rename".into(),
                            CommandArgs::new(&[("from", from.as_str()), ("to", value.as_str())]),
                        ));
                    }
                    event = PanelEvent::Commands(cmds);
                }
                TreeClick::RenameCancel => {
                    event =
                        command("draft.set", &[("key", "content-list-renaming"), ("value", "")]);
                }
                _ if list_renaming.as_deref() == Some(row.rel.as_str())
                    && session.draft("content-rename").unwrap_or_default() != list_rename_text =>
                {
                    event = command(
                        "draft.set",
                        &[("key", "content-rename"), ("value", list_rename_text.as_str())],
                    );
                }
                TreeClick::Menu(index) => {
                    // Names with a slash resolve from the project root in the core.
                    let name = if row.rel.contains('/') {
                        row.rel.clone()
                    } else {
                        format!("./{}", row.rel)
                    };
                    let full = models.content_tree_root.as_ref().map(|root| root.join(&row.rel));
                    let inside = if row.is_dir {
                        full.clone()
                    } else {
                        full.as_ref().and_then(|f| f.parent().map(|p| p.to_path_buf()))
                    };
                    event = match row_actions.get(index).copied() {
                        Some(ContentAction::Open) if row.is_dir => {
                            command("content.enter", &[("path", row.rel.as_str())])
                        }
                        Some(ContentAction::Open) => open_content_file(&row.rel),
                        Some(ContentAction::Rename) if !row.rel.is_empty() => {
                            PanelEvent::Commands(vec![
                                (
                                    "draft.set".into(),
                                    CommandArgs::new(&[
                                        ("key", "content-list-renaming"),
                                        ("value", row.rel.as_str()),
                                    ]),
                                ),
                                (
                                    "draft.set".into(),
                                    CommandArgs::new(&[
                                        ("key", "content-rename"),
                                        ("value", row.name.as_str()),
                                    ]),
                                ),
                            ])
                        }
                        Some(action) if row.rel.is_empty() => content_action_event(
                            action,
                            None,
                            None,
                            full.as_deref(),
                            models.content_tree_root.as_deref(),
                        ),
                        Some(action) => content_action_event(
                            action,
                            Some(&name),
                            full.as_deref(),
                            inside.as_deref(),
                            models.content_tree_root.as_deref(),
                        ),
                        None => PanelEvent::None,
                    };
                }
                TreeClick::FrameEntity if !row.is_dir => {
                    event = open_content_file(&row.rel);
                }
                TreeClick::Toggle if row.is_dir => {
                    event = command("content.toggle_tree", &[("path", row.rel.as_str())]);
                }
                TreeClick::Select if row.is_dir && !row.rel.is_empty() => {
                    event = command("content.enter", &[("path", row.rel.as_str())]);
                }
                TreeClick::Select => {
                    if row.rel.is_empty() {
                        event = command("content.pick", &[]);
                    } else {
                        event = command("content.pick", &[("path", row.rel.as_str())]);
                    }
                }
                _ => {}
            }
        }
    });
    match event {
        PanelEvent::None => None,
        found => Some(found),
    }
}

fn wizard_panel(frame: &mut PanelFrame, session: &EditorSession) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let mut event = PanelEvent::None;
    frame.scroll(|frame| {
        frame.padded(|frame| {
            frame.title(&text("panel-wizard-title"));
            let old_name = wizard_name(session);
            let projects = seal_editor_core::default_projects_dir();
            let mut folder = session
                .draft(DRAFT_FOLDER)
                .unwrap_or_else(|| projects.join(old_name.trim()).to_string_lossy().into());
            let mut name = old_name.clone();
            if frame.search(&mut name, &text("wizard-name-hint")) {
                let set = |key: &str, value: &str| {
                    ("draft.set".to_string(), CommandArgs::new(&[("key", key), ("value", value)]))
                };
                let mut cmds = vec![set(DRAFT_NAME, name.as_str())];
                // The folder follows the name until the user picks a folder of their own.
                let here = std::path::Path::new(&folder);
                if here == projects || here == projects.join(old_name.trim()) {
                    folder = projects.join(name.trim()).to_string_lossy().into();
                    cmds.push(set(DRAFT_FOLDER, folder.as_str()));
                }
                event = PanelEvent::Commands(cmds);
            }
            if frame.search(&mut folder, &text("wizard-folder-hint")) {
                event = command("draft.set", &[("key", DRAFT_FOLDER), ("value", folder.as_str())]);
            }
            if frame.button(&text("wizard-browse")) {
                event = command("wizard.browse", &[]);
            }
            let mut templates: Vec<(String, String)> = seal_editor_core::PROJECT_TEMPLATES
                .iter()
                .map(|t| (t.id.to_string(), text(t.title_key)))
                .collect();
            for user in seal_editor_core::list_user_templates() {
                templates.push((user.id, user.title));
            }
            let choices: Vec<(&str, String)> =
                templates.iter().map(|(id, title)| (id.as_str(), title.clone())).collect();
            let current = seal_editor_core::wizard_template(session);
            if let Some(picked) = frame.choice(&text("wizard-template"), current.as_str(), &choices)
            {
                event =
                    command("draft.set", &[("key", DRAFT_TEMPLATE), ("value", picked.as_str())]);
            }
            if let Some(user) = seal_editor_core::user_template_by_id(&current) {
                frame.label(&user.description);
            } else if let Some(t) = seal_editor_core::template_by_id(&current) {
                frame.label(&session.messages.get(t.description_key));
                let (px, w, h) = t.preview;
                if !px.is_empty() && w > 0 && h > 0 {
                    frame.ui_mut().add(|ui: &mut egui::Ui| {
                        let size = egui::vec2(96.0, 96.0);
                        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
                        let img =
                            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], px);
                        let tex = ui.ctx().load_texture(
                            format!("wizard-tpl-{}", t.id),
                            img,
                            egui::TextureOptions::NEAREST,
                        );
                        ui.painter().image(
                            tex.id(),
                            rect,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                        response
                    });
                }
            }
            let git_on = session.draft(DRAFT_GIT).as_deref() == Some("1");
            if frame.toggle(&text("wizard-git"), git_on) {
                let value = if git_on { "0" } else { "1" };
                event = command("draft.set", &[("key", DRAFT_GIT), ("value", value)]);
            }
            let lfs_on = session.draft(DRAFT_LFS).as_deref().is_none_or(|v| v != "0");
            if git_on && frame.toggle(&text("wizard-lfs"), lfs_on) {
                let value = if lfs_on { "0" } else { "1" };
                event = command("draft.set", &[("key", DRAFT_LFS), ("value", value)]);
            }
            let confirm = session.draft(DRAFT_CONFIRM_NONEMPTY).as_deref() == Some("1");
            if frame.toggle(&text("wizard-confirm-nonempty"), confirm) {
                let value = if confirm { "0" } else { "1" };
                event = command("draft.set", &[("key", DRAFT_CONFIRM_NONEMPTY), ("value", value)]);
            }
            let validation = seal_editor_core::validate_wizard(
                &session.messages,
                &name,
                std::path::Path::new(&folder),
                confirm,
            );
            if let Some(err) = &validation.name {
                let args: Vec<(&str, &str)> =
                    err.args.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                frame.label(&session.messages.format(&err.key, &args));
            }
            if let Some(err) = &validation.folder {
                let args: Vec<(&str, &str)> =
                    err.args.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                frame.label(&session.messages.format(&err.key, &args));
            }
            if frame.button(&text("wizard-create")) {
                event = command("wizard.create", &[]);
            }
            if frame.button(&text("wizard-cancel")) {
                event = command("wizard.cancel", &[]);
            }
        });
    });
    event
}

const VIEWPORT_YAW_DRAFT: &str = "viewport-yaw";
/// Project-relative picture the Create panel paints into the viewport; empty for none.
const PLACE_BRUSH_DRAFT: &str = "place-brush";
const TILE_GRID_DRAFT: &str = "tile-grid";
const TILE_LAYER_DRAFT: &str = "tile-layer";
const TILE_ERASE_DRAFT: &str = "tile-erase";
const VIEWPORT_PITCH_DRAFT: &str = "viewport-pitch";
const VIEWPORT_DISTANCE_DRAFT: &str = "viewport-distance";
const VIEWPORT_PAN_X_DRAFT: &str = "viewport-pan-x";
const VIEWPORT_PAN_Y_DRAFT: &str = "viewport-pan-y";
const DEFAULT_VIEWPORT_DISTANCE: f32 = 10.0;

fn draft_f32(session: &EditorSession, key: &str, default: f32) -> f32 {
    session.draft(key).and_then(|text| text.parse().ok()).unwrap_or(default)
}

fn viewport_layout(
    view: &str,
) -> (usize, usize, usize, f32, f32, (&'static str, &'static str), bool) {
    let wireframe = view != "perspective" && view != "2d";
    match view {
        "top" => (0, 2, 1, 1.0, 1.0, ("X", "Z"), wireframe),
        "bottom" => (0, 2, 1, 1.0, -1.0, ("X", "Z"), wireframe),
        "left" => (2, 1, 0, -1.0, 1.0, ("Z", "Y"), wireframe),
        "right" | "side" => (2, 1, 0, 1.0, 1.0, ("Z", "Y"), wireframe),
        "back" => (0, 1, 2, -1.0, 1.0, ("X", "Y"), wireframe),
        _ => (0, 1, 2, 1.0, 1.0, ("X", "Y"), wireframe),
    }
}

/// Pan that puts scene point (x, y) in the middle of the view, using the same
/// scale the viewport draws with.
fn follow_pan(camera: &ViewportCamera, perspective: bool, x: f32, y: f32) -> (f32, f32) {
    let dolly =
        if perspective { DEFAULT_VIEWPORT_DISTANCE / camera.distance.max(1.0) } else { 1.0 };
    let pitch_scale =
        if perspective { camera.pitch_deg.to_radians().cos().abs().max(0.35) } else { 1.0 };
    let cell = 20.0 * camera.zoom * dolly * pitch_scale;
    let (sin_yaw, cos_yaw) = camera.yaw_deg.to_radians().sin_cos();
    let rx = x * cos_yaw - y * sin_yaw;
    let ry = x * sin_yaw + y * cos_yaw;
    (-rx * cell, ry * cell)
}

fn viewport_camera_state(session: &EditorSession) -> (f32, f32, f32, f32, f32) {
    // Perspective opens at a three-quarter angle so it never looks like the flat front view.
    let perspective = session.models.viewport_view == "perspective";
    let (yaw0, pitch0) = if perspective { (38.0, -22.0) } else { (0.0, 0.0) };
    let yaw = draft_f32(session, VIEWPORT_YAW_DRAFT, yaw0);
    let pitch = draft_f32(session, VIEWPORT_PITCH_DRAFT, pitch0);
    let distance = draft_f32(session, VIEWPORT_DISTANCE_DRAFT, DEFAULT_VIEWPORT_DISTANCE);
    let pan_x = draft_f32(session, VIEWPORT_PAN_X_DRAFT, 0.0);
    let pan_y = draft_f32(session, VIEWPORT_PAN_Y_DRAFT, 0.0);
    (yaw, pitch, distance, pan_x, pan_y)
}

fn grade_channels(rgb: [u8; 3], post: &seal_editor_core::post_volume::Resolved) -> [u8; 3] {
    let sample = rgb.map(|channel| channel as f32 / 255.0);
    seal_editor_core::post_volume::grade_rgb(sample, post.tint, post.exposure, post.saturation)
        .map(|channel| (channel.clamp(0.0, 1.0) * 255.0) as u8)
}

pub(crate) fn mesh_surface<'a>(
    session: &'a seal_editor_core::EditorSession,
    entity: &'a seal_editor_core::Entity,
) -> seal_ui::MeshSurface<'a> {
    let mat = &entity.mesh_material;
    if mat.texture.is_empty() {
        seal_ui::MeshSurface::EMPTY
    } else {
        let kind = texture_sample_kind(session, &mat.texture);
        seal_ui::MeshSurface {
            path: Some(mat.texture.as_str()),
            layer: mat.layer,
            slice: mat.slice,
            tile: mat.tile,
            kind,
        }
    }
}

fn texture_sample_kind(
    session: &seal_editor_core::EditorSession,
    path: &str,
) -> seal_ui::gpu_textures::SampleKind {
    let picture = seal_ui::gpu_textures::sample_kind_for_picture();
    if !path.rsplit('.').next().is_some_and(|ext| ext.eq_ignore_ascii_case("texture")) {
        return picture;
    }
    let Some(root) = session.project.as_ref().map(|project| project.root.clone()) else {
        return picture;
    };
    let Ok(text) = std::fs::read_to_string(root.join(path)) else {
        return picture;
    };
    seal_editor_core::TextureAsset::parse(&text)
        .map(|asset| seal_ui::gpu_textures::sample_kind_for_texture(asset.kind))
        .unwrap_or(picture)
}

fn viewport(frame: &mut PanelFrame, session: &mut EditorSession, panel_id: &str) -> PanelEvent {
    if let Some(zoom) = session.world.camera2d_object().map(|entity| entity.camera2d.zoom) {
        let half = cam_drag_view::view_half(frame.view_size(), session.models.viewport_zoom, zoom);
        session.world.set_camera_view_half(half);
    }
    let text = |key: &str| session.messages.get(key);
    let models = &session.models;
    if panel_id != "viewport" {
        return viewport_multi::paint_extra_tab(frame, session, panel_id);
    }
    let editing = session.world.mode == seal_editor_core::PlayMode::Editing;
    let (across, up, depth_axis, flip_across, flip_up, names, wireframe) =
        viewport_layout(models.viewport_view.as_str());
    let view_entries = [
        ("perspective", text("viewport-view-perspective")),
        ("front", text("viewport-view-front")),
        ("back", text("viewport-view-back")),
        ("top", text("viewport-view-top")),
        ("bottom", text("viewport-view-bottom")),
        ("left", text("viewport-view-left")),
        ("right", text("viewport-view-right")),
        ("2d", text("viewport-view-2d")),
    ];
    let tool_tips = [
        (seal_ui::Icon::ToolSelect, "select", text("tooltip-viewport-tool-select")),
        (seal_ui::Icon::ToolMove, "move", text("tooltip-viewport-tool-move")),
        (seal_ui::Icon::ToolRotate, "rotate", text("tooltip-viewport-tool-rotate")),
        (seal_ui::Icon::ToolScale, "scale", text("tooltip-viewport-tool-scale")),
        (seal_ui::Icon::ToolMeasure, "measure", text("tooltip-viewport-tool-measure")),
        (seal_ui::Icon::ToolTransform, "collider", text("tooltip-viewport-tool-collider")),
    ];
    let view_refs: Vec<(&str, &str)> =
        view_entries.iter().map(|(id, label)| (*id, label.as_str())).collect();
    let tool_refs: Vec<(seal_ui::Icon, &str, &str)> =
        tool_tips.iter().map(|(icon, id, tip)| (*icon, *id, tip.as_str())).collect();
    let shade_entries = [
        ("lit", text("viewport-shade-lit"), true),
        ("unlit", text("viewport-shade-unlit"), true),
        ("wireframe", text("viewport-shade-wireframe"), true),
        ("collision", text("viewport-shade-collision"), true),
        ("lighting_only", text("viewport-shade-lighting-only"), true),
        ("overdraw", text("viewport-shade-overdraw"), true),
        ("base_colour", text("viewport-shade-base-colour"), true),
        ("normals", text("viewport-shade-normals"), true),
        ("depth", text("viewport-shade-depth"), true),
        ("detail", text("viewport-shade-detail-lighting"), false),
        ("reflections", text("viewport-shade-reflections"), false),
    ];
    let shade_refs: Vec<(&str, &str, bool)> =
        shade_entries.iter().map(|(id, label, on)| (*id, label.as_str(), *on)).collect();
    let show_layer_ids = seal_editor_core::VIEWPORT_SHOW_LAYER_IDS;
    let show_entries: Vec<(&str, String)> =
        show_layer_ids.iter().map(|id| (*id, text(&format!("viewport-show-{id}")))).collect();
    let show_refs: Vec<(&str, &str)> =
        show_entries.iter().map(|(id, label)| (*id, label.as_str())).collect();
    let show_title = text("viewport-show-pill");
    let show_defaults = text("viewport-show-use-defaults");
    let show_menu = seal_ui::ViewportShowMenu {
        title: show_title.as_str(),
        defaults_label: show_defaults.as_str(),
        layers: &models.viewport_show_layers,
        entries: &show_refs,
        menu_open: models.viewport_show_menu_open,
    };
    let bookmark_store_rows: Vec<(String, String)> = (0..10)
        .map(|slot| {
            let slot_s = slot.to_string();
            let label = session
                .messages
                .format("viewport-menu-bookmark-store-slot", &[("slot", slot_s.as_str())]);
            (slot_s, label)
        })
        .collect();
    let bookmark_jump_rows: Vec<(String, String)> = (0..10)
        .map(|slot| {
            let slot_s = slot.to_string();
            let label = session
                .messages
                .format("viewport-menu-bookmark-jump-slot", &[("slot", slot_s.as_str())]);
            (slot_s, label)
        })
        .collect();
    let bookmark_store_refs: Vec<(&str, &str)> =
        bookmark_store_rows.iter().map(|(slot, label)| (slot.as_str(), label.as_str())).collect();
    let bookmark_jump_refs: Vec<(&str, &str)> =
        bookmark_jump_rows.iter().map(|(slot, label)| (slot.as_str(), label.as_str())).collect();
    let layout_rows = [
        ("layout_single", text("viewport-menu-layout-single"), true),
        ("layout_two_cols", text("viewport-menu-layout-two-cols"), true),
        ("layout_two_rows", text("viewport-menu-layout-two-rows"), true),
        ("layout_three", text("viewport-menu-layout-three"), true),
        ("layout_four", text("viewport-menu-layout-four"), true),
    ];
    let layout_refs: Vec<(&str, &str, bool)> =
        layout_rows.iter().map(|(id, label, on)| (*id, label.as_str(), *on)).collect();
    let ham_tooltip = text("tooltip-viewport-hamburger");
    let ham_realtime = text("viewport-menu-realtime");
    let ham_show_stats = text("viewport-menu-show-stats");
    let ham_show_fps = text("viewport-menu-show-fps");
    let ham_show_toolbar = text("viewport-menu-show-toolbar");
    let ham_fov = text("viewport-menu-fov");
    let ham_far = text("viewport-menu-far-plane");
    let ham_screen = text("viewport-menu-screen-pct");
    let ham_cinematic = text("viewport-menu-cinematic");
    let ham_game_view = text("viewport-menu-game-view");
    let ham_immersive = text("viewport-menu-immersive");
    let ham_bookmarks = text("viewport-menu-bookmarks");
    let ham_bookmark_store_heading = text("viewport-menu-bookmark-store-heading");
    let ham_bookmark_jump_heading = text("viewport-menu-bookmark-jump-heading");
    let ham_create_camera = text("viewport-menu-create-camera");
    let ham_screenshot = text("viewport-menu-screenshot");
    let ham_layouts = text("viewport-menu-layouts");
    let ham_advanced = text("viewport-menu-advanced");
    let ham_isolate = text("viewport-menu-isolate");
    let ham_overlays = text("viewport-menu-overlays");
    let ham_collapse = text("viewport-menu-overlay-collapse-toolbar");
    let ham_stats_edge = text("viewport-menu-overlay-stats-corner");
    let edge_tl = text("viewport-menu-overlay-edge-top-left");
    let edge_tr = text("viewport-menu-overlay-edge-top-right");
    let edge_bl = text("viewport-menu-overlay-edge-bottom-left");
    let edge_br = text("viewport-menu-overlay-edge-bottom-right");
    let overlay_layout =
        seal_editor_core::viewport_overlays::load_from_panel_states(&models.panel_states);
    let overlay_edges = [
        ("tl", edge_tl.as_str()),
        ("tr", edge_tr.as_str()),
        ("bl", edge_bl.as_str()),
        ("br", edge_br.as_str()),
    ];
    let hamburger = seal_ui::ViewportHamburgerMenu {
        menu_open: models.viewport_hamburger_open,
        tooltip: ham_tooltip.as_str(),
        realtime: models.viewport_realtime,
        show_stats: models.viewport_show_stats,
        show_fps: models.viewport_show_fps,
        show_toolbar: models.viewport_show_toolbar,
        cinematic: models.viewport_cinematic,
        fov: models.viewport_fov,
        far_plane: models.viewport_far_plane,
        screen_pct: models.viewport_screen_pct,
        lbl_realtime: ham_realtime.as_str(),
        lbl_show_stats: ham_show_stats.as_str(),
        lbl_show_fps: ham_show_fps.as_str(),
        lbl_show_toolbar: ham_show_toolbar.as_str(),
        lbl_fov: ham_fov.as_str(),
        lbl_far_plane: ham_far.as_str(),
        lbl_screen_pct: ham_screen.as_str(),
        lbl_cinematic: ham_cinematic.as_str(),
        lbl_game_view: ham_game_view.as_str(),
        lbl_immersive: ham_immersive.as_str(),
        lbl_bookmarks: ham_bookmarks.as_str(),
        lbl_bookmark_store_heading: ham_bookmark_store_heading.as_str(),
        lbl_bookmark_jump_heading: ham_bookmark_jump_heading.as_str(),
        lbl_create_camera: ham_create_camera.as_str(),
        lbl_screenshot: ham_screenshot.as_str(),
        lbl_layouts: ham_layouts.as_str(),
        lbl_advanced: ham_advanced.as_str(),
        lbl_isolate: ham_isolate.as_str(),
        lbl_overlays: ham_overlays.as_str(),
        lbl_overlay_collapse: ham_collapse.as_str(),
        lbl_stats_edge: ham_stats_edge.as_str(),
        overlay_edges: &overlay_edges,
        toolbar_collapsed: overlay_layout.toolbar_collapsed,
        bookmark_store: &bookmark_store_refs,
        bookmark_jump: &bookmark_jump_refs,
        layout_entries: &layout_refs,
    };
    let bar_state = seal_ui::ViewportBarState {
        space: models.viewport_space.as_str(),
        surface_snap: models.viewport_surface_snap,
        grid_snap: models.viewport_grid_snap,
        grid_step: models.viewport_grid_snap_step,
        angle_snap: models.viewport_angle_snap,
        angle_deg: models.viewport_angle_snap_deg,
        scale_snap: models.viewport_scale_snap,
        scale_step: models.viewport_scale_snap_step,
        camera_speed: models.viewport_camera_speed,
        speed_popup: models.viewport_camera_speed_menu,
    };
    let tip_space = text("tooltip-viewport-space");
    let tip_surface = text("tooltip-viewport-surface-snap");
    let tip_grid = text("tooltip-viewport-grid-snap");
    let tip_angle = text("tooltip-viewport-angle-snap");
    let tip_scale = text("tooltip-viewport-scale-snap");
    let tip_speed = text("tooltip-viewport-camera-speed");
    let tip_maximize = text("tooltip-viewport-maximize");
    let bar_tips = seal_ui::ViewportBarTooltips {
        space: tip_space.as_str(),
        surface_snap: tip_surface.as_str(),
        grid_snap: tip_grid.as_str(),
        angle_snap: tip_angle.as_str(),
        scale_snap: tip_scale.as_str(),
        camera_speed: tip_speed.as_str(),
        maximize: tip_maximize.as_str(),
    };
    // Play in the Perspective view looks through the scene's play camera, like the game.
    let look =
        (models.viewport_view == "perspective").then(|| session.world.play_look_camera()).flatten();
    session.world.set_look_camera(look.is_some());
    let game_view = session.game_view || look.is_some();
    let tip_expand = text("viewport-menu-overlay-collapse-toolbar");
    let bar_hit = if !game_view && overlay_layout.toolbar_collapsed {
        frame.viewport_collapsed_toolbar(overlay_layout.toolbar_edge, tip_expand.as_str())
    } else if models.viewport_show_toolbar && !game_view {
        frame.viewport_toolbar(
            models.viewport_view.as_str(),
            &view_refs,
            models.viewport_shade.as_str(),
            &shade_refs,
            models.viewport_tool.as_str(),
            &tool_refs,
            &hamburger,
            show_menu,
            bar_state,
            &bar_tips,
        )
    } else {
        seal_ui::ViewportToolbarHit::default()
    };
    if bar_hit.toolbar_expand {
        return command("viewport.overlay", &[("target", "toolbar"), ("action", "collapse")]);
    }
    if let Some(pick) = bar_hit.hamburger_pick {
        return match pick.as_str() {
            "realtime" => command("viewport.realtime", &[]),
            "show_stats" => command("viewport.stats", &[]),
            "show_fps" => command("viewport.show_fps", &[]),
            "show_toolbar" => command("viewport.show_toolbar", &[]),
            "isolate" => command("viewport.isolate", &[]),
            "overlay_collapse" => {
                command("viewport.overlay", &[("target", "toolbar"), ("action", "collapse")])
            }
            pick if pick.starts_with("overlay_stats_") => {
                let edge = &pick["overlay_stats_".len()..];
                command(
                    "viewport.overlay",
                    &[("target", "stats"), ("action", "edge"), ("edge", edge)],
                )
            }
            "cinematic" => command("viewport.cinematic", &[]),
            "immersive" => command("app.toggle_fullscreen", &[]),
            "create_camera" => command("viewport.create_camera", &[]),
            "screenshot" => command("viewport.screenshot", &[]),
            bookmark if bookmark.starts_with("bookmark_store:") => {
                let slot = &bookmark["bookmark_store:".len()..];
                command("viewport.bookmark_store", &[("slot", slot)])
            }
            bookmark if bookmark.starts_with("bookmark_jump:") => {
                let slot = &bookmark["bookmark_jump:".len()..];
                command("viewport.bookmark_recall", &[("slot", slot)])
            }
            "layout_single" | "layout_two_cols" | "layout_two_rows" | "layout_three"
            | "layout_four" => command("viewport.layout", &[("preset", pick.as_str())]),
            _ => PanelEvent::None,
        };
    }
    if let Some(fov) = bar_hit.hamburger_fov {
        return command("viewport.fov", &[("fov", &fov.to_string())]);
    }
    if let Some(value) = bar_hit.hamburger_far {
        return command("viewport.far_plane", &[("value", &value.to_string())]);
    }
    if let Some(value) = bar_hit.hamburger_screen_pct {
        return command("viewport.screen_pct", &[("value", &value.to_string())]);
    }
    if bar_hit.hamburger_toggle {
        return command("viewport.hamburger_menu", &[("open", "0")]);
    }
    if let Some(view) = bar_hit.view {
        return command("viewport.view", &[("view", view.as_str())]);
    }
    if let Some(mode) = bar_hit.mode {
        return command("viewport.shade", &[("mode", mode.as_str())]);
    }
    if bar_hit.show_defaults {
        return command("viewport.show_defaults", &[]);
    }
    if let Some((layer, on)) = bar_hit.show_layer {
        return command(
            "viewport.show",
            &[("layer", layer.as_str()), ("on", if on { "1" } else { "0" })],
        );
    }
    if bar_hit.show_menu_toggle {
        return command("viewport.show_menu", &[("open", "0")]);
    }
    if bar_hit.maximize {
        if models.viewport_layout_preset != "single" {
            return command("viewport.pane_expand", &[]);
        }
        return command("view.maximize_panel", &[]);
    }
    if bar_hit.space_toggle {
        return command("viewport.space", &[]);
    }
    if bar_hit.surface_toggle {
        return command("viewport.surface_snap", &[]);
    }
    if bar_hit.grid_toggle {
        return command("viewport.grid_snap", &[]);
    }
    if let Some(step) = bar_hit.grid_step {
        return command("viewport.grid_snap_step", &[("step", &step.to_string())]);
    }
    if bar_hit.angle_toggle {
        return command("viewport.angle_snap", &[]);
    }
    if let Some(step) = bar_hit.angle_step {
        return command("viewport.angle_snap_step", &[("step", &step.to_string())]);
    }
    if bar_hit.scale_toggle {
        return command("viewport.scale_snap", &[]);
    }
    if let Some(step) = bar_hit.scale_step {
        return command("viewport.scale_snap_step", &[("step", &step.to_string())]);
    }
    if let Some(speed) = bar_hit.speed {
        return command("viewport.camera_speed", &[("speed", &speed.to_string())]);
    }
    if bar_hit.speed_toggle {
        return command("viewport.camera_speed_menu", &[("open", "1")]);
    }
    if let Some(tool) = bar_hit.tool {
        return command("viewport.tool", &[("tool", tool.as_str())]);
    }
    if models.viewport_layout_preset != "single" {
        return viewport_multi::paint(frame, session);
    }
    let layers = &models.viewport_show_layers;
    let layer_on = |key: &str| layers.get(key).copied().unwrap_or(true);
    let shown: Vec<&seal_editor_core::Entity> = session
        .world
        .entities
        .iter()
        .filter(|entity| {
            // Folders only group rows in the outliner; they have nothing to draw.
            if entity.id == "root" || entity.hidden || entity.tag == "folder" {
                return false;
            }
            // A game shows what players see: shapes and sprites, never editor icons.
            let merged = seal_editor_core::object_merge::hides_body(entity);
            if game_view {
                return entity.shape.is_some() || !entity.sprite.is_empty() || merged;
            }
            if entity.tag == "light" && !layer_on("lights") {
                return false;
            }
            if entity.shape.is_none() && !merged && entity.tag != "light" && !layer_on("icons") {
                return false;
            }
            if entity.shape.is_some() && !layer_on("statics") {
                return false;
            }
            true
        })
        .collect();
    let draw_grid = models.viewport_grid && layer_on("grid") && !game_view;
    let count = shown.len().to_string();
    let fps = if session.frame_ms > 0.0 {
        format!("{:.0}", 1000.0 / session.frame_ms)
    } else {
        "0".into()
    };
    let frame_ms = format!("{:.1}", session.frame_ms);
    let draws = (shown.len() * 3 + 48).to_string();
    let tris = (shown.len() * 12).to_string();
    let mem = format!("{}", 96 + shown.len() * 2);
    let show_stats = models.viewport_show_stats && overlay_layout.stats_visible;
    let show_fps = models.viewport_show_fps && overlay_layout.fps_visible;
    let stats_edge = if show_stats {
        overlay_layout.stats_edge
    } else if show_fps {
        overlay_layout.fps_edge
    } else {
        overlay_layout.stats_edge
    };
    let overlay = if show_stats {
        Some(session.messages.format(
            "viewport-overlay-stats",
            &[
                ("fps", fps.as_str()),
                ("frame_ms", frame_ms.as_str()),
                ("draws", draws.as_str()),
                ("tris", tris.as_str()),
                ("entities", count.as_str()),
                ("mem", mem.as_str()),
            ],
        ))
    } else if show_fps {
        Some(session.messages.format(
            "viewport-overlay-fps",
            &[("fps", fps.as_str()), ("frame_ms", frame_ms.as_str())],
        ))
    } else {
        None
    };
    let play_badge_base = match session.world.mode {
        seal_editor_core::PlayMode::Playing => Some((session.messages.get("play-running"), true)),
        seal_editor_core::PlayMode::Paused => Some((session.messages.get("play-paused"), false)),
        seal_editor_core::PlayMode::Editing => None,
    };
    // The badge already says Playing or Paused; a script state of the same word is not repeated.
    let game_line = session.world.play_game().and_then(|g| {
        let mut parts = Vec::new();
        if !g.state.is_empty() && !matches!(g.state.as_str(), "playing" | "paused") {
            parts.push(g.state.clone());
        }
        parts.extend(g.scores.iter().map(|(k, v)| format!("{k} {v}")));
        (!parts.is_empty()).then(|| parts.join(" · "))
    });
    let play_badge_base = play_badge_base.map(|(label, running)| match &game_line {
        Some(line) => (format!("{label}  {line}"), running),
        None => (label, running),
    });
    let play_badge = play_badge_base.map(|(label, running)| {
        // Scenes that count coins with scripts have no side-view coin total; show no 0/0.
        let Some(status) = session.world.play_status().filter(|s| s.coins_total > 0) else {
            return (label, running);
        };
        let got = status.coins.to_string();
        let total = status.coins_total.to_string();
        let coins = session
            .messages
            .format("play-coins", &[("got", got.as_str()), ("total", total.as_str())]);
        let coins = if status.goal_locked {
            format!("{coins}  {}", session.messages.get("play-goal-locked"))
        } else {
            coins
        };
        if game_view && !status.won {
            return (coins, running);
        }
        if status.won {
            (format!("{}  {coins}", session.messages.get("play-clear")), running)
        } else {
            (format!("{label}  {coins}"), running)
        }
    });
    let game_ui = session.world.play_ui_lines();
    let mut game_screens = session.world.play_screen_draws();
    if let Some(project) = session.project.as_ref() {
        seal_editor_core::gameui::resolve_paths(&mut game_screens, &project.root);
    }

    let scene_name = session.scene_file_name();
    let caption =
        session.messages.format("viewport-scene-caption", &[("name", scene_name.as_str())]);
    let lighting_msg = session.messages.get("viewport-warning-lighting");
    let lighting_hint = session.messages.get("viewport-warning-lighting-hint");
    let nav_msg = session.messages.get("viewport-warning-navigation");
    let nav_hint = session.messages.get("viewport-warning-navigation-hint");
    let mut on_screen: Vec<seal_ui::ViewportOnScreenWarning<'_>> = Vec::new();
    // Seal lights in real time, so editing never needs a lighting rebuild; only a pending
    // navigation rebuild raises this warning.
    if models.viewport_nav_rebuild_pending && !models.viewport_hidden_warnings.contains("lighting")
    {
        on_screen.push(seal_ui::ViewportOnScreenWarning {
            title: lighting_msg.as_str(),
            hint: lighting_hint.as_str(),
            error: false,
        });
    }
    if models.viewport_nav_rebuild_pending
        && !models.viewport_hidden_warnings.contains("navigation")
    {
        on_screen.push(seal_ui::ViewportOnScreenWarning {
            title: nav_msg.as_str(),
            hint: nav_hint.as_str(),
            error: true,
        });
    }
    if game_view {
        on_screen.clear();
    }
    let (yaw, pitch, distance, _, _) = viewport_camera_state(session);
    let perspective = models.viewport_view == "perspective";
    let orbit_pivot = if perspective {
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        for entity in &session.world.entities {
            if entity.id == "root" || entity.hidden {
                continue;
            }
            if !session.foundation.selection.contains(&entity.id)
                && models.selection.as_deref() != Some(entity.id.as_str())
            {
                continue;
            }
            xs.push(entity.translation[across] * flip_across);
            ys.push(entity.translation[up] * flip_up);
        }
        if xs.is_empty() {
            None
        } else {
            let cx = xs.iter().sum::<f32>() / xs.len() as f32;
            let cy = ys.iter().sum::<f32>() / ys.len() as f32;
            Some((cx, cy))
        }
    } else {
        None
    };
    let mut camera = ViewportCamera {
        pan_x: models.viewport_pan_x,
        pan_y: models.viewport_pan_y,
        zoom: models.viewport_zoom,
        yaw_deg: yaw,
        pitch_deg: pitch,
        distance,
    };
    // A 2D camera object steers the play view when one is on: follow with
    // smoothing, zoom, keep inside its limits, shake, snap to sprite pixels.
    let shake = session.world.play_shake();
    if let Some(view) = session.world.play_camera2d().filter(|_| !perspective) {
        camera.zoom = (models.viewport_zoom * view.zoom).clamp(0.05, 50.0);
        if view.pixel_snap > 0.0 {
            camera.zoom =
                seal_editor_core::camera2d::snap_scale(20.0 * camera.zoom, view.pixel_snap) / 20.0;
        }
        let cell = 20.0 * camera.zoom;
        let size = frame.view_size();
        let half = [size[0] * 0.5 / cell, size[1] * 0.5 / cell];
        let mut c = seal_editor_core::camera2d::clamp_center(view.center, half, view.limits);
        c = [c[0] + shake[0], c[1] + shake[1]];
        c = seal_editor_core::camera2d::snap_center(c, view.pixel_snap);
        let (pan_x, pan_y) = follow_pan(&camera, perspective, c[0] * flip_across, c[1] * flip_up);
        camera.pan_x = pan_x;
        camera.pan_y = pan_y;
    } else if let Some(player) = session.world.play_player().and_then(|id| session.world.entity(id))
    {
        let x = player.translation[across] * flip_across;
        // The view tracks runs sideways but holds still for an ordinary jump, so the
        // jump reads on screen. It only climbs or drops once the seal leaves that band.
        let start_y = session.world.play_start().map_or(player.translation[up], |s| s[1]);
        let rise = player.translation[up] - start_y;
        let shift = if rise > 3.0 {
            rise - 3.0
        } else if rise < -1.5 {
            rise + 1.5
        } else {
            0.0
        };
        // A little above the seal, so the ground under it stays in view at any zoom.
        let y = (start_y + shift) * flip_up + 0.75;
        let (pan_x, pan_y) = follow_pan(&camera, perspective, x + shake[0], y + shake[1]);
        camera.pan_x = pan_x;
        camera.pan_y = pan_y;
    } else if let Some(target) = session.world.play_view_target() {
        // Scripted play: the view stays centred on the scene's camera (or player).
        let (pan_x, pan_y) =
            follow_pan(&camera, perspective, target[across] * flip_across, target[up] * flip_up);
        camera.pan_x = pan_x;
        camera.pan_y = pan_y;
    }
    let look_through = look.map(|(eye, rotation, fov)| {
        // Rotation uses the view's angles: yaw 0 looks along +z like Front, pitch up is positive.
        camera.yaw_deg = rotation[1];
        camera.pitch_deg = rotation[0];
        camera.pan_x = 0.0;
        camera.pan_y = 0.0;
        let half_h = frame.view_size()[1] * 0.5;
        seal_ui::LookThrough {
            eye,
            focal_px: half_h / (fov.clamp(10.0, 170.0) * 0.5).to_radians().tan(),
        }
    });
    let (base_pan_x, base_pan_y) = (camera.pan_x, camera.pan_y);
    // The 2D camera scales the saved editor zoom; only user zooming is saved back.
    let (base_zoom, zoom_scale) = (camera.zoom, camera.zoom / models.viewport_zoom.max(0.0001));
    // Flat side view: the scene point in the middle of the view, for parallax layers
    // (which scroll only while the world runs) and repeated backgrounds.
    let flat_side = !perspective && camera.yaw_deg == 0.0 && across == 0 && up == 1;
    let tile_plane = across == 0 && up == 1 && flip_across > 0.0 && flip_up > 0.0;
    let particle_lights = if tile_plane && !perspective {
        scene_lights_2d(&session.world.entities)
    } else {
        (Vec::new(), Vec::new())
    };
    let cell = 20.0 * camera.zoom.max(0.01);
    let view_center = [-camera.pan_x / cell, camera.pan_y / cell];
    let running =
        session.world.play_player().is_some() || session.world.play_view_target().is_some();
    let marks: Vec<GridMark> = shown
        .iter()
        .map(|entity| {
            // Sizes match the colliders play builds, so what rests on the ground looks it.
            let scale = entity.scale;
            let (size, round, mesh) = match entity.shape {
                Some(seal_editor_core::Shape::Cube) => (
                    [0.5 * scale[across], 0.5 * scale[up], 0.5 * scale[depth_axis]],
                    false,
                    Some(seal_ui::MeshPrimitive::Cube),
                ),
                Some(seal_editor_core::Shape::Sphere) => {
                    let r = 0.5 * scale[across].max(scale[up]).max(scale[depth_axis]);
                    ([r, r, r], true, Some(seal_ui::MeshPrimitive::Sphere))
                }
                Some(seal_editor_core::Shape::Cylinder) => (
                    [
                        0.5 * scale[across].max(scale[depth_axis]),
                        0.5 * scale[up],
                        0.5 * scale[across].max(scale[depth_axis]),
                    ],
                    false,
                    Some(seal_ui::MeshPrimitive::Cylinder),
                ),
                Some(seal_editor_core::Shape::Cone) => (
                    [
                        0.5 * scale[across].max(scale[depth_axis]),
                        0.5 * scale[up],
                        0.5 * scale[across].max(scale[depth_axis]),
                    ],
                    false,
                    Some(seal_ui::MeshPrimitive::Cone),
                ),
                Some(seal_editor_core::Shape::Capsule) => (
                    [
                        0.35 * scale[across].max(scale[depth_axis]),
                        0.5 * scale[up],
                        0.35 * scale[across].max(scale[depth_axis]),
                    ],
                    false,
                    Some(seal_ui::MeshPrimitive::Capsule),
                ),
                Some(seal_editor_core::Shape::Plane) => (
                    [20.0 * scale[across], 0.05 * scale[up], 20.0 * scale[depth_axis]],
                    false,
                    Some(seal_ui::MeshPrimitive::Plane),
                ),
                None => ([0.3, 0.3, 0.3], false, None),
            };
            let (sprite, uv) = crate::sprite_draw(session, entity);
            let pivot = sprite.as_deref().map(picture_pivot).unwrap_or([0.5, 0.5]);
            let card = sprite.as_deref().map(|path| sprite_card(path, scale[across], scale[up]));
            // A gameplay object with no picture or shape still needs a body to see:
            // a blue block for the player, a gold ball for a coin, a green post for the goal.
            let stand_in = if entity.shape.is_none() && sprite.is_none() {
                match entity.tag.as_str() {
                    "player" => {
                        Some(([0.4, 0.45], false, seal_ui::MeshPrimitive::Cube, [70, 140, 235]))
                    }
                    "coin" => {
                        Some(([0.3, 0.3], true, seal_ui::MeshPrimitive::Sphere, [240, 196, 60]))
                    }
                    "goal" => {
                        Some(([0.15, 0.5], false, seal_ui::MeshPrimitive::Cube, [80, 200, 110]))
                    }
                    _ => None,
                }
            } else {
                None
            };
            let (size, round, mesh) = if let Some(card) = card {
                let (fw, fh) = if entity.slice_size[0] > 0.0 && entity.slice_size[1] > 0.0 {
                    (entity.slice_size[0], entity.slice_size[1])
                } else {
                    (card.full[0], card.full[1])
                };
                ([fw * 0.5, fh * 0.5, 0.5 * scale[depth_axis]], false, None)
            } else if let Some((half, round, mesh, _)) = stand_in {
                let mut size = [0.0; 3];
                size[across] = half[0] * scale[across].abs();
                size[up] = half[1] * scale[up].abs();
                size[depth_axis] = half[0] * scale[depth_axis].abs();
                (size, round, Some(mesh))
            } else {
                (size, round, mesh)
            };
            let mut half = [size[across], size[up]];
            if sprite.is_none() && !entity.sprite_mask.shape.is_empty() {
                half = [0.5 * scale[across].abs(), 0.5 * scale[up].abs()];
            }
            // A bare line or shape is picked by the area its points cover.
            if entity.shape.is_none() && sprite.is_none() && stand_in.is_none() {
                if let Some(mark) =
                    shape2d_mark(session, entity, [across, up], [flip_across, flip_up])
                {
                    let (cx, cy) = (
                        entity.translation[across] * flip_across,
                        entity.translation[up] * flip_up,
                    );
                    for p in &mark.points {
                        half = [half[0].max((p[0] - cx).abs()), half[1].max((p[1] - cy).abs())];
                    }
                }
            }
            let spin = match models.viewport_view.as_str() {
                "top" | "bottom" => entity.rotation[1],
                "left" | "right" => entity.rotation[0],
                _ => entity.rotation[2],
            };
            let billboard = if entity.shape.is_none() && sprite.is_none() && stand_in.is_none() {
                match entity.tag.as_str() {
                    "camera" => Some(seal_ui::Icon::EntityCamera),
                    "light" => Some(seal_ui::Icon::EntityLightDirectional),
                    "player" => Some(seal_ui::Icon::EntityPlayerStart),
                    _ => None,
                }
            } else {
                None
            };
            let shift =
                parallax_shift(entity, flat_side && running && sprite.is_some(), view_center);
            let selected = session.foundation.selection.contains(&entity.id)
                || models.selection.as_deref() == Some(entity.id.as_str());
            let flat_xy = !perspective && across == 0 && up == 1;
            let sprite_shape = sprite_shape::view_mark(
                session,
                entity,
                [across, up],
                [flip_across, flip_up],
                selected && flat_xy && editing && !entity.locked,
            );
            if let Some(shape) = &sprite_shape {
                let (cx, cy) =
                    (entity.translation[across] * flip_across, entity.translation[up] * flip_up);
                for p in shape.outline.iter() {
                    half = [half[0].max((p[0] - cx).abs()), half[1].max((p[1] - cy).abs())];
                }
            }
            let mesh = if sprite_shape.is_some() { None } else { mesh };
            let (size, half, mesh, sprite) = if seal_editor_core::object_merge::hides_body(entity) {
                ([0.02, 0.02, 0.02], [0.02, 0.02], None, None)
            } else {
                (size, half, mesh, sprite)
            };
            let billboard = if sprite_shape.is_some() { None } else { billboard };
            let (layer, order) = if entity.sprite_shape.on {
                (entity.sprite_shape.layer, entity.sprite_shape.order)
            } else {
                (entity.layer, entity.order)
            };
            GridMark {
                x: entity.translation[across] * flip_across + shift[0],
                y: entity.translation[up] * flip_up + shift[1],
                half,
                round,
                billboard,
                selected,
                faint: size[across] > 1.0 && size[up] > 1.0,
                label: &entity.name,
                tint: stand_in.map(|(_, _, _, tint)| tint).unwrap_or_else(|| {
                    entity.color.map(|channel| (channel.clamp(0.0, 1.0) * 255.0) as u8)
                }),
                angle: spin.to_radians(),
                depth: entity.translation[depth_axis],
                size,
                mesh,
                pickable: !entity.locked && !entity.unselectable,
                sprite,
                layer,
                uv,
                order,
                sprite_tint: entity.tint.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8),
                slice: sprite_place(session, entity).1,
                slice_mode: entity.slice_mode.as_str(),
                slice_size: entity.slice_size,
                // The object's own pivot wins, then the animation frame's, then the picture's import pivot.
                pivot: {
                    let frame_pivot = sprite_place(session, entity).0;
                    if entity.pivot != [0.5, 0.5] {
                        entity.pivot
                    } else if frame_pivot != [0.5, 0.5] {
                        frame_pivot
                    } else {
                        pivot
                    }
                },
                normal_map: normal_map_path(session, entity),
                shape2d: shape2d_mark(session, entity, [across, up], [flip_across, flip_up])
                    .or_else(|| {
                        light2d_view::mark(
                            entity,
                            [across, up],
                            [flip_across, flip_up],
                            selected && flat_xy && editing && !entity.locked,
                        )
                    }),
                sprite_shape,
                material: material_mark(session, entity),
                mask_mode: match entity.mask.as_str() {
                    "inside" => 1,
                    "outside" => 2,
                    _ => 0,
                },
                mask_kind: match entity.sprite_mask.shape.as_str() {
                    "rect" => 1,
                    "circle" => 2,
                    "alpha" => 3,
                    _ => 0,
                },
                surface: mesh_surface(session, entity),
            }
        })
        .collect();
    // Live particles from play, drawn as small round dots that cannot be picked.
    let mut marks = marks;
    let post = if flat_side {
        seal_editor_core::post_volume::resolve(
            &session.world.entities,
            view_center,
            &session.world.glow,
        )
    } else {
        seal_editor_core::post_volume::Resolved::off(&session.world.glow)
    };
    if post.active {
        for mark in &mut marks {
            mark.tint = grade_channels(mark.tint, &post);
            let rgb = grade_channels(
                [mark.sprite_tint[0], mark.sprite_tint[1], mark.sprite_tint[2]],
                &post,
            );
            mark.sprite_tint = [rgb[0], rgb[1], rgb[2], mark.sprite_tint[3]];
        }
    }
    marks.extend(merged_view::extra_marks(&shown, across, up, depth_axis, flip_across, flip_up));
    if flat_side {
        let reach = 70.0 / camera.zoom.max(0.25);
        let copies: Vec<GridMark> = shown
            .iter()
            .zip(marks.iter())
            .filter(|(e, m)| e.repeat_x && m.sprite.is_some())
            .flat_map(|(_, m)| repeat_copies(m, view_center[0], reach))
            .collect();
        marks.extend(copies);
    }
    let occluders: Vec<f32> = shown
        .iter()
        .filter(|e| !e.hidden && (e.shape.is_some() || !e.sprite.trim().is_empty()))
        .map(|e| e.translation[2])
        .collect();
    for (id, emitter) in &session.world.emitters {
        let root = session.project.as_ref().map(|p| p.root.as_path());
        let material = session.world.entity(id).and_then(|e| material_mark(session, e));
        marks.extend(particle_viewport::particle_marks(
            emitter,
            across,
            up,
            depth_axis,
            flip_across,
            flip_up,
            particle_viewport::ParticleLight2d {
                lights: &particle_lights.0,
                blockers: &particle_lights.1,
                ambient: AMBIENT_2D,
                project: root,
                resolve_sprite: true,
                occluders: &occluders,
            },
            material,
        ));
    }
    if flat_side && !game_view {
        for entity in &shown {
            let selected = session.foundation.selection.contains(&entity.id)
                || models.selection.as_deref() == Some(entity.id.as_str());
            if !selected {
                continue;
            }
            let half = cam_drag_view::view_half(
                frame.view_size(),
                models.viewport_zoom,
                entity.camera2d.zoom,
            );
            if let Some(mark) =
                cam_drag_view::mark(entity, half, [across, up], [flip_across, flip_up], depth_axis)
            {
                marks.push(mark);
            }
            for gizmo in seal_editor_core::area_effect::gizmos(entity) {
                let points = gizmo
                    .points
                    .into_iter()
                    .map(|p| [p[0] * flip_across, p[1] * flip_up])
                    .collect();
                marks.push(GridMark {
                    x: entity.translation[across] * flip_across,
                    y: entity.translation[up] * flip_up,
                    half: [0.2, 0.2],
                    round: false,
                    billboard: None,
                    selected: false,
                    faint: false,
                    label: "",
                    tint: [80, 170, 255],
                    angle: 0.0,
                    depth: entity.translation[depth_axis] - 0.02,
                    size: [0.2, 0.2, 0.2],
                    mesh: None,
                    pickable: false,
                    sprite: None,
                    layer: i32::MAX,
                    uv: [0.0, 0.0, 1.0, 1.0],
                    order: 1,
                    sprite_tint: [255; 4],
                    slice: [0.0; 4],
                    slice_mode: "stretch",
                    slice_size: [0.0; 2],
                    normal_map: None,
                    sprite_shape: None,
                    mask_mode: 0,
                    mask_kind: 0,
                    surface: seal_ui::MeshSurface::EMPTY,
                    pivot: [0.5, 0.5],
                    shape2d: Some(seal_ui::ShapeMark {
                        points,
                        width: 0.06,
                        color: [80, 170, 255, 230],
                        filled: false,
                        closed: gizmo.closed,
                        edit_points: false,
                        uvs: Vec::new(),
                        texture: String::new(),
                        ribbon: Vec::new(),
                        ribbon_tris: Vec::new(),
                    }),
                    material: None,
                });
            }
        }
    }
    let light_dir = session
        .world
        .entities
        .iter()
        .find(|entity| entity.tag == "light")
        .map(|light| {
            let v = light.translation;
            let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1.0);
            [v[0] / len, v[1] / len, v[2] / len]
        })
        .unwrap_or([0.2, 0.85, 0.3]);
    let fill_lights = if models.viewport_show_layers.get("probes").copied().unwrap_or(true) {
        viewport_fill_lights(&session.world.entities)
    } else {
        Vec::new()
    };
    let entity_entries = viewport_menu::entity_entries(session);
    let empty_entries = viewport_menu::empty_entries(session);
    let gizmo_select =
        session.foundation.selection.last().map(String::as_str).or(models.selection.as_deref());
    let gizmo_pivot = gizmo_select.and_then(|sel| shown.iter().position(|entity| entity.id == sel));
    let gizmo_world = gizmo_pivot.map(|ix| shown[ix].translation);
    let gizmo_start_rot = gizmo_pivot.map(|ix| shown[ix].rotation);
    let gizmo_start_scale = gizmo_pivot.map(|ix| shown[ix].scale);
    // The Create panel's sprite brush: a project picture painted with left-drag in a flat view.
    // Erase mode needs no picture: it removes whatever tile sits on the layer.
    let erasing = session.draft(TILE_ERASE_DRAFT).as_deref() == Some("true");
    let tile_tool =
        session.draft(seal_editor_core::tiles::TILE_TOOL_DRAFT).unwrap_or_else(|| "brush".into());
    let tool_needs_no_brush = matches!(tile_tool.as_str(), "picker" | "fill" | "eraser");
    let brush = session
        .draft(PLACE_BRUSH_DRAFT)
        .filter(|rel| !rel.is_empty())
        .or_else(|| erasing.then(|| "erase".to_string()))
        .or_else(|| tool_needs_no_brush.then(|| tile_tool.clone()))
        .filter(|_| !perspective && !game_view)
        .filter(|_| matches!(session.world.mode, seal_editor_core::PlayMode::Editing));
    let prefab_banner = session.prefab_edit.as_ref().map(|edit| {
        format!("Editing prefab {}: changes save to the file. Inspector > Done Editing Prefab goes back", edit.name)
    });
    let tile_grid = session.draft(TILE_GRID_DRAFT).unwrap_or_else(|| "square".into());
    let tile_erase = session.draft(TILE_ERASE_DRAFT).as_deref() == Some("true");
    let tile_layer: i32 =
        session.draft(TILE_LAYER_DRAFT).unwrap_or_else(|| "0".into()).parse().unwrap_or(0);
    let mut paint_tool = tile_tool.clone();
    if session.draft("tile-pattern-save-mode").as_deref() == Some("true") {
        paint_tool = "pattern_save".into();
    }
    let paint_grid = seal_editor_core::tiles::Grid::parse(tile_grid.as_str());
    // Tiles go on the flat Front view's x/y plane; other flat views keep the
    // simple square stamping along their own axes.
    // 2D lights only shade the flat front view the 2D scene is built in.
    let (lights2d, blockers2d) = particle_lights;
    let masks2d = if !perspective {
        let root = session.project.as_ref().map(|p| p.root.as_path());
        seal_ui::sprite_mask::view_masks(
            &session.world.entities,
            [across, up],
            [flip_across, flip_up],
            |entity| match models.viewport_view.as_str() {
                "top" | "bottom" => entity.rotation[1],
                "left" | "right" => entity.rotation[0],
                _ => entity.rotation[2],
            },
            root,
        )
    } else {
        Vec::new()
    };
    let brush_kind = brush.as_ref().and_then(|rel| {
        session.project.as_ref().map(|p| format!("sprite:{}", p.root.join(rel).to_string_lossy()))
    });
    let mut mark_entity_ids: Vec<&str> = shown.iter().map(|e| e.id.as_str()).collect();
    mark_entity_ids.resize(marks.len(), "");
    let sprite_order_storage =
        sprite_viewport::sort_sprite_mark_indices(session, &marks, &mark_entity_ids);
    seal_ui::set_look_through(look_through);
    let hit = frame.grid_view(
        &marks,
        draw_grid,
        names,
        &models.viewport_tool,
        GridMenus {
            entity: ViewportContextMenu { caption: None, entries: &entity_entries },
            empty: ViewportContextMenu { caption: None, entries: &empty_entries },
        },
        &mut camera,
        seal_ui::GridViewOptions {
            stats_overlay: overlay.as_deref().filter(|_| !game_view),
            stats_overlay_edge: stats_edge,
            scene_caption: (!game_view).then_some(caption.as_str()),
            game_view,
            play_badge: play_badge.as_ref().map(|(label, running)| (label.as_str(), *running)),
            game_ui: &game_ui,
            game_screens: &game_screens,
            warnings: &on_screen,
            show_names: models.viewport_show_names,
            wireframe,
            shade: models.viewport_shade.as_str(),
            perspective,
            camera_speed: models.viewport_camera_speed,
            orbit_pivot,
            light_dir,
            shadows: models.viewport_show_layers.get("shadows").copied().unwrap_or(true),
            shadow_map_size: seal_editor_core::editor_shadow_map_size(session),
            fill_lights: &fill_lights,
            lights2d: &lights2d,
            blockers2d: &blockers2d,
            ambient2d: AMBIENT_2D,
            masks2d: &masks2d,
            gi: models.viewport_show_layers.get("gi").copied().unwrap_or(true),
            axes_on_top: models.viewport_show_layers.get("axes").copied().unwrap_or(true),
            gizmo_pivot,
            gizmo_world,
            gizmo_start_rot,
            gizmo_start_scale,
            gizmo_space: models.viewport_space.as_str(),
            vertex_snap: models.viewport_vertex_snap,
            paint_brush: brush_kind
                .as_deref()
                .or_else(|| brush.as_ref().filter(|_| tile_plane).map(|_| "sprite:tile-tool")),
            paint_grid: tile_grid.as_str(),
            paint_erase: tile_erase,
            paint_tool: paint_tool.as_str(),
            paint_layer: tile_layer,
            prefab_banner: prefab_banner.as_deref(),
            measure_mode: matches!(models.viewport_tool.as_str(), "measure" | "collider")
                && editing
                && !game_view,
            sprite_order: Some(sprite_order_storage.as_slice()),
            glow: {
                let front = !perspective && across == 0 && up == 1;
                let on = front
                    && session.models.quality_glow
                    && (session.world.glow.enabled || (post.active && post.intensity > 0.0));
                on.then_some([
                    if post.active { post.threshold } else { session.world.glow.threshold },
                    if post.active { post.intensity } else { session.world.glow.intensity },
                    session.world.glow.radius,
                ])
            },
            anisotropy: seal_editor_core::render_quality_from_session(session).anisotropy,
            render_scale: seal_editor_core::render_quality_from_session(session).render_scale,
            light_gain: seal_ui::scene_light_gain(
                session
                    .world
                    .entities
                    .iter()
                    .find(|entity| entity.tag == "light")
                    .map(|entity| entity.intensity)
                    .unwrap_or(1.0),
                session.world.glow.enabled,
                session.world.glow.intensity,
            ),
            sky_cubemap: session.world.sky_cubemap.as_str(),
            texture_root: session.project.as_ref().map(|project| project.root.as_path()),
        },
    );
    seal_ui::set_look_through(None);
    let flat_2d = !perspective && models.viewport_view == "front";
    let pixels_per_unit = session
        .world
        .entities
        .iter()
        .map(|entity| entity.camera2d.pixel_snap)
        .find(|ppu| *ppu > 0.0)
        .unwrap_or(0.0);
    if models.viewport_tool == "measure" && editing && !game_view {
        let escape = matches!(hit.key.as_deref(), Some("Escape") | Some("KeyEscape"));
        let axes =
            measure_tool::MeasureViewAxes { across, up, _spin: depth_axis, flip_across, flip_up };
        let view_name = models.viewport_view.as_str();
        let measure_event = measure_tool::handle_viewport(
            session,
            hit.measure.as_ref(),
            escape,
            flat_2d,
            perspective,
            pixels_per_unit,
            &camera,
            view_name,
        );
        measure_tool::paint_overlay(
            frame,
            session,
            &camera,
            perspective,
            axes,
            hit.measure.as_ref(),
            view_name,
        );
        if !matches!(measure_event, PanelEvent::None) {
            return measure_event;
        }
        if let Some(key) = hit.key {
            let tool = match key.as_str() {
                "Q" | "KeyQ" => Some("select"),
                "W" | "KeyW" => Some("move"),
                "E" | "KeyE" => Some("rotate"),
                "R" | "KeyR" => Some("scale"),
                "M" | "KeyM" => Some("measure"),
                "C" | "KeyC" => Some("collider"),
                _ => None,
            };
            if let Some(tool) = tool {
                return command("viewport.tool", &[("tool", tool)]);
            }
        }
        if hit.camera_dirty
            || (camera.pan_x - models.viewport_pan_x).abs() > 0.01
            || (camera.pan_y - models.viewport_pan_y).abs() > 0.01
            || (camera.zoom - base_zoom).abs() > f32::EPSILON
            || (camera.yaw_deg - yaw).abs() > f32::EPSILON
            || (camera.pitch_deg - pitch).abs() > f32::EPSILON
            || (camera.distance - distance).abs() > f32::EPSILON
        {
            return command(
                "viewport.camera",
                &[
                    ("pan_x", &camera.pan_x.to_string()),
                    ("pan_y", &camera.pan_y.to_string()),
                    ("zoom", &(camera.zoom / zoom_scale).to_string()),
                    ("yaw", &camera.yaw_deg.to_string()),
                    ("pitch", &camera.pitch_deg.to_string()),
                    ("distance", &camera.distance.to_string()),
                ],
            );
        }
        return PanelEvent::None;
    }
    if models.viewport_tool == "collider" && editing && !game_view {
        let flip = [flip_across, flip_up];
        let view_name = models.viewport_view.as_str();
        if let Some(key) = hit.key.as_deref() {
            if let Some(event) = collider_edit::delete_key(session, key) {
                return event;
            }
            let tool = match key {
                "Q" | "KeyQ" => Some("select"),
                "W" | "KeyW" => Some("move"),
                "E" | "KeyE" => Some("rotate"),
                "R" | "KeyR" => Some("scale"),
                "M" | "KeyM" => Some("measure"),
                "C" | "KeyC" => Some("collider"),
                _ => None,
            };
            if let Some(tool) = tool {
                return command("viewport.tool", &[("tool", tool)]);
            }
        }
        collider_edit::paint(frame, session, hit.measure.as_ref(), view_name, perspective, flip);
        if let Some(event) =
            collider_edit::handle(session, hit.measure.as_ref(), view_name, perspective, flip)
        {
            if !matches!(event, PanelEvent::None) {
                return event;
            }
        }
        return PanelEvent::None;
    }
    if editing {
        if let Some(edit) = hit.spline {
            if let Some(entity) = shown.get(edit.mark) {
                let index = edit.index.to_string();
                let x = edit.x.to_string();
                let y = edit.y.to_string();
                let begin = if edit.began { "1" } else { "0" };
                let smooth = if edit.smooth { "yes" } else { "no" };
                return command(
                    "sprite_shape.edit",
                    &[
                        ("id", entity.id.as_str()),
                        ("op", edit.op),
                        ("index", index.as_str()),
                        ("x", x.as_str()),
                        ("y", y.as_str()),
                        ("begin", begin),
                        ("smooth", smooth),
                    ],
                );
            }
        }
        if let Some(edit) = hit.poly {
            if let Some(entity) = shown.get(edit.mark) {
                let index = edit.index.to_string();
                let x = edit.x.to_string();
                let y = edit.y.to_string();
                let begin = if edit.began { "1" } else { "0" };
                return command(
                    "light2d.edit",
                    &[
                        ("id", entity.id.as_str()),
                        ("op", "point"),
                        ("index", index.as_str()),
                        ("x", x.as_str()),
                        ("y", y.as_str()),
                        ("begin", begin),
                    ],
                );
            }
        }
    }
    if brush.is_some() && tile_plane {
        if let Some(mut cells) = hit.paint_stroke_commit.clone() {
            let active = seal_editor_core::tile_tools::PaintTool::parse(paint_tool.as_str());
            if active == seal_editor_core::tile_tools::PaintTool::Fill {
                if let Some(&start) = cells.first() {
                    let at = |c: (i32, i32)| {
                        seal_editor_core::tiles::sprite_at_cell(session, paint_grid, c, tile_layer)
                    };
                    cells = seal_editor_core::tile_tools::flood_fill(
                        paint_grid,
                        tile_layer,
                        &at,
                        start,
                        seal_editor_core::tile_tools::FLOOD_FILL_LIMIT,
                    );
                }
            }
            if active == seal_editor_core::tile_tools::PaintTool::Picker {
                let pick = cells.first().and_then(|c| {
                    seal_editor_core::tiles::sprite_at_cell(session, paint_grid, *c, tile_layer)
                });
                return tile_brush::stroke_command(session, "", &[], pick.as_deref());
            }
            let paint_rel = brush
                .as_deref()
                .filter(|r| {
                    !matches!(
                        *r,
                        "erase"
                            | "picker"
                            | "fill"
                            | "eraser"
                            | "brush"
                            | "line"
                            | "rectangle"
                            | "random"
                            | "pattern"
                            | "pattern_save"
                    )
                })
                .unwrap_or("");
            if tile_erase || active == seal_editor_core::tile_tools::PaintTool::Eraser {
                return tile_brush::stroke_command(session, paint_rel, &cells, None);
            }
            if !paint_rel.is_empty() && !cells.is_empty() {
                return tile_brush::stroke_command(session, paint_rel, &cells, None);
            }
        }
    }
    if let (Some(rel), Some(kind), false) = (brush.as_deref(), brush_kind.as_deref(), tile_plane) {
        // Skip cells that already hold this picture, so going over a row paints it once.
        let adds: Vec<(String, CommandArgs)> = hit
            .paint_cells
            .iter()
            .filter(|(x, y)| {
                !session.world.entities.iter().any(|e| {
                    e.sprite == rel
                        && (e.translation[across] * flip_across - x).abs() < 0.01
                        && (e.translation[up] * flip_up - y).abs() < 0.01
                })
            })
            .map(|(x, y)| {
                let (a, b) = ((x * flip_across).to_string(), (y * flip_up).to_string());
                ("scene.add".to_string(), CommandArgs::new(&[("kind", kind), ("a", &a), ("b", &b)]))
            })
            .collect();
        if !adds.is_empty() {
            return PanelEvent::Commands(adds);
        }
    }
    if brush.is_some() && hit.key.as_deref() == Some("Escape") {
        return command("draft.set", &[("key", PLACE_BRUSH_DRAFT), ("value", "")]);
    }
    if let Some(speed) = hit.camera_speed {
        return command("viewport.camera_speed", &[("speed", &speed.to_string())]);
    }
    // Looking through the play camera never moves the saved editor view.
    if look_through.is_none()
        && (hit.camera_dirty
            || (camera.pan_x - base_pan_x).abs() > 0.01
            || (camera.pan_y - base_pan_y).abs() > 0.01
            || (camera.zoom - base_zoom).abs() > f32::EPSILON
            || (camera.yaw_deg - yaw).abs() > f32::EPSILON
            || (camera.pitch_deg - pitch).abs() > f32::EPSILON
            || (camera.distance - distance).abs() > f32::EPSILON)
    {
        return command(
            "viewport.camera",
            &[
                ("pan_x", &camera.pan_x.to_string()),
                ("pan_y", &camera.pan_y.to_string()),
                ("zoom", &(camera.zoom / zoom_scale).to_string()),
                ("yaw", &camera.yaw_deg.to_string()),
                ("pitch", &camera.pitch_deg.to_string()),
                ("distance", &camera.distance.to_string()),
            ],
        );
    }
    if let Some(indices) = hit.marquee.or(hit.lasso) {
        let indices: Vec<usize> = indices
            .into_iter()
            .filter(|index| shown.get(*index).is_some_and(|e| !e.unselectable))
            .collect();
        if indices.is_empty() {
            return command("outliner.select", &[]);
        }
        let ids: String =
            indices.iter().map(|index| shown[*index].id.as_str()).collect::<Vec<_>>().join(",");
        return command("outliner.select-set", &[("ids", ids.as_str())]);
    }
    if let Some((kind, a, b)) = hit.place_drop {
        return command(
            "scene.add",
            &[("kind", kind.as_str()), ("a", &a.to_string()), ("b", &b.to_string())],
        );
    }
    if let Some((target, pick)) = hit.menu {
        let pairs: Vec<(&str, &str)> =
            pick.args.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let run = command(&pick.command, &pairs);
        if let Some(index) = target {
            let id = shown[index].id.as_str();
            if let PanelEvent::Command(cmd, args) = run {
                return PanelEvent::Commands(vec![
                    ("outliner.select".into(), CommandArgs::new(&[("id", id)])),
                    (cmd, args),
                ]);
            }
        }
        return run;
    }
    if let (Some((index, rot)), true) = (hit.gizmo_rotate, editing) {
        let begin = if hit.began { "1" } else { "0" };
        return command(
            "viewport.move",
            &[
                ("id", shown[index].id.as_str()),
                ("mode", "euler"),
                ("a", &rot[0].to_string()),
                ("b", &rot[1].to_string()),
                ("c", &rot[2].to_string()),
                ("begin", begin),
            ],
        );
    }
    if let (Some((index, scale)), true) = (hit.gizmo_scale, editing) {
        let begin = if hit.began { "1" } else { "0" };
        return command(
            "viewport.move",
            &[
                ("id", shown[index].id.as_str()),
                ("mode", "scale"),
                ("a", &scale[0].to_string()),
                ("b", &scale[1].to_string()),
                ("c", &scale[2].to_string()),
                ("begin", begin),
            ],
        );
    }
    if let (Some((index, t)), true) = (hit.gizmo_move, editing) {
        let (first, second, spin) = match models.viewport_view.as_str() {
            "top" | "bottom" => (0, 2, 1),
            "left" | "right" => (2, 1, 0),
            _ => (0, 1, 2),
        };
        let begin = if hit.began { "1" } else { "0" };
        return command(
            "viewport.move",
            &[
                ("id", shown[index].id.as_str()),
                ("a", &t[first].to_string()),
                ("b", &t[second].to_string()),
                ("c", &t[spin].to_string()),
                ("begin", begin),
            ],
        );
    }
    if let (Some((index, axis, trans)), true) = (hit.move_axis, editing) {
        let begin = if hit.began { "1" } else { "0" };
        return command(
            "viewport.move",
            &[
                ("id", shown[index].id.as_str()),
                ("mode", "axis"),
                ("axis", &axis.to_string()),
                ("a", &trans[0].to_string()),
                ("b", &trans[1].to_string()),
                ("c", &trans[2].to_string()),
                ("begin", begin),
            ],
        );
    }
    if let (Some((index, a, b)), true) = (hit.moved, editing) {
        let begin = if hit.began { "1" } else { "0" };
        if hit.began && hit.alt_drag {
            let id = shown[index].id.as_str();
            return PanelEvent::Commands(vec![
                ("outliner.select".into(), CommandArgs::new(&[("id", id)])),
                ("edit.duplicate".into(), CommandArgs::default()),
                (
                    "viewport.move".into(),
                    CommandArgs::new(&[
                        ("a", &a.to_string()),
                        ("b", &b.to_string()),
                        ("begin", "1"),
                    ]),
                ),
            ]);
        }
        return command(
            "viewport.move",
            &[
                ("id", shown[index].id.as_str()),
                ("a", &a.to_string()),
                ("b", &b.to_string()),
                ("begin", begin),
            ],
        );
    }
    // While the game runs, clicks and letter keys belong to the player, not to
    // selection or tool switching.
    if session.world.play_player().is_some() {
        return PanelEvent::None;
    }
    if let Some(index) = hit.picked {
        if shown[index].unselectable {
            return PanelEvent::None;
        }
        let id = shown[index].id.as_str();
        if hit.pick_ctrl {
            return command("outliner.select-extend", &[("id", id)]);
        }
        if hit.pick_shift {
            return command("outliner.range_select", &[("id", id)]);
        }
        return command("outliner.select", &[("id", id), ("reveal", "1")]);
    }
    if hit.clicked {
        return command("outliner.select", &[]);
    }
    if let Some(key) = hit.key {
        let tool = match key.as_str() {
            "Q" | "KeyQ" => Some("select"),
            "W" | "KeyW" => Some("move"),
            "E" | "KeyE" => Some("rotate"),
            "R" | "KeyR" => Some("scale"),
            "M" | "KeyM" => Some("measure"),
            "C" | "KeyC" => Some("collider"),
            _ => None,
        };
        if let Some(tool) = tool {
            return command("viewport.tool", &[("tool", tool)]);
        }
        if (key == "KeyBackslash" || key == "Backquote") && hit.pick_ctrl {
            return command("viewport.space", &[]);
        }
        if key == "KeyF" {
            return command("viewport.frame", &[]);
        }
        if key == "Home" {
            return command("viewport.frame", &[("all", "1")]);
        }
        if key == "End" {
            return command("scene.floor", &[]);
        }
        if key == "V" || key == "KeyV" {
            return command("viewport.vertex_snap", &[]);
        }
        if key == "OpenBracket" {
            return command("viewport.grid_snap_step", &[("dir", "-1")]);
        }
        if key == "CloseBracket" {
            return command("viewport.grid_snap_step", &[("dir", "1")]);
        }
        return command("viewport.note", &[("text", &format!("key:{key}"))]);
    }
    PanelEvent::None
}

fn play_tint_color_parts(value: &str) -> [String; 3] {
    let color = seal_editor_core::parse_play_tint_color(Some(value));
    [color.red.to_string(), color.green.to_string(), color.blue.to_string()]
}

/// Values offered for a setting. A setting that is not listed here is edited as text.
fn setting_options(session: &EditorSession, id: &str) -> Vec<(&'static str, String)> {
    match id {
        "display.mode" => ["windowed", "fullscreen", "borderless"]
            .into_iter()
            .map(|mode| {
                let key = match mode {
                    "fullscreen" => "display-mode-fullscreen",
                    "borderless" => "display-mode-borderless",
                    _ => "display-mode-windowed",
                };
                (mode, session.messages.get(key))
            })
            .collect(),
        "ui.theme" => ["dark", "light", "contrast"]
            .into_iter()
            .map(|theme| (theme, session.messages.get(&format!("theme-{theme}"))))
            .collect(),
        "ui.density" => ["comfortable", "compact"]
            .into_iter()
            .map(|density| (density, session.messages.get(&format!("density-{density}"))))
            .collect(),
        "ui.scale" => ["50", "75", "100", "150", "200", "300"]
            .into_iter()
            .map(|scale| {
                (scale, session.messages.format("setting-scale-option", &[("scale", scale)]))
            })
            .collect(),
        "ui.follow_os_theme"
        | "ui.reduced_motion"
        | "ui.show_status_bar"
        | "startup.open_last_project"
        | "startup.show_welcome"
        | "undo.clear_on_save"
        | "log.verbose"
        | "content.show_hidden"
        | "content.use_trash"
        | "agent.bus.enabled"
        | "agent.bus.require_approval"
        | "editor.remote.enabled"
        | "layout.locked"
        | "keymap.by_physical_key"
        | "physics2d.render_interpolation"
        | "physics2d.sleeping"
        | "display.resizable"
        | "display.always_on_top"
        | "display.vsync" => ["true", "false"]
            .into_iter()
            .map(|value| (value, session.messages.get(&format!("setting-flag-{value}"))))
            .collect(),
        "jobs.worker_threads" => ["auto", "1", "2", "4", "8", "16", "32"]
            .into_iter()
            .map(|value| (value, value.into()))
            .collect(),
        id if id == seal_editor_core::QUALITY_LEVEL => ["low", "medium", "high", "ultra"]
            .into_iter()
            .map(|value| (value, session.messages.get(&format!("quality-level-{value}"))))
            .collect(),
        "rendering.glow" => ["true", "false"]
            .into_iter()
            .map(|value| (value, session.messages.get(&format!("setting-flag-{value}"))))
            .collect(),
        id if id == seal_editor_core::PROJECT_DIMENSION_SETTING => ["2d", "3d"]
            .into_iter()
            .map(|value| (value, session.messages.get(&format!("project-dimension-{value}"))))
            .collect(),
        id if id == seal_editor_core::EDITOR_LANGUAGE_SETTING => seal_editor_core::EDITOR_LANGUAGES
            .iter()
            .map(|value| {
                (*value, session.messages.get(&seal_editor_core::catalog_label_key(value)))
            })
            .collect(),
        id if id == seal_editor_core::OUTPUT_BACKEND || id.ends_with(".output_backend") => {
            seal_editor_core::backend_choices()
                .iter()
                .map(|b| (*b, seal_editor_core::backend_label(b).to_string()))
                .collect()
        }
        _ => Vec::new(),
    }
}

fn settings(frame: &mut PanelFrame, session: &mut EditorSession) -> PanelEvent {
    let mut event = PanelEvent::None;
    let query = session.models.settings_query.to_lowercase();
    frame.scroll(|frame| {
        frame.padded(|frame| {
            let mut search = session.models.settings_query.clone();
            if frame.property(&session.messages.get("settings-search"), &mut search) {
                event = command("settings.filter", &[("text", search.as_str())]);
            }
            let preview = settings_platform::preview_platform_row(frame, session);
            if !matches!(preview, PanelEvent::None) {
                event = preview;
            }
            let mut last_category = String::new();
            let settings: Vec<_> = session.models.settings.clone();
            for setting in &settings {
                if setting.id == "ui.play_tint_color" {
                    let title = session.messages.get(&setting.title_key);
                    let description = session.messages.get(&setting.description_key);
                    if !seal_editor_core::setting_matches_query(
                        &setting.id,
                        &setting.category,
                        &title,
                        &description,
                        &query,
                    ) {
                        continue;
                    }
                    let mut parts = play_tint_color_parts(setting.value());
                    if frame.property3(&title, &mut parts).is_some() {
                        let value = format!("{},{},{}", parts[0], parts[1], parts[2]);
                        event = command(
                            "settings.set",
                            &[("id", setting.id.as_str()), ("value", value.as_str())],
                        );
                    }
                    frame.muted(&description);
                    continue;
                }
                if setting.id == "ui.play_tint_strength" {
                    let title = session.messages.get(&setting.title_key);
                    let description = session.messages.get(&setting.description_key);
                    if !seal_editor_core::setting_matches_query(
                        &setting.id,
                        &setting.category,
                        &title,
                        &description,
                        &query,
                    ) {
                        continue;
                    }
                    let mut strength =
                        seal_editor_core::parse_play_tint_strength(Some(setting.value())) as f32;
                    if frame.number(&title, &mut strength, 1.0) {
                        let value = format!("{}", strength.round().clamp(0.0, 100.0) as u8);
                        event = command(
                            "settings.set",
                            &[("id", setting.id.as_str()), ("value", value.as_str())],
                        );
                    }
                    frame.muted(&description);
                    continue;
                }
                if setting.id == "display.hdr_output" || setting.id == "display.paper_white" {
                    let title = session.messages.get(&setting.title_key);
                    let description = session.messages.get(&setting.description_key);
                    if !seal_editor_core::setting_matches_query(
                        &setting.id,
                        &setting.category,
                        &title,
                        &description,
                        &query,
                    ) {
                        continue;
                    }
                    if setting.category != last_category {
                        frame.title(
                            &session
                                .messages
                                .get(&format!("settings-category-{}", setting.category)),
                        );
                        last_category.clone_from(&setting.category);
                    }
                    if setting.id == "display.hdr_output" {
                        let mut menu_pick = None;
                        let flag = if setting.value() == "true" {
                            seal_ui::PropertyCheckbox::On
                        } else {
                            seal_ui::PropertyCheckbox::Off
                        };
                        if let Some(next) = frame.property_checkbox_row(
                            &title,
                            flag,
                            false,
                            None,
                            None,
                            &mut menu_pick,
                            None,
                        ) {
                            let value = if next == seal_ui::PropertyCheckbox::On {
                                "true"
                            } else {
                                "false"
                            };
                            let layer = if session.project.is_some() { "project" } else { "user" };
                            event = command(
                                "settings.set",
                                &[("id", setting.id.as_str()), ("value", value), ("layer", layer)],
                            );
                        }
                    } else {
                        let mut nits = setting.value().parse::<f32>().unwrap_or(203.0);
                        if frame.number(&title, &mut nits, 1.0) {
                            let value = format!("{}", nits.clamp(80.0, 400.0).round() as i32);
                            let layer = if session.project.is_some() { "project" } else { "user" };
                            event = command(
                                "settings.set",
                                &[
                                    ("id", setting.id.as_str()),
                                    ("value", value.as_str()),
                                    ("layer", layer),
                                ],
                            );
                        }
                    }
                    frame.muted(&description);
                    continue;
                }
                let title = session.messages.get(&setting.title_key);
                let description = session.messages.get(&setting.description_key);
                let matches = seal_editor_core::setting_matches_query(
                    &setting.id,
                    &setting.category,
                    &title,
                    &description,
                    &query,
                );
                if !matches {
                    continue;
                }
                if setting.category != last_category {
                    frame.title(
                        &session.messages.get(&format!("settings-category-{}", setting.category)),
                    );
                    last_category.clone_from(&setting.category);
                    if setting.category == "physics2d" {
                        frame.muted(&session.messages.get("settings-physics2d-platformer-note"));
                    }
                }
                if setting.id == seal_editor_core::SORTING_LAYER_SETTING {
                    let names = seal_editor_core::split_layer_names(setting.value());
                    frame.muted(&title);
                    for index in 0..32 {
                        let n = (index + 1).to_string();
                        let label = session
                            .messages
                            .format("setting-collision-layer-slot", &[("n", n.as_str())]);
                        let mut slot = names[index].clone();
                        if frame.property(&label, &mut slot) {
                            let mut next = names.clone();
                            next[index] = slot.trim().to_string();
                            let joined = seal_editor_core::join_layer_names(&next);
                            event = command(
                                "settings.set",
                                &[
                                    ("id", setting.id.as_str()),
                                    ("value", joined.as_str()),
                                    ("layer", "project"),
                                ],
                            );
                        }
                    }
                    frame.muted(&description);
                    continue;
                }
                if setting.id == seal_editor_core::COLLISION_LAYER_SETTING {
                    let names = seal_editor_core::split_layer_names(setting.value());
                    frame.muted(&title);
                    for index in 0..32 {
                        let n = (index + 1).to_string();
                        let label = session
                            .messages
                            .format("setting-collision-layer-slot", &[("n", n.as_str())]);
                        let mut slot = names[index].clone();
                        if frame.property(&label, &mut slot) {
                            let mut next = names.clone();
                            next[index] = slot.trim().to_string();
                            let joined = seal_editor_core::join_layer_names(&next);
                            event = command(
                                "settings.set",
                                &[
                                    ("id", setting.id.as_str()),
                                    ("value", joined.as_str()),
                                    ("layer", "project"),
                                ],
                            );
                        }
                    }
                    frame.muted(&description);
                    continue;
                }
                if setting.id == seal_editor_core::MOUSE_LOOK_RAW
                    || setting.id == seal_editor_core::PAD_LIGHT
                {
                    let next = crate::input_devices_ui::setting_row(
                        frame,
                        session,
                        &setting.id,
                        &title,
                        &description,
                        setting.value(),
                    );
                    if !matches!(next, PanelEvent::None) {
                        event = next;
                    }
                    continue;
                }
                let options = setting_options(session, &setting.id);
                let picked = if options.is_empty() {
                    let mut value = setting.value().to_string();
                    frame.property(&title, &mut value).then_some(value)
                } else {
                    frame.choice(&title, setting.value(), &options)
                };
                if let Some(value) = picked {
                    let layer = if (setting.id.starts_with("display.")
                        || setting.id.starts_with("rendering.")
                        || setting.id == seal_editor_core::PROJECT_DIMENSION_SETTING)
                        && session.project.is_some()
                    {
                        "project"
                    } else {
                        "user"
                    };
                    event = command(
                        "settings.set",
                        &[("id", setting.id.as_str()), ("value", value.as_str()), ("layer", layer)],
                    );
                }
                frame.muted(&description);
                {
                    let setting_id = setting.id.clone();
                    let setting_value = setting.value().to_string();
                    if let Some(override_event) =
                        settings_platform::override_row(frame, session, &setting_id, &setting_value)
                    {
                        event = override_event;
                    }
                }
                let at_default =
                    setting.source() == "default" || setting.value() == setting.default_value;
                // A default value needs no status line; one line per setting keeps the page dense.
                if at_default && !setting.restart {
                    continue;
                }
                frame.row(|frame| {
                    if at_default {
                        frame.muted(&session.messages.get("setting-default"));
                    } else {
                        frame.muted(
                            &session.messages.get(&format!("setting-source-{}", setting.source())),
                        );
                    }
                    if setting.restart {
                        frame.muted(&session.messages.get("setting-restart"));
                    }
                    if setting.source() != "default"
                        && frame.link(&session.messages.get("command-settings-reset"))
                    {
                        event = command("settings.reset", &[("id", setting.id.as_str())]);
                    }
                });
            }
            let import_title = session.messages.get("import-settings-project-title");
            let import = if query.trim().is_empty()
                || import_title.to_lowercase().contains(&query.trim().to_lowercase())
            {
                crate::import_preset_ui::settings_project_section(frame, session)
            } else {
                PanelEvent::None
            };
            if !matches!(import, PanelEvent::None) {
                event = import;
            }
            let naming_title = session.messages.get("naming-rules-project-title");
            let naming = if query.trim().is_empty()
                || naming_title.to_lowercase().contains(&query.trim().to_lowercase())
            {
                crate::naming_rules_ui::settings_section(frame, session)
            } else {
                PanelEvent::None
            };
            if !matches!(naming, PanelEvent::None) {
                event = naming;
            }
            let pipeline_title = session.messages.get("import-pipeline-title").to_lowercase();
            if session.project.is_some()
                && (query.trim().is_empty()
                    || pipeline_title.contains(&query.trim().to_lowercase()))
            {
                let pipeline = import_pipeline_ui::section(frame, session);
                if !matches!(pipeline, PanelEvent::None) {
                    event = pipeline;
                }
            }
        });
    });
    event
}

fn recovery(frame: &mut PanelFrame, session: &EditorSession) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let mut event = PanelEvent::None;
    frame.scroll(|frame| {
        frame.padded(|frame| {
            frame.title(&session.recovery_summary());
            if session.recovered.is_empty() {
                frame.muted(&text("recovery-empty"));
            }
            for doc in &session.recovered {
                frame.label(&text(&doc.label_key));
                if !doc.path.is_empty() {
                    frame.muted(&doc.path);
                }
            }
            frame.space(8.0);
            frame.row(|frame| {
                if frame.button(&text("command-recovery-restore")) {
                    event = command("recovery.restore", &[]);
                }
                if frame.button(&text("command-recovery-keep")) {
                    event = command("recovery.keep_both", &[]);
                }
                if frame.button(&text("command-recovery-discard")) {
                    event = command("recovery.discard", &[]);
                }
            });
        });
    });
    event
}

/// What a asset-browser right-click entry does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContentAction {
    Open,
    NewFolder,
    NewScene,
    Import,
    Rename,
    Duplicate,
    Delete,
    CopyName,
    Reveal,
    CreateSprite,
    SpriteSourceImport,
    LayeredPictureImport,
    CreateVariant,
    CreateDataType,
    CreateData,
    Refresh,
}

impl ContentAction {
    fn label_key(self) -> &'static str {
        match self {
            Self::Open => "content-menu-open",
            Self::NewFolder => "content-menu-new-folder",
            Self::NewScene => "content-menu-new-scene",
            Self::Import => "content-menu-import",
            Self::Rename => "content-menu-rename",
            Self::Duplicate => "content-menu-duplicate",
            Self::Delete => "content-menu-delete",
            Self::CopyName => "content-menu-copy-name",
            Self::Reveal => "content-menu-reveal",
            Self::CreateSprite => "content-menu-create-sprite",
            Self::SpriteSourceImport => "content-menu-sprite-source-import",
            Self::LayeredPictureImport => "content-menu-layered-picture-import",
            Self::CreateVariant => "content-menu-create-variant",
            Self::CreateDataType => "content-menu-create-datatype",
            Self::CreateData => "content-menu-create-data",
            Self::Refresh => "command-content-refresh",
        }
    }
}

/// Right-click entries for one item; folders also offer to create things inside them.
fn content_item_actions(is_folder: bool, name: &str) -> Vec<ContentAction> {
    use ContentAction::*;
    let mut actions = vec![Open];
    if is_folder {
        actions.extend([NewFolder, NewScene, Import]);
    }
    actions.extend([Rename, Duplicate, Delete, CopyName, Reveal]);
    if !is_folder
        && seal_ui::sprites::is_image_name(name)
        && !seal_editor_core::aseprite::is_source_name(name)
        && !seal_editor_core::layered_picture::is_source_name(name)
    {
        actions.push(CreateSprite);
    }
    if !is_folder && seal_editor_core::aseprite::is_source_name(name) {
        actions.push(SpriteSourceImport);
    }
    if !is_folder && seal_editor_core::layered_picture::is_source_name(name) {
        actions.push(LayeredPictureImport);
    }
    if !is_folder && name.ends_with(".prefab") {
        actions.push(CreateVariant);
    }
    if is_folder {
        actions.push(CreateDataType);
        actions.push(CreateData);
    }
    actions.push(Refresh);
    actions
}

/// Right-click entries for empty space in the current folder.
const CONTENT_EMPTY_ACTIONS: [ContentAction; 5] = [
    ContentAction::NewFolder,
    ContentAction::NewScene,
    ContentAction::Import,
    ContentAction::Reveal,
    ContentAction::Refresh,
];

/// Event for an action that does not depend on the grid's rename or open state.
/// `name` is the content name the core resolves; `full` is its absolute path;
/// `folder` is the absolute folder new things go into.
fn content_action_event(
    action: ContentAction,
    name: Option<&str>,
    full: Option<&std::path::Path>,
    folder: Option<&std::path::Path>,
    tree_root: Option<&std::path::Path>,
) -> PanelEvent {
    use ContentAction::*;
    let folder_text = folder.map(|f| f.to_string_lossy().to_string());
    // Folder relative to the browser's root, for `content.enter` before an import.
    let folder_rel = match (folder, tree_root) {
        (Some(f), Some(root)) => {
            f.strip_prefix(root).ok().map(|r| r.to_string_lossy().replace('\\', "/"))
        }
        _ => None,
    };
    match action {
        NewFolder => match &folder_text {
            Some(path) => {
                command("content.new_folder", &[("path", path.as_str()), ("name", "New Folder")])
            }
            None => command("content.new_folder", &[("name", "New Folder")]),
        },
        NewScene => match &folder_text {
            Some(path) => command("content.new_scene", &[("path", path.as_str())]),
            None => command("content.new_scene", &[]),
        },
        Import => match &folder_rel {
            Some(rel) => PanelEvent::Commands(vec![
                ("content.enter".into(), CommandArgs::new(&[("path", rel.as_str())])),
                ("asset.import".into(), CommandArgs::new(&[])),
            ]),
            None => command("asset.import", &[]),
        },
        Duplicate => {
            name.map_or(PanelEvent::None, |n| command("content.duplicate", &[("name", n)]))
        }
        Delete => name.map_or(PanelEvent::None, |n| command("content.delete", &[("name", n)])),
        CopyName => match name {
            Some(n) => {
                let short = n.rsplit('/').next().unwrap_or(n);
                command("edit.copy", &[("text", short)])
            }
            None => PanelEvent::None,
        },
        Reveal => match name {
            Some(n) => command("content.reveal", &[("name", n)]),
            None => command("content.reveal", &[]),
        },
        SpriteSourceImport => match name {
            Some(n) => command("asset.sprite_source_import", &[("path", n)]),
            None => PanelEvent::None,
        },
        LayeredPictureImport => match name {
            Some(n) => command("asset.layered_picture_import", &[("path", n)]),
            None => PanelEvent::None,
        },
        CreateVariant => {
            name.map_or(PanelEvent::None, |n| command("prefab.create_variant", &[("path", n)]))
        }
        CreateDataType => {
            let base = folder_text.as_deref().unwrap_or("data/types/unit");
            command("asset.datatype_new", &[("path", base), ("unique", "yes")])
        }
        CreateData => command(
            "asset.data_new",
            &[("path", "data/value"), ("type", "data/types/unit.datatype"), ("unique", "yes")],
        ),
        CreateSprite => match full {
            Some(path) => {
                let kind = format!("sprite:{}", path.to_string_lossy());
                command("scene.add", &[("kind", kind.as_str())])
            }
            None => PanelEvent::None,
        },
        Refresh => command("content.refresh", &[]),
        Open | Rename => PanelEvent::None,
    }
}

fn command(id: &str, pairs: &[(&str, &str)]) -> PanelEvent {
    PanelEvent::Command(id.to_string(), CommandArgs::new(pairs))
}

fn inspector_visible_checkbox(
    targets: &[&seal_editor_core::MockNode],
    multi: bool,
) -> seal_ui::PropertyCheckbox {
    let visible = |node: &seal_editor_core::MockNode| field(node, "hidden") != "yes";
    let Some(first) = targets.first() else {
        return seal_ui::PropertyCheckbox::Off;
    };
    if multi && targets.len() > 1 {
        let first_vis = visible(first);
        if targets.iter().skip(1).any(|node| visible(node) != first_vis) {
            return seal_ui::PropertyCheckbox::Mixed;
        }
    }
    if visible(first) {
        seal_ui::PropertyCheckbox::On
    } else {
        seal_ui::PropertyCheckbox::Off
    }
}

fn inspector_text_field_edit(
    session: &EditorSession,
    set_target: (&str, &str),
    field: &str,
    value: &str,
    changed: bool,
    commit_focus: bool,
) -> PanelEvent {
    inspector_panel::inspector_field_edit_event(
        session,
        set_target,
        field,
        value,
        changed,
        false,
        false,
        false,
        commit_focus,
    )
}

/// Sprite section rows after the picture: the clip to play (for `.frames`
/// animation files) and the two mirror checkboxes.
#[allow(clippy::too_many_arguments)]
fn sprite_extra_rows(
    frame: &mut PanelFrame,
    session: &EditorSession,
    targets: &[&seal_editor_core::MockNode],
    sprite: &str,
    set_target: (&str, &str),
    reset_target: (&str, &str),
    prop_menu: &[seal_ui::ContextMenuEntry],
    prop_menu_pick: &mut Option<seal_ui::ContextMenuPick>,
) -> Option<PanelEvent> {
    let text = |key: &str| session.messages.get(key);
    let mut event = None;
    if seal_editor_core::sprite_frames::is_frames_path(sprite) {
        let path = match &session.project {
            Some(project) if std::path::Path::new(sprite).is_relative() => {
                project.root.join(sprite)
            }
            _ => std::path::PathBuf::from(sprite),
        };
        let names = seal_editor_core::sprite_frames::load_cached(&path)
            .map(|lib| lib.names())
            .unwrap_or_default();
        let current_raw = shared_field(targets, "animation").unwrap_or_default();
        let current = if current_raw.is_empty() {
            names.first().cloned().unwrap_or_default()
        } else {
            current_raw.clone()
        };
        let options: Vec<(&str, String)> = names.iter().map(|n| (n.as_str(), n.clone())).collect();
        let modified = !current_raw.is_empty();
        let tip = modified.then(|| inspector_reset_tip(session, "animation"));
        let (picked, reset) = frame.choice_row(
            &text("inspector-sprite-animation"),
            &current,
            &options,
            modified,
            tip.as_deref(),
            None,
            Some(prop_menu),
            prop_menu_pick,
            None,
        );
        if let Some(name) = picked {
            event = Some(command(
                "inspector.set",
                &[set_target, ("field", "animation"), ("value", name.as_str())],
            ));
        } else if reset {
            event = Some(command("inspector.reset_field", &[reset_target, ("field", "animation")]));
        }
    }
    for (label, key) in [
        ("inspector-sprite-flip-x", "flip_x"),
        ("inspector-sprite-flip-y", "flip_y"),
        ("inspector-sprite-repeat-x", "repeat_x"),
    ] {
        let on: Vec<bool> = targets.iter().map(|n| crate::field(n, key) == "yes").collect();
        let check = if on.iter().all(|v| *v) {
            seal_ui::PropertyCheckbox::On
        } else if on.iter().any(|v| *v) {
            seal_ui::PropertyCheckbox::Mixed
        } else {
            seal_ui::PropertyCheckbox::Off
        };
        let modified = inspector_field_modified(targets, key);
        if let Some(next) = frame.property_checkbox_row(
            &text(label),
            check,
            modified,
            None,
            Some(prop_menu),
            prop_menu_pick,
            Some(key),
        ) {
            let value = if matches!(next, seal_ui::PropertyCheckbox::On) { "yes" } else { "no" };
            event = Some(command("inspector.set", &[set_target, ("field", key), ("value", value)]));
        }
    }
    for (label, key, options) in [
        ("inspector-sprite-sort-mode", "sort_mode", seal_editor_core::sprite_sort::SORT_MODES),
        ("inspector-sprite-slice-mode", "slice_mode", seal_editor_core::sprite_slice::MODES),
    ] {
        let default = match key {
            "sort_mode" => "default",
            "slice_mode" => "stretch",
            _ => "",
        };
        let current =
            shared_field(targets, key).filter(|v| !v.is_empty()).unwrap_or_else(|| default.into());
        let choices: Vec<(&str, String)> =
            options.iter().map(|v| (*v, text(&format!("sprite-{key}-{v}")))).collect();
        let modified = inspector_field_modified(targets, key);
        let tip = modified.then(|| inspector_reset_tip(session, key));
        let (picked, reset) = frame.choice_row(
            &text(label),
            &current,
            &choices,
            modified,
            tip.as_deref(),
            None,
            Some(prop_menu),
            prop_menu_pick,
            None,
        );
        if let Some(value) = picked {
            event = Some(command(
                "inspector.set",
                &[set_target, ("field", key), ("value", value.as_str())],
            ));
        } else if reset {
            event = Some(command("inspector.reset_field", &[reset_target, ("field", key)]));
        }
    }
    for (label, key) in
        [("inspector-sprite-pivot", "pivot"), ("inspector-sprite-slice-size", "slice_size")]
    {
        let mut draft = shared_field(targets, key).unwrap_or_default();
        let field_error = match key {
            // Empty means the default centre pivot, not a mistake.
            "pivot" if draft.trim().is_empty() => None,
            "pivot" => seal_editor_core::sprite_pivot::parse_pivot(&draft)
                .is_none()
                .then(|| session.messages.get("inspector-sprite-pivot-invalid")),
            "slice_size" => {
                if draft.trim().is_empty() {
                    None
                } else {
                    seal_editor_core::sprite_slice::parse_slice_size(&draft)
                        .is_none()
                        .then(|| session.messages.get("inspector-sprite-slice-size-invalid"))
                }
            }
            _ => None,
        };
        let modified = inspector_field_modified(targets, key);
        let tip_owned = modified.then(|| inspector_reset_tip(session, key));
        let (changed, reset, commit_focus) = frame.property_validated_row(
            &text(label),
            &mut draft,
            field_error.as_deref(),
            modified,
            tip_owned.as_deref(),
            None,
            Some(prop_menu),
            prop_menu_pick,
            None,
        );
        let editing =
            session.models.inspector_field_edit.as_ref().is_some_and(|edit| edit.field == key);
        if changed && field_error.is_none() {
            event = Some(inspector_text_field_edit(
                session,
                set_target,
                key,
                draft.as_str(),
                true,
                commit_focus,
            ));
        } else if commit_focus && editing {
            event = Some(inspector_text_field_edit(
                session,
                set_target,
                key,
                draft.as_str(),
                false,
                true,
            ));
        } else if reset {
            event = Some(command("inspector.reset_field", &[reset_target, ("field", key)]));
        }
    }
    if let Some(current) = shared_field(targets, "mask") {
        let options = [
            ("none", text("mask-mode-none")),
            ("inside", text("mask-mode-inside")),
            ("outside", text("mask-mode-outside")),
        ];
        let modified = current != "none" && !current.is_empty();
        let tip = modified.then(|| inspector_reset_tip(session, "mask"));
        let help = text("inspector-sprite-mask-help");
        let (picked, reset) = frame.choice_row(
            &text("inspector-sprite-mask-mode"),
            &current,
            &options,
            modified,
            tip.as_deref(),
            Some(&help),
            Some(prop_menu),
            prop_menu_pick,
            Some("mask"),
        );
        if let Some(value) = picked {
            event = Some(command(
                "inspector.set",
                &[set_target, ("field", "mask"), ("value", value.as_str())],
            ));
        } else if reset {
            event = Some(command("inspector.reset_field", &[reset_target, ("field", "mask")]));
        }
    }
    event
}

/// Sprite Mask rows: shape, an optional picture, and the layer and order range.
#[allow(clippy::too_many_arguments)]
fn sprite_mask_rows(
    frame: &mut PanelFrame,
    session: &EditorSession,
    targets: &[&seal_editor_core::MockNode],
    set_target: (&str, &str),
    reset_target: (&str, &str),
    prop_menu: &[seal_ui::ContextMenuEntry],
    prop_menu_pick: &mut Option<seal_ui::ContextMenuPick>,
) -> Option<PanelEvent> {
    let text = |key: &str| session.messages.get(key);
    let mut event = None;
    let shape = shared_field(targets, "mask_shape").unwrap_or_else(|| "rect".into());
    let shape_options = [
        ("none", text("mask-shape-none")),
        ("rect", text("mask-shape-rect")),
        ("circle", text("mask-shape-circle")),
        ("alpha", text("mask-shape-alpha")),
    ];
    let shape_modified = shape != "rect";
    let shape_tip = shape_modified.then(|| inspector_reset_tip(session, "mask_shape"));
    let help = text("inspector-sprite-mask-help");
    let (picked, reset) = frame.choice_row(
        &text("inspector-mask-shape"),
        &shape,
        &shape_options,
        shape_modified,
        shape_tip.as_deref(),
        Some(&help),
        Some(prop_menu),
        prop_menu_pick,
        Some("mask_shape"),
    );
    if let Some(value) = picked {
        event = Some(command(
            "inspector.set",
            &[set_target, ("field", "mask_shape"), ("value", value.as_str())],
        ));
    } else if reset {
        event = Some(command("inspector.reset_field", &[reset_target, ("field", "mask_shape")]));
    }
    let mut rows: Vec<(&str, &str)> = Vec::new();
    if shape == "alpha" {
        rows.push(("inspector-mask-image", "mask_image"));
        rows.push(("inspector-mask-cutoff", "mask_cutoff"));
    }
    rows.extend([
        ("inspector-mask-layer-min", "mask_layer_min"),
        ("inspector-mask-layer-max", "mask_layer_max"),
        ("inspector-mask-order-min", "mask_order_min"),
        ("inspector-mask-order-max", "mask_order_max"),
    ]);
    for (label, key) in rows {
        if !targets.iter().all(|node| node.fields.iter().any(|(k, _)| k == key)) {
            continue;
        }
        let Some(current) = shared_field(targets, key) else { continue };
        let mut draft = current;
        let field_error =
            seal_editor_core::sprite_mask::check(key, &draft).map(|msg| session.messages.get(msg));
        let modified = inspector_field_modified(targets, key);
        let tip_owned = modified.then(|| inspector_reset_tip(session, key));
        let (changed, reset, commit_focus) = frame.property_validated_row(
            &text(label),
            &mut draft,
            field_error.as_deref(),
            modified,
            tip_owned.as_deref(),
            None,
            Some(prop_menu),
            prop_menu_pick,
            Some(key),
        );
        let editing =
            session.models.inspector_field_edit.as_ref().is_some_and(|edit| edit.field == key);
        if changed && field_error.is_none() {
            event = Some(inspector_text_field_edit(
                session,
                set_target,
                key,
                &draft,
                true,
                commit_focus,
            ));
        } else if commit_focus && editing {
            event = Some(inspector_text_field_edit(session, set_target, key, &draft, false, true));
        } else if reset {
            event = Some(command("inspector.reset_field", &[reset_target, ("field", key)]));
        }
    }
    event
}

/// How far a parallax layer is drawn from its saved spot so it scrolls at its own
/// speed: a factor of 1 stays put, 0 rides along with the view centre.
pub fn parallax_shift(
    entity: &seal_editor_core::Entity,
    active: bool,
    view_center: [f32; 2],
) -> [f32; 2] {
    if !active {
        return [0.0; 2];
    }
    [0, 1].map(|i| view_center[i] * (1.0 - entity.parallax[i]))
}

/// Side-by-side copies of a repeating sprite that fill `reach` scene units either
/// side of `center_x`. The copies are drawn but cannot be picked.
pub fn repeat_copies<'m>(mark: &GridMark<'m>, center_x: f32, reach: f32) -> Vec<GridMark<'m>> {
    let width = (mark.half[0] * 2.0).abs();
    if width < 0.01 {
        return Vec::new();
    }
    let first = ((center_x - reach - mark.x) / width).floor() as i64;
    let last = ((center_x + reach - mark.x) / width).ceil() as i64;
    (first..=last)
        .filter(|k| *k != 0)
        .take(400)
        .map(|k| {
            let mut copy = mark.clone();
            copy.x = mark.x + k as f32 * width;
            copy.selected = false;
            copy.pickable = false;
            copy.label = "";
            copy
        })
        .collect()
}

/// The Camera section's 2D rows: a 2D Camera checkbox, then what it follows,
/// smoothing, zoom, limits and pixel snapping while it is on.
fn camera2d_rows(
    frame: &mut PanelFrame,
    session: &EditorSession,
    targets: &[&seal_editor_core::MockNode],
    set_target: (&str, &str),
    reset_target: (&str, &str),
    prop_menu: &[seal_ui::ContextMenuEntry],
    prop_menu_pick: &mut Option<seal_ui::ContextMenuPick>,
) -> Option<PanelEvent> {
    let text = |key: &str| session.messages.get(key);
    let mut event = None;
    shared_field(targets, "camera2d")?;
    let on: Vec<bool> = targets.iter().map(|n| crate::field(n, "camera2d") == "yes").collect();
    let check = if on.iter().all(|v| *v) {
        seal_ui::PropertyCheckbox::On
    } else if on.iter().any(|v| *v) {
        seal_ui::PropertyCheckbox::Mixed
    } else {
        seal_ui::PropertyCheckbox::Off
    };
    if let Some(next) = frame.property_checkbox_row(
        &text("inspector-camera2d"),
        check,
        inspector_field_modified(targets, "camera2d"),
        None,
        Some(prop_menu),
        prop_menu_pick,
        Some("camera2d"),
    ) {
        let value = if matches!(next, seal_ui::PropertyCheckbox::On) { "yes" } else { "no" };
        event =
            Some(command("inspector.set", &[set_target, ("field", "camera2d"), ("value", value)]));
    }
    if !on.iter().all(|v| *v) {
        return event;
    }
    for (label, key) in [
        ("inspector-camera2d-follow", "cam_follow"),
        ("inspector-camera2d-smoothing", "cam_smoothing"),
        ("inspector-camera2d-zoom", "cam_zoom"),
        ("inspector-camera2d-limits", "cam_limits"),
        ("inspector-camera2d-pixels", "cam_pixels"),
        ("inspector-camera2d-drag-left", "cam_drag_left"),
        ("inspector-camera2d-drag-right", "cam_drag_right"),
        ("inspector-camera2d-drag-top", "cam_drag_top"),
        ("inspector-camera2d-drag-bottom", "cam_drag_bottom"),
    ] {
        if let Some(painted) = inspector_field_row::paint_metadata_number(
            frame,
            session,
            targets,
            key,
            &text(label),
            set_target,
            reset_target,
            prop_menu,
            prop_menu_pick,
        ) {
            if !matches!(painted, PanelEvent::None) {
                event = Some(painted);
            }
            continue;
        }
        let shown = shared_field(targets, key).unwrap_or_default();
        if let Some(painted) = inspector_pickers::try_paint(
            frame,
            session,
            key,
            &text(label),
            Some(shown.as_str()),
            set_target,
        ) {
            if !matches!(painted, PanelEvent::None) {
                event = Some(painted);
            }
            continue;
        }
        let mut draft = shown;
        let field_error = match key {
            "cam_follow" => None,
            "cam_smoothing" => validate_unit(&draft, 0.0, 10.0, "inspector-invalid-range", session),
            "cam_zoom" => validate_unit(&draft, 0.05, 50.0, "inspector-invalid-range", session),
            "cam_pixels" => validate_unit(&draft, 0.0, 1024.0, "inspector-invalid-range", session),
            "cam_drag_left" | "cam_drag_right" | "cam_drag_top" | "cam_drag_bottom" => {
                validate_unit(&draft, 0.0, 1.0, "inspector-invalid-range", session)
            }
            _ => seal_editor_core::parse_limits(&draft)
                .is_none()
                .then(|| session.messages.get("inspector-camera2d-limits-invalid")),
        };
        let modified = inspector_field_modified(targets, key);
        let tip_owned = modified.then(|| inspector_reset_tip(session, key));
        let (changed, reset, commit_focus) = frame.property_validated_row(
            &text(label),
            &mut draft,
            field_error.as_deref(),
            modified,
            tip_owned.as_deref(),
            None,
            Some(prop_menu),
            prop_menu_pick,
            None,
        );
        let editing =
            session.models.inspector_field_edit.as_ref().is_some_and(|edit| edit.field == key);
        if changed && field_error.is_none() {
            event = Some(inspector_text_field_edit(
                session,
                set_target,
                key,
                &draft,
                true,
                commit_focus,
            ));
        } else if commit_focus && editing {
            event = Some(inspector_text_field_edit(session, set_target, key, &draft, false, true));
        } else if reset {
            event = Some(command("inspector.reset_field", &[reset_target, ("field", key)]));
        }
    }
    frame.muted(&text("inspector-camera2d-drag-hint"));
    let current = shared_field(targets, "cam_cull_mask").unwrap_or_default();
    let bits = seal_editor_core::phys2d::parse_layers(&current).unwrap_or(u32::MAX);
    let layer_setting = seal_editor_core::project_sorting_layer_names(&session.models.settings);
    let labels: Vec<String> =
        seal_editor_core::sorting_layer_labels(layer_setting.as_deref()).into_iter().collect();
    let on = (0..32).filter(|index| bits & (1u32 << index) != 0).count();
    let summary = session
        .messages
        .format("inspector-flags-summary", &[("count", &on.to_string()), ("total", "32")]);
    let (next_bits, reset) = frame.flags_field_row(
        &text("inspector-camera2d-cull"),
        bits,
        32,
        &labels,
        &text("inspector-flags-all"),
        &text("inspector-flags-none"),
        inspector_field_modified(targets, "cam_cull_mask"),
        None,
        Some(&summary),
        Some(prop_menu),
        prop_menu_pick,
        Some("cam_cull_mask"),
    );
    if let Some(bits) = next_bits {
        let value = seal_editor_core::phys2d::layers_text(bits);
        event = Some(command(
            "inspector.set",
            &[set_target, ("field", "cam_cull_mask"), ("value", value.as_str())],
        ));
    } else if reset {
        event = Some(command("inspector.reset_field", &[reset_target, ("field", "cam_cull_mask")]));
    }
    event
}

fn list_property_labels(session: &EditorSession) -> seal_ui::ListPropertyLabels {
    let text = |key: &str| session.messages.get(key);
    seal_ui::ListPropertyLabels {
        mixed: text("inspector-mixed"),
        add: text("inspector-list-add"),
        clear: text("inspector-list-clear"),
        damaged: text("inspector-list-damaged"),
        remove_tip: text("inspector-list-remove-tip"),
        duplicate_tip: text("inspector-list-duplicate-tip"),
        drag_tip: text("inspector-list-drag-tip"),
    }
}

fn list_field_open(session: &EditorSession, field: &str) -> bool {
    session
        .models
        .inspector_advanced_sections
        .get(&format!("list-{field}"))
        .copied()
        .unwrap_or(true)
}

#[allow(clippy::too_many_arguments)]
fn inspector_list_row(
    frame: &mut PanelFrame,
    session: &EditorSession,
    field_key: &str,
    label: &str,
    targets: &[&seal_editor_core::MockNode],
    multi: bool,
    set_target: (&str, &str),
    _reset_target: (&str, &str),
    prop_menu: &[seal_ui::ContextMenuEntry],
    menu_pick: &mut Option<seal_ui::ContextMenuPick>,
) -> Option<PanelEvent> {
    let spec = seal_editor_core::list_spec_for_field(field_key)?;
    let shared = shared_field(targets, field_key);
    let mixed = multi && shared.is_none();
    let modified = inspector_field_modified(targets, field_key);
    let tip = modified.then(|| inspector_reset_tip(session, field_key));
    let labels = list_property_labels(session);
    let action = frame.list_property_row(
        field_key,
        label,
        shared.as_deref(),
        mixed,
        list_field_open(session, field_key),
        spec,
        &labels,
        modified,
        tip.as_deref(),
        Some(prop_menu),
        menu_pick,
    )?;
    let list_cmd = |pairs: Vec<(&str, &str)>| {
        let owned: Vec<(String, String)> =
            pairs.into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let refs: Vec<(&str, &str)> = owned.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        command("inspector.list_edit", &refs)
    };
    match action {
        seal_ui::ListPropertyAction::ToggleOpen => {
            let key = format!("list-{field_key}");
            let open = list_field_open(session, field_key);
            Some(command(
                "inspector.advanced_section",
                &[("id", &key), ("open", if open { "0" } else { "1" })],
            ))
        }
        seal_ui::ListPropertyAction::Add => {
            Some(list_cmd(vec![set_target, ("field", field_key), ("op", "add"), ("index", "0")]))
        }
        seal_ui::ListPropertyAction::Remove(index) => {
            let index_s = index.to_string();
            Some(list_cmd(vec![
                set_target,
                ("field", field_key),
                ("op", "remove"),
                ("index", index_s.as_str()),
            ]))
        }
        seal_ui::ListPropertyAction::Duplicate(index) => {
            let index_s = index.to_string();
            Some(list_cmd(vec![
                set_target,
                ("field", field_key),
                ("op", "duplicate"),
                ("index", index_s.as_str()),
            ]))
        }
        seal_ui::ListPropertyAction::Reorder { from, to } => {
            let from_s = from.to_string();
            let to_s = to.to_string();
            Some(list_cmd(vec![
                set_target,
                ("field", field_key),
                ("op", "reorder"),
                ("index", from_s.as_str()),
                ("to", to_s.as_str()),
            ]))
        }
        seal_ui::ListPropertyAction::Clear => {
            Some(list_cmd(vec![set_target, ("field", field_key), ("op", "clear"), ("index", "0")]))
        }
        seal_ui::ListPropertyAction::Set { index, value } => {
            let index_s = index.to_string();
            Some(list_cmd(vec![
                set_target,
                ("field", field_key),
                ("op", "set"),
                ("index", index_s.as_str()),
                ("value", value.as_str()),
            ]))
        }
    }
}

/// The 2D Physics section: body kind, then the collision shape, layers and
/// surface while there is a body, then the joint settings while there is a joint.
fn physics2d_rows(
    frame: &mut PanelFrame,
    session: &EditorSession,
    targets: &[&seal_editor_core::MockNode],
    set_target: (&str, &str),
    reset_target: (&str, &str),
    prop_menu: &[seal_ui::ContextMenuEntry],
    prop_menu_pick: &mut Option<seal_ui::ContextMenuPick>,
) -> Option<PanelEvent> {
    use seal_editor_core::phys2d;
    let text = |key: &str| session.messages.get(key);
    let mut event = None;
    let body = shared_field(targets, "phys_body")?;
    let choices: Vec<(&str, &[&str])> = vec![
        ("phys_body", phys2d::BODIES),
        ("phys_shape", seal_editor_core::physics2d_shapes()),
        ("phys_surface", seal_editor_core::physics2d_surfaces()),
        ("joint", seal_editor_core::physics2d_joints()),
    ];
    let mut rows: Vec<&str> = vec!["phys_body"];
    if body != "none" {
        rows.extend([
            "phys_shape",
            "phys_size",
            "phys_points",
            "phys_layer",
            "phys_mask",
            "phys_one_way",
            "phys_surface",
            "phys_ccd",
            "phys_density",
            "phys_mass",
            "phys_com",
            "phys_gravity_scale",
            "phys_linear_damping",
            "phys_angular_damping",
            "phys_can_sleep",
            "phys_lock_rotation",
            "phys_constant_force",
            "phys_constant_torque",
            "joint",
        ]);
        let joint = shared_field(targets, "joint").unwrap_or_default();
        if !joint.is_empty() && joint != "none" {
            rows.extend(phys2d::KEYS.iter().copied().filter(|key| key.starts_with("joint_")));
        }
    } else {
        rows.push("phys_part");
        if shared_field(targets, "phys_part").as_deref() == Some("yes") {
            rows.extend(["phys_shape", "phys_size", "phys_points", "phys_layer", "phys_mask"]);
        }
    }
    for key in rows {
        let Some(current) = shared_field(targets, key) else { continue };
        if key == "phys_points"
            && !matches!(shared_field(targets, "phys_shape").as_deref(), Some("polygon" | "edge"))
        {
            continue;
        }
        let label = text(&format!("inspector-{}", key.replace('_', "-")));
        if seal_editor_core::list_spec_for_field(key).is_some() {
            let multi = matches!(set_target.0, "ids");
            if let Some(picked) = inspector_list_row(
                frame,
                session,
                key,
                &label,
                targets,
                multi,
                set_target,
                reset_target,
                prop_menu,
                prop_menu_pick,
            ) {
                event = Some(picked);
            }
            continue;
        }
        let modified = inspector_field_modified(targets, key);
        let tip_owned = modified.then(|| inspector_reset_tip(session, key));
        if let Some((_, values)) = choices.iter().find(|(k, _)| *k == key) {
            let options: Vec<(&str, String)> = values
                .iter()
                .map(|v| (*v, text(&format!("phys2d-value-{}", v.replace('_', "-")))))
                .collect();
            let (picked, reset) = frame.choice_row(
                &label,
                &current,
                &options,
                modified,
                tip_owned.as_deref(),
                None,
                Some(prop_menu),
                prop_menu_pick,
                Some(key),
            );
            if let Some(picked) = picked {
                event = Some(command(
                    "inspector.set",
                    &[set_target, ("field", key), ("value", picked.as_str())],
                ));
            } else if reset {
                event = Some(command("inspector.reset_field", &[reset_target, ("field", key)]));
            }
            continue;
        }
        if matches!(key, "phys_layer" | "phys_mask") {
            let bits = phys2d::parse_layers(&current).unwrap_or(0);
            let layer_setting =
                seal_editor_core::project_collision_layer_names(&session.models.settings);
            let labels: Vec<String> =
                seal_editor_core::collision_layer_labels(layer_setting.as_deref())
                    .into_iter()
                    .collect();
            let on = (0..32).filter(|index| bits & (1u32 << index) != 0).count();
            let summary = session
                .messages
                .format("inspector-flags-summary", &[("count", &on.to_string()), ("total", "32")]);
            let (next_bits, reset) = frame.flags_field_row(
                &label,
                bits,
                32,
                &labels,
                &text("inspector-flags-all"),
                &text("inspector-flags-none"),
                modified,
                tip_owned.as_deref(),
                Some(&summary),
                Some(prop_menu),
                prop_menu_pick,
                Some(key),
            );
            if let Some(bits) = next_bits {
                let value = phys2d::layers_text(bits);
                event = Some(command(
                    "inspector.set",
                    &[set_target, ("field", key), ("value", value.as_str())],
                ));
            } else if reset {
                event = Some(command("inspector.reset_field", &[reset_target, ("field", key)]));
            }
            continue;
        }
        if matches!(
            key,
            "phys_one_way" | "phys_ccd" | "phys_part" | "phys_can_sleep" | "phys_lock_rotation"
        ) {
            let on = current == "yes";
            let check =
                if on { seal_ui::PropertyCheckbox::On } else { seal_ui::PropertyCheckbox::Off };
            if let Some(next) = frame.property_checkbox_row(
                &label,
                check,
                on,
                None,
                Some(prop_menu),
                prop_menu_pick,
                Some(key),
            ) {
                let value =
                    if matches!(next, seal_ui::PropertyCheckbox::On) { "yes" } else { "no" };
                event =
                    Some(command("inspector.set", &[set_target, ("field", key), ("value", value)]));
            }
            continue;
        }
        if let Some(painted) = inspector_pickers::try_paint(
            frame,
            session,
            key,
            &label,
            Some(current.as_str()),
            set_target,
        ) {
            if !matches!(painted, PanelEvent::None) {
                event = Some(painted);
            }
            continue;
        }
        let mut draft = current;
        let field_error = phys2d::check(key, &draft).map(|msg| session.messages.get(msg));
        let (changed, reset, commit_focus) = frame.property_validated_row(
            &label,
            &mut draft,
            field_error.as_deref(),
            modified,
            tip_owned.as_deref(),
            None,
            Some(prop_menu),
            prop_menu_pick,
            None,
        );
        let editing =
            session.models.inspector_field_edit.as_ref().is_some_and(|edit| edit.field == key);
        if changed && phys2d::check(key, &draft).is_none() {
            event = Some(inspector_text_field_edit(
                session,
                set_target,
                key,
                &draft,
                true,
                commit_focus,
            ));
        } else if commit_focus && editing {
            event = Some(inspector_text_field_edit(session, set_target, key, &draft, false, true));
        } else if reset {
            event = Some(command("inspector.reset_field", &[reset_target, ("field", key)]));
        }
    }
    if set_target.0 == "id" {
        let shape = shared_field(targets, "phys_shape").unwrap_or_default();
        let has_sprite = session
            .world
            .entity(set_target.1)
            .is_some_and(|entity| !entity.sprite.trim().is_empty());
        if shape == "polygon" && has_sprite && frame.button(&text("inspector-outline-sprite")) {
            event = Some(command("collider.outline", &[("id", set_target.1)]));
        }
    }
    event
}

/// Volume rows: kind, and the fields that kind uses.
fn volume_rows(
    frame: &mut PanelFrame,
    session: &EditorSession,
    targets: &[&seal_editor_core::MockNode],
    set_target: (&str, &str),
    reset_target: (&str, &str),
    prop_menu: &[seal_ui::ContextMenuEntry],
    prop_menu_pick: &mut Option<seal_ui::ContextMenuPick>,
) -> Option<PanelEvent> {
    use seal_editor_core::volumes;
    let text = |key: &str| session.messages.get(key);
    let kind = shared_field(targets, "volume_kind").unwrap_or_else(|| "none".into());
    let mut keys = vec!["volume_kind"];
    match kind.as_str() {
        "kill" => keys.push("volume_mode"),
        "damage" => keys.extend(["volume_variable", "volume_rate", "volume_event"]),
        "checkpoint" => keys.push("volume_event"),
        "trigger" => keys.extend(["volume_tag", "volume_event"]),
        "post" => keys.extend(seal_editor_core::post_volume::KEYS),
        _ => {}
    }
    let choices: &[(&str, &[&str])] =
        &[("volume_kind", volumes::KINDS), ("volume_mode", volumes::KILL_MODES)];
    let mut event = None;
    for key in keys {
        let Some(current) = shared_field(targets, key) else { continue };
        let label = text(&format!("inspector-{}", key.replace('_', "-")));
        let modified = inspector_field_modified(targets, key);
        let tip_owned = modified.then(|| inspector_reset_tip(session, key));
        if let Some((_, values)) = choices.iter().find(|(k, _)| *k == key) {
            let options: Vec<(&str, String)> = values
                .iter()
                .map(|v| (*v, text(&format!("volume-value-{}", v.replace('_', "-")))))
                .collect();
            let (picked, reset) = frame.choice_row(
                &label,
                &current,
                &options,
                modified,
                tip_owned.as_deref(),
                None,
                Some(prop_menu),
                prop_menu_pick,
                Some(key),
            );
            if let Some(picked) = picked {
                event = Some(command(
                    "inspector.set",
                    &[set_target, ("field", key), ("value", picked.as_str())],
                ));
            } else if reset {
                event = Some(command("inspector.reset_field", &[reset_target, ("field", key)]));
            }
            continue;
        }
        if key == "post_tint" {
            if let Some(painted) = inspector_pickers::try_paint(
                frame,
                session,
                key,
                &label,
                Some(&current),
                set_target,
            ) {
                event = Some(painted);
            }
            continue;
        }
        if seal_editor_core::post_volume::NUMERIC.contains(&key) {
            let mut number = current.parse::<f32>().unwrap_or(0.0);
            let step = if key == "post_priority" { 1.0 } else { 0.05 };
            if frame.number(&label, &mut number, step) {
                let value = if key == "post_priority" {
                    format!("{}", number.round() as i32)
                } else {
                    format!("{number}")
                };
                event = Some(command(
                    "inspector.set",
                    &[set_target, ("field", key), ("value", value.as_str())],
                ));
            }
            continue;
        }
        if key == "volume_rate" {
            let mut rate = current.parse().unwrap_or(10.0);
            if frame.number(&label, &mut rate, 1.0) {
                let value = format!("{rate}");
                event = Some(command(
                    "inspector.set",
                    &[set_target, ("field", key), ("value", value.as_str())],
                ));
            }
            continue;
        }
        let mut draft = current;
        let field_error = volumes::check(key, &draft).map(|msg| session.messages.get(msg));
        let (changed, reset, commit_focus) = frame.property_validated_row(
            &label,
            &mut draft,
            field_error.as_deref(),
            modified,
            tip_owned.as_deref(),
            None,
            Some(prop_menu),
            prop_menu_pick,
            Some(key),
        );
        let editing =
            session.models.inspector_field_edit.as_ref().is_some_and(|edit| edit.field == key);
        if changed && volumes::check(key, &draft).is_none() {
            event = Some(inspector_text_field_edit(
                session,
                set_target,
                key,
                &draft,
                true,
                commit_focus,
            ));
        } else if commit_focus && editing {
            event = Some(inspector_text_field_edit(session, set_target, key, &draft, false, true));
        } else if reset {
            event = Some(command("inspector.reset_field", &[reset_target, ("field", key)]));
        }
    }
    if kind == "post" {
        frame.muted(&text("inspector-post-hint"));
    }
    event
}

/// Area effect rows for the current mode, plus the moving-surface speed.
fn area_rows(
    frame: &mut PanelFrame,
    session: &EditorSession,
    targets: &[&seal_editor_core::MockNode],
    set_target: (&str, &str),
    reset_target: (&str, &str),
    prop_menu: &[seal_ui::ContextMenuEntry],
    prop_menu_pick: &mut Option<seal_ui::ContextMenuPick>,
) -> Option<PanelEvent> {
    use seal_editor_core::area_effect;
    let text = |key: &str| session.messages.get(key);
    let mode = shared_field(targets, "area_mode")?;
    let mut keys = vec!["area_mode"];
    match mode.as_str() {
        "directional" => keys.extend([
            "area_mask",
            "area_priority",
            "area_force",
            "area_direction",
            "area_strength",
            "area_variation",
        ]),
        "point" => keys.extend([
            "area_mask",
            "area_priority",
            "area_sign",
            "area_falloff",
            "area_strength",
            "area_variation",
            "area_range",
        ]),
        "buoyancy" => keys.extend([
            "area_mask",
            "area_priority",
            "area_surface",
            "area_density",
            "area_linear_drag",
            "area_angular_drag",
            "area_flow",
        ]),
        "gravity" => {
            keys.extend(["area_mask", "area_priority", "area_gravity_mode"]);
            let gravity_mode = shared_field(targets, "area_gravity_mode").unwrap_or_default();
            if gravity_mode == "scale" {
                keys.push("area_gravity_scale");
            } else {
                keys.push("area_gravity");
            }
        }
        "drag" => keys.extend([
            "area_mask",
            "area_priority",
            "area_linear_damping",
            "area_angular_damping",
        ]),
        _ => {}
    }
    keys.push("surface_speed");
    let choices: &[(&str, &[&str])] = &[
        ("area_mode", area_effect::MODES),
        ("area_force", area_effect::FORCE_KINDS),
        ("area_sign", area_effect::SIGNS),
        ("area_falloff", area_effect::FALLOFFS),
        ("area_gravity_mode", area_effect::GRAVITY_MODES),
    ];
    let mut event = None;
    for key in keys {
        let Some(current) = shared_field(targets, key) else { continue };
        let label = text(&format!("inspector-{}", key.replace('_', "-")));
        let modified = inspector_field_modified(targets, key);
        let tip_owned = modified.then(|| inspector_reset_tip(session, key));
        if let Some((_, values)) = choices.iter().find(|(k, _)| *k == key) {
            let options: Vec<(&str, String)> = values
                .iter()
                .map(|v| (*v, text(&format!("area-value-{}", v.replace('_', "-")))))
                .collect();
            let (picked, reset) = frame.choice_row(
                &label,
                &current,
                &options,
                modified,
                tip_owned.as_deref(),
                None,
                Some(prop_menu),
                prop_menu_pick,
                Some(key),
            );
            if let Some(picked) = picked {
                event = Some(command(
                    "inspector.set",
                    &[set_target, ("field", key), ("value", picked.as_str())],
                ));
            } else if reset {
                event = Some(command("inspector.reset_field", &[reset_target, ("field", key)]));
            }
            continue;
        }
        let mut draft = current;
        let field_error = area_effect::check(key, &draft).map(|msg| session.messages.get(msg));
        let (changed, reset, commit_focus) = frame.property_validated_row(
            &label,
            &mut draft,
            field_error.as_deref(),
            modified,
            tip_owned.as_deref(),
            None,
            Some(prop_menu),
            prop_menu_pick,
            Some(key),
        );
        let editing =
            session.models.inspector_field_edit.as_ref().is_some_and(|edit| edit.field == key);
        if changed && area_effect::check(key, &draft).is_none() {
            event = Some(inspector_text_field_edit(
                session,
                set_target,
                key,
                &draft,
                true,
                commit_focus,
            ));
        } else if commit_focus && editing {
            event = Some(inspector_text_field_edit(session, set_target, key, &draft, false, true));
        } else if reset {
            event = Some(command("inspector.reset_field", &[reset_target, ("field", key)]));
        }
    }
    event
}

/// The 2D Navigation section: a Walker checkbox, then target, speed, radius
/// and stepping around other walkers while it is on.
fn nav2d_rows(
    frame: &mut PanelFrame,
    session: &EditorSession,
    targets: &[&seal_editor_core::MockNode],
    set_target: (&str, &str),
    reset_target: (&str, &str),
    prop_menu: &[seal_ui::ContextMenuEntry],
    prop_menu_pick: &mut Option<seal_ui::ContextMenuPick>,
) -> Option<PanelEvent> {
    use seal_editor_core::nav2d;
    let text = |key: &str| session.messages.get(key);
    let mut event = None;
    let on = shared_field(targets, "nav_agent")? == "yes";
    let keys: &[&str] = if on { nav2d::KEYS } else { &nav2d::KEYS[..1] };
    for &key in keys {
        let Some(current) = shared_field(targets, key) else { continue };
        let label = text(&format!("inspector-{}", key.replace('_', "-")));
        if matches!(key, "nav_agent" | "nav_avoid") {
            let on = current == "yes";
            let check =
                if on { seal_ui::PropertyCheckbox::On } else { seal_ui::PropertyCheckbox::Off };
            if let Some(next) = frame.property_checkbox_row(
                &label,
                check,
                on,
                None,
                Some(prop_menu),
                prop_menu_pick,
                Some(key),
            ) {
                let value =
                    if matches!(next, seal_ui::PropertyCheckbox::On) { "yes" } else { "no" };
                event =
                    Some(command("inspector.set", &[set_target, ("field", key), ("value", value)]));
            }
            continue;
        }
        let modified = inspector_field_modified(targets, key);
        let tip_owned = modified.then(|| inspector_reset_tip(session, key));
        if let Some(painted) = inspector_pickers::try_paint(
            frame,
            session,
            key,
            &label,
            Some(current.as_str()),
            set_target,
        ) {
            if !matches!(painted, PanelEvent::None) {
                event = Some(painted);
            }
            continue;
        }
        let mut draft = current;
        let field_error = nav2d::check(key, &draft).map(|msg| session.messages.get(msg));
        let (changed, reset, commit_focus) = frame.property_validated_row(
            &label,
            &mut draft,
            field_error.as_deref(),
            modified,
            tip_owned.as_deref(),
            None,
            Some(prop_menu),
            prop_menu_pick,
            None,
        );
        let editing =
            session.models.inspector_field_edit.as_ref().is_some_and(|edit| edit.field == key);
        if changed && nav2d::check(key, &draft).is_none() {
            event = Some(inspector_text_field_edit(
                session,
                set_target,
                key,
                &draft,
                true,
                commit_focus,
            ));
        } else if commit_focus && editing {
            event = Some(inspector_text_field_edit(session, set_target, key, &draft, false, true));
        } else if reset {
            event = Some(command("inspector.reset_field", &[reset_target, ("field", key)]));
        }
    }
    event
}

/// Rows of the 2D Shape or Follow Path section: the given keys that the
/// objects currently show (each section starts with just its first row).
#[allow(clippy::too_many_arguments)]
fn shape2d_rows(
    frame: &mut PanelFrame,
    session: &EditorSession,
    targets: &[&seal_editor_core::MockNode],
    keys: &[&str],
    set_target: (&str, &str),
    reset_target: (&str, &str),
    prop_menu: &[seal_ui::ContextMenuEntry],
    prop_menu_pick: &mut Option<seal_ui::ContextMenuPick>,
) -> Option<PanelEvent> {
    use seal_editor_core::shape2d;
    let text = |key: &str| session.messages.get(key);
    let mut event = None;
    for &key in keys {
        // Rows appear only once every picked object shows them (a shape before its points).
        if !targets.iter().all(|node| node.fields.iter().any(|(k, _)| k == key)) {
            continue;
        }
        let label = text(&format!("inspector-{}", key.replace('_', "-")));
        if seal_editor_core::list_spec_for_field(key).is_some() {
            let multi = matches!(set_target.0, "ids");
            if let Some(picked) = inspector_list_row(
                frame,
                session,
                key,
                &label,
                targets,
                multi,
                set_target,
                reset_target,
                prop_menu,
                prop_menu_pick,
            ) {
                event = Some(picked);
            }
            continue;
        }
        let Some(current) = shared_field(targets, key) else { continue };
        if let Some(painted) = inspector_field_row::paint_metadata_number(
            frame,
            session,
            targets,
            key,
            &label,
            set_target,
            reset_target,
            prop_menu,
            prop_menu_pick,
        ) {
            if !matches!(painted, PanelEvent::None) {
                event = Some(painted);
            }
            continue;
        }
        let modified = inspector_field_modified(targets, key);
        let tip_owned = modified.then(|| inspector_reset_tip(session, key));
        let choices = match key {
            "shape2d" => Some(shape2d::KINDS),
            "shape_cap_begin" | "shape_cap_end" => Some(shape2d::CAPS),
            "follow_mode" => Some(shape2d::MODES),
            "material2d" => Some(seal_editor_core::material2d::EFFECTS),
            "mat_blend" => Some(seal_editor_core::material2d::BLENDS),
            "mat_light" => Some(seal_editor_core::material2d::LIGHT_MODES),
            _ => None,
        };
        if let Some(values) = choices {
            let options: Vec<(&str, String)> =
                values.iter().map(|v| (*v, text(&format!("shape2d-value-{v}")))).collect();
            let (picked, reset) = frame.choice_row(
                &label,
                &current,
                &options,
                modified,
                tip_owned.as_deref(),
                None,
                Some(prop_menu),
                prop_menu_pick,
                None,
            );
            if let Some(picked) = picked {
                event = Some(command(
                    "inspector.set",
                    &[set_target, ("field", key), ("value", picked.as_str())],
                ));
            } else if reset {
                event = Some(command("inspector.reset_field", &[reset_target, ("field", key)]));
            }
            continue;
        }
        if matches!(key, "shape_closed" | "follow_rotate") {
            let on = current == "yes";
            let check =
                if on { seal_ui::PropertyCheckbox::On } else { seal_ui::PropertyCheckbox::Off };
            if let Some(next) = frame.property_checkbox_row(
                &label,
                check,
                on,
                None,
                Some(prop_menu),
                prop_menu_pick,
                Some(key),
            ) {
                let value =
                    if matches!(next, seal_ui::PropertyCheckbox::On) { "yes" } else { "no" };
                event =
                    Some(command("inspector.set", &[set_target, ("field", key), ("value", value)]));
            }
            continue;
        }
        if let Some(painted) = inspector_pickers::try_paint(
            frame,
            session,
            key,
            &label,
            Some(current.as_str()),
            set_target,
        ) {
            if !matches!(painted, PanelEvent::None) {
                event = Some(painted);
            }
            continue;
        }
        let mut draft = current;
        let field_error = row_check(key, &draft)
            .map(|msg| session.messages.get(msg))
            .or_else(|| shader_problem(session, targets, key));
        let (changed, reset, commit_focus) = frame.property_validated_row(
            &label,
            &mut draft,
            field_error.as_deref(),
            modified,
            tip_owned.as_deref(),
            None,
            Some(prop_menu),
            prop_menu_pick,
            None,
        );
        let editing =
            session.models.inspector_field_edit.as_ref().is_some_and(|edit| edit.field == key);
        if changed && row_check(key, &draft).is_none() {
            event = Some(inspector_text_field_edit(
                session,
                set_target,
                key,
                &draft,
                true,
                commit_focus,
            ));
        } else if commit_focus && editing {
            event = Some(inspector_text_field_edit(session, set_target, key, &draft, false, true));
        } else if reset {
            event = Some(command("inspector.reset_field", &[reset_target, ("field", key)]));
        }
    }
    event
}

/// Why a 2D shape, follow or material field's text is refused.
fn row_check(key: &str, value: &str) -> Option<&'static str> {
    if seal_editor_core::sprite_mask::KEYS.contains(&key) {
        seal_editor_core::sprite_mask::check(key, value)
    } else if seal_editor_core::material2d::KEYS.contains(&key)
        || seal_editor_core::material2d::SHEET_KEYS.contains(&key)
    {
        seal_editor_core::material2d::check(key, value)
    } else {
        seal_editor_core::shape2d::check(key, value)
    }
}

/// What is wrong with the canvas shader file a material names, shown under its field.
fn shader_problem(
    session: &EditorSession,
    targets: &[&seal_editor_core::MockNode],
    key: &str,
) -> Option<String> {
    if key != "mat_shader" {
        return None;
    }
    let entity = targets.first().and_then(|t| session.world.entity(&t.id))?;
    let material = material_mark(session, entity)?;
    if material.shader.is_empty() {
        return Some(session.messages.get("inspector-mat-shader-missing"));
    }
    seal_editor_core::material2d::load_program(std::path::Path::new(&material.shader)).err()
}

/// An object's 2D line or shape placed on the viewport plane, if it has one.
pub(crate) fn shape2d_mark(
    session: &EditorSession,
    entity: &seal_editor_core::Entity,
    axes: [usize; 2],
    flip: [f32; 2],
) -> Option<seal_ui::ShapeMark> {
    let shape = &entity.shape2d;
    if shape.kind.is_empty() || shape.points.len() < 2 {
        return None;
    }
    let map = |p: [f32; 2]| {
        let at = [p[0], p[1], entity.translation[2]];
        [at[axes[0]] * flip[0], at[axes[1]] * flip[1]]
    };
    let world = seal_editor_core::shape2d::world_points(entity);
    let points: Vec<[f32; 2]> = world.iter().copied().map(map).collect();
    let filled = shape.kind == "polygon";
    let uvs = if filled && !shape.texture.is_empty() {
        shape
            .points
            .iter()
            .map(|p| {
                seal_editor_core::shape_style::map_uv(
                    *p,
                    &shape.points,
                    shape.uv_offset,
                    shape.uv_scale,
                    shape.uv_rotation,
                )
            })
            .collect()
    } else {
        Vec::new()
    };
    let texture =
        if uvs.is_empty() { String::new() } else { picture_path(session, &shape.texture) };
    let styled = !filled
        && seal_editor_core::shape_style::line_is_styled(
            &shape.width_curve,
            &shape.gradient,
            &shape.cap_begin,
            &shape.cap_end,
        );
    let (ribbon, ribbon_tris) = if styled {
        let (verts, tris) = seal_editor_core::shape_style::ribbon(
            &world,
            seal_editor_core::shape2d::is_closed(shape),
            shape.width,
            &shape.width_curve,
            &shape.gradient,
            shape.color,
            seal_editor_core::shape_style::cap_kind(&shape.cap_begin),
            seal_editor_core::shape_style::cap_kind(&shape.cap_end),
        );
        let verts = verts
            .into_iter()
            .map(|v| {
                let color = v.color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
                (map(v.at), color)
            })
            .collect();
        (verts, tris)
    } else {
        (Vec::new(), Vec::new())
    };
    Some(seal_ui::ShapeMark {
        points,
        width: shape.width,
        color: shape.color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8),
        filled,
        closed: seal_editor_core::shape2d::is_closed(shape),
        edit_points: false,
        uvs,
        texture,
        ribbon,
        ribbon_tris,
    })
}

fn picture_path(session: &EditorSession, path: &str) -> String {
    let raw = std::path::Path::new(path);
    match (&session.project, raw.is_relative()) {
        (Some(project), true) => project.root.join(raw).to_string_lossy().into_owned(),
        _ => path.to_string(),
    }
}

/// Pivot and nine-slice for a sprite, using the frame when that frame sets them.
pub(crate) fn sprite_place(
    session: &EditorSession,
    entity: &seal_editor_core::Entity,
) -> ([f32; 2], [f32; 4]) {
    let project = session.project.as_ref().map(|p| p.root.as_path());
    let Some((pivot, border)) = seal_editor_core::sprite_frames::frame_look(
        &entity.sprite,
        &entity.animation,
        entity.frame_override,
        entity.anim_start,
        session.world.play_time,
        project,
    ) else {
        return ([0.5, 0.5], entity.slice);
    };
    let slice = if border.iter().any(|v| *v != 0.0) { border } else { entity.slice };
    (pivot, slice)
}

/// The Light section's 2D rows: light type, then range, cone and shadows for
/// point and spot lights.
fn light2d_rows(
    frame: &mut PanelFrame,
    session: &EditorSession,
    targets: &[&seal_editor_core::MockNode],
    set_target: (&str, &str),
    reset_target: (&str, &str),
    prop_menu: &[seal_ui::ContextMenuEntry],
    prop_menu_pick: &mut Option<seal_ui::ContextMenuPick>,
) -> Option<PanelEvent> {
    let text = |key: &str| session.messages.get(key);
    let mut event = None;
    let raw = shared_field(targets, "light2d")?;
    let current = if raw.is_empty() { "none".to_string() } else { raw.clone() };
    let options: Vec<(&str, String)> =
        ["none", "global", "point", "spot", "directional", "freeform"]
            .into_iter()
            .map(|kind| (kind, text(&format!("light2d-kind-{kind}"))))
            .collect();
    let modified = !raw.is_empty();
    let tip = modified.then(|| inspector_reset_tip(session, "light2d"));
    let (picked, reset) = frame.choice_row(
        &text("inspector-light2d"),
        &current,
        &options,
        modified,
        tip.as_deref(),
        Some(&text("inspector-light2d-help")),
        Some(prop_menu),
        prop_menu_pick,
        Some("light2d"),
    );
    if let Some(kind) = picked {
        let value = if kind == "none" { "" } else { kind.as_str() };
        event =
            Some(command("inspector.set", &[set_target, ("field", "light2d"), ("value", value)]));
    } else if reset {
        event = Some(command("inspector.reset_field", &[reset_target, ("field", "light2d")]));
    }
    if raw.is_empty() || raw == "none" {
        return event;
    }
    let blend_now = shared_field(targets, "light_blend").unwrap_or_else(|| "additive".into());
    let blend_options: Vec<(&str, String)> = ["additive", "multiply", "alpha"]
        .into_iter()
        .map(|kind| (kind, text(&format!("light2d-blend-{kind}"))))
        .collect();
    let blend_modified = inspector_field_modified(targets, "light_blend");
    let blend_tip = blend_modified.then(|| inspector_reset_tip(session, "light_blend"));
    let (blend_pick, blend_reset) = frame.choice_row(
        &text("inspector-light2d-blend"),
        &blend_now,
        &blend_options,
        blend_modified,
        blend_tip.as_deref(),
        Some(&text("inspector-light2d-blend-help")),
        Some(prop_menu),
        prop_menu_pick,
        Some("light_blend"),
    );
    if let Some(kind) = blend_pick {
        event = Some(command(
            "inspector.set",
            &[set_target, ("field", "light_blend"), ("value", kind.as_str())],
        ));
    } else if blend_reset {
        event = Some(command("inspector.reset_field", &[reset_target, ("field", "light_blend")]));
    }
    if raw == "global" {
        return event;
    }
    let mut rows = Vec::new();
    if matches!(raw.as_str(), "point" | "spot") {
        rows.push(("inspector-light2d-range", "light_range", 0.01, 10000.0));
    }
    if raw == "spot" {
        rows.push(("inspector-light2d-cone", "light_cone", 1.0, 360.0));
    }
    if raw == "directional" {
        rows.push(("inspector-light2d-angle", "light_angle", -360.0, 360.0));
    }
    if raw == "freeform" {
        rows.push(("inspector-light2d-falloff", "light_falloff", 0.0, 10000.0));
    }
    for (label, key, min, max) in rows {
        if let Some(painted) = inspector_field_row::paint_metadata_number(
            frame,
            session,
            targets,
            key,
            &text(label),
            set_target,
            reset_target,
            prop_menu,
            prop_menu_pick,
        ) {
            if !matches!(painted, PanelEvent::None) {
                event = Some(painted);
            }
            continue;
        }
        let mut draft = shared_field(targets, key).unwrap_or_default();
        let field_error = validate_unit(&draft, min, max, "inspector-invalid-range", session);
        let modified = inspector_field_modified(targets, key);
        let tip_owned = modified.then(|| inspector_reset_tip(session, key));
        let (changed, reset, commit_focus) = frame.property_validated_row(
            &text(label),
            &mut draft,
            field_error.as_deref(),
            modified,
            tip_owned.as_deref(),
            None,
            Some(prop_menu),
            prop_menu_pick,
            Some(key),
        );
        let editing =
            session.models.inspector_field_edit.as_ref().is_some_and(|edit| edit.field == key);
        if changed && field_error.is_none() {
            event = Some(inspector_text_field_edit(
                session,
                set_target,
                key,
                &draft,
                true,
                commit_focus,
            ));
        } else if commit_focus && editing {
            event = Some(inspector_text_field_edit(session, set_target, key, &draft, false, true));
        } else if reset {
            event = Some(command("inspector.reset_field", &[reset_target, ("field", key)]));
        }
    }
    let on: Vec<bool> = targets.iter().map(|n| crate::field(n, "light_shadows") == "yes").collect();
    let check = if on.iter().all(|v| *v) {
        seal_ui::PropertyCheckbox::On
    } else if on.iter().any(|v| *v) {
        seal_ui::PropertyCheckbox::Mixed
    } else {
        seal_ui::PropertyCheckbox::Off
    };
    if let Some(next) = frame.property_checkbox_row(
        &text("inspector-light2d-shadows"),
        check,
        inspector_field_modified(targets, "light_shadows"),
        None,
        Some(prop_menu),
        prop_menu_pick,
        Some("light_shadows"),
    ) {
        let value = if matches!(next, seal_ui::PropertyCheckbox::On) { "yes" } else { "no" };
        event = Some(command(
            "inspector.set",
            &[set_target, ("field", "light_shadows"), ("value", value)],
        ));
    }
    if raw == "freeform" {
        if let Some(picked) = inspector_list_row(
            frame,
            session,
            "light_points",
            &text("inspector-light2d-points"),
            targets,
            matches!(set_target.0, "ids"),
            set_target,
            reset_target,
            prop_menu,
            prop_menu_pick,
        ) {
            event = Some(picked);
        }
    }
    event
}

fn inspector_field_modified(targets: &[&seal_editor_core::MockNode], field_key: &str) -> bool {
    let default = seal_editor_core::inspector_field_default(field_key);
    // An unset field reads as empty and uses the default.
    targets.iter().any(|node| {
        let value = crate::field(node, field_key);
        !value.is_empty() && value != default
    })
}

fn inspector_reset_tip(session: &EditorSession, field_key: &str) -> String {
    session.messages.format(
        "inspector-reset-default",
        &[("value", seal_editor_core::inspector_field_default(field_key))],
    )
}

fn inspector_visible(
    session: &EditorSession,
    label_key: &str,
    fields: &[&str],
    targets: &[&seal_editor_core::MockNode],
) -> bool {
    if session.models.inspector_view.favourites_only
        && !session.models.inspector_favourites.contains(label_key)
    {
        return false;
    }
    if session.models.inspector_view.only_modified
        && !fields.iter().any(|key| inspector_field_modified(targets, key))
    {
        return false;
    }
    let query = session.models.inspector_query.to_lowercase();
    if query.is_empty() {
        return true;
    }
    let label = session.messages.get(label_key).to_lowercase();
    label.contains(&query)
        || label_key.contains(&query)
        || fields.iter().any(|field| field.contains(&query))
}

fn current_log(session: &EditorSession) -> Vec<LogLine> {
    collect_log_lines(session.foundation.console_clear_before)
}

#[cfg(test)]
mod content_menu_tests {
    use super::*;

    fn cmds(event: PanelEvent) -> Vec<(String, CommandArgs)> {
        match event {
            PanelEvent::Command(id, args) => vec![(id, args)],
            PanelEvent::Commands(list) => list,
            _ => vec![],
        }
    }

    #[test]
    fn menus_offer_the_right_entries() {
        use ContentAction::*;
        let png = content_item_actions(false, "coin.png");
        assert!(png.contains(&CreateSprite) && png.contains(&Duplicate) && png.contains(&Reveal));
        assert!(!content_item_actions(false, "notes.txt").contains(&CreateSprite));
        let psd = content_item_actions(false, "hero.psd");
        assert!(psd.contains(&LayeredPictureImport));
        assert!(!psd.contains(&CreateSprite));
        let folder = content_item_actions(true, "sprites");
        assert!(
            folder.contains(&NewFolder) && folder.contains(&NewScene) && folder.contains(&Import)
        );
        assert_eq!(CONTENT_EMPTY_ACTIONS, [NewFolder, NewScene, Import, Reveal, Refresh]);
    }

    #[test]
    fn menu_entries_send_core_commands() {
        use ContentAction::*;
        let root = std::path::Path::new("/p");
        let art = root.join("art");
        let png = art.join("a.png");
        let go = |a| {
            cmds(content_action_event(a, Some("art/a.png"), Some(&png), Some(&art), Some(root)))
        };
        let dup = go(Duplicate);
        assert_eq!(
            (dup[0].0.as_str(), dup[0].1.get("name")),
            ("content.duplicate", Some("art/a.png"))
        );
        assert_eq!(go(Reveal)[0].0, "content.reveal");
        let sprite = go(CreateSprite);
        assert_eq!(
            (sprite[0].0.as_str(), sprite[0].1.get("kind")),
            ("scene.add", Some(format!("sprite:{}", png.display()).as_str()))
        );
        let scene = go(NewScene);
        assert_eq!(
            (scene[0].0.as_str(), scene[0].1.get("path")),
            ("content.new_scene", Some(art.to_string_lossy().as_ref()))
        );
        let import: Vec<String> = go(Import).into_iter().map(|(id, _)| id).collect();
        assert_eq!(import, ["content.enter", "asset.import"]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_panel_has_a_unique_id_and_a_title() {
        let mut registry = PanelRegistry::default();
        register_builtin(&mut registry).unwrap();
        let messages = seal_editor_core::Messages::english();
        assert_eq!(registry.iter().count(), 32);
        for panel in registry.iter() {
            let title = messages.get(&panel.title_key);
            assert_ne!(title, panel.title_key);
        }
    }

    #[test]
    fn the_english_catalog_matches_editor_sources() {
        seal_editor_core::verify_english_catalog().expect("English catalog");
    }
}

/// World size for a sprite picture from its import sidecar when pixels-per-unit is set.
#[derive(Clone, Copy)]
pub(crate) struct SpriteCard {
    /// Full width and height in world units (the object's scale when the picture has no pixels-per-unit).
    pub full: [f32; 2],
}

pub(crate) fn sprite_card(path: &str, scale_x: f32, scale_y: f32) -> SpriteCard {
    use seal_editor_core::import_presets::{picture_look, sprite_world_size};
    let look = picture_look(std::path::Path::new(path));
    let px = seal_ui::sprites::picture_pixels(path).unwrap_or([0.0, 0.0]);
    SpriteCard { full: sprite_world_size([scale_x, scale_y], px, look.pixels_per_unit) }
}

/// Picture file and texture rectangle to draw for `entity` right now
/// (plays `.frames` clips and applies flips).
pub(crate) fn sprite_draw(
    session: &EditorSession,
    entity: &seal_editor_core::Entity,
) -> (Option<String>, [f32; 4]) {
    let root = session.project.as_ref().map(|p| p.root.as_path());
    match entity.sprite_draw(root, session.world.play_time) {
        Some((path, uv)) => {
            let path = path.to_string_lossy().into_owned();
            let uv = sheet_uv_for(session, entity, &path, uv);
            (Some(path), uv)
        }
        None => (None, [0.0, 0.0, 1.0, 1.0]),
    }
}

fn picture_pivot(path: &str) -> [f32; 2] {
    seal_editor_core::import_presets::picture_look(std::path::Path::new(path)).pivot
}

/// Multiple mode uses the horizontal sheet of square cells when the object
/// still has a single frame. An object that already sets frames keeps those.
fn sheet_uv_for(
    session: &EditorSession,
    entity: &seal_editor_core::Entity,
    path: &str,
    uv: [f32; 4],
) -> [f32; 4] {
    if entity.frames > 1 || seal_editor_core::sprite_frames::is_frames_path(&entity.sprite) {
        return uv;
    }
    let look = seal_editor_core::import_presets::picture_look(std::path::Path::new(path));
    if !look.multiple {
        return uv;
    }
    let Some([width, height]) = seal_ui::sprites::picture_pixels(path) else {
        return uv;
    };
    let frames = seal_editor_core::picture::sheet_frames(width as u32, height as u32);
    if frames <= 1 {
        return uv;
    }
    let index = seal_editor_core::picture::sheet_index(frames, session.world.play_time, entity.fps);
    seal_editor_core::picture::sheet_uv(uv, frames, index)
}

/// The node editor: a toolbar strip, the canvas, and the selected node's settings.
fn graph_panel(frame: &mut PanelFrame, session: &mut EditorSession) -> PanelEvent {
    use seal_ui::graph_canvas::{frame_all, graph_canvas, GraphCanvasText};
    use seal_ui::state_machine_canvas::{
        frame_states, state_machine_canvas, state_machine_form, MachineText,
    };
    seal_editor_core::graph_doc::restore_graph(session);
    let text = |key: &str| session.messages.get(key);
    let (new, open, undo, redo, fit) = (
        text("graph-new"),
        text("graph-open-selected"),
        text("graph-undo"),
        text("graph-redo"),
        text("graph-frame"),
    );
    let (search, delete, unlink, hint) = (
        text("graph-search"),
        text("graph-delete-node"),
        text("graph-disconnect"),
        text("graph-canvas-hint"),
    );
    let (saved, ok, settings_title) =
        (text("graph-saved"), text("graph-ok"), text("graph-settings"));
    let save_failed =
        |error: &str| session.messages.format("graph-save-failed", &[("error", error)]);
    let problems_text =
        |count: usize| session.messages.format("graph-problems", &[("count", &count.to_string())]);
    let empty = (text("graph-empty"), text("graph-empty-hint"));
    let (bp_text, cont_text, step_text, no_vars) = (
        text("graph-breakpoint"),
        text("graph-continue"),
        text("graph-step"),
        text("graph-no-vars"),
    );
    let (states_label, nodes_label, add_state, state_machine_title) = (
        text("graph-states"),
        text("graph-nodes"),
        text("graph-add-state"),
        text("graph-state-machine"),
    );
    let m_state_name = text("graph-state-name");
    let m_initial = text("graph-initial");
    let m_on_enter = text("graph-on-enter");
    let m_on_update = text("graph-on-update");
    let m_on_exit = text("graph-on-exit");
    let m_delete_state = text("graph-delete-state");
    let m_delete_transition = text("graph-delete-transition");
    let m_from = text("graph-from");
    let m_to = text("graph-to");
    let m_priority = text("graph-priority");
    let m_condition = text("graph-condition");
    let m_cond_flag = text("graph-cond-flag");
    let m_cond_compare = text("graph-cond-compare");
    let m_cond_event = text("graph-cond-event");
    let m_cond_after = text("graph-cond-after");
    let m_seconds = text("graph-seconds");
    let m_variable = text("graph-variable");
    let m_event_name = text("graph-event-name");
    let m_compare_value = text("graph-compare-value");
    let m_hint = text("graph-machine-hint");
    let m_compare_op = text("graph-compare-op");
    let machine_text = MachineText {
        state_name: &m_state_name,
        initial: &m_initial,
        on_enter: &m_on_enter,
        on_update: &m_on_update,
        on_exit: &m_on_exit,
        delete_state: &m_delete_state,
        delete_transition: &m_delete_transition,
        from: &m_from,
        to: &m_to,
        priority: &m_priority,
        condition: &m_condition,
        cond_flag: &m_cond_flag,
        cond_compare: &m_cond_compare,
        cond_event: &m_cond_event,
        cond_after: &m_cond_after,
        seconds: &m_seconds,
        variable: &m_variable,
        event_name: &m_event_name,
        compare_value: &m_compare_value,
        hint: &m_hint,
        compare_op: &m_compare_op,
    };
    let debug_line =
        session.graph.as_ref().and_then(|d| d.debug.as_ref()).map(|d| match d.paused_at {
            Some(node) => session
                .messages
                .format("graph-paused-at", &[("node", &node.to_string()), ("object", &d.object)]),
            None => session.messages.format("graph-watching", &[("object", &d.object)]),
        });
    let mut event = PanelEvent::None;
    let ui = frame.ui_mut();
    // Debugger keys while the graph panel has focus: F9 breakpoint, F5 continue, F10 step.
    let mut debug_go: Option<bool> = None;
    if session.foundation.focused_panel_id == GRAPH && !ui.ctx().egui_wants_keyboard_input() {
        let (f5, f9, f10) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::F5),
                i.key_pressed(egui::Key::F9),
                i.key_pressed(egui::Key::F10),
            )
        });
        if let Some(doc) = session.graph.as_mut() {
            if f9 {
                if let Some(id) = doc.selected.filter(|id| doc.node(*id).is_some()) {
                    doc.toggle_breakpoint(id);
                }
            }
            let paused = doc.debug.as_ref().is_some_and(|d| d.paused_at.is_some());
            if paused && f5 {
                debug_go = Some(false);
            } else if paused && f10 {
                debug_go = Some(true);
            }
        }
    }
    // Toolbar strip.
    let mut fit_now = false;
    ui.horizontal(|ui| {
        if seal_ui::crisp::button(ui, egui::Button::new(&new), true, false).clicked() {
            event = PanelEvent::Command("graph.new".into(), CommandArgs::default());
        }
        if seal_ui::crisp::button(ui, egui::Button::new(&open), true, false).clicked() {
            event = PanelEvent::Command("graph.open".into(), CommandArgs::default());
        }
        let Some(doc) = session.graph.as_mut() else { return };
        ui.separator();
        if ui
            .add_enabled_ui(doc.can_undo(), |ui| {
                seal_ui::crisp::button(ui, egui::Button::new(&undo), true, false)
            })
            .inner
            .clicked()
        {
            doc.undo();
        }
        if ui
            .add_enabled_ui(doc.can_redo(), |ui| {
                seal_ui::crisp::button(ui, egui::Button::new(&redo), true, false)
            })
            .inner
            .clicked()
        {
            doc.redo();
        }
        if seal_ui::crisp::button(ui, egui::Button::new(&fit), true, false).clicked() {
            fit_now = true;
        }
        ui.separator();
        let view = if doc.show_machine { &nodes_label } else { &states_label };
        if seal_ui::crisp::button(ui, egui::Button::new(view), true, doc.show_machine)
            .on_hover_text(&state_machine_title)
            .clicked()
        {
            doc.toggle_machine_view();
        }
        let inspector_layout_title = session.messages.get("graph-inspector-layout-tab");
        if seal_ui::crisp::button(
            ui,
            egui::Button::new(&inspector_layout_title),
            true,
            doc.show_inspector_layout,
        )
        .on_hover_text(session.messages.get("graph-inspector-layout-hint"))
        .clicked()
        {
            doc.toggle_inspector_layout_view();
        }
        if doc.show_machine
            && seal_ui::crisp::button(ui, egui::Button::new(&add_state), true, false).clicked()
        {
            let n = doc.graph.machine.as_ref().map(|m| m.states.len()).unwrap_or(0) as f32;
            doc.add_state([40.0 + (n % 4.0) * 180.0, 40.0 + (n / 4.0).floor() * 100.0]);
        }
        ui.separator();
        ui.label(egui::RichText::new(&doc.label).monospace())
            .on_hover_text(doc.path.display().to_string());
        let status = if let Some(error) = &doc.save_error {
            egui::RichText::new(save_failed(error)).color(egui::Color32::from_rgb(240, 110, 90))
        } else if !doc.problems.is_empty() {
            let first = doc.problems[0].1.clone();
            egui::RichText::new(format!("{}: {first}", problems_text(doc.problems.len())))
                .color(egui::Color32::from_rgb(255, 170, 40))
        } else {
            egui::RichText::new(format!("{saved} \u{00b7} {ok}"))
                .color(ui.visuals().weak_text_color())
        };
        ui.label(status);
    });
    // Debugger strip while playing: where play stopped, Continue and Step, and the watched variables.
    if let (Some(line), Some(debug)) =
        (&debug_line, session.graph.as_ref().and_then(|d| d.debug.clone()))
    {
        let gold = egui::Color32::from_rgb(255, 214, 64);
        ui.horizontal_wrapped(|ui| {
            let paused = debug.paused_at.is_some();
            ui.label(egui::RichText::new(line).color(if paused {
                gold
            } else {
                ui.visuals().weak_text_color()
            }));
            if paused {
                if seal_ui::crisp::button(ui, egui::Button::new(&cont_text), true, false).clicked()
                {
                    debug_go = Some(false);
                }
                if seal_ui::crisp::button(ui, egui::Button::new(&step_text), true, false).clicked()
                {
                    debug_go = Some(true);
                }
            }
            ui.separator();
            let shown: Vec<_> =
                debug.vars.iter().filter(|(name, _)| !name.starts_with("__fn_")).collect();
            if shown.is_empty() {
                ui.label(egui::RichText::new(&no_vars).color(ui.visuals().weak_text_color()));
            }
            for (name, value) in shown {
                ui.label(egui::RichText::new(format!("{name} = {value}")).monospace());
            }
            if !debug.state.is_empty() {
                ui.separator();
                ui.label(
                    egui::RichText::new(
                        session.messages.format("graph-current-state", &[("name", &debug.state)]),
                    )
                    .color(gold),
                );
            }
        });
    }
    if let Some(single) = debug_go.take() {
        session.debug_continue(single);
    }
    let ui = frame.ui_mut();
    let Some(doc) = session.graph.as_mut() else {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label(&empty.0);
            ui.label(egui::RichText::new(&empty.1).color(ui.visuals().weak_text_color()));
        });
        return event;
    };
    seal_ui::graph_functions::show(ui, doc);
    // Settings strip for the selected node or state, along the bottom.
    let machine_view = doc.show_machine;
    let inspector_layout_view = doc.show_inspector_layout;
    let selected = doc.selected.filter(|id| doc.node(*id).is_some());
    let strip_h = if inspector_layout_view {
        220.0
    } else if machine_view {
        match doc.machine_pick {
            Some(seal_editor_core::graph_doc::MachineSel::Transition(_)) => 110.0,
            Some(seal_editor_core::graph_doc::MachineSel::State(_)) => 64.0,
            None => 28.0,
        }
    } else if selected.is_some() {
        30.0
    } else {
        0.0
    };
    let canvas_rect = {
        let r = ui.available_rect_before_wrap();
        egui::Rect::from_min_max(r.min, egui::pos2(r.max.x, r.max.y - strip_h))
    };
    if fit_now || doc.zoom <= 0.0 {
        if machine_view {
            frame_states(doc, canvas_rect.size());
        } else if !inspector_layout_view {
            frame_all(doc, canvas_rect.size());
        }
    }
    let labels = GraphCanvasText {
        search: &search,
        delete_node: &delete,
        disconnect: &unlink,
        hint: &hint,
        breakpoint: &bp_text,
    };
    let result = ui
        .scope_builder(egui::UiBuilder::new().max_rect(canvas_rect), |ui| {
            if inspector_layout_view {
                seal_ui::state_machine_canvas::MachineOutput::default()
            } else if machine_view {
                state_machine_canvas(ui, doc, &machine_text)
            } else {
                let node = graph_canvas(ui, doc, &labels);
                seal_ui::state_machine_canvas::MachineOutput {
                    touched: node.touched,
                    refused: node.refused,
                }
            }
        })
        .inner;
    if result.touched {
        session.foundation.focused_panel_id = GRAPH.into();
    }
    if let Some(reason) = result.refused {
        session.notice = Some(reason);
    }
    let Some(doc) = session.graph.as_mut() else { return event };
    if inspector_layout_view {
        let text = |k: &str| session.messages.get(k);
        let strip_rect = egui::Rect::from_min_max(
            egui::pos2(canvas_rect.min.x, canvas_rect.max.y),
            egui::pos2(canvas_rect.max.x, canvas_rect.max.y + strip_h),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(strip_rect), |ui| {
            crate::graph_inspector_layout::paint_strip(&mut PanelFrame::new(ui), doc, &text);
        });
        return event;
    }
    if machine_view {
        if let Some(reason) = state_machine_form(ui, doc, &machine_text) {
            session.notice = Some(reason);
        }
        return event;
    }
    if let Some(id) = selected.filter(|id| doc.node(*id).is_some()) {
        ui.horizontal(|ui| {
            let kind = doc.node(id).map(|n| n.kind.clone()).unwrap_or_default();
            let about = seal_editor_core::graph_doc::KINDS
                .iter()
                .find(|(k, _)| *k == kind)
                .map(|(_, d)| *d)
                .unwrap_or("");
            ui.label(egui::RichText::new(format!("{settings_title}: {kind}")).strong())
                .on_hover_text(about);
            for (key, value) in doc.settings(id) {
                ui.label(&key);
                let fid = ui.id().with(("graph-setting", id, &key));
                let mut buffer: String =
                    ui.data(|d| d.get_temp(fid)).unwrap_or_else(|| value.clone());
                let edit = egui::TextEdit::singleline(&mut buffer).id(fid).desired_width(90.0);
                let resp = seal_ui::crisp::well_edit(ui, edit);
                if resp.has_focus() {
                    ui.data_mut(|d| d.insert_temp(fid, buffer));
                } else {
                    if resp.lost_focus() && buffer != value {
                        doc.set_setting(id, &key, &buffer);
                    }
                    ui.data_mut(|d| d.remove::<String>(fid));
                }
            }
        });
    }
    event
}

/// The scene's 2D lights and the solid boxes that block them, in the flat
/// front view's plane (x right, y up). Empty when the scene has no 2D light.
pub fn scene_lights_2d(
    entities: &[seal_editor_core::Entity],
) -> (Vec<seal_editor_core::light2d::Light>, Vec<seal_editor_core::light2d::Blocker>) {
    use seal_editor_core::light2d::{Blocker, Kind, Light};
    let lights: Vec<Light> = entities
        .iter()
        .filter(|e| !e.hidden)
        .filter_map(|e| {
            let kind = Kind::parse(&e.light2d.kind)?;
            Some(Light {
                kind,
                pos: [e.translation[0], e.translation[1]],
                color: e.color,
                intensity: e.intensity.max(0.0),
                range: e.light2d.range.max(0.01),
                angle: e.rotation[2],
                cone: e.light2d.cone,
                shadows: e.light2d.shadows,
                blend: seal_editor_core::light2d::Blend::parse(&e.light2d.blend),
                falloff: e.light2d.falloff.max(0.0),
                points: seal_editor_core::light2d::place_points(
                    [e.translation[0], e.translation[1]],
                    e.rotation[2],
                    [e.scale[0], e.scale[1]],
                    &e.light2d.points,
                ),
            })
        })
        .collect();
    if lights.is_empty() {
        return (lights, Vec::new());
    }
    let blockers = entities
        .iter()
        .filter(|e| !e.hidden && e.light2d.kind.is_empty() && e.tag != "light")
        .filter(|e| {
            matches!(e.tag.as_str(), "ground" | "occluder" | "nav_block")
                || (matches!(e.shape, Some(seal_editor_core::Shape::Cube)) && !e.dynamic)
        })
        .map(|e| Blocker {
            center: [e.translation[0], e.translation[1]],
            half: [0.5 * e.scale[0].abs(), 0.5 * e.scale[1].abs()],
        })
        .collect();
    (lights, blockers)
}

/// Light every sprite gets on top of the 2D lights, so unlit areas are dark but readable.
pub const AMBIENT_2D: [f32; 3] = [0.08, 0.08, 0.1];

/// Full path of an entity's normal map, or None when it has none.
/// An object's sprite material for drawing, with its shader file found in the project.
pub(crate) fn material_mark(
    session: &EditorSession,
    entity: &seal_editor_core::Entity,
) -> Option<seal_editor_core::material2d::Material2d> {
    let mut material = entity.material2d.clone();
    let default_blend = material.blend.is_empty() || material.blend == "alpha";
    let default_light = material.light_mode.is_empty() || material.light_mode == "lit";
    if material.effect.is_empty() && default_blend && default_light {
        return None;
    }
    if !material.shader.is_empty() {
        let raw = std::path::Path::new(&material.shader);
        if let (Some(project), true) = (&session.project, raw.is_relative()) {
            material.shader = project.root.join(raw).to_string_lossy().into_owned();
        }
    }
    Some(material)
}

pub(crate) fn normal_map_path(
    session: &EditorSession,
    entity: &seal_editor_core::Entity,
) -> Option<String> {
    if entity.normal_map.is_empty() {
        return None;
    }
    let raw = std::path::Path::new(&entity.normal_map);
    Some(match (&session.project, raw.is_relative()) {
        (Some(project), true) => project.root.join(raw).to_string_lossy().into_owned(),
        _ => entity.normal_map.clone(),
    })
}

/// Every visible light after the first (the first is the key light that casts
/// shadows) becomes a coloured point light for the viewport's probe bake.
pub fn viewport_fill_lights(
    entities: &[seal_editor_core::Entity],
) -> Vec<seal_editor_core::probes::Light> {
    entities
        .iter()
        .filter(|entity| entity.tag == "light" && !entity.hidden)
        .skip(1)
        .map(|light| {
            let power = light.intensity.max(0.0);
            seal_editor_core::probes::Light::Point {
                pos: light.translation,
                color: [
                    light.color[0] * power * 0.25,
                    light.color[1] * power * 0.25,
                    light.color[2] * power * 0.25,
                ],
                range: 6.0 * power.sqrt().max(0.5),
            }
        })
        .collect()
}

/// Opening a file from the Asset Browser: scenes load, prefabs open for editing,
/// anything else is picked. `rel` is relative to the project root.
fn open_content_file(rel: &str) -> PanelEvent {
    if rel.is_empty() {
        PanelEvent::None
    } else {
        command("content.open_file", &[("path", rel)])
    }
}
