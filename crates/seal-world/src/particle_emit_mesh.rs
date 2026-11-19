//! Triangle and vertex data for mesh emit shapes and mesh particle renderers.

use std::path::Path;

use super::particle_types::MeshSampleMode;

/// Resolved mesh geometry in emitter-local space.
#[derive(Clone, Debug, PartialEq)]
pub struct EmitMeshData {
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

pub fn primitive_mesh(kind: &str) -> EmitMeshData {
    match kind {
        "sphere" => sphere_mesh(12, 8),
        "plane" => plane_mesh(),
        _ => cube_mesh(),
    }
}

pub fn load_emit_mesh(path: &str, project: Option<&Path>) -> EmitMeshData {
    let full = resolve(path, project);
    if let Ok(text) = std::fs::read_to_string(&full) {
        if let Some(data) = parse_mesh_text(&text) {
            return data;
        }
    }
    primitive_mesh(path)
}

fn resolve(path: &str, project: Option<&Path>) -> std::path::PathBuf {
    let raw = Path::new(path);
    if raw.is_absolute() {
        raw.to_path_buf()
    } else if let Some(root) = project {
        root.join(raw)
    } else {
        raw.to_path_buf()
    }
}

pub fn parse_mesh_text(text: &str) -> Option<EmitMeshData> {
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut words = line.split_whitespace();
        let tag = words.next()?;
        match tag {
            "vertex" | "v" => {
                let x: f32 = words.next()?.parse().ok()?;
                let y: f32 = words.next()?.parse().ok()?;
                let z: f32 = words.next()?.parse().ok()?;
                if words.next().is_none() {
                    vertices.push([x, y, z]);
                }
            }
            "tri" | "t" | "face" | "f" => {
                let a: u32 = words.next()?.parse().ok()?;
                let b: u32 = words.next()?.parse().ok()?;
                let c: u32 = words.next()?.parse().ok()?;
                if words.next().is_none() {
                    triangles.push([a, b, c]);
                }
            }
            _ => {}
        }
    }
    if vertices.is_empty() {
        return None;
    }
    if triangles.is_empty() && vertices.len() >= 3 {
        for i in (0..vertices.len().saturating_sub(2)).step_by(3) {
            triangles.push([i as u32, i as u32 + 1, i as u32 + 2]);
        }
    }
    Some(EmitMeshData { vertices, triangles })
}

pub fn sample_on_mesh(data: &EmitMeshData, mode: MeshSampleMode, mut rand: impl FnMut() -> f32) -> [f32; 3] {
    match mode {
        MeshSampleMode::Vertices => {
            if data.vertices.is_empty() {
                return [0.0; 3];
            }
            let i = (rand().abs() * data.vertices.len() as f32).floor() as usize;
            data.vertices[i.min(data.vertices.len() - 1)]
        }
        MeshSampleMode::Surface => {
            if data.triangles.is_empty() {
                return sample_on_mesh(data, MeshSampleMode::Vertices, rand);
            }
            let mut areas = Vec::with_capacity(data.triangles.len());
            let mut total = 0.0f32;
            for tri in &data.triangles {
                let a = data.vertices.get(tri[0] as usize).copied().unwrap_or([0.0; 3]);
                let b = data.vertices.get(tri[1] as usize).copied().unwrap_or([0.0; 3]);
                let c = data.vertices.get(tri[2] as usize).copied().unwrap_or([0.0; 3]);
                let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                let cross = [
                    ab[1] * ac[2] - ab[2] * ac[1],
                    ab[2] * ac[0] - ab[0] * ac[2],
                    ab[0] * ac[1] - ab[1] * ac[0],
                ];
                let area = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt() * 0.5;
                areas.push(area.max(0.0));
                total += area.max(0.0);
            }
            let pick = rand().abs() * total.max(1e-6);
            let mut acc = 0.0;
            let mut tri_idx = 0;
            for (i, area) in areas.iter().enumerate() {
                acc += *area;
                if pick <= acc {
                    tri_idx = i;
                    break;
                }
            }
            let tri = data.triangles[tri_idx];
            let a = data.vertices.get(tri[0] as usize).copied().unwrap_or([0.0; 3]);
            let b = data.vertices.get(tri[1] as usize).copied().unwrap_or([0.0; 3]);
            let c = data.vertices.get(tri[2] as usize).copied().unwrap_or([0.0; 3]);
            let mut u = rand().abs();
            let mut v = rand().abs();
            if u + v > 1.0 {
                u = 1.0 - u;
                v = 1.0 - v;
            }
            let w = 1.0 - u - v;
            [a[0] * u + b[0] * v + c[0] * w, a[1] * u + b[1] * v + c[1] * w, a[2] * u + b[2] * v + c[2] * w]
        }
    }
}

pub fn offset_on_mesh(data: &EmitMeshData, offset: [f32; 3], mode: MeshSampleMode) -> bool {
    const EPS: f32 = 0.02;
    match mode {
        MeshSampleMode::Vertices => data
            .vertices
            .iter()
            .any(|v| (v[0] - offset[0]).hypot(v[1] - offset[1]) < EPS && (v[2] - offset[2]).abs() < EPS),
        MeshSampleMode::Surface => {
            if data.triangles.is_empty() {
                return offset_on_mesh(data, offset, MeshSampleMode::Vertices);
            }
            data.triangles.iter().any(|tri| {
                let a = data.vertices.get(tri[0] as usize).copied().unwrap_or([0.0; 3]);
                let b = data.vertices.get(tri[1] as usize).copied().unwrap_or([0.0; 3]);
                let c = data.vertices.get(tri[2] as usize).copied().unwrap_or([0.0; 3]);
                point_in_tri(offset, a, b, c, EPS)
            })
        }
    }
}

fn point_in_tri(p: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3], eps: f32) -> bool {
    let v0 = [c[0] - a[0], c[1] - a[1]];
    let v1 = [b[0] - a[0], b[1] - a[1]];
    let v2 = [p[0] - a[0], p[1] - a[1]];
    let dot00 = v0[0] * v0[0] + v0[1] * v0[1];
    let dot01 = v0[0] * v1[0] + v0[1] * v1[1];
    let dot02 = v0[0] * v2[0] + v0[1] * v2[1];
    let dot11 = v1[0] * v1[0] + v1[1] * v1[1];
    let dot12 = v1[0] * v2[0] + v1[1] * v2[1];
    let inv = 1.0 / (dot00 * dot11 - dot01 * dot01).max(1e-8);
    let u = (dot11 * dot02 - dot01 * dot12) * inv;
    let v = (dot00 * dot12 - dot01 * dot02) * inv;
        u >= -eps && v >= -eps && (u + v) <= 1.0 + eps
}

fn cube_mesh() -> EmitMeshData {
    let h = 0.5f32;
    let v = [
        [-h, -h, -h],
        [h, -h, -h],
        [h, h, -h],
        [-h, h, -h],
        [-h, -h, h],
        [h, -h, h],
        [h, h, h],
        [-h, h, h],
    ];
    let t = [
        [0, 1, 2],
        [0, 2, 3],
        [4, 6, 5],
        [4, 7, 6],
        [0, 4, 5],
        [0, 5, 1],
        [2, 6, 7],
        [2, 7, 3],
        [0, 3, 7],
        [0, 7, 4],
        [1, 5, 6],
        [1, 6, 2],
    ];
    EmitMeshData {
        vertices: v.to_vec(),
        triangles: t.to_vec(),
    }
}

fn plane_mesh() -> EmitMeshData {
    EmitMeshData {
        vertices: vec![[-0.5, 0.0, -0.5], [0.5, 0.0, -0.5], [0.5, 0.0, 0.5], [-0.5, 0.0, 0.5]],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
    }
}

fn sphere_mesh(segments: usize, rings: usize) -> EmitMeshData {
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    for y in 0..=rings {
        let v = y as f32 / rings as f32;
        let phi = v * std::f32::consts::PI;
        for x in 0..segments {
            let u = x as f32 / segments as f32;
            let theta = u * std::f32::consts::TAU;
            let sx = phi.sin() * theta.cos() * 0.5;
            let sy = phi.cos() * 0.5;
            let sz = phi.sin() * theta.sin() * 0.5;
            vertices.push([sx, sy, sz]);
        }
    }
    for y in 0..rings {
        for x in 0..segments {
            let i0 = y * segments + x;
            let i1 = i0 + 1;
            let i2 = i0 + segments;
            let i3 = i2 + 1;
            let wrap = |i: usize| (i % segments) + y * segments;
            let a = wrap(i0) as u32;
            let b = wrap(i1) as u32;
            let c = (i2 + x) as u32;
            let d = (i3 + x) as u32;
            if y > 0 {
                triangles.push([a, c, b]);
            }
            if y < rings - 1 {
                triangles.push([b, c, d]);
            }
        }
    }
    EmitMeshData { vertices, triangles }
}

pub fn mesh_renderer_primitive(path: &str) -> Option<&'static str> {
    match path {
        "cube" => Some("cube"),
        "sphere" => Some("sphere"),
        "plane" => Some("plane"),
        "cylinder" => Some("cylinder"),
        "cone" => Some("cone"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_text_round_trip() {
        let text = "vertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\ntri 0 1 2\n";
        let data = parse_mesh_text(text).unwrap();
        assert_eq!(data.vertices.len(), 3);
        assert_eq!(data.triangles.len(), 1);
    }

    #[test]
    fn damaged_mesh_text_falls_back_to_primitive() {
        assert!(parse_mesh_text("oops\n").is_none());
    }

    #[test]
    fn samples_lie_on_cube_surface() {
        let data = cube_mesh();
        let mut seed = 3u32;
        let mut rand = || {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            (seed % 10_000) as f32 / 10_000.0
        };
        for _ in 0..40 {
            let p = sample_on_mesh(&data, MeshSampleMode::Surface, || rand());
            assert!(offset_on_mesh(&data, p, MeshSampleMode::Surface), "{p:?}");
        }
    }
}