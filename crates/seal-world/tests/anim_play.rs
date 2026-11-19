use seal_scene::anim::Anim;
use seal_world::{PlayMode, World};

#[test]
fn an_animation_moves_an_object_in_play_and_stop_restores_the_scene() {
    let mut w = World::demo();
    let id = w.entities[2].id.clone();
    let before = w.entities[2].translation;
    w.play();
    assert_eq!(w.mode, PlayMode::Playing);
    w.play_anim(
        &id,
        Anim::parse("length 1\ntrack position.x linear\nkey 0 0\nkey 1 10\n").unwrap(),
    );
    for _ in 0..30 {
        w.step(1.0 / 60.0);
    }
    let x = w.entities[2].translation[0];
    assert!((x - 5.0).abs() < 0.2, "x was {x}");
    for _ in 0..60 {
        w.step(1.0 / 60.0);
    }
    assert_eq!(w.entities[2].translation[0], 10.0);
    assert!(w.anims.is_empty());
    w.stop();
    assert_eq!(w.entities[2].translation, before);
}

#[test]
fn a_sprite_frame_track_picks_the_sheet_frame_and_releases_it_at_the_end() {
    let mut w = World::demo();
    let id = w.entities[2].id.clone();
    w.entities[2].frames = 4;
    w.entities[2].fps = 8.0;
    w.play();
    w.play_anim(
        &id,
        Anim::parse("length 1\ntrack sprite_frame step\nkey 0 2\nkey 0.5 3\n").unwrap(),
    );
    w.step(1.0 / 60.0);
    assert_eq!(w.entities[2].sprite_frame(0.0), 2);
    for _ in 0..40 {
        w.step(1.0 / 60.0);
    }
    assert_eq!(w.entities[2].sprite_frame(0.0), 3);
    for _ in 0..40 {
        w.step(1.0 / 60.0);
    }
    assert_eq!(w.entities[2].frame_override, None);
}
