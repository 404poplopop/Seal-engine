//! The Seal audio mixer.
//!
//! Sounds are decoded once into `f32` samples. Each playing copy is a
//! [`Voice`] with its own gain, pan, speed and bus. [`Mixer::mix`] fills an
//! interleaved stereo buffer; a platform output (or a test) just calls it
//! for each block. The mixer only uses exactly specified float operations
//! and the engine's own sine and cosine, so the same calls give the same
//! samples on every machine, which lets replays and tests compare audio.
//!
//! Gain and pan changes ramp over a short time instead of jumping, so they
//! never click. Long music is a separate bus: [`stream::MusicPlayer`]
//! decodes a file from disk in small reads and [`Mixer::mix`] adds it.

#![allow(unknown_lints, clippy::chunks_exact_to_as_chunks)]

mod decode;
pub mod backend;
pub mod compress;
pub mod frames;
pub mod listener;
pub mod ring;
pub mod stream;
pub mod web;
mod stream_file;
#[cfg(target_arch = "wasm32")]
pub mod web_out;

pub use stream::{MusicPlayer, MUSIC_BUFFER_LIMIT, MUSIC_BUS};
pub use decode::detect_format;
pub use decode::SoundFormat;

use std::fmt;

/// Decoded audio.
#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    pub sample_rate: u32,
    pub channels: u16,
    /// Interleaved samples in -1..1.
    pub samples: Vec<f32>,
}

impl Sound {
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels as usize
    }

    fn frame(&self, index: usize) -> (f32, f32) {
        match self.channels {
            1 => {
                let s = self.samples[index];
                (s, s)
            }
            _ => {
                let i = index * self.channels as usize;
                (self.samples[i], self.samples[i + 1])
            }
        }
    }

    /// A sine tone, handy for tests and placeholder sounds.
    pub fn tone(sample_rate: u32, hz: f32, seconds: f32, amplitude: f32) -> Self {
        let n = (sample_rate as f32 * seconds) as usize;
        let step = std::f64::consts::TAU * hz as f64 / sample_rate as f64;
        let samples =
            (0..n).map(|i| seal_math::sin_cos_f64(step * i as f64).0 as f32 * amplitude).collect();
        Self { sample_rate, channels: 1, samples }
    }

    /// Read a PCM WAV file: 8, 16, 24 or 32-bit integers, or 32-bit floats; mono or stereo.
    pub fn from_wav(bytes: &[u8]) -> Result<Self, AudioError> {
        let bad = |why: &str| AudioError::BadWav(why.to_string());
        if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return Err(bad("not a RIFF WAVE file"));
        }
        let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
        let u32_at =
            |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
        let mut pos = 12;
        let mut format = None;
        while pos + 8 <= bytes.len() {
            let id = &bytes[pos..pos + 4];
            let size = u32_at(pos + 4) as usize;
            let body = pos + 8;
            let end = body
                .checked_add(size)
                .filter(|e| *e <= bytes.len())
                .ok_or_else(|| bad("chunk runs past the end"))?;
            match id {
                b"fmt " if size >= 16 => {
                    let tag = u16_at(body);
                    let tag = if tag == 0xFFFE && size >= 40 { u16_at(body + 24) } else { tag };
                    format = Some((tag, u16_at(body + 2), u32_at(body + 4), u16_at(body + 14)));
                }
                b"data" => {
                    let (tag, channels, sample_rate, bits) =
                        format.ok_or_else(|| bad("data before fmt"))?;
                    if !(1..=2).contains(&channels) {
                        return Err(bad("only mono and stereo are supported"));
                    }
                    let data = &bytes[body..end];
                    let samples: Vec<f32> = match (tag, bits) {
                        (1, 8) => data.iter().map(|b| (*b as f32 - 128.0) / 128.0).collect(),
                        (1, 16) => data
                            .chunks_exact(2)
                            .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
                            .collect(),
                        (1, 24) => data
                            .chunks_exact(3)
                            .map(|c| {
                                (i32::from_le_bytes([0, c[0], c[1], c[2]]) >> 8) as f32
                                    / 8_388_608.0
                            })
                            .collect(),
                        (1, 32) => data
                            .chunks_exact(4)
                            .map(|c| {
                                i32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f32
                                    / 2_147_483_648.0
                            })
                            .collect(),
                        (3, 32) => data
                            .chunks_exact(4)
                            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                            .collect(),
                        _ => return Err(bad("unsupported sample format")),
                    };
                    return Ok(Self { sample_rate, channels, samples });
                }
                _ => {}
            }
            pos = end + (size & 1);
        }
        Err(bad("no data chunk"))
    }

    /// Scale every sample so the loudest one reaches 1. A silent sound is left as it is.
    pub fn peak_normalize(&mut self) {
        let peak = self.samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        if peak <= 1.0e-8 {
            return;
        }
        let gain = 1.0 / peak;
        for sample in &mut self.samples {
            *sample *= gain;
        }
    }

    /// Write 16-bit PCM WAV, for captures and tests.
    pub fn to_wav16(&self) -> Vec<u8> {
        let data_len = self.samples.len() as u32 * 2;
        let mut out = Vec::with_capacity(44 + data_len as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&self.channels.to_le_bytes());
        out.extend_from_slice(&self.sample_rate.to_le_bytes());
        let block = self.channels as u32 * 2;
        out.extend_from_slice(&(self.sample_rate * block).to_le_bytes());
        out.extend_from_slice(&(block as u16).to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for s in &self.samples {
            out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioError {
    BadWav(String),
    NoSuchSound,
    NoSuchBus(String),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadWav(why) => write!(f, "cannot read WAV: {why}"),
            Self::NoSuchSound => write!(f, "no such sound"),
            Self::NoSuchBus(b) => write!(f, "no bus named {b:?}"),
        }
    }
}

impl std::error::Error for AudioError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SoundId(u32);

/// A playing voice. Stale handles are ignored safely.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VoiceId {
    slot: u32,
    generation: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BusId(u32);

/// How a voice starts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayParams {
    pub gain: f32,
    /// -1 is full left, 1 full right.
    pub pan: f32,
    /// Playback speed; 2 is an octave up.
    pub speed: f32,
    pub looping: bool,
    pub bus: BusId,
}

impl Default for PlayParams {
    fn default() -> Self {
        Self { gain: 1.0, pan: 0.0, speed: 1.0, looping: false, bus: BusId(0) }
    }
}

/// A smoothly changing value.
#[derive(Clone, Copy, Debug)]
struct Ramp {
    value: f32,
    target: f32,
    step: f32,
}

impl Ramp {
    fn new(value: f32) -> Self {
        Self { value, target: value, step: 0.0 }
    }

    fn set(&mut self, target: f32, frames: u32) {
        self.target = target;
        self.step = if frames == 0 {
            self.value = target;
            0.0
        } else {
            (target - self.value) / frames as f32
        };
    }

    fn target(&self) -> f32 {
        self.target
    }

    fn next(&mut self) -> f32 {
        if self.step != 0.0 {
            self.value += self.step;
            if (self.step > 0.0 && self.value >= self.target)
                || (self.step < 0.0 && self.value <= self.target)
            {
                self.value = self.target;
                self.step = 0.0;
            }
        }
        self.value
    }
}

struct Voice {
    generation: u32,
    sound: SoundId,
    /// Position in source frames, 32.32 fixed point so speed changes stay exact.
    position: u64,
    step: u64,
    gain: Ramp,
    pan: Ramp,
    looping: bool,
    bus: BusId,
    stop_when_silent: bool,
    playing: bool,
}

struct Bus {
    name: String,
    gain: Ramp,
    muted: bool,
}

/// Mixes voices into stereo at a fixed output rate.
pub struct Mixer {
    pub sample_rate: u32,
    sounds: Vec<Sound>,
    voices: Vec<Voice>,
    buses: Vec<Bus>,
    /// Streamed music. Mixed on its own bus, then through the master bus.
    pub music: MusicPlayer,
    /// Frames over which gain and pan changes ramp.
    pub ramp_frames: u32,
    /// Peak absolute sample written by the last mix, before clipping.
    pub last_peak: f32,
}

impl Mixer {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            sounds: Vec::new(),
            voices: Vec::new(),
            buses: vec![Bus { name: "master".into(), gain: Ramp::new(1.0), muted: false }],
            music: MusicPlayer::new(sample_rate),
            ramp_frames: sample_rate / 200,
            last_peak: 0.0,
        }
    }

    pub fn add_sound(&mut self, sound: Sound) -> SoundId {
        self.sounds.push(sound);
        SoundId(self.sounds.len() as u32 - 1)
    }

    /// The master bus, which every other bus feeds.
    pub fn master(&self) -> BusId {
        BusId(0)
    }

    pub fn add_bus(&mut self, name: &str) -> BusId {
        self.buses.push(Bus { name: name.into(), gain: Ramp::new(1.0), muted: false });
        BusId(self.buses.len() as u32 - 1)
    }

    pub fn bus(&self, name: &str) -> Result<BusId, AudioError> {
        self.buses
            .iter()
            .position(|b| b.name == name)
            .map(|i| BusId(i as u32))
            .ok_or_else(|| AudioError::NoSuchBus(name.into()))
    }

    pub fn set_bus_gain(&mut self, bus: BusId, gain: f32) {
        let frames = self.ramp_frames;
        self.buses[bus.0 as usize].gain.set(gain, frames);
    }

    /// Gain the bus is moving toward. This is the value just set, before the ramp finishes.
    pub fn bus_gain(&self, bus: BusId) -> f32 {
        self.buses.get(bus.0 as usize).map(|bus| bus.gain.target()).unwrap_or(0.0)
    }

    pub fn set_bus_muted(&mut self, bus: BusId, muted: bool) {
        self.buses[bus.0 as usize].muted = muted;
    }

    fn step_for(&self, sound: SoundId, speed: f32) -> u64 {
        let ratio = self.sounds[sound.0 as usize].sample_rate as f64 / self.sample_rate as f64
            * speed.max(0.0) as f64;
        (ratio * 4_294_967_296.0).round() as u64
    }

    pub fn play(&mut self, sound: SoundId, params: PlayParams) -> Result<VoiceId, AudioError> {
        if sound.0 as usize >= self.sounds.len() {
            return Err(AudioError::NoSuchSound);
        }
        let step = self.step_for(sound, params.speed);
        let slot = self.voices.iter().position(|v| !v.playing);
        let generation = slot.map_or(0, |s| self.voices[s].generation.wrapping_add(1));
        let voice = Voice {
            generation,
            sound,
            position: 0,
            step,
            gain: Ramp::new(params.gain),
            pan: Ramp::new(params.pan.clamp(-1.0, 1.0)),
            looping: params.looping,
            bus: params.bus,
            stop_when_silent: false,
            playing: true,
        };
        let slot = match slot {
            Some(s) => {
                self.voices[s] = voice;
                s
            }
            None => {
                self.voices.push(voice);
                self.voices.len() - 1
            }
        };
        Ok(VoiceId { slot: slot as u32, generation })
    }

    fn voice(&mut self, id: VoiceId) -> Option<&mut Voice> {
        self.voices.get_mut(id.slot as usize).filter(|v| v.playing && v.generation == id.generation)
    }

    pub fn is_playing(&self, id: VoiceId) -> bool {
        self.voices
            .get(id.slot as usize)
            .is_some_and(|v| v.playing && v.generation == id.generation)
    }

    pub fn playing_count(&self) -> usize {
        self.voices.iter().filter(|v| v.playing).count()
    }

    pub fn set_gain(&mut self, id: VoiceId, gain: f32) {
        let frames = self.ramp_frames;
        if let Some(v) = self.voice(id) {
            v.gain.set(gain, frames);
        }
    }

    pub fn set_pan(&mut self, id: VoiceId, pan: f32) {
        let frames = self.ramp_frames;
        if let Some(v) = self.voice(id) {
            v.pan.set(pan.clamp(-1.0, 1.0), frames);
        }
    }

    pub fn set_speed(&mut self, id: VoiceId, speed: f32) {
        let Some(sound) = self.voice(id).map(|v| v.sound) else { return };
        let step = self.step_for(sound, speed);
        if let Some(v) = self.voice(id) {
            v.step = step;
        }
    }

    /// Fade out over `seconds`, then stop. Zero stops after the short click-free ramp.
    pub fn stop(&mut self, id: VoiceId, seconds: f32) {
        let frames = ((seconds * self.sample_rate as f32) as u32).max(self.ramp_frames).max(1);
        if let Some(v) = self.voice(id) {
            v.gain.set(0.0, frames);
            v.stop_when_silent = true;
        }
    }

    /// Fill `out` with interleaved stereo frames.
    pub fn mix(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        let frames = out.len() / 2;
        let mut peak = 0.0f32;
        let bus_count = self.buses.len();
        // Per-frame bus gains, computed once for the block.
        let mut bus_gain = vec![vec![0.0f32; frames]; bus_count];
        for (b, bus) in self.buses.iter_mut().enumerate() {
            for g in bus_gain[b].iter_mut() {
                let value = bus.gain.next();
                *g = if bus.muted { 0.0 } else { value };
            }
        }
        for voice in self.voices.iter_mut().filter(|v| v.playing) {
            let sound = &self.sounds[voice.sound.0 as usize];
            let length = sound.frames() as u64;
            let bus = (voice.bus.0 as usize).min(bus_count - 1);
            for f in 0..frames {
                let index = voice.position >> 32;
                if index >= length {
                    if voice.looping && length > 0 {
                        voice.position -= length << 32;
                    } else {
                        voice.playing = false;
                        break;
                    }
                }
                let (l, r) = sound.frame((voice.position >> 32) as usize);
                let gain = voice.gain.next() * bus_gain[bus][f] * bus_gain[0][f];
                let pan = voice.pan.next();
                // Constant power: equal loudness at every pan position.
                let angle = (pan + 1.0) * (std::f32::consts::FRAC_PI_4);
                let (sin, cos) = seal_math::sin_cos(angle);
                out[f * 2] += l * gain * cos;
                out[f * 2 + 1] += r * gain * sin;
                voice.position += voice.step;
                if voice.stop_when_silent && voice.gain.value == 0.0 {
                    voice.playing = false;
                    break;
                }
            }
        }
        let music_scaled = self
            .buses
            .iter()
            .position(|bus| bus.name == MUSIC_BUS)
            .filter(|index| *index != 0)
            .map(|index| {
                bus_gain[0]
                    .iter()
                    .zip(&bus_gain[index])
                    .map(|(master, bus)| master * bus)
                    .collect::<Vec<_>>()
            });
        let music_gain = music_scaled.as_deref().unwrap_or(&bus_gain[0]);
        self.music.render(out, music_gain);
        for s in out.iter_mut() {
            peak = peak.max(s.abs());
            *s = s.clamp(-1.0, 1.0);
        }
        self.last_peak = peak;
    }
}
