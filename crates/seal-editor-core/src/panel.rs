use std::collections::BTreeSet;

use crate::error::CoreError;

/// Stable separator between a panel type and its instance suffix (`outliner#2`).
pub const PANEL_INSTANCE_SEP: char = '#';

/// Panel id without an instance suffix. Unknown ids such as `panel.future` pass through.
pub fn panel_base(id: &str) -> &str {
    id.split_once(PANEL_INSTANCE_SEP).map_or(id, |(base, _)| base)
}

pub fn allows_multiple_instances(base: &str) -> bool {
    matches!(base, "outliner" | "inspector" | "viewport")
}

/// Default camera and view for `viewport`, `viewport#2`, … up to four tabs.
pub fn viewport_tab_seed(panel_id: &str) -> usize {
    if panel_id == "viewport" {
        return 0;
    }
    panel_id
        .strip_prefix("viewport#")
        .and_then(|suffix| suffix.parse::<usize>().ok())
        .unwrap_or(1)
        .saturating_sub(1)
        .min(3)
}

/// Picks the lowest unused suffix for another tab of the same panel type.
/// Visible tab text for a panel id. `outliner#2` becomes `Outliner 2`.
pub fn panel_tab_title(base_title: &str, id: &str) -> String {
    let kind = panel_base(id);
    if id == kind {
        return base_title.to_string();
    }
    let prefix = format!("{kind}{PANEL_INSTANCE_SEP}");
    let Some(suffix) = id.strip_prefix(&prefix) else {
        return base_title.to_string();
    };
    let Ok(index) = suffix.parse::<usize>() else {
        return base_title.to_string();
    };
    format!("{base_title} {index}")
}

pub fn next_instance_id<'a>(existing: impl IntoIterator<Item = &'a str>, base: &str) -> String {
    let mut taken = BTreeSet::new();
    taken.insert(1_usize);
    for id in existing {
        if panel_base(id) != base {
            continue;
        }
        if id == base {
            taken.insert(1);
            continue;
        }
        let Some(suffix) = id.strip_prefix(&format!("{base}{PANEL_INSTANCE_SEP}")) else {
            continue;
        };
        if let Ok(index) = suffix.parse::<usize>() {
            taken.insert(index);
        }
    }
    let next = taken.into_iter().max().unwrap_or(1).saturating_add(1);
    format!("{base}{PANEL_INSTANCE_SEP}{next}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockPreference {
    Left,
    Center,
    Right,
    Bottom,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanelDesc {
    pub id: String,
    pub title_key: String,
    pub default_dock: DockPreference,
}

#[derive(Clone, Debug, Default)]
pub struct PanelRegistry {
    panels: Vec<PanelDesc>,
}

impl PanelRegistry {
    pub fn register(&mut self, desc: PanelDesc) -> Result<(), CoreError> {
        if self.panels.iter().any(|panel| panel.id == desc.id) {
            return Err(CoreError::new(format!("panel '{}' is already registered", desc.id)));
        }
        if desc.id.is_empty() {
            return Err(CoreError::new("panel id must not be empty"));
        }
        self.panels.push(desc);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&PanelDesc> {
        let base = panel_base(id);
        self.panels.iter().find(|panel| panel.id == base)
    }

    pub fn iter(&self) -> impl Iterator<Item = &PanelDesc> {
        self.panels.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_title_shows_a_spaced_instance_index() {
        assert_eq!(panel_tab_title("Outliner", "outliner"), "Outliner");
        assert_eq!(panel_tab_title("Outliner", "outliner#2"), "Outliner 2");
    }

    #[test]
    fn viewport_allows_four_tabs() {
        assert!(allows_multiple_instances("viewport"));
        assert_eq!(viewport_tab_seed("viewport"), 0);
        assert_eq!(viewport_tab_seed("viewport#4"), 3);
    }

    #[test]
    fn instance_suffixes_keep_the_base_type() {
        assert_eq!(panel_base("outliner#2"), "outliner");
        assert_eq!(panel_base("panel.future"), "panel.future");
        assert_eq!(next_instance_id(["outliner", "outliner#2"], "outliner"), "outliner#3");
    }

    #[test]
    fn registry_rejects_a_duplicate_id() {
        let mut registry = PanelRegistry::default();
        let desc = PanelDesc {
            id: "console".into(),
            title_key: "panel-console-title".into(),
            default_dock: DockPreference::Bottom,
        };
        registry.register(desc.clone()).unwrap();
        assert!(registry.register(desc).is_err());
        assert_eq!(registry.iter().count(), 1);
    }
}
