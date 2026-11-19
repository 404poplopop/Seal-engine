//! Block texture encoders used at export and in the Asset Editor.
// The in-tree encoders are the fallback for Windows GNU builds, which have no intel_tex_2 kernels.
#![cfg_attr(not(all(windows, target_env = "gnu")), allow(dead_code))]

use crate::gpu_block_format::GpuBlockFormat;
use crate::picture::PictureError;
use crate::picture_pack::Block;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum EncodeQuality {
  #[default]
  Fast,
  Normal,
  Best,
}

impl EncodeQuality {
  pub fn parse(text: &str) -> Option<Self> {
    match text.to_ascii_lowercase().as_str() {
      "fast" | "low" => Some(Self::Fast),
      "normal" | "balanced" | "medium" => Some(Self::Normal),
      "best" | "maximum" | "high" => Some(Self::Best),
      _ => None,
    }
  }

  pub fn search_depth(self) -> usize {
    match self {
      Self::Fast => 1,
      Self::Normal => 2,
      Self::Best => 4,
    }
  }
}

/// Mean per-channel PSNR in dB between two same-sized RGBA buffers.
pub fn psnr_rgba(original: &[u8], decoded: &[u8]) -> f64 {
  if original.len() != decoded.len() || original.is_empty() {
    return 0.0;
  }
  let mut mse = 0.0_f64;
  for (a, b) in original.chunks_exact(4).zip(decoded.chunks_exact(4)) {
    for c in 0..3 {
      let d = f64::from(a[c]) - f64::from(b[c]);
      mse += d * d;
    }
  }
  let n = original.len() / 4 * 3;
  if n == 0 {
    return 0.0;
  }
  mse /= n as f64;
  if mse < 1e-12 {
    return 99.0;
  }
  10.0 * (255.0 * 255.0 / mse).log10()
}

pub fn has_transparency(rgba: &[u8]) -> bool {
  rgba.chunks_exact(4).any(|p| p[3] < 250)
}

pub fn encode_rgba(
  format: GpuBlockFormat,
  quality: EncodeQuality,
  width: u32,
  height: u32,
  rgba: &[u8],
) -> Result<Vec<u8>, PictureError> {
  if width == 0 || height == 0 {
    return Err(PictureError::new("the picture size is not usable"));
  }
  let expect = (width as usize) * (height as usize) * 4;
  if rgba.len() != expect {
    return Err(PictureError::new("the picture size is not usable"));
  }
  #[cfg(not(all(windows, target_env = "gnu")))]
  return crate::texture_encode_intel::encode_rgba_intel(format, quality, width, height, rgba);
  #[cfg(all(windows, target_env = "gnu"))]
  return encode_rgba_fallback(format, quality, width, height, rgba);
}

#[cfg(all(windows, target_env = "gnu"))]
pub(crate) fn encode_rgba_fallback(
  format: GpuBlockFormat,
  quality: EncodeQuality,
  width: u32,
  height: u32,
  rgba: &[u8],
) -> Result<Vec<u8>, PictureError> {
  let transparent = has_transparency(rgba);
  match format {
    GpuBlockFormat::Rgba8 => Ok(rgba.to_vec()),
    GpuBlockFormat::Bc1 => {
      if transparent {
        encode_bc1a(width, height, rgba, quality)
      } else {
        encode_bc1(width, height, rgba, quality)
      }
    }
    GpuBlockFormat::Bc2 => encode_bc2(width, height, rgba, quality),
    GpuBlockFormat::Bc3 => encode_bc3(width, height, rgba, quality),
    GpuBlockFormat::Bc4 => encode_bc4(width, height, rgba, quality),
    GpuBlockFormat::Bc5 => encode_bc5(width, height, rgba, quality),
    GpuBlockFormat::Bc6h => encode_bc6h(width, height, rgba, quality),
    GpuBlockFormat::Bc7 => encode_bc7(width, height, rgba, quality),
    GpuBlockFormat::Etc2 => {
      if transparent {
        encode_etc2_rgba(width, height, rgba, quality)
      } else {
        encode_etc2_rgb(width, height, rgba, quality)
      }
    }
    GpuBlockFormat::Astc4x4 => encode_astc(width, height, rgba, quality),
  }
}

pub fn decode_to_rgba(
  format: GpuBlockFormat,
  width: u32,
  height: u32,
  data: &[u8],
) -> Result<Vec<u8>, PictureError> {
  let block = gpu_block_to_pack_block(format, width, height, data)?;
  crate::picture_pack::decode_blocks(block, data, width, height)
}

fn gpu_block_to_pack_block(
  format: GpuBlockFormat,
  width: u32,
  height: u32,
  data: &[u8],
) -> Result<Block, PictureError> {
  Ok(match format {
    GpuBlockFormat::Bc1 => Block::Bc1,
    GpuBlockFormat::Bc2 => Block::Bc2,
    GpuBlockFormat::Bc3 => Block::Bc3,
    GpuBlockFormat::Bc4 => Block::Bc4,
    GpuBlockFormat::Bc5 => Block::Bc5,
    GpuBlockFormat::Bc6h => Block::Bc6,
    GpuBlockFormat::Bc7 => Block::Bc7,
    GpuBlockFormat::Etc2 => {
      let blocks = (width.div_ceil(4) * height.div_ceil(4)) as usize;
      if data.len() >= blocks * 16 {
        Block::Etc2Rgba
      } else {
        Block::Etc2Rgb
      }
    }
    GpuBlockFormat::Astc4x4 => Block::Astc4x4,
    GpuBlockFormat::Rgba8 => {
      return Err(PictureError::new("rgb8 is not block compressed"));
    }
  })
}

fn encode_bc1(
  width: u32,
  height: u32,
  rgba: &[u8],
  quality: EncodeQuality,
) -> Result<Vec<u8>, PictureError> {
  Ok(encode_blocks_manual(width, height, 8, rgba, |block| {
    encode_bc1_block(block, quality).to_vec()
  }))
}

fn encode_bc1a(
  width: u32,
  height: u32,
  rgba: &[u8],
  quality: EncodeQuality,
) -> Result<Vec<u8>, PictureError> {
  Ok(encode_blocks_manual(width, height, 8, rgba, |block| {
    encode_bc1a_block(block, quality).to_vec()
  }))
}

fn encode_bc2(
  width: u32,
  height: u32,
  rgba: &[u8],
  quality: EncodeQuality,
) -> Result<Vec<u8>, PictureError> {
  Ok(encode_blocks_manual(width, height, 16, rgba, |block| {
    let mut out = encode_bc2_alpha(block).to_vec();
    out.extend_from_slice(&encode_bc1_block(block, quality));
    out
  }))
}

fn encode_bc3(
  width: u32,
  height: u32,
  rgba: &[u8],
  quality: EncodeQuality,
) -> Result<Vec<u8>, PictureError> {
  Ok(encode_blocks_manual(width, height, 16, rgba, |block| {
    let mut out = encode_bc4_block(block, 3, quality).to_vec();
    out.extend_from_slice(&encode_bc1_block(block, quality));
    out
  }))
}

fn encode_bc4(
  width: u32,
  height: u32,
  rgba: &[u8],
  quality: EncodeQuality,
) -> Result<Vec<u8>, PictureError> {
  Ok(encode_blocks_manual(width, height, 8, rgba, |block| {
    encode_bc4_block(block, 0, quality).to_vec()
  }))
}

fn encode_bc5(
  width: u32,
  height: u32,
  rgba: &[u8],
  quality: EncodeQuality,
) -> Result<Vec<u8>, PictureError> {
  Ok(encode_blocks_manual(width, height, 16, rgba, |block| {
    let mut out = encode_bc4_block(block, 0, quality).to_vec();
    out.extend_from_slice(&encode_bc4_block(block, 1, quality));
    out
  }))
}

fn encode_bc6h(
  width: u32,
  height: u32,
  rgba: &[u8],
  quality: EncodeQuality,
) -> Result<Vec<u8>, PictureError> {
  Ok(encode_blocks_manual(width, height, 16, rgba, |block| {
    encode_bc6h_block(block, quality).to_vec()
  }))
}

fn encode_bc7(
  width: u32,
  height: u32,
  rgba: &[u8],
  quality: EncodeQuality,
) -> Result<Vec<u8>, PictureError> {
  Ok(encode_blocks_manual(width, height, 16, rgba, |block| {
    encode_bc7_block(block, quality).to_vec()
  }))
}

fn encode_etc2_rgb(
  width: u32,
  height: u32,
  rgba: &[u8],
  quality: EncodeQuality,
) -> Result<Vec<u8>, PictureError> {
  Ok(encode_blocks_manual(width, height, 8, rgba, |block| {
    encode_etc2_rgb_block(block, quality).to_vec()
  }))
}

fn encode_etc2_rgba(
  width: u32,
  height: u32,
  rgba: &[u8],
  quality: EncodeQuality,
) -> Result<Vec<u8>, PictureError> {
  Ok(encode_blocks_manual(width, height, 16, rgba, |block| {
    let mut out = encode_etc2_a8_block(block, quality).to_vec();
    out.extend_from_slice(&encode_etc2_rgb_block(block, quality));
    out
  }))
}

fn encode_astc(
  width: u32,
  height: u32,
  rgba: &[u8],
  quality: EncodeQuality,
) -> Result<Vec<u8>, PictureError> {
  Ok(encode_blocks_manual(width, height, 16, rgba, |block| {
    encode_astc_4x4_block(block, quality).to_vec()
  }))
}

pub(crate) fn encode_blocks_manual(
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

pub(crate) fn encode_bc1_block(pixels: &[[u8; 4]; 16], quality: EncodeQuality) -> [u8; 8] {
  let solid = pixels.iter().all(|pixel| pixel == &pixels[0]);
  if solid {
    let packed = pack565(pixels[0][0], pixels[0][1], pixels[0][2]);
    let (c0, c1): (u16, u16) = if packed > 0 { (packed, 0) } else { (0, 0) };
    let mut out = [0u8; 8];
    out[..2].copy_from_slice(&c0.to_le_bytes());
    out[2..4].copy_from_slice(&c1.to_le_bytes());
    return out;
  }
  let (c0, c1) = pick_bc1_endpoints(pixels, quality);
  bc1_from_endpoints(pixels, c0, c1)
}

pub(crate) fn encode_bc1a_block(pixels: &[[u8; 4]; 16], quality: EncodeQuality) -> [u8; 8] {
  let (c0, c1) = pick_bc1_endpoints(pixels, quality);
  bc1_from_endpoints_opaque_alpha(pixels, c0, c1)
}

fn bc1_from_endpoints(pixels: &[[u8; 4]; 16], c0: u16, c1: u16) -> [u8; 8] {
  let palette = bc1_palette(c0, c1);
  let mut indices = 0u32;
  for (i, pixel) in pixels.iter().enumerate() {
    let best = best_palette_index(pixel, &palette);
    indices |= (best as u32) << (2 * i);
  }
  let mut out = [0u8; 8];
  out[..2].copy_from_slice(&c0.to_le_bytes());
  out[2..4].copy_from_slice(&c1.to_le_bytes());
  out[4..].copy_from_slice(&indices.to_le_bytes());
  out
}

fn bc1_from_endpoints_opaque_alpha(pixels: &[[u8; 4]; 16], mut c0: u16, mut c1: u16) -> [u8; 8] {
  if c0 < c1 {
    std::mem::swap(&mut c0, &mut c1);
  }
  let palette = bc1_palette_opaque_alpha(c0, c1);
  let mut indices = 0u32;
  for (i, pixel) in pixels.iter().enumerate() {
    if pixel[3] < 128 {
      indices |= 3u32 << (2 * i);
      continue;
    }
    let best = best_palette_index(pixel, &palette);
    indices |= (best as u32) << (2 * i);
  }
  let mut out = [0u8; 8];
  out[..2].copy_from_slice(&c0.to_le_bytes());
  out[2..4].copy_from_slice(&c1.to_le_bytes());
  out[4..].copy_from_slice(&indices.to_le_bytes());
  out
}

fn pick_bc1_endpoints(pixels: &[[u8; 4]; 16], quality: EncodeQuality) -> (u16, u16) {
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
  if quality.search_depth() > 2 {
    let mut best_err = u32::MAX;
    let candidates: Vec<(u16, u16)> = pixels
      .iter()
      .flat_map(|a| pixels.iter().map(move |b| (pack565(a[0], a[1], a[2]), pack565(b[0], b[1], b[2]))))
      .take(16 * quality.search_depth())
      .collect();
    let mut best = (c0, c1);
    for &(a, b) in &candidates {
      let (c0, c1) = if a >= b { (a, b) } else { (b, a) };
      let pal = bc1_palette(c0, c1);
      let err: u32 = pixels.iter().map(|p| palette_err(p, &pal)).sum();
      if err < best_err {
        best_err = err;
        best = (c0, c1);
      }
    }
    return best;
  }
  (c0, c1)
}

pub(crate) fn encode_bc2_alpha(pixels: &[[u8; 4]; 16]) -> [u8; 8] {
  let mut out = [0u8; 8];
  for (i, pixel) in pixels.iter().enumerate() {
    let v = (pixel[3] >> 4) as u64;
    out[i / 2] |= (v << (4 * (i % 2))) as u8;
  }
  out
}

pub(crate) fn encode_bc4_block(pixels: &[[u8; 4]; 16], channel: usize, quality: EncodeQuality) -> [u8; 8] {
  let mut a0 = pixels[0][channel];
  let mut a1 = a0;
  for pixel in pixels {
    a0 = a0.max(pixel[channel]);
    a1 = a1.min(pixel[channel]);
  }
  if quality == EncodeQuality::Best {
    for pixel in pixels {
      a0 = a0.max(pixel[channel]);
      a1 = a1.min(pixel[channel]);
    }
  }
  if a0 == a1 && a0 > 0 {
    a1 = a0 - 1;
  }
  let palette = if a0 > a1 {
    let blend = |wa: u16, wb: u16| ((u16::from(a0) * wa + u16::from(a1) * wb) / 7) as u8;
    [
      a0,
      a1,
      blend(6, 1),
      blend(5, 2),
      blend(4, 3),
      blend(3, 4),
      blend(2, 5),
      blend(1, 6),
    ]
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

fn encode_bc6h_block(pixels: &[[u8; 4]; 16], _quality: EncodeQuality) -> [u8; 16] {
  let mut lo = [255u8; 3];
  let mut hi = [0u8; 3];
  for px in pixels {
    for ch in 0..3 {
      lo[ch] = lo[ch].min(px[ch]);
      hi[ch] = hi[ch].max(px[ch]);
    }
  }
  let center = |channel: usize| {
    let u = f32::from(lo[channel]) / 255.0;
    half_float(u)
  };
  let mut bits = crate::picture_pack::Bits::new(16);
  bits.push(0b00011, 5);
  for ch in 0..3 {
    bits.push(u32::from(center(ch)), 10);
    bits.push(u32::from(center(ch)), 10);
  }
  let mut out = [0u8; 16];
  out.copy_from_slice(&bits.buf[..16]);
  out
}

fn encode_bc7_block(pixels: &[[u8; 4]; 16], quality: EncodeQuality) -> [u8; 16] {
  let mut lo = [255u8; 4];
  let mut hi = [0u8; 4];
  for pixel in pixels {
    for channel in 0..4 {
      lo[channel] = lo[channel].min(pixel[channel]);
      hi[channel] = hi[channel].max(pixel[channel]);
    }
  }
  if quality != EncodeQuality::Fast {
    for pixel in pixels {
      for c in 0..4 {
        lo[c] = lo[c].min(pixel[c]);
        hi[c] = hi[c].max(pixel[c]);
      }
    }
  }
  let mut lo = snap_endpoint(lo);
  let mut hi = snap_endpoint(hi);
  let mut indices = [0u8; 16];
  for (i, pixel) in pixels.iter().enumerate() {
    indices[i] = best_bc7_index(pixel, lo, hi);
  }
  if indices[0] >= 8 {
    std::mem::swap(&mut lo, &mut hi);
    for index in &mut indices {
      *index = 15 - *index;
    }
  }
  let mut bits = crate::picture_pack::Bits::new(16);
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

fn encode_etc2_rgb_block(pixels: &[[u8; 4]; 16], quality: EncodeQuality) -> [u8; 8] {
  let mut lo = pixels[0];
  let mut hi = pixels[0];
  for px in pixels {
    for c in 0..3 {
      lo[c] = lo[c].min(px[c]);
      hi[c] = hi[c].max(px[c]);
    }
  }
  if quality != EncodeQuality::Fast {
    for px in pixels {
      for c in 0..3 {
        lo[c] = lo[c].min(px[c]);
        hi[c] = hi[c].max(px[c]);
      }
    }
  }
  let mid = [
    ((u16::from(lo[0]) + u16::from(hi[0])) / 2) as u8,
    ((u16::from(lo[1]) + u16::from(hi[1])) / 2) as u8,
    ((u16::from(lo[2]) + u16::from(hi[2])) / 2) as u8,
  ];
  let r1 = (mid[0] & 0xf8) | (mid[0] >> 5);
  let g1 = (mid[1] & 0xf8) | (mid[1] >> 5);
  let b1 = (mid[2] & 0xf8) | (mid[2] >> 5);
  let dr = (hi[0] as i16 - lo[0] as i16).clamp(-8, 7) as i8;
  let dg = (hi[1] as i16 - lo[1] as i16).clamp(-4, 3) as i8;
  let db = (hi[2] as i16 - lo[2] as i16).clamp(-4, 3) as i8;
  let r2 = (((r1 as i16) + i16::from(dr) * 8).clamp(0, 255) as u8 & 0xf8) | ((r1 >> 5) & 7);
  let g2 = (((g1 as i16) + i16::from(dg) * 8).clamp(0, 255) as u8 & 0xf8) | ((g1 >> 5) & 7);
  let b2 = (((b1 as i16) + i16::from(db) * 8).clamp(0, 255) as u8 & 0xf8) | ((b1 >> 5) & 7);
  let mut out = [0u8; 8];
  out[0] = r1;
  out[1] = (g1 & 0xe0) | ((b1 >> 3) & 0x1f);
  out[2] = r2;
  out[3] = ((g2 & 0xe0) | ((b2 >> 3) & 0x1f)) | 0x02;
  let table = 0u8;
  let mut indices = 0u64;
  for (i, px) in pixels.iter().enumerate() {
    let d1 = dist2(*px, [lo[0], lo[1], lo[2], 255]);
    let d2 = dist2(*px, [hi[0], hi[1], hi[2], 255]);
    let bit: u64 = if d1 <= d2 { 0 } else { 1 };
    indices |= bit << i;
  }
  out[4..8].copy_from_slice(&indices.to_le_bytes()[..4]);
  out[3] = out[3].wrapping_add(table);
  out
}

/// ETC2 RGBA8 alpha block (8 bytes, alpha codeword before RGB in each 16-byte tile).
pub(crate) fn encode_etc2_a8_block(pixels: &[[u8; 4]; 16], quality: EncodeQuality) -> [u8; 8] {
  const WRITE_ORDER_TABLE_REV: [usize; 16] =
    [15, 11, 7, 3, 14, 10, 6, 2, 13, 9, 5, 1, 12, 8, 4, 0];
  const ETC2_ALPHA_MOD_TABLE: [[i8; 8]; 16] = [
    [-3, -6, -9, -15, 2, 5, 8, 14],
    [-3, -7, -10, -13, 2, 6, 9, 12],
    [-2, -5, -8, -13, 1, 4, 7, 12],
    [-2, -4, -6, -13, 1, 3, 5, 12],
    [-3, -6, -8, -12, 2, 5, 7, 11],
    [-3, -7, -9, -11, 2, 6, 8, 10],
    [-4, -7, -8, -11, 3, 6, 7, 10],
    [-3, -5, -8, -11, 2, 4, 7, 10],
    [-2, -6, -8, -10, 1, 5, 7, 9],
    [-2, -5, -8, -10, 1, 4, 7, 9],
    [-2, -4, -8, -10, 1, 3, 7, 9],
    [-2, -5, -7, -10, 1, 4, 6, 9],
    [-3, -4, -7, -10, 2, 3, 6, 9],
    [-1, -2, -3, -10, 0, 1, 2, 9],
    [-4, -6, -8, -9, 3, 5, 7, 8],
    [-3, -5, -7, -9, 2, 4, 6, 8],
  ];

  let target: [u8; 16] = std::array::from_fn(|i| pixels[WRITE_ORDER_TABLE_REV[i]][3]);
  if target.iter().all(|&a| a == target[0]) {
    let mut flat = [0u8; 8];
    flat[0] = target[0];
    return flat;
  }

  let mult_hi = match quality {
    EncodeQuality::Fast => 9,
    EncodeQuality::Normal => 16,
    EncodeQuality::Best => 16,
  };
  let base_step = match quality {
    EncodeQuality::Fast => 4,
    _ => 1,
  };

  let mut best = [0u8; 8];
  let mut best_err = u64::MAX;
  for (table_idx, mods) in ETC2_ALPHA_MOD_TABLE.iter().enumerate() {
    for mult in 1i32..mult_hi {
      for base in (0u8..=255).step_by(base_step) {
        let mut indices = [0u8; 16];
        let mut err = 0u64;
        for (i, &t) in target.iter().enumerate() {
          let mut idx_best = 0u8;
          let mut pix_err = u32::MAX;
          for (idx, &m) in mods.iter().enumerate() {
            let v = (i32::from(base) + mult * i32::from(m)).clamp(0, 255);
            let d = (v - i32::from(t)).unsigned_abs();
            if d < pix_err {
              pix_err = d;
              idx_best = idx as u8;
            }
          }
          indices[i] = idx_best;
          err += u64::from(pix_err) * u64::from(pix_err);
        }
        if err < best_err {
          best_err = err;
          let byte1 = ((mult as u8) << 4) | (table_idx as u8);
          let mut low = 0u64;
          for (i, &code) in indices.iter().enumerate() {
            low |= u64::from(code & 7) << (3 * i);
          }
          let word =
            (u64::from(base) << 56) | (u64::from(byte1) << 48) | (low & ((1u64 << 48) - 1));
          best = word.to_be_bytes();
        }
      }
    }
  }
  best
}

fn encode_astc_4x4_block(pixels: &[[u8; 4]; 16], quality: EncodeQuality) -> [u8; 16] {
  let mut lo = [255u8; 4];
  let mut hi = [0u8; 4];
  for px in pixels {
    for c in 0..4 {
      lo[c] = lo[c].min(px[c]);
      hi[c] = hi[c].max(px[c]);
    }
  }
  if quality != EncodeQuality::Fast {
    for px in pixels {
      for c in 0..4 {
        lo[c] = lo[c].min(px[c]);
        hi[c] = hi[c].max(px[c]);
      }
    }
  }
  let mut block = [0u8; 16];
  block[0] = 0xFC;
  block[1] = 0x01;
  block[2] = 0x00;
  block[3] = 0x00;
  block[4] = lo[0];
  block[5] = lo[1];
  block[6] = lo[2];
  block[7] = lo[3];
  block[8] = hi[0];
  block[9] = hi[1];
  block[10] = hi[2];
  block[11] = hi[3];
  let mut weights = 0u64;
  for (i, px) in pixels.iter().enumerate() {
    let w = best_astc_weight(px, lo, hi);
    weights |= u64::from(w.min(15)) << (i * 4);
  }
  block[12..16].copy_from_slice(&weights.to_le_bytes()[..4]);
  block
}

fn best_astc_weight(pixel: &[u8; 4], lo: [u8; 4], hi: [u8; 4]) -> u8 {
  let mut best = 0u8;
  let mut best_err = u32::MAX;
  for w in 0u8..16 {
    let mut err = 0u32;
    for c in 0..4 {
      let v = ((16 - u32::from(w)) * u32::from(lo[c]) + u32::from(w) * u32::from(hi[c]) + 8) / 16;
      let d = v as i32 - i32::from(pixel[c]);
      err += (d * d) as u32;
    }
    if err < best_err {
      best_err = err;
      best = w;
    }
  }
  best
}

fn best_bc7_index(pixel: &[u8; 4], lo: [u8; 4], hi: [u8; 4]) -> u8 {
  let mut best = 0u8;
  let mut best_err = u32::MAX;
  for (index, weight) in WEIGHTS4.iter().copied().enumerate() {
    let mut err = 0u32;
    for channel in 0..4 {
      let value =
        ((64 - weight) * u32::from(lo[channel]) + weight * u32::from(hi[channel]) + 32) >> 6;
      let delta = value as i32 - i32::from(pixel[channel]);
      err += (delta * delta) as u32;
    }
    if err < best_err {
      best_err = err;
      best = index as u8;
    }
  }
  best
}

fn best_palette_index(pixel: &[u8; 4], palette: &[[u8; 4]; 4]) -> usize {
  let mut best = 0usize;
  let mut best_err = u32::MAX;
  for (index, color) in palette.iter().enumerate() {
    let err = dist2(*pixel, *color);
    if err < best_err {
      best_err = err;
      best = index;
    }
  }
  best
}

fn palette_err(pixel: &[u8; 4], palette: &[[u8; 4]; 4]) -> u32 {
  palette.iter().map(|c| dist2(*pixel, *c)).min().unwrap_or(u32::MAX)
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

fn bc1_palette_opaque_alpha(c0: u16, c1: u16) -> [[u8; 4]; 4] {
  let a = expand565(c0);
  let b = expand565(c1);
  [a, b, mix(a, b, 2, 1), mix(a, b, 1, 2)]
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
  (0..4)
    .map(|i| {
      let d = i32::from(a[i]) - i32::from(b[i]);
      (d * d) as u32
    })
    .sum()
}

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

fn half_float(value: f32) -> u16 {
  let bits = value.to_bits();
  let sign = (bits >> 31) & 1;
  let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
  let frac = (bits >> 13) & 0x3ff;
  if exp <= 0 {
    return (sign << 15) as u16;
  }
  if exp >= 31 {
    return ((sign << 15) | 0x7c00) as u16;
  }
  ((sign << 15) | ((exp as u32) << 10) | frac) as u16
}

pub fn export_block_format(
  platform: &str,
  compression: &str,
  gpu_format: Option<GpuBlockFormat>,
) -> Option<(GpuBlockFormat, EncodeQuality)> {
  if compression == "none" {
    return None;
  }
  let quality = match compression {
    "lossless" => EncodeQuality::Normal,
    "fast" | "low" => EncodeQuality::Fast,
    "balanced" | "medium" | "normal" => EncodeQuality::Normal,
    "maximum" | "high" | "best" => EncodeQuality::Best,
    _ => return None,
  };
  if compression == "lossless" {
    return Some((GpuBlockFormat::Rgba8, quality));
  }
  if let Some(fmt) = gpu_format {
    if fmt != GpuBlockFormat::Rgba8 {
      return Some((fmt, quality));
    }
  }
  let mobile = matches!(platform, "web" | "android" | "ios");
  Some(if mobile {
    (
      match quality {
        EncodeQuality::Fast => GpuBlockFormat::Etc2,
        EncodeQuality::Normal => GpuBlockFormat::Etc2,
        EncodeQuality::Best => GpuBlockFormat::Astc4x4,
      },
      quality,
    )
  } else {
    (
      match quality {
        EncodeQuality::Fast => GpuBlockFormat::Bc1,
        EncodeQuality::Normal => GpuBlockFormat::Bc3,
        EncodeQuality::Best => GpuBlockFormat::Bc7,
      },
      quality,
    )
  })
}

pub fn psnr_channel(original: &[u8], decoded: &[u8], channel: usize) -> f64 {
  if original.len() != decoded.len() || original.is_empty() {
    return 0.0;
  }
  let mut mse = 0.0_f64;
  let mut n = 0usize;
  for (a, b) in original.chunks_exact(4).zip(decoded.chunks_exact(4)) {
    let d = f64::from(a[channel]) - f64::from(b[channel]);
    mse += d * d;
    n += 1;
  }
  if n == 0 {
    return 0.0;
  }
  mse /= n as f64;
  if mse < 1e-12 {
    return 99.0;
  }
  10.0 * (255.0 * 255.0 / mse).log10()
}

#[cfg(test)]
fn gradient_noise(w: u32, h: u32) -> Vec<u8> {
  let mut out = vec![0u8; (w * h * 4) as usize];
  for y in 0..h {
    for x in 0..w {
      let i = ((y * w + x) * 4) as usize;
      let n = ((x * 17 + y * 31) % 23) as u8;
      out[i] = (x * 255 / w.max(1)) as u8;
      out[i + 1] = (y * 255 / h.max(1)) as u8;
      out[i + 2] = n;
      out[i + 3] = 255;
    }
  }
  out
}

#[cfg(test)]
fn etc2_rgba_gradient(w: u32, h: u32) -> Vec<u8> {
  let mut out = gradient_noise(w, h);
  for y in 0..h {
    for x in 0..w {
      let i = ((y * w + x) * 4) as usize;
      out[i + 3] = ((x + y) * 255 / (w + h).max(1)) as u8;
    }
  }
  out
}

#[cfg(test)]
fn hdr_linear_gradient(w: u32, h: u32) -> Vec<f32> {
  let mut out = vec![0f32; (w * h * 4) as usize];
  for y in 0..h {
    for x in 0..w {
      let i = ((y * w + x) * 4) as usize;
      out[i] = (x as f32 / w.max(1) as f32) * 8.0;
      out[i + 1] = (y as f32 / h.max(1) as f32) * 4.0;
      out[i + 2] = 1.0 + (x as f32 / w.max(1) as f32) * 2.0;
      out[i + 3] = 1.0;
    }
  }
  out
}

/// BC6H unsigned blocks → linear `f32` RGB (alpha 1), via `bcdec_rs::bc6h_float` (HDR half-linear).
#[cfg(test)]
fn decode_bc6h_linear(data: &[u8], width: u32, height: u32) -> Vec<f32> {
  let blocks_x = width.div_ceil(4) as usize;
  let blocks_y = height.div_ceil(4) as usize;
  let mut out = vec![0f32; (width * height * 4) as usize];
  for by in 0..blocks_y {
    for bx in 0..blocks_x {
      let bi = (by * blocks_x + bx) * 16;
      let block = &data[bi..bi + 16];
      let mut rgb = [0f32; 48];
      bcdec_rs::bc6h_float(block, &mut rgb, 12, false);
      for py in 0..4usize {
        for px in 0..4usize {
          let x = bx * 4 + px;
          let y = by * 4 + py;
          if x < width as usize && y < height as usize {
            let si = (py * 4 + px) * 3;
            let di = (y * width as usize + x) * 4;
            out[di] = rgb[si];
            out[di + 1] = rgb[si + 1];
            out[di + 2] = rgb[si + 2];
            out[di + 3] = 1.0;
          }
        }
      }
    }
  }
  out
}

/// Normalised RMSE on linear RGB (`f32` per channel) against peak 8.0; BC6H round-trip uses
/// `bcdec_rs::bc6h_float` for decode (HDR half-linear, not 8-bit proxy). Lower is better; aim below 0.08.
pub fn hdr_linear_nrmse(original: &[f32], decoded: &[f32]) -> f64 {
  assert_eq!(original.len(), decoded.len());
  let mut mse = 0.0_f64;
  let n = original.len() / 4 * 3;
  for i in 0..original.len() / 4 {
    for c in 0..3 {
      let d = f64::from(original[i * 4 + c]) - f64::from(decoded[i * 4 + c]);
      mse += d * d;
    }
  }
  (mse / n as f64).sqrt() / 8.0
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::picture::box_mip;

  #[test]
  fn block_formats_round_trip_above_psnr() {
    let w = 64;
    let h = 64;
    let rgba = gradient_noise(w, h);
    let etc_rgba = etc2_rgba_gradient(w, h);
    let mut measured = Vec::new();
    let cases: &[(GpuBlockFormat, &[u8], f64, bool)] = &[
      (GpuBlockFormat::Bc1, &rgba, 30.0, false),
      (GpuBlockFormat::Bc2, &rgba, 30.0, false),
      (GpuBlockFormat::Bc3, &rgba, 30.0, false),
      (GpuBlockFormat::Bc4, &rgba, 38.0, true),
      (GpuBlockFormat::Bc5, &rgba, 38.0, true),
      (GpuBlockFormat::Bc7, &rgba, 38.0, false),
      (GpuBlockFormat::Etc2, &rgba, 30.0, false),
      (GpuBlockFormat::Astc4x4, &rgba, 34.0, false),
    ];
    for (format, src, min, channel0) in cases {
      let data = encode_rgba(*format, EncodeQuality::Normal, w, h, src).expect("encode");
      let back = decode_to_rgba(*format, w, h, &data).expect("decode");
      let score = if *channel0 {
        psnr_channel(src, &back, 0)
      } else {
        psnr_rgba(src, &back)
      };
      measured.push((format.name(), score));
      assert!(score >= *min, "{} psnr {:.1} (need {:.0})", format.name(), score, min);
    }
    let etc_data =
      encode_rgba(GpuBlockFormat::Etc2, EncodeQuality::Normal, w, h, &etc_rgba).expect("encode");
    let etc_back = decode_to_rgba(GpuBlockFormat::Etc2, w, h, &etc_data).expect("decode");
    let etc_score = psnr_rgba(&etc_rgba, &etc_back);
    measured.push(("etc2_rgba", etc_score));
    assert!(etc_score >= 28.0, "etc2_rgba psnr {:.1}", etc_score);

    #[cfg(not(all(windows, target_env = "gnu")))]
    {
      let hdr = hdr_linear_gradient(w, h);
      let bc6 = crate::texture_encode_intel::encode_bc6h_linear_float(
        EncodeQuality::Normal,
        w,
        h,
        &hdr,
      )
      .expect("bc6 encode");
      let decoded = decode_bc6h_linear(&bc6, w, h);
      let nrmse = hdr_linear_nrmse(&hdr, &decoded);
      measured.push(("bc6h_nrmse", nrmse));
      assert!(nrmse < 0.08, "bc6h linear nRMSE {:.4}", nrmse);
    }

    for (name, value) in measured {
      eprintln!("measured {name}: {value:.2}");
    }
  }

  #[test]
  fn quality_levels_encode() {
    let rgba = gradient_noise(8, 8);
    for q in [EncodeQuality::Fast, EncodeQuality::Normal, EncodeQuality::Best] {
      assert!(!encode_rgba(GpuBlockFormat::Bc7, q, 8, 8, &rgba)
        .unwrap()
        .is_empty());
    }
  }

  #[test]
  fn mips_chain_encode() {
    let rgba = gradient_noise(8, 8);
    let (mip, w, h) = box_mip(&rgba, 8, 8);
    let _ =
      encode_rgba(GpuBlockFormat::Bc1, EncodeQuality::Fast, w, h, &mip).expect("mip encode");
  }
}