use std::fs;
use std::path::Path;

use seal_log::warn;
use seal_sealtext::{parse, SealValue};

use crate::error::CoreError;
use crate::layout::{restore_window, Layout, MonitorInfo, WindowPlacement};
use crate::persist::{atomic_write, backup_path, move_aside};

pub const SCHEMA_VERSION: u64 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadOrigin {
    Primary,
    Backup,
    Default,
}

pub struct WorkspaceFile {
    raw: SealValue,
    origin: LoadOrigin,
    /// Placement read from disk on load; kept until the caller saves a new one.
    loaded_window: Option<WindowPlacement>,
}

impl WorkspaceFile {
    pub fn fresh() -> Self {
        let mut raw = SealValue::Map((Vec::new()).into());
        raw.insert("schema_version", SealValue::Number(SCHEMA_VERSION.to_string()));
        raw.insert("revision", SealValue::Number("0".into()));
        let file = Self { raw, origin: LoadOrigin::Default, loaded_window: None };
        let mut file = file;
        file.set_layout(&Layout::default());
        file.set_window(&WindowPlacement::default());
        file
    }

    pub fn origin(&self) -> LoadOrigin {
        self.origin
    }

    pub fn revision(&self) -> u64 {
        match self.raw.get("revision") {
            Some(SealValue::Number(lexeme)) => lexeme.parse().unwrap_or(0),
            _ => 0,
        }
    }

    pub fn set_revision(&mut self, revision: u64) {
        self.raw.insert("revision", SealValue::Number(revision.to_string()));
    }

    pub fn layout(&self) -> Layout {
        self.raw.get("layout").and_then(|value| Layout::from_value(value).ok()).unwrap_or_default()
    }

    pub fn set_layout(&mut self, layout: &Layout) {
        self.raw.insert("layout", layout.to_value());
    }

    /// Saved window placement. Load keeps the values from the file; monitor clamping
    /// is applied only through [`Self::restore_window_placement`].
    pub fn window(&self) -> WindowPlacement {
        if let Some(placement) = &self.loaded_window {
            return placement.clone();
        }
        window_from_raw(&self.raw).unwrap_or_default()
    }

    /// Adjusts the saved placement for `monitors` without changing the workspace file.
    /// When `monitors` is empty, returns the stored placement unchanged.
    pub fn restore_window_placement(&self, monitors: &[MonitorInfo]) -> WindowPlacement {
        let stored = self.window();
        if monitors.is_empty() {
            stored
        } else {
            restore_window(stored, monitors)
        }
    }

    pub fn set_window(&mut self, window: &WindowPlacement) {
        self.raw.insert("window", window.to_value());
        self.loaded_window = Some(window.clone());
    }

    pub fn extra(&self, key: &str) -> Option<&SealValue> {
        self.raw.get(key)
    }

    pub fn load(path: &Path) -> Self {
        if path.exists() {
            match read_map(path) {
                Ok(raw) => {
                    let loaded_window = window_from_raw(&raw);
                    return adopt(raw, LoadOrigin::Primary, loaded_window);
                }
                Err(err) => {
                    warn!("state file {} is unreadable ({err})", path.display());
                    if let Err(move_err) = move_aside(path) {
                        warn!("could not move the bad state file aside: {move_err}");
                    }
                }
            }
        }
        if let Ok(backup) = backup_path(path) {
            if backup.exists() {
                match read_map(&backup) {
                    Ok(raw) => {
                        let loaded_window = window_from_raw(&raw);
                        return adopt(raw, LoadOrigin::Backup, loaded_window);
                    }
                    Err(err) => {
                        warn!("backup {} is unreadable ({err})", backup.display());
                        let _ = move_aside(&backup);
                    }
                }
            }
        }
        Self::fresh()
    }

    pub fn encoded(&mut self) -> String {
        let next = self.revision().saturating_add(1);
        self.set_revision(next);
        if self.raw.get("schema_version").is_none() {
            self.raw.insert("schema_version", SealValue::Number(SCHEMA_VERSION.to_string()));
        }
        self.raw.to_text()
    }

    pub fn save(&mut self, path: &Path) -> Result<(), CoreError> {
        let text = self.encoded();
        atomic_write(path, text.as_bytes())
    }
}

fn adopt(
    raw: SealValue,
    origin: LoadOrigin,
    loaded_window: Option<WindowPlacement>,
) -> WorkspaceFile {
    let mut file = WorkspaceFile { raw, origin, loaded_window };
    if stored_schema(&file.raw) < SCHEMA_VERSION {
        file.set_layout(&Layout::default());
        file.raw.insert("schema_version", SealValue::Number(SCHEMA_VERSION.to_string()));
    }
    file
}

fn stored_schema(raw: &SealValue) -> u64 {
    match raw.get("schema_version") {
        Some(SealValue::Number(lexeme)) => lexeme.parse().unwrap_or(0),
        _ => 0,
    }
}

fn window_from_raw(raw: &SealValue) -> Option<WindowPlacement> {
    raw.get("window").and_then(|value| WindowPlacement::from_value(value).ok())
}

fn read_map(path: &Path) -> Result<SealValue, CoreError> {
    let text = fs::read_to_string(path)?;
    let value = parse(&text)?;
    if value.as_map().is_none() {
        return Err(CoreError::new("state file must be a map"));
    }
    Ok(value)
}

/// Directory beside `exe` when `portable.flag` sits next to that executable.
pub fn portable_root_from_exe(exe: &Path) -> Option<std::path::PathBuf> {
    let dir = exe.parent()?;
    if dir.join("portable.flag").is_file() {
        Some(dir.to_path_buf())
    } else {
        None
    }
}

fn portable_root() -> Option<std::path::PathBuf> {
    portable_root_from_exe(&std::env::current_exe().ok()?)
}

#[cfg(test)]
thread_local! {
    /// Lets one test use its own per-user folder without changing the
    /// process environment that tests on other threads read.
    pub(crate) static TEST_USER_DIR: std::cell::RefCell<Option<std::path::PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

/// Directory for editor state that is not tied to one project.
///
/// A portable folder (a `portable.flag` file next to the executable) keeps that
/// state in `user/` beside the executable. There is no installer and no signature.
pub fn user_state_dir() -> std::path::PathBuf {
    #[cfg(test)]
    if let Some(dir) = TEST_USER_DIR.with(|d| d.borrow().clone()) {
        return dir;
    }
    if let Some(root) = portable_root() {
        return root.join("user");
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(dir) = env_dir("APPDATA") {
            return dir.join("Seal").join("editor");
        }
        if let Some(dir) = env_dir("USERPROFILE") {
            return dir.join("AppData").join("Roaming").join("Seal").join("editor");
        }
        if let Some(dir) = env_dir("LOCALAPPDATA") {
            return dir.join("Seal").join("editor");
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = env_dir("HOME") {
            return home.join("Library").join("Application Support").join("Seal").join("editor");
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Some(dir) = env_dir("XDG_DATA_HOME") {
            return dir.join("seal").join("editor");
        }
        if let Some(home) = env_dir("HOME") {
            return home.join(".local").join("share").join("seal").join("editor");
        }
    }
    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")).join("seal-editor")
}

/// Create the per-user state folder and drop leftover save temps from a failed rename.
pub fn prepare_user_state_dir() -> std::path::PathBuf {
    let dir = user_state_dir();
    let _ = fs::create_dir_all(&dir);
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') && name.ends_with(".tmp") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    dir
}

fn env_dir(key: &str) -> Option<std::path::PathBuf> {
    let value = std::env::var(key).ok()?;
    if value.is_empty() {
        None
    } else {
        Some(std::path::PathBuf::from(value))
    }
}

/// Used by tests that want a quiet logger while loading a bad file.
#[cfg(test)]
pub fn silence_for_tests() {
    seal_log::install(seal_log::Logger::new(seal_log::LogMode::Editor));
    seal_log::with_logger(|logger| logger.clear());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Layout;
    use seal_sealtext::SealValue;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("seal-ws-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn portable_flag_keeps_state_beside_the_executable() {
        let dir = scratch("portable");
        let exe = dir.join("seal-editor");
        std::fs::write(&exe, b"").unwrap();
        assert!(portable_root_from_exe(&exe).is_none());
        std::fs::write(dir.join("portable.flag"), b"portable\n").unwrap();
        assert_eq!(portable_root_from_exe(&exe).as_deref(), Some(dir.as_path()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_and_load_roundtrip_preserves_unknown_fields() {
        let dir = scratch("roundtrip");
        let path = dir.join("layout.seal");
        let mut file = WorkspaceFile::fresh();
        file.raw_insert_for_test("future_flag", SealValue::Bool(true));
        let mut layout = Layout::default();
        layout.floating.push(crate::layout::FloatingWindow {
            x: 12.0,
            y: 14.0,
            width: 400.0,
            height: 300.0,
            monitor: None,
            node: crate::layout::DockNode::tabs(["settings"]),
        });
        file.set_layout(&layout);
        file.save(&path).unwrap();
        let loaded = WorkspaceFile::load(&path);
        assert_eq!(loaded.origin(), LoadOrigin::Primary);
        assert_eq!(loaded.layout(), layout);
        assert_eq!(loaded.extra("future_flag"), Some(&SealValue::Bool(true)));
        assert!(loaded.revision() >= 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_primary_falls_back_to_backup_and_is_moved_aside() {
        silence_for_tests();
        let dir = scratch("backup");
        let path = dir.join("layout.seal");
        let mut file = WorkspaceFile::fresh();
        file.set_revision(4);
        file.save(&path).unwrap();
        file.set_revision(5);
        file.save(&path).unwrap();
        fs::write(&path, b"!!! not sealtext").unwrap();
        let loaded = WorkspaceFile::load(&path);
        assert_eq!(loaded.origin(), LoadOrigin::Backup);
        assert!(!path.exists());
        assert!(fs::read_dir(&dir).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".bad-")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_monitor_list_keeps_the_stored_window() {
        let mut file = WorkspaceFile::fresh();
        let stored = crate::layout::WindowPlacement {
            x: 40.0,
            y: 50.0,
            width: 800.0,
            height: 600.0,
            maximized: false,
            monitor: Some("side".into()),
        };
        file.set_window(&stored);
        assert_eq!(file.restore_window_placement(&[]).x, 40.0);
        let monitors = [crate::layout::MonitorInfo {
            name: "main".into(),
            x: 0.0,
            y: 0.0,
            width: 1280.0,
            height: 720.0,
        }];
        let placed = file.restore_window_placement(&monitors);
        assert_eq!(placed.monitor.as_deref(), Some("main"));
        assert_eq!(file.window().x, 40.0);
    }

    impl WorkspaceFile {
        fn raw_insert_for_test(&mut self, key: &str, value: SealValue) {
            self.raw.insert(key, value);
        }
    }
}
