//! In-window menu labels and Alt+mnemonic activation on Windows and Linux.

use seal_editor_core::{menu_category_mnemonic, menu_mnemonic_parts};

/// Menus drawn on the bar, in order. `view` stays a command bucket and is not a bar menu.
pub const BAR_MENUS: &[&str] =
    &["file", "edit", "window", "tools", "build", "select", "entity", "help"];

pub fn mnemonic_button(ui: &egui::Ui, text: &str) -> egui::Button<'static> {
    let (visible, mnemonic) = menu_mnemonic_parts(text);
    // Underlines stay hidden until Alt, same as a normal desktop menu bar.
    let show_mark =
        ui.input(|input| input.modifiers.alt && !input.modifiers.ctrl && !input.modifiers.command);
    let letter = if show_mark {
        mnemonic.or_else(|| visible.chars().next().map(|ch| ch.to_ascii_lowercase()))
    } else {
        None
    };
    let Some(letter) = letter else {
        return egui::Button::new(visible);
    };
    let font = ui.style().text_styles[&egui::TextStyle::Button].clone();
    let ink = ui.visuals().text_color();
    // The default underline sits on the font descent and reads as a stray underscore.
    let ascent = ui.ctx().fonts_mut(|fonts| {
        let galley = fonts.layout_no_wrap("Ag".to_owned(), font.clone(), ink);
        galley
            .rows
            .first()
            .and_then(|row| row.glyphs.first().map(|glyph| glyph.font_ascent))
            .unwrap_or(font.size * 0.8)
    });
    let line = Some(ascent + 1.0);
    let plain = egui::TextFormat {
        font_id: font.clone(),
        color: ink,
        line_height: line,
        ..Default::default()
    };
    let marked = egui::TextFormat {
        font_id: font,
        color: ink,
        line_height: line,
        underline: egui::Stroke::new(1.0, ink),
        ..Default::default()
    };
    let mut job = egui::text::LayoutJob::default();
    let mut seen = false;
    let mut glyph = String::new();
    for ch in visible.chars() {
        let mark = !seen && ch.eq_ignore_ascii_case(&letter);
        if mark {
            seen = true;
        }
        glyph.clear();
        glyph.push(ch);
        job.append(&glyph, 0.0, if mark { marked.clone() } else { plain.clone() });
    }
    egui::Button::new(job)
}

pub fn menu_category_key(category: &str, messages: &seal_editor_core::Messages) -> Option<char> {
    let label = messages.get(&format!("menu-{category}"));
    let (_, mnemonic) = menu_mnemonic_parts(&label);
    mnemonic.or_else(|| menu_category_mnemonic(category))
}

pub fn take_alt_mnemonic(
    ctx: &egui::Context,
    messages: &seal_editor_core::Messages,
) -> Option<String> {
    if ctx.input(|input| input.modifiers.command) {
        return None;
    }
    ctx.input(|input| {
        if !input.modifiers.alt || input.modifiers.ctrl {
            return None;
        }
        for event in &input.events {
            let egui::Event::Key { key, pressed: true, repeat: false, .. } = event else {
                continue;
            };
            let Some(ch) = mnemonic_from_key(*key) else {
                continue;
            };
            for category in BAR_MENUS {
                if menu_category_key(category, messages) == Some(ch) {
                    return Some((*category).to_string());
                }
            }
        }
        None
    })
}

fn mnemonic_from_key(key: egui::Key) -> Option<char> {
    match key {
        egui::Key::A => Some('a'),
        egui::Key::B => Some('b'),
        egui::Key::C => Some('c'),
        egui::Key::D => Some('d'),
        egui::Key::E => Some('e'),
        egui::Key::F => Some('f'),
        egui::Key::G => Some('g'),
        egui::Key::H => Some('h'),
        egui::Key::I => Some('i'),
        egui::Key::J => Some('j'),
        egui::Key::K => Some('k'),
        egui::Key::L => Some('l'),
        egui::Key::M => Some('m'),
        egui::Key::N => Some('n'),
        egui::Key::O => Some('o'),
        egui::Key::P => Some('p'),
        egui::Key::Q => Some('q'),
        egui::Key::R => Some('r'),
        egui::Key::S => Some('s'),
        egui::Key::T => Some('t'),
        egui::Key::U => Some('u'),
        egui::Key::V => Some('v'),
        egui::Key::W => Some('w'),
        egui::Key::X => Some('x'),
        egui::Key::Y => Some('y'),
        egui::Key::Z => Some('z'),
        _ => None,
    }
}
