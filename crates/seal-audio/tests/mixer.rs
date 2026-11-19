use seal_audio::*;

fn ones(frames: usize) -> Sound {
    Sound { sample_rate: 48_000, channels: 1, samples: vec![0.5; frames] }
}

#[test]
fn centred_voices_play_at_equal_power() {
    let mut m = Mixer::new(48_000);
    let s = m.add_sound(ones(100));
    m.play(s, PlayParams::default()).unwrap();
    let mut out = vec![0.0; 20];
    m.mix(&mut out);
    let expected = 0.5 * std::f32::consts::FRAC_1_SQRT_2;
    assert!(out.iter().all(|v| (v - expected).abs() < 1e-6), "{out:?}");
}

#[test]
fn panning_hard_left_silences_the_right() {
    let mut m = Mixer::new(48_000);
    let s = m.add_sound(ones(100));
    m.play(s, PlayParams { pan: -1.0, ..Default::default() }).unwrap();
    let mut out = vec![0.0; 20];
    m.mix(&mut out);
    for frame in out.chunks(2) {
        assert!((frame[0] - 0.5).abs() < 1e-6 && frame[1].abs() < 1e-6, "{frame:?}");
    }
}

#[test]
fn one_shots_end_and_loops_wrap() {
    let mut m = Mixer::new(48_000);
    let s = m.add_sound(ones(10));
    let once = m.play(s, PlayParams::default()).unwrap();
    let looped = m.play(s, PlayParams { looping: true, ..Default::default() }).unwrap();
    let mut out = vec![0.0; 2 * 25];
    m.mix(&mut out);
    assert!(!m.is_playing(once));
    assert!(m.is_playing(looped));
    assert_eq!(m.playing_count(), 1);
    // After frame 10 only the loop is left: half the level.
    assert!((out[2 * 20] - out[0] / 2.0).abs() < 1e-6);
}

#[test]
fn double_speed_halves_the_length() {
    let mut m = Mixer::new(48_000);
    let s = m.add_sound(ones(100));
    let v = m.play(s, PlayParams { speed: 2.0, ..Default::default() }).unwrap();
    let mut out = vec![0.0; 2 * 49];
    m.mix(&mut out);
    assert!(m.is_playing(v));
    let mut out = vec![0.0; 2 * 2];
    m.mix(&mut out);
    assert!(!m.is_playing(v));
    // A 24 kHz sound at normal speed also plays at its own pitch.
    let slow = m.add_sound(Sound { sample_rate: 24_000, channels: 1, samples: vec![0.5; 10] });
    let v = m.play(slow, PlayParams::default()).unwrap();
    let mut out = vec![0.0; 2 * 19];
    m.mix(&mut out);
    assert!(m.is_playing(v));
}

#[test]
fn stopping_fades_without_a_click() {
    let mut m = Mixer::new(48_000);
    let s = m.add_sound(Sound::tone(48_000, 440.0, 1.0, 0.8));
    let v = m.play(s, PlayParams::default()).unwrap();
    let mut out = vec![0.0; 2 * 256];
    m.mix(&mut out);
    m.stop(v, 0.01);
    let mut tail = vec![0.0; 2 * 1024];
    m.mix(&mut tail);
    assert!(!m.is_playing(v));
    let max_jump = out
        .chunks(2)
        .chain(tail.chunks(2))
        .map(|f| f[0])
        .collect::<Vec<_>>()
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0f32, f32::max);
    // A 440 Hz tone at 0.8 changes by at most about 0.033 per sample; a click would jump far more.
    assert!(max_jump < 0.05, "jump {max_jump}");
    // Stale handles are ignored.
    m.set_gain(v, 1.0);
    m.stop(v, 0.0);
}

#[test]
fn buses_scale_and_mute_their_voices() {
    let mut m = Mixer::new(48_000);
    m.ramp_frames = 0;
    let music = m.add_bus("music");
    assert_eq!(m.bus("music"), Ok(music));
    assert!(m.bus("nope").is_err());
    let s = m.add_sound(ones(1000));
    m.play(s, PlayParams { pan: -1.0, bus: music, ..Default::default() }).unwrap();
    m.set_bus_gain(music, 0.5);
    let mut out = vec![0.0; 4];
    m.mix(&mut out);
    assert!((out[0] - 0.25).abs() < 1e-6);
    m.set_bus_muted(music, true);
    m.mix(&mut out);
    assert_eq!(out[0], 0.0);
    m.set_bus_muted(music, false);
    let master = m.master();
    m.set_bus_gain(master, 0.5);
    m.mix(&mut out);
    assert!((out[0] - 0.125).abs() < 1e-6);
}

#[test]
fn peak_normalize_lifts_the_loudest_sample_to_full_scale() {
    let mut sound = Sound { sample_rate: 8_000, channels: 1, samples: vec![0.25, -0.5, 0.1] };
    sound.peak_normalize();
    assert!((sound.samples[1].abs() - 1.0).abs() < 1e-5);
    assert!((sound.samples[0] - 0.5).abs() < 1e-5);
    let mut silent = Sound { sample_rate: 8_000, channels: 1, samples: vec![0.0, 0.0] };
    silent.peak_normalize();
    assert_eq!(silent.samples, vec![0.0, 0.0]);
}

#[test]
fn wav_files_round_trip() {
    let tone = Sound::tone(22_050, 330.0, 0.1, 0.5);
    let bytes = tone.to_wav16();
    let back = Sound::from_wav(&bytes).unwrap();
    assert_eq!((back.sample_rate, back.channels, back.frames()), (22_050, 1, tone.frames()));
    assert!(back.samples.iter().zip(&tone.samples).all(|(a, b)| (a - b).abs() < 1.0 / 16_000.0));
    assert!(matches!(Sound::from_wav(b"nope"), Err(AudioError::BadWav(_))));
    let mut truncated = bytes.clone();
    truncated.truncate(50);
    assert!(Sound::from_wav(&truncated).is_err());
}

#[test]
fn the_same_calls_give_the_same_samples_everywhere() {
    let mut m = Mixer::new(48_000);
    let sfx = m.add_bus("sfx");
    let a = m.add_sound(Sound::tone(48_000, 261.6, 0.5, 0.6));
    let b = m.add_sound(Sound::tone(44_100, 392.0, 0.3, 0.4));
    let v1 = m.play(a, PlayParams { pan: -0.3, looping: true, ..Default::default() }).unwrap();
    let v2 =
        m.play(b, PlayParams { pan: 0.7, speed: 1.37, bus: sfx, ..Default::default() }).unwrap();
    let mut all = Vec::new();
    for block in 0..40 {
        if block == 10 {
            m.set_pan(v1, 0.9);
            m.set_bus_gain(sfx, 0.3);
        }
        if block == 25 {
            m.stop(v2, 0.05);
            m.set_speed(v1, 0.75);
        }
        let mut out = vec![0.0; 2 * 512];
        m.mix(&mut out);
        all.extend(out);
    }
    let hash = all.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, s| {
        (h ^ s.to_bits() as u64).wrapping_mul(0x0100_0000_01b3)
    });
    assert_eq!(hash, GOLDEN, "sample hash {hash:#x}");
}

const GOLDEN: u64 = 0x566b_5f92_1460_b73e;
