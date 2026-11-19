//! Software previews for materials (`.shade2d`) and meshes (`.mesh`).

use std::path::Path;

use crate::error::CoreError;

pub const PREVIEW_SIZE: u32 = crate::scene_thumbnail::THUMB_SIZE;

/// Lit sphere preview for a canvas shader or material file.
pub fn material_sphere_rgba(source: &Path) -> Result<Vec<u8>, CoreError> {
    let text = std::fs::read_to_string(source).unwrap_or_default();
    let base = parse_material_base(&text);
    render_sphere(base)
}

/// Mesh asset preview: a solid shape from optional `color` / `bounds` lines.
pub fn mesh_asset_rgba(source: &Path) -> Result<Vec<u8>, CoreError> {
    let text = std::fs::read_to_string(source).unwrap_or_default();
    let color = parse_color_line(&text).unwrap_or([120, 150, 190]);
    let (sx, sy, sz) = parse_bounds_line(&text).unwrap_or((1.0, 1.0, 1.0));
    render_box_mesh(color, sx, sy, sz)
}

fn parse_material_base(text: &str) -> [u8; 3] {
    if let Some(c) = parse_color_line(text) {
        return c;
    }
    let mut r = 180u8;
    let mut g = 140u8;
    let mut b = 110u8;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or(line).trim();
        if let Some((name, value)) = line.split_once('=') {
            match name.trim() {
                "r" => r = float_byte(value),
                "g" => g = float_byte(value),
                "b" => b = float_byte(value),
                _ => {}
            }
        }
    }
    [r, g, b]
}

fn parse_color_line(text: &str) -> Option<[u8; 3]> {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("color ") {
            return parse_hex(rest.trim());
        }
    }
    None
}

fn parse_bounds_line(text: &str) -> Option<(f32, f32, f32)> {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("bounds ") {
            let parts: Vec<f32> = rest.split_whitespace().filter_map(|p| p.parse().ok()).collect();
            if parts.len() >= 3 {
                return Some((parts[0].max(0.1), parts[1].max(0.1), parts[2].max(0.1)));
            }
        }
    }
    None
}

fn parse_hex(hex: &str) -> Option<[u8; 3]> {
    let h = hex.trim_start_matches('#');
    if h.len() == 6 {
        let r = u8::from_str_radix(&h[0..2], 16).ok()?;
        let g = u8::from_str_radix(&h[2..4], 16).ok()?;
        let b = u8::from_str_radix(&h[4..6], 16).ok()?;
        return Some([r, g, b]);
    }
    None
}

fn float_byte(text: &str) -> u8 {
    text.trim()
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite())
        .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
        .unwrap_or(128)
}

fn render_sphere(base: [u8; 3]) -> Result<Vec<u8>, CoreError> {
    let size = PREVIEW_SIZE;
    let mut rgba = vec![32u8; (size * size * 4) as usize];
    let cx = size as f32 * 0.5;
    let cy = size as f32 * 0.56;
    let r = size as f32 * 0.36;
    let light = normalize([0.4, 0.7, 0.6]);
    for y in 0..size {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let d2 = dx * dx + dy * dy;
            if d2 > r * r {
                rgba[i + 3] = 0;
                continue;
            }
            let z = (r * r - d2).sqrt();
            let n = normalize([dx / r, dy / r, z / r]);
            let ndotl = n[0] * light[0] + n[1] * light[1] + n[2] * light[2];
            let shade = (0.35 + 0.65 * ndotl.max(0.0)).clamp(0.0, 1.0);
            rgba[i] = (f32::from(base[0]) * shade) as u8;
            rgba[i + 1] = (f32::from(base[1]) * shade) as u8;
            rgba[i + 2] = (f32::from(base[2]) * shade) as u8;
            rgba[i + 3] = 255;
        }
    }
    Ok(rgba)
}

fn render_box_mesh(color: [u8; 3], sx: f32, sy: f32, sz: f32) -> Result<Vec<u8>, CoreError> {
    let size = PREVIEW_SIZE;
    let mut rgba = vec![28u8; (size * size * 4) as usize];
    let verts: [[f32; 3]; 8] = [
        [-sx, -sy, -sz],
        [sx, -sy, -sz],
        [sx, sy, -sz],
        [-sx, sy, -sz],
        [-sx, -sy, sz],
        [sx, -sy, sz],
        [sx, sy, sz],
        [-sx, sy, sz],
    ];
    let faces: [([usize; 4], [f32; 3]); 6] = [
        ([0, 1, 2, 3], [0.0, 0.0, -1.0]),
        ([5, 4, 7, 6], [0.0, 0.0, 1.0]),
        ([4, 0, 3, 7], [-1.0, 0.0, 0.0]),
        ([1, 5, 6, 2], [1.0, 0.0, 0.0]),
        ([3, 2, 6, 7], [0.0, 1.0, 0.0]),
        ([4, 5, 1, 0], [0.0, -1.0, 0.0]),
    ];
    let light = normalize([0.3, 0.8, 0.5]);
    let mut painted = vec![false; (size * size) as usize];
    type Tri = (f32, [f32; 2], [f32; 2], [f32; 2], [f32; 3]);
    let mut tris: Vec<Tri> = Vec::new();
    for (idx, normal) in faces {
        let depth = normal[2];
        let pts: Vec<[f32; 2]> = idx.iter().map(|&vi| project(verts[vi], sx, sy, sz)).collect();
        tris.push((depth, pts[0], pts[1], pts[2], normal));
        tris.push((depth, pts[0], pts[2], pts[3], normal));
    }
    tris.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    for (_, a, b, c, normal) in tris {
        let ndotl = (normal[0] * light[0] + normal[1] * light[1] + normal[2] * light[2]).max(0.2);
        fill_tri(&mut rgba, &mut painted, size, a, b, c, color, ndotl);
    }
    Ok(rgba)
}

fn project(v: [f32; 3], sx: f32, sy: f32, sz: f32) -> [f32; 2] {
    let yaw: f32 = 0.55;
    let pitch: f32 = 0.35;
    let (syaw, cyaw) = yaw.sin_cos();
    let (spitch, cpitch) = pitch.sin_cos();
    let x = v[0] / sx.max(0.1);
    let y = v[1] / sy.max(0.1);
    let z = v[2] / sz.max(0.1);
    let rx = x * cyaw + z * syaw;
    let rz = -x * syaw + z * cyaw;
    let ry = y * cpitch - rz * spitch;
    let scale = PREVIEW_SIZE as f32 * 0.22;
    [PREVIEW_SIZE as f32 * 0.5 + rx * scale, PREVIEW_SIZE as f32 * 0.55 - ry * scale]
}

fn fill_tri(
    rgba: &mut [u8],
    painted: &mut [bool],
    size: u32,
    a: [f32; 2],
    b: [f32; 2],
    c: [f32; 2],
    color: [u8; 3],
    shade: f32,
) {
    let min_x = a[0].min(b[0]).min(c[0]).floor().max(0.0) as u32;
    let max_x = a[0].max(b[0]).max(c[0]).ceil().min(size as f32 - 1.0) as u32;
    let min_y = a[1].min(b[1]).min(c[1]).floor().max(0.0) as u32;
    let max_y = a[1].max(b[1]).max(c[1]).ceil().min(size as f32 - 1.0) as u32;
    let area = edge(a, b, c);
    if area.abs() < 1e-4 {
        return;
    }
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let p = [x as f32 + 0.5, y as f32 + 0.5];
            let w0 = edge(b, c, p) / area;
            let w1 = edge(c, a, p) / area;
            let w2 = edge(a, b, p) / area;
            if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                let i = (y * size + x) as usize;
                painted[i] = true;
                let o = i * 4;
                rgba[o] = (f32::from(color[0]) * shade) as u8;
                rgba[o + 1] = (f32::from(color[1]) * shade) as u8;
                rgba[o + 2] = (f32::from(color[2]) * shade) as u8;
                rgba[o + 3] = 255;
            }
        }
    }
}

fn edge(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (c[0] - a[0]) * (b[1] - a[1]) - (c[1] - a[1]) * (b[0] - a[0])
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / len, v[1] / len, v[2] / len]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn material_thumbnail_has_non_background_pixels() {
        let dir = std::env::temp_dir().join(format!("seal-mat-thumb-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let mat = dir.join("hero.shade2d");
        fs::write(&mat, "color #cc6622\nr = 0.9\n").unwrap();
        let rgba = material_sphere_rgba(&mat).unwrap();
        let hot = rgba.chunks(4).any(|p| p[3] > 0 && p[0] > 100);
        assert!(hot, "material preview should not be blank");
    }

    #[test]
    fn mesh_thumbnail_has_non_background_pixels() {
        let dir = std::env::temp_dir().join(format!("seal-mesh-thumb-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let mesh = dir.join("rock.mesh");
        fs::write(&mesh, "color #336699\nbounds 1 1.2 0.8\n").unwrap();
        let rgba = mesh_asset_rgba(&mesh).unwrap();
        let hot = rgba.chunks(4).any(|p| p[3] > 0 && p[2] > 50);
        assert!(hot, "mesh preview should not be blank");
    }
}
