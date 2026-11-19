//! Project default dimension (2D or 3D) drives new-scene defaults.

use std::path::Path;

use seal_sealtext::SealValue;
use seal_world::World;

use crate::error::CoreError;
use crate::import_presets::save_book;

pub const SETTING: &str = "project.default_dimension";

pub fn validate(value: &str) -> Result<(), CoreError> {
    if matches!(value, "2d" | "3d") {
        Ok(())
    } else {
        Err(CoreError::new("project.default_dimension must be 2d or 3d"))
    }
}

pub fn load(root: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(root.join("project.seal")) else {
        return "3d".into();
    };
    let Ok(raw) = seal_sealtext::parse(&text) else {
        return "3d".into();
    };
    let Some(settings) = raw.get("settings") else {
        return "3d".into();
    };
    match settings.get(SETTING) {
        Some(SealValue::String(value)) if value == "2d" => "2d".into(),
        _ => "3d".into(),
    }
}

pub fn store(root: &Path, value: &str) -> Result<(), CoreError> {
    validate(value)?;
    crate::project::set_setting(root, SETTING, SealValue::String(value.into()))
}

pub fn is_2d(session: &crate::EditorSession) -> bool {
    session
        .models
        .settings
        .iter()
        .find(|row| row.id == SETTING)
        .map(|row| row.value() == "2d")
        .unwrap_or(false)
}

/// Write project defaults after a wizard template is applied.
pub fn apply_template_defaults(folder: &Path, flat_scene: bool) -> Result<(), CoreError> {
    let dim = if flat_scene { "2d" } else { "3d" };
    store(folder, dim)?;
    if flat_scene {
        set_sprite_import_default(folder)?;
    }
    Ok(())
}

fn set_sprite_import_default(folder: &Path) -> Result<(), CoreError> {
    let mut book = crate::import_presets::load_book(folder);
    book.defaults.insert("picture".into(), "Pixel Art".into());
    save_book(folder, &book)
}

/// Scene text for **Create → New Scene** using the project default dimension.
pub fn new_scene_text(name: &str, dimension: &str) -> String {
    if dimension == "2d" {
        build_2d_scene(name)
    } else {
        World::empty_scene_text(name)
    }
}

fn build_2d_scene(name: &str) -> String {
    let mut world = World::from_text(&World::empty_scene_text(name)).expect("empty scene");
    world.gravity = 0.0;
    let cam = world.add("empty");
    {
        let entity = world.entity_mut(&cam).expect("camera");
        entity.name = "Camera".into();
        entity.parent = Some("root".into());
        entity.translation = [0.0, 0.0, 10.0];
        entity.tag = "camera".into();
        entity.camera2d.on = true;
        entity.camera2d.zoom = 48.0;
    }
    world.to_text()
}

pub fn bootstrap_new_scene(session: &mut crate::EditorSession, name: &str) -> String {
    let dimension = session
        .models
        .settings
        .iter()
        .find(|row| row.id == SETTING)
        .map(|row| row.value())
        .unwrap_or("3d");
    let text = new_scene_text(name, dimension);
    if dimension == "2d" {
        session.models.viewport_view = "front".into();
        session.models.viewport_tool = "move".into();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_d_mode_drives_new_scene_defaults() {
        let text = new_scene_text("Stage", "2d");
        let world = World::from_text(&text).unwrap();
        assert!(world.entities.iter().any(|e| e.camera2d.on));
        assert_eq!(world.gravity, 0.0);
        let text3d = new_scene_text("Stage", "3d");
        let world3d = World::from_text(&text3d).unwrap();
        assert!(!world3d.entities.iter().any(|e| e.camera2d.on));
    }

    #[test]
    fn old_projects_without_dimension_stay_three_d() {
        let dir = std::env::temp_dir().join(format!("seal-dim-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("project.seal"),
            "schema_version = 1\nname = \"P\"\nid = \"a\"\ncreated = 1\neditor_version = \"0.01\"\nmodules = []\nsettings = {}\nignore = []\n",
        )
        .unwrap();
        assert_eq!(load(&dir), "3d");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
