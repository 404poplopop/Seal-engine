//! Packed picture written when a game is exported, and read back by the loader.

use crate::picture::{Picture, PictureError, MAX_SIDE};

pub const MAGIC: &[u8] = b"SEALPIC1";

/// `lossless`, `low`, `medium` or `high`. Anything else is not packed.
pub fn kind_code(name: &str) -> Option<u8> {
    match name {
        "lossless" => Some(1),
        "low" => Some(2),
        "medium" => Some(3),
        "high" => Some(4),
        _ => None,
    }
}

/// Pack RGBA8 pixels. `low` is the smallest block format, `medium` keeps alpha,
/// `high` keeps the most detail, `lossless` round-trips the bytes exactly.
pub fn pack(name: &str, rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, PictureError> {
    let kind = kind_code(name).ok_or_else(|| PictureError::new("unknown picture compression"))?;
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(PictureError::new("the picture size is not usable"));
    }
    let expect = (width as usize).saturating_mul(height as usize).saturating_mul(4);
    if rgba.len() != expect {
        return Err(PictureError::new("the picture size is not usable"));
    }
    let payload = match kind {
        1 => miniz_oxide::deflate::compress_to_vec(rgba, 6),
        2 => encode_bc1(rgba, width, height),
        3 => encode_bc3(rgba, width, height),
        4 => encode_bc7(rgba, width, height),
        _ => return Err(PictureError::new("unknown picture compression")),
    };
    if payload.len() > u32::MAX as usize {
        return Err(PictureError::new("the picture is too large"));
    }
    let mut out = Vec::with_capacity(21 + payload.len());
    out.extend_from_slice(MAGIC);
    out.push(kind);
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

pub(crate) fn unpack(bytes: &[u8]) -> Result<Picture, PictureError> {
    if bytes.len() < 21 || &bytes[..8] != MAGIC {
        return Err(PictureError::new("the picture could not be read"));
    }
    let kind = bytes[8];
    let width = u32::from_le_bytes(bytes[9..13].try_into().unwrap());
    let height = u32::from_le_bytes(bytes[13..17].try_into().unwrap());
    let len = u32::from_le_bytes(bytes[17..21].try_into().unwrap()) as usize;
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(PictureError::new("the picture size is not usable"));
    }
    let payload =
        bytes.get(21..21 + len).ok_or_else(|| PictureError::new("the picture is truncated"))?;
    let rgba = match kind {
        1 => {
            let limit = (width as usize).saturating_mul(height as usize).saturating_mul(4);
            miniz_oxide::inflate::decompress_to_vec_with_limit(payload, limit)
                .map_err(|_| PictureError::new("the picture could not be read"))?
        }
        2 => decode_blocks(Block::Bc1, payload, width, height)?,
        3 => decode_blocks(Block::Bc3, payload, width, height)?,
        4 => decode_blocks(Block::Bc7, payload, width, height)?,
        _ => return Err(PictureError::new("the picture could not be read")),
    };
    let expect = (width as usize).saturating_mul(height as usize).saturating_mul(4);
    if rgba.len() != expect {
        return Err(PictureError::new("the picture size is not usable"));
    }
    Ok(Picture { width, height, rgba, linear: None, linear_file: false, mesh: None })
}

#[derive(Clone, Copy)]
pub(crate) enum Block {
    Bc1,
    Bc1a,
    Bc2,
    Bc3,
    Bc4,
    Bc5,
    Bc6,
    Bc6Signed,
    Bc7,
    Etc2Rgb,
    Etc2Rgba,
    Astc4x4,
}

pub(crate) fn decode_blocks(
    block: Block,
    data: &[u8],
    width: u32,
    height: u32,
) -> Result<Vec<u8>, PictureError> {
    let pixels = (width as usize).saturating_mul(height as usize);
    if pixels == 0 || pixels > (MAX_SIDE as usize) * (MAX_SIDE as usize) {
        return Err(PictureError::new("the picture size is not usable"));
    }
    let mut image = vec![0u32; pixels];
    let result = match block {
        Block::Bc1 => {
            texture2ddecoder::decode_bc1(data, width as usize, height as usize, &mut image)
        }
        Block::Bc1a => {
            texture2ddecoder::decode_bc1a(data, width as usize, height as usize, &mut image)
        }
        Block::Bc2 => {
            texture2ddecoder::decode_bc2(data, width as usize, height as usize, &mut image)
        }
        Block::Bc3 => {
            texture2ddecoder::decode_bc3(data, width as usize, height as usize, &mut image)
        }
        Block::Bc4 => {
            texture2ddecoder::decode_bc4(data, width as usize, height as usize, &mut image)
        }
        Block::Bc5 => {
            texture2ddecoder::decode_bc5(data, width as usize, height as usize, &mut image)
        }
        Block::Bc6 => {
            texture2ddecoder::decode_bc6_unsigned(data, width as usize, height as usize, &mut image)
        }
        Block::Bc6Signed => {
            texture2ddecoder::decode_bc6_signed(data, width as usize, height as usize, &mut image)
        }
        Block::Bc7 => {
            texture2ddecoder::decode_bc7(data, width as usize, height as usize, &mut image)
        }
        Block::Etc2Rgb => {
            texture2ddecoder::decode_etc2_rgb(data, width as usize, height as usize, &mut image)
        }
        Block::Etc2Rgba => {
            texture2ddecoder::decode_etc2_rgba8(data, width as usize, height as usize, &mut image)
        }
        Block::Astc4x4 => {
            texture2ddecoder::decode_astc(data, width as usize, height as usize, 4, 4, &mut image)
        }
    };
    result.map_err(|_| PictureError::new("the picture could not be read"))?;
    let mut rgba = Vec::with_capacity(pixels * 4);
    for pixel in image {
        let bytes = pixel.to_le_bytes();
        let (b, g, r, a) = (bytes[0], bytes[1], bytes[2], bytes[3]);
        match block {
            Block::Bc4 => rgba.extend_from_slice(&[r, r, r, 255]),
            Block::Bc5 => rgba.extend_from_slice(&[r, g, 0, 255]),
            _ => rgba.extend_from_slice(&[r, g, b, a]),
        }
    }
    Ok(rgba)
}

fn encode_bc1(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    encode_blocks(width, height, 8, rgba, |block| encode_bc1_block(block).to_vec())
}

fn encode_bc3(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    encode_blocks(width, height, 16, rgba, |block| {
        let mut out = encode_bc4_block(block, 3).to_vec();
        out.extend_from_slice(&encode_bc1_block(block));
        out
    })
}

fn encode_bc7(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    encode_blocks(width, height, 16, rgba, |block| encode_bc7_block(block).to_vec())
}

fn encode_blocks(
    width: u32,
    height: u32,
    block_bytes: usize,
    rgba: &[u8],
    encode: impl Fn(&[[u8; 4]; 16]) -> Vec<u8>,
) -> Vec<u8> {
    let blocks_x = width.div_ceil(4);
    let blocks_y = height.div_ceil(4);
    let mut out = Vec::with_capacity((blocks_x * blocks_y) as usize * block_bytes);
    for by in 0..blocks_y {
        for bx in 0..blocks_x {
            let mut pixels = [[0u8; 4]; 16];
            for py in 0..4 {
                for px in 0..4 {
                    let x = bx * 4 + px;
                    let y = by * 4 + py;
                    if x < width && y < height {
                        let i = ((y * width + x) * 4) as usize;
                        pixels[(py * 4 + px) as usize] =
                            [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]];
                    }
                }
            }
            out.extend(encode(&pixels));
        }
    }
    out
}

fn encode_bc1_block(pixels: &[[u8; 4]; 16]) -> [u8; 8] {
    let solid = pixels.iter().all(|pixel| pixel == &pixels[0]);
    if solid {
        let packed = pack565(pixels[0][0], pixels[0][1], pixels[0][2]);
        let (c0, c1): (u16, u16) = if packed > 0 { (packed, 0) } else { (0, 0) };
        let mut out = [0u8; 8];
        out[..2].copy_from_slice(&c0.to_le_bytes());
        out[2..4].copy_from_slice(&c1.to_le_bytes());
        return out;
    }
    let mut lo = pixels[0];
    let mut hi = pixels[0];
    for pixel in pixels {
        if luma(*pixel) < luma(lo) {
            lo = *pixel;
        }
        if luma(*pixel) > luma(hi) {
            hi = *pixel;
        }
    }
    let mut c0 = pack565(hi[0], hi[1], hi[2]);
    let mut c1 = pack565(lo[0], lo[1], lo[2]);
    if c0 < c1 {
        std::mem::swap(&mut c0, &mut c1);
    }
    if c0 == c1 && c0 > 0 {
        c1 = 0;
    }
    let palette = bc1_palette(c0, c1);
    let mut indices = 0u32;
    for (i, pixel) in pixels.iter().enumerate() {
        let mut best = 0usize;
        let mut best_err = u32::MAX;
        for (index, color) in palette.iter().enumerate() {
            let err = dist2(*pixel, *color);
            if err < best_err {
                best_err = err;
                best = index;
            }
        }
        indices |= (best as u32) << (2 * i);
    }
    let mut out = [0u8; 8];
    out[..2].copy_from_slice(&c0.to_le_bytes());
    out[2..4].copy_from_slice(&c1.to_le_bytes());
    out[4..].copy_from_slice(&indices.to_le_bytes());
    out
}

fn bc1_palette(c0: u16, c1: u16) -> [[u8; 4]; 4] {
    let a = expand565(c0);
    let b = expand565(c1);
    if c0 > c1 {
        [a, b, mix(a, b, 2, 1), mix(a, b, 1, 2)]
    } else {
        [a, b, mix(a, b, 1, 1), [0, 0, 0, 255]]
    }
}

fn mix(a: [u8; 4], b: [u8; 4], wa: u16, wb: u16) -> [u8; 4] {
    let ch = |i: usize| ((u16::from(a[i]) * wa + u16::from(b[i]) * wb) / (wa + wb)) as u8;
    [ch(0), ch(1), ch(2), 255]
}

fn expand565(d: u16) -> [u8; 4] {
    let r = ((d >> 8) & 0xf8) as u8 | (d >> 13) as u8;
    let g = ((d >> 3) & 0xfc) as u8 | ((d >> 9) & 3) as u8;
    let b = (d << 3) as u8 | ((d >> 2) & 7) as u8;
    [r, g, b, 255]
}

fn pack565(r: u8, g: u8, b: u8) -> u16 {
    (u16::from(r) >> 3 << 11) | (u16::from(g) >> 2 << 5) | (u16::from(b) >> 3)
}

fn luma(pixel: [u8; 4]) -> u32 {
    u32::from(pixel[0]) * 3 + u32::from(pixel[1]) * 6 + u32::from(pixel[2])
}

fn dist2(a: [u8; 4], b: [u8; 4]) -> u32 {
    (0..3)
        .map(|i| {
            let d = i32::from(a[i]) - i32::from(b[i]);
            (d * d) as u32
        })
        .sum()
}

fn encode_bc4_block(pixels: &[[u8; 4]; 16], channel: usize) -> [u8; 8] {
    let mut a0 = pixels[0][channel];
    let mut a1 = a0;
    for pixel in pixels {
        a0 = a0.max(pixel[channel]);
        a1 = a1.min(pixel[channel]);
    }
    if a0 == a1 && a0 > 0 {
        a1 = a0 - 1;
    }
    let palette = if a0 > a1 {
        let blend = |wa: u16, wb: u16| ((u16::from(a0) * wa + u16::from(a1) * wb) / 7) as u8;
        [a0, a1, blend(6, 1), blend(5, 2), blend(4, 3), blend(3, 4), blend(2, 5), blend(1, 6)]
    } else {
        let blend = |wa: u16, wb: u16| ((u16::from(a0) * wa + u16::from(a1) * wb) / 5) as u8;
        [a0, a1, blend(4, 1), blend(3, 2), blend(2, 3), blend(1, 4), 0, 255]
    };
    let mut bits = u64::from(a0) | (u64::from(a1) << 8);
    for (i, pixel) in pixels.iter().enumerate() {
        let mut best = 0usize;
        let mut best_err = u32::MAX;
        for (index, value) in palette.iter().enumerate() {
            let d = i32::from(*value) - i32::from(pixel[channel]);
            let err = (d * d) as u32;
            if err < best_err {
                best_err = err;
                best = index;
            }
        }
        bits |= (best as u64) << (16 + 3 * i);
    }
    bits.to_le_bytes()
}

const WEIGHTS4: [u32; 16] = [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64];

fn snap_endpoint(endpoint: [u8; 4]) -> [u8; 4] {
    let cost = |bit: u8| {
        endpoint
            .iter()
            .map(|channel| {
                let stored = (channel & !1) | bit;
                let delta = i32::from(stored) - i32::from(*channel);
                (delta * delta) as u32
            })
            .sum::<u32>()
    };
    let bit = if cost(1) < cost(0) { 1 } else { 0 };
    endpoint.map(|channel| (channel & !1) | bit)
}

fn encode_bc7_block(pixels: &[[u8; 4]; 16]) -> [u8; 16] {
    let mut lo = [255u8; 4];
    let mut hi = [0u8; 4];
    for pixel in pixels {
        for channel in 0..4 {
            lo[channel] = lo[channel].min(pixel[channel]);
            hi[channel] = hi[channel].max(pixel[channel]);
        }
    }
    let mut lo = snap_endpoint(lo);
    let mut hi = snap_endpoint(hi);
    let mut indices = [0u8; 16];
    for (i, pixel) in pixels.iter().enumerate() {
        let mut best = 0u8;
        let mut best_err = u32::MAX;
        for (index, weight) in WEIGHTS4.iter().copied().enumerate() {
            let mut err = 0u32;
            for channel in 0..4 {
                let value =
                    ((64 - weight) * u32::from(lo[channel]) + weight * u32::from(hi[channel]) + 32)
                        >> 6;
                let delta = value as i32 - i32::from(pixel[channel]);
                err += (delta * delta) as u32;
            }
            if err < best_err {
                best_err = err;
                best = index as u8;
            }
        }
        indices[i] = best;
    }
    if indices[0] >= 8 {
        std::mem::swap(&mut lo, &mut hi);
        for index in &mut indices {
            *index = 15 - *index;
        }
    }
    let mut bits = Bits::new(16);
    bits.push(0, 6);
    bits.push(1, 1);
    for channel in 0..4 {
        bits.push(u32::from(lo[channel]) >> 1, 7);
        bits.push(u32::from(hi[channel]) >> 1, 7);
    }
    bits.push(u32::from(lo[0] & 1), 1);
    bits.push(u32::from(hi[0] & 1), 1);
    bits.push(u32::from(indices[0]), 3);
    for index in &indices[1..] {
        bits.push(u32::from(*index), 4);
    }
    let mut out = [0u8; 16];
    out.copy_from_slice(&bits.buf[..16]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compression_round_trips_within_tolerance() {
        let solid = [255u8, 0, 0, 255].repeat(16);
        for kind in ["lossless", "low", "medium", "high"] {
            let packed = pack(kind, &solid, 4, 4).unwrap();
            let back = unpack(&packed).unwrap();
            let limit = if kind == "lossless" { 0 } else { 2 };
            for (got, expect) in back.rgba.iter().zip(&solid) {
                assert!(
                    (i16::from(*got) - i16::from(*expect)).abs() <= limit,
                    "{kind} {got} {expect}"
                );
            }
        }
        let mut ramp = Vec::new();
        for i in 0..16u8 {
            let value = i.saturating_mul(16);
            ramp.extend_from_slice(&[value, 0, 0, 255]);
        }
        let packed = pack("high", &ramp, 4, 4).unwrap();
        let back = unpack(&packed).unwrap();
        for (got, expect) in back.rgba.iter().zip(&ramp) {
            assert!((i16::from(*got) - i16::from(*expect)).abs() <= 24, "{got} {expect}");
        }
        assert!(std::panic::catch_unwind(|| unpack(b"SEALPIC1")).unwrap().is_err());
        assert!(unpack(&[0, 1, 2, 3]).is_err());
    }
}

pub(crate) struct Bits {
    pub(crate) buf: Vec<u8>,
    pos: usize,
}

impl Bits {
    pub(crate) fn new(bytes: usize) -> Self {
        Self { buf: vec![0; bytes], pos: 0 }
    }

    pub(crate) fn push(&mut self, value: u32, n: usize) {
        for bit in 0..n {
            if (value >> bit) & 1 == 1 {
                let index = self.pos / 8;
                if index < self.buf.len() {
                    self.buf[index] |= 1 << (self.pos % 8);
                }
            }
            self.pos += 1;
        }
    }
}
