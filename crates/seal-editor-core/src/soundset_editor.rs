//! Commands for editing `.soundset` sound container files in the Asset Editor.

use std::path::Path;

use seal_world::sound_container::{
    ContainerMode, SoundContainerDef, EXTENSION as SOUNDSET_EXTENSION,
};

use crate::asset_editor::{
    asset_kind, open_asset, show_asset, AssetKind, AudioRequest, ASSET_VOLUME_DRAFT,
};
use crate::command::CommandArgs;
use crate::error::CoreError;
use crate::session::EditorSession;

pub fn is_soundset_path(rel: &str) -> bool {
    SoundContainerDef::is_soundset_path(rel)
}

fn project_root(session: &EditorSession) -> Result<std::path::PathBuf, CoreError> {
    session
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .ok_or_else(|| CoreError::new("open a project first"))
}

fn checked_rel(rel: &str) -> Result<String, CoreError> {
    let rel = rel.trim().replace('\\', "/");
    if rel.is_empty() || rel.starts_with('/') || rel.split('/').any(|p| p == "..") {
        return Err(CoreError::new(format!("'{rel}' is not a path inside the project")));
    }
    Ok(rel)
}

fn open_soundset(
    session: &EditorSession,
) -> Result<(String, std::path::PathBuf, SoundContainerDef), CoreError> {
    let (rel, full) = open_asset(session)
        .ok_or_else(|| CoreError::new("no sound container is open in the Asset Editor"))?;
    if asset_kind(&rel) != AssetKind::SoundSet {
        return Err(CoreError::new("the open file is not a sound container"));
    }
    let text = std::fs::read_to_string(&full)?;
    let def = SoundContainerDef::parse(&text).map_err(|e| CoreError::new(format!("{rel}: {e}")))?;
    Ok((rel, full, def))
}

fn save_soundset(full: &Path, def: &SoundContainerDef) -> Result<(), CoreError> {
    crate::persist::atomic_write(full, def.to_text().as_bytes())?;
    Ok(())
}

pub fn set_soundset_value(
    def: &mut SoundContainerDef,
    key: &str,
    value: &str,
) -> Result<(), CoreError> {
    match key {
        "mode" => {
            def.mode = ContainerMode::parse(value).ok_or_else(|| {
                CoreError::new("mode must be random, shuffle, sequence or layered")
            })?;
        }
        "seed" => {
            let v: f32 =
                value.trim().parse().map_err(|_| CoreError::new("seed must be a number"))?;
            def.seed = v.max(1.0) as u32;
        }
        "volume" => def.volume = parse_f32(value, "volume")?,
        "pitch_min" => def.pitch_min = parse_f32(value, "pitch")?,
        "pitch_max" => def.pitch_max = parse_f32(value, "pitch")?,
        "volume_var_min" => def.volume_var_min = parse_f32(value, "volume range")?.max(0.0),
        "volume_var_max" => def.volume_var_max = parse_f32(value, "volume range")?.max(0.0),
        "max_voices" => {
            let v: f32 = parse_f32(value, "max voices")?;
            def.max_voices = v.max(0.0) as u32;
        }
        "cooldown" => def.cooldown = parse_f32(value, "cooldown")?.max(0.0),
        key if key.starts_with("entry.") => {
            let rest = key.strip_prefix("entry.").unwrap_or("");
            let (index_s, field) =
                rest.split_once('.').ok_or_else(|| CoreError::new("choose an entry field"))?;
            let index: usize = index_s.parse().map_err(|_| CoreError::new("choose an entry"))?;
            let entry = def
                .entries
                .get_mut(index)
                .ok_or_else(|| CoreError::new("that entry does not exist"))?;
            match field {
                "path" => {
                    if value.trim().is_empty() {
                        return Err(CoreError::new("choose a sound file"));
                    }
                    entry.path = value.trim().replace('\\', "/");
                }
                "weight" => entry.weight = parse_f32(value, "weight")?.max(0.0),
                "volume" => entry.volume = parse_f32(value, "volume")?.max(0.0),
                _ => return Err(CoreError::new(format!("unknown entry field {field}"))),
            }
        }
        _ => return Err(CoreError::new(format!("unknown sound container setting {key}"))),
    }
    Ok(())
}

fn parse_f32(value: &str, name: &str) -> Result<f32, CoreError> {
    value.trim().parse().map_err(|_| CoreError::new(format!("{name} must be a number"))).and_then(
        |v: f32| {
            if v.is_finite() {
                Ok(v)
            } else {
                Err(CoreError::new(format!("{name} must be a number")))
            }
        },
    )
}

/// `asset.soundset_new path= [unique=yes]`: create a sound container and open it.
pub(crate) fn cmd_soundset_new(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let mut rel = checked_rel(args.get("path").unwrap_or("sounds/container"))?;
    if !is_soundset_path(&rel) {
        rel.push('.');
        rel.push_str(SOUNDSET_EXTENSION);
    }
    let root = project_root(session)?;
    let mut full = root.join(&rel);
    if full.exists() {
        if args.get("unique") != Some("yes") {
            return Err(CoreError::new(format!("{rel} already exists")));
        }
        let stem = rel.trim_end_matches(&format!(".{SOUNDSET_EXTENSION}")).to_string();
        let mut n = 2;
        while full.exists() {
            rel = format!("{stem}_{n}.{SOUNDSET_EXTENSION}");
            full = root.join(&rel);
            n += 1;
        }
    }
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)?;
    }
    save_soundset(&full, &SoundContainerDef::default())?;
    show_asset(session, &rel)
}

/// `asset.soundset_set key= value=`: change one setting and save.
pub(crate) fn cmd_soundset_set(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let key = args.get("key").ok_or_else(|| CoreError::new("choose a setting"))?;
    let value = args.get("value").unwrap_or_default();
    let (_, full, mut def) = open_soundset(session)?;
    set_soundset_value(&mut def, key, value)?;
    save_soundset(&full, &def)
}

/// `asset.soundset_entry_add [path=]`: append a sound entry.
pub(crate) fn cmd_soundset_entry_add(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let (_, full, mut def) = open_soundset(session)?;
    let path = args.get("path").unwrap_or("sounds/sound.wav").trim().replace('\\', "/");
    if path.is_empty() {
        return Err(CoreError::new("choose a sound file"));
    }
    def.entries.push(seal_world::sound_container::SoundEntry { path, weight: 1.0, volume: 1.0 });
    save_soundset(&full, &def)
}

/// `asset.soundset_entry_remove index=`
pub(crate) fn cmd_soundset_entry_remove(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let (_, full, mut def) = open_soundset(session)?;
    let index: usize = args
        .get("index")
        .and_then(|i| i.trim().parse().ok())
        .ok_or_else(|| CoreError::new("choose an entry"))?;
    if index >= def.entries.len() {
        return Err(CoreError::new("that entry does not exist"));
    }
    def.entries.remove(index);
    save_soundset(&full, &def)
}

/// `asset.soundset_entry_move index= direction=up|down`
pub(crate) fn cmd_soundset_entry_move(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let (_, full, mut def) = open_soundset(session)?;
    let index: usize = args
        .get("index")
        .and_then(|i| i.trim().parse().ok())
        .ok_or_else(|| CoreError::new("choose an entry"))?;
    if index >= def.entries.len() {
        return Err(CoreError::new("that entry does not exist"));
    }
    match args.get("direction").unwrap_or_default() {
        "up" if index > 0 => def.entries.swap(index, index - 1),
        "down" if index + 1 < def.entries.len() => def.entries.swap(index, index + 1),
        "up" | "down" => {}
        other => return Err(CoreError::new(format!("unknown direction '{other}'"))),
    }
    save_soundset(&full, &def)
}

/// `asset.soundset_play`: audition the container (one pick, or all for layered).
pub(crate) fn cmd_soundset_play(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    let (rel, full, def) = open_soundset(session)?;
    let preview_vol = session
        .draft(ASSET_VOLUME_DRAFT)
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.8)
        .clamp(0.0, 1.0);
    let mut runtime = seal_world::sound_container::SoundContainerRuntime::new(def);
    let picks = runtime.preview(preview_vol, 1.0).map_err(|e| CoreError::new(e.to_string()))?;
    let root = project_root(session)?;
    let dir = full.parent().unwrap_or(root.as_path());
    let mut paths = Vec::new();
    for pick in picks {
        let p = dir.join(&pick.path);
        if !p.is_file() {
            let alt = root.join(&pick.path);
            if alt.is_file() {
                paths.push((alt, pick.volume));
            }
        } else {
            paths.push((p, pick.volume));
        }
    }
    if paths.is_empty() {
        return Err(CoreError::new("no sound files in this container were found"));
    }
    session.audio_request = Some(AudioRequest::PlaySet { paths, rel });
    session.audio_playing = None;
    Ok(())
}
