//! Plays the mixer through the default sound device.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use seal_audio::backend::{resolve_host_id, OutputSettings};
use seal_audio::Mixer;

use crate::SAMPLE_RATE;

/// Keeps sound playing while alive.
pub struct AudioOut {
    _stream: cpal::Stream,
    pub device_rate: u32,
    pub device_channels: u16,
    pub backend: String,
}

/// Fills device buffers from the stereo mixer, converting rate and channel count.
struct Feed {
    mixer: Arc<Mutex<Mixer>>,
    channels: usize,
    /// Device frames per second.
    device_rate: f64,
    /// Mixer frames per device frame.
    ratio: f64,
    phase: f64,
    /// The two mixer frames the device position sits between.
    prev: [f32; 2],
    next: [f32; 2],
    block: Vec<f32>,
    used: usize,
}

impl Feed {
    fn pull(&mut self) -> [f32; 2] {
        if self.used * 2 >= self.block.len() {
            let rate = self.mixer.lock().unwrap_or_else(|e| e.into_inner()).sample_rate.max(1);
            self.ratio = f64::from(rate) / self.device_rate;
            self.block.resize(512 * 2, 0.0);
            self.mixer.lock().unwrap_or_else(|e| e.into_inner()).mix(&mut self.block);
            self.used = 0;
        }
        let f = [self.block[self.used * 2], self.block[self.used * 2 + 1]];
        self.used += 1;
        f
    }

    fn fill<T: cpal::SizedSample + cpal::FromSample<f32>>(&mut self, out: &mut [T]) {
        for frame in out.chunks_mut(self.channels) {
            self.phase += self.ratio;
            while self.phase >= 1.0 {
                self.phase -= 1.0;
                self.prev = self.next;
                self.next = self.pull();
            }
            let t = self.phase as f32;
            let l = self.prev[0] + (self.next[0] - self.prev[0]) * t;
            let r = self.prev[1] + (self.next[1] - self.prev[1]) * t;
            for (c, s) in frame.iter_mut().enumerate() {
                let v = match (self.channels, c) {
                    (1, _) => (l + r) * 0.5,
                    (_, 0) => l,
                    (_, 1) => r,
                    _ => 0.0,
                };
                *s = T::from_sample(v.clamp(-1.0, 1.0));
            }
        }
    }
}

fn pick_host(name: &str) -> cpal::Host {
    use cpal::platform::available_hosts;
    let id = match name {
        "wasapi" => available_hosts()
            .into_iter()
            .find(|id| id.name().to_ascii_lowercase().contains("wasapi")),
        "coreaudio" => available_hosts()
            .into_iter()
            .find(|id| id.name().to_ascii_lowercase().contains("coreaudio")),
        "alsa" => {
            available_hosts().into_iter().find(|id| id.name().to_ascii_lowercase().contains("alsa"))
        }
        "pulse" => available_hosts()
            .into_iter()
            .find(|id| id.name().to_ascii_lowercase().contains("pulse")),
        _ => None,
    };
    if let Some(id) = id {
        cpal::host_from_id(id).unwrap_or_else(|_| cpal::default_host())
    } else {
        cpal::default_host()
    }
}

impl AudioOut {
    /// Start playback, or explain why there is no sound.
    pub fn start(mixer: Arc<Mutex<Mixer>>, settings: &OutputSettings) -> Result<Self, String> {
        let backend = resolve_host_id(&settings.backend).to_string();
        let host = pick_host(&backend);
        let device = host.default_output_device().ok_or("no sound output device")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let mut config = supported.config();
        // Project Settings > Audio: a sample rate the device offers for the same format and
        // channel count, and a buffer size inside the device's range; otherwise the defaults.
        if let Some(rate) = settings.sample_rate {
            let offered = device.supported_output_configs().ok().is_some_and(|mut ranges| {
                ranges.any(|range| {
                    range.channels() == config.channels
                        && range.sample_format() == supported.sample_format()
                        && range.min_sample_rate() <= rate
                        && rate <= range.max_sample_rate()
                })
            });
            if offered {
                config.sample_rate = rate;
            } else {
                eprintln!(
                    "sound output: {rate} Hz is not offered by the device; using {}",
                    config.sample_rate
                );
            }
        }
        if let Some(frames) = settings.buffer_frames {
            match supported.buffer_size() {
                cpal::SupportedBufferSize::Range { min, max } if (*min..=*max).contains(&frames) => {
                    config.buffer_size = cpal::BufferSize::Fixed(frames);
                }
                _ => eprintln!("sound output: a {frames}-frame buffer is not offered; using the device default"),
            }
        }
        let channels = config.channels as usize;
        let device_rate = config.sample_rate;
        let mut feed = Feed {
            mixer,
            channels,
            device_rate: device_rate as f64,
            ratio: SAMPLE_RATE as f64 / device_rate as f64,
            phase: 0.0,
            prev: [0.0; 2],
            next: [0.0; 2],
            block: Vec::new(),
            used: 0,
        };
        let err = |e| eprintln!("sound output: {e}");
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => device.build_output_stream(
                config,
                move |o: &mut [f32], _: &_| feed.fill(o),
                err,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_output_stream(
                config,
                move |o: &mut [i16], _: &_| feed.fill(o),
                err,
                None,
            ),
            cpal::SampleFormat::U16 => device.build_output_stream(
                config,
                move |o: &mut [u16], _: &_| feed.fill(o),
                err,
                None,
            ),
            cpal::SampleFormat::I32 => device.build_output_stream(
                config,
                move |o: &mut [i32], _: &_| feed.fill(o),
                err,
                None,
            ),
            other => return Err(format!("unsupported sound format {other:?}")),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self { _stream: stream, device_rate, device_channels: channels as u16, backend })
    }

    /// Start with defaults (tests and headless tools).
    pub fn start_default(mixer: Arc<Mutex<Mixer>>) -> Result<Self, String> {
        Self::start(mixer, &OutputSettings::default())
    }
}
