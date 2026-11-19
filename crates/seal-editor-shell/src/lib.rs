//! The editor window. Layout changes are written with the crash-safe store.
#![forbid(unsafe_code)]

mod doc_tabs;
mod native_dialog;
mod game_window;
mod hdr_game;
mod play_extra_windows;
mod gamepad;
mod input_devices;
mod sound;
pub use sound::{
    set_mix_sink, set_music_sink, set_preview_sink, set_sound_sink, MixSink, MusicSink,
    PreviewSink, SoundSink,
};
#[cfg(target_os = "macos")]
mod macos_menu;
mod menu_bar;
mod menu_ui;
mod status_bar;
mod toolbar;
mod window_placement;

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use seal_editor_core::{
    prepare_user_state_dir, BackgroundWriter, CommandArgs, EditorSession, PanelRegistry, SaveKind,
    WindowPlacement, WorkspaceFile,
};
use seal_editor_panels::{register_builtin, show, PanelEvent};
use seal_ui::{
    apply_look, paint_icon, panel_base, pressed_chord,
    show_dock_with_badges, show_palette, typeahead_key, DockHost, Icon, PaletteEvent,
    TabMenuResult, ThemeColors, ICON_PX,
};

pub fn run() -> eframe::Result<()> {
    run_with(seal_editor_core::LaunchOptions::default())
}

pub fn run_with(options: seal_editor_core::LaunchOptions) -> eframe::Result<()> {
    let user_data = options.user_data.clone().unwrap_or_else(prepare_user_state_dir);
    run_shell(user_data.join("layout.seal"), None, options)
}

pub fn run_with_path(path: PathBuf) -> eframe::Result<()> {
    let options = seal_editor_core::LaunchOptions {
        user_data: path.parent().map(Path::to_path_buf),
        ..seal_editor_core::LaunchOptions::default()
    };
    run_shell(path, None, options)
}

/// Open the editor on a fresh layout in `directory`, write the first settled
/// frame to `image` as a binary PPM, and close. Used to check the look.
pub fn run_screenshot(
    directory: &Path,
    image: PathBuf,
    launch: seal_editor_core::LaunchOptions,
) -> eframe::Result<()> {
    run_shell(directory.join("layout.seal"), Some(image), launch)
}

fn run_shell(
    path: PathBuf,
    shot: Option<PathBuf>,
    launch: seal_editor_core::LaunchOptions,
) -> eframe::Result<()> {
    let file = if launch.safe_mode { WorkspaceFile::fresh() } else { WorkspaceFile::load(&path) };
    let window = file.window();
    let (width, height, maximized) = if shot.is_some() {
        (1920.0, 1080.0, false)
    } else {
        (window.width.max(800.0), window.height.max(500.0), window.maximized && !launch.safe_mode)
    };
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Seal")
        .with_inner_size([width, height])
        .with_min_inner_size([800.0, 500.0])
        .with_position(egui::Pos2::new(window.x, window.y))
        .with_maximized(maximized)
        .with_decorations(false)
        .with_icon(seal_ui::app_icon());
    let mut vsync = true;
    if launch.game {
        if let Some(project) = launch.project.as_deref() {
            let game = game_window::startup(project);
            if game.hdr_output {
                match hdr_game::try_run(&path, &launch) {
                    Ok(()) => return Ok(()),
                    Err(err) => {
                        seal_log::warn!(
                            "HDR output is not available ({err}); using the standard window"
                        );
                    }
                }
            }
            let (game_w, game_h) = game_window::startup_size(&game);
            let title = if game.title.is_empty() { "Game".into() } else { game.title.clone() };
            viewport = viewport
                .with_title(title)
                .with_inner_size([game_w, game_h])
                .with_resizable(game.resizable)
                .with_fullscreen(game.mode == "fullscreen")
                .with_decorations(game.mode == "windowed");
            if game.always_on_top {
                viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnTop);
            }
            vsync = game.vsync;
        }
    }
    // A depth buffer lets the viewport draw 3D shapes on the graphics card.
    let mut native = eframe::NativeOptions { viewport, depth_buffer: 24, ..Default::default() };
    native.glow_options.vsync = vsync;
    // Project Settings > Rendering > MSAA smooths edges in the window (read at start).
    let msaa = launch.project.as_deref().map(seal_editor_core::project_msaa).unwrap_or(0);
    native.multisampling = if msaa > 1 { msaa.min(8) as u16 } else { 0 };
    let start = |native: eframe::NativeOptions| {
        let (path, launch, shot) = (path.clone(), launch.clone(), shot.clone());
        eframe::run_native(
            "Seal",
            native,
            Box::new(move |cc| {
                cc.egui_ctx.set_embed_viewports(false);
                let cpu_only = std::env::var_os("SEAL_CPU_VIEWPORT").is_some();
                seal_ui::gpu_viewport::set_enabled(cc.gl.is_some() && !cpu_only);
                let mut shell = Shell::new(path, &launch);
                shell.shot = shot;
                let inner = Rc::new(RefCell::new(shell));
                watch_session_end(cc, &inner);
                Ok(Box::new(ShellApp { inner }) as Box<dyn eframe::App>)
            }),
        )
    };
    match start(native.clone()) {
        // Some graphics drivers offer no multisampled window: start without MSAA instead of failing.
        Err(err) if native.multisampling > 0 => {
            seal_log::warn!("MSAA {}x is not available here ({err}); starting without it", native.multisampling);
            native.multisampling = 0;
            start(native)
        }
        other => other,
    }
}

struct ShellApp {
    inner: Rc<RefCell<Shell>>,
}

/// Save everything when Windows shuts down or signs out (see `seal_ui::session_end`).
fn watch_session_end(cc: &eframe::CreationContext<'_>, inner: &Rc<RefCell<Shell>>) {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = cc.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(win) = handle.as_raw() else {
        return;
    };
    let shell = Rc::downgrade(inner);
    let watched = seal_ui::session_end::install(
        win.hwnd.get(),
        Box::new(move || {
            let Some(shell) = shell.upgrade() else {
                return;
            };
            let Ok(mut shell) = shell.try_borrow_mut() else {
                return;
            };
            if !shell.flush_exit() {
                seal_log::error!("saving before Windows shut down did not finish");
            }
        }),
    );
    if !watched {
        seal_log::warn!("could not watch for Windows shutdown; save before signing out");
    }
}

struct Shell {
    path: PathBuf,
    file: WorkspaceFile,
    dock: DockHost,
    session: EditorSession,
    registry: PanelRegistry,
    applied_epoch: u64,
    last_change: Instant,
    dirty: bool,
    status_key: String,
    save_error: Option<String>,
    save_fault: Option<seal_editor_core::SaveFault>,
    save_attempt: u32,
    retry_at: Option<Instant>,
    safe_mode: bool,
    /// A packaged game: only the game view, play starts at once, Esc quits.
    game: bool,
    applied_look: Option<(String, f32, i32, bool, u64, String, String)>,
    shot: Option<PathBuf>,
    screenshot_done: bool,
    /// A window capture asked for by the remote screenshot endpoint is in flight.
    remote_shot_pending: bool,
    frames: u32,
    fullscreen_sent: bool,
    play_painted: Instant,
    applied_game_window: String,
    play_extra_open: std::collections::BTreeSet<u32>,
    window_restored: bool,
    last_native_ppp: Option<f32>,
    open_menu_category: Option<String>,
    last_monitor: Option<String>,
    #[cfg(target_os = "macos")]
    mac_menu: Option<macos_menu::MacMenu>,
    last_tick: Instant,
    scene_seen: u64,
    scene_idle: Option<Instant>,
    scene_since: Option<Instant>,
    scene_inflight: bool,
    scene_ticket: u64,
    writer: BackgroundWriter,
    os_clipboard: String,
    seen_focus: String,
    panel_faults: HashMap<String, String>,
    pending_path: Option<PendingPathPrompt>,
    /// Profile name carried by the typed-path fallback for export and import.
    path_prompt_arg: String,
    path_prompt_text: String,
    /// The path bar takes keyboard focus on the frame it opens, so no typed letter is lost.
    path_prompt_focus: bool,
    wizard_was_open: bool,
    save_paths: HashMap<SaveKind, PathBuf>,
    failed_path: Option<PathBuf>,
    status_cmd_history: Vec<String>,
    status_cmd_history_pos: usize,
    status_cmd_flash: Option<(Instant, bool)>,
    responsive_narrow: bool,
    layout_stash_wide: Option<seal_editor_core::Layout>,
    /// Top edge of the status bar last frame; drawers stop there.
    status_bar_top: Option<f32>,
    /// Layout epoch the narrow layout was built for; another epoch means a new layout arrived.
    narrow_epoch: u64,
    /// Gamepads, opened the first time a game plays.
    pads: Option<gamepad::Pads>,
    /// Last light-bar colour sent, so a steady colour is not set every frame.
    pad_light_sent: Option<[f32; 3]>,
    /// Pad buttons down while the bindings editor waits for one.
    pad_held: Option<Vec<String>>,
    /// Pad buttons already forwarded to play-screen focus, so a hold moves once.
    pad_ui_down: Vec<String>,
    /// Play in New Window already started an HDR window for this press.
    hdr_play_launched: bool,
}

/// D-pad and the south button, as play-screen focus keys.
fn pad_focus_key(name: &str) -> Option<&'static str> {
    Some(match name {
        "pad.dpad_up" => "DPadUp",
        "pad.dpad_down" => "DPadDown",
        "pad.dpad_left" => "DPadLeft",
        "pad.dpad_right" => "DPadRight",
        "pad.south" => "GamepadA",
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingPathPrompt {
    NewProject,
    OpenProject,
    SaveAs,
    ImportLayout,
    ImportAsset,
    ExportLayout,
    ExportProfile,
    ImportProfile,
    MigrateAssets,
}

impl PendingPathPrompt {
    fn title_key(self) -> &'static str {
        match self {
            Self::NewProject => "command-file-new-project",
            Self::OpenProject => "command-file-open-project",
            Self::SaveAs => "command-file-save-as",
            Self::ImportLayout => "command-layout-import",
            Self::ImportAsset => "command-asset-import",
            Self::ExportLayout => "command-layout-export",
            Self::ExportProfile => "command-keymap-profile-export",
            Self::ImportProfile => "command-keymap-profile-import",
            Self::MigrateAssets => "command-asset-copy-to-project",
        }
    }
}

impl Shell {
    fn new(path: PathBuf, options: &seal_editor_core::LaunchOptions) -> Self {
        let file =
            if options.safe_mode { WorkspaceFile::fresh() } else { WorkspaceFile::load(&path) };
        let status_key = match file.origin() {
            seal_editor_core::LoadOrigin::Backup => "status-recovered".to_string(),
            seal_editor_core::LoadOrigin::Default | seal_editor_core::LoadOrigin::Primary => {
                "status-saved".to_string()
            }
        };
        let layout = if options.safe_mode || options.reset_layout {
            seal_editor_core::Layout::default()
        } else {
            file.layout()
        };
        let mut session = EditorSession::new(layout);
        let state_dir = options.user_data.clone().unwrap_or_else(prepare_user_state_dir);
        if !options.safe_mode && !options.no_restore {
            if let Err(err) =
                session.load_journal(&state_dir.join("journal").join("scratch.journal"))
            {
                seal_log::warn!("journal was not loaded: {err}");
            }
        }
        session.recent_path = Some(state_dir.join("recent.seal"));
        session.user_scene = Some(state_dir.join("scene.seal"));
        session.scene_journal = Some(state_dir.join("scene.journal"));
        session.undo_journal = Some(state_dir.join("undo.journal"));
        session.layouts_dir = Some(state_dir.join("layouts"));
        session.memory_path = Some(state_dir.join("memory.seal"));
        if !options.safe_mode && !options.no_restore {
            if let Err(err) = session.load_memory() {
                session.notice = Some(err.to_string());
            }
            if let Err(err) = session.load_settings(&state_dir.join("settings.seal")) {
                seal_log::warn!("settings were not loaded: {err}");
            }
            let ceiling = session.logging_ceiling();
            seal_log::with_logger(|logger| logger.set_ceiling(ceiling));
            if let Some(text) = session.logging_notice() {
                session.notice = Some(text);
            }
            let days = session
                .foundation
                .settings
                .get("log.file_retention_days")
                .and_then(|text| text.parse().ok())
                .unwrap_or(14);
            if let Err(err) = seal_log::prune_old_logs(&state_dir.join("logs"), days) {
                session.notice = Some(err.to_string());
            }
            if let Err(err) = session.load_panel_state(&state_dir.join("session.seal")) {
                seal_log::warn!("panel state was not loaded: {err}");
            }
            if let Err(err) = session.load_named_layouts() {
                session.notice = Some(err.to_string());
            }
            if options.project.is_none() {
                if let Err(err) = session.load_scene(&state_dir.join("scene.seal")) {
                    session.notice = Some(err.to_string());
                }
                let open_last =
                    session.foundation.settings.get("startup.open_last_project") != Some("false");
                let show_welcome =
                    session.foundation.settings.get("startup.show_welcome") != Some("false");
                let recent = session
                    .recent_path
                    .as_ref()
                    .and_then(|path| seal_editor_core::read_recent(path).ok())
                    .unwrap_or_default();
                let choice =
                    seal_editor_core::startup_choice(open_last, show_welcome, &recent, false);
                if !choice.show_welcome {
                    session.layout.without_panel("welcome");
                }
                if let Some(path) = choice.project {
                    if let Err(err) = session.dispatch(
                        "file.open-project",
                        &CommandArgs::new(&[("path", path.as_str())]),
                    ) {
                        session.notice = Some(err.to_string());
                    }
                }
            }
            let unclean = seal_editor_core::take_unclean_start(&state_dir);
            if unclean {
                let mut scene_recovered = false;
                match session.recover_journals() {
                    Ok(recovered) => scene_recovered = recovered,
                    Err(err) => {
                        seal_log::warn!("scene journal was not recovered: {err}");
                    }
                }
                if let Err(err) = session.present_recovery(scene_recovered, true) {
                    session.notice = Some(err.to_string());
                }
            } else {
                session.settle_loaded_scratch();
            }
        }
        if !options.safe_mode {
            if let Err(err) = session.load_user_keymap(&state_dir.join("keymap.seal")) {
                seal_log::warn!("keymap was not loaded: {err}");
            }
        }
        if let Some(project) = &options.project {
            if let Some(text) = project.to_str() {
                if let Err(err) =
                    session.dispatch("file.open-project", &CommandArgs::new(&[("path", text)]))
                {
                    seal_log::warn!("project {text} did not open: {err}");
                    session.notice = Some(err.to_string());
                }
            }
        }
        let project_name =
            session.project.as_ref().map(|project| project.name.as_str()).unwrap_or("none");
        seal_log::info!("Seal {}", seal_editor_core::ENGINE_VERSION);
        seal_log::info!("project {project_name}");
        let dock = DockHost::from_layout(&session.layout);
        let mut registry = PanelRegistry::default();
        if let Err(err) = register_builtin(&mut registry) {
            session.notice = Some(err.to_string());
        }
        let responsive_narrow = !session.layout.has_bottom_strip();
        Self {
            path,
            file,
            dock,
            session,
            registry,
            applied_epoch: 0,
            last_change: Instant::now(),
            dirty: false,
            status_key,
            save_error: None,
            save_fault: None,
            save_attempt: 0,
            retry_at: None,
            safe_mode: options.safe_mode,
            game: options.game,
            applied_look: None,
            shot: None,
            screenshot_done: false,
            remote_shot_pending: false,
            frames: 0,
            fullscreen_sent: false,
            play_painted: Instant::now(),
            applied_game_window: String::new(),
            play_extra_open: std::collections::BTreeSet::new(),
            window_restored: false,
            last_native_ppp: None,
            open_menu_category: None,
            last_monitor: None,
            #[cfg(target_os = "macos")]
            mac_menu: None,
            last_tick: Instant::now(),
            scene_seen: 0,
            scene_idle: None,
            scene_since: None,
            scene_inflight: false,
            scene_ticket: 0,
            writer: BackgroundWriter::new(8),
            os_clipboard: String::new(),
            seen_focus: String::new(),
            panel_faults: HashMap::new(),
            pending_path: None,
            path_prompt_arg: String::new(),
            path_prompt_text: String::new(),
            path_prompt_focus: false,
            wizard_was_open: false,
            save_paths: HashMap::new(),
            failed_path: None,
            status_cmd_history: Vec::new(),
            status_cmd_history_pos: 0,
            status_cmd_flash: None,
            responsive_narrow,
            layout_stash_wide: None,
            status_bar_top: None,
            narrow_epoch: 0,
            pads: None,
            pad_light_sent: None,
            pad_held: None,
            pad_ui_down: Vec::new(),
            hdr_play_launched: false,
        }
    }

    fn sync_responsive_layout(&mut self, ctx: &egui::Context) {
        if self.safe_mode
            || self.session.foundation.layout_locked
            || self.session.foundation.maximized_panel
        {
            return;
        }
        if !self.session.layout.has_bottom_strip() && self.layout_stash_wide.is_none() {
            self.responsive_narrow = true;
        }
        let width = ctx.content_rect().width();
        let narrow = width > 0.0 && width < seal_editor_core::NARROW_LAYOUT_MAX_WIDTH;
        // Opening another project (or loading a layout) while narrow brings a wide layout:
        // convert it like at startup instead of forcing the narrow ratio onto its root split.
        if narrow
            && self.responsive_narrow
            && self.session.layout_epoch != self.narrow_epoch
            && self.session.layout.has_bottom_strip()
        {
            self.responsive_narrow = false;
            self.layout_stash_wide = None;
        }
        if narrow == self.responsive_narrow {
            if narrow {
                let fraction = seal_editor_core::Layout::narrow_right_ratio(width);
                self.dock.set_root_horizontal_fraction(fraction);
            }
            return;
        }
        self.responsive_narrow = narrow;
        if narrow {
            self.layout_stash_wide = Some(self.dock.to_layout());
            self.session.layout = seal_editor_core::Layout::m02_narrow_for_width(width);
            self.session.layout_epoch = self.session.layout_epoch.saturating_add(1);
            self.dock = DockHost::from_layout(&self.session.layout);
            self.narrow_epoch = self.session.layout_epoch;
            let fraction = seal_editor_core::Layout::narrow_right_ratio(width);
            self.dock.set_root_horizontal_fraction(fraction);
        } else if let Some(saved) = self.layout_stash_wide.take() {
            self.session.layout = saved;
            self.session.layout_epoch = self.session.layout_epoch.saturating_add(1);
            self.dock = DockHost::from_layout(&self.session.layout);
        }
        self.dirty = true;
        self.last_change = Instant::now();
    }

    fn pin_sidebar(&mut self, id: &str) {
        if let Some(path) = self.dock.state.find_tab(&id.to_string()) {
            self.dock.state.remove_tab(path);
        }
        if !self.session.foundation.sidebar_pins.iter().any(|pin| pin == id) {
            self.session.foundation.sidebar_pins.push(id.to_string());
        }
        self.session.foundation.sidebar_open = Some(id.to_string());
        self.dirty = true;
        self.last_change = Instant::now();
    }

    fn sidebar_overlay(&mut self, ctx: &egui::Context) {
        let pins = self.session.foundation.sidebar_pins.clone();
        if pins.is_empty() {
            return;
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.session.foundation.sidebar_open = None;
        }
        let content = ctx.content_rect();
        let rail_w = 26.0;
        let top = ctx
            .data(|d| d.get_temp::<f32>(egui::Id::new("seal-sidebar-rail-top")))
            .unwrap_or(content.top());
        let height = (content.bottom() - top).max(0.0);
        let mut picked = self.session.foundation.sidebar_open.clone();
        egui::Area::new(egui::Id::new("sidebar-rail"))
            .interactable(true)
            .fixed_pos(egui::pos2(content.left(), top))
            .show(ctx, |ui| {
                ui.set_width(rail_w);
                ui.set_min_height(height);
                ui.vertical(|ui| {
                    for id in &pins {
                        let kind = panel_base(id);
                        let title = self
                            .registry
                            .get(kind)
                            .map(|panel| self.session.messages.get(&panel.title_key))
                            .unwrap_or_else(|| kind.to_string());
                        let icon = match kind {
                            "console" => Icon::File,
                            "inspector" => Icon::ViewColumns,
                            "outliner" => Icon::ViewList,
                            "place" => Icon::EntityCube,
                            _ => Icon::File,
                        };
                        let active = picked.as_deref() == Some(id.as_str());
                        let fill = if active {
                            ui.visuals().faint_bg_color
                        } else {
                            ui.visuals().extreme_bg_color
                        };
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(rail_w, 56.0), egui::Sense::click());
                        let response = response.on_hover_text(title);
                        if ui.is_rect_visible(rect) {
                            ui.painter().rect_filled(rect, 0.0, fill);
                            if active {
                                ui.painter().vline(
                                    rect.left(),
                                    rect.y_range(),
                                    egui::Stroke::new(2.0, ui.visuals().hyperlink_color),
                                );
                            }
                            paint_icon(
                                ui.painter(),
                                rect.center(),
                                icon,
                                ICON_PX,
                                ui.visuals().text_color(),
                            );
                        }
                        if response.clicked() {
                            picked = if active { None } else { Some(id.clone()) };
                        }
                    }
                });
            });
        self.session.foundation.sidebar_open = picked.clone();
        let Some(open_id) = picked else {
            return;
        };
        let sheet_w = (content.width() * 0.34).clamp(280.0, 520.0);
        let sheet_rect = egui::Rect::from_min_max(
            egui::pos2(content.left() + rail_w, top),
            egui::pos2(content.left() + rail_w + sheet_w, content.bottom()),
        );
        let mut dock_back = false;
        egui::Area::new(egui::Id::new("sidebar-sheet"))
            .order(egui::Order::Foreground)
            .fixed_pos(sheet_rect.min)
            .interactable(true)
            .constrain(false)
            .show(ctx, |ui| {
                // Bound the sheet to the space under the top bars; otherwise a tall
                // panel pushes the area up over the menu bar.
                let inner = sheet_rect.size() - egui::vec2(2.0, 2.0);
                ui.set_min_size(inner);
                ui.set_max_size(inner);
                ui.set_clip_rect(sheet_rect);
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .stroke(egui::Stroke::new(
                        1.0,
                        ui.visuals().widgets.noninteractive.bg_stroke.color,
                    ))
                    .show(ui, |ui| {
                        // Header with the panel name and a button that puts it back in the dock.
                        let kind = panel_base(&open_id);
                        let title = self
                            .registry
                            .get(kind)
                            .map(|panel| self.session.messages.get(&panel.title_key))
                            .unwrap_or_else(|| kind.to_string());
                        ui.horizontal(|ui| {
                            ui.add_space(6.0);
                            ui.strong(&title);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let tip = self.session.messages.get("sidebar-dock-tip");
                                    if ui
                                        .small_button(self.session.messages.get("sidebar-dock"))
                                        .on_hover_text(tip)
                                        .clicked()
                                    {
                                        dock_back = true;
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
                            &open_id,
                            &mut deferred,
                        );
                        for (command, args) in deferred {
                            if let Err(err) = self.session.dispatch(&command, &args) {
                                self.session.notice = Some(err.to_string());
                            }
                        }
                    });
            });
        if dock_back {
            self.session.foundation.sidebar_pins.retain(|pin| pin != &open_id);
            self.session.foundation.sidebar_open = None;
            self.dock.show_panel(&open_id);
            self.dirty = true;
            self.last_change = Instant::now();
        }
    }

    fn begin_path_prompt(&mut self, kind: PendingPathPrompt) {
        self.pending_path = Some(kind);
        self.path_prompt_text.clear();
        self.path_prompt_focus = true;
    }

    fn clear_path_prompt(&mut self) {
        self.pending_path = None;
        self.path_prompt_text.clear();
    }

    fn drain_saves(&mut self) {
        let finished = self.writer.poll();
        for (kind, result) in finished {
            if result.is_err() {
                self.failed_path = self.save_paths.get(&kind).cloned();
            }
            match kind {
                SaveKind::Scene => {
                    self.scene_inflight = false;
                    if result.is_ok() {
                        let ticket = self.scene_ticket;
                        match self.session.note_scene_saved(ticket) {
                            Ok(()) => {
                                if !self.session.scene_dirty {
                                    self.scene_idle = None;
                                    self.scene_since = None;
                                }
                                note_save(self, Ok(()));
                            }
                            Err(err) => {
                                self.scene_idle = Some(Instant::now());
                                note_save(self, Err(err));
                            }
                        }
                    } else {
                        self.scene_idle = Some(Instant::now());
                        note_save(self, result);
                    }
                }
                SaveKind::Layout => {
                    if result.is_err() {
                        self.dirty = true;
                        self.last_change = Instant::now();
                    }
                    note_save(self, result);
                }
                SaveKind::Settings => {
                    if result.is_ok() {
                        self.session.settings_dirty = false;
                    } else {
                        self.last_change = Instant::now();
                    }
                    note_save(self, result);
                }
                _ => note_save(self, result),
            }
        }
    }

    fn queue_scene(&mut self) -> Result<bool, seal_editor_core::CoreError> {
        let Some((path, bytes)) = self.session.scene_snapshot() else {
            return Ok(false);
        };
        let ticket = self.session.scene_edits;
        self.save_paths.insert(SaveKind::Scene, path.clone());
        self.writer.submit(SaveKind::Scene, path, bytes)?;
        self.scene_inflight = true;
        self.scene_ticket = ticket;
        Ok(true)
    }

    fn capture_window(&mut self, ctx: &egui::Context) {
        let rect = ctx.input(|input| input.viewport().outer_rect);
        let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
        if let Some(rect) = rect {
            self.file.set_window(&WindowPlacement {
                x: rect.min.x,
                y: rect.min.y,
                width: rect.width().max(320.0),
                height: rect.height().max(240.0),
                maximized,
                monitor: self.last_monitor.clone().or_else(|| self.file.window().monitor),
            });
        }
    }

    fn maybe_restore_window(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        if self.window_restored || self.safe_mode {
            return;
        }
        let Some(window) = frame.winit_window() else {
            return;
        };
        let monitors =
            window_placement::monitors_from_window(window.as_ref(), self.session.models.scale);
        let Some(placed) =
            window_placement::restore_on_startup(&self.file, &monitors, self.safe_mode)
        else {
            self.window_restored = true;
            return;
        };
        window_placement::send_placement(ctx, &placed);
        self.window_restored = true;
    }

    #[cfg(target_os = "macos")]
    fn run_mac_menu_action(&mut self, action: &str) {
        if let Some(panel) = action.strip_prefix("dock:") {
            self.dock.show_panel(panel);
            self.dirty = true;
            self.last_change = Instant::now();
        } else {
            self.run_command(action);
        }
    }

    fn install_mac_menu(&mut self) {
        #[cfg(target_os = "macos")]
        if self.mac_menu.is_none() {
            self.mac_menu = Some(macos_menu::MacMenu::install(&self.session, &self.registry));
        }
    }

    fn save_now(&mut self, ctx: &egui::Context) {
        if self.safe_mode {
            self.dirty = false;
            self.status_key = "status-saved".into();
            return;
        }
        self.status_key = "status-saving".into();
        self.capture_window(ctx);
        self.session.layout = self.dock.to_layout();
        self.file.set_layout(&self.session.layout);
        let store = self.session.layout_store_path(&self.path);
        let bytes = self.file.encoded().into_bytes();
        self.save_paths.insert(SaveKind::Layout, store.clone());
        match self.writer.submit(SaveKind::Layout, store, bytes) {
            Ok(()) => {
                self.dirty = false;
                seal_log::file_only(
                    seal_log::Level::Debug,
                    "seal_editor_shell",
                    "queued layout save".into(),
                );
            }
            Err(err) => {
                self.last_change = Instant::now();
                note_save(self, Err(err));
            }
        }
    }
}

impl Shell {
    fn run_command(&mut self, id: &str) {
        self.before_command(id);
        if let Err(err) = self.session.dispatch(id, &CommandArgs::default()) {
            self.report_command_error(err);
        }
    }

    fn run_args(&mut self, id: &str, args: &[(&str, &str)]) {
        self.before_command(id);
        if let Err(err) = self.session.dispatch(id, &CommandArgs::new(args)) {
            self.report_command_error(err);
        }
    }

    /// A shortcut that has nothing to act on does nothing, like any desktop app;
    /// real failures still show as a toast.
    fn report_command_error(&mut self, err: seal_editor_core::CoreError) {
        if err.message == "nothing selected" {
            return;
        }
        self.session.notify(err.to_string(), true);
    }

    /// Remote control: capture the window when a client asked for a screenshot.
    fn feed_remote_screenshot(&mut self, ctx: &egui::Context) {
        if self.shot.is_some() {
            return;
        }
        if self.session.remote_screenshot_wanted && !self.remote_shot_pending {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            self.remote_shot_pending = true;
        }
        if !self.remote_shot_pending {
            return;
        }
        let image = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            let rgba: Vec<u8> =
                image.pixels.iter().flat_map(|pixel| pixel.to_srgba_unmultiplied()).collect();
            seal_editor_core::editor_remote::set_remote_frame(
                &mut self.session,
                &rgba,
                image.width() as u32,
                image.height() as u32,
            );
            self.remote_shot_pending = false;
        } else {
            ctx.request_repaint();
        }
    }

    /// Screenshot mode: ask for the image once the layout has settled, write it, close.
    fn take_shot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.shot.clone() else {
            return;
        };
        self.frames += 1;
        if self.frames <= 3 {
            let (w, h) = (1920.0, 1080.0);
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
        }
        if self.frames == 10 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let image = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            let mut bytes = format!("P6\n{} {}\n255\n", image.width(), image.height()).into_bytes();
            for pixel in &image.pixels {
                bytes.extend_from_slice(&[pixel.r(), pixel.g(), pixel.b()]);
            }
            if let Err(err) = std::fs::write(&path, bytes) {
                seal_log::error!("screenshot was not written: {err}");
            } else {
                self.screenshot_done = true;
            }
            self.shot = None;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }

    fn top_bars(&mut self, ui: &mut egui::Ui) {
        let visuals = ui.visuals().clone();
        let bar = egui::Frame::new()
            .fill(visuals.faint_bg_color)
            .inner_margin(egui::Margin::symmetric(6, 3));
        #[cfg(not(target_os = "macos"))]
        {
            let alt_open = menu_ui::take_alt_mnemonic(ui.ctx(), &self.session.messages);
            if alt_open.is_some() {
                self.open_menu_category = alt_open;
            }
            let panels = self.dock.tab_ids();
            if let Some(click) =
                menu_bar::show(ui, &self.session, &panels, &mut self.open_menu_category)
            {
                match click {
                    menu_bar::MenuClick::Command { id, args } => {
                        if args.is_empty() {
                            self.run_command(&id);
                        } else {
                            let pairs: Vec<(&str, &str)> = args
                                .iter()
                                .map(|(key, value)| (key.as_str(), value.as_str()))
                                .collect();
                            self.run_args(&id, &pairs);
                        }
                    }
                    menu_bar::MenuClick::ShowPanel(id) => {
                        self.dock.show_panel(&id);
                        self.dirty = true;
                        self.last_change = Instant::now();
                    }
                    menu_bar::MenuClick::Duplicate(id) => {
                        self.session.foundation.duplicate_panel = Some(id);
                    }
                }
            }
        }
        doc_tabs::show(ui, &mut self.session, doc_tabs::DocBarMode::Main);
        egui::Panel::top("toolbar").frame(bar).show(ui, |ui| {
            ui.set_min_height(seal_ui::TOOLBAR_PX);
            ui.spacing_mut().interact_size.y = seal_ui::TOOL_ROW_PX;
            let width = ui.available_width();
            ui.set_width(width);
            let layout_dirty = self.dirty;
            let mut pending = Vec::new();
            let mut actions = toolbar::ToolbarActions { layout_dirty, pending: &mut pending };
            toolbar::show(ui, width, &mut self.session, &mut actions);
            for effect in pending {
                match effect {
                    toolbar::ToolbarPending::Command(id) => self.run_command(&id),
                    toolbar::ToolbarPending::Args { id, args } => {
                        let pairs: Vec<(&str, &str)> = args
                            .iter()
                            .map(|(key, value)| (key.as_str(), value.as_str()))
                            .collect();
                        self.run_args(&id, &pairs);
                    }
                }
            }
        });
    }

    fn perform_host(&mut self) {
        let Some(action) = self.session.host_action.take() else {
            return;
        };
        match action {
            seal_editor_core::HostAction::PickFolder { create } => self.pick_folder(create),
            seal_editor_core::HostAction::OpenPath(path) => open_path(&path),
            seal_editor_core::HostAction::SaveLog => self.save_log(),
            seal_editor_core::HostAction::SaveElsewhere => self.save_elsewhere(),
            seal_editor_core::HostAction::SaveAs => self.save_as(),
            seal_editor_core::HostAction::ImportFile => self.import_layout(),
            seal_editor_core::HostAction::ImportAsset => self.import_asset(),
            seal_editor_core::HostAction::ExportFile => self.export_layout(),
            seal_editor_core::HostAction::Restart { safe_mode } => self.restart(safe_mode),
            seal_editor_core::HostAction::RunExternal { exe, args } => {
                let _ = std::process::Command::new(&exe).args(&args).spawn();
            }
            seal_editor_core::HostAction::SaveProfile { name } => self.pick_profile_export(&name),
            seal_editor_core::HostAction::OpenProfile { name } => self.pick_profile_import(&name),
            seal_editor_core::HostAction::PickMigrateTarget => self.pick_migrate_target(),
        }
    }

    fn pick_migrate_target(&mut self) {
        let Some(path) = self.take_path(pick_folder_dialog(), PendingPathPrompt::MigrateAssets)
        else {
            return;
        };
        let text = path.to_string_lossy().to_string();
        if let Err(err) = self.session.dispatch(
            "asset.copy_to_project.finish",
            &CommandArgs::new(&[("folder", text.as_str())]),
        ) {
            self.session.notify(err.to_string(), true);
        }
    }

    fn take_path(
        &mut self,
        picked: native_dialog::Pick,
        prompt: PendingPathPrompt,
    ) -> Option<PathBuf> {
        match picked {
            native_dialog::Pick::Path(path) => {
                self.clear_path_prompt();
                Some(path)
            }
            native_dialog::Pick::Cancelled => {
                self.clear_path_prompt();
                None
            }
            native_dialog::Pick::Unavailable => {
                self.begin_path_prompt(prompt);
                None
            }
        }
    }

    fn pick_folder(&mut self, create: bool) {
        let prompt = if create {
            PendingPathPrompt::NewProject
        } else {
            PendingPathPrompt::OpenProject
        };
        let Some(path) = self.take_path(pick_folder_dialog(), prompt) else {
            return;
        };
        self.apply_pick_folder(path, create);
    }

    fn apply_pick_folder(&mut self, path: PathBuf, create: bool) {
        let text = path.to_string_lossy().to_string();
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("project");
        let result = if create && self.session.wizard_open {
            self.session.set_draft(seal_editor_core::DRAFT_FOLDER, &text)
        } else if create {
            self.session.dispatch(
                "file.new-project",
                &CommandArgs::new(&[("path", text.as_str()), ("name", name)]),
            )
        } else {
            self.session
                .dispatch("file.open-project", &CommandArgs::new(&[("path", text.as_str())]))
        };
        // A typed path that doesn't open must say so where it's seen, not only in
        // the status line.
        if let Err(err) = result {
            self.session.notify(err.to_string(), true);
        }
    }

    fn save_elsewhere(&mut self) {
        let native_dialog::Pick::Path(path) = save_scene_dialog() else {
            return;
        };
        self.session.save_scene_elsewhere(path);
        self.save_attempt = 0;
        self.retry_at = None;
        match self.queue_scene() {
            Ok(true) => self.status_key = "status-saving".into(),
            Ok(false) => {}
            Err(err) => note_save(self, Err(err)),
        }
    }

    fn retry_save(&mut self) {
        let due = self.retry_at.is_some_and(|when| Instant::now() >= when);
        if !due {
            return;
        }
        self.retry_at = None;
        self.save_attempt = self.save_attempt.saturating_add(1);
        match self.queue_scene() {
            Ok(true) => self.status_key = "status-saving".into(),
            Ok(false) => {}
            Err(err) => note_save(self, Err(err)),
        }
    }

    fn save_status_item(&mut self, ui: &mut egui::Ui) {
        let label = self.session.messages.get("status-save-failed");
        let response = ui.button(egui::RichText::new(label).color(ui.visuals().warn_fg_color));
        let fault = self.save_fault;
        let attempt = self.save_attempt;
        let path = self.failed_path.clone();
        let reason = fault
            .and_then(|fault| seal_editor_core::plan_save_failure(fault, attempt))
            .map(|plan| self.session.messages.get(plan.reason_key))
            .or_else(|| self.save_error.clone())
            .unwrap_or_default();
        let retry = self.session.messages.get("save-retry");
        let save_as = self.session.messages.get("command-file-save-as");
        egui::Popup::from_toggle_button_response(&response).show(|ui| {
            if let Some(path) = &path {
                ui.label(path.display().to_string());
            }
            if !reason.is_empty() {
                ui.label(reason);
            }
            if ui.button(retry).clicked() {
                self.save_attempt = 0;
                self.retry_at = None;
                self.dirty = true;
                self.last_change = Instant::now();
                let ctx = ui.ctx().clone();
                self.save_now(&ctx);
                if let Err(err) = self.queue_scene() {
                    note_save(self, Err(err));
                }
            }
            if ui.button(save_as).clicked() {
                self.session.host_action = Some(seal_editor_core::HostAction::SaveAs);
            }
        });
        if let Some(when) = self.retry_at {
            ui.ctx().request_repaint_after(when.saturating_duration_since(Instant::now()));
        }
    }

    fn save_as(&mut self) {
        let Some(path) = self.take_path(save_file_dialog("scratch.seal"), PendingPathPrompt::SaveAs)
        else {
            return;
        };
        self.apply_save_as(path);
    }

    fn apply_save_as(&mut self, path: PathBuf) {
        let text = path.to_string_lossy().to_string();
        if let Err(err) =
            self.session.dispatch("file.save_as", &CommandArgs::new(&[("path", text.as_str())]))
        {
            self.session.notice = Some(err.to_string());
        }
    }

    fn import_asset(&mut self) {
        let Some(path) = self.take_path(open_files_dialog(), PendingPathPrompt::ImportAsset) else {
            return;
        };
        self.apply_import_asset(path);
    }

    fn apply_import_asset(&mut self, path: PathBuf) {
        let text = path.to_string_lossy().to_string();
        if let Err(err) =
            self.session.dispatch("asset.import", &CommandArgs::new(&[("path", text.as_str())]))
        {
            self.session.notify(err.to_string(), true);
        }
        self.dirty = true;
    }

    fn import_layout(&mut self) {
        let Some(path) = self.take_path(open_file_dialog(), PendingPathPrompt::ImportLayout) else {
            return;
        };
        self.apply_import_layout(path);
    }

    fn apply_import_layout(&mut self, path: PathBuf) {
        let text = path.to_string_lossy().to_string();
        if let Err(err) =
            self.session.dispatch("layout.import", &CommandArgs::new(&[("path", text.as_str())]))
        {
            self.session.notice = Some(err.to_string());
        }
    }

    fn export_layout(&mut self) {
        let Some(path) = self.take_path(save_file_dialog("layout.seal"), PendingPathPrompt::ExportLayout)
        else {
            return;
        };
        self.apply_export_layout(path);
    }

    fn apply_export_layout(&mut self, path: PathBuf) {
        let text = path.to_string_lossy().to_string();
        if let Err(err) =
            self.session.dispatch("layout.export", &CommandArgs::new(&[("path", text.as_str())]))
        {
            self.session.notice = Some(err.to_string());
        }
    }

    fn pick_profile_export(&mut self, name: &str) {
        let file = profile_dialog_name(name);
        self.path_prompt_arg = name.to_string();
        let Some(path) = self.take_path(save_file_dialog(&file), PendingPathPrompt::ExportProfile)
        else {
            return;
        };
        self.apply_export_profile(name, path);
    }

    fn apply_export_profile(&mut self, name: &str, path: PathBuf) {
        let text = path.to_string_lossy().to_string();
        if let Err(err) = self.session.dispatch(
            "keymap.profile.export",
            &CommandArgs::new(&[("name", name), ("path", text.as_str())]),
        ) {
            self.session.notify(err.to_string(), true);
        }
    }

    fn pick_profile_import(&mut self, name: &str) {
        self.path_prompt_arg = name.to_string();
        let Some(path) = self.take_path(open_file_dialog(), PendingPathPrompt::ImportProfile) else {
            return;
        };
        self.apply_import_profile(name, path);
    }

    fn apply_import_profile(&mut self, name: &str, path: PathBuf) {
        let text = path.to_string_lossy().to_string();
        let result = if name.trim().is_empty() {
            self.session
                .dispatch("keymap.profile.import", &CommandArgs::new(&[("path", text.as_str())]))
        } else {
            self.session.dispatch(
                "keymap.profile.import",
                &CommandArgs::new(&[("name", name), ("path", text.as_str())]),
            )
        };
        if let Err(err) = result {
            self.session.notify(err.to_string(), true);
        }
    }

    fn finish_path_prompt(&mut self, kind: PendingPathPrompt, path: PathBuf) {
        match kind {
            PendingPathPrompt::NewProject => self.apply_pick_folder(path, true),
            PendingPathPrompt::OpenProject => self.apply_pick_folder(path, false),
            PendingPathPrompt::SaveAs => self.apply_save_as(path),
            PendingPathPrompt::ImportLayout => self.apply_import_layout(path),
            PendingPathPrompt::ImportAsset => self.apply_import_asset(path),
            PendingPathPrompt::ExportLayout => self.apply_export_layout(path),
            PendingPathPrompt::ExportProfile => {
                let name = self.path_prompt_arg.clone();
                self.apply_export_profile(&name, path);
            }
            PendingPathPrompt::ImportProfile => {
                let name = self.path_prompt_arg.clone();
                self.apply_import_profile(&name, path);
            }
            PendingPathPrompt::MigrateAssets => {
                let text = path.to_string_lossy().to_string();
                if let Err(err) = self.session.dispatch(
                    "asset.copy_to_project.finish",
                    &CommandArgs::new(&[("folder", text.as_str())]),
                ) {
                    self.session.notify(err.to_string(), true);
                }
            }
        }
    }

    fn path_prompt_bar(&mut self, ui: &mut egui::Ui) {
        let Some(kind) = self.pending_path else {
            return;
        };
        let visuals = ui.visuals().clone();
        // The bar is the whole dialog on systems without a file picker, so it has
        // to stand out from the menu bar it sits above.
        let bar = egui::Frame::new()
            .fill(visuals.extreme_bg_color)
            .stroke(egui::Stroke::new(1.0, visuals.selection.bg_fill))
            .inner_margin(egui::Margin::symmetric(8, 6));
        let title =
            egui::RichText::new(self.session.messages.get(kind.title_key())).strong().size(14.0);
        let placeholder = self.session.messages.get(match kind {
            PendingPathPrompt::ImportAsset => "path-prompt-import-placeholder",
            _ => "path-prompt-placeholder",
        });
        let submit_label = self.session.messages.get("path-prompt-submit");
        let cancel_label = self.session.messages.get("modal-cancel");
        let mut submit = false;
        let mut cancel = false;
        // A floating box near the top centre, over the scene, where the eye already is.
        egui::Area::new(egui::Id::new("path-prompt"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 110.0])
            .show(ui.ctx(), |ui| {
                bar.show(ui, |ui| {
                    ui.set_width(680.0);
                    ui.label(title);
                    ui.horizontal(|ui| {
                        let response = seal_ui::crisp::well_edit(
                            ui,
                            egui::TextEdit::singleline(&mut self.path_prompt_text)
                                .desired_width(480.0)
                                .hint_text(placeholder),
                        );
                        if std::mem::take(&mut self.path_prompt_focus) {
                            response.request_focus();
                        }
                        // A single-line edit drops focus on the same frame Enter is pressed.
                        let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
                        if enter && (response.has_focus() || response.lost_focus()) {
                            submit = true;
                        }
                        if ui.button(submit_label).clicked() {
                            submit = true;
                        }
                        if ui.button(cancel_label).clicked() {
                            cancel = true;
                        }
                    });
                    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                        cancel = true;
                    }
                });
            });
        if cancel {
            self.clear_path_prompt();
        } else if submit {
            let text = self.path_prompt_text.trim().to_string();
            if !text.is_empty() && !std::path::Path::new(&text).is_absolute() {
                let message = self
                    .session
                    .messages
                    .format("path-prompt-absolute", &[("path", text.as_str())]);
                self.session.notify(message, true);
                self.path_prompt_focus = true;
            } else if !text.is_empty() {
                if let Some(kind) = self.pending_path.take() {
                    self.path_prompt_text.clear();
                    self.finish_path_prompt(kind, PathBuf::from(text));
                }
            }
        }
    }

    fn save_log(&mut self) {
        let native_dialog::Pick::Path(path) = save_log_dialog() else {
            return;
        };
        let text = path.to_string_lossy().to_string();
        if let Err(err) =
            self.session.dispatch("console.save", &CommandArgs::new(&[("path", text.as_str())]))
        {
            self.session.notice = Some(err.to_string());
        }
    }

    fn restart(&mut self, safe_mode: bool) {
        let Ok(exe) = std::env::current_exe() else {
            self.session.notice = Some(self.session.messages.get("restart-missing"));
            return;
        };
        let mut command = std::process::Command::new(exe);
        if safe_mode {
            command.arg("--safe-mode");
        }
        if let Some(project) = &self.session.project {
            command.arg(&project.root);
        }
        match command.spawn() {
            Ok(_) => self.session.request_close = true,
            Err(err) => {
                let detail = err.to_string();
                self.session.notice = Some(
                    self.session.messages.format("restart-failed", &[("detail", detail.as_str())]),
                );
            }
        }
    }

    fn sync_clipboard(&mut self, ctx: &egui::Context) {
        if self.session.foundation.clipboard != self.os_clipboard {
            self.os_clipboard.clone_from(&self.session.foundation.clipboard);
            if !self.os_clipboard.is_empty() {
                ctx.copy_text(self.os_clipboard.clone());
            }
        }
        let pasted = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        });
        if let Some(text) = pasted {
            if text != self.session.foundation.clipboard {
                self.session.foundation.clipboard_kind = "text".into();
                self.session.foundation.clipboard = text.clone();
                self.os_clipboard = text;
            }
        }
    }

    fn take_focus(&mut self, ctx: &egui::Context) {
        let dir = seal_editor_core::user_state_dir();
        let Some(request) = seal_editor_core::read_focus_request(&dir) else {
            return;
        };
        if request == self.seen_focus {
            return;
        }
        self.seen_focus = request;
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
            egui::UserAttentionType::Informational,
        ));
        if let Err(err) = seal_editor_core::clear_focus_request(&dir) {
            note_save(self, Err(err));
        }
    }

    fn before_command(&mut self, id: &str) {
        if matches!(
            id,
            "file.close-project" | "file.open-project" | "file.new-project" | "file.repair-project"
        ) {
            self.flush_switch();
        }
    }

    fn queue_layout_save(&mut self) {
        self.session.layout = self.dock.to_layout();
        self.file.set_layout(&self.session.layout);
        let store = self.session.layout_store_path(&self.path);
        self.save_paths.insert(SaveKind::Layout, store.clone());
        let queued = self.writer.submit(SaveKind::Layout, store, self.file.encoded().into_bytes());
        note_save(self, queued);
    }

    fn queue_project_persist(&mut self, keep_documents: bool) -> bool {
        if let Err(err) = self.session.flush_scene_journals() {
            note_save(self, Err(err));
        }
        let scene_queued = if !keep_documents || self.session.awaiting_scene_recovery() {
            false
        } else {
            match self.queue_scene() {
                Ok(queued) => queued,
                Err(err) => {
                    note_save(self, Err(err));
                    false
                }
            }
        };
        if let Some(dir) = self.session.project_state_dir() {
            let panel_path = dir.join("session.seal");
            self.save_paths.insert(SaveKind::Panel, panel_path.clone());
            let panel =
                self.writer.submit(SaveKind::Panel, panel_path, self.session.panel_state_bytes());
            note_save(self, panel);
            if keep_documents {
                let settings_path = dir.join("settings.seal");
                self.save_paths.insert(SaveKind::Settings, settings_path.clone());
                let settings = self.writer.submit(
                    SaveKind::Settings,
                    settings_path,
                    self.session.settings_bytes(),
                );
                note_save(self, settings);
            }
        }
        scene_queued
    }

    fn flush_switch(&mut self) {
        self.queue_layout_save();
        let _ = self.queue_project_persist(true);
        match self.writer.flush() {
            Ok(()) => {}
            Err(err) => note_save(self, Err(err)),
        }
    }

    /// Save everything before the window closes. Returns whether it all landed.
    fn flush_exit(&mut self) -> bool {
        self.queue_layout_save();
        let keep_documents = !self.session.discard_on_close;
        let scene_queued = self.queue_project_persist(keep_documents);
        let flushed = self.writer.flush();
        let saved_ok = flushed.is_ok();
        note_save(self, flushed);
        if saved_ok && scene_queued {
            let ticket = self.scene_ticket;
            let saved = self.session.note_scene_saved(ticket);
            note_save(self, saved);
            self.scene_inflight = false;
        }
        if saved_ok && keep_documents {
            self.session.settings_dirty = false;
        }
        if !saved_ok {
            return false;
        }
        if let Some(dir) = self.path.parent() {
            let saved = seal_editor_core::mark_clean_exit(dir);
            note_save(self, saved);
        }
        true
    }

    fn play_audio(&mut self) {
        let want_mix = self.session.audio_apply
            || self.session.world.mode != seal_editor_core::PlayMode::Editing
            || !self.session.world.play_sounds.is_empty()
            || !self.session.world.play_music.is_empty()
            || self.session.audio_request.is_some();
        if want_mix {
            sound::apply_project_audio(&self.session.foundation.settings);
            self.session.audio_apply = false;
        }
        let cues = std::mem::take(&mut self.session.world.play_sounds);
        if !cues.is_empty() {
            sound::play(self.session.world.project_dir.as_deref(), &self.session.world, &cues);
        }
        let music = std::mem::take(&mut self.session.world.play_music);
        if !music.is_empty() {
            sound::music(self.session.world.project_dir.as_deref(), &music);
        }
        if let Some(request) = self.session.audio_request.take() {
            sound::preview(&request);
        }
        if let Some(playing) = &self.session.audio_playing {
            if playing.position().is_none() {
                self.session.audio_playing = None;
            }
        }
    }

    fn sync_look(&mut self, ctx: &egui::Context) {
        let font = setting_number(&self.session, "ui.font_size", 14.0).round() as i32;
        let density =
            self.session.foundation.settings.get("ui.density").unwrap_or("comfortable").to_string();
        let compact = density == "compact";
        let follow = setting_flag(&self.session, "ui.follow_os_theme");
        let reduced = setting_flag(&self.session, "ui.reduced_motion");
        let os_dark =
            ctx.input(|input| input.raw.system_theme).map(|theme| theme == egui::Theme::Dark);
        let theme_id =
            seal_editor_core::resolved_theme(&self.session.models.theme, follow, os_dark);
        let token = seal_editor_core::theme_read_token(&theme_id);
        let play_tint = seal_editor_core::play_tint_look_token(&self.session);
        let native_ppp = ctx.native_pixels_per_point();
        if self.last_native_ppp != native_ppp {
            self.last_native_ppp = native_ppp;
            self.applied_look = None;
        }
        let look =
            (theme_id.clone(), self.session.models.scale, font, reduced, token, density, play_tint);
        if self.applied_look.as_ref() != Some(&look) {
            let theme_file = seal_editor_core::effective_editor_theme(&self.session, &theme_id);
            let (background, text, accent) = seal_editor_core::rgb_from_theme(&theme_file);
            let colors = ThemeColors {
                background,
                text,
                accent,
                look: seal_editor_core::look_from_theme(&theme_file)
                    .or_else(|| seal_editor_core::theme_look(&theme_id)),
            };
            let body = if compact { 11.0 } else { seal_editor_core::body_font_px(look.2 as f32) };
            apply_look(
                ctx,
                colors,
                look.1,
                body,
                seal_editor_core::motion_seconds(look.3),
                compact,
            );
            self.applied_look = Some(look);
        }
    }

    /// One frame of a packaged game on the HDR window.
    pub(crate) fn paint_packaged_game(&mut self, ui: &mut egui::Ui) {
        let now = Instant::now();
        let dt = now.saturating_duration_since(self.last_tick).as_secs_f32();
        self.last_tick = now;
        self.session.world.controls.frame_dt = dt.clamp(0.0, 0.25);
        let ctx = ui.ctx().clone();
        self.apply_pad_light(&ctx);
        if self.session.world.mode != seal_editor_core::PlayMode::Playing {
            self.publish_input(&ctx, false);
        }
        self.session.tick(dt);
        self.play_audio();
        self.retry_save();
        self.drain_saves();
        self.sync_look(&ctx);
        self.game_ui(ui, &ctx);
    }

    fn maybe_launch_hdr_play(&mut self) {
        let wants = seal_editor_core::window_game::play_in_window(&self.session)
            && seal_editor_core::window_game::from_session(&self.session).hdr_output;
        if !wants {
            self.hdr_play_launched = false;
            return;
        }
        if self.hdr_play_launched {
            return;
        }
        let Some(project) = self.session.project.clone() else {
            return;
        };
        match hdr_game::spawn_play_window(&project.root) {
            Ok(()) => {
                self.hdr_play_launched = true;
                seal_log::info!("HDR play window started");
                if let Err(err) =
                    self.session.dispatch("play.stop", &seal_editor_core::CommandArgs::default())
                {
                    seal_log::warn!("the editor did not leave play after the HDR window opened: {err}");
                }
            }
            Err(err) => {
                seal_log::warn!(
                    "HDR play window could not start ({err}); using the standard window"
                );
                self.hdr_play_launched = true;
            }
        }
    }

    /// While side-view gameplay runs, the movement keys drive the player instead of
    /// editor shortcuts. Returns whether those keys belong to the game this frame.
    /// The whole window of a packaged game: the running scene and nothing else.
    fn game_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.session.world.quit {
            std::process::exit(0);
        }
        let quit = ctx.input(|input| {
            input.viewport().close_requested() || input.key_pressed(egui::Key::Escape)
        });
        if quit {
            std::process::exit(0);
        }
        self.session.game_view = true;
        game_window::apply(ctx, &self.session, true, &mut self.applied_game_window);
        if self.session.world.mode == seal_editor_core::PlayMode::Editing {
            // A packaged 2D game starts straight-on in the flat view; it has no
            // saved editor view to fall back on.
            self.session.flatten_view_for_2d_scene();
            // A packaged 3D game with a play camera shows the game through that camera.
            let camera = self.session.world.entities.iter().any(|e| e.tag == "camera" && !e.hidden);
            if camera && !self.session.scene_is_2d() {
                self.session.models.viewport_view = "perspective".into();
            }
            if let Err(err) =
                self.session.dispatch("play.start", &seal_editor_core::CommandArgs::default())
            {
                seal_log::error!("the game could not start: {err}");
            }
        }
        let _ = self.read_play_input(ctx);
        let mut faults = HashMap::new();
        let mut deferred = Vec::new();
        paint_panel(&mut self.session, &self.registry, &mut faults, ui, "viewport", &mut deferred);
        if !game_window::limit_repaint(ctx, &self.session, true, &mut self.play_painted) {
            ctx.request_repaint();
        }
    }

    fn read_play_input(&mut self, ctx: &egui::Context) -> bool {
        self.feed_controls(ctx);
        let pad_keys = self.take_fresh_pad_focus();
        let world = &mut self.session.world;
        let ui_typing = Self::read_game_screen_input(world, ctx, &pad_keys);
        if ui_typing {
            world.play_input = seal_editor_core::PlayInput::default();
            return true;
        }
        // Object scripts hear every key press and release while play runs.
        if world.mode == seal_editor_core::PlayMode::Playing
            && world.has_scripts()
            && !ctx.text_edit_focused()
        {
            let keys: Vec<(String, bool)> = ctx.input(|input| {
                input
                    .events
                    .iter()
                    .filter_map(|event| match event {
                        egui::Event::Key { key, pressed, repeat: false, .. } => {
                            seal_editor_core::script_key_name(key.name())
                                .map(|name| (name, *pressed))
                        }
                        _ => None,
                    })
                    .collect()
            });
            world.play_keys.extend(keys);
        }
        if world.mode == seal_editor_core::PlayMode::Editing || world.play_player().is_none() {
            return false;
        }
        if ctx.text_edit_focused() {
            world.play_input = seal_editor_core::PlayInput::default();
            return false;
        }
        // A clicked button keeps focus, and egui would turn Space into a click on it
        // and arrows into focus moves. The game owns those keys while it runs.
        if let Some(id) = ctx.memory(|memory| memory.focused()) {
            ctx.memory_mut(|memory| memory.surrender_focus(id));
        }
        // `feed_controls` already turned the keys and pad into moves.
        true
    }

    /// Keys and the gamepad into the game's controls, and the game's rumble
    /// back out to the pad. Typing in a text box or holding Ctrl/Alt sends
    /// the game no keys.
    fn feed_controls(&mut self, ctx: &egui::Context) {
        let playing = self.session.world.mode == seal_editor_core::PlayMode::Playing;
        if !playing {
            if let Some(pads) = self.pads.as_mut() {
                pads.rumble(0.0, 0.0);
            }
            self.capture_pad_button(ctx);
            return;
        }
        let keys: Vec<String> = ctx.input(|input| {
            if input.modifiers.ctrl || input.modifiers.command || input.modifiers.alt {
                return Vec::new();
            }
            // A tap can press and release inside one frame, so count presses too.
            // Bindings use `arrowright`; egui names that key "Right".
            let mut names: Vec<String> =
                input.keys_down.iter().map(|k| seal_ui::binding_key_name(*k)).collect();
            for event in &input.events {
                if let egui::Event::Key { key, pressed: true, .. } = event {
                    names.push(seal_ui::binding_key_name(*key));
                }
            }
            names
        });
        let pointer_world = ctx
            .data(|data| data.get_temp::<[f32; 2]>(egui::Id::new("seal-pointer-world")))
            .unwrap_or([0.0, 0.0]);
        let mouse_wheel = ctx.input(|input| input.smooth_scroll_delta.y);
        let typing = ctx.text_edit_focused() || self.session.world.game_screens.typing();
        {
            let controls = &mut self.session.world.controls;
            controls.set_keys(if typing { Vec::new() } else { keys }.iter().map(String::as_str));
            controls.mouse_wheel = mouse_wheel;
            controls.pointer_world = pointer_world;
        }
        {
            let pads = self.pads.get_or_insert_with(gamepad::Pads::new);
            let controls = &mut self.session.world.controls;
            controls.pad_connected = pads.poll(&mut controls.raw);
            controls.extra_pads = pads.extra().to_vec();
        }
        self.publish_input(ctx, true);
        {
            let motors = self.session.world.controls.motors;
            if let Some(pads) = self.pads.as_mut() {
                pads.rumble(motors.0, motors.1);
            }
        }
        self.session.world.read_controls();
    }

    /// Fill the live input reading. `into_controls` also writes the game's axes.
    fn publish_input(&mut self, ctx: &egui::Context, into_controls: bool) {
        use seal_editor_core::seal_input::{PadBackend, RawInput};
        let (pointer, pen, devices) = input_devices::read_frame(ctx);
        let (motion, light_note, pad_devices) = {
            let pads = self.pads.get_or_insert_with(gamepad::Pads::new);
            if !into_controls {
                let mut scratch = RawInput::default();
                let _ = pads.poll(&mut scratch);
            }
            (pads.motion(), pads.light_note(), pads.device_names())
        };
        if into_controls {
            let use_raw = self.session.foundation.settings.get(seal_editor_core::MOUSE_LOOK_RAW)
                == Some("true");
            input_devices::write_controls(
                &mut self.session.world.controls,
                &pointer,
                &pen,
                &motion,
                &light_note,
                use_raw,
            );
        }
        input_devices::store(
            ctx,
            input_devices::live(&pointer, &pen, devices, pad_devices, &motion, &light_note),
        );
    }

    /// Send a script's light-bar colour, or the project colour when it changed.
    fn apply_pad_light(&mut self, _ctx: &egui::Context) {
        use seal_editor_core::seal_input::{parse_pad_light, PadBackend};
        let scripted = self.session.world.take_pad_light();
        let forced = scripted.is_some();
        let setting =
            self.session.foundation.settings.get(seal_editor_core::PAD_LIGHT).unwrap_or("");
        let from_setting = parse_pad_light(setting).ok().flatten();
        let Some(rgb) = scripted.or(from_setting) else {
            return;
        };
        if !forced && self.pad_light_sent == Some(rgb) {
            return;
        }
        let result = {
            let pads = self.pads.get_or_insert_with(gamepad::Pads::new);
            pads.set_light_bar(rgb)
        };
        match result {
            Ok(()) => self.session.world.controls.pad_light_note.clear(),
            Err(err) => self.session.world.controls.pad_light_note = err,
        }
        self.pad_light_sent = Some(rgb);
    }

    /// While the bindings editor waits for a key, the next key or newly
    /// pressed pad button is bound. This runs before keyboard shortcuts, and
    /// the key is taken out of the frame's input so no shortcut also fires.
    fn capture_pad_button(&mut self, ctx: &egui::Context) {
        use seal_editor_core::input_editor::capturing;
        if capturing(&self.session).is_none() {
            self.pad_held = None;
            return;
        }
        let key = ctx.input_mut(|input| {
            let at = input.events.iter().position(|event| {
                matches!(event, egui::Event::Key { pressed: true, repeat: false, .. })
            })?;
            let egui::Event::Key { key, .. } = input.events.remove(at) else { return None };
            input
                .events
                .retain(|event| !matches!(event, egui::Event::Key { key: k, .. } if *k == key));
            input.events.retain(|event| !matches!(event, egui::Event::Text(_)));
            input.keys_down.remove(&key);
            Some(key)
        });
        let pads = self.pads.get_or_insert_with(gamepad::Pads::new);
        let mut raw = seal_editor_core::input_editor::RawInput::default();
        pads.poll(&mut raw);
        let now: Vec<String> = raw.buttons().map(|b| b.name().to_string()).collect();
        // Buttons already held when the wait began do not count.
        let held = self.pad_held.get_or_insert_with(|| now.clone());
        let fresh = now.iter().find(|b| !held.contains(b)).cloned();
        *held = now;
        let result = match (key, fresh) {
            (Some(egui::Key::Escape), _) => {
                self.session.dispatch("asset.input_cancel", &CommandArgs::new(&[]))
            }
            (Some(key), _) => {
                let name = format!("key.{}", key.name().to_ascii_lowercase());
                self.session
                    .dispatch("asset.input_bind", &CommandArgs::new(&[("button", name.as_str())]))
            }
            (None, Some(button)) => self
                .session
                .dispatch("asset.input_bind", &CommandArgs::new(&[("button", button.as_str())])),
            (None, None) => return,
        };
        self.pad_held = None;
        if let Err(err) = result {
            seal_log::warn!("rebind failed: {err}");
        }
        self.dirty = true;
        self.last_change = Instant::now();
    }

    /// Pad buttons that just went down, named the way play screens move focus.
    fn take_fresh_pad_focus(&mut self) -> Vec<&'static str> {
        let world = &self.session.world;
        if world.mode != seal_editor_core::PlayMode::Playing
            || !world.game_screens.active()
            || world.game_screens.typing()
        {
            self.pad_ui_down.clear();
            return Vec::new();
        }
        let now: Vec<String> = world
            .controls
            .raw
            .buttons()
            .filter(|button| button.name().starts_with("pad."))
            .map(|button| button.name().to_string())
            .collect();
        let fresh = now
            .iter()
            .filter(|name| !self.pad_ui_down.iter().any(|held| held == *name))
            .filter_map(|name| pad_focus_key(name))
            .collect();
        self.pad_ui_down = now;
        fresh
    }

    /// Pointer, keys and typed text for the game screens shown in play.
    /// Returns whether a text field on them is taking the keyboard.
    fn read_game_screen_input(
        world: &mut seal_editor_core::World,
        ctx: &egui::Context,
        pad_keys: &[&str],
    ) -> bool {
        if world.mode != seal_editor_core::PlayMode::Playing || !world.game_screens.active() {
            return false;
        }
        let rect: Option<egui::Rect> =
            ctx.data(|d| d.get_temp(egui::Id::new(seal_ui::GAME_SCREEN_RECT)));
        let screens = &mut world.game_screens;
        if let Some(rect) = rect {
            let size = [rect.width(), rect.height()];
            if screens.size != size {
                screens.resize(size);
            }
        }
        let events = ctx.input(|input| input.events.clone());
        let shift = ctx.input(|input| input.modifiers.shift);
        let local = |p: egui::Pos2| rect.map(|r| [p.x - r.min.x, p.y - r.min.y]);
        for event in events {
            match event {
                egui::Event::PointerMoved(p) => {
                    if let Some(p) = local(p) {
                        screens.pointer_move(p);
                    }
                }
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    ..
                } => {
                    if let (Some(p), Some(r)) = (local(pos), rect) {
                        if pressed && r.contains(pos) {
                            screens.pointer_down(p);
                        } else if !pressed {
                            screens.pointer_up(p);
                        }
                    }
                }
                egui::Event::MouseWheel { delta, .. } => {
                    let at = ctx.input(|input| input.pointer.hover_pos());
                    if let (Some(p), Some(r)) = (at.and_then(local), rect) {
                        if at.is_some_and(|a| r.contains(a)) {
                            screens.wheel(p, -delta.y * 40.0);
                        }
                    }
                }
                egui::Event::Text(text) if screens.typing() => screens.type_text(&text),
                egui::Event::Key { key, pressed: true, .. } => {
                    let name = match key {
                        egui::Key::ArrowUp => "Up".to_string(),
                        egui::Key::ArrowDown => "Down".to_string(),
                        egui::Key::ArrowLeft => "Left".to_string(),
                        egui::Key::ArrowRight => "Right".to_string(),
                        egui::Key::Tab if shift => "Shift+Tab".to_string(),
                        other => other.name().to_string(),
                    };
                    screens.key(&name);
                }
                _ => {}
            }
        }
        for key in pad_keys {
            screens.key(key);
        }
        screens.typing()
    }

    fn nudge_from_chord(&mut self, chord: &seal_editor_core::Chord) -> bool {
        if self.session.foundation.layout_locked || !chord.ctrl || !chord.alt || !chord.shift {
            return false;
        }
        let (vertical, delta) = match chord.key.as_str() {
            "arrowleft" => (false, -0.05),
            "arrowright" => (false, 0.05),
            "arrowup" => (true, -0.05),
            "arrowdown" => (true, 0.05),
            _ => return false,
        };
        let id = self.session.foundation.focused_panel_id.clone();
        self.dock.nudge_split(&id, vertical, delta);
        true
    }

    fn handle_dock_request(&mut self) {
        let Some(request) = self.session.foundation.dock_request.take() else {
            return;
        };
        let id = self.session.foundation.focused_panel_id.clone();
        if request == seal_editor_core::DockSide::Reload {
            self.panel_faults.remove(&id);
            return;
        }
        if self.session.foundation.layout_locked {
            return;
        }
        self.dock.apply_side(&id, request);
    }

    fn toast_stack(&mut self, ctx: &egui::Context) {
        let timeout_ms = self
            .session
            .foundation
            .settings
            .get("notifications.timeout_ms")
            .and_then(|text| text.parse::<u64>().ok())
            .unwrap_or(4000)
            .max(1);
        let now = self.session.uptime_ms;
        let shown: Vec<_> = self
            .session
            .foundation
            .notices
            .visible(now, timeout_ms)
            .iter()
            .map(|notice| (*notice).clone())
            .collect();
        if shown.is_empty() {
            return;
        }
        let mut next_hide = None::<u64>;
        for notice in &shown {
            if notice.error {
                continue;
            }
            let gone_at = notice.at_ms.saturating_add(timeout_ms);
            if gone_at > now {
                let remain = gone_at - now;
                next_hide = Some(next_hide.map_or(remain, |due| due.min(remain)));
            }
        }
        if let Some(remain) = next_hide {
            ctx.request_repaint_after(Duration::from_millis(remain.clamp(50, 500)));
        }
        let messages = &self.session.messages;
        let mut close = None;
        egui::Area::new(egui::Id::new("toast-stack"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -48.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                const TOAST_W: f32 = 340.0;
                ui.set_width(TOAST_W);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    for notice in shown {
                        let stroke = if notice.error {
                            ui.visuals().error_fg_color
                        } else {
                            ui.visuals().widgets.noninteractive.bg_stroke.color
                        };
                        let frame = egui::Frame::popup(ui.style())
                            .corner_radius(0)
                            .stroke(egui::Stroke::new(1.0, stroke))
                            .show(ui, |ui| {
                                // Every toast is the same width, so the stack lines up.
                                ui.set_width(TOAST_W - 2.0 * 8.0);
                                ui.horizontal_top(|ui| {
                                    ui.vertical(|ui| {
                                        ui.set_width(TOAST_W - 2.0 * 8.0 - 22.0);
                                        ui.label(&notice.text);
                                    });
                                    let (x_rect, x) = ui.allocate_exact_size(
                                        egui::vec2(18.0, 18.0),
                                        egui::Sense::click(),
                                    );
                                    seal_ui::crisp::plate(ui, x_rect, &x, false, false);
                                    paint_icon(
                                        ui.painter(),
                                        x_rect.center(),
                                        Icon::ViewClose,
                                        12.0,
                                        ui.visuals().text_color(),
                                    );
                                    let x = x.on_hover_text(messages.get("doc-close"));
                                    if x.clicked() {
                                        close = Some(notice.id);
                                    }
                                });
                            });
                        let _ = frame;
                    }
                });
            });
        if let Some(id) = close {
            self.session.foundation.notices.dismiss(id);
        }
    }
}

impl Shell {
    pub fn detached_scene_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx();
        if !self.session.scene_tab_detached {
            ctx.send_viewport_cmd_to(
                egui::ViewportId::from_hash_of("seal-scene-document"),
                egui::ViewportCommand::Close,
            );
            return;
        }
        if ctx.input(|input| input.viewport().close_requested()) {
            self.session.dock_scene_tab();
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            return;
        }
        let marks = self.session.dirty_marks(self.status_key == "status-saved");
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(marks.title));
        doc_tabs::show(ui, &mut self.session, doc_tabs::DocBarMode::Detached);
        let gap = egui::Frame::new().fill(ui.visuals().extreme_bg_color);
        egui::CentralPanel::default().frame(gap).show(ui, |ui| {
            ui.set_min_height(320.0);
            let mut deferred: Vec<(String, seal_editor_core::CommandArgs)> = Vec::new();
            paint_panel(
                &mut self.session,
                &self.registry,
                &mut self.panel_faults,
                ui,
                "viewport",
                &mut deferred,
            );
            for (command, args) in deferred {
                if let Err(err) = self.session.dispatch(&command, &args) {
                    self.session.notice = Some(err.to_string());
                }
            }
        });
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let now = Instant::now();
        let dt = now.saturating_duration_since(self.last_tick).as_secs_f32();
        self.last_tick = now;
        self.session.world.controls.frame_dt = dt.clamp(0.0, 0.25);
        self.apply_pad_light(ui.ctx());
        if self.session.world.mode != seal_editor_core::PlayMode::Playing {
            self.publish_input(ui.ctx(), false);
        }
        self.session.tick(dt);
        if seal_editor_core::import_pipeline::job_active(&self.session) {
            ui.ctx().request_repaint();
        }
        seal_editor_core::note_worker_budget(&self.session.foundation.settings);
        self.play_audio();
        self.retry_save();
        self.drain_saves();
        self.install_mac_menu();
        #[cfg(target_os = "macos")]
        if let Some(menu) = self.mac_menu.as_ref() {
            menu.poll(&mut |action| self.run_mac_menu_action(action));
        }
        if self.session.world.mode == seal_editor_core::PlayMode::Playing {
            // Without this the window only redraws on input, and play looks frozen.
            if !game_window::limit_repaint(
                ui.ctx(),
                &self.session,
                self.game,
                &mut self.play_painted,
            ) {
                ui.ctx().request_repaint();
            }
        }
        let ctx = ui.ctx().clone();
        self.maybe_restore_window(&ctx, frame);
        self.last_monitor = frame
            .winit_window()
            .and_then(|window| window_placement::current_monitor_name(window.as_ref()));
        self.sync_clipboard(&ctx);
        self.take_focus(&ctx);
        self.sync_responsive_layout(&ctx);
        let marks = self.session.dirty_marks(self.status_key == "status-saved");
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(marks.title));
        self.sync_look(&ctx);
        if self.frames > 0 {
            self.session.foundation.loading = false;
        }
        self.take_shot(&ctx);
        self.feed_remote_screenshot(&ctx);
        if self.screenshot_done {
            std::process::exit(0);
        }
        if self.fullscreen_sent != self.session.foundation.fullscreen {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(
                self.session.foundation.fullscreen,
            ));
            self.fullscreen_sent = self.session.foundation.fullscreen;
        }
        let dropped: Vec<std::path::PathBuf> = ctx.input(|input| {
            input.raw.dropped_files.iter().map(|file| file.path().to_path_buf()).collect()
        });
        for path in dropped {
            if let Err(err) = self.session.accept_drop(&path) {
                self.session.notice = Some(err.to_string());
            }
        }
        if !self.game {
            self.maybe_launch_hdr_play();
        }
        game_window::apply(&ctx, &self.session, self.game, &mut self.applied_game_window);
        play_extra_windows::apply(&ctx, &self.session, &mut self.play_extra_open);
        play_extra_windows::attention(&ctx, &mut self.session);
        if self.game {
            self.game_ui(ui, &ctx);
            return;
        }
        let close = ctx.input(|input| input.viewport().close_requested());
        if close {
            self.session.save_failed = self.save_fault.is_some();
            self.session.begin_close();
        }
        if self.session.layout_epoch != self.applied_epoch {
            self.dock = DockHost::from_layout(&self.session.layout);
            self.applied_epoch = self.session.layout_epoch;
            self.dirty = true;
            self.last_change = Instant::now();
        }
        let gameplay_keys = self.read_play_input(&ctx);
        // Esc stops the sprite brush wherever the pointer is, unless a text box
        // or a menu wants the key first.
        if !ctx.text_edit_focused()
            && !egui::Popup::is_any_open(&ctx)
            && ctx.input(|input| input.key_pressed(egui::Key::Escape))
            && self.session.draft("place-brush").is_some_and(|brush| !brush.is_empty())
        {
            if let Err(err) = self.session.set_draft("place-brush", "") {
                seal_log::warn!("brush stop failed: {err}");
            }
        }
        if !self.session.palette_open {
            let by_physical = setting_flag(&self.session, "keymap.by_physical_key");
            let chord = pressed_chord(ui, by_physical)
                .filter(|chord| !(gameplay_keys && is_gameplay_key(chord)))
                .filter(|chord| !(ctx.text_edit_focused() && is_text_editing_key(chord)));
            if let Some(chord) = chord {
                if self.nudge_from_chord(&chord) {
                    self.dirty = true;
                    self.last_change = Instant::now();
                } else if let Some(id) = self.session.on_chord(&chord) {
                    if id == "viewport.bookmark_store" || id == "viewport.bookmark_recall" {
                        let slot = chord.key.clone();
                        self.run_args(&id, &[("slot", slot.as_str())]);
                    } else {
                        self.run_command(&id);
                    }
                } else if self.session.foundation.focused_panel_id.starts_with("outliner") {
                    if let Some(letter) = typeahead_key(&chord) {
                        let mut prefix = self.session.models.outliner_typeahead.clone();
                        prefix.push(letter);
                        let _ = self.session.dispatch(
                            "outliner.typeahead",
                            &seal_editor_core::CommandArgs::new(&[("prefix", prefix.as_str())]),
                        );
                        self.dirty = true;
                        self.last_change = Instant::now();
                    }
                }
            }
        }
        self.session.foundation.loading = false;
        self.path_prompt_bar(ui);
        self.top_bars(ui);
        self.status_bar(ui);
        if self.session.palette_open {
            let hits = seal_editor_core::palette_search_hits(&self.session);
            let rows: Vec<(String, String)> =
                hits.iter().map(|hit| (hit.title.clone(), hit.shortcut.clone())).collect();
            let ids: Vec<String> = hits.into_iter().map(|hit| hit.id).collect();
            let placeholder = self.session.messages.get("palette-placeholder");
            let pin = self.session.messages.get("picker-pin");
            let saved = seal_editor_core::saved_searches(&self.session);
            let event = show_palette(
                &ctx,
                &mut self.session.palette_query,
                &rows,
                self.session.palette_index,
                &placeholder,
                &pin,
                &saved,
            );
            match event {
                PaletteEvent::MoveUp => {
                    self.session.palette_index = self.session.palette_index.saturating_sub(1);
                }
                PaletteEvent::MoveDown => {
                    if self.session.palette_index + 1 < ids.len() {
                        self.session.palette_index += 1;
                    }
                }
                PaletteEvent::Close => self.session.palette_open = false,
                PaletteEvent::Pin => {
                    self.run_args("search.pin", &[]);
                    self.dirty = true;
                    self.last_change = Instant::now();
                }
                PaletteEvent::UseSaved(index) => {
                    if let Some(query) = saved.get(index) {
                        self.session.palette_query = query.clone();
                        self.session.palette_index = 0;
                    }
                }
                PaletteEvent::Run | PaletteEvent::Pick(_) => {
                    let index = match event {
                        PaletteEvent::Pick(index) => index,
                        _ => self.session.palette_index,
                    };
                    if let Some(id) = ids.get(index).cloned() {
                        self.session.palette_open = false;
                        if let Some((kind, target)) = seal_editor_core::split_search_activation(&id)
                        {
                            self.run_args(
                                "search.activate",
                                &[("kind", kind.as_str()), ("target", target.as_str())],
                            );
                        } else {
                            self.run_command(&id);
                        }
                    }
                }
                PaletteEvent::Idle => {}
            }
        }
        // A finished or cancelled New Project wizard closes its tab.
        if self.wizard_was_open && !self.session.wizard_open {
            self.dock.apply_side(seal_editor_panels::WIZARD, seal_editor_core::DockSide::Close);
        }
        self.wizard_was_open = self.session.wizard_open;
        if let Some(id) = self.session.reveal_panel.take() {
            if self.session.foundation.sidebar_pins.iter().any(|pin| pin == &id) {
                self.session.foundation.sidebar_open = Some(id);
            } else {
                self.dock.show_panel(&id);
            }
        }
        self.handle_dock_request();
        if let Some(source) = self.session.foundation.duplicate_panel.take() {
            if let Some(new_id) = self.dock.duplicate_tab(&source) {
                let base = panel_base(&source);
                if base == "viewport" {
                    seal_editor_core::ensure_viewport_tab(&mut self.session.models, &new_id);
                }
                if base == "inspector" {
                    let primary = self.session.models.selection.clone();
                    if let Some(entity) = self.session.models.nodes.iter().find_map(|node| {
                        if node.id == "root" {
                            return None;
                        }
                        if primary.as_deref() == Some(node.id.as_str()) {
                            return None;
                        }
                        Some(node.id.clone())
                    }) {
                        self.session.models.inspector_locks.insert(new_id.clone(), entity);
                    }
                }
                self.dock.show_panel(&new_id);
                self.session.foundation.focused_panel_id = new_id;
                self.dirty = true;
                self.last_change = Instant::now();
            }
        }
        let before = self.dock.to_layout();
        let titles: Vec<(String, String)> = self
            .registry
            .iter()
            .map(|panel| (panel.id.clone(), self.session.messages.get(&panel.title_key)))
            .collect();
        let gap = egui::Frame::new().fill(ui.visuals().extreme_bg_color);
        let mut deferred: Vec<(String, seal_editor_core::CommandArgs)> = Vec::new();
        self.dock.set_badge("viewport", "");
        let maximize = self.session.foundation.maximized_panel;
        let mut dock_menu = TabMenuResult::None;
        let mut empty_dock_reset = false;
        if maximize {
            if let Some(id) = focused_dock_panel(&self.dock) {
                self.session.foundation.focused_panel_id = id;
            }
        }
        // Reserve a column for pinned sidebar panels so the rail sits under the
        // top bars instead of over the menu and document tabs.
        let rail_top = if self.session.foundation.sidebar_pins.is_empty() {
            None
        } else {
            let rail_fill = ui.visuals().extreme_bg_color;
            let rail = egui::Panel::left("sidebar-rail-space")
                .resizable(false)
                .exact_size(26.0)
                .frame(egui::Frame::new().fill(rail_fill))
                .show_separator_line(false)
                .show(ui, |_ui| {});
            Some(rail.response.rect.top())
        };
        ui.ctx().data_mut(|d| match rail_top {
            Some(top) => {
                d.insert_temp(egui::Id::new("seal-sidebar-rail-top"), top);
            }
            None => {
                d.remove::<f32>(egui::Id::new("seal-sidebar-rail-top"));
            }
        });
        egui::CentralPanel::default().frame(gap).show(ui, |ui| {
            if maximize {
                let id = self.session.foundation.focused_panel_id.clone();
                paint_panel(
                    &mut self.session,
                    &self.registry,
                    &mut self.panel_faults,
                    ui,
                    &id,
                    &mut deferred,
                );
            } else {
                let tab_menu = [
                    self.session.messages.get("menu-close"),
                    self.session.messages.get("menu-close-others"),
                    self.session.messages.get("menu-close-right"),
                    self.session.messages.get("menu-float"),
                    self.session.messages.get("menu-move-sidebar"),
                    self.session.messages.get("menu-maximize"),
                    self.session.messages.get("menu-lock-layout"),
                ];
                let scene_dirty = self.session.scene_dirty;
                let settings_dirty = self.session.settings_dirty;
                let doc_dirty: Vec<(String, bool)> = self
                    .session
                    .documents
                    .iter()
                    .map(|doc| (doc.id.clone(), doc.dirty()))
                    .collect();
                let mut missing_titles: HashMap<String, String> = HashMap::new();
                for id in self.dock.tab_ids() {
                    let kind = panel_base(&id);
                    if titles.iter().all(|(panel_id, _)| panel_id.as_str() != kind) {
                        let label =
                            self.session.messages.format("unknown-panel", &[("id", id.as_str())]);
                        missing_titles.insert(id, label);
                    }
                }
                let no_project = self.session.project.is_none();
                let untitled = self.session.messages.get("scene-untitled");
                let session = &mut self.session;
                let registry = &self.registry;
                let faults = &mut self.panel_faults;
                let tab_badges = self.dock.tab_badges().clone();
                let dock_state = &mut self.dock.state;
                let layout_locked = session.foundation.layout_locked;
                dock_menu = show_dock_with_badges(
                    ui,
                    dock_state,
                    &|id| {
                        let kind = panel_base(id);
                        let base = if kind == "viewport" && no_project {
                            untitled.clone()
                        } else {
                            titles
                                .iter()
                                .find(|(panel_id, _)| panel_id == kind)
                                .map(|(_, title)| title.clone())
                                .unwrap_or_else(|| {
                                    missing_titles
                                        .get(id)
                                        .cloned()
                                        .unwrap_or_else(|| kind.to_string())
                                })
                        };
                        let dirty = seal_editor_core::panel_is_dirty(
                            id,
                            scene_dirty,
                            settings_dirty,
                            doc_dirty.iter().map(|(doc_id, dirty)| (doc_id.as_str(), *dirty)),
                        );
                        let labeled = seal_editor_core::panel_tab_title(&base, id);
                        seal_editor_core::tab_label(&labeled, dirty, "")
                    },
                    &tab_menu,
                    layout_locked,
                    &mut |ui, id| {
                        session.foundation.focused_panel_id = id.to_string();
                        if session.scene_tab_detached && panel_base(id) == "viewport" {
                            let tip = session.messages.get("doc-viewport-detached");
                            ui.centered_and_justified(|ui| {
                                ui.label(tip);
                            });
                            return;
                        }
                        paint_panel(session, registry, faults, ui, id, &mut deferred);
                    },
                    Some(&tab_badges),
                );
                if self.dock.state.main_surface().is_empty() {
                    // Tell the user how to get panels back instead of showing only
                    // the watermark.
                    let title = self.session.messages.get("dock-empty");
                    let hint = self.session.messages.get("dock-empty-hint");
                    let reset = self.session.messages.get("dock-empty-reset");
                    let area = ui.max_rect();
                    ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
                        let mut frame = seal_ui::PanelFrame::new(ui);
                        if frame.empty_with_actions(&title, &hint, &[reset.as_str()]).is_some() {
                            empty_dock_reset = true;
                        }
                    });
                }
            }
        });
        if empty_dock_reset {
            self.run_command("window.reset-layout");
        }
        match dock_menu {
            TabMenuResult::Maximize => {
                self.session.foundation.maximized_panel = !self.session.foundation.maximized_panel;
            }
            TabMenuResult::ToggleLayoutLock => {
                self.session.foundation.layout_locked = !self.session.foundation.layout_locked;
                let value = if self.session.foundation.layout_locked { "true" } else { "false" };
                let _ = self.session.set_draft("layoutLocked", value);
            }
            TabMenuResult::PinSidebar(id) => self.pin_sidebar(&id),
            TabMenuResult::None => {}
        }
        self.sidebar_overlay(&ctx);
        for (command, args) in deferred {
            self.flush_switch();
            if let Err(err) = self.session.dispatch(&command, &args) {
                self.session.notice = Some(err.to_string());
            }
        }
        if !maximize {
            let after = self.dock.to_layout();
            if after != before {
                self.dirty = true;
                self.last_change = Instant::now();
            }
        }
        if self.session.request_save {
            self.session.request_save = false;
            self.save_now(&ctx);
        }
        let debounce_ms = self
            .session
            .foundation
            .settings
            .get("autosave.debounce_ms")
            .and_then(|text| text.parse::<u128>().ok())
            .unwrap_or(2000);
        let max_interval_ms = self
            .session
            .foundation
            .settings
            .get("autosave.max_interval_ms")
            .and_then(|text| text.parse::<u128>().ok())
            .unwrap_or(30_000);
        if self.dirty
            && self.last_change.elapsed() >= Duration::from_millis(debounce_ms.max(500) as u64)
        {
            self.save_now(&ctx);
        }
        if self.session.scene_dirty
            && !self.session.awaiting_scene_recovery()
            && self.session.world.mode == seal_editor_core::PlayMode::Editing
        {
            if self.scene_seen != self.session.scene_edits {
                self.scene_seen = self.session.scene_edits;
                self.scene_idle = Some(Instant::now());
                if self.scene_since.is_none() {
                    self.scene_since = Some(Instant::now());
                }
            }
            let idle_ms = self.scene_idle.map(|start| start.elapsed().as_millis()).unwrap_or(0);
            let dirty_ms = self.scene_since.map(|start| start.elapsed().as_millis()).unwrap_or(0);
            if !self.scene_inflight
                && seal_editor_core::autosave_due(idle_ms, dirty_ms, debounce_ms, max_interval_ms)
            {
                match self.queue_scene() {
                    Ok(true) => {
                        self.status_key = "status-saving".into();
                        ctx.request_repaint_after(Duration::from_millis(50));
                    }
                    Ok(false) => {}
                    Err(err) => {
                        self.scene_idle = Some(Instant::now());
                        note_save(self, Err(err));
                    }
                }
            } else {
                ctx.request_repaint_after(Duration::from_millis(if self.scene_inflight {
                    50
                } else {
                    debounce_ms.max(500) as u64
                }));
            }
        } else if !self.session.scene_dirty {
            self.scene_idle = None;
            self.scene_since = None;
        }
        if self.session.settings_dirty
            && self.last_change.elapsed() >= Duration::from_millis(debounce_ms.max(500) as u64)
        {
            let path = self
                .session
                .recent_path
                .as_ref()
                .and_then(|path| path.parent().map(|dir| dir.join("settings.seal")));
            if let Some(path) = path {
                let bytes = self.session.settings_bytes();
                self.save_paths.insert(SaveKind::Settings, path.clone());
                match self.writer.submit(SaveKind::Settings, path, bytes) {
                    Ok(()) => self.status_key = "status-saving".into(),
                    Err(err) => {
                        self.last_change = Instant::now();
                        note_save(self, Err(err));
                    }
                }
            }
        }
        if self.session.retry_save_now {
            self.session.retry_save_now = false;
            self.save_attempt = 0;
            self.retry_at = Some(Instant::now());
        }
        menu_bar::resize_borders(&ctx);
        self.close_drawers_on_escape(&ctx);
        self.show_drawers(&ctx);
        self.toast_stack(&ctx);
        if let Some(modal) = self.session.foundation.modal.clone() {
            let mut confirm = false;
            let mut alternate = false;
            let mut cancel = false;
            let about = modal.kind == seal_editor_core::ModalKind::About;
            let visuals = ctx.global_style().visuals.clone();
            let frame = egui::Frame::new()
                .fill(visuals.window_fill)
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_black_alpha(200)))
                .shadow(egui::epaint::Shadow {
                    offset: [0, 6],
                    blur: 18,
                    spread: 0,
                    color: egui::Color32::from_black_alpha(110),
                })
                .corner_radius(0);
            egui::Window::new(modal.title.clone())
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 40.0))
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .frame(frame)
                .min_width(340.0)
                .show(&ctx, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                    // Raised header strip with the dialog title.
                    let (head, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width().max(340.0), 28.0),
                        egui::Sense::hover(),
                    );
                    seal_ui::crisp::raised(
                        &mut ui.painter(),
                        head,
                        ui.visuals(),
                        seal_ui::crisp::Raised::Normal,
                    );
                    ui.painter().text(
                        egui::pos2(head.left() + 12.0, head.center().y),
                        egui::Align2::LEFT_CENTER,
                        &modal.title,
                        egui::FontId::proportional(13.0),
                        ui.visuals().strong_text_color(),
                    );
                    egui::Frame::new()
                        .inner_margin(egui::Margin { left: 14, right: 14, top: 8, bottom: 12 })
                        .show(ui, |ui| {
                            ui.set_max_width(420.0);
                            ui.label(&modal.body);
                            ui.add_space(8.0);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if !modal.confirm.is_empty()
                                        && seal_ui::crisp::primary_button(
                                            ui,
                                            egui::Button::new(&modal.confirm)
                                                .min_size(egui::vec2(72.0, 24.0)),
                                        )
                                        .clicked()
                                    {
                                        confirm = true;
                                    }
                                    if !modal.alternate.is_empty()
                                        && seal_ui::crisp::button(
                                            ui,
                                            egui::Button::new(&modal.alternate)
                                                .min_size(egui::vec2(72.0, 24.0)),
                                            true,
                                            false,
                                        )
                                        .clicked()
                                    {
                                        alternate = true;
                                    }
                                    if !modal.cancel.is_empty()
                                        && seal_ui::crisp::button(
                                            ui,
                                            egui::Button::new(&modal.cancel)
                                                .min_size(egui::vec2(72.0, 24.0)),
                                            true,
                                            false,
                                        )
                                        .clicked()
                                    {
                                        cancel = true;
                                    }
                                },
                            );
                        });
                    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                        if about {
                            confirm = true;
                        } else {
                            cancel = true;
                        }
                    }
                });
            if confirm {
                self.session.answer_modal(true);
            } else if alternate {
                self.session.choose_modal(seal_editor_core::ModalChoice::Alternate);
            } else if cancel {
                self.session.answer_modal(false);
            }
        }
        self.perform_host();
        if self.session.request_close {
            self.session.request_close = false;
            if self.flush_exit() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                // The save did not land: stay open and ask (Retry or Save As).
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.session.save_failed = true;
                self.session.begin_close();
            }
        } else if close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if self.dirty {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    pub fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.flush_exit();
    }
}

static PROCESS_START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
static READY_LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Call first thing in `main`; the "ready" log line reports the time since then.
pub fn mark_process_start() {
    let _ = PROCESS_START.get_or_init(Instant::now);
}

/// Logs "ready in N ms" once, when the first frame is built.
fn log_ready_once() {
    if READY_LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let ms = PROCESS_START.get().map(|start| start.elapsed().as_millis()).unwrap_or(0);
    seal_log::info!("ready in {ms} ms");
}

impl eframe::App for ShellApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        log_ready_once();
        let ctx = ui.ctx().clone();
        if self.inner.borrow().session.scene_tab_detached {
            let shell_ref = self.inner.clone();
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("seal-scene-document"),
                egui::ViewportBuilder::default()
                    .with_title("Untitled - Seal")
                    .with_inner_size([880.0, 660.0])
                    .with_decorations(true)
                    .with_position(egui::pos2(1040.0, 80.0)),
                move |child_ui, _class| {
                    let Ok(mut shell) = shell_ref.try_borrow_mut() else {
                        return;
                    };
                    shell.detached_scene_ui(child_ui);
                },
            );
        }
        let mut shell = self.inner.borrow_mut();
        shell.ui(ui, frame);
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        if let Ok(mut shell) = self.inner.try_borrow_mut() {
            shell.on_exit(gl);
        }
    }
}

fn paint_panel(
    session: &mut EditorSession,
    _registry: &PanelRegistry,
    faults: &mut HashMap<String, String>,
    ui: &mut egui::Ui,
    id: &str,
    deferred: &mut Vec<(String, seal_editor_core::CommandArgs)>,
) {
    if let Some(details) = faults.get(id).cloned() {
        let mut frame = seal_ui::PanelFrame::new(ui);
        frame.padded(|frame| {
            frame.muted(&session.messages.get("panel-panic-title"));
            frame.muted(&session.messages.get("panel-panic-hint"));
            frame.row(|frame| {
                if frame.button(&session.messages.get("command-panel-reload")) {
                    faults.remove(id);
                }
                if frame.button(&session.messages.get("panel-copy-details")) {
                    session.foundation.clipboard = details;
                    session.notify(session.messages.get("diagnostics-copied"), false);
                }
            });
        });
        return;
    }
    let panel_id = id.to_string();
    let clear_outliner_reveal =
        id.starts_with("outliner") && session.models.outliner_scroll_to.is_some();
    let event = match catch_unwind(AssertUnwindSafe(|| {
        let mut frame = seal_ui::PanelFrame::new(ui);
        show(&mut frame, id, session)
    })) {
        Ok(event) => event,
        Err(payload) => {
            let details = panic_text(payload);
            seal_log::error!("panel '{panel_id}' panicked: {details}");
            faults.insert(panel_id, details);
            PanelEvent::None
        }
    };
    // Panels painted earlier this frame (status bar, other docks) read the result next frame.
    if !matches!(event, PanelEvent::None) {
        ui.ctx().request_repaint();
    }
    match event {
        PanelEvent::Scratch(text) => session.set_scratch_text(&text),
        PanelEvent::Command(command, args) => {
            if switches_project(&command) {
                deferred.push((command, args));
            } else if let Err(err) = session.dispatch(&command, &args) {
                session.notify(err.to_string(), true);
            }
        }
        PanelEvent::Commands(list) => {
            for (command, args) in list {
                if switches_project(&command) {
                    deferred.push((command, args));
                } else if let Err(err) = session.dispatch(&command, &args) {
                    session.notify(err.to_string(), true);
                }
            }
        }
        PanelEvent::None => {}
    }
    if clear_outliner_reveal {
        session.models.outliner_scroll_to = None;
    }
}

fn focused_dock_panel(dock: &DockHost) -> Option<String> {
    let path = dock.state.focused_leaf()?;
    let leaf = dock.state.leaf(path).ok()?;
    leaf.tabs.get(leaf.active.0).cloned()
}

fn panic_text(payload: Box<dyn Any + Send>) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "panic".into()
    }
}

fn switches_project(id: &str) -> bool {
    matches!(
        id,
        "file.close-project" | "file.open-project" | "file.new-project" | "file.repair-project"
    )
}

fn setting_number(session: &EditorSession, id: &str, fallback: f32) -> f32 {
    session.foundation.settings.get(id).and_then(|text| text.parse().ok()).unwrap_or(fallback)
}

fn setting_flag(session: &EditorSession, id: &str) -> bool {
    session.foundation.settings.get(id) == Some("true")
}

fn note_save(shell: &mut Shell, result: Result<(), seal_editor_core::CoreError>) {
    match result {
        Ok(()) => {
            shell.save_error = None;
            shell.save_fault = None;
            shell.failed_path = None;
            shell.save_attempt = 0;
            shell.retry_at = None;
            shell.status_key = "status-saved".into();
        }
        Err(err) => {
            let message = err.to_string();
            shell.status_key = "status-save-failed".into();
            if shell.save_error.as_deref() != Some(message.as_str()) {
                seal_log::file_only(
                    seal_log::Level::Error,
                    "seal_editor_shell",
                    format!("save failed: {message}"),
                );
                shell.save_error = Some(message);
            }
            let Some(fault) = err.fault else {
                shell.save_fault = None;
                shell.retry_at = None;
                return;
            };
            if shell.save_fault != Some(fault) {
                shell.save_attempt = 0;
            }
            shell.save_fault = Some(fault);
            let Some(plan) = seal_editor_core::plan_save_failure(fault, shell.save_attempt) else {
                shell.retry_at = None;
                return;
            };
            shell.retry_at =
                plan.retry_ms.map(|delay| Instant::now() + Duration::from_millis(delay));
        }
    }
}

fn with_windows(picked: native_dialog::Pick, script: &str) -> native_dialog::Pick {
    match picked {
        native_dialog::Pick::Unavailable => match windows_dialog(script) {
            Some(path) => native_dialog::Pick::Path(path),
            None => native_dialog::Pick::Unavailable,
        },
        other => other,
    }
}

fn pick_folder_dialog() -> native_dialog::Pick {
    with_windows(
        native_dialog::pick_folder(),
        "Add-Type -AssemblyName System.Windows.Forms; $dialog = New-Object System.Windows.Forms.FolderBrowserDialog; if ($dialog.ShowDialog() -eq 'OK') { Write-Output $dialog.SelectedPath }",
    )
}

fn save_scene_dialog() -> native_dialog::Pick {
    with_windows(
        native_dialog::save_file("scene.seal"),
        "Add-Type -AssemblyName System.Windows.Forms; $dialog = New-Object System.Windows.Forms.SaveFileDialog; $dialog.FileName = 'scene.seal'; if ($dialog.ShowDialog() -eq 'OK') { Write-Output $dialog.FileName }",
    )
}

fn profile_dialog_name(name: &str) -> String {
    let stem = name.trim();
    let stem = if stem.is_empty() { "profile" } else { stem };
    let stem: String =
        stem.chars().filter(|ch| !matches!(ch, '\\' | '/' | '\'' | '"' | ';' | '\0')).collect();
    let stem = if stem.is_empty() { "profile".into() } else { stem };
    format!("{stem}.seal")
}

fn save_file_dialog(name: &str) -> native_dialog::Pick {
    with_windows(
        native_dialog::save_file(name),
        &format!(
            "Add-Type -AssemblyName System.Windows.Forms; $dialog = New-Object System.Windows.Forms.SaveFileDialog; $dialog.FileName = '{name}'; if ($dialog.ShowDialog() -eq 'OK') {{ Write-Output $dialog.FileName }}"
        ),
    )
}

fn open_file_dialog() -> native_dialog::Pick {
    with_windows(
        native_dialog::open_file(),
        "Add-Type -AssemblyName System.Windows.Forms; $dialog = New-Object System.Windows.Forms.OpenFileDialog; if ($dialog.ShowDialog() -eq 'OK') { Write-Output $dialog.FileName }",
    )
}

/// Pick one or more files to import; several come back one per line.
fn open_files_dialog() -> native_dialog::Pick {
    with_windows(
        native_dialog::open_files(),
        "Add-Type -AssemblyName System.Windows.Forms; $dialog = New-Object System.Windows.Forms.OpenFileDialog; $dialog.Multiselect = $true; if ($dialog.ShowDialog() -eq 'OK') { $dialog.FileNames | ForEach-Object { Write-Output $_ } }",
    )
}

fn save_log_dialog() -> native_dialog::Pick {
    with_windows(
        native_dialog::save_file("editor-log.txt"),
        "Add-Type -AssemblyName System.Windows.Forms; $dialog = New-Object System.Windows.Forms.SaveFileDialog; $dialog.FileName = 'editor-log.txt'; if ($dialog.ShowDialog() -eq 'OK') { Write-Output $dialog.FileName }",
    )
}

/// Wrap a Windows dialog script so the dialog opens in front of the editor and paths
/// with non-English letters come back intact.
#[cfg(any(windows, test))]
fn picker_script(script: &str) -> String {
    let owner = "$owner = New-Object System.Windows.Forms.Form -Property @{ TopMost = $true; ShowInTaskbar = $false }; ";
    let body = script.replacen(
        "; $dialog",
        &format!("; [Console]::OutputEncoding = [Text.Encoding]::UTF8; {owner}$dialog"),
        1,
    );
    body.replace("ShowDialog()", "ShowDialog($owner)")
}

fn windows_dialog(script: &str) -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let script = picker_script(script);
        // No console window, and the dialog sits on top of the editor instead of behind it.
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-STA", "-Command", script.as_str()])
            .creation_flags(0x0800_0000)
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() {
            None
        } else {
            Some(std::path::PathBuf::from(text))
        }
    }
    #[cfg(not(windows))]
    {
        let _ = script;
        None
    }
}

fn open_path(path: &std::path::Path) {
    #[cfg(windows)]
    let mut command = std::process::Command::new("explorer");
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");
    let _ = command.arg(path).spawn();
}

/// Headless check used by the binary and by CI: save, kill the value, load it back.
pub fn check_persistence(directory: &Path) -> Result<(), String> {
    let path = directory.join("layout.seal");
    let mut file = WorkspaceFile::fresh();
    file.save(&path).map_err(|err| err.to_string())?;
    let loaded = WorkspaceFile::load(&path);
    if loaded.layout() != file.layout() {
        return Err("layout did not survive a save".into());
    }
    if loaded.origin() != seal_editor_core::LoadOrigin::Primary {
        return Err("fresh save was not read as the primary file".into());
    }
    Ok(())
}

/// Keys the side-view game uses, so they do not also fire editor shortcuts.
/// Keys a focused text box uses itself, so an editor shortcut on the same chord
/// (Ctrl+A selecting every entity, Delete removing one) must not fire as well.
fn is_text_editing_key(chord: &seal_editor_core::Chord) -> bool {
    if chord.alt {
        return false;
    }
    let editing = matches!(
        chord.key.as_str(),
        "a" | "c"
            | "v"
            | "x"
            | "z"
            | "y"
            | "arrowleft"
            | "arrowright"
            | "arrowup"
            | "arrowdown"
            | "backspace"
            | "delete"
            | "home"
            | "end"
    );
    if chord.ctrl {
        editing
    } else {
        !chord.key.starts_with('f') || chord.key.len() == 1 || editing
    }
}

fn is_gameplay_key(chord: &seal_editor_core::Chord) -> bool {
    !(chord.ctrl || chord.alt)
        && matches!(
            chord.key.as_str(),
            "a" | "d"
                | "w"
                | "s"
                | "e"
                | "escape"
                | "space"
                | "arrowleft"
                | "arrowright"
                | "arrowup"
                | "arrowdown"
        )
}

#[cfg(test)]
mod play_tint_shell {
    use seal_editor_core::{
        effective_editor_theme, load_theme, play_tint_applies, theme_role_color, EditorSession,
        Layout, PlayMode,
    };

    #[test]
    fn tinted_theme_while_playing_and_clean_when_stopped() {
        let mut session = EditorSession::new(Layout::default());
        session.foundation.settings.set_user("ui.play_tint_strength", "40").unwrap();
        let base = load_theme("dark").unwrap();
        let idle = effective_editor_theme(&session, "dark");
        assert_eq!(theme_role_color(&idle, "panel"), theme_role_color(&base, "panel"));
        session.world.mode = PlayMode::Playing;
        assert!(play_tint_applies(&session));
        let live = effective_editor_theme(&session, "dark");
        assert_ne!(theme_role_color(&live, "panel"), theme_role_color(&base, "panel"));
        session.world.mode = PlayMode::Editing;
        assert!(!play_tint_applies(&session));
        let after = effective_editor_theme(&session, "dark");
        assert_eq!(theme_role_color(&after, "panel"), theme_role_color(&base, "panel"));
    }
}

#[cfg(test)]
mod picker_tests {
    #[test]
    fn dialogs_open_on_top_and_return_unicode_paths() {
        let script = super::picker_script("Add-Type -AssemblyName System.Windows.Forms; $dialog = New-Object System.Windows.Forms.FolderBrowserDialog; if ($dialog.ShowDialog() -eq 'OK') { Write-Output $dialog.SelectedPath }");
        assert!(script.contains("TopMost = $true"));
        assert!(script.contains("ShowDialog($owner)"));
        assert!(script.contains("OutputEncoding"));
        assert!(script.find("$owner =").unwrap() < script.find("ShowDialog(").unwrap());
    }
}
