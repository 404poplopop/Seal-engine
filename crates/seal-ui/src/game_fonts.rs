//! Project fonts for game screens: each .ttf/.otf file becomes its own font
//! family the first time a screen draws with it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use egui::{FontData, FontFamily};

#[derive(Clone, Default)]
struct Loaded(Arc<Mutex<HashMap<String, (Option<SystemTime>, Option<String>)>>>);

/// The family to draw text in `path` (an absolute font file path) with.
/// `None` means use the built-in font: the path is empty, missing, not a font
/// Seal can read, or was added this frame and shows from the next one.
pub fn game_font(ctx: &egui::Context, path: &str) -> Option<FontFamily> {
    if path.is_empty() {
        return None;
    }
    let cache = ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Loaded>(egui::Id::new("seal-game-fonts")).clone()
    });
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let name = {
        let mut map = cache.0.lock().ok()?;
        match map.get(path) {
            Some((when, name)) if *when == mtime => name.clone(),
            _ => {
                // New or changed file: load it under a name that includes the
                // change time, so an edited font replaces the old one.
                let bytes = std::fs::read(path).ok();
                let ok =
                    bytes.as_deref().is_some_and(|b| seal_editor_core::fonts::font_info(b).is_ok());
                let name = ok.then(|| format!("seal-game-font:{path}:{mtime:?}"));
                if let (Some(n), Some(b)) = (&name, bytes) {
                    ctx.add_font(FontInsert::new(
                        n,
                        FontData::from_owned(b),
                        vec![InsertFontFamily {
                            family: FontFamily::Name(n.as_str().into()),
                            priority: FontPriority::Highest,
                        }],
                    ));
                }
                map.insert(path.to_string(), (mtime, name.clone()));
                name
            }
        }
    }?;
    let family = FontFamily::Name(name.as_str().into());
    // `add_font` takes effect on the next frame; until then use the built-in font.
    ctx.fonts(|f| f.definitions().families.contains_key(&family)).then_some(family)
}
