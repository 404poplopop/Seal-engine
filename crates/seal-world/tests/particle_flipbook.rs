use std::fs;
use std::path::PathBuf;

use seal_world::frames::SpriteFrames;
use seal_world::particles::{
    collect_draws, frame_picture, Emitter, EmitterDef, FlipbookMode, ParticleDraw, RendererMode,
};

fn temp_project() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("seal-particle-fb-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let fx = dir.join("fx");
    fs::create_dir_all(&fx).unwrap();
    let anim = SpriteFrames::parse(
        "animation spark fps=4 loop=yes\nframe tile.png 0 0 0.5 0.5\nframe tile.png 0.5 0 1.0 0.5\n",
    )
    .unwrap();
    fs::write(fx.join("spark.frames"), anim.to_text()).unwrap();
    fs::write(fx.join("tile.png"), b"x").unwrap();
    dir
}

#[test]
fn flipbook_over_life_picks_frame_from_sprite_animation() {
    let root = temp_project();
    let def = EmitterDef {
        rate: 0.0,
        lifetime: 1.0,
        size: (0.2, 0.2),
        color: ([1.0; 4], [1.0; 4]),
        extras: seal_world::particles::ParticleExtras {
            bursts: vec![seal_world::particles::Burst { time: 0.0, count: 1 }],
            renderer: RendererMode::Flipbook {
                path: "fx/spark.frames".into(),
                mode: FlipbookMode::OverLife,
            },
            ..Default::default()
        },
        ..Default::default()
    };
    let mut em = Emitter::new(def);
    em.step(0.0, [0.0; 3]);
    em.particles[0].age = 0.99;
    let draws = collect_draws(&em, Some(&root));
    let draw = draws
        .iter()
        .find_map(|d| match d {
            ParticleDraw::Flipbook { picture, frame_index, uv, .. } => {
                Some((picture.clone(), *frame_index, *uv))
            }
            _ => None,
        })
        .expect("flipbook draw");
    assert_eq!(draw.1, 1, "second frame at 75% life");
    assert!(draw.0.ends_with("tile.png"));
    assert_eq!(draw.2, [0.5, 0.0, 1.0, 0.5]);
}

#[test]
fn sheet_flipbook_uses_horizontal_strip_uv() {
    // A sheet is a row of square cells: 64 x 4 pixels holds 16 frames.
    let root = std::env::temp_dir().join(format!("seal-flip-strip-{}", std::process::id()));
    std::fs::create_dir_all(root.join("fx")).unwrap();
    let mut head = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    head.extend_from_slice(&64u32.to_be_bytes());
    head.extend_from_slice(&4u32.to_be_bytes());
    std::fs::write(root.join("fx/sheet.png"), head).unwrap();
    let (picture, uv) = frame_picture("fx/sheet.png", 3, Some(&root));
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(picture, "fx/sheet.png");
    assert!((uv[0] - 0.1875).abs() < 1e-5);
    assert!((uv[2] - 0.25).abs() < 1e-5);
}
