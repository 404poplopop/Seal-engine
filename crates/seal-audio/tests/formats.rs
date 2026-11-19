use std::path::PathBuf;

use seal_audio::{stream, AudioError, Sound};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

// Every fixture holds the same signal: 0.5 s of a 440 Hz sine at amplitude 0.5, 8000 Hz mono,
// generated for this repository and written by each format's encoder.
const RATE: u32 = 8_000;
const FRAMES: usize = 4_000;

fn expected(i: usize) -> f32 {
    0.5 * (std::f32::consts::TAU * 440.0 * i as f32 / RATE as f32).sin()
}

fn load(name: &str) -> Sound {
    let path = fixture(name);
    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.len() < 20_480, "{name} fixture is too large");
    Sound::from_bytes(&bytes, Some(&path)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// Zero crossings per second over the steady middle of the signal; 440 Hz gives 880.
fn crossings_per_second(samples: &[f32]) -> f32 {
    let mid = &samples[samples.len() / 4..samples.len() * 3 / 4];
    let n = mid.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
    n as f32 * RATE as f32 / mid.len() as f32
}

fn rms(samples: &[f32]) -> f32 {
    let mid = &samples[samples.len() / 4..samples.len() * 3 / 4];
    (mid.iter().map(|s| s * s).sum::<f32>() / mid.len() as f32).sqrt()
}

#[test]
fn lossless_fixtures_match_the_sine_sample_for_sample() {
    for name in ["tone.wav", "tone.flac"] {
        let sound = load(name);
        assert_eq!(
            (sound.sample_rate, sound.channels, sound.frames()),
            (RATE, 1, FRAMES),
            "{name}"
        );
        for (i, s) in sound.samples.iter().enumerate() {
            assert!((s - expected(i)).abs() < 1e-4, "{name} sample {i}: {s} vs {}", expected(i));
        }
    }
}

#[test]
fn lossy_fixtures_keep_rate_length_pitch_and_level() {
    for name in ["tone.ogg", "tone.mp3"] {
        let sound = load(name);
        assert_eq!((sound.sample_rate, sound.channels), (RATE, 1), "{name}");
        // Encoders may pad the start or end by up to a few frames of silence.
        assert!(
            (FRAMES..FRAMES + 2_400).contains(&sound.frames()),
            "{name} frames {}",
            sound.frames()
        );
        let cps = crossings_per_second(&sound.samples);
        assert!((cps - 880.0).abs() < 25.0, "{name} crossings per second {cps}");
        let level = rms(&sound.samples);
        assert!((level - 0.3536).abs() < 0.06, "{name} level {level}");
    }
}

#[test]
fn chunked_decode_matches_one_shot_for_every_fixture() {
    for name in ["tone.wav", "tone.flac", "tone.mp3", "tone.ogg", "stereo.flac", "stereo.mp3"] {
        let path = fixture(name);
        let bytes = std::fs::read(&path).unwrap();
        let full = Sound::from_bytes(&bytes, Some(&path)).unwrap();
        let chunked = stream::decode_file_chunked(&path, 13).unwrap();
        assert_eq!(chunked, full.samples, "{name}");
    }
}

#[test]
fn damaged_inputs_return_errors() {
    let wav = fixture("tone.wav");
    let mut bad_wav = std::fs::read(&wav).unwrap();
    bad_wav.truncate(bad_wav.len() / 2);
    assert!(Sound::from_bytes(&bad_wav, Some(&wav)).is_err());

    let mut bad_mp3 = std::fs::read(fixture("tone.mp3")).unwrap();
    bad_mp3.truncate(40);
    assert!(Sound::from_bytes(&bad_mp3, Some(&PathBuf::from("x.mp3"))).is_err());

    assert!(matches!(
        Sound::from_bytes(b"not audio", Some(&PathBuf::from("x.wav"))),
        Err(AudioError::BadWav(_))
    ));
}

#[test]
fn streaming_uses_bounded_buffers_for_long_ogg() {
    let path = fixture("tone.ogg");
    let mut player = seal_audio::MusicPlayer::new(22_050);
    player.play(&path, false, 0.0, 0.0).unwrap();
    let mut out = vec![0.0; 22_050 * 2];
    player.render(&mut out, &[]);
    assert!(player.buffered_bytes() <= seal_audio::MUSIC_BUFFER_LIMIT);
    assert!(player.buffered_bytes() < std::fs::metadata(&path).unwrap().len() as usize);
}

#[test]
fn stereo_files_keep_both_channels_and_their_length() {
    // 0.25 s: left 440 Hz at 0.5, right 660 Hz at 0.25.
    for name in ["stereo.flac", "stereo.mp3"] {
        let sound = load(name);
        assert_eq!((sound.sample_rate, sound.channels), (RATE, 2), "{name}");
        assert!(
            (2_000..2_000 + 2_400).contains(&sound.frames()),
            "{name} frames {}",
            sound.frames()
        );
        let left: Vec<f32> = sound.samples.iter().step_by(2).copied().collect();
        let right: Vec<f32> = sound.samples.iter().skip(1).step_by(2).copied().collect();
        assert!((crossings_per_second(&left) - 880.0).abs() < 30.0, "{name} left");
        assert!((crossings_per_second(&right) - 1_320.0).abs() < 40.0, "{name} right");
        assert!(rms(&left) > rms(&right) * 1.5, "{name} levels");
        let reader = seal_audio::stream::FileReader::open(&fixture(name)).unwrap();
        assert_eq!(reader.frames() as usize, sound.frames(), "{name} streamed length");
    }
}
