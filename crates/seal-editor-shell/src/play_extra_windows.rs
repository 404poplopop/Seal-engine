//! Extra OS windows while the editor is playing.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use seal_editor_core::{EditorSession, PlayMode};

struct Stamp {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: [f32; 3],
    sprite: String,
    layer: i32,
}

/// Sync graph-driven extra windows during play.
pub fn apply(ctx: &egui::Context, session: &EditorSession, open: &mut BTreeSet<u32>) {
    let mut placed: BTreeMap<u32, (i32, i32, u32, u32)> =
        ctx.data(|data| data.get_temp(placed_id())).unwrap_or_default();
    if session.world.mode == PlayMode::Editing {
        for id in open.iter().copied().collect::<Vec<_>>() {
            ctx.send_viewport_cmd_to(viewport_id(id), egui::ViewportCommand::Close);
        }
        open.clear();
        placed.clear();
        ctx.data_mut(|data| data.insert_temp(placed_id(), placed));
        return;
    }
    let windows: Vec<_> = match session.world.play_game() {
        Some(game) => {
            game.extra_windows.open.iter().map(|(id, window)| (*id, window.clone())).collect()
        }
        None => return,
    };
    let root = session.project.as_ref().map(|project| project.root.clone());
    let mut seen = BTreeSet::new();
    for (id, window) in &windows {
        seen.insert(*id);
        let stamps = scene_stamps(session);
        let (center, zoom, camera_missing) = camera_frame(session, &window.camera);
        let draws = session.world.game_screens.draws_named(
            &window.ui_screen,
            [window.width as f32, window.height as f32],
            session.world.play_time,
        );
        let screen_missing = !window.ui_screen.is_empty() && draws.is_empty();
        let title = window.title.clone();
        let width = window.width as f32;
        let height = window.height as f32;
        let x = window.x as f32;
        let y = window.y as f32;
        let camera_label = window.camera.clone();
        let screen_label = window.ui_screen.clone();
        let window_root = root.clone();
        if placed.get(id) != Some(&(window.x, window.y, window.width, window.height)) {
            let vid = viewport_id(*id);
            ctx.send_viewport_cmd_to(vid, egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
            ctx.send_viewport_cmd_to(
                vid,
                egui::ViewportCommand::InnerSize(egui::vec2(width, height)),
            );
            placed.insert(*id, (window.x, window.y, window.width, window.height));
        }
        ctx.show_viewport_immediate(
            viewport_id(*id),
            egui::ViewportBuilder::default()
                .with_title(title)
                .with_inner_size([width, height])
                .with_position(egui::pos2(x, y))
                .with_decorations(true)
                .with_resizable(true),
            move |ui, _| {
                paint_play_window(
                    ui,
                    &stamps,
                    center,
                    zoom,
                    window_root.clone(),
                    &draws,
                    &camera_label,
                    camera_missing,
                    &screen_label,
                    screen_missing,
                );
            },
        );
    }
    for id in open.iter().filter(|id| !seen.contains(id)).copied().collect::<Vec<_>>() {
        ctx.send_viewport_cmd_to(viewport_id(id), egui::ViewportCommand::Close);
        open.remove(&id);
        placed.remove(&id);
    }
    *open = seen;
    ctx.data_mut(|data| data.insert_temp(placed_id(), placed));
}

pub fn attention(ctx: &egui::Context, session: &mut seal_editor_core::EditorSession) {
    let request = session.world.play_game().is_some_and(|game| game.attention_request);
    if !request {
        return;
    }
    if let Some(play) = session.world.play_game_mut() {
        play.attention_request = false;
    }
    ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
        egui::UserAttentionType::Critical,
    ));
}

fn placed_id() -> egui::Id {
    egui::Id::new("seal-play-extra-placed")
}

fn viewport_id(id: u32) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(format!("seal-play-window-{id}"))
}

fn scene_stamps(session: &EditorSession) -> Vec<Stamp> {
    let mut stamps: Vec<Stamp> = session
        .world
        .entities
        .iter()
        .filter(|entity| !entity.hidden)
        .map(|entity| Stamp {
            x: entity.translation[0],
            y: entity.translation[1],
            w: entity.scale[0].abs().max(0.05),
            h: entity.scale[1].abs().max(0.05),
            color: entity.color,
            sprite: entity.sprite.clone(),
            layer: entity.layer,
        })
        .collect();
    stamps.sort_by_key(|stamp| stamp.layer);
    stamps
}

fn camera_frame(session: &EditorSession, camera_id: &str) -> ([f32; 2], f32, bool) {
    if camera_id.is_empty() {
        let center =
            session.world.play_view_target().map(|at| [at[0], at[1]]).unwrap_or([0.0, 0.0]);
        return (center, 1.0, false);
    }
    match session.world.entities.iter().find(|entity| entity.id == camera_id) {
        Some(entity) => {
            let zoom = if entity.camera2d.on { entity.camera2d.zoom } else { 1.0 };
            ([entity.translation[0], entity.translation[1]], zoom.clamp(0.05, 50.0), false)
        }
        None => ([0.0, 0.0], 1.0, true),
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_play_window(
    ui: &mut egui::Ui,
    stamps: &[Stamp],
    center: [f32; 2],
    zoom: f32,
    root: Option<PathBuf>,
    draws: &[seal_editor_core::gameui::Draw],
    camera: &str,
    camera_missing: bool,
    screen: &str,
    screen_missing: bool,
) {
    egui::CentralPanel::default().show(ui, |ui| {
        let rect = ui.available_rect_before_wrap();
        ui.allocate_rect(rect, egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
        let cell = 48.0 * zoom.max(0.05);
        let origin = rect.center();
        for stamp in stamps {
            let x = origin.x + (stamp.x - center[0]) * cell;
            let y = origin.y - (stamp.y - center[1]) * cell;
            let size = egui::vec2((stamp.w * cell).max(4.0), (stamp.h * cell).max(4.0));
            let mark = egui::Rect::from_center_size(egui::pos2(x, y), size);
            let tint = egui::Color32::from_rgb(
                (stamp.color[0].clamp(0.0, 1.0) * 255.0) as u8,
                (stamp.color[1].clamp(0.0, 1.0) * 255.0) as u8,
                (stamp.color[2].clamp(0.0, 1.0) * 255.0) as u8,
            );
            let sprite = root
                .as_ref()
                .filter(|_| !stamp.sprite.is_empty())
                .map(|dir| dir.join(&stamp.sprite));
            if let Some((texture, uv, _)) = sprite.as_ref().and_then(|path| {
                seal_ui::sprites::sprite_texture(ui.ctx(), &path.to_string_lossy())
            }) {
                let uv_rect =
                    egui::Rect::from_min_max(egui::pos2(uv[0], uv[1]), egui::pos2(uv[2], uv[3]));
                painter.image(texture, mark, uv_rect, tint);
            } else {
                painter.rect_filled(mark, 2.0, tint);
            }
        }
        if !draws.is_empty() {
            seal_ui::paint_game_screens(ui.ctx(), &painter, rect, draws);
        }
        let mut notes = Vec::new();
        if !camera.is_empty() {
            notes.push(format!("Camera: {camera}"));
        }
        if camera_missing {
            notes.push(format!("Camera {camera} is not in the scene."));
        }
        if !screen.is_empty() {
            notes.push(format!("UI screen: {screen}"));
        }
        if screen_missing {
            notes.push(format!("UI screen {screen} is not open."));
        }
        if notes.is_empty() {
            return;
        }
        painter.text(
            rect.left_top() + egui::vec2(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            notes.join("\n"),
            egui::FontId::proportional(13.0),
            ui.visuals().weak_text_color(),
        );
    });
}
