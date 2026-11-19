//! Game window drawn on an HDR surface.
//!
//! Used for an exported game and for Play in New Window when HDR output is on.
//! If the device does not start, or the display offers no HDR format, the caller
//! opens the ordinary window instead.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::time::Instant;

use seal_editor_core::hdr_window::{
    choose_runner, output_space, sdr_gain, window_plan, HdrBuffer, OutputSpace, RunnerChoice,
};
use seal_editor_core::{window_game, LaunchOptions};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Fullscreen, Icon, Window, WindowId, WindowLevel};

use crate::Shell;

/// Start Play in New Window as its own process so it can own an HDR surface.
pub fn spawn_play_window(project: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|err| err.to_string())?;
    let name = project.file_name().and_then(|name| name.to_str()).unwrap_or("game");
    let data = std::env::temp_dir().join(format!("seal-hdr-play-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&data).map_err(|err| err.to_string())?;
    std::process::Command::new(exe)
        .arg(project)
        .arg("--game")
        .arg("--user-data")
        .arg(&data)
        .spawn()
        .map(|_| ())
        .map_err(|err| err.to_string())
}

/// Open the HDR game window. `Err` means the ordinary window should be used.
pub fn try_run(layout: &Path, launch: &LaunchOptions) -> Result<(), String> {
    let project = launch.project.clone().ok_or_else(|| "no project".to_string())?;
    let game = window_game::load_for_launch(&project);
    if !game.hdr_output {
        return Err("hdr off".into());
    }
    let mut app = HdrApp {
        layout: layout.to_path_buf(),
        launch: launch.clone(),
        project,
        fallback: None,
        opened: false,
        shell: None,
        window: None,
        state: None,
        ctx: egui::Context::default(),
        gpu: None,
        viewport_info: egui::ViewportInfo::default(),
        frame_at: Instant::now(),
        space: OutputSpace::Hdr,
    };
    let event_loop = EventLoop::new().map_err(|err| err.to_string())?;
    event_loop.run_app(&mut app).map_err(|err| err.to_string())?;
    seal_ui::gpu_viewport_wgpu::set_active(false);
    seal_ui::gpu_viewport_wgpu::set_hdr(false);
    seal_ui::gpu_viewport::set_enabled(false);
    match app.fallback.take() {
        Some(reason) => Err(reason),
        None => Ok(()),
    }
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: egui_wgpu::Renderer,
    depth: wgpu::TextureView,
    scene: wgpu::Texture,
    scene_view: wgpu::TextureView,
    composite: wgpu::RenderPipeline,
    composite_layout: wgpu::BindGroupLayout,
    composite_sampler: wgpu::Sampler,
    tone: wgpu::Buffer,
}

struct HdrApp {
    layout: PathBuf,
    launch: LaunchOptions,
    project: PathBuf,
    fallback: Option<String>,
    opened: bool,
    shell: Option<Shell>,
    window: Option<Arc<Window>>,
    state: Option<egui_winit::State>,
    ctx: egui::Context,
    gpu: Option<Gpu>,
    viewport_info: egui::ViewportInfo,
    frame_at: Instant,
    space: OutputSpace,
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    struct Thread(std::thread::Thread);
    impl Wake for Thread {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Thread(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::park(),
        }
    }
}

fn hdr_buffers(formats: &[wgpu::TextureFormat]) -> Vec<HdrBuffer> {
    formats
        .iter()
        .copied()
        .filter_map(|format| match format {
            wgpu::TextureFormat::Rgba16Float => Some(HdrBuffer::Rgba16Float),
            wgpu::TextureFormat::Rgb10a2Unorm => Some(HdrBuffer::Rgb10a2),
            _ => None,
        })
        .collect()
}

fn texture_format(buffer: HdrBuffer) -> wgpu::TextureFormat {
    match buffer {
        HdrBuffer::Rgba16Float => wgpu::TextureFormat::Rgba16Float,
        HdrBuffer::Rgb10a2 => wgpu::TextureFormat::Rgb10a2Unorm,
    }
}

fn targets(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::TextureView, wgpu::Texture, wgpu::TextureView) {
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("seal game depth"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: seal_ui::gpu_viewport_wgpu::DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let scene = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("seal game scene"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: seal_ui::gpu_viewport_wgpu::SCENE_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let scene_view = scene.create_view(&Default::default());
    (depth.create_view(&Default::default()), scene, scene_view)
}

fn composite_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout, wgpu::Sampler) {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("seal frame composite"),
        source: wgpu::ShaderSource::Wgsl(seal_ui::gpu_viewport_wgpu::COMPOSITE_WGSL.into()),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("seal frame composite"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: std::num::NonZeroU64::new(16),
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("seal frame composite"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("seal frame composite"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("seal frame composite"),
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });
    (pipeline, layout, sampler)
}

impl HdrApp {
    fn open(&mut self, el: &ActiveEventLoop) -> Result<(), String> {
        let game = window_game::load_for_launch(&self.project);
        let plan = window_plan(&game);
        let mut attrs = Window::default_attributes()
            .with_title(plan.title.clone())
            .with_inner_size(LogicalSize::new(plan.width as f32, plan.height as f32))
            .with_resizable(plan.resizable)
            .with_decorations(plan.decorations)
            .with_visible(false)
            .with_window_level(if plan.always_on_top {
                WindowLevel::AlwaysOnTop
            } else {
                WindowLevel::Normal
            });
        if plan.fullscreen {
            attrs = attrs.with_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        if !game.icon.is_empty() {
            let path = self.project.join(&game.icon);
            if let Ok((width, height, rgba)) = window_game::read_icon_rgba(&path) {
                if let Ok(icon) = Icon::from_rgba(rgba, width, height) {
                    attrs = attrs.with_window_icon(Some(icon));
                }
            }
        }
        let window = Arc::new(el.create_window(attrs).map_err(|err| err.to_string())?);
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).map_err(|err| {
            seal_log::warn!("graphics device did not start: {err}");
            "device failed".to_string()
        })?;
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|err| {
            seal_log::warn!("graphics device did not start: {err}");
            "device failed".to_string()
        })?;
        let caps = surface.get_capabilities(&adapter);
        let format = match choose_runner(true, &hdr_buffers(&caps.formats), true) {
            RunnerChoice::Hdr { format } => texture_format(format),
            RunnerChoice::Standard { reason } => return Err(reason.into()),
        };
        let extended = format == wgpu::TextureFormat::Rgba16Float
            && caps.color_spaces(format).contains(wgpu::SurfaceColorSpaces::EXTENDED_SRGB_LINEAR);
        let color_space = if extended {
            wgpu::SurfaceColorSpace::ExtendedSrgbLinear
        } else {
            wgpu::SurfaceColorSpace::Auto
        };
        self.space = output_space(true, extended);
        let present =
            if plan.vsync { wgpu::PresentMode::AutoVsync } else { wgpu::PresentMode::AutoNoVsync };
        let present = if caps.present_modes.contains(&present) {
            present
        } else {
            caps.present_modes.first().copied().unwrap_or(wgpu::PresentMode::Fifo)
        };
        let alpha = caps.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto);
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: present,
            desired_maximum_frame_latency: 2,
            alpha_mode: alpha,
            view_formats: vec![],
            color_space,
        };
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .map_err(|err| {
                seal_log::warn!("graphics device did not start: {err}");
                "device failed".to_string()
            })?;
        let configured = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            surface.configure(&device, &config);
        }));
        if configured.is_err() {
            seal_log::warn!("HDR surface could not be configured");
            return Err("device failed".into());
        }
        let renderer = egui_wgpu::Renderer::new(
            &device,
            seal_ui::gpu_viewport_wgpu::SCENE_FORMAT,
            egui_wgpu::RendererOptions {
                msaa_samples: 1,
                depth_stencil_format: Some(seal_ui::gpu_viewport_wgpu::DEPTH_FORMAT),
                dithering: false,
                predictable_texture_filtering: false,
            },
        );
        let (depth, scene, scene_view) = targets(&device, config.width, config.height);
        let (composite, composite_layout, composite_sampler) = composite_pipeline(&device, format);
        let tone = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("seal frame tone"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let state = egui_winit::State::new(
            self.ctx.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            window.theme(),
            None,
        );
        let cpu_only = std::env::var_os("SEAL_CPU_VIEWPORT").is_some();
        seal_ui::gpu_viewport::set_enabled(!cpu_only);
        seal_ui::gpu_viewport_wgpu::set_active(!cpu_only);
        seal_ui::gpu_viewport_wgpu::set_hdr(true);
        window.set_visible(true);
        seal_log::info!("HDR window {}x{} extended {}", config.width, config.height, extended);
        self.shell = Some(Shell::new(self.layout.clone(), &self.launch));
        self.window = Some(window);
        self.state = Some(state);
        self.gpu = Some(Gpu {
            device,
            queue,
            surface,
            config,
            renderer,
            depth,
            scene,
            scene_view,
            composite,
            composite_layout,
            composite_sampler,
            tone,
        });
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };
        gpu.config.width = width;
        gpu.config.height = height;
        gpu.surface.configure(&gpu.device, &gpu.config);
        let (depth, scene, scene_view) = targets(&gpu.device, width, height);
        gpu.depth = depth;
        gpu.scene = scene;
        gpu.scene_view = scene_view;
    }

    fn paint(&mut self, el: &ActiveEventLoop) {
        self.frame_at = Instant::now();
        seal_ui::gpu_mesh_pass::begin_frame();
        let Some(window) = self.window.clone() else {
            return;
        };
        let raw = {
            let Some(state) = self.state.as_mut() else {
                return;
            };
            let mut raw = state.take_egui_input(&window);
            egui_winit::update_viewport_info(
                raw.viewports.entry(egui::ViewportId::ROOT).or_default(),
                &self.ctx,
                &window,
                false,
            );
            raw
        };
        let mut shell = self.shell.take();
        let full = self.ctx.run_ui(raw, |ui| {
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
                if let Some(shell) = shell.as_mut() {
                    shell.paint_packaged_game(ui);
                }
            });
        });
        let paper = shell
            .as_ref()
            .map(|shell| window_game::from_session(&shell.session).paper_white_nits)
            .unwrap_or(203.0);
        let fps = shell
            .as_ref()
            .map(|shell| window_game::from_session(&shell.session).target_fps)
            .unwrap_or(0);
        self.shell = shell;
        if let Some(state) = self.state.as_mut() {
            state.handle_platform_output(&window, full.platform_output);
        }
        let mut actions = Vec::new();
        if let Some(viewport) = full.viewport_output.get(&egui::ViewportId::ROOT) {
            egui_winit::process_viewport_commands(
                &self.ctx,
                &mut self.viewport_info,
                viewport.commands.clone(),
                &window,
                &mut actions,
            );
        }
        let primitives = self.ctx.tessellate(full.shapes, full.pixels_per_point);
        let mut textures = full.textures_delta;
        self.draw(&window, &primitives, &mut textures, full.pixels_per_point, paper);
        let wait = window_game::frame_wait(fps, self.frame_at.elapsed());
        if fps == 0 || wait.is_zero() {
            el.set_control_flow(ControlFlow::Poll);
        } else {
            el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + wait));
        }
    }

    fn draw(
        &mut self,
        window: &Window,
        primitives: &[egui::epaint::ClippedPrimitive],
        textures: &mut egui::epaint::textures::TexturesDelta,
        pixels_per_point: f32,
        paper: f32,
    ) {
        let Some(gpu) = self.gpu.as_mut() else {
            textures.set.clear();
            textures.free.clear();
            return;
        };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            textures.set.clear();
            textures.free.clear();
            return;
        }
        if gpu.config.width != size.width || gpu.config.height != size.height {
            gpu.config.width = size.width;
            gpu.config.height = size.height;
            gpu.surface.configure(&gpu.device, &gpu.config);
            let (depth, scene, scene_view) = targets(&gpu.device, size.width, size.height);
            gpu.depth = depth;
            gpu.scene = scene;
            gpu.scene_view = scene_view;
        }
        let gain = sdr_gain(self.space, paper);
        let mut tone_bytes = [0u8; 16];
        tone_bytes[..4].copy_from_slice(&gain.to_le_bytes());
        gpu.queue.write_buffer(&gpu.tone, 0, &tone_bytes);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [gpu.config.width, gpu.config.height],
            pixels_per_point,
        };
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        for (id, deltas) in textures.set.drain() {
            for delta in deltas {
                gpu.renderer.update_texture(&gpu.device, &gpu.queue, id, &delta);
            }
        }
        for id in seal_ui::gpu_sprite_wgpu::pending_sprite_textures() {
            if let Some(texture) = gpu.renderer.texture(&id).and_then(|tex| tex.texture.as_ref()) {
                let view = texture.create_view(&Default::default());
                seal_ui::gpu_sprite_wgpu::install_sprite_view(
                    seal_ui::sprite_blend_texture_key(id),
                    view,
                );
            }
        }
        let user =
            gpu.renderer.update_buffers(&gpu.device, &gpu.queue, &mut encoder, primitives, &screen);
        let frame = match gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            _ => {
                gpu.surface.configure(&gpu.device, &gpu.config);
                textures.free.clear();
                return;
            }
        };
        let swap = frame.texture.create_view(&Default::default());
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("seal game"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &gpu.scene_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &gpu.depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Discard,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            gpu.renderer.render(&mut pass, primitives, &screen);
        }
        let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("seal frame composite"),
            layout: &gpu.composite_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&gpu.scene_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&gpu.composite_sampler),
                },
                wgpu::BindGroupEntry { binding: 2, resource: gpu.tone.as_entire_binding() },
            ],
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("seal frame composite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &swap,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&gpu.composite);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        gpu.queue.submit(user.into_iter().chain(std::iter::once(encoder.finish())));
        gpu.queue.present(frame);
        for id in textures.free.drain() {
            gpu.renderer.free_texture(&id);
        }
    }
}

impl ApplicationHandler for HdrApp {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.opened {
            return;
        }
        self.opened = true;
        if let Err(err) = self.open(el) {
            self.fallback = Some(err);
            el.exit();
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if self.window.is_none() {
            return;
        }
        if let WindowEvent::Resized(size) = &event {
            self.resize(size.width, size.height);
        }
        if let WindowEvent::RedrawRequested = &event {
            self.paint(el);
            return;
        }
        if let (Some(state), Some(window)) = (self.state.as_mut(), self.window.as_ref()) {
            let _ = state.on_window_event(window, &event);
        }
        if let WindowEvent::CloseRequested = event {
            if let Some(shell) = self.shell.as_mut() {
                let _ = shell.flush_exit();
            }
            el.exit();
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
