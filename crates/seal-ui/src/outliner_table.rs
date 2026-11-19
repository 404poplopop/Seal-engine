//! Outliner column header and rows.

use egui::{
    pos2, text::LayoutJob, Align2, Color32, CursorIcon, FontId, Rect, RichText, Sense, Stroke,
    StrokeKind, TextFormat, TextStyle, Ui, WidgetInfo, WidgetType,
};
use seal_editor_core::{
    outliner_drop_verdict, EditorModels, Messages, OutlinerDropMode,
};

use crate::look::{ICON_PX, ROW_PX, SMALL_PX, TREE_INDENT_PX};
use crate::widgets::{filtered_menu, help_focus_ring, row_fill};
use crate::{icons, paint_context_menu, ContextMenuEntry, TreeClick, TreeRow};

pub const EYE_COL: f32 = 22.0;
pub const PIN_COL: f32 = 22.0;
pub const EXTRA_COL: f32 = 52.0;
pub const OPTIONAL_COLUMNS: &[&str] =
    &["layer", "tags", "motion_mode", "source", "id", "unsaved", "level", "unselectable"];

pub use seal_editor_core::OutlinerTable;

#[derive(Clone, Debug)]
pub struct OutlinerColumnLayout {
    pub width: f32,
    pub eye: Rect,
    pub pin: Rect,
    pub label: Rect,
    pub ty: Rect,
    pub extras: Vec<(String, Rect)>,
}

pub fn column_layout(width: f32, table: &OutlinerTable) -> OutlinerColumnLayout {
    let mut x = 0.0;
    let eye = Rect::from_min_size(pos2(x, 0.0), egui::vec2(EYE_COL, ROW_PX));
    x += EYE_COL;
    let pin = Rect::from_min_size(pos2(x, 0.0), egui::vec2(PIN_COL, ROW_PX));
    x += PIN_COL;
    let optional_w = table.visible_optional.len() as f32 * EXTRA_COL;
    let type_left = (width - table.type_width).max(x + 80.0);
    let extras_left = (type_left - optional_w).max(x + 40.0);
    let label = Rect::from_min_max(pos2(x, 0.0), pos2(extras_left, ROW_PX));
    let mut extras = Vec::new();
    let mut ex = extras_left;
    for id in &table.visible_optional {
        extras
            .push((id.clone(), Rect::from_min_size(pos2(ex, 0.0), egui::vec2(EXTRA_COL, ROW_PX))));
        ex += EXTRA_COL;
    }
    let ty = Rect::from_min_max(pos2(type_left, 0.0), pos2(width, ROW_PX));
    OutlinerColumnLayout { width, eye, pin, label, ty, extras }
}

#[derive(Clone, Debug, PartialEq)]
pub enum OutlinerHeaderHit {
    None,
    Sort(String),
    ResizeType(f32),
    ToggleColumn { id: String, on: bool },
}

pub struct OutlinerHeaderLabels<'a> {
    pub item_label: &'a str,
    pub ty: &'a str,
    pub columns_caption: &'a str,
    pub optional: &'a [(&'a str, &'a str)],
    pub column_menu: &'a [(&'a str, &'a str)],
}

fn paint_splitter(ui: &mut Ui, rect: Rect, id: &str) -> Option<f32> {
    let grab = Rect::from_center_size(rect.center(), egui::vec2(5.0, ROW_PX));
    let resp = ui.interact(grab, ui.id().with(id), Sense::drag());
    let resp = resp.on_hover_cursor(egui::CursorIcon::ResizeColumn);
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke.color;
    ui.painter().vline(rect.right(), rect.y_range(), Stroke::new(1.0, stroke));
    if resp.dragged() {
        Some(resp.drag_delta().x)
    } else {
        None
    }
}

/// Level-2 header row under the tool bar (22 px).
pub fn outliner_header(
    ui: &mut Ui,
    table: &OutlinerTable,
    labels: OutlinerHeaderLabels<'_>,
) -> OutlinerHeaderHit {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, ROW_PX), Sense::click());
    let layout = column_layout(width, table);
    let fill = ui.visuals().extreme_bg_color;
    let line = ui.visuals().widgets.noninteractive.bg_stroke;
    ui.painter().rect_filled(rect, 0.0, fill);
    ui.painter().hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, line.color));
    let small = TextStyle::Small.resolve(ui.style());
    let weak = ui.visuals().weak_text_color();
    let mut hit = OutlinerHeaderHit::None;

    let label_center = pos2(rect.left() + layout.label.left() + 4.0, rect.center().y);
    let label_resp = ui.interact(
        layout.label.translate(rect.min.to_vec2()),
        response.id.with("sort-label"),
        Sense::click(),
    );
    let mut label_text = labels.item_label.to_owned();
    if table.sort_column == "label" {
        label_text.push_str(if table.sort_asc { " ⏶" } else { " ⏷" });
    }
    ui.painter().text(label_center, Align2::LEFT_CENTER, label_text, small.clone(), weak);
    if label_resp.clicked() {
        hit = OutlinerHeaderHit::Sort("label".into());
    }

    let type_resp = ui.interact(
        layout.ty.translate(rect.min.to_vec2()),
        response.id.with("sort-type"),
        Sense::click(),
    );
    let mut ty_text = labels.ty.to_owned();
    if table.sort_column == "type" {
        ty_text.push_str(if table.sort_asc { " ⏶" } else { " ⏷" });
    }
    ui.painter().text(
        pos2(rect.left() + layout.ty.left() + 4.0, rect.center().y),
        Align2::LEFT_CENTER,
        ty_text,
        small,
        weak,
    );
    if type_resp.clicked() {
        hit = OutlinerHeaderHit::Sort("type".into());
    }

    let split_rect = Rect::from_min_max(
        pos2(rect.left() + layout.ty.left() - 2.0, rect.top()),
        pos2(rect.left() + layout.ty.left() + 2.0, rect.bottom()),
    );
    if let Some(delta) = paint_splitter(ui, split_rect, "split-type") {
        hit = OutlinerHeaderHit::ResizeType((table.type_width - delta).clamp(48.0, 240.0));
    }

    for (id, col) in &layout.extras {
        let title = labels
            .column_menu
            .iter()
            .find(|(key, _)| *key == id)
            .map(|(_, title)| *title)
            .unwrap_or(id.as_str());
        ui.painter().text(
            pos2(rect.left() + col.left() + 4.0, rect.center().y),
            Align2::LEFT_CENTER,
            title,
            TextStyle::Small.resolve(ui.style()),
            weak,
        );
    }

    response.context_menu(|ui| {
        ui.label(egui::RichText::new(labels.columns_caption).small().weak());
        for (id, title) in labels.column_menu {
            let on = table.visible_optional.iter().any(|col| col == *id);
            let mut check = on;
            if ui.checkbox(&mut check, *title).changed() {
                hit = OutlinerHeaderHit::ToggleColumn { id: (*id).to_string(), on: check };
                ui.close();
            }
        }
    });

    hit
}

pub struct OutlinerTypeAssetLink<'a> {
    pub label: &'a str,
    pub kind: &'a str,
    pub asset: &'a str,
}

pub struct OutlinerRowMarksView<'a> {
    pub unsaved: bool,
    pub prefab_badge: bool,
    pub play_spawned: bool,
    pub warning_row: bool,
    pub warning_tooltip: Option<&'a str>,
}

pub struct OutlinerRowExtra<'a> {
    pub type_text: &'a str,
    pub type_icon: icons::Icon,
    pub type_rgb: [u8; 3],
    /// Link-coloured type cell for prefab/script instances.
    pub type_asset: Option<OutlinerTypeAssetLink<'a>>,
    pub row_marks: OutlinerRowMarksView<'a>,
    /// Dim text after the label (folder child count when collapsed).
    pub label_suffix: Option<&'a str>,
    pub cells: &'a [(&'a str, &'a str)],
    /// Shown on the rename field when the text is empty.
    pub rename_empty_hint: Option<&'a str>,
    pub context_menu: Option<&'a [ContextMenuEntry]>,
    pub context_caption: Option<&'a str>,
}

/// Minimum seconds between clicks to count as a slow double-click (not expand).
const SLOW_DOUBLE_CLICK_MIN: f64 = 0.35;
const SLOW_DOUBLE_CLICK_MAX: f64 = 1.25;

fn drop_mode_at(rect: Rect, pointer: Option<egui::Pos2>) -> OutlinerDropMode {
    let Some(pos) = pointer else {
        return OutlinerDropMode::Parent;
    };
    let t = (pos.y - rect.top()) / rect.height().max(1.0);
    if t < 0.28 {
        OutlinerDropMode::Before
    } else if t > 0.72 {
        OutlinerDropMode::After
    } else {
        OutlinerDropMode::Parent
    }
}

fn paint_drop_hint(
    painter: &egui::Painter,
    rect: Rect,
    mode: OutlinerDropMode,
    allowed: bool,
    accent: Color32,
) {
    let ink = if allowed { accent } else { Color32::from_rgb(210, 72, 72) };
    match mode {
        OutlinerDropMode::Before => {
            let y = rect.top() + 1.0;
            painter.line_segment(
                [pos2(rect.left() + 4.0, y), pos2(rect.right() - 4.0, y)],
                Stroke::new(2.0, ink),
            );
        }
        OutlinerDropMode::After => {
            let y = rect.bottom() - 1.0;
            painter.line_segment(
                [pos2(rect.left() + 4.0, y), pos2(rect.right() - 4.0, y)],
                Stroke::new(2.0, ink),
            );
        }
        OutlinerDropMode::Parent => {
            painter.rect_stroke(rect, 0.0, Stroke::new(1.5, ink), StrokeKind::Inside);
        }
    }
}

fn rename_key_action(ui: &Ui, edit_resp: &egui::Response, text: &str) -> Option<TreeClick> {
    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        return Some(TreeClick::RenameCancel);
    }
    if ui.input(|input| input.key_pressed(egui::Key::Enter)) && edit_resp.has_focus() {
        return Some(TreeClick::Rename(text.to_string()));
    }
    None
}

const EXPANDER_W: f32 = 14.0;

/// One hierarchy row aligned to [`OutlinerTable`] columns.
#[allow(clippy::too_many_arguments)]
pub fn outliner_row(
    ui: &mut Ui,
    table: &OutlinerTable,
    row: TreeRow<'_>,
    extra: OutlinerRowExtra<'_>,
    panel_focused: bool,
    scroll_into_view: bool,
    models: &EditorModels,
    messages: &Messages,
) -> TreeClick {
    let TreeRow {
        depth,
        has_children,
        open,
        label,
        detail: _,
        selected,
        hidden,
        locked,
        menu,
        drag_id,
        place_kind,
        rename_text,
        component_row,
        thumb: _,
        marks: _,
    } = row;
    let width = ui.available_width();
    let layout = column_layout(width, table);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, ROW_PX), Sense::click_and_drag());
    let response = help_focus_ring(ui, response, label);
    response
        .widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, true, selected, label));
    let visuals = ui.visuals().clone();
    let target_id = drag_id.unwrap_or("root");
    if component_row {
        // Component rows are labels only; skip drag-and-drop hit targets.
    } else if let Some(kind) = place_kind {
        if response.dragged() {
            response.dnd_set_drag_payload(format!("place:{kind}"));
        }
    } else if let Some(id) = drag_id {
        response.dnd_set_drag_payload(id.to_string());
    }
    if !component_row {
        let pointer = ui.input(|input| input.pointer.hover_pos());
        if let Some(payload) = response.dnd_hover_payload::<String>() {
            let mode = drop_mode_at(rect, pointer);
            let (allowed, reason) =
                outliner_drop_verdict(models, messages, &payload, target_id, mode);
            paint_drop_hint(ui.painter(), rect, mode, allowed, visuals.selection.bg_fill);
            let hover = response.clone().on_hover_cursor(if allowed {
                CursorIcon::Grab
            } else {
                CursorIcon::NotAllowed
            });
            if let Some(tip) = reason {
                hover.on_hover_text(tip);
            }
        }
        if let Some(payload) = response.dnd_release_payload::<String>() {
            let mode = drop_mode_at(rect, ui.input(|input| input.pointer.interact_pos()));
            let (allowed, _) = outliner_drop_verdict(models, messages, &payload, target_id, mode);
            if !allowed {
                return TreeClick::None;
            }
            if let Some(kind) = seal_editor_core::place_body_from_drag(&payload) {
                return TreeClick::SpawnPlace {
                    parent: target_id.to_string(),
                    kind: kind.to_string(),
                };
            }
            let child = payload;
            if child.as_str() == target_id {
                return TreeClick::None;
            }
            return match mode {
                OutlinerDropMode::Parent => {
                    TreeClick::Reparent { child: child.to_string(), parent: target_id.to_string() }
                }
                OutlinerDropMode::Before => TreeClick::Reorder {
                    child: child.to_string(),
                    before: Some(target_id.to_string()),
                    after: None,
                },
                OutlinerDropMode::After => TreeClick::Reorder {
                    child: child.to_string(),
                    before: None,
                    after: Some(target_id.to_string()),
                },
            };
        }
    }
    let ink = row_fill(ui, rect, &response, selected, panel_focused);
    let mut text_ink = ink.unwrap_or(visuals.text_color());
    let mut dim = ink.unwrap_or(visuals.weak_text_color());
    if hidden {
        text_ink = text_ink.gamma_multiply(0.42);
        dim = dim.gamma_multiply(0.42);
    }
    if component_row {
        text_ink = text_ink.gamma_multiply(0.58);
        dim = dim.gamma_multiply(0.58);
    }
    if extra.row_marks.warning_row && !component_row {
        text_ink = visuals.warn_fg_color;
    }

    let row_off = rect.min.to_vec2();
    let eye_rect = layout.eye.translate(row_off);
    let pin_rect = layout.pin.translate(row_off);
    let eye_hit = ui.interact(eye_rect, response.id.with("eye"), Sense::click());
    let pin_hit = ui.interact(pin_rect, response.id.with("lock"), Sense::click());
    let show_eye = !component_row && (hidden || response.hovered() || eye_hit.hovered());
    if show_eye {
        icons::paint_icon(
            ui.painter(),
            eye_rect.center(),
            if hidden { icons::Icon::ViewEyeOff } else { icons::Icon::ViewEye },
            14.0,
            if hidden { dim.gamma_multiply(0.45) } else { dim },
        );
    }
    let show_pin = !component_row && (locked || response.hovered() || pin_hit.hovered());
    if show_pin {
        icons::paint_icon(
            ui.painter(),
            pin_rect.center(),
            if locked { icons::Icon::EditLock } else { icons::Icon::EditUnlock },
            14.0,
            if locked { visuals.hyperlink_color } else { dim },
        );
    }
    let mut click = TreeClick::None;
    if eye_hit.clicked() {
        let solo = ui.input(|input| input.modifiers.alt);
        click = if solo { TreeClick::EyeSolo } else { TreeClick::Eye };
    } else if pin_hit.clicked() {
        click = TreeClick::Lock;
    }

    let indent = depth as f32 * TREE_INDENT_PX;
    let row_base = rect.left() + layout.label.left() + 4.0 + indent;
    let mark = Rect::from_center_size(
        pos2(row_base + EXPANDER_W * 0.5, rect.center().y),
        egui::vec2(EXPANDER_W, EXPANDER_W),
    );
    if has_children {
        let toggle = ui.interact(mark.expand(3.0), response.id.with("mark"), Sense::click());
        let openness = if open { 1.0 } else { 0.0 };
        if toggle.clicked() {
            click = TreeClick::Toggle;
        }
        egui::collapsing_header::paint_default_icon(ui, openness, &toggle.with_new_rect(mark));
    }
    let icon_left = row_base + EXPANDER_W + 2.0;
    let icon_color = Color32::from_rgb(extra.type_rgb[0], extra.type_rgb[1], extra.type_rgb[2]);
    let icon_center = pos2(icon_left + ICON_PX * 0.5, rect.center().y);
    if icons::icon_font_ready(ui.ctx()) {
        icons::paint_icon(ui.painter(), icon_center, extra.type_icon, ICON_PX, icon_color);
    } else {
        let chip = Rect::from_center_size(icon_center, egui::vec2(ICON_PX - 2.0, ICON_PX - 2.0));
        ui.painter().rect_filled(chip, 0.0, icon_color);
        ui.ctx().request_repaint();
    }
    let mut label_left = icon_left + ICON_PX + 4.0;
    if extra.row_marks.prefab_badge && !component_row {
        let badge_center = pos2(label_left + 5.0, rect.center().y);
        if icons::icon_font_ready(ui.ctx()) {
            icons::paint_icon(
                ui.painter(),
                badge_center,
                icons::Icon::EntityPrefab,
                10.0,
                icon_color,
            );
        }
        label_left += 14.0;
    }

    if let Some(tip) = extra.row_marks.warning_tooltip.filter(|_| !component_row) {
        response.clone().on_hover_text(tip);
    }

    if let Some(text) = rename_text {
        let width = (layout.label.right() - label_left - 8.0).max(40.0);
        let edit_id = egui::Id::new(("outliner-table-rename", drag_id.unwrap_or(label)));
        let selected_all = crate::widgets::begin_rename_edit(ui.ctx(), edit_id, text);
        let edit = egui::TextEdit::singleline(text).id(edit_id).desired_width(width);
        let edit_resp = crate::crisp::put_well(
            ui,
            Rect::from_center_size(
                pos2(label_left + width * 0.5, rect.center().y),
                egui::vec2(width, ROW_PX),
            ),
            edit,
        );
        edit_resp.request_focus();
        if selected_all {
            crate::widgets::paint_rename_selection(
                ui,
                edit_resp.rect,
                text,
                TextStyle::Body,
                false,
            );
        }
        if text.trim().is_empty() {
            if let Some(hint) = extra.rename_empty_hint {
                edit_resp.clone().on_hover_text(hint);
            }
        }
        if let Some(action) = rename_key_action(ui, &edit_resp, text) {
            return action;
        }
    } else {
        let font = TextStyle::Body.resolve(ui.style());
        let clip = layout.label.translate(row_off);
        let mut label_x = label_left;
        if extra.row_marks.play_spawned && !component_row {
            let mut job = LayoutJob::default();
            job.append(
                label,
                0.0,
                TextFormat {
                    font_id: font.clone(),
                    color: text_ink,
                    italics: true,
                    ..Default::default()
                },
            );
            let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
            ui.painter().with_clip_rect(clip).galley(
                pos2(label_x, rect.center().y - galley.size().y * 0.5),
                galley,
                text_ink,
            );
            label_x +=
                ui.painter().layout_no_wrap(label.to_owned(), font.clone(), text_ink).size().x;
        } else {
            ui.painter().with_clip_rect(clip).text(
                pos2(label_x, rect.center().y),
                Align2::LEFT_CENTER,
                label,
                font.clone(),
                text_ink,
            );
            label_x +=
                ui.painter().layout_no_wrap(label.to_owned(), font.clone(), text_ink).size().x;
        }
        if extra.row_marks.unsaved && !component_row {
            ui.painter().with_clip_rect(clip).text(
                pos2(label_x + 2.0, rect.center().y),
                Align2::LEFT_CENTER,
                "*",
                font.clone(),
                text_ink,
            );
            label_x +=
                ui.painter().layout_no_wrap("*".to_owned(), font.clone(), text_ink).size().x + 2.0;
        }
        if let Some(suffix) = extra.label_suffix {
            ui.painter().with_clip_rect(clip).text(
                pos2(label_x + 6.0, rect.center().y),
                Align2::LEFT_CENTER,
                suffix,
                FontId::proportional(SMALL_PX),
                dim,
            );
        }
    }

    let ty_rect = layout.ty.translate(row_off);
    if let Some(link) = extra.type_asset.filter(|_| !component_row) {
        let link_text = RichText::new(link.label)
            .font(FontId::proportional(SMALL_PX))
            .color(visuals.hyperlink_color);
        // One line, cut with an ellipsis when the column is narrow; hover shows the full text.
        let link_rect = egui::Rect::from_min_max(
            pos2(ty_rect.left() + 4.0, ty_rect.top()),
            pos2(ty_rect.right().max(ty_rect.left() + 8.0), ty_rect.bottom()),
        );
        let link_resp = ui.put(
            link_rect,
            egui::Label::new(link_text).truncate().halign(egui::Align::LEFT).sense(Sense::click()),
        );
        if link_resp.clicked() {
            return TreeClick::EditAsset {
                kind: link.kind.to_string(),
                asset: link.asset.to_string(),
            };
        }
        if link_resp.hovered() {
            link_resp.clone().on_hover_cursor(CursorIcon::PointingHand);
        }
    } else {
        ui.painter().with_clip_rect(ty_rect).text(
            pos2(ty_rect.left() + 4.0, rect.center().y),
            Align2::LEFT_CENTER,
            extra.type_text,
            FontId::proportional(SMALL_PX),
            dim.gamma_multiply(0.95),
        );
    }

    for (col_id, col_rect) in &layout.extras {
        let value = extra.cells.iter().find(|(id, _)| id == col_id).map(|(_, v)| *v).unwrap_or("—");
        let col_world = col_rect.translate(row_off);
        if *col_id == "unselectable" && !component_row {
            let hit = ui.interact(col_world, response.id.with(("unsel", col_id)), Sense::click());
            ui.painter().with_clip_rect(col_world).text(
                pos2(col_world.left() + 4.0, col_world.center().y),
                Align2::LEFT_CENTER,
                value,
                FontId::proportional(SMALL_PX),
                if value == "yes" { visuals.warn_fg_color } else { dim },
            );
            if hit.clicked() {
                click = TreeClick::Unselectable;
            }
        } else {
            ui.painter().with_clip_rect(col_world).text(
                pos2(col_world.left() + 4.0, col_world.center().y),
                Align2::LEFT_CENTER,
                value,
                FontId::proportional(SMALL_PX),
                dim,
            );
        }
    }

    if let Some(entries) = extra.context_menu.filter(|_| !component_row) {
        let caption = extra.context_caption;
        response.context_menu(|ui| {
            if let Some(pick) = paint_context_menu(ui, caption, entries) {
                click = TreeClick::ContextPick(pick);
            }
        });
    } else if !menu.is_empty() {
        response.context_menu(|ui| {
            for index in filtered_menu(ui, menu) {
                let entry = &menu[index];
                let item = ui.button(entry).on_hover_text(entry.as_str());
                if item.clicked() {
                    click = TreeClick::Menu(index);
                    ui.close();
                }
            }
        });
    }

    if matches!(click, TreeClick::None) {
        if response.double_clicked() && has_children {
            click = TreeClick::Toggle;
        } else if response.double_clicked() && drag_id.is_some() {
            click = TreeClick::FrameEntity;
        } else if response.secondary_clicked() && !selected && drag_id.is_some() {
            // Right-clicking a row acts on that row, not on an older selection.
            click = TreeClick::Select;
        } else if response.clicked() {
            let (shift, ctrl) = ui.input(|input| (input.modifiers.shift, input.modifiers.ctrl));
            if !shift && !ctrl && drag_id.is_some() {
                let now = ui.input(|input| input.time);
                let click_key = response.id.with("slow_rename_click");
                let last: Option<f64> = ui.data(|data| data.get_temp(click_key));
                let slow = last.is_some_and(|t| {
                    let gap = now - t;
                    gap > SLOW_DOUBLE_CLICK_MIN && gap < SLOW_DOUBLE_CLICK_MAX
                });
                ui.data_mut(|data| data.insert_temp(click_key, now));
                if slow {
                    click = TreeClick::BeginRename;
                } else if shift && !ctrl {
                    click = TreeClick::SelectRange;
                } else if ctrl {
                    click = TreeClick::SelectExtend;
                } else {
                    click = TreeClick::Select;
                }
            } else {
                click = if shift && !ctrl {
                    TreeClick::SelectRange
                } else if ctrl {
                    TreeClick::SelectExtend
                } else {
                    TreeClick::Select
                };
            }
        }
    }
    if scroll_into_view {
        response.scroll_to_me(Some(egui::Align::Center));
    }
    click
}

pub fn paint_outliner_header(
    ui: &mut Ui,
    table: &OutlinerTable,
    labels: OutlinerHeaderLabels<'_>,
) -> OutlinerHeaderHit {
    outliner_header(ui, table, labels)
}

#[allow(clippy::too_many_arguments)]
pub fn paint_outliner_row(
    ui: &mut Ui,
    table: &OutlinerTable,
    row: TreeRow<'_>,
    extra: OutlinerRowExtra<'_>,
    panel_focused: bool,
    scroll_into_view: bool,
    models: &EditorModels,
    messages: &Messages,
) -> TreeClick {
    outliner_row(
        ui,
        table,
        row,
        extra,
        panel_focused,
        scroll_into_view,
        models,
        messages,
    )
}
