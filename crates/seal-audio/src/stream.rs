//! Streamed music.
//!
//! A long sound file plays from disk in small reads. Decoded frames sit in a
//! fixed ring, so memory stays at the ring size however long the file is.
//! One music bus carries the current track. A new track can crossfade in:
//! the outgoing and incoming gains add up to 1 for every frame of the fade,
//! then the music-bus volume scales both. Pausing freezes the playhead.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::stream_file::AudioFileReader;
use crate::AudioError;

pub use crate::stream_file::AudioFileReader as FileReader;

/// Decoded frames kept for one track. The read scratch is separate and small.
pub const RING_FRAMES: usize = 4096;
const READ_FRAMES: usize = 1024;
/// Upper bound on decoded audio plus the read scratch, for two tracks.
pub const MUSIC_BUFFER_LIMIT: usize = 256 * 1024;

/// The bus streamed music mixes on. Sound effects do not use it.
pub const MUSIC_BUS: &str = "music";

fn bad(why: impl Into<String>) -> AudioError {
    AudioError::BadWav(why.into())
}

fn read_exact(file: &mut File, buf: &mut [u8]) -> Result<(), AudioError> {
    file.read_exact(buf).map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            bad("truncated")
        } else {
            bad(e.to_string())
        }
    })
}

fn decode_pcm(tag: u16, bits: u16, data: &[u8], out: &mut Vec<f32>) -> Result<(), AudioError> {
    match (tag, bits) {
        (1, 8) => {
            for b in data {
                out.push((*b as f32 - 128.0) / 128.0);
            }
        }
        (1, 16) => {
            for c in data.chunks_exact(2) {
                out.push(i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0);
            }
        }
        (1, 24) => {
            for c in data.chunks_exact(3) {
                out.push((i32::from_le_bytes([0, c[0], c[1], c[2]]) >> 8) as f32 / 8_388_608.0);
            }
        }
        (1, 32) => {
            for c in data.chunks_exact(4) {
                out.push(i32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f32 / 2_147_483_648.0);
            }
        }
        (3, 32) => {
            for c in data.chunks_exact(4) {
                out.push(f32::from_le_bytes([c[0], c[1], c[2], c[3]]));
            }
        }
        _ => return Err(bad("unsupported sample format")),
    }
    Ok(())
}

/// A WAV file read from disk a few frames at a time.
pub struct WavReader {
    file: File,
    sample_rate: u32,
    channels: u16,
    tag: u16,
    bits: u16,
    block: usize,
    data_pos: u64,
    data_len: u64,
    /// Next byte to read, relative to the start of the data chunk.
    cursor: u64,
    scratch: Vec<u8>,
}

impl WavReader {
    pub fn open(path: &Path) -> Result<Self, AudioError> {
        let mut file = File::open(path).map_err(|e| bad(e.to_string()))?;
        let len = file.metadata().map_err(|e| bad(e.to_string()))?.len();
        if len < 12 {
            return Err(bad("truncated"));
        }
        let mut hdr = [0u8; 12];
        read_exact(&mut file, &mut hdr)?;
        if &hdr[0..4] != b"RIFF" || &hdr[8..12] != b"WAVE" {
            return Err(bad("not a RIFF WAVE file"));
        }
        let mut pos = 12u64;
        let mut format = None;
        let mut data_at = None;
        while pos + 8 <= len {
            file.seek(SeekFrom::Start(pos)).map_err(|e| bad(e.to_string()))?;
            let mut chunk = [0u8; 8];
            read_exact(&mut file, &mut chunk)?;
            let id = &chunk[0..4];
            let size = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]) as u64;
            let body = pos + 8;
            let end = body.checked_add(size).ok_or_else(|| bad("chunk runs past the end"))?;
            if end > len {
                return Err(bad("chunk runs past the end"));
            }
            if id == b"fmt " {
                if size < 16 {
                    return Err(bad("fmt chunk is too small"));
                }
                let need = (size as usize).min(40);
                let mut buf = [0u8; 40];
                read_exact(&mut file, &mut buf[..need])?;
                let u16_at = |i: usize| u16::from_le_bytes([buf[i], buf[i + 1]]);
                let u32_at =
                    |i: usize| u32::from_le_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]);
                let mut tag = u16_at(0);
                if tag == 0xFFFE {
                    if size < 40 {
                        return Err(bad("extensible fmt chunk is too small"));
                    }
                    tag = u16_at(24);
                }
                format = Some((tag, u16_at(2), u32_at(4), u16_at(14)));
            } else if id == b"data" {
                let format = format.ok_or_else(|| bad("data before fmt"))?;
                data_at = Some((body, size, format));
                break;
            }
            pos = end + (size & 1);
        }
        let (data_pos, data_len, (tag, channels, sample_rate, bits)) =
            data_at.ok_or_else(|| bad("no data chunk"))?;
        if !(1..=2).contains(&channels) {
            return Err(bad("only mono and stereo are supported"));
        }
        if sample_rate == 0 || bits == 0 || bits % 8 != 0 {
            return Err(bad("unsupported sample format"));
        }
        let block = channels as usize * (bits as usize / 8);
        if block == 0 || data_len % block as u64 != 0 {
            return Err(bad("truncated sample frame"));
        }
        if data_len / block as u64 == 0 {
            return Err(bad("no frames"));
        }
        let mut scratch = Vec::new();
        scratch.reserve_exact(READ_FRAMES * block);
        let mut reader = Self {
            file,
            sample_rate,
            channels,
            tag,
            bits,
            block,
            data_pos,
            data_len,
            cursor: 0,
            scratch,
        };
        reader.seek_frame(0)?;
        Ok(reader)
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn frames(&self) -> u64 {
        self.data_len / self.block as u64
    }

    pub fn buffered_bytes(&self) -> usize {
        self.scratch.capacity()
    }

    pub(crate) fn seek_frame(&mut self, frame: u64) -> Result<(), AudioError> {
        if frame > self.frames() {
            return Err(bad("seek past the end"));
        }
        let cursor = frame * self.block as u64;
        self.file.seek(SeekFrom::Start(self.data_pos + cursor)).map_err(|e| bad(e.to_string()))?;
        self.cursor = cursor;
        Ok(())
    }

    /// Decode the next `frames` source frames and append the interleaved samples.
    pub(crate) fn read_into(
        &mut self,
        frames: usize,
        out: &mut Vec<f32>,
    ) -> Result<(), AudioError> {
        let mut left = frames;
        while left > 0 {
            let n = left.min(READ_FRAMES);
            let bytes = n * self.block;
            let cursor = self.cursor.checked_add(bytes as u64).ok_or_else(|| bad("truncated"))?;
            if cursor > self.data_len {
                return Err(bad("truncated"));
            }
            self.scratch.resize(bytes, 0);
            read_exact(&mut self.file, &mut self.scratch[..bytes])?;
            self.cursor = cursor;
            decode_pcm(self.tag, self.bits, &self.scratch[..bytes], out)?;
            left -= n;
        }
        Ok(())
    }
}

/// Decode `path` in chunks of `chunk_frames` and return every sample.
/// The result matches [`crate::Sound::from_bytes`] on the same file.
pub fn decode_file_chunked(path: &Path, chunk_frames: usize) -> Result<Vec<f32>, AudioError> {
    let mut reader = AudioFileReader::open(path)?;
    let total = reader.frames() as usize;
    let mut out = Vec::with_capacity(total * reader.channels() as usize);
    let mut done = 0usize;
    let chunk = chunk_frames.max(1);
    while done < total {
        let n = chunk.min(total - done);
        let before = out.len();
        reader.read_into(n, &mut out)?;
        let got = (out.len() - before) / reader.channels() as usize;
        if got != n {
            return Err(bad("truncated"));
        }
        done += n;
    }
    Ok(out)
}

struct Track {
    reader: AudioFileReader,
    /// Interleaved decoded frames. `origin` is the source frame of index 0.
    ring: Vec<f32>,
    origin: u64,
    filled: usize,
    /// Next source position, in frames.
    pos: f64,
    looping: bool,
    loop_start: u64,
    out_rate: u32,
}

impl Track {
    fn open(
        path: &Path,
        out_rate: u32,
        looping: bool,
        loop_start: f64,
    ) -> Result<Self, AudioError> {
        let reader = AudioFileReader::open(path)?;
        let total = reader.frames();
        let mut start = if loop_start.is_finite() && loop_start > 0.0 {
            (loop_start * reader.sample_rate() as f64).round() as u64
        } else {
            0
        };
        if total > 0 {
            start = start.min(total - 1);
        }
        let ch = reader.channels() as usize;
        let mut ring = Vec::new();
        ring.reserve_exact(RING_FRAMES * ch);
        Ok(Self {
            reader,
            ring,
            origin: 0,
            filled: 0,
            pos: 0.0,
            looping,
            loop_start: start,
            out_rate: out_rate.max(1),
        })
    }

    fn buffered_bytes(&self) -> usize {
        self.ring.capacity() * std::mem::size_of::<f32>() + self.reader.buffered_bytes()
    }

    fn map_index(&self, idx: u64) -> Option<u64> {
        let total = self.reader.frames();
        if total == 0 {
            return None;
        }
        if idx < total {
            return Some(idx);
        }
        if !self.looping {
            return None;
        }
        let start = self.loop_start.min(total - 1);
        let span = total - start;
        if span == 0 {
            return Some(start);
        }
        Some(start + (idx - start) % span)
    }

    fn ensure(&mut self, frame: u64) -> Result<(), AudioError> {
        if self.filled > 0 && frame >= self.origin && frame < self.origin + self.filled as u64 {
            return Ok(());
        }
        let total = self.reader.frames();
        if frame >= total {
            return Err(bad("truncated"));
        }
        self.reader.seek_frame(frame)?;
        let n = RING_FRAMES.min((total - frame) as usize);
        self.ring.clear();
        self.reader.read_into(n, &mut self.ring)?;
        let ch = self.reader.channels() as usize;
        self.origin = frame;
        self.filled = self.ring.len() / ch;
        if self.filled == 0 {
            return Err(bad("truncated"));
        }
        Ok(())
    }

    fn sample(&self, frame: u64) -> Result<(f32, f32), AudioError> {
        if frame < self.origin || frame >= self.origin + self.filled as u64 {
            return Err(bad("frame is not buffered"));
        }
        let ch = self.reader.channels() as usize;
        let i = (frame - self.origin) as usize * ch;
        if ch == 1 {
            Ok((self.ring[i], self.ring[i]))
        } else {
            Ok((self.ring[i], self.ring[i + 1]))
        }
    }

    /// One output frame, or `None` when a one-shot has finished.
    fn pull(&mut self) -> Result<Option<(f32, f32)>, AudioError> {
        let step = self.reader.sample_rate() as f64 / self.out_rate as f64;
        if !self.pos.is_finite() {
            self.pos = 0.0;
        }
        let idx = self.pos.floor().max(0.0) as u64;
        let Some(frame) = self.map_index(idx) else { return Ok(None) };
        if self.looping && idx != frame {
            let frac = self.pos - self.pos.floor();
            self.pos = frame as f64 + frac;
        }
        self.ensure(frame)?;
        let sample = self.sample(frame)?;
        self.pos += step;
        Ok(Some(sample))
    }
}

fn fade_len(seconds: f32, rate: u32) -> u32 {
    if !seconds.is_finite() || seconds <= 0.0 {
        return 0;
    }
    let n = seconds as f64 * rate as f64;
    if n <= 0.0 {
        0
    } else if n >= u32::MAX as f64 {
        u32::MAX
    } else {
        n.round() as u32
    }
}

/// Streams one music bus: play, stop with a fade, pause, loop, volume, crossfade.
pub struct MusicPlayer {
    out_rate: u32,
    /// Music-bus volume. 1 leaves the file as recorded.
    volume: f32,
    paused: bool,
    tracks: [Option<Track>; 2],
    /// Slot that is fading in. The other slot, when present, fades out.
    fade_in: Option<usize>,
    fade_i: u32,
    fade_n: u32,
    stopping: bool,
    stop_i: u32,
    stop_n: u32,
    /// Set when a read fails. Playback of that track stops; it does not panic.
    pub last_error: Option<AudioError>,
}

impl MusicPlayer {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            out_rate: sample_rate.max(1),
            volume: 1.0,
            paused: false,
            tracks: [None, None],
            fade_in: None,
            fade_i: 0,
            fade_n: 0,
            stopping: false,
            stop_i: 0,
            stop_n: 0,
            last_error: None,
        }
    }

    pub fn bus(&self) -> &'static str {
        MUSIC_BUS
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn set_output_rate(&mut self, rate: u32) {
        self.out_rate = rate.max(1);
    }

    pub fn set_volume(&mut self, volume: f32) {
        if volume.is_finite() {
            self.volume = volume.max(0.0);
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn is_playing(&self) -> bool {
        self.tracks.iter().any(|t| t.is_some())
    }

    pub fn pause(&mut self) {
        self.paused = true;
    }

    pub fn resume(&mut self) {
        self.paused = false;
    }

    /// Bytes of decoded audio and read scratch this player is holding.
    pub fn buffered_bytes(&self) -> usize {
        self.tracks.iter().flatten().map(Track::buffered_bytes).sum()
    }

    /// Crossfade weights of the two slots, before the music-bus volume.
    /// During a crossfade they add up to 1.
    pub fn fade_gains(&self) -> [f32; 2] {
        [self.weight(0), self.weight(1)]
    }

    /// Start `path`. `loop_start` is seconds from the start of the file, used when `looping`.
    /// `fade_seconds` is how long the new track takes to replace whatever is playing.
    /// Zero starts at full gain, or replaces the current track at once.
    pub fn play(
        &mut self,
        path: &Path,
        looping: bool,
        loop_start: f64,
        fade_seconds: f32,
    ) -> Result<(), AudioError> {
        let track = Track::open(path, self.out_rate, looping, loop_start)?;
        let frames = fade_len(fade_seconds, self.out_rate);
        let busy = self.tracks.iter().any(|t| t.is_some());
        self.stopping = false;
        if frames == 0 {
            self.tracks = [Some(track), None];
            self.fade_in = None;
            return Ok(());
        }
        if !busy {
            self.tracks = [None, Some(track)];
            self.fade_in = Some(1);
            self.fade_i = 0;
            self.fade_n = frames;
            return Ok(());
        }
        let slot = if self.tracks[1].is_none() {
            1
        } else if self.tracks[0].is_none() {
            0
        } else {
            let drop = match self.fade_in {
                Some(i) => 1 - i,
                None => 0,
            };
            self.tracks[drop] = None;
            drop
        };
        self.tracks[slot] = Some(track);
        self.fade_in = Some(slot);
        self.fade_i = 0;
        self.fade_n = frames;
        Ok(())
    }

    /// Fade the music bus to silence over `fade_seconds`, then drop the tracks.
    /// Zero stops at once.
    pub fn stop(&mut self, fade_seconds: f32) {
        if self.tracks.iter().all(|t| t.is_none()) {
            self.stopping = false;
            self.fade_in = None;
            return;
        }
        let frames = fade_len(fade_seconds, self.out_rate);
        if frames == 0 {
            self.tracks = [None, None];
            self.fade_in = None;
            self.stopping = false;
            return;
        }
        self.fade_in = None;
        self.stopping = true;
        self.stop_i = 0;
        self.stop_n = frames;
    }

    fn weight(&self, slot: usize) -> f32 {
        if self.tracks[slot].is_none() {
            return 0.0;
        }
        if self.stopping {
            if self.stop_n == 0 {
                return 0.0;
            }
            return (1.0 - self.stop_i as f32 / self.stop_n as f32).clamp(0.0, 1.0);
        }
        match self.fade_in {
            Some(incoming) => {
                let t = if self.fade_n == 0 {
                    1.0
                } else {
                    (self.fade_i as f32 / self.fade_n as f32).clamp(0.0, 1.0)
                };
                if slot == incoming {
                    t
                } else {
                    1.0 - t
                }
            }
            None => 1.0,
        }
    }

    fn pull_slot(&mut self, slot: usize) -> Option<(f32, f32)> {
        let pulled = self.tracks[slot].as_mut().map(|t| t.pull());
        match pulled {
            None => None,
            Some(Ok(Some(sample))) => Some(sample),
            Some(Ok(None)) => {
                self.tracks[slot] = None;
                if self.fade_in == Some(slot) {
                    self.fade_in = None;
                }
                None
            }
            Some(Err(e)) => {
                self.last_error = Some(e);
                self.tracks[slot] = None;
                if self.fade_in == Some(slot) {
                    self.fade_in = None;
                }
                None
            }
        }
    }

    fn advance_envelope(&mut self) {
        if self.stopping {
            self.stop_i = self.stop_i.saturating_add(1);
            if self.stop_i >= self.stop_n {
                self.tracks = [None, None];
                self.stopping = false;
                self.fade_in = None;
            }
            return;
        }
        let Some(incoming) = self.fade_in else { return };
        self.fade_i = self.fade_i.saturating_add(1);
        if self.fade_i >= self.fade_n {
            let outgoing = 1 - incoming;
            self.tracks[outgoing] = None;
            self.fade_in = None;
        }
    }

    /// Add this bus onto interleaved stereo `out`. `master` is a per-frame gain
    /// (the mixer's master bus); an empty slice means 1.
    pub fn render(&mut self, out: &mut [f32], master: &[f32]) {
        if self.paused && !self.stopping {
            return;
        }
        let frames = out.len() / 2;
        for f in 0..frames {
            if self.tracks.iter().all(|t| t.is_none()) {
                break;
            }
            if self.paused {
                // A stop fade still runs while playback is frozen.
                if !self.stopping {
                    break;
                }
            }
            let w0 = self.weight(0);
            let w1 = self.weight(1);
            let master_g = master.get(f).copied().unwrap_or(1.0);
            let vol = self.volume * master_g;
            let s0 = if self.paused { None } else { self.pull_slot(0) };
            let s1 = if self.paused { None } else { self.pull_slot(1) };
            if !self.paused {
                if let Some((l, r)) = s0 {
                    out[f * 2] += l * w0 * vol;
                    out[f * 2 + 1] += r * w0 * vol;
                }
                if let Some((l, r)) = s1 {
                    out[f * 2] += l * w1 * vol;
                    out[f * 2 + 1] += r * w1 * vol;
                }
            }
            self.advance_envelope();
        }
    }
}
