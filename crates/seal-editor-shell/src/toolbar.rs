//! Main toolbar under the document tabs.

use seal_editor_core::EditorSession;
use seal_ui::{look_tooltip, paint_icon, Icon, PanelFrame, ICON_PX, TOOL_BUTTON_PX};

const MODE_KEY: &str = "toolbar.mode";

#[derive(Debug)]
pub enum ToolbarPending {
    Command(String),
    Args { id: String, args: Vec<(String, String)> },
}

pub struct ToolbarActions<'a> {
    pub layout_dirty: bool,
    pub pending: &'a mut Vec<ToolbarPending>,
}

fn queue_command(actions: &mut ToolbarActions<'_>, id: &str) {
    actions.pending.push(ToolbarPending::Command(id.to_string()));
}

fn queue_args(actions: &mut ToolbarActions<'_>, id: &str, args: &[(&str, &str)]) {
    actions.pending.push(ToolbarPending::Args {
        id: id.to_string(),
        args: args.iter().map(|(key, value)| (key.to_string(), value.to_string())).collect(),
    });
}

pub fn show(
    ui: &mut egui::Ui,
    width: f32,
    session: &mut EditorSession,
    actions: &mut ToolbarActions<'_>,
) {
    ui.set_max_width(width);
    ui.horizontal(|ui| {
        ui.set_max_width(width);
        left_groups(ui, session, actions);
        let rest = ui.available_width().max(0.0);
        let height = ui.available_height().max(seal_ui::TOOLBAR_PX);
        ui.allocate_ui_with_layout(
            egui::vec2(rest, height),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                if width < 920.0 {
                    overflow_menu(ui, session, actions);
                } else {
                    right_groups(ui, session, actions);
                }
                let mid = ui.available_width().max(0.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(mid, height),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| play_strip(ui, session, actions, mid),
                );
            },
        );
    });
}

fn left_groups(ui: &mut egui::Ui, session: &mut EditorSession, actions: &mut ToolbarActions<'_>) {
    save_icon(ui, session, actions);
    icon_action(
        ui,
        Icon::FolderOpen,
        true,
        &session.messages.get("command-toolbar-browse-scene"),
        "",
        &session.messages.get("command-toolbar-browse-scene"),
        || queue_args(actions, "view.toggle_panel", &[("id", "content")]),
    );
    toolbar_sep(ui);
    mode_combo(ui, session);
    toolbar_sep(ui);
    add_menu(ui, session, actions);
    script_menu(ui, session, actions);
    cinematics_menu(ui, session);
}

fn right_groups(ui: &mut egui::Ui, session: &mut EditorSession, actions: &mut ToolbarActions<'_>) {
    settings_menu(ui, session, actions);
    platforms_menu(ui, session, actions);
}

fn toolbar_sep(ui: &mut egui::Ui) {
    let h = ui.available_height().max(20.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, h - 6.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, ui.visuals().widgets.noninteractive.bg_stroke.color);
    ui.add_space(4.0);
}

fn save_icon(ui: &mut egui::Ui, session: &EditorSession, actions: &mut ToolbarActions<'_>) {
    let enabled = session.toolbar_save_enabled(actions.layout_dirty);
    let (name, shortcut) = session.command_label("file.save");
    let detail = session.save_tooltip_detail(actions.layout_dirty);
    let response = icon_control(ui, Icon::FileSave, enabled);
    let response = look_tooltip(ui, response, &name, &shortcut, &detail);
    if enabled && response.clicked() {
        queue_command(actions, "file.save");
    }
}

fn icon_control(ui: &mut egui::Ui, icon: Icon, enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::click());
    if enabled {
        seal_ui::crisp::plate(ui, rect, &response, false, true);
    } else {
        seal_ui::crisp::tray(&mut ui.painter(), rect, ui.visuals());
    }
    let ink = if enabled { ui.visuals().text_color() } else { ui.visuals().weak_text_color() };
    paint_icon(ui.painter(), rect.center(), icon, ICON_PX, ink);
    if enabled {
        response
    } else {
        response.on_hover_cursor(egui::CursorIcon::NotAllowed)
    }
}

fn icon_action(
    ui: &mut egui::Ui,
    icon: Icon,
    enabled: bool,
    name: &str,
    shortcut: &str,
    detail: &str,
    on_click: impl FnOnce(),
) {
    let response = icon_control(ui, icon, enabled);
    let response = look_tooltip(ui, response, name, shortcut, detail);
    if enabled && response.clicked() {
        on_click();
    }
}

fn current_mode(session: &EditorSession) -> String {
    session.draft(MODE_KEY).unwrap_or_else(|| "select".into())
}

fn set_mode(session: &mut EditorSession, mode: &str) {
    if session.set_draft(MODE_KEY, mode).is_err() {
        return;
    }
    if mode == "select" {
        // Select mode ends sprite painting.
        let _ = session.set_draft("place-brush", "");
    }
    session.reveal_panel = Some("place".into());
    session.foundation.focused_panel_id = "place".into();
}

fn mode_combo(ui: &mut egui::Ui, session: &mut EditorSession) {
    let modes = [
        ("select", "toolbar-mode-select", Icon::ToolSelect, "Shift+1"),
        ("terrain", "toolbar-mode-terrain", Icon::EntityTerrain, "Shift+2"),
        ("foliage", "toolbar-mode-foliage", Icon::EntityFog, "Shift+3"),
        ("mesh_paint", "toolbar-mode-mesh-paint", Icon::ToolMeasure, "Shift+4"),
        ("modeling", "toolbar-mode-modeling", Icon::ToolTransform, "Shift+5"),
        ("fracture", "toolbar-mode-fracture", Icon::ViewClose, "Shift+6"),
        ("brush", "toolbar-mode-brush", Icon::ToolSnapSurface, "Shift+7"),
        ("animation", "toolbar-mode-animation", Icon::AssetAnimation, "Shift+8"),
    ];
    let active = current_mode(session);
    let active_title = modes
        .iter()
        .find(|(id, _, _, _)| *id == active)
        .map(|(_, key, _, _)| session.messages.get(key))
        .unwrap_or_else(|| session.messages.get("toolbar-mode-select"));
    // The button names the active mode so a mode change is visible at a glance.
    let label = format!("{active_title} ⏷");
    seal_ui::crisp::menu_button(
        ui,
        egui::Button::new(egui::RichText::new(label).small()),
        true,
        |ui| {
            for (id, key, icon, shortcut) in modes {
                let title = session.messages.get(key);
                let _ = icon;
                let _ = shortcut;
                if ui.button(title).clicked() {
                    if matches!(
                        id,
                        "terrain" | "foliage" | "mesh_paint" | "fracture" | "brush" | "animation"
                    ) {
                        // Nothing to switch to yet: say so and stay in the current mode.
                        session.notice = Some(session.messages.get("toolbar-mode-unavailable"));
                    } else {
                        set_mode(session, id);
                    }
                    ui.close();
                }
            }
        },
    )
    .on_hover_text(format!("{active_title} — {}", session.messages.get("toolbar-mode-label")));
}

fn add_menu(ui: &mut egui::Ui, session: &EditorSession, actions: &mut ToolbarActions<'_>) {
    let tip = session.messages.get("toolbar-add-menu");
    PanelFrame::new(ui).icon_menu(Icon::ViewPlus, &tip, |ui| {
        ui.add_space(2.0);
        ui.label(egui::RichText::new(session.messages.get("toolbar-add-recent")).small().weak());
        ui.separator();
        ui.menu_button(session.messages.get("toolbar-add-shapes"), |ui| {
            for kind in ["cube", "sphere", "cylinder", "cone", "capsule", "plane"] {
                let title = session.messages.get(&format!("place-{kind}"));
                if ui.button(title).clicked() {
                    queue_args(actions, "scene.add", &[("kind", kind)]);
                    ui.close();
                }
            }
        });
        ui.menu_button(session.messages.get("toolbar-add-basic"), |ui| {
            if ui.button(session.messages.get("place-empty")).clicked() {
                queue_args(actions, "scene.add", &[("kind", "empty")]);
                ui.close();
            }
            if ui.button(session.messages.get("place-character_2d")).clicked() {
                queue_args(actions, "scene.add", &[("kind", "character_2d")]);
                ui.close();
            }
        });
        ui.menu_button(session.messages.get("toolbar-add-areas"), |ui| {
            for kind in [
                "wind_area",
                "water_area",
                "gravity_area",
                "attractor",
                "kill_zone",
                "damage_zone",
                "checkpoint",
                "trigger_volume",
            ] {
                let title = session.messages.get(&format!("place-{kind}"));
                if ui.button(title).clicked() {
                    queue_args(actions, "scene.add", &[("kind", kind)]);
                    ui.close();
                }
            }
        });
        ui.menu_button(session.messages.get("toolbar-add-volumes"), |ui| {
            for kind in ["kill_zone", "damage_zone", "checkpoint", "trigger_volume"] {
                let title = session.messages.get(&format!("place-{kind}"));
                if ui.button(title).clicked() {
                    queue_args(actions, "scene.add", &[("kind", kind)]);
                    ui.close();
                }
            }
        });
        ui.menu_button(session.messages.get("toolbar-add-lights"), |ui| {
            if ui.button(session.messages.get("place-light")).clicked() {
                queue_args(actions, "scene.add", &[("kind", "light")]);
                ui.close();
            }
        });
        for key in ["toolbar-add-cinematic", "toolbar-add-vfx", "toolbar-add-all-classes"] {
            ui.menu_button(session.messages.get(key), |ui| {
                ui.label(session.messages.get("toolbar-mode-unavailable"));
            });
        }
        if ui.button(session.messages.get("toolbar-add-place-panel")).clicked() {
            queue_args(actions, "view.toggle_panel", &[("id", "place")]);
            ui.close();
        }
    });
}

fn script_menu(ui: &mut egui::Ui, session: &EditorSession, actions: &mut ToolbarActions<'_>) {
    let tip = session.messages.get("toolbar-script-menu");
    let has_project = session.project.is_some();
    PanelFrame::new(ui).icon_menu(Icon::AssetScript, &tip, |ui| {
        for (key, command) in
            [("toolbar-script-open", "graph.open"), ("toolbar-script-new", "graph.new")]
        {
            let title = session.messages.get(key);
            if ui.add_enabled(has_project, egui::Button::new(title)).clicked() {
                queue_args(actions, command, &[]);
                ui.close();
            }
        }
        if ui.button(session.messages.get("toolbar-script-panel")).clicked() {
            queue_args(actions, "view.toggle_panel", &[("id", "graph")]);
            ui.close();
        }
    });
}

fn cinematics_menu(ui: &mut egui::Ui, session: &EditorSession) {
    let tip = session.messages.get("toolbar-cinematics-menu");
    PanelFrame::new(ui).icon_menu(Icon::Record, &tip, |ui| {
        for key in ["toolbar-cinematics-new", "toolbar-cinematics-open"] {
            let title = session.messages.get(key);
            let _ = ui.add_enabled(false, egui::Button::new(title));
        }
        ui.label(session.messages.get("toolbar-mode-unavailable"));
    });
}

fn platforms_menu(ui: &mut egui::Ui, session: &EditorSession, actions: &mut ToolbarActions<'_>) {
    let label = session.messages.get("toolbar-platforms");
    let has_project = session.project.is_some();
    seal_ui::crisp::menu_button(ui, egui::Button::new(format!("{label} ⏷")), true, |ui| {
        let entries = [
            ("toolbar-platforms-package", "project.export"),
            ("toolbar-platforms-cook", "project.export_go"),
            ("toolbar-platforms-quick", "project.export_run"),
        ];
        for (key, command) in entries {
            let title = session.messages.get(key);
            if ui.add_enabled(has_project, egui::Button::new(title)).clicked() {
                queue_command(actions, command);
                ui.close();
            }
        }
    });
}

fn settings_menu(ui: &mut egui::Ui, session: &mut EditorSession, actions: &mut ToolbarActions<'_>) {
    let label = session.messages.get("toolbar-settings-menu");
    seal_ui::crisp::menu_button(ui, egui::Button::new(format!("{label} ⏷")), true, |ui| {
        if ui.button(session.messages.get("toolbar-settings-world")).clicked() {
            queue_args(actions, "view.toggle_panel", &[("id", "world")]);
            ui.close();
        }
        if ui.button(session.messages.get("toolbar-settings-project")).clicked() {
            queue_command(actions, "settings.open");
            ui.close();
        }
        if ui.button(session.messages.get("toolbar-settings-preferences")).clicked() {
            queue_command(actions, "settings.open");
            ui.close();
        }
        let _ = ui.button(session.messages.get("toolbar-settings-plugins"));
        ui.separator();
        for (key, level) in [
            ("toolbar-settings-quality-low", "low"),
            ("toolbar-settings-quality-medium", "medium"),
            ("toolbar-settings-quality-high", "high"),
            ("toolbar-settings-quality-ultra", "ultra"),
        ] {
            if ui.button(session.messages.get(key)).clicked() {
                queue_args(actions, "settings.quality", &[("level", level)]);
                ui.close();
            }
        }
        let preview = session
            .models
            .settings
            .iter()
            .find(|row| row.id == seal_editor_core::QUALITY_LEVEL)
            .map(|row| row.value().to_string())
            .unwrap_or_else(|| "high".into());
        let preview_label = session
            .messages
            .format("toolbar-settings-preview-quality", &[("level", preview.as_str())]);
        if ui.button(preview_label).clicked() {
            queue_command(actions, "settings.open");
            ui.close();
        }
    });
}

fn overflow_menu(ui: &mut egui::Ui, session: &mut EditorSession, actions: &mut ToolbarActions<'_>) {
    let tip = session.messages.get("toolbar-overflow");
    PanelFrame::new(ui).icon_menu(Icon::EditMore, &tip, |ui| {
        if ui.button(session.messages.get("toolbar-platforms")).clicked() {
            ui.label(session.messages.get("toolbar-platforms-disabled"));
        }
        if ui.button(session.messages.get("toolbar-settings-menu")).clicked() {
            queue_command(actions, "settings.open");
        }
    });
}

fn play_strip(
    ui: &mut egui::Ui,
    session: &mut EditorSession,
    actions: &mut ToolbarActions<'_>,
    mid: f32,
) {
    let mode = session.world.mode;
    let playing = mode == seal_editor_core::PlayMode::Playing;
    let paused = mode == seal_editor_core::PlayMode::Paused;
    let strip = egui::Frame::new().inner_margin(egui::Margin::symmetric(5, 3));
    let group = TOOL_BUTTON_PX * 4.5 + 48.0;
    let pad = ((mid - group) * 0.5).max(0.0);
    if pad > 0.0 {
        ui.add_space(pad);
    }
    let tray_slot = ui.painter().add(egui::Shape::Noop);
    let tray = strip.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            let size = egui::vec2(TOOL_BUTTON_PX, TOOL_BUTTON_PX);
            // The same five slots in every mode, so Stop never moves under the pointer.
            let green = Some(egui::Color32::from_rgb(63, 185, 80));
            let red = Some(egui::Color32::from_rgb(224, 68, 62));
            let live = playing || paused;
            play_icon(ui, session, actions, Icon::Play, "play.start", size, green, !playing);
            play_icon(ui, session, actions, Icon::Pause, "play.pause", size, None, playing);
            play_icon(
                ui,
                session,
                actions,
                Icon::Stop,
                "play.stop",
                size,
                if live { red } else { None },
                live,
            );
            play_icon(ui, session, actions, Icon::Step, "play.step", size, None, live);
            play_more(ui, session, actions);
        });
    });
    let mut shapes = Vec::new();
    seal_ui::crisp::tray(&mut shapes, tray.response.rect, ui.visuals());
    seal_ui::crisp::place_behind(ui.painter(), tray_slot, shapes);
}

fn play_tip(session: &EditorSession, id: &str) -> String {
    match id {
        "play.start" => session.messages.get("tooltip-play-start"),
        "play.pause" => session.messages.get("tooltip-play-pause"),
        "play.stop" => session.messages.get("tooltip-play-stop"),
        "play.step" => session.messages.get("tooltip-play-step"),
        _ => String::new(),
    }
}

fn play_icon(
    ui: &mut egui::Ui,
    session: &EditorSession,
    actions: &mut ToolbarActions<'_>,
    icon: Icon,
    id: &str,
    size: egui::Vec2,
    tint: Option<egui::Color32>,
    enabled: bool,
) -> bool {
    let (name, shortcut) = session.command_label(id);
    let detail =
        if enabled { play_tip(session, id) } else { session.messages.get("tooltip-play-disabled") };
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    if enabled {
        seal_ui::crisp::plate(ui, rect, &response, false, true);
        // The first live control carries the run LED: lit green while the game runs.
        // Pause glows green while running; Play glows amber while paused.
        let lamp = match (id, session.world.mode) {
            ("play.pause", seal_editor_core::PlayMode::Playing) => {
                Some(egui::Color32::from_rgb(63, 185, 80))
            }
            ("play.start", seal_editor_core::PlayMode::Paused) => {
                Some(egui::Color32::from_rgb(232, 168, 56))
            }
            ("play.start", _) => None,
            _ => Some(egui::Color32::TRANSPARENT),
        };
        if id == "play.start" || id == "play.pause" {
            let running = lamp.is_some_and(|c| c != egui::Color32::TRANSPARENT);
            let lamp_color = lamp.unwrap_or(egui::Color32::from_rgb(63, 185, 80));
            let slot = egui::Rect::from_min_max(
                egui::pos2(rect.left() + 5.0, rect.top() + 2.0),
                egui::pos2(rect.right() - 5.0, rect.top() + 5.0),
            );
            seal_ui::crisp::led_slot(&mut ui.painter(), slot, running, lamp_color);
        }
    } else {
        seal_ui::crisp::tray(&mut ui.painter(), rect, ui.visuals());
    }
    let ink = tint.unwrap_or_else(|| {
        if enabled {
            ui.visuals().text_color()
        } else {
            ui.visuals().weak_text_color()
        }
    });
    paint_icon(ui.painter(), rect.center(), icon, ICON_PX, ink);
    let response =
        if enabled { response } else { response.on_hover_cursor(egui::CursorIcon::NotAllowed) };
    let response = look_tooltip(ui, response, &name, &shortcut, &detail);
    if enabled && response.clicked() {
        queue_command(actions, id);
        return true;
    }
    false
}

fn play_more(ui: &mut egui::Ui, session: &EditorSession, actions: &mut ToolbarActions<'_>) {
    let tip = session.messages.get("toolbar-play-more");
    let can_play = session.project.is_some();
    PanelFrame::new(ui).icon_menu(Icon::EditMore, &tip, |ui| {
        if ui
            .add_enabled(can_play, egui::Button::new(session.messages.get("toolbar-play-window")))
            .clicked()
        {
            queue_args(actions, "play.start", &[("window", "1")]);
            ui.close();
        }
        for key in [
            "toolbar-play-viewport",
            "toolbar-play-simulate",
            "toolbar-play-from-here",
            "toolbar-play-players",
            "toolbar-play-settings",
        ] {
            let _ = ui.button(session.messages.get(key));
        }
    });
}
