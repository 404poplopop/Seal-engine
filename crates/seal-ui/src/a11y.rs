//! Accessibility helpers: names, focus tooltips, and AccessKit checks.

use egui::{PlatformOutput, Response, Ui, WidgetInfo, WidgetType};

/// English titles for built-in panels (`panel-*-title` in en.ftl).
pub const BUILTIN_PANELS: &[(&str, &str)] = &[
    ("welcome", "Welcome"),
    ("console", "Console"),
    ("outliner", "Outliner"),
    ("inspector", "Inspector"),
    ("content", "Asset Browser"),
    ("viewport", "Viewport"),
    ("settings", "Settings"),
    ("place", "Place"),
    ("world", "Scene Settings"),
    ("stats", "Statistics"),
    ("recovery", "Recovery"),
    ("commands", "Commands"),
    ("notices", "Notices"),
    ("graph", "Graph"),
];

/// Complex-script sample used in headless IME checks (Arabic + CJK).
pub const IME_COMPLEX_SAMPLE: &str = "مرحبا世界";

pub fn accesskit_contains(output: &PlatformOutput, text: &str) -> bool {
    let Some(update) = &output.accesskit_update else {
        return false;
    };
    update.nodes.iter().any(|(_, node)| node_matches(node, text))
}

#[cfg(test)]
fn tab_path_key(path: egui_dock::TabPath) -> String {
    format!("{:?}", path.node_path())
}

fn node_matches(node: &egui::accesskit::Node, text: &str) -> bool {
    node.label().is_some_and(|label| label.contains(text))
        || node.value().is_some_and(|value| value.contains(text))
        || node.description().is_some_and(|value| value.contains(text))
        || node.tooltip().is_some_and(|value| value.contains(text))
}

/// Give an interactive control an accessible name, hover help, and a focus tooltip.
pub fn named_control(ui: &mut Ui, response: Response, kind: WidgetType, name: &str) -> Response {
    response.widget_info(|| WidgetInfo::labeled(kind, true, name));
    let response = response.on_hover_text(name);
    paint_focus_ring(ui, &response);
    show_focus_tooltip(ui, &response, name);
    response
}

pub fn named_button(ui: &mut Ui, label: &str) -> Response {
    let response = ui.button(label);
    named_control(ui, response, WidgetType::Button, label)
}

/// Hover and keyboard-focus tooltips without replacing existing widget metadata.
pub(crate) fn hover_focus_help(ui: &mut Ui, response: Response, help: &str) -> Response {
    let response = response.on_hover_text(help);
    paint_focus_ring(ui, &response);
    show_focus_tooltip(ui, &response, help);
    response
}

/// Tooltip: bright name, dim shortcut on the right, then one sentence.
pub fn look_tooltip(
    _ui: &Ui,
    response: Response,
    name: &str,
    shortcut: &str,
    detail: &str,
) -> Response {
    let name = name.to_string();
    let shortcut = shortcut.to_string();
    let detail = detail.to_string();
    let paint = move |tip: &mut Ui| {
        let bright = tip.visuals().strong_text_color();
        let dim = tip.visuals().weak_text_color();
        let ink = tip.visuals().text_color();
        tip.horizontal(|tip| {
            tip.label(egui::RichText::new(name.clone()).color(bright));
            if !shortcut.is_empty() {
                tip.with_layout(egui::Layout::right_to_left(egui::Align::Center), |tip| {
                    tip.label(egui::RichText::new(shortcut.clone()).color(dim).small());
                });
            }
        });
        if !detail.is_empty() {
            tip.label(egui::RichText::new(detail.clone()).color(ink).small());
        }
    };
    if response.enabled() {
        response.on_hover_ui(paint)
    } else {
        response.on_disabled_hover_ui(paint)
    }
}

fn show_focus_tooltip(ui: &Ui, response: &Response, text: &str) {
    if !response.has_focus() {
        return;
    }
    let mut tooltip = egui::Tooltip::for_widget(response).width(ui.style().spacing.tooltip_width);
    tooltip.popup = tooltip.popup.open(true);
    tooltip.show(|tip| {
        tip.style_mut().interaction.selectable_labels = false;
        tip.label(text);
    });
}

pub(crate) fn paint_focus_ring(ui: &Ui, response: &Response) {
    if response.has_focus() {
        let visuals = ui.visuals();
        ui.painter().rect_stroke(
            response.rect.shrink(0.5),
            crate::look::CONTROL_RADIUS,
            egui::Stroke::new(1.0, visuals.selection.stroke.color),
            egui::StrokeKind::Inside,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{default_input, run_frame};
    use seal_editor_core::{DockSide, Layout};

    #[test]
    fn accesskit_tree_lists_each_builtin_panel_title() {
        let menu: [String; 4] = std::array::from_fn(|index| format!("Action {index}"));
        for &(panel_id, title) in BUILTIN_PANELS {
            let output = run_frame(default_input(Vec::new()), |ui| {
                let mut state = egui_dock::DockState::new(vec![panel_id.to_string()]);
                crate::show_dock(
                    ui,
                    &mut state,
                    &|id| {
                        if id == panel_id {
                            title.to_string()
                        } else {
                            id.to_string()
                        }
                    },
                    &menu,
                    false,
                    &mut |ui, _| {
                        ui.label(title);
                    },
                );
            });
            assert!(
                accesskit_contains(&output, title),
                "accesskit tree should list the {panel_id} panel as {title}"
            );
        }
    }

    #[test]
    fn keyboard_docking_moves_a_tab_without_pointer_input() {
        let mut host = crate::DockHost::from_layout(&Layout::default());
        let before = host.state.find_tab(&"console".to_string()).map(tab_path_key);
        host.apply_side("console", DockSide::Right);
        let after = host.state.find_tab(&"console".to_string()).map(tab_path_key);
        assert_ne!(before, after, "docking should relocate the console tab without pointer input");
    }

    #[test]
    fn reduced_motion_sets_animation_time_to_zero() {
        let ctx = egui::Context::default();
        crate::apply_look(
            &ctx,
            crate::ThemeColors {
                background: [30, 30, 30],
                text: [220, 220, 220],
                accent: [80, 120, 200],
                look: None,
            },
            1.0,
            14.0,
            0.0,
            false,
        );
        assert_eq!(ctx.global_style().animation_time, 0.0);
    }

    #[test]
    fn a_focused_control_shows_its_tooltip() {
        let output = crate::run_frame(crate::default_input(Vec::new()), |ui| {
            let save = named_button(ui, "Save draft");
            save.request_focus();
        });
        assert!(
            accesskit_contains(&output, "Save draft"),
            "focused control should publish its tooltip text to assistive APIs"
        );
    }
}
