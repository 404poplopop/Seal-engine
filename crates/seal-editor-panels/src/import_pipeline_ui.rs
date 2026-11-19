//! Project Settings > Import: post-import scripts, custom importers, and reindex.

use seal_editor_core::import_pipeline;
use seal_editor_core::EditorSession;
use seal_ui::PanelFrame;

use crate::PanelEvent;

fn command(id: &str, pairs: &[(&str, &str)]) -> PanelEvent {
    PanelEvent::Command(id.to_string(), seal_editor_core::CommandArgs::new(pairs))
}

pub fn section(frame: &mut PanelFrame, session: &mut EditorSession) -> PanelEvent {
    let Some(root) = session.project.as_ref().map(|project| project.root.clone()) else {
        return PanelEvent::None;
    };
    let mut event = PanelEvent::None;
    frame.title(&session.messages.get("import-pipeline-title"));
    frame.muted(&session.messages.get("import-pipeline-hint"));
    let kind = session.draft("import-pipeline-kind").unwrap_or_else(|| "picture".into());
    let kind_options: Vec<(String, String)> = ["picture", "sound", "font", "effect", "file"]
        .into_iter()
        .map(|id| (id.to_string(), id.to_string()))
        .collect();
    let kind_refs: Vec<(&str, String)> =
        kind_options.iter().map(|(id, label)| (id.as_str(), label.clone())).collect();
    if let Some(picked) =
        frame.choice(&session.messages.get("import-pipeline-kind"), &kind, &kind_refs)
    {
        if picked != kind {
            let _ = session.set_draft("import-pipeline-kind", &picked);
        }
    }
    let book = import_pipeline::load_book(&root);
    if let Some(warning) = &book.load_warning {
        frame.muted(warning);
    }
    let graphs = import_pipeline::graph_files(&root);
    let current = book.script_for(&kind).unwrap_or("");
    let mut graph_options = vec![("".to_string(), session.messages.get("import-pipeline-none"))];
    graph_options.extend(graphs.iter().cloned().map(|path| (path.clone(), path)));
    let graph_refs: Vec<(&str, String)> =
        graph_options.iter().map(|(id, label)| (id.as_str(), label.clone())).collect();
    if let Some(picked) =
        frame.choice(&session.messages.get("import-pipeline-graph"), current, &graph_refs)
    {
        if picked != current {
            event = command(
                "import.script.set",
                &[("kind", kind.as_str()), ("graph", picked.as_str())],
            );
        }
    }
    if current.is_empty() && frame.button(&session.messages.get("import-pipeline-create")) {
        event = command("import.script.create", &[("kind", kind.as_str())]);
    }
    if !current.is_empty() {
        event = field_row(
            frame,
            session,
            &root,
            &kind,
            "x",
            "import-pipeline-pivot-x",
            "import.script.pivot",
            true,
        )
        .or_keep(event);
        event = field_row(
            frame,
            session,
            &root,
            &kind,
            "y",
            "import-pipeline-pivot-y",
            "import.script.pivot",
            true,
        )
        .or_keep(event);
        event = field_row(
            frame,
            session,
            &root,
            &kind,
            "name",
            "import-pipeline-rename",
            "import.script.rename",
            false,
        )
        .or_keep(event);
        event = field_row(
            frame,
            session,
            &root,
            &kind,
            "tag",
            "import-pipeline-tag",
            "import.script.tag",
            false,
        )
        .or_keep(event);
    }
    frame.title(&session.messages.get("import-pipeline-importers"));
    frame.muted(&session.messages.get("import-pipeline-importers-hint"));
    for row in &book.importers {
        let extension = row.extension.clone();
        let line = format!(".{}  {}", row.extension, row.graph);
        frame.row(|frame| {
            frame.label(&line);
            if frame.button(&session.messages.get("import-pipeline-remove")) {
                event = command("import.importer.remove", &[("extension", extension.as_str())]);
            }
        });
    }
    let mut extension = session.draft("import-pipeline-ext").unwrap_or_default();
    if frame.property(&session.messages.get("import-pipeline-extension"), &mut extension) {
        let _ = session.set_draft("import-pipeline-ext", extension.trim());
    }
    let importer_graph = session.draft("import-pipeline-importer-graph").unwrap_or_default();
    if let Some(picked) = frame.choice(
        &session.messages.get("import-pipeline-importer-graph"),
        &importer_graph,
        &graph_refs,
    ) {
        if picked != importer_graph {
            let _ = session.set_draft("import-pipeline-importer-graph", &picked);
        }
    }
    let extension_now = session.draft("import-pipeline-ext").unwrap_or_default();
    let graph_now = session.draft("import-pipeline-importer-graph").unwrap_or_default();
    if !extension_now.trim().is_empty()
        && !graph_now.is_empty()
        && frame.button(&session.messages.get("import-pipeline-add"))
    {
        event = command(
            "import.importer.add",
            &[("extension", extension_now.trim()), ("graph", graph_now.as_str())],
        );
    }
    if frame.button(&session.messages.get("import-pipeline-reindex")) {
        event = command("asset.reindex", &[]);
    }
    event
}

fn field_row(
    frame: &mut PanelFrame,
    session: &mut EditorSession,
    root: &std::path::Path,
    kind: &str,
    key: &str,
    label_key: &str,
    command_id: &str,
    number: bool,
) -> PanelEvent {
    let committed = import_pipeline::script_text(root, kind, key);
    let draft_key = format!("import-pipeline-{kind}-{key}");
    let mut text = session.draft(&draft_key).unwrap_or_else(|| committed.clone());
    if !frame.property(&session.messages.get(label_key), &mut text) {
        return PanelEvent::None;
    }
    let _ = session.set_draft(&draft_key, &text);
    if number {
        match text.parse::<f32>() {
            Ok(value) if value.is_finite() => {}
            _ => return PanelEvent::None,
        }
    }
    if key == "x" || key == "y" {
        let other = if key == "x" { "y" } else { "x" };
        let other_key = format!("import-pipeline-{kind}-{other}");
        let other_text = session
            .draft(&other_key)
            .unwrap_or_else(|| import_pipeline::script_text(root, kind, other));
        let (x, y) = if key == "x" { (text, other_text) } else { (other_text, text) };
        command("import.script.pivot", &[("kind", kind), ("x", x.as_str()), ("y", y.as_str())])
    } else if key == "name" {
        command(command_id, &[("kind", kind), ("name", text.as_str())])
    } else {
        command(command_id, &[("kind", kind), ("tag", text.as_str())])
    }
}

trait OrKeep {
    fn or_keep(self, previous: PanelEvent) -> PanelEvent;
}

impl OrKeep for PanelEvent {
    fn or_keep(self, previous: PanelEvent) -> PanelEvent {
        if matches!(self, PanelEvent::None) {
            previous
        } else {
            self
        }
    }
}
