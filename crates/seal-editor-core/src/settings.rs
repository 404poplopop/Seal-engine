use crate::error::CoreError;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    id: String,
    default_value: String,
    user: Option<String>,
    project: Option<String>,
}

/// One built-in setting: where it sits in the panel, its default, and whether it waits for a restart.
#[derive(Clone, Copy, Debug)]
pub struct SettingMeta {
    pub id: &'static str,
    pub category: &'static str,
    pub default_value: &'static str,
    pub restart: bool,
}

pub fn setting_catalog() -> &'static [SettingMeta] {
    &[
        SettingMeta {
            id: "ui.theme",
            category: "appearance",
            default_value: "dark",
            restart: false,
        },
        SettingMeta {
            id: "ui.scale",
            category: "appearance",
            default_value: "100",
            restart: false,
        },
        SettingMeta {
            id: "ui.density",
            category: "appearance",
            default_value: "comfortable",
            restart: false,
        },
        SettingMeta {
            id: "ui.font_size",
            category: "appearance",
            default_value: "14",
            restart: false,
        },
        SettingMeta {
            id: "ui.follow_os_theme",
            category: "appearance",
            default_value: "false",
            restart: false,
        },
        SettingMeta {
            id: "ui.reduced_motion",
            category: "appearance",
            default_value: "false",
            restart: false,
        },
        SettingMeta {
            id: "ui.play_tint_color",
            category: "appearance",
            default_value: crate::play_tint::DEFAULT_PLAY_TINT_COLOR,
            restart: false,
        },
        SettingMeta {
            id: "ui.play_tint_strength",
            category: "appearance",
            default_value: crate::play_tint::DEFAULT_PLAY_TINT_STRENGTH,
            restart: false,
        },
        SettingMeta {
            id: "ui.show_status_bar",
            category: "appearance",
            default_value: "true",
            restart: false,
        },
        SettingMeta {
            id: "startup.open_last_project",
            category: "startup",
            default_value: "true",
            restart: true,
        },
        SettingMeta {
            id: "startup.show_welcome",
            category: "startup",
            default_value: "true",
            restart: true,
        },
        SettingMeta {
            id: "autosave.debounce_ms",
            category: "autosave",
            default_value: "2000",
            restart: false,
        },
        SettingMeta {
            id: "autosave.max_interval_ms",
            category: "autosave",
            default_value: "30000",
            restart: false,
        },
        SettingMeta {
            id: "undo.max_steps",
            category: "undo",
            default_value: "200",
            restart: false,
        },
        SettingMeta {
            id: "undo.max_memory_mb",
            category: "undo",
            default_value: "64",
            restart: false,
        },
        SettingMeta {
            id: "undo.clear_on_save",
            category: "undo",
            default_value: "false",
            restart: false,
        },
        SettingMeta { id: "log.level", category: "logging", default_value: "info", restart: false },
        SettingMeta {
            id: "log.verbose",
            category: "logging",
            default_value: "false",
            restart: false,
        },
        SettingMeta {
            id: "log.file_retention_days",
            category: "logging",
            default_value: "14",
            restart: true,
        },
        SettingMeta {
            id: "console.max_entries",
            category: "console",
            default_value: "5000",
            restart: false,
        },
        SettingMeta {
            id: "content.show_hidden",
            category: "content",
            default_value: "false",
            restart: false,
        },
        SettingMeta {
            id: "content.use_trash",
            category: "content",
            default_value: "true",
            restart: false,
        },
        SettingMeta {
            id: "agent.bus.enabled",
            category: "agent",
            default_value: "false",
            restart: false,
        },
        SettingMeta {
            id: "agent.bus.require_approval",
            category: "agent",
            default_value: "true",
            restart: false,
        },
        SettingMeta {
            id: "editor.remote.enabled",
            category: "remote",
            default_value: "false",
            restart: false,
        },
        SettingMeta {
            id: "editor.remote.port",
            category: "remote",
            default_value: "47810",
            restart: false,
        },
        SettingMeta {
            id: "editor.remote.token",
            category: "remote",
            default_value: "seal-remote",
            restart: false,
        },
        SettingMeta {
            id: "layout.locked",
            category: "layout",
            default_value: "false",
            restart: false,
        },
        SettingMeta {
            id: "keymap.by_physical_key",
            category: "keymap",
            default_value: "false",
            restart: false,
        },
        SettingMeta {
            id: "notifications.timeout_ms",
            category: "notifications",
            default_value: "4000",
            restart: false,
        },
        SettingMeta {
            id: "physics.collision_layer_names",
            category: "physics",
            default_value: "",
            restart: false,
        },
        SettingMeta {
            id: crate::inspector_fields::SORTING_LAYER_SETTING,
            category: "layers",
            default_value: "",
            restart: false,
        },
        SettingMeta {
            id: "tools.external_editor_path",
            category: "tools",
            default_value: "",
            restart: false,
        },
        SettingMeta {
            id: "tools.external_editor_args",
            category: "tools",
            default_value: crate::editor_tools::DEFAULT_EXTERNAL_ARGS,
            restart: false,
        },
        SettingMeta {
            id: "tools.diff_tool_path",
            category: "tools",
            default_value: "",
            restart: false,
        },
        SettingMeta {
            id: "tools.diff_tool_args",
            category: "tools",
            default_value: crate::editor_tools::DEFAULT_DIFF_ARGS,
            restart: false,
        },
        SettingMeta {
            id: crate::editor_tools::WORKER_THREADS,
            category: "tools",
            default_value: "auto",
            restart: false,
        },
        SettingMeta {
            id: crate::project_audio::MASTER_VOLUME,
            category: "audio",
            default_value: "100",
            restart: false,
        },
        SettingMeta {
            id: crate::project_audio::MUSIC_VOLUME,
            category: "audio",
            default_value: "100",
            restart: false,
        },
        SettingMeta {
            id: crate::project_audio::EFFECTS_VOLUME,
            category: "audio",
            default_value: "100",
            restart: false,
        },
        SettingMeta {
            id: crate::project_audio::OUTPUT_SAMPLE_RATE,
            category: "audio",
            default_value: "",
            restart: false,
        },
        SettingMeta {
            id: crate::project_audio::OUTPUT_BUFFER,
            category: "audio",
            default_value: "",
            restart: false,
        },
        SettingMeta {
            id: crate::project_audio::OUTPUT_BACKEND,
            category: "audio",
            default_value: "",
            restart: false,
        },
        SettingMeta {
            id: "audio.linux.output_sample_rate",
            category: "audio",
            default_value: "",
            restart: false,
        },
        SettingMeta {
            id: "audio.linux.output_buffer",
            category: "audio",
            default_value: "",
            restart: false,
        },
        SettingMeta {
            id: "audio.linux.output_backend",
            category: "audio",
            default_value: "",
            restart: false,
        },
        SettingMeta {
            id: "audio.windows.output_sample_rate",
            category: "audio",
            default_value: "",
            restart: false,
        },
        SettingMeta {
            id: "audio.windows.output_buffer",
            category: "audio",
            default_value: "",
            restart: false,
        },
        SettingMeta {
            id: "audio.windows.output_backend",
            category: "audio",
            default_value: "",
            restart: false,
        },
        SettingMeta {
            id: "audio.macos.output_sample_rate",
            category: "audio",
            default_value: "",
            restart: false,
        },
        SettingMeta {
            id: "audio.macos.output_buffer",
            category: "audio",
            default_value: "",
            restart: false,
        },
        SettingMeta {
            id: "audio.macos.output_backend",
            category: "audio",
            default_value: "",
            restart: false,
        },
        SettingMeta {
            id: "audio.web.output_sample_rate",
            category: "audio",
            default_value: "",
            restart: false,
        },
        SettingMeta {
            id: "audio.web.output_buffer",
            category: "audio",
            default_value: "",
            restart: false,
        },
        SettingMeta {
            id: "audio.web.output_backend",
            category: "audio",
            default_value: "",
            restart: false,
        },
        SettingMeta {
            id: "physics2d.fixed_hz",
            category: "physics2d",
            default_value: "60",
            restart: false,
        },
        SettingMeta {
            id: "physics2d.max_substeps",
            category: "physics2d",
            default_value: "5",
            restart: false,
        },
        SettingMeta {
            id: "physics2d.velocity_iterations",
            category: "physics2d",
            default_value: "4",
            restart: false,
        },
        SettingMeta {
            id: "physics2d.position_iterations",
            category: "physics2d",
            default_value: "0",
            restart: false,
        },
        SettingMeta {
            id: "physics2d.gravity_scale",
            category: "physics2d",
            default_value: "1",
            restart: false,
        },
        SettingMeta {
            id: "physics2d.render_interpolation",
            category: "physics2d",
            default_value: "false",
            restart: false,
        },
        SettingMeta {
            id: "physics2d.sleeping",
            category: "physics2d",
            default_value: "true",
            restart: false,
        },
        SettingMeta {
            id: "physics2d.sleep_linear_threshold",
            category: "physics2d",
            default_value: "0.05",
            restart: false,
        },
        SettingMeta {
            id: "physics2d.sleep_angular_threshold",
            category: "physics2d",
            default_value: "0.05",
            restart: false,
        },
        SettingMeta {
            id: "sprites.sort_layers",
            category: "rendering",
            default_value: "",
            restart: false,
        },
        SettingMeta {
            id: "sprites.sort_axis",
            category: "rendering",
            default_value: "0, 1",
            restart: false,
        },
        SettingMeta { id: "display.title", category: "display", default_value: "", restart: false },
        SettingMeta { id: "display.icon", category: "display", default_value: "", restart: false },
        SettingMeta {
            id: "display.mode",
            category: "display",
            default_value: "windowed",
            restart: false,
        },
        SettingMeta {
            id: "display.width",
            category: "display",
            default_value: "1280",
            restart: false,
        },
        SettingMeta {
            id: "display.height",
            category: "display",
            default_value: "720",
            restart: false,
        },
        SettingMeta {
            id: "display.resizable",
            category: "display",
            default_value: "true",
            restart: false,
        },
        SettingMeta {
            id: "display.always_on_top",
            category: "display",
            default_value: "false",
            restart: false,
        },
        SettingMeta {
            id: "display.vsync",
            category: "display",
            default_value: "true",
            restart: false,
        },
        SettingMeta {
            id: "display.target_fps",
            category: "display",
            default_value: "0",
            restart: false,
        },
        SettingMeta {
            id: crate::render_quality::QUALITY_LEVEL,
            category: "rendering",
            default_value: "high",
            restart: false,
        },
        SettingMeta {
            id: crate::render_quality::MSAA,
            category: "rendering",
            default_value: "4",
            restart: true,
        },
        SettingMeta {
            id: crate::render_quality::SHADOW_MAP,
            category: "rendering",
            default_value: "512",
            restart: false,
        },
        SettingMeta {
            id: crate::render_quality::TEXTURE_MAX,
            category: "rendering",
            default_value: "4096",
            restart: false,
        },
        SettingMeta {
            id: crate::render_quality::ANISOTROPY,
            category: "rendering",
            default_value: "4",
            restart: false,
        },
        SettingMeta {
            id: crate::render_quality::GLOW,
            category: "rendering",
            default_value: "true",
            restart: false,
        },
        SettingMeta {
            id: crate::render_quality::RENDER_SCALE,
            category: "rendering",
            default_value: "100",
            restart: false,
        },
        SettingMeta {
            id: crate::texture_memory::TEXTURE_MEMORY_BUDGET_MB,
            category: "rendering",
            default_value: "512",
            restart: false,
        },
        SettingMeta {
            id: crate::project_dimension::SETTING,
            category: "project",
            default_value: "3d",
            restart: false,
        },
        SettingMeta {
            id: crate::editor_language::SETTING,
            category: "appearance",
            default_value: "en",
            restart: true,
        },
        SettingMeta {
            id: crate::input_live::MOUSE_LOOK_RAW,
            category: "input",
            default_value: "false",
            restart: false,
        },
        SettingMeta {
            id: crate::input_live::PAD_LIGHT,
            category: "input",
            default_value: "",
            restart: false,
        },
        SettingMeta {
            id: "display.hdr_output",
            category: "display",
            default_value: "false",
            restart: false,
        },
        SettingMeta {
            id: "display.paper_white",
            category: "display",
            default_value: "203",
            restart: false,
        },
    ]
}

/// Layered settings. Project overrides user, user overrides the built-in default.
#[derive(Clone, Debug)]
pub struct SettingsStore {
    rows: Vec<Row>,
}

impl SettingsStore {
    pub fn builtin() -> Self {
        Self {
            rows: setting_catalog()
                .iter()
                .map(|meta| Row {
                    id: meta.id.to_string(),
                    default_value: meta.default_value.to_string(),
                    user: None,
                    project: None,
                })
                .collect(),
        }
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.rows.iter().map(|row| row.id.as_str())
    }

    pub fn get(&self, id: &str) -> Option<&str> {
        let row = self.rows.iter().find(|row| row.id == id)?;
        Some(row.project.as_deref().or(row.user.as_deref()).unwrap_or(row.default_value.as_str()))
    }

    pub fn source(&self, id: &str) -> Option<&'static str> {
        let row = self.rows.iter().find(|row| row.id == id)?;
        Some(if row.project.is_some() {
            "project"
        } else if row.user.is_some() {
            "user"
        } else {
            "default"
        })
    }

    pub fn set_user(&mut self, id: &str, value: &str) -> Result<(), CoreError> {
        self.write(id, value, false)
    }

    pub fn set_project(&mut self, id: &str, value: &str) -> Result<(), CoreError> {
        self.write(id, value, true)
    }

    pub fn user_text(&self) -> String {
        let mut entries =
            vec![("schema_version".into(), seal_sealtext::SealValue::Number("1".into()))];
        for row in &self.rows {
            if let Some(value) = &row.user {
                entries.push((row.id.clone(), seal_sealtext::SealValue::String(value.clone())));
            }
        }
        seal_sealtext::SealValue::Map((entries).into()).to_text()
    }

    pub fn apply_user_text(&mut self, text: &str) -> Result<(), CoreError> {
        let value = seal_sealtext::parse(text)?;
        let Some(map) = value.as_map() else {
            return Err(CoreError::new("settings must be a map"));
        };
        for (key, item) in map {
            if key == "schema_version" {
                continue;
            }
            if let seal_sealtext::SealValue::String(text) = item {
                let _ = self.set_user(key, text);
            }
        }
        Ok(())
    }

    pub fn reset(&mut self, id: &str) -> Result<(), CoreError> {
        let row = self.row_mut(id)?;
        row.user = None;
        row.project = None;
        Ok(())
    }

    fn write(&mut self, id: &str, value: &str, project: bool) -> Result<(), CoreError> {
        validate(id, value)?;
        let row = self.row_mut(id)?;
        let slot = if project { &mut row.project } else { &mut row.user };
        *slot = Some(value.to_string());
        Ok(())
    }

    fn row_mut(&mut self, id: &str) -> Result<&mut Row, CoreError> {
        self.rows
            .iter_mut()
            .find(|row| row.id == id)
            .ok_or_else(|| CoreError::new(format!("unknown setting '{id}'")))
    }
}

/// Body text size in pixels. The stored size 14 matches the editor's 12px body.
pub fn body_font_px(font_size: f32) -> f32 {
    (12.0 * font_size / 14.0).clamp(8.0, 36.0)
}

/// Whether a setting row should appear for the settings panel search field.
pub fn setting_matches_query(
    id: &str,
    category: &str,
    title: &str,
    description: &str,
    query: &str,
) -> bool {
    let query = query.to_lowercase();
    if query.is_empty() {
        return true;
    }
    id.to_lowercase().contains(&query)
        || title.to_lowercase().contains(&query)
        || description.to_lowercase().contains(&query)
        || category.to_lowercase().contains(&query)
}

/// Stored theme, unless the operating system theme is followed.
pub fn resolved_theme(stored: &str, follow_os: bool, os_dark: Option<bool>) -> String {
    if !follow_os {
        return stored.to_string();
    }
    match os_dark {
        Some(true) => "dark".to_string(),
        Some(false) => "light".to_string(),
        None => stored.to_string(),
    }
}

/// Seconds of interface animation. Reduced motion turns it off.
pub fn motion_seconds(reduced: bool) -> f32 {
    if reduced {
        0.0
    } else {
        1.0 / 12.0
    }
}

fn validate(id: &str, value: &str) -> Result<(), CoreError> {
    match id {
        "ui.scale" => {
            let number: u32 = value
                .parse()
                .map_err(|_| CoreError::new("ui.scale is a percent from 50 to 300"))?;
            if !(50..=300).contains(&number) {
                return Err(CoreError::new("ui.scale is a percent from 50 to 300"));
            }
        }
        "autosave.debounce_ms" => {
            let number: u64 = value
                .parse()
                .map_err(|_| CoreError::new("autosave.debounce_ms must be at least 500"))?;
            if number < 500 {
                return Err(CoreError::new("autosave.debounce_ms must be at least 500"));
            }
        }
        "ui.theme" if !matches!(value, "dark" | "light" | "contrast") => {
            return Err(CoreError::new("ui.theme must be dark, light, or contrast"));
        }
        "ui.density" if !matches!(value, "comfortable" | "compact") => {
            return Err(CoreError::new("ui.density must be comfortable or compact"));
        }
        "ui.play_tint_color" => {
            let parts: Vec<&str> = value.split(',').collect();
            if parts.len() != 3 {
                return Err(CoreError::new(
                    "ui.play_tint_color must be red,green,blue from 0 to 255",
                ));
            }
            for part in parts {
                let channel: u16 = part.trim().parse().map_err(|_| {
                    CoreError::new("ui.play_tint_color must be red,green,blue from 0 to 255")
                })?;
                if channel > 255 {
                    return Err(CoreError::new(
                        "ui.play_tint_color must be red,green,blue from 0 to 255",
                    ));
                }
            }
        }
        "ui.play_tint_strength" => {
            let number: u16 = value.parse().map_err(|_| {
                CoreError::new("ui.play_tint_strength is a whole percent from 0 to 100")
            })?;
            if number > 100 {
                return Err(CoreError::new(
                    "ui.play_tint_strength is a whole percent from 0 to 100",
                ));
            }
        }
        "log.level" if !matches!(value, "error" | "warn" | "info" | "debug" | "trace") => {
            return Err(CoreError::new("log.level must be error, warn, info, debug, or trace"));
        }
        "log.file_retention_days" => {
            value.parse::<u64>().map_err(|_| {
                CoreError::new("log.file_retention_days must be a non-negative integer")
            })?;
        }
        id if id == crate::editor_tools::WORKER_THREADS => {
            crate::editor_tools::sanitize_worker_threads(value)?;
        }
        id if id.starts_with("audio.") && id.ends_with("_volume") => {
            let n: u16 = value
                .parse()
                .map_err(|_| CoreError::new("audio volumes are percents from 0 to 100"))?;
            if n > 100 {
                return Err(CoreError::new("audio volumes are percents from 0 to 100"));
            }
        }
        id if id == crate::project_audio::OUTPUT_SAMPLE_RATE
            || id.ends_with(".output_sample_rate") =>
        {
            if !value.trim().is_empty()
                && value.parse::<u32>().ok().filter(|r| (8_000..=192_000).contains(r)).is_none()
            {
                return Err(CoreError::new("output sample rate must be empty or 8000–192000"));
            }
        }
        id if id == crate::project_audio::OUTPUT_BUFFER || id.ends_with(".output_buffer") => {
            if !value.trim().is_empty()
                && value.parse::<u32>().ok().filter(|n| (32..=4096).contains(n)).is_none()
            {
                return Err(CoreError::new("output buffer must be empty or 32–4096 frames"));
            }
        }
        id if id == crate::project_audio::OUTPUT_BACKEND || id.ends_with(".output_backend") => {
            if !value.trim().is_empty()
                && !seal_audio::backend::BACKENDS
                    .iter()
                    .any(|b| *b == value.trim().to_ascii_lowercase())
            {
                return Err(CoreError::new("unknown audio output backend"));
            }
        }
        id if crate::window_game::is_display_field(id) => {
            crate::window_game::validate_field(id, value)?;
        }
        id if crate::render_quality::is_render_field(id) => {
            crate::render_quality::validate_field(id, value)?;
        }
        id if id == crate::texture_memory::TEXTURE_MEMORY_BUDGET_MB => {
            let mb: u32 = value
                .trim()
                .parse()
                .map_err(|_| CoreError::new("texture memory budget must be a number"))?;
            if !(1..=65_536).contains(&mb) {
                return Err(CoreError::new("texture memory budget must be 1–65536 MB"));
            }
        }
        id if id == crate::project_dimension::SETTING => {
            crate::project_dimension::validate(value)?;
        }
        id if id == crate::editor_language::SETTING => {
            crate::editor_language::validate(value)?;
        }
        id if id == crate::input_live::MOUSE_LOOK_RAW && !matches!(value, "true" | "false") => {
            return Err(CoreError::new("input.mouse_look_raw is true or false"));
        }
        id if id == crate::input_live::PAD_LIGHT => {
            if let Err(err) = crate::seal_input::parse_pad_light(value) {
                return Err(CoreError::new(err));
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_layer_wins_and_bad_scale_is_rejected() {
        let mut settings = SettingsStore::builtin();
        assert_eq!(settings.get("ui.theme"), Some("dark"));
        settings.set_user("ui.theme", "light").unwrap();
        settings.set_project("ui.theme", "contrast").unwrap();
        assert_eq!(settings.get("ui.theme"), Some("contrast"));
        assert_eq!(settings.source("ui.theme"), Some("project"));
        assert!(settings.set_user("ui.scale", "10").is_err());
        assert!(settings.set_user("missing", "1").is_err());
        settings.reset("ui.theme").unwrap();
        assert_eq!(settings.get("ui.theme"), Some("dark"));
    }

    #[test]
    fn font_size_fifteen_grows_body_text_and_the_os_theme_can_override() {
        let body = body_font_px(15.0);
        assert!((body - 12.0 * 15.0 / 14.0).abs() < 0.01);
        assert!(body > body_font_px(14.0));
        assert_eq!(resolved_theme("light", true, Some(true)), "dark");
        assert_eq!(resolved_theme("light", false, Some(true)), "light");
        assert_eq!(motion_seconds(true), 0.0);
        assert!(motion_seconds(false) > 0.0);
        let mut settings = SettingsStore::builtin();
        settings.set_user("ui.font_size", "15").unwrap();
        assert_eq!(settings.get("ui.font_size"), Some("15"));
    }

    #[test]
    fn search_auto_surfaces_autosave_settings() {
        assert!(setting_matches_query(
            "autosave.debounce_ms",
            "autosave",
            "Autosave delay",
            "Milliseconds to wait after the last edit.",
            "auto",
        ));
        assert!(setting_matches_query(
            "autosave.max_interval_ms",
            "autosave",
            "Autosave interval",
            "Longest wait between saves while editing continues.",
            "auto",
        ));
        assert!(!setting_matches_query(
            "ui.theme",
            "appearance",
            "Theme",
            "Color theme for panels and text.",
            "auto",
        ));
    }

    #[test]
    fn log_level_and_retention_are_rejected_when_invalid() {
        let mut settings = SettingsStore::builtin();
        assert!(settings.set_user("log.level", "loud").is_err());
        assert!(settings.set_user("log.file_retention_days", "-1").is_err());
        settings.set_user("log.level", "debug").unwrap();
        settings.set_user("log.file_retention_days", "0").unwrap();
        assert_eq!(settings.get("log.level"), Some("debug"));
        assert_eq!(settings.get("log.file_retention_days"), Some("0"));
    }
}
