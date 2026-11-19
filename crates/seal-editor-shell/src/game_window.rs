//! Apply project display settings to a packaged game and to Play in New Window.

use std::sync::Arc;
use std::time::Instant;

use seal_editor_core::{window_game, EditorSession};

/// Schedule the next paint. Returns true when a frame cap is in force, so the
/// caller should not also ask for an immediate repaint.
pub fn limit_repaint(
    ctx: &egui::Context,
    session: &EditorSession,
    packaged: bool,
    last: &mut Instant,
) -> bool {
    if !applies(session, packaged) {
        return false;
    }
    let window = window_game::from_session(session);
    let wait = window_game::frame_wait(window.target_fps, last.elapsed());
    if window.target_fps == 0 || wait.is_zero() {
        *last = Instant::now();
        return false;
    }
    ctx.request_repaint_after(wait);
    true
}

/// Push title, size, mode, and the icon while a game window is showing.
/// `applied` is the signature last sent. Clearing it puts the editor window back.
pub fn apply(ctx: &egui::Context, session: &EditorSession, packaged: bool, applied: &mut String) {
    let active = applies(session, packaged);
    if !active {
        if !applied.is_empty() {
            restore_editor(ctx);
            applied.clear();
        }
        return;
    }
    let mut window = window_game::from_session(session);
    if let Some(mode) = session.world.play_game().map(|game| game.window_mode.as_str()) {
        if !mode.is_empty() {
            window.mode = mode.to_string();
        }
    }
    let title = if window.title.is_empty() {
        session
            .project
            .as_ref()
            .map(|project| project.name.clone())
            .unwrap_or_else(|| "Game".into())
    } else {
        window.title.clone()
    };
    let signature = format!(
        "{title}|{}|{}|{}|{}|{}|{}|{}",
        window.mode,
        window.width,
        window.height,
        window.resizable,
        window.always_on_top,
        window.icon,
        packaged
    );
    if signature == *applied {
        return;
    }
    *applied = signature;
    ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
        window.width as f32,
        window.height as f32,
    )));
    ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(window.resizable));
    let level = if window.always_on_top {
        egui::WindowLevel::AlwaysOnTop
    } else {
        egui::WindowLevel::Normal
    };
    ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
    match window.mode.as_str() {
        "fullscreen" => {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
        }
        "borderless" => {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
        }
        _ => {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
        }
    }
    if let Some(project) = &session.project {
        let path = project.root.join(&window.icon);
        if let Ok((width, height, rgba)) = window_game::read_icon_rgba(&path) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Icon(Some(Arc::new(egui::IconData {
                rgba,
                width,
                height,
            }))));
        }
    }
}

fn applies(session: &EditorSession, packaged: bool) -> bool {
    packaged || window_game::play_in_window(session)
}

fn restore_editor(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Title("Seal".into()));
    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
}

/// Viewport fields for a packaged game, read before the window opens.
pub fn startup(project: &std::path::Path) -> window_game::GameWindow {
    window_game::load_for_launch(project)
}

pub fn startup_size(window: &window_game::GameWindow) -> (f32, f32) {
    (window.width as f32, window.height as f32)
}
