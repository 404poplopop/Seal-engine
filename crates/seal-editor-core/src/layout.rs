use seal_sealtext::{parse, SealValue};

use crate::error::CoreError;

/// Horizontal splits put `first` on the left. Vertical splits put `first` on top.
/// `ratio` is the fraction of the parent occupied by `first`, in 0..=1.
#[derive(Clone, Debug, PartialEq)]
pub enum DockNode {
    Tabs { active: usize, panels: Vec<String> },
    Split { axis: Axis, ratio: f32, first: Box<DockNode>, second: Box<DockNode> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FloatingWindow {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub monitor: Option<String>,
    pub node: DockNode,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub root: DockNode,
    pub floating: Vec<FloatingWindow>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowPlacement {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub maximized: bool,
    pub monitor: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MonitorInfo {
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Converts a monitor rectangle from physical pixels into egui points.
pub fn monitor_info_from_physical(
    name: String,
    x_px: f32,
    y_px: f32,
    width_px: f32,
    height_px: f32,
    pixels_per_point: f32,
) -> MonitorInfo {
    let scale = pixels_per_point.max(0.1);
    MonitorInfo {
        name,
        x: x_px / scale,
        y: y_px / scale,
        width: width_px / scale,
        height: height_px / scale,
    }
}

impl DockNode {
    pub fn tabs(panels: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self::Tabs { active: 0, panels: panels.into_iter().map(Into::into).collect() }
    }

    pub fn without_panel(&mut self, id: &str) {
        match self {
            DockNode::Tabs { active, panels } => {
                panels.retain(|panel| panel != id);
                if panels.is_empty() {
                    panels.push("viewport".into());
                }
                *active = (*active).min(panels.len() - 1);
            }
            DockNode::Split { first, second, .. } => {
                first.without_panel(id);
                second.without_panel(id);
            }
        }
    }
}

impl Layout {
    /// Built-in layouts. `default` is the full workspace, `debug` gives the
    /// console more room, and `minimal` is the viewport alone.
    pub fn builtin(name: &str) -> Option<Self> {
        match name {
            "default" => Some(Self::default()),
            "debug" => Some(Self::debug()),
            "minimal" => Some(Self::minimal()),
            _ => None,
        }
    }

    pub fn debug() -> Self {
        Self {
            root: DockNode::Split {
                axis: Axis::Vertical,
                ratio: 0.55,
                first: Box::new(DockNode::Split {
                    axis: Axis::Horizontal,
                    ratio: 0.22,
                    first: Box::new(DockNode::tabs(["outliner"])),
                    second: Box::new(DockNode::tabs(["viewport", "inspector"])),
                }),
                second: Box::new(DockNode::tabs(["console", "content"])),
            },
            floating: Vec::new(),
        }
    }

    pub fn minimal() -> Self {
        Self { root: DockNode::tabs(["viewport"]), floating: Vec::new() }
    }

    pub fn without_panel(&mut self, id: &str) {
        self.root.without_panel(id);
        for window in &mut self.floating {
            window.node.without_panel(id);
        }
    }
}

/// Panels that must not appear in the default dock tab sets.
pub const DEFAULT_LAYOUT_EXCLUDED: &[&str] = &[
    "welcome",
    "recovery",
    "notices",
    "layouts",
    "keymap",
    "dev",
    "wizard.project",
    "tutorial",
    "recipes",
    "commands",
    "stats",
    "graph",
    "assistant",
    "timeline",
    "asset",
    "undo-history",
    "test-runner",
];

/// Collapsed left-rail panels (Create) for the default workspace.
pub const DEFAULT_SIDEBAR_PINS: &[&str] = &[];

/// Viewport content width below this uses the narrow layout.
pub const NARROW_LAYOUT_MAX_WIDTH: f32 = 1400.0;
pub const NARROW_RIGHT_COLUMN_PX: f32 = 300.0;

const BOTTOM_STRIP_PANELS: &[&str] = &["content", "console"];

impl Default for Layout {
    fn default() -> Self {
        Self::m01_default()
    }
}

impl Layout {
    /// Default: viewport ~66% width and ~62% height, right stack 22%, bottom content+log.
    pub fn m01_default() -> Self {
        Self {
            root: DockNode::Split {
                axis: Axis::Horizontal,
                ratio: 0.14,
                // Create panel is a normal docked tab on the left: drag, tab, float or close it.
                first: Box::new(DockNode::tabs(["place"])),
                second: Box::new(Self::m01_main()),
            },
            floating: Vec::new(),
        }
    }

    fn m01_main() -> DockNode {
        Self {
            root: DockNode::Split {
                axis: Axis::Horizontal,
                ratio: 0.74,
                first: Box::new(DockNode::Split {
                    axis: Axis::Vertical,
                    ratio: 0.62,
                    first: Box::new(DockNode::tabs(["viewport"])),
                    second: Box::new(DockNode::tabs(["content", "console"])),
                }),
                second: Box::new(DockNode::Split {
                    axis: Axis::Vertical,
                    ratio: 0.35,
                    first: Box::new(DockNode::tabs(["outliner"])),
                    second: Box::new(DockNode::tabs(["inspector", "world"])),
                }),
            },
            floating: Vec::new(),
        }
        .root
    }

    pub fn panel_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        collect_panel_ids(&self.root, &mut ids);
        for window in &self.floating {
            collect_panel_ids(&window.node, &mut ids);
        }
        ids
    }

    pub fn default_excludes_present(&self) -> Vec<String> {
        self.panel_ids()
            .into_iter()
            .filter(|id| DEFAULT_LAYOUT_EXCLUDED.iter().any(|bad| bad == id))
            .collect()
    }

    pub fn has_bottom_strip(&self) -> bool {
        self.panel_ids().iter().any(|id| BOTTOM_STRIP_PANELS.iter().any(|panel| panel == id))
    }

    /// Narrow: viewport plus right column only; content and log live in status drawers.
    pub fn m02_narrow_for_width(content_width: f32) -> Self {
        let width = content_width.max(640.0);
        let right = NARROW_RIGHT_COLUMN_PX.min(width * 0.35);
        let ratio = ((width - right) / width).clamp(0.55, 0.9);
        Self {
            root: DockNode::Split {
                axis: Axis::Horizontal,
                ratio,
                first: Box::new(DockNode::tabs(["viewport"])),
                second: Box::new(DockNode::Split {
                    axis: Axis::Vertical,
                    ratio: 0.35,
                    first: Box::new(DockNode::tabs(["outliner"])),
                    second: Box::new(DockNode::tabs(["inspector", "world"])),
                }),
            },
            floating: Vec::new(),
        }
    }

    pub fn narrow_right_ratio(content_width: f32) -> f32 {
        let width = content_width.max(640.0);
        let right = NARROW_RIGHT_COLUMN_PX.min(width * 0.35);
        ((width - right) / width).clamp(0.55, 0.9)
    }
}

fn collect_panel_ids(node: &DockNode, out: &mut Vec<String>) {
    match node {
        DockNode::Tabs { panels, .. } => out.extend(panels.iter().cloned()),
        DockNode::Split { first, second, .. } => {
            collect_panel_ids(first, out);
            collect_panel_ids(second, out);
        }
    }
}

impl Default for WindowPlacement {
    fn default() -> Self {
        Self { x: 80.0, y: 80.0, width: 1280.0, height: 800.0, maximized: false, monitor: None }
    }
}

impl Layout {
    pub fn to_value(&self) -> SealValue {
        SealValue::Map(
            (vec![
                ("root".into(), node_to_value(&self.root)),
                (
                    "floating".into(),
                    SealValue::Array(self.floating.iter().map(floating_to_value).collect()),
                ),
            ])
            .into(),
        )
    }

    pub fn from_value(value: &SealValue) -> Result<Self, CoreError> {
        let map = value.as_map().ok_or_else(|| CoreError::new("layout must be a map"))?;
        let root = map
            .iter()
            .find(|(key, _)| key == "root")
            .map(|(_, value)| node_from_value(value))
            .transpose()?
            .ok_or_else(|| CoreError::new("layout is missing root"))?;
        let floating = match map.iter().find(|(key, _)| key == "floating") {
            Some((_, SealValue::Array(items))) => {
                items.iter().map(floating_from_value).collect::<Result<Vec<_>, _>>()?
            }
            Some(_) => return Err(CoreError::new("floating windows must be an array")),
            None => Vec::new(),
        };
        Ok(Self { root, floating })
    }
}

impl WindowPlacement {
    pub fn to_value(&self) -> SealValue {
        SealValue::Map(
            (vec![
                ("x".into(), number(self.x)),
                ("y".into(), number(self.y)),
                ("width".into(), number(self.width)),
                ("height".into(), number(self.height)),
                ("maximized".into(), SealValue::Bool(self.maximized)),
                (
                    "monitor".into(),
                    match &self.monitor {
                        Some(name) => SealValue::String(name.clone()),
                        None => SealValue::Null,
                    },
                ),
            ])
            .into(),
        )
    }

    pub fn from_value(value: &SealValue) -> Result<Self, CoreError> {
        let map = value.as_map().ok_or_else(|| CoreError::new("window placement must be a map"))?;
        Ok(Self {
            x: required_f32(map, "x")?,
            y: required_f32(map, "y")?,
            width: required_f32(map, "width")?,
            height: required_f32(map, "height")?,
            maximized: match find(map, "maximized") {
                Some(SealValue::Bool(value)) => *value,
                _ => false,
            },
            monitor: match find(map, "monitor") {
                Some(SealValue::String(name)) => Some(name.clone()),
                _ => None,
            },
        })
    }
}

/// Load monitor geometry saved as a SealText map of monitor records.
#[cfg_attr(not(test), allow(dead_code))]
pub fn monitors_from_text(text: &str) -> Result<Vec<MonitorInfo>, CoreError> {
    let value = parse(text)?;
    let map = value.as_map().ok_or_else(|| CoreError::new("monitors must be a map"))?;
    map.iter().map(|(_, entry)| monitor_from_value(entry)).collect()
}

/// Move a saved placement fully onto a known monitor, falling back to the first
/// monitor when the saved name is missing or the window is not on any monitor.
pub fn restore_window(placement: WindowPlacement, monitors: &[MonitorInfo]) -> WindowPlacement {
    if monitors.is_empty() {
        return placement;
    }
    let primary = &monitors[0];
    if let Some(name) = &placement.monitor {
        if let Some(monitor) = monitors.iter().find(|monitor| &monitor.name == name) {
            return clamp_into(&placement, monitor);
        }
    } else if let Some(monitor) = monitors.iter().find(|monitor| {
        intersects(monitor, placement.x, placement.y, placement.width, placement.height)
    }) {
        let mut placed = placement;
        placed.monitor = Some(monitor.name.clone());
        return clamp_into(&placed, monitor);
    }
    let mut placed = placement;
    placed.monitor = Some(primary.name.clone());
    placed.x = primary.x + 80.0;
    placed.y = primary.y + 80.0;
    clamp_into(&placed, primary)
}

/// Place a saved window on a monitor that still exists.
/// An unknown monitor name falls back to the first monitor, inset from its origin.
pub fn resolve_window(saved: &WindowPlacement, monitors: &[MonitorInfo]) -> WindowPlacement {
    restore_window(saved.clone(), monitors)
}

fn clamp_into(saved: &WindowPlacement, monitor: &MonitorInfo) -> WindowPlacement {
    let mut placed = saved.clone();
    placed.monitor = Some(monitor.name.clone());
    let width = saved.width.clamp(320.0, monitor.width.max(320.0));
    let height = saved.height.clamp(240.0, monitor.height.max(240.0));
    let max_x = monitor.x + monitor.width - 80.0;
    let max_y = monitor.y + monitor.height - 80.0;
    placed.width = width;
    placed.height = height;
    placed.x = saved.x.clamp(monitor.x, max_x.max(monitor.x));
    placed.y = saved.y.clamp(monitor.y, max_y.max(monitor.y));
    placed
}

fn intersects(monitor: &MonitorInfo, x: f32, y: f32, width: f32, height: f32) -> bool {
    let right = x + width;
    let bottom = y + height;
    let mx2 = monitor.x + monitor.width;
    let my2 = monitor.y + monitor.height;
    x < mx2 && right > monitor.x && y < my2 && bottom > monitor.y
}

fn node_to_value(node: &DockNode) -> SealValue {
    match node {
        DockNode::Tabs { active, panels } => SealValue::Map(
            (vec![
                ("kind".into(), SealValue::String("tabs".into())),
                ("active".into(), SealValue::Number(active.to_string())),
                (
                    "panels".into(),
                    SealValue::Array(
                        panels.iter().map(|panel| SealValue::String(panel.clone())).collect(),
                    ),
                ),
            ])
            .into(),
        ),
        DockNode::Split { axis, ratio, first, second } => SealValue::Map(
            (vec![
                ("kind".into(), SealValue::String("split".into())),
                (
                    "axis".into(),
                    SealValue::String(
                        match axis {
                            Axis::Horizontal => "horizontal",
                            Axis::Vertical => "vertical",
                        }
                        .into(),
                    ),
                ),
                ("ratio".into(), number(*ratio)),
                ("first".into(), node_to_value(first)),
                ("second".into(), node_to_value(second)),
            ])
            .into(),
        ),
    }
}

fn node_from_value(value: &SealValue) -> Result<DockNode, CoreError> {
    let map = value.as_map().ok_or_else(|| CoreError::new("dock node must be a map"))?;
    let kind = match find(map, "kind") {
        Some(SealValue::String(kind)) => kind.as_str(),
        _ => return Err(CoreError::new("dock node is missing kind")),
    };
    match kind {
        "tabs" => {
            let panels = match find(map, "panels") {
                Some(SealValue::Array(items)) => items
                    .iter()
                    .map(|item| match item {
                        SealValue::String(panel) => Ok(panel.clone()),
                        _ => Err(CoreError::new("panel id must be a string")),
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                _ => return Err(CoreError::new("tabs node is missing panels")),
            };
            let active = match find(map, "active") {
                Some(SealValue::Number(lexeme)) => lexeme.parse::<usize>().unwrap_or(0),
                _ => 0,
            };
            let active = if panels.is_empty() { 0 } else { active.min(panels.len() - 1) };
            Ok(DockNode::Tabs { active, panels })
        }
        "split" => {
            let axis = match find(map, "axis") {
                Some(SealValue::String(axis)) if axis == "horizontal" => Axis::Horizontal,
                Some(SealValue::String(axis)) if axis == "vertical" => Axis::Vertical,
                _ => return Err(CoreError::new("split node has no axis")),
            };
            let ratio = required_f32(map, "ratio")?.clamp(0.05, 0.95);
            let first = node_from_value(
                find(map, "first").ok_or_else(|| CoreError::new("split node is missing first"))?,
            )?;
            let second = node_from_value(
                find(map, "second")
                    .ok_or_else(|| CoreError::new("split node is missing second"))?,
            )?;
            Ok(DockNode::Split { axis, ratio, first: Box::new(first), second: Box::new(second) })
        }
        other => Err(CoreError::new(format!("unknown dock node kind '{other}'"))),
    }
}

fn floating_to_value(window: &FloatingWindow) -> SealValue {
    SealValue::Map(
        (vec![
            ("x".into(), number(window.x)),
            ("y".into(), number(window.y)),
            ("width".into(), number(window.width)),
            ("height".into(), number(window.height)),
            (
                "monitor".into(),
                match &window.monitor {
                    Some(name) => SealValue::String(name.clone()),
                    None => SealValue::Null,
                },
            ),
            ("node".into(), node_to_value(&window.node)),
        ])
        .into(),
    )
}

fn floating_from_value(value: &SealValue) -> Result<FloatingWindow, CoreError> {
    let map = value.as_map().ok_or_else(|| CoreError::new("floating window must be a map"))?;
    Ok(FloatingWindow {
        x: required_f32(map, "x")?,
        y: required_f32(map, "y")?,
        width: required_f32(map, "width")?,
        height: required_f32(map, "height")?,
        monitor: match find(map, "monitor") {
            Some(SealValue::String(name)) => Some(name.clone()),
            _ => None,
        },
        node: node_from_value(
            find(map, "node").ok_or_else(|| CoreError::new("floating window is missing node"))?,
        )?,
    })
}

#[cfg(test)]
fn layout_has_panel(node: &DockNode, id: &str) -> bool {
    match node {
        DockNode::Tabs { panels, .. } => panels.iter().any(|panel| panel == id),
        DockNode::Split { first, second, .. } => {
            layout_has_panel(first, id) || layout_has_panel(second, id)
        }
    }
}

fn number(value: f32) -> SealValue {
    let value = if value.is_finite() { value } else { 0.0 };
    let text = if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        let mut text = format!("{value:.6}");
        while text.contains('.') && text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
        text
    };
    SealValue::Number(text)
}

fn find<'a>(map: &'a [(String, SealValue)], key: &str) -> Option<&'a SealValue> {
    map.iter().find(|(name, _)| name == key).map(|(_, value)| value)
}

fn required_f32(map: &[(String, SealValue)], key: &str) -> Result<f32, CoreError> {
    match find(map, key) {
        Some(SealValue::Number(lexeme)) => lexeme
            .parse::<f32>()
            .map_err(|_| CoreError::new(format!("field '{key}' is not a number"))),
        _ => Err(CoreError::new(format!("field '{key}' is missing"))),
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn monitor_from_value(value: &SealValue) -> Result<MonitorInfo, CoreError> {
    let map = value.as_map().ok_or_else(|| CoreError::new("monitor record must be a map"))?;
    let name = match find(map, "name") {
        Some(SealValue::String(name)) => name.clone(),
        _ => return Err(CoreError::new("monitor record is missing name")),
    };
    Ok(MonitorInfo {
        name,
        x: required_f32(map, "x")?,
        y: required_f32(map, "y")?,
        width: required_f32(map, "width")?,
        height: required_f32(map, "height")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_roundtrip_keeps_splits_and_floating_windows() {
        let mut layout = Layout::default();
        layout.floating.push(FloatingWindow {
            x: 40.0,
            y: 50.0,
            width: 320.0,
            height: 240.0,
            monitor: Some("side".into()),
            node: DockNode::tabs(["settings"]),
        });
        let restored = Layout::from_value(&layout.to_value()).unwrap();
        assert_eq!(restored, layout);
    }

    #[test]
    fn m02_narrow_layout_drops_bottom_strip() {
        let layout = Layout::m02_narrow_for_width(1280.0);
        assert!(!layout.has_bottom_strip());
        assert!(layout_has_panel(&layout.root, "viewport"));
        assert!((Layout::narrow_right_ratio(1280.0) - (980.0 / 1280.0)).abs() < 0.02);
    }

    #[test]
    fn default_layout_excludes_extra_panels() {
        let layout = Layout::default();
        assert!(!layout.to_value().to_text().contains("welcome"));
        assert!(!layout.to_value().to_text().contains("stats"));
        assert!(layout_has_panel(&layout.root, "viewport"));
        assert!(layout_has_panel(&layout.root, "outliner"));
        assert!(layout_has_panel(&layout.root, "inspector"));
        assert!(layout_has_panel(&layout.root, "world"));
        assert!(layout_has_panel(&layout.root, "content"));
        assert!(layout_has_panel(&layout.root, "console"));
        assert!(layout_has_panel(&layout.root, "place"));
        assert!(layout.default_excludes_present().is_empty());
    }

    #[test]
    fn builtin_layouts_cover_default_debug_and_minimal() {
        let debug = Layout::builtin("debug").unwrap();
        let minimal = Layout::builtin("minimal").unwrap();
        assert!(layout_has_panel(&debug.root, "console"));
        assert!(layout_has_panel(&minimal.root, "viewport"));
        assert!(!layout_has_panel(&minimal.root, "console"));
        assert!(Layout::builtin("missing").is_none());
        let written = number(f32::INFINITY).to_text();
        assert!(!written.contains("inf"));
    }

    #[test]
    fn missing_monitor_falls_back_to_the_primary() {
        let saved = WindowPlacement {
            x: 4000.0,
            y: 10.0,
            width: 900.0,
            height: 700.0,
            maximized: false,
            monitor: Some("gone".into()),
        };
        let monitors = vec![
            MonitorInfo { name: "main".into(), x: 0.0, y: 0.0, width: 1920.0, height: 1080.0 },
            MonitorInfo { name: "side".into(), x: 1920.0, y: 0.0, width: 1280.0, height: 1024.0 },
        ];
        let placed = resolve_window(&saved, &monitors);
        assert_eq!(placed.monitor.as_deref(), Some("main"));
        assert!(placed.x >= 0.0 && placed.x < 1920.0);
        assert!(placed.y >= 0.0 && placed.y < 1080.0);
    }

    #[test]
    fn a_monitor_list_parses_and_rejects_a_bad_shape() {
        let text = "main = { name = \"main\", x = 0, y = 0, width = 800, height = 600 }\n";
        let monitors = monitors_from_text(text).unwrap();
        assert_eq!(monitors.len(), 1);
        assert_eq!(monitors[0].name, "main");
        assert_eq!(monitors[0].width, 800.0);
        assert!(monitors_from_text("monitors = []\n").is_err());
        assert!(monitors_from_text("not a map").is_err());
    }

    #[test]
    fn physical_monitor_sizes_convert_to_points() {
        let monitor = monitor_info_from_physical("main".into(), 0.0, 0.0, 1920.0, 1080.0, 2.0);
        assert_eq!(monitor.width, 960.0);
        assert_eq!(monitor.height, 540.0);
    }
}
