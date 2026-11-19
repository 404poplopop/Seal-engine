//! Read-only preview of a pixel animation file (.ase / .aseprite).
//!
//! The open frame is kept in a draft. Import writes a picture and a sprite
//! animation beside the file.

use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use egui::{Color32, Pos2, Rect, Sense, Vec2};
use seal_editor_core::aseprite::{self, Direction, Source, FRAME_DRAFT};
use seal_editor_core::EditorSession;
use seal_ui::PanelFrame;

use crate::{command, PanelEvent};

struct View {
    mtime: Option<SystemTime>,
    source: Result<Arc<Source>, String>,
    textures: Vec<egui::TextureHandle>,
}

impl Clone for View {
    fn clone(&self) -> Self {
        Self { mtime: self.mtime, source: self.source.clone(), textures: self.textures.clone() }
    }
}

fn view(ctx: &egui::Context, path: &Path) -> View {
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let id = egui::Id::new(("sprite-source", path.to_string_lossy().to_string()));
    if let Some(cached) = ctx.data(|d| d.get_temp::<View>(id)) {
        if cached.mtime == mtime {
            return cached;
        }
    }
    let source = aseprite::load(path).map(Arc::new);
    let textures = match &source {
        Ok(src) => src
            .frames
            .iter()
            .enumerate()
            .filter_map(|(i, rgba)| {
                let pixels =
                    (src.width as usize).checked_mul(src.height as usize)?.checked_mul(4)?;
                if rgba.len() != pixels {
                    return None;
                }
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [src.width as usize, src.height as usize],
                    rgba,
                );
                Some(ctx.load_texture(
                    format!("sprite-source:{}:{i}", path.display()),
                    image,
                    egui::TextureOptions::NEAREST,
                ))
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    let view = View { mtime, source, textures };
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
        frame.label(&text("asset-sprite-source-label"));
        frame.space(8.0);
        if let Ok(src) = &loaded.source {
            frame.muted(&session.messages.format(
                "asset-sprite-source-size",
                &[
                    ("w", &src.width.to_string()),
                    ("h", &src.height.to_string()),
                    ("frames", &src.frames.len().to_string()),
                ],
            ));
        }
        if frame.button(&text("asset-sprite-source-import")) {
            event = command("asset.sprite_source_import", &[("path", rel)]);
        }
    });
    match loaded.source.as_ref() {
        Err(error) => {
            frame.padded(|frame| {
                frame.muted(
                    &session
                        .messages
                        .format("asset-sprite-source-unreadable", &[("error", error.as_str())]),
                );
            });
            event
        }
        Ok(_) => paint(frame, session, &loaded, event),
    }
}

fn paint(
    frame: &mut PanelFrame,
    session: &EditorSession,
    loaded: &View,
    mut event: PanelEvent,
) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let Some(src) = loaded.source.as_ref().ok() else {
        return event;
    };
    let count = src.frames.len();
    let picked = session
        .draft(FRAME_DRAFT)
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0)
        .min(count.saturating_sub(1));
    frame.scroll(|frame| {
        frame.padded(|frame| {
            if src.width > 0 && src.height > 0 {
                if let Some(texture) = loaded.textures.get(picked) {
                    paint_frame(frame, texture, src.width, src.height);
                }
            }
            // Frame picker reads left to right under the picture.
            frame.row(|frame| {
                if count > 24 {
                    let which = session.messages.format(
                        "asset-sprite-source-which",
                        &[("index", &(picked + 1).to_string()), ("count", &count.to_string())],
                    );
                    frame.label(&which);
                    if picked > 0 && frame.button(&text("asset-sprite-source-prev")) {
                        event = set_frame(picked - 1);
                    }
                    if picked + 1 < count && frame.button(&text("asset-sprite-source-next")) {
                        event = set_frame(picked + 1);
                    }
                } else {
                    for i in 0..count {
                        let label = (i + 1).to_string();
                        if frame.chip(&label, i == picked) {
                            event = set_frame(i);
                        }
                    }
                }
            });
            for note in &src.notes {
                frame.muted(note);
            }
            frame.section(&text("asset-sprite-source-tags"), |frame| {
                if src.tags.is_empty() {
                    frame.muted(&text("asset-sprite-source-no-tags"));
                }
                for tag in &src.tags {
                    let direction = text(match tag.direction {
                        Direction::Forward => "asset-sprite-source-forward",
                        Direction::Reverse => "asset-sprite-source-reverse",
                        Direction::PingPong => "asset-sprite-source-pingpong",
                        Direction::PingPongReverse => "asset-sprite-source-pingpong-reverse",
                    });
                    let play = if tag.repeat == 0 {
                        text("asset-sprite-source-loops")
                    } else if tag.repeat == 1 {
                        text("asset-sprite-source-once")
                    } else {
                        session.messages.format(
                            "asset-sprite-source-repeats",
                            &[("count", &tag.repeat.to_string())],
                        )
                    };
                    let name =
                        if tag.name.is_empty() { "clip".to_string() } else { tag.name.clone() };
                    frame.muted(&session.messages.format(
                        "asset-sprite-source-tag",
                        &[
                            ("name", name.as_str()),
                            ("from", &(tag.from + 1).to_string()),
                            ("to", &(tag.to + 1).to_string()),
                            ("direction", direction.as_str()),
                            ("play", play.as_str()),
                        ],
                    ));
                }
            });
            frame.section(&text("asset-sprite-source-slices"), |frame| {
                if src.slices.is_empty() {
                    frame.muted(&text("asset-sprite-source-no-slices"));
                }
                for slice in &src.slices {
                    frame.muted(&session.messages.format(
                        "asset-sprite-source-slice",
                        &[
                            ("name", slice.name.as_str()),
                            ("x", &slice.x.to_string()),
                            ("y", &slice.y.to_string()),
                            ("w", &slice.width.to_string()),
                            ("h", &slice.height.to_string()),
                            ("frame", &(slice.frame + 1).to_string()),
                        ],
                    ));
                }
            });
            frame.hint(&text("asset-sprite-source-readonly"));
        });
    });
    event
}

fn set_frame(index: usize) -> PanelEvent {
    command("draft.set", &[("key", FRAME_DRAFT), ("value", &index.to_string())])
}

fn paint_frame(frame: &mut PanelFrame, texture: &egui::TextureHandle, w: u32, h: u32) {
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
