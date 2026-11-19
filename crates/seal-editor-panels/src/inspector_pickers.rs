//! Asset, object, colour, and map/struct rows in the inspector.

use seal_editor_core::{
    apply_record_op, asset_field, asset_reference, collect_assets, colour_field, entity_ref_field,
    filter_assets, hex_to_rgba, load_record, looks_like_record, object_reference,
    objects_from_nodes, recent_colours, rgba_to_hex, save_recent_colours, serialize_record,
    AssetCandidate, FieldValue, RecordOp,
};
use seal_ui::asset_picker::{asset_picker, AssetChoice, AssetPickerEvent, PickerLabels};
use seal_ui::colour_picker::{self, ColourLabels};
use seal_ui::map_field::{record_editor, RecordEvent, RecordLabels};
use seal_ui::object_picker::{object_picker, ObjectChoice, ObjectLabels, ObjectPickerEvent};
use seal_ui::PanelFrame;

use crate::{command, PanelEvent};

#[allow(clippy::too_many_arguments)]
pub(crate) fn try_paint(
    frame: &mut PanelFrame,
    session: &seal_editor_core::EditorSession,
    key: &str,
    label: &str,
    current: Option<&str>,
    set_target: (&str, &str),
) -> Option<PanelEvent> {
    let mixed = current.is_none();
    let current = current.unwrap_or("");
    if asset_field(key) {
        return Some(paint_asset(frame, session, key, label, current, mixed, set_target));
    }
    if entity_ref_field(key) {
        return Some(paint_object(frame, session, key, label, current, mixed, set_target));
    }
    if colour_field(key) {
        return Some(paint_colour(frame, session, key, label, current, set_target));
    }
    if looks_like_record(current) {
        return Some(paint_record(frame, session, key, label, current, set_target));
    }
    None
}

fn labels(session: &seal_editor_core::EditorSession) -> PickerLabels {
    let text = |id: &str| session.messages.get(id);
    PickerLabels {
        search: text("picker-search"),
        clear: text("picker-clear"),
        show: text("picker-show"),
        empty: text("picker-empty"),
        none: text("picker-none"),
        mixed: text("picker-mixed"),
    }
}

fn project_root(session: &seal_editor_core::EditorSession) -> Option<std::path::PathBuf> {
    session
        .project
        .as_ref()
        .map(|project| project.root.clone())
        .or_else(|| session.models.content_tree_root.clone())
}

fn choices_for(session: &seal_editor_core::EditorSession, field: &str) -> Vec<AssetChoice> {
    let Some(root) = project_root(session) else {
        return Vec::new();
    };
    let found = collect_assets(&root);
    filter_assets(&found, field, "").into_iter().map(|item| choice(&root, item)).collect()
}

/// Project-relative paths of the assets a field accepts, in listing order.
pub(crate) fn asset_paths(session: &seal_editor_core::EditorSession, field: &str) -> Vec<String> {
    choices_for(session, field).into_iter().map(|choice| choice.path).collect()
}

fn choice(root: &std::path::Path, item: &AssetCandidate) -> AssetChoice {
    let file = root.join(&item.path);
    AssetChoice {
        path: item.path.clone(),
        name: item.name.clone(),
        kind: item.kind.clone(),
        file: file.is_file().then(|| file.to_string_lossy().to_string()),
    }
}

pub(crate) enum AssetRow {
    Idle,
    Set(String),
    Clear,
    Show,
}

/// Asset picker for a path that is not an inspector field (music, a font, a theme file).
pub(crate) fn paint_asset_row(
    frame: &mut PanelFrame,
    session: &seal_editor_core::EditorSession,
    field: &str,
    salt: &str,
    label: &str,
    current: &str,
    mixed: bool,
) -> AssetRow {
    let choices = choices_for(session, field);
    match asset_picker(frame.ui_mut(), salt, label, current, mixed, &choices, &labels(session)) {
        AssetPickerEvent::Set(path) => AssetRow::Set(asset_reference(&path)),
        AssetPickerEvent::Clear => AssetRow::Clear,
        AssetPickerEvent::Show => AssetRow::Show,
        AssetPickerEvent::Idle => AssetRow::Idle,
    }
}

fn paint_asset(
    frame: &mut PanelFrame,
    session: &seal_editor_core::EditorSession,
    key: &str,
    label: &str,
    current: &str,
    mixed: bool,
    set_target: (&str, &str),
) -> PanelEvent {
    match paint_asset_row(frame, session, key, key, label, current, mixed) {
        AssetRow::Set(path) => {
            command("inspector.set", &[set_target, ("field", key), ("value", path.as_str())])
        }
        AssetRow::Clear => command("inspector.set", &[set_target, ("field", key), ("value", "")]),
        AssetRow::Show => command("picker.show_asset", &[("path", &asset_reference(current))]),
        AssetRow::Idle => PanelEvent::None,
    }
}

fn paint_object(
    frame: &mut PanelFrame,
    session: &seal_editor_core::EditorSession,
    key: &str,
    label: &str,
    current: &str,
    mixed: bool,
    set_target: (&str, &str),
) -> PanelEvent {
    let objects = objects_from_nodes(&session.models.nodes);
    let choices: Vec<ObjectChoice> = objects
        .iter()
        .map(|item| ObjectChoice {
            id: item.id.clone(),
            name: item.name.clone(),
            shape: item.shape.clone(),
            role: item.role.clone(),
        })
        .collect();
    let text = |id: &str| session.messages.get(id);
    let picked = object_picker(
        frame.ui_mut(),
        key,
        label,
        current,
        mixed,
        seal_editor_core::eyedropper_armed(session),
        &choices,
        &ObjectLabels {
            search: text("picker-search"),
            clear: text("picker-clear"),
            eyedropper: text("picker-eyedropper"),
            armed: text("picker-eyedropper-armed"),
            empty: text("picker-empty"),
            none: text("picker-none"),
            mixed: text("picker-mixed"),
        },
    );
    let mut point = PanelEvent::None;
    if key == "nav_target" {
        let mut draft = current.to_string();
        if frame
            .ui_mut()
            .add(egui::TextEdit::singleline(&mut draft).hint_text("x, y").desired_width(120.0))
            .changed()
        {
            point =
                command("inspector.set", &[set_target, ("field", key), ("value", draft.as_str())]);
        }
    }
    let picked = match picked {
        ObjectPickerEvent::Set(id) => command(
            "inspector.set",
            &[set_target, ("field", key), ("value", &object_reference(&id))],
        ),
        ObjectPickerEvent::Clear => {
            command("inspector.set", &[set_target, ("field", key), ("value", "")])
        }
        ObjectPickerEvent::Eyedropper => {
            if set_target.0 == "ids" {
                command("picker.eyedropper", &[("field", key), ("ids", set_target.1)])
            } else {
                command("picker.eyedropper", &[("field", key), ("id", set_target.1)])
            }
        }
        ObjectPickerEvent::Idle => PanelEvent::None,
    };
    if matches!(picked, PanelEvent::None) {
        point
    } else {
        picked
    }
}

fn paint_colour(
    frame: &mut PanelFrame,
    session: &seal_editor_core::EditorSession,
    key: &str,
    label: &str,
    current: &str,
    set_target: (&str, &str),
) -> PanelEvent {
    let ui = frame.ui_mut();
    colour_picker::seed_recent(ui, &recent_colours(session));
    let draft_id = ui.id().with((key, "colour-draft"));
    let parsed =
        hex_to_rgba(current).unwrap_or(seal_editor_core::Rgba { r: 1.0, g: 1.0, b: 1.0, a: 1.0 });
    let mut rgba = ui
        .data(|data| data.get_temp::<[f32; 4]>(draft_id))
        .unwrap_or([parsed.r, parsed.g, parsed.b, parsed.a]);
    let mut recent = colour_picker::recent_now(ui);
    let mut change = colour_picker::ColourChange { changed: false, commit: false };
    ui.horizontal(|ui| {
        seal_ui::property_name(ui, label, false, None);
        let (rect, response) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
        let fill = egui::Color32::from_rgba_unmultiplied(
            (rgba[0] * 255.0) as u8,
            (rgba[1] * 255.0) as u8,
            (rgba[2] * 255.0) as u8,
            (rgba[3] * 255.0) as u8,
        );
        ui.painter().rect_filled(rect.shrink(1.0), 2.0, fill);
        let text = |id: &str| session.messages.get(id);
        egui::Popup::menu(&response).show(|ui| {
            change = colour_picker::edit(
                ui,
                &mut rgba,
                &mut recent,
                &ColourLabels {
                    hex: text("picker-hex"),
                    alpha: text("picker-alpha"),
                    recent: text("picker-recent"),
                },
            );
        });
        ui.label(rgba_to_hex(seal_editor_core::Rgba {
            r: rgba[0],
            g: rgba[1],
            b: rgba[2],
            a: rgba[3],
        }));
    });
    if change.changed {
        frame.ui_mut().data_mut(|data| data.insert_temp(draft_id, rgba));
    }
    colour_picker::store_recent(frame.ui_mut(), recent.clone());
    if change.commit {
        frame.ui_mut().data_mut(|data| data.remove::<[f32; 4]>(draft_id));
        let hex =
            rgba_to_hex(seal_editor_core::Rgba { r: rgba[0], g: rgba[1], b: rgba[2], a: rgba[3] });
        let stored = save_recent_colours(&recent);
        return PanelEvent::Commands(vec![
            (
                "inspector.set_colour".into(),
                seal_editor_core::CommandArgs::new(&[
                    set_target,
                    ("field", key),
                    ("value", hex.as_str()),
                ]),
            ),
            (
                "picker.remember_colours".into(),
                seal_editor_core::CommandArgs::new(&[("value", stored.as_str())]),
            ),
        ]);
    }
    PanelEvent::None
}

fn paint_record(
    frame: &mut PanelFrame,
    _session: &seal_editor_core::EditorSession,
    key: &str,
    label: &str,
    current: &str,
    set_target: (&str, &str),
) -> PanelEvent {
    let ui = frame.ui_mut();
    ui.label(label);
    let (value, notes) = load_record(current).unwrap_or_else(|err| {
        (
            FieldValue::Damaged(current.to_string()),
            seal_editor_core::pickers::LoadNotes { damaged: vec![err] },
        )
    });
    for note in &notes.damaged {
        ui.label(note);
    }
    let labels = RecordLabels {
        add: _session.messages.get("picker-add"),
        remove: _session.messages.get("picker-remove"),
        key: _session.messages.get("picker-key"),
        kind: _session.messages.get("picker-kind"),
    };
    let action = record_editor(ui, key, &value, &labels);
    let op = match action {
        RecordEvent::Idle => return PanelEvent::None,
        RecordEvent::Set { path, kind, value } => RecordOp::Set {
            path: seal_editor_core::record_path(&path),
            value: match kind.as_str() {
                "number" => FieldValue::Number(value),
                "bool" => FieldValue::Bool(value == "yes"),
                "colour" => FieldValue::Colour(value),
                _ => FieldValue::Text(value),
            },
        },
        RecordEvent::Remove { path } => {
            RecordOp::Remove { path: seal_editor_core::record_path(&path) }
        }
        RecordEvent::Add { parent, key, kind } => RecordOp::Add {
            path: seal_editor_core::record_path(&parent),
            key,
            value: seal_editor_core::default_for_kind(&kind),
        },
    };
    match apply_record_op(current, op) {
        Ok(next) => {
            command("inspector.set", &[set_target, ("field", key), ("value", next.as_str())])
        }
        Err(err) => {
            ui.label(err);
            PanelEvent::None
        }
    }
}

/// The mesh colour row: one popup sets red, green, and blue in one undo step.
pub(crate) fn paint_rgb(
    frame: &mut PanelFrame,
    session: &seal_editor_core::EditorSession,
    rgb: [f32; 3],
    label: &str,
    set_target: (&str, &str),
) -> PanelEvent {
    let ui = frame.ui_mut();
    colour_picker::seed_recent(ui, &recent_colours(session));
    let draft_id = ui.id().with(("entity-colour", "draft"));
    let mut rgba = ui
        .data(|data| data.get_temp::<[f32; 4]>(draft_id))
        .unwrap_or([rgb[0], rgb[1], rgb[2], 1.0]);
    let mut recent = colour_picker::recent_now(ui);
    let mut change = colour_picker::ColourChange { changed: false, commit: false };
    ui.horizontal(|ui| {
        ui.label(label);
        let (rect, response) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
        ui.painter().rect_filled(
            rect.shrink(1.0),
            2.0,
            egui::Color32::from_rgb(
                (rgba[0] * 255.0) as u8,
                (rgba[1] * 255.0) as u8,
                (rgba[2] * 255.0) as u8,
            ),
        );
        let text = |id: &str| session.messages.get(id);
        egui::Popup::menu(&response).show(|ui| {
            change = colour_picker::edit(
                ui,
                &mut rgba,
                &mut recent,
                &ColourLabels {
                    hex: text("picker-hex"),
                    alpha: text("picker-alpha"),
                    recent: text("picker-recent"),
                },
            );
        });
    });
    if change.changed {
        frame.ui_mut().data_mut(|data| data.insert_temp(draft_id, rgba));
    }
    colour_picker::store_recent(frame.ui_mut(), recent.clone());
    if change.commit {
        frame.ui_mut().data_mut(|data| data.remove::<[f32; 4]>(draft_id));
        let r = rgba[0].to_string();
        let g = rgba[1].to_string();
        let b = rgba[2].to_string();
        let stored = save_recent_colours(&recent);
        return PanelEvent::Commands(vec![
            (
                "inspector.set_colour".into(),
                seal_editor_core::CommandArgs::new(&[
                    set_target,
                    ("r", r.as_str()),
                    ("g", g.as_str()),
                    ("b", b.as_str()),
                ]),
            ),
            (
                "picker.remember_colours".into(),
                seal_editor_core::CommandArgs::new(&[("value", stored.as_str())]),
            ),
        ]);
    }
    PanelEvent::None
}

/// Graph string variables that hold a map or struct, plus a control to add one.
pub(crate) fn paint_graph_vars(
    frame: &mut PanelFrame,
    session: &seal_editor_core::EditorSession,
    script: &str,
) -> PanelEvent {
    if !script.ends_with(".graph") {
        return PanelEvent::None;
    }
    let Some(root) = project_root(session) else {
        return PanelEvent::None;
    };
    let full = root.join(script);
    let Ok(text) = std::fs::read_to_string(&full) else {
        return PanelEvent::None;
    };
    let Ok(records) = seal_editor_core::pickers::graph_text_records(&text) else {
        frame.ui_mut().label(session.messages.get("picker-graph-damaged"));
        return PanelEvent::None;
    };
    // The script's inspector layout: order, sections, hidden variables, help lines, number ranges.
    let layout = seal_editor_core::script_inspector::layout_for_script(&root, script);
    let mut records = records;
    if let Some(layout) = layout.as_ref() {
        let section_order = |name: &Option<String>| {
            name.as_ref()
                .and_then(|n| layout.sections.iter().find(|s| &s.name == n))
                .map_or(i32::MAX, |s| s.order)
        };
        records.retain(|(name, _)| !layout.is_hidden(name));
        // Stable sort: ruled variables by (section, order); unruled keep file order at the end.
        records.sort_by_key(|(name, _)| match layout.rule_for(name) {
            Some(rule) => (0, section_order(&rule.section), rule.order),
            None => (1, i32::MAX, i32::MAX),
        });
    }
    let ui = frame.ui_mut();
    ui.label(session.messages.get("picker-variables"));
    let mut event = PanelEvent::None;
    let mut last_section: Option<String> = None;
    for (name, value) in records {
        let rule = layout.as_ref().and_then(|layout| layout.rule_for(&name));
        if let Some(section) = rule.and_then(|rule| rule.section.clone()) {
            if last_section.as_deref() != Some(section.as_str()) {
                ui.add_space(4.0);
                ui.strong(&section);
                last_section = Some(section);
            }
        }
        if let (Some(min), Some(max), Ok(number)) = (
            rule.and_then(|rule| rule.min),
            rule.and_then(|rule| rule.max),
            value.trim().parse::<f64>(),
        ) {
            let mut number = number;
            let changed = ui
                .horizontal(|ui| {
                    seal_ui::property_name(ui, &name, false, None);
                    ui.add(egui::Slider::new(&mut number, min.min(max)..=max.max(min))).changed()
                })
                .inner;
            if let Some(help) = rule.and_then(|rule| rule.help.as_deref()) {
                ui.weak(help);
            }
            if changed {
                let next = format!("{number}");
                event = command(
                    "graph.var_record",
                    &[("path", script), ("name", name.as_str()), ("value", next.as_str())],
                );
            }
            continue;
        }
        ui.label(&name);
        if let Some(help) = rule.and_then(|rule| rule.help.as_deref()) {
            ui.weak(help);
        }
        let shown = if value.trim().is_empty() {
            serialize_record(&FieldValue::empty_map())
        } else {
            value.clone()
        };
        let (parsed, notes) = load_record(&shown).unwrap_or((
            FieldValue::Damaged(shown.clone()),
            seal_editor_core::pickers::LoadNotes { damaged: Vec::new() },
        ));
        for note in &notes.damaged {
            ui.label(note);
        }
        let action = record_editor(
            ui,
            &name,
            &parsed,
            &RecordLabels {
                add: session.messages.get("picker-add"),
                remove: session.messages.get("picker-remove"),
                key: session.messages.get("picker-key"),
                kind: session.messages.get("picker-kind"),
            },
        );
        if let Some(next) = apply_graph_action(&shown, action) {
            event = command(
                "graph.var_record",
                &[("path", script), ("name", name.as_str()), ("value", next.as_str())],
            );
        }
    }
    let name_id = ui.id().with(("graph-var-name", script));
    let mut name = ui.data(|data| data.get_temp::<String>(name_id).unwrap_or_default());
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut name)
                .hint_text(session.messages.get("picker-key"))
                .desired_width(120.0),
        );
        if ui.button(session.messages.get("picker-add-map")).clicked() && !name.trim().is_empty() {
            let value = serialize_record(&FieldValue::empty_map());
            event = command(
                "graph.var_record",
                &[("path", script), ("name", name.trim()), ("value", value.as_str())],
            );
            name.clear();
        }
    });
    ui.data_mut(|data| data.insert_temp(name_id, name));
    event
}

fn apply_graph_action(current: &str, action: RecordEvent) -> Option<String> {
    let op = match action {
        RecordEvent::Idle => return None,
        RecordEvent::Set { path, kind, value } => RecordOp::Set {
            path: seal_editor_core::record_path(&path),
            value: match kind.as_str() {
                "number" => FieldValue::Number(value),
                "bool" => FieldValue::Bool(value == "yes"),
                "colour" => FieldValue::Colour(value),
                _ => FieldValue::Text(value),
            },
        },
        RecordEvent::Remove { path } => {
            RecordOp::Remove { path: seal_editor_core::record_path(&path) }
        }
        RecordEvent::Add { parent, key, kind } => RecordOp::Add {
            path: seal_editor_core::record_path(&parent),
            key,
            value: seal_editor_core::default_for_kind(&kind),
        },
    };
    apply_record_op(current, op).ok()
}
