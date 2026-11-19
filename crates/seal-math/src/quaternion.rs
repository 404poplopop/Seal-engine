//! Unit quaternions for rotation (x, y, z, w).

use crate::{cos, sin};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Quat {
    pub const IDENTITY: Self = Self { x: 0.0, y: 0.0, z: 0.0, w: 1.0 };

    pub fn normalize(self) -> Self {
        let len = (self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w).sqrt();
        if len <= 0.0 {
            return Self::IDENTITY;
        }
        let inv = 1.0 / len;
        Self {
            x: self.x * inv,
            y: self.y * inv,
            z: self.z * inv,
            w: self.w * inv,
        }
    }

    /// Euler angles in degrees: pitch (x), yaw (y), roll (z).
    pub fn from_euler_deg(pitch: f32, yaw: f32, roll: f32) -> Self {
        let (px, py, pz) = (
            pitch.to_radians() * 0.5,
            yaw.to_radians() * 0.5,
            roll.to_radians() * 0.5,
        );
        let (sp, cp) = (sin(px), cos(px));
        let (sy, cy) = (sin(py), cos(py));
        let (sr, cr) = (sin(pz), cos(pz));
        Self::normalize(Self {
            x: sp * cy * cr + cp * sy * sr,
            y: cp * sy * cr - sp * cy * sr,
            z: cp * cy * sr - sp * sy * cr,
            w: cp * cy * cr + sp * sy * sr,
        })
    }

    /// Axis must be non-zero; angle in degrees.
    pub fn from_axis_angle(axis: [f32; 3], angle_deg: f32) -> Self {
        let len = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
        if len <= 1e-8 {
            return Self::IDENTITY;
        }
        let half = angle_deg.to_radians() * 0.5;
        let (s, c) = (sin(half), cos(half));
        let inv = s / len;
        Self::normalize(Self {
            x: axis[0] * inv,
            y: axis[1] * inv,
            z: axis[2] * inv,
            w: c,
        })
    }

    pub fn mul(self, other: Self) -> Self {
        Self::normalize(Self {
            x: self.w * other.x + self.x * other.w + self.y * other.z - self.z * other.y,
            y: self.w * other.y - self.x * other.z + self.y * other.w + self.z * other.x,
            z: self.w * other.z + self.x * other.y - self.y * other.x + self.z * other.w,
            w: self.w * other.w - self.x * other.x - self.y * other.y - self.z * other.z,
        })
    }

    pub fn slerp(self, other: Self, t: f32) -> Self {
        let mut b = other;
        let mut dot = self.x * other.x + self.y * other.y + self.z * other.z + self.w * other.w;
        if dot < 0.0 {
            dot = -dot;
            b = Self { x: -other.x, y: -other.y, z: -other.z, w: -other.w };
        }
        if dot > 0.9995 {
            return Self::normalize(Self {
                x: self.x + t * (b.x - self.x),
                y: self.y + t * (b.y - self.y),
                z: self.z + t * (b.z - self.z),
                w: self.w + t * (b.w - self.w),
            });
        }
        let theta = dot.clamp(-1.0, 1.0).acos();
        let sin_theta = sin(theta);
        if sin_theta.abs() <= 1e-8 {
            return self;
        }
        let wa = sin((1.0 - t) * theta) / sin_theta;
        let wb = sin(t * theta) / sin_theta;
        Self::normalize(Self {
            x: self.x * wa + b.x * wb,
            y: self.y * wa + b.y * wb,
            z: self.z * wa + b.z * wb,
            w: self.w * wa + b.w * wb,
        })
    }

    pub fn rotate_vec(self, v: [f32; 3]) -> [f32; 3] {
        let q = self.normalize();
        let (qx, qy, qz, qw) = (q.x, q.y, q.z, q.w);
        let tx = 2.0 * (qy * v[2] - qz * v[1]);
        let ty = 2.0 * (qz * v[0] - qx * v[2]);
        let tz = 2.0 * (qx * v[1] - qy * v[0]);
        [
            v[0] + qw * tx + qy * tz - qz * ty,
            v[1] + qw * ty + qz * tx - qx * tz,
            v[2] + qw * tz + qx * ty - qy * tx,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_rotation_is_unchanged() {
        let v = [1.0, 0.0, 0.0];
        let out = Quat::IDENTITY.rotate_vec(v);
        assert!((out[0] - 1.0).abs() < 1e-5);
        assert!(out[1].abs() < 1e-5 && out[2].abs() < 1e-5);
    }

    #[test]
    fn ninety_degree_yaw_rotates_forward_to_right() {
        let q = Quat::from_euler_deg(0.0, 90.0, 0.0);
        let out = q.rotate_vec([0.0, 0.0, 1.0]);
        assert!(out[0] > 0.9 && out[2].abs() < 0.2);
    }

    #[test]
    fn slerp_halfway_between_opposites_faces_short_arc() {
        let a = Quat::IDENTITY;
        let b = Quat::from_axis_angle([0.0, 1.0, 0.0], 180.0);
        let mid = a.slerp(b, 0.5);
        let v = mid.rotate_vec([1.0, 0.0, 0.0]);
        assert!(v[0].abs() < 0.1 && v[2].abs() > 0.9);
    }

    #[test]
    fn mul_composes_rotations() {
        let a = Quat::from_axis_angle([0.0, 1.0, 0.0], 90.0);
        let b = Quat::from_axis_angle([1.0, 0.0, 0.0], 90.0);
        let c = a.mul(b);
        let v = c.rotate_vec([0.0, 0.0, 1.0]);
        assert!(v[1].abs() > 0.5);
    }
}