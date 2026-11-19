//! Browser output settings and sound-file loading for the web backend.
//!
//! Project Settings stores `audio.web.output_sample_rate` and
//! `audio.web.output_buffer`. Empty values use 48000 Hz and 512 frames.
//! The script callback only accepts 256, 512, 1024, 2048 or 4096 frames,
//! so the requested buffer is snapped to the nearest of those.

use crate::{AudioError, Sound};

pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;
pub const DEFAULT_BUFFER_FRAMES: u32 = 512;

const CALLBACK_SIZES: [u32; 5] = [256, 512, 1024, 2048, 4096];

/// Sample rate, buffer and the callback size a browser output will open with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebOutputConfig {
    pub sample_rate: u32,
    pub buffer_frames: u32,
    pub callback_frames: u32,
}

impl WebOutputConfig {
    /// Honour a project sample rate and buffer. Out-of-range numbers use the defaults.
    pub fn resolve(sample_rate: Option<u32>, buffer_frames: Option<u32>) -> Self {
        let sample_rate = match sample_rate {
            Some(rate) if (8_000..=192_000).contains(&rate) => rate,
            _ => DEFAULT_SAMPLE_RATE,
        };
        let buffer_frames = match buffer_frames {
            Some(frames) if (32..=4096).contains(&frames) => frames,
            _ => DEFAULT_BUFFER_FRAMES,
        };
        Self { sample_rate, buffer_frames, callback_frames: callback_frames(buffer_frames) }
    }
}

/// Nearest script-callback size for a project buffer.
pub fn callback_frames(buffer: u32) -> u32 {
    let buffer = if (32..=4096).contains(&buffer) { buffer } else { DEFAULT_BUFFER_FRAMES };
    CALLBACK_SIZES
        .into_iter()
        .min_by_key(|size| (size.abs_diff(buffer), *size))
        .unwrap_or(DEFAULT_BUFFER_FRAMES)
}

/// Decode a sound file for browser playback. An empty or truncated file is an error.
pub fn load_sound_bytes(bytes: &[u8]) -> Result<Sound, AudioError> {
    if bytes.is_empty() {
        return Err(AudioError::BadWav("empty file".into()));
    }
    Sound::from_bytes(bytes, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_settings_keep_rate_and_buffer() {
        let config = WebOutputConfig::resolve(Some(44_100), Some(256));
        assert_eq!(config.sample_rate, 44_100);
        assert_eq!(config.buffer_frames, 256);
        assert_eq!(config.callback_frames, 256);

        let defaults = WebOutputConfig::resolve(None, None);
        assert_eq!(defaults.sample_rate, 48_000);
        assert_eq!(defaults.buffer_frames, 512);
        assert_eq!(defaults.callback_frames, 512);

        let repaired = WebOutputConfig::resolve(Some(7), Some(1));
        assert_eq!(repaired.sample_rate, 48_000);
        assert_eq!(repaired.buffer_frames, 512);
        assert_eq!(callback_frames(3_000), 2_048);
        assert_eq!(callback_frames(4_096), 4_096);
        assert_eq!(callback_frames(100), 256);
    }

    #[test]
    fn truncated_and_empty_files_are_rejected() {
        assert!(load_sound_bytes(&[]).is_err());
        assert!(load_sound_bytes(b"RIFF").is_err());
        assert!(load_sound_bytes(b"not a sound file").is_err());
        let tone = Sound::tone(8_000, 440.0, 0.01, 0.2).to_wav16();
        let sound = load_sound_bytes(&tone).expect("wav");
        assert_eq!(sound.sample_rate, 8_000);
        assert!(!sound.samples.is_empty());
    }
}
