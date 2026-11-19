//! System open, save, and folder dialogs. When this machine cannot show one,
//! the caller falls back to the typed path bar.

use std::path::PathBuf;

pub enum Pick {
    Path(PathBuf),
    Cancelled,
    Unavailable,
}

pub fn pick_folder() -> Pick {
    native(true, "Choose a folder", "", false)
}

pub fn open_file() -> Pick {
    native(false, "Open", "", false)
}

pub fn open_files() -> Pick {
    native(false, "Open", "", true)
}

pub fn save_file(name: &str) -> Pick {
    save("Save", name)
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

fn native(folder: bool, title: &str, _name: &str, many: bool) -> Pick {
    if !desktop() {
        return Pick::Unavailable;
    }
    let dialog = rfd::FileDialog::new().set_title(title);
    let picked = if folder {
        dialog.pick_folder().map(|p| vec![p])
    } else if many {
        dialog.pick_files()
    } else {
        dialog.pick_file().map(|p| vec![p])
    };
    paths(picked)
}

fn save(title: &str, name: &str) -> Pick {
    if !desktop() {
        return Pick::Unavailable;
    }
    let mut dialog = rfd::FileDialog::new().set_title(title);
    if !name.is_empty() {
        dialog = dialog.set_file_name(name);
    }
    paths(dialog.save_file().map(|p| vec![p]))
}

fn paths(picked: Option<Vec<PathBuf>>) -> Pick {
    match picked {
        Some(files) if !files.is_empty() => {
            if files.len() == 1 {
                Pick::Path(files.into_iter().next().unwrap())
            } else {
                let text = files
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect::<Vec<_>>()
                    .join("\n");
                Pick::Path(PathBuf::from(text))
            }
        }
        Some(_) => Pick::Cancelled,
        None if cfg!(any(windows, target_os = "macos")) => Pick::Cancelled,
        None => Pick::Unavailable,
    }
}
