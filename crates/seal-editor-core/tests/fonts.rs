//! Imported TTF/OTF fonts: reading them, importing them into fonts/, and using
//! them on game screens and themes.

use epaint_default_fonts::HACK_REGULAR;
use seal_editor_core::fonts::font_info;
use seal_editor_core::gameui::{self, Draw, Screen, Theme};
use seal_editor_core::{CommandArgs, EditorSession, Layout};
use std::fs;

#[test]
fn a_real_font_is_read_and_broken_ones_are_refused() {
    let info = font_info(HACK_REGULAR).expect("Hack reads");
    assert_eq!(info.family, "Hack");
    assert_eq!(info.style, "Regular");
    assert!(info.glyphs > 100, "{} glyphs", info.glyphs);
    assert!(!info.italic);
    assert_eq!(info.weight, 400.0);
    assert_eq!(font_info(&HACK_REGULAR[..200]), Err("font-error-unreadable"));
    assert_eq!(font_info(b""), Err("font-error-empty"));
}

#[test]
fn importing_a_font_puts_it_in_fonts_and_the_browser_knows_it() {
    let dir = std::env::temp_dir().join(format!("seal-font-import-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("Title.ttf");
    fs::write(&src, HACK_REGULAR).unwrap();
    let otf = dir.join("Body.OTF");
    fs::write(&otf, HACK_REGULAR).unwrap();
    let folder = dir.join("Game");
    let mut s = EditorSession::new(Layout::default());
    s.recent_path = Some(dir.join("recent.seal"));
    s.dispatch(
        "file.new-project",
        &CommandArgs::new(&[("path", folder.to_str().unwrap()), ("name", "Game")]),
    )
    .unwrap();
    let both = format!("{}\n{}", src.display(), otf.display());
    s.dispatch("asset.import", &CommandArgs::new(&[("path", both.as_str())])).unwrap();
    seal_editor_core::import_pipeline::pump_all(&mut s).unwrap();
    assert_eq!(fs::read(folder.join("fonts/Title.ttf")).unwrap(), HACK_REGULAR);
    assert!(folder.join("fonts/Body.OTF").is_file());
    assert_eq!(seal_editor_core::kind_of("fonts/Title.ttf", false), "font");
    assert_eq!(seal_editor_core::kind_of("fonts/Body.OTF", false), "font");
    assert!(seal_editor_core::FILTER_KINDS.contains(&"font"));
    // Opening it shows the font preview, not "no editor".
    s.dispatch("asset.edit", &CommandArgs::new(&[("asset", "Title.ttf")])).unwrap();
    assert_eq!(
        seal_editor_core::asset_editor::asset_kind("fonts/Title.ttf"),
        seal_editor_core::asset_editor::AssetKind::Font
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn controls_and_themes_pick_a_font_and_keep_it() {
    let mut screen = Screen::default();
    let label = screen.add("label", None).unwrap();
    let button = screen.add("button", None).unwrap();
    let n = &mut screen.nodes[label];
    assert!(n.props().contains(&"font"));
    assert_eq!(n.set_prop("font", "fonts\\Title.ttf"), None);
    assert_eq!(n.font, "fonts/Title.ttf");
    for bad in ["fonts/Title.png", "../Title.ttf", "/abs/Title.ttf", "C:/x/Title.ttf", "ttf"] {
        assert_eq!(n.set_prop("font", bad), Some("ui-error-font"), "{bad}");
    }
    assert_eq!(n.font, "fonts/Title.ttf", "a refused value leaves the old one");
    // The file keeps it.
    let back = Screen::from_text(&screen.to_text()).unwrap();
    assert_eq!(back.nodes[label].font, "fonts/Title.ttf");
    assert_eq!(back.nodes[button].font, "");
    // The theme's font is used where a control has none of its own.
    let mut theme = Theme::default();
    assert_eq!(theme.set("font", "fonts/Body.otf"), None);
    assert_eq!(theme.set("font", "Body.woff"), Some("ui-error-font"));
    let theme_back = Theme::from_text(&theme.to_text()).unwrap();
    assert_eq!(theme_back.font, "fonts/Body.otf");
    let live = gameui::Live::new(back, theme_back);
    let draws = gameui::draw(&live, 0.0);
    let font_of = |name: &str| {
        draws.iter().find_map(|d| match d {
            Draw::Text { text, font, .. } if text == name => Some(font.clone()),
            _ => None,
        })
    };
    let label_text = live.screen.nodes[label].text.clone();
    let button_text = live.screen.nodes[button].text.clone();
    assert_eq!(font_of(&label_text).as_deref(), Some("fonts/Title.ttf"));
    assert_eq!(font_of(&button_text).as_deref(), Some("fonts/Body.otf"));
    // Clearing goes back to the theme's font; paths turn into full paths for drawing.
    let mut cleared = live.screen.clone();
    assert_eq!(cleared.nodes[label].set_prop("font", ""), None);
    assert_eq!(cleared.nodes[label].font, "");
    let mut resolved = draws.clone();
    gameui::resolve_paths(&mut resolved, std::path::Path::new("/proj"));
    assert!(resolved.iter().any(|d| matches!(d, Draw::Text { font, .. } if font.ends_with("fonts/Title.ttf") && font.starts_with("/proj"))));
}
