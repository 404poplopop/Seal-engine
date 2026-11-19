//! In-game UI designer commands: every edit writes the screen or theme file
//! at once, so nothing is lost when the editor closes.

use std::path::PathBuf;

use crate::asset_editor::{asset_kind, open_asset, show_asset, AssetKind};
use crate::gameui::{self, Screen, Theme, ANCHOR_PRESETS};
use crate::{CommandArgs, CoreError, EditorSession};

/// The node picked in the designer, by name.
pub const UI_NODE_DRAFT: &str = "asset-ui-node";

fn err(e: impl Into<String>) -> CoreError {
    CoreError::new(e.into())
}

fn open_screen(session: &EditorSession) -> Result<(PathBuf, Screen), CoreError> {
    let (rel, full) =
        open_asset(session).ok_or_else(|| err("no UI screen is open in the Asset Editor"))?;
    if asset_kind(&rel) != AssetKind::Screen {
        return Err(err("the open file is not a UI screen"));
    }
    let text = std::fs::read_to_string(&full)?;
    let screen = Screen::from_text(&text).map_err(|e| err(format!("{rel}: {e}")))?;
    Ok((full, screen))
}

fn save_screen(full: &std::path::Path, screen: &Screen) -> Result<(), CoreError> {
    crate::persist::atomic_write(full, screen.to_text().as_bytes())?;
    Ok(())
}

fn node_arg(
    screen: &Screen,
    args: &CommandArgs,
    session: &EditorSession,
) -> Result<usize, CoreError> {
    let name = args
        .get("node")
        .map(str::to_string)
        .or_else(|| session.draft(UI_NODE_DRAFT))
        .unwrap_or_default();
    screen.find(name.trim()).ok_or_else(|| err(format!("there is no node called '{name}'")))
}

fn refused(key: &str, why: &str) -> CoreError {
    err(format!("{key}: {}", gameui::plain_error(why)))
}

/// `asset.ui_new [path=ui/menu]`: make a new screen with a title and a button, and open it.
pub(crate) fn cmd_ui_new(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let root = session
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .ok_or_else(|| err("open a project first"))?;
    let mut rel = args.get("path").unwrap_or("ui/menu").trim().replace('\\', "/");
    if rel.is_empty() || rel.starts_with('/') || rel.split('/').any(|p| p == "..") {
        return Err(err(format!("'{rel}' is not a path inside the project")));
    }
    let ext = format!(".{}", gameui::SCREEN_EXT);
    if !rel.ends_with(&ext) {
        rel.push_str(&ext);
    }
    let stem = rel.trim_end_matches(&ext).to_string();
    let mut n = 2;
    while root.join(&rel).exists() {
        rel = format!("{stem}_{n}{ext}");
        n += 1;
    }
    let full = root.join(&rel);
    if let Some(dir) = full.parent() {
        std::fs::create_dir_all(dir)?;
    }
    save_screen(&full, &Screen::starter())?;
    show_asset(session, &rel)?;
    session.set_draft(UI_NODE_DRAFT, "Menu")
}

/// `asset.ui_add kind=`: add a node inside the picked container (or beside the picked control).
pub(crate) fn cmd_ui_add(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let (full, mut screen) = open_screen(session)?;
    let kind = args.get("kind").unwrap_or_default();
    let parent = session.draft(UI_NODE_DRAFT).and_then(|n| screen.find(&n));
    let i = screen
        .add(kind, parent)
        .ok_or_else(|| err(format!("'{kind}' is not a kind of UI node")))?;
    let name = screen.nodes[i].name.clone();
    save_screen(&full, &screen)?;
    session.set_draft(UI_NODE_DRAFT, &name)
}

/// `asset.ui_remove [node=]`: remove a node and everything inside it.
pub(crate) fn cmd_ui_remove(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let (full, mut screen) = open_screen(session)?;
    let i = node_arg(&screen, args, session)?;
    let next = screen.nodes[i].parent.map(|p| screen.nodes[p].name.clone()).unwrap_or_default();
    screen.remove(i);
    save_screen(&full, &screen)?;
    session.set_draft(UI_NODE_DRAFT, &next)
}

/// `asset.ui_move [node=] dir=up|down`: move a node before or after its neighbour.
pub(crate) fn cmd_ui_move(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let (full, mut screen) = open_screen(session)?;
    let i = node_arg(&screen, args, session)?;
    screen.move_sibling(i, args.get("dir") != Some("down"));
    save_screen(&full, &screen)
}

/// `asset.ui_set [node=] key= value=`: set a property, rename (`key=name`),
/// pick an anchor preset (`key=preset`) or the screen's theme (`key=theme`).
pub(crate) fn cmd_ui_set(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let (full, mut screen) = open_screen(session)?;
    let key = args.get("key").unwrap_or_default();
    let value = args.get("value").unwrap_or_default();
    if key == "theme" {
        let v = value.trim().replace('\\', "/");
        if !v.is_empty() && !v.ends_with(&format!(".{}", gameui::THEME_EXT)) {
            return Err(err(format!("the theme must be a .{} file", gameui::THEME_EXT)));
        }
        screen.theme = v;
        return save_screen(&full, &screen);
    }
    if key == "strings" {
        let v = value.trim().replace('\\', "/");
        if !v.is_empty() && (!v.ends_with(".strings") || v.split('/').any(|p| p == "..")) {
            return Err(err("the string table must be a .strings file in the project"));
        }
        screen.strings = v;
        return save_screen(&full, &screen);
    }
    let i = node_arg(&screen, args, session)?;
    match key {
        "name" => {
            let v = value.trim();
            if v.is_empty() || v.contains(char::is_whitespace) {
                return Err(err("a name needs letters and no spaces"));
            }
            if screen.find(v).is_some_and(|j| j != i) {
                return Err(err(format!("there is already a node called '{v}'")));
            }
            screen.nodes[i].name = v.to_string();
            save_screen(&full, &screen)?;
            return session.set_draft(UI_NODE_DRAFT, v);
        }
        "preset" => {
            let (_, a) = ANCHOR_PRESETS
                .iter()
                .find(|(n, _)| *n == value)
                .ok_or_else(|| err(format!("there is no anchor preset '{value}'")))?;
            let n = &mut screen.nodes[i];
            // Keep the node's size and put it at the preset's corner or edge.
            let (w, h) = (n.offset[2] - n.offset[0], n.offset[3] - n.offset[1]);
            let (w, h) = if w > 0.0 && h > 0.0 { (w, h) } else { (160.0, 40.0) };
            n.anchor = *a;
            let x0 = if a[0] == a[2] { -w * a[0] } else { 0.0 };
            let y0 = if a[1] == a[3] { -h * a[1] } else { 0.0 };
            let x1 = if a[0] == a[2] { x0 + w } else { 0.0 };
            let y1 = if a[1] == a[3] { y0 + h } else { 0.0 };
            n.offset = [x0, y0, x1, y1];
        }
        _ => {
            if let Some(why) = screen.nodes[i].set_prop(key, value) {
                return Err(refused(key, why));
            }
        }
    }
    save_screen(&full, &screen)
}

/// `asset.ui_nudge [node=] dx= dy= [edge=all|right|bottom]`: move a node (or
/// stretch its right / bottom edge) by screen pixels, as dragging in the preview does.
pub(crate) fn cmd_ui_nudge(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let (full, mut screen) = open_screen(session)?;
    let i = node_arg(&screen, args, session)?;
    let num = |k: &str| {
        args.get(k)
            .and_then(|v| v.trim().parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(0.0)
    };
    let (dx, dy) = (num("dx").round(), num("dy").round());
    let o = &mut screen.nodes[i].offset;
    match args.get("edge").unwrap_or("all") {
        "right" => o[2] = (o[2] + dx).max(o[0] + 4.0),
        "bottom" => o[3] = (o[3] + dy).max(o[1] + 4.0),
        _ => {
            o[0] += dx;
            o[2] += dx;
            o[1] += dy;
            o[3] += dy;
        }
    }
    save_screen(&full, &screen)
}

/// `asset.theme_set key= value=`: set one look of the open theme file, or of
/// the open screen's theme file.
pub(crate) fn cmd_theme_set(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let (rel, full) =
        open_asset(session).ok_or_else(|| err("no theme is open in the Asset Editor"))?;
    let full = match asset_kind(&rel) {
        AssetKind::Theme => full,
        AssetKind::Screen => {
            let (_, screen) = open_screen(session)?;
            if screen.theme.is_empty() {
                return Err(err("this screen has no theme file yet"));
            }
            session
                .project
                .as_ref()
                .map(|p| p.root.join(&screen.theme))
                .ok_or_else(|| err("open a project first"))?
        }
        _ => return Err(err("the open file is not a UI theme")),
    };
    let mut theme = match std::fs::read_to_string(&full) {
        Ok(t) => Theme::from_text(&t).map_err(|e| err(e.to_string()))?,
        Err(_) => Theme::default(),
    };
    let key = args.get("key").unwrap_or_default();
    if let Some(why) = theme.set(key, args.get("value").unwrap_or_default()) {
        return Err(refused(key, why));
    }
    if let Some(dir) = full.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::persist::atomic_write(&full, theme.to_text().as_bytes())?;
    Ok(())
}

/// `asset.theme_new`: give the open screen its own theme file next to it, starting from the defaults.
pub(crate) fn cmd_theme_new(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    let (rel, _) =
        open_asset(session).ok_or_else(|| err("no UI screen is open in the Asset Editor"))?;
    let (full, mut screen) = open_screen(session)?;
    let root = session
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .ok_or_else(|| err("open a project first"))?;
    let theme_rel = format!(
        "{}.{}",
        rel.trim_end_matches(&format!(".{}", gameui::SCREEN_EXT)),
        gameui::THEME_EXT
    );
    let theme_full = root.join(&theme_rel);
    if !theme_full.exists() {
        crate::persist::atomic_write(&theme_full, Theme::default().to_text().as_bytes())?;
    }
    screen.theme = theme_rel;
    save_screen(&full, &screen)
}
