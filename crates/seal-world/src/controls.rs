//! Game controls during play: the project's input bindings, the player's own
//! changes to them, the pad's rumble, and "press a button" rebinding.
//!
//! The project keeps its bindings in `input.sealinput` (the default set when
//! there is none). A player's rebinds are saved apart, in
//! `saved/input-player.sealinput`, so the game's defaults stay as authored.

use std::collections::BTreeSet;
use std::path::Path;

use seal_input::{
    ActionState, Button, Capture, InputMap, RawInput, RebindCapture, Rumble, RumbleMixer,
};

use crate::PlayInput;

/// The project's bindings file, next to `project.seal`.
pub const INPUT_FILE: &str = "input.sealinput";
/// A player's own rebinds, laid over the project's bindings.
pub const PLAYER_FILE: &str = "saved/input-player.sealinput";

/// Read the project's bindings; the default set when the file is missing.
pub fn load_project_map(dir: &Path) -> Result<InputMap, String> {
    match std::fs::read_to_string(dir.join(INPUT_FILE)) {
        Ok(text) => InputMap::parse(&text).map_err(|e| format!("{INPUT_FILE}: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(InputMap::default_play()),
        Err(e) => Err(format!("{INPUT_FILE}: {e}")),
    }
}

/// Write the project's bindings, keeping a header that explains the format.
pub fn save_project_map(dir: &Path, map: &InputMap) -> std::io::Result<()> {
    let header: String = seal_input::DEFAULT_PLAY_MAP
        .lines()
        .take_while(|l| l.starts_with('#'))
        .map(|l| format!("{l}\n"))
        .collect();
    write_atomic(&dir.join(INPUT_FILE), &(header + &map.to_text()))
}

fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("sealinput.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

/// Everything input-related while a game runs.
#[derive(Clone, Debug, Default)]
pub struct Controls {
    /// The project's bindings as authored.
    pub base: InputMap,
    /// The bindings in use: `base` with the player's rebinds over it.
    pub map: InputMap,
    /// Device state the window fills in each frame.
    pub raw: RawInput,
    pub state: ActionState,
    pub rumble: RumbleMixer,
    /// Motor strengths (strong, weak) for the pad this frame.
    pub motors: (f32, f32),
    /// A rebinding waiting for the player's next button.
    pub capture: Option<RebindCapture>,
    /// Finished rebinds the scripts have not heard about yet.
    pub rebound: Vec<(String, Capture)>,
    /// Whether a gamepad is plugged in, as the window last saw.
    pub pad_connected: bool,
    /// Pad buttons already handed to the on-screen focus, so a hold moves once.
    pub ui_pads: BTreeSet<String>,
    project: Option<std::path::PathBuf>,
    /// Seconds of the frame the window is about to step. Hold and double tap use it.
    pub frame_dt: f32,
    /// Pointer movement this frame, in screen pixels. This is the cursor.
    pub mouse_delta: [f32; 2],
    /// Unaccelerated pointer motion this frame. Zero when it was not reported.
    pub raw_mouse: [f32; 2],
    /// Whether `raw_mouse` came from the device this frame.
    pub raw_mouse_known: bool,
    /// Motion mouse look should use this frame.
    pub look_delta: [f32; 2],
    /// Pen pressure, 0 to 1.
    pub pen_pressure: f32,
    /// Pen tilt, each axis -1 to 1.
    pub pen_tilt: [f32; 2],
    /// Whether the eraser end is down.
    pub pen_eraser: bool,
    /// Empty when pen tilt was reported, or when no pen is down.
    pub pen_note: String,
    /// Gamepad rotation speed, radians per second.
    pub pad_gyro: [f32; 3],
    /// Gamepad acceleration, in gravities.
    pub pad_accel: [f32; 3],
    /// Why motion sensors are missing. Empty when they were read.
    pub pad_motion_note: String,
    /// Why the light bar cannot be set. Empty when it can.
    pub pad_light_note: String,
    /// Mouse wheel this frame. Positive is away from the user.
    pub mouse_wheel: f32,
    /// Pointer position in the flat scene, in scene units.
    pub pointer_world: [f32; 2],
    /// Gamepads after the first. The first pad stays in `raw`.
    pub extra_pads: Vec<RawInput>,
    /// Which pads had Start down last frame, so a press is seen once.
    start_down: Vec<bool>,
}

impl Controls {
    /// Bindings for a play of the project in `dir` (defaults without one).
    /// Problems are written to `log` and the defaults are used instead.
    pub fn load(dir: Option<&Path>, log: &mut Vec<String>) -> Self {
        let base = match dir.map(load_project_map) {
            Some(Ok(map)) => map,
            Some(Err(e)) => {
                log.push(format!("input bindings: {e}; using the default controls"));
                InputMap::default_play()
            }
            None => InputMap::default_play(),
        };
        let mut map = base.clone();
        if let Some(dir) = dir {
            if let Ok(text) = std::fs::read_to_string(dir.join(PLAYER_FILE)) {
                match InputMap::parse(&text) {
                    Ok(changes) => map = base.with_changes(&changes),
                    Err(e) => {
                        log.push(format!("{PLAYER_FILE}: {e}; the player's rebinds were skipped"))
                    }
                }
            }
        }
        Self { base, map, project: dir.map(Path::to_path_buf), ..Self::default() }
    }

    /// Read this frame's device state: actions, any rebinding wait, and the
    /// side-view moves derived from the bindings.
    pub fn frame(&mut self) -> PlayInput {
        if let Some(cap) = self.capture.as_mut() {
            if let Some(result) = cap.poll(&self.raw) {
                let (action, slot) = (cap.action.clone(), cap.slot);
                self.capture = None;
                if let Capture::Bound(button) = &result {
                    let _ = self.map.rebind(&action, slot, button.clone());
                    self.save_player();
                }
                self.rebound.push((action, result));
            }
            // The game hears nothing while it waits for a button.
            self.state.update_for(&self.map, &RawInput::default(), self.frame_dt);
            return PlayInput::default();
        }
        self.state.update_for(&self.map, &self.raw, self.frame_dt);
        let x = self.state.axis("move_x");
        PlayInput {
            left: self.state.pressed("move_left") || x < -0.5,
            right: self.state.pressed("move_right") || x > 0.5,
            jump: self.state.pressed("jump"),
        }
    }

    /// Start waiting for a button for `action`. Unknown actions are refused.
    pub fn start_rebind(&mut self, action: &str, slot: usize) -> Result<(), String> {
        if !self.map.has_action(action) {
            return Err(format!("rebind: there is no input action named {action:?}"));
        }
        self.capture = Some(RebindCapture::start(action, slot, &self.raw));
        Ok(())
    }

    pub fn play_rumble(&mut self, strong: f32, weak: f32, seconds: f32) {
        self.rumble.play(Rumble::new(strong, weak, seconds));
    }

    /// Pads whose Start button went down this frame. Pad 0 is `raw`; later pads are `extra_pads`.
    pub fn take_start_edges(&mut self) -> Vec<u32> {
        let start = Button::parse("pad.start").expect("pad.start is a button");
        let mut down = vec![self.raw.is_down(&start)];
        for pad in &self.extra_pads {
            down.push(pad.is_down(&start));
        }
        let mut edges = Vec::new();
        for (index, on) in down.iter().copied().enumerate() {
            let was = self.start_down.get(index).copied().unwrap_or(false);
            if on && !was {
                edges.push(index as u32);
            }
        }
        self.start_down = down;
        edges
    }

    /// Advance rumble time.
    pub fn step(&mut self, dt: f32) {
        self.motors = self.rumble.step(dt);
    }

    /// Save the player's rebinds (only what differs from the project's).
    fn save_player(&self) {
        let Some(dir) = self.project.as_deref() else { return };
        let changes = self.map.changes_from(&self.base);
        let _ = write_atomic(&dir.join(PLAYER_FILE), &changes.to_text());
    }

    /// Mark keys from the window: `names` are egui key names held or pressed
    /// this frame. Other devices keep their state.
    pub fn set_keys<'a>(&mut self, names: impl IntoIterator<Item = &'a str>) {
        self.raw.clear_device("key");
        for name in names {
            if let Ok(b) = Button::parse(&format!("key.{}", name.to_ascii_lowercase())) {
                self.raw.press(b);
            }
        }
    }
}
