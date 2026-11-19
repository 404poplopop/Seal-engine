//! Asset Editor form for `.soundset` sound containers.

use std::path::Path;

use seal_editor_core::asset_editor::{ContainerMode, SoundContainerDef};
use seal_editor_core::{CommandArgs, EditorSession};
use seal_ui::PanelFrame;

use crate::{command, PanelEvent};

pub(crate) fn soundset_editor(
    frame: &mut PanelFrame,
    session: &EditorSession,
    rel: &str,
    full: &Path,
) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let mut event = PanelEvent::None;
    let raw = std::fs::read_to_string(full).unwrap_or_default();
    let parsed = SoundContainerDef::parse(&raw);
    if let Err(error) = &parsed {
        frame.padded(|f| {
            f.muted(&session.messages.format(
                "asset-soundset-error",
                &[("name", rel), ("error", error.to_string().as_str())],
            ));
        });
        return event;
    }
    let Ok(def) = parsed else { return event };
    let defaults = SoundContainerDef::default();
    let mut pending: Option<(String, String)> = None;

    frame.bar(|frame| {
        if frame.button(&text("asset-soundset-play")) {
            event = command("asset.soundset_play", &[]);
        }
        if frame.button(&text("asset-sound-stop")) {
            event = command("asset.sound_stop", &[]);
        }
    });

    frame.scroll(|frame| {
        frame.padded(|frame| {
            let modes: [(ContainerMode, String); 4] = [
                (ContainerMode::Random, text("asset-soundset-mode-random")),
                (ContainerMode::Shuffle, text("asset-soundset-mode-shuffle")),
                (ContainerMode::Sequence, text("asset-soundset-mode-sequence")),
                (ContainerMode::Layered, text("asset-soundset-mode-layered")),
            ];
            for (mode, label) in modes {
                if frame.chip(&label, def.mode == mode) {
                    pending = Some(("mode".into(), mode.name().into()));
                }
            }
            let mut num = |label: &str, key: &str, v: f32, d: f32, step: f32| {
                let mut value = v;
                let tip = session
                    .messages
                    .format("inspector-reset-default", &[("value", &d.to_string())]);
                let mut pick = None;
                let (changed, reset, _) = frame.number_row(
                    label,
                    &mut value,
                    step,
                    false,
                    (v != d).then_some(tip.as_str()),
                    None,
                    None,
                    &mut pick,
                );
                if reset {
                    pending = Some((key.to_string(), d.to_string()));
                } else if changed {
                    pending = Some((key.to_string(), value.to_string()));
                }
            };
            num(&text("asset-volume"), "volume", def.volume, defaults.volume, 0.05);
            num(
                &text("asset-soundset-pitch-min"),
                "pitch_min",
                def.pitch_min,
                defaults.pitch_min,
                0.05,
            );
            num(
                &text("asset-soundset-pitch-max"),
                "pitch_max",
                def.pitch_max,
                defaults.pitch_max,
                0.05,
            );
            num(
                &text("asset-soundset-vol-min"),
                "volume_var_min",
                def.volume_var_min,
                defaults.volume_var_min,
                0.05,
            );
            num(
                &text("asset-soundset-vol-max"),
                "volume_var_max",
                def.volume_var_max,
                defaults.volume_var_max,
                0.05,
            );
            num(
                &text("asset-soundset-max-voices"),
                "max_voices",
                def.max_voices as f32,
                defaults.max_voices as f32,
                1.0,
            );
            num(
                &text("asset-soundset-cooldown"),
                "cooldown",
                def.cooldown,
                defaults.cooldown,
                0.05,
            );
            num(&text("asset-soundset-seed"), "seed", def.seed as f32, defaults.seed as f32, 1.0);

            frame.section(&text("asset-soundset-entries"), |frame| {
                if frame.button(&text("asset-soundset-entry-add")) {
                    event = command("asset.soundset_entry_add", &[]);
                }
                for (index, entry) in def.entries.iter().enumerate() {
                    let idx = index.to_string();
                    frame.row(|frame| {
                        frame.label(&format!("{}.", index + 1));
                        let mut path = entry.path.clone();
                        let (changed, _) =
                            frame.draft_field(&text("asset-soundset-entry-path"), &mut path);
                        if changed && path != entry.path {
                            pending = Some((format!("entry.{index}.path"), path));
                        }
                        let mut weight = entry.weight;
                        if frame.number(&text("asset-soundset-entry-weight"), &mut weight, 0.1) {
                            pending = Some((format!("entry.{index}.weight"), weight.to_string()));
                        }
                        let mut volume = entry.volume;
                        if frame.number(&text("asset-soundset-entry-volume"), &mut volume, 0.05) {
                            pending = Some((format!("entry.{index}.volume"), volume.to_string()));
                        }
                        if index > 0
                            && frame.icon_button(
                                seal_ui::Icon::ViewArrowUp,
                                &text("asset-soundset-move-up"),
                            )
                        {
                            event = command(
                                "asset.soundset_entry_move",
                                &[("index", idx.as_str()), ("direction", "up")],
                            );
                        }
                        if index + 1 < def.entries.len()
                            && frame.button(&text("asset-soundset-move-down"))
                        {
                            event = command(
                                "asset.soundset_entry_move",
                                &[("index", idx.as_str()), ("direction", "down")],
                            );
                        }
                        if frame.icon_button(
                            seal_ui::Icon::EditDelete,
                            &text("asset-soundset-entry-remove"),
                        ) {
                            event =
                                command("asset.soundset_entry_remove", &[("index", idx.as_str())]);
                        }
                    });
                }
            });
        });
    });

    if let Some((key, value)) = pending {
        if matches!(event, PanelEvent::None) {
            event = PanelEvent::Commands(vec![(
                "asset.soundset_set".into(),
                CommandArgs::new(&[("key", key.as_str()), ("value", value.as_str())]),
            )]);
        }
    }

    event
}
