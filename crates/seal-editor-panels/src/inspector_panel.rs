//! Details (inspector) header row, component tree, and picker.

use egui::{Sense, StrokeKind};
use seal_editor_core::{CommandArgs, EditorSession};
use seal_ui::{
    paint_context_menu, ContextMenuEntry, Icon, PanelFrame, ICON_PX, ROW_PX, SMALL_PX, STATUS_OK,
    TREE_INDENT_PX,
};

use crate::inspector_component_menu::inspector_component_context_entries;
use crate::{command, field, PanelEvent};
use seal_ui::ContextMenuPick;

/// The Area section belongs to objects with a collider (an area needs a trigger shape)
/// or an effect already set; plain sprites and folders skip it.
pub fn area_section_wanted(mode: Option<&str>, body: Option<&str>) -> bool {
    mode.is_some_and(|m| !m.is_empty() && m != "none")
        || body.is_some_and(|b| !b.is_empty() && b != "none")
}

pub fn inspector_section_visible(pick: &str, section: &str) -> bool {
    if pick.is_empty() {
        return true;
    }
    match pick {
        "transform" => section == "inspector-transform" || section == "inspector-physics",
        "mesh" => {
            matches!(section, "inspector-material" | "inspector-color" | "inspector-shape")
        }
        "light" => section == "inspector-light",
        "camera" => section == "inspector-camera",
        "sprite" => section == "inspector-sprite" || section == "inspector-tile-data",
        "sprite_mask" => section == "inspector-sprite-mask",
        "particles" => section == "inspector-particles",
        "script" => section == "inspector-script",
        "remote" => section == "inspector-remote",
        _ => true,
    }
}

fn component_icon(key: &str) -> Icon {
    match key {
        "light" => Icon::EntityLightDirectional,
        "camera" => Icon::EntityCamera,
        "mesh" => Icon::EntityCube,
        "sprite" => Icon::AssetTexture,
        "sprite_mask" => Icon::EntitySprite,
        "particles" => Icon::AssetParticles,
        "script" => Icon::AssetScript,
        _ => Icon::ToolTransform,
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_component_row(
    ui: &mut egui::Ui,
    entity_id: &str,
    depth: usize,
    icon: Icon,
    label: &str,
    component_key: Option<&str>,
    inherited: bool,
    selected: bool,
    edit_script: &str,
    menu: &[ContextMenuEntry],
    event: &mut PanelEvent,
) -> bool {
    let indent = TREE_INDENT_PX * depth as f32;
    let drag_w = if component_key.is_some() { 16.0 } else { 0.0 };
    let script_w = if component_key.is_some() && !inherited { 76.0 } else { 0.0 };
    let sense = if component_key.is_some() { Sense::click_and_drag() } else { Sense::click() };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_PX), sense);
    if selected {
        let fill = ui.visuals().selection.bg_fill;
        ui.painter().rect_filled(rect, 0.0, fill);
    }
    if let Some(payload) = response.dnd_hover_payload::<String>() {
        if payload.starts_with("inspcomp:") {
            ui.painter().rect_stroke(
                rect,
                0.0,
                egui::Stroke::new(1.0, ui.visuals().selection.stroke.color),
                StrokeKind::Inside,
            );
        }
    }
    if let Some(payload) = response.dnd_release_payload::<String>() {
        if let Some(rest) = payload.strip_prefix("inspcomp:") {
            let mut parts = rest.splitn(2, ':');
            let _entity = parts.next().unwrap_or("");
            let from_key = parts.next().unwrap_or("");
            let to = component_key.unwrap_or("root");
            if from_key != to {
                *event = command(
                    "inspector.reattach_component",
                    &[("entity", entity_id), ("from", from_key), ("to", to)],
                );
            }
        }
    }
    let ink = if inherited { ui.visuals().weak_text_color() } else { ui.visuals().text_color() };
    let centre_y = rect.center().y;
    if component_key.is_some() {
        seal_ui::paint_icon(
            ui.painter(),
            egui::pos2(rect.left() + indent + 8.0, centre_y),
            Icon::ViewDrag,
            12.0,
            ui.visuals().weak_text_color(),
        );
        if let Some(key) = component_key {
            let row_label = label;
            response.context_menu(|ui| {
                if let Some(pick) = paint_context_menu(ui, Some(row_label), menu) {
                    let mut pairs: Vec<(&str, &str)> =
                        pick.args.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                    pairs.push(("entity", entity_id));
                    pairs.push(("component", key));
                    *event = command(pick.command.as_str(), &pairs);
                }
            });
            if response.dragged() {
                response.dnd_set_drag_payload(format!("inspcomp:{entity_id}:{key}"));
            }
        }
    }
    let icon_x = rect.left() + indent + drag_w + ICON_PX * 0.5 + 6.0;
    seal_ui::paint_icon(
        ui.painter(),
        egui::pos2(icon_x, centre_y),
        icon,
        ICON_PX,
        if inherited { ui.visuals().weak_text_color() } else { ink },
    );
    let label_x = rect.left() + indent + drag_w + ICON_PX + 12.0;
    let label_max = rect.right() - script_w - 8.0;
    ui.painter().text(
        egui::pos2(label_x.min(label_max), centre_y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(seal_ui::BODY_PX),
        ink,
    );
    if let Some(key) = component_key {
        if !inherited {
            let script_rect = egui::Rect::from_min_max(
                egui::pos2(rect.right() - script_w - 4.0, rect.top()),
                egui::pos2(rect.right() - 4.0, rect.bottom()),
            );
            let script = ui.interact(script_rect, response.id.with("script"), Sense::click());
            ui.painter().text(
                egui::pos2(script_rect.left(), centre_y),
                egui::Align2::LEFT_CENTER,
                edit_script,
                egui::FontId::proportional(SMALL_PX),
                ui.visuals().weak_text_color(),
            );
            if script.clicked() {
                *event =
                    command("inspector.edit_script", &[("entity", entity_id), ("component", key)]);
            }
        }
    } else {
        response.context_menu(|ui| {
            if let Some(pick) = paint_context_menu(ui, Some(label), menu) {
                let pairs: Vec<(&str, &str)> =
                    pick.args.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                *event = command(pick.command.as_str(), &pairs);
            }
        });
    }
    response.clicked() && !response.dragged()
}

pub fn inspector_component_tree(
    frame: &mut PanelFrame<'_>,
    session: &EditorSession,
    panel_id: &str,
    primary: &seal_editor_core::MockNode,
    multi: bool,
    event: &mut PanelEvent,
    text: &impl Fn(&str) -> String,
) {
    if multi {
        return;
    }
    let entries = seal_editor_core::inspector_component_entries(primary);
    if entries.len() <= 1 {
        return;
    }
    let pick = session.models.component_pick(panel_id, &primary.id);
    // Never taller than its rows: a short tree has no empty band under it, and a
    // user-dragged height only matters once there are more rows than fit.
    let spacing = frame.ui_mut().spacing().item_spacing.y;
    let content_h = (entries.len() + 1) as f32 * (ROW_PX + spacing) + 2.0;
    let height =
        session.models.inspector_tree_heights.get(panel_id).copied().unwrap_or(96.0).min(content_h);
    let root_label =
        session.messages.format("inspector-instance-root", &[("name", primary.name.as_str())]);
    let menu = inspector_component_context_entries(session);
    let edit_script = text("inspector-edit-in-script");
    let entity_id = primary.id.as_str();
    frame.ui_mut().vertical(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), height),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let root_selected = pick.is_empty();
                    if paint_component_row(
                        ui,
                        entity_id,
                        0,
                        inspector_type_icon(primary),
                        &root_label,
                        None,
                        false,
                        root_selected,
                        &edit_script,
                        &menu,
                        event,
                    ) {
                        *event = command(
                            "inspector.pick_component",
                            &[("panel", panel_id), ("component", "")],
                        );
                    }
                    for (key, inherited) in entries {
                        let label = text(&format!("outliner-component-{key}"));
                        if paint_component_row(
                            ui,
                            entity_id,
                            1,
                            component_icon(key),
                            &label,
                            Some(key),
                            inherited,
                            pick == key,
                            &edit_script,
                            &menu,
                            event,
                        ) {
                            *event = command(
                                "inspector.pick_component",
                                &[("panel", panel_id), ("component", key), ("target", entity_id)],
                            );
                        }
                    }
                });
            },
        );
        let splitter = ui.allocate_response(egui::vec2(ui.available_width(), 4.0), Sense::drag());
        if splitter.dragged() {
            let new_h = (height + splitter.drag_delta().y).clamp(48.0, 240.0);
            *event = command(
                "inspector.tree_height",
                &[("panel", panel_id), ("height", &new_h.to_string())],
            );
        }
        if splitter.hovered() || splitter.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
    });
}

pub fn inspector_type_icon(node: &seal_editor_core::MockNode) -> Icon {
    let shape = field(node, "shape");
    let role = field(node, "role");
    match field(node, "source_kind") {
        "prefab" => Icon::EntityPrefab,
        "script" => Icon::EntityScript,
        _ if matches!(field(node, "mask_shape"), "rect" | "circle" | "alpha")
            && field(node, "sprite").is_empty() =>
        {
            Icon::EntitySprite
        }
        _ => seal_ui::outliner_entity_icon(shape, role),
    }
}

pub fn inspector_header_row(
    frame: &mut PanelFrame<'_>,
    session: &EditorSession,
    panel_id: &str,
    primary: &seal_editor_core::MockNode,
    ids: &[String],
    multi: bool,
    event: &mut PanelEvent,
    text: &impl Fn(&str) -> String,
) {
    let locked = session.models.inspector_locks.contains_key(panel_id);
    frame.ui_mut().horizontal(|ui| {
        let (icon_rect, _) =
            ui.allocate_exact_size(egui::vec2(ICON_PX + 4.0, 22.0), egui::Sense::hover());
        seal_ui::paint_icon(
            ui.painter(),
            icon_rect.center(),
            inspector_type_icon(primary),
            ICON_PX,
            ui.visuals().text_color(),
        );
        ui.add_space(4.0);
        // Keep the name field clear of the add-component, asset and lock buttons
        // drawn on the right of this row.
        let source_kind_for_width = field(primary, "source_kind");
        let has_asset_menu = source_kind_for_width == "prefab" || source_kind_for_width == "script";
        let reserve = 40.0 + 26.0 + if has_asset_menu { 40.0 } else { 0.0 } + 8.0;
        let name_w = (ui.available_width() - reserve).max(60.0);
        let name_h = ui.spacing().interact_size.y.max(22.0);
        ui.allocate_ui(egui::vec2(name_w, name_h), |ui| {
            if multi {
                let caption = session
                    .messages
                    .format("inspector-multi-entities", &[("count", &ids.len().to_string())]);
                let mut scratch = caption.clone();
                ui.add_enabled_ui(false, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut scratch).desired_width(f32::INFINITY));
                });
            } else {
                let id = primary.id.as_str();
                let draft_key = format!("rename:{id}");
                let mut name = session.draft(&draft_key).unwrap_or_else(|| primary.name.clone());
                let name_label = text("inspector-name");
                let label = if session.draft_is_marked(&draft_key, &primary.name) {
                    session.messages.format("draft-field", &[("label", name_label.as_str())])
                } else {
                    name_label
                };
                let (changed, committed) = PanelFrame::new(ui).draft_field(&label, &mut name);
                if committed {
                    *event = command("outliner.rename", &[("id", id), ("name", name.as_str())]);
                } else if changed {
                    *event = command(
                        "draft.set",
                        &[("key", draft_key.as_str()), ("value", name.as_str())],
                    );
                }
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let lock_tip = text("command-inspector-lock");
            if PanelFrame::new(ui).icon_toggle(Icon::EditLock, locked, &lock_tip) {
                *event = if locked {
                    command("inspector.unlock", &[("panel", panel_id)])
                } else {
                    command("inspector.lock", &[])
                };
            }
            let source_kind = field(primary, "source_kind");
            if source_kind == "prefab" || source_kind == "script" {
                let menu_tip = if source_kind == "prefab" {
                    text("inspector-prefab-actions")
                } else {
                    text("inspector-script-actions")
                };
                let asset = field(primary, "source_asset");
                let open = if source_kind == "prefab" {
                    text("command-prefab-edit")
                } else {
                    text("inspector-open-asset")
                };
                let mut open_clicked = false;
                let mut prefab_pick: Option<&str> = None;
                let mut revert_one: Option<String> = None;
                let apply = text("command-prefab-apply");
                let revert = text("command-prefab-revert");
                let changed = field(primary, "overrides");
                let _ = PanelFrame::new(ui).icon_menu(
                    if source_kind == "prefab" { Icon::EntityPrefab } else { Icon::AssetScript },
                    &menu_tip,
                    |ui| {
                        if ui.button(&open).clicked() {
                            open_clicked = true;
                            ui.close();
                        }
                        if source_kind == "prefab" {
                            let has_changes = !changed.is_empty();
                            if ui.add_enabled(has_changes, egui::Button::new(&apply)).clicked() {
                                prefab_pick = Some("prefab.apply");
                                ui.close();
                            }
                            if ui.add_enabled(has_changes, egui::Button::new(&revert)).clicked() {
                                prefab_pick = Some("prefab.revert");
                                ui.close();
                            }
                            if has_changes {
                                ui.separator();
                                ui.label("This copy's own changes:");
                                for f in changed.split(',').map(str::trim).filter(|f| !f.is_empty())
                                {
                                    if ui.button(format!("Revert {f}")).clicked() {
                                        revert_one = Some(f.to_string());
                                        ui.close();
                                    }
                                }
                            }
                        }
                        if !asset.is_empty() {
                            ui.label(asset);
                        }
                    },
                );
                if open_clicked && source_kind == "prefab" {
                    *event = command("prefab.edit", &[]);
                }
                if let Some(name) = prefab_pick {
                    *event = command(name, &[]);
                }
                if let Some(f) = revert_one {
                    *event = command("prefab.revert", &[("field", f.as_str())]);
                }
            }
            if session.prefab_edit.is_some() {
                let done = text("command-prefab-done");
                if ui.button(&done).on_hover_text(&done).clicked() {
                    *event = command("prefab.done", &[]);
                }
            }
            let nested =
                seal_editor_core::prefab_nest::override_lines(&session.world.entities, &primary.id);
            if !nested.is_empty() {
                ui.label(format!("Nested prefab changes: {}", nested.join("; ")));
            }
            let add_tip = text("inspector-add-component");
            let _ =
                PanelFrame::new(ui).icon_menu_colored(Icon::ViewPlus, &add_tip, STATUS_OK, |ui| {
                    inspector_component_picker(ui, session, primary, event, text);
                });
        });
    });
}

pub fn inspector_property_help(
    session: &EditorSession,
    entity_id: &str,
    label_key: &str,
    field: &str,
    min: &str,
    max: &str,
) -> String {
    let desc = session.messages.get(&format!("{label_key}-desc"));
    let path = format!("{entity_id}.{field}");
    let default = seal_editor_core::inspector_field_default(field);
    session.messages.format(
        "inspector-property-help",
        &[
            ("description", desc.as_str()),
            ("default", default),
            ("min", min),
            ("max", max),
            ("path", path.as_str()),
        ],
    )
}

pub fn inspector_advanced_open(session: &EditorSession, section: &str) -> bool {
    let key = format!("{section}-advanced");
    session.models.inspector_advanced_sections.get(&key).copied().unwrap_or(false)
        || session.models.inspector_view.show_advanced
}

pub fn inspector_section_advanced(
    frame: &mut PanelFrame<'_>,
    session: &EditorSession,
    section: &str,
    event: &mut PanelEvent,
    text: &impl Fn(&str) -> String,
    body: impl FnOnce(&mut PanelFrame),
) {
    let key = format!("{section}-advanced");
    let open = inspector_advanced_open(session, section);
    let caption = text("inspector-advanced");
    if let Some(now) = frame.section_keyed_with_highlight(&key, &caption, open, body, None) {
        *event = command(
            "inspector.advanced_section",
            &[("id", &key), ("open", if now { "1" } else { "0" })],
        );
    }
}

/// One undo step per inspector field edit; number drags coalesce until release; Esc cancels.
#[allow(clippy::too_many_arguments)]
pub fn inspector_field_edit_event(
    session: &EditorSession,
    set_target: (&str, &str),
    field: &str,
    value: &str,
    changed: bool,
    drag_started: bool,
    drag_stopped: bool,
    escape_cancel: bool,
    commit_focus: bool,
) -> PanelEvent {
    if escape_cancel {
        return command("inspector.field_edit_cancel", &[]);
    }
    let in_edit =
        session.models.inspector_field_edit.as_ref().is_some_and(|edit| edit.field == field);
    let needs_begin = !in_edit && (drag_started || changed);
    let coalesced = in_edit || needs_begin;
    let mut cmds: Vec<(String, CommandArgs)> = Vec::new();
    if needs_begin {
        cmds.push((
            "inspector.field_edit_begin".into(),
            CommandArgs::new(&[set_target, ("field", field)]),
        ));
    }
    if changed {
        let mut args = vec![set_target, ("field", field), ("value", value)];
        if coalesced {
            args.push(("preview", "1"));
        }
        cmds.push(("inspector.set".into(), CommandArgs::new(&args)));
    }
    if drag_stopped || (commit_focus && (in_edit || needs_begin)) {
        cmds.push((
            "inspector.field_edit_end".into(),
            CommandArgs::new(&[set_target, ("field", field), ("value", value)]),
        ));
    }
    match cmds.len() {
        0 => PanelEvent::None,
        1 => PanelEvent::Command(cmds[0].0.clone(), cmds[0].1.clone()),
        _ => PanelEvent::Commands(cmds),
    }
}

pub fn apply_property_menu_pick(pick: ContextMenuPick) -> PanelEvent {
    let pairs: Vec<(&str, &str)> =
        pick.args.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    command(pick.command.as_str(), &pairs)
}

pub const INSPECTOR_CATEGORY_CHIPS: &[&str] =
    &["all", "general", "entity", "physics", "rendering", "lod", "streaming", "misc"];

pub fn inspector_section_category(section: &str) -> &'static str {
    match section {
        "inspector-transform" | "inspector-properties" => "general",
        "inspector-tags" | "inspector-tile-data" => "entity",
        "inspector-physics" => "physics",
        "inspector-material"
        | "inspector-material2d"
        | "inspector-color"
        | "inspector-shape"
        | "inspector-sprite-shape"
        | "inspector-light"
        | "inspector-camera" => "rendering",
        _ => "misc",
    }
}

pub fn inspector_category_available(
    category: &str,
    component_pick: &str,
    has_shape: bool,
    all_lights: bool,
    all_cameras: bool,
) -> bool {
    if category == "all" {
        return true;
    }
    if category == "lod" || category == "streaming" {
        return false;
    }
    match category {
        "general" => true,
        "entity" => true,
        "misc" => false,
        "physics" => inspector_section_visible(component_pick, "inspector-physics"),
        "rendering" => {
            has_shape
                || all_lights
                || all_cameras
                || inspector_section_visible(component_pick, "inspector-color")
        }
        _ => false,
    }
}

pub fn inspector_category_matches(section: &str, active: &str) -> bool {
    if active.is_empty() || active == "all" {
        return true;
    }
    inspector_section_category(section) == active
}

fn inspector_settings_menu(
    ui: &mut egui::Ui,
    view: &seal_editor_core::InspectorViewOptions,
    text: &impl Fn(&str) -> String,
) -> Option<PanelEvent> {
    let mut event = None;
    let mut toggle = |id: &'static str, key: &str, on: bool| {
        if event.is_some() {
            return;
        }
        let mut check = on;
        if ui.checkbox(&mut check, text(key)).changed() {
            event = Some(command(
                "inspector.view_option",
                &[("id", id), ("on", if check { "1" } else { "0" })],
            ));
        }
    };
    toggle("only_modified", "inspector-filter-only-modified", view.only_modified);
    toggle("show_advanced", "inspector-filter-show-advanced", view.show_advanced);
    toggle("show_hidden_playing", "inspector-filter-hidden-playing", view.show_hidden_playing);
    ui.separator();
    if event.is_none() && ui.button(text("command-inspector-collapse-all")).clicked() {
        event = Some(command("inspector.collapse_all", &[]));
    }
    if event.is_none() && ui.button(text("command-inspector-expand-all")).clicked() {
        event = Some(command("inspector.expand_all", &[]));
    }
    event
}

pub fn inspector_filter_bar(
    frame: &mut PanelFrame<'_>,
    session: &EditorSession,
    panel_id: &str,
    event: &mut PanelEvent,
    text: &impl Fn(&str) -> String,
) {
    let view = &session.models.inspector_view;
    frame.bar(|bar| {
        let mut query = session.inspector_query_for(panel_id);
        let clear = text("inspector-search-clear");
        if bar.panel_search(&mut query, &text("inspector-search-hint"), &clear) {
            *event = command("inspector.filter", &[("query", query.as_str())]);
        }
        let list_tip = text("inspector-view-list");
        let grid_tip = text("inspector-view-grid");
        if bar.icon_toggle(Icon::ViewList, view.list_view, &list_tip) {
            *event = command("inspector.view_mode", &[("mode", "list")]);
        }
        if bar.icon_toggle(Icon::ViewGrid, !view.list_view, &grid_tip) {
            *event = command("inspector.view_mode", &[("mode", "grid")]);
        }
        let star_tip = text("inspector-favourites-only");
        let star_icon = if view.favourites_only { Icon::ViewStarFilled } else { Icon::ViewStar };
        if bar.icon_toggle(star_icon, view.favourites_only, &star_tip) {
            *event = command(
                "inspector.view_option",
                &[("id", "favourites_only"), ("on", if view.favourites_only { "0" } else { "1" })],
            );
        }
        let settings_tip = text("inspector-settings-menu");
        let _ = bar.icon_menu(Icon::EditSettings, &settings_tip, |ui| {
            ui.set_min_width(220.0);
            if let Some(pick) = inspector_settings_menu(ui, view, text) {
                *event = pick;
            }
        });
    });
}

pub fn inspector_category_bar(
    frame: &mut PanelFrame<'_>,
    session: &EditorSession,
    component_pick: &str,
    has_shape: bool,
    all_lights: bool,
    all_cameras: bool,
    event: &mut PanelEvent,
    text: &impl Fn(&str) -> String,
) {
    let active = session.models.inspector_category.as_str();
    frame.bar(|bar| {
        for id in INSPECTOR_CATEGORY_CHIPS {
            if !inspector_category_available(id, component_pick, has_shape, all_lights, all_cameras)
            {
                continue;
            }
            let label = text(&format!("inspector-category-{id}"));
            let on = active == *id || (active.is_empty() && *id == "all");
            if bar.chip(&label, on) {
                *event = command("inspector.category", &[("id", id)]);
            }
        }
    });
}

/// Project files with one of `exts` (lower case, no dot), as paths relative to
/// the project folder with forward slashes, sorted. Editor and build folders are skipped.
pub(crate) fn project_files_with(session: &EditorSession, exts: &[&str]) -> Vec<String> {
    let Some(root) = session.project.as_ref().map(|p| p.root.clone()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut stack = vec![(root.clone(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "target" || name == "dist" {
                continue;
            }
            if path.is_dir() {
                if depth < 5 {
                    stack.push((path, depth + 1));
                }
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| exts.contains(&e.to_ascii_lowercase().as_str()))
            {
                if let Ok(rel) = path.strip_prefix(&root) {
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    out.sort();
    out
}

/// What a pick in the Add Component menu does to the selected entity.
fn component_pick_event(id: &str, field: &str, value: &str) -> PanelEvent {
    command("inspector.set", &[("id", id), ("field", field), ("value", value)])
}

/// The Add Component menu: each entry really adds that part to the entity.
/// Sprite, Particles and Script list the matching files in the project.
fn inspector_component_picker(
    ui: &mut egui::Ui,
    session: &EditorSession,
    primary: &seal_editor_core::MockNode,
    event: &mut PanelEvent,
    text: &impl Fn(&str) -> String,
) {
    let search_id = egui::Id::new("inspector_component_search");
    let mut query =
        ui.memory_mut(|mem| mem.data.get_temp_mut_or_default::<String>(search_id).clone());
    ui.horizontal(|ui| {
        ui.label(text("inspector-add-component-search"));
        ui.text_edit_singleline(&mut query);
    });
    ui.memory_mut(|mem| *mem.data.get_temp_mut_or_default::<String>(search_id) = query.clone());
    let filter = query.to_lowercase();
    let shows = |label: &str| filter.is_empty() || label.to_lowercase().contains(&filter);
    let id = primary.id.as_str();
    let file_groups: [(&str, &str, &[&str]); 3] = [
        ("inspector-add-component-sprite", "sprite", seal_editor_core::picture::EXTENSIONS),
        ("inspector-add-component-particles", "particles", &["fx"]),
        ("inspector-add-component-script", "script", &["graph"]),
    ];
    ui.label(egui::RichText::new(text("inspector-add-category-rendering")).small().weak());
    for (label_key, field_name, exts) in file_groups {
        let label = text(label_key);
        if !shows(&label) {
            continue;
        }
        let files = project_files_with(session, exts);
        ui.menu_button(label, |ui| {
            if files.is_empty() {
                ui.label(text("inspector-add-component-no-files"));
            }
            for file in &files {
                if ui.button(file).clicked() {
                    *event = component_pick_event(id, field_name, file);
                    ui.close();
                }
            }
        });
    }
    let mask = text("inspector-add-component-sprite-mask");
    if shows(&mask) && ui.button(mask).clicked() {
        *event = component_pick_event(id, "mask_shape", "rect");
        ui.close();
    }
    let mesh = text("inspector-add-component-mesh");
    if shows(&mesh) && ui.button(mesh).clicked() {
        *event = component_pick_event(id, "shape", "cube");
        ui.close();
    }
    ui.separator();
    ui.label(egui::RichText::new(text("inspector-add-category-physics")).small().weak());
    let body = text("inspector-add-component-body");
    if shows(&body) && ui.button(body).clicked() {
        *event = component_pick_event(id, "motion", "dynamic");
        ui.close();
    }
    ui.separator();
    ui.label(egui::RichText::new(text("inspector-add-category-gameplay")).small().weak());
    let player = text("inspector-add-component-player");
    if shows(&player)
        && ui.button(player).on_hover_text(text("inspector-add-component-player-hint")).clicked()
    {
        // The player needs both its role and a moving body, so one click sets both.
        *event = PanelEvent::Commands(vec![
            (
                "inspector.set".into(),
                CommandArgs::new(&[("id", id), ("field", "role"), ("value", "player")]),
            ),
            (
                "inspector.set".into(),
                CommandArgs::new(&[("id", id), ("field", "motion"), ("value", "dynamic")]),
            ),
        ]);
        ui.close();
    }
    let remote = text("inspector-add-component-remote");
    if shows(&remote) && ui.button(remote).clicked() {
        *event = component_pick_event(id, "remote_on", "yes");
        ui.close();
    }
    let roles = [
        ("inspector-add-component-coin", "coin"),
        ("inspector-add-component-goal", "goal"),
        ("inspector-add-component-ground", "ground"),
    ];
    for (label_key, role) in roles {
        let label = text(label_key);
        if shows(&label) && ui.button(label).clicked() {
            *event = component_pick_event(id, "role", role);
            ui.close();
        }
    }
}

#[cfg(test)]
mod area_section_tests {
    #[test]
    fn only_colliders_or_set_effects_show_the_area_section() {
        use super::area_section_wanted as w;
        assert!(!w(Some("none"), Some("none")));
        assert!(!w(None, None));
        assert!(w(Some("none"), Some("fixed")));
        assert!(w(Some("point"), Some("none")));
    }
}
