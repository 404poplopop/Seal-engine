//! Runs a Seal 2D game.
//!
//! A game implements [`Game`]. The [`Runner`] steps it at a fixed rate:
//! input, then the game's update, then physics, then the game's
//! after-physics hook, then sound. Every step is deterministic, so a run
//! with the same recorded input gives the same world, frames and sound on
//! every machine. [`window::run`] shows a game in a window with a GPU.

#![allow(unknown_lints, clippy::chunks_exact_to_as_chunks)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use seal_audio::{Mixer, Sound, SoundId};
use seal_ecs::World;
use seal_input::{ActionState, InputMap, RawInput};
use seal_physics2d::Physics2D;
use seal_render::picture::PictureSampling;
use seal_render::{Device, TextureDesc, TextureId};
use seal_render2d::{FrameStats, Renderer2D};

#[cfg(feature = "audio-out")]
pub mod audio_out;
#[cfg(feature = "window")]
pub mod window;
pub mod render_config;
pub mod window_config;

pub use seal_audio as audio;
pub use seal_ecs as ecs;
pub use seal_input as input;
pub use seal_physics2d as physics2d;
pub use seal_render as render;
pub use seal_render2d as render2d;

/// Steps per second.
pub const STEP_RATE: u32 = 60;
/// Output sample rate; 800 frames per step.
pub const SAMPLE_RATE: u32 = 48_000;

/// Fixed settings for a game.
#[derive(Clone, Debug)]
pub struct AppConfig {
    pub title: String,
    /// Size of the frame the game draws, in pixels. The window scales it.
    pub width: u32,
    pub height: u32,
    /// Default input bindings, in `InputMap` text. A player's `input.map` file overrides it.
    pub input: String,
}

/// Everything a game can touch during a step.
pub struct Ctx {
    pub world: World,
    pub physics: Physics2D,
    pub map: InputMap,
    pub actions: ActionState,
    mixer: Arc<Mutex<Mixer>>,
    sounds: HashMap<String, SoundId>,
    images: Vec<(String, u32, u32, Vec<u8>, PictureSampling)>,
    image_meshes: HashMap<String, Vec<[f32; 2]>>,
    /// Longest picture side when importing files.
    pub texture_max_size: u32,
    /// Steps run so far.
    pub frame: u64,
    /// Seconds per step.
    pub dt: f32,
    /// Set to end the game.
    pub quit: bool,
}

impl Ctx {
    /// The sound mixer, locked. Keep the guard short; the sound thread waits on it.
    pub fn mixer(&self) -> MutexGuard<'_, Mixer> {
        self.mixer.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn add_sound(&mut self, name: &str, sound: Sound) -> SoundId {
        let id = self.mixer().add_sound(sound);
        self.sounds.insert(name.into(), id);
        id
    }

    pub fn sound(&self, name: &str) -> Option<SoundId> {
        self.sounds.get(name).copied()
    }

    /// Play a named sound with default settings and the given pan.
    pub fn play(&self, name: &str, gain: f32, pan: f32) {
        if let Some(id) = self.sound(name) {
            let params =
                seal_audio::PlayParams { gain, pan: pan.clamp(-1.0, 1.0), ..Default::default() };
            let _ = self.mixer().play(id, params);
        }
    }

    /// Make an RGBA8 image available to sprites under `name` from the next frame on.
    pub fn load_image(&mut self, name: &str, width: u32, height: u32, rgba: Vec<u8>) {
        assert_eq!(rgba.len(), (width * height * 4) as usize, "image {name}: size does not match");
        self.image_meshes.remove(name);
        self.images.push((name.into(), width, height, rgba, PictureSampling::default()));
    }

    /// Decode a picture file with the shared loader and sample it with `sampling`.
    pub fn load_picture(
        &mut self,
        name: &str,
        path: &std::path::Path,
        sampling: PictureSampling,
    ) -> Result<(), String> {
        let picture = seal_render::picture::decode_path(path).map_err(|e| e.to_string())?;
        let cap = if self.texture_max_size == 0 {
            picture.width.max(picture.height)
        } else {
            self.texture_max_size
        };
        let (rgba, width, height) =
            seal_render::picture::downscale(&picture.rgba, picture.width, picture.height, cap);
        if let Some(mesh) = picture.mesh.clone() {
            self.image_meshes.insert(name.into(), mesh);
        } else {
            self.image_meshes.remove(name);
        }
        self.images.push((name.into(), width, height, rgba, sampling));
        Ok(())
    }
}

/// A game.
pub trait Game {
    /// Called once, before the first step.
    fn start(&mut self, cx: &mut Ctx);
    /// Called every step after input is read and before physics.
    fn update(&mut self, cx: &mut Ctx);
    /// Called every step after physics, with this step's contacts in `cx.physics.contacts`.
    fn after_physics(&mut self, _cx: &mut Ctx) {}
}

/// Steps a game and draws it on a device.
pub struct Runner<G, D> {
    pub game: G,
    pub cx: Ctx,
    pub device: D,
    pub renderer: Renderer2D,
    pub target: TextureId,
    pub config: AppConfig,
    pub render: render_config::GameRenderConfig,
    /// When true, each step mixes its own sound into `audio` (headless runs).
    pub capture_audio: bool,
    pub audio: Vec<f32>,
}

impl<G: Game, D: Device> Runner<G, D> {
    pub fn new(
        mut game: G,
        mut device: D,
        config: AppConfig,
        input_override: Option<&str>,
        render: render_config::GameRenderConfig,
    ) -> Result<Self, String> {
        let mut world = World::new();
        world.register_core();
        seal_render2d::register_2d(&mut world);
        if render.glow {
            world.insert_resource(seal_render2d::Glow {
                enabled: true,
                threshold: 0.85,
                intensity: 1.25,
                radius: 6.0,
            });
        }
        seal_physics2d::register_physics2d(&mut world);
        let map = InputMap::parse(input_override.unwrap_or(&config.input))
            .map_err(|e| format!("input map: {e:?}"))?;
        let mut cx = Ctx {
            world,
            physics: Physics2D::new(),
            map,
            actions: ActionState::default(),
            mixer: Arc::new(Mutex::new(Mixer::new(SAMPLE_RATE))),
            sounds: HashMap::new(),
            images: Vec::new(),
            image_meshes: HashMap::new(),
            texture_max_size: render.texture_max_size,
            frame: 0,
            dt: 1.0 / STEP_RATE as f32,
            quit: false,
        };
        game.start(&mut cx);
        // Render scale: the frame is drawn at a percent of the game size; the window stretches it.
        let scale = render.render_scale.clamp(25, 200) as f32 / 100.0;
        let target = device.create_texture(TextureDesc::rgba8(
            ((config.width as f32 * scale).round() as u32).max(1),
            ((config.height as f32 * scale).round() as u32).max(1),
        ));
        let mut runner = Self {
            game,
            cx,
            device,
            renderer: {
                let mut renderer = Renderer2D::new();
                renderer.pixel_scale = scale;
                renderer
            },
            target,
            config,
            render,
            capture_audio: true,
            audio: Vec::new(),
        };
        runner.upload_images();
        Ok(runner)
    }

    /// The mixer, shared with a sound output thread.
    pub fn mixer_handle(&self) -> Arc<Mutex<Mixer>> {
        self.cx.mixer.clone()
    }

    fn upload_images(&mut self) {
        for (name, w, h, rgba, sampling) in self.cx.images.drain(..) {
            self.renderer.load_image_sampled(&mut self.device, &name, w, h, &rgba, sampling);
            if !self.cx.image_meshes.contains_key(&name) {
                self.renderer.set_mesh(&name, Vec::new());
            }
        }
        for (name, mesh) in self.cx.image_meshes.drain() {
            self.renderer.set_mesh(&name, mesh);
        }
    }

    /// One fixed step with this input.
    pub fn step(&mut self, raw: &RawInput) -> Result<(), String> {
        let _zone = seal_trace::zone!(seal_trace::Channel::CPU, "app.step");
        let cx = &mut self.cx;
        cx.actions.update(&cx.map, raw);
        self.game.update(cx);
        cx.physics.step(&mut cx.world, cx.dt).map_err(|e| e.to_string())?;
        self.game.after_physics(cx);
        cx.frame += 1;
        if self.capture_audio {
            let frames = (SAMPLE_RATE / STEP_RATE) as usize;
            let start = self.audio.len();
            self.audio.resize(start + frames * 2, 0.0);
            self.cx.mixer().mix(&mut self.audio[start..]);
        }
        seal_trace::frame_mark();
        Ok(())
    }

    /// Draw the current world into the frame texture.
    pub fn render(&mut self) -> FrameStats {
        self.upload_images();
        self.renderer.render(&mut self.device, &mut self.cx.world, self.target)
    }

    pub fn read_frame(&mut self) -> Vec<u8> {
        self.device.read_back(self.target)
    }
}

/// FNV-1a, for golden checks of frames, sound and saved worlds.
pub fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ *b as u64).wrapping_mul(0x0100_0000_01b3))
}

/// Write an RGBA8 frame as a binary PPM (no alpha), for quick viewing.
pub fn frame_to_ppm(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = format!("P6\n{width} {height}\n255\n").into_bytes();
    for px in rgba.chunks_exact(4) {
        out.extend_from_slice(&px[..3]);
    }
    out
}
