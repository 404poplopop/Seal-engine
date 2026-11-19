//! The input bindings editor: the project's `input.sealinput`, shown in the
//! Asset Editor as actions with their keys and pad buttons.
//!
//! Every change writes the file at once. Rebinding waits for the next key or
//! pad button; the slot being captured is kept in a draft, so the panel and the
//! window (which owns the gamepads) agree on it.

use std::path::{Path, PathBuf};

pub use seal_world::controls::INPUT_FILE;
pub use seal_world::seal_input::{
    button_label, Axis, AxisSource, Button, InputMap, RawInput, PAD_BUTTONS,
};

use crate::asset_editor::{open_asset, show_asset};
use crate::{CommandArgs, CoreError, EditorSession};

/// Draft holding `action<TAB>slot` while a binding waits for a key or button.
pub const INPUT_CAPTURE_DRAFT: &str = "input-capture";
/// Draft holding the last note shown above the bindings ("jump lost Space").
pub const INPUT_NOTE_DRAFT: &str = "input-note";
/// File extension of bindings files.
pub const INPUT_EXT: &str = "sealinput";

fn err(e: impl Into<String>) -> CoreError {
    CoreError::new(e.into())
}

/// The slot waiting for a key, if any.
pub fn capturing(session: &EditorSession) -> Option<(String, usize)> {
    let raw = session.draft(INPUT_CAPTURE_DRAFT)?;
    let (action, slot) = raw.split_once('\t')?;
    Some((action.to_string(), slot.parse().ok()?))
}

/// Read a bindings file; text that does not parse is an error, a missing file
/// is the default set.
pub fn read_map(full: &Path) -> Result<InputMap, String> {
    match std::fs::read_to_string(full) {
        Ok(text) => InputMap::parse(&text).map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(InputMap::default_play()),
        Err(e) => Err(e.to_string()),
    }
}

fn write_map(full: &Path, map: &InputMap) -> Result<(), CoreError> {
    let header: String = seal_world::seal_input::DEFAULT_PLAY_MAP
        .lines()
        .take_while(|l| l.starts_with('#'))
        .map(|l| format!("{l}\n"))
        .collect();
    crate::persist::atomic_write(full, (header + &map.to_text()).as_bytes())?;
    Ok(())
}

fn open_map(session: &EditorSession) -> Result<(PathBuf, InputMap), CoreError> {
    let (rel, full) =
        open_asset(session).ok_or_else(|| err("no input bindings are open in the Asset Editor"))?;
    if !rel.to_ascii_lowercase().ends_with(&format!(".{INPUT_EXT}")) {
        return Err(err("the open file is not an input bindings file"));
    }
    let map = read_map(&full).map_err(|e| err(format!("{rel}: {e}")))?;
    Ok((full, map))
}

fn arg<'a>(args: &'a CommandArgs, key: &str) -> Result<&'a str, CoreError> {
    args.get(key).filter(|v| !v.is_empty()).ok_or_else(|| err(format!("missing {key}=")))
}

fn slot_arg(args: &CommandArgs) -> Result<usize, CoreError> {
    let raw = arg(args, "slot")?;
    raw.parse().map_err(|_| err(format!("slot must be 0 or more, not {raw:?}")))
}

/// `project.input`: open the project's bindings, writing the default set first
/// when the project has none yet.
pub(crate) fn cmd_project_input(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    let root = session
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .ok_or_else(|| err("open a project first"))?;
    let full = root.join(INPUT_FILE);
    if !full.exists() {
        write_map(&full, &InputMap::default_play())?;
    }
    show_asset(session, INPUT_FILE)
}

/// `asset.input_capture action= slot=`: wait for the next key or pad button for
/// that slot (a slot past the end adds a binding).
pub(crate) fn cmd_input_capture(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let (_, map) = open_map(session)?;
    let action = arg(args, "action")?;
    if !map.has_action(action) {
        return Err(err(format!("there is no action named {action:?}")));
    }
    let slot = slot_arg(args)?;
    session.set_draft(INPUT_CAPTURE_DRAFT, &format!("{action}\t{slot}"))?;
    session.set_draft(INPUT_NOTE_DRAFT, "")?;
    Ok(())
}

/// `asset.input_cancel`: stop waiting for a key.
pub(crate) fn cmd_input_cancel(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    session.set_draft(INPUT_CAPTURE_DRAFT, "")?;
    Ok(())
}

/// `asset.input_bind button=` (with `action= slot=`, or the slot being
/// captured): put a key or pad button on an action. A button drives one action,
/// so another action holding it loses it; the note says which.
pub(crate) fn cmd_input_bind(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let (full, mut map) = open_map(session)?;
    let (action, slot) = match (args.get("action").filter(|a| !a.is_empty()), args.get("slot")) {
        (Some(action), Some(_)) => (action.to_string(), slot_arg(args)?),
        _ => capturing(session).ok_or_else(|| err("no binding is waiting for a key"))?,
    };
    let button = Button::parse(arg(args, "button")?).map_err(|e| err(e.to_string()))?;
    let label = button_label(&button);
    let taken = map.rebind(&action, slot, button).map_err(|e| err(e.to_string()))?;
    write_map(&full, &map)?;
    session.set_draft(INPUT_CAPTURE_DRAFT, "")?;
    let note = match taken {
        Some(other) => format!("{label} moved from {other} to {action}"),
        None => format!("{action} is now {label}"),
    };
    session.set_draft(INPUT_NOTE_DRAFT, &note)?;
    Ok(())
}

/// `asset.input_unbind action= slot=`: remove one binding.
pub(crate) fn cmd_input_unbind(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let (full, mut map) = open_map(session)?;
    let action = arg(args, "action")?;
    map.unbind(action, slot_arg(args)?);
    write_map(&full, &map)
}

/// `asset.input_add name=`: a new action with no buttons yet.
pub(crate) fn cmd_input_add(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let (full, mut map) = open_map(session)?;
    let name = arg(args, "name")?.trim().to_string();
    if map.has_action(&name) {
        return Err(err(format!("there is already an action named {name:?}")));
    }
    map.add_action(&name)
        .map_err(|_| err(format!("action names use letters, digits and _, not {name:?}")))?;
    write_map(&full, &map)
}

/// `asset.input_remove action=`: delete an action and its bindings.
pub(crate) fn cmd_input_remove(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let (full, mut map) = open_map(session)?;
    let action = arg(args, "action")?;
    if !map.remove_action(action) {
        return Err(err(format!("there is no action named {action:?}")));
    }
    if capturing(session).is_some_and(|(a, _)| a == action) {
        session.set_draft(INPUT_CAPTURE_DRAFT, "")?;
    }
    write_map(&full, &map)
}

/// `asset.input_deadzone axis= value=`: how far a stick moves before it counts.
pub(crate) fn cmd_input_deadzone(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let (full, mut map) = open_map(session)?;
    let axis = arg(args, "axis")?;
    let raw = arg(args, "value")?;
    let value: f32 =
        raw.parse().map_err(|_| err(format!("dead zone must be a number, not {raw:?}")))?;
    if !map.set_deadzone(axis, value) {
        return Err(err(format!("{axis} has no stick or trigger to set a dead zone on")));
    }
    write_map(&full, &map)
}

/// `asset.input_modifier action= slot= kind= seconds=`.
/// `kind` is `none`, `hold`, `double` or `toggle`. `seconds` is the hold or the double-tap window.
pub(crate) fn cmd_input_modifier(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    use seal_world::seal_input::BindMod;
    let (full, mut map) = open_map(session)?;
    let action = arg(args, "action")?;
    let slot = slot_arg(args)?;
    let kind = arg(args, "kind")?;
    let raw = args.get("seconds").unwrap_or("0.25");
    let seconds: f32 =
        raw.parse().map_err(|_| err(format!("seconds must be a number, not {raw:?}")))?;
    if !seconds.is_finite() || seconds < 0.0 {
        return Err(err("seconds must be zero or more"));
    }
    let modifier = match kind {
        "none" => BindMod::None,
        "hold" => BindMod::Hold(seconds),
        "double" => BindMod::Double(seconds),
        "toggle" => BindMod::Toggle,
        _ => {
            return Err(err(format!("modifier must be none, hold, double or toggle, not {kind:?}")))
        }
    };
    if !map.set_modifier(action, slot, modifier) {
        return Err(err(format!("{action} has no binding in slot {slot}")));
    }
    write_map(&full, &map)
}

/// `asset.input_bind_axis name= source=`: bind a device axis to a named axis.
pub(crate) fn cmd_input_bind_axis(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let (full, mut map) = open_map(session)?;
    let name = arg(args, "name")?.trim();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(err(format!("axis names use letters, digits and _, not {name:?}")));
    }
    let source = Axis::parse(arg(args, "source")?).map_err(|e| err(e.to_string()))?;
    let already = map.axes().any(|(axis, sources)| {
        axis == name
            && sources.iter().any(|item| match item {
                AxisSource::Analog { axis, .. } => axis == &source,
                AxisSource::Buttons { .. } => false,
            })
    });
    if !already {
        map.bind_axis(name, AxisSource::Analog { axis: source, deadzone: 0.0, scale: 1.0 });
        write_map(&full, &map)?;
    }
    Ok(())
}

/// Words for an axis source, such as "Left Stick X, dead zone 0.25".
pub fn axis_source_label(source: &AxisSource) -> String {
    match source {
        AxisSource::Buttons { negative, positive } => {
            format!("{} / {}", button_label(negative), button_label(positive))
        }
        AxisSource::Analog { axis, deadzone, scale } => {
            let mut out = axis_label(axis.name());
            if *scale != 1.0 {
                out.push_str(&format!(" x{scale}"));
            }
            let _ = deadzone;
            out
        }
    }
}

fn axis_label(name: &str) -> String {
    match name {
        "pad.left_x" => "Left Stick X".into(),
        "pad.left_y" => "Left Stick Y".into(),
        "pad.right_x" => "Right Stick X".into(),
        "pad.right_y" => "Right Stick Y".into(),
        "pad.lt_value" => "Left Trigger".into(),
        "pad.rt_value" => "Right Trigger".into(),
        "mouse.dx" => "Mouse X".into(),
        "mouse.dy" => "Mouse Y".into(),
        "mouse.raw_dx" => "Raw Mouse X".into(),
        "mouse.raw_dy" => "Raw Mouse Y".into(),
        "pen.pressure" => "Pen Pressure".into(),
        "pen.tilt_x" => "Pen Tilt X".into(),
        "pen.tilt_y" => "Pen Tilt Y".into(),
        "pad.gyro_x" => "Gyro X".into(),
        "pad.gyro_y" => "Gyro Y".into(),
        "pad.gyro_z" => "Gyro Z".into(),
        "pad.accel_x" => "Accelerometer X".into(),
        "pad.accel_y" => "Accelerometer Y".into(),
        "pad.accel_z" => "Accelerometer Z".into(),
        other => other.into(),
    }
}
