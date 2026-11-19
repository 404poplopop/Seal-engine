//! Inspector **Tile Data** section for grid-snapped painted sprites.

use seal_editor_core::tiles::{is_painted_tile, tile_inspector_info};
use seal_editor_core::EditorSession;
use seal_ui::PanelFrame;

use crate::{command, PanelEvent};

pub fn tile_data_section(
    frame: &mut PanelFrame,
    session: &EditorSession,
    id: &str,
    text: &dyn Fn(&str) -> String,
) -> PanelEvent {
    let Some(entity) = session.world.entity(id) else {
        return PanelEvent::None;
    };
    if !is_painted_tile(entity) {
        return PanelEvent::None;
    }
    let info = tile_inspector_info(session, entity);
    let mut event = PanelEvent::None;
    frame.section(&text("inspector-tile-data"), |frame| {
        for (key, value) in &info.custom_data {
            let mut value_buf = value.clone();
            frame.label(key);
            if frame.text_edit(&mut value_buf) && value_buf != *value {
                event = command(
                    "tiles.data_set",
                    &[("id", id), ("key", key.as_str()), ("value", value_buf.as_str())],
                );
            }
            if frame.button(&text("inspector-tile-data-remove")) {
                event = command("tiles.data_remove", &[("id", id), ("key", key.as_str())]);
            }
        }
        let mut new_key = String::new();
        let mut new_value = String::new();
        frame.label(&text("inspector-tile-data-add"));
        frame.text_edit(&mut new_key);
        frame.text_edit(&mut new_value);
        if frame.button(&text("inspector-tile-data-add-btn")) && !new_key.trim().is_empty() {
            event = command(
                "tiles.data_set",
                &[("id", id), ("key", new_key.trim()), ("value", new_value.as_str())],
            );
        }
        let occ = if info.occluder {
            text("inspector-tile-occluder-yes")
        } else {
            text("inspector-tile-occluder-no")
        };
        frame.label(&format!("{}: {}", text("inspector-tile-occluder"), occ));
        let nav = if info.navigation_blocked {
            text("inspector-tile-navigation-blocked")
        } else {
            text("inspector-tile-navigation-walkable")
        };
        frame.label(&format!("{}: {}", text("inspector-tile-navigation"), nav));
        let none_set = text("inspector-tile-set-none");
        let set_label = info.tile_set.as_deref().unwrap_or(none_set.as_str());
        frame.label(&format!("{}: {set_label}", text("inspector-tile-set")));
        if let Some(rel) = info.tile_set.as_deref() {
            if frame.button(&text("inspector-tile-open-set")) {
                event = command("asset.open", &[("path", rel)]);
            }
        }
    });
    event
}
