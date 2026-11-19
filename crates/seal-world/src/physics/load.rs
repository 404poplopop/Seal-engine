//! Mesh text and picture files used as collider sources.
//!
//! A mesh is lines of `v x y z` and `f i j k` (three corners). Indices are
//! 1-based when every index is at least 1 and the largest equals the vertex
//! count; otherwise they are 0-based.
//!
//! A height picture is a png (as project pictures are), a binary ppm (`P5` or `P6`) or an
//! uncompressed 24-bit bmp. Brightness becomes height. A text grid (`grid columns rows` then
//! one sample per value) is accepted as well.

use std::fs;
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub struct MeshData {
    pub vertices: Vec<[f32; 3]>,
    pub indices: Vec<[u32; 3]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    /// Brightness of each pixel, 0 to 1, row by row from the top.
    pub luma: Vec<f32>,
}

pub fn mesh_from_path(path: &Path) -> Result<MeshData, String> {
    let text = fs::read_to_string(path).map_err(|err| format!("could not read mesh: {err}"))?;
    mesh_from_text(&text)
}

pub fn mesh_from_text(text: &str) -> Result<MeshData, String> {
    let mut vertices = Vec::new();
    let mut raw: Vec<[u32; 3]> = Vec::new();
    for (line_no, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let tag = parts.next().unwrap_or("");
        let nums: Vec<&str> = parts.collect();
        let at = |n: usize| -> Result<f32, String> {
            nums.get(n)
                .ok_or_else(|| format!("line {}: not enough numbers", line_no + 1))?
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| format!("line {}: bad number", line_no + 1))
        };
        match tag {
            "v" => {
                if nums.len() < 3 {
                    return Err(format!("line {}: a vertex needs x y z", line_no + 1));
                }
                vertices.push([at(0)?, at(1)?, at(2)?]);
            }
            "f" => {
                if nums.len() != 3 {
                    return Err(format!("line {}: a face needs three corners", line_no + 1));
                }
                let idx = |n: usize| -> Result<u32, String> {
                    nums[n]
                        .split('/')
                        .next()
                        .unwrap_or("")
                        .parse::<u32>()
                        .map_err(|_| format!("line {}: bad corner", line_no + 1))
                };
                raw.push([idx(0)?, idx(1)?, idx(2)?]);
            }
            _ => return Err(format!("line {}: expected v or f", line_no + 1)),
        }
    }
    if vertices.len() < 3 || raw.is_empty() {
        return Err("mesh needs at least one triangle".into());
    }
    let max = raw.iter().flatten().copied().max().unwrap_or(0);
    let min = raw.iter().flatten().copied().min().unwrap_or(0);
    let one_based = min >= 1 && max == vertices.len() as u32;
    if !one_based && max >= vertices.len() as u32 {
        return Err("mesh corner index is out of range".into());
    }
    let indices = raw
        .into_iter()
        .map(|tri| if one_based { [tri[0] - 1, tri[1] - 1, tri[2] - 1] } else { tri })
        .collect();
    Ok(MeshData { vertices, indices })
}

pub fn picture_from_path(path: &Path) -> Result<Picture, String> {
    let bytes = fs::read(path).map_err(|err| format!("could not read picture: {err}"))?;
    picture_from_bytes(&bytes)
}

pub fn picture_from_bytes(bytes: &[u8]) -> Result<Picture, String> {
    if bytes.len() >= 4 && bytes.starts_with(b"grid") {
        return picture_from_grid(bytes);
    }
    if bytes.len() >= 2 && &bytes[..2] == b"P5" || bytes.len() >= 2 && &bytes[..2] == b"P6" {
        return picture_from_ppm(bytes);
    }
    if bytes.len() >= 2 && &bytes[..2] == b"BM" {
        return picture_from_bmp(bytes);
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return picture_from_png(bytes);
    }
    Err("picture must be a png, a ppm (P5 or P6), an uncompressed bmp, or a grid text file".into())
}

/// Project pictures are PNG; brightness of each pixel (any colour type, 8 or 16 bits) is height.
fn picture_from_png(bytes: &[u8]) -> Result<Picture, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|err| format!("could not read png: {err}"))?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("png is too large")?];
    let info = reader.next_frame(&mut buf).map_err(|err| format!("could not read png: {err}"))?;
    let (width, height) = (info.width, info.height);
    if width == 0 || height == 0 {
        return Err("png has no pixels".into());
    }
    let step = info.color_type.samples();
    let luma = buf[..info.buffer_size()]
        .chunks(step)
        .take((width * height) as usize)
        .map(|px| match px.len() {
            1 | 2 => f32::from(px[0]) / 255.0,
            _ => {
                (0.299 * f32::from(px[0]) + 0.587 * f32::from(px[1]) + 0.114 * f32::from(px[2]))
                    / 255.0
            }
        })
        .collect();
    Ok(Picture { width, height, luma })
}

fn picture_from_grid(bytes: &[u8]) -> Result<Picture, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "grid file is not text".to_string())?;
    let mut nums = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("grid") {
            if line.starts_with("grid") {
                for part in line.split_whitespace().skip(1) {
                    nums.push(part.to_string());
                }
            }
            continue;
        }
        nums.extend(line.split_whitespace().map(|p| p.to_string()));
    }
    if nums.len() < 2 {
        return Err("grid file is missing its size".into());
    }
    let columns: usize = nums[0].parse().map_err(|_| "grid width is not a number".to_string())?;
    let rows: usize = nums[1].parse().map_err(|_| "grid height is not a number".to_string())?;
    if columns == 0 || rows == 0 {
        return Err("grid size must be at least 1".into());
    }
    let samples = &nums[2..];
    if samples.len() != columns * rows {
        return Err(format!("grid has {} samples, expected {}", samples.len(), columns * rows));
    }
    let mut luma = Vec::with_capacity(samples.len());
    for sample in samples {
        let v: f32 = sample.parse().map_err(|_| format!("bad grid sample {sample}"))?;
        if !v.is_finite() {
            return Err("grid sample is not a finite number".into());
        }
        luma.push(v.clamp(0.0, 1.0));
    }
    Ok(Picture { width: columns as u32, height: rows as u32, luma })
}

fn picture_from_ppm(bytes: &[u8]) -> Result<Picture, String> {
    let color = bytes[1] == b'6';
    let mut i = 2;
    let mut header = Vec::new();
    while header.len() < 3 {
        skip_ws_comments(bytes, &mut i)?;
        let start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if start == i {
            return Err("ppm header is cut off".into());
        }
        let token = std::str::from_utf8(&bytes[start..i])
            .map_err(|_| "ppm header is not text".to_string())?;
        header.push(token.to_string());
    }
    let width: usize = header[0].parse().map_err(|_| "ppm width is not a number".to_string())?;
    let height: usize = header[1].parse().map_err(|_| "ppm height is not a number".to_string())?;
    let max: usize = header[2].parse().map_err(|_| "ppm max value is not a number".to_string())?;
    if width == 0 || height == 0 || max == 0 || max > 255 {
        return Err("ppm size is not usable".into());
    }
    if i >= bytes.len() || !bytes[i].is_ascii_whitespace() {
        return Err("ppm header is cut off".into());
    }
    i += 1;
    let channels = if color { 3 } else { 1 };
    let need = width * height * channels;
    if bytes.len() < i + need {
        return Err("ppm pixel data is cut off".into());
    }
    let mut luma = Vec::with_capacity(width * height);
    let scale = max as f32;
    for pixel in 0..width * height {
        let s = i + pixel * channels;
        let value = if color {
            (bytes[s] as f32 + bytes[s + 1] as f32 + bytes[s + 2] as f32) / (3.0 * scale)
        } else {
            bytes[s] as f32 / scale
        };
        luma.push(value.clamp(0.0, 1.0));
    }
    Ok(Picture { width: width as u32, height: height as u32, luma })
}

fn skip_ws_comments(bytes: &[u8], i: &mut usize) -> Result<(), String> {
    loop {
        while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
            *i += 1;
        }
        if *i < bytes.len() && bytes[*i] == b'#' {
            while *i < bytes.len() && bytes[*i] != b'\n' {
                *i += 1;
            }
            continue;
        }
        break;
    }
    if *i >= bytes.len() {
        return Err("ppm header is cut off".into());
    }
    Ok(())
}

fn picture_from_bmp(bytes: &[u8]) -> Result<Picture, String> {
    if bytes.len() < 54 {
        return Err("bmp header is cut off".into());
    }
    let read_u32 =
        |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let read_i32 =
        |at: usize| i32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let offset = read_u32(10) as usize;
    let width = read_i32(18);
    let height_raw = read_i32(22);
    let planes = u16::from_le_bytes([bytes[26], bytes[27]]);
    let bits = u16::from_le_bytes([bytes[28], bytes[29]]);
    let compression = read_u32(30);
    if width <= 0 || planes != 1 || bits != 24 || compression != 0 {
        return Err("bmp must be uncompressed 24-bit".into());
    }
    let width = width as usize;
    let flip = height_raw > 0;
    let height = height_raw.unsigned_abs() as usize;
    if height == 0 {
        return Err("bmp height is zero".into());
    }
    let row_stride = (width * 3 + 3) & !3;
    let need = offset + row_stride * height;
    if bytes.len() < need {
        return Err("bmp pixel data is cut off".into());
    }
    let mut luma = vec![0.0; width * height];
    for row in 0..height {
        let src_row = if flip { height - 1 - row } else { row };
        let base = offset + src_row * row_stride;
        for col in 0..width {
            let s = base + col * 3;
            let b = bytes[s] as f32;
            let g = bytes[s + 1] as f32;
            let r = bytes[s + 2] as f32;
            luma[row * width + col] = ((r + g + b) / (3.0 * 255.0)).clamp(0.0, 1.0);
        }
    }
    Ok(Picture { width: width as u32, height: height as u32, luma })
}

/// Heights for a field of `size` (width, height scale, depth). Samples are
/// brightness times the height scale. A 1×1 picture is repeated to 2×2.
pub fn heights_from_picture(
    picture: &Picture,
    size: [f32; 3],
) -> Result<(usize, usize, Vec<f32>), String> {
    if picture.luma.len() != picture.width as usize * picture.height as usize {
        return Err("picture brightness does not match its size".into());
    }
    if picture.width == 0 || picture.height == 0 {
        return Err("picture is empty".into());
    }
    let mut columns = picture.width as usize;
    let mut rows = picture.height as usize;
    let mut luma = picture.luma.clone();
    if columns == 1 {
        columns = 2;
        luma = luma.into_iter().flat_map(|v| [v, v]).collect();
    }
    if rows == 1 {
        rows = 2;
        let extra = luma.clone();
        luma.extend(extra);
    }
    let scale = if size[1].is_finite() { size[1] } else { 1.0 };
    let heights = luma.into_iter().map(|v| v * scale).collect();
    Ok((columns, rows, heights))
}
