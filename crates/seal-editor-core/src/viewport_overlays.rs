//! In-viewport overlay bar and stats placement; stored in layout session state.

const KEY: &str = "viewport.overlay_layout";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewportOverlayLayout {
    pub toolbar_edge: Edge,
    pub toolbar_collapsed: bool,
    pub stats_edge: Edge,
    pub stats_visible: bool,
    pub fps_edge: Edge,
    pub fps_visible: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Default for ViewportOverlayLayout {
    fn default() -> Self {
        Self {
            toolbar_edge: Edge::TopLeft,
            toolbar_collapsed: false,
            stats_edge: Edge::TopRight,
            stats_visible: true,
            fps_edge: Edge::TopRight,
            fps_visible: true,
        }
    }
}

impl Edge {
    /// Screen position for overlay text in a `width`×`height` view (y down).
    pub fn overlay_text_position(self, width: f32, height: f32, margin: f32) -> (f32, f32) {
        match self {
            Self::TopLeft => (margin, margin),
            Self::TopRight => (width - margin, margin),
            Self::BottomLeft => (margin, height - margin),
            Self::BottomRight => (width - margin, height - margin),
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "tl" => Some(Self::TopLeft),
            "tr" => Some(Self::TopRight),
            "bl" => Some(Self::BottomLeft),
            "br" => Some(Self::BottomRight),
            _ => None,
        }
    }

    pub fn token(self) -> &'static str {
        match self {
            Self::TopLeft => "tl",
            Self::TopRight => "tr",
            Self::BottomLeft => "bl",
            Self::BottomRight => "br",
        }
    }
}

impl ViewportOverlayLayout {
    pub fn encode(&self) -> String {
        format!(
            "toolbar={}:{};stats={}:{};fps={}:{}",
            self.toolbar_edge.token(),
            if self.toolbar_collapsed { 1 } else { 0 },
            self.stats_edge.token(),
            if self.stats_visible { 1 } else { 0 },
            self.fps_edge.token(),
            if self.fps_visible { 1 } else { 0 },
        )
    }

    pub fn decode(text: &str) -> Self {
        let mut out = Self::default();
        for part in text.split(';').filter(|s| !s.is_empty()) {
            let Some((key, rest)) = part.split_once('=') else { continue };
            let Some((edge, flag)) = rest.split_once(':') else { continue };
            let Some(e) = Edge::parse(edge) else { continue };
            let on = flag == "1";
            match key {
                "toolbar" => {
                    out.toolbar_edge = e;
                    out.toolbar_collapsed = on;
                }
                "stats" => {
                    out.stats_edge = e;
                    out.stats_visible = on;
                }
                "fps" => {
                    out.fps_edge = e;
                    out.fps_visible = on;
                }
                _ => {}
            }
        }
        out
    }
}

pub fn load_from_panel_states(
    states: &std::collections::BTreeMap<String, String>,
) -> ViewportOverlayLayout {
    states.get(KEY).map(|s| ViewportOverlayLayout::decode(s)).unwrap_or_default()
}

pub fn save_to_panel_states(
    states: &mut std::collections::BTreeMap<String, String>,
    layout: &ViewportOverlayLayout,
) {
    states.insert(KEY.into(), layout.encode());
}

/// Keep viewport model toggles aligned with stored overlay layout (e.g. after layout load).
pub fn sync_session_models(session: &mut EditorSession) {
    let layout = load_from_panel_states(&session.models.panel_states);
    session.models.viewport_show_stats = layout.stats_visible;
    session.models.viewport_show_fps = layout.fps_visible;
}

use crate::command::CommandArgs;
use crate::error::CoreError;
use crate::session::EditorSession;

/// `viewport.overlay target=toolbar|stats|fps action=collapse|show|edge edge=tl|tr|bl|br`.
pub fn apply_overlay_command(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let mut layout = load_from_panel_states(&session.models.panel_states);
    let target = args.get("target").unwrap_or("toolbar");
    let action = args.get("action").unwrap_or("show");
    match (target, action) {
        ("toolbar", "collapse") => {
            layout.toolbar_collapsed = !layout.toolbar_collapsed;
        }
        ("toolbar", "edge") => {
            let edge = args.get("edge").unwrap_or("tl");
            layout.toolbar_edge =
                Edge::parse(edge).ok_or_else(|| CoreError::new("unknown edge"))?;
        }
        ("stats", "show") => layout.stats_visible = !layout.stats_visible,
        ("stats", "edge") => {
            let edge = args.get("edge").unwrap_or("tr");
            layout.stats_edge = Edge::parse(edge).ok_or_else(|| CoreError::new("unknown edge"))?;
        }
        ("fps", "show") => layout.fps_visible = !layout.fps_visible,
        ("fps", "edge") => {
            let edge = args.get("edge").unwrap_or("tr");
            layout.fps_edge = Edge::parse(edge).ok_or_else(|| CoreError::new("unknown edge"))?;
        }
        _ => return Err(CoreError::new("unknown overlay command")),
    }
    save_to_panel_states(&mut session.models.panel_states, &layout);
    sync_session_models(session);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_layout_round_trips() {
        let lay = ViewportOverlayLayout {
            toolbar_edge: Edge::BottomLeft,
            toolbar_collapsed: true,
            stats_edge: Edge::BottomRight,
            stats_visible: false,
            fps_edge: Edge::TopLeft,
            fps_visible: true,
        };
        let again = ViewportOverlayLayout::decode(&lay.encode());
        assert_eq!(lay, again);
    }

    #[test]
    fn damaged_layout_falls_back_to_defaults() {
        let lay = ViewportOverlayLayout::decode("not-a-real=layout");
        assert_eq!(lay, ViewportOverlayLayout::default());
    }

    #[test]
    fn stats_overlay_corner_bottom_left() {
        let (x, y) = Edge::BottomLeft.overlay_text_position(800.0, 600.0, 10.0);
        assert!(x < 50.0);
        assert!(y > 550.0);
    }
}
