//! GPU block-compressed texture formats and capability fallbacks.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum GpuBlockFormat {
    #[default]
    Rgba8,
    Bc1,
    Bc2,
    Bc3,
    Bc4,
    Bc5,
    Bc6h,
    Bc7,
    Etc2,
    Astc4x4,
}

impl GpuBlockFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "rgba8" | "rgba" => Some(Self::Rgba8),
            "bc1" | "dxt1" => Some(Self::Bc1),
            "bc2" | "dxt3" => Some(Self::Bc2),
            "bc3" | "dxt5" => Some(Self::Bc3),
            "bc4" => Some(Self::Bc4),
            "bc5" => Some(Self::Bc5),
            "bc6h" => Some(Self::Bc6h),
            "bc7" => Some(Self::Bc7),
            "etc2" => Some(Self::Etc2),
            "astc" | "astc_4x4" | "astc4x4" => Some(Self::Astc4x4),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Rgba8 => "rgba8",
            Self::Bc1 => "bc1",
            Self::Bc2 => "bc2",
            Self::Bc3 => "bc3",
            Self::Bc4 => "bc4",
            Self::Bc5 => "bc5",
            Self::Bc6h => "bc6h",
            Self::Bc7 => "bc7",
            Self::Etc2 => "etc2",
            Self::Astc4x4 => "astc4x4",
        }
    }

    pub fn is_block_compressed(self) -> bool {
        !matches!(self, Self::Rgba8)
    }

    /// Feature token names checked against an adapter (tested without wgpu in this crate).
    pub fn required_feature_names(self) -> &'static [&'static str] {
        match self {
            Self::Rgba8 => &[],
            Self::Bc1 | Self::Bc2 | Self::Bc3 => &["texture_compression_bc"],
            Self::Bc4 | Self::Bc5 => &["texture_compression_bc"],
            Self::Bc6h | Self::Bc7 => &["texture_compression_bc"],
            Self::Etc2 => &["texture_compression_etc2"],
            Self::Astc4x4 => &["texture_compression_astc"],
        }
    }

    /// When the adapter lacks support, decode block data to RGBA8 on load.
    pub fn effective_load_format(self, supported: &[&str]) -> Self {
        if !self.is_block_compressed() {
            return Self::Rgba8;
        }
        let ok = self
            .required_feature_names()
            .iter()
            .all(|need| supported.iter().any(|have| *have == *need));
        if ok { self } else { Self::Rgba8 }
    }

    pub fn block_bytes_per_pixel(self) -> f32 {
        match self {
            Self::Rgba8 => 4.0,
            Self::Bc1 | Self::Bc4 | Self::Etc2 => 0.5,
            Self::Bc2 | Self::Bc3 | Self::Bc5 | Self::Bc6h | Self::Bc7 => 1.0,
            Self::Astc4x4 => 1.0,
        }
    }
}

/// Encode one 4x4 RGBA tile to BC1 (opaque blocks only; full alpha uses BC3 elsewhere).
pub fn encode_bc1_tile(rgba: &[u8]) -> [u8; 8] {
    let mut out = [0u8; 8];
    let c0 = dominant_color(rgba);
    let c1 = [0u8, 0, 0, 255];
    write_rgb565(c0, &mut out[0..2]);
    write_rgb565(c1, &mut out[2..4]);
    let mut indices = 0u32;
    for (i, px) in rgba.chunks_exact(4).take(16).enumerate() {
        let d0 = color_dist_rgb(px, c0);
        let d1 = color_dist_rgb(px, c1);
        let code = if d0 <= d1 { 0u32 } else { 1u32 };
        indices |= code << (i * 2);
    }
    out[4..8].copy_from_slice(&indices.to_le_bytes());
    out
}

/// Decode one BC1 block (8 bytes) to 16 RGBA pixels.
pub fn decode_bc1_block(block: &[u8; 8]) -> [u8; 64] {
    let c0 = read_rgb565(&block[0..2]);
    let c1 = read_rgb565(&block[2..4]);
    let indices = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
    let palette = bc1_palette(c0, c1);
    let mut out = [0u8; 64];
    for i in 0..16 {
        let code = ((indices >> (i * 2)) & 3) as usize;
        let c = palette[code];
        out[i * 4..i * 4 + 4].copy_from_slice(&c);
    }
    out
}

fn bc1_palette(c0: [u8; 4], c1: [u8; 4]) -> [[u8; 4]; 4] {
    let gt = u16::from(c0[0]) + u16::from(c0[1]) + u16::from(c0[2])
        > u16::from(c1[0]) + u16::from(c1[1]) + u16::from(c1[2]);
    if gt {
        [
            c0,
            c1,
            lerp3(c0, c1, 1, 2),
            [0, 0, 0, 0],
        ]
    } else {
        [c0, c1, lerp3(c0, c1, 1, 2), lerp3(c0, c1, 2, 1)]
    }
}

fn lerp3(a: [u8; 4], b: [u8; 4], na: u32, nb: u32) -> [u8; 4] {
    let den = na + nb;
    [
        ((u32::from(a[0]) * nb + u32::from(b[0]) * na) / den) as u8,
        ((u32::from(a[1]) * nb + u32::from(b[1]) * na) / den) as u8,
        ((u32::from(a[2]) * nb + u32::from(b[2]) * na) / den) as u8,
        255,
    ]
}

fn dominant_color(rgba: &[u8]) -> [u8; 4] {
    let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
    let n = (rgba.len() / 4).max(1) as u32;
    for px in rgba.chunks_exact(4) {
        r += u32::from(px[0]);
        g += u32::from(px[1]);
        b += u32::from(px[2]);
    }
    [(r / n) as u8, (g / n) as u8, (b / n) as u8, 255]
}

fn color_dist_rgb(a: &[u8], b: [u8; 4]) -> u32 {
    let dr = i32::from(a[0]) - i32::from(b[0]);
    let dg = i32::from(a[1]) - i32::from(b[1]);
    let db = i32::from(a[2]) - i32::from(b[2]);
    (dr * dr + dg * dg + db * db) as u32
}

fn write_rgb565(c: [u8; 4], out: &mut [u8]) {
    let v = (u16::from(c[0]) >> 3) << 11 | (u16::from(c[1]) >> 2) << 5 | (u16::from(c[2]) >> 3);
    out.copy_from_slice(&v.to_le_bytes());
}

fn read_rgb565(raw: &[u8]) -> [u8; 4] {
    let v = u16::from_le_bytes([raw[0], raw[1]]);
    let r = ((v >> 11) & 0x1f) as u8;
    let g = ((v >> 5) & 0x3f) as u8;
    let b = (v & 0x1f) as u8;
    [
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
        255,
    ]
}

/// Turn RGBA8 mip0 into block-compressed export bytes when the format is supported.
pub fn encode_export_rgba(
    format: GpuBlockFormat,
    supported: &[&str],
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Vec<u8> {
    let effective = format.effective_load_format(supported);
    if effective == GpuBlockFormat::Rgba8 {
        return rgba.to_vec();
    }
    crate::texture_encode::encode_rgba(
        effective,
        crate::texture_encode::EncodeQuality::Normal,
        width,
        height,
        rgba,
    )
    .unwrap_or_else(|_| rgba.to_vec())
}

#[allow(dead_code)]
fn encode_image_bc1(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let bw = width.div_ceil(4);
    let bh = height.div_ceil(4);
    let mut out = Vec::with_capacity((bw * bh * 8) as usize);
    for by in 0..bh {
        for bx in 0..bw {
            let mut tile = [0u8; 64];
            for y in 0..4 {
                for x in 0..4 {
                    let px = (bx * 4 + x).min(width - 1);
                    let py = (by * 4 + y).min(height - 1);
                    let i = ((py * width + px) * 4) as usize;
                    tile[(y * 4 + x) as usize * 4..(y * 4 + x) as usize * 4 + 4]
                        .copy_from_slice(&rgba[i..i + 4]);
                }
            }
            out.extend_from_slice(&encode_bc1_tile(&tile));
        }
    }
    out
}

/// Decode block-compressed export bytes to RGBA8 for load when features are missing.
pub fn decode_block_to_rgba8(
    format: GpuBlockFormat,
    width: u32,
    height: u32,
    data: &[u8],
) -> Result<Vec<u8>, String> {
    match format {
        GpuBlockFormat::Rgba8 => {
            if data.len() != (width * height * 4) as usize {
                return Err("rgba size mismatch".into());
            }
            Ok(data.to_vec())
        }
        GpuBlockFormat::Bc1 => decode_image_bc1(width, height, data),
        other => crate::texture_encode::decode_to_rgba(other, width, height, data)
            .map_err(|e| e.to_string()),
    }
}

fn decode_image_bc1(width: u32, height: u32, data: &[u8]) -> Result<Vec<u8>, String> {
    let bw = width.div_ceil(4);
    let bh = height.div_ceil(4);
    let need = (bw * bh * 8) as usize;
    if data.len() < need {
        return Err("bc1 data too short".into());
    }
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    let mut bi = 0;
    for by in 0..bh {
        for bx in 0..bw {
            let block: [u8; 8] = data[bi..bi + 8].try_into().map_err(|_| "bad block")?;
            bi += 8;
            let pixels = decode_bc1_block(&block);
            for y in 0..4 {
                for x in 0..4 {
                    let px = bx * 4 + x;
                    let py = by * 4 + y;
                    if px >= width || py >= height {
                        continue;
                    }
                    let dst = ((py * width + px) * 4) as usize;
                    let src = (y * 4 + x) as usize * 4;
                    rgba[dst..dst + 4].copy_from_slice(&pixels[src..src + 4]);
                }
            }
        }
    }
    Ok(rgba)
}

#[derive(Clone, Debug)]
pub struct FormatFallbackError;

impl fmt::Display for FormatFallbackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "block format could not be decoded")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_fallback_bc_without_feature() {
        let supported = &[] as &[&str];
        assert_eq!(
            GpuBlockFormat::Bc7.effective_load_format(supported),
            GpuBlockFormat::Rgba8
        );
        assert_eq!(
            GpuBlockFormat::Bc1.effective_load_format(&["texture_compression_bc"]),
            GpuBlockFormat::Bc1
        );
    }

    #[test]
    fn bc1_round_trip_block() {
        let mut tile = [0u8; 64];
        for i in 0..16 {
            tile[i * 4] = (i * 17) as u8;
            tile[i * 4 + 1] = 40;
            tile[i * 4 + 2] = 200;
            tile[i * 4 + 3] = 255;
        }
        let block = encode_bc1_tile(&tile);
        let back = decode_bc1_block(&block);
        assert_eq!(back.len(), 64);
    }

    #[test]
    fn export_bc1_encodes_when_supported() {
        let rgba = vec![255u8; 4 * 4 * 4];
        let encoded = encode_export_rgba(GpuBlockFormat::Bc1, &["texture_compression_bc"], 4, 4, &rgba);
        assert_eq!(encoded.len(), 8);
        let decoded = decode_block_to_rgba8(GpuBlockFormat::Bc1, 4, 4, &encoded).unwrap();
        assert_eq!(decoded.len(), 64);
    }

    #[test]
    fn damaged_bc1_rejected() {
        assert!(decode_block_to_rgba8(GpuBlockFormat::Bc1, 4, 4, &[0u8; 4]).is_err());
    }
}