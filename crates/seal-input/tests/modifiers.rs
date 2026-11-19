use seal_input::{ActionState, BindMod, Button, InputMap, RawInput};

fn press(raw: &mut RawInput, name: &str) {
    raw.press(Button::parse(name).unwrap());
}

#[test]
fn an_unmodified_map_keeps_its_text() {
    let text = "\
action jump = key.space, pad.south
axis move_x = key.a key.d
";
    let map = InputMap::parse(text).unwrap();
    assert_eq!(map.to_text(), text);
    assert_eq!(map.modifier("jump", 0), BindMod::None);
    let again = InputMap::parse(&map.to_text()).unwrap();
    assert_eq!(again, map);
}

#[test]
fn hold_waits_for_the_seconds_then_stays_down() {
    let map = InputMap::parse("action jump = key.space hold 0.4\n").unwrap();
    assert_eq!(map.modifier("jump", 0), BindMod::Hold(0.4));
    assert_eq!(map.to_text(), "action jump = key.space hold 0.4\n");
    let mut state = ActionState::default();
    let mut raw = RawInput::default();
    press(&mut raw, "key.space");
    state.update_for(&map, &raw, 0.2);
    assert!(!state.pressed("jump"));
    state.update_for(&map, &raw, 0.2);
    let held = state.get("jump");
    assert!(held.pressed && held.just_pressed);
    assert!((held.held_seconds - 0.2).abs() < 1e-4);
    state.update_for(&map, &raw, 0.2);
    assert!(state.pressed("jump") && !state.just_pressed("jump"));
    assert!((state.held_seconds("jump") - 0.4).abs() < 1e-4);
    raw.release(&Button::parse("key.space").unwrap());
    state.update_for(&map, &raw, 0.2);
    assert!(state.just_released("jump") && !state.pressed("jump"));
    assert_eq!(state.held_seconds("jump"), 0.0);
}

#[test]
fn double_tap_needs_a_second_press_inside_the_window() {
    let map = InputMap::parse("action fire = mouse.left double 0.25\n").unwrap();
    let mut state = ActionState::default();
    let mut raw = RawInput::default();
    press(&mut raw, "mouse.left");
    state.update_for(&map, &raw, 0.1);
    assert!(!state.pressed("fire"));
    raw.release(&Button::parse("mouse.left").unwrap());
    state.update_for(&map, &raw, 0.1);
    press(&mut raw, "mouse.left");
    state.update_for(&map, &raw, 0.05);
    assert!(state.just_pressed("fire") && state.pressed("fire"));
    raw.release(&Button::parse("mouse.left").unwrap());
    state.update_for(&map, &raw, 0.05);
    assert!(state.just_released("fire"));
    // A second press after the window does not count.
    state.update_for(&map, &raw, 0.4);
    press(&mut raw, "mouse.left");
    state.update_for(&map, &raw, 0.05);
    assert!(!state.pressed("fire"));
}

#[test]
fn toggle_stays_on_after_release() {
    let map = InputMap::parse("action crouch = key.c toggle\n").unwrap();
    let mut state = ActionState::default();
    let mut raw = RawInput::default();
    press(&mut raw, "key.c");
    state.update_for(&map, &raw, 0.016);
    assert!(state.just_pressed("crouch"));
    raw.release(&Button::parse("key.c").unwrap());
    state.update_for(&map, &raw, 0.016);
    assert!(state.pressed("crouch") && !state.just_released("crouch"));
    assert!(state.held_seconds("crouch") > 0.0);
    press(&mut raw, "key.c");
    state.update_for(&map, &raw, 0.016);
    assert!(state.just_released("crouch") && !state.pressed("crouch"));
}

#[test]
fn the_same_presses_produce_the_same_trace() {
    let map = InputMap::parse(
        "action jump = key.space hold 0.3\naction fire = key.f double 0.2\naction crouch = key.c toggle\n",
    )
    .unwrap();
    let run = || {
        let mut state = ActionState::default();
        let mut trace = Vec::new();
        let script = [
            (true, false, false, 0.1),
            (true, false, false, 0.1),
            (true, true, true, 0.1),
            (false, false, false, 0.05),
            (false, true, false, 0.05),
            (false, false, true, 0.2),
        ];
        for (jump, fire, crouch, dt) in script {
            let mut raw = RawInput::default();
            if jump {
                press(&mut raw, "key.space");
            }
            if fire {
                press(&mut raw, "key.f");
            }
            if crouch {
                press(&mut raw, "key.c");
            }
            state.update_for(&map, &raw, dt);
            trace.push(state.get("jump"));
            trace.push(state.get("fire"));
            trace.push(state.get("crouch"));
        }
        trace
    };
    assert_eq!(run(), run());
}

#[test]
fn a_damaged_modifier_line_is_refused() {
    let missing = InputMap::parse("action jump = key.space hold\n").unwrap_err();
    assert!(missing.to_string().contains("hold"), "{missing}");
    let bad = InputMap::parse("action jump = key.space hold nope\n").unwrap_err();
    assert!(bad.to_string().contains("nope"), "{bad}");
    let odd = InputMap::parse("action jump = key.space slide 1\n").unwrap_err();
    assert!(odd.to_string().contains("slide"), "{odd}");
}

#[test]
fn a_players_rebind_keeps_a_modifier_on_that_slot() {
    let mut map = InputMap::parse("action jump = key.space hold 0.5, pad.south\n").unwrap();
    map.rebind("jump", 0, Button::parse("key.w").unwrap()).unwrap();
    assert_eq!(map.modifier("jump", 0), BindMod::Hold(0.5));
    assert_eq!(map.bindings("jump")[0], Button::parse("key.w").unwrap());
    let changes = map.changes_from(&InputMap::parse("action jump = key.space, pad.south\n").unwrap());
    let laid = InputMap::parse("action jump = key.space hold 0.5, pad.south\n")
        .unwrap()
        .with_changes(&changes);
    assert_eq!(laid.bindings("jump")[0], Button::parse("key.w").unwrap());
}
