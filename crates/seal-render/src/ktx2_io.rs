//! Write and read Khronos KTX2 container files.

use crate::gpu_block_format::GpuBlockFormat;
use crate::picture::PictureError;
use crate::picture_blocks::KTX2_MAGIC;
use crate::texture_encode::has_transparency;

/// Parsed KTX2 image levels (mip 0 first).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ktx2Image {
    pub format: GpuBlockFormat,
    pub vk_format: u32,
    pub width: u32,
    pub height: u32,
    pub srgb: bool,
    pub levels: Vec<Vec<u8>>,
}

pub fn vk_format_for(format: GpuBlockFormat, srgb: bool, alpha: bool) -> u32 {
    match (format, srgb, alpha) {
        (GpuBlockFormat::Rgba8, true, _) => 43,
        (GpuBlockFormat::Rgba8, false, _) => 37,
        (GpuBlockFormat::Bc1, true, true) => 134,
        (GpuBlockFormat::Bc1, true, false) => 132,
        (GpuBlockFormat::Bc1, false, true) => 133,
        (GpuBlockFormat::Bc1, false, false) => 131,
        (GpuBlockFormat::Bc2, true, _) => 136,
        (GpuBlockFormat::Bc2, false, _) => 135,
        (GpuBlockFormat::Bc3, true, _) => 138,
        (GpuBlockFormat::Bc3, false, _) => 137,
        (GpuBlockFormat::Bc4, true, _) => 140,
        (GpuBlockFormat::Bc4, false, _) => 139,
        (GpuBlockFormat::Bc5, true, _) => 142,
        (GpuBlockFormat::Bc5, false, _) => 141,
        (GpuBlockFormat::Bc6h, _, _) => 143,
        (GpuBlockFormat::Bc7, true, _) => 146,
        (GpuBlockFormat::Bc7, false, _) => 145,
        (GpuBlockFormat::Etc2, true, true) => 150,
        (GpuBlockFormat::Etc2, true, false) => 148,
        (GpuBlockFormat::Etc2, false, true) => 149,
        (GpuBlockFormat::Etc2, false, false) => 147,
        (GpuBlockFormat::Astc4x4, true, _) => 158,
        (GpuBlockFormat::Astc4x4, false, _) => 157,
    }
}

pub fn gpu_format_from_vk(vk: u32) -> Option<GpuBlockFormat> {
    Some(match vk {
        131 | 132 | 133 | 134 => GpuBlockFormat::Bc1,
        135 | 136 => GpuBlockFormat::Bc2,
        137 | 138 => GpuBlockFormat::Bc3,
        139 | 140 => GpuBlockFormat::Bc4,
        141 | 142 => GpuBlockFormat::Bc5,
        143 | 144 => GpuBlockFormat::Bc6h,
        145 | 146 => GpuBlockFormat::Bc7,
        147 | 148 | 149 | 150 => GpuBlockFormat::Etc2,
        157 | 158 => GpuBlockFormat::Astc4x4,
        37 | 43 => GpuBlockFormat::Rgba8,
        _ => return None,
    })
}

pub fn is_vk_srgb(vk: u32) -> bool {
    matches!(vk, 43 | 132 | 134 | 136 | 138 | 140 | 142 | 146 | 148 | 150 | 158)
}

/// Build a KTX2 file from mip levels (level 0 is the largest).
pub fn write_ktx2(
    format: GpuBlockFormat,
    srgb: bool,
    width: u32,
    height: u32,
    levels: &[Vec<u8>],
) -> Result<Vec<u8>, PictureError> {
    if width == 0 || height == 0 || levels.is_empty() {
        return Err(PictureError::new("ktx2 size is not usable"));
    }
    let vk = vk_format_for(format, srgb, format == GpuBlockFormat::Bc3);
    let level_count = levels.len() as u32;
    const HEADER: usize = 80;
    let index_len = level_count as usize * 24;
    let mut payload = Vec::new();
    let mut index = Vec::new();
    let mut offset = (HEADER + index_len) as u64;
    for data in levels {
        index.push((offset, data.len() as u64));
        payload.extend_from_slice(data);
        offset += data.len() as u64;
    }
    let mut out = Vec::with_capacity(HEADER + index_len + payload.len());
    out.extend_from_slice(&KTX2_MAGIC);
    out.extend_from_slice(&vk.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&level_count.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    for _ in 0..8 {
        out.extend_from_slice(&0u32.to_le_bytes());
    }
    debug_assert_eq!(out.len(), HEADER);
    for (off, len) in &index {
        out.extend_from_slice(&off.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
    }
    out.extend_from_slice(&payload);
    Ok(out)
}

pub fn write_ktx2_from_rgba(
    format: GpuBlockFormat,
    srgb: bool,
    width: u32,
    height: u32,
    rgba: &[u8],
    mips: bool,
    quality: crate::texture_encode::EncodeQuality,
) -> Result<Vec<u8>, PictureError> {
    let alpha = has_transparency(rgba);
    let mut levels = Vec::new();
    let mut data = rgba.to_vec();
    let mut w = width;
    let mut h = height;
    loop {
        let encoded = crate::texture_encode::encode_rgba(format, quality, w, h, &data)?;
        levels.push(encoded);
        if !mips || w == 1 && h == 1 {
            break;
        }
        let (next, nw, nh) = crate::picture::box_mip(&data, w, h);
        data = next;
        w = nw;
        h = nh;
    }
    let vk_fmt = vk_format_for(format, srgb, alpha);
    if gpu_format_from_vk(vk_fmt).is_none() {
        return Err(PictureError::new("ktx2 format is not supported"));
    }
    write_ktx2(format, srgb, width, height, &levels)
}

/// Read a KTX2 file written by this crate or imported from a supported block format.
pub fn read_ktx2(bytes: &[u8]) -> Result<Ktx2Image, PictureError> {
    if bytes.len() < 80 || bytes[..12] != KTX2_MAGIC {
        return Err(PictureError::new("the picture could not be read"));
    }
    let u32_at = |off: usize| -> Result<u32, PictureError> {
        let raw = bytes
            .get(off..off + 4)
            .ok_or_else(|| PictureError::new("the picture is truncated"))?;
        Ok(u32::from_le_bytes(raw.try_into().map_err(|_| PictureError::new("the picture is truncated"))?))
    };
    let u64_at = |off: usize| -> Result<u64, PictureError> {
        let raw = bytes
            .get(off..off + 8)
            .ok_or_else(|| PictureError::new("the picture is truncated"))?;
        Ok(u64::from_le_bytes(raw.try_into().map_err(|_| PictureError::new("the picture is truncated"))?))
    };
    let vk = u32_at(12)?;
    let width = u32_at(20)?;
    let height = u32_at(24)?;
    let depth = u32_at(28)?;
    let layers = u32_at(32)?;
    let faces = u32_at(36)?;
    let levels = u32_at(40)?;
    if depth > 1 || layers > 1 || faces > 1 {
        return Err(PictureError::new("this picture layout is not supported"));
    }
    if levels == 0 || width == 0 || height == 0 {
        return Err(PictureError::new("the picture size is not usable"));
    }
    let format = gpu_format_from_vk(vk).ok_or_else(|| PictureError::new("this picture format is not supported"))?;
    let srgb = is_vk_srgb(vk);
    let mut mips = Vec::with_capacity(levels as usize);
    for level in 0..levels {
        let at = 80 + level as usize * 24;
        let offset = u64_at(at)?;
        let length = u64_at(at + 8)?;
        let start = usize::try_from(offset).map_err(|_| PictureError::new("the picture could not be read"))?;
        let end = start.saturating_add(length as usize);
        let slice = bytes
            .get(start..end)
            .ok_or_else(|| PictureError::new("the picture is truncated"))?;
        mips.push(slice.to_vec());
    }
    Ok(Ktx2Image {
        format,
        vk_format: vk,
        width,
        height,
        srgb,
        levels: mips,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::texture_encode::{encode_rgba, EncodeQuality, psnr_rgba};
    use crate::texture_encode::decode_to_rgba;

    fn sample() -> (Vec<u8>, u32, u32) {
        let w = 16;
        let h = 16;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                rgba[i] = (x * 11) as u8;
                rgba[i + 1] = (y * 13) as u8;
                rgba[i + 2] = 40;
                rgba[i + 3] = 255;
            }
        }
        (rgba, w, h)
    }

    #[test]
    fn ktx2_write_read_round_trip_with_mips() {
        let (rgba, w, h) = sample();
        let file = write_ktx2_from_rgba(GpuBlockFormat::Bc3, true, w, h, &rgba, true, EncodeQuality::Normal)
            .unwrap();
        let back = read_ktx2(&file).unwrap();
        assert_eq!(back.width, w);
        assert_eq!(back.height, h);
        assert!(back.srgb);
        assert_eq!(back.format, GpuBlockFormat::Bc3);
        assert!(back.levels.len() >= 2);
        let decoded = decode_to_rgba(back.format, w, h, &back.levels[0]).unwrap();
        assert!(psnr_rgba(&rgba, &decoded) > 15.0);
    }

    #[test]
    fn damaged_ktx2_errors_without_panic() {
        let bad = vec![0u8; 40];
        let ran = std::panic::catch_unwind(|| read_ktx2(&bad));
        assert!(ran.is_ok());
        assert!(ran.unwrap().is_err());
    }

    #[test]
    fn encode_all_bc_formats_to_ktx2() {
        let (rgba, w, h) = sample();
        for format in [
            GpuBlockFormat::Bc1,
            GpuBlockFormat::Bc2,
            GpuBlockFormat::Bc4,
            GpuBlockFormat::Bc7,
        ] {
            let data = encode_rgba(format, EncodeQuality::Normal, w, h, &rgba).unwrap();
            let file = write_ktx2(format, false, w, h, &[data]).unwrap();
            let img = read_ktx2(&file).unwrap();
            assert_eq!(img.format, format);
        }
    }
}