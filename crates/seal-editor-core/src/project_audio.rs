//! Project audio bus volumes and output device settings.

use seal_audio::{backend, Mixer};

use crate::settings::SettingsStore;

pub const MASTER_VOLUME: &str = "audio.master_volume";
pub const MUSIC_VOLUME: &str = "audio.music_volume";
pub const EFFECTS_VOLUME: &str = "audio.effects_volume";
pub const OUTPUT_SAMPLE_RATE: &str = "audio.output_sample_rate";
pub const OUTPUT_BUFFER: &str = "audio.output_buffer";
pub const OUTPUT_BACKEND: &str = "audio.output_backend";

/// Names offered for the output backend dropdown.
pub fn backend_choices() -> &'static [&'static str] {
    seal_audio::backend::BACKENDS
}

/// Label for an output backend id. `web` is shown as Web Audio.
pub fn backend_label(id: &str) -> &'static str {
    seal_audio::backend::backend_label(id)
}

pub const MUSIC_BUS: &str = "music";
pub const EFFECTS_BUS: &str = "effects";

const PLATFORMS: &[&str] = &["linux", "windows", "macos", "web"];

pub type AudioOutputSettings = seal_audio::backend::OutputSettings;

fn percent(value: Option<&str>, default: f32) -> f32 {
    value.and_then(|text| text.parse::<f32>().ok()).unwrap_or(default).clamp(0.0, 100.0) / 100.0
}

fn platform_key(base: &str, platform: &str) -> String {
    format!("audio.{platform}.{base}")
}

pub fn parsed_output_sample_rate(store: &SettingsStore) -> Option<u32> {
    effective_output(store).sample_rate
}

pub fn effective_output(store: &SettingsStore) -> AudioOutputSettings {
    let platform = backend::build_platform();
    let global_rate = store.get(OUTPUT_SAMPLE_RATE).unwrap_or("");
    let global_buffer = store.get(OUTPUT_BUFFER).unwrap_or("");
    let global_backend = store.get(OUTPUT_BACKEND).unwrap_or("");
    let plat_rate = store.get(&platform_key("output_sample_rate", platform)).unwrap_or("");
    let plat_buffer = store.get(&platform_key("output_buffer", platform)).unwrap_or("");
    let plat_backend = store.get(&platform_key("output_backend", platform)).unwrap_or("");
    AudioOutputSettings {
        sample_rate: backend::effective_u32(platform, global_rate, plat_rate)
            .filter(|rate| (8_000..=192_000).contains(rate)),
        buffer_frames: backend::effective_u32(platform, global_buffer, plat_buffer)
            .filter(|n| (32..=4096).contains(n)),
        backend: backend::effective_backend(platform, global_backend, plat_backend),
    }
}

pub fn is_mix_setting(id: &str) -> bool {
    if matches!(id, MASTER_VOLUME | MUSIC_VOLUME | EFFECTS_VOLUME) {
        return true;
    }
  if id == OUTPUT_SAMPLE_RATE || id == OUTPUT_BUFFER || id == OUTPUT_BACKEND {
        return true;
    }
    for platform in PLATFORMS {
        if id == platform_key("output_sample_rate", platform)
            || id == platform_key("output_buffer", platform)
            || id == platform_key("output_backend", platform)
        {
            return true;
        }
    }
    false
}

fn set_gain(mixer: &mut Mixer, bus: seal_audio::BusId, gain: f32) {
    if (mixer.bus_gain(bus) - gain).abs() > 0.000_1 {
        mixer.set_bus_gain(bus, gain);
    }
}

/// Ensure music and effects buses exist and set their gains from project settings.
pub fn apply_to_mixer(mixer: &mut Mixer, store: &SettingsStore) {
    let master = percent(store.get(MASTER_VOLUME), 100.0);
    let music = percent(store.get(MUSIC_VOLUME), 100.0);
    let effects = percent(store.get(EFFECTS_VOLUME), 100.0);
    set_gain(mixer, mixer.master(), master);
    let music_bus = mixer.bus(MUSIC_BUS).unwrap_or_else(|_| mixer.add_bus(MUSIC_BUS));
    let effects_bus = mixer.bus(EFFECTS_BUS).unwrap_or_else(|_| mixer.add_bus(EFFECTS_BUS));
    set_gain(mixer, music_bus, music);
    set_gain(mixer, effects_bus, effects);
    if let Some(rate) = parsed_output_sample_rate(store) {
        if mixer.sample_rate != rate {
            mixer.sample_rate = rate;
            mixer.ramp_frames = (rate / 200).max(1);
            mixer.music.set_output_rate(rate);
        }
    }
}

pub fn bus_gains(store: &SettingsStore) -> (f32, f32, f32) {
    (
        percent(store.get(MASTER_VOLUME), 100.0),
        percent(store.get(MUSIC_VOLUME), 100.0),
        percent(store.get(EFFECTS_VOLUME), 100.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::SettingsStore;
    use seal_audio::Mixer;

    #[test]
    fn bus_volume_is_applied_to_the_mixer() {
        let mut store = SettingsStore::builtin();
        store.set_project(MASTER_VOLUME, "50").unwrap();
        store.set_project(MUSIC_VOLUME, "25").unwrap();
        store.set_project(EFFECTS_VOLUME, "75").unwrap();
        let mut mixer = Mixer::new(44_100);
        apply_to_mixer(&mut mixer, &store);
        let (master, music, effects) = bus_gains(&store);
        assert!((master - 0.5).abs() < 0.01);
        assert!((music - 0.25).abs() < 0.01);
        assert!((effects - 0.75).abs() < 0.01);
        assert!((mixer.bus_gain(mixer.master()) - 0.5).abs() < 0.01);
        assert!((mixer.bus_gain(mixer.bus(MUSIC_BUS).unwrap()) - 0.25).abs() < 0.01);
        assert!((mixer.bus_gain(mixer.bus(EFFECTS_BUS).unwrap()) - 0.75).abs() < 0.01);
        store.set_project(OUTPUT_SAMPLE_RATE, "48000").unwrap();
        apply_to_mixer(&mut mixer, &store);
        assert_eq!(mixer.sample_rate, 48_000);
        assert!(store.set_project(OUTPUT_SAMPLE_RATE, "oops").is_err());
        assert_eq!(parsed_output_sample_rate(&store), Some(48_000));
        apply_to_mixer(&mut mixer, &store);
        assert_eq!(mixer.sample_rate, 48_000);
    }

    #[test]
    fn platform_output_settings_round_trip() {
        let mut store = SettingsStore::builtin();
        store.set_project(OUTPUT_BACKEND, "pulse").unwrap();
        store.set_project("audio.linux.output_backend", "alsa").unwrap();
        store.set_project(OUTPUT_BUFFER, "256").unwrap();
        store.set_project("audio.linux.output_buffer", "512").unwrap();
        let out = effective_output(&store);
        if backend::build_platform() == "linux" {
            assert_eq!(out.backend, "alsa");
            assert_eq!(out.buffer_frames, Some(512));
        }
        store.set_project("audio.linux.output_sample_rate", "32000").unwrap();
        if backend::build_platform() == "linux" {
            assert_eq!(effective_output(&store).sample_rate, Some(32_000));
        }
    }
}
