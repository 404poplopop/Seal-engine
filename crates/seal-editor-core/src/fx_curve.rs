//! Save a curve or gradient into an effect file. A drag is one undo step.
//! The file is written on every change, so closing the window keeps the edit.

use std::path::Path;

use seal_world::curve_text::{load_curve, load_gradient, upsert_line};
use seal_world::particles::EmitterDef;

use crate::command::CommandArgs;
use crate::error::CoreError;
use crate::models::Past;
use crate::session::EditorSession;

/// Draft set while a drag is rewriting one effect field. The value is `path<tab>key`.
pub const FX_FIELD_EDIT: &str = "fx-field-edit";

const KEYS: &[&str] =
    &["size_curve", "alpha_curve", "color_gradient", "velocity_curve", "rotation_curve"];

fn checked_rel(rel: &str) -> Result<String, CoreError> {
    let rel = rel.trim().replace('\\', "/");
    if rel.is_empty() || rel.starts_with('/') || rel.split('/').any(|part| part == "..") {
        return Err(CoreError::new(format!("'{rel}' is not a path inside the project")));
    }
    Ok(rel)
}

fn project_root(session: &EditorSession) -> Result<std::path::PathBuf, CoreError> {
    session
        .project
        .as_ref()
        .map(|project| project.root.clone())
        .ok_or_else(|| CoreError::new("open a project first"))
}

fn apply_field(current: &str, key: &str, value: &str) -> Result<String, CoreError> {
    if !KEYS.contains(&key) {
        return Err(CoreError::new(format!("unknown effect curve {key}")));
    }
    if !value.is_empty() {
        let error =
            if key == "color_gradient" { load_gradient(value).1 } else { load_curve(value).1 };
        if let Some(error) = error {
            return Err(CoreError::new(error));
        }
    }
    let stored = if value.is_empty() { None } else { Some(value) };
    Ok(upsert_line(current, key, stored))
}

fn remember(session: &mut EditorSession, path: &str, previous: String, next: String) {
    if let Some(Past::File { path: slot, next: value, .. }) = session.past.last_mut() {
        if slot == path {
            *value = next;
            return;
        }
    }
    session.push_past(Past::File { path: path.to_string(), previous, next });
    session.future.clear();
}

/// `asset.fx_text path= key= value= phase=`.
///
/// `phase` is `begin` (first frame of a drag), `preview` (later frames), `end`
/// (pointer released), `cancel` (Escape), or `commit` (a click, a preset, a typed value).
pub(crate) fn cmd_fx_text(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let rel = checked_rel(args.get("path").unwrap_or(""))?;
    if !rel.ends_with(".fx") {
        return Err(CoreError::new("curves are stored on an effect file"));
    }
    let key = args.get("key").unwrap_or("").to_string();
    let value = args.get("value").unwrap_or("");
    let phase = args.get("phase").unwrap_or("commit");
    let root = project_root(session)?;
    let full = root.join(&rel);
    if !full.is_file() {
        return Err(CoreError::new(format!("{rel} was not found")));
    }
    let current = std::fs::read_to_string(&full)?;
    // Parsing the effect rejects a file that is already broken for another reason.
    EmitterDef::parse(&current).map_err(|error| CoreError::new(format!("{rel}: {error}")))?;
    let next = apply_field(&current, &key, value)?;
    let path = full.to_string_lossy().to_string();
    let mark = format!("{rel}\t{key}");
    match phase {
        "cancel" => {
            if session.draft(FX_FIELD_EDIT).as_deref() == Some(mark.as_str()) {
                if let Some(Past::File { path, previous, .. }) = session.past.pop() {
                    crate::persist::atomic_write(Path::new(&path), previous.as_bytes())?;
                }
                session.clear_draft(FX_FIELD_EDIT)?;
                session.future.clear();
            }
            Ok(())
        }
        "begin" => {
            if next != current {
                crate::persist::atomic_write(&full, next.as_bytes())?;
                remember(session, &path, current, next);
            }
            session.set_draft(FX_FIELD_EDIT, &mark)?;
            Ok(())
        }
        "preview" => {
            if next != current {
                crate::persist::atomic_write(&full, next.as_bytes())?;
            }
            if session
                .past
                .iter()
                .rev()
                .any(|item| matches!(item, Past::File { path: slot, .. } if slot == &path))
            {
                remember(session, &path, current, next);
            } else if next != current {
                remember(session, &path, current, next);
            }
            session.set_draft(FX_FIELD_EDIT, &mark)?;
            Ok(())
        }
        "end" => {
            if next != current {
                crate::persist::atomic_write(&full, next.as_bytes())?;
                remember(session, &path, current, next);
            }
            if session.draft(FX_FIELD_EDIT).is_some() {
                session.clear_draft(FX_FIELD_EDIT)?;
            }
            Ok(())
        }
        _ => {
            if next != current {
                crate::persist::atomic_write(&full, next.as_bytes())?;
                session.push_past(Past::File { path, previous: current, next });
                session.future.clear();
            }
            if session.draft(FX_FIELD_EDIT).is_some() {
                session.clear_draft(FX_FIELD_EDIT)?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seal_world::particles::{Emitter, Particle};

    fn constant_curve(value: &str) -> String {
        format!("keys = [{{time = 0, value = {value}, in = 0, out = 0, ease = \"constant\"}}]")
    }

    fn project() -> (std::path::PathBuf, EditorSession) {
        let dir = std::env::temp_dir().join(format!("seal-fx-curve-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let project = dir.join("P");
        std::fs::create_dir_all(project.join("fx")).unwrap();
        let id = format!("{:08x}-0000-7000-9044-{:012x}", std::process::id(), 707);
        std::fs::write(
            project.join("project.seal"),
            format!(
                "schema_version = 1\nname = \"P\"\nid = \"{id}\"\ncreated = 1794814358\neditor_version = \"0.01+7bdf2aadb05a\"\nmodules = []\nsettings = {{}}\nignore = [\".seal-editor/\"]\n"
            ),
        )
        .unwrap();
        std::fs::write(project.join("scene.seal"), "scene\n").unwrap();
        std::fs::write(project.join("fx/Sparks.fx"), EmitterDef::default().to_text()).unwrap();
        let mut session = EditorSession::new(crate::layout::Layout::default());
        session
            .dispatch(
                "file.open-project",
                &CommandArgs::new(&[("path", project.to_str().unwrap())]),
            )
            .unwrap();
        (project, session)
    }

    #[test]
    fn a_drag_is_one_undo_step_and_the_effect_file_keeps_the_curve() {
        let (project, mut session) = project();
        let first = constant_curve("1");
        let second = constant_curve("4");
        let before = session.past.len();
        let send = |session: &mut EditorSession, value: &str, phase: &str| {
            session
                .dispatch(
                    "asset.fx_text",
                    &CommandArgs::new(&[
                        ("path", "fx/Sparks.fx"),
                        ("key", "size_curve"),
                        ("value", value),
                        ("phase", phase),
                    ]),
                )
                .unwrap();
        };
        send(&mut session, &first, "begin");
        send(&mut session, &second, "preview");
        send(&mut session, &second, "end");
        assert_eq!(session.past.len(), before + 1);
        let saved = std::fs::read_to_string(project.join("fx/Sparks.fx")).unwrap();
        let def = EmitterDef::parse(&saved).unwrap();
        let (_, error) = load_curve(def.size_curve.as_deref().unwrap());
        assert!(error.is_none(), "{error:?}");
        let (size, _) = Emitter::new(def).look(&Particle {
            pos: [0.0; 3],
            vel: [0.0; 3],
            base_vel: [0.0; 3],
            age: 0.4,
            angle: 0.0,
            ang_vel: 0.0,
            trail: vec![],
            flip_frame: 0,
            noise_tag: 1,
        });
        assert!((size - 4.0).abs() < 1.0e-3, "{size}");
        session.dispatch("edit.undo", &CommandArgs::default()).unwrap();
        let restored = std::fs::read_to_string(project.join("fx/Sparks.fx")).unwrap();
        assert!(EmitterDef::parse(&restored).unwrap().size_curve.is_none());
        let _ = std::fs::remove_dir_all(project.parent().unwrap());
    }
}
