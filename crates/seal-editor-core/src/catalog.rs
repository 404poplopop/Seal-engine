use std::fs;
use std::path::{Path, PathBuf};

use seal_sealtext::parse;

use crate::command::{always_enabled, CommandArgs, CommandSpec};
use crate::diagnostics::redact_paths;
use crate::error::CoreError;
use crate::foundation::{DockSide, ModalKind};
use crate::layout::Layout;
use crate::models::{
    clamp_scale, collect_log_lines, format_log_line, play_step_frame, LogLine, CONSOLE_ROW_DRAFT,
};
use crate::persist::atomic_write;
use crate::project::{self, remove_recent};
use crate::session::EditorSession;

/// Every command from the shell command list. Existing ids stay; newer ids are aliases or new commands.
pub fn register_foundation(session: &mut EditorSession) -> Result<(), CoreError> {
    let aliases = [
        ("app.quit", "command-file-quit", "file", "file.quit"),
        ("app.about", "command-help-about", "help", "help.about"),
        ("file.new_project", "command-file-new-project", "file", "file.new-project"),
        ("file.open_project", "command-file-open-project", "file", "file.open-project"),
        ("file.close_project", "command-file-close-project", "file", "file.close-project"),
        ("view.reset_layout", "command-window-reset-layout", "view", "window.reset-layout"),
        ("palette.open", "command-view-palette", "view", "view.command-palette"),
    ];
    for (id, title, category, target) in aliases {
        session.commands.register_alias(CommandSpec::new(id, title, category), target)?;
    }
    let actions: &[(&str, &str, &str, CommandFn)] = &[
        ("app.restart", "command-app-restart", "file", restart),
        ("app.reset_state", "command-app-reset", "file", reset_state),
        ("app.open_user_data_folder", "command-app-user-data", "file", open_user_data),
        ("app.copy_diagnostics", "command-app-diagnostics", "help", copy_diagnostics),
        ("app.toggle_fullscreen", "command-app-fullscreen", "view", toggle_fullscreen),
        ("app.safe_mode_restart", "command-app-safe-restart", "file", safe_restart),
        ("file.open_recent", "command-file-open-recent", "file", open_recent),
        ("file.remove_recent", "command-file-remove-recent", "file", remove_recent_cmd),
        ("file.save", "command-file-save", "file", save_file),
        ("file.save_as", "command-file-save-as", "file", save_as),
        ("file.save_all", "command-file-save-all", "file", save_all),
        ("file.revert", "command-file-revert", "file", revert_file),
        ("file.close_document", "command-file-close-document", "file", close_document),
        ("file.reveal", "command-file-reveal", "file", reveal),
        ("edit.cut", "command-edit-cut", "edit", cut),
        ("edit.copy", "command-edit-copy", "edit", copy),
        ("edit.paste", "command-edit-paste", "edit", paste),
        ("edit.delete", "command-edit-delete", "edit", delete_selection),
        ("edit.duplicate", "command-edit-duplicate", "edit", duplicate),
        ("edit.rename", "command-edit-rename", "edit", rename),
        ("edit.select_all", "command-edit-select-all", "edit", select_all),
        ("edit.select_none", "command-edit-select-none", "edit", select_none),
        ("edit.invert_selection", "command-edit-invert", "edit", invert_selection),
        ("view.toggle_panel", "command-view-toggle-panel", "view", toggle_panel),
        ("view.toggle_asset_drawer", "command-view-asset-drawer", "view", toggle_asset_drawer),
        ("view.toggle_output_drawer", "command-view-output-drawer", "view", toggle_output_drawer),
        ("view.dock_asset_drawer", "command-view-dock-content", "view", dock_asset_drawer),
        ("view.lock_layout", "command-view-lock-layout", "view", lock_layout),
        ("view.maximize_panel", "command-view-maximize", "view", maximize_panel),
        ("view.zoom_in", "command-view-zoom-in", "view", zoom_in),
        ("view.zoom_out", "command-view-zoom-out", "view", zoom_out),
        ("view.zoom_reset", "command-view-zoom-reset", "view", zoom_reset),
        ("view.theme_set", "command-view-theme", "view", theme_set),
        ("layout.save", "command-layout-save", "window", layout_save),
        ("layout.load", "command-layout-load", "window", layout_load),
        ("layout.rename", "command-layout-rename", "window", layout_rename),
        ("layout.delete", "command-layout-delete", "window", layout_delete),
        ("layout.export", "command-layout-export", "window", layout_export),
        ("layout.import", "command-layout-import", "window", layout_import),
        ("layout.undo-reset", "command-layout-undo-reset", "window", undo_reset),
        ("panel.dock_left", "command-panel-dock", "window", dock_left),
        ("panel.dock_right", "command-panel-dock", "window", dock_right),
        ("panel.dock_top", "command-panel-dock", "window", dock_top),
        ("panel.dock_bottom", "command-panel-dock", "window", dock_bottom),
        ("panel.float", "command-panel-float", "window", dock_float),
        ("panel.close", "command-panel-close", "window", dock_close),
        ("panel.reload", "command-panel-reload", "window", dock_reload),
        ("panel.duplicate", "command-panel-duplicate", "window", duplicate_panel),
        ("inspector.lock", "command-inspector-lock", "view", lock_inspector),
        ("inspector.unlock", "command-inspector-unlock", "view", unlock_inspector),
        ("console.clear", "command-console-clear", "view", console_clear),
        ("console.copy_selection", "command-console-copy", "view", console_copy),
        ("console.save", "command-console-save", "view", console_save),
        ("console.jump_latest", "command-console-latest", "view", console_latest),
        ("outliner.expand_all", "command-outliner-expand", "view", expand_all),
        ("outliner.collapse_all", "command-outliner-collapse", "view", collapse_all),
        ("content.new_folder", "command-content-new-folder", "file", new_folder),
        ("content.refresh", "command-content-refresh", "view", refresh_content),
        ("wizard.create", "command-wizard-create", "file", wizard_create),
        ("wizard.browse", "command-wizard-browse", "file", wizard_browse),
        ("wizard.cancel", "command-wizard-cancel", "file", wizard_cancel),
        ("settings.open", "command-settings-open", "tools", open_settings),
        ("settings.open_file", "command-settings-open-file", "tools", open_settings_file),
        ("keymap.open", "command-keymap-open", "tools", open_keymap),
        ("recovery.open", "command-recovery-open", "window", open_recovery),
        ("draft.set", "command-draft-set", "edit", set_draft_cmd),
        ("draft.forget", "command-draft-forget", "edit", forget_draft_cmd),
        ("recovery.restore", "command-recovery-restore", "window", recovery_restore),
        ("recovery.keep_both", "command-recovery-keep", "window", recovery_keep_both),
        ("recovery.discard", "command-recovery-discard", "window", recovery_discard),
        ("play.start", "command-runtime-play", "tools", play_start),
        ("play.pause", "command-runtime-pause", "tools", play_pause),
        ("play.stop", "command-runtime-stop", "tools", play_stop),
        ("play.step", "command-play-step", "tools", play_step_frame),
        ("sequence.play", "command-sequence-play", "tools", crate::sequences::sequence_play),
        ("sequence.scrub", "command-sequence-scrub", "tools", crate::sequences::sequence_scrub),
        ("play.host", "command-play-host", "tools", play_host),
        ("play.join", "command-play-join", "tools", play_join),
        ("project.package", "command-project-package", "file", package_project_cmd),
        (
            "project.export",
            "command-project-export",
            "file",
            crate::export_preset::open_export_panel,
        ),
        ("project.export_go", "command-project-export-go", "file", crate::export_preset::export_go),
        (
            "project.export_run",
            "command-project-export-run",
            "file",
            crate::export_preset::export_run,
        ),
        ("export.write", "command-export-write", "file", crate::export_preset::write_book_cmd),
        ("help.open_manual", "command-help-manual", "help", open_manual),
        ("help.show_license", "command-help-license", "help", show_license),
        ("toolbar.hide", "command-toolbar-hide", "view", hide_toolbar_button),
    ];
    for (id, title, category, handler) in actions {
        let mut spec = CommandSpec::new(*id, *title, *category);
        if matches!(
            *id,
            "play.start"
                | "play.pause"
                | "play.stop"
                | "play.step"
                | "sequence.play"
                | "sequence.scrub"
                | "play.host"
                | "play.join"
                | "project.package"
                | "project.export"
                | "project.export_go"
                | "project.export_run"
                | "export.write"
        ) {
            spec = spec.non_undoable("play restores the scene when it stops");
        }
        if *id == "app.reset_state" {
            spec = spec.agent_safe(false).non_undoable("reset replaces the working layout");
        }
        if *id == "edit.delete" {
            spec = spec.non_undoable("removing a mock node has no inverse yet");
        }
        if *id == "file.reveal" {
            spec = spec.requires("project");
        }
        if matches!(
            *id,
            "file.open_recent"
                | "file.remove_recent"
                | "file.save_as"
                | "file.close_document"
                | "file.reveal"
                | "content.new_folder"
                | "app.restart"
                | "app.reset_state"
                | "app.open_user_data_folder"
                | "app.safe_mode_restart"
                | "app.copy_diagnostics"
                | "layout.rename"
                | "layout.delete"
                | "layout.export"
                | "layout.import"
                | "layout.undo-reset"
                | "panel.dock_left"
                | "panel.dock_right"
                | "panel.dock_top"
                | "panel.dock_bottom"
                | "panel.float"
                | "panel.reload"
                | "panel.duplicate"
                | "inspector.lock"
                | "console.clear"
                | "console.copy_selection"
                | "console.save"
                | "console.jump_latest"
                | "outliner.expand_all"
                | "outliner.collapse_all"
                | "content.refresh"
                | "settings.open_file"
                | "recovery.open"
                | "recovery.restore"
                | "recovery.keep_both"
                | "recovery.discard"
                | "play.start"
                | "play.pause"
                | "play.stop"
                | "play.step"
                | "sequence.scrub"
                | "view.toggle_panel"
                | "view.lock_layout"
                | "view.theme_set"
                | "toolbar.hide"
                | "export.write"
        ) {
            spec = spec.menu(false);
        }
        let enabled: CommandEnabled = match *id {
            "play.start" => play_can_start,
            "play.pause" => play_can_pause,
            "play.stop" => play_can_stop,
            "play.step" => play_can_step,
            _ => always_enabled,
        };
        session.commands.register(spec, enabled, *handler)?;
    }
    crate::timeline_doc::register(session)?;
    Ok(())
}

pub const SPEC_COMMANDS: &[&str] = &[
    "app.quit",
    "app.restart",
    "app.reset_state",
    "app.open_user_data_folder",
    "app.copy_diagnostics",
    "app.about",
    "app.toggle_fullscreen",
    "app.safe_mode_restart",
    "file.new_project",
    "file.open_project",
    "file.close_project",
    "file.open_recent",
    "file.remove_recent",
    "file.save",
    "file.save_as",
    "file.save_all",
    "file.revert",
    "file.close_document",
    "file.reveal",
    "edit.undo",
    "edit.redo",
    "edit.cut",
    "edit.copy",
    "edit.paste",
    "edit.delete",
    "edit.duplicate",
    "edit.rename",
    "edit.select_all",
    "edit.select_none",
    "edit.invert_selection",
    "view.toggle_panel",
    "view.toggle_asset_drawer",
    "view.toggle_output_drawer",
    "view.dock_asset_drawer",
    "view.reset_layout",
    "view.lock_layout",
    "view.maximize_panel",
    "view.zoom_in",
    "view.zoom_out",
    "view.zoom_reset",
    "view.theme_set",
    "layout.save",
    "layout.load",
    "layout.rename",
    "layout.delete",
    "layout.export",
    "layout.import",
    "panel.dock_left",
    "panel.dock_right",
    "panel.dock_top",
    "panel.dock_bottom",
    "panel.float",
    "panel.close",
    "panel.reload",
    "panel.duplicate",
    "inspector.lock",
    "palette.open",
    "console.clear",
    "console.copy_selection",
    "console.save",
    "console.pause",
    "console.jump_latest",
    "outliner.expand_all",
    "outliner.collapse_all",
    "content.new_folder",
    "content.refresh",
    "settings.open",
    "settings.open_file",
    "keymap.open",
    "recovery.open",
    "play.start",
    "play.pause",
    "play.stop",
    "play.step",
    "help.open_manual",
    "help.show_license",
];

type CommandFn = fn(&mut EditorSession, &CommandArgs) -> Result<(), CoreError>;
type CommandEnabled = fn(&EditorSession) -> bool;

fn play_can_start(session: &EditorSession) -> bool {
    session.world.mode != seal_world::PlayMode::Playing
}

fn play_can_pause(session: &EditorSession) -> bool {
    session.world.mode == seal_world::PlayMode::Playing
}

fn play_can_stop(session: &EditorSession) -> bool {
    session.world.mode != seal_world::PlayMode::Editing
}

fn play_can_step(session: &EditorSession) -> bool {
    session.world.mode != seal_world::PlayMode::Editing
}

fn play_start(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    session.models.panel_states.remove(crate::window_game::PLAY_IN_WINDOW_KEY);
    if args.get("window") == Some("1") {
        session
            .models
            .panel_states
            .insert(crate::window_game::PLAY_IN_WINDOW_KEY.to_string(), "1".into());
    }
    session.world.project_dir = session.project.as_ref().map(|project| project.root.clone());
    session.world.scenes.open = match (session.scene_path(), session.project.as_ref()) {
        (Some(scene), Some(project)) => scene
            .strip_prefix(&project.root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default(),
        _ => String::new(),
    };
    session.world.phys2d_settings = {
        let mut values = std::collections::BTreeMap::new();
        for id in session.foundation.settings.ids() {
            if id.starts_with("physics2d.") {
                if let Some(v) = session.foundation.settings.get(id) {
                    values.insert(id.to_string(), v.to_string());
                }
            }
        }
        seal_world::phys2d::settings_from_project(&values)
    };
    session.world.play();
    crate::tutorial_hooks::on_play_tick(session);
    session.audio_apply = true;
    session.forward_play_log();
    session.notice = Some(session.messages.get("play-running"));
    Ok(())
}

/// `project.package out=`: copy the editor program and the project into a folder players can run.
fn package_project_cmd(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let Some(project) = session.project.clone() else {
        return Err(CoreError::new(session.messages.get("package-no-project")));
    };
    let out = match args.get("out").map(str::trim).filter(|o| !o.is_empty()) {
        Some(out) => PathBuf::from(out),
        None => crate::package::default_package_folder(&project.root, &project.name),
    };
    let exe = match args.get("exe") {
        Some(exe) => PathBuf::from(exe),
        None => std::env::current_exe()?,
    };
    crate::editor_tools::note_worker_budget(&session.foundation.settings);
    let report = crate::package::package_project(&project.root, &project.name, &exe, &out)
        .map_err(|err| {
            CoreError::new(
                session.messages.format("package-failed", &[("error", &err.to_string())]),
            )
        })?;
    let folder = report.folder.display().to_string();
    let files = report.files.to_string();
    session.notify(
        session.messages.format("package-done", &[("folder", &folder), ("files", &files)]),
        false,
    );
    session.host_action = Some(crate::session::HostAction::OpenPath(report.folder));
    Ok(())
}

/// Port network play listens on unless `port=` says otherwise.
pub const NET_PLAY_PORT: u16 = 47710;

/// `play.host port= interface=`: start play and let other Seal windows join it over the network.
fn play_host(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let port = match args.get("port") {
        Some(text) => text
            .trim()
            .parse::<u16>()
            .map_err(|_| CoreError::new(session.messages.get("net-bad-port")))?,
        None => NET_PLAY_PORT,
    };
    // `interface=127.0.0.1` keeps play on this computer; the default accepts the local network.
    let interface =
        args.get("interface").map(str::trim).filter(|i| !i.is_empty()).unwrap_or("0.0.0.0");
    let host = seal_world::net::Host::bind_on(interface, port).map_err(|err| {
        CoreError::new(session.messages.format("net-host-failed", &[("error", &err.to_string())]))
    })?;
    let port = host.port().to_string();
    if session.world.mode == seal_world::PlayMode::Editing {
        play_start(session, &CommandArgs::default())?;
    }
    session.world.net = Some(seal_world::net::NetRole::Host(host));
    session.notice = Some(session.messages.format("net-hosting", &[("port", &port)]));
    Ok(())
}

/// `play.join address=host:port`: show another window's game here and send it this window's input.
fn play_join(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let default = format!("127.0.0.1:{NET_PLAY_PORT}");
    let address = args
        .get("address")
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .unwrap_or(&default)
        .to_string();
    let address =
        if address.contains(':') { address } else { format!("{address}:{NET_PLAY_PORT}") };
    let join = seal_world::net::Join::connect(&address).map_err(|err| {
        CoreError::new(
            session
                .messages
                .format("net-join-failed", &[("address", &address), ("error", &err.to_string())]),
        )
    })?;
    if session.world.mode != seal_world::PlayMode::Editing {
        session.world.stop();
    }
    // A joined window runs none of its own game: no scripts, physics or effects.
    session.world.project_dir = None;
    session.world.play();
    session.world.net = Some(seal_world::net::NetRole::Join(join));
    session.notice = Some(session.messages.format("net-joined", &[("address", &address)]));
    Ok(())
}

fn play_pause(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.world.pause();
    session.notice = Some(session.messages.get("play-paused"));
    Ok(())
}

fn play_stop(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.models.panel_states.remove(crate::window_game::PLAY_IN_WINDOW_KEY);
    session.world.stop();
    session.adopt_world();
    session.notice = Some(session.messages.get("play-stopped"));
    Ok(())
}

fn restart(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.host_action = Some(crate::session::HostAction::Restart { safe_mode: false });
    Ok(())
}

fn safe_restart(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.host_action = Some(crate::session::HostAction::Restart { safe_mode: true });
    Ok(())
}

fn reset_state(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.foundation.modal = Some(crate::foundation::Modal {
        kind: ModalKind::ResetState,
        title: session.messages.get("modal-reset-title"),
        body: session.messages.get("modal-reset-body"),
        confirm: session.messages.get("modal-close-confirm"),
        alternate: String::new(),
        cancel: session.messages.get("modal-close-cancel"),
    });
    Ok(())
}

fn open_user_data(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    let Some(path) =
        session.recent_path.as_ref().and_then(|path| path.parent().map(|p| p.to_path_buf()))
    else {
        return Err(CoreError::new("no user data folder"));
    };
    session.host_action = Some(crate::session::HostAction::OpenPath(path));
    Ok(())
}

fn copy_diagnostics(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    let modules = session.foundation.modules.join(", ");
    let settings = redact_paths(&session.foundation.settings.user_text());
    let gpu = gpu_label(session);
    let log = redact_paths(&recent_log_text(&session.messages, 50));
    let text = session.messages.format(
        "diagnostics-bundle",
        &[
            ("version", crate::version::ENGINE_VERSION_FULL),
            ("os", std::env::consts::OS),
            ("arch", std::env::consts::ARCH),
            ("gpu", gpu.as_str()),
            ("modules", modules.as_str()),
            ("settings", settings.as_str()),
            ("log", log.as_str()),
        ],
    );
    session.foundation.clipboard_kind = "text".into();
    session.foundation.clipboard = text;
    session.notify(session.messages.get("diagnostics-copied"), false);
    Ok(())
}

fn gpu_label(session: &EditorSession) -> String {
    session
        .foundation
        .settings
        .get("render.gpu")
        .filter(|text| !text.is_empty())
        .map(|text| text.to_string())
        .unwrap_or_else(|| session.messages.get("diagnostics-gpu-unavailable"))
}

fn recent_log_text(messages: &crate::messages::Messages, max_lines: usize) -> String {
    seal_log::with_logger(|logger| {
        let records: Vec<_> = logger.records().iter().collect();
        let start = records.len().saturating_sub(max_lines);
        records[start..]
            .iter()
            .map(|record| {
                messages.format(
                    "diagnostics-log-line",
                    &[
                        ("level", record.level.as_str()),
                        ("source", &record.source),
                        ("message", &record.message),
                    ],
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    })
}

fn console_visible_text(session: &EditorSession) -> String {
    session
        .console_filtered_lines()
        .iter()
        .map(|line| format_log_line(&session.messages, line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn toggle_fullscreen(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.foundation.fullscreen = !session.foundation.fullscreen;
    Ok(())
}

fn open_recent(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let Some(store) = session.recent_path.clone() else {
        return Err(CoreError::new("no recent-project list"));
    };
    let recent = project::read_recent(&store)?;
    let index = args.get("index").unwrap_or("0").parse::<usize>().unwrap_or(0);
    let Some(folder) = recent.get(index).cloned() else {
        return Err(CoreError::new("no recent project at that index"));
    };
    session.dispatch("file.open-project", &CommandArgs::new(&[("path", &folder)]))
}

fn remove_recent_cmd(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let Some(store) = session.recent_path.clone() else {
        return Err(CoreError::new("no recent-project list"));
    };
    let folder = required(session, args, "path")?;
    remove_recent(&store, Path::new(&folder))
}

fn save_file(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    mark_documents_saved(session);
    session.save_scene()?;
    session.request_save = true;
    Ok(())
}

fn save_all(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    save_file(session, args)
}

fn save_as(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let Some(path) = args.get("path") else {
        session.host_action = Some(crate::session::HostAction::SaveAs);
        return Ok(());
    };
    atomic_write(Path::new(path), session.scratch_text().as_bytes())?;
    mark_documents_saved(session);
    Ok(())
}

fn revert_file(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    session.dispatch("edit.discard", args)
}

fn close_document(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    if session.documents.iter().any(|doc| doc.dirty()) {
        session.foundation.modal = Some(crate::foundation::Modal {
            kind: ModalKind::Discard,
            title: session.messages.get("modal-close-title"),
            body: session.messages.get("modal-discard-body"),
            confirm: session.messages.get("modal-close-confirm"),
            alternate: String::new(),
            cancel: session.messages.get("modal-close-cancel"),
        });
    }
    Ok(())
}

fn reveal(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    let Some(project) = &session.project else {
        return Err(CoreError::new("no project is open"));
    };
    session.host_action = Some(crate::session::HostAction::OpenPath(project.root.clone()));
    Ok(())
}

fn cut(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.foundation.clipboard = session.scratch_text();
    session.set_scratch_text("");
    Ok(())
}

fn copy(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    if let Some(text) = args.get("text") {
        session.foundation.clipboard_kind = "text".into();
        session.foundation.clipboard = text.to_string();
        return Ok(());
    }
    if session.models.selection.is_some() {
        session.foundation.clipboard_kind = "scene".into();
        session.foundation.clipboard = session.world.to_text();
        return Ok(());
    }
    session.foundation.clipboard_kind = "text".into();
    session.foundation.clipboard = session.scratch_text();
    Ok(())
}

fn paste(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    let next = session.messages.format(
        "edit-paste-concat",
        &[
            ("before", session.scratch_text().as_str()),
            ("after", session.foundation.clipboard.as_str()),
        ],
    );
    session.set_scratch_text(&next);
    Ok(())
}

fn delete_selection(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    if let Some(doc) = session.graph_in_focus() {
        if let Some(id) = doc.selected {
            doc.remove_node(id);
        }
        return Ok(());
    }
    let mut ids = session.selected_ids();
    if ids.is_empty() {
        session.set_scratch_text("");
        return Ok(());
    }
    ids.retain(|id| id != "root");
    if ids.is_empty() {
        return Err(CoreError::new("the root cannot be removed"));
    }
    let previous = session.world.to_text();
    for id in &ids {
        session.world.remove(id);
    }
    session.adopt_world();
    session.models.selection = None;
    session.foundation.selection.clear();
    crate::models::push_scene_past(session, previous);
    session.mark_scene();
    Ok(())
}

fn duplicate(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    let Some(id) = session.primary_selection() else {
        return Err(CoreError::new("nothing selected"));
    };
    let previous = session.world.to_text();
    let Some(new_id) = session.world.duplicate(&id) else {
        return Err(CoreError::new("this entity cannot be duplicated"));
    };
    session.adopt_world();
    // The copy alone is selected afterwards, so the Outliner shows "1 selected".
    session.foundation.selection = vec![new_id.clone()];
    session.models.selection = Some(new_id);
    crate::models::push_scene_past(session, previous);
    session.mark_scene();
    Ok(())
}

fn rename(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let Some(id) = session.primary_selection() else {
        return Err(CoreError::new("nothing selected"));
    };
    if let Some(name) = args.get("name") {
        return session
            .dispatch("outliner.rename", &CommandArgs::new(&[("id", &id), ("name", name)]));
    }
    let name = session
        .models
        .nodes
        .iter()
        .find(|node| node.id == id)
        .map(|node| node.name.clone())
        .unwrap_or_else(|| "Renamed".into());
    session.set_draft(&format!("rename:{id}"), &name)?;
    session.foundation.focused_panel_id = "outliner".into();
    Ok(())
}

fn select_all(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    // A search in the Outliner narrows Select All to the rows it shows. The scene
    // root is never part of it, so a bulk edit can't reach it by accident.
    let ids = if session.models.outliner_query.trim().is_empty() {
        session.models.nodes.iter().map(|node| node.id.clone()).collect()
    } else {
        crate::models::outliner_shown_ids(session)
    };
    session.foundation.selection = ids.into_iter().filter(|id| id != "root").collect();
    session.models.selection = session.foundation.selection.first().cloned();
    Ok(())
}

fn select_none(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.foundation.selection.clear();
    session.models.selection = None;
    Ok(())
}

fn invert_selection(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    let current = if session.foundation.selection.is_empty() {
        session.models.selection.clone().into_iter().collect::<Vec<_>>()
    } else {
        session.foundation.selection.clone()
    };
    session.foundation.selection = session
        .models
        .nodes
        .iter()
        .map(|node| node.id.clone())
        .filter(|id| !current.contains(id))
        .collect();
    session.models.selection = session.foundation.selection.first().cloned();
    Ok(())
}

fn toggle_panel(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    // Toolbar buttons pass `id`; scripts pass `panel`. Either brings the panel forward.
    let id = match args.get("panel").or(args.get("id")) {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => required(session, args, "panel")?,
    };
    if !session.foundation.open_panels.iter().any(|panel| panel == &id) {
        session.foundation.open_panels.push(id.clone());
    }
    session.reveal_panel = Some(id.clone());
    session.foundation.focused_panel_id = id;
    Ok(())
}

fn toggle_asset_drawer(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.foundation.output_drawer_open = false;
    session.foundation.asset_drawer_open = !session.foundation.asset_drawer_open;
    Ok(())
}

fn toggle_output_drawer(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.foundation.asset_drawer_open = false;
    session.foundation.output_drawer_open = !session.foundation.output_drawer_open;
    Ok(())
}

fn dock_asset_drawer(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.foundation.asset_drawer_open = false;
    session.reveal_panel = Some("content".into());
    Ok(())
}

fn lock_layout(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.foundation.layout_locked = !session.foundation.layout_locked;
    let value = if session.foundation.layout_locked { "true" } else { "false" };
    session.foundation.settings.set_user("layout.locked", value)?;
    Ok(())
}

fn maximize_panel(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.foundation.maximized_panel = !session.foundation.maximized_panel;
    Ok(())
}

fn zoom_in(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    set_zoom(session, session.models.scale + 0.1)
}

fn zoom_out(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    set_zoom(session, session.models.scale - 0.1)
}

fn zoom_reset(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    set_zoom(session, 1.0)
}

fn set_zoom(session: &mut EditorSession, scale: f32) -> Result<(), CoreError> {
    session.models.scale = clamp_scale(scale);
    let percent = (session.models.scale * 100.0).round() as u32;
    session.foundation.settings.set_user("ui.scale", &percent.to_string())
}

fn theme_set(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let name = required(session, args, "name")?;
    session.models.set_setting("ui.theme", &name, "user");
    session.foundation.settings.set_user("ui.theme", &name)
}

fn remember_layout(session: &mut EditorSession) {
    session.foundation.previous_layout = Some(session.layout.clone());
}

fn layout_save(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let name = args.get("name").unwrap_or("user").to_string();
    let text = session.layout.to_value().to_text();
    session.foundation.layouts.retain(|(id, _)| id != &name);
    session.foundation.layouts.push((name.clone(), text.clone()));
    write_layout_file(session, &name, &text)
}

fn layout_load(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    if session.foundation.layout_locked {
        return Err(CoreError::new("layout is locked"));
    }
    let name = args.get("name").unwrap_or("user");
    let text = layout_text(session, name)?;
    let layout = layout_from_text(&text)?;
    remember_layout(session);
    session.layout = layout;
    session.layout_epoch = session.layout_epoch.saturating_add(1);
    Ok(())
}

fn layout_rename(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let from = required(session, args, "from")?;
    let to = required(session, args, "to")?;
    let Some(text) =
        session.foundation.layouts.iter().find(|(id, _)| id == &from).map(|(_, text)| text.clone())
    else {
        return Err(catalog_error(session, "error-unknown-layout", &[("name", from.as_str())]));
    };
    session.foundation.layouts.retain(|(id, _)| id != &from && id != &to);
    session.foundation.layouts.push((to.clone(), text.clone()));
    if let Some(dir) = session.layouts_dir.clone() {
        atomic_write(&layout_path(session, &dir, &to), text.as_bytes())?;
        let old = layout_path(session, &dir, &from);
        if old != layout_path(session, &dir, &to) && old.exists() {
            fs::remove_file(old)?;
        }
    }
    Ok(())
}

fn layout_delete(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let name = required(session, args, "name")?;
    let before = session.foundation.layouts.len();
    session.foundation.layouts.retain(|(id, _)| id != &name);
    if session.foundation.layouts.len() == before {
        return Err(catalog_error(session, "error-unknown-layout", &[("name", name.as_str())]));
    }
    if let Some(dir) = &session.layouts_dir {
        let path = layout_path(session, dir, &name);
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn layout_export(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let Some(path) = args.get("path") else {
        session.host_action = Some(crate::session::HostAction::ExportFile);
        return Ok(());
    };
    let name = args.get("name").unwrap_or("user");
    let text = session
        .foundation
        .layouts
        .iter()
        .find(|(id, _)| id == name)
        .map(|(_, text)| text.clone())
        .unwrap_or_else(|| session.layout.to_value().to_text());
    atomic_write(Path::new(path), text.as_bytes())
}

fn layout_import(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let Some(path) = args.get("path") else {
        session.host_action = Some(crate::session::HostAction::ImportFile);
        return Ok(());
    };
    let text = fs::read_to_string(path)?;
    let layout = layout_from_text(&text)?;
    let name = args.get("name").unwrap_or("imported").to_string();
    session.foundation.layouts.retain(|(id, _)| id != &name);
    session.foundation.layouts.push((name, text));
    if !session.foundation.layout_locked {
        remember_layout(session);
        session.layout = layout;
        session.layout_epoch = session.layout_epoch.saturating_add(1);
    }
    Ok(())
}

fn undo_reset(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    let Some(previous) = session.foundation.previous_layout.take() else {
        return Err(CoreError::new("no layout reset to undo"));
    };
    session.layout = previous;
    session.layout_epoch = session.layout_epoch.saturating_add(1);
    Ok(())
}

fn dock(session: &mut EditorSession, side: DockSide) -> Result<(), CoreError> {
    session.foundation.dock_request = Some(side);
    Ok(())
}

fn dock_left(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    dock(session, DockSide::Left)
}

fn dock_right(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    dock(session, DockSide::Right)
}

fn dock_top(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    dock(session, DockSide::Top)
}

fn dock_bottom(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    dock(session, DockSide::Bottom)
}

fn dock_float(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    dock(session, DockSide::Float)
}

fn dock_close(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    dock(session, DockSide::Close)
}

fn dock_reload(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    dock(session, DockSide::Reload)
}

fn duplicate_panel(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    let id = session.foundation.focused_panel_id.clone();
    let base = crate::panel::panel_base(&id);
    if !crate::panel::allows_multiple_instances(base) {
        return Err(CoreError::new("this panel type cannot be duplicated"));
    }
    if session.foundation.layout_locked {
        return Err(CoreError::new("layout is locked"));
    }
    session.foundation.duplicate_panel = Some(id);
    Ok(())
}

fn lock_inspector(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    let panel = session.foundation.focused_panel_id.clone();
    if crate::panel::panel_base(&panel) != "inspector" {
        return Err(CoreError::new("focus an inspector panel"));
    }
    let Some(entity) = session.models.selection.clone().filter(|id| id != "root") else {
        return Err(CoreError::new("select an entity to lock"));
    };
    session.models.inspector_locks.insert(panel, entity);
    Ok(())
}

fn unlock_inspector(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let panel = args
        .get("panel")
        .map(str::to_string)
        .unwrap_or_else(|| session.foundation.focused_panel_id.clone());
    if crate::panel::panel_base(&panel) != "inspector" {
        return Err(CoreError::new("focus an inspector panel"));
    }
    session.models.inspector_locks.remove(&panel);
    Ok(())
}

fn console_clear(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    let count = seal_log::with_logger(|logger| logger.records().len());
    session.foundation.console_clear_before = count;
    session.foundation.console_epoch = session.foundation.console_epoch.saturating_add(1);
    session.models.console_line_selection.clear();
    session.models.console_match_index = 0;
    Ok(())
}

fn console_copy(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    let max_entries = session
        .foundation
        .settings
        .get("console.max_entries")
        .and_then(|text| text.parse().ok())
        .unwrap_or(5000);
    let records = collect_log_lines(session.foundation.console_clear_before);
    let lines: Vec<&LogLine> = session.models.filtered_logs(&records, max_entries);
    let picked: Vec<String> = if session.models.console_line_selection.is_empty() {
        lines
            .get(session.models.console_match_index)
            .map(|line| format_log_line(&session.messages, line))
            .into_iter()
            .collect()
    } else {
        let mut rows = session.models.console_line_selection.clone();
        rows.sort_unstable();
        rows.into_iter()
            .filter_map(|index| {
                lines.get(index).map(|line| format_log_line(&session.messages, line))
            })
            .collect()
    };
    session.foundation.clipboard = picked.join("\n");
    session.foundation.clipboard_kind = "text".into();
    session.notify(session.messages.get("console-copied"), false);
    Ok(())
}

fn console_save(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let Some(path) = args.get("path") else {
        session.host_action = Some(crate::session::HostAction::SaveLog);
        return Ok(());
    };
    atomic_write(Path::new(path), console_visible_text(session).as_bytes())
}

fn console_latest(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    let max_entries = session
        .foundation
        .settings
        .get("console.max_entries")
        .and_then(|text| text.parse().ok())
        .unwrap_or(5000);
    let records = collect_log_lines(session.foundation.console_clear_before);
    let total = session.models.filtered_logs(&records, max_entries).len();
    if total == 0 {
        session.notify(session.messages.get("console-jump-none"), false);
        return Ok(());
    }
    session.models.console_match_index = total - 1;
    session.models.console_line_selection = vec![session.models.console_match_index];
    session.models.console_paused = false;
    session.set_draft(CONSOLE_ROW_DRAFT, &session.models.console_match_index.to_string())?;
    session.notify(session.messages.get("console-jump-latest"), false);
    Ok(())
}

fn expand_all(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.models.expanded = session.models.nodes.iter().map(|node| node.id.clone()).collect();
    Ok(())
}

fn collapse_all(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.models.expanded.clear();
    Ok(())
}

fn new_folder(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    if session.project.as_ref().is_some_and(|project| project.read_only) {
        crate::content::set_content_error(session, "content-error-read-only");
        return Err(CoreError::new(session.messages.get("content-error-read-only")));
    }
    let parent = args
        .get("path")
        .map(str::to_string)
        .or_else(|| session.models.content_root.as_ref().map(|path| path.display().to_string()))
        .ok_or_else(|| CoreError::new("new folder needs a path"))?;
    let name = args.get("name").unwrap_or("folder");
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
        return Err(CoreError::new("content name is not a single item"));
    }
    // Never reuse an existing folder: pick "name 2", "name 3", ... like a file browser.
    let mut target = Path::new(&parent).join(name);
    let mut n = 2;
    while target.exists() {
        target = Path::new(&parent).join(format!("{name} {n}"));
        n += 1;
    }
    let result = fs::create_dir_all(&target).map_err(CoreError::from);
    if let Err(err) = &result {
        crate::content::set_content_error_fmt(
            session,
            crate::content::map_io_error(err),
            vec![("reason".into(), err.to_string())],
        );
        return result;
    }
    session.models.content_root = Some(Path::new(&parent).to_path_buf());
    crate::content::refresh_content_tree(session);
    Ok(())
}

fn refresh_content(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    crate::content::refresh_content_tree(session);
    session.notify(session.messages.get("content-refreshed"), false);
    Ok(())
}

fn wizard_create(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    let project = crate::wizard::create_from_wizard(session)?;
    let id = crate::wizard::wizard_template(session);
    let flat = crate::template_registry::template_by_id(&id).is_some_and(|t| t.is_flat_scene);
    session.wizard_open = false;
    session.attach_project(project)?;
    if flat {
        session.models.viewport_view = "front".into();
        let _ = session.set_draft("viewport-yaw", "0");
        let _ = session.set_draft("viewport-pitch", "0");
    } else {
        // 3D templates open in the Perspective view, which plays through their camera.
        session.models.viewport_view = "perspective".into();
    }
    Ok(())
}

fn wizard_browse(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.host_action = Some(crate::session::HostAction::PickFolder { create: true });
    Ok(())
}

fn wizard_cancel(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.wizard_open = false;
    Ok(())
}

fn open_settings(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.focus_panel("settings");
    Ok(())
}

fn open_settings_file(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    let Some(path) = session
        .recent_path
        .as_ref()
        .and_then(|path| path.parent().map(|dir| dir.join("settings.seal")))
    else {
        return Err(CoreError::new("no settings file"));
    };
    session.host_action = Some(crate::session::HostAction::OpenPath(path));
    Ok(())
}

fn open_keymap(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.focus_panel("settings");
    session.notify(session.messages.get("command-keymap-open"), false);
    Ok(())
}

fn set_draft_cmd(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let key = required(session, args, "key")?;
    let value = args.get("value").unwrap_or("");
    session.set_draft(&key, value)
}

fn forget_draft_cmd(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let key = required(session, args, "key")?;
    session.clear_draft(&key)
}

fn open_recovery(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.focus_panel("recovery");
    Ok(())
}

fn recovery_restore(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.restore_recovered_scene()?;
    session.notice = Some(session.messages.get("recovery-did-restore"));
    Ok(())
}

fn recovery_keep_both(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.keep_recovered_scene()?;
    session.notice = Some(session.messages.get("recovery-did-keep"));
    Ok(())
}

fn recovery_discard(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.discard_recovered_scene()?;
    session.notice = Some(session.messages.get("recovery-did-discard"));
    Ok(())
}

fn hide_toolbar_button(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let id = args.get("id").unwrap_or("play.step");
    let current = session.draft("toolbar-hidden").unwrap_or_default();
    let mut hidden: Vec<&str> = current.split(',').filter(|item| !item.is_empty()).collect();
    if let Some(index) = hidden.iter().position(|item| *item == id) {
        hidden.remove(index);
    } else {
        hidden.push(id);
    }
    session.set_draft("toolbar-hidden", &hidden.join(","))
}

fn open_manual(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    if let Some(path) = offline_manual_path() {
        session.host_action = Some(crate::session::HostAction::OpenPath(path));
    } else {
        session.notify(session.messages.get("help-manual-missing"), false);
    }
    Ok(())
}

fn show_license(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    if let Some(path) = license_path() {
        session.host_action = Some(crate::session::HostAction::OpenPath(path));
    } else {
        session.notify(session.messages.get("help-license-missing"), false);
    }
    Ok(())
}

fn offline_manual_path() -> Option<PathBuf> {
    let tree = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/book");
    if tree.is_dir() {
        return Some(tree);
    }
    std::env::current_exe().ok().and_then(|path| {
        path.parent()
            .and_then(|dir| dir.parent())
            .and_then(|dir| dir.parent())
            .map(|root| root.join("docs").join("book"))
            .filter(|book| book.is_dir())
    })
}

fn license_path() -> Option<PathBuf> {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../LICENSE.md");
    if file.is_file() {
        return Some(file);
    }
    std::env::current_exe().ok().and_then(|path| {
        path.parent()
            .and_then(|dir| dir.parent())
            .and_then(|dir| dir.parent())
            .map(|root| root.join("LICENSE.md"))
            .filter(|license| license.is_file())
    })
}

fn mark_documents_saved(session: &mut EditorSession) {
    for doc in &mut session.documents {
        doc.mark_saved();
    }
}

fn layout_from_text(text: &str) -> Result<Layout, CoreError> {
    Layout::from_value(&parse(text)?)
}

fn write_layout_file(session: &EditorSession, name: &str, text: &str) -> Result<(), CoreError> {
    if let Some(dir) = &session.layouts_dir {
        atomic_write(&layout_path(session, dir, name), text.as_bytes())?;
    }
    Ok(())
}

fn layout_text(session: &EditorSession, name: &str) -> Result<String, CoreError> {
    if let Some((_, text)) = session.foundation.layouts.iter().find(|(id, _)| id == name) {
        return Ok(text.clone());
    }
    if let Some(dir) = &session.layouts_dir {
        let path = layout_path(session, dir, name);
        if path.exists() {
            return Ok(fs::read_to_string(path)?);
        }
    }
    Err(catalog_error(session, "error-unknown-layout", &[("name", name)]))
}

fn layout_path(session: &EditorSession, dir: &Path, name: &str) -> PathBuf {
    let safe: String = name
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' { ch } else { '_' })
        .collect();
    let stem = if safe.is_empty() { "layout" } else { safe.as_str() };
    let file = session.messages.format("layout-storage-file", &[("name", stem)]);
    dir.join(file)
}

fn catalog_error(session: &EditorSession, id: &str, args: &[(&str, &str)]) -> CoreError {
    CoreError::new(session.messages.format(id, args))
}

fn required(session: &EditorSession, args: &CommandArgs, key: &str) -> Result<String, CoreError> {
    args.get(key)
        .map(str::to_string)
        .ok_or_else(|| catalog_error(session, "error-missing-argument", &[("key", key)]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::DockSide;
    use crate::settings::SettingsStore;

    #[test]
    fn every_spec_command_is_registered_and_the_seams_work() {
        let mut session = EditorSession::new(Layout::default());
        for id in SPEC_COMMANDS {
            assert!(session.commands.get(id).is_some(), "{id}");
        }
        let settings = SettingsStore::builtin();
        for id in settings.ids() {
            assert!(session.foundation.settings.get(id).is_some(), "{id}");
        }
        assert_eq!(session.foundation.modules, vec!["core-panels".to_string()]);
        assert!(session.foundation.kinds.for_extension("seal").is_some());
        session.dispatch("edit.copy", &CommandArgs::default()).unwrap();
        session.dispatch("edit.copy", &CommandArgs::new(&[("text", "line")])).unwrap();
        assert_eq!(session.foundation.clipboard, "line");
        session.dispatch("edit.select_all", &CommandArgs::default()).unwrap();
        assert!(session.foundation.selection.len() > 1);
        session.dispatch("view.lock_layout", &CommandArgs::default()).unwrap();
        assert!(session.dispatch("view.reset_layout", &CommandArgs::default()).is_err());
        session.dispatch("view.lock_layout", &CommandArgs::default()).unwrap();
        let scratch = session.scratch_text();
        session.dispatch("layout.save", &CommandArgs::new(&[("name", "mine")])).unwrap();
        session.dispatch("window.reset-layout", &CommandArgs::default()).unwrap();
        session.choose_modal(crate::foundation::ModalChoice::Confirm);
        session.dispatch("layout.load", &CommandArgs::new(&[("name", "mine")])).unwrap();
        assert_eq!(session.scratch_text(), scratch);
        session.dispatch("layout.undo-reset", &CommandArgs::default()).unwrap();
        session.dispatch("panel.dock_left", &CommandArgs::default()).unwrap();
        assert_eq!(session.foundation.dock_request, Some(DockSide::Left));
        session.foundation.settings.set_user("agent.bus.enabled", "true").unwrap();
        session.foundation.settings.set_user("agent.bus.require_approval", "false").unwrap();
        assert!(session.bus_run("app.reset_state", "").is_err());
        session.bus_run("app.toggle_fullscreen", "").unwrap();
        assert!(session.foundation.fullscreen);
        // Lift the resting demo crate so play has somewhere to drop it.
        session.world.entity_mut("crate").unwrap().translation[1] = 4.0;
        let start = session.world.entity("crate").unwrap().translation[1];
        session.dispatch("play.start", &CommandArgs::default()).unwrap();
        session.tick(0.2);
        if seal_world::play_preview_enabled() {
            assert!(session.world.entity("crate").unwrap().translation[1] < start);
        } else {
            assert_eq!(session.world.entity("crate").unwrap().translation[1], start);
        }
        session.dispatch("play.stop", &CommandArgs::default()).unwrap();
        assert_eq!(session.world.entity("crate").unwrap().translation[1], start);
        session.foundation.notices.push("kept", true, 0);
        session.foundation.notices.push("gone", false, 0);
        let visible = session.foundation.notices.visible(7_000, 6_000);
        assert_eq!(visible.len(), 1);
        assert!(visible[0].error);
        session.start_task("scan", "Scanning");
        session.set_task_progress("scan", 0.4);
        assert_eq!(session.status_snapshot().tasks, "Scanning");
        session.cancel_task("scan");
        assert!(session.status_snapshot().tasks.is_empty());
    }
}
