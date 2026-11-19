//! Irradiance probes: a regular grid of order-1 spherical-harmonic (4
//! coefficients per colour) samples of the light arriving at each point,
//! baked on the CPU from the scene's lights and sampled by lit shading.
//! Light is not blocked by geometry yet (no occlusion in the bake).

const Y0: f32 = 0.282_095;
const Y1: f32 = 0.488_603;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Light {
    /// `toward` points from the scene to the light (unit length).
    Directional {
        toward: [f32; 3],
        color: [f32; 3],
    },
    Point {
        pos: [f32; 3],
        color: [f32; 3],
        range: f32,
    },
    /// Equal light from every direction.
    Ambient {
        color: [f32; 3],
    },
}

/// SH coefficients per colour channel: [constant, y, z, x].
pub type Sh = [[f32; 3]; 4];

fn add_dir(sh: &mut Sh, w: [f32; 3], c: [f32; 3]) {
    let basis = [Y0, Y1 * w[1], Y1 * w[2], Y1 * w[0]];
    for (k, b) in basis.iter().enumerate() {
        for ch in 0..3 {
            sh[k][ch] += c[ch] * b;
        }
    }
}

/// Light arriving at `p` as SH.
pub fn bake_point(p: [f32; 3], lights: &[Light]) -> Sh {
    let mut sh = [[0.0; 3]; 4];
    for l in lights {
        match *l {
            Light::Directional { toward, color } => add_dir(&mut sh, toward, color),
            Light::Ambient { color } => {
                // Uniform radiance L has only the constant term, L / Y0 * (Y0 * Y0 * 4π / (4π)) = L * 3.5449.
                for ch in 0..3 {
                    sh[0][ch] += color[ch] * 3.544_907_7;
                }
            }
            Light::Point { pos, color, range } => {
                let d = [pos[0] - p[0], pos[1] - p[1], pos[2] - p[2]];
                let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                if dist <= 1e-6 || dist >= range {
                    continue;
                }
                let f = 1.0 - (dist / range) * (dist / range);
                let k = f * f;
                add_dir(
                    &mut sh,
                    [d[0] / dist, d[1] / dist, d[2] / dist],
                    [color[0] * k, color[1] * k, color[2] * k],
                );
            }
        }
    }
    sh
}

/// Cosine-weighted light reaching a surface facing `n` (unit), divided by π
/// so that a white ambient light of 1 gives 1.
pub fn irradiance(sh: &Sh, n: [f32; 3]) -> [f32; 3] {
    let (a0, a1) = (1.0f32, 2.0f32 / 3.0);
    let mut out = [0.0; 3];
    for ch in 0..3 {
        let e = a0 * Y0 * sh[0][ch]
            + a1 * Y1 * (sh[1][ch] * n[1] + sh[2][ch] * n[2] + sh[3][ch] * n[0]);
        out[ch] = (e).max(0.0);
    }
    out
}

pub struct ProbeGrid {
    pub origin: [f32; 3],
    pub spacing: f32,
    pub dims: [usize; 3],
    pub probes: Vec<Sh>,
}

impl ProbeGrid {
    pub fn bake(origin: [f32; 3], spacing: f32, dims: [usize; 3], lights: &[Light]) -> ProbeGrid {
        let dims = [dims[0].max(1), dims[1].max(1), dims[2].max(1)];
        let mut probes = Vec::with_capacity(dims[0] * dims[1] * dims[2]);
        for z in 0..dims[2] {
            for y in 0..dims[1] {
                for x in 0..dims[0] {
                    let p = [
                        origin[0] + x as f32 * spacing,
                        origin[1] + y as f32 * spacing,
                        origin[2] + z as f32 * spacing,
                    ];
                    probes.push(bake_point(p, lights));
                }
            }
        }
        ProbeGrid { origin, spacing, dims, probes }
    }

    fn at(&self, x: usize, y: usize, z: usize) -> &Sh {
        &self.probes[(z * self.dims[1] + y) * self.dims[0] + x]
    }

    /// Trilinearly blended light at `p` (clamped to the grid) for a surface facing `n`.
    pub fn sample(&self, p: [f32; 3], n: [f32; 3]) -> [f32; 3] {
        let mut lo = [0usize; 3];
        let mut f = [0.0f32; 3];
        for a in 0..3 {
            let g = ((p[a] - self.origin[a]) / self.spacing).clamp(0.0, (self.dims[a] - 1) as f32);
            let i = (g as usize).min(self.dims[a].saturating_sub(2));
            lo[a] = if self.dims[a] == 1 { 0 } else { i };
            f[a] = if self.dims[a] == 1 { 0.0 } else { g - i as f32 };
        }
        let mut blended: Sh = [[0.0; 3]; 4];
        for c in 0..8usize {
            let (dx, dy, dz) = (c & 1, (c >> 1) & 1, (c >> 2) & 1);
            let w = (if dx == 1 { f[0] } else { 1.0 - f[0] })
                * (if dy == 1 { f[1] } else { 1.0 - f[1] })
                * (if dz == 1 { f[2] } else { 1.0 - f[2] });
            if w == 0.0 {
                continue;
            }
            let (x, y, z) = (
                (lo[0] + dx).min(self.dims[0] - 1),
                (lo[1] + dy).min(self.dims[1] - 1),
                (lo[2] + dz).min(self.dims[2] - 1),
            );
            let s = self.at(x, y, z);
            for k in 0..4 {
                for ch in 0..3 {
                    blended[k][ch] += s[k][ch] * w;
                }
            }
        }
        irradiance(&blended, n)
    }
}
