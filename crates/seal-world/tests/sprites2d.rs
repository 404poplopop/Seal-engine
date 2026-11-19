use seal_world::{PlayMode, World};

fn sprite_scene() -> World {
    let mut world = World::demo();
    let e = &mut world.entities[2];
    e.sprite = "sprites/walk.png".into();
    e.layer = 3;
    e.frames = 4;
    e.fps = 10.0;
    world
}

#[test]
fn sprite_layer_and_sheet_settings_survive_save_and_reopen() {
    let world = sprite_scene();
    let text = world.to_text();
    let back = World::from_text(&text).unwrap();
    let e = &back.entities[2];
    assert_eq!((e.layer, e.frames, e.fps), (3, 4, 10.0));
    assert_eq!(back.to_text(), text);
    // Old scenes without these fields open as a still picture on layer 0.
    let old = text
        .replace("        layer = 3\n", "")
        .replace("        frames = 4\n", "")
        .replace("        fps = 10\n", "");
    assert_ne!(old, text);
    let e = &World::from_text(&old).unwrap().entities[2];
    assert_eq!((e.layer, e.frames, e.fps), (0, 1, 8.0));
}

#[test]
fn sheet_frames_advance_with_play_time_and_wrap() {
    let world = sprite_scene();
    let e = &world.entities[2];
    assert_eq!(e.sprite_frame(0.0), 0);
    assert_eq!(e.sprite_frame(0.15), 1);
    assert_eq!(e.sprite_frame(0.35), 3);
    assert_eq!(e.sprite_frame(0.45), 0);
    assert_eq!(e.sprite_uv(0.15), [0.25, 0.5]);
    let mut still = e.clone();
    still.frames = 1;
    assert_eq!(still.sprite_uv(5.0), [0.0, 1.0]);
}

#[test]
fn play_keeps_a_clock_that_resets_on_stop() {
    let mut world = sprite_scene();
    assert_eq!(world.play_time, 0.0);
    world.play();
    for _ in 0..10 {
        world.step(0.05);
    }
    assert_eq!(world.mode, PlayMode::Playing);
    assert!((world.play_time - 0.5).abs() < 1e-4, "{}", world.play_time);
    world.pause();
    world.step(0.05);
    assert!((world.play_time - 0.5).abs() < 1e-4, "paused play does not advance frames");
    world.stop();
    assert_eq!(world.play_time, 0.0);
}

#[test]
fn scripted_play_centres_the_view_on_the_camera_object() {
    let mut world = World::demo();
    assert_eq!(world.play_view_target(), None, "editing never steers the view");
    world.play();
    assert!(world.play_player().is_none());
    let player = world.entities.iter().find(|e| e.tag == "player").unwrap().translation;
    assert_eq!(world.play_view_target(), Some(player), "no camera object: follow the player");
    let ix = world.entities.iter().position(|e| e.tag == "prop").unwrap();
    world.entities[ix].tag = "camera".into();
    world.entities[ix].translation = [7.0, 2.0, 0.0];
    assert_eq!(world.play_view_target(), Some([7.0, 2.0, 0.0]), "a camera object wins");
    world.stop();
    assert_eq!(world.play_view_target(), None);
}
