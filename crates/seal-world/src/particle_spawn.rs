//! Spawn offsets inside emit shapes and burst scheduling.

use std::path::Path;

use super::particle_emit_mesh::{load_emit_mesh, sample_on_mesh};
use super::particle_types::{Burst, EmitShape, ParticleExtras, SphereVolume};

const TAU: f32 = std::f32::consts::TAU;

pub fn sample_emit_offset(shape: &EmitShape, rand: impl FnMut() -> f32) -> [f32; 3] {
    sample_emit_offset_project(shape, None, rand)
}

pub fn sample_emit_offset_project(
    shape: &EmitShape,
    project: Option<&Path>,
    mut rand: impl FnMut() -> f32,
) -> [f32; 3] {
    match shape {
        EmitShape::Point => [0.0; 3],
        EmitShape::Circle(r) => {
            let a = rand() * TAU;
            let u = rand().abs();
            let dist = u.sqrt() * r.max(0.0);
            [a.cos() * dist, a.sin() * dist, 0.0]
        }
        EmitShape::Ring(inner, outer) => {
            let a = rand() * TAU;
            let t = rand().abs();
            let r0 = inner.max(0.0);
            let r1 = outer.max(r0);
            let dist = r0 + t * (r1 - r0);
            [a.cos() * dist, a.sin() * dist, 0.0]
        }
        EmitShape::Box(h) => [rand() * h[0].max(0.0), rand() * h[1].max(0.0), 0.0],
        EmitShape::Edge(e) => {
            let t = (rand() + 1.0) * 0.5;
            [e[0] + (e[2] - e[0]) * t, e[1] + (e[3] - e[1]) * t, 0.0]
        }
        EmitShape::Cone(dir_deg, half_deg) => {
            let dir = dir_deg.to_radians();
            let half = half_deg.to_radians().max(0.0);
            let a = dir + rand() * half * 2.0;
            let dist = rand().abs() * 0.5 + 0.01;
            [a.sin() * dist, a.cos() * dist, 0.0]
        }
        EmitShape::Sphere(r, vol) => {
            let rad = r.max(0.0);
            let u = rand() * TAU;
            let v = rand().abs();
            let cos_phi = 1.0 - v * 2.0;
            let sin_phi = (1.0 - cos_phi * cos_phi).max(0.0).sqrt();
            let x = u.cos() * sin_phi;
            let y = cos_phi;
            let z = u.sin() * sin_phi;
            match vol {
                SphereVolume::Volume => {
                    let scale = v.cbrt() * rad;
                    [x * scale, y * scale, z * scale]
                }
                SphereVolume::Surface => [x * rad, y * rad, z * rad],
            }
        }
        EmitShape::Mesh { path, mode } => {
            let data = load_emit_mesh(path, project);
            sample_on_mesh(&data, *mode, rand)
        }
    }
}

/// Whether `offset` lies inside `shape` (for tests; cone uses its wedge).
pub fn offset_inside_shape(shape: &EmitShape, offset: [f32; 3]) -> bool {
    let (x, y) = (offset[0], offset[1]);
    const EPS: f32 = 1e-4;
    match shape {
        EmitShape::Point => x.abs() < EPS && y.abs() < EPS,
        EmitShape::Circle(r) => (x * x + y * y) <= r * r + EPS,
        EmitShape::Ring(inner, outer) => {
            let d2 = x * x + y * y;
            d2 >= inner * inner - EPS && d2 <= outer * outer + EPS
        }
        EmitShape::Box(h) => x.abs() <= h[0] + EPS && y.abs() <= h[1] + EPS,
        EmitShape::Edge(e) => {
            let (x0, y0, x1, y1) = (e[0], e[1], e[2], e[3]);
            let dx = x1 - x0;
            let dy = y1 - y0;
            let len2 = dx * dx + dy * dy;
            if len2 < EPS {
                return (x - x0).abs() < EPS && (y - y0).abs() < EPS;
            }
            let t = ((x - x0) * dx + (y - y0) * dy) / len2;
            if !(0.0..=1.0).contains(&t) {
                return false;
            }
            let px = x0 + dx * t;
            let py = y0 + dy * t;
            (x - px).abs() < EPS && (y - py).abs() < EPS
        }
        EmitShape::Cone(dir_deg, half_deg) => {
            let dist = (x * x + y * y).sqrt();
            if dist > 0.51 + EPS {
                return false;
            }
            let angle = y.atan2(x);
            let dir = dir_deg.to_radians();
            let half = half_deg.to_radians().max(0.0);
            let diff = (angle - dir + std::f32::consts::PI).rem_euclid(TAU) - std::f32::consts::PI;
            diff.abs() <= half + EPS
        }
        EmitShape::Sphere(r, vol) => {
            let d2 = x * x + y * y + offset[2] * offset[2];
            let rad = r.max(0.0);
            match vol {
                SphereVolume::Volume => d2 <= rad * rad + EPS,
                SphereVolume::Surface => (d2.sqrt() - rad).abs() <= EPS.max(rad * 0.05),
            }
        }
        EmitShape::Mesh { path, mode } => {
            let data = load_emit_mesh(path, None);
            super::particle_emit_mesh::offset_on_mesh(&data, offset, *mode)
        }
    }
}

/// Bursts due since `last_time` up to `now`, resetting cycle when `burst_repeat` > 0.
pub fn bursts_due(extras: &ParticleExtras, last_time: f32, now: f32) -> Vec<(f32, u32)> {
    if extras.bursts.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let repeat = extras.burst_repeat;
    if repeat <= 0.0 {
        for b in &extras.bursts {
            if b.time > last_time && b.time <= now {
                out.push((b.time, b.count));
            }
        }
        return out;
    }
    let mut cycle = (last_time / repeat).floor() as i32;
    let end_cycle = (now / repeat).floor() as i32;
    while cycle <= end_cycle {
        let base = cycle as f32 * repeat;
        for b in &extras.bursts {
            let t = base + b.time;
            if t > last_time && t <= now {
                out.push((t, b.count));
            }
        }
        cycle += 1;
    }
    out
}

pub fn sort_bursts(bursts: &mut [Burst]) {
    bursts.sort_by(|a, b| a.time.total_cmp(&b.time));
}
