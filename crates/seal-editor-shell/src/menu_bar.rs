//! The menu bar: nine menus, icon rows, and the window buttons.

use seal_editor_core::EditorSession;
use seal_ui::{paint_icon, Icon};

use crate::menu_ui;

#[derive(Clone)]
pub enum MenuClick {
    Command { id: String, args: Vec<(String, String)> },
    ShowPanel(String),
    Duplicate(String),
}

#[derive(Clone)]
struct Row {
    label: String,
    icon: Icon,
    shortcut: String,
    enabled: bool,
    reason: String,
    checked: bool,
    click: Option<MenuClick>,
    children: Vec<Entry>,
}

#[derive(Clone)]
enum Entry {
    Section(String),
    Row(Row),
}

struct Draft<'a> {
    session: &'a EditorSession,
    panels: &'a [String],
}

impl<'a> Draft<'a> {
    fn text(&self, key: &str) -> String {
        self.session.messages.get(key)
    }

    fn has_panel(&self, id: &str) -> bool {
        self.panels.iter().any(|panel| panel == id)
    }

    fn selected(&self) -> bool {
        self.session.models.selection.is_some()
    }

    fn shortcut(&self, id: &str) -> String {
        self.session.keymap.shortcut(id)
    }

    fn section(&self, key: &str) -> Entry {
        Entry::Section(self.text(key))
    }

    fn off(&self, label: &str, icon: Icon) -> Entry {
        self.off_reason(label, icon, "menu-unavailable")
    }

    /// Whether the primary selection has at least one child node.
    fn selection_has_children(&self) -> bool {
        let Some(parent) = self.session.models.selection.as_deref() else {
            return false;
        };
        self.session.models.nodes.iter().any(|node| node.parent.as_deref() == Some(parent))
    }

    fn off_reason(&self, label: &str, icon: Icon, reason: &str) -> Entry {
        Entry::Row(Row {
            label: self.text(label),
            icon,
            shortcut: String::new(),
            enabled: false,
            reason: self.text(reason),
            checked: false,
            click: None,
            children: Vec::new(),
        })
    }

    fn command(&self, label: &str, icon: Icon, id: &str, needs_selection: bool) -> Entry {
        let exists = self.session.commands.get(id).is_some();
        let live = exists && self.session.commands.is_enabled(id, self.session);
        let enabled = live && (!needs_selection || self.selected());
        let reason = if !live {
            self.text("menu-unavailable")
        } else if needs_selection && !self.selected() {
            self.text("menu-needs-selection")
        } else {
            String::new()
        };
        Entry::Row(Row {
            label: self.text(label),
            icon,
            shortcut: self.shortcut(id),
            enabled,
            reason,
            checked: false,
            click: Some(MenuClick::Command { id: id.to_string(), args: Vec::new() }),
            children: Vec::new(),
        })
    }

    fn command_args(
        &self,
        label: String,
        icon: Icon,
        id: &str,
        args: Vec<(String, String)>,
    ) -> Entry {
        let enabled = self.session.commands.get(id).is_some();
        Entry::Row(Row {
            label,
            icon,
            shortcut: String::new(),
            enabled,
            reason: if enabled { String::new() } else { self.text("menu-unavailable") },
            checked: false,
            click: Some(MenuClick::Command { id: id.to_string(), args }),
            children: Vec::new(),
        })
    }

    fn branch(&self, label: &str, icon: Icon, children: Vec<Entry>) -> Entry {
        Entry::Row(Row {
            label: self.text(label),
            icon,
            shortcut: String::new(),
            enabled: true,
            reason: String::new(),
            checked: false,
            click: None,
            children,
        })
    }

    fn recent_rows(&self) -> Vec<Entry> {
        let recent = self
            .session
            .recent_path
            .as_ref()
            .and_then(|path| seal_editor_core::read_recent(path).ok())
            .unwrap_or_default();
        if recent.is_empty() {
            return vec![self.off("menu-no-recent", Icon::FileRecent)];
        }
        recent
            .into_iter()
            .map(|path| {
                self.command_args(
                    path.clone(),
                    Icon::FolderOpen,
                    "file.open-project",
                    vec![("path".into(), path)],
                )
            })
            .collect()
    }

    fn viewport_layout_menu(&self) -> Entry {
        let current = self.session.models.viewport_layout_preset.as_str();
        let rows = [
            ("layout_single", "single", "viewport-menu-layout-single"),
            ("layout_two_cols", "two_cols", "viewport-menu-layout-two-cols"),
            ("layout_two_rows", "two_rows", "viewport-menu-layout-two-rows"),
            ("layout_three", "three", "viewport-menu-layout-three"),
            ("layout_four", "four", "viewport-menu-layout-four"),
        ];
        let children = rows
            .into_iter()
            .map(|(id, preset, key)| {
                Entry::Row(Row {
                    label: self.text(key),
                    icon: Icon::ViewTiles,
                    shortcut: String::new(),
                    enabled: true,
                    reason: String::new(),
                    checked: current == preset,
                    click: Some(MenuClick::Command {
                        id: "viewport.layout".into(),
                        args: vec![("preset".into(), id.into())],
                    }),
                    children: Vec::new(),
                })
            })
            .collect();
        self.branch("menu-viewport-layout", Icon::ViewTiles, children)
    }

    fn layouts(&self, label: &str, command: &str) -> Entry {
        let names = [
            ("default", "menu-layout-default"),
            ("debug", "menu-layout-debug"),
            ("minimal", "menu-layout-minimal"),
        ];
        let children = names
            .into_iter()
            .map(|(id, key)| {
                self.command_args(
                    self.text(key),
                    Icon::ViewTiles,
                    command,
                    vec![("name".into(), id.to_string())],
                )
            })
            .collect();
        self.branch(label, Icon::ViewTiles, children)
    }

    fn slots(&self, label: &str, base: &str) -> Entry {
        let title = self.text(label);
        let mut children = Vec::new();
        for slot in 1..=4 {
            let id = if slot == 1 { base.to_string() } else { format!("{base}#{slot}") };
            let open = self.has_panel(&id);
            let previous = if slot <= 2 {
                self.has_panel(base)
            } else {
                self.has_panel(&format!("{base}#{}", slot - 1))
            };
            let multiple = seal_editor_core::allows_multiple_instances(base);
            let (enabled, click, reason) = if open || slot == 1 {
                (
                    true,
                    Some(MenuClick::ShowPanel(if slot == 1 { base.to_string() } else { id })),
                    String::new(),
                )
            } else if multiple && previous {
                (true, Some(MenuClick::Duplicate(base.to_string())), String::new())
            } else {
                (false, None, self.text("menu-one-panel"))
            };
            let name = self.session.messages.format("menu-slot-n", &[("n", &slot.to_string())]);
            children.push(Entry::Row(Row {
                label: name,
                icon: Icon::ViewColumns,
                shortcut: String::new(),
                enabled,
                reason,
                checked: open,
                click,
                children: Vec::new(),
            }));
        }
        self.branch_owned(title, Icon::ViewColumns, children)
    }

    fn branch_owned(&self, label: String, icon: Icon, children: Vec<Entry>) -> Entry {
        Entry::Row(Row {
            label,
            icon,
            shortcut: String::new(),
            enabled: true,
            reason: String::new(),
            checked: false,
            click: None,
            children,
        })
    }

    fn panel(&self, label: &str, icon: Icon, id: &str) -> Entry {
        Entry::Row(Row {
            label: self.text(label),
            icon,
            shortcut: String::new(),
            enabled: true,
            reason: String::new(),
            checked: self.has_panel(id),
            click: Some(MenuClick::ShowPanel(id.to_string())),
            children: Vec::new(),
        })
    }

    fn named_undo(&self, redo: bool) -> Entry {
        let (can, action) = if redo {
            (self.session.can_redo(), self.session.redo_action_name())
        } else {
            (self.session.can_undo(), self.session.undo_action_name())
        };
        let label = match action {
            Some(name) if can => {
                let key = if redo { "menu-redo-named" } else { "menu-undo-named" };
                self.session.messages.format(key, &[("name", name.as_str())])
            }
            _ => self.text(if redo { "command-edit-redo" } else { "command-edit-undo" }),
        };
        let id = if redo { "edit.redo" } else { "edit.undo" };
        Entry::Row(Row {
            label,
            icon: if redo { Icon::EditRedo } else { Icon::EditUndo },
            shortcut: self.shortcut(id),
            enabled: can,
            reason: if can {
                String::new()
            } else {
                self.text(if redo { "notice-nothing-to-redo" } else { "notice-nothing-to-undo" })
            },
            checked: false,
            click: Some(MenuClick::Command { id: id.to_string(), args: Vec::new() }),
            children: Vec::new(),
        })
    }

    fn menus(&self) -> Vec<(&'static str, Vec<Entry>)> {
        let recent = self.recent_rows();
        vec![
            (
                "file",
                vec![
                    self.off("menu-new-scene", Icon::FileNew),
                    self.off("menu-open-scene", Icon::FileOpen),
                    self.branch("menu-open-recent", Icon::FileRecent, recent.clone()),
                    self.command("menu-save", Icon::FileSave, "file.save", false),
                    self.command("menu-save-as", Icon::FileSave, "file.save_as", false),
                    self.command("menu-save-all", Icon::FileSaveAll, "file.save_all", false),
                    self.off("menu-choose-save", Icon::File),
                    self.command("menu-import-scene", Icon::FileImport, "asset.import", false),
                    self.off("menu-export-all", Icon::FileExport),
                    self.off("menu-export-selected", Icon::FileExport),
                    self.command(
                        "command-file-new-project",
                        Icon::Folder,
                        "file.new-project",
                        false,
                    ),
                    self.command(
                        "command-file-open-project",
                        Icon::FolderOpen,
                        "file.open-project",
                        false,
                    ),
                    self.branch("menu-recent-projects", Icon::FileRecent, recent),
                    self.command(
                        "command-file-close-project",
                        Icon::ViewClose,
                        "file.close-project",
                        false,
                    ),
                    self.command(
                        "menu-save-template",
                        Icon::FileSave,
                        "project.save_template",
                        false,
                    ),
                    self.command(
                        "menu-build-scenes",
                        Icon::EditSettings,
                        "project.open_build",
                        false,
                    ),
                    self.command(
                        "command-project-export",
                        Icon::FileExport,
                        "project.export",
                        false,
                    ),
                    self.command("menu-exit", Icon::ViewClose, "file.quit", false),
                ],
            ),
            (
                "edit",
                vec![
                    self.section("menu-sec-history"),
                    self.named_undo(false),
                    self.named_undo(true),
                    self.panel("menu-undo-history", Icon::FileRecent, "undo-history"),
                    self.section("menu-sec-edit"),
                    self.command("menu-cut", Icon::EditCut, "edit.cut", true),
                    self.command("menu-copy", Icon::EditCopy, "edit.copy", true),
                    self.command("menu-paste", Icon::EditPaste, "edit.paste", false),
                    self.command("menu-duplicate", Icon::EditDuplicate, "edit.duplicate", true),
                    self.command("menu-delete", Icon::EditDelete, "edit.delete", true),
                    self.command("menu-rename", Icon::EditRename, "edit.rename", true),
                    self.branch(
                        "menu-shape",
                        Icon::EntityPlane,
                        vec![
                            self.command(
                                "menu-shape-merge",
                                Icon::EntityPlane,
                                "shape.merge",
                                true,
                            ),
                            self.command(
                                "menu-shape-subtract",
                                Icon::EntityPlane,
                                "shape.subtract",
                                true,
                            ),
                            self.command(
                                "menu-shape-intersect",
                                Icon::EntityPlane,
                                "shape.intersect",
                                true,
                            ),
                            self.command(
                                "menu-shape-offset",
                                Icon::EntityPlane,
                                "shape.offset",
                                true,
                            ),
                        ],
                    ),
                    self.section("menu-sec-config"),
                    self.command("menu-preferences", Icon::EditSettings, "settings.open", false),
                    self.command(
                        "menu-project-settings",
                        Icon::EditSettings,
                        "settings.open",
                        false,
                    ),
                    self.command(
                        "menu-project-import",
                        Icon::FileImport,
                        "project.open_import",
                        false,
                    ),
                    self.command("menu-input-bindings", Icon::EditSettings, "project.input", false),
                    self.command("menu-save-games", Icon::FileSave, "project.saves", false),
                    self.off("menu-plugins", Icon::StatusPlug),
                    self.command("menu-shortcuts", Icon::EditSettings, "keymap.open", false),
                ],
            ),
            (
                "view",
                vec![
                    self.viewport_layout_menu(),
                    self.command(
                        "viewport-pane-maximize",
                        Icon::ViewMaximize,
                        "viewport.pane_expand",
                        false,
                    ),
                ],
            ),
            (
                "window",
                vec![
                    self.section("menu-sec-scene"),
                    self.slots("menu-viewports", "viewport"),
                    self.slots("panel-outliner-title", "outliner"),
                    self.slots("menu-details", "inspector"),
                    self.panel("menu-place", Icon::EntityCube, "place"),
                    self.panel("panel-world-title", Icon::ToolWorld, "world"),
                    self.off("menu-layers", Icon::ViewList),
                    self.panel("panel-stats-title", Icon::StatusInfo, "stats"),
                    self.panel("panel-test-runner-title", Icon::StatusInfo, "test-runner"),
                    self.panel(
                        "panel-physics2d-profiler-title",
                        Icon::StatusInfo,
                        "physics2d-profiler",
                    ),
                    self.panel("panel-physics-title", Icon::StatusInfo, "physics"),
                    self.section("menu-sec-general"),
                    self.slots("panel-content-title", "content"),
                    self.panel("panel-import-activity-title", Icon::StatusInfo, "import-activity"),
                    self.panel(
                        "panel-naming-violations-title",
                        Icon::StatusInfo,
                        "naming-violations",
                    ),
                    self.off("menu-asset-drawer", Icon::ViewList),
                    self.panel("menu-console-log", Icon::File, "console"),
                    self.panel("menu-message-log", Icon::StatusBell, "notices"),
                    self.panel("menu-undo-history", Icon::FileRecent, "undo-history"),
                    self.panel("panel-commands-title", Icon::EditSearch, "commands"),
                    self.panel("panel-assistant-title", Icon::StatusInfo, "assistant"),
                    self.panel("panel-timeline-title", Icon::ViewList, "timeline"),
                    self.panel("panel-asset-title", Icon::AssetTexture, "asset"),
                    self.branch(
                        "menu-devtools",
                        Icon::EditSettings,
                        vec![
                            self.panel("menu-dev", Icon::EditSettings, "dev"),
                            self.command(
                                "menu-copy-diagnostics",
                                Icon::StatusInfo,
                                "app.copy_diagnostics",
                                false,
                            ),
                        ],
                    ),
                    self.section("menu-sec-layout"),
                    self.layouts("menu-load-layout", "window.apply-layout"),
                    self.layouts("menu-save-layout", "layout.save"),
                    self.layouts("menu-remove-layout", "layout.delete"),
                    self.command(
                        "menu-reset-layout",
                        Icon::ViewReset,
                        "window.reset-layout",
                        false,
                    ),
                    self.command(
                        "menu-fullscreen",
                        Icon::ViewMaximize,
                        "app.toggle_fullscreen",
                        false,
                    ),
                ],
            ),
            (
                "tools",
                vec![
                    self.off("menu-find-files", Icon::EditSearch),
                    self.off("menu-audit", Icon::StatusInfo),
                    self.off("menu-frontend", Icon::StatusBusy),
                    self.branch(
                        "menu-debug",
                        Icon::StatusWarning,
                        vec![self.command(
                            "menu-copy-diagnostics",
                            Icon::StatusInfo,
                            "app.copy_diagnostics",
                            false,
                        )],
                    ),
                    self.off("menu-macros", Icon::EntityScript),
                    self.off("menu-run-script", Icon::Play),
                    self.command("menu-refresh", Icon::ViewRefresh, "content.refresh", false),
                ],
            ),
            (
                "build",
                vec![
                    self.off_reason("menu-build-all", Icon::ToolTransform, "menu-build-reason"),
                    self.off_reason(
                        "menu-build-lighting",
                        Icon::EntityLightPoint,
                        "menu-build-reason",
                    ),
                    self.off_reason(
                        "menu-build-navigation",
                        Icon::EntitySpline,
                        "menu-build-reason",
                    ),
                    self.off_reason("menu-build-collision", Icon::EntityCube, "menu-build-reason"),
                    self.command(
                        "command-project-export",
                        Icon::FileExport,
                        "project.export",
                        false,
                    ),
                    self.command(
                        "command-project-export-go",
                        Icon::FileExport,
                        "project.export_go",
                        false,
                    ),
                    self.command(
                        "command-project-export-run",
                        Icon::Play,
                        "project.export_run",
                        false,
                    ),
                    self.off_reason("menu-build-log", Icon::File, "menu-build-reason"),
                ],
            ),
            (
                "select",
                vec![
                    self.command("menu-select-all", Icon::ToolSelect, "edit.select_all", false),
                    self.command("menu-deselect", Icon::ViewClose, "edit.select_none", false),
                    self.command("menu-invert", Icon::ViewRefresh, "edit.invert_selection", false),
                    if self.selected() && !self.selection_has_children() {
                        self.off_reason(
                            "menu-select-children",
                            Icon::ViewChevronDown,
                            "menu-no-children",
                        )
                    } else {
                        self.command(
                            "menu-select-children",
                            Icon::ViewChevronDown,
                            "edit.select-children",
                            true,
                        )
                    },
                    self.off("menu-select-descendants", Icon::ViewExpandAll),
                    self.off("menu-select-parent", Icon::ViewChevronUp),
                    self.branch(
                        "menu-by-type",
                        Icon::EntityCube,
                        vec![self.off("menu-by-type", Icon::EntityCube)],
                    ),
                    self.off("menu-by-material", Icon::AssetMaterial),
                    self.branch(
                        "menu-by-layer",
                        Icon::ViewList,
                        vec![self.off("menu-layers", Icon::ViewList)],
                    ),
                    self.command("menu-hide-selected", Icon::ViewEyeOff, "scene.hide", true),
                    self.off("menu-show-all", Icon::ViewEye),
                    self.command("menu-lock-selected", Icon::EditLock, "scene.lock", true),
                    self.off("menu-unlock-all", Icon::EditUnlock),
                    self.off("menu-focus-selected", Icon::ToolCamera),
                ],
            ),
            (
                "entity",
                vec![
                    self.branch(
                        "menu-entity-edit",
                        Icon::EditRename,
                        vec![
                            self.command("menu-rename", Icon::EditRename, "edit.rename", true),
                            self.command(
                                "menu-duplicate",
                                Icon::EditDuplicate,
                                "edit.duplicate",
                                true,
                            ),
                            self.command("menu-delete", Icon::EditDelete, "edit.delete", true),
                        ],
                    ),
                    self.branch(
                        "menu-visibility",
                        Icon::ViewEye,
                        vec![self.command(
                            "menu-hide-selected",
                            Icon::ViewEyeOff,
                            "scene.hide",
                            true,
                        )],
                    ),
                    self.branch(
                        "menu-transform",
                        Icon::ToolTransform,
                        vec![
                            self.off("menu-reset-location", Icon::ToolMove),
                            self.off("menu-reset-rotation", Icon::ToolRotate),
                            self.off("menu-reset-scale", Icon::ToolScale),
                            self.off("menu-mirror-x", Icon::ToolMove),
                            self.off("menu-mirror-y", Icon::ToolMove),
                            self.off("menu-mirror-z", Icon::ToolMove),
                            self.command(
                                "menu-align-floor",
                                Icon::ViewChevronDown,
                                "scene.floor",
                                true,
                            ),
                        ],
                    ),
                    self.branch(
                        "menu-snapping",
                        Icon::ToolGridSnap,
                        vec![
                            self.off("menu-snap-grid", Icon::ToolGridSnap),
                            self.off("menu-snap-rotation", Icon::ToolAngleSnap),
                            self.off("menu-snap-scale", Icon::ToolScaleSnap),
                            self.off("menu-snap-surface", Icon::ToolSnapSurface),
                        ],
                    ),
                    self.branch(
                        "menu-pivot",
                        Icon::ToolMove,
                        vec![
                            self.off("menu-pivot-center", Icon::ToolMove),
                            self.off("menu-pivot-object", Icon::EntityCube),
                            self.off("menu-pivot-world", Icon::ToolWorld),
                        ],
                    ),
                    self.command("menu-group", Icon::EntityGroup, "scene.group", true),
                    self.command(
                        "menu-merge-objects",
                        Icon::EntityGroup,
                        "scene.merge_objects",
                        true,
                    ),
                    self.command("menu-ungroup", Icon::EditUnlink, "scene.ungroup", true),
                    self.command(
                        "menu-create-prefab",
                        Icon::EntityGroup,
                        "scene.make_prefab",
                        true,
                    ),
                    self.branch(
                        "menu-attach",
                        Icon::EditLink,
                        vec![self.off("menu-attach-none", Icon::EditLink)],
                    ),
                    self.off("menu-detach", Icon::EditUnlink),
                    self.branch(
                        "menu-convert",
                        Icon::ViewRefresh,
                        vec![
                            self.off("menu-convert-empty", Icon::EntityEmpty),
                            self.off("menu-convert-cube", Icon::EntityCube),
                            self.off("menu-convert-sphere", Icon::EntitySphere),
                            self.off("menu-convert-plane", Icon::EntityPlane),
                        ],
                    ),
                    self.branch(
                        "menu-replace",
                        Icon::EntityCube,
                        vec![self.off("menu-replace-browse", Icon::FileOpen)],
                    ),
                    self.branch(
                        "menu-move-folder",
                        Icon::EntityFolder,
                        vec![self.off("menu-folder-none", Icon::EntityFolder)],
                    ),
                    self.off("menu-pin", Icon::EditPin),
                    self.off("menu-snap-view", Icon::ToolCamera),
                    self.off("menu-snap-object", Icon::ToolCamera),
                    self.off("menu-pilot", Icon::Play),
                ],
            ),
            (
                "help",
                vec![
                    self.command("menu-docs", Icon::StatusInfo, "help.open_manual", false),
                    self.command("menu-recipes", Icon::StatusInfo, "help.open_recipes", false),
                    self.command("menu-tutorial", Icon::StatusInfo, "tutorial.open", false),
                    self.command("menu-shortcut-sheet", Icon::EditSettings, "keymap.open", false),
                    self.off("menu-report", Icon::StatusWarning),
                    self.command(
                        "menu-log-folder",
                        Icon::FolderOpen,
                        "app.open_user_data_folder",
                        false,
                    ),
                    self.off("menu-updates", Icon::ViewRefresh),
                    self.command("command-help-about", Icon::StatusInfo, "help.about", false),
                ],
            ),
        ]
    }
}

/// Drawn when the menu bar is up. Returns the row the pointer chose.
pub fn show(
    ui: &mut egui::Ui,
    session: &EditorSession,
    panels: &[String],
    open: &mut Option<String>,
) -> Option<MenuClick> {
    let draft = Draft { session, panels };
    let menus = draft.menus();
    let mut chosen = None;
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            egui::Id::new("seal-menu-search-hint"),
            session.messages.get("menu-search"),
        );
        data.insert_temp(egui::Id::new("seal-menu-miss"), session.messages.get("menu-no-matches"));
    });
    let visuals = ui.visuals().clone();
    let bar =
        egui::Frame::new().fill(visuals.panel_fill).inner_margin(egui::Margin::symmetric(6, 2));
    egui::Panel::top("menu").frame(bar).show_separator_line(false).show(ui, |ui| {
        ui.set_min_height(seal_ui::MENU_PX);
        ui.spacing_mut().item_spacing.x = 2.0;
        let width = ui.available_width();
        ui.horizontal(|ui| {
            ui.set_width(width);
            for (key, entries) in &menus {
                let label = session.messages.get(&format!("menu-{key}"));
                let button = menu_ui::mnemonic_button(ui, &label);
                let slot = ui.painter().add(egui::Shape::Noop);
                let response = seal_ui::crisp::add_frameless(ui, button);
                let force = open.as_deref() == Some(*key);
                let is_open = force
                    || egui::Popup::is_id_open(
                        ui.ctx(),
                        egui::Popup::default_response_id(&response),
                    );
                seal_ui::crisp::paint_behind_button(ui, slot, &response, false, is_open);
                let mut popup = egui::Popup::menu(&response)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
                if force {
                    popup = popup.open_memory(Some(egui::SetOpenCommand::Bool(true)));
                }
                popup.show(|ui| {
                    ui.set_min_width(240.0);
                    show_entries(ui, entries, &mut chosen);
                });
            }
            if open.is_some() {
                *open = None;
            }
            let rest = ui.available_width().max(0.0);
            let height = seal_ui::MENU_PX;
            ui.allocate_ui_with_layout(
                egui::vec2(rest, height),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| chrome(ui, session, &mut chosen),
            );
        });
    });
    if chosen.is_none() {
        if let Some(id) =
            ui.ctx().data(|data| data.get_temp::<String>(egui::Id::new("seal-help-pick")))
        {
            ui.ctx().data_mut(|data| data.remove::<String>(egui::Id::new("seal-help-pick")));
            chosen = Some(MenuClick::Command { id, args: Vec::new() });
        }
    }
    chosen
}

fn chrome(ui: &mut egui::Ui, session: &EditorSession, _chosen: &mut Option<MenuClick>) {
    let ctx = ui.ctx().clone();
    let tip = |key: &str| session.messages.get(key);
    if icon_hit(ui, Icon::ViewClose, &tip("menu-close-window")) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
    let max_icon = if maximized { Icon::ViewRestore } else { Icon::ViewMaximize };
    let max_tip = if maximized { tip("menu-restore") } else { tip("menu-maximize") };
    if icon_hit(ui, max_icon, &max_tip) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
    if icon_hit(ui, Icon::ViewMinus, &tip("menu-minimize")) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
    }
    let config = if cfg!(debug_assertions) {
        session.messages.get("menu-config-debug")
    } else {
        session.messages.get("menu-config-release")
    };
    ui.add_space(10.0);
    ui.label(egui::RichText::new(config).small().weak()).on_hover_text(tip("menu-config-tip"));
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(6.0);
    let project = session.project.as_ref().map(|project| project.name.clone());
    match project {
        Some(name) => {
            ui.label(egui::RichText::new(name).strong());
        }
        None => {
            ui.label(egui::RichText::new(session.messages.get("menu-no-project")).weak());
        }
    }
    ui.add_space(12.0);
    help_search(ui, session);
    let drag = ui.allocate_response(
        egui::vec2(ui.available_width().max(1.0), seal_ui::MENU_PX),
        egui::Sense::click_and_drag(),
    );
    if drag.double_clicked() {
        // Double-clicking the empty title bar maximizes or restores, as on any desktop.
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    } else if drag.drag_started() {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    // The document tabs draw into this free stretch so they need no row of their own.
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("seal-doc-slot"), drag.rect));
}

fn icon_hit(ui: &mut egui::Ui, icon: Icon, tip: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::click());
    let response = response.on_hover_text(tip);
    seal_ui::crisp::plate(ui, rect, &response, false, false);
    paint_icon(ui.painter(), rect.center(), icon, seal_ui::ICON_PX, ui.visuals().text_color());
    response.clicked()
}

fn help_search(ui: &mut egui::Ui, session: &EditorSession) {
    let id = egui::Id::new("seal-help-search");
    let mut query = ui.ctx().data(|data| data.get_temp::<String>(id).unwrap_or_default());
    let response = seal_ui::crisp::well_edit(
        ui,
        egui::TextEdit::singleline(&mut query)
            .hint_text(session.messages.get("menu-help-search"))
            .desired_width(168.0),
    );
    let popup_id = egui::Popup::default_response_id(&response);
    let escape = ui.input(|input| input.key_pressed(egui::Key::Escape));
    if escape && (response.has_focus() || egui::Popup::is_id_open(ui.ctx(), popup_id)) {
        // Escape dismisses the results and leaves the field.
        egui::Popup::close_id(ui.ctx(), popup_id);
        response.surrender_focus();
        query.clear();
    }
    if response.lost_focus() && !egui::Popup::is_id_open(ui.ctx(), popup_id) {
        // Clicking away dismisses the search; a stale query should not linger in the bar.
        query.clear();
    }
    ui.ctx().data_mut(|data| data.insert_temp(id, query.clone()));
    if query.trim().is_empty() {
        egui::Popup::close_id(ui.ctx(), popup_id);
        return;
    }
    // Open on typing or focusing; after that the popup's own memory decides, so a click
    // outside or Escape really closes it.
    let reopen = (response.changed() || response.gained_focus() || response.clicked()) && !escape;
    let mut picked = false;
    egui::Popup::menu(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .open_memory(reopen.then_some(egui::SetOpenCommand::Bool(true)))
        .show(|ui| {
            ui.set_min_width(240.0);
            let hits = help_hits(session, query.trim());
            if hits.is_empty() {
                ui.label(egui::RichText::new(session.messages.get("menu-no-matches")).weak());
                return;
            }
            for (label, command) in hits {
                if seal_ui::crisp::menu_row(ui, None, &label).clicked() {
                    // The bar handles commands after the popup. Stash the id on the context.
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(egui::Id::new("seal-help-pick"), command)
                    });
                    picked = true;
                    ui.close();
                }
            }
        });
    let closed_away = !response.has_focus() && !egui::Popup::is_id_open(ui.ctx(), popup_id);
    if picked || closed_away {
        ui.ctx().data_mut(|data| data.insert_temp(id, String::new()));
    }
}

fn help_hits(session: &EditorSession, query: &str) -> Vec<(String, String)> {
    let needle = query.to_lowercase();
    let mut hits = Vec::new();
    for spec in session.commands.iter() {
        if spec.alias_of.is_some() {
            continue;
        }
        let title = session.messages.get(&spec.title_key);
        if title.to_lowercase().contains(&needle) {
            hits.push((title, spec.id.clone()));
        }
        if hits.len() == 8 {
            break;
        }
    }
    for meta in seal_editor_core::setting_catalog() {
        let title = setting_title(session, meta.id);
        if seal_editor_core::setting_matches_query(meta.id, meta.category, &title, "", &needle) {
            hits.push((title, "settings.open".into()));
        }
        if hits.len() == 14 {
            break;
        }
    }
    hits
}

fn setting_title(session: &EditorSession, id: &str) -> String {
    match id {
        "ui.theme" => session.messages.get("setting-theme"),
        "ui.scale" => session.messages.get("setting-scale"),
        other => session.messages.get(&format!("setting-{}", other.replace(['.', '_'], "-"))),
    }
}

fn show_entries(ui: &mut egui::Ui, entries: &[Entry], chosen: &mut Option<MenuClick>) {
    let query = search_query(ui, entries);
    let shown = filter_entries(entries, &query);
    if shown.is_empty() {
        let miss = menu_miss(ui);
        ui.label(egui::RichText::new(miss).weak());
        return;
    }
    let width = rows_width(ui, &shown);
    for entry in &shown {
        match entry {
            Entry::Section(title) => section(ui, title),
            Entry::Row(row) => show_row(ui, row, width, chosen),
        }
    }
}

/// Natural width of a menu: the widest label plus icon, shortcut and padding, so a
/// short submenu stays compact instead of stretching to the parent's width.
fn rows_width(ui: &egui::Ui, entries: &[Entry]) -> f32 {
    let body = egui::TextStyle::Body.resolve(ui.style());
    let small = egui::TextStyle::Small.resolve(ui.style());
    let measure = |text: &str, font: &egui::FontId| {
        ui.painter().layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::WHITE).size().x
    };
    let mut width: f32 = 180.0;
    for entry in entries {
        match entry {
            Entry::Section(title) => {
                width = width.max(measure(&title.to_uppercase(), &small) + 40.0)
            }
            Entry::Row(row) => {
                let trailing = if row.children.is_empty() { row.shortcut.as_str() } else { "▶" };
                let extra =
                    if trailing.is_empty() { 0.0 } else { measure(trailing, &small) + 32.0 };
                width = width.max(44.0 + measure(&row.label, &body) + extra + 16.0);
            }
        }
    }
    width.ceil()
}

fn menu_miss(ui: &mut egui::Ui) -> String {
    ui.ctx()
        .data(|data| data.get_temp::<String>(egui::Id::new("seal-menu-miss")))
        .unwrap_or_else(|| "menu-no-matches".to_string())
}

fn search_query(ui: &mut egui::Ui, entries: &[Entry]) -> String {
    if count_rows(entries) < 8 {
        return String::new();
    }
    let id = ui.id().with("search");
    let mut query = ui.ctx().data(|data| data.get_temp::<String>(id).unwrap_or_default());
    let hint = ui
        .ctx()
        .data(|data| data.get_temp::<String>(egui::Id::new("seal-menu-search-hint")))
        .unwrap_or_default();
    ui.add_space(2.0);
    seal_ui::crisp::well_edit(
        ui,
        egui::TextEdit::singleline(&mut query).hint_text(hint).desired_width(f32::INFINITY),
    );
    ui.add_space(4.0);
    ui.ctx().data_mut(|data| data.insert_temp(id, query.clone()));
    query
}

fn count_rows(entries: &[Entry]) -> usize {
    entries
        .iter()
        .map(|entry| match entry {
            Entry::Section(_) => 0,
            Entry::Row(row) => 1 + count_rows(&row.children),
        })
        .sum()
}

fn filter_entries(entries: &[Entry], query: &str) -> Vec<Entry> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return entries.to_vec();
    }
    let mut out = Vec::new();
    let mut pending: Option<String> = None;
    for entry in entries {
        match entry {
            Entry::Section(title) => pending = Some(title.clone()),
            Entry::Row(row) => {
                let children = filter_entries(&row.children, query);
                let hit = row.label.to_lowercase().contains(&needle) || !children.is_empty();
                if !hit {
                    continue;
                }
                if let Some(title) = pending.take() {
                    out.push(Entry::Section(title));
                }
                let mut copy = row.clone();
                if !children.is_empty() {
                    copy.children = children;
                }
                out.push(Entry::Row(copy));
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

fn show_row(ui: &mut egui::Ui, row: &Row, width: f32, chosen: &mut Option<MenuClick>) {
    let height = seal_ui::MENU_ROW_PX;
    let width = width.max(ui.min_rect().width());
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    if row.enabled && response.hovered() {
        ui.painter().rect_filled(rect, 0.0, ui.visuals().widgets.hovered.bg_fill);
        let bar = egui::Rect::from_min_size(rect.left_top(), egui::vec2(2.0, rect.height()));
        ui.painter().rect_filled(bar, 0.0, ui.visuals().selection.stroke.color);
    }
    let ink = if row.enabled {
        ui.visuals().text_color()
    } else {
        ui.visuals().text_color().gamma_multiply(0.4)
    };
    let painter = ui.painter();
    if row.checked {
        let center = egui::pos2(rect.left() + 10.0, rect.center().y);
        let stroke = egui::Stroke::new(1.6, ink);
        painter
            .line_segment([center + egui::vec2(-4.0, 0.0), center + egui::vec2(-1.0, 3.0)], stroke);
        painter
            .line_segment([center + egui::vec2(-1.0, 3.0), center + egui::vec2(4.5, -4.0)], stroke);
    }
    paint_icon(painter, egui::pos2(rect.left() + 28.0, rect.center().y), row.icon, 16.0, ink);
    painter.text(
        egui::pos2(rect.left() + 44.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        &row.label,
        egui::TextStyle::Body.resolve(ui.style()),
        ink,
    );
    let trailing = if row.children.is_empty() { row.shortcut.as_str() } else { "▶" };
    if !trailing.is_empty() {
        painter.text(
            egui::pos2(rect.right() - 12.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            trailing,
            egui::TextStyle::Small.resolve(ui.style()),
            ui.visuals().weak_text_color(),
        );
    }
    if !row.enabled && !row.reason.is_empty() {
        response.clone().on_hover_text(&row.reason);
    }
    if !row.children.is_empty() {
        egui::containers::menu::SubMenu::new().show(ui, &response, |ui| {
            show_entries(ui, &row.children, chosen);
        });
        return;
    }
    if row.enabled && response.clicked() {
        if let Some(click) = row.click.clone() {
            *chosen = Some(click);
            ui.close();
        }
    }
}

/// Edges of an undecorated window. Maximized and fullscreen windows keep the system frame.
pub fn resize_borders(ctx: &egui::Context) {
    let (maximized, fullscreen) = ctx.input(|input| {
        (input.viewport().maximized.unwrap_or(false), input.viewport().fullscreen.unwrap_or(false))
    });
    if maximized || fullscreen {
        return;
    }
    let rect = ctx.content_rect();
    if rect.width() < 40.0 || rect.height() < 40.0 {
        return;
    }
    let t = 5.0;
    let corners = [
        (
            "nw",
            egui::Rect::from_min_size(rect.min, egui::vec2(t, t)),
            egui::ResizeDirection::NorthWest,
            egui::CursorIcon::ResizeNwSe,
        ),
        (
            "ne",
            egui::Rect::from_min_size(egui::pos2(rect.right() - t, rect.top()), egui::vec2(t, t)),
            egui::ResizeDirection::NorthEast,
            egui::CursorIcon::ResizeNeSw,
        ),
        (
            "sw",
            egui::Rect::from_min_size(egui::pos2(rect.left(), rect.bottom() - t), egui::vec2(t, t)),
            egui::ResizeDirection::SouthWest,
            egui::CursorIcon::ResizeNeSw,
        ),
        (
            "se",
            egui::Rect::from_min_size(
                egui::pos2(rect.right() - t, rect.bottom() - t),
                egui::vec2(t, t),
            ),
            egui::ResizeDirection::SouthEast,
            egui::CursorIcon::ResizeNwSe,
        ),
    ];
    let edges = [
        (
            "n",
            egui::Rect::from_min_max(rect.left_top(), egui::pos2(rect.right(), rect.top() + t)),
            egui::ResizeDirection::North,
            egui::CursorIcon::ResizeVertical,
        ),
        (
            "s",
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.bottom() - t),
                rect.right_bottom(),
            ),
            egui::ResizeDirection::South,
            egui::CursorIcon::ResizeVertical,
        ),
        (
            "w",
            egui::Rect::from_min_max(rect.left_top(), egui::pos2(rect.left() + t, rect.bottom())),
            egui::ResizeDirection::West,
            egui::CursorIcon::ResizeHorizontal,
        ),
        (
            "e",
            egui::Rect::from_min_max(egui::pos2(rect.right() - t, rect.top()), rect.right_bottom()),
            egui::ResizeDirection::East,
            egui::CursorIcon::ResizeHorizontal,
        ),
    ];
    for (name, zone, direction, cursor) in edges {
        resize_zone(ctx, name, zone, direction, cursor);
    }
    for (name, zone, direction, cursor) in corners {
        resize_zone(ctx, name, zone, direction, cursor);
    }
}

fn resize_zone(
    ctx: &egui::Context,
    name: &str,
    zone: egui::Rect,
    direction: egui::ResizeDirection,
    cursor: egui::CursorIcon,
) {
    egui::Area::new(egui::Id::new(("seal-resize", name)))
        .order(egui::Order::Foreground)
        .fixed_pos(zone.min)
        .show(ctx, |ui| {
            let response = ui.allocate_exact_size(zone.size(), egui::Sense::drag()).1;
            if response.hovered() {
                ui.ctx().set_cursor_icon(cursor);
            }
            if response.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
            }
        });
}
