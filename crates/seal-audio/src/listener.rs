//! Pan and distance attenuation for sounds heard through an audio listener.

use seal_math::sin_cos;

/// Where the listener sits in the world and which way it faces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListenerPose {
    pub position: [f32; 3],
    /// Unit vector; used for 3D pan on the horizontal plane.
    pub forward: [f32; 3],
}

impl Default for ListenerPose {
    fn default() -> Self {
        // Facing +z like the Front view, so +x is to the right.
        Self { position: [0.0; 3], forward: [0.0, 0.0, 1.0] }
    }
}

/// How quickly level falls off with distance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListenerTuning {
    pub max_distance: f32,
    /// 1 is linear; larger values steepen the rolloff.
    pub rolloff: f32,
}

impl Default for ListenerTuning {
    fn default() -> Self {
        Self { max_distance: 50.0, rolloff: 1.0 }
    }
}

/// Stereo pan (-1..1) and gain multiplier for a source relative to a listener.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpatialMix {
    pub pan: f32,
    pub gain: f32,
}

fn horizontal_unit(mut v: [f32; 3]) -> [f32; 3] {
    v[1] = 0.0;
    let len = (v[0] * v[0] + v[2] * v[2]).sqrt();
    if len < 1.0e-6 {
        return [0.0, 0.0, 1.0];
    }
    [v[0] / len, 0.0, v[2] / len]
}

/// Pan and attenuate a sound at `source` for play through `listener`.
pub fn spatial_mix(
    listener: &ListenerPose,
    tuning: &ListenerTuning,
    source: [f32; 3],
    base_gain: f32,
) -> SpatialMix {
    let dx = source[0] - listener.position[0];
    let dy = source[1] - listener.position[1];
    let dz = source[2] - listener.position[2];
    let dist_3d = (dx * dx + dy * dy + dz * dz).sqrt();
    let dist_2d = (dx * dx + dz * dz).sqrt();
    let max_d = tuning.max_distance.max(0.001);
    let atten_3d = attenuate(dist_3d, max_d, tuning.rolloff);
    let atten_2d = attenuate(dist_2d, max_d, tuning.rolloff);
    let gain = base_gain * atten_3d.min(atten_2d).clamp(0.0, 1.0);
    let pan_2d = if dist_2d < 1.0e-5 {
        0.0
    } else {
        // The view's right: +x when facing +z (the editor's views are drawn this way).
        let right = [listener.forward[2], -listener.forward[0]];
        let along = (dx * listener.forward[0] + dz * listener.forward[2]) / dist_2d;
        let side = (dx * right[0] + dz * right[1]) / dist_2d;
        side.clamp(-1.0, 1.0) * (1.0 - along.abs() * 0.15)
    };
    let pan_3d = if dist_3d < 1.0e-5 {
        0.0
    } else {
        let fwd = horizontal_unit(listener.forward);
        let to = [dx / dist_3d, 0.0, dz / dist_3d];
        let dot = fwd[0] * to[0] + fwd[2] * to[2];
        let cross = fwd[2] * to[0] - fwd[0] * to[2];
        let angle = cross.atan2(dot);
        let (sin, cos) = sin_cos(angle);
        (sin / (cos.abs() + 1.0)).clamp(-1.0, 1.0)
    };
    let pan = if dist_2d > 1.0e-4 && dist_3d > 1.0e-4 {
        (pan_2d + pan_3d) * 0.5
    } else if dist_2d > 1.0e-4 {
        pan_2d
    } else {
        pan_3d
    };
    SpatialMix { pan: pan.clamp(-1.0, 1.0), gain: gain.max(0.0) }
}

fn attenuate(distance: f32, max_distance: f32, rolloff: f32) -> f32 {
    if distance >= max_distance {
        return 0.0;
    }
    if distance <= 0.0 {
        return 1.0;
    }
    let t = (distance / max_distance).clamp(0.0, 1.0);
    let curve = t.powf(rolloff.max(0.01));
    1.0 - curve
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centre_source_is_full_level_and_centre_pan() {
        let mix = spatial_mix(&ListenerPose::default(), &ListenerTuning::default(), [0.0; 3], 1.0);
        assert!((mix.gain - 1.0).abs() < 0.001);
        assert!(mix.pan.abs() < 0.001);
    }

    #[test]
    fn pan_and_gain_follow_distance_and_side() {
        let listener = ListenerPose { position: [0.0; 3], forward: [0.0, 0.0, 1.0] };
        let tuning = ListenerTuning { max_distance: 10.0, rolloff: 1.0 };
        let left = spatial_mix(&listener, &tuning, [-5.0, 0.0, 0.0], 1.0);
        let right = spatial_mix(&listener, &tuning, [5.0, 0.0, 0.0], 1.0);
        assert!(left.pan < -0.2, "left pan {left:?}");
        assert!(right.pan > 0.2, "right pan {right:?}");
        let far = spatial_mix(&listener, &tuning, [0.0, 0.0, 9.5], 1.0);
        assert!(far.gain < left.gain);
        assert!(far.gain > 0.0);
        let beyond = spatial_mix(&listener, &tuning, [0.0, 0.0, 20.0], 1.0);
        assert!(beyond.gain.abs() < 0.001);
    }
}