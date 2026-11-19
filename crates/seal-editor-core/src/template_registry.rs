//! New-project wizard templates: id, title, description, and project bootstrap.

use std::path::Path;

use crate::error::CoreError;
use crate::project::CreateProjectOptions;

/// One wizard choice: metadata plus the function that fills a new project folder.
pub struct ProjectTemplate {
    pub id: &'static str,
    pub title_key: &'static str,
    pub description_key: &'static str,
    /// Flat colour preview for the wizard (RGBA 8-bit, width × height).
    pub preview: (&'static [u8], u32, u32),
    pub is_flat_scene: bool,
    pub create: fn(&Path, &str, &CreateProjectOptions) -> Result<(), CoreError>,
}

fn create_empty_2d(
    folder: &Path,
    name: &str,
    _options: &CreateProjectOptions,
) -> Result<(), CoreError> {
    let scene = folder.join("scene.seal");
    if !scene.exists() {
        std::fs::write(&scene, seal_world::World::empty_scene_text(name))?;
    }
    Ok(())
}

fn create_empty_3d(
    _folder: &Path,
    _name: &str,
    _options: &CreateProjectOptions,
) -> Result<(), CoreError> {
    Ok(())
}

const PREVIEW_2D: &[u8] = &[
    70, 130, 180, 255, 70, 130, 180, 255, 70, 130, 180, 255, 70, 130, 180, 255, 70, 130, 180, 255,
    70, 130, 180, 255, 70, 130, 180, 255, 70, 130, 180, 255, 220, 220, 220, 255, 220, 220, 220,
    255, 220, 220, 220, 255, 220, 220, 220, 255, 220, 220, 220, 255, 220, 220, 220, 255, 220, 220,
    220, 255, 220, 220, 220, 255,
];

const PREVIEW_3D: &[u8] = &[
    90, 70, 50, 255, 90, 70, 50, 255, 90, 70, 50, 255, 90, 70, 50, 255, 90, 70, 50, 255, 90, 70,
    50, 255, 90, 70, 50, 255, 90, 70, 50, 255, 120, 160, 90, 255, 120, 160, 90, 255, 120, 160, 90,
    255, 120, 160, 90, 255, 120, 160, 90, 255, 120, 160, 90, 255, 120, 160, 90, 255, 120, 160, 90,
    255, 60, 60, 80, 255, 60, 60, 80, 255, 60, 60, 80, 255, 60, 60, 80, 255, 60, 60, 80, 255, 60,
    60, 80, 255, 60, 60, 80, 255, 60, 60, 80, 255, 40, 40, 60, 255, 40, 40, 60, 255, 40, 40, 60,
    255, 40, 40, 60, 255, 40, 40, 60, 255, 40, 40, 60, 255, 40, 40, 60, 255, 40, 40, 60, 255,
];

/// All templates shown in the New Project wizard, in display order.
pub static PROJECT_TEMPLATES: &[ProjectTemplate] = &[
    ProjectTemplate {
        id: "2d",
        title_key: "wizard-template-2d",
        description_key: "wizard-template-2d-desc",
        preview: (PREVIEW_2D, 8, 2),
        is_flat_scene: true,
        create: create_empty_2d,
    },
    ProjectTemplate {
        id: "3d",
        title_key: "wizard-template-3d",
        description_key: "wizard-template-3d-desc",
        preview: (PREVIEW_3D, 8, 4),
        is_flat_scene: false,
        create: create_empty_3d,
    },
    ProjectTemplate {
        id: "puzzle",
        title_key: "wizard-template-puzzle",
        description_key: "wizard-template-puzzle-desc",
        preview: crate::template_puzzle::PREVIEW,
        is_flat_scene: true,
        create: crate::template_puzzle::create,
    },
    ProjectTemplate {
        id: "topdown",
        title_key: "wizard-template-topdown",
        description_key: "wizard-template-topdown-desc",
        preview: (PREVIEW_TOPDOWN, 8, 4),
        is_flat_scene: true,
        create: create_topdown,
    },
    ProjectTemplate {
        id: "platformer",
        title_key: "wizard-template-platformer-title",
        description_key: "wizard-template-platformer-desc",
        preview: (PREVIEW_PLATFORMER, 8, 4),
        is_flat_scene: true,
        create: create_platformer,
    },
    ProjectTemplate {
        id: "first_person",
        title_key: "wizard-template-first-person",
        description_key: "wizard-template-first-person-desc",
        preview: crate::template_view3d::PREVIEW_ARENA,
        is_flat_scene: false,
        create: create_first_person,
    },
    ProjectTemplate {
        id: "third_person",
        title_key: "wizard-template-third-person",
        description_key: "wizard-template-third-person-desc",
        preview: crate::template_view3d::PREVIEW_ARENA,
        is_flat_scene: false,
        create: create_third_person,
    },
];

fn create_platformer(
    folder: &Path,
    name: &str,
    _options: &CreateProjectOptions,
) -> Result<(), CoreError> {
    crate::template_platformer::write(folder, name)
}

// Sky, a ledge with a coin, the hero, and ground: 8 x 4 pixels.
const PREVIEW_PLATFORMER: &[u8] = &{
    const S: [u8; 4] = [110, 170, 230, 255];
    const C: [u8; 4] = [250, 205, 60, 255];
    const H: [u8; 4] = [150, 170, 200, 255];
    const G: [u8; 4] = [96, 160, 70, 255];
    const D: [u8; 4] = [140, 96, 60, 255];
    let rows: [[[u8; 4]; 8]; 4] = [
        [S, S, S, S, S, C, S, S],
        [S, S, S, S, G, G, G, S],
        [S, H, S, S, S, S, S, S],
        [G, G, G, D, D, G, G, G],
    ];
    let mut out = [0u8; 128];
    let mut i = 0;
    while i < 128 {
        out[i] = rows[i / 32][(i / 4) % 8][i % 4];
        i += 1;
    }
    out
};

fn create_topdown(
    folder: &Path,
    name: &str,
    _options: &CreateProjectOptions,
) -> Result<(), CoreError> {
    crate::template_topdown::create(folder, name)
}

fn create_first_person(
    folder: &Path,
    name: &str,
    _options: &CreateProjectOptions,
) -> Result<(), CoreError> {
    crate::template_first_person::write(folder, name)
}

fn create_third_person(
    folder: &Path,
    name: &str,
    _options: &CreateProjectOptions,
) -> Result<(), CoreError> {
    crate::template_third_person::write(folder, name)
}

// Grass, a path across it, and a hero dot: 8 x 4 pixels.
const PREVIEW_TOPDOWN: &[u8] = &{
    const G: [u8; 4] = [86, 148, 82, 255];
    const P: [u8; 4] = [196, 170, 120, 255];
    const H: [u8; 4] = [220, 70, 60, 255];
    let rows: [[[u8; 4]; 8]; 4] = [
        [G, G, G, G, G, G, G, G],
        [P, P, P, H, P, P, P, P],
        [G, G, G, P, G, G, G, G],
        [G, G, G, P, G, G, G, G],
    ];
    let mut out = [0u8; 128];
    let mut i = 0;
    while i < 128 {
        out[i] = rows[i / 32][(i / 4) % 8][i % 4];
        i += 1;
    }
    out
};

pub fn template_by_id(id: &str) -> Option<&'static ProjectTemplate> {
    PROJECT_TEMPLATES.iter().find(|t| t.id == id)
}

pub fn apply_template(
    folder: &Path,
    name: &str,
    id: &str,
    options: &CreateProjectOptions,
) -> Result<(), CoreError> {
    if crate::prefab_data_cmds::wizard_user_template(id).is_some() {
        return crate::prefab_data_cmds::apply_wizard_user_template(folder, name, id);
    }
    let t = template_by_id(id).or_else(|| template_by_id("2d")).expect("2d template exists");
    (t.create)(folder, name, options)?;
    crate::project_dimension::apply_template_defaults(folder, t.is_flat_scene)?;
    crate::tutorial_hooks::after_apply_template(folder, id)?;
    Ok(())
}
#[cfg(test)]
mod preview_tests {
    #[test]
    fn previews_match_their_size() {
        for t in super::PROJECT_TEMPLATES.iter() {
            let (px, w, h) = t.preview;
            assert_eq!(px.len(), (w * h * 4) as usize, "{}", t.id);
        }
    }
}
