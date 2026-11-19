use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::command::{always_enabled, CommandArgs, CommandSpec};
use crate::content::{
    self, map_io_error, set_content_error, set_content_error_fmt, ContentInlineError,
    ContentWatchState,
};
use crate::error::CoreError;
use crate::session::EditorSession;

#[derive(Clone, Debug, PartialEq)]
pub struct ViewportPaneState {
    pub view: String,
    pub pan_x: f32,
    pub pan_y: f32,
    pub zoom: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    /// `lit`, `unlit`, `wireframe`, or `collision`.
    pub shade: String,
    /// Per-pane show flags. Empty means the editor defaults.
    pub show_layers: BTreeMap<String, bool>,
}

impl Default for ViewportPaneState {
    fn default() -> Self {
        Self {
            view: "front".into(),
            pan_x: 0.0,
            pan_y: 200.0,
            zoom: 1.0,
            yaw: 35.0,
            pitch: -22.0,
            distance: 14.0,
            shade: "lit".into(),
            show_layers: default_viewport_show_layers(),
        }
    }
}

pub fn viewport_tab_default(panel_id: &str) -> ViewportPaneState {
    let seed = crate::panel::viewport_tab_seed(panel_id);
    let view = match seed {
        1 => "top",
        2 => "front",
        3 => "right",
        _ => "perspective",
    };
    ViewportPaneState {
        view: view.into(),
        pan_y: 200.0,
        yaw: if seed == 0 { 38.0 } else { 0.0 },
        pitch: if seed == 0 { -22.0 } else { 0.0 },
        distance: if seed == 0 { 14.0 } else { 10.0 },
        ..ViewportPaneState::default()
    }
}

pub fn ensure_viewport_tab(models: &mut EditorModels, panel_id: &str) {
    if panel_id == "viewport" || models.viewport_tabs.contains_key(panel_id) {
        return;
    }
    models.viewport_tabs.insert(panel_id.to_string(), viewport_tab_default(panel_id));
}

pub fn viewport_layout_default_panes(preset: &str) -> Vec<ViewportPaneState> {
    let pane = |view: &str, pan_y: f32, yaw: f32, pitch: f32| ViewportPaneState {
        view: view.into(),
        pan_y,
        yaw,
        pitch,
        ..ViewportPaneState::default()
    };
    match preset {
        "two_cols" => vec![pane("perspective", 180.0, 38.0, -22.0), pane("top", 200.0, 0.0, 0.0)],
        "two_rows" => vec![pane("perspective", 180.0, 38.0, -22.0), pane("front", 200.0, 0.0, 0.0)],
        "three" => vec![
            pane("perspective", 180.0, 38.0, -22.0),
            pane("top", 200.0, 0.0, 0.0),
            pane("front", 200.0, 0.0, 0.0),
        ],
        "four" => vec![
            pane("perspective", 180.0, 38.0, -22.0),
            pane("top", 200.0, 0.0, 0.0),
            pane("front", 200.0, 0.0, 0.0),
            pane("right", 200.0, 0.0, 0.0),
        ],
        _ => vec![ViewportPaneState::default()],
    }
}

pub fn viewport_store_active_pane(session: &mut EditorSession) {
    let index = session.models.viewport_active_pane;
    if session.models.viewport_panes.len() <= index {
        session.models.viewport_panes.resize(index + 1, ViewportPaneState::default());
    }
    let (yaw_default, pitch_default, dist_default) = {
        let pane = &session.models.viewport_panes[index];
        (pane.yaw, pane.pitch, pane.distance)
    };
    let view = session.models.viewport_view.clone();
    let pan_x = session.models.viewport_pan_x;
    let pan_y = session.models.viewport_pan_y;
    let zoom = session.models.viewport_zoom;
    let yaw = camera_draft_f32(session, "viewport-yaw", yaw_default);
    let pitch = camera_draft_f32(session, "viewport-pitch", pitch_default);
    let distance = camera_draft_f32(session, "viewport-distance", dist_default);
    let pane = &mut session.models.viewport_panes[index];
    pane.view = view;
    pane.pan_x = pan_x;
    pane.pan_y = pan_y;
    pane.zoom = zoom;
    pane.yaw = yaw;
    pane.pitch = pitch;
    pane.distance = distance;
    pane.shade = session.models.viewport_shade.clone();
    pane.show_layers = session.models.viewport_show_layers.clone();
}

pub fn viewport_load_active_pane(session: &mut EditorSession) {
    let index = session.models.viewport_active_pane;
    if index >= session.models.viewport_panes.len() {
        return;
    }
    let pane = session.models.viewport_panes[index].clone();
    session.models.viewport_view = pane.view;
    session.models.viewport_pan_x = pane.pan_x;
    session.models.viewport_pan_y = pane.pan_y;
    session.models.viewport_zoom = pane.zoom;
    if !pane.shade.is_empty() {
        session.models.viewport_shade = pane.shade;
    }
    if !pane.show_layers.is_empty() {
        session.models.viewport_show_layers = pane.show_layers;
        sync_viewport_show_aliases(&mut session.models);
    }
    let _ = session.set_draft("viewport-yaw", &pane.yaw.to_string());
    let _ = session.set_draft("viewport-pitch", &pane.pitch.to_string());
    let _ = session.set_draft("viewport-distance", &pane.distance.to_string());
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewportBookmark {
    pub view: String,
    pub selection: Option<String>,
    pub selection_ids: Vec<String>,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub zoom: f32,
}

pub const VIEWPORT_SHOW_LAYER_IDS: &[&str] = &[
    "grid",
    "bounds",
    "collision",
    "navigation",
    "icons",
    "names",
    "lights",
    "volumes",
    "sprites",
    "fog",
    "particles",
    "statics",
    "dynamics",
    "shadows",
    "probes",
    "gi",
    "axes",
];

pub fn default_viewport_show_layers() -> BTreeMap<String, bool> {
    let mut layers = BTreeMap::new();
    for &id in VIEWPORT_SHOW_LAYER_IDS {
        let on = id != "names";
        layers.insert(id.to_string(), on);
    }
    layers
}

fn sync_viewport_show_aliases(models: &mut EditorModels) {
    if let Some(on) = models.viewport_show_layers.get("grid").copied() {
        models.viewport_grid = on;
    }
    if let Some(on) = models.viewport_show_layers.get("names").copied() {
        models.viewport_show_names = on;
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OutlinerTable {
    pub type_width: f32,
    pub sort_column: String,
    pub sort_asc: bool,
    pub visible_optional: Vec<String>,
}

impl OutlinerTable {
    pub fn fresh() -> Self {
        Self {
            type_width: 72.0,
            sort_column: "label".into(),
            sort_asc: true,
            visible_optional: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlinerViewOptions {
    pub hierarchical: bool,
    pub only_selected: bool,
    pub hide_temporary: bool,
    pub only_current_level: bool,
    pub hide_folders: bool,
    pub stack_headers: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InspectorFieldEditSession {
    pub field: String,
    pub past_from: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InspectorViewOptions {
    pub list_view: bool,
    pub favourites_only: bool,
    pub only_modified: bool,
    pub show_advanced: bool,
    pub show_hidden_playing: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OutlinerActiveFilters {
    pub types: BTreeSet<String>,
    pub only_selected: bool,
    pub only_visible: bool,
    pub only_unsaved: bool,
    pub only_warnings: bool,
}

impl OutlinerActiveFilters {
    pub fn any(&self) -> bool {
        !self.types.is_empty()
            || self.only_selected
            || self.only_visible
            || self.only_unsaved
            || self.only_warnings
    }
}

impl Default for OutlinerViewOptions {
    fn default() -> Self {
        Self {
            hierarchical: true,
            only_selected: false,
            hide_temporary: false,
            only_current_level: false,
            hide_folders: false,
            stack_headers: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutlinerVisibleRow {
    Entity(String),
    Component { entity: String, component: String },
}

impl OutlinerVisibleRow {
    pub fn entity_id(&self) -> &str {
        match self {
            Self::Entity(id) => id.as_str(),
            Self::Component { entity, .. } => entity.as_str(),
        }
    }
}

/// Flat children under `root` for outliner stress and performance budgets.
pub const OUTLINER_STRESS_ROW_COUNT: usize = 100_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MockNode {
    pub id: String,
    pub name: String,
    pub parent: Option<String>,
    pub fields: Vec<(String, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutlinerDropMode {
    Parent,
    Before,
    After,
}

#[derive(Clone, Debug)]
pub struct EditorModels {
    pub nodes: Vec<MockNode>,
    pub expanded: Vec<String>,
    pub selection: Option<String>,
    pub console_query: String,
    pub console_level: String,
    pub console_match_index: usize,
    pub console_paused: bool,
    pub console_regex: bool,
    pub console_line_selection: Vec<usize>,
    pub console_folded: Vec<usize>,
    pub content_root: Option<PathBuf>,
    pub content_tree_root: Option<PathBuf>,
    pub content_query: String,
    pub content_list: bool,
    pub content_expanded: BTreeSet<String>,
    pub content_error: Option<ContentInlineError>,
    pub content_generation: u64,
    pub content_watch: ContentWatchState,
    pub asset_browser: crate::asset_browser::BrowserState,
    pub import_presets: crate::import_presets::ImportPresetBook,
    pub viewport_log: Vec<String>,
    /// `front` (x, y), `top` (x, z) or `side` (z, y).
    pub viewport_view: String,
    pub viewport_grid: bool,
    /// Per-layer visibility for the Show menu.
    pub viewport_show_layers: BTreeMap<String, bool>,
    pub viewport_show_menu_open: bool,
    pub snap: bool,
    /// `world` or `local` transform gizmo space.
    pub viewport_space: String,
    /// Surface align while placing.
    pub viewport_surface_snap: bool,
    /// Snap translations to grid vertices while moving.
    pub viewport_vertex_snap: bool,
    pub viewport_grid_snap: bool,
    pub viewport_grid_snap_step: f32,
    pub viewport_angle_snap: bool,
    pub viewport_angle_snap_deg: f32,
    pub viewport_scale_snap: bool,
    pub viewport_scale_snap_step: f32,
    pub viewport_camera_speed: f32,
    pub viewport_camera_speed_menu: bool,
    /// Warning ids the user hid from the viewport overlay.
    pub viewport_hidden_warnings: BTreeSet<String>,
    /// Show the navigation rebuild banner until hidden or cleared.
    pub viewport_nav_rebuild_pending: bool,
    /// `move`, `rotate`, or `scale`.
    pub viewport_tool: String,
    pub outliner_query: String,
    pub outliner_typeahead: String,
    /// Next outliner paint scrolls this entity into view, then clears.
    pub outliner_scroll_to: Option<String>,
    pub outliner_table: OutlinerTable,
    pub outliner_view: OutlinerViewOptions,
    pub outliner_filters: OutlinerActiveFilters,
    /// One-shot vertical scroll offset (pixels) for the outliner list.
    pub outliner_initial_scroll_y: Option<f32>,
    outliner_visible_cache_generation: u64,
    outliner_visible_cache: Vec<OutlinerVisibleRow>,
    pub inspector_query: String,
    pub inspector_sections: BTreeMap<String, bool>,
    /// Inspector instance id -> (active component key, object it was picked on).
    pub inspector_component_pick: BTreeMap<String, (String, Option<String>)>,
    /// Pixel height of the component tree band per inspector instance.
    pub inspector_tree_heights: BTreeMap<String, f32>,
    pub inspector_view: InspectorViewOptions,
    /// Property label keys shown when favourites-only is on.
    pub inspector_favourites: BTreeSet<String>,
    /// Active category chip id (`all` shows every category).
    pub inspector_category: String,
    /// Per-section advanced block open (`inspector-transform-advanced`, …).
    pub inspector_advanced_sections: BTreeMap<String, bool>,
    /// Location edit space for the inspector vector row (`absolute` or `relative`).
    pub inspector_location_space: String,
    /// When true, editing one scale axis updates every axis together.
    pub inspector_scale_uniform: bool,
    /// Active inspector property edit (preview until commit or cancel).
    pub inspector_field_edit: Option<InspectorFieldEditSession>,
    pub viewport_pan_x: f32,
    pub viewport_pan_y: f32,
    pub viewport_zoom: f32,
    /// Entity name labels over the scene (off by default).
    pub viewport_show_names: bool,
    /// FPS and entity stats overlay in the viewport corner.
    pub viewport_show_stats: bool,
    pub viewport_show_fps: bool,
    pub viewport_show_toolbar: bool,
    pub viewport_realtime: bool,
    pub viewport_fov: f32,
    pub viewport_far_plane: f32,
    pub viewport_screen_pct: f32,
    pub viewport_cinematic: bool,
    pub viewport_hamburger_open: bool,
    /// `lit`, `unlit`, `wireframe`, or `collision`.
    pub viewport_shade: String,
    pub content_pick: Option<String>,
    /// Last asset opened from an outliner type-column link (kind, asset name).
    pub asset_edit_last: Option<(String, String)>,
    pub viewport_bookmarks: BTreeMap<String, ViewportBookmark>,
    /// `single`, `two_cols`, `two_rows`, `three`, or `four`.
    pub viewport_layout_preset: String,
    pub viewport_panes: Vec<ViewportPaneState>,
    pub viewport_active_pane: usize,
    /// One viewport tile fills the panel while the layout preset stays set.
    pub viewport_pane_expanded: bool,
    /// Splitter fractions for the current layout. Empty uses the preset defaults.
    pub viewport_splits: Vec<f32>,
    /// Extra viewport tabs (`viewport#2` …) keep their own view and camera.
    pub viewport_tabs: BTreeMap<String, ViewportPaneState>,
    pub settings: Vec<Setting>,
    pub settings_query: String,
    pub keymap_profile_name: String,
    pub keymap_profile_draft: String,
    pub focused_panel: usize,
    pub theme: String,
    pub scale: f32,
    /// Saved panel state keyed by full panel instance id (including unknown panels).
    pub panel_states: BTreeMap<String, String>,
    /// Inspector instance id -> locked entity id.
    pub inspector_locks: BTreeMap<String, String>,
    pub asset_index: crate::asset_index::AssetIndex,
    pub pending_content_path: Option<crate::asset_index::PendingContentPath>,
    pub pending_delete_path: Option<String>,
    pub asset_reference_target: Option<String>,
    pub asset_reference_report: crate::asset_index::ReferenceReport,
    pub import_activity: Vec<crate::import_activity::ImportActivityEntry>,
    pub naming_rules: crate::naming_rules::NamingRuleBook,
    pub pending_merge_duplicates: Option<crate::asset_database_ops::PendingMergeDuplicates>,
    pub pending_migrate_assets: Option<crate::asset_database_ops::PendingMigrateAssets>,
    pub platform_overrides: crate::platform_overrides::PlatformOverrides,
    /// Platform shown in the Settings panel when resolving overrides.
    pub settings_preview_platform: String,
    /// Batch import, export, or reindex advanced a few files per frame.
    pub long_job: Option<crate::import_pipeline::LongJob>,
    /// The active quality level allows 2D glow (Project Settings > Rendering > Sprite glow).
    pub quality_glow: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Setting {
    pub id: String,
    pub category: String,
    pub title_key: String,
    pub description_key: String,
    pub default_value: String,
    pub user: Option<String>,
    pub project: Option<String>,
    pub restart: bool,
}

impl Setting {
    pub fn value(&self) -> &str {
        self.project.as_deref().or(self.user.as_deref()).unwrap_or(self.default_value.as_str())
    }

    pub fn source(&self) -> &'static str {
        if self.project.is_some() {
            "project"
        } else if self.user.is_some() {
            "user"
        } else {
            "default"
        }
    }
}

impl Default for EditorModels {
    fn default() -> Self {
        Self {
            nodes: vec![
                node("root", "World", None, &[("kind", "root")]),
                node("hero", "Hero", Some("root"), &[("role", "player")]),
                node("crate", "Crate", Some("root"), &[("role", "prop")]),
            ],
            expanded: vec!["root".into()],
            selection: Some("hero".into()),
            console_query: String::new(),
            console_level: "all".into(),
            console_match_index: 0,
            console_paused: false,
            console_regex: false,
            console_line_selection: Vec::new(),
            console_folded: Vec::new(),
            content_root: None,
            content_tree_root: None,
            content_query: String::new(),
            content_list: false,
            content_expanded: BTreeSet::new(),
            content_error: None,
            content_generation: 0,
            content_watch: ContentWatchState::default(),
            asset_browser: crate::asset_browser::BrowserState::default(),
            import_presets: crate::import_presets::ImportPresetBook::with_builtins(),
            viewport_log: Vec::new(),
            viewport_view: "front".into(),
            viewport_grid: true,
            viewport_show_layers: default_viewport_show_layers(),
            viewport_show_menu_open: false,
            snap: false,
            viewport_space: "world".into(),
            viewport_surface_snap: false,
            viewport_vertex_snap: false,
            viewport_grid_snap: false,
            viewport_grid_snap_step: 0.5,
            viewport_angle_snap: false,
            viewport_angle_snap_deg: 10.0,
            viewport_scale_snap: false,
            viewport_scale_snap_step: 0.25,
            viewport_camera_speed: 4.0,
            viewport_camera_speed_menu: false,
            viewport_hidden_warnings: BTreeSet::new(),
            viewport_nav_rebuild_pending: false,
            viewport_tool: "move".into(),
            outliner_query: String::new(),
            outliner_typeahead: String::new(),
            outliner_scroll_to: None,
            outliner_table: OutlinerTable::fresh(),
            outliner_view: OutlinerViewOptions::default(),
            outliner_filters: OutlinerActiveFilters::default(),
            outliner_initial_scroll_y: None,
            outliner_visible_cache_generation: 0,
            outliner_visible_cache: Vec::new(),
            inspector_query: String::new(),
            inspector_sections: BTreeMap::new(),
            inspector_component_pick: BTreeMap::new(),
            inspector_tree_heights: BTreeMap::new(),
            inspector_view: InspectorViewOptions::default(),
            inspector_favourites: BTreeSet::from([
                "inspector-location".into(),
                "inspector-intensity".into(),
            ]),
            inspector_category: "all".into(),
            inspector_advanced_sections: BTreeMap::new(),
            inspector_location_space: "absolute".into(),
            inspector_scale_uniform: false,
            inspector_field_edit: None,
            viewport_pan_x: 0.0,
            viewport_pan_y: 200.0,
            viewport_zoom: 1.0,
            viewport_show_names: false,
            viewport_show_stats: false,
            viewport_show_fps: false,
            viewport_show_toolbar: true,
            viewport_realtime: true,
            viewport_fov: 90.0,
            viewport_far_plane: 10_000.0,
            viewport_screen_pct: 100.0,
            viewport_cinematic: false,
            viewport_hamburger_open: false,
            viewport_shade: "lit".into(),
            content_pick: None,
            asset_edit_last: None,
            viewport_bookmarks: BTreeMap::new(),
            viewport_layout_preset: "single".into(),
            viewport_panes: vec![ViewportPaneState::default()],
            viewport_active_pane: 0,
            viewport_pane_expanded: false,
            viewport_splits: Vec::new(),
            viewport_tabs: BTreeMap::new(),
            settings: crate::settings::setting_catalog().iter().map(setting_from_meta).collect(),
            settings_query: String::new(),
            platform_overrides: crate::platform_overrides::PlatformOverrides::default(),
            settings_preview_platform: crate::platform_overrides::host_platform().into(),
            keymap_profile_name: String::new(),
            keymap_profile_draft: String::new(),
            focused_panel: 0,
            theme: "dark".into(),
            scale: 1.0,
            panel_states: BTreeMap::new(),
            inspector_locks: BTreeMap::new(),
            asset_index: crate::asset_index::AssetIndex::default(),
            pending_content_path: None,
            pending_delete_path: None,
            asset_reference_target: None,
            asset_reference_report: crate::asset_index::ReferenceReport::default(),
            import_activity: Vec::new(),
            naming_rules: crate::naming_rules::NamingRuleBook::default(),
            pending_merge_duplicates: None,
            pending_migrate_assets: None,
            long_job: None,
            quality_glow: true,
        }
    }
}

/// Component rows for the inspector tree: key and whether the row is inherited (dim).
pub fn inspector_component_entries(node: &MockNode) -> Vec<(&'static str, bool)> {
    outliner_component_keys(node)
        .into_iter()
        .map(|key| {
            let inherited = key == "mesh" && field_on_node(node, "source_kind") == "prefab";
            (key, inherited)
        })
        .collect()
}

pub fn outliner_component_keys(node: &MockNode) -> Vec<&'static str> {
    if node.id == "root" {
        return Vec::new();
    }
    let role = field_on_node(node, "role");
    if role == "folder" {
        return Vec::new();
    }
    let shape = field_on_node(node, "shape");
    let mut keys = vec!["transform"];
    if role == "light" {
        keys.push("light");
    } else if role == "camera" {
        keys.push("camera");
    } else if shape != "none" && !shape.is_empty() {
        keys.push("mesh");
    }
    for (key, field) in [("sprite", "sprite"), ("particles", "particles"), ("script", "script")] {
        if !field_on_node(node, field).is_empty() {
            keys.push(key);
        }
    }
    if field_on_node(node, "remote_on") == "yes" {
        keys.push("remote");
    }
    if matches!(field_on_node(node, "mask_shape"), "rect" | "circle" | "alpha") {
        keys.push("sprite_mask");
    }
    keys
}

fn field_on_node<'n>(node: &'n MockNode, name: &str) -> &'n str {
    node.fields.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str()).unwrap_or("")
}

fn node(id: &str, name: &str, parent: Option<&str>, fields: &[(&str, &str)]) -> MockNode {
    MockNode {
        id: id.into(),
        name: name.into(),
        parent: parent.map(str::to_string),
        fields: fields.iter().map(|(key, value)| ((*key).into(), (*value).into())).collect(),
    }
}

fn setting_from_meta(meta: &crate::settings::SettingMeta) -> Setting {
    Setting {
        id: meta.id.into(),
        category: meta.category.into(),
        title_key: crate::catalog_ids::setting_title_key(meta.id),
        description_key: crate::catalog_ids::setting_description_key(meta.id),
        default_value: meta.default_value.into(),
        user: None,
        project: None,
        restart: meta.restart,
    }
}

pub const PANEL_ORDER: &[&str] = &[
    "place",
    "outliner",
    "viewport",
    "welcome",
    "inspector",
    "world",
    "console",
    "content",
    "stats",
    "physics2d-profiler",
    "settings",
    "recovery",
    "test-runner",
];

impl EditorModels {
    pub fn children(&self, parent: Option<&str>) -> Vec<&MockNode> {
        self.nodes.iter().filter(|node| node.parent.as_deref() == parent).collect()
    }

    pub fn visible_ids(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.walk(None, &mut out);
        out
    }

    fn walk(&self, parent: Option<&str>, out: &mut Vec<String>) {
        for node in self.children(parent) {
            out.push(node.id.clone());
            if self.expanded.iter().any(|id| id == &node.id) {
                self.walk(Some(&node.id), out);
            }
        }
    }

    pub fn visible_outliner_rows(&self) -> Vec<OutlinerVisibleRow> {
        self.visible_outliner_rows_filtered(&[])
    }

    pub fn visible_outliner_rows_filtered(&self, selection: &[String]) -> Vec<OutlinerVisibleRow> {
        let mut out = Vec::new();
        if !self.outliner_view.hierarchical {
            for node in &self.nodes {
                if node.id == "root" {
                    continue;
                }
                if self.outliner_entity_visible(node, 0, selection) {
                    out.push(OutlinerVisibleRow::Entity(node.id.clone()));
                }
            }
            return out;
        }
        self.walk_outliner(None, 0, selection, &mut out);
        out
    }

    fn outliner_visible_cache_key(&self, query: &str, selection: &[String]) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.nodes.len().hash(&mut hasher);
        self.expanded.hash(&mut hasher);
        self.outliner_view.hierarchical.hash(&mut hasher);
        self.outliner_view.only_selected.hash(&mut hasher);
        self.outliner_view.hide_temporary.hash(&mut hasher);
        self.outliner_view.only_current_level.hash(&mut hasher);
        self.outliner_view.hide_folders.hash(&mut hasher);
        self.outliner_filters.types.hash(&mut hasher);
        self.outliner_filters.only_selected.hash(&mut hasher);
        self.outliner_filters.only_visible.hash(&mut hasher);
        self.outliner_filters.only_unsaved.hash(&mut hasher);
        self.outliner_filters.only_warnings.hash(&mut hasher);
        query.hash(&mut hasher);
        selection.hash(&mut hasher);
        self.outliner_table.sort_column.hash(&mut hasher);
        self.outliner_table.sort_asc.hash(&mut hasher);
        hasher.finish()
    }

    pub fn prepare_outliner_visible_rows(&mut self, query: &str, selection: &[String]) {
        let key = self.outliner_visible_cache_key(query, selection);
        if self.outliner_visible_cache_generation != key {
            self.outliner_visible_cache = self.outliner_visible_rows_for_query(query, selection);
            self.outliner_visible_cache_generation = key;
        }
    }

    pub fn outliner_visible_rows_slice(&self) -> &[OutlinerVisibleRow] {
        &self.outliner_visible_cache
    }

    pub fn take_outliner_initial_scroll_y(&mut self) -> Option<f32> {
        self.outliner_initial_scroll_y.take()
    }

    pub fn reset_outliner_visible_cache(&mut self) {
        self.outliner_visible_cache_generation = 0;
        self.outliner_visible_cache.clear();
    }

    fn outliner_visible_rows_for_query(
        &self,
        query: &str,
        selection: &[String],
    ) -> Vec<OutlinerVisibleRow> {
        if query.trim().is_empty() {
            return self.visible_outliner_rows_filtered(selection);
        }
        let mut ids: Vec<String> = self
            .nodes
            .iter()
            .filter(|node| crate::outliner_query::outliner_matches(node, query))
            .map(|node| node.id.clone())
            .collect();
        let node_name = |id: &str| {
            self.nodes
                .iter()
                .find(|node| node.id == id)
                .map(|node| node.name.to_lowercase())
                .unwrap_or_default()
        };
        let node_type = |id: &str| {
            self.nodes.iter().find(|node| node.id == id).map_or(String::new(), |node| {
                crate::outliner_query::outliner_type_text(node).to_lowercase()
            })
        };
        if self.outliner_table.sort_column == "type" {
            ids.sort_by(|left, right| {
                let order = node_type(left).cmp(&node_type(right));
                if self.outliner_table.sort_asc {
                    order
                } else {
                    order.reverse()
                }
            });
        } else {
            ids.sort_by(|left, right| {
                let order = crate::outliner_query::natural_cmp(&node_name(left), &node_name(right));
                if self.outliner_table.sort_asc {
                    order
                } else {
                    order.reverse()
                }
            });
        }
        ids.into_iter().map(OutlinerVisibleRow::Entity).collect()
    }

    fn outliner_entity_visible(&self, node: &MockNode, depth: usize, selection: &[String]) -> bool {
        if node.id == "root" {
            return true;
        }
        if !self.outliner_passes_filters(node, selection) {
            return false;
        }
        if self.outliner_view.hide_folders && field_on_node(node, "role") == "folder" {
            return false;
        }
        if self.outliner_view.hide_temporary && field_on_node(node, "spawned_play") == "yes" {
            return false;
        }
        if self.outliner_view.only_selected
            && !selection.is_empty()
            && !selection.iter().any(|id| id == &node.id)
        {
            return false;
        }
        if self.outliner_view.only_current_level && depth != 1 {
            return false;
        }
        true
    }

    fn outliner_passes_filters(&self, node: &MockNode, selection: &[String]) -> bool {
        let filters = &self.outliner_filters;
        if !filters.any() {
            return true;
        }
        if filters.only_selected
            && (selection.is_empty() || !selection.iter().any(|id| id == &node.id))
        {
            return false;
        }
        if filters.only_visible && field_on_node(node, "hidden") == "yes" {
            return false;
        }
        if filters.only_unsaved && field_on_node(node, "unsaved") != "yes" {
            return false;
        }
        if filters.only_warnings && field_on_node(node, "outliner_warning").is_empty() {
            return false;
        }
        if !filters.types.is_empty() {
            let key = crate::outliner_query::outliner_filter_type_key(node);
            if !filters.types.contains(&key) {
                return false;
            }
        }
        true
    }

    fn walk_outliner(
        &self,
        parent: Option<&str>,
        depth: usize,
        selection: &[String],
        out: &mut Vec<OutlinerVisibleRow>,
    ) {
        for node in self.children(parent) {
            if !self.outliner_entity_visible(node, depth, selection) {
                continue;
            }
            out.push(OutlinerVisibleRow::Entity(node.id.clone()));
            let child_depth = depth + 1;
            let descend = self.expanded.iter().any(|id| id == &node.id)
                && !self.outliner_view.only_current_level;
            if descend {
                for key in outliner_component_keys(node) {
                    out.push(OutlinerVisibleRow::Component {
                        entity: node.id.clone(),
                        component: key.to_string(),
                    });
                }
                self.walk_outliner(Some(&node.id), child_depth, selection, out);
            }
        }
    }

    pub fn toggle(&mut self, id: &str) {
        if let Some(index) = self.expanded.iter().position(|item| item == id) {
            self.expanded.remove(index);
        } else {
            self.expanded.push(id.to_string());
        }
    }

    pub fn rename(&mut self, id: &str, name: &str) -> Option<String> {
        let node = self.nodes.iter_mut().find(|node| node.id == id)?;
        let previous = node.name.clone();
        node.name = name.to_string();
        Some(previous)
    }

    pub fn reparent(
        &mut self,
        id: &str,
        parent: Option<&str>,
    ) -> Result<Option<String>, CoreError> {
        if parent == Some(id) || self.is_descendant(id, parent) {
            return Err(CoreError::new("cannot parent a node under itself"));
        }
        let node = self
            .nodes
            .iter_mut()
            .find(|node| node.id == id)
            .ok_or_else(|| CoreError::new("unknown mock node"))?;
        let previous = node.parent.clone();
        node.parent = parent.map(str::to_string);
        Ok(previous)
    }

    pub fn outliner_reparent_allowed(&self, child: &str, new_parent: &str) -> bool {
        if child == "root" || child == new_parent {
            return false;
        }
        !self.is_descendant(child, Some(new_parent))
    }

    pub fn reorder_sibling(
        &mut self,
        child: &str,
        before: Option<&str>,
        after: Option<&str>,
    ) -> Result<(), CoreError> {
        if child == "root" {
            return Err(CoreError::new("cannot move the world root"));
        }
        let anchor = before.or(after).ok_or_else(|| CoreError::new("need a drop target"))?;
        let anchor_parent =
            self.nodes.iter().find(|node| node.id == anchor).and_then(|node| node.parent.clone());
        self.reparent(child, anchor_parent.as_deref())?;
        let parent =
            self.nodes.iter().find(|node| node.id == child).and_then(|node| node.parent.clone());
        let sibling_order: Vec<String> = self
            .nodes
            .iter()
            .filter(|node| node.parent == parent)
            .map(|node| node.id.clone())
            .collect();
        let first_ix =
            self.nodes.iter().position(|node| node.parent == parent).unwrap_or(self.nodes.len());
        let mut order: Vec<String> =
            sibling_order.iter().filter(|id| id.as_str() != child).cloned().collect();
        let insert_at = if let Some(b) = before {
            order.iter().position(|id| id == b).unwrap_or(order.len())
        } else if let Some(a) = after {
            order.iter().position(|id| id == a).map(|i| i + 1).unwrap_or(order.len())
        } else {
            order.len()
        };
        order.insert(insert_at, child.to_string());
        let siblings: Vec<MockNode> = order
            .iter()
            .filter_map(|id| self.nodes.iter().find(|node| &node.id == id).cloned())
            .collect();
        self.nodes.retain(|node| node.parent != parent);
        for (offset, node) in siblings.into_iter().enumerate() {
            self.nodes.insert(first_ix + offset, node);
        }
        Ok(())
    }

    fn is_descendant(&self, id: &str, parent: Option<&str>) -> bool {
        let Some(parent) = parent else {
            return false;
        };
        let mut cursor = Some(parent);
        while let Some(current) = cursor {
            if current == id {
                return true;
            }
            cursor = self
                .nodes
                .iter()
                .find(|node| node.id == current)
                .and_then(|node| node.parent.as_deref());
        }
        false
    }

    pub fn set_field(&mut self, id: &str, field: &str, value: &str) -> Option<String> {
        let node = self.nodes.iter_mut().find(|node| node.id == id)?;
        if let Some(existing) = node.fields.iter_mut().find(|(key, _)| key == field) {
            let previous = existing.1.clone();
            existing.1 = value.to_string();
            Some(previous)
        } else {
            node.fields.push((field.into(), value.into()));
            Some(String::new())
        }
    }

    pub fn content_names(&self, show_hidden: bool) -> Vec<String> {
        let Some(root) = &self.content_root else {
            return Vec::new();
        };
        content::list_names_in_folder(root, show_hidden, &self.content_query, &[])
    }

    pub fn filtered_logs<'a>(
        &self,
        records: &'a [LogLine],
        max_entries: usize,
    ) -> Vec<&'a LogLine> {
        let matched: Vec<&LogLine> = records
            .iter()
            .filter(|line| {
                self.console_level == "all" || line.level.eq_ignore_ascii_case(&self.console_level)
            })
            .filter(|line| self.console_query_matches(line))
            .collect();
        let keep = max_entries.max(1);
        let start = matched.len().saturating_sub(keep);
        matched[start..].to_vec()
    }

    pub fn set_setting(&mut self, id: &str, value: &str, layer: &str) -> Option<String> {
        let setting = self.settings.iter_mut().find(|setting| setting.id == id)?;
        let previous = setting.value().to_string();
        match layer {
            "project" => setting.project = Some(value.to_string()),
            _ => setting.user = Some(value.to_string()),
        }
        if id == "ui.theme" {
            self.theme = setting.value().to_string();
        }
        if id == "ui.scale" {
            if let Ok(number) = setting.value().parse::<f32>() {
                let factor = if number > 3.0 { number / 100.0 } else { number };
                self.scale = clamp_scale(factor);
            }
        }
        Some(previous)
    }

    pub fn reset_setting(&mut self, id: &str) {
        if let Some(setting) = self.settings.iter_mut().find(|setting| setting.id == id) {
            setting.user = None;
            setting.project = None;
        }
        if id == "ui.theme" {
            self.theme = "dark".into();
        }
        if id == "ui.scale" {
            self.scale = 1.0;
        }
    }

    pub fn console_query_matches(&self, line: &LogLine) -> bool {
        if self.console_query.is_empty() {
            return true;
        }
        if self.console_regex {
            regex::Regex::new(&self.console_query)
                .map(|pattern| pattern.is_match(&line.message))
                .unwrap_or_else(|_| {
                    line.message.to_lowercase().contains(&self.console_query.to_lowercase())
                })
        } else {
            line.message.to_lowercase().contains(&self.console_query.to_lowercase())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogLine {
    pub level: String,
    pub message: String,
    pub source: String,
    pub fields: Vec<(String, String)>,
}

pub fn collect_log_lines(clear_before: usize) -> Vec<LogLine> {
    seal_log::with_logger(|logger| {
        logger.records().iter().skip(clear_before).map(record_to_line).collect()
    })
}

fn record_to_line(record: &seal_log::Record) -> LogLine {
    let level = match record.level {
        seal_log::Level::Error => "error",
        seal_log::Level::Warn => "warn",
        seal_log::Level::Info => "info",
        seal_log::Level::Debug => "debug",
        seal_log::Level::Trace => "trace",
    }
    .to_string();
    let fields = record
        .fields
        .as_ref()
        .map(|map| map.iter().map(|(key, value)| (key.clone(), value.clone())).collect())
        .unwrap_or_default();
    LogLine { level, message: record.message.clone(), source: record.source.clone(), fields }
}

pub fn console_level_counts(clear_before: usize) -> (usize, usize) {
    let mut errors = 0;
    let mut warnings = 0;
    seal_log::with_logger(|logger| {
        for record in logger.records().iter().skip(clear_before) {
            match record.level {
                seal_log::Level::Error => errors += 1,
                seal_log::Level::Warn => warnings += 1,
                _ => {}
            }
        }
    });
    (errors, warnings)
}

pub const CONSOLE_ROW_DRAFT: &str = "console-row";

pub fn log_records() -> Vec<LogLine> {
    seal_log::with_logger(|logger| logger.records().iter().map(record_to_line).collect())
}

pub fn format_log_line(messages: &crate::messages::Messages, line: &LogLine) -> String {
    messages.format(
        "diagnostics-log-line",
        &[
            ("level", line.level.as_str()),
            ("source", "console"),
            ("message", line.message.as_str()),
        ],
    )
}

pub fn clamp_scale(value: f32) -> f32 {
    value.clamp(0.5, 3.0)
}

pub fn visible_range(total: usize, scroll: usize, view: usize) -> std::ops::Range<usize> {
    let start = scroll.min(total);
    let end = start.saturating_add(view).min(total);
    start..end
}

pub fn seed_outliner_stress_rows(models: &mut EditorModels) {
    models.nodes.clear();
    models.nodes.push(MockNode {
        id: "root".into(),
        name: "World".into(),
        parent: None,
        fields: Vec::new(),
    });
    models.nodes.extend((0..OUTLINER_STRESS_ROW_COUNT).map(|index| MockNode {
        id: format!("n{index}"),
        name: format!("Entity {index:06}"),
        parent: Some("root".into()),
        fields: Vec::new(),
    }));
    models.expanded = vec!["root".into()];
    models.outliner_filters = OutlinerActiveFilters::default();
    models.outliner_view = OutlinerViewOptions::default();
    models.reset_outliner_visible_cache();
}

pub fn register_model_commands(session: &mut EditorSession) -> Result<(), CoreError> {
    let specs: &[(&str, &str, &str, CommandFn)] = &[
        ("outliner.toggle", "command-outliner-toggle", "view", toggle_node),
        ("outliner.select", "command-outliner-select", "view", select_node),
        ("outliner.select-extend", "command-outliner-select", "view", select_extend),
        ("outliner.select-set", "command-outliner-select-set", "view", select_set),
        ("outliner.range_select", "command-outliner-select", "view", range_select),
        ("outliner.rename", "command-outliner-rename", "edit", rename_node),
        ("outliner.reparent", "command-outliner-reparent", "edit", reparent_node),
        ("outliner.reorder", "command-outliner-reorder", "edit", reorder_outliner),
        ("inspector.set", "command-inspector-set", "edit", set_field),
        ("inspector.list_edit", "command-inspector-list-edit", "edit", list_edit_field),
        (
            "inspector.field_edit_begin",
            "command-inspector-field-edit-begin",
            "edit",
            begin_inspector_field_edit,
        ),
        (
            "inspector.field_edit_end",
            "command-inspector-field-edit-end",
            "edit",
            end_inspector_field_edit,
        ),
        (
            "inspector.field_edit_cancel",
            "command-inspector-field-edit-cancel",
            "edit",
            cancel_inspector_field_edit,
        ),
        (
            "inspector.vector_space",
            "command-inspector-vector-space",
            "view",
            set_inspector_vector_space,
        ),
        (
            "inspector.scale_uniform",
            "command-inspector-scale-uniform",
            "view",
            set_inspector_scale_uniform,
        ),
        ("inspector.filter", "command-inspector-filter", "view", filter_inspector),
        (
            "inspector.view_option",
            "command-inspector-view-option",
            "view",
            set_inspector_view_option,
        ),
        ("inspector.view_mode", "command-inspector-view-mode", "view", set_inspector_view_mode),
        ("inspector.category", "command-inspector-category", "view", set_inspector_category),
        ("inspector.expand_all", "command-inspector-expand-all", "view", expand_inspector_sections),
        (
            "inspector.collapse_all",
            "command-inspector-collapse-all",
            "view",
            collapse_inspector_sections,
        ),
        ("inspector.section", "command-inspector-section", "view", toggle_inspector_section),
        (
            "inspector.advanced_section",
            "command-inspector-advanced-section",
            "view",
            toggle_inspector_advanced_section,
        ),
        (
            "inspector.property_copy",
            "command-inspector-property-copy",
            "edit",
            stub_inspector_property,
        ),
        (
            "inspector.property_paste",
            "command-inspector-property-paste",
            "edit",
            stub_inspector_property,
        ),
        (
            "inspector.property_copy_name",
            "command-inspector-property-copy-name",
            "edit",
            stub_inspector_property,
        ),
        (
            "inspector.property_reset",
            "command-inspector-property-reset",
            "edit",
            reset_inspector_property,
        ),
        (
            "inspector.property_favourite",
            "command-inspector-property-favourite",
            "edit",
            stub_inspector_property,
        ),
        (
            "inspector.property_settings_search",
            "command-inspector-property-settings",
            "edit",
            stub_inspector_property,
        ),
        (
            "inspector.pick_component",
            "command-inspector-pick-component",
            "view",
            pick_inspector_component,
        ),
        (
            "inspector.tree_height",
            "command-inspector-tree-height",
            "view",
            set_inspector_tree_height,
        ),
        (
            "inspector.component_rename",
            "command-inspector-component-rename",
            "edit",
            stub_inspector_component,
        ),
        (
            "inspector.component_duplicate",
            "command-inspector-component-duplicate",
            "edit",
            stub_inspector_component,
        ),
        (
            "inspector.component_delete",
            "command-inspector-component-delete",
            "edit",
            stub_inspector_component,
        ),
        (
            "inspector.component_copy",
            "command-inspector-component-copy",
            "edit",
            stub_inspector_component,
        ),
        (
            "inspector.component_paste",
            "command-inspector-component-paste",
            "edit",
            stub_inspector_component,
        ),
        (
            "inspector.component_make_root",
            "command-inspector-component-make-root",
            "edit",
            stub_inspector_component,
        ),
        (
            "inspector.reattach_component",
            "command-inspector-reattach-component",
            "edit",
            reattach_inspector_component,
        ),
        (
            "inspector.edit_script",
            "command-inspector-edit-script",
            "edit",
            stub_inspector_component,
        ),
        ("inspector.reset_field", "command-inspector-reset-field", "edit", reset_inspector_field),
        ("graph.new", "command-graph-new", "edit", crate::graph_doc::cmd_graph_new),
        ("graph.open", "command-graph-open", "edit", crate::graph_doc::cmd_graph_open),
        ("console.filter", "command-console-filter", "view", filter_console),
        ("console.jump_match", "command-console-jump-match", "view", jump_console_match),
        ("console.pause", "command-console-pause", "view", pause_console),
        ("console.toggle_regex", "command-console-regex", "view", toggle_console_regex),
        ("console.select_row", "command-console-select-row", "view", select_console_row),
        ("console.select_extend", "command-console-select-extend", "view", extend_console_row),
        ("console.fold_toggle", "command-console-fold", "view", toggle_console_fold),
        ("console.run", "command-console-run", "view", run_console_command),
        ("console.focus_level", "command-console-focus-level", "view", focus_console_level),
        ("content.query", "command-content-query", "view", query_content),
        ("content.toggle-view", "command-content-view", "view", toggle_content),
        ("content.pick", "command-content-pick", "view", pick_content),
        ("content.rename", "command-content-rename", "file", rename_content),
        ("content.move", "command-content-move", "file", move_content),
        ("content.open", "command-content-open", "file", open_content),
        ("asset.edit", "command-asset-edit", "file", edit_asset),
        ("content.toggle_tree", "command-content-toggle-tree", "view", toggle_content_tree),
        ("content.enter", "command-content-enter", "view", enter_content),
        ("content.delete", "command-content-delete", "file", delete_content_cmd),
        ("asset.find_references", "command-asset-find-references", "view", asset_find_references),
        ("content.duplicate", "command-content-duplicate", "file", duplicate_content_cmd),
        ("content.reveal", "command-content-reveal", "file", reveal_content),
        ("content.new_scene", "command-content-new-scene", "file", new_content_scene),
        ("viewport.note", "command-viewport-note", "view", note_viewport),
        (
            "viewport.bookmark_store",
            "command-viewport-bookmark-store",
            "view",
            store_viewport_bookmark,
        ),
        (
            "viewport.bookmark_recall",
            "command-viewport-bookmark-recall",
            "view",
            recall_viewport_bookmark,
        ),
        ("viewport.camera", "command-viewport-camera", "view", set_viewport_camera),
        ("viewport.view", "command-viewport-view", "view", set_view),
        ("viewport.layout", "command-viewport-layout", "view", set_viewport_layout),
        ("viewport.pane_focus", "command-viewport-pane-focus", "view", set_viewport_pane_focus),
        (
            "viewport.pane_expand",
            "command-viewport-pane-expand",
            "view",
            toggle_viewport_pane_expand,
        ),
        ("viewport.shade", "command-viewport-shade", "view", set_shade),
        ("viewport.grid", "command-viewport-grid", "view", toggle_grid),
        ("viewport.show", "command-viewport-show", "view", set_viewport_show),
        (
            "viewport.show_defaults",
            "command-viewport-show-defaults",
            "view",
            viewport_show_defaults,
        ),
        ("viewport.show_menu", "command-viewport-show", "view", set_viewport_show_menu),
        (
            "viewport.hamburger_menu",
            "command-viewport-hamburger-menu",
            "view",
            set_viewport_hamburger_menu,
        ),
        ("viewport.realtime", "command-viewport-realtime", "view", toggle_viewport_realtime),
        ("viewport.show_fps", "command-viewport-show-fps", "view", toggle_viewport_show_fps),
        (
            "viewport.show_toolbar",
            "command-viewport-show-toolbar",
            "view",
            toggle_viewport_show_toolbar,
        ),
        ("viewport.stats", "command-viewport-show-stats", "view", toggle_viewport_show_stats),
        ("viewport.isolate", "command-viewport-isolate", "view", viewport_isolate_cmd),
        ("viewport.overlay", "command-viewport-overlay", "view", viewport_overlay_cmd),
        ("viewport.cinematic", "command-viewport-cinematic", "view", toggle_viewport_cinematic),
        ("viewport.fov", "command-viewport-fov", "view", set_viewport_fov),
        ("viewport.far_plane", "command-viewport-far-plane", "view", set_viewport_far_plane),
        ("viewport.screen_pct", "command-viewport-screen-pct", "view", set_viewport_screen_pct),
        (
            "viewport.create_camera",
            "command-viewport-create-camera",
            "view",
            create_viewport_camera,
        ),
        ("viewport.screenshot", "command-viewport-screenshot", "view", viewport_screenshot),
        ("viewport.frame", "command-viewport-frame", "view", frame_viewport_selection),
        (
            "viewport.seed_primitives",
            "command-viewport-seed-primitives",
            "view",
            seed_viewport_primitives,
        ),
        ("viewport.snap", "command-viewport-snap", "view", toggle_snap),
        ("viewport.space", "command-viewport-space", "view", toggle_viewport_space),
        ("viewport.surface_snap", "command-viewport-surface-snap", "view", toggle_surface_snap),
        ("viewport.vertex_snap", "command-viewport-vertex-snap", "view", toggle_vertex_snap),
        ("viewport.grid_snap", "command-viewport-grid-snap", "view", toggle_grid_snap),
        ("viewport.grid_snap_step", "command-viewport-grid-snap-step", "view", set_grid_snap_step),
        (
            "viewport.grid_snap_decrease",
            "command-viewport-grid-snap-decrease",
            "view",
            grid_snap_decrease,
        ),
        (
            "viewport.grid_snap_increase",
            "command-viewport-grid-snap-increase",
            "view",
            grid_snap_increase,
        ),
        ("viewport.angle_snap", "command-viewport-angle-snap", "view", toggle_angle_snap),
        (
            "viewport.angle_snap_step",
            "command-viewport-angle-snap-step",
            "view",
            set_angle_snap_step,
        ),
        ("viewport.scale_snap", "command-viewport-scale-snap", "view", toggle_scale_snap),
        (
            "viewport.scale_snap_step",
            "command-viewport-scale-snap-step",
            "view",
            set_scale_snap_step,
        ),
        ("viewport.camera_speed", "command-viewport-camera-speed", "view", set_camera_speed),
        (
            "viewport.camera_speed_menu",
            "command-viewport-camera-speed",
            "view",
            set_camera_speed_menu,
        ),
        ("viewport.warning_hide", "command-viewport-warning-hide", "view", hide_viewport_warning),
        ("viewport.move", "command-viewport-move", "edit", move_entity),
        ("viewport.tool", "command-viewport-tool", "view", set_tool),
        ("outliner.filter", "command-outliner-filter", "view", filter_outliner),
        ("outliner.filter_next", "command-outliner-filter-next", "view", jump_outliner_filter_next),
        ("outliner.filter_prev", "command-outliner-filter-prev", "view", jump_outliner_filter_prev),
        ("outliner.typeahead", "command-outliner-typeahead", "view", typeahead_outliner),
        ("outliner.new_folder", "command-outliner-new-folder", "edit", new_outliner_folder),
        ("outliner.sort", "command-outliner-sort", "view", sort_outliner),
        ("outliner.column", "command-outliner-column", "view", toggle_outliner_column),
        ("outliner.resize_type", "command-outliner-resize-type", "view", resize_outliner_type),
        ("outliner.view_option", "command-outliner-view-option", "view", set_outliner_view_option),
        (
            "outliner.filter_toggle",
            "command-outliner-filter-toggle",
            "view",
            toggle_outliner_filter,
        ),
        ("scene.add", "command-scene-add", "edit", add_entity),
        ("sprite_shape.edit", "command-sprite-shape", "edit", crate::sprite_shape_cmd::edit_cmd),
        ("collider.edit", "command-collider-edit", "edit", crate::collider_edit::edit_cmd),
        ("collider.outline", "command-collider-outline", "edit", crate::collider_edit::outline_cmd),
        ("tiles.paint", "command-tiles-paint", "edit", crate::tiles::paint_cmd),
        ("tiles.erase", "command-tiles-erase", "edit", crate::tiles::erase_cmd),
        (
            "tiles.pattern_save",
            "command-tiles-pattern-save",
            "edit",
            crate::tiles::pattern_save_cmd,
        ),
        (
            "tiles.pattern_paint",
            "command-tiles-pattern-paint",
            "edit",
            crate::tiles::pattern_paint_cmd,
        ),
        ("tiles.data_set", "command-tiles-data-set", "edit", crate::tiles::tile_data_set_cmd),
        (
            "tiles.data_remove",
            "command-tiles-data-remove",
            "edit",
            crate::tiles::tile_data_remove_cmd,
        ),
        ("tiles.tool_brush", "command-tiles-tool-brush", "edit", tile_tool_brush),
        ("tiles.tool_rectangle", "command-tiles-tool-rectangle", "edit", tile_tool_rectangle),
        ("tiles.tool_line", "command-tiles-tool-line", "edit", tile_tool_line),
        ("tiles.tool_fill", "command-tiles-tool-fill", "edit", tile_tool_fill),
        ("tiles.tool_picker", "command-tiles-tool-picker", "edit", tile_tool_picker),
        ("tiles.tool_eraser", "command-tiles-tool-eraser", "edit", tile_tool_eraser),
        ("tiles.tool_random", "command-tiles-tool-random", "edit", tile_tool_random),
        ("tiles.tool_pattern", "command-tiles-tool-pattern", "edit", tile_tool_pattern),
        ("asset.import", "command-asset-import", "file", import_asset),
        (
            "asset.sprite_source_import",
            "command-asset-sprite-source-import",
            "file",
            crate::aseprite::cmd_import,
        ),
        ("scene.floor", "command-scene-floor", "edit", snap_floor),
        ("scene.reset-transform", "command-scene-reset", "edit", reset_transform),
        ("scene.hide", "command-scene-hide", "edit", toggle_hidden),
        ("scene.hide_solo", "command-scene-hide-solo", "edit", hide_solo),
        ("scene.lock", "command-scene-lock", "edit", toggle_locked),
        ("scene.unselectable", "command-scene-unselectable", "edit", toggle_unselectable),
        ("scene.group", "command-scene-group", "edit", group_selection),
        ("scene.ungroup", "command-scene-ungroup", "edit", ungroup_selection),
        ("scene.make_prefab", "command-scene-make-prefab", "edit", make_prefab),
        ("scene.add_prefab", "command-scene-add-prefab", "edit", add_prefab),
        ("prefab.edit", "command-prefab-edit", "edit", prefab_edit),
        ("scene.open", "command-scene-open", "file", open_scene),
        ("content.open_file", "command-content-open-file", "file", open_content_file),
        ("asset.open", "command-asset-open", "file", crate::asset_editor::cmd_asset_open),
        ("asset.close", "command-asset-close", "file", crate::asset_editor::cmd_asset_close),
        ("asset.fx_set", "command-asset-fx-set", "edit", crate::asset_editor::cmd_fx_set),
        ("asset.fx_text", "command-asset-fx-text", "edit", crate::fx_curve::cmd_fx_text),
        ("asset.fx_reset", "command-asset-fx-reset", "edit", crate::asset_editor::cmd_fx_reset),
        ("asset.fx_new", "command-asset-fx-new", "file", crate::asset_editor::cmd_fx_new),
        (
            "asset.frames_new",
            "command-asset-frames-new",
            "file",
            crate::asset_editor::cmd_frames_new,
        ),
        (
            "asset.soundset_new",
            "command-asset-soundset-new",
            "file",
            crate::soundset_editor::cmd_soundset_new,
        ),
        (
            "asset.soundset_set",
            "command-asset-soundset-set",
            "edit",
            crate::soundset_editor::cmd_soundset_set,
        ),
        (
            "asset.soundset_entry_add",
            "command-asset-soundset-entry-add",
            "edit",
            crate::soundset_editor::cmd_soundset_entry_add,
        ),
        (
            "asset.soundset_entry_remove",
            "command-asset-soundset-entry-remove",
            "edit",
            crate::soundset_editor::cmd_soundset_entry_remove,
        ),
        (
            "asset.soundset_entry_move",
            "command-asset-soundset-entry-move",
            "edit",
            crate::soundset_editor::cmd_soundset_entry_move,
        ),
        (
            "asset.soundset_play",
            "command-asset-soundset-play",
            "edit",
            crate::soundset_editor::cmd_soundset_play,
        ),
        (
            "asset.strings_new",
            "command-asset-strings-new",
            "file",
            crate::strings_editor::cmd_strings_new,
        ),
        (
            "asset.strings_set",
            "command-asset-strings-set",
            "edit",
            crate::strings_editor::cmd_strings_set,
        ),
        (
            "asset.strings_add",
            "command-asset-strings-add",
            "edit",
            crate::strings_editor::cmd_strings_add,
        ),
        (
            "asset.strings_remove",
            "command-asset-strings-remove",
            "edit",
            crate::strings_editor::cmd_strings_remove,
        ),
        (
            "asset.strings_rename",
            "command-asset-strings-rename",
            "edit",
            crate::strings_editor::cmd_strings_rename,
        ),
        (
            "asset.frames_clip_add",
            "command-asset-frames-clip-add",
            "edit",
            crate::asset_editor::cmd_frames_clip_add,
        ),
        (
            "asset.frames_clip_remove",
            "command-asset-frames-clip-remove",
            "edit",
            crate::asset_editor::cmd_frames_clip_remove,
        ),
        (
            "asset.frames_clip_set",
            "command-asset-frames-clip-set",
            "edit",
            crate::asset_editor::cmd_frames_clip_set,
        ),
        (
            "asset.frames_slice",
            "command-asset-frames-slice",
            "edit",
            crate::asset_editor::cmd_frames_slice,
        ),
        (
            "asset.frames_frame",
            "command-asset-frames-frame",
            "edit",
            crate::asset_editor::cmd_frames_frame,
        ),
        (
            "asset.frames_socket",
            "command-asset-frames-socket",
            "edit",
            crate::asset_editor::cmd_frames_socket,
        ),
        ("asset.ui_new", "command-asset-ui-new", "file", crate::ui_designer::cmd_ui_new),
        ("asset.ui_add", "command-asset-ui-add", "edit", crate::ui_designer::cmd_ui_add),
        ("asset.ui_remove", "command-asset-ui-remove", "edit", crate::ui_designer::cmd_ui_remove),
        ("asset.ui_move", "command-asset-ui-move", "edit", crate::ui_designer::cmd_ui_move),
        ("asset.ui_set", "command-asset-ui-set", "edit", crate::ui_designer::cmd_ui_set),
        ("asset.ui_nudge", "command-asset-ui-nudge", "edit", crate::ui_designer::cmd_ui_nudge),
        ("asset.theme_set", "command-asset-theme-set", "edit", crate::ui_designer::cmd_theme_set),
        ("asset.theme_new", "command-asset-theme-new", "edit", crate::ui_designer::cmd_theme_new),
        ("project.input", "command-project-input", "file", crate::input_editor::cmd_project_input),
        ("project.saves", "command-project-saves", "file", crate::save_editor::cmd_project_saves),
        (
            "asset.save_delete",
            "command-asset-save-delete",
            "edit",
            crate::save_editor::cmd_save_delete,
        ),
        (
            "asset.save_version",
            "command-asset-save-version",
            "edit",
            crate::save_editor::cmd_save_version,
        ),
        (
            "asset.save_encrypt",
            "command-asset-save-encrypt",
            "edit",
            crate::save_editor::cmd_save_encrypt,
        ),
        ("asset.save_key", "command-asset-save-key", "edit", crate::save_editor::cmd_save_key),
        (
            "asset.cloud_save",
            "command-asset-cloud-save",
            "edit",
            crate::save_editor::cmd_cloud_save,
        ),
        (
            "asset.cloud_provider",
            "command-asset-cloud-provider",
            "edit",
            crate::save_editor::cmd_cloud_provider,
        ),
        ("asset.cloud_url", "command-asset-cloud-url", "edit", crate::save_editor::cmd_cloud_url),
        (
            "asset.input_capture",
            "command-asset-input-capture",
            "edit",
            crate::input_editor::cmd_input_capture,
        ),
        (
            "asset.input_cancel",
            "command-asset-input-cancel",
            "edit",
            crate::input_editor::cmd_input_cancel,
        ),
        (
            "asset.input_bind",
            "command-asset-input-bind",
            "edit",
            crate::input_editor::cmd_input_bind,
        ),
        (
            "asset.input_unbind",
            "command-asset-input-unbind",
            "edit",
            crate::input_editor::cmd_input_unbind,
        ),
        ("asset.input_add", "command-asset-input-add", "edit", crate::input_editor::cmd_input_add),
        (
            "asset.input_remove",
            "command-asset-input-remove",
            "edit",
            crate::input_editor::cmd_input_remove,
        ),
        (
            "asset.input_deadzone",
            "command-asset-input-deadzone",
            "edit",
            crate::input_editor::cmd_input_deadzone,
        ),
        (
            "asset.input_modifier",
            "command-asset-input-modifier",
            "edit",
            crate::input_editor::cmd_input_modifier,
        ),
        (
            "asset.input_bind_axis",
            "command-asset-input-bind-axis",
            "edit",
            crate::input_editor::cmd_input_bind_axis,
        ),
        ("asset.text_save", "command-asset-text-save", "edit", crate::asset_editor::cmd_text_save),
        (
            "asset.sound_play",
            "command-asset-sound-play",
            "tools",
            crate::asset_editor::cmd_sound_play,
        ),
        (
            "asset.sound_stop",
            "command-asset-sound-stop",
            "tools",
            crate::asset_editor::cmd_sound_stop,
        ),
        ("prefab.done", "command-prefab-done", "edit", prefab_done),
        ("prefab.apply", "command-prefab-apply", "edit", prefab_apply),
        ("prefab.revert", "command-prefab-revert", "edit", prefab_revert),
        ("prefab.refresh", "command-prefab-refresh", "edit", prefab_refresh),
        ("edit.select-children", "command-edit-select-children", "edit", select_children),
        ("viewport.play-from-selection", "command-viewport-play-from", "view", play_from_selection),
        ("viewport.go-here", "command-viewport-go-here", "view", viewport_go_here),
        ("world.time-scale", "command-world-time", "edit", set_time_scale),
        ("world.gravity", "command-world-gravity", "edit", set_gravity),
        ("world.join", "command-world-join", "edit", set_join),
        ("world.music", "command-world-music", "edit", set_music),
        ("world.sky", "command-world-sky", "edit", set_sky),
        ("world.glow", "command-world-glow", "edit", set_glow),
        ("physics.add_world", "command-physics-add-world", "edit", crate::physics_cmd::add_world),
        (
            "physics.remove_world",
            "command-physics-remove-world",
            "edit",
            crate::physics_cmd::remove_world,
        ),
        ("physics.world", "command-physics-world", "edit", crate::physics_cmd::set_world),
        ("physics.member", "command-physics-member", "edit", crate::physics_cmd::set_member),
        ("physics.collider", "command-physics-collider", "edit", crate::physics_cmd::set_collider),
        ("light2d.edit", "command-light2d-edit", "edit", crate::light2d_cmd::edit_cmd),
        ("settings.set", "command-settings-set", "tools", set_setting_cmd),
        ("settings.reset", "command-settings-reset", "tools", reset_setting_cmd),
        ("settings.filter", "command-settings-filter", "tools", filter_settings),
        ("settings.quality", "command-settings-quality", "tools", set_quality_level_cmd),
        (
            "settings.platform.override",
            "command-settings-platform-override",
            "tools",
            set_platform_override_cmd,
        ),
        (
            "settings.preview_platform",
            "command-settings-preview-platform",
            "tools",
            set_preview_platform_cmd,
        ),
        (
            "keymap.profile.save",
            "command-keymap-profile-save",
            "tools",
            crate::keymap_profiles::cmd_profile_save,
        ),
        (
            "keymap.profile.switch",
            "command-keymap-profile-switch",
            "tools",
            crate::keymap_profiles::cmd_profile_switch,
        ),
        (
            "keymap.profile.export",
            "command-keymap-profile-export",
            "tools",
            crate::keymap_profiles::cmd_profile_export,
        ),
        (
            "keymap.profile.import",
            "command-keymap-profile-import",
            "tools",
            crate::keymap_profiles::cmd_profile_import,
        ),
        (
            "keymap.conflict.fix",
            "command-keymap-conflict-fix",
            "tools",
            crate::keymap_profiles::cmd_conflict_fix,
        ),
        (
            "asset.open_external",
            "command-asset-open-external",
            "edit",
            crate::keymap_profiles::cmd_open_external_editor,
        ),
        (
            "asset.compare_saved",
            "command-asset-compare-saved",
            "edit",
            crate::keymap_profiles::cmd_compare_with_saved,
        ),
        ("view.focus-next", "command-focus-next", "view", focus_next),
        ("view.focus-previous", "command-focus-previous", "view", focus_previous),
    ];
    for (id, title_key, category, handler) in specs
        .iter()
        .chain(crate::asset_browser::COMMANDS)
        .chain(crate::viewport_split::COMMANDS)
        .chain(crate::import_presets::COMMANDS)
        .chain(crate::sequences::COMMANDS)
        .chain(crate::pickers::COMMANDS)
        .chain(crate::shape_ops::COMMANDS)
        .chain(crate::asset_database_ops::COMMANDS)
        .chain(crate::scene_pack::COMMANDS)
        .chain(crate::prefab_data_cmds::COMMANDS)
        .chain(crate::import_pipeline::COMMANDS)
        .chain(crate::functional_tests::COMMANDS)
        .chain(crate::composite_texture_cmds::COMMANDS)
        .chain(crate::layered_picture::COMMANDS)
        .chain(crate::tutorial_hooks::COMMANDS)
    {
        let mut spec = CommandSpec::new(*id, *title_key, *category);
        if matches!(*id, "console.select_row" | "viewport.play-from-selection")
            || id.starts_with("search.")
            || id.starts_with("picker.")
            || matches!(*id, "inspector.set_colour" | "inspector.record_edit" | "graph.var_record")
        {
            spec = spec.menu(false);
        }
        session.commands.register(spec, always_enabled, *handler)?;
    }
    Ok(())
}

type CommandFn = fn(&mut EditorSession, &CommandArgs) -> Result<(), CoreError>;

fn toggle_node(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let id = required(args, "id")?;
    session.models.toggle(&id);
    Ok(())
}

fn expand_outliner_ancestors(session: &mut EditorSession, id: &str) {
    if id == "root" {
        return;
    }
    let mut parent =
        session.models.nodes.iter().find(|node| node.id == id).and_then(|node| node.parent.clone());
    while let Some(pid) = parent {
        if !session.models.expanded.iter().any(|item| item == &pid) {
            session.models.expanded.push(pid.clone());
        }
        parent = session
            .models
            .nodes
            .iter()
            .find(|node| node.id == pid)
            .and_then(|node| node.parent.clone());
    }
}

fn queue_outliner_reveal(session: &mut EditorSession, id: &str) {
    if id == "root" {
        return;
    }
    expand_outliner_ancestors(session, id);
    session.models.outliner_scroll_to = Some(id.to_string());
}

fn select_node(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    session.foundation.selection.clear();
    session.models.selection = args.get("id").map(str::to_string);
    if let Some(id) = args.get("id") {
        session.foundation.selection.push(id.to_string());
        crate::pickers::complete_eyedropper(session, id);
        if args.get("reveal") == Some("1") {
            queue_outliner_reveal(session, id);
        }
    }
    Ok(())
}

fn select_set(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let ids = required(args, "ids")?;
    let picked: Vec<String> =
        ids.split(',').filter(|id| !id.is_empty()).map(str::to_string).collect();
    session.foundation.selection = picked.clone();
    session.models.selection = picked.first().cloned();
    if args.get("reveal") == Some("1") {
        if let Some(id) = session.models.selection.clone() {
            queue_outliner_reveal(session, &id);
        }
    }
    Ok(())
}

fn select_extend(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let Some(id) = args.get("id") else {
        return Ok(());
    };
    if let Some(index) = session.foundation.selection.iter().position(|item| item == id) {
        session.foundation.selection.remove(index);
    } else {
        session.foundation.selection.push(id.to_string());
    }
    session.models.selection = Some(id.to_string());
    Ok(())
}

fn range_select(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let id = required(args, "id")?;
    let visible = session.models.visible_ids();
    let Some(to_index) = visible.iter().position(|item| item == &id) else {
        return Err(CoreError::new("unknown mock node"));
    };
    let Some(anchor) = session.models.selection.clone() else {
        return Err(CoreError::new("nothing selected"));
    };
    let Some(from_index) = visible.iter().position(|item| item == &anchor) else {
        return Err(CoreError::new("unknown mock node"));
    };
    let (start, end) =
        if from_index <= to_index { (from_index, to_index) } else { (to_index, from_index) };
    session.foundation.selection = visible[start..=end].to_vec();
    session.models.selection = Some(id);
    Ok(())
}

fn rename_node(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let id = required(args, "id")?;
    let name = required(args, "name")?;
    if name.trim().is_empty() {
        return Err(CoreError::new(session.messages.get("outliner-rename-empty")));
    }
    let Some(previous) = session.models.rename(&id, &name) else {
        return Err(CoreError::new("unknown mock node"));
    };
    session.push_past(Past::Rename { id: id.clone(), previous, next: name });
    session.future.clear();
    session.mirror_node(&id);
    session.clear_draft(&format!("rename:{id}"))
}

fn reparent_node(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let id = required(args, "id")?;
    let parent = args.get("parent").filter(|parent| !parent.is_empty());
    let previous = session.models.reparent(&id, parent)?;
    session.mirror_node(&id);
    session.adopt_world();
    session.push_past(Past::Reparent { id, previous, next: parent.map(str::to_string) });
    session.future.clear();
    session.mark_scene();
    Ok(())
}

fn reorder_outliner(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let id = required(args, "id")?;
    let before = args.get("before").filter(|value| !value.is_empty());
    let after = args.get("after").filter(|value| !value.is_empty());
    let previous = session.world.to_text();
    session.models.reorder_sibling(&id, before, after)?;
    session.adopt_world();
    session.mirror_node(&id);
    push_scene_past(session, previous);
    session.mark_scene();
    Ok(())
}

pub fn outliner_drop_verdict(
    models: &EditorModels,
    messages: &crate::Messages,
    payload: &str,
    target_id: &str,
    mode: OutlinerDropMode,
) -> (bool, Option<String>) {
    if target_id == "root" && matches!(mode, OutlinerDropMode::Before | OutlinerDropMode::After) {
        return (false, Some(messages.get("outliner-drop-no-reorder-root")));
    }
    if let Some(kind) = payload.strip_prefix("place:") {
        let _ = kind;
        return (true, None);
    }
    let child = payload;
    if child == "root" {
        return (false, Some(messages.get("outliner-drop-root")));
    }
    if child == target_id {
        return (false, Some(messages.get("outliner-drop-self")));
    }
    match mode {
        OutlinerDropMode::Parent => {
            if models.outliner_reparent_allowed(child, target_id) {
                (true, None)
            } else {
                (false, Some(messages.get("outliner-drop-cycle")))
            }
        }
        OutlinerDropMode::Before | OutlinerDropMode::After => {
            let parent = models
                .nodes
                .iter()
                .find(|node| node.id == target_id)
                .and_then(|node| node.parent.as_deref())
                .unwrap_or("root");
            if models.outliner_reparent_allowed(child, parent) {
                (true, None)
            } else {
                (false, Some(messages.get("outliner-drop-cycle")))
            }
        }
    }
}

fn sync_inspector_field_past(session: &mut EditorSession, field: &str, value: &str, id: &str) {
    let from = session
        .models
        .inspector_field_edit
        .as_ref()
        .filter(|edit| edit.field == field)
        .map(|edit| edit.past_from)
        .unwrap_or(session.past.len());
    for entry in session.past.iter_mut().skip(from) {
        if let Past::Field { id: past_id, field: past_field, next, .. } = entry {
            if past_field == field && past_id == id {
                *next = value.to_string();
            }
        }
    }
}

fn list_edit_field(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let field = required(args, "field")?;
    let spec = crate::list_field::list_spec_for_field(&field)
        .ok_or_else(|| CoreError::new("not a list field"))?;
    let op_name = required(args, "op")?;
    let index = args.get("index").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
    let to = args.get("to").and_then(|v| v.parse::<usize>().ok()).unwrap_or(index);
    let targets = field_targets(args)?;
    let mut touched = false;
    for id in &targets {
        let Some(node) = session.models.nodes.iter().find(|n| n.id == *id) else {
            continue;
        };
        let current =
            node.fields.iter().find(|(k, _)| k == &field).map(|(_, v)| v.as_str()).unwrap_or("");
        let next = if op_name == "set" {
            let value = required(args, "value")?;
            crate::list_field::set_list_element(spec, current, index, &value)
        } else {
            let edit = crate::list_field::parse_list_edit_op(&op_name, to)
                .ok_or_else(|| CoreError::new("unknown list operation"))?;
            crate::list_field::apply_list_edit(spec, current, index, edit)
        };
        let Some(value) = next else {
            return Err(CoreError::new("list edit could not apply"));
        };
        let Some(previous) = session.models.set_field(id, &field, &value) else {
            continue;
        };
        session.push_past(Past::Field {
            id: id.clone(),
            field: field.clone(),
            previous,
            next: value.clone(),
        });
        session.mirror_node(id);
        note_prefab_override(session, id, &field);
        touched = true;
    }
    if !touched {
        return Err(CoreError::new("unknown mock node"));
    }
    session.future.clear();
    Ok(())
}

fn set_field(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let field = required(args, "field")?;
    let value = required(args, "value")?.to_string();
    let preview = args.get("preview") == Some("1");
    let targets = field_targets(args)?;
    let mut touched = false;
    for id in targets {
        let Some(previous) = session.models.set_field(&id, &field, &value) else {
            continue;
        };
        if preview || session.models.inspector_field_edit.is_some() {
            sync_inspector_field_past(session, &field, &value, &id);
        } else {
            session.push_past(Past::Field {
                id: id.clone(),
                field: field.clone(),
                previous,
                next: value.clone(),
            });
        }
        if session.models.inspector_scale_uniform && matches!(field.as_str(), "sx" | "sy" | "sz") {
            for extra in ["sx", "sy", "sz"] {
                if extra != field {
                    let _ = session.models.set_field(&id, extra, &value);
                }
            }
        }
        session.mirror_node(&id);
        note_prefab_override(session, &id, &field);
        touched = true;
    }
    if !touched {
        return Err(CoreError::new("unknown mock node"));
    }
    if !preview {
        session.future.clear();
    }
    if !preview
        && session.models.inspector_field_edit.is_none()
        && (seal_world::sprite_mask::KEYS.contains(&field.as_str())
            || seal_world::volumes::KEYS.contains(&field.as_str())
            || field == "script_order"
            || field == "script_group"
            || field.starts_with("remote_")
            || matches!(
                field.as_str(),
                "light2d"
                    | "light_blend"
                    | "light_falloff"
                    | "light_points"
                    | "light_angle"
                    | "mat_blend"
                    | "mat_light"
            ))
    {
        session.adopt_world();
    }
    Ok(())
}

/// A field changed on a prefab copy is remembered so rebuilding from the file keeps it.
/// Placement, visibility and lock always stay with the copy, so they are not listed.
fn note_prefab_override(session: &mut EditorSession, id: &str, field: &str) {
    const OWN: &[&str] = &["x", "y", "z", "rx", "ry", "rz", "sx", "sy", "sz", "hidden", "locked"];
    if OWN.contains(&field) {
        return;
    }
    let Some(entity) = session.world.entity_mut(id) else { return };
    if entity.source_kind != "prefab" {
        return;
    }
    if !entity.overrides.split(',').any(|f| f.trim() == field) {
        if !entity.overrides.is_empty() {
            entity.overrides.push(',');
        }
        entity.overrides.push_str(field);
    }
}

/// The prefab copy named by `id=` or the selection, with its prefab file.
fn prefab_copy(
    session: &EditorSession,
    args: &CommandArgs,
) -> Result<(String, std::path::PathBuf), CoreError> {
    let id = match args.get("id").filter(|v| !v.is_empty()) {
        Some(id) => id.to_string(),
        None => session
            .models
            .selection
            .clone()
            .ok_or_else(|| CoreError::new("select a prefab copy first"))?,
    };
    let e =
        session.world.entity(&id).ok_or_else(|| CoreError::new("select a prefab copy first"))?;
    if e.source_kind != "prefab" || e.source_asset.is_empty() {
        return Err(CoreError::new("the selected object is not a prefab copy"));
    }
    let file = project_root(session)?.join("prefabs").join(format!("{}.prefab", e.source_asset));
    Ok((id, file))
}

/// Write the copy's own changes into its prefab file, then update every copy.
fn prefab_apply(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let (id, file) = prefab_copy(session, args)?;
    let mut scratch =
        seal_world::World::from_text(&session.world.to_text()).map_err(CoreError::new)?;
    if let Some(e) = scratch.entity_mut(&id) {
        e.source_kind.clear();
        e.source_asset.clear();
        e.overrides.clear();
    }
    let text = scratch.prefab_text(&id).ok_or_else(|| CoreError::new("nothing to apply"))?;
    crate::persist::atomic_write(&file, text.as_bytes())?;
    let previous = session.world.to_text();
    if let Some(e) = session.world.entity_mut(&id) {
        e.overrides.clear();
    }
    session.refresh_prefabs();
    session.adopt_world();
    push_scene_past(session, previous);
    session.mark_scene();
    session.notify("Applied the changes to the prefab".to_string(), false);
    Ok(())
}

/// Drop the copy's own changes (all, or just `field=`) and take the prefab's values again.
fn prefab_revert(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let (id, _) = prefab_copy(session, args)?;
    let previous = session.world.to_text();
    if let Some(e) = session.world.entity_mut(&id) {
        e.overrides = match args.get("field").filter(|f| !f.is_empty()) {
            Some(field) => e
                .overrides
                .split(',')
                .map(str::trim)
                .filter(|f| !f.is_empty() && *f != field)
                .collect::<Vec<_>>()
                .join(","),
            None => String::new(),
        };
    }
    session.refresh_prefabs();
    session.adopt_world();
    if session.world.to_text() != previous {
        push_scene_past(session, previous);
        session.mark_scene();
    }
    Ok(())
}

fn prefab_refresh(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let previous = session.world.to_text();
    let count = session.refresh_prefabs();
    session.adopt_world();
    if session.world.to_text() != previous {
        push_scene_past(session, previous);
        session.mark_scene();
    }
    session.notify(format!("Updated {count} prefab copies"), false);
    Ok(())
}

fn begin_inspector_field_edit(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let field = required(args, "field")?;
    let targets = field_targets(args)?;
    let past_from = session.past.len();
    for id in &targets {
        let Some(node) = session.models.nodes.iter().find(|n| &n.id == id) else {
            continue;
        };
        let previous = node
            .fields
            .iter()
            .find(|(key, _)| key == &field)
            .map(|(_, value)| value.clone())
            .unwrap_or_default();
        session.push_past(Past::Field {
            id: id.clone(),
            field: field.clone(),
            previous: previous.clone(),
            next: previous,
        });
    }
    if past_from == session.past.len() {
        return Err(CoreError::new("unknown mock node"));
    }
    session.models.inspector_field_edit =
        Some(InspectorFieldEditSession { field: field.clone(), past_from });
    session.future.clear();
    Ok(())
}

fn end_inspector_field_edit(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let field = required(args, "field")?;
    let value = required(args, "value")?.to_string();
    let targets = field_targets(args)?;
    let Some(edit) = session.models.inspector_field_edit.clone() else {
        return set_field(session, args);
    };
    if edit.field != field {
        return Err(CoreError::new("inspector field edit mismatch"));
    }
    for id in targets {
        let _ = session.models.set_field(&id, &field, &value);
        sync_inspector_field_past(session, &field, &value, &id);
        session.mirror_node(&id);
    }
    session.models.inspector_field_edit = None;
    session.future.clear();
    Ok(())
}

fn set_inspector_vector_space(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let space = required(args, "space")?;
    if space != "absolute" && space != "relative" {
        return Err(CoreError::new("unknown vector space"));
    }
    session.models.inspector_location_space = space.to_string();
    session.future.clear();
    Ok(())
}

fn set_inspector_scale_uniform(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    session.models.inspector_scale_uniform = args.get("locked") == Some("1");
    session.future.clear();
    Ok(())
}

fn cancel_inspector_field_edit(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    let Some(edit) = session.models.inspector_field_edit.take() else {
        return Ok(());
    };
    let drained: Vec<Past> = session.past.drain(edit.past_from..).collect();
    for past in drained {
        if let Past::Field { id, field, previous, .. } = past {
            let _ = session.models.set_field(&id, &field, &previous);
            session.mirror_node(&id);
        }
    }
    session.future.clear();
    Ok(())
}

fn filter_console(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    if let Some(level) = args.get("level") {
        session.models.console_level = level.to_string();
    }
    if let Some(query) = args.get("query") {
        if query != session.models.console_query.as_str() {
            session.models.console_match_index = 0;
        }
        session.models.console_query = query.to_string();
    }
    if let Some(flag) = args.get("regex") {
        session.models.console_regex = flag == "1" || flag.eq_ignore_ascii_case("true");
    }
    Ok(())
}

fn toggle_console_regex(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.models.console_regex = !session.models.console_regex;
    session.models.console_match_index = 0;
    Ok(())
}

fn select_console_row(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let max_entries = session
        .foundation
        .settings
        .get("console.max_entries")
        .and_then(|text| text.parse().ok())
        .unwrap_or(5000);
    let records = collect_log_lines(session.foundation.console_clear_before);
    let total = session.models.filtered_logs(&records, max_entries).len();
    if total == 0 {
        return Ok(());
    }
    let row = if let Some(index) = args.get("index").and_then(|text| text.parse::<usize>().ok()) {
        index.min(total.saturating_sub(1))
    } else if args.get("direction") == Some("prev") {
        session.models.console_match_index.saturating_sub(1)
    } else if args.get("direction") == Some("next") {
        (session.models.console_match_index + 1).min(total.saturating_sub(1))
    } else {
        session.models.console_match_index
    };
    session.models.console_match_index = row;
    session.models.console_line_selection = vec![row];
    Ok(())
}

fn extend_console_row(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let Some(index) = args.get("index").and_then(|text| text.parse::<usize>().ok()) else {
        return Ok(());
    };
    if let Some(place) =
        session.models.console_line_selection.iter().position(|item| *item == index)
    {
        session.models.console_line_selection.remove(place);
    } else {
        session.models.console_line_selection.push(index);
    }
    session.models.console_match_index = index;
    Ok(())
}

fn toggle_console_fold(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let Some(index) = args.get("index").and_then(|text| text.parse::<usize>().ok()) else {
        return Ok(());
    };
    if let Some(place) = session.models.console_folded.iter().position(|item| *item == index) {
        session.models.console_folded.remove(place);
    } else {
        session.models.console_folded.push(index);
    }
    Ok(())
}

fn run_console_command(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let line = args.get("line").unwrap_or_default();
    if line.is_empty() {
        return Ok(());
    }
    let mut parts = line.split_whitespace();
    let Some(id) = parts.next() else {
        return Ok(());
    };
    // Echo every line so the Console shows what ran, like a shell transcript.
    seal_log::info!("> {line}");
    if id == "help" || id == "?" {
        let filter = parts.next().unwrap_or("").to_lowercase();
        let mut ids: Vec<(String, String)> = session
            .commands
            .iter()
            .filter(|spec| spec.alias_of.is_none())
            .filter(|spec| filter.is_empty() || spec.id.to_lowercase().contains(&filter))
            .map(|spec| (spec.id.clone(), session.messages.get(&spec.title_key)))
            .collect();
        ids.sort();
        seal_log::info!(
            "{} commands. Type a command id, then key=value pairs. Filter with: help <word>",
            ids.len()
        );
        for (id, title) in ids {
            seal_log::info!("  {id}  {title}");
        }
        return Ok(());
    }
    let pairs: Vec<(&str, &str)> = parts
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (key, value)
        })
        .collect();
    let result = session.dispatch(id, &CommandArgs::new(&pairs));
    if let Err(err) = &result {
        seal_log::warn!("{id}: {err}");
    }
    result
}

fn focus_console_level(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    if let Some(level) = args.get("level") {
        session.models.console_level = level.to_string();
    }
    session.focus_panel("console");
    session.reveal_panel = Some("console".into());
    Ok(())
}

fn jump_console_match(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let direction = args.get("direction").unwrap_or("next");
    let max_entries = session
        .foundation
        .settings
        .get("console.max_entries")
        .and_then(|text| text.parse().ok())
        .unwrap_or(5000);
    let records = collect_log_lines(session.foundation.console_clear_before);
    let lines = session.models.filtered_logs(&records, max_entries);
    let matches: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| session.models.console_query_matches(line))
        .map(|(index, _)| index)
        .collect();
    if matches.is_empty() {
        session.notify(session.messages.get("console-jump-none"), false);
        return Ok(());
    }
    let current = session.models.console_match_index;
    let pos = matches.iter().position(|&index| index == current);
    let next = match (direction, pos) {
        ("prev", Some(place)) => matches[(place + matches.len() - 1) % matches.len()],
        ("prev", None) => matches[matches.len() - 1],
        (_, Some(place)) => matches[(place + 1) % matches.len()],
        (_, None) => matches[0],
    };
    let human = matches.iter().position(|&index| index == next).unwrap_or(0) + 1;
    session.models.console_match_index = next;
    session.notify(
        session.messages.format(
            "console-jump-match",
            &[("index", &human.to_string()), ("total", &matches.len().to_string())],
        ),
        false,
    );
    Ok(())
}

pub(crate) fn outliner_shown_ids(session: &EditorSession) -> Vec<String> {
    let query = session.models.outliner_query.trim();
    if query.is_empty() {
        session.models.visible_ids()
    } else {
        session
            .models
            .nodes
            .iter()
            .filter(|node| crate::outliner_query::outliner_matches(node, query))
            .map(|node| node.id.clone())
            .collect()
    }
}

fn jump_outliner_filter_next(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    jump_outliner_match(session, "next")
}

fn jump_outliner_filter_prev(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    jump_outliner_match(session, "prev")
}

fn jump_outliner_match(session: &mut EditorSession, direction: &str) -> Result<(), CoreError> {
    let matches = outliner_shown_ids(session);
    if matches.is_empty() {
        return Ok(());
    }
    let current = session.models.selection.as_deref();
    let index = current.and_then(|id| matches.iter().position(|item| item == id)).unwrap_or(0);
    let next = if direction == "prev" {
        if index == 0 {
            matches.len() - 1
        } else {
            index - 1
        }
    } else if index + 1 >= matches.len() {
        0
    } else {
        index + 1
    };
    let pick = matches[next].clone();
    session.foundation.selection.clear();
    session.models.selection = Some(pick);
    Ok(())
}

fn sort_outliner(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let column = args.get("column").unwrap_or("label");
    let table = &mut session.models.outliner_table;
    if table.sort_column == column {
        table.sort_asc = !table.sort_asc;
    } else {
        table.sort_column = column.to_string();
        table.sort_asc = true;
    }
    Ok(())
}

fn toggle_outliner_column(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let id = required(args, "id")?;
    let on = args.get("on") != Some("0");
    let cols = &mut session.models.outliner_table.visible_optional;
    if on {
        if !cols.iter().any(|col| col == &id) {
            cols.push(id.clone());
            cols.sort();
        }
    } else {
        cols.retain(|col| col != &id);
    }
    Ok(())
}

fn resize_outliner_type(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let width: f32 = required(args, "width")?
        .parse()
        .map_err(|_| CoreError::new("column width must be a number"))?;
    session.models.outliner_table.type_width = width.clamp(48.0, 280.0);
    Ok(())
}

fn toggle_outliner_filter(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let id = required(args, "id")?;
    let on = args.get("on").unwrap_or("1") == "1";
    let filters = &mut session.models.outliner_filters;
    if let Some(ty) = id.strip_prefix("type:") {
        if on {
            filters.types.insert(ty.to_string());
        } else {
            filters.types.remove(ty);
        }
        return Ok(());
    }
    if let Some(flag) = id.strip_prefix("flag:") {
        match flag {
            "only_selected" => filters.only_selected = on,
            "only_visible" => filters.only_visible = on,
            "only_unsaved" => filters.only_unsaved = on,
            "only_warnings" => filters.only_warnings = on,
            other => return Err(CoreError::new(format!("unknown outliner filter flag: {other}"))),
        }
        return Ok(());
    }
    Err(CoreError::new("filter id must start with type: or flag:"))
}

fn set_outliner_view_option(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let id = required(args, "id")?;
    let on = args.get("on").unwrap_or("1") == "1";
    let view = &mut session.models.outliner_view;
    match id.as_str() {
        "hierarchical" => view.hierarchical = on,
        "only_selected" => view.only_selected = on,
        "hide_temporary" => view.hide_temporary = on,
        "only_current_level" => view.only_current_level = on,
        "hide_folders" => view.hide_folders = on,
        "stack_headers" => view.stack_headers = on,
        other => return Err(CoreError::new(format!("unknown outliner view option: {other}"))),
    }
    Ok(())
}

fn new_outliner_folder(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let parent = session
        .models
        .selection
        .clone()
        .filter(|id| id != "root")
        .unwrap_or_else(|| "root".to_string());
    let folder_name = session.messages.get("outliner-new-folder-name");
    let previous = session.world.to_text();
    let id = session.world.add("empty");
    if let Some(entity) = session.world.entity_mut(&id) {
        entity.name = folder_name;
        entity.tag = "folder".into();
        entity.parent = Some(parent.clone());
    }
    session.adopt_world();
    session.models.selection = Some(id.clone());
    session.models.expanded.push(parent);
    push_scene_past(session, previous);
    session.mark_scene();
    Ok(())
}

fn typeahead_outliner(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let prefix = args.get("prefix").unwrap_or("").to_lowercase();
    session.models.outliner_typeahead = prefix.clone();
    if prefix.is_empty() {
        return Ok(());
    }
    let matches: Vec<String> = outliner_shown_ids(session)
        .into_iter()
        .filter(|id| {
            session
                .models
                .nodes
                .iter()
                .find(|node| &node.id == id)
                .is_some_and(|node| node.name.to_lowercase().starts_with(&prefix))
        })
        .collect();
    if matches.is_empty() {
        session.notify(session.messages.get("outliner-typeahead-none"), false);
        return Ok(());
    }
    let pick = if args.get("advance") == Some("1") {
        let current = session.models.selection.as_deref();
        let place = current.and_then(|id| matches.iter().position(|item| item == id)).unwrap_or(0);
        matches[(place + 1) % matches.len()].clone()
    } else {
        matches[0].clone()
    };
    session.foundation.selection.clear();
    session.foundation.selection.push(pick.clone());
    session.models.selection = Some(pick);
    Ok(())
}

const INSPECTOR_SECTION_IDS: &[&str] = &[
    "inspector-transform",
    "inspector-material",
    "inspector-light",
    "inspector-camera",
    "inspector-color",
    "inspector-shape",
    "inspector-physics",
    "inspector-tags",
    "inspector-properties",
];

pub fn inspector_section_match_keys(id: &str) -> &'static [&'static str] {
    match id {
        "inspector-transform" => &[
            "inspector-transform",
            "inspector-location",
            "inspector-rotation",
            "inspector-scale",
            "x",
            "y",
            "z",
            "rx",
            "ry",
            "rz",
            "sx",
            "sy",
            "sz",
        ],
        "inspector-material" => {
            &["inspector-material", "inspector-roughness", "inspector-metal", "roughness", "metal"]
        }
        "inspector-light" => &["inspector-light", "inspector-intensity", "intensity"],
        "inspector-camera" => &[
            "inspector-camera",
            "inspector-fov",
            "inspector-near",
            "inspector-far",
            "fov",
            "near",
            "far",
        ],
        "inspector-color" => &["inspector-color", "r", "g", "b"],
        "inspector-shape" => &["inspector-shape", "shape"],
        "inspector-physics" => &["inspector-physics", "inspector-motion", "motion"],
        "inspector-tags" => &["inspector-tags", "inspector-role", "role"],
        "inspector-properties" => {
            &["inspector-properties", "inspector-id", "inspector-parent", "id", "parent"]
        }
        _ => &[],
    }
}

pub fn inspector_open_sections_for_query(session: &mut EditorSession) {
    let q = session.models.inspector_query.to_lowercase();
    if q.is_empty() {
        return;
    }
    for id in INSPECTOR_SECTION_IDS {
        let keys = inspector_section_match_keys(id);
        let hit = keys.iter().any(|key| {
            let lower = key.to_lowercase();
            lower.contains(&q) || session.messages.get(key).to_lowercase().contains(&q)
        });
        if hit {
            session.models.inspector_sections.insert(id.to_string(), true);
        }
    }
}

fn filter_inspector(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let query = args.get("query").unwrap_or("").to_string();
    let panel = session.foundation.focused_panel_id.clone();
    session.set_inspector_query_for(&panel, query);
    inspector_open_sections_for_query(session);
    Ok(())
}

fn set_inspector_view_option(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let id = required(args, "id")?;
    let on = args.get("on") == Some("1");
    match id.as_str() {
        "favourites_only" => session.models.inspector_view.favourites_only = on,
        "only_modified" => session.models.inspector_view.only_modified = on,
        "show_advanced" => session.models.inspector_view.show_advanced = on,
        "show_hidden_playing" => session.models.inspector_view.show_hidden_playing = on,
        _ => return Err(CoreError::new("unknown inspector view option")),
    }
    Ok(())
}

fn set_inspector_view_mode(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let list = args.get("mode") != Some("grid");
    session.models.inspector_view.list_view = list;
    Ok(())
}

fn set_inspector_category(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    session.models.inspector_category =
        args.get("id").filter(|id| !id.is_empty()).unwrap_or("all").to_string();
    Ok(())
}

fn expand_inspector_sections(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    for id in INSPECTOR_SECTION_IDS {
        session.models.inspector_sections.insert(id.to_string(), true);
    }
    Ok(())
}

fn collapse_inspector_sections(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    for id in INSPECTOR_SECTION_IDS {
        session.models.inspector_sections.insert(id.to_string(), false);
    }
    Ok(())
}

fn pick_inspector_component(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let panel = args
        .get("panel")
        .map(str::to_string)
        .unwrap_or_else(|| session.foundation.focused_panel_id.clone());
    if crate::panel::panel_base(&panel) != "inspector" {
        return Err(CoreError::new("focus an inspector panel"));
    }
    let component = args.get("component").unwrap_or("").to_string();
    if component.is_empty() {
        session.models.inspector_component_pick.remove(&panel);
    } else {
        let target =
            args.get("target").map(str::to_string).or_else(|| session.models.selection.clone());
        session.models.inspector_component_pick.insert(panel, (component, target));
    }
    Ok(())
}

impl EditorModels {
    /// The component picked in this inspector, only while it still shows the object it was
    /// picked on; another object shows every section again.
    pub fn component_pick(&self, panel: &str, shown: &str) -> &str {
        match self.inspector_component_pick.get(panel) {
            Some((component, on)) if on.as_deref() == Some(shown) => component,
            _ => "",
        }
    }
}

#[cfg(test)]
mod component_pick_tests {
    #[test]
    fn a_pick_ends_when_another_object_is_shown() {
        let mut session = crate::EditorSession::new(crate::Layout::default());
        let args = crate::CommandArgs::new(&[
            ("panel", "inspector"),
            ("component", "transform"),
            ("target", "a"),
        ]);
        session.dispatch("inspector.pick_component", &args).unwrap();
        assert_eq!(session.models.component_pick("inspector", "a"), "transform");
        assert_eq!(session.models.component_pick("inspector", "b"), "");
    }
}

fn stub_inspector_component(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    session.notice = Some(session.messages.get("inspector-component-stub"));
    Ok(())
}

fn reattach_inspector_component(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let _from = required(args, "from")?;
    let _to = args.get("to").unwrap_or("root");
    let _entity = required(args, "entity")?;
    session.notice = Some(session.messages.get("inspector-component-reattach-stub"));
    Ok(())
}

fn set_inspector_tree_height(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let panel = args
        .get("panel")
        .map(str::to_string)
        .unwrap_or_else(|| session.foundation.focused_panel_id.clone());
    let height = args
        .get("height")
        .and_then(|text| text.parse::<f32>().ok())
        .unwrap_or(96.0)
        .clamp(48.0, 240.0);
    session.models.inspector_tree_heights.insert(panel, height);
    Ok(())
}

fn toggle_inspector_section(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let id = required(args, "id")?;
    let open = args.get("open") == Some("1");
    session.models.inspector_sections.insert(id, open);
    Ok(())
}

fn toggle_inspector_advanced_section(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let id = required(args, "id")?;
    let open = args.get("open") == Some("1");
    session.models.inspector_advanced_sections.insert(id, open);
    Ok(())
}

fn stub_inspector_property(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    session.notice = Some(session.messages.get("inspector-property-stub"));
    Ok(())
}

fn reset_inspector_property(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let field = required(args, "field")?;
    let mut owned: Vec<(String, String)> = vec![("field".into(), field.to_string())];
    if let Some(id) = args.get("id") {
        owned.push(("id".into(), id.to_string()));
    }
    if let Some(ids) = args.get("ids") {
        owned.push(("ids".into(), ids.to_string()));
    }
    let pairs: Vec<(&str, &str)> = owned.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    reset_inspector_field(session, &CommandArgs::new(&pairs))
}

fn field_targets(args: &CommandArgs) -> Result<Vec<String>, CoreError> {
    if let Some(list) = args.get("ids") {
        let picked: Vec<String> =
            list.split(',').filter(|id| !id.is_empty()).map(str::to_string).collect();
        if picked.is_empty() {
            return Err(CoreError::new("no targets given"));
        }
        return Ok(picked);
    }
    Ok(vec![required(args, "id")?.to_string()])
}

fn camera_draft_f32(session: &EditorSession, key: &str, default: f32) -> f32 {
    session.draft(key).and_then(|text| text.parse().ok()).unwrap_or(default)
}

fn viewport_panel_id(args: &CommandArgs) -> &str {
    match args.get("panel") {
        Some(text) => text,
        None => "viewport",
    }
}

fn set_viewport_camera(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    if let Some(index) = pane_index_arg(args) {
        return crate::viewport_split::set_pane_camera(session, index, args);
    }
    let panel = viewport_panel_id(args);
    if panel == "viewport" {
        if let Some(text) = args.get("pan_x") {
            session.models.viewport_pan_x = text.parse().unwrap_or(session.models.viewport_pan_x);
        }
        if let Some(text) = args.get("pan_y") {
            session.models.viewport_pan_y = text.parse().unwrap_or(session.models.viewport_pan_y);
        }
        if let Some(text) = args.get("zoom") {
            session.models.viewport_zoom =
                text.parse::<f32>().unwrap_or(session.models.viewport_zoom).clamp(0.25, 6.0);
        }
        if let Some(text) = args.get("yaw") {
            session.set_draft("viewport-yaw", text)?;
        }
        if let Some(text) = args.get("pitch") {
            session.set_draft("viewport-pitch", text)?;
        }
        if let Some(text) = args.get("distance") {
            session.set_draft("viewport-distance", text)?;
        }
        if session.models.viewport_layout_preset != "single" {
            viewport_store_active_pane(session);
        }
    } else {
        ensure_viewport_tab(&mut session.models, panel);
        let tab = session
            .models
            .viewport_tabs
            .get_mut(panel)
            .ok_or_else(|| CoreError::new("viewport tab state was missing"))?;
        if let Some(text) = args.get("pan_x") {
            tab.pan_x = text.parse().unwrap_or(tab.pan_x);
        }
        if let Some(text) = args.get("pan_y") {
            tab.pan_y = text.parse().unwrap_or(tab.pan_y);
        }
        if let Some(text) = args.get("zoom") {
            tab.zoom = text.parse::<f32>().unwrap_or(tab.zoom).clamp(0.25, 6.0);
        }
        if let Some(text) = args.get("yaw") {
            tab.yaw = text.parse().unwrap_or(tab.yaw);
        }
        if let Some(text) = args.get("pitch") {
            tab.pitch = text.parse().unwrap_or(tab.pitch);
        }
        if let Some(text) = args.get("distance") {
            tab.distance = text.parse().unwrap_or(tab.distance);
        }
    }
    Ok(())
}

pub fn inspector_field_default(field: &str) -> &'static str {
    match field {
        "x" | "y" | "z" | "rx" | "ry" | "rz" => "0",
        "sx" | "sy" | "sz" => "1",
        "r" | "g" | "b" => "1",
        "roughness" | "metal" => "0.5",
        "mesh_texture" => "",
        "mesh_layer" | "mesh_slice" => "0",
        "mesh_tile_u" | "mesh_tile_v" => "1",
        "intensity" => "1",
        "fov" => "60",
        "near" => "0.1",
        "far" => "1000",
        "layer" => "0",
        "frames" => "1",
        "fps" => "8",
        "flip_x" | "flip_y" => "no",
        "tint" => "#FFFFFF",
        "opacity" => "1",
        "order" => "0",
        "slice" => "0, 0, 0, 0",
        "slice_mode" => "stretch",
        "slice_size" => "",
        "pivot" => "0.5, 0.5",
        "sort_mode" => "default",
        "parallax" => "1, 1",
        "repeat_x" => "no",
        "light_range" => "5",
        "light_cone" => "60",
        "light_shadows" => "yes",
        "light_blend" => "additive",
        "light_angle" => "0",
        "light_falloff" => "1",
        "light_points" => "",
        "camera2d" => "no",
        "cam_smoothing" => "0.15",
        "cam_zoom" => "1",
        "cam_limits" => "0, 0, 0, 0",
        "cam_pixels" => "0",
        "cam_drag_left" | "cam_drag_right" | "cam_drag_top" | "cam_drag_bottom" => "0",
        "cam_cull_mask" => "1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32",
        "audio_listener" => "no",
        "listener_max_distance" => "50",
        "listener_rolloff" => "1",
        "shape2d" => "none",
        "ss_closed" | "ss_collider" => "no",
        "ss_fill_scale" => "1, 1",
        "ss_thickness" => "0.2",
        "ss_corner" => "miter",
        "ss_layer" | "ss_order" => "0",
        "ss_tint" => "#FFFFFF",
        "ss_top" => "-40, 40",
        "ss_side" => "40, 140",
        "ss_bottom" => "140, -140",
        "ss_tolerance" => "0.05",
        "material2d" => "none",
        "mask_shape" => "none",
        "mask" => "none",
        "mask_image" => "",
        "mask_cutoff" => "0.5",
        "mask_layer_min" | "mask_order_min" => "-1000",
        "mask_layer_max" | "mask_order_max" => "1000",
        "mat_color" => "#FFFFFF",
        "mat_amount" => "1",
        "mat_speed" => "0",
        "mat_shader" => "",
        "mat_blend" => "alpha",
        "mat_light" => "lit",
        "shape_width" => "0.1",
        "shape_color" => "#FFFFFF",
        "shape_closed" => "no",
        "shape_uv_offset" => "0, 0",
        "shape_uv_scale" => "1, 1",
        "shape_uv_rotation" => "0",
        "shape_cap_begin" | "shape_cap_end" => "none",
        "follow_path" => "",
        "follow_speed" => "2",
        "follow_mode" => "loop",
        "follow_rotate" => "no",
        "follow_start" => "0",
        "nav_agent" => "no",
        "nav_speed" => "3",
        "nav_radius" => "0.4",
        "nav_avoid" => "yes",
        "char_on" => "no",
        "char_mode" => "walk",
        "char_speed" => "6",
        "char_fly_speed" => "8",
        "char_swim_speed" => "3",
        "char_climb_speed" => "3",
        "char_jump_speed" => "12",
        "char_gravity" => "24",
        "char_floor_angle" => "45",
        "char_step" => "0.4",
        "char_snap" => "0.3",
        "char_coyote" => "0.1",
        "char_buffer" => "0.15",
        "char_size" => "0.8, 1.6",
        "char_crouch_height" => "0.8",
        "char_crouch" => "no",
        "area_mode" => "none",
        "volume_kind" => "none",
        "post_glow_threshold" => "0.8",
        "post_glow_intensity" => "1",
        "post_tint" => "#FFFFFF",
        "post_exposure" => "1",
        "post_saturation" => "1",
        "post_blend" => "1",
        "post_priority" => "0",
        "volume_mode" => "respawn",
        "volume_variable" => "health",
        "volume_rate" => "10",
        "volume_event" => "",
        "volume_tag" => "",
        "script_order" => "0",
        "script_group" => "before_physics",
        "remote_on" => "no",
        "remote_target" => "",
        "remote_position" | "remote_rotation" | "remote_scale" => "yes",
        "area_mask" => "all",
        "area_priority" => "0",
        "area_force" => "acceleration",
        "area_direction" => "1, 0",
        "area_strength" => "10",
        "area_variation" => "0",
        "area_sign" => "attract",
        "area_falloff" => "none",
        "area_range" => "0",
        "area_surface" => "0",
        "area_density" => "1",
        "area_linear_drag" => "2",
        "area_angular_drag" => "0.5",
        "area_flow" => "0, 0",
        "area_gravity_mode" => "replace",
        "area_gravity" => "0, 0",
        "area_gravity_scale" => "0",
        "area_linear_damping" => "1",
        "area_angular_damping" => "0.5",
        "surface_speed" => "0",
        "phys_body" => "none",
        "phys_shape" => "box",
        "phys_size" => "1, 1",
        "phys_layer" => "1",
        "phys_mask" => "all",
        "phys_one_way" | "phys_ccd" | "phys_part" => "no",
        "phys_can_sleep" => "yes",
        "phys_lock_rotation" => "no",
        "phys_surface" => "custom",
        "phys_density" | "phys_gravity_scale" => "1",
        "phys_mass" | "phys_linear_damping" | "phys_angular_damping" | "phys_constant_torque" => "0",
        "phys_com" | "phys_constant_force" => "0, 0",
        "joint" => "none",
        "joint_anchor" | "joint_other_anchor" | "joint_limits" | "joint_motor" => "0, 0",
        "joint_axis" => "1, 0",
        "joint_length" => "0",
        "joint_stiffness" => "50",
        "joint_damping" => "2",
        "joint_break_force" | "joint_break_torque" | "joint_max_force" | "joint_max_torque" => "0",
        "joint_correction" => "1",
        "joint_offset" | "joint_target" => "0, 0",
        "shape" => "none",
        "motion" => "static",
        "motion_mode" => "dynamic",
        "role" => "",
        _ => "",
    }
}

fn reset_inspector_field(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    // `field` may name several fields separated by commas (for example `r,g,b`).
    let fields = required(args, "field")?;
    // A reset clicked mid-edit closes that edit; the reset is its own undo step.
    session.models.inspector_field_edit = None;
    let targets = field_targets(args)?;
    let mut touched = false;
    for field in fields.split(',').map(str::trim).filter(|f| !f.is_empty()) {
        let value = inspector_field_default(field).to_string();
        for id in &targets {
            let Some(previous) = session.models.set_field(id, field, &value) else {
                continue;
            };
            session.push_past(Past::Field {
                id: id.clone(),
                field: field.to_string(),
                previous,
                next: value.clone(),
            });
            session.mirror_node(id);
            touched = true;
        }
    }
    if !touched {
        return Err(CoreError::new("unknown mock node"));
    }
    session.future.clear();
    if fields.split(',').any(|field| seal_world::sprite_mask::KEYS.contains(&field.trim())) {
        session.adopt_world();
    }
    Ok(())
}

fn asset_find_references(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let rel = args
        .get("path")
        .map(str::to_string)
        .or_else(|| session.models.content_pick.clone())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| CoreError::new("pick an asset in the Asset Browser"))?;
    crate::asset_index::cmd_find_references(session, &rel);
    Ok(())
}

fn pick_content(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let picked =
        args.get("path").or(args.get("name")).filter(|name| !name.is_empty()).map(str::to_string);
    session.models.content_pick = picked;
    Ok(())
}

fn rename_content(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let from = required(args, "from")?;
    let to = required(args, "to")?;
    if !valid_content_segment(&to) {
        return Err(CoreError::new("content name is not a single item"));
    }
    let from_path = resolve_content_path(session, &from)?;
    let parent = from_path.parent().ok_or_else(|| CoreError::new("no content folder is open"))?;
    let dest = parent.join(&to);
    let (from_rel, to_rel) = content_rel_pair(session, &from_path, &dest)?;
    let result =
        crate::asset_index::begin_content_path_change(session, "rename", &from_rel, &to_rel);
    if let Err(err) = &result {
        set_content_error_fmt(session, map_io_error(err), vec![("reason".into(), err.to_string())]);
        return result;
    }
    session.clear_draft("content-rename")?;
    if result.is_ok() {
        crate::naming_rules::note_violation(session, &to_rel);
    }
    result
}

fn move_content(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let from = required(args, "from")?;
    let to = required(args, "to")?;
    if !valid_content_segment(&from) || !valid_content_segment(&to) {
        return Err(CoreError::new("content name is not a single item"));
    }
    let root = session
        .models
        .content_root
        .clone()
        .ok_or_else(|| CoreError::new("no content folder is open"))?;
    let from_path = root.join(&from);
    let to_path = root.join(&to);
    let (from_rel, to_rel) = content_rel_pair(session, &from_path, &to_path)?;
    let result = crate::asset_index::begin_content_path_change(session, "move", &from_rel, &to_rel);
    if let Err(err) = &result {
        set_content_error_fmt(session, map_io_error(err), vec![("reason".into(), err.to_string())]);
        return result;
    }
    session.clear_draft("content-move")?;
    result
}

fn content_rel_pair(
    session: &EditorSession,
    from: &Path,
    to: &Path,
) -> Result<(String, String), CoreError> {
    let root = session
        .project
        .as_ref()
        .map(|p| p.root.as_path())
        .or(session.models.content_root.as_deref())
        .ok_or_else(|| CoreError::new("no content folder is open"))?;
    let from_rel = from
        .strip_prefix(root)
        .map_err(|_| CoreError::new("path is outside the project"))?
        .to_string_lossy();
    let to_rel = to
        .strip_prefix(root)
        .map_err(|_| CoreError::new("path is outside the project"))?
        .to_string_lossy();
    Ok((crate::asset_index::normalize_rel(&from_rel), crate::asset_index::normalize_rel(&to_rel)))
}

fn toggle_content_tree(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let rel = args.get("path").unwrap_or("");
    content::toggle_tree_dir(session, rel);
    Ok(())
}

fn enter_content(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let rel = args.get("path").unwrap_or("").trim_matches('/').to_string();
    crate::asset_browser::go_to(session, &rel);
    Ok(())
}

fn store_viewport_bookmark(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let slot = required(args, "slot")?;
    if slot.is_empty() {
        return Err(CoreError::new("bookmark slot is empty"));
    }
    let selection_ids = if session.foundation.selection.is_empty() {
        session.models.selection.iter().cloned().collect::<Vec<_>>()
    } else {
        session.foundation.selection.clone()
    };
    let bookmark = ViewportBookmark {
        view: session.models.viewport_view.clone(),
        selection: session.models.selection.clone(),
        selection_ids,
        yaw: camera_draft_f32(session, "viewport-yaw", 0.0),
        pitch: camera_draft_f32(session, "viewport-pitch", 0.0),
        distance: camera_draft_f32(session, "viewport-distance", 10.0),
        pan_x: session.models.viewport_pan_x,
        pan_y: session.models.viewport_pan_y,
        zoom: session.models.viewport_zoom,
    };
    session.notify(session.messages.format("viewport-bookmark-stored", &[("slot", &slot)]), false);
    session.models.viewport_bookmarks.insert(slot, bookmark);
    Ok(())
}

fn recall_viewport_bookmark(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let slot = required(args, "slot")?;
    let Some(bookmark) = session.models.viewport_bookmarks.get(&slot).cloned() else {
        session.notify(
            session.messages.format("viewport-bookmark-missing", &[("slot", &slot)]),
            false,
        );
        return Ok(());
    };
    session.models.viewport_view = bookmark.view;
    session.foundation.selection = if bookmark.selection_ids.is_empty() {
        bookmark.selection.iter().cloned().collect()
    } else {
        bookmark.selection_ids.clone()
    };
    session.models.selection =
        bookmark.selection.or_else(|| session.foundation.selection.first().cloned());
    session.models.viewport_pan_x = bookmark.pan_x;
    session.models.viewport_pan_y = bookmark.pan_y;
    session.models.viewport_zoom = bookmark.zoom.clamp(0.25, 6.0);
    session.set_draft("viewport-yaw", &bookmark.yaw.to_string())?;
    session.set_draft("viewport-pitch", &bookmark.pitch.to_string())?;
    session.set_draft("viewport-distance", &bookmark.distance.to_string())?;
    session
        .notify(session.messages.format("viewport-bookmark-recalled", &[("slot", &slot)]), false);
    Ok(())
}

fn pause_console(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.models.console_paused = !session.models.console_paused;
    Ok(())
}

fn query_content(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    session.models.content_query = args.get("query").unwrap_or("").to_string();
    Ok(())
}

fn toggle_content(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.models.content_list = !session.models.content_list;
    Ok(())
}

fn delete_content_cmd(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    if session.project.as_ref().is_some_and(|project| project.read_only) {
        set_content_error(session, "content-error-read-only");
        return Err(CoreError::new(session.messages.get("content-error-read-only")));
    }
    let name = required(args, "name")?;
    if args.get("confirm") != Some("yes") {
        if let Some(root) = session.project.as_ref().map(|project| project.root.clone()) {
            let path = resolve_content_path(session, &name)?;
            if let Some(rel) = crate::asset_index::relative_to_project(&root, &path) {
                if crate::asset_index::delete_reference_check(session, &rel) {
                    // Keep the name the command was given (`./hero.png` or `sprites/hero.png`)
                    // so Confirm deletes that same item.
                    session.models.pending_delete_path = Some(name.clone());
                    return Ok(());
                }
            }
        }
    }
    let path = resolve_content_path(session, &name)?;
    let use_trash = session.foundation.settings.get("content.use_trash") != Some("false");
    let result = delete_content_at(&path, use_trash);
    if let Err(err) = &result {
        set_content_error_fmt(session, map_io_error(err), vec![("reason".into(), err.to_string())]);
        return result;
    }
    content::refresh_content_tree(session);
    if session.models.content_pick.as_deref() == Some(name.as_str()) {
        session.models.content_pick = None;
    }
    result
}

fn content_read_only(session: &mut EditorSession) -> Result<(), CoreError> {
    if session.project.as_ref().is_some_and(|project| project.read_only) {
        set_content_error(session, "content-error-read-only");
        return Err(CoreError::new(session.messages.get("content-error-read-only")));
    }
    Ok(())
}

/// `dir/stem copy.ext`, then `stem copy 2.ext`, ... until the name is free.
pub fn free_copy_name(path: &Path) -> PathBuf {
    let dir = path.parent().unwrap_or(Path::new(""));
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let mut n = 1;
    loop {
        let name =
            if n == 1 { format!("{stem} copy{ext}") } else { format!("{stem} copy {n}{ext}") };
        let candidate = dir.join(name);
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(from, to).map(|_| ())
    }
}

/// Copy a file or folder next to itself as "name copy" and select the copy.
fn duplicate_content_cmd(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    content_read_only(session)?;
    let name = required(args, "name")?;
    let path = resolve_content_path(session, &name)?;
    if !path.exists() {
        return Err(CoreError::new(format!("{name} does not exist")));
    }
    let target = free_copy_name(&path);
    if let Err(err) = copy_tree(&path, &target) {
        let err = CoreError::from(err);
        set_content_error_fmt(
            session,
            map_io_error(&err),
            vec![("reason".into(), err.to_string())],
        );
        return Err(err);
    }
    content::refresh_content_tree(session);
    session.models.content_generation = session.models.content_generation.wrapping_add(1);
    if let Some(file) = target.file_name() {
        session.notify(format!("Duplicated as {}", file.to_string_lossy()), false);
    }
    Ok(())
}

/// Open the system file browser at an item's folder (or the current folder).
fn reveal_content(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let folder = match args.get("name").filter(|n| !n.is_empty()) {
        Some(name) => {
            let path = resolve_content_path(session, name)?;
            if path.is_dir() {
                path
            } else {
                path.parent().map(Path::to_path_buf).unwrap_or(path)
            }
        }
        None => session
            .models
            .content_root
            .clone()
            .or_else(|| session.models.content_tree_root.clone())
            .ok_or_else(|| CoreError::new("no content folder is open"))?,
    };
    session.host_action = Some(crate::session::HostAction::OpenPath(folder));
    Ok(())
}

/// Write an empty `New Scene.seal` (or `New Scene 2.seal`, ...) in the current folder.
fn new_content_scene(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    content_read_only(session)?;
    let folder = args
        .get("path")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| session.models.content_root.clone())
        .or_else(|| session.models.content_tree_root.clone())
        .ok_or_else(|| CoreError::new("no content folder is open"))?;
    let name = args.get("name").filter(|n| !n.trim().is_empty()).unwrap_or("New Scene").trim();
    if name.contains(['/', '\\']) || name == "." || name == ".." {
        return Err(CoreError::new("content name is not a single item"));
    }
    let mut target = folder.join(format!("{name}.seal"));
    let mut n = 2;
    while target.exists() {
        target = folder.join(format!("{name} {n}.seal"));
        n += 1;
    }
    let text = crate::project_dimension::bootstrap_new_scene(session, name);
    crate::persist::atomic_write(&target, text.as_bytes())?;
    content::refresh_content_tree(session);
    session.models.content_generation = session.models.content_generation.wrapping_add(1);
    if let Some(file) = target.file_name() {
        session.notify(format!("Created {}", file.to_string_lossy()), false);
    }
    Ok(())
}

fn open_content(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let path = required(args, "path")?;
    content::open_content_folder(session, Path::new(&path));
    Ok(())
}

fn edit_asset(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let asset = required(args, "asset")?;
    let kind = args.get("kind").unwrap_or("prefab");
    session.models.asset_edit_last = Some((kind.to_string(), asset.clone()));
    if kind == "prefab" && session.world.mode == seal_world::PlayMode::Editing {
        // A real prefab file opens on its own for editing.
        if let Some(root) = session.project.as_ref().map(|p| p.root.clone()) {
            let file = root.join("prefabs").join(format!("{asset}.prefab"));
            if file.is_file() {
                return session.begin_prefab_edit(file);
            }
        }
    }
    session.models.content_query = asset.clone();
    session.reveal_panel = Some("content".into());
    session.focus_panel("content");
    Ok(())
}

fn set_shade(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let shade = required(args, "mode")?;
    if !crate::viewport_selection::shade_mode_allowed(&shade) {
        return Err(CoreError::new(
            "the shade is lit, unlit, wireframe, collision, lighting only, overdraw, or a buffer view",
        ));
    }
    if let Some(index) = pane_index_arg(args) {
        return crate::viewport_split::set_pane_shade(session, index, &shade);
    }
    session.models.viewport_shade = shade;
    if session.models.viewport_layout_preset == "single" {
        session.models.viewport_active_pane = 0;
    }
    viewport_store_active_pane(session);
    Ok(())
}

fn pane_index_arg(args: &CommandArgs) -> Option<usize> {
    args.get("index")?.parse().ok()
}

fn set_view(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let view = required(args, "view")?;
    let normalized = if view == "side" { "right".into() } else { view };
    if !matches!(
        normalized.as_str(),
        "perspective" | "front" | "back" | "top" | "bottom" | "left" | "right" | "2d"
    ) {
        return Err(CoreError::new(
            "the view is perspective, front, back, top, bottom, left, right, or 2d",
        ));
    }
    if let Some(index) = pane_index_arg(args) {
        return crate::viewport_split::set_pane_view(session, index, &normalized);
    }
    let panel = viewport_panel_id(args);
    if panel == "viewport" {
        session.models.viewport_view = normalized.clone();
        if session.models.viewport_layout_preset != "single" {
            viewport_store_active_pane(session);
        }
    } else {
        ensure_viewport_tab(&mut session.models, panel);
        if let Some(tab) = session.models.viewport_tabs.get_mut(panel) {
            tab.view = normalized.clone();
        }
    }
    Ok(())
}

fn set_viewport_layout(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let id = required(args, "preset")?;
    crate::viewport_split::change_layout(session, &id)
}

fn set_viewport_pane_focus(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let index = required(args, "index")?
        .parse::<usize>()
        .map_err(|_| CoreError::new("pane index was not a number"))?;
    if session.models.viewport_layout_preset == "single" {
        return Ok(());
    }
    viewport_store_active_pane(session);
    let max = session.models.viewport_panes.len().saturating_sub(1);
    session.models.viewport_active_pane = index.min(max);
    viewport_load_active_pane(session);
    Ok(())
}

fn toggle_viewport_pane_expand(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    if session.models.viewport_layout_preset == "single" {
        return Ok(());
    }
    if let Some(index) = pane_index_arg(args) {
        if index < session.models.viewport_panes.len()
            && index != session.models.viewport_active_pane
        {
            viewport_store_active_pane(session);
            session.models.viewport_active_pane = index;
            viewport_load_active_pane(session);
        }
    }
    session.models.viewport_pane_expanded = !session.models.viewport_pane_expanded;
    Ok(())
}

fn toggle_grid(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.models.viewport_grid = !session.models.viewport_grid;
    let on = session.models.viewport_grid;
    session.models.viewport_show_layers.insert("grid".into(), on);
    Ok(())
}

fn set_viewport_show(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let layer = required(args, "layer")?;
    if !VIEWPORT_SHOW_LAYER_IDS.contains(&layer.as_str()) {
        return Err(CoreError::new("unknown viewport show layer"));
    }
    let on = match args.get("on") {
        Some(text) => text == "1",
        None => !session.models.viewport_show_layers.get(layer.as_str()).copied().unwrap_or(true),
    };
    if let Some(index) = pane_index_arg(args) {
        return crate::viewport_split::set_pane_show(session, index, &layer, on);
    }
    session.models.viewport_show_layers.insert(layer.clone(), on);
    sync_viewport_show_aliases(&mut session.models);
    if session.models.viewport_layout_preset == "single" {
        session.models.viewport_active_pane = 0;
    }
    viewport_store_active_pane(session);
    Ok(())
}

fn set_viewport_show_menu(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    if let Some(open) = args.get("open") {
        session.models.viewport_show_menu_open = open == "1";
    } else {
        session.models.viewport_show_menu_open = !session.models.viewport_show_menu_open;
    }
    Ok(())
}

fn set_viewport_hamburger_menu(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    if let Some(open) = args.get("open") {
        session.models.viewport_hamburger_open = open == "1";
    } else {
        session.models.viewport_hamburger_open = !session.models.viewport_hamburger_open;
    }
    Ok(())
}

fn toggle_viewport_realtime(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    session.models.viewport_realtime = !session.models.viewport_realtime;
    Ok(())
}

fn toggle_viewport_show_fps(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    session.models.viewport_show_fps = !session.models.viewport_show_fps;
    Ok(())
}

fn toggle_viewport_show_toolbar(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    session.models.viewport_show_toolbar = !session.models.viewport_show_toolbar;
    Ok(())
}

fn toggle_viewport_show_stats(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    session.models.viewport_show_stats = !session.models.viewport_show_stats;
    Ok(())
}

fn toggle_viewport_cinematic(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    session.models.viewport_cinematic = !session.models.viewport_cinematic;
    Ok(())
}

fn set_viewport_fov(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let fov = required(args, "fov")?
        .parse::<f32>()
        .map_err(|_| CoreError::new("field of view must be a number"))?;
    if !fov.is_finite() {
        return Err(CoreError::new("field of view must be a number"));
    }
    session.models.viewport_fov = fov.clamp(40.0, 120.0);
    Ok(())
}

fn set_viewport_far_plane(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let value = required(args, "value")?
        .parse::<f32>()
        .map_err(|_| CoreError::new("far plane must be a number"))?;
    if !value.is_finite() {
        return Err(CoreError::new("far plane must be a number"));
    }
    session.models.viewport_far_plane = value.clamp(100.0, 1_000_000.0);
    Ok(())
}

fn set_viewport_screen_pct(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let value = required(args, "value")?
        .parse::<f32>()
        .map_err(|_| CoreError::new("screen percentage must be a number"))?;
    if !value.is_finite() {
        return Err(CoreError::new("screen percentage must be a number"));
    }
    session.models.viewport_screen_pct = value.clamp(25.0, 200.0);
    Ok(())
}

fn create_viewport_camera(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    editing(session)?;
    let previous = session.world.to_text();
    let id = session.world.add("camera");
    // Put the new camera where the editor view is looking from, aimed at its focus.
    let draft = |key: &str, fallback: f32| {
        session.draft(key).and_then(|text| text.parse::<f32>().ok()).unwrap_or(fallback)
    };
    let yaw = draft("viewport-yaw", 0.0);
    let pitch = draft("viewport-pitch", 0.0);
    let distance = draft("viewport-distance", 10.0).max(1.0);
    if let Some(entity) = session.world.entity_mut(&id) {
        let (sin_y, cos_y) = yaw.to_radians().sin_cos();
        let (sin_p, cos_p) = pitch.to_radians().sin_cos();
        entity.translation = [distance * cos_p * sin_y, distance * sin_p, distance * cos_p * cos_y];
        entity.rotation = [-pitch, yaw, 0.0];
        entity.fov = session.models.viewport_fov;
    }
    session.adopt_world();
    session.models.selection = Some(id.clone());
    session.foundation.selection = vec![id];
    push_scene_past(session, previous);
    session.mark_scene();
    session.notify(session.messages.get("viewport-camera-created"), false);
    Ok(())
}

fn viewport_screenshot(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.notify(session.messages.get("viewport-screenshot-queued"), false);
    Ok(())
}

fn seed_viewport_primitives(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    editing(session)?;
    let previous = session.world.to_text();
    let specs: &[(&str, [f32; 3], [f32; 3])] = &[
        ("cube", [0.0, 0.5, 0.0], [0.82, 0.32, 0.32]),
        ("sphere", [2.4, 0.55, 0.0], [0.32, 0.62, 0.88]),
        ("cylinder", [-2.4, 0.55, 0.0], [0.4, 0.78, 0.36]),
        ("cone", [4.2, 0.45, 0.0], [0.92, 0.72, 0.28]),
        ("capsule", [-4.2, 0.7, 0.0], [0.68, 0.38, 0.86]),
    ];
    for (kind, translation, color) in specs {
        let id = session.world.add(kind);
        if let Some(entity) = session.world.entity_mut(&id) {
            entity.translation = *translation;
            entity.color = *color;
            entity.dynamic = false;
        }
    }
    session.adopt_world();
    push_scene_past(session, previous);
    session.mark_scene();
    Ok(())
}

fn frame_viewport_selection(
    session: &mut EditorSession,
    args: &CommandArgs,
) -> Result<(), CoreError> {
    let frame_all = args.get("all") == Some("1");
    let ids: Vec<String> = if let Some(id) = args.get("id").filter(|value| !value.is_empty()) {
        vec![id.to_string()]
    } else if frame_all {
        session
            .world
            .entities
            .iter()
            .filter(|entity| entity.id != "root" && !entity.hidden)
            .map(|entity| entity.id.clone())
            .collect()
    } else if session.foundation.selection.is_empty() {
        if let Some(id) = session.models.selection.clone() {
            vec![id]
        } else {
            return Err(CoreError::new("nothing selected to frame"));
        }
    } else {
        session.foundation.selection.clone()
    };
    if ids.is_empty() {
        return Err(CoreError::new("nothing selected to frame"));
    }
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut found = false;
    for entity in &session.world.entities {
        if !ids.iter().any(|id| id == &entity.id) {
            continue;
        }
        let x = entity.translation[0];
        let y = entity.translation[1];
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
        found = true;
    }
    if !found {
        return Err(CoreError::new("nothing selected to frame"));
    }
    let pad = 1.5_f32;
    min_x -= pad;
    max_x += pad;
    min_y -= pad;
    max_y += pad;
    let span = (max_x - min_x).max(max_y - min_y).max(2.0);
    let cx = (min_x + max_x) * 0.5;
    let cy = (min_y + max_y) * 0.5;
    let zoom = (8.0 / span).clamp(0.35, 4.0);
    let cell = 32.0_f32 * zoom;
    session.models.viewport_pan_x = -cx * cell;
    session.models.viewport_pan_y = 200.0 - cy * cell;
    session.models.viewport_zoom = zoom;
    let distance = (span * 3.5).clamp(4.0, 80.0);
    session.set_draft("viewport-distance", &distance.to_string())?;
    Ok(())
}

fn viewport_show_defaults(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    session.models.viewport_show_layers = default_viewport_show_layers();
    sync_viewport_show_aliases(&mut session.models);
    Ok(())
}

fn toggle_snap(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.models.viewport_grid_snap = !session.models.viewport_grid_snap;
    session.models.snap = session.models.viewport_grid_snap;
    Ok(())
}

fn toggle_viewport_space(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    session.models.viewport_space =
        if session.models.viewport_space == "local" { "world".into() } else { "local".into() };
    Ok(())
}

fn toggle_surface_snap(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.models.viewport_surface_snap = !session.models.viewport_surface_snap;
    Ok(())
}

fn toggle_vertex_snap(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.models.viewport_vertex_snap = !session.models.viewport_vertex_snap;
    Ok(())
}

fn toggle_grid_snap(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.models.viewport_grid_snap = !session.models.viewport_grid_snap;
    session.models.snap = session.models.viewport_grid_snap;
    Ok(())
}

fn snap_step_index(steps: &[f32], current: f32) -> usize {
    steps
        .iter()
        .position(|value| (*value - current).abs() < f32::EPSILON)
        .unwrap_or_else(|| steps.iter().position(|v| *v >= current).unwrap_or(steps.len() - 1))
}

fn toggle_angle_snap(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.models.viewport_angle_snap = !session.models.viewport_angle_snap;
    Ok(())
}

fn toggle_scale_snap(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.models.viewport_scale_snap = !session.models.viewport_scale_snap;
    Ok(())
}

fn grid_snap_decrease(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    set_grid_snap_step(session, &CommandArgs::new(&[("dir", "-1")]))
}

fn grid_snap_increase(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    set_grid_snap_step(session, &CommandArgs::new(&[("dir", "1")]))
}

fn set_grid_snap_step(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    // Metres, matching the viewport bar's list.
    const STEPS: [f32; 7] = [0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0];
    if let Some(text) = args.get("step") {
        let parsed =
            text.parse::<f32>().map_err(|_| CoreError::new("grid snap step must be a number"))?;
        if STEPS.iter().any(|value| (*value - parsed).abs() < f32::EPSILON) {
            session.models.viewport_grid_snap_step = parsed;
        }
        return Ok(());
    }
    let dir: i32 = args.get("dir").and_then(|t| t.parse().ok()).unwrap_or(0);
    if dir == 0 {
        return Ok(());
    }
    let index = snap_step_index(&STEPS, session.models.viewport_grid_snap_step);
    let next = if dir > 0 { (index + 1).min(STEPS.len() - 1) } else { index.saturating_sub(1) };
    session.models.viewport_grid_snap_step = STEPS[next];
    Ok(())
}

fn set_angle_snap_step(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    const STEPS: [f32; 8] = [5.0, 10.0, 15.0, 30.0, 45.0, 60.0, 90.0, 120.0];
    if let Some(text) = args.get("step") {
        let parsed =
            text.parse::<f32>().map_err(|_| CoreError::new("angle snap step must be a number"))?;
        if STEPS.iter().any(|value| (*value - parsed).abs() < f32::EPSILON) {
            session.models.viewport_angle_snap_deg = parsed;
        }
        return Ok(());
    }
    let dir: i32 = args.get("dir").and_then(|t| t.parse().ok()).unwrap_or(0);
    if dir == 0 {
        return Ok(());
    }
    let index = snap_step_index(&STEPS, session.models.viewport_angle_snap_deg);
    let next = if dir > 0 { (index + 1).min(STEPS.len() - 1) } else { index.saturating_sub(1) };
    session.models.viewport_angle_snap_deg = STEPS[next];
    Ok(())
}

fn set_scale_snap_step(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    const STEPS: [f32; 6] = [10.0, 1.0, 0.5, 0.25, 0.125, 0.0625];
    if let Some(text) = args.get("step") {
        let parsed =
            text.parse::<f32>().map_err(|_| CoreError::new("scale snap step must be a number"))?;
        if STEPS.iter().any(|value| (*value - parsed).abs() < f32::EPSILON) {
            session.models.viewport_scale_snap_step = parsed;
        }
        return Ok(());
    }
    let dir: i32 = args.get("dir").and_then(|t| t.parse().ok()).unwrap_or(0);
    if dir == 0 {
        return Ok(());
    }
    let index = snap_step_index(&STEPS, session.models.viewport_scale_snap_step);
    let next = if dir > 0 { (index + 1).min(STEPS.len() - 1) } else { index.saturating_sub(1) };
    session.models.viewport_scale_snap_step = STEPS[next];
    Ok(())
}

fn set_camera_speed(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    if let Some(text) = args.get("speed") {
        let speed =
            text.parse::<f32>().map_err(|_| CoreError::new("camera speed must be a number"))?;
        session.models.viewport_camera_speed = speed.clamp(1.0, 8.0);
    }
    Ok(())
}

fn hide_viewport_warning(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let id = required(args, "id")?;
    session.models.viewport_hidden_warnings.insert(id.to_string());
    Ok(())
}

fn set_camera_speed_menu(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    if let Some(open) = args.get("open") {
        session.models.viewport_camera_speed_menu = open == "1";
    } else {
        session.models.viewport_camera_speed_menu = !session.models.viewport_camera_speed_menu;
    }
    Ok(())
}

fn editing(session: &EditorSession) -> Result<(), CoreError> {
    if session.world.mode == seal_world::PlayMode::Editing {
        Ok(())
    } else {
        Err(CoreError::new("stop play before editing the scene"))
    }
}

fn set_tool(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let tool = required(args, "tool")?;
    if !matches!(tool.as_str(), "select" | "move" | "rotate" | "scale" | "measure") {
        return Err(CoreError::new("the tool is select, move, rotate, scale, or measure"));
    }
    let previous = session.models.viewport_tool.clone();
    session.models.viewport_tool = tool.clone();
    if previous != tool && tool != "measure" {
        let _ = session.set_draft("measure-path", "");
        let _ = session.set_draft("measure-drag", "");
        let _ = session.set_draft("measure-status", "");
        let _ = session.set_draft("measure-anchor", "");
    }
    // Picking a tool ends sprite painting, so clicks select and drag again.
    if session.draft("place-brush").is_some_and(|brush| !brush.is_empty()) {
        session.set_draft("place-brush", "")?;
    }
    Ok(())
}

fn pose_of(entity: &seal_world::Entity) -> [f32; 9] {
    [
        entity.translation[0],
        entity.translation[1],
        entity.translation[2],
        entity.rotation[0],
        entity.rotation[1],
        entity.rotation[2],
        entity.scale[0],
        entity.scale[1],
        entity.scale[2],
    ]
}

fn write_pose(entity: &mut seal_world::Entity, pose: [f32; 9]) {
    entity.translation = [pose[0], pose[1], pose[2]];
    entity.rotation = [pose[3], pose[4], pose[5]];
    entity.scale = [pose[6], pose[7], pose[8]];
}

fn viewport_drag_label(session: &EditorSession, tool: &str) -> String {
    let verb = match tool {
        "rotate" => "Rotate",
        "scale" => "Scale",
        _ => "Move",
    };
    let count = session.foundation.selection.len();
    if count > 1 {
        return format!("{verb} {count} Entities");
    }
    let name = session
        .models
        .selection
        .as_ref()
        .and_then(|id| {
            session.models.nodes.iter().find(|node| node.id == *id).map(|node| node.name.clone())
        })
        .unwrap_or_else(|| "Entity".into());
    format!("{verb} {name}")
}

/// Drag in the viewport. Move sends a new center. Rotate and scale send a delta for this frame.
/// `begin` records one undo step for the whole drag.
fn move_entity(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let id = if let Some(raw) = args.get("id") {
        if raw.is_empty() {
            selected_id(session)?
        } else {
            raw.to_string()
        }
    } else {
        selected_id(session)?
    };
    let begin = args.get("begin") == Some("1");
    let (first, second, spin) = match session.models.viewport_view.as_str() {
        "top" | "bottom" => (0, 2, 1),
        "left" | "right" => (2, 1, 0),
        _ => (0, 1, 2),
    };
    let tool = session.models.viewport_tool.clone();
    let snap = session.models.viewport_grid_snap;
    let grid_step = session.models.viewport_grid_snap_step.max(0.001);
    let Some(entity) = session.world.entity(&id) else {
        return Err(CoreError::new("unknown entity"));
    };
    let previous = pose_of(entity);
    if begin {
        let label = viewport_drag_label(session, &tool);
        session.push_past(Past::Pose { id: id.clone(), previous, next: previous, label });
        session.future.clear();
    }
    let Some(entity) = session.world.entity_mut(&id) else {
        return Err(CoreError::new("unknown entity"));
    };
    let a = required(args, "a")?
        .parse::<f32>()
        .map_err(|_| CoreError::new("a transform value must be a number"))?;
    let b = required(args, "b")?
        .parse::<f32>()
        .map_err(|_| CoreError::new("a transform value must be a number"))?;
    if entity.locked {
        return Err(CoreError::new("the entity is locked"));
    }
    match tool.as_str() {
        "rotate" => {
            if args.get("mode") == Some("euler") {
                let snap = session.models.viewport_angle_snap;
                let step = session.models.viewport_angle_snap_deg.to_radians();
                for (key, axis) in [("a", 0), ("b", 1), ("c", 2)] {
                    if let Some(raw) = args.get(key) {
                        if let Ok(value) = raw.parse::<f32>() {
                            if value.is_finite() {
                                entity.rotation[axis] =
                                    if snap { (value / step).round() * step } else { value };
                            }
                        }
                    }
                }
            } else if a.is_finite() {
                let axis =
                    args.get("axis").and_then(|raw| raw.parse::<usize>().ok()).unwrap_or(spin);
                let axis = axis.min(2);
                entity.rotation[axis] += a;
                if session.models.viewport_angle_snap {
                    let step = session.models.viewport_angle_snap_deg.to_radians();
                    entity.rotation[axis] = (entity.rotation[axis] / step).round() * step;
                }
            }
        }
        "scale" => {
            if args.get("mode") == Some("scale") {
                let snap = session.models.viewport_scale_snap;
                let step = session.models.viewport_scale_snap_step.max(0.0001);
                for (key, axis) in [("a", 0), ("b", 1), ("c", 2)] {
                    if let Some(raw) = args.get(key) {
                        if let Ok(value) = raw.parse::<f32>() {
                            if value.is_finite() && value > 0.0 {
                                entity.scale[axis] = if snap {
                                    (value / step).round() * step
                                } else {
                                    value.clamp(0.05, 100.0)
                                };
                            }
                        }
                    }
                }
            } else if a.is_finite() && a > 0.0 {
                for axis in [first, second] {
                    let next = (entity.scale[axis] * a).clamp(0.05, 100.0);
                    entity.scale[axis] = if session.models.viewport_scale_snap {
                        let step = session.models.viewport_scale_snap_step.max(0.0001);
                        (next / step).round() * step
                    } else {
                        next
                    };
                }
            }
        }
        _ => {
            if args.get("mode") == Some("axis") {
                if let Some(raw) = args.get("axis") {
                    if let Ok(axis) = raw.parse::<usize>() {
                        if axis < 3 {
                            let value = [
                                a,
                                b,
                                args.get("c").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0),
                            ][axis];
                            if value.is_finite() {
                                entity.translation[axis] = if snap {
                                    (value / grid_step).round() * grid_step
                                } else {
                                    value
                                };
                            }
                        }
                    }
                }
            } else {
                for (axis, value) in [(first, a), (second, b)] {
                    if value.is_finite() {
                        entity.translation[axis] =
                            if snap { (value / grid_step).round() * grid_step } else { value };
                    }
                }
                if let Some(c_raw) = args.get("c") {
                    if let Ok(c) = c_raw.parse::<f32>() {
                        if c.is_finite() {
                            entity.translation[spin] =
                                if snap { (c / grid_step).round() * grid_step } else { c };
                        }
                    }
                }
            }
            if session.models.viewport_vertex_snap {
                let step = session.models.viewport_grid_snap_step.max(0.001);
                for axis in 0..3 {
                    entity.translation[axis] = (entity.translation[axis] / step).round() * step;
                }
            }
        }
    }
    let next = pose_of(entity);
    if let Some(Past::Pose { id: past_id, next: slot, .. }) = session.past.last_mut() {
        if past_id == &id {
            *slot = next;
        }
    }
    session.models.selection = Some(id);
    session.adopt_world();
    session.mark_scene();
    Ok(())
}

fn filter_outliner(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let query = args.get("query").unwrap_or_default().to_string();
    let panel = session.foundation.focused_panel_id.clone();
    session.set_outliner_query_for(&panel, query);
    Ok(())
}

fn selected_id(session: &EditorSession) -> Result<String, CoreError> {
    session
        .models
        .selection
        .clone()
        .filter(|id| id != "root")
        .ok_or_else(|| CoreError::new("select something other than the world"))
}

fn remember_pose(session: &mut EditorSession, id: &str, label: &str) -> Result<(), CoreError> {
    let entity = session.world.entity(id).ok_or_else(|| CoreError::new("unknown entity"))?;
    let pose = pose_of(entity);
    session.push_past(Past::Pose {
        id: id.to_string(),
        previous: pose,
        next: pose,
        label: label.to_string(),
    });
    session.future.clear();
    Ok(())
}

fn finish_pose(session: &mut EditorSession, id: &str) {
    if let Some(entity) = session.world.entity(id) {
        let next = pose_of(entity);
        if let Some(Past::Pose { next: slot, .. }) = session.past.last_mut() {
            *slot = next;
        }
    }
    session.adopt_world();
    session.mark_scene();
}

fn tile_tool_brush(session: &mut EditorSession, _: &CommandArgs) -> Result<(), CoreError> {
    crate::tiles::set_paint_tool(session, "brush")
}

fn tile_tool_rectangle(session: &mut EditorSession, _: &CommandArgs) -> Result<(), CoreError> {
    crate::tiles::set_paint_tool(session, "rectangle")
}

fn tile_tool_line(session: &mut EditorSession, _: &CommandArgs) -> Result<(), CoreError> {
    crate::tiles::set_paint_tool(session, "line")
}

fn tile_tool_fill(session: &mut EditorSession, _: &CommandArgs) -> Result<(), CoreError> {
    crate::tiles::set_paint_tool(session, "fill")
}

fn tile_tool_picker(session: &mut EditorSession, _: &CommandArgs) -> Result<(), CoreError> {
    crate::tiles::set_paint_tool(session, "picker")
}

fn tile_tool_eraser(session: &mut EditorSession, _: &CommandArgs) -> Result<(), CoreError> {
    crate::tiles::set_paint_tool(session, "eraser")
}

fn tile_tool_random(session: &mut EditorSession, _: &CommandArgs) -> Result<(), CoreError> {
    crate::tiles::set_paint_tool(session, "random")
}

fn tile_tool_pattern(session: &mut EditorSession, _: &CommandArgs) -> Result<(), CoreError> {
    crate::tiles::set_paint_tool(session, "pattern")
}

fn snap_floor(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let id = selected_id(session)?;
    let label = session.messages.get("command-scene-floor");
    remember_pose(session, &id, &label)?;
    if !session.world.snap_floor(&id) {
        return Err(CoreError::new("unknown entity"));
    }
    finish_pose(session, &id);
    Ok(())
}

fn reset_transform(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let id = selected_id(session)?;
    let label = session.messages.get("command-scene-reset");
    remember_pose(session, &id, &label)?;
    if !session.world.reset_transform(&id) {
        return Err(CoreError::new("unknown entity"));
    }
    finish_pose(session, &id);
    Ok(())
}

fn world_descendants(world: &seal_world::World, root: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut queue: Vec<String> = world
        .entities
        .iter()
        .filter(|entity| entity.parent.as_deref() == Some(root))
        .map(|entity| entity.id.clone())
        .collect();
    while let Some(parent) = queue.pop() {
        out.push(parent.clone());
        for entity in &world.entities {
            if entity.parent.as_deref() == Some(parent.as_str()) {
                queue.push(entity.id.clone());
            }
        }
    }
    out
}

fn toggle_hidden(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let id = if let Some(id) = args.get("id") { id.to_string() } else { selected_id(session)? };
    if id == "root" {
        return Ok(());
    }
    let previous = session.world.to_text();
    let Some(entity) = session.world.entity_mut(&id) else {
        return Err(CoreError::new("unknown entity"));
    };
    let new_hidden = !entity.hidden;
    entity.hidden = new_hidden;
    let descendants = world_descendants(&session.world, &id);
    for child in descendants {
        if let Some(slot) = session.world.entity_mut(&child) {
            slot.hidden = new_hidden;
        }
    }
    session.adopt_world();
    push_scene_past(session, previous);
    session.mark_scene();
    Ok(())
}

fn hide_solo(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let id = if let Some(id) = args.get("id") { id.to_string() } else { selected_id(session)? };
    if id == "root" {
        return Ok(());
    }
    let previous = session.world.to_text();
    for entity in &mut session.world.entities {
        if entity.id == "root" {
            continue;
        }
        entity.hidden = entity.id != id;
    }
    session.adopt_world();
    push_scene_past(session, previous);
    session.mark_scene();
    Ok(())
}

fn toggle_locked(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let id = if let Some(id) = args.get("id") { id.to_string() } else { selected_id(session)? };
    if id == "root" {
        return Ok(());
    }
    let previous = session.world.to_text();
    let Some(entity) = session.world.entity_mut(&id) else {
        return Err(CoreError::new("unknown entity"));
    };
    entity.locked = !entity.locked;
    session.adopt_world();
    push_scene_past(session, previous);
    session.mark_scene();
    Ok(())
}

fn toggle_unselectable(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let id = if let Some(id) = args.get("id") { id.to_string() } else { selected_id(session)? };
    if id == "root" {
        return Ok(());
    }
    let previous = session.world.to_text();
    let Some(entity) = session.world.entity_mut(&id) else {
        return Err(CoreError::new("unknown entity"));
    };
    entity.unselectable = !entity.unselectable;
    session.adopt_world();
    push_scene_past(session, previous);
    session.mark_scene();
    Ok(())
}

fn viewport_isolate_cmd(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    let previous = session.world.to_text();
    crate::viewport_isolate::toggle_isolate(session)?;
    push_scene_past(session, previous);
    session.mark_scene();
    Ok(())
}

fn viewport_overlay_cmd(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    crate::viewport_overlays::apply_overlay_command(session, args)
}

fn group_selection(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let mut ids = session.foundation.selection.clone();
    if let Some(id) = &session.models.selection {
        if !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    ids.retain(|id| id != "root");
    if ids.is_empty() {
        return Err(CoreError::new("select something other than the world"));
    }
    let previous = session.world.to_text();
    let id = session.world.group(&ids);
    session.adopt_world();
    session.models.selection = Some(id);
    session.foundation.selection.clear();
    push_scene_past(session, previous);
    session.mark_scene();
    Ok(())
}

fn project_root(session: &EditorSession) -> Result<std::path::PathBuf, CoreError> {
    session
        .project
        .as_ref()
        .map(|p| p.root.clone())
        .ok_or_else(|| CoreError::new("open a project first"))
}

/// Save the selected object and its children as prefabs/<Name>.prefab and link it as an instance.
fn make_prefab(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let id = selected_id(session)?;
    let root = project_root(session)?;
    let text = session
        .world
        .prefab_text(&id)
        .ok_or_else(|| CoreError::new("select something other than the world"))?;
    let wanted = args
        .get("name")
        .map(str::to_string)
        .unwrap_or_else(|| session.world.entity(&id).map(|e| e.name.clone()).unwrap_or_default());
    let stem: String = wanted
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let stem = if stem.trim().is_empty() { "Prefab".to_string() } else { stem.trim().to_string() };
    let dir = root.join("prefabs");
    std::fs::create_dir_all(&dir)?;
    let mut name = stem.clone();
    let mut n = 2;
    while dir.join(format!("{name}.prefab")).exists() {
        name = format!("{stem} {n}");
        n += 1;
    }
    std::fs::write(dir.join(format!("{name}.prefab")), text)?;
    let previous = session.world.to_text();
    if let Some(e) = session.world.entity_mut(&id) {
        e.source_kind = "prefab".into();
        e.source_asset = name.clone();
    }
    session.adopt_world();
    push_scene_past(session, previous);
    session.mark_scene();
    session.notify(format!("Saved prefab {name}"), false);
    Ok(())
}

/// Add a prefab file (path relative to the project) under the selection or the root.
/// Open a prefab on its own: `path=` a project file, or the selected copy's prefab.
fn prefab_edit(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let root = project_root(session)?;
    let rel = match args.get("path").filter(|p| !p.is_empty()) {
        Some(rel) => rel.to_string(),
        None => {
            let id = session
                .models
                .selection
                .clone()
                .ok_or_else(|| CoreError::new("select a prefab copy first"))?;
            let e = session
                .world
                .entity(&id)
                .ok_or_else(|| CoreError::new("select a prefab copy first"))?;
            if e.source_kind != "prefab" || e.source_asset.is_empty() {
                return Err(CoreError::new("the selected object is not a prefab copy"));
            }
            format!("prefabs/{}.prefab", e.source_asset)
        }
    };
    if rel.contains("..") {
        return Err(CoreError::new("prefab path must stay inside the project"));
    }
    let file =
        if root.join(&rel).is_file() { root.join(&rel) } else { root.join("prefabs").join(&rel) };
    session.begin_prefab_edit(file)
}

/// Open another scene file of the project (the current one is saved first).
fn open_scene(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let rel = required(args, "path")?;
    if rel.contains("..") {
        return Err(CoreError::new("scene path must stay inside the project"));
    }
    let root = project_root(session)?;
    session.open_scene_file(root.join(rel))
}

/// `content.open_file path=`: open a Asset Browser file the way its kind is opened.
/// Folders are entered, scenes open in the viewport, prefabs open for editing,
/// logic graphs open in the Graph editor, sequences in the Timeline, another
/// project's `project.seal` switches project, and anything else (pictures,
/// effects, sounds, text) opens in the system's default program.
fn open_content_file(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let rel = required(args, "path")?.replace('\\', "/");
    if rel.split('/').any(|part| part == "..") || rel.starts_with('/') {
        return Err(CoreError::new("the file must stay inside the project"));
    }
    let root = session
        .models
        .content_tree_root
        .clone()
        .or_else(|| session.project.as_ref().map(|p| p.root.clone()))
        .ok_or_else(|| CoreError::new("open a project first"))?;
    let full = root.join(&rel);
    if full.is_dir() {
        crate::content::open_content_folder(session, &full);
        return Ok(());
    }
    if !full.is_file() {
        return Err(CoreError::new(format!("{rel} was not found")));
    }
    session.models.content_pick = Some(rel.clone());
    let name =
        full.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let ext =
        full.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let project_root = session.project.as_ref().map(|p| p.root.clone());
    let in_project = |p: &Path| {
        project_root
            .as_ref()
            .and_then(|r| p.strip_prefix(r).ok())
            .map(|r| r.to_string_lossy().replace('\\', "/"))
    };
    match ext.as_str() {
        "seal" if name == "project.seal" => {
            let folder = full.parent().map(Path::to_path_buf).unwrap_or_default();
            if project_root.as_deref() == Some(folder.as_path()) {
                let project = session.project.as_ref().map(|p| p.name.clone()).unwrap_or_default();
                session.notify(
                    session
                        .messages
                        .format("content-open-project-current", &[("name", project.as_str())]),
                    false,
                );
                return Ok(());
            }
            let folder = folder.to_string_lossy().to_string();
            session.dispatch("file.open-project", &CommandArgs::new(&[("path", folder.as_str())]))
        }
        "seal" => {
            editing(session)?;
            session.open_scene_file(full)
        }
        "prefab" => {
            editing(session)?;
            session.begin_prefab_edit(full)
        }
        "graph" => match in_project(&full) {
            Some(rel) => crate::graph_doc::open_graph(session, &rel),
            None => Err(CoreError::new("the graph is outside the project")),
        },
        "seq" => match in_project(&full) {
            Some(rel) => {
                session.set_draft(crate::sequences::TIMELINE_SEQ_DRAFT, &rel)?;
                session.set_draft(crate::sequences::TIMELINE_TIME_DRAFT, "0")?;
                session.focus_panel("timeline");
                Ok(())
            }
            None => Err(CoreError::new("the sequence is outside the project")),
        },
        _ => match in_project(&full) {
            Some(rel) => crate::asset_editor::show_asset(session, &rel),
            None => Err(CoreError::new("the file is outside the project")),
        },
    }
}

fn prefab_done(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.finish_prefab_edit()
}

fn add_prefab(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let rel = required(args, "path")?;
    if rel.contains("..") {
        return Err(CoreError::new("prefab path must stay inside the project"));
    }
    let root = project_root(session)?;
    // A bare file name (dragged from the Asset Browser) is looked up in prefabs/.
    let file =
        if root.join(&rel).is_file() { root.join(&rel) } else { root.join("prefabs").join(&rel) };
    let text = std::fs::read_to_string(&file)
        .map_err(|_| CoreError::new(format!("prefab {rel} was not found")))?;
    session.world.project_dir = Some(root.clone());
    let prefabs = root.join("prefabs");
    let text = seal_world::prefab_variant::read_prefab_for_world(Some(&prefabs), &text, false)
        .map_err(CoreError::new)?;
    let previous = session.world.to_text();
    let parent = session.models.selection.clone();
    let id = session.world.add_prefab_text(&text, parent.as_deref()).map_err(CoreError::new)?;
    let asset = std::path::Path::new(&rel)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let at = (
        args.get("a").and_then(|v| v.parse::<f32>().ok()),
        args.get("b").and_then(|v| v.parse::<f32>().ok()),
    );
    let mut delta = [0.0; 3];
    if let Some(e) = session.world.entity_mut(&id) {
        e.source_kind = "prefab".into();
        e.source_asset = asset;
        if let (Some(a), Some(b)) = at {
            delta = [a - e.translation[0], b - e.translation[1], 0.0];
            e.translation[0] = a;
            e.translation[1] = b;
        }
    }
    session.world.shift_children(&id, delta);
    session.adopt_world();
    // The new object is the only one selected, so the Inspector shows it at once.
    session.foundation.selection = vec![id.clone()];
    session.models.selection = Some(id);
    push_scene_past(session, previous);
    session.mark_scene();
    Ok(())
}

fn ungroup_selection(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let id = selected_id(session)?;
    let previous = session.world.to_text();
    if !session.world.ungroup(&id) {
        return Err(CoreError::new("that selection is not a group"));
    }
    session.adopt_world();
    session.models.selection = None;
    push_scene_past(session, previous);
    session.mark_scene();
    Ok(())
}

pub(crate) fn play_step_frame(
    session: &mut EditorSession,
    _args: &CommandArgs,
) -> Result<(), CoreError> {
    if !session.world.step_once() {
        return Err(CoreError::new("step needs play or pause"));
    }
    session.adopt_world();
    Ok(())
}

fn viewport_go_here(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let x = args.get("x").and_then(|text| text.parse::<f32>().ok()).unwrap_or(0.0);
    let y = args.get("y").and_then(|text| text.parse::<f32>().ok()).unwrap_or(0.0);
    let yaw = session
        .draft("viewport-yaw")
        .and_then(|text| text.parse::<f32>().ok())
        .unwrap_or(0.0)
        .to_radians();
    let (sin_y, cos_y) = yaw.sin_cos();
    let rx = x * cos_y - y * sin_y;
    let ry = x * sin_y + y * cos_y;
    let distance = session
        .draft("viewport-distance")
        .and_then(|text| text.parse::<f32>().ok())
        .unwrap_or(10.0)
        .max(1.0);
    let dolly = 10.0 / distance;
    let pitch = session
        .draft("viewport-pitch")
        .and_then(|text| text.parse::<f32>().ok())
        .unwrap_or(0.0)
        .to_radians();
    let pitch_scale = pitch.cos().abs().max(0.35);
    let cell = 20.0 * session.models.viewport_zoom * dolly * pitch_scale;
    session.models.viewport_pan_x = -rx * cell;
    session.models.viewport_pan_y = ry * cell;
    Ok(())
}

fn play_from_selection(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    let id = selected_id(session)?;
    if !session.world.play_from(&id) {
        return Err(CoreError::new("unknown entity"));
    }
    session.adopt_world();
    session.notice = Some(session.messages.get("play-running"));
    Ok(())
}

fn select_children(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    let Some(parent) = session.models.selection.clone() else {
        return Err(CoreError::new("nothing selected"));
    };
    let children: Vec<String> = session
        .models
        .nodes
        .iter()
        .filter(|node| node.parent.as_deref() == Some(parent.as_str()))
        .map(|node| node.id.clone())
        .collect();
    if children.is_empty() {
        return Err(CoreError::new("no child nodes"));
    }
    session.foundation.selection = children.clone();
    session.models.selection = children.first().cloned();
    Ok(())
}

fn set_time_scale(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let value: f32 = required(args, "value")?
        .parse()
        .map_err(|_| CoreError::new("time scale must be a number"))?;
    if !value.is_finite() || value < 0.0 {
        return Err(CoreError::new("time scale must be a number"));
    }
    session.world.time_scale = value;
    Ok(())
}

/// Project-relative path for a picture named in the asset browser (`sprite:<name>`).
fn sprite_rel_path(session: &EditorSession, name: &str) -> Result<String, CoreError> {
    let root = session
        .project
        .as_ref()
        .map(|project| project.root.clone())
        .ok_or_else(|| CoreError::new("open a project before placing files"))?;
    let base = session.models.content_root.clone().unwrap_or_else(|| root.clone());
    let full = if Path::new(name).is_absolute() { PathBuf::from(name) } else { base.join(name) };
    let rel = full
        .strip_prefix(&root)
        .map_err(|_| CoreError::new("the file is outside the project"))?
        .to_string_lossy()
        .replace('\\', "/");
    Ok(rel)
}

/// Copy files into the project, each into the folder for its kind.
/// `path` may hold several files, one per line or separated by `;`.
/// A picture also appears in the scene as a sprite object only when it was
/// dropped on the viewport (`a`/`b`) or `place` is `1`. Without a path the host asks.
/// The copy runs on the worker pool and commits one file at a time so Cancel
/// can stop it. Call [`crate::import_pipeline::pump_all`] to finish in tests.
fn import_asset(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let Some(raw) = args.get("path").filter(|p| !p.trim().is_empty()) else {
        session.host_action = Some(crate::session::HostAction::ImportAsset);
        return Ok(());
    };
    crate::import_pipeline::begin_import_from_arg(session, raw, args)
}


fn add_entity(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    editing(session)?;
    let kind = args.get("kind").unwrap_or("empty");
    // A prefab dragged from the Asset Browser places a copy of it.
    if let Some(name) = kind.strip_prefix("prefab:") {
        let rel = sprite_rel_path(session, name)?;
        let mut pairs: Vec<(&str, &str)> = vec![("path", rel.as_str())];
        let (a, b) = (args.get("a").unwrap_or_default(), args.get("b").unwrap_or_default());
        if !a.is_empty() && !b.is_empty() {
            pairs.push(("a", a));
            pairs.push(("b", b));
        }
        if let Some(parent) = args.get("parent").filter(|p| !p.is_empty() && *p != "root") {
            session.models.selection = Some(parent.to_string());
        }
        return add_prefab(session, &CommandArgs::new(&pairs));
    }
    // A logic graph dropped on an object becomes that object's script.
    if let Some(name) = kind.strip_prefix("graph:") {
        let rel = sprite_rel_path(session, name)?;
        let target = args
            .get("parent")
            .filter(|p| !p.is_empty() && *p != "root")
            .map(str::to_string)
            .or_else(|| session.models.selection.clone().filter(|id| id != "root"))
            .ok_or_else(|| CoreError::new(session.messages.get("asset-graph-drop-target")))?;
        let previous = session
            .models
            .set_field(&target, "script", &rel)
            .ok_or_else(|| CoreError::new("unknown mock node"))?;
        session.push_past(Past::Field {
            id: target.clone(),
            field: "script".into(),
            previous,
            next: rel,
        });
        session.mirror_node(&target);
        session.future.clear();
        session.mark_scene();
        return Ok(());
    }
    let sprite = match kind.strip_prefix("sprite:") {
        Some(name) => Some(sprite_rel_path(session, name)?),
        None => None,
    };
    // An effect file makes an object that plays it.
    let effect = match kind.strip_prefix("fx:") {
        Some(name) => Some(sprite_rel_path(session, name)?),
        None => None,
    };
    let previous = session.world.to_text();
    let plain = sprite.is_some() || effect.is_some();
    let id = session.world.add(if plain { "empty" } else { kind });
    if let Some(rel) = sprite {
        let stem = Path::new(&rel).file_stem().map(|s| s.to_string_lossy().to_string());
        let name = session.world.free_name(&stem.unwrap_or_else(|| "Sprite".into()));
        if let Some(entity) = session.world.entity_mut(&id) {
            entity.name = name;
            entity.sprite = rel;
        }
    }
    if let Some(rel) = effect {
        let stem = Path::new(&rel).file_stem().map(|s| s.to_string_lossy().to_string());
        let name = session.world.free_name(&stem.unwrap_or_else(|| "Effect".into()));
        if let Some(entity) = session.world.entity_mut(&id) {
            entity.name = name;
            entity.particles = rel;
        }
    }
    if let Some(parent) = args.get("parent").filter(|value| !value.is_empty()) {
        if let Some(entity) = session.world.entity_mut(&id) {
            entity.parent = Some(parent.to_string());
        }
    }
    if let (Some(a), Some(b)) = (args.get("a"), args.get("b")) {
        if let (Ok(a), Ok(b)) = (a.parse::<f32>(), b.parse::<f32>()) {
            if let Some(entity) = session.world.entity_mut(&id) {
                entity.translation[0] = a;
                entity.translation[1] = b;
            }
        }
    }
    session.adopt_world();
    if let Some(parent) = args.get("parent").filter(|value| !value.is_empty()) {
        if !session.models.expanded.iter().any(|item| item == parent) {
            session.models.expanded.push(parent.to_string());
        }
    }
    // The new object is the only one selected, so the Inspector shows it at once.
    session.foundation.selection = vec![id.clone()];
    // The Undo History names the new object: "Add Wind Area", not "Add Entity".
    let name = session.world.entity(&id).map(|e| e.name.clone()).unwrap_or_default();
    session.models.selection = Some(id);
    let label = session.messages.format("undo-add-named", &[("name", name.as_str())]);
    session.push_past(Past::Scene { previous, next: session.world.to_text(), label });
    session.future.clear();
    session.mark_scene();
    Ok(())
}

fn set_sky(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let value = required(args, "value")?;
    if value.chars().any(|c| c == '\n' || c == '\r') {
        return Err(CoreError::new("the sky cubemap must be one line"));
    }
    let previous = session.world.to_text();
    session.world.sky_cubemap = value.trim().replace('\\', "/");
    push_scene_past(session, previous);
    session.future.clear();
    session.mark_scene();
    Ok(())
}

fn set_music(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let value = required(args, "value")?;
    if value.chars().any(|c| c == '\n' || c == '\r') {
        return Err(CoreError::new("the music file must be one line"));
    }
    let previous = session.world.to_text();
    session.world.music = value;
    push_scene_past(session, previous);
    session.future.clear();
    session.mark_scene();
    Ok(())
}

fn set_glow(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let field = required(args, "field")?;
    let value = required(args, "value")?;
    let previous = session.world.to_text();
    let glow = &mut session.world.glow;
    match field.as_str() {
        "enabled" => glow.enabled = matches!(value.as_str(), "yes" | "1" | "true"),
        "threshold" => {
            let number: f32 =
                value.parse().map_err(|_| CoreError::new("threshold must be a number"))?;
            if !number.is_finite() {
                return Err(CoreError::new("threshold must be a number"));
            }
            glow.threshold = number.clamp(0.0, 4.0);
        }
        "intensity" => {
            let number: f32 =
                value.parse().map_err(|_| CoreError::new("intensity must be a number"))?;
            if !number.is_finite() {
                return Err(CoreError::new("intensity must be a number"));
            }
            glow.intensity = number.clamp(0.0, 8.0);
        }
        "radius" => {
            let number: f32 =
                value.parse().map_err(|_| CoreError::new("radius must be a number"))?;
            if !number.is_finite() {
                return Err(CoreError::new("radius must be a number"));
            }
            glow.radius = number.clamp(0.0, 64.0);
        }
        _ => return Err(CoreError::new("unknown glow setting")),
    }
    push_scene_past(session, previous);
    session.mark_scene();
    Ok(())
}

fn set_join(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let value = required(args, "value")?;
    let on = matches!(value.as_str(), "1" | "true" | "yes" | "on");
    let previous = session.world.to_text();
    session.world.join_on = on;
    push_scene_past(session, previous);
    session.mark_scene();
    Ok(())
}

fn set_gravity(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let value: f32 =
        required(args, "value")?.parse().map_err(|_| CoreError::new("gravity must be a number"))?;
    if !value.is_finite() {
        return Err(CoreError::new("gravity must be a number"));
    }
    let previous = session.world.to_text();
    session.world.gravity = value;
    push_scene_past(session, previous);
    session.mark_scene();
    Ok(())
}

fn note_viewport(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let note = required(args, "text")?;
    session.models.viewport_log.push(note);
    Ok(())
}

fn set_quality_level_cmd(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let level = required(args, "level")?;
    crate::render_quality::apply_level_to_session(session, &level)
}

fn set_platform_override_cmd(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let id = required(args, "id")?;
    let platform = required(args, "platform")?;
    if !crate::platform_overrides::platform_id_valid(&platform) {
        return Err(CoreError::new("unknown platform"));
    }
    let enabled = args.get("enabled").is_some_and(|v| v == "true" || v == "1");
    if enabled {
        let value = args.get("value").unwrap_or_else(|| {
            session
                .models
                .settings
                .iter()
                .find(|row| row.id == id)
                .map(|row| row.value())
                .unwrap_or("")
        });
        if value.is_empty() {
            return Err(CoreError::new("set a value before enabling a platform override"));
        }
        session.models.platform_overrides.set(&id, &platform, value);
    } else {
        session.models.platform_overrides.clear(&id, &platform);
    }
    if let Some(root) = session.project.as_ref().map(|p| p.root.clone()) {
        crate::platform_overrides::store(&root, &session.models.platform_overrides)?;
    }
    session.settings_dirty = true;
    Ok(())
}

fn set_preview_platform_cmd(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let platform = required(args, "platform")?;
    if !crate::platform_overrides::platform_id_valid(&platform) {
        return Err(CoreError::new("unknown platform"));
    }
    session.models.settings_preview_platform = platform;
    Ok(())
}

fn set_setting_cmd(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let id = required(args, "id")?;
    let value = required(args, "value")?;
    if id == crate::render_quality::QUALITY_LEVEL {
        return crate::render_quality::apply_level_to_session(session, &value);
    }
    if matches!(
        id.as_str(),
        crate::inspector_fields::COLLISION_LAYER_SETTING
            | crate::inspector_fields::SORTING_LAYER_SETTING
    ) {
        let stored = crate::inspector_fields::join_layer_names(
            &crate::inspector_fields::split_layer_names(&value),
        );
        let root = session.project.as_ref().map(|project| project.root.clone());
        let store = |root: &std::path::Path| -> Result<(), CoreError> {
            if id == crate::inspector_fields::COLLISION_LAYER_SETTING {
                crate::inspector_fields::store_collision_layer_names(root, &stored)
            } else {
                crate::inspector_fields::store_sorting_layer_names(root, &stored)
            }
        };
        if let Some(root) = root {
            store(&root)?;
            session.foundation.settings.set_project(&id, &stored)?;
            session
                .models
                .set_setting(&id, &stored, "project")
                .ok_or_else(|| CoreError::new("unknown setting"))?;
        } else {
            session.foundation.settings.set_user(&id, &stored)?;
            session
                .models
                .set_setting(&id, &stored, "user")
                .ok_or_else(|| CoreError::new("unknown setting"))?;
        }
        session.settings_dirty = true;
        if crate::project_audio::is_mix_setting(&id) {
            session.audio_apply = true;
        }
        return Ok(());
    }
    if crate::window_game::is_display_field(&id) {
        crate::window_game::validate_field(&id, &value)?;
        if let Some(root) = session.project.as_ref().map(|project| project.root.clone()) {
            crate::window_game::store_field(&root, &id, &value)?;
            session.foundation.settings.set_project(&id, &value)?;
            session
                .models
                .set_setting(&id, &value, "project")
                .ok_or_else(|| CoreError::new("unknown setting"))?;
        } else {
            session.foundation.settings.set_user(&id, &value)?;
            session
                .models
                .set_setting(&id, &value, "user")
                .ok_or_else(|| CoreError::new("unknown setting"))?;
        }
        session.settings_dirty = true;
        return Ok(());
    }
    let layer = args.get("layer").unwrap_or("user");
    let project_audio = [
        crate::project_audio::MASTER_VOLUME,
        crate::project_audio::MUSIC_VOLUME,
        crate::project_audio::EFFECTS_VOLUME,
        crate::project_audio::OUTPUT_SAMPLE_RATE,
    ];
    if project_audio.contains(&id.as_str()) && session.project.is_some() {
        session.foundation.settings.set_project(&id, &value)?;
        session
            .models
            .set_setting(&id, &value, "project")
            .ok_or_else(|| CoreError::new("unknown setting"))?;
        session.settings_dirty = true;
        session.audio_apply = true;
        return Ok(());
    }
    if layer == "project" && session.project.is_some() {
        session.foundation.settings.set_project(&id, &value)?;
        if crate::window_game::is_display_field(&id) {
            if let Some(root) = session.project.as_ref().map(|p| p.root.clone()) {
                crate::window_game::store_field(&root, &id, &value)?;
            }
        } else if crate::render_quality::is_render_field(&id) {
            if let Some(root) = session.project.as_ref().map(|p| p.root.clone()) {
                crate::render_quality::store_field(&root, &id, &value)?;
            }
            crate::render_quality::sync_world_glow(session);
        } else if id == crate::project_dimension::SETTING {
            if let Some(root) = session.project.as_ref().map(|p| p.root.clone()) {
                crate::project_dimension::store(&root, &value)?;
            }
        }
    } else {
        session.foundation.settings.set_user(&id, &value)?;
    }
    session
        .models
        .set_setting(&id, &value, layer)
        .ok_or_else(|| CoreError::new("unknown setting"))?;
    session.settings_dirty = true;
    if id == crate::editor_language::SETTING {
        session.messages = crate::editor_language::messages_for_store(&session.foundation.settings);
        session.foundation.restart_safe = Some(true);
    }
    if crate::render_quality::is_render_field(&id) {
        crate::render_quality::sync_world_glow(session);
    }
    if crate::project_audio::is_mix_setting(&id) {
        session.audio_apply = true;
    }
    if id == crate::editor_tools::WORKER_THREADS {
        crate::editor_tools::note_worker_budget(&session.foundation.settings);
    }
    if id == "layout.locked" {
        session.foundation.layout_locked = value == "true";
    }
    if id.starts_with("log.") {
        let ceiling = session.logging_ceiling();
        seal_log::with_logger(|logger| logger.set_ceiling(ceiling));
        if let Some(text) = session.logging_notice() {
            session.notice = Some(text);
        }
    }
    Ok(())
}

fn reset_setting_cmd(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let id = required(args, "id")?;
    session.foundation.settings.reset(&id)?;
    session.models.reset_setting(&id);
    if id == crate::inspector_fields::COLLISION_LAYER_SETTING {
        if let Some(root) = session.project.as_ref().map(|project| project.root.clone()) {
            crate::inspector_fields::store_collision_layer_names(&root, "")?;
        }
    }
    if id == crate::inspector_fields::SORTING_LAYER_SETTING {
        if let Some(root) = session.project.as_ref().map(|project| project.root.clone()) {
            crate::inspector_fields::store_sorting_layer_names(&root, "")?;
        }
    }
    if crate::window_game::is_display_field(&id) {
        if let Some(root) = session.project.as_ref().map(|project| project.root.clone()) {
            let fallback = crate::window_game::field_default(&id);
            let _ = crate::window_game::store_field(&root, &id, fallback);
        }
    }
    session.settings_dirty = true;
    if crate::project_audio::is_mix_setting(&id) {
        session.audio_apply = true;
    }
    if id == crate::editor_tools::WORKER_THREADS {
        crate::editor_tools::note_worker_budget(&session.foundation.settings);
    }
    Ok(())
}

fn filter_settings(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    session.models.settings_query = args.get("text").unwrap_or("").to_string();
    Ok(())
}

fn focus_next(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.models.focused_panel = (session.models.focused_panel + 1) % PANEL_ORDER.len();
    session.foundation.focused_panel_id = PANEL_ORDER[session.models.focused_panel].to_string();
    Ok(())
}

fn focus_previous(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.models.focused_panel = if session.models.focused_panel == 0 {
        PANEL_ORDER.len() - 1
    } else {
        session.models.focused_panel - 1
    };
    session.foundation.focused_panel_id = PANEL_ORDER[session.models.focused_panel].to_string();
    Ok(())
}

pub(crate) fn restore_scene(session: &mut EditorSession, text: &str) {
    if let Ok(world) = seal_world::World::from_text(text) {
        session.world = world;
        session.adopt_world();
        session.mark_scene();
    }
}

fn write_pose_on(session: &mut EditorSession, id: &str, pose: [f32; 9]) {
    if let Some(entity) = session.world.entity_mut(id) {
        write_pose(entity, pose);
    }
    session.adopt_world();
}

fn required(args: &CommandArgs, key: &str) -> Result<String, CoreError> {
    args.get(key)
        .map(str::to_string)
        .ok_or_else(|| CoreError::new(format!("missing argument '{key}'")))
}

#[derive(Clone, Debug, PartialEq)]
pub enum Past {
    Scratch,
    Rename {
        id: String,
        previous: String,
        next: String,
    },
    Reparent {
        id: String,
        previous: Option<String>,
        next: Option<String>,
    },
    Field {
        id: String,
        field: String,
        previous: String,
        next: String,
    },
    Pose {
        id: String,
        previous: [f32; 9],
        next: [f32; 9],
        /// Menu label such as `Move Cube` or `Rotate 3 Entities`.
        label: String,
    },
    Scene {
        previous: String,
        next: String,
        /// Menu label such as `Add Cube` from the command that made the edit.
        label: String,
    },
    /// An effect file. `previous` and `next` are the whole file. A drag updates `next` only.
    File {
        path: String,
        previous: String,
        next: String,
    },
    ImportBatch {
        label: String,
        snapshots: Vec<(String, Option<String>, Option<String>)>,
    },
    ContentPathChange {
        change: crate::asset_index::ContentPathChange,
    },
    MergeDuplicates {
        change: crate::asset_database_ops::MergeDuplicatesChange,
    },
}

/// Scene-wide undo step with the active command title when available.
pub(crate) fn push_scene_past(session: &mut EditorSession, previous: String) {
    let label = session.undo_step_label_now();
    session.push_past(Past::Scene { previous, next: session.world.to_text(), label });
    session.future.clear();
}

pub(crate) fn encode_history(items: &[Past]) -> Vec<u8> {
    let mut out = Vec::new();
    for item in items {
        match item {
            Past::Scratch => out.push(1),
            Past::Rename { id, previous, next } => {
                out.push(2);
                push_str(&mut out, id);
                push_str(&mut out, previous);
                push_str(&mut out, next);
            }
            Past::Reparent { id, previous, next } => {
                out.push(3);
                push_str(&mut out, id);
                push_opt(&mut out, previous.as_deref());
                push_opt(&mut out, next.as_deref());
            }
            Past::Field { id, field, previous, next } => {
                out.push(4);
                push_str(&mut out, id);
                push_str(&mut out, field);
                push_str(&mut out, previous);
                push_str(&mut out, next);
            }
            Past::Pose { id, previous, next, label } => {
                out.push(5);
                push_str(&mut out, id);
                for value in previous.iter().chain(next.iter()) {
                    out.extend(value.to_le_bytes());
                }
                push_str(&mut out, label);
            }
            Past::Scene { previous, next, label } => {
                out.push(6);
                push_str(&mut out, previous);
                push_str(&mut out, next);
                push_str(&mut out, label);
            }
            Past::File { path, previous, next } => {
                out.push(7);
                push_str(&mut out, path);
                push_str(&mut out, previous);
                push_str(&mut out, next);
            }
            Past::ImportBatch { label, snapshots } => {
                out.push(8);
                push_str(&mut out, label);
                out.extend((snapshots.len() as u32).to_le_bytes());
                for (rel, before, after) in snapshots {
                    push_str(&mut out, rel);
                    push_opt(&mut out, before.as_deref());
                    push_opt(&mut out, after.as_deref());
                }
            }
            Past::MergeDuplicates { change } => {
                out.push(10);
                push_str(&mut out, &change.keeper);
                out.extend((change.removed.len() as u32).to_le_bytes());
                for (path, bytes) in &change.removed {
                    push_str(&mut out, path);
                    out.extend((bytes.len() as u32).to_le_bytes());
                    out.extend(bytes);
                }
                out.extend((change.file_snapshots.len() as u32).to_le_bytes());
                for (path, previous, next) in &change.file_snapshots {
                    push_str(&mut out, path);
                    push_str(&mut out, previous);
                    push_str(&mut out, next);
                }
                push_str(&mut out, &change.index_before);
                push_str(&mut out, &change.index_after);
            }
            Past::ContentPathChange { change } => {
                out.push(9);
                push_str(&mut out, &change.from);
                push_str(&mut out, &change.to);
                out.extend((change.file_snapshots.len() as u32).to_le_bytes());
                for (path, previous, next) in &change.file_snapshots {
                    push_str(&mut out, path);
                    push_str(&mut out, previous);
                    push_str(&mut out, next);
                }
                out.extend((change.sidecar_snapshots.len() as u32).to_le_bytes());
                for (rel, before, after) in &change.sidecar_snapshots {
                    push_str(&mut out, rel);
                    push_opt(&mut out, before.as_deref());
                    push_opt(&mut out, after.as_deref());
                }
                push_opt(&mut out, change.import_book_before.as_deref());
                push_opt(&mut out, change.import_book_after.as_deref());
            }
        }
    }
    out
}

pub(crate) fn decode_history(bytes: &[u8]) -> Vec<Past> {
    let mut items = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let Some(item) = take_past(bytes, &mut offset) else {
            break;
        };
        items.push(item);
    }
    items
}

pub(crate) fn past_weight(item: &Past) -> usize {
    match item {
        Past::Scratch => 1,
        Past::Rename { id, previous, next } => id.len() + previous.len() + next.len(),
        Past::Reparent { id, previous, next } => {
            id.len()
                + previous.as_ref().map(String::len).unwrap_or(0)
                + next.as_ref().map(String::len).unwrap_or(0)
        }
        Past::Field { id, field, previous, next } => {
            id.len() + field.len() + previous.len() + next.len()
        }
        Past::Pose { id, label, .. } => id.len() + label.len() + 72,
        Past::Scene { previous, next, label } => previous.len() + next.len() + label.len(),
        Past::File { path, previous, next } => path.len() + previous.len() + next.len(),
        Past::ImportBatch { label, snapshots } => {
            label.len()
                + snapshots
                    .iter()
                    .map(|(rel, before, after)| {
                        rel.len()
                            + before.as_ref().map(String::len).unwrap_or(0)
                            + after.as_ref().map(String::len).unwrap_or(0)
                    })
                    .sum::<usize>()
        }
        Past::ContentPathChange { change } => {
            change.from.len()
                + change.to.len()
                + change
                    .file_snapshots
                    .iter()
                    .map(|(p, a, b)| p.len() + a.len() + b.len())
                    .sum::<usize>()
        }
        Past::MergeDuplicates { change } => {
            change.keeper.len()
                + change.index_before.len()
                + change.index_after.len()
                + change
                    .removed
                    .iter()
                    .map(|(p, b)| p.len() + b.len())
                    .sum::<usize>()
                + change
                    .file_snapshots
                    .iter()
                    .map(|(p, a, b)| p.len() + a.len() + b.len())
                    .sum::<usize>()
        }
    }
}

fn push_str(out: &mut Vec<u8>, text: &str) {
    let bytes = text.as_bytes();
    out.extend((bytes.len() as u32).to_le_bytes());
    out.extend(bytes);
}

fn push_opt(out: &mut Vec<u8>, text: Option<&str>) {
    match text {
        Some(text) => {
            out.push(1);
            push_str(out, text);
        }
        None => out.push(0),
    }
}

fn take_past(bytes: &[u8], offset: &mut usize) -> Option<Past> {
    let tag = *bytes.get(*offset)?;
    *offset += 1;
    Some(match tag {
        1 => Past::Scratch,
        2 => Past::Rename {
            id: take_str(bytes, offset)?,
            previous: take_str(bytes, offset)?,
            next: take_str(bytes, offset)?,
        },
        3 => Past::Reparent {
            id: take_str(bytes, offset)?,
            previous: take_opt(bytes, offset)?,
            next: take_opt(bytes, offset)?,
        },
        4 => Past::Field {
            id: take_str(bytes, offset)?,
            field: take_str(bytes, offset)?,
            previous: take_str(bytes, offset)?,
            next: take_str(bytes, offset)?,
        },
        5 => {
            let id = take_str(bytes, offset)?;
            let mut pose = [0.0; 18];
            for slot in &mut pose {
                if *offset + 4 > bytes.len() {
                    return None;
                }
                let raw = bytes[*offset..*offset + 4].try_into().ok()?;
                *slot = f32::from_le_bytes(raw);
                *offset += 4;
            }
            let label = if *offset < bytes.len() {
                take_str(bytes, offset).unwrap_or_default()
            } else {
                String::new()
            };
            Past::Pose {
                id,
                previous: pose[..9].try_into().ok()?,
                next: pose[9..].try_into().ok()?,
                label,
            }
        }
        6 => {
            let previous = take_str(bytes, offset)?;
            let next = take_str(bytes, offset)?;
            let label = if *offset < bytes.len() {
                take_str(bytes, offset).unwrap_or_default()
            } else {
                String::new()
            };
            Past::Scene { previous, next, label }
        }
        7 => Past::File {
            path: take_str(bytes, offset)?,
            previous: take_str(bytes, offset)?,
            next: take_str(bytes, offset)?,
        },
        8 => {
            let label = take_str(bytes, offset)?;
            if *offset + 4 > bytes.len() {
                return None;
            }
            let count = u32::from_le_bytes(bytes[*offset..*offset + 4].try_into().ok()?) as usize;
            *offset += 4;
            let mut snapshots = Vec::with_capacity(count);
            for _ in 0..count {
                let rel = take_str(bytes, offset)?;
                let before = take_opt(bytes, offset)?;
                let after = take_opt(bytes, offset)?;
                snapshots.push((rel, before, after));
            }
            Past::ImportBatch { label, snapshots }
        }
        9 => {
            let from = take_str(bytes, offset)?;
            let to = take_str(bytes, offset)?;
            if *offset + 4 > bytes.len() {
                return None;
            }
            let file_count =
                u32::from_le_bytes(bytes[*offset..*offset + 4].try_into().ok()?) as usize;
            *offset += 4;
            let mut file_snapshots = Vec::with_capacity(file_count);
            for _ in 0..file_count {
                let path = take_str(bytes, offset)?;
                let previous = take_str(bytes, offset)?;
                let next = take_str(bytes, offset)?;
                file_snapshots.push((path, previous, next));
            }
            if *offset + 4 > bytes.len() {
                return None;
            }
            let side_count =
                u32::from_le_bytes(bytes[*offset..*offset + 4].try_into().ok()?) as usize;
            *offset += 4;
            let mut sidecar_snapshots = Vec::with_capacity(side_count);
            for _ in 0..side_count {
                let rel = take_str(bytes, offset)?;
                let before = take_opt(bytes, offset)?;
                let after = take_opt(bytes, offset)?;
                sidecar_snapshots.push((rel, before, after));
            }
            let import_book_before = take_opt(bytes, offset)?;
            let import_book_after = take_opt(bytes, offset)?;
            Past::ContentPathChange {
                change: crate::asset_index::ContentPathChange {
                    from,
                    to,
                    file_snapshots,
                    sidecar_snapshots,
                    import_book_before,
                    import_book_after,
                },
            }
        }
        10 => {
            let keeper = take_str(bytes, offset)?;
            if *offset + 4 > bytes.len() {
                return None;
            }
            let removed_count = u32::from_le_bytes(bytes[*offset..*offset + 4].try_into().ok()?) as usize;
            *offset += 4;
            let mut removed = Vec::with_capacity(removed_count);
            for _ in 0..removed_count {
                let path = take_str(bytes, offset)?;
                if *offset + 4 > bytes.len() {
                    return None;
                }
                let len = u32::from_le_bytes(bytes[*offset..*offset + 4].try_into().ok()?) as usize;
                *offset += 4;
                let end = offset.saturating_add(len);
                if end > bytes.len() {
                    return None;
                }
                let payload = bytes[*offset..end].to_vec();
                *offset = end;
                removed.push((path, payload));
            }
            if *offset + 4 > bytes.len() {
                return None;
            }
            let file_count = u32::from_le_bytes(bytes[*offset..*offset + 4].try_into().ok()?) as usize;
            *offset += 4;
            let mut file_snapshots = Vec::with_capacity(file_count);
            for _ in 0..file_count {
                let path = take_str(bytes, offset)?;
                let previous = take_str(bytes, offset)?;
                let next = take_str(bytes, offset)?;
                file_snapshots.push((path, previous, next));
            }
            let index_before = take_str(bytes, offset)?;
            let index_after = take_str(bytes, offset)?;
            Past::MergeDuplicates {
                change: crate::asset_database_ops::MergeDuplicatesChange {
                    keeper,
                    removed,
                    file_snapshots,
                    index_before,
                    index_after,
                },
            }
        }
        _ => return None,
    })
}

fn take_str(bytes: &[u8], offset: &mut usize) -> Option<String> {
    if *offset + 4 > bytes.len() {
        return None;
    }
    let len = u32::from_le_bytes(bytes[*offset..*offset + 4].try_into().ok()?) as usize;
    *offset += 4;
    if *offset + len > bytes.len() {
        return None;
    }
    let text = String::from_utf8(bytes[*offset..*offset + len].to_vec()).ok()?;
    *offset += len;
    Some(text)
}

fn take_opt(bytes: &[u8], offset: &mut usize) -> Option<Option<String>> {
    let flag = *bytes.get(*offset)?;
    *offset += 1;
    match flag {
        0 => Some(None),
        1 => Some(Some(take_str(bytes, offset)?)),
        _ => None,
    }
}

pub fn undo_model(session: &mut EditorSession, past: Past) {
    let touched = match &past {
        Past::Scratch => None,
        Past::Rename { id, .. }
        | Past::Reparent { id, .. }
        | Past::Field { id, .. }
        | Past::Pose { id, .. } => Some(id.clone()),
        Past::Scene { .. }
        | Past::File { .. }
        | Past::ImportBatch { .. }
        | Past::ContentPathChange { .. }
        | Past::MergeDuplicates { .. } => None,
    };
    match &past {
        Past::Scratch => {}
        Past::Rename { id, previous, .. } => {
            session.models.rename(id, previous);
        }
        Past::Reparent { id, previous, .. } => {
            let _ = session.models.reparent(id, previous.as_deref());
        }
        Past::Field { id, field, previous, .. } => {
            session.models.set_field(id, field, previous);
        }
        Past::Pose { id, previous, .. } => write_pose_on(session, id, *previous),
        Past::Scene { previous, .. } => restore_scene(session, previous),
        Past::File { path, previous, .. } => {
            let _ = crate::persist::atomic_write(std::path::Path::new(path), previous.as_bytes());
            let _ = session.clear_draft(crate::fx_curve::FX_FIELD_EDIT);
        }
        Past::ImportBatch { .. } => crate::import_presets::undo_import_batch(session, &past),
        Past::ContentPathChange { .. } => crate::asset_index::undo_content_path_change(session, &past),
        Past::MergeDuplicates { change } => crate::asset_database_ops::undo_merge(session, change),
    }
    session.future.push(past);
    if let Some(id) = touched {
        session.mirror_node(&id);
    }
}

pub fn redo_model(session: &mut EditorSession, past: Past) {
    let touched = match &past {
        Past::Scratch => None,
        Past::Rename { id, .. }
        | Past::Reparent { id, .. }
        | Past::Field { id, .. }
        | Past::Pose { id, .. } => Some(id.clone()),
        Past::Scene { .. }
        | Past::File { .. }
        | Past::ImportBatch { .. }
        | Past::ContentPathChange { .. }
        | Past::MergeDuplicates { .. } => None,
    };
    match &past {
        Past::Scratch => {}
        Past::Rename { id, next, .. } => {
            session.models.rename(id, next);
        }
        Past::Reparent { id, next, .. } => {
            let _ = session.models.reparent(id, next.as_deref());
        }
        Past::Field { id, field, next, .. } => {
            session.models.set_field(id, field, next);
        }
        Past::Pose { id, next, .. } => write_pose_on(session, id, *next),
        Past::Scene { next, .. } => restore_scene(session, next),
        Past::File { path, next, .. } => {
            let _ = crate::persist::atomic_write(std::path::Path::new(path), next.as_bytes());
            let _ = session.clear_draft(crate::fx_curve::FX_FIELD_EDIT);
        }
        Past::ImportBatch { .. } => crate::import_presets::redo_import_batch(session, &past),
        Past::ContentPathChange { .. } => crate::asset_index::redo_content_path_change(session, &past),
        Past::MergeDuplicates { change } => crate::asset_database_ops::redo_merge(session, change),
    }
    session.push_past(past);
    if let Some(id) = touched {
        session.mirror_node(&id);
    }
}

pub fn list_folder(path: &Path, query: &str) -> Vec<String> {
    let models = EditorModels {
        content_root: Some(path.to_path_buf()),
        content_query: query.to_string(),
        ..EditorModels::default()
    };
    models.content_names(false)
}

fn valid_content_segment(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\'])
}

fn resolve_content_path(session: &EditorSession, picked: &str) -> Result<PathBuf, CoreError> {
    if picked.is_empty() || picked == "." || picked == ".." {
        return Err(CoreError::new("content name is not a single item"));
    }
    let root = session
        .models
        .content_root
        .clone()
        .or_else(|| session.models.content_tree_root.clone())
        .ok_or_else(|| CoreError::new("no content folder is open"))?;
    let base = session.models.content_tree_root.as_ref().unwrap_or(&root);
    let path = if picked.contains('/') {
        base.join(picked)
    } else {
        session.models.content_root.as_ref().unwrap_or(base).join(picked)
    };
    Ok(path)
}

/// Move a content item into `.trash`, or delete it when trash is off.
pub fn delete_content(root: &Path, name: &str, use_trash: bool) -> Result<(), CoreError> {
    if !valid_content_segment(name) {
        return Err(CoreError::new("content name is not a single item"));
    }
    delete_content_at(&root.join(name), use_trash)
}

pub fn delete_content_at(path: &Path, use_trash: bool) -> Result<(), CoreError> {
    let name = path
        .file_name()
        .and_then(|part| part.to_str())
        .ok_or_else(|| CoreError::new("content name is not a single item"))?;
    if use_trash {
        let trash = path
            .parent()
            .ok_or_else(|| CoreError::new("no content folder is open"))?
            .join(".trash");
        fs::create_dir_all(&trash)?;
        let dest = unique_trash(&trash, name);
        fs::rename(path, &dest)?;
    } else if path.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn unique_trash(trash: &Path, name: &str) -> std::path::PathBuf {
    let direct = trash.join(name);
    if !direct.exists() {
        return direct;
    }
    for index in 2..1000 {
        let candidate = trash.join(format!("{name}-{index}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    trash.join(format!("{name}-{}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Layout;

    #[test]
    fn outliner_inspector_and_settings_honor_keyboard_commands_and_undo() {
        let mut session = EditorSession::new(Layout::default());
        session
            .dispatch("outliner.rename", &CommandArgs::new(&[("id", "hero"), ("name", "Pilot")]))
            .unwrap();
        assert_eq!(session.models.nodes.iter().find(|n| n.id == "hero").unwrap().name, "Pilot");
        session.dispatch("edit.undo", &CommandArgs::default()).unwrap();
        assert_eq!(
            session.models.nodes.iter().find(|n| n.id == "hero").unwrap().name,
            "Player Start"
        );
        session.dispatch("edit.redo", &CommandArgs::default()).unwrap();
        assert_eq!(session.models.nodes.iter().find(|n| n.id == "hero").unwrap().name, "Pilot");
        session
            .dispatch(
                "inspector.set",
                &CommandArgs::new(&[("id", "hero"), ("field", "role"), ("value", "npc")]),
            )
            .unwrap();
        assert_eq!(
            session.models.nodes.iter().find(|n| n.id == "hero").unwrap().fields[0].1,
            "npc"
        );
        session
            .dispatch("settings.set", &CommandArgs::new(&[("id", "ui.scale"), ("value", "150")]))
            .unwrap();
        assert_eq!(session.models.scale, 1.5);
        assert_eq!(session.models.settings[1].source(), "user");
        session.dispatch("settings.reset", &CommandArgs::new(&[("id", "ui.scale")])).unwrap();
        assert_eq!(session.models.scale, 1.0);
        let before = session.models.focused_panel;
        session.dispatch("view.focus-next", &CommandArgs::default()).unwrap();
        assert_ne!(session.models.focused_panel, before);
        let error = session.dispatch(
            "outliner.reparent",
            &CommandArgs::new(&[("id", "root"), ("parent", "hero")]),
        );
        assert!(error.is_err());
    }

    #[test]
    fn inspector_set_writes_shared_fields_for_many_ids() {
        let mut session = EditorSession::new(Layout::default());
        session
            .dispatch("outliner.select-set", &CommandArgs::new(&[("ids", "hero,crate")]))
            .unwrap();
        session
            .dispatch(
                "inspector.set",
                &CommandArgs::new(&[("ids", "hero,crate"), ("field", "role"), ("value", "prop")]),
            )
            .unwrap();
        let hero = session.models.nodes.iter().find(|node| node.id == "hero").unwrap();
        let crate_node = session.models.nodes.iter().find(|node| node.id == "crate").unwrap();
        assert_eq!(
            hero.fields.iter().find(|(key, _)| key == "role").map(|(_, v)| v.as_str()),
            Some("prop")
        );
        assert_eq!(
            crate_node.fields.iter().find(|(key, _)| key == "role").map(|(_, v)| v.as_str()),
            Some("prop")
        );
    }

    #[test]
    fn viewport_bookmark_stores_camera_and_selection_set() {
        let mut session = EditorSession::new(Layout::default());
        session.models.viewport_pan_x = 12.0;
        session.models.viewport_zoom = 1.5;
        session.set_draft("viewport-yaw", "30").unwrap();
        session
            .dispatch("outliner.select-set", &CommandArgs::new(&[("ids", "hero,crate")]))
            .unwrap();
        session.dispatch("viewport.bookmark_store", &CommandArgs::new(&[("slot", "1")])).unwrap();
        let bookmark = session.models.viewport_bookmarks.get("1").unwrap();
        assert_eq!(bookmark.pan_x, 12.0);
        assert!((bookmark.zoom - 1.5).abs() < f32::EPSILON);
        assert!((bookmark.yaw - 30.0).abs() < f32::EPSILON);
        assert_eq!(bookmark.selection_ids, vec!["hero", "crate"]);
    }

    #[test]
    fn a_rotate_drag_is_one_undo_step() {
        let mut session = EditorSession::new(Layout::default());
        session.dispatch("viewport.tool", &CommandArgs::new(&[("tool", "rotate")])).unwrap();
        session
            .dispatch(
                "viewport.move",
                &CommandArgs::new(&[("id", "crate"), ("a", "15"), ("b", "0"), ("begin", "1")]),
            )
            .unwrap();
        session
            .dispatch(
                "viewport.move",
                &CommandArgs::new(&[("id", "crate"), ("a", "10"), ("b", "0")]),
            )
            .unwrap();
        assert!((session.world.entity("crate").unwrap().rotation[2] - 25.0).abs() < 0.01);
        session.dispatch("edit.undo", &CommandArgs::default()).unwrap();
        assert_eq!(session.world.entity("crate").unwrap().rotation[2], 0.0);
    }

    #[test]
    fn asset_browser_filters_and_viewport_notes_are_logged() {
        let dir = std::env::temp_dir().join(format!("seal-content-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("alpha.txt"), b"a").unwrap();
        fs::write(dir.join("beta.txt"), b"b").unwrap();
        let names = list_folder(&dir, "alp");
        assert_eq!(names, vec!["alpha.txt".to_string()]);
        let mut session = EditorSession::new(Layout::default());
        session
            .dispatch("content.open", &CommandArgs::new(&[("path", dir.to_str().unwrap())]))
            .unwrap();
        session.dispatch("content.query", &CommandArgs::new(&[("query", "beta")])).unwrap();
        assert_eq!(session.models.content_names(false), vec!["beta.txt".to_string()]);
        session.models.content_query.clear();
        fs::write(dir.join(".secret"), b"hidden").unwrap();
        assert!(!session.models.content_names(false).iter().any(|name| name == ".secret"));
        assert!(session.models.content_names(true).iter().any(|name| name == ".secret"));
        session.foundation.settings.set_user("content.use_trash", "true").unwrap();
        session.dispatch("content.delete", &CommandArgs::new(&[("name", "beta.txt")])).unwrap();
        assert!(dir.join(".trash").join("beta.txt").is_file());
        assert!(!dir.join("beta.txt").exists());
        session.dispatch("viewport.note", &CommandArgs::new(&[("text", "key:w")])).unwrap();
        assert_eq!(session.models.viewport_log, vec!["key:w".to_string()]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn content_menu_actions_duplicate_reveal_and_make_scenes() {
        let dir = std::env::temp_dir().join(format!("seal-content-menu-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("art")).unwrap();
        fs::write(dir.join("art").join("a.png"), b"p").unwrap();
        fs::write(dir.join("notes.txt"), b"n").unwrap();
        let mut session = EditorSession::new(Layout::default());
        session
            .dispatch("content.open", &CommandArgs::new(&[("path", dir.to_str().unwrap())]))
            .unwrap();
        let dup = |s: &mut EditorSession, name: &str| {
            s.dispatch("content.duplicate", &CommandArgs::new(&[("name", name)])).unwrap()
        };
        dup(&mut session, "notes.txt");
        dup(&mut session, "notes.txt");
        assert_eq!(fs::read(dir.join("notes copy.txt")).unwrap(), b"n");
        assert!(dir.join("notes copy 2.txt").is_file());
        dup(&mut session, "art");
        assert!(dir.join("art copy").join("a.png").is_file());

        session.dispatch("content.reveal", &CommandArgs::new(&[("name", "notes.txt")])).unwrap();
        assert!(matches!(
            session.host_action.take(),
            Some(crate::session::HostAction::OpenPath(p)) if p == dir
        ));
        session.dispatch("content.reveal", &CommandArgs::new(&[("name", "art")])).unwrap();
        assert!(matches!(
            session.host_action.take(),
            Some(crate::session::HostAction::OpenPath(p)) if p == dir.join("art")
        ));

        session.dispatch("content.new_scene", &CommandArgs::new(&[])).unwrap();
        session.dispatch("content.new_scene", &CommandArgs::new(&[])).unwrap();
        let text = fs::read_to_string(dir.join("New Scene.seal")).unwrap();
        assert!(seal_world::World::from_text(&text).is_ok());
        assert!(dir.join("New Scene 2.seal").is_file());
        let art = dir.join("art");
        session
            .dispatch(
                "content.new_scene",
                &CommandArgs::new(&[("path", art.to_str().unwrap()), ("name", "Level")]),
            )
            .unwrap();
        assert!(art.join("Level.seal").is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn console_keeps_only_the_newest_entries() {
        let models = EditorModels::default();
        let records: Vec<LogLine> = (0..5)
            .map(|index| LogLine {
                level: "info".into(),
                message: index.to_string(),
                source: String::new(),
                fields: Vec::new(),
            })
            .collect();
        let shown = models.filtered_logs(&records, 2);
        assert_eq!(shown.len(), 2);
        assert_eq!(shown[0].message, "3");
        assert_eq!(shown[1].message, "4");
    }

    #[test]
    fn a_large_list_exposes_only_the_visible_window() {
        let range = visible_range(100_000, 50_000, 20);
        assert_eq!(range.len(), 20);
        assert_eq!(range.start, 50_000);
    }

    #[test]
    fn console_clear_hides_old_lines_but_keeps_the_log_file() {
        let mut session = EditorSession::new(Layout::default());
        seal_log::info!("before-clear");
        let count = seal_log::with_logger(|logger| logger.records().len());
        session.dispatch("console.clear", &CommandArgs::default()).unwrap();
        assert!(session.foundation.console_clear_before >= count);
        let models = EditorModels::default();
        let records = collect_log_lines(session.foundation.console_clear_before);
        // The logger is shared with tests running in parallel, so only this
        // test's own lines are checked.
        let hidden = models.filtered_logs(&records, 100);
        assert!(hidden.iter().all(|line| line.message != "before-clear"));
        seal_log::info!("after-clear");
        let records = collect_log_lines(session.foundation.console_clear_before);
        let shown = models.filtered_logs(&records, 100);
        assert!(shown.iter().all(|line| line.message != "before-clear"));
        assert!(shown.iter().any(|line| line.message == "after-clear"));
    }

    #[test]
    fn console_regex_filter_matches_patterns() {
        let models = EditorModels {
            console_regex: true,
            console_query: "err[0-9]".into(),
            ..EditorModels::default()
        };
        let lines = vec![
            LogLine {
                level: "info".into(),
                message: "err1".into(),
                source: String::new(),
                fields: Vec::new(),
            },
            LogLine {
                level: "info".into(),
                message: "fine".into(),
                source: String::new(),
                fields: Vec::new(),
            },
        ];
        let shown = models.filtered_logs(&lines, 100);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].message, "err1");
    }

    #[test]
    fn outliner_reparent_undo_restores_parent() {
        let mut session = EditorSession::new(Layout::default());
        let original = session
            .models
            .nodes
            .iter()
            .find(|node| node.id == "crate")
            .and_then(|node| node.parent.clone());
        assert_ne!(original.as_deref(), Some("hero"));
        session
            .dispatch(
                "outliner.reparent",
                &CommandArgs::new(&[("id", "crate"), ("parent", "hero")]),
            )
            .unwrap();
        let parent = session
            .models
            .nodes
            .iter()
            .find(|node| node.id == "crate")
            .and_then(|node| node.parent.clone());
        assert_eq!(parent.as_deref(), Some("hero"));
        session.dispatch("edit.undo", &CommandArgs::default()).unwrap();
        let parent = session
            .models
            .nodes
            .iter()
            .find(|node| node.id == "crate")
            .and_then(|node| node.parent.clone());
        assert_eq!(parent, original);
        // The scene itself must agree, not only the outliner row.
        assert_eq!(session.world.entity("crate").unwrap().parent, original);
        session.dispatch("edit.redo", &CommandArgs::default()).unwrap();
        assert_eq!(session.world.entity("crate").unwrap().parent.as_deref(), Some("hero"));
    }
}
