//! Context menus with the same row chrome as the menu bar.

use egui::{Align2, TextStyle};

use crate::icons::{paint_icon, Icon};
use crate::look::MENU_ROW_PX;

#[derive(Clone)]
pub enum ContextMenuEntry {
    Section(String),
    Row(ContextMenuRow),
}

#[derive(Clone)]
pub struct ContextMenuRow {
    pub label: String,
    pub icon: Icon,
    pub shortcut: String,
    pub enabled: bool,
    pub reason: String,
    pub command: Option<(String, Vec<(String, String)>)>,
    pub children: Vec<ContextMenuEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextMenuPick {
    pub command: String,
    pub args: Vec<(String, String)>,
}

pub struct ViewportContextMenu<'a> {
    pub caption: Option<&'a str>,
    pub entries: &'a [ContextMenuEntry],
}

/// Dark panel with optional caption, search when long, icon rows and submenus.
pub fn paint_context_menu(
    ui: &mut egui::Ui,
    caption: Option<&str>,
    entries: &[ContextMenuEntry],
) -> Option<ContextMenuPick> {
    let frame = egui::Frame::popup(ui.style()).inner_margin(egui::Margin::symmetric(4, 2));
    let mut chosen = None;
    frame.show(ui, |ui| {
        ui.set_min_width(280.0);
        if let Some(title) = caption {
            ui.add_space(2.0);
            ui.label(egui::RichText::new(title).strong());
            ui.separator();
        }
        let query = search_query(ui, entries);
        let shown = filter_entries(entries, &query);
        if shown.is_empty() {
            let miss = ui
                .ctx()
                .data(|data| data.get_temp::<String>(egui::Id::new("seal-menu-miss")))
                .unwrap_or_else(|| "menu-no-matches".to_string());
            ui.label(egui::RichText::new(miss).weak());
            return;
        }
        for entry in &shown {
            match entry {
                ContextMenuEntry::Section(title) => section(ui, title),
                ContextMenuEntry::Row(row) => {
                    if let Some(pick) = show_row(ui, row) {
                        chosen = Some(pick);
                    }
                }
            }
        }
    });
    chosen
}

fn search_query(ui: &mut egui::Ui, entries: &[ContextMenuEntry]) -> String {
    if count_rows(entries) < 8 {
        return String::new();
    }
    let id = ui.id().with("ctx-search");
    let mut query = ui.ctx().data(|data| data.get_temp::<String>(id).unwrap_or_default());
    let hint = ui
        .ctx()
        .data(|data| data.get_temp::<String>(egui::Id::new("seal-menu-search-hint")))
        .unwrap_or_default();
    ui.add(egui::TextEdit::singleline(&mut query).hint_text(hint).desired_width(f32::INFINITY));
    ui.ctx().data_mut(|data| data.insert_temp(id, query.clone()));
    query
}

fn count_rows(entries: &[ContextMenuEntry]) -> usize {
    entries
        .iter()
        .map(|entry| match entry {
            ContextMenuEntry::Section(_) => 0,
            ContextMenuEntry::Row(row) => 1 + count_rows(&row.children),
        })
        .sum()
}

fn filter_entries(entries: &[ContextMenuEntry], query: &str) -> Vec<ContextMenuEntry> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return entries.to_vec();
    }
    let mut out = Vec::new();
    let mut pending: Option<String> = None;
    for entry in entries {
        match entry {
            ContextMenuEntry::Section(title) => pending = Some(title.clone()),
            ContextMenuEntry::Row(row) => {
                let children = filter_entries(&row.children, query);
                let hit = row.label.to_lowercase().contains(&needle) || !children.is_empty();
                if !hit {
                    continue;
                }
                if let Some(title) = pending.take() {
                    out.push(ContextMenuEntry::Section(title));
                }
                let mut copy = row.clone();
                if !children.is_empty() {
                    copy.children = children;
                }
                out.push(ContextMenuEntry::Row(copy));
            }
        }
    }
    out
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title.to_uppercase()).small().weak());
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 10.0), egui::Sense::hover());
        ui.painter().hline(
            rect.x_range(),
            rect.center().y,
            ui.visuals().widgets.noninteractive.bg_stroke,
        );
    });
}

fn show_row(ui: &mut egui::Ui, row: &ContextMenuRow) -> Option<ContextMenuPick> {
    let height = MENU_ROW_PX;
    let width = ui.available_width().max(260.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    if row.enabled && response.hovered() {
        ui.painter().rect_filled(rect, 0.0, ui.visuals().widgets.hovered.bg_fill);
    }
    let ink = if row.enabled {
        ui.visuals().text_color()
    } else {
        ui.visuals().text_color().gamma_multiply(0.4)
    };
    let painter = ui.painter();
    paint_icon(painter, egui::pos2(rect.left() + 28.0, rect.center().y), row.icon, 16.0, ink);
    painter.text(
        egui::pos2(rect.left() + 44.0, rect.center().y),
        Align2::LEFT_CENTER,
        &row.label,
        TextStyle::Body.resolve(ui.style()),
        ink,
    );
    let trailing = if row.children.is_empty() { row.shortcut.as_str() } else { "▶" };
    if !trailing.is_empty() {
        painter.text(
            egui::pos2(rect.right() - 8.0, rect.center().y),
            Align2::RIGHT_CENTER,
            trailing,
            TextStyle::Small.resolve(ui.style()),
            ui.visuals().weak_text_color(),
        );
    }
    if !row.enabled && !row.reason.is_empty() {
        response.clone().on_hover_text(&row.reason);
    }
    if !row.children.is_empty() {
        let mut pick = None;
        egui::containers::menu::SubMenu::new().show(ui, &response, |ui| {
            for entry in &row.children {
                match entry {
                    ContextMenuEntry::Section(title) => section(ui, title),
                    ContextMenuEntry::Row(child) => {
                        if let Some(chosen) = show_row(ui, child) {
                            pick = Some(chosen);
                        }
                    }
                }
            }
        });
        return pick;
    }
    if row.enabled {
        if let Some((command, args)) = row.command.clone() {
            if response.clicked() {
                ui.close();
                return Some(ContextMenuPick { command, args });
            }
        }
    }
    None
}
