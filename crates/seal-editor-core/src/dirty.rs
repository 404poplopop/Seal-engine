/// What the window title and status bar should show for unsaved work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirtyMarks {
    pub title: String,
    pub status: DirtyStatus,
}

/// `Unsaved` is the star. `Saved` is the small check after a write lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirtyStatus {
    Unsaved,
    Saved,
    Idle,
}

pub fn dirty_marks(
    project: &str,
    scene_name: &str,
    any_dirty: bool,
    save_confirmed: bool,
) -> DirtyMarks {
    let scene = if any_dirty { format!("{scene_name}*") } else { scene_name.to_string() };
    let title = if project.is_empty() {
        format!("{scene} - Seal")
    } else {
        format!("{scene} - {project} - Seal")
    };
    DirtyMarks {
        title,
        status: if any_dirty {
            DirtyStatus::Unsaved
        } else if save_confirmed {
            DirtyStatus::Saved
        } else {
            DirtyStatus::Idle
        },
    }
}

pub fn tab_label(title: &str, dirty: bool, _mark: &str) -> String {
    if dirty {
        format!("{title}*")
    } else {
        title.to_string()
    }
}

pub fn panel_is_dirty<'a>(
    panel: &str,
    scene_dirty: bool,
    settings_dirty: bool,
    docs: impl IntoIterator<Item = (&'a str, bool)>,
) -> bool {
    let kind = crate::panel::panel_base(panel);
    match kind {
        "viewport" => scene_dirty,
        "settings" => settings_dirty,
        other => docs.into_iter().any(|(id, dirty)| id == other && dirty),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scene_edit_stars_the_title_until_the_save_check() {
        let dirty = dirty_marks("Demo", "level_01.scene", true, false);
        assert_eq!(dirty.title, "level_01.scene* - Demo - Seal");
        assert_eq!(dirty.status, DirtyStatus::Unsaved);
        let saved = dirty_marks("Demo", "level_01.scene", false, true);
        assert_eq!(saved.title, "level_01.scene - Demo - Seal");
        assert!(!saved.title.contains('*'));
        assert_eq!(dirty_marks("", "Untitled", false, false).title, "Untitled - Seal");
        assert_eq!(saved.status, DirtyStatus::Saved);
        assert_eq!(tab_label("Viewport", true, "•"), "Viewport*");
        assert!(panel_is_dirty("viewport", true, false, []));
        assert!(panel_is_dirty("scratch", false, false, [("scratch", true)]));
        assert!(panel_is_dirty("settings", false, true, []));
    }
}
