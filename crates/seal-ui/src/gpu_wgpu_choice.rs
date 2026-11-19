//! Texture shape, block format and blend mode for the HDR window.
//!
//! The choices are plain data so tests cover them without a graphics device.
//! The device upload uses the same answers.

use seal_editor_core::GpuBlockFormat;
use wgpu::BlendFactor;

use crate::gpu_textures::SampleKind;

/// Which block-compression features the adapter offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompressionSupport {
    pub bc: bool,
    pub etc2: bool,
    pub astc: bool,
}

impl CompressionSupport {
    pub fn from_features(features: wgpu::Features) -> Self {
        Self {
            bc: features.contains(wgpu::Features::TEXTURE_COMPRESSION_BC),
            etc2: features.contains(wgpu::Features::TEXTURE_COMPRESSION_ETC2),
            astc: features.contains(wgpu::Features::TEXTURE_COMPRESSION_ASTC),
        }
    }

    pub fn names(self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.bc {
            out.push("texture_compression_bc");
        }
        if self.etc2 {
            out.push("texture_compression_etc2");
        }
        if self.astc {
            out.push("texture_compression_astc");
        }
        out
    }
}

/// Image shape stored on the device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TexShape {
    D2,
    D2Array,
    D3,
    Cube,
}

pub fn tex_shape(kind: SampleKind) -> TexShape {
    match kind {
        SampleKind::None | SampleKind::D2 => TexShape::D2,
        SampleKind::Array => TexShape::D2Array,
        SampleKind::Volume => TexShape::D3,
        SampleKind::Cube => TexShape::Cube,
    }
}

pub fn texture_dimension(shape: TexShape) -> wgpu::TextureDimension {
    match shape {
        TexShape::D3 => wgpu::TextureDimension::D3,
        _ => wgpu::TextureDimension::D2,
    }
}

pub fn view_dimension(shape: TexShape) -> wgpu::TextureViewDimension {
    match shape {
        TexShape::D2 => wgpu::TextureViewDimension::D2,
        TexShape::D2Array => wgpu::TextureViewDimension::D2Array,
        TexShape::D3 => wgpu::TextureViewDimension::D3,
        TexShape::Cube => wgpu::TextureViewDimension::Cube,
    }
}

/// Format the device should store. Block formats stay compressed only when the
/// adapter can sample them. Everything else is RGBA8.
pub fn choose_format(format: GpuBlockFormat, support: CompressionSupport) -> wgpu::TextureFormat {
    let block = match format {
        GpuBlockFormat::Rgba8 => return wgpu::TextureFormat::Rgba8Unorm,
        GpuBlockFormat::Bc1 => wgpu::TextureFormat::Bc1RgbaUnorm,
        GpuBlockFormat::Bc2 => wgpu::TextureFormat::Bc2RgbaUnorm,
        GpuBlockFormat::Bc3 => wgpu::TextureFormat::Bc3RgbaUnorm,
        GpuBlockFormat::Bc4 => wgpu::TextureFormat::Bc4RUnorm,
        GpuBlockFormat::Bc5 => wgpu::TextureFormat::Bc5RgUnorm,
        GpuBlockFormat::Bc6h => wgpu::TextureFormat::Bc6hRgbUfloat,
        GpuBlockFormat::Bc7 => wgpu::TextureFormat::Bc7RgbaUnorm,
        GpuBlockFormat::Etc2 => wgpu::TextureFormat::Etc2Rgba8Unorm,
        GpuBlockFormat::Astc4x4 => wgpu::TextureFormat::Astc {
            block: wgpu::AstcBlock::B4x4,
            channel: wgpu::AstcChannel::Unorm,
        },
    };
    let ok = match format {
        GpuBlockFormat::Bc1
        | GpuBlockFormat::Bc2
        | GpuBlockFormat::Bc3
        | GpuBlockFormat::Bc4
        | GpuBlockFormat::Bc5
        | GpuBlockFormat::Bc6h
        | GpuBlockFormat::Bc7 => support.bc,
        GpuBlockFormat::Etc2 => support.etc2,
        GpuBlockFormat::Astc4x4 => support.astc,
        GpuBlockFormat::Rgba8 => true,
    };
    if ok {
        block
    } else {
        wgpu::TextureFormat::Rgba8Unorm
    }
}

pub fn stores_blocks(format: GpuBlockFormat, support: CompressionSupport) -> bool {
    format.is_block_compressed()
        && choose_format(format, support) != wgpu::TextureFormat::Rgba8Unorm
}

/// Anisotropic clamp the sampler can ask for. 1 is ordinary linear filtering.
pub fn anisotropy_clamp(requested: u32) -> u16 {
    requested.clamp(1, 16) as u16
}

fn factor_of(value: u32) -> BlendFactor {
    if value == eframe::glow::ZERO {
        BlendFactor::Zero
    } else if value == eframe::glow::ONE {
        BlendFactor::One
    } else if value == eframe::glow::SRC_ALPHA {
        BlendFactor::SrcAlpha
    } else if value == eframe::glow::ONE_MINUS_SRC_ALPHA {
        BlendFactor::OneMinusSrcAlpha
    } else if value == eframe::glow::DST_COLOR {
        BlendFactor::Dst
    } else if value == eframe::glow::ONE_MINUS_SRC_COLOR {
        BlendFactor::OneMinusSrc
    } else if value == eframe::glow::ONE_MINUS_DST_ALPHA {
        BlendFactor::OneMinusDstAlpha
    } else {
        BlendFactor::One
    }
}

/// Blend state for one 2D material mode. Draw order is the caller's job.
pub fn blend_state(mode: &str) -> wgpu::BlendState {
    let (src_rgb, dst_rgb, src_a, dst_a) = crate::sprite_blend::gl_blend_factors(mode);
    wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: factor_of(src_rgb),
            dst_factor: factor_of(dst_rgb),
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: factor_of(src_a),
            dst_factor: factor_of(dst_a),
            operation: wgpu::BlendOperation::Add,
        },
    }
}

/// Pixels of one picture or texture asset, ready to upload.
pub struct CpuTexture {
    pub kind: SampleKind,
    pub format: GpuBlockFormat,
    pub width: u32,
    pub height: u32,
    pub layers: u32,
    pub rgba: Vec<u8>,
}

pub struct UploadedTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub slices: f32,
    pub shape: TexShape,
}

fn align_row(row: u32) -> u32 {
    row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT
}

fn pad_rows(texels: &[u8], row: u32, rows: u32) -> (Vec<u8>, u32) {
    let stride = align_row(row.max(1));
    if stride == row && texels.len() == row as usize * rows as usize {
        return (texels.to_vec(), row);
    }
    let mut out = vec![0u8; stride as usize * rows.max(1) as usize];
    let copy = row.min(stride) as usize;
    for y in 0..rows.max(1) as usize {
        let src = y * row as usize;
        let dst = y * stride as usize;
        if src + copy <= texels.len() {
            out[dst..dst + copy].copy_from_slice(&texels[src..src + copy]);
        }
    }
    (out, stride)
}

fn layer_bytes(
    rgba: &[u8],
    format: GpuBlockFormat,
    support: CompressionSupport,
    width: u32,
    height: u32,
    layer: u32,
) -> (wgpu::TextureFormat, Vec<u8>) {
    let stride = (width.max(1) * height.max(1) * 4) as usize;
    let start = layer as usize * stride;
    let end = (start + stride).min(rgba.len());
    let pixels = if start < rgba.len() && end > start { &rgba[start..end] } else { &[][..] };
    if pixels.len() != stride {
        return (wgpu::TextureFormat::Rgba8Unorm, vec![255, 255, 255, 255]);
    }
    if stores_blocks(format, support) {
        let names = support.names();
        let encoded = seal_editor_core::encode_export_rgba(format, &names, width, height, pixels);
        if encoded.len() != pixels.len() {
            return (choose_format(format, support), encoded);
        }
    }
    (wgpu::TextureFormat::Rgba8Unorm, pixels.to_vec())
}

pub fn upload_cpu_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: &CpuTexture,
    support: CompressionSupport,
) -> UploadedTexture {
    let shape = tex_shape(image.kind);
    let layers = image.layers.max(1);
    let (format, _) = layer_bytes(&image.rgba, image.format, support, image.width, image.height, 0);
    let depth = if shape == TexShape::D2 { 1 } else { layers };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("seal albedo"),
        size: wgpu::Extent3d {
            width: image.width.max(1),
            height: image.height.max(1),
            depth_or_array_layers: depth,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: texture_dimension(shape),
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let block = format.is_compressed();
    for layer in 0..depth {
        let (layer_format, bytes) =
            layer_bytes(&image.rgba, image.format, support, image.width, image.height, layer);
        let bytes = if layer_format == format { bytes } else { vec![255, 255, 255, 255] };
        let (row, rows) = if block && layer_format == format {
            let (bw, bh, block_bytes) = crate::gpu_textures::block_footprint(image.format);
            (image.width.max(1).div_ceil(bw) * block_bytes as u32, image.height.max(1).div_ceil(bh))
        } else {
            (image.width.max(1) * 4, image.height.max(1))
        };
        let (upload, stride) = pad_rows(&bytes, row, rows);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: 0, z: layer },
                aspect: wgpu::TextureAspect::All,
            },
            &upload,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(rows.max(1)),
            },
            wgpu::Extent3d {
                width: image.width.max(1),
                height: image.height.max(1),
                depth_or_array_layers: 1,
            },
        );
    }
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(view_dimension(shape)),
        ..Default::default()
    });
    UploadedTexture { texture, view, slices: layers as f32, shape }
}

pub fn upload_white(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    shape: TexShape,
) -> UploadedTexture {
    let layers = if shape == TexShape::Cube { 6 } else { 1 };
    upload_cpu_texture(
        device,
        queue,
        &CpuTexture {
            kind: match shape {
                TexShape::D2 => SampleKind::D2,
                TexShape::D2Array => SampleKind::Array,
                TexShape::D3 => SampleKind::Volume,
                TexShape::Cube => SampleKind::Cube,
            },
            format: GpuBlockFormat::Rgba8,
            width: 1,
            height: 1,
            layers,
            rgba: [255u8, 255, 255, 255].repeat(layers as usize),
        },
        CompressionSupport { bc: false, etc2: false, astc: false },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_follows_the_sampler_kind() {
        assert_eq!(tex_shape(SampleKind::None), TexShape::D2);
        assert_eq!(tex_shape(SampleKind::D2), TexShape::D2);
        assert_eq!(tex_shape(SampleKind::Array), TexShape::D2Array);
        assert_eq!(tex_shape(SampleKind::Volume), TexShape::D3);
        assert_eq!(tex_shape(SampleKind::Cube), TexShape::Cube);
        assert_eq!(texture_dimension(TexShape::D3), wgpu::TextureDimension::D3);
        assert_eq!(texture_dimension(TexShape::Cube), wgpu::TextureDimension::D2);
        assert_eq!(view_dimension(TexShape::D2Array), wgpu::TextureViewDimension::D2Array);
        assert_eq!(view_dimension(TexShape::Cube), wgpu::TextureViewDimension::Cube);
        assert_eq!(view_dimension(TexShape::D3), wgpu::TextureViewDimension::D3);
    }

    #[test]
    fn block_formats_need_the_matching_adapter_feature() {
        let none = CompressionSupport { bc: false, etc2: false, astc: false };
        assert_eq!(choose_format(GpuBlockFormat::Rgba8, none), wgpu::TextureFormat::Rgba8Unorm);
        assert_eq!(choose_format(GpuBlockFormat::Bc7, none), wgpu::TextureFormat::Rgba8Unorm);
        assert_eq!(choose_format(GpuBlockFormat::Etc2, none), wgpu::TextureFormat::Rgba8Unorm);
        assert_eq!(choose_format(GpuBlockFormat::Astc4x4, none), wgpu::TextureFormat::Rgba8Unorm);
        assert!(!stores_blocks(GpuBlockFormat::Bc1, none));

        let bc = CompressionSupport { bc: true, etc2: false, astc: false };
        assert_eq!(choose_format(GpuBlockFormat::Bc1, bc), wgpu::TextureFormat::Bc1RgbaUnorm);
        assert_eq!(choose_format(GpuBlockFormat::Bc3, bc), wgpu::TextureFormat::Bc3RgbaUnorm);
        assert_eq!(choose_format(GpuBlockFormat::Bc5, bc), wgpu::TextureFormat::Bc5RgUnorm);
        assert_eq!(choose_format(GpuBlockFormat::Bc7, bc), wgpu::TextureFormat::Bc7RgbaUnorm);
        assert_eq!(choose_format(GpuBlockFormat::Etc2, bc), wgpu::TextureFormat::Rgba8Unorm);
        assert!(stores_blocks(GpuBlockFormat::Bc1, bc));

        let etc = CompressionSupport { bc: false, etc2: true, astc: false };
        assert_eq!(choose_format(GpuBlockFormat::Etc2, etc), wgpu::TextureFormat::Etc2Rgba8Unorm);
        assert_eq!(choose_format(GpuBlockFormat::Bc1, etc), wgpu::TextureFormat::Rgba8Unorm);

        let astc = CompressionSupport { bc: false, etc2: false, astc: true };
        assert!(matches!(
            choose_format(GpuBlockFormat::Astc4x4, astc),
            wgpu::TextureFormat::Astc { .. }
        ));
        assert_eq!(anisotropy_clamp(0), 1);
        assert_eq!(anisotropy_clamp(8), 8);
        assert_eq!(anisotropy_clamp(64), 16);
    }

    #[test]
    fn each_blend_mode_has_its_own_factors() {
        let alpha = blend_state("alpha");
        let add = blend_state("additive");
        let mul = blend_state("multiply");
        let screen = blend_state("screen");
        let premul = blend_state("premultiplied");
        // Premultiplied fragments (egui textures are premultiplied), as in the OpenGL pass.
        assert_eq!(alpha.color.src_factor, BlendFactor::One);
        assert_eq!(alpha.color.dst_factor, BlendFactor::OneMinusSrcAlpha);
        assert_eq!(add.color.src_factor, BlendFactor::One);
        assert_eq!(add.color.dst_factor, BlendFactor::One);
        assert_eq!(mul.color.src_factor, BlendFactor::Dst);
        assert_eq!(mul.color.dst_factor, BlendFactor::OneMinusSrcAlpha);
        assert_eq!(screen.color.src_factor, BlendFactor::One);
        assert_eq!(screen.color.dst_factor, BlendFactor::OneMinusSrc);
        assert_eq!(premul.color.src_factor, BlendFactor::One);
        assert_eq!(premul.color.dst_factor, BlendFactor::OneMinusSrcAlpha);
        assert_ne!(add, mul);
        assert_ne!(mul, screen);
    }
}
