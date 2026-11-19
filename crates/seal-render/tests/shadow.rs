use seal_render::shadow::{LightFrame, ShadowMap};

fn scene() -> ShadowMap {
    // Light shining straight down (-y); a 2x2 slab floats at height 4.
    let frame = LightFrame::fit([0.0, -1.0, 0.0], [0.0, 0.0, 0.0], 10.0);
    let mut m = ShadowMap::new(128, frame);
    m.draw_box([-1.0, 4.0, -1.0], [1.0, 4.4, 1.0]);
    m
}

#[test]
fn a_point_under_a_slab_is_shadowed_and_open_ground_is_lit() {
    let m = scene();
    assert_eq!(m.lit([0.0, 0.0, 0.0], 0.005, 1), 0.0);
    assert_eq!(m.lit([6.0, 0.0, 0.0], 0.005, 1), 1.0);
    assert_eq!(m.lit([0.0, 4.5, 0.0], 0.005, 1), 1.0, "above the slab is lit");
    assert_eq!(m.lit([0.0, 4.4, 0.0], 0.02, 1), 1.0, "the slab's own top is not self-shadowed");
}

#[test]
fn the_shadow_edge_is_soft_with_pcf() {
    let m = scene();
    let edge = m.lit([1.0, 0.0, 0.0], 0.005, 2);
    assert!(edge > 0.0 && edge < 1.0, "edge was {edge}");
    assert!(m.lit([1.0, 0.0, 0.0], 0.005, 0) == 0.0 || m.lit([1.0, 0.0, 0.0], 0.005, 0) == 1.0);
}

#[test]
fn light_frames_are_orthonormal_and_deterministic() {
    let f = LightFrame::fit([1.0, -2.0, 0.5], [3.0, 0.0, 1.0], 8.0);
    let c = f.project([3.0, 0.0, 1.0]);
    assert_eq!(c, [0.5, 0.5, 0.5]);
    assert_eq!(f, LightFrame::fit([1.0, -2.0, 0.5], [3.0, 0.0, 1.0], 8.0));
    let d = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    assert!(
        d(f.right, f.up).abs() < 1e-6
            && d(f.dir, f.up).abs() < 1e-6
            && d(f.dir, f.right).abs() < 1e-6
    );
    // Straight-down light must not break the basis.
    let g = LightFrame::fit([0.0, -1.0, 0.0], [0.0; 3], 1.0);
    assert!(d(g.right, g.right) > 0.99);
}
