//! Front-view scene preview raster for Asset Browser thumbnails.

use std::path::Path;

use seal_world::{Entity, Shape, World};

use crate::error::CoreError;
use crate::import_presets::{picture_look, sprite_world_size};

pub const THUMB_SIZE: u32 = 128;
const PAD: f32 = 6.0;

#[derive(Clone, Copy)]
struct Bounds {
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
}

impl Bounds {
    fn new() -> Self {
        Self {
            min_x: f32::INFINITY,
            max_x: f32::NEG_INFINITY,
            min_y: f32::INFINITY,
            max_y: f32::NEG_INFINITY,
        }
    }

    fn include(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.min_x = self.min_x.min(x0).min(x1);
        self.max_x = self.max_x.max(x0).max(x1);
        self.min_y = self.min_y.min(y0).min(y1);
        self.max_y = self.max_y.max(y0).max(y1);
    }

    fn span(&self) -> (f32, f32) {
        let w = (self.max_x - self.min_x).max(0.25);
        let h = (self.max_y - self.min_y).max(0.25);
        (w, h)
    }
}

struct SpriteSource {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

fn decode_picture(path: &Path) -> Option<SpriteSource> {
    let file = std::io::BufReader::new(std::fs::File::open(path).ok()?);
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut raw = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut raw).ok()?;
    let packed = &raw[..frame.buffer_size()];
    let rgba = match frame.color_type {
        png::ColorType::Rgba => packed.to_vec(),
        png::ColorType::Rgb => packed.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        _ => return None,
    };
    Some(SpriteSource { rgba, width: frame.width, height: frame.height })
}

fn picture_px(path: &Path) -> [f32; 2] {
    let file = std::io::BufReader::new(match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return [16.0, 16.0],
    });
    let mut decoder = png::Decoder::new(file);
    if let Ok(info) = decoder.read_header_info() {
        return [info.width as f32, info.height as f32];
    }
    [16.0, 16.0]
}

fn entity_color(entity: &Entity) -> [u8; 4] {
    let tint = if entity.tint[3] > 0.0 { entity.tint } else { [1.0, 1.0, 1.0, 1.0] };
    let base = entity.color;
    [
        (base[0] * tint[0] * 255.0).round().clamp(0.0, 255.0) as u8,
        (base[1] * tint[1] * 255.0).round().clamp(0.0, 255.0) as u8,
        (base[2] * tint[2] * 255.0).round().clamp(0.0, 255.0) as u8,
        (tint[3] * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

fn shape_front_half(entity: &Entity) -> [f32; 2] {
    let scale = entity.scale;
    match entity.shape {
        Some(Shape::Cube) => [0.5 * scale[0].abs(), 0.5 * scale[1].abs()],
        Some(Shape::Sphere) => {
            let r = 0.5 * scale[0].abs().max(scale[1].abs()).max(scale[2].abs());
            [r, r]
        }
        Some(Shape::Cylinder) | Some(Shape::Cone) | Some(Shape::Capsule) => {
            [0.5 * scale[0].abs().max(scale[2].abs()), 0.5 * scale[1].abs()]
        }
        Some(Shape::Plane) => [20.0 * scale[0].abs(), 20.0 * scale[1].abs()],
        None => [0.3, 0.3],
    }
}

fn sprite_front_rect(entity: &Entity, picture: &Path) -> [f32; 4] {
    let px = picture_px(picture);
    let look = picture_look(picture);
    let full = sprite_world_size([entity.scale[0], entity.scale[1]], px, look.pixels_per_unit);
    let pivot = entity.pivot;
    let cx = entity.translation[0];
    let cy = entity.translation[1];
    let left = cx - full[0] * pivot[0];
    let right = left + full[0];
    let bottom = cy - full[1] * pivot[1];
    let top = bottom + full[1];
    [left, bottom, right, top]
}

fn shape2d_bounds(entity: &Entity) -> Option<[f32; 4]> {
    let s = &entity.shape2d;
    if s.kind.is_empty() || s.points.is_empty() {
        return None;
    }
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let sx = entity.scale[0];
    let sy = entity.scale[1];
    let rot = entity.rotation[2].to_radians();
    let (sn, cs) = rot.sin_cos();
    for p in &s.points {
        let lx = p[0] * sx;
        let ly = p[1] * sy;
        let wx = entity.translation[0] + lx * cs - ly * sn;
        let wy = entity.translation[1] + lx * sn + ly * cs;
        min_x = min_x.min(wx);
        max_x = max_x.max(wx);
        min_y = min_y.min(wy);
        max_y = max_y.max(wy);
    }
    Some([min_x, min_y, max_x, max_y])
}

fn world_to_thumb(x: f32, y: f32, bounds: &Bounds, out_w: u32, out_h: u32) -> (i32, i32) {
    let (sw, sh) = bounds.span();
    let fx = (x - bounds.min_x) / sw;
    let fy = (bounds.max_y - y) / sh;
    let px = (PAD + fx * (out_w as f32 - 2.0 * PAD)).round() as i32;
    let py = (PAD + fy * (out_h as f32 - 2.0 * PAD)).round() as i32;
    (px, py)
}

fn blend_px(dst: &mut [u8], src: [u8; 4]) {
    let a = src[3] as f32 / 255.0;
    if a <= 0.0 {
        return;
    }
    if a >= 0.999 {
        dst.copy_from_slice(&src);
        return;
    }
    for i in 0..3 {
        dst[i] = ((1.0 - a) * f32::from(dst[i]) + a * f32::from(src[i])).round() as u8;
    }
    dst[3] = ((1.0 - a) * f32::from(dst[3]) + a * 255.0).round() as u8;
}

fn fill_rect(rgba: &mut [u8], w: u32, h: u32, x0: i32, y0: i32, x1: i32, y1: i32, color: [u8; 4]) {
    let x0 = x0.max(0);
    let y0 = y0.max(0);
    let x1 = x1.min(w as i32);
    let y1 = y1.min(h as i32);
    for y in y0..y1 {
        for x in x0..x1 {
            let i = ((y * w as i32 + x) * 4) as usize;
            blend_px(&mut rgba[i..i + 4], color);
        }
    }
}

fn blit_sprite(
    rgba: &mut [u8],
    out_w: u32,
    _out_h: u32,
    dst: [i32; 4],
    src: &SpriteSource,
    uv: [f32; 4],
    tint: [u8; 4],
) {
    let (x0, y0, x1, y1) = (dst[0], dst[1], dst[2], dst[3]);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let u0 = uv[0];
    let v0 = uv[1];
    let u1 = uv[2];
    let v1 = uv[3];
    for y in y0..y1 {
        let ty = (y - y0) as f32 / (y1 - y0) as f32;
        let sy = (v0 + ty * (v1 - v0)) * src.height as f32;
        let sy = sy.floor().clamp(0.0, src.height as f32 - 1.0) as u32;
        for x in x0..x1 {
            let tx = (x - x0) as f32 / (x1 - x0) as f32;
            let sx = (u0 + tx * (u1 - u0)) * src.width as f32;
            let sx = sx.floor().clamp(0.0, src.width as f32 - 1.0) as u32;
            let si = ((sy * src.width + sx) * 4) as usize;
            let mut px = [src.rgba[si], src.rgba[si + 1], src.rgba[si + 2], src.rgba[si + 3]];
            for c in 0..3 {
                px[c] = ((px[c] as f32 / 255.0) * (tint[c] as f32 / 255.0) * 255.0).round() as u8;
            }
            px[3] = ((px[3] as f32 / 255.0) * (tint[3] as f32 / 255.0) * 255.0).round() as u8;
            if px[3] == 0 {
                continue;
            }
            let i = ((y * out_w as i32 + x) * 4) as usize;
            if i + 3 < rgba.len() {
                blend_px(&mut rgba[i..i + 4], px);
            }
        }
    }
}

fn draw_shape2d(rgba: &mut [u8], out_w: u32, out_h: u32, entity: &Entity, bounds: &Bounds) {
    let s = &entity.shape2d;
    if s.kind != "polygon" || s.points.len() < 3 {
        return;
    }
    let color = {
        let c = s.color;
        [
            (c[0] * 255.0).round().clamp(0.0, 255.0) as u8,
            (c[1] * 255.0).round().clamp(0.0, 255.0) as u8,
            (c[2] * 255.0).round().clamp(0.0, 255.0) as u8,
            255,
        ]
    };
    let sx = entity.scale[0];
    let sy = entity.scale[1];
    let rot = entity.rotation[2].to_radians();
    let (sn, cs) = rot.sin_cos();
    let mut pts = Vec::new();
    for p in &s.points {
        let lx = p[0] * sx;
        let ly = p[1] * sy;
        let wx = entity.translation[0] + lx * cs - ly * sn;
        let wy = entity.translation[1] + lx * sn + ly * cs;
        pts.push(world_to_thumb(wx, wy, bounds, out_w, out_h));
    }
    let min_y = pts.iter().map(|p| p.1).min().unwrap_or(0);
    let max_y = pts.iter().map(|p| p.1).max().unwrap_or(0);
    for y in min_y..=max_y {
        let mut xs = Vec::new();
        for i in 0..pts.len() {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            if (a.1 <= y && b.1 > y) || (b.1 <= y && a.1 > y) {
                let t = (y - a.1) as f32 / (b.1 - a.1) as f32;
                let x = a.0 as f32 + t * (b.0 - a.0) as f32;
                xs.push(x.round() as i32);
            }
        }
        xs.sort_unstable();
        let mut i = 0;
        while i + 1 < xs.len() {
            fill_rect(rgba, out_w, out_h, xs[i], y, xs[i + 1] + 1, y + 1, color);
            i += 2;
        }
    }
}

/// Rasterise the scene's flat front view into RGBA8 (`THUMB_SIZE` square).
pub fn render(project_root: &Path, world_text: &str) -> Result<Vec<u8>, CoreError> {
    let world = World::from_text(world_text).map_err(CoreError::new)?;
    let mut bounds = Bounds::new();
    let mut order: Vec<usize> = Vec::new();

    for (index, entity) in world.entities.iter().enumerate() {
        if entity.id == "root" || entity.hidden || entity.tag == "folder" {
            continue;
        }
        let mut found = false;
        if let Some((path, _)) = entity.sprite_draw(Some(project_root), 0.0) {
            let rect = sprite_front_rect(entity, &path);
            bounds.include(rect[0], rect[1], rect[2], rect[3]);
            found = true;
        } else if entity.shape.is_some() {
            let half = shape_front_half(entity);
            let cx = entity.translation[0];
            let cy = entity.translation[1];
            bounds.include(cx - half[0], cy - half[1], cx + half[0], cy + half[1]);
            found = true;
        }
        if let Some(b) = shape2d_bounds(entity) {
            bounds.include(b[0], b[1], b[2], b[3]);
            found = true;
        }
        if found {
            order.push(index);
        }
    }

    if bounds.min_x.is_infinite() {
        let mut rgba = vec![60u8; (THUMB_SIZE * THUMB_SIZE * 4) as usize];
        rgba.chunks_mut(4).for_each(|px| {
            px[0] = 60;
            px[1] = 80;
            px[2] = 110;
            px[3] = 255;
        });
        return Ok(rgba);
    }

    order.sort_by_key(|&i| {
        let e = &world.entities[i];
        (e.layer, e.order, (e.translation[2] * 1000.0) as i32)
    });

    let mut rgba = vec![40u8; (THUMB_SIZE * THUMB_SIZE * 4) as usize];
    for row in rgba.chunks_mut(THUMB_SIZE as usize * 4) {
        for px in row.chunks_mut(4) {
            px[0] = 40;
            px[1] = 48;
            px[2] = 58;
            px[3] = 255;
        }
    }

    for &index in &order {
        let entity = &world.entities[index];
        if let Some((path, uv)) = entity.sprite_draw(Some(project_root), 0.0) {
            let rect = sprite_front_rect(entity, &path);
            let (x0, y0) = world_to_thumb(rect[0], rect[3], &bounds, THUMB_SIZE, THUMB_SIZE);
            let (x1, y1) = world_to_thumb(rect[2], rect[1], &bounds, THUMB_SIZE, THUMB_SIZE);
            if let Some(src) = decode_picture(&path) {
                let tint = entity_color(entity);
                blit_sprite(&mut rgba, THUMB_SIZE, THUMB_SIZE, [x0, y0, x1, y1], &src, uv, tint);
            } else {
                fill_rect(&mut rgba, THUMB_SIZE, THUMB_SIZE, x0, y0, x1, y1, entity_color(entity));
            }
        } else if entity.shape.is_some() {
            let half = shape_front_half(entity);
            let cx = entity.translation[0];
            let cy = entity.translation[1];
            let (x0, y0) =
                world_to_thumb(cx - half[0], cy + half[1], &bounds, THUMB_SIZE, THUMB_SIZE);
            let (x1, y1) =
                world_to_thumb(cx + half[0], cy - half[1], &bounds, THUMB_SIZE, THUMB_SIZE);
            fill_rect(&mut rgba, THUMB_SIZE, THUMB_SIZE, x0, y0, x1, y1, entity_color(entity));
        }
        draw_shape2d(&mut rgba, THUMB_SIZE, THUMB_SIZE, entity, &bounds);
    }
    Ok(rgba)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    fn write_png(path: &Path, w: u32, h: u32, rgb: [u8; 3]) {
        let file = fs::File::create(path).unwrap();
        let mut enc = png::Encoder::new(file, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().unwrap();
        let row: Vec<u8> = (0..w).flat_map(|_| rgb).collect();
        let pixels: Vec<u8> = (0..h).flat_map(|_| row.iter().copied()).collect();
        writer.write_image_data(&pixels).unwrap();
    }

    fn pixel(rgba: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * w + x) * 4) as usize;
        rgba[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn thumb_places_sprite_colours() {
        let dir = std::env::temp_dir().join(format!("seal-thumb-scene-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let red = dir.join("red.png");
        let blue = dir.join("blue.png");
        write_png(&red, 8, 8, [220, 30, 30]);
        write_png(&blue, 8, 8, [30, 60, 220]);
        let mut world = World::from_text(&World::empty_scene_text("Thumb")).unwrap();
        let left_id = world.add("empty");
        world.entity_mut(&left_id).unwrap().sprite = "red.png".into();
        world.entity_mut(&left_id).unwrap().translation = [-2.0, 0.0, 0.0];
        let right_id = world.add("empty");
        world.entity_mut(&right_id).unwrap().sprite = "blue.png".into();
        world.entity_mut(&right_id).unwrap().translation = [2.0, 0.0, 0.0];
        let text = world.to_text();
        let rgba = render(&dir, &text).unwrap();
        let left = pixel(&rgba, THUMB_SIZE, 24, 64);
        let right = pixel(&rgba, THUMB_SIZE, 104, 64);
        assert!(left[0] > left[2] + 40, "red sprite on the left: {:?}", left);
        assert!(right[2] > right[0] + 40, "blue sprite on the right: {:?}", right);
    }
}
