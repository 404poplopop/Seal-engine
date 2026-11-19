use std::path::Path;

use seal_sealtext::{parse, SealValue};

use crate::error::CoreError;
use crate::persist::atomic_write;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    pub key: String,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Chord {
    pub fn parse(text: &str) -> Result<Self, CoreError> {
        let mut ctrl = false;
        let mut shift = false;
        let mut alt = false;
        let mut key = None;
        for part in text.split('+') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "cmd" | "primary" => ctrl = true,
                "shift" => shift = true,
                "alt" => alt = true,
                other => {
                    if key.is_some() {
                        return Err(CoreError::new(format!("chord '{text}' has two keys")));
                    }
                    // Common spellings of the same key resolve to the name the input layer reports.
                    let name = match other {
                        "del" => "delete",
                        "esc" => "escape",
                        "return" => "enter",
                        "bksp" => "backspace",
                        named => named,
                    };
                    key = Some(name.to_string());
                }
            }
        }
        let key = key.ok_or_else(|| CoreError::new(format!("chord '{text}' has no key")))?;
        Ok(Self { key, ctrl, shift, alt })
    }

    /// The key a binding should see. Physical mode uses the key's position, not the typed character.
    pub fn chosen_key(logical: &str, physical: Option<&str>, by_physical: bool) -> String {
        if by_physical {
            physical.unwrap_or(logical).to_string()
        } else {
            logical.to_string()
        }
    }

    pub fn label(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        // Keys are stored lower-case; show them the way keycaps read: K, F11, Space.
        let mut key = String::new();
        let mut chars = self.key.chars();
        if let Some(first) = chars.next() {
            key.extend(first.to_uppercase());
            key.push_str(chars.as_str());
        }
        let mut text = parts.join("+");
        if !text.is_empty() {
            text.push('+');
        }
        text.push_str(&key);
        text
    }

    pub fn token(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("ctrl");
        }
        if self.alt {
            parts.push("alt");
        }
        if self.shift {
            parts.push("shift");
        }
        parts.push(self.key.as_str());
        parts.join("+")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Binding {
    command: String,
    steps: Vec<Chord>,
    when: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Keymap {
    bindings: Vec<Binding>,
}

/// Result of feeding one key into the keymap, including a two-step chord waiting for its second key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyResolution {
    pub pending: Option<Chord>,
    pub command: Option<String>,
}

impl Keymap {
    pub fn defaults() -> Result<Self, CoreError> {
        let mut keymap = Self::default();
        for (command, chord) in [
            ("view.command-palette", "ctrl+shift+p"),
            ("file.save", "ctrl+s"),
            ("window.reset-layout", "ctrl+shift+r"),
            ("edit.undo", "ctrl+z"),
            ("edit.redo", "ctrl+shift+z"),
            ("file.quit", "ctrl+q"),
            ("file.new-project", "ctrl+n"),
            ("file.open-project", "ctrl+o"),
            ("file.save_all", "ctrl+shift+s"),
            ("edit.cut", "ctrl+x"),
            ("edit.copy", "ctrl+c"),
            ("edit.paste", "ctrl+v"),
            ("edit.select_all", "ctrl+a"),
            ("edit.rename", "f2"),
            ("edit.delete", "del"),
            ("edit.duplicate", "ctrl+d"),
            ("scene.group", "ctrl+g"),
            ("app.toggle_fullscreen", "shift+f11"),
            ("view.maximize_panel", "f11"),
            ("viewport.pane_expand", "alt+shift+w"),
            ("view.zoom_in", "ctrl+="),
            ("view.zoom_out", "ctrl+-"),
            ("view.zoom_reset", "ctrl+0"),
            ("settings.open", "ctrl+,"),
            ("panel.close", "ctrl+w"),
            ("view.focus-next", "ctrl+tab"),
            ("panel.dock_left", "ctrl+alt+arrowleft"),
            ("panel.dock_right", "ctrl+alt+arrowright"),
            ("panel.dock_top", "ctrl+alt+arrowup"),
            ("panel.dock_bottom", "ctrl+alt+arrowdown"),
            ("viewport.frame", "f"),
            ("outliner.range_select", "ctrl+shift+l"),
            ("view.toggle_asset_drawer", "ctrl+space"),
            ("viewport.grid_snap", "shift+g"),
            ("viewport.grid_snap_decrease", "ctrl+openbracket"),
            ("viewport.grid_snap_increase", "ctrl+closebracket"),
            ("viewport.angle_snap", "shift+a"),
        ] {
            let chord = Chord::parse(chord)?;
            keymap.override_bind(command, chord)?;
        }
        // Second chords for the same commands, as most editors offer them.
        for (command, chord) in [("edit.redo", "ctrl+y"), ("edit.delete", "backspace")] {
            keymap.bindings.push(Binding {
                command: command.into(),
                steps: vec![Chord::parse(chord)?],
                when: String::new(),
            });
        }
        keymap.override_steps(
            "keymap.open",
            vec![Chord::parse("ctrl+k")?, Chord::parse("ctrl+s")?],
            "",
        )?;
        keymap.override_steps(
            "console.clear",
            vec![Chord::parse("ctrl+l")?],
            "panelFocus == 'console'",
        )?;
        keymap.override_steps(
            "console.jump_match",
            vec![Chord::parse("f3")?],
            "panelFocus == 'console'",
        )?;
        keymap.override_steps(
            "outliner.filter_next",
            vec![Chord::parse("f3")?],
            "outlinerFocused == 'true'",
        )?;
        keymap.override_steps(
            "outliner.filter_prev",
            vec![Chord::parse("shift+f3")?],
            "outlinerFocused == 'true'",
        )?;
        for (command, key) in [
            ("tiles.tool_brush", "b"),
            ("tiles.tool_rectangle", "u"),
            ("tiles.tool_line", "n"),
            ("tiles.tool_fill", "g"),
            ("tiles.tool_picker", "i"),
            ("tiles.tool_eraser", "x"),
            ("tiles.tool_random", "y"),
            ("tiles.tool_pattern", "p"),
        ] {
            keymap.override_steps(command, vec![Chord::parse(key)?], "panelFocus == 'place'")?;
        }
        for slot in 1..=9 {
            let slot = slot.to_string();
            let store = Chord::parse(&format!("ctrl+{slot}"))?;
            let recall = Chord::parse(&slot)?;
            if store.key == slot {
                keymap.bindings.push(Binding {
                    command: "viewport.bookmark_store".into(),
                    steps: vec![store],
                    when: String::new(),
                });
            }
            if recall.key == slot {
                keymap.bindings.push(Binding {
                    command: "viewport.bookmark_recall".into(),
                    steps: vec![recall],
                    when: String::new(),
                });
            }
        }
        Ok(keymap)
    }

    pub fn bind(&mut self, command: &str, chord: Chord) -> Result<(), CoreError> {
        if let Some(owner) = self.command_for(&chord) {
            if owner != command {
                return Err(CoreError::new(format!(
                    "chord {} is already used by {owner}",
                    chord.label()
                )));
            }
        }
        self.override_bind(command, chord)
    }

    pub fn override_bind(&mut self, command: &str, chord: Chord) -> Result<(), CoreError> {
        self.override_steps(command, vec![chord], "")
    }

    pub fn override_steps(
        &mut self,
        command: &str,
        steps: Vec<Chord>,
        when: &str,
    ) -> Result<(), CoreError> {
        self.bindings.retain(|binding| binding.command != command && binding.steps != steps);
        self.bindings.push(Binding { command: command.to_string(), steps, when: when.to_string() });
        Ok(())
    }

    pub fn command_for(&self, chord: &Chord) -> Option<&str> {
        self.bindings
            .iter()
            .find(|binding| binding.steps.len() == 1 && binding.steps[0] == *chord)
            .map(|binding| binding.command.as_str())
    }

    pub fn chord_for(&self, command: &str) -> Option<&Chord> {
        self.bindings
            .iter()
            .find(|binding| binding.command == command)
            .and_then(|binding| binding.steps.first())
    }

    /// Single-step chords for listing and conflict detection.
    pub fn all_single_bindings(&self) -> impl Iterator<Item = (String, Chord)> + '_ {
        self.bindings.iter().filter_map(|binding| {
            if binding.steps.len() == 1 {
                Some((binding.command.clone(), binding.steps[0].clone()))
            } else {
                None
            }
        })
    }

    pub fn remove_command(&mut self, command: &str) {
        self.bindings.retain(|binding| binding.command != command);
    }

    /// Add another binding even when the chord is already in use (conflict detection uses this).
    pub fn push_binding(&mut self, command: &str, chord: Chord) {
        self.bindings.push(Binding {
            command: command.into(),
            steps: vec![chord],
            when: String::new(),
        });
    }

    pub fn shortcut(&self, command: &str) -> String {
        self.bindings
            .iter()
            .find(|binding| binding.command == command)
            .map(|binding| binding.steps.iter().map(Chord::label).collect::<Vec<_>>().join(" "))
            .unwrap_or_default()
    }

    /// Resolve one physical press. A chord that begins a two-step binding is held in `pending`.
    pub fn resolve(
        &self,
        pending: Option<&Chord>,
        chord: &Chord,
        text_input: bool,
        ctx: &crate::when::WhenContext,
    ) -> KeyResolution {
        if text_input && !chord.ctrl && !chord.alt && pending.is_none() {
            return KeyResolution { pending: None, command: None };
        }
        if let Some(first) = pending {
            let steps = [first.clone(), chord.clone()];
            if let Some(command) = self.match_steps(&steps, ctx) {
                return KeyResolution { pending: None, command: Some(command) };
            }
        }
        if self.starts_sequence(chord, ctx) {
            return KeyResolution { pending: Some(chord.clone()), command: None };
        }
        let one = [chord.clone()];
        KeyResolution { pending: None, command: self.match_steps(&one, ctx) }
    }

    fn match_steps(&self, steps: &[Chord], ctx: &crate::when::WhenContext) -> Option<String> {
        self.bindings.iter().find_map(|binding| {
            if binding.steps == steps && crate::when::when_allows(&binding.when, ctx) {
                Some(binding.command.clone())
            } else {
                None
            }
        })
    }

    fn starts_sequence(&self, chord: &Chord, ctx: &crate::when::WhenContext) -> bool {
        self.bindings.iter().any(|binding| {
            binding.steps.len() > 1
                && binding.steps[0] == *chord
                && crate::when::when_allows(&binding.when, ctx)
        })
    }

    pub fn apply_user(&mut self, user: &Keymap) {
        for binding in &user.bindings {
            let _ = self.override_steps(&binding.command, binding.steps.clone(), &binding.when);
        }
    }

    pub fn to_text(&self) -> String {
        let mut entries = vec![("schema_version".to_string(), SealValue::Number("1".into()))];
        let bindings = SealValue::Array(
            self.bindings
                .iter()
                .map(|binding| {
                    let mut fields = vec![
                        ("command".into(), SealValue::String(binding.command.clone())),
                        (
                            "chord".into(),
                            SealValue::String(
                                binding
                                    .steps
                                    .iter()
                                    .map(Chord::token)
                                    .collect::<Vec<_>>()
                                    .join(" "),
                            ),
                        ),
                    ];
                    if !binding.when.is_empty() {
                        fields.push(("when".into(), SealValue::String(binding.when.clone())));
                    }
                    SealValue::Map((fields).into())
                })
                .collect(),
        );
        entries.push(("bindings".into(), bindings));
        SealValue::Map((entries).into()).to_text()
    }

    pub fn from_text(text: &str) -> Result<Self, CoreError> {
        let value = parse(text)?;
        let map = value.as_map().ok_or_else(|| CoreError::new("keymap must be a map"))?;
        let items = match map.iter().find(|(key, _)| key == "bindings") {
            Some((_, SealValue::Array(items))) => items,
            _ => return Ok(Self::default()),
        };
        let mut keymap = Self::default();
        for item in items {
            let Some(fields) = item.as_map() else {
                continue;
            };
            let command = match fields.iter().find(|(key, _)| key == "command") {
                Some((_, SealValue::String(command))) => command.clone(),
                _ => continue,
            };
            let chord = match fields.iter().find(|(key, _)| key == "chord") {
                Some((_, SealValue::String(chord))) => chord.clone(),
                _ => continue,
            };
            let when = match fields.iter().find(|(key, _)| key == "when") {
                Some((_, SealValue::String(when))) => when.clone(),
                _ => String::new(),
            };
            let steps =
                chord.split_whitespace().map(Chord::parse).collect::<Result<Vec<_>, _>>()?;
            keymap.override_steps(&command, steps, &when)?;
        }
        Ok(keymap)
    }

    pub fn save(&self, path: &Path) -> Result<(), CoreError> {
        atomic_write(path, self.to_text().as_bytes())
    }

    pub fn load(path: &Path) -> Result<Self, CoreError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_text(&text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_key_mode_uses_the_key_position() {
        assert_eq!(Chord::chosen_key("a", Some("q"), true), "q");
        assert_eq!(Chord::chosen_key("a", Some("q"), false), "a");
        assert_eq!(Chord::chosen_key("a", None, true), "a");
    }

    #[test]
    fn rebind_persists_and_rejects_conflicts() {
        let dir = std::env::temp_dir().join(format!("seal-keymap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("keymap.seal");
        let mut keymap = Keymap::defaults().unwrap();
        let chord = Chord::parse("ctrl+shift+k").unwrap();
        keymap.bind("file.quit", chord.clone()).unwrap();
        let conflict = keymap.bind("window.save-layout", chord);
        assert!(conflict.is_err());
        keymap.save(&path).unwrap();
        let loaded = Keymap::load(&path).unwrap();
        assert_eq!(loaded.command_for(&Chord::parse("ctrl+shift+k").unwrap()), Some("file.quit"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn user_keymap_overrides_a_default_chord() {
        let mut keymap = Keymap::defaults().unwrap();
        let mut user = Keymap::default();
        user.override_bind("file.quit", Chord::parse("ctrl+shift+q").unwrap()).unwrap();
        keymap.apply_user(&user);
        assert!(keymap.command_for(&Chord::parse("ctrl+q").unwrap()).is_none());
        assert_eq!(keymap.command_for(&Chord::parse("ctrl+shift+q").unwrap()), Some("file.quit"));
    }

    #[test]
    fn two_step_chord_waits_and_text_input_blocks_plain_keys() {
        let keymap = Keymap::defaults().unwrap();
        let ctx = crate::when::WhenContext::new();
        let first = keymap.resolve(None, &Chord::parse("ctrl+k").unwrap(), false, &ctx);
        assert!(first.command.is_none());
        let second =
            keymap.resolve(first.pending.as_ref(), &Chord::parse("ctrl+s").unwrap(), false, &ctx);
        assert_eq!(second.command.as_deref(), Some("keymap.open"));
        let typed = keymap.resolve(None, &Chord::parse("del").unwrap(), true, &ctx);
        assert!(typed.command.is_none());
        let held = keymap.resolve(None, &Chord::parse("ctrl+z").unwrap(), true, &ctx);
        assert_eq!(held.command.as_deref(), Some("edit.undo"));
    }

    #[test]
    fn everyday_editing_keys_reach_their_commands() {
        let keymap = Keymap::defaults().unwrap();
        let ctx = crate::when::WhenContext::new();
        let run =
            |text: &str| keymap.resolve(None, &Chord::parse(text).unwrap(), false, &ctx).command;
        // The input layer names the key "delete"; the default binding must use the same name.
        let pressed = Chord { key: "delete".into(), ctrl: false, shift: false, alt: false };
        assert_eq!(
            keymap.resolve(None, &pressed, false, &ctx).command.as_deref(),
            Some("edit.delete")
        );
        assert_eq!(run("backspace").as_deref(), Some("edit.delete"));
        assert_eq!(run("ctrl+s").as_deref(), Some("file.save"));
        assert_eq!(run("ctrl+d").as_deref(), Some("edit.duplicate"));
        assert_eq!(run("ctrl+y").as_deref(), Some("edit.redo"));
        assert_eq!(run("ctrl+shift+z").as_deref(), Some("edit.redo"));
        assert_eq!(run("f2").as_deref(), Some("edit.rename"));
    }
}
