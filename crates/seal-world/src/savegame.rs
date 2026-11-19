//! Save games: a running game written to a named slot and put back later.
//!
//! Each slot is one readable file, `saved/games/<slot>.sealsave` in the
//! project folder. It holds every object as it is in the scene file, their
//! velocities, the scores, the game's state, the player each index controls
//! and the values scripts kept with `set_saved`.
//!
//! Two versions travel with a save. `seal_save` is the file format; a save
//! from a newer format is refused. `game_version` is the project's
//! `save_version` setting (in `project.seal`, 1 when unset) at the time of
//! saving; a game raises it when its saves change shape. Older saves still
//! load, and scripts read the version they came from as `saved("save_version")`.
//! Saves from a newer game version than the project are refused.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use seal_sealtext::{parse, SealValue};

use crate::Entity;

/// The folder save slots live in, inside the project folder.
pub const SAVE_DIR: &str = "saved/games";
pub const SAVE_EXT: &str = "sealsave";
/// The save file format this build writes and the newest it reads.
pub const SAVE_FORMAT: u32 = 1;
/// The kept value scripts read to learn which game version a loaded save came from.
pub const VERSION_KEY: &str = "save_version";

/// One saved game.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SaveGame {
    pub slot: String,
    pub format: u32,
    pub game_version: u32,
    /// Seconds since 1970 (UTC) when it was saved.
    pub saved_at: u64,
    /// Seconds of play when it was saved.
    pub play_time: f32,
    pub entities: Vec<Entity>,
    pub scores: BTreeMap<String, i64>,
    pub state: String,
    pub possessed: BTreeMap<i64, String>,
    pub values: BTreeMap<String, String>,
    /// Scene file that was open, project-relative. Empty in saves written
    /// before this field existed.
    pub scene: String,
}

/// What a slot list shows for one save.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotInfo {
    pub slot: String,
    pub saved_at: u64,
    pub play_time: f32,
    pub game_version: u32,
    /// Why the save cannot load, when it cannot.
    pub problem: Option<String>,
}

/// A slot name as given, trimmed; letters, digits, spaces, `-` and `_`, up to 64 characters.
pub fn slot_name(slot: &str) -> Result<String, String> {
    let s = slot.trim();
    if s.is_empty() {
        return Err("a save slot needs a name".into());
    }
    if s.chars().count() > 64 {
        return Err(format!("the save slot name {s:?} is longer than 64 characters"));
    }
    if !s.chars().all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_') {
        return Err(format!(
            "the save slot name {s:?} can only have letters, digits, spaces, - and _"
        ));
    }
    Ok(s.to_string())
}

/// The file of `slot` in project folder `dir`.
pub fn slot_path(dir: &Path, slot: &str) -> Result<PathBuf, String> {
    Ok(dir.join(SAVE_DIR).join(format!("{}.{SAVE_EXT}", slot_name(slot)?)))
}

fn settings_value(dir: &Path) -> Option<SealValue> {
    let text = std::fs::read_to_string(dir.join("project.seal")).ok()?;
    let value = parse(&text).ok()?;
    value.get("settings").cloned()
}

/// A yes/no project setting. Absent is false.
pub fn project_flag(dir: &Path, key: &str) -> bool {
    match settings_value(dir).as_ref().and_then(|s| s.get(key)) {
        Some(SealValue::Bool(on)) => *on,
        Some(SealValue::String(s)) => matches!(s.as_str(), "yes" | "true" | "on"),
        _ => false,
    }
}

/// A text project setting. Absent is empty, which is not an error.
pub fn project_text(dir: &Path, key: &str) -> String {
    match settings_value(dir).as_ref().and_then(|s| s.get(key)) {
        Some(SealValue::String(s)) => s.clone(),
        _ => String::new(),
    }
}

/// Turn an encrypted envelope back into save text. Plain text is unchanged.
pub fn decode(dir: &Path, text: &str) -> Result<String, String> {
    if seal_graph::crypto::is_envelope(text) {
        seal_graph::crypto::open(text, &project_text(dir, "encryption_key"))
    } else {
        Ok(text.to_string())
    }
}

/// The project's `save_version` setting from `project.seal`; 1 when unset.
pub fn project_save_version(dir: &Path) -> u32 {
    let Ok(text) = std::fs::read_to_string(dir.join("project.seal")) else { return 1 };
    let Ok(value) = parse(&text) else { return 1 };
    match value.get("settings").and_then(|s| s.get(VERSION_KEY)) {
        Some(SealValue::Number(n)) => n.parse::<u32>().ok().filter(|v| *v >= 1).unwrap_or(1),
        _ => 1,
    }
}

fn num(v: f64) -> SealValue {
    SealValue::Number(if v.is_finite() { format!("{v}") } else { "0".into() })
}

fn str_map<V>(m: &BTreeMap<String, V>, f: impl Fn(&V) -> SealValue) -> SealValue {
    SealValue::Map(m.iter().map(|(k, v)| (k.clone(), f(v))).collect::<Vec<_>>().into())
}

fn get_u(value: &SealValue, key: &str) -> Option<u64> {
    match value.get(key) {
        Some(SealValue::Number(n)) => n.parse::<f64>().ok().filter(|v| *v >= 0.0).map(|v| v as u64),
        _ => None,
    }
}

impl SaveGame {
    pub fn to_text(&self) -> String {
        let velocities: Vec<(String, SealValue)> = self
            .entities
            .iter()
            .filter(|e| e.velocity != [0.0; 3])
            .map(|e| {
                (
                    e.id.clone(),
                    SealValue::Array(e.velocity.iter().map(|v| num(*v as f64)).collect()),
                )
            })
            .collect();
        let possessed: Vec<(String, SealValue)> = self
            .possessed
            .iter()
            .map(|(i, id)| (i.to_string(), SealValue::String(id.clone())))
            .collect();
        SealValue::Map(
            vec![
                ("seal_save".into(), num(self.format as f64)),
                ("game_version".into(), num(self.game_version as f64)),
                ("slot".into(), SealValue::String(self.slot.clone())),
                ("saved_at".into(), num(self.saved_at as f64)),
                ("play_time".into(), num(self.play_time as f64)),
                ("state".into(), SealValue::String(self.state.clone())),
                ("scores".into(), str_map(&self.scores, |v| num(*v as f64))),
                ("values".into(), str_map(&self.values, |v| SealValue::String(v.clone()))),
                ("possessed".into(), SealValue::Map(possessed.into())),
                ("velocities".into(), SealValue::Map(velocities.into())),
                (
                    "entities".into(),
                    SealValue::Array(self.entities.iter().map(crate::entity_value).collect()),
                ),
                ("scene".into(), SealValue::String(self.scene.clone())),
            ]
            .into(),
        )
        .to_text()
    }

    /// Read a save; `current` is the project's save version now.
    pub fn from_text(text: &str, current: u32) -> Result<Self, String> {
        let value = parse(text).map_err(|e| format!("the save file is damaged ({e})"))?;
        let format = get_u(&value, "seal_save")
            .ok_or("this is not a save file (it has no seal_save version)")?
            as u32;
        if format > SAVE_FORMAT {
            return Err(format!(
                "this save was made by a newer Seal (save format {format}; this one reads up to {SAVE_FORMAT})"
            ));
        }
        let game_version = get_u(&value, "game_version").unwrap_or(1).max(1) as u32;
        if game_version > current {
            return Err(format!(
                "this save is from a newer version of the game (save version {game_version}; the game is at {current})"
            ));
        }
        let text_of = |key: &str| match value.get(key) {
            Some(SealValue::String(s)) => s.clone(),
            _ => String::new(),
        };
        let pairs = |key: &str| -> Vec<(String, SealValue)> {
            value.get(key).and_then(|m| m.as_map()).map(|m| m.to_vec()).unwrap_or_default()
        };
        let mut entities = Vec::new();
        if let Some(SealValue::Array(items)) = value.get("entities") {
            for item in items {
                entities.push(crate::entity_from(item)?);
            }
        }
        if entities.is_empty() {
            return Err("the save holds no objects".into());
        }
        for (id, v) in pairs("velocities") {
            if let (Some(e), SealValue::Array(xs)) = (entities.iter_mut().find(|e| e.id == id), v) {
                for (k, x) in xs.iter().take(3).enumerate() {
                    if let SealValue::Number(n) = x {
                        e.velocity[k] = n.parse().unwrap_or(0.0);
                    }
                }
            }
        }
        let scores = pairs("scores")
            .into_iter()
            .filter_map(|(k, v)| match v {
                SealValue::Number(n) => n.parse::<f64>().ok().map(|x| (k, x as i64)),
                _ => None,
            })
            .collect();
        let values = pairs("values")
            .into_iter()
            .filter_map(|(k, v)| match v {
                SealValue::String(s) => Some((k, s)),
                _ => None,
            })
            .collect();
        let possessed = pairs("possessed")
            .into_iter()
            .filter_map(|(k, v)| match v {
                SealValue::String(s) => k.parse::<i64>().ok().map(|i| (i, s)),
                _ => None,
            })
            .collect();
        let play_time = match value.get("play_time") {
            Some(SealValue::Number(n)) => n.parse().unwrap_or(0.0),
            _ => 0.0,
        };
        Ok(SaveGame {
            slot: text_of("slot"),
            format,
            game_version,
            saved_at: get_u(&value, "saved_at").unwrap_or(0),
            play_time,
            entities,
            scores,
            state: text_of("state"),
            possessed,
            values,
            scene: text_of("scene"),
        })
    }
}

/// Seconds since 1970 now.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Write `save` to its slot in project folder `dir`, replacing the old one
/// only once the new file is complete.
pub fn write(dir: &Path, save: &SaveGame) -> Result<PathBuf, String> {
    let path = slot_path(dir, &save.slot)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("cannot make {SAVE_DIR}: {e}"))?;
    }
    let tmp = path.with_extension(format!("{SAVE_EXT}.tmp"));
    let plain = save.to_text();
    let body = if project_flag(dir, "save_encrypt") {
        seal_graph::crypto::seal(&plain, &project_text(dir, "encryption_key"))?
    } else {
        plain
    };
    std::fs::write(&tmp, body)
        .and_then(|_| std::fs::rename(&tmp, &path))
        .map_err(|e| format!("cannot write the save {}: {e}", save.slot))?;
    Ok(path)
}

/// Read the save in `slot`.
pub fn read(dir: &Path, slot: &str) -> Result<SaveGame, String> {
    let path = slot_path(dir, slot)?;
    let text = std::fs::read_to_string(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => format!("there is no save in the slot {}", slot.trim()),
        _ => format!("cannot read the save {}: {e}", slot.trim()),
    })?;
    let text =
        decode(dir, &text).map_err(|e| format!("cannot load the save {}: {e}", slot.trim()))?;
    let mut save = SaveGame::from_text(&text, project_save_version(dir))
        .map_err(|e| format!("cannot load the save {}: {e}", slot.trim()))?;
    save.slot = slot_name(slot)?;
    Ok(save)
}

/// Remove the save in `slot`. Returns whether there was one.
pub fn delete(dir: &Path, slot: &str) -> Result<bool, String> {
    let path = slot_path(dir, slot)?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("cannot delete the save {}: {e}", slot.trim())),
    }
}

/// Every save slot in project folder `dir`, newest first.
pub fn list(dir: &Path) -> Vec<SlotInfo> {
    let current = project_save_version(dir);
    let Ok(read_dir) = std::fs::read_dir(dir.join(SAVE_DIR)) else { return Vec::new() };
    let mut out: Vec<SlotInfo> = read_dir
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some(SAVE_EXT) {
                return None;
            }
            let slot = path.file_stem()?.to_str()?.to_string();
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            Some(match decode(dir, &text).and_then(|text| SaveGame::from_text(&text, current)) {
                Ok(s) => SlotInfo {
                    slot,
                    saved_at: s.saved_at,
                    play_time: s.play_time,
                    game_version: s.game_version,
                    problem: None,
                },
                Err(e) => SlotInfo {
                    slot,
                    saved_at: 0,
                    play_time: 0.0,
                    game_version: 0,
                    problem: Some(e),
                },
            })
        })
        .collect();
    out.sort_by(|a, b| b.saved_at.cmp(&a.saved_at).then(a.slot.cmp(&b.slot)));
    out
}
