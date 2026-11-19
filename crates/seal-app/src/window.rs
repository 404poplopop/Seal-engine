//! Shows a game in a window: GPU drawing, keyboard and mouse input, sound.

use std::sync::Arc;
use std::time::{Duration, Instant};

use seal_input::{
    pen_from_event, write_pen, write_pointer, Button, InputRecording, PenEvent, PenSample, RawInput,
};
use seal_render_wgpu::{wgpu, Presenter, WgpuDevice};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, Force, MouseButton, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::{AppConfig, Game, Runner, STEP_RATE};

/// How a windowed run behaves.
#[derive(Clone, Debug, Default)]
pub struct RunOptions {
    /// Bindings that replace the game's defaults (the text of an `input.map` file).
    pub input_override: Option<String>,
    /// Save every step's input here when the window closes.
    pub record: Option<std::path::PathBuf>,
    /// Play this recording instead of reading the keyboard.
    pub replay: Option<InputRecording>,
    pub vsync: bool,
    /// Scale of the window over the game's frame size.
    pub scale: u32,
    pub mute: bool,
    /// Text of an export `window.txt`. Empty keeps the title, size, and vsync above.
    pub window_text: String,
}

/// The engine name for a key: `KeyA` is `key.a`, `ArrowLeft` is `key.left`,
/// `Digit1` is `key.1`, `ShiftLeft` is `key.shift_left`.
pub fn key_name(code: KeyCode) -> Option<Button> {
    let debug = format!("{code:?}");
    let base = if let Some(rest) = debug.strip_prefix("Key") {
        rest.to_string()
    } else if let Some(rest) = debug.strip_prefix("Digit") {
        rest.to_string()
    } else if let Some(rest) = debug.strip_prefix("Arrow") {
        rest.to_string()
    } else {
        debug
    };
    let mut name = String::new();
    for (i, c) in base.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            name.push('_');
        }
        name.push(c.to_ascii_lowercase());
    }
    Button::parse(&format!("key.{name}")).ok()
}

fn mouse_name(button: MouseButton) -> Option<Button> {
    let name = match button {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
        MouseButton::Back => "back",
        MouseButton::Forward => "forward",
        MouseButton::Other(_) => return None,
    };
    Button::parse(&format!("mouse.{name}")).ok()
}

struct Live<G: Game> {
    window: Arc<Window>,
    runner: Runner<G, WgpuDevice>,
    presenter: Presenter,
    #[cfg(feature = "audio-out")]
    _audio: Option<crate::audio_out::AudioOut>,
}

struct App<G: Game> {
    make: Option<Box<dyn FnOnce() -> G>>,
    config: AppConfig,
    options: RunOptions,
    live: Option<Live<G>>,
    raw: RawInput,
    cursor_at: Option<[f64; 2]>,
    cursor_delta: [f32; 2],
    raw_delta: [f32; 2],
    pen: PenSample,
    recording: InputRecording,
    last: Instant,
    pending: Duration,
    presented: Instant,
    error: Option<String>,
    launch: crate::window_config::RunnerConfig,
}

impl<G: Game> App<G> {
    fn open(&mut self, el: &ActiveEventLoop) -> Result<(), String> {
        let scale = self.options.scale.max(1);
        let launch = if self.options.window_text.trim().is_empty() {
            crate::window_config::RunnerConfig {
                title: self.config.title.clone(),
                width: self.config.width,
                height: self.config.height,
                vsync: self.options.vsync,
                ..crate::window_config::RunnerConfig::default()
            }
        } else {
            crate::window_config::parse(&self.options.window_text)?
        };
        self.launch = launch.clone();
        let (win_w, win_h) = if self.options.window_text.trim().is_empty() {
            (self.config.width * scale, self.config.height * scale)
        } else {
            (launch.width, launch.height)
        };
        let mut attrs = Window::default_attributes()
            .with_title(launch.title.clone())
            .with_inner_size(winit::dpi::PhysicalSize::new(win_w, win_h))
            .with_resizable(launch.resizable)
            .with_decorations(launch.mode == crate::window_config::WindowMode::Windowed);
        if launch.always_on_top {
            attrs = attrs.with_window_level(winit::window::WindowLevel::AlwaysOnTop);
        }
        if launch.mode == crate::window_config::WindowMode::Fullscreen {
            attrs = attrs.with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
        }
        let window = Arc::new(el.create_window(attrs).map_err(|e| e.to_string())?);
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).map_err(|e| e.to_string())?;
        let adapter = pollster_block(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|e| format!("no GPU adapter: {e}"))?;
        let mut device = WgpuDevice::from_adapter(&adapter).map_err(|e| e.to_string())?;
        let size = window.inner_size();
        let presenter = Presenter::new(
            &device,
            &adapter,
            surface,
            size.width,
            size.height,
            self.launch.vsync,
            self.launch.hdr_output,
            self.launch.paper_white_nits,
        );
        let make = self.make.take().expect("opened once");
        let game_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
            .unwrap_or_default();
        let render = crate::render_config::load_next_to_game(&game_dir);
        device.set_quality(render.msaa, render.anisotropy);
        let mut runner = Runner::new(
            make(),
            device,
            self.config.clone(),
            self.options.input_override.as_deref(),
            render,
        )?;
        runner.capture_audio = false;
        #[cfg(feature = "audio-out")]
        let audio = if self.options.mute {
            None
        } else {
            match crate::audio_out::AudioOut::start_default(runner.mixer_handle()) {
                Ok(a) => Some(a),
                Err(e) => {
                    eprintln!("no sound: {e}");
                    None
                }
            }
        };
        eprintln!("{}: GPU {}", self.config.title, runner.device.adapter_name);
        self.live = Some(Live {
            window,
            runner,
            presenter,
            #[cfg(feature = "audio-out")]
            _audio: audio,
        });
        self.last = Instant::now();
        Ok(())
    }

    /// Fold this frame's cursor, raw motion and pen into `raw`. Later steps
    /// in the same frame keep the pen and drop the motion, so it is not applied twice.
    fn stash_pointer(&mut self, first: bool) {
        if first {
            write_pointer(&mut self.raw, self.cursor_delta, self.raw_delta);
            self.cursor_delta = [0.0, 0.0];
            self.raw_delta = [0.0, 0.0];
        } else {
            write_pointer(&mut self.raw, [0.0, 0.0], [0.0, 0.0]);
        }
        write_pen(&mut self.raw, &self.pen);
    }

    fn frame(&mut self, el: &ActiveEventLoop) {
        if self.live.is_none() {
            return;
        }
        let now = Instant::now();
        self.pending += now - self.last;
        self.last = now;
        let step = Duration::from_secs(1) / STEP_RATE;
        // Never fall more than a few steps behind; after a stall, drop the backlog.
        if self.pending > step * 5 {
            self.pending = step * 5;
        }
        let mut first_motion = true;
        while self.pending >= step {
            self.pending -= step;
            let frame_index = self.live.as_ref().map(|live| live.runner.cx.frame).unwrap_or(0);
            let raw = if self.options.replay.is_some() {
                let Some(rec) = &self.options.replay else { unreachable!("replay was checked") };
                match rec.frame(frame_index as usize) {
                    Some(r) => r.clone(),
                    None => {
                        el.exit();
                        return;
                    }
                }
            } else {
                self.stash_pointer(first_motion);
                first_motion = false;
                self.raw.clone()
            };
            self.recording.push(&raw);
            let Some(live) = self.live.as_mut() else { return };
            if let Err(e) = live.runner.step(&raw) {
                self.error = Some(e);
                el.exit();
                return;
            }
            if live.runner.cx.quit {
                el.exit();
                return;
            }
        }
        let wait =
            crate::window_config::frame_wait(self.launch.target_fps, self.presented.elapsed());
        if self.launch.target_fps > 0 && !wait.is_zero() {
            el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + wait));
            return;
        }
        let Some(live) = self.live.as_mut() else { return };
        live.runner.render();
        live.presenter.present(&live.runner.device, live.runner.target);
        self.presented = Instant::now();
        if self.launch.target_fps == 0 {
            live.window.request_redraw();
        } else {
            let frame = Duration::from_secs(1) / self.launch.target_fps;
            el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + frame));
        }
    }
}

fn pollster_block<F: std::future::Future>(f: F) -> F::Output {
    // A tiny blocking executor; GPU setup futures resolve at once on native backends.
    use std::task::{Context, Poll, Wake, Waker};
    struct Thread(std::thread::Thread);
    impl Wake for Thread {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Thread(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut f = std::pin::pin!(f);
    loop {
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::park(),
        }
    }
}

impl<G: Game> ApplicationHandler for App<G> {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.live.is_none() && self.make.is_some() {
            if let Err(e) = self.open(el) {
                self.error = Some(e);
                el.exit();
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(size) => {
                if let Some(live) = self.live.as_mut() {
                    live.presenter.resize(&live.runner.device, size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if let Some(b) = key_name(code) {
                        match event.state {
                            ElementState::Pressed => self.raw.press(b),
                            ElementState::Released => self.raw.release(&b),
                        }
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(b) = mouse_name(button) {
                    match state {
                        ElementState::Pressed => self.raw.press(b),
                        ElementState::Released => self.raw.release(&b),
                    }
                }
            }
            WindowEvent::Focused(false) => {
                self.raw = RawInput::default();
                self.cursor_delta = [0.0, 0.0];
                self.raw_delta = [0.0, 0.0];
                self.pen = PenSample::default();
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(prev) = self.cursor_at {
                    self.cursor_delta[0] += (position.x - prev[0]) as f32;
                    self.cursor_delta[1] += (position.y - prev[1]) as f32;
                }
                self.cursor_at = Some([position.x, position.y]);
            }
            WindowEvent::Touch(touch) => {
                let (pressure, altitude) = match touch.force {
                    Some(Force::Normalized(force)) => (force as f32, None),
                    Some(Force::Calibrated { force, max_possible_force, altitude_angle }) => {
                        let pressure = if max_possible_force > 0.0 {
                            (force / max_possible_force) as f32
                        } else {
                            0.0
                        };
                        (pressure, altitude_angle.map(|angle| angle as f32))
                    }
                    None => (0.0, None),
                };
                if touch.force.is_some() {
                    let down = matches!(touch.phase, TouchPhase::Started | TouchPhase::Moved);
                    self.pen = pen_from_event(PenEvent {
                        pressure,
                        altitude,
                        azimuth: None,
                        eraser: false,
                        down,
                    });
                }
            }
            WindowEvent::RedrawRequested => self.frame(el),
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event {
            self.raw_delta[0] += delta.0 as f32;
            self.raw_delta[1] += delta.1 as f32;
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if self.launch.target_fps > 0 {
            if let Some(live) = &self.live {
                live.window.request_redraw();
            }
        }
    }
}

/// Open a window and play the game until it quits or the window closes.
pub fn run<G: Game + 'static>(
    config: AppConfig,
    options: RunOptions,
    make: impl FnOnce() -> G + 'static,
) -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let record = options.record.clone();
    let mut app = App {
        make: Some(Box::new(make)),
        config,
        options,
        live: None,
        raw: RawInput::default(),
        cursor_at: None,
        cursor_delta: [0.0, 0.0],
        raw_delta: [0.0, 0.0],
        pen: PenSample::default(),
        recording: InputRecording::new(),
        last: Instant::now(),
        pending: Duration::ZERO,
        presented: Instant::now(),
        error: None,
        launch: crate::window_config::RunnerConfig::default(),
    };
    event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    if let Some(path) = record {
        std::fs::write(&path, app.recording.to_text())
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    match app.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
