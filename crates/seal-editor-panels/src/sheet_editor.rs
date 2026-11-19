//! Extra sprite-sheet controls: island cuts, and one frame's pivot, border and outlines.

use seal_editor_core::sprite_frames::Frame;
use seal_editor_core::EditorSession;
use seal_ui::PanelFrame;

use crate::PanelEvent;

const MIN_DRAFT: &str = "asset-anim-island-min";
const PAD_DRAFT: &str = "asset-anim-island-pad";

/// Island cut, and the picked frame's pivot, border and outlines.
pub fn controls(
    frame: &mut PanelFrame,
    session: &EditorSession,
    clip: &str,
    image: &str,
    index: usize,
    shown: Option<&Frame>,
) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let mut event = PanelEvent::None;
    let mut number = |frame: &mut PanelFrame, label: &str, key: &str, fallback: f32| {
        let v: f32 = session.draft(key).and_then(|v| v.parse().ok()).unwrap_or(fallback);
        let mut value = v;
        let mut pick = None;
        let (changed, _, _) =
            frame.number_row(label, &mut value, 1.0, false, None, None, None, &mut pick);
        if changed {
            let n = value.round().clamp(0.0, 256.0);
            event = PanelEvent::Commands(vec![(
                "draft.set".into(),
                seal_editor_core::CommandArgs::new(&[("key", key), ("value", &n.to_string())]),
            )]);
        }
        v.round().clamp(0.0, 256.0) as u32
    };
    let min = number(frame, &text("asset-anim-island-min"), MIN_DRAFT, 1.0);
    let pad = number(frame, &text("asset-anim-island-pad"), PAD_DRAFT, 0.0);
    if frame.button(&text("asset-anim-islands")) {
        let (min, pad) = (min.to_string(), pad.to_string());
        event = crate::command(
            "asset.frames_slice",
            &[
                ("clip", clip),
                ("image", image),
                ("mode", "islands"),
                ("min", min.as_str()),
                ("pad", pad.as_str()),
            ],
        );
    }
    frame.hint(&text("asset-anim-islands-hint"));
    let Some(shown) = shown else { return event };
    let index_text = index.to_string();
    let base = [("clip", clip), ("index", index_text.as_str()), ("action", "set")];
    let mut pivot_x = shown.pivot[0];
    let mut pivot_y = shown.pivot[1];
    let mut pick = None;
    let (cx, _, _) = frame.number_row(
        &text("asset-anim-pivot-x"),
        &mut pivot_x,
        0.01,
        false,
        None,
        None,
        None,
        &mut pick,
    );
    let (cy, _, _) = frame.number_row(
        &text("asset-anim-pivot-y"),
        &mut pivot_y,
        0.01,
        false,
        None,
        None,
        None,
        &mut pick,
    );
    if cx || cy {
        pivot_x = pivot_x.clamp(0.0, 1.0);
        pivot_y = pivot_y.clamp(0.0, 1.0);
        let pivot = format!("{pivot_x},{pivot_y}");
        let mut args = base.to_vec();
        args.push(("pivot", pivot.as_str()));
        event = crate::command("asset.frames_frame", &args);
    }
    let mut border = shown.border;
    let names = [
        "asset-anim-border-left",
        "asset-anim-border-right",
        "asset-anim-border-top",
        "asset-anim-border-bottom",
    ];
    let mut border_changed = false;
    for (i, name) in names.iter().enumerate() {
        let (changed, _, _) =
            frame.number_row(&text(name), &mut border[i], 1.0, false, None, None, None, &mut pick);
        border_changed |= changed;
    }
    if border_changed {
        let text = format!(
            "{},{},{},{}",
            border[0].max(0.0),
            border[1].max(0.0),
            border[2].max(0.0),
            border[3].max(0.0)
        );
        let mut args = base.to_vec();
        args.push(("border", text.as_str()));
        event = crate::command("asset.frames_frame", &args);
    }
    frame.row(|frame| {
        if frame.button(&text("asset-anim-outline")) {
            event = crate::command(
                "asset.frames_frame",
                &[("clip", clip), ("index", index_text.as_str()), ("action", "outline")],
            );
        }
        if frame.button(&text("asset-anim-physics")) {
            event = crate::command(
                "asset.frames_frame",
                &[("clip", clip), ("index", index_text.as_str()), ("action", "physics")],
            );
        }
        if frame.button(&text("asset-anim-collider")) {
            event = crate::command("sprite.collider_from_sprite", &[]);
        }
    });
    event
}
