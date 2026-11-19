//! Game window settings stored with the project and written into an export.
//!
//! The text form is what a packaged game and the runner read. A target frame
//! rate of 0 means the window is not asked to wait between frames.

use std::path::Path;
use std::time::Duration;

use seal_sealtext::SealValue;

use crate::error::CoreError;

/// File written beside the program of a packaged game.
pub const WINDOW_FILE: &str = "window.txt";

/// Panel-state key set while Play in New Window is running.
pub const PLAY_IN_WINDOW_KEY: &str = "play.in_window";

pub const MODE_WINDOWED: &str = "windowed";
pub const MODE_FULLSCREEN: &str = "fullscreen";
pub const MODE_BORDERLESS: &str = "borderless";

/// One display setting id and the value used when the project has not set it.
pub const FIELDS: &[(&str, &str)] = &[
    ("display.title", ""),
    ("display.icon", ""),
    ("display.mode", MODE_WINDOWED),
    ("display.width", "1280"),
    ("display.height", "720"),
    ("display.resizable", "true"),
    ("display.always_on_top", "false"),
    ("display.vsync", "true"),
    ("display.target_fps", "0"),
    ("display.hdr_output", "false"),
    ("display.paper_white", "203"),
];

/// How the game window should open.
#[derive(Clone, Debug, PartialEq)]
pub struct GameWindow {
    pub title: String,
    pub icon: String,
    pub mode: String,
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
    pub always_on_top: bool,
    pub vsync: bool,
    pub target_fps: u32,
    pub hdr_output: bool,
    pub paper_white_nits: f32,
}

impl Default for GameWindow {
    fn default() -> Self {
        Self::from_pairs(|id| field_default(id).to_string())
    }
}

impl GameWindow {
    /// Read the display fields. `get` returns the stored text for an id.
    pub fn from_pairs(get: impl Fn(&str) -> String) -> Self {
        let title = get("display.title");
        let icon = get("display.icon");
        let mode = get("display.mode");
        let width = get("display.width");
        let height = get("display.height");
        let resizable = get("display.resizable");
        let always_on_top = get("display.always_on_top");
        let vsync = get("display.vsync");
        let target_fps = get("display.target_fps");
        let hdr_output = get("display.hdr_output");
        let paper_white = get("display.paper_white");
        Self {
            title: title.trim().to_string(),
            icon: icon.trim().replace('\\', "/"),
            mode: normalize_mode(&mode).unwrap_or_else(|_| MODE_WINDOWED.into()),
            width: parse_u32(&width).unwrap_or(1280).clamp(64, 8192),
            height: parse_u32(&height).unwrap_or(720).clamp(64, 8192),
            resizable: resizable != "false",
            always_on_top: always_on_top == "true",
            vsync: vsync != "false",
            target_fps: parse_u32(&target_fps).unwrap_or(0).min(1000),
            hdr_output: hdr_output == "true",
            paper_white_nits: parse_paper_white(&paper_white).unwrap_or(203.0),
        }
    }

    /// The text a runner reads. An empty title becomes `fallback` (the project name).
    pub fn runner_text(&self, fallback: &str) -> String {
        let title = if self.title.is_empty() { fallback.trim() } else { self.title.trim() };
        let title = if title.is_empty() { "Game" } else { title };
        format!(
            "title {title}\nicon {}\nmode {}\nwidth {}\nheight {}\nresizable {}\nalways_on_top {}\nvsync {}\nfps {}\nhdr {}\npaper_white {}\n",
            self.icon,
            self.mode,
            self.width,
            self.height,
            bool_word(self.resizable),
            bool_word(self.always_on_top),
            bool_word(self.vsync),
            self.target_fps,
            bool_word(self.hdr_output),
            self.paper_white_nits,
        )
    }
}

/// The window a runner opens from [`GameWindow::runner_text`].
#[derive(Clone, Debug, PartialEq)]
pub struct RunnerWindow {
    pub title: String,
    pub icon: String,
    pub mode: String,
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
    pub always_on_top: bool,
    pub vsync: bool,
    pub target_fps: u32,
    pub hdr_output: bool,
    pub paper_white_nits: f32,
}

/// Read runner text. Unknown keys and bad values are errors. Missing keys use the defaults.
pub fn parse_runner(text: &str) -> Result<RunnerWindow, String> {
    if text.trim().is_empty() {
        return Err("window settings are empty".into());
    }
    let mut title = String::new();
    let mut icon = String::new();
    let mut mode = MODE_WINDOWED.to_string();
    let mut width = 1280u32;
    let mut height = 720u32;
    let mut resizable = true;
    let mut always_on_top = false;
    let mut vsync = true;
    let mut fps = 0u32;
    let mut hdr = false;
    let mut paper_white = 203.0f32;
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
            "title" => title = value.to_string(),
            "icon" => icon = value.replace('\\', "/"),
            "mode" => mode = normalize_mode(value)?,
            "width" => width = parse_size(value, "width")?,
            "height" => height = parse_size(value, "height")?,
            "resizable" => resizable = parse_bool(value, "resizable")?,
            "always_on_top" => always_on_top = parse_bool(value, "always_on_top")?,
            "vsync" => vsync = parse_bool(value, "vsync")?,
            "fps" => fps = parse_fps(value)?,
            "hdr" => hdr = parse_bool(value, "hdr")?,
            "paper_white" => paper_white = parse_paper_white(value)?,
            other => return Err(format!("line {}: unknown window setting {other}", line_no + 1)),
        }
    }
    if title.is_empty() {
        title = "Game".into();
    }
    Ok(RunnerWindow {
        title,
        icon,
        mode,
        width,
        height,
        resizable,
        always_on_top,
        vsync,
        target_fps: fps,
        hdr_output: hdr,
        paper_white_nits: paper_white,
    })
}

/// Build the runner window from project display settings.
pub fn runner_config(window: &GameWindow, project_name: &str) -> Result<RunnerWindow, String> {
    parse_runner(&window.runner_text(project_name))
}

/// How long to wait before the next frame. Zero when `target_fps` is 0 or the
/// budget has already passed.
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

/// Reject a value that cannot be stored for `id`. Other ids are left to the caller.
pub fn validate_field(id: &str, value: &str) -> Result<(), CoreError> {
    let bad = |message: &str| Err(CoreError::new(message));
    match id {
        "display.title" | "display.icon" => Ok(()),
        "display.mode" => normalize_mode(value).map(|_| ()).map_err(CoreError::new),
        "display.width" => parse_size(value, "width").map(|_| ()).map_err(CoreError::new),
        "display.height" => parse_size(value, "height").map(|_| ()).map_err(CoreError::new),
        "display.resizable" | "display.always_on_top" | "display.vsync" => {
            parse_bool(value, id).map(|_| ()).map_err(CoreError::new)
        }
        "display.target_fps" => parse_fps(value).map(|_| ()).map_err(CoreError::new),
        "display.hdr_output" => parse_bool(value, id).map(|_| ()).map_err(CoreError::new),
        "display.paper_white" => parse_paper_white(value).map(|_| ()).map_err(CoreError::new),
        _ => bad("unknown display setting"),
    }
}

pub fn is_display_field(id: &str) -> bool {
    FIELDS.iter().any(|(key, _)| *key == id)
}

pub fn field_default(id: &str) -> &'static str {
    FIELDS.iter().find(|(key, _)| *key == id).map(|(_, value)| *value).unwrap_or("")
}

/// Display fields saved in `project.seal`. Missing keys are left out.
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
    for (id, _) in FIELDS {
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

/// Settings for a launch. A `window.txt` beside the project folder wins, then
/// the project's display fields.
pub fn load_for_launch(project: &Path) -> GameWindow {
    if let Some(parent) = project.parent() {
        if let Ok(text) = std::fs::read_to_string(parent.join(WINDOW_FILE)) {
            if let Ok(runner) = parse_runner(&text) {
                return GameWindow {
                    title: runner.title,
                    icon: runner.icon,
                    mode: runner.mode,
                    width: runner.width,
                    height: runner.height,
                    resizable: runner.resizable,
                    always_on_top: runner.always_on_top,
                    vsync: runner.vsync,
                    target_fps: runner.target_fps,
                    hdr_output: runner.hdr_output,
                    paper_white_nits: runner.paper_white_nits,
                };
            }
        }
    }
    let fields = load_fields(project);
    GameWindow::from_pairs(|id| {
        fields
            .iter()
            .find(|(key, _)| key == id)
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| field_default(id).to_string())
    })
}

pub fn from_session(session: &crate::EditorSession) -> GameWindow {
    GameWindow::from_pairs(|id| {
        session
            .models
            .settings
            .iter()
            .find(|row| row.id == id)
            .map(|row| row.value().to_string())
            .unwrap_or_else(|| field_default(id).to_string())
    })
}

pub fn play_in_window(session: &crate::EditorSession) -> bool {
    session.models.panel_states.get(PLAY_IN_WINDOW_KEY).is_some_and(|value| value == "1")
}

/// RGBA8 pixels of a picture, for a window icon. Width and height must be multiples of 4.
pub fn read_icon_rgba(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let (rgba, width, height) = crate::asset_editor::decode_png(path).ok_or_else(|| {
        "the window icon must be an 8-bit picture with an alpha channel".to_string()
    })?;
    if width == 0 || height == 0 || width % 4 != 0 || height % 4 != 0 {
        return Err("the window icon width and height must be multiples of 4".into());
    }
    if rgba.len() != (width as usize) * (height as usize) * 4 {
        return Err("the window icon must be an 8-bit picture with an alpha channel".into());
    }
    Ok((width, height, rgba))
}

pub fn normalize_mode(value: &str) -> Result<String, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        MODE_WINDOWED | "window" => Ok(MODE_WINDOWED.into()),
        MODE_FULLSCREEN | "full screen" => Ok(MODE_FULLSCREEN.into()),
        MODE_BORDERLESS => Ok(MODE_BORDERLESS.into()),
        other => Err(format!("window mode is windowed, fullscreen, or borderless, not {other}")),
    }
}

fn bool_word(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

fn parse_bool(value: &str, name: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("{name} is true or false")),
    }
}

fn parse_u32(value: &str) -> Option<u32> {
    value.trim().parse().ok()
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
