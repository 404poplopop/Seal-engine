use seal_editor_core::Chord;

/// The chord for a key that was pressed this frame, if the text fields are not using the keyboard.
/// Letter keys used for outliner type-ahead when no chord modifiers are held.
pub fn typeahead_key(chord: &Chord) -> Option<char> {
    if chord.ctrl || chord.alt || chord.shift {
        return None;
    }
    if chord.key.len() != 1 {
        return None;
    }
    let ch = chord.key.chars().next()?;
    if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
        Some(ch)
    } else {
        None
    }
}

pub fn pressed_chord(ui: &egui::Ui, by_physical: bool) -> Option<Chord> {
    // Only a text field keeps plain keys. A focused button or checkbox must not
    // swallow shortcuts (egui focuses buttons when they are clicked).
    let typing = ui.ctx().text_edit_focused();
    let chord = first_chord(ui, by_physical)?;
    if typing && !reaches_past_text_field(&chord) {
        return None;
    }
    Some(chord)
}

/// While a text field has focus it keeps plain keys and its own editing chords (copy, paste,
/// undo, word jumps...). Other ctrl or alt chords, such as Ctrl+S, still reach the editor.
fn reaches_past_text_field(chord: &Chord) -> bool {
    if !(chord.ctrl || chord.alt) {
        return false;
    }
    let editing = matches!(
        chord.key.as_str(),
        "a" | "c"
            | "v"
            | "x"
            | "z"
            | "y"
            | "backspace"
            | "delete"
            | "home"
            | "end"
            | "arrowleft"
            | "arrowright"
            | "arrowup"
            | "arrowdown"
    );
    !(chord.ctrl && !chord.alt && editing)
}

fn first_chord(ui: &egui::Ui, by_physical: bool) -> Option<Chord> {
    ui.input(|input| {
        for event in &input.events {
            let egui::Event::Key {
                key, physical_key, pressed: true, repeat: false, modifiers, ..
            } = event
            else {
                continue;
            };
            let logical = key_name(*key);
            if logical == "other" {
                continue;
            }
            let physical = physical_key.map(key_name).filter(|name| *name != "other");
            let name = Chord::chosen_key(logical, physical, by_physical);
            return Some(Chord {
                key: name,
                ctrl: modifiers.command || modifiers.ctrl,
                shift: modifiers.shift,
                alt: modifiers.alt,
            });
        }
        None
    })
}

/// A key's name in input bindings (`key.arrowright`, `key.space`): the shortcut name,
/// or egui's own name in lower case for keys shortcuts do not use.
pub fn binding_key_name(key: egui::Key) -> String {
    match key_name(key) {
        "other" => key.name().to_ascii_lowercase(),
        name => name.to_string(),
    }
}

fn key_name(key: egui::Key) -> &'static str {
    match key {
        egui::Key::A => "a",
        egui::Key::B => "b",
        egui::Key::C => "c",
        egui::Key::D => "d",
        egui::Key::E => "e",
        egui::Key::F => "f",
        egui::Key::G => "g",
        egui::Key::H => "h",
        egui::Key::I => "i",
        egui::Key::J => "j",
        egui::Key::K => "k",
        egui::Key::L => "l",
        egui::Key::M => "m",
        egui::Key::N => "n",
        egui::Key::O => "o",
        egui::Key::P => "p",
        egui::Key::Q => "q",
        egui::Key::R => "r",
        egui::Key::S => "s",
        egui::Key::T => "t",
        egui::Key::U => "u",
        egui::Key::V => "v",
        egui::Key::W => "w",
        egui::Key::X => "x",
        egui::Key::Y => "y",
        egui::Key::Z => "z",
        egui::Key::Enter => "enter",
        egui::Key::Escape => "escape",
        egui::Key::ArrowLeft => "arrowleft",
        egui::Key::ArrowRight => "arrowright",
        egui::Key::ArrowUp => "arrowup",
        egui::Key::ArrowDown => "arrowdown",
        egui::Key::Num0 => "0",
        egui::Key::Num1 => "1",
        egui::Key::Num2 => "2",
        egui::Key::Num3 => "3",
        egui::Key::Num4 => "4",
        egui::Key::Num5 => "5",
        egui::Key::Num6 => "6",
        egui::Key::Num7 => "7",
        egui::Key::Num8 => "8",
        egui::Key::Num9 => "9",
        egui::Key::F1 => "f1",
        egui::Key::F2 => "f2",
        egui::Key::F3 => "f3",
        egui::Key::F4 => "f4",
        egui::Key::F5 => "f5",
        egui::Key::F6 => "f6",
        egui::Key::F7 => "f7",
        egui::Key::F8 => "f8",
        egui::Key::F9 => "f9",
        egui::Key::F10 => "f10",
        egui::Key::F11 => "f11",
        egui::Key::F12 => "f12",
        egui::Key::Home => "home",
        egui::Key::End => "end",
        egui::Key::PageUp => "pageup",
        egui::Key::PageDown => "pagedown",
        egui::Key::Backspace => "backspace",
        egui::Key::Delete => "delete",
        _ => "other",
    }
}

pub enum PaletteEvent {
    Idle,
    MoveUp,
    MoveDown,
    Run,
    /// A row was clicked.
    Pick(usize),
    Close,
    /// Pin the current query.
    Pin,
    /// Fill the box with a saved query.
    UseSaved(usize),
}

/// Command palette. `rows` are `(title, shortcut)`.
pub fn show_palette(
    ctx: &egui::Context,
    query: &mut String,
    rows: &[(String, String)],
    selected: usize,
    placeholder: &str,
    pin_label: &str,
    saved: &[String],
) -> PaletteEvent {
    let mut event = PaletteEvent::Idle;
    egui::Window::new(placeholder)
        .title_bar(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_TOP, egui::Vec2::new(0.0, 72.0))
        .show(ctx, |ui| {
            ui.set_width(480.0);
            let response = ui.add(
                egui::TextEdit::singleline(query)
                    .hint_text(placeholder)
                    .desired_width(f32::INFINITY),
            );
            response.request_focus();
            if ui.button(pin_label).clicked() {
                event = PaletteEvent::Pin;
            }
            for (index, query_text) in saved.iter().enumerate() {
                if ui.selectable_label(false, query_text).clicked() {
                    event = PaletteEvent::UseSaved(index);
                }
            }
            if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                event = PaletteEvent::Close;
            } else if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                event = PaletteEvent::Run;
            } else if ui.input(|input| input.key_pressed(egui::Key::ArrowUp)) {
                event = PaletteEvent::MoveUp;
            } else if ui.input(|input| input.key_pressed(egui::Key::ArrowDown)) {
                event = PaletteEvent::MoveDown;
            }
            let moved = matches!(event, PaletteEvent::MoveUp | PaletteEvent::MoveDown);
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                for (index, (title, shortcut)) in rows.iter().enumerate() {
                    let size = egui::vec2(ui.available_width(), 24.0);
                    let (rect, row) = ui.allocate_exact_size(size, egui::Sense::click());
                    row.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::SelectableLabel,
                            true,
                            index == selected,
                            title,
                        )
                    });
                    let ink = crate::widgets::row_fill(ui, rect, &row, index == selected, true)
                        .unwrap_or(ui.visuals().text_color());
                    let font = egui::TextStyle::Body.resolve(ui.style());
                    let painter = ui.painter().with_clip_rect(rect);
                    painter.text(
                        rect.left_center() + egui::vec2(8.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        title,
                        font.clone(),
                        ink,
                    );
                    painter.text(
                        rect.right_center() - egui::vec2(8.0, 0.0),
                        egui::Align2::RIGHT_CENTER,
                        shortcut,
                        font,
                        ink.gamma_multiply(0.6),
                    );
                    if index == selected && moved {
                        row.scroll_to_me(None);
                    }
                    if row.clicked() {
                        event = PaletteEvent::Pick(index);
                    }
                }
            });
        });
    event
}

#[cfg(test)]
mod tests {
    use super::{binding_key_name, key_name};

    #[test]
    fn navigation_and_edit_keys_keep_their_chord_names() {
        assert_eq!(key_name(egui::Key::Home), "home");
        assert_eq!(key_name(egui::Key::End), "end");
        assert_eq!(key_name(egui::Key::PageUp), "pageup");
        assert_eq!(key_name(egui::Key::PageDown), "pagedown");
        assert_eq!(key_name(egui::Key::Backspace), "backspace");
        assert_eq!(binding_key_name(egui::Key::ArrowRight), "arrowright");
        assert_eq!(binding_key_name(egui::Key::Space), "space");
        assert_eq!(key_name(egui::Key::Delete), "delete");
        assert_eq!(key_name(egui::Key::Num1), "1");
        assert_eq!(key_name(egui::Key::F3), "f3");
    }
}
