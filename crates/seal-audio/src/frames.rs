//! Convert mixer frames to the device rate and channel count.
//!
//! Equal rates copy frames directly. Any other pair uses linear interpolation
//! and keeps unused source frames for the next block, so a later block
//! continues the same stream. Non-finite samples become silence.

use crate::Mixer;

fn clean(sample: f32) -> f32 {
    if sample.is_finite() {
        sample.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

fn clean_pair(frame: [f32; 2]) -> [f32; 2] {
    [clean(frame[0]), clean(frame[1])]
}

fn write_frame(dest: &mut [f32], channels: usize, frame: [f32; 2]) {
    let left = clean(frame[0]);
    let right = clean(frame[1]);
    for (channel, slot) in dest.iter_mut().enumerate() {
        *slot = match (channels, channel) {
            (1, _) => clean((left + right) * 0.5),
            (_, 0) => left,
            (_, 1) => right,
            _ => 0.0,
        };
    }
}

/// Stateful rate and channel conversion.
#[derive(Clone, Debug)]
pub struct FrameConverter {
    mixer_rate: u32,
    device_rate: u32,
    channels: usize,
    primed: bool,
    position: f64,
    current: [f32; 2],
    next: [f32; 2],
}

impl FrameConverter {
    pub fn new(mixer_rate: u32, device_rate: u32, channels: usize) -> Self {
        Self {
            mixer_rate: mixer_rate.max(1),
            device_rate: device_rate.max(1),
            channels,
            primed: false,
            position: 0.0,
            current: [0.0; 2],
            next: [0.0; 2],
        }
    }

    pub fn mixer_rate(&self) -> u32 {
        self.mixer_rate
    }

    pub fn device_rate(&self) -> u32 {
        self.device_rate
    }

    /// Fill `out` with interleaved device frames. `pull` returns the next mixer frame.
    /// `None` is silence. A short tail that is not a whole frame is left untouched.
    pub fn fill(&mut self, out: &mut [f32], mut pull: impl FnMut() -> Option<[f32; 2]>) {
        if self.channels == 0 || out.is_empty() {
            return;
        }
        if self.mixer_rate == self.device_rate {
            for frame in out.chunks_exact_mut(self.channels) {
                let sample = pull().map(clean_pair).unwrap_or([0.0; 2]);
                write_frame(frame, self.channels, sample);
            }
            return;
        }
        let step = f64::from(self.mixer_rate) / f64::from(self.device_rate);
        for frame in out.chunks_exact_mut(self.channels) {
            if !self.primed {
                self.current = pull().map(clean_pair).unwrap_or([0.0; 2]);
                self.next = pull().map(clean_pair).unwrap_or(self.current);
                self.primed = true;
                self.position = 0.0;
            }
            let fraction = self.position as f32;
            let sample = [
                self.current[0] + (self.next[0] - self.current[0]) * fraction,
                self.current[1] + (self.next[1] - self.current[1]) * fraction,
            ];
            write_frame(frame, self.channels, sample);
            self.position += step;
            while self.position >= 1.0 {
                self.position -= 1.0;
                self.current = self.next;
                self.next = pull().map(clean_pair).unwrap_or([0.0; 2]);
            }
        }
    }
}

/// Mixes the mixer into device-rate stereo, carrying leftover source frames forward.
pub struct MixerFeed {
    device_rate: u32,
    converter: FrameConverter,
    stash: Vec<f32>,
    stash_at: usize,
}

impl MixerFeed {
    pub fn new(mixer_rate: u32, device_rate: u32) -> Self {
        let mixer_rate = mixer_rate.max(1);
        let device_rate = device_rate.max(1);
        Self {
            device_rate,
            converter: FrameConverter::new(mixer_rate, device_rate, 2),
            stash: Vec::new(),
            stash_at: 0,
        }
    }

    pub fn device_rate(&self) -> u32 {
        self.device_rate
    }

    pub fn set_rates(&mut self, mixer_rate: u32, device_rate: u32) {
        let mixer_rate = mixer_rate.max(1);
        let device_rate = device_rate.max(1);
        if mixer_rate == self.converter.mixer_rate() && device_rate == self.device_rate {
            return;
        }
        self.device_rate = device_rate;
        self.converter = FrameConverter::new(mixer_rate, device_rate, 2);
        self.stash.clear();
        self.stash_at = 0;
    }

    /// Render `device_frames` stereo frames at the feed's device rate.
    pub fn render(&mut self, mixer: &mut Mixer, device_frames: usize) -> Vec<[f32; 2]> {
        if device_frames == 0 {
            return Vec::new();
        }
        self.set_rates(mixer.sample_rate, self.device_rate);
        if self.converter.mixer_rate() == self.device_rate {
            let mut mixed = vec![0.0; device_frames * 2];
            mixer.mix(&mut mixed);
            return mixed.chunks_exact(2).map(|frame| [clean(frame[0]), clean(frame[1])]).collect();
        }
        let step = f64::from(self.converter.mixer_rate()) / f64::from(self.device_rate);
        let need = (device_frames as f64 * step).ceil() as usize + 4;
        self.ensure(mixer, need);
        let mut interleaved = vec![0.0; device_frames * 2];
        let stash = &self.stash;
        let mut at = self.stash_at;
        self.converter.fill(&mut interleaved, || {
            if at * 2 + 1 >= stash.len() {
                None
            } else {
                let frame = [stash[at * 2], stash[at * 2 + 1]];
                at += 1;
                Some(frame)
            }
        });
        self.stash_at = at;
        if self.stash_at > 0 {
            self.stash.drain(..self.stash_at * 2);
            self.stash_at = 0;
        }
        interleaved.chunks_exact(2).map(|frame| [frame[0], frame[1]]).collect()
    }

    fn ensure(&mut self, mixer: &mut Mixer, frames: usize) {
        let have = self.stash.len() / 2 - self.stash_at;
        if have >= frames {
            return;
        }
        let mut block = vec![0.0; (frames - have) * 2];
        mixer.mix(&mut block);
        if self.stash_at > 0 {
            self.stash.drain(..self.stash_at * 2);
            self.stash_at = 0;
        }
        self.stash.extend_from_slice(&block);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PlayParams, Sound};

    #[test]
    fn matching_rates_copy_frames_and_fold_channels() {
        let mut converter = FrameConverter::new(48_000, 48_000, 2);
        let mut out = vec![0.0; 4];
        let mut cursor = 0;
        let source = [[0.5, -0.25], [1.5, -1.5]];
        converter.fill(&mut out, || {
            let frame = source.get(cursor).copied();
            cursor += 1;
            frame
        });
        assert_eq!(out, vec![0.5, -0.25, 1.0, -1.0]);

        let mut mono = FrameConverter::new(48_000, 48_000, 1);
        let mut folded = [0.0; 1];
        mono.fill(&mut folded, || Some([0.5, -0.5]));
        assert!(folded[0].abs() < 1.0e-6);

        let mut wide = FrameConverter::new(48_000, 48_000, 3);
        let mut extra = [1.0; 3];
        wide.fill(&mut extra, || Some([0.2, -0.4]));
        assert_eq!(extra, [0.2, -0.4, 0.0]);
    }

    #[test]
    fn half_speed_playback_interpolates() {
        let mut converter = FrameConverter::new(2, 4, 2);
        let mut out = vec![0.0; 8];
        let mut cursor = 0usize;
        let source = [[0.0, 0.0], [1.0, -1.0], [0.0, 0.0]];
        converter.fill(&mut out, || {
            let frame = source.get(cursor).copied();
            cursor += 1;
            frame
        });
        assert!((out[0] - 0.0).abs() < 1.0e-5);
        assert!((out[2] - 0.5).abs() < 1.0e-5);
        assert!((out[3] + 0.5).abs() < 1.0e-5);
        assert!((out[4] - 1.0).abs() < 1.0e-5);
    }

    #[test]
    fn damaged_samples_become_silence() {
        let mut converter = FrameConverter::new(48_000, 48_000, 2);
        let mut out = [0.0; 6];
        let mut cursor = 0;
        let source = [[f32::NAN, 0.25], [f32::INFINITY, f32::NEG_INFINITY], [0.1, -0.1]];
        converter.fill(&mut out, || {
            let frame = source.get(cursor).copied();
            cursor += 1;
            frame
        });
        assert_eq!(&out[..4], &[0.0, 0.25, 0.0, 0.0]);
        assert_eq!(&out[4..], &[0.1, -0.1]);

        let mut idle = FrameConverter::new(0, 0, 0);
        let mut untouched = [3.0; 2];
        idle.fill(&mut untouched, || panic!("no pull"));
        assert_eq!(untouched, [3.0, 3.0]);

        let mut empty = FrameConverter::new(8_000, 8_000, 2);
        let mut silence = [1.0; 2];
        empty.fill(&mut silence, || None);
        assert_eq!(silence, [0.0, 0.0]);
    }

    #[test]
    fn mixer_feed_renders_a_tone_and_an_empty_block() {
        let mut mixer = Mixer::new(8_000);
        let tone = mixer.add_sound(Sound::tone(8_000, 440.0, 0.05, 0.4));
        mixer.play(tone, PlayParams::default()).unwrap();
        let mut feed = MixerFeed::new(8_000, 8_000);
        assert!(feed.render(&mut mixer, 0).is_empty());
        let frames = feed.render(&mut mixer, 128);
        assert_eq!(frames.len(), 128);
        assert!(frames.iter().all(|frame| frame[0].is_finite() && frame[1].is_finite()));
        let peak = frames.iter().map(|frame| frame[0].abs()).fold(0.0f32, f32::max);
        assert!(peak > 0.05, "peak {peak}");

        let mut fast = MixerFeed::new(8_000, 16_000);
        let up = fast.render(&mut mixer, 64);
        assert_eq!(up.len(), 64);
        assert!(up.iter().all(|frame| frame[0].is_finite() && frame[1].is_finite()));
    }
}
