use std::collections::{HashMap, HashSet};

use egui_dock::{
    AllowedSplits, DockArea, DockState, Node, NodeIndex, NodePath, Split, SurfaceIndex, TabIndex,
    TabInsert,
};
use seal_editor_core::{
    allows_multiple_instances, next_instance_id, panel_base, Axis, DockNode, DockSide,
    FloatingWindow, Layout,
};

#[derive(Clone, Copy)]
struct RequestedWindow {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

pub struct BuiltDock {
    pub state: DockState<String>,
    requested: Vec<RequestedWindow>,
}

pub struct DockHost {
    pub state: DockState<String>,
    /// Sizes requested before the toolkit has painted the floating windows.
    requested: Vec<RequestedWindow>,
    badges: HashMap<String, String>,
}

/// Result of a tab context-menu action after the dock has finished painting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TabMenuResult {
    None,
    Maximize,
    ToggleLayoutLock,
    PinSidebar(String),
}

const TAB_CLOSE_PX: f32 = 12.0;
const TAB_ICON_PAD: f32 = 26.0;

impl DockHost {
    pub fn from_layout(layout: &Layout) -> Self {
        let built = dock_from_layout(layout);
        Self { state: built.state, requested: built.requested, badges: HashMap::new() }
    }

    pub fn to_layout(&self) -> Layout {
        let mut layout = layout_from_dock(&self.state);
        for (window, requested) in layout.floating.iter_mut().zip(&self.requested) {
            if !window.width.is_finite() || window.width <= 0.0 {
                window.x = requested.x;
                window.y = requested.y;
                window.width = requested.width;
                window.height = requested.height;
            }
        }
        layout
    }

    pub fn has_panel(&self, id: &str) -> bool {
        self.state.find_tab(&id.to_string()).is_some()
    }

    pub fn tab_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        collect_panel_ids(&self.to_layout().root, &mut ids);
        for window in &self.to_layout().floating {
            collect_panel_ids(&window.node, &mut ids);
        }
        ids
    }

    /// Optional label shown after the panel title on its dock tab. An empty `text` clears the badge.
    pub fn set_badge(&mut self, id: &str, text: &str) {
        if text.is_empty() {
            self.badges.remove(id);
        } else {
            self.badges.insert(id.to_string(), text.to_string());
        }
    }

    pub fn tab_badges(&self) -> &HashMap<String, String> {
        &self.badges
    }

    /// Bring a panel to the front, or put it back if its tab was closed.
    /// Move the named tab. A tab that is alone in its leaf stays put, because a split needs a neighbor.
    pub fn apply_side(&mut self, id: &str, side: DockSide) {
        let Some(path) = self.state.find_tab(&id.to_string()) else {
            return;
        };
        match side {
            DockSide::Close => {
                self.state.remove_tab(path);
            }
            DockSide::Float => {
                let rect =
                    egui::Rect::from_min_size(egui::pos2(96.0, 72.0), egui::vec2(520.0, 360.0));
                self.state.detach_tab(path, rect);
            }
            DockSide::Reload => {}
            DockSide::Left | DockSide::Right | DockSide::Top | DockSide::Bottom => {
                let count =
                    self.state.leaf(path.node_path()).map(|leaf| leaf.tabs.len()).unwrap_or(0);
                if count < 2 {
                    return;
                }
                let split = match side {
                    DockSide::Left => Split::Left,
                    DockSide::Right => Split::Right,
                    DockSide::Top => Split::Above,
                    DockSide::Bottom => Split::Below,
                    _ => return,
                };
                self.state.move_tab(path, (path.node_path(), TabInsert::Split(split)));
            }
        }
    }

    pub fn show_panel(&mut self, id: &str) {
        match self.state.find_tab(&id.to_string()) {
            Some(path) => {
                let _ = self.state.set_active_tab(path);
            }
            None => {
                // Editors of files open as a big tab beside the main Viewport.
                if id == "asset" {
                    if let Some(path) = self.state.find_tab(&"viewport".to_string()) {
                        self.state.set_focused_node_and_surface(path.node_path());
                    }
                }
                self.state.push_to_focused_leaf(id.to_string())
            }
        }
    }

    /// Open another instance of a multi-instance panel beside the source tab.
    pub fn clear_main_tabs(&mut self) {
        loop {
            let Some(path) = self
                .state
                .iter_all_tabs()
                .find_map(|(path, _)| (path.surface.is_main()).then_some(path))
            else {
                break;
            };
            self.state.remove_tab(path);
        }
    }

    pub fn float_panel(&mut self, id: &str, rect: egui::Rect) -> bool {
        let Some(path) = self.state.find_tab(&id.to_string()) else {
            return false;
        };
        self.state.detach_tab(path, rect);
        true
    }

    pub fn duplicate_tab(&mut self, source_id: &str) -> Option<String> {
        let base = panel_base(source_id);
        if !allows_multiple_instances(base) {
            return None;
        }
        let existing = self.tab_ids();
        let new_id = next_instance_id(existing.iter().map(String::as_str), base);
        let path = self.state.find_tab(&source_id.to_string())?;
        let leaf_path = path.node_path();
        let Ok(leaf) = self.state.leaf_mut(leaf_path) else {
            return None;
        };
        leaf.tabs.push(new_id.clone());
        leaf.active = TabIndex(leaf.tabs.len().saturating_sub(1));
        Some(new_id)
    }

    /// Nudge the split beside the focused tab in the panel leaf. `vertical` picks a vertical split.
    pub fn set_root_horizontal_fraction(&mut self, fraction: f32) {
        let fraction = fraction.clamp(0.15, 0.9);
        let tree = self.state.main_surface_mut();
        let root = NodeIndex::root();
        if let Node::Horizontal(split) = &mut tree[root] {
            split.fraction = fraction;
        }
    }

    pub fn nudge_split(&mut self, id: &str, vertical: bool, delta: f32) {
        let Some(path) = self.state.find_tab(&id.to_string()) else {
            return;
        };
        let Some(focused) = self.state.focused_leaf() else {
            return;
        };
        if focused != path.node_path() {
            return;
        }
        let Ok(leaf) = self.state.leaf(path.node_path()) else {
            return;
        };
        if leaf.active != path.tab {
            return;
        }
        let Some(parent) = path.node.parent() else {
            return;
        };
        let parent_path = NodePath::new(path.surface, parent);
        let Ok(parent_node) = self.state.node_mut(parent_path) else {
            return;
        };
        let split = match parent_node {
            Node::Vertical(split) if vertical => split,
            Node::Horizontal(split) if !vertical => split,
            _ => return,
        };
        split.fraction = (split.fraction + delta).clamp(0.15, 0.85);
    }
}

pub fn show_dock(
    ui: &mut egui::Ui,
    state: &mut DockState<String>,
    titles: &dyn Fn(&str) -> String,
    menu: &[String],
    locked: bool,
    body: &mut dyn FnMut(&mut egui::Ui, &str),
) -> TabMenuResult {
    show_dock_with_badges(ui, state, titles, menu, locked, body, None)
}

/// Tab label, with a non-empty badge appended after the title.
pub fn tab_title(title: &str, badge: Option<&str>) -> String {
    match badge {
        Some(badge) if !badge.is_empty() => format!("{title} {badge}"),
        _ => title.to_string(),
    }
}

pub fn show_dock_with_badges(
    ui: &mut egui::Ui,
    state: &mut DockState<String>,
    titles: &dyn Fn(&str) -> String,
    menu: &[String],
    locked: bool,
    body: &mut dyn FnMut(&mut egui::Ui, &str),
    badges: Option<&HashMap<String, String>>,
) -> TabMenuResult {
    let active_tabs = open_tab_ids(state);
    let mut viewer = PanelViewer {
        titles,
        menu,
        body,
        badges,
        active_tabs,
        accent: ui.visuals().hyperlink_color,
        weak_ink: ui.visuals().weak_text_color(),
        text_ink: ui.visuals().text_color(),
    };
    let mut area = DockArea::new(state)
        .style(crate::look::dock_style(ui.style()))
        .draggable_tabs(!locked)
        .show_close_buttons(false)
        .show_leaf_close_all_buttons(false)
        .show_leaf_collapse_buttons(false)
        .tab_context_menus(!locked);
    if locked {
        area = area.allowed_splits(AllowedSplits::None);
    }
    area.show_inside(ui, &mut viewer);
    if state.main_surface().is_empty() {
        crate::logo::paint_dock_watermark(ui, ui.clip_rect());
    }
    let pending = ui.data(|data| data.get_temp::<(String, usize)>(egui::Id::new("seal-tab-menu")));
    ui.data_mut(|data| data.remove::<(String, usize)>(egui::Id::new("seal-tab-menu")));
    let menu = pending.map(|(id, action)| apply_tab_menu(state, &id, action));
    if ui.ctx().data(|d| d.get_temp::<bool>(egui::Id::new("seal-tab-maximize"))) == Some(true) {
        ui.ctx().data_mut(|d| d.remove::<bool>(egui::Id::new("seal-tab-maximize")));
        return TabMenuResult::Maximize;
    }
    if let Some(id) = ui.ctx().data(|d| d.get_temp::<String>(egui::Id::new("seal-tab-close"))) {
        ui.ctx().data_mut(|d| d.remove::<String>(egui::Id::new("seal-tab-close")));
        if let Some(path) = state.find_tab(&id) {
            state.remove_tab(path);
        }
        return menu.unwrap_or(TabMenuResult::None);
    }
    menu.unwrap_or(TabMenuResult::None)
}

fn open_tab_ids(state: &DockState<String>) -> HashSet<String> {
    let mut ids = HashSet::new();
    for (_index, surface) in state.iter_surfaces_indexed() {
        for (_node_index, node) in surface.iter_nodes_indexed() {
            if let egui_dock::Node::Leaf(leaf) = node {
                if let Some(tab) = leaf.tabs.get(leaf.active.0) {
                    ids.insert(tab.clone());
                }
            }
        }
    }
    ids
}

/// Tab context menu: close variants, float, sidebar, maximize, lock layout.
pub fn apply_tab_menu(state: &mut DockState<String>, id: &str, action: usize) -> TabMenuResult {
    let Some(path) = state.find_tab(&id.to_string()) else {
        return TabMenuResult::None;
    };
    match action {
        0 => {
            state.remove_tab(path);
            TabMenuResult::None
        }
        1 => {
            let others = state.leaf(path.node_path()).map(|leaf| {
                leaf.tabs.iter().filter(|tab| tab.as_str() != id).cloned().collect::<Vec<_>>()
            });
            if let Ok(others) = others {
                for other in others {
                    if let Some(path) = state.find_tab(&other) {
                        state.remove_tab(path);
                    }
                }
            }
            TabMenuResult::None
        }
        2 => {
            close_tabs_to_right(state, id);
            TabMenuResult::None
        }
        3 => {
            let rect = egui::Rect::from_min_size(egui::pos2(96.0, 72.0), egui::vec2(520.0, 360.0));
            state.detach_tab(path, rect);
            TabMenuResult::None
        }
        4 => TabMenuResult::PinSidebar(id.to_string()),
        5 => TabMenuResult::Maximize,
        6 => TabMenuResult::ToggleLayoutLock,
        _ => TabMenuResult::None,
    }
}

fn close_tabs_to_right(state: &mut DockState<String>, id: &str) {
    let Some(path) = state.find_tab(&id.to_string()) else {
        return;
    };
    let Ok(leaf) = state.leaf(path.node_path()) else {
        return;
    };
    let index = path.tab.0;
    let trailing: Vec<String> = leaf.tabs.iter().skip(index.saturating_add(1)).cloned().collect();
    for tab_id in trailing {
        if let Some(tab_path) = state.find_tab(&tab_id) {
            state.remove_tab(tab_path);
        }
    }
}

fn panel_icon(kind: &str) -> crate::icons::Icon {
    use crate::icons::Icon;
    match kind {
        "viewport" => Icon::ViewGrid,
        "outliner" => Icon::ViewList,
        "inspector" => Icon::ViewColumns,
        "content" => Icon::FolderOpen,
        "console" => Icon::File,
        "welcome" => Icon::ViewHome,
        "settings" => Icon::EditSettings,
        "place" => Icon::ViewPlus,
        "world" => Icon::EditSettings,
        "stats" => Icon::StatusInfo,
        "recovery" => Icon::FileRevert,
        "commands" => Icon::EditSearch,
        "notices" => Icon::StatusBell,
        "layouts" => Icon::ViewColumns,
        "keymap" => Icon::EditSettings,
        "dev" => Icon::EditMore,
        "graph" => Icon::AssetScript,
        "assistant" => Icon::StatusInfo,
        "timeline" => Icon::ViewList,
        "asset" => Icon::AssetTexture,
        _ => Icon::File,
    }
}

fn tab_close_clicked(
    ctx: &egui::Context,
    tab_rect: egui::Rect,
    id: egui::Id,
    accent: egui::Color32,
    weak: egui::Color32,
) -> bool {
    let close_rect = egui::Rect::from_min_size(
        egui::pos2(tab_rect.right() - TAB_CLOSE_PX - 4.0, tab_rect.center().y - TAB_CLOSE_PX * 0.5),
        egui::vec2(TAB_CLOSE_PX, TAB_CLOSE_PX),
    );
    let mut clicked = false;
    egui::Area::new(id).interactable(true).fixed_pos(close_rect.min).show(ctx, |ui| {
        let (rect, response) = ui.allocate_exact_size(close_rect.size(), egui::Sense::click());
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.hovered() {
            ui.painter().rect_filled(rect, 0.0, weak.gamma_multiply(0.25));
        }
        let stroke = if response.hovered() { accent } else { weak };
        let pad = 3.0;
        // Paint in a fixed square so a taller button never stretches the cross.
        let r = egui::Rect::from_center_size(
            response.rect.center(),
            egui::vec2(TAB_CLOSE_PX, TAB_CLOSE_PX),
        );
        ui.painter().line_segment(
            [r.left_top() + egui::vec2(pad, pad), r.right_bottom() - egui::vec2(pad, pad)],
            egui::Stroke::new(1.25, stroke),
        );
        ui.painter().line_segment(
            [r.right_top() + egui::vec2(-pad, pad), r.left_bottom() + egui::vec2(pad, -pad)],
            egui::Stroke::new(1.25, stroke),
        );
        clicked = response.clicked();
    });
    clicked
}

fn collect_panel_ids(node: &DockNode, out: &mut Vec<String>) {
    match node {
        DockNode::Tabs { panels, .. } => out.extend(panels.iter().cloned()),
        DockNode::Split { first, second, .. } => {
            collect_panel_ids(first, out);
            collect_panel_ids(second, out);
        }
    }
}

struct PanelViewer<'a> {
    titles: &'a dyn Fn(&str) -> String,
    menu: &'a [String],
    body: &'a mut dyn FnMut(&mut egui::Ui, &str),
    badges: Option<&'a HashMap<String, String>>,
    active_tabs: HashSet<String>,
    accent: egui::Color32,
    weak_ink: egui::Color32,
    text_ink: egui::Color32,
}

impl egui_dock::TabViewer for PanelViewer<'_> {
    type Tab = String;

    fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
        egui::Id::new(tab.clone())
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        let base = (self.titles)(tab);
        let pad = " ".repeat((TAB_ICON_PAD / 4.0) as usize);
        let badge = self.badges.and_then(|badges| badges.get(tab.as_str()).map(String::as_str));
        // Trailing room so the close cross never sits on the title.
        let tail = " ".repeat(((TAB_CLOSE_PX + 8.0) / 4.0) as usize);
        let text = format!("{}{tail}", tab_title(&format!("{pad}{base}"), badge));
        text.into()
    }

    fn on_tab_button(&mut self, tab: &mut Self::Tab, response: &egui::Response) {
        let rect = response.rect;
        let active = self.active_tabs.contains(tab);
        let kind = panel_base(tab);
        let icon = panel_icon(kind);
        let ink = if active || response.hovered() { self.text_ink } else { self.weak_ink };
        crate::icons::paint_icon(
            &response.ctx.layer_painter(response.layer_id),
            egui::pos2(rect.left() + 8.0 + crate::ICON_PX * 0.5, rect.center().y),
            icon,
            crate::ICON_PX,
            ink,
        );
        if active {
            response.ctx.layer_painter(response.layer_id).hline(
                rect.x_range(),
                rect.top(),
                egui::Stroke::new(2.0, self.accent),
            );
        }
        if (active || response.hovered())
            && tab_close_clicked(
                &response.ctx,
                rect,
                response.id.with("seal-tab-close"),
                self.accent,
                self.weak_ink,
            )
        {
            response.ctx.data_mut(|d| d.insert_temp(egui::Id::new("seal-tab-close"), tab.clone()));
        }
        if response.double_clicked() {
            response.ctx.data_mut(|d| d.insert_temp(egui::Id::new("seal-tab-maximize"), true));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        (self.body)(ui, tab);
    }

    fn context_menu(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab, _path: egui_dock::NodePath) {
        for action in crate::widgets::filtered_menu(ui, self.menu) {
            if crate::a11y::named_button(ui, &self.menu[action]).clicked() {
                let id = tab.clone();
                ui.data_mut(|data| data.insert_temp(egui::Id::new("seal-tab-menu"), (id, action)));
                ui.close();
            }
        }
    }

    fn scroll_bars(&self, tab: &Self::Tab) -> [bool; 2] {
        let kind = panel_base(tab);
        // The viewport fills its cell. Scroll bars there steal size and clip the canvas.
        if kind == "viewport" {
            [false, false]
        } else {
            [true, true]
        }
    }
}

pub fn dock_from_layout(layout: &Layout) -> BuiltDock {
    let mut requested = Vec::new();
    let mut state = DockState::new(vec!["welcome".to_string()]);
    {
        let tree = state.main_surface_mut();
        write_node(tree, NodeIndex::root(), &layout.root);
    }
    for window in &layout.floating {
        let surface = state.add_window(vec!["welcome".to_string()]);
        if let Some(tree) = window_tree_mut(&mut state, surface) {
            write_node(tree, NodeIndex::root(), &window.node);
        }
        if let Some(window_state) = state.get_window_state_mut(surface) {
            window_state.set_position(egui::Pos2::new(window.x, window.y));
            window_state.set_size(egui::Vec2::new(window.width, window.height));
        }
        requested.push(RequestedWindow {
            x: window.x,
            y: window.y,
            width: window.width,
            height: window.height,
        });
    }
    BuiltDock { state, requested }
}

pub fn layout_from_dock(state: &DockState<String>) -> Layout {
    let root = read_node(state.main_surface(), NodeIndex::root());
    let mut floating = Vec::new();
    for (index, surface) in state.iter_surfaces_indexed() {
        if index == SurfaceIndex::main() {
            continue;
        }
        let egui_dock::Surface::Window(tree, window) = surface else {
            continue;
        };
        let rect = window.rect();
        let placed = rect.width().is_finite()
            && rect.height().is_finite()
            && rect.width() > 1.0
            && rect.height() > 1.0;
        floating.push(FloatingWindow {
            x: if placed { rect.min.x } else { 80.0 },
            y: if placed { rect.min.y } else { 80.0 },
            width: if placed { rect.width() } else { 480.0 },
            height: if placed { rect.height() } else { 320.0 },
            monitor: None,
            node: read_node(tree, NodeIndex::root()),
        });
    }
    Layout { root, floating }
}

fn write_node(tree: &mut egui_dock::Tree<String>, index: NodeIndex, node: &DockNode) {
    match node {
        DockNode::Tabs { active, panels } => {
            let panels =
                if panels.is_empty() { vec!["welcome".to_string()] } else { panels.clone() };
            tree[index] = Node::leaf_with(panels);
            if let Node::Leaf(leaf) = &mut tree[index] {
                let last = leaf.tabs.len().saturating_sub(1);
                leaf.active = TabIndex((*active).min(last));
            }
        }
        DockNode::Split { axis, ratio, first, second } => {
            tree[index] = Node::leaf("__split__".into());
            let split = match axis {
                Axis::Horizontal => Split::Right,
                Axis::Vertical => Split::Below,
            };
            let [old, new] = tree.split(index, split, *ratio, Node::leaf("__split__".into()));
            write_node(tree, old, first);
            write_node(tree, new, second);
        }
    }
}

fn read_node(tree: &egui_dock::Tree<String>, index: NodeIndex) -> DockNode {
    if index.0 >= tree.len() {
        return DockNode::tabs(["welcome"]);
    }
    match &tree[index] {
        Node::Leaf(leaf) => {
            let panels = leaf.tabs.clone();
            let active = if panels.is_empty() { 0 } else { leaf.active.0.min(panels.len() - 1) };
            DockNode::Tabs { active, panels }
        }
        Node::Horizontal(split) => DockNode::Split {
            axis: Axis::Horizontal,
            ratio: split.fraction.clamp(0.05, 0.95),
            first: Box::new(read_node(tree, index.left())),
            second: Box::new(read_node(tree, index.right())),
        },
        Node::Vertical(split) => DockNode::Split {
            axis: Axis::Vertical,
            ratio: split.fraction.clamp(0.05, 0.95),
            first: Box::new(read_node(tree, index.left())),
            second: Box::new(read_node(tree, index.right())),
        },
        Node::Empty => DockNode::tabs(["welcome"]),
    }
}

fn window_tree_mut(
    state: &mut DockState<String>,
    index: SurfaceIndex,
) -> Option<&mut egui_dock::Tree<String>> {
    let surface = state.get_surface_mut(index)?;
    match surface {
        egui_dock::Surface::Window(tree, _) => Some(tree),
        egui_dock::Surface::Main(tree) => Some(tree),
        egui_dock::Surface::Empty => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seal_editor_core::Layout;

    #[test]
    fn a_badge_is_stored_and_appended_to_the_title() {
        let mut host = DockHost::from_layout(&Layout::default());
        host.set_badge("viewport", "*");
        assert_eq!(host.tab_badges().get("viewport").map(String::as_str), Some("*"));
        assert_eq!(tab_title("Viewport", Some("*")), "Viewport *");
        host.set_badge("viewport", "");
        assert!(!host.tab_badges().contains_key("viewport"));
        assert_eq!(tab_title("Viewport", None), "Viewport");
        assert_eq!(tab_title("Viewport", Some("")), "Viewport");
    }

    #[test]
    fn duplicate_tab_uses_a_stable_hash_suffix() {
        let mut host = DockHost::from_layout(&Layout::default());
        assert!(host.has_panel("outliner"));
        let second = host.duplicate_tab("outliner").expect("outliner");
        assert_eq!(second, "outliner#2");
        let third = host.duplicate_tab("outliner").expect("outliner");
        assert_eq!(third, "outliner#3");
    }

    #[test]
    fn locked_layout_still_renders_tabs() {
        let menu = [
            "Close".into(),
            "Close Others".into(),
            "Close Right".into(),
            "Float".into(),
            "Sidebar".into(),
            "Maximize".into(),
            "Lock".into(),
        ];
        let mut host = DockHost::from_layout(&Layout::default());
        crate::run_frame(crate::default_input(Vec::new()), |ui| {
            show_dock(ui, &mut host.state, &|id| id.to_string(), &menu, true, &mut |ui, id| {
                ui.label(id);
            });
        });
        assert!(host.has_panel("outliner"));
    }
}
