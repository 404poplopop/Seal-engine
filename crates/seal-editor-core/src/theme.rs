use std::collections::BTreeMap;

use seal_sealtext::{parse, SealValue};

use crate::error::CoreError;
use crate::workspace::user_state_dir;

pub const DARK: &str = include_str!("../themes/dark.seal");
pub const LIGHT: &str = include_str!("../themes/light.seal");
pub const CONTRAST: &str = include_str!("../themes/contrast.seal");

/// Named editor color roles (THM-001). Every theme must resolve each name.
pub const THEME_ROLE_NAMES: &[&str] = &[
    "accent",
    "accent_hover",
    "accent_pressed",
    "background",
    "border",
    "border_strong",
    "button",
    "button_hover",
    "button_text",
    "divider",
    "error",
    "focus_ring",
    "info",
    "input_background",
    "input_border",
    "menu",
    "menu_hover",
    "panel",
    "panel_header",
    "scrollbar",
    "scrollbar_hover",
    "selection",
    "selection_text",
    "status_bar",
    "success",
    "surface",
    "surface_raised",
    "tab",
    "tab_active",
    "tab_hover",
    "text",
    "text_disabled",
    "text_link",
    "text_muted",
    "title_bar",
    "warning",
    "look_level0",
    "look_level1",
    "look_level2",
    "look_level3",
    "look_input",
    "look_input_border",
    "look_text",
    "look_text_dim",
    "look_text_bright",
    "look_selection",
    "look_selection_focus",
    "look_accent",
    "look_axis_x",
    "look_axis_y",
    "look_axis_z",
    "look_ok",
    "look_warn",
    "look_error",
];

pub const BUILTIN_THEME_IDS: &[&str] = &["dark", "light", "contrast"];

/// Ink roles checked against fill roles for WCAG body text.
pub const THEME_TEXT_ROLES: &[&str] =
    &["text", "text_muted", "text_link", "button_text", "selection_text"];

/// Fill roles that can sit behind ink.
pub const THEME_SURFACE_ROLES: &[&str] = &[
    "accent",
    "accent_hover",
    "accent_pressed",
    "background",
    "button",
    "button_hover",
    "error",
    "info",
    "input_background",
    "menu",
    "menu_hover",
    "panel",
    "panel_header",
    "scrollbar",
    "scrollbar_hover",
    "selection",
    "status_bar",
    "success",
    "surface",
    "surface_raised",
    "tab",
    "tab_active",
    "tab_hover",
    "title_bar",
    "warning",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeFile {
    pub id: String,
    pub background: Rgb,
    pub text: Rgb,
    pub accent: Rgb,
    pub colors: BTreeMap<String, Rgb>,
}

pub fn theme_role_color(theme: &ThemeFile, role: &str) -> Rgb {
    if let Some(color) = theme.colors.get(role) {
        return *color;
    }
    theme.background
}

/// WCAG contrast ratio between two sRGB colors. 1 is identical, 21 is black on white.
pub fn contrast_ratio(first: Rgb, second: Rgb) -> f32 {
    let first = relative_luminance(first);
    let second = relative_luminance(second);
    let (lighter, darker) = if first > second { (first, second) } else { (second, first) };
    (lighter + 0.05) / (darker + 0.05)
}

pub fn parse_theme(text: &str) -> Result<ThemeFile, CoreError> {
    let value = parse(text)?;
    let id = string_field(&value, "id")?;
    let background = color_field(&value, "background")?;
    let text = color_field(&value, "text")?;
    let accent = color_field(&value, "accent")?;
    let high_contrast = id == "contrast";
    let derived = derive_roles(background, text, accent, high_contrast);
    let mut colors = BTreeMap::new();
    for role in THEME_ROLE_NAMES {
        let color = optional_color_field(&value, role)
            .unwrap_or_else(|| derived.get(*role).copied().unwrap_or(background));
        colors.insert((*role).to_string(), color);
    }
    let mut theme = ThemeFile { id, background, text, accent, colors };
    polish_theme_contrast(&mut theme);
    Ok(ThemeFile {
        background: theme_role_color(&theme, "background"),
        text: theme_role_color(&theme, "text"),
        accent: theme_role_color(&theme, "accent"),
        ..theme
    })
}

/// Re-fit ink and surface roles after an external colour blend (play tint).
pub fn refit_theme_contrast(theme: &mut ThemeFile) {
    polish_theme_contrast(theme);
}

fn polish_theme_contrast(theme: &mut ThemeFile) {
    let text = theme.text;
    let mut roles: BTreeMap<&'static str, Rgb> = BTreeMap::new();
    for &name in THEME_SURFACE_ROLES {
        roles.insert(name, theme_role_color(theme, name));
    }
    for &name in THEME_TEXT_ROLES {
        roles.insert(name, theme_role_color(theme, name));
    }
    for &surface in THEME_SURFACE_ROLES {
        let color = fit_surface_for_ink(text, roles[surface]);
        roles.insert(surface, color);
    }
    tune_ink_roles_for_surfaces(text, &mut roles);
    for (name, color) in roles {
        theme.colors.insert(name.to_string(), color);
    }
    if let Some(accent) = theme.colors.get("accent") {
        theme.accent = *accent;
    }
}

/// Five surface levels and the ink that sits on them. Absent when a theme file does not carry them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LookColors {
    pub level0: [u8; 3],
    pub level1: [u8; 3],
    pub level2: [u8; 3],
    pub level3: [u8; 3],
    pub input: [u8; 3],
    pub input_border: [u8; 3],
    pub text: [u8; 3],
    pub text_dim: [u8; 3],
    pub text_bright: [u8; 3],
    pub selection: [u8; 3],
    pub selection_focus: [u8; 3],
    pub accent: [u8; 3],
    pub axis_x: [u8; 3],
    pub axis_y: [u8; 3],
    pub axis_z: [u8; 3],
    pub ok: [u8; 3],
    pub warn: [u8; 3],
    pub error: [u8; 3],
}

pub fn theme_rgb(id: &str) -> ([u8; 3], [u8; 3], [u8; 3]) {
    let theme = load_theme(id).unwrap_or_else(default_dark_fallback);
    rgb_from_theme(&theme)
}

pub fn rgb_from_theme(theme: &ThemeFile) -> ([u8; 3], [u8; 3], [u8; 3]) {
    (
        [theme.background.red, theme.background.green, theme.background.blue],
        [theme.text.red, theme.text.green, theme.text.blue],
        [theme.accent.red, theme.accent.green, theme.accent.blue],
    )
}

pub fn look_from_theme(theme: &ThemeFile) -> Option<LookColors> {
    let color = |role: &str| -> Option<[u8; 3]> {
        theme.colors.get(role).map(|item| [item.red, item.green, item.blue])
    };
    Some(LookColors {
        level0: color("look_level0")?,
        level1: color("look_level1")?,
        level2: color("look_level2")?,
        level3: color("look_level3")?,
        input: color("look_input")?,
        input_border: color("look_input_border")?,
        text: color("look_text")?,
        text_dim: color("look_text_dim")?,
        text_bright: color("look_text_bright")?,
        selection: color("look_selection")?,
        selection_focus: color("look_selection_focus")?,
        accent: color("look_accent")?,
        axis_x: color("look_axis_x")?,
        axis_y: color("look_axis_y")?,
        axis_z: color("look_axis_z")?,
        ok: color("look_ok")?,
        warn: color("look_warn")?,
        error: color("look_error")?,
    })
}

/// Painted surfaces from a theme file that lists `look_level0`. Derived themes return none.
pub fn theme_look(id: &str) -> Option<LookColors> {
    let text =
        match std::fs::read_to_string(user_state_dir().join("themes").join(format!("{id}.seal"))) {
            Ok(text) => text,
            Err(_) => theme_text(id).to_string(),
        };
    if !text.contains("look_level0") {
        return None;
    }
    let theme = parse_theme(&text).ok()?;
    look_from_theme(&theme)
}

pub fn load_theme(id: &str) -> Option<ThemeFile> {
    if let Ok(theme) = load_user_theme(id) {
        return Some(theme);
    }
    parse_theme(theme_text(id)).ok()
}

/// Changes when the on-disk user theme for `id` changes. Built-ins read as 0.
pub fn theme_read_token(id: &str) -> u64 {
    let path = user_state_dir().join("themes").join(format!("{id}.seal"));
    let Ok(meta) = std::fs::metadata(path) else {
        return 0;
    };
    let Ok(modified) = meta.modified() else {
        return 0;
    };
    modified.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() as u64
}

pub fn theme_text(id: &str) -> &'static str {
    match id {
        "light" => LIGHT,
        "contrast" => CONTRAST,
        _ => DARK,
    }
}

fn load_user_theme(id: &str) -> Result<ThemeFile, CoreError> {
    let path = user_state_dir().join("themes").join(format!("{id}.seal"));
    let text = std::fs::read_to_string(path).map_err(CoreError::from)?;
    parse_theme(&text)
}

fn default_dark_fallback() -> ThemeFile {
    ThemeFile {
        id: "dark".into(),
        background: Rgb { red: 28, green: 28, blue: 30 },
        text: Rgb { red: 230, green: 230, blue: 230 },
        accent: Rgb { red: 90, green: 160, blue: 230 },
        colors: BTreeMap::new(),
    }
}

fn string_field(value: &SealValue, key: &str) -> Result<String, CoreError> {
    match value.get(key) {
        Some(SealValue::String(text)) => Ok(text.clone()),
        _ => Err(CoreError::new(format!("theme is missing {key}"))),
    }
}

fn optional_color_field(value: &SealValue, key: &str) -> Option<Rgb> {
    color_field(value, key).ok()
}

fn color_field(value: &SealValue, key: &str) -> Result<Rgb, CoreError> {
    let Some(SealValue::Array(items)) = value.get(key) else {
        return Err(CoreError::new(format!("theme color {key} is missing")));
    };
    if items.len() != 3 {
        return Err(CoreError::new(format!("theme color {key} needs 3 numbers")));
    }
    let channel = |index: usize| match &items[index] {
        SealValue::Number(lexeme) => lexeme
            .parse::<u8>()
            .map_err(|_| CoreError::new(format!("theme color {key} is out of range"))),
        _ => Err(CoreError::new(format!("theme color {key} must be numbers"))),
    };
    Ok(Rgb { red: channel(0)?, green: channel(1)?, blue: channel(2)? })
}

fn derive_roles(
    background: Rgb,
    text: Rgb,
    accent: Rgb,
    high_contrast: bool,
) -> BTreeMap<&'static str, Rgb> {
    if high_contrast {
        return derive_contrast_roles(background, text, accent);
    }
    let dark = relative_luminance(background) < relative_luminance(text);
    let mut roles = BTreeMap::new();
    roles.insert("background", background);
    roles.insert("text", text);
    let accent = fit_surface_for_ink(text, accent);
    roles.insert("accent", accent);
    roles.insert("surface", adjust(background, if dark { 12 } else { -6 }));
    roles.insert("surface_raised", adjust(background, if dark { 18 } else { -12 }));
    roles.insert("panel", adjust(background, if dark { 8 } else { -4 }));
    roles.insert("panel_header", adjust(background, if dark { 14 } else { -8 }));
    roles.insert("border", mix(background, text, if dark { 0.22 } else { 0.18 }));
    roles.insert("border_strong", mix(background, text, if dark { 0.38 } else { 0.32 }));
    roles.insert("divider", mix(background, text, if dark { 0.28 } else { 0.24 }));
    roles.insert(
        "text_muted",
        fit_ink_for_surface(
            text,
            mix(text, background, if dark { 0.32 } else { 0.28 }),
            background,
        ),
    );
    roles.insert("text_disabled", mix(text, background, if dark { 0.65 } else { 0.6 }));
    roles.insert("text_link", fit_ink_for_surface(text, accent, background));
    let accent_hover = fit_surface_for_ink(text, adjust(accent, if dark { 18 } else { -12 }));
    let accent_pressed = fit_surface_for_ink(text, adjust(accent, if dark { -10 } else { 8 }));
    roles.insert("accent_hover", accent_hover);
    roles.insert("accent_pressed", accent_pressed);
    let selection =
        fit_surface_for_ink(text, mix(background, accent, if dark { 0.55 } else { 0.35 }));
    roles.insert("selection", selection);
    roles.insert("selection_text", text);
    roles.insert("focus_ring", accent);
    let mut success = Rgb { red: 72, green: 158, blue: 84 };
    let mut warning = Rgb { red: 210, green: 153, blue: 44 };
    let mut error = Rgb { red: 201, green: 74, blue: 74 };
    let mut info = Rgb { red: 82, green: 148, blue: 226 };
    success = fit_surface_for_ink(text, success);
    warning = fit_surface_for_ink(text, warning);
    error = fit_surface_for_ink(text, error);
    info = fit_surface_for_ink(text, info);
    roles.insert("success", success);
    roles.insert("warning", warning);
    roles.insert("error", error);
    roles.insert("info", info);
    let button = mix(background, text, if dark { 0.12 } else { 0.08 });
    roles.insert("button", button);
    roles.insert(
        "button_hover",
        fit_surface_for_ink(text, adjust(button, if dark { 12 } else { -10 })),
    );
    roles.insert("button_text", text);
    let border = roles.get("border").copied().unwrap_or(background);
    let panel = roles.get("panel").copied().unwrap_or(background);
    let tab = mix(background, text, if dark { 0.1 } else { 0.06 });
    let surface_raised = roles.get("surface_raised").copied().unwrap_or(background);
    let panel_header = roles.get("panel_header").copied().unwrap_or(background);
    roles.insert("input_background", adjust(background, if dark { 6 } else { -4 }));
    roles.insert("input_border", border);
    roles.insert("scrollbar", mix(background, text, if dark { 0.35 } else { 0.28 }));
    roles.insert(
        "scrollbar_hover",
        fit_surface_for_ink(text, mix(background, text, if dark { 0.48 } else { 0.4 })),
    );
    roles.insert("tab", tab);
    roles.insert("tab_active", panel);
    roles.insert("tab_hover", fit_surface_for_ink(text, adjust(tab, if dark { 10 } else { -8 })));
    roles.insert("menu", surface_raised);
    roles.insert(
        "menu_hover",
        fit_surface_for_ink(text, mix(background, accent, if dark { 0.4 } else { 0.25 })),
    );
    roles.insert("status_bar", panel_header);
    roles.insert("title_bar", surface_raised);
    tune_ink_roles_for_surfaces(text, &mut roles);
    roles
}

fn tune_ink_roles_for_surfaces(primary: Rgb, roles: &mut BTreeMap<&'static str, Rgb>) {
    for &ink_role in THEME_TEXT_ROLES {
        if ink_role == "text" {
            continue;
        }
        let Some(mut ink) = roles.get(ink_role).copied() else {
            continue;
        };
        for &surface_role in THEME_SURFACE_ROLES {
            let Some(surface) = roles.get(surface_role).copied() else {
                continue;
            };
            ink = fit_ink_for_surface(primary, ink, surface);
        }
        roles.insert(ink_role, ink);
    }
}

fn derive_contrast_roles(background: Rgb, text: Rgb, accent: Rgb) -> BTreeMap<&'static str, Rgb> {
    let mut roles = BTreeMap::new();
    for &role in THEME_ROLE_NAMES {
        let color = match role {
            "background" | "surface" | "surface_raised" | "panel" | "panel_header"
            | "input_background" | "tab" | "tab_active" | "menu" | "status_bar" | "title_bar"
            | "button" | "scrollbar" => background,
            "text" | "selection_text" | "button_text" | "text_muted" => text,
            "text_disabled" => mix(text, background, 0.35),
            "text_link" => text,
            "accent" | "accent_hover" | "accent_pressed" | "focus_ring" => accent,
            "selection" | "menu_hover" | "tab_hover" | "button_hover" | "scrollbar_hover" => accent,
            "border" | "border_strong" | "divider" | "input_border" => text,
            "success" => Rgb { red: 0, green: 255, blue: 0 },
            "warning" => Rgb { red: 255, green: 255, blue: 0 },
            "error" => Rgb { red: 255, green: 0, blue: 0 },
            "info" => Rgb { red: 0, green: 255, blue: 255 },
            _ => text,
        };
        roles.insert(role, color);
    }
    roles
}

fn relative_luminance(color: Rgb) -> f32 {
    let channel = |value: u8| {
        let v = f32::from(value) / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(color.red) + 0.7152 * channel(color.green) + 0.0722 * channel(color.blue)
}

const MIN_TEXT_CONTRAST: f32 = 4.5;

fn fit_surface_for_ink(ink: Rgb, surface: Rgb) -> Rgb {
    if contrast_ratio(ink, surface) >= MIN_TEXT_CONTRAST {
        return surface;
    }
    let toward = if relative_luminance(ink) > 0.5 {
        Rgb { red: 0, green: 0, blue: 0 }
    } else {
        Rgb { red: 255, green: 255, blue: 255 }
    };
    for step in 1..=24 {
        let mixed = mix(surface, toward, step as f32 / 24.0);
        if contrast_ratio(ink, mixed) >= MIN_TEXT_CONTRAST {
            return mixed;
        }
    }
    toward
}

fn fit_ink_for_surface(ink: Rgb, candidate: Rgb, surface: Rgb) -> Rgb {
    if contrast_ratio(candidate, surface) >= MIN_TEXT_CONTRAST {
        return candidate;
    }
    if contrast_ratio(ink, surface) >= MIN_TEXT_CONTRAST {
        return ink;
    }
    let toward = if relative_luminance(surface) > 0.5 {
        Rgb { red: 0, green: 0, blue: 0 }
    } else {
        Rgb { red: 255, green: 255, blue: 255 }
    };
    for step in 1..=24 {
        let mixed = mix(candidate, toward, step as f32 / 24.0);
        if contrast_ratio(mixed, surface) >= MIN_TEXT_CONTRAST {
            return mixed;
        }
    }
    toward
}

fn mix(a: Rgb, b: Rgb, amount_toward_b: f32) -> Rgb {
    let amount = amount_toward_b.clamp(0.0, 1.0);
    Rgb {
        red: lerp_u8(a.red, b.red, amount),
        green: lerp_u8(a.green, b.green, amount),
        blue: lerp_u8(a.blue, b.blue, amount),
    }
}

fn adjust(color: Rgb, delta: i16) -> Rgb {
    Rgb {
        red: clamp_u8(i16::from(color.red) + delta),
        green: clamp_u8(i16::from(color.green) + delta),
        blue: clamp_u8(i16::from(color.blue) + delta),
    }
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let value = f32::from(a) * (1.0 - t) + f32::from(b) * t;
    clamp_u8(value.round() as i16)
}

fn clamp_u8(value: i16) -> u8 {
    value.clamp(0, 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_theme_files_parse() {
        for &id in BUILTIN_THEME_IDS {
            let theme = parse_theme(theme_text(id)).unwrap();
            assert_eq!(theme.id, id);
            assert!(theme.colors.len() >= 30);
            assert_eq!(theme_role_color(&theme, "text"), theme.text);
        }
        let contrast = parse_theme(CONTRAST).unwrap();
        assert_eq!(contrast.background.red, 0);
        assert_eq!(contrast.text.red, 255);
    }

    #[test]
    fn every_builtin_theme_meets_text_on_surface_contrast() {
        for &id in BUILTIN_THEME_IDS {
            let theme = parse_theme(theme_text(id)).unwrap();
            let mut failures = Vec::new();
            for &text_role in THEME_TEXT_ROLES {
                for &surface_role in THEME_SURFACE_ROLES {
                    let ink = theme_role_color(&theme, text_role);
                    let surface = theme_role_color(&theme, surface_role);
                    let ratio = contrast_ratio(ink, surface);
                    if ratio < MIN_TEXT_CONTRAST {
                        failures.push(format!("{id} {text_role} on {surface_role}: {ratio:.1}:1"));
                    }
                }
            }
            assert!(failures.is_empty(), "{}", failures.join("\n"));
        }
    }

    #[test]
    fn user_theme_file_is_reread_on_each_load() {
        let root = std::env::temp_dir().join(format!("seal-theme-reload-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        // Only this thread sees the override, so tests running alongside keep their folder.
        crate::workspace::TEST_USER_DIR
            .with(|d| *d.borrow_mut() = Some(root.join("seal").join("editor")));
        let path = root.join("seal").join("editor").join("themes").join("dark.seal");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "id = \"dark\"\nbackground = [0, 0, 0]\ntext = [255, 255, 255]\naccent = [255, 200, 0]\n",
        )
        .unwrap();
        let first = load_theme("dark").unwrap();
        assert_eq!(first.text.red, 255);
        std::fs::write(
            &path,
            "id = \"dark\"\nbackground = [0, 0, 0]\ntext = [128, 128, 128]\naccent = [255, 200, 0]\n",
        )
        .unwrap();
        let second = load_theme("dark").unwrap();
        assert_eq!(second.text.red, 128);
        assert!(theme_read_token("dark") > 0);
        crate::workspace::TEST_USER_DIR.with(|d| *d.borrow_mut() = None);
        let _ = std::fs::remove_dir_all(root);
    }
}
