//! Read-only preview of a layered picture (`.psd`).
//!
//! Import writes a flattened picture. Layers as sprites also writes one trimmed
//! picture per visible layer and a prefab that places them.

use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use egui::{Color32, Pos2, Rect, Sense, Vec2};
use seal_editor_core::layered_picture::{self, LayerLine, Source};
use seal_editor_core::EditorSession;
use seal_ui::{PanelFrame, PropertyCheckbox};

use crate::{command, PanelEvent};

struct View {
    mtime: Option<SystemTime>,
    source: Result<Arc<Source>, String>,
    texture: Option<egui::TextureHandle>,
}

impl Clone for View {
    fn clone(&self) -> Self {
        Self { mtime: self.mtime, source: self.source.clone(), texture: self.texture.clone() }
    }
}

fn view(ctx: &egui::Context, path: &Path) -> View {
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let id = egui::Id::new(("layered-picture", path.to_string_lossy().to_string()));
    if let Some(cached) = ctx.data(|d| d.get_temp::<View>(id)) {
        if cached.mtime == mtime {
            return cached;
        }
    }
    let source = layered_picture::load(path).map(Arc::new);
    let texture = match &source {
        Ok(src) if src.width > 0 && src.height > 0 => {
            let pixels = (src.width as usize).saturating_mul(src.height as usize).saturating_mul(4);
            if src.rgba.len() == pixels {
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [src.width as usize, src.height as usize],
                    &src.rgba,
                );
                Some(ctx.load_texture(
                    format!("layered-picture:{}", path.display()),
                    image,
                    egui::TextureOptions::NEAREST,
                ))
            } else {
                None
            }
        }
        _ => None,
    };
    let view = View { mtime, source, texture };
    ctx.data_mut(|d| d.insert_temp(id, view.clone()));
    view
}

pub(crate) fn editor(
    frame: &mut PanelFrame,
    session: &EditorSession,
    rel: &str,
    full: &Path,
) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let ctx = frame.ui_mut().ctx().clone();
    let loaded = view(&ctx, full);
    let mut event = PanelEvent::None;
    frame.bar(|frame| {
        frame.label(&text("asset-layered-picture-label"));
        frame.space(8.0);
        if let Ok(src) = &loaded.source {
            frame.muted(&session.messages.format(
                "asset-layered-picture-size",
                &[
                    ("w", &src.width.to_string()),
                    ("h", &src.height.to_string()),
                    ("layers", &src.layers.iter().filter(|layer| !layer.group).count().to_string()),
                ],
            ));
        }
        if frame.button(&text("asset-layered-picture-import")) {
            event = command("asset.layered_picture_import", &[("path", rel)]);
        }
    });
    match loaded.source.as_ref() {
        Err(error) => {
            frame.padded(|frame| {
                frame.muted(
                    &session
                        .messages
                        .format("asset-layered-picture-unreadable", &[("error", error.as_str())]),
                );
            });
            event
        }
        Ok(src) => paint(frame, session, full, rel, src, loaded.texture.as_ref(), event),
    }
}

fn paint(
    frame: &mut PanelFrame,
    session: &EditorSession,
    full: &Path,
    rel: &str,
    src: &Source,
    texture: Option<&egui::TextureHandle>,
    mut event: PanelEvent,
) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let on = layered_picture::layers_as_sprites(full);
    frame.scroll(|frame| {
        frame.padded(|frame| {
            if let Some(texture) = texture {
                paint_image(frame, texture, src.width, src.height);
            }
            let mut menu_pick = None;
            if let Some(next) = frame.property_checkbox_row(
                &text("asset-layered-picture-layers"),
                if on { PropertyCheckbox::On } else { PropertyCheckbox::Off },
                false,
                None,
                None,
                &mut menu_pick,
                Some("layers_as_sprites"),
            ) {
                let value = if matches!(next, PropertyCheckbox::On) { "yes" } else { "no" };
                event = command("asset.layered_picture_option", &[("path", rel), ("value", value)]);
            }
            frame.hint(&text("asset-layered-picture-hint"));
            for layer in &src.layers {
                frame.muted(&layer_line(session, layer));
            }
            for note in &src.notes {
                frame.muted(note);
            }
        });
    });
    event
}

fn layer_line(session: &EditorSession, layer: &LayerLine) -> String {
    let indent = "  ".repeat(layer.depth as usize);
    let kind = if !layer.visible {
        session.messages.get("asset-layered-picture-hidden")
    } else if layer.group {
        session.messages.get("asset-layered-picture-group")
    } else {
        String::new()
    };
    if kind.is_empty() {
        format!("{indent}{}", layer.name)
    } else {
        format!("{indent}{} ({kind})", layer.name)
    }
}

fn paint_image(frame: &mut PanelFrame, texture: &egui::TextureHandle, w: u32, h: u32) {
    let ui = frame.ui_mut();
    let avail = ui.available_width().max(32.0);
    let scale = ((avail - 8.0) / w as f32).min(240.0 / h as f32).clamp(1.0, 48.0);
    let size = Vec2::new(w as f32 * scale, h as f32 * scale);
    let (rect, _) = ui.allocate_exact_size(size + Vec2::splat(8.0), Sense::hover());
    let img = Rect::from_center_size(rect.center(), size);
    let painter = ui.painter_at(rect);
    let (a, b) = (Color32::from_gray(58), Color32::from_gray(74));
    let cell = 8.0_f32;
    let mut y = img.top();
    let mut row = 0;
    while y < img.bottom() {
        let mut x = img.left();
        let mut col = row;
        while x < img.right() {
            let r = Rect::from_min_max(
                Pos2::new(x, y),
                Pos2::new((x + cell).min(img.right()), (y + cell).min(img.bottom())),
            );
            painter.rect_filled(r, 0.0, if col % 2 == 0 { a } else { b });
            x += cell;
            col += 1;
        }
        y += cell;
        row += 1;
    }
    seal_ui::sprites::paint_quad(
        &painter,
        texture.id(),
        [img.left_top(), img.right_top(), img.right_bottom(), img.left_bottom()],
    );
}
