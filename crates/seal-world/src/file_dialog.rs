//! Open and save file dialogs for a running game.
//!
//! Tests install a mock so no window opens. Otherwise the system dialog is
//! used when this machine has one, and an empty path means the player
//! cancelled or no dialog could be shown.

use std::cell::RefCell;
use std::path::PathBuf;

struct Mock {
    open: String,
    save: String,
    log: Vec<String>,
}

thread_local! {
    static MOCK: RefCell<Option<Mock>> = const { RefCell::new(None) };
}

/// Make the next dialogs return these paths and record what was asked.
pub fn set_mock(open: &str, save: &str) {
    MOCK.with(|slot| {
        *slot.borrow_mut() = Some(Mock { open: open.into(), save: save.into(), log: Vec::new() });
    });
}

pub fn clear_mock() {
    MOCK.with(|slot| *slot.borrow_mut() = None);
}

pub fn mock_log() -> Vec<String> {
    MOCK.with(|slot| slot.borrow().as_ref().map(|m| m.log.clone()).unwrap_or_default())
}

/// The path the player picked, or empty when they cancelled or no dialog opened.
pub fn open_file(title: &str, filter: &str) -> String {
    if let Some(path) = mocked("open", title, filter) {
        return path;
    }
    native_open(title, filter).map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
}

pub fn save_file(title: &str, name: &str) -> String {
    if let Some(path) = mocked("save", title, name) {
        return path;
    }
    native_save(title, name).map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
}

fn mocked(kind: &str, title: &str, extra: &str) -> Option<String> {
    MOCK.with(|slot| {
        let mut slot = slot.borrow_mut();
        let mock = slot.as_mut()?;
        mock.log.push(format!("{kind}\t{title}\t{extra}"));
        Some(if kind == "open" { mock.open.clone() } else { mock.save.clone() })
    })
}

fn desktop() -> bool {
    #[cfg(any(windows, target_os = "macos"))]
    {
        true
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some()
    }
}

fn native_open(title: &str, filter: &str) -> Option<PathBuf> {
    if !desktop() {
        return None;
    }
    let mut dialog = rfd::FileDialog::new().set_title(title);
    let filter = filter.trim().trim_start_matches('.');
    if !filter.is_empty() {
        dialog = dialog.add_filter(filter, &[filter]);
    }
    dialog.pick_file()
}

fn native_save(title: &str, name: &str) -> Option<PathBuf> {
    if !desktop() {
        return None;
    }
    let mut dialog = rfd::FileDialog::new().set_title(title);
    if !name.trim().is_empty() {
        dialog = dialog.set_file_name(name);
    }
    dialog.save_file()
}
