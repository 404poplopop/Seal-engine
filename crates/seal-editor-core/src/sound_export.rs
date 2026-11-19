//! Apply sound compression when packaging a project.

use std::path::Path;

use seal_audio::compress::{decode_exported, encode_for_export, SoundCompression};

use crate::import_presets::{load_file_state, ImportKind};

const SOUND_EXT: &[&str] = &["wav", "ogg", "flac", "mp3"];

pub fn is_sound_rel(rel: &str) -> bool {
    let lower = rel.to_ascii_lowercase();
    SOUND_EXT.iter().any(|ext| lower.ends_with(&format!(".{ext}")))
}

/// Bytes written into a package for one sound file.
pub fn package_bytes(project: &Path, rel: &str, raw: &[u8]) -> Result<Vec<u8>, String> {
    if !is_sound_rel(rel) {
        return Ok(raw.to_vec());
    }
    let state = load_file_state(project, rel, ImportKind::Sound);
    let settings = match state.settings {
        crate::import_presets::ImportSettings::Sound(s) => s,
        _ => return Ok(raw.to_vec()),
    };
    let mode = SoundCompression::parse(&settings.compression);
    // An OGG Vorbis source is already Vorbis: ship it unchanged rather than re-encoding.
    if mode == SoundCompression::Vorbis && raw.starts_with(b"OggS") {
        return Ok(raw.to_vec());
    }
    let sound = decode_exported(raw)
        .or_else(|_| seal_audio::Sound::from_bytes(raw, Some(&project.join(rel))))
        .map_err(|e| e.to_string())?;
    let bytes = encode_for_export(
        &sound,
        mode,
        settings.vorbis_quality,
    );
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use seal_audio::Sound;
    use crate::import_presets::{save_file_state, FileImportState, ImportSettings, SoundSettings};
    use std::fs;

    #[test]
    fn export_compression_shrinks_adpcm_payload() {
        let dir = std::env::temp_dir().join(format!("seal-sound-export-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let rel = "sfx/ping.wav";
        let tone = Sound::tone(44_100, 300.0, 0.2, 0.5);
        let raw = tone.to_wav16();
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, &raw).unwrap();
        let state = FileImportState {
            settings: ImportSettings::Sound(SoundSettings {
                normalize: false,
                compression: "adpcm".into(),
                vorbis_quality: 5,
                load_type: "memory".into(),
            }),
            customized: true,
            platform_overrides: Default::default(),
        };
        save_file_state(&dir, rel, &state).unwrap();
        let packed = package_bytes(&dir, rel, &raw).unwrap();
        let _ = fs::remove_dir_all(&dir);
        assert!(packed.len() < raw.len());
        let back = decode_exported(&packed).unwrap();
        assert_eq!(back.sample_rate, tone.sample_rate);
    }
}