#![allow(clippy::needless_range_loop)]

use seal_input::*;

fn map() -> InputMap {
    InputMap::parse(
        "# platformer\n\
         action jump = key.space, pad.south\n\
         axis move_x = key.a key.d, pad.left_x deadzone 0.2\n\
         axis look_x = mouse.dx scale 0.5\n",
    )
    .unwrap()
}

#[test]
fn actions_report_press_hold_and_release() {
    let map = map();
    let mut state = ActionState::default();
    let mut raw = RawInput::default();
    raw.press(Button::key("space"));
    state.update(&map, &raw);
    assert_eq!(
        state.get("jump"),
        ActionValue {
            pressed: true,
            just_pressed: true,
            just_released: false,
            held_frames: 1,
            held_seconds: 0.0,
        }
    );
    state.update(&map, &raw);
    assert!(state.pressed("jump") && !state.just_pressed("jump"));
    assert_eq!(state.get("jump").held_frames, 2);
    raw.release(&Button::key("space"));
    state.update(&map, &raw);
    assert!(state.just_released("jump") && !state.pressed("jump"));
    // Either binding works.
    raw.press(Button::parse("pad.south").unwrap());
    state.update(&map, &raw);
    assert!(state.just_pressed("jump"));
}

#[test]
fn axes_use_buttons_or_sticks_with_deadzones() {
    let map = map();
    let mut state = ActionState::default();
    let mut raw = RawInput::default();
    raw.press(Button::key("d"));
    state.update(&map, &raw);
    assert_eq!(state.axis("move_x"), 1.0);
    raw.press(Button::key("a"));
    state.update(&map, &raw);
    assert_eq!(state.axis("move_x"), 0.0, "both keys cancel out");
    raw.release(&Button::key("a"));
    raw.release(&Button::key("d"));
    let stick = Axis::parse("pad.left_x").unwrap();
    raw.set_axis(stick.clone(), 0.15);
    state.update(&map, &raw);
    assert_eq!(state.axis("move_x"), 0.0, "inside the deadzone");
    raw.set_axis(stick.clone(), -1.0);
    state.update(&map, &raw);
    assert_eq!(state.axis("move_x"), -1.0, "full tilt still reaches 1");
    raw.set_axis(stick, 0.6);
    state.update(&map, &raw);
    assert!((state.axis("move_x") - 0.5).abs() < 1e-3);
    raw.set_axis(Axis::parse("mouse.dx").unwrap(), 0.8);
    state.update(&map, &raw);
    assert!((state.axis("look_x") - 0.4).abs() < 1e-3);
}

#[test]
fn maps_round_trip_as_text_and_reject_bad_lines() {
    let map = map();
    assert_eq!(InputMap::parse(&map.to_text()).unwrap(), map);
    assert!(matches!(InputMap::parse("action jump = space"), Err(InputError::BadName(_))));
    assert!(matches!(
        InputMap::parse("jump = key.space"),
        Err(InputError::BadLine { line: 1, .. })
    ));
    let mut rebound = map.clone();
    rebound.clear("jump");
    rebound.bind_action("jump", Button::key("w"));
    assert!(rebound.to_text().contains("action jump = key.w\n"));
}

#[test]
fn a_replay_drives_the_game_exactly_like_the_live_run() {
    let map = map();
    let stick = Axis::parse("pad.left_x").unwrap();
    let mut recording = InputRecording::new();
    let mut live = ActionState::default();
    let mut live_trace = Vec::new();
    let mut raw = RawInput::default();
    for frame in 0..300u32 {
        if frame % 37 == 0 {
            raw.press(Button::key("space"));
        }
        if frame % 37 == 5 {
            raw.release(&Button::key("space"));
        }
        // An awkward analog value every frame: it must replay to the same bits.
        raw.set_axis(stick.clone(), ((frame as f32) * 0.0123).sin());
        recording.push(&raw);
        live.update(&map, &raw);
        live_trace.push((live.get("jump"), live.axis("move_x").to_bits()));
    }
    let text = recording.to_text();
    let loaded = InputRecording::parse(&text).unwrap();
    assert_eq!(loaded, recording);
    let mut replay = ActionState::default();
    for frame in 0..loaded.len() {
        replay.update(&map, loaded.frame(frame).unwrap());
        assert_eq!(
            (replay.get("jump"), replay.axis("move_x").to_bits()),
            live_trace[frame],
            "frame {frame}"
        );
    }
}

#[test]
fn recordings_store_only_changes() {
    let mut recording = InputRecording::new();
    let mut raw = RawInput::default();
    for frame in 0..1000 {
        if frame == 10 {
            raw.press(Button::key("space"));
        }
        if frame == 20 {
            raw.release(&Button::key("space"));
        }
        recording.push(&raw);
    }
    assert_eq!(
        recording.to_text(),
        "seal_input_recording 1 frames 1000\n10 +key.space\n20 -key.space\n"
    );
    assert_eq!(InputRecording::parse(&recording.to_text()).unwrap(), recording);
}
