//! Showing a finished frame in a window.

use bytemuck::{Pod, Zeroable};
use seal_render::TextureId;
use wgpu::util::DeviceExt;

use crate::WgpuDevice;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ToneUniform {
    gain: f32,
    _pad: [f32; 3],
}

/// Copies a frame texture onto a window surface, scaling it to fit.
pub struct Presenter {
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub hdr_output: bool,
    pub paper_white_nits: f32,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    tone: wgpu::Buffer,
}

/// Prefer a plain (non-sRGB) format so the window shows the same bytes the
/// frame holds, exactly as the software device would.
pub fn pick_format(formats: &[wgpu::TextureFormat]) -> wgpu::TextureFormat {
    formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(formats[0])
}

impl Presenter {
    pub fn new(
        device: &WgpuDevice,
        adapter: &wgpu::Adapter,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
        vsync: bool,
        hdr_output: bool,
        paper_white_nits: f32,
    ) -> Self {
        let caps = surface.get_capabilities(adapter);
        let pick = crate::hdr_surface::pick_surface_format(&caps.formats, hdr_output);
        let format = pick.format;
        let hdr_output = pick.hdr;
        let paper_white_nits = crate::hdr_surface::clamp_paper_white_nits(paper_white_nits);
        let present_mode =
            if vsync { wgpu::PresentMode::AutoVsync } else { wgpu::PresentMode::AutoNoVsync };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device.device, &config);
        let d = &device.device;
        let shader = d.create_shader_module(wgpu::include_wgsl!("present.wgsl"));
        let layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("seal present"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let gain = if hdr_output {
            paper_white_nits / 203.0
        } else {
            1.0
        };
        let tone = d.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("seal present tone"),
            contents: bytemuck::bytes_of(&ToneUniform { gain, _pad: [0.0; 3] }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let pipeline_layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("seal present"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("seal present"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
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
        let sampler = d.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("seal present"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Self {
            surface,
            config,
            hdr_output,
            paper_white_nits,
            pipeline,
            layout,
            sampler,
            tone,
        }
    }

    pub fn resize(&mut self, device: &WgpuDevice, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&device.device, &self.config);
    }

    /// Show `frame`. Returns false when the surface was lost and has been reconfigured.
    pub fn present(&mut self, device: &WgpuDevice, frame: TextureId) -> bool {
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            _ => {
                self.surface.configure(&device.device, &self.config);
                return false;
            }
        };
        let view = output.texture.create_view(&Default::default());
        let bind = device.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("seal present"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(device.view(frame)),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.tone.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("seal present"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        device.queue.submit([encoder.finish()]);
        device.queue.present(output);
        true
    }
}
