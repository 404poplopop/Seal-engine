//! Sound compression for export and loading compressed assets.

use crate::{AudioError, Sound};

/// How import/export stores sample data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundCompression {
    Pcm,
    Adpcm,
    Vorbis,
}

impl SoundCompression {
    pub fn parse(text: &str) -> Self {
        match text.trim().to_ascii_lowercase().as_str() {
            "adpcm" => Self::Adpcm,
            "vorbis" => Self::Vorbis,
            _ => Self::Pcm,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Pcm => "pcm",
            Self::Adpcm => "adpcm",
            Self::Vorbis => "vorbis",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundLoadType {
    Memory,
    Stream,
}

impl SoundLoadType {
    pub fn parse(text: &str) -> Self {
        if text.trim().eq_ignore_ascii_case("stream") {
            Self::Stream
        } else {
            Self::Memory
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Stream => "stream",
        }
    }
}

const ADPCM_TAG: u16 = 0x0011;

/// Encode `sound` for export. Vorbis output is an OGG stream; PCM and ADPCM use WAV.
pub fn encode_for_export(sound: &Sound, mode: SoundCompression, vorbis_quality: u8) -> Vec<u8> {
    match mode {
        SoundCompression::Pcm => sound.to_wav16(),
        SoundCompression::Adpcm => encode_adpcm_wav(sound),
        SoundCompression::Vorbis => encode_vorbis_ogg(sound, vorbis_quality),
    }
}

/// Decode bytes written by [`encode_for_export`].
pub fn decode_exported(bytes: &[u8]) -> Result<Sound, AudioError> {
    if bytes.len() >= 4 && &bytes[0..4] == b"OggS" {
        return crate::Sound::from_bytes(bytes, Some(std::path::Path::new("x.ogg")));
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" {
        return decode_wav_with_adpcm(bytes);
    }
    Sound::from_bytes(bytes, Some(std::path::Path::new("x.wav")))
}

/// Vorbis export: no Vorbis encoder ships with Seal, so a source that is already OGG Vorbis is
/// kept as it is by the export step, and any other source is stored as IMA ADPCM.
fn encode_vorbis_ogg(sound: &Sound, _quality: u8) -> Vec<u8> {
    encode_adpcm_wav(sound)
}

const IMA_STEPS: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408,
    449, 494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066,
    2272, 2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630,
    9493, 10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794,
    32767,
];
const IMA_INDEX: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];
/// Bytes per channel in one block (header included); 256 is the usual size.
const BLOCK_PER_CHANNEL: usize = 256;

#[derive(Clone, Copy)]
struct ImaState {
    predictor: i32,
    index: i32,
}

impl ImaState {
    fn decode(&mut self, nibble: u8) -> i16 {
        let step = IMA_STEPS[self.index as usize];
        let mut diff = step >> 3;
        if nibble & 4 != 0 {
            diff += step;
        }
        if nibble & 2 != 0 {
            diff += step >> 1;
        }
        if nibble & 1 != 0 {
            diff += step >> 2;
        }
        self.predictor += if nibble & 8 != 0 { -diff } else { diff };
        self.predictor = self.predictor.clamp(-32768, 32767);
        self.index = (self.index + IMA_INDEX[nibble as usize & 15]).clamp(0, 88);
        self.predictor as i16
    }

    fn encode(&mut self, sample: i16) -> u8 {
        let step = IMA_STEPS[self.index as usize];
        let mut delta = i32::from(sample) - self.predictor;
        let mut nibble = 0u8;
        if delta < 0 {
            nibble = 8;
            delta = -delta;
        }
        let mut s = step;
        if delta >= s {
            nibble |= 4;
            delta -= s;
        }
        s >>= 1;
        if delta >= s {
            nibble |= 2;
            delta -= s;
        }
        s >>= 1;
        if delta >= s {
            nibble |= 1;
        }
        // Track the decoder exactly so encoder and decoder never drift apart.
        self.decode(nibble);
        nibble
    }
}

fn to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

/// Frames held by one block: the header sample plus two per data byte.
fn frames_per_block() -> usize {
    1 + (BLOCK_PER_CHANNEL - 4) * 2
}

/// Standard IMA ADPCM in a WAV file (format tag 0x0011): 4 bits per sample, about a quarter of
/// 16-bit PCM. Each block starts with every channel's first sample and step index, then the
/// channels' nibbles interleaved in 4-byte groups (8 samples each).
fn encode_adpcm_wav(sound: &Sound) -> Vec<u8> {
    let channels = sound.channels.max(1) as usize;
    let frames = sound.frames();
    let per_block = frames_per_block();
    let block_align = BLOCK_PER_CHANNEL * channels;
    let sample_at = |frame: usize, ch: usize| {
        if frame < frames {
            to_i16(sound.samples.get(frame * channels + ch).copied().unwrap_or(0.0))
        } else {
            0
        }
    };
    // Start each channel's step size near its first change so the opening samples track the
    // wave instead of ramping up from the smallest step (the block header carries the index).
    let mut states: Vec<ImaState> = (0..channels)
        .map(|ch| {
            let delta = (i32::from(sample_at(1, ch)) - i32::from(sample_at(0, ch))).abs();
            let index = IMA_STEPS.iter().position(|&step| step >= delta).unwrap_or(88) as i32;
            ImaState { predictor: 0, index }
        })
        .collect();
    let mut data = Vec::new();
    let mut start = 0;
    while start < frames.max(1) {
        for (ch, state) in states.iter_mut().enumerate() {
            let first = sample_at(start, ch);
            state.predictor = i32::from(first);
            data.extend_from_slice(&first.to_le_bytes());
            data.push(state.index as u8);
            data.push(0);
        }
        // 8 samples per 4-byte group per channel.
        let mut frame = start + 1;
        while frame < start + per_block {
            for (ch, state) in states.iter_mut().enumerate() {
                for pair in 0..4 {
                    let lo = state.encode(sample_at(frame + pair * 2, ch));
                    let hi = state.encode(sample_at(frame + pair * 2 + 1, ch));
                    data.push(lo | (hi << 4));
                }
            }
            frame += 8;
        }
        start += per_block;
    }
    let data_len = data.len() as u32;
    let mut out = Vec::with_capacity(64 + data.len());
    out.extend_from_slice(b"RIFF");
    let riff_size = 4 + (8 + 20) + (8 + 4) + 8 + data_len;
    out.extend_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&20u32.to_le_bytes());
    out.extend_from_slice(&ADPCM_TAG.to_le_bytes());
    out.extend_from_slice(&(channels as u16).to_le_bytes());
    out.extend_from_slice(&sound.sample_rate.to_le_bytes());
    let byte_rate = (sound.sample_rate as u64 * block_align as u64 / per_block as u64) as u32;
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&(block_align as u16).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&(per_block as u16).to_le_bytes());
    out.extend_from_slice(b"fact");
    out.extend_from_slice(&4u32.to_le_bytes());
    out.extend_from_slice(&(frames as u32).to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(&data);
    out
}

fn decode_wav_with_adpcm(bytes: &[u8]) -> Result<Sound, AudioError> {
    let bad = |why: &str| AudioError::BadWav(why.to_string());
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(bad("not a WAV file"));
    }
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at =
        |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    let mut pos = 12;
    let mut format = None;
    let mut fact_frames = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32_at(pos + 4) as usize;
        let body = pos + 8;
        let end = body
            .checked_add(size)
            .filter(|e| *e <= bytes.len())
            .ok_or_else(|| bad("chunk past end"))?;
        if id == b"fmt " && size >= 16 {
            format = Some((u16_at(body), u16_at(body + 2), u32_at(body + 4), u16_at(body + 12)));
        }
        if id == b"fact" && size >= 4 {
            fact_frames = Some(u32_at(body) as usize);
        }
        if id == b"data" {
            let (tag, channels, sample_rate, block_align) =
                format.ok_or_else(|| bad("data before fmt"))?;
            if tag == ADPCM_TAG {
                return decode_ima_adpcm(&bytes[body..end], channels, sample_rate, block_align, fact_frames);
            }
            return Sound::from_wav(bytes);
        }
        pos = end + (size & 1);
    }
    Err(bad("no data"))
}

fn decode_ima_adpcm(
    data: &[u8],
    channels: u16,
    sample_rate: u32,
    block_align: u16,
    fact_frames: Option<usize>,
) -> Result<Sound, AudioError> {
    let channels = channels.max(1) as usize;
    let block = block_align as usize;
    if block < 4 * channels || (block - 4 * channels) % (4 * channels) != 0 {
        return Err(AudioError::BadWav("bad ADPCM block size".into()));
    }
    let per_block = 1 + (block - 4 * channels) * 2 / channels;
    let mut samples: Vec<f32> = Vec::with_capacity(data.len() / block * per_block * channels);
    for chunk in data.chunks(block) {
        if chunk.len() < 4 * channels {
            break;
        }
        let mut states = Vec::with_capacity(channels);
        let mut frames = vec![vec![0i16; per_block]; channels];
        for ch in 0..channels {
            let h = &chunk[ch * 4..ch * 4 + 4];
            let first = i16::from_le_bytes([h[0], h[1]]);
            states.push(ImaState { predictor: i32::from(first), index: i32::from(h[2]).clamp(0, 88) });
            frames[ch][0] = first;
        }
        let body = &chunk[4 * channels..];
        let mut filled = vec![1usize; channels];
        for (group, bytes) in body.chunks(4).enumerate() {
            let ch = group % channels;
            for byte in bytes {
                for nibble in [byte & 15, byte >> 4] {
                    if filled[ch] < per_block {
                        frames[ch][filled[ch]] = states[ch].decode(nibble);
                        filled[ch] += 1;
                    }
                }
            }
        }
        let usable = filled.iter().copied().min().unwrap_or(0);
        for i in 0..usable {
            for ch in 0..channels {
                samples.push(f32::from(frames[ch][i]) / 32767.0);
            }
        }
    }
    if let Some(total) = fact_frames {
        samples.truncate(total * channels);
    }
    Ok(Sound { sample_rate, channels: channels as u16, samples })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(a: &[f32], b: &[f32]) -> (f32, f32) {
        let n = a.len().min(b.len()).max(1);
        let sq: f32 = a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum();
        let max = a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0f32, f32::max);
        ((sq / n as f32).sqrt(), max)
    }

    #[test]
    fn adpcm_round_trip_is_close_and_about_a_quarter_of_pcm() {
        let sound = Sound::tone(44_100, 440.0, 0.25, 0.8);
        let pcm = encode_for_export(&sound, SoundCompression::Pcm, 5);
        let adpcm = encode_for_export(&sound, SoundCompression::Adpcm, 5);
        assert!(adpcm.len() * 3 < pcm.len(), "{} vs {}", adpcm.len(), pcm.len());
        let back = decode_exported(&adpcm).unwrap();
        assert_eq!((back.sample_rate, back.channels), (sound.sample_rate, sound.channels));
        assert_eq!(back.samples.len(), sound.samples.len());
        let (rms, max) = errors(&back.samples, &sound.samples);
        // IMA ADPCM adapts its step size over the first few samples, so the bound is on RMS.
        assert!(rms < 0.01 && max < 0.1, "rms {rms} max {max}");
    }

    #[test]
    fn stereo_ima_blocks_keep_channels_apart_and_the_exact_length() {
        let tone = Sound::tone(22_050, 300.0, 0.3, 0.5);
        let samples: Vec<f32> = tone.samples.iter().flat_map(|s| [*s, 0.1]).collect();
        let sound = Sound { sample_rate: 22_050, channels: 2, samples };
        let back = decode_exported(&encode_for_export(&sound, SoundCompression::Adpcm, 5)).unwrap();
        assert_eq!(back.channels, 2);
        assert_eq!(back.samples.len(), sound.samples.len());
        let right: Vec<f32> = back.samples.iter().skip(1).step_by(2).copied().collect();
        assert!(right.iter().all(|v| (v - 0.1).abs() < 0.01), "right channel holds its level");
        let left: Vec<f32> = back.samples.iter().step_by(2).copied().collect();
        assert!(errors(&left, &tone.samples).0 < 0.01);
    }

    #[test]
    fn damaged_compressed_input_fails_cleanly() {
        assert!(decode_exported(b"not audio").is_err());
        assert!(decode_exported(b"RIFF____WAVE").is_err());
    }
}