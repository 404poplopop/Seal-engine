use seal_render::probes::*;

#[test]
fn ambient_light_gives_the_same_value_in_every_direction() {
    let g =
        ProbeGrid::bake([0.0; 3], 1.0, [2, 2, 2], &[Light::Ambient { color: [0.5, 0.25, 1.0] }]);
    for n in [[0.0, 1.0, 0.0], [0.0, -1.0, 0.0], [1.0, 0.0, 0.0]] {
        let v = g.sample([0.5, 0.5, 0.5], n);
        assert!(
            (v[0] - 0.5).abs() < 1e-3 && (v[1] - 0.25).abs() < 1e-3 && (v[2] - 1.0).abs() < 1e-3,
            "{v:?}"
        );
    }
}

#[test]
fn a_sky_light_brightens_up_facing_surfaces_and_leaves_down_facing_ones_darker() {
    let sky = Light::Directional { toward: [0.0, 1.0, 0.0], color: [1.0, 1.0, 1.0] };
    let g = ProbeGrid::bake([0.0; 3], 1.0, [2, 2, 2], &[sky]);
    let up = g.sample([0.5, 0.5, 0.5], [0.0, 1.0, 0.0]);
    let down = g.sample([0.5, 0.5, 0.5], [0.0, -1.0, 0.0]);
    let side = g.sample([0.5, 0.5, 0.5], [1.0, 0.0, 0.0]);
    assert!(up[0] > side[0] && side[0] > down[0], "{up:?} {side:?} {down:?}");
    assert_eq!(down[0], 0.0);
}

#[test]
fn point_lights_fade_with_distance_and_stop_at_their_range() {
    let lamp = Light::Point { pos: [0.0, 2.0, 0.0], color: [1.0, 0.0, 0.0], range: 6.0 };
    let near = irradiance(&bake_point([0.0, 1.0, 0.0], &[lamp]), [0.0, 1.0, 0.0]);
    let far = irradiance(&bake_point([0.0, -3.0, 0.0], &[lamp]), [0.0, 1.0, 0.0]);
    let out = irradiance(&bake_point([0.0, -5.0, 0.0], &[lamp]), [0.0, 1.0, 0.0]);
    assert!(near[0] > far[0] && far[0] > 0.0);
    assert_eq!(out, [0.0, 0.0, 0.0]);
    assert_eq!(near[1], 0.0);
}

#[test]
fn sampling_blends_between_probes_and_clamps_outside_the_grid() {
    let lamp = Light::Point { pos: [0.0, 1.0, 0.0], color: [1.0; 3], range: 4.0 };
    let g = ProbeGrid::bake([0.0; 3], 2.0, [3, 1, 1], &[lamp]);
    let a = g.sample([0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    let b = g.sample([2.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    let mid = g.sample([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    assert!(a[0] > b[0]);
    assert!(mid[0] < a[0] && mid[0] > b[0], "{a:?} {mid:?} {b:?}");
    assert_eq!(g.sample([-9.0, 0.0, 0.0], [0.0, 1.0, 0.0]), a);
    assert_eq!(
        g.sample([40.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        g.sample([4.0, 0.0, 0.0], [0.0, 1.0, 0.0])
    );
}
