//! The window a runner opens, read from the same text an export writes.
//!
//! A target frame rate of 0 does not wait between frames.

use std::time::Duration;

/// How the game window opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowMode {
    Windowed,
    Fullscreen,
    Borderless,
}

/// Settings the runner turns into a window.
#[derive(Clone, Debug, PartialEq)]
pub struct RunnerConfig {
    pub title: String,
    pub icon: String,
    pub mode: WindowMode,
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
    pub always_on_top: bool,
    pub vsync: bool,
    pub target_fps: u32,
    pub hdr_output: bool,
    /// Paper white in nits when HDR output is active.
    pub paper_white_nits: f32,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            title: "Game".into(),
            icon: String::new(),
            mode: WindowMode::Windowed,
            width: 1280,
            height: 720,
            resizable: true,
            always_on_top: false,
            vsync: true,
            target_fps: 0,
            hdr_output: false,
            paper_white_nits: 203.0,
        }
    }
}

/// Read export `window.txt` text. Unknown keys and bad values are errors.
pub fn parse(text: &str) -> Result<RunnerConfig, String> {
    if text.trim().is_empty() {
        return Err("window settings are empty".into());
    }
    let mut config = RunnerConfig::default();
    let mut seen = std::collections::BTreeSet::new();
    for (line_no, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // A key with nothing after it is an empty value. Trimming "icon " first
        // would drop the space and look like a line with no value.
        let (key, value) = match line.split_once(' ') {
            Some((key, value)) => (key.trim(), value.trim()),
            None => (line, ""),
        };
        if !seen.insert(key.to_string()) {
            return Err(format!("line {}: {key} is set twice", line_no + 1));
        }
        match key {
            "title" => config.title = value.to_string(),
            "icon" => config.icon = value.replace('\\', "/"),
            "mode" => config.mode = parse_mode(value)?,
            "width" => config.width = parse_size(value, "width")?,
            "height" => config.height = parse_size(value, "height")?,
            "resizable" => config.resizable = parse_bool(value, "resizable")?,
            "always_on_top" => config.always_on_top = parse_bool(value, "always_on_top")?,
            "vsync" => config.vsync = parse_bool(value, "vsync")?,
            "fps" => config.target_fps = parse_fps(value)?,
            "hdr" => config.hdr_output = parse_bool(value, "hdr")?,
            "paper_white" => config.paper_white_nits = parse_paper_white(value)?,
            other => return Err(format!("line {}: unknown window setting {other}", line_no + 1)),
        }
    }
    if config.title.is_empty() {
        config.title = "Game".into();
    }
    Ok(config)
}

/// How long to wait before the next presented frame.
pub fn frame_wait(target_fps: u32, elapsed: Duration) -> Duration {
    if target_fps == 0 {
        return Duration::ZERO;
    }
    let frame = Duration::from_secs(1) / target_fps;
    if elapsed >= frame {
        Duration::ZERO
    } else {
        frame - elapsed
    }
}

fn parse_mode(value: &str) -> Result<WindowMode, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "windowed" | "window" => Ok(WindowMode::Windowed),
        "fullscreen" | "full screen" => Ok(WindowMode::Fullscreen),
        "borderless" => Ok(WindowMode::Borderless),
        other => Err(format!("window mode is windowed, fullscreen, or borderless, not {other}")),
    }
}

fn parse_bool(value: &str, name: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("{name} is true or false")),
    }
}

fn parse_size(value: &str, name: &str) -> Result<u32, String> {
    let number: u32 = value
        .trim()
        .parse()
        .map_err(|_| format!("{name} is a whole number of pixels from 64 to 8192"))?;
    if !(64..=8192).contains(&number) {
        return Err(format!("{name} is a whole number of pixels from 64 to 8192"));
    }
    Ok(number)
}

fn parse_paper_white(value: &str) -> Result<f32, String> {
    let number: f32 = value
        .trim()
        .parse()
        .map_err(|_| "paper white is a number from 80 to 400 nits".to_string())?;
    if !(80.0..=400.0).contains(&number) {
        return Err("paper white is a number from 80 to 400 nits".into());
    }
    Ok(number)
}

fn parse_fps(value: &str) -> Result<u32, String> {
    let number: u32 = value
        .trim()
        .parse()
        .map_err(|_| "target frame rate is a whole number from 0 to 1000".to_string())?;
    if number > 1000 {
        return Err("target frame rate is a whole number from 0 to 1000".into());
    }
    Ok(number)
}
