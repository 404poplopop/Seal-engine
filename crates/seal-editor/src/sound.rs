//! Plays the sounds object scripts start during play through the default
//! sound device; installed as the editor shell's sound sink. The device opens on the first sound; files load once.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use seal_app::audio_out::AudioOut;
use seal_audio::listener::spatial_mix;
use seal_audio::{BusId, Mixer, PlayParams, Sound, SoundId, VoiceId};
use seal_editor_core::asset_editor::AudioRequest;
use seal_editor_core::{
    effective_output,
    apply_to_mixer, bus_gains, parsed_output_sample_rate, sound_load_type, ScriptMusic as MusicCue,
    ScriptSound as SoundCue, SettingsStore, EFFECTS_BUS,
};
use seal_editor_core::{active_listener, World};

struct LoadedSound {
    image: Option<std::time::SystemTime>,
    sidecar: Option<std::time::SystemTime>,
    id: Option<SoundId>,
}

struct EditorAudio {
    mixer: Arc<Mutex<Mixer>>,
    _out: AudioOut,
    /// Loaded files by full path; `None` for files that failed (reported once).
    loaded: HashMap<PathBuf, LoadedSound>,
    /// The Asset Editor preview that is playing, if any.
    preview: Option<VoiceId>,
    output: seal_editor_core::AudioOutputSettings,
}

thread_local! {
    static AUDIO: RefCell<Option<Result<EditorAudio, ()>>> = const { RefCell::new(None) };
    static APPLIED: RefCell<Option<(u32, u32, u32, u32)>> = const { RefCell::new(None) };
    static SETTINGS: RefCell<Option<seal_editor_core::AudioOutputSettings>> = const { RefCell::new(None) };
}

fn mix_stamp(store: &SettingsStore) -> (u32, u32, u32, u32) {
    let (master, music, effects) = bus_gains(store);
    let bits = |value: f32| (value * 1000.0).round() as u32;
    (bits(master), bits(music), bits(effects), parsed_output_sample_rate(store).unwrap_or(0))
}

/// Copy project bus volumes and the output sample rate onto the live mixer.
pub fn apply_project_audio(store: &SettingsStore) {
    let next = mix_stamp(store);
    let same = APPLIED.with(|slot| *slot.borrow() == Some(next));
    SETTINGS.with(|slot| *slot.borrow_mut() = Some(effective_output(store)));
    if same {
        return;
    }
    let mut applied = false;
    with_audio(|audio| {
        let mut mixer = audio.mixer.lock().unwrap_or_else(|e| e.into_inner());
        apply_to_mixer(&mut mixer, store);
        audio.output = effective_output(store);
        applied = true;
    });
    if applied {
        APPLIED.with(|slot| *slot.borrow_mut() = Some(next));
    }
}

fn effects_bus(mixer: &seal_audio::Mixer) -> BusId {
    mixer.bus(EFFECTS_BUS).unwrap_or_else(|_| mixer.master())
}

/// Where a script's sound path points: inside the project, with `.wav`
/// added when it has no extension.
pub fn sound_file(project: Option<&Path>, sound: &str) -> PathBuf {
    let rel = Path::new(sound.trim_start_matches(['/', '\\']));
    let rel = if rel.extension().is_none() { rel.with_extension("wav") } else { rel.to_path_buf() };
    project.map_or(rel.clone(), |p| p.join(&rel))
}

fn decode_file(path: &Path) -> Result<Sound, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    seal_audio::compress::decode_exported(&bytes)
        .or_else(|_| Sound::from_bytes(&bytes, Some(path)))
        .map_err(|e| e.to_string())
        .map(|s| seal_editor_core::import_presets::finish_sound(path, s))
}

fn with_audio(f: impl FnOnce(&mut EditorAudio)) {
    AUDIO.with(|slot| {
        let mut slot = slot.borrow_mut();
        let settings = SETTINGS.with(|s| s.borrow().clone()).unwrap_or_default();
        let audio = slot.get_or_insert_with(|| {
            let mixer = Arc::new(Mutex::new(Mixer::new(seal_app::SAMPLE_RATE)));
            match AudioOut::start(mixer.clone(), &settings) {
                Ok(out) => Ok(EditorAudio {
                    mixer,
                    _out: out,
                    loaded: HashMap::new(),
                    preview: None,
                    output: settings,
                }),
                Err(e) => {
                    seal_log::log(seal_log::Level::Warn, "sound", format!("no sound output: {e}"));
                    Err(())
                }
            }
        });
        if let Ok(audio) = audio {
            f(audio);
        }
    });
}

/// Play or stop the Asset Editor's sound preview. The file is read fresh each
/// time so edits to it are heard; only one preview plays at once.
pub fn preview(request: &AudioRequest) {
    with_audio(|audio| {
        let mut mixer = audio.mixer.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(voice) = audio.preview.take() {
            mixer.stop(voice, 0.02);
        }
        if let AudioRequest::PlaySet { paths, .. } = request {
            for (path, volume) in paths {
                if let Ok(s) = decode_file(path) {
                    let id = mixer.add_sound(s);
                    let params = PlayParams {
                        gain: volume.clamp(0.0, 1.0),
                        bus: effects_bus(&mixer),
                        ..PlayParams::default()
                    };
                    let _ = mixer.play(id, params);
                }
            }
        } else if let AudioRequest::Play { path, volume } = request {
            match decode_file(path) {
                Ok(s) => {
                    let id = mixer.add_sound(s);
                    let params = PlayParams {
                        gain: volume.clamp(0.0, 1.0),
                        bus: effects_bus(&mixer),
                        ..PlayParams::default()
                    };
                    audio.preview = mixer.play(id, params).ok();
                }
                Err(e) => seal_log::log(
                    seal_log::Level::Warn,
                    "sound",
                    format!("{}: {e}", path.display()),
                ),
            }
        }
    });
}

/// Apply music-bus commands. Files stream from disk; they are not decoded up front.
pub fn music(project: Option<&Path>, cues: &[MusicCue]) {
    with_audio(|audio| {
        let mut mixer = audio.mixer.lock().unwrap_or_else(|e| e.into_inner());
        for cue in cues {
            match cue {
                MusicCue::Play { file, looping, fade, loop_start } => {
                    let path = sound_file(project, file);
                    if let Err(e) = mixer.music.play(&path, *looping, f64::from(*loop_start), *fade)
                    {
                        seal_log::log(
                            seal_log::Level::Warn,
                            "sound",
                            format!("{}: {e}", path.display()),
                        );
                    }
                }
                MusicCue::Stop { fade } => mixer.music.stop(*fade),
                MusicCue::Volume { volume } => mixer.music.set_volume(*volume),
                MusicCue::Pause => mixer.music.pause(),
                MusicCue::Resume => mixer.music.resume(),
            }
        }
    });
}

pub fn play(project: Option<&Path>, world: &World, cues: &[SoundCue]) {
    let listener = active_listener(world);
    with_audio(|audio| {
        for cue in cues {
            let path = sound_file(project, &cue.sound);
            let stream = sound_load_type(&path) == seal_audio::compress::SoundLoadType::Stream;
            let image = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            let sidecar = std::fs::metadata(format!("{}.import.seal", path.display()))
                .and_then(|m| m.modified())
                .ok();
            let stale = audio.loaded.get(&path).is_none_or(|loaded| {
                loaded.image != image || loaded.sidecar != sidecar
            });
            let (gain, pan) = cue.at.and_then(|at| {
                listener.map(|l| {
                    let pose = seal_audio::listener::ListenerPose {
                        position: l.pose.position,
                        forward: l.pose.forward,
                    };
                    let tuning = seal_audio::listener::ListenerTuning {
                        max_distance: l.tuning.max_distance,
                        rolloff: l.tuning.rolloff,
                    };
                    spatial_mix(&pose, &tuning, at, cue.volume)
                })
            })
                .map(|m| (m.gain, m.pan))
                .unwrap_or((cue.volume, 0.0));
            if stream {
                match decode_file(&path) {
                    Ok(s) => {
                        let mut mixer = audio.mixer.lock().unwrap_or_else(|e| e.into_inner());
                        let id = mixer.add_sound(s);
                        let params = PlayParams {
                            gain,
                            pan,
                            speed: cue.pitch,
                            bus: effects_bus(&mixer),
                            ..PlayParams::default()
                        };
                        let _ = mixer.play(id, params);
                    }
                    Err(e) => seal_log::log(
                        seal_log::Level::Warn,
                        "sound",
                        format!("{}: {e}", path.display()),
                    ),
                }
                continue;
            }
            if stale {
                let sound = decode_file(&path);
                let id = match sound {
                    Ok(s) => {
                        Some(audio.mixer.lock().unwrap_or_else(|e| e.into_inner()).add_sound(s))
                    }
                    Err(e) => {
                        seal_log::log(
                            seal_log::Level::Warn,
                            "sound",
                            format!("{}: {e}", path.display()),
                        );
                        None
                    }
                };
                audio.loaded.insert(path.clone(), LoadedSound { image, sidecar, id });
            }
            let id = audio.loaded.get(&path).and_then(|loaded| loaded.id);
            if let Some(id) = id {
                let mut mixer = audio.mixer.lock().unwrap_or_else(|e| e.into_inner());
                let params = PlayParams {
                    gain,
                    pan,
                    speed: cue.pitch,
                    bus: effects_bus(&mixer),
                    ..PlayParams::default()
                };
                let _ = mixer.play(id, params);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::sound_file;
    use std::path::Path;

    #[test]
    fn sound_paths_resolve_inside_the_project() {
        let p = Path::new("proj");
        assert_eq!(sound_file(Some(p), "Audio/Jump"), p.join("Audio/Jump.wav"));
        assert_eq!(sound_file(Some(p), "/Audio/Hit.wav"), p.join("Audio/Hit.wav"));
        assert_eq!(sound_file(None, "a.ogg"), Path::new("a.ogg"));
    }
}