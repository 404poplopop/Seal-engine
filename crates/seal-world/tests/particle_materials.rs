use seal_world::material2d::{self, shade};
use seal_world::material2d::{particle_anim_uv, Material2d, Picture};
use seal_world::particles::{
    collect_draws, collect_draws_depth, EmitShape, Emitter, EmitterDef, ParticleDraw, RendererMode,
    SphereVolume,
};
use seal_world::particles::{offset_inside_shape, sample_emit_offset, soft_fade_factor};

#[test]
fn sphere_emit_volume_and_surface() {
    let vol = EmitShape::Sphere(0.4, SphereVolume::Volume);
    let surf = EmitShape::Sphere(0.4, SphereVolume::Surface);
    let mut seed = 11u32;
    let mut rand = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 16) as f32 / 65535.0
    };
    for shape in [vol.clone(), surf.clone()] {
        for _ in 0..40 {
            let o = sample_emit_offset(&shape, || rand());
            assert!(offset_inside_shape(&shape, o), "{shape:?} {o:?}");
        }
    }
}

#[test]
fn mesh_emit_on_cube_surface() {
    let shape = EmitShape::Mesh {
        path: "cube".into(),
        mode: seal_world::particles::MeshSampleMode::Surface,
    };
    let mut seed = 4u32;
    let mut rand = || {
        seed = seed.wrapping_mul(22695477).wrapping_add(1);
        (seed % 10000) as f32 / 10000.0
    };
    for _ in 0..30 {
        let o = sample_emit_offset(&shape, || rand());
        assert!(offset_inside_shape(&shape, o));
    }
}

#[test]
fn mesh_renderer_draws_per_particle() {
    let def = EmitterDef {
        rate: 0.0,
        extras: seal_world::particles::ParticleExtras {
            bursts: vec![seal_world::particles::Burst { time: 0.0, count: 3 }],
            renderer: RendererMode::Mesh { path: "sphere".into() },
            ..Default::default()
        },
        ..Default::default()
    };
    let mut em = Emitter::new(def);
    em.step(0.05, [0.0; 3]);
    let draws = collect_draws(&em, None);
    assert_eq!(draws.len(), 3);
    assert!(draws.iter().all(|d| matches!(d, ParticleDraw::Mesh { mesh, .. } if mesh == "sphere")));
}

#[test]
fn soft_fade_and_billboard_material_round_trip() {
    let text = "\
rate 5
lifetime 1
renderer billboard
shape sphere 0.3 volume
soft_fade 0.15
billboard_material fx/sheet.png 4 2 loop
";
    let def = EmitterDef::parse(text).unwrap();
    let round = EmitterDef::parse(&def.to_text()).unwrap();
    assert_eq!(def.extras.soft_fade, round.extras.soft_fade);
    assert_eq!(def.extras.billboard_material, round.extras.billboard_material);
}

#[test]
fn soft_fade_on_draw_alpha() {
    let def = EmitterDef {
        rate: 0.0,
        extras: seal_world::particles::ParticleExtras {
            bursts: vec![seal_world::particles::Burst { time: 0.0, count: 1 }],
            soft_fade: 0.2,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut em = Emitter::new(def);
    em.step(0.01, [0.0; 3]);
    let full = collect_draws_depth(&em, None, &[]);
    let fade = collect_draws_depth(&em, None, &[0.1]);
    let a_full = match &full[0] {
        ParticleDraw::Billboard { color, .. } => color.rgba[3],
        _ => panic!(),
    };
    let a_soft = match &fade[0] {
        ParticleDraw::Billboard { color, .. } => color.rgba[3],
        _ => panic!(),
    };
    assert!(a_soft < a_full);
    assert!((soft_fade_factor(0.0, &[0.1], 0.2) - 0.5).abs() < 1e-4);
}

#[test]
fn billboard_material_frame_over_life() {
    let def = EmitterDef {
        rate: 0.0,
        lifetime: 2.0,
        extras: seal_world::particles::ParticleExtras {
            bursts: vec![seal_world::particles::Burst { time: 0.0, count: 1 }],
            billboard_material: Some(seal_world::particles::BillboardAnimMaterial {
                sheet: "fx/a.png".into(),
                cols: 4,
                rows: 1,
                r#loop: true,
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut em = Emitter::new(def);
    em.step(0.01, [0.0; 3]);
    em.particles[0].age = 0.0;
    let young = collect_draws(&em, None);
    em.particles[0].age = 1.5;
    let old = collect_draws(&em, None);
    let f0 = match &young[0] {
        ParticleDraw::Flipbook { frame_index, .. } => *frame_index,
        _ => panic!(),
    };
    let f1 = match &old[0] {
        ParticleDraw::Flipbook { frame_index, .. } => *frame_index,
        _ => panic!(),
    };
    assert!(f1 > f0);
}

#[test]
fn old_fx_without_new_lines_unchanged() {
    let text = "rate 10\nlifetime 1\nvelocity 0 2 0\nspread 0\n";
    let def = EmitterDef::parse(text).unwrap();
    assert_eq!(def.extras.soft_fade, 0.0);
    assert!(def.extras.billboard_material.is_none());
    let round = def.to_text();
    assert!(!round.contains("soft_fade"));
    assert!(!round.contains("billboard_material"));
}

#[test]
fn particle_anim_material_frame_index() {
    let m = Material2d {
        effect: "particle_anim".into(),
        shader: "s.png".into(),
        sheet_cols: 4,
        sheet_rows: 2,
        sheet_loop: true,
        speed: 0.0,
        ..Material2d::default()
    };
    let (_, uv0) = particle_anim_uv(&m, 0.0, 0.0);
    let (idx1, _) = particle_anim_uv(&m, 0.6, 0.0);
    assert_eq!(uv0[0], 0.0);
    assert!(idx1 >= 4);
}

#[test]
fn particle_anim_material_shade_steps_pixels() {
    let mut px = vec![0u8; 8 * 8 * 4];
    for y in 0..8 {
        for x in 0..8 {
            let i = (y * 8 + x) * 4;
            px[i] = if x < 4 { 32 } else { 200 };
            px[i + 3] = 255;
        }
    }
    let pic = Picture { width: 8, height: 8, pixels: &px };
    let m = Material2d {
        effect: "particle_anim".into(),
        shader: "s.png".into(),
        sheet_cols: 2,
        sheet_rows: 1,
        speed: 1.0,
        ..Material2d::default()
    };
    let a = shade(&pic, &m, None, 0.0);
    let b = shade(&pic, &m, None, 0.75);
    assert_ne!(a, b);
    assert!(material2d::animated(&m));
}
