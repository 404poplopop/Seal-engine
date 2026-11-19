//! Split viewport layouts: pane cameras, splitter fractions, and pointer routing.
//!
//! The panel state file stores one `viewport_split` string. A damaged string
//! falls back to a single pane so a bad save cannot blank the viewport.

use seal_sealtext::SealValue;

use crate::command::CommandArgs;
use crate::error::CoreError;
use crate::models::{
    default_viewport_show_layers, viewport_layout_default_panes, viewport_load_active_pane,
    viewport_store_active_pane, EditorModels, ViewportPaneState, VIEWPORT_SHOW_LAYER_IDS,
};
use crate::session::EditorSession;

const MIN_RATIO: f32 = 0.15;
const MAX_RATIO: f32 = 0.85;

/// Flat orthographic view used for 2D scenes.
pub const FLAT_2D_VIEW: &str = "2d";

type CommandFn = fn(&mut EditorSession, &CommandArgs) -> Result<(), CoreError>;

pub const COMMANDS: &[(&str, &str, &str, CommandFn)] =
    &[("viewport.split", "command-viewport-split", "view", set_splits_command)];

/// One pane rectangle in panel pixels. Origin is the top-left of the viewport body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl PaneRect {
    pub fn contains(self, px: f32, py: f32) -> bool {
        px >= self.x && py >= self.y && px <= self.x + self.w && py <= self.y + self.h
    }
}

/// A draggable bar between panes. `vertical` bars move left and right.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplitBar {
    pub index: usize,
    pub vertical: bool,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

struct Cuts {
    panes: Vec<PaneRect>,
    bars: Vec<SplitBar>,
}

pub fn split_count(preset: &str) -> usize {
    match preset {
        "two_cols" | "two_rows" => 1,
        "three" | "four" => 2,
        _ => 0,
    }
}

pub fn default_splits(preset: &str) -> Vec<f32> {
    match preset {
        "two_cols" | "two_rows" => vec![0.5],
        "three" => vec![0.58, 0.5],
        "four" => vec![0.5, 0.5],
        _ => Vec::new(),
    }
}

/// Fractions actually used for `preset`, filled from `stored` and clamped.
pub fn ratios_for(preset: &str, stored: &[f32]) -> Vec<f32> {
    default_splits(preset)
        .into_iter()
        .enumerate()
        .map(|(index, fallback)| {
            stored.get(index).copied().unwrap_or(fallback).clamp(MIN_RATIO, MAX_RATIO)
        })
        .collect()
}

pub fn pane_rects(outer: PaneRect, preset: &str, splits: &[f32], gap: f32) -> Vec<PaneRect> {
    layout_cuts(outer, preset, splits, gap).panes
}

pub fn split_bars(outer: PaneRect, preset: &str, splits: &[f32], gap: f32) -> Vec<SplitBar> {
    layout_cuts(outer, preset, splits, gap).bars
}

/// Pane index under a pointer, or none when the point misses every pane (a gap or the outside).
pub fn pane_under_pointer(rects: &[PaneRect], x: f32, y: f32) -> Option<usize> {
    rects.iter().position(|rect| rect.contains(x, y))
}

fn layout_cuts(outer: PaneRect, preset: &str, splits: &[f32], gap: f32) -> Cuts {
    let gap = gap.max(2.0);
    if outer.w < 8.0 || outer.h < 8.0 || preset == "single" {
        return Cuts { panes: vec![outer], bars: Vec::new() };
    }
    let ratios = ratios_for(preset, splits);
    match preset {
        "two_cols" => {
            let (left, bar, right) = cut_x(outer, ratios[0], gap);
            Cuts { panes: vec![left, right], bars: vec![bar_of(0, true, bar)] }
        }
        "two_rows" => {
            let (top, bar, bottom) = cut_y(outer, ratios[0], gap);
            Cuts { panes: vec![top, bottom], bars: vec![bar_of(0, false, bar)] }
        }
        "three" => {
            let (left, bar_x, right) = cut_x(outer, ratios[0], gap);
            let (top, bar_y, bottom) = cut_y(right, ratios[1], gap);
            Cuts {
                panes: vec![left, top, bottom],
                bars: vec![bar_of(0, true, bar_x), bar_of(1, false, bar_y)],
            }
        }
        "four" => {
            let (top, bar_y, bottom) = cut_y(outer, ratios[1], gap);
            let (tl, bar_x, tr) = cut_x(top, ratios[0], gap);
            let (bl, _, br) = cut_x(bottom, ratios[0], gap);
            let vertical = PaneRect { x: bar_x.x, y: outer.y, w: bar_x.w, h: outer.h };
            Cuts {
                panes: vec![tl, tr, bl, br],
                bars: vec![bar_of(0, true, vertical), bar_of(1, false, bar_y)],
            }
        }
        _ => Cuts { panes: vec![outer], bars: Vec::new() },
    }
}

fn bar_of(index: usize, vertical: bool, rect: PaneRect) -> SplitBar {
    SplitBar { index, vertical, x: rect.x, y: rect.y, w: rect.w, h: rect.h }
}

fn cut_x(outer: PaneRect, ratio: f32, gap: f32) -> (PaneRect, PaneRect, PaneRect) {
    let inner = (outer.w - gap).max(1.0);
    let left_w = inner * ratio;
    let right_w = (inner - left_w).max(1.0);
    let left = PaneRect { x: outer.x, y: outer.y, w: left_w, h: outer.h };
    let bar = PaneRect { x: outer.x + left_w, y: outer.y, w: gap, h: outer.h };
    let right = PaneRect { x: outer.x + left_w + gap, y: outer.y, w: right_w, h: outer.h };
    (left, bar, right)
}

fn cut_y(outer: PaneRect, ratio: f32, gap: f32) -> (PaneRect, PaneRect, PaneRect) {
    let inner = (outer.h - gap).max(1.0);
    let top_h = inner * ratio;
    let bottom_h = (inner - top_h).max(1.0);
    let top = PaneRect { x: outer.x, y: outer.y, w: outer.w, h: top_h };
    let bar = PaneRect { x: outer.x, y: outer.y + top_h, w: outer.w, h: gap };
    let bottom = PaneRect { x: outer.x, y: outer.y + top_h + gap, w: outer.w, h: bottom_h };
    (top, bar, bottom)
}

pub fn normalize_preset(id: &str) -> Option<&'static str> {
    match id {
        "layout_single" | "single" => Some("single"),
        "layout_two_cols" | "two_cols" => Some("two_cols"),
        "layout_two_rows" | "two_rows" => Some("two_rows"),
        "layout_three" | "three" => Some("three"),
        "layout_four" | "four" => Some("four"),
        _ => None,
    }
}

/// New panes for `preset`. Pane 0 keeps the previous pane 0 camera.
pub fn layout_panes(
    previous: &[ViewportPaneState],
    preset: &str,
    flat_2d: bool,
) -> Vec<ViewportPaneState> {
    let mut panes = viewport_layout_default_panes(preset);
    if let (Some(src), Some(dst)) = (previous.first(), panes.first_mut()) {
        dst.pan_x = src.pan_x;
        dst.pan_y = src.pan_y;
        dst.zoom = src.zoom;
        dst.distance = src.distance;
        dst.shade = src.shade.clone();
        dst.show_layers = src.show_layers.clone();
        if flat_2d {
            dst.view = FLAT_2D_VIEW.into();
            dst.yaw = 0.0;
            dst.pitch = 0.0;
        } else {
            dst.view = src.view.clone();
            dst.yaw = src.yaw;
            dst.pitch = src.pitch;
        }
    }
    if flat_2d {
        for pane in &mut panes {
            pane.view = FLAT_2D_VIEW.into();
            pane.yaw = 0.0;
            pane.pitch = 0.0;
        }
    }
    panes
}

pub fn change_layout(session: &mut EditorSession, preset_id: &str) -> Result<(), CoreError> {
    let key =
        normalize_preset(preset_id).ok_or_else(|| CoreError::new("unknown viewport layout"))?;
    if session.models.viewport_layout_preset == "single" {
        session.models.viewport_active_pane = 0;
    }
    viewport_store_active_pane(session);
    let previous = session.models.viewport_panes.clone();
    let panes = layout_panes(&previous, key, session.scene_is_2d());
    session.models.viewport_layout_preset = key.to_string();
    session.models.viewport_panes = panes;
    session.models.viewport_active_pane = 0;
    session.models.viewport_pane_expanded = false;
    session.models.viewport_splits = default_splits(key);
    viewport_load_active_pane(session);
    Ok(())
}

pub fn set_pane_view(
    session: &mut EditorSession,
    index: usize,
    view: &str,
) -> Result<(), CoreError> {
    let flat = session.scene_is_2d() && view == "perspective";
    let pane = session
        .models
        .viewport_panes
        .get_mut(index)
        .ok_or_else(|| CoreError::new("viewport pane was missing"))?;
    if flat {
        pane.view = FLAT_2D_VIEW.into();
        pane.yaw = 0.0;
        pane.pitch = 0.0;
    } else {
        pane.view = view.to_string();
    }
    session.models.viewport_active_pane = index;
    viewport_load_active_pane(session);
    Ok(())
}

pub fn set_pane_shade(
    session: &mut EditorSession,
    index: usize,
    shade: &str,
) -> Result<(), CoreError> {
    if !crate::viewport_selection::shade_mode_allowed(shade) {
        return Err(CoreError::new(
            "the shade is lit, unlit, wireframe, collision, lighting only, or overdraw",
        ));
    }
    let pane = session
        .models
        .viewport_panes
        .get_mut(index)
        .ok_or_else(|| CoreError::new("viewport pane was missing"))?;
    pane.shade = shade.to_string();
    session.models.viewport_active_pane = index;
    viewport_load_active_pane(session);
    Ok(())
}

pub fn set_pane_show(
    session: &mut EditorSession,
    index: usize,
    layer: &str,
    on: bool,
) -> Result<(), CoreError> {
    if !VIEWPORT_SHOW_LAYER_IDS.contains(&layer) {
        return Err(CoreError::new("unknown viewport show layer"));
    }
    let pane = session
        .models
        .viewport_panes
        .get_mut(index)
        .ok_or_else(|| CoreError::new("viewport pane was missing"))?;
    if pane.show_layers.is_empty() {
        pane.show_layers = default_viewport_show_layers();
    }
    pane.show_layers.insert(layer.to_string(), on);
    session.models.viewport_active_pane = index;
    viewport_load_active_pane(session);
    Ok(())
}

pub fn set_pane_camera(
    session: &mut EditorSession,
    index: usize,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    if index >= session.models.viewport_panes.len() {
        return Err(CoreError::new("viewport pane was missing"));
    }
    {
        let pane = &mut session.models.viewport_panes[index];
        if let Some(text) = args.get("pan_x") {
            if let Ok(value) = text.parse() {
                pane.pan_x = value;
            }
        }
        if let Some(text) = args.get("pan_y") {
            if let Ok(value) = text.parse() {
                pane.pan_y = value;
            }
        }
        if let Some(text) = args.get("zoom") {
            if let Ok(value) = text.parse::<f32>() {
                pane.zoom = value.clamp(0.25, 6.0);
            }
        }
        if let Some(text) = args.get("yaw") {
            if let Ok(value) = text.parse() {
                pane.yaw = value;
            }
        }
        if let Some(text) = args.get("pitch") {
            if let Ok(value) = text.parse() {
                pane.pitch = value;
            }
        }
        if let Some(text) = args.get("distance") {
            if let Ok(value) = text.parse() {
                pane.distance = value;
            }
        }
    }
    session.models.viewport_active_pane = index;
    viewport_load_active_pane(session);
    Ok(())
}

fn set_splits_command(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let text =
        args.get("values").ok_or_else(|| CoreError::new("viewport split values were missing"))?;
    let preset = session.models.viewport_layout_preset.clone();
    let need = split_count(&preset);
    if need == 0 {
        return Err(CoreError::new("this viewport layout has no splitters"));
    }
    let parsed: Vec<f32> = text
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<f32>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| CoreError::new("viewport split value was not a number"))?;
    if parsed.len() != need {
        return Err(CoreError::new("viewport split count does not match the layout"));
    }
    if parsed.iter().any(|value| *value < MIN_RATIO || *value > MAX_RATIO) {
        return Err(CoreError::new("viewport split is out of range"));
    }
    session.models.viewport_splits = parsed;
    Ok(())
}

pub fn fallback_single(models: &mut EditorModels) {
    models.viewport_layout_preset = "single".into();
    models.viewport_panes = vec![ViewportPaneState::default()];
    models.viewport_active_pane = 0;
    models.viewport_pane_expanded = false;
    models.viewport_splits.clear();
}

/// SealText document for the open layout, splitter fractions, and every pane.
pub fn encode_split(session: &EditorSession) -> String {
    let mut panes = session.models.viewport_panes.clone();
    if panes.is_empty() {
        panes.push(ViewportPaneState::default());
    }
    let index = session.models.viewport_active_pane.min(panes.len() - 1);
    overlay_live_camera(session, &mut panes[index]);
    let value = SealValue::Map(
        vec![
            ("preset".into(), SealValue::String(session.models.viewport_layout_preset.clone())),
            ("expanded".into(), SealValue::Bool(session.models.viewport_pane_expanded)),
            ("active".into(), number(session.models.viewport_active_pane as f32)),
            (
                "splits".into(),
                SealValue::Array(
                    session.models.viewport_splits.iter().copied().map(number).collect(),
                ),
            ),
            ("panes".into(), SealValue::Array(panes.iter().map(encode_pane).collect())),
        ]
        .into(),
    );
    value.to_text()
}

fn overlay_live_camera(session: &EditorSession, pane: &mut ViewportPaneState) {
    pane.view = session.models.viewport_view.clone();
    pane.pan_x = session.models.viewport_pan_x;
    pane.pan_y = session.models.viewport_pan_y;
    pane.zoom = session.models.viewport_zoom;
    pane.shade = session.models.viewport_shade.clone();
    if !session.models.viewport_show_layers.is_empty() {
        pane.show_layers = session.models.viewport_show_layers.clone();
    }
    if let Some(value) = draft_f32(session, "viewport-yaw") {
        pane.yaw = value;
    }
    if let Some(value) = draft_f32(session, "viewport-pitch") {
        pane.pitch = value;
    }
    if let Some(value) = draft_f32(session, "viewport-distance") {
        pane.distance = value;
    }
}

fn draft_f32(session: &EditorSession, key: &str) -> Option<f32> {
    session.draft(key).and_then(|text| text.parse().ok())
}

fn encode_pane(pane: &ViewportPaneState) -> SealValue {
    let show = SealValue::Map(
        pane.show_layers
            .iter()
            .map(|(key, on)| (key.clone(), SealValue::Bool(*on)))
            .collect::<Vec<_>>()
            .into(),
    );
    SealValue::Map(
        vec![
            ("view".into(), SealValue::String(pane.view.clone())),
            ("pan_x".into(), number(pane.pan_x)),
            ("pan_y".into(), number(pane.pan_y)),
            ("zoom".into(), number(pane.zoom)),
            ("yaw".into(), number(pane.yaw)),
            ("pitch".into(), number(pane.pitch)),
            ("distance".into(), number(pane.distance)),
            ("shade".into(), SealValue::String(pane.shade.clone())),
            ("show".into(), show),
        ]
        .into(),
    )
}

fn number(value: f32) -> SealValue {
    SealValue::Number(value.to_string())
}

/// Apply a saved split document. Damaged input falls back to a single pane and returns false.
pub fn restore_split_text(session: &mut EditorSession, text: &str) -> bool {
    match decode_split(text) {
        Ok(state) => {
            session.models.viewport_layout_preset = state.preset;
            session.models.viewport_panes = state.panes;
            session.models.viewport_splits = state.splits;
            session.models.viewport_active_pane = state.active;
            session.models.viewport_pane_expanded = state.expanded;
            true
        }
        Err(()) => {
            fallback_single(&mut session.models);
            false
        }
    }
}

struct SplitFile {
    preset: String,
    panes: Vec<ViewportPaneState>,
    splits: Vec<f32>,
    active: usize,
    expanded: bool,
}

fn decode_split(text: &str) -> Result<SplitFile, ()> {
    let value = seal_sealtext::parse(text).map_err(|_| ())?;
    let preset = match value.get("preset") {
        Some(SealValue::String(text)) => normalize_preset(text).ok_or(())?.to_string(),
        _ => return Err(()),
    };
    let panes_value = value.get("panes").ok_or(())?;
    let SealValue::Array(items) = panes_value else {
        return Err(());
    };
    if items.is_empty() {
        return Err(());
    }
    let mut panes = Vec::new();
    for item in items {
        panes.push(decode_pane(item)?);
    }
    let expect = match preset.as_str() {
        "single" => 1,
        "two_cols" | "two_rows" => 2,
        "three" => 3,
        "four" => 4,
        _ => return Err(()),
    };
    if panes.len() != expect {
        return Err(());
    }
    let splits = match value.get("splits") {
        None => default_splits(&preset),
        Some(SealValue::Array(items)) => {
            if items.len() != split_count(&preset) {
                return Err(());
            }
            let mut values = Vec::new();
            for item in items {
                let SealValue::Number(text) = item else {
                    return Err(());
                };
                let value = text.parse::<f32>().map_err(|_| ())?;
                if !(MIN_RATIO..=MAX_RATIO).contains(&value) {
                    return Err(());
                }
                values.push(value);
            }
            values
        }
        Some(_) => return Err(()),
    };
    let active = match value.get("active") {
        None => 0,
        Some(SealValue::Number(text)) => text.parse::<usize>().map_err(|_| ())?,
        Some(_) => return Err(()),
    };
    if active >= panes.len() {
        return Err(());
    }
    let expanded = match value.get("expanded") {
        None => false,
        Some(SealValue::Bool(flag)) => *flag,
        Some(_) => return Err(()),
    };
    Ok(SplitFile { preset, panes, splits, active, expanded })
}

fn decode_pane(value: &SealValue) -> Result<ViewportPaneState, ()> {
    let view = match value.get("view") {
        Some(SealValue::String(text)) if allowed_view(text) => text.clone(),
        _ => return Err(()),
    };
    let pan_x = required_f32(value, "pan_x")?;
    let pan_y = required_f32(value, "pan_y")?;
    let zoom = required_f32(value, "zoom")?;
    let yaw = required_f32(value, "yaw")?;
    let pitch = required_f32(value, "pitch")?;
    let distance = required_f32(value, "distance")?;
    let shade = match value.get("shade") {
        None => "lit".to_string(),
        Some(SealValue::String(text))
            if matches!(text.as_str(), "lit" | "unlit" | "wireframe" | "collision") =>
        {
            text.clone()
        }
        Some(_) => return Err(()),
    };
    let show_layers = match value.get("show") {
        None => default_viewport_show_layers(),
        Some(SealValue::Map(map)) => {
            let mut layers = default_viewport_show_layers();
            for (key, item) in &map.entries {
                if !VIEWPORT_SHOW_LAYER_IDS.contains(&key.as_str()) {
                    continue;
                }
                let SealValue::Bool(on) = item else {
                    return Err(());
                };
                layers.insert(key.clone(), *on);
            }
            layers
        }
        Some(_) => return Err(()),
    };
    Ok(ViewportPaneState { view, pan_x, pan_y, zoom, yaw, pitch, distance, shade, show_layers })
}

fn allowed_view(view: &str) -> bool {
    matches!(
        view,
        "perspective" | "front" | "back" | "top" | "bottom" | "left" | "right" | "side" | "2d"
    )
}

fn required_f32(value: &SealValue, key: &str) -> Result<f32, ()> {
    match value.get(key) {
        Some(SealValue::Number(text)) => text.parse().map_err(|_| ()),
        _ => Err(()),
    }
}
