//! Sounds object scripts start during play go to a sink the app installs;
//! without one they are dropped (tests, screenshots).

use std::cell::RefCell;
use std::path::Path;

use seal_editor_core::asset_editor::AudioRequest;
use seal_editor_core::{ScriptMusic, ScriptSound, SettingsStore};

/// Receives the project folder, live scene, and the sounds started this frame.
pub type SoundSink = Box<dyn FnMut(Option<&Path>, &seal_editor_core::World, &[ScriptSound])>;

/// Receives music-bus commands for this frame.
pub type MusicSink = Box<dyn FnMut(Option<&Path>, &[ScriptMusic])>;

/// Receives Asset Editor sound previews (play a file, or stop).
pub type PreviewSink = Box<dyn FnMut(&AudioRequest)>;

/// Applies project bus volumes and sample rate to the live mixer.
pub type MixSink = Box<dyn FnMut(&SettingsStore)>;

thread_local! {
    static SINK: RefCell<Option<SoundSink>> = const { RefCell::new(None) };
    static MUSIC: RefCell<Option<MusicSink>> = const { RefCell::new(None) };
    static PREVIEW: RefCell<Option<PreviewSink>> = const { RefCell::new(None) };
    static MIX: RefCell<Option<MixSink>> = const { RefCell::new(None) };
}

/// Install where Asset Editor sound previews go.
pub fn set_preview_sink(sink: PreviewSink) {
    PREVIEW.with(|s| *s.borrow_mut() = Some(sink));
}

pub(crate) fn preview(request: &AudioRequest) {
    PREVIEW.with(|s| {
        if let Some(sink) = s.borrow_mut().as_mut() {
            sink(request);
        }
    });
}

/// Install where play-mode sounds go. Call on the thread that runs the editor.
pub fn set_sound_sink(sink: SoundSink) {
    SINK.with(|s| *s.borrow_mut() = Some(sink));
}

/// Install where play-mode music goes. Call on the thread that runs the editor.
pub fn set_music_sink(sink: MusicSink) {
    MUSIC.with(|s| *s.borrow_mut() = Some(sink));
}

pub(crate) fn music(project: Option<&Path>, cues: &[ScriptMusic]) {
    MUSIC.with(|s| {
        if let Some(sink) = s.borrow_mut().as_mut() {
            sink(project, cues);
        }
    });
}

/// Install where project audio settings are applied. Call on the thread that runs the editor.
pub fn set_mix_sink(sink: MixSink) {
    MIX.with(|s| *s.borrow_mut() = Some(sink));
}

pub(crate) fn apply_project_audio(store: &SettingsStore) {
    MIX.with(|s| {
        if let Some(sink) = s.borrow_mut().as_mut() {
            sink(store);
        }
    });
}

pub(crate) fn play(project: Option<&Path>, world: &seal_editor_core::World, cues: &[ScriptSound]) {
    SINK.with(|s| {
        if let Some(sink) = s.borrow_mut().as_mut() {
            sink(project, world, cues);
        }
    });
}
