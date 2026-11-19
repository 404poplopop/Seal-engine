//! KTX2 and DDS picture containers, uncompressed and block-compressed.

use crate::picture::{Picture, PictureError, MAX_SIDE};
use crate::picture_pack::{decode_blocks, Block};

pub(crate) const KTX2_MAGIC: [u8; 12] =
    [0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A];

pub(crate) fn decode_ktx2(bytes: &[u8]) -> Result<Picture, PictureError> {
    if bytes.len() < 80 || bytes[..12] != KTX2_MAGIC {
        return Err(PictureError::new("the picture could not be read"));
    }
    let u32_at = |off: usize| -> Result<u32, PictureError> {
        let raw =
            bytes.get(off..off + 4).ok_or_else(|| PictureError::new("the picture is truncated"))?;
        Ok(u32::from_le_bytes(raw.try_into().unwrap()))
    };
    let u64_at = |off: usize| -> Result<u64, PictureError> {
        let raw =
            bytes.get(off..off + 8).ok_or_else(|| PictureError::new("the picture is truncated"))?;
        Ok(u64::from_le_bytes(raw.try_into().unwrap()))
    };
    let format = u32_at(12)?;
    let width = u32_at(20)?;
    let height = u32_at(24)?.max(1);
    let depth = u32_at(28)?;
    let layers = u32_at(32)?;
    let faces = u32_at(36)?;
    let levels = u32_at(40)?;
    let packed = u32_at(44)?;
    if depth > 1 || layers > 1 || faces > 1 {
        return Err(PictureError::new("this picture layout is not supported"));
    }
    if packed != 0 {
        return Err(PictureError::new("this picture's packed data is not supported"));
    }
    if levels == 0 || width == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(PictureError::new("the picture size is not usable"));
    }
    let mut best: Option<(u64, u64)> = None;
    for level in 0..levels {
        let at = 80 + level as usize * 24;
        let offset = u64_at(at)?;
        let length = u64_at(at + 8)?;
        let raw = u64_at(at + 16)?;
        let rank = raw.max(length);
        if best.map(|(_, size)| rank >= size).unwrap_or(true) {
            best = Some((offset, rank));
        }
    }
    let (offset, _) = best.ok_or_else(|| PictureError::new("the picture could not be read"))?;
    let start =
        usize::try_from(offset).map_err(|_| PictureError::new("the picture could not be read"))?;
    let data = bytes.get(start..).ok_or_else(|| PictureError::new("the picture is truncated"))?;
    let rgba = match format {
        37 | 43 => take_rgba(data, width, height)?,
        131 => decode_blocks(Block::Bc1, data, width, height)?,
        133 => decode_blocks(Block::Bc1a, data, width, height)?,
        135 => decode_blocks(Block::Bc2, data, width, height)?,
        137 => decode_blocks(Block::Bc3, data, width, height)?,
        139 => decode_blocks(Block::Bc4, data, width, height)?,
        141 => decode_blocks(Block::Bc5, data, width, height)?,
        143 => decode_blocks(Block::Bc6, data, width, height)?,
        144 => decode_blocks(Block::Bc6Signed, data, width, height)?,
        145 | 146 => decode_blocks(Block::Bc7, data, width, height)?,
        147 | 148 => decode_blocks(Block::Etc2Rgb, data, width, height)?,
        149 | 150 => decode_blocks(Block::Etc2Rgba, data, width, height)?,
        157 | 158 => decode_blocks(Block::Astc4x4, data, width, height)?,
        _ => return Err(PictureError::new("this picture format is not supported")),
    };
    Ok(picture(width, height, rgba))
}

pub(crate) fn decode_dds(bytes: &[u8]) -> Result<Picture, PictureError> {
    if bytes.len() < 128 || &bytes[..4] != b"DDS " {
        return Err(PictureError::new("the picture could not be read"));
    }
    let size = u32_at(bytes, 4)?;
    let height = u32_at(bytes, 12)?;
    let width = u32_at(bytes, 16)?;
    let format_size = u32_at(bytes, 76)?;
    if size != 124
        || format_size != 32
        || width == 0
        || height == 0
        || width > MAX_SIDE
        || height > MAX_SIDE
    {
        return Err(PictureError::new("the picture size is not usable"));
    }
    let flags = u32_at(bytes, 80)?;
    let fourcc = u32_at(bytes, 84)?;
    let bit_count = u32_at(bytes, 88)?;
    let masks = [u32_at(bytes, 92)?, u32_at(bytes, 96)?, u32_at(bytes, 100)?, u32_at(bytes, 104)?];
    let fourcc_block = flags & 0x4 != 0;
    if fourcc == 0x3031_5844 {
        if bytes.len() < 148 {
            return Err(PictureError::new("the picture is truncated"));
        }
        let code = u32_at(bytes, 128)?;
        let data = &bytes[148..];
        let rgba = match code {
            28 | 29 => take_rgba(data, width, height)?,
            71 | 72 => decode_blocks(Block::Bc1a, data, width, height)?,
            74 | 75 => decode_blocks(Block::Bc2, data, width, height)?,
            77 | 78 => decode_blocks(Block::Bc3, data, width, height)?,
            80 => decode_blocks(Block::Bc4, data, width, height)?,
            83 => decode_blocks(Block::Bc5, data, width, height)?,
            95 => decode_blocks(Block::Bc6, data, width, height)?,
            96 => decode_blocks(Block::Bc6Signed, data, width, height)?,
            98 | 99 => decode_blocks(Block::Bc7, data, width, height)?,
            _ => return Err(PictureError::new("this picture format is not supported")),
        };
        return Ok(picture(width, height, rgba));
    }
    if fourcc_block {
        let data = &bytes[128..];
        let block = match fourcc {
            0x3154_5844 => Block::Bc1a,
            0x3354_5844 | 0x3254_5844 => Block::Bc2,
            0x3554_5844 | 0x3454_5844 => Block::Bc3,
            0x3149_5441 | 0x5534_4342 => Block::Bc4,
            0x3249_5441 | 0x5535_4342 => Block::Bc5,
            _ => return Err(PictureError::new("this picture format is not supported")),
        };
        return Ok(picture(width, height, decode_blocks(block, data, width, height)?));
    }
    if bit_count != 32 {
        return Err(PictureError::new("this picture format is not supported"));
    }
    Ok(picture(width, height, masked_rgba(&bytes[128..], width, height, masks)?))
}

fn picture(width: u32, height: u32, rgba: Vec<u8>) -> Picture {
    Picture { width, height, rgba, linear: None, linear_file: false, mesh: None }
}

fn u32_at(bytes: &[u8], off: usize) -> Result<u32, PictureError> {
    let raw =
        bytes.get(off..off + 4).ok_or_else(|| PictureError::new("the picture is truncated"))?;
    Ok(u32::from_le_bytes(raw.try_into().unwrap()))
}

fn take_rgba(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>, PictureError> {
    let expect = (width as usize).saturating_mul(height as usize).saturating_mul(4);
    let raw = data.get(..expect).ok_or_else(|| PictureError::new("the picture is truncated"))?;
    Ok(raw.to_vec())
}

fn masked_rgba(
    data: &[u8],
    width: u32,
    height: u32,
    masks: [u32; 4],
) -> Result<Vec<u8>, PictureError> {
    let expect = (width as usize).saturating_mul(height as usize).saturating_mul(4);
    let raw = data.get(..expect).ok_or_else(|| PictureError::new("the picture is truncated"))?;
    let mut out = Vec::with_capacity(expect);
    for pixel in raw.chunks_exact(4) {
        let value = u32::from_le_bytes(pixel.try_into().unwrap());
        let channel = |mask: u32, missing: u8| -> u8 {
            if mask == 0 {
                return missing;
            }
            ((value & mask) >> mask.trailing_zeros()) as u8
        };
        out.extend_from_slice(&[
            channel(masks[0], 0),
            channel(masks[1], 0),
            channel(masks[2], 0),
            channel(masks[3], 255),
        ]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::picture::decode_bytes;

    fn ktx2(format: u32, width: u32, height: u32, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&KTX2_MAGIC);
        out.extend_from_slice(&format.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&width.to_le_bytes());
        out.extend_from_slice(&height.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u64.to_le_bytes());
        out.extend_from_slice(&0u64.to_le_bytes());
        let offset = 80u64 + 24;
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&(data.len() as u64).to_le_bytes());
        out.extend_from_slice(&(data.len() as u64).to_le_bytes());
        out.extend_from_slice(data);
        out
    }

    fn dds_uncompressed(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
        let mut header = vec![0u8; 128];
        header[..4].copy_from_slice(b"DDS ");
        header[4..8].copy_from_slice(&124u32.to_le_bytes());
        header[12..16].copy_from_slice(&height.to_le_bytes());
        header[16..20].copy_from_slice(&width.to_le_bytes());
        header[76..80].copy_from_slice(&32u32.to_le_bytes());
        header[88..92].copy_from_slice(&32u32.to_le_bytes());
        for (i, mask) in [0x0000_00FFu32, 0x0000_FF00, 0x00FF_0000, 0xFF00_0000].iter().enumerate()
        {
            header[92 + i * 4..96 + i * 4].copy_from_slice(&mask.to_le_bytes());
        }
        header.extend_from_slice(rgba);
        header
    }

    fn dds_fourcc(width: u32, height: u32, fourcc: u32, data: &[u8]) -> Vec<u8> {
        let mut header = vec![0u8; 128];
        header[..4].copy_from_slice(b"DDS ");
        header[4..8].copy_from_slice(&124u32.to_le_bytes());
        header[12..16].copy_from_slice(&height.to_le_bytes());
        header[16..20].copy_from_slice(&width.to_le_bytes());
        header[76..80].copy_from_slice(&32u32.to_le_bytes());
        header[80..84].copy_from_slice(&4u32.to_le_bytes());
        header[84..88].copy_from_slice(&fourcc.to_le_bytes());
        header.extend_from_slice(data);
        header
    }

    fn solid_bc1_red() -> [u8; 8] {
        let mut block = [0u8; 8];
        block[..2].copy_from_slice(&0xF800u16.to_le_bytes());
        block
    }

    fn bc6_white() -> [u8; 16] {
        let mut bits = crate::picture_pack::Bits::new(16);
        bits.push(0b00011, 5);
        for _ in 0..6 {
            bits.push(512, 10);
        }
        let mut out = [0u8; 16];
        out.copy_from_slice(&bits.buf[..16]);
        out
    }

    #[test]
    fn uncompressed_and_block_pictures_match_their_pixels() {
        let rgba = [255u8, 0, 0, 255, 0, 255, 0, 255];
        let ktx = decode_bytes(&ktx2(37, 2, 1, &rgba)).unwrap();
        assert_eq!((ktx.width, ktx.height), (2, 1));
        assert_eq!(ktx.rgba, rgba);
        let dds = decode_bytes(&dds_uncompressed(2, 1, &rgba)).unwrap();
        assert_eq!(dds.rgba, rgba);

        let red = decode_bytes(&ktx2(133, 4, 4, &solid_bc1_red())).unwrap();
        assert!(red.rgba.chunks(4).all(|pixel| pixel == [255, 0, 0, 255]));
        let from_dds = decode_bytes(&dds_fourcc(4, 4, 0x3154_5844, &solid_bc1_red())).unwrap();
        assert_eq!(from_dds.rgba, red.rgba);

        let bc2 = {
            let mut block = [0xFFu8; 16];
            block[8..].copy_from_slice(&solid_bc1_red());
            block
        };
        let decoded = decode_bytes(&ktx2(135, 4, 4, &bc2)).unwrap();
        assert!(decoded.rgba.chunks(4).all(|pixel| pixel == [255, 0, 0, 255]));

        let mut bc3 = [0u8; 16];
        bc3[0] = 255;
        bc3[8..].copy_from_slice(&solid_bc1_red());
        let decoded = decode_bytes(&ktx2(137, 4, 4, &bc3)).unwrap();
        assert!(decoded.rgba.chunks(4).all(|pixel| pixel == [255, 0, 0, 255]));

        let mut bc4 = [0u8; 8];
        bc4[0] = 255;
        let decoded = decode_bytes(&ktx2(139, 4, 4, &bc4)).unwrap();
        assert!(decoded.rgba.chunks(4).all(|pixel| pixel == [255, 255, 255, 255]));

        let mut bc5 = [0u8; 16];
        bc5[0] = 255;
        bc5[8] = 128;
        let decoded = decode_bytes(&ktx2(141, 4, 4, &bc5)).unwrap();
        assert_eq!(&decoded.rgba[..4], &[255, 128, 0, 255]);

        let bc6 = decode_bytes(&ktx2(143, 4, 4, &bc6_white())).unwrap();
        assert!(bc6.rgba.chunks(4).all(|pixel| pixel[0] > 250 && pixel[1] > 250 && pixel[2] > 250));

        let packed = crate::picture_pack::pack("high", &[255, 0, 0, 255].repeat(16), 4, 4).unwrap();
        let bc7 = decode_bytes(&ktx2(145, 4, 4, &packed[21..])).unwrap();
        assert!(bc7.rgba.chunks(4).all(|pixel| {
            (i16::from(pixel[0]) - 255).abs() <= 2
                && pixel[1] <= 2
                && pixel[2] <= 2
                && (i16::from(pixel[3]) - 255).abs() <= 2
        }));
    }

    #[test]
    fn damaged_containers_are_errors() {
        for bytes in [b"DDS ".as_slice(), &KTX2_MAGIC, b"SEALPIC1", &[0xAB, 0x4B]] {
            let ran = std::panic::catch_unwind(|| decode_bytes(bytes));
            assert!(ran.is_ok(), "decoder panicked");
            assert!(ran.unwrap().is_err());
        }
    }
}
