//! Soft particle fade against opaque depth in 2D views.

/// Opaque draws sort front-to-back by `depth` (view axis). A particle at `particle_depth`
/// fades when it sits just in front of an occluder within `fade_distance` world units.
/// Alpha multiplies by `((particle_depth - occluder_depth) / fade_distance).clamp(0, 1)`.
pub fn soft_fade_factor(particle_depth: f32, occluder_depths: &[f32], fade_distance: f32) -> f32 {
    if fade_distance <= 1e-6 {
        return 1.0;
    }
    let mut min_gap = f32::INFINITY;
    for &d in occluder_depths {
        if d <= particle_depth {
            continue;
        }
        min_gap = min_gap.min(d - particle_depth);
    }
    if !min_gap.is_finite() {
        return 1.0;
    }
    (min_gap / fade_distance).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_at_distances() {
        assert_eq!(soft_fade_factor(0.0, &[], 0.2), 1.0);
        assert!((soft_fade_factor(0.0, &[0.1], 0.2) - 0.5).abs() < 1e-5);
        assert!((soft_fade_factor(0.0, &[0.2], 0.2) - 1.0).abs() < 1e-5);
        assert_eq!(soft_fade_factor(0.5, &[0.4], 0.2), 1.0);
    }
}
