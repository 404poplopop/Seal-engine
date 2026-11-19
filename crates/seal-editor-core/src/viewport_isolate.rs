//! Hide everything except the current selection; toggle restores prior visibility.

use std::collections::BTreeMap;

use crate::error::CoreError;
use crate::session::EditorSession;

const BACKUP_KEY: &str = "viewport-isolate-backup";

/// Whether isolate mode is active (backup draft present).
pub fn isolate_active(session: &EditorSession) -> bool {
    session.draft(BACKUP_KEY).is_some()
}

/// Turn isolation on or off. When turning on, stores each entity's hidden flag.
pub fn toggle_isolate(session: &mut EditorSession) -> Result<(), CoreError> {
    if session.world.mode != seal_world::PlayMode::Editing {
        return Err(CoreError::new("isolation is for edit mode"));
    }
    if isolate_active(session) {
        let backup = session.draft(BACKUP_KEY).unwrap_or_default();
        for line in backup.split('\n').filter(|l| !l.is_empty()) {
            let Some((id, hidden)) = line.split_once('\t') else { continue };
            if let Some(entity) = session.world.entity_mut(id) {
                entity.hidden = hidden == "1";
            }
        }
        let _ = session.set_draft(BACKUP_KEY, "");
        session.adopt_world();
        return Ok(());
    }
    let mut ids = session.foundation.selection.clone();
    if let Some(id) = &session.models.selection {
        if !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    ids.retain(|id| id != "root");
    if ids.is_empty() {
        return Err(CoreError::new("select something to isolate"));
    }
    let mut backup = String::new();
    for entity in &mut session.world.entities {
        if entity.id == "root" {
            continue;
        }
        backup.push_str(&entity.id);
        backup.push('\t');
        backup.push(if entity.hidden { '1' } else { '0' });
        backup.push('\n');
        entity.hidden = !ids.contains(&entity.id);
    }
    let _ = session.set_draft(BACKUP_KEY, &backup);
    session.adopt_world();
    Ok(())
}

/// Exposed for tests: parse a backup line map.
pub fn parse_backup(text: &str) -> BTreeMap<String, bool> {
    let mut map = BTreeMap::new();
    for line in text.split('\n').filter(|l| !l.is_empty()) {
        let Some((id, flag)) = line.split_once('\t') else { continue };
        map.insert(id.to_string(), flag == "1");
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_parse_round_trip() {
        let text = "hero\t0\ncrate\t1\n";
        let map = parse_backup(text);
        assert_eq!(map.get("hero"), Some(&false));
        assert_eq!(map.get("crate"), Some(&true));
    }
}
