//! Monitor discovery and applying saved window placement through egui viewports.

use seal_editor_core::{monitor_info_from_physical, MonitorInfo, WindowPlacement, WorkspaceFile};

#[cfg(not(target_arch = "wasm32"))]
pub fn monitors_from_window(window: &winit::window::Window, ui_zoom: f32) -> Vec<MonitorInfo> {
    let mut monitors: Vec<MonitorInfo> = window
        .available_monitors()
        .filter_map(|handle| {
            let name = handle.name()?;
            let pos = handle.position();
            let size = handle.size();
            let pixels_per_point = ui_zoom * handle.scale_factor() as f32;
            Some(monitor_info_from_physical(
                name.to_string(),
                pos.x as f32,
                pos.y as f32,
                size.width as f32,
                size.height as f32,
                pixels_per_point,
            ))
        })
        .collect();
    let primary = window.primary_monitor().or_else(|| window.available_monitors().next());
    if let Some(primary) = primary {
        if let Some(primary_name) = primary.name() {
            let primary_name = primary_name.to_string();
            monitors.sort_by_key(|monitor| u8::from(monitor.name != primary_name));
        }
    }
    monitors
}

#[cfg(target_arch = "wasm32")]
pub fn monitors_from_window(_window: &winit::window::Window, _ui_zoom: f32) -> Vec<MonitorInfo> {
    Vec::new()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn current_monitor_name(window: &winit::window::Window) -> Option<String> {
    window.current_monitor()?.name().map(|name| name.to_string())
}

#[cfg(target_arch = "wasm32")]
pub fn current_monitor_name(_window: &winit::window::Window) -> Option<String> {
    None
}

pub fn restore_on_startup(
    file: &WorkspaceFile,
    monitors: &[MonitorInfo],
    safe_mode: bool,
) -> Option<WindowPlacement> {
    if safe_mode || monitors.is_empty() {
        return None;
    }
    Some(file.restore_window_placement(monitors))
}

pub fn send_placement(ctx: &egui::Context, placement: &WindowPlacement) {
    if placement.maximized {
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
        return;
    }
    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
        placement.x,
        placement.y,
    )));
    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
        placement.width,
        placement.height,
    )));
}
