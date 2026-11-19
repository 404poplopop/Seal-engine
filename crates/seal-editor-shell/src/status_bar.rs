//! Bottom status chrome: drawer triggers, command line, and icon items.

use std::time::{Duration, Instant};

use egui::{self, Response, Sense};
use seal_editor_core::CommandArgs;
use seal_ui::crisp::CrispCombo as _;
use seal_ui::{paint_icon, Icon, ICON_PX};

use crate::{paint_panel, Shell};

const DRAWER_FRAC: f32 = 0.45;
const CMD_INPUT_W: f32 = 220.0;

impl Shell {
    pub(crate) fn status_bar(&mut self, ui: &mut egui::Ui) {
        if self.session.status_snapshot().hidden {
            return;
        }
        let visuals = ui.visuals().clone();
        let bar = egui::Frame::new()
            .fill(visuals.faint_bg_color)
            .inner_margin(egui::Margin::symmetric(6, 2));
        let shown = egui::Panel::bottom("status").frame(bar).show(ui, |ui| {
            ui.set_min_height(seal_ui::STATUS_PX);
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
            ui.spacing_mut().interact_size.y = seal_ui::ROW_COMPACT_PX;
            let width = ui.available_width();
            ui.set_max_width(width);
            ui.horizontal(|ui| {
                ui.set_max_width(width);
                self.status_bar_left(ui);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.status_bar_right(ui);
                });
            });
        });
        self.status_bar_top = Some(shown.response.rect.top());
    }

    fn status_bar_left(&mut self, ui: &mut egui::Ui) {
        if let Some((label, paused)) = play_status_chip(&self.session) {
            let dot = if paused {
                egui::Color32::from_rgb(232, 168, 56)
            } else {
                ui.visuals().warn_fg_color
            };
            labeled_status_chip(ui, Icon::Play, &label, dot);
            ui.add_space(6.0);
        }
        let content = self.session.messages.get("menu-asset-drawer");
        let output = self.session.messages.get("menu-console-log");
        let shortcut = self.session.keymap.shortcut("view.toggle_asset_drawer");
        let pick_content = drawer_button(ui, Icon::ViewList, &content, &shortcut)
            .on_hover_text(shortcut)
            .clicked();
        let pick_output = drawer_button(ui, Icon::File, &output, "").clicked();
        if pick_content {
            self.run_command("view.toggle_asset_drawer");
        }
        if pick_output {
            self.run_command("view.toggle_output_drawer");
        }
        ui.add_space(6.0);
        if self.session.models.viewport_tool == "measure" {
            if let Some(readout) = self.session.draft("measure-status") {
                if !readout.is_empty() {
                    ui.label(egui::RichText::new(readout).small());
                    ui.separator();
                }
            }
        }
        self.status_command_row(ui);
    }

    fn status_command_row(&mut self, ui: &mut egui::Ui) {
        let lang = self.session.messages.get("status-cmd-lang-seal");
        let lang_py = self.session.messages.get("status-cmd-lang-python");
        let cmd_label = self.session.messages.get("status-cmd-label");
        let hint = self.session.messages.get("status-cmd-input-hint");
        egui::ComboBox::from_id_salt("status-cmd-lang")
            .selected_text(&lang)
            .width(72.0)
            .show_crisp(ui, |ui| {
                let _ = ui.selectable_label(true, &lang);
                let _ = ui.selectable_label(false, &lang_py);
            });
        ui.label(egui::RichText::new(cmd_label).weak());
        let mut line = self.session.draft("status-cmd-line").unwrap_or_default();
        let response = seal_ui::crisp::well_edit(
            ui,
            egui::TextEdit::singleline(&mut line).desired_width(CMD_INPUT_W).hint_text(hint),
        );
        if response.changed() {
            let _ = self.session.dispatch(
                "draft.set",
                &CommandArgs::new(&[("key", "status-cmd-line"), ("value", line.as_str())]),
            );
        }
        let ctx = ui.ctx().clone();
        // A single-line field gives up focus on Enter, so check the frame it lost it.
        if (response.has_focus() || response.lost_focus())
            && ui.input(|input| input.key_pressed(egui::Key::Enter))
        {
            self.commit_status_command(&line);
            response.request_focus();
        }
        if response.has_focus() {
            if ui.input(|input| input.key_pressed(egui::Key::ArrowUp)) {
                self.status_cmd_history_back(&mut line);
            }
            if ui.input(|input| input.key_pressed(egui::Key::ArrowDown)) {
                self.status_cmd_history_forward(&mut line);
            }
            if !line.is_empty() {
                let hits: Vec<(String, String)> = self
                    .session
                    .palette_hits()
                    .into_iter()
                    .filter(|hit| {
                        hit.title.to_lowercase().contains(&line.to_lowercase())
                            || hit.id.to_lowercase().contains(&line.to_lowercase())
                    })
                    .take(6)
                    .map(|hit| (hit.id, hit.title))
                    .collect();
                if !hits.is_empty() {
                    egui::Popup::from_response(&response).show(|ui| {
                        for (id, title) in hits {
                            if ui.selectable_label(false, format!("{title} ({id})")).clicked() {
                                let _ = self.session.dispatch(
                                    "draft.set",
                                    &CommandArgs::new(&[
                                        ("key", "status-cmd-line"),
                                        ("value", id.as_str()),
                                    ]),
                                );
                                ui.close();
                            }
                        }
                    });
                }
            }
        }
        if let Some((until, ok)) = self.status_cmd_flash {
            if Instant::now() < until {
                let key = if ok { "status-cmd-done" } else { "status-cmd-failed" };
                let color =
                    if ok { ui.visuals().warn_fg_color } else { ui.visuals().error_fg_color };
                ui.label(egui::RichText::new(self.session.messages.get(key)).color(color).small());
            } else {
                self.status_cmd_flash = None;
            }
        }
        if response.has_focus() {
            ctx.request_repaint();
        }
    }

    fn commit_status_command(&mut self, line: &str) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return;
        }
        if !self.status_cmd_history.first().is_some_and(|prev| prev == trimmed) {
            self.status_cmd_history.insert(0, trimmed.to_string());
            self.status_cmd_history.truncate(32);
        }
        self.status_cmd_history_pos = 0;
        let ok =
            self.session.dispatch("console.run", &CommandArgs::new(&[("line", trimmed)])).is_ok();
        self.status_cmd_flash = Some((Instant::now() + Duration::from_millis(1200), ok));
        let _ = self
            .session
            .dispatch("draft.set", &CommandArgs::new(&[("key", "status-cmd-line"), ("value", "")]));
    }

    fn status_cmd_history_back(&mut self, line: &mut String) {
        if self.status_cmd_history.is_empty() {
            return;
        }
        if self.status_cmd_history_pos < self.status_cmd_history.len() {
            self.status_cmd_history_pos += 1;
        }
        if let Some(picked) =
            self.status_cmd_history.get(self.status_cmd_history_pos.saturating_sub(1))
        {
            *line = picked.clone();
        }
    }

    fn status_cmd_history_forward(&mut self, line: &mut String) {
        if self.status_cmd_history_pos == 0 {
            return;
        }
        self.status_cmd_history_pos -= 1;
        if self.status_cmd_history_pos == 0 {
            line.clear();
        } else if let Some(picked) =
            self.status_cmd_history.get(self.status_cmd_history_pos.saturating_sub(1))
        {
            *line = picked.clone();
        }
    }

    fn status_bar_right(&mut self, ui: &mut egui::Ui) {
        let revision = self.session.messages.get("status-revision-control");
        let rev_resp = labeled_status_chip(ui, Icon::EditLink, &revision, dot_gray(ui));
        let menu_connect = self.session.messages.get("status-menu-connect");
        let menu_submit = self.session.messages.get("status-menu-submit");
        let menu_sync = self.session.messages.get("status-menu-sync");
        let menu_view = self.session.messages.get("status-menu-view-changes");
        egui::Popup::from_toggle_button_response(&rev_resp).show(|ui| {
            let _ = ui.button(menu_connect);
            let _ = ui.button(menu_submit);
            let _ = ui.button(menu_sync);
            let _ = ui.button(menu_view);
        });

        let dirty = self.session.dirty_count();
        let autosaved = self.session.project.is_none();
        let saved_label = if dirty == 0 && autosaved {
            self.session.messages.get("status-autosaved")
        } else if dirty == 0 {
            self.session.messages.get("status-all-saved")
        } else {
            self.session.messages.format("status-n-unsaved", &[("count", &dirty.to_string())])
        };
        let choose_save = match (autosaved, self.session.scene_path()) {
            (true, Some(path)) => self
                .session
                .messages
                .format("status-autosaved-to", &[("path", &path.display().to_string())]),
            _ => self.session.messages.get("status-choose-save"),
        };
        let dot = if dirty == 0 { seal_ui::STATUS_OK } else { ui.visuals().warn_fg_color };
        let saved_resp =
            labeled_status_chip(ui, Icon::FileSave, &saved_label, dot).on_hover_text(choose_save);
        if saved_resp.clicked() && dirty > 0 {
            self.run_command("file.save_all");
        }

        let ready = self.session.messages.get("status-compile-ready");
        let ready_resp = icon_only_button(ui, Icon::StatusOk).on_hover_text(&ready);
        egui::Popup::from_toggle_button_response(&ready_resp).show(|ui| {
            ui.label(ready);
        });

        let tasks = self.session.foundation.tasks.clone();
        let cancel_label = self.session.messages.get("import-job-cancel");
        for task in tasks.iter().filter(|task| !task.cancelled && task.progress < 1.0) {
            ui.add(
                egui::ProgressBar::new(task.progress)
                    .desired_width(140.0)
                    .show_percentage()
                    .text(&task.title),
            );
            if ui.button(&cancel_label).clicked() {
                self.session.cancel_task(&task.id);
            }
        }
        if !tasks.is_empty() {
            let task_label = tasks.len().to_string();
            let task_resp = icon_only_button(ui, Icon::ViewRefresh).on_hover_text(&task_label);
            let title = self.session.messages.get("status-tasks-title");
            egui::Popup::from_toggle_button_response(&task_resp).show(|ui| {
                ui.heading(title);
                for task in &tasks {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(&task.title);
                        ui.weak(format!("{:.0}%", task.progress * 100.0));
                    });
                }
            });
        }

        let trace = self.session.messages.get("status-trace");
        let trace_resp = icon_only_button(ui, Icon::ViewRefresh).on_hover_text(&trace);
        let trace_log = self.session.messages.get("status-trace-log");
        let trace_stats = self.session.messages.get("status-trace-stats");
        let mut jump_latest = false;
        let mut reveal_stats = false;
        egui::Popup::from_toggle_button_response(&trace_resp).show(|ui| {
            if ui.button(trace_log).clicked() {
                jump_latest = true;
            }
            if ui.button(trace_stats).clicked() {
                reveal_stats = true;
            }
        });
        if jump_latest {
            self.run_command("console.jump_latest");
        }
        if reveal_stats {
            self.session.reveal_panel = Some("stats".into());
        }

        let diag = self.session.messages.get("status-diagnostics");
        let diag_resp = icon_only_button(ui, Icon::StatusBell).on_hover_text(&diag);
        let (errors, warnings) =
            seal_editor_core::console_level_counts(self.session.foundation.console_clear_before);
        let err_line = self
            .session
            .messages
            .format("status-console-errors", &[("count", &errors.to_string())]);
        let warn_line = self
            .session
            .messages
            .format("status-console-warnings", &[("count", &warnings.to_string())]);
        let copy_diag = self.session.messages.get("command-app-diagnostics");
        let mut copy = false;
        egui::Popup::from_toggle_button_response(&diag_resp).show(|ui| {
            ui.label(diag);
            ui.label(err_line);
            ui.label(warn_line);
            if ui.button(copy_diag).clicked() {
                copy = true;
            }
        });
        if copy {
            self.run_command("app.copy_diagnostics");
        }

        let live = self.session.messages.get("status-agent-live");
        let activity = self.session.foundation.agent_activity.clone();
        let live_resp = icon_only_button(ui, Icon::ViewColumns).on_hover_text(&live);
        egui::Popup::from_toggle_button_response(&live_resp).show(|ui| {
            if activity.is_empty() {
                ui.weak(live);
            } else {
                ui.label(activity);
            }
        });

        if self.save_fault.is_some() {
            self.save_status_item(ui);
        }
    }

    pub(crate) fn close_drawers_on_escape(&mut self, ctx: &egui::Context) {
        if !ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            return;
        }
        self.session.foundation.asset_drawer_open = false;
        self.session.foundation.output_drawer_open = false;
    }

    pub(crate) fn show_drawers(&mut self, ctx: &egui::Context) {
        let content_open = self.session.foundation.asset_drawer_open;
        let output_open = self.session.foundation.output_drawer_open;
        if !content_open && !output_open {
            return;
        }
        let content = ctx.content_rect();
        // The drawer ends above the status bar, so the button that opened it stays clickable.
        let bottom = self.status_bar_top.unwrap_or(content.bottom()).min(content.bottom());
        let panel_h = (bottom - content.top()) * DRAWER_FRAC;
        let top_y = bottom - panel_h;
        let panel_rect = egui::Rect::from_min_max(
            egui::pos2(content.left(), top_y),
            egui::pos2(content.right(), bottom),
        );
        let backdrop =
            egui::Rect::from_min_max(content.left_top(), egui::pos2(content.right(), top_y));
        // No backdrop layer: one would sit over the dock and swallow drags dropped
        // from the drawer onto the viewport or Outliner. A press above the drawer
        // closes it instead.
        let pressed_outside = ctx.input(|i| {
            i.pointer.any_pressed()
                && i.pointer.interact_pos().is_some_and(|p| backdrop.contains(p))
        }) && !over_floating_layer(ctx);
        if pressed_outside {
            self.session.foundation.asset_drawer_open = false;
            self.session.foundation.output_drawer_open = false;
        }
        let panel_id = if content_open { "content" } else { "console" };
        let title = if content_open {
            self.session.messages.get("menu-asset-drawer")
        } else {
            self.session.messages.get("menu-console-log")
        };
        let dock_label = self.session.messages.get("status-dock-in-layout");
        let mut dock = false;
        egui::Area::new(egui::Id::new("drawer-panel"))
            .order(egui::Order::Foreground)
            .fixed_pos(panel_rect.min)
            .show(ctx, |ui| {
                // Fixed size: a long Console list scrolls inside instead of growing over the window.
                ui.set_min_size(panel_rect.size());
                ui.set_max_size(panel_rect.size());
                ui.set_clip_rect(panel_rect);
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.heading(title);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if content_open && ui.button(dock_label).clicked() {
                                        dock = true;
                                    }
                                },
                            );
                        });
                        ui.separator();
                        let mut deferred = Vec::new();
                        paint_panel(
                            &mut self.session,
                            &self.registry,
                            &mut self.panel_faults,
                            ui,
                            panel_id,
                            &mut deferred,
                        );
                        for (command, args) in deferred {
                            if let Err(err) = self.session.dispatch(&command, &args) {
                                self.session.notify(err.to_string(), true);
                            }
                        }
                    });
            });
        if dock {
            self.run_command("view.dock_asset_drawer");
        }
    }
}

/// True when the pointer is over a menu, popup or other floating layer, which may
/// reach above the drawer without being "outside" it.
fn over_floating_layer(ctx: &egui::Context) -> bool {
    let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) else {
        return false;
    };
    ctx.layer_id_at(pos)
        .is_some_and(|layer| !matches!(layer.order, egui::Order::Background | egui::Order::Middle))
}

fn drawer_button(ui: &mut egui::Ui, icon: Icon, label: &str, _shortcut: &str) -> Response {
    let slot = ui.painter().add(egui::Shape::Noop);
    let inner = ui.horizontal(|ui| {
        ui.add_space(4.0);
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ICON_PX + 2.0, ICON_PX + 2.0), Sense::hover());
        if ui.is_rect_visible(rect) {
            paint_icon(ui.painter(), rect.center(), icon, ICON_PX, ui.visuals().text_color());
        }
        ui.label(egui::RichText::new(label).small());
        ui.add_space(4.0);
    });
    clickable_chip(ui, inner.response.rect, slot, label)
}

/// Make a laid-out status item clickable, with the flat crisp plate behind it.
fn clickable_chip(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    slot: egui::layers::ShapeIdx,
    label: &str,
) -> Response {
    let response = ui.interact(rect, ui.id().with(("status-chip", label)), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    if response.hovered() || response.is_pointer_button_down_on() {
        let mut shapes = Vec::new();
        let state = seal_ui::crisp::state_of(&response, false);
        seal_ui::crisp::raised(&mut shapes, rect, ui.visuals(), state);
        seal_ui::crisp::place_behind(ui.painter(), slot, shapes);
    }
    response
}

fn icon_only_button(ui: &mut egui::Ui, icon: Icon) -> Response {
    let size = egui::vec2(22.0, 20.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        paint_icon(ui.painter(), rect.center(), icon, ICON_PX - 2.0, ui.visuals().text_color());
    }
    response
}

fn play_status_chip(session: &seal_editor_core::EditorSession) -> Option<(String, bool)> {
    match session.world.mode {
        seal_editor_core::PlayMode::Playing => {
            Some((session.messages.get("status-play-live"), false))
        }
        seal_editor_core::PlayMode::Paused => {
            Some((session.messages.get("status-play-paused-chip"), true))
        }
        seal_editor_core::PlayMode::Editing => None,
    }
}

fn dot_gray(ui: &egui::Ui) -> egui::Color32 {
    ui.visuals().weak_text_color()
}

fn labeled_status_chip(ui: &mut egui::Ui, icon: Icon, label: &str, dot: egui::Color32) -> Response {
    let slot = ui.painter().add(egui::Shape::Noop);
    let inner = ui.horizontal(|ui| {
        let (dot_rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 18.0), Sense::hover());
        ui.painter().circle_filled(dot_rect.center(), 3.0, dot);
        let (icon_rect, _) =
            ui.allocate_exact_size(egui::vec2(ICON_PX + 4.0, 18.0), Sense::hover());
        if ui.is_rect_visible(icon_rect) {
            paint_icon(
                ui.painter(),
                icon_rect.center(),
                icon,
                ICON_PX - 2.0,
                ui.visuals().text_color(),
            );
        }
        ui.label(egui::RichText::new(label).small());
        ui.add_space(4.0);
    });
    clickable_chip(ui, inner.response.rect, slot, label)
}
