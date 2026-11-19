//! Directional shadow maps: the light's view of the scene as a depth grid,
//! and a percentage-closer lookup. Only IEEE exact operations are used
//! (`+ - * /`, comparisons, correctly rounded `sqrt`), so shadows match on
//! every machine. The GPU depth pass uses the same `LightFrame` matrix.

/// An orthographic light view that covers a sphere around `center`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightFrame {
    pub dir: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
    pub center: [f32; 3],
    pub radius: f32,
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt();
    [a[0] / l, a[1] / l, a[2] / l]
}

impl LightFrame {
    /// `dir` is the way the light travels (any length, not zero).
    pub fn fit(dir: [f32; 3], center: [f32; 3], radius: f32) -> LightFrame {
        let dir = norm(dir);
        let helper = if dir[1].abs() > 0.99 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        let right = norm(cross(helper, dir));
        let up = cross(dir, right);
        LightFrame { dir, right, up, center, radius: radius.max(1e-6) }
    }

    /// Texture coordinates (0 to 1) and normalised depth (0 near the light,
    /// 1 far) of a world point.
    pub fn project(&self, p: [f32; 3]) -> [f32; 3] {
        let d = [p[0] - self.center[0], p[1] - self.center[1], p[2] - self.center[2]];
        let r = self.radius;
        [
            (dot(d, self.right) / r) * 0.5 + 0.5,
            (dot(d, self.up) / r) * 0.5 + 0.5,
            (dot(d, self.dir) / r) * 0.5 + 0.5,
        ]
    }
}

/// Nearest depth the light sees through each texel.
pub struct ShadowMap {
    pub size: usize,
    pub frame: LightFrame,
    depth: Vec<f32>,
}

impl ShadowMap {
    pub fn new(size: usize, frame: LightFrame) -> ShadowMap {
        ShadowMap { size, frame, depth: vec![f32::INFINITY; size * size] }
    }

    /// Nearest depth per texel, row by row (infinity where nothing was drawn).
    pub fn depths(&self) -> &[f32] {
        &self.depth
    }

    /// Record a surface point.
    pub fn splat(&mut self, p: [f32; 3]) {
        let t = self.frame.project(p);
        if let Some(i) = self.index(t[0], t[1]) {
            if t[2] < self.depth[i] {
                self.depth[i] = t[2];
            }
        }
    }

    /// Record the surface of an axis-aligned box, sampled finely enough to leave no holes.
    pub fn draw_box(&mut self, min: [f32; 3], max: [f32; 3]) {
        let texel = (self.frame.radius * 2.0) / self.size as f32;
        let step = texel * 0.5;
        let n = |a: usize| (((max[a] - min[a]) / step) as usize).max(1);
        let (nx, ny, nz) = (n(0), n(1), n(2));
        let at =
            |a: usize, i: usize, cnt: usize| min[a] + (max[a] - min[a]) * (i as f32 / cnt as f32);
        for i in 0..=nx {
            for j in 0..=ny {
                self.splat([at(0, i, nx), at(1, j, ny), min[2]]);
                self.splat([at(0, i, nx), at(1, j, ny), max[2]]);
            }
        }
        for i in 0..=nx {
            for k in 0..=nz {
                self.splat([at(0, i, nx), min[1], at(2, k, nz)]);
                self.splat([at(0, i, nx), max[1], at(2, k, nz)]);
            }
        }
        for j in 0..=ny {
            for k in 0..=nz {
                self.splat([min[0], at(1, j, ny), at(2, k, nz)]);
                self.splat([max[0], at(1, j, ny), at(2, k, nz)]);
            }
        }
    }

    /// Record a triangle, filling every texel whose centre it covers plus its edges.
    pub fn draw_triangle(&mut self, a: [f32; 3], b: [f32; 3], c: [f32; 3]) {
        let n = self.size as f32;
        let [ta, tb, tc] = [a, b, c].map(|p| {
            let t = self.frame.project(p);
            [t[0] * n, t[1] * n, t[2]]
        });
        let area = (tb[0] - ta[0]) * (tc[1] - ta[1]) - (tb[1] - ta[1]) * (tc[0] - ta[0]);
        if area.abs() > 1e-9 {
            let x0 = ta[0].min(tb[0]).min(tc[0]).floor().max(0.0) as usize;
            let y0 = ta[1].min(tb[1]).min(tc[1]).floor().max(0.0) as usize;
            let x1 = (ta[0].max(tb[0]).max(tc[0]).ceil().min(n) as usize).min(self.size);
            let y1 = (ta[1].max(tb[1]).max(tc[1]).ceil().min(n) as usize).min(self.size);
            for y in y0..y1 {
                for x in x0..x1 {
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    let w0 = ((tb[0] - px) * (tc[1] - py) - (tb[1] - py) * (tc[0] - px)) / area;
                    let w1 = ((tc[0] - px) * (ta[1] - py) - (tc[1] - py) * (ta[0] - px)) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                        let z = w0 * ta[2] + w1 * tb[2] + w2 * tc[2];
                        let i = y * self.size + x;
                        if z < self.depth[i] {
                            self.depth[i] = z;
                        }
                    }
                }
            }
        }
        // Edges, so thin or edge-on triangles still leave no gaps along outlines.
        let texel = (self.frame.radius * 2.0) / n;
        for (p, q) in [(a, b), (b, c), (c, a)] {
            let len =
                ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) + (q[2] - p[2]).powi(2)).sqrt();
            let steps = ((len / (texel * 0.5)) as usize).max(1);
            for k in 0..=steps {
                let t = k as f32 / steps as f32;
                self.splat([
                    p[0] + (q[0] - p[0]) * t,
                    p[1] + (q[1] - p[1]) * t,
                    p[2] + (q[2] - p[2]) * t,
                ]);
            }
        }
    }

    fn index(&self, u: f32, v: f32) -> Option<usize> {
        if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
            return None;
        }
        let (x, y) = ((u * self.size as f32) as usize, (v * self.size as f32) as usize);
        Some(y * self.size + x)
    }

    /// Fraction of the light that reaches `p` (1 lit, 0 shadowed), averaged
    /// over a (2*pcf+1) square of texels. `bias` is in normalised depth.
    pub fn lit(&self, p: [f32; 3], bias: f32, pcf: i32) -> f32 {
        let t = self.frame.project(p);
        let texel = 1.0 / self.size as f32;
        let (mut lit, mut total) = (0u32, 0u32);
        for dy in -pcf..=pcf {
            for dx in -pcf..=pcf {
                total += 1;
                let (u, v) = (t[0] + dx as f32 * texel, t[1] + dy as f32 * texel);
                match self.index(u, v) {
                    Some(i) if t[2] - bias > self.depth[i] => {}
                    _ => lit += 1,
                }
            }
        }
        lit as f32 / total as f32
    }
}

#[cfg(test)]
mod triangle_tests {
    use super::*;

    #[test]
    fn a_triangle_shadows_what_is_under_it_and_not_beside_it() {
        let frame = LightFrame::fit([0.0, -1.0, 0.0], [0.0, 0.0, 0.0], 2.0);
        let mut map = ShadowMap::new(64, frame);
        map.draw_triangle([-1.0, 1.0, -1.0], [1.0, 1.0, -1.0], [-1.0, 1.0, 1.0]);
        assert_eq!(map.lit([-0.5, 0.0, -0.5], 0.01, 0), 0.0);
        assert_eq!(map.lit([0.8, 0.0, 0.8], 0.01, 0), 1.0);
        assert_eq!(map.lit([-0.5, 1.5, -0.5], 0.01, 0), 1.0);
    }
}
