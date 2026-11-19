//! Pick a sound output backend from project settings and the build target.

/// Names shown in Project Settings for the output driver.
pub const BACKENDS: &[&str] = &["default", "wasapi", "coreaudio", "pulse", "alsa", "web"];

/// Short name for the platform this binary was built for.
pub fn build_platform() -> &'static str {
    #[cfg(target_arch = "wasm32")]
    {
        return "web";
    }
    #[cfg(target_os = "windows")]
    {
        return "windows";
    }
    #[cfg(target_os = "macos")]
    {
        return "macos";
    }
    #[cfg(target_os = "linux")]
    {
        return "linux";
    }
    #[cfg(not(any(target_arch = "wasm32", target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        "other"
    }
}

/// Suggested backend when a project leaves the field empty.
pub fn platform_default_backend() -> &'static str {
    match build_platform() {
        "windows" => "wasapi",
        "macos" => "coreaudio",
        "linux" => "pulse",
        "web" => "web",
        _ => "default",
    }
}

/// Merge global audio settings with a per-platform override.
pub fn effective_backend(platform: &str, global: &str, platform_override: &str) -> String {
    let trimmed = platform_override.trim();
    if !trimmed.is_empty() {
        return trimmed.to_ascii_lowercase();
    }
    let trimmed = global.trim();
    if !trimmed.is_empty() {
        return trimmed.to_ascii_lowercase();
    }
    match platform {
        "windows" => "wasapi".into(),
        "macos" => "coreaudio".into(),
        "linux" => "pulse".into(),
        "web" => "web".into(),
        _ => "default".into(),
    }
}

pub fn effective_u32(_platform: &str, global: &str, platform_override: &str) -> Option<u32> {
    let pick = |text: &str| {
        let text = text.trim();
        if text.is_empty() {
            None
        } else {
            text.parse().ok()
        }
    };
    pick(platform_override).or_else(|| pick(global))
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct OutputSettings {
    pub sample_rate: Option<u32>,
    pub buffer_frames: Option<u32>,
    pub backend: String,
}

/// Label shown for a backend id in Project Settings. The stored value stays the id.
pub fn backend_label(id: &str) -> &'static str {
    match id.trim().to_ascii_lowercase().as_str() {
        "web" => "Web Audio",
        "wasapi" => "wasapi",
        "coreaudio" => "coreaudio",
        "pulse" => "pulse",
        "alsa" => "alsa",
        "default" => "default",
        _ => "default",
    }
}

/// Map a backend name to the host id used at runtime. Unknown names fall back.
pub fn resolve_host_id(name: &str) -> &'static str {
    match name.trim().to_ascii_lowercase().as_str() {
        "wasapi" => "wasapi",
        "coreaudio" => "coreaudio",
        "alsa" => "alsa",
        "pulse" => "pulse",
        "web" => "web",
        "default" | "" => platform_default_backend(),
        _ => platform_default_backend(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_override_wins_over_global_backend() {
        assert_eq!(effective_backend("linux", "wasapi", "alsa"), "alsa");
        assert_eq!(effective_backend("linux", "wasapi", ""), "wasapi");
        assert_eq!(effective_backend("linux", "", ""), "pulse");
    }

    #[test]
    fn sample_rate_prefers_platform_value() {
        assert_eq!(effective_u32("linux", "44100", "48000"), Some(48_000));
        assert_eq!(effective_u32("linux", "44100", ""), Some(44_100));
        assert_eq!(effective_u32("linux", "", ""), None);
    }

    #[test]
    fn unknown_backend_falls_back_to_platform_default() {
        assert_eq!(resolve_host_id("nope"), platform_default_backend());
        assert_eq!(resolve_host_id("wasapi"), "wasapi");
    }

    #[test]
    fn web_backend_is_labelled_web_audio() {
        assert_eq!(backend_label("web"), "Web Audio");
        assert_eq!(backend_label("pulse"), "pulse");
        assert_eq!(resolve_host_id("web"), "web");
        assert_eq!(effective_backend("web", "", ""), "web");
        assert_eq!(effective_backend("web", "pulse", ""), "pulse");
    }
}