//! Per-binding timing for actions: hold, double tap, and toggle.
//!
//! A binding with no modifier behaves as a plain button. The text form only
//! writes a modifier when one is set, so an existing bindings file is unchanged.

use super::InputError;

/// How one button binding turns into an action.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum BindMod {
    /// Down while the button is down.
    #[default]
    None,
    /// Down only after the button has been held for this many seconds.
    Hold(f32),
    /// Down on a second press that lands within this many seconds of the first,
    /// and while that second press is held.
    Double(f32),
    /// Each press flips the action on or off. Releasing the button does not.
    Toggle,
}

/// Read `key.space`, `key.space hold 0.4`, `mouse.left double 0.25`, or `key.c toggle`.
pub fn parse_binding(words: &[&str]) -> Result<(String, BindMod), InputError> {
    let bad = || InputError::BadLine { line: 0, text: words.join(" ") };
    let name = words.first().copied().ok_or_else(bad)?;
    let modifier = match words {
        [_] => BindMod::None,
        [_, "toggle"] => BindMod::Toggle,
        [_, "hold", n] => BindMod::Hold(seconds(n).ok_or_else(bad)?),
        [_, "double", n] => BindMod::Double(seconds(n).ok_or_else(bad)?),
        _ => return Err(bad()),
    };
    Ok((name.to_string(), modifier))
}

fn seconds(text: &str) -> Option<f32> {
    let n: f32 = text.parse().ok()?;
    n.is_finite().then_some(n.max(0.0))
}

/// Short text for a seconds value, trailing zeros dropped.
pub fn seconds_text(value: f32) -> String {
    let text = format!("{value:.4}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() {
        "0".into()
    } else {
        trimmed.into()
    }
}

/// The button text plus its modifier, when it has one.
pub fn binding_text(button: &str, modifier: BindMod) -> String {
    match modifier {
        BindMod::None => button.to_string(),
        BindMod::Hold(s) => format!("{button} hold {}", seconds_text(s)),
        BindMod::Double(s) => format!("{button} double {}", seconds_text(s)),
        BindMod::Toggle => format!("{button} toggle"),
    }
}

/// Memory for one binding across frames.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SlotClock {
    was_raw: bool,
    down_for: f32,
    since_tap: f32,
    seen_tap: bool,
    /// Toggle latch, or the successful second tap of a double press.
    latched: bool,
}

impl SlotClock {
    /// One frame. Returns whether this binding drives the action now.
    pub fn step(&mut self, modifier: BindMod, raw_down: bool, dt: f32) -> bool {
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        let edge = raw_down && !self.was_raw;
        let on = match modifier {
            BindMod::None => raw_down,
            BindMod::Hold(seconds) => {
                if raw_down {
                    self.down_for += dt;
                } else {
                    self.down_for = 0.0;
                }
                let need = if seconds.is_finite() { seconds.max(0.0) } else { 0.0 };
                raw_down && self.down_for + 1e-4 >= need
            }
            BindMod::Double(window) => {
                let window = if window.is_finite() { window.max(0.0) } else { 0.0 };
                if edge {
                    let hit = self.seen_tap && self.since_tap <= window + 1e-4;
                    self.since_tap = 0.0;
                    self.seen_tap = true;
                    self.latched = hit;
                    hit
                } else {
                    self.since_tap += dt;
                    if !raw_down {
                        self.latched = false;
                    }
                    raw_down && self.latched
                }
            }
            BindMod::Toggle => {
                if edge {
                    self.latched = !self.latched;
                }
                self.latched
            }
        };
        if !matches!(modifier, BindMod::Double(_)) {
            if edge {
                self.since_tap = 0.0;
                self.seen_tap = true;
            } else {
                self.since_tap += dt;
            }
        }
        self.was_raw = raw_down;
        on
    }
}
