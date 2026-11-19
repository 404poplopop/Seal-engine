//! Soft glow for a flat picture.
//!
//! Bright pixels above `threshold` are pulled out, blurred by `radius` pixels
//! and added back with `intensity`. Values may sit above 1 before they are
//! stored in a picture, so a threshold of 1 waits for light that has blown
//! past white. A radius of 0 leaves the picture byte for byte.

/// Pull out the brightness above `threshold` from an RGBA8 picture.
/// Each pixel is three floats, red green blue, row by row from the top.
pub fn extract(rgba: &[u8], width: u32, height: u32, threshold: f32) -> Vec<f32> {
    let (w, h) = (width as usize, height as usize);
    let mut out = vec![0.0f32; w * h * 3];
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return out;
    }
    for i in 0..w * h {
        let p = i * 4;
        let rgb = [rgba[p] as f32 / 255.0, rgba[p + 1] as f32 / 255.0, rgba[p + 2] as f32 / 255.0];
        let lum = 0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2];
        if lum <= threshold || lum <= 1e-6 {
            continue;
        }
        let keep = (lum - threshold) / lum;
        let o = i * 3;
        out[o] = rgb[0] * keep;
        out[o + 1] = rgb[1] * keep;
        out[o + 2] = rgb[2] * keep;
    }
    out
}

fn box_blur(src: &[f32], width: usize, height: usize, radius: usize) -> Vec<f32> {
    if radius == 0 {
        return src.to_vec();
    }
    let mut horizontal = vec![0.0f32; src.len()];
    let span = radius as i32;
    let weight = (radius * 2 + 1) as f32;
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0.0f32; 3];
            for dx in -span..=span {
                let sx = (x as i32 + dx).clamp(0, width as i32 - 1) as usize;
                let i = (y * width + sx) * 3;
                sum[0] += src[i];
                sum[1] += src[i + 1];
                sum[2] += src[i + 2];
            }
            let o = (y * width + x) * 3;
            horizontal[o] = sum[0] / weight;
            horizontal[o + 1] = sum[1] / weight;
            horizontal[o + 2] = sum[2] / weight;
        }
    }
    let mut out = vec![0.0f32; src.len()];
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0.0f32; 3];
            for dy in -span..=span {
                let sy = (y as i32 + dy).clamp(0, height as i32 - 1) as usize;
                let i = (sy * width + x) * 3;
                sum[0] += horizontal[i];
                sum[1] += horizontal[i + 1];
                sum[2] += horizontal[i + 2];
            }
            let o = (y * width + x) * 3;
            out[o] = sum[0] / weight;
            out[o + 1] = sum[1] / weight;
            out[o + 2] = sum[2] / weight;
        }
    }
    out
}

/// Add a glow on top of an RGBA8 picture. `radius` of 0, or `intensity` of 0,
/// returns the same bytes.
pub fn apply(
    rgba: &[u8],
    width: u32,
    height: u32,
    threshold: f32,
    intensity: f32,
    radius: f32,
) -> Vec<u8> {
    if intensity <= 0.0 || radius <= 0.0 || width == 0 || height == 0 {
        return rgba.to_vec();
    }
    let (w, h) = (width as usize, height as usize);
    if rgba.len() < w * h * 4 {
        return rgba.to_vec();
    }
    let bright = extract(rgba, width, height, threshold);
    let blurred = box_blur(&bright, w, h, radius.round().max(0.0) as usize);
    let mut out = rgba.to_vec();
    for i in 0..w * h {
        let o = i * 3;
        let p = i * 4;
        for c in 0..3 {
            let v = rgba[p + c] as f32 / 255.0 + blurred[o + c] * intensity;
            out[p + c] = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_keeps_only_pixels_above_the_threshold() {
        let mut px = vec![0u8; 2 * 2 * 4];
        px[0..4].copy_from_slice(&[255, 255, 255, 255]);
        px[4..8].copy_from_slice(&[20, 20, 20, 255]);
        let got = extract(&px, 2, 2, 0.5);
        assert!(got[0] > 0.4, "the white pixel is kept: {}", got[0]);
        assert_eq!(&got[3..6], &[0.0, 0.0, 0.0], "a dim pixel is dropped");
        assert_eq!(extract(&px, 2, 2, 1.0)[0], 0.0, "full white is not above 1");
    }

    #[test]
    fn bloom_spreads_a_bright_pixel_and_a_zero_radius_is_a_copy() {
        let mut px = vec![0u8; 3 * 4];
        px[4..8].copy_from_slice(&[255, 255, 255, 255]);
        assert_eq!(apply(&px, 3, 1, 0.5, 1.0, 0.0), px);
        let glowed = apply(&px, 3, 1, 0.5, 1.0, 1.0);
        assert!(glowed[0] > 0, "the neighbour picks up glow: {}", glowed[0]);
        assert!(glowed[8] > 0, "the other neighbour picks up glow: {}", glowed[8]);
        assert!(glowed[4] >= px[4]);
    }
}
