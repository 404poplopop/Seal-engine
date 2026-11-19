//! The function bar above the graph canvas.
//!
//! Lists the event graph and every function, and edits the open function's
//! name, inputs, outputs and local variables. Each control writes through
//! `GraphDoc`, which saves the graph.

use seal_editor_core::graph_doc::GraphDoc;

use crate::crisp;

fn field(ui: &mut egui::Ui, id: egui::Id, current: &str, width: f32) -> Option<String> {
    let mut buffer: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| current.to_string());
    let edit = egui::TextEdit::singleline(&mut buffer).id(id).desired_width(width);
    let resp = crisp::well_edit(ui, edit);
    if resp.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, buffer));
        None
    } else {
        let changed = resp.lost_focus() && buffer != current;
        ui.data_mut(|d| d.remove::<String>(id));
        changed.then_some(buffer)
    }
}

fn chip(ui: &mut egui::Ui, label: &str, open: bool) -> bool {
    crisp::button(ui, egui::Button::new(label), true, open).clicked()
}

/// Draw the function list and, when a function is open, its signature.
pub fn show(ui: &mut egui::Ui, doc: &mut GraphDoc) {
    let names: Vec<String> = doc.graph.functions.iter().map(|f| f.name.clone()).collect();
    let page = doc.page.clone();
    ui.horizontal_wrapped(|ui| {
        if chip(ui, "Event graph", page.is_none()) {
            doc.show_events();
        }
        for name in &names {
            if chip(ui, name, page.as_deref() == Some(name.as_str())) {
                doc.show_function(name);
            }
        }
        if chip(ui, "+ Function", false) {
            doc.add_function();
        }
    });
    let Some(name) = doc.page.clone() else { return };
    let Some(func) = doc.graph.functions.iter().find(|f| f.name == name).cloned() else {
        return;
    };
    ui.horizontal_wrapped(|ui| {
        ui.label("Name");
        let nid = ui.id().with(("fn-name", &func.name));
        if let Some(new_name) = field(ui, nid, &func.name, 110.0) {
            let _ = doc.rename_function(&func.name, new_name.trim());
        }
        if chip(ui, "Delete", false) {
            doc.delete_function(&func.name);
            return;
        }
        ui.separator();
        ui.label("Inputs");
        for (index, pin) in func.inputs.iter().enumerate() {
            let id = ui.id().with(("fn-in", &func.name, index));
            if let Some(value) = field(ui, id, &pin.name, 72.0) {
                let _ = doc.set_input_name(&func.name, index, value.trim());
            }
            let id = ui.id().with(("fn-in-ty", &func.name, index));
            if let Some(value) = field(ui, id, pin.ty.name(), 78.0) {
                let _ = doc.set_input_type(&func.name, index, value.trim());
            }
            if chip(ui, "x", false) {
                doc.remove_input(&func.name, index);
            }
        }
        if chip(ui, "+ Input", false) {
            doc.add_input(&func.name);
        }
        ui.separator();
        ui.label("Outputs");
        for (index, pin) in func.outputs.iter().enumerate() {
            let id = ui.id().with(("fn-out", &func.name, index));
            if let Some(value) = field(ui, id, &pin.name, 72.0) {
                let _ = doc.set_output_name(&func.name, index, value.trim());
            }
            let id = ui.id().with(("fn-out-ty", &func.name, index));
            if let Some(value) = field(ui, id, pin.ty.name(), 78.0) {
                let _ = doc.set_output_type(&func.name, index, value.trim());
            }
            if chip(ui, "x", false) {
                doc.remove_output(&func.name, index);
            }
        }
        if chip(ui, "+ Output", false) {
            doc.add_output(&func.name);
        }
        ui.separator();
        ui.label("Locals");
        for (index, local) in func.locals.iter().enumerate() {
            let id = ui.id().with(("fn-local", &func.name, index));
            if let Some(value) = field(ui, id, &local.name, 72.0) {
                let _ = doc.set_local_name(&func.name, index, value.trim());
            }
            let id = ui.id().with(("fn-local-ty", &func.name, index));
            if let Some(value) = field(ui, id, local.ty.name(), 78.0) {
                let _ = doc.set_local_type(&func.name, index, value.trim());
            }
            if chip(ui, "x", false) {
                doc.remove_local(&func.name, index);
            }
        }
        if chip(ui, "+ Local", false) {
            doc.add_local(&func.name);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{accesskit_contains, default_input, run_frame};

    #[test]
    fn function_bar_lists_the_event_graph_and_adds_a_function() {
        let dir = std::env::temp_dir().join(format!("seal-fnbar-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("logic").join("hello.graph");
        let mut doc = GraphDoc::open(&path, "logic/hello.graph").expect("open");
        let output = run_frame(default_input(Vec::new()), |ui| {
            show(ui, &mut doc);
        });
        assert!(accesskit_contains(&output, "Event graph"));
        assert!(accesskit_contains(&output, "+ Function"));
        assert!(doc.graph.functions.is_empty());
        doc.add_function();
        assert_eq!(doc.graph.functions.len(), 1);
        assert_eq!(doc.page.as_deref(), Some("function"));
        let output = run_frame(default_input(Vec::new()), |ui| {
            show(ui, &mut doc);
        });
        assert!(accesskit_contains(&output, "function"));
        assert!(accesskit_contains(&output, "Inputs"));
        assert!(accesskit_contains(&output, "Outputs"));
        assert!(accesskit_contains(&output, "Locals"));
        assert!(accesskit_contains(&output, "Delete"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
