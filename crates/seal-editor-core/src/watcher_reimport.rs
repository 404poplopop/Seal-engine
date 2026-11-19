//! Reimports the folder watcher noticed, and records each one in Import Activity.

use std::path::Path;

use crate::session::EditorSession;

/// Reimport pixel animations in `dir`, and pictures, sounds and fonts whose
/// names are in `changed_names`. Each real reimport becomes one Import Activity row.
pub fn reimport_external(session: &mut EditorSession, dir: &Path, changed_names: &[String]) {
    for row in crate::aseprite::reimport_dir_outcomes(dir) {
        let rel = project_file(session, dir, &row.name);
        let status = if row.result.is_ok() { "ok" } else { "failed" };
        crate::asset_index::record_import_activity(
            session,
            &rel,
            "sprite import",
            status,
            row.started,
        );
    }
    for row in crate::layered_picture::reimport_dir_outcomes(dir) {
        let rel = project_file(session, dir, &row.name);
        let status = if row.result.is_ok() { "ok" } else { "failed" };
        crate::asset_index::record_import_activity(
            session,
            &rel,
            "layered picture",
            status,
            row.started,
        );
    }
    crate::import_pipeline::reimport_changed(session, dir, changed_names);
}

fn project_file(session: &EditorSession, dir: &Path, name: &str) -> String {
    let full = dir.join(name);
    session
        .project
        .as_ref()
        .and_then(|project| crate::asset_index::relative_to_project(&project.root, &full))
        .unwrap_or_else(|| name.to_string())
}
