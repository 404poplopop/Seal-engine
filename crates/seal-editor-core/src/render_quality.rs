//! Rendering quality levels and the values a packaged game reads from `render.txt`.

use std::path::Path;

use seal_sealtext::SealValue;

use crate::error::CoreError;

pub const RENDER_FILE: &str = "render.txt";
pub const QUALITY_LEVEL: &str = "rendering.quality_level";
pub const MSAA: &str = "rendering.msaa";
pub const SHADOW_MAP: &str = "rendering.shadow_map_size";
pub const TEXTURE_MAX: &str = "rendering.texture_max_size";
pub const ANISOTROPY: &str = "rendering.anisotropy";
pub const GLOW: &str = "rendering.glow";
pub const RENDER_SCALE: &str = "rendering.render_scale";

pub const FIELD_IDS: &[&str] =
    &[QUALITY_LEVEL, MSAA, SHADOW_MAP, TEXTURE_MAX, ANISOTROPY, GLOW, RENDER_SCALE, TEXTURE_BUDGET];

/// Stored with the other render fields so exported games load it from project.seal.
pub const TEXTURE_BUDGET: &str = crate::texture_memory::TEXTURE_MEMORY_BUDGET_MB;

/// One named quality preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QualityLevel {
    pub id: &'static str,
    pub msaa: u32,
    pub shadow_map_size: u32,
    pub texture_max_size: u32,
    pub anisotropy: u32,
    pub glow: bool,
    /// Percent of the window resolution used when drawing (100 = full).
    pub render_scale: u32,
}

pub const LEVELS: &[QualityLevel] = &[
    QualityLevel {
        id: "low",
        msaa: 0,
        shadow_map_size: 256,
        texture_max_size: 1024,
        anisotropy: 1,
        glow: false,
        render_scale: 75,
    },
    QualityLevel {
        id: "medium",
        msaa: 2,
        shadow_map_size: 384,
        texture_max_size: 2048,
        anisotropy: 2,
        glow: true,
        render_scale: 90,
    },
    QualityLevel {
        id: "high",
        msaa: 4,
        shadow_map_size: 512,
        texture_max_size: 4096,
        anisotropy: 4,
        glow: true,
        render_scale: 100,
    },
    QualityLevel {
        id: "ultra",
        msaa: 8,
        shadow_map_size: 768,
        texture_max_size: 8192,
        anisotropy: 8,
        glow: true,
        render_scale: 100,
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderQuality {
    pub level: String,
    pub msaa: u32,
    pub shadow_map_size: u32,
    pub texture_max_size: u32,
    pub anisotropy: u32,
    pub glow: bool,
    pub render_scale: u32,
}

impl Default for RenderQuality {
    fn default() -> Self {
        from_level("high")
    }
}

pub fn level_by_id(id: &str) -> Option<&'static QualityLevel> {
    LEVELS.iter().find(|level| level.id == id)
}

pub fn from_level(id: &str) -> RenderQuality {
    let preset = level_by_id(id).unwrap_or(&LEVELS[2]);
    RenderQuality {
        level: preset.id.into(),
        msaa: preset.msaa,
        shadow_map_size: preset.shadow_map_size,
        texture_max_size: preset.texture_max_size,
        anisotropy: preset.anisotropy,
        glow: preset.glow,
        render_scale: preset.render_scale,
    }
}

pub fn from_store(get: impl Fn(&str) -> Option<String>) -> RenderQuality {
    let level = get(QUALITY_LEVEL).unwrap_or_else(|| "high".into());
    let level = level.as_str();
    let mut out = from_level(level);
    if let Some(text) = get(MSAA) {
        out.msaa = parse_u32(&text, 0, 8).unwrap_or(out.msaa);
    }
    if let Some(text) = get(SHADOW_MAP) {
        out.shadow_map_size = parse_u32(&text, 64, 2048).unwrap_or(out.shadow_map_size);
    }
    if let Some(text) = get(TEXTURE_MAX) {
        out.texture_max_size = parse_u32(&text, 0, 8192).unwrap_or(out.texture_max_size);
    }
    if let Some(text) = get(ANISOTROPY) {
        out.anisotropy = parse_u32(&text, 1, 16).unwrap_or(out.anisotropy);
    }
    if let Some(text) = get(GLOW) {
        out.glow = text == "true";
    }
    if let Some(text) = get(RENDER_SCALE) {
        out.render_scale = parse_u32(&text, 25, 100).unwrap_or(out.render_scale);
    }
    out
}

pub fn from_session(session: &crate::EditorSession) -> RenderQuality {
    from_store(|id| {
        session
            .models
            .settings
            .iter()
            .find(|row| row.id == id)
            .map(|row| row.value().to_string())
            .or_else(|| session.foundation.settings.get(id).map(str::to_string))
    })
}

pub fn from_effective(get: impl Fn(&str) -> String) -> RenderQuality {
    from_store(|id| {
        let value = get(id);
        if value.is_empty() { None } else { Some(value) }
    })
}

pub fn for_export(session: &crate::EditorSession, platform: &str) -> RenderQuality {
    from_effective(|id| crate::platform_overrides::effective_from_session(session, id, platform))
}

pub fn editor_shadow_map_size(session: &crate::EditorSession) -> usize {
    from_session(session).shadow_map_size as usize
}

/// Apply a quality level's bundled values to the settings store and model rows.
pub fn apply_level_to_session(session: &mut crate::EditorSession, level: &str) -> Result<(), CoreError> {
    let preset = level_by_id(level).ok_or_else(|| CoreError::new("quality level must be low, medium, high, or ultra"))?;
    let pairs = [
        (QUALITY_LEVEL, preset.id),
        (MSAA, &preset.msaa.to_string()),
        (SHADOW_MAP, &preset.shadow_map_size.to_string()),
        (TEXTURE_MAX, &preset.texture_max_size.to_string()),
        (ANISOTROPY, &preset.anisotropy.to_string()),
        (GLOW, if preset.glow { "true" } else { "false" }),
        (RENDER_SCALE, &preset.render_scale.to_string()),
    ];
    for (id, value) in pairs {
        if session.project.is_some() {
            session.foundation.settings.set_project(id, value)?;
            if let Some(root) = session.project.as_ref().map(|p| p.root.clone()) {
                crate::project::set_setting(&root, id, SealValue::String(value.into()))?;
            }
        } else {
            session.foundation.settings.set_user(id, value)?;
        }
        session
            .models
            .set_setting(id, value, if session.project.is_some() { "project" } else { "user" })
            .ok_or_else(|| CoreError::new("unknown setting"))?;
    }
    sync_world_glow(session);
    session.settings_dirty = true;
    Ok(())
}

/// The quality level only allows glow; whether a scene glows stays the scene's own
/// setting (Scene Settings > Glow), so opening a project never rewrites its scenes.
pub fn sync_world_glow(session: &mut crate::EditorSession) {
    session.models.quality_glow = from_session(session).glow;
}

pub fn runner_text(quality: &RenderQuality) -> String {
    format!(
        "level {}\nmsaa {}\nshadow_map_size {}\ntexture_max_size {}\nanisotropy {}\nglow {}\nrender_scale {}\n",
        quality.level,
        quality.msaa,
        quality.shadow_map_size,
        quality.texture_max_size,
        quality.anisotropy,
        bool_word(quality.glow),
        quality.render_scale,
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunnerQuality {
    pub level: String,
    pub msaa: u32,
    pub shadow_map_size: u32,
    pub texture_max_size: u32,
    pub anisotropy: u32,
    pub glow: bool,
    pub render_scale: u32,
}

pub fn parse_runner(text: &str) -> Result<RunnerQuality, String> {
    if text.trim().is_empty() {
        return Err("render settings are empty".into());
    }
    let mut level = "high".to_string();
    let mut msaa = 4u32;
    let mut shadow_map_size = 512u32;
    let mut texture_max_size = 4096u32;
    let mut anisotropy = 4u32;
    let mut glow = true;
    let mut render_scale = 100u32;
    let mut seen = std::collections::BTreeSet::new();
    for (line_no, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = match line.split_once(' ') {
            Some((key, value)) => (key.trim(), value.trim()),
            None => (line, ""),
        };
        if !seen.insert(key.to_string()) {
            return Err(format!("line {}: {key} is set twice", line_no + 1));
        }
        match key {
            "level" => level = value.to_string(),
            "msaa" => {
                msaa = parse_u32(value, 0, 8)
                    .map_err(|e| format!("line {}: {e}", line_no + 1))?;
            }
            "shadow_map_size" => {
                shadow_map_size =
                    parse_u32(value, 64, 2048).map_err(|e| format!("line {}: {e}", line_no + 1))?;
            }
            "texture_max_size" => {
                texture_max_size =
                    parse_u32(value, 0, 8192).map_err(|e| format!("line {}: {e}", line_no + 1))?;
            }
            "anisotropy" => {
                anisotropy = parse_u32(value, 1, 16).map_err(|e| format!("line {}: {e}", line_no + 1))?;
            }
            "glow" => glow = parse_bool(value).map_err(|e| format!("line {}: {e}", line_no + 1))?,
            "render_scale" => {
                render_scale =
                    parse_u32(value, 25, 100).map_err(|e| format!("line {}: {e}", line_no + 1))?;
            }
            other => return Err(format!("line {}: unknown render setting {other}", line_no + 1)),
        }
    }
    if level_by_id(&level).is_none() {
        return Err(format!("unknown quality level {level}"));
    }
    Ok(RunnerQuality {
        level,
        msaa,
        shadow_map_size,
        texture_max_size,
        anisotropy,
        glow,
        render_scale,
    })
}

/// Samples per pixel for the window, from a project's saved render settings (read at start).
pub fn project_msaa(root: &Path) -> u32 {
    let fields = load_fields(root);
    from_store(|id| fields.iter().find(|(key, _)| key == id).map(|(_, value)| value.clone())).msaa
}

pub fn load_fields(root: &Path) -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(root.join("project.seal")) else {
        return Vec::new();
    };
    let Ok(raw) = seal_sealtext::parse(&text) else {
        return Vec::new();
    };
    let Some(settings) = raw.get("settings") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for id in FIELD_IDS {
        if let Some(SealValue::String(value)) = settings.get(id) {
            if validate_field(id, value).is_ok() {
                out.push(((*id).to_string(), value.clone()));
            }
        }
    }
    out
}

pub fn store_field(root: &Path, id: &str, value: &str) -> Result<(), CoreError> {
    validate_field(id, value)?;
    crate::project::set_setting(root, id, SealValue::String(value.to_string()))
}

pub fn validate_field(id: &str, value: &str) -> Result<(), CoreError> {
    match id {
        QUALITY_LEVEL if level_by_id(value).is_none() => {
            Err(CoreError::new("rendering.quality_level must be low, medium, high, or ultra"))
        }
        MSAA => parse_u32(value, 0, 8).map(|_| ()).map_err(CoreError::new),
        SHADOW_MAP => parse_u32(value, 64, 2048).map(|_| ()).map_err(CoreError::new),
        TEXTURE_MAX => parse_u32(value, 0, 8192).map(|_| ()).map_err(CoreError::new),
        ANISOTROPY => parse_u32(value, 1, 16).map(|_| ()).map_err(CoreError::new),
        GLOW if !matches!(value, "true" | "false") => Err(CoreError::new("rendering.glow is true or false")),
        RENDER_SCALE => parse_u32(value, 25, 100).map(|_| ()).map_err(CoreError::new),
        TEXTURE_BUDGET => parse_u32(value, 1, 65536).map(|_| ()).map_err(CoreError::new),
        _ if FIELD_IDS.contains(&id) => Ok(()),
        _ => Err(CoreError::new("unknown render setting")),
    }
}

pub fn is_render_field(id: &str) -> bool {
    FIELD_IDS.contains(&id)
}

fn parse_u32(text: &str, min: u32, max: u32) -> Result<u32, String> {
    let number: u32 = text.trim().parse().map_err(|_| format!("{text} is not a whole number"))?;
    if !(min..=max).contains(&number) {
        return Err(format!("{text} must be between {min} and {max}"));
    }
    Ok(number)
}

fn parse_bool(value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err("glow is true or false".into()),
    }
}

fn bool_word(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_level_selection_changes_applied_values() {
        let mut session = crate::EditorSession::new(crate::layout::Layout::default());
        apply_level_to_session(&mut session, "low").unwrap();
        assert_eq!(session.foundation.settings.get(MSAA), Some("0"));
        assert_eq!(session.foundation.settings.get(SHADOW_MAP), Some("256"));
        assert_eq!(session.foundation.settings.get(TEXTURE_MAX), Some("1024"));
        assert!(!session.models.quality_glow);
        apply_level_to_session(&mut session, "ultra").unwrap();
        assert_eq!(session.foundation.settings.get(MSAA), Some("8"));
        assert_eq!(session.foundation.settings.get(SHADOW_MAP), Some("768"));
        assert!(session.models.quality_glow);
        // The scene's own glow switch is left alone.
        assert!(!session.world.glow.enabled);
    }

    #[test]
    fn runner_text_round_trip() {
        let quality = from_level("medium");
        let text = runner_text(&quality);
        let parsed = parse_runner(&text).unwrap();
        assert_eq!(parsed.level, "medium");
        assert_eq!(parsed.shadow_map_size, 384);
        assert_eq!(parsed.glow, true);
    }

    #[test]
    fn damaged_runner_text_is_rejected() {
        assert!(parse_runner("glow maybe\n").is_err());
        assert!(parse_runner("level cosmic\nmsaa 1\n").is_err());
    }
}