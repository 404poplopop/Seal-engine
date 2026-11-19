//! Diffuse global illumination for irradiance probes, gathered by ray casts on
//! the CPU. Each probe shoots a fixed set of directions into the scene's boxes:
//! a ray that hits a box picks up the light that box reflects (its colour times
//! the direct light on it, with shadows), and a ray that escapes picks up the
//! sky. Lights the probe sees directly are added with shadows too. The result
//! is ordinary probe SH, so `ProbeGrid::sample` shades with it unchanged.
//!
//! Ray tests loop over every box, which the straight-line kernel language
//! cannot express, so the bake stays on the CPU and is split across threads.

use crate::probes::{Light, ProbeGrid, Sh};

const Y0: f32 = 0.282_095;
const Y1: f32 = 0.488_603;
const PI: f32 = std::f32::consts::PI;

/// A solid axis-aligned box that blocks and reflects light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Occluder {
    pub min: [f32; 3],
    pub max: [f32; 3],
    /// Diffuse colour, 0 to 1 per channel.
    pub albedo: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct GiScene {
    pub occluders: Vec<Occluder>,
    pub lights: Vec<Light>,
    /// Light from rays that hit nothing.
    pub sky: [f32; 3],
    /// Add the lights a probe sees directly. Off when the renderer already
    /// shades direct light and only wants the bounced part.
    pub direct: bool,
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Nearest hit of a ray with a box: distance and the face normal.
pub fn ray_box(o: [f32; 3], d: [f32; 3], b: &Occluder) -> Option<(f32, [f32; 3])> {
    let mut t0 = 0.0f32;
    let mut t1 = f32::INFINITY;
    let mut axis = usize::MAX;
    let mut sign = 0.0f32;
    for a in 0..3 {
        if d[a].abs() < 1e-12 {
            if o[a] < b.min[a] || o[a] > b.max[a] {
                return None;
            }
            continue;
        }
        let inv = 1.0 / d[a];
        let (mut n, mut f) = ((b.min[a] - o[a]) * inv, (b.max[a] - o[a]) * inv);
        let mut s = -1.0;
        if n > f {
            std::mem::swap(&mut n, &mut f);
            s = 1.0;
        }
        if n > t0 {
            t0 = n;
            axis = a;
            sign = s;
        }
        t1 = t1.min(f);
        if t0 > t1 {
            return None;
        }
    }
    if axis == usize::MAX {
        // The origin is inside the box.
        return None;
    }
    let mut normal = [0.0; 3];
    normal[axis] = sign;
    Some((t0, normal))
}

impl GiScene {
    fn hit(&self, o: [f32; 3], d: [f32; 3], max: f32) -> Option<(f32, [f32; 3], usize)> {
        let mut best: Option<(f32, [f32; 3], usize)> = None;
        for (i, b) in self.occluders.iter().enumerate() {
            if let Some((t, n)) = ray_box(o, d, b) {
                if t > 1e-4 && t < max && best.is_none_or(|(bt, _, _)| t < bt) {
                    best = Some((t, n, i));
                }
            }
        }
        best
    }

    fn sees(&self, p: [f32; 3], toward: [f32; 3], dist: f32) -> bool {
        self.hit(p, toward, dist).is_none()
    }

    /// Direct light on a surface at `p` facing `n`, scaled so a white light
    /// straight on gives 1/π (the same scale `probes::irradiance` uses).
    pub fn direct_at(&self, p: [f32; 3], n: [f32; 3]) -> [f32; 3] {
        let lift = [p[0] + n[0] * 1e-3, p[1] + n[1] * 1e-3, p[2] + n[2] * 1e-3];
        let mut out = [0.0; 3];
        for l in &self.lights {
            let (dir, color, dist) = match *l {
                Light::Directional { toward, color } => (toward, color, f32::INFINITY),
                Light::Point { pos, color, range } => {
                    let d = [pos[0] - p[0], pos[1] - p[1], pos[2] - p[2]];
                    let len = dot(d, d).sqrt();
                    if len <= 1e-6 || len >= range {
                        continue;
                    }
                    let f = 1.0 - (len / range) * (len / range);
                    let k = f * f;
                    (
                        [d[0] / len, d[1] / len, d[2] / len],
                        [color[0] * k, color[1] * k, color[2] * k],
                        len,
                    )
                }
                Light::Ambient { color } => {
                    for c in 0..3 {
                        out[c] += color[c];
                    }
                    continue;
                }
            };
            let cos = dot(n, dir);
            if cos <= 0.0 || !self.sees(lift, dir, dist) {
                continue;
            }
            for c in 0..3 {
                out[c] += color[c] * cos / PI;
            }
        }
        out
    }

    /// Light arriving at `p` as SH, from `rays` directions.
    pub fn bake_point(&self, p: [f32; 3], rays: usize) -> Sh {
        let mut sh: Sh = [[0.0; 3]; 4];
        let rays = rays.max(1);
        let weight = 4.0 * PI / rays as f32;
        for d in sphere_dirs(rays) {
            let radiance = match self.hit(p, d, f32::INFINITY) {
                Some((t, n, i)) => {
                    let q = [p[0] + d[0] * t, p[1] + d[1] * t, p[2] + d[2] * t];
                    let e = self.direct_at(q, n);
                    let a = self.occluders[i].albedo;
                    [a[0] * e[0], a[1] * e[1], a[2] * e[2]]
                }
                None => self.sky,
            };
            let basis = [Y0, Y1 * d[1], Y1 * d[2], Y1 * d[0]];
            for (k, b) in basis.iter().enumerate() {
                for c in 0..3 {
                    sh[k][c] += radiance[c] * b * weight;
                }
            }
        }
        if self.direct {
            for l in &self.lights {
                let (dir, color, dist) = match *l {
                    Light::Directional { toward, color } => (toward, color, f32::INFINITY),
                    Light::Point { pos, color, range } => {
                        let d = [pos[0] - p[0], pos[1] - p[1], pos[2] - p[2]];
                        let len = dot(d, d).sqrt();
                        if len <= 1e-6 || len >= range {
                            continue;
                        }
                        let f = 1.0 - (len / range) * (len / range);
                        let k = f * f;
                        (
                            [d[0] / len, d[1] / len, d[2] / len],
                            [color[0] * k, color[1] * k, color[2] * k],
                            len,
                        )
                    }
                    Light::Ambient { color } => {
                        for c in 0..3 {
                            sh[0][c] += color[c] * 3.544_907_7;
                        }
                        continue;
                    }
                };
                if !self.sees(p, dir, dist) {
                    continue;
                }
                let basis = [Y0, Y1 * dir[1], Y1 * dir[2], Y1 * dir[0]];
                for (k, b) in basis.iter().enumerate() {
                    for c in 0..3 {
                        sh[k][c] += color[c] * b;
                    }
                }
            }
        }
        sh
    }

    /// A probe grid with bounced light, baked on every core.
    pub fn bake_grid(
        &self,
        origin: [f32; 3],
        spacing: f32,
        dims: [usize; 3],
        rays: usize,
    ) -> ProbeGrid {
        let dims = [dims[0].max(1), dims[1].max(1), dims[2].max(1)];
        let count = dims[0] * dims[1] * dims[2];
        let point = |i: usize| {
            let (x, y, z) = (i % dims[0], (i / dims[0]) % dims[1], i / (dims[0] * dims[1]));
            [
                origin[0] + x as f32 * spacing,
                origin[1] + y as f32 * spacing,
                origin[2] + z as f32 * spacing,
            ]
        };
        let mut probes: Vec<Sh> = vec![[[0.0; 3]; 4]; count];
        let jobs = std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, 16);
        let chunk = count.div_ceil(jobs).max(1);
        std::thread::scope(|scope| {
            for (j, part) in probes.chunks_mut(chunk).enumerate() {
                scope.spawn(move || {
                    for (k, slot) in part.iter_mut().enumerate() {
                        *slot = self.bake_point(point(j * chunk + k), rays);
                    }
                });
            }
        });
        ProbeGrid { origin, spacing, dims, probes }
    }
}

/// `n` directions spread evenly over the sphere (a Fibonacci spiral), the same every run.
pub fn sphere_dirs(n: usize) -> Vec<[f32; 3]> {
    let golden = PI * (3.0 - 5.0f32.sqrt());
    (0..n)
        .map(|i| {
            let y = 1.0 - (i as f32 + 0.5) * 2.0 / n as f32;
            let r = (1.0 - y * y).max(0.0).sqrt();
            let a = golden * i as f32;
            [r * a.cos(), y, r * a.sin()]
        })
        .collect()
}
