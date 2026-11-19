//! Save games in the editor: a `.sealsave` file shown in the Asset Editor
//! with what it holds, every slot of the project, and the project's save
//! version (the `save_version` setting in `project.seal`).

use std::path::Path;

use seal_world::savegame;
pub use seal_world::savegame::{SaveGame, SlotInfo, SAVE_DIR, SAVE_EXT, VERSION_KEY};

use crate::asset_editor::{open_asset, show_asset};
use crate::{CommandArgs, CoreError, EditorSession};

fn err(e: impl Into<String>) -> CoreError {
    CoreError::new(e.into())
}

fn root(session: &EditorSession) -> Result<std::path::PathBuf, CoreError> {
    session.project.as_ref().map(|p| p.root.clone()).ok_or_else(|| err("open a project first"))
}

/// What the Asset Editor shows for an open save file.
#[derive(Clone, Debug, PartialEq)]
pub struct SaveView {
    pub slot: String,
    /// The save itself, or why it cannot load.
    pub save: Result<SaveGame, String>,
    pub slots: Vec<SlotInfo>,
    pub project_version: u32,
    /// Whether new saves are sealed with the project key.
    pub encrypt: bool,
    /// The project key. Empty is allowed until something is encrypted.
    pub key: String,
    /// Whether save slots are sent to a cloud provider.
    pub cloud: bool,
    /// `http` or `script`.
    pub provider: String,
    /// HTTP endpoint. Empty when the game uses a script hook.
    pub url: String,
    /// Transfers waiting because the provider was offline.
    pub queued: usize,
}

/// The open save file and the project's slots, when a save is open.
pub fn save_view(session: &EditorSession) -> Option<SaveView> {
    let (rel, full) = open_asset(session)?;
    if !rel.to_ascii_lowercase().ends_with(&format!(".{SAVE_EXT}")) {
        return None;
    }
    let dir = session.project.as_ref()?.root.clone();
    let slot = full.file_stem()?.to_string_lossy().to_string();
    let current = savegame::project_save_version(&dir);
    let save = std::fs::read_to_string(&full)
        .map_err(|e| e.to_string())
        .and_then(|t| savegame::decode(&dir, &t))
        .and_then(|t| SaveGame::from_text(&t, current));
    let provider = savegame::project_text(&dir, "cloud_provider");
    let queued = seal_world::cloud::load(&dir).map(|q| q.jobs.len()).unwrap_or(0);
    Some(SaveView {
        slot,
        save,
        slots: savegame::list(&dir),
        project_version: current,
        encrypt: savegame::project_flag(&dir, "save_encrypt"),
        key: savegame::project_text(&dir, "encryption_key"),
        cloud: savegame::project_flag(&dir, "cloud_save"),
        provider: if provider == "http" { "http".into() } else { "script".into() },
        url: savegame::project_text(&dir, "cloud_url"),
        queued,
    })
}

/// The project-relative path of a slot's file.
pub fn slot_rel(slot: &str) -> String {
    format!("{SAVE_DIR}/{slot}.{SAVE_EXT}")
}

/// A time as `2026-11-17 02:40 UTC`.
pub fn utc_time(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Days since 1970-01-01 to a civil date (Howard Hinnant's method).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02} UTC", rem / 3600, rem % 3600 / 60)
}

/// Seconds of play as `1:05:09` or `4:31`.
pub fn play_time(secs: f32) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// `project.saves`: open the newest save in the Asset Editor.
pub(crate) fn cmd_project_saves(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    let dir = root(session)?;
    let newest = savegame::list(&dir)
        .into_iter()
        .next()
        .ok_or_else(|| err("there are no saved games yet: a save_game node in play makes one"))?;
    show_asset(session, &slot_rel(&newest.slot))
}

/// `asset.save_delete slot=`: remove a save; when it was open, the next one opens.
pub(crate) fn cmd_save_delete(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let dir = root(session)?;
    let slot = args.get("slot").filter(|s| !s.is_empty()).ok_or_else(|| err("missing slot="))?;
    if !savegame::delete(&dir, slot).map_err(err)? {
        return Err(err(format!("there is no save in the slot {slot}")));
    }
    let was_open = open_asset(session).is_none();
    if was_open {
        match savegame::list(&dir).into_iter().next() {
            Some(next) => show_asset(session, &slot_rel(&next.slot))?,
            None => session.set_draft(crate::asset_editor::ASSET_DRAFT, "")?,
        }
    }
    Ok(())
}

/// `asset.save_version value=`: set the project's save version (1 or more).
pub(crate) fn cmd_save_version(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let dir = root(session)?;
    let raw = args.get("value").unwrap_or_default().trim();
    let v: f64 = raw
        .parse()
        .map_err(|_| err(format!("the save version must be a whole number, not {raw:?}")))?;
    if !(1.0..=1_000_000.0).contains(&v) {
        return Err(err("the save version must be 1 or more"));
    }
    set_save_version(&dir, v.round() as u32)
}

/// Write `settings.save_version` into the project's `project.seal`.
pub fn set_save_version(dir: &Path, version: u32) -> Result<(), CoreError> {
    crate::project::set_setting(
        dir,
        VERSION_KEY,
        seal_sealtext::SealValue::Number(version.max(1).to_string()),
    )
}

fn flag(raw: &str) -> Result<bool, CoreError> {
    match raw {
        "true" | "yes" | "on" => Ok(true),
        "false" | "no" | "off" => Ok(false),
        _ => Err(err("expected true or false")),
    }
}

/// `asset.save_encrypt value=`: seal new saves with the project key.
pub(crate) fn cmd_save_encrypt(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let dir = root(session)?;
    let on = flag(args.get("value").unwrap_or("").trim())?;
    crate::project::set_setting(&dir, "save_encrypt", seal_sealtext::SealValue::Bool(on))
}

/// `asset.save_key value=`: the project key. Empty is kept, and is not an error.
pub(crate) fn cmd_save_key(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let dir = root(session)?;
    let value = args.get("value").unwrap_or("");
    crate::project::set_setting(
        &dir,
        "encryption_key",
        seal_sealtext::SealValue::String(value.into()),
    )
}

/// `asset.cloud_save value=`: upload and download save slots.
pub(crate) fn cmd_cloud_save(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let dir = root(session)?;
    let on = flag(args.get("value").unwrap_or("").trim())?;
    crate::project::set_setting(&dir, "cloud_save", seal_sealtext::SealValue::Bool(on))
}

/// `asset.cloud_provider value=`: `http` or `script`.
pub(crate) fn cmd_cloud_provider(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let dir = root(session)?;
    let value = args.get("value").unwrap_or("").trim();
    if value != "http" && value != "script" {
        return Err(err("the cloud provider is http or script"));
    }
    crate::project::set_setting(
        &dir,
        "cloud_provider",
        seal_sealtext::SealValue::String(value.into()),
    )
}

/// `asset.cloud_url value=`: the HTTP endpoint. Empty means a script hook.
pub(crate) fn cmd_cloud_url(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let dir = root(session)?;
    let value = args.get("value").unwrap_or("");
    crate::project::set_setting(&dir, "cloud_url", seal_sealtext::SealValue::String(value.into()))
}
