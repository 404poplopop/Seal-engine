//! Damaged key maps and input recordings must be refused, not crash the loader.

use seal_input::{Button, InputMap, InputRecording, RawInput};
use seal_testkit::garbage::{survives, text};

#[test]
fn broken_key_maps_and_recordings_are_refused_not_crashed_on() {
    let mut map = InputMap::new();
    map.bind_action("jump", Button::key("space"));
    map.bind_action("jump", Button::key("w"));
    map.bind_action("fire", Button::parse("pad.south").unwrap());
    survives("key map", map.to_text().as_bytes(), text(|t| drop(InputMap::parse(t))));

    let mut recording = InputRecording::new();
    let mut raw = RawInput::default();
    recording.push(&raw);
    raw.press(Button::key("space"));
    recording.push(&raw);
    raw.release(&Button::key("space"));
    recording.push(&raw);
    survives("recording", recording.to_text().as_bytes(), text(|t| drop(InputRecording::parse(t))));
}
