//! A GPU [`Device`] for the Seal render graph.
//!
//! It draws the same quads the software device draws, with the same blend
//! rules. A texture samples nearest or linear according to its description,
//! so a frame can be checked on the CPU and shown on the GPU. The default
//! colour path stores the bytes as they are, matching the software device.
//! A linear picture is encoded for display on upload, the same encode the
//! software sampler applies when it reads.

pub mod background_loader;
pub mod compute;
pub mod cull;
pub mod hdr_surface;
mod present;
pub mod texture_gpu;
pub use hdr_surface::{clamp_paper_white_nits, pick_surface_format, SurfacePick};
pub use present::{pick_format, Presenter};
pub use wgpu;

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use seal_render::{
    BlendMode, ColourSpace, Device, QuadBatch, TextureDesc, TextureFilter, TextureId, TextureWrap,
};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuInstance {
    corners: [f32; 8],
    uv01: [f32; 4],
    uv23: [f32; 4],
    color: [f32; 4],
}

struct Texture {
    desc: TextureDesc,
    dimension: wgpu::TextureDimension,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind: wgpu::BindGroup,
}

#[derive(Debug)]
pub enum GpuError {
    NoAdapter(String),
    NoDevice(String),
}

impl std::fmt::Display for GpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAdapter(e) => write!(f, "no GPU adapter: {e}"),
            Self::NoDevice(e) => write!(f, "GPU device failed: {e}"),
        }
    }
}

impl std::error::Error for GpuError {}

pub struct WgpuDevice {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter_name: String,
    pub adapter_features: wgpu::Features,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    linear_sampler: wgpu::Sampler,
    pipelines: HashMap<BlendMode, wgpu::RenderPipeline>,
    textures: Vec<Option<Texture>>,
    white: TextureId,
    quad_shader: wgpu::ShaderModule,
    pipeline_layout: wgpu::PipelineLayout,
    /// Samples per pixel for quad drawing (1 = off). See [`WgpuDevice::set_quality`].
    samples: u32,
    /// Anisotropic filtering clamp for linear pictures (1 = off).
    anisotropy: u16,
    /// Multisampled colour buffer per render target, resolved into the target after each pass.
    msaa_views: HashMap<u32, wgpu::TextureView>,
}

pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

impl WgpuDevice {
    /// Open the default GPU without a window (tests, offline rendering).
    pub fn headless() -> Result<Self, GpuError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .map_err(|e| GpuError::NoAdapter(e.to_string()))?;
        Self::from_adapter(&adapter)
    }

    pub fn from_adapter(adapter: &wgpu::Adapter) -> Result<Self, GpuError> {
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            // Culled indirect draws start each mesh at its own first instance.
            required_features: adapter.features() & wgpu::Features::INDIRECT_FIRST_INSTANCE,
            ..Default::default()
        }))
        .map_err(|e| GpuError::NoDevice(e.to_string()))?;
        Ok(Self::new(device, queue, adapter.get_info().name, adapter.features()))
    }

    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        adapter_name: String,
        adapter_features: wgpu::Features,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("quad.wgsl"));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("seal quad textures"),
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
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("seal quads"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("seal nearest"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("seal linear"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let pipelines = quad_pipelines(&device, &shader, &pipeline_layout, 1);
        let mut this = Self {
            device,
            queue,
            adapter_name,
            adapter_features,
            layout,
            sampler,
            linear_sampler,
            pipelines,
            textures: Vec::new(),
            white: TextureId(0),
            quad_shader: shader,
            pipeline_layout,
            samples: 1,
            anisotropy: 1,
            msaa_views: HashMap::new(),
        };
        this.white = this.create_texture(TextureDesc::rgba8(1, 1));
        this.upload(this.white, &[255; 4]);
        this
    }

    fn tex(&self, id: TextureId) -> &Texture {
        self.textures[id.0 as usize].as_ref().expect("live texture")
    }

    /// The view of a texture, for showing it in a window.
    pub fn view(&self, id: TextureId) -> &wgpu::TextureView {
        &self.tex(id).view
    }

    pub fn raw_texture(&self, id: TextureId) -> &wgpu::Texture {
        &self.tex(id).texture
    }

    /// Live textures, not counting the internal white one.
    pub fn live_textures(&self) -> usize {
        self.textures.iter().filter(|t| t.is_some()).count() - 1
    }

    pub fn supported_format_tokens(&self) -> Vec<&'static str> {
        texture_gpu::adapter_feature_names(self.adapter_features)
    }

    pub fn texture_layer_count(&self, id: TextureId) -> u32 {
        self.tex(id).desc.depth_or_layers()
    }

    pub fn texture_dimension(&self, id: TextureId) -> wgpu::TextureDimension {
        self.tex(id).dimension
    }

    /// Quality from the game's render settings: `msaa` above 1 draws quads with 4 samples
    /// per pixel (the count every GPU supports for this format); `anisotropy` (1-16) applies
    /// to linear pictures created after this call.
    pub fn set_quality(&mut self, msaa: u32, anisotropy: u16) {
        let samples = if msaa > 1 { 4 } else { 1 };
        if samples != self.samples {
            self.samples = samples;
            self.pipelines =
                quad_pipelines(&self.device, &self.quad_shader, &self.pipeline_layout, samples);
            self.msaa_views.clear();
        }
        self.anisotropy = anisotropy.clamp(1, 16);
    }

    pub fn samples(&self) -> u32 {
        self.samples
    }

    /// Where a pass draws: the target itself, or its multisampled buffer resolved into it.
    fn attachment(&mut self, target: TextureId) -> (wgpu::TextureView, Option<wgpu::TextureView>) {
        let direct = self.tex(target).view.clone();
        if self.samples <= 1 {
            return (direct, None);
        }
        let desc = self.tex(target).desc;
        let samples = self.samples;
        let device = &self.device;
        let view = self
            .msaa_views
            .entry(target.0)
            .or_insert_with(|| {
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("seal multisample"),
                        size: extent(desc),
                        mip_level_count: 1,
                        sample_count: samples,
                        dimension: wgpu::TextureDimension::D2,
                        format: FORMAT,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    })
                    .create_view(&Default::default())
            })
            .clone();
        (view, Some(direct))
    }
}

/// One quad pipeline per blend mode, drawing with `samples` samples per pixel.
fn quad_pipelines(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    pipeline_layout: &wgpu::PipelineLayout,
    samples: u32,
) -> HashMap<BlendMode, wgpu::RenderPipeline> {
    let blend_for = |mode: BlendMode| match mode {
        BlendMode::Opaque => None,
        BlendMode::Alpha => Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        }),
        BlendMode::Additive => Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        }),
    };
    let attributes = wgpu::vertex_attr_array![
        0 => Float32x4,
        1 => Float32x4,
        2 => Float32x4,
        3 => Float32x4,
        4 => Float32x4
    ];
    let mut pipelines = HashMap::new();
    for mode in [BlendMode::Opaque, BlendMode::Alpha, BlendMode::Additive] {
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("seal quads"),
            layout: Some(pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GpuInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &attributes,
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: blend_for(mode),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        pipelines.insert(mode, pipeline);
    }
    pipelines
}

fn extent(desc: TextureDesc) -> wgpu::Extent3d {
    texture_gpu::wgpu_extent(desc)
}

fn picture_sampler(desc: TextureDesc, anisotropy: u16) -> wgpu::SamplerDescriptor<'static> {
    let filter = if desc.filter == TextureFilter::Linear {
        wgpu::FilterMode::Linear
    } else {
        wgpu::FilterMode::Nearest
    };
    let mip = if desc.filter == TextureFilter::Linear {
        wgpu::MipmapFilterMode::Linear
    } else {
        wgpu::MipmapFilterMode::Nearest
    };
    let address = match desc.wrap {
        TextureWrap::Repeat => wgpu::AddressMode::Repeat,
        TextureWrap::Mirror => wgpu::AddressMode::MirrorRepeat,
        TextureWrap::Clamp => wgpu::AddressMode::ClampToEdge,
    };
    wgpu::SamplerDescriptor {
        label: Some("seal picture"),
        mag_filter: filter,
        min_filter: filter,
        mipmap_filter: mip,
        address_mode_u: address,
        address_mode_v: address,
        address_mode_w: address,
        // Anisotropic filtering needs every filter linear.
        anisotropy_clamp: if desc.filter == TextureFilter::Linear {
            anisotropy.clamp(1, 16)
        } else {
            1
        },
        ..Default::default()
    }
}

impl Device for WgpuDevice {
    fn create_texture(&mut self, desc: TextureDesc) -> TextureId {
        let dimension = texture_gpu::wgpu_dimension(desc);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("seal texture"),
            size: extent(desc),
            mip_level_count: if desc.mipmaps { 2 } else { 1 },
            sample_count: 1,
            dimension,
            format: FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = match desc.shape {
            seal_render::TextureShape::D2 => texture.create_view(&Default::default()),
            seal_render::TextureShape::D2Array { .. } | seal_render::TextureShape::Cube => texture
                .create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: 0,
                    array_layer_count: Some(1),
                    ..Default::default()
                }),
            seal_render::TextureShape::D3 { .. } => {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D3),
                    ..Default::default()
                })
            }
        };
        let extra = self.device.create_sampler(&picture_sampler(desc, self.anisotropy));
        // The two built-in samplers are the historical clamp samplers. Anything else
        // (wrap, mipmaps) uses a sampler built from the texture's descriptor.
        let sampler = match (desc.filter, desc.wrap, desc.mipmaps) {
            (TextureFilter::Nearest, TextureWrap::Clamp, false) => &self.sampler,
            (TextureFilter::Linear, TextureWrap::Clamp, false) if self.anisotropy <= 1 => {
                &self.linear_sampler
            }
            _ => &extra,
        };
        let bind_view = match desc.shape {
            seal_render::TextureShape::D3 { .. } => &self.tex(self.white).view,
            _ => &view,
        };
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("seal texture"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(bind_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        let entry = Some(Texture { desc, dimension, texture, view, bind });
        match self.textures.iter().position(Option::is_none) {
            Some(i) => {
                self.textures[i] = entry;
                TextureId(i as u32)
            }
            None => {
                self.textures.push(entry);
                TextureId(self.textures.len() as u32 - 1)
            }
        }
    }

    fn destroy_texture(&mut self, texture: TextureId) {
        // Ids are reused; a stale multisample buffer could have another size.
        self.msaa_views.remove(&texture.0);
        if let Some(t) = self.textures[texture.0 as usize].take() {
            t.texture.destroy();
        }
    }

    fn desc(&self, texture: TextureId) -> TextureDesc {
        self.tex(texture).desc
    }

    fn upload(&mut self, texture: TextureId, rgba: &[u8]) {
        let desc = self.tex(texture).desc;
        assert_eq!(
            rgba.len(),
            (desc.width * desc.height * 4) as usize,
            "upload size must match the texture"
        );
        let linear = desc.colour_space == ColourSpace::Linear;
        let encoded = linear.then(|| seal_render::picture::display_linear_rgba(rgba));
        let pixels = encoded.as_deref().unwrap_or(rgba);
        let t = self.tex(texture);
        self.queue.write_texture(
            t.texture.as_image_copy(),
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(desc.width * 4),
                rows_per_image: None,
            },
            extent(desc),
        );
        if desc.mipmaps {
            let (raw_mip, mw, mh) = seal_render::picture::box_mip(rgba, desc.width, desc.height);
            let encoded_mip = linear.then(|| seal_render::picture::display_linear_rgba(&raw_mip));
            let mip = encoded_mip.as_deref().unwrap_or(raw_mip.as_slice());
            let t = self.tex(texture);
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &t.texture,
                    mip_level: 1,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                mip,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(mw * 4),
                    rows_per_image: None,
                },
                wgpu::Extent3d { width: mw, height: mh, depth_or_array_layers: 1 },
            );
        }
    }

    fn clear(&mut self, target: TextureId, color: [f32; 4]) {
        // Round like the software device so both clear to the same bytes.
        let q = color.map(|c| ((c.clamp(0.0, 1.0) * 255.0).round() / 255.0) as f64);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let (view, resolve) = self.attachment(target);
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("seal clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: resolve.as_ref(),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: q[0],
                            g: q[1],
                            b: q[2],
                            a: q[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        self.queue.submit([encoder.finish()]);
    }

    fn draw_quads(&mut self, target: TextureId, batch: &QuadBatch) {
        if batch.instances.is_empty() {
            return;
        }
        let desc = self.tex(target).desc;
        let (w, h) = (desc.width as f32, desc.height as f32);
        let to_clip = |p: [f32; 2]| [p[0] / w * 2.0 - 1.0, 1.0 - p[1] / h * 2.0];
        let data: Vec<GpuInstance> = batch
            .instances
            .iter()
            .map(|i| {
                let c = i.corners.map(to_clip);
                let uvs = i.corner_uvs();
                GpuInstance {
                    corners: [
                        c[0][0], c[0][1], c[1][0], c[1][1], c[2][0], c[2][1], c[3][0], c[3][1],
                    ],
                    uv01: [uvs[0][0], uvs[0][1], uvs[1][0], uvs[1][1]],
                    uv23: [uvs[2][0], uvs[2][1], uvs[3][0], uvs[3][1]],
                    color: i.color,
                }
            })
            .collect();
        use wgpu::util::DeviceExt;
        let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("seal quads"),
            contents: bytemuck::cast_slice(&data),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let source = batch.texture.unwrap_or(self.white);
        let (view, resolve) = self.attachment(target);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("seal quads"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: resolve.as_ref(),
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.pipelines[&batch.blend]);
            pass.set_bind_group(0, &self.tex(source).bind, &[]);
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..6, 0..data.len() as u32);
        }
        self.queue.submit([encoder.finish()]);
    }

    fn read_back(&mut self, texture: TextureId) -> Vec<u8> {
        let t = self.tex(texture);
        let row = t.desc.width * 4;
        let padded =
            row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("seal read back"),
            size: (padded * t.desc.height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            t.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: None,
                },
            },
            extent(t.desc),
        );
        self.queue.submit([encoder.finish()]);
        buffer.map_async(wgpu::MapMode::Read, .., |r| r.expect("map read-back buffer"));
        self.device.poll(wgpu::PollType::wait_indefinitely()).expect("GPU finished");
        let mapped = buffer.get_mapped_range(..).expect("mapped read-back range");
        let mut out = Vec::with_capacity((row * t.desc.height) as usize);
        for y in 0..t.desc.height {
            let start = (y * padded) as usize;
            out.extend_from_slice(&mapped[start..start + row as usize]);
        }
        out
    }

    fn copy(&mut self, source: TextureId, target: TextureId) {
        let (s, t) = (self.tex(source), self.tex(target));
        assert_eq!(s.desc, t.desc, "copy needs matching textures");
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_texture(
            s.texture.as_image_copy(),
            t.texture.as_image_copy(),
            extent(s.desc),
        );
        self.queue.submit([encoder.finish()]);
    }
}

/// WGSL percent-closer shadow lookup (compute), twin of `seal_render::shadow::ShadowMap::lit`.
pub const SHADOW_WGSL: &str = include_str!("shadow.wgsl");
