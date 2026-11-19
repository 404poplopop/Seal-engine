//! Lit mesh, sky cubemap and texture preview on the HDR window.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use egui::Rect;
use wgpu::util::DeviceExt as _;

use crate::gpu_textures::{load_cpu_texture, SampleKind};
use crate::gpu_viewport::{pack_shadow, vertex_data, AlbedoBatch, GpuFrame, ShadowTexture};
use crate::gpu_wgpu_choice::{
    anisotropy_clamp, upload_cpu_texture, upload_white, CompressionSupport, TexShape, UploadedTexture,
};
use crate::viewport_mesh::LitTriangle;

pub const SKY_WGSL: &str = include_str!("sky_wgpu.wgsl");
pub const PREVIEW_WGSL: &str = include_str!("preview_wgpu.wgsl");
pub const STRETCH_WGSL: &str = include_str!("scale_stretch.wgsl");

const OWNER_SKY: u8 = 1;
const OWNER_MESH: u8 = 2;

static FRAME: AtomicU64 = AtomicU64::new(1);

/// Call once per displayed frame before the callbacks are prepared.
pub fn begin_frame() {
    FRAME.fetch_add(1, Ordering::Relaxed);
}

/// Smaller scene image for `percent`, or `None` when the scene stays full size.
pub fn offscreen_size(width: u32, height: u32, percent: u32) -> Option<(u32, u32)> {
    let width = width.max(1);
    let height = height.max(1);
    let size = crate::gpu_viewport::scaled_size(width, height, percent);
    if size == (width, height) {
        None
    } else {
        Some(size)
    }
}

fn viewport_px(rect: Rect, pixels_per_point: f32) -> (u32, u32) {
    let ppp = if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
        pixels_per_point
    } else {
        1.0
    };
    let width = (rect.width() * ppp).round().max(1.0) as u32;
    let height = (rect.height() * ppp).round().max(1.0) as u32;
    (width, height)
}

const BASE_FLOATS: usize = 22;
const EXT_FLOATS: usize = 23;

pub const MESH_ATTRS: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
    0 => Float32x3,
    1 => Float32x4,
    2 => Float32x4,
    3 => Float32x4,
    4 => Float32x4,
    5 => Float32x3,
    6 => Float32,
];

pub(crate) fn with_light(tris: &[LitTriangle], rect: Rect) -> (Vec<f32>, Vec<AlbedoBatch>) {
    let (data, batches) = vertex_data(tris, rect);
    let mut order: Vec<usize> = (0..tris.len()).collect();
    order.sort_by(|&a, &b| {
        (&tris[a].tex_path, tris[a].tex_kind, tris[a].tex_layer, tris[a].tex_slice.to_bits()).cmp(&(
            &tris[b].tex_path,
            tris[b].tex_kind,
            tris[b].tex_layer,
            tris[b].tex_slice.to_bits(),
        ))
    });
    let mut ext = Vec::with_capacity(data.len() / BASE_FLOATS * EXT_FLOATS);
    let mut chunks = data.chunks(BASE_FLOATS);
    for index in &order {
        let light = tris[*index].light.max(0.0);
        for _ in 0..3 {
            let Some(chunk) = chunks.next() else { break };
            ext.extend_from_slice(chunk);
            ext.push(light);
        }
    }
    (ext, batches)
}

struct MeshGpu {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    shadow_sampler: wgpu::Sampler,
    support: CompressionSupport,
    white: HashMap<u8, UploadedTexture>,
    albedo: HashMap<String, UploadedTexture>,
    samplers: HashMap<u16, wgpu::Sampler>,
}

struct BatchDraw {
    bind: wgpu::BindGroup,
    first: u32,
    count: u32,
}

struct Prepared {
    vertices: wgpu::Buffer,
    batches: Vec<BatchDraw>,
    _keep: Vec<wgpu::Texture>,
    /// Scene was drawn into a smaller image; paint stretches it.
    scaled: bool,
}

struct MeshCallback {
    rect: Rect,
    data: Arc<Vec<f32>>,
    batches: Arc<Vec<AlbedoBatch>>,
    shadow: Option<ShadowTexture>,
    frame: GpuFrame,
    hdr: bool,
    prepared: Mutex<Option<Prepared>>,
}

pub fn paint_callback(
    rect: Rect,
    tris: &[LitTriangle],
    shadow: Option<&seal_editor_core::shadow::ShadowMap>,
    frame: GpuFrame,
) -> egui::PaintCallback {
    let (data, batches) = with_light(tris, rect);
    let shadow = shadow.map(|map| ShadowTexture { size: map.size, texels: pack_shadow(map.depths()) });
    egui_wgpu::Callback::new_paint_callback(
        rect,
        MeshCallback {
            rect,
            data: Arc::new(data),
            batches: Arc::new(batches),
            shadow,
            frame,
            hdr: crate::gpu_viewport_wgpu::hdr(),
            prepared: Mutex::new(None),
        },
    )
}

fn f32_bytes(values: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * 4);
    for value in values {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

fn depth_state(write: bool, compare: wgpu::CompareFunction) -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: crate::gpu_viewport_wgpu::DEPTH_FORMAT,
        depth_write_enabled: Some(write),
        depth_compare: Some(compare),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    }
}

fn color_target(blend: Option<wgpu::BlendState>) -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format: crate::gpu_viewport_wgpu::SCENE_FORMAT,
        blend,
        write_mask: wgpu::ColorWrites::ALL,
    }
}

impl MeshGpu {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("seal lit mesh"),
            source: wgpu::ShaderSource::Wgsl(crate::gpu_viewport_wgpu::MESH_WGSL.into()),
        });
        let tex = |binding, view| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: view,
                multisampled: false,
            },
            count: None,
        };
        let samp = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("seal lit mesh"),
            entries: &[
                tex(0, wgpu::TextureViewDimension::D2),
                samp(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: std::num::NonZeroU64::new(32),
                    },
                    count: None,
                },
                tex(3, wgpu::TextureViewDimension::D2),
                samp(4),
                tex(5, wgpu::TextureViewDimension::D2Array),
                tex(6, wgpu::TextureViewDimension::D3),
                tex(7, wgpu::TextureViewDimension::Cube),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("seal lit mesh"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("seal lit mesh"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: (EXT_FLOATS * 4) as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &MESH_ATTRS,
                })],
            },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(depth_state(true, wgpu::CompareFunction::LessEqual)),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(color_target(None))],
            }),
            multiview_mask: None,
            cache: None,
        });
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("seal shadow"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let support = CompressionSupport::from_features(device.features());
        let mut white = HashMap::new();
        for shape in [TexShape::D2, TexShape::D2Array, TexShape::D3, TexShape::Cube] {
            white.insert(shape as u8, upload_white(device, queue, shape));
        }
        Self {
            pipeline,
            layout,
            shadow_sampler,
            support,
            white,
            albedo: HashMap::new(),
            samplers: HashMap::new(),
        }
    }

    fn sampler(&mut self, device: &wgpu::Device, anisotropy: u32) -> &wgpu::Sampler {
        let clamp = anisotropy_clamp(anisotropy);
        self.samplers.entry(clamp).or_insert_with(|| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("seal albedo"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                address_mode_w: wgpu::AddressMode::Repeat,
                anisotropy_clamp: clamp,
                ..Default::default()
            })
        })
    }

    fn fresh_view(&self, shape: TexShape) -> wgpu::TextureView {
        let image = self.white.get(&(shape as u8)).expect("white");
        image.texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(crate::gpu_wgpu_choice::view_dimension(shape)),
            ..Default::default()
        })
    }

    fn material(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &GpuFrame,
        batch: &AlbedoBatch,
    ) -> (wgpu::TextureView, f32, TexShape, f32) {
        if batch.path.is_empty() {
            let shape = crate::gpu_wgpu_choice::tex_shape(SampleKind::D2);
            return (self.fresh_view(shape), 1.0, shape, 0.0);
        }
        if !self.albedo.contains_key(&batch.path) {
            if let Ok(cpu) = load_cpu_texture(frame.project.as_deref(), &batch.path) {
                let uploaded = upload_cpu_texture(device, queue, &cpu, self.support);
                self.albedo.insert(batch.path.clone(), uploaded);
            }
        }
        if let Some(found) = self.albedo.get(&batch.path) {
            let shape = found.shape;
            let slices = found.slices;
            let view = found.texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(crate::gpu_wgpu_choice::view_dimension(shape)),
                ..Default::default()
            });
            (view, slices, shape, batch.kind.code() as f32)
        } else {
            let shape = crate::gpu_wgpu_choice::tex_shape(batch.kind);
            (self.fresh_view(shape), 1.0, shape, 0.0)
        }
    }
}

impl egui_wgpu::CallbackTrait for MeshCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if !resources.contains::<MeshGpu>() {
            resources.insert(MeshGpu::new(device, queue));
        }
        let Some(gpu) = resources.get_mut::<MeshGpu>() else {
            return Vec::new();
        };
        let (size, texels) = match &self.shadow {
            Some(map) if map.size > 0 && map.texels.len() == map.size * map.size * 4 => {
                (map.size as u32, map.texels.clone())
            }
            _ => (1, vec![255, 255, 255, 255]),
        };
        let row = size.saturating_mul(4);
        let stride = row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let mut upload = vec![0u8; stride as usize * size.max(1) as usize];
        for y in 0..size.max(1) as usize {
            let src = y * row as usize;
            let dst = y * stride as usize;
            if src + row as usize <= texels.len() {
                upload[dst..dst + row as usize].copy_from_slice(&texels[src..src + row as usize]);
            }
        }
        let shadow = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("seal lit mesh shadow"),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &shadow,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &upload,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(size.max(1)),
            },
            wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        );
        let shadow_view = shadow.create_view(&Default::default());
        let has = if self.shadow.as_ref().is_some_and(|map| map.size > 0) { 1.0 } else { 0.0 };
        let hdr = if self.hdr { 1.0 } else { 0.0 };
        let mut batches = Vec::new();
        let keep = vec![shadow];
        let clamp = anisotropy_clamp(self.frame.anisotropy);
        let _ = gpu.sampler(device, self.frame.anisotropy);
        for batch in self.batches.iter() {
            let (material, slices, shape, kind) = gpu.material(device, queue, &self.frame, batch);
            let params = f32_bytes(&[1.0 / size as f32, has, hdr, kind, slices.max(1.0), 0.0, 0.0, 0.0]);
            let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("seal lit mesh params"),
                contents: &params,
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let d2 = gpu.fresh_view(TexShape::D2);
            let arr = gpu.fresh_view(TexShape::D2Array);
            let vol = gpu.fresh_view(TexShape::D3);
            let cube = gpu.fresh_view(TexShape::Cube);
            let (d2v, arrv, volv, cubev): (
                &wgpu::TextureView,
                &wgpu::TextureView,
                &wgpu::TextureView,
                &wgpu::TextureView,
            ) = match shape {
                TexShape::D2Array => (&d2, &material, &vol, &cube),
                TexShape::D3 => (&d2, &arr, &material, &cube),
                TexShape::Cube => (&d2, &arr, &vol, &material),
                TexShape::D2 => (&material, &arr, &vol, &cube),
            };
            let albedo_sampler = gpu.samplers.get(&clamp).expect("sampler");
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("seal lit mesh"),
                layout: &gpu.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&shadow_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&gpu.shadow_sampler),
                    },
                    wgpu::BindGroupEntry { binding: 2, resource: uniform.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(d2v) },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(albedo_sampler),
                    },
                    wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(arrv) },
                    wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(volv) },
                    wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(cubev) },
                ],
            });
            batches.push(BatchDraw { bind, first: batch.first as u32, count: batch.count as u32 });
        }
        let vertices = if self.data.is_empty() {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("seal lit mesh empty"),
                size: (EXT_FLOATS * 4) as u64,
                usage: wgpu::BufferUsages::VERTEX,
                mapped_at_creation: false,
            })
        } else {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("seal lit mesh"),
                contents: &f32_bytes(&self.data),
                usage: wgpu::BufferUsages::VERTEX,
            })
        };
        let _ = shadow_view;
        let (vw, vh) = viewport_px(self.rect, screen.pixels_per_point);
        let scaled = offscreen_size(vw, vh, self.frame.render_scale);
        if let Some(size) = scaled {
            record_mesh(device, encoder, size, gpu, &vertices, &batches);
        }
        *self.prepared.lock().unwrap_or_else(|err| err.into_inner()) =
            Some(Prepared { vertices, batches, _keep: keep, scaled: scaled.is_some() });
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::epaint::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(gpu) = resources.get::<MeshGpu>() else { return };
        let guard = self.prepared.lock().unwrap_or_else(|err| err.into_inner());
        let Some(prepared) = guard.as_ref() else { return };
        if prepared.scaled {
            if scale_owner() == OWNER_MESH {
                stretch(pass);
            }
            return;
        }
        pass.set_pipeline(&gpu.pipeline);
        pass.set_vertex_buffer(0, prepared.vertices.slice(..));
        for batch in &prepared.batches {
            if batch.count == 0 {
                continue;
            }
            pass.set_bind_group(0, &batch.bind, &[]);
            pass.draw(batch.first..batch.first + batch.count, 0..1);
        }
    }
}

struct SkyGpu {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    cache: HashMap<String, UploadedTexture>,
    support: CompressionSupport,
}

enum SkyPrep {
    Direct(wgpu::BindGroup),
    Scaled,
}

struct SkyCallback {
    rect: Rect,
    frame: GpuFrame,
    prepared: Mutex<Option<SkyPrep>>,
}

pub fn sky_callback(rect: Rect, frame: GpuFrame) -> egui::PaintCallback {
    egui_wgpu::Callback::new_paint_callback(
        rect,
        SkyCallback { rect, frame, prepared: Mutex::new(None) },
    )
}

impl egui_wgpu::CallbackTrait for SkyCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if self.frame.sky.is_empty() {
            return Vec::new();
        }
        if !resources.contains::<SkyGpu>() {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("seal sky"),
                source: wgpu::ShaderSource::Wgsl(SKY_WGSL.into()),
            });
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("seal sky"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::Cube,
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
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
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
                label: Some("seal sky"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("seal sky"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: Some(depth_state(false, wgpu::CompareFunction::Always)),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(color_target(None))],
                }),
                multiview_mask: None,
                cache: None,
            });
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("seal sky"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
            resources.insert(SkyGpu {
                pipeline,
                layout,
                sampler,
                cache: HashMap::new(),
                support: CompressionSupport::from_features(device.features()),
            });
        }
        let Some(gpu) = resources.get_mut::<SkyGpu>() else { return Vec::new() };
        if !gpu.cache.contains_key(&self.frame.sky) {
            if let Ok(cpu) = load_cpu_texture(self.frame.project.as_deref(), &self.frame.sky) {
                if cpu.kind == SampleKind::Cube {
                    let uploaded = upload_cpu_texture(device, queue, &cpu, gpu.support);
                    gpu.cache.insert(self.frame.sky.clone(), uploaded);
                }
            }
        }
        let view = {
            let Some(image) = gpu.cache.get(&self.frame.sky) else { return Vec::new() };
            image.texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::Cube),
                ..Default::default()
            })
        };
        let spread = if self.frame.look_through { 0.75 } else { 0.4 };
        let params = f32_bytes(&[self.frame.yaw_deg, self.frame.pitch_deg, spread, 0.0]);
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("seal sky"),
            contents: &params,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("seal sky"),
            layout: &gpu.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&gpu.sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: uniform.as_entire_binding() },
            ],
        });
        let (vw, vh) = viewport_px(self.rect, screen.pixels_per_point);
        if let Some(size) = offscreen_size(vw, vh, self.frame.render_scale) {
            record_sky(device, encoder, size, gpu, &bind);
            *self.prepared.lock().unwrap_or_else(|err| err.into_inner()) = Some(SkyPrep::Scaled);
        } else {
            *self.prepared.lock().unwrap_or_else(|err| err.into_inner()) = Some(SkyPrep::Direct(bind));
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::epaint::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let guard = self.prepared.lock().unwrap_or_else(|err| err.into_inner());
        match guard.as_ref() {
            Some(SkyPrep::Scaled) => {
                if scale_owner() == OWNER_SKY {
                    stretch(pass);
                }
            }
            Some(SkyPrep::Direct(bind)) => {
                let Some(gpu) = resources.get::<SkyGpu>() else { return };
                pass.set_pipeline(&gpu.pipeline);
                pass.set_bind_group(0, bind, &[]);
                pass.draw(0..3, 0..1);
            }
            None => {}
        }
    }
}

struct PreviewGpu {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

struct PreviewCallback {
    rgba: Arc<Vec<u8>>,
    width: u32,
    height: u32,
    format: seal_editor_core::GpuBlockFormat,
    prepared: Mutex<Option<wgpu::BindGroup>>,
    _texture: Mutex<Option<wgpu::Texture>>,
}

pub fn preview_callback(
    rect: Rect,
    rgba: Arc<Vec<u8>>,
    width: u32,
    height: u32,
    format: seal_editor_core::GpuBlockFormat,
) -> egui::PaintCallback {
    egui_wgpu::Callback::new_paint_callback(
        rect,
        PreviewCallback {
            rgba,
            width,
            height,
            format,
            prepared: Mutex::new(None),
            _texture: Mutex::new(None),
        },
    )
}

impl egui_wgpu::CallbackTrait for PreviewCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if !resources.contains::<PreviewGpu>() {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("seal texture preview"),
                source: wgpu::ShaderSource::Wgsl(PREVIEW_WGSL.into()),
            });
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("seal texture preview"),
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
                label: Some("seal texture preview"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("seal texture preview"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: Some(depth_state(false, wgpu::CompareFunction::Always)),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(color_target(None))],
                }),
                multiview_mask: None,
                cache: None,
            });
            let sampler = device.create_sampler(&Default::default());
            resources.insert(PreviewGpu { pipeline, layout, sampler });
        }
        let Some(gpu) = resources.get::<PreviewGpu>() else { return Vec::new() };
        let support = CompressionSupport::from_features(device.features());
        let uploaded = upload_cpu_texture(
            device,
            queue,
            &crate::gpu_wgpu_choice::CpuTexture {
                kind: SampleKind::D2,
                format: self.format,
                width: self.width.max(1),
                height: self.height.max(1),
                layers: 1,
                rgba: (*self.rgba).clone(),
            },
            support,
        );
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("seal texture preview"),
            layout: &gpu.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&uploaded.view),
                },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&gpu.sampler) },
            ],
        });
        *self._texture.lock().unwrap_or_else(|err| err.into_inner()) = Some(uploaded.texture);
        *self.prepared.lock().unwrap_or_else(|err| err.into_inner()) = Some(bind);
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::epaint::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(gpu) = resources.get::<PreviewGpu>() else { return };
        let guard = self.prepared.lock().unwrap_or_else(|err| err.into_inner());
        let Some(bind) = guard.as_ref() else { return };
        pass.set_pipeline(&gpu.pipeline);
        pass.set_bind_group(0, bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

struct ScaleState {
    frame: u64,
    size: (u32, u32),
    _color: wgpu::Texture,
    color_view: wgpu::TextureView,
    _depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    pipeline: wgpu::RenderPipeline,
    bind: wgpu::BindGroup,
    cleared: bool,
    owner: u8,
}

static SCALE: Mutex<Option<ScaleState>> = Mutex::new(None);

fn alpha_blend() -> wgpu::BlendState {
    wgpu::BlendState {
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
    }
}

impl ScaleState {
    fn new(device: &wgpu::Device, size: (u32, u32)) -> Self {
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("seal render scale"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: crate::gpu_viewport_wgpu::SCENE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("seal render scale depth"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: crate::gpu_viewport_wgpu::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let color_view = color.create_view(&Default::default());
        let depth_view = depth.create_view(&Default::default());
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("seal render scale"),
            source: wgpu::ShaderSource::Wgsl(STRETCH_WGSL.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("seal render scale"),
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
            label: Some("seal render scale"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("seal render scale"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: Some(depth_state(false, wgpu::CompareFunction::Always)),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: crate::gpu_viewport_wgpu::SCENE_FORMAT,
                    blend: Some(alpha_blend()),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("seal render scale"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("seal render scale"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color_view),
                },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        Self {
            frame: FRAME.load(Ordering::Relaxed),
            size,
            _color: color,
            color_view,
            _depth: depth,
            depth_view,
            pipeline,
            bind,
            cleared: false,
            owner: 0,
        }
    }
}

fn prepare_scale(device: &wgpu::Device, size: (u32, u32)) -> bool {
    let mut guard = SCALE.lock().unwrap_or_else(|err| err.into_inner());
    let frame = FRAME.load(Ordering::Relaxed);
    let rebuild = guard.as_ref().map(|state| state.size) != Some(size);
    if rebuild {
        *guard = Some(ScaleState::new(device, size));
    }
    let state = guard.as_mut().expect("scale");
    if state.frame != frame {
        state.frame = frame;
        state.cleared = false;
        state.owner = 0;
    }
    let clear = !state.cleared;
    state.cleared = true;
    clear
}

fn claim(owner: u8) {
    let mut guard = SCALE.lock().unwrap_or_else(|err| err.into_inner());
    if let Some(state) = guard.as_mut() {
        if owner >= state.owner {
            state.owner = owner;
        }
    }
}

fn scale_owner() -> u8 {
    SCALE.lock().unwrap_or_else(|err| err.into_inner()).as_ref().map(|state| state.owner).unwrap_or(0)
}

fn stretch(pass: &mut wgpu::RenderPass<'static>) {
    let guard = SCALE.lock().unwrap_or_else(|err| err.into_inner());
    let Some(state) = guard.as_ref() else { return };
    pass.set_pipeline(&state.pipeline);
    pass.set_bind_group(0, &state.bind, &[]);
    pass.draw(0..3, 0..1);
}

fn record_sky(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    size: (u32, u32),
    gpu: &SkyGpu,
    bind: &wgpu::BindGroup,
) {
    let clear = prepare_scale(device, size);
    {
        let guard = SCALE.lock().unwrap_or_else(|err| err.into_inner());
        let state = guard.as_ref().expect("scale");
        let (width, height) = state.size;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("seal render scale"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &state.color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: if clear {
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &state.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: if clear { wgpu::LoadOp::Clear(1.0) } else { wgpu::LoadOp::Load },
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);
        pass.set_scissor_rect(0, 0, width, height);
        pass.set_pipeline(&gpu.pipeline);
        pass.set_bind_group(0, bind, &[]);
        pass.draw(0..3, 0..1);
    }
    claim(OWNER_SKY);
}

fn record_mesh(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    size: (u32, u32),
    gpu: &MeshGpu,
    vertices: &wgpu::Buffer,
    batches: &[BatchDraw],
) {
    let clear = prepare_scale(device, size);
    {
        let guard = SCALE.lock().unwrap_or_else(|err| err.into_inner());
        let state = guard.as_ref().expect("scale");
        let (width, height) = state.size;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("seal render scale"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &state.color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: if clear {
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &state.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: if clear { wgpu::LoadOp::Clear(1.0) } else { wgpu::LoadOp::Load },
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);
        pass.set_scissor_rect(0, 0, width, height);
        pass.set_pipeline(&gpu.pipeline);
        pass.set_vertex_buffer(0, vertices.slice(..));
        for batch in batches {
            if batch.count == 0 {
                continue;
            }
            pass.set_bind_group(0, &batch.bind, &[]);
            pass.draw(batch.first..batch.first + batch.count, 0..1);
        }
    }
    claim(OWNER_MESH);
}

#[cfg(test)]
mod scale_tests {
    use super::{offscreen_size, STRETCH_WGSL};

    #[test]
    fn a_full_scale_stays_on_the_direct_path_and_smaller_scales_shrink() {
        assert!(offscreen_size(1920, 1080, 100).is_none());
        assert!(offscreen_size(1280, 720, 100).is_none());
        assert_eq!(offscreen_size(1280, 720, 50), Some((640, 360)));
        assert_eq!(offscreen_size(800, 600, 75), Some((600, 450)));
        assert_eq!(offscreen_size(800, 600, 25), Some(crate::gpu_viewport::scaled_size(800, 600, 25)));
        assert_eq!(offscreen_size(100, 80, 10), Some(crate::gpu_viewport::scaled_size(100, 80, 10)));
        assert_eq!(offscreen_size(8, 8, 25), Some((2, 2)));
        assert!(offscreen_size(1, 1, 25).is_none());
        assert!(offscreen_size(1, 1, 100).is_none());
    }

    #[test]
    fn the_stretch_shader_is_valid() {
        let module = naga::front::wgsl::parse_str(STRETCH_WGSL).expect("parse");
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        );
        validator.validate(&module).expect("validate");
        let names: Vec<_> = module.entry_points.iter().map(|entry| entry.name.as_str()).collect();
        assert!(names.contains(&"vs") && names.contains(&"fs"));
    }
}

