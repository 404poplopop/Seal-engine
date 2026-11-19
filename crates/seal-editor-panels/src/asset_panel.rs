//! Asset Editor panel: the built-in editor for pictures, sounds, effects,
//! sprite animations, fonts and text.
//!
//! Every change is sent as a command that writes the file at once, so closing the
//! editor or the app never loses an edit.

use std::path::{Path, PathBuf};

use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};
use seal_editor_core::asset_editor::{
    asset_kind, open_asset, picked_clip, AssetKind, Emitter, EmitterDef, Sound, ASSET_CLIP_DRAFT,
    ASSET_DRAFT, ASSET_GRID_DRAFT, ASSET_VOLUME_DRAFT, ASSET_ZOOM_DRAFT,
};
use seal_editor_core::{CommandArgs, EditorSession};
use seal_ui::PanelFrame;

use crate::{command, PanelEvent};

fn toolbar_button(frame: &mut PanelFrame, label: &str, enabled: bool) -> bool {
    frame
        .ui_mut()
        .add_enabled_ui(enabled, |ui| {
            seal_ui::crisp::button(ui, egui::Button::new(label), true, false).clicked()
        })
        .inner
}

fn draft_cmd(key: &str, value: &str) -> (String, CommandArgs) {
    ("draft.set".into(), CommandArgs::new(&[("key", key), ("value", value)]))
}

pub(crate) fn asset_panel(frame: &mut PanelFrame, session: &mut EditorSession) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let Some((rel, full)) = open_asset(session) else {
        let missing = session.draft(ASSET_DRAFT).filter(|r| !r.is_empty());
        let mut event = PanelEvent::None;
        frame.padded(|frame| match missing {
            Some(name) => {
                frame.muted(&session.messages.format("asset-missing", &[("name", name.as_str())]));
                if frame.button(&text("asset-close")) {
                    event = command("asset.close", &[]);
                }
            }
            None => frame.empty(&text("asset-none")),
        });
        return event;
    };
    let mut event = PanelEvent::None;
    let name = rel.rsplit('/').next().unwrap_or(&rel).to_string();
    frame.bar(|frame| {
        frame.label(&rel);
        frame.space(12.0);
        if frame.button(&text("asset-reveal")) {
            event = command("asset.edit", &[("asset", name.as_str()), ("kind", "file")]);
        }
        let open_label = text("command-asset-open-external");
        let compare_label = text("command-asset-compare-saved");
        let open_on = seal_editor_core::open_external_enabled(session);
        let compare_on = seal_editor_core::compare_saved_enabled(session);
        let line = frame
            .ui_mut()
            .ctx()
            .data(|data| data.get_temp::<u32>(egui::Id::new(("asset-text-line", rel.as_str()))))
            .unwrap_or(1);
        let line_s = line.to_string();
        if toolbar_button(frame, &open_label, open_on) {
            event = command(
                "asset.open_external",
                &[("path", rel.as_str()), ("line", line_s.as_str())],
            );
        }
        if toolbar_button(frame, &compare_label, compare_on) {
            event = command("asset.compare_saved", &[("path", rel.as_str())]);
        }
        if frame.button(&text("asset-close")) {
            event = command("asset.close", &[]);
        }
    });
    let body = match asset_kind(&rel) {
        AssetKind::Image => image_editor(frame, session, &rel, &full),
        AssetKind::Sound => sound_editor(frame, session, &rel, &full),
        AssetKind::Effect => effect_editor(frame, session, &rel, &full),
        AssetKind::Text => text_editor(frame, session, &rel, &full),
        AssetKind::Animation => animation_editor(frame, session, &rel, &full),
        AssetKind::SoundSet => crate::soundset_panel::soundset_editor(frame, session, &rel, &full),
        AssetKind::Strings => crate::strings_panel::strings_editor(frame, session, &rel, &full),
        AssetKind::SpriteSource => crate::sprite_source_panel::editor(frame, session, &rel, &full),
        AssetKind::LayeredPicture => {
            crate::layered_picture_panel::editor(frame, session, &rel, &full)
        }
        AssetKind::Screen => {
            crate::ui_designer::screen_editor(frame, session, &rel, &full, |frame| {
                text_editor(frame, session, &rel, &full)
            })
        }
        AssetKind::Theme => crate::ui_designer::theme_editor(frame, session, &rel, &full),
        AssetKind::Font => font_editor(frame, session, &rel, &full),
        AssetKind::Input => input_editor(frame, session, &rel, &full),
        AssetKind::Save => save_editor(frame, session),
        AssetKind::TileSet => tile_set_editor(frame, session, &rel, &full),
        AssetKind::TextureComposite => {
            crate::texture_asset_panel::texture_composite_editor(frame, session, &rel, &full)
        }
        AssetKind::DataType | AssetKind::Data => text_editor(frame, session, &rel, &full),
        AssetKind::Test => crate::test_runner_panel::test_asset_editor(frame, session, &rel, &full),
        AssetKind::Material => material_editor(frame, session, &rel, &full),
        AssetKind::Mesh => mesh_editor(frame, session, &rel, &full),
        AssetKind::Other => {
            let size = std::fs::metadata(&full).map(|m| m.len()).unwrap_or(0);
            let size = human_size(size);
            frame.padded(|frame| {
                frame.muted(
                    &session
                        .messages
                        .format("asset-other", &[("name", name.as_str()), ("size", size.as_str())]),
                );
            });
            PanelEvent::None
        }
    };
    if !matches!(body, PanelEvent::None) {
        event = body;
    }
    event
}

// ---------------------------------------------------------------- input bindings

/// A `.sealinput` file: each action with a chip per key or pad button. Click a
/// chip, then press the new key or button; every change saves at once.
fn input_editor(
    frame: &mut PanelFrame,
    session: &EditorSession,
    rel: &str,
    full: &Path,
) -> PanelEvent {
    use seal_editor_core::input_editor::{
        axis_source_label, button_label, capturing, read_map, AxisSource, INPUT_NOTE_DRAFT,
    };
    let text = |key: &str| session.messages.get(key);
    let map = match read_map(full) {
        Ok(map) => map,
        Err(e) => {
            frame.padded(|f| {
                f.muted(
                    &session
                        .messages
                        .format("asset-input-broken", &[("name", rel), ("error", e.as_str())]),
                )
            });
            return PanelEvent::None;
        }
    };
    let mut out: Vec<(String, CommandArgs)> = Vec::new();
    // The window binds the next key or pad button (before shortcuts see it).
    let waiting = capturing(session);
    let ctx = frame.ui_mut().ctx().clone();
    frame.scroll(|f| {
        f.padded(|f| {
            if let Some(note) = session.draft(INPUT_NOTE_DRAFT).filter(|n| !n.is_empty()) {
                f.muted(&note);
            }
            f.section(&text("asset-input-actions"), |f| {
                for (action, buttons) in map.actions() {
                    f.row(|f| {
                        // A fixed name column keeps the chips lined up.
                        f.ui_mut().allocate_ui_with_layout(
                            egui::vec2(120.0, 22.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.set_min_width(120.0);
                                ui.label(action);
                            },
                        );
                        if buttons.is_empty() && waiting.as_ref().is_none_or(|(a, _)| a != action) {
                            f.muted(&text("asset-input-empty"));
                        }
                        for (slot, button) in buttons.iter().enumerate() {
                            let here =
                                waiting.as_ref().is_some_and(|(a, s)| a == action && *s == slot);
                            let label =
                                if here { text("asset-input-press") } else { button_label(button) };
                            let slot_s = slot.to_string();
                            if f.chip(&label, here) && !here {
                                out.push((
                                    "asset.input_capture".into(),
                                    CommandArgs::new(&[
                                        ("action", action),
                                        ("slot", slot_s.as_str()),
                                    ]),
                                ));
                            }
                            let tip = session.messages.format(
                                "asset-input-unbind",
                                &[("button", button_label(button).as_str()), ("action", action)],
                            );
                            if f.icon_button(seal_ui::Icon::ViewClose, &tip) {
                                out.push((
                                    "asset.input_unbind".into(),
                                    CommandArgs::new(&[
                                        ("action", action),
                                        ("slot", slot_s.as_str()),
                                    ]),
                                ));
                            }
                            let modifier = map.modifier(action, slot);
                            let kind = match modifier {
                                seal_editor_core::seal_input::BindMod::Hold(_) => "hold",
                                seal_editor_core::seal_input::BindMod::Double(_) => "double",
                                seal_editor_core::seal_input::BindMod::Toggle => "toggle",
                                seal_editor_core::seal_input::BindMod::None => "none",
                            };
                            let options = [
                                ("none", text("asset-input-mod-none")),
                                ("hold", text("asset-input-mod-hold")),
                                ("double", text("asset-input-mod-double")),
                                ("toggle", text("asset-input-mod-toggle")),
                            ];
                            if let Some(pick) =
                                f.choice(&text("asset-input-modifier"), kind, &options)
                            {
                                let seconds = match modifier {
                                    seal_editor_core::seal_input::BindMod::Hold(s)
                                    | seal_editor_core::seal_input::BindMod::Double(s) => s,
                                    _ => 0.25,
                                };
                                let seconds_s = format!("{seconds}");
                                out.push((
                                    "asset.input_modifier".into(),
                                    CommandArgs::new(&[
                                        ("action", action),
                                        ("slot", slot_s.as_str()),
                                        ("kind", pick.as_str()),
                                        ("seconds", seconds_s.as_str()),
                                    ]),
                                ));
                            }
                            if let seal_editor_core::seal_input::BindMod::Hold(seconds)
                            | seal_editor_core::seal_input::BindMod::Double(seconds) = modifier
                            {
                                let mut seconds = seconds;
                                if f.number(&text("asset-input-mod-seconds"), &mut seconds, 0.05) {
                                    let seconds_s = format!("{}", seconds.max(0.0));
                                    out.push((
                                        "asset.input_modifier".into(),
                                        CommandArgs::new(&[
                                            ("action", action),
                                            ("slot", slot_s.as_str()),
                                            ("kind", kind),
                                            ("seconds", seconds_s.as_str()),
                                        ]),
                                    ));
                                }
                            }
                        }
                        let end = buttons.len();
                        let adding =
                            waiting.as_ref().is_some_and(|(a, s)| a == action && *s >= end);
                        if adding {
                            f.chip(&text("asset-input-press"), true);
                        } else {
                            let tip = session
                                .messages
                                .format("asset-input-add-slot", &[("action", action)]);
                            if f.icon_button(seal_ui::Icon::ViewPlus, &tip) {
                                let end_s = end.to_string();
                                out.push((
                                    "asset.input_capture".into(),
                                    CommandArgs::new(&[
                                        ("action", action),
                                        ("slot", end_s.as_str()),
                                    ]),
                                ));
                            }
                        }
                        let tip =
                            session.messages.format("asset-input-remove", &[("action", action)]);
                        if f.icon_button(seal_ui::Icon::EditDelete, &tip) {
                            out.push((
                                "asset.input_remove".into(),
                                CommandArgs::new(&[("action", action)]),
                            ));
                        }
                    });
                }
                let name_id = egui::Id::new(("asset-input-new", rel));
                let mut name: String = ctx.data(|d| d.get_temp(name_id)).unwrap_or_default();
                let (changed, committed) = f.draft_field(&text("asset-input-new"), &mut name);
                if changed {
                    ctx.data_mut(|d| d.insert_temp(name_id, name.clone()));
                }
                if (committed || f.button(&text("asset-input-add"))) && !name.trim().is_empty() {
                    ctx.data_mut(|d| d.remove::<String>(name_id));
                    out.push((
                        "asset.input_add".into(),
                        CommandArgs::new(&[("name", name.trim())]),
                    ));
                }
            });
            let axes: Vec<_> = map.axes().collect();
            if !axes.is_empty() {
                f.section(&text("asset-input-axes"), |f| {
                    for (axis, sources) in axes {
                        let words: Vec<String> = sources.iter().map(axis_source_label).collect();
                        f.readout(axis, &words.join(", "));
                        let dead = sources.iter().find_map(|s| match s {
                            AxisSource::Analog { deadzone, .. } => Some(*deadzone),
                            _ => None,
                        });
                        if let Some(dead) = dead {
                            let mut value = dead;
                            if f.number(&text("asset-input-deadzone"), &mut value, 0.01)
                                && value != dead
                            {
                                let v = format!("{:.2}", value.clamp(0.0, 0.95));
                                out.push((
                                    "asset.input_deadzone".into(),
                                    CommandArgs::new(&[("axis", axis), ("value", v.as_str())]),
                                ));
                            }
                        }
                    }
                });
            }
            crate::input_devices_ui::section(f, session, &mut out);
            f.muted(&text("asset-input-hint"));
        })
    });
    if out.is_empty() {
        PanelEvent::None
    } else {
        PanelEvent::Commands(out)
    }
}

// ---------------------------------------------------------------- save games

fn save_editor(frame: &mut PanelFrame, session: &EditorSession) -> PanelEvent {
    use seal_editor_core::save_editor::{play_time, save_view, slot_rel, utc_time};
    let text = |key: &str| session.messages.get(key);
    let Some(view) = save_view(session) else { return PanelEvent::None };
    let mut out: Vec<(String, CommandArgs)> = Vec::new();
    frame.scroll(|f| {
        f.padded(|f| {
            f.section(&view.slot, |f| match &view.save {
                Ok(save) => {
                    f.readout(&text("asset-save-saved"), &utc_time(save.saved_at));
                    f.readout(&text("asset-save-played"), &play_time(save.play_time));
                    let version = if save.game_version < view.project_version {
                        session.messages.format(
                            "asset-save-older",
                            &[
                                ("version", save.game_version.to_string().as_str()),
                                ("current", view.project_version.to_string().as_str()),
                            ],
                        )
                    } else {
                        save.game_version.to_string()
                    };
                    f.readout(&text("asset-save-version"), &version);
                    f.readout(&text("asset-save-objects"), &save.entities.len().to_string());
                    if !save.state.is_empty() {
                        f.readout(&text("asset-save-state"), &save.state);
                    }
                    for (name, score) in &save.scores {
                        f.readout(name, &score.to_string());
                    }
                    if !save.values.is_empty() {
                        f.muted(&text("asset-save-values"));
                        for (key, value) in &save.values {
                            f.readout(key, value);
                        }
                    }
                }
                Err(e) => {
                    f.muted(&session.messages.format("asset-save-broken", &[("error", e.as_str())]))
                }
            });
            f.section(&text("asset-save-slots"), |f| {
                for info in &view.slots {
                    f.row(|f| {
                        let here = info.slot == view.slot;
                        if f.chip(&info.slot, here) && !here {
                            let rel = slot_rel(&info.slot);
                            out.push((
                                "asset.open".into(),
                                CommandArgs::new(&[("path", rel.as_str())]),
                            ));
                        }
                        let when = match &info.problem {
                            Some(_) => text("asset-save-unreadable"),
                            None => utc_time(info.saved_at),
                        };
                        f.muted(&when);
                        let tip = session
                            .messages
                            .format("asset-save-delete", &[("slot", info.slot.as_str())]);
                        if f.icon_button(seal_ui::Icon::EditDelete, &tip) {
                            out.push((
                                "asset.save_delete".into(),
                                CommandArgs::new(&[("slot", info.slot.as_str())]),
                            ));
                        }
                    });
                }
            });
            f.section(&text("asset-save-project"), |f| {
                let mut v = view.project_version as f32;
                if f.number(&text("asset-save-project-version"), &mut v, 1.0)
                    && v.round() as u32 != view.project_version
                {
                    let s = (v.round().max(1.0) as u32).to_string();
                    out.push((
                        "asset.save_version".into(),
                        CommandArgs::new(&[("value", s.as_str())]),
                    ));
                }
                f.muted(&text("asset-save-hint"));
                let mut menu_pick = None;
                let encrypt = if view.encrypt {
                    seal_ui::PropertyCheckbox::On
                } else {
                    seal_ui::PropertyCheckbox::Off
                };
                if let Some(next) = f.property_checkbox_row(
                    &text("asset-save-encrypt"),
                    encrypt,
                    view.encrypt,
                    None,
                    None,
                    &mut menu_pick,
                    None,
                ) {
                    let value = if next == seal_ui::PropertyCheckbox::On { "true" } else { "false" };
                    out.push(("asset.save_encrypt".into(), CommandArgs::new(&[("value", value)])));
                }
                let mut key = view.key.clone();
                if f.property(&text("asset-save-key"), &mut key) && key != view.key {
                    out.push(("asset.save_key".into(), CommandArgs::new(&[("value", key.as_str())])));
                }
                f.muted(&text("asset-save-key-hint"));
                let cloud = if view.cloud {
                    seal_ui::PropertyCheckbox::On
                } else {
                    seal_ui::PropertyCheckbox::Off
                };
                if let Some(next) = f.property_checkbox_row(
                    &text("asset-cloud-save"),
                    cloud,
                    view.cloud,
                    None,
                    None,
                    &mut menu_pick,
                    None,
                ) {
                    let value = if next == seal_ui::PropertyCheckbox::On { "true" } else { "false" };
                    out.push(("asset.cloud_save".into(), CommandArgs::new(&[("value", value)])));
                }
                let http = text("asset-cloud-http");
                let script = text("asset-cloud-script");
                let options = [("http", http), ("script", script)];
                if let Some(picked) = f.choice(&text("asset-cloud-provider"), &view.provider, &options)
                {
                    if picked != view.provider {
                        out.push((
                            "asset.cloud_provider".into(),
                            CommandArgs::new(&[("value", picked.as_str())]),
                        ));
                    }
                }
                let mut url = view.url.clone();
                if f.property(&text("asset-cloud-url"), &mut url) && url != view.url {
                    out.push(("asset.cloud_url".into(), CommandArgs::new(&[("value", url.as_str())])));
                }
                f.muted(&text("asset-cloud-hint"));
                f.readout(&text("asset-cloud-queued"), &view.queued.to_string());
            });
        })
    });
    if out.is_empty() {
        PanelEvent::None
    } else {
        PanelEvent::Commands(out)
    }
}

// ---------------------------------------------------------------- fonts

/// The sample line the font preview shows; kept as a draft so it survives restarts.
const FONT_SAMPLE_DRAFT: &str = "asset-font-sample";
const FONT_SIZES: &[f32] = &[12.0, 18.0, 24.0, 36.0, 48.0, 72.0];

fn font_editor(
    frame: &mut PanelFrame,
    session: &mut EditorSession,
    rel: &str,
    full: &Path,
) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let mut event = PanelEvent::None;
    let bytes = std::fs::read(full).unwrap_or_default();
    let info = match seal_editor_core::fonts::font_info(&bytes) {
        Ok(info) => info,
        Err(why) => {
            frame.padded(|frame| frame.muted(&text(why)));
            return event;
        }
    };
    let ctx = frame.ui_mut().ctx().clone();
    let family = seal_ui::game_fonts::game_font(&ctx, &full.to_string_lossy());
    if family.is_none() {
        ctx.request_repaint();
    }
    let glyphs = info.glyphs.to_string();
    let summary = session.messages.format(
        "asset-font-summary",
        &[
            ("family", info.family.as_str()),
            ("style", info.style.as_str()),
            ("glyphs", glyphs.as_str()),
        ],
    );
    let sample =
        session.draft(FONT_SAMPLE_DRAFT).unwrap_or_else(|| text("asset-font-sample-default"));
    frame.padded(|frame| {
        frame.label(&summary);
        frame.muted(&session.messages.format("asset-font-use", &[("path", rel)]));
        let ui = frame.ui_mut();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(text("asset-font-sample"));
            let mut line = sample.clone();
            let edit = ui.add(
                egui::TextEdit::singleline(&mut line).desired_width(ui.available_width() - 8.0),
            );
            if edit.changed() {
                event = PanelEvent::Commands(vec![draft_cmd(FONT_SAMPLE_DRAFT, &line)]);
            }
        });
        ui.add_space(8.0);
        egui::ScrollArea::vertical().id_salt("asset-font-sizes").auto_shrink([false, false]).show(
            ui,
            |ui| {
                for &size in FONT_SIZES {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [44.0, size.max(16.0)],
                            egui::Label::new(egui::RichText::new(format!("{size}")).weak()),
                        );
                        let fam = family.clone().unwrap_or(egui::FontFamily::Proportional);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&sample)
                                    .font(egui::FontId::new(size, fam))
                                    .color(Color32::from_gray(235)),
                            )
                            .truncate(),
                        );
                    });
                    ui.add_space(4.0);
                }
            },
        );
    });
    let imp = crate::import_preset_ui::import_settings_block(frame, session, rel);
    if !matches!(imp, PanelEvent::None) {
        event = imp;
    }
    event
}

fn human_size(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1u64 << 20) as f64),
        b if b >= 1 << 10 => format!("{:.1} KB", b as f64 / 1024.0),
        b => format!("{b} B"),
    }
}

// ---------------------------------------------------------------- pictures

type Pixels = (
    Option<std::time::SystemTime>,
    Option<std::time::SystemTime>,
    Option<std::sync::Arc<(Vec<u8>, u32, u32)>>,
);

/// Decoded pixels of a picture, kept until the file or its import sidecar changes.
fn pixels(ctx: &egui::Context, path: &Path) -> Option<std::sync::Arc<(Vec<u8>, u32, u32)>> {
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let side = std::fs::metadata(format!("{}.import.seal", path.display()))
        .and_then(|m| m.modified())
        .ok();
    let id = egui::Id::new(("asset-pixels", path));
    if let Some((seen, seen_side, data)) = ctx.data(|d| d.get_temp::<Pixels>(id)) {
        if seen == mtime && seen_side == side {
            return data;
        }
    }
    let data = seal_ui::sprites::decode_image(path).map(std::sync::Arc::new);
    ctx.data_mut(|d| d.insert_temp::<Pixels>(id, (mtime, side, data.clone())));
    data
}

fn image_editor(
    frame: &mut PanelFrame,
    session: &mut EditorSession,
    rel: &str,
    full: &Path,
) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let mut event = PanelEvent::None;
    let ctx = frame.ui_mut().ctx().clone();
    let path = full.to_string_lossy().to_string();
    let texture = seal_ui::sprites::texture(&ctx, &path);
    let data = pixels(&ctx, full);
    let Some((texture, data)) = texture.zip(data) else {
        frame.padded(|frame| frame.muted(&text("asset-image-unreadable")));
        return event;
    };
    let (w, h) = (data.1, data.2);
    let zoom: f32 = session
        .draft(ASSET_ZOOM_DRAFT)
        .and_then(|z| z.parse::<f32>().ok())
        .unwrap_or(0.0)
        .clamp(0.0, 64.0);
    let grid = session.draft(ASSET_GRID_DRAFT).as_deref() == Some("1");
    let selected = session.models.selection.clone().filter(|id| id != "root");
    frame.bar(|frame| {
        frame.muted(
            &session
                .messages
                .format("asset-image-size", &[("w", &w.to_string()), ("h", &h.to_string())]),
        );
        let set_zoom =
            |z: f32| PanelEvent::Commands(vec![draft_cmd(ASSET_ZOOM_DRAFT, &z.to_string())]);
        if frame.chip(&text("asset-zoom-fit"), zoom == 0.0) {
            event = set_zoom(0.0);
        }
        if frame.chip(&text("asset-zoom-1"), zoom == 1.0) {
            event = set_zoom(1.0);
        }
        let current = if zoom == 0.0 { 1.0 } else { zoom };
        if frame.button(&text("asset-zoom-out")) {
            event = set_zoom((current / 2.0).max(0.125));
        }
        if frame.button(&text("asset-zoom-in")) {
            event = set_zoom((current * 2.0).min(64.0));
        }
        if frame.toggle(&text("asset-grid"), grid) {
            event = PanelEvent::Commands(vec![draft_cmd(
                ASSET_GRID_DRAFT,
                if grid { "0" } else { "1" },
            )]);
        }
        if let Some(id) = selected.as_deref() {
            if frame.button(&text("asset-use-sprite")) {
                event =
                    command("inspector.set", &[("id", id), ("field", "sprite"), ("value", rel)]);
            }
        }
        if frame.button(&text("asset-make-animation")) {
            let stem = rel.rsplit_once('.').map(|(s, _)| s).unwrap_or(rel);
            event = command(
                "asset.frames_new",
                &[("path", stem), ("image", rel), ("mode", "auto"), ("unique", "yes")],
            );
        }
        if session.world.mode == seal_editor_core::PlayMode::Editing
            && frame.button(&text("asset-place"))
        {
            event = command("scene.add", &[("kind", &format!("sprite:{path}"))]);
        }
    });
    // Above the canvas: the canvas takes all the height that is left.
    let imp = crate::import_preset_ui::import_settings_block(frame, session, rel);
    if !matches!(imp, PanelEvent::None) {
        event = imp;
    }
    let ui = frame.ui_mut();
    let avail = ui.available_size();
    let fit = ((avail.x - 16.0) / w as f32).min((avail.y - 30.0) / h as f32).max(0.01);
    let scale = if zoom == 0.0 { fit } else { zoom };
    let size = Vec2::new(w as f32 * scale, h as f32 * scale);
    let mut readout = None;
    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
        let canvas = size.max(ui.available_size() - Vec2::new(0.0, 24.0));
        let (rect, response) = ui.allocate_exact_size(canvas, Sense::hover());
        // Snap to whole screen pixels so the checkerboard and grid lines land on
        // the same edges.
        let img =
            Rect::from_min_size(Rect::from_center_size(rect.center(), size).min.round(), size);
        let painter = ui.painter_at(rect);
        // Checkerboard so transparent pixels read as transparent. Its squares
        // follow the picture's pixels: whole pixels per square when zoomed out,
        // an even split of each pixel when zoomed in, so they match the grid.
        let cell = checker_cell(scale);
        let (a, b) = (Color32::from_gray(58), Color32::from_gray(74));
        let mut y = img.top();
        let mut row = 0;
        while y < img.bottom() {
            let mut x = img.left();
            let mut col = row;
            while x < img.right() {
                let r = Rect::from_min_max(
                    Pos2::new(x, y),
                    Pos2::new((x + cell).min(img.right()), (y + cell).min(img.bottom())),
                );
                painter.rect_filled(r, 0.0, if col % 2 == 0 { a } else { b });
                x += cell;
                col += 1;
            }
            y += cell;
            row += 1;
        }
        seal_ui::sprites::paint_quad(
            &painter,
            texture,
            [img.left_top(), img.right_top(), img.right_bottom(), img.left_bottom()],
        );
        if grid && scale >= 4.0 {
            let stroke = Stroke::new(1.0, Color32::from_black_alpha(90));
            for i in 0..=w {
                let x = img.left() + i as f32 * scale;
                painter.line_segment([Pos2::new(x, img.top()), Pos2::new(x, img.bottom())], stroke);
            }
            for j in 0..=h {
                let y = img.top() + j as f32 * scale;
                painter.line_segment([Pos2::new(img.left(), y), Pos2::new(img.right(), y)], stroke);
            }
        }
        painter.rect_stroke(
            img,
            0.0,
            Stroke::new(1.0, Color32::from_gray(30)),
            egui::StrokeKind::Outside,
        );
        if let Some(pos) = response.hover_pos().filter(|p| img.contains(*p)) {
            let px = (((pos.x - img.left()) / scale) as u32).min(w - 1);
            let py = (((pos.y - img.top()) / scale) as u32).min(h - 1);
            let i = ((py * w + px) * 4) as usize;
            if let Some(c) = data.0.get(i..i + 4) {
                readout =
                    Some((px, py, format!("#{:02X}{:02X}{:02X}{:02X}", c[0], c[1], c[2], c[3])));
            }
            if scale >= 4.0 {
                let cell = Rect::from_min_size(
                    Pos2::new(img.left() + px as f32 * scale, img.top() + py as f32 * scale),
                    Vec2::splat(scale),
                );
                painter.rect_stroke(
                    cell,
                    0.0,
                    Stroke::new(1.0, Color32::WHITE),
                    egui::StrokeKind::Inside,
                );
            }
        }
    });
    if let Some((x, y, rgba)) = readout {
        frame.hint(&session.messages.format(
            "asset-pixel",
            &[("x", &x.to_string()), ("y", &y.to_string()), ("rgba", &rgba)],
        ));
    }
    event
}

// ---------------------------------------------------------------- sounds

type Wave = (
    Option<std::time::SystemTime>,
    Option<std::time::SystemTime>,
    Option<std::sync::Arc<WaveInfo>>,
);

struct WaveInfo {
    rate: u32,
    channels: u16,
    seconds: f32,
    /// (min, max) of each slice of the sound, left to right.
    peaks: Vec<(f32, f32)>,
}

fn wave(ctx: &egui::Context, path: &Path) -> Option<std::sync::Arc<WaveInfo>> {
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let side = std::fs::metadata(format!("{}.import.seal", path.display()))
        .and_then(|m| m.modified())
        .ok();
    let id = egui::Id::new(("asset-wave", path));
    if let Some((seen, seen_side, info)) = ctx.data(|d| d.get_temp::<Wave>(id)) {
        if seen == mtime && seen_side == side {
            return info;
        }
    }
    let info = std::fs::read(path)
        .ok()
        .and_then(|bytes| Sound::from_bytes(&bytes, Some(path)).ok())
        .map(|sound| {
            let sound = seal_editor_core::import_presets::finish_sound(path, sound);
            let frames = sound.frames().max(1);
            let ch = sound.channels.max(1) as usize;
            let slices = 600.min(frames);
            let peaks = (0..slices)
                .map(|s| {
                    let (a, b) = (
                        s * frames / slices,
                        ((s + 1) * frames / slices).max(s * frames / slices + 1),
                    );
                    let mut lo = 0.0f32;
                    let mut hi = 0.0f32;
                    for f in a..b.min(frames) {
                        let v = sound.samples[f * ch];
                        lo = lo.min(v);
                        hi = hi.max(v);
                    }
                    (lo, hi)
                })
                .collect();
            std::sync::Arc::new(WaveInfo {
                rate: sound.sample_rate,
                channels: sound.channels,
                seconds: frames as f32 / sound.sample_rate.max(1) as f32,
                peaks,
            })
        });
    ctx.data_mut(|d| d.insert_temp::<Wave>(id, (mtime, side, info.clone())));
    info
}

fn sound_editor(
    frame: &mut PanelFrame,
    session: &mut EditorSession,
    rel: &str,
    full: &Path,
) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let mut event = PanelEvent::None;
    let ctx = frame.ui_mut().ctx().clone();
    let Some(info) = wave(&ctx, full) else {
        frame.padded(|frame| {
            frame.muted(&session.messages.format("asset-sound-unsupported", &[("name", rel)]));
        });
        return event;
    };
    let position =
        session.audio_playing.as_ref().filter(|p| p.rel == rel).and_then(|p| p.position());
    let mut volume: f32 =
        session.draft(ASSET_VOLUME_DRAFT).and_then(|v| v.parse().ok()).unwrap_or(0.8);
    frame.bar(|frame| {
        let channels = info.channels.to_string();
        frame.muted(&session.messages.format(
            "asset-sound-info",
            &[
                ("seconds", &format!("{:.2}", info.seconds)),
                ("rate", &info.rate.to_string()),
                ("channels", channels.as_str()),
            ],
        ));
        if position.is_some() {
            if frame.button(&text("asset-sound-stop")) {
                event = command("asset.sound_stop", &[]);
            }
        } else if frame.button(&text("asset-sound-play")) {
            event = command("asset.sound_play", &[]);
        }
    });
    frame.padded(|frame| {
        let before = volume;
        let mut menu_pick = None;
        let spec = seal_ui::NumberFieldSpec {
            min: Some(0.0),
            max: Some(1.0),
            unit_suffix: Some(text("inspector-unit-scalar")),
            range_bar: true,
            initial_text: None,
            level_led: false,
        };
        let (changed, _reset, drag) = frame.number_field_row(
            &text("asset-volume"),
            &mut volume,
            0.01,
            false,
            None,
            None,
            None,
            &mut menu_pick,
            &spec,
            None,
        );
        if changed || drag.drag_stopped || drag.commit_focus {
            let v = volume.clamp(0.0, 1.0);
            if (v - before).abs() > f32::EPSILON || drag.drag_stopped || drag.commit_focus {
                event =
                    PanelEvent::Commands(vec![draft_cmd(ASSET_VOLUME_DRAFT, &format!("{v:.2}"))]);
            }
        }
    });
    let ui = frame.ui_mut();
    let size =
        Vec2::new(ui.available_width() - 16.0, (ui.available_height() - 16.0).clamp(60.0, 260.0));
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
    let mid = rect.center().y;
    let half = rect.height() * 0.45;
    let ink = ui.visuals().selection.bg_fill;
    let n = info.peaks.len().max(1) as f32;
    for (i, (lo, hi)) in info.peaks.iter().enumerate() {
        let x = rect.left() + (i as f32 + 0.5) / n * rect.width();
        painter.line_segment(
            [Pos2::new(x, mid - hi * half), Pos2::new(x, mid - lo * half - 0.5)],
            Stroke::new((rect.width() / n).max(1.0), ink),
        );
    }
    painter.line_segment(
        [Pos2::new(rect.left(), mid), Pos2::new(rect.right(), mid)],
        Stroke::new(1.0, Color32::from_white_alpha(30)),
    );
    if let Some(t) = position {
        let x = rect.left() + (t / info.seconds.max(0.001)).clamp(0.0, 1.0) * rect.width();
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(2.0, Color32::WHITE),
        );
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }
    let imp = crate::import_preset_ui::import_settings_block(frame, session, rel);
    if !matches!(imp, PanelEvent::None) {
        event = imp;
    }
    event
}

// ---------------------------------------------------------------- effects

#[derive(Clone)]
struct Preview {
    def: EmitterDef,
    emitter: Emitter,
}

fn effect_editor(
    frame: &mut PanelFrame,
    session: &EditorSession,
    rel: &str,
    full: &Path,
) -> PanelEvent {
    let text = |key: &str| session.messages.get(key);
    let mut event = PanelEvent::None;
    let raw = std::fs::read_to_string(full).unwrap_or_default();
    let as_text = session.draft("asset-fx-text").as_deref() == Some("1");
    let parsed = EmitterDef::parse(&raw);
    if as_text || parsed.is_err() {
        frame.bar(|frame| {
            if let Err(error) = &parsed {
                frame.muted(
                    &session
                        .messages
                        .format("asset-fx-error", &[("name", rel), ("error", error.as_str())]),
                );
            }
            if parsed.is_ok() && frame.toggle(&text("asset-fx-edit-text"), true) {
                event = PanelEvent::Commands(vec![draft_cmd("asset-fx-text", "0")]);
            }
        });
        let body = text_editor(frame, session, rel, full);
        if !matches!(body, PanelEvent::None) {
            event = body;
        }
        return event;
    }
    let Ok(def) = parsed else { return event };
    let defaults = EmitterDef::default();
    let selected = session.models.selection.clone().filter(|id| id != "root");
    frame.bar(|frame| {
        if frame.toggle(&text("asset-fx-edit-text"), false) {
            event = PanelEvent::Commands(vec![draft_cmd("asset-fx-text", "1")]);
        }
        if frame.button(&text("asset-fx-reset")) {
            event = command("asset.fx_reset", &[]);
        }
        if let Some(id) = selected.as_deref() {
            if frame.button(&text("asset-fx-use")) {
                event =
                    command("inspector.set", &[("id", id), ("field", "particles"), ("value", rel)]);
            }
        }
    });
    let mut set: Option<(String, f32)> = None;
    let mut color_set: Vec<(String, f32)> = Vec::new();
    let ctx = frame.ui_mut().ctx().clone();
    let preview_h = (frame.ui_mut().available_height() * 0.4).clamp(90.0, 240.0);
    // Live preview on top, settings below.
    {
        let ui = frame.ui_mut();
        let size = Vec2::new(ui.available_width() - 16.0, preview_h);
        ui.add_space(4.0);
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_gray(18));
        let id = egui::Id::new(("asset-fx-preview", rel));
        let mut preview = ctx
            .data(|d| d.get_temp::<Preview>(id))
            .filter(|p| p.def == def)
            .unwrap_or_else(|| Preview { def: def.clone(), emitter: Emitter::new(def.clone()) });
        let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.05);
        preview.emitter.step(dt, [0.0; 3]);
        let origin = Pos2::new(rect.center().x, rect.bottom() - rect.height() * 0.25);
        let ppu = 60.0;
        let project = session.project.as_ref().map(|p| p.root.as_path());
        for draw in seal_editor_core::asset_editor::collect_draws(&preview.emitter, project) {
            use seal_editor_core::asset_editor::ParticleDraw;
            match draw {
                ParticleDraw::Billboard { pos, half, color, .. }
                | ParticleDraw::Stretched { pos, half, color, .. } => {
                    let p = Pos2::new(origin.x + pos[0] * ppu, origin.y - pos[1] * ppu);
                    let c = color.rgba;
                    let col = Color32::from_rgba_unmultiplied(
                        (c[0].clamp(0.0, 1.0) * 255.0) as u8,
                        (c[1].clamp(0.0, 1.0) * 255.0) as u8,
                        (c[2].clamp(0.0, 1.0) * 255.0) as u8,
                        (c[3].clamp(0.0, 1.0) * 255.0) as u8,
                    );
                    painter.circle_filled(p, (half * ppu * 2.0).max(1.0), col);
                }
                ParticleDraw::Flipbook { pos, half, picture, uv, color, .. } => {
                    let p = Pos2::new(origin.x + pos[0] * ppu, origin.y - pos[1] * ppu);
                    let full = project
                        .map(|root| root.join(&picture))
                        .unwrap_or_else(|| PathBuf::from(&picture));
                    let tint = color.rgba.map(|c| (c.clamp(0.0, 1.0) * 255.0) as u8);
                    if let Some((texture, place, _)) =
                        seal_ui::sprites::sprite_texture(&ctx, full.to_str().unwrap_or(""))
                    {
                        let frame_uv = seal_ui::sprites::uv_within(place, uv);
                        let uv = egui::Rect::from_min_max(
                            Pos2::new(frame_uv[0], frame_uv[1]),
                            Pos2::new(frame_uv[2], frame_uv[3]),
                        );
                        let size = (half * ppu * 2.0).max(2.0);
                        let rect = egui::Rect::from_center_size(p, egui::vec2(size, size));
                        painter.image(
                            texture,
                            rect,
                            uv,
                            Color32::from_rgba_unmultiplied(tint[0], tint[1], tint[2], tint[3]),
                        );
                    } else {
                        painter.circle_filled(p, (half * ppu * 2.0).max(1.0), Color32::WHITE);
                    }
                }
                ParticleDraw::Mesh { pos, half, angle, mesh, color, .. } => {
                    use seal_editor_core::asset_editor::mesh_renderer_primitive;
                    let p = Pos2::new(origin.x + pos[0] * ppu, origin.y - pos[1] * ppu);
                    let c = color.rgba;
                    let col = Color32::from_rgba_unmultiplied(
                        (c[0].clamp(0.0, 1.0) * 255.0) as u8,
                        (c[1].clamp(0.0, 1.0) * 255.0) as u8,
                        (c[2].clamp(0.0, 1.0) * 255.0) as u8,
                        (c[3].clamp(0.0, 1.0) * 255.0) as u8,
                    );
                    let sz = (half * ppu * 2.0).max(2.0);
                    if mesh_renderer_primitive(&mesh) == Some("sphere") {
                        painter.circle_filled(p, sz * 0.5, col);
                    } else {
                        let rad = angle.to_radians();
                        let (s, co) = rad.sin_cos();
                        let hx = sz * 0.5 * co;
                        let hy = sz * 0.5 * s;
                        painter.add(egui::Shape::convex_polygon(
                            vec![
                                Pos2::new(p.x - hx, p.y - hy),
                                Pos2::new(p.x + hy, p.y - hx),
                                Pos2::new(p.x + hx, p.y + hy),
                                Pos2::new(p.x - hy, p.y + hx),
                            ],
                            col,
                            Stroke::NONE,
                        ));
                    }
                }
                ParticleDraw::Trail { .. } => {}
            }
        }
        painter.circle_stroke(origin, 3.0, Stroke::new(1.0, Color32::from_white_alpha(60)));
        ctx.data_mut(|d| d.insert_temp(id, preview));
        // About 30 frames a second is smooth enough and leaves the editor responsive.
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }
    frame.scroll(|frame| {
        frame.padded(|frame| {
            let mut num =
                |frame: &mut PanelFrame, label: &str, key: &str, v: f32, d: f32, step: f32| {
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
                        set = Some((key.to_string(), d));
                    } else if changed {
                        set = Some((key.to_string(), value));
                    }
                };
            num(frame, &text("asset-fx-rate"), "rate", def.rate, defaults.rate, 0.5);
            num(
                frame,
                &text("asset-fx-lifetime"),
                "lifetime",
                def.lifetime,
                defaults.lifetime,
                0.05,
            );
            for (i, axis) in ["x", "y", "z"].iter().enumerate() {
                let label = format!("{} {}", text("asset-fx-velocity"), axis.to_uppercase());
                num(
                    frame,
                    &label,
                    &format!("velocity.{axis}"),
                    def.velocity[i],
                    defaults.velocity[i],
                    0.05,
                );
            }
            num(frame, &text("asset-fx-spread"), "spread", def.spread, defaults.spread, 0.02);
            for (i, axis) in ["x", "y", "z"].iter().enumerate() {
                let label = format!("{} {}", text("asset-fx-gravity"), axis.to_uppercase());
                num(
                    frame,
                    &label,
                    &format!("gravity.{axis}"),
                    def.gravity[i],
                    defaults.gravity[i],
                    0.05,
                );
            }
            let size = text("asset-fx-size");
            num(frame, &format!("{size} 1"), "size.start", def.size.0, defaults.size.0, 0.01);
            num(frame, &format!("{size} 2"), "size.end", def.size.1, defaults.size.1, 0.01);
            for (end, label, rgba, d) in [
                ("start", text("asset-fx-color-start"), def.color.0, defaults.color.0),
                ("end", text("asset-fx-color-end"), def.color.1, defaults.color.1),
            ] {
                let mut rgb = [rgba[0], rgba[1], rgba[2]];
                let before = rgb;
                if frame.color_rgb(&label, &mut rgb, false, "").is_some() {
                    for (index, channel) in ["r", "g", "b"].into_iter().enumerate() {
                        if (rgb[index] - before[index]).abs() > 0.0001 {
                            color_set.push((format!("color.{end}.{channel}"), rgb[index]));
                        }
                    }
                }
                num(
                    frame,
                    &format!("{label} alpha"),
                    &format!("color.{end}.a"),
                    rgba[3],
                    d[3],
                    0.01,
                );
            }
            let curves = crate::curve_fields::effect_curves(frame, session, rel);
            if !matches!(curves, PanelEvent::None) {
                event = curves;
            }
            let extra = crate::fx_extra_fields::effect_extra_fields(
                frame,
                session,
                &def,
                &defaults,
                Some(rel),
            );
            if !matches!(extra, PanelEvent::None) {
                event = extra;
            }
            num(frame, &text("asset-fx-max"), "max", def.max as f32, defaults.max as f32, 1.0);
            num(frame, &text("asset-fx-seed"), "seed", def.seed as f32, defaults.seed as f32, 1.0);
            frame.hint(&text("asset-saved-hint"));
        });
    });
    if let Some((key, value)) = set {
        event = command("asset.fx_set", &[("key", key.as_str()), ("value", &value.to_string())]);
    } else if color_set.len() == 1 {
        let (key, value) = &color_set[0];
        event = command("asset.fx_set", &[("key", key.as_str()), ("value", &value.to_string())]);
    } else if !color_set.is_empty() {
        event = PanelEvent::Commands(
            color_set
                .iter()
                .map(|(key, value)| {
                    (
                        "asset.fx_set".into(),
                        CommandArgs::new(&[("key", key.as_str()), ("value", &value.to_string())]),
                    )
                })
                .collect(),
        );
    }
    event
}

// ---------------------------------------------------------------- animations

const ANIM_SRC_DRAFT: &str = "asset-anim-src";
const ANIM_COLS_DRAFT: &str = "asset-anim-cols";
const ANIM_ROWS_DRAFT: &str = "asset-anim-rows";
const ANIM_PAUSED_DRAFT: &str = "asset-anim-paused";

/// Project path of a frame picture written relative to the animation file.
fn frame_picture(anim_rel: &str, image: &str) -> String {
    let mut parts: Vec<&str> =
        anim_rel.rsplit_once('/').map(|(d, _)| d.split('/').collect()).unwrap_or_default();
    for p in image.split('/') {
        match p {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            p => parts.push(p),
        }
    }
    parts.join("/")
}

/// Draw one frame of a sprite animation into `rect`, keeping its aspect.
fn paint_frame(
    ui: &egui::Ui,
    painter: &egui::Painter,
    root: &Path,
    anim_rel: &str,
    f: &seal_editor_core::sprite_frames::Frame,
    rect: Rect,
) -> Option<Rect> {
    let path = root.join(frame_picture(anim_rel, &f.image));
    let Some(texture) = seal_ui::sprites::texture(ui.ctx(), &path.to_string_lossy()) else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "?",
            egui::FontId::proportional(14.0),
            Color32::from_gray(140),
        );
        return None;
    };
    let Some(size) = pixels(ui.ctx(), &path) else { return None };
    let (tw, th) = (size.1, size.2);
    let [u0, v0, u1, v1] = f.uv;
    let (w, h) = (((u1 - u0).abs() * tw as f32).max(1.0), ((v1 - v0).abs() * th as f32).max(1.0));
    let scale = (rect.width() / w).min(rect.height() / h);
    let dest = Rect::from_center_size(rect.center(), Vec2::new(w * scale, h * scale));
    let dest = Rect::from_min_size(dest.min.round(), dest.size().round());
    let uv = Rect::from_min_max(Pos2::new(u0, v0), Pos2::new(u1, v1));
    painter.image(texture, dest, uv, Color32::WHITE);
    let pivot =
        Pos2::new(dest.min.x + f.pivot[0] * dest.width(), dest.max.y - f.pivot[1] * dest.height());
    painter.circle_filled(pivot, 3.0, Color32::from_rgb(255, 220, 80));
    Some(dest)
}

fn animation_editor(
    frame: &mut PanelFrame,
    session: &EditorSession,
    rel: &str,
    full: &Path,
) -> PanelEvent {
    use seal_editor_core::sprite_frames::SpriteFrames;
    let text = |key: &str| session.messages.get(key);
    let mut event = PanelEvent::None;
    let lib = match SpriteFrames::load(full) {
        Ok(lib) => lib,
        Err(e) => {
            frame.padded(|frame| {
                frame.muted(
                    &session.messages.format("asset-anim-unreadable", &[("error", &e.to_string())]),
                )
            });
            return event;
        }
    };
    // Project root: walk up one folder per '/' in the project-relative path.
    let mut root = full.parent().map(Path::to_path_buf).unwrap_or_default();
    for _ in 0..rel.matches('/').count() {
        root = root.parent().map(Path::to_path_buf).unwrap_or(root);
    }
    let picked = picked_clip(session, &lib);
    let clip = picked.as_deref().and_then(|n| lib.clip(n)).cloned();
    let clip_name = picked.clone().unwrap_or_default();
    let selected = session.models.selection.clone().filter(|id| id != "root");
    let paused = session.draft(ANIM_PAUSED_DRAFT).as_deref() == Some("1");
    let frame_id = egui::Id::new(("asset-anim-frame", rel, clip_name.clone()));
    let ctx = frame.ui_mut().ctx().clone();
    let mut picked_frame: usize = ctx.data(|d| d.get_temp(frame_id)).unwrap_or(0);

    // Clip row: one chip per clip, add and delete.
    frame.bar(|frame| {
        frame.muted(&text("asset-anim-clips"));
        for c in &lib.clips {
            if frame.chip(&c.name, Some(&c.name) == picked.as_ref()) {
                event = PanelEvent::Commands(vec![draft_cmd(ASSET_CLIP_DRAFT, &c.name)]);
            }
        }
        if frame.button(&text("asset-anim-add-clip")) {
            event = command("asset.frames_clip_add", &[("name", "clip")]);
        }
        if clip.is_some() && frame.button(&text("asset-anim-delete-clip")) {
            event = command("asset.frames_clip_remove", &[("clip", clip_name.as_str())]);
        }
        if let Some(id) = selected.as_deref() {
            if frame.button(&text("asset-anim-use")) {
                event =
                    command("inspector.set", &[("id", id), ("field", "sprite"), ("value", rel)]);
            }
        }
    });
    let Some(clip) = clip else {
        frame.padded(|frame| frame.empty(&text("asset-anim-no-frames")));
        return event;
    };
    if picked_frame >= clip.frames.len() {
        picked_frame = clip.frames.len().saturating_sub(1);
    }

    // Live preview: plays the clip unless paused, then shows the picked frame.
    let preview_h = (frame.ui_mut().available_height() * 0.38).clamp(90.0, 260.0);
    {
        let ui = frame.ui_mut();
        ui.add_space(4.0);
        let size = Vec2::new(ui.available_width() - 16.0, preview_h);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_gray(18));
        let shown_index = if paused || clip.frames.is_empty() {
            picked_frame
        } else {
            let t = ctx.input(|i| i.time) as f32;
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
            clip.frame_at(t % (clip.length() + if clip.looping { 0.0 } else { 0.5 }).max(0.001))
        };
        let shown = clip.frames.get(shown_index);
        let dest = shown.and_then(|f| paint_frame(ui, &painter, &root, rel, f, rect.shrink(10.0)));
        if let (Some(f), Some(dest)) = (shown, dest) {
            let socket_id = egui::Id::new(("asset-anim-socket", rel, clip_name.clone()));
            let mut selected: String = ctx.data(|d| d.get_temp(socket_id)).unwrap_or_default();
            for socket in &f.sockets {
                let p =
                    dest.min + Vec2::new(socket.at[0] * dest.width(), socket.at[1] * dest.height());
                let on = socket.name == selected;
                painter.circle_filled(
                    p,
                    if on { 6.0 } else { 4.0 },
                    if on {
                        Color32::from_rgb(255, 196, 64)
                    } else {
                        Color32::from_rgb(80, 180, 255)
                    },
                );
            }
            if response.dragged() && paused && f.sockets.is_empty() {
                // A paused frame without sockets: dragging sets the frame's pivot.
                if let Some(pos) = response.interact_pointer_pos() {
                    let u = ((pos.x - dest.min.x) / dest.width().max(1.0)).clamp(0.0, 1.0);
                    let v = 1.0 - ((pos.y - dest.min.y) / dest.height().max(1.0)).clamp(0.0, 1.0);
                    let pivot = format!("{u},{v}");
                    let index = shown_index.to_string();
                    event = command(
                        "asset.frames_frame",
                        &[
                            ("clip", clip_name.as_str()),
                            ("index", index.as_str()),
                            ("action", "set"),
                            ("pivot", pivot.as_str()),
                        ],
                    );
                }
            } else if response.dragged() {
                if let Some(pos) = response.interact_pointer_pos() {
                    if selected.is_empty() || !f.sockets.iter().any(|s| s.name == selected) {
                        selected = f
                            .sockets
                            .iter()
                            .min_by(|a, b| {
                                let da = (dest.min
                                    + Vec2::new(a.at[0] * dest.width(), a.at[1] * dest.height())
                                    - pos)
                                    .length();
                                let db = (dest.min
                                    + Vec2::new(b.at[0] * dest.width(), b.at[1] * dest.height())
                                    - pos)
                                    .length();
                                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                            })
                            .map(|s| s.name.clone())
                            .unwrap_or_default();
                    }
                    if !selected.is_empty() && dest.width() > 0.0 && dest.height() > 0.0 {
                        let u = ((pos.x - dest.min.x) / dest.width()).clamp(0.0, 1.0);
                        let v = ((pos.y - dest.min.y) / dest.height()).clamp(0.0, 1.0);
                        let index = shown_index.to_string();
                        let u_text = format!("{u:.4}");
                        let v_text = format!("{v:.4}");
                        event = command(
                            "asset.frames_socket",
                            &[
                                ("clip", clip_name.as_str()),
                                ("action", "move"),
                                ("name", selected.as_str()),
                                ("index", index.as_str()),
                                ("u", u_text.as_str()),
                                ("v", v_text.as_str()),
                            ],
                        );
                    }
                }
            } else if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    if let Some(name) = f.sockets.iter().min_by(|a, b| {
                        let da = (dest.min
                            + Vec2::new(a.at[0] * dest.width(), a.at[1] * dest.height())
                            - pos)
                            .length();
                        let db = (dest.min
                            + Vec2::new(b.at[0] * dest.width(), b.at[1] * dest.height())
                            - pos)
                            .length();
                        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                    }) {
                        selected = name.name.clone();
                    }
                }
            }
            ctx.data_mut(|d| d.insert_temp(socket_id, selected));
        }
    }

    frame.bar(|frame| {
        let label = if paused { text("asset-anim-play") } else { text("asset-anim-pause") };
        if frame.button(&label) {
            event = PanelEvent::Commands(vec![draft_cmd(
                ANIM_PAUSED_DRAFT,
                if paused { "0" } else { "1" },
            )]);
        }
        frame.muted(
            &session
                .messages
                .format("asset-anim-frames", &[("count", &clip.frames.len().to_string())]),
        );
        if !clip.frames.is_empty() {
            let i = picked_frame.to_string();
            let act = |a: &str| {
                command(
                    "asset.frames_frame",
                    &[("clip", clip_name.as_str()), ("index", i.as_str()), ("action", a)],
                )
            };
            if frame.button(&text("asset-anim-frame-left")) {
                event = act("left");
                picked_frame = picked_frame.saturating_sub(1);
            }
            if frame.button(&text("asset-anim-frame-right")) {
                event = act("right");
                picked_frame = (picked_frame + 1).min(clip.frames.len() - 1);
            }
            if frame.button(&text("asset-anim-frame-copy")) {
                event = act("copy");
            }
            if frame.button(&text("asset-anim-frame-remove")) {
                event = act("remove");
            }
        }
    });

    // Frame strip: a thumbnail per frame; click picks it.
    {
        let ui = frame.ui_mut();
        let thumb = 56.0;
        egui::ScrollArea::horizontal()
            .id_salt(("asset-anim-strip", rel))
            .max_height(thumb + 22.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    for (i, f) in clip.frames.iter().enumerate() {
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::new(thumb, thumb + 14.0), Sense::click());
                        let painter = ui.painter_at(rect);
                        let cell = Rect::from_min_size(rect.min, Vec2::splat(thumb));
                        painter.rect_filled(cell, 0.0, Color32::from_gray(24));
                        paint_frame(ui, &painter, &root, rel, f, cell.shrink(4.0));
                        let on = i == picked_frame;
                        let edge = if on {
                            Color32::from_rgb(0x4A, 0x90, 0xD9)
                        } else {
                            Color32::from_gray(52)
                        };
                        painter.rect_stroke(
                            cell,
                            0.0,
                            Stroke::new(if on { 2.0 } else { 1.0 }, edge),
                            egui::StrokeKind::Inside,
                        );
                        painter.text(
                            Pos2::new(cell.center().x, cell.bottom() + 7.0),
                            egui::Align2::CENTER_CENTER,
                            (i + 1).to_string(),
                            egui::FontId::proportional(10.0),
                            Color32::from_gray(150),
                        );
                        if response.clicked() {
                            picked_frame = i;
                        }
                    }
                });
            });
    }
    ctx.data_mut(|d| d.insert_temp(frame_id, picked_frame));

    frame.scroll(|frame| {
        frame.padded(|frame| {
            // Clip settings.
            let name_id = egui::Id::new(("asset-anim-name", rel, clip_name.clone()));
            let mut name: String =
                ctx.data(|d| d.get_temp(name_id)).unwrap_or_else(|| clip_name.clone());
            let (changed, committed) = frame.draft_field(&text("asset-anim-name"), &mut name);
            if changed {
                ctx.data_mut(|d| d.insert_temp(name_id, name.clone()));
            }
            if committed && name.trim() != clip_name {
                ctx.data_mut(|d| d.remove::<String>(name_id));
                event = command(
                    "asset.frames_clip_set",
                    &[("clip", clip_name.as_str()), ("name", name.trim())],
                );
            }
            let mut fps = clip.fps;
            let tip = session.messages.format("inspector-reset-default", &[("value", "8")]);
            let mut pick = None;
            let (changed, reset, _) = frame.number_row(
                &text("asset-anim-fps"),
                &mut fps,
                0.5,
                false,
                (clip.fps != 8.0).then_some(tip.as_str()),
                None,
                None,
                &mut pick,
            );
            if reset {
                fps = 8.0;
            }
            if changed || reset {
                event = command(
                    "asset.frames_clip_set",
                    &[("clip", clip_name.as_str()), ("fps", &fps.to_string())],
                );
            }
            if frame.toggle(&text("asset-anim-loop"), clip.looping) {
                let v = if clip.looping { "no" } else { "yes" };
                event =
                    command("asset.frames_clip_set", &[("clip", clip_name.as_str()), ("loop", v)]);
            }
            frame.muted(&text("asset-anim-event-hint"));
            let rows = crate::anim_event_ui::rows(frame, session, &clip_name, &clip.frames);
            if !matches!(rows, PanelEvent::None) {
                event = rows;
            }
            frame.rule();
            if frame.button(&text("asset-anim-socket-add")) {
                event = command(
                    "asset.frames_socket",
                    &[("clip", clip_name.as_str()), ("action", "add"), ("name", "socket")],
                );
            }
            if let Some(frame_pic) = clip.frames.get(picked_frame) {
                let socket_id = egui::Id::new(("asset-anim-socket", rel, clip_name.clone()));
                let selected: String = ctx.data(|d| d.get_temp(socket_id)).unwrap_or_default();
                for socket in &frame_pic.sockets {
                    frame.label(&socket.name);
                    let rename_id = egui::Id::new((
                        "asset-anim-socket-name",
                        rel,
                        clip_name.clone(),
                        socket.name.clone(),
                    ));
                    let mut draft: String =
                        ctx.data(|d| d.get_temp(rename_id)).unwrap_or_else(|| socket.name.clone());
                    let (changed, committed) =
                        frame.draft_field(&text("asset-anim-socket-name"), &mut draft);
                    if changed {
                        ctx.data_mut(|d| d.insert_temp(rename_id, draft.clone()));
                    }
                    if committed && draft.trim() != socket.name {
                        ctx.data_mut(|d| d.remove::<String>(rename_id));
                        event = command(
                            "asset.frames_socket",
                            &[
                                ("clip", clip_name.as_str()),
                                ("action", "rename"),
                                ("name", socket.name.as_str()),
                                ("to", draft.trim()),
                            ],
                        );
                    }
                    if frame.button(&text("asset-anim-socket-remove")) {
                        event = command(
                            "asset.frames_socket",
                            &[
                                ("clip", clip_name.as_str()),
                                ("action", "remove"),
                                ("name", socket.name.as_str()),
                            ],
                        );
                    }
                    if socket.name == selected {
                        frame.muted(&text("asset-anim-socket-selected"));
                    }
                }
            }
            frame.rule();

            // Cut frames from a picture.
            let default_src = clip
                .frames
                .first()
                .map(|f| frame_picture(rel, &f.image))
                .unwrap_or_else(|| rel.trim_end_matches(".frames").to_string() + ".png");
            let mut src =
                session.draft(ANIM_SRC_DRAFT).filter(|s| !s.is_empty()).unwrap_or(default_src);
            let (changed, _) = frame.draft_field(&text("asset-anim-source"), &mut src);
            if changed {
                event = PanelEvent::Commands(vec![draft_cmd(ANIM_SRC_DRAFT, &src)]);
            }
            frame.hint(&text("asset-anim-source-hint"));
            let mut grid_num = |frame: &mut PanelFrame, label: &str, key: &str| {
                let v: f32 = session.draft(key).and_then(|v| v.parse().ok()).unwrap_or(1.0);
                let mut value = v;
                let mut pick = None;
                let (changed, _, _) =
                    frame.number_row(label, &mut value, 1.0, false, None, None, None, &mut pick);
                if changed {
                    let n = value.round().clamp(1.0, 256.0);
                    event = PanelEvent::Commands(vec![draft_cmd(key, &n.to_string())]);
                }
                v.round().clamp(1.0, 256.0) as u32
            };
            let cols = grid_num(frame, &text("asset-anim-cols"), ANIM_COLS_DRAFT);
            let rows = grid_num(frame, &text("asset-anim-rows"), ANIM_ROWS_DRAFT);
            frame.row(|frame| {
                let (c, r) = (cols.to_string(), rows.to_string());
                let base = [("clip", clip_name.as_str()), ("image", src.as_str())];
                if frame.button(&text("asset-anim-grid")) {
                    let mut a = base.to_vec();
                    a.extend([("mode", "grid"), ("cols", c.as_str()), ("rows", r.as_str())]);
                    event = command("asset.frames_slice", &a);
                }
                if frame.button(&text("asset-anim-auto")) {
                    let mut a = base.to_vec();
                    a.push(("mode", "auto"));
                    event = command("asset.frames_slice", &a);
                }
                if frame.button(&text("asset-anim-whole")) {
                    let mut a = base.to_vec();
                    a.extend([("mode", "whole"), ("append", "yes")]);
                    event = command("asset.frames_slice", &a);
                }
            });
            let shown = clip.frames.get(picked_frame);
            let extra = crate::sheet_editor::controls(
                frame,
                session,
                clip_name.as_str(),
                src.as_str(),
                picked_frame,
                shown,
            );
            if !matches!(extra, PanelEvent::None) {
                event = extra;
            }
            frame.hint(&text("asset-saved-hint"));
        });
    });
    event
}

fn material_editor(
    frame: &mut PanelFrame,
    session: &EditorSession,
    rel: &str,
    full: &Path,
) -> PanelEvent {
    let ui = frame.ui_mut();
    if let Ok(rgba) = seal_editor_core::asset_preview::material_sphere_rgba(full) {
        let size = seal_editor_core::asset_preview::PREVIEW_SIZE as f32;
        let texture = ui.ctx().load_texture(
            format!("mat-prev-{rel}"),
            egui::ColorImage::from_rgba_unmultiplied([size as usize, size as usize], &rgba),
            egui::TextureOptions::LINEAR,
        );
        ui.image((texture.id(), egui::vec2(size, size)));
    }
    text_editor(frame, session, rel, full)
}

fn mesh_editor(
    frame: &mut PanelFrame,
    session: &EditorSession,
    rel: &str,
    full: &Path,
) -> PanelEvent {
    let ui = frame.ui_mut();
    if let Ok(rgba) = seal_editor_core::asset_preview::mesh_asset_rgba(full) {
        let size = seal_editor_core::asset_preview::PREVIEW_SIZE as f32;
        let texture = ui.ctx().load_texture(
            format!("mesh-prev-{rel}"),
            egui::ColorImage::from_rgba_unmultiplied([size as usize, size as usize], &rgba),
            egui::TextureOptions::LINEAR,
        );
        ui.image((texture.id(), egui::vec2(size, size)));
    }
    frame.padded(|frame| {
        frame.muted(&session.messages.get("asset-mesh-preview-hint"));
    });
    if let Ok(text) = std::fs::read_to_string(full) {
        frame.padded(|frame| frame.muted(&format!("{rel}: {} lines", text.lines().count())));
    }
    PanelEvent::None
}

// ---------------------------------------------------------------- text

type TextBuf = (String, String);

fn text_editor(
    frame: &mut PanelFrame,
    session: &EditorSession,
    rel: &str,
    full: &Path,
) -> PanelEvent {
    let mut event = PanelEvent::None;
    let ui = frame.ui_mut();
    let id = egui::Id::new("asset-text-buffer");
    let disk = std::fs::read_to_string(full).unwrap_or_default();
    // The buffer follows the file unless it holds an edit that is still being saved.
    let mut buf = ui
        .ctx()
        .data(|d| d.get_temp::<TextBuf>(id))
        .filter(|(path, _)| path == rel)
        .map(|(_, text)| text)
        .unwrap_or_else(|| disk.clone());
    let lines = buf.lines().count().max(1);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.weak(session.messages.format("asset-text-lines", &[("lines", &lines.to_string())]));
        ui.weak(session.messages.get("asset-saved-hint"));
    });
    if rel.to_ascii_lowercase().ends_with(".shade2d") {
        if let Err(err) = seal_editor_core::material2d::Program::parse(&buf) {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.colored_label(egui::Color32::from_rgb(220, 80, 70), err.to_string());
            });
        }
    }
    let edit_id = ui.id().with(("asset-text-edit", rel));
    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
        let response = ui.add(
            egui::TextEdit::multiline(&mut buf)
                .id(edit_id)
                .code_editor()
                .desired_width(f32::INFINITY)
                .desired_rows(24)
                .lock_focus(true),
        );
        if response.changed() {
            event = command("asset.text_save", &[("text", buf.as_str())]);
        }
    });
    let line = egui::TextEdit::load_state(ui.ctx(), edit_id)
        .and_then(|state| state.cursor.char_range())
        .map(|range| {
            buf.chars().take(range.primary.index.into()).filter(|ch| *ch == '\n').count() as u32 + 1
        })
        .unwrap_or(1);
    ui.ctx().data_mut(|data| data.insert_temp(egui::Id::new(("asset-text-line", rel)), line));
    ui.ctx().data_mut(|d| d.insert_temp::<TextBuf>(id, (rel.to_string(), buf)));
    event
}

fn tile_set_editor(
    frame: &mut PanelFrame,
    session: &EditorSession,
    rel: &str,
    full: &Path,
) -> PanelEvent {
    use seal_editor_core::tile_set_editor::{read_rows, write_row};
    use seal_editor_core::tiles::TileNavigation;
    let text = |key: &str| session.messages.get(key);
    let rows = match read_rows(full) {
        Ok(rows) => rows,
        Err(e) => {
            frame.padded(|f| f.muted(&format!("{rel}: {e}")));
            return PanelEvent::None;
        }
    };
    let mut event = PanelEvent::None;
    let pick = session
        .draft("tile-set-pick")
        .unwrap_or_else(|| rows.first().map(|r| r.picture.clone()).unwrap_or_default());
    frame.scroll(|frame| {
        frame.padded(|frame| frame.label(&text("asset-tile-set-title")));
        for row in &rows {
            if frame.button(&row.picture) && pick != row.picture {
                event = command(
                    "draft.set",
                    &[("key", "tile-set-pick"), ("value", row.picture.as_str())],
                );
            }
        }
        let Some(mut def) = rows.iter().find(|r| r.picture == pick).map(|r| r.def.clone()) else {
            return;
        };
        let mut data = def.custom_data.first().map(|(k, v)| format!("{k}={v}")).unwrap_or_default();
        frame.label(&text("asset-tile-set-data"));
        if frame.text_edit(&mut data) {
            def.custom_data.clear();
            if let Some((k, v)) = data.split_once('=') {
                def.custom_data.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
        let occluder = def.occluder;
        if frame.toggle(&text("asset-tile-set-occluder"), occluder) {
            def.occluder = !occluder;
        }
        let blocked = def.navigation == TileNavigation::Blocked;
        if frame.toggle(&text("asset-tile-set-navigation-blocked"), blocked) {
            def.navigation =
                if blocked { TileNavigation::Walkable } else { TileNavigation::Blocked };
        }
        let mut prefab = def.prefab.clone();
        frame.label(&text("asset-tile-set-prefab"));
        if frame.text_edit(&mut prefab) {
            def.prefab = prefab;
        }
        if frame.button(&text("asset-tile-set-apply")) {
            let _ = write_row(full, &pick, &def);
        }
    });
    event
}

/// Size of one checkerboard square for a picture drawn at `scale` screen pixels
/// per picture pixel. Always a whole multiple or an even split of `scale`, about
/// 8 to 16 screen pixels.
fn checker_cell(scale: f32) -> f32 {
    if scale >= 16.0 {
        scale / (scale / 12.0).round().max(1.0)
    } else {
        scale * (8.0 / scale).ceil().max(1.0)
    }
}

#[cfg(test)]
mod checker_tests {
    use super::checker_cell;

    #[test]
    fn checker_squares_line_up_with_picture_pixels() {
        for scale in [0.5_f32, 1.0, 3.0, 4.0, 7.5, 16.0, 25.0, 40.0, 64.0] {
            let cell = checker_cell(scale);
            let ratio = if cell >= scale { cell / scale } else { scale / cell };
            assert!((ratio - ratio.round()).abs() < 1e-4, "scale {scale} cell {cell}");
            assert!(cell >= 4.0, "scale {scale} cell {cell}");
        }
    }
}
