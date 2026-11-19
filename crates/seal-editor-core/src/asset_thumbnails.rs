//! Disk thumbnail cache under `.seal-editor/thumbs/`, invalidated when sources change.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::CoreError;

const SIZE: u32 = 128;

pub fn cache_dir(project_root: &Path) -> PathBuf {
    project_root.join(".seal-editor").join("thumbs")
}

fn cache_path(project_root: &Path, rel: &str) -> PathBuf {
    let safe = rel.replace('/', "__");
    cache_dir(project_root).join(format!("{safe}.png"))
}

fn sidecar(path: &Path) -> PathBuf {
    path.with_extension("thumb.stamp")
}

fn source_mtime(path: &Path) -> Option<std::time::SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

pub fn cache_stale(project_root: &Path, rel: &str, source: &Path) -> bool {
    let cache = cache_path(project_root, rel);
    let stamp = sidecar(&cache);
    let Some(src_time) = source_mtime(source) else {
        return true;
    };
    let Ok(stamp_text) = fs::read_to_string(&stamp) else {
        return true;
    };
    let Ok(stored) = stamp_text.parse::<u128>() else {
        return true;
    };
    let Ok(src) = src_time.duration_since(std::time::UNIX_EPOCH) else {
        return true;
    };
    stored != src.as_millis()
}

fn write_stamp(cache: &Path, source: &Path) -> Result<(), CoreError> {
    let Some(src_time) = source_mtime(source) else {
        return Ok(());
    };
    let Ok(duration) = src_time.duration_since(std::time::UNIX_EPOCH) else {
        return Ok(());
    };
    fs::write(sidecar(cache), duration.as_millis().to_string())?;
    Ok(())
}

fn write_rgba(path: &Path, rgba: &[u8], w: u32, h: u32) -> Result<(), CoreError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = std::io::BufWriter::new(fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| CoreError::new(e.to_string()))?;
    writer.write_image_data(rgba).map_err(|e| CoreError::new(e.to_string()))?;
    Ok(())
}

/// Front-view scene preview (captured when the scene is saved).
pub fn refresh_scene_thumbnail(
    project_root: &Path,
    rel: &str,
    world_text: &str,
) -> Result<(), CoreError> {
    let rgba = crate::scene_thumbnail::render(project_root, world_text)?;
    let cache = cache_path(project_root, rel);
    write_rgba(&cache, &rgba, SIZE, SIZE)?;
    let source = project_root.join(rel);
    write_stamp(&cache, &source)
}

/// Peak waveform from raw audio bytes (no decoder): envelope of byte samples.
pub fn waveform_thumbnail(source: &Path) -> Result<(Vec<u8>, u32, u32), CoreError> {
    let mut raw = Vec::new();
    fs::File::open(source)?.read_to_end(&mut raw)?;
    let bins = SIZE as usize;
    let mut peaks = vec![0u8; bins];
    if raw.is_empty() {
        return Ok((flat_waveform(), SIZE, SIZE));
    }
    let step = (raw.len() / bins).max(1);
    for (i, peak) in peaks.iter_mut().enumerate() {
        let start = i * step;
        let end = (start + step).min(raw.len());
        let max = raw[start..end].iter().copied().max().unwrap_or(128);
        *peak = max;
    }
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    for x in 0..bins {
        let h = u32::from(peaks[x]) * SIZE / 255;
        let y0 = SIZE.saturating_sub(h.max(4));
        for y in y0..SIZE {
            let i = ((y * SIZE + x as u32) * 4) as usize;
            rgba[i] = 90;
            rgba[i + 1] = 160;
            rgba[i + 2] = 220;
            rgba[i + 3] = 255;
        }
    }
    Ok((rgba, SIZE, SIZE))
}

fn flat_waveform() -> Vec<u8> {
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    let mid = SIZE / 2;
    for x in 0..SIZE {
        for y in mid.saturating_sub(2)..mid + 3 {
            let i = ((y * SIZE + x) * 4) as usize;
            rgba[i] = 90;
            rgba[i + 1] = 160;
            rgba[i + 2] = 220;
            rgba[i + 3] = 255;
        }
    }
    rgba
}

/// Accent colour from an effect file's `color_gradient` line, or a default.
pub fn fx_icon_color(source: &Path) -> [u8; 3] {
    let Ok(text) = fs::read_to_string(source) else {
        return [200, 120, 60];
    };
    for line in text.lines() {
        if line.starts_with("color_gradient ") {
            if let Some(hex) = line.split_whitespace().nth(1) {
                if let Some(rgb) = parse_hex_color(hex) {
                    return rgb;
                }
            }
        }
    }
    [200, 120, 60]
}

fn parse_hex_color(hex: &str) -> Option<[u8; 3]> {
    let h = hex.trim_start_matches('#');
    if h.len() == 6 {
        let r = u8::from_str_radix(&h[0..2], 16).ok()?;
        let g = u8::from_str_radix(&h[2..4], 16).ok()?;
        let b = u8::from_str_radix(&h[4..6], 16).ok()?;
        return Some([r, g, b]);
    }
    None
}

pub fn fx_icon_rgba(color: [u8; 3]) -> Vec<u8> {
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    let margin = 24;
    for y in margin..SIZE - margin {
        for x in margin..SIZE - margin {
            let i = ((y * SIZE + x) * 4) as usize;
            rgba[i] = color[0];
            rgba[i + 1] = color[1];
            rgba[i + 2] = color[2];
            rgba[i + 3] = 255;
        }
    }
    rgba
}

/// Build missing thumbnails on `workers` threads. Cached pictures are left alone.
pub fn warm(project_root: &Path, jobs: &[(String, PathBuf, String)], workers: usize) {
    let stale: Vec<(String, PathBuf, String)> = jobs
        .iter()
        .filter(|(rel, source, kind)| kind != "folder" && cache_stale(project_root, rel, source))
        .cloned()
        .collect();
    if stale.is_empty() {
        return;
    }
    let root = project_root.to_path_buf();
    crate::editor_tools::map_in_order(workers, stale, |(rel, source, kind)| {
        ensure_thumbnail(&root, &rel, &source, &kind);
    });
}

/// Regenerate when missing or stale.
pub fn ensure_thumbnail(
    project_root: &Path,
    rel: &str,
    source: &Path,
    kind: &str,
) -> Option<PathBuf> {
    let cache = cache_path(project_root, rel);
    if !cache_stale(project_root, rel, source) && cache.is_file() {
        return Some(cache);
    }
    if let Some(Ok((rgba, w, h))) = crate::preview_providers::render_with_provider(source) {
        write_rgba(&cache, &rgba, w, h).ok()?;
        write_stamp(&cache, source).ok()?;
        return Some(cache);
    }
    let result = match kind {
        "material" => {
            let rgba = crate::asset_preview::material_sphere_rgba(source).ok()?;
            let s = crate::asset_preview::PREVIEW_SIZE;
            write_rgba(&cache, &rgba, s, s).ok()?;
            write_stamp(&cache, source).ok()?;
            Some(cache)
        }
        "mesh" => {
            let rgba = crate::asset_preview::mesh_asset_rgba(source).ok()?;
            let s = crate::asset_preview::PREVIEW_SIZE;
            write_rgba(&cache, &rgba, s, s).ok()?;
            write_stamp(&cache, source).ok()?;
            Some(cache)
        }
        "scene" => {
            let text = fs::read_to_string(source).ok()?;
            refresh_scene_thumbnail(project_root, rel, &text).ok()?;
            Some(cache)
        }
        "sound" => {
            let (rgba, w, h) = waveform_thumbnail(source).ok()?;
            write_rgba(&cache, &rgba, w, h).ok()?;
            write_stamp(&cache, source).ok()?;
            Some(cache)
        }
        "effect" => {
            let rgba = fx_icon_rgba(fx_icon_color(source));
            write_rgba(&cache, &rgba, SIZE, SIZE).ok()?;
            write_stamp(&cache, source).ok()?;
            Some(cache)
        }
        "animation" => {
            if rel.ends_with(".frames") {
                return first_frames_thumbnail(project_root, rel, source);
            }
            None
        }
        _ => None,
    };
    result
}

fn first_frames_thumbnail(project_root: &Path, rel: &str, source: &Path) -> Option<PathBuf> {
    let lib = seal_world::frames::load_cached(source)?;
    let clip = lib.clip_or_first("")?;
    let frame = clip.frames.first()?;
    let image = source.parent()?.join(&frame.image);
    if !image.is_file() {
        return None;
    }
    let cache = cache_path(project_root, rel);
    let (rgba, w, h) = decode_png(&image).ok()?;
    let (small, sw, sh) = shrink_rgba(&rgba, w, h, SIZE);
    write_rgba(&cache, &small, sw, sh).ok()?;
    write_stamp(&cache, source).ok()?;
    Some(cache)
}

pub fn thumbnail_path(project_root: &Path, rel: &str) -> PathBuf {
    cache_path(project_root, rel)
}

fn decode_png(path: &Path) -> Result<(Vec<u8>, u32, u32), CoreError> {
    let file = std::io::BufReader::new(fs::File::open(path)?);
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| CoreError::new(e.to_string()))?;
    let mut raw = vec![
        0;
        reader
            .output_buffer_size()
            .ok_or_else(|| CoreError::new("could not read picture size"))?
    ];
    let frame = reader.next_frame(&mut raw).map_err(|e| CoreError::new(e.to_string()))?;
    let packed = &raw[..frame.buffer_size()];
    let rgba = match frame.color_type {
        png::ColorType::Rgba => packed.to_vec(),
        png::ColorType::Rgb => packed.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        _ => return Err(CoreError::new("thumbnail picture format is not supported")),
    };
    Ok((rgba, frame.width, frame.height))
}

fn shrink_rgba(rgba: &[u8], w: u32, h: u32, max: u32) -> (Vec<u8>, u32, u32) {
    let scale = (w.max(h) as f32 / max as f32).max(1.0);
    let nw = ((w as f32 / scale).round() as u32).max(1);
    let nh = ((h as f32 / scale).round() as u32).max(1);
    if (nw, nh) == (w, h) {
        return (rgba.to_vec(), w, h);
    }
    let mut out = Vec::with_capacity((nw * nh * 4) as usize);
    for y in 0..nh {
        let y0 = y * h / nh;
        let y1 = ((y + 1) * h / nh).max(y0 + 1);
        for x in 0..nw {
            let x0 = x * w / nw;
            let x1 = ((x + 1) * w / nw).max(x0 + 1);
            let mut sum = [0u32; 4];
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let i = ((sy * w + sx) * 4) as usize;
                    for k in 0..4 {
                        sum[k] += u32::from(rgba[i + k]);
                    }
                }
            }
            let n = (y1 - y0) * (x1 - x0);
            out.extend(sum.iter().map(|v| (v / n) as u8));
        }
    }
    (out, nw, nh)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fx_color_parses_hex_line() {
        let dir = std::env::temp_dir().join(format!("seal-thumb-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let fx = dir.join("spark.fx");
        fs::write(&fx, "color_gradient #336699\n").unwrap();
        assert_eq!(fx_icon_color(&fx), [0x33, 0x66, 0x99]);
    }

    #[test]
    fn waveform_thumbnail_has_pixels() {
        let dir = std::env::temp_dir().join(format!("seal-wave-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let wav = dir.join("beep.raw");
        fs::write(&wav, (0u8..=255).collect::<Vec<_>>()).unwrap();
        let (rgba, w, h) = waveform_thumbnail(&wav).unwrap();
        assert_eq!((w, h), (SIZE, SIZE));
        assert!(rgba.iter().any(|&b| b > 0));
    }

    #[test]
    fn scene_thumb_cache_invalidates_on_mtime() {
        let dir = std::env::temp_dir().join(format!("seal-scene-stale-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let scene = dir.join("levels/a.seal");
        if let Some(parent) = scene.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let text = seal_world::World::empty_scene_text("A");
        std::fs::write(&scene, &text).unwrap();
        refresh_scene_thumbnail(&dir, "levels/a.seal", &text).unwrap();
        let cache = thumbnail_path(&dir, "levels/a.seal");
        assert!(cache.is_file());
        assert!(!cache_stale(&dir, "levels/a.seal", &scene));
        std::thread::sleep(std::time::Duration::from_millis(5));
        let edited = format!("{text}\n");
        std::fs::write(&scene, edited.as_bytes()).unwrap();
        assert!(cache_stale(&dir, "levels/a.seal", &scene));
    }

    #[test]
    fn cache_stale_without_stamp() {
        let dir = std::env::temp_dir().join(format!("seal-stale-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let src = dir.join("a.wav");
        fs::write(&src, b"data").unwrap();
        assert!(cache_stale(&dir, "a.wav", &src));
    }
}
