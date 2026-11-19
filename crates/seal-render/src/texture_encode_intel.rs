//! Block compressors via Intel ISPC kernels (not on Windows GNU).

use intel_tex_2::{self, RgbaSurface, RgSurface, RSurface};

use crate::gpu_block_format::GpuBlockFormat;
use crate::picture::PictureError;
use crate::texture_encode::{
    encode_blocks_manual, encode_bc1a_block, encode_etc2_a8_block, EncodeQuality,
};

pub fn encode_rgba_intel(
    format: GpuBlockFormat,
    quality: EncodeQuality,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Result<Vec<u8>, PictureError> {
    let transparent = rgba.chunks_exact(4).any(|p| p[3] < 250);
    match format {
        GpuBlockFormat::Rgba8 => Ok(rgba.to_vec()),
        GpuBlockFormat::Bc1 => {
            if transparent {
                Ok(encode_blocks_manual(width, height, 8, rgba, |block| {
                    encode_bc1a_block(block, quality).to_vec()
                }))
            } else {
                Ok(intel_tex_2::bc1::compress_blocks(&rgba_surface(rgba, width, height)))
            }
        }
        GpuBlockFormat::Bc2 => Ok(encode_blocks_manual(width, height, 16, rgba, |block| {
            let mut out = crate::texture_encode::encode_bc2_alpha(block).to_vec();
            let tile = block_rgba(block);
            let color = intel_tex_2::bc1::compress_blocks(&RgbaSurface {
                data: &tile,
                width: 4,
                height: 4,
                stride: 16,
            });
            out.extend_from_slice(&color);
            out
        })),
        GpuBlockFormat::Bc3 => Ok(intel_tex_2::bc3::compress_blocks(&rgba_surface(
            rgba, width, height,
        ))),
        GpuBlockFormat::Bc4 => {
            let (r, ()) = r_surface_owned(rgba, width, height, 0);
            Ok(intel_tex_2::bc4::compress_blocks(&RSurface {
                data: &r,
                width,
                height,
                stride: width,
            }))
        }
        GpuBlockFormat::Bc5 => {
            let (rg, ()) = rg_surface_owned(rgba, width, height);
            Ok(intel_tex_2::bc5::compress_blocks(&RgSurface {
                data: &rg,
                width,
                height,
                stride: width * 2,
            }))
        }
        GpuBlockFormat::Bc6h => encode_bc6h_from_rgba8(width, height, rgba, quality),
        GpuBlockFormat::Bc7 => {
            let settings = bc7_settings(quality, transparent);
            Ok(intel_tex_2::bc7::compress_blocks(
                &settings,
                &rgba_surface(rgba, width, height),
            ))
        }
        GpuBlockFormat::Etc2 => {
            if transparent {
                Ok(encode_etc2_rgba(width, height, rgba, quality))
            } else {
                Ok(intel_tex_2::etc1::compress_blocks(
                    &etc1_settings(quality),
                    &rgba_surface(rgba, width, height),
                ))
            }
        }
        GpuBlockFormat::Astc4x4 => {
            let settings = astc_settings(quality);
            Ok(intel_tex_2::astc::compress_blocks(
                &settings,
                &rgba_surface(rgba, width, height),
            ))
        }
    }
}

/// BC6H from linear HDR floats. Input is tightly packed RGBA `f32` linear values.
pub fn encode_bc6h_linear_float(
    quality: EncodeQuality,
    width: u32,
    height: u32,
    rgba_linear: &[f32],
) -> Result<Vec<u8>, PictureError> {
    let expect = (width as usize) * (height as usize) * 4;
    if rgba_linear.len() != expect {
        return Err(PictureError::new("the picture size is not usable"));
    }
    let fp16 = floats_to_fp16_rgba(rgba_linear, width, height);
    let stride = width * 8;
    let surface = RgbaSurface {
        data: &fp16,
        width,
        height,
        stride,
    };
    let settings = bc6h_settings(quality);
    Ok(intel_tex_2::bc6h::compress_blocks(&settings, &surface))
}

fn encode_bc6h_from_rgba8(
    width: u32,
    height: u32,
    rgba: &[u8],
    quality: EncodeQuality,
) -> Result<Vec<u8>, PictureError> {
    let mut linear = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        linear.push(px[0] as f32 / 255.0);
        linear.push(px[1] as f32 / 255.0);
        linear.push(px[2] as f32 / 255.0);
        linear.push(1.0);
    }
    encode_bc6h_linear_float(quality, width, height, &linear)
}

fn rgba_surface(rgba: &[u8], width: u32, height: u32) -> RgbaSurface<'_> {
    RgbaSurface {
        data: rgba,
        width,
        height,
        stride: width * 4,
    }
}

fn encode_etc2_rgba(
    width: u32,
    height: u32,
    rgba: &[u8],
    quality: EncodeQuality,
) -> Vec<u8> {
    let etc1 = intel_tex_2::etc1::compress_blocks(
        &etc1_settings(quality),
        &rgba_surface(rgba, width, height),
    );
    let a8 = encode_blocks_manual(width, height, 8, rgba, |block| {
        encode_etc2_a8_block(block, quality).to_vec()
    });
    let blocks = (width.div_ceil(4) * height.div_ceil(4)) as usize;
    let mut out = Vec::with_capacity(blocks * 16);
    for i in 0..blocks {
        out.extend_from_slice(&a8[i * 8..i * 8 + 8]);
        out.extend_from_slice(&etc1[i * 8..i * 8 + 8]);
    }
    out
}

fn block_rgba(block: &[[u8; 4]; 16]) -> Vec<u8> {
    let mut rgba = vec![0u8; 64];
    for (i, px) in block.iter().enumerate() {
        rgba[i * 4..i * 4 + 4].copy_from_slice(px);
    }
    rgba
}

fn r_surface_owned(rgba: &[u8], width: u32, height: u32, channel: usize) -> (Vec<u8>, ()) {
    let pixels = (width * height) as usize;
    let mut r = Vec::with_capacity(pixels);
    for px in rgba.chunks_exact(4) {
        r.push(px[channel]);
    }
    (r, ())
}

fn rg_surface_owned(rgba: &[u8], width: u32, height: u32) -> (Vec<u8>, ()) {
    let pixels = (width * height) as usize;
    let mut rg = Vec::with_capacity(pixels * 2);
    for px in rgba.chunks_exact(4) {
        rg.push(px[0]);
        rg.push(px[1]);
    }
    (rg, ())
}

fn etc1_settings(quality: EncodeQuality) -> intel_tex_2::etc1::EncodeSettings {
    match quality {
        EncodeQuality::Fast => intel_tex_2::etc1::EncodeSettings {
            fast_skip_threshold: 12,
        },
        EncodeQuality::Normal => intel_tex_2::etc1::slow_settings(),
        EncodeQuality::Best => intel_tex_2::etc1::EncodeSettings {
            fast_skip_threshold: 2,
        },
    }
}

fn bc7_settings(quality: EncodeQuality, alpha: bool) -> intel_tex_2::bc7::EncodeSettings {
    match (quality, alpha) {
        (EncodeQuality::Fast, false) => intel_tex_2::bc7::opaque_fast_settings(),
        (EncodeQuality::Fast, true) => intel_tex_2::bc7::alpha_fast_settings(),
        (EncodeQuality::Normal, false) => intel_tex_2::bc7::opaque_basic_settings(),
        (EncodeQuality::Normal, true) => intel_tex_2::bc7::alpha_basic_settings(),
        (EncodeQuality::Best, false) => intel_tex_2::bc7::opaque_slow_settings(),
        (EncodeQuality::Best, true) => intel_tex_2::bc7::alpha_slow_settings(),
    }
}

fn bc6h_settings(quality: EncodeQuality) -> intel_tex_2::bc6h::EncodeSettings {
    match quality {
        EncodeQuality::Fast => intel_tex_2::bc6h::very_fast_settings(),
        EncodeQuality::Normal => intel_tex_2::bc6h::basic_settings(),
        EncodeQuality::Best => intel_tex_2::bc6h::slow_settings(),
    }
}

fn astc_settings(quality: EncodeQuality) -> intel_tex_2::astc::EncodeSettings {
    match quality {
        EncodeQuality::Fast => intel_tex_2::astc::alpha_fast_settings(4, 4),
        EncodeQuality::Normal => intel_tex_2::astc::alpha_slow_settings(4, 4),
        EncodeQuality::Best => intel_tex_2::astc::alpha_slow_settings(4, 4),
    }
}

fn floats_to_fp16_rgba(linear: &[f32], width: u32, height: u32) -> Vec<u8> {
    let _ = width;
    let _ = height;
    let mut out = vec![0u8; (linear.len() / 4) * 8];
    for (i, chunk) in linear.chunks_exact(4).enumerate() {
        let base = i * 8;
        for c in 0..4 {
            let half = half::f16::from_f32(chunk[c]).to_bits();
            out[base + c * 2..base + c * 2 + 2].copy_from_slice(&half.to_le_bytes());
        }
    }
    out
}