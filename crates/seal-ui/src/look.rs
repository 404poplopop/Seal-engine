//! Colors, spacing and type sizes for the whole editor, derived from the three theme colors.

use egui::{Color32, CornerRadius, FontId, Stroke, TextStyle};

/// Sizes at 100% scale. Widgets read these instead of local pixel literals.
pub const BODY_PX: f32 = 12.0;
pub const SMALL_PX: f32 = 10.5;
pub const TITLE_PX: f32 = 12.0;
pub const ROW_PX: f32 = 22.0;
pub const ROW_COMPACT_PX: f32 = 18.0;
pub const TOOLBAR_PX: f32 = 34.0;
pub const TOOL_BUTTON_PX: f32 = 28.0;
pub const ICON_PX: f32 = 16.0;
pub const TOOL_ROW_PX: f32 = 26.0;
pub const TAB_PX: f32 = 26.0;
pub const STATUS_PX: f32 = 24.0;
pub const MENU_PX: f32 = 24.0;
pub const MENU_ROW_PX: f32 = 22.0;
pub const TREE_INDENT_PX: f32 = 14.0;
pub const PANEL_PAD_PX: i8 = 4;
pub const DOCK_GAP_PX: f32 = 2.0;
pub const CONTROL_RADIUS: u8 = 0;

/// Section-header label. Body text stays at [`BODY_PX`].
pub const SECTION_PX: f32 = 11.0;

/// Saved-state mark. Status ok from the look table.
pub const STATUS_OK: Color32 = Color32::from_rgb(0x3f, 0xb9, 0x50);
pub const AXIS_X: Color32 = Color32::from_rgb(0xe0, 0x44, 0x3e);
pub const AXIS_Y: Color32 = Color32::from_rgb(0x6f, 0xbf, 0x3b);
pub const AXIS_Z: Color32 = Color32::from_rgb(0x3b, 0x82, 0xf6);

#[derive(Clone, Copy)]
pub struct ThemeColors {
    pub background: [u8; 3],
    pub text: [u8; 3],
    pub accent: [u8; 3],
    pub look: Option<seal_editor_core::LookColors>,
}

pub fn apply_look(
    ctx: &egui::Context,
    colors: ThemeColors,
    scale: f32,
    body_px: f32,
    motion: f32,
    compact: bool,
) {
    crate::icons::install_icon_font(ctx);
    // Zoom multiplies every metric. It does not replace the monitor scale.
    ctx.set_zoom_factor(scale.clamp(0.5, 3.0));
    let painted = colors.look.map(paint_from_look);
    let panel = painted.map(|item| item.level1).unwrap_or_else(|| rgb(colors.background));
    let text = painted.map(|item| item.text).unwrap_or_else(|| rgb(colors.text));
    let accent = painted.map(|item| item.accent).unwrap_or_else(|| rgb(colors.accent));
    let dark = brightness(panel) < 128;
    // A theme at the ends of the range asks for strong edges instead of soft tone steps.
    let stark = brightness(panel).abs_diff(brightness(text)) > 250;
    let line = mix(panel, text, if stark { 0.6 } else { 0.14 });
    // Recessed surfaces: text fields, the tab strip and the gaps between panels.
    let well =
        if dark { mix(panel, Color32::BLACK, 0.38) } else { mix(panel, Color32::BLACK, 0.1) };

    let mut visuals = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    let level0 = painted.map(|item| item.level0).unwrap_or(well);
    let level2 = painted.map(|item| item.level2).unwrap_or(mix(panel, text, 0.08));
    let level3 = painted.map(|item| item.level3).unwrap_or(mix(panel, text, 0.16));
    let input = painted.map(|item| item.input).unwrap_or(well);
    let input_border = painted.map(|item| item.input_border).unwrap_or(line);
    let dim = painted.map(|item| item.text_dim).unwrap_or(mix(text, panel, 0.45));
    let selection = painted.map(|item| item.selection).unwrap_or(mix(panel, accent, 0.45));
    let selection_focus = painted.map(|item| item.selection_focus).unwrap_or(accent);
    visuals.panel_fill = panel;
    visuals.window_fill = level0;
    visuals.window_stroke = Stroke::new(1.0, input_border);
    visuals.window_corner_radius = CornerRadius::ZERO;
    visuals.menu_corner_radius = CornerRadius::same(CONTROL_RADIUS);
    visuals.extreme_bg_color = level0;
    visuals.text_edit_bg_color = Some(input);
    visuals.faint_bg_color = level2;
    visuals.code_bg_color = input;
    visuals.weak_text_color = Some(dim);
    visuals.hyperlink_color = accent;
    visuals.selection.bg_fill = selection;
    visuals.selection.stroke = Stroke::new(1.0, accent);
    visuals.disabled_alpha = 0.4;
    if let Some(paint) = painted {
        visuals.error_fg_color = paint.error;
        visuals.warn_fg_color = paint.warn;
    }
    visuals.collapsing_header_frame = true;
    visuals.striped = true;
    visuals.override_text_color = None;

    let radius = CornerRadius::same(CONTROL_RADIUS);
    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = panel;
    widgets.noninteractive.weak_bg_fill = panel;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, line);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0, mix(text, panel, 0.12));
    let states = [
        (&mut widgets.inactive, level2, input_border, text),
        (&mut widgets.hovered, level3, accent, text),
        (&mut widgets.active, mix(level2, accent, 0.28), accent, text),
        (&mut widgets.open, selection_focus, accent, text),
    ];
    for (state, fill, edge, ink) in states {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0, if stark { ink } else { edge });
        state.fg_stroke = Stroke::new(1.0, ink);
        state.corner_radius = radius;
        state.expansion = 0.0;
    }
    widgets.noninteractive.corner_radius = radius;

    ctx.all_styles_mut(|style| {
        style.visuals = visuals.clone();
        style.spacing.item_spacing = egui::vec2(PANEL_PAD_PX as f32, DOCK_GAP_PX);
        style.spacing.button_padding = egui::vec2(6.0, 2.0);
        let row = if compact { ROW_COMPACT_PX } else { ROW_PX };
        style.spacing.interact_size = egui::vec2(row, row);
        let scroll = &mut style.spacing.scroll;
        scroll.floating = true;
        scroll.bar_width = 8.0;
        scroll.floating_width = 8.0;
        scroll.floating_allocated_width = 0.0;
        scroll.bar_inner_margin = 0.0;
        scroll.dormant_background_opacity = 0.0;
        scroll.active_background_opacity = 0.0;
        scroll.interact_background_opacity = 0.0;
        scroll.dormant_handle_opacity = 0.0;
        scroll.active_handle_opacity = 0.9;
        scroll.interact_handle_opacity = 1.0;
        scroll.foreground_color = true;
        style.spacing.indent = TREE_INDENT_PX;
        style.spacing.window_margin = egui::Margin::same(PANEL_PAD_PX);
        style.spacing.menu_margin = egui::Margin::same(2);
        let body = body_px.clamp(8.0, 36.0);
        let small = body * SMALL_PX / BODY_PX;
        style.animation_time = motion.max(0.0);
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(small)),
            (TextStyle::Body, FontId::proportional(body)),
            (TextStyle::Button, FontId::proportional(body)),
            (TextStyle::Heading, FontId::proportional(body.max(TITLE_PX))),
            (TextStyle::Monospace, FontId::monospace(small)),
        ]
        .into();
    });
}

/// Tab strip and splitter look for the dock area, taken from the current style.
pub(crate) fn dock_style(style: &egui::Style) -> egui_dock::Style {
    let visuals = &style.visuals;
    let well = visuals.extreme_bg_color;
    let mut dock = egui_dock::Style::from_egui(style);
    dock.tab_bar.height = TAB_PX;
    dock.tab_bar.show_scroll_bar_on_overflow = false;
    dock.tab_bar.bg_fill = well;
    dock.tab_bar.hline_color = well;
    dock.tab_bar.corner_radius = CornerRadius::ZERO;
    dock.separator.width = DOCK_GAP_PX;
    dock.separator.extra_interact_width = 2.0;
    dock.separator.color_idle = well;
    dock.separator.color_hovered = visuals.hyperlink_color;
    dock.separator.color_dragged = visuals.hyperlink_color;
    // Panels pad themselves, so the viewport can reach the panel edge.
    dock.tab.tab_body.inner_margin = egui::Margin::ZERO;
    dock.tab.tab_body.stroke = Stroke::NONE;
    dock.tab.tab_body.bg_fill = visuals.panel_fill;
    dock.tab.spacing = 1.0;
    let weak = visuals.weak_text_color();
    let level1 = visuals.faint_bg_color;
    let body = visuals.panel_fill;
    let stark =
        visuals.widgets.inactive.bg_stroke.color == visuals.widgets.inactive.fg_stroke.color;
    for (tab, fill, ink) in [
        (&mut dock.tab.inactive, well, weak),
        (&mut dock.tab.inactive_with_kb_focus, well, weak),
        (&mut dock.tab.hovered, level1, visuals.text_color()),
        // The open tab shares its panel's fill so tab and body read as one sheet.
        (&mut dock.tab.active, body, visuals.text_color()),
        (&mut dock.tab.active_with_kb_focus, body, visuals.text_color()),
        (&mut dock.tab.focused, body, visuals.strong_text_color()),
        (&mut dock.tab.focused_with_kb_focus, body, visuals.strong_text_color()),
    ] {
        tab.bg_fill = fill;
        tab.text_color = ink;
        tab.outline_color = if stark { ink } else { fill };
        tab.corner_radius = CornerRadius::ZERO;
    }
    dock.tab.active.outline_color = body;
    dock.tab.focused.outline_color = body;
    dock.tab.hline_below_active_tab_name = false;
    let accent = visuals.hyperlink_color;
    dock.overlay.selection_color =
        egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 76);
    dock.overlay.selection_stroke_width = 1.5;
    dock.overlay.overlay_type = egui_dock::OverlayType::Widgets;
    dock
}

#[derive(Clone, Copy)]
struct Painted {
    level0: Color32,
    level1: Color32,
    level2: Color32,
    level3: Color32,
    input: Color32,
    input_border: Color32,
    text: Color32,
    text_dim: Color32,
    selection: Color32,
    selection_focus: Color32,
    accent: Color32,
    warn: Color32,
    error: Color32,
}

fn paint_from_look(colors: seal_editor_core::LookColors) -> Painted {
    Painted {
        level0: rgb(colors.level0),
        level1: rgb(colors.level1),
        level2: rgb(colors.level2),
        level3: rgb(colors.level3),
        input: rgb(colors.input),
        input_border: rgb(colors.input_border),
        text: rgb(colors.text),
        text_dim: rgb(colors.text_dim),
        selection: rgb(colors.selection),
        selection_focus: rgb(colors.selection_focus),
        accent: rgb(colors.accent),
        warn: rgb(colors.warn),
        error: rgb(colors.error),
    }
}

fn brightness(color: Color32) -> u8 {
    ((u16::from(color.r()) + u16::from(color.g()) + u16::from(color.b())) / 3) as u8
}

fn mix(base: Color32, tint: Color32, amount: f32) -> Color32 {
    let amount = amount.clamp(0.0, 1.0);
    let channel = |from: u8, to: u8| -> u8 {
        (f32::from(from) + (f32::from(to) - f32::from(from)) * amount).round() as u8
    };
    Color32::from_rgb(
        channel(base.r(), tint.r()),
        channel(base.g(), tint.g()),
        channel(base.b(), tint.b()),
    )
}

fn rgb(color: [u8; 3]) -> Color32 {
    Color32::from_rgb(color[0], color[1], color[2])
}
