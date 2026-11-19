use seal_render::gi::{ray_box, sphere_dirs, GiScene, Occluder};
use seal_render::probes::{irradiance, Light};

const UP: [f32; 3] = [0.0, 1.0, 0.0];

fn sun() -> Light {
    Light::Directional { toward: UP, color: [1.0, 1.0, 1.0] }
}

fn scene(occluders: Vec<Occluder>, sky: [f32; 3], direct: bool) -> GiScene {
    GiScene { occluders, lights: vec![sun()], sky, direct }
}

fn grey(min: [f32; 3], max: [f32; 3]) -> Occluder {
    Occluder { min, max, albedo: [0.5, 0.5, 0.5] }
}

#[test]
fn rays_hit_the_near_face_with_its_normal() {
    let b = grey([-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]);
    let (t, n) = ray_box([-5.0, 0.0, 0.0], [1.0, 0.0, 0.0], &b).unwrap();
    assert!((t - 4.0).abs() < 1e-5);
    assert_eq!(n, [-1.0, 0.0, 0.0]);
    assert!(ray_box([-5.0, 3.0, 0.0], [1.0, 0.0, 0.0], &b).is_none());
    assert!(ray_box([5.0, 0.0, 0.0], [1.0, 0.0, 0.0], &b).is_none(), "box is behind the ray");
}

#[test]
fn sample_directions_cover_the_sphere_evenly() {
    let dirs = sphere_dirs(256);
    let mut sum = [0.0f32; 3];
    for d in &dirs {
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        assert!((len - 1.0).abs() < 1e-4);
        for k in 0..3 {
            sum[k] += d[k];
        }
    }
    for (k, s) in sum.iter().enumerate() {
        assert!(s.abs() / 256.0 < 0.02, "directions lean along axis {k}");
    }
}

#[test]
fn an_open_probe_sees_the_sky_at_its_own_brightness() {
    let s = GiScene { occluders: vec![], lights: vec![], sky: [0.3, 0.4, 0.5], direct: true };
    let sh = s.bake_point([0.0; 3], 512);
    for n in [UP, [1.0, 0.0, 0.0], [0.0, -1.0, 0.0]] {
        let e = irradiance(&sh, n);
        for (got, want) in e.iter().zip([0.3, 0.4, 0.5]) {
            assert!((got - want).abs() < 0.02, "sky {got} vs {want} facing {n:?}");
        }
    }
}

#[test]
fn a_roof_shadows_the_sun_and_the_sky() {
    let roof = grey([-50.0, 2.0, -50.0], [50.0, 3.0, 50.0]);
    let open = scene(vec![], [0.2; 3], true);
    let covered = scene(vec![roof], [0.2; 3], true);
    let lit = irradiance(&open.bake_point([0.0; 3], 256), UP);
    let dark = irradiance(&covered.bake_point([0.0; 3], 256), UP);
    assert!(dark[0] < lit[0] * 0.3, "under a roof {dark:?} vs open {lit:?}");
}

#[test]
fn a_sunlit_red_wall_bounces_red_light_onto_its_neighbour() {
    // A red wall on the probe's +x side, lit by a sun shining at it from the -x side.
    let wall = Occluder { min: [2.0, -5.0, -5.0], max: [3.0, 5.0, 5.0], albedo: [0.9, 0.1, 0.1] };
    let s = GiScene {
        occluders: vec![wall],
        lights: vec![Light::Directional { toward: [-1.0, 0.0, 0.0], color: [1.0, 1.0, 1.0] }],
        sky: [0.0; 3],
        direct: false,
    };
    let e = irradiance(&s.bake_point([0.0; 3], 512), [1.0, 0.0, 0.0]);
    assert!(e[0] > 0.05, "red bounce {e:?}");
    assert!(e[0] > e[2] * 4.0, "bounce takes the wall's colour {e:?}");
    // Facing away from the wall gets almost none of it.
    let back = irradiance(&s.bake_point([0.0; 3], 512), [-1.0, 0.0, 0.0]);
    assert!(back[0] < e[0] * 0.2, "{back:?}");
}

#[test]
fn a_shadowed_wall_bounces_nothing() {
    let wall = Occluder { min: [2.0, -5.0, -5.0], max: [3.0, 5.0, 5.0], albedo: [0.9, 0.1, 0.1] };
    let blocker = grey([-1.0, -5.0, -5.0], [-0.5, 5.0, 5.0]);
    // Sun from -x: the blocker sits between the sun and everything else... but the wall's
    // lit face points -x, toward the probe, and the blocker shades it.
    let s = GiScene {
        occluders: vec![wall, blocker],
        lights: vec![Light::Directional { toward: [-1.0, 0.0, 0.0], color: [1.0, 1.0, 1.0] }],
        sky: [0.0; 3],
        direct: false,
    };
    let e = irradiance(&s.bake_point([0.0; 3], 256), [1.0, 0.0, 0.0]);
    assert!(e[0] < 0.01, "shadowed wall still glows {e:?}");
}

#[test]
fn the_grid_bake_is_the_same_every_run_and_matches_single_points() {
    let s = scene(vec![grey([-2.0, -1.0, -2.0], [2.0, 0.0, 2.0])], [0.1; 3], false);
    let a = s.bake_grid([-3.0, 0.5, -3.0], 2.0, [4, 2, 4], 64);
    let b = s.bake_grid([-3.0, 0.5, -3.0], 2.0, [4, 2, 4], 64);
    assert_eq!(a.probes, b.probes);
    assert_eq!(a.probes.len(), 32);
    // Index 5 is x 1, y 1, z 0.
    assert_eq!(a.probes[5], s.bake_point([-1.0, 2.5, -3.0], 64));
    // Probes right above the sunlit floor get grey bounce light from below.
    let below = a.sample([0.0, 0.5, 0.0], [0.0, -1.0, 0.0]);
    assert!(below[0] > 0.05, "{below:?}");
}
