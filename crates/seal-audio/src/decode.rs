//! Sound file decoding by content and extension: WAV, FLAC, OGG Vorbis and MP3.

use std::ffi::OsStr;
use std::io::Cursor;
use std::path::Path;

use crate::{AudioError, Sound};

pub(crate) fn bad(why: impl Into<String>) -> AudioError {
    AudioError::BadWav(why.into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundFormat {
    Wav,
    Flac,
    Ogg,
    Mp3,
}

/// Guess format from magic bytes and an optional path hint.
pub fn detect_format(hint: Option<&Path>, bytes: &[u8]) -> SoundFormat {
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        return SoundFormat::Wav;
    }
    if bytes.len() >= 4 && &bytes[0..4] == b"fLaC" {
        return SoundFormat::Flac;
    }
    if bytes.len() >= 4 && &bytes[0..4] == b"OggS" {
        return SoundFormat::Ogg;
    }
    if bytes.len() >= 2 && bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0 {
        return SoundFormat::Mp3;
    }
    if bytes.len() >= 3 && &bytes[0..3] == b"ID3" {
        return SoundFormat::Mp3;
    }
    match hint.and_then(|p| p.extension()).and_then(OsStr::to_str).map(str::to_ascii_lowercase) {
        Some(ext) if ext == "wav" => SoundFormat::Wav,
        Some(ext) if ext == "flac" => SoundFormat::Flac,
        Some(ext) if ext == "ogg" => SoundFormat::Ogg,
        Some(ext) if ext == "mp3" => SoundFormat::Mp3,
        _ => SoundFormat::Wav,
    }
}

impl Sound {
    /// Decode a sound from bytes. The optional path supplies the extension when magic bytes are missing.
    pub fn from_bytes(bytes: &[u8], hint: Option<&Path>) -> Result<Self, AudioError> {
        match detect_format(hint, bytes) {
            SoundFormat::Wav => Self::from_wav(bytes),
            SoundFormat::Flac => decode_flac(bytes),
            SoundFormat::Ogg => decode_ogg(bytes),
            SoundFormat::Mp3 => decode_mp3(bytes),
        }
    }
}

pub(crate) fn decode_flac(bytes: &[u8]) -> Result<Sound, AudioError> {
    let mut reader =
        claxon::FlacReader::new(Cursor::new(bytes)).map_err(|e| bad(format!("FLAC: {e}")))?;
    let info = reader.streaminfo();
    let channels = info.channels as u16;
    if !(1..=2).contains(&channels) {
        return Err(bad("only mono and stereo are supported"));
    }
    let sample_rate = info.sample_rate;
    let scale = 2f32.powi((info.bits_per_sample - 1) as i32);
    let mut samples = Vec::new();
    for sample in reader.samples() {
        let s = sample.map_err(|e| bad(format!("FLAC: {e}")))?;
        samples.push(s as f32 / scale);
    }
    Ok(Sound { sample_rate, channels, samples })
}

pub(crate) fn decode_ogg(bytes: &[u8]) -> Result<Sound, AudioError> {
    let mut reader = lewton::inside_ogg::OggStreamReader::new(Cursor::new(bytes))
        .map_err(|e| bad(format!("Vorbis: {e}")))?;
    let channels = reader.ident_hdr.audio_channels as u16;
    if !(1..=2).contains(&channels) {
        return Err(bad("only mono and stereo are supported"));
    }
    let sample_rate = reader.ident_hdr.audio_sample_rate;
    let mut samples = Vec::new();
    while let Some(packet) = reader
        .read_dec_packet_itl()
        .map_err(|e| bad(format!("Vorbis: {e}")))?
    {
        for s in packet {
            samples.push(s as f32 / 32768.0);
        }
    }
    Ok(Sound { sample_rate, channels, samples })
}

pub(crate) fn decode_mp3(bytes: &[u8]) -> Result<Sound, AudioError> {
    // Same reader as the streaming path, so one-shot and chunked decoding match sample for sample.
    let mut reader = nanomp3::SliceReader::<f32>::new(bytes);
    let channels = match reader.channels() {
        Some(c) => u16::from(c.num()),
        None => return Err(bad("MP3: no audio frames")),
    };
    let sample_rate = reader.sample_rate();
    let mut samples = Vec::new();
    let mut pcm = vec![0f32; 4096];
    loop {
        let n = reader.read(&mut pcm).map_err(|e| bad(format!("MP3: {e}")))?;
        if n == 0 {
            break;
        }
        samples.extend_from_slice(&pcm[..n]);
    }
    if samples.is_empty() {
        return Err(bad("MP3: no audio frames"));
    }
    Ok(Sound { sample_rate, channels, samples })
}
