//! Ring, conversion and web settings behaviour.

use seal_audio::frames::{FrameConverter, MixerFeed};
use seal_audio::ring::StereoRing;
use seal_audio::web::{load_sound_bytes, WebOutputConfig};
use seal_audio::{Mixer, PlayParams, Sound};

#[test]
fn rendered_tone_reaches_the_ring() {
    let mut mixer = Mixer::new(16_000);
    let tone = mixer.add_sound(Sound::tone(16_000, 440.0, 0.1, 0.5));
    mixer.play(tone, PlayParams::default()).unwrap();
    let mut feed = MixerFeed::new(16_000, 16_000);
    let frames = feed.render(&mut mixer, 256);
    let ring = StereoRing::with_capacity(128);
    let wrote = ring.push(&frames[..128]);
    assert_eq!(wrote, 128);
    let mut out = vec![[0.0; 2]; 128];
    assert_eq!(ring.pop(&mut out), 128);
    let peak = out.iter().map(|frame| frame[0].abs()).fold(0.0f32, f32::max);
    assert!(peak > 0.05, "peak {peak}");
    assert!(out.iter().all(|frame| frame[0].is_finite() && frame[1].is_finite()));
}

#[test]
fn slower_device_rate_still_fills_the_requested_block() {
    let mut mixer = Mixer::new(16_000);
    let tone = mixer.add_sound(Sound::tone(16_000, 220.0, 0.2, 0.3));
    mixer.play(tone, PlayParams::default()).unwrap();
    let mut feed = MixerFeed::new(16_000, 8_000);
    let first = feed.render(&mut mixer, 64);
    let second = feed.render(&mut mixer, 64);
    assert_eq!(first.len(), 64);
    assert_eq!(second.len(), 64);
    assert!(first.iter().chain(&second).any(|frame| frame[0].abs() > 0.01));
}

#[test]
fn web_output_config_follows_project_fields() {
    let config = WebOutputConfig::resolve(Some(48_000), Some(1_024));
    assert_eq!(config.sample_rate, 48_000);
    assert_eq!(config.buffer_frames, 1_024);
    assert_eq!(config.callback_frames, 1_024);
    let fallback = WebOutputConfig::resolve(Some(0), Some(9_999));
    assert_eq!(fallback.sample_rate, 48_000);
    assert_eq!(fallback.buffer_frames, 512);
}

#[test]
fn damaged_sound_file_is_rejected_and_a_wav_plays() {
    assert!(load_sound_bytes(&[]).is_err());
    let truncated = b"RIFF\x10\x00\x00\x00WAVEfmt ";
    assert!(load_sound_bytes(truncated).is_err());
    let wav = Sound::tone(8_000, 880.0, 0.02, 0.25).to_wav16();
    let sound = load_sound_bytes(&wav).unwrap();
    let mut mixer = Mixer::new(8_000);
    let id = mixer.add_sound(sound);
    mixer.play(id, PlayParams::default()).unwrap();
    let mut block = vec![0.0; 64];
    mixer.mix(&mut block);
    assert!(block.iter().any(|sample| sample.abs() > 0.01));
}

#[test]
fn partial_output_tail_is_left_alone() {
    let mut converter = FrameConverter::new(8_000, 8_000, 2);
    let mut out = [9.0; 5];
    let mut cursor = 0;
    let source = [[0.2, 0.3], [0.4, 0.5]];
    converter.fill(&mut out, || {
        let frame = source.get(cursor).copied();
        cursor += 1;
        frame
    });
    assert_eq!(&out[..4], &[0.2, 0.3, 0.4, 0.5]);
    assert_eq!(out[4], 9.0);
}
