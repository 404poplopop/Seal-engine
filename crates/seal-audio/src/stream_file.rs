//! Disk readers for streamed music: WAV, FLAC, OGG Vorbis and MP3.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::decode::{bad, detect_format, SoundFormat};
use crate::stream::WavReader;
use crate::AudioError;

const READ_FRAMES: usize = 1024;

pub struct AudioFileReader {
    inner: ReaderKind,
}

enum ReaderKind {
    Wav(WavReader),
    Flac(FlacReader),
    Ogg(OggReader),
    Mp3(Mp3Reader),
}

impl AudioFileReader {
    pub fn open(path: &Path) -> Result<Self, AudioError> {
        let mut file = File::open(path).map_err(|e| bad(e.to_string()))?;
        let mut peek = [0u8; 12];
        let read = file.read(&mut peek).map_err(|e| bad(e.to_string()))?;
        file.seek(SeekFrom::Start(0)).map_err(|e| bad(e.to_string()))?;
        let fmt = detect_format(Some(path), &peek[..read]);
        let inner = match fmt {
            SoundFormat::Wav => ReaderKind::Wav(WavReader::open(path)?),
            SoundFormat::Flac => ReaderKind::Flac(FlacReader::open(path)?),
            SoundFormat::Ogg => ReaderKind::Ogg(OggReader::open(path)?),
            SoundFormat::Mp3 => ReaderKind::Mp3(Mp3Reader::open(path)?),
        };
        Ok(Self { inner })
    }

    pub fn sample_rate(&self) -> u32 {
        match &self.inner {
            ReaderKind::Wav(r) => r.sample_rate(),
            ReaderKind::Flac(r) => r.sample_rate(),
            ReaderKind::Ogg(r) => r.sample_rate(),
            ReaderKind::Mp3(r) => r.sample_rate(),
        }
    }

    pub fn channels(&self) -> u16 {
        match &self.inner {
            ReaderKind::Wav(r) => r.channels(),
            ReaderKind::Flac(r) => r.channels(),
            ReaderKind::Ogg(r) => r.channels(),
            ReaderKind::Mp3(r) => r.channels(),
        }
    }

    pub fn frames(&self) -> u64 {
        match &self.inner {
            ReaderKind::Wav(r) => r.frames(),
            ReaderKind::Flac(r) => r.frames(),
            ReaderKind::Ogg(r) => r.frames(),
            ReaderKind::Mp3(r) => r.frames(),
        }
    }

    pub fn buffered_bytes(&self) -> usize {
        match &self.inner {
            ReaderKind::Wav(r) => r.buffered_bytes(),
            ReaderKind::Flac(r) => r.buffered_bytes(),
            ReaderKind::Ogg(r) => r.buffered_bytes(),
            ReaderKind::Mp3(r) => r.buffered_bytes(),
        }
    }

    pub fn seek_frame(&mut self, frame: u64) -> Result<(), AudioError> {
        match &mut self.inner {
            ReaderKind::Wav(r) => r.seek_frame(frame),
            ReaderKind::Flac(r) => r.seek_frame(frame),
            ReaderKind::Ogg(r) => r.seek_frame(frame),
            ReaderKind::Mp3(r) => r.seek_frame(frame),
        }
    }

    pub fn read_into(&mut self, frames: usize, out: &mut Vec<f32>) -> Result<(), AudioError> {
        match &mut self.inner {
            ReaderKind::Wav(r) => r.read_into(frames, out),
            ReaderKind::Flac(r) => r.read_into(frames, out),
            ReaderKind::Ogg(r) => r.read_into(frames, out),
            ReaderKind::Mp3(r) => r.read_into(frames, out),
        }
    }
}

struct FlacReader {
    path: PathBuf,
    sample_rate: u32,
    channels: u16,
    total_frames: u64,
    scale: f32,
    reader: claxon::FlacReader<File>,
    frame_pos: u64,
    pending: Vec<f32>,
    block_buf: Vec<i32>,
}

impl FlacReader {
    fn open(path: &Path) -> Result<Self, AudioError> {
        let reader =
            claxon::FlacReader::open(path).map_err(|e| bad(format!("FLAC: {e}")))?;
        let info = reader.streaminfo();
        let channels = info.channels as u16;
        if !(1..=2).contains(&channels) {
            return Err(bad("only mono and stereo are supported"));
        }
        let total = match info.samples {
            // STREAMINFO counts samples per channel already.
            Some(samples) if samples > 0 => samples,
            _ => count_flac_frames(path, channels)?,
        };
        if total == 0 {
            return Err(bad("no frames"));
        }
        let scale = 2f32.powi((info.bits_per_sample - 1) as i32);
        Ok(Self {
            path: path.to_path_buf(),
            sample_rate: info.sample_rate,
            channels,
            total_frames: total,
            scale,
            reader,
            frame_pos: 0,
            pending: Vec::new(),
            block_buf: Vec::new(),
        })
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn frames(&self) -> u64 {
        self.total_frames
    }

    fn buffered_bytes(&self) -> usize {
        self.pending.capacity() * std::mem::size_of::<f32>()
    }

    fn seek_frame(&mut self, frame: u64) -> Result<(), AudioError> {
        if frame > self.total_frames {
            return Err(bad("seek past the end"));
        }
        self.reader =
            claxon::FlacReader::open(&self.path).map_err(|e| bad(format!("FLAC: {e}")))?;
        self.frame_pos = 0;
        self.pending.clear();
        self.fill_until_frame(frame)?;
        Ok(())
    }

    fn fill_until_frame(&mut self, target: u64) -> Result<(), AudioError> {
        while self.frame_pos < target {
            let step = (target - self.frame_pos).min(READ_FRAMES as u64) as usize;
            let mut tmp = Vec::new();
            self.read_into(step, &mut tmp)?;
            if tmp.is_empty() {
                return Err(bad("truncated"));
            }
        }
        Ok(())
    }

    fn read_into(&mut self, frames: usize, out: &mut Vec<f32>) -> Result<(), AudioError> {
        let ch = self.channels as usize;
        let mut need = frames;
        while need > 0 {
            if self.pending.len() >= ch {
                let take = (self.pending.len() / ch).min(need);
                let n = take * ch;
                out.extend_from_slice(&self.pending[..n]);
                self.pending.drain(..n);
                self.frame_pos += take as u64;
                need -= take;
                continue;
            }
            // Whole blocks only: stopping a sample iterator part way drops the rest of its block.
            let buffer = std::mem::take(&mut self.block_buf);
            let block = self
                .reader
                .blocks()
                .read_next_or_eof(buffer)
                .map_err(|e| bad(format!("FLAC: {e}")))?;
            let mut block_samples = Vec::new();
            if let Some(block) = block {
                for i in 0..block.duration() {
                    for c in 0..ch as u32 {
                        block_samples.push(block.sample(c, i) as f32 / self.scale);
                    }
                }
                self.block_buf = block.into_buffer();
            }
            if block_samples.is_empty() {
                if need == frames {
                    return Err(bad("truncated"));
                }
                break;
            }
            self.pending.extend(block_samples);
        }
        Ok(())
    }
}

fn count_flac_frames(path: &Path, _channels: u16) -> Result<u64, AudioError> {
    let bytes = std::fs::read(path).map_err(|e| bad(e.to_string()))?;
    Ok(decode_flac(&bytes)?.frames() as u64)
}

fn decode_flac(bytes: &[u8]) -> Result<crate::Sound, AudioError> {
    crate::decode::decode_flac(bytes)
}

struct OggReader {
    sample_rate: u32,
    channels: u16,
    total_frames: u64,
    reader: lewton::inside_ogg::OggStreamReader<File>,
    frame_pos: u64,
    pending: Vec<f32>,
}

impl OggReader {
    fn open(path: &Path) -> Result<Self, AudioError> {
        let file = File::open(path).map_err(|e| bad(e.to_string()))?;
        let reader = lewton::inside_ogg::OggStreamReader::new(file)
            .map_err(|e| bad(format!("Vorbis: {e}")))?;
        let channels = reader.ident_hdr.audio_channels as u16;
        if !(1..=2).contains(&channels) {
            return Err(bad("only mono and stereo are supported"));
        }
        let sample_rate = reader.ident_hdr.audio_sample_rate;
        let total_frames = count_ogg_frames(path, channels)?;
        let file = File::open(path).map_err(|e| bad(e.to_string()))?;
        let reader = lewton::inside_ogg::OggStreamReader::new(file)
            .map_err(|e| bad(format!("Vorbis: {e}")))?;
        Ok(Self {
            sample_rate,
            channels,
            total_frames,
            reader,
            frame_pos: 0,
            pending: Vec::new(),
        })
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn frames(&self) -> u64 {
        self.total_frames
    }

    fn buffered_bytes(&self) -> usize {
        self.pending.capacity() * std::mem::size_of::<f32>()
    }

    fn seek_frame(&mut self, frame: u64) -> Result<(), AudioError> {
        if frame > self.total_frames {
            return Err(bad("seek past the end"));
        }
        self.reader
            .seek_absgp_pg(frame)
            .map_err(|e| bad(format!("Vorbis: {e}")))?;
        self.frame_pos = frame;
        self.pending.clear();
        Ok(())
    }

    fn read_into(&mut self, frames: usize, out: &mut Vec<f32>) -> Result<(), AudioError> {
        let ch = self.channels as usize;
        let mut need = frames;
        while need > 0 {
            if self.pending.len() >= ch {
                let take = (self.pending.len() / ch).min(need);
                let n = take * ch;
                out.extend_from_slice(&self.pending[..n]);
                self.pending.drain(..n);
                self.frame_pos += take as u64;
                need -= take;
                continue;
            }
            let packet = self
                .reader
                .read_dec_packet_itl()
                .map_err(|e| bad(format!("Vorbis: {e}")))?;
            let Some(packet) = packet else {
                if need == frames {
                    return Err(bad("truncated"));
                }
                break;
            };
            for s in packet {
                self.pending.push(s as f32 / 32768.0);
            }
        }
        Ok(())
    }
}

fn count_ogg_frames(path: &Path, channels: u16) -> Result<u64, AudioError> {
    let file = File::open(path).map_err(|e| bad(e.to_string()))?;
    let mut reader = lewton::inside_ogg::OggStreamReader::new(file)
        .map_err(|e| bad(format!("Vorbis: {e}")))?;
    let ch = channels as usize;
    let mut frames = 0u64;
    while let Some(packet) = reader
        .read_dec_packet_itl()
        .map_err(|e| bad(format!("Vorbis: {e}")))?
    {
        if ch == 0 {
            return Err(bad("unsupported channel count"));
        }
        frames += packet.len() as u64 / ch as u64;
    }
    if frames == 0 {
        return Err(bad("no frames"));
    }
    Ok(frames)
}

struct Mp3Reader {
    reader: nanomp3::Reader<std::io::BufReader<File>, f32>,
    sample_rate: u32,
    channels: u16,
    total_frames: u64,
    pcm: Vec<f32>,
}

impl Mp3Reader {
    fn open(path: &Path) -> Result<Self, AudioError> {
        let file = File::open(path).map_err(|e| bad(e.to_string()))?;
        let reader = nanomp3::Reader::<_, f32>::new(std::io::BufReader::new(file))
            .map_err(|e| bad(format!("MP3: {e}")))?;
        let channels = match reader.channels() {
            Some(c) => u16::from(c.num()),
            None => return Err(bad("MP3: no audio frames")),
        };
        let total_frames = reader.total_samples().unwrap_or(0);
        if total_frames == 0 {
            return Err(bad("MP3: no audio frames"));
        }
        Ok(Self { sample_rate: reader.sample_rate(), channels, total_frames, reader, pcm: Vec::new() })
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn frames(&self) -> u64 {
        self.total_frames
    }

    fn buffered_bytes(&self) -> usize {
        self.pcm.capacity() * std::mem::size_of::<f32>()
    }

    fn seek_frame(&mut self, frame: u64) -> Result<(), AudioError> {
        if frame > self.total_frames {
            return Err(bad("seek past the end"));
        }
        self.reader.seek(frame).map_err(|e| bad(format!("MP3: {e}")))
    }

    fn read_into(&mut self, frames: usize, out: &mut Vec<f32>) -> Result<(), AudioError> {
        let ch = self.channels as usize;
        let mut need = frames * ch;
        self.pcm.resize((READ_FRAMES * ch).max(ch), 0.0);
        while need > 0 {
            let want = need.min(self.pcm.len());
            let n = self.reader.read(&mut self.pcm[..want]).map_err(|e| bad(format!("MP3: {e}")))?;
            if n == 0 {
                if need == frames * ch {
                    return Err(bad("truncated"));
                }
                break;
            }
            out.extend_from_slice(&self.pcm[..n]);
            need -= n;
        }
        Ok(())
    }
}
