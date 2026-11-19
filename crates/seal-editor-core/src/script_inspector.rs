//! Load custom inspector layouts from graph scripts and plan entity field rows.

use seal_graph::inspector_layout::{InspectorLayout, plan_rows, InspectorRowPlan};
use std::path::Path;

/// Read the inspector layout from a `.graph` file when it is readable.
pub fn layout_from_graph_file(full: &Path) -> Option<InspectorLayout> {
    let text = std::fs::read_to_string(full).ok()?;
    let graph = seal_graph::Graph::from_text(&text).ok()?;
    graph.inspector
}

pub fn layout_for_script(project_root: &Path, script_rel: &str) -> Option<InspectorLayout> {
    if script_rel.trim().is_empty() {
        return None;
    }
    let rel = script_rel.trim().replace('\\', "/");
    layout_from_graph_file(&project_root.join(&rel))
}

/// Default transform-ish keys used in layout tests and planning helpers.
pub const SAMPLE_FIELDS: &[&str] = &["x", "y", "z", "walk_speed", "bounce", "hidden"];

pub fn plan_sample(layout: Option<&InspectorLayout>) -> Vec<InspectorRowPlan> {
    plan_rows(SAMPLE_FIELDS, layout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use seal_graph::inspector_layout::{InspectorFieldRule, InspectorSection};
    use std::fs;

    #[test]
    fn layout_file_round_trip() {
        let dir = std::env::temp_dir().join(format!("seal-layout-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("logic/test.graph");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut graph = seal_graph::Graph::new("Test");
        let mut layout = InspectorLayout::default();
        layout.sections.push(InspectorSection {
            name: "Move".into(),
            order: 0,
        });
        layout.fields.push(InspectorFieldRule {
            field: "bounce".into(),
            section: None,
            order: 5,
            hidden: true,
            min: None,
            max: None,
            help: None,
        });
        graph.inspector = Some(layout.clone());
        fs::write(&path, graph.to_text()).unwrap();
        let loaded = layout_for_script(&dir, "logic/test.graph").expect("layout");
        assert!(loaded.is_hidden("bounce"));
        let plan = plan_sample(Some(&loaded));
        assert!(plan.iter().any(|row| row.field == "bounce" && row.hidden));
    }
}