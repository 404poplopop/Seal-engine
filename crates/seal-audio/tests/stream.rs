use std::io::Write;
use std::path::{Path, PathBuf};

use seal_audio::{Mixer, MusicPlayer, Sound, MUSIC_BUFFER_LIMIT};

fn dir(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("seal-music-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
}

fn wav_f32(rate: u32, channels: u16, samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 4) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    let block = channels as u32 * 4;
    out.extend_from_slice(&(rate * block).to_le_bytes());
    out.extend_from_slice(&(block as u16).to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

fn left(out: &[f32]) -> Vec<f32> {
    out.chunks(2).map(|f| f[0]).collect()
}

#[test]
fn chunked_decode_matches_a_full_decode() {
    let root = dir("decode");
    let tone = Sound::tone(8_000, 440.0, 0.05, 0.6);
    let pcm = root.join("tone.wav");
    write(&pcm, &tone.to_wav16());
    let full = Sound::from_wav(&std::fs::read(&pcm).unwrap()).unwrap();
    let chunked = seal_audio::stream::decode_file_chunked(&pcm, 7).unwrap();
    assert_eq!(chunked, full.samples);

    let stereo = Sound {
        sample_rate: 8_000,
        channels: 2,
        samples: vec![0.25, -0.5, 0.0, 1.0, -1.0, 0.125, 0.5, -0.25],
    };
    let wide = root.join("wide.wav");
    write(&wide, &stereo.to_wav16());
    let full = Sound::from_wav(&std::fs::read(&wide).unwrap()).unwrap();
    let chunked = seal_audio::stream::decode_file_chunked(&wide, 3).unwrap();
    assert_eq!(chunked, full.samples);

    let floats: Vec<f32> = (0..20).map(|i| i as f32 * 0.1 - 0.9).collect();
    let exact = root.join("float.wav");
    write(&exact, &wav_f32(8_000, 1, &floats));
    let full = Sound::from_wav(&std::fs::read(&exact).unwrap()).unwrap();
    let chunked = seal_audio::stream::decode_file_chunked(&exact, 1).unwrap();
    assert_eq!(chunked, full.samples);
    assert_eq!(chunked, floats);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_loop_wraps_at_the_loop_start_frame() {
    let root = dir("loop");
    let short = root.join("short.wav");
    write(&short, &wav_f32(8, 1, &[10.0, 11.0, 12.0, 13.0, 14.0]));
    let mut player = MusicPlayer::new(8);
    player.play(&short, true, 2.0 / 8.0, 0.0).unwrap();
    let mut out = vec![0.0; 8 * 2];
    player.render(&mut out, &[]);
    assert_eq!(left(&out), vec![10.0, 11.0, 12.0, 13.0, 14.0, 12.0, 13.0, 14.0]);

    let frames = 10_000u32;
    let samples: Vec<f32> = (0..frames).map(|i| i as f32).collect();
    let long = root.join("long.wav");
    write(&long, &wav_f32(8_000, 1, &samples));
    assert!(std::fs::metadata(&long).unwrap().len() > seal_audio::stream::RING_FRAMES as u64 * 8);
    let mut player = MusicPlayer::new(8_000);
    player.play(&long, true, 100.0 / 8_000.0, 0.0).unwrap();
    let mut out = vec![0.0; (frames as usize + 1) * 2];
    player.render(&mut out, &[]);
    for i in 0..frames as usize {
        assert!((out[i * 2] - i as f32).abs() < 1e-3, "frame {i} is {}", out[i * 2]);
    }
    assert!((out[frames as usize * 2] - 100.0).abs() < 1e-3);
    assert!(player.buffered_bytes() <= MUSIC_BUFFER_LIMIT);
    assert!(player.buffered_bytes() < std::fs::metadata(&long).unwrap().len() as usize);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn crossfade_gains_add_up_to_one() {
    let root = dir("fade");
    let a = root.join("a.wav");
    let b = root.join("b.wav");
    write(&a, &wav_f32(8, 1, &[1.0; 32]));
    write(&b, &wav_f32(8, 1, &[0.5; 32]));
    let mut player = MusicPlayer::new(8);
    assert_eq!(player.bus(), "music");
    player.play(&a, true, 0.0, 0.0).unwrap();
    player.play(&b, true, 0.0, 0.5).unwrap();
    let expect = [1.0, 0.75, 0.5, 0.25];
    for (i, old) in expect.iter().enumerate() {
        let g = player.fade_gains();
        assert!((g[0] + g[1] - 1.0).abs() < 1e-5, "frame {i}: {g:?}");
        assert!((g[0] - old).abs() < 1e-5, "frame {i}: {g:?}");
        let mut out = [0.0; 2];
        player.render(&mut out, &[]);
        let heard = 1.0 * g[0] + 0.5 * g[1];
        assert!((out[0] - heard).abs() < 1e-5 && (out[1] - heard).abs() < 1e-5, "{out:?}");
    }
    let g = player.fade_gains();
    assert!((g.iter().sum::<f32>() - 1.0).abs() < 1e-5, "{g:?}");
    let mut out = [0.0; 2];
    player.render(&mut out, &[]);
    assert!((out[0] - 0.5).abs() < 1e-5);

    player.set_volume(0.5);
    player.play(&a, true, 0.0, 0.0).unwrap();
    player.play(&b, true, 0.0, 0.5).unwrap();
    let g = player.fade_gains();
    assert!((g[0] + g[1] - 1.0).abs() < 1e-5);
    let mut out = [0.0; 2];
    player.render(&mut out, &[]);
    let heard = (1.0 * g[0] + 0.5 * g[1]) * player.volume();
    assert!((out[0] - heard).abs() < 1e-5);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn pause_resume_stop_and_volume() {
    let root = dir("transport");
    let path = root.join("ramp.wav");
    write(&path, &wav_f32(8, 1, &[0.0, 1.0, 2.0, 3.0, 4.0]));
    let mut player = MusicPlayer::new(8);
    player.play(&path, false, 0.0, 0.0).unwrap();
    let mut out = [0.0; 4];
    player.render(&mut out, &[]);
    assert_eq!(left(&out), vec![0.0, 1.0]);
    player.pause();
    assert!(player.is_paused());
    let mut held = [0.0; 4];
    player.render(&mut held, &[]);
    assert_eq!(held, [0.0; 4]);
    player.resume();
    let mut out = [0.0; 2];
    player.render(&mut out, &[]);
    assert!((out[0] - 2.0).abs() < 1e-6);

    let tone = root.join("one.wav");
    write(&tone, &wav_f32(8, 1, &[1.0; 16]));
    let mut player = MusicPlayer::new(8);
    player.play(&tone, true, 0.0, 0.0).unwrap();
    player.set_volume(0.25);
    let mut out = [0.0; 2];
    player.render(&mut out, &[]);
    assert!((out[0] - 0.25).abs() < 1e-6);
    player.set_volume(1.0);
    player.stop(0.5);
    let mut heard = Vec::new();
    for _ in 0..4 {
        let mut out = [0.0; 2];
        player.render(&mut out, &[]);
        heard.push(out[0]);
    }
    assert!(heard.windows(2).all(|w| w[1] < w[0] - 0.1), "fade should fall, got {heard:?}");
    assert!(!player.is_playing());
    let mut out = [0.0; 2];
    player.render(&mut out, &[]);
    assert_eq!(out, [0.0; 2]);
    player.play(&tone, true, 0.0, 0.0).unwrap();
    player.stop(0.0);
    assert!(!player.is_playing());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn memory_stays_at_the_buffer_not_the_file() {
    let root = dir("memory");
    let path = root.join("long.wav");
    let frames = 400_000u32;
    let data_len = frames * 2;
    let mut file = std::fs::File::create(&path).unwrap();
    let mut hdr = Vec::new();
    hdr.extend_from_slice(b"RIFF");
    hdr.extend_from_slice(&(36 + data_len).to_le_bytes());
    hdr.extend_from_slice(b"WAVEfmt ");
    hdr.extend_from_slice(&16u32.to_le_bytes());
    hdr.extend_from_slice(&1u16.to_le_bytes());
    hdr.extend_from_slice(&1u16.to_le_bytes());
    hdr.extend_from_slice(&8_000u32.to_le_bytes());
    hdr.extend_from_slice(&(8_000u32 * 2).to_le_bytes());
    hdr.extend_from_slice(&2u16.to_le_bytes());
    hdr.extend_from_slice(&16u16.to_le_bytes());
    hdr.extend_from_slice(b"data");
    hdr.extend_from_slice(&data_len.to_le_bytes());
    file.write_all(&hdr).unwrap();
    let zeros = vec![0u8; 8192];
    let mut left = data_len as usize;
    while left > 0 {
        let n = left.min(zeros.len());
        file.write_all(&zeros[..n]).unwrap();
        left -= n;
    }
    drop(file);
    let file_len = std::fs::metadata(&path).unwrap().len();
    let mut player = MusicPlayer::new(8_000);
    player.play(&path, true, 0.0, 0.0).unwrap();
    let mut out = vec![0.0; 2 * 2_000];
    player.render(&mut out, &[]);
    assert!(out.iter().all(|s| *s == 0.0));
    let held = player.buffered_bytes();
    assert!(held <= MUSIC_BUFFER_LIMIT, "held {held}");
    assert!(held > 0);
    assert!(file_len > held as u64 * 4, "file {file_len} buffer {held}");
    assert!(player.last_error.is_none());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn damaged_files_fail_without_panicking() {
    let root = dir("bad");
    let cases = ["nope", "empty", "cut", "odd", "past"];
    write(&root.join("nope.wav"), b"this is not a wave file");
    write(&root.join("empty.wav"), b"");
    let mut cut = Sound::tone(8_000, 220.0, 0.02, 0.2).to_wav16();
    cut.truncate(20);
    write(&root.join("cut.wav"), &cut);
    let mut odd = wav_f32(8_000, 1, &[0.1, 0.2, 0.3]);
    let size = u32::from_le_bytes(odd[40..44].try_into().unwrap());
    odd[40..44].copy_from_slice(&(size - 1).to_le_bytes());
    odd.pop();
    write(&root.join("odd.wav"), &odd);
    let mut past = wav_f32(8_000, 1, &[0.1, 0.2, 0.3, 0.4]);
    let size = u32::from_le_bytes(past[40..44].try_into().unwrap());
    past[40..44].copy_from_slice(&(size + 64).to_le_bytes());
    write(&root.join("past.wav"), &past);
    for name in cases {
        let path = root.join(format!("{name}.wav"));
        let opened = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            seal_audio::stream::FileReader::open(&path).map(|_| ())
        }));
        let opened = opened.expect("panic while opening a damaged file");
        assert!(opened.is_err(), "{name} opened");
        let played = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            MusicPlayer::new(8_000).play(&path, true, 0.0, 0.0)
        }));
        assert!(played.expect("panic while playing a damaged file").is_err(), "{name}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_mixer_adds_the_music_bus() {
    let root = dir("mix");
    let path = root.join("tone.wav");
    write(&path, &wav_f32(8_000, 1, &[0.5; 64]));
    let mut mixer = Mixer::new(8_000);
    mixer.music.play(&path, false, 0.0, 0.0).unwrap();
    let mut out = vec![0.0; 8];
    mixer.mix(&mut out);
    assert!(out.iter().all(|s| (*s - 0.5).abs() < 1e-5), "{out:?}");
    mixer.music.set_volume(0.0);
    mixer.mix(&mut out);
    assert!(out.iter().all(|s| s.abs() < 1e-5), "{out:?}");
    let _ = std::fs::remove_dir_all(&root);
}
