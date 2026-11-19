//! One picture loader for the editor, the Asset Editor and a running game.
//!
//! PNG, JPEG, BMP, GIF (first frame), TGA, WebP, Radiance HDR, EXR, SVG,
//! KTX2 and DDS. Float pictures keep linear samples and a tone-mapped byte
//! copy. Other formats are straight RGBA8. A damaged file returns an error.

use std::io::Cursor;
use std::path::Path;

use image::{ColorType, DynamicImage, ImageFormat, ImageReader, Limits};

use crate::device::{ColourSpace, TextureDesc, TextureFilter, TextureWrap};

/// File extensions this loader decodes.
pub const EXTENSIONS: &[&str] =
    &["png", "jpg", "jpeg", "bmp", "gif", "tga", "webp", "hdr", "exr", "svg", "ktx2", "dds"];

/// Longest side kept on import. `0` leaves the file's size. The list is powers of two.
pub const MAX_SIZES: &[u32] = &[32, 64, 128, 256, 512, 1024, 2048, 4096];

pub use crate::picture_mesh::{
    covers_opaque, polygon_area, read_mesh_text, tight_mesh, write_mesh_text,
};
pub use crate::picture_pack::pack;

pub(crate) const MAX_SIDE: u32 = 8192;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PictureError {
    pub message: String,
}

impl PictureError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl std::fmt::Display for PictureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PictureError {}

/// A decoded picture.
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    /// Straight RGBA8. HDR is the linear value clamped into 0..1.
    pub rgba: Vec<u8>,
    /// Linear RGBA, only for a float picture (HDR or EXR).
    pub linear: Option<Vec<f32>>,
    /// The file stores linear light (HDR or EXR).
    pub linear_file: bool,
    /// Tight sprite mesh in cell space, y up, when a mesh file sits beside the picture.
    pub mesh: Option<Vec<[f32; 2]>>,
}

/// How a picture is sampled once it is on the device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PictureSampling {
    pub filter: TextureFilter,
    pub wrap: TextureWrap,
    pub colour_space: ColourSpace,
    pub mipmaps: bool,
}

impl Default for PictureSampling {
    fn default() -> Self {
        Self {
            filter: TextureFilter::Nearest,
            wrap: TextureWrap::Clamp,
            colour_space: ColourSpace::Srgb,
            mipmaps: false,
        }
    }
}

impl PictureSampling {
    pub fn apply(self, desc: &mut TextureDesc) {
        desc.filter = self.filter;
        desc.wrap = self.wrap;
        desc.colour_space = self.colour_space;
        desc.mipmaps = self.mipmaps;
    }
}

pub fn is_extension(ext: &str) -> bool {
    let ext = ext.trim().trim_start_matches('.').to_ascii_lowercase();
    EXTENSIONS.contains(&ext.as_str())
}

pub fn is_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(is_extension)
}

fn format_of(ext: &str) -> Option<ImageFormat> {
    match ext.trim().trim_start_matches('.').to_ascii_lowercase().as_str() {
        "png" => Some(ImageFormat::Png),
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "bmp" => Some(ImageFormat::Bmp),
        "gif" => Some(ImageFormat::Gif),
        "tga" => Some(ImageFormat::Tga),
        "webp" => Some(ImageFormat::WebP),
        "hdr" => Some(ImageFormat::Hdr),
        "exr" => Some(ImageFormat::OpenExr),
        _ => None,
    }
}

/// Width and height from the file header only (no pixel decode).
pub fn file_dimensions(path: &Path) -> Result<(u32, u32), PictureError> {
    image::image_dimensions(path).map_err(|e| PictureError::new(e.to_string()))
}

/// Decode a file. The extension picks the format; a failed guess is an error.
pub fn decode_path(path: &Path) -> Result<Picture, PictureError> {
    decode_path_with(path, 0)
}

/// Like [`decode_path`]. `svg_size` is the longer side used when the file is SVG.
/// `0` uses 256. A packed sibling (`name.ext.sealpic`) wins over the source file.
pub fn decode_path_with(path: &Path, svg_size: u32) -> Result<Picture, PictureError> {
    let packed = pack_sidecar(path);
    let ktx2 = ktx2_sidecar(path);
    let (bytes, ext) = if packed.is_file() {
        let bytes = std::fs::read(&packed).map_err(|e| PictureError::new(e.to_string()))?;
        (bytes, "sealpic".to_string())
    } else if ktx2.is_file() {
        let bytes = std::fs::read(&ktx2).map_err(|e| PictureError::new(e.to_string()))?;
        (bytes, "ktx2".to_string())
    } else {
        let bytes = std::fs::read(path).map_err(|e| PictureError::new(e.to_string()))?;
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
        (bytes, ext)
    };
    let mut picture = decode_known(&bytes, &ext, svg_size)?;
    if let Some(mesh) =
        crate::picture_mesh::read_mesh_file(&crate::picture_mesh::mesh_sidecar(path))
    {
        picture.mesh = Some(mesh);
    }
    Ok(picture)
}

pub(crate) fn pack_sidecar(path: &Path) -> std::path::PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".sealpic");
    std::path::PathBuf::from(name)
}

pub(crate) fn ktx2_sidecar(path: &Path) -> std::path::PathBuf {
    path.with_extension("ktx2")
}

fn decode_known(bytes: &[u8], ext: &str, svg_size: u32) -> Result<Picture, PictureError> {
    if bytes.is_empty() {
        return Err(PictureError::new("the picture is empty"));
    }
    let ext = ext.trim().trim_start_matches('.').to_ascii_lowercase();
    if ext == "sealpic" || bytes.starts_with(crate::picture_pack::MAGIC) {
        return crate::picture_pack::unpack(bytes);
    }
    if ext == "svg" || (ext.is_empty() && looks_like_svg(bytes)) {
        return crate::picture_svg::raster(bytes, svg_size);
    }
    if ext == "ktx2" || (ext.is_empty() && bytes.starts_with(&crate::picture_blocks::KTX2_MAGIC)) {
        return crate::picture_blocks::decode_ktx2(bytes);
    }
    if ext == "dds" || (ext.is_empty() && bytes.starts_with(b"DDS ")) {
        return crate::picture_blocks::decode_dds(bytes);
    }
    decode_bytes_with(bytes, format_of(&ext))
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    let text = bytes.split(|b| b.is_ascii_whitespace()).find(|part| !part.is_empty());
    matches!(text, Some(part) if part.starts_with(b"<svg") || part.starts_with(b"<SVG") || part.starts_with(b"<?xml"))
}

/// Decode bytes, guessing the format from the header.
pub fn decode_bytes(bytes: &[u8]) -> Result<Picture, PictureError> {
    decode_known(bytes, "", 0)
}

pub(crate) fn decode_bytes_with(
    bytes: &[u8],
    format: Option<ImageFormat>,
) -> Result<Picture, PictureError> {
    if bytes.is_empty() {
        return Err(PictureError::new("the picture is empty"));
    }
    let hinted = decode_reader(bytes, format);
    if hinted.is_ok() || format.is_none() {
        return hinted;
    }
    // A wrong extension still tries the header.
    match decode_reader(bytes, None) {
        Ok(picture) => Ok(picture),
        Err(_) => hinted,
    }
}

fn decode_reader(bytes: &[u8], format: Option<ImageFormat>) -> Result<Picture, PictureError> {
    let mut reader = ImageReader::new(Cursor::new(bytes));
    if let Some(format) = format {
        reader.set_format(format);
    } else {
        reader = reader.with_guessed_format().map_err(|e| PictureError::new(e.to_string()))?;
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(|e| PictureError::new(e.to_string()))?;
    picture_from_image(image)
}

fn picture_from_image(image: DynamicImage) -> Result<Picture, PictureError> {
    let (width, height) = (image.width(), image.height());
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(PictureError::new("the picture size is not usable"));
    }
    let pixels = (width as usize).saturating_mul(height as usize);
    if pixels > (MAX_SIDE as usize) * (MAX_SIDE as usize) {
        return Err(PictureError::new("the picture size is not usable"));
    }
    let linear_file = matches!(image.color(), ColorType::Rgb32F | ColorType::Rgba32F);
    if linear_file {
        let buffer = image.into_rgba32f();
        let raw = buffer.into_raw();
        if raw.len() != pixels * 4 {
            return Err(PictureError::new("the picture size is not usable"));
        }
        let rgba = rgba_from_linear(&raw);
        return Ok(Picture {
            width,
            height,
            rgba,
            linear: Some(raw),
            linear_file: true,
            mesh: None,
        });
    }
    let buffer = image.into_rgba8();
    let rgba = buffer.into_raw();
    if rgba.len() != pixels * 4 {
        return Err(PictureError::new("the picture size is not usable"));
    }
    Ok(Picture { width, height, rgba, linear: None, linear_file: false, mesh: None })
}

/// Tone-map linear samples into 0..1. When nothing is brighter than 1 the
/// result is the sample itself, so an existing HDR file keeps its byte copy.
fn rgba_from_linear(raw: &[f32]) -> Vec<u8> {
    let mut peak = 1.0f32;
    for pixel in raw.chunks_exact(4) {
        for channel in &pixel[..3] {
            if channel.is_finite() {
                peak = peak.max(*channel);
            }
        }
    }
    let white2 = peak * peak;
    let mut out = Vec::with_capacity(raw.len());
    for pixel in raw.chunks_exact(4) {
        out.extend_from_slice(&[
            byte_of(tone(pixel[0], white2)),
            byte_of(tone(pixel[1], white2)),
            byte_of(tone(pixel[2], white2)),
            byte_of(pixel[3]),
        ]);
    }
    out
}

fn tone(value: f32, white2: f32) -> f32 {
    if !value.is_finite() || value <= 0.0 || white2 <= 0.0 {
        return 0.0;
    }
    let mapped = (value * (1.0 + value / white2)) / (1.0 + value);
    if mapped.is_finite() {
        mapped
    } else {
        1.0
    }
}

fn byte_of(value: f32) -> u8 {
    if !value.is_finite() {
        return 0;
    }
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Shrink so the longer side is at most `max_size`. `0` keeps the original.
/// The filter is a box average, so the same file always gives the same pixels.
pub fn downscale(rgba: &[u8], width: u32, height: u32, max_size: u32) -> (Vec<u8>, u32, u32) {
    if width == 0 || height == 0 || max_size == 0 || width.max(height) <= max_size {
        return (rgba.to_vec(), width, height);
    }
    let expect = (width as usize).saturating_mul(height as usize).saturating_mul(4);
    if rgba.len() != expect {
        return (rgba.to_vec(), width, height);
    }
    let scale = width.max(height) as f32 / max_size as f32;
    let nw = ((width as f32 / scale).round() as u32).max(1);
    let nh = ((height as f32 / scale).round() as u32).max(1);
    if (nw, nh) == (width, height) {
        return (rgba.to_vec(), width, height);
    }
    let mut out = Vec::with_capacity((nw * nh * 4) as usize);
    for y in 0..nh {
        let y0 = y * height / nh;
        let y1 = ((y + 1) * height / nh).max(y0 + 1);
        for x in 0..nw {
            let x0 = x * width / nw;
            let x1 = ((x + 1) * width / nw).max(x0 + 1);
            let mut sum = [0u32; 4];
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let i = ((sy * width + sx) * 4) as usize;
                    for channel in 0..4 {
                        sum[channel] += u32::from(rgba[i + channel]);
                    }
                }
            }
            let n = (y1 - y0) * (x1 - x0);
            out.extend(sum.map(|v| (v / n) as u8));
        }
    }
    (out, nw, nh)
}

/// Write RGBA8 to `path`. `format` is `png`, `jpeg`, `webp`, or `keep` (uses the file extension).
pub fn write_rgba8(
    path: &Path,
    rgba: &[u8],
    width: u32,
    height: u32,
    format: &str,
    compression: &str,
) -> Result<(), PictureError> {
    if width == 0 || height == 0 || rgba.len() != (width * height * 4) as usize {
        return Err(PictureError::new("picture size does not match the pixel buffer"));
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    let format_key = if format == "keep" {
        if ext == "jpg" { "jpeg" } else { ext.as_str() }
    } else {
        format
    };
    let image_format = match format_key {
        "png" => ImageFormat::Png,
        "jpeg" => ImageFormat::Jpeg,
        "webp" => ImageFormat::WebP,
        other => return Err(PictureError::new(format!("format {other} is not supported for export"))),
    };
    let image = image::RgbaImage::from_raw(width, height, rgba.to_vec())
        .ok_or_else(|| PictureError::new("picture pixels could not be turned into an image"))?;
    let dynamic = DynamicImage::ImageRgba8(image);
    let mut bytes = Vec::new();
    let _compression = compression;
    dynamic
        .write_to(&mut std::io::Cursor::new(&mut bytes), image_format)
        .map_err(|e| PictureError::new(e.to_string()))?;
    std::fs::write(path, bytes).map_err(|e| PictureError::new(e.to_string()))
}

/// One mip level down: the longer side is halved, at least one pixel.
pub fn box_mip(rgba: &[u8], width: u32, height: u32) -> (Vec<u8>, u32, u32) {
    let max_side = (width.max(height) / 2).max(1);
    downscale(rgba, width, height, max_side)
}

/// Encode a linear 8-bit channel for an sRGB display. Square root, so it is
/// the same on every machine. Alpha is left alone by the caller.
pub fn linear_channel_to_display(channel: u8) -> u8 {
    let u = channel as f32 / 255.0;
    (u.sqrt() * 255.0).round() as u8
}

pub fn display_linear_rgba(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for pixel in rgba.chunks_exact(4) {
        out.extend_from_slice(&[
            linear_channel_to_display(pixel[0]),
            linear_channel_to_display(pixel[1]),
            linear_channel_to_display(pixel[2]),
            pixel[3],
        ]);
    }
    out
}

/// Texture coordinate wrapped into `0..size`. Size 0 or 1 stays on pixel 0.
pub fn wrap_index(x: i64, size: i64, wrap: TextureWrap) -> i64 {
    if size <= 1 {
        return 0;
    }
    match wrap {
        TextureWrap::Clamp => x.clamp(0, size - 1),
        TextureWrap::Repeat => x.rem_euclid(size),
        TextureWrap::Mirror => {
            let period = size * 2;
            let m = x.rem_euclid(period);
            if m < size {
                m
            } else {
                period - 1 - m
            }
        }
    }
}

/// Use the half-resolution mip when the picture is drawn at half size or smaller.
pub fn use_mip(mipmaps: bool, texture_width: f32, uv_span: f32, pixel_span: f32) -> bool {
    mipmaps && uv_span * texture_width >= pixel_span.max(1.0) * 2.0
}

/// Square cells in a row, when the width is a whole number of heights.
/// That is the sheet layout a sprite already uses for frames laid out left to right.
pub fn sheet_frames(width: u32, height: u32) -> u32 {
    if height >= 1 && width > height && width % height == 0 {
        (width / height).min(256)
    } else {
        1
    }
}

/// Frame `index` inside a horizontal sheet. `full` may already be flipped.
pub fn sheet_uv(full: [f32; 4], frames: u32, index: u32) -> [f32; 4] {
    if frames <= 1 {
        return full;
    }
    let index = index % frames;
    let flipped = full[0] > full[2];
    let (u_lo, u_hi) = if flipped { (full[2], full[0]) } else { (full[0], full[2]) };
    let span = u_hi - u_lo;
    let a = u_lo + span * (index as f32 / frames as f32);
    let b = u_lo + span * ((index + 1) as f32 / frames as f32);
    let (u0, u1) = if flipped { (b, a) } else { (a, b) };
    [u0, full[1], u1, full[3]]
}

pub fn sheet_index(frames: u32, seconds: f32, fps: f32) -> u32 {
    if frames <= 1 || !seconds.is_finite() || seconds <= 0.0 || !fps.is_finite() || fps <= 0.0 {
        return 0;
    }
    ((seconds * fps).floor() as u64 % frames as u64) as u32
}

/// Pivot in sprite space: 0,0 is the bottom left, 0.5,0.5 the centre.
pub fn pivot_xy(name: &str, custom: [f32; 2]) -> [f32; 2] {
    let point = match name {
        "top_left" => [0.0, 1.0],
        "top" => [0.5, 1.0],
        "top_right" => [1.0, 1.0],
        "left" => [0.0, 0.5],
        "right" => [1.0, 0.5],
        "bottom_left" => [0.0, 0.0],
        "bottom" => [0.5, 0.0],
        "bottom_right" => [1.0, 0.0],
        "custom" => custom,
        _ => [0.5, 0.5],
    };
    point.map(|v| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.5 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat};

    fn assert_rgba(bytes: &[u8], format: Option<ImageFormat>, expect: &[u8], w: u32, h: u32) {
        let picture = decode_bytes_with(bytes, format).unwrap();
        assert_eq!((picture.width, picture.height), (w, h));
        assert_eq!(picture.rgba, expect);
        assert!(!picture.linear_file);
    }

    #[test]
    fn tga_bmp_and_gif_match_their_pixels() {
        // 2×1 TGA, 24-bit, bottom origin: red then green.
        let tga = [0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 1, 0, 24, 0, 0, 0, 255, 0, 255, 0];
        assert_rgba(&tga, Some(ImageFormat::Tga), &[255, 0, 0, 255, 0, 255, 0, 255], 2, 1);
        // 1×1 BMP and GIF, red, written by the format encoders and read back.
        let red = [255u8, 0, 0, 255];
        let image = image::RgbaImage::from_raw(1, 1, red.to_vec()).unwrap();
        let dynamic = DynamicImage::ImageRgba8(image);
        for format in [ImageFormat::Bmp, ImageFormat::Gif] {
            let mut bytes = Vec::new();
            dynamic.write_to(&mut Cursor::new(&mut bytes), format).unwrap();
            assert_rgba(&bytes, Some(format), &red, 1, 1);
        }
    }

    #[test]
    fn webp_png_and_jpeg_round_trip_known_pixels() {
        let rgba = [10u8, 20, 30, 255, 40, 50, 60, 255];
        let image = image::RgbaImage::from_raw(2, 1, rgba.to_vec()).unwrap();
        let dynamic = DynamicImage::ImageRgba8(image);
        for format in [ImageFormat::Png, ImageFormat::WebP] {
            let mut bytes = Vec::new();
            dynamic.write_to(&mut Cursor::new(&mut bytes), format).unwrap();
            let picture = decode_bytes_with(&bytes, Some(format)).unwrap();
            assert_eq!(picture.rgba, rgba, "{format:?}");
            assert_eq!((picture.width, picture.height), (2, 1));
        }
        // A 1×1 baseline JPEG. The loader must accept it; the exact colour is the
        // codec's, so only the size is fixed.
        let jpg: &[u8] = &[
            0xFF, 0xD8, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06, 0x07, 0x06, 0x05, 0x08,
            0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0A, 0x0C, 0x14, 0x0D, 0x0C, 0x0B, 0x0B, 0x0C,
            0x19, 0x12, 0x13, 0x0F, 0x14, 0x1D, 0x1A, 0x1F, 0x1E, 0x1D, 0x1A, 0x1C, 0x1C, 0x20,
            0x24, 0x2E, 0x27, 0x20, 0x22, 0x2C, 0x23, 0x1C, 0x1C, 0x28, 0x37, 0x29, 0x2C, 0x30,
            0x31, 0x34, 0x34, 0x34, 0x1F, 0x27, 0x39, 0x3D, 0x38, 0x32, 0x3C, 0x2E, 0x33, 0x34,
            0x32, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01, 0x01, 0x01, 0x11, 0x00,
            0xFF, 0xC4, 0x00, 0x14, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0xFF, 0xC4, 0x00, 0x14,
            0x10, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00,
            0xD2, 0xCF, 0x20, 0xFF, 0xD9,
        ];
        let jpeg = decode_bytes_with(jpg, Some(ImageFormat::Jpeg)).unwrap();
        assert_eq!((jpeg.width, jpeg.height), (1, 1));
        assert_eq!(jpeg.rgba.len(), 4);
    }

    #[test]
    fn radiance_hdr_is_linear_float() {
        // One pixel, red = 1.0 in Radiance RGBE (128 * 2^(129-136) = 1).
        let mut bytes = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 1 +X 1\n".to_vec();
        bytes.extend_from_slice(&[128, 0, 0, 129]);
        let picture = decode_bytes_with(&bytes, Some(ImageFormat::Hdr)).unwrap();
        assert!(picture.linear_file);
        assert_eq!((picture.width, picture.height), (1, 1));
        let linear = picture.linear.unwrap();
        assert!((linear[0] - 1.0).abs() < 1.0e-5, "red {}", linear[0]);
        assert!(linear[1].abs() < 1.0e-5 && linear[2].abs() < 1.0e-5);
        assert!((linear[3] - 1.0).abs() < 1.0e-5);
        assert_eq!(picture.rgba, [255, 0, 0, 255]);
    }

    #[test]
    fn a_brighter_hdr_pixel_stays_linear_and_is_tone_mapped() {
        let mut bytes = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 1 +X 2\n".to_vec();
        bytes.extend_from_slice(&[128, 0, 0, 129, 128, 0, 0, 131]);
        let picture = decode_bytes_with(&bytes, Some(ImageFormat::Hdr)).unwrap();
        let linear = picture.linear.unwrap();
        assert!((linear[0] - 1.0).abs() < 1.0e-4, "{}", linear[0]);
        assert!((linear[4] - 4.0).abs() < 1.0e-3, "{}", linear[4]);
        assert_eq!(picture.rgba[4], 255, "the brightest pixel maps to white");
        assert!(picture.rgba[0] < 255, "a dimmer pixel in the same picture is pulled down");
    }

    #[test]
    fn an_exr_pixel_is_stored_linear() {
        let bytes = tiny_exr(1, 1, &[2.0, 0.0, 0.0, 1.0]);
        let picture = decode_bytes_with(&bytes, Some(ImageFormat::OpenExr)).unwrap();
        assert!(picture.linear_file);
        assert_eq!((picture.width, picture.height), (1, 1));
        let linear = picture.linear.unwrap();
        assert!((linear[0] - 2.0).abs() < 1.0e-4, "{}", linear[0]);
        assert_eq!(picture.rgba[0], 255);
        let torn = std::panic::catch_unwind(|| {
            decode_bytes_with(&[0x76, 0x2f, 0x31, 0x01], Some(ImageFormat::OpenExr))
        });
        assert!(torn.unwrap().is_err());
    }

    fn tiny_exr(width: u32, height: u32, rgba: &[f32]) -> Vec<u8> {
        fn attr(out: &mut Vec<u8>, name: &str, kind: &str, value: &[u8]) {
            out.extend_from_slice(name.as_bytes());
            out.push(0);
            out.extend_from_slice(kind.as_bytes());
            out.push(0);
            out.extend_from_slice(&(value.len() as i32).to_le_bytes());
            out.extend_from_slice(value);
        }
        fn channel(name: &str) -> Vec<u8> {
            let mut out = Vec::new();
            out.extend_from_slice(name.as_bytes());
            out.push(0);
            out.extend_from_slice(&2i32.to_le_bytes());
            out.extend_from_slice(&[0, 0, 0, 0]);
            out.extend_from_slice(&1i32.to_le_bytes());
            out.extend_from_slice(&1i32.to_le_bytes());
            out
        }
        let mut channels = Vec::new();
        for name in ["A", "B", "G", "R"] {
            channels.extend(channel(name));
        }
        channels.push(0);
        let mut window = Vec::new();
        for value in [0, 0, width as i32 - 1, height as i32 - 1] {
            window.extend_from_slice(&value.to_le_bytes());
        }
        let mut header = Vec::new();
        header.extend_from_slice(&20000630i32.to_le_bytes());
        header.extend_from_slice(&2i32.to_le_bytes());
        attr(&mut header, "channels", "chlist", &channels);
        attr(&mut header, "compression", "compression", &[0]);
        attr(&mut header, "dataWindow", "box2i", &window);
        attr(&mut header, "displayWindow", "box2i", &window);
        attr(&mut header, "lineOrder", "lineOrder", &[0]);
        attr(&mut header, "pixelAspectRatio", "float", &1.0f32.to_le_bytes());
        attr(&mut header, "screenWindowCenter", "v2f", &[0u8; 8]);
        attr(&mut header, "screenWindowWidth", "float", &1.0f32.to_le_bytes());
        header.push(0);
        let row_bytes = width as usize * 4 * 4;
        let table_at = header.len();
        header.extend(std::iter::repeat(0u8).take(height as usize * 8));
        let mut offsets = Vec::new();
        for y in 0..height {
            offsets.push(header.len() as u64);
            header.extend_from_slice(&(y as i32).to_le_bytes());
            header.extend_from_slice(&(row_bytes as i32).to_le_bytes());
            for channel in [3usize, 2, 1, 0] {
                for x in 0..width {
                    let pixel = ((y * width + x) as usize) * 4 + channel;
                    header.extend_from_slice(&rgba[pixel].to_le_bytes());
                }
            }
        }
        for (index, offset) in offsets.iter().enumerate() {
            header[table_at + index * 8..table_at + index * 8 + 8]
                .copy_from_slice(&offset.to_le_bytes());
        }
        header
    }

    #[test]
    fn damaged_input_is_an_error_and_does_not_panic() {
        for bytes in [b"".as_slice(), b"not a picture", b"GIF89a", &[0, 0, 2, 0, 0]] {
            let ran = std::panic::catch_unwind(|| decode_bytes(bytes));
            assert!(ran.is_ok(), "decoder panicked");
            assert!(ran.unwrap().is_err());
        }
        let torn =
            std::panic::catch_unwind(|| decode_bytes_with(&[0, 0, 2], Some(ImageFormat::Tga)));
        assert!(torn.is_ok());
        assert!(torn.unwrap().is_err());
    }

    #[test]
    fn max_size_downscales_and_a_smaller_picture_stays() {
        // 4×2 stored row by row, two rows of red,red,blue,blue.
        let rgba: Vec<u8> = (0..2)
            .flat_map(|_| {
                (0..4).flat_map(|x| if x < 2 { [255, 0, 0, 255] } else { [0, 0, 255, 255] })
            })
            .collect();
        let (small, w, h) = downscale(&rgba, 4, 2, 2);
        assert_eq!((w, h), (2, 1));
        assert_eq!(small, [255, 0, 0, 255, 0, 0, 255, 255]);
        let (same, w, h) = downscale(&rgba, 4, 2, 0);
        assert_eq!((w, h), (4, 2));
        assert_eq!(same, rgba);
        let (kept, w, h) = downscale(&rgba, 4, 2, 8);
        assert_eq!((w, h), (4, 2));
        assert_eq!(kept.len(), rgba.len());
    }

    #[test]
    fn sheet_pivot_and_display_encode() {
        assert_eq!(sheet_frames(32, 16), 2);
        assert_eq!(sheet_frames(16, 16), 1);
        assert_eq!(sheet_uv([0.0, 0.0, 1.0, 1.0], 2, 0), [0.0, 0.0, 0.5, 1.0]);
        // A flipped rectangle keeps the first cell and the flip: left edge samples the cell's right side.
        assert_eq!(sheet_uv([1.0, 0.0, 0.0, 1.0], 2, 0), [0.5, 0.0, 0.0, 1.0]);
        assert_eq!(pivot_xy("center", [0.0, 0.0]), [0.5, 0.5]);
        assert_eq!(pivot_xy("bottom_left", [0.0, 0.0]), [0.0, 0.0]);
        assert_eq!(pivot_xy("custom", [0.25, 0.75]), [0.25, 0.75]);
        assert_eq!(linear_channel_to_display(128), 181);
        assert_eq!(linear_channel_to_display(0), 0);
        assert_eq!(linear_channel_to_display(255), 255);
        assert!(use_mip(true, 4.0, 1.0, 2.0));
        assert!(!use_mip(false, 4.0, 1.0, 2.0));
        assert!(!use_mip(true, 4.0, 1.0, 4.0));
    }
}
