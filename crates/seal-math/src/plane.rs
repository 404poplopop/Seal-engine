//! Infinite planes: normal and signed distance (n·p + d = 0).

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    pub nx: f32,
    pub ny: f32,
    pub nz: f32,
    pub d: f32,
}

impl Plane {
    pub fn from_point_normal(point: [f32; 3], normal: [f32; 3]) -> Self {
        let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        if len <= 1e-8 {
            return Self { nx: 0.0, ny: 1.0, nz: 0.0, d: -point[1] };
        }
        let inv = 1.0 / len;
        let (nx, ny, nz) = (normal[0] * inv, normal[1] * inv, normal[2] * inv);
        let d = -(nx * point[0] + ny * point[1] + nz * point[2]);
        Self { nx, ny, nz, d }
    }

    pub fn distance(self, point: [f32; 3]) -> f32 {
        self.nx * point[0] + self.ny * point[1] + self.nz * point[2] + self.d
    }

    /// Distance along `dir` from `origin` to the plane, or None when parallel.
    pub fn ray_hit(self, origin: [f32; 3], dir: [f32; 3]) -> Option<f32> {
        let denom = self.nx * dir[0] + self.ny * dir[1] + self.nz * dir[2];
        if denom.abs() <= 1e-8 {
            return None;
        }
        let t = -(self.nx * origin[0] + self.ny * origin[1] + self.nz * origin[2] + self.d) / denom;
        if t.is_finite() { Some(t) } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_to_horizontal_plane() {
        let p = Plane::from_point_normal([0.0, 2.0, 0.0], [0.0, 1.0, 0.0]);
        assert!((p.distance([0.0, 5.0, 0.0]) - 3.0).abs() < 1e-5);
        assert!((p.distance([0.0, 0.0, 0.0]) + 2.0).abs() < 1e-5);
    }

    #[test]
    fn ray_hits_ground() {
        let p = Plane::from_point_normal([0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        let t = p.ray_hit([0.0, 3.0, 0.0], [0.0, -1.0, 0.0]).unwrap();
        assert!((t - 3.0).abs() < 1e-5);
    }

    #[test]
    fn parallel_ray_misses() {
        let p = Plane::from_point_normal([0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        assert!(p.ray_hit([0.0, 1.0, 0.0], [1.0, 0.0, 0.0]).is_none());
    }
}