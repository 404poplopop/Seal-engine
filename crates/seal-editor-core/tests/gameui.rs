//! In-game UI: screen files, layout, controls, focus, themes and play.

use seal_editor_core::gameui::{self, Draw, Live, Screen, Screens, Theme, UiEvent};
use std::collections::{BTreeMap, BTreeSet};

const MENU: &str = r#"seal ui 1
# a pause menu
panel Root
  anchor 0 0 1 1
  vbox Menu
    anchor 0.5 0.5 0.5 0.5
    offset -150 -150 150 150
    spacing 10
    label Title
      text "Paused"
      align center
    button Resume
    slider Volume
      max 10
      step 1
      value 5
    toggle Fullscreen
    dropdown Quality
      options Low | Medium | High
      selected 1
    input PlayerName
      placeholder "Your name"
  progress Health
    anchor 0 0 0 0
    offset 20 20 220 40
    value 0.75
  image Icon
    anchor 1 0 1 0
    offset -84 20 -20 84
    picture sprites/coin.png
"#;

fn live() -> Live {
    let mut l = Live::new(Screen::from_text(MENU).expect("menu parses"), Theme::default());
    l.resize([1280.0, 720.0]);
    l
}

fn center(l: &Live, name: &str) -> [f32; 2] {
    let r = l.layout().rect(l.screen.find(name).unwrap()).unwrap();
    [r.x + r.w / 2.0, r.y + r.h / 2.0]
}

#[test]
fn screen_files_round_trip_and_report_lines() {
    let s = Screen::from_text(MENU).unwrap();
    assert_eq!(s.nodes.len(), 10);
    let vol = &s.nodes[s.find("Volume").unwrap()];
    assert_eq!((vol.value, vol.max, vol.step), (5.0, 10.0, 1.0));
    assert_eq!(s.nodes[s.find("Quality").unwrap()].options, ["Low", "Medium", "High"]);
    assert_eq!(s.nodes[s.find("Icon").unwrap()].image, "sprites/coin.png");
    assert_eq!(s.nodes[s.find("Title").unwrap()].parent, s.find("Menu"));
    // Saving and reading back keeps everything.
    let again = Screen::from_text(&s.to_text()).unwrap();
    assert_eq!(again, s);
    assert_eq!(again.to_text(), s.to_text());
    // Broken files name the line.
    let bad = "seal ui 1\npanel A\n  slider S\n    value lots\n";
    let e = Screen::from_text(bad).unwrap_err();
    assert_eq!(e.line, 4);
    assert!(e.to_string().contains("value"), "{e}");
    assert_eq!(Screen::from_text("seal ui 1\nlabel A\n  button B\n").unwrap_err().line, 3);
    assert_eq!(Screen::from_text("hello\n").unwrap_err().line, 1);
    assert_eq!(Screen::from_text("seal ui 1\nbutton A\nbutton A\n").unwrap_err().line, 3);
    assert_eq!(Screen::from_text("seal ui 1\nwidget A\n").unwrap_err().line, 2);
    // Anchors must stay inside 0..1 with left before right.
    let mut n = gameui::Node::new("panel", "P");
    assert!(n.set_prop("anchor", "0.8 0 0.2 1").is_some());
    assert!(n.set_prop("anchor", "0 0 1 1").is_none());
}

#[test]
fn anchors_containers_and_grid_lay_out() {
    let l = live();
    let lay = l.layout();
    let r = |n: &str| lay.rect(l.screen.find(n).unwrap()).unwrap();
    assert_eq!((r("Root").w, r("Root").h), (1280.0, 720.0));
    // Centre-anchored column, 300 px square.
    let m = r("Menu");
    assert_eq!((m.x, m.y, m.w, m.h), (490.0, 210.0, 300.0, 300.0));
    // The column stacks its children top to bottom with spacing 10.
    let (t, b, s) = (r("Title"), r("Resume"), r("Volume"));
    assert!(t.y < b.y && b.y < s.y);
    assert!((b.y - (t.y + t.h) - 10.0).abs() < 0.01);
    // Top-right image keeps its size when the screen grows.
    let mut big = live();
    big.resize([1920.0, 1080.0]);
    let i = big.layout().rect(big.screen.find("Icon").unwrap()).unwrap();
    assert_eq!((i.x, i.w, i.h), (1920.0 - 84.0, 64.0, 64.0));
    // A grid fills rows of `columns`.
    let text = "seal ui 1\ngrid G\n  anchor 0 0 1 1\n  columns 3\n  spacing 0\n  button A\n  button B\n  button C\n  button D\n";
    let mut g = Live::new(Screen::from_text(text).unwrap(), Theme::default());
    g.resize([600.0, 400.0]);
    let gl = g.layout();
    let p = |n: &str| gl.rect(g.screen.find(n).unwrap()).unwrap();
    assert_eq!(p("A").y, p("C").y);
    assert!(p("B").x > p("A").x && p("C").x > p("B").x);
    assert!(p("D").y > p("A").y);
    assert_eq!(p("D").x, p("A").x);
}

#[test]
fn mouse_drives_every_control() {
    let mut l = live();
    // Button click on release over it.
    let at = center(&l, "Resume");
    l.pointer_move(at);
    l.pointer_down(at);
    l.pointer_up(at);
    assert_eq!(l.events, [UiEvent::Clicked("Resume".into())]);
    assert_eq!(l.events[0].script_event(), "ui_Resume_clicked");
    l.events.clear();
    // Releasing elsewhere does not click.
    l.pointer_down(at);
    l.pointer_up([5.0, 700.0]);
    assert!(l.events.is_empty());
    // Slider: drag to the right end gives max, snapped to step.
    let r = l.layout().rect(l.screen.find("Volume").unwrap()).unwrap();
    l.pointer_down([r.x + r.w * 0.5, r.y + r.h / 2.0]);
    l.pointer_move([r.x + r.w + 50.0, r.y]);
    l.pointer_up([r.x + r.w + 50.0, r.y]);
    assert_eq!(l.value_of("Volume"), Some(10.0));
    assert!(l.events.contains(&UiEvent::Changed("Volume".into())));
    // Toggle flips.
    let t = center(&l, "Fullscreen");
    l.pointer_down(t);
    l.pointer_up(t);
    assert_eq!(l.value_of("Fullscreen"), Some(1.0));
    // Dropdown opens, then a click on the third choice picks it.
    let d = center(&l, "Quality");
    l.pointer_down(d);
    l.pointer_up(d);
    assert!(l.open.is_some());
    let dr = l.layout().rect(l.screen.find("Quality").unwrap()).unwrap();
    let row = (dr.h).max(1.0);
    let draws = gameui::draw(&l, 0.0);
    assert!(draws.iter().any(|x| matches!(x, Draw::Text { text, .. } if text == "High")));
    let third = [dr.x + 10.0, dr.y + dr.h + 2.5 * gameui_row(&l, row)];
    l.pointer_move(third);
    l.pointer_down(third);
    assert_eq!(l.text_of("Quality").as_deref(), Some("High"));
    assert!(l.open.is_none());
    // Typing goes into the focused text field; Backspace deletes.
    let i = center(&l, "PlayerName");
    l.pointer_down(i);
    l.pointer_up(i);
    l.type_text("Ada");
    l.key("W"); // a letter key while typing is text, not a focus move
    l.key("Backspace");
    assert_eq!(l.text_of("PlayerName").as_deref(), Some("Ad"));
    assert_eq!(l.focus, l.screen.find("PlayerName"));
}

/// Height of one open dropdown choice row, measured from the draw list.
fn gameui_row(l: &Live, fallback: f32) -> f32 {
    let draws = gameui::draw(l, 0.0);
    let ys: Vec<f32> = draws
        .iter()
        .filter_map(|d| match d {
            Draw::Text { rect, text, .. } if ["Low", "Medium", "High"].contains(&text.as_str()) => {
                Some(rect.y)
            }
            _ => None,
        })
        .collect();
    if ys.len() >= 2 {
        (ys[ys.len() - 1] - ys[ys.len() - 2]).abs()
    } else {
        fallback
    }
}

#[test]
fn keyboard_and_gamepad_move_focus_and_press() {
    let mut l = live();
    let name = |l: &Live| l.focus.map(|i| l.screen.nodes[i].name.clone());
    assert_eq!(
        l.focus_order().iter().map(|&i| l.screen.nodes[i].name.as_str()).collect::<Vec<_>>(),
        ["Resume", "Volume", "Fullscreen", "Quality", "PlayerName"]
    );
    l.key("Tab");
    assert_eq!(name(&l).as_deref(), Some("Resume"));
    assert!(l.keyboard, "the focus frame shows after keys");
    assert!(gameui::draw(&l, 0.0)
        .iter()
        .any(|d| matches!(d, Draw::Frame { color, .. } if *color == l.theme.focus)));
    l.key("Enter");
    assert_eq!(l.events, [UiEvent::Clicked("Resume".into())]);
    // Gamepad down moves to the slider; right nudges it by one step.
    l.key("DPadDown");
    assert_eq!(name(&l).as_deref(), Some("Volume"));
    l.key("DPadRight");
    assert_eq!(l.value_of("Volume"), Some(6.0));
    l.key("Left");
    l.key("Left");
    assert_eq!(l.value_of("Volume"), Some(4.0));
    // Shift+Tab goes back; Tab wraps around.
    l.key("Shift+Tab");
    assert_eq!(name(&l).as_deref(), Some("Resume"));
    l.key("Shift+Tab");
    assert_eq!(name(&l).as_deref(), Some("PlayerName"));
    // Gamepad A on the toggle flips it.
    l.key("Up");
    l.key("Up");
    assert_eq!(name(&l).as_deref(), Some("Fullscreen"));
    l.key("GamepadA");
    assert_eq!(l.value_of("Fullscreen"), Some(1.0));
    // A dropdown opens with A, moves with down, picks with A, closes with B.
    l.key("Down");
    l.key("GamepadA");
    assert!(l.open.is_some());
    l.key("DPadDown");
    l.key("GamepadA");
    assert_eq!(l.text_of("Quality").as_deref(), Some("High"));
    l.key("GamepadA");
    l.key("GamepadB");
    assert!(l.open.is_none());
    // Disabled and hidden controls are skipped.
    let v = l.screen.find("Volume").unwrap();
    l.screen.nodes[v].enabled = false;
    let menu = l.screen.find("Menu").unwrap();
    assert!(!l.focus_order().contains(&v));
    l.screen.nodes[menu].visible = false;
    assert!(l.focus_order().is_empty());
}

#[test]
fn themes_load_and_style_controls() {
    let mut t = Theme::default();
    assert!(t.set("button", "#AA2244").is_none());
    assert!(t.set("font_size", "28").is_none());
    assert!(t.set("font_size", "1").is_some(), "too small is refused");
    assert!(t.set("corner", "lots").is_some());
    let text = t.to_text();
    assert!(text.starts_with("seal theme 1"));
    let back = Theme::from_text(&text).unwrap();
    assert_eq!(back, t);
    let e = Theme::from_text("seal theme 1\nbutton not-a-colour\n").unwrap_err();
    assert_eq!(e.line, 2);
    // A screen file points at its theme; loading applies it.
    let dir = std::env::temp_dir().join(format!("seal-gameui-theme-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("ui")).unwrap();
    std::fs::write(dir.join("ui/red.sealtheme"), &text).unwrap();
    std::fs::write(
        dir.join("ui/menu.sealui"),
        format!("seal ui 1\ntheme ui/red.sealtheme\nbutton Go\n"),
    )
    .unwrap();
    let live = gameui::load(&dir, "ui/menu.sealui").unwrap();
    assert_eq!(live.theme.font_size, 28.0);
    let draws = gameui::draw(&live, 0.0);
    assert!(draws
        .iter()
        .any(|d| matches!(d, Draw::Fill { color, .. } if *color == live.theme.button)));
    assert!(draws
        .iter()
        .any(|d| matches!(d, Draw::Text { text, size, .. } if text == "Go" && *size == 28.0)));
    // A missing theme is an error naming the file.
    std::fs::write(dir.join("ui/bad.sealui"), "seal ui 1\ntheme ui/none.sealtheme\nbutton Go\n")
        .unwrap();
    assert!(gameui::load(&dir, "ui/bad.sealui").unwrap_err().contains("ui/none.sealtheme"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn designer_edits_tree() {
    let mut s = Screen::starter();
    let menu = s.find("Menu").unwrap();
    let b = s.add("button", Some(menu)).unwrap();
    assert_eq!(s.nodes[b].name, "Button");
    let b2 = s.add("button", Some(menu)).unwrap();
    assert_eq!(s.nodes[b2].name, "Button2");
    // With a button selected, a new node goes beside it in the same container.
    let l = s.add("label", s.find("Play")).unwrap();
    assert_eq!(s.nodes[l].parent, s.find("Menu"));
    s.remove(l);
    assert!(s.add("widget", None).is_none());
    // Moving a node up swaps it with its previous sibling and keeps the file valid.
    let moved = s.move_sibling(s.find("Button2").unwrap(), true);
    assert_eq!(s.nodes[moved].name, "Button2");
    let order: Vec<_> =
        s.children(Some(s.find("Menu").unwrap())).map(|i| s.nodes[i].name.clone()).collect();
    assert_eq!(order, ["Title", "Play", "Button2", "Button"]);
    assert_eq!(Screen::from_text(&s.to_text()).unwrap(), s);
    // Removing a container removes its children too.
    s.remove(s.find("Menu").unwrap());
    assert_eq!(s.nodes.len(), 1);
}

#[test]
fn screens_follow_scripts_during_play() {
    let dir = std::env::temp_dir().join(format!("seal-gameui-play-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("ui")).unwrap();
    std::fs::write(dir.join("ui/pause.sealui"), MENU).unwrap();
    let mut screens = Screens::default();
    screens.resize([1280.0, 720.0]);
    let wanted: BTreeSet<String> = ["pause".to_string(), "hud".to_string()].into();
    let mut texts = BTreeMap::new();
    texts.insert("Title".to_string(), "Game over".to_string());
    screens.sync(Some(&dir), &wanted, &texts);
    assert!(screens.active());
    assert!(screens.uses_lines(), "hud has no screen file, so it stays as text lines");
    assert_eq!(screens.text_of("Title").as_deref(), Some("Game over"));
    // Typed text is not overwritten when script texts did not change.
    screens.pointer_down(center(&screens.shown[0].1, "PlayerName"));
    screens.type_text("Bo");
    assert!(screens.typing());
    screens.sync(Some(&dir), &wanted, &texts);
    assert_eq!(screens.text_of("PlayerName").as_deref(), Some("Bo"));
    let at = center(&screens.shown[0].1, "Resume");
    screens.pointer_down(at);
    screens.pointer_up(at);
    let events: Vec<String> = screens.take_events().iter().map(|e| e.script_event()).collect();
    assert!(events.contains(&"ui_Resume_clicked".to_string()), "{events:?}");
    assert!(events.contains(&"ui_PlayerName_changed".to_string()), "{events:?}");
    assert_eq!(screens.values()["Volume"].0, 5.0);
    assert_eq!(screens.values()["Quality"].1, "Medium");
    // Hiding the widget hides the screen.
    screens.sync(Some(&dir), &BTreeSet::new(), &texts);
    assert!(!screens.active());
    // A broken file is reported once and falls back to lines.
    std::fs::write(dir.join("ui/broken.sealui"), "seal ui 1\nbutton\n").unwrap();
    screens.sync(Some(&dir), &["broken".to_string()].into(), &texts);
    assert_eq!(screens.errors.len(), 1);
    assert!(screens.errors[0].contains("line 2"), "{:?}", screens.errors);
    let _ = std::fs::remove_dir_all(&dir);
}
