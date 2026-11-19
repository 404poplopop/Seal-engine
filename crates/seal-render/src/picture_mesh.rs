//! A tight sprite mesh from opaque pixels, in cell space (y up).

use std::path::Path;

/// Outline of the opaque pixels. A picture with no opaque pixels, or one that
/// already fills its rectangle, returns the four corners of that rectangle.
pub fn tight_mesh(rgba: &[u8], width: u32, height: u32) -> Vec<[f32; 2]> {
    let full = unit_quad();
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return full;
    }
    let mut points = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            if rgba[i + 3] < 128 {
                continue;
            }
            let x0 = x as f32 / width as f32;
            let x1 = (x + 1) as f32 / width as f32;
            let v1 = 1.0 - y as f32 / height as f32;
            let v0 = 1.0 - (y + 1) as f32 / height as f32;
            points.push([x0, v0]);
            points.push([x1, v0]);
            points.push([x1, v1]);
            points.push([x0, v1]);
        }
    }
    if points.is_empty() {
        return full;
    }
    let hull = convex_hull(points);
    if hull.len() < 3 || polygon_area(&hull) >= 0.98 {
        full
    } else {
        hull
    }
}

/// Area of a polygon. `1` is the full rectangle.
pub fn polygon_area(points: &[[f32; 2]]) -> f32 {
    if points.len() < 3 {
        return 0.0;
    }
    let mut sum = 0.0f32;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        sum += a[0] * b[1] - b[0] * a[1];
    }
    (sum * 0.5).abs()
}

/// Whether every opaque pixel centre sits inside `mesh`.
pub fn covers_opaque(rgba: &[u8], width: u32, height: u32, mesh: &[[f32; 2]]) -> bool {
    let (w, h) = (width as usize, height as usize);
    if mesh.len() < 3 || rgba.len() < w * h * 4 {
        return false;
    }
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            if rgba[i + 3] < 128 {
                continue;
            }
            let point = [(x as f32 + 0.5) / width as f32, 1.0 - (y as f32 + 0.5) / height as f32];
            if !contains(mesh, point) {
                return false;
            }
        }
    }
    true
}

pub(crate) fn mesh_sidecar(path: &Path) -> std::path::PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".spmesh");
    std::path::PathBuf::from(name)
}

pub(crate) fn read_mesh_file(path: &Path) -> Option<Vec<[f32; 2]>> {
    let text = std::fs::read_to_string(path).ok()?;
    read_mesh_text(&text)
}

pub fn read_mesh_text(text: &str) -> Option<Vec<[f32; 2]>> {
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    let count: usize = lines.next()?.trim().parse().ok()?;
    if count < 3 || count > 4096 {
        return None;
    }
    let mut points = Vec::with_capacity(count);
    for line in lines.take(count) {
        let mut parts = line.split_whitespace();
        let u: f32 = parts.next()?.parse().ok()?;
        let v: f32 = parts.next()?.parse().ok()?;
        if !u.is_finite() || !v.is_finite() {
            return None;
        }
        points.push([u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)]);
    }
    (points.len() == count).then_some(points)
}

pub fn write_mesh_text(points: &[[f32; 2]]) -> String {
    let mut text = format!("{}\n", points.len());
    for point in points {
        text.push_str(&format!("{:.6} {:.6}\n", point[0], point[1]));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tight_mesh_covers_opaque_pixels_and_is_smaller_than_the_rectangle() {
        let mut rgba = vec![0u8; 8 * 8 * 4];
        for y in 2..4 {
            for x in 2..4 {
                let i = (y * 8 + x) * 4;
                rgba[i] = 255;
                rgba[i + 3] = 255;
            }
        }
        let mesh = tight_mesh(&rgba, 8, 8);
        assert!(covers_opaque(&rgba, 8, 8, &mesh), "{mesh:?}");
        assert!(polygon_area(&mesh) < 0.5, "{}", polygon_area(&mesh));
        let full = tight_mesh(&[255u8, 0, 0, 255].repeat(4), 2, 2);
        assert!((polygon_area(&full) - 1.0).abs() < 1.0e-3);
        let text = write_mesh_text(&mesh);
        assert_eq!(read_mesh_text(&text).as_deref(), Some(mesh.as_slice()));
        assert!(read_mesh_text("nope").is_none());
        assert!(std::panic::catch_unwind(|| read_mesh_text("2\n0 0\n")).unwrap().is_none());
    }
}

fn unit_quad() -> Vec<[f32; 2]> {
    vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
}

fn convex_hull(mut points: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    points.dedup_by(|a, b| (a[0] - b[0]).abs() < 1.0e-6 && (a[1] - b[1]).abs() < 1.0e-6);
    if points.len() <= 1 {
        return points;
    }
    let cross = |o: [f32; 2], a: [f32; 2], b: [f32; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut lower = Vec::new();
    for point in &points {
        while lower.len() >= 2
            && cross(lower[lower.len() - 2], lower[lower.len() - 1], *point) <= 0.0
        {
            lower.pop();
        }
        lower.push(*point);
    }
    let mut upper = Vec::new();
    for point in points.iter().rev() {
        while upper.len() >= 2
            && cross(upper[upper.len() - 2], upper[upper.len() - 1], *point) <= 0.0
        {
            upper.pop();
        }
        upper.push(*point);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

fn contains(poly: &[[f32; 2]], point: [f32; 2]) -> bool {
    let mut inside = false;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        if on_segment(point, a, b) {
            return true;
        }
        let crosses = (a[1] > point[1]) != (b[1] > point[1]);
        if crosses {
            let x = (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0];
            if point[0] < x {
                inside = !inside;
            }
        }
    }
    inside
}

fn on_segment(point: [f32; 2], a: [f32; 2], b: [f32; 2]) -> bool {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let ap = [point[0] - a[0], point[1] - a[1]];
    let cross = ab[0] * ap[1] - ab[1] * ap[0];
    let dot = ap[0] * ab[0] + ap[1] * ab[1];
    let len2 = ab[0] * ab[0] + ab[1] * ab[1];
    cross.abs() <= 1.0e-5 && dot >= -1.0e-5 && dot <= len2 + 1.0e-5
}
