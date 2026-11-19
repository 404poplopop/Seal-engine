//! Real gamepads: their buttons and sticks go into the game's input, and the
//! game's rumble goes back out to them. Buttons are read on Windows and macOS.
//! The light bar and motion sensors are read over the raw device on Windows
//! and Linux, and matched to player 1 by vendor, product and order.

use seal_editor_core::seal_input::{Axis, Button, PadBackend, PadMotion, PadSensors, RawInput};

/// Plugged-in pads, read once a frame.
pub struct Pads {
    #[cfg(any(windows, target_os = "macos"))]
    imp: Option<imp::Pads>,
    sensors: PadSensors,
    extra: Vec<RawInput>,
}

impl Default for Pads {
    fn default() -> Self {
        Self::new()
    }
}

impl Pads {
    pub fn new() -> Self {
        Self {
            #[cfg(any(windows, target_os = "macos"))]
            imp: imp::Pads::new(),
            sensors: PadSensors::new(),
            extra: Vec::new(),
        }
    }

    /// Copy the first connected pad into `raw` (its `pad.*` buttons and
    /// axes). Later pads are kept for [`Self::extra`]. Returns whether a pad is connected.
    pub fn poll(&mut self, raw: &mut RawInput) -> bool {
        raw.clear_device("pad");
        self.extra.clear();
        #[cfg(any(windows, target_os = "macos"))]
        let connected = self.imp.as_mut().is_some_and(|imp| imp.poll(raw, &mut self.extra));
        #[cfg(not(any(windows, target_os = "macos")))]
        let connected = false;
        self.sync_sensors();
        connected
    }

    fn sync_sensors(&mut self) {
        #[cfg(any(windows, target_os = "macos"))]
        if let Some(imp) = self.imp.as_ref() {
            let players = imp.players();
            self.sensors.sync(Some(&players));
            return;
        }
        #[cfg(any(windows, target_os = "linux"))]
        {
            self.sensors.sync(None);
            return;
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        self.sensors.sync(Some(&[]));
    }

    /// Pads after the first, filled by the last [`Self::poll`].
    pub fn extra(&self) -> &[RawInput] {
        &self.extra
    }

    /// Names of connected pads, as `(name, "pad")`.
    pub fn device_names(&self) -> Vec<(String, String)> {
        #[cfg(any(windows, target_os = "macos"))]
        if let Some(imp) = self.imp.as_ref() {
            let names = imp.device_names();
            if !names.is_empty() {
                return names;
            }
        }
        self.sensors.label().map(|name| vec![(name, "pad".to_string())]).unwrap_or_default()
    }

    /// Run the motors at these strengths (0..1) until told otherwise.
    pub fn rumble(&mut self, strong: f32, weak: f32) {
        #[cfg(any(windows, target_os = "macos"))]
        if let Some(imp) = self.imp.as_mut() {
            imp.rumble(strong, weak);
        }
        let _ = (strong, weak);
    }
}

impl PadBackend for Pads {
    fn set_light_bar(&mut self, rgb: [f32; 3]) -> Result<(), String> {
        self.sensors.set_light(rgb)
    }

    fn motion(&self) -> PadMotion {
        self.sensors.motion()
    }

    fn light_note(&self) -> String {
        self.sensors.light_note()
    }
}

#[allow(dead_code)]
fn press(raw: &mut RawInput, name: &str) {
    if let Ok(b) = Button::parse(name) {
        raw.press(b);
    }
}

#[allow(dead_code)]
fn axis(raw: &mut RawInput, name: &str, value: f32) {
    if let Ok(a) = Axis::parse(name) {
        raw.set_axis(a, value);
    }
}

#[cfg(any(windows, target_os = "macos"))]
mod imp {
    use gilrs::ff::{BaseEffect, BaseEffectType, Effect, EffectBuilder, Replay, Ticks};
    use gilrs::{Axis as GAxis, Button as GButton, Gilrs};
    use seal_editor_core::seal_input::{PadId, RawInput};

    pub struct Pads {
        gilrs: Gilrs,
        effect: Option<Effect>,
        level: (u16, u16),
    }

    const MAP: &[(GButton, &str)] = &[
        (GButton::South, "pad.south"),
        (GButton::East, "pad.east"),
        (GButton::West, "pad.west"),
        (GButton::North, "pad.north"),
        (GButton::DPadUp, "pad.dpad_up"),
        (GButton::DPadDown, "pad.dpad_down"),
        (GButton::DPadLeft, "pad.dpad_left"),
        (GButton::DPadRight, "pad.dpad_right"),
        (GButton::LeftTrigger, "pad.lb"),
        (GButton::RightTrigger, "pad.rb"),
        (GButton::LeftTrigger2, "pad.lt"),
        (GButton::RightTrigger2, "pad.rt"),
        (GButton::Start, "pad.start"),
        (GButton::Select, "pad.select"),
        (GButton::LeftThumb, "pad.lstick"),
        (GButton::RightThumb, "pad.rstick"),
    ];

    impl Pads {
        pub fn new() -> Option<Self> {
            match Gilrs::new() {
                Ok(gilrs) => Some(Self { gilrs, effect: None, level: (0, 0) }),
                Err(e) => {
                    seal_log::warn!("gamepads are not available: {e}");
                    None
                }
            }
        }

        pub fn poll(&mut self, raw: &mut RawInput, extra: &mut Vec<RawInput>) -> bool {
            while self.gilrs.next_event().is_some() {}
            let mut connected = self.gilrs.gamepads().filter(|(_, pad)| pad.is_connected());
            let Some((_, pad)) = connected.next() else {
                return false;
            };
            Self::copy(&pad, raw);
            for (_, pad) in connected {
                let mut one = RawInput::default();
                Self::copy(&pad, &mut one);
                extra.push(one);
            }
            true
        }

        fn copy(pad: &gilrs::Gamepad<'_>, raw: &mut RawInput) {
            for (button, name) in MAP {
                if pad.is_pressed(*button) {
                    super::press(raw, name);
                }
            }
            // Y is up on sticks, as players expect.
            super::axis(raw, "pad.left_x", pad.value(GAxis::LeftStickX));
            super::axis(raw, "pad.left_y", pad.value(GAxis::LeftStickY));
            super::axis(raw, "pad.right_x", pad.value(GAxis::RightStickX));
            super::axis(raw, "pad.right_y", pad.value(GAxis::RightStickY));
            let trigger = |b| pad.button_data(b).map_or(0.0, |d| d.value());
            super::axis(raw, "pad.lt_value", trigger(GButton::LeftTrigger2));
            super::axis(raw, "pad.rt_value", trigger(GButton::RightTrigger2));
        }

        pub fn rumble(&mut self, strong: f32, weak: f32) {
            let q = |v: f32| (v.clamp(0.0, 1.0) * f32::from(u16::MAX)) as u16;
            let level = (q(strong), q(weak));
            if level == self.level {
                return;
            }
            self.level = level;
            if let Some(old) = self.effect.take() {
                let _ = old.stop();
            }
            if level == (0, 0) {
                return;
            }
            let ids: Vec<_> = self
                .gilrs
                .gamepads()
                .filter(|(_, p)| p.is_connected() && p.is_ff_supported())
                .map(|(id, _)| id)
                .collect();
            if ids.is_empty() {
                return;
            }
            // Long enough to outlast any frame gap; the next change stops it.
            let replay = Replay { play_for: Ticks::from_ms(10_000), ..Default::default() };
            let built = EffectBuilder::new()
                .add_effect(BaseEffect {
                    kind: BaseEffectType::Strong { magnitude: level.0 },
                    scheduling: replay,
                    ..Default::default()
                })
                .add_effect(BaseEffect {
                    kind: BaseEffectType::Weak { magnitude: level.1 },
                    scheduling: replay,
                    ..Default::default()
                })
                .gamepads(&ids)
                .finish(&mut self.gilrs);
            match built {
                Ok(effect) => {
                    let _ = effect.play();
                    self.effect = Some(effect);
                }
                Err(e) => seal_log::warn!("the gamepad could not rumble: {e}"),
            }
        }

        pub fn device_names(&self) -> Vec<(String, String)> {
            self.gilrs
                .gamepads()
                .filter(|(_, pad)| pad.is_connected())
                .map(|(_, pad)| (pad.name().to_string(), "pad".to_string()))
                .collect()
        }

        pub fn players(&self) -> Vec<PadId> {
            self.gilrs
                .gamepads()
                .filter(|(_, pad)| pad.is_connected())
                .map(|(_, pad)| match (pad.vendor_id(), pad.product_id()) {
                    (Some(vendor), Some(product)) => PadId { vendor, product },
                    _ => PadId { vendor: 0, product: 0 },
                })
                .collect()
        }
    }
}
