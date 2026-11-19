//! 2D blend modes on the HDR window. Each batch keeps its own blend state and
//! the batches stay in the order they were submitted.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use egui::{Rect, TextureId};
use wgpu::util::DeviceExt as _;

use crate::gpu_sprite_blend::vertex_floats;
use crate::gpu_wgpu_choice::blend_state;
use crate::sprite_blend::{split_batches, texture_key, BlendDraw};

pub const SPRITE_WGSL: &str = include_str!("sprite_wgpu.wgsl");

static PENDING: Mutex<Vec<TextureId>> = Mutex::new(Vec::new());
static VIEWS: Mutex<Option<HashMap<u64, wgpu::TextureView>>> = Mutex::new(None);

pub fn queue_sprite_texture(id: TextureId) {
    PENDING.lock().unwrap_or_else(|err| err.into_inner()).push(id);
}

pub fn pending_sprite_textures() -> Vec<TextureId> {
    std::mem::take(&mut *PENDING.lock().unwrap_or_else(|err| err.into_inner()))
}

pub fn install_sprite_view(key: u64, view: wgpu::TextureView) {
    VIEWS
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(key, view);
}

struct SpriteGpu {
    pipelines: HashMap<String, wgpu::RenderPipeline>,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    module: wgpu::ShaderModule,
}

struct SpritePrepared {
    vertices: wgpu::Buffer,
    batches: Vec<(String, wgpu::BindGroup, u32)>,
}

struct SpriteCallback {
    rect: Rect,
    draws: Arc<Vec<BlendDraw>>,
    prepared: Mutex<Option<SpritePrepared>>,
}

pub fn paint_callback(rect: Rect, draws: Vec<BlendDraw>) -> egui::PaintCallback {
    for draw in &draws {
        queue_sprite_texture(draw.texture);
    }
    egui_wgpu::Callback::new_paint_callback(
        rect,
        SpriteCallback { rect, draws: Arc::new(draws), prepared: Mutex::new(None) },
    )
}

fn f32_bytes(values: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * 4);
    for value in values {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

impl SpriteGpu {
    fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("seal sprite blend"),
            source: wgpu::ShaderSource::Wgsl(SPRITE_WGSL.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("seal sprite blend"),
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("seal sprite blend"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self { pipelines: HashMap::new(), layout, sampler, module }
    }

    fn pipeline(&mut self, device: &wgpu::Device, mode: &str) -> &wgpu::RenderPipeline {
        if !self.pipelines.contains_key(mode) {
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("seal sprite blend"),
                bind_group_layouts: &[Some(&self.layout)],
                immediate_size: 0,
            });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("seal sprite blend"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &self.module,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: 32,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4],
                    })],
                },
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: crate::gpu_viewport_wgpu::DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &self.module,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: crate::gpu_viewport_wgpu::SCENE_FORMAT,
                        blend: Some(blend_state(mode)),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
            self.pipelines.insert(mode.to_string(), pipeline);
        }
        self.pipelines.get(mode).expect("pipeline")
    }
}

impl egui_wgpu::CallbackTrait for SpriteCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if !resources.contains::<SpriteGpu>() {
            resources.insert(SpriteGpu::new(device));
        }
        let Some(gpu) = resources.get_mut::<SpriteGpu>() else { return Vec::new() };
        let views = VIEWS.lock().unwrap_or_else(|err| err.into_inner());
        let Some(views) = views.as_ref() else { return Vec::new() };
        let mut data = Vec::new();
        let mut batches = Vec::new();
        for batch in split_batches(&self.draws) {
            let key = texture_key(batch.key.texture_id());
            let Some(view) = views.get(&key) else { continue };
            let start = data.len() / 8;
            for draw in &batch.items {
                for v in &draw.vertices {
                    data.extend(vertex_floats(v, self.rect));
                }
            }
            let count = (data.len() / 8) - start;
            if count == 0 {
                continue;
            }
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("seal sprite blend"),
                layout: &gpu.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&gpu.sampler) },
                ],
            });
            let _ = gpu.pipeline(device, &batch.key.mode);
            batches.push((batch.key.mode.clone(), bind, count as u32));
            let _ = start;
        }
        if data.is_empty() {
            return Vec::new();
        }
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("seal sprite blend"),
            contents: &f32_bytes(&data),
            usage: wgpu::BufferUsages::VERTEX,
        });
        *self.prepared.lock().unwrap_or_else(|err| err.into_inner()) =
            Some(SpritePrepared { vertices, batches });
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::epaint::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(gpu) = resources.get::<SpriteGpu>() else { return };
        let guard = self.prepared.lock().unwrap_or_else(|err| err.into_inner());
        let Some(prepared) = guard.as_ref() else { return };
        pass.set_vertex_buffer(0, prepared.vertices.slice(..));
        let mut first = 0u32;
        for (mode, bind, count) in &prepared.batches {
            let Some(pipeline) = gpu.pipelines.get(mode) else {
                first += count;
                continue;
            };
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind, &[]);
            pass.draw(first..first + count, 0..1);
            first += count;
        }
    }
}

trait TextureOf {
    fn texture_id(&self) -> TextureId;
}

impl TextureOf for crate::sprite_blend::BatchKey {
    fn texture_id(&self) -> TextureId {
        if self.texture & (1u64 << 63) != 0 {
            TextureId::User(self.texture & !(1u64 << 63))
        } else {
            TextureId::Managed(self.texture)
        }
    }
}
