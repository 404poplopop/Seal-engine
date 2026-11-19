use seal_world::particles::{bursts_due, offset_inside_shape, sample_emit_offset};
use seal_world::particles::{
    collect_draws, EmitShape, Emitter, EmitterDef, ParticleDraw, RendererMode,
};

fn burst_def() -> EmitterDef {
    EmitterDef {
        rate: 0.0,
        max: 500,
        lifetime: 2.0,
        extras: seal_world::particles::ParticleExtras {
            bursts: vec![
                seal_world::particles::Burst { time: 0.0, count: 10 },
                seal_world::particles::Burst { time: 0.5, count: 5 },
            ],
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn bursts_fire_at_the_right_times() {
    let mut e = Emitter::new(burst_def());
    e.step(0.25, [0.0; 3]);
    assert_eq!(e.particles.len(), 10);
    e.step(0.3, [0.0; 3]);
    assert_eq!(e.particles.len(), 15);
    let due = bursts_due(&burst_def().extras, -1.0, 0.4);
    assert!(due.iter().any(|(_, c)| *c == 10));
}

#[test]
fn emit_shapes_keep_spawns_inside() {
    let shapes = [
        EmitShape::Circle(1.0),
        EmitShape::Ring(0.3, 0.8),
        EmitShape::Box([0.5, 0.4]),
        EmitShape::Edge([0.0, 0.0, 1.0, 0.0]),
    ];
    for shape in shapes {
        let def = EmitterDef {
            rate: 200.0,
            spread: 0.0,
            velocity: [0.0; 3],
            gravity: [0.0; 3],
            max: 200,
            seed: 4,
            extras: seal_world::particles::ParticleExtras {
                shape: shape.clone(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut em = Emitter::new(def);
        for _ in 0..200 {
            em.step(0.001, [0.0; 3]);
        }
        for p in &em.particles {
            if p.age > 0.02 {
                continue;
            }
            let off = [p.pos[0], p.pos[1], p.pos[2]];
            assert!(offset_inside_shape(&shape, off), "{shape:?} {off:?}");
        }
    }
}

#[test]
fn noise_is_deterministic_for_a_seed() {
    let mut a = Emitter::new(EmitterDef {
        rate: 40.0,
        spread: 0.0,
        seed: 99,
        extras: seal_world::particles::ParticleExtras {
            noise_strength: 0.4,
            noise_frequency: 2.0,
            ..Default::default()
        },
        ..Default::default()
    });
    let mut b = Emitter::new(a.def.clone());
    for _ in 0..30 {
        a.step(1.0 / 60.0, [0.0; 3]);
        b.step(1.0 / 60.0, [0.0; 3]);
    }
    assert_eq!(a.particles.len(), b.particles.len());
    for (pa, pb) in a.particles.iter().zip(&b.particles) {
        assert!((pa.pos[0] - pb.pos[0]).abs() < 1e-6);
        assert!((pa.pos[1] - pb.pos[1]).abs() < 1e-6);
    }
}

#[test]
fn renderer_modes_produce_expected_draw_items() {
    let stretched = EmitterDef {
        rate: 5.0,
        spread: 0.0,
        velocity: [3.0, 0.0, 0.0],
        extras: seal_world::particles::ParticleExtras {
            renderer: RendererMode::Stretched(0.5),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut e = Emitter::new(stretched);
    e.step(0.2, [0.0; 3]);
    let draws = collect_draws(&e, None);
    assert!(draws
        .iter()
        .any(|d| matches!(d, ParticleDraw::Stretched { stretch, .. } if *stretch > 0.2)));

    let flip = EmitterDef {
        rate: 0.0,
        spread: 0.0,
        extras: seal_world::particles::ParticleExtras {
            bursts: vec![seal_world::particles::Burst { time: 0.0, count: 1 }],
            renderer: RendererMode::Flipbook {
                path: "fx/spark.frames".into(),
                mode: seal_world::particles::FlipbookMode::Random,
            },
            ..Default::default()
        },
        ..Default::default()
    };
    let mut f = Emitter::new(flip);
    f.step(0.1, [0.0; 3]);
    assert!(collect_draws(&f, None).iter().any(|d| {
        matches!(d, ParticleDraw::Flipbook { picture, .. } if picture.contains("spark"))
    }));

    let trail = EmitterDef {
        rate: 2.0,
        lifetime: 1.0,
        extras: seal_world::particles::ParticleExtras {
            renderer: RendererMode::Trail { points: 6, width_end: 0.1 },
            ..Default::default()
        },
        ..Default::default()
    };
    let mut t = Emitter::new(trail);
    for _ in 0..20 {
        t.step(0.05, [0.0; 3]);
    }
    assert!(collect_draws(&t, None).iter().any(|d| matches!(d, ParticleDraw::Trail { .. })));
}

#[test]
fn old_fx_round_trips_unchanged() {
    let text = "rate 10\nlifetime 1\nvelocity 0 2 0\nspread 0\ngravity 0 0 0\nsize 0.2 0.05\ncolor 1 1 1 1  1 1 1 0\nmax 100\nseed 3\n";
    let def = EmitterDef::parse(text).unwrap();
    let round = EmitterDef::parse(&def.to_text()).unwrap();
    assert_eq!(def, round);
    assert!(def.extras.renderer == Default::default());
}

#[test]
fn damaged_burst_line_keeps_defaults() {
    let def = EmitterDef::parse("rate 1\nlifetime 1\nburst oops\n").unwrap();
    assert!(def.extras.damaged.contains(&"burst".to_string()));
    assert!(def.extras.bursts.is_empty());
}

#[test]
fn sample_offset_stays_inside_circle() {
    let shape = EmitShape::Circle(0.5);
    let mut seed = 8u32;
    let mut rand = || {
        seed = seed.wrapping_mul(16807);
        (seed % 1000) as f32 / 1000.0 * 2.0 - 1.0
    };
    for _ in 0..50 {
        let o = sample_emit_offset(&shape, &mut rand);
        assert!(offset_inside_shape(&shape, o));
    }
}
