//! Inspector layout editor inside the graph panel.

use seal_editor_core::graph_doc::GraphDoc;
use seal_editor_core::INSPECTOR_FIELD_SPECS;
use seal_editor_core::InspectorFieldRule;
use seal_ui::PanelFrame;

use crate::PanelEvent;

pub fn paint_strip(
    frame: &mut PanelFrame,
    doc: &mut GraphDoc,
    text: &dyn Fn(&str) -> String,
) -> PanelEvent {
    let layout = doc.graph.inspector.clone().unwrap_or_default();
    frame.padded(|outer| {
        outer.label(&text("graph-inspector-layout-title"));
        outer.muted(&text("graph-inspector-layout-hint"));
        let options: Vec<(&str, String)> = INSPECTOR_FIELD_SPECS
            .iter()
            .map(|spec| (spec.key, spec.key.to_string()))
            .collect();
        let mut pick = layout
            .fields
            .first()
            .map(|f| f.field.clone())
            .unwrap_or_else(|| "x".to_string());
        if let Some(name) = outer
            .choice_row(
                &text("graph-inspector-layout-field"),
                pick.as_str(),
                &options,
                false,
                None,
                None,
                None,
                &mut None,
                None,
            )
            .0
        {
            pick = name;
        }
        let rule = layout.rule_for(&pick).cloned().unwrap_or_else(|| InspectorFieldRule {
            field: pick.clone(),
            ..InspectorFieldRule::default()
        });
        let mut order = rule.order.to_string();
        let mut hidden = if rule.hidden {
            seal_ui::PropertyCheckbox::On
        } else {
            seal_ui::PropertyCheckbox::Off
        };
        let mut min_s = rule.min.map(|v| v.to_string()).unwrap_or_default();
        let mut max_s = rule.max.map(|v| v.to_string()).unwrap_or_default();
        let mut help = rule.help.clone().unwrap_or_default();
        let mut section = rule.section.clone().unwrap_or_default();
        outer.property_validated_row(
            &text("graph-inspector-layout-order"),
            &mut order,
            None,
            false,
            None,
            None,
            None,
            &mut None,
            None,
        );
        if let Some(next) = outer.property_checkbox_row(
            &text("graph-inspector-layout-hidden"),
            hidden,
            false,
            None,
            None,
            &mut None,
            None,
        ) {
            hidden = next;
        }
        outer.property_validated_row(
            &text("graph-inspector-layout-min"),
            &mut min_s,
            None,
            false,
            None,
            None,
            None,
            &mut None,
            None,
        );
        outer.property_validated_row(
            &text("graph-inspector-layout-max"),
            &mut max_s,
            None,
            false,
            None,
            None,
            None,
            &mut None,
            None,
        );
        outer.property_validated_row(
            &text("graph-inspector-layout-section"),
            &mut section,
            None,
            false,
            None,
            None,
            None,
            &mut None,
            None,
        );
        outer.property_validated_row(
            &text("graph-inspector-layout-help"),
            &mut help,
            None,
            false,
            None,
            None,
            None,
            &mut None,
            None,
        );
        if outer.button(&text("graph-inspector-layout-apply")) {
            doc.apply_inspector_rule(InspectorFieldRule {
                field: pick.clone(),
                section: if section.trim().is_empty() {
                    None
                } else {
                    Some(section.trim().to_string())
                },
                    order: order.trim().parse().unwrap_or(0),
                    hidden: hidden == seal_ui::PropertyCheckbox::On,
                min: min_s.trim().parse().ok(),
                max: max_s.trim().parse().ok(),
                help: if help.trim().is_empty() {
                    None
                } else {
                    Some(help)
                },
            });
        }
        if layout.sections.is_empty() && outer.button(&text("graph-inspector-layout-add-section"))
        {
            doc.ensure_inspector_section("Properties");
        }
    });
    PanelEvent::None
}