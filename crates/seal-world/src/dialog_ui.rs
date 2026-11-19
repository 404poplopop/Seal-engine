//! The built-in game dialog: a message, a yes/no question, or a text field.
//! Scripts show it with one node. The player's answer arrives as the
//! `dialog_answered` event. Colors come from a theme.

use std::path::Path;

use crate::gameui::{self, Live, Screen, Screens, Theme, UiEvent};

pub const WIDGET: &str = "__dialog";

/// What a script asked to show.
#[derive(Clone, Debug, PartialEq)]
pub struct Ask {
    pub gen: u64,
    pub kind: String,
    pub title: String,
    pub message: String,
    pub placeholder: String,
    pub theme: String,
}

pub fn normalize_kind(kind: &str) -> Result<&'static str, String> {
    match kind.trim() {
        "message" => Ok("message"),
        "confirm" => Ok("confirm"),
        "input" => Ok("input"),
        other => Err(format!("a dialog kind is message, confirm, or input, not {other:?}")),
    }
}

const SCREEN: &str = r#"seal ui 1
panel Dialog
  anchor 0.5 0.5 0.5 0.5
  offset -220 -140 220 140
  vbox Body
    anchor 0 0 1 1
    label Title
      text Title
    label Message
      text Message
    input Answer
      placeholder Type here
    hbox Choice
      button Yes
        text Yes
      button No
        text No
      button Ok
        text OK
"#;

/// Build the dialog screen for `ask`. A theme path loads that theme file;
/// an empty path uses the built-in theme.
pub fn build(ask: &Ask, project: Option<&Path>) -> Result<Live, String> {
    let kind = normalize_kind(&ask.kind)?;
    let mut screen = Screen::from_text(SCREEN).map_err(|e| e.message)?;
    let theme = load_theme(&ask.theme, project)?;
    if let Some(n) = screen.nodes.iter_mut().find(|n| n.name == "Title") {
        n.text = ask.title.clone();
    }
    if let Some(n) = screen.nodes.iter_mut().find(|n| n.name == "Message") {
        n.text = ask.message.clone();
    }
    if let Some(n) = screen.nodes.iter_mut().find(|n| n.name == "Answer") {
        n.placeholder = ask.placeholder.clone();
        n.visible = kind == "input";
    }
    let show = |name: &str, on: bool, screen: &mut Screen| {
        if let Some(n) = screen.nodes.iter_mut().find(|n| n.name == name) {
            n.visible = on;
        }
    };
    show("Yes", kind == "confirm", &mut screen);
    show("No", kind == "confirm", &mut screen);
    show("Ok", kind != "confirm", &mut screen);
    let mut live = Live::new(screen, theme);
    if let Some(project) = project {
        live.project = Some(project.to_path_buf());
    }
    Ok(live)
}

fn load_theme(path: &str, project: Option<&Path>) -> Result<Theme, String> {
    let path = path.trim();
    if path.is_empty() {
        return Ok(Theme::default());
    }
    let root = project.ok_or_else(|| format!("cannot read the dialog theme {path}"))?;
    let text = std::fs::read_to_string(root.join(path))
        .map_err(|_| format!("cannot read the dialog theme {path}"))?;
    Theme::from_text(&text).map_err(|e| format!("{path}: {}", e.message))
}

/// Put the dialog on `screens`, replacing an older one when the ask changed.
pub fn install(screens: &mut Screens, ask: &Ask, project: Option<&Path>) -> Result<(), String> {
    if screens.dialog_gen == ask.gen && screens.shown.iter().any(|(w, _)| w == WIDGET) {
        return Ok(());
    }
    let mut live = build(ask, project)?;
    if screens.size[0] > 0.0 {
        live.resize(screens.size);
    }
    live.focus = live.focus_order().first().copied();
    live.keyboard = live.focus.is_some();
    screens.shown.retain(|(w, _)| w != WIDGET);
    screens.missing.remove(WIDGET);
    screens.shown.push((WIDGET.to_string(), live));
    screens.dialog_gen = ask.gen;
    Ok(())
}

pub fn dismiss(screens: &mut Screens) {
    screens.shown.retain(|(w, _)| w != WIDGET);
    screens.dialog_gen = 0;
}

/// A Yes, No, or Ok click on the dialog, plus the text field.
pub fn clicked(event: &UiEvent) -> Option<&'static str> {
    let UiEvent::Clicked(name) = event else { return None };
    match name.as_str() {
        "Yes" => Some("yes"),
        "No" => Some("no"),
        "Ok" => Some("ok"),
        _ => None,
    }
}

/// The text the player typed into the dialog, if it is showing.
pub fn answer_text(screens: &Screens) -> String {
    screens.text_of("Answer").unwrap_or_default()
}

/// Button fill from a theme, so a test can see the dialog uses it.
pub fn button_color(theme: &Theme) -> [f32; 4] {
    theme.button
}

/// Draws of a freshly built dialog, for tests and the play view.
pub fn preview(ask: &Ask, size: [f32; 2]) -> Result<Vec<gameui::Draw>, String> {
    let mut screens = Screens::default();
    screens.resize(size);
    install(&mut screens, ask, None)?;
    Ok(screens.draws(0.0))
}
