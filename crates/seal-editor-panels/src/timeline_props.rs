//! Property-track controls for the Timeline panel.

use seal_editor_core::timeline_doc::{property_choices, selected_entity_id, TimelinePropertyTrack};
use seal_editor_core::{CommandArgs, Sequence};
use seal_ui::crisp::CrispCombo;
use seal_ui::PanelFrame;

use crate::PanelEvent;

/// Add-track list, and per-track key, easing, time and delete controls.
pub fn controls(
    frame: &mut PanelFrame,
    session: &mut seal_editor_core::EditorSession,
    path: &str,
    seq: &Sequence,
    time: f32,
) -> PanelEvent {
    if let Some(error) = session.timeline.as_ref().and_then(|doc| doc.save_error.clone()) {
        frame.muted(&session.messages.format("timeline-save-error", &[("error", error.as_str())]));
    }
    let warp = time_warp_row(frame, session, path, seq, time);
    if !matches!(warp, PanelEvent::None) {
        return warp;
    }
    let add = add_track(frame, session, path);
    if !matches!(add, PanelEvent::None) {
        return add;
    }
    for track in &seq.properties {
        let event = track_row(frame, session, path, track, time);
        if !matches!(event, PanelEvent::None) {
            return event;
        }
    }
    PanelEvent::None
}

fn time_warp_row(
    frame: &mut PanelFrame,
    session: &mut seal_editor_core::EditorSession,
    path: &str,
    seq: &Sequence,
    time: f32,
) -> PanelEvent {
    frame.muted(&session.messages.get("timeline-time-warp-hint"));
    let content = seq
        .time_warp
        .keys
        .iter()
        .find(|(t, _)| (*t - time).abs() <= 0.0005)
        .map(|(_, v)| *v)
        .unwrap_or(time);
    let label = session.messages.get("timeline-time-warp-content");
    let mut picked = content.to_string();
    frame.ui_mut().horizontal(|ui| {
        seal_ui::property_name(ui, &label, false, None);
        ui.text_edit_singleline(&mut picked);
    });
    if frame.button(&session.messages.get("timeline-time-warp-key")) {
        return PanelEvent::Command(
            "sequence.time_warp_add".into(),
            CommandArgs::new(&[
                ("path", path),
                ("time", &format!("{time:.4}")),
                ("content", picked.trim()),
            ]),
        );
    }
    if !seq.time_warp.keys.is_empty()
        && frame.button(&session.messages.get("timeline-time-warp-clear"))
    {
        return PanelEvent::Command(
            "sequence.time_warp_clear".into(),
            CommandArgs::new(&[("path", path)]),
        );
    }
    PanelEvent::None
}

fn add_track(
    frame: &mut PanelFrame,
    session: &seal_editor_core::EditorSession,
    path: &str,
) -> PanelEvent {
    let Some(entity) = selected_entity_id(session) else {
        frame.muted(&session.messages.get("timeline-no-entity"));
        return PanelEvent::None;
    };
    let fields = property_choices(session, &entity);
    if fields.is_empty() {
        frame.muted(&session.messages.get("timeline-no-entity"));
        return PanelEvent::None;
    }
    let prompt = session.messages.get("timeline-add-property");
    let mut options = vec![("", prompt)];
    for field in &fields {
        options.push((field.as_str(), field_label(field)));
    }
    let Some(picked) = frame.choice(&session.messages.get("timeline-add-property"), "", &options)
    else {
        return PanelEvent::None;
    };
    if picked.is_empty() {
        return PanelEvent::None;
    }
    PanelEvent::Command(
        "sequence.property_add".into(),
        CommandArgs::new(&[
            ("path", path),
            ("entity", entity.as_str()),
            ("field", picked.as_str()),
        ]),
    )
}

fn track_row(
    frame: &mut PanelFrame,
    session: &seal_editor_core::EditorSession,
    path: &str,
    track: &TimelinePropertyTrack,
    time: f32,
) -> PanelEvent {
    let entity = track.entity.clone();
    let field = track.path.clone();
    let nearest =
        track.keys.iter().min_by(|a, b| (a.time - time).abs().total_cmp(&(b.time - time).abs()));
    let key_time = nearest.map(|key| key.time);
    let ease = nearest.map(|key| key.ease.name()).unwrap_or("linear");
    let steps = nearest.is_some_and(|key| key.value.steps());
    let mut key_clicked = false;
    let mut delete_clicked = false;
    let mut ease_picked = None;
    let mut moved = None;
    let key_label = session.messages.get("timeline-key");
    let ease_label = session.messages.get("timeline-easing");
    let time_label = session.messages.get("timeline-key-time");
    let delete_label = session.messages.get("timeline-delete-key");
    frame.ui_mut().horizontal(|ui| {
        let name =
            session.world.entity(&entity).map(|e| e.name.clone()).unwrap_or_else(|| entity.clone());
        ui.label(format!("{name}  {}", field_label(&field)));
        if ui.button(&key_label).clicked() {
            key_clicked = true;
        }
        if key_time.is_some() {
            let choices: &[&str] = if steps {
                &["step"]
            } else {
                &["step", "linear", "ease-in", "ease-out", "ease-in-out"]
            };
            ui.label(&ease_label);
            egui::ComboBox::from_id_salt(("prop-ease", &entity, &field))
                .selected_text(ease_label_text(ease))
                .show_crisp(ui, |ui| {
                    for choice in choices {
                        if ui.selectable_label(*choice == ease, ease_label_text(choice)).clicked() {
                            ease_picked = Some((*choice).to_string());
                        }
                    }
                });
            let id = ui.id().with(("prop-time", entity.clone(), field.clone()));
            let mut text = ui
                .data(|data| data.get_temp::<String>(id))
                .unwrap_or_else(|| format!("{:.3}", key_time.unwrap_or(0.0)));
            let edit = ui.add(
                egui::TextEdit::singleline(&mut text)
                    .id_salt(("prop-time-edit", &entity, &field))
                    .desired_width(72.0)
                    .hint_text(&time_label),
            );
            if edit.changed() {
                ui.data_mut(|data| data.insert_temp(id, text.clone()));
            }
            if edit.lost_focus() {
                ui.data_mut(|data| data.remove::<String>(id));
                if let Ok(to) = text.trim().parse::<f32>() {
                    if key_time.is_some_and(|from| (from - to).abs() > 0.0005) {
                        moved = Some(to);
                    }
                }
            }
        }
        if ui.button(&delete_label).clicked() {
            delete_clicked = true;
        }
    });
    let t = format!("{:.3}", time);
    if key_clicked {
        return PanelEvent::Command(
            "sequence.property_key".into(),
            CommandArgs::new(&[
                ("path", path),
                ("entity", entity.as_str()),
                ("field", field.as_str()),
                ("t", t.as_str()),
            ]),
        );
    }
    if let Some(ease) = ease_picked {
        return PanelEvent::Command(
            "sequence.property_ease".into(),
            CommandArgs::new(&[
                ("path", path),
                ("entity", entity.as_str()),
                ("field", field.as_str()),
                ("t", t.as_str()),
                ("ease", ease.as_str()),
            ]),
        );
    }
    if let Some(to) = moved {
        let from = format!("{:.6}", key_time.unwrap_or(0.0));
        let to = format!("{:.6}", to);
        return PanelEvent::Command(
            "sequence.property_move".into(),
            CommandArgs::new(&[
                ("path", path),
                ("entity", entity.as_str()),
                ("field", field.as_str()),
                ("from", from.as_str()),
                ("to", to.as_str()),
            ]),
        );
    }
    if delete_clicked {
        return PanelEvent::Command(
            "sequence.property_delete".into(),
            CommandArgs::new(&[
                ("path", path),
                ("entity", entity.as_str()),
                ("field", field.as_str()),
                ("t", t.as_str()),
            ]),
        );
    }
    PanelEvent::None
}

/// "camera.fov" -> "Camera: Field of View", "body.walk_speed" -> "Body: Walk Speed".
pub(crate) fn field_label(path: &str) -> String {
    path.split('.')
        .map(|part| match part {
            "fov" => "Field of View".to_string(),
            "rgb" => "Color".to_string(),
            _ => part
                .split('_')
                .filter(|w| !w.is_empty())
                .map(|w| {
                    let mut c = w.chars();
                    c.next()
                        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
                .join(" "),
        })
        .collect::<Vec<_>>()
        .join(": ")
}

fn ease_label_text(ease: &str) -> &'static str {
    match ease {
        "step" => "Step",
        "ease-in" => "Ease In",
        "ease-out" => "Ease Out",
        "ease-in-out" => "Ease In-Out",
        _ => "Linear",
    }
}

#[cfg(test)]
mod label_tests {
    #[test]
    fn field_paths_read_as_words() {
        assert_eq!(super::field_label("camera.fov"), "Camera: Field of View");
        assert_eq!(super::field_label("body.walk_speed"), "Body: Walk Speed");
        assert_eq!(super::ease_label_text("ease-in-out"), "Ease In-Out");
    }
}
