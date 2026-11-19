//! Thin editor UI. Panels and the shell talk to this crate, not to the toolkit.

mod a11y;
pub mod asset_picker;
pub mod colour_picker;
mod context_menu;
pub mod crisp;
mod curve_property;
mod dock;
mod flags_field;
pub mod game_fonts;
pub mod gpu_mesh_pass;
pub mod gpu_sprite_blend;
pub mod gpu_sprite_wgpu;
pub mod gpu_textures;
pub mod gpu_viewport;
pub mod gpu_viewport_wgpu;
mod gpu_wgpu_choice;
mod gradient_property;
pub mod graph_canvas;
pub mod graph_functions;
mod icons;
mod input;
mod list_property;
mod logo;
mod look;
pub mod map_field;
mod number_field;
pub mod object_picker;
mod outliner_table;
mod poly_edit;
pub mod session_end;
mod snapshot;
mod sprite_blend;
mod sprite_load;
pub mod sprite_mask;
mod sprite_shape;
pub mod sprites;
pub mod state_machine_canvas;
mod viewport_gizmo;
mod viewport_mesh;
mod widgets;

pub use a11y::{
    accesskit_contains, look_tooltip, named_button, BUILTIN_PANELS, IME_COMPLEX_SAMPLE,
};
pub use context_menu::{
    paint_context_menu, ContextMenuEntry, ContextMenuPick, ContextMenuRow, ViewportContextMenu,
};
pub use curve_property::{curve_row, CurveFieldEdit, CurveUiText};
pub use dock::{
    apply_tab_menu, dock_from_layout, layout_from_dock, show_dock, show_dock_with_badges,
    tab_title, DockHost, TabMenuResult,
};
pub use flags_field::paint_flags_value;
pub use gpu_textures::{MeshSurface, CPU_FALLBACK};
pub use gradient_property::{gradient_row, GradientFieldEdit, GradientUiText};
pub use icons::{
    asset_tint, icon_font_ready, install_icon_font, outliner_entity_icon, outliner_entity_tint,
    paint_icon, place_icon, Icon,
};
pub use input::{binding_key_name, pressed_chord, show_palette, typeahead_key, PaletteEvent};
pub use list_property::{ListPropertyAction, ListPropertyLabels};
pub use logo::app_icon;
pub use look::{
    apply_look, ThemeColors, AXIS_X, AXIS_Y, AXIS_Z, BODY_PX, ICON_PX, MENU_PX, MENU_ROW_PX,
    ROW_COMPACT_PX, ROW_PX, SECTION_PX, SMALL_PX, STATUS_OK, STATUS_PX, TAB_PX, TITLE_PX,
    TOOLBAR_PX, TOOL_BUTTON_PX, TOOL_ROW_PX, TREE_INDENT_PX,
};
pub use number_field::{eval_number_expression, NumberFieldSpec};
pub use outliner_table::{
    paint_outliner_header, paint_outliner_row, OutlinerColumnLayout, OutlinerHeaderHit,
    OutlinerHeaderLabels, OutlinerRowExtra, OutlinerRowMarksView, OutlinerTypeAssetLink,
    OPTIONAL_COLUMNS,
};
pub use poly_edit::{PolyEdit, PolyGrab};
pub use seal_editor_core::panel_base;
pub use snapshot::{accesskit_tree_text, panel_accesskit_snapshot, snapshot_header};
pub use sprite_blend::texture_key as sprite_blend_texture_key;
pub use sprite_shape::{SplineControl, SplineEdit, SplineGrab, SpriteShapeMark};
pub use viewport_gizmo::{
    parse_move_handle, parse_rotate_handle, parse_scale_handle, rotate_axis_index, MoveHandle,
    RotateHandle, ScaleHandle,
};
pub use viewport_mesh::{
    look_through, mesh_for_place_kind, project_scene_point, scene_light_gain, set_look_through,
    LookThrough, MeshPrimitive,
};
pub use widgets::{
    asset_kind_color, paint_folder_tile, paint_game_screens, paint_viewport_pane_border,
    panel_body_rect, property_name, viewport_tile_rects, FileHit, FileTile, GridHit, GridMark,
    GridMenus, GridViewOptions, LogLineView, LogListHit, MeasurePointer, NumberDragSignals,
    PanelFrame, PropertyCheckbox, ShapeMark, TreeClick, TreeRow, VectorRowMode, VectorRowStyle,
    ViewportBarState, ViewportBarTooltips, ViewportCamera, ViewportHamburgerMenu,
    ViewportOnScreenWarning, ViewportShowMenu, ViewportToolbarHit, GAME_SCREEN_RECT,
};

use seal_editor_core::Layout;

/// Run one immediate-mode frame without opening a window.
/// Used to prove input and accessibility in tests and in headless checks.
pub fn run_frame(input: egui::RawInput, body: impl FnMut(&mut egui::Ui)) -> egui::PlatformOutput {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    finish(ctx.run_ui(input, body))
}

pub(crate) fn finish(mut output: egui::FullOutput) -> egui::PlatformOutput {
    let platform = std::mem::take(&mut output.platform_output);
    output.textures_delta.clear();
    platform
}

pub fn default_input(events: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::Vec2::new(1280.0, 800.0),
        )),
        events,
        focused: true,
        ..Default::default()
    }
}

/// Commit an IME string into a focused text field. Returns the resulting text.
pub fn commit_ime(existing: &str, committed: &str) -> String {
    commit_ime_field(existing, committed, "scratch")
}

/// Commit an IME string into the scratch document field. Returns the resulting text.
pub fn commit_ime_scratch(existing: &str, committed: &str) -> String {
    commit_ime_field(existing, committed, "scratch")
}

fn commit_ime_field(existing: &str, committed: &str, field: &str) -> String {
    ime_session(existing, field, &[ImeStep::Commit(committed.to_string())]).0
}

/// One step a platform IME sends into the focused field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImeStep {
    /// Composition preview. An empty string dismisses the preview.
    Preedit { text: String, active_chars: Option<std::ops::Range<usize>> },
    /// Final text from the IME.
    Commit(String),
}

/// Drive preedit and commit the way Windows, macOS, and Linux IMEs do.
///
/// Returns `(field text, last preedit)`. CI runs this on Ubuntu, Windows, and macOS.
pub fn ime_session(existing: &str, field: &str, steps: &[ImeStep]) -> (String, String) {
    let ctx = egui::Context::default();
    let mut text = existing.to_string();
    finish(ctx.run_ui(default_input(Vec::new()), |ui| {
        let response = ui
            .add(egui::TextEdit::singleline(&mut text).id_salt(field).desired_width(f32::INFINITY));
        response.request_focus();
    }));
    let mut last_preedit = String::new();
    for step in steps {
        let event = match step {
            ImeStep::Preedit { text, active_chars } => {
                last_preedit.clone_from(text);
                egui::ImeEvent::Preedit {
                    text: text.clone(),
                    active_range_chars: active_chars.clone(),
                }
            }
            ImeStep::Commit(committed) => {
                last_preedit.clear();
                egui::ImeEvent::Commit(committed.clone())
            }
        };
        finish(ctx.run_ui(default_input(vec![egui::Event::Ime(event)]), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut text).id_salt(field).desired_width(f32::INFINITY),
            );
        }));
    }
    (text, last_preedit)
}

/// True when `text` contains a right-to-left letter (Arabic or Hebrew).
pub fn text_has_rtl_letter(text: &str) -> bool {
    text.chars().any(|ch| {
        let code = ch as u32;
        (0x0590..=0x08FF).contains(&code)
    })
}

pub fn accesskit_has_nodes(label: &str) -> bool {
    let output = run_frame(default_input(Vec::new()), |ui| {
        ui.label(label);
        let _ = a11y::named_button(ui, "Open");
    });
    accesskit_contains(&output, label)
}

pub fn layout_roundtrip(layout: &Layout) -> Layout {
    DockHost::from_layout(layout).to_layout()
}

#[cfg(test)]
mod tests {
    use super::*;
    use seal_editor_core::{DockNode, FloatingWindow, Layout};

    #[test]
    fn docking_roundtrip_keeps_splits_tabs_and_a_floating_window() {
        let mut layout = Layout::default();
        layout.floating.push(FloatingWindow {
            x: 30.0,
            y: 40.0,
            width: 480.0,
            height: 320.0,
            monitor: Some("side".into()),
            node: DockNode::tabs(["settings"]),
        });
        let restored = layout_roundtrip(&layout);
        assert_eq!(restored.root, layout.root);
        assert_eq!(restored.floating.len(), 1);
        assert_eq!(restored.floating[0].node, layout.floating[0].node);
        let ctx = egui::Context::default();
        let mut host = DockHost::from_layout(&layout);
        finish(ctx.run_ui(default_input(Vec::new()), |ui| {
            let menu = [
                "Close".to_string(),
                "Close Others".to_string(),
                "Float".to_string(),
                "Maximize".to_string(),
                "Copy Name".to_string(),
            ];
            show_dock(ui, &mut host.state, &|id| id.to_string(), &menu, false, &mut |ui, id| {
                ui.label(id);
            });
        }));
        let shown = host.to_layout();
        assert_eq!(shown.floating[0].width, layout.floating[0].width);
    }

    #[test]
    fn a_tab_menu_closes_the_others_and_copies_the_name() {
        let mut state =
            egui_dock::DockState::new(vec!["outliner".to_string(), "console".to_string()]);
        assert_eq!(apply_tab_menu(&mut state, "outliner", 1), TabMenuResult::None);
        assert!(state.find_tab(&"console".to_string()).is_none());
        assert!(state.find_tab(&"outliner".to_string()).is_some());
        assert_eq!(
            apply_tab_menu(&mut state, "outliner", 4),
            TabMenuResult::PinSidebar("outliner".into())
        );
        assert_eq!(apply_tab_menu(&mut state, "outliner", 5), TabMenuResult::Maximize);
    }

    #[test]
    fn ime_commit_appends_to_the_focused_field() {
        let text = commit_ime("hello", "世界");
        assert_eq!(text, "hello世界");
    }

    #[test]
    fn ime_commit_writes_complex_script_into_the_scratch_field() {
        let text = commit_ime_scratch("note: ", IME_COMPLEX_SAMPLE);
        assert_eq!(text, format!("note: {IME_COMPLEX_SAMPLE}"));
        assert!(text_has_rtl_letter(IME_COMPLEX_SAMPLE));
    }

    #[test]
    fn ime_preedit_then_commit_keeps_arabic_order_and_cjk() {
        let arabic = "مرحبا";
        let (during, preedit) = ime_session(
            "",
            "scratch",
            &[ImeStep::Preedit { text: "مر".into(), active_chars: Some(0..2) }],
        );
        assert_eq!(preedit, "مر");
        assert_eq!(during, "مر", "preedit is shown in the field until commit");
        let (text, preedit) = ime_session(
            "start ",
            "scratch",
            &[
                ImeStep::Preedit {
                    text: arabic.into(),
                    active_chars: Some(0..arabic.chars().count()),
                },
                ImeStep::Commit(format!("{arabic}世界")),
            ],
        );
        assert!(preedit.is_empty());
        assert_eq!(text, format!("start {arabic}世界"));
        assert!(text_has_rtl_letter(&text));
        assert!(text.contains('世'));
    }

    #[test]
    fn accesskit_tree_includes_the_panel_label() {
        assert!(accesskit_has_nodes("Welcome"));
    }
}
