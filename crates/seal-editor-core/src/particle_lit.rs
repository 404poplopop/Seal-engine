//! 2D lighting on particle colours, matching flat sprite shading.

use crate::light2d::{light_at, Blocker, Light};

/// Multiplies particle colour by 2D lights at the given scene point.
pub fn lit_rgba(
    at: [f32; 2],
    rgba: [f32; 4],
    lit: bool,
    lights: &[Light],
    blockers: &[Blocker],
    ambient: [f32; 3],
) -> [f32; 4] {
    if !lit || lights.is_empty() {
        return rgba;
    }
    let l = light_at(at, None, lights, blockers, ambient);
    [
        (rgba[0] * l[0]).clamp(0.0, 1.0),
        (rgba[1] * l[1]).clamp(0.0, 1.0),
        (rgba[2] * l[2]).clamp(0.0, 1.0),
        rgba[3],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::light2d::{Kind, Light};

    #[test]
    fn lit_particle_matches_sprite_light_at_point() {
        let lamp = Light {
            kind: Kind::Point,
            pos: [0.0, 0.0],
            color: [1.0, 0.5, 0.25],
            intensity: 1.0,
            range: 10.0,
            angle: 0.0,
            cone: 90.0,
            shadows: false,
            blend: crate::light2d::Blend::Additive,
            falloff: 0.0,
            points: Vec::new(),
        };
        let lights = [lamp];
        let base = [1.0, 1.0, 1.0, 1.0];
        let particle = lit_rgba([0.5, 0.0], base, true, &lights, &[], [0.0; 3]);
        let at = light_at([0.5, 0.0], None, &lights, &[], [0.0; 3]);
        assert!((particle[0] - at[0]).abs() < 1e-5);
        assert!((particle[1] - at[1]).abs() < 1e-5);
        assert!((particle[2] - at[2]).abs() < 1e-5);
    }
}
