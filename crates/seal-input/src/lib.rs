//! Input for Seal games.
//!
//! Devices feed a [`RawInput`] each frame: which buttons are down and where
//! the analog axes are. An [`InputMap`] turns that into named actions and
//! axes ("jump", "move_x") held in [`ActionState`], which gameplay reads.
//!
//! Analog values are rounded to 1/32767 steps the moment they enter, so the
//! game sees exactly the numbers a recording stores. Playing an
//! [`InputRecording`] back therefore drives the simulation bit for bit as
//! the live run did.

mod devices;
mod modifiers;
mod pad_link;
mod pad_reports;
pub use devices::{
    deliver_light, format_pad_light, look_delta, parse_pad_light, pen_from_event, write_motion,
    write_pen, write_pointer, PadBackend, PadMotion, PenEvent, PenSample, PenTool, PointerSample,
    ACCEL_FULL_SCALE, GYRO_FULL_SCALE, MOUSE_AXIS_PIXELS,
};
pub use modifiers::BindMod;
pub use pad_link::{PadId, PadSensors};
pub use pad_reports::{
    controller_notes, crc32_ieee, descriptor_is_gamepad, light_report, match_hid_index, pad_family,
    parse_pro_calibration, parse_pro_motion, parse_pro_reply, pick_interface, pro_enable_imu,
    pro_output, pro_read_factory_imu, pro_report_mode, report_crc, resolve, sony_calibration,
    sony_motion, PadFamily, ProScale, Resolved, SonyScale, Wire, NOTE_NOT_FOUND, NOTE_NO_LIGHT,
    NOTE_NO_MOTION, NOTE_NO_PAD,
};

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// A digital input, named like `key.space`, `mouse.left` or `pad.south`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Button(String);

impl Button {
    /// Accepts `key.*`, `mouse.*`, `pad.*` and `pen.*` names; names are lower case.
    pub fn parse(name: &str) -> Result<Self, InputError> {
        let name = name.trim().to_ascii_lowercase();
        let (device, rest) =
            name.split_once('.').ok_or_else(|| InputError::BadName(name.clone()))?;
        let valid = matches!(device, "key" | "mouse" | "pad" | "pen")
            && !rest.is_empty()
            && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if valid {
            Ok(Self(name))
        } else {
            Err(InputError::BadName(name))
        }
    }

    pub fn key(name: &str) -> Self {
        Self::parse(&format!("key.{name}")).expect("valid key name")
    }

    pub fn name(&self) -> &str {
        &self.0
    }
}

/// An analog input, named like `pad.left_x` or `mouse.dx`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Axis(String);

impl Axis {
    pub fn parse(name: &str) -> Result<Self, InputError> {
        Button::parse(name).map(|b| Self(b.0))
    }

    pub fn name(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputError {
    BadName(String),
    BadLine { line: usize, text: String },
    UnknownAction(String),
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadName(n) => {
                write!(f, "{n:?} is not an input name like key.space or pad.left_x")
            }
            Self::BadLine { line, text } => write!(f, "line {line}: cannot read {text:?}"),
            Self::UnknownAction(a) => write!(f, "no action or axis named {a:?}"),
        }
    }
}

impl std::error::Error for InputError {}

const STEPS: f32 = 32767.0;

fn quantize(v: f32) -> i16 {
    (v.clamp(-1.0, 1.0) * STEPS).round() as i16
}

/// One frame of device state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RawInput {
    down: BTreeSet<Button>,
    axes: BTreeMap<Axis, i16>,
}

impl RawInput {
    pub fn press(&mut self, button: Button) {
        self.down.insert(button);
    }

    pub fn release(&mut self, button: &Button) {
        self.down.remove(button);
    }

    pub fn is_down(&self, button: &Button) -> bool {
        self.down.contains(button)
    }

    /// Set an analog value in -1..1. It is rounded to the recording's precision.
    pub fn set_axis(&mut self, axis: Axis, value: f32) {
        let q = quantize(value);
        if q == 0 {
            self.axes.remove(&axis);
        } else {
            self.axes.insert(axis, q);
        }
    }

    pub fn axis(&self, axis: &Axis) -> f32 {
        self.axes.get(axis).map_or(0.0, |q| *q as f32 / STEPS)
    }
}

/// Where an axis value comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum AxisSource {
    /// -1 while `negative` is held, +1 while `positive` is held, 0 for both.
    Buttons { negative: Button, positive: Button },
    /// A device axis, zero inside `deadzone`, then rescaled to reach 1 at full tilt.
    Analog { axis: Axis, deadzone: f32, scale: f32 },
}

/// Names actions and axes and lists what drives them.
///
/// Text form, one binding per line:
/// ```text
/// action jump = key.space, pad.south
/// axis move_x = key.a key.d, pad.left_x deadzone 0.2
/// axis look_x = mouse.dx scale 0.5
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InputMap {
    actions: BTreeMap<String, Vec<Button>>,
    /// Modifiers aligned with each action's buttons. Missing slots are plain buttons.
    mods: BTreeMap<String, Vec<BindMod>>,
    axes: BTreeMap<String, Vec<AxisSource>>,
}

impl InputMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind_action(&mut self, action: &str, button: Button) -> &mut Self {
        let list = self.actions.entry(action.into()).or_default();
        if !list.contains(&button) {
            list.push(button);
        }
        self
    }

    pub fn bind_axis(&mut self, axis: &str, source: AxisSource) -> &mut Self {
        self.axes.entry(axis.into()).or_default().push(source);
        self
    }

    /// Remove every binding of an action or axis, for rebinding screens.
    pub fn clear(&mut self, name: &str) {
        self.actions.remove(name);
        self.mods.remove(name);
        self.axes.remove(name);
    }

    /// The modifier on one binding. A missing slot is a plain button.
    pub fn modifier(&self, action: &str, slot: usize) -> BindMod {
        self.mods.get(action).and_then(|m| m.get(slot)).copied().unwrap_or(BindMod::None)
    }

    /// Set the modifier on a binding that already exists.
    pub fn set_modifier(&mut self, action: &str, slot: usize, modifier: BindMod) -> bool {
        let Some(list) = self.actions.get(action) else { return false };
        if slot >= list.len() {
            return false;
        }
        let mods = self.mods.entry(action.to_string()).or_default();
        while mods.len() <= slot {
            mods.push(BindMod::None);
        }
        mods[slot] = modifier;
        if mods.iter().all(|m| *m == BindMod::None) {
            self.mods.remove(action);
        }
        true
    }

    pub fn actions(&self) -> impl Iterator<Item = (&str, &[Button])> {
        self.actions.iter().map(|(k, v)| (k.as_str(), v.as_slice()))
    }

    pub fn parse(text: &str) -> Result<Self, InputError> {
        let mut map = Self::new();
        for (index, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let bad = || InputError::BadLine { line: index + 1, text: raw.to_string() };
            let (head, body) = line.split_once('=').ok_or_else(bad)?;
            let mut head = head.split_whitespace();
            let (kind, name) = (head.next().ok_or_else(bad)?, head.next().ok_or_else(bad)?);
            if head.next().is_some() {
                return Err(bad());
            }
            if kind == "action" {
                // An action with no buttons yet is kept, so a new one survives a save.
                map.actions.entry(name.into()).or_default();
            }
            for part in body.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                let words: Vec<&str> = part.split_whitespace().collect();
                match kind {
                    "action" => {
                        let (button_name, modifier) =
                            modifiers::parse_binding(&words).map_err(|e| match e {
                                InputError::BadLine { text, .. } => {
                                    InputError::BadLine { line: index + 1, text }
                                }
                                other => other,
                            })?;
                        map.bind_action(name, Button::parse(&button_name)?);
                        if modifier != BindMod::None {
                            let slot = map.bindings(name).len() - 1;
                            map.set_modifier(name, slot, modifier);
                        }
                    }
                    "axis" => {
                        let source = if words.len() == 2 {
                            AxisSource::Buttons {
                                negative: Button::parse(words[0])?,
                                positive: Button::parse(words[1])?,
                            }
                        } else {
                            let mut deadzone = 0.0;
                            let mut scale = 1.0;
                            let mut rest = words[1..].chunks(2);
                            for pair in &mut rest {
                                let value: f32 =
                                    pair.get(1).and_then(|v| v.parse().ok()).ok_or_else(bad)?;
                                match pair[0] {
                                    "deadzone" => deadzone = value,
                                    "scale" => scale = value,
                                    _ => return Err(bad()),
                                }
                            }
                            AxisSource::Analog { axis: Axis::parse(words[0])?, deadzone, scale }
                        };
                        map.bind_axis(name, source);
                    }
                    _ => return Err(bad()),
                }
            }
        }
        Ok(map)
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for (name, buttons) in &self.actions {
            let list: Vec<String> = buttons
                .iter()
                .enumerate()
                .map(|(slot, button)| {
                    modifiers::binding_text(button.name(), self.modifier(name, slot))
                })
                .collect();
            out += &format!("action {name} = {}\n", list.join(", "));
        }
        for (name, sources) in &self.axes {
            let list: Vec<String> = sources
                .iter()
                .map(|s| match s {
                    AxisSource::Buttons { negative, positive } => {
                        format!("{} {}", negative.name(), positive.name())
                    }
                    AxisSource::Analog { axis, deadzone, scale } => {
                        let mut s = axis.name().to_string();
                        if *deadzone != 0.0 {
                            s += &format!(" deadzone {deadzone}");
                        }
                        if *scale != 1.0 {
                            s += &format!(" scale {scale}");
                        }
                        s
                    }
                })
                .collect();
            out += &format!("axis {name} = {}\n", list.join(", "));
        }
        out
    }
}

/// One action's state this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ActionValue {
    pub pressed: bool,
    pub just_pressed: bool,
    pub just_released: bool,
    /// Frames the action has been held, counting this one.
    pub held_frames: u32,
    /// Seconds the action has been held, after modifiers. Zero while it is up.
    pub held_seconds: f32,
}

/// What gameplay reads: actions and axes after mapping.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActionState {
    actions: BTreeMap<String, ActionValue>,
    axes: BTreeMap<String, f32>,
    clocks: BTreeMap<String, Vec<modifiers::SlotClock>>,
    pub frame: u64,
}

impl ActionState {
    /// Advance one frame from the device state. Bindings with no modifier ignore `dt`.
    pub fn update_for(&mut self, map: &InputMap, raw: &RawInput, dt: f32) {
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        self.frame += 1;
        let names: Vec<String> = map.actions.keys().cloned().collect();
        self.clocks.retain(|name, _| names.iter().any(|n| n == name));
        for (name, buttons) in &map.actions {
            let clocks = self.clocks.entry(name.clone()).or_default();
            if clocks.len() < buttons.len() {
                clocks.resize(buttons.len(), modifiers::SlotClock::default());
            } else if clocks.len() > buttons.len() {
                clocks.truncate(buttons.len());
            }
            let mut down = false;
            for (slot, button) in buttons.iter().enumerate() {
                let on = clocks[slot].step(map.modifier(name, slot), raw.is_down(button), dt);
                down |= on;
            }
            let v = self.actions.entry(name.clone()).or_default();
            v.just_pressed = down && !v.pressed;
            v.just_released = !down && v.pressed;
            v.pressed = down;
            v.held_frames = if down { v.held_frames.saturating_add(1) } else { 0 };
            v.held_seconds = if down { v.held_seconds + dt } else { 0.0 };
        }
        for (name, sources) in &map.axes {
            let mut value = 0.0f32;
            for source in sources {
                let v = match source {
                    AxisSource::Buttons { negative, positive } => {
                        (raw.is_down(positive) as i32 - raw.is_down(negative) as i32) as f32
                    }
                    AxisSource::Analog { axis, deadzone, scale } => {
                        let raw_value = raw.axis(axis);
                        let mag = raw_value.abs();
                        if mag <= *deadzone {
                            0.0
                        } else {
                            raw_value.signum() * (mag - deadzone) / (1.0 - deadzone) * scale
                        }
                    }
                };
                // The strongest source wins, so a stick and keys don't add up past 1.
                if v.abs() > value.abs() {
                    value = v;
                }
            }
            self.axes.insert(name.clone(), value);
        }
    }

    /// Advance one frame. Same as [`Self::update_for`] with no elapsed time,
    /// which leaves hold and double-tap modifiers waiting.
    pub fn update(&mut self, map: &InputMap, raw: &RawInput) {
        self.update_for(map, raw, 0.0);
    }

    pub fn get(&self, action: &str) -> ActionValue {
        self.actions.get(action).copied().unwrap_or_default()
    }

    pub fn pressed(&self, action: &str) -> bool {
        self.get(action).pressed
    }

    pub fn just_pressed(&self, action: &str) -> bool {
        self.get(action).just_pressed
    }

    pub fn just_released(&self, action: &str) -> bool {
        self.get(action).just_released
    }

    /// Seconds the action has been held after its modifiers, or 0.
    pub fn held_seconds(&self, action: &str) -> f32 {
        self.get(action).held_seconds
    }

    pub fn axis(&self, axis: &str) -> f32 {
        self.axes.get(axis).copied().unwrap_or(0.0)
    }
}

/// Device frames saved as changes, for replays and bug reports.
///
/// Text form: a header line, then one line per frame that changed:
/// `<frame> +key.space -key.a pad.left_x=12000`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InputRecording {
    frames: Vec<RawInput>,
}

impl InputRecording {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, frame: &RawInput) {
        self.frames.push(frame.clone());
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn frame(&self, index: usize) -> Option<&RawInput> {
        self.frames.get(index)
    }

    pub fn to_text(&self) -> String {
        let mut out = format!("seal_input_recording 1 frames {}\n", self.frames.len());
        let empty = RawInput::default();
        let mut prev = &empty;
        for (index, frame) in self.frames.iter().enumerate() {
            let mut parts = Vec::new();
            for b in frame.down.difference(&prev.down) {
                parts.push(format!("+{}", b.name()));
            }
            for b in prev.down.difference(&frame.down) {
                parts.push(format!("-{}", b.name()));
            }
            let names: BTreeSet<&Axis> = frame.axes.keys().chain(prev.axes.keys()).collect();
            for axis in names {
                let (now, before) = (frame.axes.get(axis), prev.axes.get(axis));
                if now != before {
                    parts.push(format!("{}={}", axis.name(), now.copied().unwrap_or(0)));
                }
            }
            if !parts.is_empty() {
                out += &format!("{index} {}\n", parts.join(" "));
            }
            prev = frame;
        }
        out
    }

    pub fn parse(text: &str) -> Result<Self, InputError> {
        let mut lines = text.lines().enumerate();
        let bad = |line: usize, text: &str| InputError::BadLine {
            line: line + 1,
            text: text.to_string(),
        };
        let (_, header) = lines.next().ok_or_else(|| bad(0, ""))?;
        let words: Vec<&str> = header.split_whitespace().collect();
        let count: usize = match words.as_slice() {
            ["seal_input_recording", "1", "frames", n] => n.parse().map_err(|_| bad(0, header))?,
            _ => return Err(bad(0, header)),
        };
        let mut frames = Vec::with_capacity(count);
        let mut current = RawInput::default();
        for (line, text) in lines {
            let mut words = text.split_whitespace();
            let Some(first) = words.next() else { continue };
            let index: usize = first.parse().map_err(|_| bad(line, text))?;
            if index < frames.len() || index >= count {
                return Err(bad(line, text));
            }
            while frames.len() < index {
                frames.push(current.clone());
            }
            for word in words {
                if let Some(name) = word.strip_prefix('+') {
                    current.down.insert(Button::parse(name)?);
                } else if let Some(name) = word.strip_prefix('-') {
                    current.down.remove(&Button::parse(name)?);
                } else if let Some((name, value)) = word.split_once('=') {
                    let q: i16 = value.parse().map_err(|_| bad(line, text))?;
                    let axis = Axis::parse(name)?;
                    if q == 0 {
                        current.axes.remove(&axis);
                    } else {
                        current.axes.insert(axis, q);
                    }
                } else {
                    return Err(bad(line, text));
                }
            }
            frames.push(current.clone());
        }
        while frames.len() < count {
            frames.push(current.clone());
        }
        Ok(Self { frames })
    }
}

/// Gamepad buttons, by the names bindings use (standard pad layout).
pub const PAD_BUTTONS: &[&str] = &[
    "pad.south",
    "pad.east",
    "pad.west",
    "pad.north",
    "pad.dpad_up",
    "pad.dpad_down",
    "pad.dpad_left",
    "pad.dpad_right",
    "pad.lb",
    "pad.rb",
    "pad.lt",
    "pad.rt",
    "pad.start",
    "pad.select",
    "pad.lstick",
    "pad.rstick",
];

/// Gamepad sticks and triggers, -1..1 (triggers 0..1).
pub const PAD_AXES: &[&str] =
    &["pad.left_x", "pad.left_y", "pad.right_x", "pad.right_y", "pad.lt_value", "pad.rt_value"];

/// Controls a new project starts with: keys and a gamepad for each action.
pub const DEFAULT_PLAY_MAP: &str = "\
# Seal input bindings: action <name> = <button>, <button>...
# axis <name> = <negative button> <positive button>, <stick> deadzone <0..1>
action move_left = key.a, key.arrowleft, pad.dpad_left
action move_right = key.d, key.arrowright, pad.dpad_right
action jump = key.space, key.w, key.arrowup, pad.south
action pause = key.escape, pad.start
axis move_x = pad.left_x deadzone 0.25
";

impl InputMap {
    /// The controls every new project starts with.
    pub fn default_play() -> Self {
        Self::parse(DEFAULT_PLAY_MAP).expect("default bindings parse")
    }

    /// The buttons bound to an action, in slot order.
    pub fn bindings(&self, action: &str) -> &[Button] {
        self.actions.get(action).map_or(&[], Vec::as_slice)
    }

    pub fn has_action(&self, action: &str) -> bool {
        self.actions.contains_key(action)
    }

    pub fn axes(&self) -> impl Iterator<Item = (&str, &[AxisSource])> {
        self.axes.iter().map(|(k, v)| (k.as_str(), v.as_slice()))
    }

    /// Add an action with no buttons yet. Names are letters, digits and `_`.
    pub fn add_action(&mut self, action: &str) -> Result<(), InputError> {
        let ok =
            !action.is_empty() && action.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !ok {
            return Err(InputError::BadName(action.to_string()));
        }
        self.actions.entry(action.into()).or_default();
        Ok(())
    }

    /// Put `button` in `slot` of `action`, or add it when `slot` is past the
    /// end. A button drives one action only, so it leaves any other action
    /// that had it; that action's name is returned so a screen can say so.
    pub fn rebind(
        &mut self,
        action: &str,
        slot: usize,
        button: Button,
    ) -> Result<Option<String>, InputError> {
        if !self.actions.contains_key(action) {
            return Err(InputError::UnknownAction(action.to_string()));
        }
        let mut taken_from = None;
        let others: Vec<String> =
            self.actions.keys().filter(|n| n.as_str() != action).cloned().collect();
        for name in others {
            let Some(list) = self.actions.get_mut(&name) else { continue };
            if !list.contains(&button) {
                continue;
            }
            let slots: Vec<usize> =
                list.iter().enumerate().filter(|(_, b)| **b == button).map(|(i, _)| i).collect();
            list.retain(|b| b != &button);
            taken_from = Some(name.clone());
            if let Some(mods) = self.mods.get_mut(&name) {
                for s in slots.into_iter().rev() {
                    if s < mods.len() {
                        mods.remove(s);
                    }
                }
            }
        }
        self.drop_blank_mods();
        let already =
            self.actions.get(action).expect("checked above").iter().position(|b| b == &button);
        let carried = already.map(|at| self.modifier(action, at));
        if let Some(at) = already {
            self.actions.get_mut(action).expect("checked").remove(at);
            if let Some(mods) = self.mods.get_mut(action) {
                if at < mods.len() {
                    mods.remove(at);
                }
            }
        }
        let placed = {
            let list = self.actions.get_mut(action).expect("checked above");
            if slot < list.len() {
                list[slot] = button;
                slot
            } else {
                list.push(button);
                list.len() - 1
            }
        };
        if let Some(modifier) = carried {
            self.set_modifier(action, placed, modifier);
        }
        Ok(taken_from)
    }

    fn drop_blank_mods(&mut self) {
        self.mods.retain(|_, mods| mods.iter().any(|m| *m != BindMod::None));
    }

    /// Remove an action and its buttons.
    pub fn remove_action(&mut self, action: &str) -> bool {
        self.mods.remove(action);
        self.actions.remove(action).is_some()
    }

    /// Set the dead zone of every stick or trigger that drives `axis`
    /// (clamped to 0..0.95). False when the axis has no analog source.
    pub fn set_deadzone(&mut self, axis: &str, deadzone: f32) -> bool {
        let deadzone = if deadzone.is_finite() { deadzone.clamp(0.0, 0.95) } else { 0.0 };
        let mut found = false;
        for source in self.axes.get_mut(axis).into_iter().flatten() {
            if let AxisSource::Analog { deadzone: d, .. } = source {
                *d = deadzone;
                found = true;
            }
        }
        found
    }

    /// Drop the button in `slot` of `action`, if there is one.
    pub fn unbind(&mut self, action: &str, slot: usize) {
        if let Some(list) = self.actions.get_mut(action) {
            if slot < list.len() {
                list.remove(slot);
            }
        }
        if let Some(mods) = self.mods.get_mut(action) {
            if slot < mods.len() {
                mods.remove(slot);
            }
        }
        self.drop_blank_mods();
    }

    /// The actions whose buttons differ from `base`, for saving a player's
    /// own bindings next to the game's defaults.
    pub fn changes_from(&self, base: &InputMap) -> InputMap {
        let mut out = InputMap::new();
        for (name, list) in &self.actions {
            let buttons_differ = base.actions.get(name) != Some(list);
            let mods_differ = self.modifier_list(name) != base.modifier_list(name);
            if buttons_differ || mods_differ {
                out.actions.insert(name.clone(), list.clone());
                if let Some(mods) = self.mods.get(name) {
                    out.mods.insert(name.clone(), mods.clone());
                }
            }
        }
        out
    }

    fn modifier_list(&self, action: &str) -> Vec<BindMod> {
        let n = self.actions.get(action).map(|l| l.len()).unwrap_or(0);
        (0..n).map(|slot| self.modifier(action, slot)).collect()
    }

    /// These bindings with a player's changed actions laid over them.
    pub fn with_changes(&self, changes: &InputMap) -> InputMap {
        let mut out = self.clone();
        for (name, list) in &changes.actions {
            if out.actions.contains_key(name) {
                out.actions.insert(name.clone(), list.clone());
                match changes.mods.get(name).filter(|m| m.iter().any(|x| *x != BindMod::None)) {
                    Some(mods) => {
                        out.mods.insert(name.clone(), mods.clone());
                    }
                    None => {
                        out.mods.remove(name);
                    }
                }
            }
        }
        out
    }
}

impl RawInput {
    /// Every button held this frame.
    pub fn buttons(&self) -> impl Iterator<Item = &Button> {
        self.down.iter()
    }

    /// Let go of every button of one device (`key`, `mouse`, `pad` or `pen`) and
    /// zero its axes, e.g. when a pad is unplugged.
    pub fn clear_device(&mut self, device: &str) {
        let prefix = format!("{device}.");
        self.down.retain(|b| !b.0.starts_with(&prefix));
        self.axes.retain(|a, _| !a.0.starts_with(&prefix));
    }
}

/// What a "press a button" rebinding wait ended with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Capture {
    Bound(Button),
    Cancelled,
}

/// Waits for the next button press to bind to an action. Buttons already
/// held when it starts don't count until they are let go and pressed again.
/// Escape cancels. Mouse buttons are ignored, so clicking the Rebind button
/// itself doesn't bind the mouse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RebindCapture {
    pub action: String,
    pub slot: usize,
    held: BTreeSet<Button>,
}

impl RebindCapture {
    pub fn start(action: &str, slot: usize, raw: &RawInput) -> Self {
        Self { action: action.to_string(), slot, held: raw.down.clone() }
    }

    /// Call once a frame; returns the result when the wait is over.
    pub fn poll(&mut self, raw: &RawInput) -> Option<Capture> {
        self.held.retain(|b| raw.is_down(b));
        let fresh = raw.down.iter().find(|b| !self.held.contains(b) && !b.0.starts_with("mouse."));
        match fresh {
            Some(b) if b.0 == "key.escape" => Some(Capture::Cancelled),
            Some(b) => Some(Capture::Bound(b.clone())),
            None => None,
        }
    }
}

/// One rumble request: motor strengths 0..1 for `seconds`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rumble {
    /// The heavy, low-frequency motor.
    pub strong: f32,
    /// The light, high-frequency motor.
    pub weak: f32,
    pub seconds: f32,
}

impl Rumble {
    /// Strengths are clamped to 0..1 and the length to 0..10 seconds.
    pub fn new(strong: f32, weak: f32, seconds: f32) -> Self {
        let unit = |v: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
        let secs = if seconds.is_finite() { seconds.clamp(0.0, 10.0) } else { 0.0 };
        Self { strong: unit(strong), weak: unit(weak), seconds: secs }
    }
}

/// Rumbles playing at once. Each motor runs at the strongest request still
/// going, so short hits on top of a long hum don't add up past full power.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RumbleMixer {
    playing: Vec<(Rumble, f32)>,
}

impl RumbleMixer {
    pub fn play(&mut self, rumble: Rumble) {
        if rumble.seconds > 0.0 && (rumble.strong > 0.0 || rumble.weak > 0.0) {
            self.playing.push((rumble, rumble.seconds));
        }
    }

    pub fn stop(&mut self) {
        self.playing.clear();
    }

    /// Motor strengths right now, as (strong, weak).
    pub fn level(&self) -> (f32, f32) {
        self.playing
            .iter()
            .fold((0.0f32, 0.0f32), |(s, w), (r, _)| (s.max(r.strong), w.max(r.weak)))
    }

    /// Advance time; returns the motor strengths for the coming frame.
    pub fn step(&mut self, dt: f32) -> (f32, f32) {
        for (_, left) in &mut self.playing {
            *left -= dt.max(0.0);
        }
        self.playing.retain(|(_, left)| *left > 0.0);
        self.level()
    }
}

/// A button's name as a player reads it: "Space", "Left Arrow", "Pad A".
pub fn button_label(button: &Button) -> String {
    let (device, rest) = button.0.split_once('.').unwrap_or(("", &button.0));
    match device {
        "pad" => match rest {
            "south" => "Pad A (bottom)".into(),
            "east" => "Pad B (right)".into(),
            "west" => "Pad X (left)".into(),
            "north" => "Pad Y (top)".into(),
            "dpad_up" => "Pad D-pad Up".into(),
            "dpad_down" => "Pad D-pad Down".into(),
            "dpad_left" => "Pad D-pad Left".into(),
            "dpad_right" => "Pad D-pad Right".into(),
            "lb" => "Pad Left Bumper".into(),
            "rb" => "Pad Right Bumper".into(),
            "lt" => "Pad Left Trigger".into(),
            "rt" => "Pad Right Trigger".into(),
            "start" => "Pad Start".into(),
            "select" => "Pad Select".into(),
            "lstick" => "Pad Left Stick Press".into(),
            "rstick" => "Pad Right Stick Press".into(),
            other => format!("Pad {other}"),
        },
        "mouse" => match rest {
            "left" => "Left Mouse Button".into(),
            "right" => "Right Mouse Button".into(),
            "middle" => "Middle Mouse Button".into(),
            other => format!("Mouse {other}"),
        },
        "pen" => match rest {
            "tip" => "Pen".into(),
            "eraser" => "Eraser".into(),
            other => format!("Pen {other}"),
        },
        _ => match rest {
            "arrowleft" => "Left Arrow".into(),
            "arrowright" => "Right Arrow".into(),
            "arrowup" => "Up Arrow".into(),
            "arrowdown" => "Down Arrow".into(),
            "space" => "Space".into(),
            "escape" => "Esc".into(),
            "enter" => "Enter".into(),
            "tab" => "Tab".into(),
            "backspace" => "Backspace".into(),
            other if other.len() == 1 => other.to_ascii_uppercase(),
            other => {
                let mut c = other.chars();
                c.next()
                    .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                    .unwrap_or_default()
            }
        },
    }
}
