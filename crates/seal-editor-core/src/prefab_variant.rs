//! Create prefab variants from the Asset Browser.

use std::path::Path;

use crate::command::CommandArgs;
use crate::content::refresh_content_tree;
use crate::error::CoreError;
use crate::session::EditorSession;

fn project_root(session: &EditorSession) -> Result<std::path::PathBuf, CoreError> {
    session
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .ok_or_else(|| CoreError::new("open a project first"))
}

/// `prefab.create_variant path=`: write a variant that points at the chosen prefab.
pub fn cmd_create_variant(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let rel = args.get("path").ok_or_else(|| CoreError::new("choose a prefab"))?.replace('\\', "/");
    if rel.contains("..") {
        return Err(CoreError::new("prefab path must stay inside the project"));
    }
    let root = project_root(session)?;
    let file =
        if root.join(&rel).is_file() { root.join(&rel) } else { root.join("prefabs").join(&rel) };
    let base_stem = file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| CoreError::new("choose a prefab file"))?;
    let base_text = std::fs::read_to_string(&file)
        .map_err(|_| CoreError::new(format!("prefab {rel} was not found")))?;
    if seal_world::prefab_variant::is_variant_document(&base_text) {
        return Err(CoreError::new("choose a base prefab, not another variant"));
    }
    let prefabs = file.parent().unwrap_or_else(|| Path::new("prefabs"));
    let out = seal_world::prefab_variant::unique_variant_path(prefabs, &base_stem);
    let variant_name = out
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Variant".into());
    let expanded = seal_world::prefab_variant::expand_prefab_file(prefabs, &base_text)
        .map_err(CoreError::new)?;
    let compact = seal_world::prefab_variant::compact_variant_file(
        prefabs,
        &base_stem,
        &expanded,
        &variant_name,
    )
    .map_err(CoreError::new)?;
    crate::persist::atomic_write(&out, compact.as_bytes())?;
    refresh_content_tree(session);
    session.notify(format!("Created variant {variant_name}"), false);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Layout;
    use crate::project::create_project;
    use crate::session::EditorSession;
    use seal_sealtext::parse;

    #[test]
    fn variant_inherits_and_refresh_keeps_override() {
        let dir = std::env::temp_dir().join(format!("seal-core-var-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        create_project(&dir, "Game").unwrap();
        let prefabs = dir.join("prefabs");
        std::fs::create_dir_all(&prefabs).unwrap();
        std::fs::write(
            prefabs.join("Box.prefab"),
            r#"seal_prefab = 1
prefab = "Box"
root = "box"
entities = [ { id = "box", name = "Box", tag = "wood", metal = 0.1 } ]
"#,
        )
        .unwrap();
        let mut session = EditorSession::new(Layout::default());
        session
            .dispatch("file.open_project", &CommandArgs::new(&[("path", dir.to_str().unwrap())]))
            .unwrap();
        session
            .dispatch("prefab.create_variant", &CommandArgs::new(&[("path", "prefabs/Box.prefab")]))
            .unwrap();
        let variant = std::fs::read_dir(&prefabs)
            .unwrap()
            .filter_map(|e| e.ok())
            .find(|e| {
                e.path().extension().is_some_and(|x| x == "prefab") && e.file_name() != "Box.prefab"
            })
            .unwrap()
            .path();
        let text = std::fs::read_to_string(&variant).unwrap();
        assert!(parse(&text).unwrap().get("base").is_some());
        session
            .dispatch(
                "scene.add_prefab",
                &CommandArgs::new(&[(
                    "path",
                    variant.strip_prefix(&dir).unwrap().to_string_lossy().as_ref(),
                )]),
            )
            .unwrap();
        let id = session.models.selection.clone().unwrap();
        session
            .dispatch(
                "inspector.set",
                &CommandArgs::new(&[("id", id.as_str()), ("field", "metal"), ("value", "0.95")]),
            )
            .unwrap();
        session.dispatch("prefab.refresh", &CommandArgs::default()).unwrap();
        assert!((session.world.entity(&id).unwrap().metal - 0.95).abs() < 0.01);
    }
}
