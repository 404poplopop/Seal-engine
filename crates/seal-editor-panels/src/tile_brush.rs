//! Tile Brush section in the Create panel: tools, random weights, patterns.

use seal_editor_core::tile_tools::PaintTool;
use seal_editor_core::tiles::{
    PREFAB_BRUSH_PREFIX, TILE_GRID_DRAFT, TILE_LAYER_DRAFT, TILE_PATTERN_NAME_DRAFT,
    TILE_RANDOM_SEED_DRAFT, TILE_RANDOM_WEIGHTS_DRAFT, TILE_SOLID_DRAFT, TILE_TOOL_DRAFT,
};
use seal_editor_core::{CommandArgs, EditorSession};
use seal_ui::PanelFrame;

use crate::inspector_panel;
use crate::{command, PanelEvent, PLACE_BRUSH_DRAFT};

const TILE_PATTERN_SAVE_DRAFT: &str = "tile-pattern-save-mode";

fn paint_tool_save_mode(session: &EditorSession) -> bool {
    session.draft(TILE_PATTERN_SAVE_DRAFT).as_deref() == Some("true")
}

pub fn tile_brush_section(
    frame: &mut PanelFrame,
    session: &EditorSession,
    text: &dyn Fn(&str) -> String,
) -> PanelEvent {
    let mut event = PanelEvent::None;
    let tool = PaintTool::parse(&session.draft(TILE_TOOL_DRAFT).unwrap_or_default());
    let grid = session.draft(TILE_GRID_DRAFT).unwrap_or_else(|| "square".into());
    let layer_text = session.draft(TILE_LAYER_DRAFT).unwrap_or_else(|| "0".into());
    let solid = session.draft(TILE_SOLID_DRAFT).as_deref() == Some("true");
    frame.section(&text("place-tiles"), |frame| {
        let tools: &[(&str, PaintTool, &str)] = &[
            ("place-tiles-tool-brush", PaintTool::Brush, "tiles.tool_brush"),
            ("place-tiles-tool-rectangle", PaintTool::Rectangle, "tiles.tool_rectangle"),
            ("place-tiles-tool-line", PaintTool::Line, "tiles.tool_line"),
            ("place-tiles-tool-fill", PaintTool::Fill, "tiles.tool_fill"),
            ("place-tiles-tool-picker", PaintTool::Picker, "tiles.tool_picker"),
            ("place-tiles-tool-eraser", PaintTool::Eraser, "tiles.tool_eraser"),
            ("place-tiles-tool-random", PaintTool::Random, "tiles.tool_random"),
            ("place-tiles-tool-pattern", PaintTool::PatternPaint, "tiles.tool_pattern"),
        ];
        let current = tools
            .iter()
            .find(|(_, pick, _)| std::mem::discriminant(&tool) == std::mem::discriminant(pick))
            .map_or("", |(_, _, cmd)| *cmd);
        let tool_options: Vec<(&str, String)> =
            tools.iter().map(|(label_key, _, cmd)| (*cmd, text(label_key))).collect();
        if let Some(pick) = frame.choice(&text("place-tiles-tools"), current, &tool_options) {
            event = command(&pick, &[]);
        }
        let options = [
            ("square", text("place-tiles-square")),
            ("iso", text("place-tiles-iso")),
            ("hex", text("place-tiles-hex")),
        ];
        if let Some(pick) = frame.choice(&text("place-tiles-grid"), &grid, &options) {
            event = command("draft.set", &[("key", TILE_GRID_DRAFT), ("value", &pick)]);
        }
        let mut layer: f32 = layer_text.parse().unwrap_or(0.0);
        if frame.number(&text("place-tiles-layer"), &mut layer, 1.0) {
            let value = (layer.round() as i32).clamp(-99, 99).to_string();
            event = command("draft.set", &[("key", TILE_LAYER_DRAFT), ("value", &value)]);
        }
        if frame.toggle(&text("place-tiles-solid"), solid) {
            let value = if solid { "false" } else { "true" };
            event = command("draft.set", &[("key", TILE_SOLID_DRAFT), ("value", value)]);
        }
        if tool == PaintTool::Random {
            let weights = session.draft(TILE_RANDOM_WEIGHTS_DRAFT).unwrap_or_default();
            frame.label(&text("place-tiles-random-hint"));
            let mut buf = weights.clone();
            frame.label(&text("place-tiles-random-weights"));
            if frame.text_edit(&mut buf) {
                event =
                    command("draft.set", &[("key", TILE_RANDOM_WEIGHTS_DRAFT), ("value", &buf)]);
            }
            let seed = session.draft(TILE_RANDOM_SEED_DRAFT).unwrap_or_else(|| "1".into());
            let mut seed_val: f32 = seed.parse().unwrap_or(1.0);
            if frame.number(&text("place-tiles-random-seed"), &mut seed_val, 1.0) {
                let v = (seed_val.round() as i64).max(0).to_string();
                event = command("draft.set", &[("key", TILE_RANDOM_SEED_DRAFT), ("value", &v)]);
            }
        }
        let prefabs = inspector_panel::project_files_with(session, &["prefab"]);
        if !prefabs.is_empty() {
            let active = session.draft(PLACE_BRUSH_DRAFT).unwrap_or_default();
            frame.label(&text("place-tiles-prefabs"));
            for rel in &prefabs {
                let brush_key = format!("{PREFAB_BRUSH_PREFIX}{rel}");
                let on = active == brush_key;
                let name = std::path::Path::new(rel)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| rel.clone());
                if frame.toggle(&name, on) {
                    let value = if on { "" } else { brush_key.as_str() };
                    event = command("draft.set", &[("key", PLACE_BRUSH_DRAFT), ("value", value)]);
                }
            }
        }
        if matches!(tool, PaintTool::PatternPaint | PaintTool::PatternSave) {
            let pattern = session.draft(TILE_PATTERN_NAME_DRAFT).unwrap_or_default();
            let mut name = pattern.clone();
            frame.label(&text("place-tiles-pattern-name"));
            if frame.text_edit(&mut name) {
                event = command("draft.set", &[("key", TILE_PATTERN_NAME_DRAFT), ("value", &name)]);
            }
            let save_mode = session.draft(TILE_PATTERN_SAVE_DRAFT).as_deref() == Some("true");
            if frame.toggle(&text("place-tiles-pattern-save-mode"), save_mode) {
                let v = if save_mode { "false" } else { "true" };
                event = command("draft.set", &[("key", TILE_PATTERN_SAVE_DRAFT), ("value", v)]);
            }
        }
        frame.label(&text("place-tiles-hint"));
    });
    event
}

/// Turn a committed viewport stroke into editor commands.
pub fn stroke_command(
    session: &EditorSession,
    rel: &str,
    cells: &[(i32, i32)],
    pick_sprite: Option<&str>,
) -> PanelEvent {
    if cells.is_empty() {
        if let Some(sprite) = pick_sprite {
            return PanelEvent::Command(
                "draft.set".into(),
                CommandArgs::new(&[("key", PLACE_BRUSH_DRAFT), ("value", sprite)]),
            );
        }
        return PanelEvent::None;
    }
    let mut tool = PaintTool::parse(&session.draft(TILE_TOOL_DRAFT).unwrap_or_default());
    if paint_tool_save_mode(session) {
        tool = PaintTool::PatternSave;
    }
    let tile_grid = session.draft(TILE_GRID_DRAFT).unwrap_or_else(|| "square".into());
    let layer = session.draft(TILE_LAYER_DRAFT).unwrap_or_else(|| "0".into());
    let solid =
        if session.draft(TILE_SOLID_DRAFT).as_deref() == Some("true") { "true" } else { "false" };
    let cells_s: String =
        cells.iter().map(|(i, j)| format!("{i},{j}")).collect::<Vec<_>>().join(";");
    if tool == PaintTool::Picker {
        if let Some(sprite) = pick_sprite {
            return PanelEvent::Command(
                "draft.set".into(),
                CommandArgs::new(&[("key", PLACE_BRUSH_DRAFT), ("value", sprite)]),
            );
        }
        return PanelEvent::None;
    }
    if tool == PaintTool::PatternSave {
        let name = session.draft(TILE_PATTERN_NAME_DRAFT).unwrap_or_default();
        if name.is_empty() {
            return PanelEvent::None;
        }
        return PanelEvent::Command(
            "tiles.pattern_save".into(),
            CommandArgs::new(&[
                ("name", name.as_str()),
                ("cells", cells_s.as_str()),
                ("grid", tile_grid.as_str()),
                ("layer", layer.as_str()),
            ]),
        );
    }
    if tool == PaintTool::PatternPaint {
        let name = session.draft(TILE_PATTERN_NAME_DRAFT).unwrap_or_default();
        if name.is_empty() {
            return PanelEvent::None;
        }
        let origin = cells.first().copied().unwrap_or((0, 0));
        let origin_s = format!("{},{}", origin.0, origin.1);
        return PanelEvent::Command(
            "tiles.pattern_paint".into(),
            CommandArgs::new(&[
                ("name", name.as_str()),
                ("origin", origin_s.as_str()),
                ("layer", layer.as_str()),
            ]),
        );
    }
    let erase = matches!(tool, PaintTool::Eraser);
    let id = if erase { "tiles.erase" } else { "tiles.paint" };
    let seed = session.draft(TILE_RANDOM_SEED_DRAFT).unwrap_or_else(|| "1".into());
    let weights = session.draft(TILE_RANDOM_WEIGHTS_DRAFT).unwrap_or_default();
    let mut args: Vec<(&str, String)> = vec![
        ("brush", rel.to_string()),
        ("cells", cells_s),
        ("grid", tile_grid),
        ("layer", layer),
        ("solid", solid.to_string()),
    ];
    if tool == PaintTool::Random && !weights.is_empty() {
        args.push(("weights", weights));
        args.push(("seed", seed));
    }
    let pairs: Vec<(&str, &str)> = args.iter().map(|(k, v)| (*k, v.as_str())).collect();
    PanelEvent::Command(id.into(), CommandArgs::new(&pairs))
}
