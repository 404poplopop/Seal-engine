//! Per-platform overrides for project settings, stored in `project.seal`.

use std::collections::BTreeMap;
use std::path::Path;

use seal_sealtext::{parse, SealValue};

use crate::error::CoreError;

pub const STORE_KEY: &str = "settings.platform_overrides";

pub const PLATFORMS: &[(&str, &str)] = &[
    ("windows", "setting-platform-windows"),
    ("linux", "setting-platform-linux"),
    ("macos", "setting-platform-macos"),
    ("web", "setting-platform-web"),
    ("android", "setting-platform-android"),
    ("ios", "setting-platform-ios"),
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlatformOverrides {
    pub values: BTreeMap<String, BTreeMap<String, String>>,
}

impl PlatformOverrides {
    pub fn get(&self, setting_id: &str, platform: &str) -> Option<&str> {
        self.values.get(setting_id)?.get(platform).map(String::as_str)
    }

    pub fn set(&mut self, setting_id: &str, platform: &str, value: &str) {
        self.values.entry(setting_id.into()).or_default().insert(platform.into(), value.into());
    }

    pub fn clear(&mut self, setting_id: &str, platform: &str) {
        if let Some(map) = self.values.get_mut(setting_id) {
            map.remove(platform);
            if map.is_empty() {
                self.values.remove(setting_id);
            }
        }
    }

    pub fn enabled(&self, setting_id: &str, platform: &str) -> bool {
        self.get(setting_id, platform).is_some()
    }
}

pub fn host_platform() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "macos",
        "linux" => "linux",
        _ => "linux",
    }
}

pub fn platform_id_valid(id: &str) -> bool {
    PLATFORMS.iter().any(|(platform, _)| *platform == id)
}

pub fn load(root: &Path) -> PlatformOverrides {
    let Ok(text) = std::fs::read_to_string(root.join("project.seal")) else {
        return PlatformOverrides::default();
    };
    let Ok(raw) = parse(&text) else {
        return PlatformOverrides::default();
    };
    let Some(settings) = raw.get("settings") else {
        return PlatformOverrides::default();
    };
    let Some(SealValue::String(blob)) = settings.get(STORE_KEY) else {
        return PlatformOverrides::default();
    };
    parse_blob(blob).unwrap_or_default()
}

pub fn store(root: &Path, book: &PlatformOverrides) -> Result<(), CoreError> {
    let text = to_blob(book);
    crate::project::set_setting(root, STORE_KEY, SealValue::String(text))
}

pub fn effective_value(
    base: &str,
    book: &PlatformOverrides,
    setting_id: &str,
    platform: &str,
) -> String {
    book.get(setting_id, platform).unwrap_or(base).to_string()
}

pub fn effective_from_session(
    session: &crate::EditorSession,
    setting_id: &str,
    platform: &str,
) -> String {
    let base = session
        .models
        .settings
        .iter()
        .find(|row| row.id == setting_id)
        .map(|row| row.value().to_string())
        .unwrap_or_default();
    effective_value(&base, &session.models.platform_overrides, setting_id, platform)
}

pub fn parse_blob(text: &str) -> Result<PlatformOverrides, CoreError> {
    if text.trim().is_empty() {
        return Ok(PlatformOverrides::default());
    }
    let value = parse(text).map_err(|err| CoreError::new(format!("platform overrides: {err}")))?;
    let Some(map) = value.as_map() else {
        return Err(CoreError::new("platform overrides must be a map"));
    };
    let mut out = PlatformOverrides::default();
    for (setting_id, platforms) in map {
        let SealValue::Map(items) = platforms else {
            continue;
        };
        for (platform, item) in &items.entries {
            if !platform_id_valid(platform) {
                continue;
            }
            if let SealValue::String(value) = item {
                out.set(setting_id, platform, value.as_str());
            }
        }
    }
    Ok(out)
}

pub fn to_blob(book: &PlatformOverrides) -> String {
    if book.values.is_empty() {
        return String::new();
    }
    let mut entries = Vec::new();
    for (setting_id, platforms) in &book.values {
        let inner: Vec<(String, SealValue)> = platforms
            .iter()
            .map(|(platform, value)| (platform.clone(), SealValue::String(value.clone())))
            .collect();
        entries.push((setting_id.clone(), SealValue::Map(inner.into())));
    }
    SealValue::Map(entries.into()).to_text()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_layer_round_trip_and_effective_value() {
        let mut book = PlatformOverrides::default();
        book.set("display.width", "android", "720");
        let text = to_blob(&book);
        let again = parse_blob(&text).unwrap();
        assert_eq!(again.get("display.width", "android"), Some("720"));
        assert_eq!(effective_value("1280", &again, "display.width", "android"), "720".to_string());
        assert_eq!(effective_value("1280", &again, "display.width", "linux"), "1280".to_string());
    }

    #[test]
    fn damaged_override_blob_is_rejected() {
        assert!(parse_blob("not a map").is_err());
    }
}
