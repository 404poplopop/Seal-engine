//! Editor chrome tint while play or pause is active (temporary edits).

use seal_world::PlayMode;

use crate::session::EditorSession;
use crate::settings::SettingsStore;
use crate::theme::{
    load_theme, refit_theme_contrast, theme_role_color, Rgb, ThemeFile, THEME_SURFACE_ROLES,
};

pub const DEFAULT_PLAY_TINT_COLOR: &str = "48,128,104";
pub const DEFAULT_PLAY_TINT_STRENGTH: &str = "28";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayTintSettings {
    pub color: Rgb,
    pub strength: u8,
}

impl Default for PlayTintSettings {
    fn default() -> Self {
        Self { color: parse_play_tint_color(None), strength: parse_play_tint_strength(None) }
    }
}

pub fn play_tint_active(mode: PlayMode) -> bool {
    matches!(mode, PlayMode::Playing | PlayMode::Paused)
}

pub fn play_tint_settings_from_store(store: &SettingsStore) -> PlayTintSettings {
    PlayTintSettings {
        color: parse_play_tint_color(store.get("ui.play_tint_color")),
        strength: parse_play_tint_strength(store.get("ui.play_tint_strength")),
    }
}

pub fn play_tint_settings(session: &EditorSession) -> PlayTintSettings {
    play_tint_settings_from_store(&session.foundation.settings)
}

/// Whether the shell should paint the play tint for the current session.
pub fn play_tint_applies(session: &EditorSession) -> bool {
    play_tint_active(session.world.mode) && play_tint_settings(session).strength > 0
}

/// Theme after optional play tint; editing mode returns the base theme unchanged.
pub fn effective_editor_theme(session: &EditorSession, theme_id: &str) -> ThemeFile {
    let base = load_theme(theme_id).unwrap_or_else(default_dark_theme);
    if !play_tint_applies(session) {
        return base;
    }
    let cfg = play_tint_settings(session);
    blend_play_tint(&base, cfg.color, cfg.strength)
}

/// Cache key fragment for shell look application while settings or mode change.
pub fn play_tint_look_token(session: &EditorSession) -> String {
    if !play_tint_applies(session) {
        return String::new();
    }
    let cfg = play_tint_settings(session);
    format!(
        "{}:{}:{}:{}:{}",
        match session.world.mode {
            PlayMode::Playing => "run",
            PlayMode::Paused => "pause",
            PlayMode::Editing => "edit",
        },
        cfg.strength,
        cfg.color.red,
        cfg.color.green,
        cfg.color.blue
    )
}

pub fn parse_play_tint_color(text: Option<&str>) -> Rgb {
    let Some(raw) = text else {
        return default_play_tint_rgb();
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return default_play_tint_rgb();
    }
    let parts: Vec<&str> = trimmed.split(',').collect();
    if parts.len() != 3 {
        return default_play_tint_rgb();
    }
    let parse = |part: &str| part.trim().parse::<u16>().ok().map(|n| n.min(255) as u8);
    match (parse(parts[0]), parse(parts[1]), parse(parts[2])) {
        (Some(red), Some(green), Some(blue)) => Rgb { red, green, blue },
        _ => default_play_tint_rgb(),
    }
}

pub fn format_play_tint_color(color: Rgb) -> String {
    format!("{},{},{}", color.red, color.green, color.blue)
}

pub fn parse_play_tint_strength(text: Option<&str>) -> u8 {
    let Some(raw) = text else {
        return default_play_tint_strength();
    };
    let Ok(value) = raw.trim().parse::<u16>() else {
        return default_play_tint_strength();
    };
    if value > 100 {
        default_play_tint_strength()
    } else {
        value as u8
    }
}

pub fn blend_play_tint(base: &ThemeFile, tint: Rgb, strength_percent: u8) -> ThemeFile {
    if strength_percent == 0 {
        return base.clone();
    }
    let amount = f32::from(strength_percent) / 100.0;
    let mut out = base.clone();
    if strength_percent >= 100 {
        for role in roles_to_tint() {
            out.colors.insert(role.to_string(), tint);
        }
        out.background = tint;
        refit_theme_contrast(&mut out);
        return out;
    }
    for role in roles_to_tint() {
        let surface = theme_role_color(base, role);
        let blended = mix_rgb(surface, tint, amount);
        out.colors.insert(role.to_string(), blended);
    }
    out.background = theme_role_color(&out, "background");
    out.text = theme_role_color(&out, "text");
    out.accent = theme_role_color(&out, "accent");
    refit_theme_contrast(&mut out);
    out
}

fn roles_to_tint() -> impl Iterator<Item = &'static str> {
    THEME_SURFACE_ROLES.iter().copied().chain([
        "look_level0",
        "look_level1",
        "look_level2",
        "look_level3",
        "look_input",
        "title_bar",
    ])
}

fn default_play_tint_rgb() -> Rgb {
    parse_play_tint_color(Some(DEFAULT_PLAY_TINT_COLOR))
}

fn default_play_tint_strength() -> u8 {
    parse_play_tint_strength(Some(DEFAULT_PLAY_TINT_STRENGTH))
}

fn default_dark_theme() -> ThemeFile {
    load_theme("dark").expect("built-in dark theme")
}

fn mix_rgb(a: Rgb, b: Rgb, amount_toward_b: f32) -> Rgb {
    let t = amount_toward_b.clamp(0.0, 1.0);
    Rgb {
        red: lerp_u8(a.red, b.red, t),
        green: lerp_u8(a.green, b.green, t),
        blue: lerp_u8(a.blue, b.blue, t),
    }
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let value = f32::from(a) * (1.0 - t) + f32::from(b) * t;
    value.round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Layout;
    use crate::theme::{contrast_ratio, THEME_TEXT_ROLES};

    fn theme_meets_contrast_floor(theme: &ThemeFile) -> bool {
        for &text_role in THEME_TEXT_ROLES {
            for &surface_role in THEME_SURFACE_ROLES {
                let ink = theme_role_color(theme, text_role);
                let surface = theme_role_color(theme, surface_role);
                if contrast_ratio(ink, surface) < 4.5 {
                    return false;
                }
            }
        }
        true
    }

    #[test]
    fn strength_zero_leaves_theme_unchanged() {
        let base = load_theme("dark").unwrap();
        let panel = theme_role_color(&base, "panel");
        let out = blend_play_tint(&base, Rgb { red: 255, green: 0, blue: 0 }, 0);
        assert_eq!(theme_role_color(&out, "panel"), panel);
    }

    #[test]
    fn strength_hundred_fully_blends_surfaces() {
        let base = load_theme("dark").unwrap();
        let tint = Rgb { red: 200, green: 40, blue: 40 };
        let base_panel = theme_role_color(&base, "panel");
        let out = blend_play_tint(&base, tint, 100);
        let panel = theme_role_color(&out, "panel");
        assert_ne!(panel, base_panel);
        assert!(color_distance(panel, tint) < color_distance(base_panel, tint) / 2);
        assert!(theme_meets_contrast_floor(&out));
    }

    fn color_distance(a: Rgb, b: Rgb) -> u32 {
        u32::from(a.red.abs_diff(b.red))
            + u32::from(a.green.abs_diff(b.green))
            + u32::from(a.blue.abs_diff(b.blue))
    }

    #[test]
    fn contrast_floor_holds_at_full_strength() {
        let base = load_theme("light").unwrap();
        let tint = Rgb { red: 255, green: 200, blue: 0 };
        let out = blend_play_tint(&base, tint, 100);
        assert!(theme_meets_contrast_floor(&out));
    }

    #[test]
    fn damaged_color_and_strength_use_defaults() {
        assert_eq!(parse_play_tint_color(Some("not-a-color")), default_play_tint_rgb());
        assert_eq!(parse_play_tint_strength(Some("150")), default_play_tint_strength());
        assert_eq!(parse_play_tint_strength(Some("xy")), default_play_tint_strength());
    }

    #[test]
    fn settings_round_trip_and_damaged_user_file_value() {
        let mut store = SettingsStore::builtin();
        let color = "12,34,56";
        store.set_user("ui.play_tint_color", color).unwrap();
        store.set_user("ui.play_tint_strength", "40").unwrap();
        assert_eq!(store.get("ui.play_tint_color"), Some(color));
        assert_eq!(store.get("ui.play_tint_strength"), Some("40"));
        let text = store.user_text();
        let mut fresh = SettingsStore::builtin();
        fresh.apply_user_text(&text).unwrap();
        assert_eq!(fresh.get("ui.play_tint_color"), Some(color));
        assert_eq!(fresh.get("ui.play_tint_strength"), Some("40"));
        assert_eq!(parse_play_tint_strength(Some("oops")), default_play_tint_strength());
    }

    #[test]
    fn session_reports_tint_only_while_playing_or_paused() {
        let mut session = EditorSession::new(Layout::default());
        session.foundation.settings.set_user("ui.play_tint_strength", "30").unwrap();
        assert!(!play_tint_applies(&session));
        session.world.mode = PlayMode::Playing;
        assert!(play_tint_applies(&session));
        session.world.mode = PlayMode::Paused;
        assert!(play_tint_applies(&session));
        session.world.mode = PlayMode::Editing;
        assert!(!play_tint_applies(&session));
        session.foundation.settings.set_user("ui.play_tint_strength", "0").unwrap();
        session.world.mode = PlayMode::Playing;
        assert!(!play_tint_applies(&session));
    }

    #[test]
    fn effective_theme_differs_only_in_play() {
        let mut session = EditorSession::new(Layout::default());
        session.foundation.settings.set_user("ui.play_tint_strength", "50").unwrap();
        let base = load_theme("dark").unwrap();
        let editing = effective_editor_theme(&session, "dark");
        assert_eq!(theme_role_color(&editing, "panel"), theme_role_color(&base, "panel"));
        session.world.mode = PlayMode::Playing;
        let live = effective_editor_theme(&session, "dark");
        assert_ne!(theme_role_color(&live, "panel"), theme_role_color(&base, "panel"));
    }
}
